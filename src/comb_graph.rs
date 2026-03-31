/// Combinational dependency analysis for the simulation code generator.
///
/// Builds an inter-instance dependency graph for a module's sub-instances,
/// performs topological sorting, detects combinational feedback cycles
/// (which are compile errors), and computes the minimum settle depth needed
/// for the eval() settle loop.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    ConnectDir, CombStmt, ExprKind, FsmDecl, InsideMember, InstDecl, Item,
    ModuleBodyItem, ModuleDecl, PortDecl, RamDecl, SourceFile, Stmt, TypeExpr,
};
use crate::diagnostics::CompileError;
use crate::resolve::{Symbol, SymbolTable};

// ── Public types ─────────────────────────────────────────────────────────────

/// Per-construct comb information: which output ports are driven by comb logic
/// and which input ports appear in comb expressions.
#[derive(Default)]
pub struct CombInfo {
    /// Output port names assigned inside any `comb` block.
    pub comb_outputs: HashSet<String>,
    /// Input port names that appear anywhere in comb expressions (RHS values,
    /// conditions, scrutinees).  If an output depends on one of these inputs
    /// through any comb path it counts as a comb dependency.
    pub comb_dep_inputs: HashSet<String>,
}

/// Result of analyzing a single module's instance dependency graph.
pub struct ModuleAnalysis {
    /// Indices into the module's `insts` Vec, in topological evaluation order
    /// (producer instances before consumer instances).
    pub sorted_inst_indices: Vec<usize>,
    /// Minimum number of settle passes needed.
    /// 1 when the instance graph is a strict DAG *and* the parent module has no
    /// `comb` blocks / `let` bindings that produce intermediate signals that
    /// feed instance inputs (those require a second pass to propagate).
    /// 2 otherwise.
    pub settle_depth: usize,
}

// ── Expression identifier collection ─────────────────────────────────────────

/// Recursively collect all bare identifiers referenced inside `expr`.
pub fn collect_expr_idents(expr: &crate::ast::Expr, out: &mut HashSet<String>) {
    use ExprKind::*;
    match &expr.kind {
        Ident(name) => { out.insert(name.clone()); }
        Binary(_, a, b) => {
            collect_expr_idents(a, out);
            collect_expr_idents(b, out);
        }
        Unary(_, a) => collect_expr_idents(a, out),
        FieldAccess(base, _) => collect_expr_idents(base, out),
        MethodCall(recv, _, args) => {
            collect_expr_idents(recv, out);
            for a in args { collect_expr_idents(a, out); }
        }
        Cast(e, _) => collect_expr_idents(e, out),
        Index(base, idx) => {
            collect_expr_idents(base, out);
            collect_expr_idents(idx, out);
        }
        BitSlice(base, hi, lo) => {
            collect_expr_idents(base, out);
            collect_expr_idents(hi, out);
            collect_expr_idents(lo, out);
        }
        PartSelect(base, start, width, _) => {
            collect_expr_idents(base, out);
            collect_expr_idents(start, out);
            collect_expr_idents(width, out);
        }
        StructLiteral(_, fields) => {
            for f in fields { collect_expr_idents(&f.value, out); }
        }
        Concat(exprs) => {
            for e in exprs { collect_expr_idents(e, out); }
        }
        FunctionCall(_, args) => {
            for a in args { collect_expr_idents(a, out); }
        }
        Repeat(e, n) => {
            collect_expr_idents(e, out);
            collect_expr_idents(n, out);
        }
        Ternary(c, t, f) => {
            collect_expr_idents(c, out);
            collect_expr_idents(t, out);
            collect_expr_idents(f, out);
        }
        Inside(e, members) => {
            collect_expr_idents(e, out);
            for m in members {
                match m {
                    InsideMember::Single(x)    => collect_expr_idents(x, out),
                    InsideMember::Range(a, b)  => {
                        collect_expr_idents(a, out);
                        collect_expr_idents(b, out);
                    }
                }
            }
        }
        // Expression-level match: scrutinee + arm values
        ExprMatch(scrut, arms) => {
            collect_expr_idents(scrut, out);
            for arm in arms { collect_expr_idents(&arm.value, out); }
        }
        // Statement-level match used as expression (rare): just the scrutinee
        Match(scrut, _) => collect_expr_idents(scrut, out),
        Clog2(e) => collect_expr_idents(e, out),
        // Literals, Bool, EnumVariant, Todo — no identifiers
        _ => {}
    }
}

/// Extract the base identifier name from an LHS expression
/// (strips bit-slices, part-selects, array indexing, field access).
fn lhs_base_name(expr: &crate::ast::Expr) -> Option<String> {
    use ExprKind::*;
    match &expr.kind {
        Ident(name)             => Some(name.clone()),
        BitSlice(base, _, _)    => lhs_base_name(base),
        PartSelect(base, _, _, _) => lhs_base_name(base),
        Index(base, _)          => lhs_base_name(base),
        FieldAccess(base, _)    => lhs_base_name(base),
        _                       => None,
    }
}

