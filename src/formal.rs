//! `arch formal` — direct SMT-LIB2 bounded model checking.
//!
//! Lowers a single flat `module` from the post-elaboration AST into an
//! unrolled SMT-LIB2 formula (QF_BV), then shells out to a bit-vector solver
//! (z3 / boolector / bitwuzla) to prove or refute each `assert` / `cover`.
//!
//! Design notes:
//! - Scalars only (UInt/SInt/Bool/Bit). Vec / struct / enum port types error out.
//! - No sub-instances. Multi-clock and thread-bearing designs error out.
//! - Signal `foo` at cycle `t` is named `foo_t`. Lets are inlined.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::ast::*;
use crate::diagnostics::CompileError;
use crate::lexer::Span;
use crate::resolve::SymbolTable;

// ── Public API ───────────────────────────────────────────────────────────────

pub struct FormalArgs {
    pub top: Option<String>,
    pub bound: u32,
    pub solver: String,
    pub emit_smt: Option<PathBuf>,
    pub timeout: u32,
}

#[derive(Debug, Clone)]
pub enum PropertyStatus {
    Proved(u32),          // bound
    Refuted(u32),         // cycle
    Hit(u32),             // cycle
    NotReached(u32),      // bound
    Inconclusive(String), // reason
}

#[derive(Debug, Clone)]
pub struct PropertyResult {
    pub name: String,
    pub kind: AssertKind,
    pub status: PropertyStatus,
    pub counterexample: Option<String>,
}

pub struct FormalReport {
    pub results: Vec<PropertyResult>,
}

impl FormalReport {
    pub fn exit_code(&self) -> i32 {
        let mut any_bad = false;
        let mut any_incon = false;
        for r in &self.results {
            match &r.status {
                PropertyStatus::Proved(_) | PropertyStatus::Hit(_) => {}
                PropertyStatus::Refuted(_) | PropertyStatus::NotReached(_) => any_bad = true,
                PropertyStatus::Inconclusive(_) => any_incon = true,
            }
        }
        if any_bad { 1 } else if any_incon { 2 } else { 0 }
    }
}

pub fn run(
    ast: &SourceFile,
    symbols: &SymbolTable,
    args: &FormalArgs,
) -> Result<FormalReport, CompileError> {
    // 1. Pick the top module
    let module = select_top(ast, args.top.as_deref())?;

    // 2. Flatten sub-instances into a synthetic flat module. For designs
    //    without any sub-inst, this is a no-op clone. See
    //    doc/plan_hierarchical_formal.md for the design.
    let flat_module: ModuleDecl;
    let mut carried_credit_sites: Vec<CarriedCreditSite> = Vec::new();
    let encode_module: &ModuleDecl = if module.body.iter().any(|b| matches!(b, ModuleBodyItem::Inst(_))) {
        let out = flatten_for_formal(ast, module, symbols)?;
        flat_module = out.module;
        carried_credit_sites = out.carried_sites;
        &flat_module
    } else {
        module
    };

    // 3. Build encoder state
    let mut ctx = FormalCtx::new(encode_module, symbols);
    ctx.carried_credit_sites = carried_credit_sites;
    ctx.preprocess()?;

    // 4. Emit SMT-LIB2 (header + declarations + transitions + comb)
    let base = ctx.emit_base(args.bound)?;

    // 4. Optionally dump
    if let Some(path) = &args.emit_smt {
        std::fs::write(path, &base).map_err(|e| CompileError::general(
            &format!("failed to write --emit-smt output: {e}"),
            module.span,
        ))?;
    }

    // 5. For each assert/cover, run one (push)/(check-sat)/(pop) scope
    let mut results = Vec::new();
    for prop in ctx.properties.clone().iter() {
        let res = ctx.run_property(prop, &base, args)?;
        results.push(res);
    }

    render_report(&results);

    Ok(FormalReport { results })
}

// ── Hierarchical flattening (PR-hf1b) ────────────────────────────────────────
//
// Bottom-up inline each sub-inst of the top module into a synthetic flat
// ModuleDecl. The existing FormalCtx then encodes the flat module as if
// it were written hand-flat.
//
// v1 scope (matching plan doc):
//   - Single level of nesting (no inst-inside-inst).
//   - Scalar ports only.
//   - Sub-module body may contain ONLY `let` bindings (pure comb).
//     Regs/comb/seq/assert/cover inside sub = unsupported in this slice
//     (PR-hf2 extends coverage).
//   - Same-clock hierarchy (connections must bind sub's clk/rst to top's).
//
// Name mangling:
//   - Sub's local let `x` becomes `<inst>_x` in the flat body.
//   - Sub's port references resolve to the parent signal expression
//     from the inst's connection list.
//   - Sub's ports themselves are NOT carried into the flat module (they
//     dissolve into connection bindings).

/// One credit_channel site that flatten_for_formal carried across an
/// inst boundary into the flat module. The `port_name` is the parent-
/// side connection identifier (e.g. `chwire` from `s <- chwire`); it
/// becomes the prefix in lifted-state names like `__chwire_<ch>_credit`.
/// FormalCtx consumes these alongside its own bus-port walk.
#[derive(Debug, Clone)]
#[allow(dead_code)] // consumed in next item-5 sub-step (registration)
struct CarriedCreditSite {
    port_name: String,
    meta: CreditChannelMeta,
    is_sender: bool,
}

/// Return type of `flatten_for_formal`: the flattened module plus
/// credit_channel sites carried in from sub-instances. Sites whose
/// `port_name` collides (one sender + one receiver from two insts
/// sharing the same parent connection name) compose into the
/// occupancy invariant.
struct FlattenOutput {
    module: ModuleDecl,
    carried_sites: Vec<CarriedCreditSite>,
}

fn flatten_for_formal(
    ast: &SourceFile,
    top: &ModuleDecl,
    symbols: &SymbolTable,
) -> Result<FlattenOutput, CompileError> {
    use std::collections::HashMap;

    let mut flat = top.clone();
    let mut new_body: Vec<ModuleBodyItem> = Vec::with_capacity(flat.body.len());
    let mut carried_sites: Vec<CarriedCreditSite> = Vec::new();

    for item in std::mem::take(&mut flat.body) {
        match item {
            ModuleBodyItem::Inst(inst) => {
                let sub = lookup_module(ast, &inst.module_name.name).ok_or_else(|| {
                    CompileError::general(
                        &format!(
                            "hierarchical formal: sub-module `{}` not found in source",
                            inst.module_name.name
                        ),
                        inst.module_name.span,
                    )
                })?;

                validate_sub_for_formal(sub)?;

                // Port map: sub-port name → parent-side Expr. Connections
                // pair port names with signal expressions regardless of
                // direction (ConnectDir just documents intent; at the
                // formal flattening level, we substitute Ident(port) →
                // signal_expr everywhere in the sub's body).
                let mut port_map: HashMap<String, Expr> = HashMap::new();
                for c in &inst.connections {
                    port_map.insert(c.port_name.name.clone(), c.signal.clone());
                }

                // PR-hf4 item 5: collect credit_channel sites carried in
                // through bus ports + build a sub-port → parent-name remap
                // for SynthIdent rewriting. Bus port connections must use
                // a simple Ident as the parent-side name in v1; complex
                // expressions are rejected because the synthesized state
                // names need a single string prefix.
                let mut bus_remap: HashMap<String, String> = HashMap::new();
                for sp in &sub.ports {
                    let Some(bi) = &sp.bus_info else { continue; };
                    let Some(parent_expr) = port_map.get(&sp.name.name) else { continue; };
                    let parent_name = match &parent_expr.kind {
                        ExprKind::Ident(n) => n.clone(),
                        _ => {
                            return Err(CompileError::general(
                                &format!(
                                    "hierarchical formal v1: inst `{}.{}` bus-port connection must be a simple identifier (got a complex expression); refactor the parent connection to a named wire",
                                    inst.name.name, sp.name.name,
                                ),
                                inst.span,
                            ));
                        }
                    };
                    bus_remap.insert(sp.name.name.clone(), parent_name.clone());
                    let Some((crate::resolve::Symbol::Bus(bus_info), _)) =
                        symbols.globals.get(&bi.bus_name.name)
                    else {
                        return Err(CompileError::general(
                            &format!(
                                "hierarchical formal: bus `{}` referenced by inst `{}.{}` not found in symbol table",
                                bi.bus_name.name, inst.name.name, sp.name.name,
                            ),
                            sp.span,
                        ));
                    };
                    for cc in &bus_info.credit_channels {
                        let is_sender = matches!(
                            (cc.role_dir, bi.perspective),
                            (Direction::Out, crate::ast::BusPerspective::Initiator)
                                | (Direction::In, crate::ast::BusPerspective::Target)
                        );
                        carried_sites.push(CarriedCreditSite {
                            port_name: parent_name.clone(),
                            meta: cc.clone(),
                            is_sender,
                        });
                    }
                }

                // Enforce: every sub port must have a connection.
                for p in &sub.ports {
                    if !port_map.contains_key(&p.name.name) {
                        return Err(CompileError::general(
                            &format!(
                                "hierarchical formal: inst `{}` of `{}` leaves port `{}` unconnected (required in v1)",
                                inst.name.name, inst.module_name.name, p.name.name,
                            ),
                            inst.span,
                        ));
                    }
                }

                // Collect local (non-port) names in the sub that need
                // prefixing. Locals = let-bound names + RegDecl names
                // whose name isn't also a port.
                let port_names: std::collections::HashSet<String> =
                    sub.ports.iter().map(|p| p.name.name.clone()).collect();
                let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
                for bi in &sub.body {
                    match bi {
                        ModuleBodyItem::LetBinding(lb) => {
                            if !port_names.contains(&lb.name.name) {
                                locals.insert(lb.name.name.clone());
                            }
                        }
                        ModuleBodyItem::RegDecl(rd) => {
                            if !port_names.contains(&rd.name.name) {
                                locals.insert(rd.name.name.clone());
                            }
                        }
                        _ => {}
                    }
                }

                let prefix = format!("{}_", inst.name.name);
                let new_body_start = new_body.len();
                for bi in &sub.body {
                    match bi {
                        ModuleBodyItem::LetBinding(lb) => {
                            // Decide the rewritten name of the let itself.
                            // If it shares a name with a sub-port, it
                            // IS the driver for that port — its rewritten
                            // target is whatever the parent side connects
                            // to that port.
                            let rewritten_value = subst_expr_for_formal(
                                &lb.value, &port_map, &locals, &prefix,
                            );
                            if port_names.contains(&lb.name.name) {
                                // Port-driving let. Emit
                                //   `let <parent_side_name> = <value>;`
                                // ONLY if the parent side expression is a
                                // simple Ident. Otherwise (e.g., a complex
                                // bit-slice), emit a comb assignment to
                                // the parent signal.
                                let parent_expr = port_map.get(&lb.name.name).unwrap().clone();
                                match &parent_expr.kind {
                                    ExprKind::Ident(parent_name) => {
                                        new_body.push(ModuleBodyItem::LetBinding(LetBinding {
                                            name: Ident::new(parent_name.clone(), lb.name.span),
                                            ty: lb.ty.clone(),
                                            value: rewritten_value,
                                            span: lb.span,
                                            destructure_fields: Vec::new(),
                                        }));
                                    }
                                    _ => {
                                        return Err(CompileError::general(
                                            &format!(
                                                "hierarchical formal: inst `{}.{}` port connection must be a simple identifier in v1 (got a complex expression); refactor the parent to a named wire",
                                                inst.name.name, lb.name.name,
                                            ),
                                            inst.span,
                                        ));
                                    }
                                }
                            } else {
                                // Internal let — prefix its name.
                                new_body.push(ModuleBodyItem::LetBinding(LetBinding {
                                    name: Ident::new(format!("{prefix}{}", lb.name.name), lb.name.span),
                                    ty: lb.ty.clone(),
                                    value: rewritten_value,
                                    span: lb.span,
                                    destructure_fields: Vec::new(),
                                }));
                            }
                        }
                        ModuleBodyItem::CombBlock(cb) => {
                            let new_stmts: Vec<CombStmt> = cb.stmts.iter()
                                .map(|s| subst_comb_stmt_for_formal(
                                    s, &port_map, &locals, &prefix,
                                ))
                                .collect::<Result<_, _>>()?;
                            new_body.push(ModuleBodyItem::CombBlock(CombBlock {
                                stmts: new_stmts,
                                span: cb.span,
                            }));
                        }
                        ModuleBodyItem::RegDecl(rd) => {
                            // Prefix the reg name (regs don't share names
                            // with ports — that'd be a driver conflict in
                            // the sub-module itself).
                            let new_init = rd.init.as_ref()
                                .map(|e| subst_expr_for_formal(e, &port_map, &locals, &prefix));
                            let new_reset = subst_reg_reset_for_formal(
                                &rd.reset, &port_map, &locals, &prefix,
                            );
                            new_body.push(ModuleBodyItem::RegDecl(RegDecl {
                                name: Ident::new(format!("{prefix}{}", rd.name.name), rd.name.span),
                                ty: rd.ty.clone(),
                                init: new_init,
                                reset: new_reset,
                                guard: rd.guard.clone(),
                                span: rd.span,
                            }));
                        }
                        ModuleBodyItem::RegBlock(rb) => {
                            // Clock ident: substitute via port_map (sub's
                            // `clk` port binds to parent's clock via the
                            // inst connection).
                            let clock = resolve_port_ident_for_formal(
                                &rb.clock, &port_map, &inst.name.name,
                            )?;
                            let new_stmts: Vec<Stmt> = rb.stmts.iter()
                                .map(|s| subst_stmt_for_formal(
                                    s, &port_map, &locals, &prefix,
                                ))
                                .collect::<Result<_, _>>()?;
                            new_body.push(ModuleBodyItem::RegBlock(RegBlock {
                                clock,
                                clock_edge: rb.clock_edge,
                                stmts: new_stmts,
                                span: rb.span,
                            }));
                        }
                        _ => unreachable!("validate_sub_for_formal rejects other items"),
                    }
                }

                // Rewrite SynthIdent strings in items appended for this
                // inst, replacing each sub-bus-port-name prefix with the
                // parent-side connection name. This carries credit_channel
                // synthesized references (e.g. `s_data_send_valid`,
                // `__s_data_credit`) across the inst boundary so the
                // flat module's lifted state and SynthIdent lookups all
                // key on the parent name (`chwire_data_*`).
                if !bus_remap.is_empty() {
                    for item in &mut new_body[new_body_start..] {
                        rewrite_synth_idents_in_body_item(item, &bus_remap);
                    }
                }
            }
            other => new_body.push(other),
        }
    }

    flat.body = new_body;
    Ok(FlattenOutput { module: flat, carried_sites })
}

