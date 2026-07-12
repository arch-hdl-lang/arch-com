use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostics::{span_to_source_span, CompileError, CompileWarning};
use crate::lexer::Span;
use crate::resolve::{Symbol, SymbolTable};

/// Resolved type information
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    UInt(u32),
    SInt(u32),
    Bool,
    /// IEEE-754 binary32 (1+8+23 = 32 bits). Stored/carried as 32 bits.
    FP32,
    /// bfloat16 (1+8+7 = 16 bits). Stored/carried as 16 bits.
    BF16,
    Clock(String),                // domain name
    Reset(ResetKind, ResetLevel), // always concrete (Param resolved during elaboration)
    Vec(Box<Ty>, u32),
    Struct(String),
    Enum(String, u32), // name, bit width
    Bus(String),       // bus type name
    Todo,
    Error,
}

impl Ty {
    /// True for the floating-point types (FP32, BF16).
    pub fn is_float(&self) -> bool {
        matches!(self, Ty::FP32 | Ty::BF16)
    }
}

#[derive(Clone)]
pub(crate) enum HandshakePayloadGuard {
    Field(String),
    ReqAck2PhasePending {
        req_field: String,
        ack_field: String,
    },
}

impl HandshakePayloadGuard {
    fn display(&self, port: &str) -> String {
        match self {
            HandshakePayloadGuard::Field(field) => format!("{port}.{field}"),
            HandshakePayloadGuard::ReqAck2PhasePending {
                req_field,
                ack_field,
            } => {
                format!("({port}.{req_field} != {port}.{ack_field})")
            }
        }
    }
}

impl Ty {
    pub fn width(&self) -> Option<u32> {
        match self {
            Ty::UInt(w) | Ty::SInt(w) => Some(*w),
            Ty::Bool => Some(1),
            Ty::FP32 => Some(32),
            Ty::BF16 => Some(16),
            Ty::Enum(_, w) => Some(*w),
            Ty::Vec(inner, count) => inner.width().map(|w| w * count),
            Ty::Struct(_) | Ty::Bus(_) => None,
            Ty::Clock(_) | Ty::Reset(_, _) => Some(1),
            Ty::Todo | Ty::Error => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Ty::UInt(w) => format!("UInt<{w}>"),
            Ty::SInt(w) => format!("SInt<{w}>"),
            Ty::Bool => "Bool".to_string(),
            Ty::FP32 => "FP32".to_string(),
            Ty::BF16 => "BF16".to_string(),
            Ty::Clock(d) => format!("Clock<{d}>"),
            Ty::Reset(k, l) => format!(
                "Reset<{}, {}>",
                match k {
                    ResetKind::Sync => "Sync",
                    ResetKind::Async => "Async",
                },
                match l {
                    ResetLevel::High => "High",
                    ResetLevel::Low => "Low",
                },
            ),
            Ty::Vec(inner, n) => format!("Vec<{}, {n}>", inner.display()),
            Ty::Struct(name) => name.clone(),
            Ty::Enum(name, _) => name.clone(),
            Ty::Bus(name) => format!("bus {name}"),
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
    /// True while resolving an `assert`/`cover` body. Multi-cycle SVA
    /// constructs (`past(x, N)`, `a |=> b`) are only legal inside this
    /// scope; flagged as compile errors elsewhere.
    pub in_sva_context: bool,
    /// Per-module map of Vec-of-bus port names → element count. Populated
    /// at the start of check_module so check_stmt can expand indexed
    /// driver-tracking writes (`chans[i].sig = ...`) over all N copies
    /// when the index isn't a compile-time literal (e.g. inside a `for`
    /// loop). Cleared between modules.
    pub vec_of_bus_ports: HashMap<String, u32>,
    /// Parameter scope for the construct currently being typechecked.
    ///
    /// Constant-width expressions must resolve `local param` names within the
    /// current construct first. Falling back directly to the whole source file
    /// lets unrelated modules with the same local-param name collide.
    active_params: Vec<ParamDecl>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a SymbolTable, source: &'a SourceFile) -> Self {
        Self {
            symbols,
            source,
            errors: Vec::new(),
            warnings: Vec::new(),
            overload_map: HashMap::new(),
            in_sva_context: false,
            vec_of_bus_ports: HashMap::new(),
            active_params: Vec::new(),
        }
    }

    pub fn check(
        mut self,
    ) -> Result<(Vec<CompileWarning>, HashMap<usize, usize>), Vec<CompileError>> {
        // Global pre-pass: scan every compile-time expression for divide-by-zero
        // whose divisor is a reducible constant. Catches bad param defaults, etc.
        // Runs before per-item checks so reported errors sort naturally.
        self.check_const_div_zero();
        for item in self.source.items.clone().iter() {
            let saved_params = std::mem::replace(&mut self.active_params, Self::item_params(item));
            item.as_construct().typecheck(&mut self);
            self.active_params = saved_params;
        }
        // Cross-item check: every `inst foo: SomeRegfile` whose target
        // regfile has `kind: latch` must drive its write-port `addr` /
        // `data` connections from a flop-like source (reg / port reg /
        // pipe_reg / input port / inst output). Combinational sources
        // (let / wire / arithmetic expr) are rejected — see
        // `doc/plan_regfile_latch.md` for the timing assumption.
        for item in &self.source.items {
            if let Item::Module(m) = item {
                self.check_latch_regfile_writes(m);
            }
        }
        // Whole-design comb-loop analysis (issue #246). Walks the full
        // instance hierarchy starting from top-level modules and runs
        // Tarjan's SCC over a unified `(inst_path, signal)` graph. The
        // existing per-module analyzer (`comb_graph::analyze_module`)
        // only catches sibling-instance cycles within one parent and
        // silently absorbs them as settle_depth=2; this surfaces them
        // as warnings and also catches cycles that span hierarchy.
        self.check_whole_design_comb_loops();
        if self.errors.is_empty() {
            Ok((self.warnings, self.overload_map))
        } else {
            Err(self.errors)
        }
    }

    fn item_params(item: &Item) -> Vec<ParamDecl> {
        match item {
            Item::Module(m) => m.params.clone(),
            Item::Fsm(f) => f.params.clone(),
            Item::Fifo(f) => f.params.clone(),
            Item::Ram(r) => r.params.clone(),
            Item::Cam(c) => c.params.clone(),
            Item::Counter(c) => c.params.clone(),
            Item::Arbiter(a) => a.params.clone(),
            Item::Regfile(r) => r.params.clone(),
            Item::Pipeline(p) => p.params.clone(),
            Item::Linklist(l) => l.params.clone(),
            Item::Bus(b) => b.params.clone(),
            Item::Synchronizer(s) => s.params.clone(),
            Item::Clkgate(c) => c.params.clone(),
            Item::Template(t) => t.params.clone(),
            Item::Package(p) => p.params.clone(),
            _ => Vec::new(),
        }
    }

    /// Issue #246 MVP: warn on every comb-feedback SCC found in the
    /// whole-design instance-flat graph. Blessed-by-pragma SCCs are
    /// silently suppressed.
    fn check_whole_design_comb_loops(&mut self) {
        let analysis = crate::comb_graph::analyze_whole_design(self.source, self.symbols);
        if analysis.sccs.is_empty() && analysis.total_sccs == 0 {
            return;
        }
        for scc in &analysis.sccs {
            // Pick a span for the warning: the span of the first owning
            // module in the SCC (top-level module if vec![]). Fall back
            // to the first item's span.
            let span: Span = scc
                .owning_modules
                .iter()
                .filter_map(|mn| {
                    self.source.items.iter().find_map(|it| {
                        if let Item::Module(m) = it {
                            if &m.name.name == mn {
                                return Some(m.span);
                            }
                        }
                        None
                    })
                })
                .next()
                .unwrap_or_else(|| {
                    self.source
                        .items
                        .first()
                        .map(|it| it.span())
                        .unwrap_or(Span { start: 0, end: 0 })
                });

            let path_str: Vec<String> = scc.nodes.iter().map(|n| n.display()).collect();
            let module_list: Vec<String> = {
                let mut v: Vec<String> = scc.owning_modules.iter().cloned().collect();
                v.sort();
                v
            };
            let msg = format!(
                "whole-design combinational feedback cycle ({} nodes) involving modules [{}]; cycle: {}{}",
                scc.nodes.len(),
                module_list.join(", "),
                path_str.join(" -> "),
                if path_str.is_empty() { String::new() } else { format!(" -> {}", path_str[0]) },
            );
            self.warnings.push(CompileWarning { message: msg, span });
        }
        // Summary line emitted as a single warning so it shows up in the
        // standard warning stream.
        if analysis.total_sccs > 0 {
            let span = self
                .source
                .items
                .first()
                .map(|it| it.span())
                .unwrap_or(Span { start: 0, end: 0 });
            let summary = format!(
                "arch check: {} comb SCC(s) found; {} suppressed by pragma; {} unblessed (warnings)",
                analysis.total_sccs,
                analysis.suppressed,
                analysis.sccs.len(),
            );
            self.warnings.push(CompileWarning {
                message: summary,
                span,
            });
        }
    }

    /// Static dataflow check for `kind: latch` regfile instances:
    /// the `addr` and `data` write-port connections must each be a
    /// bare identifier (or `port.signal` field access on a bus port)
    /// resolving to a register-typed source in the parent module —
    /// `reg`, `port reg`, `pipe_reg`, an input port, or another
    /// inst's output. Combinational sources are rejected because
    /// transparent latches require their addr/data inputs to be
    /// stable for the duration of `we`.
    pub(crate) fn check_latch_regfile_writes(&mut self, m: &ModuleDecl) {
        // Build a per-module signal-kind map: name → "reg" / "port_reg"
        // / "pipe_reg" / "input_port" / "inst_output" / "wire" / "let".
        let mut kind_of: std::collections::HashMap<String, &'static str> =
            std::collections::HashMap::new();
        for p in &m.ports {
            let k = if p.reg_info.is_some() {
                "port_reg"
            } else if p.direction == Direction::In {
                "input_port"
            } else {
                "comb_port"
            };
            kind_of.insert(p.name.name.clone(), k);
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    kind_of.insert(r.name.name.clone(), "reg");
                }
                ModuleBodyItem::PipeRegDecl(p) => {
                    kind_of.insert(p.name.name.clone(), "pipe_reg");
                }
                ModuleBodyItem::WireDecl(w) => {
                    kind_of.insert(w.name.name.clone(), "wire");
                }
                ModuleBodyItem::LetBinding(l) => {
                    kind_of.insert(l.name.name.clone(), "let");
                }
                ModuleBodyItem::Inst(i) => {
                    kind_of.insert(i.name.name.clone(), "inst_name");
                }
                _ => {}
            }
        }

