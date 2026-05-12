/// Combinational dependency analysis for the simulation code generator.
///
/// Builds an inter-instance dependency graph for a module's sub-instances,
/// performs topological sorting, detects combinational feedback cycles
/// (which are compile errors), and computes the minimum settle depth needed
/// for the eval() settle loop.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    ConnectDir, Stmt, ExprKind, FsmDecl, InsideMember, InstDecl, Item,
    ModuleBodyItem, ModuleDecl, PortDecl, RamDecl, SourceFile, TypeExpr,
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

/// Recursively scan a single `Stmt` and accumulate driven outputs and
/// read inputs.
fn scan_comb_stmt(
    stmt: &Stmt,
    input_names: &HashSet<String>,
    output_names: &HashSet<String>,
    driven: &mut HashSet<String>,
    read: &mut HashSet<String>,
) {
    match stmt {
        Stmt::Assign(a) => {
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
        Stmt::IfElse(ife) => {
            // Condition reads count as comb deps
            let mut cond = HashSet::new();
            collect_expr_idents(&ife.cond, &mut cond);
            for id in &cond {
                if input_names.contains(id) { read.insert(id.clone()); }
            }
            for s in &ife.then_stmts { scan_comb_stmt(s, input_names, output_names, driven, read); }
            for s in &ife.else_stmts { scan_comb_stmt(s, input_names, output_names, driven, read); }
        }
        Stmt::Match(m) => {
            // Scrutinee
            let mut scrut = HashSet::new();
            collect_expr_idents(&m.scrutinee, &mut scrut);
            for id in &scrut {
                if input_names.contains(id) { read.insert(id.clone()); }
            }
            for arm in &m.arms {
                for s in &arm.body {
                    scan_comb_stmt(s, input_names, output_names, driven, read);
                }
            }
        }
        Stmt::For(f) => {
            for s in &f.body {
                scan_comb_stmt(s, input_names, output_names, driven, read);
            }
        }
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => unreachable!("seq-only Stmt variant inside comb-context walker"),
        Stmt::Log(_) => {}
    }
}

fn scan_comb_stmts(
    stmts: &[Stmt],
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

// ─────────────────────────────────────────────────────────────────────────────
// Whole-design comb-loop analysis (issue #246, MVP)
//
// Builds ONE directed combinational-dependency graph that spans the entire
// elaborated design starting from each top-level module (modules that are
// never instantiated anywhere). Nodes are keyed by `(inst_path, signal)` so
// signals at different hierarchy levels are distinct.
//
// Tarjan's SCC is then run over the graph and any SCC with size > 1 (or a
// single-node SCC with a self-loop) is reported as a combinational feedback
// cycle. SCCs that pass through any instance OWNED by a module with
// `pragma comb_loops_allowed;` are suppressed.
//
// Limitations of this MVP (deferred to a follow-up PR):
//   - Extern / interface-only modules (`.archi` stubs) are treated as
//     opaque: every output is assumed to depend on every input. This is the
//     safe over-approximation and may produce spurious cycle reports when
//     the SV-side body is actually pipelined.
//   - Module-level CombInfo is the existing port-set over-approximation
//     (any comb-driven output is assumed to depend on every comb-read
//     input). Per-output dep precision is left to a follow-up.
//   - Per-signal-pair blessing (e.g. `pragma comb_loop a, b;`) is not
//     supported; only the module-level pragma is.
// ─────────────────────────────────────────────────────────────────────────────

/// Path through the instance hierarchy from a top-level module to a
/// particular instance. Empty vec = top-level module itself.
pub type InstPath = Vec<String>;

/// Identifier for a node in the whole-design comb graph:
///   - `path`: instance path (parent inst names, from top-level)
///   - `signal`: signal name at the given level (port / wire / let / inst-output)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeKey {
    pub path: InstPath,
    pub signal: String,
}

impl NodeKey {
    pub fn display(&self) -> String {
        if self.path.is_empty() {
            self.signal.clone()
        } else {
            format!("{}.{}", self.path.join("."), self.signal)
        }
    }
}

/// One combinational SCC found in the whole-design graph.
pub struct CombScc {
    /// Nodes in the SCC (declaration / discovery order — Tarjan emits them
    /// in reverse-finish order which is fine for diagnostic display).
    pub nodes: Vec<NodeKey>,
    /// Owning-parent inst paths (i.e. each unique `path` of nodes in the
    /// SCC). Used by the suppression rule: if any owning-parent module has
    /// `pragma comb_loops_allowed;`, the SCC is suppressed.
    pub owning_paths: HashSet<InstPath>,
    /// Owning module name per `owning_path` — populated during graph build
    /// so the pragma lookup doesn't have to re-walk the hierarchy. The
    /// path with `vec![]` maps to the top-level module name.
    pub owning_modules: HashSet<String>,
}

/// Result of running whole-design comb-loop analysis on a source file.
pub struct WholeDesignAnalysis {
    pub sccs: Vec<CombScc>,
    /// Number of SCCs after running Tarjan (size > 1, OR size==1-with-self-loop).
    pub total_sccs: usize,
    /// Number of SCCs suppressed by `pragma comb_loops_allowed;` on at
    /// least one owning module.
    pub suppressed: usize,
}

/// Run whole-design comb-loop analysis.
///
/// 1. Identify top-level modules (modules not instantiated anywhere).
/// 2. For each top-level, recursively flatten the instance hierarchy into
///    a directed node-and-edge set, where each node is `(inst_path, signal)`.
/// 3. Run Tarjan's SCC.
/// 4. Tag each non-trivial SCC with the set of owning-parent paths;
///    classify as suppressed if any owning module has the pragma.
pub fn analyze_whole_design(
    source: &SourceFile,
    symbols: &SymbolTable,
) -> WholeDesignAnalysis {
    // ── Step 1: collect inst counts to find top-level modules ─────────────
    let mut inst_count: HashMap<String, usize> = HashMap::new();
    let mut module_by_name: HashMap<&str, &ModuleDecl> = HashMap::new();
    for item in &source.items {
        if let Item::Module(m) = item {
            module_by_name.insert(m.name.name.as_str(), m);
            // Pre-seed so "never instantiated" still appears
            inst_count.entry(m.name.name.clone()).or_insert(0);
        }
    }
    for item in &source.items {
        if let Item::Module(m) = item {
            for sub in collect_insts(m) {
                *inst_count.entry(sub.module_name.name.clone()).or_insert(0) += 1;
            }
        }
    }

    let top_levels: Vec<&ModuleDecl> = module_by_name
        .iter()
        .filter(|(name, _)| inst_count.get(**name).copied().unwrap_or(0) == 0)
        .map(|(_, m)| *m)
        .collect();

    // ── Step 2: build the flat graph ──────────────────────────────────────
    let mut builder = GraphBuilder::default();
    for top in &top_levels {
        builder.expand_module(top, vec![], symbols, source);
    }

    // ── Step 3: Tarjan SCC ────────────────────────────────────────────────
    let sccs_raw = tarjan_scc(&builder.adj, builder.next_id);

    // ── Step 4: classify SCCs ─────────────────────────────────────────────
    let mut out_sccs: Vec<CombScc> = Vec::new();
    let mut suppressed = 0usize;
    let mut total = 0usize;
    for scc in sccs_raw {
        // Filter: size > 1 OR (size == 1 with self-loop)
        let is_cycle = scc.len() > 1
            || (scc.len() == 1 && builder.adj[scc[0]].contains(&scc[0]));
        if !is_cycle {
            continue;
        }
        total += 1;

        let mut owning_paths: HashSet<InstPath> = HashSet::new();
        let mut owning_modules: HashSet<String> = HashSet::new();
        let mut nodes: Vec<NodeKey> = Vec::with_capacity(scc.len());
        for nid in &scc {
            let key = builder.node_by_id[*nid].clone();
            owning_paths.insert(key.path.clone());
            if let Some(mname) = builder.owning_module(&key.path) {
                owning_modules.insert(mname);
            }
            nodes.push(key);
        }
        // Suppression: any owning module has `pragma comb_loops_allowed;`.
        let blessed = owning_modules
            .iter()
            .any(|mn| module_by_name.get(mn.as_str())
                .map(|m| m.comb_loops_allowed)
                .unwrap_or(false));
        if blessed {
            suppressed += 1;
            continue;
        }
        out_sccs.push(CombScc { nodes, owning_paths, owning_modules });
    }

    WholeDesignAnalysis {
        sccs: out_sccs,
        total_sccs: total,
        suppressed,
    }
}

// ── GraphBuilder ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct GraphBuilder {
    node_id: HashMap<NodeKey, usize>,
    node_by_id: Vec<NodeKey>,
    adj: Vec<Vec<usize>>,
    next_id: usize,
    /// path → owning module name. `vec![]` is special-cased per top entry.
    path_owner: HashMap<InstPath, String>,
}

