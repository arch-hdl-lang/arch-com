/// Verilator-compatible C++ simulation model generator.
///
/// For each synthesizable construct in the ARCH source (module, counter, fsm)
/// this emits:
///   VFunctions.h  – inline C++ for all `function` items
///   V{Name}.h     – class declaration with public port fields and private state
///   V{Name}.cpp   – eval() / eval_posedge() / eval_comb() implementations
///
/// The generated class matches the Verilator interface:
///   VFoo* dut = new VFoo;
///   dut->clk = 0; dut->eval();
///   dut->clk = 1; dut->eval();   // rising edge detected inside eval()
///   dut->final();
///   delete dut;
use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::comb_graph;
use crate::resolve::{Symbol, SymbolTable};
use crate::thread_map::expr_label;
use crate::typecheck::enum_width;

// Per-construct emitters split out to keep this file from growing further.
// Each submodule extends `impl SimCodegen` with a single `gen_*` entry
// point and calls back into the shared helpers in this file via `super::`.
mod cam;
mod fifo;
mod fsm;
mod linklist;
mod pipeline;
mod ram;
pub mod thread_sim;

// Construct emitters split out in P4 phase 1 (move-only).
mod arbiter;
mod clkgate;
mod counter;
mod functions;
mod pybind;
mod regfile;
mod structs;
mod synchronizer;

// Shared-helper modules split out in P4 phase 1 (move-only).
// Re-exported so every sibling emitter's existing `use super::*;`
// keeps resolving these by their original bare names.
mod bus_expand;
mod collect;
mod const_eval;
mod expr_codegen;
mod stmt_codegen;
mod trace;
mod width;
use bus_expand::*;
use collect::*;
use const_eval::*;
use expr_codegen::*;
use stmt_codegen::*;
use trace::*;
use width::*;
// functions.rs also holds a free helper (collect_stmt_assigns) alongside
// gen_functions; re-export it too so the remaining call site in gen_module
// still resolves it unqualified.
use functions::collect_stmt_assigns;

// ── Public API ────────────────────────────────────────────────────────────────

pub struct SimModel {
    pub class_name: String,
    pub header: String,
    pub impl_: String,
}

pub struct SimCodegen<'a> {
    symbols: &'a SymbolTable,
    source: &'a SourceFile,
    #[allow(dead_code)]
    overload_map: HashMap<usize, usize>,
    check_uninit: bool,
    inputs_start_uninit: bool,
    check_uninit_ram: bool,
    cdc_random: bool,
    debug: bool,
    debug_depth: u32,
    debug_fsm: bool,
    coverage: bool,
    /// Phase 5: also write a Verilator-compatible coverage.dat.
    /// Implies --coverage. Filename comes via main.rs (defaults to
    /// `coverage.dat` in cwd).
    coverage_dat: Option<String>,
    /// Optional source map for resolving span byte offsets to
    /// (file:line). Populated by main.rs from MultiSource when
    /// --coverage is enabled.
    source_map: Option<SourceMap>,
    /// Float formats visible inside the pipeline currently being emitted:
    /// ports and stage regs/lets/wires by name, plus cross-stage
    /// `Stage.reg` compound keys. Populated at gen_pipeline entry; drives
    /// float-op dispatch in `pipeline_sim_expr` (RefCell because the
    /// pipeline emitter methods take `&self`).
    pipeline_float_names: std::cell::RefCell<HashMap<String, FpFmt>>,
}

/// Maps byte offsets in the concatenated source (as produced by
/// `MultiSource::from_files` in `main.rs`) back to (file_path,
/// 1-based line number). Used by --coverage to render
/// `cache_mshr.arch:111` instead of opaque `branch[3]` ordinals.
#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    /// (start_offset_in_combined, file_path, source_text). Sorted by
    /// start_offset; segments may have padding bytes between them.
    segments: Vec<(usize, String, String)>,
}

impl SourceMap {
    pub fn new(segments: Vec<(usize, String, String)>) -> Self {
        let mut s = segments;
        s.sort_by_key(|(start, _, _)| *start);
        Self { segments: s }
    }

    /// Resolve a byte offset → (file_path, 1-based line). Returns None
    /// when the offset doesn't fall inside any registered segment
    /// (defensive — well-formed AST spans should always resolve).
    pub fn locate(&self, offset: usize) -> Option<(&str, u32)> {
        for i in 0..self.segments.len() {
            let (start, file, src) = &self.segments[i];
            let next_start = self.segments.get(i + 1).map(|s| s.0).unwrap_or(usize::MAX);
            if offset >= *start && offset < next_start {
                let local = offset.saturating_sub(*start);
                if local > src.len() {
                    return None;
                }
                let line = 1 + src[..local].matches('\n').count() as u32;
                return Some((file.as_str(), line));
            }
        }
        None
    }
}

/// One coverage point recorded during gen_module. Covers statement arms
/// (`if`/`elsif`/`else`, `match`) plus expression-level control such as
/// ternary arms, then block-entry/toggle/FSM points.
#[derive(Debug, Clone)]
pub(crate) struct CovPoint {
    /// "if", "elsif", "else", "expr-then", "expr-else", ...
    pub kind: &'static str,
    /// Source byte offset of the cond expr (else: of the `else` keyword).
    /// Resolved to file:line at dump-emit time via the SourceFile span map.
    pub span_start: usize,
    /// Brief textual hint for the dump (typically the cond source) — empty
    /// for `else`. Truncated to ~60 chars.
    pub label: String,
}

/// Per-module coverage state, threaded through the emit functions via
/// `Ctx::coverage`. Single counter id namespace per module/class.
#[derive(Debug, Default)]
pub(crate) struct CoverageRegistry {
    pub points: Vec<CovPoint>,
}

impl CoverageRegistry {
    pub fn alloc(&mut self, kind: &'static str, span_start: usize, label: String) -> usize {
        let idx = self.points.len();
        self.points.push(CovPoint {
            kind,
            span_start,
            label,
        });
        idx
    }
}

fn coverage_expr_label(prefix: &str, expr: &Expr) -> String {
    let mut label = format!("{prefix} {}", expr_label(expr).replace('\n', " "));
    const MAX_LABEL_CHARS: usize = 60;
    if label.chars().count() > MAX_LABEL_CHARS {
        label = label.chars().take(MAX_LABEL_CHARS).collect();
        label.push_str("...");
    }
    label
}

use crate::ast::extract_reset_info;

