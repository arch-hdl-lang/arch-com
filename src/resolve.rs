use std::collections::HashMap;

use crate::ast::*;
use crate::diagnostics::CompileError;
use crate::lexer::Span;

#[derive(Debug, Clone)]
pub enum Symbol {
    Domain(DomainInfo),
    Struct(StructInfo),
    Enum(EnumInfo),
    Module(ModuleInfo),
    Fsm(FsmInfo),
    Fifo(FifoInfo),
    Ram(RamInfo),
    Counter(CounterInfo),
    Arbiter(ArbiterInfo),
    Regfile(RegfileInfo),
    Pipeline(PipelineInfo),
    Function(Vec<FunctionInfo>),
    Linklist(LinklistInfo),
    Template(String),
    Bus(BusInfo),
    Synchronizer(SynchronizerInfo),
    Clkgate(ClkGateInfo),
    Param(String),
    Port(PortInfo),
    Reg(RegInfo),
    Let(String),
    Instance(InstanceInfo),
}

#[derive(Debug, Clone)]
pub struct DomainInfo {
    pub name: String,
    pub freq_mhz: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SynchronizerInfo {
    pub name: String,
    pub stages: u64,
}

#[derive(Debug, Clone)]
pub struct ClkGateInfo {
    pub name: String,
    pub kind: crate::ast::ClkGateKind,
}

#[derive(Debug, Clone)]
pub struct CounterInfo {
    pub name: String,
    pub mode: crate::ast::CounterMode,
    pub direction: crate::ast::CounterDirection,
}

#[derive(Debug, Clone)]
pub struct ArbiterInfo {
    pub name: String,
    pub num_req: u64,
}

#[derive(Debug, Clone)]
pub struct RegfileInfo {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub arg_types: Vec<crate::ast::TypeExpr>,
    pub ret_ty: crate::ast::TypeExpr,
}

#[derive(Debug, Clone)]
pub struct PipelineInfo {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub stage_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LinklistInfo {
    pub name: String,
    pub kind: crate::ast::LinklistKind,
}

#[derive(Debug, Clone)]
pub struct RamInfo {
    pub name: String,
    pub kind: crate::ast::RamKind,
    pub latency: u32,
}

#[derive(Debug, Clone)]
pub struct FsmInfo {
    pub name: String,
    pub ports: Vec<PortDecl>,
    pub state_names: Vec<String>,
    pub default_state: String,
}

#[derive(Debug, Clone)]
pub struct FifoInfo {
    pub name: String,
    pub ports: Vec<PortDecl>,
    /// true when two Clock<> ports with different domains are declared
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub fields: Vec<(String, TypeExpr)>,
}

#[derive(Debug, Clone)]
pub struct BusInfo {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub signals: Vec<(String, Direction, TypeExpr)>,
    pub generates: Vec<BusGenerateIf>,
    /// Handshake channels declared in this bus (Tier 2 SVA emission).
    pub handshakes: Vec<crate::ast::HandshakeMeta>,
}

impl BusInfo {
    /// Build a param map from this bus's default param values.
    pub fn default_param_map(&self) -> HashMap<String, &Expr> {
        self.params.iter()
            .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
            .collect()
    }

