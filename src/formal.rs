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

    // 2. Build encoder state
    let mut ctx = FormalCtx::new(module, symbols);
    ctx.preprocess()?;

    // 3. Emit SMT-LIB2 (header + declarations + transitions + comb)
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
}

#[derive(Debug, Clone)]
struct CombAssignFlat {
    target: String,          // flat name (e.g. "y" or "out[2]"); v1 supports ident targets only
    guard: Vec<Expr>,        // stack of conditions (ANDed)
    value: Expr,
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

        // Sub-instances: hierarchical encoding is PR-hf1b (tracked in
        // doc/plan_hierarchical_formal.md). For now, produce a more
        // actionable error listing the detected inst sites + the
        // workaround paths (target the sub-module in isolation via
        // --top, or use `arch build` + EBMC/SymbiYosys for hierarchy).
        let mut inst_names: Vec<String> = Vec::new();
        for b in &self.module.body {
            if let ModuleBodyItem::Inst(inst) = b {
                inst_names.push(format!("{} (module {})", inst.name.name, inst.module_name.name));
            }
        }
        if !inst_names.is_empty() {
            let first_inst = self.module.body.iter().find_map(|b| match b {
                ModuleBodyItem::Inst(i) => Some(i.span),
                _ => None,
            }).unwrap();
            return Err(CompileError::general(
                &format!(
                    "hierarchical `arch formal` is not yet implemented — module `{}` contains {} sub-instance(s): {}. Workarounds: (a) run `arch formal --top <sub_module>` to verify each sub-module in isolation; (b) use `arch build` + EBMC / SymbiYosys on the composed SV for whole-design BMC. Tracked in doc/plan_hierarchical_formal.md.",
                    self.module.name.name,
                    inst_names.len(),
                    inst_names.join(", ")
                ),
                first_inst,
            ));
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
            // SynthIdent is not yet handled by formal encoding — it points
            // at codegen-emitted SV state (credit_channel synthesized
            // wires) that `arch formal` has no SMT mirror for. Reject
            // clearly; the credit_channel formal story lands with the
            // Tier-2 SVA PR.
            SynthIdent(name, _) => {
                return Err(CompileError::general(
                    &format!(
                        "formal encoding of synthesized identifier `{name}` is not yet supported — credit_channel formal invariants land in a follow-up PR",
                    ),
                    expr.span,
                ));
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