/// Walk a ModuleBodyItem and rewrite SynthIdent prefixes per `remap`
/// (sub-bus-port-name → parent-side-connection-name). Used by
/// `flatten_for_formal` to carry credit_channel synthesized references
/// across inst boundaries — e.g. `s_data_send_valid` (from sub-port `s`)
/// becomes `chwire_data_send_valid` (parent connection `chwire`).
fn rewrite_synth_idents_in_body_item(
    item: &mut ModuleBodyItem,
    remap: &std::collections::HashMap<String, String>,
) {
    match item {
        ModuleBodyItem::LetBinding(lb) => rewrite_synth_idents_in_expr(&mut lb.value, remap),
        ModuleBodyItem::CombBlock(cb) => {
            for s in &mut cb.stmts { rewrite_synth_idents_in_comb_stmt(s, remap); }
        }
        ModuleBodyItem::RegDecl(rd) => {
            if let Some(init) = &mut rd.init { rewrite_synth_idents_in_expr(init, remap); }
            match &mut rd.reset {
                RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) => {
                    rewrite_synth_idents_in_expr(val, remap);
                }
                RegReset::None => {}
            }
        }
        ModuleBodyItem::RegBlock(rb) => {
            for s in &mut rb.stmts { rewrite_synth_idents_in_stmt(s, remap); }
        }
        _ => {}
    }
}

fn rewrite_synth_idents_in_comb_stmt(
    s: &mut CombStmt,
    remap: &std::collections::HashMap<String, String>,
) {
    match s {
        CombStmt::Assign(a) => {
            rewrite_synth_idents_in_expr(&mut a.target, remap);
            rewrite_synth_idents_in_expr(&mut a.value, remap);
        }
        CombStmt::IfElse(ie) => {
            rewrite_synth_idents_in_expr(&mut ie.cond, remap);
            for st in &mut ie.then_stmts { rewrite_synth_idents_in_comb_stmt(st, remap); }
            for st in &mut ie.else_stmts { rewrite_synth_idents_in_comb_stmt(st, remap); }
        }
        _ => {}
    }
}

fn rewrite_synth_idents_in_stmt(
    s: &mut Stmt,
    remap: &std::collections::HashMap<String, String>,
) {
    match s {
        Stmt::Assign(a) => {
            rewrite_synth_idents_in_expr(&mut a.target, remap);
            rewrite_synth_idents_in_expr(&mut a.value, remap);
        }
        Stmt::IfElse(ie) => {
            rewrite_synth_idents_in_expr(&mut ie.cond, remap);
            for st in &mut ie.then_stmts { rewrite_synth_idents_in_stmt(st, remap); }
            for st in &mut ie.else_stmts { rewrite_synth_idents_in_stmt(st, remap); }
        }
        _ => {}
    }
}

/// Try-rewrite the prefix of a SynthIdent's name string. Matches both
/// `<old>_<rest>` and `__<old>_<rest>` (the latter is the codegen-style
/// double-underscore-prefixed state names like `__s_data_credit`).
fn try_remap_synth_name(
    name: &str,
    remap: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let (under, rest) = if let Some(r) = name.strip_prefix("__") {
        ("__", r)
    } else {
        ("", name)
    };
    for (old, new) in remap {
        let prefix = format!("{old}_");
        if let Some(suffix) = rest.strip_prefix(&prefix) {
            return Some(format!("{under}{new}_{suffix}"));
        }
    }
    None
}

fn rewrite_synth_idents_in_expr(
    expr: &mut Expr,
    remap: &std::collections::HashMap<String, String>,
) {
    use ExprKind::*;
    match &mut expr.kind {
        SynthIdent(name, _) => {
            if let Some(new_name) = try_remap_synth_name(name, remap) {
                *name = new_name;
            }
        }
        Binary(_, l, r) => {
            rewrite_synth_idents_in_expr(l, remap);
            rewrite_synth_idents_in_expr(r, remap);
        }
        Unary(_, e) | Cast(e, _) | Clog2(e) | Onehot(e) | Signed(e) | Unsigned(e) => {
            rewrite_synth_idents_in_expr(e, remap);
        }
        Index(b, i) => {
            rewrite_synth_idents_in_expr(b, remap);
            rewrite_synth_idents_in_expr(i, remap);
        }
        BitSlice(b, hi, lo) => {
            rewrite_synth_idents_in_expr(b, remap);
            rewrite_synth_idents_in_expr(hi, remap);
            rewrite_synth_idents_in_expr(lo, remap);
        }
        PartSelect(b, s, w, _) => {
            rewrite_synth_idents_in_expr(b, remap);
            rewrite_synth_idents_in_expr(s, remap);
            rewrite_synth_idents_in_expr(w, remap);
        }
        Ternary(c, t, f) => {
            rewrite_synth_idents_in_expr(c, remap);
            rewrite_synth_idents_in_expr(t, remap);
            rewrite_synth_idents_in_expr(f, remap);
        }
        Concat(xs) => {
            for x in xs { rewrite_synth_idents_in_expr(x, remap); }
        }
        Repeat(n, x) => {
            rewrite_synth_idents_in_expr(n, remap);
            rewrite_synth_idents_in_expr(x, remap);
        }
        FieldAccess(b, _) => rewrite_synth_idents_in_expr(b, remap),
        MethodCall(recv, _, args) => {
            rewrite_synth_idents_in_expr(recv, remap);
            for a in args { rewrite_synth_idents_in_expr(a, remap); }
        }
        FunctionCall(_, xs) => {
            for x in xs { rewrite_synth_idents_in_expr(x, remap); }
        }
        _ => {}
    }
}

fn subst_comb_stmt_for_formal(
    s: &CombStmt,
    port_map: &std::collections::HashMap<String, Expr>,
    locals: &std::collections::HashSet<String>,
    prefix: &str,
) -> Result<CombStmt, CompileError> {
    match s {
        CombStmt::Assign(a) => {
            let target = subst_expr_for_formal(&a.target, port_map, locals, prefix);
            let value = subst_expr_for_formal(&a.value, port_map, locals, prefix);
            Ok(CombStmt::Assign(Assign { target, value, span: a.span }))
        }
        CombStmt::IfElse(ie) => {
            let cond = subst_expr_for_formal(&ie.cond, port_map, locals, prefix);
            let then_stmts: Vec<CombStmt> = ie.then_stmts.iter()
                .map(|s| subst_comb_stmt_for_formal(s, port_map, locals, prefix))
                .collect::<Result<_, _>>()?;
            let else_stmts: Vec<CombStmt> = ie.else_stmts.iter()
                .map(|s| subst_comb_stmt_for_formal(s, port_map, locals, prefix))
                .collect::<Result<_, _>>()?;
            Ok(CombStmt::IfElse(IfElseOf {
                cond,
                then_stmts,
                else_stmts,
                unique: ie.unique,
                span: ie.span,
            }))
        }
        other => {
            let sp = match other {
                CombStmt::For(f) => f.span,
                CombStmt::MatchExpr(m) => m.span,
                CombStmt::Log(l) => l.span,
                _ => Span { start: 0, end: 0 },
            };
            Err(CompileError::general(
                &format!(
                    "hierarchical formal v1: unsupported comb stmt in sub-module ({:?}); only Assign and IfElse allowed in this slice",
                    std::mem::discriminant(other),
                ),
                sp,
            ))
        }
    }
}

fn lookup_module<'a>(ast: &'a SourceFile, name: &str) -> Option<&'a ModuleDecl> {
    ast.items.iter().find_map(|it| match it {
        Item::Module(m) if m.name.name == name => Some(m),
        _ => None,
    })
}

fn validate_sub_for_formal(sub: &ModuleDecl) -> Result<(), CompileError> {
    for p in &sub.ports {
        // Bus ports are accepted when carrying credit_channels (PR-hf4
        // item 5). Other bus contents (handshake / tlm_method / plain
        // signals) still need their own modelling and aren't supported
        // in this slice — `flatten_for_formal` checks the bus's content
        // when it processes the inst.
        if p.bus_info.is_some() { continue; }
        // Scalar ports only.
        match &p.ty {
            TypeExpr::UInt(_) | TypeExpr::SInt(_) | TypeExpr::Bool
                | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => {}
            _ => {
                return Err(CompileError::general(
                    &format!(
                        "hierarchical formal v1: sub-module `{}` port `{}` has non-scalar type; only UInt/SInt/Bool/Bit/Clock/Reset are supported",
                        sub.name.name, p.name.name,
                    ),
                    p.span,
                ));
            }
        }
    }
    for bi in &sub.body {
        match bi {
            ModuleBodyItem::LetBinding(_) => {}
            ModuleBodyItem::CombBlock(_) => {}
            ModuleBodyItem::RegDecl(_) => {}
            ModuleBodyItem::RegBlock(_) => {}
            other => {
                let kind = match other {
                    ModuleBodyItem::LatchBlock(_) => "latch block",
                    ModuleBodyItem::Inst(_) => "nested instance",
                    ModuleBodyItem::Generate(_) => "generate",
                    ModuleBodyItem::PipeRegDecl(_) => "pipe_reg",
                    ModuleBodyItem::WireDecl(_) => "wire",
                    ModuleBodyItem::Thread(_) => "thread",
                    ModuleBodyItem::Resource(_) => "resource",
                    ModuleBodyItem::Assert(_) => "assert/cover",
                    ModuleBodyItem::Function(_) => "function",
                    _ => "item",
                };
                return Err(CompileError::general(
                    &format!(
                        "hierarchical formal v1: sub-module `{}` contains a {} — supported: `let` bindings, `comb` blocks, `reg` decls, `seq` blocks. Other constructs land in follow-up PRs.",
                        sub.name.name, kind
                    ),
                    bi.span(),
                ));
            }
        }
    }
    Ok(())
}

/// Substitute the reset clause on a RegDecl. The signal ident in
/// `reset <sig> => <val>` is a sub port → resolve via port_map.
fn subst_reg_reset_for_formal(
    reset: &RegReset,
    port_map: &std::collections::HashMap<String, Expr>,
    locals: &std::collections::HashSet<String>,
    prefix: &str,
) -> RegReset {
    match reset {
        RegReset::None => RegReset::None,
        RegReset::Inherit(sig, val) => {
            let new_sig = resolve_port_or_prefix(sig, port_map, locals, prefix);
            let new_val = subst_expr_for_formal(val, port_map, locals, prefix);
            RegReset::Inherit(new_sig, new_val)
        }
        RegReset::Explicit(sig, kind, lvl, val) => {
            let new_sig = resolve_port_or_prefix(sig, port_map, locals, prefix);
            let new_val = subst_expr_for_formal(val, port_map, locals, prefix);
            RegReset::Explicit(new_sig, *kind, *lvl, new_val)
        }
    }
}

/// Resolve an Ident that names a sub signal (port or local). If it's a
/// port, pull the parent-side expression from port_map and require it
/// to be a simple Ident (v1 constraint — same as port-driving lets).
/// If it's a local, prefix.
fn resolve_port_or_prefix(
    id: &Ident,
    port_map: &std::collections::HashMap<String, Expr>,
    locals: &std::collections::HashSet<String>,
    prefix: &str,
) -> Ident {
    if let Some(expr) = port_map.get(&id.name) {
        if let ExprKind::Ident(parent_name) = &expr.kind {
            return Ident::new(parent_name.clone(), id.span);
        }
    }
    if locals.contains(&id.name) {
        return Ident::new(format!("{prefix}{}", id.name), id.span);
    }
    id.clone()
}

fn resolve_port_ident_for_formal(
    id: &Ident,
    port_map: &std::collections::HashMap<String, Expr>,
    inst_name: &str,
) -> Result<Ident, CompileError> {
    if let Some(expr) = port_map.get(&id.name) {
        if let ExprKind::Ident(parent_name) = &expr.kind {
            return Ok(Ident::new(parent_name.clone(), id.span));
        }
        return Err(CompileError::general(
            &format!(
                "hierarchical formal v1: inst `{}` port `{}` used as clock/reset must bind to a simple parent identifier",
                inst_name, id.name
            ),
            id.span,
        ));
    }
    // Not a port — leave as-is (could be a parent-scope clock reference
    // if the sub-module body uses a name that coincidentally matches).
    Ok(id.clone())
}

