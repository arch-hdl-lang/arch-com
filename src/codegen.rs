use crate::ast::*;
use crate::diagnostics::CompileWarning;
use crate::lexer::Span;
use crate::resolve::{Symbol, SymbolTable};
use crate::typecheck::enum_width;

fn stmt_span_start(stmt: &Stmt) -> usize {
    match stmt {
        Stmt::Assign(a) => a.span.start,
        Stmt::IfElse(i) => i.span.start,
        Stmt::Match(m) => m.span.start,
        Stmt::Log(l) => l.span.start,
        Stmt::For(f) => f.span.start,
        Stmt::Init(ib) => ib.span.start,
        Stmt::WaitUntil(_, sp) => sp.start,
        Stmt::DoUntil { span, .. } => span.start,
    }
}

fn comb_stmt_span_start(stmt: &CombStmt) -> usize {
    match stmt {
        CombStmt::Assign(a) => a.span.start,
        CombStmt::IfElse(i) => i.span.start,
        CombStmt::MatchExpr(m) => m.span.start,
        CombStmt::Log(l) => l.span.start,
        CombStmt::For(f) => f.span.start,
    }
}

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
    /// Bus port names in the current module → bus name (for FieldAccess rewriting).
    bus_ports: std::collections::HashMap<String, String>,
    /// Bus-typed wire names in the current module → bus name. Bus wires are
    /// flattened into individual SV signals `<wire>_<field>` at emission
    /// time (no SV interfaces or structs are generated for buses), so
    /// FieldAccess on a bus wire rewrites to the flat name just like a bus
    /// port does.
    bus_wires: std::collections::HashMap<String, String>,
    /// Reset port names in the current module → (kind, level), for `.asserted` emission.
    reset_ports: std::collections::HashMap<String, (ResetKind, ResetLevel)>,
    /// Name of the construct currently being emitted (for symbol lookups).
    current_construct: String,
    /// Context-sensitive identifier substitutions.
    /// Used during Vec method predicate emission to rebind `item` and
    /// `index` to per-iteration expressions (e.g. `vec[3]`, `2'd3`).
    /// Checked first in `emit_expr_str`'s Ident branch; empty otherwise.
    ident_subst: std::collections::HashMap<String, String>,
    /// Map of Vec-typed signal name → element count N.
    /// Populated per-module at emit time so Vec method lowerings
    /// (`any`/`all`/`count`/etc.) can unroll over N iterations.
    vec_sizes: std::collections::HashMap<String, u32>,
    /// Set of index widths used by `.find_first(...)` calls in this file.
    /// Drives emission of one `typedef struct packed ... __ArchFindResult_<W>;`
    /// per unique W at the top of the generated SV. Interior-mutability
    /// so the `&self` emission path can record widths as it goes.
    find_first_widths: std::cell::RefCell<std::collections::BTreeSet<u32>>,
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
            bus_ports: std::collections::HashMap::new(),
            bus_wires: std::collections::HashMap::new(),
            reset_ports: std::collections::HashMap::new(),
            current_construct: String::new(),
            ident_subst: std::collections::HashMap::new(),
            vec_sizes: std::collections::HashMap::new(),
            find_first_widths: std::cell::RefCell::new(std::collections::BTreeSet::new()),
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
            .flat_map(|i| match i {
                Item::Function(f) => vec![f.clone()],
                Item::Package(p) => p.functions.clone(),
                _ => vec![],
            })
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
                Item::Cam(c) => self.emit_cam(c),

                Item::Counter(c) => self.emit_counter(c),
                Item::Arbiter(a) => self.emit_arbiter(a),
                Item::Regfile(r) => self.emit_regfile(r),
                Item::Pipeline(p) => self.emit_pipeline(p),
                Item::Function(_) => {} // emitted inside modules
                Item::Linklist(l) => self.emit_linklist(l),
                Item::Template(_) => {} // compile-time only
                Item::Bus(_) => {} // compile-time only; flattened at port sites
                Item::Synchronizer(s) => self.emit_synchronizer(s),
                Item::Clkgate(c) => self.emit_clkgate(c),
                Item::Package(p) => self.emit_package(p),
                Item::Use(_) => {} // import emitted inside modules
            }
        }
        // Flush any trailing comments after the last item.
        let end = usize::MAX;
        self.emit_comments_before(end);

        // Prepend typedefs for any synthesized find_first result structs.
        // One packed struct per unique index width used in the source.
        let widths = self.find_first_widths.borrow();
        if !widths.is_empty() {
            let mut prefix = String::new();
            prefix.push_str("// Auto-generated result struct(s) for Vec.find_first\n");
            for w in widths.iter() {
                prefix.push_str(&format!(
                    "typedef struct packed {{ logic found; logic [{}:0] index; }} __ArchFindResult_{};\n",
                    w.saturating_sub(1),
                    w
                ));
            }
            prefix.push('\n');
            prefix.push_str(&self.out);
            self.out = prefix;
        }
        std::mem::take(&mut self.out)
    }

    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn emit_param_decl(&mut self, p: &ParamDecl, comma: &str) {
        let default_str = if let Some(d) = &p.default {
            format!(" = {}", self.emit_expr_str(d))
        } else {
            String::new()
        };
        let kw = if p.is_local { "localparam" } else { "parameter" };
        match &p.kind {
            ParamKind::WidthConst(hi, lo) => {
                let hi_s = self.emit_expr_str(hi);
                let lo_s = self.emit_expr_str(lo);
                self.line(&format!("{kw} [{}:{}] {}{}{}", hi_s, lo_s, p.name.name, default_str, comma));
            }
            ParamKind::EnumConst(enum_name) => {
                self.line(&format!("{kw} {} {}{}{}", enum_name, p.name.name, default_str, comma));
            }
            _ => {
                self.line(&format!("{kw} int {}{}{}", p.name.name, default_str, comma));
            }
        }
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
                FunctionBodyItem::IfElse(ie) => {
                    self.emit_function_if(ie);
                }
                FunctionBodyItem::For(fl) => {
                    self.emit_function_for(fl);
                }
                FunctionBodyItem::Assign(a) => {
                    let target = self.emit_expr_str(&a.target);
                    let val = self.emit_expr_str(&a.value);
                    self.line(&format!("{target} = {val};"));
                }
            }
        }
        self.indent -= 1;
        self.line("endfunction");
        self.line("");
    }

    fn emit_function_body_items(&mut self, items: &[FunctionBodyItem]) {
        for item in items {
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
                FunctionBodyItem::IfElse(ie) => self.emit_function_if(ie),
                FunctionBodyItem::For(fl) => self.emit_function_for(fl),
                FunctionBodyItem::Assign(a) => {
                    let target = self.emit_expr_str(&a.target);
                    let val = self.emit_expr_str(&a.value);
                    self.line(&format!("{target} = {val};"));
                }
            }
        }
    }

    fn emit_function_if(&mut self, ie: &FunctionIfElse) {
        let cond = self.emit_expr_str(&ie.cond);
        self.line(&format!("if ({cond}) begin"));
        self.indent += 1;
        self.emit_function_body_items(&ie.then_body);
        self.indent -= 1;
        if !ie.else_body.is_empty() {
            // Check if else body is a single elsif (nested IfElse)
            if ie.else_body.len() == 1 {
                if let FunctionBodyItem::IfElse(nested) = &ie.else_body[0] {
                    let ncond = self.emit_expr_str(&nested.cond);
                    self.line(&format!("end else if ({ncond}) begin"));
                    self.indent += 1;
                    self.emit_function_body_items(&nested.then_body);
                    self.indent -= 1;
                    if !nested.else_body.is_empty() {
                        self.line("end else begin");
                        self.indent += 1;
                        self.emit_function_body_items(&nested.else_body);
                        self.indent -= 1;
                    }
                    self.line("end");
                    return;
                }
            }
            self.line("end else begin");
            self.indent += 1;
            self.emit_function_body_items(&ie.else_body);
            self.indent -= 1;
        }
        self.line("end");
    }

    fn emit_function_for(&mut self, fl: &FunctionFor) {
        let var = &fl.var.name;
        match &fl.range {
            ForRange::Range(lo, hi) => {
                let lo_s = self.emit_expr_str(lo);
                let hi_s = self.emit_expr_str(hi);
                self.line(&format!("for (int {var} = {lo_s}; {var} <= {hi_s}; {var}++) begin"));
            }
            ForRange::ValueList(_vals) => {
                // Value-list for loops are compile-time unrolled — emit sequentially
                // For simplicity in functions, just emit as sequential statements
                if let ForRange::ValueList(vals) = &fl.range {
                    for val in vals {
                        let v = self.emit_expr_str(val);
                        self.line(&format!("// {var} = {v}"));
                        // TODO: proper unrolling with variable substitution
                    }
                }
                return;
            }
        }
        self.indent += 1;
        self.emit_function_body_items(&fl.body);
        self.indent -= 1;
        self.line("end");
    }

    fn emit_package(&mut self, pkg: &PackageDecl) {
        self.line(&format!("package {};", pkg.name.name));
        self.indent += 1;

        // params → localparam
        for p in &pkg.params {
            if let Some(d) = &p.default {
                let val = self.emit_expr_str(d);
                self.line(&format!("localparam int {} = {};", p.name.name, val));
            }
        }

        // enums
        for e in &pkg.enums {
            self.emit_enum(e);
        }

        // structs
        for s in &pkg.structs {
            self.emit_struct(s);
        }

        // functions
        for f in &pkg.functions {
            self.emit_function(f);
        }

        self.indent -= 1;
        self.line("endpackage");
        self.line("");
    }

    fn emit_struct(&mut self, s: &StructDecl) {
        // Canonical ARCH packed-struct bit layout: first-declared field = MSB,
        // last-declared field = LSB — matching SV's `struct packed` convention
        // (fields listed top-to-bottom inside `struct packed { ... }` are emitted
        // MSB-first by the SV standard). Emit fields in declaration order.
        self.line("typedef struct packed {");
        self.indent += 1;
        for field in s.fields.iter() {
            let ty_str = self.emit_type_str(&field.ty);
            self.line(&format!("{} {};", ty_str, field.name.name));
        }
        self.indent -= 1;
        self.line(&format!("}} {};", s.name.name));
        self.line("");
    }

    fn emit_enum(&mut self, e: &EnumDecl) {
        // Compute effective values: explicit where provided, auto-sequential otherwise
        let effective_values: Vec<u64> = e.values.iter().enumerate().map(|(i, v)| {
            v.as_ref().and_then(|expr| match &expr.kind {
                ExprKind::Literal(LitKind::Dec(n)) => Some(*n),
                ExprKind::Literal(LitKind::Hex(n)) => Some(*n),
                ExprKind::Literal(LitKind::Bin(n)) => Some(*n),
                ExprKind::Literal(LitKind::Sized(_, n)) => Some(*n),
                _ => None,
            }).unwrap_or(i as u64)
        }).collect();
        // Width: from max value (covers explicit encodings like one-hot)
        let max_val = effective_values.iter().copied().max().unwrap_or(0);
        let width = if max_val == 0 { 1 } else { 64 - max_val.leading_zeros() };
        let width = std::cmp::max(width, enum_width(e.variants.len()));
        let variants: Vec<String> = e.variants.iter().zip(effective_values.iter())
            .map(|(v, val)| format!("{} = {}'d{}", v.name.to_uppercase(), width, val))
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
        self.current_construct = m.name.name.clone();
        // Emit SV `import NAME::*;` only for `use NAME;` whose target is an
        // actual `package` — package contents become an SV package and need
        // the import for typedef/enum/struct visibility. Other `use` targets
        // (bus, module, fsm, ...) are pure compile-time references that the
        // ARCH compiler resolves before codegen; emitting `import` for them
        // breaks Verilator/iverilog since no corresponding SV package exists.
        for item in &self.source.items {
            if let Item::Use(u) = item {
                let is_package = self.source.items.iter().any(|i| {
                    matches!(i, Item::Package(p) if p.name.name == u.name.name)
                });
                if is_package {
                    self.out.push_str(&format!("import {}::*;\n", u.name.name));
                }
            }
        }

        // Module header with parameters
        if m.params.is_empty() {
            self.out.push_str(&format!("module {} (\n", m.name.name));
        } else {
            self.out.push_str(&format!("module {} #(\n", m.name.name));
            self.indent += 1;
            for (i, p) in m.params.iter().enumerate() {
                let comma = if i < m.params.len() - 1 { "," } else { "" };
                self.emit_param_decl(p, comma);
            }
            self.indent -= 1;
            self.line(") (");
        }

        // Ports — bus ports are flattened to individual signals
        self.bus_ports.clear();
        self.bus_wires.clear();
        self.reset_ports.clear();
        self.vec_sizes.clear();
        for p in m.ports.iter() {
            if let TypeExpr::Reset(kind, level) = &p.ty {
                self.reset_ports.insert(p.name.name.clone(), (*kind, *level));
            }
            if let TypeExpr::Vec(_, size_expr) = &p.ty {
                if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                    self.vec_sizes.insert(p.name.name.clone(), n);
                }
            }
        }
        // Vec-typed regs, wires, and let bindings are also eligible receivers.
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    if let TypeExpr::Vec(_, size_expr) = &r.ty {
                        if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                            self.vec_sizes.insert(r.name.name.clone(), n);
                        }
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    if let TypeExpr::Vec(_, size_expr) = &w.ty {
                        if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                            self.vec_sizes.insert(w.name.name.clone(), n);
                        }
                    }
                }
                ModuleBodyItem::LetBinding(lb) => {
                    if let Some(TypeExpr::Vec(_, size_expr)) = &lb.ty {
                        if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                            self.vec_sizes.insert(lb.name.name.clone(), n);
                        }
                    }
                }
                _ => {}
            }
        }
        // Collect all flattened port lines first so we can add commas correctly
        let mut port_lines: Vec<String> = Vec::new();
        for p in m.ports.iter() {
            if let Some(ref bi) = p.bus_info {
                let bus_name = &bi.bus_name.name;
                self.bus_ports.insert(p.name.name.clone(), bus_name.clone());
                if let Some((crate::resolve::Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name) {
                    // Build param substitution map: start with bus defaults, override with port params
                    let mut param_map: std::collections::HashMap<String, &Expr> = info.params.iter()
                        .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                        .collect();
                    for pa in &bi.params {
                        param_map.insert(pa.name.name.clone(), &pa.value);
                    }
                    let eff_signals = info.effective_signals(&param_map);
                    for (sname, sdir, sty) in &eff_signals {
                        let actual_dir = match bi.perspective {
                            BusPerspective::Initiator => *sdir,
                            BusPerspective::Target => (*sdir).flip(),
                        };
                        let dir_str = match actual_dir {
                            Direction::In => "input",
                            Direction::Out => "output",
                        };
                        let subst_ty = Self::subst_type_expr(sty, &param_map);
                        let ty_str = self.emit_port_type_str(&subst_ty);
                        port_lines.push(format!("{} {} {}_{}", dir_str, ty_str, p.name.name, sname));
                    }
                }
            } else {
                let dir = match p.direction {
                    Direction::In => "input",
                    Direction::Out => "output",
                };
                // Vec types: emit unpacked array dimensions after the port name
                if let TypeExpr::Vec(_, _) = &p.ty {
                    let (base_ty, suffix) = self.emit_type_and_array_suffix(&p.ty);
                    let init_str = p.reg_info.as_ref()
                        .and_then(|ri| ri.init.as_ref())
                        .map(|e| format!(" = {}", self.emit_expr_str(e)))
                        .unwrap_or_default();
                    port_lines.push(format!("{} {} {}{}{}", dir, base_ty, p.name.name, suffix, init_str));
                } else {
                    let ty_str = self.emit_port_type_str(&p.ty);
                    let init_str = p.reg_info.as_ref()
                        .and_then(|ri| ri.init.as_ref())
                        .map(|e| format!(" = {}", self.emit_expr_str(e)))
                        .unwrap_or_default();
                    port_lines.push(format!("{} {} {}{}", dir, ty_str, p.name.name, init_str));
                }
            }
        }
        self.indent += 1;
        for (i, line) in port_lines.iter().enumerate() {
            let comma = if i < port_lines.len() - 1 { "," } else { "" };
            self.line(&format!("{}{}", line, comma));
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
            self.line("// synopsys translate_off");
            self.line("integer _arch_verbosity = 1; // 0=Always 1=Low 2=Medium 3=High 4=Full 5=Debug");
            self.line("initial void'($value$plusargs(\"arch_verbosity=%0d\", _arch_verbosity));");
            self.line("// synopsys translate_on");
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
                    if let Some(ref init_expr) = r.init {
                        let init_str = self.emit_expr_str(init_expr);
                        if arr_suffix.is_empty() {
                            self.line(&format!("{} {} = {};", ty_str, r.name.name, init_str));
                        } else {
                            // Skip declaration initializer for unpacked arrays (icarus doesn't support '{default:})
                            self.line(&format!("{} {}{};", ty_str, r.name.name, arr_suffix));
                        }
                    } else {
                        self.line(&format!("{} {}{};", ty_str, r.name.name, arr_suffix));
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    // Destructuring: `let {a, b} = expr;` — emit one
                    // wire + assignment per bound field, using struct-field
                    // access on the original RHS. The RHS is re-emitted
                    // per binding; structurally-identical expressions are
                    // fine at this stage (synth CSE handles it).
                    if !l.destructure_fields.is_empty() {
                        // Special case: RHS is `vec.find_first(pred)`.
                        // Emit the raw OR + priority encoder directly so we
                        // don't pay for the bulky struct-literal-then-field
                        // access shape. Widths come from the synthesized
                        // __ArchFindResult_<W> name.
                        if let ExprKind::MethodCall(recv, mname, margs) = &l.value.kind {
                            if mname.name == "find_first" {
                                let recv_str = self.emit_expr_str(recv);
                                let n = match &recv.kind {
                                    ExprKind::Ident(nm) => self.vec_sizes.get(nm).copied(),
                                    _ => None,
                                };
                                if let Some(n) = n {
                                    let idx_w = std::cmp::max(1, (n as f64).log2().ceil() as u32);
                                    // Record width so the typedef still emits
                                    // (field access paths may still reference
                                    // the struct type).
                                    self.find_first_widths.borrow_mut().insert(idx_w);
                                    // Emit per-iteration predicate strings.
                                    let emit_at_i = |cg: &Codegen, i: u32| -> String {
                                        let this = cg as *const Codegen as *mut Codegen;
                                        unsafe {
                                            (*this).ident_subst.insert("item".to_string(), format!("{recv_str}[{i}]"));
                                            (*this).ident_subst.insert("index".to_string(), format!("{idx_w}'d{i}"));
                                        }
                                        let s = cg.emit_expr_str(&margs[0]);
                                        unsafe {
                                            (*this).ident_subst.remove("item");
                                            (*this).ident_subst.remove("index");
                                        }
                                        s
                                    };
                                    let hits: Vec<String> = (0..n).map(|i| emit_at_i(self, i)).collect();
                                    let found_expr = hits.join(" || ");
                                    let mut idx_expr = format!("{idx_w}'d0");
                                    for i in (0..n).rev() {
                                        let hit = &hits[i as usize];
                                        idx_expr = format!("({hit}) ? {idx_w}'d{i} : {idx_expr}");
                                    }
                                    for bind in &l.destructure_fields {
                                        match bind.name.as_str() {
                                            "found" => {
                                                self.line("logic found;");
                                                self.line(&format!("assign found = {found_expr};"));
                                            }
                                            "index" => {
                                                self.line(&format!("logic [{}:0] index;", idx_w.saturating_sub(1)));
                                                self.line(&format!("assign index = {idx_expr};"));
                                            }
                                            _ => {}
                                        }
                                    }
                                    continue;
                                }
                            }
                        }
                        let rhs_ty = self.infer_expr_struct_name(&l.value);
                        let val_str = self.emit_expr_str(&l.value);
                        for bind in &l.destructure_fields {
                            if let Some(field_ty) = rhs_ty.as_ref()
                                .and_then(|sname| self.struct_field_type(sname, &bind.name))
                            {
                                let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&field_ty);
                                self.line(&format!("{} {}{};", ty_str, bind.name, arr_suffix));
                                self.line(&format!("assign {} = {}.{};", bind.name, val_str, bind.name));
                            } else {
                                // Fallback: struct type unknown at codegen — emit as logic.
                                self.line(&format!("logic {};", bind.name));
                                self.line(&format!("assign {} = {}.{};", bind.name, val_str, bind.name));
                            }
                        }
                        continue;
                    }
                    // Special case: `let x: T = match scrut ... end match;` emits as
                    // `always_comb` with `case` instead of a deeply-nested ternary.
                    // Threshold: 3+ arms makes the ternary chain unreadable.
                    if let ExprKind::ExprMatch(scrut, arms) = &l.value.kind {
                        if arms.len() >= 3 {
                            if let Some(ty) = &l.ty {
                                let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                                self.line(&format!("{} {}{};", ty_str, l.name.name, arr_suffix));
                            }
                            let scrut_str = self.emit_expr_str(scrut);
                            self.line("always_comb begin");
                            self.indent += 1;
                            // Default to '0 (covers the unmatched-pattern case explicitly)
                            self.line(&format!("{} = '0;", l.name.name));
                            self.line(&format!("case ({})", scrut_str));
                            self.indent += 1;
                            for arm in arms {
                                let pat = self.emit_pattern(&arm.pattern);
                                let val_str = self.emit_expr_str(&arm.value);
                                self.line(&format!("{}: {} = {};", pat, l.name.name, val_str));
                            }
                            self.indent -= 1;
                            self.line("endcase");
                            self.indent -= 1;
                            self.line("end");
                            continue;
                        }
                    }
                    let val_str = self.emit_expr_str(&l.value);
                    if let Some(ty) = &l.ty {
                        let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                        self.line(&format!("{} {}{};", ty_str, l.name.name, arr_suffix));
                        self.line(&format!("assign {} = {};", l.name.name, val_str));
                    } else {
                        // ty=None: assignment to existing port or wire — no logic declaration
                        self.line(&format!("assign {} = {};", l.name.name, val_str));
                    }
                }
                ModuleBodyItem::CombBlock(cb) => self.emit_comb_block(cb),
                ModuleBodyItem::RegBlock(rb) => self.emit_reg_block(rb, &m_clone),
                ModuleBodyItem::LatchBlock(lb) => self.emit_latch_block(lb),
                ModuleBodyItem::Inst(inst) => {
                    // Auto-declare output wires that aren't already declared.
                    // Track newly-emitted names so a later inst that connects
                    // to the same parent-side bus wire skips re-declaration.
                    let mut just_added: Vec<String> = Vec::new();
                    self.emit_inst_output_wire_decls(inst, &declared_names);
                    // Collect names we added so subsequent insts know.
                    if let Some((Symbol::Module(info), _)) =
                        self.symbols.globals.get(&inst.module_name.name)
                    {
                        let module_ports = info.ports.clone();
                        for conn in &inst.connections {
                            let Some(port) = module_ports.iter().find(|p| p.name.name == conn.port_name.name) else { continue; };
                            let Some(bi) = &port.bus_info else { continue; };
                            let ExprKind::Ident(parent_name) = &conn.signal.kind else { continue; };
                            let Some((Symbol::Bus(bus_info), _)) =
                                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
                            let mut pm = bus_info.default_param_map();
                            for pa in &bi.params { pm.insert(pa.name.name.clone(), &pa.value); }
                            for (sname, _sdir, _ty) in bus_info.effective_signals(&pm) {
                                just_added.push(format!("{parent_name}_{sname}"));
                            }
                            // Also mark the whole-bus parent name as "claimed"
                            // so later code doesn't re-emit a scalar for it.
                            just_added.push(parent_name.clone());
                        }
                    }
                    for n in just_added { declared_names.insert(n); }
                    self.emit_inst(inst);
                }
                ModuleBodyItem::PipeRegDecl(p) => {
                    self.emit_pipe_reg(p, &m_clone);
                }
                ModuleBodyItem::WireDecl(w) => {
                    // Bus-typed wires: flatten into individual SV signals
                    // `<wire>_<field>`. No SV interface/struct is generated
                    // for the bus; the bus exists purely as a compile-time
                    // abstraction. Record the wire in `bus_wires` so field
                    // access rewrites (see emit_expr_str) produce the flat
                    // name.
                    if let TypeExpr::Named(id) = &w.ty {
                        if let Some((crate::resolve::Symbol::Bus(info), _)) =
                            self.symbols.globals.get(&id.name)
                        {
                            self.bus_wires.insert(w.name.name.clone(), id.name.clone());
                            let param_map: std::collections::HashMap<String, &Expr> =
                                info.params.iter()
                                    .filter_map(|pd| pd.default.as_ref()
                                        .map(|d| (pd.name.name.clone(), d)))
                                    .collect();
                            for (sname, _sdir, sty) in info.effective_signals(&param_map) {
                                let (ty_str, arr_suffix) =
                                    self.emit_type_and_array_suffix(&sty);
                                self.line(&format!(
                                    "{} {}_{}{};",
                                    ty_str, w.name.name, sname, arr_suffix
                                ));
                                declared_names.insert(format!("{}_{}", w.name.name, sname));
                            }
                            declared_names.insert(w.name.name.clone());
                            continue;
                        }
                    }
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&w.ty);
                    self.line(&format!("{} {}{};", ty_str, w.name.name, arr_suffix));
                    declared_names.insert(w.name.name.clone());
                }
                ModuleBodyItem::Generate(ref gen) => {
                    self.emit_generate(gen);
                }
                ModuleBodyItem::Thread(_) | ModuleBodyItem::Resource(_) => {
                    // Threads and resources are lowered before codegen
                    unreachable!("thread/resource should have been lowered before codegen");
                }
                ModuleBodyItem::Assert(_) => {
                    // Collected and emitted as a group below (with translate_off/on)
                }
                ModuleBodyItem::Function(f) => {
                    self.emit_function(f);
                }
            }
        }

        // Emit module-level assert/cover declarations (grouped with translate_off/on)
        {
            let module_asserts: Vec<&AssertDecl> = body_items.iter()
                .filter_map(|i| if let ModuleBodyItem::Assert(a) = i { Some(a) } else { None })
                .collect();
            if !module_asserts.is_empty() {
                let clk_name = m_clone.ports.iter()
                    .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                    .map(|p| p.name.name.clone())
                    .unwrap_or_else(|| "clk".to_string());
                let owned: Vec<AssertDecl> = module_asserts.into_iter().cloned().collect();
                self.emit_asserts_for_construct(&owned, &m_clone.name.name, &clk_name);
            }
        }

        // Emit reset-only always_ff for any reg whose decl has a
        // `reset` clause but which is never assigned in any seq block.
        // Without this, a `reg X: T reset rst => V;` paired with no
        // seq-block update vanishes from the emitted SV — the decl is
        // kept but the reset never drives the flop, leaving it at X.
        // That shape is common for spec-constant CSR fields
        // (xdebugver, mhpmevent*, vendor/arch/impl IDs, etc.).
        self.emit_orphan_reset_regs(&m_clone);

        // Emit guard-contract SVA: for each `reg ... guard <sig>`, prove that
        // whenever `<sig>` is high, the reg has been written at least once.
        // Uses a shadow `_<reg>_written` set on any seq-block commit (over-approx).
        self.emit_guard_contracts(&m_clone);

        // Emit bounds-check SVA for runtime-indexed Vec / bit-select /
        // part-select accesses in seq/latch blocks. Mirrors arch sim's
        // _ARCH_BCHK so iverilog/Verilator/formal tools see the invariant.
        self.emit_bound_asserts(&m_clone);

        // Emit per-variant handshake protocol SVA for each bus port whose
        // bus definition contains one or more `handshake` channels.
        self.emit_handshake_asserts(&m_clone);

        // Emit the synthesized credit counter + can_send wire for each
        // `send`-role credit_channel bus port on the module (PR #3b-ii).
        self.emit_credit_channel_state(&m_clone);

        // Emit the target-side FIFO for each credit_channel bus port on
        // the module where this side is the receiver (PR #3b-iii).
        self.emit_credit_channel_receiver_state(&m_clone);

        // Emit the Tier-2 credit_channel protocol assertions (PR #4).
        self.emit_credit_channel_asserts(&m_clone);

        // Emit log file descriptors: initial $fopen / final $fclose
        let log_files = Self::collect_log_files(&m_clone.body);
        if !log_files.is_empty() {
            self.line("");
            self.line("// synopsys translate_off");
            for path in &log_files {
                let fd = Self::log_fd_name(path);
                self.line(&format!("integer {fd};"));
            }
            self.line("initial begin");
            self.indent += 1;
            for path in &log_files {
                let fd = Self::log_fd_name(path);
                self.line(&format!("{fd} = $fopen(\"{path}\", \"w\");"));
            }
            self.indent -= 1;
            self.line("end");
            self.line("final begin");
            self.indent += 1;
            for path in &log_files {
                let fd = Self::log_fd_name(path);
                self.line(&format!("$fclose({fd});"));
            }
            self.indent -= 1;
            self.line("end");
            self.line("// synopsys translate_on");
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Collect all unique log file paths from module body items.
    fn collect_log_files(body: &[ModuleBodyItem]) -> Vec<String> {
        let mut files = Vec::new();
        let mut seen = std::collections::HashSet::new();
        fn collect_from_comb(stmts: &[CombStmt], files: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
            for stmt in stmts {
                match stmt {
                    CombStmt::Log(l) => {
                        if let Some(ref path) = l.file {
                            if seen.insert(path.clone()) { files.push(path.clone()); }
                        }
                    }
                    CombStmt::IfElse(ie) => {
                        collect_from_comb(&ie.then_stmts, files, seen);
                        collect_from_comb(&ie.else_stmts, files, seen);
                    }
                    CombStmt::MatchExpr(m) => {
                        for arm in &m.arms { collect_from_seq(&arm.body, files, seen); }
                    }
                    _ => {}
                }
            }
        }
        fn collect_from_seq(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
            for stmt in stmts {
                match stmt {
                    Stmt::Log(l) => {
                        if let Some(ref path) = l.file {
                            if seen.insert(path.clone()) { files.push(path.clone()); }
                        }
                    }
                    Stmt::IfElse(ie) => {
                        collect_from_seq(&ie.then_stmts, files, seen);
                        collect_from_seq(&ie.else_stmts, files, seen);
                    }
                    Stmt::Match(m) => {
                        for arm in &m.arms { collect_from_seq(&arm.body, files, seen); }
                    }
                    _ => {}
                }
            }
        }
        for item in body {
            match item {
                ModuleBodyItem::CombBlock(cb) => collect_from_comb(&cb.stmts, &mut files, &mut seen),
                ModuleBodyItem::RegBlock(rb) => collect_from_seq(&rb.stmts, &mut files, &mut seen),
                _ => {}
            }
        }
        files
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
                ModuleBodyItem::WireDecl(w) if w.name.name == name => {
                    return self.emit_type_str(&w.ty);
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
                    let tgt = self.emit_expr_str(&a.target);
                    self.line(&format!("assign {} = {};", tgt, val));
                }
            }
        } else {
            self.line("always_comb begin");
            self.indent += 1;
            for stmt in &cb.stmts {
                self.emit_comb_stmt(stmt);
            }
            self.emit_comments_before(cb.span.end);
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
            Stmt::For(f) => f.body.iter().any(Self::stmt_has_log),
            Stmt::Init(ib) => ib.body.iter().any(Self::stmt_has_log),
            Stmt::WaitUntil(_, _) => false,
            Stmt::DoUntil { body, .. } => body.iter().any(Self::stmt_has_log),
        }
    }

    fn comb_stmt_has_log(s: &CombStmt) -> bool {
        match s {
            CombStmt::Log(_) => true,
            CombStmt::IfElse(ie) => ie.then_stmts.iter().any(Self::comb_stmt_has_log)
                || ie.else_stmts.iter().any(Self::comb_stmt_has_log),
            CombStmt::Assign(_) | CombStmt::MatchExpr(_) => false,
            CombStmt::For(f) => f.body.iter().any(Self::stmt_has_log),
        }
    }

    /// Emit a for-loop (Range or ValueList) as SV.
    fn emit_for_loop_sv(&mut self, f: &ForLoop, mut emit_body_stmt: impl FnMut(&mut Self, &Stmt)) {
        let var = &f.var.name;
        match &f.range {
            ForRange::Range(rs, re) => {
                let start = self.emit_expr_str(rs);
                let end = self.emit_expr_str(re);
                self.line(&format!("for (int {var} = {start}; {var} <= {end}; {var}++) begin"));
                self.indent += 1;
                for s in &f.body { emit_body_stmt(self, s); }
                self.indent -= 1;
                self.line("end");
            }
            ForRange::ValueList(vals) => {
                for v in vals {
                    let val = self.emit_expr_str(v);
                    // Emit as a for-loop with a single iteration for Icarus compatibility
                    // (Icarus doesn't support variable declarations inside always_* blocks)
                    self.line(&format!("for (int {var} = {val}; {var} == {val}; {var}++) begin"));
                    self.indent += 1;
                    for s in &f.body { emit_body_stmt(self, s); }
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }
    }

    /// Emit a `log(...)` statement as an `if`-guarded `$display` or `$fwrite`.
    /// Wrapped in translate_off/on so synthesis tools ignore it.
    fn emit_log_stmt(&mut self, l: &LogStmt) {
        let args_str: String = l.args.iter()
            .map(|a| format!(", {}", self.emit_expr_str(a)))
            .collect();
        let stmt = if let Some(ref path) = l.file {
            let fd_name = Self::log_fd_name(path);
            format!(
                "$fwrite({}, \"[%0t][{}][{}] {}\\n\", $time{});",
                fd_name, l.level.name(), l.tag, l.fmt, args_str
            )
        } else {
            format!(
                "$display(\"[%0t][{}][{}] {}\", $time{});",
                l.level.name(), l.tag, l.fmt, args_str
            )
        };
        self.line("// synopsys translate_off");
        if l.level == LogLevel::Always {
            self.line(&stmt);
        } else {
            self.line(&format!("if (_arch_verbosity >= {}) {}", l.level.value(), stmt));
        }
        self.line("// synopsys translate_on");
    }

    /// Generate a deterministic SV file descriptor name from a log file path.
    fn log_fd_name(path: &str) -> String {
        let clean: String = path.chars()
            .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        format!("_log_fd_{clean}")
    }

    fn emit_comb_stmt(&mut self, stmt: &CombStmt) {
        self.emit_comments_before(comb_stmt_span_start(stmt));
        match stmt {
            CombStmt::Assign(a) => {
                // Match-expression RHS: emit as a case block for readability
                if let ExprKind::ExprMatch(scrutinee, arms) = &a.value.kind {
                    let s = self.emit_expr_str(scrutinee);
                    let target = self.emit_expr_str(&a.target);
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
                    let tgt = self.emit_expr_str(&a.target);
                    self.line(&format!("{} = {};", tgt, val));
                }
            }
            CombStmt::IfElse(ie) => {
                self.emit_comb_if_else(ie);
            }
            CombStmt::MatchExpr(m) => {
                let scrut = self.emit_expr_str(&m.scrutinee);
                let u = if m.unique { "unique " } else { "" };
                self.line(&format!("{}case ({})", u, scrut));
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
            CombStmt::For(f) => {
                self.emit_for_loop_sv(f, |s, stmt| s.emit_reg_stmt_as_comb(stmt));
            }
        }
    }

    fn emit_comb_if_else(&mut self, ie: &CombIfElse) {
        self.emit_comb_if_else_inner(ie, false);
    }

    fn emit_comb_if_else_inner(&mut self, ie: &CombIfElse, is_chain: bool) {
        let cond = self.emit_expr_str(&ie.cond);
        let u = if ie.unique && !is_chain { "unique " } else { "" };
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("{}if ({}) begin", u, cond));
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
            Stmt::IfElse(ie) => {
                self.emit_comb_if_else_from_reg(ie, false);
            }
            Stmt::Match(m) => {
                let scrut = self.emit_expr_str(&m.scrutinee);
                let u = if m.unique { "unique " } else { "" };
                self.line(&format!("{}case ({})", u, scrut));
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
            Stmt::Log(l) => { self.emit_log_stmt(l); }
            Stmt::For(f) => {
                self.emit_for_loop_sv(f, |s, stmt| s.emit_reg_stmt_as_comb(stmt));
            }
            Stmt::Init(_ib) => unreachable!("Stmt::Init should not appear in latch/comb context"),
            Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => {
                unreachable!("WaitUntil/DoUntil only valid in pipeline stage seq blocks")
            }
        }
    }

    fn emit_comb_if_else_from_reg(&mut self, ie: &IfElse, is_chain: bool) {
        let cond = self.emit_expr_str(&ie.cond);
        let u = if ie.unique && !is_chain { "unique " } else { "" };
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("{}if ({}) begin", u, cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_reg_stmt_as_comb(s);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_comb_if_else_from_reg(nested, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_reg_stmt_as_comb(s);
            }
            self.indent -= 1;
        }
        self.line("end");
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
            // Look up reset info from RegDecl or port reg
            let reset_ref: Option<&RegReset> = reg_decls.iter()
                .find(|r| r.name.name == *name)
                .map(|r| &r.reset)
                .or_else(|| m.ports.iter()
                    .find(|p| p.name.name == *name && p.reg_info.is_some())
                    .and_then(|p| p.reg_info.as_ref().map(|ri| &ri.reset)));
            if let Some(reg_reset) = reset_ref {
                let resolved = self.resolve_reg_reset(reg_reset, m);
                if let Some((rst_sig, is_async, is_low)) = resolved {
                    if reset_info.is_none() {
                        reset_info = Some(ResolvedReset {
                            signal: rst_sig.clone(),
                            is_async,
                            is_low,
                        });
                    }
                    let reset_val = Self::reset_value_expr(reg_reset).unwrap();
                    let init = self.emit_expr_str(reset_val);
                    resets.push((name.clone(), init));
                }
            }
        }

        if let Some(ref ri) = reset_info {
            // Build set of register names that have reset
            let reset_reg_names: std::collections::BTreeSet<String> =
                resets.iter().map(|(n, _)| n.clone()).collect();

            // Partition top-level statements: those that assign to reset
            // registers vs. those that only assign to non-reset registers.
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

            let rst_cond_str = if ri.is_low {
                format!("(!{})", ri.signal)
            } else {
                ri.signal.clone()
            };

            // Emit always_ff with reset sensitivity for resetable registers
            if ri.is_async {
                let rst_edge = if ri.is_low { "negedge" } else { "posedge" };
                self.line(&format!(
                    "always_ff @({clk_edge} {} or {rst_edge} {}) begin",
                    rb.clock.name, ri.signal
                ));
            } else {
                self.line(&format!("always_ff @({clk_edge} {}) begin", rb.clock.name));
            }
            self.indent += 1;
            self.line(&format!("if ({rst_cond_str}) begin"));
            self.indent += 1;
            for (name, init) in &resets {
                // Look up Vec depth from reg decls OR port-reg declarations
                let reg_ty = reg_decls.iter()
                    .find(|r| r.name.name == *name)
                    .map(|r| &r.ty)
                    .or_else(|| m.ports.iter()
                        .find(|p| p.name.name == *name && p.reg_info.is_some())
                        .map(|p| &p.ty));
                let vec_depth = reg_ty.map(|ty| {
                    let mut depth = 0u32;
                    let mut t = ty;
                    while let TypeExpr::Vec(inner, _) = t {
                        depth += 1;
                        t = inner;
                    }
                    depth
                }).unwrap_or(0);
                if vec_depth > 0 {
                    // Emit for-loop reset for unpacked arrays (icarus-compatible)
                    if let Some(ty) = reg_ty {
                        // Collect Vec dimensions
                        let mut dims = Vec::new();
                        let mut t = ty;
                        while let TypeExpr::Vec(inner, size) = t {
                            dims.push(self.emit_expr_str(size));
                            t = inner;
                        }
                        // Generate nested for-loops
                        let idx_vars: Vec<String> = (0..dims.len()).map(|d| format!("__ri{d}")).collect();
                        for (d, dim_size) in dims.iter().enumerate() {
                            self.line(&format!("for (int {} = 0; {} < {}; {}++) begin",
                                idx_vars[d], idx_vars[d], dim_size, idx_vars[d]));
                            self.indent += 1;
                        }
                        let idx_str: String = idx_vars.iter().map(|v| format!("[{v}]")).collect();
                        self.line(&format!("{name}{idx_str} <= {init};"));
                        for _ in 0..dims.len() {
                            self.indent -= 1;
                            self.line("end");
                        }
                    }
                } else {
                    self.line(&format!("{name} <= {init};"));
                }
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            for stmt in &guarded_stmts {
                if let Stmt::Init(ib) = stmt {
                    let port = m.ports.iter().find(|p| p.name.name == ib.reset_signal.name);
                    let is_low = port.map_or(false, |p| matches!(&p.ty, TypeExpr::Reset(_, ResetLevel::Low)));
                    let cond = if is_low { format!("(!{})", ib.reset_signal.name) } else { ib.reset_signal.name.clone() };
                    self.line(&format!("if ({cond}) begin"));
                    self.indent += 1;
                    for s in &ib.body { self.emit_reg_stmt(s); }
                    self.indent -= 1;
                    self.line("end");
                } else {
                    self.emit_reg_stmt(stmt);
                }
            }
            self.emit_comments_before(rb.span.end);
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");

            // Emit separate always_ff WITHOUT reset sensitivity for non-reset registers.
            // Mixing resetable and non-resetable regs in one always_ff with async reset
            // in the sensitivity list causes synthesis tools to infer unintended clock
            // gating on the reset path for the non-reset registers.
            if !unguarded_stmts.is_empty() {
                self.line(&format!("always_ff @({clk_edge} {}) begin", rb.clock.name));
                self.indent += 1;
                for stmt in &unguarded_stmts {
                    if let Stmt::Init(ib) = stmt {
                        let port = m.ports.iter().find(|p| p.name.name == ib.reset_signal.name);
                        let is_low = port.map_or(false, |p| matches!(&p.ty, TypeExpr::Reset(_, ResetLevel::Low)));
                        let cond = if is_low { format!("(!{})", ib.reset_signal.name) } else { ib.reset_signal.name.clone() };
                        self.line(&format!("if ({cond}) begin"));
                        self.indent += 1;
                        for s in &ib.body { self.emit_reg_stmt(s); }
                        self.indent -= 1;
                        self.line("end");
                    } else {
                        self.emit_reg_stmt(stmt);
                    }
                }
                self.emit_comments_before(rb.span.end);
                self.indent -= 1;
                self.line("end");
            }
        } else {
            // No registers with declared reset.
            // Check for explicit `init on rst.asserted` blocks — these drive async sensitivity.
            let init_block = rb.stmts.iter().find_map(|s| {
                if let Stmt::Init(ib) = s { Some(ib) } else { None }
            });
            let async_asserted = if let Some(ib) = init_block {
                // Determine async/sync and polarity from the referenced reset port
                m.ports.iter().find(|p| p.name.name == ib.reset_signal.name)
                    .and_then(|p| if let TypeExpr::Reset(ResetKind::Async, level) = &p.ty {
                        Some((ib.reset_signal.name.clone(), *level == ResetLevel::Low))
                    } else { None })
            } else {
                // Still check for `rst.asserted` expressions in the body — if any reference an
                // async reset port, we must add the async edge to the sensitivity list.
                Self::find_asserted_async_reset(&rb.stmts, &m.ports)
            };
            let sens = if let Some((ref rst_sig, is_low)) = async_asserted {
                let rst_edge = if is_low { "negedge" } else { "posedge" };
                format!("always_ff @({clk_edge} {} or {rst_edge} {rst_sig}) begin", rb.clock.name)
            } else {
                format!("always_ff @({clk_edge} {}) begin", rb.clock.name)
            };
            self.line(&sens);
            self.indent += 1;
            for stmt in &rb.stmts {
                if let Stmt::Init(ib) = stmt {
                    // Emit as an explicit `if (rst_cond) begin ... end` block
                    let port = m.ports.iter().find(|p| p.name.name == ib.reset_signal.name);
                    let is_low = port.map_or(false, |p| matches!(&p.ty, TypeExpr::Reset(_, ResetLevel::Low)));
                    let cond = if is_low {
                        format!("(!{})", ib.reset_signal.name)
                    } else {
                        ib.reset_signal.name.clone()
                    };
                    self.line(&format!("if ({cond}) begin"));
                    self.indent += 1;
                    for s in &ib.body {
                        self.emit_reg_stmt(s);
                    }
                    self.indent -= 1;
                    self.line("end");
                } else {
                    self.emit_reg_stmt(stmt);
                }
            }
            self.emit_comments_before(rb.span.end);
            self.indent -= 1;
            self.line("end");
        }
    }

    /// Scan statements for `name.asserted` where `name` is an async Reset port.
    /// Returns `Some((signal_name, is_low))` for the first one found.
    fn find_asserted_async_reset(stmts: &[Stmt], ports: &[PortDecl]) -> Option<(String, bool)> {
        fn scan_expr(expr: &Expr, ports: &[PortDecl]) -> Option<(String, bool)> {
            if let ExprKind::FieldAccess(base, field) = &expr.kind {
                if field.name == "asserted" {
                    if let ExprKind::Ident(name) = &base.kind {
                        if let Some(port) = ports.iter().find(|p| p.name.name == *name) {
                            if let TypeExpr::Reset(ResetKind::Async, level) = &port.ty {
                                return Some((name.clone(), *level == ResetLevel::Low));
                            }
                        }
                    }
                }
            }
            // Recurse into sub-expressions
            match &expr.kind {
                ExprKind::Binary(_, l, r) => scan_expr(l, ports).or_else(|| scan_expr(r, ports)),
                ExprKind::Unary(_, inner) | ExprKind::FieldAccess(inner, _) => scan_expr(inner, ports),
                ExprKind::Ternary(c, t, e) => scan_expr(c, ports).or_else(|| scan_expr(t, ports)).or_else(|| scan_expr(e, ports)),
                ExprKind::MethodCall(base, _, args) => scan_expr(base, ports).or_else(|| args.iter().find_map(|a| scan_expr(a, ports))),
                ExprKind::Index(base, idx) => scan_expr(base, ports).or_else(|| scan_expr(idx, ports)),
                ExprKind::Cast(inner, _) => scan_expr(inner, ports),
                _ => None,
            }
        }
        fn scan_stmt(stmt: &Stmt, ports: &[PortDecl]) -> Option<(String, bool)> {
            match stmt {
                Stmt::Assign(a) => scan_expr(&a.value, ports),
                Stmt::IfElse(ie) => scan_expr(&ie.cond, ports)
                    .or_else(|| ie.then_stmts.iter().find_map(|s| scan_stmt(s, ports)))
                    .or_else(|| ie.else_stmts.iter().find_map(|s| scan_stmt(s, ports))),
                Stmt::For(f) => f.body.iter().find_map(|s| scan_stmt(s, ports)),
                Stmt::Match(m) => m.arms.iter().find_map(|arm| arm.body.iter().find_map(|s| scan_stmt(s, ports))),
                _ => None,
            }
        }
        stmts.iter().find_map(|s| scan_stmt(s, ports))
    }

    fn emit_latch_block(&mut self, lb: &LatchBlock) {
        self.line(&format!("always_latch begin"));
        self.indent += 1;
        self.line(&format!("if ({}) begin", lb.enable.name));
        self.indent += 1;
        for stmt in &lb.stmts {
            self.emit_reg_stmt_as_comb(stmt);
        }
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    /// Resolve a register's reset info: returns Some((signal_name, is_async, is_low))
    /// or None if the register has no reset.
    /// Extract the reset value expression from a RegReset variant.
    /// Emit `always_ff` reset assignments for registers that have a
    /// `reset` clause on their decl but are never assigned in any seq
    /// block. Without this, the reset clause is silently dropped and
    /// the flop sits at X after reset (Verilator lints as UNDRIVEN).
    ///
    /// Clock source: we prefer a clock that's already the `seq on
    /// <clk>` for some RegBlock in this module (preserves clock-domain
    /// grouping with the rest of the flops). If the module has no
    /// RegBlocks at all, fall back to the first `Clock`-typed input
    /// port — by construction that's the only clock the orphan reg can
    /// belong to in a well-formed module.
    ///
    /// Orphans are grouped by (clock, reset_signal, is_async, is_low)
    /// so each group becomes exactly one `always_ff`.
    fn emit_orphan_reset_regs(&mut self, m: &ModuleDecl) {
        // 1. Collect names assigned in any RegBlock.
        let mut assigned_in_any_block = std::collections::BTreeSet::new();
        for item in &m.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                Self::collect_assigned_roots(&rb.stmts, &mut assigned_in_any_block);
            }
        }

        // 2. Find orphans: RegDecls with reset != None and not assigned.
        #[derive(Debug)]
        struct Orphan<'a> {
            name: String,
            init: String,
            reset_signal: String,
            is_async: bool,
            is_low: bool,
            ty: &'a TypeExpr,
        }

        let mut orphans: Vec<Orphan> = Vec::new();
        for item in &m.body {
            let ModuleBodyItem::RegDecl(r) = item else { continue };
            if matches!(r.reset, RegReset::None) {
                continue;
            }
            if assigned_in_any_block.contains(&r.name.name) {
                continue;
            }
            let Some((signal, is_async, is_low)) = self.resolve_reg_reset(&r.reset, m) else {
                // Reset resolution failed (malformed decl after typecheck —
                // shouldn't happen, but guard anyway).
                continue;
            };
            let val_expr = Self::reset_value_expr(&r.reset).expect("non-None reset has value");
            let init = self.emit_expr_str(val_expr);
            orphans.push(Orphan {
                name: r.name.name.clone(),
                init,
                reset_signal: signal,
                is_async,
                is_low,
                ty: &r.ty,
            });
        }

        if orphans.is_empty() {
            return;
        }

        // 3. Pick a clock. Prefer one used by an existing RegBlock so
        // the orphans land in the same clock domain as the other
        // flops. Fall back to the first Clock-typed input port.
        let rb_clock: Option<&Ident> = m.body.iter().find_map(|i| {
            if let ModuleBodyItem::RegBlock(rb) = i { Some(&rb.clock) } else { None }
        });
        let port_clock = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| &p.name);
        let Some(clock_ident) = rb_clock.or(port_clock) else {
            // No clock available — can't emit a clocked reset. Leave
            // the reg as-is; Verilator will still warn, but that's
            // a more fundamental design issue the user needs to fix.
            return;
        };
        let clk_name = &clock_ident.name;

        // 4. Group by (reset_signal, is_async, is_low). We collapse
        // orphans into one always_ff per group.
        let mut groups: std::collections::BTreeMap<(String, bool, bool), Vec<&Orphan>> =
            std::collections::BTreeMap::new();
        for o in &orphans {
            groups.entry((o.reset_signal.clone(), o.is_async, o.is_low))
                .or_default()
                .push(o);
        }

        // 5. Emit one always_ff per group.
        for ((reset_signal, is_async, is_low), group) in &groups {
            let rst_cond = if *is_low {
                format!("(!{})", reset_signal)
            } else {
                reset_signal.clone()
            };
            if *is_async {
                let rst_edge = if *is_low { "negedge" } else { "posedge" };
                self.line(&format!(
                    "always_ff @(posedge {} or {} {}) begin",
                    clk_name, rst_edge, reset_signal
                ));
            } else {
                self.line(&format!("always_ff @(posedge {}) begin", clk_name));
            }
            self.indent += 1;
            self.line(&format!("if ({}) begin", rst_cond));
            self.indent += 1;
            for o in group {
                self.emit_reset_only_assignment(o.name.as_str(), o.ty, o.init.as_str());
            }
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");
        }
    }

    /// Emit a single `<name> <= <init>;` with for-loop unpacking if the
    /// reg is a Vec (unpacked array). Mirrors the per-name emission in
    /// `emit_reg_block`'s reset branch, just factored out for reuse.
    fn emit_reset_only_assignment(&mut self, name: &str, ty: &TypeExpr, init: &str) {
        // Collect Vec dimensions (outermost first — same order as the
        // SV unpacked-array suffix).
        let mut dims: Vec<String> = Vec::new();
        let mut t = ty;
        while let TypeExpr::Vec(inner, size) = t {
            dims.push(self.emit_expr_str(size));
            t = inner;
        }
        if dims.is_empty() {
            self.line(&format!("{name} <= {init};"));
            return;
        }
        let idx_vars: Vec<String> = (0..dims.len()).map(|d| format!("__ri{d}")).collect();
        for (d, dim_size) in dims.iter().enumerate() {
            self.line(&format!(
                "for (int {} = 0; {} < {}; {}++) begin",
                idx_vars[d], idx_vars[d], dim_size, idx_vars[d]
            ));
            self.indent += 1;
        }
        let idx_str: String = idx_vars.iter().map(|v| format!("[{v}]")).collect();
        self.line(&format!("{name}{idx_str} <= {init};"));
        for _ in 0..dims.len() {
            self.indent -= 1;
            self.line("end");
        }
    }

    fn reset_value_expr(reset: &RegReset) -> Option<&Expr> {
        match reset {
            RegReset::None => None,
            RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) => Some(val),
        }
    }

    fn resolve_reg_reset(&self, reset: &RegReset, m: &ModuleDecl) -> Option<(String, bool, bool)> {
        match reset {
            RegReset::None => Option::None,
            RegReset::Explicit(signal, kind, level, _) => {
                Some((
                    signal.name.clone(),
                    *kind == ResetKind::Async,
                    *level == ResetLevel::Low,
                ))
            }
            RegReset::Inherit(signal, _) => {
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
                Stmt::For(f) => {
                    Self::collect_assigned_roots(&f.body, out);
                }
                Stmt::Init(ib) => {
                    Self::collect_assigned_roots(&ib.body, out);
                }
                Stmt::WaitUntil(_, _) => {}
                Stmt::DoUntil { body, .. } => {
                    Self::collect_assigned_roots(body, out);
                }
            }
        }
    }

    /// Check if an expression produces a signed (SInt) value.
    fn expr_is_signed(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Cast(_, ty) => matches!(&**ty, TypeExpr::SInt(_)),
            ExprKind::Ident(name) => self.ident_is_sint(name),
            ExprKind::MethodCall(recv, method, _) => {
                // sext always produces signed; trunc preserves signedness
                match method.name.as_str() {
                    "sext" => true,
                    "trunc" | "resize" => self.expr_is_signed(recv),
                    _ => false,
                }
            }
            ExprKind::Signed(_) => true,
            ExprKind::Unsigned(_) => false,
            ExprKind::Binary(_, lhs, _) => self.expr_is_signed(lhs),
            _ => false,
        }
    }

    /// Check if an identifier is declared as SInt in the current construct's scope.
    fn ident_is_sint(&self, name: &str) -> bool {
        if let Some(scope) = self.symbols.module_scopes.get(&self.current_construct) {
            if let Some((sym, _)) = scope.get(name) {
                return match sym {
                    Symbol::Port(p) => matches!(&p.ty, TypeExpr::SInt(_)),
                    Symbol::Reg(r) => matches!(&r.ty, TypeExpr::SInt(_)),
                    Symbol::Let(_) => self.let_binding_is_sint(name),
                    _ => false,
                };
            }
        }
        false
    }

    /// Check if a let binding is typed as SInt by looking up the AST.
    fn let_binding_is_sint(&self, name: &str) -> bool {
        for item in &self.source.items {
            if let Item::Module(m) = item {
                if m.name.name == self.current_construct {
                    for bi in &m.body {
                        if let ModuleBodyItem::LetBinding(l) = bi {
                            if l.name.name == name {
                                return l.ty.as_ref().map_or(false, |t| matches!(t, TypeExpr::SInt(_)));
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Try to detect indexed part-select pattern: hi = lo + (width - 1).
    /// Returns Some(width) if the width is a compile-time constant,
    /// enabling emission of `base[lo +: width]` instead of `base[hi:lo]`.
    fn try_indexed_part_select(hi: &Expr, lo: &Expr) -> Option<String> {
        // Try to check if hi == lo + (W - 1) structurally.
        // Strategy: subtract lo from hi symbolically, add 1, and see if we get a constant.
        // We do this by collecting all terms as (coefficient, variable_or_empty) pairs.

        // Simpler approach: check the common pattern where
        // hi = Binary(Add, Binary(Mul, var, W), Binary(Sub, W, 1))
        // or hi = Binary(Sub, Binary(Add, Binary(Mul, var, W), W), 1)
        // and lo = Binary(Mul, var, W)
        //
        // Most robust: check if hi and lo both contain a non-constant sub-expression,
        // and if (hi - lo) const-evaluates to a constant.
        fn try_const_eval(expr: &Expr) -> Option<i64> {
            match &expr.kind {
                ExprKind::Literal(lit) => {
                    let val = match lit {
                        LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => *v as i64,
                        LitKind::Sized(_, v) => *v as i64,
                    };
                    Some(val)
                }
                ExprKind::Binary(op, lhs, rhs) => {
                    let l = try_const_eval(lhs)?;
                    let r = try_const_eval(rhs)?;
                    match op {
                        BinOp::Add => Some(l + r),
                        BinOp::Sub => Some(l - r),
                        BinOp::Mul => Some(l * r),
                        _ => None,
                    }
                }
                _ => None, // Contains variable — not a constant
            }
        }

        // Check if lo contains any non-constant part (otherwise static slice is fine)
        if try_const_eval(lo).is_some() {
            return None; // Both constant — normal [hi:lo] is fine
        }

        // Collect additive terms from an expression: returns Vec<(sign, term)>
        // where term is either a constant or an opaque expression.
        // Produce a span-independent string key for an expression
        fn expr_key(expr: &Expr) -> String {
            match &expr.kind {
                ExprKind::Ident(name) => name.clone(),
                ExprKind::Literal(lit) => match lit {
                    LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => format!("{v}"),
                    LitKind::Sized(w, v) => format!("{w}'{v}"),
                },
                ExprKind::Binary(op, lhs, rhs) => {
                    format!("({} {:?} {})", expr_key(lhs), op, expr_key(rhs))
                }
                ExprKind::Unary(op, inner) => format!("{:?}({})", op, expr_key(inner)),
                ExprKind::Index(base, idx) => format!("{}[{}]", expr_key(base), expr_key(idx)),
                ExprKind::FieldAccess(base, field) => format!("{}.{}", expr_key(base), field.name),
                _ => format!("{:?}", std::mem::discriminant(&expr.kind)),
            }
        }

        fn collect_terms(expr: &Expr, sign: i64, terms: &mut Vec<(i64, Option<i64>, String)>) {
            match &expr.kind {
                ExprKind::Literal(lit) => {
                    let val = match lit {
                        LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => *v as i64,
                        LitKind::Sized(_, v) => *v as i64,
                    };
                    terms.push((sign, Some(val), String::new()));
                }
                ExprKind::Binary(BinOp::Add, lhs, rhs) => {
                    collect_terms(lhs, sign, terms);
                    collect_terms(rhs, sign, terms);
                }
                ExprKind::Binary(BinOp::Sub, lhs, rhs) => {
                    collect_terms(lhs, sign, terms);
                    collect_terms(rhs, -sign, terms);
                }
                _ => {
                    // Opaque expression — use span-free representation as key
                    terms.push((sign, None, expr_key(expr)));
                }
            }
        }

        let mut hi_terms = Vec::new();
        let mut lo_terms = Vec::new();
        collect_terms(hi, 1, &mut hi_terms);
        collect_terms(lo, -1, &mut lo_terms);

        // Compute (hi - lo + 1): cancel non-constant terms, sum constants
        let mut all_terms = hi_terms;
        all_terms.extend(lo_terms);

        // Separate constants and variable terms
        let mut const_sum: i64 = 1; // the +1
        let mut var_terms: Vec<(i64, String)> = Vec::new();

        for (sign, val, key) in &all_terms {
            if let Some(v) = val {
                const_sum += sign * v;
            } else {
                var_terms.push((*sign, key.clone()));
            }
        }

        // Check if variable terms cancel out
        let mut var_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for (sign, key) in &var_terms {
            *var_map.entry(key.clone()).or_insert(0) += sign;
        }

        // Collect remaining (non-cancelled) variable terms
        let remaining_vars: Vec<(&String, &i64)> = var_map.iter().filter(|(_, &v)| v != 0).collect();

        if remaining_vars.is_empty() && const_sum > 0 {
            // Pure constant width
            Some(const_sum.to_string())
        } else if remaining_vars.len() == 1 {
            let (key, &coeff) = remaining_vars[0];
            if coeff == 1 {
                // Width = key + const_sum (key is an expression like a param name)
                // Only emit if key looks like a simple identifier or expression
                // (not something with parentheses that would be ambiguous)
                if const_sum == 0 {
                    Some(key.clone())
                } else if const_sum > 0 {
                    Some(format!("{key} + {const_sum}"))
                } else {
                    Some(format!("{key} - {}", -const_sum))
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    fn expr_root_name(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::FieldAccess(base, _) => Self::expr_root_name(base),
            ExprKind::Index(base, _) | ExprKind::BitSlice(base, _, _) | ExprKind::PartSelect(base, _, _, _) => Self::expr_root_name(base),
            ExprKind::LatencyAt(inner, _) => Self::expr_root_name(inner),
            _ => String::new(),
        }
    }

    /// Extract reset info from a port list: (name, is_async, is_low).
    /// Returns ("rst", false, false) as defaults if no Reset port found.
    fn extract_reset_info(ports: &[PortDecl]) -> (String, bool, bool) {
        crate::ast::extract_reset_info(ports)
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
        self.emit_comments_before(stmt_span_start(stmt));
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
                let u = if m.unique { "unique " } else { "" };
                self.line(&format!("{}case ({})", u, scrut));
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
            Stmt::For(f) => {
                self.emit_for_loop_sv(f, |s, stmt| s.emit_reg_stmt(stmt));
            }
            Stmt::Init(_ib) => {
                // init blocks are extracted and emitted by emit_reg_block before this point
                unreachable!("Stmt::Init should be handled by emit_reg_block, not emit_reg_stmt")
            }
            Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => {
                unreachable!("WaitUntil/DoUntil only valid in pipeline stage seq blocks")
            }
        }
    }

    fn emit_reg_if_else(&mut self, ie: &IfElse) {
        self.emit_reg_if_else_inner(ie, false);
    }

    fn emit_reg_if_else_inner(&mut self, ie: &IfElse, is_chain: bool) {
        let cond = self.emit_expr_str(&ie.cond);
        let u = if ie.unique && !is_chain { "unique " } else { "" };
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("{}if ({}) begin", u, cond));
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
        } else if let Some((Symbol::Fsm(info), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            info.ports.clone()
        } else if let Some((Symbol::Ram(_), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            // RAM uses port groups — handle separately below
            Vec::new()
        } else if let Some((Symbol::Regfile(_), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            // Regfile uses port arrays — handle separately below
            Vec::new()
        } else {
            return;
        };

        // For RAM instances, build a flattened port map from port groups
        // Resolve type params (e.g. WIDTH → UInt<32>) from the RAM's param list.
        let ram_flat_ports: Vec<(String, TypeExpr)> = if let Some((Symbol::Ram(_), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            let mut flat = Vec::new();
            for item in &self.source.items {
                if let Item::Ram(r) = item {
                    if r.name.name == inst.module_name.name {
                        // Build type param map: param name → resolved TypeExpr
                        let type_params: std::collections::HashMap<String, TypeExpr> = r.params.iter()
                            .filter_map(|p| match &p.kind {
                                crate::ast::ParamKind::Type(ty) => Some((p.name.name.clone(), ty.clone())),
                                _ => None,
                            })
                            .collect();
                        for pg in &r.port_groups {
                            for s in &pg.signals {
                                // Resolve Named type params to their actual types
                                let resolved_ty = match &s.ty {
                                    TypeExpr::Named(ident) => {
                                        type_params.get(&ident.name).cloned().unwrap_or_else(|| s.ty.clone())
                                    }
                                    other => other.clone(),
                                };
                                flat.push((
                                    format!("{}_{}", pg.name.name, s.name.name),
                                    resolved_ty,
                                ));
                            }
                        }
                        break;
                    }
                }
            }
            flat
        } else {
            Vec::new()
        };

        // For Regfile instances, build a flattened port map from port arrays
        let regfile_flat_ports: Vec<(String, TypeExpr)> = if let Some((Symbol::Regfile(_), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            let mut flat = Vec::new();
            for item in &self.source.items {
                if let Item::Regfile(r) = item {
                    if r.name.name == inst.module_name.name {
                        // Scalar ports
                        for p in &r.ports {
                            flat.push((p.name.name.clone(), p.ty.clone()));
                        }
                        // Read port array: read{i}_signal
                        if let Some(rp) = &r.read_ports {
                            let count = self.resolve_regfile_count(&rp.count_expr, r);
                            for i in 0..count {
                                for s in &rp.signals {
                                    flat.push((format!("{}{i}_{}", rp.name.name, s.name.name), s.ty.clone()));
                                }
                            }
                        }
                        // Write port array: write{i}_signal
                        if let Some(wp) = &r.write_ports {
                            let count = self.resolve_regfile_count(&wp.count_expr, r);
                            for i in 0..count {
                                for s in &wp.signals {
                                    flat.push((format!("{}{i}_{}", wp.name.name, s.name.name), s.ty.clone()));
                                }
                            }
                        }
                        break;
                    }
                }
            }
            flat
        } else {
            Vec::new()
        };

        // Implicit bus wires: for any inst connection on a bus port
        // whose parent-side signal is an undeclared Ident, declare each
        // flattened bus signal as a wire on the parent. Mirrors the
        // sim_codegen fix from PRs #110+#112. Without this, Verilator
        // creates 1-bit IMPLICIT wires that silently truncate wider
        // signals like `_flits_send_data` and the design appears dead.
        let mut bus_emitted: std::collections::HashSet<String> = std::collections::HashSet::new();
        for conn in &inst.connections {
            let Some(port) = module_ports.iter().find(|p| p.name.name == conn.port_name.name) else { continue; };
            let Some(bi) = &port.bus_info else { continue; };
            let ExprKind::Ident(parent_name) = &conn.signal.kind else { continue; };
            let Some((Symbol::Bus(bus_info), _)) =
                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
            let mut pm = bus_info.default_param_map();
            for pa in &bi.params { pm.insert(pa.name.name.clone(), &pa.value); }
            for (sname, _sdir, ty) in bus_info.effective_signals(&pm) {
                let flat = format!("{parent_name}_{sname}");
                if declared.contains(&flat) || !bus_emitted.insert(flat.clone()) { continue; }
                let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&ty);
                self.line(&format!("{} {}{};", ty_str, flat, arr_suffix));
            }
        }

        for conn in &inst.connections {
            if conn.direction != ConnectDir::Output {
                continue;
            }
            if let ExprKind::Ident(target) = &conn.signal.kind {
                if declared.contains(target) {
                    continue;
                }
                // Bus ports are handled above as a separate pass; skip.
                if let Some(port) = module_ports.iter().find(|p| p.name.name == conn.port_name.name) {
                    if port.bus_info.is_some() { continue; }
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&port.ty);
                    self.line(&format!("{} {}{};", ty_str, target, arr_suffix));
                } else if let Some((_, ty)) = ram_flat_ports.iter().find(|(n, _)| *n == conn.port_name.name) {
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                    self.line(&format!("{} {}{};", ty_str, target, arr_suffix));
                } else if let Some((_, ty)) = regfile_flat_ports.iter().find(|(n, _)| *n == conn.port_name.name) {
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                    self.line(&format!("{} {}{};", ty_str, target, arr_suffix));
                } else {
                    self.line(&format!("logic {};", target));
                }
            }
        }
    }

    fn resolve_regfile_count(&self, expr: &crate::ast::Expr, r: &crate::ast::RegfileDecl) -> u64 {
        use crate::ast::{ExprKind, LitKind, ParamKind};
        match &expr.kind {
            ExprKind::Literal(LitKind::Dec(v)) => *v,
            ExprKind::Ident(name) => {
                r.params.iter()
                    .find(|p| p.name.name == *name)
                    .and_then(|p| match &p.kind {
                        ParamKind::Const | ParamKind::WidthConst(..) => p.default.as_ref(),
                        _ => None,
                    })
                    .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
                    .unwrap_or(1)
            }
            _ => 1,
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

        // Expand bus port connections: one bus connect → N signal connects
        let mut connections: Vec<String> = Vec::new();
        // Find the target construct's ports to detect bus ports (modules and FSMs)
        let target_bus_ports: Vec<(String, String, Vec<ParamAssign>)> = {
            let target_ports: Option<&[PortDecl]> = self.source.items.iter()
                .find_map(|item| match item {
                    Item::Module(m) if m.name.name == inst.module_name.name => Some(m.ports.as_slice()),
                    Item::Fsm(f) if f.name.name == inst.module_name.name => Some(f.ports.as_slice()),
                    _ => None,
                });
            target_ports.map(|ports| ports.iter()
                .filter_map(|p| p.bus_info.as_ref().map(|bi| (p.name.name.clone(), bi.bus_name.name.clone(), bi.params.clone())))
                .collect())
                .unwrap_or_default()
        };

        for c in &inst.connections {
            if let Some((_, bus_name, bus_params)) = target_bus_ports.iter().find(|(pn, _, _)| *pn == c.port_name.name) {
                // Bus connection — expand to individual signals
                if let Some((crate::resolve::Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name) {
                    let mut param_map: std::collections::HashMap<String, &Expr> = info.params.iter()
                        .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                        .collect();
                    for pa in bus_params {
                        param_map.insert(pa.name.name.clone(), &pa.value);
                    }
                    let eff_signals = info.effective_signals(&param_map);
                    let sig_str = self.emit_expr_str(&c.signal);
                    for (sname, _, _) in &eff_signals {
                        connections.push(format!(".{}_{}({}_{})", c.port_name.name, sname, sig_str, sname));
                    }
                }
            } else {
                connections.push(format!(".{}({})", c.port_name.name, self.emit_expr_str(&c.signal)));
            }
        }

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

    fn emit_generate(&mut self, gen: &GenerateDecl) {
        match gen {
            GenerateDecl::For(gf) => {
                let var = &gf.var.name;
                let start_str = self.emit_expr_str(&gf.start);
                let end_str = self.emit_expr_str(&gf.end);
                self.line(&format!("genvar {var};"));
                self.line(&format!(
                    "for ({var} = {start_str}; {var} <= {end_str}; {var} = {var} + 1) begin : gen_{var}",
                ));
                self.indent += 1;
                for item in &gf.items {
                    match item {
                        GenItem::Inst(inst) => self.emit_inst(inst),
                        GenItem::Port(_) => unreachable!("port GenItems should have been lifted by elaboration"),
                        GenItem::Thread(_) => unreachable!("thread GenItems should have been lowered by elaboration"),
                        GenItem::Seq(_) | GenItem::Comb(_) => unreachable!(
                            "seq/comb GenItems should have been unrolled by elaboration"),
                        GenItem::Assert(_) => {
                            // SVA inside generate for: not yet supported in SV codegen (SVA needs static clock ref)
                        }
                    }
                }
                self.indent -= 1;
                self.line("end");
            }
            GenerateDecl::If(gi) => {
                let cond_str = self.emit_expr_str(&gi.cond);
                self.line(&format!("if ({cond_str}) begin : gen_if"));
                self.indent += 1;
                for item in &gi.then_items {
                    match item {
                        GenItem::Inst(inst) => self.emit_inst(inst),
                        GenItem::Port(_) => unreachable!("port GenItems should have been lifted by elaboration"),
                        GenItem::Thread(_) => unreachable!("thread GenItems should have been lowered by elaboration"),
                        GenItem::Seq(_) | GenItem::Comb(_) => unreachable!(
                            "seq/comb GenItems should have been lifted by elaboration"),
                        GenItem::Assert(_) => {}
                    }
                }
                self.indent -= 1;
                if !gi.else_items.is_empty() {
                    self.line("end else begin : gen_else");
                    self.indent += 1;
                    for item in &gi.else_items {
                        match item {
                            GenItem::Inst(inst) => self.emit_inst(inst),
                            GenItem::Port(_) => unreachable!("port GenItems should have been lifted by elaboration"),
                            GenItem::Thread(_) => unreachable!("thread GenItems should have been lowered by elaboration"),
                            GenItem::Seq(_) | GenItem::Comb(_) => unreachable!(
                                "seq/comb GenItems should have been lifted by elaboration"),
                            GenItem::Assert(_) => {}
                        }
                    }
                    self.indent -= 1;
                }
                self.line("end");
            }
        }
    }

    fn emit_assert_sva(&mut self, a: &AssertDecl, construct_name: &str, clk: &str) {
        let expr_str = self.emit_expr_str(&a.expr);
        let label = a.name.as_ref().map(|n| n.name.as_str().to_string())
            .unwrap_or_else(|| match a.kind {
                AssertKind::Assert => "_assert_anon".to_string(),
                AssertKind::Cover  => "_cover_anon".to_string(),
            });
        match a.kind {
            AssertKind::Assert => {
                self.line(&format!(
                    "{label}: assert property (@(posedge {clk}) {expr_str})"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"ASSERTION FAILED: {construct_name}.{label}\");"
                ));
            }
            AssertKind::Cover => {
                self.line(&format!(
                    "{label}: cover property (@(posedge {clk}) {expr_str});"
                ));
            }
        }
    }

    /// Emit assert/cover SVA for construct-level assert declarations (FSM, FIFO, etc.)
    /// Wrapped in translate_off/on so synthesis tools and Yosys ignore the SVA.
    fn emit_asserts_for_construct(&mut self, asserts: &[AssertDecl], name: &str, clk: &str) {
        if asserts.is_empty() { return; }
        self.line("// synopsys translate_off");
        for a in asserts {
            self.emit_assert_sva(a, name, clk);
        }
        self.line("// synopsys translate_on");
    }

    /// For each `reg ... guard <sig>` in the module, emit:
    ///   1. A shadow `_<reg>_written` flag, set on any seq-block commit for the reg.
    ///   2. An SVA contract `<sig> |-> _<reg>_written` (in translate_off).
    /// This catches the producer-bug pattern: `valid` asserts but data was never
    /// written. Verilator `--assert` and EBMC formal both consume this.
    ///
    /// v1 uses a coarse "written at least once after reset" approximation:
    /// the shadow flag is set whenever the ff block's reset branch is NOT taken
    /// (i.e. any non-reset cycle). This may over-approximate (flag goes high
    /// before the actual `<reg> <= ...` assignment), which is safe — it only
    /// misses some bug detections, never false-alarms.
    fn emit_guard_contracts(&mut self, m: &ModuleDecl) {
        let mut guarded: Vec<(String, String, crate::ast::RegReset)> = Vec::new();
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let Some(ref g) = r.guard {
                    guarded.push((r.name.name.clone(), g.name.clone(), r.reset.clone()));
                }
            }
        }
        for p in &m.ports {
            if let Some(ri) = &p.reg_info {
                if let Some(ref g) = ri.guard {
                    guarded.push((p.name.name.clone(), g.name.clone(), ri.reset.clone()));
                }
            }
        }
        if guarded.is_empty() { return; }

        let clk = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst_name, _, is_low) = Self::extract_reset_info(&m.ports);
        let rst_active = if is_low { format!("!{rst_name}") } else { rst_name.clone() };

        self.line("");
        self.line("// synopsys translate_off");
        self.line("// Guard-contract shadow regs + SVA (one per `reg ... guard <sig>`)");
        for (reg_name, guard_sig, _) in &guarded {
            // Collect the disjunction of conditions under which `reg_name` is written.
            // If reg_name is never assigned anywhere, condition is just `false`.
            let write_conds = self.collect_write_conds(m, reg_name);
            let write_cond_expr = if write_conds.is_empty() {
                "1'b0".to_string()
            } else {
                // OR-reduce
                write_conds.iter().map(|s| format!("({s})")).collect::<Vec<_>>().join(" || ")
            };

            // Shadow "written at least once" flag; goes high only when reg is actually assigned
            self.line(&format!("logic _{reg_name}_written;"));
            self.line(&format!("always_ff @(posedge {clk}) begin"));
            self.indent += 1;
            self.line(&format!("if ({rst_active}) _{reg_name}_written <= 1'b0;"));
            self.line(&format!("else if ({write_cond_expr}) _{reg_name}_written <= 1'b1;"));
            self.indent -= 1;
            self.line("end");
            // SVA contract (disable iff rst to exclude reset states from evaluation)
            self.line(&format!(
                "_{reg_name}_guard_contract: assert property \
                 (@(posedge {clk}) disable iff ({rst_active}) {guard_sig} |-> _{reg_name}_written)"
            ));
            self.line(&format!(
                "  else $fatal(1, \"GUARD VIOLATION: {mod}.{reg_name} — \
                 {guard_sig} asserted but {reg_name} never written\");",
                mod = m.name.name,
            ));
        }
        self.line("// synopsys translate_on");
    }

    /// Emit concurrent SVA safety checks for runtime-risky expressions in
    /// seq/latch blocks. Covers two classes:
    ///   * Bounds: Vec indexing, bit-select, variable part-select — mirrors
    ///     arch sim's `_ARCH_BCHK` runtime aborts.
    ///   * Divide-by-zero: `/` and `%` with non-const divisor — mirrors
    ///     arch sim's `_ARCH_DCHK`.
    ///
    /// **Scope** — seq/latch contexts only. Accesses that appear exclusively
    /// in comb blocks or `let` bindings are not covered here; concurrent
    /// assertions can't catch sub-cycle glitches, and wiring in immediate
    /// assertions inside generated `always_comb` is a future extension.
    /// The arch sim runtime checks (`_ARCH_BCHK`, `_ARCH_DCHK`) still fire
    /// for those paths.
    fn emit_bound_asserts(&mut self, m: &ModuleDecl) {
        // Collect const-param names — identifiers bound to compile-time constants.
        // `is_const_reducible_with` treats these as foldable so divisors named by
        // them do not produce spurious assertions.
        let const_params: std::collections::HashSet<String> = m.params.iter()
            .filter(|p| matches!(&p.kind, ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::EnumConst(_)))
            .map(|p| p.name.name.clone())
            .collect();

        // Build Vec<T,N> size and scalar-width lookups for accesses in this module.
        let mut vec_sizes: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut scalar_widths: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let record = |name: &str, ty: &TypeExpr,
                      vec_sizes: &mut std::collections::HashMap<String, String>,
                      scalar_widths: &mut std::collections::HashMap<String, String>| {
            match ty {
                TypeExpr::Vec(_, count) => {
                    let s = Self::expr_to_sv_const(count);
                    vec_sizes.insert(name.to_string(), s);
                }
                TypeExpr::UInt(w) | TypeExpr::SInt(w) => {
                    let s = Self::expr_to_sv_const(w);
                    scalar_widths.insert(name.to_string(), s);
                }
                TypeExpr::Bool | TypeExpr::Bit => {
                    scalar_widths.insert(name.to_string(), "1".to_string());
                }
                _ => {}
            }
        };
        for p in &m.ports {
            if p.bus_info.is_some() { continue; }
            record(&p.name.name, &p.ty, &mut vec_sizes, &mut scalar_widths);
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => record(&r.name.name, &r.ty, &mut vec_sizes, &mut scalar_widths),
                ModuleBodyItem::WireDecl(w) => record(&w.name.name, &w.ty, &mut vec_sizes, &mut scalar_widths),
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(ty) = &l.ty {
                        record(&l.name.name, ty, &mut vec_sizes, &mut scalar_widths);
                    }
                }
                _ => {}
            }
        }

        // Walk seq + latch bodies, collect unique (predicate, label-tag) pairs.
        let mut sites: Vec<(String, String)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in &m.body {
            match item {
                ModuleBodyItem::RegBlock(rb) => {
                    for s in &rb.stmts {
                        self.collect_bound_stmt(s, &vec_sizes, &scalar_widths, &const_params, &mut sites, &mut seen);
                    }
                }
                ModuleBodyItem::LatchBlock(lb) => {
                    for s in &lb.stmts {
                        self.collect_bound_stmt(s, &vec_sizes, &scalar_widths, &const_params, &mut sites, &mut seen);
                    }
                }
                _ => {}
            }
        }
        if sites.is_empty() { return; }

        // Pick the module's clock and reset (best-effort; use first of each).
        let clk = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let rst_active = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });

        // A module with no clock has no meaningful concurrent assertion — skip.
        let Some(clk) = clk else { return; };

        self.line("// synopsys translate_off");
        self.line("// Auto-generated safety assertions (bounds / divide-by-zero)");
        for (i, (predicate, tag)) in sites.iter().enumerate() {
            let is_div0 = tag == "div0" || tag == "mod0";
            let label_prefix = if is_div0 { "_auto_div0" } else { "_auto_bound" };
            let label = format!("{label_prefix}_{}_{}", tag, i);
            let violation_kind = if is_div0 { "DIV-BY-ZERO" } else { "BOUNDS" };
            let disable = rst_active.as_ref()
                .map(|r| format!(" disable iff ({r})"))
                .unwrap_or_default();
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {predicate})"
            ));
            self.line(&format!(
                "  else $fatal(1, \"{violation_kind} VIOLATION: {mod}.{label}\");",
                mod = m.name.name
            ));
        }
        self.line("// synopsys translate_on");
    }

    /// Tier 2 of the handshake primitive: for every bus port on this module
    /// whose bus definition declares `handshake` channels, emit per-variant
    /// concurrent SVA protocol assertions, wrapped in `translate_off/on`.
    ///
    /// Labels follow `_auto_hs_<port>_<channel>_<rule>`, mirroring
    /// `_auto_bound_*` / `_auto_div0_*` for consistency with formal tools
    /// (EBMC, SymbiYosys) and simulator lint (`--assert`).
    ///
    /// The protocol rules are symmetric — they bind regardless of whether
    /// this module is the sender (initiator) or receiver (target), so
    /// perspective-flip on the bus port doesn't change which signals
    /// participate in the property.
    ///
    /// Current coverage (v1): valid_ready → valid-stable-until-ready,
    /// valid_stall → valid-stable-while-stalled, req_ack_4phase →
    /// req-holds-until-ack. Other variants are parsed and their ports
    /// expand correctly, but no auto-SVA is emitted for them yet
    /// (valid_only has no back-signal; ready_only has no valid;
    /// req_ack_2phase requires $past tracking that's deferred).
    fn emit_handshake_asserts(&mut self, m: &ModuleDecl) {
        // Gather (port_name, HandshakeMeta) for each bus-typed port whose
        // bus declares one or more handshake channels.
        let mut emissions: Vec<(String, crate::ast::HandshakeMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else { continue; };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
            for hs in &info.handshakes {
                emissions.push((p.name.name.clone(), hs.clone()));
            }
        }
        if emissions.is_empty() { return; }

        // Reuse the same clock/reset picking convention as emit_bound_asserts.
        let clk = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else { return; };
        let rst_active = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });
        let disable = rst_active.as_ref()
            .map(|r| format!(" disable iff ({r})"))
            .unwrap_or_default();

        self.line("// synopsys translate_off");
        self.line("// Auto-generated handshake protocol assertions (Tier 2)");
        for (port_name, hs) in &emissions {
            let ch = &hs.name.name;
            let variant = hs.variant.name.as_str();
            let sig = |s: &str| format!("{}_{}_{}", port_name, ch, s);
            let mod_name = &m.name.name;
            let emit_property = |cg: &mut Codegen, rule: &str, predicate: String, message: &str| {
                let label = format!("_auto_hs_{}_{}_{}", port_name, ch, rule);
                cg.line(&format!(
                    "{label}: assert property (@(posedge {clk}){disable} {predicate})"
                ));
                cg.line(&format!(
                    "  else $fatal(1, \"HANDSHAKE VIOLATION ({message}): {mod_name}.{label}\");"
                ));
            };
            match variant {
                "valid_ready" => {
                    let v = sig("valid"); let r = sig("ready");
                    emit_property(self, "valid_stable",
                        format!("({v} && !{r}) |=> {v}"),
                        "valid must stay asserted until ready");
                }
                "valid_stall" => {
                    let v = sig("valid"); let s = sig("stall");
                    emit_property(self, "valid_stable_while_stall",
                        format!("({v} && {s}) |=> {v}"),
                        "valid must not change while stalled");
                }
                "req_ack_4phase" => {
                    let rq = sig("req"); let ak = sig("ack");
                    emit_property(self, "req_holds_until_ack",
                        format!("({rq} && !{ak}) |=> {rq}"),
                        "req must stay asserted until ack");
                }
                // Variants with no Tier-2 v1 property are silently skipped.
                _ => {}
            }
        }
        self.line("// synopsys translate_on");
    }

    /// Emit the synthesized credit-counter state for each `send`-role
    /// `credit_channel` sub-construct on a bus port of this module.
    ///
    /// Per port+channel pair, emits three things:
    ///
    /// 1. `logic [W-1:0] __<port>_<ch>_credit;` — the credit register,
    ///    width = clog2(DEPTH+1).
    /// 2. An `always_ff` block that resets the counter to DEPTH on reset
    ///    and updates it each cycle:
    ///       -1 when send_valid && !credit_return
    ///       +1 when credit_return && !send_valid
    ///       no change when both fire in the same cycle (plan §Lowering).
    /// 3. `wire __<port>_<ch>_can_send = __<port>_<ch>_credit != 0;` —
    ///    combinational current-cycle availability. Users whose design
    ///    needs a timing-relief flop will opt in via the upcoming
    ///    `CAN_SEND_REGISTERED` channel param (next-state flop semantics,
    ///    option (b) — see doc/plan_credit_channel.md).
    ///
    /// PR #3b-ii emits only the sender-side state — target-side FIFO +
    /// credit_return-pulse wiring lands in PR #3b-iii; `ch.send()` /
    /// `ch.can_send` method dispatch desugars to `__<port>_<ch>_*` in a
    /// follow-up. Users today can read `__<port>_<ch>_can_send` directly
    /// and drive `<port>_<ch>_send_valid` from their own comb to build
    /// a compliant sender without the sugar.
    fn emit_credit_channel_state(&mut self, m: &ModuleDecl) {
        let mut emissions: Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else { continue; };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
            for cc in &info.credit_channels {
                // Initiator perspective drives send; on the target perspective
                // the same bus flip inverts the data direction, but the sender
                // state belongs on whichever side actually issues sends.
                let is_sender = match (cc.role_dir, bi.perspective) {
                    (Direction::Out, crate::ast::BusPerspective::Initiator) => true,
                    (Direction::In,  crate::ast::BusPerspective::Target)    => true,
                    _ => false,
                };
                if is_sender {
                    emissions.push((p.name.name.clone(), cc.clone()));
                }
            }
        }
        if emissions.is_empty() { return; }

        let clk = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else { return; };
        let rst_port = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)));
        let (rst_edge, rst_active) = match rst_port {
            Some(p) => {
                let active = match &p.ty {
                    TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                    _ => p.name.name.clone(),
                };
                let edge = match &p.ty {
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::Low) => format!(" or negedge {}", p.name.name),
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::High) => format!(" or posedge {}", p.name.name),
                    _ => String::new(),
                };
                (edge, Some(active))
            }
            None => (String::new(), None),
        };

        self.line("");
        self.line("// Auto-generated credit_channel state (PR #3b-ii, sender side)");
        for (port_name, cc) in &emissions {
            let ch = &cc.name.name;
            let depth_expr = cc.params.iter()
                .find(|p| p.name.name == "DEPTH")
                .and_then(|p| p.default.as_ref());
            let Some(depth_expr) = depth_expr else { continue; };
            let depth_str = self.emit_expr_str(depth_expr);
            let credit_reg = format!("__{port_name}_{ch}_credit");
            let cs_name    = format!("__{port_name}_{ch}_can_send");
            let send_valid = format!("{port_name}_{ch}_send_valid");
            let credit_ret = format!("{port_name}_{ch}_credit_return");
            // Look up CAN_SEND_REGISTERED (option b — next-state flop, agreed
            // semantics). Non-zero = register the can_send signal so its
            // fan-out comes off a flop; the combinational critical path then
            // ends at the flop input. Full throughput is preserved because the
            // flopped signal reflects counter_next (current counter ± same-
            // cycle send/return), so send_valid |-> counter > 0 still holds.
            let registered = cc.params.iter()
                .find(|p| p.name.name == "CAN_SEND_REGISTERED")
                .and_then(|p| p.default.as_ref())
                .map(|e| self.emit_expr_str(e))
                .map(|s| s.trim() != "0")
                .unwrap_or(false);
            // Width = $clog2(DEPTH + 1). Emit as-is; SV reduces at elab.
            self.line(&format!(
                "logic [$clog2(({depth_str}) + 1) - 1:0] {credit_reg};"
            ));
            if registered {
                self.line(&format!("logic {cs_name};"));
            } else {
                self.line(&format!("wire  {cs_name} = {credit_reg} != 0;"));
            }
            // Emit the counter update (always_ff). If registered, also flop
            // can_send: `__..._can_send <= counter_next > 0`. The counter_next
            // is not an SV-visible signal; we inline the next-state expression
            // to preserve semantics without introducing an extra wire.
            //
            // counter_next =  credit + 1   when (credit_return && !send_valid)
            //               | credit - 1   when (send_valid && !credit_return)
            //               | credit       otherwise
            //
            // So counter_next > 0 reduces to:
            //   (credit_return && !send_valid) ? 1
            //   : (send_valid && !credit_return) ? (credit > 1)
            //   : (credit > 0)
            let cs_next = format!(
                "({credit_ret} && !{send_valid}) ? 1'b1 : \
                 ({send_valid} && !{credit_ret}) ? ({credit_reg} > 1) : \
                 ({credit_reg} != 0)"
            );
            match &rst_active {
                Some(r) => {
                    self.line(&format!("always_ff @(posedge {clk}{rst_edge}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({r}) begin"));
                    self.indent += 1;
                    self.line(&format!("{credit_reg} <= {depth_str};"));
                    if registered { self.line(&format!("{cs_name} <= ({depth_str}) != 0;")); }
                    self.indent -= 1;
                    self.line("end else begin");
                    self.indent += 1;
                    self.line(&format!("if ({send_valid} && !{credit_ret}) {credit_reg} <= {credit_reg} - 1;"));
                    self.line(&format!("else if ({credit_ret} && !{send_valid}) {credit_reg} <= {credit_reg} + 1;"));
                    if registered { self.line(&format!("{cs_name} <= {cs_next};")); }
                    self.indent -= 1;
                    self.line("end");
                    self.indent -= 1;
                    self.line("end");
                }
                None => {
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({send_valid} && !{credit_ret}) {credit_reg} <= {credit_reg} - 1;"));
                    self.line(&format!("else if ({credit_ret} && !{send_valid}) {credit_reg} <= {credit_reg} + 1;"));
                    if registered { self.line(&format!("{cs_name} <= {cs_next};")); }
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }
    }

    /// Emit the receiver-side FIFO + pop wiring for each credit_channel
    /// where this module is the consumer (target on a `send`-role channel,
    /// or initiator on a `receive`-role channel). Pops when the user-driven
    /// `<port>_<ch>_credit_return` is asserted and the FIFO is non-empty.
    ///
    /// Emits the following per (port, credit_channel):
    ///   logic [W-1:0]      __<port>_<ch>_buf [DEPTH];
    ///   logic [AW-1:0]     __<port>_<ch>_head;
    ///   logic [AW-1:0]     __<port>_<ch>_tail;
    ///   logic [OW-1:0]     __<port>_<ch>_occ;     // 0..DEPTH
    ///   wire              __<port>_<ch>_valid = __<port>_<ch>_occ != 0;
    ///   wire [W-1:0]      __<port>_<ch>_data  = __<port>_<ch>_buf[head];
    ///   always_ff          // push on send_valid, pop on credit_return && valid
    ///
    /// Where W = type width of the payload T, AW = $clog2(DEPTH),
    /// OW = $clog2(DEPTH+1).
    ///
    /// Scope note (PR #3b-iii): these wires are SV-level only. ARCH-level
    /// method dispatch (`port.ch.valid`, `port.ch.data`) is not yet wired
    /// up — that lands once the AST-level synthesized-wire story is
    /// locked down. In the interim, the FIFO is observable by reading
    /// the SV names directly (e.g. from a cocotb TB) or by writing raw
    /// send/credit_return drives and trusting the invariants hold.
    fn emit_credit_channel_receiver_state(&mut self, m: &ModuleDecl) {
        let mut emissions: Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else { continue; };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
            for cc in &info.credit_channels {
                // Receiver side mirrors the sender-state selector:
                //   send role + target perspective → this module is the receiver
                //   receive role + initiator perspective → this module is the receiver
                let is_receiver = match (cc.role_dir, bi.perspective) {
                    (Direction::Out, crate::ast::BusPerspective::Target)    => true,
                    (Direction::In,  crate::ast::BusPerspective::Initiator) => true,
                    _ => false,
                };
                if is_receiver {
                    emissions.push((p.name.name.clone(), cc.clone()));
                }
            }
        }
        if emissions.is_empty() { return; }

        let clk = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else { return; };
        let rst_port = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)));
        let (rst_edge, rst_active) = match rst_port {
            Some(p) => {
                let active = match &p.ty {
                    TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                    _ => p.name.name.clone(),
                };
                let edge = match &p.ty {
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::Low) => format!(" or negedge {}", p.name.name),
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::High) => format!(" or posedge {}", p.name.name),
                    _ => String::new(),
                };
                (edge, Some(active))
            }
            None => (String::new(), None),
        };

        self.line("");
        self.line("// Auto-generated credit_channel target-side FIFO (PR #3b-iii)");
        for (port_name, cc) in &emissions {
            let ch = &cc.name.name;
            let depth_expr = cc.params.iter()
                .find(|p| p.name.name == "DEPTH")
                .and_then(|p| p.default.as_ref());
            let Some(depth_expr) = depth_expr else { continue; };
            let depth_str = self.emit_expr_str(depth_expr);
            // Payload type width — resolve via the ParamKind::Type default.
            let payload_ty_opt = cc.params.iter()
                .find(|p| p.name.name == "T")
                .and_then(|p| match &p.kind {
                    crate::ast::ParamKind::Type(te) => Some(te.clone()),
                    _ => None,
                });
            let Some(payload_ty) = payload_ty_opt else { continue; };
            let Some(width_str) = self.type_expr_data_width(&payload_ty) else { continue; };
            let buf = format!("__{port_name}_{ch}_buf");
            let head = format!("__{port_name}_{ch}_head");
            let tail = format!("__{port_name}_{ch}_tail");
            let occ  = format!("__{port_name}_{ch}_occ");
            let valid_w = format!("__{port_name}_{ch}_valid");
            let data_w  = format!("__{port_name}_{ch}_data");
            let push = format!("{port_name}_{ch}_send_valid");
            let pushd= format!("{port_name}_{ch}_send_data");
            let pop_drv = format!("{port_name}_{ch}_credit_return");

            self.line(&format!("logic [({width_str}) - 1:0] {buf} [({depth_str})];"));
            self.line(&format!("logic [$clog2({depth_str}) == 0 ? 0 : $clog2({depth_str}) - 1:0] {head};"));
            self.line(&format!("logic [$clog2({depth_str}) == 0 ? 0 : $clog2({depth_str}) - 1:0] {tail};"));
            self.line(&format!("logic [$clog2(({depth_str}) + 1) - 1:0] {occ};"));
            self.line(&format!("wire  {valid_w} = {occ} != 0;"));
            self.line(&format!("wire [({width_str}) - 1:0] {data_w} = {buf}[{head}];"));

            // Update block: push on send_valid, pop on user-driven credit_return.
            let pop_fire = format!("({pop_drv} && {valid_w})");
            match &rst_active {
                Some(r) => {
                    self.line(&format!("always_ff @(posedge {clk}{rst_edge}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({r}) begin"));
                    self.indent += 1;
                    self.line(&format!("{head} <= 0;"));
                    self.line(&format!("{tail} <= 0;"));
                    self.line(&format!("{occ}  <= 0;"));
                    self.indent -= 1;
                    self.line("end else begin");
                    self.indent += 1;
                    self.line(&format!("if ({push}) begin"));
                    self.indent += 1;
                    self.line(&format!("{buf}[{tail}] <= {pushd};"));
                    self.line(&format!("{tail} <= ({tail} + 1) % ({depth_str});"));
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("if ({pop_fire}) {head} <= ({head} + 1) % ({depth_str});"));
                    self.line(&format!("if ({push} && !{pop_fire}) {occ} <= {occ} + 1;"));
                    self.line(&format!("else if (!{push} &&  {pop_fire}) {occ} <= {occ} - 1;"));
                    self.indent -= 1;
                    self.line("end");
                    self.indent -= 1;
                    self.line("end");
                }
                None => {
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({push}) begin"));
                    self.indent += 1;
                    self.line(&format!("{buf}[{tail}] <= {pushd};"));
                    self.line(&format!("{tail} <= ({tail} + 1) % ({depth_str});"));
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("if ({pop_fire}) {head} <= ({head} + 1) % ({depth_str});"));
                    self.line(&format!("if ({push} && !{pop_fire}) {occ} <= {occ} + 1;"));
                    self.line(&format!("else if (!{push} &&  {pop_fire}) {occ} <= {occ} - 1;"));
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }
    }

    /// "Is this a compile-time reducible constant?" test. Matches the sim-
    /// codegen rule so runtime vs compile-time treatment of divisors stays
    /// consistent. Literals, `$clog2(const)`, arithmetic over reducibles, and
    /// identifier references to const params declared in the current module.
    /// Runs during `emit_bound_asserts`, which already has the module's
    /// const-param set in scope.
    fn is_const_reducible_with(
        e: &Expr,
        const_params: &std::collections::HashSet<String>,
    ) -> bool {
        match &e.kind {
            ExprKind::Literal(_) => true,
            ExprKind::Ident(n) => const_params.contains(n),
            ExprKind::Clog2(a) => Self::is_const_reducible_with(a, const_params),
            ExprKind::Binary(_, a, b) => {
                Self::is_const_reducible_with(a, const_params)
                    && Self::is_const_reducible_with(b, const_params)
            }
            ExprKind::Unary(_, a) => Self::is_const_reducible_with(a, const_params),
            _ => false,
        }
    }

    /// Emit Tier-2 SVA protocol assertions for each credit_channel on this
    /// module. Labels follow `_auto_cc_<port>_<ch>_<rule>`, mirroring the
    /// handshake / bounds / divide-by-zero naming so EBMC and Verilator
    /// `--assert` consumers can target them uniformly.
    ///
    /// Invariants emitted:
    /// - **credit_bounds** (sender): `__<port>_<ch>_credit <= DEPTH`. Holds
    ///   because the counter update is strictly ±1 and the reset value is
    ///   DEPTH — but provable properties catch any future regression that
    ///   double-decrements or misses reset.
    /// - **send_requires_credit** (sender): `send_valid |-> credit > 0`.
    ///   The user is responsible for gating send_valid on can_send; if they
    ///   fail to, this trips.
    /// - **credit_return_requires_buffered** (receiver): `credit_return |->
    ///   __<port>_<ch>_valid`. The receiver must only pulse credit_return
    ///   when the FIFO actually has something to pop; otherwise the sender
    ///   sees a spurious credit and can overflow the buffer.
    ///
    /// Deferred: occupancy = DEPTH - credit (cross-module property; lands
    /// with a hierarchical-formal story).
    fn emit_credit_channel_asserts(&mut self, m: &ModuleDecl) {
        let mut sender_emissions:   Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        let mut receiver_emissions: Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else { continue; };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
            for cc in &info.credit_channels {
                let is_sender = matches!(
                    (cc.role_dir, bi.perspective),
                    (Direction::Out, crate::ast::BusPerspective::Initiator)
                  | (Direction::In,  crate::ast::BusPerspective::Target)
                );
                if is_sender { sender_emissions.push((p.name.name.clone(), cc.clone())); }
                else         { receiver_emissions.push((p.name.name.clone(), cc.clone())); }
            }
        }
        if sender_emissions.is_empty() && receiver_emissions.is_empty() { return; }

        let clk = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else { return; };
        let rst_active = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });
        let disable = rst_active.as_ref()
            .map(|r| format!(" disable iff ({r})"))
            .unwrap_or_default();
        let mod_name = m.name.name.clone();

        self.line("");
        self.line("// synopsys translate_off");
        self.line("// Auto-generated credit_channel protocol assertions (Tier 2)");

        for (port_name, cc) in &sender_emissions {
            let ch = &cc.name.name;
            let Some(depth_expr) = cc.params.iter()
                .find(|p| p.name.name == "DEPTH")
                .and_then(|p| p.default.as_ref()) else { continue; };
            let depth_str  = self.emit_expr_str(depth_expr);
            let credit_reg = format!("__{port_name}_{ch}_credit");
            let send_valid = format!("{port_name}_{ch}_send_valid");

            let label = format!("_auto_cc_{port_name}_{ch}_credit_bounds");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {credit_reg} <= ({depth_str}))"
            ));
            self.line(&format!(
                "  else $fatal(1, \"CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): {mod_name}.{label}\");"
            ));

            let label = format!("_auto_cc_{port_name}_{ch}_send_requires_credit");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {send_valid} |-> {credit_reg} > 0)"
            ));
            self.line(&format!(
                "  else $fatal(1, \"CREDIT-CHANNEL VIOLATION (send without credit): {mod_name}.{label}\");"
            ));
        }

        for (port_name, cc) in &receiver_emissions {
            let ch = &cc.name.name;
            let credit_ret = format!("{port_name}_{ch}_credit_return");
            let buf_valid  = format!("__{port_name}_{ch}_valid");
            let label = format!("_auto_cc_{port_name}_{ch}_credit_return_requires_buffered");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {credit_ret} |-> {buf_valid})"
            ));
            self.line(&format!(
                "  else $fatal(1, \"CREDIT-CHANNEL VIOLATION (credit_return without buffered data): {mod_name}.{label}\");"
            ));
        }

        self.line("// synopsys translate_on");
    }

    /// Stringify a compile-time constant expression to an SV literal/expression.
    /// For the common case (integer literal) just prints the number; for
    /// `$clog2(...)` / param refs / arithmetic, prints the SV form.
    fn expr_to_sv_const(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Literal(LitKind::Dec(v))
            | ExprKind::Literal(LitKind::Hex(v))
            | ExprKind::Literal(LitKind::Bin(v))
            | ExprKind::Literal(LitKind::Sized(_, v)) => v.to_string(),
            ExprKind::Ident(n) => n.clone(),
            _ => "0".to_string(),
        }
    }

    /// Recursively collect bound-assertion sites from a seq-context Stmt.
    fn collect_bound_stmt(
        &self,
        s: &Stmt,
        vec_sizes: &std::collections::HashMap<String, String>,
        scalar_widths: &std::collections::HashMap<String, String>,
        const_params: &std::collections::HashSet<String>,
        sites: &mut Vec<(String, String)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match s {
            Stmt::Assign(a) => {
                self.collect_bound_expr(&a.target, vec_sizes, scalar_widths, const_params, sites, seen);
                self.collect_bound_expr(&a.value, vec_sizes, scalar_widths, const_params, sites, seen);
            }
            Stmt::IfElse(ie) => {
                self.collect_bound_expr(&ie.cond, vec_sizes, scalar_widths, const_params, sites, seen);
                for s in &ie.then_stmts { self.collect_bound_stmt(s, vec_sizes, scalar_widths, const_params, sites, seen); }
                for s in &ie.else_stmts { self.collect_bound_stmt(s, vec_sizes, scalar_widths, const_params, sites, seen); }
            }
            Stmt::Match(m) => {
                self.collect_bound_expr(&m.scrutinee, vec_sizes, scalar_widths, const_params, sites, seen);
                for arm in &m.arms {
                    for s in &arm.body { self.collect_bound_stmt(s, vec_sizes, scalar_widths, const_params, sites, seen); }
                }
            }
            Stmt::For(f) => {
                if let ForRange::Range(lo, hi) = &f.range {
                    self.collect_bound_expr(lo, vec_sizes, scalar_widths, const_params, sites, seen);
                    self.collect_bound_expr(hi, vec_sizes, scalar_widths, const_params, sites, seen);
                }
                for s in &f.body { self.collect_bound_stmt(s, vec_sizes, scalar_widths, const_params, sites, seen); }
            }
            Stmt::Init(ib) => {
                for s in &ib.body { self.collect_bound_stmt(s, vec_sizes, scalar_widths, const_params, sites, seen); }
            }
            Stmt::WaitUntil(e, _) => self.collect_bound_expr(e, vec_sizes, scalar_widths, const_params, sites, seen),
            Stmt::DoUntil { body, cond, .. } => {
                for s in body { self.collect_bound_stmt(s, vec_sizes, scalar_widths, const_params, sites, seen); }
                self.collect_bound_expr(cond, vec_sizes, scalar_widths, const_params, sites, seen);
            }
            Stmt::Log(_) => {}
        }
    }

    /// Recursively collect bound-assertion sites from an expression. At each
    /// Index / PartSelect with a non-literal index whose base is an ident of
    /// known size, push a predicate. Also catches `/` and `%` with non-const
    /// divisor. Dedups by predicate string.
    fn collect_bound_expr(
        &self,
        e: &Expr,
        vec_sizes: &std::collections::HashMap<String, String>,
        scalar_widths: &std::collections::HashMap<String, String>,
        const_params: &std::collections::HashSet<String>,
        sites: &mut Vec<(String, String)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        let idx_is_const = |ex: &Expr| matches!(&ex.kind, ExprKind::Literal(_));
        let base_ident = |ex: &Expr| -> Option<String> {
            if let ExprKind::Ident(n) = &ex.kind { Some(n.clone()) } else { None }
        };
        let push = |predicate: String, tag: &str, sites: &mut Vec<(String, String)>,
                        seen: &mut std::collections::HashSet<String>| {
            if seen.insert(predicate.clone()) {
                sites.push((predicate, tag.to_string()));
            }
        };
        match &e.kind {
            ExprKind::Index(base, idx) => {
                if !idx_is_const(idx) {
                    if let Some(name) = base_ident(base) {
                        let idx_s = self.emit_expr_str(idx);
                        if let Some(limit) = vec_sizes.get(&name) {
                            push(format!("({idx_s}) < ({limit})"), "vec", sites, seen);
                        } else if let Some(w) = scalar_widths.get(&name) {
                            push(format!("({idx_s}) < ({w})"), "bitsel", sites, seen);
                        }
                    }
                }
                self.collect_bound_expr(base, vec_sizes, scalar_widths, const_params, sites, seen);
                self.collect_bound_expr(idx, vec_sizes, scalar_widths, const_params, sites, seen);
            }
            ExprKind::PartSelect(base, start, width, up) => {
                if !idx_is_const(start) {
                    if let Some(name) = base_ident(base) {
                        if let Some(bw) = scalar_widths.get(&name) {
                            let s_s = self.emit_expr_str(start);
                            let w_s = Self::expr_to_sv_const(width);
                            let (pred, tag) = if *up {
                                // [+:W]: need start + W <= bw
                                (format!("(({s_s}) + ({w_s})) <= ({bw})"), "partsel_up")
                            } else {
                                // [-:W]: need start < bw AND start >= W-1
                                (
                                    format!("(({s_s}) < ({bw})) && (({s_s}) >= (({w_s}) - 1))"),
                                    "partsel_down",
                                )
                            };
                            push(pred, tag, sites, seen);
                        }
                    }
                }
                self.collect_bound_expr(base, vec_sizes, scalar_widths, const_params, sites, seen);
                self.collect_bound_expr(start, vec_sizes, scalar_widths, const_params, sites, seen);
            }
            ExprKind::Binary(op, a, b) => {
                // Divide-by-zero assertion: divisor must be non-zero at every
                // clock edge this access is live. Skip if divisor is a
                // compile-time reducible constant (typecheck already rejected
                // literal zero; non-zero folded constants need no check).
                if matches!(op, BinOp::Div | BinOp::Mod)
                    && !Self::is_const_reducible_with(b, const_params)
                {
                    let rhs_s = self.emit_expr_str(b);
                    let tag = if *op == BinOp::Div { "div0" } else { "mod0" };
                    let pred = format!("({rhs_s}) != 0");
                    if seen.insert(pred.clone()) {
                        sites.push((pred, tag.to_string()));
                    }
                }
                self.collect_bound_expr(a, vec_sizes, scalar_widths, const_params, sites, seen);
                self.collect_bound_expr(b, vec_sizes, scalar_widths, const_params, sites, seen);
            }
            ExprKind::Unary(_, a) => self.collect_bound_expr(a, vec_sizes, scalar_widths, const_params, sites, seen),
            ExprKind::Ternary(c, t, f) => {
                self.collect_bound_expr(c, vec_sizes, scalar_widths, const_params, sites, seen);
                self.collect_bound_expr(t, vec_sizes, scalar_widths, const_params, sites, seen);
                self.collect_bound_expr(f, vec_sizes, scalar_widths, const_params, sites, seen);
            }
            ExprKind::MethodCall(base, _, args) => {
                self.collect_bound_expr(base, vec_sizes, scalar_widths, const_params, sites, seen);
                for a in args { self.collect_bound_expr(a, vec_sizes, scalar_widths, const_params, sites, seen); }
            }
            ExprKind::FunctionCall(_, args) => {
                for a in args { self.collect_bound_expr(a, vec_sizes, scalar_widths, const_params, sites, seen); }
            }
            ExprKind::Concat(parts) => {
                for p in parts { self.collect_bound_expr(p, vec_sizes, scalar_widths, const_params, sites, seen); }
            }
            ExprKind::FieldAccess(base, _) => self.collect_bound_expr(base, vec_sizes, scalar_widths, const_params, sites, seen),
            ExprKind::BitSlice(base, _, _) => self.collect_bound_expr(base, vec_sizes, scalar_widths, const_params, sites, seen),
            _ => {}
        }
    }

    /// Walk all seq blocks in the module and return a list of SV-string path
    /// conditions under which `reg_name` is written. For `if cond data <= ...`,
    /// returns `["cond"]`. For `if A data <= 1; else if B data <= 2;`, returns
    /// `["(A)", "(!(A) && (B))"]`. Conditions are AND-ed down the nesting; the
    /// caller OR-reduces them to get the full write condition.
    ///
    /// Used by the guard-contract SVA emitter to tightly track when a guarded
    /// reg has been written at least once.
    fn collect_write_conds(&self, m: &ModuleDecl, reg_name: &str) -> Vec<String> {
        let mut out = Vec::new();
        for item in &m.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                for s in &rb.stmts {
                    self.walk_stmt_for_writes(s, reg_name, &[], &mut out);
                }
            }
        }
        out
    }

    /// Recursively walk a Stmt, appending the path-condition (stringified) to
    /// `out` whenever an assignment to `reg_name` is found.
    /// `path` is the stack of conditions (each already stringified) leading here.
    fn walk_stmt_for_writes(
        &self,
        s: &Stmt,
        reg_name: &str,
        path: &[String],
        out: &mut Vec<String>,
    ) {
        match s {
            Stmt::Assign(a) => {
                // Check if target root is reg_name
                let targets_reg = match &a.target.kind {
                    ExprKind::Ident(n) => n == reg_name,
                    ExprKind::Index(base, _) | ExprKind::FieldAccess(base, _)
                    | ExprKind::BitSlice(base, _, _) | ExprKind::PartSelect(base, _, _, _) => {
                        matches!(&base.kind, ExprKind::Ident(n) if n == reg_name)
                    }
                    _ => false,
                };
                if targets_reg {
                    // Path is the AND of all conditions leading here
                    let cond = if path.is_empty() {
                        "1'b1".to_string()
                    } else {
                        path.join(" && ")
                    };
                    out.push(cond);
                }
            }
            Stmt::IfElse(ie) => {
                let c_str = format!("({})", self.emit_expr_str(&ie.cond));
                let mut then_path: Vec<String> = path.to_vec();
                then_path.push(c_str.clone());
                for child in &ie.then_stmts {
                    self.walk_stmt_for_writes(child, reg_name, &then_path, out);
                }
                let mut else_path: Vec<String> = path.to_vec();
                else_path.push(format!("!{}", c_str));
                for child in &ie.else_stmts {
                    self.walk_stmt_for_writes(child, reg_name, &else_path, out);
                }
            }
            Stmt::Init(ib) => {
                for child in &ib.body {
                    self.walk_stmt_for_writes(child, reg_name, path, out);
                }
            }
            Stmt::For(fl) => {
                for child in &fl.body {
                    self.walk_stmt_for_writes(child, reg_name, path, out);
                }
            }
            // Match and Log: skip for v1 (match with pattern conditions is more complex)
            _ => {}
        }
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
        self.current_construct = f.name.name.clone();
        // Built-in `state` identifier inside fsm scope: read of the current
        // encoded state register. SV emission lowers to `state_r` (the enum
        // register), which implicitly casts to the user-declared output port
        // width. Cleared at the end of emit_fsm.
        self.ident_subst.insert("state".to_string(), "state_r".to_string());
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
                let comma = if i < f.params.len() - 1 { "," } else { "" };
                self.emit_param_decl(p, comma);
            }
            self.indent -= 1;
            self.line(") (");
        }
        // Collect all port lines (bus ports are flattened, same as module codegen)
        self.bus_ports.clear();
        let mut port_lines: Vec<String> = Vec::new();
        for p in f.ports.iter() {
            if let Some(ref bi) = p.bus_info {
                let bus_name = &bi.bus_name.name;
                self.bus_ports.insert(p.name.name.clone(), bus_name.clone());
                if let Some((crate::resolve::Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name) {
                    let mut param_map: std::collections::HashMap<String, &Expr> = info.params.iter()
                        .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                        .collect();
                    for pa in &bi.params {
                        param_map.insert(pa.name.name.clone(), &pa.value);
                    }
                    let eff_signals = info.effective_signals(&param_map);
                    for (sname, sdir, sty) in &eff_signals {
                        let actual_dir = match bi.perspective {
                            BusPerspective::Initiator => *sdir,
                            BusPerspective::Target => (*sdir).flip(),
                        };
                        let dir_str = match actual_dir {
                            Direction::In => "input",
                            Direction::Out => "output",
                        };
                        let subst_ty = Self::subst_type_expr(sty, &param_map);
                        let ty_str = self.emit_port_type_str(&subst_ty);
                        port_lines.push(format!("{} {} {}_{}", dir_str, ty_str, p.name.name, sname));
                    }
                }
            } else {
                let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
                if let TypeExpr::Vec(_, _) = &p.ty {
                    let (base_ty, suffix) = self.emit_type_and_array_suffix(&p.ty);
                    port_lines.push(format!("{dir} {base_ty} {}{suffix}", p.name.name));
                } else {
                    let ty = self.emit_port_type_str(&p.ty);
                    port_lines.push(format!("{dir} {ty} {}", p.name.name));
                }
            }
        }
        self.indent += 1;
        for (i, line) in port_lines.iter().enumerate() {
            let comma = if i < port_lines.len() - 1 { "," } else { "" };
            self.line(&format!("{line}{comma}"));
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

        // ── Datapath register declarations ───────────────────────────────────
        for reg in &f.regs {
            let (ty, arr_suffix) = self.emit_type_and_array_suffix(&reg.ty);
            self.line(&format!("{ty} {}{arr_suffix};", reg.name.name));
        }
        if !f.regs.is_empty() {
            self.line("");
        }

        // ── Let wire declarations ────────────────────────────────────────────
        let port_names_in_fsm: std::collections::HashSet<&str> =
            f.ports.iter().map(|p| p.name.name.as_str()).collect();
        for lb in &f.lets {
            let val = self.emit_expr_str(&lb.value);
            let aliases_port = lb.ty.is_none() && port_names_in_fsm.contains(lb.name.name.as_str());
            if !aliases_port {
                let ty = if let Some(t) = &lb.ty {
                    self.emit_type_str(t)
                } else {
                    "logic".to_string()
                };
                self.line(&format!("{ty} {};", lb.name.name));
            }
            self.line(&format!("assign {} = {};", lb.name.name, val));
        }
        if !f.lets.is_empty() {
            self.line("");
        }

        // ── Wire declarations ───────────────────────────────────────────────
        for w in &f.wires {
            let (ty, arr_suffix) = self.emit_type_and_array_suffix(&w.ty);
            self.line(&format!("{ty} {}{arr_suffix};", w.name.name));
        }
        if !f.wires.is_empty() {
            self.line("");
        }

        // Identify clock and reset port names
        let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
        let clk_name = clk_port.map(|p| p.name.name.as_str()).unwrap_or("clk");
        let (rst_name, is_async, is_low) = Self::extract_reset_info(&f.ports);
        let ff_sens = Self::ff_sensitivity(clk_name, &rst_name, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst_name, is_low);

        // ── State register FF ────────────────────────────────────────────────
        let has_seq = f.states.iter().any(|s| !s.seq_stmts.is_empty());
        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line(&format!("state_r <= {};", f.default_state.name.to_uppercase()));
        // Reset datapath registers
        for reg in &f.regs {
            let reset_expr = Self::reset_value_expr(&reg.reset)
                .or(reg.init.as_ref());
            if let Some(val_expr) = reset_expr {
                let init_str = self.emit_expr_str(val_expr);
                if let TypeExpr::Vec(_, size_expr) = &reg.ty {
                    let sz = self.emit_expr_str(size_expr);
                    let ri = format!("__ri_{}", reg.name.name);
                    self.line(&format!("for (int {ri} = 0; {ri} < {sz}; {ri}++) begin"));
                    self.indent += 1;
                    self.line(&format!("{}[{ri}] <= {init_str};", reg.name.name));
                    self.indent -= 1;
                    self.line("end");
                } else {
                    self.line(&format!("{} <= {init_str};", reg.name.name));
                }
            }
        }
        // Reset port-reg outputs (ports with reg_info)
        for p in &f.ports {
            if let Some(ri) = &p.reg_info {
                let reset_expr = Self::reset_value_expr(&ri.reset)
                    .or(ri.init.as_ref());
                if let Some(val_expr) = reset_expr {
                    let init_str = self.emit_expr_str(val_expr);
                    self.line(&format!("{} <= {init_str};", p.name.name));
                }
            }
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("state_r <= state_next;");
        // Default sequential assignments (before state case)
        for stmt in &f.default_seq {
            self.emit_reg_stmt(stmt);
        }
        // Per-state sequential logic
        if has_seq || !f.default_seq.is_empty() {
            self.line("case (state_r)");
            self.indent += 1;
            for sb in &f.states {
                if sb.seq_stmts.is_empty() {
                    continue;
                }
                self.line(&format!("{}: begin", sb.name.name.to_uppercase()));
                self.indent += 1;
                for stmt in &sb.seq_stmts {
                    self.emit_reg_stmt(stmt);
                }
                self.indent -= 1;
                self.line("end");
            }
            self.line("default: ;");
            self.indent -= 1;
            self.line("endcase");
        }
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
                    let is_true = cond_strs[i] == "1'b1" || cond_strs[i] == "1";
                    if i == 0 && is_true {
                        // First and unconditional — plain assignment
                        self.line(&format!("state_next = {};",
                            tr.target.name.to_uppercase()));
                    } else if i > 0 && is_true {
                        // Last catch-all — emit as else
                        self.line(&format!("else state_next = {};",
                            tr.target.name.to_uppercase()));
                    } else {
                        let kw = if i == 0 { "if" } else { "else if" };
                        self.line(&format!("{kw} ({}) state_next = {};",
                            cond_strs[i], tr.target.name.to_uppercase()));
                    }
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
        let has_comb = !out_ports.is_empty() || !f.default_comb.is_empty()
            || f.states.iter().any(|s| !s.comb_stmts.is_empty());
        if has_comb {
            self.line("always_comb begin");
            self.indent += 1;
            // Default combinational assignments (before state case)
            for stmt in &f.default_comb {
                self.emit_comb_stmt(stmt);
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

        // Auto-generated FSM safety assertions and coverage
        {
            let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
            let clk = clk_port.map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            let (rst_name, _, is_low) = Self::extract_reset_info(&f.ports);
            let rst_inactive = if is_low { rst_name.clone() } else { format!("!{rst_name}") };
            let n = &f.name.name;
            let n_states = f.state_names.len();
            self.line("");
            self.line("// synopsys translate_off");

            // Assert: no illegal state (reset-guarded)
            self.line(&format!(
                "_auto_legal_state: assert property (@(posedge {clk}) {rst_inactive} |-> state_r < {n_states})"
            ));
            self.line(&format!(
                "  else $fatal(1, \"FSM ILLEGAL STATE: {n}.state_r = %0d\", state_r);"
            ));

            // Cover: each state is reachable
            for sn in &f.state_names {
                let su = sn.name.to_uppercase();
                self.line(&format!(
                    "_auto_reach_{}: cover property (@(posedge {clk}) state_r == {su});",
                    sn.name
                ));
            }

            // Cover: each declared transition can fire
            // Use a counter to disambiguate duplicate src→tgt transitions
            {
                let mut tr_counts: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
                for sb in &f.states {
                    let src = sb.name.name.to_uppercase();
                    for tr in &sb.transitions {
                        let tgt = tr.target.name.to_uppercase();
                        let key = (src.clone(), tgt.clone());
                        let count = tr_counts.entry(key).or_insert(0);
                        let suffix = if *count > 0 { format!("_{count}") } else { String::new() };
                        *count += 1;
                        self.line(&format!(
                            "_auto_tr_{src}_to_{tgt}{suffix}: cover property (@(posedge {clk}) state_r == {src} && state_next == {tgt});",
                        ));
                    }
                }
            }

            self.line("// synopsys translate_on");
        }

        // ── Assert / cover SVA ───────────────────────────────────────────────
        if !f.asserts.is_empty() {
            let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
            let clk = clk_port.map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = f.asserts.clone();
            let fname = f.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &fname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
        self.ident_subst.remove("state");
    }

    // ── Pipeline ──────────────────────────────────────────────────────────────

    fn emit_pipeline(&mut self, p: &PipelineDecl) {
        self.current_construct = p.name.name.clone();
        let n = &p.name.name;

        // ── Module header ────────────────────────────────────────────────────
        if p.params.is_empty() {
            self.out.push_str(&format!("module {} (\n", n));
        } else {
            self.out.push_str(&format!("module {} #(\n", n));
            self.indent += 1;
            for (i, param) in p.params.iter().enumerate() {
                let comma = if i < p.params.len() - 1 { "," } else { "" };
                self.emit_param_decl(param, comma);
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
                    let init_str = if let Some(reset_val) = Self::reset_value_expr(&r.reset) {
                        self.emit_expr_str(reset_val)
                    } else if let Some(ref init_expr) = r.init {
                        self.emit_expr_str(init_expr)
                    } else {
                        "0".to_string()
                    };
                    regs.push((r.name.name.clone(), ty_str, init_str));
                }
                // LetBindings in stages are combinational wires — add to stage_regs
                // so they get declared as `logic` and their names get stage-prefixed.
                if let ModuleBodyItem::LetBinding(l) = item {
                    let ty_str = if let Some(ref te) = l.ty {
                        self.emit_logic_type_str(te)
                    } else {
                        "logic".to_string()
                    };
                    regs.push((l.name.name.clone(), ty_str, String::new())); // empty init = comb wire
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

        // ── Detect wait-stages (variable-latency with wait until / do..until) ─
        let wait_stage_flags: Vec<bool> = p.stages.iter().map(|s| Self::stage_has_wait(s)).collect();
        let has_any_wait_stage = wait_stage_flags.iter().any(|f| *f);

        // Declare FSM state registers for wait-stages
        if has_any_wait_stage {
            self.line("// ── Wait-stage FSM registers ──");
            for (si, stage) in p.stages.iter().enumerate() {
                if !wait_stage_flags[si] { continue; }
                let prefix = stage.name.name.to_lowercase();
                let n_states = Self::count_wait_fsm_states(stage);
                let bits = if n_states <= 2 { 1 } else { ((n_states as f64).log2().ceil()) as usize };
                self.line(&format!("logic [{}:0] {prefix}_fsm_state;", bits - 1));
                self.line(&format!("logic {prefix}_fsm_busy;"));
            }
            self.line("");
        }

        // ── Per-stage stall signals ──────────────────────────────────────────
        // Determine whether any stage or the pipeline has stall conditions.
        let has_per_stage_stall = p.stages.iter().any(|s| s.stall_cond.is_some());
        let has_global_stall = !p.stall_conds.is_empty();
        let has_any_stall = has_per_stage_stall || has_global_stall || has_any_wait_stage;

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

                // Wait-stage FSM busy signal
                if wait_stage_flags[si] {
                    let pfx = stage_names[si].to_lowercase();
                    parts.push(format!("{pfx}_fsm_busy"));
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

        // ── Wait-stage FSM busy assignments ─────────────────────────────────
        if has_any_wait_stage {
            for (si, stage) in p.stages.iter().enumerate() {
                if !wait_stage_flags[si] { continue; }
                let prefix = stage.name.name.to_lowercase();
                // FSM is busy when not in idle state (state 0)
                self.line(&format!("assign {prefix}_fsm_busy = ({prefix}_fsm_state != '0);"));
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
            if wait_stage_flags[si] {
                self.line(&format!("{prefix}_fsm_state <= '0;"));
            }
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

            if wait_stage_flags[si] {
                // ── Wait-stage: generate FSM transition logic ────────────
                self.emit_pipeline_wait_stage_ff(
                    stage, &prefix, si, &stage_names, &stage_regs, &port_names,
                );
            } else if has_any_stall {
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
            // Reset FSM state on flush for wait-stages
            let flush_si = stage_names.iter().position(|n| n.to_lowercase() == target_prefix);
            if let Some(si) = flush_si {
                if wait_stage_flags[si] {
                    self.line(&format!("{target_prefix}_fsm_state <= '0;"));
                }
                // `flush ... clear`: also reset every data reg in the
                // target stage to its declared reset value. Comb wires
                // (init_str empty) are skipped — they're not registers.
                if flush.clear {
                    for (sig_name, _ty, init_str) in &stage_regs[si] {
                        if init_str.is_empty() { continue; }
                        self.line(&format!("{target_prefix}_{sig_name} <= {init_str};"));
                    }
                }
            }
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
                                let target = if let ExprKind::Ident(name) = &a.target.kind {
                                    if port_names.contains(name) {
                                        name.clone()
                                    } else {
                                        format!("{}_{}", prefix, name)
                                    }
                                } else {
                                    self.emit_expr_str(&a.target)
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
                    self.line(&format!("assign {}_{} = {};", prefix, l.name.name, val));
                }
                if let ModuleBodyItem::Inst(inst) = item {
                    self.emit_pipeline_inst(inst, &prefix, si, &stage_names, &stage_regs, &port_names);
                }
            }
        }

        // ── Assert / cover SVA ───────────────────────────────────────────────
        if !p.asserts.is_empty() {
            let clk = p.ports.iter().find(|pt| matches!(&pt.ty, TypeExpr::Clock(_)))
                .map(|pt| pt.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = p.asserts.clone();
            let pname = p.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &pname, &clk);
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

    // ── Pipeline wait-stage helpers ─────────────────────────────────────────

    /// Check if a pipeline stage contains `wait until` or `do..until` in its seq block.
    fn stage_has_wait(stage: &StageDecl) -> bool {
        stage.body.iter().any(|item| {
            if let ModuleBodyItem::RegBlock(rb) = item {
                Self::stmts_contain_wait(&rb.stmts)
            } else {
                false
            }
        })
    }

    fn stmts_contain_wait(stmts: &[Stmt]) -> bool {
        stmts.iter().any(|s| match s {
            Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => true,
            Stmt::IfElse(ie) => Self::stmts_contain_wait(&ie.then_stmts) || Self::stmts_contain_wait(&ie.else_stmts),
            Stmt::For(f) => Self::stmts_contain_wait(&f.body),
            _ => false,
        })
    }

    /// Count FSM states needed for a wait-stage.
    /// State 0 = idle. Each `wait until` / `do..until` adds one wait state.
    /// Pre-wait assigns and trailing assigns are merged into adjacent states.
    fn count_wait_fsm_states(stage: &StageDecl) -> usize {
        let mut wait_count = 0;
        for item in &stage.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                for s in &rb.stmts {
                    match s {
                        Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => { wait_count += 1; }
                        _ => {}
                    }
                }
            }
        }
        wait_count + 1 // +1 for idle state 0
    }

    /// Emit the always_ff logic for a wait-stage: FSM transitions + register updates.
    ///
    /// State 0 is idle: checks upstream valid, fast-paths if wait condition already met.
    /// Wait states loop until their condition is satisfied, then advance.
    /// Trailing assigns execute when the last wait condition fires, returning to idle.
    fn emit_pipeline_wait_stage_ff(
        &mut self,
        stage: &StageDecl,
        prefix: &str,
        si: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        // Collect seq stmts from the stage's RegBlock
        let mut seq_stmts: &[Stmt] = &[];
        for item in &stage.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                seq_stmts = &rb.stmts;
                break;
            }
        }

        // Partition into groups: [pre-wait assigns, wait, post-wait assigns, wait, ...]
        // Each wait creates a wait-state. Pre-wait assigns execute on entry.
        // Trailing assigns execute when the last wait completes.
        struct WaitGroup<'a> {
            pre_assigns: Vec<&'a Stmt>,   // assigns before the wait
            cond: &'a Expr,               // wait condition
            hold_assigns: Vec<&'a Stmt>,  // do..until body (empty for wait until)
        }

        let mut groups: Vec<WaitGroup> = Vec::new();
        let mut cur_assigns: Vec<&Stmt> = Vec::new();

        for stmt in seq_stmts {
            match stmt {
                Stmt::WaitUntil(cond, _) => {
                    groups.push(WaitGroup {
                        pre_assigns: std::mem::take(&mut cur_assigns),
                        cond,
                        hold_assigns: Vec::new(),
                    });
                }
                Stmt::DoUntil { body, cond, .. } => {
                    groups.push(WaitGroup {
                        pre_assigns: std::mem::take(&mut cur_assigns),
                        cond,
                        hold_assigns: body.iter().collect(),
                    });
                }
                other => {
                    cur_assigns.push(other);
                }
            }
        }
        let trailing = std::mem::take(&mut cur_assigns);

        let upstream_valid = if si > 0 {
            format!("{}_valid_r", stage_names[si - 1].to_lowercase())
        } else {
            "1'b1".to_string()
        };

        // For each wait group, we generate:
        //   - State N (wait): loops checking condition; on true, runs trailing/next pre-assigns
        // State 0 is special: checks upstream valid, fast-paths.
        // Total states = 1 (idle) + number of wait groups
        let n_states = 1 + groups.len();
        let bits = if n_states <= 2 { 1 } else { ((n_states as f64).log2().ceil()) as usize };

        self.line(&format!("// Wait-stage FSM: {prefix}"));
        self.line(&format!("case ({prefix}_fsm_state)"));
        self.indent += 1;

        // State 0: idle — check upstream valid, optionally fast-path first wait
        self.line(&format!("{bits}'d0: begin"));
        self.indent += 1;
        if let Some(g) = groups.first() {
            let cond = self.emit_pipeline_expr_str(g.cond, stage_names, stage_regs, port_names);

            // Run pre-assigns (fire once on entry to the wait)
            // For state 0 these only fire when upstream has valid data
            self.line(&format!("if ({upstream_valid}) begin"));
            self.indent += 1;
            for a in &g.pre_assigns {
                self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
            }
            // Fast path: condition already met
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            if groups.len() == 1 {
                // Only one wait group: run trailing assigns and stay idle
                for a in &trailing {
                    self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
                }
                // Propagate valid
                self.line(&format!("{prefix}_valid_r <= {upstream_valid};"));
            } else {
                // Advance to next wait state
                self.line(&format!("{prefix}_fsm_state <= {bits}'d2;"));
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            // Slow path: enter wait state 1
            self.line(&format!("{prefix}_fsm_state <= {bits}'d1;"));
            for a in &g.hold_assigns {
                self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
            }
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");

        // States 1..N: wait states (one per wait group)
        for (gi, g) in groups.iter().enumerate() {
            let state_num = gi + 1;
            self.line(&format!("{bits}'d{state_num}: begin"));
            self.indent += 1;

            let cond = self.emit_pipeline_expr_str(g.cond, stage_names, stage_regs, port_names);

            // Emit hold assigns (for do..until, every cycle)
            for a in &g.hold_assigns {
                self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
            }

            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;

            let is_last = gi + 1 >= groups.len();
            if is_last {
                // Last wait: run trailing assigns, return to idle
                for a in &trailing {
                    self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
                }
                self.line(&format!("{prefix}_fsm_state <= '0;"));
                self.line(&format!("{prefix}_valid_r <= 1'b1;"));
            } else {
                // Not last: run next group's pre-assigns, advance to next wait state
                let next_g = &groups[gi + 1];
                for a in &next_g.pre_assigns {
                    self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
                }
                self.line(&format!("{prefix}_fsm_state <= {bits}'d{};", state_num + 1));
            }

            self.indent -= 1;
            self.line("end");

            self.indent -= 1;
            self.line("end");
        }

        // Default case
        self.line("default: begin");
        self.indent += 1;
        self.line(&format!("{prefix}_fsm_state <= '0;"));
        self.indent -= 1;
        self.line("end");

        self.indent -= 1;
        self.line("endcase");
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
            Stmt::For(f) => {
                let var = &f.var.name;
                match &f.range {
                    ForRange::Range(rs, re) => {
                        let start = self.emit_expr_str(rs);
                        let end = self.emit_expr_str(re);
                        self.line(&format!("for (int {var} = {start}; {var} <= {end}; {var}++) begin"));
                        self.indent += 1;
                        for s in &f.body { self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names); }
                        self.indent -= 1;
                        self.line("end");
                    }
                    ForRange::ValueList(vals) => {
                        for v in vals {
                            let val = self.emit_expr_str(v);
                            self.line(&format!("for (int {var} = {val}; {var} == {val}; {var}++) begin"));
                            self.indent += 1;
                            for s in &f.body { self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names); }
                            self.indent -= 1;
                            self.line("end");
                        }
                    }
                }
            }
            Stmt::Init(_) => unreachable!("Stmt::Init should not appear in pipeline reg stmt context"),
            Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => {
                // Pipeline wait-stages handled separately by pipeline codegen
                unreachable!("WaitUntil/DoUntil handled by pipeline stage codegen, not emit_pipeline_reg_stmt")
            }
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
                    if let ExprKind::Ident(name) = &a.target.kind {
                        if !targets.contains(name) {
                            targets.push(name.clone());
                        }
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
                CombStmt::For(f) => {
                    // ForLoop body is Vec<Stmt>; collect ident targets from assigns
                    for s in &f.body {
                        if let Stmt::Assign(a) = s {
                            if let ExprKind::Ident(name) = &a.target.kind {
                                if !targets.contains(name) { targets.push(name.clone()); }
                            }
                        }
                    }
                }
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
                CombStmt::Assign(a) if matches!(&a.target.kind, ExprKind::Ident(n) if n == target) => {
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
                let target = if let ExprKind::Ident(name) = &a.target.kind {
                    if port_names.contains(name) {
                        name.clone()
                    } else {
                        format!("{}_{}", current_prefix, name)
                    }
                } else {
                    self.emit_expr_str(&a.target)
                };
                self.line(&format!("{} = {};", target, val));
            }
            CombStmt::IfElse(ie) => {
                self.emit_pipeline_comb_if_else(ie, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, false);
            }
            CombStmt::MatchExpr(_) => {} // TODO if needed
            CombStmt::Log(l) => { self.emit_log_stmt(l); }
            CombStmt::For(f) => {
                self.emit_for_loop_sv(f, |s, stmt| s.emit_reg_stmt_as_comb(stmt));
            }
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
                if *op == BinOp::Implies {
                    return format!("(!{l} || {r})");
                }
                let op_str = match op {
                    BinOp::Add | BinOp::AddWrap => "+", BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*",
                    BinOp::Div => "/", BinOp::Mod => "%", BinOp::Eq => "==",
                    BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "&&",
                    BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::Shl => "<<", BinOp::Shr => ">>",
                    BinOp::Implies => unreachable!(),
                };
                if matches!(op, BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap) {
                    let lw = self.infer_sv_width_str(lhs);
                    let rw = self.infer_sv_width_str(rhs);
                    let w = if lw == rw { lw } else { format!("({lw} > {rw} ? {lw} : {rw})") };
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({l} {op_str} {r})")
                } else {
                    format!("({l} {op_str} {r})")
                }
            }
            ExprKind::Unary(op, operand) => {
                let o = self.emit_pipeline_stage_expr_str(operand, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match op {
                    UnaryOp::Not => format!("(!{o})"),
                    UnaryOp::BitNot => format!("(~{o})"),
                    UnaryOp::Neg => format!("(-{o})"),
                    UnaryOp::RedAnd => format!("(&{o})"),
                    UnaryOp::RedOr => format!("(|{o})"),
                    UnaryOp::RedXor => format!("(^{o})"),
                }
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match method.name.as_str() {
                    "trunc" | "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            format!("{wp}'({b})")
                        } else {
                            b
                        }
                    }
                    "sext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{{{{({w}-$bits({b})){{{b}[$bits({b})-1]}}}}, {b}}}")
                        } else {
                            b
                        }
                    }
                    "resize" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            if self.expr_is_signed(base) {
                                format!("{wp}'($signed({b}))")
                            } else {
                                format!("{wp}'($unsigned({b}))")
                            }
                        } else {
                            b
                        }
                    }
                    "reverse" => {
                        if let Some(chunk) = args.first() {
                            let c = self.emit_expr_str(chunk);
                            format!("{{<<{c}{{{b}}}}}")
                        } else {
                            b
                        }
                    }
                    "any" | "all" | "count" | "contains"
                    | "reduce_or" | "reduce_and" | "reduce_xor"
                    | "find_first" => {
                        self.emit_vec_method(&b, base, method, args)
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let i = self.emit_pipeline_stage_expr_str(idx, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("{b}[{i}]")
            }
            ExprKind::BitSlice(base, hi, lo) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                if let Some(width) = Self::try_indexed_part_select(hi, lo) {
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{l} +: {width}]")
                } else {
                    let h = self.emit_expr_str(hi);
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{h}:{l}]")
                }
            }
            ExprKind::PartSelect(base, start, width, up) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let s = self.emit_expr_str(start);
                let w = self.emit_expr_str(width);
                let op = if *up { "+:" } else { "-:" };
                format!("{b}[{s} {op} {w}]")
            }
            ExprKind::Concat(parts) => {
                let parts_str: Vec<String> = parts.iter()
                    .map(|p| self.emit_pipeline_stage_expr_str(p, current_prefix, current_stage_idx, stage_names, stage_regs, port_names))
                    .collect();
                format!("{{{}}}", parts_str.join(", "))
            }
            ExprKind::Cast(inner, ty) => {
                let e = self.emit_pipeline_stage_expr_str(inner, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match &**ty {
                    TypeExpr::SInt(_) => format!("$signed({e})"),
                    TypeExpr::UInt(w) => {
                        let ws = self.emit_expr_str(w);
                        format!("{ws}'($unsigned({e}))")
                    }
                    _ => {
                        let t = self.emit_type_str(ty);
                        format!("{t}'({e})")
                    }
                }
            }
            ExprKind::Signed(inner) => {
                let e = self.emit_pipeline_stage_expr_str(inner, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("$signed({e})")
            }
            ExprKind::Unsigned(inner) => {
                let e = self.emit_pipeline_stage_expr_str(inner, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("$unsigned({e})")
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                let c = self.emit_pipeline_stage_expr_str(cond, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let t = self.emit_pipeline_stage_expr_str(then_expr, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let e = self.emit_pipeline_stage_expr_str(else_expr, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("({c}) ? ({t}) : ({e})")
            }
            ExprKind::Bool(b) => if *b { "1'b1".to_string() } else { "1'b0".to_string() },
            ExprKind::Clog2(arg) => {
                let a = self.emit_pipeline_stage_expr_str(arg, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("$clog2({a})")
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
                if *op == BinOp::Implies {
                    return format!("(!{l} || {r})");
                }
                let op_str = match op {
                    BinOp::Add | BinOp::AddWrap => "+", BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*",
                    BinOp::Div => "/", BinOp::Mod => "%", BinOp::Eq => "==",
                    BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "&&",
                    BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::Shl => "<<", BinOp::Shr => ">>",
                    BinOp::Implies => unreachable!(),
                };
                if matches!(op, BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap) {
                    let lw = self.infer_sv_width_str(lhs);
                    let rw = self.infer_sv_width_str(rhs);
                    let w = if lw == rw { lw } else { format!("({lw} > {rw} ? {lw} : {rw})") };
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({l} {op_str} {r})")
                } else {
                    format!("({l} {op_str} {r})")
                }
            }
            ExprKind::Unary(op, operand) => {
                let o = self.emit_pipeline_expr_str(operand, stage_names, stage_regs, port_names);
                match op {
                    UnaryOp::Not => format!("(!{o})"),
                    UnaryOp::BitNot => format!("(~{o})"),
                    UnaryOp::Neg => format!("(-{o})"),
                    UnaryOp::RedAnd => format!("(&{o})"),
                    UnaryOp::RedOr => format!("(|{o})"),
                    UnaryOp::RedXor => format!("(^{o})"),
                }
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                match method.name.as_str() {
                    "trunc" | "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            format!("{wp}'({b})")
                        } else {
                            b
                        }
                    }
                    "sext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{{{{({w}-$bits({b})){{{b}[$bits({b})-1]}}}}, {b}}}")
                        } else {
                            b
                        }
                    }
                    "resize" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            if self.expr_is_signed(base) {
                                format!("{wp}'($signed({b}))")
                            } else {
                                format!("{wp}'($unsigned({b}))")
                            }
                        } else {
                            b
                        }
                    }
                    "reverse" => {
                        if let Some(chunk) = args.first() {
                            let c = self.emit_expr_str(chunk);
                            format!("{{<<{c}{{{b}}}}}")
                        } else {
                            b
                        }
                    }
                    "any" | "all" | "count" | "contains"
                    | "reduce_or" | "reduce_and" | "reduce_xor"
                    | "find_first" => {
                        self.emit_vec_method(&b, base, method, args)
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                let i = self.emit_pipeline_expr_str(idx, stage_names, stage_regs, port_names);
                format!("{b}[{i}]")
            }
            ExprKind::BitSlice(base, hi, lo) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                if let Some(width) = Self::try_indexed_part_select(hi, lo) {
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{l} +: {width}]")
                } else {
                    let h = self.emit_expr_str(hi);
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{h}:{l}]")
                }
            }
            ExprKind::PartSelect(base, start, width, up) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                let s = self.emit_expr_str(start);
                let w = self.emit_expr_str(width);
                let op = if *up { "+:" } else { "-:" };
                format!("{b}[{s} {op} {w}]")
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

        // Find the type parameter (any name) and compute its bit-width for DATA_WIDTH
        let type_param_name = f.params.iter()
            .find(|p| matches!(p.kind, crate::ast::ParamKind::Type(_)))
            .map(|p| p.name.name.clone());
        let data_width_str = f.params.iter()
            .find(|p| matches!(p.kind, crate::ast::ParamKind::Type(_)))
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => self.type_expr_data_width(ty),
                _ => None,
            })
            .unwrap_or_else(|| "8".to_string());

        // Check for OVERFLOW param (0 = block when full, 1 = overwrite oldest)
        let overflow_expr = f.params.iter()
            .find(|p| p.name.name == "OVERFLOW")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());
        let has_overflow_param = f.params.iter().any(|p| p.name.name == "OVERFLOW");

        // Collect port names to know what's declared
        let port_names: Vec<&str> = f.ports.iter().map(|p| p.name.name.as_str()).collect();

        let n = &f.name.name;

        // ── Module header ────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        self.line(&format!("parameter int  DEPTH      = {depth_expr},"));
        if has_overflow_param {
            self.line(&format!("parameter int  OVERFLOW   = {overflow_expr},"));
        }
        self.line(&format!("parameter int  DATA_WIDTH = {data_width_str}"));
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Emit declared ports
        for (i, p) in f.ports.iter().enumerate() {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            // Type param references → use DATA_WIDTH
            let ty_str = self.emit_fifo_port_type(&p.ty, &type_param_name);
            let comma = if i < f.ports.len() - 1 { "," } else { "" };
            self.line(&format!("{dir} {ty_str} {}{comma}", p.name.name));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        if is_async {
            self.emit_fifo_async_body(f, &port_names, has_overflow_param);
        } else if f.kind == FifoKind::Lifo {
            self.emit_fifo_lifo_body(f, &port_names);
        } else {
            self.emit_fifo_sync_body(f, &port_names, has_overflow_param);
        }

        // Auto-generated safety assertions for FIFO invariants
        {
            let clk_names: Vec<String> = f.ports.iter()
                .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone())
                .collect();
            let clk = clk_names.first().cloned().unwrap_or_else(|| "clk".to_string());
            let n = &f.name.name;

            self.line("");
            self.line("// synopsys translate_off");
            if is_async {
                // Async FIFO: assertions in write domain (wr_clk) and read domain (rd_clk)
                let wr_clk = clk_names.first().map(|s| s.as_str()).unwrap_or("wr_clk");
                let rd_clk = clk_names.get(1).map(|s| s.as_str()).unwrap_or("rd_clk");
                self.line(&format!(
                    "_auto_no_overflow: assert property (@(posedge {wr_clk}) !(push_valid && push_ready && full_r))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO OVERFLOW: {n}.push while full\");"
                ));
                self.line(&format!(
                    "_auto_no_underflow: assert property (@(posedge {rd_clk}) !(pop_valid && pop_ready && empty_r))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO UNDERFLOW: {n}.pop while empty\");"
                ));
            } else if f.kind == FifoKind::Lifo {
                // LIFO: sp-based full/empty
                self.line(&format!(
                    "_auto_no_overflow: assert property (@(posedge {clk}) !(push_valid && push_ready && full))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO OVERFLOW: {n}.push while full\");"
                ));
                self.line(&format!(
                    "_auto_no_underflow: assert property (@(posedge {clk}) !(pop_valid && pop_ready && empty))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO UNDERFLOW: {n}.pop while empty\");"
                ));
            } else {
                // Sync FIFO: pointer-based full/empty
                if has_overflow_param {
                    // Only assert overflow when OVERFLOW mode is disabled
                    self.line(&format!(
                        "_auto_no_overflow: assert property (@(posedge {clk}) OVERFLOW || !(push_valid && push_ready && full))"
                    ));
                } else {
                    self.line(&format!(
                        "_auto_no_overflow: assert property (@(posedge {clk}) !(push_valid && push_ready && full))"
                    ));
                }
                self.line(&format!(
                    "  else $fatal(1, \"FIFO OVERFLOW: {n}.push while full\");"
                ));
                self.line(&format!(
                    "_auto_no_underflow: assert property (@(posedge {clk}) !(pop_valid && pop_ready && empty))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO UNDERFLOW: {n}.pop while empty\");"
                ));
            }
            self.line("// synopsys translate_on");
        }

        if !f.asserts.is_empty() {
            let clk = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = f.asserts.clone();
            let fname = f.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &fname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Compute the total bit-width of a TypeExpr (for FIFO DATA_WIDTH parameter).
    fn type_expr_data_width(&self, ty: &TypeExpr) -> Option<String> {
        match ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => {
                Some(self.emit_expr_str(w))
            }
            TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => {
                Some("1".to_string())
            }
            TypeExpr::Vec(inner, size) => {
                let iw = self.type_expr_data_width(inner)?;
                let n = self.emit_expr_str(size);
                Some(format!("({iw}) * ({n})"))
            }
            TypeExpr::Named(ident) => {
                // Look up struct in symbol table to sum field widths
                if let Some((crate::resolve::Symbol::Struct(info), _)) = self.symbols.globals.get(&ident.name) {
                    let mut parts = Vec::new();
                    for (_, field_ty) in &info.fields {
                        parts.push(self.type_expr_data_width(field_ty)?);
                    }
                    if parts.len() == 1 {
                        Some(parts.into_iter().next().unwrap())
                    } else {
                        Some(parts.join(" + "))
                    }
                } else if let Some((crate::resolve::Symbol::Enum(info), _)) = self.symbols.globals.get(&ident.name) {
                    let n = info.variants.len();
                    let bits = if n <= 1 { 1 } else { (n as f64).log2().ceil() as u32 };
                    Some(bits.to_string())
                } else {
                    None
                }
            }
        }
    }

    fn emit_fifo_port_type(&self, ty: &TypeExpr, type_param_name: &Option<String>) -> String {
        if let Some(tpn) = type_param_name {
            if let TypeExpr::Named(ident) = ty {
                if ident.name == *tpn {
                    return "logic [DATA_WIDTH-1:0]".to_string();
                }
            }
        }
        self.emit_port_type_str(ty)
    }

    fn emit_fifo_sync_body(&mut self, f: &FifoDecl, port_names: &[&str], has_overflow_param: bool) {
        self.line("localparam int PTR_W = $clog2(DEPTH) + 1;");
        self.line("");
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");
        self.line("logic [PTR_W-1:0]     wr_ptr;");
        self.line("logic [PTR_W-1:0]     rd_ptr;");
        if !port_names.contains(&"full") {
            self.line("logic                 full;");
        }
        if !port_names.contains(&"empty") {
            self.line("logic                 empty;");
        }
        self.line("");
        self.line("// Full when MSBs differ and lower bits match");
        self.line("assign full        = (wr_ptr[PTR_W-1] != rd_ptr[PTR_W-1]) &&");
        self.line("                     (wr_ptr[PTR_W-2:0] == rd_ptr[PTR_W-2:0]);");
        self.line("assign empty       = (wr_ptr == rd_ptr);");
        if has_overflow_param {
            self.line("// OVERFLOW mode: push_ready always high; overwrite oldest when full");
            self.line("assign push_ready  = (OVERFLOW != 0) ? 1'b1 : !full;");
        } else {
            self.line("assign push_ready  = !full;");
        }
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
        if has_overflow_param {
            self.line("if (push_valid && push_ready) begin");
            self.indent += 1;
            self.line("mem[wr_ptr[PTR_W-2:0]] <= push_data;");
            self.line("wr_ptr <= wr_ptr + 1;");
            self.line("// In overflow mode, advance rd_ptr when writing to a full FIFO");
            self.line("if ((OVERFLOW != 0) && full && !(pop_ready)) rd_ptr <= rd_ptr + 1;");
            self.indent -= 1;
            self.line("end");
        } else {
            self.line("if (push_valid && push_ready) begin");
            self.indent += 1;
            self.line("mem[wr_ptr[PTR_W-2:0]] <= push_data;");
            self.line("wr_ptr <= wr_ptr + 1;");
            self.indent -= 1;
            self.line("end");
        }
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

    fn emit_fifo_lifo_body(&mut self, f: &FifoDecl, port_names: &[&str]) {
        self.line("localparam int PTR_W = $clog2(DEPTH + 1);");
        self.line("");
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");
        self.line("logic [PTR_W-1:0]     sp;");
        if !port_names.contains(&"full") {
            self.line("logic                 full;");
        }
        if !port_names.contains(&"empty") {
            self.line("logic                 empty;");
        }
        self.line("");
        self.line("assign full        = (sp == DEPTH[PTR_W-1:0]);");
        self.line("assign empty       = (sp == '0);");
        self.line("assign push_ready  = !full;");
        self.line("assign pop_valid   = !empty;");
        self.line("assign pop_data    = mem[sp - 1];");
        self.line("");

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
        self.line("sp <= '0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("if (push_valid && push_ready && pop_valid && pop_ready) begin");
        self.indent += 1;
        self.line("// Simultaneous push+pop: replace top of stack");
        self.line("mem[sp - 1] <= push_data;");
        self.indent -= 1;
        self.line("end else if (push_valid && push_ready) begin");
        self.indent += 1;
        self.line("mem[sp] <= push_data;");
        self.line("sp <= sp + 1;");
        self.indent -= 1;
        self.line("end else if (pop_valid && pop_ready) begin");
        self.indent += 1;
        self.line("sp <= sp - 1;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_fifo_async_body(&mut self, f: &FifoDecl, port_names: &[&str], has_overflow_param: bool) {
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
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");
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
        if has_overflow_param {
            self.line("assign push_ready = (OVERFLOW != 0) ? 1'b1 : !full_r;");
        } else {
            self.line("assign push_ready = !full_r;");
        }
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

    /// Return operator precedence for SV emission (higher = tighter binding).
    ///
    /// ARCH and SV disagree on the relative precedence of comparison operators
    /// (`==`, `!=`, `<`, `>`, `<=`, `>=`) vs bitwise operators (`&`, `^`, `|`):
    ///   - SV (IEEE 1800-2017):  `==`/relational bind TIGHTER than `&`/`^`/`|`
    ///   - ARCH:                 `&`/`^`/`|` bind TIGHTER than `==`/relational
    ///
    /// To guarantee correct SV regardless of which precedence the reader assumes,
    /// we collapse comparison and bitwise ops into a single precedence tier.
    /// This forces parentheses whenever they are mixed (e.g. `(a == b) & (c == d)`),
    /// which is always safe and improves readability.
    fn sv_binop_prec(op: &BinOp) -> u8 {
        match op {
            BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::MulWrap => 12,
            BinOp::Add | BinOp::Sub | BinOp::AddWrap | BinOp::SubWrap => 11,
            BinOp::Shl | BinOp::Shr => 10,
            // Collapsed tier: comparison and bitwise ops share the same level
            // so any mixing produces parentheses.
            BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => 7,
            BinOp::Eq | BinOp::Neq => 7,
            BinOp::BitAnd => 7,
            BinOp::BitXor => 7,
            BinOp::BitOr => 7,
            BinOp::And => 4,
            BinOp::Or => 3,
            BinOp::Implies => 2,
        }
    }

    /// Precedence of the outermost operator in `expr`, or u8::MAX for atoms.
    fn expr_prec(expr: &Expr) -> u8 {
        match &expr.kind {
            ExprKind::Binary(op, _, _) => Self::sv_binop_prec(op),
            ExprKind::Unary(..) => 14,
            ExprKind::Ternary(..) => 2,
            _ => u8::MAX, // atoms — never need wrapping
        }
    }

    fn emit_expr_str(&self, expr: &Expr) -> String {
        self.emit_expr_prec(expr, 0)
    }

    /// Best-effort struct name for an expression. Walks a small set of
    /// expression shapes that typically produce a struct value in ARCH
    /// today (method calls returning structs, function calls, struct
    /// literals, struct-typed ports/regs/wires/lets). Returns None if
    /// the type isn't determinable at codegen time — caller falls back
    /// to emitting a `logic` wire.
    fn infer_expr_struct_name(&self, e: &Expr) -> Option<String> {
        // Struct literal: `'{field: value, ...}` carries the struct name.
        if let ExprKind::StructLiteral(name, _) = &e.kind {
            return Some(name.name.clone());
        }
        // Plain identifier: look up in the current module's symbol scope.
        if let ExprKind::Ident(n) = &e.kind {
            let scope = self.symbols.module_scopes.get(&self.current_construct)?;
            let (sym, _) = scope.get(n.as_str())?;
            let te_opt: Option<&TypeExpr> = match sym {
                Symbol::Port(p) => Some(&p.ty),
                Symbol::Reg(r)  => Some(&r.ty),
                _ => None,
            };
            if let Some(TypeExpr::Named(struct_name)) = te_opt {
                return Some(struct_name.name.clone());
            }
            // Let bindings: scan the module body for the declared type.
            for item in &self.source.items {
                if let Item::Module(m) = item {
                    if m.name.name == self.current_construct {
                        for bi in &m.body {
                            if let ModuleBodyItem::LetBinding(lb) = bi {
                                if lb.name.name == *n {
                                    if let Some(TypeExpr::Named(sn)) = &lb.ty {
                                        return Some(sn.name.clone());
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

    fn struct_field_type(&self, struct_name: &str, field_name: &str) -> Option<TypeExpr> {
        for item in &self.source.items {
            if let Item::Struct(s) = item {
                if s.name.name == struct_name {
                    for f in &s.fields {
                        if f.name.name == field_name {
                            return Some(f.ty.clone());
                        }
                    }
                }
            }
            if let Item::Package(pkg) = item {
                for s in &pkg.structs {
                    if s.name.name == struct_name {
                        for f in &s.fields {
                            if f.name.name == field_name {
                                return Some(f.ty.clone());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Lower a Vec method call (any/all/count/contains/reduce_*) to a
    /// parallel-compare + reduction expression. Fully unrolled at codegen
    /// time because N is known.
    ///
    /// Predicate identifier substitution for `item` / `index` is applied
    /// via `self.ident_subst`, which is a reentrant context we push before
    /// emitting each iteration's expression and pop after.
    ///
    /// Safety: this is `&self`, but we need to temporarily mutate
    /// `ident_subst`. We cast away immutability in a narrowly-scoped
    /// block that restores the previous state before returning. The
    /// alternative (threading a mutable binding map through every
    /// `emit_expr_str` caller) would touch ~30 sites; this is localized.
    #[allow(clippy::ptr_arg)]
    fn emit_vec_method(
        &self,
        recv_b: &str,
        recv: &Expr,
        method: &Ident,
        args: &[Expr],
    ) -> String {
        // Resolve N. The receiver is an Ident in v1; more complex
        // expressions are not lowered (falls through to placeholder).
        let n = match &recv.kind {
            ExprKind::Ident(n) => self.vec_sizes.get(n).copied(),
            _ => None,
        };
        let Some(n) = n else {
            // Size unknown → bail to the fallback shape; SV tools will
            // reject it, telling the user we couldn't unroll.
            return format!("{recv_b}.{}()", method.name);
        };
        let n_usize = n as usize;
        let idx_w = std::cmp::max(1, (n as f64).log2().ceil() as u32);

        // Helper: emit an expression with `item` bound to recv[i] and
        // `index` bound to a sized literal. `ident_subst` is a field of
        // Codegen; we use interior-mutability-via-unsafe here because
        // emit_expr_str is `&self`. The Codegen type is `!Sync` and
        // emission is single-threaded, so this is safe.
        let emit_at = |i: u32| -> String {
            let this = self as *const Codegen as *mut Codegen;
            // SAFETY: single-threaded emission; no aliasing.
            unsafe {
                (*this).ident_subst.insert("item".to_string(), format!("{recv_b}[{i}]"));
                (*this).ident_subst.insert("index".to_string(), format!("{idx_w}'d{i}"));
            }
            let result = if let Some(pred) = args.first() {
                self.emit_expr_str(pred)
            } else {
                // contains / reduce_*: see caller below; we won't be called
                // without args from those paths.
                String::new()
            };
            unsafe {
                (*this).ident_subst.remove("item");
                (*this).ident_subst.remove("index");
            }
            result
        };

        match method.name.as_str() {
            "any" => {
                if n_usize == 0 { return "1'b0".to_string(); }
                (0..n).map(emit_at).collect::<Vec<_>>().join(" || ")
            }
            "all" => {
                if n_usize == 0 { return "1'b1".to_string(); }
                (0..n).map(emit_at).collect::<Vec<_>>().join(" && ")
            }
            "count" => {
                if n_usize == 0 { return "0".to_string(); }
                let w = std::cmp::max(1, ((n + 1) as f64).log2().ceil() as u32);
                // Sum of bool conversions. SV auto-widens `+` per 1800-2012 §11.6.
                let terms: Vec<String> = (0..n)
                    .map(|i| format!("{w}'({} ? 1 : 0)", emit_at(i)))
                    .collect();
                format!("({})", terms.join(" + "))
            }
            "contains" => {
                // `contains(x)` is `any(item == x)` — but the user supplies x,
                // not a predicate. Emit n equality comparisons against the
                // argument, OR'd.
                let Some(x_expr) = args.first() else {
                    return "1'b0".to_string();
                };
                let x = self.emit_expr_str(x_expr);
                if n_usize == 0 { return "1'b0".to_string(); }
                (0..n).map(|i| format!("({recv_b}[{i}] == {x})"))
                      .collect::<Vec<_>>()
                      .join(" || ")
            }
            "reduce_or" => {
                if n_usize == 0 { return "0".to_string(); }
                (0..n).map(|i| format!("{recv_b}[{i}]"))
                      .collect::<Vec<_>>()
                      .join(" | ")
            }
            "reduce_and" => {
                if n_usize == 0 { return "0".to_string(); }
                (0..n).map(|i| format!("{recv_b}[{i}]"))
                      .collect::<Vec<_>>()
                      .join(" & ")
            }
            "reduce_xor" => {
                if n_usize == 0 { return "0".to_string(); }
                (0..n).map(|i| format!("{recv_b}[{i}]"))
                      .collect::<Vec<_>>()
                      .join(" ^ ")
            }
            "find_first" => {
                // Record the index width so a matching typedef is emitted
                // at the top of the generated SV file.
                self.find_first_widths.borrow_mut().insert(idx_w);
                if n_usize == 0 {
                    return format!("'{{found: 1'b0, index: {idx_w}'d0}}");
                }
                // Per-iteration hit expression: <pred with item=recv[i], index=i'd>.
                let hits: Vec<String> = (0..n).map(emit_at).collect();
                // found: OR of all hits.
                let found = hits.join(" || ");
                // index: priority-encoded first hit via nested ternary,
                // lowest-index-wins. Falls through to 0 when no hit.
                let mut index = format!("{idx_w}'d0");
                for i in (0..n).rev() {
                    let hit = &hits[i as usize];
                    index = format!("({hit}) ? {idx_w}'d{i} : {index}");
                }
                format!("'{{found: ({found}), index: ({index})}}")
            }
            _ => format!("{recv_b}.{}()", method.name),
        }
    }

    /// Evaluate a compile-time constant expression (Vec size, etc.) to a u32.
    /// Handles literals, const-param references, and simple binary ops.
    /// Returns None if the expression can't be reduced — caller then treats
    /// the receiver as size-unknown and skips Vec method lowering.
    fn eval_const_u32(&self, e: &Expr, params: &[ParamDecl]) -> Option<u32> {
        match &e.kind {
            ExprKind::Literal(LitKind::Dec(v)) => Some(*v as u32),
            ExprKind::Literal(LitKind::Hex(v))
            | ExprKind::Literal(LitKind::Bin(v))
            | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v as u32),
            ExprKind::Ident(n) => {
                let p = params.iter().find(|p| p.name.name == *n)?;
                match &p.kind {
                    ParamKind::Const | ParamKind::WidthConst(..) => {}
                    _ => return None,
                }
                let d = p.default.as_ref()?;
                self.eval_const_u32(d, params)
            }
            ExprKind::Binary(op, l, r) => {
                let lv = self.eval_const_u32(l, params)?;
                let rv = self.eval_const_u32(r, params)?;
                Some(match op {
                    BinOp::Add => lv + rv,
                    BinOp::Sub => lv.saturating_sub(rv),
                    BinOp::Mul => lv * rv,
                    BinOp::Div if rv != 0 => lv / rv,
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    /// Infer the SV bit-width of an expression as a string constant expression.
    /// Used to emit the width cast for wrapping arithmetic operators (+%, -%, *%).
    fn infer_sv_width_str(&self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if let Some(scope) = self.symbols.module_scopes.get(&self.current_construct) {
                    if let Some((sym, _)) = scope.get(name.as_str()) {
                        let te_opt: Option<&TypeExpr> = match sym {
                            Symbol::Port(p) => Some(&p.ty),
                            Symbol::Reg(r) => Some(&r.ty),
                            _ => None,
                        };
                        if let Some(te) = te_opt {
                            match te {
                                TypeExpr::UInt(w) | TypeExpr::SInt(w) => return self.emit_expr_str(w),
                                TypeExpr::Bool | TypeExpr::Bit => return "1".to_string(),
                                _ => {}
                            }
                        }
                        // For Let bindings, look up in AST
                        if matches!(sym, Symbol::Let(_)) {
                            for item in &self.source.items {
                                if let Item::Module(m) = item {
                                    if m.name.name == self.current_construct {
                                        for bi in &m.body {
                                            if let ModuleBodyItem::LetBinding(lb) = bi {
                                                if lb.name.name == *name {
                                                    if let Some(ty) = &lb.ty {
                                                        match ty {
                                                            TypeExpr::UInt(w) | TypeExpr::SInt(w) => return self.emit_expr_str(w),
                                                            TypeExpr::Bool | TypeExpr::Bit => return "1".to_string(),
                                                            _ => {}
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
                format!("$bits({})", self.emit_expr_str(expr))
            }
            // Unsized literals: compute minimum bit width from value (never 0 bits)
            ExprKind::Literal(LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v)) => {
                let bits = if *v == 0 { 1 } else { (64 - v.leading_zeros()) as u32 };
                bits.to_string()
            }
            ExprKind::Literal(LitKind::Sized(w, _)) => w.to_string(),
            ExprKind::MethodCall(_, method, args)
                if matches!(method.name.as_str(), "trunc" | "zext" | "sext" | "resize") =>
            {
                args.first()
                    .map(|w| self.emit_expr_str(w))
                    .unwrap_or_else(|| format!("$bits({})", self.emit_expr_str(expr)))
            }
            ExprKind::Cast(_, ty) => self
                .type_expr_data_width(ty)
                .unwrap_or_else(|| format!("$bits({})", self.emit_expr_str(expr))),
            // Vec element access: width comes from the inner element type
            ExprKind::Index(base, _) => {
                if let ExprKind::Ident(name) = &base.kind {
                    // Search current scope, then fallback to thread submodule scope
                    // (thread-driven regs are moved to _ModuleName_threads after lowering)
                    let fallback = format!("_{}_threads", self.current_construct);
                    let scopes = [self.current_construct.as_str(), fallback.as_str()];
                    'outer: for scope_key in &scopes {
                        if let Some(scope) = self.symbols.module_scopes.get(*scope_key) {
                            if let Some((sym, _)) = scope.get(name.as_str()) {
                                let te_opt: Option<&TypeExpr> = match sym {
                                    Symbol::Port(p) => Some(&p.ty),
                                    Symbol::Reg(r) => Some(&r.ty),
                                    _ => None,
                                };
                                if let Some(TypeExpr::Vec(inner, _)) = te_opt {
                                    match inner.as_ref() {
                                        TypeExpr::UInt(w) | TypeExpr::SInt(w) => return self.emit_expr_str(w),
                                        TypeExpr::Bool | TypeExpr::Bit => return "1".to_string(),
                                        _ => {}
                                    }
                                }
                                break 'outer;
                            }
                        }
                    }
                }
                format!("$bits({})", self.emit_expr_str(expr))
            }
            // Chained wrapping ops: result width = max(lhs width, rhs width)
            ExprKind::Binary(BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap, lhs, rhs) => {
                let lw = self.infer_sv_width_str(lhs);
                let rw = self.infer_sv_width_str(rhs);
                if lw == rw { lw } else { format!("({lw} > {rw} ? {lw} : {rw})") }
            }
            _ => format!("$bits({})", self.emit_expr_str(expr)),
        }
    }

    /// Wrap a width expression in parens if it contains operators,
    /// so that `W'(expr)` SV cast syntax parses correctly even when W is e.g. `DATA_WIDTH + 1`.
    fn paren_width(w: &str) -> String {
        if w.contains('+') || w.contains('-') || w.contains('*') || w.contains('/') {
            format!("({w})")
        } else {
            w.to_string()
        }
    }

    /// Emit an expression, wrapping in parens only when its precedence is
    /// below `parent_prec` (i.e. the context requires tighter binding).
    fn emit_expr_prec(&self, expr: &Expr, parent_prec: u8) -> String {
        let result = self.emit_expr_inner(expr);
        let my_prec = Self::expr_prec(expr);
        if my_prec < parent_prec {
            format!("({result})")
        } else {
            result
        }
    }

    /// Core expression emitter — never adds outer parens itself.
    fn emit_expr_inner(&self, expr: &Expr) -> String {
        match &expr.kind {
            // Latency annotation is purely documentary on emission.
            // On RHS, `q@0` reads as the current value of `q` (the pipe's
            // final output). On LHS inside an assignment, the enclosing
            // assignment emitter strips the annotation before routing the
            // value to the appropriate stage 0 of the pipe chain.
            ExprKind::LatencyAt(inner, _) => self.emit_expr_inner(inner),
            // SynthIdent: compiler-synthesized name pointing at codegen-
            // emitted SV wires (credit_channel dispatch targets). Emits as
            // a plain identifier — the declaration + driver live elsewhere
            // in the emitted SV.
            ExprKind::SynthIdent(name, _) => name.clone(),
            ExprKind::Literal(lit) => match lit {
                LitKind::Dec(v) => format!("{v}"),
                LitKind::Hex(v) => format!("'h{v:X}"),
                LitKind::Bin(v) => format!("'b{v:b}"),
                LitKind::Sized(w, v) => format!("{w}'d{v}"),
            },
            ExprKind::Bool(true) => "1'b1".to_string(),
            ExprKind::Bool(false) => "1'b0".to_string(),
            ExprKind::Ident(name) => {
                // Context-sensitive substitution: used by Vec method predicate
                // lowering to rebind `item` → `recv[i]`, `index` → `W'd<i>`.
                if let Some(sub) = self.ident_subst.get(name) {
                    return sub.clone();
                }
                name.clone()
            }
            ExprKind::Binary(op, lhs, rhs) => {
                // `implies` lowers to (!lhs || rhs)
                if *op == BinOp::Implies {
                    let l = self.emit_expr_prec(lhs, 14); // unary prec for !
                    let r = self.emit_expr_prec(rhs, 4);  // || prec
                    return format!("!{l} || {r}");
                }
                let prec = Self::sv_binop_prec(op);
                // LHS: same-prec left-assoc chain of the SAME associative op → no wrap;
                // otherwise wrap if same-or-lower precedence.
                let lhs_prec = if matches!(&lhs.kind, ExprKind::Binary(lop, _, _) if lop == op
                    && matches!(op, BinOp::Add | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::And | BinOp::Or))
                {
                    prec // same assoc op — don't wrap
                } else {
                    prec + 1 // different op at same level — wrap
                };
                let l = self.emit_expr_prec(lhs, lhs_prec);
                // RHS: wrap if same-or-lower precedence to respect left-associativity,
                // EXCEPT for the same commutative/associative op (chain without parens).
                let rhs_prec = if matches!(&rhs.kind, ExprKind::Binary(rop, _, _) if rop == op
                    && matches!(op, BinOp::Add | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::And | BinOp::Or))
                {
                    prec // same assoc op — don't wrap
                } else {
                    prec + 1 // different op at same level — wrap
                };
                let r = self.emit_expr_prec(rhs, rhs_prec);
                // Use arithmetic shift (>>>) when LHS is cast to SInt
                let shr_str = if matches!(op, BinOp::Shr) && self.expr_is_signed(lhs) {
                    ">>>"
                } else {
                    ">>"
                };
                let op_str = match op {
                    BinOp::Add | BinOp::AddWrap => "+",
                    BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*",
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
                    BinOp::Shr => shr_str,
                    BinOp::Implies => unreachable!("implies handled above"),
                };
                if matches!(op, BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap) {
                    let lw = self.infer_sv_width_str(lhs);
                    let rw = self.infer_sv_width_str(rhs);
                    let w = if lw == rw { lw } else { format!("({lw} > {rw} ? {lw} : {rw})") };
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({l} {op_str} {r})")
                } else {
                    format!("{l} {op_str} {r}")
                }
            }
            ExprKind::Unary(op, operand) => {
                // Unary has prec 14 — wrap child only if it's a binary/ternary
                let o = self.emit_expr_prec(operand, 14);
                match op {
                    UnaryOp::Not => format!("!{o}"),
                    UnaryOp::BitNot => format!("~{o}"),
                    UnaryOp::Neg => format!("-{o}"),
                    UnaryOp::RedAnd => format!("&{o}"),
                    UnaryOp::RedOr => format!("|{o}"),
                    UnaryOp::RedXor => format!("^{o}"),
                }
            }
            ExprKind::FieldAccess(base, field) => {
                // rst.asserted — polarity-abstracted reset active check
                if field.name == "asserted" {
                    if let ExprKind::Ident(base_name) = &base.kind {
                        if let Some((_, level)) = self.reset_ports.get(base_name) {
                            return if *level == ResetLevel::Low {
                                format!("(!{base_name})")
                            } else {
                                base_name.clone()
                            };
                        }
                    }
                }
                // Bus port / bus wire: axi.aw_valid → axi_aw_valid (flat).
                // Bus wires flatten to individual SV signals, same naming.
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.bus_ports.contains_key(base_name)
                        || self.bus_wires.contains_key(base_name)
                    {
                        return format!("{}_{}", base_name, field.name);
                    }
                }
                // Indexed bus port: m_axi[0].valid → m_axi_0_valid
                if let ExprKind::Index(arr, idx) = &base.kind {
                    if let (ExprKind::Ident(arr_name), ExprKind::Literal(LitKind::Dec(i))) = (&arr.kind, &idx.kind) {
                        let expanded = format!("{}_{}", arr_name, i);
                        if self.bus_ports.contains_key(&expanded) {
                            return format!("{}_{}_{}", arr_name, i, field.name);
                        }
                    }
                }
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
                            let wp = Self::paren_width(&w);
                            format!("{wp}'({b})")
                        } else {
                            b
                        }
                    }
                    "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            // $unsigned prevents context-dependent width expansion before the cast
                            let wp = Self::paren_width(&w);
                            format!("{wp}'($unsigned({b}))")
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
                    "resize" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            // Direction-agnostic resize: pads or truncates, preserving signedness.
                            // e.g. x.resize<16>() on UInt<8> → 16'($unsigned(x))
                            // e.g. x.resize<4>()  on SInt<8> → 4'($signed(x))
                            let wp = Self::paren_width(&w);
                            if self.expr_is_signed(base) {
                                format!("{wp}'($signed({b}))")
                            } else {
                                format!("{wp}'($unsigned({b}))")
                            }
                        } else {
                            b
                        }
                    }
                    // as_clock removed — use `as Clock<Domain>` cast syntax // identity — 1-bit logic used as clock
                    "reverse" => {
                        if let Some(chunk) = args.first() {
                            let c = self.emit_expr_str(chunk);
                            format!("{{<<{c}{{{b}}}}}")
                        } else {
                            b
                        }
                    }
                    "any" | "all" | "count" | "contains"
                    | "reduce_or" | "reduce_and" | "reduce_xor"
                    | "find_first" => {
                        self.emit_vec_method(&b, base, method, args)
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Cast(expr, ty) => {
                let e = self.emit_expr_str(expr);
                match &**ty {
                    TypeExpr::SInt(_) => {
                        format!("$signed({e})")
                    }
                    TypeExpr::UInt(w) => {
                        let ws = self.emit_expr_str(w);
                        format!("{ws}'($unsigned({e}))")
                    }
                    _ => {
                        let t = self.emit_type_str(ty);
                        format!("{t}'({e})")
                    }
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_expr_str(base);
                let i = self.emit_expr_str(idx);
                format!("{b}[{i}]")
            }
            ExprKind::BitSlice(base, hi, lo) => {
                let b = self.emit_expr_str(base);
                // Parenthesize complex base expressions to avoid precedence issues.
                // SynthIdent is a compiler-renamed bare identifier with the same
                // semantics as Ident — no parens needed (Verilator rejects
                // `(__name)[hi:lo]` as a syntax error).
                let b = if matches!(base.kind, ExprKind::Ident(_) | ExprKind::SynthIdent(_, _)
                    | ExprKind::Literal(_)
                    | ExprKind::Index(_, _) | ExprKind::FieldAccess(_, _)) { b }
                    else { format!("({})", b) };
                // Try to emit indexed part-select: base[lo +: width]
                if let Some(width) = Self::try_indexed_part_select(hi, lo) {
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{l} +: {width}]")
                } else {
                    let h = self.emit_expr_str(hi);
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{h}:{l}]")
                }
            }
            ExprKind::PartSelect(base, start, width, up) => {
                let b = self.emit_expr_str(base);
                let s = self.emit_expr_str(start);
                let w = self.emit_expr_str(width);
                let op = if *up { "+:" } else { "-:" };
                format!("{b}[{s} {op} {w}]")
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
            ExprKind::Repeat(count, value) => {
                let c = self.emit_expr_str(count);
                let v = self.emit_expr_str(value);
                format!("{{{c}{{{v}}}}}")
            }
            ExprKind::Clog2(arg) => {
                let a = self.emit_expr_str(arg);
                format!("$clog2({a})")
            }
            ExprKind::Onehot(index) => {
                let idx = self.emit_expr_str(index);
                format!("(1 << {idx})")
            }
            ExprKind::Signed(inner) => {
                let e = self.emit_expr_str(inner);
                format!("$signed({e})")
            }
            ExprKind::Unsigned(inner) => {
                let e = self.emit_expr_str(inner);
                format!("$unsigned({e})")
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
                // Inside ?: operands, any precedence is fine (delimited by ? and :)
                let c = self.emit_expr_prec(cond, 3); // wrap only if lower than ternary
                let t = self.emit_expr_str(then_expr);
                let e = self.emit_expr_str(else_expr);
                format!("{c} ? {t} : {e}")
            }
            ExprKind::Inside(scrutinee, members) => {
                let s = self.emit_expr_str(scrutinee);
                let member_strs: Vec<String> = members.iter().map(|m| match m {
                    InsideMember::Single(e) => self.emit_expr_str(e),
                    InsideMember::Range(lo, hi) => {
                        let l = self.emit_expr_str(lo);
                        let h = self.emit_expr_str(hi);
                        format!("[{l}:{h}]")
                    }
                }).collect();
                format!("{s} inside {{{}}}", member_strs.join(", "))
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

    /// Convert a width expression to a Verilog range string `[N:0]`.
    /// For literal widths, folds the arithmetic: `Dec(8)` → `"7:0"`.
    /// For expressions (params, binaries), keeps the expression: `"N-1:0"`.
    fn emit_width_range(&self, w: &Expr) -> String {
        match &w.kind {
            ExprKind::Literal(LitKind::Dec(n)) => {
                format!("{}:0", n.saturating_sub(1))
            }
            _ => {
                let ws = self.emit_expr_str(w);
                format!("{ws}-1:0")
            }
        }
    }

    /// Fold a width string (output of emit_expr_str) to a range.
    /// If `s` parses as a decimal integer, emits `"N-1:0"` pre-computed.
    /// Otherwise keeps `"s-1:0"`.
    fn fold_width_str(s: &str) -> String {
        if let Ok(n) = s.parse::<u64>() {
            format!("{}:0", n.saturating_sub(1))
        } else {
            format!("{s}-1:0")
        }
    }

    fn emit_type_str(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::UInt(w) => {
                let range = self.emit_width_range(w);
                format!("logic [{range}]")
            }
            TypeExpr::SInt(w) => {
                let range = self.emit_width_range(w);
                format!("logic signed [{range}]")
            }
            TypeExpr::Bool => "logic".to_string(),
            TypeExpr::Bit => "logic".to_string(),
            TypeExpr::Clock(_) => "logic".to_string(),
            TypeExpr::Reset(_, _) => "logic".to_string(),
            TypeExpr::Vec(_, _) => {
                // Packed multi-dimensional: all dims are in the type string, no suffix.
                let (type_str, _suffix) = self.emit_type_and_array_suffix(ty);
                type_str
            }
            TypeExpr::Named(ident) => ident.name.clone(),
        }
    }

    fn emit_port_type_str(&self, ty: &TypeExpr) -> String {
        // Port types use the same emission as internal types.
        self.emit_type_str(ty)
    }

    /// Substitute bus parameter names in a TypeExpr with actual value expressions.
    fn subst_type_expr(ty: &TypeExpr, params: &std::collections::HashMap<String, &Expr>) -> TypeExpr {
        match ty {
            TypeExpr::UInt(w) => TypeExpr::UInt(Box::new(Self::subst_expr(w, params))),
            TypeExpr::SInt(w) => TypeExpr::SInt(Box::new(Self::subst_expr(w, params))),
            TypeExpr::Vec(inner, len) => TypeExpr::Vec(
                Box::new(Self::subst_type_expr(inner, params)),
                Box::new(Self::subst_expr(len, params)),
            ),
            other => other.clone(),
        }
    }

    fn subst_expr(expr: &Expr, params: &std::collections::HashMap<String, &Expr>) -> Expr {
        let kind = match &expr.kind {
            ExprKind::Ident(name) => {
                if let Some(replacement) = params.get(name) {
                    return (*replacement).clone();
                }
                ExprKind::Ident(name.clone())
            }
            // Recurse into expression trees so arithmetic width expressions
            // (e.g. `UInt<DATA_W / 8>`, `UInt<DATA_W * 2>`) get the param
            // substituted in every operand. Without this, the ident shows
            // up verbatim in the emitted SV and downstream tools fail.
            ExprKind::Binary(op, l, r) => ExprKind::Binary(
                *op,
                Box::new(Self::subst_expr(l, params)),
                Box::new(Self::subst_expr(r, params)),
            ),
            ExprKind::Unary(op, e) => ExprKind::Unary(
                *op,
                Box::new(Self::subst_expr(e, params)),
            ),
            ExprKind::Ternary(c, t, e) => ExprKind::Ternary(
                Box::new(Self::subst_expr(c, params)),
                Box::new(Self::subst_expr(t, params)),
                Box::new(Self::subst_expr(e, params)),
            ),
            ExprKind::Clog2(e) => ExprKind::Clog2(Box::new(Self::subst_expr(e, params))),
            ExprKind::Index(b, i) => ExprKind::Index(
                Box::new(Self::subst_expr(b, params)),
                Box::new(Self::subst_expr(i, params)),
            ),
            _ => return expr.clone(),
        };
        Expr {
            kind,
            span: expr.span,
            parenthesized: expr.parenthesized,
        }
    }

    fn emit_logic_type_str(&self, ty: &TypeExpr) -> String {
        self.emit_type_str(ty)
    }

    /// For Vec types (including nested), returns (packed_type_str, "").
    /// The array dimensions are folded into the type as SV packed multi-dimensional
    /// ranges, e.g. `Vec<UInt<16>, 4>` → `("logic [3:0][15:0]", "")`.
    /// Packed arrays are portable across Verilator, Yosys, and iverilog; unpacked
    /// array dimensions after the signal name are rejected by Yosys during synthesis.
    /// For non-Vec types, returns (type_str, "").
    fn emit_type_and_array_suffix(&self, ty: &TypeExpr) -> (String, String) {
        let mut dims = Vec::new();
        let mut cur = ty;
        while let TypeExpr::Vec(inner, size) = cur {
            let range = self.emit_width_range(size);
            dims.push(format!("[{range}]"));
            cur = inner;
        }
        if dims.is_empty() {
            return (self.emit_type_str(ty), String::new());
        }
        // Build packed multi-dim type: "logic [outerDim][innerDim][baseRange]"
        // emit_type_str(cur) returns e.g. "logic [15:0]" for UInt<16>.
        // We insert the packed dims immediately after the "logic" keyword.
        let inner_type = self.emit_type_str(cur);
        let packed_dims: String = dims.join("");
        let type_str = if let Some(rest) = inner_type.strip_prefix("logic") {
            // rest is e.g. " [15:0]", " signed [15:0]", or "" for Bool.
            // For signed inner types hoist "signed" before the packed dims so the
            // result is valid SV: "logic signed [M-1:0][N-1:0]" not the illegal
            // "logic [M-1:0] signed [N-1:0]".
            if let Some(after_signed) = rest.strip_prefix(" signed") {
                format!("logic signed {packed_dims}{after_signed}")
            } else {
                format!("logic {packed_dims}{rest}")
            }
        } else {
            format!("{inner_type} {packed_dims}")
        };
        (type_str, String::new())
    }

    // ── Synchronizer ─────────────────────────────────────────────────────────

    fn emit_clkgate(&mut self, c: &crate::ast::ClkGateDecl) {
        let n = &c.name.name;

        // Find port names
        let clk_in = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::In)
            .map(|p| p.name.name.as_str()).unwrap_or("clk_in");
        let clk_out = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::Out)
            .map(|p| p.name.name.as_str()).unwrap_or("clk_out");
        let enable = c.ports.iter().find(|p| p.name.name == "enable")
            .map(|p| p.name.name.as_str()).unwrap_or("enable");
        let test_en = c.ports.iter().find(|p| p.name.name == "test_en")
            .map(|p| p.name.name.as_str());

        // Module header
        self.line(&format!("module {n} ("));
        self.indent += 1;
        let port_strs: Vec<String> = c.ports.iter().map(|p| {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty = self.emit_port_type_str(&p.ty);
            format!("{dir} {ty} {}", p.name.name)
        }).collect();
        for (i, ps) in port_strs.iter().enumerate() {
            let comma = if i < port_strs.len() - 1 { "," } else { "" };
            self.line(&format!("{ps}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        let en_expr = if let Some(te) = test_en {
            format!("{enable} | {te}")
        } else {
            enable.to_string()
        };

        match c.kind {
            crate::ast::ClkGateKind::Latch => {
                self.line("logic en_latched;");
                self.line(&format!("always_latch if (!{clk_in}) en_latched = {en_expr};"));
                self.line(&format!("assign {clk_out} = {clk_in} & en_latched;"));
            }
            crate::ast::ClkGateKind::And => {
                self.line(&format!("assign {clk_out} = {clk_in} & ({en_expr});"));
            }
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    fn emit_synchronizer(&mut self, s: &SynchronizerDecl) {
        let n = &s.name.name;

        // Resolve STAGES (default 2)
        let stages = s.params.iter()
            .find(|p| p.name.name == "STAGES")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v as usize) } else { None })
            .unwrap_or(2);

        // Find clock ports (first = source clock, second = destination clock)
        let clk_ports: Vec<&PortDecl> = s.ports.iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .collect();
        let src_clk = &clk_ports[0].name.name;
        let dst_clk = &clk_ports[1].name.name;

        // Find data ports
        let data_in_port = s.ports.iter().find(|p| p.name.name == "data_in").unwrap();
        let data_ty = self.emit_port_type_str(&data_in_port.ty);

        // Check for reset port
        let rst_port = s.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Reset(..)));

        // Module header — emit all declared params as SV parameters
        self.line(&format!("module {n} #("));
        self.indent += 1;
        let param_strs: Vec<String> = s.params.iter().map(|p| {
            let val = p.default.as_ref()
                .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(v.to_string()) } else { None })
                .unwrap_or_else(|| "0".to_string());
            format!("parameter int {} = {}", p.name.name, val)
        }).collect();
        // Always include STAGES if not already declared
        let has_stages = s.params.iter().any(|p| p.name.name == "STAGES");
        let mut all_param_strs = param_strs;
        if !has_stages {
            all_param_strs.push(format!("parameter int STAGES = {stages}"));
        }
        for (i, ps) in all_param_strs.iter().enumerate() {
            let comma = if i < all_param_strs.len() - 1 { "," } else { "" };
            self.line(&format!("{ps}{comma}"));
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;
        // Emit ports
        let port_strs: Vec<String> = s.ports.iter().map(|p| {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty = self.emit_port_type_str(&p.ty);
            format!("{dir} {ty} {}", p.name.name)
        }).collect();
        for (i, ps) in port_strs.iter().enumerate() {
            let comma = if i < port_strs.len() - 1 { "," } else { "" };
            self.line(&format!("{ps}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        match s.kind {
            SyncKind::Ff => self.emit_sync_ff(dst_clk, &data_ty, rst_port, stages),
            SyncKind::Gray => self.emit_sync_gray(src_clk, dst_clk, &data_ty, rst_port, stages),
            SyncKind::Handshake => self.emit_sync_handshake(src_clk, dst_clk, &data_ty, rst_port, stages),
            SyncKind::Reset => self.emit_sync_reset(dst_clk, rst_port, stages),
            SyncKind::Pulse => self.emit_sync_pulse(src_clk, dst_clk, rst_port, stages),
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Synchronizer kind helpers ────────────────────────────────────────────

    fn emit_sync_reset_begin(&mut self, dst_clk: &str, rst_port: Option<&PortDecl>) -> Option<String> {
        if let Some(rp) = rst_port {
            let is_low = matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low);
            let is_async = matches!(&rp.ty, TypeExpr::Reset(sync_type, _) if *sync_type == ResetKind::Async);
            let sensitivity = if is_async {
                let edge = if is_low { "negedge" } else { "posedge" };
                format!(" or {edge} {}", rp.name.name)
            } else {
                String::new()
            };
            self.line(&format!("always_ff @(posedge {dst_clk}{sensitivity}) begin"));
            let cond = if is_low { format!("!{}", rp.name.name) } else { rp.name.name.clone() };
            Some(cond)
        } else {
            self.line(&format!("always_ff @(posedge {dst_clk}) begin"));
            None
        }
    }

    fn emit_sync_ff(&mut self, dst_clk: &str, data_ty: &str, rst_port: Option<&PortDecl>, stages: usize) {
        self.line(&format!("// {stages}-stage FF synchronizer chain (destination clock: {dst_clk})"));
        self.line(&format!("{data_ty} sync_chain [0:STAGES-1];"));
        self.line("");

        let rst_cond = self.emit_sync_reset_begin(dst_clk, rst_port);
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            self.line("for (int i = 0; i < STAGES; i++) sync_chain[i] <= '0;");
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }
        self.line("sync_chain[0] <= data_in;");
        self.line("for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];");
        if rst_cond.is_some() {
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("assign data_out = sync_chain[STAGES-1];");
    }

    fn emit_sync_gray(&mut self, src_clk: &str, dst_clk: &str, data_ty: &str, rst_port: Option<&PortDecl>, stages: usize) {
        self.line(&format!("// Gray-code synchronizer ({stages} stages, {src_clk} → {dst_clk})"));
        self.line(&format!("{data_ty} bin_to_gray;"));
        self.line(&format!("{data_ty} gray_chain [0:STAGES-1];"));
        self.line(&format!("{data_ty} gray_to_bin;"));
        self.line("");

        // Binary-to-gray encode (combinational, source domain)
        self.line("assign bin_to_gray = data_in ^ (data_in >> 1);");
        self.line("");

        // FF chain on destination clock
        let rst_cond = self.emit_sync_reset_begin(dst_clk, rst_port);
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            self.line("for (int i = 0; i < STAGES; i++) gray_chain[i] <= '0;");
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }
        self.line("gray_chain[0] <= bin_to_gray;");
        self.line("for (int i = 1; i < STAGES; i++) gray_chain[i] <= gray_chain[i-1];");
        if rst_cond.is_some() {
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Gray-to-binary decode: binary[i] = XOR of all gray bits from MSB down to i
        // Computed as: b = g ^ (g>>1) ^ (g>>2) ^ ... — no self-reference, no ordering issue.
        self.line("// Gray-to-binary decode (prefix XOR — no self-reference)");
        self.line(&format!("always_comb begin"));
        self.indent += 1;
        self.line("gray_to_bin = gray_chain[STAGES-1];");
        self.line(&format!("for (int i = 1; i < $bits({data_ty}); i++)"));
        self.indent += 1;
        self.line("gray_to_bin ^= gray_chain[STAGES-1] >> i;");
        self.indent -= 1;
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("assign data_out = gray_to_bin;");
    }

    fn emit_sync_handshake(&mut self, src_clk: &str, dst_clk: &str, data_ty: &str, rst_port: Option<&PortDecl>, stages: usize) {
        self.line(&format!("// Handshake synchronizer ({stages} stages, {src_clk} → {dst_clk})"));
        self.line(&format!("{data_ty} data_reg;"));
        self.line("logic req_src, ack_src;");
        self.line(&format!("logic req_sync [0:STAGES-1];  // req synchronized to {dst_clk}"));
        self.line(&format!("logic ack_sync [0:STAGES-1];  // ack synchronized to {src_clk}"));
        self.line("logic ack_dst;");
        self.line("");

        let rst_name = rst_port.map(|rp| rp.name.name.as_str()).unwrap_or("1'b0");
        let is_low = rst_port.map_or(false, |rp| matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low));
        let rst_active = if is_low { format!("!{rst_name}") } else { rst_name.to_string() };

        // Source domain: latch data and toggle req
        self.line(&format!("// Source domain ({src_clk}): latch data, manage req/ack"));
        self.line(&format!("always_ff @(posedge {src_clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_active}) begin"));
        self.indent += 1;
        self.line("req_src <= 1'b0;");
        self.line("data_reg <= '0;");
        self.indent -= 1;
        self.line("end else if (data_in !== data_reg && req_src == ack_src) begin");
        self.indent += 1;
        self.line("data_reg <= data_in;");
        self.line("req_src <= ~req_src;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Synchronize req into destination domain
        self.line(&format!("// Synchronize req into {dst_clk}"));
        self.line(&format!("always_ff @(posedge {dst_clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_active}) begin"));
        self.indent += 1;
        self.line("for (int i = 0; i < STAGES; i++) req_sync[i] <= 1'b0;");
        self.line("ack_dst <= 1'b0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("req_sync[0] <= req_src;");
        self.line("for (int i = 1; i < STAGES; i++) req_sync[i] <= req_sync[i-1];");
        self.line("ack_dst <= req_sync[STAGES-1];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Synchronize ack back into source domain
        self.line(&format!("// Synchronize ack back into {src_clk}"));
        self.line(&format!("always_ff @(posedge {src_clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_active}) begin"));
        self.indent += 1;
        self.line("for (int i = 0; i < STAGES; i++) ack_sync[i] <= 1'b0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("ack_sync[0] <= ack_dst;");
        self.line("for (int i = 1; i < STAGES; i++) ack_sync[i] <= ack_sync[i-1];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        self.line("assign ack_src = ack_sync[STAGES-1];");
        self.line("assign data_out = data_reg;");
    }

    fn emit_sync_reset(&mut self, dst_clk: &str, _rst_port: Option<&PortDecl>, _stages: usize) {
        // Reset synchronizer: data_in is the async reset input (active high).
        // Assert immediately (async), deassert through N-stage FF chain (sync to dst_clk).
        self.line(&format!("// Reset synchronizer: async assert, sync deassert on {dst_clk}"));
        self.line("logic sync_chain [0:STAGES-1];");
        self.line("");

        self.line(&format!("always_ff @(posedge {dst_clk} or posedge data_in) begin"));
        self.indent += 1;
        self.line("if (data_in) begin");
        self.indent += 1;
        self.line("for (int i = 0; i < STAGES; i++) sync_chain[i] <= 1'b1;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("sync_chain[0] <= 1'b0;");
        self.line("for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("assign data_out = sync_chain[STAGES-1];");
    }

    fn emit_sync_pulse(&mut self, src_clk: &str, dst_clk: &str, rst_port: Option<&PortDecl>, _stages: usize) {
        let rst_name = rst_port.map(|rp| rp.name.name.as_str());
        let is_low = rst_port.map_or(false, |rp| matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low));
        let rst_cond = rst_name.map(|n| if is_low { format!("!{n}") } else { n.to_string() });

        self.line(&format!("// Pulse synchronizer: {src_clk} → {dst_clk}"));
        self.line("// Source: pulse → toggle; Destination: sync toggle → edge detect → pulse");
        self.line("logic toggle_src;");
        self.line("logic sync_chain [0:STAGES-1];");
        self.line("logic pulse_dst;");
        self.line("");

        // Source domain: toggle on input pulse
        self.line(&format!("always_ff @(posedge {src_clk}) begin"));
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) toggle_src <= 1'b0;"));
            self.line("else if (data_in) toggle_src <= ~toggle_src;");
        } else {
            self.line("if (data_in) toggle_src <= ~toggle_src;");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Destination domain: sync the toggle through FF chain
        self.line(&format!("always_ff @(posedge {dst_clk}) begin"));
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            self.line("for (int i = 0; i < STAGES; i++) sync_chain[i] <= 1'b0;");
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }
        self.line("sync_chain[0] <= toggle_src;");
        self.line("for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];");
        if rst_cond.is_some() {
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Edge detect: XOR of last two stages → single-cycle pulse
        self.line("assign data_out = sync_chain[STAGES-1] ^ sync_chain[STAGES-2];");
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
        // Compute the bit-width number directly from the TypeExpr to avoid
        // fragile string parsing of the emitted type (e.g. "logic [7:0]").
        let data_width_num = r.params.iter()
            .find(|p| p.name.name == "WIDTH")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => self.type_expr_data_width(ty),
                _ => None,
            })
            .unwrap_or_else(|| "8".to_string());

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
            RamKind::Rom => self.emit_ram_rom(r, &clk_name),
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
                RamInit::File(path, format) => {
                    let func = match format {
                        FileFormat::Hex => "$readmemh",
                        FileFormat::Bin => "$readmemb",
                    };
                    self.line(&format!("initial {func}(\"{path}\", mem);"));
                }
                RamInit::Array(values) => {
                    self.line("initial begin");
                    self.indent += 1;
                    for (i, v) in values.iter().enumerate() {
                        self.line(&format!("mem[{i}] = {data_width_num}'h{v:X};"));
                    }
                    self.indent -= 1;
                    self.line("end");
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

        if !r.asserts.is_empty() {
            let clk = r.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = r.asserts.clone();
            let rname = r.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &rname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Counter ───────────────────────────────────────────────────────────────

    fn emit_cam(&mut self, c: &crate::ast::CamDecl) {
        let n = c.name.name.clone();

        // Required params (validated by typecheck): DEPTH, KEY_W
        let depth_default = c.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());
        let key_w_default = c.params.iter()
            .find(|p| p.name.name == "KEY_W")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());
        // v3: optional VAL_W param activates the value-payload bundle.
        let has_value = c.params.iter().any(|p| p.name.name == "VAL_W");
        let val_w_default = c.params.iter()
            .find(|p| p.name.name == "VAL_W")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());

        let clk = c.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&c.ports);

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        if has_value {
            self.line(&format!("parameter int DEPTH = {depth_default},"));
            self.line(&format!("parameter int KEY_W = {key_w_default},"));
            self.line(&format!("parameter int VAL_W = {val_w_default}"));
        } else {
            self.line(&format!("parameter int DEPTH = {depth_default},"));
            self.line(&format!("parameter int KEY_W = {key_w_default}"));
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

        // ── Storage ──────────────────────────────────────────────────────────
        self.line("logic [DEPTH-1:0]      entry_valid_r;");
        self.line("logic [KEY_W-1:0]      entry_key_r [DEPTH];");
        if has_value {
            self.line("logic [VAL_W-1:0]      entry_value_r [DEPTH];");
        }
        self.line("");

        // ── Combinational match ──────────────────────────────────────────────
        // search_mask[i] = entry_valid_r[i] && (entry_key_r[i] == search_key)
        self.line("always_comb begin");
        self.indent += 1;
        self.line("for (int i = 0; i < DEPTH; i++) begin");
        self.indent += 1;
        self.line("search_mask[i] = entry_valid_r[i] && (entry_key_r[i] == search_key);");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("assign search_any = |search_mask;");
        self.line("");

        // ── Priority encoder for search_first (LSB-first) ────────────────────
        self.line("always_comb begin");
        self.indent += 1;
        self.line("search_first = '0;");
        self.line("for (int i = DEPTH-1; i >= 0; i--) begin");
        self.indent += 1;
        self.line("if (search_mask[i]) search_first = i[$clog2(DEPTH)-1:0];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Value-payload read (v3) ──────────────────────────────────────────
        // read_value reflects the entry at search_first; the caller should
        // qualify with search_any (it reads as 0 when there is no match).
        if has_value {
            self.line("assign read_value = entry_value_r[search_first];");
            self.line("");
        }

        // ── Sequential write port(s) ─────────────────────────────────────────
        // v2: if write2_* ports exist, emit two sequential write blocks back-
        // to-back (write1 first, then write2) so write2 wins on same-index
        // conflict (last-write semantics). Different-index writes both commit.
        let has_w2 = c.ports.iter().any(|p| p.name.name == "write2_valid");

        let ff_sens = Self::ff_sensitivity(&clk, &rst, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line("entry_valid_r <= '0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        // Port 1
        self.line("if (write_valid) begin");
        self.indent += 1;
        self.line("if (write_set) begin");
        self.indent += 1;
        self.line("entry_valid_r[write_idx] <= 1'b1;");
        self.line("entry_key_r[write_idx] <= write_key;");
        if has_value {
            self.line("entry_value_r[write_idx] <= write_value;");
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("entry_valid_r[write_idx] <= 1'b0;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        // Port 2 (v2 only) — placed AFTER port 1 so it wins on same-index conflict.
        if has_w2 {
            self.line("if (write2_valid) begin");
            self.indent += 1;
            self.line("if (write2_set) begin");
            self.indent += 1;
            self.line("entry_valid_r[write2_idx] <= 1'b1;");
            self.line("entry_key_r[write2_idx] <= write2_key;");
            if has_value {
                self.line("entry_value_r[write2_idx] <= write2_value;");
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            self.line("entry_valid_r[write2_idx] <= 1'b0;");
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        if !c.asserts.is_empty() {
            let asserts = c.asserts.clone();
            self.emit_asserts_for_construct(&asserts, &n, &clk);
        }

        self.indent -= 1;
        self.line("endmodule");
        self.line("");
    }

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
        self.line(&format!("logic [{}] count_r;", Self::fold_width_str(&count_width)));

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

        // Auto-generated safety assertions for counter invariants
        {
            self.line("");
            self.line("// synopsys translate_off");
            if max_param.is_some() {
                // Use the SV parameter name `MAX` so instantiation overrides are honored.
                self.line(&format!(
                    "_auto_count_range: assert property (@(posedge {clk}) count_r <= {count_width}'(MAX))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"COUNTER OVERFLOW: {n}.count_r > MAX\");"
                ));
            }
            self.line("// synopsys translate_on");
        }

        if !c.asserts.is_empty() {
            let clk = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = c.asserts.clone();
            let cname = c.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &cname, &clk);
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
                        parenthesized: false,
                    })
            );
            self.line(&format!("logic grant_valid_comb;"));
            self.line(&format!("logic [{}:0] grant_requester_comb;", req_width - 1));
            self.line(&format!("logic [{}] {req_ready_sig}_comb;", Self::fold_width_str(&num_req_str)));
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
                    self.line(&format!("logic [{}:0] {dst_gr};", req_width - 1));
                    self.line(&format!("logic [{}] {dst_rr};", Self::fold_width_str(&num_req_str)));
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

        if !a.asserts.is_empty() {
            let clk = a.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = a.asserts.clone();
            let aname = a.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &aname, &clk);
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
                    format!("{dir} logic [{}] {name}", Self::fold_width_str(&count_str))
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
        self.line(&format!("logic [{}:0] rr_ptr_r;", req_width - 1));
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
        self.line(&format!("logic [{}:0] last_grant_r;", num_req - 1));
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
        self.line(&format!("logic [{}:0] grant_onehot;", num_req - 1));
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
                        ParamKind::Const | ParamKind::WidthConst(..) => p.default.as_ref().map(|e| self.emit_expr_str(e)),
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
        self.line(&format!("logic [{}] rf_data [0:NREGS-1];", Self::fold_width_str(&data_width_num)));
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

        if !r.asserts.is_empty() {
            let clk = r.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = r.asserts.clone();
            let rname = r.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &rname, &clk);
        }

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
                format!("logic [{}]", Self::fold_width_str(data_width))
            }
            TypeExpr::Named(id) if id.name == "ADDR_WIDTH" || id.name.to_lowercase().contains("addr") => {
                format!("logic [{}:0]", addr_width - 1)
            }
            TypeExpr::UInt(w) => {
                let ws = self.emit_expr_str(w);
                format!("logic [{}]", Self::fold_width_str(&ws))
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

    fn emit_ram_rom(&mut self, r: &RamDecl, clk: &str) {
        let pg = &r.port_groups[0];
        let pfx = &pg.name.name;
        let out_sig = pg.signals.iter().find(|s| s.direction == Direction::Out);

        match r.latency {
            0 => {
                // Combinational read
                if let Some(os) = out_sig {
                    self.line("");
                    self.line(&format!("assign {pfx}_{} = mem[{pfx}_addr];", os.name.name));
                }
            }
            1 => {
                if let Some(os) = out_sig {
                    let rdata_r = format!("{pfx}_{}_r", os.name.name);
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                    self.line("");
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    let has_en = pg.signals.iter().any(|s| s.name.name == "en");
                    if has_en {
                        self.line(&format!("if ({pfx}_en)"));
                        self.indent += 1;
                    }
                    self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                    if has_en { self.indent -= 1; }
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("assign {pfx}_{} = {rdata_r};", os.name.name));
                }
            }
            2 => {
                if let Some(os) = out_sig {
                    let rdata_r = format!("{pfx}_{}_r", os.name.name);
                    let rdata_r2 = format!("{pfx}_{}_r2", os.name.name);
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r2};"));
                    self.line("");
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                    self.line(&format!("{rdata_r2} <= {rdata_r};"));
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("assign {pfx}_{} = {rdata_r2};", os.name.name));
                }
            }
            _ => {}
        }
    }

    // ── Linklist ─────────────────────────────────────────────────────────────

    fn emit_linklist(&mut self, l: &crate::ast::LinklistDecl) {
        use crate::ast::LinklistKind;
        let n = &l.name.name;
        let is_doubly = matches!(l.kind, LinklistKind::Doubly | LinklistKind::CircularDoubly);
        let is_circular = matches!(l.kind, LinklistKind::CircularSingly | LinklistKind::CircularDoubly);

        // Multi-head linklist support. NUM_HEADS defaults to 1; when set
        // to N > 1, the head / tail / length registers become arrays
        // indexed by a per-op `req_head_idx` port (validated at typecheck).
        // The node pool and free list stay shared across all heads.
        let num_heads = crate::typecheck::linklist_num_heads(l);
        let multi_head = num_heads > 1;
        let num_heads_expr = l.params.iter()
            .find(|p| p.name.name == "NUM_HEADS")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "1".to_string());

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

        // Operations that touch a specific head (need `req_head_idx` when
        // multi-head). Shared-pool ops (alloc, free) and slot-addressed
        // ops (read_data, write_data, next, prev) don't.
        let head_addressed_op = |name: &str| matches!(
            name,
            "insert_head" | "insert_tail" | "insert_after" | "delete_head" | "delete"
        );

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
        if multi_head {
            self.line(&format!("parameter int  NUM_HEADS = {num_heads_expr},"));
        }
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
        if multi_head {
            self.line("localparam int HEAD_IDX_W = $clog2(NUM_HEADS);");
        }
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
        if multi_head {
            self.line("logic [HANDLE_W-1:0] _head_r [NUM_HEADS];");
            if l.track_tail {
                self.line("logic [HANDLE_W-1:0] _tail_r [NUM_HEADS];");
            }
            // Internal per-head occupancy counter — used for "this head
            // is empty" detection (insert vs. append branch, delete
            // req_ready gating). Always emitted in multi-head mode;
            // not user-visible.
            self.line("logic [CNT_W-1:0]    _length_r [NUM_HEADS];");
        } else {
            self.line("logic [HANDLE_W-1:0] _head_r;");
            if l.track_tail {
                self.line("logic [HANDLE_W-1:0] _tail_r;");
            }
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
            // Multi-head: latch the requested head idx at accept cycle so
            // the busy cycle can reuse it without re-reading the live port.
            if multi_head && head_addressed_op(on) && op.latency > 1 {
                self.line(&format!("logic [HEAD_IDX_W-1:0] _ctrl_{on}_head_idx;"));
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
            let is_head_addr = head_addressed_op(on);
            if op.ports.iter().any(|p| p.name.name == "req_ready") {
                let guard = if op.latency > 1 {
                    format!("!_ctrl_{on}_busy && ")
                } else {
                    String::new()
                };
                // Multi-head delete: gate on "this head has entries"
                // rather than "pool has any entries". Insert ops still
                // gate on the shared free list (full pool = stall).
                let cond = match on.as_str() {
                    "alloc" | "insert_head" | "insert_tail" | "insert_after" => {
                        format!("{guard}!(_fl_cnt == '0)")
                    }
                    "free" => {
                        format!("{guard}!(_fl_cnt == CNT_W'(DEPTH))")
                    }
                    "delete_head" | "delete" if multi_head && is_head_addr => {
                        format!("{guard}(_length_r[{on}_req_head_idx] != '0)")
                    }
                    "delete_head" | "delete" => {
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
        if multi_head {
            self.line("for (_ll_i = 0; _ll_i < NUM_HEADS; _ll_i++) begin");
            self.indent += 1;
            self.line("_head_r[_ll_i] <= '0;");
            if l.track_tail { self.line("_tail_r[_ll_i] <= '0;"); }
            self.line("_length_r[_ll_i] <= '0;");
            self.indent -= 1;
            self.line("end");
        } else {
            self.line("_head_r <= '0;");
            if l.track_tail { self.line("_tail_r <= '0;"); }
        }
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
            self.emit_ll_op_controller(op, l.track_tail, is_doubly, is_circular, num_heads);
        }

        self.indent -= 1;
        self.line("end"); // else
        self.indent -= 1;
        self.line("end"); // always_ff
        self.line("");

        if !l.asserts.is_empty() {
            let clk = l.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = l.asserts.clone();
            let lname = l.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &lname, &clk);
        }

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
        num_heads: u32,
    ) {
        let on = &op.name.name;
        let has_req_valid   = op.ports.iter().any(|p| p.name.name == "req_valid");
        let has_resp_valid  = op.ports.iter().any(|p| p.name.name == "resp_valid");
        let has_req_handle  = op.ports.iter().any(|p| p.name.name == "req_handle");
        let has_req_data    = op.ports.iter().any(|p| p.name.name == "req_data");
        let multi_head = num_heads > 1;
        let is_head_addr = matches!(
            on.as_str(),
            "insert_head" | "insert_tail" | "insert_after" | "delete_head" | "delete"
        );
        // Head-register access expressions.
        // - `_accept` variant is used in the accept cycle (latency==1 or
        //   the first branch of a latency>1 op). Reads the live
        //   `req_head_idx` port directly.
        // - `_busy` variant is used in subsequent busy cycles. Reads the
        //   latched `_ctrl_<op>_head_idx`.
        // For single-head lists both resolve to bare `_head_r` / `_tail_r`
        // so the emitted SV stays byte-identical with the pre-multi-head
        // compiler.
        let head_r_accept = if multi_head && is_head_addr {
            format!("_head_r[{on}_req_head_idx]")
        } else { "_head_r".to_string() };
        let head_r_busy = if multi_head && is_head_addr {
            format!("_head_r[_ctrl_{on}_head_idx]")
        } else { "_head_r".to_string() };
        // _tail_r is only read at the busy cycle (post-accept). The
        // accept-cycle variant would be `_tail_r[<op>_req_head_idx]`
        // if an op ever needed it.
        let tail_r_busy = if multi_head && is_head_addr {
            format!("_tail_r[_ctrl_{on}_head_idx]")
        } else { "_tail_r".to_string() };

        self.line(&format!("// ── {on} ─────────────────────────────────────────"));

        // Phase-B scope: multi-head supports insert_tail + delete_head
        // only. Other head-addressed ops need wiring the latched head_idx
        // into their branching / pointer-patch paths; deferred to a
        // follow-up phase.
        if multi_head && matches!(on.as_str(), "insert_head" | "insert_after" | "delete") {
            self.line(&format!(
                "// NOTE: op `{on}` is not yet supported for multi-head linklist (Phase B)."
            ));
            self.line(&format!(
                "initial $fatal(1, \"linklist: op `{on}` not yet implemented for multi-head (NUM_HEADS > 1)\");"
            ));
            return;
        }

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
                // Empty check: single-head uses pool occupancy; multi-head
                // uses the per-head length counter so chains from other
                // heads don't mask this head's emptiness.
                if multi_head {
                    self.line(&format!("_ctrl_{on}_was_empty <= (_length_r[{on}_req_head_idx] == '0);"));
                    self.line(&format!("_ctrl_{on}_head_idx  <= {on}_req_head_idx;"));
                } else {
                    self.line(&format!("_ctrl_{on}_was_empty <= (_fl_cnt == CNT_W'(DEPTH));"));
                }
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                if track_tail {
                    self.line(&format!("if (!_ctrl_{on}_was_empty) _next_mem[{tail_r_busy}] <= _ctrl_{on}_resp_handle;"));
                    if is_doubly {
                        // new node.prev = old tail
                        self.line(&format!("_prev_mem[_ctrl_{on}_resp_handle] <= {tail_r_busy};"));
                    }
                    self.line(&format!("{tail_r_busy} <= _ctrl_{on}_resp_handle;"));
                    self.line(&format!("if (_ctrl_{on}_was_empty) {head_r_busy} <= _ctrl_{on}_resp_handle;"));
                } else {
                    self.line(&format!("if (!_ctrl_{on}_was_empty) _next_mem[{head_r_busy}] <= _ctrl_{on}_resp_handle;"));
                    self.line(&format!("if (_ctrl_{on}_was_empty) {head_r_busy} <= _ctrl_{on}_resp_handle;"));
                }
                if multi_head {
                    self.line(&format!("_length_r[_ctrl_{on}_head_idx] <= _length_r[_ctrl_{on}_head_idx] + 1'b1;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            "delete_head" => {
                // Latency-2: read head data, advance head, free old head slot
                let pool_gate = if multi_head {
                    format!("(_length_r[{on}_req_head_idx] != '0)")
                } else {
                    "!(_fl_cnt == CNT_W'(DEPTH))".to_string()
                };
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && {pool_gate}");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                self.line(&format!("_ctrl_delete_head_resp_data <= _data_mem[{head_r_accept}];"));
                self.line(&format!("_ctrl_delete_head_slot      <= {head_r_accept};"));
                if multi_head {
                    self.line(&format!("_ctrl_{on}_head_idx          <= {on}_req_head_idx;"));
                }
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                // Free the old head slot
                self.line("_fl_mem[_fl_wrp[HANDLE_W-1:0]] <= _ctrl_delete_head_slot;");
                self.line("_fl_wrp <= _fl_wrp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt + 1'b1;");
                // Advance head
                self.line(&format!("{head_r_busy} <= _next_mem[_ctrl_delete_head_slot];"));
                if multi_head {
                    self.line(&format!("_length_r[_ctrl_{on}_head_idx] <= _length_r[_ctrl_{on}_head_idx] - 1'b1;"));
                }
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

