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
use crate::construct_formal_ir::{
    ConstructFormalModel, CreditChannelFormalSpec, CreditChannelRole, FormalSignalKind,
};
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
    /// Float special-value profile — selects the canonical-NaN constants in
    /// the inlined float helper define-funs (same flag as build/sim).
    pub fp_compat: crate::FpCompat,
    /// Engine for `assert<bound_err>` properties. Currently only "gappa".
    pub error_engine: String,
}

#[derive(Debug, Clone)]
pub enum PropertyStatus {
    Proved(u32),          // bound
    Refuted(u32),         // cycle
    Hit(u32),             // cycle
    NotReached(u32),      // bound
    Inconclusive(String), // reason
    /// `assert<bound_err>` proved by the error engine; the string carries
    /// the engine's derived enclosure for the error term (best bound).
    ProvedEnclosure(String),
    /// A proof that holds only because its premise is never exercised — a
    /// soundness trap, not a pass. Two causes, distinguished by the string:
    /// jointly-unsatisfiable `assume` clauses (empty state space), or an
    /// implication whose antecedent is unreachable (trigger never fires).
    Vacuous(String),
    /// The sat-side dual of `Vacuous`: the solver claimed a violation (or
    /// cover hit), but independent replay of the returned model — the same
    /// property expression evaluated concretely, floats via the fp_ir
    /// interpreter over the identical operator definitions the query
    /// embedded — shows the property is NOT violated at any cycle. That
    /// contradiction means the BMC query generation itself is unsound: an
    /// internal compiler bug, not a design error.
    EncodingUnsound(String),
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
        let mut any_unsound = false;
        let mut any_bad = false;
        let mut any_incon = false;
        for r in &self.results {
            match &r.status {
                PropertyStatus::Proved(_)
                | PropertyStatus::Hit(_)
                | PropertyStatus::ProvedEnclosure(_) => {}
                PropertyStatus::Refuted(_)
                | PropertyStatus::NotReached(_)
                | PropertyStatus::Vacuous(_) => any_bad = true,
                PropertyStatus::Inconclusive(_) => any_incon = true,
                // Highest precedence and a dedicated code: a compiler bug is
                // categorically different from a design bug (1), so CI can
                // hard-fail on 3 while design-verification flows tolerate 1.
                PropertyStatus::EncodingUnsound(_) => any_unsound = true,
            }
        }
        if any_unsound {
            3
        } else if any_bad {
            1
        } else if any_incon {
            2
        } else {
            0
        }
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
    //    doc/archive/plan_hierarchical_formal.md for the design.
    let flat_module: ModuleDecl;
    let mut carried_credit_sites: Vec<CarriedCreditSite> = Vec::new();
    let encode_module: &ModuleDecl = if module
        .body
        .iter()
        .any(|b| matches!(b, ModuleBodyItem::Inst(_)))
    {
        let out = flatten_for_formal(ast, module, symbols)?;
        flat_module = out.module;
        carried_credit_sites = out.carried_sites;
        &flat_module
    } else {
        module
    };

    // 3. Build encoder state
    let mut ctx = FormalCtx::new(encode_module, symbols);
    ctx.fp_compat = args.fp_compat;
    ctx.carried_credit_sites = carried_credit_sites;
    ctx.preprocess()?;

    // 4. Emit SMT-LIB2 (header + declarations + transitions + comb)
    let base = ctx.emit_base(args.bound)?;

    // 4. Optionally dump
    if let Some(path) = &args.emit_smt {
        std::fs::write(path, &base).map_err(|e| {
            CompileError::general(
                &format!("failed to write --emit-smt output: {e}"),
                module.span,
            )
        })?;
    }

    // 4b. Vacuity guard. `assume` clauses are conjoined into `base`, so if
    // `base` is itself UNSAT the constrained state space is empty and EVERY
    // assert/cover would return `unsat` on its negated-property miter —
    // reporting a vacuous PROVED. Detect it with one satisfiability check on
    // the constrained transition system (no property) and, if unsatisfiable,
    // mark all solver-path properties Vacuous instead of trusting the proof.
    // Only meaningful when there are assumes (an unconstrained system is
    // trivially satisfiable). bound_err properties encode their own
    // hypotheses separately and are checked there.
    let vacuous = if !ctx.assumes.is_empty() {
        let probe = format!("{base}(check-sat)\n");
        let sr = invoke_solver(&args.solver, &probe, args.timeout)
            .map_err(|e| CompileError::general(&format!("solver error: {e}"), module.span))?;
        sr.stdout.split_ascii_whitespace().next() == Some("unsat")
    } else {
        false
    };

    // 5. For each assert/cover, run one (push)/(check-sat)/(pop) scope
    let mut results = Vec::new();
    for prop in ctx.properties.clone().iter() {
        if vacuous && prop.engine != crate::ast::AssertEngine::BoundErr {
            results.push(PropertyResult {
                name: prop.name.clone(),
                kind: prop.kind.clone(),
                status: PropertyStatus::Vacuous(
                    "`assume` clauses are jointly unsatisfiable — the constrained state space is empty, so every property proves vacuously".to_string(),
                ),
                counterexample: None,
            });
            continue;
        }
        let res = if prop.engine == crate::ast::AssertEngine::BoundErr {
            ctx.run_bound_err(prop, args)?
        } else {
            ctx.run_property(prop, &base, args)?
        };
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
                    let Some(bi) = &sp.bus_info else {
                        continue;
                    };
                    let Some(parent_expr) = port_map.get(&sp.name.name) else {
                        continue;
                    };
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
                let mut locals: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
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
                // `port reg o: out T` (and its `out pipe_reg<T,1>` spelling)
                // is a PortDecl, not a RegDecl body item, so the body walk
                // below never sees it and no register is emitted for it.
                // Left alone, the sub's `o <= ...` would be port-mapped
                // straight onto the parent signal, land in `reg_writes`
                // under a name that is not in `self.regs`, and be silently
                // DROPPED by emit_base — leaving the parent signal declared
                // but unconstrained, i.e. a free variable that admits
                // spurious counterexamples (issue #821).
                //
                // Model it the way the RTL actually behaves: a register
                // local to the instance, driving the parent signal
                // combinationally. Treating the port name as a local makes
                // every reference inside the sub (reads and the seq write)
                // resolve to `<inst>_<port>`.
                let mut carried_port_regs: Vec<(&PortDecl, String)> = Vec::new();
                for p in &sub.ports {
                    let Some(ri) = &p.reg_info else { continue };
                    if ri.latency != 1 {
                        return Err(CompileError::general(
                            &format!(
                                "hierarchical formal: inst `{}` port `{}` declares `pipe_reg<_, {}>`; \
                                 `arch formal` v1 models only single-cycle port registers (latency 1). \
                                 Refactor the sub-module to expose an explicit `reg` chain.",
                                inst.name.name, p.name.name, ri.latency,
                            ),
                            inst.span,
                        ));
                    }
                    let Some(parent_expr) = port_map.get(&p.name.name) else {
                        continue;
                    };
                    let ExprKind::Ident(parent_name) = &parent_expr.kind else {
                        return Err(CompileError::general(
                            &format!(
                                "hierarchical formal: inst `{}.{}` is a registered output port and \
                                 must connect to a simple identifier in v1 (got a complex \
                                 expression); refactor the parent to a named wire",
                                inst.name.name, p.name.name,
                            ),
                            inst.span,
                        ));
                    };
                    locals.insert(p.name.name.clone());
                    carried_port_regs.push((p, parent_name.clone()));
                }
                // `subst_expr_for_formal` consults `port_map` BEFORE `locals`,
                // so a carried port reg must be dropped from the port map or
                // every reference to it — including the `seq` write that is
                // the whole point — would still be rewritten to the parent
                // signal and dropped. Removing it lets the `locals` path
                // rewrite `o` to `<inst>_o`; the parent connection is driven
                // instead by the `let` emitted alongside the RegDecl below.
                for (p, _) in &carried_port_regs {
                    port_map.remove(&p.name.name);
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
                            let rewritten_value =
                                subst_expr_for_formal(&lb.value, &port_map, &locals, &prefix);
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
                                    name: Ident::new(
                                        format!("{prefix}{}", lb.name.name),
                                        lb.name.span,
                                    ),
                                    ty: lb.ty.clone(),
                                    value: rewritten_value,
                                    span: lb.span,
                                    destructure_fields: Vec::new(),
                                }));
                            }
                        }
                        ModuleBodyItem::CombBlock(cb) => {
                            let new_stmts: Vec<Stmt> = cb
                                .stmts
                                .iter()
                                .map(|s| subst_comb_stmt_for_formal(s, &port_map, &locals, &prefix))
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
                            let new_init = rd
                                .init
                                .as_ref()
                                .map(|e| subst_expr_for_formal(e, &port_map, &locals, &prefix));
                            let new_reset =
                                subst_reg_reset_for_formal(&rd.reset, &port_map, &locals, &prefix);
                            new_body.push(ModuleBodyItem::RegDecl(RegDecl {
                                name: Ident::new(format!("{prefix}{}", rd.name.name), rd.name.span),
                                ty: rd.ty.clone(),
                                init: new_init,
                                reset: new_reset,
                                guard: rd.guard.clone(),
                                multicycle: rd.multicycle,
                                span: rd.span,
                            }));
                        }
                        ModuleBodyItem::RegBlock(rb) => {
                            // Clock ident: substitute via port_map (sub's
                            // `clk` port binds to parent's clock via the
                            // inst connection).
                            let clock = resolve_port_ident_for_formal(
                                &rb.clock,
                                &port_map,
                                &inst.name.name,
                            )?;
                            let new_stmts: Vec<Stmt> = rb
                                .stmts
                                .iter()
                                .map(|s| subst_stmt_for_formal(s, &port_map, &locals, &prefix))
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

                // Emit the carried `port reg`s collected above: one prefixed
                // RegDecl each, plus a let binding so the parent-side signal
                // observes the registered value. `reg <inst>_<port>` +
                // `let <parent> = <inst>_<port>` is exactly what `port reg`
                // means in the RTL (always_ff drives it; the connection
                // observes it). See the #821 note above.
                for (p, parent_name) in carried_port_regs {
                    let ri = p.reg_info.as_ref().expect("collected with reg_info");
                    let new_init = ri
                        .init
                        .as_ref()
                        .map(|e| subst_expr_for_formal(e, &port_map, &locals, &prefix));
                    let new_reset =
                        subst_reg_reset_for_formal(&ri.reset, &port_map, &locals, &prefix);
                    let reg_name = format!("{prefix}{}", p.name.name);
                    new_body.push(ModuleBodyItem::RegDecl(RegDecl {
                        name: Ident::new(reg_name.clone(), p.name.span),
                        ty: p.ty.clone(),
                        init: new_init,
                        reset: new_reset,
                        guard: ri.guard.clone(),
                        multicycle: None,
                        span: p.span,
                    }));
                    new_body.push(ModuleBodyItem::LetBinding(LetBinding {
                        name: Ident::new(parent_name, p.name.span),
                        ty: Some(p.ty.clone()),
                        value: Expr {
                            kind: ExprKind::Ident(reg_name),
                            span: p.span,
                            parenthesized: false,
                        },
                        span: p.span,
                        destructure_fields: Vec::new(),
                    }));
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
    Ok(FlattenOutput {
        module: flat,
        carried_sites,
    })
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
            for s in &mut cb.stmts {
                rewrite_synth_idents_in_comb_stmt(s, remap);
            }
        }
        ModuleBodyItem::RegDecl(rd) => {
            if let Some(init) = &mut rd.init {
                rewrite_synth_idents_in_expr(init, remap);
            }
            match &mut rd.reset {
                RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) => {
                    rewrite_synth_idents_in_expr(val, remap);
                }
                RegReset::None => {}
            }
        }
        ModuleBodyItem::RegBlock(rb) => {
            for s in &mut rb.stmts {
                rewrite_synth_idents_in_stmt(s, remap);
            }
        }
        _ => {}
    }
}

fn rewrite_synth_idents_in_comb_stmt(
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
            for st in &mut ie.then_stmts {
                rewrite_synth_idents_in_comb_stmt(st, remap);
            }
            for st in &mut ie.else_stmts {
                rewrite_synth_idents_in_comb_stmt(st, remap);
            }
        }
        _ => {}
    }
}

fn rewrite_synth_idents_in_stmt(s: &mut Stmt, remap: &std::collections::HashMap<String, String>) {
    match s {
        Stmt::Assign(a) => {
            rewrite_synth_idents_in_expr(&mut a.target, remap);
            rewrite_synth_idents_in_expr(&mut a.value, remap);
        }
        Stmt::IfElse(ie) => {
            rewrite_synth_idents_in_expr(&mut ie.cond, remap);
            for st in &mut ie.then_stmts {
                rewrite_synth_idents_in_stmt(st, remap);
            }
            for st in &mut ie.else_stmts {
                rewrite_synth_idents_in_stmt(st, remap);
            }
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
            for x in xs {
                rewrite_synth_idents_in_expr(x, remap);
            }
        }
        Repeat(n, x) => {
            rewrite_synth_idents_in_expr(n, remap);
            rewrite_synth_idents_in_expr(x, remap);
        }
        FieldAccess(b, _) => rewrite_synth_idents_in_expr(b, remap),
        MethodCall(recv, _, args) => {
            rewrite_synth_idents_in_expr(recv, remap);
            for a in args {
                rewrite_synth_idents_in_expr(a, remap);
            }
        }
        FunctionCall(_, xs) => {
            for x in xs {
                rewrite_synth_idents_in_expr(x, remap);
            }
        }
        _ => {}
    }
}

fn subst_comb_stmt_for_formal(
    s: &Stmt,
    port_map: &std::collections::HashMap<String, Expr>,
    locals: &std::collections::HashSet<String>,
    prefix: &str,
) -> Result<Stmt, CompileError> {
    match s {
        Stmt::Assign(a) => {
            let target = subst_expr_for_formal(&a.target, port_map, locals, prefix);
            let value = subst_expr_for_formal(&a.value, port_map, locals, prefix);
            Ok(Stmt::Assign(Assign {
                target,
                value,
                span: a.span,
            }))
        }
        Stmt::IfElse(ie) => {
            let cond = subst_expr_for_formal(&ie.cond, port_map, locals, prefix);
            let then_stmts: Vec<Stmt> = ie
                .then_stmts
                .iter()
                .map(|s| subst_comb_stmt_for_formal(s, port_map, locals, prefix))
                .collect::<Result<_, _>>()?;
            let else_stmts: Vec<Stmt> = ie
                .else_stmts
                .iter()
                .map(|s| subst_comb_stmt_for_formal(s, port_map, locals, prefix))
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
                Stmt::For(f) => f.span,
                Stmt::Match(m) => m.span,
                Stmt::Log(l) => l.span,
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
        // in this slice. NOTE: nothing validates the bus's non-credit
        // content up front — `flatten_for_formal` only walks
        // `bus_info.credit_channels`. Unsupported usage is caught at the
        // point of use instead: reads in `encode_raw`'s FieldAccess arm,
        // writes in `check_assign_targets_registered()`.
        if p.bus_info.is_some() {
            continue;
        }
        // Scalar ports only.
        match &p.ty {
            TypeExpr::UInt(_)
            | TypeExpr::SInt(_)
            | TypeExpr::Bool
            | TypeExpr::Bit
            | TypeExpr::Clock(_)
            | TypeExpr::Reset(_, _) => {}
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

/// Substitute a seq-block Stmt. Mirrors the Stmt substituter but
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
            Ok(Stmt::Assign(Assign {
                target,
                value,
                span: a.span,
            }))
        }
        Stmt::IfElse(ie) => {
            let cond = subst_expr_for_formal(&ie.cond, port_map, locals, prefix);
            let then_stmts: Vec<Stmt> = ie
                .then_stmts
                .iter()
                .map(|s| subst_stmt_for_formal(s, port_map, locals, prefix))
                .collect::<Result<_, _>>()?;
            let else_stmts: Vec<Stmt> = ie
                .else_stmts
                .iter()
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
            if let Some(p) = port_map.get(name) {
                return p.clone();
            }
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
        Unary(op, e) => Unary(
            *op,
            Box::new(subst_expr_for_formal(e, port_map, locals, prefix)),
        ),
        Cast(e, ty) => Cast(
            Box::new(subst_expr_for_formal(e, port_map, locals, prefix)),
            ty.clone(),
        ),
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
            args.iter()
                .map(|a| subst_expr_for_formal(a, port_map, locals, prefix))
                .collect(),
        ),
        Concat(xs) => Concat(
            xs.iter()
                .map(|x| subst_expr_for_formal(x, port_map, locals, prefix))
                .collect(),
        ),
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
            xs.iter()
                .map(|x| subst_expr_for_formal(x, port_map, locals, prefix))
                .collect(),
        ),
        _ => return expr.clone(),
    };
    Expr {
        kind: new_kind,
        span: expr.span,
        parenthesized: expr.parenthesized,
    }
}

// ── Top-module selection ─────────────────────────────────────────────────────

fn select_top<'a>(
    ast: &'a SourceFile,
    requested: Option<&str>,
) -> Result<&'a ModuleDecl, CompileError> {
    // Visible modules = non-underscore-prefixed (hides `_<Name>_threads` helpers).
    let visible: Vec<&ModuleDecl> = ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Module(m) if !m.name.name.starts_with('_') => Some(m),
            _ => None,
        })
        .collect();

    if let Some(name) = requested {
        for m in ast.items.iter().filter_map(|it| match it {
            Item::Module(m) => Some(m),
            _ => None,
        }) {
            if m.name.name == name {
                return Ok(m);
            }
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
                &format!(
                    "multiple modules in input ({}); specify --top <Name>",
                    names.join(", ")
                ),
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
    /// Float helper tag ("f32"/"bf16"/"e4m3"/"e5m2") when the signal's HDL
    /// type is a float format. Drives operator dispatch to the inlined
    /// `arch_{tag}_*` define-funs.
    float: Option<&'static str>,
    /// True for the E8M0 block-scale type, which is NOT a float format (no
    /// sign, no mantissa, no zero) and therefore has `float: None` — but
    /// does have a NaN code, so `is_nan` needs to find it somehow.
    is_e8m0: bool,
}

/// Float helper tag of a scalar TypeExpr, if any.
/// Float dispatch tag of a surface type, or `None` if it is not a float.
///
/// Reads the canonical table, which closes a silent-failure chain: this
/// used to be a hand-written match ending in `_ => None`, so a float type
/// missing from it resolved to `None`, which left `SignalInfo::float` unset,
/// which made `expr_float_tag` return `None`, which the call sites turned
/// into `f32` via `unwrap_or("f32")`. A new format would then have been
/// encoded as FP32 — right through to the solver — with nothing anywhere
/// reporting a problem. Sourcing the tag from the table means a format that
/// exists at all resolves correctly.
fn float_tag_of(ty: &TypeExpr) -> Option<&'static str> {
    crate::fp_format::by_type_expr(ty).map(|f| f.tag)
}

/// Carrier width of a float dispatch tag.
///
/// Backed by the canonical format table rather than a hand-written match:
/// the old version ended in `_ => 8`, which is correct only while every
/// format narrower than `bf16` happens to be 8 bits. A 4- or 6-bit format
/// would have silently taken 8 here and mis-coerced every SMT term built
/// from it.
///
/// The `unreachable!` cannot fire for a tag produced by `float_tag_of` /
/// `expr_float_tag`, both of which draw from the same table; it exists so a
/// future format that is added to one and not the other fails loudly instead
/// of computing on a wrong width.
fn float_tag_width(tag: &str) -> u32 {
    match crate::fp_format::width_of_tag(tag) {
        Some(w) => w,
        None => unreachable!(
            "float dispatch tag `{tag}` has no row in fp_format::FORMATS — \
             add the format to the table"
        ),
    }
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
    engine: crate::ast::AssertEngine,
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
    /// Synthesized derived signals whose formal value is `source != 0`.
    derived_nonzero: HashMap<String, String>,
    /// `assume Name: expr;` input constraints — conjoined as hypotheses
    /// at every timestep of the QF_BV unroll, and read as interval
    /// hypotheses by the error-bound engine.
    assumes: Vec<Expr>,
    /// Any float-typed signal registered → prepend the float helper
    /// define-funs to every emitted query.
    uses_float: bool,
    /// Profile for the inlined helpers' canonical-NaN constants.
    fp_compat: crate::FpCompat,
}