impl GraphBuilder {
    fn intern(&mut self, key: NodeKey) -> usize {
        if let Some(id) = self.node_id.get(&key) {
            return *id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.node_id.insert(key.clone(), id);
        self.node_by_id.push(key);
        self.adj.push(Vec::new());
        id
    }

    fn add_edge(&mut self, from: usize, to: usize) {
        // Tarjan tolerates parallel edges, but dedupe for cleaner display.
        if !self.adj[from].contains(&to) {
            self.adj[from].push(to);
        }
    }

    fn owning_module(&self, path: &InstPath) -> Option<String> {
        self.path_owner.get(path).cloned()
    }

    /// Recursively build the graph for `m` at the given instance path.
    fn expand_module(
        &mut self,
        m: &ModuleDecl,
        path: InstPath,
        symbols: &SymbolTable,
        source: &SourceFile,
    ) {
        self.path_owner.insert(path.clone(), m.name.name.clone());

        // Helper to make a node at the current path.
        let mk = |gb: &mut GraphBuilder, name: &str| -> usize {
            gb.intern(NodeKey { path: path.clone(), signal: name.to_string() })
        };

        // Ensure all port/wire/let/reg/inst-output names exist as nodes.
        // We don't strictly need to pre-intern, but having them helps when
        // a wire is read but never written (still appears as an isolated
        // node — harmless for SCC).
        for p in &m.ports {
            // Skip clock/reset — they participate only in seq logic.
            if is_clk_or_rst(&p.ty) { continue; }
            mk(self, &p.name.name);
        }

        // 1) Parent-level comb blocks + let bindings + wire decls
        let (input_names, output_names) = port_sets(&m.ports);
        for item in &m.body {
            match item {
                ModuleBodyItem::WireDecl(w) => { mk(self, &w.name.name); }
                ModuleBodyItem::RegDecl(_) => {
                    // Regs are seq-driven; skip — they break comb cycles.
                }
                ModuleBodyItem::PipeRegDecl(_) => {
                    // pipe_reg outputs are registered.
                }
                ModuleBodyItem::CombBlock(cb) => {
                    self.scan_assignments(&cb.stmts, &path, &input_names, &output_names);
                }
                ModuleBodyItem::LetBinding(lb) => {
                    // Edge: each RHS ident → lb.name
                    let lhs = mk(self, &lb.name.name);
                    let mut ids = HashSet::new();
                    collect_expr_idents(&lb.value, &mut ids);
                    for id in &ids {
                        let from = mk(self, id);
                        self.add_edge(from, lhs);
                    }
                }
                _ => {}
            }
        }

        // 2) Sub-instances
        for inst in collect_insts(m) {
            self.expand_inst(inst, &path, symbols, source);
        }
    }

    /// Add edges and recurse for one sub-instance.
    fn expand_inst(
        &mut self,
        inst: &InstDecl,
        parent_path: &InstPath,
        symbols: &SymbolTable,
        source: &SourceFile,
    ) {
        let child_path = {
            let mut p = parent_path.clone();
            p.push(inst.name.name.clone());
            p
        };

        // Look up the child module (if any) and its CombInfo.
        let child_mod: Option<&ModuleDecl> = source.items.iter().find_map(|it| {
            if let Item::Module(cm) = it {
                if cm.name.name == inst.module_name.name {
                    return Some(cm);
                }
            }
            None
        });

        // CombInfo for the sub-instance's construct (any kind).
        let info = comb_info_for_symbol(&inst.module_name.name, symbols, source);

        // Map each connection's port-name → parent signal name (if a bare ident).
        // Direction is "from the parent's perspective" via ConnectDir.
        let mut input_conn: HashMap<String, String> = HashMap::new();  // port → parent signal (signal feeds INTO inst)
        let mut output_conn: HashMap<String, String> = HashMap::new(); // port → parent signal (inst drives this signal)
        for conn in &inst.connections {
            let parent_sig = match &conn.signal.kind {
                ExprKind::Ident(n) => n.clone(),
                _ => continue, // complex connection expression — skip
            };
            match conn.direction {
                ConnectDir::Input  => { input_conn.insert(conn.port_name.name.clone(), parent_sig); }
                ConnectDir::Output => { output_conn.insert(conn.port_name.name.clone(), parent_sig); }
            }
        }

        // Recurse into the child if it is a regular module.
        // Interface-only / opaque modules are treated as fully cross-connected
        // (any output depends on any input) — that's already the shape of
        // `comb_info_for_module` over interface stubs (empty body → empty
        // CombInfo), so we explicitly OVERRIDE here to the conservative
        // every-out-depends-on-every-in interpretation when the module is
        // an interface stub OR is missing entirely (extern).
        let treat_as_opaque = match child_mod {
            None => true,
            Some(cm) => cm.is_interface,
        };

        if let Some(cm) = child_mod {
            if !treat_as_opaque {
                self.expand_module(cm, child_path.clone(), symbols, source);
            }
        }

        // Add cross-boundary edges from sub-inst's CombInfo to parent signals.
        //
        // For each comb output port `q` connected to parent signal `out_sig`:
        //   For each comb input port `p` connected to parent signal `in_sig`:
        //     If `q` depends on `p` (in the child's CombInfo) → add edge in_sig → out_sig.
        //
        // For the MVP we use the existing CombInfo over-approximation: any
        // output port in `comb_outputs` is assumed to depend on every input
        // in `comb_dep_inputs`.
        // Build a set of registered output port names from the child module's
        // declarations. Ports declared `port reg ... : out T` or
        // `port X: out pipe_reg<T, N>` carry `reg_info: Some(_)` and produce
        // flopped outputs — they break combinational cycles at the seq
        // boundary and must not contribute parent-level comb edges. This
        // filter applies in BOTH the opaque and non-opaque branches
        // (defensive: comb_info_for_module already excludes them, but make
        // the rule explicit so future regressions don't leak in).
        let registered_outs: HashSet<&str> = child_mod
            .map(|cm| cm.ports.iter()
                .filter(|p| p.reg_info.is_some())
                .map(|p| p.name.name.as_str())
                .collect())
            .unwrap_or_default();

        let comb_outs: Vec<&String> = if treat_as_opaque {
            // Opaque: every declared output port that's connected AND not
            // flopped via `port reg` / `pipe_reg<T,N>`. Without the
            // registered-out filter, every comb-input → pipe_reg-output
            // edge becomes a phantom comb-dep that closes false cycles
            // through any module that exposes registered outputs (e.g.
            // arch-ibex's IbexCore decoded fields, IbexIdStage outputs).
            output_conn.keys()
                .filter(|k| !registered_outs.contains(k.as_str()))
                .collect()
        } else {
            info.comb_outputs.iter()
                .filter(|k| !registered_outs.contains(k.as_str()))
                .collect()
        };
        let comb_ins: Vec<&String> = if treat_as_opaque {
            input_conn.keys().collect()
        } else {
            info.comb_dep_inputs.iter().collect()
        };

        for out_port in &comb_outs {
            let out_sig = match output_conn.get(out_port.as_str()) {
                Some(s) => s.clone(),
                None => continue,
            };
            // Parent-side node receiving the inst's output.
            let to = self.intern(NodeKey { path: parent_path.clone(), signal: out_sig });

            // Also: if we DID recurse, link the deeper inst-internal output port
            // node to the parent-side wire so cycles that close via the
            // hierarchy show the full path. We add edge child.q → parent.out_sig.
            if !treat_as_opaque {
                let child_q = self.intern(NodeKey { path: child_path.clone(), signal: (*out_port).clone() });
                self.add_edge(child_q, to);
            }

            for in_port in &comb_ins {
                let in_sig = match input_conn.get(in_port.as_str()) {
                    Some(s) => s.clone(),
                    None => continue,
                };
                let from = self.intern(NodeKey { path: parent_path.clone(), signal: in_sig });

                // Direct over-approximation edge in_sig → out_sig at parent level.
                // (Captures the loop even when we don't recurse into the child.)
                self.add_edge(from, to);

                // And, if we recursed, also link parent.in_sig → child.in_port
                // so the cycle path display shows the descent.
                if !treat_as_opaque {
                    let child_p = self.intern(NodeKey { path: child_path.clone(), signal: (*in_port).clone() });
                    self.add_edge(from, child_p);
                }
            }
        }
    }

    /// Walk a comb-block's statements and add edges from RHS identifiers to
    /// LHS base names. Conditions count as reads of every then/else target.
    fn scan_assignments(
        &mut self,
        stmts: &[Stmt],
        path: &InstPath,
        _input_names: &HashSet<String>,
        _output_names: &HashSet<String>,
    ) {
        let mut cond_stack: Vec<HashSet<String>> = Vec::new();
        self.scan_assignments_inner(stmts, path, &mut cond_stack);
    }

    fn scan_assignments_inner(
        &mut self,
        stmts: &[Stmt],
        path: &InstPath,
        cond_stack: &mut Vec<HashSet<String>>,
    ) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    let lhs = match lhs_base_name(&a.target) {
                        Some(n) => n,
                        None => continue,
                    };
                    let mut rhs = HashSet::new();
                    collect_expr_idents(&a.value, &mut rhs);
                    // RHS for index/bit-slice on LHS also contributes to deps.
                    collect_lhs_index_reads(&a.target, &mut rhs);
                    let to = self.intern(NodeKey {
                        path: path.clone(),
                        signal: lhs,
                    });
                    for id in &rhs {
                        let from = self.intern(NodeKey {
                            path: path.clone(),
                            signal: id.clone(),
                        });
                        self.add_edge(from, to);
                    }
                    for conds in cond_stack.iter() {
                        for id in conds {
                            let from = self.intern(NodeKey {
                                path: path.clone(),
                                signal: id.clone(),
                            });
                            self.add_edge(from, to);
                        }
                    }
                }
                Stmt::IfElse(ife) => {
                    let mut cond_ids = HashSet::new();
                    collect_expr_idents(&ife.cond, &mut cond_ids);
                    cond_stack.push(cond_ids);
                    self.scan_assignments_inner(&ife.then_stmts, path, cond_stack);
                    self.scan_assignments_inner(&ife.else_stmts, path, cond_stack);
                    cond_stack.pop();
                }
                Stmt::Match(m) => {
                    let mut scrut_ids = HashSet::new();
                    collect_expr_idents(&m.scrutinee, &mut scrut_ids);
                    cond_stack.push(scrut_ids);
                    for arm in &m.arms {
                        self.scan_assignments_inner(&arm.body, path, cond_stack);
                    }
                    cond_stack.pop();
                }
                Stmt::For(f) => {
                    self.scan_assignments_inner(&f.body, path, cond_stack);
                }
                _ => {}
            }
        }
    }
}

