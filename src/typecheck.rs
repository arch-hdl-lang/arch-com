use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostics::{CompileError, CompileWarning};
use crate::lexer::Span;
use crate::resolve::{Symbol, SymbolTable};

/// Resolved type information
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    UInt(u32),
    SInt(u32),
    Bool,
    Bit,
    Clock(String), // domain name
    Reset(ResetKind),
    Vec(Box<Ty>, u32),
    Struct(String),
    Enum(String, u32), // name, bit width
    Todo,
    Error,
}

impl Ty {
    pub fn width(&self) -> Option<u32> {
        match self {
            Ty::UInt(w) | Ty::SInt(w) => Some(*w),
            Ty::Bool => Some(1),
            Ty::Bit => Some(1),
            Ty::Enum(_, w) => Some(*w),
            Ty::Vec(inner, count) => inner.width().map(|w| w * count),
            Ty::Struct(_) => None, // would need lookup
            Ty::Clock(_) | Ty::Reset(_) => Some(1),
            Ty::Todo | Ty::Error => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Ty::UInt(w) => format!("UInt<{w}>"),
            Ty::SInt(w) => format!("SInt<{w}>"),
            Ty::Bool => "Bool".to_string(),
            Ty::Bit => "Bit".to_string(),
            Ty::Clock(d) => format!("Clock<{d}>"),
            Ty::Reset(k) => format!("Reset<{}>", match k {
                ResetKind::Sync => "Sync",
                ResetKind::Async => "Async",
            }),
            Ty::Vec(inner, n) => format!("Vec<{}, {n}>", inner.display()),
            Ty::Struct(name) => name.clone(),
            Ty::Enum(name, _) => name.clone(),
            Ty::Todo => "todo!".to_string(),
            Ty::Error => "<error>".to_string(),
        }
    }
}

