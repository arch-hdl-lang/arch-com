//! Elaboration pass: expands `generate for`/`if` blocks and monomorphizes
//! modules that are instantiated with different param combinations.
//!
//! Algorithm
//! ---------
//! 1. Compute default const-param values for every module.
//! 2. Collect raw param overrides from every `inst` block in the file
//!    (including inst blocks nested inside generate items).
//! 3. For each module, derive the set of *distinct effective-param maps*
//!    that appear across all inst sites (defaults + per-site overrides).
//!    If there is only one distinct map, the module keeps its original name.
//!    If there are multiple, every variant is emitted as a separate SV module
//!    named `ModName_PARAM1_VAL1_PARAM2_VAL2` (only params that differ across
//!    variants appear in the suffix; params are sorted alphabetically).
//! 4. Elaborate each variant: expand generate blocks using that variant's
//!    param map, rewrite inner inst module-names to point at the correct
//!    variant of the instantiated module, and rename the module itself.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::*;
use crate::diagnostics::CompileError;
use crate::lexer::Span;

// Param resolution, override application, elaborate-side const-eval, and
// derived-param variant rewriting (P4 phase 2a, move-only extraction). See
// `params.rs` module doc for the exact boundary. `try_eval_i64` stays `pub`
// and is re-exported here so every existing `crate::elaborate::try_eval_i64`
// call site (this file's own bare calls, plus `typecheck.rs`'s `where`-clause
// enforcement) keeps resolving unchanged; the rest are re-imported as plain
// `use` so this file's existing bare call sites are untouched too.
mod params;
pub use params::try_eval_i64;
use params::{
    collect_raw_overrides_from_body, compute_all_variants, compute_defaults_with_enums,
    elaborate_module_variant, try_eval_bool,
};

// Thread → FSM lowering: `partition_thread_body_*`, fork/join and `for`-loop
// lowering, lock/semaphore/arbiter synthesis, shared-reduction lowering, and
// the auto-thread-assert SVA emission (P4 phase 2b, move-only extraction).
// See `threads.rs` module doc for the full inventory.
// `lower_threads`/`lower_threads_with_opts`/`ThreadLowerOpts` stay `pub` and
// are re-exported here so every existing `crate::elaborate::lower_threads`
// call site (`main.rs`, `tests/integration_test.rs`,
// `tests/param_where_constraints.rs`) keeps resolving unchanged.
mod threads;
pub use threads::{lower_threads, lower_threads_with_opts, ThreadLowerOpts};

// TLM (`tlm_method`) lowering: bus-initiator/target `tlm_connect` sugar
// (`lower_tlm_connects`), `thread port.method(...)` target-body lowering
// (`lower_tlm_target_threads`), and initiator-call cohort/request-arbiter/
// response-router/tag-lane synthesis (`lower_tlm_initiator_calls`) (P4
// phase 2c, move-only extraction). See `tlm.rs` module doc for the full
// inventory and the 2b/2c boundary rationale. `lower_tlm_target_threads`
// and `lower_tlm_initiator_calls` stay `pub` and are re-exported here so
// every existing `crate::elaborate::lower_tlm_*` call site (`main.rs`,
// `tests/integration_test.rs`, `tests/param_where_constraints.rs`) keeps
// resolving unchanged; `lower_tlm_connects` has no external caller —
// bumped `fn` → `pub(super) fn` and re-imported as a plain `use` so this
// file's existing bare call site is untouched.
mod tlm;
use tlm::lower_tlm_connects;
pub use tlm::{lower_tlm_initiator_calls, lower_tlm_target_threads};

// Bus/const-eval utilities shared by two lowering passes below —
// `elaborate::threads` (thread-FSM lowering) and `elaborate::tlm` (TLM
// lowering). Not thread- or TLM-specific — kept here rather than in either
// submodule so neither side of the split needs to reach back across a
// module boundary. `thread_stmt_span` (just below `build_module_type_map`)
// joins this cluster as of the phase 2c move: both `elaborate::threads`
// and `elaborate::tlm` call it directly. See `elaborate::threads`/
// `elaborate::tlm` module docs for the full rationale.

/// Collected type info for a signal in the enclosing module.
#[derive(Clone, Debug)]
struct SignalInfo {
    ty: TypeExpr,
    reg_reset: RegReset,
    reg_init: Option<Expr>,
    shared: Option<SharedReduction>,
    /// Carried so the threads-submodule's synthesized port declarations
    /// inherit the parent's `unpacked Vec<T,N>` shape — otherwise the
    /// instantiation in the parent gets a packed-vs-unpacked port
    /// connection mismatch.
    unpacked: bool,
    /// Mirror of `unpacked_ascending` for the same reason — synthesized
    /// sub-module ports must inherit the dimension direction or
    /// SV port-boundary index reversal silently corrupts cross-module
    /// arrays. See arch-com#307.
    unpacked_ascending: bool,
}

/// Wrapper around `build_module_type_map` that ALSO seeds entries for the
/// flattened bus-port signals: `port b: target B` with `B { v: out Bool; }`
/// gets an entry `b_v` → Bool. Lets the thread-lowering pass treat
/// `b.v = true;` inside a thread body the same as a write to a bare flat
/// output port — without this, the synthesized `_<mod>_threads` sub-module
/// fails to expose `b_v` and the parent's driver-completeness check
/// reports it as undriven.
///
/// `bus_defs` is the top-level item index; consulted to resolve each bus
/// port's effective signal list (including the `target` perspective flip).
fn build_module_type_map_with_buses(
    m: &ModuleDecl,
    bus_defs: &HashMap<String, BusDecl>,
) -> HashMap<String, SignalInfo> {
    let mut map = build_module_type_map(m);
    for p in &m.ports {
        let Some(bi) = p.bus_info.as_ref() else {
            continue;
        };
        let Some(bd) = bus_defs.get(&bi.bus_name.name) else {
            continue;
        };
        // Effective signals (with `generate_if READ`/`WRITE` resolved). Use the
        // bus port's own param overrides + bus defaults so width-bearing types
        // like `UInt<DATA_W>` substitute correctly.
        let mut param_map: HashMap<String, &Expr> = bd
            .params
            .iter()
            .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
            .collect();
        for pa in &bi.params {
            param_map.insert(pa.name.name.clone(), &pa.value);
        }
        let eff = bus_effective_signals(bd, &param_map);
        // For Vec-of-bus ports, register entries for each indexed copy.
        let prefixes: Vec<String> = match bi.count.as_ref() {
            None => vec![p.name.name.clone()],
            Some(count_expr) => {
                let n = eval_const_expr_for_lower(count_expr, &m.params) as u32;
                (0..n).map(|i| format!("{}_{}", p.name.name, i)).collect()
            }
        };
        for prefix in &prefixes {
            for (sname, _sdir, sty) in &eff {
                let subst_ty = subst_type_expr_for_lower(sty, &param_map);
                map.entry(format!("{prefix}_{sname}"))
                    .or_insert(SignalInfo {
                        ty: subst_ty,
                        reg_reset: RegReset::None,
                        reg_init: None,
                        shared: None,
                        unpacked: false,
                        unpacked_ascending: false,
                    });
            }
        }
    }
    map
}

/// Minimal `effective_signals` walker for `BusDecl`. Inlines bus-level
/// `generate_if` gates by folding their condition against the param map;
/// signals inside a falsy branch are dropped. Mirrors the resolve-pass
/// `BusInfo::effective_signals` but runs pre-resolve (lower_threads is
/// invoked before `resolve::resolve`).
fn bus_effective_signals(
    bd: &BusDecl,
    param_map: &HashMap<String, &Expr>,
) -> Vec<(String, Direction, TypeExpr)> {
    let mut out: Vec<(String, Direction, TypeExpr)> = bd
        .signals
        .iter()
        .map(|s| (s.name.name.clone(), s.direction, s.ty.clone()))
        .collect();
    for gi in &bd.generates {
        let cond_v = eval_const_expr_for_lower(&gi.cond, &[]);
        // Resolve any param references in the cond by substituting from
        // param_map; fall back to the bare const eval otherwise.
        let cond = if cond_v != 0 {
            true
        } else {
            param_map.get(&format!("{:?}", gi.cond.kind)).is_some()
        };
        // Simpler: re-evaluate cond by walking param_map for Ident matches.
        let cond = cond || gen_if_cond_truthy(&gi.cond, param_map);
        let branch = if cond {
            &gi.then_signals
        } else {
            &gi.else_signals
        };
        for s in branch {
            out.push((s.name.name.clone(), s.direction, s.ty.clone()));
        }
    }
    for method in tlm_effective_methods_for_bus(bd, param_map) {
        out.extend(tlm_method_effective_signals(&method));
    }
    out
}

fn tlm_effective_methods_for_bus(
    bd: &BusDecl,
    param_map: &HashMap<String, &Expr>,
) -> Vec<TlmMethodMeta> {
    let mut methods = bd.tlm_methods.clone();
    for gi in &bd.generates {
        let cond_v = eval_const_expr_for_lower(&gi.cond, &[]);
        let cond = if cond_v != 0 {
            true
        } else {
            param_map.get(&format!("{:?}", gi.cond.kind)).is_some()
        };
        let cond = cond || gen_if_cond_truthy(&gi.cond, param_map);
        let branch = if cond {
            &gi.then_tlm_methods
        } else {
            &gi.else_tlm_methods
        };
        methods.extend(branch.clone());
    }
    methods
}

fn tlm_method_effective_signals(method: &TlmMethodMeta) -> Vec<(String, Direction, TypeExpr)> {
    let name = &method.name.name;
    let bool_ty = TypeExpr::Bool;
    let mut out = vec![(format!("{name}_req_valid"), Direction::Out, bool_ty.clone())];
    if let Some(tag_w) = &method.out_of_order_tags {
        out.push((
            format!("{name}_req_tag"),
            Direction::Out,
            TypeExpr::UInt(Box::new(tag_w.clone())),
        ));
    }
    for (arg_name, arg_ty) in &method.args {
        out.push((
            format!("{name}_{}", arg_name.name),
            Direction::Out,
            arg_ty.clone(),
        ));
    }
    out.push((format!("{name}_req_ready"), Direction::In, bool_ty.clone()));
    out.push((format!("{name}_rsp_valid"), Direction::In, bool_ty.clone()));
    if let Some(tag_w) = &method.out_of_order_tags {
        out.push((
            format!("{name}_rsp_tag"),
            Direction::In,
            TypeExpr::UInt(Box::new(tag_w.clone())),
        ));
    }
    if let Some(ret_ty) = &method.ret {
        out.push((format!("{name}_rsp_data"), Direction::In, ret_ty.clone()));
    }
    out.push((format!("{name}_rsp_ready"), Direction::Out, bool_ty));
    out
}

fn gen_if_cond_truthy(e: &Expr, params: &HashMap<String, &Expr>) -> bool {
    match &e.kind {
        ExprKind::Literal(LitKind::Dec(n))
        | ExprKind::Literal(LitKind::Hex(n))
        | ExprKind::Literal(LitKind::Bin(n))
        | ExprKind::Literal(LitKind::Sized(_, n)) => *n != 0,
        ExprKind::Bool(b) => *b,
        ExprKind::Ident(name) => params
            .get(name)
            .map_or(false, |v| gen_if_cond_truthy(v, params)),
        _ => false,
    }
}

fn eval_const_expr_from_param_map_for_lower(
    expr: &Expr,
    params: &HashMap<String, &Expr>,
) -> Option<u64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(n))
        | ExprKind::Literal(LitKind::Hex(n))
        | ExprKind::Literal(LitKind::Bin(n))
        | ExprKind::Literal(LitKind::Sized(_, n)) => Some(*n),
        ExprKind::Bool(b) => Some(if *b { 1 } else { 0 }),
        ExprKind::Ident(name) => params
            .get(name)
            .and_then(|v| eval_const_expr_from_param_map_for_lower(v, params)),
        ExprKind::Unary(op, expr) => {
            let v = eval_const_expr_from_param_map_for_lower(expr, params)?;
            match op {
                UnaryOp::Neg => Some((0u64).wrapping_sub(v)),
                UnaryOp::Not => Some(if v == 0 { 1 } else { 0 }),
                UnaryOp::BitNot => Some(!v),
                _ => None,
            }
        }
        ExprKind::Binary(op, lhs, rhs) => {
            let a = eval_const_expr_from_param_map_for_lower(lhs, params)?;
            let b = eval_const_expr_from_param_map_for_lower(rhs, params)?;
            match op {
                BinOp::Add => Some(a.wrapping_add(b)),
                BinOp::Sub => Some(a.wrapping_sub(b)),
                BinOp::Mul => Some(a.wrapping_mul(b)),
                BinOp::Div => (b != 0).then_some(a / b),
                BinOp::Mod => (b != 0).then_some(a % b),
                BinOp::Shl => Some(a.wrapping_shl(b as u32)),
                BinOp::Shr => Some(a.wrapping_shr(b as u32)),
                BinOp::BitAnd => Some(a & b),
                BinOp::BitOr => Some(a | b),
                BinOp::BitXor => Some(a ^ b),
                BinOp::Eq => Some((a == b) as u64),
                BinOp::Neq => Some((a != b) as u64),
                BinOp::Lt => Some((a < b) as u64),
                BinOp::Lte => Some((a <= b) as u64),
                BinOp::Gt => Some((a > b) as u64),
                BinOp::Gte => Some((a >= b) as u64),
                BinOp::And => Some((a != 0 && b != 0) as u64),
                BinOp::Or => Some((a != 0 || b != 0) as u64),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Param-aware constant folder for the pre-resolve thread-lowering pass.
/// A trimmed copy of the sim_codegen variant — handles literals, plain
/// param-ident lookups, and the small arithmetic subset that surfaces in
/// `Vec<Bus, N>` counts and bus param expressions.
fn eval_const_expr_for_lower(expr: &Expr, params: &[ParamDecl]) -> u64 {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(n))
        | ExprKind::Literal(LitKind::Hex(n))
        | ExprKind::Literal(LitKind::Bin(n))
        | ExprKind::Literal(LitKind::Sized(_, n)) => *n,
        ExprKind::Ident(name) => params
            .iter()
            .find(|p| p.name.name == *name)
            .and_then(|p| p.default.as_ref())
            .map(|d| eval_const_expr_for_lower(d, params))
            .unwrap_or(0),
        ExprKind::Binary(op, l, r) => {
            let lv = eval_const_expr_for_lower(l, params);
            let rv = eval_const_expr_for_lower(r, params);
            match op {
                BinOp::Add => lv.wrapping_add(rv),
                BinOp::Sub => lv.wrapping_sub(rv),
                BinOp::Mul => lv.wrapping_mul(rv),
                BinOp::Div if rv != 0 => lv / rv,
                BinOp::Mod if rv != 0 => lv % rv,
                BinOp::Shl => lv << (rv & 63),
                BinOp::Shr => lv >> (rv & 63),
                _ => 0,
            }
        }
        _ => 0,
    }
}