/// Substitute a seq-block Stmt. Mirrors the CombStmt substituter but
/// over the Stmt variants.
fn subst_stmt_for_formal(
    s: &Stmt,
    port_map: &std::collections::HashMap<String, Expr>,
    locals: &std::collections::HashSet<String>,
    prefix: &str,
) -> Result<Stmt, CompileError> {
    match s {
        Stmt::Assign(a) => {
            let target = subst_expr_for_formal(&a.target, port_map, locals, prefix);
            let value = subst_expr_for_formal(&a.value, port_map, locals, prefix);
            Ok(Stmt::Assign(Assign { target, value, span: a.span }))
        }
        Stmt::IfElse(ie) => {
            let cond = subst_expr_for_formal(&ie.cond, port_map, locals, prefix);
            let then_stmts: Vec<Stmt> = ie.then_stmts.iter()
                .map(|s| subst_stmt_for_formal(s, port_map, locals, prefix))
                .collect::<Result<_, _>>()?;
            let else_stmts: Vec<Stmt> = ie.else_stmts.iter()
                .map(|s| subst_stmt_for_formal(s, port_map, locals, prefix))
                .collect::<Result<_, _>>()?;
            Ok(Stmt::IfElse(IfElseOf {
                cond,
                then_stmts,
                else_stmts,
                unique: ie.unique,
                span: ie.span,
            }))
        }
        other => {
            let sp = match other {
                Stmt::Match(m) => m.span,
                Stmt::Log(l) => l.span,
                Stmt::For(f) => f.span,
                Stmt::Init(i) => i.span,
                Stmt::WaitUntil(_, sp) => *sp,
                Stmt::DoUntil { span, .. } => *span,
                _ => Span { start: 0, end: 0 },
            };
            Err(CompileError::general(
                &format!(
                    "hierarchical formal v1: unsupported seq stmt in sub-module ({:?}); only Assign and IfElse allowed in this slice",
                    std::mem::discriminant(other),
                ),
                sp,
            ))
        }
    }
}