pub struct TypeChecker<'a> {
    pub symbols: &'a SymbolTable,
    pub source: &'a SourceFile,
    pub errors: Vec<CompileError>,
    pub warnings: Vec<CompileWarning>,
    /// Maps call-site span.start → overload index within Symbol::Function vec.
    /// Only populated for calls to overloaded functions (vec.len() > 1).
    pub overload_map: HashMap<usize, usize>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a SymbolTable, source: &'a SourceFile) -> Self {
        Self {
            symbols,
            source,
            errors: Vec::new(),
            warnings: Vec::new(),
            overload_map: HashMap::new(),
        }
    }

    pub fn check(mut self) -> Result<(Vec<CompileWarning>, HashMap<usize, usize>), Vec<CompileError>> {
        for item in &self.source.items {
            match item {
                Item::Domain(d) => self.check_domain(d),
                Item::Struct(s) => self.check_struct(s),
                Item::Enum(e) => self.check_enum(e),
                Item::Module(m) => self.check_module(m),
                Item::Fsm(f) => self.check_fsm(f),
                Item::Fifo(f) => self.check_fifo(f),
                Item::Ram(r) => self.check_ram(r),
                Item::Counter(c) => self.check_counter(c),
                Item::Arbiter(a) => self.check_arbiter(a),
                Item::Regfile(r) => self.check_regfile(r),
                Item::Pipeline(p) => self.check_pipeline(p),
                Item::Function(f) => self.check_function(f),
                Item::Linklist(l) => self.check_linklist(l),
            }
        }
        if self.errors.is_empty() {
            Ok((self.warnings, self.overload_map))
        } else {
            Err(self.errors)
        }
    }

    fn check_domain(&mut self, d: &DomainDecl) {
        self.check_pascal_case(&d.name);
    }

    fn check_struct(&mut self, s: &StructDecl) {
        self.check_pascal_case(&s.name);
        for field in &s.fields {
            self.check_snake_case(&field.name);
        }
    }

    fn check_enum(&mut self, e: &EnumDecl) {
        self.check_pascal_case(&e.name);
        for variant in &e.variants {
            self.check_pascal_case(variant);
        }
    }

    fn check_module(&mut self, m: &ModuleDecl) {
        self.check_pascal_case(&m.name);

        // Track driven signals
        let mut driven: HashSet<String> = HashSet::new();

        // Check params
        for p in &m.params {
            self.check_upper_snake(&p.name);
        }

        // Check ports
        for p in &m.ports {
            self.check_snake_case(&p.name);
        }

        // Build local type environment
        let mut local_types: HashMap<String, Ty> = HashMap::new();
        for p in &m.params {
            if let Some(default) = &p.default {
                let ty = self.resolve_expr_type(default, &m.name.name, &local_types);
                local_types.insert(p.name.name.clone(), ty);
            }
        }
        for p in &m.ports {
            let ty = self.resolve_type_expr(&p.ty, &m.name.name, &local_types);
            local_types.insert(p.name.name.clone(), ty);
        }

        // Check body items
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    self.check_snake_case(&r.name);
                    let ty = self.resolve_type_expr(&r.ty, &m.name.name, &local_types);
                    local_types.insert(r.name.name.clone(), ty);
                }
                ModuleBodyItem::RegBlock(rb) => {
                    // Check stmts
                    for stmt in &rb.stmts {
                        self.check_reg_stmt(stmt, &m.name.name, &local_types, &mut driven);
                    }
                    // Validate reset consistency: all registers with reset in the
                    // same always block must agree on signal name, sync/async, and polarity.
                    self.check_always_block_reset_consistency(rb, m);
                }
                ModuleBodyItem::CombBlock(cb) => {
                    for stmt in &cb.stmts {
                        self.check_comb_stmt(stmt, &m.name.name, &local_types, &mut driven);
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    self.check_snake_case(&l.name);
                    if l.ty.is_none() {
                        self.errors.push(CompileError::general(
                            &format!(
                                "let binding '{}' requires an explicit type annotation: let {}: Type = ...",
                                l.name.name, l.name.name
                            ),
                            l.span,
                        ));
                    }
                    let ty = self.resolve_expr_type(&l.value, &m.name.name, &local_types);
                    if let Some(declared_ty) = &l.ty {
                        let expected = self.resolve_type_expr(declared_ty, &m.name.name, &local_types);
                        if expected != Ty::Error && ty != Ty::Error && ty != Ty::Todo && expected != ty
                            && !types_compatible(&expected, &ty)
                        {
                            self.errors.push(CompileError::type_mismatch(
                                &expected.display(),
                                &ty.display(),
                                l.value.span,
                            ));
                        }
                    }
                    // Use the declared type if provided (it may be wider than what was inferred)
                    let final_ty = if let Some(declared_ty) = &l.ty {
                        self.resolve_type_expr(declared_ty, &m.name.name, &local_types)
                    } else {
                        ty
                    };
                    local_types.insert(l.name.name.clone(), final_ty);
                    driven.insert(l.name.name.clone());
                }
                ModuleBodyItem::PipeRegDecl(p) => {
                    self.check_snake_case(&p.name);
                    if p.stages == 0 {
                        self.errors.push(CompileError::general(
                            &format!("pipe_reg '{}': stages must be > 0", p.name.name),
                            p.span,
                        ));
                    }
                    if !local_types.contains_key(&p.source.name) {
                        self.errors.push(CompileError::general(
                            &format!("pipe_reg '{}': source signal '{}' not found", p.name.name, p.source.name),
                            p.source.span,
                        ));
                    }
                    if local_types.contains_key(&p.name.name) {
                        self.errors.push(CompileError::general(
                            &format!("pipe_reg '{}': name already declared", p.name.name),
                            p.name.span,
                        ));
                    }
                    let ty = local_types.get(&p.source.name).cloned().unwrap_or(Ty::Error);
                    local_types.insert(p.name.name.clone(), ty);
                    driven.insert(p.name.name.clone());
                }
                ModuleBodyItem::Inst(inst) => {
                    self.check_snake_case(&inst.name);
                    // Mark connected output ports as driven
                    for conn in &inst.connections {
                        if conn.direction == ConnectDir::Output {
                            if let ExprKind::Ident(name) = &conn.signal.kind {
                                driven.insert(name.clone());
                            }
                        }
                    }
                }
                // Generate blocks are fully expanded by the elaboration pass before
                // type-checking runs; this arm should never be reached.
                ModuleBodyItem::Generate(_) => {}
            }
        }

        // Check all output ports are driven
        for p in &m.ports {
            if p.direction == Direction::Out && !driven.contains(&p.name.name) {
                self.errors.push(CompileError::UndriveOutput {
                    name: p.name.name.clone(),
                    span: crate::diagnostics::span_to_source_span(p.name.span),
                });
            }
        }
    }

    /// Validate that all registers with reset assigned in an `always on` block
    /// agree on reset signal name, sync/async kind, and polarity.
    fn check_always_block_reset_consistency(&mut self, rb: &RegBlock, m: &ModuleDecl) {
        // Collect assigned register root names
        let mut assigned = std::collections::BTreeSet::new();
        Self::collect_assigned_roots_tc(&rb.stmts, &mut assigned);

        // Gather reg declarations for assigned registers
        let reg_decls: Vec<&RegDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i { Some(r) } else { None })
            .collect();

        // Resolved reset info: (signal_name, kind, level)
        struct ResetProps {
            signal: String,
            kind: ResetKind,
            level: ResetLevel,
        }

        let mut first_reset: Option<ResetProps> = None;

        for name in &assigned {
            if name.is_empty() { continue; }
            let rd = match reg_decls.iter().find(|r| r.name.name == *name) {
                Some(rd) => rd,
                None => continue,
            };

            let (signal, kind, level) = match &rd.reset {
                RegReset::None => continue,
                RegReset::Explicit(sig, k, l) => (sig.name.clone(), *k, *l),
                RegReset::Inherit(sig) => {
                    // Look up port to resolve kind and level
                    if let Some(port) = m.ports.iter().find(|p| p.name.name == sig.name) {
                        if let TypeExpr::Reset(k, l) = &port.ty {
                            (sig.name.clone(), *k, *l)
                        } else {
                            self.errors.push(CompileError::general(
                                &format!("`{}` reset signal `{}` is not a Reset port", name, sig.name),
                                sig.span,
                            ));
                            continue;
                        }
                    } else {
                        self.errors.push(CompileError::general(
                            &format!("`{}` reset signal `{}` not found in module ports", name, sig.name),
                            sig.span,
                        ));
                        continue;
                    }
                }
            };

            if let Some(ref first) = first_reset {
                if signal != first.signal {
                    self.errors.push(CompileError::general(
                        &format!(
                            "register `{}` uses reset signal `{}` but other registers in the same always block use `{}`",
                            name, signal, first.signal
                        ),
                        rd.span,
                    ));
                }
                if kind != first.kind {
                    self.errors.push(CompileError::general(
                        &format!(
                            "register `{}` uses {} reset but other registers in the same always block use {}",
                            name,
                            if kind == ResetKind::Async { "async" } else { "sync" },
                            if first.kind == ResetKind::Async { "async" } else { "sync" },
                        ),
                        rd.span,
                    ));
                }
                if level != first.level {
                    self.errors.push(CompileError::general(
                        &format!(
                            "register `{}` uses active-{} reset but other registers in the same always block use active-{}",
                            name,
                            if level == ResetLevel::Low { "low" } else { "high" },
                            if first.level == ResetLevel::Low { "low" } else { "high" },
                        ),
                        rd.span,
                    ));
                }
            } else {
                first_reset = Some(ResetProps { signal, kind, level });
            }
        }
    }

    /// Collect root signal names from LHS assignments (typecheck version, no codegen dependency).
    fn collect_assigned_roots_tc(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    out.insert(Self::expr_root_name_tc(&a.target));
                }
                Stmt::IfElse(ie) => {
                    Self::collect_assigned_roots_tc(&ie.then_stmts, out);
                    Self::collect_assigned_roots_tc(&ie.else_stmts, out);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        Self::collect_assigned_roots_tc(&arm.body, out);
                    }
                }
                Stmt::Log(_) => {}
            }
        }
    }

    fn expr_root_name_tc(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::FieldAccess(base, _) => Self::expr_root_name_tc(base),
            ExprKind::Index(base, _) => Self::expr_root_name_tc(base),
            _ => String::new(),
        }
    }

    /// Emit an error when the RHS is wider than the LHS register/port.
    fn check_width_compatible(&mut self, lhs_ty: &Ty, rhs_ty: &Ty, name: &str, span: Span) {
        match (lhs_ty, rhs_ty) {
            (Ty::UInt(lw), Ty::UInt(rw)) if rw > lw => {
                let hint = if *rw == lw + 1 { " (arithmetic widening)" } else { "" };
                self.errors.push(CompileError::general(
                    &format!(
                        "width mismatch: `{name}` is UInt<{lw}> but RHS is UInt<{rw}>{hint}; \
                         use `.trunc<{lw}>()` to truncate explicitly"
                    ),
                    span,
                ));
            }
            (Ty::SInt(lw), Ty::SInt(rw)) if rw > lw => {
                let hint = if *rw == lw + 1 { " (arithmetic widening)" } else { "" };
                self.errors.push(CompileError::general(
                    &format!(
                        "width mismatch: `{name}` is SInt<{lw}> but RHS is SInt<{rw}>{hint}; \
                         use `.trunc<{lw}>()` to truncate explicitly"
                    ),
                    span,
                ));
            }
            _ => {}
        }
    }

    /// Emit an error when an enum match is not exhaustive (no wildcard and missing variants).
    fn check_match_exhaustive(&mut self, scrutinee: &Expr, patterns: &[Pattern], span: Span,
                              module_name: &str, local_types: &HashMap<String, Ty>) {
        let scrutinee_ty = self.resolve_expr_type(scrutinee, module_name, local_types);
        let enum_name = match &scrutinee_ty {
            Ty::Enum(name, _) => name.clone(),
            _ => return, // only check enum matches
        };
        if patterns.iter().any(|p| matches!(p, Pattern::Wildcard)) {
            return; // wildcard covers everything
        }
        let covered: HashSet<String> = patterns.iter().filter_map(|p| {
            if let Pattern::EnumVariant(_, variant) = p { Some(variant.name.clone()) } else { None }
        }).collect();
        if let Some((Symbol::Enum(info), _)) = self.symbols.globals.get(&enum_name).cloned() {
            let missing: Vec<String> = info.variants.iter()
                .filter(|v| !covered.contains(*v))
                .map(|v| format!("`{enum_name}::{v}`"))
                .collect();
            if !missing.is_empty() {
                self.errors.push(CompileError::general(
                    &format!(
                        "non-exhaustive match on `{enum_name}`: missing {}; \
                         add arms or a wildcard `_`",
                        missing.join(", ")
                    ),
                    span,
                ));
            }
        }
    }

    fn check_reg_stmt(
        &mut self,
        stmt: &Stmt,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
        driven: &mut HashSet<String>,
    ) {
        match stmt {
            Stmt::Assign(a) => {
                if let ExprKind::Ident(name) = &a.target.kind {
                    driven.insert(name.clone());
                    let rhs_ty = self.resolve_expr_type(&a.value, module_name, local_types);
                    if let Some(lhs_ty) = local_types.get(name).cloned() {
                        self.check_width_compatible(&lhs_ty, &rhs_ty, name, a.span);
                    }
                }
            }
            Stmt::IfElse(ie) => {
                let _cond_ty = self.resolve_expr_type(&ie.cond, module_name, local_types);
                for s in &ie.then_stmts {
                    self.check_reg_stmt(s, module_name, local_types, driven);
                }
                for s in &ie.else_stmts {
                    self.check_reg_stmt(s, module_name, local_types, driven);
                }
            }
            Stmt::Match(m) => {
                let patterns: Vec<Pattern> = m.arms.iter().map(|a| a.pattern.clone()).collect();
                self.check_match_exhaustive(&m.scrutinee, &patterns, m.span, module_name, local_types);
                for arm in &m.arms {
                    for s in &arm.body {
                        self.check_reg_stmt(s, module_name, local_types, driven);
                    }
                }
            }
            Stmt::Log(l) => {
                for arg in &l.args {
                    self.resolve_expr_type(arg, module_name, local_types);
                }
            }
        }
    }

    fn check_comb_stmt(
        &mut self,
        stmt: &CombStmt,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
        driven: &mut HashSet<String>,
    ) {
        match stmt {
            CombStmt::Assign(a) => {
                if driven.contains(&a.target.name) {
                    self.errors.push(CompileError::MultipleDrivers {
                        name: a.target.name.clone(),
                        span: crate::diagnostics::span_to_source_span(a.target.span),
                    });
                }
                driven.insert(a.target.name.clone());
                let rhs_ty = self.resolve_expr_type(&a.value, module_name, local_types);
                if let Some(lhs_ty) = local_types.get(&a.target.name).cloned() {
                    self.check_width_compatible(&lhs_ty, &rhs_ty, &a.target.name, a.span);
                }
            }
            CombStmt::IfElse(ie) => {
                let _cond_ty = self.resolve_expr_type(&ie.cond, module_name, local_types);
                // Each branch gets its own copy of driven — signals assigned
                // in mutually exclusive branches are not multiple drivers.
                let mut then_driven = driven.clone();
                for s in &ie.then_stmts {
                    self.check_comb_stmt(s, module_name, local_types, &mut then_driven);
                }
                let mut else_driven = driven.clone();
                for s in &ie.else_stmts {
                    self.check_comb_stmt(s, module_name, local_types, &mut else_driven);
                }
                // Merge both branches back — a signal driven in either branch
                // counts as driven for subsequent statements.
                for name in then_driven.iter().chain(else_driven.iter()) {
                    driven.insert(name.clone());
                }
            }
            CombStmt::MatchExpr(m) => {
                let patterns: Vec<Pattern> = m.arms.iter().map(|a| a.pattern.clone()).collect();
                self.check_match_exhaustive(&m.scrutinee, &patterns, m.span, module_name, local_types);
                for arm in &m.arms {
                    for s in &arm.body {
                        self.check_reg_stmt(s, module_name, local_types, driven);
                    }
                }
            }
            CombStmt::Log(l) => {
                for arg in &l.args {
                    self.resolve_expr_type(arg, module_name, local_types);
                }
            }
        }
    }

    fn resolve_type_expr(
        &mut self,
        ty: &TypeExpr,
        _module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) -> Ty {
        match ty {
            TypeExpr::UInt(width_expr) => {
                if let Some(w) = self.eval_const_expr(width_expr, local_types) {
                    Ty::UInt(w as u32)
                } else {
                    Ty::Error
                }
            }
            TypeExpr::SInt(width_expr) => {
                if let Some(w) = self.eval_const_expr(width_expr, local_types) {
                    Ty::SInt(w as u32)
                } else {
                    Ty::Error
                }
            }
            TypeExpr::Bool => Ty::Bool,
            TypeExpr::Bit => Ty::Bit,
            TypeExpr::Clock(domain) => Ty::Clock(domain.name.clone()),
            TypeExpr::Reset(kind, _level) => Ty::Reset(*kind),
            TypeExpr::Vec(inner, size_expr) => {
                let inner_ty = self.resolve_type_expr(inner, _module_name, local_types);
                if let Some(n) = self.eval_const_expr(size_expr, local_types) {
                    Ty::Vec(Box::new(inner_ty), n as u32)
                } else {
                    Ty::Error
                }
            }
            TypeExpr::Named(ident) => {
                if let Some((sym, _)) = self.symbols.globals.get(&ident.name) {
                    match sym {
                        crate::resolve::Symbol::Struct(_) => Ty::Struct(ident.name.clone()),
                        crate::resolve::Symbol::Enum(info) => {
                            let bits = enum_width(info.variants.len());
                            Ty::Enum(ident.name.clone(), bits)
                        }
                        _ => {
                            self.errors.push(CompileError::type_mismatch(
                                "type",
                                &ident.name,
                                ident.span,
                            ));
                            Ty::Error
                        }
                    }
                } else {
                    self.errors.push(CompileError::undefined(&ident.name, ident.span));
                    Ty::Error
                }
            }
        }
    }

    fn resolve_expr_type(
        &mut self,
        expr: &Expr,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) -> Ty {
        match &expr.kind {
            ExprKind::Todo => {
                self.warnings.push(CompileWarning {
                    message: "todo! placeholder will abort at runtime".to_string(),
                    span: expr.span,
                });
                Ty::Todo
            }
            ExprKind::Literal(lit) => match lit {
                LitKind::Dec(v) => {
                    let bits = if *v == 0 { 1 } else { 64 - v.leading_zeros() };
                    Ty::UInt(bits)
                }
                LitKind::Hex(v) => {
                    let bits = if *v == 0 { 1 } else { 64 - v.leading_zeros() };
                    Ty::UInt(bits)
                }
                LitKind::Bin(v) => {
                    let bits = if *v == 0 { 1 } else { 64 - v.leading_zeros() };
                    Ty::UInt(bits)
                }
                LitKind::Sized(w, _) => Ty::UInt(*w),
            },
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Ident(name) => {
                if let Some(ty) = local_types.get(name) {
                    ty.clone()
                } else {
                    // Check param (treat as generic width)
                    Ty::Error
                }
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let lt = self.resolve_expr_type(lhs, module_name, local_types);
                let rt = self.resolve_expr_type(rhs, module_name, local_types);
                self.binop_result_type(*op, &lt, &rt, expr.span)
            }
            ExprKind::Unary(op, operand) => {
                let t = self.resolve_expr_type(operand, module_name, local_types);
                match op {
                    UnaryOp::Not => Ty::Bool,
                    UnaryOp::BitNot => t,
                    UnaryOp::Neg => {
                        if let Ty::UInt(w) = t {
                            Ty::SInt(w + 1)
                        } else {
                            t
                        }
                    }
                }
            }
            ExprKind::FieldAccess(base, field) => {
                let base_ty = self.resolve_expr_type(base, module_name, local_types);
                if let Ty::Struct(name) = &base_ty {
                    if let Some((sym, _)) = self.symbols.globals.get(name) {
                        if let crate::resolve::Symbol::Struct(info) = sym {
                            for (fname, fty) in &info.fields {
                                if fname == &field.name {
                                    return self.resolve_type_expr(fty, module_name, local_types);
                                }
                            }
                        }
                    }
                }
                Ty::Error
            }
            ExprKind::MethodCall(base, method, args) => {
                let base_ty = self.resolve_expr_type(base, module_name, local_types);
                match method.name.as_str() {
                    "trunc" | "zext" | "sext" => {
                        if method.name == "trunc" && args.len() == 2 {
                            // trunc<Hi,Lo>() → extracts bits [Hi:Lo], result width = Hi - Lo + 1
                            let hi = self.eval_const_expr(&args[0], local_types);
                            let lo = self.eval_const_expr(&args[1], local_types);
                            match (hi, lo) {
                                (Some(h), Some(l)) if h >= l => {
                                    let w = (h - l + 1) as u32;
                                    if let Ty::SInt(_) = base_ty { Ty::SInt(w) } else { Ty::UInt(w) }
                                }
                                _ => Ty::Error,
                            }
                        } else if let Some(width_expr) = args.first() {
                            if let Some(w) = self.eval_const_expr(width_expr, local_types) {
                                if method.name == "sext" {
                                    Ty::SInt(w as u32)
                                } else if let Ty::SInt(_) = base_ty {
                                    Ty::SInt(w as u32)
                                } else {
                                    Ty::UInt(w as u32)
                                }
                            } else {
                                Ty::Error
                            }
                        } else {
                            Ty::Error
                        }
                    }
                    _ => Ty::Error,
                }
            }
            ExprKind::Cast(_, ty) => self.resolve_type_expr(ty, module_name, local_types),
            ExprKind::Index(base, _) => {
                let base_ty = self.resolve_expr_type(base, module_name, local_types);
                match base_ty {
                    Ty::Vec(inner, _) => *inner,
                    // Bit-select of a UInt/SInt produces a single bit; treat as Bool
                    // so it can be used directly in boolean expressions.
                    Ty::UInt(_) | Ty::SInt(_) => Ty::Bool,
                    _ => Ty::Bit,
                }
            }
            ExprKind::StructLiteral(name, _) => Ty::Struct(name.name.clone()),
            ExprKind::EnumVariant(name, _) => {
                if let Some((sym, _)) = self.symbols.globals.get(&name.name) {
                    if let crate::resolve::Symbol::Enum(info) = sym {
                        let bits = enum_width(info.variants.len());
                        return Ty::Enum(name.name.clone(), bits);
                    }
                }
                Ty::Error
            }
            ExprKind::Match(scrutinee, arms) => {
                let _ty = self.resolve_expr_type(scrutinee, module_name, local_types);
                if let Some(arm) = arms.first() {
                    if let Some(_stmt) = arm.body.first() {}
                }
                Ty::Error
            }
            ExprKind::ExprMatch(scrutinee, arms) => {
                let patterns: Vec<Pattern> = arms.iter().map(|a| a.pattern.clone()).collect();
                self.check_match_exhaustive(scrutinee, &patterns, expr.span, module_name, local_types);
                // Return type from first non-wildcard arm
                for arm in arms {
                    return self.resolve_expr_type(&arm.value, module_name, local_types);
                }
                Ty::Error
            }
            ExprKind::Concat(parts) => {
                // Total width = sum of each part's width (Bool=1, UInt<N>=N, else 1)
                let total: u32 = parts.iter().map(|p| {
                    match self.resolve_expr_type(p, module_name, local_types) {
                        Ty::UInt(w) | Ty::SInt(w) => w,
                        Ty::Bool | Ty::Bit => 1,
                        _ => 1,
                    }
                }).sum();
                Ty::UInt(total)
            }
            ExprKind::Clog2(arg) => {
                // $clog2 returns a compile-time constant width value
                if let Some(v) = self.eval_const_expr(arg, local_types) {
                    let bits = if v == 0 { 1 } else { 64 - v.leading_zeros() as u64 };
                    Ty::UInt(bits as u32)
                } else {
                    Ty::UInt(32) // fallback: treat as generic integer
                }
            }
            ExprKind::Ternary(_cond, then_expr, else_expr) => {
                // Return the type of the then branch; else branch should match.
                let then_ty = self.resolve_expr_type(then_expr, module_name, local_types);
                if matches!(then_ty, Ty::Error) {
                    self.resolve_expr_type(else_expr, module_name, local_types)
                } else {
                    then_ty
                }
            }
            ExprKind::FunctionCall(name, call_args) => {
                if let Some((Symbol::Function(overloads), _)) = self.symbols.globals.get(name) {
                    // Resolve argument types first.
                    let arg_tys: Vec<Ty> = call_args.iter()
                        .map(|a| {
                            let mut lt = local_types.clone();
                            self.resolve_expr_type(a, module_name, &mut lt)
                        })
                        .collect();

                    // Find matching overload: same arity, compatible types.
                    let overloads = overloads.clone(); // detach borrow so we can call &mut self methods
                    let chosen = overloads.iter().enumerate().find(|(_, ov)| {
                        if ov.arg_types.len() != arg_tys.len() { return false; }
                        ov.arg_types.iter().zip(arg_tys.iter()).all(|(expected_te, actual_ty)| {
                            match (expected_te, actual_ty) {
                                (TypeExpr::UInt(we), Ty::UInt(wa)) => {
                                    // Compare widths when the expression is a simple literal.
                                    eval_type_width_expr(we).map_or(true, |ew| ew == *wa)
                                }
                                (TypeExpr::SInt(we), Ty::SInt(wa)) => {
                                    eval_type_width_expr(we).map_or(true, |ew| ew == *wa)
                                }
                                (TypeExpr::Bool, Ty::Bool) => true,
                                (TypeExpr::Bit,  Ty::Bit)  => true,
                                (TypeExpr::UInt(_), Ty::Todo)
                                | (TypeExpr::SInt(_), Ty::Todo) => true,
                                _ => false,
                            }
                        })
                    });

                    match chosen {
                        Some((idx, ov)) => {
                            if overloads.len() > 1 {
                                self.overload_map.insert(expr.span.start, idx);
                            }
                            let ret_ty = ov.ret_ty.clone();
                            self.resolve_type_expr(&ret_ty, module_name, local_types)
                        }
                        None => {
                            // No exact type match; try arity-only match as fallback.
                            if let Some(ov) = overloads.iter().find(|ov| ov.arg_types.len() == call_args.len()) {
                                let ret_ty = ov.ret_ty.clone();
                                self.resolve_type_expr(&ret_ty, module_name, local_types)
                            } else {
                                self.errors.push(CompileError::general(
                                    &format!("no matching overload for `{name}` with {} argument(s)", call_args.len()),
                                    expr.span,
                                ));
                                Ty::Error
                            }
                        }
                    }
                } else {
                    self.errors.push(CompileError::general(
                        &format!("unknown function `{name}`"),
                        expr.span,
                    ));
                    Ty::Error
                }
            }
        }
    }

    fn binop_result_type(&mut self, op: BinOp, lt: &Ty, rt: &Ty, _span: Span) -> Ty {
        if *lt == Ty::Todo || *rt == Ty::Todo {
            return Ty::Todo;
        }
        if *lt == Ty::Error || *rt == Ty::Error {
            return Ty::Error;
        }

        match op {
            BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
                Ty::Bool
            }
            BinOp::And | BinOp::Or => Ty::Bool,
            BinOp::Add | BinOp::Sub => {
                let lw = lt.width().unwrap_or(1);
                let rw = rt.width().unwrap_or(1);
                let w = lw.max(rw) + 1;
                if matches!(lt, Ty::SInt(_)) || matches!(rt, Ty::SInt(_)) {
                    Ty::SInt(w)
                } else {
                    Ty::UInt(w)
                }
            }
            BinOp::Mul => {
                let lw = lt.width().unwrap_or(1);
                let rw = rt.width().unwrap_or(1);
                if matches!(lt, Ty::SInt(_)) || matches!(rt, Ty::SInt(_)) {
                    Ty::SInt(lw + rw)
                } else {
                    Ty::UInt(lw + rw)
                }
            }
            BinOp::Div | BinOp::Mod => lt.clone(),
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                // Bool is UInt<1>; bitwise ops on two 1-bit types stay Bool.
                let lw = lt.width().unwrap_or(1);
                let rw = rt.width().unwrap_or(1);
                if lw.max(rw) == 1 { Ty::Bool } else { Ty::UInt(lw.max(rw)) }
            }
            BinOp::Shl | BinOp::Shr => lt.clone(),
        }
    }

    fn eval_const_expr(&self, expr: &Expr, local_types: &HashMap<String, Ty>) -> Option<u64> {
        match &expr.kind {
            ExprKind::Literal(LitKind::Dec(v)) => Some(*v),
            ExprKind::Literal(LitKind::Hex(v)) => Some(*v),
            ExprKind::Literal(LitKind::Bin(v)) => Some(*v),
            ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v),
            ExprKind::Ident(name) => {
                // Try to resolve param value from symbol table
                // For MVP, check if it's a known param with a default
                for item in &self.source.items {
                    if let Item::Module(m) = item {
                        for p in &m.params {
                            if p.name.name == *name {
                                if let Some(default) = &p.default {
                                    return self.eval_const_expr(default, local_types);
                                }
                            }
                        }
                    }
                }
                None
            }
            ExprKind::Binary(BinOp::Add, lhs, rhs) => {
                let l = self.eval_const_expr(lhs, local_types)?;
                let r = self.eval_const_expr(rhs, local_types)?;
                Some(l + r)
            }
            ExprKind::Binary(BinOp::Sub, lhs, rhs) => {
                let l = self.eval_const_expr(lhs, local_types)?;
                let r = self.eval_const_expr(rhs, local_types)?;
                Some(l.wrapping_sub(r))
            }
            ExprKind::Binary(BinOp::Mul, lhs, rhs) => {
                let l = self.eval_const_expr(lhs, local_types)?;
                let r = self.eval_const_expr(rhs, local_types)?;
                Some(l * r)
            }
            ExprKind::Clog2(arg) => {
                let v = self.eval_const_expr(arg, local_types)?;
                if v <= 1 { Some(1) } else { Some(64 - (v - 1).leading_zeros() as u64) }
            }
            _ => None,
        }
    }

    fn check_pascal_case(&mut self, ident: &Ident) {
        let name = &ident.name;
        if name.is_empty() {
            return;
        }
        // Monomorphized variant names contain `__` (e.g. `Foo__ENABLE_1`).
        // They are compiler-generated and do not need to satisfy PascalCase.
        if name.contains("__") {
            return;
        }
        if !name.chars().next().unwrap().is_uppercase() || name.contains('_') {
            self.errors.push(CompileError::NamingViolation {
                message: format!("`{name}` should be PascalCase"),
                span: crate::diagnostics::span_to_source_span(ident.span),
            });
        }
    }

    fn check_snake_case(&mut self, ident: &Ident) {
        let name = &ident.name;
        if name.is_empty() {
            return;
        }
        if name.chars().any(|c| c.is_uppercase()) {
            self.errors.push(CompileError::NamingViolation {
                message: format!("`{name}` should be snake_case"),
                span: crate::diagnostics::span_to_source_span(ident.span),
            });
        }
    }

    fn check_upper_snake(&mut self, ident: &Ident) {
        let name = &ident.name;
        if name.is_empty() {
            return;
        }
        if name.chars().any(|c| c.is_lowercase()) {
            self.errors.push(CompileError::NamingViolation {
                message: format!("`{name}` should be UPPER_SNAKE_CASE"),
                span: crate::diagnostics::span_to_source_span(ident.span),
            });
        }
    }

    // ── FSM ───────────────────────────────────────────────────────────────────

    fn check_fsm(&mut self, f: &FsmDecl) {
        self.check_pascal_case(&f.name);
        for p in &f.params {
            self.check_upper_snake(&p.name);
        }
        for p in &f.ports {
            self.check_snake_case(&p.name);
        }

        let _state_names: Vec<&str> = f.state_names.iter().map(|s| s.name.as_str()).collect();

        // Every declared state must have a state body
        for sn in &f.state_names {
            if !f.states.iter().any(|sb| sb.name.name == sn.name) {
                self.errors.push(CompileError::general(
                    &format!("state `{}` has no body", sn.name),
                    sn.span,
                ));
            }
        }

        // Every state body must have at least one transition
        for sb in &f.states {
            if sb.transitions.is_empty() {
                self.errors.push(CompileError::general(
                    &format!("state `{}` has no transitions", sb.name.name),
                    sb.name.span,
                ));
            }
            // All output ports must be driven in each state, unless they have
            // a `default` value declared (in which case the FSM codegen emits
            // the default and the per-state block only needs to override it).
            let out_ports: Vec<&PortDecl> = f
                .ports
                .iter()
                .filter(|p| p.direction == Direction::Out)
                .collect();
            let driven: Vec<&str> = sb
                .comb_stmts
                .iter()
                .filter_map(|s| {
                    if let CombStmt::Assign(a) = s {
                        Some(a.target.name.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            for op in &out_ports {
                let name = op.name.name.as_str();
                if !driven.contains(&name) && op.default.is_none() {
                    self.errors.push(CompileError::general(
                        &format!(
                            "output port `{name}` not driven in state `{}`",
                            sb.name.name
                        ),
                        sb.name.span,
                    ));
                }
            }
        }
    }

    // ── RAM ───────────────────────────────────────────────────────────────────

    fn check_ram(&mut self, r: &RamDecl) {
        self.check_pascal_case(&r.name);
        for p in &r.params {
            self.check_upper_snake(&p.name);
        }
        for p in &r.ports {
            self.check_snake_case(&p.name);
        }
        for pg in &r.port_groups {
            self.check_snake_case(&pg.name);
            for s in &pg.signals {
                self.check_snake_case(&s.name);
            }
        }
        // Require at least one port group
        if r.port_groups.is_empty() {
            self.errors.push(CompileError::general(
                &format!("ram `{}` has no port groups", r.name.name),
                r.name.span,
            ));
        }
        // true_dual requires exactly 2 port groups
        if r.kind == crate::ast::RamKind::TrueDual && r.port_groups.len() != 2 {
            self.errors.push(CompileError::general(
                &format!("true_dual ram `{}` must have exactly 2 port groups", r.name.name),
                r.name.span,
            ));
        }
        // simple_dual requires exactly 2 port groups
        if r.kind == crate::ast::RamKind::SimpleDual && r.port_groups.len() != 2 {
            self.errors.push(CompileError::general(
                &format!("simple_dual ram `{}` must have exactly 2 port groups", r.name.name),
                r.name.span,
            ));
        }
    }

    // ── FIFO ──────────────────────────────────────────────────────────────────

    fn check_fifo(&mut self, f: &FifoDecl) {
        self.check_pascal_case(&f.name);
        for p in &f.params {
            self.check_upper_snake(&p.name);
        }
        for p in &f.ports {
            self.check_snake_case(&p.name);
        }

        // Required port names
        let required = ["push_valid", "push_ready", "push_data",
                        "pop_valid",  "pop_ready",  "pop_data"];
        let present: Vec<&str> = f.ports.iter().map(|p| p.name.name.as_str()).collect();
        for req in &required {
            if !present.contains(req) {
                self.errors.push(CompileError::general(
                    &format!("fifo `{}` is missing required port `{req}`", f.name.name),
                    f.name.span,
                ));
            }
        }
    }

    // ── Counter ───────────────────────────────────────────────────────────────

    fn check_counter(&mut self, c: &crate::ast::CounterDecl) {
        self.check_pascal_case(&c.name);
        for p in &c.params {
            self.check_upper_snake(&p.name);
        }
        for p in &c.ports {
            self.check_snake_case(&p.name);
        }
    }

    // ── Arbiter ───────────────────────────────────────────────────────────────

    fn check_arbiter(&mut self, a: &crate::ast::ArbiterDecl) {
        self.check_pascal_case(&a.name);
        for p in &a.params {
            self.check_upper_snake(&p.name);
        }
        for p in &a.ports {
            self.check_snake_case(&p.name);
        }
        for pa in &a.port_arrays {
            self.check_snake_case(&pa.name);
            for s in &pa.signals {
                self.check_snake_case(&s.name);
            }
        }
    }

    // ── Regfile ───────────────────────────────────────────────────────────────

    fn check_regfile(&mut self, r: &crate::ast::RegfileDecl) {
        self.check_pascal_case(&r.name);
        for p in &r.params {
            self.check_upper_snake(&p.name);
        }
        for p in &r.ports {
            self.check_snake_case(&p.name);
        }
        if let Some(rp) = &r.read_ports {
            self.check_snake_case(&rp.name);
            for s in &rp.signals {
                self.check_snake_case(&s.name);
            }
        }
        if let Some(wp) = &r.write_ports {
            self.check_snake_case(&wp.name);
            for s in &wp.signals {
                self.check_snake_case(&s.name);
            }
        }
    }
    // ── Pipeline ──────────────────────────────────────────────────────────────

    fn check_pipeline(&mut self, p: &PipelineDecl) {
        self.check_pascal_case(&p.name);

        for param in &p.params {
            self.check_upper_snake(&param.name);
        }
        for port in &p.ports {
            self.check_snake_case(&port.name);
        }

        let stage_names: Vec<&str> = p.stages.iter().map(|s| s.name.name.as_str()).collect();

        for stage in &p.stages {
            self.check_pascal_case(&stage.name);

            // Every stage must have at least one RegDecl + RegBlock (always on)
            let has_reg = stage.body.iter().any(|i| matches!(i, ModuleBodyItem::RegDecl(_)));
            let has_always = stage.body.iter().any(|i| matches!(i, ModuleBodyItem::RegBlock(_)));

            if !has_reg || !has_always {
                self.errors.push(CompileError::general(
                    &format!(
                        "pipeline stage `{}` has no registers; every stage must capture data into at least one register",
                        stage.name.name
                    ),
                    stage.name.span,
                ));
            }

            // Check naming within stage body
            for item in &stage.body {
                match item {
                    ModuleBodyItem::RegDecl(r) => self.check_snake_case(&r.name),
                    ModuleBodyItem::LetBinding(l) => self.check_snake_case(&l.name),
                    ModuleBodyItem::PipeRegDecl(p) => self.check_snake_case(&p.name),
                    ModuleBodyItem::Inst(inst) => self.check_snake_case(&inst.name),
                    _ => {}
                }
            }
        }

        // Validate flush targets are declared stages
        for flush in &p.flush_directives {
            if !stage_names.contains(&flush.target_stage.name.as_str()) {
                self.errors.push(CompileError::general(
                    &format!(
                        "flush target `{}` is not a declared stage in pipeline `{}`",
                        flush.target_stage.name, p.name.name
                    ),
                    flush.target_stage.span,
                ));
            }
        }

        // Check output ports are driven (at least one comb block in some stage assigns them)
        let mut driven: HashSet<String> = HashSet::new();
        for stage in &p.stages {
            for item in &stage.body {
                if let ModuleBodyItem::CombBlock(cb) = item {
                    for stmt in &cb.stmts {
                        if let CombStmt::Assign(a) = stmt {
                            driven.insert(a.target.name.clone());
                        }
                    }
                }
                if let ModuleBodyItem::Inst(inst) = item {
                    for conn in &inst.connections {
                        if conn.direction == ConnectDir::Output {
                            if let ExprKind::Ident(name) = &conn.signal.kind {
                                driven.insert(name.clone());
                            }
                        }
                    }
                }
            }
        }
        for port in &p.ports {
            if port.direction == Direction::Out && !driven.contains(&port.name.name) {
                self.errors.push(CompileError::UndriveOutput {
                    name: port.name.name.clone(),
                    span: crate::diagnostics::span_to_source_span(port.name.span),
                });
            }
        }
    }

    // ── Linklist ──────────────────────────────────────────────────────────────

    fn check_linklist(&mut self, l: &crate::ast::LinklistDecl) {
        use crate::ast::LinklistKind;

        self.check_pascal_case(&l.name);
        for p in &l.params {
            self.check_upper_snake(&p.name);
        }
        for p in &l.ports {
            self.check_snake_case(&p.name);
        }

        // Required params: DEPTH (const) and DATA (type)
        let has_depth = l.params.iter().any(|p| p.name.name == "DEPTH");
        let has_data  = l.params.iter().any(|p| p.name.name == "DATA");
        if !has_depth {
            self.errors.push(CompileError::general(
                &format!("linklist `{}` is missing required param `DEPTH: const`", l.name.name),
                l.name.span,
            ));
        }
        if !has_data {
            self.errors.push(CompileError::general(
                &format!("linklist `{}` is missing required param `DATA: type`", l.name.name),
                l.name.span,
            ));
        }

        // Required ports: clk and rst
        let has_clk = l.ports.iter().any(|p| matches!(&p.ty, crate::ast::TypeExpr::Clock(_)));
        let has_rst = l.ports.iter().any(|p| matches!(&p.ty, crate::ast::TypeExpr::Reset(_, _)));
        if !has_clk {
            self.errors.push(CompileError::general(
                &format!("linklist `{}` is missing required `clk: in Clock<...>` port", l.name.name),
                l.name.span,
            ));
        }
        if !has_rst {
            self.errors.push(CompileError::general(
                &format!("linklist `{}` is missing required `rst: in Reset<...>` port", l.name.name),
                l.name.span,
            ));
        }

        // `prev` op requires doubly or circular_doubly
        for op in &l.ops {
            self.check_snake_case(&op.name);
            for p in &op.ports { self.check_snake_case(&p.name); }

            if op.name.name == "prev"
                && !matches!(l.kind, LinklistKind::Doubly | LinklistKind::CircularDoubly)
            {
                self.errors.push(CompileError::general(
                    &format!(
                        "linklist `{}`: op `prev` requires `kind doubly` or `kind circular_doubly`",
                        l.name.name
                    ),
                    op.name.span,
                ));
            }

            // Known op names
            let known_ops = [
                "alloc", "free", "insert_head", "insert_tail", "insert_after",
                "delete_head", "delete", "read_data", "write_data", "next", "prev", "length",
            ];
            if !known_ops.contains(&op.name.name.as_str()) {
                self.errors.push(CompileError::general(
                    &format!(
                        "linklist `{}`: unknown op `{}`; known ops: {}",
                        l.name.name, op.name.name, known_ops.join(", ")
                    ),
                    op.name.span,
                ));
            }

            if op.latency == 0 {
                self.errors.push(CompileError::general(
                    &format!("linklist `{}`: op `{}` latency must be ≥ 1", l.name.name, op.name.name),
                    op.name.span,
                ));
            }
        }

        // Warn about O(N) insert_tail without track tail
        if l.ops.iter().any(|op| op.name.name == "insert_tail") && !l.track_tail {
            self.warnings.push(CompileWarning {
                message: format!(
                    "linklist `{}`: `op insert_tail` without `track tail: true` requires O(N) traversal",
                    l.name.name
                ),
                span: l.name.span,
            });
        }
    }

    fn check_function(&mut self, f: &FunctionDecl) {
        self.check_pascal_case(&f.name);
        for arg in &f.args {
            self.check_snake_case(&arg.name);
        }

        // Build local type environment with args
        let mut local_types: HashMap<String, Ty> = HashMap::new();
        for arg in &f.args {
            let ty = self.resolve_type_expr(&arg.ty, &f.name.name, &local_types);
            local_types.insert(arg.name.name.clone(), ty);
        }

        let expected_ret = self.resolve_type_expr(&f.ret_ty, &f.name.name, &local_types);

        for item in &f.body {
            match item {
                FunctionBodyItem::Let(l) => {
                    self.check_snake_case(&l.name);
                    let val_ty = self.resolve_expr_type(&l.value, &f.name.name, &local_types);
                    let ty = if let Some(ann) = &l.ty {
                        self.resolve_type_expr(ann, &f.name.name, &local_types)
                    } else {
                        val_ty
                    };
                    local_types.insert(l.name.name.clone(), ty);
                }
                FunctionBodyItem::Return(expr) => {
                    let ret_ty = self.resolve_expr_type(expr, &f.name.name, &local_types);
                    if !matches!(ret_ty, Ty::Error | Ty::Todo)
                        && !matches!(expected_ret, Ty::Error | Ty::Todo)
                        && ret_ty != expected_ret
                        && !types_compatible(&expected_ret, &ret_ty)
                    {
                        self.warnings.push(CompileWarning {
                            message: format!(
                                "function `{}`: return type mismatch (declared {}, got {})",
                                f.name.name, expected_ret.display(), ret_ty.display()
                            ),
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
}

/// Evaluate a simple literal type-width expression (e.g. the `8` in `UInt<8>`).
/// Returns `None` for non-literal expressions (params, arithmetic, etc.).
fn eval_type_width_expr(e: &Expr) -> Option<u32> {
    match &e.kind {
        ExprKind::Literal(LitKind::Dec(n)) => Some(*n as u32),
        ExprKind::Literal(LitKind::Hex(n)) => Some(*n as u32),
        _ => None,
    }
}

/// Returns true if `actual` is assignable to `expected` without an explicit cast.
/// In hardware, narrower unsigned values zero-extend to wider wires.
fn types_compatible(expected: &Ty, actual: &Ty) -> bool {
    match (expected, actual) {
        (Ty::UInt(em), Ty::UInt(am)) => am <= em,
        (Ty::SInt(em), Ty::SInt(am)) => am <= em,
        // Bool ≡ UInt<1>: freely assignable in both directions.
        (Ty::Bool, Ty::UInt(1)) | (Ty::UInt(1), Ty::Bool) => true,
        (Ty::Bool, Ty::Bool) => true,
        _ => false,
    }
}

pub fn enum_width(num_variants: usize) -> u32 {
    if num_variants <= 1 {
        1
    } else {
        (num_variants as f64).log2().ceil() as u32
    }
}
