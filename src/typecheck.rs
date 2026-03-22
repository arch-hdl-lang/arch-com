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
                Item::Template(t) => self.check_template(t),
                Item::Synchronizer(s) => self.check_synchronizer(s),
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

        // Collect reg names for comb target validation
        let reg_names: HashSet<String> = m.body.iter().filter_map(|item| {
            if let ModuleBodyItem::RegDecl(r) = item { Some(r.name.name.clone()) } else { None }
        }).collect();

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
                    // same seq block must agree on signal name, sync/async, and polarity.
                    self.check_always_block_reset_consistency(rb, m);
                }
                ModuleBodyItem::CombBlock(cb) => {
                    for stmt in &cb.stmts {
                        self.check_comb_stmt(stmt, &m.name.name, &local_types, &mut driven, &reg_names);
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
                ModuleBodyItem::WireDecl(w) => {
                    self.check_snake_case(&w.name);
                    let ty = self.resolve_type_expr(&w.ty, &m.name.name, &local_types);
                    local_types.insert(w.name.name.clone(), ty);
                    // Wire is NOT marked as driven here — it must be driven by a comb block
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

        // ── CDC check: detect cross-domain register reads ─────────────────────
        // Build clock port → domain name map
        let clk_domain: HashMap<String, String> = m.ports.iter()
            .filter_map(|p| if let TypeExpr::Clock(domain) = &p.ty {
                Some((p.name.name.clone(), domain.name.clone()))
            } else { None })
            .collect();

        if clk_domain.len() >= 2 {
            // Build reg → domain map (which domain drives each register)
            let mut reg_domain: HashMap<String, String> = HashMap::new();
            for item in &m.body {
                if let ModuleBodyItem::RegBlock(rb) = item {
                    if let Some(domain) = clk_domain.get(&rb.clock.name) {
                        let mut assigned = HashSet::new();
                        Self::collect_stmt_targets(&rb.stmts, &mut assigned);
                        for name in assigned {
                            reg_domain.insert(name, domain.clone());
                        }
                    }
                }
            }

            // For each seq block, check reads against domain map
            for item in &m.body {
                if let ModuleBodyItem::RegBlock(rb) = item {
                    if let Some(this_domain) = clk_domain.get(&rb.clock.name) {
                        let mut reads = HashSet::new();
                        Self::collect_stmt_reads(&rb.stmts, &mut reads);
                        for name in &reads {
                            if let Some(src_domain) = reg_domain.get(name) {
                                if src_domain != this_domain {
                                    self.errors.push(CompileError::general(
                                        &format!(
                                            "CDC violation: register `{name}` is driven in domain `{src_domain}` \
                                             but read in domain `{this_domain}` (clock `{}`). \
                                             Use a `synchronizer` or async `fifo` to cross clock domains",
                                            rb.clock.name
                                        ),
                                        rb.span,
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            // For each comb block, check if it reads registers from multiple domains
            for item in &m.body {
                if let ModuleBodyItem::CombBlock(cb) = item {
                    let mut reads = HashSet::new();
                    Self::collect_comb_stmt_reads(&cb.stmts, &mut reads);
                    for name in &reads {
                        // A comb block reading a cross-domain register is unsafe —
                        // it could be consumed by any domain downstream
                        if reg_domain.contains_key(name) {
                            // Find which domains consume this comb block's outputs
                            let mut comb_targets = HashSet::new();
                            Self::collect_comb_stmt_targets(&cb.stmts, &mut comb_targets);
                            for target in &comb_targets {
                                // Check if any seq block in a different domain reads this target
                                for item2 in &m.body {
                                    if let ModuleBodyItem::RegBlock(rb) = item2 {
                                        if let Some(consumer_domain) = clk_domain.get(&rb.clock.name) {
                                            let mut seq_reads = HashSet::new();
                                            Self::collect_stmt_reads(&rb.stmts, &mut seq_reads);
                                            if seq_reads.contains(target) {
                                                if let Some(src_domain) = reg_domain.get(name) {
                                                    if src_domain != consumer_domain {
                                                        self.errors.push(CompileError::general(
                                                            &format!(
                                                                "CDC violation: comb signal `{target}` reads register `{name}` \
                                                                 (domain `{src_domain}`) but is consumed in domain `{consumer_domain}`. \
                                                                 Use a `synchronizer` or async `fifo` to cross clock domains"
                                                            ),
                                                            cb.span,
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // CDC check across instance boundaries
            for item in &m.body {
                if let ModuleBodyItem::Inst(inst) = item {
                    self.check_inst_cdc(inst, &clk_domain, &reg_domain, m);
                }
            }
        }

        // Validate `implements` template conformance
        if let Some(ref tmpl_name) = m.implements {
            self.check_implements(m, tmpl_name);
        }
    }

    fn check_implements(&mut self, m: &ModuleDecl, tmpl_name: &Ident) {
        // Find the template in the source file
        let tmpl = self.source.items.iter().find_map(|item| {
            if let Item::Template(t) = item {
                if t.name.name == tmpl_name.name { Some(t) } else { None }
            } else {
                None
            }
        });
        let tmpl = match tmpl {
            Some(t) => t,
            None => {
                self.errors.push(CompileError::general(
                    &format!("template `{}` not found", tmpl_name.name),
                    tmpl_name.span,
                ));
                return;
            }
        };

        // Check required params
        for tp in &tmpl.params {
            let found = m.params.iter().any(|mp| mp.name.name == tp.name.name);
            if !found {
                self.errors.push(CompileError::general(
                    &format!("module `{}` is missing param `{}` required by template `{}`",
                             m.name.name, tp.name.name, tmpl.name.name),
                    m.name.span,
                ));
            }
        }

        // Check required ports (name + direction)
        for tp in &tmpl.ports {
            let found = m.ports.iter().find(|mp| mp.name.name == tp.name.name);
            match found {
                None => {
                    self.errors.push(CompileError::general(
                        &format!("module `{}` is missing port `{}` required by template `{}`",
                                 m.name.name, tp.name.name, tmpl.name.name),
                        m.name.span,
                    ));
                }
                Some(mp) => {
                    if mp.direction != tp.direction {
                        self.errors.push(CompileError::general(
                            &format!("port `{}` direction mismatch: template requires {:?}, module has {:?}",
                                     tp.name.name, tp.direction, mp.direction),
                            mp.name.span,
                        ));
                    }
                }
            }
        }

        // Check required hooks
        for th in &tmpl.hooks {
            let found = m.hooks.iter().any(|mh| mh.hook_name.name == th.name.name);
            if !found {
                self.errors.push(CompileError::general(
                    &format!("module `{}` is missing hook `{}` required by template `{}`",
                             m.name.name, th.name.name, tmpl.name.name),
                    m.name.span,
                ));
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
                            "register `{}` uses reset signal `{}` but other registers in the same seq block use `{}`",
                            name, signal, first.signal
                        ),
                        rd.span,
                    ));
                }
                if kind != first.kind {
                    self.errors.push(CompileError::general(
                        &format!(
                            "register `{}` uses {} reset but other registers in the same seq block use {}",
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
                            "register `{}` uses active-{} reset but other registers in the same seq block use active-{}",
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
        reg_names: &HashSet<String>,
    ) {
        match stmt {
            CombStmt::Assign(a) => {
                // Regs must be assigned in seq blocks, not comb blocks
                if reg_names.contains(&a.target.name) {
                    self.errors.push(CompileError::general(
                        &format!(
                            "`{}` is a reg — assign it with `<=` in a `seq` block, not `=` in a `comb` block",
                            a.target.name
                        ),
                        a.span,
                    ));
                }
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
                    self.check_comb_stmt(s, module_name, local_types, &mut then_driven, reg_names);
                }
                let mut else_driven = driven.clone();
                for s in &ie.else_stmts {
                    self.check_comb_stmt(s, module_name, local_types, &mut else_driven, reg_names);
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
                // Check for forbidden hierarchical instance reference (inst_name.port_name)
                if let ExprKind::Ident(base_name) = &base.kind {
                    let is_inst = self.source.items.iter().any(|item| {
                        if let crate::ast::Item::Module(m) = item {
                            if m.name.name == module_name {
                                return m.body.iter().any(|bi| {
                                    if let ModuleBodyItem::Inst(inst) = bi {
                                        inst.name.name == *base_name
                                    } else {
                                        false
                                    }
                                });
                            }
                        }
                        false
                    });
                    if is_inst {
                        self.errors.push(CompileError::general(
                            &format!(
                                "hierarchical reference `{}.{}` is not allowed; \
                                 use `connect {} -> wire_name` in the inst block instead",
                                base_name, field.name, field.name
                            ),
                            expr.span,
                        ));
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
            ExprKind::Repeat(count, value) => {
                // {N{expr}} — total width = N * width(expr)
                let val_width = match self.resolve_expr_type(value, module_name, local_types) {
                    Ty::UInt(w) | Ty::SInt(w) => w,
                    Ty::Bool | Ty::Bit => 1,
                    _ => 1,
                };
                let n = self.eval_const_expr(count, local_types).unwrap_or(1) as u32;
                Ty::UInt(n * val_width)
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

    // ── CDC helpers ────────────────────────────────────────────────────────

    /// Check CDC violations across an instance boundary.
    /// For each data connection, verify that the signal's clock domain in the
    /// parent matches the port's clock domain in the child module.
    fn check_inst_cdc(
        &mut self,
        inst: &InstDecl,
        parent_clk_domain: &HashMap<String, String>,
        parent_reg_domain: &HashMap<String, String>,
        parent_module: &ModuleDecl,
    ) {
        // Find the instantiated module's definition
        let child_module = self.source.items.iter().find_map(|item| {
            if let Item::Module(m) = item {
                if m.name.name == inst.module_name.name { Some(m) } else { None }
            } else { None }
        });
        let child_module = match child_module {
            Some(m) => m,
            None => return, // Module not found in this file; skip
        };

        // Build child module's clock port → domain map
        let child_clk_domain: HashMap<String, String> = child_module.ports.iter()
            .filter_map(|p| if let TypeExpr::Clock(domain) = &p.ty {
                Some((p.name.name.clone(), domain.name.clone()))
            } else { None })
            .collect();

        if child_clk_domain.is_empty() {
            return; // Single-clock or no-clock child; no CDC concern
        }

        // Build child module's port → domain map (which domain uses each port)
        // A port is in a domain if a seq block on that clock reads/writes it
        let mut child_port_domain: HashMap<String, String> = HashMap::new();

        // Clock ports map to their own domain
        for (clk_name, domain) in &child_clk_domain {
            child_port_domain.insert(clk_name.clone(), domain.clone());
        }

        // Reset ports: if there's only one, it's shared; skip domain assignment
        // Data ports: determine domain from seq block usage
        for body_item in &child_module.body {
            if let ModuleBodyItem::RegBlock(rb) = body_item {
                if let Some(domain) = child_clk_domain.get(&rb.clock.name) {
                    // Registers assigned in this seq block
                    let mut assigned = HashSet::new();
                    Self::collect_stmt_targets(&rb.stmts, &mut assigned);

                    // Find which output ports these registers feed via comb blocks
                    for comb_item in &child_module.body {
                        if let ModuleBodyItem::CombBlock(cb) = comb_item {
                            let mut comb_reads = HashSet::new();
                            Self::collect_comb_stmt_reads(&cb.stmts, &mut comb_reads);
                            let mut comb_targets = HashSet::new();
                            Self::collect_comb_stmt_targets(&cb.stmts, &mut comb_targets);

                            // If this comb block reads any register from this domain,
                            // its output ports belong to this domain
                            if comb_reads.iter().any(|r| assigned.contains(r)) {
                                for target in &comb_targets {
                                    if child_module.ports.iter().any(|p| p.name.name == *target) {
                                        child_port_domain.insert(target.clone(), domain.clone());
                                    }
                                }
                            }
                        }
                    }

                    // Input ports read in this seq block belong to this domain
                    let mut reads = HashSet::new();
                    Self::collect_stmt_reads(&rb.stmts, &mut reads);
                    for read_name in &reads {
                        if child_module.ports.iter().any(|p| p.name.name == *read_name && p.direction == Direction::In) {
                            child_port_domain.insert(read_name.clone(), domain.clone());
                        }
                    }
                }
            }
        }

        // Now build the parent signal → domain map
        // Include: registers, comb outputs, and ports (clocks map to their domain)
        let mut parent_signal_domain: HashMap<String, String> = parent_reg_domain.clone();
        for (clk_name, domain) in parent_clk_domain {
            parent_signal_domain.insert(clk_name.clone(), domain.clone());
        }
        // Comb blocks: if a comb output is driven from a single-domain register, it's in that domain
        for body_item in &parent_module.body {
            if let ModuleBodyItem::CombBlock(cb) = body_item {
                let mut reads = HashSet::new();
                Self::collect_comb_stmt_reads(&cb.stmts, &mut reads);
                let mut targets = HashSet::new();
                Self::collect_comb_stmt_targets(&cb.stmts, &mut targets);
                // If all register reads are from the same domain, targets inherit that domain
                let domains: HashSet<&String> = reads.iter()
                    .filter_map(|r| parent_reg_domain.get(r))
                    .collect();
                if domains.len() == 1 {
                    let domain = domains.into_iter().next().unwrap();
                    for target in targets {
                        parent_signal_domain.insert(target, domain.clone());
                    }
                }
            }
        }

        // Build connection map: inst port name → connected signal name
        let conn_signal: HashMap<String, String> = inst.connections.iter()
            .filter_map(|c| {
                if let ExprKind::Ident(sig_name) = &c.signal.kind {
                    Some((c.port_name.name.clone(), sig_name.clone()))
                } else { None }
            })
            .collect();

        // Find which clock domain each inst clock port is connected to
        let inst_clk_mapping: HashMap<String, String> = inst.connections.iter()
            .filter_map(|c| {
                let child_port = child_module.ports.iter().find(|p| p.name.name == c.port_name.name)?;
                if let TypeExpr::Clock(_) = &child_port.ty {
                    if let ExprKind::Ident(sig_name) = &c.signal.kind {
                        parent_clk_domain.get(sig_name).map(|d| (c.port_name.name.clone(), d.clone()))
                    } else { None }
                } else { None }
            })
            .collect();

        // For each data connection, check domain compatibility
        for conn in &inst.connections {
            let port_name = &conn.port_name.name;

            // Skip clock and reset ports
            if let Some(child_port) = child_module.ports.iter().find(|p| p.name.name == *port_name) {
                if matches!(&child_port.ty, TypeExpr::Clock(_) | TypeExpr::Reset(..)) {
                    continue;
                }
            }

            // Get the child port's expected domain
            let child_domain = match child_port_domain.get(port_name) {
                Some(d) => d,
                None => continue, // Can't determine port's domain; skip
            };

            // Map child domain to parent domain via clock connections
            // Find which parent clock is connected to the child clock in this domain
            let expected_parent_domain = inst_clk_mapping.iter()
                .find_map(|(child_clk, parent_domain)| {
                    if child_clk_domain.get(child_clk) == Some(child_domain) {
                        Some(parent_domain.as_str())
                    } else { None }
                });

            let expected_parent_domain = match expected_parent_domain {
                Some(d) => d,
                None => continue,
            };

            // Get the connected signal's domain in the parent
            if let Some(sig_name) = conn_signal.get(port_name) {
                if let Some(sig_domain) = parent_signal_domain.get(sig_name) {
                    if sig_domain != expected_parent_domain {
                        self.errors.push(CompileError::general(
                            &format!(
                                "CDC violation at instance `{}`: signal `{}` (domain `{}`) \
                                 connected to port `{}` which operates in domain `{}` (mapped to parent domain `{}`). \
                                 Use a `synchronizer` or async `fifo` to cross clock domains",
                                inst.name.name, sig_name, sig_domain,
                                port_name, child_domain, expected_parent_domain
                            ),
                            conn.span,
                        ));
                    }
                }
            }
        }
    }

    /// Collect all register names assigned (targets) in a list of seq stmts.
    fn collect_stmt_targets(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    if let ExprKind::Ident(name) = &a.target.kind {
                        out.insert(name.clone());
                    }
                }
                Stmt::IfElse(ie) => {
                    Self::collect_stmt_targets(&ie.then_stmts, out);
                    Self::collect_stmt_targets(&ie.else_stmts, out);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        Self::collect_stmt_targets(&arm.body, out);
                    }
                }
                Stmt::Log(_) => {}
            }
        }
    }

    /// Collect all identifier names read (RHS) in a list of seq stmts.
    fn collect_stmt_reads(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    Self::collect_expr_reads(&a.value, out);
                }
                Stmt::IfElse(ie) => {
                    Self::collect_expr_reads(&ie.cond, out);
                    Self::collect_stmt_reads(&ie.then_stmts, out);
                    Self::collect_stmt_reads(&ie.else_stmts, out);
                }
                Stmt::Match(m) => {
                    Self::collect_expr_reads(&m.scrutinee, out);
                    for arm in &m.arms {
                        Self::collect_stmt_reads(&arm.body, out);
                    }
                }
                Stmt::Log(l) => {
                    for arg in &l.args { Self::collect_expr_reads(arg, out); }
                }
            }
        }
    }

    fn collect_expr_reads(expr: &Expr, out: &mut HashSet<String>) {
        match &expr.kind {
            ExprKind::Ident(name) => { out.insert(name.clone()); }
            ExprKind::Binary(_, lhs, rhs) => {
                Self::collect_expr_reads(lhs, out);
                Self::collect_expr_reads(rhs, out);
            }
            ExprKind::Unary(_, e) => Self::collect_expr_reads(e, out),
            ExprKind::Index(base, idx) => {
                Self::collect_expr_reads(base, out);
                Self::collect_expr_reads(idx, out);
            }
            ExprKind::FieldAccess(base, _) => Self::collect_expr_reads(base, out),
            ExprKind::MethodCall(base, _, args) => {
                Self::collect_expr_reads(base, out);
                for a in args { Self::collect_expr_reads(a, out); }
            }
            ExprKind::FunctionCall(_, args) => {
                for a in args { Self::collect_expr_reads(a, out); }
            }
            ExprKind::Ternary(cond, then_e, else_e) => {
                Self::collect_expr_reads(cond, out);
                Self::collect_expr_reads(then_e, out);
                Self::collect_expr_reads(else_e, out);
            }
            ExprKind::Match(scrut, arms) => {
                Self::collect_expr_reads(scrut, out);
                for arm in arms { Self::collect_stmt_reads(&arm.body, out); }
            }
            ExprKind::ExprMatch(scrut, arms) => {
                Self::collect_expr_reads(scrut, out);
                for arm in arms { Self::collect_expr_reads(&arm.value, out); }
            }
            _ => {}
        }
    }

    /// Collect all identifier names read in comb statements.
    fn collect_comb_stmt_reads(stmts: &[CombStmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                CombStmt::Assign(a) => Self::collect_expr_reads(&a.value, out),
                CombStmt::IfElse(ie) => {
                    Self::collect_expr_reads(&ie.cond, out);
                    Self::collect_comb_stmt_reads(&ie.then_stmts, out);
                    Self::collect_comb_stmt_reads(&ie.else_stmts, out);
                }
                CombStmt::MatchExpr(m) => {
                    Self::collect_expr_reads(&m.scrutinee, out);
                    for arm in &m.arms { Self::collect_stmt_reads(&arm.body, out); }
                }
                CombStmt::Log(l) => {
                    for arg in &l.args { Self::collect_expr_reads(arg, out); }
                }
            }
        }
    }

    /// Collect all target names assigned in comb statements.
    fn collect_comb_stmt_targets(stmts: &[CombStmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                CombStmt::Assign(a) => { out.insert(a.target.name.clone()); }
                CombStmt::IfElse(ie) => {
                    Self::collect_comb_stmt_targets(&ie.then_stmts, out);
                    Self::collect_comb_stmt_targets(&ie.else_stmts, out);
                }
                CombStmt::MatchExpr(m) => {
                    for arm in &m.arms { Self::collect_stmt_targets(&arm.body, out); }
                }
                CombStmt::Log(_) => {}
            }
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

        // Every state must have at least one transition (a state with zero
        // transitions is a dead end — the FSM can never leave it).
        // However, a catch-all `transition to Self when true` is NOT required;
        // the codegen emits `state_next = state_r` as the default hold.
        for sb in &f.states {
            if sb.transitions.is_empty() {
                self.errors.push(CompileError::general(
                    &format!("state `{}` has no transitions (dead-end state)", sb.name.name),
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

    // ── Synchronizer ─────────────────────────────────────────────────────────

    fn check_synchronizer(&mut self, s: &SynchronizerDecl) {
        self.check_pascal_case(&s.name);
        for p in &s.params {
            self.check_upper_snake(&p.name);
        }
        for p in &s.ports {
            self.check_snake_case(&p.name);
        }

        // Must have exactly two clock ports from different domains
        let clk_ports: Vec<(&Ident, &Ident)> = s.ports.iter()
            .filter_map(|p| if let TypeExpr::Clock(domain) = &p.ty { Some((&p.name, domain)) } else { None })
            .collect();
        if clk_ports.len() != 2 {
            self.errors.push(CompileError::general(
                &format!("synchronizer `{}` must have exactly 2 Clock<Domain> ports (source and destination)", s.name.name),
                s.name.span,
            ));
        } else if clk_ports[0].1.name == clk_ports[1].1.name {
            self.errors.push(CompileError::general(
                &format!("synchronizer `{}` has two clock ports in the same domain `{}`; use different domains", s.name.name, clk_ports[0].1.name),
                s.name.span,
            ));
        }

        // Must have data_in and data_out ports
        let port_names: Vec<&str> = s.ports.iter().map(|p| p.name.name.as_str()).collect();
        for req in &["data_in", "data_out"] {
            if !port_names.contains(req) {
                self.errors.push(CompileError::general(
                    &format!("synchronizer `{}` is missing required port `{req}`", s.name.name),
                    s.name.span,
                ));
            }
        }

        // STAGES param must be >= 2
        if let Some(stages_param) = s.params.iter().find(|p| p.name.name == "STAGES") {
            if let Some(ref default) = stages_param.default {
                if let ExprKind::Literal(LitKind::Dec(v)) = &default.kind {
                    if *v < 2 {
                        self.errors.push(CompileError::general(
                            &format!("synchronizer `{}`: STAGES must be >= 2 (got {})", s.name.name, v),
                            stages_param.name.span,
                        ));
                    }
                }
            }
        }

        // Kind-specific checks
        if let Some(data_in) = s.ports.iter().find(|p| p.name.name == "data_in") {
            let is_single_bit = match &data_in.ty {
                TypeExpr::Bool | TypeExpr::Bit => true,
                _ => false,
            };

            match s.kind {
                SyncKind::Ff if !is_single_bit => {
                    self.warnings.push(CompileWarning {
                        message: format!(
                            "synchronizer `{}`: `kind ff` on multi-bit data is unsafe — \
                             consider `kind gray` (for counters) or `kind handshake` (for arbitrary data)",
                            s.name.name
                        ),
                        span: s.name.span,
                    });
                }
                SyncKind::Reset if !is_single_bit => {
                    self.errors.push(CompileError::general(
                        &format!("synchronizer `{}`: `kind reset` requires single-bit (Bool) data ports", s.name.name),
                        data_in.span,
                    ));
                }
                SyncKind::Pulse if !is_single_bit => {
                    self.errors.push(CompileError::general(
                        &format!("synchronizer `{}`: `kind pulse` requires single-bit (Bool) data ports", s.name.name),
                        data_in.span,
                    ));
                }
                _ => {}
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
        use crate::ast::ArbiterPolicy;
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
        // Validate latency
        if a.latency < 1 {
            self.errors.push(CompileError::general(
                "arbiter latency must be >= 1",
                a.name.span,
            ));
        }
        // Validate hook for custom policy
        if let ArbiterPolicy::Custom(ref fn_ident) = a.policy {
            if a.hook.is_none() {
                self.errors.push(CompileError::general(
                    &format!("custom policy `{}` requires a `hook grant_select` declaration", fn_ident.name),
                    fn_ident.span,
                ));
                return;
            }
            let hook = a.hook.as_ref().unwrap();
            // Verify the hook's bound function name matches the policy name
            if hook.fn_name.name != fn_ident.name {
                self.errors.push(CompileError::general(
                    &format!("hook function `{}` does not match policy name `{}`", hook.fn_name.name, fn_ident.name),
                    hook.fn_name.span,
                ));
            }
            // Verify the function exists in the compilation unit
            let fn_exists = self.source.items.iter().any(|item| {
                if let crate::ast::Item::Function(f) = item {
                    f.name.name == fn_ident.name
                } else {
                    false
                }
            });
            if !fn_exists {
                self.errors.push(CompileError::general(
                    &format!("function `{}` not found", fn_ident.name),
                    fn_ident.span,
                ));
            }
            // Verify hook argument bindings reference declared ports or params
            let port_names: Vec<&str> = a.ports.iter().map(|p| p.name.name.as_str()).collect();
            let param_names: Vec<&str> = a.params.iter().map(|p| p.name.name.as_str()).collect();
            let hook_param_names: Vec<&str> = hook.params.iter().map(|p| p.name.name.as_str()).collect();
            for arg in &hook.fn_args {
                if !hook_param_names.contains(&arg.name.as_str())
                    && !port_names.contains(&arg.name.as_str())
                    && !param_names.contains(&arg.name.as_str())
                {
                    self.errors.push(CompileError::general(
                        &format!("hook argument `{}` is not a hook parameter, port, or param", arg.name),
                        arg.span,
                    ));
                }
            }
        } else if a.hook.is_some() {
            self.warnings.push(CompileWarning {
                message: "hook is ignored for built-in arbiter policies".to_string(),
                span: a.hook.as_ref().unwrap().span,
            });
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

    fn check_template(&mut self, t: &crate::ast::TemplateDecl) {
        self.check_pascal_case(&t.name);
        for p in &t.params {
            self.check_upper_snake(&p.name);
        }
        for p in &t.ports {
            self.check_snake_case(&p.name);
        }
        for pa in &t.port_arrays {
            self.check_snake_case(&pa.name);
            for s in &pa.signals {
                self.check_snake_case(&s.name);
            }
        }
        for h in &t.hooks {
            self.check_snake_case(&h.name);
        }
    }

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