/// Walk `expr` and substitute per the rules:
///   - `Ident(name)` where `name ∈ port_map` → the parent-side expression.
///   - `Ident(name)` where `name ∈ locals` → `Ident("<prefix><name>")`.
///   - anything else → recurse, otherwise unchanged.
fn subst_expr_for_formal(
    expr: &Expr,
    port_map: &std::collections::HashMap<String, Expr>,
    locals: &std::collections::HashSet<String>,
    prefix: &str,
) -> Expr {
    use ExprKind::*;
    let new_kind = match &expr.kind {
        Ident(name) => {
            if let Some(p) = port_map.get(name) { return p.clone(); }
            if locals.contains(name) {
                Ident(format!("{prefix}{name}"))
            } else {
                return expr.clone();
            }
        }
        Binary(op, l, r) => Binary(
            *op,
            Box::new(subst_expr_for_formal(l, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(r, port_map, locals, prefix)),
        ),
        Unary(op, e) => Unary(*op, Box::new(subst_expr_for_formal(e, port_map, locals, prefix))),
        Cast(e, ty) => Cast(Box::new(subst_expr_for_formal(e, port_map, locals, prefix)), ty.clone()),
        Index(b, i) => Index(
            Box::new(subst_expr_for_formal(b, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(i, port_map, locals, prefix)),
        ),
        BitSlice(b, hi, lo) => BitSlice(
            Box::new(subst_expr_for_formal(b, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(hi, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(lo, port_map, locals, prefix)),
        ),
        PartSelect(b, s, w, plus) => PartSelect(
            Box::new(subst_expr_for_formal(b, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(s, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(w, port_map, locals, prefix)),
            *plus,
        ),
        Ternary(c, t, f) => Ternary(
            Box::new(subst_expr_for_formal(c, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(t, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(f, port_map, locals, prefix)),
        ),
        Clog2(e) => Clog2(Box::new(subst_expr_for_formal(e, port_map, locals, prefix))),
        Onehot(e) => Onehot(Box::new(subst_expr_for_formal(e, port_map, locals, prefix))),
        Signed(e) => Signed(Box::new(subst_expr_for_formal(e, port_map, locals, prefix))),
        Unsigned(e) => Unsigned(Box::new(subst_expr_for_formal(e, port_map, locals, prefix))),
        MethodCall(recv, m, args) => MethodCall(
            Box::new(subst_expr_for_formal(recv, port_map, locals, prefix)),
            m.clone(),
            args.iter().map(|a| subst_expr_for_formal(a, port_map, locals, prefix)).collect(),
        ),
        Concat(xs) => Concat(xs.iter().map(|x| subst_expr_for_formal(x, port_map, locals, prefix)).collect()),
        Repeat(n, x) => Repeat(
            Box::new(subst_expr_for_formal(n, port_map, locals, prefix)),
            Box::new(subst_expr_for_formal(x, port_map, locals, prefix)),
        ),
        FieldAccess(b, f) => FieldAccess(
            Box::new(subst_expr_for_formal(b, port_map, locals, prefix)),
            f.clone(),
        ),
        FunctionCall(n, xs) => FunctionCall(
            n.clone(),
            xs.iter().map(|x| subst_expr_for_formal(x, port_map, locals, prefix)).collect(),
        ),
        _ => return expr.clone(),
    };
    Expr { kind: new_kind, span: expr.span, parenthesized: expr.parenthesized }
}

// ── Top-module selection ─────────────────────────────────────────────────────

fn select_top<'a>(
    ast: &'a SourceFile,
    requested: Option<&str>,
) -> Result<&'a ModuleDecl, CompileError> {
    // Visible modules = non-underscore-prefixed (hides `_<Name>_threads` helpers).
    let visible: Vec<&ModuleDecl> = ast.items.iter().filter_map(|it| match it {
        Item::Module(m) if !m.name.name.starts_with('_') => Some(m),
        _ => None,
    }).collect();

    if let Some(name) = requested {
        for m in ast.items.iter().filter_map(|it| match it {
            Item::Module(m) => Some(m),
            _ => None,
        }) {
            if m.name.name == name { return Ok(m); }
        }
        return Err(CompileError::general(
            &format!("module `{name}` not found in input"),
            Span { start: 0, end: 0 },
        ));
    }

    match visible.len() {
        0 => Err(CompileError::general(
            "no module found in input — arch formal requires a `module` declaration",
            Span { start: 0, end: 0 },
        )),
        1 => Ok(visible[0]),
        _ => {
            let names: Vec<&str> = visible.iter().map(|m| m.name.name.as_str()).collect();
            Err(CompileError::general(
                &format!("multiple modules in input ({}); specify --top <Name>", names.join(", ")),
                Span { start: 0, end: 0 },
            ))
        }
    }
}

// ── Context ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SignalInfo {
    width: u32,
    signed: bool,
    /// "input", "reg", "wire", "output" — for declaration ordering.
    kind: SignalKind,
}

#[derive(Debug, Clone, PartialEq)]
enum SignalKind {
    Input,
    Output,
    Reg,
    Wire,
}

#[derive(Debug, Clone)]
struct ResetInfo {
    name: String,
    #[allow(dead_code)]
    is_async: bool,
    is_low: bool,
}

#[derive(Debug, Clone)]
struct PropertyDecl {
    name: String,
    kind: AssertKind,
    expr: Expr,
    span: Span,
}

struct FormalCtx<'a> {
    module: &'a ModuleDecl,
    #[allow(dead_code)]
    symbols: &'a SymbolTable,
    /// Signal name → width / signedness / kind.
    sigs: HashMap<String, SignalInfo>,
    /// Ordered list of input-port names (for unrolled declaration emission).
    inputs: Vec<String>,
    /// Ordered list of output-port names.
    outputs: Vec<String>,
    /// Ordered list of reg names.
    regs: Vec<String>,
    /// Ordered list of wire names.
    wires: Vec<String>,
    /// Reg name → reset value expression (if Inherit or Explicit).
    reg_reset: HashMap<String, Expr>,
    /// Reg name → rhs expression for assignment in its RegBlock, gated by path conds.
    /// (path_cond_expr, rhs_expr) pairs in declaration order.
    reg_writes: HashMap<String, Vec<(Expr, Expr)>>,
    /// `comb` block statements (flattened list of (target_ident_or_expr, guard, value)).
    comb_assigns: Vec<CombAssignFlat>,
    /// `let name = value;` bindings, inlined at emission.
    let_bindings: HashMap<String, Expr>,
    /// Reset port info.
    reset: ResetInfo,
    /// Param name → constant u64 value (from `param NAME: const = value`).
    params: HashMap<String, u64>,
    /// Enum variants: "EnumName::Variant" → (u64 value, bit width).
    enum_variants: HashMap<String, (u64, u32)>,
    /// Collected assert/cover properties.
    properties: Vec<PropertyDecl>,
    /// Comb-topological ordering of wire / output names.
    comb_order: Vec<String>,
    /// credit_channel sites attached to bus ports on this module.
    /// Populated by `collect_credit_channel_sites()`; consumed by
    /// follow-up items in PR-hf4 (state registration, transitions,
    /// SynthIdent resolution).
    credit_sites: Vec<CreditChannelSite>,
    /// Sites carried in by `flatten_for_formal` from sub-instances'
    /// bus ports (PR-hf4 item 5). Pre-loaded by `run()` before
    /// `preprocess()` runs; merged into `credit_sites` and registered
    /// against the parent-side connection name.
    carried_credit_sites: Vec<CarriedCreditSite>,
}

#[derive(Debug, Clone)]
struct CombAssignFlat {
    target: String,          // flat name (e.g. "y" or "out[2]"); v1 supports ident targets only
    guard: Vec<Expr>,        // stack of conditions (ANDed)
    value: Expr,
}

/// One credit_channel instance, attached to a specific bus port on the
/// current (post-flattening) module. PR-hf4 Phase 1 item 1: collection
/// only — later items use the sites to register BV state, emit
/// transitions, and resolve `SynthIdent` references in the encoder.
///
/// `is_sender` records which side of the channel this port binds —
/// codegen emits the counter reg on the sender side and the FIFO
/// occupancy regs on the receiver side, and we mirror that split.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields consumed by PR-hf4 items 2+ (state / transitions / SynthIdent)
struct CreditChannelSite {
    /// Owning port name (e.g. `s` for `port s: initiator MyBus`).
    port_name: String,
    /// Channel meta as declared on the bus.
    meta: CreditChannelMeta,
    /// True if this port is the sender side (initiator/Out or target/In).
    is_sender: bool,
    /// DEPTH folded to a constant. `None` means the default wasn't
    /// foldable with current params — the site is recorded but later
    /// items will skip it with an error.
    depth: Option<u64>,
}

impl<'a> FormalCtx<'a> {
    fn new(module: &'a ModuleDecl, symbols: &'a SymbolTable) -> Self {
        FormalCtx {
            module,
            symbols,
            sigs: HashMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            regs: Vec::new(),
            wires: Vec::new(),
            reg_reset: HashMap::new(),
            reg_writes: HashMap::new(),
            comb_assigns: Vec::new(),
            let_bindings: HashMap::new(),
            reset: ResetInfo { name: "rst".to_string(), is_async: false, is_low: false },
            params: HashMap::new(),
            enum_variants: HashMap::new(),
            properties: Vec::new(),
            comb_order: Vec::new(),
            credit_sites: Vec::new(),
            carried_credit_sites: Vec::new(),
        }
    }

    /// Walk module bus ports and record every credit_channel sub-construct
    /// carried by their bus. Mirrors `codegen::emit_credit_channel_state`'s
    /// sender/receiver role derivation so the BV state we register later
    /// will use the same naming convention (`__<port>_<ch>_credit` etc.).
    ///
    /// Called from `preprocess()`. PR-hf4 Phase 1 item 1: collection only —
    /// the populated `credit_sites` vector is unused by the encoder in this
    /// commit; subsequent items wire it into BV declarations, reset /
    /// transitions, and SynthIdent resolution.
    fn collect_credit_channel_sites(&mut self) {
        // Carried sites first — flatten_for_formal already keyed them on
        // the parent-side connection name, which is what the lifted
        // state should use as the prefix.
        let carried = std::mem::take(&mut self.carried_credit_sites);
        for cs in carried {
            let depth = cs.meta.params.iter()
                .find(|pp| pp.name.name == "DEPTH")
                .and_then(|pp| pp.default.as_ref())
                .and_then(|e| fold_const_expr(e, &self.params));
            self.credit_sites.push(CreditChannelSite {
                port_name: cs.port_name,
                meta: cs.meta,
                is_sender: cs.is_sender,
                depth,
            });
        }
        for p in &self.module.ports {
            let Some(bi) = &p.bus_info else { continue; };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
            for cc in &info.credit_channels {
                // Role flipping: on the target perspective the bus
                // reverses directions, so an `Out` channel role on the
                // initiator becomes the receiver on the target side.
                let is_sender = matches!(
                    (cc.role_dir, bi.perspective),
                    (Direction::Out, crate::ast::BusPerspective::Initiator)
                        | (Direction::In, crate::ast::BusPerspective::Target)
                );
                let depth = cc.params.iter()
                    .find(|pp| pp.name.name == "DEPTH")
                    .and_then(|pp| pp.default.as_ref())
                    .and_then(|e| fold_const_expr(e, &self.params));
                self.credit_sites.push(CreditChannelSite {
                    port_name: p.name.name.clone(),
                    meta: cc.clone(),
                    is_sender,
                    depth,
                });
            }
        }
    }

    /// For each collected credit_channel site, register the synthesized BV
    /// state that codegen would emit in SV: sender-side `__<port>_<ch>_credit`
    /// counter, or receiver-side `__occ`/`__head`/`__tail` regs. Also
    /// registers the handshake signals (`<port>_<ch>_send_valid` and
    /// `<port>_<ch>_credit_return`) as module-level inputs/outputs based
    /// on the port's role. Payload `send_data` is deferred — the occupancy
    /// invariant doesn't reference it and modelling it requires Vec state.
    ///
    /// Reset values are registered in `reg_reset` as Expr literals so the
    /// existing reset emission picks them up. Next-state rhs (item 3) is
    /// not populated here — consumers that read these regs before item 3
    /// lands will see them hold their reset value throughout.
    fn register_credit_channel_state(&mut self) -> Result<(), CompileError> {
        // Clone into a local vec so we don't hold an immutable borrow of
        // self while mutating sigs/regs/inputs/outputs below.
        let sites = self.credit_sites.clone();
        // Detect cross-module channels: both sender and receiver sites
        // share the same (port_name, channel_name). For those, the
        // handshake signals (send_valid, credit_return) become internal
        // wires shared between the two sides instead of separate
        // input/output ports on the flat module.
        let mut both_sides: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        let mut have_sender: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        let mut have_receiver: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        for s in &sites {
            let key = (s.port_name.clone(), s.meta.name.name.clone());
            if s.is_sender { have_sender.insert(key); }
            else { have_receiver.insert(key); }
        }
        for k in have_sender.intersection(&have_receiver) {
            both_sides.insert(k.clone());
        }
        for site in &sites {
            let Some(depth) = site.depth else {
                return Err(CompileError::general(
                    &format!(
                        "credit_channel `{}` on port `{}`: DEPTH could not be folded to a constant — formal encoding requires a concrete DEPTH",
                        site.meta.name.name, site.port_name,
                    ),
                    site.meta.span,
                ));
            };
            if depth == 0 {
                return Err(CompileError::general(
                    &format!(
                        "credit_channel `{}` on port `{}`: DEPTH must be > 0",
                        site.meta.name.name, site.port_name,
                    ),
                    site.meta.span,
                ));
            }
            let ch = &site.meta.name.name;
            let port = &site.port_name;
            // ceil_log2(n) with n >= 1. For n=1 returns 0; callers width-
            // guard per-reg.
            let clog2 = |n: u64| -> u32 {
                if n <= 1 { 0 } else { (n - 1).ilog2() + 1 }
            };
            // Counter width = ceil_log2(DEPTH+1), always >= 1 for DEPTH>=1.
            let cnt_width = clog2(depth + 1).max(1);
            // Pointer width = ceil_log2(DEPTH); 0 if DEPTH==1 (single-slot
            // FIFO doesn't need a pointer reg).
            let ptr_width = clog2(depth);
            let send_valid = format!("{port}_{ch}_send_valid");
            let credit_ret = format!("{port}_{ch}_credit_return");
            let merged = both_sides.contains(&(port.clone(), ch.clone()));
            // Helper: register a 1-bit handshake signal. For merged
            // channels, register once as a Wire (idempotent on the
            // second site). For unmerged channels, register as the
            // requested I/O direction.
            let register_handshake = |this: &mut Self, name: String, kind_if_unmerged: SignalKind| {
                if merged {
                    if !this.sigs.contains_key(&name) {
                        this.sigs.insert(name.clone(), SignalInfo {
                            width: 1, signed: false, kind: SignalKind::Wire,
                        });
                        this.wires.push(name);
                    }
                } else {
                    this.sigs.insert(name.clone(), SignalInfo {
                        width: 1, signed: false, kind: kind_if_unmerged.clone(),
                    });
                    match kind_if_unmerged {
                        SignalKind::Input => this.inputs.push(name),
                        SignalKind::Output => this.outputs.push(name),
                        _ => {}
                    }
                }
            };
            if site.is_sender {
                // Sender side: credit counter, reset to DEPTH.
                let credit = format!("__{port}_{ch}_credit");
                self.sigs.insert(credit.clone(), SignalInfo {
                    width: cnt_width, signed: false, kind: SignalKind::Reg,
                });
                self.regs.push(credit.clone());
                self.reg_reset.insert(credit, mk_sized_lit(cnt_width, depth, site.meta.span));
                register_handshake(self, send_valid, SignalKind::Output);
                register_handshake(self, credit_ret, SignalKind::Input);
            } else {
                // Receiver side: occupancy reg, reset to 0.
                let occ = format!("__{port}_{ch}_occ");
                self.sigs.insert(occ.clone(), SignalInfo {
                    width: cnt_width, signed: false, kind: SignalKind::Reg,
                });
                self.regs.push(occ.clone());
                self.reg_reset.insert(occ, mk_sized_lit(cnt_width, 0, site.meta.span));
                if ptr_width > 0 {
                    let head = format!("__{port}_{ch}_head");
                    let tail = format!("__{port}_{ch}_tail");
                    self.sigs.insert(head.clone(), SignalInfo {
                        width: ptr_width, signed: false, kind: SignalKind::Reg,
                    });
                    self.regs.push(head.clone());
                    self.reg_reset.insert(head, mk_sized_lit(ptr_width, 0, site.meta.span));
                    self.sigs.insert(tail.clone(), SignalInfo {
                        width: ptr_width, signed: false, kind: SignalKind::Reg,
                    });
                    self.regs.push(tail.clone());
                    self.reg_reset.insert(tail, mk_sized_lit(ptr_width, 0, site.meta.span));
                }
                register_handshake(self, send_valid, SignalKind::Input);
                register_handshake(self, credit_ret, SignalKind::Output);
            }
        }
        Ok(())
    }

    /// Emit next-state (reg_writes) for every credit_channel state reg
    /// that was registered in item 2.
    ///
    /// Sender credit reg:
    ///   send_valid && !credit_return ⇒ credit - 1
    ///   !send_valid && credit_return ⇒ credit + 1
    ///   else ⇒ hold
    ///
    /// Receiver occ reg:
    ///   send_valid && !credit_return ⇒ occ + 1     (push, no credit return)
    ///   !send_valid && credit_return ⇒ occ - 1     (credit return = pop_fire)
    ///   else ⇒ hold
    ///
    /// Notes:
    /// - Under normal hierarchical composition the sender's `credit_return`
    ///   input is bound to the receiver's `credit_return` output by bus
    ///   flattening (item 5), so both sides see the same value and the
    ///   transitions stay consistent.
    /// - Underflow/overflow protection (send_valid ⇒ credit > 0 and
    ///   pop_fire ⇒ occ > 0) is a protocol invariant enforced by codegen
    ///   in user-driven designs; in a formal harness where send_valid
    ///   and pop are environment inputs, the harness is expected to add
    ///   those assumptions alongside the occupancy invariant.
    /// - Head / tail pointer rotation uses `(ptr + 1) % DEPTH`. When
    ///   DEPTH isn't a power of two the `%` encoding is emitted as the
    ///   current Mod op (covered by `encode_binary`).
    fn emit_credit_channel_transitions(&mut self) {
        let sites = self.credit_sites.clone();
        for site in &sites {
            let Some(depth) = site.depth else { continue };
            let ch = &site.meta.name.name;
            let port = &site.port_name;
            let span = site.meta.span;
            let send_valid = mk_ident(&format!("{port}_{ch}_send_valid"), span);
            let credit_return = mk_ident(&format!("{port}_{ch}_credit_return"), span);
            let not_send = mk_not(send_valid.clone(), span);
            let not_ret = mk_not(credit_return.clone(), span);
            let dec_cond = mk_bin(BinOp::And, send_valid.clone(), not_ret.clone(), span);
            let inc_cond = mk_bin(BinOp::And, not_send.clone(), credit_return.clone(), span);

            if site.is_sender {
                let credit = mk_ident(&format!("__{port}_{ch}_credit"), span);
                let one_cnt = mk_sized_lit(1, 1, span);
                let dec_rhs = mk_bin(BinOp::Sub, credit.clone(), one_cnt.clone(), span);
                let inc_rhs = mk_bin(BinOp::Add, credit, one_cnt, span);
                let writes = self.reg_writes.entry(format!("__{port}_{ch}_credit")).or_default();
                writes.push((dec_cond.clone(), dec_rhs));
                writes.push((inc_cond.clone(), inc_rhs));
            } else {
                let occ = mk_ident(&format!("__{port}_{ch}_occ"), span);
                let one_cnt = mk_sized_lit(1, 1, span);
                let inc_rhs = mk_bin(BinOp::Add, occ.clone(), one_cnt.clone(), span);
                let dec_rhs = mk_bin(BinOp::Sub, occ, one_cnt, span);
                let writes = self.reg_writes.entry(format!("__{port}_{ch}_occ")).or_default();
                // Priority: push first (inc), pop second (dec).
                writes.push((dec_cond.clone(), inc_rhs));
                writes.push((inc_cond.clone(), dec_rhs));

                // Head / tail pointer rotation (skip when DEPTH==1).
                let ptr_width = if depth <= 1 { 0 } else { (depth - 1).ilog2() + 1 };
                if ptr_width > 0 {
                    let head_name = format!("__{port}_{ch}_head");
                    let tail_name = format!("__{port}_{ch}_tail");
                    let one_ptr = mk_sized_lit(1, 1, span);
                    let depth_lit = mk_sized_lit(ptr_width + 1, depth, span);
                    // head advances on pop_fire (= credit_return on receiver).
                    let head = mk_ident(&head_name, span);
                    let head_plus = mk_bin(BinOp::Add, head, one_ptr.clone(), span);
                    let head_next = mk_bin(BinOp::Mod, head_plus, depth_lit.clone(), span);
                    self.reg_writes.entry(head_name).or_default()
                        .push((credit_return.clone(), head_next));
                    // tail advances on push (= send_valid on receiver).
                    let tail = mk_ident(&tail_name, span);
                    let tail_plus = mk_bin(BinOp::Add, tail, one_ptr, span);
                    let tail_next = mk_bin(BinOp::Mod, tail_plus, depth_lit, span);
                    self.reg_writes.entry(tail_name).or_default()
                        .push((send_valid.clone(), tail_next));
                }
            }
        }
    }

    fn preprocess(&mut self) -> Result<(), CompileError> {
        // Collect param constants
        for p in &self.module.params {
            if let ParamKind::Const = p.kind {
                if let Some(def) = &p.default {
                    if let Some(v) = fold_const_expr(def, &self.params) {
                        self.params.insert(p.name.name.clone(), v);
                    }
                }
            }
        }

        // Collect enum variant values (module-scope enums not common; look at top-level ast)
        // Populated lazily from the symbol table would be ideal; for v1 handle Literal only
        // and let the encoder fail on EnumVariant with a clear error.

        // Reset info
        let (rn, is_async, is_low) = crate::ast::extract_reset_info(&self.module.ports);
        self.reset = ResetInfo { name: rn, is_async, is_low };

        // PR-hf4 item 1: collect credit_channel sites for later state
        // registration and SynthIdent resolution.
        self.collect_credit_channel_sites();
        // PR-hf4 item 2: register BV state + handshake signals per site.
        self.register_credit_channel_state()?;
        // PR-hf4 item 3: emit next-state (reg_writes) for the lifted
        // regs. SynthIdent resolution in item 4; hierarchical carry in
        // item 5.
        self.emit_credit_channel_transitions();

        // Defensive: any Inst items here mean the flattener didn't run.
        // `run()` invokes `flatten_for_formal` before preprocess() for
        // modules containing insts, so reaching this means a caller
        // bypassed the pipeline.
        for b in &self.module.body {
            if let ModuleBodyItem::Inst(inst) = b {
                return Err(CompileError::general(
                    &format!(
                        "internal error: `inst {}` reached FormalCtx::preprocess without flattening. Call `flatten_for_formal` first (see run()).",
                        inst.name.name
                    ),
                    inst.span,
                ));
            }
        }
        for b in &self.module.body {
            if let ModuleBodyItem::Thread(t) = b {
                return Err(CompileError::general(
                    "`thread` blocks must be lowered before `arch formal` — run via the normal compile pipeline (they're lowered automatically); if you see this error you're likely targeting an unlowered AST",
                    t.span,
                ));
            }
        }

        // Ports (declare inputs/outputs + widths)
        for port in &self.module.ports {
            // Bus ports: the bus's individual signals aren't in the AST as
            // scalar ports; we register them per-credit_channel below in
            // `register_credit_channel_state()`. Skip generic scalar-only
            // handling. (Non-credit_channel bus usage is still unsupported
            // in formal v1 — the encoder will fail later on any reference.)
            if port.bus_info.is_some() {
                continue;
            }
            // Reject bus / vec / struct / enum types
            self.check_scalar_type(&port.ty, port.span)?;
            let (w, signed) = self.type_width_signed(&port.ty, port.span)?;
            let kind = match port.direction {
                Direction::In => SignalKind::Input,
                Direction::Out => SignalKind::Output,
            };
            self.sigs.insert(port.name.name.clone(), SignalInfo { width: w, signed, kind: kind.clone() });
            match kind {
                SignalKind::Input => self.inputs.push(port.name.name.clone()),
                SignalKind::Output => self.outputs.push(port.name.name.clone()),
                _ => {}
            }
            // A `port reg o: out T` is both an output and a reg.
            if let Some(reg_info) = &port.reg_info {
                self.regs.push(port.name.name.clone());
                self.sigs.get_mut(&port.name.name).unwrap().kind = SignalKind::Reg;
                if let RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) = &reg_info.reset {
                    self.reg_reset.insert(port.name.name.clone(), val.clone());
                } else if let Some(init) = &reg_info.init {
                    self.reg_reset.insert(port.name.name.clone(), init.clone());
                }
            }
        }

        // Reg / Wire decls and collect RegBlock writes
        let mut reg_block_clock: Option<String> = None;
        for b in &self.module.body {
            match b {
                ModuleBodyItem::RegDecl(r) => {
                    self.check_scalar_type(&r.ty, r.span)?;
                    let (w, signed) = self.type_width_signed(&r.ty, r.span)?;
                    self.sigs.insert(r.name.name.clone(), SignalInfo { width: w, signed, kind: SignalKind::Reg });
                    self.regs.push(r.name.name.clone());
                    match &r.reset {
                        RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) => {
                            self.reg_reset.insert(r.name.name.clone(), val.clone());
                        }
                        RegReset::None => {
                            if let Some(init) = &r.init {
                                self.reg_reset.insert(r.name.name.clone(), init.clone());
                            }
                        }
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    self.check_scalar_type(&w.ty, w.span)?;
                    let (width, signed) = self.type_width_signed(&w.ty, w.span)?;
                    self.sigs.insert(w.name.name.clone(), SignalInfo { width, signed, kind: SignalKind::Wire });
                    self.wires.push(w.name.name.clone());
                }
                ModuleBodyItem::LetBinding(lb) => {
                    self.let_bindings.insert(lb.name.name.clone(), lb.value.clone());
                }
                ModuleBodyItem::Assert(a) => {
                    let name = a.name.as_ref().map(|i| i.name.clone())
                        .unwrap_or_else(|| format!("prop_{}", a.span.start));
                    self.properties.push(PropertyDecl {
                        name,
                        kind: a.kind.clone(),
                        expr: a.expr.clone(),
                        span: a.span,
                    });
                }
                ModuleBodyItem::RegBlock(rb) => {
                    if let Some(existing) = &reg_block_clock {
                        if existing != &rb.clock.name {
                            return Err(CompileError::general(
                                &format!(
                                    "arch formal v1 only supports single-clock designs; found reg blocks on `{}` and `{}`",
                                    existing, rb.clock.name
                                ),
                                rb.span,
                            ));
                        }
                    } else {
                        reg_block_clock = Some(rb.clock.name.clone());
                    }
                    // Walk and collect (path_cond_expr, rhs) per reg
                    for s in &rb.stmts {
                        self.walk_reg_stmt(s, &[])?;
                    }
                }
                ModuleBodyItem::CombBlock(cb) => {
                    for s in &cb.stmts {
                        self.walk_comb_stmt(s, &[])?;
                    }
                }
                ModuleBodyItem::LatchBlock(l) => {
                    return Err(CompileError::general(
                        "`latch` blocks are not supported by `arch formal` v1",
                        l.span,
                    ));
                }
                ModuleBodyItem::PipeRegDecl(p) => {
                    return Err(CompileError::general(
                        "`pipe_reg` is not supported by `arch formal` v1",
                        p.span,
                    ));
                }
                ModuleBodyItem::Generate(_) => {
                    // Should have been expanded by elaborate.
                    return Err(CompileError::general(
                        "unexpanded `generate` block — compile pipeline should have expanded this",
                        self.module.span,
                    ));
                }
                ModuleBodyItem::Function(_) | ModuleBodyItem::Resource(_) => {
                    // Ignore; v1 doesn't encode module-local functions
                }
                ModuleBodyItem::Inst(_) | ModuleBodyItem::Thread(_) => {
                    // Already handled above
                }
            }
        }

        // Build comb-block topological order over wires + output ports
        self.comb_order = self.comb_topo_order()?;

        // Detect circular let references (simple DFS)
        self.check_let_cycles()?;

        Ok(())
    }

    /// Walk a reg-block Stmt, collecting (path_cond_expr, rhs) per reg into `reg_writes`.
    fn walk_reg_stmt(&mut self, s: &Stmt, path: &[Expr]) -> Result<(), CompileError> {
        match s {
            Stmt::Assign(a) => {
                let name = match target_root_ident(&a.target) {
                    Some(n) => n,
                    None => return Err(CompileError::general(
                        "arch formal v1 only supports reg assignments to bare identifiers (no Vec/struct/field targets)",
                        a.span,
                    )),
                };
                let cond = and_all(path);
                let entry = self.reg_writes.entry(name).or_default();
                entry.push((cond, a.value.clone()));
            }
            Stmt::IfElse(ie) => {
                let mut then_path = path.to_vec();
                then_path.push(ie.cond.clone());
                for child in &ie.then_stmts {
                    self.walk_reg_stmt(child, &then_path)?;
                }
                let mut else_path = path.to_vec();
                else_path.push(not_expr(ie.cond.clone()));
                for child in &ie.else_stmts {
                    self.walk_reg_stmt(child, &else_path)?;
                }
            }
            Stmt::Init(ib) => {
                // Treat Init-block writes as reset-time assigns: merge into reg_reset.
                for child in &ib.body {
                    self.collect_init_writes(child)?;
                }
            }
            Stmt::For(_) => {
                return Err(CompileError::general(
                    "`for` loops inside `seq` blocks are not supported by `arch formal` v1 (unroll manually)",
                    s_span(s),
                ));
            }
            Stmt::Match(m) => {
                return Err(CompileError::general(
                    "`match` inside `seq` blocks is not supported by `arch formal` v1 (rewrite as if/else)",
                    m.span,
                ));
            }
            Stmt::Log(_) => { /* ignore */ }
            Stmt::WaitUntil(_, span) | Stmt::DoUntil { span, .. } => {
                return Err(CompileError::general(
                    "pipeline `wait`/`do-until` is not supported by `arch formal` v1",
                    *span,
                ));
            }
        }
        Ok(())
    }

    fn collect_init_writes(&mut self, s: &Stmt) -> Result<(), CompileError> {
        match s {
            Stmt::Assign(a) => {
                if let Some(name) = target_root_ident(&a.target) {
                    self.reg_reset.insert(name, a.value.clone());
                }
            }
            Stmt::IfElse(ie) => {
                for c in &ie.then_stmts { self.collect_init_writes(c)?; }
                for c in &ie.else_stmts { self.collect_init_writes(c)?; }
            }
            Stmt::Init(ib) => {
                for c in &ib.body { self.collect_init_writes(c)?; }
            }
            _ => {}
        }
        Ok(())
    }

    fn walk_comb_stmt(&mut self, s: &CombStmt, path: &[Expr]) -> Result<(), CompileError> {
        match s {
            CombStmt::Assign(a) => {
                let name = match target_root_ident(&a.target) {
                    Some(n) => n,
                    None => return Err(CompileError::general(
                        "arch formal v1 only supports comb assignments to bare identifiers",
                        a.span,
                    )),
                };
                self.comb_assigns.push(CombAssignFlat {
                    target: name,
                    guard: path.to_vec(),
                    value: a.value.clone(),
                });
            }
            CombStmt::IfElse(ie) => {
                let mut then_path = path.to_vec();
                then_path.push(ie.cond.clone());
                for c in &ie.then_stmts { self.walk_comb_stmt(c, &then_path)?; }
                let mut else_path = path.to_vec();
                else_path.push(not_expr(ie.cond.clone()));
                for c in &ie.else_stmts { self.walk_comb_stmt(c, &else_path)?; }
            }
            CombStmt::MatchExpr(m) => {
                return Err(CompileError::general(
                    "`match` inside `comb` blocks is not supported by `arch formal` v1 (rewrite as if/else or expression-level match)",
                    m.span,
                ));
            }
            CombStmt::For(fl) => {
                return Err(CompileError::general(
                    "`for` inside `comb` blocks is not supported by `arch formal` v1 (unroll manually)",
                    fl.span,
                ));
            }
            CombStmt::Log(_) => { /* ignore */ }
        }
        Ok(())
    }

    fn comb_topo_order(&self) -> Result<Vec<String>, CompileError> {
        // Build dep graph: target → set of idents referenced in its guarded value.
        let mut deps: HashMap<String, HashSet<String>> = HashMap::new();
        let mut targets: HashSet<String> = HashSet::new();
        for ca in &self.comb_assigns {
            targets.insert(ca.target.clone());
            let set = deps.entry(ca.target.clone()).or_default();
            for g in &ca.guard { collect_idents(g, set); }
            collect_idents(&ca.value, set);
        }
        // Add let bindings as targets too (so they participate in ordering if referenced).
        for (name, val) in &self.let_bindings {
            targets.insert(name.clone());
            let set = deps.entry(name.clone()).or_default();
            collect_idents(val, set);
        }

        // Topological sort — only among targets that depend on other targets.
        let mut order: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut visiting: HashSet<String> = HashSet::new();
        for t in targets.iter() {
            self.topo_visit(t, &deps, &targets, &mut order, &mut visited, &mut visiting)?;
        }
        Ok(order)
    }

    fn topo_visit(
        &self,
        name: &str,
        deps: &HashMap<String, HashSet<String>>,
        targets: &HashSet<String>,
        order: &mut Vec<String>,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
    ) -> Result<(), CompileError> {
        if visited.contains(name) { return Ok(()); }
        if visiting.contains(name) {
            return Err(CompileError::general(
                &format!("combinational feedback loop through `{name}` — arch formal cannot handle cyclic comb"),
                self.module.span,
            ));
        }
        visiting.insert(name.to_string());
        if let Some(dep_set) = deps.get(name) {
            for d in dep_set {
                if targets.contains(d) && d != name {
                    self.topo_visit(d, deps, targets, order, visited, visiting)?;
                }
            }
        }
        visiting.remove(name);
        visited.insert(name.to_string());
        order.push(name.to_string());
        Ok(())
    }

    fn check_let_cycles(&self) -> Result<(), CompileError> {
        for name in self.let_bindings.keys() {
            let mut stack: Vec<String> = vec![name.clone()];
            self.check_let_path(name, &mut stack)?;
        }
        Ok(())
    }

    fn check_let_path(&self, name: &str, stack: &mut Vec<String>) -> Result<(), CompileError> {
        if let Some(val) = self.let_bindings.get(name) {
            let mut refs = HashSet::new();
            collect_idents(val, &mut refs);
            for r in refs {
                if stack.iter().any(|s| s == &r) {
                    return Err(CompileError::general(
                        &format!("circular let binding involving `{r}`"),
                        self.module.span,
                    ));
                }
                if self.let_bindings.contains_key(&r) {
                    stack.push(r.clone());
                    self.check_let_path(&r, stack)?;
                    stack.pop();
                }
            }
        }
        Ok(())
    }

    // ── Width / type helpers ─────────────────────────────────────────────────

    fn check_scalar_type(&self, ty: &TypeExpr, span: Span) -> Result<(), CompileError> {
        match ty {
            TypeExpr::UInt(_) | TypeExpr::SInt(_) | TypeExpr::Bool | TypeExpr::Bit
                | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => Ok(()),
            TypeExpr::Vec(_, _) => Err(CompileError::general(
                "Vec types are not supported by `arch formal` v1 — use scalars",
                span,
            )),
            TypeExpr::Named(n) => Err(CompileError::general(
                &format!("named type `{}` (struct / enum / typedef) is not supported by `arch formal` v1", n.name),
                span,
            )),
        }
    }

    fn type_width_signed(&self, ty: &TypeExpr, span: Span) -> Result<(u32, bool), CompileError> {
        match ty {
            TypeExpr::UInt(w) => {
                let width = fold_const_expr(w, &self.params).ok_or_else(|| CompileError::general(
                    "could not fold UInt<W> width to a compile-time constant",
                    span,
                ))? as u32;
                if width == 0 {
                    return Err(CompileError::general("width of 0 is not supported", span));
                }
                Ok((width, false))
            }
            TypeExpr::SInt(w) => {
                let width = fold_const_expr(w, &self.params).ok_or_else(|| CompileError::general(
                    "could not fold SInt<W> width to a compile-time constant",
                    span,
                ))? as u32;
                if width == 0 {
                    return Err(CompileError::general("width of 0 is not supported", span));
                }
                Ok((width, true))
            }
            TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) =>
                Ok((1, false)),
            TypeExpr::Vec(_, _) | TypeExpr::Named(_) => Err(CompileError::general(
                "type not supported by arch formal v1",
                span,
            )),
        }
    }

    // ── Emission ─────────────────────────────────────────────────────────────

    fn emit_base(&self, bound: u32) -> Result<String, CompileError> {
        let mut out = String::new();
        out.push_str("; auto-generated by `arch formal`\n");
        out.push_str("(set-logic QF_BV)\n");
        out.push_str("(set-option :produce-models true)\n\n");

        // Declare every non-reg signal at each cycle (inputs get free choice per cycle;
        // wires and outputs are constrained by comb equations).
        for t in 0..=bound {
            out.push_str(&format!("; ── cycle {t} ──\n"));
            for name in &self.inputs {
                let w = self.sigs[name].width;
                out.push_str(&format!("(declare-fun {name}_{t} () (_ BitVec {w}))\n"));
            }
            for name in &self.outputs {
                if self.sigs[name].kind == SignalKind::Reg { continue; }
                let w = self.sigs[name].width;
                out.push_str(&format!("(declare-fun {name}_{t} () (_ BitVec {w}))\n"));
            }
            for name in &self.regs {
                let w = self.sigs[name].width;
                out.push_str(&format!("(declare-fun {name}_{t} () (_ BitVec {w}))\n"));
            }
            for name in &self.wires {
                let w = self.sigs[name].width;
                out.push_str(&format!("(declare-fun {name}_{t} () (_ BitVec {w}))\n"));
            }
            out.push('\n');
        }

        // Initial (t=0) reset-value constraints
        out.push_str("; ── t=0 reset initialization ──\n");
        for reg in &self.regs {
            if let Some(val_expr) = self.reg_reset.get(reg) {
                let w = self.sigs[reg].width;
                let signed = self.sigs[reg].signed;
                let v = self.encode_expr(val_expr, 0, Some((w, signed)))?;
                out.push_str(&format!("(assert (= {reg}_0 {}))\n", v.s));
            }
        }
        out.push('\n');

        // Comb / output equations per cycle
        for t in 0..=bound {
            out.push_str(&format!("; ── comb equations at cycle {t} ──\n"));
            // Walk comb targets in topo order.
            for tgt in &self.comb_order {
                // Resolve value: either a let binding (direct), or one or more guarded comb assigns.
                if let Some(let_val) = self.let_bindings.get(tgt) {
                    // Only emit a constraint if `tgt` is a declared signal (wire/output).
                    if let Some(info) = self.sigs.get(tgt) {
                        let term = self.encode_expr(let_val, t, Some((info.width, info.signed)))?;
                        out.push_str(&format!("(assert (= {tgt}_{t} {}))\n", term.s));
                    }
                    continue;
                }
                let assigns: Vec<&CombAssignFlat> = self.comb_assigns.iter()
                    .filter(|c| &c.target == tgt).collect();
                if assigns.is_empty() { continue; }
                let info = &self.sigs[tgt];
                // Build nested ite from the guard chain. Last unguarded write wins as default.
                let rhs = self.build_comb_ite(&assigns, t, info.width, info.signed)?;
                out.push_str(&format!("(assert (= {tgt}_{t} {rhs}))\n"));
            }
            out.push('\n');
        }

        // Register transition: r_{t+1} = ite(reset, reset_val, next_value)
        for t in 0..bound {
            out.push_str(&format!("; ── register transition cycle {t}→{} ──\n", t + 1));
            for reg in &self.regs {
                let info = &self.sigs[reg];
                let next = self.reg_next(reg, t, info.width, info.signed)?;
                // Reset gate: use reset signal at cycle t (sync) — BMC convention.
                let reset_active = self.reset_active_at(t);
                let reset_val = if let Some(val_expr) = self.reg_reset.get(reg) {
                    let term = self.encode_expr(val_expr, t, Some((info.width, info.signed)))?;
                    term.s
                } else {
                    // No reset value: hold current value on reset.
                    format!("{reg}_{t}")
                };
                let next_gated = if self.reg_reset.contains_key(reg) {
                    format!("(ite {reset_active} {reset_val} {next})")
                } else {
                    next
                };
                out.push_str(&format!("(assert (= {reg}_{} {next_gated}))\n", t + 1));
            }
            out.push('\n');
        }

        Ok(out)
    }

    /// Build nested ite for a reg's next value at cycle t.
    fn reg_next(&self, reg: &str, t: u32, width: u32, signed: bool) -> Result<String, CompileError> {
        let writes = match self.reg_writes.get(reg) {
            Some(w) if !w.is_empty() => w,
            _ => return Ok(format!("{reg}_{t}")), // hold
        };
        // Build from bottom up: start with "hold" and wrap each (cond, rhs) as outer ite.
        let mut inner = format!("{reg}_{t}");
        for (cond_expr, rhs_expr) in writes.iter().rev() {
            let c = self.encode_expr(cond_expr, t, Some((1, false)))?;
            let r = self.encode_expr(rhs_expr, t, Some((width, signed)))?;
            let c_bool = as_bool(&c);
            inner = format!("(ite {c_bool} {} {inner})", r.s);
        }
        Ok(inner)
    }

    fn build_comb_ite(
        &self,
        assigns: &[&CombAssignFlat],
        t: u32,
        width: u32,
        signed: bool,
    ) -> Result<String, CompileError> {
        // Fallthrough: '0 (zero of width)
        let mut inner = bv_zero(width);
        for a in assigns.iter().rev() {
            let rhs = self.encode_expr(&a.value, t, Some((width, signed)))?;
            // AND all guard conditions
            let cond_expr = and_all(&a.guard);
            if a.guard.is_empty() {
                // Unconditional assign — becomes the default.
                inner = rhs.s;
            } else {
                let c = self.encode_expr(&cond_expr, t, Some((1, false)))?;
                let c_bool = as_bool(&c);
                inner = format!("(ite {c_bool} {} {inner})", rhs.s);
            }
        }
        Ok(inner)
    }

    fn reset_active_at(&self, t: u32) -> String {
        // `(= rst_t #b1)` for high-active, `(= rst_t #b0)` for low-active.
        let bit = if self.reset.is_low { "#b0" } else { "#b1" };
        format!("(= {}_{} {bit})", self.reset.name, t)
    }

    /// Encode an expression at cycle `t`, optionally coercing to (width, signed).
    fn encode_expr(
        &self,
        expr: &Expr,
        t: u32,
        target: Option<(u32, bool)>,
    ) -> Result<SmtTerm, CompileError> {
        let term = self.encode_raw(expr, t)?;
        if let Some((w, s)) = target {
            Ok(coerce(term, w, s))
        } else {
            Ok(term)
        }
    }

    fn encode_raw(&self, expr: &Expr, t: u32) -> Result<SmtTerm, CompileError> {
        use ExprKind::*;
        match &expr.kind {
            // Latency annotation is transparent to SMT: at timepoint t,
            // `q@0` is the same as `q` at t. Non-@0 reads are rejected by
            // typecheck before reaching formal emission.
            LatencyAt(inner, _) => self.encode_raw(inner, t),
            // SynthIdent points at codegen-emitted state (credit_channel
            // counter / occ / valid / data wires). PR-hf4 item 2 registered
            // the scalar ones (credit, occ, head, tail, send_valid,
            // credit_return) as real BV signals; resolve those through
            // the normal Ident path. Anything else (payload `_data`,
            // `_can_send` when the bus parameter enables the registered
            // variant) is still unsupported.
            SynthIdent(name, _) => {
                if self.sigs.contains_key(name) {
                    self.encode_ident(name, t, expr.span)
                } else {
                    Err(CompileError::general(
                        &format!(
                            "formal encoding of synthesized identifier `{name}` is not yet supported — only credit_channel scalar state is modelled today (see doc/plan_hierarchical_formal.md PR-hf4)",
                        ),
                        expr.span,
                    ))
                }
            }
            Literal(l) => Ok(lit_to_term(l)),
            Bool(b) => Ok(SmtTerm {
                s: if *b { "#b1".to_string() } else { "#b0".to_string() },
                width: 1,
                signed: false,
            }),
            Ident(name) => self.encode_ident(name, t, expr.span),
            Binary(op, a, b) => self.encode_binary(*op, a, b, t, expr.span),
            Unary(op, a) => self.encode_unary(*op, a, t, expr.span),
            Ternary(c, then_e, else_e) => {
                let ct = self.encode_raw(c, t)?;
                let tt = self.encode_raw(then_e, t)?;
                let et = self.encode_raw(else_e, t)?;
                let w = tt.width.max(et.width);
                let signed = tt.signed || et.signed;
                let th = coerce(tt, w, signed);
                let el = coerce(et, w, signed);
                Ok(SmtTerm {
                    s: format!("(ite {} {} {})", as_bool(&ct), th.s, el.s),
                    width: w,
                    signed,
                })
            }
            MethodCall(recv, method, args) => self.encode_method(recv, method, args, t, expr.span),
            BitSlice(base, hi, lo) => {
                let b = self.encode_raw(base, t)?;
                let hi_v = fold_const_expr(hi, &self.params).ok_or_else(|| CompileError::general(
                    "bit-slice bounds must be compile-time constants", expr.span,
                ))?;
                let lo_v = fold_const_expr(lo, &self.params).ok_or_else(|| CompileError::general(
                    "bit-slice bounds must be compile-time constants", expr.span,
                ))?;
                if hi_v < lo_v {
                    return Err(CompileError::general("bit-slice hi < lo", expr.span));
                }
                let w = (hi_v - lo_v + 1) as u32;
                Ok(SmtTerm {
                    s: format!("((_ extract {hi_v} {lo_v}) {})", b.s),
                    width: w,
                    signed: b.signed,
                })
            }
            PartSelect(base, start, width, is_plus) => {
                let b = self.encode_raw(base, t)?;
                let s_v = fold_const_expr(start, &self.params).ok_or_else(|| CompileError::general(
                    "part-select start must be compile-time constant in arch formal v1",
                    expr.span,
                ))?;
                let w_v = fold_const_expr(width, &self.params).ok_or_else(|| CompileError::general(
                    "part-select width must be compile-time constant",
                    expr.span,
                ))?;
                let (hi, lo) = if *is_plus {
                    (s_v + w_v - 1, s_v)
                } else {
                    (s_v, s_v - (w_v - 1))
                };
                Ok(SmtTerm {
                    s: format!("((_ extract {hi} {lo}) {})", b.s),
                    width: w_v as u32,
                    signed: b.signed,
                })
            }
            Concat(es) => {
                // MSB first in source {a, b} — concat (concat a b) in SMT.
                let parts: Vec<SmtTerm> = es.iter()
                    .map(|e| self.encode_raw(e, t)).collect::<Result<_, _>>()?;
                let total: u32 = parts.iter().map(|p| p.width).sum();
                if parts.len() == 1 {
                    return Ok(parts.into_iter().next().unwrap());
                }
                let mut s = parts[0].s.clone();
                let mut ws = parts[0].width;
                for p in parts.iter().skip(1) {
                    s = format!("(concat {s} {})", p.s);
                    ws += p.width;
                }
                debug_assert_eq!(total, ws);
                Ok(SmtTerm { s, width: total, signed: false })
            }
            Repeat(n, x) => {
                let n_v = fold_const_expr(n, &self.params).ok_or_else(|| CompileError::general(
                    "repeat count must be compile-time constant",
                    expr.span,
                ))?;
                let xt = self.encode_raw(x, t)?;
                let n_v_u = n_v as u32;
                if n_v_u == 0 {
                    return Err(CompileError::general("repeat count must be > 0", expr.span));
                }
                if n_v_u == 1 {
                    return Ok(xt);
                }
                let mut s = xt.s.clone();
                for _ in 1..n_v_u {
                    s = format!("(concat {s} {})", xt.s);
                }
                Ok(SmtTerm { s, width: xt.width * n_v_u, signed: false })
            }
            Signed(inner) => {
                let t_inner = self.encode_raw(inner, t)?;
                Ok(SmtTerm { signed: true, ..t_inner })
            }
            Unsigned(inner) => {
                let t_inner = self.encode_raw(inner, t)?;
                Ok(SmtTerm { signed: false, ..t_inner })
            }
            Clog2(inner) => {
                let v = fold_const_expr(inner, &self.params).ok_or_else(|| CompileError::general(
                    "$clog2 argument must be compile-time constant in arch formal v1",
                    expr.span,
                ))?;
                let r = if v <= 1 { 1 } else { 64 - (v - 1).leading_zeros() as u64 };
                Ok(SmtTerm { s: bv_lit(r, 32), width: 32, signed: false })
            }
            Onehot(idx) => {
                // 1 << idx, in some contextual width. We don't know output width here —
                // default: produce the shift against a 32-bit 1; caller's coerce will size.
                let idx_t = self.encode_raw(idx, t)?;
                // Shift amount must match width of LHS; encode as 32-bit BV.
                let idx32 = coerce(idx_t, 32, false);
                Ok(SmtTerm {
                    s: format!("(bvshl {} {})", bv_lit(1, 32), idx32.s),
                    width: 32,
                    signed: false,
                })
            }
            EnumVariant(en, v) => {
                let key = format!("{}::{}", en.name, v.name);
                if let Some((val, w)) = self.enum_variants.get(&key) {
                    Ok(SmtTerm { s: bv_lit(*val, *w), width: *w, signed: false })
                } else {
                    Err(CompileError::general(
                        &format!("unknown enum variant `{key}` in arch formal v1 (struct/enum support is limited)"),
                        expr.span,
                    ))
                }
            }
            FieldAccess(_, _) | StructLiteral(_, _) | Cast(_, _) | Index(_, _)
            | FunctionCall(_, _) | Inside(_, _) | Match(_, _) | ExprMatch(_, _) | Todo => {
                Err(CompileError::general(
                    "expression kind not supported by arch formal v1 (struct field / cast / index / function call / match / inside / todo)",
                    expr.span,
                ))
            }
        }
    }

    fn encode_ident(&self, name: &str, t: u32, span: Span) -> Result<SmtTerm, CompileError> {
        // 1. Const param?
        if let Some(val) = self.params.get(name) {
            // Default to 32-bit; coerce() resizes as needed.
            return Ok(SmtTerm { s: bv_lit(*val, 32), width: 32, signed: false });
        }
        // 2. Let binding? Inline expand.
        if let Some(val) = self.let_bindings.get(name) {
            return self.encode_raw(val, t);
        }
        // 3. Signal (port / reg / wire)
        if let Some(info) = self.sigs.get(name) {
            return Ok(SmtTerm {
                s: format!("{name}_{t}"),
                width: info.width,
                signed: info.signed,
            });
        }
        Err(CompileError::general(
            &format!("unknown identifier `{name}` in arch formal encoding"),
            span,
        ))
    }

    fn encode_binary(
        &self,
        op: BinOp,
        a: &Expr,
        b: &Expr,
        t: u32,
        span: Span,
    ) -> Result<SmtTerm, CompileError> {
        let ta = self.encode_raw(a, t)?;
        let tb = self.encode_raw(b, t)?;
        match op {
            BinOp::Add | BinOp::Sub => {
                // Non-wrapping: result width = max(W) + 1
                let common = ta.width.max(tb.width);
                let out_w = common + 1;
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, out_w, signed);
                let lb = coerce(tb, out_w, signed);
                let opname = if op == BinOp::Add { "bvadd" } else { "bvsub" };
                Ok(SmtTerm { s: format!("({opname} {} {})", la.s, lb.s), width: out_w, signed })
            }
            BinOp::Mul => {
                // Non-wrapping: result width = W(a) + W(b)
                let out_w = ta.width + tb.width;
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, out_w, signed);
                let lb = coerce(tb, out_w, signed);
                Ok(SmtTerm { s: format!("(bvmul {} {})", la.s, lb.s), width: out_w, signed })
            }
            BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap => {
                // Wrapping: result width = max(W(a), W(b))
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, common, signed);
                let lb = coerce(tb, common, signed);
                let opname = match op {
                    BinOp::AddWrap => "bvadd",
                    BinOp::SubWrap => "bvsub",
                    BinOp::MulWrap => "bvmul",
                    _ => unreachable!(),
                };
                Ok(SmtTerm { s: format!("({opname} {} {})", la.s, lb.s), width: common, signed })
            }
            BinOp::Div | BinOp::Mod => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, common, signed);
                let lb = coerce(tb, common, signed);
                let opname = match (op, signed) {
                    (BinOp::Div, true) => "bvsdiv",
                    (BinOp::Div, false) => "bvudiv",
                    (BinOp::Mod, true) => "bvsrem",
                    (BinOp::Mod, false) => "bvurem",
                    _ => unreachable!(),
                };
                Ok(SmtTerm { s: format!("({opname} {} {})", la.s, lb.s), width: common, signed })
            }
            BinOp::Eq | BinOp::Neq => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, common, signed);
                let lb = coerce(tb, common, signed);
                let eq = format!("(= {} {})", la.s, lb.s);
                let s = if op == BinOp::Eq {
                    format!("(ite {eq} #b1 #b0)")
                } else {
                    format!("(ite {eq} #b0 #b1)")
                };
                Ok(SmtTerm { s, width: 1, signed: false })
            }
            BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, common, signed);
                let lb = coerce(tb, common, signed);
                let opname = match (op, signed) {
                    (BinOp::Lt, false) => "bvult",
                    (BinOp::Gt, false) => "bvugt",
                    (BinOp::Lte, false) => "bvule",
                    (BinOp::Gte, false) => "bvuge",
                    (BinOp::Lt, true) => "bvslt",
                    (BinOp::Gt, true) => "bvsgt",
                    (BinOp::Lte, true) => "bvsle",
                    (BinOp::Gte, true) => "bvsge",
                    _ => unreachable!(),
                };
                let cmp = format!("({opname} {} {})", la.s, lb.s);
                Ok(SmtTerm { s: format!("(ite {cmp} #b1 #b0)"), width: 1, signed: false })
            }
            BinOp::And | BinOp::Or => {
                // Logical — both must be 1-bit BV. Reduce wider operands with `!= 0`.
                let la = as_bv1_bool(&ta);
                let lb = as_bv1_bool(&tb);
                let opname = if op == BinOp::And { "bvand" } else { "bvor" };
                Ok(SmtTerm {
                    s: format!("({opname} {la} {lb})"),
                    width: 1,
                    signed: false,
                })
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, common, signed);
                let lb = coerce(tb, common, signed);
                let opname = match op {
                    BinOp::BitAnd => "bvand",
                    BinOp::BitOr => "bvor",
                    BinOp::BitXor => "bvxor",
                    _ => unreachable!(),
                };
                Ok(SmtTerm { s: format!("({opname} {} {})", la.s, lb.s), width: common, signed })
            }
            BinOp::Shl => {
                // Result width = W(a). Amount zero-extended to W(a).
                let w = ta.width;
                let signed = ta.signed;
                let lb = coerce(tb, w, false);
                Ok(SmtTerm { s: format!("(bvshl {} {})", ta.s, lb.s), width: w, signed })
            }
            BinOp::Shr => {
                let w = ta.width;
                let signed = ta.signed;
                let lb = coerce(tb, w, false);
                let opname = if signed { "bvashr" } else { "bvlshr" };
                Ok(SmtTerm { s: format!("({opname} {} {})", ta.s, lb.s), width: w, signed })
            }
            BinOp::Implies => {
                // a implies b  ≡  !a | b
                let la = as_bv1_bool(&ta);
                let lb = as_bv1_bool(&tb);
                Ok(SmtTerm {
                    s: format!("(bvor (bvnot {la}) {lb})"),
                    width: 1,
                    signed: false,
                })
            }
        }
        .map_err(|e: CompileError| CompileError::general(
            &format!("{}", e_display(&e, span)),
            span,
        ))
    }

    fn encode_unary(
        &self,
        op: UnaryOp,
        a: &Expr,
        t: u32,
        _span: Span,
    ) -> Result<SmtTerm, CompileError> {
        let ta = self.encode_raw(a, t)?;
        match op {
            UnaryOp::Not => {
                let b = as_bv1_bool(&ta);
                Ok(SmtTerm { s: format!("(bvxor {b} #b1)"), width: 1, signed: false })
            }
            UnaryOp::BitNot => {
                Ok(SmtTerm { s: format!("(bvnot {})", ta.s), width: ta.width, signed: ta.signed })
            }
            UnaryOp::Neg => {
                Ok(SmtTerm { s: format!("(bvneg {})", ta.s), width: ta.width, signed: true })
            }
            UnaryOp::RedAnd => {
                // (= x ~0)
                let all_ones = bv_all_ones(ta.width);
                Ok(SmtTerm {
                    s: format!("(ite (= {} {all_ones}) #b1 #b0)", ta.s),
                    width: 1,
                    signed: false,
                })
            }
            UnaryOp::RedOr => {
                let zero = bv_zero(ta.width);
                Ok(SmtTerm {
                    s: format!("(ite (= {} {zero}) #b0 #b1)", ta.s),
                    width: 1,
                    signed: false,
                })
            }
            UnaryOp::RedXor => {
                // Fold bit-by-bit via bvxor on extracted bits
                if ta.width == 1 { return Ok(ta); }
                let mut s = format!("((_ extract 0 0) {})", ta.s);
                for i in 1..ta.width {
                    s = format!("(bvxor {s} ((_ extract {i} {i}) {}))", ta.s);
                }
                Ok(SmtTerm { s, width: 1, signed: false })
            }
        }
    }

    fn encode_method(
        &self,
        recv: &Expr,
        method: &Ident,
        args: &[Expr],
        t: u32,
        span: Span,
    ) -> Result<SmtTerm, CompileError> {
        let r = self.encode_raw(recv, t)?;
        let n = method.name.as_str();
        // Width arg: .trunc<N>()/.zext<N>()/.sext<N>()/.resize<N>() — N encoded as a
        // type-arg expression in args[0] (parser lowers to literal).
        let target_w = if args.is_empty() {
            None
        } else {
            fold_const_expr(&args[0], &self.params).map(|v| v as u32)
        };
        match n {
            "trunc" => {
                let w = target_w.ok_or_else(|| CompileError::general(
                    ".trunc<N>() requires a constant width argument", span,
                ))?;
                if w > r.width {
                    return Err(CompileError::general(
                        ".trunc<N>() target must be ≤ current width", span,
                    ));
                }
                Ok(SmtTerm {
                    s: format!("((_ extract {} 0) {})", w - 1, r.s),
                    width: w,
                    signed: r.signed,
                })
            }
            "zext" => {
                let w = target_w.ok_or_else(|| CompileError::general(
                    ".zext<N>() requires a constant width argument", span,
                ))?;
                if w < r.width {
                    return Err(CompileError::general(
                        ".zext<N>() target must be ≥ current width", span,
                    ));
                }
                let pad = w - r.width;
                Ok(SmtTerm {
                    s: if pad == 0 { r.s.clone() }
                       else { format!("((_ zero_extend {pad}) {})", r.s) },
                    width: w,
                    signed: false,
                })
            }
            "sext" => {
                let w = target_w.ok_or_else(|| CompileError::general(
                    ".sext<N>() requires a constant width argument", span,
                ))?;
                if w < r.width {
                    return Err(CompileError::general(
                        ".sext<N>() target must be ≥ current width", span,
                    ));
                }
                let pad = w - r.width;
                Ok(SmtTerm {
                    s: if pad == 0 { r.s.clone() }
                       else { format!("((_ sign_extend {pad}) {})", r.s) },
                    width: w,
                    signed: true,
                })
            }
            "resize" => {
                let w = target_w.ok_or_else(|| CompileError::general(
                    ".resize<N>() requires a constant width argument", span,
                ))?;
                let signed = r.signed;
                Ok(coerce(r, w, signed))
            }
            _ => Err(CompileError::general(
                &format!("method `.{n}()` not supported by arch formal v1"),
                span,
            )),
        }
    }

    // ── Property solving ─────────────────────────────────────────────────────

    fn run_property(
        &self,
        prop: &PropertyDecl,
        base: &str,
        args: &FormalArgs,
    ) -> Result<PropertyResult, CompileError> {
        // Encode the property at each cycle 0..=bound.
        let mut per_cycle: Vec<String> = Vec::with_capacity(args.bound as usize + 1);
        for t in 0..=args.bound {
            let term = self.encode_expr(&prop.expr, t, Some((1, false)))?;
            per_cycle.push(as_bv1_bool(&term));
        }

        // Build the check. For Assert, we want to find ANY violation:
        //   (assert (or (= p_0 #b0) (= p_1 #b0) ...))
        // For Cover, we want to find ANY hit:
        //   (assert (or (= p_0 #b1) (= p_1 #b1) ...))
        let matcher = match prop.kind {
            AssertKind::Assert => "#b0",
            AssertKind::Cover => "#b1",
        };
        let disjuncts: Vec<String> = per_cycle.iter().enumerate()
            .map(|(_i, p)| format!("(= {p} {matcher})"))
            .collect();
        let assertion = if disjuncts.len() == 1 {
            disjuncts.into_iter().next().unwrap()
        } else {
            format!("(or {})", disjuncts.join(" "))
        };

        // Compose final SMT text
        let mut smt = String::with_capacity(base.len() + 256);
        smt.push_str(base);
        smt.push_str(&format!("\n; ── property `{}` ({:?}) ──\n", prop.name, prop.kind));
        smt.push_str(&format!("(assert {assertion})\n"));
        smt.push_str("(check-sat)\n");
        // We always emit get-model; the solver will ignore it on unsat/unknown for most tools.
        // To be safe wrap with a push/pop so get-model only runs meaningfully.
        // Actually z3 returns "model is not available" on unsat which we tolerate.
        smt.push_str("(get-model)\n");

        // Shell out
        let sr = invoke_solver(&args.solver, &smt, args.timeout).map_err(|e| {
            CompileError::general(&format!("solver error: {e}"), prop.span)
        })?;

        // Parse result
        let first_word = sr.stdout.split_ascii_whitespace().next().unwrap_or("");
        let status = match first_word {
            "sat" => {
                // Find earliest cycle where per_cycle[i] equals matcher.
                let model = sr.stdout.splitn(2, '\n').nth(1).unwrap_or("").to_string();
                let assignments = parse_model(&model);
                // Determine failing cycle by evaluating per_cycle against the model.
                let failing_cycle = find_first_failing_cycle(&prop.kind, &prop.expr, self, &assignments, args.bound);
                let cex = render_counterexample(&prop.name, failing_cycle, self, &assignments, args.bound);
                match prop.kind {
                    AssertKind::Assert => PropertyStatus::Refuted(failing_cycle),
                    AssertKind::Cover  => PropertyStatus::Hit(failing_cycle),
                }
                .with_cex(cex)
            }
            "unsat" => match prop.kind {
                AssertKind::Assert => PropertyStatus::Proved(args.bound).with_cex(None),
                AssertKind::Cover  => PropertyStatus::NotReached(args.bound).with_cex(None),
            },
            _ => PropertyStatus::Inconclusive(
                if sr.stdout.contains("timeout") || !sr.stderr.is_empty() {
                    format!("solver returned `{first_word}`: {}{}", sr.stdout, sr.stderr).trim().to_string()
                } else {
                    format!("solver returned `{first_word}`")
                },
            ).with_cex(None),
        };

        Ok(PropertyResult {
            name: prop.name.clone(),
            kind: prop.kind.clone(),
            status: status.status,
            counterexample: status.cex,
        })
    }
}