/// Substitute param-ident references in a type expression, walking
/// Vec<T,N> recursively and folding width-bearing UInt/SInt N expressions.
fn subst_type_expr_for_lower(ty: &TypeExpr, params: &HashMap<String, &Expr>) -> TypeExpr {
    fn subst(e: &Expr, params: &HashMap<String, &Expr>) -> Expr {
        let kind = match &e.kind {
            ExprKind::Ident(name) => {
                if let Some(v) = params.get(name) {
                    return (*v).clone();
                }
                ExprKind::Ident(name.clone())
            }
            // Recurse into arithmetic shapes so widths like `UInt<DATA_W / 8>`
            // and `UInt<DATA_W * 2>` substitute every operand. Without this
            // the downstream type_map ends up with an unresolved param ident
            // and the synthesized `_<mod>_threads` sub-module emits SV ports
            // referencing the bus's local param name (DATA_W) instead of the
            // enclosing module's (DATA_WIDTH).
            ExprKind::Binary(op, l, r) => {
                ExprKind::Binary(*op, Box::new(subst(l, params)), Box::new(subst(r, params)))
            }
            ExprKind::Unary(op, x) => ExprKind::Unary(*op, Box::new(subst(x, params))),
            ExprKind::Ternary(c, t, f) => ExprKind::Ternary(
                Box::new(subst(c, params)),
                Box::new(subst(t, params)),
                Box::new(subst(f, params)),
            ),
            ExprKind::Clog2(x) => ExprKind::Clog2(Box::new(subst(x, params))),
            _ => return e.clone(),
        };
        Expr {
            kind,
            span: e.span,
            parenthesized: e.parenthesized,
        }
    }
    match ty {
        TypeExpr::UInt(w) => TypeExpr::UInt(Box::new(subst(w, params))),
        TypeExpr::SInt(w) => TypeExpr::SInt(Box::new(subst(w, params))),
        TypeExpr::Vec(elem, n) => TypeExpr::Vec(
            Box::new(subst_type_expr_for_lower(elem, params)),
            Box::new(subst(n, params)),
        ),
        _ => ty.clone(),
    }
}

fn build_module_type_map(m: &ModuleDecl) -> HashMap<String, SignalInfo> {
    let mut map = HashMap::new();
    for p in &m.ports {
        map.insert(
            p.name.name.clone(),
            SignalInfo {
                ty: p.ty.clone(),
                reg_reset: p
                    .reg_info
                    .as_ref()
                    .map(|ri| ri.reset.clone())
                    .unwrap_or(RegReset::None),
                reg_init: p.reg_info.as_ref().and_then(|ri| ri.init.clone()),
                shared: p.shared,
                unpacked: p.unpacked,
                unpacked_ascending: p.unpacked_ascending,
            },
        );
    }
    for item in &m.body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                map.insert(
                    r.name.name.clone(),
                    SignalInfo {
                        ty: r.ty.clone(),
                        reg_reset: r.reset.clone(),
                        reg_init: r.init.clone(),
                        shared: None,
                        unpacked: false,
                        unpacked_ascending: false,
                    },
                );
            }
            ModuleBodyItem::WireDecl(w) => {
                map.insert(
                    w.name.name.clone(),
                    SignalInfo {
                        ty: w.ty.clone(),
                        reg_reset: RegReset::None,
                        reg_init: None,
                        shared: None,
                        unpacked: false,
                        unpacked_ascending: false,
                    },
                );
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(ty) = &l.ty {
                    map.insert(
                        l.name.name.clone(),
                        SignalInfo {
                            ty: ty.clone(),
                            reg_reset: RegReset::None,
                            reg_init: None,
                            shared: None,
                            unpacked: false,
                            unpacked_ascending: false,
                        },
                    );
                }
            }
            _ => {}
        }
    }
    map
}

/// Span of a thread statement, for diagnostics. Shared by `elaborate::threads`
/// (`disallow_nested_control_in_do_until`) and `elaborate::tlm` (fork/join
/// issue-collection and TLM-target return-state diagnostics) — relocated here
/// from its old position (originally mid-file inside the TLM lowering region)
/// in the P4 phase 2c move, joining the rest of the bus/const-eval cluster
/// above rather than being duplicated across, or bounced between, both
/// submodules.
fn thread_stmt_span(stmt: &ThreadStmt) -> Span {
    match stmt {
        ThreadStmt::SeqAssign(a) | ThreadStmt::CombAssign(a) | ThreadStmt::ForkTlmAssign(a) => {
            a.span
        }
        ThreadStmt::WaitUntil(_, sp)
        | ThreadStmt::WaitCycles(_, sp)
        | ThreadStmt::ForkJoin(_, sp)
        | ThreadStmt::JoinAll(sp)
        | ThreadStmt::Return(_, sp) => *sp,
        ThreadStmt::IfElse(ie) => ie.span,
        ThreadStmt::For { span, .. }
        | ThreadStmt::Lock { span, .. }
        | ThreadStmt::DoUntil { span, .. } => *span,
        ThreadStmt::Log(l) => l.span,
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn elaborate(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    // Substitute module-scope `type` aliases before any other pass sees the
    // AST. Aliases are pure substitution — once resolved, downstream passes
    // (typecheck / elaborate / codegen / sim) treat the AST as if the user
    // had inlined the aliased types by hand.
    let mut ast = crate::type_alias::resolve_type_aliases(ast)?;

    // Context-typed float literals (arch#622) + the BF16 constant-fold fixes
    // (arch#620/#623/#624): a bare float literal sitting in any known-BF16
    // slot (typed `let`, `reg`/`port reg` `init` AND `reset`, comparisons/
    // arithmetic against a known-BF16 operand, port defaults) is rounded
    // directly to bf16 at compile time (single RNE step, decimal -> f64 ->
    // bf16) and rewritten to a self-contained `TypedFloat` literal. Must run
    // before typecheck (the rewritten literal is what gets type-checked).
    //
    // The `reset` slot originally used an eval-based `(lit).to_bf16()`
    // rewrite instead (#623), which routed through an FP32 intermediate
    // (decimal -> f64 -> f32 -> bf16, a double rounding). It was unified
    // onto this single-rounding fold path by maintainer decision on
    // #622/#624 — an AUTHORIZED behavior change: the two paths diverge for
    // decimals that land within half an f32-ulp of a bf16 rounding midpoint
    // (witness: `1.003906250931322574615478515625` = 1 + 2^-8 + 2^-30 gives
    // 0x3F80 via the f32 route but correctly rounds to 0x3F81 — locked in
    // fp_lit's `double_rounding_via_f32_diverges_on_witness` test). The
    // fold's single-step result is the correctly-rounded one.
    let float_lit_errors = coerce_typed_float_literals(&mut ast);
    if !float_lit_errors.is_empty() {
        return Err(float_lit_errors);
    }

    // Build enum variant → value map for resolving enum-typed params
    let enum_values: HashMap<String, Vec<(String, u64)>> = ast
        .items
        .iter()
        .filter_map(|item| {
            let e = match item {
                Item::Enum(e) => Some(e),
                Item::Package(p) => p.enums.first(), // simplification: first enum in pkg
                _ => None,
            }?;
            let entries: Vec<(String, u64)> = e
                .variants
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let val = e
                        .values
                        .get(i)
                        .and_then(|opt| opt.as_ref())
                        .and_then(|expr| match &expr.kind {
                            ExprKind::Literal(LitKind::Dec(n)) => Some(*n),
                            ExprKind::Literal(LitKind::Hex(n)) => Some(*n),
                            ExprKind::Literal(LitKind::Bin(n)) => Some(*n),
                            ExprKind::Literal(LitKind::Sized(_, n)) => Some(*n),
                            _ => None,
                        })
                        .unwrap_or(i as u64);
                    (v.name.clone(), val)
                })
                .collect();
            Some((e.name.name.clone(), entries))
        })
        .collect();

    // Step 1 — default params (resolve enum variant defaults to integers first)
    let module_defaults: HashMap<String, HashMap<String, i64>> = ast
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Module(m) = item {
                Some((
                    m.name.name.clone(),
                    compute_defaults_with_enums(&m.params, &enum_values),
                ))
            } else {
                None
            }
        })
        .collect();

    // Step 2 + 3 — discover all instantiation variants, transitively.
    //
    // A variant of module M is one distinct (effective param) set M is ever
    // instantiated with. Discovery is a FIXPOINT over the instantiation graph:
    // an override of a *base* param on M (`param W = 5`) can flow through M's
    // *derived* params (`param PW = W + 2`) into the param expressions of M's
    // own inner insts, producing inner-module variants that don't exist under
    // M's default params. Walking each module body only once with its DEFAULT
    // params (the old behavior) misses those — the inner inst then rewrites to a
    // variant name that was never emitted ("undefined module" at SV/sim build).
    //
    // So we iterate: collect raw overrides using each *currently known* variant
    // of the enclosing module as the enclosing param context, recompute
    // variants, and repeat until no new variant appears. Modules form a DAG
    // (no recursive instantiation in ARCH), so this terminates.
    let mut module_variants = compute_all_variants(&ast.items, &module_defaults, &HashMap::new());
    loop {
        let mut inst_raw: HashMap<String, Vec<HashMap<String, i64>>> = HashMap::new();
        for item in &ast.items {
            if let Item::Module(m) = item {
                // Re-walk this module's body once per known variant, using that
                // variant's effective params as the enclosing context so inner
                // inst param expressions (which may reference derived params)
                // resolve against the overridden values.
                let enclosing_sets: Vec<HashMap<String, i64>> = module_variants
                    .get(&m.name.name)
                    .map(|vs| vs.iter().map(|(p, _)| p.clone()).collect())
                    .unwrap_or_else(|| {
                        vec![module_defaults
                            .get(&m.name.name)
                            .cloned()
                            .unwrap_or_default()]
                    });
                for enclosing in &enclosing_sets {
                    collect_raw_overrides_from_body(&m.body, &mut inst_raw, enclosing);
                }
            }
        }
        let next = compute_all_variants(&ast.items, &module_defaults, &inst_raw);
        if next == module_variants {
            module_variants = next;
            break;
        }
        module_variants = next;
    }

    // Child-module port info: needed by expand_generate_for to detect
    // Vec-of-bus child ports that disqualify the SV-genvar preservation.
    // Built once for the whole file (mirrors the lower_tlm_connects map).
    let child_module_ports: HashMap<String, Vec<PortDecl>> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Module(m) => Some((m.name.name.clone(), m.ports.clone())),
            Item::Fsm(f) => Some((f.name.name.clone(), f.ports.clone())),
            Item::Pipeline(p) => Some((p.name.name.clone(), p.ports.clone())),
            _ => None,
        })
        .collect();

    // Step 4 — elaborate and emit
    let mut new_items: Vec<Item> = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();

    for item in ast.items {
        match item {
            Item::Module(m) => {
                let variants = module_variants
                    .get(&m.name.name)
                    .cloned()
                    .unwrap_or_else(|| {
                        let d = module_defaults
                            .get(&m.name.name)
                            .cloned()
                            .unwrap_or_default();
                        vec![(d, m.name.name.clone())]
                    });
                for (param_vals, variant_name) in variants {
                    match elaborate_module_variant(
                        m.clone(),
                        param_vals,
                        variant_name,
                        &module_variants,
                        &module_defaults,
                        &child_module_ports,
                    ) {
                        Ok(elaborated) => new_items.push(Item::Module(elaborated)),
                        Err(mut errs) => errors.append(&mut errs),
                    }
                }
            }
            Item::Pipeline(p) => new_items.push(Item::Pipeline(p)),
            Item::Package(p) => new_items.push(Item::Package(p)),
            Item::Use(u) => new_items.push(Item::Use(u)),
            other => new_items.push(other),
        }
    }

    if !errors.is_empty() {
        Err(errors)
    } else {
        let mut sf = SourceFile {
            items: new_items,
            inner_doc: None,
            frontmatter: None,
        };
        normalize_count1_portarray_conns(&mut sf);
        lower_tlm_connects(sf)
    }
}

/// Pre-typecheck normalization implementing arch#622 (context-typed float
/// literals) and arch#624 (BF16 `init` constant folding): a bare float
/// literal sitting in a slot whose expected type is a **known BF16-typed**
/// context — a typed `let` initializer, a `reg`/`port reg` `init` value, a
/// `reg`/`port reg` `reset` value, a `PortDecl` default, or one side of a
/// comparison/arithmetic `Binary` op against an operand of known BF16 type —
/// is rewritten in place to `LitKind::TypedFloat(FloatLitFmt::Bf16, bits)`,
/// where `bits` is the literal's decimal value rounded *directly* (single
/// step, RNE) to bf16 via [`crate::fp_lit::f64_to_bf16_bits`]. Downstream
/// (typecheck, both codegen backends) treats a `TypedFloat` literal as
/// already being its target type, emitting the exact rounded bit pattern with
/// no runtime conversion call — which is what makes this safe to use for
/// `init` (arch#624): the native sim's C++ constructor member-init list needs
/// a foldable constant, and a `TypedFloat` literal already is one (unlike
/// `(lit).to_bf16()`, which requires an eval call the constructor list can't
/// perform).
///
/// **Reset unification (supersedes the #623 rewrite).** The `reset` slot
/// originally went through a separate eval-based `(lit).to_bf16()` rewrite
/// (#620/#623), which routed the constant through an FP32 intermediate
/// (decimal -> f64 -> f32 -> bf16 — a double rounding). By maintainer
/// decision on #622/#624 it now folds through this same single-rounding path.
/// This is a deliberate behavior change for pathological decimals that land
/// within half an f32-ulp of a bf16 rounding midpoint: e.g.
/// `1.003906250931322574615478515625` (= 1 + 2^-8 + 2^-30) produced 0x3F80
/// via the old f32 route but correctly rounds to 0x3F81. The fold's result is
/// the correctly-rounded one; every ordinary literal (1.5, 0.5, pi, ...) is
/// bit-identical under both paths.
///
/// FP32 is not a target of this pass: a standalone/ambiguous float literal
/// already defaults to FP32 (`typecheck`'s `LitKind::Float => Ty::FP32`), so
/// an FP32-typed slot with a bare float literal already type-checks and
/// already emits the correctly-rounded 32-bit constant — there is nothing to
/// fix. (The helper and the `TypedFloat` representation are format-generic so
/// a future narrower format, e.g. FP16/FP8, is a small follow-up: extend the
/// ident-type map + slot list below.)
///
/// Integer literals are never rewritten by this pass (only
/// `LitKind::Float`), so `let h: BF16 = 1;` / `reg r: BF16 init 1;` remain
/// exactly as type-mismatched as before — `typecheck` rejects them (a
/// `TypedFloat`-typed slot expects `Ty::BF16`; an integer literal's inferred
/// type is `Ty::UInt(_)`, a mismatch).
/// Narrow-float format a declared type coerces bare float literals into.
/// FP32 is absent on purpose: bare literals already default to FP32.
fn narrow_fmt_of_type(ty: &TypeExpr) -> Option<FloatLitFmt> {
    match ty {
        TypeExpr::BF16 => Some(FloatLitFmt::Bf16),
        TypeExpr::FP8E4M3 => Some(FloatLitFmt::E4m3),
        TypeExpr::FP8E5M2 => Some(FloatLitFmt::E5m2),
        // Vec-of-narrow-float: a literal in this signal's reset/init slot
        // (or an element assignment) targets the ELEMENT format — without
        // this, `reg h: Vec<BF16,2> reset rst => 0.5;` kept the literal as
        // FP32 bits and reset every element to a truncated wrong pattern.
        TypeExpr::Vec(elem, _) => narrow_fmt_of_type(elem),
        _ => None,
    }
}