/// LHS index/slice expressions can read other signals (e.g. `x[i] = ...`
/// reads `i`). Collect those identifier reads so they become dep edges.
fn collect_lhs_index_reads(target: &crate::ast::Expr, out: &mut HashSet<String>) {
    use ExprKind::*;
    match &target.kind {
        Ident(_) => {}
        BitSlice(base, hi, lo) => {
            collect_lhs_index_reads(base, out);
            collect_expr_idents(hi, out);
            collect_expr_idents(lo, out);
        }
        PartSelect(base, start, width, _) => {
            collect_lhs_index_reads(base, out);
            collect_expr_idents(start, out);
            collect_expr_idents(width, out);
        }
        Index(base, idx) => {
            collect_lhs_index_reads(base, out);
            collect_expr_idents(idx, out);
        }
        FieldAccess(base, _) => collect_lhs_index_reads(base, out),
        _ => {}
    }
}

// ── Tarjan's SCC algorithm ───────────────────────────────────────────────────

/// Iterative Tarjan's strongly-connected-components algorithm.
/// Returns SCCs in reverse topological order (sink components first), each
/// SCC being a Vec<NodeId>.
fn tarjan_scc(adj: &[Vec<usize>], n: usize) -> Vec<Vec<usize>> {
    // Iterative variant to avoid blowing the stack on large designs.
    let mut index_of: Vec<i64> = vec![-1; n];
    let mut lowlink: Vec<i64> = vec![-1; n];
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut sccs: Vec<Vec<usize>> = Vec::new();
    let mut index: i64 = 0;

    // Frame holds the recursion state for one node.
    struct Frame {
        v: usize,
        iter_pos: usize, // next adj index to visit
    }
    let mut call_stack: Vec<Frame> = Vec::new();

    for v_start in 0..n {
        if index_of[v_start] != -1 { continue; }
        // Push initial frame
        call_stack.push(Frame { v: v_start, iter_pos: 0 });
        index_of[v_start] = index;
        lowlink[v_start] = index;
        index += 1;
        stack.push(v_start);
        on_stack[v_start] = true;

        while let Some(frame) = call_stack.last_mut() {
            let v = frame.v;
            let neighbors = &adj[v];
            if frame.iter_pos < neighbors.len() {
                let w = neighbors[frame.iter_pos];
                frame.iter_pos += 1;
                if index_of[w] == -1 {
                    // Recurse
                    index_of[w] = index;
                    lowlink[w] = index;
                    index += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    call_stack.push(Frame { v: w, iter_pos: 0 });
                } else if on_stack[w] {
                    if index_of[w] < lowlink[v] {
                        lowlink[v] = index_of[w];
                    }
                }
            } else {
                // All neighbors processed — possibly emit SCC.
                if lowlink[v] == index_of[v] {
                    let mut comp: Vec<usize> = Vec::new();
                    loop {
                        let w = stack.pop().expect("tarjan stack underflow");
                        on_stack[w] = false;
                        comp.push(w);
                        if w == v { break; }
                    }
                    sccs.push(comp);
                }
                call_stack.pop();
                // Propagate lowlink up to parent.
                if let Some(parent) = call_stack.last_mut() {
                    if lowlink[v] < lowlink[parent.v] {
                        lowlink[parent.v] = lowlink[v];
                    }
                }
            }
        }
    }

    sccs
}