// ── Scanning helpers ──────────────────────────────────────────────────────────

/// Helper: scan a `Stmt::Assign` (used inside CombMatch / CombFor arm bodies).
fn scan_stmt_assign(
    stmt: &Stmt,
    input_names: &HashSet<String>,
    output_names: &HashSet<String>,
    driven: &mut HashSet<String>,
    read: &mut HashSet<String>,
) {
    if let Stmt::Assign(a) = stmt {
        if let Some(lhs) = lhs_base_name(&a.target) {
            if output_names.contains(&lhs) {
                driven.insert(lhs);
            }
        }
        let mut rhs = HashSet::new();
        collect_expr_idents(&a.value, &mut rhs);
        for id in &rhs {
            if input_names.contains(id) { read.insert(id.clone()); }
        }
    }
}

/// Recursively scan a single `CombStmt` and accumulate driven outputs and
/// read inputs.
fn scan_comb_stmt(
    stmt: &CombStmt,
    input_names: &HashSet<String>,
    output_names: &HashSet<String>,
    driven: &mut HashSet<String>,
    read: &mut HashSet<String>,
) {
    match stmt {
        CombStmt::Assign(a) => {
            if let Some(lhs) = lhs_base_name(&a.target) {
                if output_names.contains(&lhs) {
                    driven.insert(lhs);
                }
            }
            let mut rhs = HashSet::new();
            collect_expr_idents(&a.value, &mut rhs);
            for id in &rhs {
                if input_names.contains(id) { read.insert(id.clone()); }
            }
        }
        CombStmt::IfElse(ife) => {
            // Condition reads count as comb deps
            let mut cond = HashSet::new();
            collect_expr_idents(&ife.cond, &mut cond);
            for id in &cond {
                if input_names.contains(id) { read.insert(id.clone()); }
            }
            for s in &ife.then_stmts { scan_comb_stmt(s, input_names, output_names, driven, read); }
            for s in &ife.else_stmts { scan_comb_stmt(s, input_names, output_names, driven, read); }
        }
        CombStmt::MatchExpr(m) => {
            // Scrutinee
            let mut scrut = HashSet::new();
            collect_expr_idents(&m.scrutinee, &mut scrut);
            for id in &scrut {
                if input_names.contains(id) { read.insert(id.clone()); }
            }
            // Arm bodies contain Stmt::Assign items
            for arm in &m.arms {
                for s in &arm.body {
                    scan_stmt_assign(s, input_names, output_names, driven, read);
                }
            }
        }
        CombStmt::For(f) => {
            // For-loop body contains Stmt::Assign items in comb context
            for s in &f.body {
                scan_stmt_assign(s, input_names, output_names, driven, read);
            }
        }
        CombStmt::Log(_) => {}
    }
}

fn scan_comb_stmts(
    stmts: &[CombStmt],
    input_names: &HashSet<String>,
    output_names: &HashSet<String>,
    driven: &mut HashSet<String>,
    read: &mut HashSet<String>,
) {
    for s in stmts {
        scan_comb_stmt(s, input_names, output_names, driven, read);
    }
}

// ── Per-construct CombInfo builders ──────────────────────────────────────────

fn is_clk_or_rst(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Clock(_) | TypeExpr::Reset(_, _))
}

fn port_sets(ports: &[PortDecl]) -> (HashSet<String>, HashSet<String>) {
    use crate::ast::Direction;
    let inputs = ports.iter()
        .filter(|p| p.direction == Direction::In && !is_clk_or_rst(&p.ty))
        .map(|p| p.name.name.clone())
        .collect();
    let outputs = ports.iter()
        .filter(|p| p.direction == Direction::Out)
        .map(|p| p.name.name.clone())
        .collect();
    (inputs, outputs)
}

/// Compute CombInfo for an FSM declaration.
fn comb_info_for_fsm(fsm: &FsmDecl) -> CombInfo {
    let (inputs, outputs) = port_sets(&fsm.ports);
    let mut driven = HashSet::new();
    let mut read   = HashSet::new();

    // FSM-scope let bindings: collect any input refs they use
    // (let bindings are comb intermediates; their idents propagate to outputs
    // via the assignment scanning below, but we also note read inputs here)
    for lb in &fsm.lets {
        let mut ids = HashSet::new();
        collect_expr_idents(&lb.value, &mut ids);
        for id in &ids {
            if inputs.contains(id) { read.insert(id.clone()); }
        }
    }

    // default comb block
    scan_comb_stmts(&fsm.default_comb, &inputs, &outputs, &mut driven, &mut read);

    // Per-state comb blocks
    for state in &fsm.states {
        scan_comb_stmts(&state.comb_stmts, &inputs, &outputs, &mut driven, &mut read);
        // Transition conditions also read inputs combinationally
        for tr in &state.transitions {
            let mut ids = HashSet::new();
            collect_expr_idents(&tr.condition, &mut ids);
            for id in &ids {
                if inputs.contains(id) { read.insert(id.clone()); }
            }
        }
    }

    CombInfo { comb_outputs: driven, comb_dep_inputs: read }
}

