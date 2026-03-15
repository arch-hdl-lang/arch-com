use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostics::{CompileError, CompileWarning};
use crate::lexer::Span;
use crate::resolve::SymbolTable;

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
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a SymbolTable, source: &'a SourceFile) -> Self {
        Self {
            symbols,
            source,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn check(mut self) -> Result<Vec<CompileWarning>, Vec<CompileError>> {
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
            }
        }
        if self.errors.is_empty() {
            Ok(self.warnings)
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
                }
                ModuleBodyItem::CombBlock(cb) => {
                    for stmt in &cb.stmts {
                        self.check_comb_stmt(stmt, &m.name.name, &local_types, &mut driven);
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    self.check_snake_case(&l.name);
                    let ty = self.resolve_expr_type(&l.value, &m.name.name, &local_types);
                    if let Some(declared_ty) = &l.ty {
                        let expected = self.resolve_type_expr(declared_ty, &m.name.name, &local_types);
                        if expected != Ty::Error && ty != Ty::Error && ty != Ty::Todo && expected != ty {
                            self.errors.push(CompileError::type_mismatch(
                                &expected.display(),
                                &ty.display(),
                                l.value.span,
                            ));
                        }
                    }
                    local_types.insert(l.name.name.clone(), ty);
                    driven.insert(l.name.name.clone());
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
                let _ty = self.resolve_expr_type(&m.scrutinee, module_name, local_types);
                for arm in &m.arms {
                    for s in &arm.body {
                        self.check_reg_stmt(s, module_name, local_types, driven);
                    }
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
            }
            CombStmt::IfElse(ie) => {
                let _cond_ty = self.resolve_expr_type(&ie.cond, module_name, local_types);
                for s in &ie.then_stmts {
                    self.check_comb_stmt(s, module_name, local_types, driven);
                }
                for s in &ie.else_stmts {
                    self.check_comb_stmt(s, module_name, local_types, driven);
                }
            }
            CombStmt::MatchExpr(m) => {
                let _ty = self.resolve_expr_type(&m.scrutinee, module_name, local_types);
                for arm in &m.arms {
                    for _s in &arm.body {
                        // Convert Stmt to check like comb (best effort for MVP)
                    }
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
            TypeExpr::Reset(kind) => Ty::Reset(*kind),
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
                        if let Some(width_expr) = args.first() {
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
                let _ty = self.resolve_expr_type(scrutinee, module_name, local_types);
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
                let lw = lt.width().unwrap_or(1);
                let rw = rt.width().unwrap_or(1);
                Ty::UInt(lw.max(rw))
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
            _ => None,
        }
    }

    fn check_pascal_case(&mut self, ident: &Ident) {
        let name = &ident.name;
        if name.is_empty() {
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
            // All output ports must be driven in each state
            let out_ports: Vec<&str> = f
                .ports
                .iter()
                .filter(|p| p.direction == Direction::Out)
                .map(|p| p.name.name.as_str())
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
                if !driven.contains(op) {
                    self.errors.push(CompileError::general(
                        &format!(
                            "output port `{op}` not driven in state `{}`",
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
}

pub fn enum_width(num_variants: usize) -> u32 {
    if num_variants <= 1 {
        1
    } else {
        (num_variants as f64).log2().ceil() as u32
    }
}
