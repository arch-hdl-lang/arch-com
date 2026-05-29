use std::collections::HashMap;

use crate::ast::*;
use crate::diagnostics::{span_to_source_span, CompileError};
use crate::lexer::Span;

/// Returns true when `port_name` is a bus port (has `bus_info`) in the
/// named module/fsm/pipeline found in `source`.  Used to skip bus-port
/// output connections from the multi-driver check — both sides (initiator
/// and target) of a bus wire have `ConnectDir::Output` connections but
/// drive *disjoint* sets of flat signals, so no real conflict exists.
fn is_bus_port_in_child(
    module_name: &str,
    port_name: &str,
    source: &SourceFile,
) -> bool {
    source.items.iter().find_map(|item| match item {
        Item::Module(m) if m.name.name == module_name => Some(m.ports.as_slice()),
        Item::Fsm(f) if f.name.name == module_name => Some(f.ports.as_slice()),
        _ => None,
    })
    .and_then(|ports| ports.iter().find(|p| p.name.name == port_name))
    .map(|p| p.bus_info.is_some())
    .unwrap_or(false)
}

/// One block-level drive record for a signal.
pub struct DriveEntry {
    pub span: Span,
}

/// Extracts the signal name from an LHS expression.
///
/// Bit-slices, part-selects, index accesses, and latency annotations are
/// stripped (they all write to the same underlying register/wire).  Field
/// accesses are *preserved* — `bus_wire.cmd` and `bus_wire.resp` are
/// distinct flat signals in the generated SV, so driving both from
/// different blocks is legal and must not trigger a multi-driver error.
fn lhs_base_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::BitSlice(base, _, _)
        | ExprKind::PartSelect(base, _, _, _)
        | ExprKind::Index(base, _)
        | ExprKind::LatencyAt(base, _) => lhs_base_name(base),
        ExprKind::FieldAccess(base, field) => {
            lhs_base_name(base).map(|b| format!("{}.{}", b, field.name))
        }
        _ => None,
    }
}

/// Collect every distinct signal name driven by `stmts`, storing the
/// span of the first assignment to each name.
fn collect_stmts(stmts: &[Stmt], out: &mut HashMap<String, Span>) {
    for s in stmts {
        collect_one_stmt(s, out);
    }
}

fn collect_one_stmt(stmt: &Stmt, out: &mut HashMap<String, Span>) {
    match stmt {
        Stmt::Assign(a) => {
            if let Some(name) = lhs_base_name(&a.target) {
                out.entry(name).or_insert(a.span);
            }
        }
        Stmt::IfElse(ie) => {
            collect_stmts(&ie.then_stmts, out);
            collect_stmts(&ie.else_stmts, out);
        }
        Stmt::Match(ms) => {
            for arm in &ms.arms {
                collect_stmts(&arm.body, out);
            }
        }
        Stmt::For(fl) => collect_stmts(&fl.body, out),
        Stmt::Init(ib) => collect_stmts(&ib.body, out),
        Stmt::DoUntil { body, .. } => collect_stmts(body, out),
        Stmt::Log(_) | Stmt::WaitUntil(_, _) => {}
    }
}

#[allow(dead_code)]
fn collect_thread_stmts(stmts: &[ThreadStmt], out: &mut HashMap<String, Span>) {
    for s in stmts {
        collect_one_thread_stmt(s, out);
    }
}

#[allow(dead_code)]
fn collect_one_thread_stmt(stmt: &ThreadStmt, out: &mut HashMap<String, Span>) {
    match stmt {
        ThreadStmt::CombAssign(a)
        | ThreadStmt::SeqAssign(a)
        | ThreadStmt::ForkTlmAssign(a) => {
            if let Some(name) = lhs_base_name(&a.target) {
                out.entry(name).or_insert(a.span);
            }
        }
        ThreadStmt::IfElse(ie) => {
            collect_thread_stmts(&ie.then_stmts, out);
            collect_thread_stmts(&ie.else_stmts, out);
        }
        ThreadStmt::ForkJoin(branches, _) => {
            for b in branches {
                collect_thread_stmts(b, out);
            }
        }
        ThreadStmt::For { body, .. } => collect_thread_stmts(body, out),
        ThreadStmt::Lock { body, .. } => collect_thread_stmts(body, out),
        ThreadStmt::DoUntil { body, .. } => collect_thread_stmts(body, out),
        ThreadStmt::WaitUntil(_, _)
        | ThreadStmt::WaitUntilMealy(_, _)
        | ThreadStmt::WaitCycles(_, _)
        | ThreadStmt::JoinAll(_)
        | ThreadStmt::Log(_)
        | ThreadStmt::Return(_, _) => {}
    }
}