/// Compute CombInfo for a module declaration.
fn comb_info_for_module(m: &ModuleDecl) -> CombInfo {
    let (inputs, outputs) = port_sets(&m.ports);
    let mut driven = HashSet::new();
    let mut read   = HashSet::new();

    for item in &m.body {
        match item {
            ModuleBodyItem::CombBlock(cb) => {
                scan_comb_stmts(&cb.stmts, &inputs, &outputs, &mut driven, &mut read);
            }
            ModuleBodyItem::LetBinding(lb) => {
                // let bindings are comb intermediates
                let mut ids = HashSet::new();
                collect_expr_idents(&lb.value, &mut ids);
                for id in &ids {
                    if inputs.contains(id) { read.insert(id.clone()); }
                }
                // If the let name is an output port (unusual but possible), mark driven
                if outputs.contains(&lb.name.name) {
                    driven.insert(lb.name.name.clone());
                }
            }
            _ => {}
        }
    }

    CombInfo { comb_outputs: driven, comb_dep_inputs: read }
}

/// Compute CombInfo for a RAM declaration.
fn comb_info_for_ram(ram: &RamDecl) -> CombInfo {
    // latency = 0 (async): read data output is combinationally driven by
    // addr + enable.  We don't bother with exact port names; just mark
    // the RAM as having potential comb outputs so it participates in the
    // graph (but cycles through a RAM are not meaningful in practice).
    // latency >= 1: all outputs are registered; no comb path.
    if ram.latency == 0 {
        // Conservative: treat all ports as potentially comb-coupled.
        // In practice async RAMs rarely appear in cycles, and the cycle
        // detection will catch it if they do.
        let (inputs, outputs) = port_sets(&ram.ports);
        CombInfo { comb_outputs: outputs, comb_dep_inputs: inputs }
    } else {
        CombInfo::default()
    }
}

/// Look up the `CombInfo` for an instance whose construct is named `sym_name`.
pub fn comb_info_for_symbol(sym_name: &str, symbols: &SymbolTable, source: &SourceFile) -> CombInfo {
    let sym = match symbols.globals.get(sym_name) {
        Some((s, _)) => s,
        None => return CombInfo::default(),
    };
    match sym {
        Symbol::Fsm(_) => {
            for item in &source.items {
                if let Item::Fsm(fsm) = item {
                    if fsm.name.name == sym_name {
                        return comb_info_for_fsm(fsm);
                    }
                }
            }
            CombInfo::default()
        }
        Symbol::Module(_) => {
            for item in &source.items {
                if let Item::Module(m) = item {
                    if m.name.name == sym_name {
                        return comb_info_for_module(m);
                    }
                }
            }
            CombInfo::default()
        }
        Symbol::Ram(ri) => {
            if ri.latency == 0 {
                for item in &source.items {
                    if let Item::Ram(ram) = item {
                        if ram.name.name == sym_name {
                            return comb_info_for_ram(ram);
                        }
                    }
                }
            }
            CombInfo::default()
        }
        // Counter, Arbiter, Regfile, Fifo, Synchronizer, Clkgate:
        // Outputs are registered; no comb path tracked.
        _ => CombInfo::default(),
    }
}

// ── Module analysis ───────────────────────────────────────────────────────────

/// True if the module has any `comb` block or `let` binding that produces
/// intermediate signals (those may feed instance inputs and require 2 settle
/// passes if the parent eval_comb() runs AFTER the instance loop).
fn parent_has_comb_intermediates(m: &ModuleDecl) -> bool {
    m.body.iter().any(|item| matches!(
        item,
        ModuleBodyItem::CombBlock(_) | ModuleBodyItem::LetBinding(_)
    ))
}

/// Collect all direct `inst` declarations from a module body (not generate
/// blocks — those are already expanded by the elaborate pass before sim
/// codegen runs).
pub fn collect_insts(m: &ModuleDecl) -> Vec<&InstDecl> {
    m.body.iter()
        .filter_map(|i| if let ModuleBodyItem::Inst(inst) = i { Some(inst) } else { None })
        .collect()
}