impl<'a> SimCodegen<'a> {
    /// For a destructuring-let RHS, best-effort infer the struct name
    /// so we can look up individual field types. Returns None if not
    /// determinable at sim-codegen time.
    fn infer_rhs_struct_name(
        &self,
        e: &Expr,
        ports: &[PortDecl],
        body: &[ModuleBodyItem],
    ) -> Option<String> {
        if let ExprKind::StructLiteral(name, _) = &e.kind {
            return Some(name.name.clone());
        }
        if let ExprKind::Ident(n) = &e.kind {
            for p in ports {
                if p.name.name == *n {
                    if let TypeExpr::Named(sn) = &p.ty {
                        return Some(sn.name.clone());
                    }
                }
            }
            for bi in body {
                match bi {
                    ModuleBodyItem::RegDecl(r) if r.name.name == *n => {
                        if let TypeExpr::Named(sn) = &r.ty {
                            return Some(sn.name.clone());
                        }
                    }
                    ModuleBodyItem::WireDecl(w) if w.name.name == *n => {
                        if let TypeExpr::Named(sn) = &w.ty {
                            return Some(sn.name.clone());
                        }
                    }
                    ModuleBodyItem::LetBinding(lb) if lb.name.name == *n => {
                        if let Some(TypeExpr::Named(sn)) = &lb.ty {
                            return Some(sn.name.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn lookup_struct_field_ty(&self, struct_name: &str, field_name: &str) -> Option<TypeExpr> {
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

    /// Look up the port declarations for a sub-instance's module/construct type.
    /// Look up ports for a sub-instance's construct type by walking the source AST.
    /// Every first-class construct (Module, Fsm, Fifo, Ram, Counter, Arbiter,
    /// Regfile, Pipeline, Linklist, Synchronizer, Clkgate) has a `ports` field
    /// via `ConstructCommon` and we return it directly from the AST rather than
    /// going through the resolve::Symbol summary (which not all construct kinds
    /// expose `ports` through).
    fn lookup_inst_arbiter(&self, module_name: &str) -> Option<&ArbiterDecl> {
        self.source.items.iter().find_map(|item| match item {
            Item::Arbiter(a) if a.name.name == module_name => Some(a),
            _ => None,
        })
    }

    fn indexed_arbiter_port(
        &self,
        inst: &crate::ast::InstDecl,
        port_name: &str,
    ) -> Option<(String, u64)> {
        let (indexed_name, signal_name) = port_name.rsplit_once('_')?;
        let digit_start = indexed_name
            .char_indices()
            .rev()
            .find(|(_, ch)| !ch.is_ascii_digit())
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(0);
        if digit_start == indexed_name.len() {
            return None;
        }

        let group_name = &indexed_name[..digit_start];
        let index: u64 = indexed_name[digit_start..].parse().ok()?;
        if group_name.is_empty() {
            return None;
        }

        let arbiter = self.lookup_inst_arbiter(&inst.module_name.name)?;
        let port_array = arbiter
            .port_arrays
            .iter()
            .find(|pa| pa.name.name == group_name)?;
        if !port_array
            .signals
            .iter()
            .any(|sig| sig.name.name == signal_name)
        {
            return None;
        }

        let mut sub_params = arbiter.params.clone();
        for pa in &inst.param_assigns {
            if let Some(p) = sub_params.iter_mut().find(|p| p.name.name == pa.name.name) {
                p.default = Some(pa.value.clone());
            }
        }
        let count = eval_const_expr_with_params(&port_array.count_expr, &sub_params);
        if index >= count {
            return None;
        }

        Some((format!("{group_name}_{signal_name}"), index))
    }

    fn lookup_inst_ports(&self, module_name: &str) -> Vec<PortDecl> {
        for item in &self.source.items {
            let ports = match item {
                Item::Module(m) if m.name.name == module_name => Some(&m.ports),
                Item::Fsm(f) if f.name.name == module_name => Some(&f.ports),
                Item::Fifo(f) if f.name.name == module_name => Some(&f.ports),
                Item::Ram(r) if r.name.name == module_name => Some(&r.ports),
                Item::Cam(c) if c.name.name == module_name => Some(&c.ports),
                Item::Counter(c) if c.name.name == module_name => Some(&c.ports),
                Item::Arbiter(a) if a.name.name == module_name => Some(&a.ports),
                Item::Regfile(r) if r.name.name == module_name => Some(&r.ports),
                Item::Pipeline(p) if p.name.name == module_name => Some(&p.ports),
                Item::Linklist(l) if l.name.name == module_name => Some(&l.ports),
                Item::Synchronizer(s) if s.name.name == module_name => Some(&s.ports),
                Item::Clkgate(c) if c.name.name == module_name => Some(&c.ports),
                _ => None,
            };
            if let Some(p) = ports {
                return p.clone();
            }
        }
        if let Some((sym, _)) = self.symbols.globals.get(module_name) {
            match sym {
                crate::resolve::Symbol::Module(info) => return info.ports.clone(),
                crate::resolve::Symbol::Fsm(info) => return info.ports.clone(),
                crate::resolve::Symbol::Fifo(info) => return info.ports.clone(),
                crate::resolve::Symbol::Pipeline(info) => return info.ports.clone(),
                _ => {}
            }
        }
        Vec::new()
    }

    /// Sibling of [`lookup_inst_ports`] for the sub-module's params. Used
    /// when resolving Vec<_, PARAM> port widths at an inst site so the
    /// generated sim doesn't silently drop the wiring with a 0-count
    /// degenerate match. Same construct coverage as the ports lookup.
    fn lookup_inst_params(&self, module_name: &str) -> Vec<ParamDecl> {
        for item in &self.source.items {
            let params = match item {
                Item::Module(m) if m.name.name == module_name => Some(&m.params),
                Item::Fsm(f) if f.name.name == module_name => Some(&f.params),
                Item::Fifo(f) if f.name.name == module_name => Some(&f.params),
                Item::Ram(r) if r.name.name == module_name => Some(&r.params),
                Item::Cam(c) if c.name.name == module_name => Some(&c.params),
                Item::Counter(c) if c.name.name == module_name => Some(&c.params),
                Item::Arbiter(a) if a.name.name == module_name => Some(&a.params),
                Item::Regfile(r) if r.name.name == module_name => Some(&r.params),
                Item::Pipeline(p) if p.name.name == module_name => Some(&p.params),
                Item::Linklist(l) if l.name.name == module_name => Some(&l.params),
                Item::Synchronizer(s) if s.name.name == module_name => Some(&s.params),
                Item::Clkgate(c) if c.name.name == module_name => Some(&c.params),
                _ => None,
            };
            if let Some(p) = params {
                return p.clone();
            }
        }
        if let Some((sym, _)) = self.symbols.globals.get(module_name) {
            match sym {
                crate::resolve::Symbol::Module(info) => return info.params.clone(),
                crate::resolve::Symbol::Pipeline(info) => return info.params.clone(),
                _ => {}
            }
        }
        Vec::new()
    }

    pub(crate) fn gen_module(
        &self,
        m: &ModuleDecl,
        emit_debug: bool,
        debug_module_set: &std::collections::HashSet<String>,
    ) -> SimModel {
        // Sim-local flatten: SV genvar `generate_for` blocks (which the
        // elaborator preserves when an inst-bearing body's connections
        // are shape-stable) have no sim equivalent. Unroll any preserved
        // `Generate(For)` here so the rest of sim codegen sees a flat
        // body — same shape it saw before issue #399 restored the
        // SV-genvar optimization. The expansion is local to gen_module;
        // the AST passed in by `generate()` is unchanged.
        let m_flat_holder;
        let m: &ModuleDecl = if module_body_has_preserved_generate(&m.body) {
            let mut clone = m.clone();
            clone.body = flatten_preserved_generates_for_sim(&m.body, &m.params);
            m_flat_holder = clone;
            &m_flat_holder
        } else {
            m
        };

        let name = &m.name.name;
        let class = format!("V{name}");
        let enum_map = build_enum_map(self.symbols);

        // --coverage: per-module branch-coverage registry. emit_reg_if_else
        // and (later phase 1b) emit_comb_if_else allocate counter ids here.
        // Threaded into Ctx via .with_coverage(Some(&cov_reg)).
        let cov_reg: std::cell::RefCell<CoverageRegistry> =
            std::cell::RefCell::new(CoverageRegistry::default());
        let cov_handle: Option<&std::cell::RefCell<CoverageRegistry>> =
            if self.coverage { Some(&cov_reg) } else { None };

        // Collect bus port names and flattened signals (with direction for debug)
        let mut bus_port_names: HashSet<String> = HashSet::new();
        let mut bus_flat: Vec<(String, TypeExpr)> = Vec::new();
        let mut bus_flat_dirs: HashMap<String, Direction> = HashMap::new();
        // Vec-of-bus port and wire counts — drive the static unroll path
        // in emit_stmt for `for` loops that index a Vec<Bus,N> by the loop
        // variable. The loop_var_subst RefCell carries the per-iteration
        // binding while emit_stmt walks the body.
        let mut vec_of_bus_port_count_map: HashMap<String, u32> = HashMap::new();
        let mut vec_of_bus_wire_count_map: HashMap<String, u32> = HashMap::new();
        let loop_var_subst_cell: std::cell::RefCell<HashMap<String, u32>> =
            std::cell::RefCell::new(HashMap::new());
        for p in &m.ports {
            if let Some(ref bi) = p.bus_info {
                // Vec<Bus,N> ports register N indexed names so bracket-dot
                // expression lookup hits a known bus prefix. N is resolved
                // against the module's params for the param-driven case.
                match bi.count.as_ref() {
                    None => {
                        bus_port_names.insert(p.name.name.clone());
                    }
                    Some(count_expr) => {
                        let n = eval_const_expr_with_params(count_expr, &m.params) as u32;
                        for i in 0..n {
                            bus_port_names.insert(format!("{}_{}", p.name.name, i));
                        }
                        if n > 0 {
                            vec_of_bus_port_count_map.insert(p.name.name.clone(), n);
                        }
                    }
                }
                let with_dir = flatten_bus_port_with_dir(&p.name.name, bi, self.symbols, &m.params);
                for (fname, fdir, fty) in with_dir {
                    bus_flat_dirs.insert(fname.clone(), fdir);
                    bus_flat.push((fname, fty));
                }
            }
        }

        let mut port_names: HashSet<String> = m
            .ports
            .iter()
            .filter(|p| p.bus_info.is_none())
            .map(|p| p.name.name.clone())
            .collect();
        // Add flattened bus signal names to port_names
        for (flat_name, _) in &bus_flat {
            port_names.insert(flat_name.clone());
        }

        // Keep `.asserted` polarity-aware in every generated C++ expression
        // context. The eval() refactor moved expression emission into
        // eval_posedge()/eval_comb(), so both contexts need the module's reset
        // port map explicitly.
        let reset_levels: HashMap<String, ResetLevel> = m
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Reset(_, level) = &p.ty {
                    Some((p.name.name.clone(), *level))
                } else {
                    None
                }
            })
            .collect();

        let mut reg_names = collect_reg_names(&m.body, &m.ports);
        reg_names.extend(collect_pipe_reg_names(&m.body));
        let port_reg_names = collect_port_reg_names(&m.ports);
        let let_names = collect_let_names(&m.body);
        let let_values = collect_let_values(&m.body, &m.params);
        let inst_names = collect_inst_names(&m.body);
        let inst_out = collect_inst_output_signals(&m.body);
        let mut wide_names = collect_wide_names(&m.ports, &m.body, &m.params);
        let mut widths = build_widths(&m.ports, &m.body, &m.params);
        let mut signed_names = build_signed_names(&m.ports, &m.body);
        let float_names = build_float_names(&m.ports, &m.body);
        // Declared types + struct defs for composite float resolution
        // (Vec<float,N> elements, struct float fields).
        let mut decl_types: HashMap<String, TypeExpr> = HashMap::new();
        for p in &m.ports {
            decl_types.insert(p.name.name.clone(), p.ty.clone());
            // Bus ports contribute compound "port.signal" keys with each
            // signal's declared type, so float bus fields (`s.data + ...`)
            // dispatch float ops instead of integer ops on the bit pattern.
            // Bus params are substituted (defaults overridden by the
            // port-site binding, same recipe as flatten_bus_port_with_dir):
            // the raw declaration types carry bus-param idents that are
            // meaningless at module scope, which silently degraded any
            // width resolved through decl_types — e.g. a Vec<UInt<W>,N>
            // signal's element width (arch#858).
            if let Some(bi) = &p.bus_info {
                if let Some((crate::resolve::Symbol::Bus(bd), _)) =
                    self.symbols.globals.get(&bi.bus_name.name)
                {
                    let mut bus_param_map = bd.default_param_map();
                    for pa in &bi.params {
                        bus_param_map.insert(pa.name.name.clone(), &pa.value);
                    }
                    for (sig, _dir, sty) in &bd.effective_signals(&bus_param_map) {
                        decl_types.insert(
                            format!("{}.{}", p.name.name, sig),
                            subst_type_expr_sim(sty, &bus_param_map),
                        );
                    }
                }
            }
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    decl_types.insert(r.name.name.clone(), r.ty.clone());
                }
                ModuleBodyItem::WireDecl(w) => {
                    decl_types.insert(w.name.name.clone(), w.ty.clone());
                }
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(t) = &l.ty {
                        decl_types.insert(l.name.name.clone(), t.clone());
                    }
                }
                _ => {}
            }
        }
        let mut struct_defs: HashMap<String, Vec<(String, TypeExpr)>> = HashMap::new();
        for item in &self.source.items {
            if let Item::Struct(sd) = item {
                struct_defs.insert(
                    sd.name.name.clone(),
                    sd.fields
                        .iter()
                        .map(|f| (f.name.name.clone(), f.ty.clone()))
                        .collect(),
                );
            }
        }

        // Add bus flattened signals to wide_names and widths.
        // Use the param-aware width evaluator (issue #427): when a bus's
        // per-signal width depends on a bus param that the call site binds
        // to an enclosing-module param Ident (e.g. `up: target MiniAxi<ID_W=ID_W>`
        // where the module declares `param ID_W: const = 3`), the
        // substituted `flat_ty` still contains the module-param Ident;
        // resolving it requires the enclosing module's params. Without this,
        // the param-aware fold fails and the legacy `eval_width` fallback
        // returns the conservative 32, corrupting concat shift offsets.
        for (flat_name, flat_ty) in &bus_flat {
            let bits = type_bits_te_with_params(flat_ty, &m.params);
            widths.insert(flat_name.clone(), bits);
            if type_is_signed_scalar(flat_ty) {
                signed_names.insert(flat_name.clone());
            }
            if bits > 64 {
                wide_names.insert(flat_name.clone());
            }
        }

        // Populate widths with per-struct-field keys: "ctrl_r.mode" → 4, etc.
        // Required for concat-width inference when struct fields appear inside
        // a concat expression (the default `unwrap_or(8)` silently corrupts
        // readback shifts otherwise).
        let struct_decls: HashMap<&str, &StructDecl> = {
            let mut map: HashMap<&str, &StructDecl> = HashMap::new();
            for item in &self.source.items {
                match item {
                    Item::Struct(s) => {
                        map.insert(s.name.name.as_str(), s);
                    }
                    Item::Package(p) => {
                        for s in &p.structs {
                            map.insert(s.name.name.as_str(), s);
                        }
                    }
                    _ => {}
                }
            }
            map
        };
        let mut struct_typed_names: Vec<(String, &str)> = Vec::new();
        // Helper: peel `Vec<T, N>` once so a Vec-of-named-struct reg/port
        // also contributes per-field widths (the body indexes into it as
        // `<reg>[i].<field>`, and infer_expr_width's FieldAccess handler
        // looks up `<reg>.<field>` for that case).
        fn named_or_vec_named(ty: &TypeExpr) -> Option<&Ident> {
            match ty {
                TypeExpr::Named(n) => Some(n),
                TypeExpr::Vec(inner, _) => match inner.as_ref() {
                    TypeExpr::Named(n) => Some(n),
                    _ => None,
                },
                _ => None,
            }
        }
        for p in &m.ports {
            if let Some(n) = named_or_vec_named(&p.ty) {
                struct_typed_names.push((p.name.name.clone(), n.name.as_str()));
            }
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    if let Some(n) = named_or_vec_named(&r.ty) {
                        struct_typed_names.push((r.name.name.clone(), n.name.as_str()));
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    if let Some(n) = named_or_vec_named(&w.ty) {
                        struct_typed_names.push((w.name.name.clone(), n.name.as_str()));
                    }
                }
                _ => {}
            }
        }
        for (instance_name, struct_name) in &struct_typed_names {
            if let Some(sd) = struct_decls.get(struct_name) {
                for f in &sd.fields {
                    widths.insert(
                        format!("{instance_name}.{}", f.name.name),
                        type_bits_te_with_params(&f.ty, &m.params),
                    );
                }
            }
        }
        let mut named_signal_names: HashSet<String> = HashSet::new();
        for p in &m.ports {
            if ty_references_named(&p.ty) {
                named_signal_names.insert(p.name.name.clone());
            }
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    if ty_references_named(&r.ty) {
                        named_signal_names.insert(r.name.name.clone());
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    if ty_references_named(&w.ty) {
                        named_signal_names.insert(w.name.name.clone());
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(ty) = &l.ty {
                        if ty_references_named(ty) {
                            named_signal_names.insert(l.name.name.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        // Vec-typed reg names (use C array subscript `[i]` instead of bit extraction)
        let mut vec_reg_names: HashSet<String> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::RegDecl(r) = i {
                    if matches!(r.ty, TypeExpr::Vec(..)) {
                        Some(r.name.name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Vec-typed wires also use C-array indexing internally
        let vec_wire_names: HashSet<String> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::WireDecl(w) = i {
                    if matches!(w.ty, TypeExpr::Vec(..)) {
                        Some(w.name.name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        vec_reg_names.extend(vec_wire_names.iter().cloned());

        // 2D Vec names — outer indexing returns another Vec, so the inner
        // subscript must stay as C array indexing instead of bit-extraction.
        // Covers:
        //   - `wire edges: Vec<Vec<Bus,N>,M>;` (the 2D bus wire case from PR #394)
        //   - `reg rf: Vec<Vec<UInt<W>,N>,M>;` (nested-Vec regs — the case
        //     this PR newly handles, paired with the recursive
        //     `vec_array_info` fix that emits `uint32_t _rf[M][N]` rather
        //     than truncating the inner dim to a scalar)
        //   - same shape for wires whose elem is a non-bus Vec (e.g.
        //     `Vec<Vec<UInt<W>, N>, M>`)
        let vec_2d_names: HashSet<String> = m
            .body
            .iter()
            .filter_map(|i| {
                let (name, ty) = match i {
                    ModuleBodyItem::WireDecl(w) => (&w.name.name, &w.ty),
                    ModuleBodyItem::RegDecl(r) => (&r.name.name, &r.ty),
                    _ => return None,
                };
                if let TypeExpr::Vec(elem, _) = ty {
                    if matches!(elem.as_ref(), TypeExpr::Vec(_, _)) {
                        return Some(name.clone());
                    }
                }
                None
            })
            .collect();

        // D2 Vec-of-bus port array members: for `port chans: Vec<Bus, N>`,
        // the C++ class has `<ty> chans_<sig>[N]` array members (Phase 2
        // mirror) — so any Ident reference to `chans_<sig>` is a C array
        // and `chans_<sig>[i]` indexing uses C subscript, not bit-shift.
        // Register these names in vec_reg_names so expr_is_vec recognises
        // them in the Index emitter.
        for p in &m.ports {
            let Some(bi) = p.bus_info.as_ref() else {
                continue;
            };
            if bi.count.is_none() {
                continue;
            }
            let bus_name = &bi.bus_name.name;
            let Some((crate::resolve::Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name)
            else {
                continue;
            };
            let mut pm = info.default_param_map();
            for pa in &bi.params {
                pm.insert(pa.name.name.clone(), &pa.value);
            }
            for (sname, _, _) in info.effective_signals(&pm) {
                vec_reg_names.insert(format!("{}_{}", p.name.name, sname));
            }
        }

        // Vec wire/reg name → element count (for expanding inst port connections).
        // Must use the param-aware evaluator so `wire/reg Vec<T, PARAM>` resolves
        // to the param's literal value. Without this, a param-sized parent Vec
        // wire connected to a sub-inst Vec input port silently emits zero
        // fan-out lines (loop `for i in 0..0`), leaving the sub-inst's inputs
        // permanently default-constructed.
        let mut vec_wire_counts: HashMap<String, u64> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::WireDecl(w) = i {
                    if let TypeExpr::Vec(_, count_expr) = &w.ty {
                        Some((
                            w.name.name.clone(),
                            eval_const_expr_with_params(count_expr, &m.params),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let TypeExpr::Vec(_, count_expr) = &r.ty {
                    vec_wire_counts.insert(
                        r.name.name.clone(),
                        eval_const_expr_with_params(count_expr, &m.params),
                    );
                }
            }
        }

        // Collect Vec port info early (needed for header, constructor, and eval_comb).
        struct VecPortInfo {
            name: String,
            elem_ty: String,
            count: u64,
            is_input: bool,
            is_port_reg: bool,
        }
        let mut vec_port_infos: Vec<VecPortInfo> = m
            .ports
            .iter()
            .filter(|p| p.bus_info.is_none())
            .filter_map(|p| {
                if let Some((elem_ty, count_str)) = vec_array_info_with_params(&p.ty, &m.params) {
                    let count: u64 = count_str.parse().unwrap_or(0);
                    Some(VecPortInfo {
                        name: p.name.name.clone(),
                        elem_ty,
                        count,
                        is_input: p.direction == Direction::In,
                        is_port_reg: p.reg_info.is_some(),
                    })
                } else {
                    None
                }
            })
            .collect();
        let bus_flat_vec_names: HashSet<String> = bus_flat
            .iter()
            .filter_map(|(flat_name, flat_ty)| {
                if let Some((elem_ty, count_str)) = vec_array_info_with_params(flat_ty, &m.params) {
                    let count: u64 = count_str.parse().unwrap_or(0);
                    vec_port_infos.push(VecPortInfo {
                        name: flat_name.clone(),
                        elem_ty,
                        count,
                        is_input: bus_flat_dirs
                            .get(flat_name)
                            .copied()
                            .unwrap_or(Direction::In)
                            == Direction::In,
                        is_port_reg: false,
                    });
                    Some(flat_name.clone())
                } else {
                    None
                }
            })
            .collect();
        let vec_port_names: HashSet<String> =
            vec_port_infos.iter().map(|v| v.name.clone()).collect();
        // Vec ports also use C array subscript `[i]` internally
        vec_reg_names.extend(vec_port_names.iter().cloned());
        // Unified Vec<T,N> size map: wires + regs + ports. Used by bounds-check codegen.
        let mut vec_sizes: HashMap<String, u64> = vec_wire_counts.clone();
        for vi in &vec_port_infos {
            vec_sizes.insert(vi.name.clone(), vi.count);
        }
        // Vec-typed reg counts (e.g. `reg rf_reg: Vec<UInt<32>, 32>`).
        // Needed by the async-reset emitter to lower `reset r => 0` for
        // Vec regs into a per-element loop instead of an invalid scalar
        // `_rf_reg = 0` (a C array isn't assignable from a scalar).
        for r in m.body.iter().filter_map(|i| {
            if let ModuleBodyItem::RegDecl(r) = i {
                Some(r)
            } else {
                None
            }
        }) {
            if let TypeExpr::Vec(_, count_expr) = &r.ty {
                let count = eval_const_expr_with_params(count_expr, &m.params);
                if count > 0 {
                    vec_sizes.insert(r.name.name.clone(), count);
                }
            }
        }
        // Vec fields inside struct-typed ports/regs/wires use paths like
        // `r.data` for indexing (`r.data[i]`) rather than top-level names.
        // Teach the generic index lowering and bounds-check paths about
        // those field paths.
        for (instance_name, struct_name) in &struct_typed_names {
            if let Some(sd) = struct_decls.get(struct_name) {
                for f in &sd.fields {
                    if let TypeExpr::Vec(_, count_expr) = &f.ty {
                        let count = eval_const_expr_with_params(count_expr, &m.params);
                        if count > 0 {
                            let path = format!("{instance_name}.{}", f.name.name);
                            vec_reg_names.insert(path.clone());
                            vec_sizes.insert(path, count);
                        }
                    }
                }
            }
        }
        // Bus-typed wires are emitted as C++ structs. If a bus field is Vec
        // typed (notably TLM response payloads), record the `<wire>.<field>`
        // path so instance wiring copies the array element-by-element.
        for item in &m.body {
            let ModuleBodyItem::WireDecl(w) = item else {
                continue;
            };
            let TypeExpr::Named(id) = &w.ty else {
                continue;
            };
            let Some((crate::resolve::Symbol::Bus(info), _)) = self.symbols.globals.get(&id.name)
            else {
                continue;
            };
            let pm = info.default_param_map();
            for (sname, _sdir, sty) in info.effective_signals(&pm) {
                if let TypeExpr::Vec(_, count_expr) = &sty {
                    let count = eval_const_expr_with_params(count_expr, &m.params);
                    if count > 0 {
                        let path = format!("{}.{}", w.name.name, sname);
                        vec_reg_names.insert(path.clone());
                        vec_sizes.insert(path, count);
                    }
                }
            }
        }

        // Collect reset-none reg names for --check-uninit warning plumbing.
        let mut uninit_regs: HashSet<String> = if self.check_uninit {
            m.body
                .iter()
                .filter_map(|i| {
                    if let ModuleBodyItem::RegDecl(r) = i {
                        if matches!(r.reset, RegReset::None) || r.guard.is_some() {
                            Some(r.name.name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .chain(m.ports.iter().filter_map(|p| {
                    if let Some(ri) = &p.reg_info {
                        if matches!(ri.reset, RegReset::None) || ri.guard.is_some() {
                            Some(p.name.name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }))
                .collect()
        } else {
            HashSet::new()
        };

        // --inputs-start-uninit: treat every primary input port as uninitialized.
        // TB must call the generated `set_<port>()` setter to mark an input initialized.
        // Reads of uninit inputs anywhere in the design emit a warning.
        // v2 scope: scalar non-clock/reset inputs PLUS bus-flattened In signals
        // (per-signal perspective flip respected; Clock/Reset sub-signals skipped).
        let mut uninit_inputs: HashSet<String> = HashSet::new();
        if self.inputs_start_uninit {
            for p in m.ports.iter() {
                // Scalar non-bus input ports.
                if p.bus_info.is_none() {
                    if matches!(p.direction, Direction::In)
                        && !matches!(&p.ty, TypeExpr::Clock(_) | TypeExpr::Reset(_, _))
                    {
                        uninit_inputs.insert(p.name.name.clone());
                    }
                    continue;
                }
                // Bus-typed port: expand flattened signals via the symbol table,
                // apply per-signal perspective flip, track the ones that are
                // inputs from THIS module's side.
                let Some(ref bi) = p.bus_info else {
                    continue;
                };
                let Some(crate::resolve::Symbol::Bus(info)) =
                    self.symbols.globals.get(&bi.bus_name.name).map(|(s, _)| s)
                else {
                    continue;
                };
                // Build param map: bus defaults, overridden by port-site params.
                let mut param_map: std::collections::HashMap<String, &Expr> = info
                    .params
                    .iter()
                    .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                    .collect();
                for pa in &bi.params {
                    param_map.insert(pa.name.name.clone(), &pa.value);
                }
                for (sname, sdir, sty) in info.effective_signals(&param_map) {
                    // Apply perspective flip (target flips every signal).
                    let actual_dir = match bi.perspective {
                        crate::ast::BusPerspective::Initiator => sdir,
                        crate::ast::BusPerspective::Target => sdir.flip(),
                    };
                    if !matches!(actual_dir, Direction::In) {
                        continue;
                    }
                    // Clock/Reset sub-signals follow the scalar-path exclusion.
                    if matches!(&sty, TypeExpr::Clock(_) | TypeExpr::Reset(_, _)) {
                        continue;
                    }
                    uninit_inputs.insert(format!("{}_{}", p.name.name, sname));
                }
            }
        }
        // Fold inputs into the shared uninit_regs set so existing warning plumbing
        // (shadow-bit decl + read-site warning) picks them up uniformly.
        uninit_regs.extend(uninit_inputs.iter().cloned());

        // Tier 1.5 (Option D): for every bus input that is a handshake payload,
        // compute the channel's valid/req guard signal name. The --inputs-
        // start-uninit read-site warning will gate on this guard so it only
        // fires when the channel is actively asserting data — silencing the
        // legitimate "TB hasn't driven valid yet" case without weakening
        // detection of the producer bug "valid asserted, payload never set."
        //
        // Variant guard map:
        //   valid_ready | valid_only | valid_stall  -> "valid"
        //   req_ack_4phase                          -> "req"
        //   req_ack_2phase                          -> active transfer window (req != ack)
        //   ready_only                              -> no guard (continuous payload)
        let mut payload_guards: HashMap<String, String> = HashMap::new();
        if self.inputs_start_uninit {
            for p in m.ports.iter() {
                let Some(ref bi) = p.bus_info else {
                    continue;
                };
                let Some(crate::resolve::Symbol::Bus(info)) =
                    self.symbols.globals.get(&bi.bus_name.name).map(|(s, _)| s)
                else {
                    continue;
                };
                for hs in &info.handshakes {
                    let guard_expr = match hs.variant.name.as_str() {
                        "valid_ready" | "valid_only" | "valid_stall" => {
                            format!("{}_{}_valid", p.name.name, hs.name.name)
                        }
                        "req_ack_4phase" => {
                            format!("{}_{}_req", p.name.name, hs.name.name)
                        }
                        "req_ack_2phase" => {
                            format!(
                                "({}_{}_req != {}_{}_ack)",
                                p.name.name, hs.name.name, p.name.name, hs.name.name
                            )
                        }
                        _ => continue, // ready_only: no producer-valid guard
                    };
                    for payload in &hs.payload_names {
                        let payload_flat =
                            format!("{}_{}_{}", p.name.name, hs.name.name, payload.name);
                        payload_guards.insert(payload_flat, guard_expr.clone());
                    }
                }
            }
        }

        // Collect guard-annotated regs: reg_name → guard_signal_name.
        // Used for Check A (producer bug: "guard asserts but reg never written").
        let guarded_regs: HashMap<String, String> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::RegDecl(r) = i {
                    r.guard
                        .as_ref()
                        .map(|g| (r.name.name.clone(), g.name.clone()))
                } else {
                    None
                }
            })
            .chain(m.ports.iter().filter_map(|p| {
                p.reg_info.as_ref().and_then(|ri| {
                    ri.guard
                        .as_ref()
                        .map(|g| (p.name.name.clone(), g.name.clone()))
                })
            }))
            .collect();
        // Guard checks are emitted in normal native sim too, so guarded regs
        // need shadow valid bits even when the broader --check-uninit warning
        // machinery is disabled.
        let mut vinit_regs = uninit_regs.clone();
        vinit_regs.extend(guarded_regs.keys().cloned());

        // Also include inst_out in "known" names for the wide set and widths
        // (they come from sub-inst ports — we'll default them to uint32_t for now)

        let insts: Vec<&InstDecl> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::Inst(inst) = i {
                    Some(inst)
                } else {
                    None
                }
            })
            .collect();

        // Bus-typed wires in this module — needed by expand_bus_connections so
        // that `child_port -> bus_wire` emits struct-field-access exprs instead
        // of flat `<wire>_<field>` idents (which would dangle; bus wires are
        // declared as a C++ struct field, not as N flat fields).
        // A bus wire is either a scalar `wire w: BusName;` or an array
        // `wire w: Vec<BusName, N>;`. expand_bus_connections needs to see
        // BOTH cases so that `child_port -> w` and `child_port -> w[i]`
        // both lower correctly.
        let bus_wire_names: HashSet<String> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::WireDecl(w) = i {
                    let bus_named = match &w.ty {
                        TypeExpr::Named(id) => Some(&id.name),
                        TypeExpr::Vec(elem, _) => {
                            if let TypeExpr::Named(id) = elem.as_ref() {
                                Some(&id.name)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(bn) = bus_named {
                        if matches!(
                            self.symbols.globals.get(bn),
                            Some((crate::resolve::Symbol::Bus(_), _))
                        ) {
                            // Record Vec-of-bus wire counts for the for-loop
                            // static-unroll path.
                            if let TypeExpr::Vec(_, size_expr) = &w.ty {
                                let n = eval_const_expr_with_params(size_expr, &m.params) as u32;
                                if n > 0 {
                                    vec_of_bus_wire_count_map.insert(w.name.name.clone(), n);
                                }
                            }
                            return Some(w.name.name.clone());
                        }
                    }
                    None
                } else {
                    None
                }
            })
            .collect();

        // Pre-expand bus connections: whole-bus connections like `axi_rd -> m_axi_mm2s`
        // are expanded to per-signal connections using the bus definition.
        let expanded_conns: Vec<Vec<Connection>> = insts
            .iter()
            .map(|inst| expand_bus_connections(inst, m, self.source, self.symbols, &bus_wire_names))
            .collect();

        // Augment `inst_out` with output signals discovered through bus
        // expansion. The raw `collect_inst_output_signals(&m.body)` only
        // sees `out <- noc_link` (whole-bus) and records nothing useful;
        // the per-signal expansion produces directional connections like
        // `noc_link_flits_send_valid` (from prod) that must be declared
        // as private members on the parent. Without this, two insts
        // sharing an undeclared bus name (the implicit-bus-wire case)
        // generate code that references undeclared identifiers.
        //
        // We also include INPUT-direction signals here, not just outputs:
        // when a bus wire is one-side-connected (the self-loop tie-off
        // pattern in mesh tops, where only the receiving inst references
        // the wire's send_valid path), the unconnected side has no
        // assignment but the read site still references the name. The
        // member then default-initializes to 0, giving the desired idle
        // tie-off behaviour.
        let mut inst_out = inst_out;
        for (inst_idx, inst) in insts.iter().enumerate() {
            let mut bus_flat_port_names: HashSet<String> = HashSet::new();
            let mut sub_params = self.lookup_inst_params(&inst.module_name.name);
            for pa in &inst.param_assigns {
                if let Some(p) = sub_params.iter_mut().find(|p| p.name.name == pa.name.name) {
                    p.default = Some(pa.value.clone());
                }
            }
            for port in self.lookup_inst_ports(&inst.module_name.name) {
                let Some(bi) = port.bus_info.as_ref() else {
                    continue;
                };
                let Some((crate::resolve::Symbol::Bus(info), _)) =
                    self.symbols.globals.get(&bi.bus_name.name)
                else {
                    continue;
                };
                let prefixes: Vec<String> = match bi.count.as_ref() {
                    None => vec![port.name.name.clone()],
                    Some(count_expr) => {
                        let n = eval_const_expr_with_params(count_expr, &sub_params);
                        (0..n)
                            .map(|i| format!("{}_{}", port.name.name, i))
                            .collect()
                    }
                };
                let mut pm = info.default_param_map();
                for pa in &bi.params {
                    pm.insert(pa.name.name.clone(), &pa.value);
                }
                for (sname, _, _) in info.effective_signals(&pm) {
                    for prefix in &prefixes {
                        bus_flat_port_names.insert(format!("{}_{}", prefix, sname));
                    }
                }
            }
            for conn in &expanded_conns[inst_idx] {
                if !bus_flat_port_names.contains(&conn.port_name.name) {
                    continue;
                }
                if let ExprKind::Ident(name) = &conn.signal.kind {
                    inst_out.insert(name.clone());
                }
            }
        }
        // Also populate `widths` for implicit-bus-wire signals so the
        // private member emission picks the right C++ type (e.g. uint64_t
        // for a 64-bit `send_data` instead of the uint32_t fallback).
        for inst in insts.iter() {
            for p in &m.ports {
                let _ = p;
            } // placate borrow-check noise
            for sub_port in self.lookup_inst_ports(&inst.module_name.name) {
                let Some(bi) = &sub_port.bus_info else {
                    continue;
                };
                let Some((crate::resolve::Symbol::Bus(info), _)) =
                    self.symbols.globals.get(&bi.bus_name.name)
                else {
                    continue;
                };
                // Find the parent-side connection name for this bus port.
                let parent_name = inst
                    .connections
                    .iter()
                    .find(|c| c.port_name.name == sub_port.name.name)
                    .and_then(|c| {
                        if let ExprKind::Ident(n) = &c.signal.kind {
                            Some(n.clone())
                        } else {
                            None
                        }
                    });
                let Some(parent_name) = parent_name else {
                    continue;
                };
                let mut pm = info.default_param_map();
                for pa in &bi.params {
                    pm.insert(pa.name.name.clone(), &pa.value);
                }
                for (sname, _sdir, ty) in info.effective_signals(&pm) {
                    let subst_ty = subst_type_expr_sim(&ty, &pm);
                    let bits = type_bits_te_with_params(&subst_ty, &m.params);
                    widths
                        .entry(format!("{parent_name}_{sname}"))
                        .or_insert(bits);
                    if type_is_signed_scalar(&subst_ty) {
                        signed_names.insert(format!("{parent_name}_{sname}"));
                    }
                }
            }
        }

        // Preserve signedness for implicit scalar fields that capture
        // sub-instance outputs. Width-only fallback storage would otherwise
        // choose uint64_t for an SInt<40> child output and expose a large
        // unsigned value to parent/native-sim code.
        for (inst_idx, inst) in insts.iter().enumerate() {
            let mut sub_params = self.lookup_inst_params(&inst.module_name.name);
            for pa in &inst.param_assigns {
                if let Some(p) = sub_params.iter_mut().find(|p| p.name.name == pa.name.name) {
                    p.default = Some(pa.value.clone());
                }
            }
            let sub_ports = self.lookup_inst_ports(&inst.module_name.name);
            for conn in &expanded_conns[inst_idx] {
                if conn.direction != ConnectDir::Output {
                    continue;
                }
                let ExprKind::Ident(sig_name) = &conn.signal.kind else {
                    continue;
                };
                let Some(port) = sub_ports
                    .iter()
                    .find(|p| p.name.name == conn.port_name.name)
                else {
                    continue;
                };
                if type_is_signed_scalar(&port.ty) {
                    signed_names.insert(sig_name.clone());
                }
                widths
                    .entry(sig_name.clone())
                    .or_insert(type_bits_te_with_params(&port.ty, &sub_params));
            }
        }

        // Build map: parent_signal_name → Vec element count for inst-output Vec ports.
        // When a sub-instance has a Vec output port and the parent connects it to a scalar
        // wire (e.g. thread lowering creates `thread_complete -> thread_complete`), we need
        // to emit flat fields and element-by-element copies instead of scalar assignments.
        let mut inst_vec_out: HashMap<String, (String, u64, u32, bool)> = HashMap::new(); // sig → (elem_ty, count, elem_bits, array_storage)
        for (inst_idx, inst) in insts.iter().enumerate() {
            let sub_ports = self.lookup_inst_ports(&inst.module_name.name);
            // Build the effective param map for this instance: start with
            // the sub-module's defaults, then apply the inst's `param NAME = …;`
            // overrides. Without this, a Vec<_, PARAM> port on the sub-module
            // resolves only against the sub-module's default (which may be a
            // small placeholder) instead of the actual instantiated width.
            let mut sub_params = self.lookup_inst_params(&inst.module_name.name);
            for pa in &inst.param_assigns {
                if let Some(p) = sub_params.iter_mut().find(|p| p.name.name == pa.name.name) {
                    p.default = Some(pa.value.clone());
                }
            }
            let conns = &expanded_conns[inst_idx];
            for conn in conns {
                if conn.direction == ConnectDir::Output {
                    if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                        // Check if the port on the sub-instance is a Vec type
                        if let Some(port) = sub_ports
                            .iter()
                            .find(|p| p.name.name == conn.port_name.name)
                        {
                            if let Some((elem_ty, count_str)) =
                                vec_array_info_with_params(&port.ty, &sub_params)
                            {
                                let count: u64 = count_str.parse().unwrap_or(0);
                                if count > 0 {
                                    let elem_bits = if let TypeExpr::Vec(elem, _) = &port.ty {
                                        type_bits_te_with_params(elem, &sub_params)
                                    } else {
                                        0
                                    };
                                    let target_is_vec_port = m.ports.iter().any(|p| {
                                        p.name.name == *sig_name
                                            && matches!(p.ty, TypeExpr::Vec(..))
                                    });
                                    let target_is_declared_vec =
                                        target_is_vec_port || vec_reg_names.contains(sig_name);
                                    let target_is_declared_scalar = !target_is_declared_vec
                                        && (port_names.contains(sig_name)
                                            || reg_names.contains(sig_name)
                                            || let_names.contains(sig_name));
                                    let array_storage =
                                        target_is_declared_vec || !target_is_declared_scalar;

                                    inst_vec_out.insert(
                                        sig_name.clone(),
                                        (elem_ty, count, elem_bits, array_storage),
                                    );
                                    // Only array-backed targets should use Vec indexing. A packed
                                    // scalar parent connected to a child Vec output is packed below.
                                    if array_storage {
                                        vec_wire_counts.insert(sig_name.clone(), count);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Add inst-output Vec names to vec_reg_names so Index uses [i] syntax,
        // and add their element widths to the width map for expression codegen.
        for (name, (elem_ty, count, _elem_bits, array_storage)) in &inst_vec_out {
            if *array_storage {
                vec_reg_names.insert(name.clone());
            }
            // Infer element width from C++ type
            let elem_bits = match elem_ty.as_str() {
                "uint8_t" => 8,
                "uint16_t" => 16,
                "uint32_t" => 32,
                "uint64_t" => 64,
                "int8_t" => 8,
                "int16_t" => 16,
                "int32_t" => 32,
                "int64_t" => 64,
                _ => 32,
            };
            widths.insert(name.clone(), elem_bits * (*count as u32));
        }

        // Analyze combinational instance dependency graph.
        // Detects feedback cycles (compile error) and computes topological
        // evaluation order + minimum settle depth for the eval() loop.
        let (inst_eval_order, settle_depth) = {
            match comb_graph::analyze_module(m, self.symbols, self.source) {
                Ok(analysis) => (analysis.sorted_inst_indices, analysis.settle_depth),
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        };
        // If analysis produced fewer indices than insts (e.g. only partial
        // coverage due to unknown construct types), use identity order for
        // any remaining instances.
        let inst_eval_order: Vec<usize> = if inst_eval_order.len() == insts.len() {
            inst_eval_order
        } else {
            (0..insts.len()).collect()
        };

        // Determine if there are any functions defined in the same source file.
        // Includes module-internal `function` items so the per-module header
        // pulls in VFunctions.h when those callees were emitted.
        let has_functions = self.source.items.iter().any(|i| match i {
            Item::Function(_) => true,
            Item::Package(p) => !p.functions.is_empty(),
            Item::Module(mm) => mm
                .body
                .iter()
                .any(|b| matches!(b, ModuleBodyItem::Function(_))),
            _ => false,
        });

        // ── Header ───────────────────────────────────────────────────────────
        // Recurse into Vec<> so `reg foo: Vec<Entry, N>` and port types like
        // `Vec<SomeStruct, N>` trigger the VStructs.h include. Previously
        // `has_structs` only matched bare `TypeExpr::Named(_)`, so a design
        // whose only struct use was inside a Vec produced headers that
        // referenced the struct without declaring it — both the reg storage
        // line (`Entry _ent[N];`) and the pybind wrapper failed to compile.
        let has_structs = m
            .body
            .iter()
            .any(|i| matches!(i, ModuleBodyItem::RegDecl(r) if ty_references_named(&r.ty)))
            || m.body
                .iter()
                .any(|i| matches!(i, ModuleBodyItem::WireDecl(w) if ty_references_named(&w.ty)))
            || m.ports.iter().any(|p| ty_references_named(&p.ty));
        let mut h = String::new();
        h.push_str(&format!(
            "#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n"
        ));
        if has_structs {
            h.push_str("#include \"VStructs.h\"\n");
        }
        if has_functions {
            h.push_str("#include \"VFunctions.h\"\n");
        }
        for inst in &insts {
            h.push_str(&format!("#include \"V{}.h\"\n", inst.module_name.name));
        }
        h.push('\n');
        // Emit param constants as #define (deduplicated via eval_param_const_value helper)
        for p in &m.params {
            match &p.kind {
                ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::Logic(_) => {
                    if let Some(val) = eval_param_const_value(p, &m.params) {
                        h.push_str(&format!(
                            "#ifndef {}\n#define {} {val}ULL\n#endif\n",
                            p.name.name, p.name.name
                        ));
                    }
                }
                ParamKind::EnumConst(enum_name) => {
                    if let Some(ref def) = p.default {
                        if let ExprKind::EnumVariant(_, variant) = &def.kind {
                            if let Some(val) =
                                resolve_enum_variant(&enum_map, enum_name, &variant.name)
                            {
                                h.push_str(&format!(
                                    "#ifndef {}\n#define {} {val}ULL\n#endif\n",
                                    p.name.name, p.name.name
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        h.push('\n');
        h.push_str(&format!("class {class} {{\npublic:\n"));

        // Build the set of D2 Vec-of-bus port arrays. For each port with
        // `bi.count.is_some()` (Vec<Bus, N>), emit one C++ array member per
        // bus signal (D2 shape: `dut.chans_v[i]`), plus per-element
        // reference aliases (`dut.chans_0_v` → reference to `chans_v[0]`)
        // so existing flat-style TBs keep working unchanged.
        //
        // Returns Vec<(port_name, sig_name, cpp_elem_ty, count)>.
        let d2_arrays: Vec<(String, String, String, u64)> = {
            let mut out: Vec<(String, String, String, u64)> = Vec::new();
            for p in &m.ports {
                let Some(bi) = p.bus_info.as_ref() else {
                    continue;
                };
                let Some(count_expr) = bi.count.as_ref() else {
                    continue;
                };
                let n = eval_const_expr_with_params(count_expr, &m.params) as u64;
                if n == 0 {
                    continue;
                }
                let bus_name = &bi.bus_name.name;
                let Some((crate::resolve::Symbol::Bus(info), _)) =
                    self.symbols.globals.get(bus_name)
                else {
                    continue;
                };
                let mut param_map: HashMap<String, &Expr> = info
                    .params
                    .iter()
                    .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                    .collect();
                for pa in &bi.params {
                    param_map.insert(pa.name.name.clone(), &pa.value);
                }
                let eff = info.effective_signals(&param_map);
                for (sname, _sdir, sty) in &eff {
                    let subst_ty = subst_type_expr_sim(sty, &param_map);
                    let cpp_ty = cpp_port_type_with_params(&subst_ty, &m.params);
                    out.push((p.name.name.clone(), sname.clone(), cpp_ty, n));
                }
            }
            out
        };
        let d2_alias_names: HashSet<String> = d2_arrays
            .iter()
            .flat_map(|(port, sname, _, n)| {
                (0..*n).map(move |i| format!("{}_{}_{}", port, i, sname))
            })
            .collect();

        // Public port fields. Vec ports preserve the source-level array as
        // `name[N]` and keep the historical flat lane names (`name_0`, ...)
        // as references into that array for backwards-compatible C++/HARC TBs.
        for p in &m.ports {
            if p.bus_info.is_some() {
                continue;
            }
            if let Some(vi) = vec_port_infos.iter().find(|v| v.name == p.name.name) {
                h.push_str(&format!("  {} {}[{}];\n", vi.elem_ty, vi.name, vi.count));
                for i in 0..vi.count {
                    h.push_str(&format!("  {}& {}_{i};\n", vi.elem_ty, vi.name));
                }
            } else {
                let ty = cpp_port_type_with_params(&p.ty, &m.params);
                h.push_str(&format!("  {ty} {};\n", p.name.name));
            }
        }
        // D2 Vec-of-bus port arrays + per-element flat-name aliases.
        for (port, sname, cpp_ty, n) in &d2_arrays {
            h.push_str(&format!("  {cpp_ty} {port}_{sname}[{n}];\n"));
            for i in 0..*n {
                h.push_str(&format!("  {cpp_ty}& {port}_{i}_{sname};\n"));
            }
        }
        for (flat_name, flat_ty) in &bus_flat {
            if bus_flat_vec_names.contains(flat_name) {
                continue;
            }
            // Skip flat names already emitted as D2 aliases.
            if d2_alias_names.contains(flat_name) {
                continue;
            }
            let ty = cpp_port_type_with_params(flat_ty, &m.params);
            h.push_str(&format!("  {ty} {flat_name};\n"));
        }
        for vi in &vec_port_infos {
            if bus_flat_vec_names.contains(&vi.name) {
                h.push_str(&format!("  {} {}[{}];\n", vi.elem_ty, vi.name, vi.count));
                for i in 0..vi.count {
                    h.push_str(&format!("  {}& {}_{i};\n", vi.elem_ty, vi.name));
                }
            }
        }
        h.push('\n');

        // Constructor — build init list. Struct-typed ports get the default
        // ctor (`name()`); scalar ports get `name(0)`.
        let mut port_inits: Vec<String> = m
            .ports
            .iter()
            .filter(|p| {
                p.bus_info.is_none()
                    && !wide_names.contains(&p.name.name)
                    && !vec_port_names.contains(&p.name.name)
            })
            .map(|p| {
                if matches!(p.ty, TypeExpr::Named(_)) {
                    format!("{}()", p.name.name)
                } else {
                    format!("{}(0)", p.name.name)
                }
            })
            .collect();
        // Add flat Vec port alias inits (name_0(name[0]), ...).
        for vi in &vec_port_infos {
            for i in 0..vi.count {
                port_inits.push(format!("{}_{i}({}[{i}])", vi.name, vi.name));
            }
        }
        // D2 Vec-of-bus per-element alias inits: chans_0_v(chans_v[0]), ...
        for (port, sname, _cpp_ty, n) in &d2_arrays {
            for i in 0..*n {
                port_inits.push(format!("{port}_{i}_{sname}({port}_{sname}[{i}])"));
            }
        }
        // Add flattened bus signal inits — skip names that are now D2 aliases.
        for (flat_name, _) in &bus_flat {
            if bus_flat_vec_names.contains(flat_name) {
                continue;
            }
            if d2_alias_names.contains(flat_name) {
                continue;
            }
            if !wide_names.contains(flat_name) {
                port_inits.push(format!("{flat_name}(0)"));
            }
        }
        // Collect Vec-array regs that need memset in constructor body
        let mut vec_reg_inits: Vec<String> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::RegDecl(r) = i {
                    if vec_array_info_with_params(&r.ty, &m.params).is_some() {
                        let n = &r.name.name;
                        Some(format!("    memset(_{n}, 0, sizeof(_{n}));"))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        // Add memset for Vec port internal arrays
        for vi in &vec_port_infos {
            let n = &vi.name;
            vec_reg_inits.push(format!("    memset({n}, 0, sizeof({n}));"));
            vec_reg_inits.push(format!("    memset(_{n}, 0, sizeof(_{n}));"));
        }
        // Add memset for D2 Vec-of-bus arrays. Per-element flat-name
        // references alias into the array, so zeroing the array also
        // zeros the aliases (no separate init needed).
        for (port, sname, _cpp_ty, _n) in &d2_arrays {
            vec_reg_inits.push(format!(
                "    memset({port}_{sname}, 0, sizeof({port}_{sname}));"
            ));
        }

        let reg_inits: Vec<String> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::RegDecl(r) = i {
                    if vec_array_info_with_params(&r.ty, &m.params).is_some() {
                        None // handled via memset in constructor body
                    } else if matches!(r.ty, TypeExpr::Named(_)) {
                        Some(format!("_{}()", r.name.name)) // struct default constructor
                    } else if wide_names.contains(&r.name.name) {
                        Some(format!("_{}()", r.name.name)) // VlWide or _arch_u128 zero-inits
                    } else {
                        let init_val = if let Some(ref init_expr) = r.init {
                            match &init_expr.kind {
                                ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                                ExprKind::Literal(LitKind::Hex(v)) => format!("0x{:X}", v),
                                ExprKind::Literal(LitKind::Bin(v)) => v.to_string(),
                                ExprKind::Literal(LitKind::Sized(_, v)) => v.to_string(),
                                // Already rounded to its context float type
                                // at compile time (arch#622/#624) — the
                                // constructor member-init list needs a
                                // foldable constant, which this already is
                                // (no runtime to_bf16() call needed).
                                ExprKind::Literal(LitKind::TypedFloat(_, bits)) => {
                                    format!("0x{bits:X}")
                                }
                                ExprKind::Bool(b) => {
                                    if *b {
                                        "1".to_string()
                                    } else {
                                        "0".to_string()
                                    }
                                }
                                _ => "0".to_string(),
                            }
                        } else {
                            "0".to_string()
                        };
                        Some(format!("_{}({})", r.name.name, init_val))
                    }
                } else {
                    None
                }
            })
            .collect();
        // port reg shadow inits (skip Vec port-regs — they use memset in ctor body)
        let port_reg_inits: Vec<String> = m
            .ports
            .iter()
            .filter_map(|p| {
                let ri = p.reg_info.as_ref()?;
                // Vec port-regs are C arrays — can't use (0) in init list
                if vec_array_info_with_params(&p.ty, &m.params).is_some() {
                    return None;
                }
                let init_val = if let Some(ref init_expr) = ri.init {
                    match &init_expr.kind {
                        ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                        ExprKind::Literal(LitKind::Hex(v)) => format!("0x{:X}", v),
                        ExprKind::Literal(LitKind::Bin(v)) => v.to_string(),
                        ExprKind::Literal(LitKind::Sized(_, v)) => v.to_string(),
                        // See the `RegDecl` init-list case above (arch#622/#624).
                        ExprKind::Literal(LitKind::TypedFloat(_, bits)) => {
                            format!("0x{bits:X}")
                        }
                        ExprKind::Bool(b) => {
                            if *b {
                                "1".to_string()
                            } else {
                                "0".to_string()
                            }
                        }
                        _ => "0".to_string(),
                    }
                } else {
                    "0".to_string()
                };
                Some(format!("_{}({})", p.name.name, init_val))
            })
            .collect();
        // pipe_reg inits
        let pipe_reg_inits: Vec<String> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::PipeRegDecl(p) = i {
                    let mut inits = Vec::new();
                    for i in 0..p.stages {
                        let name = if i == p.stages - 1 {
                            p.name.name.clone()
                        } else {
                            format!("{}_stg{}", p.name.name, i + 1)
                        };
                        inits.push(format!("_{}(0)", name));
                    }
                    Some(inits)
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        // Collect all clock ports with domain frequency info (multi-domain support)
        let clk_ports: Vec<String> = m
            .ports
            .iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .collect();
        // Map clock port name → freq_mhz (if domain has it)
        let clk_freqs: Vec<(String, Option<u64>)> = m
            .ports
            .iter()
            .filter_map(|p| {
                if let TypeExpr::Clock(domain) = &p.ty {
                    let freq = self
                        .symbols
                        .globals
                        .get(&domain.name)
                        .and_then(|(_sym, _span)| {
                            if let crate::resolve::Symbol::Domain(info) = _sym {
                                info.freq_mhz
                            } else {
                                None
                            }
                        });
                    Some((p.name.name.clone(), freq))
                } else {
                    None
                }
            })
            .collect();
        // Collect internal clock wires: clocks referenced in `seq on X rising` that are
        // not port-level clocks (i.e. derived from inst outputs, like a clock divider).
        let internal_clks: Vec<String> = {
            let clk_set: std::collections::HashSet<&str> =
                clk_ports.iter().map(|s| s.as_str()).collect();
            let mut seen = std::collections::HashSet::new();
            m.body
                .iter()
                .filter_map(|i| {
                    if let ModuleBodyItem::RegBlock(rb) = i {
                        Some(rb)
                    } else {
                        None
                    }
                })
                .filter(|rb| !clk_set.contains(rb.clock.name.as_str()))
                .filter(|rb| seen.insert(rb.clock.name.clone()))
                .map(|rb| rb.clock.name.clone())
                .collect()
        };
        // all_clks = port clocks + internal derived clocks
        let all_clks: Vec<String> = clk_ports
            .iter()
            .chain(internal_clks.iter())
            .cloned()
            .collect();
        let has_clk = !all_clks.is_empty();
        let clk_prev_inits: Vec<String> = all_clks
            .iter()
            .map(|c| format!("_clk_prev_{}(0)", c))
            .collect();
        let all_freqs_known_early =
            clk_freqs.len() >= 2 && clk_freqs.iter().all(|(_, f)| f.is_some());
        let time_init = if all_freqs_known_early {
            vec!["time_ps(0)".to_string()]
        } else {
            vec![]
        };
        let all_inits: Vec<String> = port_inits
            .into_iter()
            .chain(reg_inits)
            .chain(port_reg_inits)
            .chain(pipe_reg_inits)
            .chain(clk_prev_inits)
            .chain(time_init)
            .collect();

        // Collect log file paths early so constructor can open them
        let log_files_for_ctor = collect_log_files(&m.body);
        // Credit-channel sites are used by the constructor (zero-init), the
        // field-decl section, the eval_posedge update, and eval_comb — so
        // collect once up front.
        let cc_sites = crate::sim_credit_channel::collect_credit_channels(m, self.symbols);
        // Constructor always has a body (for auto-trace open). Omit the
        // member-init `:` entirely when there are no scalar inits — e.g. a
        // pure-comb module whose only members are wide (VlWide) ports, which
        // self-init via VlWide's default ctor. Emitting a bare `() : {` is a
        // C++ syntax error (dangling colon with no initializers).
        let ctor_init = if all_inits.is_empty() {
            String::new()
        } else {
            format!(" : {}", all_inits.join(", "))
        };
        h.push_str(&format!("  {class}(){} {{\n", ctor_init));
        for line in &vec_reg_inits {
            h.push_str(&format!("{line}\n"));
        }
        // Zero-init credit_channel synthesized fields (DEPTH for the counter).
        crate::sim_credit_channel::emit_constructor_inits(&cc_sites, &mut h);
        for path in &log_files_for_ctor {
            h.push_str(&format!(
                "    {} = fopen(\"{}\", \"w\");\n",
                log_fd_name(path),
                path
            ));
        }
        // Note: VCD auto-open is deferred to first eval() call via Verilated::claimTrace()
        h.push_str("  }\n");
        // Verilator-compatible constructor: accepts VerilatedContext* but ignores it
        h.push_str(&format!(
            "  explicit {class}(VerilatedContext*) : {class}() {{}}\n"
        ));
        // Collect trace signals for VCD waveform support
        let trace_signals = collect_trace_signals(
            &m.ports,
            &m.body,
            &wide_names,
            &widths,
            &bus_flat,
            &m.params,
        );
        let (trace_h_decls, trace_cpp_impl) = emit_trace_methods(&class, name, &trace_signals);

        h.push_str("  void eval();\n");
        h.push_str("  void eval_comb();\n");
        h.push_str("  void eval_posedge();\n");
        if emit_debug {
            h.push_str("  void _debug_log_ports();  // --debug: print I/O port changes\n");
        }
        h.push_str(&trace_h_decls);
        // Generate tick() for multi-clock modules with known frequencies
        let all_freqs_known = clk_freqs.len() >= 2 && clk_freqs.iter().all(|(_, f)| f.is_some());
        if all_freqs_known {
            h.push_str(
                "  void tick();  // advance one time step, auto-toggle clocks at correct ratio\n",
            );
            h.push_str("  uint64_t time_ps;  // current simulation time in picoseconds\n");
        }
        // final(): close trace + log file handles
        h.push_str("  void final() {\n");
        h.push_str("    trace_close();\n");
        for path in &log_files_for_ctor {
            h.push_str(&format!(
                "    if ({fd}) fclose({fd});\n",
                fd = log_fd_name(path)
            ));
        }
        h.push_str("  }\n\n");
        // All members public for pybind11/testbench signal inspection
        h.push_str("public:\n");
        for c in &all_clks {
            h.push_str(&format!("  uint8_t _clk_prev_{c};\n"));
        }
        for c in &all_clks {
            h.push_str(&format!("  bool _rising_{c};\n"));
        }
        // --coverage Phase 6: per-(inst, output-port) prev-value
        // shadow for construct port toggle counters. Allocated only
        // when coverage is on AND port is scalar (≤64 bits). Skips
        // bus / Vec / wide ports (v1).
        if self.coverage {
            for (inst_idx, inst) in insts.iter().enumerate() {
                let conns = &expanded_conns[inst_idx];
                for conn in conns {
                    if conn.direction != ConnectDir::Output {
                        continue;
                    }
                    let sig_name = if let crate::ast::ExprKind::Ident(n) = &conn.signal.kind {
                        n.as_str()
                    } else {
                        continue;
                    };
                    let w = widths.get(sig_name).copied().unwrap_or(0);
                    if w == 0 || w > 64 {
                        continue;
                    }
                    if wide_names.contains(sig_name) {
                        continue;
                    }
                    if named_signal_names.contains(sig_name) {
                        continue;
                    }
                    if vec_port_names.contains(sig_name) {
                        continue;
                    }
                    // Skip Vec regs/wires (they connect to flattened
                    // sub-instance port names like `name_0..name_{n-1}`,
                    // not the bare `name`). Phase 6 v1 = scalars only.
                    if vec_wire_counts.contains_key(sig_name) {
                        continue;
                    }
                    h.push_str(&format!(
                        "  uint64_t _prev_{}_{} = 0;\n",
                        inst.name.name, conn.port_name.name
                    ));
                }
            }
        }

        // Private reg fields. Use params-aware Vec sizing — bare
        // `vec_array_info` returns 0 for params-as-length, which would
        // emit `_arr[0]` and corrupt stack on memcpy / index.
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let Some((elem_ty, count)) = vec_array_info_with_params(&r.ty, &m.params) {
                    h.push_str(&format!("  {elem_ty} _{}[{count}];\n", r.name.name));
                } else {
                    let ty = cpp_internal_type_with_params(&r.ty, &m.params);
                    h.push_str(&format!("  {ty} _{};\n", r.name.name));
                }
            }
        }

        // Private shadow fields for port reg outputs (and internal arrays for Vec ports)
        for p in &m.ports {
            if p.reg_info.is_some() {
                if let Some(vi) = vec_port_infos.iter().find(|v| v.name == p.name.name) {
                    // Vec port-reg: internal C array
                    h.push_str(&format!("  {} _{}[{}];\n", vi.elem_ty, vi.name, vi.count));
                } else {
                    let ty = cpp_internal_type_with_params(&p.ty, &m.params);
                    h.push_str(&format!("  {ty} _{};\n", p.name.name));
                }
            } else if vec_port_names.contains(&p.name.name) {
                // Vec non-reg port: also needs internal array for indexed access
                let vi = vec_port_infos
                    .iter()
                    .find(|v| v.name == p.name.name)
                    .unwrap();
                h.push_str(&format!("  {} _{}[{}];\n", vi.elem_ty, vi.name, vi.count));
            }
        }
        for vi in &vec_port_infos {
            if bus_flat_vec_names.contains(&vi.name) {
                h.push_str(&format!("  {} _{}[{}];\n", vi.elem_ty, vi.name, vi.count));
            }
        }

        // Shadow valid bits for --check-uninit and guarded-reg checks.
        if !vinit_regs.is_empty() {
            h.push_str("  // --check-uninit / guard-check shadow valid bits\n");
            for name in &vinit_regs {
                h.push_str(&format!("  bool _{name}_vinit = false;\n"));
            }
            // pipe_reg stages whose source is uninit also get shadow bits
            for item in &m.body {
                if let ModuleBodyItem::PipeRegDecl(p) = item {
                    // pipe_reg always gets shadow bits (propagated from source)
                    for i in 0..p.stages {
                        let sname = if i == p.stages - 1 {
                            p.name.name.clone()
                        } else {
                            format!("{}_stg{}", p.name.name, i + 1)
                        };
                        h.push_str(&format!("  bool _{sname}_vinit = false;\n"));
                    }
                }
            }
        }

        // --inputs-start-uninit: inline setters mark an input as initialized when TB drives it.
        if !uninit_inputs.is_empty() {
            h.push_str(
                "  // --inputs-start-uninit setters (mark TB-driven inputs as initialized)\n",
            );
            for p in &m.ports {
                // Scalar non-bus input.
                if p.bus_info.is_none() {
                    if !uninit_inputs.contains(&p.name.name) {
                        continue;
                    }
                    let pname = &p.name.name;
                    let ty = cpp_port_type_with_params(&p.ty, &m.params);
                    h.push_str(&format!(
                        "  void set_{pname}({ty} v) {{ {pname} = v; _{pname}_vinit = true; }}\n"
                    ));
                    continue;
                }
                // Bus port: emit one setter per flattened In signal.
                let Some(ref bi) = p.bus_info else {
                    continue;
                };
                let Some(crate::resolve::Symbol::Bus(info)) =
                    self.symbols.globals.get(&bi.bus_name.name).map(|(s, _)| s)
                else {
                    continue;
                };
                let mut param_map: std::collections::HashMap<String, &Expr> = info
                    .params
                    .iter()
                    .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                    .collect();
                for pa in &bi.params {
                    param_map.insert(pa.name.name.clone(), &pa.value);
                }
                for (sname, sdir, sty) in info.effective_signals(&param_map) {
                    let actual_dir = match bi.perspective {
                        crate::ast::BusPerspective::Initiator => sdir,
                        crate::ast::BusPerspective::Target => sdir.flip(),
                    };
                    if !matches!(actual_dir, Direction::In) {
                        continue;
                    }
                    if matches!(&sty, TypeExpr::Clock(_) | TypeExpr::Reset(_, _)) {
                        continue;
                    }
                    let flat = format!("{}_{}", p.name.name, sname);
                    if !uninit_inputs.contains(&flat) {
                        continue;
                    }
                    let subst_ty = subst_type_expr_sim(&sty, &param_map);
                    let ty = cpp_port_type_with_params(&subst_ty, &m.params);
                    h.push_str(&format!(
                        "  void set_{flat}({ty} v) {{ {flat} = v; _{flat}_vinit = true; }}\n"
                    ));
                }
            }
        }

        // Private let/wire fields (computed in eval_comb, read in eval_posedge)
        for item in &m.body {
            match item {
                ModuleBodyItem::LetBinding(l) => {
                    // Destructuring: emit a field per bound name with the
                    // corresponding struct field's width.
                    if !l.destructure_fields.is_empty() {
                        let sname = self.infer_rhs_struct_name(&l.value, &m.ports, &m.body);
                        for bind in &l.destructure_fields {
                            let ty = sname
                                .as_ref()
                                .and_then(|n| self.lookup_struct_field_ty(n, &bind.name))
                                .map(|t| cpp_internal_type_with_params(&t, &m.params))
                                .unwrap_or_else(|| "uint32_t".to_string());
                            h.push_str(&format!("  {ty} _let_{};\n", bind.name));
                        }
                        continue;
                    }
                    // ty=None: assignment to existing port/wire — no new field needed
                    if l.ty.is_none() {
                        continue;
                    }
                    let ty =
                        l.ty.as_ref()
                            .map(|t| cpp_internal_type_with_params(t, &m.params))
                            .unwrap_or_else(|| "uint32_t".to_string());
                    h.push_str(&format!("  {ty} _let_{};\n", l.name.name));
                }
                ModuleBodyItem::WireDecl(w) => {
                    // 2D bus wire: `wire edges: Vec<Vec<B, N>, M>;` →
                    //   B _let_edges[M][N];
                    // Emitted *before* the generic vec_array_info path, which
                    // would otherwise treat the outer Vec's element as
                    // `uint32_t` and silently flatten the 2D-bus shape into
                    // a 1D scalar array.
                    if let TypeExpr::Vec(outer_elem, outer_count) = &w.ty {
                        if let TypeExpr::Vec(inner_elem, inner_count) = outer_elem.as_ref() {
                            if let TypeExpr::Named(bus_id) = inner_elem.as_ref() {
                                let m_count = eval_const_expr_with_params(outer_count, &m.params);
                                let n_count = eval_const_expr_with_params(inner_count, &m.params);
                                h.push_str(&format!(
                                    "  {} _let_{}[{}][{}];\n",
                                    bus_id.name, w.name.name, m_count, n_count
                                ));
                                continue;
                            }
                        }
                    }
                    if let Some((elem_ty, count)) = vec_array_info_with_params(&w.ty, &m.params) {
                        h.push_str(&format!("  {elem_ty} _let_{}[{count}];\n", w.name.name));
                    } else {
                        let ty = cpp_internal_type_with_params(&w.ty, &m.params);
                        h.push_str(&format!("  {ty} _let_{};\n", w.name.name));
                    }
                }
                _ => {}
            }
        }

        // Private pipe_reg fields
        for item in &m.body {
            if let ModuleBodyItem::PipeRegDecl(p) = item {
                let w = widths.get(&p.source.name).copied().unwrap_or(32);
                let ty = if signed_names.contains(p.source.name.as_str()) {
                    cpp_sint(w)
                } else {
                    cpp_uint(w)
                };
                for i in 0..p.stages {
                    let name = if i == p.stages - 1 {
                        p.name.name.clone()
                    } else {
                        format!("{}_stg{}", p.name.name, i + 1)
                    };
                    h.push_str(&format!("  {ty} _{name};\n"));
                }
            }
        }

        // Private fields for sub-instance output wires
        // Sort for deterministic output — HashSet iteration order is not stable.
        let mut sorted_inst_out: Vec<&String> = inst_out.iter().collect();
        sorted_inst_out.sort();
        for sig_name in sorted_inst_out {
            if !port_names.contains(sig_name) && !reg_names.contains(sig_name)
                // Bus wires are handled via the struct-typed `_let_<name>`
                // field emitted above; a fallback `uint32_t <name>;` here
                // would shadow the bus wire with a scalar.
                && !bus_wire_names.contains(sig_name)
            {
                // Vec output ports need a C array, not a scalar
                if let Some((elem_ty, count, _elem_bits, _array_storage)) =
                    inst_vec_out.get(sig_name)
                {
                    h.push_str(&format!("  {elem_ty} {sig_name}[{count}];\n"));
                } else {
                    // Pick the C++ type from the resolved width when known
                    // (implicit bus wires + flat bus signals propagate through
                    // `widths`). Default to uint32_t when the width isn't
                    // tracked — preserves prior behaviour for plain scalars.
                    let ty = widths
                        .get(sig_name)
                        .copied()
                        .map(|w| {
                            if signed_names.contains(sig_name.as_str()) {
                                cpp_sint(w)
                            } else {
                                cpp_uint(w)
                            }
                        })
                        .unwrap_or("uint32_t");
                    h.push_str(&format!("  {ty} {sig_name};\n"));
                }
            }
        }

        // Credit-channel synthesized fields (sim mirror of SV codegen's
        // emit_credit_channel_state / _receiver_state).
        crate::sim_credit_channel::emit_header_fields(&cc_sites, &mut h);

        // Private fields for comb-block intermediate signals (not ports/regs/inst_out)
        // Sort for deterministic output — HashSet iteration order is not stable.
        let comb_targets = collect_comb_targets(&m.body);
        let mut sorted_comb_targets: Vec<&String> = comb_targets.iter().collect();
        sorted_comb_targets.sort();
        for sig_name in sorted_comb_targets {
            if !port_names.contains(sig_name)
                && !reg_names.contains(sig_name)
                && !inst_out.contains(sig_name)
                && !let_names.contains(sig_name)
            {
                h.push_str(&format!("  uint32_t {sig_name};\n"));
            }
        }

        // Sub-instance private fields
        for inst in &insts {
            h.push_str(&format!(
                "  V{} _inst_{};\n",
                inst.module_name.name, inst.name.name
            ));
        }

        // Log file handles
        for path in &log_files_for_ctor {
            h.push_str(&format!("  FILE* {} = nullptr;\n", log_fd_name(path)));
        }

        // VCD trace state
        h.push_str("  FILE* _trace_fp = nullptr;\n");
        h.push_str("  uint64_t _trace_time = 0;\n");

        // --debug port shadow copies (previous values for change detection)
        if emit_debug {
            h.push_str("  // --debug port shadow copies\n");
            for p in &m.ports {
                if p.bus_info.is_some() {
                    continue;
                } // bus flat signals handled below
                if matches!(&p.ty, TypeExpr::Clock(_)) {
                    continue;
                }
                let pname = &p.name.name;
                if let Some(vi) = vec_port_infos.iter().find(|v| v.name == *pname) {
                    // Vec port: one shadow per flat element
                    for i in 0..vi.count {
                        h.push_str(&format!("  {} _dbg_prev_{pname}_{i} = 0;\n", vi.elem_ty));
                    }
                } else {
                    let bits = type_width_of(&p.ty);
                    if bits > 64 {
                        let words = wide_words(bits);
                        h.push_str(&format!("  VlWide<{words}> _dbg_prev_{pname};\n"));
                    } else {
                        let shadow_ty = cpp_uint(bits.max(8));
                        h.push_str(&format!("  {shadow_ty} _dbg_prev_{pname} = 0;\n"));
                    }
                }
            }
            // Bus flat signal shadows
            for (flat_name, flat_ty) in &bus_flat {
                if matches!(flat_ty, TypeExpr::Vec(..)) {
                    continue;
                }
                let bits = type_width_of(flat_ty);
                if bits > 64 {
                    let words = wide_words(bits);
                    h.push_str(&format!("  VlWide<{words}> _dbg_prev_{flat_name};\n"));
                } else {
                    let shadow_ty = cpp_uint(bits.max(8));
                    h.push_str(&format!("  {shadow_ty} _dbg_prev_{flat_name} = 0;\n"));
                }
            }
            h.push_str("  uint64_t _dbg_cycle = 0;\n");
            if clk_ports.len() > 1 {
                h.push_str("  const char* _dbg_last_clk = \"?\";\n");
            }
        }

        // --coverage: emit a placeholder for the per-class counter array
        // declaration. The actual size isn't known until seq emission has
        // populated cov_reg, so we patch this placeholder just before
        // returning the SimModel.
        if self.coverage {
            h.push_str("__ARCH_COV_HEADER_DECL__");
        }

        h.push_str("};\n");

        // ── Implementation ────────────────────────────────────────────────────
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        if self.coverage {
            cpp.push_str("__ARCH_COV_IMPL_DEFN__");
        }

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        // Auto-open VCD on first eval() — only the top-level module (called by testbench) claims it
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        // Edge detection is done inside eval_posedge(), not in eval().
        // This ensures derived clocks from sub-instances (e.g. clock dividers) are
        // settled before edges are detected, and sub-instances correctly detect their
        // own clock edges when called from a parent's eval_posedge().

        let child_vec_output_shape =
            |inst: &crate::ast::InstDecl, port_name: &str| -> Option<(u64, u32)> {
                let mut sub_params = self.lookup_inst_params(&inst.module_name.name);
                for pa in &inst.param_assigns {
                    if let Some(p) = sub_params.iter_mut().find(|p| p.name.name == pa.name.name) {
                        p.default = Some(pa.value.clone());
                    }
                }
                let sub_ports = self.lookup_inst_ports(&inst.module_name.name);
                let port = sub_ports.iter().find(|p| p.name.name == port_name)?;
                let TypeExpr::Vec(elem, _) = &port.ty else {
                    return None;
                };
                let (_elem_ty, count_str) = vec_array_info_with_params(&port.ty, &sub_params)?;
                let count = count_str.parse().ok()?;
                let elem_bits = type_bits_te_with_params(elem, &sub_params);
                Some((count, elem_bits))
            };

        if insts.is_empty() {
            // No sub-instances: simple path
            cpp.push_str("  eval_comb();\n");
            if has_clk {
                cpp.push_str("  eval_posedge();\n");
                cpp.push_str("  eval_comb();\n");
            } else {
                // Pure-comb modules: emit a second eval_comb() pass so that
                // comb assignments which forward-reference signals driven
                // later in source order (e.g. `let port_o = result_w;`
                // before the comb block that drives `result_w`) settle.
                // Mirrors the two-pass shape clocked modules already use.
                // For deeper chains a topological-sort emission would be
                // required; this catches the common one-level case.
                cpp.push_str("  eval_comb();\n");
            }
        } else {
            // Modules with sub-instances: preserve simultaneity of posedge across hierarchy.
            // All always_ff blocks in the design fire simultaneously — parent and sub-instance
            // registers update at the same posedge.  This means the parent's eval_posedge()
            // must read the sub-instance's PRE-posedge combinational outputs (which reflect the
            // sub-instance's current registered values, not the new ones).
            //
            // eval_comb() is self-settling: it mirrors inst inputs, evaluates the
            // inst chain in topological order, and iterates the whole body
            // settle_depth times internally (see the comb_settle_depth loop in
            // its emission below). Running another settle loop here would square
            // the number of comb evaluations per eval() — and compound through
            // instantiation depth — so eval() just delegates, mirroring the
            // no-inst path.
            //
            // Edge detection must happen AFTER comb settle: sub-instances may
            // produce derived clocks (clock dividers/gates) whose values are only
            // valid after eval_comb(). eval_posedge() detects edges internally,
            // propagates the posedge to sub-instances (simultaneous register
            // update), and the final eval_comb() then refreshes comb state from
            // the post-posedge registers.
            cpp.push_str("  eval_comb();\n");
            if has_clk {
                cpp.push_str("  eval_posedge();\n");
                cpp.push_str("  eval_comb();\n");
            }
        }

        // --debug: log I/O port changes after settle is complete
        if emit_debug {
            cpp.push_str("  _debug_log_ports();\n");
            // Also call for sub-instances that are instrumented (depth > 1)
            for inst in &insts {
                if debug_module_set.contains(&inst.module_name.name) {
                    cpp.push_str(&format!("  _inst_{}._debug_log_ports();\n", inst.name.name));
                }
            }
        }

        // --coverage Phase 6: construct port toggle. For each scalar
        // OUTPUT port of each instantiated sub-construct, popcount-XOR
        // the current value against a per-port _prev shadow and
        // accumulate into a coverage counter. Surfaces dead lanes /
        // tied-off interfaces at black-box construct boundaries
        // (fifo, arbiter, ram, cam — anywhere the sub's internals
        // contribute zero coverage from the consumer's viewpoint).
        // Skip in v1: bus ports, wide (>64b) ports.
        if let Some(reg) = cov_handle {
            for (inst_idx, inst) in insts.iter().enumerate() {
                let conns = &expanded_conns[inst_idx];
                for conn in conns {
                    if conn.direction != ConnectDir::Output {
                        continue;
                    }
                    // Resolve parent-side storage name + width.
                    let sig_name = if let crate::ast::ExprKind::Ident(n) = &conn.signal.kind {
                        n.as_str()
                    } else {
                        continue;
                    };
                    let w = widths.get(sig_name).copied().unwrap_or(0);
                    if w == 0 || w > 64 {
                        continue;
                    }
                    if wide_names.contains(sig_name) {
                        continue;
                    }
                    if named_signal_names.contains(sig_name) {
                        continue;
                    }
                    if vec_port_names.contains(sig_name) {
                        continue;
                    }
                    // Skip Vec regs/wires (they connect to flattened
                    // sub-instance port names like `name_0..name_{n-1}`,
                    // not the bare `name`). Phase 6 v1 = scalars only.
                    if vec_wire_counts.contains_key(sig_name) {
                        continue;
                    }
                    let cidx = reg.borrow_mut().alloc(
                        "toggle",
                        inst.span.start,
                        format!("toggle {}.{}", inst.name.name, conn.port_name.name),
                    );
                    let inst_n = &inst.name.name;
                    let port_n = &conn.port_name.name;
                    let port_read = if let Some((packed_port, index)) =
                        self.indexed_arbiter_port(inst, &conn.port_name.name)
                    {
                        format!("((_inst_{inst_n}.{packed_port} >> {index}) & 1ULL)")
                    } else {
                        format!("_inst_{inst_n}.{port_n}")
                    };
                    cpp.push_str(&format!(
                        "  {{ uint64_t _cur = (uint64_t){port_read}; \
                         _arch_cov[{cidx}] += __builtin_popcountll(_cur ^ _prev_{inst_n}_{port_n}); \
                         _prev_{inst_n}_{port_n} = _cur; }}\n"
                    ));
                }
            }
        }

        // Auto-dump VCD trace after each eval()
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        // eval_posedge()
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));

        // Edge detection: detect rising edges and update _clk_prev for all clocks.
        // This runs inside eval_posedge() so that:
        //   - Derived clocks from sub-instances are already settled before detection
        //   - Sub-instances correctly detect their own clock edges when called from parent
        for c in &all_clks {
            cpp.push_str(&format!("  _rising_{c} = ({c} && !_clk_prev_{c});\n"));
            cpp.push_str(&format!("  _clk_prev_{c} = {c};\n"));
        }

        let reg_blocks: Vec<&RegBlock> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::RegBlock(rb) = i {
                    Some(rb)
                } else {
                    None
                }
            })
            .collect();
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

        // Collect pipe_reg declarations for _n_ temporary handling
        let pipe_regs: Vec<&PipeRegDecl> = m
            .body
            .iter()
            .filter_map(|i| {
                if let ModuleBodyItem::PipeRegDecl(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .collect();

        if !reg_blocks.is_empty() || !pipe_regs.is_empty() {
            // Declare _n_ temporaries for all regs. Use the param-aware
            // helper so Vec<_, PARAM_NAME> resolves to the literal default
            // (otherwise we emit `_n_arr[0]` and corrupt stack on memcpy).
            for rd in &reg_decls {
                let n = &rd.name.name;
                if let Some((elem_ty, count)) = vec_array_info_with_params(&rd.ty, &m.params) {
                    cpp.push_str(&format!(
                        "  {elem_ty} _n_{n}[{count}]; memcpy(_n_{n}, _{n}, sizeof(_{n}));\n"
                    ));
                } else {
                    let ty = cpp_internal_type_with_params(&rd.ty, &m.params);
                    cpp.push_str(&format!("  {ty} _n_{n} = _{n};\n"));
                }
            }
            // Declare _n_ temporaries for port reg shadows
            for p in &m.ports {
                if p.reg_info.is_some() {
                    let n = &p.name.name;
                    if let Some(vi) = vec_port_infos.iter().find(|v| v.name == *n) {
                        // Vec port-reg: _n_ is an array, initialized by memcpy
                        cpp.push_str(&format!(
                            "  {} _n_{n}[{}]; memcpy(_n_{n}, _{n}, sizeof(_{n}));\n",
                            vi.elem_ty, vi.count
                        ));
                    } else {
                        let ty = cpp_internal_type_with_params(&p.ty, &m.params);
                        cpp.push_str(&format!("  {ty} _n_{n} = _{n};\n"));
                    }
                }
            }
            // Declare _n_ temporaries for pipe_reg stages
            for p in &pipe_regs {
                let w = widths.get(&p.source.name).copied().unwrap_or(32);
                let ty = if signed_names.contains(p.source.name.as_str()) {
                    cpp_sint(w)
                } else {
                    cpp_uint(w)
                };
                for i in 0..p.stages {
                    let name = if i == p.stages - 1 {
                        p.name.name.clone()
                    } else {
                        format!("{}_stg{}", p.name.name, i + 1)
                    };
                    cpp.push_str(&format!("  {ty} _n_{name} = _{name};\n"));
                }
            }
            cpp.push('\n');

            let ctx = Ctx::new(
                &reg_names,
                &port_names,
                &let_names,
                &inst_names,
                &wide_names,
                &widths,
                &enum_map,
                &bus_port_names,
            )
            .with_signed_names(&signed_names)
            .with_float_names(&float_names)
            .with_decl_types(&decl_types, &struct_defs)
            .with_reset_levels(&reset_levels)
            .with_vec_names(&vec_reg_names)
            .with_vec_2d_names(&vec_2d_names)
            .with_vec_sizes(&vec_sizes)
            .posedge()
            .with_coverage(cov_handle)
            .with_let_values(&let_values)
            .with_params(&m.params)
            .with_vinit_regs(&vinit_regs);

            for rb in &reg_blocks {
                let mut assigned = std::collections::BTreeSet::new();
                collect_stmt_assigns(&rb.stmts, &mut assigned);

                let mut reset_sig: Option<(String, bool, bool)> = None;
                let mut reset_regs: Vec<(&str, String)> = Vec::new();

                for name in &assigned {
                    // Look up reset from RegDecl or port reg
                    let reset_ref: Option<&RegReset> = reg_decls
                        .iter()
                        .find(|r| r.name.name == *name)
                        .map(|r| &r.reset)
                        .or_else(|| {
                            m.ports
                                .iter()
                                .find(|p| p.name.name == *name && p.reg_info.is_some())
                                .and_then(|p| p.reg_info.as_ref().map(|ri| &ri.reset))
                        });
                    if let Some(reg_reset) = reset_ref {
                        if let Some(info) = resolve_reg_reset_info(reg_reset, &m.ports) {
                            if reset_sig.is_none() {
                                reset_sig = Some(info.clone());
                            }
                            let reset_expr = reset_value_from_reg_reset(reg_reset);
                            let init_val = if let Some(expr) = reset_expr {
                                match &expr.kind {
                                    // Literal/bool shortcuts keep the emitted
                                    // reset branch readable (`_n_foo = 5;`
                                    // rather than a pointlessly wrapped expr).
                                    ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                                    ExprKind::Literal(LitKind::Hex(v)) => format!("0x{:X}", v),
                                    ExprKind::Literal(LitKind::Bin(v)) => v.to_string(),
                                    ExprKind::Literal(LitKind::Sized(_, v)) => v.to_string(),
                                    ExprKind::Bool(b) => {
                                        if *b {
                                            "1".to_string()
                                        } else {
                                            "0".to_string()
                                        }
                                    }
                                    // Everything else — struct literals, enum
                                    // variants, idents, calls, casts — lowers
                                    // via the normal expression path. Previously
                                    // this default silently emitted "0", which
                                    // could corrupt non-literal reset values
                                    // (see #6 struct-literal reset bug).
                                    _ => {
                                        let tmp_ctx = Ctx::new(
                                            &reg_names,
                                            &port_names,
                                            &let_names,
                                            &inst_names,
                                            &wide_names,
                                            &widths,
                                            &enum_map,
                                            &bus_port_names,
                                        )
                                        .with_signed_names(&signed_names)
                                        .with_reset_levels(&reset_levels)
                                        .with_float_names(&float_names)
                                        .with_decl_types(&decl_types, &struct_defs);
                                        cpp_expr(expr, &tmp_ctx)
                                    }
                                }
                            } else {
                                "0".to_string()
                            };
                            reset_regs.push((name.as_str(), init_val));
                        }
                    }
                }

                // For async reset, emit the reset arm OUTSIDE the rising-edge
                // gate so an asserted reset clears the regs immediately
                // (visible to the very next eval_comb()), not only after
                // the next clock edge. Write to both `_q` (the live,
                // user-visible value) and `_n_q` (the shadow) so the
                // end-of-cycle commit doesn't restore stale state. The
                // sync-reset case keeps the original gated form.
                let async_reset_emitted = if let Some((rst_name, is_async, is_low)) = &reset_sig {
                    if !is_async {
                        // sync-reset path is handled below; skip the async pre-gate emit
                        false
                    } else {
                        let cond = if *is_low {
                            format!("(!{})", rst_name)
                        } else {
                            rst_name.clone()
                        };
                        cpp.push_str(&format!("  if ({cond}) {{\n"));
                        for (reg_name, init) in &reset_regs {
                            // Vec-typed regs are C arrays — write each element
                            // via a loop. `init` is a scalar broadcast value
                            // per the ARCH spec (`reset r => 0` distributes
                            // the scalar across every element).
                            if vec_reg_names.contains(*reg_name) {
                                let count = vec_sizes.get(*reg_name).copied().unwrap_or(0);
                                if count > 0 {
                                    // For nested-Vec regs the inner dim is a C array,
                                    // not a scalar — a per-element scalar assign
                                    // `_rf[_i] = 0` fails to compile. memset zeroes
                                    // the whole storage in one shot regardless of
                                    // dimensionality; the spec's reset broadcast
                                    // semantics (scalar distributed across every
                                    // element) collapse to the zero case in
                                    // practice. For non-zero broadcasts, fall back
                                    // to the per-element-loop form (correct for
                                    // 1D, generates a compile error for nested-
                                    // Vec the user must resolve by writing the
                                    // reset by hand).
                                    if init == "0" {
                                        cpp.push_str(&format!(
                                            "    memset(_{reg_name}, 0, sizeof(_{reg_name}));\n"
                                        ));
                                        cpp.push_str(&format!("    memset(_n_{reg_name}, 0, sizeof(_n_{reg_name}));\n"));
                                    } else {
                                        cpp.push_str(&format!("    for (size_t _i = 0; _i < {count}; ++_i) {{ _{reg_name}[_i] = {init}; _n_{reg_name}[_i] = {init}; }}\n"));
                                    }
                                }
                            } else if wide_names.contains(*reg_name) {
                                let bits = widths.get(*reg_name).copied().unwrap_or(0);
                                if bits > 128 {
                                    let words = wide_words(bits);
                                    cpp.push_str(&format!(
                                        "    _{reg_name} = VlWide<{words}>({init});\n"
                                    ));
                                    cpp.push_str(&format!(
                                        "    _n_{reg_name} = VlWide<{words}>({init});\n"
                                    ));
                                } else {
                                    cpp.push_str(&format!(
                                        "    _{reg_name} = (_arch_u128){init};\n"
                                    ));
                                    cpp.push_str(&format!(
                                        "    _n_{reg_name} = (_arch_u128){init};\n"
                                    ));
                                }
                            } else {
                                cpp.push_str(&format!("    _{reg_name} = {init};\n"));
                                cpp.push_str(&format!("    _n_{reg_name} = {init};\n"));
                            }
                        }
                        cpp.push_str("  }\n");
                        true
                    }
                } else {
                    false
                };

                // Guard each seq block on its specific clock's rising edge.
                // For async reset: use `else if` so the seq body is skipped
                // when reset was active — the reset arm already cleared the
                // regs; executing the seq body (e.g. toggle) would overwrite.
                let rising_gate = if async_reset_emitted {
                    format!("  else if (_rising_{}) {{\n", rb.clock.name)
                } else {
                    format!("  if (_rising_{}) {{\n", rb.clock.name)
                };
                cpp.push_str(&rising_gate);
                let base_indent: usize = 2;
                // --coverage phase 2: count seq-block entries (rising
                // edges seen). One counter per top-level seq block;
                // catches dead clock domains where branch coverage
                // shows 0/0 trivially.
                if let Some(reg) = cov_handle {
                    let idx = reg.borrow_mut().alloc(
                        "seq",
                        rb.span.start,
                        format!("seq @{}", rb.clock.name),
                    );
                    cpp.push_str(&format!(
                        "{}_arch_cov[{idx}]++;\n",
                        "  ".repeat(base_indent)
                    ));
                }

                if async_reset_emitted {
                    // Seq body: reset already cleared regs above; any
                    // read-modify-write patterns (e.g. toggle) now see
                    // the reset-cleared value.
                    let mut body = String::new();
                    emit_reg_stmts(&rb.stmts, &ctx, &mut body, base_indent);
                    cpp.push_str(&body);
                } else if let Some((rst_name, _is_async, is_low)) = &reset_sig {
                    // Sync reset — original gated form.
                    let cond = if *is_low {
                        format!("(!{})", rst_name)
                    } else {
                        rst_name.clone()
                    };
                    cpp.push_str(&format!("{}if ({cond}) {{\n", "  ".repeat(base_indent)));
                    for (reg_name, init) in &reset_regs {
                        if vec_reg_names.contains(*reg_name) {
                            let count = vec_sizes.get(*reg_name).copied().unwrap_or(0);
                            if count > 0 {
                                // memset for zero (covers nested-Vec); per-element
                                // loop for non-zero broadcasts (works for 1D).
                                if init == "0" {
                                    cpp.push_str(&format!(
                                        "{}memset(_n_{reg_name}, 0, sizeof(_n_{reg_name}));\n",
                                        "  ".repeat(base_indent + 1)
                                    ));
                                } else {
                                    cpp.push_str(&format!("{}for (size_t _i = 0; _i < {count}; ++_i) {{ _n_{reg_name}[_i] = {init}; }}\n", "  ".repeat(base_indent + 1)));
                                }
                            }
                        } else if wide_names.contains(*reg_name) {
                            let bits = widths.get(*reg_name).copied().unwrap_or(0);
                            if bits > 128 {
                                let words = wide_words(bits);
                                cpp.push_str(&format!(
                                    "{}_n_{reg_name} = VlWide<{words}>({init});\n",
                                    "  ".repeat(base_indent + 1)
                                ));
                            } else {
                                cpp.push_str(&format!(
                                    "{}_n_{reg_name} = (_arch_u128){init};\n",
                                    "  ".repeat(base_indent + 1)
                                ));
                            }
                        } else {
                            cpp.push_str(&format!(
                                "{}_n_{reg_name} = {init};\n",
                                "  ".repeat(base_indent + 1)
                            ));
                        }
                    }
                    cpp.push_str(&format!("{}}} else {{\n", "  ".repeat(base_indent)));
                    let mut body = String::new();
                    emit_reg_stmts(&rb.stmts, &ctx, &mut body, base_indent + 1);
                    cpp.push_str(&body);
                    cpp.push_str(&format!("{}}}\n", "  ".repeat(base_indent)));
                } else {
                    let mut body = String::new();
                    emit_reg_stmts(&rb.stmts, &ctx, &mut body, base_indent);
                    cpp.push_str(&body);
                }

                cpp.push_str("  }\n");
            }

            // pipe_reg chain assignments — write to _n_ temporaries (before commit).
            // Gate on the primary clock's rising edge so stages advance once per
            // clock cycle, not per eval() call.
            let pipe_reg_clk = all_clks.first().cloned();
            if !pipe_regs.is_empty() {
                if let Some(clk) = &pipe_reg_clk {
                    cpp.push_str(&format!("  if (_rising_{clk}) {{\n"));
                }
            }
            {
                let rst_info = m
                    .ports
                    .iter()
                    .find(|p| matches!(&p.ty, TypeExpr::Reset(..)))
                    .map(|p| {
                        let is_low =
                            matches!(&p.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low);
                        (p.name.name.clone(), is_low)
                    });
                for p in &pipe_regs {
                    let mut chain: Vec<String> = Vec::new();
                    for i in 0..p.stages {
                        if i == p.stages - 1 {
                            chain.push(p.name.name.clone());
                        } else {
                            chain.push(format!("{}_stg{}", p.name.name, i + 1));
                        }
                    }
                    let ctx_pe = Ctx::new(
                        &reg_names,
                        &port_names,
                        &let_names,
                        &inst_names,
                        &wide_names,
                        &widths,
                        &enum_map,
                        &bus_port_names,
                    )
                    .with_signed_names(&signed_names)
                    .with_float_names(&float_names)
                    .with_decl_types(&decl_types, &struct_defs)
                    .with_reset_levels(&reset_levels)
                    .with_vec_names(&vec_reg_names)
                    .with_vec_2d_names(&vec_2d_names)
                    .with_vec_sizes(&vec_sizes)
                    .with_let_values(&let_values)
                    .with_params(&m.params);
                    let src = ctx_pe.resolve_name(&p.source.name, false);
                    if let Some((ref rst_name, is_low)) = rst_info {
                        let cond = if is_low {
                            format!("(!{})", rst_name)
                        } else {
                            rst_name.clone()
                        };
                        cpp.push_str(&format!("  if ({cond}) {{\n"));
                        for name in &chain {
                            cpp.push_str(&format!("    _n_{name} = 0;\n"));
                        }
                        cpp.push_str("  } else {\n");
                        for name in &chain {
                            let prev = if *name == chain[0] {
                                src.clone()
                            } else {
                                let idx = chain.iter().position(|n| n == name).unwrap();
                                format!("_{}", chain[idx - 1])
                            };
                            cpp.push_str(&format!("    _n_{name} = {prev};\n"));
                        }
                        cpp.push_str("  }\n");
                    } else {
                        for name in &chain {
                            let prev = if *name == chain[0] {
                                src.clone()
                            } else {
                                let idx = chain.iter().position(|n| n == name).unwrap();
                                format!("_{}", chain[idx - 1])
                            };
                            cpp.push_str(&format!("  _n_{name} = {prev};\n"));
                        }
                    }
                }
            }
            if !pipe_regs.is_empty() && pipe_reg_clk.is_some() {
                cpp.push_str("  }\n");
            }

            // --debug-fsm: save old state values before commit
            if self.debug_fsm {
                for rd in &reg_decls {
                    let n = &rd.name.name;
                    if is_thread_fsm_state_reg(n) {
                        let ty = cpp_internal_type_with_params(&rd.ty, &m.params);
                        cpp.push_str(&format!("  {ty} _dbg_old_{n} = _{n};\n"));
                    }
                }
            }

            // Commit all _n_ temporaries (regs + pipe_regs)
            cpp.push('\n');
            for rd in &reg_decls {
                let n = &rd.name.name;
                if let Some((_, count_str)) = vec_array_info_with_params(&rd.ty, &m.params) {
                    // --coverage phase 4b: per-Vec-reg aggregate toggle
                    // counter — sum of popcount(prev XOR new) across all
                    // elements. One counter per Vec reg (not per element)
                    // to keep the dump size manageable; the per-element
                    // breakdown stays a future opt-in.
                    if let Some(reg) = cov_handle {
                        if let TypeExpr::Vec(elem_ty, _) = &rd.ty {
                            let elem_bits = type_bits_te_with_params(elem_ty, &m.params);
                            if elem_bits > 0 && elem_bits <= 64 {
                                let cidx = reg.borrow_mut().alloc(
                                    "toggle",
                                    rd.name.span.start,
                                    format!("toggle {n}[]"),
                                );
                                cpp.push_str(&format!(
                                    "  for (uint32_t _ti = 0; _ti < {count_str}; _ti++) {{ _arch_cov[{cidx}] += __builtin_popcountll((uint64_t)_{n}[_ti] ^ (uint64_t)_n_{n}[_ti]); }}\n"
                                ));
                            }
                        }
                    }
                    cpp.push_str(&format!("  memcpy(_{n}, _n_{n}, sizeof(_{n}));\n"));
                } else {
                    // --coverage phase 4: toggle counter — popcount of
                    // (prev XOR new) sums all bits that flipped this
                    // posedge. Skip Vec / wide regs in v1 (Vec needs
                    // per-element handling; wide needs split popcount).
                    // Skip enums — toggle on a state reg is mostly
                    // noise, FSM coverage is more useful there.
                    if let Some(reg) = cov_handle {
                        let bits = type_bits_te_with_params(&rd.ty, &m.params);
                        if bits > 0 && bits <= 64 && !matches!(rd.ty, TypeExpr::Named(_)) {
                            let cidx = reg.borrow_mut().alloc(
                                "toggle",
                                rd.name.span.start,
                                format!("toggle {n}"),
                            );
                            cpp.push_str(&format!(
                                "  _arch_cov[{cidx}] += __builtin_popcountll((uint64_t)_{n} ^ (uint64_t)_n_{n});\n"
                            ));
                        }
                    }
                    cpp.push_str(&format!("  _{n} = _n_{n};\n"));
                }
            }
            for p in &pipe_regs {
                for i in 0..p.stages {
                    let name = if i == p.stages - 1 {
                        p.name.name.clone()
                    } else {
                        format!("{}_stg{}", p.name.name, i + 1)
                    };
                    cpp.push_str(&format!("  _{name} = _n_{name};\n"));
                }
            }

            // Commit port reg shadows: _n_ → shadow → public port
            for p in &m.ports {
                if p.reg_info.is_some() {
                    let n = &p.name.name;
                    if let Some(vi) = vec_port_infos.iter().find(|v| v.name == *n) {
                        // Vec port-reg: memcpy shadow array, then fan out to flat fields
                        cpp.push_str(&format!("  memcpy(_{n}, _n_{n}, sizeof(_{n}));\n"));
                        for i in 0..vi.count {
                            cpp.push_str(&format!("  {n}_{i} = _{n}[{i}];\n"));
                        }
                    } else {
                        cpp.push_str(&format!("  _{n} = _n_{n};\n"));
                        cpp.push_str(&format!("  {n} = _{n};\n"));
                    }
                }
            }

            // --debug-fsm: print state transitions for thread-lowered FSM regs
            if self.debug_fsm {
                for rd in &reg_decls {
                    let n = &rd.name.name;
                    if is_thread_fsm_state_reg(n) {
                        let label = n.trim_start_matches('_');
                        cpp.push_str(&format!(
                            "  if (_{n} != _dbg_old_{n}) \
                             printf(\"[FSM][{module_name}.{label}] S%u -> S%u\\n\", \
                             (unsigned)_dbg_old_{n}, (unsigned)_{n});\n",
                            module_name = name,
                            label = label,
                            n = n,
                        ));
                    }
                }
            }

            // --check-uninit: propagate vinit for pipe_reg stages
            if !uninit_regs.is_empty() {
                for p in &pipe_regs {
                    let mut chain: Vec<String> = Vec::new();
                    for i in 0..p.stages {
                        if i == p.stages - 1 {
                            chain.push(p.name.name.clone());
                        } else {
                            chain.push(format!("{}_stg{}", p.name.name, i + 1));
                        }
                    }
                    // Propagate vinit in reverse (like data) — shift valid bits
                    for i in (0..chain.len()).rev() {
                        let prev_vinit = if i == 0 {
                            // Source's vinit: check if source is an uninit reg
                            if uninit_regs.contains(&p.source.name) {
                                format!("_{}_vinit", p.source.name)
                            } else {
                                "true".to_string() // source is always valid (port, let, or reset-initialized reg)
                            }
                        } else {
                            format!("_{}_vinit", chain[i - 1])
                        };
                        cpp.push_str(&format!("  _{}_vinit = {};\n", chain[i], prev_vinit));
                    }
                }
            }

            // Guard Check A: for each `reg ... guard <sig>`, warn if guard asserts
            // but the reg has never been written. Fires once per module per signal.
            // Sort for deterministic output — HashMap iteration order is not stable.
            let mut sorted_guarded_regs: Vec<(&String, &String)> = guarded_regs.iter().collect();
            sorted_guarded_regs.sort();
            for (reg_name, guard_sig) in sorted_guarded_regs {
                let guard_cpp = if reg_decls.iter().any(|r| r.name.name == *guard_sig)
                    || m.ports
                        .iter()
                        .any(|p| p.name.name == *guard_sig && p.reg_info.is_some())
                {
                    format!("_{guard_sig}")
                } else {
                    guard_sig.clone()
                };
                cpp.push_str(&format!(
                    "  if ({guard_cpp} && !_{reg_name}_vinit) {{\n\
                     \x20   static bool _w_{reg_name}_guard = false;\n\
                     \x20   if (!_w_{reg_name}_guard) {{\n\
                     \x20     _w_{reg_name}_guard = true;\n\
                     \x20     fprintf(stderr, \"GUARD VIOLATION: {name}.{reg_name} — \"\n\
                     \x20             \"{guard_sig}=1 but {reg_name} was never written\\n\");\n\
                     \x20   }}\n\
                     \x20 }}\n",
                    guard_cpp = guard_cpp,
                    guard_sig = guard_sig,
                    reg_name = reg_name,
                    name = name,
                ));
            }
        }

        // Propagate eval_posedge to sub-instances unconditionally.
        // Each sub-instance tracks its own _clk_prev and determines internally
        // whether this call is a rising edge. Guarding with the parent's
        // _rising_clk would prevent the child's _clk_prev from being updated on
        // falling edges, causing the child to miss every other rising edge.
        if !insts.is_empty() {
            for inst in &insts {
                cpp.push_str(&format!("  _inst_{}.eval_posedge();\n", inst.name.name));
            }
        }

        // Credit-channel counter update (sender side). Gated on the
        // primary clock's rising edge and the module's first reset port
        // (active-high / active-low derived from the port's polarity).
        if !cc_sites.is_empty() {
            let primary_clk = all_clks.first().cloned();
            let rst_expr = m
                .ports
                .iter()
                .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
                .map(|p| match &p.ty {
                    TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                    _ => p.name.name.clone(),
                });
            if let Some(clk) = primary_clk {
                cpp.push_str(&format!("  if (_rising_{clk}) {{\n"));
                crate::sim_credit_channel::emit_posedge_updates(
                    &cc_sites,
                    rst_expr.as_deref(),
                    &mut cpp,
                );
                cpp.push_str("  }\n");
            }
        }

        cpp.push_str("}\n\n");

        // eval_comb()
        // For modules with sub-instances, eval_comb includes re-evaluation of the
        // inst chain so that combinational feedback settles when called from parent.
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        let ctx_comb = Ctx::new(
            &reg_names,
            &port_names,
            &let_names,
            &inst_names,
            &wide_names,
            &widths,
            &enum_map,
            &bus_port_names,
        )
        .with_signed_names(&signed_names)
        .with_float_names(&float_names)
        .with_decl_types(&decl_types, &struct_defs)
        .with_reset_levels(&reset_levels)
        .with_vec_names(&vec_reg_names)
        .with_vec_2d_names(&vec_2d_names)
        .with_vec_sizes(&vec_sizes)
        .with_coverage(cov_handle)
        .with_let_values(&let_values)
        .with_params(&m.params)
        .with_vec_of_bus(
            &vec_of_bus_port_count_map,
            &vec_of_bus_wire_count_map,
            &loop_var_subst_cell,
        );

        // eval_comb() must reach the module's comb fixed point on its own.
        // When this module is instantiated as a child, the parent's settle
        // loop calls eval_comb() once per parent pass — and the parent's
        // settle_depth knows nothing about feedback *internal* to this
        // module (e.g. thread req → arbiter inst → grant → thread comb).
        // Without an internal settle loop, each call advances that
        // feedback by only one iteration, so grants lag requests by a
        // full cycle relative to Verilator on the same SV. Modules with
        // no internal feedback (settle_depth == 1) keep the single pass.
        let comb_settle_depth = if !insts.is_empty() { settle_depth } else { 1 };
        if comb_settle_depth > 1 {
            cpp.push_str(&format!(
                "  for (int _settle = 0; _settle < {comb_settle_depth}; _settle++) {{\n"
            ));
        }

        // Credit-channel combinational wires (sender can_send; receiver
        // valid/data once PR-sim-2 lands). Emit early so user comb code
        // can read them.
        crate::sim_credit_channel::emit_comb_updates(&cc_sites, &mut cpp);

        // Flat → internal bridge for input Vec ports (non-reg)
        for vi in &vec_port_infos {
            if vi.is_input && !vi.is_port_reg {
                let n = &vi.name;
                for i in 0..vi.count {
                    cpp.push_str(&format!("  _{n}[{i}] = {n}_{i};\n"));
                }
            }
        }

        // Let bindings → private fields (assign before inst eval so instances see current values)
        for item in &m.body {
            if let ModuleBodyItem::LetBinding(l) = item {
                // Destructuring: emit one assignment per bound field.
                if !l.destructure_fields.is_empty() {
                    // Special case: RHS is `vec.find_first(pred)`. Emit the
                    // raw OR + priority encoder directly; avoids the
                    // non-existent `.find_first()` member access on C++
                    // vector fields.
                    if let ExprKind::MethodCall(recv, mname, margs) = &l.value.kind {
                        if mname.name == "find_first" {
                            let recv_cpp = cpp_expr(recv, &ctx_comb);
                            let n = match &recv.kind {
                                ExprKind::Ident(nm) => {
                                    ctx_comb.vec_sizes.and_then(|s| s.get(nm)).copied()
                                }
                                _ => None,
                            };
                            if let Some(n) = n {
                                // Build per-iteration predicate strings.
                                let mut hits: Vec<String> = Vec::with_capacity(n as usize);
                                for i in 0..n {
                                    let mut sub: HashMap<String, String> = HashMap::new();
                                    sub.insert("item".to_string(), format!("{recv_cpp}[{i}]"));
                                    sub.insert("index".to_string(), format!("{i}"));
                                    let sub_ctx = Ctx {
                                        reg_names: ctx_comb.reg_names,
                                        port_names: ctx_comb.port_names,
                                        let_names: ctx_comb.let_names,
                                        let_values: ctx_comb.let_values,
                                        inst_names: ctx_comb.inst_names,
                                        wide_names: ctx_comb.wide_names,
                                        widths: ctx_comb.widths,
                                        signed_names: ctx_comb.signed_names,
                                        float_names: ctx_comb.float_names,
                                        posedge_lhs: ctx_comb.posedge_lhs,
                                        fsm_mode: ctx_comb.fsm_mode,
                                        enum_map: ctx_comb.enum_map,
                                        bus_ports: ctx_comb.bus_ports,
                                        reset_levels: ctx_comb.reset_levels,
                                        vec_names: ctx_comb.vec_names,
                                        vec_2d_names: ctx_comb.vec_2d_names,
                                        vec_sizes: ctx_comb.vec_sizes,
                                        fsm_vec_port_regs: ctx_comb.fsm_vec_port_regs,
                                        ident_subst: Some(&sub),
                                        loop_var_subst: ctx_comb.loop_var_subst,
                                        vec_of_bus_port_count: ctx_comb.vec_of_bus_port_count,
                                        vec_of_bus_wire_count: ctx_comb.vec_of_bus_wire_count,
                                        coverage: ctx_comb.coverage,
                                        params: ctx_comb.params,
                                        vinit_regs: ctx_comb.vinit_regs,
                                        decl_types: ctx_comb.decl_types,
                                        struct_defs: ctx_comb.struct_defs,
                                    };
                                    hits.push(cpp_expr(&margs[0], &sub_ctx));
                                }
                                let found_expr: String = hits
                                    .iter()
                                    .map(|h| format!("({h})"))
                                    .collect::<Vec<_>>()
                                    .join(" || ");
                                let mut idx_expr = "0u".to_string();
                                for i in (0..n as u64).rev() {
                                    let hit = &hits[i as usize];
                                    idx_expr = format!("(({hit}) ? (uint32_t){i} : {idx_expr})");
                                }
                                for bind in &l.destructure_fields {
                                    let rhs = match bind.name.as_str() {
                                        "found" => format!("({found_expr})"),
                                        "index" => idx_expr.clone(),
                                        _ => continue,
                                    };
                                    cpp.push_str(&format!(
                                        "  _let_{fn} = {rhs};\n", fn = bind.name
                                    ));
                                }
                                continue;
                            }
                        }
                    }
                    let val = cpp_expr(&l.value, &ctx_comb);
                    for bind in &l.destructure_fields {
                        cpp.push_str(&format!(
                            "  _let_{fn} = {val}.{fn};\n", fn = bind.name
                        ));
                    }
                    continue;
                }
                let val = cpp_expr(&l.value, &ctx_comb);
                if l.ty.is_none() {
                    // ty=None: assignment to existing port or wire
                    let name = &l.name.name;
                    if port_names.contains(name) {
                        // Output port — public field, plain name. Wide ports
                        // need the same 65–128-bit conversion stmt_codegen's
                        // comb arm applies: expression-context RHS is
                        // _arch_u128, the port is VlWide<ceil(W/32)>, and a
                        // bare assignment truncates through uint64_t
                        // (`let y = {a, b};` with y: out UInt<128> dropped
                        // the high word pair — found while fixing arch#858).
                        if wide_names.contains(name.as_str()) {
                            let bits = widths.get(name.as_str()).copied().unwrap_or(0);
                            if bits > 128 {
                                // >128 bits: both sides are VlWide<N> — direct assign.
                                cpp.push_str(&format!("  {name} = {val};\n"));
                            } else {
                                cpp.push_str(&format!(
                                    "  _arch_u128_to_vl({val}, {name}._data, {});\n",
                                    wide_words(bits)
                                ));
                            }
                        } else {
                            cpp.push_str(&format!("  {name} = {val};\n"));
                        }
                    } else {
                        // Wire — private field with _let_ prefix
                        cpp.push_str(&format!("  _let_{name} = {val};\n"));
                    }
                } else {
                    cpp.push_str(&format!("  _let_{} = {};\n", l.name.name, val));
                }
            }
        }

        // If there are sub-instances, re-evaluate the inst chain.
        // Topological order (producers before consumers) — the settle_depth
        // analysis assumes it, and eval() relies on this chain entirely now
        // that it delegates instead of running its own settle loop.
        if !insts.is_empty() {
            for &inst_i in &inst_eval_order {
                let inst = insts[inst_i];
                let conns = &expanded_conns[inst_i];
                for conn in conns {
                    if conn.direction == ConnectDir::Input {
                        if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                            if let Some(n) = ctx_comb.expr_vec_size(&conn.signal) {
                                let sig = cpp_expr(&conn.signal, &ctx_comb);
                                for i in 0..n {
                                    cpp.push_str(&format!(
                                        "  _inst_{}.{}_{i} = {sig}[{i}];\n",
                                        inst.name.name, conn.port_name.name
                                    ));
                                }
                                continue;
                            }
                        }
                        if let crate::ast::ExprKind::Ident(src_name) = &conn.signal.kind {
                            // Vec wire/reg → inst Vec port: expand element-by-element
                            if let Some(&n) = vec_wire_counts.get(src_name.as_str()) {
                                let _vec_pfx = vec_storage_prefix(
                                    src_name.as_str(),
                                    &reg_names,
                                    &let_names,
                                    &inst_out,
                                );
                                for i in 0..n {
                                    cpp.push_str(&format!(
                                        "  _inst_{}.{}_{i} = {_vec_pfx}{src_name}[{i}];\n",
                                        inst.name.name, conn.port_name.name
                                    ));
                                }
                                continue;
                            }
                            // Parent Vec PORT (input) → inst Vec port: flat field syntax
                            if vec_port_names.contains(src_name.as_str()) {
                                let n = vec_port_infos
                                    .iter()
                                    .find(|v| v.name == *src_name)
                                    .map(|v| v.count)
                                    .unwrap_or(0);
                                for i in 0..n {
                                    cpp.push_str(&format!(
                                        "  _inst_{}.{}_{i} = {src_name}_{i};\n",
                                        inst.name.name, conn.port_name.name
                                    ));
                                }
                                continue;
                            }
                            if wide_names.contains(src_name.as_str()) {
                                let resolved = ctx_comb.resolve_name(src_name, false);
                                if let Some((packed_port, index)) =
                                    self.indexed_arbiter_port(inst, &conn.port_name.name)
                                {
                                    cpp.push_str(&format!(
                                        "  _inst_{}.{} = (_inst_{}.{} & ~(1ULL << {})) | ((((uint64_t)({})) & 1ULL) << {});\n",
                                        inst.name.name, packed_port, inst.name.name, packed_port, index, resolved, index
                                    ));
                                    continue;
                                }
                                cpp.push_str(&format!(
                                    "  _inst_{}.{} = {};\n",
                                    inst.name.name, conn.port_name.name, resolved
                                ));
                                continue;
                            }
                        }
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some((_elem_ty, count, elem_bits, array_storage)) =
                                inst_vec_out.get(sig_name)
                            {
                                let total_bits = elem_bits.saturating_mul(*count as u32);
                                if !*array_storage && *elem_bits > 0 && total_bits <= 64 {
                                    let sig = cpp_expr(&conn.signal, &ctx_comb);
                                    cpp.push_str(&format!("  {sig} = 0;\n"));
                                    let mask = if *elem_bits >= 64 {
                                        "UINT64_MAX".to_string()
                                    } else {
                                        format!("((1ULL << {elem_bits}) - 1ULL)")
                                    };
                                    for i in 0..*count {
                                        let shift = i as u32 * *elem_bits;
                                        cpp.push_str(&format!(
                                            "  {sig} |= (((uint64_t)_inst_{}.{}_{i}) & {mask}) << {shift};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                            }
                        }
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some((_elem_ty, count, elem_bits, array_storage)) =
                                inst_vec_out.get(sig_name)
                            {
                                let total_bits = elem_bits.saturating_mul(*count as u32);
                                if !*array_storage && *elem_bits > 0 && total_bits <= 64 {
                                    let sig = cpp_expr(&conn.signal, &ctx_comb);
                                    cpp.push_str(&format!("  {sig} = 0;\n"));
                                    let mask = if *elem_bits >= 64 {
                                        "UINT64_MAX".to_string()
                                    } else {
                                        format!("((1ULL << {elem_bits}) - 1ULL)")
                                    };
                                    for i in 0..*count {
                                        let shift = i as u32 * *elem_bits;
                                        cpp.push_str(&format!(
                                            "  {sig} |= (((uint64_t)_inst_{}.{}_{i}) & {mask}) << {shift};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                            }
                        }
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some((count, elem_bits)) =
                                child_vec_output_shape(inst, &conn.port_name.name)
                            {
                                let total_bits = elem_bits.saturating_mul(count as u32);
                                if !vec_wire_counts.contains_key(sig_name.as_str())
                                    && !vec_port_names.contains(sig_name.as_str())
                                    && elem_bits > 0
                                    && total_bits <= 64
                                {
                                    let sig = cpp_expr(&conn.signal, &ctx_comb);
                                    cpp.push_str(&format!("  {sig} = 0;\n"));
                                    let mask = if elem_bits >= 64 {
                                        "UINT64_MAX".to_string()
                                    } else {
                                        format!("((1ULL << {elem_bits}) - 1ULL)")
                                    };
                                    for i in 0..count {
                                        let shift = i as u32 * elem_bits;
                                        cpp.push_str(&format!(
                                            "  {sig} |= (((uint64_t)_inst_{}.{}_{i}) & {mask}) << {shift};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                            }
                        }
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some((count, elem_bits)) =
                                child_vec_output_shape(inst, &conn.port_name.name)
                            {
                                let total_bits = elem_bits.saturating_mul(count as u32);
                                if !vec_wire_counts.contains_key(sig_name.as_str())
                                    && !vec_port_names.contains(sig_name.as_str())
                                    && elem_bits > 0
                                    && total_bits <= 64
                                {
                                    let sig = cpp_expr(&conn.signal, &ctx_comb);
                                    cpp.push_str(&format!("  {sig} = 0;\n"));
                                    let mask = if elem_bits >= 64 {
                                        "UINT64_MAX".to_string()
                                    } else {
                                        format!("((1ULL << {elem_bits}) - 1ULL)")
                                    };
                                    for i in 0..count {
                                        let shift = i as u32 * elem_bits;
                                        cpp.push_str(&format!(
                                            "  {sig} |= (((uint64_t)_inst_{}.{}_{i}) & {mask}) << {shift};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                            }
                        }
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some((count, elem_bits)) =
                                child_vec_output_shape(inst, &conn.port_name.name)
                            {
                                let total_bits = elem_bits.saturating_mul(count as u32);
                                if !vec_wire_counts.contains_key(sig_name.as_str())
                                    && !vec_port_names.contains(sig_name.as_str())
                                    && elem_bits > 0
                                    && total_bits <= 64
                                {
                                    let sig = cpp_expr(&conn.signal, &ctx_comb);
                                    cpp.push_str(&format!("  {sig} = 0;\n"));
                                    let mask = if elem_bits >= 64 {
                                        "UINT64_MAX".to_string()
                                    } else {
                                        format!("((1ULL << {elem_bits}) - 1ULL)")
                                    };
                                    for i in 0..count {
                                        let shift = i as u32 * elem_bits;
                                        cpp.push_str(&format!(
                                            "  {sig} |= (((uint64_t)_inst_{}.{}_{i}) & {mask}) << {shift};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx_comb);
                        // Wide type (>64 bits): parent _arch_u128 → inst VlWide
                        let _in_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                            widths.get(n.as_str()).copied().unwrap_or(0)
                        } else {
                            0
                        };
                        if _in_w > 64 {
                            cpp.push_str(&format!(
                                "  _arch_u128_to_vl({}, _inst_{}.{}.data(), {});\n",
                                sig,
                                inst.name.name,
                                conn.port_name.name,
                                wide_words(_in_w)
                            ));
                        } else if let Some((packed_port, index)) =
                            self.indexed_arbiter_port(inst, &conn.port_name.name)
                        {
                            cpp.push_str(&format!(
                                "  _inst_{}.{} = (_inst_{}.{} & ~(1ULL << {})) | ((((uint64_t)({})) & 1ULL) << {});\n",
                                inst.name.name, packed_port, inst.name.name, packed_port, index, sig, index
                            ));
                        } else {
                            cpp.push_str(&format!(
                                "  _inst_{}.{} = {};\n",
                                inst.name.name, conn.port_name.name, sig
                            ));
                        }
                    }
                }
                cpp.push_str(&format!("  _inst_{}.eval_comb();\n", inst.name.name));
                for conn in conns {
                    if conn.direction == ConnectDir::Output {
                        if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                            if let Some(n) = ctx_comb.expr_vec_size(&conn.signal) {
                                let sig = cpp_expr(&conn.signal, &ctx_comb);
                                for i in 0..n {
                                    cpp.push_str(&format!(
                                        "  {sig}[{i}] = _inst_{}.{}_{i};\n",
                                        inst.name.name, conn.port_name.name
                                    ));
                                }
                                continue;
                            }
                        }
                        // inst Vec port → Vec wire/reg: expand element-by-element
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some(&n) = vec_wire_counts.get(sig_name.as_str()) {
                                if vec_port_names.contains(sig_name.as_str()) {
                                    // See note in the input-wiring case: write
                                    // to internal _{name}[i] storage, not flat
                                    // field, so the eval_comb-tail sync isn't
                                    // overwritten.
                                    for i in 0..n {
                                        cpp.push_str(&format!(
                                            "  _{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    if port_reg_names.contains(sig_name.as_str()) {
                                        emit_port_reg_public_copy(
                                            &mut cpp,
                                            sig_name,
                                            &widths,
                                            Some(n),
                                            "  ",
                                        );
                                    }
                                } else {
                                    let prefix = vec_storage_prefix(
                                        sig_name.as_str(),
                                        &reg_names,
                                        &let_names,
                                        &inst_out,
                                    );
                                    for i in 0..n {
                                        cpp.push_str(&format!(
                                            "  {prefix}{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                }
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx_comb);
                        let _out_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                            widths.get(n.as_str()).copied().unwrap_or(0)
                        } else {
                            0
                        };
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some((count, elem_bits)) =
                                child_vec_output_shape(inst, &conn.port_name.name)
                            {
                                let total_bits = elem_bits.saturating_mul(count as u32);
                                if !vec_wire_counts.contains_key(sig_name.as_str())
                                    && !vec_port_names.contains(sig_name.as_str())
                                    && elem_bits > 0
                                    && total_bits <= 64
                                {
                                    cpp.push_str(&format!("  {sig} = 0;\n"));
                                    let mask = if elem_bits >= 64 {
                                        "UINT64_MAX".to_string()
                                    } else {
                                        format!("((1ULL << {elem_bits}) - 1ULL)")
                                    };
                                    for i in 0..count {
                                        let shift = i as u32 * elem_bits;
                                        cpp.push_str(&format!(
                                            "  {sig} |= (((uint64_t)_inst_{}.{}_{i}) & {mask}) << {shift};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                            }
                        }
                        if _out_w > 64 {
                            cpp.push_str(&format!(
                                "  {} = _arch_vl_to_u128(_inst_{}.{}.data(), {});\n",
                                sig,
                                inst.name.name,
                                conn.port_name.name,
                                wide_words(_out_w)
                            ));
                        } else if let Some((packed_port, index)) =
                            self.indexed_arbiter_port(inst, &conn.port_name.name)
                        {
                            cpp.push_str(&format!(
                                "  {} = ((_inst_{}.{} >> {}) & 1ULL);\n",
                                sig, inst.name.name, packed_port, index
                            ));
                        } else {
                            cpp.push_str(&format!(
                                "  {} = _inst_{}.{};\n",
                                sig, inst.name.name, conn.port_name.name
                            ));
                        }
                        if let ExprKind::Ident(name) = &conn.signal.kind {
                            if port_reg_names.contains(name.as_str()) {
                                emit_port_reg_public_copy(&mut cpp, name, &widths, None, "  ");
                            }
                        }
                        // --check-uninit: mark inst output as initialized
                        if let ExprKind::Ident(name) = &conn.signal.kind {
                            if vinit_regs.contains(name.as_str()) {
                                cpp.push_str(&format!("  _{name}_vinit = true;\n"));
                            }
                        }
                    }
                }
            }
        }

        // --check-uninit: warn if any uninit reg/pipe_reg output is read in comb
        if !uninit_regs.is_empty() {
            // Collect all signal names read in comb blocks AND in let bindings
            // (let values are lowered into eval_comb too).
            let mut comb_reads: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            for item in &m.body {
                match item {
                    ModuleBodyItem::CombBlock(cb) => {
                        for stmt in &cb.stmts {
                            collect_comb_reads(stmt, &mut comb_reads);
                        }
                    }
                    ModuleBodyItem::LetBinding(l) => {
                        collect_expr_idents(&l.value, &mut comb_reads);
                    }
                    _ => {}
                }
            }
            // Check uninit regs that are read in comb (warn once per signal)
            for name in &comb_reads {
                if uninit_regs.contains(name) && !uninit_inputs.contains(name) {
                    cpp.push_str(&format!(
                        "  {{ static bool _w_{name} = false; if (!_{name}_vinit && !_w_{name}) {{ fprintf(stderr, \"WARNING: read of uninitialized reg '{name}' in {n}\\n\"); _w_{name} = true; }} }}\n",
                        name = name, n = name
                    ));
                }
            }
            // --inputs-start-uninit: warn on reads of uninit inputs anywhere in the design
            // (comb blocks, let bindings, and seq blocks). Seq reads only happen when the
            // corresponding clock edge fires, so we collect them too.
            if !uninit_inputs.is_empty() {
                let mut all_reads: std::collections::BTreeSet<String> = comb_reads.clone();
                for item in &m.body {
                    if let ModuleBodyItem::RegBlock(sb) = item {
                        for stmt in &sb.stmts {
                            collect_stmt_idents(stmt, &mut all_reads);
                        }
                    }
                    if let ModuleBodyItem::LatchBlock(lb) = item {
                        for stmt in &lb.stmts {
                            collect_stmt_idents(stmt, &mut all_reads);
                        }
                    }
                }
                for name in &all_reads {
                    if uninit_inputs.contains(name) {
                        // Tier 1.5 (Option D): if this input is a handshake
                        // payload, gate the warning on the channel's valid/req
                        // signal — only the producer bug (valid asserted but
                        // payload never set) should fire. Non-payload inputs
                        // fall through to the unconditional check.
                        let gate = payload_guards
                            .get(name)
                            .map(|g| format!(" && {g}"))
                            .unwrap_or_default();
                        cpp.push_str(&format!(
                            "  {{ static bool _w_{name} = false; if (!_{name}_vinit{gate} && !_w_{name}) {{ fprintf(stderr, \"WARNING: read of uninitialized input '{name}' — TB never called set_{name}()\\n\"); _w_{name} = true; }} }}\n",
                            name = name, gate = gate
                        ));
                    }
                }
            }
            // Check pipe_reg outputs whose source chain includes uninit regs
            for item in &m.body {
                if let ModuleBodyItem::PipeRegDecl(p) = item {
                    if comb_reads.contains(&p.name.name) {
                        let pn = &p.name.name;
                        cpp.push_str(&format!(
                            "  {{ static bool _w_{pn} = false; if (!_{pn}_vinit && !_w_{pn}) {{ fprintf(stderr, \"WARNING: read of uninitialized pipe_reg '{pn}' in {n}\\n\"); _w_{pn} = true; }} }}\n",
                            pn = pn, n = name
                        ));
                    }
                }
            }
        }

        // Comb block output assignments
        for item in &m.body {
            if let ModuleBodyItem::CombBlock(cb) = item {
                let mut body = String::new();
                // --coverage phase 2: count comb-block entries (eval_comb
                // calls per block). Caveat: comb blocks may evaluate
                // multiple times per cycle during the settle loop, so
                // counters reflect "block evaluations" rather than
                // "cycles where block was active".
                if let Some(reg) = cov_handle {
                    let idx = reg
                        .borrow_mut()
                        .alloc("comb", cb.span.start, "comb".to_string());
                    body.push_str(&format!("  _arch_cov[{idx}]++;\n"));
                }
                emit_comb_stmts(&cb.stmts, &ctx_comb, &mut body, 1);
                cpp.push_str(&body);
            }
            // Latch blocks: level-sensitive — update reg when enable is active
            if let ModuleBodyItem::LatchBlock(lb) = item {
                let en = ctx_comb.resolve_name(&lb.enable.name, false);
                cpp.push_str(&format!("  if ({}) {{\n", en));
                let mut body = String::new();
                emit_reg_stmts(&lb.stmts, &ctx_comb, &mut body, 2);
                cpp.push_str(&body);
                cpp.push_str("  }\n");
            }
        }

        // Internal → flat bridge for output Vec ports (non-reg; reg outputs are committed in eval_posedge)
        for vi in &vec_port_infos {
            if !vi.is_input && !vi.is_port_reg {
                let n = &vi.name;
                for i in 0..vi.count {
                    cpp.push_str(&format!("  {n}_{i} = _{n}[{i}];\n"));
                }
            }
        }
        if comb_settle_depth > 1 {
            cpp.push_str("  } // settle\n");
        }
        cpp.push_str("}\n");

        // Generate tick() for multi-clock modules with known frequencies
        if all_freqs_known {
            let freqs: Vec<(String, u64)> = clk_freqs
                .iter()
                .map(|(name, f)| (name.clone(), f.unwrap()))
                .collect();

            // Compute half-periods in picoseconds: half_period = 1e6 / (2 * freq_mhz)
            // To avoid floating point, use: half_period_ps = 500_000 / freq_mhz
            let half_periods: Vec<(String, u64)> = freqs
                .iter()
                .map(|(name, f)| (name.clone(), 500_000 / f))
                .collect();

            // Find GCD of all half-periods for the time step
            fn gcd(a: u64, b: u64) -> u64 {
                if b == 0 {
                    a
                } else {
                    gcd(b, a % b)
                }
            }
            let step_ps = half_periods
                .iter()
                .map(|(_, hp)| *hp)
                .reduce(|a, b| gcd(a, b))
                .unwrap();

            cpp.push_str(&format!("\nvoid {class}::tick() {{\n"));
            cpp.push_str(&format!(
                "  // Auto-generated clock driver (step = {} ps)\n",
                step_ps
            ));
            for (name, hp) in &half_periods {
                cpp.push_str(&format!(
                    "  // {name}: half-period = {hp} ps ({} MHz)\n",
                    500_000 / hp
                ));
            }
            // Toggle each clock: flip when time_ps is at a half-period boundary
            for (name, hp) in &half_periods {
                cpp.push_str(&format!("  if (time_ps % {hp} == 0) {name} = !{name};\n"));
            }
            cpp.push_str("  eval();\n");
            cpp.push_str(&format!("  time_ps += {step_ps};\n"));
            cpp.push_str("}\n");
        }

        // Trace method implementations
        cpp.push_str(&trace_cpp_impl);

        // --debug: _debug_log_ports() method
        let multi_clk = clk_ports.len() > 1;
        // Printf format for cycle prefix: single-clock uses "[%llu]", multi-clock uses "%s" with _dbg_hdr
        let cyc_fmt = if multi_clk { "%s" } else { "[%llu]" };
        let cyc_arg = if multi_clk {
            "_dbg_hdr"
        } else {
            "(unsigned long long)_dbg_cycle"
        };
        if emit_debug {
            cpp.push_str(&format!("void {class}::_debug_log_ports() {{\n"));
            if multi_clk {
                // For multi-clock modules, build a header string like "[42@wr_clk]"
                cpp.push_str("  char _dbg_hdr[80];\n");
                cpp.push_str("  snprintf(_dbg_hdr, sizeof(_dbg_hdr), \"[%llu@%s]\", (unsigned long long)_dbg_cycle, _dbg_last_clk);\n");
            }
            for p in &m.ports {
                if p.bus_info.is_some() {
                    continue;
                }
                let pname = &p.name.name;
                let dir_str = match p.direction {
                    Direction::In => "in",
                    Direction::Out => "out",
                };
                match &p.ty {
                    TypeExpr::Clock(_) => {
                        cpp.push_str(&format!("  // {pname}: clock — skipped\n"));
                        continue;
                    }
                    _ => {}
                }

                if let Some(vi) = vec_port_infos.iter().find(|v| v.name == *pname) {
                    // Vec port: compare each flat element
                    for i in 0..vi.count {
                        cpp.push_str(&format!("  if ({pname}_{i} != _dbg_prev_{pname}_{i}) {{\n"));
                        cpp.push_str(&format!(
                            "    printf(\"{cyc_fmt}[{name}.{pname}[{i}]]({dir}) 0x%llx -> 0x%llx\\n\",\n",
                            dir = dir_str
                        ));
                        cpp.push_str(&format!("           {cyc_arg},\n"));
                        cpp.push_str(&format!(
                            "           (unsigned long long)_dbg_prev_{pname}_{i},\n"
                        ));
                        cpp.push_str(&format!("           (unsigned long long){pname}_{i});\n"));
                        cpp.push_str(&format!("    _dbg_prev_{pname}_{i} = {pname}_{i};\n"));
                        cpp.push_str("  }\n");
                    }
                } else {
                    let bits = type_width_of(&p.ty);
                    if bits > 64 {
                        // Wide port: use memcmp + print all 32-bit words as hex
                        let words = wide_words(bits);
                        cpp.push_str(&format!(
                            "  if (memcmp(&{pname}, &_dbg_prev_{pname}, sizeof({pname})) != 0) {{\n"
                        ));
                        cpp.push_str(&format!(
                            "    printf(\"{cyc_fmt}[{name}.{pname}]({dir}) 0x\",\n           {cyc_arg});\n",
                            dir = dir_str
                        ));
                        // Print old value (MSB first)
                        cpp.push_str(&format!(
                            "    for (int _w = {words} - 1; _w >= 0; _w--) printf(\"%08x\", _dbg_prev_{pname}.data()[_w]);\n"
                        ));
                        cpp.push_str("    printf(\" -> 0x\");\n");
                        // Print new value
                        cpp.push_str(&format!(
                            "    for (int _w = {words} - 1; _w >= 0; _w--) printf(\"%08x\", {pname}.data()[_w]);\n"
                        ));
                        cpp.push_str("    printf(\"\\n\");\n");
                        cpp.push_str(&format!("    _dbg_prev_{pname} = {pname};\n"));
                        cpp.push_str("  }\n");
                    } else {
                        // Scalar port (≤64 bits)
                        cpp.push_str(&format!("  if ({pname} != _dbg_prev_{pname}) {{\n"));
                        cpp.push_str(&format!(
                            "    printf(\"{cyc_fmt}[{name}.{pname}]({dir}) 0x%llx -> 0x%llx\\n\",\n",
                            dir = dir_str
                        ));
                        cpp.push_str(&format!("           {cyc_arg},\n"));
                        cpp.push_str(&format!(
                            "           (unsigned long long)_dbg_prev_{pname},\n"
                        ));
                        cpp.push_str(&format!("           (unsigned long long){pname});\n"));
                        cpp.push_str(&format!("    _dbg_prev_{pname} = {pname};\n"));
                        cpp.push_str("  }\n");
                    }
                }
            }
            // Bus flat signals: log each flattened bus signal with direction
            for (flat_name, flat_ty) in &bus_flat {
                let dir_str = match bus_flat_dirs.get(flat_name) {
                    Some(Direction::In) => "in",
                    Some(Direction::Out) => "out",
                    None => "bus",
                };
                let bits = type_width_of(flat_ty);
                if bits > 64 {
                    let words = wide_words(bits);
                    cpp.push_str(&format!(
                        "  if (memcmp(&{flat_name}, &_dbg_prev_{flat_name}, sizeof({flat_name})) != 0) {{\n"
                    ));
                    cpp.push_str(&format!(
                        "    printf(\"{cyc_fmt}[{name}.{flat_name}]({dir_str}) 0x\",\n           {cyc_arg});\n"
                    ));
                    cpp.push_str(&format!(
                        "    for (int _w = {words} - 1; _w >= 0; _w--) printf(\"%08x\", _dbg_prev_{flat_name}.data()[_w]);\n"
                    ));
                    cpp.push_str("    printf(\" -> 0x\");\n");
                    cpp.push_str(&format!(
                        "    for (int _w = {words} - 1; _w >= 0; _w--) printf(\"%08x\", {flat_name}.data()[_w]);\n"
                    ));
                    cpp.push_str("    printf(\"\\n\");\n");
                    cpp.push_str(&format!("    _dbg_prev_{flat_name} = {flat_name};\n"));
                    cpp.push_str("  }\n");
                } else {
                    cpp.push_str(&format!("  if ({flat_name} != _dbg_prev_{flat_name}) {{\n"));
                    cpp.push_str(&format!(
                        "    printf(\"{cyc_fmt}[{name}.{flat_name}]({dir_str}) 0x%llx -> 0x%llx\\n\",\n"
                    ));
                    cpp.push_str(&format!("           {cyc_arg},\n"));
                    cpp.push_str(&format!(
                        "           (unsigned long long)_dbg_prev_{flat_name},\n"
                    ));
                    cpp.push_str(&format!("           (unsigned long long){flat_name});\n"));
                    cpp.push_str(&format!("    _dbg_prev_{flat_name} = {flat_name};\n"));
                    cpp.push_str("  }\n");
                }
            }

            // Increment cycle counter on any rising clock edge.
            // Multi-clock: also track which clock fired last for the label.
            if clk_ports.is_empty() {
                cpp.push_str("  _dbg_cycle++;\n");
            } else if clk_ports.len() == 1 {
                cpp.push_str(&format!("  if (_rising_{}) _dbg_cycle++;\n", clk_ports[0]));
            } else {
                // Multi-clock: increment on any posedge, record which clock
                cpp.push_str("  ");
                for (i, c) in clk_ports.iter().enumerate() {
                    if i > 0 {
                        cpp.push_str(" else ");
                    }
                    cpp.push_str(&format!(
                        "if (_rising_{c}) {{ _dbg_cycle++; _dbg_last_clk = \"{c}\"; }}"
                    ));
                }
                cpp.push_str("\n");
            }
            cpp.push_str("}\n\n");
        }

        // --coverage: now that all seq emission is done, the registry has
        // its final point count. Patch the header / impl placeholders.
        let n_cov = cov_reg.borrow().points.len();
        let header_decl = if self.coverage && n_cov > 0 {
            format!(
                "public:\n  static uint64_t _arch_cov[{n_cov}];\n  static bool _arch_cov_dumped;\n"
            )
        } else {
            String::new()
        };
        let impl_defn = if self.coverage && n_cov > 0 {
            format!("uint64_t {class}::_arch_cov[{n_cov}] = {{}};\nbool {class}::_arch_cov_dumped = false;\n\n")
        } else {
            String::new()
        };
        h = h.replace("__ARCH_COV_HEADER_DECL__", &header_decl);
        cpp = cpp.replace("__ARCH_COV_IMPL_DEFN__", &impl_defn);

        // --coverage: per-class atexit dumper. Registered via a static
        // initializer so a normal exit (return from main) flushes the
        // counter table to stderr. abort() / fast-exit paths skip atexit
        // handlers — that's documented in doc/plan_arch_coverage.md.
        if self.coverage && n_cov > 0 {
            cpp.push_str("namespace {\n");
            cpp.push_str("static void _arch_cov_dump() {\n");
            cpp.push_str(&format!("  if ({class}::_arch_cov_dumped) return;\n"));
            cpp.push_str(&format!("  {class}::_arch_cov_dumped = true;\n"));
            cpp.push_str(&format!("  uint64_t total = 0; uint64_t hit = 0;\n"));
            cpp.push_str(&format!("  for (uint32_t i = 0; i < {n_cov}; i++) {{ total++; if ({class}::_arch_cov[i]) hit++; }}\n"));
            cpp.push_str(&format!("  fprintf(stderr, \"[{class}] branch coverage: %llu/%llu hit (%.1f%%)\\n\", (unsigned long long)hit, (unsigned long long)total, total ? (100.0 * hit / total) : 0.0);\n"));
            // Per-arm breakdown — file:line if a SourceMap is available,
            // ordinal-only fallback otherwise. (Phase 1b lands the
            // source-text plumbing so the dump shows
            // `tests/cvdp/cache_mshr.arch:111` instead of `branch[0]`.)
            // --coverage-dat: also append per-point Verilator-compatible
            // lines to the coverage.dat file.
            if let Some(path) = &self.coverage_dat {
                let path_lit = path.replace('\\', "\\\\").replace('"', "\\\"");
                cpp.push_str(&format!(
                    "  FILE* _dat = _arch_cov_dat_open(\"{path_lit}\");\n"
                ));
            }
            for (i, p) in cov_reg.borrow().points.iter().enumerate() {
                let (file_disp, line_no) = if let Some(sm) = &self.source_map {
                    sm.locate(p.span_start)
                        .map(|(f, l)| (f.to_string(), l))
                        .unwrap_or_else(|| (String::new(), 0))
                } else {
                    (String::new(), 0)
                };
                let location = if !file_disp.is_empty() {
                    format!("{file_disp}:{line_no}")
                } else {
                    format!("branch[{i}]")
                };
                cpp.push_str(&format!(
                    "  fprintf(stderr, \"  {location} ({}): %llu hits%s\\n\", (unsigned long long){class}::_arch_cov[{i}], {class}::_arch_cov[{i}] ? \"\" : \" *NOT HIT*\");\n",
                    p.kind
                ));
                if self.coverage_dat.is_some() && !file_disp.is_empty() {
                    let file_esc = file_disp.replace('\\', "\\\\").replace('"', "\\\"");
                    let page = match p.kind {
                        "if" | "elsif" | "else" => "v_branch",
                        "expr-then" | "expr-else" => "v_expr",
                        "seq" | "comb" => "v_line",
                        "state" | "trans" => "v_user/fsm",
                        "toggle" => "v_toggle",
                        _ => "v_user",
                    };
                    let comment = p.label.replace('\\', "\\\\").replace('"', "\\\"");
                    // Verilator coverage.dat field separators are \x01 (key)
                    // and \x02 (value). C++ greedy-matches hex escapes, so
                    // each escape is its own string literal — adjacent
                    // string concatenation joins them safely.
                    cpp.push_str(&format!(
                        "  if (_dat) fprintf(_dat, \"C '\" \"\\x01\" \"file\" \"\\x02\" \"{file_esc}\" \"\\x01\" \"line\" \"\\x02\" \"{line_no}\" \"\\x01\" \"page\" \"\\x02\" \"{page}\" \"\\x01\" \"comment\" \"\\x02\" \"{kind} {comment}\" \"' %llu\\n\", (unsigned long long){class}::_arch_cov[{i}]);\n",
                        kind = p.kind
                    ));
                }
            }
            if self.coverage_dat.is_some() {
                cpp.push_str("  if (_dat) fclose(_dat);\n");
            }
            cpp.push_str("}\n");
            cpp.push_str("struct _ArchCovInit { _ArchCovInit() { atexit(_arch_cov_dump); } };\n");
            cpp.push_str("static _ArchCovInit _arch_cov_init;\n");
            cpp.push_str("} // namespace\n\n");
        }

        SimModel {
            class_name: class.clone(),
            header: h,
            impl_: cpp,
        }
    }
}