// Helper: associate a counter-example with a status without double-wrapping.
struct StatusWithCex { status: PropertyStatus, cex: Option<String> }

impl PropertyStatus {
    fn with_cex(self, cex: Option<String>) -> StatusWithCex { StatusWithCex { status: self, cex } }
}

// ── SMT value helpers ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SmtTerm {
    s: String,
    width: u32,
    signed: bool,
}

fn bv_lit(value: u64, width: u32) -> String {
    // Prefer hex for widths divisible by 4, else decimal form.
    if width % 4 == 0 && width <= 64 {
        let digits = (width / 4) as usize;
        let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
        format!("#x{:0width$x}", value & mask, width = digits)
    } else if width <= 64 {
        let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
        format!("(_ bv{} {})", value & mask, width)
    } else {
        format!("(_ bv{value} {width})")
    }
}

fn bv_zero(width: u32) -> String { bv_lit(0, width) }

fn bv_all_ones(width: u32) -> String {
    if width <= 64 {
        let v = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
        bv_lit(v, width)
    } else {
        format!("(bvnot {})", bv_zero(width))
    }
}

/// Build an `Expr` for a sized literal with the given bit width + value.
/// Used by credit_channel state registration to synthesize reset
/// expressions for the lifted regs.
fn mk_sized_lit(width: u32, value: u64, span: Span) -> Expr {
    Expr::new(ExprKind::Literal(LitKind::Sized(width, value)), span)
}