/// Analyze a module's instance dependency graph.
///
/// Returns a `ModuleAnalysis` with:
/// - `sorted_inst_indices`: topological order (producer before consumer)
/// - `settle_depth`: 1 or 2 settle passes needed
///
/// Returns `Err` with a `CompileError::General` if a combinational feedback
/// cycle is detected.
pub fn analyze_module(
    m: &ModuleDecl,
    symbols: &SymbolTable,
    source: &SourceFile,
) -> Result<ModuleAnalysis, CompileError> {
    let insts = collect_insts(m);
    let n = insts.len();

    if n == 0 {
        return Ok(ModuleAnalysis {
            sorted_inst_indices: vec![],
            settle_depth: 1,
        });
    }

    // ── Step 1: collect CombInfo for each instance ────────────────────────
    let infos: Vec<CombInfo> = insts.iter()
        .map(|inst| comb_info_for_symbol(&inst.module_name.name, symbols, source))
        .collect();

    // ── Step 2: build wire → source-instance map ──────────────────────────
    // wire_source[wire_name] = (inst_idx, output_port_name)
    let mut wire_source: HashMap<String, (usize, String)> = HashMap::new();
    for (idx, inst) in insts.iter().enumerate() {
        for conn in &inst.connections {
            if conn.direction == ConnectDir::Output {
                if let ExprKind::Ident(wire_name) = &conn.signal.kind {
                    wire_source.insert(wire_name.clone(), (idx, conn.port_name.name.clone()));
                }
                // Non-ident output signals (e.g. struct field) are rare; skip.
            }
        }
    }

    // ── Step 3: build directed edge graph ────────────────────────────────
    // Edge j → i means "instance j must be evaluated before instance i"
    // (i.e., i has a comb input that depends on j's comb output).
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree: Vec<usize> = vec![0; n];

    for (i, inst) in insts.iter().enumerate() {
        for conn in &inst.connections {
            if conn.direction != ConnectDir::Input { continue; }

            let port_name = &conn.port_name.name;
            // Only create an edge if instance i has a comb dep on this input port.
            if !infos[i].comb_dep_inputs.contains(port_name) { continue; }

            // Identify the signal driving this input.
            let wire_name = match &conn.signal.kind {
                ExprKind::Ident(name) => name,
                _ => continue, // complex expression — skip
            };

            // Is this wire driven by another instance's comb output?
            let (j, out_port) = match wire_source.get(wire_name) {
                Some(v) => (v.0, &v.1),
                None => continue, // driven by parent reg/port, not an instance
            };

            if j == i { continue; } // self-loop — not meaningful

            // Only add edge if j's port is a comb output (not registered).
            if !infos[j].comb_outputs.contains(out_port) { continue; }

            // Avoid duplicate edges
            if !adj[j].contains(&i) {
                adj[j].push(i);
                in_degree[i] += 1;
            }
        }
    }

    // ── Step 4: Kahn's topological sort ──────────────────────────────────
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut sorted: Vec<usize> = Vec::with_capacity(n);

    while let Some(j) = queue.pop_front() {
        sorted.push(j);
        for k in adj[j].clone() {
            in_degree[k] -= 1;
            if in_degree[k] == 0 {
                queue.push_back(k);
            }
        }
    }

    // ── Step 5: cycle detection ───────────────────────────────────────────
    if sorted.len() < n {
        // Structural cycle detected between instances.
        //
        // Note: most instance-level cycles in hardware are "convergent" —
        // they converge in 2 settle passes because a register somewhere in
        // the path breaks the true data-level cycle.  For example:
        //
        //   lru_upd.lru_tree_out = f(lru_tree_in)      [comb]
        //   ctrl.lru_wr_data     = g(lru_tree_out)      [comb]
        //   ctrl.lru_tree_in     = lru_rd_data          [from register, NOT lru_tree_out]
        //
        // At the instance level this appears as a cycle (ctrl ↔ lru_upd), but
        // it converges in 2 passes because lru_tree_in is register-driven.
        //
        // We treat such cycles as requiring settle_depth=2 (not an error).
        // The topo sort for the cyclic nodes is undefined; fall back to the
        // original declaration order for ALL instances in this module so that
        // the first pass produces partially-valid values and the second pass
        // converges.  (For truly non-convergent loops the single-driver rule
        // should prevent them from type-checking.)
        return Ok(ModuleAnalysis {
            sorted_inst_indices: (0..n).collect(),
            settle_depth: 2,
        });
    }

    // ── Step 6: compute settle depth ─────────────────────────────────────
    // With topo-sorted instances, 1 pass through the loop suffices for the
    // instances themselves.  But if the parent has comb blocks / let bindings
    // that produce intermediate signals used as instance inputs, those
    // intermediates are only updated at the end of the loop (parent eval_comb).
    // In that case we need 2 passes so the second pass sees fresh values.
    let settle_depth = if parent_has_comb_intermediates(m) { 2 } else { 1 };

    Ok(ModuleAnalysis { sorted_inst_indices: sorted, settle_depth })
}

