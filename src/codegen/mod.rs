use crate::ast::*;
use crate::diagnostics::CompileWarning;
use crate::lexer::Span;
use crate::resolve::{Symbol, SymbolTable};
use crate::typecheck::enum_width;

// Per-construct submodules. Each contributes `pub(super) fn emit_<name>`
// to `impl Codegen` and lives in its own file mirroring the layout of
// `sim_codegen/`. New constructs land in their own file rather than
// growing this `mod.rs`.
mod arbiter;
mod cam;
mod clkgate;
mod counter;
mod fifo;
mod fsm;
mod linklist;
mod module;
mod pipeline;
mod ram;
mod regfile;
mod synchronizer;

/// SV assignment-operator context for the unified `emit_stmt` walker.
/// `Blocking` = `=` (comb / latch / reg-as-comb), `NonBlocking` = `<=` (seq).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum AssignCtx {
    Blocking,
    NonBlocking,
}

impl AssignCtx {
    fn op(&self) -> &'static str {
        match self {
            AssignCtx::Blocking => "=",
            AssignCtx::NonBlocking => "<=",
        }
    }
}

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
    /// Map of pipe_reg name → (source name, total stages N) for the
    /// current module being emitted. Used to lower `q@K` reads on RHS
    /// to the right SV intermediate signal: `q@0` → source, `q@K` for
    /// 1≤K<N → `q_stg{K}`, `q@N` → `q` (= bare q).
    pipe_regs: std::collections::HashMap<String, (String, u32)>,
    /// Vec-of-const param name → (element TypeExpr) for the current
    /// module. iverilog rejects unpacked-array params, so codegen emits
    /// the param packed and rewrites `B[i]` reads to `B[i*W +: W]`
    /// part-selects. Lookup populated per-module at emit time.
    vec_params: std::collections::HashMap<String, TypeExpr>,
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
            pipe_regs: std::collections::HashMap::new(),
            vec_params: std::collections::HashMap::new(),
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
            // Function / Template / Bus / Use have no top-level SV emit
            // (Function is emitted inside each module body, Template is
            // compile-time only, Bus is flattened at port sites, Use is
            // an import emitted inside modules) — their `Construct::emit_sv`
            // impl is the trait default no-op.
            item.as_construct().emit_sv(self);
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

    /// Emit one inst-site param override `.NAME(...)`. Handles two cases:
    ///
    /// 1. **Value override** (`pa.ty == None`) — emit `.NAME(<expr>)`.
    /// 2. **Type override** (`pa.ty == Some(te)`) — the override targets
    ///    a child param declared as `param NAME: type = ...`. SV codegen
    ///    has two conventions for these:
    ///    - `fifo` synthesizes an int `parameter DATA_WIDTH` from the
    ///      type-param's bit width. So a type override translates to
    ///      `.DATA_WIDTH(<bits-of-new-type>)` at the inst site.
    ///    - User modules emit type-typed params as `parameter int NAME`
    ///      (legacy quirk; type params on user modules aren't fully
    ///      supported at SV level today). Type overrides for those emit
    ///      `.NAME(<bits-of-new-type>)` as a best-effort.
    fn emit_param_override(&self, child: &str, pa: &ParamAssign) -> String {
        let Some(te) = &pa.ty else {
            return format!(".{}({})", pa.name.name, self.emit_expr_str(&pa.value));
        };
        let width = self.type_expr_data_width(te).unwrap_or_else(|| "0".to_string());
        // Map T → DATA_WIDTH for fifo children.
        let is_fifo_type_param = self.source.items.iter().any(|it| match it {
            Item::Fifo(f) if f.name.name == child => f.params.iter().any(|p|
                p.name.name == pa.name.name
                && matches!(p.kind, crate::ast::ParamKind::Type(_))),
            _ => false,
        });
        if is_fifo_type_param {
            format!(".DATA_WIDTH({width})")
        } else {
            format!(".{}({width})", pa.name.name)
        }
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
            ParamKind::ConstVec(ty) => {
                // Vec<T, N> param. iverilog rejects unpacked-array parameters,
                // so emit a packed `parameter logic [N*W-1:0] NAME = {…}` and
                // expose a sibling `wire NAME_arr [0:N-1]` (driven elsewhere
                // in the module body) for `NAME[i]` indexing.
                //
                // Default `{a, b, c, …}` (parsed as ExprKind::Concat) packs
                // with reversed ordering so `NAME[0]` lands at the LSB and
                // matches the user's literal index — `parts[0]` = LSB chunk.
                let (elem_ty, size_expr) = match ty {
                    TypeExpr::Vec(elem, size) => (elem.as_ref().clone(), (**size).clone()),
                    _ => {
                        self.line(&format!("{kw} int {}{}{}", p.name.name, default_str, comma));
                        return;
                    }
                };
                let elem_w_expr = match &elem_ty {
                    TypeExpr::UInt(w) | TypeExpr::SInt(w) => (**w).clone(),
                    _ => Expr::new(ExprKind::Literal(LitKind::Dec(1)), p.span),
                };
                let elem_w_s = self.emit_expr_str(&elem_w_expr);
                let signed = matches!(&elem_ty, TypeExpr::SInt(_));
                let signed_kw = if signed { "signed " } else { "" };
                let size_s = self.emit_expr_str(&size_expr);
                let default_packed = if let Some(d) = &p.default {
                    if let ExprKind::Concat(parts) = &d.kind {
                        // Reverse so parts[0] is the LSB chunk → NAME[0] reads parts[0].
                        let mut rev: Vec<&Expr> = parts.iter().collect();
                        rev.reverse();
                        let chunks: Vec<String> = rev.iter()
                            .map(|e| format!("({})'({})", elem_w_s, self.emit_expr_str(e)))
                            .collect();
                        format!(" = {{{}}}", chunks.join(", "))
                    } else {
                        format!(" = {}", self.emit_expr_str(d))
                    }
                } else { String::new() };
                self.line(&format!(
                    "{kw} logic {signed_kw}[({size_s})*({elem_w_s})-1:0] {}{default_packed}{comma}",
                    p.name.name
                ));
            }
            _ => {
                self.line(&format!("{kw} int {}{}{}", p.name.name, default_str, comma));
            }
        }
    }

    pub(crate) fn emit_domain(&mut self, d: &DomainDecl) {
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

    pub(crate) fn emit_package(&mut self, pkg: &PackageDecl) {
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

    pub(crate) fn emit_struct(&mut self, s: &StructDecl) {
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

    pub(crate) fn emit_enum(&mut self, e: &EnumDecl) {
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

    fn emit_for_loop_sv<S>(&mut self, f: &ForLoop<S>, mut emit_body_stmt: impl FnMut(&mut Self, &S)) {
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

    /// Unified `Stmt` emitter for `comb` and `seq` (and `latch`-as-comb)
    /// contexts. Phase 5b consolidation: the only thing the wrapping block
    /// decides is the assignment operator (`=` for blocking comb, `<=` for
    /// non-blocking seq). All other stmt-shape codegen is identical.
    ///
    /// `Blocking` is also used for the latch-block emitter and for emitting
    /// register-shaped FSM/state bodies as combinational logic (e.g. inside
    /// always_comb when the FSM lowering pulls the body out of seq).
    fn emit_stmt(&mut self, stmt: &Stmt, ctx: AssignCtx) {
        self.emit_comments_before(stmt_span_start(stmt));
        match stmt {
            Stmt::Assign(a) => {
                // Comb-context special case: `target = match scrut { ... }`
                // expands to a case block so the RHS can branch per pattern.
                // Seq context drives the same RHS through emit_expr_str, which
                // pretty-prints it as a ternary chain — no expansion needed.
                if ctx == AssignCtx::Blocking {
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
                        return;
                    }
                }
                let val = self.emit_expr_str(&a.value);
                let tgt = self.emit_expr_str(&a.target);
                self.line(&format!("{} {} {};", tgt, ctx.op(), val));
            }
            Stmt::IfElse(ie) => {
                self.emit_if_else(ie, ctx, false);
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
                        self.emit_stmt(s, ctx);
                    }
                    self.indent -= 1;
                    self.line("end");
                }
                self.indent -= 1;
                self.line("endcase");
            }
            Stmt::Log(l) => self.emit_log_stmt(l),
            Stmt::For(f) => {
                self.emit_for_loop_sv(f, |s, stmt| s.emit_stmt(stmt, ctx));
            }
            Stmt::Init(_) => {
                // `init on RST.asserted` blocks are extracted by emit_reg_block
                // before this walker runs; reaching here is a compiler bug.
                unreachable!("Stmt::Init reached emit_stmt; should be handled by emit_reg_block");
            }
            Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("Stmt::WaitUntil / DoUntil are pipeline-stage-seq only");
            }
        }
    }

    fn emit_if_else(&mut self, ie: &IfElse, ctx: AssignCtx, is_chain: bool) {
        let cond = self.emit_expr_str(&ie.cond);
        let u = if ie.unique && !is_chain { "unique " } else { "" };
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("{}if ({}) begin", u, cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_stmt(s, ctx);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_if_else(nested, ctx, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_stmt(s, ctx);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    fn emit_comb_stmt(&mut self, stmt: &Stmt) {
        self.emit_stmt(stmt, AssignCtx::Blocking);
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
    /// Searches modules and fsms (which carry their own `lets` field).
    fn let_binding_is_sint(&self, name: &str) -> bool {
        for item in &self.source.items {
            match item {
                Item::Module(m) if m.name.name == self.current_construct => {
                    for bi in &m.body {
                        if let ModuleBodyItem::LetBinding(l) = bi {
                            if l.name.name == name {
                                return l.ty.as_ref().map_or(false, |t| matches!(t, TypeExpr::SInt(_)));
                            }
                        }
                    }
                }
                Item::Fsm(f) if f.name.name == self.current_construct => {
                    for l in &f.lets {
                        if l.name.name == name {
                            return l.ty.as_ref().map_or(false, |t| matches!(t, TypeExpr::SInt(_)));
                        }
                    }
                }
                _ => {}
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
            ExprKind::LatencyAt(inner, _) | ExprKind::SvaNext(_, inner) => Self::expr_root_name(inner),
            _ => String::new(),
        }
    }

    /// Extract reset info from a port list: (name, is_async, is_low).
    /// Returns ("rst", false, false) as defaults if no Reset port found.
    fn extract_reset_info(ports: &[PortDecl]) -> (String, bool, bool) {
        crate::ast::extract_reset_info(ports)
    }

    /// Compute the total bit-width of a TypeExpr (for FIFO `DATA_WIDTH`,
    /// RAM `WIDTH`, and similar param-derived widths). Recurses through
    /// `Vec<T, N>`, struct fields, and enum variants. Used by the fifo /
    /// ram codegen submodules; lives here as a shared helper because
    /// nothing about it is fifo-specific.
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
                    let bits = crate::width::index_width(n as u64);
                    Some(bits.to_string())
                } else {
                    None
                }
            }
        }
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
        self.emit_stmt(stmt, AssignCtx::NonBlocking);
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
                .map(|p| self.emit_param_override(&inst.module_name.name, p))
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
                            push(format!("int'({idx_s}) < ({limit})"), "vec", sites, seen);
                        } else if let Some(w) = scalar_widths.get(&name) {
                            push(format!("int'({idx_s}) < ({w})"), "bitsel", sites, seen);
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
            BinOp::Implies | BinOp::ImpliesNext => 2,
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
        let idx_w = crate::width::index_width(n as u64);

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
                let w = crate::width::index_width((n + 1) as u64);
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
            // `q@K` on RHS lowers to the K-th tap of the pipe_reg
            // chain (`q` being the final flop, source being the input
            // before any flop). Numbering counts cycles of delay from
            // the input: `@0` = source comb, `@K` = after K flops,
            // `@N` = bare `q`. Falls through transparently when the
            // base isn't a known pipe_reg name (typecheck rejects
            // out-of-range / non-pipe-reg uses earlier).
            ExprKind::LatencyAt(inner, n) => {
                if let ExprKind::Ident(name) = &inner.kind {
                    if let Some((source, stages)) = self.pipe_regs.get(name) {
                        let stages = *stages;
                        if *n == 0 {
                            return source.clone();
                        }
                        if *n == stages {
                            return name.clone();
                        }
                        if *n < stages {
                            return format!("{name}_stg{n}");
                        }
                    }
                }
                self.emit_expr_inner(inner)
            }
            // SVA forward-shift: `##N expr` only legal inside an assert
            // /cover property (typecheck enforces). Emit verbatim — SV
            // accepts it natively in property context.
            ExprKind::SvaNext(n, inner) => format!("##{n} {}", self.emit_expr_inner(inner)),
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
                    return format!("{l} |-> {r}");
                }
                if *op == BinOp::ImpliesNext {
                    // SVA next-cycle implication. Only valid inside
                    // assert/cover property contexts (typechecker enforces).
                    let l = self.emit_expr_prec(lhs, 4);
                    let r = self.emit_expr_prec(rhs, 4);
                    return format!("{l} |=> {r}");
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
                    BinOp::Implies | BinOp::ImpliesNext => unreachable!("implies handled above"),
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
                            // Direction-agnostic resize: pads or truncates, preserving
                            // signedness. SV's `N'(expr)` size cast inherits the
                            // signedness of `expr` and — critically — forwards
                            // context-determined evaluation through arithmetic
                            // operators inside it. Earlier emission used
                            // `N'($signed(expr))` / `N'($unsigned(expr))`, but
                            // `$signed`/`$unsigned` evaluate their argument in
                            // self-determined context (LRM §11.6.1, §20.5), which
                            // truncates a multiply like `a * b` to operand width
                            // BEFORE the outer cast widens — silently losing the
                            // upper bits of any product. Dropping the wrapper lets
                            // `N'(a * b)` widen both operands to N before the
                            // multiply. For non-arithmetic `expr` (idents, slices),
                            // the cast still preserves signedness from the
                            // underlying declaration, so no behaviour changes.
                            let wp = Self::paren_width(&w);
                            format!("{wp}'({b})")
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
                    // `as Vec<T, N>` is a typecheck-only view (UInt<N>'s
                    // bits read as N elements). Width is identical so SV
                    // can pass the inner expression through unchanged.
                    TypeExpr::Vec(_, _) => e,
                    _ => {
                        let t = self.emit_type_str(ty);
                        format!("{t}'({e})")
                    }
                }
            }
            ExprKind::Index(base, idx) => {
                // Vec-of-const param `B[i]`: rewrite to packed part-select
                // `B[i*W +: W]` since iverilog rejects unpacked-array params.
                if let ExprKind::Ident(name) = &base.kind {
                    if let Some(elem_ty) = self.vec_params.get(name) {
                        let w = match elem_ty {
                            TypeExpr::UInt(w) | TypeExpr::SInt(w) => self.emit_expr_str(w),
                            _ => "1".to_string(),
                        };
                        let i = self.emit_expr_str(idx);
                        // The packed param is declared `signed` for SInt
                        // elements, so the part-select inherits signedness
                        // without an explicit `$signed()` wrap.
                        return format!("{name}[({i}) * ({w}) +: ({w})]");
                    }
                }
                let b = self.emit_expr_str(base);
                let i = self.emit_expr_str(idx);
                format!("{b}[{i}]")
            }
            ExprKind::BitSlice(base, hi, lo) => {
                let b = self.emit_expr_str(base);
                // Parenthesize complex base expressions to avoid precedence issues.
                // SynthIdent is a compiler-renamed bare identifier with the same
                // semantics as Ident — no parens needed (Verilator rejects
                // `(__name)[hi:lo]` as a syntax error). Concat is also accepted
                // bare per SV-2009 §11.4.12 (concatenation with bit-select):
                // Verilator rejects `({a, b})[hi:lo]` for the same reason.
                // FunctionCall / MethodCall result bit-select is similarly
                // accepted bare; `(func())[hi:lo]` is rejected by Verilator
                // because bit-select doesn't compose with the parenthesized
                // expression — but `func()[hi:lo]` is valid (function-call
                // result is an "lvalue-like" form per the SV grammar).
                let b = if matches!(base.kind, ExprKind::Ident(_) | ExprKind::SynthIdent(_, _)
                    | ExprKind::Literal(_)
                    | ExprKind::Index(_, _) | ExprKind::FieldAccess(_, _)
                    | ExprKind::Concat(_)
                    | ExprKind::FunctionCall(_, _) | ExprKind::MethodCall(_, _, _)) { b }
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
                // Built-in SVA: past/rose/fell → SV $past/$rose/$fell
                if name == "past" || name == "rose" || name == "fell" {
                    return format!("${name}({})", arg_strs.join(", "));
                }
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

    /// Emit `Vec<T,N>` as an SV **unpacked** array at port boundaries:
    /// base type is the element type (e.g. `logic [W-1:0]`); array
    /// dimensions go in the suffix after the port name (e.g. `[N-1:0]`).
    ///
    /// Used only for ports declared with the `unpacked` modifier. Caller
    /// is responsible for restricting this to port emission — unpacked
    /// arrays are fine in Verilator but Yosys-unfriendly in synthesis,
    /// so all internal nets/regs/signals continue to use the packed shape
    /// from `emit_type_and_array_suffix`.
    fn emit_type_and_unpacked_suffix(&self, ty: &TypeExpr) -> (String, String) {
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
        let base_ty = self.emit_type_str(cur);
        let suffix: String = dims.iter().map(|d| format!(" {d}")).collect();
        (base_ty, suffix)
    }

    // ── Synchronizer ─────────────────────────────────────────────────────────
    // ── RAM ───────────────────────────────────────────────────────────────────

}
