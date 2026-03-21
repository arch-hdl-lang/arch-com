use crate::ast::*;
use crate::diagnostics::CompileWarning;
use crate::lexer::Span;
use crate::resolve::{Symbol, SymbolTable};
use crate::typecheck::enum_width;

pub struct Codegen<'a> {
    pub symbols: &'a SymbolTable,
    pub source: &'a SourceFile,
    out: String,
    indent: usize,
    pub warnings: Vec<CompileWarning>,
    /// Comments extracted from the original source (byte span, text).
    comments: Vec<(Span, String)>,
    /// Cursor into `comments` — advanced as items are emitted.
    comment_idx: usize,
    /// Functions collected from the current file; emitted inside each module body.
    pending_functions: Vec<FunctionDecl>,
    /// Maps call-site span.start → overload index (for overloaded functions only).
    overload_map: std::collections::HashMap<usize, usize>,
}

impl<'a> Codegen<'a> {
    pub fn new(
        symbols: &'a SymbolTable,
        source: &'a SourceFile,
        overload_map: std::collections::HashMap<usize, usize>,
    ) -> Self {
        Self {
            symbols,
            source,
            out: String::new(),
            indent: 0,
            warnings: Vec::new(),
            comments: Vec::new(),
            comment_idx: 0,
            pending_functions: Vec::new(),
            overload_map,
        }
    }

    /// Emit all pending comments whose byte offset is before `pos`.
    fn emit_comments_before(&mut self, pos: usize) {
        while self.comment_idx < self.comments.len()
            && self.comments[self.comment_idx].0.start < pos
        {
            let text = self.comments[self.comment_idx].1.clone();
            self.line(&text);
            self.comment_idx += 1;
        }
    }

    /// Attach extracted source comments so they are interleaved with the output.
    pub fn with_comments(mut self, comments: Vec<(Span, String)>) -> Self {
        self.comments = comments;
        self
    }

    pub fn generate(mut self) -> String {
        let items: &[Item] = self.source.items.as_slice();
        self.generate_items(items)
    }

    /// Generate SV for a specific subset of items (used for per-file output).
    pub fn generate_items(&mut self, items: &[Item]) -> String {
        self.out.clear();
        self.comment_idx = 0;
        // Pre-collect all functions so they can be emitted inside each module.
        self.pending_functions = items.iter()
            .filter_map(|i| if let Item::Function(f) = i { Some(f) } else { None })
            .cloned()
            .collect();
        for item in items {
            self.emit_comments_before(item.span().start);
            match item {
                Item::Domain(d) => self.emit_domain(d),
                Item::Struct(s) => self.emit_struct(s),
                Item::Enum(e) => self.emit_enum(e),
                Item::Module(m) => self.emit_module(m),
                Item::Fsm(f) => self.emit_fsm(f),
                Item::Fifo(f) => self.emit_fifo(f),
                Item::Ram(r) => self.emit_ram(r),
                Item::Counter(c) => self.emit_counter(c),
                Item::Arbiter(a) => self.emit_arbiter(a),
                Item::Regfile(r) => self.emit_regfile(r),
                Item::Pipeline(p) => self.emit_pipeline(p),
                Item::Function(_) => {} // emitted inside modules
                Item::Linklist(l) => self.emit_linklist(l),
            }
        }
        // Flush any trailing comments after the last item.
        let end = usize::MAX;
        self.emit_comments_before(end);
        std::mem::take(&mut self.out)
    }

    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn emit_domain(&mut self, d: &DomainDecl) {
        self.line(&format!("// domain {}", d.name.name));
        for field in &d.fields {
            self.line(&format!("//   {}: {}", field.name.name, self.emit_expr_str(&field.value)));
        }
        self.line("");
    }

    /// Compute a short tag string for a TypeExpr used in mangled function names.
    /// `UInt<8>` → "8", `SInt<16>` → "s16", `Bool` → "b", etc.
    fn type_mangle_tag(te: &TypeExpr) -> String {
        match te {
            TypeExpr::UInt(e) => Self::expr_simple_str(e),
            TypeExpr::SInt(e) => format!("s{}", Self::expr_simple_str(e)),
            TypeExpr::Bool => "b".to_string(),
            TypeExpr::Bit  => "1".to_string(),
            TypeExpr::Named(n) => n.name.clone(),
            _ => "x".to_string(),
        }
    }