#[derive(Debug, Clone)]
struct CombAssignFlat {
    target: String,   // flat name (e.g. "y" or "out[2]"); v1 supports ident targets only
    guard: Vec<Expr>, // stack of conditions (ANDed)
    value: Expr,
    /// Span of the originating assignment statement. Used by
    /// `check_assign_targets_registered` to point at an unsupported write,
    /// and by `emit_base`'s internal-error backstop.
    span: Span,
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
            reset: ResetInfo {
                name: "rst".to_string(),
                is_async: false,
                is_low: false,
            },
            params: HashMap::new(),
            enum_variants: HashMap::new(),
            properties: Vec::new(),
            comb_order: Vec::new(),
            credit_sites: Vec::new(),
            carried_credit_sites: Vec::new(),
            derived_nonzero: HashMap::new(),
            assumes: Vec::new(),
            uses_float: false,
            fp_compat: crate::FpCompat::Riscv,
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
            let depth = cs
                .meta
                .params
                .iter()
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
            let Some(bi) = &p.bus_info else {
                continue;
            };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name)
            else {
                continue;
            };
            for cc in &info.credit_channels {
                // Role flipping: on the target perspective the bus
                // reverses directions, so an `Out` channel role on the
                // initiator becomes the receiver on the target side.
                let is_sender = matches!(
                    (cc.role_dir, bi.perspective),
                    (Direction::Out, crate::ast::BusPerspective::Initiator)
                        | (Direction::In, crate::ast::BusPerspective::Target)
                );
                let depth = cc
                    .params
                    .iter()
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
        let mut both_sides: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut have_sender: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut have_receiver: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for s in &sites {
            let key = (s.port_name.clone(), s.meta.name.name.clone());
            if s.is_sender {
                have_sender.insert(key);
            } else {
                have_receiver.insert(key);
            }
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
            let role = if site.is_sender {
                CreditChannelRole::Sender
            } else {
                CreditChannelRole::Receiver
            };
            let model = ConstructFormalModel::credit_channel(&CreditChannelFormalSpec {
                port_name: port.clone(),
                channel_name: ch.clone(),
                role,
                depth,
                payload_width: cc_payload_width(&site.meta),
                merged: both_sides.contains(&(port.clone(), ch.clone())),
                span: site.meta.span,
            });
            self.apply_construct_formal_model(model);
        }
        Ok(())
    }

    fn apply_construct_formal_model(&mut self, model: ConstructFormalModel) {
        for sig in model.signals {
            let kind = match sig.kind {
                FormalSignalKind::Input => SignalKind::Input,
                FormalSignalKind::Output => SignalKind::Output,
                FormalSignalKind::Reg => SignalKind::Reg,
                FormalSignalKind::Wire => SignalKind::Wire,
            };
            if self.sigs.contains_key(&sig.name) {
                continue;
            }
            self.sigs.insert(
                sig.name.clone(),
                SignalInfo {
                    width: sig.width,
                    signed: sig.signed,
                    kind: kind.clone(),
                    float: None,
                    is_e8m0: false,
                },
            );
            match kind {
                SignalKind::Input => self.inputs.push(sig.name),
                SignalKind::Output => self.outputs.push(sig.name),
                SignalKind::Reg => self.regs.push(sig.name),
                SignalKind::Wire => self.wires.push(sig.name),
            }
        }
        for (name, value) in model.resets {
            self.reg_reset.insert(name, value);
        }
        for eq in model.comb_equations {
            let span = eq.value.span;
            self.comb_assigns.push(CombAssignFlat {
                target: eq.target,
                guard: Vec::new(),
                value: eq.value,
                span,
            });
        }
        for eq in model.next_equations {
            self.reg_writes
                .entry(eq.target)
                .or_default()
                .push((eq.cond, eq.value));
        }
        for derived in model.derived_nonzero {
            self.derived_nonzero.insert(derived.name, derived.source);
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
        self.reset = ResetInfo {
            name: rn,
            is_async,
            is_low,
        };

        // PR-hf4 item 1: collect credit_channel sites for later state
        // registration and SynthIdent resolution.
        self.collect_credit_channel_sites();
        // Register BV state, handshake signals, derived comb aliases, and
        // next-state equations per site.
        self.register_credit_channel_state()?;

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
            // scalar ports; we register them per-credit_channel in
            // `register_credit_channel_state()`. Skip generic scalar-only
            // handling. Non-credit_channel bus usage stays unsupported in
            // formal v1, and both directions are refused explicitly: reads
            // in `encode_raw`'s FieldAccess arm, writes in
            // `check_assign_targets_registered()` at the end of preprocess.
            // (Before that pass existed, a write panicked in `emit_base` on
            // `self.sigs[tgt]` — issue #818.)
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
            self.sigs.insert(
                port.name.name.clone(),
                SignalInfo {
                    width: w,
                    signed,
                    kind: kind.clone(),
                    float: float_tag_of(&port.ty),
                    is_e8m0: matches!(port.ty, TypeExpr::E8M0),
                },
            );
            if float_tag_of(&port.ty).is_some() || matches!(port.ty, TypeExpr::E8M0) {
                // E8M0 is not a float, but its conversions live in the same
                // inlined helper preamble, so it must request it too.
                self.uses_float = true;
            }
            match kind {
                SignalKind::Input => self.inputs.push(port.name.name.clone()),
                SignalKind::Output => self.outputs.push(port.name.name.clone()),
                _ => {}
            }
            // A `port reg o: out T` is both an output and a reg.
            if let Some(reg_info) = &port.reg_info {
                self.regs.push(port.name.name.clone());
                self.sigs.get_mut(&port.name.name).unwrap().kind = SignalKind::Reg;
                if let RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) =
                    &reg_info.reset
                {
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
                    self.sigs.insert(
                        r.name.name.clone(),
                        SignalInfo {
                            width: w,
                            signed,
                            kind: SignalKind::Reg,
                            float: float_tag_of(&r.ty),
                            is_e8m0: matches!(r.ty, TypeExpr::E8M0),
                        },
                    );
                    if float_tag_of(&r.ty).is_some() || matches!(r.ty, TypeExpr::E8M0) {
                        self.uses_float = true;
                    }
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
                    self.sigs.insert(
                        w.name.name.clone(),
                        SignalInfo {
                            width,
                            signed,
                            kind: SignalKind::Wire,
                            float: float_tag_of(&w.ty),
                            is_e8m0: matches!(w.ty, TypeExpr::E8M0),
                        },
                    );
                    if float_tag_of(&w.ty).is_some() || matches!(w.ty, TypeExpr::E8M0) {
                        self.uses_float = true;
                    }
                    self.wires.push(w.name.name.clone());
                }
                ModuleBodyItem::LetBinding(lb) => {
                    self.let_bindings
                        .insert(lb.name.name.clone(), lb.value.clone());
                }
                ModuleBodyItem::Assert(a) => {
                    if a.kind == AssertKind::Assume {
                        self.assumes.push(a.expr.clone());
                        continue;
                    }
                    let name = a
                        .name
                        .as_ref()
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| format!("prop_{}", a.span.start));
                    self.properties.push(PropertyDecl {
                        name,
                        kind: a.kind.clone(),
                        engine: a.engine,
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
                ModuleBodyItem::Function(_)
                | ModuleBodyItem::Resource(_)
                | ModuleBodyItem::TlmConnect(_) => {
                    // Ignore; v1 doesn't encode module-local functions
                }
                ModuleBodyItem::Inst(_) | ModuleBodyItem::Thread(_) => {
                    // Already handled above
                }
                ModuleBodyItem::TypeAlias(_) => {
                    // Type aliases are inlined by `type_alias::resolve_type_aliases`
                    // before formal codegen runs; they should never reach here.
                }
            }
        }

        // Reject assignment targets that no pass registered as a modelled
        // signal (issue #818 — today that means plain, non-credit_channel
        // bus fields). Must run after the body loop, since a `comb` block
        // may textually precede the `wire` decl it drives; and before
        // `comb_topo_order`, whose HashSet iteration would make the choice
        // of reported offender nondeterministic.
        self.check_assign_targets_registered()?;

        // Build comb-block topological order over wires + output ports
        self.comb_order = self.comb_topo_order()?;

        // Detect circular let references (simple DFS)
        self.check_let_cycles()?;

        Ok(())
    }

    /// Reject `comb`/`seq` assignment targets that no earlier pass registered
    /// as a modelled signal.
    ///
    /// Runs at the END of `preprocess`, so every `sigs` insert has already
    /// happened: credit_channel lifted state, ports, and `reg`/`wire` decls.
    /// Checking at push time in `walk_comb_stmt` would be source-order
    /// fragile — module body items are walked in ONE pass, and a `comb`
    /// block may legally precede the `wire` decl it drives.
    ///
    /// Why an error rather than a silent skip: an unregistered target is
    /// never declared (`emit_base`'s declaration loops iterate
    /// `inputs`/`outputs`/`regs`/`wires`) and can never be read either
    /// (`encode_raw`'s FieldAccess arm rejects reads of unregistered flat
    /// names), so dropping the equation would not corrupt the query — it
    /// would silently shrink the *design under proof* and still print
    /// PROVED. `arch formal` v1's contract is to error clearly on
    /// unsupported constructs, and the read path already does.
    ///
    /// `let` bindings are deliberately exempt: `encode_ident` inlines a
    /// let's value at every reference site, so an undeclared let needs no
    /// declaration-site equation. `flatten_for_formal` relies on this —
    /// sub-module locals become `<inst>_<name>` let bindings that are not
    /// in `sigs` — which is why `emit_base` skips them behind
    /// `self.sigs.get(tgt)`.
    fn check_assign_targets_registered(&self) -> Result<(), CompileError> {
        // (target, span, block-kind) for every unmodelled write.
        let mut offenders: Vec<(&str, Span, &'static str)> = Vec::new();

        for ca in &self.comb_assigns {
            if !self.sigs.contains_key(&ca.target) && !self.let_bindings.contains_key(&ca.target) {
                offenders.push((ca.target.as_str(), ca.span, "comb"));
            }
        }
        // A `seq` write must land on something `emit_base` will actually
        // emit a transition for, and that loop iterates `self.regs` — NOT
        // `self.sigs`. A target that is in `sigs` but is not a register is
        // therefore silently DROPPED, and because it *is* declared it
        // becomes an unconstrained free variable, admitting spurious
        // counterexamples. That is the #821 failure mode, and it is strictly
        // worse than a refusal, so check membership in `regs` here rather
        // than settling for `sigs`.
        for (target, writes) in &self.reg_writes {
            if self.regs.iter().any(|r| r == target) {
                continue;
            }
            let span = writes
                .first()
                .map(|(_, v)| v.span)
                .unwrap_or(self.module.span);
            let kind = if self.sigs.contains_key(target) || self.let_bindings.contains_key(target) {
                "seq-nonreg"
            } else {
                "seq"
            };
            offenders.push((target.as_str(), span, kind));
        }

        // `reg_writes` is a HashMap, so sort for a deterministic choice of
        // reported offender: earliest source position wins, name breaks ties.
        offenders.sort_by_key(|(name, sp, _)| (sp.start, sp.end, *name));
        match offenders.first() {
            None => Ok(()),
            Some(&(target, span, block)) => {
                Err(self.unregistered_target_error(target, span, block))
            }
        }
    }

    /// Diagnostic for an assignment target that is not a modelled signal.
    /// Writes to a bus port get a scope-specific message naming the port and
    /// field; anything else gets a generic "not a declared signal".
    fn unregistered_target_error(
        &self,
        target: &str,
        span: Span,
        block: &'static str,
    ) -> CompileError {
        for p in &self.module.ports {
            if p.bus_info.is_none() {
                continue;
            }
            let port = &p.name.name;
            let Some(field) = target.strip_prefix(&format!("{port}_")) else {
                continue;
            };
            return CompileError::general(
                &format!(
                    "assignment to bus signal `{port}.{field}` is not supported by \
                     `arch formal` v1 — only `credit_channel` signals on a bus port are \
                     modelled (`<port>.<channel>.send(..)` / `.no_send()` / `.pop()` / \
                     `.no_pop()`); plain bus signals, handshake groups and `tlm_method` \
                     bundles are not encoded yet"
                ),
                span,
            );
        }
        if block == "seq-nonreg" {
            // In `sigs` but not a register: `emit_base` would emit no
            // transition for it, so the write vanishes and the (declared)
            // signal is left unconstrained — a free variable that admits
            // spurious counterexamples. Refuse instead (issue #821).
            return CompileError::general(
                &format!(
                    "`seq` block assigns to `{target}`, which `arch formal` v1 does not \
                     model as a register — the write would be silently dropped and \
                     `{target}` left unconstrained, which can produce a spurious \
                     counterexample. Declare `{target}` as a `reg`, or drive it from one"
                ),
                span,
            );
        }
        CompileError::general(
            &format!(
                "`{block}` block assigns to `{target}`, which is not a signal \
                 `arch formal` v1 models — expected a `wire`, a `reg`, or an output \
                 port declared in this module"
            ),
            span,
        )
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
                for c in &ie.then_stmts {
                    self.collect_init_writes(c)?;
                }
                for c in &ie.else_stmts {
                    self.collect_init_writes(c)?;
                }
            }
            Stmt::Init(ib) => {
                for c in &ib.body {
                    self.collect_init_writes(c)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn walk_comb_stmt(&mut self, s: &Stmt, path: &[Expr]) -> Result<(), CompileError> {
        match s {
            Stmt::Assign(a) => {
                let name = match target_root_ident(&a.target) {
                    Some(n) => n,
                    None => {
                        return Err(CompileError::general(
                            "arch formal v1 only supports comb assignments to bare identifiers",
                            a.span,
                        ))
                    }
                };
                self.comb_assigns.push(CombAssignFlat {
                    target: name,
                    guard: path.to_vec(),
                    value: a.value.clone(),
                    span: a.span,
                });
            }
            Stmt::IfElse(ie) => {
                let mut then_path = path.to_vec();
                then_path.push(ie.cond.clone());
                for c in &ie.then_stmts {
                    self.walk_comb_stmt(c, &then_path)?;
                }
                let mut else_path = path.to_vec();
                else_path.push(not_expr(ie.cond.clone()));
                for c in &ie.else_stmts {
                    self.walk_comb_stmt(c, &else_path)?;
                }
            }
            Stmt::Match(m) => {
                return Err(CompileError::general(
                    "`match` inside `comb` blocks is not supported by `arch formal` v1 (rewrite as if/else or expression-level match)",
                    m.span,
                ));
            }
            Stmt::For(fl) => {
                return Err(CompileError::general(
                    "`for` inside `comb` blocks is not supported by `arch formal` v1 (unroll manually)",
                    fl.span,
                ));
            }
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("seq-only Stmt variant inside comb-context walker")
            }
            Stmt::Log(_) => { /* ignore */ }
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
            for g in &ca.guard {
                collect_idents(g, set);
            }
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
        if visited.contains(name) {
            return Ok(());
        }
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
            // Floats are BV carriers; operator semantics come from the
            // inlined proven define-funs (see emit_base).
            TypeExpr::FP32
            | TypeExpr::BF16
            | TypeExpr::FP8E4M3
            | TypeExpr::FP8E5M2
            | TypeExpr::FP4E2M1
            | TypeExpr::FP6E2M3
            | TypeExpr::FP6E3M2
            | TypeExpr::E8M0
            | TypeExpr::UE4M3 => Ok(()),
            // A block is ONE packed bit-vector, not an aggregate, so it fits
            // the `(_ BitVec W)`-per-signal model directly — unlike Vec.
            TypeExpr::ScaledVec(..) => Ok(()),
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
            TypeExpr::FP32 => Ok((32, false)),
            TypeExpr::BF16 => Ok((16, false)),
            TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 => Ok((8, false)),
            TypeExpr::FP4E2M1 => Ok((4, false)),
            TypeExpr::FP6E2M3 | TypeExpr::FP6E3M2 => Ok((6, false)),
            TypeExpr::E8M0 | TypeExpr::UE4M3 => Ok((8, false)),
            TypeExpr::ScaledVec(elem, n, scale) => {
                let n = fold_const_expr(n, &self.params).ok_or_else(|| {
                    CompileError::general(
                        "could not fold ScaledVec block size N to a compile-time constant",
                        span,
                    )
                })? as u32;
                let w = crate::fp_format::scaled_vec_width(elem, n, scale).ok_or_else(|| {
                    CompileError::general(
                        "ScaledVec element/scale type is not a valid block member",
                        span,
                    )
                })?;
                Ok((w, false))
            }
            TypeExpr::UInt(w) => {
                let width = fold_const_expr(w, &self.params).ok_or_else(|| {
                    CompileError::general(
                        "could not fold UInt<W> width to a compile-time constant",
                        span,
                    )
                })? as u32;
                if width == 0 {
                    return Err(CompileError::general("width of 0 is not supported", span));
                }
                Ok((width, false))
            }
            TypeExpr::SInt(w) => {
                let width = fold_const_expr(w, &self.params).ok_or_else(|| {
                    CompileError::general(
                        "could not fold SInt<W> width to a compile-time constant",
                        span,
                    )
                })? as u32;
                if width == 0 {
                    return Err(CompileError::general("width of 0 is not supported", span));
                }
                Ok((width, true))
            }
            TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => {
                Ok((1, false))
            }
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
        if self.uses_float {
            // Float operator semantics: the SAME QF_BV define-funs the SV
            // and the offline SMT proofs are rendered from (single-source
            // IR, machine-proved correctly rounded — tests/fp_v1/smt_proof).
            // User properties compose over proven operators; no FP theory
            // is involved. NOTE: a property whose cone includes a float
            // multiplier/fma at FP32 width can be SAT-hard; fp8/bf16 cones
            // and add/compare cones are tractable.
            out.push_str(&crate::fp_ir::render_smt(&crate::fp_ops::fp_functions(
                self.fp_compat,
            )));
        }
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
                if self.sigs[name].kind == SignalKind::Reg {
                    continue;
                }
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
                let assigns: Vec<&CombAssignFlat> = self
                    .comb_assigns
                    .iter()
                    .filter(|c| &c.target == tgt)
                    .collect();
                if assigns.is_empty() {
                    continue;
                }
                // Backstop: `check_assign_targets_registered` rejects
                // unregistered targets during preprocess, so reaching here
                // means that pass missed a case. Fail with a report-this
                // error rather than panicking on the index, and never skip
                // silently — a dropped equation would quietly shrink the
                // design under proof.
                let Some(info) = self.sigs.get(tgt) else {
                    return Err(CompileError::general(
                        &format!(
                            "internal error: comb target `{tgt}` reached SMT emission without \
                             being registered as a signal — `check_assign_targets_registered` \
                             should have rejected this during preprocess. Please report this \
                             design as an `arch formal` bug."
                        ),
                        assigns[0].span,
                    ));
                };
                // Build nested ite from the guard chain. Last unguarded write wins as default.
                let rhs = self.build_comb_ite(&assigns, t, info.width, info.signed)?;
                out.push_str(&format!("(assert (= {tgt}_{t} {rhs}))\n"));
            }
            out.push('\n');
        }

        // Register transition: r_{t+1} = ite(reset, reset_val, next_value)
        for t in 0..bound {
            out.push_str(&format!(
                "; ── register transition cycle {t}→{} ──\n",
                t + 1
            ));
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

        // `assume` clauses: conjoined as hypotheses at every timestep, so
        // both assert and cover properties see only constrained inputs.
        if !self.assumes.is_empty() {
            out.push_str("; assume clauses\n");
            for t in 0..=bound {
                for a in &self.assumes {
                    let enc = self.encode_expr(a, t, None)?;
                    out.push_str(&format!("(assert {})\n", as_bool(&enc)));
                }
            }
            out.push('\n');
        }

        Ok(out)
    }

    /// Build nested ite for a reg's next value at cycle t.
    fn reg_next(
        &self,
        reg: &str,
        t: u32,
        width: u32,
        signed: bool,
    ) -> Result<String, CompileError> {
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
            ScaledQuantize(..) => Err(CompileError::general(
                "`scaled_quantize` is not yet supported by `arch formal` \
                 (arch#884 phase 2b)",
                expr.span,
            )),
            LatencyAt(inner, _) => self.encode_raw(inner, t),
            // `op<pipelined, N>(...)` — the retimed staged datapath is proven
            // bit-identical to the single-cycle `op` for every input: the SMT
            // miter shows the register-shorted transfer function equals the
            // comb operator's model and the pipeline is balanced (Route A,
            // tests/fp_v1/smt_proof/staged_ops_miter.sh), and the Lean retiming
            // lemma bridges that to `output[t+N] = op(input[t])` (Route B,
            // proofs/lean_fp_equiv/ArchFpEquiv/StagedPipeline.lean; arch#968).
            // So encode it as exactly that comb operator — the value the
            // pipeline delivers. The earlier refusal was to avoid
            // misrepresenting an *unverified* pipeline as formally checked;
            // that verification now exists, so the comb encoding is faithful.
            // The pipe_reg latency is modeled the same way `arch formal` v1
            // models every `@N` register — the pre-existing single-cycle
            // approximation shared by all pipe_regs, pipelined or not — not a
            // new approximation introduced here.
            PipelinedCall(name, args, _stages) => {
                let comb = Expr {
                    kind: ExprKind::FunctionCall(name.clone(), args.clone()),
                    span: expr.span,
                    parenthesized: expr.parenthesized,
                };
                self.encode_raw(&comb, t)
            }
            // SVA `##N expr` — forward cycle-shift. Encode `expr` at
            // cycle `t + N`. run_property clamps max_t so this never
            // goes out of the unrolled range.
            SvaNext(n, inner) => self.encode_raw(inner, t + *n),
            // SynthIdent points at codegen-emitted state (credit_channel
            // counter / occ / valid / data wires). PR-hf4 item 2 registered
            // the scalar ones (credit, occ, head, tail, send_valid,
            // credit_return) as real BV signals; resolve those through
            // the normal Ident path. Anything else (payload `_data`,
            // `_can_send` when the bus parameter enables the registered
            // variant) is still unsupported.
            SynthIdent(name, _) => {
                if self.sigs.contains_key(name) {
                    return self.encode_ident(name, t, expr.span);
                }
                // Derived comb signals: codegen exposes `can_send` and
                // `valid` as combinational outputs of the credit_channel
                // state. Encode them as comparisons against the lifted
                // regs so user code that gates traffic on these (the
                // canonical pattern) flows through correctly.
                //
                //   __<port>_<ch>_can_send  ≡  __<port>_<ch>_credit != 0
                //   __<port>_<ch>_valid     ≡  __<port>_<ch>_occ    != 0
                if let Some(reg) = self.derived_nonzero.get(name) {
                    let r = self.encode_ident(reg, t, expr.span)?;
                    let zero = bv_zero(r.width);
                    return Ok(SmtTerm {
                        s: format!("(ite (= {} {zero}) #b0 #b1)", r.s),
                        width: 1,
                        signed: false,
                    });
                }
                if let Some(stem) = name.strip_suffix("_can_send")
                    .or_else(|| name.strip_suffix("_valid"))
                {
                    let suffix = if name.ends_with("_can_send") { "_credit" } else { "_occ" };
                    let reg = format!("{stem}{suffix}");
                    if self.sigs.contains_key(&reg) {
                        let r = self.encode_ident(&reg, t, expr.span)?;
                        let zero = bv_zero(r.width);
                        return Ok(SmtTerm {
                            s: format!("(ite (= {} {zero}) #b0 #b1)", r.s),
                            width: 1,
                            signed: false,
                        });
                    }
                }
                Err(CompileError::general(
                    &format!(
                        "formal encoding of synthesized identifier `{name}` is not yet supported — only credit_channel scalar state and derived can_send/valid are modelled today (see doc/archive/plan_hierarchical_formal.md PR-hf4)",
                    ),
                    expr.span,
                ))
            }
            Literal(l) => lit_to_term(l, &self.params, expr.span),
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
            FieldAccess(base, field) => {
                // Bus-port field access — `s.data_send_valid` resolves
                // to the codegen-flat name `s_data_send_valid`. The
                // formal lifted-state pass registered these flat names
                // (for credit_channel signals) as BV in `sigs`, so we
                // can route through the normal Ident path.
                if let ExprKind::Ident(port) = &base.kind {
                    let flat = format!("{port}_{}", field.name);
                    if self.sigs.contains_key(&flat) {
                        return self.encode_ident(&flat, t, expr.span);
                    }
                }
                Err(CompileError::general(
                    "expression kind not supported by arch formal v1 (struct field access on a non-bus port — only `<bus_port>.<channel_signal>` is supported)",
                    expr.span,
                ))
            }
            // Float builtins: fused multiply-add and NaN classification.
            FunctionCall(name, args) if name == "fma" && args.len() == 3 => {
                let tag = args
                    .iter()
                    .find_map(|a| self.expr_float_tag(a))
                    .unwrap_or("f32");
                let w = float_tag_width(tag);
                let ea = coerce(self.encode_raw(&args[0], t)?, w, false);
                let eb = coerce(self.encode_raw(&args[1], t)?, w, false);
                let ec = coerce(self.encode_raw(&args[2], t)?, w, false);
                Ok(SmtTerm {
                    s: format!("(arch_fma_{tag} {} {} {})", ea.s, eb.s, ec.s),
                    width: w,
                    signed: false,
                })
            }
            FunctionCall(name, args) if name == "is_nan" && args.len() == 1 => {
                // E8M0 carries no float tag (it is a scale type); its NaN is
                // the single code 0xFF. Without this it would take the f32
                // test at f32's bit offsets on an 8-bit value.
                if self.expr_is_e8m0(&args[0]) {
                    let x = coerce(self.encode_raw(&args[0], t)?, 8, false);
                    return Ok(SmtTerm {
                        s: format!("(ite (= {} #xff) #b1 #b0)", x.s),
                        width: 1,
                        signed: false,
                    });
                }
                let tag = self.expr_float_tag(&args[0]).unwrap_or("f32");
                let w = float_tag_width(tag);
                let x = coerce(self.encode_raw(&args[0], t)?, w, false);
                // Bit-field NaN test, DERIVED from the format table. The old
                // hand-written match ended in `_ =>` returning the *e5m2*
                // test at e5m2's offsets, so any unnamed format was probed
                // at the wrong bits. Deriving it makes a new format a table
                // row rather than a fifth arm here.
                let d = crate::fp_format::by_tag(tag)
                    .unwrap_or_else(|| crate::fp_format::by_id(crate::fp_format::FpFormatId::Fp32));
                let cond = nan_test_smt(d, &x.s);
                Ok(SmtTerm {
                    s: format!("(ite {cond} #b1 #b0)"),
                    width: 1,
                    signed: false,
                })
            }
            FunctionCall(name, args) if (name == "rose" || name == "fell") && args.len() == 1 => {
                // rose(a) ≡ a@t AND NOT a@(t-1); fell(a) ≡ NOT a@t AND a@(t-1).
                // run_property's max_past_depth treats these as depth 1.
                if t < 1 {
                    return Err(CompileError::general(
                        &format!("internal: {name}() at cycle 0 — run_property should have skipped this cycle"),
                        expr.span,
                    ));
                }
                let now = self.encode_raw(&args[0], t)?;
                let prev = self.encode_raw(&args[0], t - 1)?;
                let now_b = as_bv1_bool(&now);
                let prev_b = as_bv1_bool(&prev);
                let term = if name == "rose" {
                    format!("(bvand {now_b} (bvnot {prev_b}))")
                } else {
                    format!("(bvand (bvnot {now_b}) {prev_b})")
                };
                Ok(SmtTerm { s: term, width: 1, signed: false })
            }
            FunctionCall(name, args) if name == "past" && args.len() == 2 => {
                // SVA past(expr, N): encode `expr` at cycle t - N. Caller
                // (run_property) computes the min cycle for the property
                // and skips earlier ones, so t < N indicates a bug.
                let n = match &args[1].kind {
                    Literal(LitKind::Dec(n)) | Literal(LitKind::Sized(_, n)) => *n as u32,
                    _ => return Err(CompileError::general(
                        "`past(expr, N)` requires N to be a compile-time integer literal",
                        expr.span,
                    )),
                };
                if t < n {
                    return Err(CompileError::general(
                        &format!("internal: past depth {n} exceeds cycle index {t} — run_property should have skipped this cycle"),
                        expr.span,
                    ));
                }
                self.encode_raw(&args[0], t - n)
            }
            StructLiteral(_, _) | Cast(_, _) | Index(_, _)
            | FunctionCall(_, _) | Inside(_, _) | Match(_, _) | ExprMatch(_, _) | Todo => {
                Err(CompileError::general(
                    "expression kind not supported by arch formal v1 (struct literal / cast / index / function call / match / inside / todo)",
                    expr.span,
                ))
            }
        }
    }

    fn encode_ident(&self, name: &str, t: u32, span: Span) -> Result<SmtTerm, CompileError> {
        // 1. Const param?
        if let Some(val) = self.params.get(name) {
            // Default to 32-bit; coerce() resizes as needed.
            return Ok(SmtTerm {
                s: bv_lit(*val, 32),
                width: 32,
                signed: false,
            });
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
        // 4. Derived credit_channel signal: `__<port>_<ch>_can_send`
        // resolves to `credit != 0`, `_valid` resolves to `occ != 0`.
        // Mirrors the SynthIdent path so user-written asserts that
        // reference the lifted state work too.
        if let Some(reg) = self.derived_nonzero.get(name) {
            if let Some(info) = self.sigs.get(reg) {
                let zero = bv_zero(info.width);
                return Ok(SmtTerm {
                    s: format!("(ite (= {reg}_{t} {zero}) #b0 #b1)"),
                    width: 1,
                    signed: false,
                });
            }
        }
        if let Some(stem) = name
            .strip_suffix("_can_send")
            .or_else(|| name.strip_suffix("_valid"))
        {
            let suffix = if name.ends_with("_can_send") {
                "_credit"
            } else {
                "_occ"
            };
            let reg = format!("{stem}{suffix}");
            if let Some(info) = self.sigs.get(&reg) {
                let zero = bv_zero(info.width);
                return Ok(SmtTerm {
                    s: format!("(ite (= {reg}_{t} {zero}) #b0 #b1)"),
                    width: 1,
                    signed: false,
                });
            }
        }
        Err(CompileError::general(
            &format!("unknown identifier `{name}` in arch formal encoding"),
            span,
        ))
    }

    /// Is this expression an E8M0 scale value? E8M0 is deliberately not a
    /// float format, so `expr_float_tag` returns `None` for it and every
    /// float-shaped code path must ask this instead.
    fn expr_is_e8m0(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Ident(n) => self.sigs.get(n).map(|i| i.is_e8m0).unwrap_or(false),
            ExprKind::MethodCall(_, m, _) => m.name == "to_e8m0",
            ExprKind::LatencyAt(inner, _) => self.expr_is_e8m0(inner),
            _ => false,
        }
    }

    /// Float helper tag of an expression, mirroring `encode_ident`'s
    /// resolution (signals, inline-expanded lets) plus literal/derived
    /// forms. Mixing formats is rejected upstream by typecheck.
    fn expr_float_tag(&self, e: &Expr) -> Option<&'static str> {
        use ExprKind::*;
        match &e.kind {
            Ident(name) => {
                if let Some(info) = self.sigs.get(name) {
                    return info.float;
                }
                if let Some(val) = self.let_bindings.get(name) {
                    return self.expr_float_tag(val);
                }
                None
            }
            Literal(LitKind::Float(_)) => Some("f32"),
            Literal(LitKind::TypedFloat(fmt, _)) => Some(match fmt {
                crate::ast::FloatLitFmt::Fp32 => "f32",
                crate::ast::FloatLitFmt::Bf16 => "bf16",
                crate::ast::FloatLitFmt::E4m3 => "e4m3",
                crate::ast::FloatLitFmt::E5m2 => "e5m2",
                crate::ast::FloatLitFmt::E2m1 => "e2m1",
                crate::ast::FloatLitFmt::E2m3 => "e2m3",
                crate::ast::FloatLitFmt::E3m2 => "e3m2",
            }),
            Binary(op, l, r) => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul => {
                    self.expr_float_tag(l).or_else(|| self.expr_float_tag(r))
                }
                _ => None,
            },
            Ternary(_, th, el) => self.expr_float_tag(th).or_else(|| self.expr_float_tag(el)),
            FunctionCall(name, args) if name == "fma" => {
                args.first().and_then(|a| self.expr_float_tag(a))
            }
            MethodCall(_, m, _) => match m.name.as_str() {
                "to_fp32" => Some("f32"),
                "to_bf16" => Some("bf16"),
                "to_fp8e4m3" => Some("e4m3"),
                "to_fp8e5m2" => Some("e5m2"),
                // The sub-8-bit OCP MX storage formats. Their TYPES were
                // already accepted by formal (`check_scalar_type`), and their
                // narrows are the same proven `arch_f32_to_*` helpers every
                // other backend calls — only these two dispatch tables were
                // never extended when the formats landed, so a property
                // mentioning `.to_fp4e2m1()` was refused outright.
                "to_fp4e2m1" => Some("e2m1"),
                "to_fp6e2m3" => Some("e2m3"),
                "to_fp6e3m2" => Some("e3m2"),
                _ => None,
            },
            LatencyAt(inner, _) => self.expr_float_tag(inner),
            _ => None,
        }
    }

    fn encode_binary(
        &self,
        op: BinOp,
        a: &Expr,
        b: &Expr,
        t: u32,
        span: Span,
    ) -> Result<SmtTerm, CompileError> {
        // Float operands dispatch to the inlined proven define-funs — the
        // formal mirror of the SV/sim operator dispatch.
        if let Some(tag) = self.expr_float_tag(a).or_else(|| self.expr_float_tag(b)) {
            let fop = match op {
                BinOp::Add => Some(("add", false)),
                BinOp::Sub => Some(("sub", false)),
                BinOp::Mul => Some(("mul", false)),
                BinOp::Eq => Some(("eq", true)),
                BinOp::Neq => Some(("ne", true)),
                BinOp::Lt => Some(("lt", true)),
                BinOp::Gt => Some(("gt", true)),
                BinOp::Lte => Some(("le", true)),
                BinOp::Gte => Some(("ge", true)),
                _ => None,
            };
            if let Some((fop, is_cmp)) = fop {
                let w = float_tag_width(tag);
                let ta = self.encode_raw(a, t)?;
                let tb = self.encode_raw(b, t)?;
                let la = coerce(ta, w, false);
                let lb = coerce(tb, w, false);
                return Ok(SmtTerm {
                    s: format!("(arch_{tag}_{fop} {} {})", la.s, lb.s),
                    width: if is_cmp { 1 } else { w },
                    signed: false,
                });
            }
        }
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
                Ok(SmtTerm {
                    s: format!("({opname} {} {})", la.s, lb.s),
                    width: out_w,
                    signed,
                })
            }
            BinOp::Mul => {
                // Non-wrapping: result width = W(a) + W(b)
                let out_w = ta.width + tb.width;
                let signed = ta.signed || tb.signed;
                let la = coerce(ta, out_w, signed);
                let lb = coerce(tb, out_w, signed);
                Ok(SmtTerm {
                    s: format!("(bvmul {} {})", la.s, lb.s),
                    width: out_w,
                    signed,
                })
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
                Ok(SmtTerm {
                    s: format!("({opname} {} {})", la.s, lb.s),
                    width: common,
                    signed,
                })
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
                Ok(SmtTerm {
                    s: format!("({opname} {} {})", la.s, lb.s),
                    width: common,
                    signed,
                })
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
                Ok(SmtTerm {
                    s,
                    width: 1,
                    signed: false,
                })
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
                Ok(SmtTerm {
                    s: format!("(ite {cmp} #b1 #b0)"),
                    width: 1,
                    signed: false,
                })
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
                Ok(SmtTerm {
                    s: format!("({opname} {} {})", la.s, lb.s),
                    width: common,
                    signed,
                })
            }
            BinOp::Shl => {
                // Result width = W(a). Amount zero-extended to W(a).
                let w = ta.width;
                let signed = ta.signed;
                let lb = coerce(tb, w, false);
                Ok(SmtTerm {
                    s: format!("(bvshl {} {})", ta.s, lb.s),
                    width: w,
                    signed,
                })
            }
            BinOp::Shr => {
                let w = ta.width;
                let signed = ta.signed;
                let lb = coerce(tb, w, false);
                let opname = if signed { "bvashr" } else { "bvlshr" };
                Ok(SmtTerm {
                    s: format!("({opname} {} {})", ta.s, lb.s),
                    width: w,
                    signed,
                })
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
            BinOp::ImpliesNext => {
                // `a |=> b` is best handled at the property level
                // (run_property recognizes a top-level ImpliesNext and
                // encodes `a@t → b@(t+1)` directly). When it appears
                // nested inside another expression — `(a |=> b) and c` —
                // we don't have a cycle-shift context, so reject.
                Err(CompileError::general(
                    "`|=>` is only supported as the top-level operator of an assert/cover \
                     property in `arch formal`; nested use (e.g. `(a |=> b) and c`) is not \
                     yet implemented",
                    span,
                ))
            }
        }
        .map_err(|e: CompileError| CompileError::general(&format!("{}", e_display(&e, span)), span))
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
                Ok(SmtTerm {
                    s: format!("(bvxor {b} #b1)"),
                    width: 1,
                    signed: false,
                })
            }
            UnaryOp::BitNot => Ok(SmtTerm {
                s: format!("(bvnot {})", ta.s),
                width: ta.width,
                signed: ta.signed,
            }),
            UnaryOp::Neg => Ok(SmtTerm {
                s: format!("(bvneg {})", ta.s),
                width: ta.width,
                signed: true,
            }),
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
                if ta.width == 1 {
                    return Ok(ta);
                }
                let mut s = format!("((_ extract 0 0) {})", ta.s);
                for i in 1..ta.width {
                    s = format!("(bvxor {s} ((_ extract {i} {i}) {}))", ta.s);
                }
                Ok(SmtTerm {
                    s,
                    width: 1,
                    signed: false,
                })
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
        // Float conversion surface — dispatch to the inlined helpers.
        let recv_tag = self.expr_float_tag(recv);
        match n {
            "to_e8m0" => {
                // Any float widens to f32 first; FP32 goes straight in.
                let f32s = match recv_tag {
                    Some("f32") | None => coerce(r, 32, false).s,
                    Some(tag) => format!(
                        "(arch_{tag}_to_f32 {})",
                        coerce(r, float_tag_width(tag), false).s
                    ),
                };
                return Ok(SmtTerm {
                    s: format!("(arch_f32_to_e8m0 {f32s})"),
                    width: 8,
                    signed: false,
                });
            }
            "to_fp32" => {
                // E8M0 carries no float tag, so it would otherwise fall into
                // the `None` identity arm and return its raw 8 bits instead
                // of the scale VALUE 2^(e-127).
                if self.expr_is_e8m0(recv) {
                    return Ok(SmtTerm {
                        s: format!("(arch_e8m0_to_f32 {})", coerce(r, 8, false).s),
                        width: 32,
                        signed: false,
                    });
                }
                return match recv_tag {
                    Some("f32") | None => Ok(r),
                    Some(tag) => Ok(SmtTerm {
                        s: format!(
                            "(arch_{tag}_to_f32 {})",
                            coerce(r, float_tag_width(tag), false).s
                        ),
                        width: 32,
                        signed: false,
                    }),
                };
            }
            "to_bf16" | "to_fp8e4m3" | "to_fp8e5m2" | "to_fp4e2m1" | "to_fp6e2m3"
            | "to_fp6e3m2" => {
                // Widths come from the format table, never a literal: `8` was
                // right for both fp8s and is wrong for FP4 (4) and FP6 (6),
                // which is precisely the wildcard shape the table removed.
                let (helper, tgt) = narrow_target(n);
                let w = float_tag_width(tgt);
                // Any float source composes through f32 (widen exact); the
                // target's narrow is the single rounding.
                return match recv_tag {
                    Some(t) if t == tgt => Ok(r),
                    Some("f32") => Ok(SmtTerm {
                        s: format!("({helper} {})", coerce(r, 32, false).s),
                        width: w,
                        signed: false,
                    }),
                    Some(src) => Ok(SmtTerm {
                        s: format!(
                            "({helper} (arch_{src}_to_f32 {}))",
                            coerce(r, float_tag_width(src), false).s
                        ),
                        width: w,
                        signed: false,
                    }),
                    None => Err(CompileError::general(
                        &format!(
                            ".{n}() on an integer is not supported inside `arch formal` properties"
                        ),
                        span,
                    )),
                };
            }
            "to_uint" | "to_sint" if recv_tag.is_some() => {
                let tag = recv_tag.unwrap();
                let w = target_w.ok_or_else(|| {
                    CompileError::general(
                        &format!(".{n}<N>() requires a constant width argument"),
                        span,
                    )
                })?;
                // Widen (exact) then the proven saturating f32->int helper;
                // the helper returns 64 bits already clamped to N.
                let f32s = if tag == "f32" {
                    coerce(r, 32, false).s
                } else {
                    format!(
                        "(arch_{tag}_to_f32 {})",
                        coerce(r, float_tag_width(tag), false).s
                    )
                };
                let conv = if n == "to_sint" {
                    "arch_f32_to_sint"
                } else {
                    "arch_f32_to_uint"
                };
                return Ok(SmtTerm {
                    s: format!(
                        "((_ extract {} 0) ({conv} {f32s} {}))",
                        w - 1,
                        bv_lit(w as u64, 32)
                    ),
                    width: w,
                    signed: n == "to_sint",
                });
            }
            _ => {}
        }
        match n {
            "trunc" => {
                let w = target_w.ok_or_else(|| {
                    CompileError::general(".trunc<N>() requires a constant width argument", span)
                })?;
                if w > r.width {
                    return Err(CompileError::general(
                        ".trunc<N>() target must be ≤ current width",
                        span,
                    ));
                }
                Ok(SmtTerm {
                    s: format!("((_ extract {} 0) {})", w - 1, r.s),
                    width: w,
                    signed: r.signed,
                })
            }
            "zext" => {
                let w = target_w.ok_or_else(|| {
                    CompileError::general(".zext<N>() requires a constant width argument", span)
                })?;
                if w < r.width {
                    return Err(CompileError::general(
                        ".zext<N>() target must be ≥ current width",
                        span,
                    ));
                }
                let pad = w - r.width;
                Ok(SmtTerm {
                    s: if pad == 0 {
                        r.s.clone()
                    } else {
                        format!("((_ zero_extend {pad}) {})", r.s)
                    },
                    width: w,
                    signed: false,
                })
            }
            "sext" => {
                let w = target_w.ok_or_else(|| {
                    CompileError::general(".sext<N>() requires a constant width argument", span)
                })?;
                if w < r.width {
                    return Err(CompileError::general(
                        ".sext<N>() target must be ≥ current width",
                        span,
                    ));
                }
                let pad = w - r.width;
                Ok(SmtTerm {
                    s: if pad == 0 {
                        r.s.clone()
                    } else {
                        format!("((_ sign_extend {pad}) {})", r.s)
                    },
                    width: w,
                    signed: true,
                })
            }
            "resize" => {
                let w = target_w.ok_or_else(|| {
                    CompileError::general(".resize<N>() requires a constant width argument", span)
                })?;
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

    // ── assert<bound_err>: numeric error bounds via an external engine ──────
    //
    // The property compares a comb float signal against `exact(sig)` — the
    // real-valued evaluation of the SAME dataflow with every rounding
    // removed. We render the signal's cone twice into a Gappa script
    // (rounded: each op wrapped in its format's rounding operator, modeling
    // the RTL faithfully including the VR(f32) double-rounded narrow ops;
    // exact: bare over ℝ), turn range-shaped `assume`s into interval
    // hypotheses, and ask the engine to prove the bound. A second goal-free
    // query reports the tightest enclosure the engine can derive for the
    // error term. Soundness of modeling each op as ideal rounding is exactly
    // the per-operator CR theorems (tests/fp_v1/smt_proof, proofs/).
    fn run_bound_err(
        &self,
        prop: &PropertyDecl,
        args: &FormalArgs,
    ) -> Result<PropertyResult, CompileError> {
        if args.error_engine != "gappa" {
            return Err(CompileError::general(
                &format!(
                    "unknown --error-engine `{}` (supported: gappa)",
                    args.error_engine
                ),
                prop.span,
            ));
        }
        let mut goal =
            parse_bound_goal(&prop.expr).map_err(|m| CompileError::general(&m, prop.span))?;
        // The ulp() unit is the SIGNAL's format, not always f32.
        if let BoundKind::Ulps(n, _) = goal.kind {
            let tag = self
                .sigs
                .get(&goal.sig)
                .and_then(|i| i.float)
                .unwrap_or("f32");
            goal.kind = BoundKind::Ulps(n, tag);
        }

        // Build definitions for the signal's cone, rounded + exact.
        let mut defs: Vec<String> = Vec::new();
        let mut done: HashSet<String> = HashSet::new();
        let mut gctx = GappaCtx::default();
        self.emit_gappa_defs(&goal.sig, &mut defs, &mut done, &mut gctx, prop.span)?;

        // Hypotheses from range-shaped assumes over the cone's free ports.
        let mut hyps: Vec<String> = Vec::new();
        let mut ranged: HashSet<String> = HashSet::new();
        for a in &self.assumes {
            collect_range_hyps(a, &mut hyps, &mut ranged);
        }
        // Every input port feeding the cone must be range-constrained —
        // error bounds are range-dependent by nature.
        let mut free_ports: HashSet<String> = HashSet::new();
        self.collect_cone_ports(&goal.sig, &mut free_ports, &mut HashSet::new());
        let missing: Vec<String> = free_ports.difference(&ranged).cloned().collect();
        if !missing.is_empty() {
            let mut m = missing;
            m.sort();
            return Err(CompileError::general(
                &format!(
                    "assert<bound_err> requires every input in the cone to be range-constrained by `assume` clauses (missing: {}) — e.g. `assume a_rng: (a >= -1.0) and (a <= 1.0);`",
                    m.join(", ")
                ),
                prop.span,
            ));
        }

        // Iterate the formats the cone actually used, not a hardcoded list —
        // the old list silently dropped the `@rnd_` definition for any format
        // outside it, which gappa would then reject as an unknown identifier
        // (or, worse, the cone would never reach here at all). Sorted so the
        // emitted script is byte-stable.
        let mut header = String::new();
        let mut used: Vec<&str> = gctx.fmts.iter().copied().collect();
        used.sort_unstable();
        for f in used {
            let (p, emin) = gappa_fmt_params(f);
            header.push_str(&format!("@rnd_{f} = float<{p},{emin},ne>;\n"));
        }
        let defs_s = defs.join("\n");
        let hyp_s = hyps.join(" /\\ ");
        let goal_s = goal.render();
        let script = format!("{header}\n{defs_s}\n\n{{ {hyp_s} -> {goal_s} }}\n");
        let encl_script = format!(
            "{header}\n{defs_s}\n\n{{ {hyp_s} -> {sig} - M_{sig} in ? }}\n",
            sig = goal.sig
        );

        if std::env::var("GAPPA_DEBUG").is_ok() {
            eprintln!("--- gappa script for {} ---\n{script}", prop.name);
        }
        let bin = gappa_binary();
        let Some(bin) = bin else {
            return Ok(PropertyResult {
                name: prop.name.clone(),
                kind: prop.kind.clone(),
                status: PropertyStatus::Inconclusive(
                    "gappa binary not found (install gappa or set GAPPA_BIN)".to_string(),
                ),
                counterexample: None,
            });
        };

        let run = |input: &str| -> std::io::Result<(bool, String)> {
            use std::io::Write as _;
            use std::process::{Command, Stdio};
            let mut child = Command::new(&bin)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?;
            child.stdin.as_mut().unwrap().write_all(input.as_bytes())?;
            let out = child.wait_with_output()?;
            Ok((
                out.status.success(),
                String::from_utf8_lossy(&out.stderr).to_string(),
            ))
        };

        // ── Soundness side-conditions: no narrow in the cone may overflow ──
        //
        // Gappa's `float<p,emin,ne>` bounds precision and the smallest
        // exponent, but has no largest one — it reasons about an idealized
        // format of unbounded range and so never models overflow. The real
        // hardware saturates (FP4/FP6, and fp8 under `--fp-compat=cuda`) or
        // produces NaN/Inf (fp8 under riscv), and in both cases the error is
        // nothing like the rounding error gappa computed.
        //
        // Before arch#898 this was simply assumed, and a cone narrowing
        // `[1000, 2000]` to FP8E4M3 (max finite 448) reported PROVED for a
        // bound of 64 while `arch sim` returned NaN. Each narrow now carries
        // an explicit obligation `|input| <= max_finite`, and a bound is only
        // reported PROVED if every one of them is discharged.
        //
        // This matters most where MX quantization analysis is headed: under
        // `floor_pow2` the block maximum normalizes ABOVE the element
        // format's largest representable value, so saturation is routine
        // rather than exceptional.
        for (tag, input) in &gctx.narrows {
            let d = crate::fp_format::by_tag(tag).unwrap_or_else(|| {
                unreachable!("narrow target `{tag}` has no row in fp_format::FORMATS")
            });
            let max = gappa_real(d.max_finite);
            let rng_script = format!("{header}\n{defs_s}\n\n{{ {hyp_s} -> |{input}| <= {max} }}\n");
            if std::env::var("GAPPA_DEBUG").is_ok() {
                eprintln!(
                    "--- gappa range obligation for {} ---\n{rng_script}",
                    prop.name
                );
            }
            let in_range = run(&rng_script).map(|(ok, _)| ok).unwrap_or(false);
            if !in_range {
                return Ok(PropertyResult {
                    name: prop.name.clone(),
                    kind: prop.kind.clone(),
                    status: PropertyStatus::Inconclusive(format!(
                        "cannot show the narrow to {} stays in range (|input| <= {}, its \
                         largest finite value) — gappa models an unbounded exponent range, \
                         so a bound proved past that point would not describe the \
                         saturating hardware. Tighten the `assume` ranges on the cone inputs.",
                        d.type_name, d.max_finite
                    )),
                    counterexample: None,
                });
            }
        }

        let (ok, err) = run(&script)
            .map_err(|e| CompileError::general(&format!("failed to run gappa: {e}"), prop.span))?;
        // Enclosure query (best-effort; gappa prints `... in [lo, hi]` on stderr).
        let encl = run(&encl_script)
            .ok()
            .map(|(_, e)| {
                e.lines()
                    .find(|l| l.contains(" in ["))
                    .map(|l| l.trim().to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let status = if ok {
            let detail = if encl.is_empty() {
                "error bound proved".to_string()
            } else {
                format!("error bound proved; derived {encl}")
            };
            PropertyStatus::ProvedEnclosure(detail)
        } else {
            let mut why = err
                .lines()
                .find(|l| l.contains("error") || l.contains("Error") || l.contains("not satisfied"))
                .unwrap_or("engine could not prove the bound")
                .trim()
                .to_string();
            if !encl.is_empty() {
                why.push_str(&format!(" (best derivable: {encl})"));
            }
            PropertyStatus::Inconclusive(why)
        };
        Ok(PropertyResult {
            name: prop.name.clone(),
            kind: prop.kind.clone(),
            status,
            counterexample: None,
        })
    }

    /// Emit gappa definitions (rounded `name = ...;` and exact
    /// `M_name = ...;`) for `sig` and its cone, post-order.
    fn emit_gappa_defs(
        &self,
        sig: &str,
        defs: &mut Vec<String>,
        done: &mut HashSet<String>,
        gctx: &mut GappaCtx,
        span: Span,
    ) -> Result<(), CompileError> {
        if done.contains(sig) {
            return Ok(());
        }
        done.insert(sig.to_string());
        let def_expr = self.defining_expr(sig);
        let Some(expr) = def_expr else {
            // Free port — no definition (shared real variable).
            return Ok(());
        };
        // Emit dependencies first.
        let mut deps: HashSet<String> = HashSet::new();
        collect_idents(&expr, &mut deps);
        for d in &deps {
            self.emit_gappa_defs(d, defs, done, gctx, span)?;
        }
        let rounded = self.gappa_expr(&expr, true, gctx, span)?;
        let exact = self.gappa_expr(&expr, false, gctx, span)?;
        defs.push(format!("{sig} = {rounded};"));
        defs.push(format!("M_{sig} = {exact};"));
        Ok(())
    }

    /// Defining expression of a comb-driven signal (comb assign target or
    /// let binding). Ports/regs have none.
    fn defining_expr(&self, name: &str) -> Option<Expr> {
        if let Some(v) = self.let_bindings.get(name) {
            return Some(v.clone());
        }
        for ca in &self.comb_assigns {
            if ca.target == name && ca.guard.is_empty() {
                return Some(ca.value.clone());
            }
        }
        None
    }

    /// Free input ports of the cone (idents with no defining expression).
    fn collect_cone_ports(&self, sig: &str, out: &mut HashSet<String>, seen: &mut HashSet<String>) {
        if seen.contains(sig) {
            return;
        }
        seen.insert(sig.to_string());
        match self.defining_expr(sig) {
            Some(e) => {
                let mut ids = HashSet::new();
                collect_idents(&e, &mut ids);
                for i in ids {
                    self.collect_cone_ports(&i, out, seen);
                }
            }
            None => {
                out.insert(sig.to_string());
            }
        }
    }

    /// Render an ARCH float expression to gappa. `rounded` selects the
    /// implementation rendering (rounding operators per op, incl. the
    /// VR(f32) double rounding of narrow-format arith) vs the real-valued
    /// spec rendering (no roundings; conversions are identity over ℝ).
    fn gappa_expr(
        &self,
        e: &Expr,
        rounded: bool,
        gctx: &mut GappaCtx,
        span: Span,
    ) -> Result<String, CompileError> {
        use ExprKind::*;
        match &e.kind {
            Ident(n) => Ok(if rounded || self.defining_expr(n).is_none() {
                n.clone()
            } else {
                format!("M_{n}")
            }),
            Literal(LitKind::Float(bits)) => Ok(gappa_real(f64::from_bits(*bits))),
            Literal(LitKind::TypedFloat(fmt, bits)) => {
                let v = match fmt {
                    crate::ast::FloatLitFmt::Fp32 => f32::from_bits(*bits as u32) as f64,
                    crate::ast::FloatLitFmt::Bf16 => {
                        f32::from_bits((*bits as u32) << 16) as f64
                    }
                    crate::ast::FloatLitFmt::E4m3 => crate::fp_lit::e4m3_bits_to_f64(*bits as u8),
                    crate::ast::FloatLitFmt::E5m2 => crate::fp_lit::e5m2_bits_to_f64(*bits as u8),
                    crate::ast::FloatLitFmt::E2m1 => crate::fp_lit::e2m1_bits_to_f64(*bits as u8),
                    crate::ast::FloatLitFmt::E2m3 => crate::fp_lit::e2m3_bits_to_f64(*bits as u8),
                    crate::ast::FloatLitFmt::E3m2 => crate::fp_lit::e3m2_bits_to_f64(*bits as u8),
                };
                Ok(gappa_real(v))
            }
            Binary(op, a, b) => {
                let tag = self
                    .expr_float_tag(a)
                    .or_else(|| self.expr_float_tag(b))
                    .ok_or_else(|| {
                        CompileError::general(
                            "assert<bound_err> cones must be floating-point arithmetic",
                            span,
                        )
                    })?;
                let (osym, supported) = match op {
                    BinOp::Add => ("+", true),
                    BinOp::Sub => ("-", true),
                    BinOp::Mul => ("*", true),
                    _ => ("", false),
                };
                if !supported {
                    return Err(CompileError::general(
                        "assert<bound_err> cones support + - * fma and float conversions only",
                        span,
                    ));
                }
                let ga = self.gappa_expr(a, rounded, gctx, span)?;
                let gb = self.gappa_expr(b, rounded, gctx, span)?;
                let raw = format!("({ga} {osym} {gb})");
                Ok(if rounded {
                    wrap_impl_rounding(tag, &raw, gctx)
                } else {
                    raw
                })
            }
            FunctionCall(name, cargs) if name == "fma" && cargs.len() == 3 => {
                let tag = cargs
                    .iter()
                    .find_map(|a| self.expr_float_tag(a))
                    .unwrap_or("f32");
                let ga = self.gappa_expr(&cargs[0], rounded, gctx, span)?;
                let gb = self.gappa_expr(&cargs[1], rounded, gctx, span)?;
                let gc = self.gappa_expr(&cargs[2], rounded, gctx, span)?;
                let raw = format!("({ga} * {gb} + {gc})");
                Ok(if rounded {
                    wrap_impl_rounding(tag, &raw, gctx)
                } else {
                    raw
                })
            }
            MethodCall(base, m, _) => {
                let gb_r = self.gappa_expr(base, rounded, gctx, span)?;
                match m.name.as_str() {
                    // Exact widen: identity over ℝ.
                    "to_fp32" => Ok(gb_r),
                    "to_bf16" | "to_fp8e4m3" | "to_fp8e5m2" | "to_fp4e2m1"
                    | "to_fp6e2m3" | "to_fp6e3m2" => {
                        let (_, tgt) = narrow_target(m.name.as_str());
                        if rounded {
                            gctx.fmts.insert(tgt);
                            // Gappa's `float<p,emin,ne>` has a minimum but NO
                            // MAXIMUM: it models an idealized format with an
                            // unbounded exponent above, so it never sees
                            // overflow, saturation, or a NaN/Inf result. Its
                            // bound is therefore valid only where this narrow
                            // does not overflow — record that as a proof
                            // obligation to discharge separately rather than
                            // assuming it (arch#898).
                            gctx.narrows.push((tgt, gb_r.clone()));
                            Ok(format!("rnd_{tgt}({gb_r})"))
                        } else {
                            Ok(gb_r)
                        }
                    }
                    other => Err(CompileError::general(
                        &format!(
                            "`.{other}()` is not supported inside assert<bound_err> cones"
                        ),
                        span,
                    )),
                }
            }
            LatencyAt(inner, _) => self.gappa_expr(inner, rounded, gctx, span),
            _ => Err(CompileError::general(
                "assert<bound_err> cones support + - * fma, float conversions, signals, and float literals only",
                span,
            )),
        }
    }

    /// True if the implication antecedent `a` cannot be satisfied at any
    /// cycle in `[min_t, max_t]` of the constrained unroll — i.e. the
    /// trigger never fires, so an `a |-> b` / `a |=> b` proof is vacuous.
    /// One extra solver query on `base` (which already carries all `assume`
    /// constraints) asserting the disjunction of `a@t` over the window.
    fn antecedent_unreachable(
        &self,
        antecedent: &Expr,
        base: &str,
        min_t: u32,
        max_t: u32,
        args: &FormalArgs,
    ) -> Result<bool, CompileError> {
        let mut smt = String::with_capacity(base.len() + 128);
        smt.push_str(base);
        smt.push_str("\n; ── antecedent reachability (vacuity) ──\n");
        let mut terms = Vec::new();
        for t in min_t..=max_t {
            let enc = self.encode_expr(antecedent, t, Some((1, false)))?;
            terms.push(format!("(= {} #b1)", as_bv1_bool(&enc)));
        }
        let disj = if terms.len() == 1 {
            terms.into_iter().next().unwrap()
        } else {
            format!("(or {})", terms.join(" "))
        };
        smt.push_str(&format!("(assert {disj})\n(check-sat)\n"));
        let sr = invoke_solver(&args.solver, &smt, args.timeout)
            .map_err(|e| CompileError::general(&format!("solver error: {e}"), antecedent.span))?;
        // unsat ⇒ the antecedent is never satisfiable ⇒ vacuous. On a
        // `sat`/`unknown` we conservatively treat the trigger as reachable
        // (do NOT flag vacuity on an inconclusive reachability check).
        Ok(sr.stdout.split_ascii_whitespace().next() == Some("unsat"))
    }

    fn run_property(
        &self,
        prop: &PropertyDecl,
        base: &str,
        args: &FormalArgs,
    ) -> Result<PropertyResult, CompileError> {
        // Detect top-level `a |=> b` — encode `a@t → b@(t+1)` directly.
        let toplevel_implies_next =
            matches!(&prop.expr.kind, ExprKind::Binary(BinOp::ImpliesNext, _, _));

        // Determine the cycle range over which the property is well-defined:
        // - past(_, N), rose, fell need t ≥ past_depth (past values undefined).
        // - ##M expr needs t + M ≤ bound (future cycle inside unroll).
        // - Top-level `|=>` adds an additional +1 for the RHS sample at t+1.
        // SVA's vacuous-true / vacuous-no-hit semantics handle the skipped
        // cycles cleanly.
        let (min_t, future_depth) = max_cycle_offsets(&prop.expr);
        let extra_for_implies = if toplevel_implies_next { 1 } else { 0 };
        let max_t = args.bound.saturating_sub(future_depth + extra_for_implies);

        if min_t > max_t {
            let need = (min_t + future_depth + extra_for_implies).max(1);
            let msg = format!(
                "bound {} too small for property (needs ≥ {need}: past_depth={min_t}, future_depth={future_depth}{})",
                args.bound,
                if toplevel_implies_next { ", +1 for top-level `|=>`" } else { "" },
            );
            return Ok(PropertyResult {
                name: prop.name.clone(),
                kind: prop.kind.clone(),
                status: PropertyStatus::Inconclusive(msg),
                counterexample: None,
            });
        }

        // For `lhs |=> rhs`, the RHS samples one cycle after the LHS
        // sequence ENDS — for plain `a |=> b` that's t+1; for `##N a |=> b`
        // the lhs sequence ends at t+N, so rhs samples at t+N+1.
        let lhs_future = if let ExprKind::Binary(BinOp::ImpliesNext, lhs, _) = &prop.expr.kind {
            max_cycle_offsets(lhs).1
        } else {
            0
        };

        // Encode the property at each cycle min_t..=max_t.
        let mut per_cycle: Vec<String> = Vec::with_capacity((max_t - min_t + 1) as usize);
        for t in min_t..=max_t {
            let bool_term = if let ExprKind::Binary(BinOp::ImpliesNext, lhs, rhs) = &prop.expr.kind
            {
                // a@t → b@(t+1+lhs_future) ≡ ¬a@t ∨ b@(t+1+lhs_future)
                let la = self.encode_expr(lhs, t, Some((1, false)))?;
                let lb = self.encode_expr(rhs, t + 1 + lhs_future, Some((1, false)))?;
                let la_b = as_bv1_bool(&la);
                let lb_b = as_bv1_bool(&lb);
                format!("(bvor (bvnot {la_b}) {lb_b})")
            } else {
                let term = self.encode_expr(&prop.expr, t, Some((1, false)))?;
                as_bv1_bool(&term)
            };
            per_cycle.push(bool_term);
        }

        // Build the check. For Assert, we want to find ANY violation:
        //   (assert (or (= p_0 #b0) (= p_1 #b0) ...))
        // For Cover, we want to find ANY hit:
        //   (assert (or (= p_0 #b1) (= p_1 #b1) ...))
        let matcher = match prop.kind {
            AssertKind::Assert => "#b0",
            AssertKind::Cover => "#b1",
            AssertKind::Assume => {
                unreachable!("assumes are hypotheses, filtered before run_property")
            }
        };
        let disjuncts: Vec<String> = (0..per_cycle.len())
            .map(|i| format!("(= __prop_{} {matcher})", min_t + i as u32))
            .collect();
        let assertion = if disjuncts.len() == 1 {
            disjuncts.into_iter().next().unwrap()
        } else {
            format!("(or {})", disjuncts.join(" "))
        };

        // Compose final SMT text. Each cycle's property bit is bound to a
        // named constant `__prop_<t>` so the failing cycle can be read
        // directly from the solver model — the numeric evaluator cannot
        // evaluate float-helper applications, and guessing from inputs
        // rendered the WRONG cycle's (arbitrary) values as counterexamples.
        let mut smt = String::with_capacity(base.len() + 256);
        smt.push_str(base);
        smt.push_str(&format!(
            "\n; ── property `{}` ({:?}) ──\n",
            prop.name, prop.kind
        ));
        for (i, p) in per_cycle.iter().enumerate() {
            let t = min_t + i as u32;
            smt.push_str(&format!(
                "(declare-fun __prop_{t} () (_ BitVec 1))\n(assert (= __prop_{t} {p}))\n"
            ));
        }
        smt.push_str(&format!("(assert {assertion})\n"));
        smt.push_str("(check-sat)\n");
        // We always emit get-model; the solver will ignore it on unsat/unknown for most tools.
        // To be safe wrap with a push/pop so get-model only runs meaningfully.
        // Actually z3 returns "model is not available" on unsat which we tolerate.
        smt.push_str("(get-model)\n");

        // Shell out
        let sr = invoke_solver(&args.solver, &smt, args.timeout)
            .map_err(|e| CompileError::general(&format!("solver error: {e}"), prop.span))?;

        // Parse result
        let first_word = sr.stdout.split_ascii_whitespace().next().unwrap_or("");
        let status = match first_word {
            "sat" => {
                // Find earliest cycle where per_cycle[i] equals matcher.
                let model = sr.stdout.splitn(2, '\n').nth(1).unwrap_or("").to_string();
                let assignments = parse_model(&model);
                // Determine failing cycle by evaluating per_cycle against the model.
                let failing_cycle = find_first_failing_cycle(
                    &prop.kind,
                    &prop.expr,
                    self,
                    &assignments,
                    args.bound,
                );
                // Counterexample replay — the sat-side dual of the vacuity
                // guard below. Independently re-evaluate the property on the
                // solver's model (floats via the fp_ir interpreter, over the
                // identical operator definitions the query embedded) to
                // confirm the claimed violation is real. Conservative: only a
                // confident contradiction (every cycle decidable, none
                // violating) flags; anything undecidable retains the solver's
                // verdict. Kill-switch: ARCH_FORMAL_NO_REPLAY=1.
                let replay = if std::env::var_os("ARCH_FORMAL_NO_REPLAY").is_some() {
                    None
                } else {
                    let fns = crate::fp_ops::fp_functions(self.fp_compat);
                    Some(self.replay_check(prop, &assignments, min_t, max_t, lhs_future, &fns))
                };
                match replay {
                    Some(ReplayVerdict::Contradicted) => {
                        // The solver's claim differs by kind: an assert is
                        // "violated", a cover is "hit".
                        let (claim, note) = if matches!(prop.kind, AssertKind::Cover) {
                            (
                                "reported this cover as hit, but independent replay of its \
                                 model shows the cover expression holds at no cycle",
                                "cover expression holds at no cycle",
                            )
                        } else {
                            (
                                "reported this property as violated, but independent replay \
                                 of its model shows no violation at any cycle",
                                "property evaluates un-violated at every cycle",
                            )
                        };
                        PropertyStatus::EncodingUnsound(format!(
                            "internal compiler bug: the solver {claim} — \
                             the BMC query generation is unsound. This is not a design error; \
                             please report it (attach the --emit-smt output)"
                        ))
                        .with_cex(
                            render_counterexample(
                                &prop.name,
                                failing_cycle,
                                self,
                                &assignments,
                                args.bound,
                            )
                            .map(|c| {
                                format!("{c}\n  replay: {note} in [{min_t}, {max_t}] on this model")
                            }),
                        )
                    }
                    Some(ReplayVerdict::Confirmed(c)) => {
                        // Replay's earliest independently-confirmed cycle wins
                        // over the solver-bit guess (defense-in-depth against
                        // the #792 wrong-cycle class).
                        let cex =
                            render_counterexample(&prop.name, c, self, &assignments, args.bound);
                        match prop.kind {
                            AssertKind::Assert => PropertyStatus::Refuted(c),
                            AssertKind::Cover => PropertyStatus::Hit(c),
                            AssertKind::Assume => {
                                unreachable!("assumes filtered before run_property")
                            }
                        }
                        .with_cex(cex)
                    }
                    Some(ReplayVerdict::Inconclusive) | None => {
                        let cex = render_counterexample(
                            &prop.name,
                            failing_cycle,
                            self,
                            &assignments,
                            args.bound,
                        )
                        .map(|c| {
                            if replay.is_some() {
                                format!(
                                    "{c}\n  note: independent replay could not decide this \
                                     property on the model; solver verdict retained"
                                )
                            } else {
                                c
                            }
                        });
                        match prop.kind {
                            AssertKind::Assert => PropertyStatus::Refuted(failing_cycle),
                            AssertKind::Cover => PropertyStatus::Hit(failing_cycle),
                            AssertKind::Assume => {
                                unreachable!("assumes filtered before run_property")
                            }
                        }
                        .with_cex(cex)
                    }
                }
            }
            "unsat" => match prop.kind {
                AssertKind::Assert => {
                    // Antecedent-reachability (vacuity) check: an implication
                    // `a |-> b` / `a |=> b` proves vacuously whenever the
                    // antecedent `a` is unreachable in the constrained state
                    // space — the consequent is never tested. A genuine proof
                    // requires the trigger to fire at least once. If it never
                    // can, this is a vacuous pass, not a real one.
                    if let Some(antecedent) = implication_antecedent(&prop.expr) {
                        if self.antecedent_unreachable(antecedent, base, min_t, max_t, args)? {
                            PropertyStatus::Vacuous(
                                "implication antecedent is unreachable — the trigger never fires, so the consequent is never tested".to_string(),
                            )
                            .with_cex(None)
                        } else {
                            PropertyStatus::Proved(args.bound).with_cex(None)
                        }
                    } else {
                        PropertyStatus::Proved(args.bound).with_cex(None)
                    }
                }
                AssertKind::Cover => PropertyStatus::NotReached(args.bound).with_cex(None),
                AssertKind::Assume => unreachable!("assumes filtered before run_property"),
            },
            _ => PropertyStatus::Inconclusive(
                if sr.stdout.contains("timeout") || !sr.stderr.is_empty() {
                    format!("solver returned `{first_word}`: {}{}", sr.stdout, sr.stderr)
                        .trim()
                        .to_string()
                } else {
                    format!("solver returned `{first_word}`")
                },
            )
            .with_cex(None),
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
struct StatusWithCex {
    status: PropertyStatus,
    cex: Option<String>,
}

impl PropertyStatus {
    fn with_cex(self, cex: Option<String>) -> StatusWithCex {
        StatusWithCex { status: self, cex }
    }
}

// ── SMT value helpers ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SmtTerm {
    s: String,
    width: u32,
    signed: bool,
}

// ── assert<bound_err> free helpers ──────────────────────────────────────────

/// Parsed goal shape: `abs(SIG - exact(SIG)) <= BOUND` where BOUND is a
/// float literal, `LIT * ulp(exact(SIG))`, or `LIT * abs(exact(SIG))`.
struct BoundGoal {
    sig: String,
    kind: BoundKind,
}
enum BoundKind {
    Abs(f64),
    /// N ulps, checked via the sound relative form N·2⁻ᵖ (conservative by
    /// at most 2× — ulp(x) > 2⁻ᵖ·|x| for normal x).
    Ulps(f64, &'static str),
    Rel(f64),
}
impl BoundGoal {
    fn render(&self) -> String {
        match &self.kind {
            BoundKind::Abs(c) => format!("|{s} - M_{s}| <= {}", gappa_real(*c), s = self.sig),
            BoundKind::Ulps(n, tag) => {
                let p = gappa_fmt_params(tag).0;
                format!(
                    "|{s} -/ M_{s}| <= {}",
                    gappa_real(*n * (2.0f64).powi(-(p as i32))),
                    s = self.sig
                )
            }
            BoundKind::Rel(c) => {
                format!("|{s} -/ M_{s}| <= {}", gappa_real(*c), s = self.sig)
            }
        }
    }
}

fn lit_f64(e: &Expr) -> Option<f64> {
    match &e.kind {
        ExprKind::Unary(crate::ast::UnaryOp::Neg, inner) => lit_f64(inner).map(|v| -v),
        ExprKind::Literal(LitKind::Float(b)) => Some(f64::from_bits(*b)),
        ExprKind::Literal(LitKind::TypedFloat(fmt, b)) => Some(match fmt {
            crate::ast::FloatLitFmt::Fp32 => f32::from_bits(*b as u32) as f64,
            crate::ast::FloatLitFmt::Bf16 => f32::from_bits((*b as u32) << 16) as f64,
            crate::ast::FloatLitFmt::E4m3 => crate::fp_lit::e4m3_bits_to_f64(*b as u8),
            crate::ast::FloatLitFmt::E5m2 => crate::fp_lit::e5m2_bits_to_f64(*b as u8),
            crate::ast::FloatLitFmt::E2m1 => crate::fp_lit::e2m1_bits_to_f64(*b as u8),
            crate::ast::FloatLitFmt::E2m3 => crate::fp_lit::e2m3_bits_to_f64(*b as u8),
            crate::ast::FloatLitFmt::E3m2 => crate::fp_lit::e3m2_bits_to_f64(*b as u8),
        }),
        ExprKind::Literal(LitKind::Dec(v)) => Some(*v as f64),
        _ => None,
    }
}

/// `abs(SIG - exact(SIG))` matcher → SIG name.
fn match_err_term(e: &Expr) -> Option<String> {
    let ExprKind::FunctionCall(n, args) = &e.kind else {
        return None;
    };
    if n != "abs" || args.len() != 1 {
        return None;
    }
    let ExprKind::Binary(BinOp::Sub, a, b) = &args[0].kind else {
        return None;
    };
    let ExprKind::Ident(sig) = &a.kind else {
        return None;
    };
    let ExprKind::FunctionCall(en, eargs) = &b.kind else {
        return None;
    };
    if en != "exact" || eargs.len() != 1 {
        return None;
    }
    let ExprKind::Ident(sig2) = &eargs[0].kind else {
        return None;
    };
    if sig != sig2 {
        return None;
    }
    Some(sig.clone())
}

fn parse_bound_goal(e: &Expr) -> Result<BoundGoal, String> {
    const SHAPE: &str = "assert<bound_err> expects `abs(sig - exact(sig)) <= C`, `<= N * ulp(exact(sig))`, or `<= C * abs(exact(sig))`";
    let ExprKind::Binary(op, lhs, rhs) = &e.kind else {
        return Err(SHAPE.to_string());
    };
    if !matches!(op, BinOp::Lte | BinOp::Lt) {
        return Err(SHAPE.to_string());
    }
    let sig = match_err_term(lhs).ok_or_else(|| SHAPE.to_string())?;
    // RHS: literal | LIT * ulp(exact(sig)) | LIT * abs(exact(sig))
    if let Some(c) = lit_f64(rhs) {
        return Ok(BoundGoal {
            sig,
            kind: BoundKind::Abs(c),
        });
    }
    if let ExprKind::Binary(BinOp::Mul, a, b) = &rhs.kind {
        let c = lit_f64(a).ok_or_else(|| SHAPE.to_string())?;
        if let ExprKind::FunctionCall(fname, fargs) = &b.kind {
            if fargs.len() == 1 {
                if let ExprKind::FunctionCall(en, eargs) = &fargs[0].kind {
                    if en == "exact" && eargs.len() == 1 {
                        if let ExprKind::Ident(s2) = &eargs[0].kind {
                            if *s2 == sig {
                                if fname == "ulp" {
                                    // Format resolved by the caller from the
                                    // signal; default f32, refined below.
                                    return Ok(BoundGoal {
                                        sig,
                                        kind: BoundKind::Ulps(c, "f32"),
                                    });
                                }
                                if fname == "abs" {
                                    return Ok(BoundGoal {
                                        sig,
                                        kind: BoundKind::Rel(c),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Err(SHAPE.to_string())
}

/// Faithful implementation rounding for one op: f32 ops round once; the
/// narrow formats round through f32 first (the VR(f32) datapath).
fn wrap_impl_rounding(tag: &'static str, raw: &str, gctx: &mut GappaCtx) -> String {
    gctx.fmts.insert("f32");
    if tag == "f32" {
        format!("rnd_f32{raw}")
    } else {
        gctx.fmts.insert(tag);
        format!("rnd_{tag}(rnd_f32{raw})")
    }
}

/// `.to_<fmt>()` → (narrowing helper, dispatch tag). One table for all three
/// places formal dispatches conversions — the SMT encoder, the gappa cone
/// renderer and counterexample replay — because keeping three copies in step
/// is how the sub-8-bit formats came to be accepted by `check_scalar_type`
/// while every one of these tables still rejected them.
fn narrow_target(m: &str) -> (&'static str, &'static str) {
    match m {
        "to_bf16" => ("arch_f32_to_bf16", "bf16"),
        "to_fp8e4m3" => ("arch_f32_to_e4m3", "e4m3"),
        "to_fp8e5m2" => ("arch_f32_to_e5m2", "e5m2"),
        "to_fp4e2m1" => ("arch_f32_to_e2m1", "e2m1"),
        "to_fp6e2m3" => ("arch_f32_to_e2m3", "e2m3"),
        "to_fp6e3m2" => ("arch_f32_to_e3m2", "e3m2"),
        other => unreachable!("narrow_target called on non-narrowing method `{other}`"),
    }
}

/// What a `bound_err` cone accumulated while being rendered to gappa.
///
/// `fmts` drives the `@rnd_*` header. `narrows` is the soundness half: one
/// entry per narrowing conversion in the cone, holding the target format and
/// the gappa expression flowing into it, so that "this narrow does not
/// overflow" can be **discharged** rather than assumed (arch#898).
#[derive(Default)]
struct GappaCtx {
    fmts: HashSet<&'static str>,
    narrows: Vec<(&'static str, String)>,
}

/// (precision, emin) per format for gappa's `float<p,emin,ne>`, DERIVED from
/// the format table rather than tabulated here.
///
/// The previous hand-written map ended in `_ => (3, -16)`, so any format
/// without an explicit arm silently borrowed E5M2's parameters — the
/// silent-wildcard class of arch#829/#858, except that here the consequence
/// is a *certified numeric bound computed for the wrong format*, which is
/// worse than a crash. `gappa_fmt_params_reproduce_the_hand_table` pins that
/// this derivation reproduces all four original entries exactly.
///
/// `p` is the significand width including the implicit bit; `emin` is the
/// exponent of the smallest subnormal, `(1 - bias) - mant_bits`.
fn gappa_fmt_params(tag: &str) -> (u32, i32) {
    let d = crate::fp_format::by_tag(tag).unwrap_or_else(|| {
        unreachable!(
            "float dispatch tag `{tag}` has no row in fp_format::FORMATS — \
             add the format to the table"
        )
    });
    let bias = (1i32 << (d.exp_bits - 1)) - 1;
    (d.mant_bits + 1, (1 - bias) - d.mant_bits as i32)
}

/// Exact real literal in gappa's `<mantissa>b<exponent>` notation.
fn gappa_real(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let bits = v.to_bits();
    let neg = bits >> 63 == 1;
    let e = ((bits >> 52) & 0x7FF) as i64;
    let m = bits & ((1u64 << 52) - 1);
    let (mut mant, mut exp) = if e == 0 {
        (m as i128, -1074i64)
    } else {
        ((m | (1 << 52)) as i128, e - 1075)
    };
    while mant != 0 && mant % 2 == 0 {
        mant /= 2;
        exp += 1;
    }
    let sign = if neg { "-" } else { "" };
    if exp == 0 {
        format!("{sign}{mant}")
    } else {
        format!("{sign}{mant}b{exp}")
    }
}

/// Locate the gappa binary: PATH, then $GAPPA_BIN, then ~/bin/gappa.
fn gappa_binary() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("GAPPA_BIN") {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    if let Ok(out) = std::process::Command::new("which").arg("gappa").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return Some(std::path::PathBuf::from(p));
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let pb = std::path::PathBuf::from(home).join("bin/gappa");
        if pb.exists() {
            return Some(pb);
        }
    }
    None
}

/// Range-shaped assume mining: conjunctions of `x >= lit` / `x <= lit`
/// (either operand order) become gappa interval hypotheses. Ports with
/// BOTH bounds seen are recorded in `ranged`.
fn collect_range_hyps(e: &Expr, hyps: &mut Vec<String>, ranged: &mut HashSet<String>) {
    fn bounds(e: &Expr, lo: &mut HashMap<String, f64>, hi: &mut HashMap<String, f64>) {
        match &e.kind {
            ExprKind::Binary(BinOp::And, a, b) => {
                bounds(a, lo, hi);
                bounds(b, lo, hi);
            }
            ExprKind::Binary(op, a, b) => {
                let (id, lit, ge) = match (&a.kind, &b.kind, op) {
                    (ExprKind::Ident(n), _, BinOp::Gte) => (Some(n.clone()), lit_f64(b), true),
                    (ExprKind::Ident(n), _, BinOp::Lte) => (Some(n.clone()), lit_f64(b), false),
                    (_, ExprKind::Ident(n), BinOp::Gte) => (Some(n.clone()), lit_f64(a), false),
                    (_, ExprKind::Ident(n), BinOp::Lte) => (Some(n.clone()), lit_f64(a), true),
                    _ => (None, None, false),
                };
                if let (Some(id), Some(v)) = (id, lit) {
                    if ge {
                        lo.insert(id, v);
                    } else {
                        hi.insert(id, v);
                    }
                }
            }
            _ => {}
        }
    }
    let mut lo = HashMap::new();
    let mut hi = HashMap::new();
    bounds(e, &mut lo, &mut hi);
    for (n, l) in &lo {
        if let Some(h) = hi.get(n) {
            hyps.push(format!("{n} in [{}, {}]", gappa_real(*l), gappa_real(*h)));
            ranged.insert(n.clone());
        }
    }
}

fn bv_lit(value: u64, width: u32) -> String {
    // Prefer hex for widths divisible by 4, else decimal form.
    if width % 4 == 0 && width <= 64 {
        let digits = (width / 4) as usize;
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        format!("#x{:0width$x}", value & mask, width = digits)
    } else if width <= 64 {
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        format!("(_ bv{} {})", value & mask, width)
    } else {
        format!("(_ bv{value} {width})")
    }
}

fn bv_zero(width: u32) -> String {
    bv_lit(0, width)
}

/// SMT bit-field NaN test for `x`, derived from the format descriptor.
///
/// Factored out of `encode_raw` so it is unit-testable: `--emit-smt` writes
/// only the base (declarations + transitions), and per-property queries are
/// built inside `run_property` and piped straight to the solver, so
/// `scripts/refactor_diff.sh` cannot see this string. The test in this
/// module pins it against the hand-written originals instead.
fn nan_test_smt(d: &'static crate::fp_format::FpFormat, x: &str) -> String {
    use crate::fp_format::NanRule;
    // These field literals are written in the `#b…` binary form the
    // hand-written tests used. `bv_lit` would emit `#xff` for widths
    // divisible by 4 but `(_ bv0 23)` otherwise — semantically the same
    // value, different text. Matching the original spelling exactly keeps
    // this refactor provably inert; the solver would accept either.
    let ones_bin = |w: u32| format!("#b{}", "1".repeat(w as usize));
    let zeros_bin = |w: u32| format!("#b{}", "0".repeat(w as usize));
    // The exponent field kept its hex spelling where `bv_lit` produced one.
    let exp_ones = if d.exp_bits % 4 == 0 {
        bv_all_ones(d.exp_bits)
    } else {
        ones_bin(d.exp_bits)
    };
    match d.nan_rule {
        NanRule::IeeeExpAllOnes => {
            let (eh, el) = d.exp_field();
            let (mh, ml) = d
                .mant_field()
                .expect("IEEE-shaped format must have a mantissa");
            format!(
                "(and (= ((_ extract {eh} {el}) {x}) {exp_ones}) (distinct ((_ extract {mh} {ml}) {x}) {}))",
                zeros_bin(d.mant_bits),
            )
        }
        NanRule::OcpAllMagnitudeOnes => {
            let (gh, gl) = d.magnitude_field();
            format!(
                "(= ((_ extract {gh} {gl}) {x}) {})",
                ones_bin(d.magnitude_bits()),
            )
        }
        // Unreachable: typecheck rejects `is_nan` on a format with no NaN
        // encoding (`Ty::is_float_arith`).
        NanRule::NoNan => unreachable!(
            "is_nan on `{}`, which has no NaN encoding — typecheck should have \
             rejected this",
            d.type_name
        ),
    }
}

fn bv_all_ones(width: u32) -> String {
    if width <= 64 {
        let v = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        bv_lit(v, width)
    } else {
        format!("(bvnot {})", bv_zero(width))
    }
}

/// Resolve the BV width of a credit_channel's payload type T, when T is
/// a scalar UInt/SInt/Bool/Bit. Returns None for non-scalar payloads
/// (Vec / struct / named) — those can't be modelled in formal v1.
fn cc_payload_width(meta: &CreditChannelMeta) -> Option<u32> {
    let t = meta
        .params
        .iter()
        .find(|p| p.name.name == "T")
        .and_then(|p| match &p.kind {
            ParamKind::Type(te) => Some(te.clone()),
            _ => None,
        })?;
    match t {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => {
            // Width must fold to a constant — try with empty params,
            // good enough for literal widths like UInt<8>. Param-driven
            // widths inside credit_channel T aren't common in v1.
            let params = std::collections::HashMap::new();
            fold_const_expr(&w, &params).map(|v| v as u32)
        }
        TypeExpr::Bool | TypeExpr::Bit => Some(1),
        _ => None,
    }
}

fn lit_to_term(
    l: &LitKind,
    params: &HashMap<String, u64>,
    span: Span,
) -> Result<SmtTerm, CompileError> {
    match l {
        LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => {
            // Intrinsic width = bit-length, or 1 for value 0.
            let w = if *v == 0 { 1 } else { 64 - v.leading_zeros() };
            Ok(SmtTerm {
                s: bv_lit(*v, w),
                width: w,
                signed: false,
            })
        }
        LitKind::Sized(w, v) => Ok(SmtTerm {
            s: bv_lit(*v, *w),
            width: *w,
            signed: false,
        }),
        LitKind::ParamSized(name, v) => {
            let Some(width) = params.get(name).copied().map(|w| w as u32) else {
                return Err(CompileError::general(
                    &format!(
                        "param-sized literal width `{name}` is not a resolvable const parameter"
                    ),
                    span,
                ));
            };
            Ok(SmtTerm {
                s: bv_lit(*v, width),
                width,
                signed: false,
            })
        }
        // Float literals are unreachable here in practice — FP types are rejected
        // by `check_scalar_type` before emission. Fall back to the FP32 bit
        // pattern as a 32-bit vector so this stays total.
        LitKind::Float(bits) => {
            let f = (f64::from_bits(*bits)) as f32;
            Ok(SmtTerm {
                s: bv_lit(f.to_bits() as u64, 32),
                width: 32,
                signed: false,
            })
        }
        // A literal already rounded to its context float type at compile
        // time (arch#622/#624). Same "unreachable in practice" caveat as
        // `Float` above; stays total for width purposes.
        LitKind::TypedFloat(fmt, bits) => {
            let width = fmt.width();
            Ok(SmtTerm {
                s: bv_lit(*bits, width),
                width,
                signed: false,
            })
        }
    }
}

/// Coerce `t` to `(width, signed)` via sign/zero extend or extract.
fn coerce(t: SmtTerm, width: u32, signed: bool) -> SmtTerm {
    if t.width == width {
        return SmtTerm { signed, ..t };
    }
    if t.width < width {
        let pad = width - t.width;
        let op = if t.signed {
            "sign_extend"
        } else {
            "zero_extend"
        };
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
        // Bus-port field access on the LHS — `s.data_send_valid = X`
        // becomes a write to the codegen-flat name `s_data_send_valid`.
        // Formal accepts the flattened identifier so user code that
        // drives bus signals via the conventional dotted form works
        // unchanged in the encoding.
        ExprKind::FieldAccess(base, field) => {
            if let ExprKind::Ident(port) = &base.kind {
                Some(format!("{port}_{}", field.name))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn collect_idents(expr: &Expr, out: &mut HashSet<String>) {
    use ExprKind::*;
    match &expr.kind {
        Ident(n) => {
            out.insert(n.clone());
        }
        Binary(_, a, b) => {
            collect_idents(a, out);
            collect_idents(b, out);
        }
        Unary(_, a) => collect_idents(a, out),
        FunctionCall(_, args) => {
            for a in args {
                collect_idents(a, out);
            }
        }
        Ternary(c, t, e) => {
            collect_idents(c, out);
            collect_idents(t, out);
            collect_idents(e, out);
        }
        MethodCall(recv, _, args) => {
            collect_idents(recv, out);
            for a in args {
                collect_idents(a, out);
            }
        }
        BitSlice(b, hi, lo) => {
            collect_idents(b, out);
            collect_idents(hi, out);
            collect_idents(lo, out);
        }
        PartSelect(b, s, w, _) => {
            collect_idents(b, out);
            collect_idents(s, out);
            collect_idents(w, out);
        }
        Concat(es) => {
            for e in es {
                collect_idents(e, out);
            }
        }
        Repeat(n, x) => {
            collect_idents(n, out);
            collect_idents(x, out);
        }
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
        let span = Span {
            start: acc.span.start.min(c.span.start),
            end: acc.span.end.max(c.span.end),
        };
        acc = Expr::new(
            ExprKind::Binary(BinOp::And, Box::new(acc), Box::new(c.clone())),
            span,
        );
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

fn e_display(e: &CompileError, _sp: Span) -> String {
    format!("{e}")
}

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
                BinOp::Div => {
                    if vb == 0 {
                        return None;
                    } else {
                        va / vb
                    }
                }
                BinOp::Mod => {
                    if vb == 0 {
                        return None;
                    } else {
                        va % vb
                    }
                }
                BinOp::BitAnd => va & vb,
                BinOp::BitOr => va | vb,
                BinOp::BitXor => va ^ vb,
                BinOp::Shl => va << (vb & 63),
                BinOp::Shr => va >> (vb & 63),
                _ => return None,
            })
        }
        ExprKind::Unary(UnaryOp::Neg, a) => {
            let v = fold_const_expr(a, params)?;
            Some(v.wrapping_neg())
        }
        ExprKind::Clog2(inner) => {
            let v = fold_const_expr(inner, params)?;
            Some(if v <= 1 {
                1
            } else {
                64 - (v - 1).leading_zeros() as u64
            })
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
        "z3" => (
            "z3",
            vec![
                "-in".to_string(),
                format!("-T:{timeout_s}"),
                "-smt2".to_string(),
            ],
        ),
        "boolector" => (
            "boolector",
            vec![
                "--smt2".to_string(),
                "-m".to_string(),
                format!("--time={timeout_s}"),
            ],
        ),
        "bitwuzla" => (
            "bitwuzla",
            vec![
                "--produce-models=true".to_string(),
                // bitwuzla -t takes milliseconds.
                format!("-t"),
                format!("{}", timeout_s * 1000),
            ],
        ),
        other => (
            "z3",
            vec![
                "-in".to_string(),
                format!("-T:{timeout_s}"),
                format!("--solver={other}"),
            ],
        ),
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
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            // group spans i..=j, inclusive of both parens.
            let inner = &flat[i + needle.len()..j];
            // inner: `NAME () (_ BitVec W) VAL`
            // Extract name (first whitespace-separated token).
            let mut name_end = 0;
            for (k, c) in inner.char_indices() {
                if c.is_whitespace() {
                    name_end = k;
                    break;
                }
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

// ── SVA `past(_, N)` depth analysis ─────────────────────────────────────────

/// Returns `(past_depth, future_depth)`: the largest `N` such that
/// `past(_, N)` (or `rose`/`fell` which act like depth-1 past) appears
/// in `e`, paired with the largest `M` such that `##M expr` appears.
/// run_property uses these to bound the per-cycle eval loop:
/// skip `t < past_depth` (past values undefined), `t > bound - future_depth`
/// (future values out of unroll). Both follow SVA's vacuous semantics for
/// the skipped cycles.
fn max_cycle_offsets(e: &Expr) -> (u32, u32) {
    use ExprKind::*;
    fn cmb(a: (u32, u32), b: (u32, u32)) -> (u32, u32) {
        (a.0.max(b.0), a.1.max(b.1))
    }
    match &e.kind {
        FunctionCall(name, args) if name == "past" && args.len() == 2 => {
            let n = match &args[1].kind {
                Literal(LitKind::Dec(n)) | Literal(LitKind::Sized(_, n)) => *n as u32,
                _ => 0,
            };
            let (p, f) = max_cycle_offsets(&args[0]);
            (n + p, f)
        }
        FunctionCall(name, args) if (name == "rose" || name == "fell") && args.len() == 1 => {
            let (p, f) = max_cycle_offsets(&args[0]);
            (1 + p, f)
        }
        SvaNext(n, inner) => {
            let (p, f) = max_cycle_offsets(inner);
            (p, n + f)
        }
        Binary(_, l, r) => cmb(max_cycle_offsets(l), max_cycle_offsets(r)),
        Unary(_, x)
        | Cast(x, _)
        | Clog2(x)
        | Onehot(x)
        | Signed(x)
        | Unsigned(x)
        | LatencyAt(x, _) => max_cycle_offsets(x),
        Ternary(c, t, el) => cmb(
            cmb(max_cycle_offsets(c), max_cycle_offsets(t)),
            max_cycle_offsets(el),
        ),
        Index(b, i) => cmb(max_cycle_offsets(b), max_cycle_offsets(i)),
        BitSlice(b, _, _) => max_cycle_offsets(b),
        PartSelect(b, s, w, _) => cmb(
            cmb(max_cycle_offsets(b), max_cycle_offsets(s)),
            max_cycle_offsets(w),
        ),
        Concat(xs) | FunctionCall(_, xs) => {
            xs.iter().fold((0, 0), |a, x| cmb(a, max_cycle_offsets(x)))
        }
        Repeat(n, x) => cmb(max_cycle_offsets(n), max_cycle_offsets(x)),
        MethodCall(r, _, args) => cmb(
            max_cycle_offsets(r),
            args.iter()
                .fold((0, 0), |a, x| cmb(a, max_cycle_offsets(x))),
        ),
        FieldAccess(b, _) => max_cycle_offsets(b),
        StructLiteral(_, fs) => fs
            .iter()
            .fold((0, 0), |a, fi| cmb(a, max_cycle_offsets(&fi.value))),
        _ => (0, 0),
    }
}

// ── Counterexample rendering ────────────────────────────────────────────────

/// Antecedent of a top-level implication property (`a |-> b` or `a |=> b`),
/// if the expression is one. Used for vacuity (antecedent-reachability)
/// checking: an implication whose antecedent is unreachable proves
/// vacuously — the consequent is never exercised.
fn implication_antecedent(expr: &Expr) -> Option<&Expr> {
    match &expr.kind {
        ExprKind::Binary(BinOp::Implies, a, _) | ExprKind::Binary(BinOp::ImpliesNext, a, _) => {
            Some(a)
        }
        _ => None,
    }
}

fn find_first_failing_cycle(
    kind: &AssertKind,
    expr: &Expr,
    ctx: &FormalCtx,
    assignments: &HashMap<String, u64>,
    bound: u32,
) -> u32 {
    let target_bit = matches!(kind, AssertKind::Cover) as u64; // cover: want 1; assert: want 0 (failing)
    let (min_t, future_depth) = max_cycle_offsets(expr);
    let extra = if matches!(&expr.kind, ExprKind::Binary(BinOp::ImpliesNext, _, _)) {
        1
    } else {
        0
    };
    let max_t = bound.saturating_sub(future_depth + extra);
    if min_t > max_t {
        return min_t.min(bound);
    }
    // Primary: the named per-cycle property bits from the model.
    for t in min_t..=max_t {
        if let Some(v) = assignments.get(&format!("__prop_{t}")) {
            if (v & 1) == target_bit {
                return t;
            }
        }
    }
    // Fallback (older models without the named bits): numeric evaluation —
    // skip cycles the evaluator cannot decide instead of claiming them.
    for t in min_t..=max_t {
        if let Some(v) = eval_expr_numeric(expr, t, ctx, assignments) {
            if (v & 1) == target_bit {
                return t;
            }
        }
    }
    max_t
}

fn render_counterexample(
    prop_name: &str,
    cycle: u32,
    ctx: &FormalCtx,
    assignments: &HashMap<String, u64>,
    _bound: u32,
) -> Option<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "Counterexample for `{prop_name}` at cycle {cycle}:"
    ));
    lines.push(String::new());
    // Header
    let mut names: Vec<String> = Vec::new();
    names.push(ctx.reset.name.clone());
    names.extend(ctx.inputs.iter().filter(|n| *n != &ctx.reset.name).cloned());
    names.extend(ctx.regs.iter().cloned());
    let header: Vec<String> = std::iter::once("cycle".to_string())
        .chain(names.iter().cloned())
        .collect();
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
        Literal(LitKind::Dec(v))
        | Literal(LitKind::Hex(v))
        | Literal(LitKind::Bin(v))
        | Literal(LitKind::Sized(_, v)) => Some(*v),
        Bool(b) => Some(if *b { 1 } else { 0 }),
        Ident(n) => {
            if let Some(v) = ctx.params.get(n) {
                return Some(*v);
            }
            if let Some(val) = ctx.let_bindings.get(n) {
                return eval_expr_numeric(val, t, ctx, assignments);
            }
            assignments.get(&format!("{n}_{t}")).copied()
        }
        Binary(op, a, b) => {
            // SVA `a |=> b`: sample b at t+1 (next cycle).
            if matches!(op, BinOp::ImpliesNext) {
                let va = eval_expr_numeric(a, t, ctx, assignments)?;
                let vb = eval_expr_numeric(b, t + 1, ctx, assignments)?;
                return Some(((va == 0) || (vb != 0)) as u64);
            }
            let va = eval_expr_numeric(a, t, ctx, assignments)?;
            let vb = eval_expr_numeric(b, t, ctx, assignments)?;
            Some(match op {
                BinOp::Add | BinOp::AddWrap => va.wrapping_add(vb),
                BinOp::Sub | BinOp::SubWrap => va.wrapping_sub(vb),
                BinOp::Mul | BinOp::MulWrap => va.wrapping_mul(vb),
                BinOp::Div => {
                    if vb == 0 {
                        0
                    } else {
                        va / vb
                    }
                }
                BinOp::Mod => {
                    if vb == 0 {
                        0
                    } else {
                        va % vb
                    }
                }
                BinOp::Eq => (va == vb) as u64,
                BinOp::Neq => (va != vb) as u64,
                BinOp::Lt => (va < vb) as u64,
                BinOp::Gt => (va > vb) as u64,
                BinOp::Lte => (va <= vb) as u64,
                BinOp::Gte => (va >= vb) as u64,
                BinOp::And => ((va != 0) && (vb != 0)) as u64,
                BinOp::Or => ((va != 0) || (vb != 0)) as u64,
                BinOp::BitAnd => va & vb,
                BinOp::BitOr => va | vb,
                BinOp::BitXor => va ^ vb,
                BinOp::Shl => va << (vb & 63),
                BinOp::Shr => va >> (vb & 63),
                BinOp::Implies => ((va == 0) || (vb != 0)) as u64,
                BinOp::ImpliesNext => unreachable!("handled above"),
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
            if cv != 0 {
                eval_expr_numeric(tt, t, ctx, assignments)
            } else {
                eval_expr_numeric(ee, t, ctx, assignments)
            }
        }
        FunctionCall(name, args) if name == "past" && args.len() == 2 => {
            let n = match &args[1].kind {
                Literal(LitKind::Dec(n)) | Literal(LitKind::Sized(_, n)) => *n as u32,
                _ => return None,
            };
            if t < n {
                return None;
            }
            eval_expr_numeric(&args[0], t - n, ctx, assignments)
        }
        FunctionCall(name, args) if (name == "rose" || name == "fell") && args.len() == 1 => {
            if t < 1 {
                return None;
            }
            let now = eval_expr_numeric(&args[0], t, ctx, assignments)? & 1;
            let prev = eval_expr_numeric(&args[0], t - 1, ctx, assignments)? & 1;
            Some(if name == "rose" {
                (now == 1 && prev == 0) as u64
            } else {
                (now == 0 && prev == 1) as u64
            })
        }
        SvaNext(n, inner) => eval_expr_numeric(inner, t + n, ctx, assignments),
        _ => None,
    }
}

// ── Counterexample replay ────────────────────────────────────────────────────
//
// The sat-side dual of the vacuity guard: when the solver claims a violation
// (REFUTED) or cover hit, independently re-evaluate the property on the
// returned model to confirm the claim. A confident contradiction — every
// cycle decidable, none violating — means the BMC query generation itself is
// unsound (`PropertyStatus::EncodingUnsound`, exit 3).
//
// This is a deliberately PARALLEL implementation of the encoder's semantics,
// not a shared helper: a shared dispatch would mirror an encoder bug into the
// replay and mask the contradiction. Drift is safe by construction — any
// form replay does not recognize returns `None`, which can only weaken the
// verdict to inconclusive, never fabricate a flag.
//
// Unlike `eval_expr_numeric` (a heuristic cycle-finder that computes on bare
// u64s), replay must be width- and signedness-exact against the SMT encoding:
// `bvnot`/`bvneg`/widening arithmetic/signed compares all depend on operand
// width, and a width divergence here could produce a false contradiction.
// `NumVal` therefore mirrors `SmtTerm` (value, width, signedness), and every
// arm below mirrors the corresponding `encode_*` arm's width rules. Float
// operations evaluate through `fp_ir::eval_bv` against the same
// `fp_ops::fp_functions` table the query inlined — the identical operator
// definitions, so no cross-model equivalence assumption.

/// A width- and signedness-tracked concrete value — the replay mirror of
/// `SmtTerm`. The value lives in the low `width` bits of `v`; anything the
/// replay cannot represent (width > 64) is `None` at the call site.
#[derive(Debug, Clone, Copy)]
struct NumVal {
    v: u64,
    width: u32,
    signed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ReplayVerdict {
    /// Earliest cycle at which replay independently confirms the violation
    /// (assert) or hit (cover).
    Confirmed(u32),
    /// Every cycle in range was decidable and none shows the violation —
    /// the solver's claim is wrong. Internal compiler bug.
    Contradicted,
    /// At least one cycle was undecidable and none confirmed. The solver's
    /// verdict is retained; replay must never flag from uncertainty.
    Inconclusive,
}

fn nv_mask(w: u32) -> u64 {
    if w >= 64 {
        u64::MAX
    } else {
        (1u64 << w) - 1
    }
}

fn nv(v: u64, width: u32, signed: bool) -> NumVal {
    NumVal {
        v: v & nv_mask(width),
        width,
        signed,
    }
}

/// Reinterpret the masked `width`-bit value as a signed integer.
fn nv_as_i64(x: &NumVal) -> i64 {
    if x.width >= 64 || (x.v >> (x.width - 1)) & 1 == 0 {
        x.v as i64
    } else {
        (x.v | !nv_mask(x.width)) as i64
    }
}

/// The replay mirror of `coerce`: resize to `(width, signed)` via
/// sign/zero-extension (by the SOURCE's signedness, as `coerce` does) or
/// low-bit extraction. `None` when the target width is unrepresentable.
fn nv_coerce(x: NumVal, width: u32, signed: bool) -> Option<NumVal> {
    if width == 0 || width > 64 {
        return None;
    }
    Some(if x.width == width {
        NumVal { signed, ..x }
    } else if x.width < width {
        let v = if x.signed { nv_as_i64(&x) as u64 } else { x.v };
        nv(v, width, signed)
    } else {
        nv(x.v, width, signed)
    })
}

impl FormalCtx<'_> {
    /// Property truth value at cycle `t`, mirroring `run_property`'s
    /// per-cycle encoding: top-level `a |=> b` samples the RHS at
    /// `t + 1 + lhs_future`; everything else is the expression coerced to
    /// 1 bit (i.e. bit 0), exactly as `encode_expr(_, _, Some((1, false)))`.
    fn replay_prop_at(
        &self,
        expr: &Expr,
        t: u32,
        lhs_future: u32,
        m: &HashMap<String, u64>,
        fns: &[crate::fp_ir::FpFn],
    ) -> Option<bool> {
        if let ExprKind::Binary(BinOp::ImpliesNext, lhs, rhs) = &expr.kind {
            let la = self.replay_raw(lhs, t, m, fns)?;
            let lb = self.replay_raw(rhs, t + 1 + lhs_future, m, fns)?;
            Some((la.v & 1) == 0 || (lb.v & 1) == 1)
        } else {
            let v = self.replay_raw(expr, t, m, fns)?;
            Some((v.v & 1) == 1)
        }
    }

    /// Scan the property over the same cycle range the query asserted and
    /// classify the solver's sat claim. See `ReplayVerdict`.
    fn replay_check(
        &self,
        prop: &PropertyDecl,
        m: &HashMap<String, u64>,
        min_t: u32,
        max_t: u32,
        lhs_future: u32,
        fns: &[crate::fp_ir::FpFn],
    ) -> ReplayVerdict {
        // Assert: violation ⇔ property false. Cover: hit ⇔ property true.
        let violation_truth = matches!(prop.kind, AssertKind::Cover);
        let mut all_decided = true;
        let mut confirmed = None;
        for t in min_t..=max_t {
            match self.replay_prop_at(&prop.expr, t, lhs_future, m, fns) {
                Some(b) => {
                    if b == violation_truth && confirmed.is_none() {
                        confirmed = Some(t);
                    }
                }
                None => all_decided = false,
            }
        }
        match (confirmed, all_decided) {
            (Some(c), _) => ReplayVerdict::Confirmed(c),
            (None, true) => ReplayVerdict::Contradicted,
            (None, false) => ReplayVerdict::Inconclusive,
        }
    }

    /// Evaluate a float operator application through the fp_ir interpreter:
    /// operands are already-evaluated concrete bit patterns, wrapped as
    /// constants in a single `Call` node against the operator table.
    fn replay_fp_call(
        &self,
        name: &str,
        args: &[NumVal],
        ret_w: u32,
        fns: &[crate::fp_ir::FpFn],
    ) -> Option<NumVal> {
        let bv_args: Vec<crate::fp_ir::Bv> = args
            .iter()
            .map(|a| crate::fp_ir::cst(a.v as u128, a.width))
            .collect();
        let node = crate::fp_ir::call(name, &bv_args, ret_w);
        let v = crate::fp_ir::eval_bv(&node, &HashMap::new(), fns)?;
        if ret_w > 64 {
            return None;
        }
        Some(nv(v as u64, ret_w, false))
    }

    /// The replay mirror of `encode_raw`. Every arm reproduces the
    /// corresponding encoder arm's width/signedness rules; any form outside
    /// the mirrored surface returns `None` (inconclusive), never a guess.
    fn replay_raw(
        &self,
        e: &Expr,
        t: u32,
        m: &HashMap<String, u64>,
        fns: &[crate::fp_ir::FpFn],
    ) -> Option<NumVal> {
        use ExprKind::*;
        match &e.kind {
            LatencyAt(inner, _) => self.replay_raw(inner, t, m, fns),
            SvaNext(n, inner) => self.replay_raw(inner, t + *n, m, fns),
            Bool(b) => Some(nv(*b as u64, 1, false)),
            Literal(l) => self.replay_literal(l),
            Ident(name) => self.replay_ident(name, t, m, fns),
            SynthIdent(name, _) => {
                if self.sigs.contains_key(name) {
                    return self.replay_ident(name, t, m, fns);
                }
                self.replay_derived_nonzero(name, t, m)
            }
            Binary(op, a, b) => self.replay_binary(*op, a, b, t, m, fns),
            Unary(op, a) => {
                let ta = self.replay_raw(a, t, m, fns)?;
                Some(match op {
                    UnaryOp::Not => nv((ta.v == 0) as u64, 1, false),
                    UnaryOp::BitNot => nv(!ta.v, ta.width, ta.signed),
                    UnaryOp::Neg => nv(ta.v.wrapping_neg(), ta.width, true),
                    UnaryOp::RedAnd => nv((ta.v == nv_mask(ta.width)) as u64, 1, false),
                    UnaryOp::RedOr => nv((ta.v != 0) as u64, 1, false),
                    UnaryOp::RedXor => nv((ta.v.count_ones() & 1) as u64, 1, false),
                })
            }
            Ternary(c, then_e, else_e) => {
                // Mirror the encoder's totality: both branches must be
                // decidable (their widths shape the result), even though
                // only one value is selected.
                let ct = self.replay_raw(c, t, m, fns)?;
                let tt = self.replay_raw(then_e, t, m, fns)?;
                let et = self.replay_raw(else_e, t, m, fns)?;
                let w = tt.width.max(et.width);
                let signed = tt.signed || et.signed;
                let th = nv_coerce(tt, w, signed)?;
                let el = nv_coerce(et, w, signed)?;
                Some(if ct.v != 0 { th } else { el })
            }
            MethodCall(recv, method, args) => self.replay_method(recv, method, args, t, m, fns),
            BitSlice(base, hi, lo) => {
                let b = self.replay_raw(base, t, m, fns)?;
                let hi_v = fold_const_expr(hi, &self.params)?;
                let lo_v = fold_const_expr(lo, &self.params)?;
                if hi_v < lo_v || hi_v >= b.width as u64 {
                    return None;
                }
                let w = (hi_v - lo_v + 1) as u32;
                Some(nv(b.v >> lo_v, w, b.signed))
            }
            PartSelect(base, start, width, is_plus) => {
                let b = self.replay_raw(base, t, m, fns)?;
                let s_v = fold_const_expr(start, &self.params)?;
                let w_v = fold_const_expr(width, &self.params)?;
                if w_v == 0 {
                    return None;
                }
                let (hi, lo) = if *is_plus {
                    (s_v + w_v - 1, s_v)
                } else {
                    (s_v, s_v.checked_sub(w_v - 1)?)
                };
                if hi < lo || hi >= b.width as u64 {
                    return None;
                }
                Some(nv(b.v >> lo, w_v as u32, b.signed))
            }
            Concat(es) => {
                let parts: Option<Vec<NumVal>> =
                    es.iter().map(|p| self.replay_raw(p, t, m, fns)).collect();
                let parts = parts?;
                // Mirror the encoder: a singleton {a} passes the sole part
                // through unchanged, keeping its signedness.
                if parts.len() == 1 {
                    return parts.into_iter().next();
                }
                let total: u32 = parts.iter().map(|p| p.width).sum();
                if total > 64 {
                    return None;
                }
                let mut v = 0u64;
                for p in &parts {
                    v = (v << p.width) | p.v;
                }
                Some(nv(v, total, false))
            }
            Repeat(n, x) => {
                let n_v = fold_const_expr(n, &self.params)?;
                if n_v == 0 {
                    return None;
                }
                let xt = self.replay_raw(x, t, m, fns)?;
                // Mirror the encoder: {1{x}} passes the operand through
                // unchanged, keeping its signedness.
                if n_v == 1 {
                    return Some(xt);
                }
                let total = xt.width.checked_mul(n_v as u32)?;
                if total > 64 {
                    return None;
                }
                let mut v = 0u64;
                for _ in 0..n_v {
                    v = (v << xt.width) | xt.v;
                }
                Some(nv(v, total, false))
            }
            Signed(inner) => {
                let ti = self.replay_raw(inner, t, m, fns)?;
                Some(NumVal { signed: true, ..ti })
            }
            Unsigned(inner) => {
                let ti = self.replay_raw(inner, t, m, fns)?;
                Some(NumVal {
                    signed: false,
                    ..ti
                })
            }
            Clog2(inner) => {
                let v = fold_const_expr(inner, &self.params)?;
                let r = if v <= 1 {
                    1
                } else {
                    64 - (v - 1).leading_zeros() as u64
                };
                Some(nv(r, 32, false))
            }
            Onehot(idx) => {
                let idx_t = self.replay_raw(idx, t, m, fns)?;
                let amt = nv_coerce(idx_t, 32, false)?.v;
                let v = if amt >= 64 { 0 } else { 1u64 << amt };
                Some(nv(v, 32, false))
            }
            EnumVariant(en, v) => {
                let key = format!("{}::{}", en.name, v.name);
                let (val, w) = self.enum_variants.get(&key)?;
                Some(nv(*val, *w, false))
            }
            FieldAccess(base, field) => {
                if let Ident(port) = &base.kind {
                    let flat = format!("{port}_{}", field.name);
                    if self.sigs.contains_key(&flat) {
                        return self.replay_ident(&flat, t, m, fns);
                    }
                }
                None
            }
            FunctionCall(name, args) if name == "fma" && args.len() == 3 => {
                let tag = args
                    .iter()
                    .find_map(|a| self.expr_float_tag(a))
                    .unwrap_or("f32");
                let w = float_tag_width(tag);
                let ea = nv_coerce(self.replay_raw(&args[0], t, m, fns)?, w, false)?;
                let eb = nv_coerce(self.replay_raw(&args[1], t, m, fns)?, w, false)?;
                let ec = nv_coerce(self.replay_raw(&args[2], t, m, fns)?, w, false)?;
                self.replay_fp_call(&format!("arch_fma_{tag}"), &[ea, eb, ec], w, fns)
            }
            FunctionCall(name, args) if name == "is_nan" && args.len() == 1 => {
                let tag = self.expr_float_tag(&args[0]).unwrap_or("f32");
                let w = float_tag_width(tag);
                let x = nv_coerce(self.replay_raw(&args[0], t, m, fns)?, w, false)?.v;
                // Bit-field NaN test. The FIELD EXTENTS and the rule come
                // from the format table — those are format facts, already
                // shared with the encoder the same way carrier widths are
                // (see `float_tag_width`) — but the arithmetic below stays
                // replay's own, deliberately not a call into the encoder's
                // `nan_test_smt`.
                //
                // This narrows replay's independence slightly and it is a
                // conscious trade: the previous `_ =>` arm fell back to the
                // e5m2 test, so a new format would have been probed at
                // e5m2's offsets — silently wrong on BOTH sides, which is
                // worse than sharing a table row. Independence still covers
                // what it is for: operator dispatch and semantics.
                let d = crate::fp_format::by_tag(tag)
                    .unwrap_or_else(|| crate::fp_format::by_id(crate::fp_format::FpFormatId::Fp32));
                let is_nan = match d.nan_rule {
                    crate::fp_format::NanRule::IeeeExpAllOnes => {
                        let (_, el) = d.exp_field();
                        let exp_mask = (1u64 << d.exp_bits) - 1;
                        let mant_mask = (1u64 << d.mant_bits) - 1;
                        (x >> el) & exp_mask == exp_mask && x & mant_mask != 0
                    }
                    crate::fp_format::NanRule::OcpAllMagnitudeOnes => {
                        let mag_mask = (1u64 << d.magnitude_bits()) - 1;
                        x & mag_mask == mag_mask
                    }
                    // Typecheck rejects `is_nan` on a format with no NaN
                    // encoding; replay stays conservative and declines
                    // rather than inventing an answer.
                    crate::fp_format::NanRule::NoNan => return None,
                };
                Some(nv(is_nan as u64, 1, false))
            }
            FunctionCall(name, args) if name == "past" && args.len() == 2 => {
                let n = match &args[1].kind {
                    Literal(LitKind::Dec(n)) | Literal(LitKind::Sized(_, n)) => *n as u32,
                    _ => return None,
                };
                if t < n {
                    return None;
                }
                self.replay_raw(&args[0], t - n, m, fns)
            }
            FunctionCall(name, args) if (name == "rose" || name == "fell") && args.len() == 1 => {
                if t < 1 {
                    return None;
                }
                let now = self.replay_raw(&args[0], t, m, fns)?;
                let prev = self.replay_raw(&args[0], t - 1, m, fns)?;
                let (now_b, prev_b) = (now.v != 0, prev.v != 0);
                let v = if name == "rose" {
                    now_b && !prev_b
                } else {
                    !now_b && prev_b
                };
                Some(nv(v as u64, 1, false))
            }
            // Everything else — struct literals, casts, indexing, unknown
            // calls, match forms, todo!, pipelined ops — is outside the
            // mirrored surface: inconclusive, never a guess.
            _ => None,
        }
    }

    /// Mirror of `lit_to_term` plus the float-literal forms.
    fn replay_literal(&self, l: &LitKind) -> Option<NumVal> {
        Some(match l {
            LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => {
                let w = if *v == 0 { 1 } else { 64 - v.leading_zeros() };
                nv(*v, w, false)
            }
            LitKind::Sized(w, v) => {
                if *w > 64 {
                    return None;
                }
                nv(*v, *w, false)
            }
            LitKind::ParamSized(name, v) => {
                let w = *self.params.get(name)? as u32;
                if w > 64 {
                    return None;
                }
                nv(*v, w, false)
            }
            LitKind::Float(bits) => {
                let f = (f64::from_bits(*bits)) as f32;
                nv(f.to_bits() as u64, 32, false)
            }
            LitKind::TypedFloat(fmt, bits) => nv(*bits, fmt.width(), false),
        })
    }

    /// Mirror of `encode_ident`: const params (32-bit), inline-expanded let
    /// bindings, then signals read from the model at `<name>_<t>`.
    fn replay_ident(
        &self,
        name: &str,
        t: u32,
        m: &HashMap<String, u64>,
        fns: &[crate::fp_ir::FpFn],
    ) -> Option<NumVal> {
        if let Some(val) = self.params.get(name) {
            return Some(nv(*val, 32, false));
        }
        if let Some(val) = self.let_bindings.get(name) {
            return self.replay_raw(val, t, m, fns);
        }
        if let Some(info) = self.sigs.get(name) {
            // parse_model stores u64s — a wider signal's value would be
            // truncated, so refuse rather than compute on garbage.
            if info.width > 64 {
                return None;
            }
            let v = *m.get(&format!("{name}_{t}"))?;
            return Some(nv(v, info.width, info.signed));
        }
        self.replay_derived_nonzero(name, t, m)
    }

    /// Mirror of the derived credit_channel resolution shared by
    /// `encode_ident` and the SynthIdent path: `<stem>_can_send` ≡
    /// `<stem>_credit != 0`, `<stem>_valid` ≡ `<stem>_occ != 0`.
    fn replay_derived_nonzero(
        &self,
        name: &str,
        t: u32,
        m: &HashMap<String, u64>,
    ) -> Option<NumVal> {
        if let Some(reg) = self.derived_nonzero.get(name) {
            if self.sigs.get(reg).is_some() {
                let v = *m.get(&format!("{reg}_{t}"))?;
                return Some(nv((v != 0) as u64, 1, false));
            }
        }
        if let Some(stem) = name
            .strip_suffix("_can_send")
            .or_else(|| name.strip_suffix("_valid"))
        {
            let suffix = if name.ends_with("_can_send") {
                "_credit"
            } else {
                "_occ"
            };
            let reg = format!("{stem}{suffix}");
            if self.sigs.get(&reg).is_some() {
                let v = *m.get(&format!("{reg}_{t}"))?;
                return Some(nv((v != 0) as u64, 1, false));
            }
        }
        None
    }

    /// Mirror of `encode_binary`: float operands dispatch to the operator
    /// table via the fp_ir interpreter; integer forms reproduce the
    /// encoder's IEEE-1800 width rules exactly.
    fn replay_binary(
        &self,
        op: BinOp,
        a: &Expr,
        b: &Expr,
        t: u32,
        m: &HashMap<String, u64>,
        fns: &[crate::fp_ir::FpFn],
    ) -> Option<NumVal> {
        if let Some(tag) = self.expr_float_tag(a).or_else(|| self.expr_float_tag(b)) {
            let fop = match op {
                BinOp::Add => Some(("add", false)),
                BinOp::Sub => Some(("sub", false)),
                BinOp::Mul => Some(("mul", false)),
                BinOp::Eq => Some(("eq", true)),
                BinOp::Neq => Some(("ne", true)),
                BinOp::Lt => Some(("lt", true)),
                BinOp::Gt => Some(("gt", true)),
                BinOp::Lte => Some(("le", true)),
                BinOp::Gte => Some(("ge", true)),
                _ => None,
            };
            if let Some((fop, is_cmp)) = fop {
                let w = float_tag_width(tag);
                let la = nv_coerce(self.replay_raw(a, t, m, fns)?, w, false)?;
                let lb = nv_coerce(self.replay_raw(b, t, m, fns)?, w, false)?;
                let ret_w = if is_cmp { 1 } else { w };
                return self.replay_fp_call(&format!("arch_{tag}_{fop}"), &[la, lb], ret_w, fns);
            }
        }
        let ta = self.replay_raw(a, t, m, fns)?;
        let tb = self.replay_raw(b, t, m, fns)?;
        match op {
            BinOp::Add | BinOp::Sub => {
                let out_w = ta.width.max(tb.width) + 1;
                let signed = ta.signed || tb.signed;
                let la = nv_coerce(ta, out_w, signed)?;
                let lb = nv_coerce(tb, out_w, signed)?;
                let v = if op == BinOp::Add {
                    la.v.wrapping_add(lb.v)
                } else {
                    la.v.wrapping_sub(lb.v)
                };
                Some(nv(v, out_w, signed))
            }
            BinOp::Mul => {
                let out_w = ta.width.checked_add(tb.width)?;
                let signed = ta.signed || tb.signed;
                let la = nv_coerce(ta, out_w, signed)?;
                let lb = nv_coerce(tb, out_w, signed)?;
                Some(nv(la.v.wrapping_mul(lb.v), out_w, signed))
            }
            BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = nv_coerce(ta, common, signed)?;
                let lb = nv_coerce(tb, common, signed)?;
                let v = match op {
                    BinOp::AddWrap => la.v.wrapping_add(lb.v),
                    BinOp::SubWrap => la.v.wrapping_sub(lb.v),
                    BinOp::MulWrap => la.v.wrapping_mul(lb.v),
                    _ => unreachable!(),
                };
                Some(nv(v, common, signed))
            }
            BinOp::Div | BinOp::Mod => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = nv_coerce(ta, common, signed)?;
                let lb = nv_coerce(tb, common, signed)?;
                // SMT-LIB defines bvudiv/bvsdiv-by-zero, but a counterexample
                // hinging on that convention is pathological — refuse rather
                // than risk a convention mismatch.
                if lb.v == 0 {
                    return None;
                }
                let v = if signed {
                    let (x, y) = (nv_as_i64(&la), nv_as_i64(&lb));
                    if op == BinOp::Div {
                        x.wrapping_div(y) as u64
                    } else {
                        x.wrapping_rem(y) as u64
                    }
                } else if op == BinOp::Div {
                    la.v / lb.v
                } else {
                    la.v % lb.v
                };
                Some(nv(v, common, signed))
            }
            BinOp::Eq | BinOp::Neq => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = nv_coerce(ta, common, signed)?;
                let lb = nv_coerce(tb, common, signed)?;
                let eq = la.v == lb.v;
                Some(nv(
                    (if op == BinOp::Eq { eq } else { !eq }) as u64,
                    1,
                    false,
                ))
            }
            BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = nv_coerce(ta, common, signed)?;
                let lb = nv_coerce(tb, common, signed)?;
                let r = if signed {
                    let (x, y) = (nv_as_i64(&la), nv_as_i64(&lb));
                    match op {
                        BinOp::Lt => x < y,
                        BinOp::Gt => x > y,
                        BinOp::Lte => x <= y,
                        BinOp::Gte => x >= y,
                        _ => unreachable!(),
                    }
                } else {
                    match op {
                        BinOp::Lt => la.v < lb.v,
                        BinOp::Gt => la.v > lb.v,
                        BinOp::Lte => la.v <= lb.v,
                        BinOp::Gte => la.v >= lb.v,
                        _ => unreachable!(),
                    }
                };
                Some(nv(r as u64, 1, false))
            }
            BinOp::And | BinOp::Or => {
                let (ba, bb) = (ta.v != 0, tb.v != 0);
                let v = if op == BinOp::And { ba && bb } else { ba || bb };
                Some(nv(v as u64, 1, false))
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                let common = ta.width.max(tb.width);
                let signed = ta.signed || tb.signed;
                let la = nv_coerce(ta, common, signed)?;
                let lb = nv_coerce(tb, common, signed)?;
                let v = match op {
                    BinOp::BitAnd => la.v & lb.v,
                    BinOp::BitOr => la.v | lb.v,
                    BinOp::BitXor => la.v ^ lb.v,
                    _ => unreachable!(),
                };
                Some(nv(v, common, signed))
            }
            BinOp::Shl => {
                let w = ta.width;
                let amt = nv_coerce(tb, w, false)?.v;
                let v = if amt >= 64 { 0 } else { ta.v << amt };
                Some(nv(v, w, ta.signed))
            }
            BinOp::Shr => {
                let w = ta.width;
                let amt = nv_coerce(tb, w, false)?.v;
                let v = if ta.signed {
                    // bvashr: sign-fill; amounts ≥ width leave all sign bits.
                    (nv_as_i64(&ta) >> amt.min(63)) as u64
                } else if amt >= 64 {
                    0
                } else {
                    ta.v >> amt
                };
                Some(nv(v, w, ta.signed))
            }
            BinOp::Implies => Some(nv((ta.v == 0 || tb.v != 0) as u64, 1, false)),
            // Nested `|=>` is rejected by the encoder; top-level `|=>` is
            // handled in replay_prop_at.
            BinOp::ImpliesNext => None,
        }
    }

    /// Mirror of `encode_method`: float conversions through the operator
    /// table, integer width methods per the encoder's rules.
    fn replay_method(
        &self,
        recv: &Expr,
        method: &Ident,
        args: &[Expr],
        t: u32,
        m: &HashMap<String, u64>,
        fns: &[crate::fp_ir::FpFn],
    ) -> Option<NumVal> {
        let r = self.replay_raw(recv, t, m, fns)?;
        let n = method.name.as_str();
        let target_w = if args.is_empty() {
            None
        } else {
            fold_const_expr(&args[0], &self.params).map(|v| v as u32)
        };
        let recv_tag = self.expr_float_tag(recv);
        match n {
            "to_fp32" => {
                return match recv_tag {
                    Some("f32") => Some(r),
                    Some(tag) => {
                        let x = nv_coerce(r, float_tag_width(tag), false)?;
                        self.replay_fp_call(&format!("arch_{tag}_to_f32"), &[x], 32, fns)
                    }
                    // The encoder passes an integer receiver through
                    // unconverted; that shape is suspicious, so replay
                    // declines to mirror it rather than bless it.
                    None => None,
                };
            }
            "to_bf16" | "to_fp8e4m3" | "to_fp8e5m2" | "to_fp4e2m1" | "to_fp6e2m3"
            | "to_fp6e3m2" => {
                // Widths come from the format table, never a literal: `8` was
                // right for both fp8s and is wrong for FP4 (4) and FP6 (6),
                // which is precisely the wildcard shape the table removed.
                let (helper, tgt) = narrow_target(n);
                let w = float_tag_width(tgt);
                return match recv_tag {
                    Some(src) if src == tgt => Some(r),
                    Some("f32") => {
                        let x = nv_coerce(r, 32, false)?;
                        self.replay_fp_call(helper, &[x], w, fns)
                    }
                    Some(src) => {
                        let x = nv_coerce(r, float_tag_width(src), false)?;
                        let widened =
                            self.replay_fp_call(&format!("arch_{src}_to_f32"), &[x], 32, fns)?;
                        self.replay_fp_call(helper, &[widened], w, fns)
                    }
                    None => None,
                };
            }
            "to_uint" | "to_sint" if recv_tag.is_some() => {
                let tag = recv_tag.unwrap();
                let w = target_w?;
                if w == 0 || w > 64 {
                    return None;
                }
                let f32s = if tag == "f32" {
                    nv_coerce(r, 32, false)?
                } else {
                    let x = nv_coerce(r, float_tag_width(tag), false)?;
                    self.replay_fp_call(&format!("arch_{tag}_to_f32"), &[x], 32, fns)?
                };
                let conv = if n == "to_sint" {
                    "arch_f32_to_sint"
                } else {
                    "arch_f32_to_uint"
                };
                let full = self.replay_fp_call(conv, &[f32s, nv(w as u64, 32, false)], 64, fns)?;
                return Some(nv(full.v, w, n == "to_sint"));
            }
            _ => {}
        }
        match n {
            "trunc" => {
                let w = target_w?;
                if w == 0 || w > r.width {
                    return None;
                }
                Some(nv(r.v, w, r.signed))
            }
            "zext" => {
                let w = target_w?;
                if w < r.width || w > 64 {
                    return None;
                }
                Some(nv(r.v, w, false))
            }
            "sext" => {
                let w = target_w?;
                if w < r.width || w > 64 {
                    return None;
                }
                Some(nv(nv_as_i64(&r) as u64, w, true))
            }
            "resize" => {
                let w = target_w?;
                let signed = r.signed;
                nv_coerce(r, w, signed)
            }
            _ => None,
        }
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
            PropertyStatus::ProvedEnclosure(enc) => ("PROVED", enc.clone()),
            PropertyStatus::Vacuous(why) => ("VACUOUS", why.clone()),
            PropertyStatus::EncodingUnsound(why) => {
                ("ENCODING UNSOUND (compiler bug)", why.clone())
            }
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

#[cfg(test)]
mod tests {
    //! Counterexample-replay unit tests. These drive `replay_check` directly
    //! on hand-built solver models, which is the only way to exercise the
    //! CONTRADICTED verdict: no .arch fixture can make the real encoder
    //! produce an unsound query, so the z3-gated integration tests only
    //! cover the CONFIRMED path (every genuine REFUTED must stay REFUTED).
    use super::*;

    /// `gappa_fmt_params` is now derived from `fp_format::FORMATS`. It
    /// replaced a hand-written map whose `_ => (3, -16)` wildcard handed
    /// E5M2's parameters to any format without an explicit arm — and the
    /// consequence there is not a crash but a *certified numeric bound
    /// computed for the wrong format*. Pin that the derivation reproduces
    /// every original entry, and that the formats it newly reaches get the
    /// values their encodings actually imply.
    #[test]
    fn gappa_fmt_params_reproduce_the_hand_table() {
        // The four entries the hand-written map had, verbatim.
        for (tag, want) in [
            ("f32", (24u32, -149i32)),
            ("bf16", (8, -133)),
            ("e4m3", (4, -9)),
            ("e5m2", (3, -16)),
        ] {
            assert_eq!(gappa_fmt_params(tag), want, "{tag} must be unchanged");
        }
        // The sub-8-bit formats the wildcard would have silently given
        // E5M2's (3, -16). Smallest subnormal is `(1 - bias) - mant_bits`:
        // E2M1 bias 1, mant 1 -> 2^-1; E2M3 bias 1, mant 3 -> 2^-3;
        // E3M2 bias 3, mant 2 -> 2^-4.
        for (tag, want) in [
            ("e2m1", (2u32, -1i32)),
            ("e2m3", (4, -3)),
            ("e3m2", (3, -4)),
        ] {
            assert_ne!(
                gappa_fmt_params(tag),
                (3, -16),
                "{tag} must not inherit E5M2's parameters"
            );
            assert_eq!(gappa_fmt_params(tag), want);
        }
    }

    /// Every narrowing method formal dispatches must resolve to a helper and
    /// a tag that the format table knows. A method added to one of the three
    /// dispatch sites but not to this table is a compile error rather than a
    /// silent refusal, which is how the sub-8-bit formats stayed unreachable
    /// from `arch formal` after they shipped everywhere else.
    #[test]
    fn narrow_target_covers_every_narrowing_method() {
        for m in [
            "to_bf16",
            "to_fp8e4m3",
            "to_fp8e5m2",
            "to_fp4e2m1",
            "to_fp6e2m3",
            "to_fp6e3m2",
        ] {
            let (helper, tag) = narrow_target(m);
            assert!(helper.starts_with("arch_f32_to_"), "{m}: {helper}");
            assert!(
                crate::fp_format::by_tag(tag).is_some(),
                "{m}: tag `{tag}` has no format row"
            );
            // The helper name and the tag must agree, or a conversion
            // silently narrows to a different format than it reports.
            assert_eq!(helper, format!("arch_f32_to_{tag}"), "{m}");
        }
    }

    fn parse_and_resolve(src: &str) -> (crate::ast::SourceFile, SymbolTable) {
        let tokens = crate::lexer::tokenize(src).expect("lexer error");
        let mut p = crate::parser::Parser::new(tokens, src);
        let parsed = p.parse_source_file().expect("parse error");
        let ast = crate::elaborate::elaborate(parsed).expect("elaborate error");
        let symbols = crate::resolve::resolve(&ast).expect("resolve error");
        (ast, symbols)
    }

    fn build_ctx<'a>(ast: &'a crate::ast::SourceFile, symbols: &'a SymbolTable) -> FormalCtx<'a> {
        let module = select_top(ast, None).expect("select_top");
        let mut ctx = FormalCtx::new(module, symbols);
        ctx.preprocess().expect("preprocess");
        ctx
    }

    fn model(entries: &[(&str, u64)]) -> HashMap<String, u64> {
        entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    const INT_SRC: &str = r#"
module ReplayInt
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<8>;
  port o: out Bool;
  comb o = x < 100; end comb
  assert ok: x < 100;
end module ReplayInt
"#;

    #[test]
    fn replay_confirms_integer_violation_at_earliest_cycle() {
        let (ast, symbols) = parse_and_resolve(INT_SRC);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        // Violated at cycle 0 (x=200 ≥ 100).
        let m = model(&[("x_0", 200), ("x_1", 5), ("x_2", 5)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Confirmed(0)
        );
        // Violated only at cycle 2 — replay reports the EARLIEST confirmed
        // cycle (the #792 wrong-cycle relocation path).
        let m = model(&[("x_0", 5), ("x_1", 5), ("x_2", 200)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Confirmed(2)
        );
    }

    #[test]
    fn replay_contradicts_nonviolating_model() {
        // The key negative test: a model that does NOT violate the property
        // while the solver claimed sat. Every cycle decidable, none
        // violating → Contradicted → EncodingUnsound at the caller. This is
        // the stand-in for injecting an encoder bug, which no fixture can do.
        let (ast, symbols) = parse_and_resolve(INT_SRC);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let m = model(&[("x_0", 5), ("x_1", 6), ("x_2", 7)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Contradicted
        );
    }

    #[test]
    fn replay_conservative_on_undecidable_cycle() {
        // Cycle 1's value is missing from the model: that cycle is
        // undecidable, so even though no cycle confirms a violation the
        // verdict must be Inconclusive — NEVER Contradicted from uncertainty.
        let (ast, symbols) = parse_and_resolve(INT_SRC);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let m = model(&[("x_0", 5), ("x_2", 7)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Inconclusive
        );
        // ...but a missing cycle does not mask a confirmed violation
        // elsewhere.
        let m = model(&[("x_0", 200), ("x_2", 7)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Confirmed(0)
        );
    }

    #[test]
    fn replay_is_width_exact_for_bitnot() {
        // Regression pin for the width hazard that rules out a bare-u64
        // evaluator: `~x` on UInt<8> is an 8-bit bvnot in the query
        // (~5 = 250), while 64-bit `!5` is a huge value. A width-naive
        // replay would judge the property false and CONFIRM a phantom
        // violation; the width-exact replay must CONTRADICT.
        let src = r#"
module ReplayNot
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<8>;
  port o: out Bool;
  comb o = (~x) == 250; end comb
  assert inv: (~x) == 250;
end module ReplayNot
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let m = model(&[("x_0", 5), ("x_1", 5)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Contradicted,
            "8-bit ~5 is 250: the property holds, so a sat claim contradicts"
        );
    }

    #[test]
    fn replay_preserves_signedness_through_singleton_concat_and_repeat() {
        // The encoder passes a singleton {a} / {1{x}} through unchanged,
        // keeping the operand's signed flag; the replay mirror used to fall
        // into the general concat/repeat arms and force `signed: false`.
        // With x=200, `{1{signed(x)}} > 1` is a bvsgt in the query
        // (-56 > 1, false → violated → Confirmed), but a signedness-lossy
        // replay does 200 > 1 unsigned (true → property holds) and
        // manufactures a false Contradicted → EncodingUnsound.
        let src = r#"
module ReplaySignRep
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<8>;
  port o: out Bool;
  comb o = {1{signed(x)}} > 1; end comb
  assert pos: {1{signed(x)}} > 1;
end module ReplaySignRep
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let m = model(&[("x_0", 200), ("x_1", 200)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Confirmed(0),
            "signed(200) as SInt<8> is -56: the signed compare is violated"
        );
        // And the property genuinely holds for a positive value — replay
        // must agree with the encoder there too (Contradicted on a bogus
        // sat claim), still through the signed compare.
        let m = model(&[("x_0", 5), ("x_1", 5)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Contradicted
        );

        // Same divergence through the singleton-Concat shape {signed(x)}.
        let src = r#"
module ReplaySignCat
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<8>;
  port o: out Bool;
  comb o = {signed(x)} > 1; end comb
  assert pos: {signed(x)} > 1;
end module ReplaySignCat
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let m = model(&[("x_0", 200), ("x_1", 200)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Confirmed(0)
        );
    }

    #[test]
    fn replay_evaluates_float_ops_through_fp_ir() {
        let src = r#"
module ReplayFp
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in FP32;
  port b: in FP32;
  port o: out Bool;
  comb o = (a + b) < 3.0; end comb
  assert lim: (a + b) < 3.0;
end module ReplayFp
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let bits = |f: f32| f.to_bits() as u64;
        // 2.0 + 2.5 = 4.5 ≥ 3.0 at cycle 1: violated there, satisfied at
        // cycle 0 (1.0 + 1.0 = 2.0 < 3.0).
        let m = model(&[
            ("a_0", bits(1.0)),
            ("b_0", bits(1.0)),
            ("a_1", bits(2.0)),
            ("b_1", bits(2.5)),
        ]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Confirmed(1)
        );
        // No violation anywhere → Contradicted.
        let m = model(&[
            ("a_0", bits(1.0)),
            ("b_0", bits(1.0)),
            ("a_1", bits(0.5)),
            ("b_1", bits(0.25)),
        ]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Contradicted
        );
        // A missing float operand makes that cycle undecidable →
        // Inconclusive, not Contradicted (conservatism on the float path).
        let m = model(&[("a_0", bits(1.0)), ("b_0", bits(1.0)), ("a_1", bits(0.5))]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Inconclusive
        );
    }

    /// The SMT `is_nan` test is now derived from the format table rather
    /// than hand-tabulated per tag. `refactor_diff.sh` cannot cover it —
    /// `--emit-smt` writes only the base, and per-property queries go
    /// straight to the solver — so pin the derived strings against the
    /// hand-written originals here, verbatim.
    ///
    /// The originals' fallthrough is the reason this matters: the old match
    /// ended in `_ =>` returning the *e5m2* test, so any format it did not
    /// name was probed at e5m2's bit offsets.
    #[test]
    fn nan_test_smt_reproduces_the_handwritten_tests() {
        use crate::fp_format::{by_tag, FpFormatId};
        let cases: [(&str, &str); 4] = [
            (
                "f32",
                "(and (= ((_ extract 30 23) X) #xff) (distinct ((_ extract 22 0) X) #b00000000000000000000000))",
            ),
            (
                "bf16",
                "(and (= ((_ extract 14 7) X) #xff) (distinct ((_ extract 6 0) X) #b0000000))",
            ),
            ("e4m3", "(= ((_ extract 6 0) X) #b1111111)"),
            (
                "e5m2",
                "(and (= ((_ extract 6 2) X) #b11111) (distinct ((_ extract 1 0) X) #b00))",
            ),
        ];
        for (tag, want) in cases {
            let d = by_tag(tag).expect("known tag must have a table row");
            assert_eq!(
                nan_test_smt(d, "X"),
                want,
                "{tag}: derived NaN test must match the hand-written original"
            );
        }
        // The derivation is driven by the rule, not by the tag string.
        assert_eq!(
            by_tag("e4m3").unwrap().nan_rule,
            crate::fp_format::NanRule::OcpAllMagnitudeOnes,
            "OCP E4M3 is not IEEE-shaped: its sole NaN is all-magnitude-ones"
        );
        assert_eq!(
            crate::fp_format::by_id(FpFormatId::E5m2).nan_rule,
            crate::fp_format::NanRule::IeeeExpAllOnes
        );
    }

    /// Replay computes `is_nan` with its own arithmetic rather than calling
    /// `nan_test_smt`, so the two can drift. They share only the format
    /// table's rule and field extents. Pin them against hand-picked
    /// encodings — every canonical NaN, and the near-misses that a wrong
    /// field offset would misclassify.
    #[test]
    fn replay_is_nan_agrees_with_the_format_rules() {
        use crate::fp_format::{by_tag, NanRule};
        // (tag, value, expected is_nan)
        let cases: &[(&str, u64, bool)] = &[
            // f32: exp all ones + nonzero mantissa.
            ("f32", 0x7FC0_0000, true),  // canonical qNaN
            ("f32", 0x7F80_0000, false), // +inf: exp ones, mantissa zero
            ("f32", 0xFF80_0001, true),  // -sNaN
            ("f32", 0x3F80_0000, false), // 1.0
            // bf16: same shape, 8-bit exponent at [14:7].
            ("bf16", 0x7FC0, true),
            ("bf16", 0x7F80, false), // +inf
            ("bf16", 0x3F80, false), // 1.0
            // e4m3 (OCP): the SOLE NaN is all magnitude bits set.
            ("e4m3", 0x7F, true),
            ("e4m3", 0xFF, true),  // sign is not part of the test
            ("e4m3", 0x7E, false), // 448.0, the max finite — NOT NaN
            ("e4m3", 0x78, false),
            // e5m2: IEEE-shaped, exp [6:2].
            ("e5m2", 0x7D, true),
            ("e5m2", 0x7C, false), // +inf
            ("e5m2", 0xFF, true),
            ("e5m2", 0x7B, false), // max finite
        ];
        for &(tag, x, want) in cases {
            let d = by_tag(tag).expect("known tag");
            let got = match d.nan_rule {
                NanRule::IeeeExpAllOnes => {
                    let (_, el) = d.exp_field();
                    let exp_mask = (1u64 << d.exp_bits) - 1;
                    let mant_mask = (1u64 << d.mant_bits) - 1;
                    (x >> el) & exp_mask == exp_mask && x & mant_mask != 0
                }
                NanRule::OcpAllMagnitudeOnes => {
                    let mag_mask = (1u64 << d.magnitude_bits()) - 1;
                    x & mag_mask == mag_mask
                }
                NanRule::NoNan => false,
            };
            assert_eq!(got, want, "{tag}: is_nan({x:#X}) should be {want}");
        }
    }

    /// Issue #818: the assignment-target validation must run at the END of
    /// `preprocess`, not at push time in `walk_comb_stmt`. Module body items
    /// are walked in ONE source-order pass, so a `comb` block may legally
    /// precede the `wire` decl it drives (verified with `arch check`) — a
    /// push-time check would reject this valid design.
    #[test]
    fn preprocess_accepts_comb_block_before_wire_decl() {
        const SRC: &str = r#"
module CombBeforeWire
  port a: in UInt<8>;
  port o: out UInt<8>;
  comb
    w = a;
    o = w;
  end comb
  wire w: UInt<8>;
end module CombBeforeWire
"#;
        let (ast, symbols) = parse_and_resolve(SRC);
        let module = select_top(&ast, None).expect("select_top");
        let mut ctx = FormalCtx::new(module, &symbols);
        ctx.preprocess()
            .expect("a comb block preceding its wire decl must preprocess cleanly");
    }

    /// A comb write to a plain (non-credit_channel) bus field is refused
    /// with a scope message naming the port and field — not a panic
    /// (issue #818), and not a silent skip.
    #[test]
    fn preprocess_rejects_plain_bus_field_write() {
        const SRC: &str = r#"
bus PlainBus
  valid: out Bool;
  data:  out UInt<8>;
end bus PlainBus

module PlainBusWrite
  port s: target PlainBus;
  port m: initiator PlainBus;
  comb
    m.valid = s.valid;
  end comb
end module PlainBusWrite
"#;
        let (ast, symbols) = parse_and_resolve(SRC);
        let module = select_top(&ast, None).expect("select_top");
        let mut ctx = FormalCtx::new(module, &symbols);
        let err = ctx
            .preprocess()
            .expect_err("plain bus field write must be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("assignment to bus signal `m.valid`"),
            "should name the port and field: {msg}"
        );
        assert!(
            msg.contains("is not supported by `arch formal` v1"),
            "should name the v1 scope: {msg}"
        );
    }

    #[test]
    fn replay_handles_implies_next_sampling() {
        // Top-level `a |=> b`: RHS samples at t+1. Model where the
        // implication is violated exactly at t=0 (a_0=1 but b_1=0).
        let src = r#"
module ReplayImpl
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in Bool;
  port b: in Bool;
  port o: out Bool;
  comb o = a; end comb
  assert follows: a |=> b;
end module ReplayImpl
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let m = model(&[
            ("a_0", 1),
            ("b_0", 0),
            ("a_1", 0),
            ("b_1", 0),
            ("a_2", 0),
            ("b_2", 1),
        ]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Confirmed(0)
        );
        // a never fires → implication vacuously true everywhere →
        // a sat claim would contradict.
        let m = model(&[
            ("a_0", 0),
            ("b_0", 0),
            ("a_1", 0),
            ("b_1", 0),
            ("a_2", 0),
            ("b_2", 0),
        ]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 1, 0, &fns),
            ReplayVerdict::Contradicted
        );
    }

    #[test]
    fn replay_classifies_cover_hits() {
        // Cover: the solver's sat claim is a HIT, so `violation_truth` flips —
        // replay must confirm at the earliest cycle where the expression
        // holds, contradict when it holds nowhere, and stay inconclusive on a
        // missing model value. No cover property exercised replay before this.
        let src = r#"
module ReplayCover
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<8>;
  port o: out Bool;
  comb o = x == 42; end comb
  cover seen: x == 42;
end module ReplayCover
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        assert!(matches!(prop.kind, AssertKind::Cover));
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        // Hit at cycles 1 and 2 — earliest wins.
        let m = model(&[("x_0", 7), ("x_1", 42), ("x_2", 42)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Confirmed(1)
        );
        // Expression holds at no cycle → a sat (hit) claim contradicts.
        let m = model(&[("x_0", 7), ("x_1", 8), ("x_2", 9)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Contradicted
        );
        // Missing model value → that cycle undecidable → Inconclusive,
        // never Contradicted from uncertainty.
        let m = model(&[("x_0", 7), ("x_2", 9)]);
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Inconclusive
        );
    }

    #[test]
    fn replay_evaluates_past_across_cycles() {
        // `past(x, 1)` reads the model one cycle back. The real caller
        // (run_property) excludes t < past_depth via max_cycle_offsets, so
        // the realistic range starts at min_t = 1.
        let src = r#"
module ReplayPast
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<8>;
  port o: out Bool;
  comb o = x == 3; end comb
  assert stable: past(x, 1) == x;
end module ReplayPast
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let prop = &ctx.properties[0];
        let (min_t, _) = max_cycle_offsets(&prop.expr);
        assert_eq!(min_t, 1, "past(x, 1) must impose past-depth 1");
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        // x changes 3 → 7 at cycle 2: past(x,1)=3 ≠ 7 violates there.
        let m = model(&[("x_0", 3), ("x_1", 3), ("x_2", 7)]);
        assert_eq!(
            ctx.replay_check(prop, &m, min_t, 2, 0, &fns),
            ReplayVerdict::Confirmed(2)
        );
        // x constant → property holds at every in-range cycle → contradiction.
        let m = model(&[("x_0", 3), ("x_1", 3), ("x_2", 3)]);
        assert_eq!(
            ctx.replay_check(prop, &m, min_t, 2, 0, &fns),
            ReplayVerdict::Contradicted
        );
        // Conservatism: if the range wrongly includes t=0 (below past depth),
        // that cycle is undecidable (t < n) and blocks Contradicted.
        assert_eq!(
            ctx.replay_check(prop, &m, 0, 2, 0, &fns),
            ReplayVerdict::Inconclusive
        );
    }

    #[test]
    fn replay_evaluates_rose_and_fell() {
        // rose(x) ≡ x@t ∧ ¬x@(t-1); fell(x) is the mirror. Both carry
        // past-depth 1, mirroring the encoder's t≥1 requirement.
        let src = r#"
module ReplayEdge
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in Bool;
  port o: out Bool;
  comb o = x; end comb
  assert no_rise: !rose(x);
  assert no_fall: !fell(x);
end module ReplayEdge
"#;
        let (ast, symbols) = parse_and_resolve(src);
        let ctx = build_ctx(&ast, &symbols);
        let no_rise = &ctx.properties[0];
        let no_fall = &ctx.properties[1];
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        // 0 → 1 at cycle 1: rose fires there, fell never does.
        let m = model(&[("x_0", 0), ("x_1", 1), ("x_2", 1)]);
        assert_eq!(
            ctx.replay_check(no_rise, &m, 1, 2, 0, &fns),
            ReplayVerdict::Confirmed(1)
        );
        assert_eq!(
            ctx.replay_check(no_fall, &m, 1, 2, 0, &fns),
            ReplayVerdict::Contradicted
        );
        // 1 → 0 at cycle 2: fell fires there, rose never does.
        let m = model(&[("x_0", 1), ("x_1", 1), ("x_2", 0)]);
        assert_eq!(
            ctx.replay_check(no_fall, &m, 1, 2, 0, &fns),
            ReplayVerdict::Confirmed(2)
        );
        assert_eq!(
            ctx.replay_check(no_rise, &m, 1, 2, 0, &fns),
            ReplayVerdict::Contradicted
        );
    }
}
