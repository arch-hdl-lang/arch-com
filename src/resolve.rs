use std::collections::HashMap;

use crate::ast::*;
use crate::diagnostics::CompileError;
use crate::lexer::Span;

#[derive(Debug, Clone)]
pub enum Symbol {
    Domain(String),
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
    Param(String),
    Port(PortInfo),
    Reg(RegInfo),
    Let(String),
    Instance(InstanceInfo),
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
pub struct EnumInfo {
    pub name: String,
    pub variants: Vec<String>,
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
                    table.globals.insert(
                        d.name.name.clone(),
                        (Symbol::Domain(d.name.name.clone()), d.name.span),
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
                    let info = EnumInfo {
                        name: e.name.name.clone(),
                        variants: e.variants.iter().map(|v| v.name.clone()).collect(),
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
                        if scope.contains_key(&l.name.name) {
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
                                errors.push(CompileError::undefined(
                                    &i.module_name.name,
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