/// Build a per-signal driver map for `m`.
///
/// Each entry is the ordered list of block-level drivers for that signal.
/// Multiple assignments *within the same block* collapse to a single
/// `DriveEntry` — a `comb` block with a `default` + an `if`-override is
/// one logical driver, not two.  Conflicts arise only between *different*
/// blocks (e.g. two `inst` outputs, or two `comb` blocks).
///
/// `source` is used to look up child module port declarations so that
/// bus-port connections (which are legitimately present on both the
/// initiator and target sides of a bus wire) can be excluded.
pub fn collect_module_drivers(m: &ModuleDecl, source: &SourceFile) -> HashMap<String, Vec<DriveEntry>> {
    let mut drivers: HashMap<String, Vec<DriveEntry>> = HashMap::new();

    // Bus and struct wires (TypeExpr::Named) are legitimately connected from
    // multiple inst items — e.g. `wire link: Mem` is connected from both the
    // initiator (`m -> link`) and the target (`s -> link`), each driving a
    // disjoint subset of the bus fields.  Tracking these through inst output
    // connections would produce false-positive multi-driver errors, so we
    // build a skip-set of their names.
    let named_wire_names: std::collections::HashSet<&str> = m
        .body
        .iter()
        .filter_map(|item| {
            if let ModuleBodyItem::WireDecl(w) = item {
                if matches!(&w.ty, TypeExpr::Named(_)) {
                    return Some(w.name.name.as_str());
                }
            }
            None
        })
        .collect();

    for item in &m.body {
        // Collect signals driven by this block (one entry per signal —
        // deduplicates repeated assignments inside the same block).
        let block_targets: HashMap<String, Span> = match item {
            ModuleBodyItem::CombBlock(b) => {
                let mut t = HashMap::new();
                collect_stmts(&b.stmts, &mut t);
                t
            }
            // RegBlock (seq) multi-driver checking is deferred: the TLM
            // target-thread inline lowering generates multiple RegBlocks that
            // legitimately share register assignments (gated by state
            // conditions), so a naive count-of-writers check produces false
            // positives here.  The C-seq repro from issue #375 needs a
            // follow-up PR that can distinguish user-written from
            // compiler-generated blocks before this check is safe to enable.
            ModuleBodyItem::RegBlock(_) => HashMap::new(),
            // Latch blocks are similarly deferred.
            ModuleBodyItem::LatchBlock(_) => HashMap::new(),
            // Thread items are only present at typecheck time when threads
            // have NOT been lowered (e.g. `--thread-sim parallel` mode).
            // Two threads driving the same signal is intentional (the FSM
            // combines them into one always_ff), so skip this context too.
            ModuleBodyItem::Thread(_) => HashMap::new(),
            ModuleBodyItem::Inst(inst) => {
                let mut t = HashMap::new();
                for conn in &inst.connections {
                    if conn.direction == ConnectDir::Output {
                        if let Some(name) = lhs_base_name(&conn.signal) {
                            // Skip connections to explicitly-declared bus/struct
                            // wires — these are driven by multiple inst items by
                            // design.
                            if named_wire_names.contains(name.as_str()) {
                                continue;
                            }
                            // Skip connections where the child port is a bus
                            // port — both initiator and target instances connect
                            // their bus ports as Output to the same (possibly
                            // implicit) bus wire, driving disjoint subsets of
                            // its flat signals.
                            if is_bus_port_in_child(
                                &inst.module_name.name,
                                &conn.port_name.name,
                                source,
                            ) {
                                continue;
                            }
                            t.entry(name).or_insert(conn.span);
                        }
                    }
                }
                t
            }
            // The items below don't generate drive edges in the parent module.
            ModuleBodyItem::RegDecl(_)
            | ModuleBodyItem::WireDecl(_)
            | ModuleBodyItem::PipeRegDecl(_)
            | ModuleBodyItem::LetBinding(_)
            | ModuleBodyItem::Generate(_)
            | ModuleBodyItem::Resource(_)
            | ModuleBodyItem::Assert(_)
            | ModuleBodyItem::Function(_)
            | ModuleBodyItem::TlmConnect(_)
            | ModuleBodyItem::TypeAlias(_) => continue,
        };

        for (name, span) in block_targets {
            drivers.entry(name).or_default().push(DriveEntry { span });
        }
    }

    drivers
}

/// Check for multi-driver conflicts.
///
/// Returns one `CompileError::MultipleDrivers` per signal that is driven
/// by two or more distinct blocks.  Signals annotated `shared(or)` or
/// `shared(and)` on the enclosing module's port list are exempt — they are
/// intentionally multi-driven with compiler-synthesized reduction logic.
pub fn check_multi_driver(
    m: &ModuleDecl,
    drivers: &HashMap<String, Vec<DriveEntry>>,
) -> Vec<CompileError> {
    let shared: std::collections::HashSet<&str> = m
        .ports
        .iter()
        .filter(|p| p.shared.is_some())
        .map(|p| p.name.name.as_str())
        .collect();

    let mut errors: Vec<CompileError> = Vec::new();
    for (name, entries) in drivers {
        if entries.len() < 2 {
            continue;
        }
        if shared.contains(name.as_str()) {
            continue;
        }
        errors.push(CompileError::MultipleDrivers {
            name: name.clone(),
            span: span_to_source_span(entries[1].span),
        });
    }
    errors.sort_by_key(|e| {
        if let CompileError::MultipleDrivers { span, .. } = e {
            span.offset()
        } else {
            0
        }
    });
    errors
}
