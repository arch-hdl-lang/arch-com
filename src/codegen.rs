use crate::ast::*;
use crate::diagnostics::CompileWarning;
use crate::lexer::Span;
use crate::resolve::SymbolTable;
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
    /// When inside a `reg on ... rst low` block, holds the reset signal name
    /// so that bare references to it in expressions are emitted as `(!name)`.
    active_low_rst: Option<String>,
}

impl<'a> Codegen<'a> {
    pub fn new(symbols: &'a SymbolTable, source: &'a SourceFile) -> Self {
        Self {
            symbols,
            source,
            out: String::new(),
            indent: 0,
            warnings: Vec::new(),
            comments: Vec::new(),
            comment_idx: 0,
            active_low_rst: None,
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
        for item in &self.source.items {
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
            }
        }
        // Flush any trailing comments after the last item.
        let end = usize::MAX;
        self.emit_comments_before(end);
        self.out
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

        // Single pass in source order; interleave comments by byte position.
        // We need a clone of m to satisfy the borrow checker when calling
        // emit_reg_block (which takes &ModuleDecl) while also mutating self.
        let body_items: Vec<ModuleBodyItem> = m.body.clone();
        let m_clone = m.clone();
        for item in &body_items {
            self.emit_comments_before(item.span().start);
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    let ty_str = self.emit_logic_type_str(&r.ty);
                    let init_str = self.emit_expr_str(&r.init);
                    self.line(&format!("{} {} = {};", ty_str, r.name.name, init_str));
                }
                ModuleBodyItem::LetBinding(l) => {
                    let val_str = self.emit_expr_str(&l.value);
                    if let Some(ty) = &l.ty {
                        let ty_str = self.emit_logic_type_str(ty);
                        self.line(&format!("{} {};", ty_str, l.name.name));
                        self.line(&format!("assign {} = {};", l.name.name, val_str));
                    } else {
                        self.line(&format!("logic {} = {};", l.name.name, val_str));
                    }
                }
                ModuleBodyItem::CombBlock(cb) => self.emit_comb_block(cb),
                ModuleBodyItem::RegBlock(rb) => self.emit_reg_block(rb, &m_clone),
                ModuleBodyItem::Inst(inst) => self.emit_inst(inst),
                ModuleBodyItem::Generate(_) => {} // expanded before codegen
            }
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
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
                let cond = self.emit_expr_str(&ie.cond);
                self.line(&format!("if ({}) begin", cond));
                self.indent += 1;
                for s in &ie.then_stmts {
                    self.emit_comb_stmt(s);
                }
                self.indent -= 1;
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
        }
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

        let is_async_reset = self.is_async_reset(&rb.reset.name, m);
        let is_low = rb.reset_level == ResetLevel::Low;
        let rst_name = rb.reset.name.clone();
        let rst_edge = if is_low { "negedge" } else { "posedge" };
        let rst_cond_str = if is_low {
            format!("(!{})", rst_name)
        } else {
            rst_name.clone()
        };

        if is_async_reset {
            self.line(&format!(
                "always_ff @({clk_edge} {} or {rst_edge} {rst_name}) begin",
                rb.clock.name
            ));
        } else {
            self.line(&format!("always_ff @({clk_edge} {}) begin", rb.clock.name));
        }
        self.indent += 1;

        let user_has_rst_guard = Self::has_rst_guard(&rb.stmts, &rst_name, is_low);

        if user_has_rst_guard {
            // User wrote `if rst` (or `if not rst`) — emit body as-is.
            // active_low_rst causes bare rst references in expressions to be
            // emitted as (!rst), which is correct for the user-authored guard.
            if is_low {
                self.active_low_rst = Some(rst_name.clone());
            }
            for stmt in &rb.stmts {
                self.emit_reg_stmt(stmt);
            }
            self.active_low_rst = None;
        } else {
            // No explicit reset guard — auto-generate one from init values.
            let mut assigned = std::collections::BTreeSet::new();
            Self::collect_assigned_roots(&rb.stmts, &mut assigned);

            // Pre-compute (name, init_str) so emit_expr_str borrows are scoped.
            let mut resets: Vec<(String, String)> = Vec::new();
            for name in &assigned {
                if name.is_empty() {
                    continue;
                }
                let init = m.body.iter()
                    .filter_map(|i| {
                        if let ModuleBodyItem::RegDecl(r) = i { Some(r) } else { None }
                    })
                    .find(|r| r.name.name == *name)
                    .map(|r| self.emit_expr_str(&r.init))
                    .unwrap_or_else(|| "'0".to_string());
                resets.push((name.clone(), init));
            }

            self.line(&format!("if ({rst_cond_str}) begin"));
            self.indent += 1;
            for (name, init) in &resets {
                self.line(&format!("{name} <= {init};"));
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            for stmt in &rb.stmts {
                self.emit_reg_stmt(stmt);
            }
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("end");
    }

    /// Returns true if any top-level statement is an if-reset guard.
    fn has_rst_guard(stmts: &[Stmt], rst_name: &str, is_low: bool) -> bool {
        stmts.iter().any(|s| {
            if let Stmt::IfElse(ie) = s {
                Self::is_rst_cond(&ie.cond, rst_name, is_low)
            } else {
                false
            }
        })
    }

    fn is_rst_cond(expr: &Expr, rst_name: &str, _is_low: bool) -> bool {
        match &expr.kind {
            // User writes `if rst` (high) or `if rst_n` (low — inverted by active_low_rst).
            ExprKind::Ident(n) => n == rst_name,
            // User writes `if not rst` — also accepted as a guard.
            ExprKind::Unary(UnaryOp::Not, inner) => {
                matches!(&inner.kind, ExprKind::Ident(n) if n == rst_name)
            }
            _ => false,
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

    fn is_async_reset(&self, reset_name: &str, m: &ModuleDecl) -> bool {
        for p in &m.ports {
            if p.name.name == reset_name {
                if let TypeExpr::Reset(ResetKind::Async) = &p.ty {
                    return true;
                }
            }
        }
        false
    }

    fn emit_reg_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign(a) => {
                let target = self.emit_expr_str(&a.target);
                let val = self.emit_expr_str(&a.value);
                self.line(&format!("{} <= {};", target, val));
            }
            Stmt::IfElse(ie) => {
                let cond = self.emit_expr_str(&ie.cond);
                self.line(&format!("if ({}) begin", cond));
                self.indent += 1;
                for s in &ie.then_stmts {
                    self.emit_reg_stmt(s);
                }
                self.indent -= 1;
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
        let rst_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Reset(_)));
        let clk_name = clk_port.map(|p| p.name.name.as_str()).unwrap_or("clk");
        let rst_name = rst_port.map(|p| p.name.name.as_str()).unwrap_or("rst");
        let is_async = rst_port.map(|p| matches!(&p.ty, TypeExpr::Reset(ResetKind::Async))).unwrap_or(false);

        // ── State register FF ────────────────────────────────────────────────
        if is_async {
            self.line(&format!("always_ff @(posedge {clk_name} or posedge {rst_name}) begin"));
        } else {
            self.line(&format!("always_ff @(posedge {clk_name}) begin"));
        }
        self.indent += 1;
        self.line(&format!("if ({rst_name}) begin"));
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

        // Determine reset port name
        let rst = f.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("rst");
        let clk = f.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");

        self.line(&format!("always_ff @(posedge {clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst}) begin"));
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
        let rst = f.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("rst");

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
        self.line(&format!("always_ff @(posedge {rd_clk} or posedge {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst}) begin wr_ptr_gray_s1 <= '0; wr_ptr_gray_sync <= '0; end"));
        self.line("else begin wr_ptr_gray_s1 <= wr_ptr_gray; wr_ptr_gray_sync <= wr_ptr_gray_s1; end");
        self.indent -= 1;
        self.line("end");
        self.line(&format!("// Sync rd_ptr into wr domain ({wr_clk})"));
        self.line(&format!("always_ff @(posedge {wr_clk} or posedge {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst}) begin rd_ptr_gray_s1 <= '0; rd_ptr_gray_sync <= '0; end"));
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
        self.line(&format!("always_ff @(posedge {wr_clk} or posedge {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst}) wr_ptr_bin <= '0;"));
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
        self.line(&format!("always_ff @(posedge {rd_clk} or posedge {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst}) rd_ptr_bin <= '0;"));
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
                // Inside an active-low reset block, invert bare references to
                // the reset signal so `if rst` correctly emits `if (!rst_n)`.
                if self.active_low_rst.as_deref() == Some(name.as_str()) {
                    format!("(!{name})")
                } else {
                    name.clone()
                }
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
            TypeExpr::Reset(_) => "logic".to_string(),
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
            TypeExpr::Reset(_) => "logic".to_string(),
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
        let rst = c.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "rst".to_string());

        let is_async = c.ports.iter()
            .any(|p| matches!(&p.ty, TypeExpr::Reset(crate::ast::ResetKind::Async)));

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
        let ff_sens = if is_async {
            format!("posedge {clk} or posedge {rst}")
        } else {
            format!("posedge {clk}")
        };

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;

        // Reset branch
        let rst_cond = if is_async { rst.clone() } else { rst.clone() };
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
        let rst = a.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "rst".to_string());
        let is_async = a.ports.iter()
            .any(|p| matches!(&p.ty, TypeExpr::Reset(crate::ast::ResetKind::Async)));

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

        let policy = a.policy.clone();
        // ── Arbiter logic ─────────────────────────────────────────────────────
        match policy {
            ArbiterPolicy::RoundRobin => {
                self.emit_arbiter_round_robin(&clk, &rst, is_async, req_width, num_req_int, &req_valid_sig, &req_ready_sig);
            }
            ArbiterPolicy::Priority => {
                self.emit_arbiter_priority(req_width, num_req_int, &req_valid_sig, &req_ready_sig);
            }
            ArbiterPolicy::Lru => {
                self.emit_arbiter_round_robin(&clk, &rst, is_async, req_width, num_req_int, &req_valid_sig, &req_ready_sig);
            }
            ArbiterPolicy::Weighted(_) => {
                self.emit_arbiter_priority(req_width, num_req_int, &req_valid_sig, &req_ready_sig);
            }
            ArbiterPolicy::Custom => {
                self.line("// custom arbiter — implement grant logic here");
                self.line("assign grant_valid = '0;");
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
        req_width: u32,
        num_req: u64,
        req_valid: &str,
        req_ready: &str,
    ) {
        self.line(&format!("logic [{req_width}-1:0] rr_ptr_r;"));
        self.line("integer arb_i;");
        self.line("logic arb_found;");
        self.line("");

        let ff_sens = if is_async {
            format!("posedge {clk} or posedge {rst}")
        } else {
            format!("posedge {clk}")
        };

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst}) rr_ptr_r <= '0;"));
        self.line("else if (grant_valid) rr_ptr_r <= rr_ptr_r + 1;");
        self.indent -= 1;
        self.line("end");
        self.line("");
        // Use a shared integer index to avoid width-expansion warnings
        self.line("always_comb begin");
        self.indent += 1;
        self.line("grant_valid = 1'b0;");
        self.line(&format!("{req_ready} = '0;"));
        self.line("grant_requester = '0;");
        self.line("arb_found = 1'b0;");
        self.line(&format!("for (arb_i = 0; arb_i < {num_req}; arb_i++) begin"));
        self.indent += 1;
        // All index arithmetic in integer domain; only cast at use sites
        self.line(&format!(
            "if (!arb_found && {req_valid}[(int'(rr_ptr_r) + arb_i) % {num_req}]) begin"
        ));
        self.indent += 1;
        self.line("arb_found = 1'b1;");
        self.line("grant_valid = 1'b1;");
        self.line(&format!(
            "grant_requester = {req_width}'((int'(rr_ptr_r) + arb_i) % {num_req});"
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

    fn emit_arbiter_priority(&mut self, req_width: u32, num_req: u64, req_valid: &str, req_ready: &str) {
        self.line("always_comb begin");
        self.indent += 1;
        self.line("grant_valid = 1'b0;");
        self.line(&format!("{req_ready} = '0;"));
        self.line("grant_requester = '0;");
        self.line(&format!("for (int pri_i = 0; pri_i < {num_req}; pri_i++) begin"));
        self.indent += 1;
        self.line(&format!("if (!grant_valid && {req_valid}[pri_i]) begin"));
        self.indent += 1;
        self.line("grant_valid = 1'b1;");
        self.line(&format!("grant_requester = {req_width}'(pri_i);"));
        self.line(&format!("{req_ready}[pri_i] = 1'b1;"));
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    // ── Regfile ───────────────────────────────────────────────────────────────

    fn emit_regfile(&mut self, r: &crate::ast::RegfileDecl) {
        let n = &r.name.name.clone();

        // Find NREGS, DATA_WIDTH params
        let nregs_str = r.params.iter()
            .find(|p| p.name.name == "NREGS")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "32".to_string());

        let data_width_ty = r.params.iter()
            .find(|p| p.name.name == "WIDTH" || p.name.name == "DATA_WIDTH")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => Some(self.emit_port_type_str(ty)),
                _ => None,
            })
            .unwrap_or_else(|| "logic [7:0]".to_string());
        let data_width_num = self.width_of_type_str(&data_width_ty);

        // Determine addr width: ceil(log2(NREGS))
        let nregs_int: u64 = nregs_str.parse().unwrap_or(32);
        let addr_width = if nregs_int <= 1 { 1 } else { (nregs_int as f64).log2().ceil() as u32 };

        // Read/write port counts
        let nread = r.read_ports.as_ref()
            .map(|rp| { let s = self.emit_expr_str(&rp.count_expr); s.parse::<u64>().unwrap_or(1) })
            .unwrap_or(1);
        let nwrite = r.write_ports.as_ref()
            .map(|wp| { let s = self.emit_expr_str(&wp.count_expr); s.parse::<u64>().unwrap_or(1) })
            .unwrap_or(1);

        let clk = r.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let rst = r.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "rst".to_string());
        let is_async = r.ports.iter()
            .any(|p| matches!(&p.ty, TypeExpr::Reset(crate::ast::ResetKind::Async)));

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        self.line(&format!("parameter int NREGS = {nregs_str},"));
        self.line(&format!("parameter int DATA_WIDTH = {data_width_num}"));
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
        self.line(&format!("logic [DATA_WIDTH-1:0] rf_data [0:NREGS-1];"));
        self.line("");

        // ── Determine read/write port signal names (flat) ─────────────────────
        let write_pfx = r.write_ports.as_ref().map(|wp| wp.name.name.clone()).unwrap_or_else(|| "write".to_string());
        let read_pfx  = r.read_ports.as_ref().map(|rp| rp.name.name.clone()).unwrap_or_else(|| "read".to_string());

        // Flat name helper: "{pfx}{i}_{sig}" when count>1, else "{pfx}_{sig}"
        let flat = |pfx: &str, i: u64, count: u64, sig: &str| -> String {
            if count == 1 { format!("{pfx}_{sig}") } else { format!("{pfx}{i}_{sig}") }
        };

        // ── Write always_ff ───────────────────────────────────────────────────
        let ff_sens = if is_async {
            format!("posedge {clk} or posedge {rst}")
        } else {
            format!("posedge {clk}")
        };

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst}) begin"));
        self.indent += 1;
        for init in &r.inits {
            let idx = self.emit_expr_str(&init.index);
            let val = self.emit_expr_str(&init.value);
            self.line(&format!("rf_data[{idx}] <= {val};"));
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        // Unroll write ports
        for wi in 0..nwrite {
            let wen   = flat(&write_pfx, wi, nwrite, "en");
            let waddr = flat(&write_pfx, wi, nwrite, "addr");
            let wdata = flat(&write_pfx, wi, nwrite, "data");
            self.line(&format!("if ({wen})"));
            self.indent += 1;
            self.line(&format!("rf_data[{waddr}] <= {wdata};"));
            self.indent -= 1;
        }
        self.indent -= 1;
        self.line("end");
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
        use crate::ast::{RamReadMode, RamWriteMode};
        // The single port group
        let pg = &r.port_groups[0];
        let pfx = &pg.name.name.clone();

        // Detect signal names
        let has_wen = pg.signals.iter().any(|s| s.name.name == "wen");
        let out_sig = pg.signals.iter().find(|s| s.direction == Direction::Out).cloned();

        match r.read_mode {
            RamReadMode::Async => {
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
            RamReadMode::Sync | RamReadMode::SyncOut => {
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
                    // sync_out adds an extra output register stage
                    if r.read_mode == RamReadMode::SyncOut {
                        let rdata_r2 = format!("{pfx}_{}_r2", os.name.name);
                        self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r2};"));
                        self.line(&format!("always_ff @(posedge {clk}) {rdata_r2} <= {rdata_r};"));
                        self.line(&format!("assign {pfx}_{} = {rdata_r2};", os.name.name));
                    }
                }
            }
        }
    }

    fn emit_ram_simple_dual(&mut self, r: &RamDecl, clk: &str, _data_width_ty: &str) {
        use crate::ast::RamReadMode;
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

        match r.read_mode {
            RamReadMode::Async => {
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
            RamReadMode::Sync | RamReadMode::SyncOut => {
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
                self.line(&format!("assign {rpfx}_{out_sig} = {rdata_r};"));
            }
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
}