fn mk_ident(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Ident(name.to_string()), span)
}

fn mk_bin(op: BinOp, a: Expr, b: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Binary(op, Box::new(a), Box::new(b)), span)
}

fn mk_not(a: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(a)), span)
}

fn lit_to_term(l: &LitKind) -> SmtTerm {
    match l {
        LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => {
            // Intrinsic width = bit-length, or 1 for value 0.
            let w = if *v == 0 { 1 } else { 64 - v.leading_zeros() };
            SmtTerm { s: bv_lit(*v, w), width: w, signed: false }
        }
        LitKind::Sized(w, v) => SmtTerm { s: bv_lit(*v, *w), width: *w, signed: false },
    }
}

/// Coerce `t` to `(width, signed)` via sign/zero extend or extract.
fn coerce(t: SmtTerm, width: u32, signed: bool) -> SmtTerm {
    if t.width == width {
        return SmtTerm { signed, ..t };
    }
    if t.width < width {
        let pad = width - t.width;
        let op = if t.signed { "sign_extend" } else { "zero_extend" };
        SmtTerm {
            s: format!("((_ {op} {pad}) {})", t.s),
            width,
            signed,
        }
    } else {
        SmtTerm {
            s: format!("((_ extract {} 0) {})", width - 1, t.s),
            width,
            signed,
        }
    }
}