        for item in &m.body {
            let inst = match item {
                ModuleBodyItem::Inst(i) => i,
                _ => continue,
            };
            // Resolve the target — only regfile constructs matter.
            let target = self.source.items.iter().find_map(|it| match it {
                Item::Regfile(rf) if rf.name.name == inst.module_name.name => {
                    Some((rf.kind, rf.flops))
                }
                _ => None,
            });
            let Some((crate::ast::RegfileKind::Latch, flops)) = target else {
                continue;
            };
            // `flops: internal` means the regfile auto-emits its own wdata_q /
            // waddr_q sample flops + per-row ICG, so the caller is allowed to
            // drive write pins combinationally — skip the static flop-source
            // check entirely. (`flops: external` is the default; caller must
            // pre-flop, which is the property this check enforces.)
            if matches!(flops, crate::ast::RegfileFlops::Internal) {
                continue;
            }

            for c in &inst.connections {
                // Latch-RF write-port pins follow the "<pfx>_addr" / "<pfx>_addr"
                // shape (or "<pfx>{i}_addr" for multi-port). We only care about
                // pins that end with `_addr` or `_data` and live on a write port
                // (input direction at the inst's module side).
                let pname = &c.port_name.name;
                let is_addr = pname.ends_with("_addr") || pname.ends_with("_waddr");
                let is_data = pname.ends_with("_data") || pname.ends_with("_wdata");
                if !(is_addr || is_data) {
                    continue;
                }
                if c.direction != ConnectDir::Input {
                    continue;
                }

                // Walk the signal expression: accept Ident, Member, Index by
                // const ident; reject anything else (Binary / Unary / arbitrary
                // arithmetic).
                let root = root_ident_for_latch_check(&c.signal);
                let what = match &root {
                    Some(name) => kind_of.get(name.as_str()).copied(),
                    None => None,
                };
                let pin_label = if is_addr { "addr" } else { "data" };
                match what {
                    Some("reg") | Some("port_reg") | Some("pipe_reg") | Some("input_port")
                    | Some("inst_name") | Some("inst_output") => {
                        // OK — register-typed source (or boundary trust).
                    }
                    Some("wire") | Some("let") => {
                        self.errors.push(CompileError::general(
                            &format!(
                                "kind: latch regfile `{}` requires `{pin_label}` to be driven directly from a flop (a `reg` / `port reg` / `pipe_reg` / input port / inst output) — not a `wire` or `let` binding. Latches need addr/data stable while `we` is high; combinational sources can glitch. Move the value into a `reg` first.",
                                inst.module_name.name
                            ),
                            c.span,
                        ));
                    }
                    Some("comb_port") => {
                        self.errors.push(CompileError::general(
                            &format!(
                                "kind: latch regfile `{}` requires `{pin_label}` to be driven from a flop, but the source is a combinational output port. Move the value into a `reg` (or use `port reg`) before connecting.",
                                inst.module_name.name
                            ),
                            c.span,
                        ));
                    }
                    None => {
                        self.errors.push(CompileError::general(
                            &format!(
                                "kind: latch regfile `{}` requires `{pin_label}` to be a bare identifier resolving to a flop-typed source (no arithmetic, slicing, or concat). The current expression cannot be statically verified as glitch-free; move the value into a `reg` first.",
                                inst.module_name.name
                            ),
                            c.span,
                        ));
                    }
                    _ => {} // unreachable
                }
            }
        }
    }

    /// Walk every compile-time expression in the source (param defaults +
    /// const-typed `let` bindings) and emit an error when a `/` or `%`
    /// subexpression's divisor folds to 0. We rely on `eval_const_expr`
    /// already returning `None` on /0, so this pass is the only place
    /// that *rejects* such cases — elsewhere they're just "not reducible".
    pub(crate) fn check_const_div_zero(&mut self) {
        fn params_of(item: &Item) -> &[ParamDecl] {
            match item {
                Item::Module(m) => &m.params,
                Item::Fsm(f) => &f.params,
                Item::Fifo(f) => &f.params,
                Item::Ram(r) => &r.params,
                Item::Cam(c) => &c.params,
                Item::Counter(c) => &c.params,
                Item::Arbiter(a) => &a.params,
                Item::Regfile(r) => &r.params,
                Item::Pipeline(p) => &p.params,
                Item::Synchronizer(s) => &s.params,
                _ => &[],
            }
        }

        // Collect param defaults + const lets from the whole source tree.
        // Use an empty local_types map; any name that can't resolve as a
        // const-propagating identifier just returns None (ignored).
        let empty_types: HashMap<String, Ty> = HashMap::new();
        let mut report_sites: Vec<Span> = Vec::new();
        for item in &self.source.items {
            for p in params_of(item) {
                if let Some(def) = &p.default {
                    self.scan_expr_for_div_zero(def, &empty_types, &mut report_sites);
                }
            }
            // Module-body const lets (and const regs) — divisor in their
            // initializer is also compile-time and worth catching.
            if let Item::Module(m) = item {
                for it in &m.body {
                    if let ModuleBodyItem::LetBinding(l) = it {
                        self.scan_expr_for_div_zero(&l.value, &empty_types, &mut report_sites);
                    }
                }
            }
        }
        for sp in report_sites {
            self.errors.push(CompileError::General {
                message: "divide by zero in constant expression: divisor evaluates to 0"
                    .to_string(),
                span: span_to_source_span(sp),
            });
        }
    }

    fn scan_expr_for_div_zero(
        &self,
        e: &Expr,
        local_types: &HashMap<String, Ty>,
        out: &mut Vec<Span>,
    ) {
        match &e.kind {
            ExprKind::Binary(op, lhs, rhs) => {
                self.scan_expr_for_div_zero(lhs, local_types, out);
                self.scan_expr_for_div_zero(rhs, local_types, out);
                if matches!(op, BinOp::Div | BinOp::Mod) {
                    if let Some(0) = self.eval_const_expr(rhs, local_types) {
                        out.push(rhs.span);
                    }
                }
            }
            ExprKind::Unary(_, inner) => self.scan_expr_for_div_zero(inner, local_types, out),
            ExprKind::Index(base, idx) => {
                self.scan_expr_for_div_zero(base, local_types, out);
                self.scan_expr_for_div_zero(idx, local_types, out);
            }
            ExprKind::BitSlice(base, _, _) => {
                self.scan_expr_for_div_zero(base, local_types, out);
            }
            ExprKind::PartSelect(a, start, width, _) => {
                self.scan_expr_for_div_zero(a, local_types, out);
                self.scan_expr_for_div_zero(start, local_types, out);
                self.scan_expr_for_div_zero(width, local_types, out);
            }
            ExprKind::Ternary(c, t, f) => {
                self.scan_expr_for_div_zero(c, local_types, out);
                self.scan_expr_for_div_zero(t, local_types, out);
                self.scan_expr_for_div_zero(f, local_types, out);
            }
            ExprKind::MethodCall(base, _, args) => {
                self.scan_expr_for_div_zero(base, local_types, out);
                for a in args {
                    self.scan_expr_for_div_zero(a, local_types, out);
                }
            }
            ExprKind::FunctionCall(_, args) => {
                for a in args {
                    self.scan_expr_for_div_zero(a, local_types, out);
                }
            }
            ExprKind::Clog2(a) => self.scan_expr_for_div_zero(a, local_types, out),
            ExprKind::Concat(parts) => {
                for p in parts {
                    self.scan_expr_for_div_zero(p, local_types, out);
                }
            }
            ExprKind::FieldAccess(base, _) => self.scan_expr_for_div_zero(base, local_types, out),
            _ => {}
        }
    }

    pub(crate) fn check_domain(&mut self, d: &DomainDecl) {
        self.check_pascal_case(&d.name);
    }

    pub(crate) fn check_struct(&mut self, s: &StructDecl) {
        self.check_pascal_case(&s.name);
        for field in &s.fields {
            self.check_snake_case(&field.name);
            // v1: floats are only supported as scalar module signals. A float
            // inside a struct would reach codegen via FieldAccess, which the
            // float-op dispatch does not yet resolve — reject rather than
            // silently emit integer arithmetic on the bit pattern.
            if type_expr_contains_float(&field.ty) {
                self.errors.push(CompileError::general(
                    &format!(
                        "floating-point types (FP32/BF16) are not supported in struct fields in v1 (field `{}` of `{}`)",
                        field.name.name, s.name.name
                    ),
                    field.name.span,
                ));
            }
        }
    }

    pub(crate) fn check_enum(&mut self, e: &EnumDecl) {
        self.check_pascal_case(&e.name);
        for variant in &e.variants {
            self.check_pascal_case(variant);
        }
    }

    pub(crate) fn check_module(&mut self, m: &ModuleDecl) {
        self.check_pascal_case(&m.name);

        // Interface stub loaded from a `.archi` file — body is empty by
        // construction. Skip body-driven checks (output-driven, CDC/RDC,
        // body item validation) entirely; the stub exists only to provide
        // the port signature for parent-side instantiation checking, which
        // happens in `check_inst_decl` when validating the inst connections.
        if m.is_interface {
            return;
        }

        // Per-module map of Vec-of-bus port → count. Used by the Assign
        // path so that `chans[i].sig = ...` records all N flat copies as
        // driven when `i` is a loop variable (or any non-literal index).
        self.vec_of_bus_ports.clear();
        let empty_types: HashMap<String, Ty> = HashMap::new();
        for p in &m.ports {
            if let Some(bi) = p.bus_info.as_ref() {
                if let Some(count_expr) = bi.count.as_ref() {
                    if let Some(n) = self.eval_const_expr(count_expr, &empty_types) {
                        if n > 0 {
                            self.vec_of_bus_ports.insert(p.name.name.clone(), n as u32);
                        }
                    }
                }
            }
        }

        // v1 float restriction: scalar FP32/BF16 signals are supported, but a
        // float nested inside a Vec is not — `Vec<FP32,N>` element access
        // (Index) is not yet resolved by the float-op dispatch, so it would
        // silently emit integer arithmetic. Reject Vec-of-float on every
        // declared signal type. (Scalar floats pass `is_float` but not the
        // `Vec(...)` guard below.)
        let mut float_decls: Vec<(&TypeExpr, Span, String)> = Vec::new();
        for p in &m.ports {
            float_decls.push((&p.ty, p.name.span, p.name.name.clone()));
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    float_decls.push((&r.ty, r.name.span, r.name.name.clone()))
                }
                ModuleBodyItem::WireDecl(w) => {
                    float_decls.push((&w.ty, w.name.span, w.name.name.clone()))
                }
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(t) = l.ty.as_ref() {
                        float_decls.push((t, l.name.span, l.name.name.clone()));
                    }
                }
                _ => {}
            }
        }
        for (ty, span, name) in float_decls {
            if matches!(ty, TypeExpr::Vec(..)) && type_expr_contains_float(ty) {
                self.errors.push(CompileError::general(
                    &format!("floating-point types (FP32/BF16) inside `Vec` are not supported in v1 (signal `{name}`)"),
                    span,
                ));
            }
        }

        // A float `reg`'s reset value must be a float literal, not an integer
        // literal (`reset rst => 1` would store the bit pattern 0x1, a tiny
        // subnormal — almost never the intent). Catch the common foot-gun.
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if matches!(r.ty, TypeExpr::FP32 | TypeExpr::BF16) {
                    let val = match &r.reset {
                        RegReset::Explicit(_, _, _, v) => Some(v),
                        RegReset::Inherit(_, v) => Some(v),
                        RegReset::None => None,
                    };
                    if let Some(v) = val {
                        if matches!(
                            &v.kind,
                            ExprKind::Literal(
                                LitKind::Dec(_)
                                    | LitKind::Hex(_)
                                    | LitKind::Bin(_)
                                    | LitKind::Sized(_, _)
                            )
                        ) {
                            let tn = if matches!(r.ty, TypeExpr::FP32) {
                                "FP32"
                            } else {
                                "BF16"
                            };
                            self.errors.push(CompileError::general(
                                &format!("float `reg {}: {tn}` reset value must be a float literal (e.g. `=> 0.0`), not an integer literal", r.name.name),
                                v.span,
                            ));
                        }
                    }
                }
            }
        }

        // Same foot-gun check as above, for the `init` slot (arch#624) and
        // `port` default-value slot (arch#622): an integer literal directly
        // initializing a float reg/port is almost never the intent (it would
        // store the bit pattern, not the numeric value) and is rejected
        // consistently with `reset`, never silently accepted.
        {
            let mut int_lit_float_slots: Vec<(Span, &'static str, String)> = Vec::new();
            for item in &m.body {
                if let ModuleBodyItem::RegDecl(r) = item {
                    if matches!(r.ty, TypeExpr::FP32 | TypeExpr::BF16) {
                        if let Some(v) = &r.init {
                            if is_bare_int_literal(v) {
                                let tn = if matches!(r.ty, TypeExpr::FP32) {
                                    "FP32"
                                } else {
                                    "BF16"
                                };
                                int_lit_float_slots.push((v.span, tn, r.name.name.clone()));
                            }
                        }
                    }
                }
            }
            for p in &m.ports {
                if matches!(p.ty, TypeExpr::FP32 | TypeExpr::BF16) {
                    let tn = if matches!(p.ty, TypeExpr::FP32) {
                        "FP32"
                    } else {
                        "BF16"
                    };
                    if let Some(ri) = &p.reg_info {
                        if let Some(v) = &ri.init {
                            if is_bare_int_literal(v) {
                                int_lit_float_slots.push((v.span, tn, p.name.name.clone()));
                            }
                        }
                    }
                    if let Some(v) = &p.default {
                        if is_bare_int_literal(v) {
                            int_lit_float_slots.push((v.span, tn, p.name.name.clone()));
                        }
                    }
                }
            }
            for (span, tn, name) in int_lit_float_slots {
                self.errors.push(CompileError::general(
                    &format!("float `{name}: {tn}` initializer must be a float literal (e.g. `0.0`), not an integer literal"),
                    span,
                ));
            }
        }

        // Track driven signals
        let mut driven: HashSet<String> = HashSet::new();

        // Collect reg names for comb target validation (includes port reg ports)
        let reg_names: HashSet<String> = m
            .body
            .iter()
            .filter_map(|item| {
                if let ModuleBodyItem::RegDecl(r) = item {
                    Some(r.name.name.clone())
                } else {
                    None
                }
            })
            .chain(m.ports.iter().filter_map(|p| {
                if p.reg_info.is_some() {
                    Some(p.name.name.clone())
                } else {
                    None
                }
            }))
            .collect();

        // Validate `guard <sig>` annotations: signal must exist in scope and be Bool.
        self.check_guards(m);

        // Check params
        for p in &m.params {
            self.check_upper_snake(&p.name);
            self.check_width_const_overflow(p);
        }

        // Check ports — no naming enforcement; ports must match external interfaces
        // which may use any convention (uppercase, PascalCase, etc.)

        // Build local type environment
        let mut local_types: HashMap<String, Ty> = HashMap::new();
        for p in &m.params {
            if let Some(default) = &p.default {
                let ty = self.resolve_expr_type(default, &m.name.name, &local_types);
                local_types.insert(p.name.name.clone(), ty);
            }
        }
        for p in &m.ports {
            if p.bus_info.is_some() {
                // Bus ports: validate bus exists, register as a special type
                if let Some(ref bi) = p.bus_info {
                    if let Some((crate::resolve::Symbol::Bus(_), _)) =
                        self.symbols.globals.get(&bi.bus_name.name)
                    {
                        local_types.insert(p.name.name.clone(), Ty::Bus(bi.bus_name.name.clone()));
                    } else {
                        self.errors.push(CompileError::general(
                            &format!("unknown bus type `{}`", bi.bus_name.name),
                            bi.bus_name.span,
                        ));
                    }
                }
                continue;
            }
            let ty = self.resolve_type_expr(&p.ty, &m.name.name, &local_types);
            local_types.insert(p.name.name.clone(), ty);
        }

        // Pre-pass: collect all declared names/types so declarations are order-independent.
        // No validation here — just populate local_types for forward reference resolution.
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    let ty = self.resolve_type_expr(&r.ty, &m.name.name, &local_types);
                    local_types.insert(r.name.name.clone(), ty);
                }
                ModuleBodyItem::LetBinding(l) => {
                    if !l.destructure_fields.is_empty() {
                        // Destructuring: infer each bound name's type from the
                        // RHS struct. If we can't resolve the struct yet
                        // (forward reference), leave names unbound — the
                        // main pass re-checks.
                        let rhs_ty = self.resolve_expr_type(&l.value, &m.name.name, &local_types);
                        if let Ty::Struct(sname) = &rhs_ty {
                            // Synthesized find_first result: derive fields
                            // directly from the width-suffixed name.
                            if let Some(w_str) = sname.strip_prefix("__ArchFindResult_") {
                                if let Ok(w) = w_str.parse::<u32>() {
                                    for bind in &l.destructure_fields {
                                        let bty = match bind.name.as_str() {
                                            "found" => Ty::Bool,
                                            "index" => Ty::UInt(w),
                                            _ => Ty::Error,
                                        };
                                        local_types.insert(bind.name.clone(), bty);
                                    }
                                    continue;
                                }
                            }
                            if let Some((crate::resolve::Symbol::Struct(info), _)) =
                                self.symbols.globals.get(sname)
                            {
                                for bind in &l.destructure_fields {
                                    if let Some((_, fty)) =
                                        info.fields.iter().find(|(fname, _)| fname == &bind.name)
                                    {
                                        let bty =
                                            self.resolve_type_expr(fty, &m.name.name, &local_types);
                                        local_types.insert(bind.name.clone(), bty);
                                    }
                                }
                            }
                        }
                    } else if let Some(ty) = &l.ty {
                        let resolved = self.resolve_type_expr(ty, &m.name.name, &local_types);
                        local_types.insert(l.name.name.clone(), resolved);
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    let ty = self.resolve_type_expr(&w.ty, &m.name.name, &local_types);
                    local_types.insert(w.name.name.clone(), ty);
                }
                ModuleBodyItem::PipeRegDecl(p) => {
                    // Type = source type; may not be resolved yet, will be set in main pass
                    // Just reserve the name so other pipe_regs can chain from it
                    local_types.entry(p.name.name.clone()).or_insert(Ty::Error);
                }
                _ => {}
            }
        }

        // Main pass: check body items (validation, expression checking, driver tracking)
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    self.check_snake_case(&r.name);
                    let ty = self.resolve_type_expr(&r.ty, &m.name.name, &local_types);
                    local_types.insert(r.name.name.clone(), ty);
                    // Defensive: parser rejects `multicycle 0`, but if the AST
                    // is ever constructed programmatically (elaboration, tests)
                    // surface the constraint here too. Phase A landing point;
                    // Phase B will add input-feeding-tree analysis for the
                    // `--check-uninit` valid-tracking codegen pass.
                    if let Some(0) = r.multicycle {
                        self.errors.push(CompileError::general(
                            "`multicycle <N>` requires N >= 1",
                            r.span,
                        ));
                    }
                }
                ModuleBodyItem::RegBlock(rb) => {
                    // Check stmts
                    for stmt in &rb.stmts {
                        self.check_reg_stmt(stmt, &m.name.name, &local_types, &mut driven);
                    }
                    // Reject wait until / do..until in module seq blocks (only valid in pipeline stages)
                    Self::reject_wait_in_stmts(&rb.stmts, &mut self.errors);
                    // Reject `target <= expr;` (bare-ident target) inside a
                    // `for` loop in seq — last-iteration's NBA write wins,
                    // so the loop never has the cumulative effect users
                    // expect. Indexed targets (`vec[i] <= ...`) write a
                    // distinct element each iteration and stay allowed.
                    for stmt in &rb.stmts {
                        Self::reject_bare_assign_in_for(stmt, false, &mut self.errors);
                    }
                    // Validate reset consistency: all registers with reset in the
                    // same seq block must agree on signal name, sync/async, and polarity.
                    self.check_always_block_reset_consistency(rb, m);
                    // Warn if the seq body contains a redundant `if reset_signal` branch
                    // that shadows the declaration-level reset guard.
                    self.check_redundant_reset_branch(rb, m);
                }
                ModuleBodyItem::LatchBlock(lb) => {
                    // Validate enable signal exists and is Bool
                    if let Some(ty) = local_types.get(&lb.enable.name) {
                        if !matches!(ty, Ty::Bool | Ty::Clock(_)) {
                            self.errors.push(CompileError::general(
                                &format!(
                                    "latch enable signal `{}` must be Bool or Clock, found {:?}",
                                    lb.enable.name, ty
                                ),
                                lb.span,
                            ));
                        }
                    }
                    // Check stmts (same as seq — targets must be regs)
                    for stmt in &lb.stmts {
                        self.check_reg_stmt(stmt, &m.name.name, &local_types, &mut driven);
                    }
                }
                ModuleBodyItem::CombBlock(cb) => {
                    for stmt in &cb.stmts {
                        self.check_comb_stmt(
                            stmt,
                            &m.name.name,
                            &local_types,
                            &mut driven,
                            &reg_names,
                        );
                    }
                    self.check_comb_latch(&cb.stmts, cb.span);
                }
                ModuleBodyItem::LetBinding(l) => {
                    // Destructuring form: `let {f1, f2} = expr;`
                    if !l.destructure_fields.is_empty() {
                        let rhs_ty = self.resolve_expr_type(&l.value, &m.name.name, &local_types);
                        if l.ty.is_some() {
                            self.errors.push(CompileError::general(
                                "destructuring `let` does not accept a type annotation — types are inferred from the RHS struct",
                                l.span,
                            ));
                        }
                        let Ty::Struct(sname) = &rhs_ty else {
                            if rhs_ty != Ty::Error {
                                self.errors.push(CompileError::general(
                                    &format!(
                                        "destructuring `let` requires a struct-typed RHS, got `{}`",
                                        rhs_ty.display()
                                    ),
                                    l.value.span,
                                ));
                            }
                            continue;
                        };
                        // Synthesized find_first result: fields are derived
                        // from the struct name's width suffix, no StructInfo
                        // is registered in globals.
                        if sname.starts_with("__ArchFindResult_") {
                            for bind in &l.destructure_fields {
                                self.check_snake_case(bind);
                                if !matches!(bind.name.as_str(), "found" | "index") {
                                    self.errors.push(CompileError::general(
                                        &format!(
                                            "find_first result has no field named `{}`; valid fields are `found` and `index`",
                                            bind.name),
                                        bind.span,
                                    ));
                                }
                                let is_port = m.ports.iter().any(|p| p.name.name == bind.name);
                                if is_port {
                                    self.errors.push(CompileError::general(
                                        &format!("`{}` is already declared as a port", bind.name),
                                        bind.span,
                                    ));
                                }
                            }
                            continue;
                        }
                        let Some((crate::resolve::Symbol::Struct(info), _)) =
                            self.symbols.globals.get(sname).cloned()
                        else {
                            continue;
                        };
                        for bind in &l.destructure_fields {
                            self.check_snake_case(bind);
                            let field = info.fields.iter().find(|(fname, _)| fname == &bind.name);
                            if field.is_none() {
                                self.errors.push(CompileError::general(
                                    &format!(
                                        "struct `{}` has no field named `{}`",
                                        sname, bind.name
                                    ),
                                    bind.span,
                                ));
                                continue;
                            }
                            // local_types already contains these names — the
                            // pre-pass inserted them for forward-reference
                            // resolution. Only real name collisions (existing
                            // driven signals, port names) are problems.
                            let is_port = m.ports.iter().any(|p| p.name.name == bind.name);
                            if is_port {
                                self.errors.push(CompileError::general(
                                    &format!("`{}` is already declared as a port", bind.name),
                                    bind.span,
                                ));
                            }
                        }
                        continue;
                    }
                    self.check_snake_case(&l.name);
                    if l.ty.is_none() {
                        // `let x = expr;` without type annotation — assign to existing port/wire
                        let name = &l.name.name;
                        // Check if it's an input port
                        let is_input_port = m
                            .ports
                            .iter()
                            .any(|p| &p.name.name == name && p.direction == Direction::In);
                        // Check if it's an output port (non-reg)
                        let is_output_port = m.ports.iter().any(|p| {
                            &p.name.name == name
                                && p.direction == Direction::Out
                                && p.reg_info.is_none()
                        });
                        // Check if it's a reg (declared reg or port-reg)
                        let is_reg = reg_names.contains(name);

                        if is_input_port {
                            self.errors.push(CompileError::general(
                                &format!("cannot assign to input port `{}` in let", name),
                                l.span,
                            ));
                        } else if is_reg {
                            self.errors.push(CompileError::general(
                                &format!("cannot assign to reg `{}` in let; use seq block", name),
                                l.span,
                            ));
                        } else if is_output_port {
                            // Comb assignment to output port
                            let rhs_ty =
                                self.resolve_expr_type(&l.value, &m.name.name, &local_types);
                            if let Some(port_ty) = local_types.get(name).cloned() {
                                if rhs_ty != Ty::Error
                                    && rhs_ty != Ty::Todo
                                    && port_ty != rhs_ty
                                    && !types_compatible(&port_ty, &rhs_ty)
                                {
                                    self.errors.push(CompileError::type_mismatch(
                                        &port_ty.display(),
                                        &rhs_ty.display(),
                                        l.value.span,
                                    ));
                                }
                            }
                            if driven.contains(name) {
                                self.errors.push(CompileError::general(
                                    &format!("signal `{}` already has a driver", name),
                                    l.span,
                                ));
                            } else {
                                driven.insert(name.clone());
                            }
                        } else if local_types.contains_key(name) {
                            // Wire or previously declared let
                            let rhs_ty =
                                self.resolve_expr_type(&l.value, &m.name.name, &local_types);
                            if let Some(wire_ty) = local_types.get(name).cloned() {
                                if rhs_ty != Ty::Error
                                    && rhs_ty != Ty::Todo
                                    && wire_ty != rhs_ty
                                    && !types_compatible(&wire_ty, &rhs_ty)
                                {
                                    self.errors.push(CompileError::type_mismatch(
                                        &wire_ty.display(),
                                        &rhs_ty.display(),
                                        l.value.span,
                                    ));
                                }
                            }
                            if driven.contains(name) {
                                self.errors.push(CompileError::general(
                                    &format!("signal `{}` already has a driver", name),
                                    l.span,
                                ));
                            } else {
                                driven.insert(name.clone());
                            }
                        } else {
                            self.errors.push(CompileError::general(
                                &format!("`{}` is not declared; use `let {}: T = expr;` to declare a new wire", name, name),
                                l.span,
                            ));
                        }
                        // Do NOT insert into local_types or driven here — handled above per case
                    } else {
                        let ty = self.resolve_expr_type(&l.value, &m.name.name, &local_types);
                        // `let` bindings are always a fixed combinational
                        // expression (spec: "declaration (fixed combinational
                        // expr)") — a latency-N (N>0) `<pipelined, N>` result
                        // has no cycle to land on here; it must be bound in a
                        // `seq` block via an `@N` tap.
                        if let ExprKind::PipelinedCall(name, _, call_stages) = &l.value.kind {
                            self.errors.push(CompileError::general(
                                &format!(
                                    "`{name}<pipelined, {call_stages}>(...)` produces a latency-{call_stages} \
                                     result and cannot be used in a `let` binding (always combinational); \
                                     bind it in a `seq` block via `target@{call_stages} <= {name}<pipelined, {call_stages}>(...)`"
                                ),
                                l.value.span,
                            ));
                        }
                        if let Some(declared_ty) = &l.ty {
                            let expected =
                                self.resolve_type_expr(declared_ty, &m.name.name, &local_types);
                            if expected != Ty::Error
                                && ty != Ty::Error
                                && ty != Ty::Todo
                                && expected != ty
                                && !types_compatible(&expected, &ty)
                            {
                                self.errors.push(CompileError::type_mismatch(
                                    &expected.display(),
                                    &ty.display(),
                                    l.value.span,
                                ));
                            }
                            // Shift width check (IEEE §11.6.1: shifts are non-widening)
                            if let (Some(ew), Some(aw)) = (expected.width(), ty.width()) {
                                if ew > aw && expr_is_shift(&l.value) {
                                    self.errors.push(CompileError::general(
                                        &format!(
                                            "shift result is UInt<{aw}> but target `{}` is UInt<{ew}>; \
                                             shifts do not widen (IEEE §11.6.1). \
                                             To capture overflow, widen the operand first: `.zext<{ew}>() << n`",
                                            l.name.name
                                        ),
                                        l.value.span,
                                    ));
                                }
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
                            &format!(
                                "pipe_reg '{}': source signal '{}' not found",
                                p.name.name, p.source.name
                            ),
                            p.source.span,
                        ));
                    }
                    // Update type from pre-pass placeholder (Ty::Error) to actual source type
                    let ty = local_types
                        .get(&p.source.name)
                        .cloned()
                        .unwrap_or(Ty::Error);
                    local_types.insert(p.name.name.clone(), ty);
                    driven.insert(p.name.name.clone());
                }
                ModuleBodyItem::Inst(inst) => self.check_inst_decl(inst, &mut driven),
                ModuleBodyItem::WireDecl(w) => {
                    self.check_snake_case(&w.name);
                    let ty = self.resolve_type_expr(&w.ty, &m.name.name, &local_types);
                    local_types.insert(w.name.name.clone(), ty);
                    // Wire is NOT marked as driven here — it must be driven by a comb block
                }
                // Generate blocks that were preserved (param-dependent range) —
                // mark their inst output connections as driven.
                ModuleBodyItem::Generate(gen) => {
                    let items = match gen {
                        crate::ast::GenerateDecl::For(gf) => &gf.items,
                        crate::ast::GenerateDecl::If(gi) => &gi.then_items,
                    };
                    for gi in items {
                        if let crate::ast::GenItem::Inst(inst) = gi {
                            // Look up the instantiated module's bus ports so we can
                            // mark per-bus-signal flat names for bus-typed inst-outputs
                            // (`inst_port -> outer_vec[loop_var]`), mirroring
                            // check_inst_decl's static-inst handling.
                            let inst_module_ports: Option<&[crate::ast::PortDecl]> =
                                self.source.items.iter().find_map(|item| match item {
                                    Item::Module(m2) if m2.name.name == inst.module_name.name => {
                                        Some(m2.ports.as_slice())
                                    }
                                    Item::Fsm(f2) if f2.name.name == inst.module_name.name => {
                                        Some(f2.ports.as_slice())
                                    }
                                    _ => None,
                                });
                            for conn in &inst.connections {
                                if conn.direction == ConnectDir::Output {
                                    if let ExprKind::Ident(name) = &conn.signal.kind {
                                        driven.insert(name.clone());
                                    }
                                    // Handle bit-slice targets (e.g. data_out[...])
                                    if let ExprKind::BitSlice(base, _, _) = &conn.signal.kind {
                                        if let ExprKind::Ident(name) = &base.kind {
                                            driven.insert(name.clone());
                                        }
                                    }
                                    let flat = Self::expr_flat_name_tc(&conn.signal);
                                    if !flat.is_empty() {
                                        driven.insert(flat);
                                    }
                                    // Bus-port output to indexed Vec-of-bus parent
                                    // port (`inst_bus_port -> vec_port[loop_var]`):
                                    // SV emitter uses D2 array shape but driver
                                    // tracking still uses per-element flat names.
                                    // For a literal idx mark just that element; for
                                    // a loop variable, conservatively mark ALL N
                                    // (each sibling unroll iteration fills one).
                                    let inst_bus_info = inst_module_ports.and_then(|ports| {
                                        ports
                                            .iter()
                                            .find(|p| p.name.name == conn.port_name.name)
                                            .and_then(|p| p.bus_info.as_ref())
                                    });
                                    if let Some(bi) = inst_bus_info {
                                        if let Some((crate::resolve::Symbol::Bus(info), _)) =
                                            self.symbols.globals.get(&bi.bus_name.name)
                                        {
                                            let mut pm = info.default_param_map();
                                            for pa in &bi.params {
                                                pm.insert(pa.name.name.clone(), &pa.value);
                                            }
                                            let eff = info.effective_signals(&pm);
                                            let prefixes: Vec<String> = match &conn.signal.kind {
                                                ExprKind::Ident(n) => vec![n.clone()],
                                                ExprKind::Index(arr, idx) => {
                                                    if let ExprKind::Ident(arr_name) = &arr.kind {
                                                        if let ExprKind::Literal(LitKind::Dec(i)) =
                                                            &idx.kind
                                                        {
                                                            vec![format!("{}_{}", arr_name, i)]
                                                        } else if let Some(&n) =
                                                            self.vec_of_bus_ports.get(arr_name)
                                                        {
                                                            (0..n)
                                                                .map(|i| {
                                                                    format!("{}_{}", arr_name, i)
                                                                })
                                                                .collect()
                                                        } else {
                                                            Vec::new()
                                                        }
                                                    } else {
                                                        Vec::new()
                                                    }
                                                }
                                                _ => Vec::new(),
                                            };
                                            for prefix in &prefixes {
                                                for (sname, sdir, _) in &eff {
                                                    let inst_dir = match bi.perspective {
                                                        BusPerspective::Initiator => *sdir,
                                                        BusPerspective::Target => (*sdir).flip(),
                                                    };
                                                    if inst_dir == Direction::Out {
                                                        driven.insert(format!(
                                                            "{}_{}",
                                                            prefix, sname
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
                ModuleBodyItem::Thread(t) => {
                    // Under --thread-sim parallel, lower_threads is skipped,
                    // so threads survive to typecheck. Mark thread-driven
                    // signals (CombAssign + SeqAssign LHS) as driven so the
                    // single-driver check sees them. Full typecheck of thread
                    // bodies is light in Phase 1 (the spike emitter rejects
                    // unsupported shapes itself).
                    let vob_ports = self.vec_of_bus_ports.clone();
                    fn mark_target(
                        target: &crate::ast::Expr,
                        driven: &mut HashSet<String>,
                        vob_ports: &HashMap<String, u32>,
                    ) {
                        // Bare Ident, `bus.sig`, `arr[i].sig`, `arr[i]` all flow
                        // through expr_flat_name_tc / expr_root_name_tc the same
                        // way they do for comb-block targets — ensures bus-port
                        // outputs assigned inside thread bodies satisfy the
                        // driver-completeness check.
                        let root = TypeChecker::expr_root_name_tc(target);
                        if !root.is_empty() {
                            driven.insert(root.clone());
                        }
                        let flat = TypeChecker::expr_flat_name_tc(target);
                        if !flat.is_empty() && flat != root {
                            driven.insert(flat.clone());
                        }
                        // Indexed Vec-of-bus target with a non-literal idx —
                        // mirror the comb-block handling: mark every flat copy.
                        if let crate::ast::ExprKind::FieldAccess(base, field) = &target.kind {
                            if let crate::ast::ExprKind::Index(arr, idx) = &base.kind {
                                if let crate::ast::ExprKind::Ident(arr_name) = &arr.kind {
                                    let is_lit =
                                        matches!(&idx.kind, crate::ast::ExprKind::Literal(_));
                                    if !is_lit {
                                        if let Some(&n) = vob_ports.get(arr_name) {
                                            for i in 0..n {
                                                driven.insert(format!(
                                                    "{}_{}_{}",
                                                    arr_name, i, field.name
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    fn walk_thread(
                        stmts: &[crate::ast::ThreadStmt],
                        driven: &mut HashSet<String>,
                        vob_ports: &HashMap<String, u32>,
                    ) {
                        use crate::ast::ThreadStmt;
                        for s in stmts {
                            match s {
                                ThreadStmt::CombAssign(a) => {
                                    mark_target(&a.target, driven, vob_ports)
                                }
                                ThreadStmt::SeqAssign(a) => {
                                    mark_target(&a.target, driven, vob_ports)
                                }
                                ThreadStmt::ForkTlmAssign(a) => {
                                    mark_target(&a.target, driven, vob_ports)
                                }
                                ThreadStmt::IfElse(ie) => {
                                    walk_thread(&ie.then_stmts, driven, vob_ports);
                                    walk_thread(&ie.else_stmts, driven, vob_ports);
                                }
                                ThreadStmt::For { body, .. }
                                | ThreadStmt::Lock { body, .. }
                                | ThreadStmt::DoUntil { body, .. } => {
                                    walk_thread(body, driven, vob_ports)
                                }
                                ThreadStmt::ForkJoin(branches, _) => {
                                    for b in branches {
                                        walk_thread(b, driven, vob_ports);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    walk_thread(&t.body, &mut driven, &vob_ports);
                    // `default comb ... end default` targets — these drive
                    // bus signals between active states; they also satisfy
                    // the driver-completeness check (every output port has
                    // at least a default value).
                    for s in &t.default_comb {
                        if let Stmt::Assign(a) = s {
                            mark_target(&a.target, &mut driven, &vob_ports);
                        }
                    }
                }
                ModuleBodyItem::Resource(_) => {
                    // Resources are lowered before typecheck.
                }
                ModuleBodyItem::Assert(a) => {
                    // Verify expr is Bool; require a Clock port. Resolve in
                    // SVA context so multi-cycle constructs (`past`, `|=>`)
                    // are legal inside this body and rejected elsewhere.
                    self.in_sva_context = true;
                    let ty = self.resolve_expr_type(&a.expr, &m.name.name, &local_types);
                    self.in_sva_context = false;
                    if ty != Ty::Bool && ty != Ty::Error && ty != Ty::Todo {
                        self.errors.push(CompileError::general(
                            &format!(
                                "assert/cover expression must be Bool, found {}",
                                ty.display()
                            ),
                            a.expr.span,
                        ));
                    }
                    let has_clock = m.ports.iter().any(|p| matches!(&p.ty, TypeExpr::Clock(_)));
                    if !has_clock {
                        self.errors.push(CompileError::general(
                            "assert/cover requires a Clock port (needed for concurrent SVA)",
                            a.span,
                        ));
                    }
                }
                ModuleBodyItem::Function(f) => {
                    self.check_function(f);
                }
                ModuleBodyItem::TlmConnect(_) => {
                    // Source-level sugar lowered during elaboration.
                }
                ModuleBodyItem::TypeAlias(_) => {
                    // Aliases are substituted by `type_alias::resolve_type_aliases`
                    // before typecheck runs; reaching here means the resolver
                    // pre-pass was skipped (e.g. a programmatic AST). Silently
                    // ignore — the alias has no semantic effect on typecheck.
                }
            }
        }

        // Multi-driver check (SFG Check 1, closes #375)
        let sfg_drivers = crate::signal_flow::collect_module_drivers(m, self.source);
        self.errors
            .extend(crate::signal_flow::check_multi_driver(m, &sfg_drivers));

        // Check all output ports are driven
        for p in &m.ports {
            if let Some(ref bi) = p.bus_info {
                // Bus port: check each output signal is driven (flattened name: port_signal).
                // For Vec<Bus,N> ports, check each of the N copies independently.
                let bus_name = &bi.bus_name.name;
                if let Some((crate::resolve::Symbol::Bus(info), _)) =
                    self.symbols.globals.get(bus_name)
                {
                    let mut pm = info.default_param_map();
                    for pa in &bi.params {
                        pm.insert(pa.name.name.clone(), &pa.value);
                    }
                    let eff = info.effective_signals(&pm);
                    let prefixes: Vec<String> = match bi.count.as_ref() {
                        None => vec![p.name.name.clone()],
                        Some(_) => {
                            let n = self
                                .vec_of_bus_ports
                                .get(&p.name.name)
                                .copied()
                                .unwrap_or(0);
                            (0..n).map(|i| format!("{}_{}", p.name.name, i)).collect()
                        }
                    };
                    for prefix in &prefixes {
                        for (sname, sdir, _) in &eff {
                            let actual_dir = match bi.perspective {
                                BusPerspective::Initiator => *sdir,
                                BusPerspective::Target => (*sdir).flip(),
                            };
                            if actual_dir == Direction::Out {
                                let flat = format!("{}_{}", prefix, sname);
                                if !driven.contains(&flat) {
                                    self.errors.push(CompileError::UndriveOutput {
                                        name: flat,
                                        span: crate::diagnostics::span_to_source_span(p.name.span),
                                    });
                                }
                            }
                        }
                    }
                }
            } else if p.direction == Direction::Out && !driven.contains(&p.name.name) {
                self.errors.push(CompileError::UndriveOutput {
                    name: p.name.name.clone(),
                    span: crate::diagnostics::span_to_source_span(p.name.span),
                });
            }
        }

        // ── CDC check: detect cross-domain register reads ─────────────────────
        // Build clock port → domain name map
        let clk_domain: HashMap<String, String> = m
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Clock(domain) = &p.ty {
                    Some((p.name.name.clone(), domain.name.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Phase 1 RDC + the surrounding CDC pass share this gate.
        // `pragma cdc_safe;` opts out of CDC + phase 1 (legacy);
        // `pragma rdc_safe;` opts out of phase 1 too (unified RDC opt-out).
        if clk_domain.len() >= 2 && !m.cdc_safe && !m.rdc_safe {
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
                                        if let Some(consumer_domain) =
                                            clk_domain.get(&rb.clock.name)
                                        {
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

            // ── RDC check: a reset signal used by registers in more than
            // one clock domain is unsafe (deassertion not synchronised).
            // Reset's "domain" is inferred from the registers that use it
            // — no separate annotation needed. Fix is `synchronizer kind
            // reset` to deassert-synchronise the reset into the new domain.
            //
            // v1 narrows the check to **async** reset ports. Sync resets
            // crossing domains are technically a CDC concern (reset signal
            // treated as data) but rarely a real bug in practice — they
            // propagate through clocks and the deassertion-edge race that
            // makes async cross-domain reset dangerous doesn't apply. If
            // false-negatives become an issue, broaden by removing the
            // is-async filter below.
            let async_reset_ports: HashSet<String> = m
                .ports
                .iter()
                .filter_map(|p| {
                    if let TypeExpr::Reset(ResetKind::Async, _) = &p.ty {
                        Some(p.name.name.clone())
                    } else {
                        None
                    }
                })
                .collect();
            // Tracks (reset_signal_name → set of (clock_domain, conflict span)).
            // Span carries the first reg-decl that introduced each domain so
            // the diagnostic can point at the offending site. Both inline
            // `reg` decls and `port reg` decls participate.
            let mut reset_users: HashMap<String, Vec<(String, crate::lexer::Span)>> =
                HashMap::new();
            let record_reset = |sig: &str,
                                reg_name: &str,
                                span: crate::lexer::Span,
                                reset_users: &mut HashMap<
                String,
                Vec<(String, crate::lexer::Span)>,
            >| {
                if let Some(domain) = reg_domain.get(reg_name) {
                    let entry = reset_users.entry(sig.to_string()).or_default();
                    if !entry.iter().any(|(d, _)| d == domain) {
                        entry.push((domain.clone(), span));
                    }
                }
            };
            for item in &m.body {
                if let ModuleBodyItem::RegDecl(rd) = item {
                    let sig_name = match &rd.reset {
                        RegReset::None => continue,
                        RegReset::Explicit(s, _, _, _) => s.name.clone(),
                        RegReset::Inherit(s, _) => s.name.clone(),
                    };
                    if !async_reset_ports.contains(&sig_name) {
                        continue;
                    }
                    record_reset(&sig_name, &rd.name.name, rd.name.span, &mut reset_users);
                }
            }
            for p in &m.ports {
                if let Some(ri) = &p.reg_info {
                    let sig_name = match &ri.reset {
                        RegReset::None => continue,
                        RegReset::Explicit(s, _, _, _) => s.name.clone(),
                        RegReset::Inherit(s, _) => s.name.clone(),
                    };
                    if !async_reset_ports.contains(&sig_name) {
                        continue;
                    }
                    record_reset(&sig_name, &p.name.name, p.name.span, &mut reset_users);
                }
            }
            for (sig, users) in &reset_users {
                if users.len() > 1 {
                    let domains: Vec<&str> = users.iter().map(|(d, _)| d.as_str()).collect();
                    // Point at the second domain's introducer — the first is
                    // the established domain, the second is the violating
                    // crossing.
                    let report_span = users[1].1;
                    self.errors.push(CompileError::general(
                        &format!(
                            "RDC violation: reset signal `{sig}` is used by registers in \
                             multiple clock domains ({}). Use `synchronizer kind reset` to \
                             deassert-synchronise the reset into each receiving domain.",
                            domains.join(", ")
                        ),
                        report_span,
                    ));
                }
            }
        }

        // ── Phase 2a RDC: data-path async reset domain crossing ─────────
        // Each async reset signal originates its OWN domain (by name).
        // Sync and reset-none flops are transparent — they propagate
        // whatever async domains reach their data input. Violation:
        //   f.Async        and any data source reaches domain ≠ f.reset
        //   f.{Sync,None}  and the reach set contains > 1 domain.
        //
        // Fires on any module (single- or multi-clock-domain). Phase 2a
        // is intentionally NOT gated on `pragma cdc_safe;` — that pragma
        // suppresses CDC and the phase-1 cross-clock RDC structural
        // check, but the data-path RDC hazard is structurally distinct
        // (a single-clock multi-reset module trips it without any CDC
        // concern). A future `pragma rdc_safe;` annotation will be the
        // dedicated opt-out; for now, fix the design or refactor the
        // resets through synchronizers.
        // `pragma rdc_safe;` blanket-suppresses every data-flow / boundary
        // RDC check (phases 2a–2d). The cross-clock structural rule
        // (phase 1, gated above) honours this pragma too.
        if !m.rdc_safe {
            self.check_rdc_phase2a(m);
            self.check_reconvergent_syncs(m);
            self.check_rdc_combiner_at_inst(m);
        }

        // Validate `implements` template conformance
        if let Some(ref tmpl_name) = m.implements {
            self.check_implements(m, tmpl_name);
        }

        // Warn about port reg outputs assigned inside state-dependent if/elsif chains.
        // port reg adds 1-cycle output latency — if the output is driven by FSM state,
        // the value appears 1 cycle after the state transition, which is a common
        // source of timing mismatch with testbench models.
        self.check_port_reg_timing(m);

        // Deprecation: legacy `port reg NAME: out T` → suggest
        // `port NAME: out pipe_reg<T, 1>`. Both spellings emit identical
        // SV, so this is a pure soft nudge. Suppressed when ARCH_NO_DEPRECATIONS
        // is set (useful for large legacy codebases migrating incrementally).
        if std::env::var("ARCH_NO_DEPRECATIONS").is_err() {
            for p in &m.ports {
                if let Some(ri) = &p.reg_info {
                    if ri.legacy_port_reg {
                        self.warnings.push(CompileWarning {
                            message: format!(
                                "`port reg {name}: ...` is deprecated — use `port {name}: out pipe_reg<T, 1> ...` instead (identical SV; latency is visible in the port signature).",
                                name = p.name.name
                            ),
                            span: p.span,
                        });
                    }
                }
            }
        }

        // Tier 1.5 (Option A): warn on handshake payload reads that are not
        // enclosed in an `if <port>.<valid>` scope. Catches consumer-side
        // contract violations (reading stale/undefined payload when the
        // producer hasn't asserted valid). See doc/plan_handshake_construct.md.
        self.check_handshake_reads(m);
    }

    /// Check an `inst` declaration: validates port connections (unconnected
    /// inputs error, unconnected outputs warn), expands whole-bus connections
    /// to per-signal driven entries, and marks output signals as driven.
    /// Extracted from `check_module`'s main pass for readability — the
    /// original arm was 122 lines.
    pub(crate) fn check_inst_decl(&mut self, inst: &InstDecl, driven: &mut HashSet<String>) {
        self.check_snake_case(&inst.name);
        // Find the target construct's bus port info for whole-bus expansion
        let target_bus_ports: Vec<(String, String)> = self
            .source
            .items
            .iter()
            .find_map(|item| match item {
                Item::Module(m2) if m2.name.name == inst.module_name.name => {
                    Some(m2.ports.as_slice())
                }
                Item::Fsm(f2) if f2.name.name == inst.module_name.name => Some(f2.ports.as_slice()),
                _ => None,
            })
            .map(|ports| {
                ports
                    .iter()
                    .filter_map(|p| {
                        p.bus_info
                            .as_ref()
                            .map(|bi| (p.name.name.clone(), bi.bus_name.name.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Check for unconnected ports on the instantiated construct.
        //
        // Bus ports can be connected in one of two shapes:
        //   (a) whole-bus: `p -> tb;` — connection.port_name == "p"
        //   (b) per-field: `p.cmd_valid <- x; p.cmd_addr <- y;` —
        //       the parser concatenates base.field, producing
        //       port_name == "p_cmd_valid", "p_cmd_addr", ...
        // For the per-field shape we consider the bus port connected
        // if any connection's port_name starts with `<bus_port>_`.
        {
            let child_ports: Option<&[PortDecl]> =
                self.source.items.iter().find_map(|item| match item {
                    Item::Module(m2) if m2.name.name == inst.module_name.name => {
                        Some(m2.ports.as_slice())
                    }
                    Item::Fsm(f2) if f2.name.name == inst.module_name.name => {
                        Some(f2.ports.as_slice())
                    }
                    Item::Pipeline(p2) if p2.name.name == inst.module_name.name => {
                        Some(p2.ports.as_slice())
                    }
                    _ => None,
                });
            if let Some(ports) = child_ports {
                let connected: std::collections::HashSet<&str> = inst
                    .connections
                    .iter()
                    .map(|c| c.port_name.name.as_str())
                    .collect();
                for port in ports {
                    // Skip Clock and Reset ports — they may be handled via domain defaults
                    let is_infra = matches!(&port.ty, TypeExpr::Clock(_) | TypeExpr::Reset(_, _));
                    if is_infra {
                        continue;
                    }
                    let name = port.name.name.as_str();
                    let is_connected = if port.bus_info.is_some() {
                        // Accept whole-bus OR per-field bindings.
                        let prefix = format!("{}_", name);
                        connected.contains(name) || connected.iter().any(|c| c.starts_with(&prefix))
                    } else {
                        connected.contains(name)
                    };
                    if !is_connected {
                        if port.direction == Direction::In {
                            self.errors.push(CompileError::general(
                                &format!(
                                    "input port `{}` of `{}` is not connected in inst `{}`",
                                    name, inst.module_name.name, inst.name.name
                                ),
                                inst.span,
                            ));
                        } else {
                            self.warnings.push(CompileWarning {
                                message: format!(
                                    "output port `{}` of `{}` is not connected in inst `{}`",
                                    name, inst.module_name.name, inst.name.name
                                ),
                                span: inst.span,
                            });
                        }
                    }
                }
            }
        }

        // Base names of the child's Vec-of-bus ports (`port mm: ...
        // Vec<Bus, N>`). The parser flattens a per-element connection
        // `mm[k] <- ...` into port_name `mm_<k>`, so to credit the right
        // parent driver below we must recognise `mm_<k>` as element k of
        // the child's `mm` port. We only need to know which child ports are
        // Vec-of-bus (count present) — not the concrete N — so this avoids
        // resolving a possibly param-dependent count here.
        let child_vob_bases: Vec<String> = self
            .source
            .items
            .iter()
            .find_map(|item| match item {
                Item::Module(m2) if m2.name.name == inst.module_name.name => {
                    Some(m2.ports.as_slice())
                }
                Item::Fsm(f2) if f2.name.name == inst.module_name.name => Some(f2.ports.as_slice()),
                Item::Pipeline(p2) if p2.name.name == inst.module_name.name => {
                    Some(p2.ports.as_slice())
                }
                _ => None,
            })
            .map(|ports| {
                ports
                    .iter()
                    .filter(|p| p.bus_info.as_ref().map_or(false, |bi| bi.count.is_some()))
                    .map(|p| p.name.name.clone())
                    .collect()
            })
            .unwrap_or_default();

        // Mark connected output ports as driven
        for conn in &inst.connections {
            // Resolve the child bus port this connection targets. Whole-bus
            // (`mm <- ...`) keeps the port name verbatim; per-element
            // (`mm[k] <- ...`, flattened to `mm_<k>`) strips the trailing
            // `_<idx>` back to the Vec-of-bus base `mm`.
            let conn_port_base: String = child_vob_bases
                .iter()
                .find_map(|base| {
                    let rest = conn.port_name.name.strip_prefix(&format!("{base}_"))?;
                    rest.parse::<u32>().ok().map(|_| base.clone())
                })
                .unwrap_or_else(|| conn.port_name.name.clone());
            if conn.direction == ConnectDir::Output {
                if let ExprKind::Ident(name) = &conn.signal.kind {
                    driven.insert(name.clone());
                }
                // Bus port FieldAccess: itcm.cmd_valid → driven itcm_cmd_valid
                let flat = Self::expr_flat_name_tc(&conn.signal);
                if !flat.is_empty() {
                    driven.insert(flat);
                }
                // Packed Vec-of-bus port-element drive:
                // `Index(Ident("<base>_<sig>"), <i>)` from the thread-wrapper
                // emission credits the per-element flat name `<base>_<i>_<sig>`
                // that the undriven-port check looks for. Detect when the
                // base ident matches a `<vobport>_<sig>` pair.
                if let ExprKind::Index(arr, idx) = &conn.signal.kind {
                    if let (ExprKind::Ident(arr_name), ExprKind::Literal(LitKind::Dec(i))) =
                        (&arr.kind, &idx.kind)
                    {
                        // Try every Vec-of-bus port name as a prefix.
                        for (vobport, _n) in self.vec_of_bus_ports.iter() {
                            let prefix = format!("{vobport}_");
                            if let Some(sig) = arr_name.strip_prefix(&prefix) {
                                driven.insert(format!("{vobport}_{i}_{sig}"));
                            }
                        }
                    }
                }
            }
            // Whole-bus connection: axi_rd -> m_axi_mm2s expands to N signals.
            // The inst's bus port drives/receives signals based on its perspective.
            // We need to mark parent signals as "driven" when the inst OUTPUTS them.
            if let Some((_, bus_name)) = target_bus_ports
                .iter()
                .find(|(pn, _)| *pn == conn_port_base)
            {
                if let Some((crate::resolve::Symbol::Bus(info), _)) =
                    self.symbols.globals.get(bus_name)
                {
                    // Find the inst's bus port perspective, params, and Vec count.
                    let inst_bus_info = self
                        .source
                        .items
                        .iter()
                        .find_map(|item| match item {
                            Item::Module(m2) if m2.name.name == inst.module_name.name => {
                                Some(m2.ports.as_slice())
                            }
                            Item::Fsm(f2) if f2.name.name == inst.module_name.name => {
                                Some(f2.ports.as_slice())
                            }
                            _ => None,
                        })
                        .and_then(|ports| {
                            ports
                                .iter()
                                .find(|p| p.name.name == conn_port_base)
                                .and_then(|p| p.bus_info.as_ref())
                        });
                    let inst_perspective = inst_bus_info.map(|bi| bi.perspective);
                    // Inst port's Vec count (Some(N) means `port: ... Vec<Bus, N>`).
                    let inst_vec_count: Option<u32> = inst_bus_info
                        .and_then(|bi| bi.count.as_ref())
                        .and_then(|ce| {
                            let empty: HashMap<String, Ty> = HashMap::new();
                            self.eval_const_expr(ce, &empty).map(|v| v as u32)
                        });

                    // Resolve the parent-side base prefix(es):
                    //   * `Ident("w")`           → `w`            (scalar bus wire/port)
                    //                              or when inst port is Vec<Bus,N> and
                    //                              `w` is also Vec<Bus,N>: expand to
                    //                              `w_0`, `w_1`, ..., `w_{N-1}`.
                    //   * `Index(Ident("v"), i)` → `v_<i>`        (Vec-of-bus element)
                    let sig_bases: Vec<String> = match &conn.signal.kind {
                        ExprKind::Ident(n) => {
                            // Whole-Vec forwarding: child port `m: ... Vec<Bus,N>`
                            // wired to parent ident `n` that is itself a Vec-of-Bus
                            // port. Expand to N per-element prefixes so the
                            // undriven-port check sees `n_0_<sig>`, `n_1_<sig>`, ...
                            // (which is what the prefixes loop at the module-level
                            // undriven check produces for a Vec<Bus,N> parent port).
                            if let Some(n_count) = inst_vec_count {
                                if self.vec_of_bus_ports.get(n).copied() == Some(n_count) {
                                    (0..n_count).map(|i| format!("{}_{}", n, i)).collect()
                                } else {
                                    vec![n.clone()]
                                }
                            } else {
                                vec![n.clone()]
                            }
                        }
                        ExprKind::Index(arr, idx) => {
                            if let (ExprKind::Ident(arr_name), ExprKind::Literal(LitKind::Dec(i))) =
                                (&arr.kind, &idx.kind)
                            {
                                vec![format!("{}_{}", arr_name, i)]
                            } else {
                                Vec::new()
                            }
                        }
                        _ => Vec::new(),
                    };
                    if !sig_bases.is_empty() {
                        let mut pm = info.default_param_map();
                        if let Some(bi) = inst_bus_info {
                            for pa in &bi.params {
                                pm.insert(pa.name.name.clone(), &pa.value);
                            }
                        }
                        let eff = info.effective_signals(&pm);
                        for sig_base in &sig_bases {
                            for (sname, sdir, _) in &eff {
                                // Determine actual direction from inst's perspective
                                let inst_dir = match inst_perspective {
                                    Some(BusPerspective::Initiator) => *sdir,
                                    Some(BusPerspective::Target) => (*sdir).flip(),
                                    None => *sdir,
                                };
                                // If signal is an output FROM the inst, it drives the parent wire/port
                                if inst_dir == Direction::Out {
                                    driven.insert(format!("{}_{}", sig_base, sname));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    /// Compile-time lint: if a module has a bus port whose bus declares one or
    /// more `handshake` channels, every read of a payload signal inside
    /// comb/seq/latch blocks should sit under an `if <port>.<valid>` (or the
    /// variant's `<port>.<req>`) conditional.
    ///
    /// v1 scope:
    /// - Recognized guards: the exact valid/req field access (`port.ch_valid`),
    ///   either directly as the if-condition or as an AND-conjunct of it.
    /// - Does NOT trace let-bindings: `let g = port.ch_valid; if g ...` is a
    ///   known false-positive and documented in the plan. If this becomes
    ///   noisy, extend by resolving single-ident let RHS before matching.
    /// - Variants with no valid signal (`ready_only`) are skipped entirely;
    ///   `req_ack_2phase` uses the pending-transfer guard (`req != ack`).
    pub(crate) fn check_handshake_reads(&mut self, m: &ModuleDecl) {
        use std::collections::HashMap as Map;
        // port_name -> Vec<(channel_name, guard, payload_field_names)>
        let mut info: Map<String, Vec<(String, HandshakePayloadGuard, Vec<String>)>> = Map::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else {
                continue;
            };
            let Some(crate::resolve::Symbol::Bus(binfo)) =
                self.symbols.globals.get(&bi.bus_name.name).map(|(s, _)| s)
            else {
                continue;
            };
            for hs in &binfo.handshakes {
                let guard = match hs.variant.name.as_str() {
                    "valid_ready" | "valid_only" | "valid_stall" => {
                        HandshakePayloadGuard::Field(format!("{}_valid", hs.name.name))
                    }
                    "req_ack_4phase" => {
                        HandshakePayloadGuard::Field(format!("{}_req", hs.name.name))
                    }
                    "req_ack_2phase" => HandshakePayloadGuard::ReqAck2PhasePending {
                        req_field: format!("{}_req", hs.name.name),
                        ack_field: format!("{}_ack", hs.name.name),
                    },
                    _ => continue,
                };
                let payloads: Vec<String> =
                    hs.payload_names.iter().map(|i| i.name.clone()).collect();
                info.entry(p.name.name.clone()).or_default().push((
                    hs.name.name.clone(),
                    guard,
                    payloads
                        .into_iter()
                        .map(|n| format!("{}_{}", hs.name.name, n))
                        .collect(),
                ));
            }
        }
        if info.is_empty() {
            return;
        }

        for item in &m.body {
            match item {
                ModuleBodyItem::CombBlock(cb) => {
                    for s in &cb.stmts {
                        self.walk_comb_for_hs_reads(s, &[], &info);
                    }
                }
                ModuleBodyItem::RegBlock(rb) => {
                    for s in &rb.stmts {
                        self.walk_seq_for_hs_reads(s, &[], &info);
                    }
                }
                ModuleBodyItem::LatchBlock(lb) => {
                    for s in &lb.stmts {
                        self.walk_seq_for_hs_reads(s, &[], &info);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_comb_for_hs_reads(
        &mut self,
        stmt: &Stmt,
        enclosing: &[&Expr],
        info: &std::collections::HashMap<String, Vec<(String, HandshakePayloadGuard, Vec<String>)>>,
    ) {
        match stmt {
            Stmt::Assign(a) => {
                self.check_expr_for_unguarded_payload(&a.value, enclosing, info, a.span);
            }
            Stmt::IfElse(ie) => {
                // Expressions inside the condition itself don't get the
                // condition as a guard — they're evaluated before the branch.
                self.check_expr_for_unguarded_payload(&ie.cond, enclosing, info, ie.span);
                let mut then_stack: Vec<&Expr> = enclosing.to_vec();
                then_stack.push(&ie.cond);
                for s in &ie.then_stmts {
                    self.walk_comb_for_hs_reads(s, &then_stack, info);
                }
                // Else branch does NOT add the condition (would need negation logic).
                for s in &ie.else_stmts {
                    self.walk_comb_for_hs_reads(s, enclosing, info);
                }
            }
            Stmt::Match(mm) => {
                self.check_expr_for_unguarded_payload(&mm.scrutinee, enclosing, info, mm.span);
                for arm in &mm.arms {
                    for s in &arm.body {
                        self.walk_comb_for_hs_reads(s, enclosing, info);
                    }
                }
            }
            Stmt::For(fl) => {
                for s in &fl.body {
                    self.walk_comb_for_hs_reads(s, enclosing, info);
                }
            }
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("seq-only Stmt variant inside comb-context walker")
            }
            Stmt::Log(_) => {}
        }
    }

    fn walk_seq_for_hs_reads(
        &mut self,
        stmt: &Stmt,
        enclosing: &[&Expr],
        info: &std::collections::HashMap<String, Vec<(String, HandshakePayloadGuard, Vec<String>)>>,
    ) {
        match stmt {
            Stmt::Assign(a) => {
                self.check_expr_for_unguarded_payload(&a.value, enclosing, info, a.span);
            }
            Stmt::IfElse(ie) => {
                self.check_expr_for_unguarded_payload(&ie.cond, enclosing, info, ie.span);
                let mut then_stack: Vec<&Expr> = enclosing.to_vec();
                then_stack.push(&ie.cond);
                for s in &ie.then_stmts {
                    self.walk_seq_for_hs_reads(s, &then_stack, info);
                }
                for s in &ie.else_stmts {
                    self.walk_seq_for_hs_reads(s, enclosing, info);
                }
            }
            Stmt::Match(mm) => {
                self.check_expr_for_unguarded_payload(&mm.scrutinee, enclosing, info, mm.span);
                for arm in &mm.arms {
                    for s in &arm.body {
                        self.walk_seq_for_hs_reads(s, enclosing, info);
                    }
                }
            }
            Stmt::For(fl) => {
                for s in &fl.body {
                    self.walk_seq_for_hs_reads(s, enclosing, info);
                }
            }
            Stmt::Init(ib) => {
                for s in &ib.body {
                    self.walk_seq_for_hs_reads(s, enclosing, info);
                }
            }
            Stmt::DoUntil { body, cond, span } => {
                self.check_expr_for_unguarded_payload(cond, enclosing, info, *span);
                for s in body {
                    self.walk_seq_for_hs_reads(s, enclosing, info);
                }
            }
            Stmt::WaitUntil(e, sp) => {
                self.check_expr_for_unguarded_payload(e, enclosing, info, *sp);
            }
            Stmt::Log(_) => {}
        }
    }

    /// Scan `expr` for reads of `<port>.<payload_field>` where the (port,field)
    /// pair is known to be a handshake payload. If no enclosing condition
    /// guards the access, emit a warning.
    pub(crate) fn check_expr_for_unguarded_payload(
        &mut self,
        expr: &Expr,
        enclosing: &[&Expr],
        info: &std::collections::HashMap<String, Vec<(String, HandshakePayloadGuard, Vec<String>)>>,
        default_span: Span,
    ) {
        match &expr.kind {
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(port) = &base.kind {
                    if let Some(channels) = info.get(port) {
                        for (ch_name, guard, payload_fields) in channels {
                            if payload_fields.iter().any(|pf| pf == &field.name) {
                                let needs_guard = guard.display(port);
                                if !enclosing
                                    .iter()
                                    .any(|c| cond_contains_guard(c, port, guard))
                                {
                                    let span = if expr.span.start == 0 && expr.span.end == 0 {
                                        default_span
                                    } else {
                                        expr.span
                                    };
                                    self.warnings.push(CompileWarning {
                                        message: format!(
                                            "handshake payload `{}.{}` (channel `{}`) is read outside an `if {}` guard — consumer may observe stale/undefined data. Guard the read: `if {} ...`",
                                            port, field.name, ch_name, needs_guard, needs_guard
                                        ),
                                        span,
                                    });
                                }
                            }
                        }
                    }
                }
                self.check_expr_for_unguarded_payload(base, enclosing, info, default_span);
            }
            ExprKind::Binary(op, l, r) => {
                self.check_expr_for_unguarded_payload(l, enclosing, info, default_span);
                // Short-circuit `and` / `&`: the right-hand side only evaluates
                // when the left-hand side is true, so `l` acts as an enclosing
                // guard while we check `r`. Matches the recursive walk in
                // `cond_contains_guard` which also treats And/BitAnd as
                // conjunction. `Or` does not short-circuit this way (either
                // side can be the deciding operand), so leave it alone.
                if matches!(op, BinOp::And | BinOp::BitAnd) {
                    let mut extended: Vec<&Expr> = enclosing.to_vec();
                    extended.push(l);
                    self.check_expr_for_unguarded_payload(r, &extended, info, default_span);
                } else {
                    self.check_expr_for_unguarded_payload(r, enclosing, info, default_span);
                }
            }
            ExprKind::Unary(_, e) => {
                self.check_expr_for_unguarded_payload(e, enclosing, info, default_span);
            }
            ExprKind::Index(b, i) => {
                self.check_expr_for_unguarded_payload(b, enclosing, info, default_span);
                self.check_expr_for_unguarded_payload(i, enclosing, info, default_span);
            }
            ExprKind::BitSlice(b, hi, lo) => {
                self.check_expr_for_unguarded_payload(b, enclosing, info, default_span);
                self.check_expr_for_unguarded_payload(hi, enclosing, info, default_span);
                self.check_expr_for_unguarded_payload(lo, enclosing, info, default_span);
            }
            ExprKind::PartSelect(b, s, w, _) => {
                self.check_expr_for_unguarded_payload(b, enclosing, info, default_span);
                self.check_expr_for_unguarded_payload(s, enclosing, info, default_span);
                self.check_expr_for_unguarded_payload(w, enclosing, info, default_span);
            }
            ExprKind::MethodCall(b, _, args) => {
                self.check_expr_for_unguarded_payload(b, enclosing, info, default_span);
                for a in args {
                    self.check_expr_for_unguarded_payload(a, enclosing, info, default_span);
                }
            }
            ExprKind::FunctionCall(_, args) => {
                for a in args {
                    self.check_expr_for_unguarded_payload(a, enclosing, info, default_span);
                }
            }
            ExprKind::Ternary(c, t, e) => {
                self.check_expr_for_unguarded_payload(c, enclosing, info, default_span);
                // Then-branch only evaluates when `c` is true → treat `c` as
                // an enclosing guard. Else-branch stays un-augmented because
                // we can't easily synthesize `!c` as a condition the existing
                // `cond_contains_guard` walker understands (it only accepts
                // positive AND-conjunctions of guard fields).
                let mut then_encl: Vec<&Expr> = enclosing.to_vec();
                then_encl.push(c);
                self.check_expr_for_unguarded_payload(t, &then_encl, info, default_span);
                self.check_expr_for_unguarded_payload(e, enclosing, info, default_span);
            }
            ExprKind::ExprMatch(s, arms) => {
                self.check_expr_for_unguarded_payload(s, enclosing, info, default_span);
                for arm in arms {
                    self.check_expr_for_unguarded_payload(
                        &arm.value,
                        enclosing,
                        info,
                        default_span,
                    );
                }
            }
            _ => {}
        }
    }

    pub(crate) fn check_implements(&mut self, m: &ModuleDecl, tmpl_name: &Ident) {
        // Find the template in the source file
        let tmpl = self.source.items.iter().find_map(|item| {
            if let Item::Template(t) = item {
                if t.name.name == tmpl_name.name {
                    Some(t)
                } else {
                    None
                }
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
                    &format!(
                        "module `{}` is missing param `{}` required by template `{}`",
                        m.name.name, tp.name.name, tmpl.name.name
                    ),
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
                        &format!(
                            "module `{}` is missing port `{}` required by template `{}`",
                            m.name.name, tp.name.name, tmpl.name.name
                        ),
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
                    &format!(
                        "module `{}` is missing hook `{}` required by template `{}`",
                        m.name.name, th.name.name, tmpl.name.name
                    ),
                    m.name.span,
                ));
            }
        }
    }

    /// Warn when **deprecated** `port reg` outputs are assigned inside
    /// state-dependent if/elsif chains in seq blocks. The motivation is the
    /// implicit 1-cycle latency the legacy `port reg` form hides — a common
    /// timing mismatch with testbench models that expect same-cycle outputs.
    ///
    /// Filter is `ri.legacy_port_reg` (not just `reg_info.is_some()`) so that:
    ///   * user-written `port: out pipe_reg<T, N>` is silent — the user has
    ///     explicitly opted into the N-cycle latency by writing the type, so
    ///     the foot-gun the warning was designed to catch doesn't apply;
    ///   * thread-lowering synthesizes port-regs with `legacy_port_reg: false`
    ///     (see `src/elaborate.rs::lower_threads`), so internal artifacts the
    ///     user can't act on are silent automatically — no separate
    ///     `synthesized` flag needed.
    pub(crate) fn check_port_reg_timing(&mut self, m: &ModuleDecl) {
        // Collect names of *legacy-form* registered output ports only.
        let port_reg_names: HashSet<String> = m
            .ports
            .iter()
            .filter(|p| {
                p.direction == Direction::Out
                    && p.reg_info.as_ref().is_some_and(|ri| ri.legacy_port_reg)
            })
            .map(|p| p.name.name.clone())
            .collect();
        if port_reg_names.is_empty() {
            return;
        }

        // Collect internal register names (potential state variables)
        let internal_reg_names: HashSet<String> = m
            .body
            .iter()
            .filter_map(|item| {
                if let ModuleBodyItem::RegDecl(r) = item {
                    Some(r.name.name.clone())
                } else {
                    None
                }
            })
            .collect();

        // Collect registered output spans for warning locations.
        let port_reg_spans: HashMap<String, Span> = m
            .ports
            .iter()
            .filter(|p| {
                p.direction == Direction::Out
                    && p.reg_info.as_ref().is_some_and(|ri| ri.legacy_port_reg)
            })
            .map(|p| (p.name.name.clone(), p.span))
            .collect();

        // Scan seq blocks for state-dependent registered-output assignments.
        let mut warned: HashSet<String> = HashSet::new();
        for item in &m.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                for stmt in &rb.stmts {
                    self.find_state_dependent_port_reg_assigns(
                        stmt,
                        &port_reg_names,
                        &internal_reg_names,
                        &port_reg_spans,
                        &mut warned,
                        false,
                    );
                }
            }
        }
    }

    /// Recursively scan a seq statement for registered-output assignments inside
    /// state-dependent if/elsif chains.
    fn find_state_dependent_port_reg_assigns(
        &mut self,
        stmt: &Stmt,
        port_reg_names: &HashSet<String>,
        reg_names: &HashSet<String>,
        port_reg_spans: &HashMap<String, Span>,
        warned: &mut HashSet<String>,
        inside_state_if: bool,
    ) {
        match stmt {
            Stmt::IfElse(ie) => {
                // Check if this condition tests a register (state variable)
                let cond_tests_reg = Self::expr_references_any(&ie.cond, reg_names);
                let in_state = inside_state_if || cond_tests_reg;

                // Check assignments in then/else branches
                for s in &ie.then_stmts {
                    if in_state {
                        if let Stmt::Assign(ra) = s {
                            let target = Self::expr_root_name_tc(&ra.target);
                            if port_reg_names.contains(&target) && !warned.contains(&target) {
                                warned.insert(target.clone());
                                if let Some(&span) = port_reg_spans.get(&target) {
                                    self.warnings.push(CompileWarning {
                                        message: format!(
                                            "`{target}` is a deprecated `port reg` output assigned inside a state-dependent \
                                             branch — the implicit 1-cycle latency makes the output appear one cycle after \
                                             the state that drives it. Migrate to either `port {target}: out T` driven by \
                                             `comb` (same-cycle output) or `port {target}: out pipe_reg<T, N>` (explicit \
                                             N-cycle registered output)"
                                        ),
                                        span,
                                    });
                                }
                            }
                        }
                    }
                    self.find_state_dependent_port_reg_assigns(
                        s,
                        port_reg_names,
                        reg_names,
                        port_reg_spans,
                        warned,
                        in_state,
                    );
                }
                for s in &ie.else_stmts {
                    self.find_state_dependent_port_reg_assigns(
                        s,
                        port_reg_names,
                        reg_names,
                        port_reg_spans,
                        warned,
                        in_state,
                    );
                }
            }
            Stmt::Match(ms) => {
                let cond_tests_reg = Self::expr_references_any(&ms.scrutinee, reg_names);
                let in_state = inside_state_if || cond_tests_reg;
                for arm in &ms.arms {
                    for s in &arm.body {
                        self.find_state_dependent_port_reg_assigns(
                            s,
                            port_reg_names,
                            reg_names,
                            port_reg_spans,
                            warned,
                            in_state,
                        );
                    }
                }
            }
            Stmt::For(fl) => {
                for s in &fl.body {
                    self.find_state_dependent_port_reg_assigns(
                        s,
                        port_reg_names,
                        reg_names,
                        port_reg_spans,
                        warned,
                        inside_state_if,
                    );
                }
            }
            _ => {}
        }
    }

    /// Check if an expression references any name in the given set.
    fn expr_references_any(expr: &Expr, names: &HashSet<String>) -> bool {
        match &expr.kind {
            ExprKind::Ident(name) => names.contains(name.as_str()),
            ExprKind::Binary(_, l, r) => {
                Self::expr_references_any(l, names) || Self::expr_references_any(r, names)
            }
            ExprKind::Unary(_, inner) => Self::expr_references_any(inner, names),
            ExprKind::Index(base, idx) => {
                Self::expr_references_any(base, names) || Self::expr_references_any(idx, names)
            }
            ExprKind::MethodCall(base, _, _) => Self::expr_references_any(base, names),
            _ => false,
        }
    }

    /// Validate that every `reg ... guard <sig>` and `port reg ... guard <sig>`
    /// annotation references a signal that:
    ///  (a) exists in scope (module ports, regs, wires, or let bindings), and
    ///  (b) resolves to a Bool type.
    /// Reports `CompileError::general` with the offending identifier's span.
    pub(crate) fn check_guards(&mut self, m: &ModuleDecl) {
        // Build name → TypeExpr map for all in-scope signals
        let mut sig_types: HashMap<String, TypeExpr> = HashMap::new();
        for p in &m.ports {
            if p.bus_info.is_some() {
                continue;
            }
            sig_types.insert(p.name.name.clone(), p.ty.clone());
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    sig_types.insert(r.name.name.clone(), r.ty.clone());
                }
                ModuleBodyItem::WireDecl(w) => {
                    sig_types.insert(w.name.name.clone(), w.ty.clone());
                }
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(ty) = &l.ty {
                        sig_types.insert(l.name.name.clone(), ty.clone());
                    }
                }
                _ => {}
            }
        }

        // Helper: validate a single guard annotation
        let check_one = |errors: &mut Vec<CompileError>, guard: &Ident, owner: &str| {
            match sig_types.get(&guard.name) {
                Some(TypeExpr::Bool) => {} // OK
                Some(other) => {
                    let ty_str = match other {
                        TypeExpr::UInt(_) => "UInt",
                        TypeExpr::SInt(_) => "SInt",
                        TypeExpr::Bit => "Bit",
                        TypeExpr::Clock(_) => "Clock",
                        TypeExpr::Reset(..) => "Reset",
                        TypeExpr::Vec(..) => "Vec",
                        TypeExpr::Named(n) => &n.name,
                        _ => "<other>",
                    };
                    errors.push(CompileError::general(
                        &format!(
                            "guard signal `{}` for `{}` must be Bool, found {}",
                            guard.name, owner, ty_str
                        ),
                        guard.span,
                    ));
                }
                None => errors.push(CompileError::general(
                    &format!(
                        "guard signal `{}` for `{}` not found in module scope",
                        guard.name, owner
                    ),
                    guard.span,
                )),
            }
        };

        for p in &m.ports {
            if let Some(ri) = &p.reg_info {
                if let Some(ref g) = ri.guard {
                    check_one(&mut self.errors, g, &p.name.name);
                }
            }
        }
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let Some(ref g) = r.guard {
                    check_one(&mut self.errors, g, &r.name.name);
                }
            }
        }
    }

    /// Validate that all registers with reset assigned in an `always on` block
    /// agree on reset signal name, sync/async kind, and polarity.
    pub(crate) fn check_always_block_reset_consistency(&mut self, rb: &RegBlock, m: &ModuleDecl) {
        // Collect assigned register root names
        let mut assigned = std::collections::BTreeSet::new();
        Self::collect_assigned_roots_tc(&rb.stmts, &mut assigned);

        // Gather reg declarations for assigned registers
        let reg_decls: Vec<&RegDecl> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::RegDecl(r) = i {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();

        // Resolved reset info: (signal_name, kind, level)
        struct ResetProps {
            signal: String,
            kind: ResetKind,
            level: ResetLevel,
        }

        let mut first_reset: Option<ResetProps> = None;

        for name in &assigned {
            if name.is_empty() {
                continue;
            }
            let rd = match reg_decls.iter().find(|r| r.name.name == *name) {
                Some(rd) => rd,
                None => continue,
            };

            let (signal, kind, level) = match &rd.reset {
                RegReset::None => continue,
                RegReset::Explicit(sig, k, l, _) => (sig.name.clone(), *k, *l),
                RegReset::Inherit(sig, _) => {
                    // Look up port to resolve kind and level
                    if let Some(port) = m.ports.iter().find(|p| p.name.name == sig.name) {
                        if let TypeExpr::Reset(k, l) = &port.ty {
                            (sig.name.clone(), *k, *l)
                        } else {
                            self.errors.push(CompileError::general(
                                &format!(
                                    "`{}` reset signal `{}` is not a Reset port",
                                    name, sig.name
                                ),
                                sig.span,
                            ));
                            continue;
                        }
                    } else {
                        self.errors.push(CompileError::general(
                            &format!(
                                "`{}` reset signal `{}` not found in module ports",
                                name, sig.name
                            ),
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
                first_reset = Some(ResetProps {
                    signal,
                    kind,
                    level,
                });
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
                Stmt::For(f) => {
                    Self::collect_assigned_roots_tc(&f.body, out);
                }
                Stmt::Init(ib) => {
                    Self::collect_assigned_roots_tc(&ib.body, out);
                }
                Stmt::WaitUntil(_, _) => {}
                Stmt::DoUntil { body, .. } => {
                    Self::collect_assigned_roots_tc(body, out);
                }
            }
        }
    }

    fn expr_root_name_tc(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::FieldAccess(base, _) => Self::expr_root_name_tc(base),
            ExprKind::Index(base, _)
            | ExprKind::BitSlice(base, _, _)
            | ExprKind::PartSelect(base, _, _, _) => Self::expr_root_name_tc(base),
            ExprKind::LatencyAt(inner, _) | ExprKind::SvaNext(_, inner) => {
                Self::expr_root_name_tc(inner)
            }
            _ => String::new(),
        }
    }

    /// Like expr_root_name_tc but returns the flattened name for single-level FieldAccess
    /// (e.g. `itcm.cmd_valid` → `"itcm_cmd_valid"`). Used for bus port driven tracking.
    fn expr_flat_name_tc(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::LatencyAt(inner, _) | ExprKind::SvaNext(_, inner) => {
                Self::expr_flat_name_tc(inner)
            }
            ExprKind::Ident(n) => n.clone(),
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    format!("{}_{}", base_name, field.name)
                // Indexed bus: m_axi[0].valid → m_axi_0_valid
                } else if let ExprKind::Index(arr, idx) = &base.kind {
                    if let (ExprKind::Ident(arr_name), ExprKind::Literal(LitKind::Dec(i))) =
                        (&arr.kind, &idx.kind)
                    {
                        format!("{}_{}_{}", arr_name, i, field.name)
                    } else {
                        Self::expr_root_name_tc(base)
                    }
                } else {
                    Self::expr_root_name_tc(base)
                }
            }
            ExprKind::Index(base, _)
            | ExprKind::BitSlice(base, _, _)
            | ExprKind::PartSelect(base, _, _, _) => Self::expr_flat_name_tc(base),
            _ => String::new(),
        }
    }

    /// Emit an error when the RHS is wider than the LHS register/port.
    /// Compute total bit width of a type, resolving structs via symbol table.
    fn type_total_width(&self, ty: &Ty) -> Option<u32> {
        match ty {
            Ty::UInt(w) | Ty::SInt(w) => Some(*w),
            Ty::Bool | Ty::Clock(_) | Ty::Reset(_, _) => Some(1),
            Ty::FP32 => Some(32),
            Ty::BF16 => Some(16),
            Ty::Enum(_, w) => Some(*w),
            Ty::Vec(inner, count) => self.type_total_width(inner).map(|w| w * count),
            Ty::Struct(name) => {
                if let Some((crate::resolve::Symbol::Struct(info), _)) =
                    self.symbols.globals.get(name)
                {
                    let mut total = 0u32;
                    for (_, field_ty) in &info.fields {
                        let w = self.type_expr_width(field_ty)?;
                        total += w;
                    }
                    Some(total)
                } else {
                    None
                }
            }
            Ty::Bus(_) => None, // bus is not a single-width type
            Ty::Todo | Ty::Error => None,
        }
    }

    /// Compute bit width directly from a TypeExpr without needing &mut self.
    fn type_expr_width(&self, ty: &TypeExpr) -> Option<u32> {
        match ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_type_width_expr(w),
            TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => Some(1),
            TypeExpr::FP32 => Some(32),
            TypeExpr::BF16 => Some(16),
            TypeExpr::Vec(inner, size) => {
                let iw = self.type_expr_width(inner)?;
                let n = eval_type_width_expr(size)?;
                Some(iw * n)
            }
            TypeExpr::Named(ident) => {
                if let Some((crate::resolve::Symbol::Struct(info), _)) =
                    self.symbols.globals.get(&ident.name)
                {
                    let mut total = 0u32;
                    for (_, field_ty) in &info.fields {
                        total += self.type_expr_width(field_ty)?;
                    }
                    Some(total)
                } else if let Some((crate::resolve::Symbol::Enum(info), _)) =
                    self.symbols.globals.get(&ident.name)
                {
                    Some(enum_width(info.variants.len()))
                } else {
                    None
                }
            }
        }
    }

    /// Warn when a seq block contains a top-level `if reset_signal` branch that
    /// is dead code because the declaration-level `reset signal=>value` already
    /// generates an outer reset guard wrapping the entire seq body.
    pub(crate) fn check_redundant_reset_branch(&mut self, rb: &RegBlock, m: &ModuleDecl) {
        // Collect all reset signal names used by regs (decl or port reg) assigned in this block.
        let mut assigned = std::collections::BTreeSet::new();
        Self::collect_assigned_roots_tc(&rb.stmts, &mut assigned);

        let mut reset_signals: std::collections::HashSet<String> = std::collections::HashSet::new();

        for name in &assigned {
            // Check RegDecl
            for item in &m.body {
                if let ModuleBodyItem::RegDecl(r) = item {
                    if r.name.name != *name {
                        continue;
                    }
                    let sig = match &r.reset {
                        RegReset::Inherit(sig, _) | RegReset::Explicit(sig, _, _, _) => {
                            Some(sig.name.clone())
                        }
                        RegReset::None => None,
                    };
                    if let Some(s) = sig {
                        reset_signals.insert(s);
                    }
                }
            }
            // Check port reg
            for p in &m.ports {
                if p.name.name != *name {
                    continue;
                }
                if let Some(ri) = &p.reg_info {
                    let sig = match &ri.reset {
                        RegReset::Inherit(sig, _) | RegReset::Explicit(sig, _, _, _) => {
                            Some(sig.name.clone())
                        }
                        RegReset::None => None,
                    };
                    if let Some(s) = sig {
                        reset_signals.insert(s);
                    }
                }
            }
        }

        if reset_signals.is_empty() {
            return;
        }

        // Check top-level stmts for `if reset_signal { ... }` or `if ~reset_signal { ... }`
        for stmt in &rb.stmts {
            if let Stmt::IfElse(ie) = stmt {
                let tested = match &ie.cond.kind {
                    ExprKind::Ident(id) => Some(id.clone()),
                    ExprKind::Unary(crate::ast::UnaryOp::Not, inner) => {
                        if let ExprKind::Ident(id) = &inner.kind {
                            Some(id.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(sig) = tested {
                    if reset_signals.contains(&sig) {
                        self.warnings.push(crate::diagnostics::CompileWarning {
                            message: format!(
                                "redundant reset branch: `if {}` in seq body is dead code — \
                                 the `reset {}=>...` declaration already generates an outer reset guard",
                                sig, sig
                            ),
                            span: ie.span,
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn check_width_compatible(
        &mut self,
        lhs_ty: &Ty,
        rhs_ty: &Ty,
        name: &str,
        span: Span,
    ) {
        // Floating-point: no implicit conversion. The target and RHS must be the
        // exact same float type; a float can't be assigned to/from a non-float
        // either. (Errors and todo! propagate silently.)
        if (lhs_ty.is_float() || rhs_ty.is_float())
            && !matches!(lhs_ty, Ty::Error | Ty::Todo)
            && !matches!(rhs_ty, Ty::Error | Ty::Todo)
            && lhs_ty != rhs_ty
        {
            self.errors.push(CompileError::general(
                &format!(
                    "type mismatch: `{name}` is {} but RHS is {} (no implicit float conversion; use .to_fp32()/.to_bf16()/.to_uint<N>()/.to_sint<N>())",
                    lhs_ty.display(), rhs_ty.display()
                ),
                span,
            ));
            return;
        }
        match (lhs_ty, rhs_ty) {
            (Ty::UInt(lw), Ty::UInt(rw)) if rw > lw => {
                let hint = if *rw == lw + 1 {
                    " (arithmetic widening)"
                } else {
                    ""
                };
                self.errors.push(CompileError::general(
                    &format!(
                        "width mismatch: `{name}` is UInt<{lw}> but RHS is UInt<{rw}>{hint}; \
                         use `.trunc<{lw}>()` to truncate explicitly"
                    ),
                    span,
                ));
            }
            (Ty::SInt(lw), Ty::SInt(rw)) if rw > lw => {
                let hint = if *rw == lw + 1 {
                    " (arithmetic widening)"
                } else {
                    ""
                };
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
    pub(crate) fn check_match_exhaustive(
        &mut self,
        scrutinee: &Expr,
        patterns: &[Pattern],
        span: Span,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) {
        // `_` (wildcard) must be the final arm, and may appear at most once.
        // Match arms resolve by priority (source order; `unique match` only
        // asserts mutual exclusivity), so `_` matches every remaining value and
        // any arm written after it is unreachable. A bare identifier arm
        // (`FOO =>`) is a constant comparison (lowered to `s == FOO`), not a
        // catch-all, so it is unrestricted. This applies to every match — enum
        // or integer/literal — so it runs before the enum-only logic below.
        // Both spellings of the wildcard (`Pattern::Wildcard` and the bare
        // identifier `_`) count, mirroring codegen's `default` lowering.
        let is_wildcard = |p: &Pattern| {
            matches!(p, Pattern::Wildcard) || matches!(p, Pattern::Ident(id) if id.name == "_")
        };
        let wildcard_idxs: Vec<usize> = patterns
            .iter()
            .enumerate()
            .filter(|(_, p)| is_wildcard(p))
            .map(|(i, _)| i)
            .collect();
        if wildcard_idxs.len() > 1 {
            self.errors.push(CompileError::general(
                "duplicate wildcard arm: a `match` may contain at most one `_`",
                span,
            ));
        } else if let Some(&first) = wildcard_idxs.first() {
            if first != patterns.len() - 1 {
                self.errors.push(CompileError::general(
                    "unreachable match arm: the wildcard `_` already matches every \
                     value — make `_` the last arm, or remove the arm(s) after it",
                    span,
                ));
            }
        }

        let scrutinee_ty = self.resolve_expr_type(scrutinee, module_name, local_types);
        let enum_name = match &scrutinee_ty {
            Ty::Enum(name, _) => name.clone(),
            _ => return, // only check enum matches
        };
        if patterns.iter().any(is_wildcard) {
            return; // wildcard covers everything
        }
        let covered: HashSet<String> = patterns
            .iter()
            .filter_map(|p| {
                if let Pattern::EnumVariant(_, variant) = p {
                    Some(variant.name.clone())
                } else {
                    None
                }
            })
            .collect();
        if let Some((Symbol::Enum(info), _)) = self.symbols.globals.get(&enum_name).cloned() {
            let missing: Vec<String> = info
                .variants
                .iter()
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

    pub(crate) fn check_reg_stmt(
        &mut self,
        stmt: &Stmt,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
        driven: &mut HashSet<String>,
    ) {
        let empty_regs: HashSet<String> = HashSet::new();
        self.check_stmt(
            stmt,
            module_name,
            local_types,
            driven,
            BlockKind::Seq,
            &empty_regs,
        );
    }

    /// Unified `Stmt` typecheck walker for both `comb` and `seq` (and seq's
    /// `init` sub-block) blocks. Phase 5b part 3: replaces the previously
    /// parallel `check_reg_stmt` and `check_comb_stmt` walkers. Behavior
    /// gated by `block_kind`:
    /// - `Comb`: rejects assignment targets that name a `reg`; tracks the
    ///   `driven` set with branch-aware merging across `if`/`else` so a
    ///   signal driven on one branch is not flagged as undriven on the
    ///   other; rejects `Init` / `WaitUntil` / `DoUntil` (defensive — the
    ///   parser already routes them to seq blocks only).
    /// - `Seq` / `PipelineStage`: drives the common width / shift / match
    ///   exhaustiveness / log / for-body / wait / do-until / init-block
    ///   checks. `WaitUntil` and `DoUntil` are not rejected by block-kind
    ///   here — that's `reject_wait_in_stmts`'s job at the block-level
    ///   call site (it has the context to allow them in pipeline stages
    ///   and reject them in plain seq).
    pub(crate) fn check_stmt(
        &mut self,
        stmt: &Stmt,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
        driven: &mut HashSet<String>,
        block_kind: BlockKind,
        reg_names: &HashSet<String>,
    ) {
        let in_comb = block_kind == BlockKind::Comb;
        match stmt {
            Stmt::Assign(a) => {
                let name = Self::expr_root_name_tc(&a.target);
                let target_name = if name.is_empty() {
                    format!("{:?}", a.target.kind)
                } else {
                    name.clone()
                };
                // Comb-only: `reg` targets must be assigned in seq, not comb.
                if in_comb && reg_names.contains(&target_name) {
                    self.errors.push(CompileError::general(
                        &format!(
                            "`{}` is a reg — assign it with `<=` in a `seq` block, not `=` in a `comb` block",
                            target_name
                        ),
                        a.span,
                    ));
                }
                if !target_name.is_empty() {
                    driven.insert(target_name.clone());
                }
                let flat = Self::expr_flat_name_tc(&a.target);
                if flat != target_name {
                    driven.insert(flat.clone());
                }
                // Vec-of-bus indexed write with a non-literal index — e.g.
                // `chans[i].sig = ...` inside a `for i in 0..N` loop. The
                // root name alone ("chans") doesn't satisfy the per-copy
                // completeness check, and the index isn't a literal so
                // `expr_flat_name_tc` couldn't pin it to a specific copy.
                // Conservatively mark every copy's matching signal driven
                // (the scalar Vec port case has the same "any indexed
                // write covers the whole array" semantics).
                if let ExprKind::FieldAccess(base, field) = &a.target.kind {
                    if let ExprKind::Index(arr, idx) = &base.kind {
                        if let ExprKind::Ident(arr_name) = &arr.kind {
                            let is_literal = matches!(
                                &idx.kind,
                                ExprKind::Literal(LitKind::Dec(_))
                                    | ExprKind::Literal(LitKind::Hex(_))
                                    | ExprKind::Literal(LitKind::Bin(_))
                                    | ExprKind::Literal(LitKind::Sized(..))
                            );
                            if !is_literal {
                                if let Some(&n) = self.vec_of_bus_ports.get(arr_name) {
                                    for i in 0..n {
                                        driven.insert(format!("{}_{}_{}", arr_name, i, field.name));
                                    }
                                }
                            }
                        }
                    }
                }

                let rhs_ty = self.resolve_expr_type(&a.value, module_name, local_types);
                self.check_pipelined_call_binding(a, in_comb);
                let is_indexed = !matches!(&a.target.kind, ExprKind::Ident(_));
                // LHS-type lookup: comb uses local_types directly so missing
                // entries silently skip the width check (matches historical
                // behavior); seq uses resolve_expr_type to handle Index /
                // BitSlice correctly.
                let lhs_ty: Option<Ty> = if in_comb {
                    if is_indexed {
                        None
                    } else {
                        local_types.get(&target_name).cloned()
                    }
                } else {
                    let t = self.resolve_expr_type(&a.target, module_name, local_types);
                    if t != Ty::Error && local_types.contains_key(&target_name) {
                        Some(t)
                    } else {
                        None
                    }
                };
                if let Some(lhs_ty) = lhs_ty {
                    self.check_width_compatible(&lhs_ty, &rhs_ty, &target_name, a.span);
                    if let (Some(lw), Some(rw)) = (lhs_ty.width(), rhs_ty.width()) {
                        if lw > rw && expr_is_shift(&a.value) {
                            self.errors.push(CompileError::general(
                                &format!(
                                    "shift result is UInt<{rw}> but target `{target_name}` is UInt<{lw}>; \
                                     shifts do not widen (IEEE §11.6.1). \
                                     To capture overflow, widen the operand first: `.zext<{lw}>() << n`"
                                ),
                                a.span,
                            ));
                        }
                    }
                }
            }
            Stmt::IfElse(ie) => {
                let _cond_ty = self.resolve_expr_type(&ie.cond, module_name, local_types);
                if in_comb {
                    // Branch-aware driven tracking: each branch sees a clone
                    // of driven; signals assigned in mutually-exclusive
                    // branches are not multi-driven. Merge after.
                    let mut then_driven = driven.clone();
                    for s in &ie.then_stmts {
                        self.check_stmt(
                            s,
                            module_name,
                            local_types,
                            &mut then_driven,
                            block_kind,
                            reg_names,
                        );
                    }
                    let mut else_driven = driven.clone();
                    for s in &ie.else_stmts {
                        self.check_stmt(
                            s,
                            module_name,
                            local_types,
                            &mut else_driven,
                            block_kind,
                            reg_names,
                        );
                    }
                    for nm in then_driven.iter().chain(else_driven.iter()) {
                        driven.insert(nm.clone());
                    }
                } else {
                    for s in &ie.then_stmts {
                        self.check_stmt(s, module_name, local_types, driven, block_kind, reg_names);
                    }
                    for s in &ie.else_stmts {
                        self.check_stmt(s, module_name, local_types, driven, block_kind, reg_names);
                    }
                }
            }
            Stmt::Match(m) => {
                let patterns: Vec<Pattern> = m.arms.iter().map(|a| a.pattern.clone()).collect();
                self.check_match_exhaustive(
                    &m.scrutinee,
                    &patterns,
                    m.span,
                    module_name,
                    local_types,
                );
                for arm in &m.arms {
                    for s in &arm.body {
                        self.check_stmt(s, module_name, local_types, driven, block_kind, reg_names);
                    }
                }
            }
            Stmt::Log(l) => {
                for arg in &l.args {
                    self.resolve_expr_type(arg, module_name, local_types);
                }
            }
            Stmt::For(f) => {
                for s in &f.body {
                    self.check_stmt(s, module_name, local_types, driven, block_kind, reg_names);
                }
            }
            Stmt::Init(ib) => {
                if in_comb {
                    self.errors.push(CompileError::general(
                        "`init on rst.asserted` is only valid inside a `seq` block, not `comb`",
                        ib.span,
                    ));
                    return;
                }
                let valid_reset = self
                    .source
                    .items
                    .iter()
                    .find_map(|item| {
                        if let Item::Module(m) = item {
                            if m.name.name == module_name {
                                return Some(m.ports.iter().any(|p| {
                                    p.name.name == ib.reset_signal.name
                                        && matches!(&p.ty, TypeExpr::Reset(_, _))
                                }));
                            }
                        }
                        None
                    })
                    .unwrap_or(false);
                if !valid_reset {
                    self.errors.push(CompileError::general(
                        &format!(
                            "`init on {}.asserted`: `{}` is not a Reset port in module `{}`",
                            ib.reset_signal.name, ib.reset_signal.name, module_name
                        ),
                        ib.reset_signal.span,
                    ));
                }
                for s in &ib.body {
                    self.check_stmt(s, module_name, local_types, driven, block_kind, reg_names);
                }
            }
            Stmt::WaitUntil(expr, span) => {
                if in_comb {
                    self.errors.push(CompileError::general(
                        "`wait until` is only valid inside a pipeline stage `seq` block, not `comb`",
                        *span,
                    ));
                    return;
                }
                let ty = self.resolve_expr_type(expr, module_name, local_types);
                if ty != Ty::Bool && ty != Ty::Error {
                    self.errors.push(CompileError::general(
                        &format!("wait until condition must be Bool, found {:?}", ty),
                        *span,
                    ));
                }
            }
            Stmt::DoUntil { body, cond, span } => {
                if in_comb {
                    self.errors.push(CompileError::general(
                        "`do..until` is only valid inside a pipeline stage `seq` block, not `comb`",
                        *span,
                    ));
                    return;
                }
                for s in body {
                    self.check_stmt(s, module_name, local_types, driven, block_kind, reg_names);
                }
                let ty = self.resolve_expr_type(cond, module_name, local_types);
                if ty != Ty::Bool && ty != Ty::Error {
                    self.errors.push(CompileError::general(
                        &format!("do-until condition must be Bool, found {:?}", ty),
                        *span,
                    ));
                }
            }
        }
    }

    /// Reject `wait until` / `do..until` in non-pipeline seq blocks.
    /// Reject `target <= expr;` (bare-ident LHS) inside a `for` loop in
    /// a seq block. Each iteration evaluates the RHS using the same
    /// pre-block value of every signal, then the last iteration's
    /// non-blocking schedule wins — so the loop never has the
    /// cumulative effect users expect (see also the SV antipattern
    /// `sum <= sum + data[i];` inside `for`).
    ///
    /// Indexed targets (`vec[i] <= ...`) write a different element
    /// each iteration and stay allowed — that's the canonical shift
    /// register pattern. Same for field-access targets like
    /// `bus.data <= ...` where the LHS varies per iteration.
    ///
    /// Recurses into nested `if/elsif/else`, `match`, and nested `for`
    /// (where the rule still applies). The `in_for` flag activates the
    /// rejection only when we're inside at least one for-loop.
    fn reject_bare_assign_in_for(stmt: &Stmt, in_for: bool, errors: &mut Vec<CompileError>) {
        match stmt {
            Stmt::Assign(a) => {
                if in_for && matches!(&a.target.kind, ExprKind::Ident(_)) {
                    errors.push(CompileError::general(
                        "non-blocking assignment `<=` to a bare identifier inside a `for` loop in seq has no cumulative effect — every iteration reads the same pre-block value of the target and only the last iteration's update commits. Compute the value combinationally in a `comb` block (which uses blocking `=` and accumulates correctly), then register it once with `<=` in seq. Indexed targets like `vec[i] <= ...` are fine because each iteration writes a different element.",
                        a.span,
                    ));
                }
            }
            Stmt::IfElse(ie) => {
                for s in &ie.then_stmts {
                    Self::reject_bare_assign_in_for(s, in_for, errors);
                }
                for s in &ie.else_stmts {
                    Self::reject_bare_assign_in_for(s, in_for, errors);
                }
            }
            Stmt::For(f) => {
                for s in &f.body {
                    Self::reject_bare_assign_in_for(s, true, errors);
                }
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    for s in &arm.body {
                        Self::reject_bare_assign_in_for(s, in_for, errors);
                    }
                }
            }
            Stmt::Init(ib) => {
                for s in &ib.body {
                    Self::reject_bare_assign_in_for(s, in_for, errors);
                }
            }
            Stmt::DoUntil { body, .. } => {
                for s in body {
                    Self::reject_bare_assign_in_for(s, in_for, errors);
                }
            }
            _ => {}
        }
    }

    fn reject_wait_in_stmts(stmts: &[Stmt], errors: &mut Vec<CompileError>) {
        for stmt in stmts {
            match stmt {
                Stmt::WaitUntil(_, span) => {
                    errors.push(CompileError::general(
                        "`wait until` is only valid inside pipeline stage `seq` blocks",
                        *span,
                    ));
                }
                Stmt::DoUntil { span, .. } => {
                    errors.push(CompileError::general(
                        "`do..until` is only valid inside pipeline stage `seq` blocks",
                        *span,
                    ));
                }
                Stmt::IfElse(ie) => {
                    Self::reject_wait_in_stmts(&ie.then_stmts, errors);
                    Self::reject_wait_in_stmts(&ie.else_stmts, errors);
                }
                Stmt::For(f) => {
                    Self::reject_wait_in_stmts(&f.body, errors);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn check_comb_stmt(
        &mut self,
        stmt: &Stmt,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
        driven: &mut HashSet<String>,
        reg_names: &HashSet<String>,
    ) {
        self.check_stmt(
            stmt,
            module_name,
            local_types,
            driven,
            BlockKind::Comb,
            reg_names,
        );
    }

    /// Check for latches: signals assigned on some but not all paths in a comb block.
    /// Returns (all_assigned, fully_assigned) for the statement list.
    fn comb_latch_targets(
        stmts: &[Stmt],
        symbols: &crate::resolve::SymbolTable,
    ) -> (HashSet<String>, HashSet<String>) {
        let mut all = HashSet::new();
        let mut full = HashSet::new();

        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    let name = Self::expr_flat_name_tc(&a.target);
                    if !name.is_empty() {
                        all.insert(name.clone());
                        full.insert(name);
                    }
                }
                Stmt::IfElse(ie) => {
                    let (then_all, then_full) = Self::comb_latch_targets(&ie.then_stmts, symbols);
                    let (else_all, else_full) = Self::comb_latch_targets(&ie.else_stmts, symbols);
                    all.extend(then_all);
                    all.extend(else_all);
                    // Const-true cond (e.g. desugared `port.ch.no_send()` /
                    // `.send(x)` wrappers): the then-branch is unconditional,
                    // promote its assigns to full regardless of an empty else.
                    let cond_is_true = matches!(&ie.cond.kind,
                        ExprKind::Literal(LitKind::Sized(_, n)) if *n != 0)
                        || matches!(&ie.cond.kind, ExprKind::Literal(LitKind::Dec(n)) if *n != 0);
                    if cond_is_true {
                        for name in &then_full {
                            full.insert(name.clone());
                        }
                    } else {
                        // A signal is fully assigned through an if/else only if
                        // assigned on BOTH branches.  No else = empty else_full.
                        for name in then_full.intersection(&else_full) {
                            full.insert(name.clone());
                        }
                    }
                }
                Stmt::Match(m) => {
                    let has_wildcard = m
                        .arms
                        .iter()
                        .any(|a| matches!(a.pattern, Pattern::Wildcard));
                    let arm_results: Vec<(HashSet<String>, HashSet<String>)> = m
                        .arms
                        .iter()
                        .map(|arm| {
                            // Comb match arm bodies are Vec<Stmt> — extract assign targets.
                            let mut arm_all = HashSet::new();
                            let mut arm_full = HashSet::new();
                            for s in &arm.body {
                                if let Stmt::Assign(a) = s {
                                    let name = Self::expr_flat_name_tc(&a.target);
                                    if !name.is_empty() {
                                        arm_all.insert(name.clone());
                                        arm_full.insert(name);
                                    }
                                }
                            }
                            (arm_all, arm_full)
                        })
                        .collect();
                    for (arm_all, _) in &arm_results {
                        all.extend(arm_all.iter().cloned());
                    }
                    // Check if match is exhaustive: wildcard, unique, or all enum variants covered
                    let mut is_exhaustive = has_wildcard || m.unique;
                    if !is_exhaustive {
                        // Check if all arms are EnumVariant patterns covering every variant
                        let covered: HashSet<String> = m
                            .arms
                            .iter()
                            .filter_map(|a| {
                                if let Pattern::EnumVariant(_, v) = &a.pattern {
                                    Some(v.name.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        // Find the enum name from the first EnumVariant pattern
                        if let Some(enum_name) = m.arms.iter().find_map(|a| {
                            if let Pattern::EnumVariant(e, _) = &a.pattern {
                                Some(e.name.clone())
                            } else {
                                None
                            }
                        }) {
                            if let Some((Symbol::Enum(info), _)) = symbols.globals.get(&enum_name) {
                                is_exhaustive = info.variants.iter().all(|v| covered.contains(v));
                            }
                        }
                    }
                    if is_exhaustive {
                        if let Some(first_full) = arm_results.first().map(|(_, f)| f.clone()) {
                            let intersection: HashSet<String> =
                                arm_results.iter().fold(first_full, |acc, (_, f)| {
                                    acc.intersection(f).cloned().collect()
                                });
                            full.extend(intersection);
                        }
                    }
                }
                Stmt::For(f) => {
                    // Comb for-loop body is Vec<Stmt> — treat assigns as fully driven.
                    for s in &f.body {
                        if let Stmt::Assign(a) = s {
                            let name = Self::expr_flat_name_tc(&a.target);
                            if !name.is_empty() {
                                all.insert(name.clone());
                                full.insert(name);
                            }
                        }
                    }
                }
                Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                    unreachable!("seq-only Stmt variant inside comb-context walker")
                }
                Stmt::Log(_) => {}
            }
        }
        (all, full)
    }

    /// Check a comb block for latch-inducing patterns and emit warnings.
    pub(crate) fn check_comb_latch(&mut self, stmts: &[Stmt], span: Span) {
        let (all_assigned, fully_assigned) = Self::comb_latch_targets(stmts, self.symbols);
        for name in &all_assigned {
            if !fully_assigned.contains(name) {
                self.errors.push(CompileError::general(
                    &format!(
                        "signal `{}` is not assigned on all control paths in comb block \
                         (infers a latch); add an `else` branch or a default assignment",
                        name
                    ),
                    span,
                ));
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
            TypeExpr::Bit => Ty::UInt(1),
            TypeExpr::FP32 => Ty::FP32,
            TypeExpr::BF16 => Ty::BF16,
            TypeExpr::Clock(domain) => Ty::Clock(domain.name.clone()),
            TypeExpr::Reset(kind, level) => Ty::Reset(*kind, *level),
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
                        // Bus types are permitted as wire/reg types — direction
                        // metadata is only meaningful on ports; in a wire
                        // context the bus is just a named bundle of fields.
                        // Each field becomes a flat signal at codegen.
                        crate::resolve::Symbol::Bus(_) => Ty::Bus(ident.name.clone()),
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
                    self.errors
                        .push(CompileError::undefined(&ident.name, ident.span));
                    Ty::Error
                }
            }
        }
    }

    /// The cycle offset an expression's value materializes at, per the
    /// `LatencyAt`/`PipelinedCall` machinery (`doc/proposal_pipelined_operators.md`
    /// §2). `0` for any expression not explicitly annotated — i.e. a plain
    /// signal read or a comb result. Used by [`Self::check_operand_latency_alignment`]
    /// to reject mixed-latency combinations without auto-alignment (v1: no
    /// auto-inserted delay lines — see the proposal's "Mixed-latency
    /// expressions" decision).
    fn expr_latency_tc(expr: &Expr) -> u32 {
        match &expr.kind {
            ExprKind::PipelinedCall(_, _, n) => *n,
            ExprKind::LatencyAt(_, n) => *n,
            _ => 0,
        }
    }

    /// Rejects combining operands materializing at different cycle offsets
    /// in a single expression (e.g. `acc@6 + x` where `x` is latency-0),
    /// per the proposal's "no auto-alignment in v1" rule. Reports the
    /// *first* mismatching pair found, phrased like the proposal's
    /// `fadd(acc@6, x)` example: "operands at cycle 6 and cycle 0".
    fn check_operand_latency_alignment(&mut self, operands: &[&Expr], span: Span) {
        let mut first: Option<(u32, Span)> = None;
        for op in operands {
            let lat = Self::expr_latency_tc(op);
            match &first {
                None => first = Some((lat, op.span)),
                Some((f, _)) if *f != lat => {
                    self.errors.push(CompileError::general(
                        &format!("operands at cycle {} and cycle {}", f, lat),
                        span,
                    ));
                    return;
                }
                _ => {}
            }
        }
    }

    /// Typecheck resolution for `Name<pipelined, N>(args...)` —
    /// `doc/proposal_pipelined_operators.md` §2. Reuses the same operand
    /// type-profile detection each registry operator's bare comb form
    /// already uses (currently only `fma`), resolves `(operator, profile, N)`
    /// against `pipelined_ops::lookup`, and returns the operator's normal
    /// result type. The call's own latency (`N`) is not carried in `Ty` —
    /// callers recover it structurally via [`Self::expr_latency_tc`], which
    /// treats a `PipelinedCall` node itself as materializing at cycle `N`.
    fn resolve_pipelined_call_type(
        &mut self,
        name: &str,
        call_args: &[Expr],
        stages: u32,
        span: Span,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) -> Ty {
        if name != "fma" {
            self.errors.push(CompileError::general(
                &format!(
                    "`{name}<pipelined, {stages}>(...)` — `{name}` is not a registry-backed \
                     pipelined operator; only `fma` is registered today (run `arch ops` to list all)"
                ),
                span,
            ));
            return Ty::Error;
        }
        // Same arity/type-profile rule as bare `fma(a, b, c)` (see the
        // `FunctionCall` arm below) — three same-float-type operands.
        if call_args.len() != 3 {
            self.errors.push(CompileError::general(
                &format!(
                    "`fma<pipelined, {stages}>(a, b, c)` takes 3 arguments, got {}",
                    call_args.len()
                ),
                span,
            ));
            return Ty::Error;
        }
        let arg_tys: Vec<Ty> = call_args
            .iter()
            .map(|a| self.resolve_expr_type(a, module_name, local_types))
            .collect();
        if arg_tys.iter().any(|t| *t == Ty::Error) {
            return Ty::Error;
        }
        let ta = &arg_tys[0];
        if !ta.is_float() || arg_tys[1] != *ta || arg_tys[2] != *ta {
            self.errors.push(CompileError::general(
                &format!(
                    "`fma<pipelined, {stages}>` requires three operands of the same float type, got {}, {}, {}",
                    arg_tys[0].display(), arg_tys[1].display(), arg_tys[2].display()
                ),
                span,
            ));
            return Ty::Error;
        }
        // The pipelined call's own inputs are combinational (cycle-0) reads
        // — no auto-alignment, so mixed-latency operands are rejected here
        // the same way they are for any other multi-operand call.
        let arg_refs: Vec<&Expr> = call_args.iter().collect();
        self.check_operand_latency_alignment(&arg_refs, span);

        let profile = match ta {
            Ty::FP32 => "FP32",
            Ty::BF16 => "BF16",
            _ => unreachable!("is_float() already restricted ta to FP32/BF16"),
        };
        match crate::pipelined_ops::lookup(name, profile, stages) {
            Ok(_entry) => ta.clone(),
            Err(miss) => {
                // Reuse LookupMiss's Display verbatim — it already renders
                // the exact enumerated-miss error text specified in
                // doc/proposal_pipelined_operators.md §1.
                self.errors
                    .push(CompileError::general(&miss.to_string(), span));
                Ty::Error
            }
        }
    }

    /// Enforces the comb-context restriction and the optional delay-line
    /// warning from `doc/proposal_pipelined_operators.md` §2 for one
    /// assignment statement (`target <op> value;`).
    ///
    /// - A bare `PipelinedCall` (latency N>0) directly assigned in a
    ///   `comb` block is an error — it has no cycle to land on.
    /// - The optional delay-line warning: `acc@N <= fma(a,b,c)` (comb fma,
    ///   *not* `<pipelined, N>`) written into an `@N` (N>=1) tap compiles
    ///   (unchanged pipe_reg delay-line semantics) but warns, since it's
    ///   almost always a "did you mean `fma<pipelined, N>`?" mistake — the
    ///   delay-line trap the proposal's Motivation section describes.
    ///
    /// The seq-context binding/consistency rule ("latency-M result bound
    /// at @N", "must be bound via a tapped target") is enforced earlier,
    /// in `elaborate::validate_pipe_assign_stmt` — it runs *before* the
    /// pipe_reg cascade rewrite strips the `@N` off the target, which by
    /// the time this function runs has already happened for any
    /// `PipelinedCall` reaching a seq assignment (a program that failed
    /// that check never reaches typecheck at all).
    fn check_pipelined_call_binding(&mut self, a: &Assign, in_comb: bool) {
        let target_latency = match &a.target.kind {
            ExprKind::LatencyAt(_, n) => Some(*n),
            _ => None,
        };
        match &a.value.kind {
            ExprKind::PipelinedCall(name, _, call_stages) => {
                if in_comb {
                    self.errors.push(CompileError::general(
                        &format!(
                            "`{name}<pipelined, {call_stages}>(...)` produces a latency-{call_stages} \
                             result and cannot be used in a `comb` block; bind it in a `seq` block via \
                             `target@{call_stages} <= {name}<pipelined, {call_stages}>(...)`"
                        ),
                        a.span,
                    ));
                }
            }
            ExprKind::FunctionCall(name, _) if !in_comb => {
                // Optional warning (proposal §2, "No silent retiming of
                // arbitrary exprs"): a bare comb call delayed via a
                // pipe_reg tap is legal (unchanged delay-line semantics)
                // but is almost always meant to be the pipelined variant.
                if let Some(target_n) = target_latency {
                    if target_n >= 1
                        && crate::pipelined_ops::BUILTIN_REGISTRY
                            .iter()
                            .any(|e| e.operator == name)
                    {
                        self.warnings.push(CompileWarning {
                            message: format!(
                                "comb `{name}` delayed {target_n} cycles via `@{target_n}`; did you mean \
                                 `{name}<pipelined, {target_n}>(...)`?"
                            ),
                            span: a.span,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn resolve_expr_type(
        &mut self,
        expr: &Expr,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) -> Ty {
        match &expr.kind {
            ExprKind::SvaNext(_, inner) => {
                if !self.in_sva_context {
                    self.errors.push(CompileError::general(
                        "`##N expr` is only legal inside `assert` / `cover` bodies",
                        expr.span,
                    ));
                    return Ty::Error;
                }
                // Cycle-shift only — type matches the inner expression.
                self.resolve_expr_type(inner, module_name, local_types)
            }
            ExprKind::LatencyAt(inner, _) => {
                // Latency annotation is a typing no-op — the value's type
                // matches the underlying signal. Placement/value validation
                // happens in check_reg_stmt / check_comb_stmt where the
                // target context is known.
                self.resolve_expr_type(inner, module_name, local_types)
            }
            ExprKind::PipelinedCall(name, call_args, stages) => self.resolve_pipelined_call_type(
                name,
                call_args,
                *stages,
                expr.span,
                module_name,
                local_types,
            ),
            ExprKind::SynthIdent(_, ty) => {
                // SynthIdent carries its own type — used by the
                // credit_channel dispatch pass (PR #3b-v). No symbol-table
                // lookup needed; the declaration lives in codegen.
                match ty {
                    TypeExpr::UInt(w) => eval_type_width_expr(w).map(Ty::UInt).unwrap_or(Ty::Error),
                    TypeExpr::SInt(w) => eval_type_width_expr(w).map(Ty::SInt).unwrap_or(Ty::Error),
                    TypeExpr::Bool | TypeExpr::Bit => Ty::Bool,
                    TypeExpr::Named(ident) => {
                        if let Some((crate::resolve::Symbol::Struct(_), _)) =
                            self.symbols.globals.get(&ident.name)
                        {
                            Ty::Struct(ident.name.clone())
                        } else if let Some((crate::resolve::Symbol::Enum(info), _)) =
                            self.symbols.globals.get(&ident.name)
                        {
                            Ty::Enum(ident.name.clone(), enum_width(info.variants.len()))
                        } else {
                            Ty::Error
                        }
                    }
                    _ => Ty::Error,
                }
            }
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
                LitKind::ParamSized(name, _) => self
                    .resolve_param_sized_literal_width(name, expr, local_types)
                    .map(Ty::UInt)
                    .unwrap_or(Ty::Error),
                // Float literals default to FP32; BF16 values are written via an
                // explicit `.to_bf16()` conversion (no implicit float narrowing)
                // OR by sitting in a known-BF16-type context slot, which the
                // pre-typecheck `coerce_typed_float_literals` pass (arch#622)
                // already rewrote to `LitKind::TypedFloat` below.
                LitKind::Float(_) => Ty::FP32,
                // Already resolved against its context type at compile time
                // (arch#622/#624) — take that type directly.
                LitKind::TypedFloat(FloatLitFmt::Fp32, _) => Ty::FP32,
                LitKind::TypedFloat(FloatLitFmt::Bf16, _) => Ty::BF16,
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
                if *op == BinOp::ImpliesNext && !self.in_sva_context {
                    self.errors.push(CompileError::general(
                        "`|=>` is only legal inside `assert` / `cover` bodies",
                        expr.span,
                    ));
                    return Ty::Error;
                }
                if *op == BinOp::Implies && !self.in_sva_context {
                    self.errors.push(CompileError::general(
                        "`|->` (and the deprecated `implies` keyword) is only legal inside `assert` / `cover` bodies; use `(!a) || b` for plain Boolean implication",
                        expr.span,
                    ));
                    return Ty::Error;
                }
                // Check for precedence ambiguity between bitwise and comparison ops.
                // ARCH and SV parse these differently — require parentheses to be explicit.
                self.check_precedence_ambiguity(*op, lhs, rhs, expr.span);
                let lt = self.resolve_expr_type(lhs, module_name, local_types);
                let rt = self.resolve_expr_type(rhs, module_name, local_types);
                self.check_operand_latency_alignment(&[lhs.as_ref(), rhs.as_ref()], expr.span);
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
                    UnaryOp::RedAnd | UnaryOp::RedOr | UnaryOp::RedXor => Ty::Bool,
                }
            }
            ExprKind::FieldAccess(base, field) => {
                self.resolve_field_access_type(base, field, expr.span, module_name, local_types)
            }
            ExprKind::MethodCall(base, method, args) => {
                self.resolve_method_call_type(base, method, args, module_name, local_types)
            }
            ExprKind::Cast(inner, ty) => {
                let src_ty = self.resolve_expr_type(inner, module_name, local_types);
                let dst_ty = self.resolve_type_expr(ty, module_name, local_types);
                // Bool/UInt<1> as Clock<Domain> — same as .as_clock<Domain>()
                if let Ty::Clock(_) = &dst_ty {
                    match &src_ty {
                        Ty::Bool | Ty::UInt(1) => {}
                        _ => {
                            self.errors.push(CompileError::general(
                                &format!(
                                    "`as Clock<D>` requires Bool or UInt<1> source, got {}",
                                    src_ty.display()
                                ),
                                inner.span,
                            ));
                        }
                    }
                    return dst_ty;
                }
                // Width check: if both widths are known and differ, emit error
                let src_w = self.type_total_width(&src_ty);
                let dst_w = self.type_total_width(&dst_ty);
                if let (Some(sw), Some(dw)) = (src_w, dst_w) {
                    if sw != dw {
                        self.errors.push(CompileError::general(
                            &format!(
                                "cast width mismatch: source is {} bits ({}), target is {} bits ({})",
                                sw, src_ty.display(), dw, dst_ty.display()
                            ),
                            inner.span,
                        ));
                    }
                }
                dst_ty
            }
            ExprKind::Index(base, _) => {
                let base_ty = self.resolve_expr_type(base, module_name, local_types);
                match base_ty {
                    Ty::Vec(inner, _) => *inner,
                    // Bit-select of a UInt/SInt produces a single bit; treat as Bool
                    // so it can be used directly in boolean expressions.
                    Ty::UInt(_) | Ty::SInt(_) => Ty::Bool,
                    // Propagate errors — don't silently produce UInt<1> for unresolved base types.
                    Ty::Error | Ty::Todo => Ty::Error,
                    _ => Ty::UInt(1),
                }
            }
            ExprKind::BitSlice(base, hi, lo) => {
                if !Self::is_portable_bit_slice_base(base) {
                    self.errors.push(CompileError::general(
                        "cannot bit-slice this expression directly; SystemVerilog backends cannot portably emit `(expr)[hi:lo]`. For same-width modular arithmetic, use wrapping operators such as `+%` or `-%`; otherwise assign the expression to a typed `let`/wire first and slice the named value.",
                        base.span,
                    ));
                    return Ty::Error;
                }
                let base_ty = self.resolve_expr_type(base, module_name, local_types);
                let hi_val = self.eval_const_expr(hi, local_types);
                let lo_val = self.eval_const_expr(lo, local_types);
                match (hi_val, lo_val) {
                    (Some(h), Some(l)) if h >= l => {
                        let w = (h - l + 1) as u32;
                        if let Ty::SInt(_) = base_ty {
                            Ty::SInt(w)
                        } else {
                            Ty::UInt(w)
                        }
                    }
                    _ => Ty::Error,
                }
            }
            ExprKind::PartSelect(base, _start, width, _up) => {
                if !Self::is_portable_bit_slice_base(base) {
                    self.errors.push(CompileError::general(
                        "cannot part-select this expression directly; SystemVerilog backends cannot portably emit `(expr)[start +: width]` (or `-:`). For same-width modular arithmetic, use wrapping operators such as `+%` or `-%`; otherwise assign the expression to a typed `let`/wire first and part-select the named value.",
                        base.span,
                    ));
                    return Ty::Error;
                }
                // width is const; result type is UInt<width>
                match self.eval_const_expr(width, local_types) {
                    Some(w) if w > 0 => Ty::UInt(w as u32),
                    _ => Ty::Error,
                }
            }
            ExprKind::StructLiteral(name, _) => Ty::Struct(name.name.clone()),
            ExprKind::EnumVariant(name, _) => {
                if let Some((sym, _)) = self.symbols.globals.get(&name.name) {
                    match sym {
                        crate::resolve::Symbol::Enum(info) => {
                            let bits = enum_width(info.variants.len());
                            return Ty::Enum(name.name.clone(), bits);
                        }
                        crate::resolve::Symbol::ExternEnum(_) => {
                            // Opaque extern type — variant values are unchecked.
                            return Ty::Enum(name.name.clone(), 1);
                        }
                        _ => {}
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
                self.check_match_exhaustive(
                    scrutinee,
                    &patterns,
                    expr.span,
                    module_name,
                    local_types,
                );
                // Return type from first non-wildcard arm
                match arms.first() {
                    Some(arm) => self.resolve_expr_type(&arm.value, module_name, local_types),
                    None => Ty::Error,
                }
            }
            ExprKind::Concat(parts) => {
                // Total width = sum of each part's width (Bool=1, UInt<N>=N, else 1)
                let total: u32 = parts
                    .iter()
                    .map(
                        |p| match self.resolve_expr_type(p, module_name, local_types) {
                            Ty::UInt(w) | Ty::SInt(w) => w,
                            Ty::Bool => 1,
                            _ => 1,
                        },
                    )
                    .sum();
                Ty::UInt(total)
            }
            ExprKind::Repeat(count, value) => {
                // {N{expr}} — total width = N * width(expr)
                let val_width = match self.resolve_expr_type(value, module_name, local_types) {
                    Ty::UInt(w) | Ty::SInt(w) => w,
                    Ty::Bool => 1,
                    _ => 1,
                };
                let n = self.eval_const_expr(count, local_types).unwrap_or(1) as u32;
                Ty::UInt(n * val_width)
            }
            ExprKind::Clog2(arg) => {
                // $clog2 returns a compile-time constant width value
                if let Some(v) = self.eval_const_expr(arg, local_types) {
                    let bits = if v == 0 {
                        1
                    } else {
                        64 - v.leading_zeros() as u64
                    };
                    Ty::UInt(bits as u32)
                } else {
                    Ty::UInt(32) // fallback: treat as generic integer
                }
            }
            ExprKind::Onehot(index) => {
                // onehot(index) returns a one-hot value; width = 2^index_width
                // but we can't easily compute that, so infer from context (assignment target).
                // Return a generic UInt that will be width-checked at assignment.
                let idx_ty = self.resolve_expr_type(index, module_name, local_types);
                match idx_ty {
                    Ty::UInt(w) => Ty::UInt(1 << w),
                    _ => Ty::UInt(32),
                }
            }
            ExprKind::Signed(inner) => {
                let inner_ty = self.resolve_expr_type(inner, module_name, local_types);
                match inner_ty {
                    Ty::UInt(w) | Ty::SInt(w) => Ty::SInt(w),
                    Ty::Bool => Ty::SInt(1),
                    Ty::Enum(_, w) => Ty::SInt(w),
                    _ => {
                        self.errors.push(CompileError::general(
                            &format!(
                                "signed() requires UInt, SInt, or Bool operand, got {}",
                                inner_ty.display()
                            ),
                            expr.span,
                        ));
                        Ty::Error
                    }
                }
            }
            ExprKind::Unsigned(inner) => {
                let inner_ty = self.resolve_expr_type(inner, module_name, local_types);
                match inner_ty {
                    Ty::UInt(w) | Ty::SInt(w) => Ty::UInt(w),
                    Ty::Bool => Ty::UInt(1),
                    Ty::Enum(_, w) => Ty::UInt(w),
                    _ => {
                        self.errors.push(CompileError::general(
                            &format!(
                                "unsigned() requires UInt, SInt, or Bool operand, got {}",
                                inner_ty.display()
                            ),
                            expr.span,
                        ));
                        Ty::Error
                    }
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
            ExprKind::Inside(scrutinee, members) => {
                self.resolve_expr_type(scrutinee, module_name, local_types);
                for m in members {
                    match m {
                        InsideMember::Single(e) => {
                            self.resolve_expr_type(e, module_name, local_types);
                        }
                        InsideMember::Range(lo, hi) => {
                            self.resolve_expr_type(lo, module_name, local_types);
                            self.resolve_expr_type(hi, module_name, local_types);
                        }
                    }
                }
                Ty::Bool
            }
            ExprKind::FunctionCall(name, call_args) => {
                // Built-in float intrinsics. `fma(a, b, c)` is a single-rounded
                // fused multiply-add (a*b + c); all three operands must be the
                // same float type, result is that type. `is_nan(x)` → Bool.
                if name == "fma" {
                    if call_args.len() != 3 {
                        self.errors.push(CompileError::general(
                            &format!("`fma(a, b, c)` takes 3 arguments, got {}", call_args.len()),
                            expr.span,
                        ));
                        return Ty::Error;
                    }
                    let ta = self.resolve_expr_type(&call_args[0], module_name, local_types);
                    let tb = self.resolve_expr_type(&call_args[1], module_name, local_types);
                    let tc = self.resolve_expr_type(&call_args[2], module_name, local_types);
                    if ta == Ty::Error || tb == Ty::Error || tc == Ty::Error {
                        return Ty::Error;
                    }
                    if !ta.is_float() || tb != ta || tc != ta {
                        self.errors.push(CompileError::general(
                            &format!("`fma` requires three operands of the same float type, got {}, {}, {}",
                                ta.display(), tb.display(), tc.display()),
                            expr.span,
                        ));
                        return Ty::Error;
                    }
                    self.check_operand_latency_alignment(
                        &[&call_args[0], &call_args[1], &call_args[2]],
                        expr.span,
                    );
                    return ta;
                }
                if name == "is_nan" {
                    if call_args.len() != 1 {
                        self.errors.push(CompileError::general(
                            &format!("`is_nan(x)` takes 1 argument, got {}", call_args.len()),
                            expr.span,
                        ));
                        return Ty::Error;
                    }
                    let tx = self.resolve_expr_type(&call_args[0], module_name, local_types);
                    if tx != Ty::Error && !tx.is_float() {
                        self.errors.push(CompileError::general(
                            &format!("`is_nan(x)` requires a float operand, got {}", tx.display()),
                            call_args[0].span,
                        ));
                        return Ty::Error;
                    }
                    return Ty::Bool;
                }
                // Built-in SVA edge sugar: `rose(a)` ≡ `a and not past(a, 1)`,
                // `fell(a)` ≡ `not a and past(a, 1)`. Both Bool-returning,
                // arity 1, SVA-context only.
                if name == "rose" || name == "fell" {
                    if !self.in_sva_context {
                        self.errors.push(CompileError::general(
                            &format!(
                                "`{name}(...)` is only legal inside `assert` / `cover` bodies"
                            ),
                            expr.span,
                        ));
                        return Ty::Error;
                    }
                    if call_args.len() != 1 {
                        self.errors.push(CompileError::general(
                            &format!("`{name}(expr)` takes 1 argument, got {}", call_args.len()),
                            expr.span,
                        ));
                        return Ty::Error;
                    }
                    let inner = self.resolve_expr_type(&call_args[0], module_name, local_types);
                    if inner != Ty::Bool && inner != Ty::Error && inner != Ty::Todo {
                        self.errors.push(CompileError::general(
                            &format!(
                                "`{name}(expr)` requires Bool argument, got {}",
                                inner.display()
                            ),
                            call_args[0].span,
                        ));
                    }
                    return Ty::Bool;
                }
                // Built-in: `past(expr, N)` — SVA shadow-reg sugar.
                if name == "past" {
                    if !self.in_sva_context {
                        self.errors.push(CompileError::general(
                            "`past(...)` is only legal inside `assert` / `cover` bodies",
                            expr.span,
                        ));
                        return Ty::Error;
                    }
                    if call_args.len() != 2 {
                        self.errors.push(CompileError::general(
                            &format!("`past(expr, N)` takes 2 arguments, got {}", call_args.len()),
                            expr.span,
                        ));
                        return Ty::Error;
                    }
                    // N must be a const positive integer.
                    let n_val = match &call_args[1].kind {
                        ExprKind::Literal(LitKind::Dec(n)) => Some(*n),
                        ExprKind::Literal(LitKind::Sized(_, n)) => Some(*n),
                        _ => None,
                    };
                    match n_val {
                        Some(n) if n >= 1 => {}
                        Some(_) => {
                            self.errors.push(CompileError::general(
                                "`past(expr, N)` requires N >= 1 (current cycle is just `expr`)",
                                call_args[1].span,
                            ));
                            return Ty::Error;
                        }
                        None => {
                            self.errors.push(CompileError::general(
                                "`past(expr, N)` requires N to be a compile-time integer literal",
                                call_args[1].span,
                            ));
                            return Ty::Error;
                        }
                    }
                    // Result type matches the inner expression.
                    return self.resolve_expr_type(&call_args[0], module_name, local_types);
                }
                if let Some((Symbol::Function(overloads), _)) = self.symbols.globals.get(name) {
                    // Resolve argument types first.
                    let arg_tys: Vec<Ty> = call_args
                        .iter()
                        .map(|a| {
                            let mut lt = local_types.clone();
                            self.resolve_expr_type(a, module_name, &mut lt)
                        })
                        .collect();

                    // Find matching overload: same arity, compatible types.
                    let overloads = overloads.clone(); // detach borrow so we can call &mut self methods
                    let chosen = overloads.iter().enumerate().find(|(_, ov)| {
                        if ov.arg_types.len() != arg_tys.len() {
                            return false;
                        }
                        ov.arg_types
                            .iter()
                            .zip(arg_tys.iter())
                            .all(|(expected_te, actual_ty)| {
                                match (expected_te, actual_ty) {
                                    (TypeExpr::UInt(we), Ty::UInt(wa)) => {
                                        // Compare widths when the expression is a simple literal.
                                        eval_type_width_expr(we).map_or(true, |ew| ew == *wa)
                                    }
                                    (TypeExpr::SInt(we), Ty::SInt(wa)) => {
                                        eval_type_width_expr(we).map_or(true, |ew| ew == *wa)
                                    }
                                    (TypeExpr::Bool, Ty::Bool) => true,
                                    (TypeExpr::Bit, Ty::UInt(1)) => true,
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
                            if let Some(ov) = overloads
                                .iter()
                                .find(|ov| ov.arg_types.len() == call_args.len())
                            {
                                let ret_ty = ov.ret_ty.clone();
                                self.resolve_type_expr(&ret_ty, module_name, local_types)
                            } else {
                                self.errors.push(CompileError::general(
                                    &format!(
                                        "no matching overload for `{name}` with {} argument(s)",
                                        call_args.len()
                                    ),
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

    /// Bases that SystemVerilog backends (Verilator/iverilog) accept as the
    /// target of a bit-slice `[hi:lo]` or part-select `[start +: w]` without
    /// needing to be bound to a named `let` first. `BitSlice`, `PartSelect`,
    /// `Bool`, and `EnumVariant` were removed from this list — chained
    /// bit-select (`(a[7:4])[1:0]`) and slicing/part-selecting a literal
    /// bool/enum-variant produce SV that Verilator/iverilog reject even when
    /// parenthesized, so those bases must be rejected here rather than
    /// allowed through to codegen. See issue #653.
    fn is_portable_bit_slice_base(base: &Expr) -> bool {
        matches!(
            base.kind,
            ExprKind::Ident(_)
                | ExprKind::SynthIdent(_, _)
                | ExprKind::Literal(_)
                | ExprKind::Index(_, _)
                | ExprKind::FieldAccess(_, _)
                | ExprKind::Concat(_)
                | ExprKind::Repeat(_, _)
                | ExprKind::FunctionCall(_, _)
                | ExprKind::MethodCall(_, _, _)
        )
    }

    /// Find the port-site `<P=expr>` bus-param overrides for a bus field
    /// access base (`port.sig` or `vec_port[i].sig`) and fold each value to a
    /// literal in the *current* (enclosing-construct) param scope — override
    /// exprs like `initiator BusVr<DATA_W=DATA_W>` reference the enclosing
    /// module's params, not the bus's. Values that don't const-fold are
    /// dropped, falling back to the bus's declared default.
    fn bus_port_param_overrides(
        &self,
        base: &Expr,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) -> Vec<(String, Expr)> {
        let base_name = match &base.kind {
            ExprKind::Ident(n) => n,
            ExprKind::Index(inner, _) => match &inner.kind {
                ExprKind::Ident(n) => n,
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let Some(port) = self.source.items.iter().find_map(|item| {
            let ports: &[PortDecl] = match item {
                Item::Module(m) if m.name.name == module_name => &m.ports,
                Item::Fsm(f) if f.name.name == module_name => &f.ports,
                Item::Pipeline(p) if p.name.name == module_name => &p.ports,
                _ => return None,
            };
            ports.iter().find(|p| p.name.name == *base_name)
        }) else {
            return Vec::new();
        };
        let Some(ref bi) = port.bus_info else {
            return Vec::new();
        };
        bi.params
            .iter()
            .filter_map(|pa| {
                let v = self.eval_const_expr(&pa.value, local_types)?;
                let lit = Expr::new(ExprKind::Literal(LitKind::Dec(v)), pa.value.span);
                Some((pa.name.name.clone(), lit))
            })
            .collect()
    }

    /// Resolve `ExprKind::FieldAccess(base, field)` — the type of a `.field`
    /// access on a struct, bus, or `Reset.asserted` polarity-abstracted bool.
    /// Extracted from `resolve_expr_type` for readability — the original arm
    /// was 80 lines.
    fn resolve_field_access_type(
        &mut self,
        base: &Expr,
        field: &Ident,
        expr_span: Span,
        module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) -> Ty {
        let base_ty = self.resolve_expr_type(base, module_name, local_types);
        // rst.asserted — polarity-abstracted boolean: true when reset is active
        if field.name == "asserted" {
            if matches!(base_ty, Ty::Reset(_, _)) {
                return Ty::Bool;
            }
            self.errors.push(CompileError::general(
                "`.asserted` is only valid on Reset ports",
                field.span,
            ));
            return Ty::Error;
        }
        if let Ty::Struct(name) = &base_ty {
            // Synthesized find_first result struct: no entry lives in
            // symbols.globals; fields are computed from the name's
            // width suffix.
            if let Some(w_str) = name.strip_prefix("__ArchFindResult_") {
                if let Ok(w) = w_str.parse::<u32>() {
                    return match field.name.as_str() {
                        "found" => Ty::Bool,
                        "index" => Ty::UInt(w),
                        _ => Ty::Error,
                    };
                }
            }
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
        if let Ty::Bus(name) = &base_ty {
            if let Some((sym, _)) = self.symbols.globals.get(name) {
                if let crate::resolve::Symbol::Bus(info) = sym {
                    // Port-site `<P=expr>` overrides, when the base names a
                    // bus port (or a `Vec<Bus,N>` element) of the enclosing
                    // construct. The override exprs were written at the port
                    // site, so they fold in the *enclosing* param scope —
                    // e.g. `initiator BusVr<DATA_W=DATA_W>` binds the bus's
                    // DATA_W to the module's — and are pre-folded to literals
                    // here, while the enclosing scope is still active.
                    let overrides = self.bus_port_param_overrides(base, module_name, local_types);
                    // The signal's declared type folds in the *bus's* param
                    // scope: bus param defaults, with the pre-folded port-site
                    // overrides substituted in. Resolving it in the enclosing
                    // scope instead lets a same-named module param shadow the
                    // bus param — the cross-construct collision class issue
                    // #462 fixed for sibling modules.
                    let mut bus_scope = info.params.clone();
                    for (pname, pval) in &overrides {
                        if let Some(pd) = bus_scope.iter_mut().find(|pd| &pd.name.name == pname) {
                            pd.default = Some(pval.clone());
                        }
                    }
                    let mut pm = info.default_param_map();
                    for (pname, pval) in &overrides {
                        pm.insert(pname.clone(), pval);
                    }
                    let eff = info.effective_signals(&pm);
                    for (sname, _dir, sty) in &eff {
                        if sname == &field.name {
                            let saved = std::mem::replace(&mut self.active_params, bus_scope);
                            let ty = self.resolve_type_expr(sty, module_name, local_types);
                            self.active_params = saved;
                            return ty;
                        }
                    }
                    self.errors.push(CompileError::general(
                        &format!("bus `{}` has no signal `{}`", name, field.name),
                        field.span,
                    ));
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
                    expr_span,
                ));
            }
        }
        Ty::Error
    }

    /// Resolve `ExprKind::MethodCall(base, method, args)` — width casts
    /// (`trunc` / `zext` / `sext` / `resize`), `Vec` methods (`any` / `all` /
    /// `count` / `find_first` / `reduce_*`), `Option`-style chans, and the rest.
    /// Extracted from `resolve_expr_type` for readability — the original arm
    /// was 230 lines, the largest single arm in the function.
    fn resolve_method_call_type(
        &mut self,
        base: &Expr,
        method: &Ident,
        args: &[Expr],
        module_name: &str,
        local_types: &HashMap<String, Ty>,
    ) -> Ty {
        let base_ty = self.resolve_expr_type(base, module_name, local_types);
        match method.name.as_str() {
            "trunc" | "zext" | "sext" | "resize" => {
                if let Some(width_expr) = args.first() {
                    if let Some(w) = self.eval_const_expr(width_expr, local_types) {
                        let target_w = w as u32;
                        let source_w = match &base_ty {
                            Ty::UInt(sw) | Ty::SInt(sw) => Some(*sw),
                            Ty::Bool => Some(1),
                            _ => None, // param-dependent width — can't verify statically
                        };
                        if let Some(sw) = source_w {
                            if method.name == "trunc" && target_w == sw {
                                self.errors.push(CompileError::general(
                                    &format!(".trunc<{}>() on a {}-bit value is a no-op — remove the cast", target_w, sw),
                                    method.span,
                                ));
                                return Ty::Error;
                            }
                            if method.name == "trunc" && target_w > sw {
                                self.errors.push(CompileError::general(
                                    &format!(".trunc<{}>() on a {}-bit value widens rather than truncates — use .zext<{}>() or .sext<{}>() to extend", target_w, sw, target_w, target_w),
                                    method.span,
                                ));
                                return Ty::Error;
                            }
                            if (method.name == "zext" || method.name == "sext") && target_w == sw {
                                self.errors.push(CompileError::general(
                                    &format!(
                                        ".{}<{}>() on a {}-bit value is a no-op — remove the cast",
                                        method.name, target_w, sw
                                    ),
                                    method.span,
                                ));
                                return Ty::Error;
                            }
                            if (method.name == "zext" || method.name == "sext") && target_w < sw {
                                self.errors.push(CompileError::general(
                                    &format!(".{}<{}>() on a {}-bit value narrows rather than extends — use .trunc<{}>() to narrow", method.name, target_w, sw, target_w),
                                    method.span,
                                ));
                                return Ty::Error;
                            }
                        }
                        if method.name == "sext" {
                            Ty::SInt(target_w)
                        } else if let Ty::SInt(_) = base_ty {
                            Ty::SInt(target_w)
                        } else {
                            Ty::UInt(target_w)
                        }
                    } else {
                        Ty::Error
                    }
                } else {
                    Ty::Error
                }
            }
            // Float conversions. `.to_fp32()` / `.to_bf16()` take no args and
            // widen/narrow/convert into the named float type. `.to_uint<N>()` /
            // `.to_sint<N>()` convert a float to an integer (toward-zero,
            // saturating per the RISC-V profile — see doc/plan_fp_types.md §6).
            "to_fp32" | "to_bf16" => {
                let target = if method.name == "to_fp32" {
                    Ty::FP32
                } else {
                    Ty::BF16
                };
                match &base_ty {
                    Ty::FP32 | Ty::BF16 | Ty::UInt(_) | Ty::SInt(_) | Ty::Bool => {
                        if base_ty == target {
                            self.errors.push(CompileError::general(
                                &format!(
                                    ".{}() on a {} value is a no-op — remove the cast",
                                    method.name,
                                    target.display()
                                ),
                                method.span,
                            ));
                            return Ty::Error;
                        }
                        target
                    }
                    Ty::Todo => Ty::Todo,
                    Ty::Error => Ty::Error,
                    _ => {
                        self.errors.push(CompileError::general(
                            &format!(
                                ".{}() requires a float or integer operand, got {}",
                                method.name,
                                base_ty.display()
                            ),
                            method.span,
                        ));
                        Ty::Error
                    }
                }
            }
            "to_uint" | "to_sint" => {
                if !base_ty.is_float() && !matches!(base_ty, Ty::Todo | Ty::Error) {
                    self.errors.push(CompileError::general(
                        &format!(
                            ".{}<N>() requires a floating-point operand, got {}",
                            method.name,
                            base_ty.display()
                        ),
                        method.span,
                    ));
                    return Ty::Error;
                }
                if let Some(width_expr) = args.first() {
                    if let Some(w) = self.eval_const_expr(width_expr, local_types) {
                        let target_w = w as u32;
                        if method.name == "to_uint" {
                            Ty::UInt(target_w)
                        } else {
                            Ty::SInt(target_w)
                        }
                    } else {
                        Ty::Error
                    }
                } else {
                    self.errors.push(CompileError::general(
                        &format!(
                            ".{}<N>() requires a width type argument, e.g. .{}<32>()",
                            method.name, method.name
                        ),
                        method.span,
                    ));
                    Ty::Error
                }
            }
            "reverse" => {
                if let Some(chunk_expr) = args.first() {
                    if let Some(chunk) = self.eval_const_expr(chunk_expr, local_types) {
                        let chunk = chunk as u32;
                        if chunk == 0 {
                            self.errors.push(CompileError::general(
                                ".reverse(N) chunk size must be > 0",
                                method.span,
                            ));
                            Ty::Error
                        } else {
                            let base_w = match &base_ty {
                                Ty::UInt(w) | Ty::SInt(w) => *w,
                                Ty::Bool => 1,
                                _ => {
                                    self.errors.push(CompileError::general(
                                        &format!(
                                            ".reverse(N) requires UInt/SInt/Bool base, got {}",
                                            base_ty.display()
                                        ),
                                        method.span,
                                    ));
                                    return Ty::Error;
                                }
                            };
                            if base_w % chunk != 0 {
                                self.errors.push(CompileError::general(
                                    &format!(".reverse({chunk}) requires width divisible by {chunk}, got UInt<{base_w}>"),
                                    method.span,
                                ));
                                Ty::Error
                            } else {
                                base_ty
                            }
                        }
                    } else {
                        Ty::Error
                    }
                } else {
                    self.errors.push(CompileError::general(
                        ".reverse(N) requires a chunk size argument",
                        method.span,
                    ));
                    Ty::Error
                }
            }
            // Vec reduction + predicate methods (plan_vec_methods.md v1, PR #1 subset).
            // `item` is the per-iteration element, `index` is the position (UInt<clog2(N)>).
            // Both are injected into the predicate's local scope during checking.
            "any" | "all" | "count" | "contains" | "reduce_or" | "reduce_and" | "reduce_xor"
            | "find_first" => {
                let (elem_ty, n) = match &base_ty {
                    Ty::Vec(inner, count) => ((**inner).clone(), *count),
                    _ => {
                        self.errors.push(CompileError::general(
                            &format!(
                                "`.{}(...)` requires a Vec<T,N> receiver, got {}",
                                method.name,
                                base_ty.display()
                            ),
                            method.span,
                        ));
                        return Ty::Error;
                    }
                };
                if n == 0 {
                    self.errors.push(CompileError::general(
                        &format!(
                            "`.{}(...)` on a zero-length Vec has no meaningful result",
                            method.name
                        ),
                        method.span,
                    ));
                    return Ty::Error;
                }
                let idx_w = crate::width::index_width(n as u64);
                let pred_needed = !matches!(
                    method.name.as_str(),
                    "reduce_or" | "reduce_and" | "reduce_xor" | "contains"
                );

                if pred_needed {
                    if args.len() != 1 {
                        self.errors.push(CompileError::general(
                            &format!(
                                "`.{}(pred)` takes exactly 1 argument (the predicate)",
                                method.name
                            ),
                            method.span,
                        ));
                        return Ty::Error;
                    }
                    // Inject item/index into the predicate's scope.
                    let mut pred_scope = local_types.clone();
                    // Shadow warnings: user-declared signals with these names.
                    for n in ["item", "index"] {
                        if local_types.contains_key(n) {
                            self.warnings.push(CompileWarning {
                                message: format!(
                                    "Vec method predicate binder `{}` shadows an enclosing signal with the same name — rename the outer signal to avoid confusion",
                                    n),
                                span: method.span,
                            });
                        }
                    }
                    pred_scope.insert("item".to_string(), elem_ty.clone());
                    pred_scope.insert("index".to_string(), Ty::UInt(idx_w));
                    let pred_ty = self.resolve_expr_type(&args[0], module_name, &pred_scope);
                    if !matches!(pred_ty, Ty::Bool | Ty::UInt(1)) && pred_ty != Ty::Error {
                        self.errors.push(CompileError::general(
                            &format!(
                                "`.{}` predicate must be Bool, got {}",
                                method.name,
                                pred_ty.display()
                            ),
                            args[0].span,
                        ));
                        return Ty::Error;
                    }
                } else if matches!(method.name.as_str(), "contains") {
                    if args.len() != 1 {
                        self.errors.push(CompileError::general(
                            "`.contains(x)` takes exactly 1 argument",
                            method.span,
                        ));
                        return Ty::Error;
                    }
                    let arg_ty = self.resolve_expr_type(&args[0], module_name, local_types);
                    // Basic element-type compatibility (same kind + width).
                    let compatible = match (&elem_ty, &arg_ty) {
                        (Ty::UInt(a), Ty::UInt(b)) | (Ty::SInt(a), Ty::SInt(b)) => a == b,
                        (Ty::Bool, Ty::Bool) => true,
                        _ => elem_ty == arg_ty,
                    };
                    if !compatible && arg_ty != Ty::Error && elem_ty != Ty::Error {
                        self.errors.push(CompileError::general(
                            &format!("`.contains(x)` argument type `{}` doesn't match Vec element type `{}`",
                                arg_ty.display(), elem_ty.display()),
                            args[0].span,
                        ));
                        return Ty::Error;
                    }
                } else {
                    // reduce_or/and/xor: no argument
                    if !args.is_empty() {
                        self.errors.push(CompileError::general(
                            &format!("`.{}()` takes no arguments", method.name),
                            method.span,
                        ));
                        return Ty::Error;
                    }
                }

                match method.name.as_str() {
                    "any" | "all" | "contains" => Ty::Bool,
                    "find_first" => {
                        // Synthesized struct { found: Bool; index: UInt<idx_w> }.
                        // Name is unique per idx_w; the typechecker's struct-field
                        // lookup has a targeted fallback for this prefix, so no
                        // entry needs to live in symbols.globals.
                        Ty::Struct(format!("__ArchFindResult_{}", idx_w))
                    }
                    "count" => {
                        // clog2(N+1) for popcount result width.
                        let w = crate::width::index_width((n + 1) as u64);
                        Ty::UInt(w)
                    }
                    "reduce_or" | "reduce_and" | "reduce_xor" => {
                        // Returns a value of the element's width (or Bool if element is Bool).
                        match &elem_ty {
                            Ty::Bool => Ty::Bool,
                            Ty::UInt(w) => Ty::UInt(*w),
                            Ty::SInt(w) => Ty::SInt(*w),
                            _ => {
                                self.errors.push(CompileError::general(
                                    &format!(
                                        "`.{}()` requires UInt/SInt/Bool element type, got `{}`",
                                        method.name,
                                        elem_ty.display()
                                    ),
                                    method.span,
                                ));
                                return Ty::Error;
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            _ => Ty::Error,
        }
    }
    /// Detects expressions where ARCH and SV precedence differ and the user
    /// has not added parentheses. Specifically: bitwise ops (`&`, `|`, `^`)
    /// mixed with comparison ops (`==`, `!=`, `<`, `>`, `<=`, `>=`) as children.
    pub(crate) fn check_precedence_ambiguity(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) {
        let is_bitwise = matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor);
        let is_comparison = matches!(
            op,
            BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
        );

        // Case 1: comparison with unparenthesized bitwise child
        // e.g. `a & b == c` — ARCH parses as (a & b) == c, SV parses as a & (b == c)
        if is_comparison {
            for child in [lhs, rhs] {
                if let ExprKind::Binary(child_op, _, _) = &child.kind {
                    if matches!(child_op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor)
                        && !child.parenthesized
                    {
                        self.errors.push(CompileError::general(
                            &format!(
                                "ambiguous precedence: bitwise '{}' inside comparison '{}' — add parentheses (ARCH and SystemVerilog parse this differently)",
                                child_op, op
                            ),
                            span,
                        ));
                    }
                }
            }
        }

        // Case 2: bitwise with unparenthesized comparison child
        // e.g. `a == b & c` — ARCH parses as a == (b & c), SV parses as (a == b) & c
        if is_bitwise {
            for child in [lhs, rhs] {
                if let ExprKind::Binary(child_op, _, _) = &child.kind {
                    if matches!(
                        child_op,
                        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
                    ) && !child.parenthesized
                    {
                        self.errors.push(CompileError::general(
                            &format!(
                                "ambiguous precedence: comparison '{}' inside bitwise '{}' — add parentheses (ARCH and SystemVerilog parse this differently)",
                                child_op, op
                            ),
                            span,
                        ));
                    }
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

        // Floating-point operands: comparisons → Bool; `+ - *` → the same float
        // type (no widening). The two operands must be the identical float type;
        // there is no implicit float conversion (use `.to_fp32()`/`.to_bf16()`).
        if lt.is_float() || rt.is_float() {
            // Render the real operator (`/`, `%`, `<<`, …) for the diagnostics
            // below, including the unsupported-op arm — never a `<op>` placeholder.
            let sym = op.to_string();
            match op {
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
                    if lt != rt {
                        self.errors.push(CompileError::general(
                            &format!(
                                "floating-point comparison `{sym}` requires matching types, got {} and {}",
                                lt.display(), rt.display()
                            ),
                            _span,
                        ));
                        return Ty::Error;
                    }
                    return Ty::Bool;
                }
                BinOp::Add | BinOp::Sub | BinOp::Mul => {
                    if lt != rt {
                        self.errors.push(CompileError::general(
                            &format!(
                                "type mismatch in floating-point `{sym}`: {} vs {} (no implicit float conversion; use .to_fp32()/.to_bf16())",
                                lt.display(), rt.display()
                            ),
                            _span,
                        ));
                        return Ty::Error;
                    }
                    return lt.clone();
                }
                _ => {
                    self.errors.push(CompileError::general(
                        &format!(
                            "operator `{sym}` is not supported on floating-point type {} (v1 supports + - * and comparisons; use fma() for fused multiply-add)",
                            if lt.is_float() { lt.display() } else { rt.display() }
                        ),
                        _span,
                    ));
                    return Ty::Error;
                }
            }
        }

        match op {
            BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => Ty::Bool,
            BinOp::And | BinOp::Or | BinOp::Implies | BinOp::ImpliesNext => Ty::Bool,
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
            BinOp::AddWrap | BinOp::SubWrap => {
                let lw = lt.width().unwrap_or(1);
                let rw = rt.width().unwrap_or(1);
                let w = lw.max(rw);
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
            BinOp::MulWrap => {
                let lw = lt.width().unwrap_or(1);
                let rw = rt.width().unwrap_or(1);
                let w = lw.max(rw);
                if matches!(lt, Ty::SInt(_)) || matches!(rt, Ty::SInt(_)) {
                    Ty::SInt(w)
                } else {
                    Ty::UInt(w)
                }
            }
            BinOp::Div | BinOp::Mod => lt.clone(),
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                // Bool is UInt<1>; bitwise ops on two 1-bit types stay Bool.
                let lw = lt.width().unwrap_or(1);
                let rw = rt.width().unwrap_or(1);
                if lw.max(rw) == 1 {
                    Ty::Bool
                } else {
                    Ty::UInt(lw.max(rw))
                }
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
                // Resolve local params in the current construct before
                // consulting the whole source. Multiple unrelated modules can
                // legitimately use the same local-param name with different
                // values.
                if let Some(p) = self.active_params.iter().find(|p| p.name.name == *name) {
                    if let Some(default) = &p.default {
                        return self.eval_const_expr(default, local_types);
                    }
                }

                // Legacy fallback for contexts that do not enter a construct
                // scope, such as package-level scans.
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
            ExprKind::Binary(BinOp::Div, lhs, rhs) => {
                let l = self.eval_const_expr(lhs, local_types)?;
                let r = self.eval_const_expr(rhs, local_types)?;
                if r == 0 {
                    None
                } else {
                    Some(l / r)
                }
            }
            ExprKind::Binary(BinOp::Mod, lhs, rhs) => {
                let l = self.eval_const_expr(lhs, local_types)?;
                let r = self.eval_const_expr(rhs, local_types)?;
                if r == 0 {
                    None
                } else {
                    Some(l % r)
                }
            }
            ExprKind::Clog2(arg) => {
                let v = self.eval_const_expr(arg, local_types)?;
                if v <= 1 {
                    Some(1)
                } else {
                    Some(64 - (v - 1).leading_zeros() as u64)
                }
            }
            _ => None,
        }
    }

    fn eval_const_expr_i64(&self, expr: &Expr, local_types: &HashMap<String, Ty>) -> Option<i64> {
        match &expr.kind {
            ExprKind::Literal(LitKind::Dec(v))
            | ExprKind::Literal(LitKind::Hex(v))
            | ExprKind::Literal(LitKind::Bin(v))
            | ExprKind::Literal(LitKind::Sized(_, v)) => i64::try_from(*v).ok(),
            ExprKind::Ident(name) => {
                if let Some(p) = self.active_params.iter().find(|p| p.name.name == *name) {
                    if let Some(default) = &p.default {
                        return self.eval_const_expr_i64(default, local_types);
                    }
                }

                for item in &self.source.items {
                    if let Item::Module(m) = item {
                        for p in &m.params {
                            if p.name.name == *name {
                                if let Some(default) = &p.default {
                                    return self.eval_const_expr_i64(default, local_types);
                                }
                            }
                        }
                    }
                }
                None
            }
            ExprKind::Unary(UnaryOp::Neg, inner) => {
                self.eval_const_expr_i64(inner, local_types)?.checked_neg()
            }
            ExprKind::Binary(BinOp::Add, lhs, rhs) => self
                .eval_const_expr_i64(lhs, local_types)?
                .checked_add(self.eval_const_expr_i64(rhs, local_types)?),
            ExprKind::Binary(BinOp::Sub, lhs, rhs) => self
                .eval_const_expr_i64(lhs, local_types)?
                .checked_sub(self.eval_const_expr_i64(rhs, local_types)?),
            ExprKind::Binary(BinOp::Mul, lhs, rhs) => self
                .eval_const_expr_i64(lhs, local_types)?
                .checked_mul(self.eval_const_expr_i64(rhs, local_types)?),
            ExprKind::Binary(BinOp::Div, lhs, rhs) => {
                let l = self.eval_const_expr_i64(lhs, local_types)?;
                let r = self.eval_const_expr_i64(rhs, local_types)?;
                if r == 0 {
                    None
                } else {
                    l.checked_div(r)
                }
            }
            ExprKind::Binary(BinOp::Mod, lhs, rhs) => {
                let l = self.eval_const_expr_i64(lhs, local_types)?;
                let r = self.eval_const_expr_i64(rhs, local_types)?;
                if r == 0 {
                    None
                } else {
                    l.checked_rem(r)
                }
            }
            ExprKind::Clog2(arg) => {
                let v = self.eval_const_expr_i64(arg, local_types)?;
                if v <= 1 {
                    Some(1)
                } else {
                    Some((64 - ((v as u64) - 1).leading_zeros()) as i64)
                }
            }
            _ => None,
        }
    }

    fn resolve_param_sized_literal_width(
        &mut self,
        name: &str,
        expr: &Expr,
        local_types: &HashMap<String, Ty>,
    ) -> Option<u32> {
        match self.eval_const_expr_i64(
            &Expr::new(ExprKind::Ident(name.to_string()), expr.span),
            local_types,
        ) {
            Some(width) if width > 0 => Some(width as u32),
            Some(width) => {
                self.errors.push(CompileError::General {
                    message: format!(
                        "sized literal width param `{name}` must be greater than zero, got {width}"
                    ),
                    span: crate::diagnostics::span_to_source_span(expr.span),
                });
                None
            }
            None => {
                self.errors.push(CompileError::General {
                    message: format!(
                        "sized literal width param `{name}` must resolve to a positive integer constant"
                    ),
                    span: crate::diagnostics::span_to_source_span(expr.span),
                });
                None
            }
        }
    }

    // ── CDC helpers ────────────────────────────────────────────────────────

    /// Check CDC violations across an instance boundary.
    /// Phase 2a RDC: data-path reset-domain crossing detection.
    ///
    /// Sync and reset-none flops are *transparent* for propagation
    /// (originate no domain, just forward whatever async domains reach
    /// their data input). The strict textbook rule: a flop downstream
    /// of an async-reset flop must itself be async-reset by the SAME
    /// signal — sync and reset-none flops can't gate their data input
    /// on the source's async reset event, so they capture mid-deassert
    /// transients and propagate metastability downstream.
    ///
    ///   reach[f] = { f.reset } if f.reset_kind == Async
    ///            = ⋃ reach[srcs] over data-flow sources otherwise
    ///   violation:
    ///     f.Async        and any reach[src] contains a domain ≠ f.reset
    ///     f.Sync         and reach[f] is non-empty
    ///     f.None         and reach[f] is non-empty
    pub(crate) fn check_rdc_phase2a(&mut self, m: &ModuleDecl) {
        use std::collections::HashSet;

        // 1. Build flop info (reset signal name + kind) for every flop in
        //    the module. Both inline `reg` decls and `port reg` decls
        //    participate. Flops carry a span we point at on violation.
        //    `guard_sig` is the optional `guard <NAME>` annotation (issue
        //    #260) — recognized as a flop-granular RDC waiver when the
        //    guard signal is itself async-reset on the same domain whose
        //    crossing would otherwise be flagged.
        struct FlopInfo {
            reset_sig: Option<String>,
            reset_kind: Option<ResetKind>,
            decl_span: crate::lexer::Span,
            guard_sig: Option<String>,
        }

        let async_resets: HashSet<String> = m
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Reset(ResetKind::Async, _) = &p.ty {
                    Some(p.name.name.clone())
                } else {
                    None
                }
            })
            .collect();
        let sync_resets: HashSet<String> = m
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Reset(ResetKind::Sync, _) = &p.ty {
                    Some(p.name.name.clone())
                } else {
                    None
                }
            })
            .collect();
        let kind_of_reset = |sig: &str| -> Option<ResetKind> {
            if async_resets.contains(sig) {
                Some(ResetKind::Async)
            } else if sync_resets.contains(sig) {
                Some(ResetKind::Sync)
            } else {
                None
            }
        };

        let mut flop_info: HashMap<String, FlopInfo> = HashMap::new();
        let extract_reset_sig = |r: &RegReset| -> Option<String> {
            match r {
                RegReset::None => None,
                RegReset::Explicit(s, _, _, _) => Some(s.name.clone()),
                RegReset::Inherit(s, _) => Some(s.name.clone()),
            }
        };
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(rd) = item {
                let sig = extract_reset_sig(&rd.reset);
                let kind = sig.as_deref().and_then(kind_of_reset);
                flop_info.insert(
                    rd.name.name.clone(),
                    FlopInfo {
                        reset_sig: sig,
                        reset_kind: kind,
                        decl_span: rd.name.span,
                        guard_sig: rd.guard.as_ref().map(|g| g.name.clone()),
                    },
                );
            }
        }
        for p in &m.ports {
            if let Some(ri) = &p.reg_info {
                let sig = extract_reset_sig(&ri.reset);
                let kind = sig.as_deref().and_then(kind_of_reset);
                flop_info.insert(
                    p.name.name.clone(),
                    FlopInfo {
                        reset_sig: sig,
                        reset_kind: kind,
                        decl_span: p.name.span,
                        guard_sig: ri.guard.as_ref().map(|g| g.name.clone()),
                    },
                );
            }
        }
        // Fast path: if no async-reset flops exist, no domain originated,
        // no violation possible. Skip the heavier work.
        let any_async = flop_info
            .values()
            .any(|fi| matches!(fi.reset_kind, Some(ResetKind::Async)));
        if !any_async {
            return;
        }

        let flop_set: HashSet<String> = flop_info.keys().cloned().collect();

        // 2. Build let-binding transitive flop reads. A `let x = expr;` is
        //    a combinational wire; if `expr` reads flop r, then any
        //    consumer reading `x` is effectively reading r.
        let lets: Vec<&LetBinding> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::LetBinding(l) = i {
                    Some(l)
                } else {
                    None
                }
            })
            .collect();
        let let_names: HashSet<String> = lets.iter().map(|l| l.name.name.clone()).collect();
        let mut let_deps: HashMap<String, HashSet<String>> = HashMap::new();
        for l in &lets {
            let mut reads = HashSet::new();
            Self::collect_expr_reads(&l.value, &mut reads);
            let direct: HashSet<String> = reads.intersection(&flop_set).cloned().collect();
            let_deps.insert(l.name.name.clone(), direct);
        }
        // Fixpoint: expand let-of-let.
        let mut changed = true;
        while changed {
            changed = false;
            for l in &lets {
                let mut reads = HashSet::new();
                Self::collect_expr_reads(&l.value, &mut reads);
                let mut to_add: HashSet<String> = HashSet::new();
                for r in &reads {
                    if let_names.contains(r) {
                        if let Some(deps) = let_deps.get(r) {
                            to_add.extend(deps.iter().cloned());
                        }
                    }
                }
                let entry = let_deps.get_mut(&l.name.name).unwrap();
                let before = entry.len();
                entry.extend(to_add);
                if entry.len() != before {
                    changed = true;
                }
            }
        }

        // 3. Build per-flop data-flow deps: for each `dst <= rhs` in any
        //    seq block, collect rhs reads; flops feed directly, lets feed
        //    transitively via let_deps.
        let mut flop_deps: HashMap<String, HashSet<String>> = HashMap::new();
        for f in &flop_set {
            flop_deps.insert(f.clone(), HashSet::new());
        }

        fn walk_seq_assigns(
            stmts: &[Stmt],
            flop_set: &HashSet<String>,
            let_names: &HashSet<String>,
            let_deps: &HashMap<String, HashSet<String>>,
            flop_deps: &mut HashMap<String, HashSet<String>>,
        ) {
            for s in stmts {
                match s {
                    Stmt::Assign(a) => {
                        if let ExprKind::Ident(target) = &a.target.kind {
                            if flop_set.contains(target) {
                                let mut reads = HashSet::new();
                                TypeChecker::collect_expr_reads(&a.value, &mut reads);
                                let entry = flop_deps.get_mut(target).unwrap();
                                for r in reads {
                                    if flop_set.contains(&r) {
                                        entry.insert(r);
                                    } else if let_names.contains(&r) {
                                        if let Some(d) = let_deps.get(&r) {
                                            entry.extend(d.iter().cloned());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Stmt::IfElse(ie) => {
                        walk_seq_assigns(&ie.then_stmts, flop_set, let_names, let_deps, flop_deps);
                        walk_seq_assigns(&ie.else_stmts, flop_set, let_names, let_deps, flop_deps);
                    }
                    Stmt::Match(mm) => {
                        for arm in &mm.arms {
                            walk_seq_assigns(&arm.body, flop_set, let_names, let_deps, flop_deps);
                        }
                    }
                    Stmt::For(f) => {
                        walk_seq_assigns(&f.body, flop_set, let_names, let_deps, flop_deps);
                    }
                    _ => {}
                }
            }
        }
        for item in &m.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                walk_seq_assigns(&rb.stmts, &flop_set, &let_names, &let_deps, &mut flop_deps);
            }
        }

        // 4. Compute reach via fixpoint.
        //    - Async flops: reach is fixed at { self.reset }.
        //    - Sync/None flops: reach = ⋃ reach[deps].
        let mut reach: HashMap<String, HashSet<String>> = HashMap::new();
        for (name, info) in &flop_info {
            let mut s = HashSet::new();
            if matches!(info.reset_kind, Some(ResetKind::Async)) {
                if let Some(sig) = &info.reset_sig {
                    s.insert(sig.clone());
                }
            }
            reach.insert(name.clone(), s);
        }
        let mut changed = true;
        while changed {
            changed = false;
            for (name, info) in &flop_info {
                if matches!(info.reset_kind, Some(ResetKind::Async)) {
                    continue;
                }
                let deps = flop_deps.get(name).cloned().unwrap_or_default();
                let mut new_reach: HashSet<String> = HashSet::new();
                for src in &deps {
                    if let Some(r) = reach.get(src) {
                        new_reach.extend(r.iter().cloned());
                    }
                }
                let cur = reach.get_mut(name).unwrap();
                if cur != &new_reach {
                    *cur = new_reach;
                    changed = true;
                }
            }
        }

        // 5. Detect violations and emit diagnostics. Sort domain names in
        //    error messages for deterministic output across HashSet iteration.
        let mut sorted_flops: Vec<&String> = flop_info.keys().collect();
        sorted_flops.sort();
        for name in sorted_flops {
            let info = &flop_info[name];
            let deps = flop_deps.get(name).cloned().unwrap_or_default();
            match (&info.reset_sig, info.reset_kind) {
                (Some(my_reset), Some(ResetKind::Async)) => {
                    // Async flop: any source reaching a foreign domain is a violation.
                    let mut foreign: Vec<String> = Vec::new();
                    for src in &deps {
                        if let Some(r) = reach.get(src) {
                            for d in r {
                                if d != my_reset && !foreign.contains(d) {
                                    foreign.push(d.clone());
                                }
                            }
                        }
                    }
                    if !foreign.is_empty() {
                        foreign.sort();
                        self.errors.push(CompileError::general(
                            &format!(
                                "RDC violation: register `{name}` is reset by async signal \
                                 `{my_reset}` but its data input transitively reads from \
                                 register(s) reset by async signal(s) {} — async reset \
                                 domains cannot be crossed without a `synchronizer kind reset`.",
                                foreign
                                    .iter()
                                    .map(|d| format!("`{d}`"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            info.decl_span,
                        ));
                    }
                }
                _ => {
                    // Sync / None flop: any async reach is a violation.
                    // The clock-edge capture isn't gated on the upstream's
                    // async reset event, so mid-deassert transients land
                    // in this flop and metastability propagates downstream.
                    let r = reach.get(name).cloned().unwrap_or_default();
                    if !r.is_empty() {
                        // Issue #260: `guard VALID_SIG` waiver — when the
                        // flop carries a guard annotation AND the guard
                        // signal is itself async-reset, downstream readers
                        // structurally gate the data on the guard, so the
                        // metastability hazard is contained without
                        // requiring this flop to be reset. The guard's
                        // async-reset domain must cover all reach domains
                        // for the waiver to apply (the guard goes off in
                        // those domains, gating the unreset data).
                        let waived_by_guard = info.guard_sig.as_ref().and_then(|g| {
                            let gi = flop_info.get(g)?;
                            if !matches!(gi.reset_kind, Some(ResetKind::Async)) {
                                return None;
                            }
                            let g_reset = gi.reset_sig.as_ref()?;
                            // Every reach domain must equal the guard's
                            // reset signal — otherwise the guard doesn't
                            // protect the foreign domain(s) we cross.
                            if r.iter().all(|d| d == g_reset) {
                                Some((g.clone(), g_reset.clone()))
                            } else {
                                None
                            }
                        });
                        if waived_by_guard.is_some() {
                            continue;
                        }

                        let mut domains: Vec<String> = r.into_iter().collect();
                        domains.sort();
                        let kind_label = match info.reset_kind {
                            Some(ResetKind::Sync) => "sync-reset",
                            None => "reset-none",
                            Some(ResetKind::Async) => unreachable!(),
                        };
                        let domain_phrase = if domains.len() == 1 {
                            format!("async reset domain `{}`", domains[0])
                        } else {
                            format!(
                                "multiple async reset domains ({})",
                                domains
                                    .iter()
                                    .map(|d| format!("`{d}`"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        };
                        // Hint about the issue-#260 waiver path when the
                        // user has a guard but it doesn't qualify (so they
                        // can fix the guard's reset rather than reaching
                        // for `pragma rdc_safe`).
                        let guard_hint = match (
                            &info.guard_sig,
                            info.guard_sig.as_ref().and_then(|g| flop_info.get(g)),
                        ) {
                            (Some(g), Some(gi))
                                if !matches!(gi.reset_kind, Some(ResetKind::Async)) =>
                            {
                                format!(" (Note: `guard {g}` is present but `{g}` is not async-reset, so the guard waiver does not apply.)")
                            }
                            (Some(g), None) => {
                                format!(" (Note: `guard {g}` is present but `{g}` is not a register in this module, so the guard waiver does not apply.)")
                            }
                            _ => String::new(),
                        };
                        self.errors.push(CompileError::general(
                            &format!(
                                "RDC violation: {kind_label} register `{name}` captures data \
                                 reaching from {domain_phrase}. The clock-edge capture is not \
                                 gated on the upstream's async reset event, so mid-deassert \
                                 transients metastabilise and propagate downstream. Either \
                                 reset `{name}` async-by the same signal, or insert a \
                                 `synchronizer kind reset` upstream, or annotate `{name}` \
                                 with `guard <VALID_SIG>` where `<VALID_SIG>` is async-reset.{guard_hint}"
                            ),
                            info.decl_span,
                        ));
                    }
                }
            }
        }

        // ── Phase 2b: clock-gating cell enable must not see async reach ──
        // Per article-3 hazard: an async-reset flop driving a `clkgate`
        // enable (or any logic that derives the enable) causes the gate
        // to glitch on async reset events, producing partial/glitchy
        // clock pulses on `clk_out`. Walk every inst whose target
        // construct is a `clkgate` and compute reach for the parent-side
        // signal driving its `enable` port. Non-empty → violation.
        let clkgate_constructs: HashSet<String> = self
            .source
            .items
            .iter()
            .filter_map(|it| {
                if let Item::Clkgate(c) = it {
                    Some(c.name.name.clone())
                } else {
                    None
                }
            })
            .collect();
        if !clkgate_constructs.is_empty() {
            let reach_for_signal = |sig: &Expr| -> HashSet<String> {
                let mut reads = HashSet::new();
                Self::collect_expr_reads(sig, &mut reads);
                let mut acc: HashSet<String> = HashSet::new();
                for r in &reads {
                    if let Some(rr) = reach.get(r) {
                        acc.extend(rr.iter().cloned());
                    }
                    if let Some(ld) = let_deps.get(r) {
                        for f in ld {
                            if let Some(rr) = reach.get(f) {
                                acc.extend(rr.iter().cloned());
                            }
                        }
                    }
                }
                acc
            };
            for item in &m.body {
                if let ModuleBodyItem::Inst(inst) = item {
                    if !clkgate_constructs.contains(&inst.module_name.name) {
                        continue;
                    }
                    for conn in &inst.connections {
                        if conn.port_name.name != "enable" {
                            continue;
                        }
                        let domains = reach_for_signal(&conn.signal);
                        if !domains.is_empty() {
                            let mut sorted: Vec<String> = domains.into_iter().collect();
                            sorted.sort();
                            let domain_phrase = if sorted.len() == 1 {
                                format!("async reset domain `{}`", sorted[0])
                            } else {
                                format!(
                                    "async reset domains ({})",
                                    sorted
                                        .iter()
                                        .map(|d| format!("`{d}`"))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            };
                            self.errors.push(CompileError::general(
                                &format!(
                                    "RDC violation: clkgate `{}` (instance of `{}`) has its \
                                     `enable` driven by logic in {domain_phrase}. The async \
                                     reset event causes `enable` to glitch, producing partial \
                                     clock pulses on the gated output. Drive `enable` from a \
                                     synchronously-clean source (a flop reset by the gated \
                                     clock's domain reset, or via a `synchronizer kind reset`).",
                                    inst.name.name, inst.module_name.name
                                ),
                                conn.span,
                            ));
                        }
                    }
                }
            }
        }

        // (Phase 2c — reconvergent RDC — runs as its own method called
        // from check_module so it isn't gated on this module having any
        // async-reset flops. The hazard lives at the synchronizer inst
        // boundary, not at the receiving flops.)
    }

    /// Reconvergent synchronisers — "loss of functional correlation".
    /// One source signal routed through two or more `synchronizer`
    /// instances (any kind) all targeting the same destination clock
    /// domain produces multiple synced outputs that can settle on
    /// different cycles in that domain. Logic consuming both outputs
    /// ends up in inconsistent state during the convergence window.
    ///
    /// The hazard's shape is identical for reset synchronisers (RDC
    /// variant — phase 2c) and data synchronisers (CDC variant — the
    /// reconvergent CDC class also listed in spec §5.2a). One method
    /// handles both; the diagnostic reports "RDC" for `kind reset` and
    /// "CDC" for the rest.
    ///
    /// Detection: walk every `inst x: Y` where Y is a synchroniser
    /// construct, read the `data_in` connection's parent-side ident
    /// (source signal) and the `dst_clk` connection's ident (look up
    /// its clock domain). Group by `(source, dest_domain)`. Any group
    /// with ≥ 2 insts is a violation. Same-source / different-domain
    /// is the legitimate fan-out pattern (one sync per receiving
    /// domain) and is not flagged.
    pub(crate) fn check_reconvergent_syncs(&mut self, m: &ModuleDecl) {
        // Map synchroniser construct name → its kind, so we can
        // classify each violating group as RDC vs CDC for the
        // diagnostic. A heterogeneous group (some reset, some data
        // synchronisers off the same source) gets the more general
        // "RDC/CDC" wording.
        let sync_kinds: HashMap<String, SyncKind> = self
            .source
            .items
            .iter()
            .filter_map(|it| {
                if let Item::Synchronizer(s) = it {
                    Some((s.name.name.clone(), s.kind))
                } else {
                    None
                }
            })
            .collect();
        if sync_kinds.is_empty() {
            return;
        }
        // Per-port clock domain map (rebuilt locally — independent of
        // phase 1's CDC gate so a single-clock module with reconvergent
        // syncs into that one domain still trips).
        let clk_domain: HashMap<String, String> = m
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Clock(domain) = &p.ty {
                    Some((p.name.name.clone(), domain.name.clone()))
                } else {
                    None
                }
            })
            .collect();
        // Build let-binding indirection map: `let x = expr;` lets the
        // source-tracing pass walk through `x` to its underlying source
        // registers, catching common-source-via-comb cases (Aldec article
        // 2140's bit-slice / common-source-register patterns).
        let let_map: HashMap<String, &Expr> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::LetBinding(l) = i {
                    Some((l.name.name.clone(), &l.value))
                } else {
                    None
                }
            })
            .collect();
        // Per-sync-instance terminal source-register set (after walking
        // through bit-slice, concat, unary/binary, let bindings). For
        // each terminal source ident, group by (ident, dest_domain).
        #[allow(clippy::type_complexity)]
        let mut groups: HashMap<
            (String, String),
            Vec<(String, SyncKind, crate::lexer::Span)>,
        > = HashMap::new();
        for item in &m.body {
            let ModuleBodyItem::Inst(inst) = item else {
                continue;
            };
            let Some(kind) = sync_kinds.get(&inst.module_name.name) else {
                continue;
            };
            let mut src_set: HashSet<String> = HashSet::new();
            let mut dst_clk_sig: Option<String> = None;
            for conn in &inst.connections {
                match conn.port_name.name.as_str() {
                    "data_in" => {
                        let mut visited = HashSet::new();
                        Self::collect_source_idents(
                            &conn.signal,
                            &let_map,
                            &mut visited,
                            &mut src_set,
                        );
                    }
                    "dst_clk" => {
                        if let ExprKind::Ident(n) = &conn.signal.kind {
                            dst_clk_sig = Some(n.clone());
                        }
                    }
                    _ => {}
                }
            }
            if src_set.is_empty() {
                continue;
            }
            let Some(clk) = dst_clk_sig else {
                continue;
            };
            let Some(dom) = clk_domain.get(&clk) else {
                continue;
            };
            for src in &src_set {
                groups.entry((src.clone(), dom.clone())).or_default().push((
                    inst.name.name.clone(),
                    *kind,
                    inst.span,
                ));
            }
        }
        // Sort for deterministic diagnostics across HashMap iteration.
        let mut sorted_keys: Vec<_> = groups.keys().cloned().collect();
        sorted_keys.sort();
        // Two syncs that share *multiple* terminal sources (e.g. both
        // read `data[0]` and `data[1]` after tracing through bit-slice)
        // would otherwise emit one error per shared source. Dedup by the
        // sorted set of inst names involved.
        let mut reported_inst_sets: HashSet<Vec<String>> = HashSet::new();
        for key in sorted_keys {
            let users = &groups[&key];
            if users.len() < 2 {
                continue;
            }
            let mut inst_set: Vec<String> = users.iter().map(|(n, _, _)| n.clone()).collect();
            inst_set.sort();
            inst_set.dedup();
            if inst_set.len() < 2 {
                continue;
            }
            if !reported_inst_sets.insert(inst_set.clone()) {
                continue;
            }
            let (source, domain) = key;
            let inst_list = inst_set
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let report_span = users[1].2;
            // Classify the hazard: pure-reset → RDC, no-reset → CDC,
            // mixed → RDC/CDC. Mixed is rare but not impossible (e.g.
            // a Bool gate signal fed into both a kind-ff sync and a
            // kind-reset sync).
            let any_reset = users.iter().any(|(_, k, _)| *k == SyncKind::Reset);
            let any_data = users.iter().any(|(_, k, _)| *k != SyncKind::Reset);
            let (label, sync_word, settle_word) = match (any_reset, any_data) {
                (true, false) => ("RDC", "reset synchronisers", "deassert"),
                (false, true) => ("CDC", "synchronisers", "settle"),
                _ => ("RDC/CDC", "synchronisers", "settle"),
            };
            self.errors.push(CompileError::general(
                &format!(
                    "{label} violation: source signal `{source}` is fed into multiple \
                     {sync_word} ({inst_list}) all targeting clock domain `{domain}`. \
                     The independent synchronisers can {settle_word} on different cycles in \
                     that domain, leaving downstream logic that consumes both outputs in \
                     inconsistent state (loss of functional correlation, reconvergent \
                     {label}). Use a single synchroniser per destination clock domain and \
                     fan out its output."
                ),
                report_span,
            ));
        }
    }

    /// Walk `expr` and collect the set of *terminal* source identifiers
    /// it ultimately reads — descending through bit-slice (`x[i]`,
    /// `x[hi:lo]`, `x[s +: w]`), field access, concat, unary/binary
    /// operators, ternaries, function/method calls, and let-binding
    /// indirection. A "terminal" ident is one that is *not* the LHS of
    /// any module-scope `let` (i.e. a port, register, wire, or sync
    /// output). Used by `check_reconvergent_syncs` to recognise the
    /// Aldec-2140 patterns where two synchronisers share a source after
    /// being split via combinational logic.
    fn collect_source_idents(
        expr: &Expr,
        let_map: &HashMap<String, &Expr>,
        visited: &mut HashSet<String>,
        out: &mut HashSet<String>,
    ) {
        match &expr.kind {
            ExprKind::Ident(name) | ExprKind::SynthIdent(name, _) => {
                if let Some(rhs) = let_map.get(name) {
                    if visited.insert(name.clone()) {
                        Self::collect_source_idents(rhs, let_map, visited, out);
                        visited.remove(name);
                    }
                } else {
                    out.insert(name.clone());
                }
            }
            ExprKind::Binary(_, l, r) => {
                Self::collect_source_idents(l, let_map, visited, out);
                Self::collect_source_idents(r, let_map, visited, out);
            }
            ExprKind::Unary(_, e) => Self::collect_source_idents(e, let_map, visited, out),
            ExprKind::Index(base, idx) => {
                Self::collect_source_idents(base, let_map, visited, out);
                Self::collect_source_idents(idx, let_map, visited, out);
            }
            ExprKind::BitSlice(base, _, _) => {
                Self::collect_source_idents(base, let_map, visited, out)
            }
            ExprKind::PartSelect(base, _, _, _) => {
                Self::collect_source_idents(base, let_map, visited, out)
            }
            ExprKind::FieldAccess(base, _) => {
                Self::collect_source_idents(base, let_map, visited, out)
            }
            ExprKind::Cast(e, _) => Self::collect_source_idents(e, let_map, visited, out),
            ExprKind::Signed(e) | ExprKind::Unsigned(e) => {
                Self::collect_source_idents(e, let_map, visited, out)
            }
            ExprKind::Concat(parts) => {
                for p in parts {
                    Self::collect_source_idents(p, let_map, visited, out);
                }
            }
            ExprKind::Repeat(n, e) => {
                Self::collect_source_idents(n, let_map, visited, out);
                Self::collect_source_idents(e, let_map, visited, out);
            }
            ExprKind::Ternary(c, t, e) => {
                Self::collect_source_idents(c, let_map, visited, out);
                Self::collect_source_idents(t, let_map, visited, out);
                Self::collect_source_idents(e, let_map, visited, out);
            }
            ExprKind::FunctionCall(_, args) => {
                for a in args {
                    Self::collect_source_idents(a, let_map, visited, out);
                }
            }
            ExprKind::MethodCall(base, _, args) => {
                Self::collect_source_idents(base, let_map, visited, out);
                for a in args {
                    Self::collect_source_idents(a, let_map, visited, out);
                }
            }
            ExprKind::Clog2(e) | ExprKind::Onehot(e) => {
                Self::collect_source_idents(e, let_map, visited, out)
            }
            _ => {}
        }
    }

    /// Phase 2d RDC: combiner-derived reset glitches at inst boundaries.
    ///
    /// A sub-module's `Reset<...>` input port wired by a combinational
    /// expression (e.g. `rst <- rst_a | rst_b`) sees transient pulses
    /// on edge skew between the inputs. The async-reset glitch can
    /// trigger partial flop resets in the sub-module — exactly the
    /// hazard mainstream RDC literature flags as "glitches from
    /// multi-source combiners". The ARCH type system prevents writing
    /// `let combined: Reset = ...` inside a module, but inst
    /// connections currently accept any Expr in the signal slot, so
    /// the gate is open at the boundary.
    ///
    /// Detection: walk every inst, look up the sub-module's port list,
    /// for each connection whose target port has type `Reset<...>`,
    /// inspect the parent-side signal's expression. If it's anything
    /// other than a simple `Ident` (which refers to a parent port,
    /// wire, or synchroniser output), flag. Idents are trusted —
    /// they're the legal direct routings; combinational shapes are
    /// the violators.
    ///
    /// Note: this check fires regardless of whether the parent signal
    /// is wired to two different reset domains or just one. A single-
    /// source negation (`rst <- !rst_a`) is also a glitch source on
    /// the boundary because the inverter has its own propagation
    /// delay relative to the original signal.
    pub(crate) fn check_rdc_combiner_at_inst(&mut self, m: &ModuleDecl) {
        // Look up the sub-construct's port list across every construct
        // kind that can be `inst`-ed (matches the lookup in mod.rs's
        // sim_codegen helper of the same name).
        let lookup_ports = |name: &str| -> Vec<PortDecl> {
            for item in &self.source.items {
                let ports = match item {
                    Item::Module(m) if m.name.name == name => Some(&m.ports),
                    Item::Fsm(f) if f.name.name == name => Some(&f.ports),
                    Item::Fifo(f) if f.name.name == name => Some(&f.ports),
                    Item::Ram(r) if r.name.name == name => Some(&r.ports),
                    Item::Cam(c) if c.name.name == name => Some(&c.ports),
                    Item::Counter(c) if c.name.name == name => Some(&c.ports),
                    Item::Arbiter(a) if a.name.name == name => Some(&a.ports),
                    Item::Regfile(r) if r.name.name == name => Some(&r.ports),
                    Item::Pipeline(p) if p.name.name == name => Some(&p.ports),
                    Item::Linklist(l) if l.name.name == name => Some(&l.ports),
                    Item::Synchronizer(s) if s.name.name == name => Some(&s.ports),
                    Item::Clkgate(c) if c.name.name == name => Some(&c.ports),
                    _ => None,
                };
                if let Some(p) = ports {
                    return p.clone();
                }
            }
            Vec::new()
        };
        for item in &m.body {
            let ModuleBodyItem::Inst(inst) = item else {
                continue;
            };
            let sub_ports = lookup_ports(&inst.module_name.name);
            for conn in &inst.connections {
                let port = sub_ports
                    .iter()
                    .find(|p| p.name.name == conn.port_name.name);
                let Some(port) = port else {
                    continue;
                };
                if !matches!(&port.ty, TypeExpr::Reset(_, _)) {
                    continue;
                }
                if conn.direction != ConnectDir::Input {
                    continue;
                }
                // Direct reset source → trust. A reset-type cast such as
                // `rst as Reset<Async, Low>` is an instantiation-time reset
                // annotation, not reset-combining logic; peel through it so
                // legacy reset-override examples stay legal. Real logic under
                // the cast, e.g. `(rst_a | rst_b) as Reset<Async>`, remains a
                // combiner and is still rejected.
                if Self::is_direct_reset_inst_signal(&conn.signal) {
                    continue;
                }
                self.errors.push(CompileError::general(
                    &format!(
                        "RDC violation: inst `{inst_name}` (instance of `{sub}`) has its \
                         `Reset`-typed port `{port_name}` driven by a combinational \
                         expression in the parent. Reset combiners (e.g. `rst_a | rst_b`) \
                         glitch on edge skew between their inputs and can trigger partial \
                         flop resets in the sub-module. Drive `{port_name}` from a single \
                         `Reset` source port (or a `synchronizer kind reset` output) and \
                         do any combination upstream through dedicated reset-merging logic.",
                        inst_name = inst.name.name,
                        sub = inst.module_name.name,
                        port_name = conn.port_name.name,
                    ),
                    conn.span,
                ));
            }
        }
    }

    fn is_direct_reset_inst_signal(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Ident(_) | ExprKind::SynthIdent(_, _) => true,
            ExprKind::Cast(inner, ty) if matches!(ty.as_ref(), TypeExpr::Reset(_, _)) => {
                Self::is_direct_reset_inst_signal(inner)
            }
            _ => false,
        }
    }

    /// For each data connection, verify that the signal's clock domain in the
    /// parent matches the port's clock domain in the child module.
    pub(crate) fn check_inst_cdc(
        &mut self,
        inst: &InstDecl,
        parent_clk_domain: &HashMap<String, String>,
        parent_reg_domain: &HashMap<String, String>,
        parent_module: &ModuleDecl,
    ) {
        // Find the instantiated module's definition
        let child_module = self.source.items.iter().find_map(|item| {
            if let Item::Module(m) = item {
                if m.name.name == inst.module_name.name {
                    Some(m)
                } else {
                    None
                }
            } else {
                None
            }
        });
        let child_module = match child_module {
            Some(m) => m,
            None => return, // Module not found in this file; skip
        };

        // Build child module's clock port → domain map
        let child_clk_domain: HashMap<String, String> = child_module
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Clock(domain) = &p.ty {
                    Some((p.name.name.clone(), domain.name.clone()))
                } else {
                    None
                }
            })
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
                        if child_module
                            .ports
                            .iter()
                            .any(|p| p.name.name == *read_name && p.direction == Direction::In)
                        {
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
                let domains: HashSet<&String> = reads
                    .iter()
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
        let conn_signal: HashMap<String, String> = inst
            .connections
            .iter()
            .filter_map(|c| {
                if let ExprKind::Ident(sig_name) = &c.signal.kind {
                    Some((c.port_name.name.clone(), sig_name.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Find which clock domain each inst clock port is connected to
        let inst_clk_mapping: HashMap<String, String> = inst
            .connections
            .iter()
            .filter_map(|c| {
                let child_port = child_module
                    .ports
                    .iter()
                    .find(|p| p.name.name == c.port_name.name)?;
                if let TypeExpr::Clock(_) = &child_port.ty {
                    if let ExprKind::Ident(sig_name) = &c.signal.kind {
                        parent_clk_domain
                            .get(sig_name)
                            .map(|d| (c.port_name.name.clone(), d.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // For each data connection, check domain compatibility
        for conn in &inst.connections {
            let port_name = &conn.port_name.name;

            // Skip clock and reset ports
            if let Some(child_port) = child_module
                .ports
                .iter()
                .find(|p| p.name.name == *port_name)
            {
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
            let expected_parent_domain =
                inst_clk_mapping
                    .iter()
                    .find_map(|(child_clk, parent_domain)| {
                        if child_clk_domain.get(child_clk) == Some(child_domain) {
                            Some(parent_domain.as_str())
                        } else {
                            None
                        }
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
                Stmt::For(f) => {
                    Self::collect_stmt_targets(&f.body, out);
                }
                Stmt::Init(ib) => {
                    Self::collect_stmt_targets(&ib.body, out);
                }
                Stmt::WaitUntil(_, _) => {}
                Stmt::DoUntil { body, .. } => {
                    Self::collect_stmt_targets(body, out);
                }
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
                    for arg in &l.args {
                        Self::collect_expr_reads(arg, out);
                    }
                }
                Stmt::For(f) => {
                    Self::collect_stmt_reads(&f.body, out);
                }
                Stmt::Init(ib) => {
                    Self::collect_stmt_reads(&ib.body, out);
                }
                Stmt::WaitUntil(expr, _) => {
                    Self::collect_expr_reads(expr, out);
                }
                Stmt::DoUntil { body, cond, .. } => {
                    Self::collect_stmt_reads(body, out);
                    Self::collect_expr_reads(cond, out);
                }
            }
        }
    }

    fn collect_expr_reads(expr: &Expr, out: &mut HashSet<String>) {
        match &expr.kind {
            ExprKind::Ident(name) => {
                out.insert(name.clone());
            }
            ExprKind::Binary(_, lhs, rhs) => {
                Self::collect_expr_reads(lhs, out);
                Self::collect_expr_reads(rhs, out);
            }
            ExprKind::Unary(_, e) => Self::collect_expr_reads(e, out),
            ExprKind::Index(base, idx) => {
                Self::collect_expr_reads(base, out);
                Self::collect_expr_reads(idx, out);
            }
            ExprKind::BitSlice(base, hi, lo) => {
                Self::collect_expr_reads(base, out);
                Self::collect_expr_reads(hi, out);
                Self::collect_expr_reads(lo, out);
            }
            ExprKind::PartSelect(base, start, width, _) => {
                Self::collect_expr_reads(base, out);
                Self::collect_expr_reads(start, out);
                Self::collect_expr_reads(width, out);
            }
            ExprKind::FieldAccess(base, _) => Self::collect_expr_reads(base, out),
            ExprKind::MethodCall(base, _, args) => {
                Self::collect_expr_reads(base, out);
                for a in args {
                    Self::collect_expr_reads(a, out);
                }
            }
            ExprKind::FunctionCall(_, args) => {
                for a in args {
                    Self::collect_expr_reads(a, out);
                }
            }
            ExprKind::Ternary(cond, then_e, else_e) => {
                Self::collect_expr_reads(cond, out);
                Self::collect_expr_reads(then_e, out);
                Self::collect_expr_reads(else_e, out);
            }
            ExprKind::Match(scrut, arms) => {
                Self::collect_expr_reads(scrut, out);
                for arm in arms {
                    Self::collect_stmt_reads(&arm.body, out);
                }
            }
            ExprKind::ExprMatch(scrut, arms) => {
                Self::collect_expr_reads(scrut, out);
                for arm in arms {
                    Self::collect_expr_reads(&arm.value, out);
                }
            }
            ExprKind::Inside(scrut, members) => {
                Self::collect_expr_reads(scrut, out);
                for m in members {
                    match m {
                        InsideMember::Single(e) => Self::collect_expr_reads(e, out),
                        InsideMember::Range(lo, hi) => {
                            Self::collect_expr_reads(lo, out);
                            Self::collect_expr_reads(hi, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Collect all identifier names read in comb statements.
    fn collect_comb_stmt_reads(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => Self::collect_expr_reads(&a.value, out),
                Stmt::IfElse(ie) => {
                    Self::collect_expr_reads(&ie.cond, out);
                    Self::collect_comb_stmt_reads(&ie.then_stmts, out);
                    Self::collect_comb_stmt_reads(&ie.else_stmts, out);
                }
                Stmt::Match(m) => {
                    Self::collect_expr_reads(&m.scrutinee, out);
                    for arm in &m.arms {
                        Self::collect_comb_stmt_reads(&arm.body, out);
                    }
                }
                Stmt::Log(l) => {
                    for arg in &l.args {
                        Self::collect_expr_reads(arg, out);
                    }
                }
                Stmt::For(f) => {
                    Self::collect_comb_stmt_reads(&f.body, out);
                }
                Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                    unreachable!("seq-only Stmt variant inside comb-context walker")
                }
            }
        }
    }

    /// Collect all target names assigned in comb statements.
    fn collect_comb_stmt_targets(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    let name = Self::expr_root_name_tc(&a.target);
                    if !name.is_empty() {
                        out.insert(name);
                    }
                }
                Stmt::IfElse(ie) => {
                    Self::collect_comb_stmt_targets(&ie.then_stmts, out);
                    Self::collect_comb_stmt_targets(&ie.else_stmts, out);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        Self::collect_comb_stmt_targets(&arm.body, out);
                    }
                }
                Stmt::Log(_) => {}
                Stmt::For(f) => {
                    Self::collect_comb_stmt_targets(&f.body, out);
                }
                Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                    unreachable!("seq-only Stmt variant inside comb-context walker")
                }
            }
        }
    }

    // Naming convention checks removed — style is a convention (LLM defaults
    // to snake_case), not a compiler-enforced rule.
    pub(crate) fn check_pascal_case(&mut self, _ident: &Ident) {}
    pub(crate) fn check_snake_case(&mut self, _ident: &Ident) {}
    pub(crate) fn check_upper_snake(&mut self, _ident: &Ident) {}

    /// Check that a WidthConst param's default value fits in the declared width.
    pub(crate) fn check_width_const_overflow(&mut self, p: &ParamDecl) {
        if let ParamKind::WidthConst(hi, lo) = &p.kind {
            let empty = std::collections::HashMap::new();
            if let (Some(h), Some(l), Some(default)) = (
                crate::elaborate::try_eval_i64(hi, &empty),
                crate::elaborate::try_eval_i64(lo, &empty),
                p.default
                    .as_ref()
                    .and_then(|d| crate::elaborate::try_eval_i64(d, &empty)),
            ) {
                let width = (h - l + 1).max(0) as u32;
                if width < 64 && default as u64 >= (1u64 << width) {
                    self.errors.push(CompileError::general(
                        &format!(
                            "param `{}` default value {} does not fit in declared width [{}:{}] ({} bits)",
                            p.name.name, default, h, l, width
                        ),
                        p.name.span,
                    ));
                }
            }
        }
    }

    // ── FSM ───────────────────────────────────────────────────────────────────

    pub(crate) fn check_fsm(&mut self, f: &FsmDecl) {
        self.check_pascal_case(&f.name);
        for p in &f.params {
            self.check_upper_snake(&p.name);
        }
        for p in &f.ports {
            self.check_snake_case(&p.name);
        }

        // Interface stub from a `.archi` file — body is empty by
        // construction. Skip body-driven checks (state coverage,
        // transition validation, output-driven, etc.) entirely; the
        // stub exists only to expose the port signature to parent-side
        // instantiation typechecking. Mirrors the same short-circuit in
        // `check_module`.
        if f.common.is_interface {
            return;
        }

        // `state` is reserved — the codegen maps it to the internal
        // `state_r` register via ident_subst. `state_r` is also reserved:
        // SV/native codegen expose that compiler-owned state register name
        // directly to support white-box probes. User declarations using either
        // name would collide with generated state storage.
        for p in &f.ports {
            if p.name.name == "state" {
                self.errors.push(CompileError::general(
                    "'state' is reserved in fsm (the codegen maps it to the internal state_r register). Rename the port (e.g. 'state_o').",
                    p.name.span,
                ));
            }
            if p.name.name == "state_r" {
                self.errors.push(CompileError::general(
                    "'state_r' is reserved in fsm for compiler-generated state storage. Rename the port (e.g. 'state_o').",
                    p.name.span,
                ));
            }
        }
        for r in &f.regs {
            if r.name.name == "state" {
                self.errors.push(CompileError::general(
                    "'state' is reserved in fsm. Rename the signal (e.g. 'state_r').",
                    r.name.span,
                ));
            }
            if r.name.name == "state_r" {
                self.errors.push(CompileError::general(
                    "'state_r' is reserved in fsm for compiler-generated state storage. Rename the signal.",
                    r.name.span,
                ));
            }
        }
        for w in &f.wires {
            if w.name.name == "state" {
                self.errors.push(CompileError::general(
                    "'state' is reserved in fsm. Rename the signal.",
                    w.name.span,
                ));
            }
            if w.name.name == "state_r" {
                self.errors.push(CompileError::general(
                    "'state_r' is reserved in fsm for compiler-generated state storage. Rename the signal.",
                    w.name.span,
                ));
            }
        }
        for l in &f.lets {
            if l.name.name == "state" {
                self.errors.push(CompileError::general(
                    "'state' is reserved in fsm. Rename the binding.",
                    l.name.span,
                ));
            }
            if l.name.name == "state_r" {
                self.errors.push(CompileError::general(
                    "'state_r' is reserved in fsm for compiler-generated state storage. Rename the binding.",
                    l.name.span,
                ));
            }
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
                    &format!(
                        "state `{}` has no transitions (dead-end state)",
                        sb.name.name
                    ),
                    sb.name.span,
                ));
            }
            // All output ports must be driven in each state, unless they have
            // a `default` value declared (in which case the FSM codegen emits
            // the default and the per-state block only needs to override it).
        }
    }

    // ── RAM ───────────────────────────────────────────────────────────────────

    pub(crate) fn check_cam(&mut self, c: &CamDecl) {
        // Phase A: minimal naming check + presence of required params/ports.
        // Full validation (port widths from $clog2(DEPTH), $clog2(KEY_W),
        // exact port name list) deferred to Phase A continuation.
        // v2 (cam-dual-write): if any of write2_{valid,idx,key,set} is
        // declared, all four must be present (all-or-nothing).
        self.check_pascal_case(&c.name);
        for p in &c.params {
            self.check_upper_snake(&p.name);
        }
        for p in &c.ports {
            self.check_snake_case(&p.name);
        }
        let has_depth = c.params.iter().any(|p| p.name.name == "DEPTH");
        let has_key_w = c.params.iter().any(|p| p.name.name == "KEY_W");
        if !has_depth {
            self.errors.push(CompileError::general(
                "cam: missing required `param DEPTH: const = N;`",
                c.name.span,
            ));
        }
        if !has_key_w {
            self.errors.push(CompileError::general(
                "cam: missing required `param KEY_W: const = N;`",
                c.name.span,
            ));
        }
        // v2: optional dual-write port. If any write2_* port is present,
        // all four must be, so codegen can assume the full bundle.
        let w2_names = ["write2_valid", "write2_idx", "write2_key", "write2_set"];
        let w2_present: Vec<bool> = w2_names
            .iter()
            .map(|n| c.ports.iter().any(|p| p.name.name == *n))
            .collect();
        let has_w2 = w2_present.iter().any(|b| *b);
        if has_w2 && !w2_present.iter().all(|b| *b) {
            let missing: Vec<&str> = w2_names
                .iter()
                .zip(&w2_present)
                .filter(|(_, present)| !**present)
                .map(|(name, _)| *name)
                .collect();
            self.errors.push(CompileError::general(
                &format!(
                    "cam: dual-write port is all-or-nothing — missing port(s): {}",
                    missing.join(", ")
                ),
                c.name.span,
            ));
        }
        // v3: optional value payload. Activation = VAL_W param + write_value
        // + read_value (and write2_value if dual-write is enabled).
        let has_val_w = c.params.iter().any(|p| p.name.name == "VAL_W");
        let has_write_val = c.ports.iter().any(|p| p.name.name == "write_value");
        let has_read_val = c.ports.iter().any(|p| p.name.name == "read_value");
        let has_w2_val = c.ports.iter().any(|p| p.name.name == "write2_value");
        if has_val_w || has_write_val || has_read_val || has_w2_val {
            // Any one present → all required (matched to the active write port set).
            let mut missing: Vec<&str> = Vec::new();
            if !has_val_w {
                missing.push("param VAL_W");
            }
            if !has_write_val {
                missing.push("port write_value");
            }
            if !has_read_val {
                missing.push("port read_value");
            }
            if has_w2 && !has_w2_val {
                missing.push("port write2_value");
            }
            if !missing.is_empty() {
                self.errors.push(CompileError::general(
                    &format!(
                        "cam: value-type bundle is all-or-nothing — missing: {}",
                        missing.join(", ")
                    ),
                    c.name.span,
                ));
            }
        }
        // Reject value-side ports declared without VAL_W (caught above when
        // VAL_W is missing) or write2_value without dual-write.
        if has_w2_val && !has_w2 {
            self.errors.push(CompileError::general(
                "cam: `write2_value` requires the full dual-write port set (write2_{valid,idx,key,set})",
                c.name.span,
            ));
        }
    }

    pub(crate) fn check_ram(&mut self, r: &RamDecl) {
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
        // latency must be 0 (async), 1 (sync), or 2 (sync_out). Higher
        // values are unsupported by codegen — the SV/sim emitters only
        // handle 0/1/2 and would otherwise silently leave `rdata` outputs
        // undriven. Reject loudly instead.
        if r.latency > 2 {
            self.errors.push(CompileError::general(
                &format!(
                    "ram `{}`: latency {} is out of range — must be 0 (async), 1 (sync), or 2 (sync_out)",
                    r.name.name, r.latency
                ),
                r.name.span,
            ));
        }
        // true_dual requires exactly 2 port groups
        if r.kind == crate::ast::RamKind::TrueDual && r.port_groups.len() != 2 {
            self.errors.push(CompileError::general(
                &format!(
                    "true_dual ram `{}` must have exactly 2 port groups",
                    r.name.name
                ),
                r.name.span,
            ));
        }
        // simple_dual requires exactly 2 port groups
        if r.kind == crate::ast::RamKind::SimpleDual && r.port_groups.len() != 2 {
            self.errors.push(CompileError::general(
                &format!(
                    "simple_dual ram `{}` must have exactly 2 port groups",
                    r.name.name
                ),
                r.name.span,
            ));
        }
        // ROM validation
        if r.kind == crate::ast::RamKind::Rom {
            // ROM must have init
            if r.init.is_none() {
                self.errors.push(CompileError::general(
                    &format!("rom `{}` must have an init clause", r.name.name),
                    r.name.span,
                ));
            }
            // ROM must not have write signals
            for pg in &r.port_groups {
                for s in &pg.signals {
                    if s.name.name == "wen" || s.name.name == "wdata" {
                        self.errors.push(CompileError::general(
                            &format!(
                                "rom `{}` must not have write signal `{}`",
                                r.name.name, s.name.name
                            ),
                            s.name.span,
                        ));
                    }
                }
            }
        }
    }

    // ── FIFO ──────────────────────────────────────────────────────────────────

    pub(crate) fn check_fifo(&mut self, f: &FifoDecl) {
        self.check_pascal_case(&f.name);
        for p in &f.params {
            self.check_upper_snake(&p.name);
        }
        for p in &f.ports {
            self.check_snake_case(&p.name);
        }

        // Required port names
        let required = [
            "push_valid",
            "push_ready",
            "push_data",
            "pop_valid",
            "pop_ready",
            "pop_data",
        ];
        let present: Vec<&str> = f.ports.iter().map(|p| p.name.name.as_str()).collect();
        for req in &required {
            if !present.contains(req) {
                self.errors.push(CompileError::general(
                    &format!("fifo `{}` is missing required port `{req}`", f.name.name),
                    f.name.span,
                ));
            }
        }

        // Require a type parameter for memory element width.
        // Without it, push_data/pop_data widths won't propagate to the
        // internal memory array, producing silently wrong codegen.
        let has_type_param = f
            .params
            .iter()
            .any(|p| matches!(p.kind, crate::ast::ParamKind::Type(_)));
        if !has_type_param {
            self.errors.push(CompileError::general(
                &format!(
                    "fifo `{}` requires a `param NAME: type = UInt<N>;` to set memory element width.\n  \
                     push_data and pop_data ports must use this type parameter, e.g. `in WIDTH`.",
                    f.name.name
                ),
                f.name.span,
            ));
        }

        // LIFO must be single-clock (synchronous)
        if f.kind == FifoKind::Lifo {
            let is_async = crate::resolve::detect_async_fifo(&f.ports);
            if is_async {
                self.errors.push(CompileError::general(
                    &format!("lifo `{}` must be single-clock (synchronous); dual-clock lifo is not supported", f.name.name),
                    f.name.span,
                ));
            }
        }
    }

    // ── Synchronizer ─────────────────────────────────────────────────────────

    pub(crate) fn check_synchronizer(&mut self, s: &SynchronizerDecl) {
        self.check_pascal_case(&s.name);
        for p in &s.params {
            self.check_upper_snake(&p.name);
        }
        for p in &s.ports {
            self.check_snake_case(&p.name);
        }

        // Must have exactly two clock ports from different domains
        let clk_ports: Vec<(&Ident, &Ident)> = s
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Clock(domain) = &p.ty {
                    Some((&p.name, domain))
                } else {
                    None
                }
            })
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
                    &format!(
                        "synchronizer `{}` is missing required port `{req}`",
                        s.name.name
                    ),
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
                            &format!(
                                "synchronizer `{}`: STAGES must be >= 2 (got {})",
                                s.name.name, v
                            ),
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
                        &format!(
                            "synchronizer `{}`: `kind reset` requires single-bit (Bool) data ports",
                            s.name.name
                        ),
                        data_in.span,
                    ));
                }
                SyncKind::Pulse if !is_single_bit => {
                    self.errors.push(CompileError::general(
                        &format!(
                            "synchronizer `{}`: `kind pulse` requires single-bit (Bool) data ports",
                            s.name.name
                        ),
                        data_in.span,
                    ));
                }
                _ => {}
            }
        }
    }

    // ── Clock Gate ─────────────────────────────────────────────────────────────

    pub(crate) fn check_clkgate(&mut self, c: &crate::ast::ClkGateDecl) {
        self.check_pascal_case(&c.name);
        for p in &c.params {
            self.check_upper_snake(&p.name);
        }
        for p in &c.ports {
            self.check_snake_case(&p.name);
        }

        // Must have exactly one Clock input and one Clock output with matching domain
        let clk_in_ports: Vec<&crate::ast::PortDecl> = c
            .ports
            .iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::In)
            .collect();
        let clk_out_ports: Vec<&crate::ast::PortDecl> = c
            .ports
            .iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::Out)
            .collect();

        if clk_in_ports.len() != 1 {
            self.errors.push(CompileError::general(
                &format!(
                    "clkgate `{}` must have exactly 1 Clock input port",
                    c.name.name
                ),
                c.name.span,
            ));
        }
        if clk_out_ports.len() != 1 {
            self.errors.push(CompileError::general(
                &format!(
                    "clkgate `{}` must have exactly 1 Clock output port",
                    c.name.name
                ),
                c.name.span,
            ));
        }

        // Check domains match
        if clk_in_ports.len() == 1 && clk_out_ports.len() == 1 {
            if let (TypeExpr::Clock(d_in), TypeExpr::Clock(d_out)) =
                (&clk_in_ports[0].ty, &clk_out_ports[0].ty)
            {
                if d_in.name != d_out.name {
                    self.errors.push(CompileError::general(
                        &format!("clkgate `{}`: input clock domain `{}` must match output clock domain `{}`",
                                 c.name.name, d_in.name, d_out.name),
                        c.name.span,
                    ));
                }
            }
        }

        // Must have enable port (Bool input)
        let has_enable = c
            .ports
            .iter()
            .any(|p| p.name.name == "enable" && p.direction == Direction::In);
        if !has_enable {
            self.errors.push(CompileError::general(
                &format!(
                    "clkgate `{}` is missing required `enable: in Bool` port",
                    c.name.name
                ),
                c.name.span,
            ));
        }
    }

    // ── Counter ───────────────────────────────────────────────────────────────

    pub(crate) fn check_counter(&mut self, c: &crate::ast::CounterDecl) {
        self.check_pascal_case(&c.name);
        for p in &c.params {
            self.check_upper_snake(&p.name);
        }
        for p in &c.ports {
            self.check_snake_case(&p.name);
        }
    }

    // ── Arbiter ───────────────────────────────────────────────────────────────

    pub(crate) fn check_arbiter(&mut self, a: &crate::ast::ArbiterDecl) {
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
                    &format!(
                        "custom policy `{}` requires a `hook grant_select` declaration",
                        fn_ident.name
                    ),
                    fn_ident.span,
                ));
                return;
            }
            let hook = a.hook.as_ref().unwrap();
            // Verify the hook's bound function name matches the policy name
            if hook.fn_name.name != fn_ident.name {
                self.errors.push(CompileError::general(
                    &format!(
                        "hook function `{}` does not match policy name `{}`",
                        hook.fn_name.name, fn_ident.name
                    ),
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
            let hook_param_names: Vec<&str> =
                hook.params.iter().map(|p| p.name.name.as_str()).collect();
            // Hook parameter names must not shadow arbiter port names — the
            // codegen emits the function inside the module, so a name collision
            // produces SV VARHIDDEN warnings.
            for hp in &hook.params {
                if port_names.contains(&hp.name.name.as_str()) {
                    self.errors.push(CompileError::general(
                        &format!(
                            "hook parameter `{}` shadows arbiter port of the same name. \
                             Rename the hook parameter (e.g. `{}s`) or the port.",
                            hp.name.name, hp.name.name,
                        ),
                        hp.name.span,
                    ));
                }
            }
            for arg in &hook.fn_args {
                if !hook_param_names.contains(&arg.name.as_str())
                    && !port_names.contains(&arg.name.as_str())
                    && !param_names.contains(&arg.name.as_str())
                {
                    self.errors.push(CompileError::general(
                        &format!(
                            "hook argument `{}` is not a hook parameter, port, or param",
                            arg.name
                        ),
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

    pub(crate) fn check_regfile(&mut self, r: &crate::ast::RegfileDecl) {
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

    /// Walk a body item and report any `<Stage>.<field>` reference
    /// whose target stage is more than one hop from the owning stage.
    /// Stall conditions, flush directives, and forward directives are
    /// hazard signals and live outside the body — they're intentionally
    /// exempt from this check.
    fn collect_pipeline_cross_stage_refs(
        &mut self,
        item: &ModuleBodyItem,
        cur_idx: usize,
        stage_idx: &HashMap<&str, usize>,
        cur_name: &str,
    ) {
        match item {
            ModuleBodyItem::CombBlock(cb) => {
                for s in &cb.stmts {
                    self.walk_pipeline_comb_stmt(s, cur_idx, stage_idx, cur_name);
                }
            }
            ModuleBodyItem::RegBlock(rb) => {
                for s in &rb.stmts {
                    self.walk_pipeline_stmt(s, cur_idx, stage_idx, cur_name);
                }
            }
            ModuleBodyItem::LetBinding(lb) => {
                self.check_pipeline_cross_stage_expr(&lb.value, cur_idx, stage_idx, cur_name);
            }
            ModuleBodyItem::RegDecl(rd) => {
                if let Some(init) = &rd.init {
                    self.check_pipeline_cross_stage_expr(init, cur_idx, stage_idx, cur_name);
                }
                if let RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) = &rd.reset {
                    self.check_pipeline_cross_stage_expr(val, cur_idx, stage_idx, cur_name);
                }
            }
            _ => {}
        }
    }

    fn walk_pipeline_comb_stmt(
        &mut self,
        s: &Stmt,
        cur_idx: usize,
        stage_idx: &HashMap<&str, usize>,
        cur_name: &str,
    ) {
        match s {
            Stmt::Assign(a) => {
                self.check_pipeline_cross_stage_expr(&a.value, cur_idx, stage_idx, cur_name);
            }
            Stmt::IfElse(ie) => {
                self.check_pipeline_cross_stage_expr(&ie.cond, cur_idx, stage_idx, cur_name);
                for s in &ie.then_stmts {
                    self.walk_pipeline_comb_stmt(s, cur_idx, stage_idx, cur_name);
                }
                for s in &ie.else_stmts {
                    self.walk_pipeline_comb_stmt(s, cur_idx, stage_idx, cur_name);
                }
            }
            _ => {}
        }
    }

    fn walk_pipeline_stmt(
        &mut self,
        s: &Stmt,
        cur_idx: usize,
        stage_idx: &HashMap<&str, usize>,
        cur_name: &str,
    ) {
        match s {
            Stmt::Assign(a) => {
                self.check_pipeline_cross_stage_expr(&a.value, cur_idx, stage_idx, cur_name);
            }
            Stmt::IfElse(ie) => {
                self.check_pipeline_cross_stage_expr(&ie.cond, cur_idx, stage_idx, cur_name);
                for s in &ie.then_stmts {
                    self.walk_pipeline_stmt(s, cur_idx, stage_idx, cur_name);
                }
                for s in &ie.else_stmts {
                    self.walk_pipeline_stmt(s, cur_idx, stage_idx, cur_name);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn check_pipeline_cross_stage_expr(
        &mut self,
        expr: &Expr,
        cur_idx: usize,
        stage_idx: &HashMap<&str, usize>,
        cur_name: &str,
    ) {
        match &expr.kind {
            ExprKind::FieldAccess(base, _field) => {
                if let ExprKind::Ident(name) = &base.kind {
                    if let Some(&j) = stage_idx.get(name.as_str()) {
                        // Only flag backward references that *skip* a stage,
                        // i.e. j < cur_idx - 1. These bypass the intermediate
                        // stages' registers and emit a direct combinational
                        // path. Self (j == cur_idx) and previous-stage
                        // (j + 1 == cur_idx) reads are the canonical data-flow
                        // patterns. Forward references (j > cur_idx) are
                        // hazard reads — Decode reading Execute for forwarding
                        // / load-use stall — and are intentional.
                        if j + 1 < cur_idx {
                            let hops = cur_idx - j;
                            self.errors.push(CompileError::general(
                                &format!(
                                    "pipeline stage `{cur_name}` references stage `{name}` ({hops} stages back), bypassing the intermediate stages' registers. This emits a direct combinational path that silently breaks timing. Pass the value forward through stage registers (one register per intermediate stage). Forward references (Decode reading Execute, etc.) are allowed because they're hazard reads, but backward references must go through registered pipeline state.",
                                ),
                                expr.span,
                            ));
                        }
                    }
                }
                // Recurse to catch nested cases like `Stage.field.bit`.
                self.check_pipeline_cross_stage_expr(base, cur_idx, stage_idx, cur_name);
            }
            ExprKind::Binary(_, l, r) => {
                self.check_pipeline_cross_stage_expr(l, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(r, cur_idx, stage_idx, cur_name);
            }
            ExprKind::Unary(_, e)
            | ExprKind::Cast(e, _)
            | ExprKind::Clog2(e)
            | ExprKind::Onehot(e)
            | ExprKind::Signed(e)
            | ExprKind::Unsigned(e)
            | ExprKind::LatencyAt(e, _)
            | ExprKind::SvaNext(_, e) => {
                self.check_pipeline_cross_stage_expr(e, cur_idx, stage_idx, cur_name);
            }
            ExprKind::Index(b, i) => {
                self.check_pipeline_cross_stage_expr(b, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(i, cur_idx, stage_idx, cur_name);
            }
            ExprKind::BitSlice(b, hi, lo) => {
                self.check_pipeline_cross_stage_expr(b, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(hi, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(lo, cur_idx, stage_idx, cur_name);
            }
            ExprKind::PartSelect(b, s, w, _) => {
                self.check_pipeline_cross_stage_expr(b, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(s, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(w, cur_idx, stage_idx, cur_name);
            }
            ExprKind::Ternary(c, t, e) => {
                self.check_pipeline_cross_stage_expr(c, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(t, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(e, cur_idx, stage_idx, cur_name);
            }
            ExprKind::Concat(xs) | ExprKind::FunctionCall(_, xs) => {
                for x in xs {
                    self.check_pipeline_cross_stage_expr(x, cur_idx, stage_idx, cur_name);
                }
            }
            ExprKind::Repeat(n, x) => {
                self.check_pipeline_cross_stage_expr(n, cur_idx, stage_idx, cur_name);
                self.check_pipeline_cross_stage_expr(x, cur_idx, stage_idx, cur_name);
            }
            ExprKind::MethodCall(recv, _, args) => {
                self.check_pipeline_cross_stage_expr(recv, cur_idx, stage_idx, cur_name);
                for a in args {
                    self.check_pipeline_cross_stage_expr(a, cur_idx, stage_idx, cur_name);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn check_pipeline(&mut self, p: &PipelineDecl) {
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
            let has_reg = stage
                .body
                .iter()
                .any(|i| matches!(i, ModuleBodyItem::RegDecl(_)));
            let has_always = stage
                .body
                .iter()
                .any(|i| matches!(i, ModuleBodyItem::RegBlock(_)));

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
                // Reject bare-ident `<=` inside `for` loops in stage seq blocks.
                if let ModuleBodyItem::RegBlock(rb) = item {
                    for s in &rb.stmts {
                        Self::reject_bare_assign_in_for(s, false, &mut self.errors);
                    }
                }
            }
        }

        // Cross-stage span check: in a stage's body (data path),
        // `<Stage>.<field>` references are allowed only for self
        // (`<Stage_i>.<field>`) and the immediately preceding stage
        // (`<Stage_{i-1}>.<field>`). References that span more than one
        // hop emit a direct combinational path through the pipeline,
        // bypassing the intermediate stages' registers — silently
        // breaks timing. Hazard expressions (stall_cond / flush /
        // forward) live outside `stage.body` and are intentionally
        // exempt.
        let stage_idx: HashMap<&str, usize> = stage_names
            .iter()
            .enumerate()
            .map(|(i, n)| (*n, i))
            .collect();
        for (i, stage) in p.stages.iter().enumerate() {
            for item in &stage.body {
                self.collect_pipeline_cross_stage_refs(item, i, &stage_idx, &stage.name.name);
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
                    Self::collect_comb_stmt_targets(&cb.stmts, &mut driven);
                    // `collect_comb_stmt_targets` above only tells us a target
                    // is assigned on SOME path (e.g. one arm of an if/else) —
                    // it doesn't confirm every control path assigns it. Since
                    // `wire` isn't legal in a stage and `reg` can't be
                    // assigned in `comb`, an output port is the only
                    // comb-assignable target here, so `check_comb_latch`
                    // (the same no-implicit-latch analysis module bodies
                    // use) doubles as the pipeline stage's latch guard: a
                    // partial if/match with no covering else/wildcard arm is
                    // reported as "infers a latch" instead of being silently
                    // accepted or mis-reported as "not driven". See #557.
                    self.check_comb_latch(&cb.stmts, cb.span);
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

        // A `pipeline` exists to stage a datapath: every output must come from
        // a stage register, never from a combinational path through an input
        // port. A comb passthrough (`comb out = in`, or `out = f(in)`, possibly
        // via a `let`/`wire`) defeats the construct's purpose AND breaks a
        // soundness assumption downstream — the whole-design comb-loop detector
        // models a pipeline inst as having registered (PURE) outputs, so a real
        // comb input→output path inside one would let a feedback loop routed
        // through the pipeline go undetected (Verilator flags it `UNOPTFLAT`).
        // Reject it here; the combinational logic belongs in a wrapping
        // `module` whose result the pipeline then registers.
        self.check_pipeline_no_comb_input_to_output(p);

        // Warn when a wait-stage's idle fast-path can collapse an
        // inter-wait assignment into the same cycle as the assignment that
        // follows the next wait boundary (see #590 — fixed by emitting the
        // next group's pre-assigns on the fast-path edge, but if the wait
        // condition is already true at dispatch, both writes still land on
        // the same clock edge and the pre-wait value is never observable).
        for stage in &p.stages {
            self.check_pipeline_wait_stage_collapse(stage);
        }
    }

    /// See #590. Partition a wait-stage's `seq` statements into the same
    /// [pre-assigns, wait, pre-assigns, wait, ..., trailing] groups the
    /// codegen (`emit_pipeline_wait_stage_ff` in `src/codegen/pipeline.rs`)
    /// and sim codegen (`emit_pipeline_sim_wait_stage` in
    /// `src/sim_codegen/pipeline.rs`) use, and warn on any register that is
    /// assigned in both the group before the first wait and the group that
    /// runs immediately after it — those two groups can now execute back to
    /// back in a single cycle via the idle fast-path (state 0), in which
    /// case the second write silently wins.
    fn check_pipeline_wait_stage_collapse(&mut self, stage: &StageDecl) {
        let mut seq_stmts: &[Stmt] = &[];
        for item in &stage.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                seq_stmts = &rb.stmts;
                break;
            }
        }
        if seq_stmts.is_empty() {
            return;
        }

        struct WaitGroup<'a> {
            pre_assigns: Vec<&'a Stmt>,
            wait_span: Span,
        }

        let mut groups: Vec<WaitGroup> = Vec::new();
        let mut cur_assigns: Vec<&Stmt> = Vec::new();
        for stmt in seq_stmts {
            match stmt {
                Stmt::WaitUntil(_, span) => {
                    groups.push(WaitGroup {
                        pre_assigns: std::mem::take(&mut cur_assigns),
                        wait_span: *span,
                    });
                }
                Stmt::DoUntil { span, .. } => {
                    groups.push(WaitGroup {
                        pre_assigns: std::mem::take(&mut cur_assigns),
                        wait_span: *span,
                    });
                }
                other => cur_assigns.push(other),
            }
        }
        let trailing = std::mem::take(&mut cur_assigns);

        if groups.is_empty() {
            return;
        }

        // The two groups that the idle-state fast path can now run back to
        // back on the same clock edge: group[0].pre_assigns, followed by
        // either group[1].pre_assigns (2+ waits) or `trailing` (exactly 1
        // wait — see the `if (cond) { trailing }` fast-path arm in codegen).
        let second: &[&Stmt] = if groups.len() > 1 {
            &groups[1].pre_assigns
        } else {
            &trailing
        };
        if second.is_empty() {
            return;
        }

        let mut first_targets: HashSet<String> = HashSet::new();
        for s in &groups[0].pre_assigns {
            Self::collect_stmt_targets(std::slice::from_ref(*s), &mut first_targets);
        }
        if first_targets.is_empty() {
            return;
        }

        let mut second_targets: HashSet<String> = HashSet::new();
        for s in second {
            Self::collect_stmt_targets(std::slice::from_ref(*s), &mut second_targets);
        }

        let wait_span = groups[0].wait_span;
        let mut names: Vec<&String> = first_targets.intersection(&second_targets).collect();
        names.sort();
        for name in names {
            self.warnings.push(CompileWarning {
                message: format!(
                    "register '{name}' is assigned both before and after a `wait` in stage `{}`; when the wait condition is already true at dispatch both assignments execute in the same cycle and the last write wins — the pre-wait value is never observable on the fast path",
                    stage.name.name
                ),
                span: wait_span,
            });
        }
    }

    /// Reject any `pipeline` output port that combinationally depends on an
    /// input port (directly, or transitively through `let`/`wire`/comb
    /// intermediates, including `if`/`match` guards). Reading a register —
    /// local (`result`) or cross-stage (`Fetch.captured`) — is safe: that
    /// breaks the comb path. See the call site for the rationale.
    fn check_pipeline_no_comb_input_to_output(&mut self, p: &PipelineDecl) {
        use std::collections::HashSet;
        let input_ports: HashSet<String> = p
            .ports
            .iter()
            .filter(|pt| pt.direction == Direction::In)
            .map(|pt| pt.name.name.clone())
            .collect();
        let output_ports: HashSet<String> = p
            .ports
            .iter()
            .filter(|pt| pt.direction == Direction::Out)
            .map(|pt| pt.name.name.clone())
            .collect();
        if input_ports.is_empty() || output_ports.is_empty() {
            return;
        }

        // Registers across all stages — reading one breaks the comb path, so a
        // register name is never tainted (`Stage.reg` surfaces as the stage
        // ident via `collect_expr_idents`, so cross-stage reg reads are safe
        // too).
        let mut reg_names: HashSet<String> = HashSet::new();
        for stage in &p.stages {
            for item in &stage.body {
                match item {
                    ModuleBodyItem::RegDecl(r) => {
                        reg_names.insert(r.name.name.clone());
                    }
                    ModuleBodyItem::PipeRegDecl(pr) => {
                        reg_names.insert(pr.name.name.clone());
                    }
                    _ => {}
                }
            }
        }

        // Every combinational driver: (target-base-name, idents-read, span).
        // `idents-read` folds in the value plus any enclosing if/match guards.
        let mut drivers: Vec<(String, HashSet<String>, crate::lexer::Span)> = Vec::new();
        for stage in &p.stages {
            for item in &stage.body {
                match item {
                    ModuleBodyItem::LetBinding(lb) => {
                        let mut reads = HashSet::new();
                        crate::comb_graph::collect_expr_idents(&lb.value, &mut reads);
                        drivers.push((lb.name.name.clone(), reads, lb.name.span));
                    }
                    ModuleBodyItem::CombBlock(cb) => {
                        Self::collect_pipeline_comb_drivers(
                            &cb.stmts,
                            &HashSet::new(),
                            &mut drivers,
                        );
                    }
                    _ => {}
                }
            }
        }

        // Taint fixpoint: a signal is input-tainted if it reads an input port
        // or another tainted signal (and isn't a register).
        let mut tainted: HashSet<String> = input_ports.clone();
        let mut changed = true;
        while changed {
            changed = false;
            for (tgt, reads, _) in &drivers {
                if reg_names.contains(tgt) || tainted.contains(tgt) {
                    continue;
                }
                if reads.iter().any(|id| tainted.contains(id)) {
                    tainted.insert(tgt.clone());
                    changed = true;
                }
            }
        }

        // Flag output ports driven combinationally from a tainted expression.
        for (tgt, reads, span) in &drivers {
            if !output_ports.contains(tgt) {
                continue;
            }
            let direct_input = reads.iter().find(|id| input_ports.contains(*id)).cloned();
            let via = reads
                .iter()
                .find(|id| tainted.contains(*id) && !input_ports.contains(*id))
                .cloned();
            let src_desc = match (&direct_input, &via) {
                (Some(inp), _) => format!("input port `{inp}`"),
                (None, Some(v)) => format!("`{v}` (combinationally derived from a pipeline input)"),
                (None, None) => continue,
            };
            self.errors.push(CompileError::general(
                &format!(
                    "pipeline `{}` output port `{}` is driven combinationally from {}; a `pipeline` output must come from a stage register, not a combinational path through an input. Move the combinational logic into a wrapping `module` and register the result in the pipeline.",
                    p.name.name, tgt, src_desc
                ),
                *span,
            ));
        }
    }

    /// Recursively collect `(target-base, idents-read, span)` for every
    /// combinational assignment under `stmts`, threading enclosing `if`/`match`
    /// guard idents into each driver's read set.
    fn collect_pipeline_comb_drivers(
        stmts: &[Stmt],
        guards: &std::collections::HashSet<String>,
        out: &mut Vec<(
            String,
            std::collections::HashSet<String>,
            crate::lexer::Span,
        )>,
    ) {
        use crate::comb_graph::collect_expr_idents;
        for s in stmts {
            match s {
                Stmt::Assign(a) => {
                    if let Some(tgt) = Self::pipe_assign_base_name(&a.target) {
                        let mut reads = guards.clone();
                        collect_expr_idents(&a.value, &mut reads);
                        out.push((tgt, reads, a.span));
                    }
                }
                Stmt::IfElse(ie) => {
                    let mut g = guards.clone();
                    collect_expr_idents(&ie.cond, &mut g);
                    Self::collect_pipeline_comb_drivers(&ie.then_stmts, &g, out);
                    Self::collect_pipeline_comb_drivers(&ie.else_stmts, &g, out);
                }
                Stmt::Match(m) => {
                    let mut g = guards.clone();
                    collect_expr_idents(&m.scrutinee, &mut g);
                    for arm in &m.arms {
                        Self::collect_pipeline_comb_drivers(&arm.body, &g, out);
                    }
                }
                Stmt::For(fl) => {
                    Self::collect_pipeline_comb_drivers(&fl.body, guards, out);
                }
                _ => {}
            }
        }
    }

    /// The base port/signal name an assignment target writes — peels
    /// `Index`/`BitSlice`/`PartSelect` wrappers off a partial assignment.
    fn pipe_assign_base_name(e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            ExprKind::Index(b, _)
            | ExprKind::BitSlice(b, _, _)
            | ExprKind::PartSelect(b, _, _, _) => Self::pipe_assign_base_name(b),
            _ => None,
        }
    }

    // ── Linklist ──────────────────────────────────────────────────────────────

    pub(crate) fn check_template(&mut self, t: &crate::ast::TemplateDecl) {
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

    /// Bus typecheck: deprecation warning for the legacy `handshake`
    /// keyword inside a bus (renamed to `handshake_channel` for
    /// consistency with `credit_channel` / `tlm_method`). Same soft
    /// nudge pattern as `port reg`; silenceable via
    /// `ARCH_NO_DEPRECATIONS=1`. Other bus-level rules (wire-flattening
    /// validity, channel param shapes) are enforced at the bus *use*
    /// site (port resolution + emit), not here.
    pub(crate) fn check_bus(&mut self, b: &crate::ast::BusDecl) {
        if std::env::var("ARCH_NO_DEPRECATIONS").is_err() {
            for hs in &b.handshakes {
                if hs.legacy_handshake_kw {
                    self.warnings.push(CompileWarning {
                        message: format!(
                            "`handshake {name}: ...` is deprecated — use `handshake_channel {name}: ...` instead (identical semantics; matches the new `credit_channel` / `tlm_method` sibling sub-construct naming).",
                            name = hs.name.name
                        ),
                        span: hs.span,
                    });
                }
            }
        }
    }

    /// Package typecheck: recurses into the package's declared
    /// enums / structs / functions. Each contained item is checked the
    /// same way it would be at top level.
    pub(crate) fn check_package(&mut self, pkg: &crate::ast::PackageDecl) {
        for e in &pkg.enums {
            self.check_enum(e);
        }
        for s in &pkg.structs {
            self.check_struct(s);
        }
        for f in &pkg.functions {
            self.check_function(f);
        }
    }

    /// Extern packages have no ARCH-side values to validate.
    pub(crate) fn check_extern_package(&mut self, _ep: &crate::ast::ExternPackageDecl) {}

    pub(crate) fn check_linklist(&mut self, l: &crate::ast::LinklistDecl) {
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
        let has_data = l.params.iter().any(|p| p.name.name == "DATA");
        if !has_depth {
            self.errors.push(CompileError::general(
                &format!(
                    "linklist `{}` is missing required param `DEPTH: const`",
                    l.name.name
                ),
                l.name.span,
            ));
        }
        if !has_data {
            self.errors.push(CompileError::general(
                &format!(
                    "linklist `{}` is missing required param `DATA: type`",
                    l.name.name
                ),
                l.name.span,
            ));
        }

        // Required ports: clk and rst
        let has_clk = l
            .ports
            .iter()
            .any(|p| matches!(&p.ty, crate::ast::TypeExpr::Clock(_)));
        let has_rst = l
            .ports
            .iter()
            .any(|p| matches!(&p.ty, crate::ast::TypeExpr::Reset(_, _)));
        if !has_clk {
            self.errors.push(CompileError::general(
                &format!(
                    "linklist `{}` is missing required `clk: in Clock<...>` port",
                    l.name.name
                ),
                l.name.span,
            ));
        }
        if !has_rst {
            self.errors.push(CompileError::general(
                &format!(
                    "linklist `{}` is missing required `rst: in Reset<...>` port",
                    l.name.name
                ),
                l.name.span,
            ));
        }

        // `prev` op requires doubly or circular_doubly
        for op in &l.ops {
            self.check_snake_case(&op.name);
            for p in &op.ports {
                self.check_snake_case(&p.name);
            }

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

            // Known op names. NOTE: `length` is intentionally NOT an op — it is a
            // status port (`port length: out ...`) maintained by `track length:`.
            // It was previously listed here, which let `op length` type-check and
            // then hit the `unreachable!` in codegen (src/codegen/linklist.rs), since
            // there is no codegen arm for it. Keep this list in lockstep with the
            // dispatch arms in emit_ll_op_controller.
            let known_ops = [
                "alloc",
                "free",
                "insert_head",
                "insert_tail",
                "insert_after",
                "delete_head",
                "delete",
                "read_data",
                "write_data",
                "next",
                "prev",
            ];
            if !known_ops.contains(&op.name.name.as_str()) {
                self.errors.push(CompileError::general(
                    &format!(
                        "linklist `{}`: unknown op `{}`; known ops: {}",
                        l.name.name,
                        op.name.name,
                        known_ops.join(", ")
                    ),
                    op.name.span,
                ));
            }

            if op.latency == 0 {
                self.errors.push(CompileError::general(
                    &format!(
                        "linklist `{}`: op `{}` latency must be ≥ 1",
                        l.name.name, op.name.name
                    ),
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

        // ── Multi-head linklist (NUM_HEADS param) ──────────────────────────
        //
        // When NUM_HEADS > 1, ops that touch a specific chain (insert_*,
        // delete_*) must take a `req_head_idx` port of the right width so
        // the controller knows which head to index. When NUM_HEADS == 1,
        // the port must NOT appear (back-compat + avoids confusion).
        let num_heads = linklist_num_heads(l);
        let head_idx_w = if num_heads <= 1 {
            0
        } else {
            clog2_u32(num_heads)
        };
        let head_addressed = [
            "insert_head",
            "insert_tail",
            "insert_after",
            "delete_head",
            "delete",
        ];
        for op in &l.ops {
            let has_head_idx = op.ports.iter().any(|p| p.name.name == "req_head_idx");
            let is_head_addressed = head_addressed.contains(&op.name.name.as_str());
            if num_heads <= 1 && has_head_idx {
                self.errors.push(CompileError::general(
                    &format!(
                        "linklist `{}`: op `{}` declares `req_head_idx` but the linklist is single-head (no `param NUM_HEADS: const = N;` with N > 1). Remove the port, or set NUM_HEADS > 1 to opt into multi-head mode.",
                        l.name.name, op.name.name,
                    ),
                    op.name.span,
                ));
            }
            if num_heads > 1 && is_head_addressed && !has_head_idx {
                self.errors.push(CompileError::general(
                    &format!(
                        "linklist `{}`: op `{}` is a per-head op but the linklist has NUM_HEADS = {num_heads} and the op does not declare `req_head_idx: in UInt<{head_idx_w}>`. Add the port so the controller can route the op to the requested chain.",
                        l.name.name, op.name.name,
                    ),
                    op.name.span,
                ));
            }
            // Check req_head_idx width when it exists and the linklist is
            // multi-head. Expected width is ceil_log2(NUM_HEADS).
            if num_heads > 1 && has_head_idx {
                if let Some(p) = op.ports.iter().find(|p| p.name.name == "req_head_idx") {
                    if p.direction != crate::ast::Direction::In {
                        self.errors.push(CompileError::general(
                            &format!(
                                "linklist `{}`: `req_head_idx` must be an input port (`in UInt<{head_idx_w}>`)",
                                l.name.name,
                            ),
                            p.span,
                        ));
                    }
                    let width_ok = match &p.ty {
                        crate::ast::TypeExpr::UInt(w) => {
                            matches!(self.eval_const_expr(w, &HashMap::new()), Some(v) if v as u32 == head_idx_w)
                        }
                        _ => false,
                    };
                    if !width_ok {
                        self.errors.push(CompileError::general(
                            &format!(
                                "linklist `{}`: op `{}` declares `req_head_idx` with the wrong type. Expected `in UInt<{head_idx_w}>` for NUM_HEADS = {num_heads}.",
                                l.name.name, op.name.name,
                            ),
                            p.span,
                        ));
                    }
                }
            }
        }
    }

    pub(crate) fn check_function(&mut self, f: &FunctionDecl) {
        self.check_pascal_case(&f.name);
        for arg in &f.args {
            self.check_snake_case(&arg.name);
        }
        // v1: floats are not supported in module-local functions — function
        // params/locals are not added to the backend float-op dispatch scope,
        // so a float `x + y` inside a function would silently emit integer
        // arithmetic. Reject float signatures rather than miscompile.
        for arg in &f.args {
            if type_expr_contains_float(&arg.ty) {
                self.errors.push(CompileError::general(
                    &format!("floating-point types (FP32/BF16) are not supported in function parameters in v1 (parameter `{}` of `{}`)", arg.name.name, f.name.name),
                    arg.name.span,
                ));
            }
        }
        if type_expr_contains_float(&f.ret_ty) {
            self.errors.push(CompileError::general(
                &format!("floating-point types (FP32/BF16) are not supported as a function return type in v1 (function `{}`)", f.name.name),
                f.name.span,
            ));
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
                                f.name.name,
                                expected_ret.display(),
                                ret_ty.display()
                            ),
                            span: expr.span,
                        });
                    }
                }
                FunctionBodyItem::IfElse(_)
                | FunctionBodyItem::For(_)
                | FunctionBodyItem::Assign(_) => {
                    // Type checking for control flow in functions is deferred
                    // (expressions within are checked by the SV backend)
                }
            }
        }

        // Verify all code paths return a value (no latches).
        if !Self::fn_body_always_returns(&f.body) {
            self.errors.push(CompileError::general(
                &format!(
                    "function `{}`: not all code paths return a value — \
                     add an `else` branch or a final `return` to prevent latch inference",
                    f.name.name
                ),
                f.span,
            ));
        }
    }

    /// Check whether a function body always reaches a `return` on every code path.
    fn fn_body_always_returns(body: &[FunctionBodyItem]) -> bool {
        // Walk backwards: if the last statement guarantees a return, we're good.
        for item in body.iter().rev() {
            match item {
                FunctionBodyItem::Return(_) => return true,
                FunctionBodyItem::IfElse(ie) => {
                    // Both branches must return, AND else must exist
                    if !ie.else_body.is_empty()
                        && Self::fn_body_always_returns(&ie.then_body)
                        && Self::fn_body_always_returns(&ie.else_body)
                    {
                        return true;
                    }
                    // If the if/else doesn't fully return, keep scanning backwards
                    // (there might be a return after the if)
                    continue;
                }
                FunctionBodyItem::For(_) => {
                    // For loops may execute 0 times — can't guarantee return
                    continue;
                }
                FunctionBodyItem::Let(_) | FunctionBodyItem::Assign(_) => {
                    // Not a return — keep scanning
                    continue;
                }
            }
        }
        false
    }
}

/// Does `cond` contain a top-level AND-conjunct that is the channel's positive
/// guard? Used by the handshake-read lint to decide whether an enclosing `if`
/// condition properly guards a payload read.
///
/// Accepted patterns:
/// - `port.valid` / `port.req`                 (exact level guard)
/// - `port.req != port.ack`                    (2-phase pending guard)
/// - `guard && X`                              (AND conjunct, either side)
/// - `(guard) && X`                            (parens are transparent in AST)
///
/// Not accepted:
/// - `port.valid || X`                         (not guaranteed)
/// - `let g = port.valid; if g ...`            (v1 does not trace lets)
/// - `!port.valid` / else branch               (negation not modeled)
fn cond_contains_guard(cond: &Expr, port: &str, guard: &HandshakePayloadGuard) -> bool {
    match &cond.kind {
        ExprKind::FieldAccess(base, field) => {
            matches!(guard, HandshakePayloadGuard::Field(guard_field)
                if matches!(&base.kind, ExprKind::Ident(p) if p == port)
                    && field.name == *guard_field)
        }
        ExprKind::Binary(BinOp::Neq, lhs, rhs) => {
            matches!(guard, HandshakePayloadGuard::ReqAck2PhasePending { req_field, ack_field }
                if (expr_is_port_field(lhs, port, req_field)
                    && expr_is_port_field(rhs, port, ack_field))
                    || (expr_is_port_field(lhs, port, ack_field)
                        && expr_is_port_field(rhs, port, req_field)))
        }
        ExprKind::Binary(BinOp::And, lhs, rhs) | ExprKind::Binary(BinOp::BitAnd, lhs, rhs) => {
            cond_contains_guard(lhs, port, guard) || cond_contains_guard(rhs, port, guard)
        }
        _ => false,
    }
}

fn expr_is_port_field(expr: &Expr, port: &str, field_name: &str) -> bool {
    match &expr.kind {
        ExprKind::FieldAccess(base, field) => {
            matches!(&base.kind, ExprKind::Ident(p) if p == port) && field.name == field_name
        }
        _ => false,
    }
}

/// Returns true if the expression's top-level operation is a shift (`<<` or `>>`).
fn expr_is_shift(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Binary(BinOp::Shl | BinOp::Shr, _, _))
}

// ── Operator precedence ambiguity pass ──────────────────────────────────────
//
// Enforces explicit parens for common precedence footguns. A child is a "naked
// binary" if it's a Binary expr without explicit parens.

#[derive(Copy, Clone, PartialEq, Eq)]
enum OpClass {
    Bitwise,    // & | ^
    Comparison, // == != < > <= >=
    Logical,    // and or implies
    Shift,      // << >>
    Arith,      // + - +% -% * *% / %
}

fn classify(op: BinOp) -> OpClass {
    match op {
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => OpClass::Bitwise,
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
            OpClass::Comparison
        }
        BinOp::And | BinOp::Or | BinOp::Implies | BinOp::ImpliesNext => OpClass::Logical,
        BinOp::Shl | BinOp::Shr => OpClass::Shift,
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Mod
        | BinOp::AddWrap
        | BinOp::SubWrap
        | BinOp::MulWrap => OpClass::Arith,
    }
}

/// If `child` is a naked Binary expression, return its operator class + span.
fn naked_binary_class(child: &Expr) -> Option<(OpClass, BinOp, Span)> {
    if child.parenthesized {
        return None;
    }
    if let ExprKind::Binary(op, _, _) = &child.kind {
        Some((classify(*op), *op, child.span))
    } else {
        None
    }
}

fn check_precedence_expr(e: &Expr, errors: &mut Vec<CompileError>) {
    match &e.kind {
        ExprKind::Binary(op, lhs, rhs) => {
            let parent = classify(*op);
            for child in [lhs.as_ref(), rhs.as_ref()] {
                if let Some((cclass, cop, cspan)) = naked_binary_class(child) {
                    // Helper: name for an OpClass
                    let class_name = |c: OpClass| match c {
                        OpClass::Bitwise => "bitwise",
                        OpClass::Comparison => "comparison",
                        OpClass::Logical => "logical",
                        OpClass::Shift => "shift",
                        OpClass::Arith => "arithmetic",
                    };
                    let pair_check = |a: OpClass, b: OpClass| -> bool {
                        (parent == a && cclass == b) || (parent == b && cclass == a)
                    };

                    // Rule 1: bitwise vs comparison
                    // e.g. `a & mask == 0` → parses as `a & (mask == 0)`
                    if pair_check(OpClass::Bitwise, OpClass::Comparison) {
                        errors.push(CompileError::general(
                            &format!(
                                "ambiguous precedence: mixing {p_name} (`{op}`) with {c_name} (`{cop}`) — wrap one side in parens",
                                p_name = class_name(parent),
                                c_name = class_name(cclass),
                            ),
                            cspan,
                        ));
                    }
                    // Rule 2: bitwise vs logical (and/or)
                    else if pair_check(OpClass::Bitwise, OpClass::Logical) {
                        errors.push(CompileError::general(
                            &format!(
                                "ambiguous precedence: mixing {p_name} (`{op}`) with {c_name} (`{cop}`) — wrap one side in parens",
                                p_name = class_name(parent),
                                c_name = class_name(cclass),
                            ),
                            cspan,
                        ));
                    }
                    // Rule 4: shift vs arithmetic
                    // e.g. `1 << bit + 1` → `1 << (bit + 1)`
                    else if pair_check(OpClass::Shift, OpClass::Arith) {
                        errors.push(CompileError::general(
                            &format!(
                                "ambiguous precedence: mixing {p_name} (`{op}`) with {c_name} (`{cop}`) — wrap one side in parens",
                                p_name = class_name(parent),
                                c_name = class_name(cclass),
                            ),
                            cspan,
                        ));
                    }
                }
            }
            check_precedence_expr(lhs, errors);
            check_precedence_expr(rhs, errors);
        }
        ExprKind::Ternary(cond, then_e, else_e) => {
            // Rule 5 (part A): `en ? a : b + 1` parses as `en ? a : (b + 1)`.
            // If either branch is a naked binary, require parens — the user
            // likely meant `(en ? a : b) + 1` or intended the wider expression.
            for branch in [then_e.as_ref(), else_e.as_ref()] {
                if !branch.parenthesized {
                    if let ExprKind::Binary(bop, _, _) = &branch.kind {
                        // Only warn when the binary is arithmetic/bitwise/shift/comparison
                        // (logical is usually intended as the boolean result).
                        let bc = classify(*bop);
                        if matches!(
                            bc,
                            OpClass::Arith
                                | OpClass::Bitwise
                                | OpClass::Shift
                                | OpClass::Comparison
                        ) {
                            errors.push(CompileError::general(
                                &format!(
                                    "ambiguous precedence: ternary branch contains a `{bop}` expression — wrap branch in parens: `... ? ... : (expr)` or wrap the whole ternary"
                                ),
                                branch.span,
                            ));
                        }
                    }
                }
            }
            check_precedence_expr(cond, errors);
            check_precedence_expr(then_e, errors);
            check_precedence_expr(else_e, errors);
        }
        ExprKind::Unary(_, inner) => check_precedence_expr(inner, errors),
        ExprKind::Index(base, idx) => {
            check_precedence_expr(base, errors);
            check_precedence_expr(idx, errors);
        }
        ExprKind::BitSlice(base, hi, lo) => {
            check_precedence_expr(base, errors);
            check_precedence_expr(hi, errors);
            check_precedence_expr(lo, errors);
        }
        ExprKind::PartSelect(base, start, width, _) => {
            check_precedence_expr(base, errors);
            check_precedence_expr(start, errors);
            check_precedence_expr(width, errors);
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args {
                check_precedence_expr(a, errors);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                check_precedence_expr(p, errors);
            }
        }
        ExprKind::Repeat(n, expr) => {
            check_precedence_expr(n, errors);
            check_precedence_expr(expr, errors);
        }
        ExprKind::FieldAccess(base, _) => check_precedence_expr(base, errors),
        ExprKind::Cast(inner, _) => check_precedence_expr(inner, errors),
        ExprKind::MethodCall(base, _, args) => {
            check_precedence_expr(base, errors);
            for a in args {
                check_precedence_expr(a, errors);
            }
        }
        ExprKind::Clog2(inner) => check_precedence_expr(inner, errors),
        ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => check_precedence_expr(inner, errors),
        _ => {}
    }

    // Rule 5: ternary inside a binary expression without parens is ambiguous.
    // e.g. `en ? a : b + 1` means `en ? a : (b + 1)` — surprising.
    if let ExprKind::Binary(_, lhs, rhs) = &e.kind {
        for child in [lhs.as_ref(), rhs.as_ref()] {
            if !child.parenthesized && matches!(child.kind, ExprKind::Ternary(..)) {
                errors.push(CompileError::general(
                    "ambiguous precedence: ternary `? :` inside arithmetic/bitwise/comparison — wrap the ternary in parens: `(cond ? a : b)`",
                    child.span,
                ));
            }
        }
    }
}

/// For the latch-regfile write-port source check: walk an `Expr` and
/// return the *root* identifier name when the expression is a path of
/// idents / member accesses / const-index reads (e.g. `r`, `port.signal`,
/// `inst.out`, `arr[3]`). Returns `None` for anything else (Binary,
/// Unary, MethodCall, BitConcat, Ternary, etc.) — those are
/// combinational and the latch RF cannot accept them as inputs.
fn root_ident_for_latch_check(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::FieldAccess(inner, _) => root_ident_for_latch_check(inner),
        ExprKind::Index(inner, idx) => {
            // Index by literal is fine (it picks one element of a Vec — the
            // Vec itself is the source). Index by an arbitrary expr could
            // glitch the index path; reject by returning None.
            match &idx.kind {
                ExprKind::Literal(_) | ExprKind::Ident(_) => root_ident_for_latch_check(inner),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Run the precedence-ambiguity check on a parsed SourceFile (pre-elaboration).
/// Returns any ambiguity errors found.
pub fn check_precedence(source: &SourceFile) -> Vec<CompileError> {
    let mut errors = Vec::new();
    for item in &source.items {
        check_precedence_in_item(item, &mut errors);
    }
    errors
}

/// Walk all items and check every expression for ambiguous precedence.
fn check_precedence_in_item(item: &Item, errors: &mut Vec<CompileError>) {
    // Simple helper: invoke the walker on every expression we find in any
    // construct body. We approach this via a best-effort walker on common
    // body item kinds — for statements, comb blocks, reg blocks, let bindings,
    // asserts, etc.

    fn walk_stmt(s: &Stmt, errors: &mut Vec<CompileError>) {
        match s {
            Stmt::Assign(a) => {
                check_precedence_expr(&a.target, errors);
                check_precedence_expr(&a.value, errors);
            }
            Stmt::IfElse(ie) => {
                check_precedence_expr(&ie.cond, errors);
                for s in &ie.then_stmts {
                    walk_stmt(s, errors);
                }
                for s in &ie.else_stmts {
                    walk_stmt(s, errors);
                }
            }
            Stmt::Match(m) => {
                check_precedence_expr(&m.scrutinee, errors);
                for arm in &m.arms {
                    for s in &arm.body {
                        walk_stmt(s, errors);
                    }
                }
            }
            Stmt::Log(l) => {
                for a in &l.args {
                    check_precedence_expr(a, errors);
                }
            }
            Stmt::For(fl) => {
                match &fl.range {
                    ForRange::Range(s, e) => {
                        check_precedence_expr(s, errors);
                        check_precedence_expr(e, errors);
                    }
                    ForRange::ValueList(vs) => {
                        for v in vs {
                            check_precedence_expr(v, errors);
                        }
                    }
                }
                for s in &fl.body {
                    walk_stmt(s, errors);
                }
            }
            Stmt::Init(ib) => {
                for s in &ib.body {
                    walk_stmt(s, errors);
                }
            }
            Stmt::WaitUntil(expr, _) => {
                check_precedence_expr(expr, errors);
            }
            Stmt::DoUntil { body, cond, .. } => {
                for s in body {
                    walk_stmt(s, errors);
                }
                check_precedence_expr(cond, errors);
            }
        }
    }

    fn walk_comb(cs: &Stmt, errors: &mut Vec<CompileError>) {
        match cs {
            Stmt::Assign(a) => {
                check_precedence_expr(&a.target, errors);
                check_precedence_expr(&a.value, errors);
            }
            Stmt::IfElse(ie) => {
                check_precedence_expr(&ie.cond, errors);
                for s in &ie.then_stmts {
                    walk_comb(s, errors);
                }
                for s in &ie.else_stmts {
                    walk_comb(s, errors);
                }
            }
            Stmt::Match(m) => {
                check_precedence_expr(&m.scrutinee, errors);
                for arm in &m.arms {
                    for s in &arm.body {
                        walk_comb(s, errors);
                    }
                }
            }
            Stmt::Log(l) => {
                for a in &l.args {
                    check_precedence_expr(a, errors);
                }
            }
            Stmt::For(fl) => {
                match &fl.range {
                    ForRange::Range(s, e) => {
                        check_precedence_expr(s, errors);
                        check_precedence_expr(e, errors);
                    }
                    ForRange::ValueList(vs) => {
                        for v in vs {
                            check_precedence_expr(v, errors);
                        }
                    }
                }
                for s in &fl.body {
                    walk_comb(s, errors);
                }
            }
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("seq-only Stmt variant inside comb-context walker")
            }
        }
    }

    fn walk_body(body: &[ModuleBodyItem], errors: &mut Vec<CompileError>) {
        for it in body {
            match it {
                ModuleBodyItem::RegDecl(r) => {
                    if let Some(ref e) = r.init {
                        check_precedence_expr(e, errors);
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    check_precedence_expr(&l.value, errors);
                }
                ModuleBodyItem::CombBlock(cb) => {
                    for s in &cb.stmts {
                        walk_comb(s, errors);
                    }
                }
                ModuleBodyItem::RegBlock(rb) => {
                    for s in &rb.stmts {
                        walk_stmt(s, errors);
                    }
                }
                ModuleBodyItem::LatchBlock(lb) => {
                    for s in &lb.stmts {
                        walk_stmt(s, errors);
                    }
                }
                ModuleBodyItem::Inst(inst) => {
                    for c in &inst.connections {
                        check_precedence_expr(&c.signal, errors);
                    }
                }
                ModuleBodyItem::Assert(a) => {
                    check_precedence_expr(&a.expr, errors);
                }
                _ => {}
            }
        }
    }

    match item {
        Item::Module(m) => walk_body(&m.body, errors),
        Item::Fsm(f) => {
            for l in &f.lets {
                check_precedence_expr(&l.value, errors);
            }
            for r in &f.regs {
                if let Some(ref e) = r.init {
                    check_precedence_expr(e, errors);
                }
            }
            for sb in &f.states {
                for s in &sb.seq_stmts {
                    walk_stmt(s, errors);
                }
                for s in &sb.comb_stmts {
                    walk_comb(s, errors);
                }
                for tr in &sb.transitions {
                    check_precedence_expr(&tr.condition, errors);
                }
            }
            for s in &f.default_seq {
                walk_stmt(s, errors);
            }
            for s in &f.default_comb {
                walk_comb(s, errors);
            }
            for a in &f.asserts {
                check_precedence_expr(&a.expr, errors);
            }
        }
        Item::Function(f) => {
            for it in &f.body {
                match it {
                    FunctionBodyItem::Let(l) => check_precedence_expr(&l.value, errors),
                    FunctionBodyItem::Return(e) => check_precedence_expr(e, errors),
                    FunctionBodyItem::IfElse(_)
                    | FunctionBodyItem::For(_)
                    | FunctionBodyItem::Assign(_) => {}
                }
            }
        }
        _ => {}
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
/// True if a TypeExpr is, or contains (via Vec nesting), a float type.
/// Used to reject FP32/BF16 in positions the v1 float-op dispatch can't
/// resolve (Vec elements, struct fields, function signatures).
fn type_expr_contains_float(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::FP32 | TypeExpr::BF16 => true,
        TypeExpr::Vec(inner, _) => type_expr_contains_float(inner),
        _ => false,
    }
}

/// True if `e` is a bare integer literal (`Dec`/`Hex`/`Bin`/`Sized`) — used to
/// reject the "integer literal into a float slot" foot-gun consistently
/// across `reset` (arch#620/#623), `init` and port `default` (arch#622/#624).
fn is_bare_int_literal(e: &Expr) -> bool {
    matches!(
        &e.kind,
        ExprKind::Literal(
            LitKind::Dec(_) | LitKind::Hex(_) | LitKind::Bin(_) | LitKind::Sized(_, _)
        )
    )
}

fn types_compatible(expected: &Ty, actual: &Ty) -> bool {
    match (expected, actual) {
        (Ty::UInt(em), Ty::UInt(am)) => am <= em,
        (Ty::SInt(em), Ty::SInt(am)) => am <= em,
        // Bool ≡ UInt<1>: freely assignable in both directions.
        (Ty::Bool, Ty::UInt(1)) | (Ty::UInt(1), Ty::Bool) => true,
        (Ty::Bool, Ty::Bool) => true,
        // Floating-point: same type only — no implicit FP32↔BF16 conversion.
        (Ty::FP32, Ty::FP32) => true,
        (Ty::BF16, Ty::BF16) => true,
        _ => false,
    }
}

pub fn enum_width(num_variants: usize) -> u32 {
    if num_variants <= 1 {
        1
    } else {
        crate::width::clog2(num_variants as u64)
    }
}

/// Fold a linklist's `NUM_HEADS` param to a u32. Returns 1 when the param
/// is absent (back-compat default) or the default doesn't reduce to a
/// plain integer literal — typecheck only allows literal defaults for
/// this param (matches DEPTH).
pub fn linklist_num_heads(l: &crate::ast::LinklistDecl) -> u32 {
    use crate::ast::{ExprKind, LitKind};
    let Some(p) = l.params.iter().find(|p| p.name.name == "NUM_HEADS") else {
        return 1;
    };
    let Some(def) = &p.default else {
        return 1;
    };
    match &def.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v)) => *v as u32,
        ExprKind::Literal(LitKind::Sized(_, v)) => *v as u32,
        _ => 1,
    }
}

/// ceil_log2 for u32. Compatibility shim — delegates to [`crate::width::clog2`].
pub fn clog2_u32(n: u32) -> u32 {
    crate::width::clog2(n as u64)
}