    fn expr_simple_str(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Literal(LitKind::Dec(n)) => n.to_string(),
            ExprKind::Literal(LitKind::Hex(n)) => n.to_string(),
            _ => "x".to_string(),
        }
    }

    /// Return the SV name for a function overload.  When a name has multiple overloads,
    /// mangle as `Name_W1_W2` using the declared arg type widths (e.g. `Xtime_8`).
    fn sv_function_name(&self, f: &FunctionDecl) -> String {
        if let Some((Symbol::Function(overloads), _)) = self.symbols.globals.get(&f.name.name) {
            if overloads.len() > 1 {
                let suffix: String = f.args.iter()
                    .map(|a| Self::type_mangle_tag(&a.ty))
                    .collect::<Vec<_>>()
                    .join("_");
                return format!("{}_{}", f.name.name, suffix);
            }
        }
        f.name.name.clone()
    }

    fn emit_function(&mut self, f: &FunctionDecl) {
        let sv_name = self.sv_function_name(f);
        let ret_str = self.emit_type_str(&f.ret_ty);
        let args_str: Vec<String> = f.args.iter()
            .map(|a| format!("input {} {}", self.emit_type_str(&a.ty), a.name.name))
            .collect();
        self.line(&format!(
            "function automatic {} {}({});",
            ret_str,
            sv_name,
            args_str.join(", ")
        ));
        self.indent += 1;
        for item in &f.body {
            match item {
                FunctionBodyItem::Let(l) => {
                    let ty_str = if let Some(ann) = &l.ty {
                        self.emit_type_str(ann)
                    } else {
                        "logic".to_string()
                    };
                    let val = self.emit_expr_str(&l.value);
                    self.line(&format!("{} {} = {};", ty_str, l.name.name, val));
                }
                FunctionBodyItem::Return(expr) => {
                    let val = self.emit_expr_str(expr);
                    self.line(&format!("return {};", val));
                }
            }
        }
        self.indent -= 1;
        self.line("endfunction");
        self.line("");
    }

    fn emit_struct(&mut self, s: &StructDecl) {
        // SV packed structs are MSB-first: first field listed = most significant bits.
        // Fields are reversed so the first ARCH field occupies the LSBs (C-style layout).
        self.line(&format!("typedef struct packed {{ // fields: LSB→MSB (reverse of declaration order)"));
        self.indent += 1;
        for field in s.fields.iter().rev() {
            let ty_str = self.emit_type_str(&field.ty);
            self.line(&format!("{} {};", ty_str, field.name.name));
        }
        self.indent -= 1;
        self.line(&format!("}} {};", s.name.name));
        self.line("");
    }

    fn emit_enum(&mut self, e: &EnumDecl) {
        let width = enum_width(e.variants.len());
        let variants: Vec<String> = e
            .variants
            .iter()
            .enumerate()
            .map(|(i, v)| format!("{} = {}'d{}", v.name.to_uppercase(), width, i))
            .collect();
        self.line(&format!(
            "typedef enum logic [{}:0] {{",
            width.saturating_sub(1)
        ));
        self.indent += 1;
        for (i, v) in variants.iter().enumerate() {
            if i < variants.len() - 1 {
                self.line(&format!("{v},"));
            } else {
                self.line(v);
            }
        }
        self.indent -= 1;
        self.line(&format!("}} {};", e.name.name));
        self.line("");
    }

    fn emit_module(&mut self, m: &ModuleDecl) {
        // Module header with parameters
        if m.params.is_empty() {
            self.out.push_str(&format!("module {} (\n", m.name.name));
        } else {
            self.out.push_str(&format!("module {} #(\n", m.name.name));
            self.indent += 1;
            for (i, p) in m.params.iter().enumerate() {
                let default_str = if let Some(d) = &p.default {
                    format!(" = {}", self.emit_expr_str(d))
                } else {
                    String::new()
                };
                let comma = if i < m.params.len() - 1 { "," } else { "" };
                self.line(&format!("parameter int {}{}{}", p.name.name, default_str, comma));
            }
            self.indent -= 1;
            self.line(") (");
        }

        // Ports
        self.indent += 1;
        for (i, p) in m.ports.iter().enumerate() {
            let dir = match p.direction {
                Direction::In => "input",
                Direction::Out => "output",
            };
            let ty_str = self.emit_port_type_str(&p.ty);
            let comma = if i < m.ports.len() - 1 { "," } else { "" };
            self.line(&format!("{} {} {}{}", dir, ty_str, p.name.name, comma));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");

        self.indent += 1;

        // Emit any functions defined in the same file as local `function automatic` declarations.
        let fns = std::mem::take(&mut self.pending_functions);
        for f in &fns {
            self.emit_function(f);
        }
        self.pending_functions = fns;

        // If any log() statements exist in this module, emit the per-module verbosity variable.
        // Override at simulation: +arch_verbosity=N on the simulator command line.
        if Self::module_has_log(&m.body) {
            self.line("integer _arch_verbosity = 1; // 0=Always 1=Low 2=Medium 3=High 4=Full 5=Debug");
            self.line("initial void'($value$plusargs(\"arch_verbosity=%0d\", _arch_verbosity));");
            self.line("");
        }

        // Collect names already declared as ports, regs, or lets so we can
        // auto-declare inst output wires that aren't otherwise declared.
        let mut declared_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for p in &m.ports { declared_names.insert(p.name.name.clone()); }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => { declared_names.insert(r.name.name.clone()); }
                ModuleBodyItem::LetBinding(l) => { declared_names.insert(l.name.name.clone()); }
                ModuleBodyItem::PipeRegDecl(p) => {
                    declared_names.insert(p.name.name.clone());
                    for i in 0..p.stages.saturating_sub(1) {
                        declared_names.insert(format!("{}_stg{}", p.name.name, i + 1));
                    }
                }
                _ => {}
            }
        }

        // Single pass in source order; interleave comments by byte position.
        // We need a clone of m to satisfy the borrow checker when calling
        // emit_reg_block (which takes &ModuleDecl) while also mutating self.
        let body_items: Vec<ModuleBodyItem> = m.body.clone();
        let m_clone = m.clone();
        for item in &body_items {
            self.emit_comments_before(item.span().start);
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&r.ty);
                    let init_str = self.emit_expr_str(&r.init);
                    if arr_suffix.is_empty() {
                        self.line(&format!("{} {} = {};", ty_str, r.name.name, init_str));
                    } else {
                        self.line(&format!("{} {}{} = '{{default: {}}};", ty_str, r.name.name, arr_suffix, init_str));
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    let val_str = self.emit_expr_str(&l.value);
                    if let Some(ty) = &l.ty {
                        let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                        self.line(&format!("{} {}{};", ty_str, l.name.name, arr_suffix));
                        self.line(&format!("assign {} = {};", l.name.name, val_str));
                    } else {
                        self.line(&format!("logic {};", l.name.name));
                        self.line(&format!("assign {} = {};", l.name.name, val_str));
                    }
                }
                ModuleBodyItem::CombBlock(cb) => self.emit_comb_block(cb),
                ModuleBodyItem::RegBlock(rb) => self.emit_reg_block(rb, &m_clone),
                ModuleBodyItem::Inst(inst) => {
                    // Auto-declare output wires that aren't already declared
                    self.emit_inst_output_wire_decls(inst, &declared_names);
                    self.emit_inst(inst);
                }
                ModuleBodyItem::PipeRegDecl(p) => {
                    self.emit_pipe_reg(p, &m_clone);
                }
                ModuleBodyItem::Generate(_) => {} // expanded before codegen
            }
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Find the SV type string for a signal by looking up ports, regs, and let bindings.
    fn find_signal_sv_type(&self, name: &str, m: &ModuleDecl) -> String {
        // Check ports
        for p in &m.ports {
            if p.name.name == name {
                return self.emit_type_str(&p.ty);
            }
        }
        // Check reg decls
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) if r.name.name == name => {
                    return self.emit_type_str(&r.ty);
                }
                ModuleBodyItem::LetBinding(l) if l.name.name == name => {
                    if let Some(ty) = &l.ty {
                        return self.emit_type_str(ty);
                    }
                }
                ModuleBodyItem::PipeRegDecl(p) if p.name.name == name => {
                    // Recursively resolve from source
                    return self.find_signal_sv_type(&p.source.name, m);
                }
                _ => {}
            }
        }
        "logic".to_string() // fallback
    }

    fn emit_pipe_reg(&mut self, p: &PipeRegDecl, m: &ModuleDecl) {
        let ty_str = self.find_signal_sv_type(&p.source.name, m);

        // Build chain of names: stg1, stg2, ..., final name
        let mut chain: Vec<String> = Vec::new();
        for i in 0..p.stages {
            if i == p.stages - 1 {
                chain.push(p.name.name.clone());
            } else {
                chain.push(format!("{}_stg{}", p.name.name, i + 1));
            }
        }

        // Declare all as regs
        for name in &chain {
            self.line(&format!("{} {};", ty_str, name));
        }

        // Find clock and reset from module ports
        let clk_name = m.ports.iter()
            .find(|port| matches!(&port.ty, TypeExpr::Clock(_)))
            .map(|port| port.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());

        let rst_name = m.ports.iter()
            .find(|port| matches!(&port.ty, TypeExpr::Reset(..)))
            .map(|port| port.name.name.clone());

        self.line(&format!("always_ff @(posedge {}) begin", clk_name));
        self.indent += 1;

        if let Some(ref rst) = rst_name {
            self.line(&format!("if ({}) begin", rst));
            self.indent += 1;
            for name in &chain {
                self.line(&format!("{} <= '0;", name));
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }

        let mut prev = p.source.name.clone();
        for name in &chain {
            self.line(&format!("{} <= {};", name, prev));
            prev = name.clone();
        }

        if rst_name.is_some() {
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("end");
    }

    fn emit_comb_block(&mut self, cb: &CombBlock) {
        // Simple assign form only when every statement is a plain assign with
        // no match-expression RHS (those need always_comb for the case block).
        let all_simple = cb.stmts.iter().all(|s| match s {
            CombStmt::Assign(a) => !matches!(a.value.kind, ExprKind::ExprMatch(..)),
            _ => false,
        });
        if all_simple {
            for stmt in &cb.stmts {
                if let CombStmt::Assign(a) = stmt {
                    let val = self.emit_expr_str(&a.value);
                    self.line(&format!("assign {} = {};", a.target.name, val));
                }
            }
        } else {
            self.line("always_comb begin");
            self.indent += 1;
            for stmt in &cb.stmts {
                self.emit_comb_stmt(stmt);
            }
            self.indent -= 1;
            self.line("end");
        }
    }

    /// Return true if any log() statement exists anywhere in the module body.
    fn module_has_log(body: &[ModuleBodyItem]) -> bool {
        body.iter().any(|item| match item {
            ModuleBodyItem::RegBlock(rb) => rb.stmts.iter().any(Self::stmt_has_log),
            ModuleBodyItem::CombBlock(cb) => cb.stmts.iter().any(Self::comb_stmt_has_log),
            _ => false,
        })
    }

    fn stmt_has_log(s: &Stmt) -> bool {
        match s {
            Stmt::Log(_) => true,
            Stmt::IfElse(ie) => ie.then_stmts.iter().any(Self::stmt_has_log)
                || ie.else_stmts.iter().any(Self::stmt_has_log),
            Stmt::Match(m) => m.arms.iter().any(|a| a.body.iter().any(Self::stmt_has_log)),
            Stmt::Assign(_) => false,
        }
    }

    fn comb_stmt_has_log(s: &CombStmt) -> bool {
        match s {
            CombStmt::Log(_) => true,
            CombStmt::IfElse(ie) => ie.then_stmts.iter().any(Self::comb_stmt_has_log)
                || ie.else_stmts.iter().any(Self::comb_stmt_has_log),
            CombStmt::Assign(_) | CombStmt::MatchExpr(_) => false,
        }
    }

    /// Emit a `log(...)` statement as an `if`-guarded `$display`.
    fn emit_log_stmt(&mut self, l: &LogStmt) {
        let args_str: String = l.args.iter()
            .map(|a| format!(", {}", self.emit_expr_str(a)))
            .collect();
        let display = format!(
            "$display(\"[%0t][{}][{}] {}\", $time{});",
            l.level.name(), l.tag, l.fmt, args_str
        );
        if l.level == LogLevel::Always {
            self.line(&display);
        } else {
            self.line(&format!("if (_arch_verbosity >= {}) {}", l.level.value(), display));
        }
    }

    fn emit_comb_stmt(&mut self, stmt: &CombStmt) {
        match stmt {
            CombStmt::Assign(a) => {
                // Match-expression RHS: emit as a case block for readability
                if let ExprKind::ExprMatch(scrutinee, arms) = &a.value.kind {
                    let s = self.emit_expr_str(scrutinee);
                    let target = a.target.name.clone();
                    self.line(&format!("case ({s})"));
                    self.indent += 1;
                    for arm in arms {
                        let pat = match &arm.pattern {
                            Pattern::Wildcard => "default".to_string(),
                            Pattern::Ident(id) if id.name == "_" => "default".to_string(),
                            Pattern::Literal(e) => self.emit_expr_str(e),
                            Pattern::Ident(id) => id.name.clone(),
                            Pattern::EnumVariant(en, vr) => {
                                format!("{}__{}", en.name.to_uppercase(), vr.name.to_uppercase())
                            }
                        };
                        let val = self.emit_expr_str(&arm.value);
                        self.line(&format!("{pat}: {target} = {val};"));
                    }
                    self.indent -= 1;
                    self.line("endcase");
                } else {
                    let val = self.emit_expr_str(&a.value);
                    self.line(&format!("{} = {};", a.target.name, val));
                }
            }
            CombStmt::IfElse(ie) => {
                self.emit_comb_if_else(ie);
            }
            CombStmt::MatchExpr(m) => {
                let scrut = self.emit_expr_str(&m.scrutinee);
                self.line(&format!("case ({})", scrut));
                self.indent += 1;
                for arm in &m.arms {
                    let pat = self.emit_pattern(&arm.pattern);
                    self.line(&format!("{}: begin", pat));
                    self.indent += 1;
                    for s in &arm.body {
                        self.emit_reg_stmt_as_comb(s);
                    }
                    self.indent -= 1;
                    self.line("end");
                }
                self.indent -= 1;
                self.line("endcase");
            }
            CombStmt::Log(l) => { self.emit_log_stmt(l); }
        }
    }

    fn emit_comb_if_else(&mut self, ie: &CombIfElse) {
        self.emit_comb_if_else_inner(ie, false);
    }

    fn emit_comb_if_else_inner(&mut self, ie: &CombIfElse, is_chain: bool) {
        let cond = self.emit_expr_str(&ie.cond);
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("if ({}) begin", cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_comb_stmt(s);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let CombStmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_comb_if_else_inner(nested, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_comb_stmt(s);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    fn emit_reg_stmt_as_comb(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign(a) => {
                let target = self.emit_expr_str(&a.target);
                let val = self.emit_expr_str(&a.value);
                self.line(&format!("{} = {};", target, val));
            }
            _ => {} // MVP: basic case only
        }
    }

    fn emit_reg_block(&mut self, rb: &RegBlock, m: &ModuleDecl) {
        let clk_edge = match rb.clock_edge {
            ClockEdge::Rising => "posedge",
            ClockEdge::Falling => "negedge",
        };

        // Collect all assigned register names in this block
        let mut assigned = std::collections::BTreeSet::new();
        Self::collect_assigned_roots(&rb.stmts, &mut assigned);

        // Look up reset info for each assigned register from its RegDecl
        let reg_decls: Vec<&RegDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i { Some(r) } else { None })
            .collect();

        // Resolve reset info: (rst_name, is_async, is_low) for registers that have reset
        struct ResolvedReset {
            signal: String,
            is_async: bool,
            is_low: bool,
        }
        let mut reset_info: Option<ResolvedReset> = Option::None;
        let mut resets: Vec<(String, String)> = Vec::new(); // (reg_name, init_str)
        for name in &assigned {
            if name.is_empty() { continue; }
            if let Some(rd) = reg_decls.iter().find(|r| r.name.name == *name) {
                let resolved = self.resolve_reg_reset(&rd.reset, m);
                if let Some((rst_sig, is_async, is_low)) = resolved {
                    if reset_info.is_none() {
                        reset_info = Some(ResolvedReset {
                            signal: rst_sig.clone(),
                            is_async,
                            is_low,
                        });
                    }
                    let init = self.emit_expr_str(&rd.init);
                    resets.push((name.clone(), init));
                }
            }
        }

        // Emit sensitivity list
        if let Some(ref ri) = reset_info {
            if ri.is_async {
                let rst_edge = if ri.is_low { "negedge" } else { "posedge" };
                self.line(&format!(
                    "always_ff @({clk_edge} {} or {rst_edge} {}) begin",
                    rb.clock.name, ri.signal
                ));
            } else {
                self.line(&format!("always_ff @({clk_edge} {}) begin", rb.clock.name));
            }
        } else {
            self.line(&format!("always_ff @({clk_edge} {}) begin", rb.clock.name));
        }
        self.indent += 1;

        if let Some(ref ri) = reset_info {
            let rst_cond_str = if ri.is_low {
                format!("(!{})", ri.signal)
            } else {
                ri.signal.clone()
            };

            // Build set of register names that have reset
            let reset_reg_names: std::collections::BTreeSet<String> =
                resets.iter().map(|(n, _)| n.clone()).collect();

            // Partition top-level statements: those that only assign to reset
            // registers go inside the if/else guard; those that only assign
            // to no-reset registers go outside (always execute).
            let mut guarded_stmts = Vec::new();
            let mut unguarded_stmts = Vec::new();
            for stmt in &rb.stmts {
                let mut stmt_roots = std::collections::BTreeSet::new();
                Self::collect_assigned_roots(std::slice::from_ref(stmt), &mut stmt_roots);
                let any_reset = stmt_roots.iter().any(|n| reset_reg_names.contains(n));
                if any_reset {
                    guarded_stmts.push(stmt);
                } else {
                    unguarded_stmts.push(stmt);
                }
            }

            self.line(&format!("if ({rst_cond_str}) begin"));
            self.indent += 1;
            for (name, init) in &resets {
                let is_vec = reg_decls.iter().any(|r| r.name.name == *name && matches!(&r.ty, TypeExpr::Vec(_, _)));
                if is_vec {
                    self.line(&format!("{name} <= '{{default: {init}}};"));
                } else {
                    self.line(&format!("{name} <= {init};"));
                }
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            for stmt in &guarded_stmts {
                self.emit_reg_stmt(stmt);
            }
            self.indent -= 1;
            self.line("end");

            // Emit no-reset statements outside the if/else guard
            for stmt in &unguarded_stmts {
                self.emit_reg_stmt(stmt);
            }
        } else {
            // No registers with reset — emit body directly
            for stmt in &rb.stmts {
                self.emit_reg_stmt(stmt);
            }
        }

        self.indent -= 1;
        self.line("end");
    }

    /// Resolve a register's reset info: returns Some((signal_name, is_async, is_low))
    /// or None if the register has no reset.
    fn resolve_reg_reset(&self, reset: &RegReset, m: &ModuleDecl) -> Option<(String, bool, bool)> {
        match reset {
            RegReset::None => Option::None,
            RegReset::Explicit(signal, kind, level) => {
                Some((
                    signal.name.clone(),
                    *kind == ResetKind::Async,
                    *level == ResetLevel::Low,
                ))
            }
            RegReset::Inherit(signal) => {
                // Look up the port declaration to get sync/async and polarity
                let port = m.ports.iter().find(|p| p.name.name == signal.name);
                if let Some(port) = port {
                    if let TypeExpr::Reset(kind, level) = &port.ty {
                        Some((
                            signal.name.clone(),
                            *kind == ResetKind::Async,
                            *level == ResetLevel::Low,
                        ))
                    } else {
                        // Port exists but isn't a Reset type — treat as no reset
                        Option::None
                    }
                } else {
                    // Signal not found as port — shouldn't happen after typecheck
                    Option::None
                }
            }
        }
    }

    /// Collect root signal names from all LHS assignments in a statement list.
    fn collect_assigned_roots(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    out.insert(Self::expr_root_name(&a.target));
                }
                Stmt::IfElse(ie) => {
                    Self::collect_assigned_roots(&ie.then_stmts, out);
                    Self::collect_assigned_roots(&ie.else_stmts, out);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        Self::collect_assigned_roots(&arm.body, out);
                    }
                }
                Stmt::Log(_) => {}
            }
        }
    }

    fn expr_root_name(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::FieldAccess(base, _) => Self::expr_root_name(base),
            ExprKind::Index(base, _) => Self::expr_root_name(base),
            _ => String::new(),
        }
    }

    /// Extract reset info from a port list: (name, is_async, is_low).
    /// Returns ("rst", false, false) as defaults if no Reset port found.
    fn extract_reset_info(ports: &[PortDecl]) -> (String, bool, bool) {
        let rst_port = ports.iter().find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)));
        let rst_name = rst_port.map(|p| p.name.name.clone()).unwrap_or_else(|| "rst".to_string());
        let (is_async, is_low) = rst_port.map(|p| {
            if let TypeExpr::Reset(kind, level) = &p.ty {
                (*kind == ResetKind::Async, *level == ResetLevel::Low)
            } else {
                (false, false)
            }
        }).unwrap_or((false, false));
        (rst_name, is_async, is_low)
    }

    /// Build the sensitivity list string for an always_ff block.
    fn ff_sensitivity(clk: &str, rst: &str, is_async: bool, is_low: bool) -> String {
        if is_async {
            let rst_edge = if is_low { "negedge" } else { "posedge" };
            format!("posedge {clk} or {rst_edge} {rst}")
        } else {
            format!("posedge {clk}")
        }
    }

    /// Build the reset condition string (e.g. "rst" or "(!rst_n)").
    fn rst_condition(rst: &str, is_low: bool) -> String {
        if is_low {
            format!("(!{rst})")
        } else {
            rst.to_string()
        }
    }

    fn emit_reg_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign(a) => {
                let target = self.emit_expr_str(&a.target);
                let val = self.emit_expr_str(&a.value);
                self.line(&format!("{} <= {};", target, val));
            }
            Stmt::IfElse(ie) => {
                self.emit_reg_if_else(ie);
            }
            Stmt::Match(m) => {
                let scrut = self.emit_expr_str(&m.scrutinee);
                self.line(&format!("case ({})", scrut));
                self.indent += 1;
                for arm in &m.arms {
                    let pat = self.emit_pattern(&arm.pattern);
                    self.line(&format!("{}: begin", pat));
                    self.indent += 1;
                    for s in &arm.body {
                        self.emit_reg_stmt(s);
                    }
                    self.indent -= 1;
                    self.line("end");
                }
                self.indent -= 1;
                self.line("endcase");
            }
            Stmt::Log(l) => { self.emit_log_stmt(l); }
        }
    }

    fn emit_reg_if_else(&mut self, ie: &IfElse) {
        self.emit_reg_if_else_inner(ie, false);
    }

    fn emit_reg_if_else_inner(&mut self, ie: &IfElse, is_chain: bool) {
        let cond = self.emit_expr_str(&ie.cond);
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("if ({}) begin", cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_reg_stmt(s);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_reg_if_else_inner(nested, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_reg_stmt(s);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    /// Auto-declare `logic` wires for inst output connections that reference
    /// names not already declared as ports, regs, or lets in the current module.
    /// The wire type is resolved from the source module's port definition.
    fn emit_inst_output_wire_decls(
        &mut self,
        inst: &InstDecl,
        declared: &std::collections::HashSet<String>,
    ) {
        // Look up the instantiated module's port info
        let module_ports = if let Some((Symbol::Module(info), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            info.ports.clone()
        } else if let Some((Symbol::Pipeline(info), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            info.ports.clone()
        } else {
            return;
        };

        for conn in &inst.connections {
            if conn.direction != ConnectDir::Output {
                continue;
            }
            if let ExprKind::Ident(target) = &conn.signal.kind {
                if declared.contains(target) {
                    continue;
                }
                // Find the port type from the module definition
                if let Some(port) = module_ports.iter().find(|p| p.name.name == conn.port_name.name) {
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&port.ty);
                    self.line(&format!("{} {}{};", ty_str, target, arr_suffix));
                } else {
                    self.line(&format!("logic {};", target));
                }
            }
        }
    }

    fn emit_inst(&mut self, inst: &InstDecl) {
        let mut parts = Vec::new();

        // Module name with params
        if inst.param_assigns.is_empty() {
            parts.push(format!("{} {} (", inst.module_name.name, inst.name.name));
        } else {
            let params: Vec<String> = inst
                .param_assigns
                .iter()
                .map(|p| format!(".{}({})", p.name.name, self.emit_expr_str(&p.value)))
                .collect();
            parts.push(format!(
                "{} #({}) {} (",
                inst.module_name.name,
                params.join(", "),
                inst.name.name,
            ));
        }

        let connections: Vec<String> = inst
            .connections
            .iter()
            .map(|c| format!(".{}({})", c.port_name.name, self.emit_expr_str(&c.signal)))
            .collect();

        self.line(&parts[0]);
        self.indent += 1;
        for (i, conn) in connections.iter().enumerate() {
            if i < connections.len() - 1 {
                self.line(&format!("{},", conn));
            } else {
                self.line(conn);
            }
        }
        self.indent -= 1;
        self.line(");");
    }

    fn emit_pipeline_inst(
        &mut self,
        inst: &InstDecl,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        let header = if inst.param_assigns.is_empty() {
            format!("{} {} (", inst.module_name.name, inst.name.name)
        } else {
            let params: Vec<String> = inst
                .param_assigns
                .iter()
                .map(|p| format!(".{}({})", p.name.name, self.emit_expr_str(&p.value)))
                .collect();
            format!(
                "{} #({}) {} (",
                inst.module_name.name,
                params.join(", "),
                inst.name.name,
            )
        };

        let connections: Vec<String> = inst
            .connections
            .iter()
            .map(|c| {
                let sig = self.emit_pipeline_stage_expr_str(
                    &c.signal, current_prefix, current_stage_idx,
                    stage_names, stage_regs, port_names,
                );
                format!(".{}({})", c.port_name.name, sig)
            })
            .collect();

        self.line(&header);
        self.indent += 1;
        for (i, conn) in connections.iter().enumerate() {
            if i < connections.len() - 1 {
                self.line(&format!("{},", conn));
            } else {
                self.line(conn);
            }
        }
        self.indent -= 1;
        self.line(");");
    }

    // ── FSM ───────────────────────────────────────────────────────────────────

    fn emit_fsm(&mut self, f: &FsmDecl) {
        let n = f.name.name.clone();
        let n_states = f.state_names.len();
        let state_bits = enum_width(n_states);

        // ── Module header ────────────────────────────────────────────────────
        if f.params.is_empty() {
            self.line(&format!("module {n} ("));
        } else {
            self.line(&format!("module {n} #("));
            self.indent += 1;
            for (i, p) in f.params.iter().enumerate() {
                let default_str = if let Some(d) = &p.default {
                    format!(" = {}", self.emit_expr_str(d))
                } else {
                    String::new()
                };
                let comma = if i < f.params.len() - 1 { "," } else { "" };
                self.line(&format!("parameter int {}{}{}", p.name.name, default_str, comma));
            }
            self.indent -= 1;
            self.line(") (");
        }
        self.indent += 1;
        for (i, p) in f.ports.iter().enumerate() {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty = self.emit_port_type_str(&p.ty);
            let comma = if i < f.ports.len() - 1 { "," } else { "" };
            self.line(&format!("{dir} {ty} {}{comma}", p.name.name));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // ── State type ───────────────────────────────────────────────────────
        self.line(&format!("typedef enum logic [{}:0] {{", state_bits.saturating_sub(1)));
        self.indent += 1;
        for (i, sn) in f.state_names.iter().enumerate() {
            let comma = if i < f.state_names.len() - 1 { "," } else { "" };
            self.line(&format!("{} = {state_bits}'d{i}{comma}", sn.name.to_uppercase()));
        }
        self.indent -= 1;
        self.line(&format!("}} {n}_state_t;"));
        self.line("");

        // ── State register ───────────────────────────────────────────────────
        self.line(&format!("{n}_state_t state_r, state_next;"));
        self.line("");

        // Identify clock and reset port names
        let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
        let clk_name = clk_port.map(|p| p.name.name.as_str()).unwrap_or("clk");
        let (rst_name, is_async, is_low) = Self::extract_reset_info(&f.ports);
        let ff_sens = Self::ff_sensitivity(clk_name, &rst_name, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst_name, is_low);

        // ── State register FF ────────────────────────────────────────────────
        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line(&format!("state_r <= {};", f.default_state.name.to_uppercase()));
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("state_r <= state_next;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Next-state logic ─────────────────────────────────────────────────
        self.line("always_comb begin");
        self.indent += 1;
        self.line("state_next = state_r; // hold by default");
        self.line("case (state_r)");
        self.indent += 1;
        for sb in &f.states {
            self.line(&format!("{}: begin", sb.name.name.to_uppercase()));
            self.indent += 1;
            let cond_strs: Vec<String> = sb.transitions.iter()
                .map(|tr| self.emit_expr_str(&tr.condition))
                .collect();
            // Single unconditional transition — emit plain assignment.
            if cond_strs.len() == 1 && (cond_strs[0] == "1'b1" || cond_strs[0] == "1") {
                self.line(&format!("state_next = {};",
                    sb.transitions[0].target.name.to_uppercase()));
            } else {
                for (i, tr) in sb.transitions.iter().enumerate() {
                    let kw = if i == 0 { "if" } else { "else if" };
                    self.line(&format!("{kw} ({}) state_next = {};",
                        cond_strs[i], tr.target.name.to_uppercase()));
                }
            }
            self.indent -= 1;
            self.line("end");
        }
        self.line("default: state_next = state_r;");
        self.indent -= 1;
        self.line("endcase");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Output logic ─────────────────────────────────────────────────────
        // Emit default zeros for all outputs
        let out_ports: Vec<&PortDecl> = f.ports.iter()
            .filter(|p| p.direction == Direction::Out)
            .collect();
        if !out_ports.is_empty() {
            self.line("always_comb begin");
            self.indent += 1;
            // Defaults
            for op in &out_ports {
                let default_str = if let Some(d) = &op.default {
                    self.emit_expr_str(d)
                } else {
                    "'0".to_string()
                };
                self.line(&format!("{} = {}; // default", op.name.name, default_str));
            }
            self.line("case (state_r)");
            self.indent += 1;
            for sb in &f.states {
                self.line(&format!("{}: begin", sb.name.name.to_uppercase()));
                self.indent += 1;
                for stmt in &sb.comb_stmts {
                    self.emit_comb_stmt(stmt);
                }
                self.indent -= 1;
                self.line("end");
            }
            self.line("default: ;");
            self.indent -= 1;
            self.line("endcase");
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Pipeline ──────────────────────────────────────────────────────────────

    fn emit_pipeline(&mut self, p: &PipelineDecl) {
        let n = &p.name.name;

        // ── Module header ────────────────────────────────────────────────────
        if p.params.is_empty() {
            self.out.push_str(&format!("module {} (\n", n));
        } else {
            self.out.push_str(&format!("module {} #(\n", n));
            self.indent += 1;
            for (i, param) in p.params.iter().enumerate() {
                let default_str = if let Some(d) = &param.default {
                    format!(" = {}", self.emit_expr_str(d))
                } else {
                    String::new()
                };
                let comma = if i < p.params.len() - 1 { "," } else { "" };
                self.line(&format!("parameter int {}{}{}", param.name.name, default_str, comma));
            }
            self.indent -= 1;
            self.line(") (");
        }

        self.indent += 1;
        for (i, port) in p.ports.iter().enumerate() {
            let dir = match port.direction {
                Direction::In => "input",
                Direction::Out => "output",
            };
            let ty_str = self.emit_port_type_str(&port.ty);
            let comma = if i < p.ports.len() - 1 { "," } else { "" };
            self.line(&format!("{} {} {}{}", dir, ty_str, port.name.name, comma));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");

        self.indent += 1;

        // Collect port names for name resolution
        let port_names: std::collections::HashSet<String> = p.ports.iter()
            .map(|pt| pt.name.name.clone())
            .collect();

        // Collect stage names (in order) and signal names per stage
        let stage_names: Vec<&str> = p.stages.iter().map(|s| s.name.name.as_str()).collect();

        // Build map: stage_name -> Vec<(signal_name, type_str, init_str)> for registers
        // Comb wire entries have init_str="" to distinguish from real registers.
        let mut stage_regs: Vec<Vec<(String, String, String)>> = Vec::new();
        for stage in &p.stages {
            let mut regs = Vec::new();
            for item in &stage.body {
                if let ModuleBodyItem::RegDecl(r) = item {
                    let ty_str = self.emit_logic_type_str(&r.ty);
                    let init_str = self.emit_expr_str(&r.init);
                    regs.push((r.name.name.clone(), ty_str, init_str));
                }
            }
            stage_regs.push(regs);
        }

        // ── Stage valid registers ────────────────────────────────────────────
        self.line("// ── Stage valid registers ──");
        for sn in &stage_names {
            self.line(&format!("logic {}_valid_r;", sn.to_lowercase()));
        }
        self.line("");

        // ── Collect comb wire declarations per stage ──────────────────────────
        // Scan comb blocks for assign targets that aren't ports or regs.
        // These need explicit `logic` declarations. Type is resolved from
        // assignment sources (register or cross-stage reference).
        // Comb wires are added to stage_regs with init_str="" so name rewriting
        // automatically prefixes them.
        for (si, stage) in p.stages.iter().enumerate() {
            let mut wires: Vec<(String, String)> = Vec::new();
            for item in &stage.body {
                if let ModuleBodyItem::CombBlock(cb) = item {
                    let targets = Self::collect_comb_targets(&cb.stmts);
                    for target in targets {
                        if port_names.contains(&target) {
                            continue;
                        }
                        if stage_regs[si].iter().any(|(rn, _, _)| rn == &target) {
                            continue;
                        }
                        let ty = Self::resolve_comb_wire_type(
                            &target, &cb.stmts, si, &stage_regs, &stage_names,
                        ).unwrap_or_else(|| "logic".to_string());
                        if !wires.iter().any(|(n, _)| n == &target) {
                            wires.push((target, ty));
                        }
                    }
                }
            }
            for (name, ty) in wires {
                stage_regs[si].push((name, ty, String::new())); // empty init = comb wire
            }

            // Collect inst output connection targets as wires.
            // Resolve type by finding the register this wire is assigned to in
            // the stage's seq block (e.g. `alu_result <= alu_out` → use alu_result's type).
            for item in &stage.body {
                if let ModuleBodyItem::Inst(inst) = item {
                    for conn in &inst.connections {
                        if conn.direction != ConnectDir::Output {
                            continue;
                        }
                        if let ExprKind::Ident(target) = &conn.signal.kind {
                            if port_names.contains(target) {
                                continue;
                            }
                            if stage_regs[si].iter().any(|(rn, _, _)| rn == target) {
                                continue;
                            }
                            // Find type from the register that reads this wire
                            let ty = Self::resolve_inst_wire_type_from_consumers(
                                target, &stage.body, &stage_regs[si],
                            ).unwrap_or_else(|| "logic".to_string());
                            stage_regs[si].push((target.clone(), ty, String::new()));
                        }
                    }
                }
            }
        }

        // ── Stage data registers ─────────────────────────────────────────────
        self.line("// ── Stage data registers ──");
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();
            for (sig_name, ty_str, init_str) in &stage_regs[si] {
                if !init_str.is_empty() {
                    // Real register with initial value
                    self.line(&format!("{} {}_{} = {};", ty_str, prefix, sig_name, init_str));
                } else {
                    // Comb wire (forwarding mux, etc.)
                    self.line(&format!("{} {}_{};", ty_str, prefix, sig_name));
                }
            }
        }
        self.line("");

        // ── Per-stage stall signals ──────────────────────────────────────────
        // Determine whether any stage or the pipeline has stall conditions.
        let has_per_stage_stall = p.stages.iter().any(|s| s.stall_cond.is_some());
        let has_global_stall = !p.stall_conds.is_empty();
        let has_any_stall = has_per_stage_stall || has_global_stall;

        if has_any_stall {
            self.line("// ── Stall signals ──");

            // Global stall (top-level `stall when`)
            if has_global_stall {
                let stall_parts: Vec<String> = p.stall_conds.iter()
                    .map(|s| self.emit_pipeline_expr_str(&s.condition, &stage_names, &stage_regs, &port_names))
                    .collect();
                self.line("logic pipeline_stall;");
                self.line(&format!("assign pipeline_stall = {};", stall_parts.join(" | ")));
            }

            // Per-stage stall wires: stall_N = local_stall_N || stall_{N+1}
            // (backpressure: downstream stall propagates upstream)
            // Last stage only has its local condition (no downstream).
            let n = p.stages.len();
            for si in 0..n {
                let prefix = stage_names[si].to_lowercase();
                self.line(&format!("logic {prefix}_stall;"));
            }

            // Build assigns in reverse order (last stage first)
            for si in (0..n).rev() {
                let prefix = stage_names[si].to_lowercase();
                let mut parts: Vec<String> = Vec::new();

                // Local stall condition from `stage X stall when <expr>`
                if let Some(ref cond) = p.stages[si].stall_cond {
                    parts.push(self.emit_pipeline_expr_str(cond, &stage_names, &stage_regs, &port_names));
                }

                // Global stall contributes to every stage
                if has_global_stall {
                    parts.push("pipeline_stall".to_string());
                }

                // Backpressure from downstream stage
                if si + 1 < n {
                    let next_prefix = stage_names[si + 1].to_lowercase();
                    parts.push(format!("{next_prefix}_stall"));
                }

                if parts.is_empty() {
                    self.line(&format!("assign {prefix}_stall = 1'b0;"));
                } else {
                    self.line(&format!("assign {prefix}_stall = {};", parts.join(" || ")));
                }
            }
            self.line("");
        }

        // ── Forward mux wires ────────────────────────────────────────────────
        for fwd in &p.forward_directives {
            let dest_str = self.emit_pipeline_expr_str(&fwd.dest, &stage_names, &stage_regs, &port_names);
            let src_str = self.emit_pipeline_expr_str(&fwd.source, &stage_names, &stage_regs, &port_names);
            let cond_str = self.emit_pipeline_expr_str(&fwd.condition, &stage_names, &stage_regs, &port_names);
            self.line(&format!("// Forward: {} from {} when {}", dest_str, src_str, cond_str));
        }
        if !p.forward_directives.is_empty() {
            self.line("");
        }

        // ── Identify clock and reset ─────────────────────────────────────────
        let clk_name = p.ports.iter()
            .find(|pt| matches!(&pt.ty, TypeExpr::Clock(_)))
            .map(|pt| pt.name.name.as_str())
            .unwrap_or("clk");
        let (rst_name, is_async, is_low) = Self::extract_reset_info(&p.ports);
        let ff_sens = Self::ff_sensitivity(clk_name, &rst_name, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst_name, is_low);

        // ── always_ff block ──────────────────────────────────────────────────
        self.line("// ── Stage register updates ──");
        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;

        // Reset branch
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();
            self.line(&format!("{}_valid_r <= 1'b0;", prefix));
            for (sig_name, _ty_str, init_str) in &stage_regs[si] {
                if !init_str.is_empty() {
                    self.line(&format!("{}_{} <= {};", prefix, sig_name, init_str));
                }
            }
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;

        // Per-stage update logic
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();

            if has_any_stall {
                // When this stage is not stalled, it accepts new data
                self.line(&format!("if (!{prefix}_stall) begin"));
                self.indent += 1;

                // Valid propagation:
                //   If upstream is stalled, insert bubble (valid=0)
                //   Otherwise, accept upstream's valid
                if si == 0 {
                    self.line(&format!("{prefix}_valid_r <= 1'b1;"));
                } else {
                    let prev_prefix = p.stages[si - 1].name.name.to_lowercase();
                    self.line(&format!("{prefix}_valid_r <= {prev_prefix}_stall ? 1'b0 : {prev_prefix}_valid_r;"));
                }

                // Register assignments from seq blocks
                for item in &stage.body {
                    if let ModuleBodyItem::RegBlock(rb) = item {
                        for stmt in &rb.stmts {
                            self.emit_pipeline_reg_stmt(stmt, &prefix, si, &stage_names, &stage_regs, &port_names);
                        }
                    }
                }

                self.indent -= 1;
                self.line("end");
            } else {
                // No stall logic — unconditional advancement
                if si == 0 {
                    self.line(&format!("{prefix}_valid_r <= 1'b1;"));
                } else {
                    let prev_prefix = p.stages[si - 1].name.name.to_lowercase();
                    self.line(&format!("{prefix}_valid_r <= {prev_prefix}_valid_r;"));
                }

                for item in &stage.body {
                    if let ModuleBodyItem::RegBlock(rb) = item {
                        for stmt in &rb.stmts {
                            self.emit_pipeline_reg_stmt(stmt, &prefix, si, &stage_names, &stage_regs, &port_names);
                        }
                    }
                }
            }
        }

        // Flush overrides
        for flush in &p.flush_directives {
            let target_prefix = flush.target_stage.name.to_lowercase();
            let cond_str = self.emit_pipeline_expr_str(&flush.condition, &stage_names, &stage_regs, &port_names);
            self.line(&format!("if ({}) begin", cond_str));
            self.indent += 1;
            self.line(&format!("{}_valid_r <= 1'b0;", target_prefix));
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("end");

        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Combinational outputs ────────────────────────────────────────────
        self.line("// ── Combinational outputs ──");
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();
            for item in &stage.body {
                if let ModuleBodyItem::CombBlock(cb) = item {
                    let all_simple = cb.stmts.iter().all(|s| matches!(s, CombStmt::Assign(_)));
                    if all_simple {
                        for stmt in &cb.stmts {
                            if let CombStmt::Assign(a) = stmt {
                                let val = self.emit_pipeline_stage_expr_str(&a.value, &prefix, si, &stage_names, &stage_regs, &port_names);
                                let target = if port_names.contains(&a.target.name) {
                                    a.target.name.clone()
                                } else {
                                    format!("{}_{}", prefix, a.target.name)
                                };
                                self.line(&format!("assign {} = {};", target, val));
                            }
                        }
                    } else {
                        // Use always_comb for blocks with if/else or match
                        self.line("always_comb begin");
                        self.indent += 1;
                        for stmt in &cb.stmts {
                            self.emit_pipeline_comb_stmt(stmt, &prefix, si, &stage_names, &stage_regs, &port_names);
                        }
                        self.indent -= 1;
                        self.line("end");
                    }
                }
                if let ModuleBodyItem::LetBinding(l) = item {
                    let val = self.emit_pipeline_stage_expr_str(&l.value, &prefix, si, &stage_names, &stage_regs, &port_names);
                    self.line(&format!("assign {} = {};", l.name.name, val));
                }
                if let ModuleBodyItem::Inst(inst) = item {
                    self.emit_pipeline_inst(inst, &prefix, si, &stage_names, &stage_regs, &port_names);
                }
            }
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
        // Skip comments that fall within the pipeline body — they were already
        // incorporated into the SV output or are not meaningful after codegen.
        while self.comment_idx < self.comments.len()
            && self.comments[self.comment_idx].0.start < p.span.end
        {
            self.comment_idx += 1;
        }
    }

    /// Emit a register statement with pipeline name rewriting.
    fn emit_pipeline_reg_stmt(
        &mut self,
        stmt: &Stmt,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        match stmt {
            Stmt::Assign(a) => {
                let target = self.emit_pipeline_lhs_str(&a.target, current_prefix, port_names);
                let val = self.emit_pipeline_stage_expr_str(&a.value, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                self.line(&format!("{} <= {};", target, val));
            }
            Stmt::IfElse(ie) => {
                self.emit_pipeline_reg_if_else(ie, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, false);
            }
            Stmt::Match(_) => {
                // MVP: basic pipeline doesn't need match in seq blocks
            }
            Stmt::Log(l) => { self.emit_log_stmt(l); }
        }
    }

    /// Rewrite a LHS expression (assignment target) with pipeline prefixing.
    fn emit_pipeline_lhs_str(
        &self,
        expr: &Expr,
        current_prefix: &str,
        port_names: &std::collections::HashSet<String>,
    ) -> String {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if port_names.contains(name) {
                    name.clone()
                } else {
                    format!("{}_{}", current_prefix, name)
                }
            }
            _ => self.emit_expr_str(expr),
        }
    }

    /// Collect all unique comb assign targets from a list of comb statements (recursive).
    fn collect_comb_targets(stmts: &[CombStmt]) -> Vec<String> {
        let mut targets = Vec::new();
        for stmt in stmts {
            match stmt {
                CombStmt::Assign(a) => {
                    if !targets.contains(&a.target.name) {
                        targets.push(a.target.name.clone());
                    }
                }
                CombStmt::IfElse(ie) => {
                    for t in Self::collect_comb_targets(&ie.then_stmts) {
                        if !targets.contains(&t) { targets.push(t); }
                    }
                    for t in Self::collect_comb_targets(&ie.else_stmts) {
                        if !targets.contains(&t) { targets.push(t); }
                    }
                }
                CombStmt::MatchExpr(_) | CombStmt::Log(_) => {}
            }
        }
        targets
    }

    /// Resolve the type of an inst output wire by finding which register reads it
    /// in the stage's seq block (e.g. `alu_result <= alu_out` → use alu_result's type).
    fn resolve_inst_wire_type_from_consumers(
        wire_name: &str,
        body: &[ModuleBodyItem],
        regs: &[(String, String, String)],
    ) -> Option<String> {
        for item in body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                for stmt in &rb.stmts {
                    if let Stmt::Assign(a) = stmt {
                        // Check if RHS references the wire name
                        if let ExprKind::Ident(rhs) = &a.value.kind {
                            if rhs == wire_name {
                                // LHS is the register — find its type
                                if let ExprKind::Ident(lhs) = &a.target.kind {
                                    if let Some(r) = regs.iter().find(|(rn, _, _)| rn == lhs) {
                                        return Some(r.1.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Resolve the type of a comb wire by inspecting assignment sources.
    /// Looks for known registers (local or cross-stage) in assignment RHS.
    fn resolve_comb_wire_type(
        target: &str,
        stmts: &[CombStmt],
        current_stage_idx: usize,
        stage_regs: &[Vec<(String, String, String)>],
        stage_names: &[&str],
    ) -> Option<String> {
        for stmt in stmts {
            match stmt {
                CombStmt::Assign(a) if a.target.name == target => {
                    // Check if RHS is a bare identifier (local register)
                    if let ExprKind::Ident(name) = &a.value.kind {
                        if let Some(r) = stage_regs[current_stage_idx].iter()
                            .find(|(rn, _, _)| rn == name)
                        {
                            return Some(r.1.clone());
                        }
                    }
                    // Check if RHS is a cross-stage reference: Stage.signal
                    if let ExprKind::FieldAccess(base, field) = &a.value.kind {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            if let Some(si) = stage_names.iter().position(|&sn| sn == base_name) {
                                if let Some(r) = stage_regs[si].iter()
                                    .find(|(rn, _, _)| rn == &field.name)
                                {
                                    return Some(r.1.clone());
                                }
                            }
                        }
                    }
                }
                CombStmt::IfElse(ie) => {
                    if let Some(ty) = Self::resolve_comb_wire_type(target, &ie.then_stmts, current_stage_idx, stage_regs, stage_names) {
                        return Some(ty);
                    }
                    if let Some(ty) = Self::resolve_comb_wire_type(target, &ie.else_stmts, current_stage_idx, stage_regs, stage_names) {
                        return Some(ty);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Emit a comb statement within a pipeline stage context (inside always_comb).
    /// Handles Assign, IfElse with pipeline name rewriting.
    fn emit_pipeline_comb_stmt(
        &mut self,
        stmt: &CombStmt,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        match stmt {
            CombStmt::Assign(a) => {
                let val = self.emit_pipeline_stage_expr_str(&a.value, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let target = if port_names.contains(&a.target.name) {
                    a.target.name.clone()
                } else {
                    format!("{}_{}", current_prefix, a.target.name)
                };
                self.line(&format!("{} = {};", target, val));
            }
            CombStmt::IfElse(ie) => {
                self.emit_pipeline_comb_if_else(ie, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, false);
            }
            CombStmt::MatchExpr(_) => {} // TODO if needed
            CombStmt::Log(l) => { self.emit_log_stmt(l); }
        }
    }

    fn emit_pipeline_reg_if_else(
        &mut self,
        ie: &IfElse,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
        is_chain: bool,
    ) {
        let cond = self.emit_pipeline_stage_expr_str(&ie.cond, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("if ({}) begin", cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_pipeline_reg_if_else(nested, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    fn emit_pipeline_comb_if_else(
        &mut self,
        ie: &CombIfElse,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
        is_chain: bool,
    ) {
        let cond = self.emit_pipeline_stage_expr_str(&ie.cond, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("if ({}) begin", cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_pipeline_comb_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let CombStmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_pipeline_comb_if_else(nested, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_pipeline_comb_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    /// Emit an expression within a specific stage context (knows which stage it's in,
    /// so bare identifiers that are stage registers get prefixed).
    fn emit_pipeline_stage_expr_str(
        &self,
        expr: &Expr,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) -> String {
        match &expr.kind {
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if let Some(si) = stage_names.iter().position(|&sn| sn == base_name) {
                        let prefix = stage_names[si].to_lowercase();
                        return format!("{}_{}", prefix, field.name);
                    }
                }
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("{}.{}", b, field.name)
            }
            ExprKind::Ident(name) => {
                if port_names.contains(name) {
                    return name.clone();
                }
                // Check if it's a register in the current stage
                if let Some(regs) = stage_regs.get(current_stage_idx) {
                    if regs.iter().any(|(rn, _, _)| rn == name) {
                        return format!("{}_{}", current_prefix, name);
                    }
                }
                // Compiler-generated stage signals (valid_r)
                if name == "valid_r" {
                    return format!("{}_valid_r", current_prefix);
                }
                name.clone()
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let l = self.emit_pipeline_stage_expr_str(lhs, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let r = self.emit_pipeline_stage_expr_str(rhs, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let op_str = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*",
                    BinOp::Div => "/", BinOp::Mod => "%", BinOp::Eq => "==",
                    BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "&&",
                    BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::Shl => "<<", BinOp::Shr => ">>",
                };
                format!("({l} {op_str} {r})")
            }
            ExprKind::Unary(op, operand) => {
                let o = self.emit_pipeline_stage_expr_str(operand, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match op {
                    UnaryOp::Not => format!("(!{o})"),
                    UnaryOp::BitNot => format!("(~{o})"),
                    UnaryOp::Neg => format!("(-{o})"),
                }
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match method.name.as_str() {
                    "trunc" if args.len() == 2 => {
                        let hi = self.emit_expr_str(&args[0]);
                        let lo = self.emit_expr_str(&args[1]);
                        format!("{b}[{hi}:{lo}]")
                    }
                    "trunc" | "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{w}'({b})")
                        } else {
                            b
                        }
                    }
                    "sext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{{{{({w}-$bits({b})){{{b}[$bits({b})-1]}}}}}}, {b}}}")
                        } else {
                            b
                        }
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let i = self.emit_pipeline_stage_expr_str(idx, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("{b}[{i}]")
            }
            _ => self.emit_expr_str(expr),
        }
    }

    /// Emit an expression with pipeline name rewriting:
    /// - `Stage.signal` → `stage_signal`
    /// - Bare signal in stage context → preserved (caller handles prefix)
    /// - Port names → kept as-is
    fn emit_pipeline_expr_str(
        &self,
        expr: &Expr,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) -> String {
        match &expr.kind {
            ExprKind::FieldAccess(base, field) => {
                // Check if base is a stage name → rewrite to stage_signal
                if let ExprKind::Ident(base_name) = &base.kind {
                    if let Some(si) = stage_names.iter().position(|&sn| sn == base_name) {
                        let prefix = stage_names[si].to_lowercase();
                        return format!("{}_{}", prefix, field.name);
                    }
                }
                // Otherwise use default emission
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                format!("{}.{}", b, field.name)
            }
            ExprKind::Ident(name) => {
                // Port names stay as-is
                if port_names.contains(name) {
                    return name.clone();
                }
                // Check if it's a stage name itself (shouldn't appear bare normally)
                // Otherwise it's a local — keep as-is (the caller adds prefix if needed)
                name.clone()
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let l = self.emit_pipeline_expr_str(lhs, stage_names, stage_regs, port_names);
                let r = self.emit_pipeline_expr_str(rhs, stage_names, stage_regs, port_names);
                let op_str = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*",
                    BinOp::Div => "/", BinOp::Mod => "%", BinOp::Eq => "==",
                    BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "&&",
                    BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::Shl => "<<", BinOp::Shr => ">>",
                };
                format!("({l} {op_str} {r})")
            }
            ExprKind::Unary(op, operand) => {
                let o = self.emit_pipeline_expr_str(operand, stage_names, stage_regs, port_names);
                match op {
                    UnaryOp::Not => format!("(!{o})"),
                    UnaryOp::BitNot => format!("(~{o})"),
                    UnaryOp::Neg => format!("(-{o})"),
                }
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                match method.name.as_str() {
                    "trunc" if args.len() == 2 => {
                        let hi = self.emit_expr_str(&args[0]);
                        let lo = self.emit_expr_str(&args[1]);
                        format!("{b}[{hi}:{lo}]")
                    }
                    "trunc" | "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{w}'({b})")
                        } else {
                            b
                        }
                    }
                    "sext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{{{{({w}-$bits({b})){{{b}[$bits({b})-1]}}}}}}, {b}}}")
                        } else {
                            b
                        }
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                let i = self.emit_pipeline_expr_str(idx, stage_names, stage_regs, port_names);
                format!("{b}[{i}]")
            }
            // For everything else, fall back to regular emit
            _ => self.emit_expr_str(expr),
        }
    }

    // ── FIFO ──────────────────────────────────────────────────────────────────

    fn emit_fifo(&mut self, f: &FifoDecl) {
        use crate::resolve::detect_async_fifo;
        let is_async = detect_async_fifo(&f.ports);

        // Resolve DEPTH and TYPE from params
        let depth_expr = f.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "16".to_string());

        // Resolve TYPE default as an SV type string
        let type_default_sv = f.params.iter()
            .find(|p| p.name.name == "TYPE")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => Some(self.emit_port_type_str(ty)),
                _ => None,
            })
            .unwrap_or_else(|| "logic [7:0]".to_string());

        // Collect port names to know what's declared
        let port_names: Vec<&str> = f.ports.iter().map(|p| p.name.name.as_str()).collect();

        let n = &f.name.name;

        // ── Module header ────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        self.line(&format!("parameter int  DEPTH = {depth_expr},"));
        self.line(&format!("parameter type TYPE  = {type_default_sv}"));
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Emit declared ports
        for (i, p) in f.ports.iter().enumerate() {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            // Named("TYPE") references → use the TYPE parameter directly
            let ty_str = self.emit_fifo_port_type(&p.ty);
            let comma = if i < f.ports.len() - 1 { "," } else { "" };
            self.line(&format!("{dir} {ty_str} {}{comma}", p.name.name));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        if is_async {
            self.emit_fifo_async_body(f, &port_names);
        } else {
            self.emit_fifo_sync_body(f, &port_names);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    fn width_of_type_str(&self, ty_str: &str) -> String {
        // Extract bit width from "logic [N-1:0]" → "N"
        // or "logic" → "1"
        if let Some(bracket) = ty_str.find('[') {
            let inner = &ty_str[bracket+1..];
            if let Some(colon) = inner.find(':') {
                let upper = inner[..colon].trim();
                // upper is "N-1", we want N
                if upper.ends_with("-1") {
                    return upper[..upper.len()-2].to_string();
                }
                return upper.to_string();
            }
        }
        "1".to_string()
    }

    fn emit_fifo_port_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(ident) if ident.name == "TYPE" => "TYPE".to_string(),
            other => self.emit_port_type_str(other),
        }
    }

    fn emit_fifo_sync_body(&mut self, f: &FifoDecl, _port_names: &[&str]) {
        self.line("localparam int PTR_W = $clog2(DEPTH) + 1;");
        self.line("");
        self.line("TYPE                  mem [0:DEPTH-1];");
        self.line("logic [PTR_W-1:0]     wr_ptr;");
        self.line("logic [PTR_W-1:0]     rd_ptr;");
        self.line("logic                 full;");
        self.line("logic                 empty;");
        self.line("");
        self.line("// Full when MSBs differ and lower bits match");
        self.line("assign full        = (wr_ptr[PTR_W-1] != rd_ptr[PTR_W-1]) &&");
        self.line("                     (wr_ptr[PTR_W-2:0] == rd_ptr[PTR_W-2:0]);");
        self.line("assign empty       = (wr_ptr == rd_ptr);");
        self.line("assign push_ready  = !full;");
        self.line("assign pop_valid   = !empty;");
        self.line("assign pop_data    = mem[rd_ptr[PTR_W-2:0]];");
        self.line("");

        // Determine reset port info
        let (rst, is_async, is_low) = Self::extract_reset_info(&f.ports);
        let clk = f.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");
        let ff_sens = Self::ff_sensitivity(clk, &rst, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line("wr_ptr <= '0;");
        self.line("rd_ptr <= '0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("if (push_valid && push_ready) begin");
        self.indent += 1;
        self.line("mem[wr_ptr[PTR_W-2:0]] <= push_data;");
        self.line("wr_ptr <= wr_ptr + 1;");
        self.indent -= 1;
        self.line("end");
        self.line("if (pop_valid && pop_ready) begin");
        self.indent += 1;
        self.line("rd_ptr <= rd_ptr + 1;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_fifo_async_body(&mut self, f: &FifoDecl, port_names: &[&str]) {
        // Find wr_clk, rd_clk, rst port names
        let clock_ports: Vec<&PortDecl> = f.ports.iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .collect();
        let wr_clk = clock_ports.get(0).map(|p| p.name.name.as_str()).unwrap_or("wr_clk");
        let rd_clk = clock_ports.get(1).map(|p| p.name.name.as_str()).unwrap_or("rd_clk");
        let (rst, _is_async_rst, is_low) = Self::extract_reset_info(&f.ports);
        // Async FIFOs always use async reset (reset in sensitivity list for all FF blocks)
        let rst_cond = Self::rst_condition(&rst, is_low);
        let rst_edge = if is_low { "negedge" } else { "posedge" };

        self.line("localparam int PTR_W = $clog2(DEPTH) + 1;");
        self.line("");
        self.line("// Gray-code helper functions");
        self.line("function automatic logic [PTR_W-1:0] bin2gray(input logic [PTR_W-1:0] b);");
        self.indent += 1;
        self.line("return b ^ (b >> 1);");
        self.indent -= 1;
        self.line("endfunction");
        self.line("function automatic logic [PTR_W-1:0] gray2bin(input logic [PTR_W-1:0] g);");
        self.indent += 1;
        self.line("logic [PTR_W-1:0] b;");
        self.line("b[PTR_W-1] = g[PTR_W-1];");
        self.line("for (int i = PTR_W-2; i >= 0; i--) b[i] = b[i+1] ^ g[i];");
        self.line("return b;");
        self.indent -= 1;
        self.line("endfunction");
        self.line("");
        self.line("TYPE              mem [0:DEPTH-1];");
        self.line("logic [PTR_W-1:0] wr_ptr_bin, rd_ptr_bin;");
        self.line("logic [PTR_W-1:0] wr_ptr_gray, rd_ptr_gray;");
        self.line("// Two-stage synchronizers");
        self.line("logic [PTR_W-1:0] wr_ptr_gray_s1, wr_ptr_gray_sync; // in rd domain");
        self.line("logic [PTR_W-1:0] rd_ptr_gray_s1, rd_ptr_gray_sync; // in wr domain");
        self.line("");
        self.line("assign wr_ptr_gray = bin2gray(wr_ptr_bin);");
        self.line("assign rd_ptr_gray = bin2gray(rd_ptr_bin);");
        self.line("");
        self.line(&format!("// Sync wr_ptr into rd domain ({rd_clk})"));
        self.line(&format!("always_ff @(posedge {rd_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin wr_ptr_gray_s1 <= '0; wr_ptr_gray_sync <= '0; end"));
        self.line("else begin wr_ptr_gray_s1 <= wr_ptr_gray; wr_ptr_gray_sync <= wr_ptr_gray_s1; end");
        self.indent -= 1;
        self.line("end");
        self.line(&format!("// Sync rd_ptr into wr domain ({wr_clk})"));
        self.line(&format!("always_ff @(posedge {wr_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin rd_ptr_gray_s1 <= '0; rd_ptr_gray_sync <= '0; end"));
        self.line("else begin rd_ptr_gray_s1 <= rd_ptr_gray; rd_ptr_gray_sync <= rd_ptr_gray_s1; end");
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("// Write domain: full detection using synced rd_ptr");
        self.line("logic full_r;");
        self.line("logic [PTR_W-1:0] rd_ptr_bin_wr;");
        self.line("assign rd_ptr_bin_wr = gray2bin(rd_ptr_gray_sync);");
        self.line("assign full_r  = (wr_ptr_bin[PTR_W-1] != rd_ptr_bin_wr[PTR_W-1]) &&");
        self.line("                 (wr_ptr_bin[PTR_W-2:0] == rd_ptr_bin_wr[PTR_W-2:0]);");
        self.line("assign push_ready = !full_r;");
        self.line(&format!("always_ff @(posedge {wr_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) wr_ptr_bin <= '0;"));
        self.line("else if (push_valid && push_ready) begin");
        self.indent += 1;
        self.line("mem[wr_ptr_bin[PTR_W-2:0]] <= push_data;");
        self.line("wr_ptr_bin <= wr_ptr_bin + 1;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("// Read domain: empty detection using synced wr_ptr");
        self.line("logic empty_r;");
        self.line("logic [PTR_W-1:0] wr_ptr_bin_rd;");
        self.line("assign wr_ptr_bin_rd = gray2bin(wr_ptr_gray_sync);");
        self.line("assign empty_r = (rd_ptr_bin == wr_ptr_bin_rd);");
        self.line("assign pop_valid = !empty_r;");
        self.line("assign pop_data  = mem[rd_ptr_bin[PTR_W-2:0]];");
        if port_names.contains(&"full") {
            self.line("assign full  = full_r;");
        }
        if port_names.contains(&"empty") {
            self.line("assign empty = empty_r;");
        }
        self.line(&format!("always_ff @(posedge {rd_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) rd_ptr_bin <= '0;"));
        self.line("else if (pop_valid && pop_ready) rd_ptr_bin <= rd_ptr_bin + 1;");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_pattern(&self, pat: &Pattern) -> String {
        match pat {
            Pattern::Ident(id) => id.name.clone(),
            Pattern::EnumVariant(_, variant) => variant.name.to_uppercase(),
            Pattern::Literal(expr) => self.emit_expr_str(expr),
            Pattern::Wildcard => "default".to_string(),
        }
    }

    fn emit_expr_str(&self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Literal(lit) => match lit {
                LitKind::Dec(v) => format!("{v}"),
                LitKind::Hex(v) => format!("'h{v:X}"),
                LitKind::Bin(v) => format!("'b{v:b}"),
                LitKind::Sized(w, v) => format!("{w}'d{v}"),
            },
            ExprKind::Bool(true) => "1'b1".to_string(),
            ExprKind::Bool(false) => "1'b0".to_string(),
            ExprKind::Ident(name) => {
                name.clone()
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let l = self.emit_expr_str(lhs);
                let r = self.emit_expr_str(rhs);
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Eq => "==",
                    BinOp::Neq => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Lte => "<=",
                    BinOp::Gte => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<",
                    BinOp::Shr => ">>",
                };
                format!("({l} {op_str} {r})")
            }
            ExprKind::Unary(op, operand) => {
                let o = self.emit_expr_str(operand);
                match op {
                    UnaryOp::Not => format!("(!{o})"),
                    UnaryOp::BitNot => format!("(~{o})"),
                    UnaryOp::Neg => format!("(-{o})"),
                }
            }
            ExprKind::FieldAccess(base, field) => {
                let b = self.emit_expr_str(base);
                format!("{b}.{}", field.name)
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.emit_expr_str(base);
                match method.name.as_str() {
                    "trunc" if args.len() == 2 => {
                        let hi = self.emit_expr_str(&args[0]);
                        let lo = self.emit_expr_str(&args[1]);
                        format!("{b}[{hi}:{lo}]")
                    }
                    "trunc" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            // SV size cast: valid on any expression, including compound ones.
                            // e.g. (count_r + 1).trunc<8>() → 8'(count_r + 1)
                            format!("{w}'({b})")
                        } else {
                            b
                        }
                    }
                    "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            // SV size cast zero-extends when target is wider than source.
                            format!("{w}'({b})")
                        } else {
                            b
                        }
                    }
                    "sext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            // Sign-extension: replicate the MSB into the upper bits.
                            format!("{{{{({w}-$bits({b})){{{b}[$bits({b})-1]}}}}, {b}}}")
                        } else {
                            b
                        }
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Cast(expr, ty) => {
                let e = self.emit_expr_str(expr);
                let t = self.emit_type_str(ty);
                format!("{t}'({e})")
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_expr_str(base);
                let i = self.emit_expr_str(idx);
                format!("{b}[{i}]")
            }
            ExprKind::StructLiteral(_name, fields) => {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name.name, self.emit_expr_str(&f.value)))
                    .collect();
                format!("'{{{}}}", field_strs.join(", "))
            }
            ExprKind::EnumVariant(_, variant) => variant.name.to_uppercase(),
            ExprKind::Todo => {
                "'0 /* TODO: todo! placeholder */".to_string()
            }
            ExprKind::Concat(parts) => {
                let strs: Vec<String> = parts.iter().map(|p| self.emit_expr_str(p)).collect();
                format!("{{{}}}", strs.join(", "))
            }
            ExprKind::Clog2(arg) => {
                let a = self.emit_expr_str(arg);
                format!("$clog2({a})")
            }
            ExprKind::Match(scrutinee, _arms) => {
                let s = self.emit_expr_str(scrutinee);
                format!("/* match({s}) */ '0")
            }
            ExprKind::ExprMatch(scrutinee, arms) => {
                // Emit as nested ternary: (cond) ? val : (cond) ? val : default
                let s = self.emit_expr_str(scrutinee);
                let mut result = "'0".to_string();
                for arm in arms.iter().rev() {
                    let val = self.emit_expr_str(&arm.value);
                    let cond = match &arm.pattern {
                        Pattern::Wildcard => {
                            result = val;
                            continue;
                        }
                        Pattern::Literal(e) => {
                            let lit = self.emit_expr_str(e);
                            format!("({s} == {lit})")
                        }
                        Pattern::Ident(id) if id.name == "_" => {
                            result = val;
                            continue;
                        }
                        Pattern::Ident(id) => format!("({s} == {id})", id = id.name),
                        Pattern::EnumVariant(en, vr) => {
                            format!("({s} == {en}__{vr})", en = en.name.to_uppercase(), vr = vr.name.to_uppercase())
                        }
                    };
                    result = format!("({cond} ? {val} : {result})");
                }
                result
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                let c = self.emit_expr_str(cond);
                let t = self.emit_expr_str(then_expr);
                let e = self.emit_expr_str(else_expr);
                format!("({c}) ? ({t}) : ({e})")
            }
            ExprKind::FunctionCall(name, args) => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.emit_expr_str(a)).collect();
                // Resolve mangled name if this is an overloaded function.
                let sv_name = if let Some((Symbol::Function(overloads), _)) = self.symbols.globals.get(name) {
                    if overloads.len() > 1 {
                        let idx = self.overload_map.get(&expr.span.start).copied().unwrap_or(0);
                        let ov = &overloads[idx];
                        let suffix: String = ov.arg_types.iter()
                            .map(|t| Self::type_mangle_tag(t))
                            .collect::<Vec<_>>()
                            .join("_");
                        format!("{name}_{suffix}")
                    } else {
                        name.clone()
                    }
                } else {
                    name.clone()
                };
                format!("{sv_name}({})", arg_strs.join(", "))
            }
        }
    }

    fn emit_type_str(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::UInt(w) => {
                let ws = self.emit_expr_str(w);
                format!("logic [{ws}-1:0]")
            }
            TypeExpr::SInt(w) => {
                let ws = self.emit_expr_str(w);
                format!("logic signed [{ws}-1:0]")
            }
            TypeExpr::Bool => "logic".to_string(),
            TypeExpr::Bit => "logic".to_string(),
            TypeExpr::Clock(_) => "logic".to_string(),
            TypeExpr::Reset(_, _) => "logic".to_string(),
            TypeExpr::Vec(inner, size) => {
                let inner_str = self.emit_type_str(inner);
                let size_str = self.emit_expr_str(size);
                format!("{inner_str} [0:{size_str}-1]")
            }
            TypeExpr::Named(ident) => ident.name.clone(),
        }
    }

    fn emit_port_type_str(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::UInt(w) => {
                let ws = self.emit_expr_str(w);
                format!("logic [{ws}-1:0]", )
            }
            TypeExpr::SInt(w) => {
                let ws = self.emit_expr_str(w);
                format!("logic signed [{ws}-1:0]")
            }
            TypeExpr::Bool => "logic".to_string(),
            TypeExpr::Bit => "logic".to_string(),
            TypeExpr::Clock(_) => "logic".to_string(),
            TypeExpr::Reset(_, _) => "logic".to_string(),
            TypeExpr::Vec(inner, size) => {
                let inner_str = self.emit_port_type_str(inner);
                let size_str = self.emit_expr_str(size);
                format!("{inner_str} [0:{size_str}-1]")
            }
            TypeExpr::Named(ident) => ident.name.clone(),
        }
    }

    fn emit_logic_type_str(&self, ty: &TypeExpr) -> String {
        self.emit_type_str(ty)
    }

    /// For Vec types, returns (element_type_str, " [0:N-1]") so the unpacked
    /// dimension can be placed after the signal name in declarations.
    /// For non-Vec types, returns (type_str, "").
    fn emit_type_and_array_suffix(&self, ty: &TypeExpr) -> (String, String) {
        if let TypeExpr::Vec(inner, size) = ty {
            let inner_str = self.emit_type_str(inner);
            let size_str = self.emit_expr_str(size);
            (inner_str, format!(" [0:{size_str}-1]"))
        } else {
            (self.emit_type_str(ty), String::new())
        }
    }

    // ── RAM ───────────────────────────────────────────────────────────────────

    fn emit_ram(&mut self, r: &RamDecl) {
        use crate::ast::{RamKind, RamInit};

        // Resolve DATA_WIDTH from WIDTH type param
        let data_width_ty = r.params.iter()
            .find(|p| p.name.name == "WIDTH")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => Some(self.emit_port_type_str(ty)),
                _ => None,
            })
            .unwrap_or_else(|| "logic [7:0]".to_string());
        let data_width_num = self.width_of_type_str(&data_width_ty);

        // Resolve DEPTH from param default
        let depth_expr = r.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "256".to_string());

        let n = &r.name.name.clone();

        // ── Module header ────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        self.line(&format!("parameter int DEPTH = {depth_expr},"));
        self.line(&format!("parameter int DATA_WIDTH = {data_width_num}"));
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Top-level ports (clk, rst)
        let mut all_ports: Vec<String> = Vec::new();
        for p in &r.ports {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        // Port group signals flattened: {group}_{signal}
        for pg in &r.port_groups {
            for s in &pg.signals {
                let dir = match s.direction { Direction::In => "input", Direction::Out => "output" };
                let ty_str = self.emit_ram_signal_type(&s.ty);
                all_ports.push(format!("{dir} {ty_str} {}_{}", pg.name.name, s.name.name));
            }
        }
        let port_count = all_ports.len();
        for (i, p) in all_ports.iter().enumerate() {
            let comma = if i < port_count - 1 { "," } else { "" };
            self.line(&format!("{p}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // ── Memory array ─────────────────────────────────────────────────────
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");

        // Find the clock signal name
        let clk_name = r.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());

        match r.kind {
            RamKind::Single => self.emit_ram_single(r, &clk_name, &data_width_ty),
            RamKind::SimpleDual => self.emit_ram_simple_dual(r, &clk_name, &data_width_ty),
            RamKind::TrueDual => self.emit_ram_true_dual(r, &clk_name, &data_width_ty),
        }

        // ── Init block ───────────────────────────────────────────────────────
        if let Some(init) = &r.init {
            self.line("");
            match init {
                RamInit::Zero => {
                    self.line("initial begin");
                    self.indent += 1;
                    self.line("for (int i = 0; i < DEPTH; i++) mem[i] = '0;");
                    self.indent -= 1;
                    self.line("end");
                }
                RamInit::None => {}
                RamInit::File(path) => {
                    self.line(&format!("initial $readmemh(\"{path}\", mem);"));
                }
                RamInit::Value(expr) => {
                    let v = self.emit_expr_str(expr);
                    self.line("initial begin");
                    self.indent += 1;
                    self.line(&format!("for (int i = 0; i < DEPTH; i++) mem[i] = {v};"));
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Counter ───────────────────────────────────────────────────────────────

    fn emit_counter(&mut self, c: &crate::ast::CounterDecl) {
        use crate::ast::{CounterMode, CounterDirection};

        let n = &c.name.name.clone();

        // Find relevant params
        let max_param = c.params.iter()
            .find(|p| p.name.name == "MAX")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e));

        // Determine counter width
        // Use MAX param if present, else look for WIDTH
        let width_param = c.params.iter()
            .find(|p| p.name.name == "WIDTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e));

        // Find ports to determine direction
        let has_inc  = c.ports.iter().any(|p| p.name.name == "inc");
        let has_dec  = c.ports.iter().any(|p| p.name.name == "dec");
        let has_load = c.ports.iter().any(|p| p.name.name == "load");
        let has_clear= c.ports.iter().any(|p| p.name.name == "clear");
        let value_port = c.ports.iter().find(|p| p.name.name == "value");

        // Compute width from value port type or fallback
        let count_width = if let Some(vp) = value_port {
            match &vp.ty {
                TypeExpr::UInt(w) => self.emit_expr_str(w),
                _ => width_param.clone().unwrap_or_else(|| "8".to_string()),
            }
        } else {
            width_param.clone().unwrap_or_else(|| "8".to_string())
        };

        let clk = c.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&c.ports);

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        for (i, p) in c.params.iter().enumerate() {
            let comma = if i < c.params.len() - 1 { "," } else { "" };
            let default_str = p.default.as_ref()
                .map(|e| format!(" = {}", self.emit_expr_str(e)))
                .unwrap_or_default();
            self.line(&format!("parameter int {}{default_str}{comma}", p.name.name));
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        let mut all_ports: Vec<String> = Vec::new();
        for p in &c.ports {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        let port_count = all_ports.len();
        for (i, p) in all_ports.iter().enumerate() {
            let comma = if i < port_count - 1 { "," } else { "" };
            self.line(&format!("{p}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        let init_val = c.init.as_ref()
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "'0".to_string());

        // ── Internal register ─────────────────────────────────────────────────
        self.line(&format!("logic [{count_width}-1:0] count_r;"));

        // ── Determine FF sensitivity list ─────────────────────────────────────
        let ff_sens = Self::ff_sensitivity(&clk, &rst, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;

        // Reset branch
        self.line(&format!("if ({rst_cond}) count_r <= {init_val};"));

        // Load/clear
        if has_clear {
            self.line(&format!("else if (clear) count_r <= {init_val};"));
        }
        if has_load {
            self.line("else if (load) count_r <= load_data;");
        }

        match (c.direction, c.mode) {
            (CounterDirection::Up, CounterMode::Wrap) => {
                let max_cond = if max_param.is_some() {
                    format!("count_r == {count_width}'(MAX)")
                } else {
                    format!("&count_r")  // all bits set
                };
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("if ({max_cond}) count_r <= {init_val};"));
                self.line("else count_r <= count_r + 1;");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Down, CounterMode::Wrap) => {
                let min_cond = "count_r == '0";
                let max_val = if max_param.is_some() {
                    format!("{count_width}'(MAX)")
                } else {
                    format!("'1")
                };
                let dec_cond = if has_dec { "else if (dec) begin" } else { "else begin" };
                self.line(dec_cond);
                self.indent += 1;
                self.line(&format!("if ({min_cond}) count_r <= {max_val};"));
                self.line("else count_r <= count_r - 1;");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::UpDown, CounterMode::Wrap) => {
                self.line("else if (inc && !dec) count_r <= count_r + 1;");
                self.line("else if (dec && !inc) count_r <= count_r - 1;");
            }
            (CounterDirection::Up, CounterMode::Saturate) => {
                let max_cond = if max_param.is_some() {
                    format!("count_r < {count_width}'(MAX)")
                } else {
                    format!("!(&count_r)")
                };
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("if ({max_cond}) count_r <= count_r + 1;"));
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Down, CounterMode::Saturate) => {
                let dec_cond = if has_dec { "else if (dec) begin" } else { "else begin" };
                self.line(dec_cond);
                self.indent += 1;
                self.line("if (count_r > '0) count_r <= count_r - 1;");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Up, CounterMode::Gray) => {
                // Gray: increment binary then apply bin→gray
                self.line("else if (inc) begin");
                self.indent += 1;
                self.line("count_r <= (count_r + 1) ^ ((count_r + 1) >> 1);");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Up, CounterMode::OneHot) => {
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("count_r <= {{count_r[{count_width}-2:0], count_r[{count_width}-1]}};"));
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Up, CounterMode::Johnson) => {
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("count_r <= {{~count_r[0], count_r[{count_width}-1:1]}};"));
                self.indent -= 1;
                self.line("end");
            }
            // Default: simple up wrap
            _ => {
                let inc_cond = if has_inc { "else if (inc)" } else { "else" };
                self.line(&format!("{inc_cond} count_r <= count_r + 1;"));
            }
        }

        self.indent -= 1;
        self.line("end");

        // ── Output assignments ────────────────────────────────────────────────
        if value_port.is_some() {
            self.line("assign value = count_r;");
        }
        // at_max
        if c.ports.iter().any(|p| p.name.name == "at_max") {
            let max_expr = if max_param.is_some() {
                format!("count_r == {count_width}'(MAX)")
            } else {
                format!("&count_r")
            };
            self.line(&format!("assign at_max = ({max_expr});"));
        }
        // at_min
        if c.ports.iter().any(|p| p.name.name == "at_min") {
            self.line("assign at_min = (count_r == '0);");
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Arbiter ───────────────────────────────────────────────────────────────

    fn emit_arbiter(&mut self, a: &crate::ast::ArbiterDecl) {
        use crate::ast::ArbiterPolicy;

        let n = &a.name.name.clone();

        // Find NUM_REQ param
        let num_req_default = a.params.iter()
            .find(|p| p.name.name == "NUM_REQ")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "4".to_string());

        // Parse NUM_REQ as integer for bit width calculations
        let num_req_int: u64 = num_req_default.parse().unwrap_or(4);
        let req_width = if num_req_int <= 1 { 1 } else { (num_req_int as f64).log2().ceil() as u32 };

        let clk = a.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&a.ports);

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        if a.params.is_empty() {
            self.line("parameter int NUM_REQ = 4");
        } else {
            for (i, p) in a.params.iter().enumerate() {
                let comma = if i < a.params.len() - 1 { "," } else { "" };
                let default_str = p.default.as_ref()
                    .map(|e| format!(" = {}", self.emit_expr_str(e)))
                    .unwrap_or_default();
                self.line(&format!("parameter int {}{default_str}{comma}", p.name.name));
            }
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Scalar ports (clk, rst)
        let mut all_ports: Vec<String> = Vec::new();
        for p in &a.ports {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        // Port arrays
        for pa in &a.port_arrays {
            let count_str = self.emit_expr_str(&pa.count_expr);
            for s in &pa.signals {
                let dir = match s.direction { Direction::In => "input", Direction::Out => "output" };
                let port_name = format!("{}_{}", pa.name.name, s.name.name);
                let port_str = self.emit_port_array_signal_str(dir, &s.ty, &port_name, &count_str);
                all_ports.push(port_str);
            }
        }
        let port_count = all_ports.len();
        for (i, p) in all_ports.iter().enumerate() {
            let comma = if i < port_count - 1 { "," } else { "" };
            self.line(&format!("{p}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // Emit hook function inside arbiter module if custom policy
        if let ArbiterPolicy::Custom(ref fn_ident) = a.policy {
            let fns = std::mem::take(&mut self.pending_functions);
            for f in &fns {
                if f.name.name == fn_ident.name {
                    self.emit_function(f);
                }
            }
            self.pending_functions = fns;
        }

        // ── Detect request/grant signal names from port arrays ─────────────────
        // The first port array is assumed to be the request ports.
        // Input signal in it → req_valid_sig; output signal → req_ready_sig
        let (req_valid_sig, req_ready_sig) = if let Some(pa) = a.port_arrays.first() {
            let pfx = &pa.name.name;
            let valid = pa.signals.iter()
                .find(|s| s.direction == Direction::In)
                .map(|s| format!("{pfx}_{}", s.name.name))
                .unwrap_or_else(|| format!("{pfx}_valid"));
            let ready = pa.signals.iter()
                .find(|s| s.direction == Direction::Out)
                .map(|s| format!("{pfx}_{}", s.name.name))
                .unwrap_or_else(|| format!("{pfx}_ready"));
            (valid, ready)
        } else {
            ("request_valid".to_string(), "request_ready".to_string())
        };

        let latency = a.latency;
        let policy = a.policy.clone();

        // When latency > 1, grant logic targets intermediate _comb signals
        // which are then pipelined to the actual output ports.
        let (gv_sig, gr_sig, rr_sig) = if latency > 1 {
            let num_req_str = self.emit_expr_str(
                &a.params.iter().find(|p| p.name.name == "NUM_REQ")
                    .and_then(|p| p.default.clone())
                    .unwrap_or(crate::ast::Expr {
                        kind: crate::ast::ExprKind::Literal(crate::ast::LitKind::Dec(num_req_int)),
                        span: a.span,
                    })
            );
            self.line(&format!("logic grant_valid_comb;"));
            self.line(&format!("logic [{req_width}-1:0] grant_requester_comb;"));
            self.line(&format!("logic [{num_req_str}-1:0] {req_ready_sig}_comb;"));
            self.line("");
            ("grant_valid_comb".to_string(), "grant_requester_comb".to_string(), format!("{req_ready_sig}_comb"))
        } else {
            ("grant_valid".to_string(), "grant_requester".to_string(), req_ready_sig.clone())
        };

        // ── Arbiter logic ─────────────────────────────────────────────────────
        match policy {
            ArbiterPolicy::RoundRobin => {
                self.emit_arbiter_round_robin(&clk, &rst, is_async, is_low, req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Priority => {
                self.emit_arbiter_priority(req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Lru => {
                self.emit_arbiter_round_robin(&clk, &rst, is_async, is_low, req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Weighted(_) => {
                self.emit_arbiter_priority(req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Custom(ref fn_ident) => {
                self.emit_arbiter_custom(a, fn_ident, &clk, &rst, is_async, is_low, req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
        }

        // ── Pipeline registers for latency > 1 ───────────────────────────────
        if latency > 1 {
            let stages = latency - 1;
            let ff_sens = Self::ff_sensitivity(&clk, &rst, is_async, is_low);
            let rst_cond = Self::rst_condition(&rst, is_low);
            let num_req_str = &num_req_default;
            self.line("");

            for s in 0..stages {
                let src_gv = if s == 0 { gv_sig.clone() } else { format!("grant_valid_p{s}") };
                let src_gr = if s == 0 { gr_sig.clone() } else { format!("grant_requester_p{s}") };
                let src_rr = if s == 0 { rr_sig.clone() } else { format!("{req_ready_sig}_p{s}") };
                let dst_suffix = if s == stages - 1 {
                    // Last stage drives the output ports directly
                    String::new()
                } else {
                    format!("_p{}", s + 1)
                };

                let dst_gv = if dst_suffix.is_empty() { "grant_valid".to_string() } else { format!("grant_valid{dst_suffix}") };
                let dst_gr = if dst_suffix.is_empty() { "grant_requester".to_string() } else { format!("grant_requester{dst_suffix}") };
                let dst_rr = if dst_suffix.is_empty() { req_ready_sig.clone() } else { format!("{req_ready_sig}{dst_suffix}") };

                // Declare intermediate regs (not needed for last stage which drives output ports)
                if !dst_suffix.is_empty() {
                    self.line(&format!("logic {dst_gv};"));
                    self.line(&format!("logic [{req_width}-1:0] {dst_gr};"));
                    self.line(&format!("logic [{num_req_str}-1:0] {dst_rr};"));
                }

                self.line(&format!("always_ff @({ff_sens}) begin"));
                self.indent += 1;
                self.line(&format!("if ({rst_cond}) begin"));
                self.indent += 1;
                self.line(&format!("{dst_gv} <= 1'b0;"));
                self.line(&format!("{dst_gr} <= '0;"));
                self.line(&format!("{dst_rr} <= '0;"));
                self.indent -= 1;
                self.line("end else begin");
                self.indent += 1;
                self.line(&format!("{dst_gv} <= {src_gv};"));
                self.line(&format!("{dst_gr} <= {src_gr};"));
                self.line(&format!("{dst_rr} <= {src_rr};"));
                self.indent -= 1;
                self.line("end");
                self.indent -= 1;
                self.line("end");
            }
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Emit `dir type name` or `dir type [N-1:0] name` / `dir type name [0:N-1]`
    /// depending on type and whether count is 1.
    fn emit_port_array_signal_str(
        &self,
        dir: &str,
        ty: &TypeExpr,
        name: &str,
        count_str: &str,
    ) -> String {
        let is_scalar = count_str == "1";
        match ty {
            TypeExpr::Bool => {
                if is_scalar {
                    format!("{dir} logic {name}")
                } else {
                    format!("{dir} logic [{count_str}-1:0] {name}")
                }
            }
            _ => {
                let base = self.emit_port_type_str(ty);
                if is_scalar {
                    format!("{dir} {base} {name}")
                } else {
                    format!("{dir} {base} {name} [0:{count_str}-1]")
                }
            }
        }
    }


    fn emit_arbiter_round_robin(
        &mut self,
        clk: &str,
        rst: &str,
        is_async: bool,
        is_low: bool,
        req_width: u32,
        num_req: u64,
        req_valid: &str,
        req_ready: &str,
        grant_valid_sig: &str,
        grant_requester_sig: &str,
    ) {
        self.line(&format!("logic [{req_width}-1:0] rr_ptr_r;"));
        self.line("integer arb_i;");
        self.line("logic arb_found;");
        self.line("");

        let ff_sens = Self::ff_sensitivity(clk, rst, is_async, is_low);
        let rst_cond = Self::rst_condition(rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) rr_ptr_r <= '0;"));
        self.line(&format!("else if ({grant_valid_sig}) rr_ptr_r <= rr_ptr_r + 1;"));
        self.indent -= 1;
        self.line("end");
        self.line("");
        // Use a shared integer index to avoid width-expansion warnings
        self.line("always_comb begin");
        self.indent += 1;
        self.line(&format!("{grant_valid_sig} = 1'b0;"));
        self.line(&format!("{req_ready} = '0;"));
        self.line(&format!("{grant_requester_sig} = '0;"));
        self.line("arb_found = 1'b0;");
        self.line(&format!("for (arb_i = 0; arb_i < {num_req}; arb_i++) begin"));
        self.indent += 1;
        // All index arithmetic in integer domain; only cast at use sites
        self.line(&format!(
            "if (!arb_found && {req_valid}[(int'(rr_ptr_r) + arb_i) % {num_req}]) begin"
        ));
        self.indent += 1;
        self.line("arb_found = 1'b1;");
        self.line(&format!("{grant_valid_sig} = 1'b1;"));
        self.line(&format!(
            "{grant_requester_sig} = {req_width}'((int'(rr_ptr_r) + arb_i) % {num_req});"
        ));
        self.line(&format!(
            "{req_ready}[(int'(rr_ptr_r) + arb_i) % {num_req}] = 1'b1;"
        ));
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_arbiter_priority(&mut self, req_width: u32, num_req: u64, req_valid: &str, req_ready: &str, grant_valid_sig: &str, grant_requester_sig: &str) {
        self.line("always_comb begin");
        self.indent += 1;
        self.line(&format!("{grant_valid_sig} = 1'b0;"));
        self.line(&format!("{req_ready} = '0;"));
        self.line(&format!("{grant_requester_sig} = '0;"));
        self.line(&format!("for (int pri_i = 0; pri_i < {num_req}; pri_i++) begin"));
        self.indent += 1;
        self.line(&format!("if (!{grant_valid_sig} && {req_valid}[pri_i]) begin"));
        self.indent += 1;
        self.line(&format!("{grant_valid_sig} = 1'b1;"));
        self.line(&format!("{grant_requester_sig} = {req_width}'(pri_i);"));
        self.line(&format!("{req_ready}[pri_i] = 1'b1;"));
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_arbiter_custom(
        &mut self,
        a: &crate::ast::ArbiterDecl,
        fn_ident: &crate::ast::Ident,
        clk: &str,
        rst: &str,
        is_async: bool,
        is_low: bool,
        req_width: u32,
        num_req: u64,
        req_valid: &str,
        req_ready: &str,
        grant_valid_sig: &str,
        grant_requester_sig: &str,
    ) {
        let fn_name = &fn_ident.name;

        // last_grant_r register for fairness state
        self.line(&format!("logic [{num_req}-1:0] last_grant_r;"));
        self.line("");

        let ff_sens = Self::ff_sensitivity(clk, rst, is_async, is_low);
        let rst_cond = Self::rst_condition(rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) last_grant_r <= '0;"));
        self.line(&format!("else if ({grant_valid_sig}) last_grant_r <= grant_onehot;"));
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Build function call arguments from hook bindings
        let hook = a.hook.as_ref().unwrap();
        let args: Vec<String> = hook.fn_args.iter().map(|arg| {
            let name = &arg.name;
            // Map hook formal param names to actual SV signals
            let hook_param = hook.params.iter().find(|p| p.name.name == *name);
            if hook_param.is_some() {
                // This is a hook formal param — map known names to SV signals
                match name.as_str() {
                    "req_mask" => req_valid.to_string(),
                    "last_grant" => "last_grant_r".to_string(),
                    _ => name.clone(),
                }
            } else {
                // Must be a port or param name on the arbiter — use as-is
                name.clone()
            }
        }).collect();
        let args_str = args.join(", ");

        // Call the function to get one-hot grant mask
        self.line(&format!("logic [{num_req}-1:0] grant_onehot;"));
        self.line("");
        self.line("always_comb begin");
        self.indent += 1;
        self.line(&format!("grant_onehot = {fn_name}({args_str});"));
        self.line(&format!("{grant_valid_sig} = |grant_onehot;"));
        self.line(&format!("{req_ready} = grant_onehot;"));
        // Priority encode one-hot to index
        self.line(&format!("{grant_requester_sig} = '0;"));
        self.line(&format!("for (int ci = 0; ci < {num_req}; ci++) begin"));
        self.indent += 1;
        self.line(&format!("if (grant_onehot[ci]) {grant_requester_sig} = {req_width}'(ci);"));
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    // ── Regfile ───────────────────────────────────────────────────────────────

    fn emit_regfile(&mut self, r: &crate::ast::RegfileDecl) {
        use crate::ast::{ParamKind, ExprKind, LitKind};
        let n = &r.name.name.clone();

        // Helper: resolve a param by name to its default integer value.
        let param_int = |name: &str, default: u64| -> u64 {
            r.params.iter()
                .find(|p| p.name.name == name)
                .and_then(|p| p.default.as_ref())
                .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
                .unwrap_or(default)
        };

        // Helper: resolve a count_expr — literal or param-name reference.
        let resolve_count = |expr: &crate::ast::Expr| -> u64 {
            match &expr.kind {
                ExprKind::Literal(LitKind::Dec(v)) => *v,
                ExprKind::Ident(name) => param_int(name, 1),
                _ => 1,
            }
        };

        let nregs = param_int("NREGS", 32);

        // Data width: prefer the UInt<N> type of the data signal in the write port,
        // then fall back to XLEN/WIDTH/DATA_WIDTH params.
        let data_width_num: String = r.write_ports.as_ref()
            .and_then(|wp| wp.signals.iter().find(|s| s.name.name == "data"))
            .and_then(|s| if let TypeExpr::UInt(w) = &s.ty { Some(self.emit_expr_str(w)) } else { None })
            .or_else(|| {
                r.params.iter()
                    .find(|p| matches!(p.name.name.as_str(), "XLEN" | "WIDTH" | "DATA_WIDTH"))
                    .and_then(|p| match &p.kind {
                        ParamKind::Const => p.default.as_ref().map(|e| self.emit_expr_str(e)),
                        _ => None,
                    })
            })
            .unwrap_or_else(|| "32".to_string());

        // Determine addr width: ceil(log2(NREGS))
        let addr_width = if nregs <= 1 { 1u32 } else { (nregs as f64).log2().ceil() as u32 };

        // Read/write port counts — resolve param references
        let nread = r.read_ports.as_ref()
            .map(|rp| resolve_count(&rp.count_expr))
            .unwrap_or(1);
        let nwrite = r.write_ports.as_ref()
            .map(|wp| resolve_count(&wp.count_expr))
            .unwrap_or(1);

        let clk = r.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&r.ports);

        // ── Module header ─────────────────────────────────────────────────────
        // Emit one SV parameter per ARCH param declaration
        self.line(&format!("module {n} #("));
        self.indent += 1;
        let param_count = r.params.len();
        for (i, p) in r.params.iter().enumerate() {
            let comma = if i < param_count - 1 { "," } else { "" };
            let val = p.default.as_ref().map(|e| self.emit_expr_str(e)).unwrap_or_else(|| "0".to_string());
            self.line(&format!("parameter int {} = {}{}", p.name.name, val, comma));
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Build port list — multi-port groups are flattened to scalar ports
        // named {pfx}{i}_{signal} (e.g. read0_addr, read1_addr) so that each
        // can be connected individually via a `connect` statement.
        let mut all_ports: Vec<String> = Vec::new();
        for p in &r.ports {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        // Read ports — one set of scalars per port index
        if let Some(rp) = &r.read_ports {
            let pfx = &rp.name.name;
            for i in 0..nread {
                for s in &rp.signals {
                    let dir = match s.direction { Direction::In => "input", Direction::Out => "output" };
                    let flat_name = if nread == 1 {
                        format!("{pfx}_{}", s.name.name)
                    } else {
                        format!("{pfx}{i}_{}", s.name.name)
                    };
                    let port_str = self.emit_regfile_port_scalar(dir, &s.ty, &flat_name, addr_width, &data_width_num);
                    all_ports.push(port_str);
                }
            }
        }
        // Write ports — one set of scalars per port index
        if let Some(wp) = &r.write_ports {
            let pfx = &wp.name.name;
            for i in 0..nwrite {
                for s in &wp.signals {
                    let dir = match s.direction { Direction::In => "input", Direction::Out => "output" };
                    let flat_name = if nwrite == 1 {
                        format!("{pfx}_{}", s.name.name)
                    } else {
                        format!("{pfx}{i}_{}", s.name.name)
                    };
                    let port_str = self.emit_regfile_port_scalar(dir, &s.ty, &flat_name, addr_width, &data_width_num);
                    all_ports.push(port_str);
                }
            }
        }
        let port_count = all_ports.len();
        for (i, p) in all_ports.iter().enumerate() {
            let comma = if i < port_count - 1 { "," } else { "" };
            self.line(&format!("{p}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // ── Register array ────────────────────────────────────────────────────
        self.line(&format!("logic [{data_width_num}-1:0] rf_data [0:NREGS-1];"));
        self.line("");

        // ── Determine read/write port signal names (flat) ─────────────────────
        let write_pfx = r.write_ports.as_ref().map(|wp| wp.name.name.clone()).unwrap_or_else(|| "write".to_string());
        let read_pfx  = r.read_ports.as_ref().map(|rp| rp.name.name.clone()).unwrap_or_else(|| "read".to_string());

        // Flat name helper: "{pfx}{i}_{sig}" when count>1, else "{pfx}_{sig}"
        let flat = |pfx: &str, i: u64, count: u64, sig: &str| -> String {
            if count == 1 { format!("{pfx}_{sig}") } else { format!("{pfx}{i}_{sig}") }
        };

        // ── Write always_ff ───────────────────────────────────────────────────
        // Collect init-guarded addresses: init[k]=v means addr k is immutable
        // (implemented as a write guard), not as a reset.
        let guarded_addrs: Vec<String> = r.inits.iter()
            .map(|init| self.emit_expr_str(&init.index))
            .collect();

        // Only include reset sensitivity when a reset port is actually present
        // and there are reset-driven init entries. For register files that use
        // write guards for x0 (not reset), emit plain posedge-only always_ff.
        let has_reset_port = !rst.is_empty();
        let use_reset = has_reset_port && r.inits.iter().any(|_| false); // reserved for future explicit reset-on-init

        let ff_sens = if use_reset {
            Self::ff_sensitivity(&clk, &rst, is_async, is_low)
        } else {
            format!("posedge {clk}")
        };

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;

        if use_reset {
            let rst_cond = Self::rst_condition(&rst, is_low);
            self.line(&format!("if ({rst_cond}) begin"));
            self.indent += 1;
            for init in &r.inits {
                let idx = self.emit_expr_str(&init.index);
                let val = self.emit_expr_str(&init.value);
                self.line(&format!("rf_data[{idx}] <= {val};"));
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }

        // Unroll write ports; add address guards for init[k] entries
        for wi in 0..nwrite {
            let wen   = flat(&write_pfx, wi, nwrite, "en");
            let waddr = flat(&write_pfx, wi, nwrite, "addr");
            let wdata = flat(&write_pfx, wi, nwrite, "data");
            // Build guard: skip writes to any init-protected address
            let addr_guards: Vec<String> = guarded_addrs.iter()
                .map(|a| format!("{waddr} != {a}"))
                .collect();
            let guard = if addr_guards.is_empty() {
                wen.clone()
            } else {
                format!("{wen} && {}", addr_guards.join(" && "))
            };
            self.line(&format!("if ({guard})"));
            self.indent += 1;
            self.line(&format!("rf_data[{waddr}] <= {wdata};"));
            self.indent -= 1;
        }

        if use_reset {
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Read always_comb — unrolled per read port ─────────────────────────
        self.line("always_comb begin");
        self.indent += 1;
        for ri in 0..nread {
            let raddr = flat(&read_pfx, ri, nread, "addr");
            let rdata = flat(&read_pfx, ri, nread, "data");
            if r.forward_write_before_read {
                // Forward from the first write port (write port 0)
                let wen   = flat(&write_pfx, 0, nwrite, "en");
                let waddr = flat(&write_pfx, 0, nwrite, "addr");
                let wdata = flat(&write_pfx, 0, nwrite, "data");
                self.line(&format!("if ({wen} && {waddr} == {raddr})"));
                self.indent += 1;
                self.line(&format!("{rdata} = {wdata};"));
                self.indent -= 1;
                self.line("else");
                self.indent += 1;
                self.line(&format!("{rdata} = rf_data[{raddr}];"));
                self.indent -= 1;
            } else {
                self.line(&format!("{rdata} = rf_data[{raddr}];"));
            }
        }
        self.indent -= 1;
        self.line("end");

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Emit a single scalar regfile port signal declaration.
    fn emit_regfile_port_scalar(
        &self,
        dir: &str,
        ty: &TypeExpr,
        name: &str,
        addr_width: u32,
        data_width: &str,
    ) -> String {
        let phy_ty = match ty {
            TypeExpr::Bool => "logic".to_string(),
            TypeExpr::Named(id) if id.name == "WIDTH" || id.name == "DATA_WIDTH" => {
                format!("logic [{data_width}-1:0]")
            }
            TypeExpr::Named(id) if id.name == "ADDR_WIDTH" || id.name.to_lowercase().contains("addr") => {
                format!("logic [{addr_width}-1:0]")
            }
            TypeExpr::UInt(w) => {
                let ws = self.emit_expr_str(w);
                format!("logic [{ws}-1:0]")
            }
            _ => self.emit_port_type_str(ty),
        };
        format!("{dir} {phy_ty} {name}")
    }

    /// Map signal types: Named("WIDTH") → `logic [DATA_WIDTH-1:0]`; others pass through
    fn emit_ram_signal_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(ident) if ident.name == "WIDTH" => {
                "logic [DATA_WIDTH-1:0]".to_string()
            }
            other => self.emit_port_type_str(other),
        }
    }

    fn emit_ram_single(&mut self, r: &RamDecl, clk: &str, _data_width_ty: &str) {
        use crate::ast::RamWriteMode;
        // The single port group
        let pg = &r.port_groups[0];
        let pfx = &pg.name.name.clone();

        // Detect signal names
        let has_wen = pg.signals.iter().any(|s| s.name.name == "wen");
        let out_sig = pg.signals.iter().find(|s| s.direction == Direction::Out).cloned();

        match r.latency {
            0 => {
                // Combinational read
                if let Some(ref os) = out_sig {
                    self.line("");
                    self.line(&format!("assign {pfx}_{} = mem[{pfx}_addr];", os.name.name));
                }
                self.line("");
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                self.line(&format!("if ({pfx}_en && {pfx}_wen)"));
                self.indent += 1;
                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                self.indent -= 1;
                self.indent -= 1;
                self.line("end");
            }
            1 | 2 => {
                if let Some(ref os) = out_sig {
                    let rdata_r = format!("{pfx}_{}_r", os.name.name);
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                    self.line("");
                    let write_mode = r.write_mode.unwrap_or(RamWriteMode::NoChange);
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({pfx}_en) begin"));
                    self.indent += 1;
                    match write_mode {
                        RamWriteMode::WriteFirst => {
                            if has_wen {
                                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                            }
                            self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                        }
                        RamWriteMode::ReadFirst => {
                            self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                            if has_wen {
                                self.line(&format!("if ({pfx}_wen)"));
                                self.indent += 1;
                                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                                self.indent -= 1;
                            }
                        }
                        RamWriteMode::NoChange => {
                            if has_wen {
                                self.line(&format!("if ({pfx}_wen)"));
                                self.indent += 1;
                                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                                self.indent -= 1;
                                self.line("else");
                                self.indent += 1;
                                self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                                self.indent -= 1;
                            } else {
                                self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                            }
                        }
                    }
                    self.indent -= 1;
                    self.line("end");
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("assign {pfx}_{} = {rdata_r};", os.name.name));
                    // latency 2 adds an extra output register stage
                    if r.latency == 2 {
                        let rdata_r2 = format!("{pfx}_{}_r2", os.name.name);
                        self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r2};"));
                        self.line(&format!("always_ff @(posedge {clk}) {rdata_r2} <= {rdata_r};"));
                        self.line(&format!("assign {pfx}_{} = {rdata_r2};", os.name.name));
                    }
                }
            }
            _ => {}
        }
    }

    fn emit_ram_simple_dual(&mut self, r: &RamDecl, clk: &str, _data_width_ty: &str) {
        // Identify read port (has output signal) and write port (all inputs)
        let read_pg = r.port_groups.iter()
            .find(|pg| pg.signals.iter().any(|s| s.direction == Direction::Out));
        let write_pg = r.port_groups.iter()
            .find(|pg| pg.signals.iter().all(|s| s.direction == Direction::In));

        let (rpfx, wpfx) = match (read_pg, write_pg) {
            (Some(rp), Some(wp)) => (rp.name.name.clone(), wp.name.name.clone()),
            _ => return, // malformed
        };
        let out_sig = read_pg.unwrap().signals.iter()
            .find(|s| s.direction == Direction::Out)
            .map(|s| s.name.name.clone())
            .unwrap_or_else(|| "data".to_string());

        // Find write data signal (input data in write port)
        let wdata_sig = write_pg.unwrap().signals.iter()
            .find(|s| s.direction == Direction::In
                && !["en", "addr", "mask", "wen"].contains(&s.name.name.as_str()))
            .map(|s| s.name.name.clone())
            .unwrap_or_else(|| "data".to_string());

        match r.latency {
            0 => {
                self.line("");
                self.line(&format!("assign {rpfx}_{out_sig} = mem[{rpfx}_addr];"));
                self.line("");
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                self.line(&format!("if ({wpfx}_en)"));
                self.indent += 1;
                self.line(&format!("mem[{wpfx}_addr] <= {wpfx}_{wdata_sig};"));
                self.indent -= 1;
                self.indent -= 1;
                self.line("end");
            }
            1 | 2 => {
                let rdata_r = format!("{rpfx}_{out_sig}_r");
                self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                self.line("");
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                self.line(&format!("if ({wpfx}_en)"));
                self.indent += 1;
                self.line(&format!("mem[{wpfx}_addr] <= {wpfx}_{wdata_sig};"));
                self.indent -= 1;
                self.line(&format!("if ({rpfx}_en)"));
                self.indent += 1;
                self.line(&format!("{rdata_r} <= mem[{rpfx}_addr];"));
                self.indent -= 1;
                self.indent -= 1;
                self.line("end");
                if r.latency == 2 {
                    let rdata_r2 = format!("{rpfx}_{out_sig}_r2");
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r2};"));
                    self.line(&format!("always_ff @(posedge {clk}) {rdata_r2} <= {rdata_r};"));
                    self.line(&format!("assign {rpfx}_{out_sig} = {rdata_r2};"));
                } else {
                    self.line(&format!("assign {rpfx}_{out_sig} = {rdata_r};"));
                }
            }
            _ => {}
        }
    }

    fn emit_ram_true_dual(&mut self, r: &RamDecl, clk: &str, _data_width_ty: &str) {
        // Both port groups can read and write
        let pa = &r.port_groups[0];
        let pb = &r.port_groups[1];
        let pfx_a = pa.name.name.clone();
        let pfx_b = pb.name.name.clone();

        let rdata_a = pa.signals.iter().find(|s| s.direction == Direction::Out)
            .map(|s| s.name.name.clone()).unwrap_or_else(|| "rdata".to_string());
        let rdata_b = pb.signals.iter().find(|s| s.direction == Direction::Out)
            .map(|s| s.name.name.clone()).unwrap_or_else(|| "rdata".to_string());

        let rdata_a_r = format!("{pfx_a}_{rdata_a}_r");
        let rdata_b_r = format!("{pfx_b}_{rdata_b}_r");
        self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_a_r};"));
        self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_b_r};"));
        self.line("");
        self.line(&format!("always_ff @(posedge {clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({pfx_a}_en) begin"));
        self.indent += 1;
        self.line(&format!("if ({pfx_a}_wen)"));
        self.indent += 1;
        self.line(&format!("mem[{pfx_a}_addr] <= {pfx_a}_wdata;"));
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        self.line(&format!("{rdata_a_r} <= mem[{pfx_a}_addr];"));
        self.indent -= 1;
        self.indent -= 1;
        self.line("end");
        self.line(&format!("if ({pfx_b}_en) begin"));
        self.indent += 1;
        self.line(&format!("if ({pfx_b}_wen)"));
        self.indent += 1;
        self.line(&format!("mem[{pfx_b}_addr] <= {pfx_b}_wdata;"));
        self.indent -= 1;
        self.line("else");
        self.indent += 1;
        self.line(&format!("{rdata_b_r} <= mem[{pfx_b}_addr];"));
        self.indent -= 1;
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line(&format!("assign {pfx_a}_{rdata_a} = {rdata_a_r};"));
        self.line(&format!("assign {pfx_b}_{rdata_b} = {rdata_b_r};"));
    }

    // ── Linklist ─────────────────────────────────────────────────────────────

    fn emit_linklist(&mut self, l: &crate::ast::LinklistDecl) {
        use crate::ast::LinklistKind;
        let n = &l.name.name;
        let is_doubly = matches!(l.kind, LinklistKind::Doubly | LinklistKind::CircularDoubly);
        let is_circular = matches!(l.kind, LinklistKind::CircularSingly | LinklistKind::CircularDoubly);

        // Resolve DEPTH default expression and DATA SV type
        let depth_expr = l.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "16".to_string());

        let data_default_sv = l.params.iter()
            .find(|p| p.name.name == "DATA")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => Some(self.emit_port_type_str(ty)),
                _ => None,
            })
            .unwrap_or_else(|| "logic [7:0]".to_string());

        // Find clk/rst port names
        let clk_name = l.ports.iter()
            .find(|p| matches!(&p.ty, crate::ast::TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");
        let rst_name = l.ports.iter()
            .find(|p| matches!(&p.ty, crate::ast::TypeExpr::Reset(_, _)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("rst");

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        self.line(&format!("parameter int  DEPTH = {depth_expr},"));
        self.line(&format!("parameter type DATA  = {data_default_sv}"));
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // clk / rst ports
        self.line(&format!("input  logic {clk_name},"));
        self.line(&format!("input  logic {rst_name},"));

        // Op ports — one group per declared op
        let all_ops = &l.ops;
        let status_ports: Vec<&crate::ast::PortDecl> = l.ports.iter()
            .filter(|p| !matches!(&p.ty, crate::ast::TypeExpr::Clock(_) | crate::ast::TypeExpr::Reset(_, _)))
            .collect();

        // Collect all port lines then emit with trailing comma logic
        let mut port_lines: Vec<String> = Vec::new();
        for op in all_ops {
            for p in &op.ports {
                let dir = match p.direction { Direction::In => "input ", Direction::Out => "output" };
                let ty_str = self.emit_ll_port_type(&p.ty);
                port_lines.push(format!("{dir} {ty_str} {}_{}", op.name.name, p.name.name));
            }
        }
        for p in &status_ports {
            let dir = match p.direction { Direction::In => "input ", Direction::Out => "output" };
            let ty_str = self.emit_ll_port_type(&p.ty);
            port_lines.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        for (i, line) in port_lines.iter().enumerate() {
            let comma = if i < port_lines.len() - 1 { "," } else { "" };
            self.line(&format!("{line}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // ── Internal constants ────────────────────────────────────────────────
        self.line("localparam int HANDLE_W = $clog2(DEPTH);");
        self.line("localparam int CNT_W    = $clog2(DEPTH + 1);");
        self.line("");

        // ── Free list: circular FIFO of slot indices ──────────────────────────
        self.line("// Free list — circular FIFO of available slot indices");
        self.line("logic [HANDLE_W-1:0] _fl_mem  [0:DEPTH-1];");
        self.line("logic [CNT_W-1:0]    _fl_rdp;");
        self.line("logic [CNT_W-1:0]    _fl_wrp;");
        self.line("logic [CNT_W-1:0]    _fl_cnt;");
        self.line("");

        // ── Payload and link RAMs ─────────────────────────────────────────────
        self.line("// Payload and link RAMs");
        self.line("DATA                 _data_mem [0:DEPTH-1];");
        self.line("logic [HANDLE_W-1:0] _next_mem [0:DEPTH-1];");
        if is_doubly {
            self.line("logic [HANDLE_W-1:0] _prev_mem [0:DEPTH-1];");
        }
        self.line("");

        // ── Head / tail / length registers ───────────────────────────────────
        self.line("// Head / tail registers");
        self.line("logic [HANDLE_W-1:0] _head_r;");
        if l.track_tail {
            self.line("logic [HANDLE_W-1:0] _tail_r;");
        }
        self.line("");

        // ── Per-op controller registers ───────────────────────────────────────
        for op in all_ops {
            let on = &op.name.name;
            // Every op gets a busy flag (for latency > 1) and resp_valid pipeline
            self.line(&format!("// {on} controller registers"));
            if op.latency > 1 {
                self.line(&format!("logic _ctrl_{on}_busy;"));
            }
            // resp_valid output register
            let has_resp_valid = op.ports.iter().any(|p| p.name.name == "resp_valid");
            if has_resp_valid {
                self.line(&format!("logic _ctrl_{on}_resp_v;"));
            }
            // latch any output data ports
            for p in op.ports.iter().filter(|p| p.direction == Direction::Out && p.name.name != "req_ready" && p.name.name != "resp_valid") {
                let ty = self.emit_ll_port_type(&p.ty);
                self.line(&format!("{ty} _ctrl_{on}_{};", p.name.name));
            }
            // Op-specific internal temporaries
            match on.as_str() {
                "delete_head" | "delete" => {
                    self.line(&format!("logic [HANDLE_W-1:0] _ctrl_{on}_slot;"));
                }
                "insert_tail" | "insert_head" => {
                    self.line(&format!("logic _ctrl_{on}_was_empty;"));
                }
                "insert_after" => {
                    self.line(&format!("logic [HANDLE_W-1:0] _ctrl_{on}_after_handle;"));
                }
                _ => {}
            }
            self.line("");
        }

        // ── Status assigns ────────────────────────────────────────────────────
        self.line("// Status outputs");
        // empty: free list count == DEPTH (all slots available = list is empty)
        if status_ports.iter().any(|p| p.name.name == "empty") {
            self.line("assign empty  = (_fl_cnt == CNT_W'(DEPTH));");
        }
        // full: free list count == 0 (no slots available = list is full)
        if status_ports.iter().any(|p| p.name.name == "full") {
            self.line("assign full   = (_fl_cnt == '0);");
        }
        // length: occupied slots = DEPTH - free count
        if status_ports.iter().any(|p| p.name.name == "length") {
            self.line("assign length = CNT_W'(DEPTH) - _fl_cnt;");
        }

        // req_ready assigns (combinational: not busy and not full/empty as applicable)
        self.line("");
        self.line("// req_ready: combinational");
        for op in all_ops {
            let on = &op.name.name;
            if op.ports.iter().any(|p| p.name.name == "req_ready") {
                let guard = if op.latency > 1 {
                    format!("!_ctrl_{on}_busy && ")
                } else {
                    String::new()
                };
                let cond = match on.as_str() {
                    "alloc" | "insert_head" | "insert_tail" | "insert_after" => {
                        format!("{guard}!(_fl_cnt == '0)")
                    }
                    "free" | "delete_head" | "delete" => {
                        format!("{guard}!(_fl_cnt == CNT_W'(DEPTH))")
                    }
                    _ => format!("{guard}1'b1"),
                };
                self.line(&format!("assign {on}_req_ready = {cond};"));
            }
            // wire resp_valid output from register
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                self.line(&format!("assign {on}_resp_valid = _ctrl_{on}_resp_v;"));
            }
            // wire other output data ports
            for p in op.ports.iter().filter(|p| p.direction == Direction::Out && p.name.name != "req_ready" && p.name.name != "resp_valid") {
                self.line(&format!("assign {}_{} = _ctrl_{on}_{};", on, p.name.name, p.name.name));
            }
        }
        self.line("");

        // ── Reset + free-list init + op controllers ───────────────────────────
        self.line(&format!("integer _ll_i;"));
        self.line(&format!("always_ff @(posedge {clk_name}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_name}) begin"));
        self.indent += 1;
        self.line("for (_ll_i = 0; _ll_i < DEPTH; _ll_i++)");
        self.indent += 1;
        self.line("_fl_mem[_ll_i] <= HANDLE_W'(_ll_i);");
        self.indent -= 1;
        self.line("_fl_rdp <= '0;");
        self.line("_fl_wrp <= '0;");
        self.line("_fl_cnt <= CNT_W'(DEPTH);");
        self.line("_head_r <= '0;");
        if l.track_tail { self.line("_tail_r <= '0;"); }
        for op in all_ops {
            let on = &op.name.name;
            if op.latency > 1 { self.line(&format!("_ctrl_{on}_busy <= 1'b0;")); }
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                self.line(&format!("_ctrl_{on}_resp_v <= 1'b0;"));
            }
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;

        // Clear resp_valid by default each cycle (pulse behaviour)
        for op in all_ops {
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                self.line(&format!("_ctrl_{}_resp_v <= 1'b0;", op.name.name));
            }
        }
        self.line("");

        // Per-op logic
        for op in all_ops {
            self.emit_ll_op_controller(op, l.track_tail, is_doubly, is_circular);
        }

        self.indent -= 1;
        self.line("end"); // else
        self.indent -= 1;
        self.line("end"); // always_ff
        self.line("");

        self.indent -= 1;
        self.line("endmodule");
        self.line("");
    }

    /// Emit SV type string for a linklist port — DATA named type → "DATA".
    fn emit_ll_port_type(&self, ty: &crate::ast::TypeExpr) -> String {
        match ty {
            crate::ast::TypeExpr::Named(id) if id.name == "DATA" => "DATA".to_string(),
            crate::ast::TypeExpr::Bool => "logic".to_string(),
            other => self.emit_port_type_str(other),
        }
    }

    /// Emit the always_ff body for one declared op.
    fn emit_ll_op_controller(
        &mut self,
        op: &crate::ast::OpDecl,
        track_tail: bool,
        is_doubly: bool,
        _is_circular: bool,
    ) {
        let on = &op.name.name;
        let has_req_valid   = op.ports.iter().any(|p| p.name.name == "req_valid");
        let has_resp_valid  = op.ports.iter().any(|p| p.name.name == "resp_valid");
        let has_req_handle  = op.ports.iter().any(|p| p.name.name == "req_handle");
        let has_req_data    = op.ports.iter().any(|p| p.name.name == "req_data");

        self.line(&format!("// ── {on} ─────────────────────────────────────────"));

        match on.as_str() {
            "alloc" => {
                // Latency-1: dequeue one slot from free list
                let guard = if has_req_valid { format!("{on}_req_valid && !(_fl_cnt == '0)") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                if has_resp_valid {
                    self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;"));
                    self.line(&format!("_ctrl_{on}_resp_handle <= _fl_mem[_fl_rdp[HANDLE_W-1:0]];"));
                }
                self.indent -= 1;
                self.line("end");
            }
            "free" => {
                // Latency-1: enqueue slot back onto free list
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_fl_mem[_fl_wrp[HANDLE_W-1:0]] <= {on}_req_handle;"));
                }
                self.line("_fl_wrp <= _fl_wrp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt + 1'b1;");
                self.indent -= 1;
                self.line("end");
            }
            "insert_head" => {
                // Latency-2: alloc slot, write data, update head
                if op.latency >= 2 {
                    let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && !(_fl_cnt == '0)");
                    self.line(&format!("if ({guard}) begin"));
                    self.indent += 1;
                    let slot = format!("_fl_mem[_fl_rdp[HANDLE_W-1:0]]");
                    self.line(&format!("_ctrl_{on}_resp_handle <= {slot};"));
                    if has_req_data {
                        self.line(&format!("_data_mem[{slot}] <= {on}_req_data;"));
                    }
                    self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                    self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                    self.line(&format!("_ctrl_{on}_was_empty <= (_fl_cnt == CNT_W'(DEPTH));"));
                    self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                    self.indent -= 1;
                    self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                    self.indent += 1;
                    self.line(&format!("_next_mem[_ctrl_{on}_resp_handle] <= _head_r;"));
                    if is_doubly {
                        // old head.prev = new node; new node.prev = sentinel (0)
                        self.line(&format!("_prev_mem[_head_r] <= _ctrl_{on}_resp_handle;"));
                    }
                    self.line(&format!("_head_r <= _ctrl_{on}_resp_handle;"));
                    if track_tail {
                        self.line(&format!("if (_ctrl_{on}_was_empty) _tail_r <= _ctrl_{on}_resp_handle;"));
                    }
                    if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                    self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                    self.indent -= 1;
                    self.line("end");
                } else {
                    // Latency-1 shortcut (caller's responsibility to allow 2-cycle settling)
                    let slot = "_fl_mem[_fl_rdp[HANDLE_W-1:0]]";
                    self.line(&format!("if ({on}_req_valid && !(_fl_cnt == '0)) begin"));
                    self.indent += 1;
                    if has_req_data { self.line(&format!("_data_mem[{slot}] <= {on}_req_data;")); }
                    self.line(&format!("_next_mem[{slot}] <= _head_r;"));
                    self.line(&format!("_head_r <= {slot};"));
                    self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                    self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                    if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                    self.indent -= 1;
                    self.line("end");
                }
            }
            "insert_tail" => {
                // Latency-2: alloc, write data, patch tail's next, update tail
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && !(_fl_cnt == '0)");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                let slot = "_fl_mem[_fl_rdp[HANDLE_W-1:0]]";
                self.line(&format!("_ctrl_{on}_resp_handle <= {slot};"));
                if has_req_data { self.line(&format!("_data_mem[{slot}] <= {on}_req_data;")); }
                self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                self.line(&format!("_ctrl_{on}_was_empty <= (_fl_cnt == CNT_W'(DEPTH));"));
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                if track_tail {
                    self.line(&format!("if (!_ctrl_{on}_was_empty) _next_mem[_tail_r] <= _ctrl_{on}_resp_handle;"));
                    if is_doubly {
                        // new node.prev = old tail
                        self.line(&format!("_prev_mem[_ctrl_{on}_resp_handle] <= _tail_r;"));
                    }
                    self.line(&format!("_tail_r <= _ctrl_{on}_resp_handle;"));
                    self.line(&format!("if (_ctrl_{on}_was_empty) _head_r <= _ctrl_{on}_resp_handle;"));
                } else {
                    self.line(&format!("if (!_ctrl_{on}_was_empty) _next_mem[_head_r] <= _ctrl_{on}_resp_handle;"));
                    self.line(&format!("if (_ctrl_{on}_was_empty) _head_r <= _ctrl_{on}_resp_handle;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            "delete_head" => {
                // Latency-2: read head data, advance head, free old head slot
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && !(_fl_cnt == CNT_W'(DEPTH))");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                self.line("_ctrl_delete_head_resp_data <= _data_mem[_head_r];");
                self.line("_ctrl_delete_head_slot      <= _head_r;");
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                // Free the old head slot
                self.line("_fl_mem[_fl_wrp[HANDLE_W-1:0]] <= _ctrl_delete_head_slot;");
                self.line("_fl_wrp <= _fl_wrp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt + 1'b1;");
                // Advance head
                self.line("_head_r <= _next_mem[_ctrl_delete_head_slot];");
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            "read_data" => {
                // Latency-1: RAM read (registered output)
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_resp_data <= _data_mem[{on}_req_handle];"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "write_data" => {
                // Latency-1: RAM write
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle && has_req_data {
                    self.line(&format!("_data_mem[{on}_req_handle] <= {on}_req_data;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "next" => {
                // Latency-1: follow next pointer
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_resp_handle <= _next_mem[{on}_req_handle];"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "prev" => {
                // Latency-1: follow prev pointer (doubly only)
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_resp_handle <= _prev_mem[{on}_req_handle];"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "insert_after" => {
                // Latency-2: alloc, write data+next link; cycle 2 patches after.next (and prev ptrs)
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && !(_fl_cnt == '0)");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                let slot = "_fl_mem[_fl_rdp[HANDLE_W-1:0]]";
                self.line(&format!("_ctrl_{on}_resp_handle <= {slot};"));
                if has_req_data { self.line(&format!("_data_mem[{slot}] <= {on}_req_data;")); }
                // Latch after_handle so cycle 2 doesn't read live port
                self.line(&format!("_ctrl_{on}_after_handle <= {on}_req_handle;"));
                // new.next = after.next (the successor)
                self.line(&format!("_next_mem[{slot}] <= _next_mem[{on}_req_handle];"));
                self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                // after.next = new
                self.line(&format!("_next_mem[_ctrl_{on}_after_handle] <= _ctrl_{on}_resp_handle;"));
                if is_doubly {
                    // new.prev = after
                    self.line(&format!("_prev_mem[_ctrl_{on}_resp_handle] <= _ctrl_{on}_after_handle;"));
                    // successor.prev = new  (new.next is already committed from cycle 1)
                    self.line(&format!("_prev_mem[_next_mem[_ctrl_{on}_resp_handle]] <= _ctrl_{on}_resp_handle;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            "delete" => {
                // Latency-2 (doubly): unlink by patching prev.next and next.prev
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_slot <= {on}_req_handle;"));
                }
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                self.line(&format!("_fl_mem[_fl_wrp[HANDLE_W-1:0]] <= _ctrl_{on}_slot;"));
                self.line("_fl_wrp <= _fl_wrp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt + 1'b1;");
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            _ => {
                // Unknown op — emit a comment placeholder
                self.line(&format!("// op `{on}` — not implemented"));
            }
        }
        self.line("");
    }
}