/// Force a term to a 1-bit BV (for logical ops). Width-N ≠0 → 1, ==0 → 0.
fn as_bv1_bool(t: &SmtTerm) -> String {
    if t.width == 1 {
        t.s.clone()
    } else {
        let zero = bv_zero(t.width);
        format!("(ite (= {} {zero}) #b0 #b1)", t.s)
    }
}

/// Convert a 1-bit BV term into an SMT Bool (`(= x #b1)`).
fn as_bool(t: &SmtTerm) -> String {
    format!("(= {} #b1)", as_bv1_bool(t))
}

// ── Expr helpers ─────────────────────────────────────────────────────────────

fn target_root_ident(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(n) => Some(n.clone()),
        _ => None,
    }
}

fn collect_idents(expr: &Expr, out: &mut HashSet<String>) {
    use ExprKind::*;
    match &expr.kind {
        Ident(n) => { out.insert(n.clone()); }
        Binary(_, a, b) => { collect_idents(a, out); collect_idents(b, out); }
        Unary(_, a) => collect_idents(a, out),
        Ternary(c, t, e) => { collect_idents(c, out); collect_idents(t, out); collect_idents(e, out); }
        MethodCall(recv, _, args) => {
            collect_idents(recv, out);
            for a in args { collect_idents(a, out); }
        }
        BitSlice(b, hi, lo) => { collect_idents(b, out); collect_idents(hi, out); collect_idents(lo, out); }
        PartSelect(b, s, w, _) => { collect_idents(b, out); collect_idents(s, out); collect_idents(w, out); }
        Concat(es) => for e in es { collect_idents(e, out); }
        Repeat(n, x) => { collect_idents(n, out); collect_idents(x, out); }
        Signed(e) | Unsigned(e) | Clog2(e) | Onehot(e) => collect_idents(e, out),
        Cast(e, _) | FieldAccess(e, _) | Index(e, _) => collect_idents(e, out),
        _ => {}
    }
}