    /// Return the effective signal list after evaluating generate_if blocks
    /// using the given param map (bus defaults + port-site overrides).
    pub fn effective_signals(&self, param_map: &HashMap<String, &Expr>) -> Vec<(String, Direction, TypeExpr)> {
        let mut result = self.signals.clone();
        for gen in &self.generates {
            let cond_val = eval_bus_cond(&gen.cond, param_map);
            let sigs = if cond_val { &gen.then_signals } else { &gen.else_signals };
            for s in sigs {
                result.push((s.name.name.clone(), s.direction, s.ty.clone()));
            }
        }
        result
    }
}

/// Evaluate a generate_if condition in a bus context.
/// Supports simple param references (truthy if nonzero) and literal integers.
fn eval_bus_cond(expr: &Expr, param_map: &HashMap<String, &Expr>) -> bool {
    // Truthy-if-nonzero rule for Ident / Literal expressions.
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(val_expr) = param_map.get(name.as_str()) {
                eval_bus_cond(val_expr, param_map)
            } else {
                false
            }
        }
        ExprKind::Literal(lit) => {
            match lit {
                LitKind::Dec(n) | LitKind::Hex(n) | LitKind::Bin(n) => *n != 0,
                LitKind::Sized(_, n) => *n != 0,
            }
        }
        ExprKind::Bool(b) => *b,
        // Binary comparison / logical ops — evaluate both operands as
        // integers (when possible) and apply the op. Supports the common
        // stdlib-bus patterns `ID_W > 0`, `MODE == 2`, `A && B`.
        ExprKind::Binary(op, l, r) => {
            use crate::ast::BinOp;
            match op {
                BinOp::And => eval_bus_cond(l, param_map) && eval_bus_cond(r, param_map),
                BinOp::Or  => eval_bus_cond(l, param_map) || eval_bus_cond(r, param_map),
                _ => match (eval_bus_int(l, param_map), eval_bus_int(r, param_map)) {
                    (Some(lv), Some(rv)) => match op {
                        BinOp::Eq  => lv == rv,
                        BinOp::Neq => lv != rv,
                        BinOp::Lt  => lv < rv,
                        BinOp::Gt  => lv > rv,
                        BinOp::Lte => lv <= rv,
                        BinOp::Gte => lv >= rv,
                        _ => true, // conservative
                    },
                    _ => true, // conservative
                }
            }
        }
        ExprKind::Unary(op, e) => {
            use crate::ast::UnaryOp;
            match op {
                UnaryOp::Not => !eval_bus_cond(e, param_map),
                _ => true,
            }
        }
        _ => true, // conservative: include signals if condition can't be evaluated
    }
}

/// Evaluate a bus-condition expression as an integer when possible.
/// Returns None if the expression can't be reduced (e.g. runtime signals).
fn eval_bus_int(expr: &Expr, param_map: &HashMap<String, &Expr>) -> Option<i64> {
    match &expr.kind {
        ExprKind::Literal(lit) => match lit {
            LitKind::Dec(n) | LitKind::Hex(n) | LitKind::Bin(n) | LitKind::Sized(_, n) => Some(*n as i64),
        },
        ExprKind::Ident(name) => {
            let val_expr = param_map.get(name.as_str())?;
            eval_bus_int(val_expr, param_map)
        }
        ExprKind::Bool(b) => Some(if *b { 1 } else { 0 }),
        ExprKind::Binary(op, l, r) => {
            use crate::ast::BinOp;
            let lv = eval_bus_int(l, param_map)?;
            let rv = eval_bus_int(r, param_map)?;
            Some(match op {
                BinOp::Add => lv + rv,
                BinOp::Sub => lv - rv,
                BinOp::Mul => lv * rv,
                BinOp::Div if rv != 0 => lv / rv,
                BinOp::Mod if rv != 0 => lv % rv,
                _ => return None,
            })
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub variants: Vec<String>,
    /// Explicit encoding values per variant (None = auto-sequential).
    pub values: Vec<Option<u64>>,
}

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
}

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: String,
    pub direction: Direction,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone)]
pub struct RegInfo {
    pub name: String,
    pub ty: TypeExpr,
    pub reset: RegReset,
}

#[derive(Debug, Clone)]
pub struct InstanceInfo {
    pub name: String,
    pub module_name: String,
}

#[derive(Debug)]
pub struct SymbolTable {
    pub globals: HashMap<String, (Symbol, Span)>,
    pub module_scopes: HashMap<String, HashMap<String, (Symbol, Span)>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            globals: HashMap::new(),
            module_scopes: HashMap::new(),
        }
    }
}

