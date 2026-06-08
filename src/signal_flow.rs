use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::*;
use crate::comb_graph;
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

/// Extracts the integer value of a compile-time-constant literal index.
///
/// Returns `Some(v)` only for bare numeric literals (`out[2]`).  Any
/// non-literal index — a variable (`out[i]`), an arithmetic expression,
/// or a param reference — returns `None`.  This is deliberately narrow:
/// a literal index targets ONE element that downstream codegen emits as a
/// distinct flat signal, whereas a variable index conservatively aliases
/// the whole vector (a single `always_comb` writing `out[idx]` is one
/// driver regardless of which element it hits at runtime).
fn const_index_value(idx: &Expr) -> Option<u64> {
    match &idx.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v)) => Some(*v),
        ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v),
        _ => None,
    }
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

/// Like [`lhs_base_name`], but preserves a trailing **constant-literal**
/// index as part of the key (`out[0]` → `"out[0]"`, `out[i]` → `"out"`).
///
/// Used ONLY for `inst` output connections.  An inst output wired to a
/// vector element (`last -> out[0]`) lowers to a continuous driver of that
/// single element (`.last(out[0])` in the SV port map), so two inst items
/// driving distinct constant elements (`out[0]` and `out[1]`) are NOT a
/// conflict.  This is the shape a `generate_for` over a Vec-of-bus port
/// unrolls into at elaboration time: N separate inst blocks each driving
/// `out[<const i>]`.  Collapsing them to a bare `out` reported a phantom
/// multi-driver.
///
/// A *variable* index (`out[i]`) is still stripped to the bare name — it
/// conservatively aliases the whole vector — so two inst items driving the
/// same constant element (`out[0]` twice) still collide, preserving
/// genuine multi-driver detection.
///
/// This granularity is deliberately NOT applied to `comb`/`seq` blocks:
/// each such block is one `always_comb`/`always_ff` over the whole array,
/// so two blocks writing different constant indices of one Vec ARE a real
/// SV conflict and must keep erroring (see
/// `test_multi_driver_vec_index_from_two_blocks_errors`).
fn inst_conn_driver_name(expr: &Expr) -> Option<String> {
    if let ExprKind::Index(base, idx) = &expr.kind {
        if let Some(v) = const_index_value(idx) {
            return lhs_base_name(base).map(|b| format!("{}[{}]", b, v));
        }
    }
    lhs_base_name(expr)
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

fn collect_thread_stmts(stmts: &[ThreadStmt], out: &mut HashMap<String, Span>) {
    for s in stmts {
        collect_one_thread_stmt(s, out);
    }
}

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
                        if let Some(name) = inst_conn_driver_name(&conn.signal) {
                            // Skip connections to explicitly-declared bus/struct
                            // wires — these are driven by multiple inst items by
                            // design.  Match on the BASE name: `name` may carry a
                            // constant-index suffix (`link[0]`), but the skip-set
                            // is keyed by the bare declaration name (`link`).
                            let base = lhs_base_name(&conn.signal);
                            if base
                                .as_deref()
                                .map(|b| named_wire_names.contains(b))
                                .unwrap_or(false)
                            {
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

// ─────────────────────────────────────────────────────────────────────────────
// Dead-skid combinational-feedback analysis (issue #245)
//
// A `thread` lowers to an FSM with compiler-inserted "dead-skid" sub-states
// (e.g. the cycle after a `wait until`).  During those cycles the output ports
// the thread was driving fall to their default value.  If the thread later
// READS a signal that is a same-cycle *combinational* function of a signal the
// thread itself DRIVES — including paths that cross a child-instance boundary —
// it can observe a stale/spurious value mid-walk.  This was the most expensive
// single class of bug in the arch-ibex Phase A work (pitfall #11).
//
// This analysis is the pure detector: given a module (BEFORE thread lowering,
// so `ModuleBodyItem::Thread` is still present) and the enclosing source file,
// it returns the set of (thread, driven-signal → comb-path → read-signal)
// hazards.  Wiring into `arch check` and diagnostic rendering are layered on
// top of this in a follow-up.
//
// Scope (Option B — one boundary deep): cross-module comb paths are traced
// through ONE level of child instance using the child's per-output comb
// dependency map.  Hazards routed through two-or-more nested child levels are
// not reported in v1 (deferred); the arch-ibex repro and the NIC-400
// thread-drives-channel-into-child pattern are all one boundary deep.
// ─────────────────────────────────────────────────────────────────────────────

/// One dead-skid comb-feedback hazard: a `thread` reads `read_signal`, which is
/// a combinational function of `driven_signal` that the same thread drives.
#[derive(Debug, Clone)]
pub struct DeadSkidHazard {
    /// Source-level thread name (or `<anonymous>`).
    pub thread_name: String,
    /// The hazardous read signal (end of the comb path).
    pub read_signal: String,
    /// Span of the thread's read of `read_signal`.
    pub read_span: Span,
    /// A thread-driven signal that combinationally reaches `read_signal`.
    pub driven_signal: String,
    /// Signal path `driven_signal → … → read_signal` (inclusive of both ends).
    pub path: Vec<String>,
}

/// Collect the set of signal names a thread drives (write set), with the span
/// of the first write to each.  Covers the body, the `default comb` block, and
/// the `default when` soft-reset body.
pub fn thread_write_set(t: &ThreadBlock) -> HashMap<String, Span> {
    let mut out = HashMap::new();
    collect_thread_stmts(&t.body, &mut out);
    collect_stmts(&t.default_comb, &mut out);
    if let Some((_, body)) = &t.default_when {
        collect_thread_stmts(body, &mut out);
    }
    out
}

/// Collect only the **combinationally-driven** thread writes (`=` /
/// `CombAssign` and the `default comb` block).  These are the signals subject
/// to dead-skid collapse: during the compiler-inserted skid sub-states they
/// fall to their default value.  Registered drives (`<=` / `SeqAssign`,
/// `ForkTlmAssign`) HOLD across those cycles, so a comb mirror of a register is
/// stable and must NOT seed the hazard search — including them produces false
/// positives on the common `reg x_r; comb x = x_r; wait until x;` mirror shape.
pub fn thread_comb_write_set(t: &ThreadBlock) -> HashMap<String, Span> {
    let mut out = HashMap::new();
    collect_thread_comb_stmts(&t.body, &mut out);
    collect_stmts(&t.default_comb, &mut out);
    out
}

fn collect_thread_comb_stmts(stmts: &[ThreadStmt], out: &mut HashMap<String, Span>) {
    for s in stmts {
        collect_one_thread_comb_stmt(s, out);
    }
}

fn collect_one_thread_comb_stmt(stmt: &ThreadStmt, out: &mut HashMap<String, Span>) {
    match stmt {
        ThreadStmt::CombAssign(a) => {
            if let Some(name) = lhs_base_name(&a.target) {
                out.entry(name).or_insert(a.span);
            }
        }
        // Registered drives hold during dead-skid — not seeds.
        ThreadStmt::SeqAssign(_) | ThreadStmt::ForkTlmAssign(_) => {}
        ThreadStmt::IfElse(ie) => {
            collect_thread_comb_stmts(&ie.then_stmts, out);
            collect_thread_comb_stmts(&ie.else_stmts, out);
        }
        ThreadStmt::ForkJoin(branches, _) => {
            for b in branches {
                collect_thread_comb_stmts(b, out);
            }
        }
        ThreadStmt::For { body, .. } => collect_thread_comb_stmts(body, out),
        ThreadStmt::Lock { body, .. } => collect_thread_comb_stmts(body, out),
        ThreadStmt::DoUntil { body, .. } => collect_thread_comb_stmts(body, out),
        ThreadStmt::WaitUntil(_, _)
        | ThreadStmt::WaitCycles(_, _)
        | ThreadStmt::JoinAll(_)
        | ThreadStmt::Log(_)
        | ThreadStmt::Return(_, _) => {}
    }
}

/// Collect the set of signal names a thread reads (read set), with the span of
/// the first read of each. Reads come from RHS values in the body and
/// `default comb`, `wait until` / `do until` conditions, `if` conditions, loop
/// bounds, and LHS index/slice expressions.
pub fn thread_read_set(t: &ThreadBlock) -> HashMap<String, Span> {
    let mut out = HashMap::new();
    collect_thread_reads(&t.body, &mut out);
    collect_stmt_reads(&t.default_comb, &mut out);
    if let Some((cond, body)) = &t.default_when {
        add_expr_reads(cond, t.span, &mut out);
        collect_thread_reads(body, &mut out);
    }
    out
}

fn add_expr_reads(e: &Expr, span: Span, out: &mut HashMap<String, Span>) {
    let mut ids = HashSet::new();
    comb_graph::collect_expr_idents(e, &mut ids);
    for id in ids {
        out.entry(id).or_insert(span);
    }
}

/// Identifiers read inside the index / slice positions of an LHS expression
/// (e.g. `vec[i] <= x` reads `i`).  The base signal itself is a write target,
/// not a read.
fn collect_lhs_index_reads(target: &Expr, span: Span, out: &mut HashMap<String, Span>) {
    match &target.kind {
        ExprKind::Index(base, idx) => {
            add_expr_reads(idx, span, out);
            collect_lhs_index_reads(base, span, out);
        }
        ExprKind::BitSlice(base, hi, lo) => {
            add_expr_reads(hi, span, out);
            add_expr_reads(lo, span, out);
            collect_lhs_index_reads(base, span, out);
        }
        ExprKind::PartSelect(base, start, width, _) => {
            add_expr_reads(start, span, out);
            add_expr_reads(width, span, out);
            collect_lhs_index_reads(base, span, out);
        }
        ExprKind::LatencyAt(base, _) => collect_lhs_index_reads(base, span, out),
        _ => {}
    }
}

fn collect_thread_reads(stmts: &[ThreadStmt], out: &mut HashMap<String, Span>) {
    for s in stmts {
        collect_one_thread_read(s, out);
    }
}

fn collect_stmt_reads(stmts: &[Stmt], out: &mut HashMap<String, Span>) {
    for s in stmts {
        collect_one_stmt_read(s, out);
    }
}

fn collect_one_stmt_read(stmt: &Stmt, out: &mut HashMap<String, Span>) {
    match stmt {
        Stmt::Assign(a) => {
            add_expr_reads(&a.value, a.span, out);
            collect_lhs_index_reads(&a.target, a.span, out);
        }
        Stmt::IfElse(ie) => {
            add_expr_reads(&ie.cond, ie.span, out);
            collect_stmt_reads(&ie.then_stmts, out);
            collect_stmt_reads(&ie.else_stmts, out);
        }
        Stmt::Match(ms) => {
            add_expr_reads(&ms.scrutinee, ms.span, out);
            for arm in &ms.arms {
                if let Pattern::Literal(e) = &arm.pattern {
                    add_expr_reads(e, ms.span, out);
                }
                collect_stmt_reads(&arm.body, out);
            }
        }
        Stmt::For(fl) => {
            match &fl.range {
                ForRange::Range(start, end) => {
                    add_expr_reads(start, fl.span, out);
                    add_expr_reads(end, fl.span, out);
                }
                ForRange::ValueList(vals) => {
                    for v in vals {
                        add_expr_reads(v, fl.span, out);
                    }
                }
            }
            collect_stmt_reads(&fl.body, out);
        }
        Stmt::Init(ib) => collect_stmt_reads(&ib.body, out),
        Stmt::DoUntil { body, cond, span } => {
            collect_stmt_reads(body, out);
            add_expr_reads(cond, *span, out);
        }
        Stmt::WaitUntil(cond, span) => add_expr_reads(cond, *span, out),
        Stmt::Log(_) => {}
    }
}

fn collect_one_thread_read(stmt: &ThreadStmt, out: &mut HashMap<String, Span>) {
    match stmt {
        ThreadStmt::CombAssign(a)
        | ThreadStmt::SeqAssign(a)
        | ThreadStmt::ForkTlmAssign(a) => {
            add_expr_reads(&a.value, a.span, out);
            collect_lhs_index_reads(&a.target, a.span, out);
        }
        ThreadStmt::WaitUntil(c, sp) => add_expr_reads(c, *sp, out),
        ThreadStmt::WaitCycles(e, sp) => add_expr_reads(e, *sp, out),
        ThreadStmt::IfElse(ie) => {
            add_expr_reads(&ie.cond, ie.span, out);
            collect_thread_reads(&ie.then_stmts, out);
            collect_thread_reads(&ie.else_stmts, out);
        }
        ThreadStmt::ForkJoin(branches, _) => {
            for b in branches {
                collect_thread_reads(b, out);
            }
        }
        ThreadStmt::For { start, end, body, span, .. } => {
            add_expr_reads(start, *span, out);
            add_expr_reads(end, *span, out);
            collect_thread_reads(body, out);
        }
        ThreadStmt::Lock { body, .. } => collect_thread_reads(body, out),
        ThreadStmt::DoUntil { body, cond, span } => {
            collect_thread_reads(body, out);
            add_expr_reads(cond, *span, out);
        }
        ThreadStmt::Return(e, sp) => add_expr_reads(e, *sp, out),
        ThreadStmt::JoinAll(_) | ThreadStmt::Log(_) => {}
    }
}

/// Look up a child construct's per-output combinational dependency map by name.
/// Returns `output port → {input port names it combinationally depends on}`.
/// Extern / `.archi`-only children (no body in `source`) yield an empty map —
/// their hazards are not traced in v1 (deferred over-approximation).
fn child_comb_deps(module_name: &str, source: &SourceFile) -> HashMap<String, HashSet<String>> {
    for item in &source.items {
        match item {
            Item::Module(m) if m.name.name == module_name => {
                return comb_graph::per_output_comb_deps(m);
            }
            Item::Fsm(f) if f.name.name == module_name => {
                return comb_graph::per_output_comb_deps_fsm(f);
            }
            _ => {}
        }
    }
    HashMap::new()
}

/// Build the forward combinational adjacency for one module (Option B, one
/// boundary deep): `fwd[a]` = signals combinationally driven *using* `a`.
///
/// Edges come from:
///   - parent `comb` blocks and `let` bindings (each RHS / enclosing-condition
///     ident → LHS), via `comb_graph::collect_comb_deps`;
///   - one level of child instances: if a child output port `o` (wired to
///     parent signal `ps_o`) combinationally depends on a child input port `i`
///     (wired to parent signal `ps_i`), add `ps_i → ps_o`.
///
/// Registered child outputs and `seq`/`reg` parent drives are excluded (they
/// break the combinational path), matching `per_output_comb_deps`.
pub fn module_comb_fwd_edges(
    m: &ModuleDecl,
    source: &SourceFile,
) -> HashMap<String, HashSet<String>> {
    let mut fwd: HashMap<String, HashSet<String>> = HashMap::new();
    let add = |from: &str, to: &str, fwd: &mut HashMap<String, HashSet<String>>| {
        if from != to {
            fwd.entry(from.to_string()).or_default().insert(to.to_string());
        }
    };

    // 1) Parent comb blocks + let bindings → LHS-depends-on-RHS map, inverted
    //    into forward edges.
    let mut direct: HashMap<String, HashSet<String>> = HashMap::new();
    for item in &m.body {
        match item {
            ModuleBodyItem::CombBlock(cb) => {
                comb_graph::collect_comb_deps(&cb.stmts, &mut direct, &mut Vec::new());
            }
            ModuleBodyItem::LetBinding(lb) => {
                let mut ids = HashSet::new();
                comb_graph::collect_expr_idents(&lb.value, &mut ids);
                direct.entry(lb.name.name.clone()).or_default().extend(ids);
            }
            _ => {}
        }
    }
    for (lhs, rhs_set) in &direct {
        for rhs in rhs_set {
            add(rhs, lhs, &mut fwd);
        }
    }

    // 2) One level of child instances.
    for inst in comb_graph::collect_insts(m) {
        let deps = child_comb_deps(&inst.module_name.name, source);
        if deps.is_empty() {
            continue;
        }
        // child port name → parent signal name, from the connection list.
        let mut port_to_parent: HashMap<&str, String> = HashMap::new();
        for conn in &inst.connections {
            if let Some(ps) = lhs_base_name(&conn.signal) {
                port_to_parent.insert(conn.port_name.name.as_str(), ps);
            }
        }
        for (out_port, in_ports) in &deps {
            let Some(ps_o) = port_to_parent.get(out_port.as_str()) else {
                continue;
            };
            for in_port in in_ports {
                if let Some(ps_i) = port_to_parent.get(in_port.as_str()) {
                    let (ps_i, ps_o) = (ps_i.clone(), ps_o.clone());
                    add(&ps_i, &ps_o, &mut fwd);
                }
            }
        }
    }

    fwd
}

/// Signals combinationally reachable FROM `seeds` over forward adjacency `fwd`.
/// Seeds are not themselves included unless reached via a (comb) cycle.
pub fn comb_reachable_from(
    seeds: &HashSet<String>,
    fwd: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut reached: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();
    while let Some(s) = queue.pop_front() {
        if let Some(succ) = fwd.get(&s) {
            for t in succ {
                if reached.insert(t.clone()) {
                    queue.push_back(t.clone());
                }
            }
        }
    }
    reached
}

/// Reconstruct one comb path `start_set → … → target` over `fwd` (BFS shortest
/// path).  Returns the inclusive node list, or `None` if unreachable.
fn comb_path_to(
    seeds: &HashSet<String>,
    target: &str,
    fwd: &HashMap<String, HashSet<String>>,
) -> Option<Vec<String>> {
    let mut pred: HashMap<String, String> = HashMap::new();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();
    let mut visited: HashSet<String> = seeds.iter().cloned().collect();
    while let Some(s) = queue.pop_front() {
        if let Some(succ) = fwd.get(&s) {
            for t in succ {
                if visited.insert(t.clone()) {
                    pred.insert(t.clone(), s.clone());
                    if t == target {
                        // Walk predecessors back to a seed.
                        let mut path = vec![t.clone()];
                        let mut cur = t.clone();
                        while let Some(p) = pred.get(&cur) {
                            path.push(p.clone());
                            cur = p.clone();
                        }
                        path.reverse();
                        return Some(path);
                    }
                    queue.push_back(t.clone());
                }
            }
        }
    }
    None
}

/// Find all dead-skid comb-feedback hazards in `m`.  `m` must be the
/// PRE-thread-lowering module (so `ModuleBodyItem::Thread` is still present).
/// Hazards are de-duplicated per (thread, read_signal): the shortest driving
/// path is reported.
pub fn find_dead_skid_hazards(m: &ModuleDecl, source: &SourceFile) -> Vec<DeadSkidHazard> {
    let threads: Vec<&ThreadBlock> = m
        .body
        .iter()
        .filter_map(|it| match it {
            ModuleBodyItem::Thread(t) => Some(t),
            _ => None,
        })
        .collect();
    if threads.is_empty() {
        return Vec::new();
    }

    let fwd = module_comb_fwd_edges(m, source);
    let mut hazards = Vec::new();

    for t in threads {
        // Seed only from comb-driven writes: dead-skid collapse affects `=`
        // drives, not registered `<=` values (which hold across skid cycles).
        let writes = thread_comb_write_set(t);
        let reads = thread_read_set(t);
        if writes.is_empty() || reads.is_empty() {
            continue;
        }
        let write_set: HashSet<String> = writes.keys().cloned().collect();
        let read_set: HashSet<String> = reads.keys().cloned().collect();
        let reachable = comb_reachable_from(&write_set, &fwd);

        let thread_name = t
            .name
            .as_ref()
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "<anonymous>".to_string());

        // Deterministic order: sort the hazardous reads by name.
        let mut hits: Vec<&String> = reachable.intersection(&read_set).collect();
        hits.sort();
        for read_sig in hits {
            let path = comb_path_to(&write_set, read_sig, &fwd).unwrap_or_default();
            let driven_signal = path.first().cloned().unwrap_or_default();
            hazards.push(DeadSkidHazard {
                thread_name: thread_name.clone(),
                read_signal: read_sig.clone(),
                read_span: reads.get(read_sig).copied().unwrap_or(t.span),
                driven_signal,
                path,
            });
        }
    }

    hazards
}