fn and_all(conds: &[Expr]) -> Expr {
    if conds.is_empty() {
        return Expr::new(ExprKind::Bool(true), Span { start: 0, end: 0 });
    }
    let mut acc = conds[0].clone();
    for c in conds.iter().skip(1) {
        let span = Span { start: acc.span.start.min(c.span.start), end: acc.span.end.max(c.span.end) };
        acc = Expr::new(ExprKind::Binary(BinOp::And, Box::new(acc), Box::new(c.clone())), span);
    }
    acc
}

fn not_expr(e: Expr) -> Expr {
    let span = e.span;
    Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(e)), span)
}

fn s_span(s: &Stmt) -> Span {
    match s {
        Stmt::Assign(a) => a.span,
        Stmt::IfElse(ie) => ie.span,
        Stmt::Match(m) => m.span,
        Stmt::Log(l) => l.span,
        Stmt::For(f) => f.span,
        Stmt::Init(i) => i.span,
        Stmt::WaitUntil(_, sp) => *sp,
        Stmt::DoUntil { span, .. } => *span,
    }
}

fn e_display(e: &CompileError, _sp: Span) -> String { format!("{e}") }

/// Minimal constant folder for compile-time expressions.
/// Handles literals, param refs, and common arithmetic.
fn fold_const_expr(expr: &Expr, params: &HashMap<String, u64>) -> Option<u64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v))
        | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v),
        ExprKind::Ident(n) => params.get(n).copied(),
        ExprKind::Binary(op, a, b) => {
            let va = fold_const_expr(a, params)?;
            let vb = fold_const_expr(b, params)?;
            Some(match op {
                BinOp::Add | BinOp::AddWrap => va.wrapping_add(vb),
                BinOp::Sub | BinOp::SubWrap => va.wrapping_sub(vb),
                BinOp::Mul | BinOp::MulWrap => va.wrapping_mul(vb),
                BinOp::Div => if vb == 0 { return None; } else { va / vb },
                BinOp::Mod => if vb == 0 { return None; } else { va % vb },
                BinOp::BitAnd => va & vb,
                BinOp::BitOr  => va | vb,
                BinOp::BitXor => va ^ vb,
                BinOp::Shl    => va << (vb & 63),
                BinOp::Shr    => va >> (vb & 63),
                _ => return None,
            })
        }
        ExprKind::Unary(UnaryOp::Neg, a) => {
            let v = fold_const_expr(a, params)?;
            Some(v.wrapping_neg())
        }
        ExprKind::Clog2(inner) => {
            let v = fold_const_expr(inner, params)?;
            Some(if v <= 1 { 1 } else { 64 - (v - 1).leading_zeros() as u64 })
        }
        _ => None,
    }
}

// ── Solver invocation ────────────────────────────────────────────────────────

struct SolverResult {
    stdout: String,
    stderr: String,
}

fn invoke_solver(solver: &str, smt: &str, timeout_s: u32) -> std::io::Result<SolverResult> {
    let (prog, args): (&str, Vec<String>) = match solver {
        "z3" => ("z3", vec![
            "-in".to_string(),
            format!("-T:{timeout_s}"),
            "-smt2".to_string(),
        ]),
        "boolector" => ("boolector", vec![
            "--smt2".to_string(),
            "-m".to_string(),
            format!("--time={timeout_s}"),
        ]),
        "bitwuzla" => ("bitwuzla", vec![
            "--produce-models=true".to_string(),
            // bitwuzla -t takes milliseconds.
            format!("-t"), format!("{}", timeout_s * 1000),
        ]),
        other => ("z3", vec!["-in".to_string(), format!("-T:{timeout_s}"), format!("--solver={other}")]),
    };

    let mut child = Command::new(prog)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(smt.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    Ok(SolverResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

// ── Model parsing ────────────────────────────────────────────────────────────

/// Parse a Z3/Boolector/Bitwuzla `(get-model)` response into signal_cycle → u64.
///
/// Handles the common patterns emitted by each solver:
///   Z3:        `(define-fun NAME () (_ BitVec W)\n    #xHH)`  (newline inside!)
///   Boolector: `(define-fun NAME () (_ BitVec W) #bHH)`
///   Bitwuzla:  `(define-fun NAME () (_ BitVec W) #xHH)`
///
/// We normalize whitespace to a single space and then extract `(define-fun
/// NAME ... VAL)` groups by tracking paren depth.
fn parse_model(text: &str) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    // Flatten newlines / tabs into spaces for simpler scanning.
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .collect();

    // Walk the string looking for "(define-fun " — then capture the balanced
    // parenthesized form that follows.
    let bytes = flat.as_bytes();
    let needle = b"(define-fun ";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // Find the opening paren of the overall group is at `i`.
            let mut depth = 0i32;
            let mut j = i;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 { break; }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= bytes.len() { break; }
            // group spans i..=j, inclusive of both parens.
            let inner = &flat[i + needle.len()..j];
            // inner: `NAME () (_ BitVec W) VAL`
            // Extract name (first whitespace-separated token).
            let mut name_end = 0;
            for (k, c) in inner.char_indices() {
                if c.is_whitespace() { name_end = k; break; }
            }
            if name_end == 0 {
                i = j + 1;
                continue;
            }
            let name = &inner[..name_end];
            let rest = inner[name_end..].trim();
            // The value is whatever follows the sort `(_ BitVec W)` (or a plain
            // sort keyword). Find the *last* balanced s-expression or literal.
            if let Some(v) = extract_last_bv_value(rest) {
                out.insert(name.to_string(), v);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Given "() (_ BitVec 8) #x0f" or "() (_ BitVec 1) #b0", return 0xf or 0.
fn extract_last_bv_value(rest: &str) -> Option<u64> {
    // Skip the first `()`, then the sort. Everything after the sort's closing
    // paren (or non-paren sort token) is the value.
    let s = rest.trim_start();
    let s = s.strip_prefix("()")?.trim_start();
    // Skip sort: either `(_ BitVec W)` or a bare word.
    let after_sort = if let Some(rem) = s.strip_prefix('(') {
        // balanced-paren skip
        let bytes = rem.as_bytes();
        let mut depth = 1i32;
        let mut k = 0usize;
        while k < bytes.len() && depth > 0 {
            match bytes[k] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            k += 1;
        }
        &rem[k..]
    } else {
        // bare word — skip until whitespace
        let idx = s.find(char::is_whitespace).unwrap_or(s.len());
        &s[idx..]
    };
    let val = after_sort.trim();
    parse_bv_literal(val)
}

fn parse_bv_literal(s: &str) -> Option<u64> {
    let s = s.trim().trim_end_matches(')').trim();
    if let Some(hex) = s.strip_prefix("#x") {
        return u64::from_str_radix(hex, 16).ok();
    }
    if let Some(bin) = s.strip_prefix("#b") {
        return u64::from_str_radix(bin, 2).ok();
    }
    // `(_ bv12345 8)` — with or without the surrounding parens.
    let core = s.trim_start_matches('(').trim();
    if let Some(rest) = core.strip_prefix("_ bv") {
        let val = rest.split_whitespace().next()?;
        return val.parse::<u64>().ok();
    }
    None
}

// ── Counterexample rendering ────────────────────────────────────────────────

fn find_first_failing_cycle(
    kind: &AssertKind,
    expr: &Expr,
    ctx: &FormalCtx,
    assignments: &HashMap<String, u64>,
    bound: u32,
) -> u32 {
    let target_bit = matches!(kind, AssertKind::Cover) as u64; // cover: want 1; assert: want 0 (failing)
    for t in 0..=bound {
        let v = eval_expr_numeric(expr, t, ctx, assignments).unwrap_or(0);
        let bit = v & 1;
        if bit == target_bit {
            return t;
        }
    }
    bound
}

fn render_counterexample(
    prop_name: &str,
    cycle: u32,
    ctx: &FormalCtx,
    assignments: &HashMap<String, u64>,
    _bound: u32,
) -> Option<String> {
    let mut lines = Vec::new();
    lines.push(format!("Counterexample for `{prop_name}` at cycle {cycle}:"));
    lines.push(String::new());
    // Header
    let mut names: Vec<String> = Vec::new();
    names.push(ctx.reset.name.clone());
    names.extend(ctx.inputs.iter().filter(|n| *n != &ctx.reset.name).cloned());
    names.extend(ctx.regs.iter().cloned());
    let header: Vec<String> = std::iter::once("cycle".to_string())
        .chain(names.iter().cloned()).collect();
    lines.push(header.join("  "));

    let start = cycle.saturating_sub(2);
    for t in start..=cycle {
        let mut row = vec![format!("{t:>5}")];
        for n in &names {
            let key = format!("{n}_{t}");
            let val = assignments.get(&key).copied().unwrap_or(0);
            row.push(format!("0x{val:x}"));
        }
        lines.push(row.join("  "));
    }
    Some(lines.join("\n"))
}

fn eval_expr_numeric(
    expr: &Expr,
    t: u32,
    ctx: &FormalCtx,
    assignments: &HashMap<String, u64>,
) -> Option<u64> {
    use ExprKind::*;
    match &expr.kind {
        Literal(LitKind::Dec(v)) | Literal(LitKind::Hex(v)) | Literal(LitKind::Bin(v))
        | Literal(LitKind::Sized(_, v)) => Some(*v),
        Bool(b) => Some(if *b { 1 } else { 0 }),
        Ident(n) => {
            if let Some(v) = ctx.params.get(n) { return Some(*v); }
            if let Some(val) = ctx.let_bindings.get(n) {
                return eval_expr_numeric(val, t, ctx, assignments);
            }
            assignments.get(&format!("{n}_{t}")).copied()
        }
        Binary(op, a, b) => {
            let va = eval_expr_numeric(a, t, ctx, assignments)?;
            let vb = eval_expr_numeric(b, t, ctx, assignments)?;
            Some(match op {
                BinOp::Add | BinOp::AddWrap => va.wrapping_add(vb),
                BinOp::Sub | BinOp::SubWrap => va.wrapping_sub(vb),
                BinOp::Mul | BinOp::MulWrap => va.wrapping_mul(vb),
                BinOp::Div => if vb == 0 { 0 } else { va / vb },
                BinOp::Mod => if vb == 0 { 0 } else { va % vb },
                BinOp::Eq => (va == vb) as u64,
                BinOp::Neq => (va != vb) as u64,
                BinOp::Lt => (va < vb) as u64,
                BinOp::Gt => (va > vb) as u64,
                BinOp::Lte => (va <= vb) as u64,
                BinOp::Gte => (va >= vb) as u64,
                BinOp::And => ((va != 0) && (vb != 0)) as u64,
                BinOp::Or  => ((va != 0) || (vb != 0)) as u64,
                BinOp::BitAnd => va & vb,
                BinOp::BitOr  => va | vb,
                BinOp::BitXor => va ^ vb,
                BinOp::Shl => va << (vb & 63),
                BinOp::Shr => va >> (vb & 63),
                BinOp::Implies => ((va == 0) || (vb != 0)) as u64,
            })
        }
        Unary(op, a) => {
            let v = eval_expr_numeric(a, t, ctx, assignments)?;
            Some(match op {
                UnaryOp::Not => (v == 0) as u64,
                UnaryOp::BitNot => !v,
                UnaryOp::Neg => v.wrapping_neg(),
                UnaryOp::RedAnd => (v.count_ones() >= 1 && (v + 1).is_power_of_two()) as u64,
                UnaryOp::RedOr => (v != 0) as u64,
                UnaryOp::RedXor => (v.count_ones() & 1) as u64,
            })
        }
        Ternary(c, tt, ee) => {
            let cv = eval_expr_numeric(c, t, ctx, assignments)?;
            if cv != 0 { eval_expr_numeric(tt, t, ctx, assignments) }
            else       { eval_expr_numeric(ee, t, ctx, assignments) }
        }
        _ => None,
    }
}

// ── User-visible report ──────────────────────────────────────────────────────

fn render_report(results: &[PropertyResult]) {
    eprintln!();
    eprintln!("=== arch formal report ===");
    for r in results {
        let (tag, detail) = match &r.status {
            PropertyStatus::Proved(n) => ("PROVED", format!("up to bound {n}")),
            PropertyStatus::Refuted(c) => ("REFUTED", format!("at cycle {c}")),
            PropertyStatus::Hit(c) => ("HIT", format!("at cycle {c}")),
            PropertyStatus::NotReached(n) => ("NOT REACHED", format!("within bound {n}")),
            PropertyStatus::Inconclusive(why) => ("INCONCLUSIVE", why.clone()),
        };
        eprintln!("[{:?}] {:<24} {}  — {}", r.kind, r.name, tag, detail);
        if let Some(cex) = &r.counterexample {
            for line in cex.lines() {
                eprintln!("    {line}");
            }
        }
    }
    eprintln!();
}