pub fn resolve(source_file: &SourceFile) -> Result<SymbolTable, Vec<CompileError>> {
    let mut table = SymbolTable::new();
    let mut errors = Vec::new();

    // Built-in domain: SysDomain is always available (can be overridden by user)
    table.globals.insert(
        "SysDomain".to_string(),
        (Symbol::Domain(DomainInfo { name: "SysDomain".to_string(), freq_mhz: None }), Span { start: 0, end: 0 }),
    );

    // First pass: register all global items
    for item in &source_file.items {
        match item {
            Item::Domain(d) => {
                // Allow duplicate domain definitions (common in multi-file projects)
                if let Some((Symbol::Domain(_), _)) = table.globals.get(&d.name.name) {
                    // Same domain re-declared — silently accept
                } else if table.globals.contains_key(&d.name.name) {
                    errors.push(CompileError::duplicate(&d.name.name, d.name.span));
                } else {
                    let freq_mhz = d.fields.iter()
                        .find(|f| f.name.name == "freq_mhz")
                        .and_then(|f| if let ExprKind::Literal(LitKind::Dec(v)) = &f.value.kind { Some(*v) } else { None });
                    table.globals.insert(
                        d.name.name.clone(),
                        (Symbol::Domain(DomainInfo { name: d.name.name.clone(), freq_mhz }), d.name.span),
                    );
                }
            }
            Item::Struct(s) => {
                if table.globals.contains_key(&s.name.name) {
                    errors.push(CompileError::duplicate(&s.name.name, s.name.span));
                } else {
                    let info = StructInfo {
                        name: s.name.name.clone(),
                        fields: s
                            .fields
                            .iter()
                            .map(|f| (f.name.name.clone(), f.ty.clone()))
                            .collect(),
                    };
                    table.globals.insert(
                        s.name.name.clone(),
                        (Symbol::Struct(info), s.name.span),
                    );
                }
            }
            Item::Enum(e) => {
                if table.globals.contains_key(&e.name.name) {
                    errors.push(CompileError::duplicate(&e.name.name, e.name.span));
                } else {
                    let values: Vec<Option<u64>> = e.values.iter().map(|v| {
                        v.as_ref().and_then(|expr| match &expr.kind {
                            crate::ast::ExprKind::Literal(crate::ast::LitKind::Dec(n)) => Some(*n),
                            crate::ast::ExprKind::Literal(crate::ast::LitKind::Hex(n)) => Some(*n),
                            crate::ast::ExprKind::Literal(crate::ast::LitKind::Bin(n)) => Some(*n),
                            crate::ast::ExprKind::Literal(crate::ast::LitKind::Sized(_, n)) => Some(*n),
                            _ => None,
                        })
                    }).collect();
                    let info = EnumInfo {
                        name: e.name.name.clone(),
                        variants: e.variants.iter().map(|v| v.name.clone()).collect(),
                        values,
                    };
                    table.globals.insert(
                        e.name.name.clone(),
                        (Symbol::Enum(info), e.name.span),
                    );
                }
            }
            Item::Module(m) => {
                if table.globals.contains_key(&m.name.name) {
                    errors.push(CompileError::duplicate(&m.name.name, m.name.span));
                } else {
                    let info = ModuleInfo {
                        name: m.name.name.clone(),
                        params: m.params.clone(),
                        ports: m.ports.clone(),
                    };
                    table.globals.insert(
                        m.name.name.clone(),
                        (Symbol::Module(info), m.name.span),
                    );
                }
            }
            Item::Fsm(f) => {
                if table.globals.contains_key(&f.name.name) {
                    errors.push(CompileError::duplicate(&f.name.name, f.name.span));
                } else {
                    // Validate default_state is declared
                    let declared: Vec<String> = f.state_names.iter().map(|s| s.name.clone()).collect();
                    if !declared.contains(&f.default_state.name) {
                        errors.push(CompileError::general(
                            &format!("default state `{}` not declared", f.default_state.name),
                            f.default_state.span,
                        ));
                    }
                    // Validate transition targets exist
                    for sb in &f.states {
                        for tr in &sb.transitions {
                            if !declared.contains(&tr.target.name) {
                                errors.push(CompileError::undefined(&tr.target.name, tr.target.span));
                            }
                        }
                    }
                    let info = FsmInfo {
                        name: f.name.name.clone(),
                        ports: f.ports.clone(),
                        state_names: declared,
                        default_state: f.default_state.name.clone(),
                    };
                    table.globals.insert(f.name.name.clone(), (Symbol::Fsm(info), f.name.span));
                }
            }
            Item::Fifo(f) => {
                if table.globals.contains_key(&f.name.name) {
                    errors.push(CompileError::duplicate(&f.name.name, f.name.span));
                } else {
                    let is_async = detect_async_fifo(&f.ports);
                    let info = FifoInfo {
                        name: f.name.name.clone(),
                        ports: f.ports.clone(),
                        is_async,
                    };
                    table.globals.insert(f.name.name.clone(), (Symbol::Fifo(info), f.name.span));
                }
            }
            Item::Ram(r) => {
                if table.globals.contains_key(&r.name.name) {
                    errors.push(CompileError::duplicate(&r.name.name, r.name.span));
                } else {
                    let info = RamInfo {
                        name: r.name.name.clone(),
                        kind: r.kind,
                        latency: r.latency,
                    };
                    table.globals.insert(r.name.name.clone(), (Symbol::Ram(info), r.name.span));
                }
            }
            Item::Counter(c) => {
                if table.globals.contains_key(&c.name.name) {
                    errors.push(CompileError::duplicate(&c.name.name, c.name.span));
                } else {
                    let info = CounterInfo {
                        name: c.name.name.clone(),
                        mode: c.mode,
                        direction: c.direction,
                    };
                    table.globals.insert(c.name.name.clone(), (Symbol::Counter(info), c.name.span));
                }
            }
            Item::Arbiter(a) => {
                if table.globals.contains_key(&a.name.name) {
                    errors.push(CompileError::duplicate(&a.name.name, a.name.span));
                } else {
                    // Try to find NUM_REQ param
                    let num_req = a.params.iter().find_map(|p| {
                        if p.name.name == "NUM_REQ" {
                            if let Some(Expr { kind: ExprKind::Literal(LitKind::Dec(n)), .. }) = &p.default {
                                return Some(*n);
                            }
                        }
                        None
                    }).unwrap_or(2);
                    let info = ArbiterInfo { name: a.name.name.clone(), num_req };
                    table.globals.insert(a.name.name.clone(), (Symbol::Arbiter(info), a.name.span));
                }
            }
            Item::Regfile(r) => {
                if table.globals.contains_key(&r.name.name) {
                    errors.push(CompileError::duplicate(&r.name.name, r.name.span));
                } else {
                    let info = RegfileInfo { name: r.name.name.clone() };
                    table.globals.insert(r.name.name.clone(), (Symbol::Regfile(info), r.name.span));
                }
            }
            Item::Pipeline(p) => {
                if table.globals.contains_key(&p.name.name) {
                    errors.push(CompileError::duplicate(&p.name.name, p.name.span));
                } else {
                    let info = PipelineInfo {
                        name: p.name.name.clone(),
                        params: p.params.clone(),
                        ports: p.ports.clone(),
                        stage_names: p.stages.iter().map(|s| s.name.name.clone()).collect(),
                    };
                    table.globals.insert(p.name.name.clone(), (Symbol::Pipeline(info), p.name.span));
                }
            }
            Item::Function(f) => {
                let info = FunctionInfo {
                    name: f.name.name.clone(),
                    arg_types: f.args.iter().map(|a| a.ty.clone()).collect(),
                    ret_ty: f.ret_ty.clone(),
                };
                if let Some((Symbol::Function(overloads), _)) = table.globals.get_mut(&f.name.name) {
                    overloads.push(info);
                } else if table.globals.contains_key(&f.name.name) {
                    errors.push(CompileError::duplicate(&f.name.name, f.name.span));
                } else {
                    table.globals.insert(f.name.name.clone(), (Symbol::Function(vec![info]), f.name.span));
                }
            }
            Item::Linklist(l) => {
                if table.globals.contains_key(&l.name.name) {
                    errors.push(CompileError::duplicate(&l.name.name, l.name.span));
                } else {
                    let info = LinklistInfo { name: l.name.name.clone(), kind: l.kind.clone() };
                    table.globals.insert(l.name.name.clone(), (Symbol::Linklist(info), l.name.span));
                }
            }
            Item::Template(t) => {
                if table.globals.contains_key(&t.name.name) {
                    errors.push(CompileError::duplicate(&t.name.name, t.name.span));
                } else {
                    table.globals.insert(t.name.name.clone(), (Symbol::Template(t.name.name.clone()), t.name.span));
                }
            }
            Item::Bus(b) => {
                if table.globals.contains_key(&b.name.name) {
                    errors.push(CompileError::duplicate(&b.name.name, b.name.span));
                } else {
                    let info = BusInfo {
                        name: b.name.name.clone(),
                        params: b.params.clone(),
                        signals: b.signals.iter()
                            .map(|s| (s.name.name.clone(), s.direction, s.ty.clone()))
                            .collect(),
                        generates: b.generates.clone(),
                        handshakes: b.handshakes.clone(),
                    };
                    table.globals.insert(b.name.name.clone(), (Symbol::Bus(info), b.name.span));
                }
            }
            Item::Package(pkg) => {
                if table.globals.contains_key(&pkg.name.name) {
                    errors.push(CompileError::duplicate(&pkg.name.name, pkg.name.span));
                } else {
                    // Register the package name itself (not strictly needed but consistent)
                    table.globals.insert(
                        pkg.name.name.clone(),
                        (Symbol::Template(pkg.name.name.clone()), pkg.name.span),
                    );
                    // Register contained items as globals
                    for d in &pkg.domains {
                        if let Some((Symbol::Domain(_), _)) = table.globals.get(&d.name.name) {
                            // Same domain re-declared — silently accept
                        } else if table.globals.contains_key(&d.name.name) {
                            errors.push(CompileError::duplicate(&d.name.name, d.name.span));
                        } else {
                            let freq_mhz = d.fields.iter()
                                .find(|f| f.name.name == "freq_mhz")
                                .and_then(|f| if let ExprKind::Literal(LitKind::Dec(v)) = &f.value.kind { Some(*v) } else { None });
                            table.globals.insert(
                                d.name.name.clone(),
                                (Symbol::Domain(DomainInfo { name: d.name.name.clone(), freq_mhz }), d.name.span),
                            );
                        }
                    }
                    for e in &pkg.enums {
                        if table.globals.contains_key(&e.name.name) {
                            errors.push(CompileError::duplicate(&e.name.name, e.name.span));
                        } else {
                            let values: Vec<Option<u64>> = e.values.iter().map(|v| {
                                v.as_ref().and_then(|expr| match &expr.kind {
                                    crate::ast::ExprKind::Literal(crate::ast::LitKind::Dec(n)) => Some(*n),
                                    crate::ast::ExprKind::Literal(crate::ast::LitKind::Hex(n)) => Some(*n),
                                    crate::ast::ExprKind::Literal(crate::ast::LitKind::Bin(n)) => Some(*n),
                                    crate::ast::ExprKind::Literal(crate::ast::LitKind::Sized(_, n)) => Some(*n),
                                    _ => None,
                                })
                            }).collect();
                            let info = EnumInfo {
                                name: e.name.name.clone(),
                                variants: e.variants.iter().map(|v| v.name.clone()).collect(),
                                values,
                            };
                            table.globals.insert(e.name.name.clone(), (Symbol::Enum(info), e.name.span));
                        }
                    }
                    for s in &pkg.structs {
                        if table.globals.contains_key(&s.name.name) {
                            errors.push(CompileError::duplicate(&s.name.name, s.name.span));
                        } else {
                            let info = StructInfo {
                                name: s.name.name.clone(),
                                fields: s.fields.iter().map(|f| (f.name.name.clone(), f.ty.clone())).collect(),
                            };
                            table.globals.insert(s.name.name.clone(), (Symbol::Struct(info), s.name.span));
                        }
                    }
                    for b in &pkg.buses {
                        if table.globals.contains_key(&b.name.name) {
                            errors.push(CompileError::duplicate(&b.name.name, b.name.span));
                        } else {
                            let info = BusInfo {
                                name: b.name.name.clone(),
                                params: b.params.clone(),
                                signals: b.signals.iter()
                                    .map(|s| (s.name.name.clone(), s.direction, s.ty.clone()))
                                    .collect(),
                                generates: b.generates.clone(),
                                handshakes: b.handshakes.clone(),
                            };
                            table.globals.insert(b.name.name.clone(), (Symbol::Bus(info), b.name.span));
                        }
                    }
                    for f in &pkg.functions {
                        let info = FunctionInfo {
                            name: f.name.name.clone(),
                            arg_types: f.args.iter().map(|a| a.ty.clone()).collect(),
                            ret_ty: f.ret_ty.clone(),
                        };
                        if let Some((Symbol::Function(overloads), _)) = table.globals.get_mut(&f.name.name) {
                            overloads.push(info);
                        } else if table.globals.contains_key(&f.name.name) {
                            errors.push(CompileError::duplicate(&f.name.name, f.name.span));
                        } else {
                            table.globals.insert(f.name.name.clone(), (Symbol::Function(vec![info]), f.name.span));
                        }
                    }
                    for p in &pkg.params {
                        table.globals.insert(
                            p.name.name.clone(),
                            (Symbol::Param(p.name.name.clone()), p.name.span),
                        );
                    }
                }
            }
            Item::Use(_) => {} // file already loaded; no-op
            Item::Synchronizer(s) => {
                if table.globals.contains_key(&s.name.name) {
                    errors.push(CompileError::duplicate(&s.name.name, s.name.span));
                } else {
                    let stages = s.params.iter()
                        .find(|p| p.name.name == "STAGES")
                        .and_then(|p| p.default.as_ref())
                        .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
                        .unwrap_or(2);
                    table.globals.insert(s.name.name.clone(), (Symbol::Synchronizer(SynchronizerInfo {
                        name: s.name.name.clone(),
                        stages,
                    }), s.name.span));
                }
            }
            Item::Clkgate(c) => {
                if table.globals.contains_key(&c.name.name) {
                    errors.push(CompileError::duplicate(&c.name.name, c.name.span));
                } else {
                    table.globals.insert(c.name.name.clone(), (Symbol::Clkgate(ClkGateInfo {
                        name: c.name.name.clone(),
                        kind: c.kind.clone(),
                    }), c.name.span));
                }
            }
        }
    }

    // Second pass: resolve module-level symbols
    for item in &source_file.items {
        if let Item::Module(m) = item {
            let mut scope = HashMap::new();

            for p in &m.params {
                scope.insert(
                    p.name.name.clone(),
                    (Symbol::Param(p.name.name.clone()), p.name.span),
                );
            }

            for p in &m.ports {
                scope.insert(
                    p.name.name.clone(),
                    (
                        Symbol::Port(PortInfo {
                            name: p.name.name.clone(),
                            direction: p.direction,
                            ty: p.ty.clone(),
                        }),
                        p.name.span,
                    ),
                );
            }

            for body_item in &m.body {
                match body_item {
                    ModuleBodyItem::RegDecl(r) => {
                        if scope.contains_key(&r.name.name) {
                            errors.push(CompileError::duplicate(&r.name.name, r.name.span));
                        } else {
                            scope.insert(
                                r.name.name.clone(),
                                (
                                    Symbol::Reg(RegInfo {
                                        name: r.name.name.clone(),
                                        ty: r.ty.clone(),
                                        reset: r.reset.clone(),
                                    }),
                                    r.name.span,
                                ),
                            );
                        }
                    }
                    ModuleBodyItem::LetBinding(l) => {
                        if l.ty.is_none() {
                            // ty=None: assignment to existing port or wire — not a new binding,
                            // so don't check for or insert into scope.
                        } else if scope.contains_key(&l.name.name) {
                            errors.push(CompileError::duplicate(&l.name.name, l.name.span));
                        } else {
                            scope.insert(
                                l.name.name.clone(),
                                (Symbol::Let(l.name.name.clone()), l.name.span),
                            );
                        }
                    }
                    ModuleBodyItem::Inst(i) => {
                        if scope.contains_key(&i.name.name) {
                            errors.push(CompileError::duplicate(&i.name.name, i.name.span));
                        } else {
                            // Verify the instantiated module exists
                            if !table.globals.contains_key(&i.module_name.name) {
                                let mname = &i.module_name.name;
                                errors.push(CompileError::undefined_module(
                                    mname,
                                    &format!(
                                        "build the sub-module first: `arch build {mname}.arch` \
                                         (generates {mname}.archi), then re-compile this module"
                                    ),
                                    i.module_name.span,
                                ));
                            }
                            scope.insert(
                                i.name.name.clone(),
                                (
                                    Symbol::Instance(InstanceInfo {
                                        name: i.name.name.clone(),
                                        module_name: i.module_name.name.clone(),
                                    }),
                                    i.name.span,
                                ),
                            );
                        }
                    }
                    ModuleBodyItem::Function(f) => {
                        // Register module-local functions in globals so they're
                        // callable from expressions within this module.
                        let info = FunctionInfo {
                            name: f.name.name.clone(),
                            arg_types: f.args.iter().map(|a| a.ty.clone()).collect(),
                            ret_ty: f.ret_ty.clone(),
                        };
                        if let Some((Symbol::Function(overloads), _)) = table.globals.get_mut(&f.name.name) {
                            overloads.push(info);
                        } else if !table.globals.contains_key(&f.name.name) {
                            table.globals.insert(f.name.name.clone(), (Symbol::Function(vec![info]), f.name.span));
                        }
                    }
                    _ => {}
                }
            }

            table.module_scopes.insert(m.name.name.clone(), scope);
        }

        // Validate inst references inside pipeline stages
        if let Item::Pipeline(p) = item {
            for stage in &p.stages {
                for body_item in &stage.body {
                    if let ModuleBodyItem::Inst(i) = body_item {
                        if !table.globals.contains_key(&i.module_name.name) {
                            errors.push(CompileError::undefined(
                                &i.module_name.name,
                                i.module_name.span,
                            ));
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(table)
    } else {
        Err(errors)
    }
}

/// A FIFO is async when it has two Clock<> ports with different domain names.
pub fn detect_async_fifo(ports: &[PortDecl]) -> bool {
    let clock_domains: Vec<&str> = ports
        .iter()
        .filter_map(|p| {
            if let TypeExpr::Clock(domain) = &p.ty {
                Some(domain.name.as_str())
            } else {
                None
            }
        })
        .collect();
    clock_domains.len() >= 2 && clock_domains[0] != clock_domains[1]
}