fn coerce_typed_float_literals(ast: &mut SourceFile) -> Vec<CompileError> {
    let mut errors: Vec<CompileError> = Vec::new();
    // Struct name -> narrow-float fields, cloned up front so the mutable
    // module walk below doesn't conflict with the item borrow.
    let struct_items: Vec<(String, Vec<(String, FloatLitFmt)>)> = ast
        .items
        .iter()
        .filter_map(|it| {
            let Item::Struct(sd) = it else { return None };
            let fields: Vec<(String, FloatLitFmt)> = sd
                .fields
                .iter()
                .filter_map(|f| narrow_fmt_of_type(&f.ty).map(|fmt| (f.name.name.clone(), fmt)))
                .collect();
            if fields.is_empty() {
                None
            } else {
                Some((sd.name.name.clone(), fields))
            }
        })
        .collect();
    for item in &mut ast.items {
        let Item::Module(m) = item else { continue };

        // Local (module-scope only — good enough for direct comparisons
        // against a same-module signal, which is the common case the #622
        // issue calls out) ident -> narrow float format map, built from
        // ports, `reg`/`port reg` decls, typed `let` bindings, and `wire`
        // decls.
        let mut narrow_idents: HashMap<String, FloatLitFmt> = HashMap::new();
        // Struct-typed signals contribute compound "signal.field" keys for
        // each narrow-float field, so `s.lo + 0.5` coerces like a scalar.
        let field_fmts = |ty: &TypeExpr| -> Vec<(String, FloatLitFmt)> {
            let TypeExpr::Named(sn) = ty else {
                return Vec::new();
            };
            for it in &struct_items {
                if it.0 == sn.name {
                    return it.1.clone();
                }
            }
            Vec::new()
        };
        for p in &m.ports {
            if let Some(fmt) = narrow_fmt_of_type(&p.ty) {
                narrow_idents.insert(p.name.name.clone(), fmt);
            }
            for (f, fmt) in field_fmts(&p.ty) {
                narrow_idents.insert(format!("{}.{}", p.name.name, f), fmt);
            }
        }
        // Float-typed params: a bare literal default on a narrow-float
        // param is context-typed to the declared format (same rule as
        // typed `let`), and the param name coerces literals in
        // expressions it appears in (`h + HBIAS` etc.).
        for prm in &mut m.params {
            if let crate::ast::ParamKind::Logic(ty) = &prm.kind {
                if let Some(fmt) = narrow_fmt_of_type(ty) {
                    narrow_idents.insert(prm.name.name.clone(), fmt);
                    if let Some(d) = &mut prm.default {
                        coerce_narrow_lit(d, fmt, &mut errors);
                    }
                }
            }
        }
        for bi in &m.body {
            match bi {
                ModuleBodyItem::RegDecl(r) => {
                    if let Some(fmt) = narrow_fmt_of_type(&r.ty) {
                        narrow_idents.insert(r.name.name.clone(), fmt);
                    }
                    for (f, fmt) in field_fmts(&r.ty) {
                        narrow_idents.insert(format!("{}.{}", r.name.name, f), fmt);
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(fmt) = l.ty.as_ref().and_then(narrow_fmt_of_type) {
                        narrow_idents.insert(l.name.name.clone(), fmt);
                    }
                    if let Some(t) = &l.ty {
                        for (f, fmt) in field_fmts(t) {
                            narrow_idents.insert(format!("{}.{}", l.name.name, f), fmt);
                        }
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    if let Some(fmt) = narrow_fmt_of_type(&w.ty) {
                        narrow_idents.insert(w.name.name.clone(), fmt);
                    }
                    for (f, fmt) in field_fmts(&w.ty) {
                        narrow_idents.insert(format!("{}.{}", w.name.name, f), fmt);
                    }
                }
                _ => {}
            }
        }

        // `port reg` outputs: `init`, `reset`, and `default` slots.
        for p in &mut m.ports {
            if let Some(fmt) = narrow_fmt_of_type(&p.ty) {
                if let Some(ri) = &mut p.reg_info {
                    if let Some(init) = &mut ri.init {
                        coerce_narrow_lit(init, fmt, &mut errors);
                    }
                    coerce_narrow_reset(&mut ri.reset, fmt, &mut errors);
                }
                if let Some(default) = &mut p.default {
                    coerce_narrow_lit(default, fmt, &mut errors);
                }
            }
        }

        for bi in &mut m.body {
            match bi {
                ModuleBodyItem::RegDecl(r) => {
                    if let Some(fmt) = narrow_fmt_of_type(&r.ty) {
                        if let Some(init) = &mut r.init {
                            coerce_narrow_lit(init, fmt, &mut errors);
                        }
                        coerce_narrow_reset(&mut r.reset, fmt, &mut errors);
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(fmt) = l.ty.as_ref().and_then(narrow_fmt_of_type) {
                        coerce_narrow_lit(&mut l.value, fmt, &mut errors);
                    }
                }
                ModuleBodyItem::CombBlock(cb) => {
                    for s in &mut cb.stmts {
                        coerce_narrow_lits_in_stmt(s, &narrow_idents, &mut errors);
                    }
                }
                ModuleBodyItem::RegBlock(rb) => {
                    for s in &mut rb.stmts {
                        coerce_narrow_lits_in_stmt(s, &narrow_idents, &mut errors);
                    }
                }
                ModuleBodyItem::LatchBlock(lb) => {
                    for s in &mut lb.stmts {
                        coerce_narrow_lits_in_stmt(s, &narrow_idents, &mut errors);
                    }
                }
                ModuleBodyItem::Assert(a) => {
                    coerce_narrow_lits_in_expr(&mut a.expr, &narrow_idents, &mut errors);
                }
                _ => {}
            }
        }
    }
    errors
}

/// Apply [`coerce_narrow_lit`] to the value expression of a reset clause.
fn coerce_narrow_reset(reset: &mut RegReset, fmt: FloatLitFmt, errors: &mut Vec<CompileError>) {
    match reset {
        RegReset::Explicit(_, _, _, v) | RegReset::Inherit(_, v) => {
            coerce_narrow_lit(v, fmt, errors)
        }
        RegReset::None => {}
    }
}

/// If `e` is a bare float literal, replace it in place with the compile-time
/// rounded `LitKind::TypedFloat` for `fmt`. Non-literal expressions are
/// untouched. An fp8 literal whose rounded magnitude overflows the format's
/// largest finite is a compile error (runtime overflow behavior depends on
/// `--fp-compat`, so a source constant must not fold profile-dependently).
fn coerce_narrow_lit(e: &mut Expr, fmt: FloatLitFmt, errors: &mut Vec<CompileError>) {
    if let ExprKind::Literal(LitKind::Float(bits)) = &e.kind {
        let v = f64::from_bits(*bits);
        let rounded: Option<u64> = match fmt {
            FloatLitFmt::Bf16 => Some(crate::fp_lit::f64_to_bf16_bits(v) as u64),
            FloatLitFmt::Fp32 => Some(crate::fp_lit::f64_to_fp32_bits(v) as u64),
            FloatLitFmt::E4m3 => crate::fp_lit::f64_to_e4m3_bits(v).map(|b| b as u64),
            FloatLitFmt::E5m2 => crate::fp_lit::f64_to_e5m2_bits(v).map(|b| b as u64),
        };
        match rounded {
            Some(bits8) => {
                e.kind = ExprKind::Literal(LitKind::TypedFloat(fmt, bits8));
            }
            None => {
                let (name, max) = match fmt {
                    FloatLitFmt::E4m3 => ("FP8E4M3", 448.0),
                    _ => ("FP8E5M2", 57344.0),
                };
                errors.push(CompileError::general(
                    &format!(
                        "float literal {v} overflows {name} (largest finite value is {max}) — \
use an FP32 value and convert at runtime if saturation/infinity is intended"
                    ),
                    e.span,
                ));
            }
        }
    }
}

/// Walk a `Binary` expression tree, rewriting a bare float-literal operand to
/// bf16 whenever the *other* operand is a `+`/`-`/`*`/comparison sibling of a
/// known-BF16 identifier (from `bf16_idents`). Recurses into the common
/// expression-tree shapes that can appear inside a statement (nested binary
/// ops, unary, ternary, method-call args, function-call args, index/field
/// access, concat/repeat) so a literal buried a few levels deep (e.g.
/// `(a_bf16 + 1.0) > 2.0`) is still found. Does not cross a `Cast`/
/// `MethodCall` boundary that already fixes a different float type (e.g.
/// inside `x.to_fp32()`'s argument) — those already have explicit typing.
fn coerce_narrow_lits_in_expr(
    e: &mut Expr,
    narrow_idents: &HashMap<String, FloatLitFmt>,
    errors: &mut Vec<CompileError>,
) {
    match &mut e.kind {
        ExprKind::Binary(_, lhs, rhs) => {
            if let Some(fmt) = ident_narrow_fmt(rhs, narrow_idents) {
                coerce_narrow_lit(lhs, fmt, errors);
            }
            if let Some(fmt) = ident_narrow_fmt(lhs, narrow_idents) {
                coerce_narrow_lit(rhs, fmt, errors);
            }
            coerce_narrow_lits_in_expr(lhs, narrow_idents, errors);
            coerce_narrow_lits_in_expr(rhs, narrow_idents, errors);
        }
        ExprKind::Unary(_, inner) | ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
            coerce_narrow_lits_in_expr(inner, narrow_idents, errors);
        }
        ExprKind::Ternary(cond, t, f) => {
            coerce_narrow_lits_in_expr(cond, narrow_idents, errors);
            coerce_narrow_lits_in_expr(t, narrow_idents, errors);
            coerce_narrow_lits_in_expr(f, narrow_idents, errors);
        }
        ExprKind::FieldAccess(base, _) => coerce_narrow_lits_in_expr(base, narrow_idents, errors),
        ExprKind::MethodCall(base, _, args) => {
            coerce_narrow_lits_in_expr(base, narrow_idents, errors);
            for a in args {
                coerce_narrow_lits_in_expr(a, narrow_idents, errors);
            }
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args {
                coerce_narrow_lits_in_expr(a, narrow_idents, errors);
            }
        }
        ExprKind::Index(base, idx) => {
            coerce_narrow_lits_in_expr(base, narrow_idents, errors);
            coerce_narrow_lits_in_expr(idx, narrow_idents, errors);
        }
        ExprKind::Concat(items) => {
            for it in items {
                coerce_narrow_lits_in_expr(it, narrow_idents, errors);
            }
        }
        ExprKind::Repeat(n, inner) => {
            coerce_narrow_lits_in_expr(n, narrow_idents, errors);
            coerce_narrow_lits_in_expr(inner, narrow_idents, errors);
        }
        ExprKind::Inside(base, _) => coerce_narrow_lits_in_expr(base, narrow_idents, errors),
        _ => {}
    }
}

/// Narrow float format of `e` when it is a bare identifier declared with a
/// narrow float type in the current module scope.
fn ident_narrow_fmt(e: &Expr, narrow_idents: &HashMap<String, FloatLitFmt>) -> Option<FloatLitFmt> {
    match &e.kind {
        ExprKind::Ident(name) => narrow_idents.get(name).copied(),
        // Conversion results carry their target format, so a literal
        // compared/combined with one context-types the same way as a
        // declared signal: `a.to_bf16() > 1.0`, `x.to_fp8e4m3() + 0.5`.
        ExprKind::MethodCall(_, m, _) => match m.name.as_str() {
            "to_bf16" => Some(FloatLitFmt::Bf16),
            "to_fp8e4m3" => Some(FloatLitFmt::E4m3),
            "to_fp8e5m2" => Some(FloatLitFmt::E5m2),
            _ => None,
        },
        // Vec element read: the map is keyed on the signal name with the
        // ELEMENT format (narrow_fmt_of_type sees through Vec), so `h[i]`
        // coerces literals exactly like the scalar `h` would.
        ExprKind::Index(base, _) => ident_narrow_fmt(base, narrow_idents),
        // Struct field read: compound "signal.field" keys (populated for
        // signals of struct type with narrow-float fields). One level deep
        // — nested composites reject loudly at typecheck instead.
        ExprKind::FieldAccess(base, field) => {
            if let ExprKind::Ident(root) = &base.kind {
                narrow_idents
                    .get(&format!("{}.{}", root, field.name))
                    .copied()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Walk a statement tree (comb/seq/latch bodies), applying
/// [`coerce_bf16_lits_in_expr`] to every expression reachable from it
/// (assignment RHS, condition exprs, nested if/for/match bodies).
/// Coerce a direct-assignment RHS: a bare float literal takes the target's
/// narrow format; ternary arms recurse (each arm is itself a direct value
/// for the target). Anything else is left to the binop/compare walker.
fn coerce_assign_rhs_lit(e: &mut Expr, fmt: FloatLitFmt, errors: &mut Vec<CompileError>) {
    match &mut e.kind {
        ExprKind::Literal(LitKind::Float(_)) => coerce_narrow_lit(e, fmt, errors),
        ExprKind::Ternary(_, t, f) => {
            coerce_assign_rhs_lit(t, fmt, errors);
            coerce_assign_rhs_lit(f, fmt, errors);
        }
        _ => {}
    }
}

fn coerce_narrow_lits_in_stmt(
    s: &mut Stmt,
    narrow_idents: &HashMap<String, FloatLitFmt>,
    errors: &mut Vec<CompileError>,
) {
    match s {
        Stmt::Assign(a) => {
            // Direct-assignment slot: a bare float literal (or ternary arm)
            // assigned to a narrow-float target takes the TARGET's format —
            // `h <= 0.5;`, `y[i] = 0.5;`, `s.f = 0.5;` all context-type,
            // completing the literal-slot uniformity rule (let/init/reset/
            // default/compare/binop already coerced).
            if let Some(fmt) = ident_narrow_fmt(&a.target, narrow_idents) {
                coerce_assign_rhs_lit(&mut a.value, fmt, errors);
            }
            coerce_narrow_lits_in_expr(&mut a.value, narrow_idents, errors)
        }
        Stmt::IfElse(ie) => {
            coerce_narrow_lits_in_expr(&mut ie.cond, narrow_idents, errors);
            for s in &mut ie.then_stmts {
                coerce_narrow_lits_in_stmt(s, narrow_idents, errors);
            }
            for s in &mut ie.else_stmts {
                coerce_narrow_lits_in_stmt(s, narrow_idents, errors);
            }
        }
        Stmt::For(fl) => {
            for s in &mut fl.body {
                coerce_narrow_lits_in_stmt(s, narrow_idents, errors);
            }
        }
        Stmt::Match(me) => {
            coerce_narrow_lits_in_expr(&mut me.scrutinee, narrow_idents, errors);
            for arm in &mut me.arms {
                for s in &mut arm.body {
                    coerce_narrow_lits_in_stmt(s, narrow_idents, errors);
                }
            }
        }
        _ => {}
    }
}

/// Normalize inst-connection names to a single-element (`ports[1]`) port-array
/// member.
///
/// A first-class construct with a `ports[N]` group (regfile read/write, arbiter
/// request, template) flattens its member ports as `{group}{i}_{sig}` for N>1
/// but DROPS the index when N==1 — the count-1 declaration emits `read_addr`,
/// not `read0_addr` (see `src/codegen/regfile.rs`). The parser, however,
/// flattens an explicit `read[0].addr` connection to `read0_addr` regardless of
/// the target's count. Against a count-1 target that produced a pin/member-name
/// mismatch (`read0_addr` vs `read_addr`) — Verilator `PINNOTFOUND`, and a sim
/// `no member named 'read0_addr'`. Both the idiomatic `read.addr` (no index) and
/// the explicit `read[0].addr` should resolve to the same `read_addr`.
///
/// Here we rewrite any connection named `{group}0_{sig}` to `{group}_{sig}` when
/// the inst's target construct has a LITERAL count-1 group `group` — once, in
/// the AST, so every downstream backend (SV, sim, .archi) sees the consistent
/// un-indexed name. Param-driven counts that resolve to 1 are out of scope (a
/// literal `ports[1]` is the case that occurs in practice).
fn normalize_count1_portarray_conns(sf: &mut SourceFile) {
    use std::collections::HashMap;
    fn count1_groups(groups: &[&PortArrayDecl]) -> Vec<(String, Vec<String>)> {
        groups
            .iter()
            .filter(|pa| matches!(&pa.count_expr.kind, ExprKind::Literal(LitKind::Dec(1))))
            .map(|pa| {
                (
                    pa.name.name.clone(),
                    pa.signals.iter().map(|s| s.name.name.clone()).collect(),
                )
            })
            .collect()
    }

    // construct name → [(group_name, [signal names])] for literal count-1 groups
    let mut count1: HashMap<String, Vec<(String, Vec<String>)>> = HashMap::new();
    for item in &sf.items {
        let (cname, groups): (&str, Vec<&PortArrayDecl>) = match item {
            Item::Regfile(r) => (
                &r.common.name.name,
                r.read_ports.iter().chain(r.write_ports.iter()).collect(),
            ),
            Item::Arbiter(a) => (&a.common.name.name, a.port_arrays.iter().collect()),
            Item::Template(t) => (&t.name.name, t.port_arrays.iter().collect()),
            _ => continue,
        };
        let g1 = count1_groups(&groups);
        if !g1.is_empty() {
            count1.insert(cname.to_string(), g1);
        }
    }
    if count1.is_empty() {
        return;
    }

    for item in &mut sf.items {
        if let Item::Module(m) = item {
            for bi in &mut m.body {
                if let ModuleBodyItem::Inst(inst) = bi {
                    if let Some(groups) = count1.get(&inst.module_name.name) {
                        for conn in &mut inst.connections {
                            for (g, sigs) in groups {
                                for s in sigs {
                                    if conn.port_name.name == format!("{g}0_{s}") {
                                        conn.port_name.name = format!("{g}_{s}");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Generate expansion ────────────────────────────────────────────────────────

/// Read-only view of the *parent* module's port + wire shapes, used by
/// `expand_generate_for` to classify inst-bearing bodies as "shape-stable"
/// (SV-genvar-preservable) vs needing per-iteration unroll. We only need
/// the per-name Vec-of-bus shape — full TypeExpr isn't required.
#[derive(Default)]
pub(crate) struct ParentShapeInfo {
    /// Parent ports/wires that are Vec-of-bus (either `Vec<Bus,N>` port
    /// directly via `bus_info.count.is_some()`, OR a non-bus port/wire
    /// whose type is `Vec<Named, _>` / `Vec<Vec<Named, _>, _>` and so on,
    /// where `Named` *might* be a bus — see `type_expr_contains_bus` for
    /// why we conservatively treat all Named types as potential buses).
    /// Indexing one of these by the loop variable produces a per-iteration
    /// bus-shape reference that the SV genvar form can't emit cleanly.
    vec_of_bus_names: HashSet<String>,
}

impl ParentShapeInfo {
    fn from_module(m: &ModuleDecl) -> Self {
        let mut vec_of_bus_names = HashSet::new();
        for p in &m.ports {
            if let Some(bi) = &p.bus_info {
                if bi.count.is_some() {
                    vec_of_bus_names.insert(p.name.name.clone());
                }
            } else if matches!(p.ty, TypeExpr::Vec(..)) && type_expr_contains_bus(&p.ty) {
                // Defensive: bus type wrapped in Vec without bus_info
                // shouldn't happen for ports today, but cover it anyway.
                vec_of_bus_names.insert(p.name.name.clone());
            }
        }
        for item in &m.body {
            if let ModuleBodyItem::WireDecl(w) = item {
                if matches!(w.ty, TypeExpr::Vec(..)) && type_expr_contains_bus(&w.ty) {
                    vec_of_bus_names.insert(w.name.name.clone());
                }
            }
        }
        ParentShapeInfo { vec_of_bus_names }
    }
}

/// Conservative recursive check: does this TypeExpr contain a `Named`
/// type that *might* be a bus? The symbol table isn't available at this
/// elaboration pass, so we can't distinguish bus-typed Named from
/// struct/enum-typed Named. Treating all `Named` as potentially-bus
/// over-approximates: a Vec-of-struct port/wire gets classified as
/// Vec-of-bus and falls back to elaboration-time unroll if an inst-
/// bearing generate_for reads it via `arr[i]`. That's a conservative
/// loss of compactness for one rare shape (positional Vec-of-struct
/// through an inst connection), not a correctness regression.
///
/// For bus wires the parser records the bus reference as
/// `Named(BusName)` plus `bus_params` for per-site overrides, so
/// `Vec<BusName, N>` becomes `Vec(Named(BusName), N)`. This catches the
/// NIC-400 `wire edges: Vec<Vec<SlaveBus, N>, M>` case — exactly what
/// the unsafe path must keep unrolling.
fn type_expr_contains_bus(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Named(_) => true,
        TypeExpr::Vec(inner, _) => type_expr_contains_bus(inner),
        _ => false,
    }
}

fn expand_generate(
    gen: GenerateDecl,
    param_vals: &HashMap<String, i64>,
    parent_shape: &ParentShapeInfo,
    child_module_ports: &HashMap<String, Vec<PortDecl>>,
) -> Result<(Vec<PortDecl>, Vec<ModuleBodyItem>), Vec<CompileError>> {
    match gen {
        GenerateDecl::For(gf) => {
            expand_generate_for(gf, param_vals, parent_shape, child_module_ports)
        }
        GenerateDecl::If(gi) => expand_generate_if(gi, param_vals),
    }
}

/// Check whether an expression references any identifier that is a param name.
fn expr_references_param(expr: &Expr, param_names: &[String]) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => param_names.contains(name),
        ExprKind::Binary(_, l, r) => {
            expr_references_param(l, param_names) || expr_references_param(r, param_names)
        }
        ExprKind::Unary(_, e) => expr_references_param(e, param_names),
        ExprKind::Clog2(e) | ExprKind::Signed(e) | ExprKind::Unsigned(e) => {
            expr_references_param(e, param_names)
        }
        _ => false,
    }
}

fn expand_generate_for(
    gf: GenerateFor,
    param_vals: &HashMap<String, i64>,
    parent_shape: &ParentShapeInfo,
    child_module_ports: &HashMap<String, Vec<PortDecl>>,
) -> Result<(Vec<PortDecl>, Vec<ModuleBodyItem>), Vec<CompileError>> {
    // Collect param names from param_vals
    let param_names: Vec<String> = param_vals.keys().cloned().collect();

    let has_port_items = gf.items.iter().any(|item| matches!(item, GenItem::Port(_)));
    let has_thread_items = gf
        .items
        .iter()
        .any(|item| matches!(item, GenItem::Thread(_)));
    let has_connect_items = gf
        .items
        .iter()
        .any(|item| matches!(item, GenItem::TlmConnect(_)));
    let has_inst_items = gf.items.iter().any(|item| matches!(item, GenItem::Inst(_)));
    let has_wire_items = gf.items.iter().any(|item| matches!(item, GenItem::Wire(_)));
    let range_depends_on_param = expr_references_param(&gf.start, &param_names)
        || expr_references_param(&gf.end, &param_names);

    // Try to evaluate the range bounds
    let start_val = try_eval_i64(&gf.start, param_vals);
    let end_val = try_eval_i64(&gf.end, param_vals);

    // Classify inst-bearing bodies: even when the body has only `inst`
    // items, we may still be able to preserve the SV genvar form if every
    // connection is "shape-stable" — i.e. doesn't index a Vec-of-bus
    // parent port/wire and doesn't drive a Vec-of-bus child port. Those
    // shapes need per-iteration flat-name expansion that SV genvar can't
    // express, and elaboration must unroll them.
    let only_inst_items = has_inst_items
        && !has_port_items
        && !has_thread_items
        && !has_connect_items
        && !has_wire_items;
    let inst_items_shape_stable = only_inst_items
        && gf.items.iter().all(|item| {
            if let GenItem::Inst(inst) = item {
                inst_is_shape_stable_for_genvar(inst, parent_shape, child_module_ports)
            } else {
                true
            }
        });

    // Preserve the generate block as a SV genvar `for` loop only when the
    // range references a module param AND the body has no items that
    // require elaboration-time unrolling:
    //   - port items: SV `for` can't introduce new ports at the boundary
    //   - thread items: threads lower to FSMs at elaboration time
    //   - TLM connect items: elaborate to private bus wires
    //   - wire items: SV genvars can't introduce new wire identifiers per
    //     iteration (would need hierarchical `gen_i.w` access, which we
    //     don't want at the SV boundary).
    //   - inst items: only preserve when every connection is shape-stable.
    //     "Shape-stable" means the inst's connections are pure scalar or
    //     simple `Ident` / `Index(Ident, loop_var)` references against
    //     parent ports/wires that are NOT Vec-of-bus, AND the child
    //     module's ports themselves are not Vec-of-bus. In that safe case
    //     SV emits `gen_i.foo_i.port(arr[i])` cleanly; sim codegen runs
    //     its own local unroll pass to keep both backends in sync.
    let body_preservable = !has_port_items
        && !has_thread_items
        && !has_connect_items
        && !has_wire_items
        && (!has_inst_items || inst_items_shape_stable);

    if range_depends_on_param && body_preservable {
        return Ok((
            Vec::new(),
            vec![ModuleBodyItem::Generate(GenerateDecl::For(gf))],
        ));
    }

    let start = start_val.ok_or_else(|| {
        vec![CompileError::general(
            "generate for: start expression must be a compile-time constant",
            gf.start.span,
        )]
    })?;
    let end = end_val.ok_or_else(|| {
        vec![CompileError::general(
            "generate for: end expression must be a compile-time constant",
            gf.end.span,
        )]
    })?;

    if end < start {
        return Ok((Vec::new(), Vec::new()));
    }

    let var = &gf.var.name;
    let mut ports = Vec::new();
    let mut body = Vec::new();

    // Before unrolling, enforce Reading B on seq / comb bodies: every LHS
    // must be indexed by the loop variable. Writing to a scalar from inside
    // generate_for would produce N conflicting drivers after unrolling.
    let mut errors: Vec<CompileError> = Vec::new();
    for item in &gf.items {
        match item {
            GenItem::Seq(rb) => check_gen_for_reg_stmts(&rb.stmts, var, &mut errors),
            GenItem::Comb(cb) => check_gen_for_comb_stmts(&cb.stmts, var, &mut errors),
            _ => {}
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    for i in start..=end {
        for item in &gf.items {
            match item {
                GenItem::Port(p) => ports.push(subst_port(p, var, i)),
                GenItem::Inst(inst) => body.push(ModuleBodyItem::Inst(subst_inst(inst, var, i))),
                GenItem::TlmConnect(c) => {
                    body.push(ModuleBodyItem::TlmConnect(subst_tlm_connect(c, var, i)));
                }
                GenItem::Thread(t) => body.push(ModuleBodyItem::Thread(subst_thread(t, var, i))),
                GenItem::Assert(a) => body.push(ModuleBodyItem::Assert(subst_assert(a, var, i))),
                GenItem::Seq(rb) => {
                    body.push(ModuleBodyItem::RegBlock(subst_reg_block(rb, var, i)))
                }
                GenItem::Comb(cb) => {
                    body.push(ModuleBodyItem::CombBlock(subst_comb_block(cb, var, i)))
                }
                GenItem::Wire(w) => body.push(ModuleBodyItem::WireDecl(subst_wire_decl(w, var, i))),
            }
        }
    }

    Ok((ports, body))
}

// ── generate_for shape-stable inst classification ─────────────────────────────
//
// SV genvar form survives if every connection in every inst inside the
// generate_for body satisfies all of:
//
//   1. The connection's signal expression doesn't reference a Vec-of-bus
//      parent port/wire. We allow plain idents (`clk`, `rst`) and
//      `Index(Ident, loop_var)` against non-bus parent ports/wires
//      (e.g. `req[i]` on `port req: in Vec<Bool, N>`).
//
//   2. The child module's matched port is not itself a Vec-of-bus port
//      (`bus_info.count.is_some()`). Vec-of-bus child ports need per-
//      iteration packed-slice assembly that SV genvar can't express.
//
// The conservative default is "unsafe" — if we can't classify, fall back
// to the existing unroll-at-elaboration path. Inst-body for-loops (the
// `for k in ... ins[k] <- edges[k][j]` macro inside an inst body) are
// also unsafe by design: they generate per-iteration flat connection
// names that SV genvar can't carry.
fn inst_is_shape_stable_for_genvar(
    inst: &InstDecl,
    parent_shape: &ParentShapeInfo,
    child_module_ports: &HashMap<String, Vec<PortDecl>>,
) -> bool {
    // Inst-body for-loops produce per-iteration flat wiring — always unsafe.
    if !inst.for_loops.is_empty() {
        return false;
    }

    // Look up child module ports; if we don't have them, be conservative.
    let child_ports = match child_module_ports.get(&inst.module_name.name) {
        Some(p) => p,
        None => return false,
    };

    for conn in &inst.connections {
        // Child-side: Vec-of-bus port? Unsafe.
        if let Some(p) = child_ports
            .iter()
            .find(|p| p.name.name == conn.port_name.name)
        {
            if let Some(bi) = &p.bus_info {
                if bi.count.is_some() {
                    return false;
                }
            }
            // Vec ports on the child side: scalar Vec is fine (e.g.
            // `port a: in Bool` driven by `req[i]`). But if the child
            // port itself is a Vec type AND not a bus, that's still
            // fine for SV genvar — the genvar substitution turns it
            // into `pt_<i>.a(req[i])` which is unambiguous.
        }

        // Parent signal-side: indexing a Vec-of-bus port/wire by the
        // loop var produces a per-iteration bus shape. Unsafe.
        if signal_indexes_vec_of_bus(&conn.signal, parent_shape) {
            return false;
        }
    }

    true
}

/// True iff the connection signal references a Vec-of-bus parent
/// port/wire by indexing (e.g. `m[i]`, `edges[i]`) — exactly the shape
/// that needs per-iteration flat-name expansion. Plain ident references
/// to non-Vec-of-bus signals (`clk`, `rst`) and plain idents to scalar
/// Vec ports (`req`) are both fine for SV genvar.
fn signal_indexes_vec_of_bus(expr: &Expr, parent_shape: &ParentShapeInfo) -> bool {
    match &expr.kind {
        // `name[idx]` — check if `name` is Vec-of-bus.
        ExprKind::Index(base, _) => {
            if let ExprKind::Ident(n) = &base.kind {
                if parent_shape.vec_of_bus_names.contains(n) {
                    return true;
                }
            }
            // Recurse: e.g. `edges[i][j]` — outer `Index(Index(Ident, _), _)`.
            signal_indexes_vec_of_bus(base, parent_shape)
        }
        // Plain ident: not an indexed Vec-of-bus reference.
        ExprKind::Ident(_) => false,
        // Field access on a bus is fine (`bus.signal`) and won't appear
        // at the top of an inst connection signal anyway. Recurse just
        // in case.
        ExprKind::FieldAccess(b, _) => signal_indexes_vec_of_bus(b, parent_shape),
        // Casts (`x as Reset<...>`) — recurse on inner.
        ExprKind::Cast(b, _) => signal_indexes_vec_of_bus(b, parent_shape),
        _ => false,
    }
}

// ── generate_for seq/comb write-target check (Reading B) ──────────────────────
//
// Inside a generate_for's seq/comb body, every assignment LHS must be of the
// form `<ident>[<expr-using-loop-var>]` (with optional nested struct-field or
// bit-slice access). Reads on RHS are unrestricted.

fn expr_mentions_ident(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Ident(n) => n == name,
        ExprKind::Binary(_, l, r) => expr_mentions_ident(l, name) || expr_mentions_ident(r, name),
        ExprKind::Unary(_, x) => expr_mentions_ident(x, name),
        ExprKind::FieldAccess(b, _) => expr_mentions_ident(b, name),
        ExprKind::Index(b, i) => expr_mentions_ident(b, name) || expr_mentions_ident(i, name),
        ExprKind::BitSlice(b, h, l) => {
            expr_mentions_ident(b, name)
                || expr_mentions_ident(h, name)
                || expr_mentions_ident(l, name)
        }
        ExprKind::Cast(e, _) => expr_mentions_ident(e, name),
        ExprKind::Ternary(c, t, f) => {
            expr_mentions_ident(c, name)
                || expr_mentions_ident(t, name)
                || expr_mentions_ident(f, name)
        }
        ExprKind::Concat(xs) => xs.iter().any(|x| expr_mentions_ident(x, name)),
        ExprKind::MethodCall(r, _, args) => {
            expr_mentions_ident(r, name) || args.iter().any(|a| expr_mentions_ident(a, name))
        }
        _ => false,
    }
}

/// Every unrolled iteration of a generate_for must write a *distinct* target,
/// otherwise N copies of the loop body all drive the same signal. The only
/// accepted LHS shape is an index by the loop variable:
///
///   `vec_reg[i] <= ...`, or nested through a field / bit-slice, e.g.
///   `vec_reg[i].field <= ...`, `vec_reg[i][7:0] = ...`.
///
/// A bare-identifier LHS — even one with an `_i` suffix — is rejected. The
/// earlier revision accepted suffix names on the reasoning that ports / insts
/// declared inside generate_for get substituted into distinct `_0 / _1 / ...`
/// names. But that only holds when the target IS a generate_for-substituted
/// declaration; a scalar reg at module scope happening to end with `_i` was
/// silently accepted, then substituted to non-existent names like `cnt_0`,
/// leaving `arch check` / `arch build` happy while emitting SV that Verilator
/// rejects. The Vec-at-module-scope pattern (`reg store: Vec<T, N>` + `store[i]
/// <= ...`) supersedes the suffix shape cleanly.
fn lhs_is_loop_indexed(lhs: &Expr, var: &str) -> bool {
    match &lhs.kind {
        ExprKind::Index(_, idx) => expr_mentions_ident(idx, var),
        ExprKind::FieldAccess(base, _) => lhs_is_loop_indexed(base, var),
        ExprKind::BitSlice(base, _, _) => lhs_is_loop_indexed(base, var),
        _ => false,
    }
}

fn reject_bad_lhs(lhs: &Expr, var: &str, errors: &mut Vec<CompileError>) {
    if !lhs_is_loop_indexed(lhs, var) {
        errors.push(CompileError::general(
            &format!(
                "write target inside generate_for must be indexed by the loop \
                 variable `{var}`, e.g. `vec_reg[{var}] <= ...`. Declare the \
                 Vec-typed reg or port at module scope and index it here — \
                 scalar writes would produce multiple drivers after unrolling."
            ),
            lhs.span,
        ));
    }
}

fn check_gen_for_reg_stmts(stmts: &[Stmt], var: &str, errors: &mut Vec<CompileError>) {
    for s in stmts {
        match s {
            Stmt::Assign(a) => reject_bad_lhs(&a.target, var, errors),
            Stmt::IfElse(ie) => {
                check_gen_for_reg_stmts(&ie.then_stmts, var, errors);
                check_gen_for_reg_stmts(&ie.else_stmts, var, errors);
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    check_gen_for_reg_stmts(&arm.body, var, errors);
                }
            }
            Stmt::For(f) => check_gen_for_reg_stmts(&f.body, var, errors),
            Stmt::Init(ib) => check_gen_for_reg_stmts(&ib.body, var, errors),
            Stmt::Log(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {}
        }
    }
}

fn check_gen_for_comb_stmts(stmts: &[Stmt], var: &str, errors: &mut Vec<CompileError>) {
    for s in stmts {
        match s {
            Stmt::Assign(a) => reject_bad_lhs(&a.target, var, errors),
            Stmt::IfElse(ie) => {
                check_gen_for_comb_stmts(&ie.then_stmts, var, errors);
                check_gen_for_comb_stmts(&ie.else_stmts, var, errors);
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    check_gen_for_comb_stmts(&arm.body, var, errors);
                }
            }
            Stmt::For(f) => check_gen_for_comb_stmts(&f.body, var, errors),
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("seq-only Stmt variant inside comb-context walker")
            }
            Stmt::Log(_) => {}
        }
    }
}

// ── Substitution helpers for generate_for's seq / comb bodies ─────────────────

fn subst_reg_block(rb: &RegBlock, var: &str, val: i64) -> RegBlock {
    RegBlock {
        clock: rb.clock.clone(),
        clock_edge: rb.clock_edge,
        stmts: rb
            .stmts
            .iter()
            .map(|s| subst_reg_stmt(s, var, val))
            .collect(),
        span: rb.span,
    }
}

fn subst_reg_stmt(s: &Stmt, var: &str, val: i64) -> Stmt {
    // Use subst_expr_names (suffix-rewriting variant) consistent with how
    // thread bodies and generate_for ports/insts are substituted. That
    // correctly rewrites `rdata_i` → `rdata_0` and also substitutes bare `i`
    // uses in indices like `store[i]` → `store[0]`.
    match s {
        Stmt::Assign(a) => Stmt::Assign(Assign {
            target: subst_expr_names(a.target.clone(), var, val),
            value: subst_expr_names(a.value.clone(), var, val),
            span: a.span,
        }),
        Stmt::IfElse(ie) => Stmt::IfElse(IfElseOf {
            cond: subst_expr_names(ie.cond.clone(), var, val),
            then_stmts: ie
                .then_stmts
                .iter()
                .map(|x| subst_reg_stmt(x, var, val))
                .collect(),
            else_stmts: ie
                .else_stmts
                .iter()
                .map(|x| subst_reg_stmt(x, var, val))
                .collect(),
            unique: ie.unique,
            span: ie.span,
        }),
        Stmt::Match(m) => Stmt::Match(MatchStmt {
            scrutinee: subst_expr_names(m.scrutinee.clone(), var, val),
            arms: m
                .arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: arm.pattern.clone(),
                    body: arm
                        .body
                        .iter()
                        .map(|s| subst_reg_stmt(s, var, val))
                        .collect(),
                })
                .collect(),
            unique: m.unique,
            span: m.span,
        }),
        // Log/For/Init/WaitUntil/DoUntil: pass through. If we ever want to
        // support loop-var substitution in these nested contexts we can
        // extend this match — for now they're unusual inside generate_for
        // and the LHS check above already guards correctness.
        other => other.clone(),
    }
}

fn subst_comb_block(cb: &CombBlock, var: &str, val: i64) -> CombBlock {
    CombBlock {
        stmts: cb
            .stmts
            .iter()
            .map(|s| subst_comb_stmt(s, var, val))
            .collect(),
        span: cb.span,
    }
}

fn subst_comb_stmt(s: &Stmt, var: &str, val: i64) -> Stmt {
    match s {
        Stmt::Assign(a) => Stmt::Assign(Assign {
            target: subst_expr_names(a.target.clone(), var, val),
            value: subst_expr_names(a.value.clone(), var, val),
            span: a.span,
        }),
        Stmt::IfElse(ie) => Stmt::IfElse(IfElseOf {
            cond: subst_expr_names(ie.cond.clone(), var, val),
            then_stmts: ie
                .then_stmts
                .iter()
                .map(|x| subst_comb_stmt(x, var, val))
                .collect(),
            else_stmts: ie
                .else_stmts
                .iter()
                .map(|x| subst_comb_stmt(x, var, val))
                .collect(),
            unique: ie.unique,
            span: ie.span,
        }),
        other => other.clone(),
    }
}

fn expand_generate_if(
    gi: GenerateIf,
    param_vals: &HashMap<String, i64>,
) -> Result<(Vec<PortDecl>, Vec<ModuleBodyItem>), Vec<CompileError>> {
    let cond = try_eval_bool(&gi.cond, param_vals).ok_or_else(|| {
        vec![CompileError::general(
            "generate if: condition must be a compile-time constant boolean \
             (literal, param name, or comparison of params/literals)",
            gi.cond.span,
        )]
    })?;

    let active_items = if cond { gi.then_items } else { gi.else_items };

    let mut ports = Vec::new();
    let mut body = Vec::new();
    for item in active_items {
        match item {
            GenItem::Port(p) => ports.push(p),
            GenItem::Inst(inst) => body.push(ModuleBodyItem::Inst(inst)),
            GenItem::TlmConnect(c) => body.push(ModuleBodyItem::TlmConnect(c)),
            GenItem::Thread(t) => body.push(ModuleBodyItem::Thread(t)),
            GenItem::Assert(a) => body.push(ModuleBodyItem::Assert(a)),
            // No loop var in generate_if, so seq/comb/wire pass through
            // verbatim. Reading B's write-target rule only applies to
            // generate_for.
            GenItem::Seq(rb) => body.push(ModuleBodyItem::RegBlock(rb)),
            GenItem::Comb(cb) => body.push(ModuleBodyItem::CombBlock(cb)),
            GenItem::Wire(w) => body.push(ModuleBodyItem::WireDecl(w)),
        }
    }
    Ok((ports, body))
}

// ── Substitution helpers ──────────────────────────────────────────────────────

fn subst_tlm_connect(c: &TlmConnectDecl, var: &str, val: i64) -> TlmConnectDecl {
    TlmConnectDecl {
        from_inst: subst_ident(&c.from_inst, var, val),
        from_port: c.from_port.clone(),
        targets: c
            .targets
            .iter()
            .map(|target| TlmConnectTarget {
                to_inst: subst_ident(&target.to_inst, var, val),
                to_port: target.to_port.clone(),
                decode: target.decode.as_ref().map(|decode| match decode {
                    TlmConnectDecode::Range { lo, hi } => TlmConnectDecode::Range {
                        lo: subst_expr(lo.clone(), var, val),
                        hi: subst_expr(hi.clone(), var, val),
                    },
                    TlmConnectDecode::Default => TlmConnectDecode::Default,
                }),
                span: target.span,
            })
            .collect(),
        decode_field: c.decode_field.clone(),
        span: c.span,
    }
}

fn subst_port(p: &PortDecl, var: &str, val: i64) -> PortDecl {
    PortDecl {
        name: subst_ident(&p.name, var, val),
        direction: p.direction,
        ty: subst_type_expr(&p.ty, var, val),
        default: p.default.as_ref().map(|e| subst_expr(e.clone(), var, val)),
        reg_info: p.reg_info.clone(),
        bus_info: p.bus_info.clone(),
        shared: p.shared,
        unpacked: p.unpacked,
        unpacked_ascending: p.unpacked_ascending,
        comb_deps: p.comb_deps.clone(),
        span: p.span,
    }
}

/// Unroll any `for VAR in S..E ... end for` blocks inside `inst.for_loops`
/// into flat `Connection`s appended to `inst.connections`. Loop ranges may
/// reference the enclosing module's params (passed via `param_vals`). After
/// this pass `for_loops` is empty.
///
/// Substitution semantics match the rest of the elaborator: bare-ident
/// matches of the loop variable become literals, and `signal_VAR` →
/// `signal_<val>` suffix rewrites also happen — same as `subst_expr_names`.
/// This means a hand-enumerated form and the loop-unrolled form produce
/// byte-identical `InstDecl.connections` lists.
pub(crate) fn flatten_inst_for_loops(
    mut inst: InstDecl,
    param_vals: &HashMap<String, i64>,
) -> Result<InstDecl, Vec<CompileError>> {
    if inst.for_loops.is_empty() {
        return Ok(inst);
    }
    let loops = std::mem::take(&mut inst.for_loops);
    let mut errors: Vec<CompileError> = Vec::new();
    for fl in loops {
        match unroll_inst_for_loop(&fl, param_vals) {
            Ok(conns) => inst.connections.extend(conns),
            Err(mut errs) => errors.append(&mut errs),
        }
    }
    if errors.is_empty() {
        Ok(inst)
    } else {
        Err(errors)
    }
}

fn unroll_inst_for_loop(
    fl: &InstForLoop,
    param_vals: &HashMap<String, i64>,
) -> Result<Vec<Connection>, Vec<CompileError>> {
    let start = try_eval_i64(&fl.start, param_vals).ok_or_else(|| {
        vec![CompileError::general(
            "inst body `for`: start expression must be a compile-time constant",
            fl.start.span,
        )]
    })?;
    let end = try_eval_i64(&fl.end, param_vals).ok_or_else(|| {
        vec![CompileError::general(
            "inst body `for`: end expression must be a compile-time constant",
            fl.end.span,
        )]
    })?;
    if end < start {
        return Ok(Vec::new());
    }
    let var = &fl.var.name;
    let mut out: Vec<Connection> = Vec::new();
    for i in start..=end {
        for item in &fl.body {
            match item {
                InstBodyItem::Connection(c) => {
                    out.push(Connection {
                        port_name: subst_ident(&c.port_name, var, i),
                        direction: c.direction,
                        signal: subst_expr_names(c.signal.clone(), var, i),
                        reset_override: c.reset_override,
                        span: c.span,
                    });
                }
                InstBodyItem::For(inner) => {
                    // Recurse with the outer loop var substituted into the
                    // inner range/body, then evaluate the inner loop in the
                    // same param scope (loop vars don't pollute param_vals;
                    // they're applied via subst).
                    let inner_subst = subst_inst_for_loop(inner, var, i);
                    let mut inner_conns = unroll_inst_for_loop(&inner_subst, param_vals)?;
                    out.append(&mut inner_conns);
                }
            }
        }
    }
    Ok(out)
}

pub(crate) fn subst_inst(inst: &InstDecl, var: &str, val: i64) -> InstDecl {
    InstDecl {
        name: subst_ident(&inst.name, var, val),
        module_name: inst.module_name.clone(),
        param_assigns: inst
            .param_assigns
            .iter()
            .map(|pa| ParamAssign {
                name: pa.name.clone(),
                value: subst_expr(pa.value.clone(), var, val),
                ty: pa.ty.clone(),
            })
            .collect(),
        // Connection signals may reference suffix-substituted names from the
        // enclosing generate_for (e.g. `done -> done_i` becomes `done -> done_0`
        // for i=0). `subst_expr` only rewrites bare loop-var idents; using the
        // suffix-aware `subst_expr_names` matches how thread-stmt / seq-stmt
        // substitution already handles this, and fixes a bug where inst
        // outputs connecting to per-iteration output ports didn't propagate
        // the drive through unroll.
        connections: inst
            .connections
            .iter()
            .map(|c| Connection {
                port_name: subst_ident(&c.port_name, var, val),
                direction: c.direction,
                signal: subst_expr_names(c.signal.clone(), var, val),
                reset_override: c.reset_override,
                span: c.span,
            })
            .collect(),
        // Inst-body for-loops may also reference the outer loop var in their
        // range bounds or body. Substitute through them so that
        // `flatten_inst_for_loops` later sees a fully-resolved-w.r.t.-outer-vars
        // form. The inner loop var itself is *not* substituted (it shadows).
        for_loops: inst
            .for_loops
            .iter()
            .map(|fl| subst_inst_for_loop(fl, var, val))
            .collect(),
        span: inst.span,
    }
}

/// Substitute an outer loop var into an inst-body for-loop's range bounds
/// and body. If the inner loop's variable shadows the outer name, the body
/// is left untouched (the inner binding wins).
fn subst_inst_for_loop(fl: &InstForLoop, var: &str, val: i64) -> InstForLoop {
    let shadowed = fl.var.name == var;
    InstForLoop {
        var: fl.var.clone(),
        start: subst_expr_names(fl.start.clone(), var, val),
        end: subst_expr_names(fl.end.clone(), var, val),
        body: if shadowed {
            fl.body.clone()
        } else {
            fl.body
                .iter()
                .map(|it| subst_inst_body_item(it, var, val))
                .collect()
        },
        span: fl.span,
    }
}

fn subst_inst_body_item(it: &InstBodyItem, var: &str, val: i64) -> InstBodyItem {
    match it {
        InstBodyItem::Connection(c) => InstBodyItem::Connection(Connection {
            port_name: subst_ident(&c.port_name, var, val),
            direction: c.direction,
            signal: subst_expr_names(c.signal.clone(), var, val),
            reset_override: c.reset_override,
            span: c.span,
        }),
        InstBodyItem::For(inner) => InstBodyItem::For(subst_inst_for_loop(inner, var, val)),
    }
}

fn subst_thread(t: &ThreadBlock, var: &str, val: i64) -> ThreadBlock {
    ThreadBlock {
        name: t.name.as_ref().map(|n| subst_ident(n, var, val)),
        clock: t.clock.clone(),
        clock_edge: t.clock_edge,
        reset: t.reset.clone(),
        reset_level: t.reset_level,
        once: t.once,
        default_when: t.default_when.as_ref().map(|(cond, stmts)| {
            (
                subst_expr_names(cond.clone(), var, val),
                stmts
                    .iter()
                    .map(|s| subst_thread_stmt(s, var, val))
                    .collect(),
            )
        }),
        default_comb: t
            .default_comb
            .iter()
            .map(|s| subst_comb_stmt(s, var, val))
            .collect(),
        tlm_target: t.tlm_target.as_ref().map(|tb| TlmTargetBinding {
            port: tb.port.clone(),
            method: tb.method.clone(),
            tag_lane: tb
                .tag_lane
                .as_ref()
                .map(|e| subst_expr_names(e.clone(), var, val)),
            args: tb.args.clone(),
        }),
        implement: t.implement.clone(),
        body: t
            .body
            .iter()
            .map(|s| subst_thread_stmt(s, var, val))
            .collect(),
        span: t.span,
    }
}

fn subst_thread_stmt(stmt: &ThreadStmt, var: &str, val: i64) -> ThreadStmt {
    match stmt {
        ThreadStmt::CombAssign(ca) => ThreadStmt::CombAssign(CombAssign {
            target: subst_expr_names(ca.target.clone(), var, val),
            value: subst_expr_names(ca.value.clone(), var, val),
            span: ca.span,
        }),
        ThreadStmt::SeqAssign(ra) => ThreadStmt::SeqAssign(RegAssign {
            target: subst_expr_names(ra.target.clone(), var, val),
            value: subst_expr_names(ra.value.clone(), var, val),
            span: ra.span,
        }),
        ThreadStmt::ForkTlmAssign(ra) => ThreadStmt::ForkTlmAssign(RegAssign {
            target: subst_expr_names(ra.target.clone(), var, val),
            value: subst_expr_names(ra.value.clone(), var, val),
            span: ra.span,
        }),
        ThreadStmt::JoinAll(sp) => ThreadStmt::JoinAll(*sp),
        ThreadStmt::WaitUntil(cond, sp) => {
            ThreadStmt::WaitUntil(subst_expr_names(cond.clone(), var, val), *sp)
        }
        ThreadStmt::WaitCycles(n, sp) => {
            ThreadStmt::WaitCycles(subst_expr_names(n.clone(), var, val), *sp)
        }
        ThreadStmt::IfElse(ie) => ThreadStmt::IfElse(ThreadIfElse {
            cond: subst_expr_names(ie.cond.clone(), var, val),
            then_stmts: ie
                .then_stmts
                .iter()
                .map(|s| subst_thread_stmt(s, var, val))
                .collect(),
            else_stmts: ie
                .else_stmts
                .iter()
                .map(|s| subst_thread_stmt(s, var, val))
                .collect(),
            unique: ie.unique,
            span: ie.span,
        }),
        ThreadStmt::ForkJoin(branches, sp) => ThreadStmt::ForkJoin(
            branches
                .iter()
                .map(|br| br.iter().map(|s| subst_thread_stmt(s, var, val)).collect())
                .collect(),
            *sp,
        ),
        ThreadStmt::For {
            var: fvar,
            start: fstart,
            end: fend,
            body,
            span,
        } => ThreadStmt::For {
            var: subst_ident(fvar, var, val),
            start: subst_expr_names(fstart.clone(), var, val),
            end: subst_expr_names(fend.clone(), var, val),
            body: body
                .iter()
                .map(|s| subst_thread_stmt(s, var, val))
                .collect(),
            span: *span,
        },
        ThreadStmt::Lock {
            resource,
            body,
            span,
        } => ThreadStmt::Lock {
            resource: resource.clone(),
            body: body
                .iter()
                .map(|s| subst_thread_stmt(s, var, val))
                .collect(),
            span: *span,
        },
        ThreadStmt::DoUntil { body, cond, span } => ThreadStmt::DoUntil {
            body: body
                .iter()
                .map(|s| subst_thread_stmt(s, var, val))
                .collect(),
            cond: subst_expr_names(cond.clone(), var, val),
            span: *span,
        },
        ThreadStmt::Log(l) => ThreadStmt::Log(l.clone()),
        ThreadStmt::Return(e, span) => {
            ThreadStmt::Return(subst_expr_names(e.clone(), var, val), *span)
        }
    }
}

/// Like `subst_expr` but also applies `subst_name` to all identifiers (for thread
/// signal name substitution: `valid_i` → `valid_0`).
fn subst_expr_names(expr: Expr, var: &str, val: i64) -> Expr {
    let new_kind = match expr.kind {
        ExprKind::Ident(ref name) => {
            // Exact match: bare loop variable → replace with literal
            if name == var {
                ExprKind::Literal(LitKind::Dec(val as u64))
            } else {
                // Suffix match: signal_i → signal_0 (name substitution)
                let new_name = subst_name(name, var, val);
                if new_name != *name {
                    ExprKind::Ident(new_name)
                } else {
                    return expr;
                }
            }
        }
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            op,
            Box::new(subst_expr_names(*l, var, val)),
            Box::new(subst_expr_names(*r, var, val)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(op, Box::new(subst_expr_names(*e, var, val))),
        ExprKind::FieldAccess(e, f) => ExprKind::FieldAccess(
            Box::new(subst_expr_names(*e, var, val)),
            subst_ident(&f, var, val),
        ),
        ExprKind::MethodCall(e, m, args) => ExprKind::MethodCall(
            Box::new(subst_expr_names(*e, var, val)),
            m,
            args.into_iter()
                .map(|a| subst_expr_names(a, var, val))
                .collect(),
        ),
        ExprKind::Index(base, idx) => ExprKind::Index(
            Box::new(subst_expr_names(*base, var, val)),
            Box::new(subst_expr_names(*idx, var, val)),
        ),
        ExprKind::BitSlice(base, hi, lo) => ExprKind::BitSlice(
            Box::new(subst_expr_names(*base, var, val)),
            Box::new(subst_expr_names(*hi, var, val)),
            Box::new(subst_expr_names(*lo, var, val)),
        ),
        ExprKind::Cast(e, ty) => ExprKind::Cast(Box::new(subst_expr_names(*e, var, val)), ty),
        ExprKind::Concat(exprs) => ExprKind::Concat(
            exprs
                .into_iter()
                .map(|e| subst_expr_names(e, var, val))
                .collect(),
        ),
        ExprKind::Ternary(c, t, f) => ExprKind::Ternary(
            Box::new(subst_expr_names(*c, var, val)),
            Box::new(subst_expr_names(*t, var, val)),
            Box::new(subst_expr_names(*f, var, val)),
        ),
        other => other,
    };
    Expr {
        kind: new_kind,
        span: expr.span,
        parenthesized: expr.parenthesized,
    }
}

fn subst_ident(ident: &Ident, var: &str, val: i64) -> Ident {
    Ident {
        name: subst_name(&ident.name, var, val),
        span: ident.span,
    }
}

/// Substitute the loop var into a wire declaration: rename `w_i` →
/// `w_<val>` (via subst_ident → subst_name's suffix rewrite), and walk
/// any expressions in the wire type (e.g. `Vec<T, N-i>` where N-i references
/// the loop var). Bus-wire params and Vec count expressions all flow
/// through subst_expr / subst_type_expr.
fn subst_wire_decl(w: &WireDecl, var: &str, val: i64) -> WireDecl {
    WireDecl {
        name: subst_ident(&w.name, var, val),
        ty: subst_type_expr(&w.ty, var, val),
        unpacked: w.unpacked,
        unpacked_ascending: w.unpacked_ascending,
        bus_params: w
            .bus_params
            .iter()
            .map(|pa| ParamAssign {
                name: pa.name.clone(),
                value: subst_expr(pa.value.clone(), var, val),
                ty: pa.ty.clone(),
            })
            .collect(),
        span: w.span,
    }
}

fn subst_assert(a: &AssertDecl, var: &str, val: i64) -> AssertDecl {
    AssertDecl {
        kind: a.kind.clone(),
        name: a.name.as_ref().map(|n| subst_ident(n, var, val)),
        expr: subst_expr(a.expr.clone(), var, val),
        span: a.span,
    }
}

fn subst_name(name: &str, var: &str, val: i64) -> String {
    let suffix = format!("_{}", var);
    if name.ends_with(&suffix) {
        let base = &name[..name.len() - suffix.len()];
        format!("{}_{}", base, val)
    } else if name == var {
        format!("g{}", val)
    } else {
        name.to_string()
    }
}

fn subst_type_expr(ty: &TypeExpr, var: &str, val: i64) -> TypeExpr {
    match ty {
        TypeExpr::UInt(e) => TypeExpr::UInt(Box::new(subst_expr(*e.clone(), var, val))),
        TypeExpr::SInt(e) => TypeExpr::SInt(Box::new(subst_expr(*e.clone(), var, val))),
        TypeExpr::Vec(inner, size) => TypeExpr::Vec(
            Box::new(subst_type_expr(inner, var, val)),
            Box::new(subst_expr(*size.clone(), var, val)),
        ),
        other => other.clone(),
    }
}

fn subst_expr(expr: Expr, var: &str, val: i64) -> Expr {
    let new_kind = match expr.kind {
        ExprKind::Ident(ref name) if name == var => ExprKind::Literal(LitKind::Dec(val as u64)),
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            op,
            Box::new(subst_expr(*l, var, val)),
            Box::new(subst_expr(*r, var, val)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(op, Box::new(subst_expr(*e, var, val))),
        ExprKind::FieldAccess(e, f) => ExprKind::FieldAccess(Box::new(subst_expr(*e, var, val)), f),
        ExprKind::MethodCall(e, m, args) => ExprKind::MethodCall(
            Box::new(subst_expr(*e, var, val)),
            m,
            args.into_iter().map(|a| subst_expr(a, var, val)).collect(),
        ),
        ExprKind::Index(base, idx) => ExprKind::Index(
            Box::new(subst_expr(*base, var, val)),
            Box::new(subst_expr(*idx, var, val)),
        ),
        ExprKind::BitSlice(base, hi, lo) => ExprKind::BitSlice(
            Box::new(subst_expr(*base, var, val)),
            Box::new(subst_expr(*hi, var, val)),
            Box::new(subst_expr(*lo, var, val)),
        ),
        ExprKind::Cast(e, ty) => ExprKind::Cast(Box::new(subst_expr(*e, var, val)), ty),
        ExprKind::Concat(exprs) => {
            ExprKind::Concat(exprs.into_iter().map(|e| subst_expr(e, var, val)).collect())
        }
        other => other,
    };
    Expr {
        kind: new_kind,
        span: expr.span,
        parenthesized: false,
    }
}

fn _dummy_span() -> Span {
    Span::new(0, 0)
}

// ── pipe_reg<T, N> port lowering ─────────────────────────────────────────
//
// Expand every `port q: out pipe_reg<T, N>` with N > 1 into:
//   - The original port keeps `latency = 1` (emits as today's `port reg`)
//   - N-1 synthesized regs `q_stg1` .. `q_stg{N-1}` of type T
//   - Every `q@N <= expr` is rewritten to the cascade:
//         q_stg1 <= expr;
//         q_stg2 <= q_stg1;
//         ...
//         q      <= q_stg{N-1};
//
// Reset/init propagate from the original port's reg_info to every
// intermediate reg (uniform behavior — all stages reset to the same value,
// matching today's pipe_reg semantics).
//
// Called from main.rs after `lower_threads` so every other elaboration
// pass sees the original unexpanded form.

pub fn lower_pipe_reg_ports(
    ast: SourceFile,
) -> Result<(SourceFile, Vec<crate::diagnostics::CompileWarning>), Vec<CompileError>> {
    let mut new_items: Vec<Item> = Vec::with_capacity(ast.items.len());
    let mut errors: Vec<CompileError> = Vec::new();
    let mut warnings: Vec<crate::diagnostics::CompileWarning> = Vec::new();
    for item in ast.items {
        match item {
            Item::Module(m) => match lower_pipe_reg_module(m, &mut warnings) {
                Ok(new_m) => new_items.push(Item::Module(new_m)),
                Err(mut errs) => errors.append(&mut errs),
            },
            other => new_items.push(other),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok((
        SourceFile {
            items: new_items,
            inner_doc: None,
            frontmatter: None,
        },
        warnings,
    ))
}

struct PipePortInfoLocal {
    name: String,
    latency: u32,
    ty: TypeExpr,
    reset: RegReset,
    init: Option<Expr>,
    span: Span,
}

fn lower_pipe_reg_module(
    mut m: ModuleDecl,
    warnings: &mut Vec<crate::diagnostics::CompileWarning>,
) -> Result<ModuleDecl, Vec<CompileError>> {
    // Collect metadata for every pipe_reg port (latency >= 1).
    // Ports with latency == 1 still participate in the @N validation —
    // legacy `port reg` is equivalent to `pipe_reg<T, 1>`.
    let mut all_pipe_ports: Vec<PipePortInfoLocal> = Vec::new();
    for p in &m.ports {
        if let Some(ri) = &p.reg_info {
            all_pipe_ports.push(PipePortInfoLocal {
                name: p.name.name.clone(),
                latency: ri.latency,
                ty: p.ty.clone(),
                reset: ri.reset.clone(),
                init: ri.init.clone(),
                span: p.span,
            });
        }
    }
    // Validation: walk every seq assignment. Errors for
    //   - q@N <= Y when N != declared depth
    //   - bare q <= Y on pipe_reg with depth > 1 (ambiguous)
    //   - q@K on RHS for K > 0 (intermediate stage reads not supported v1)
    //   - q@0 = Y on combinational port (not a pipe_reg)
    // Build name → total-stages for tap-bound checks of `q@K` reads.
    // Includes module-scope `pipe_reg` decls (depth = `stages`) and
    // pipe_reg ports (depth = port latency).
    let mut pipe_depths: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for pp in &all_pipe_ports {
        pipe_depths.insert(pp.name.clone(), pp.latency);
    }
    for bi in &m.body {
        if let ModuleBodyItem::PipeRegDecl(p) = bi {
            pipe_depths.insert(p.name.name.clone(), p.stages);
        }
    }
    let mut errors: Vec<CompileError> = Vec::new();
    for bi in &m.body {
        if let ModuleBodyItem::RegBlock(rb) = bi {
            validate_pipe_assignments(
                &rb.stmts,
                &all_pipe_ports,
                &pipe_depths,
                &mut errors,
                warnings,
            );
        }
        if let ModuleBodyItem::CombBlock(cb) = bi {
            validate_comb_pipe_refs(
                &cb.stmts,
                &all_pipe_ports,
                &m.ports,
                &pipe_depths,
                &mut errors,
            );
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Filter to ports that actually need the cascade expansion (latency > 1).
    let pipes: Vec<PipePortInfoLocal> = all_pipe_ports
        .into_iter()
        .filter(|pp| pp.latency > 1)
        .collect();
    if pipes.is_empty() {
        return Ok(m);
    }
    // Collapse each pipe port to latency=1 (so it emits as a regular port-reg).
    for p in &mut m.ports {
        if let Some(ri) = &mut p.reg_info {
            if ri.latency > 1 {
                ri.latency = 1;
            }
        }
    }

    // For each pipe port, insert N-1 RegDecls for the intermediate stages.
    let mut extra_body: Vec<ModuleBodyItem> = Vec::new();
    for pp in &pipes {
        for stage in 1..pp.latency {
            let stg_name = format!("{}_stg{}", pp.name, stage);
            extra_body.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(stg_name, pp.span),
                ty: pp.ty.clone(),
                init: pp.init.clone(),
                reset: pp.reset.clone(),
                guard: None,
                multicycle: None,
                span: pp.span,
            }));
        }
    }

    // Rewrite every `q@N <= expr` assignment inside seq/reg blocks into the
    // cascade. The rewrite happens recursively through if/match/for bodies.
    for bi in &mut m.body {
        if let ModuleBodyItem::RegBlock(rb) = bi {
            rb.stmts = rewrite_seq_stmts(std::mem::take(&mut rb.stmts), &pipes);
        }
    }

    // Prepend the synthesized regs just before the first RegBlock, so
    // module-body ordering stays sane (regs before seq blocks by
    // convention).
    let mut new_body: Vec<ModuleBodyItem> = Vec::with_capacity(m.body.len() + extra_body.len());
    let mut inserted = false;
    for bi in m.body {
        if !inserted && matches!(bi, ModuleBodyItem::RegBlock(_)) {
            new_body.extend(extra_body.drain(..));
            inserted = true;
        }
        new_body.push(bi);
    }
    if !inserted {
        new_body.extend(extra_body.drain(..));
    }
    m.body = new_body;
    Ok(m)
}

// Validation helpers for @N placement / depth consistency.

fn validate_pipe_assignments(
    stmts: &[Stmt],
    ports: &[PipePortInfoLocal],
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
    warnings: &mut Vec<crate::diagnostics::CompileWarning>,
) {
    for s in stmts {
        validate_pipe_assign_stmt(s, ports, pipe_depths, errors, warnings);
    }
}

fn validate_pipe_assign_stmt(
    stmt: &Stmt,
    ports: &[PipePortInfoLocal],
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
    warnings: &mut Vec<crate::diagnostics::CompileWarning>,
) {
    match stmt {
        Stmt::Assign(a) => {
            // Inspect the target: LatencyAt(Ident, N) or bare Ident into a
            // pipe_reg port. Validate per the error matrix.
            let (target_name, latency_opt) = match &a.target.kind {
                ExprKind::LatencyAt(inner, n) => match &inner.kind {
                    ExprKind::Ident(name) => (Some(name.clone()), Some(*n)),
                    _ => (None, None),
                },
                ExprKind::Ident(name) => (Some(name.clone()), None),
                _ => (None, None),
            };
            let pp = target_name
                .as_ref()
                .and_then(|n| ports.iter().find(|p| &p.name == n));
            // `doc/proposal_pipelined_operators.md` §2: the call is
            // authoritative for latency, the `@N` tap is a consistency
            // check against it. Checked here — *before* the cascade
            // rewrite below, which would otherwise strip the `@N` off the
            // target, leaving nothing left for typecheck to compare
            // against — and independent of whether the target happens to
            // be a recognized `pipe_reg` port (a bare `reg` target is
            // equally required to be tapped: a 1-cycle flop can't hold an
            // N>1-cycle-delayed value).
            if let ExprKind::PipelinedCall(name, _, call_stages) = &a.value.kind {
                let declared_depth = pp.map(|p| p.latency);
                match (latency_opt, declared_depth) {
                    (Some(n), _) if n != *call_stages => {
                        errors.push(CompileError::general(
                            &format!("latency-{call_stages} result bound at @{n}"),
                            a.span,
                        ));
                    }
                    (Some(_), Some(depth)) if depth != *call_stages => {
                        // Tapped at the right N, but the port itself is
                        // declared a different depth — still a mismatch.
                        errors.push(CompileError::general(
                            &format!("latency-{call_stages} result bound at @{depth}"),
                            a.span,
                        ));
                    }
                    (None, _) => {
                        errors.push(CompileError::general(
                            &format!(
                                "`{name}<pipelined, {call_stages}>(...)` produces a latency-{call_stages} \
                                 result — it must be bound via a tapped target, e.g. \
                                 `target@{call_stages} <= {name}<pipelined, {call_stages}>(...)`"
                            ),
                            a.span,
                        ));
                    }
                    _ => {}
                }
            }
            let Some(pp) = pp else {
                return;
            };
            match latency_opt {
                Some(n) if n != pp.latency => {
                    errors.push(CompileError::general(
                        &format!(
                            "`{name}@{n}` exceeds declared latency {depth} — write `{name}@{depth} <= ...` for this port",
                            name = pp.name, n = n, depth = pp.latency
                        ),
                        a.span,
                    ));
                }
                None if pp.latency > 1 => {
                    errors.push(CompileError::general(
                        &format!(
                            "assignment to pipe_reg port `{name}` is ambiguous — write `{name}@{depth} <= ...` to state the latency",
                            name = pp.name, depth = pp.latency
                        ),
                        a.span,
                    ));
                }
                _ => {}
            }
            // Optional warning (doc/proposal_pipelined_operators.md §2,
            // "No silent retiming of arbitrary exprs"): a bare comb call
            // delayed via a pipe_reg tap is legal (unchanged delay-line
            // semantics — the flop cascade just holds the *result*) but is
            // almost always a "meant the pipelined variant" mistake. Only
            // fires for tapped writes into an *actual* pipe_reg port
            // (`pp` is `Some` here), and only when the callee is a
            // registered pipelined operator.
            if let ExprKind::FunctionCall(name, _) = &a.value.kind {
                if pp.latency >= 1
                    && crate::pipelined_ops::BUILTIN_REGISTRY
                        .iter()
                        .any(|e| e.operator == *name)
                {
                    warnings.push(crate::diagnostics::CompileWarning {
                        message: format!(
                            "comb `{name}` delayed {depth} cycles via `@{depth}`; did you mean \
                             `{name}<pipelined, {depth}>(...)`?",
                            depth = pp.latency
                        ),
                        span: a.span,
                    });
                }
            }
            // RHS `q@K` for pipe_reg `q`: K must be 0..=N.
            validate_rhs_latency_with_depths(&a.value, pipe_depths, errors);
        }
        Stmt::IfElse(ie) => {
            validate_pipe_assignments(&ie.then_stmts, ports, pipe_depths, errors, warnings);
            validate_pipe_assignments(&ie.else_stmts, ports, pipe_depths, errors, warnings);
        }
        Stmt::Match(m) => {
            for arm in &m.arms {
                validate_pipe_assignments(&arm.body, ports, pipe_depths, errors, warnings);
            }
        }
        Stmt::For(f) => validate_pipe_assignments(&f.body, ports, pipe_depths, errors, warnings),
        Stmt::Init(ib) => validate_pipe_assignments(&ib.body, ports, pipe_depths, errors, warnings),
        _ => {}
    }
}

fn validate_rhs_latency_with_depths(
    e: &Expr,
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
) {
    // RHS `q@K` reads the K-th tap of pipe_reg `q` (K=0 = source comb,
    // K=N = final output = bare `q`). Validate K ≤ N when the base is
    // a known pipe_reg name; if the base isn't a pipe_reg, reject @K
    // for K > 0 (legacy "no @ on plain regs" rule).
    match &e.kind {
        ExprKind::LatencyAt(inner, n) => {
            if let ExprKind::Ident(name) = &inner.kind {
                match pipe_depths.get(name) {
                    Some(depth) if *n > *depth => {
                        errors.push(CompileError::general(
                            &format!("`{name}@{n}` exceeds pipe_reg depth {depth} (valid taps: 0..={depth})"),
                            e.span,
                        ));
                    }
                    None if *n != 0 => {
                        errors.push(CompileError::general(
                            &format!("`{name}@{n}` — `{name}` is not a pipe_reg, only `@0` is allowed on plain signals"),
                            e.span,
                        ));
                    }
                    _ => {}
                }
            }
            validate_rhs_latency_with_depths(inner, pipe_depths, errors);
        }
        ExprKind::Binary(_, l, r) => {
            validate_rhs_latency_with_depths(l, pipe_depths, errors);
            validate_rhs_latency_with_depths(r, pipe_depths, errors);
        }
        ExprKind::Unary(_, x) => validate_rhs_latency_with_depths(x, pipe_depths, errors),
        ExprKind::Ternary(c, t, e2) => {
            validate_rhs_latency_with_depths(c, pipe_depths, errors);
            validate_rhs_latency_with_depths(t, pipe_depths, errors);
            validate_rhs_latency_with_depths(e2, pipe_depths, errors);
        }
        ExprKind::FieldAccess(b, _) => validate_rhs_latency_with_depths(b, pipe_depths, errors),
        ExprKind::Index(b, i) => {
            validate_rhs_latency_with_depths(b, pipe_depths, errors);
            validate_rhs_latency_with_depths(i, pipe_depths, errors);
        }
        ExprKind::BitSlice(b, h, l) => {
            validate_rhs_latency_with_depths(b, pipe_depths, errors);
            validate_rhs_latency_with_depths(h, pipe_depths, errors);
            validate_rhs_latency_with_depths(l, pipe_depths, errors);
        }
        ExprKind::MethodCall(b, _, args) => {
            validate_rhs_latency_with_depths(b, pipe_depths, errors);
            for a in args {
                validate_rhs_latency_with_depths(a, pipe_depths, errors);
            }
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args {
                validate_rhs_latency_with_depths(a, pipe_depths, errors);
            }
        }
        _ => {}
    }
}

fn validate_comb_pipe_refs(
    stmts: &[Stmt],
    pipe_ports: &[PipePortInfoLocal],
    all_ports: &[PortDecl],
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
) {
    for s in stmts {
        match s {
            Stmt::Assign(a) => {
                // LHS @0 on a plain (non-pipe_reg) port is an error.
                if let ExprKind::LatencyAt(inner, n) = &a.target.kind {
                    if let ExprKind::Ident(name) = &inner.kind {
                        let is_pipe = pipe_ports.iter().any(|p| &p.name == name);
                        if !is_pipe && all_ports.iter().any(|p| p.name.name == *name) {
                            errors.push(CompileError::general(
                                &format!("`{name}@{n}` is only valid on pipe_reg<T, N> ports; drop the `@{n}` or change the port type"),
                                a.target.span,
                            ));
                        }
                    }
                }
                validate_rhs_latency_with_depths(&a.value, pipe_depths, errors);
            }
            Stmt::IfElse(ie) => {
                validate_comb_pipe_refs(&ie.then_stmts, pipe_ports, all_ports, pipe_depths, errors);
                validate_comb_pipe_refs(&ie.else_stmts, pipe_ports, all_ports, pipe_depths, errors);
            }
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("seq-only Stmt variant inside comb-context walker")
            }
            Stmt::Match(_) | Stmt::For(_) | Stmt::Log(_) => {}
        }
    }
}

fn rewrite_seq_stmts(stmts: Vec<Stmt>, pipes: &[PipePortInfoLocal]) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = Vec::with_capacity(stmts.len());
    for s in stmts {
        out.extend(rewrite_seq_stmt(s, pipes));
    }
    out
}

fn rewrite_seq_stmt(stmt: Stmt, pipes: &[PipePortInfoLocal]) -> Vec<Stmt> {
    match stmt {
        Stmt::Assign(a) => {
            let (root, latency, span) = match &a.target.kind {
                ExprKind::LatencyAt(inner, n) => match &inner.kind {
                    ExprKind::Ident(name) => (name.clone(), *n, a.span),
                    _ => return vec![Stmt::Assign(a)],
                },
                _ => return vec![Stmt::Assign(a)],
            };
            let Some(pp) = pipes.iter().find(|p| p.name == root) else {
                return vec![Stmt::Assign(a)];
            };
            if latency != pp.latency {
                // Typecheck should have rejected this; leave it and let
                // downstream errors surface.
                return vec![Stmt::Assign(a)];
            }
            // Build the cascade: stg1 <= expr; stg2 <= stg1; ...; q <= stg{N-1};
            let value = a.value;
            let n = pp.latency;
            let mut out: Vec<Stmt> = Vec::with_capacity(n as usize);
            // stg1 <= value
            out.push(Stmt::Assign(Assign {
                target: Expr::new(ExprKind::Ident(format!("{}_stg1", pp.name)), span),
                value,
                span,
            }));
            // stg{k} <= stg{k-1} for k = 2..N-1
            for k in 2..n {
                out.push(Stmt::Assign(Assign {
                    target: Expr::new(ExprKind::Ident(format!("{}_stg{}", pp.name, k)), span),
                    value: Expr::new(ExprKind::Ident(format!("{}_stg{}", pp.name, k - 1)), span),
                    span,
                }));
            }
            // q <= stg{N-1}
            out.push(Stmt::Assign(Assign {
                target: Expr::new(ExprKind::Ident(pp.name.clone()), span),
                value: Expr::new(ExprKind::Ident(format!("{}_stg{}", pp.name, n - 1)), span),
                span,
            }));
            out
        }
        Stmt::IfElse(mut ie) => {
            ie.then_stmts = rewrite_seq_stmts_pp(std::mem::take(&mut ie.then_stmts), pipes);
            ie.else_stmts = rewrite_seq_stmts_pp(std::mem::take(&mut ie.else_stmts), pipes);
            vec![Stmt::IfElse(ie)]
        }
        Stmt::Match(mut m) => {
            for arm in &mut m.arms {
                arm.body = rewrite_seq_stmts_pp(std::mem::take(&mut arm.body), pipes);
            }
            vec![Stmt::Match(m)]
        }
        Stmt::For(mut f) => {
            f.body = rewrite_seq_stmts_pp(std::mem::take(&mut f.body), pipes);
            vec![Stmt::For(f)]
        }
        Stmt::Init(mut ib) => {
            ib.body = rewrite_seq_stmts_pp(std::mem::take(&mut ib.body), pipes);
            vec![Stmt::Init(ib)]
        }
        other => vec![other],
    }
}

fn rewrite_seq_stmts_pp(stmts: Vec<Stmt>, pipes: &[PipePortInfoLocal]) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = Vec::with_capacity(stmts.len());
    for s in stmts {
        out.extend(rewrite_seq_stmt(s, pipes));
    }
    out
}

// ── credit_channel method-dispatch (PR #3b-v-β) ─────────────────────────────
//
// Rewrites `port.ch.valid` / `port.ch.data` / `port.ch.can_send` expressions,
// where `port` is a bus port declaring `credit_channel ch`, into
// `ExprKind::SynthIdent(__<port>_<ch>_<member>, ty)` pointing at the SV wires
// emitted by codegen boilerplate in PR #3b-ii / #3b-iii.
//
// Role-gated: `can_send` is valid only on the sender side (initiator of a
// `send` channel, target of a `receive` channel); `valid` and `data` are
// valid only on the receiver side. Mismatches are left as untransformed
// nested FieldAccess and fall through to normal bus-member resolution.

pub fn lower_credit_channel_dispatch(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    use std::collections::HashMap;
    let mut bus_ccs: HashMap<String, Vec<CreditChannelMeta>> = HashMap::new();
    for item in &ast.items {
        match item {
            Item::Bus(b) => {
                if !b.credit_channels.is_empty() {
                    bus_ccs.insert(b.name.name.clone(), b.credit_channels.clone());
                }
            }
            Item::Package(pkg) => {
                for b in &pkg.buses {
                    if !b.credit_channels.is_empty() {
                        bus_ccs.insert(b.name.name.clone(), b.credit_channels.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if bus_ccs.is_empty() {
        return Ok(ast);
    }
    let mut items: Vec<Item> = Vec::with_capacity(ast.items.len());
    let mut errors: Vec<CompileError> = Vec::new();
    for item in ast.items {
        match item {
            Item::Module(mut m) => {
                let port_buses: HashMap<String, (String, BusPerspective)> = m
                    .ports
                    .iter()
                    .filter_map(|p| {
                        p.bus_info.as_ref().map(|bi| {
                            (
                                p.name.name.clone(),
                                (bi.bus_name.name.clone(), bi.perspective),
                            )
                        })
                    })
                    .collect();
                if port_buses.values().any(|(b, _)| bus_ccs.contains_key(b)) {
                    let ctx = CcDispatchCtx {
                        bus_ccs: &bus_ccs,
                        port_buses: &port_buses,
                    };
                    for bi in &mut m.body {
                        rewrite_body_item_cc(bi, &ctx, &mut errors);
                    }
                }
                items.push(Item::Module(m));
            }
            other => items.push(other),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(SourceFile {
        items,
        inner_doc: None,
        frontmatter: None,
    })
}

struct CcDispatchCtx<'a> {
    bus_ccs: &'a std::collections::HashMap<String, Vec<CreditChannelMeta>>,
    port_buses: &'a std::collections::HashMap<String, (String, BusPerspective)>,
}

fn rewrite_body_item_cc(
    bi: &mut ModuleBodyItem,
    ctx: &CcDispatchCtx,
    errors: &mut Vec<CompileError>,
) {
    match bi {
        ModuleBodyItem::CombBlock(cb) => {
            for s in &mut cb.stmts {
                rewrite_stmt_cc(s, ctx, errors);
            }
        }
        ModuleBodyItem::RegBlock(rb) => {
            for s in &mut rb.stmts {
                rewrite_stmt_cc(s, ctx, errors);
            }
        }
        ModuleBodyItem::LetBinding(l) => {
            rewrite_expr_cc(&mut l.value, ctx, errors);
        }
        _ => {}
    }
}

/// Rewrite credit-channel field access (`port.ch.data`, `port.ch.valid`,
/// `port.ch.can_send`) into the synthetic identifier the SV codegen emits
/// (`__{port}_{ch}_{suffix}`). Walks every expression position in `Stmt`
/// recursively. The reg/comb/pipeline-stage block context doesn't affect
/// the rewrite — the same field access is invalid for the same reason in
/// every block, and the synthesized identifier is the same.
///
/// History: pre-unification this was two near-identical functions
/// (`rewrite_reg_stmt_cc`, `rewrite_comb_stmt_cc`) — but the seq variant
/// silently skipped scrutinees of `Stmt::Match`, the bodies of
/// `Stmt::Init`, and the cond/body of `Stmt::WaitUntil` / `DoUntil`,
/// leaving CC field accesses inside those positions for the resolver to
/// trip over with a misleading "bus has no signal X" error. Unifying
/// (and exhaustively covering all expression positions) closes that gap.
fn rewrite_stmt_cc(s: &mut Stmt, ctx: &CcDispatchCtx, errors: &mut Vec<CompileError>) {
    match s {
        Stmt::Assign(a) => {
            rewrite_expr_cc(&mut a.target, ctx, errors);
            rewrite_expr_cc(&mut a.value, ctx, errors);
        }
        Stmt::IfElse(ie) => {
            rewrite_expr_cc(&mut ie.cond, ctx, errors);
            for s in &mut ie.then_stmts {
                rewrite_stmt_cc(s, ctx, errors);
            }
            for s in &mut ie.else_stmts {
                rewrite_stmt_cc(s, ctx, errors);
            }
        }
        Stmt::For(fl) => {
            for s in &mut fl.body {
                rewrite_stmt_cc(s, ctx, errors);
            }
        }
        Stmt::Match(m) => {
            rewrite_expr_cc(&mut m.scrutinee, ctx, errors);
            for arm in &mut m.arms {
                for s in &mut arm.body {
                    rewrite_stmt_cc(s, ctx, errors);
                }
            }
        }
        Stmt::Init(ib) => {
            for s in &mut ib.body {
                rewrite_stmt_cc(s, ctx, errors);
            }
        }
        Stmt::WaitUntil(expr, _) => {
            rewrite_expr_cc(expr, ctx, errors);
        }
        Stmt::DoUntil { body, cond, .. } => {
            for s in body {
                rewrite_stmt_cc(s, ctx, errors);
            }
            rewrite_expr_cc(cond, ctx, errors);
        }
        Stmt::Log(l) => {
            for arg in &mut l.args {
                rewrite_expr_cc(arg, ctx, errors);
            }
        }
    }
}

fn rewrite_expr_cc(e: &mut Expr, ctx: &CcDispatchCtx, errors: &mut Vec<CompileError>) {
    match &mut e.kind {
        ExprKind::Binary(_, l, r) => {
            rewrite_expr_cc(l, ctx, errors);
            rewrite_expr_cc(r, ctx, errors);
        }
        ExprKind::Unary(_, x)
        | ExprKind::Cast(x, _)
        | ExprKind::Clog2(x)
        | ExprKind::Onehot(x)
        | ExprKind::Signed(x)
        | ExprKind::Unsigned(x)
        | ExprKind::LatencyAt(x, _)
        | ExprKind::SvaNext(_, x) => {
            rewrite_expr_cc(x, ctx, errors);
        }
        ExprKind::Index(b, i) => {
            rewrite_expr_cc(b, ctx, errors);
            rewrite_expr_cc(i, ctx, errors);
        }
        ExprKind::BitSlice(b, hi, lo) => {
            rewrite_expr_cc(b, ctx, errors);
            rewrite_expr_cc(hi, ctx, errors);
            rewrite_expr_cc(lo, ctx, errors);
        }
        ExprKind::PartSelect(b, s, w, _) => {
            rewrite_expr_cc(b, ctx, errors);
            rewrite_expr_cc(s, ctx, errors);
            rewrite_expr_cc(w, ctx, errors);
        }
        ExprKind::Ternary(c, t, el) => {
            rewrite_expr_cc(c, ctx, errors);
            rewrite_expr_cc(t, ctx, errors);
            rewrite_expr_cc(el, ctx, errors);
        }
        ExprKind::Concat(xs) | ExprKind::FunctionCall(_, xs) => {
            for x in xs {
                rewrite_expr_cc(x, ctx, errors);
            }
        }
        ExprKind::Repeat(n, x) => {
            rewrite_expr_cc(n, ctx, errors);
            rewrite_expr_cc(x, ctx, errors);
        }
        ExprKind::MethodCall(recv, _, args) => {
            rewrite_expr_cc(recv, ctx, errors);
            for a in args {
                rewrite_expr_cc(a, ctx, errors);
            }
        }
        ExprKind::FieldAccess(base, _) => {
            rewrite_expr_cc(base, ctx, errors);
        }
        ExprKind::StructLiteral(_, fields) => {
            for fi in fields {
                rewrite_expr_cc(&mut fi.value, ctx, errors);
            }
        }
        _ => {}
    }
    // Reject the underscored credit_channel access form (`port.<ch>_send_valid`,
    // `port.<ch>_send_data`, `port.<ch>_credit_return`). Tell the user to use
    // the dotted method form instead.
    if let ExprKind::FieldAccess(base, member) = &e.kind {
        if let ExprKind::Ident(port) = &base.kind {
            if let Some((bus_name, _)) = ctx.port_buses.get(port) {
                if let Some(ccs) = ctx.bus_ccs.get(bus_name) {
                    for cc in ccs {
                        let ch = &cc.name.name;
                        let m = &member.name;
                        let suggest = if m == &format!("{ch}_send_valid")
                            || m == &format!("{ch}_send_data")
                        {
                            Some(format!("{port}.{ch}.send(...) or {port}.{ch}.no_send()"))
                        } else if m == &format!("{ch}_credit_return") {
                            Some(format!("{port}.{ch}.pop() or {port}.{ch}.no_pop()"))
                        } else {
                            None
                        };
                        if let Some(s) = suggest {
                            errors.push(CompileError::general(
                                &format!(
                                    "underscored credit_channel access `{port}.{m}` is no longer accepted — use the dotted method form: {s}"
                                ),
                                e.span,
                            ));
                            break;
                        }
                    }
                }
            }
        }
    }
    if let ExprKind::FieldAccess(base, member) = &e.kind {
        if let ExprKind::FieldAccess(inner, ch) = &base.kind {
            if let ExprKind::Ident(port) = &inner.kind {
                if let Some((bus_name, perspective)) = ctx.port_buses.get(port) {
                    if let Some(ccs) = ctx.bus_ccs.get(bus_name) {
                        if let Some(cc) = ccs.iter().find(|c| c.name.name == ch.name) {
                            let is_sender = matches!(
                                (cc.role_dir, perspective),
                                (Direction::Out, BusPerspective::Initiator)
                                    | (Direction::In, BusPerspective::Target)
                            );
                            let synth = match member.name.as_str() {
                                "can_send" if is_sender => Some((TypeExpr::Bool, "can_send")),
                                "valid" if !is_sender => Some((TypeExpr::Bool, "valid")),
                                "data" if !is_sender => cc
                                    .params
                                    .iter()
                                    .find(|p| p.name.name == "T")
                                    .and_then(|p| match &p.kind {
                                        ParamKind::Type(te) => Some(te.clone()),
                                        _ => None,
                                    })
                                    .map(|ty| (ty, "data")),
                                _ => None,
                            };
                            if let Some((ty, suffix)) = synth {
                                let name = format!("__{port}_{}_{suffix}", ch.name);
                                e.kind = ExprKind::SynthIdent(name, ty);
                            } else if matches!(
                                member.name.as_str(),
                                "send_valid" | "send_data" | "credit_return"
                            ) {
                                // Dotted access to raw wire (escape hatch for
                                // direct conditional drives that no_send/no_pop
                                // can't express). Rewrite to the flat bus signal
                                // name so the resolver finds it via the normal
                                // bus-member path.
                                let flat = format!("{}_{}", ch.name, member.name);
                                let new_member = Ident::new(flat, member.span);
                                e.kind = ExprKind::FieldAccess((*inner).clone(), new_member);
                            }
                        }
                    }
                }
            }
        }
    }
}
