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

/// One coverage point recorded during gen_module. Currently only branch
/// coverage (one entry per if/elsif/else arm in seq+comb) — see
/// doc/plan_arch_coverage.md for the phased rollout.
#[derive(Debug, Clone)]
pub(crate) struct CovPoint {
    /// "if", "elsif", or "else"
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

impl<'a> SimCodegen<'a> {
    pub fn new(
        symbols: &'a SymbolTable,
        source: &'a SourceFile,
        overload_map: HashMap<usize, usize>,
    ) -> Self {
        Self {
            symbols,
            source,
            overload_map,
            check_uninit: false,
            inputs_start_uninit: false,
            check_uninit_ram: false,
            cdc_random: false,
            debug: false,
            debug_depth: 1,
            debug_fsm: false,
            coverage: false,
            coverage_dat: None,
            source_map: None,
        }
    }

    pub fn coverage(mut self, enabled: bool) -> Self {
        self.coverage = enabled;
        self
    }

    pub fn coverage_dat(mut self, path: Option<String>) -> Self {
        self.coverage_dat = path;
        if self.coverage_dat.is_some() {
            self.coverage = true;
        }
        self
    }

    pub fn with_source_map(mut self, sm: SourceMap) -> Self {
        self.source_map = Some(sm);
        self
    }

    pub fn check_uninit(mut self, enabled: bool) -> Self {
        self.check_uninit = enabled;
        self
    }

    pub fn inputs_start_uninit(mut self, enabled: bool) -> Self {
        self.inputs_start_uninit = enabled;
        self
    }

    pub fn check_uninit_ram(mut self, enabled: bool) -> Self {
        self.check_uninit_ram = enabled;
        self
    }

    pub fn cdc_random(mut self, enabled: bool) -> Self {
        self.cdc_random = enabled;
        self
    }

    pub fn debug(mut self, enabled: bool, depth: u32) -> Self {
        self.debug = enabled;
        self.debug_depth = depth;
        self
    }

    pub fn with_debug_fsm(mut self, enabled: bool) -> Self {
        self.debug_fsm = enabled;
        self
    }

    /// Generate a SimModel for each synthesizable construct in the source.
    /// Also returns an optional VFunctions model (header-only) for function items.
    pub fn generate(&self) -> Vec<SimModel> {
        let mut models = Vec::new();

        // Functions → VFunctions.h (header-only).
        // Sources: top-level `function` items, package-level functions,
        // and module-internal `function` items. Module-internal functions
        // were previously dropped — calls in the same module's comb body
        // emitted as bare identifiers, which then failed C++ compile with
        // "use of undeclared identifier <fn_name>". Hoisting to VFunctions.h
        // mirrors how top-level free functions are exposed; name collisions
        // across modules are the caller's responsibility (same as today).
        //
        // Dedupe by name: a module-internal `function fn` shared between
        // a parent module and a thread-lowered submodule (the lowering
        // copies the function decl into the new submodule so its body
        // can call it) must only emit once into VFunctions.h, otherwise
        // we get "redefinition of <fn>".
        let mut fn_items: Vec<&FunctionDecl> = Vec::new();
        let mut seen_fn_names: HashSet<String> = HashSet::new();
        for i in &self.source.items {
            let candidates: Vec<&FunctionDecl> = match i {
                Item::Function(f) => vec![f],
                Item::Package(p) => p.functions.iter().collect(),
                Item::Module(m) => m
                    .body
                    .iter()
                    .filter_map(|b| {
                        if let ModuleBodyItem::Function(f) = b {
                            Some(f)
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => vec![],
            };
            for f in candidates {
                if seen_fn_names.insert(f.name.name.clone()) {
                    fn_items.push(f);
                }
            }
        }
        if !fn_items.is_empty() {
            models.push(self.gen_functions(&fn_items));
        }

        // Always emit VStructs.h/cpp (contains enum typedefs + struct definitions)
        models.push(self.gen_structs_file());

        // Compute which modules to instrument when --debug is active.
        // BFS from root module(s) up to debug_depth levels.
        let debug_module_set: std::collections::HashSet<String> = if self.debug {
            // Build inst-children map: module_name → [child_module_names it instantiates]
            let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
            let mut all_module_names: Vec<String> = Vec::new();
            for item in &self.source.items {
                if let Item::Module(m) = item {
                    all_module_names.push(m.name.name.clone());
                    let children: Vec<String> = m
                        .body
                        .iter()
                        .filter_map(|b| {
                            if let ModuleBodyItem::Inst(inst) = b {
                                Some(inst.module_name.name.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    children_map.insert(m.name.name.clone(), children);
                }
            }
            // Root = modules not instantiated by any other module
            let instantiated: std::collections::HashSet<String> = children_map
                .values()
                .flat_map(|v| v.iter().cloned())
                .collect();
            let roots: Vec<String> = all_module_names
                .into_iter()
                .filter(|n| !instantiated.contains(n))
                .collect();
            // BFS up to debug_depth levels
            let mut result: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut queue: std::collections::VecDeque<(String, u32)> =
                roots.into_iter().map(|n| (n, 1u32)).collect();
            while let Some((mod_name, depth)) = queue.pop_front() {
                if depth > self.debug_depth {
                    continue;
                }
                result.insert(mod_name.clone());
                if depth < self.debug_depth {
                    if let Some(children) = children_map.get(&mod_name) {
                        for child in children {
                            queue.push_back((child.clone(), depth + 1));
                        }
                    }
                }
            }
            result
        } else {
            std::collections::HashSet::new()
        };

        for item in &self.source.items {
            // Module is special — it needs the debug-module set passed
            // through so each emitted class can wire its own debug
            // instrumentation. All other sim-emitting constructs go
            // through the uniform `Construct::emit_sim` dispatch, which
            // returns `Some(model)` for the 11 sim-emitting variants
            // and `None` for the rest.
            if let Item::Module(m) = item {
                // Interface stubs from `.archi`: real sim model lives
                // alongside the .archi as a separately-built artifact.
                if m.is_interface {
                    continue;
                }
                models.push(self.gen_module(
                    m,
                    debug_module_set.contains(m.name.name.as_str()),
                    &debug_module_set,
                ));
            } else {
                // Skip interface stubs from `.archi` for any
                // ConstructCommon-bearing variant (Fsm, Fifo, Ram, …).
                // Same reason as Module: real sim model is built
                // separately alongside the `.archi`.
                if item.is_interface() {
                    continue;
                }
                if let Some(model) = item.as_construct().emit_sim(self) {
                    models.push(model);
                }
            }
        }
        models
    }

    /// Generate pybind11 wrapper `.cpp` files for each model.
    /// Each wrapper exposes ports, internal registers, parameters, and eval methods
    /// as a Python module for use with the `arch_cocotb` test adapter.
    pub fn generate_pybind(&self) -> Vec<SimModel> {
        let mut wrappers = Vec::new();
        for item in &self.source.items {
            match item {
                Item::Module(m) => {
                    // Skip interface stubs from `.archi`: the pybind wrapper
                    // for the real implementation is built separately.
                    if m.is_interface {
                        continue;
                    }
                    if let Some(w) = self.emit_pybind_module(m) {
                        wrappers.push(w);
                    }
                }
                Item::Fsm(f) => {
                    // Skip interface stubs from `.archi`: pybind wrapper
                    // for the real implementation is built separately.
                    if f.common.is_interface {
                        continue;
                    }
                    if let Some(w) = self.emit_pybind_fsm(f) {
                        wrappers.push(w);
                    }
                }
                Item::Counter(c) => {
                    if c.common.is_interface {
                        continue;
                    }
                    if let Some(w) = self.emit_pybind_counter(c) {
                        wrappers.push(w);
                    }
                }
                _ => {}
            }
        }
        wrappers
    }

    /// Structs the module actually depends on (port types, internal reg
    /// types, plus the transitive closure of their field types). Only these
    /// get `py::class_<...>` bindings — the module's own `V{Name}.h` won't
    /// declare unrelated package structs.
    fn collect_used_structs(
        m: &ModuleDecl,
        all_structs: &HashMap<String, &StructDecl>,
    ) -> HashSet<String> {
        fn push_named(ty: &TypeExpr, stack: &mut Vec<String>) {
            match ty {
                TypeExpr::Named(id) => stack.push(id.name.clone()),
                TypeExpr::Vec(inner, _) => push_named(inner, stack),
                _ => {}
            }
        }
        let mut stack: Vec<String> = Vec::new();
        for p in &m.ports {
            push_named(&p.ty, &mut stack);
        }
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                push_named(&r.ty, &mut stack);
            }
        }
        let mut used: HashSet<String> = HashSet::new();
        while let Some(name) = stack.pop() {
            if used.insert(name.clone()) {
                if let Some(sd) = all_structs.get(&name) {
                    for f in &sd.fields {
                        push_named(&f.ty, &mut stack);
                    }
                }
            }
        }
        used
    }

    /// Emit pybind11 wrapper for a module.
    fn emit_pybind_module(&self, m: &ModuleDecl) -> Option<SimModel> {
        let name = &m.name.name;
        let class = format!("V{name}");
        let pybind_module = format!("{class}_pybind");

        // Collect port metadata: (field_name, width, is_signed, is_input, is_param, is_internal)
        let mut port_info: Vec<(String, u32, bool, bool, bool, bool)> = Vec::new();
        let mut bindings = Vec::new();

        // Bus port flattening. For Vec<Bus,N> ports, the indexed names
        // `port_0`, `port_1`, ..., `port_{N-1}` populate `bus_port_names`
        // so the bracket-dot expression path (`chans[i].sig`) resolves.
        let mut bus_port_names: HashSet<String> = HashSet::new();
        let mut bus_flat: Vec<(String, TypeExpr)> = Vec::new();
        for p in &m.ports {
            if let Some(ref bi) = p.bus_info {
                match bi.count.as_ref() {
                    None => {
                        bus_port_names.insert(p.name.name.clone());
                    }
                    Some(count_expr) => {
                        let n = eval_const_expr_with_params(count_expr, &m.params) as u32;
                        for i in 0..n {
                            bus_port_names.insert(format!("{}_{}", p.name.name, i));
                        }
                    }
                }
                bus_flat.extend(flatten_bus_port(&p.name.name, bi, self.symbols, &m.params));
            }
        }

        // Vec port info
        let vec_port_infos: Vec<(String, String, u64, bool)> = m
            .ports
            .iter()
            .filter(|p| p.bus_info.is_none())
            .filter_map(|p| {
                if let Some((elem_ty, count_str)) = vec_array_info_with_params(&p.ty, &m.params) {
                    let count: u64 = count_str.parse().unwrap_or(0);
                    Some((
                        p.name.name.clone(),
                        elem_ty,
                        count,
                        p.direction == Direction::In,
                    ))
                } else {
                    None
                }
            })
            .collect();
        let vec_port_names: HashSet<String> = vec_port_infos.iter().map(|v| v.0.clone()).collect();

        // Wide signal names
        let wide_names = collect_wide_names(&m.ports, &m.body, &m.params);

        // Regular scalar ports
        for p in &m.ports {
            if p.bus_info.is_some() {
                continue;
            }
            if vec_port_names.contains(&p.name.name) {
                continue;
            }
            let field = &p.name.name;
            let width = self.port_width(&p.ty);
            let is_signed = matches!(p.ty, TypeExpr::SInt(_));
            let is_input = p.direction == Direction::In;

            if wide_names.contains(field) {
                // VlWide — generate lambda-based get/set
                bindings.push(self.emit_wide_binding(&class, field, width));
            } else {
                bindings.push(format!(
                    "        .def_readwrite(\"{field}\", &{class}::{field})"
                ));
            }
            port_info.push((field.clone(), width, is_signed, is_input, false, false));
        }

        // Vec port flattened fields
        for (base_name, _elem_ty, count, is_input) in &vec_port_infos {
            let width = self.vec_elem_width(&m.ports, base_name);
            for i in 0..*count {
                let field = format!("{base_name}_{i}");
                bindings.push(format!(
                    "        .def_readwrite(\"{field}\", &{class}::{field})"
                ));
                port_info.push((field, width, false, *is_input, false, false));
            }
        }

        // Bus port flattened fields. Use the param-aware width evaluator
        // (issue #427): when a bus's per-signal width depends on a bus param
        // that the call site binds to an enclosing-module param Ident (e.g.
        // `up: target MiniAxi<ID_W=ID_W>` with `param ID_W: const = 3`), the
        // substituted `flat_ty` still contains the module-param Ident;
        // resolving it requires the enclosing module's params. Bare
        // `type_bits_te` would mis-classify a >64b signal as scalar and
        // emit a corrupted `def_readwrite` instead of the wide binding,
        // and the downstream `port_info` width would be wrong too.
        for (flat_name, flat_ty) in &bus_flat {
            let width = type_bits_te_with_params(flat_ty, &m.params);
            let is_signed = matches!(flat_ty, TypeExpr::SInt(_));
            if wide_names.contains(flat_name) {
                bindings.push(self.emit_wide_binding(&class, flat_name, width));
            } else {
                bindings.push(format!(
                    "        .def_readwrite(\"{flat_name}\", &{class}::{flat_name})"
                ));
            }
            port_info.push((flat_name.clone(), width, is_signed, true, false, false));
        }

        // Internal registers (exposed as readonly for testbench inspection)
        let mut internal_reg_helpers = String::new();
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                let rname = &r.name.name;
                // Skip if it's also a port name (port regs already handled)
                if m.ports.iter().any(|p| p.name.name == *rname) {
                    continue;
                }
                let width = self.reg_width(&r.ty);
                let is_signed = matches!(r.ty, TypeExpr::SInt(_));
                let cpp_field = format!("_{rname}");
                let helper_name = format!("read_internal_{rname}");
                if vec_array_info_with_params(&r.ty, &m.params).is_some() {
                    // Vec reg — skip for now (complex)
                    continue;
                }
                // Normal sim stores internal regs as `_reg`, while
                // pre-lowering thread sim stores them as `reg`. This helper
                // covers both scalar and wide internal regs.
                internal_reg_helpers.push_str(&format!(
                    r#"
template <typename T>
auto {helper_name}(const T& self) {{
    if constexpr (requires(const T& t) {{ t.{cpp_field}; }}) {{
        return self.{cpp_field};
    }} else {{
        return self.{rname};
    }}
}}
"#
                ));
                bindings.push(format!(
                    "        .def_property_readonly(\"{rname}\", &arch_pybind_detail::{helper_name}<{class}>)"
                ));
                port_info.push((rname.clone(), width, is_signed, false, false, true));
            }
        }

        // Parameters
        let enum_map = build_enum_map(self.symbols);
        for p in &m.params {
            match &p.kind {
                ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::Logic(_) => {
                    if let Some(ref def) = p.default {
                        let val = eval_const_expr_with_params(def, &m.params);
                        let pname = &p.name.name;
                        bindings.push(format!(
                            "        .def_property_readonly_static(\"{pname}\", [](py::object) {{ return {val}ULL; }})"
                        ));
                        let width = match &p.kind {
                            ParamKind::Logic(ty) => type_bits_te_with_params(ty, &m.params),
                            ParamKind::WidthConst(hi, lo) => {
                                let h = eval_const_expr_with_params(hi, &m.params);
                                let l = eval_const_expr_with_params(lo, &m.params);
                                (h - l + 1) as u32
                            }
                            _ => 32,
                        };
                        let is_signed = matches!(&p.kind, ParamKind::Logic(TypeExpr::SInt(_)));
                        port_info.push((pname.clone(), width, is_signed, false, true, false));
                    }
                }
                ParamKind::EnumConst(enum_name) => {
                    if let Some(ref def) = p.default {
                        if let ExprKind::EnumVariant(_, variant) = &def.kind {
                            if let Some(val) =
                                resolve_enum_variant(&enum_map, enum_name, &variant.name)
                            {
                                let pname = &p.name.name;
                                bindings.push(format!(
                                    "        .def_property_readonly_static(\"{pname}\", [](py::object) {{ return {val}ULL; }})"
                                ));
                                port_info.push((pname.clone(), 32, false, false, true, false));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Methods. Normal sim models expose eval_comb/eval_posedge in
        // addition to eval(); pre-lowering thread-sim models intentionally
        // expose edge-sensitive eval() only. Bind compatibility shims so the
        // same pybind wrapper generator can target both model APIs.
        bindings.push(format!("        .def(\"eval\", &{class}::eval)"));
        bindings.push(format!(
            "        .def(\"eval_comb\", &arch_pybind_detail::eval_comb<{class}>)"
        ));
        bindings.push(format!(
            "        .def(\"eval_posedge\", &arch_pybind_detail::eval_posedge<{class}>)"
        ));
        bindings.push(format!(
            "        .def(\"run_cycles\", &arch_pybind_detail::run_cycles<{class}>)"
        ));

        // _port_info static method
        let port_info_entries: Vec<String> = port_info
            .iter()
            .map(|(n, w, s, inp, par, int)| {
                format!(
                    "            py::make_tuple(\"{n}\", {w}, {}, {}, {}, {})",
                    if *s { "true" } else { "false" },
                    if *inp { "true" } else { "false" },
                    if *par { "true" } else { "false" },
                    if *int { "true" } else { "false" },
                )
            })
            .collect();
        let port_info_str = port_info_entries.join(",\n");

        // Collect all struct types declared in the compilation unit (file-scope
        // and inside packages), then bind ONLY the ones this module actually
        // references through its ports or internal regs (plus any nested
        // structs they transitively contain). Binding every unit-level struct
        // regardless of use produced `undeclared identifier` errors when a
        // shared package was built with a module whose own `V{Name}.h` didn't
        // include those structs — a sibling module's header did instead.
        let mut all_structs: HashMap<String, &StructDecl> = HashMap::new();
        for item in &self.source.items {
            match item {
                Item::Struct(s) => {
                    all_structs.insert(s.name.name.clone(), s);
                }
                Item::Package(p) => {
                    for s in &p.structs {
                        all_structs.insert(s.name.name.clone(), s);
                    }
                }
                _ => {}
            }
        }
        let used_structs = Self::collect_used_structs(m, &all_structs);
        let mut struct_bindings = String::new();
        // Iterate in source order (not HashMap order) for stable output.
        let ordered: Vec<&StructDecl> = self
            .source
            .items
            .iter()
            .flat_map(|item| -> Vec<&StructDecl> {
                match item {
                    Item::Struct(s) => vec![s],
                    Item::Package(p) => p.structs.iter().collect(),
                    _ => vec![],
                }
            })
            .collect();
        for s in ordered {
            let sname = &s.name.name;
            if !used_structs.contains(sname) {
                continue;
            }
            // `py::module_local()` scopes the struct type to this extension
            // module so multiple pybind builds sharing struct names (e.g. two
            // cpuif variants of the same design) can coexist in one process.
            struct_bindings.push_str(&format!(
                "    py::class_<{sname}>(m, \"{sname}\", py::module_local())\n        .def(py::init<>())\n"
            ));
            for f in &s.fields {
                let fname = &f.name.name;
                struct_bindings.push_str(&format!(
                    "        .def_readwrite(\"{fname}\", &{sname}::{fname})\n"
                ));
            }
            struct_bindings.push_str("        ;\n");
        }

        let cpp = format!(
            r#"// Auto-generated pybind11 wrapper for {class}
#include <pybind11/pybind11.h>
#include <pybind11/stl.h>
#include <cstdint>
#include "{class}.h"
namespace py = pybind11;

namespace arch_pybind_detail {{
template <typename T>
void eval_comb(T& self) {{
    if constexpr (requires(T& t) {{ t.eval_comb(); }}) {{
        self.eval_comb();
    }} else {{
        self.eval();
    }}
}}

template <typename T>
void eval_posedge(T& self) {{
    if constexpr (requires(T& t) {{ t.eval_posedge(); }}) {{
        self.eval_posedge();
    }} else {{
        self.eval();
    }}
}}

template <typename T>
void run_cycles(T& self, uint64_t cycles) {{
    if constexpr (requires(T& t, uint64_t n) {{ t.run_cycles(n); }}) {{
        self.run_cycles(cycles);
    }} else {{
        for (uint64_t i = 0; i < cycles; ++i) self.eval();
    }}
}}
{internal_reg_helpers}
}} // namespace arch_pybind_detail

PYBIND11_MODULE({pybind_module}, m) {{
{struct_bindings}    py::class_<{class}>(m, "{class}")
        .def(py::init<>())
{bindings}
        .def_static("_port_info", []() {{
            return std::vector<py::tuple>{{
{port_info_str}
            }};
        }});
}}
"#,
            bindings = bindings.join("\n"),
        );

        Some(SimModel {
            class_name: pybind_module,
            header: String::new(),
            impl_: cpp,
        })
    }

    /// Emit pybind11 wrapper for an FSM construct.
    fn emit_pybind_fsm(&self, _f: &crate::ast::FsmDecl) -> Option<SimModel> {
        // FSM constructs generate a VFsmName class with similar port structure.
        // For now, FSM pybind11 support is deferred — most CVDP tests use modules.
        None
    }

    /// Emit pybind11 wrapper for a counter construct.
    fn emit_pybind_counter(&self, _c: &crate::ast::CounterDecl) -> Option<SimModel> {
        None
    }

    /// Get the width in bits of a port type.
    fn port_width(&self, ty: &TypeExpr) -> u32 {
        match ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
            TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => 1,
            TypeExpr::FP32 => 32,
            TypeExpr::BF16 => 16,
            TypeExpr::Named(_) => 32,
            TypeExpr::Vec(_, _) => 32,
        }
    }

    /// Get the width in bits of a register type.
    fn reg_width(&self, ty: &TypeExpr) -> u32 {
        self.port_width(ty)
    }

    /// Get the element width of a Vec port.
    fn vec_elem_width(&self, ports: &[PortDecl], name: &str) -> u32 {
        for p in ports {
            if p.name.name == name {
                if let TypeExpr::Vec(elem, _) = &p.ty {
                    return self.port_width(elem);
                }
            }
        }
        32
    }

    /// Emit a lambda-based pybind11 binding for a VlWide field.
    fn emit_wide_binding(&self, class: &str, field: &str, width: u32) -> String {
        let words = (width + 31) / 32;
        format!(
            r#"        .def_property("{field}",
            []({class}& self) -> uint64_t {{
                uint64_t v = 0;
                for (int i = std::min({words}u, 2u) - 1; i >= 0; i--)
                    v = (v << 32) | self.{field}.data()[i];
                return v;
            }},
            []({class}& self, uint64_t v) {{
                self.{field} = VlWide<{words}>(v);
            }})"#,
        )
    }

    /// Return the contents of the `verilated.h` stub.
    pub fn verilated_h(fp_compat: crate::FpCompat) -> String {
        let prelude = r#"#pragma once
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>

// --coverage-dat: forward declaration for the helper defined in
// verilated.cpp. Each class's atexit dumper calls this to get a
// FILE* opened for append (with the header line written once on
// first call).
extern "C" FILE* _arch_cov_dat_open(const char* path);

/// Minimal Verilated compatibility shim for arch-generated C++ simulation models.
class Verilated {
public:
    static void commandArgs(int argc, char** argv) {
        for (int i = 1; i < argc; i++) {
            int v;
            if (sscanf(argv[i], "+arch_verbosity=%d", &v) == 1) {
                _s_verbosity = v;
            }
            if (strncmp(argv[i], "+trace+", 7) == 0 && argv[i][7]) {
                _s_trace_file = argv[i] + 7;
            }
        }
    }
    static int verbosity() { return _s_verbosity; }
    static const char* traceFile() { return _s_trace_file; }
    /// Returns true if this is the first caller (top-level module).
    static bool claimTrace() { if (_s_trace_claimed) return false; _s_trace_claimed = true; return true; }
    static int _s_verbosity;
    static const char* _s_trace_file;
    static bool _s_trace_claimed;
};

/// Stub VerilatedContext for Verilator testbench portability.
/// Arch-sim testbenches can use `new VerilatedContext` without changes.
class VerilatedContext {
public:
    void commandArgs(int argc, char** argv) { Verilated::commandArgs(argc, argv); }
    void traceEverOn(bool) {}
};

// ── Wide signal support ───────────────────────────────────────────────────────

/// Wide word type for signals wider than 64 bits (matches Verilator VlWide).
/// Word layout: _data[0] = bits 31:0 (LSB), _data[N-1] = MSB words.
/// Supports signals up to 2048 bits (WORDS=64).
template<int WORDS>
struct VlWide {
    uint32_t _data[WORDS];
    VlWide()                    { memset(_data, 0, sizeof(_data)); }
    VlWide(const VlWide& o)     { memcpy(_data, o._data, sizeof(_data)); }
    /// Construct from a 64-bit integer (zero-extends into MSB words).
    explicit VlWide(uint64_t v) { memset(_data, 0, sizeof(_data));
        _data[0] = (uint32_t)v; if (WORDS > 1) _data[1] = (uint32_t)(v >> 32); }
    VlWide& operator=(const VlWide& o) { memcpy(_data, o._data, sizeof(_data)); return *this; }
    VlWide& operator=(uint64_t v)      { memset(_data, 0, sizeof(_data));
        _data[0] = (uint32_t)v; if (WORDS > 1) _data[1] = (uint32_t)(v >> 32); return *this; }
    uint32_t*       data()       { return _data; }
    const uint32_t* data() const { return _data; }

    // ── Bitwise operators ────────────────────────────────────────────────────
    VlWide operator|(const VlWide& b) const {
        VlWide r; for (int i=0;i<WORDS;i++) r._data[i]=_data[i]|b._data[i]; return r; }
    VlWide operator&(const VlWide& b) const {
        VlWide r; for (int i=0;i<WORDS;i++) r._data[i]=_data[i]&b._data[i]; return r; }
    VlWide operator^(const VlWide& b) const {
        VlWide r; for (int i=0;i<WORDS;i++) r._data[i]=_data[i]^b._data[i]; return r; }
    VlWide operator~() const {
        VlWide r; for (int i=0;i<WORDS;i++) r._data[i]=~_data[i]; return r; }

    // ── Arithmetic ───────────────────────────────────────────────────────────
    VlWide operator+(const VlWide& b) const {
        VlWide r; uint64_t c=0;
        for (int i=0;i<WORDS;i++) { uint64_t s=(uint64_t)_data[i]+b._data[i]+c; r._data[i]=(uint32_t)s; c=s>>32; }
        return r; }
    VlWide operator-(const VlWide& b) const {
        VlWide r; int64_t c=0;
        for (int i=0;i<WORDS;i++) { int64_t s=(int64_t)(uint64_t)_data[i]-(int64_t)(uint64_t)b._data[i]+c; r._data[i]=(uint32_t)(uint64_t)s; c=(s<0)?-1:0; }
        return r; }

    // ── Shifts ───────────────────────────────────────────────────────────────
    VlWide operator<<(int n) const {
        VlWide r{};
        if (n<=0) return *this; if (n>=WORDS*32) return r;
        const int ws=n/32, bs=n%32;
        for (int di=0;di<WORDS;di++) {
            const int sh=di-ws, sl=sh-1;
            if (sh>=0&&sh<WORDS) r._data[di]|=_data[sh]<<bs;
            if (bs>0&&sl>=0&&sl<WORDS) r._data[di]|=_data[sl]>>(32-bs);
        }
        return r; }
    VlWide operator>>(int n) const {
        VlWide r{};
        if (n<=0) return *this; if (n>=WORDS*32) return r;
        const int ws=n/32, bs=n%32;
        for (int di=0;di<WORDS;di++) {
            const int sl=di+ws, sh=sl+1;
            if (sl>=0&&sl<WORDS) r._data[di]|=_data[sl]>>bs;
            if (bs>0&&sh>=0&&sh<WORDS) r._data[di]|=_data[sh]<<(32-bs);
        }
        return r; }

    // ── Comparisons ──────────────────────────────────────────────────────────
    bool operator==(const VlWide& b) const {
        for (int i=0;i<WORDS;i++) if (_data[i]!=b._data[i]) return false; return true; }
    bool operator!=(const VlWide& b) const { return !(*this==b); }
    explicit operator bool() const {
        for (int i=0;i<WORDS;i++) if (_data[i]) return true; return false; }
};

/// 128-bit internal arithmetic type (used for 65–128 bit signals).
typedef unsigned __int128 _arch_u128;

/// Convert a VlWide<N> backing array → 128-bit integer. `words` is the actual
/// element count of the array (= ceil(W/32)); only those words are read, so a
/// `VlWide<3>` (66–96-bit) payload is NOT read out of bounds. Missing high words
/// contribute 0.
static inline _arch_u128 _arch_vl_to_u128(const uint32_t* w, int words) {
    _arch_u128 r = 0;
    for (int i = 0; i < words && i < 4; i++) r |= ((_arch_u128)w[i]) << (32 * i);
    return r;
}

/// Convert 128-bit integer → a VlWide<N> backing array. `words` is the actual
/// element count; only those words are written, so writing into a `VlWide<3>`
/// payload does NOT clobber the adjacent struct member past `_data[2]`.
static inline void _arch_u128_to_vl(const _arch_u128 v, uint32_t* w, int words) {
    for (int i = 0; i < words && i < 4; i++) w[i] = (uint32_t)(v >> (32 * i));
}

/// Extract up to 64 bits [hi:lo] from a VlWide _data array.
static inline uint64_t _arch_vw_bits(const uint32_t* data, uint32_t hi, uint32_t lo) {
    uint32_t width = hi - lo + 1; if (width > 64) width = 64;
    uint32_t w0 = lo >> 5, b0 = lo & 31;
    uint64_t v = (uint64_t)data[w0];
    v |= (uint64_t)data[w0+1] << 32;
    v >>= b0;
    if (b0 > 0 && width > (64 - b0)) v |= (uint64_t)data[w0+2] << (64 - b0);
    uint64_t mask = (width >= 64) ? ~0ULL : ((1ULL << width) - 1ULL);
    return v & mask;
}

/// Ceiling log2 helper.
static inline uint32_t _arch_clog2(uint64_t v) {
    if (v <= 1) return 1;
    uint32_t r = 0; v--; while (v) { v >>= 1; r++; } return r;
}

/// Bit replication helper: {N{val}} where val is val_width bits wide.
static inline uint64_t _arch_repeat(uint64_t val, uint32_t n, uint32_t val_width) {
    uint64_t mask = (val_width >= 64) ? ~0ULL : ((1ULL << val_width) - 1);
    uint64_t result = 0;
    for (uint32_t i = 0; i < n; i++) {
        result = (result << val_width) | (val & mask);
    }
    return result;
}

/// Runtime bounds check — hard abort on out-of-range index.
/// Used for Vec<T,N> indexing, single-bit selects on UInt<W>/SInt<W>,
/// and variable part-selects [+:]/[-:].
[[noreturn]] static inline void _arch_bounds_abort(unsigned long long idx,
                                                   unsigned long long limit,
                                                   const char* loc) {
    fprintf(stderr, "ARCH-ERROR: %s: index %llu out of bounds [0..%llu)\n",
            loc, idx, limit);
    abort();
}
#define _ARCH_BCHK(idx, limit, loc) \
    ((unsigned long long)(idx) < (unsigned long long)(limit) \
        ? (void)0 : _arch_bounds_abort((unsigned long long)(idx), (unsigned long long)(limit), (loc)))

/// Runtime divide-by-zero check — hard abort when a `/` or `%` runtime
/// divisor is zero. Constant divisors are verified at compile time, so
/// this only wraps truly-runtime operands.
[[noreturn]] static inline void _arch_div0_abort(const char* loc) {
    fprintf(stderr, "ARCH-ERROR: %s: division by zero\n", loc);
    abort();
}
#define _ARCH_DCHK(divisor, loc) \
    ((unsigned long long)(divisor) != 0 \
        ? (void)0 : _arch_div0_abort((loc)))

// ── Floating-point (FP32 / BF16) runtime ─────────────────────────────────────
// Floats are carried as raw bit patterns (FP32→uint32_t, BF16→uint16_t).
// Arithmetic uses the host FPU, which is IEEE-754 round-to-nearest-even and
// therefore bit-identical to Berkeley SoftFloat for + - * and fma. BF16 ops go
// through an f32 intermediate then round once to bf16 — innocuous double
// rounding (24 >= 2*8+2), so the result equals direct correctly-rounded bf16
// (doc/plan_fp_types.md §5.3). NaN results are canonicalized to the RISC-V
// default pattern (0x7FC00000 / 0x7FC0); float→int is toward-zero, saturating,
// NaN→type-max (RISC-V profile, §6).
static inline float    _arch_f32b(uint32_t b){ float f; memcpy(&f,&b,4); return f; }
static inline uint32_t _arch_b32f(float f){ uint32_t b; memcpy(&b,&f,4); return b; }
static inline uint32_t _arch_f32_canon(uint32_t b){
    if (((b>>23)&0xFFu)==0xFFu && (b&0x7FFFFFu)!=0u) return 0x7FC00000u;
    return b;
}
static inline uint32_t _arch_f32_add(uint32_t a,uint32_t b){ return _arch_f32_canon(_arch_b32f(_arch_f32b(a)+_arch_f32b(b))); }
static inline uint32_t _arch_f32_sub(uint32_t a,uint32_t b){ return _arch_f32_canon(_arch_b32f(_arch_f32b(a)-_arch_f32b(b))); }
static inline uint32_t _arch_f32_mul(uint32_t a,uint32_t b){ return _arch_f32_canon(_arch_b32f(_arch_f32b(a)*_arch_f32b(b))); }
static inline uint32_t _arch_fma_f32(uint32_t a,uint32_t b,uint32_t c){ return _arch_f32_canon(_arch_b32f(fmaf(_arch_f32b(a),_arch_f32b(b),_arch_f32b(c)))); }
static inline uint8_t _arch_f32_eq(uint32_t a,uint32_t b){ return _arch_f32b(a)==_arch_f32b(b); }
static inline uint8_t _arch_f32_ne(uint32_t a,uint32_t b){ return _arch_f32b(a)!=_arch_f32b(b); }
static inline uint8_t _arch_f32_lt(uint32_t a,uint32_t b){ return _arch_f32b(a)< _arch_f32b(b); }
static inline uint8_t _arch_f32_gt(uint32_t a,uint32_t b){ return _arch_f32b(a)> _arch_f32b(b); }
static inline uint8_t _arch_f32_le(uint32_t a,uint32_t b){ return _arch_f32b(a)<=_arch_f32b(b); }
static inline uint8_t _arch_f32_ge(uint32_t a,uint32_t b){ return _arch_f32b(a)>=_arch_f32b(b); }
static inline uint8_t _arch_f32_isnan(uint32_t a){ return std::isnan(_arch_f32b(a))?1:0; }

// BF16 <-> f32: bf16 is the top 16 bits of binary32.
static inline float    _arch_bf16f(uint16_t h){ return _arch_f32b(((uint32_t)h)<<16); }
static inline uint16_t _arch_f2bf16(float f){
    uint32_t x=_arch_b32f(f);
    if (((x>>23)&0xFFu)==0xFFu && (x&0x7FFFFFu)!=0u) return 0x7FC0u; // canonical NaN
    uint32_t lsb=(x>>16)&1u; x += 0x7FFFu+lsb; // round-to-nearest-even
    return (uint16_t)(x>>16);
}
static inline uint32_t _arch_bf16_to_f32(uint16_t h){ return _arch_f32_canon(((uint32_t)h)<<16); }
static inline uint16_t _arch_f32_to_bf16(uint32_t b){ return _arch_f2bf16(_arch_f32b(b)); }
static inline uint16_t _arch_bf16_add(uint16_t a,uint16_t b){ return _arch_f2bf16(_arch_bf16f(a)+_arch_bf16f(b)); }
static inline uint16_t _arch_bf16_sub(uint16_t a,uint16_t b){ return _arch_f2bf16(_arch_bf16f(a)-_arch_bf16f(b)); }
static inline uint16_t _arch_bf16_mul(uint16_t a,uint16_t b){ return _arch_f2bf16(_arch_bf16f(a)*_arch_bf16f(b)); }
static inline uint16_t _arch_fma_bf16(uint16_t a,uint16_t b,uint16_t c){ return _arch_f2bf16(fmaf(_arch_bf16f(a),_arch_bf16f(b),_arch_bf16f(c))); }
static inline uint8_t _arch_bf16_eq(uint16_t a,uint16_t b){ return _arch_bf16f(a)==_arch_bf16f(b); }
static inline uint8_t _arch_bf16_ne(uint16_t a,uint16_t b){ return _arch_bf16f(a)!=_arch_bf16f(b); }
static inline uint8_t _arch_bf16_lt(uint16_t a,uint16_t b){ return _arch_bf16f(a)< _arch_bf16f(b); }
static inline uint8_t _arch_bf16_gt(uint16_t a,uint16_t b){ return _arch_bf16f(a)> _arch_bf16f(b); }
static inline uint8_t _arch_bf16_le(uint16_t a,uint16_t b){ return _arch_bf16f(a)<=_arch_bf16f(b); }
static inline uint8_t _arch_bf16_ge(uint16_t a,uint16_t b){ return _arch_bf16f(a)>=_arch_bf16f(b); }
static inline uint8_t _arch_bf16_isnan(uint16_t a){ return std::isnan(_arch_bf16f(a))?1:0; }

// int <-> float conversions.
static inline uint32_t _arch_i_to_f32(int64_t v){ return _arch_b32f((float)v); }
static inline uint32_t _arch_u_to_f32(uint64_t v){ return _arch_b32f((float)v); }
static inline uint16_t _arch_i_to_bf16(int64_t v){ return _arch_f2bf16((float)v); }
static inline uint16_t _arch_u_to_bf16(uint64_t v){ return _arch_f2bf16((float)v); }
static inline int64_t  _arch_f32_to_i(uint32_t b){
    float f=_arch_f32b(b);
    if (std::isnan(f)) return INT64_MAX;
    if (f >= 9223372036854775808.0f) return INT64_MAX;
    if (f <  -9223372036854775808.0f) return INT64_MIN;
    return (int64_t)f; // truncates toward zero
}
static inline uint64_t _arch_f32_to_u(uint32_t b){
    float f=_arch_f32b(b);
    if (std::isnan(f)) return UINT64_MAX;
    if (f <= 0.0f) return 0;
    if (f >= 18446744073709551616.0f) return UINT64_MAX;
    return (uint64_t)f;
}
static inline int64_t  _arch_bf16_to_i(uint16_t h){ return _arch_f32_to_i(_arch_bf16_to_f32(h)); }
static inline uint64_t _arch_bf16_to_u(uint16_t h){ return _arch_f32_to_u(_arch_bf16_to_f32(h)); }
// Width-aware float→int: toward-zero, saturating to the N-bit target range,
// NaN→type-max (RISC-V profile). Builds on the 64-bit-safe conversions above
// (which already map NaN→max and saturate to the 64-bit range) then clamps to
// the requested width — so the int64 cast never sees an out-of-range float.
static inline int64_t _arch_f32_to_sint(uint32_t b, int bits){
    int64_t v = _arch_f32_to_i(b);
    if (bits >= 64) return v;
    int64_t maxv = ((int64_t)1 << (bits - 1)) - 1;
    int64_t minv = -((int64_t)1 << (bits - 1));
    if (v > maxv) return maxv;
    if (v < minv) return minv;
    return v;
}
static inline uint64_t _arch_f32_to_uint(uint32_t b, int bits){
    uint64_t v = _arch_f32_to_u(b);
    if (bits >= 64) return v;
    uint64_t maxv = ((uint64_t)1 << bits) - 1;
    return (v > maxv) ? maxv : v;
}
"#.to_string();
        // Profile shim (doc/plan_fp_types.md §6.2): the `cuda` profile differs
        // from the default `riscv` only in the canonical NaN pattern and the
        // NaN→int result; the arithmetic core is untouched.
        match fp_compat {
            crate::FpCompat::Riscv => prelude,
            crate::FpCompat::Cuda => prelude
                .replace("return 0x7FC00000u;", "return 0x7FFFFFFFu;")
                .replace("return 0x7FC0u;", "return 0x7FFFu;")
                .replace(
                    "if (std::isnan(f)) return INT64_MAX;",
                    "if (std::isnan(f)) return 0;",
                )
                .replace(
                    "if (std::isnan(f)) return UINT64_MAX;",
                    "if (std::isnan(f)) return 0;",
                ),
        }
    }

    pub fn verilated_cpp() -> String {
        r##"#include "verilated.h"
#include <cstdio>
#include <cstdlib>
int Verilated::_s_verbosity = 1;
const char* Verilated::_s_trace_file = nullptr;
bool Verilated::_s_trace_claimed = false;

// --coverage-dat: Verilator-compatible coverage.dat writer. Each
// class's atexit dumper calls _arch_cov_dat_open() to get a FILE*
// opened for append; the first call writes the header line so
// verilator_coverage --annotate parses cleanly. Subsequent calls
// just append their point lines.
extern "C" FILE* _arch_cov_dat_open(const char* path) {
    static bool _header_written = false;
    FILE* f = fopen(path, _header_written ? "a" : "w");
    if (!f) return nullptr;
    if (!_header_written) {
        fprintf(f, "# SystemC::Coverage-3\n");
        _header_written = true;
    }
    return f;
}
"##
        .to_string()
    }
}

// ── VCD Trace helpers ────────────────────────────────────────────────────────

/// A signal to be traced in VCD output.
pub(crate) struct TraceSignal {
    pub(crate) vcd_name: String, // display name in VCD scope
    pub(crate) cpp_expr: String, // C++ expression to read the value
    pub(crate) width: u32,       // bit width
    pub(crate) is_wide: bool,    // true if VlWide<N> type
}

/// Generate a short VCD identifier from a signal index.
/// Uses alphanumeric chars only (a-z, A-Z, 0-9) to avoid C string/printf conflicts.
fn vcd_id(index: usize) -> String {
    // Prefix with 's' to ensure valid VCD id, then index
    format!("s{index}")
}

/// Emit trace_open / trace_dump / trace_close C++ method implementations.
/// Returns (header_declarations, cpp_implementations).
pub(crate) fn emit_trace_methods(
    class: &str,
    module_name: &str,
    signals: &[TraceSignal],
) -> (String, String) {
    let mut h = String::new();
    let mut cpp = String::new();

    h.push_str("  void trace_open(const char* filename);\n");
    h.push_str("  void trace_dump(uint64_t time);\n");
    h.push_str("  void trace_close();\n");

    // ── trace_open ──
    cpp.push_str(&format!(
        "void {class}::trace_open(const char* filename) {{\n"
    ));
    cpp.push_str("  _trace_fp = fopen(filename, \"w\");\n");
    cpp.push_str("  if (!_trace_fp) return;\n");
    cpp.push_str("  fprintf(_trace_fp, \"$timescale 1ns $end\\n\");\n");
    cpp.push_str(&format!(
        "  fprintf(_trace_fp, \"$scope module {} $end\\n\");\n",
        module_name
    ));
    for (i, sig) in signals.iter().enumerate() {
        let id = vcd_id(i);
        let kind = if sig.vcd_name.starts_with('_') {
            "reg"
        } else {
            "wire"
        };
        cpp.push_str(&format!(
            "  fprintf(_trace_fp, \"$var {} {} {} {} $end\\n\");\n",
            kind, sig.width, id, sig.vcd_name
        ));
    }
    cpp.push_str("  fprintf(_trace_fp, \"$upscope $end\\n$enddefinitions $end\\n\");\n");
    cpp.push_str("}\n\n");

    // ── trace_dump ──
    cpp.push_str(&format!("void {class}::trace_dump(uint64_t time) {{\n"));
    cpp.push_str("  if (!_trace_fp) return;\n");
    cpp.push_str("  fprintf(_trace_fp, \"#%lu\\n\", (unsigned long)time);\n");
    for (i, sig) in signals.iter().enumerate() {
        let id = vcd_id(i);
        if sig.width == 1 {
            cpp.push_str(&format!(
                "  fprintf(_trace_fp, \"%c{}\\n\", {} ? '1' : '0');\n",
                id, sig.cpp_expr
            ));
        } else if sig.is_wide {
            // Wide signal (VlWide port): emit bit-by-bit via .data()
            cpp.push_str("  fprintf(_trace_fp, \"b\");\n");
            cpp.push_str(&format!(
                "  for (int _i = {w} - 1; _i >= 0; _i--) fprintf(_trace_fp, \"%c\", ({expr}.data()[_i/32] >> (_i%32)) & 1 ? '1' : '0');\n",
                w = sig.width, expr = sig.cpp_expr
            ));
            cpp.push_str(&format!("  fprintf(_trace_fp, \" {}\\n\");\n", id));
        } else if sig.width > 64 {
            // Wide signal (_arch_u128 reg/let): emit bit-by-bit via shift
            cpp.push_str("  fprintf(_trace_fp, \"b\");\n");
            cpp.push_str(&format!(
                "  for (int _i = {w} - 1; _i >= 0; _i--) fprintf(_trace_fp, \"%c\", (int)(({expr} >> _i) & 1) ? '1' : '0');\n",
                w = sig.width, expr = sig.cpp_expr
            ));
            cpp.push_str(&format!("  fprintf(_trace_fp, \" {}\\n\");\n", id));
        } else {
            // Multi-bit (<=64): emit binary
            cpp.push_str("  fprintf(_trace_fp, \"b\");\n");
            cpp.push_str(&format!(
                "  for (int _i = {w} - 1; _i >= 0; _i--) fprintf(_trace_fp, \"%c\", (int)(({expr} >> _i) & 1) ? '1' : '0');\n",
                w = sig.width, expr = sig.cpp_expr
            ));
            cpp.push_str(&format!("  fprintf(_trace_fp, \" {}\\n\");\n", id));
        }
    }
    cpp.push_str("}\n\n");

    // ── trace_close ──
    cpp.push_str(&format!("void {class}::trace_close() {{\n"));
    cpp.push_str("  if (_trace_fp) { fclose(_trace_fp); _trace_fp = nullptr; }\n");
    cpp.push_str("}\n\n");

    (h, cpp)
}

/// Collect trace signals from a module's ports and body.
fn collect_trace_signals(
    ports: &[PortDecl],
    body: &[ModuleBodyItem],
    wide_names: &HashSet<String>,
    widths: &HashMap<String, u32>,
    bus_flat: &[(String, TypeExpr)],
    params: &[ParamDecl],
) -> Vec<TraceSignal> {
    let mut sigs = Vec::new();

    // Ports (skip bus ports and Vec ports — flattened signals added separately;
    // also skip struct/enum-typed ports, which can't be bit-shifted scalar-style)
    for p in ports {
        if p.bus_info.is_some() {
            continue;
        }
        if matches!(p.ty, TypeExpr::Vec(..) | TypeExpr::Named(_)) {
            continue;
        }
        let name = &p.name.name;
        let width = type_width_with_params(&p.ty, params);
        let is_wide = wide_names.contains(name.as_str());
        sigs.push(TraceSignal {
            vcd_name: name.clone(),
            cpp_expr: name.clone(),
            width,
            is_wide,
        });
    }
    // Flattened bus signals
    for (flat_name, flat_ty) in bus_flat {
        if matches!(flat_ty, TypeExpr::Vec(..) | TypeExpr::Named(_)) {
            continue;
        }
        let width = type_width_with_params(flat_ty, params);
        let is_wide = wide_names.contains(flat_name.as_str());
        sigs.push(TraceSignal {
            vcd_name: flat_name.clone(),
            cpp_expr: flat_name.clone(),
            width,
            is_wide,
        });
    }

    // Registers. Skip struct/named types (can't bit-shift). Scalars
    // emit one signal; Vec<T,N> regs emit one signal per element so
    // each is independently visible in the waveform viewer.
    // Regs >64 bits use _arch_u128, not VlWide, so is_wide = false.
    for item in body {
        if let ModuleBodyItem::RegDecl(r) = item {
            if matches!(r.ty, TypeExpr::Named(_)) {
                continue;
            }
            let name = &r.name.name;
            if let TypeExpr::Vec(elem, count_expr) = &r.ty {
                // Skip Vec-of-named (struct/enum element); per-element
                // bit-shift only works for scalar elements.
                if matches!(elem.as_ref(), TypeExpr::Named(_)) {
                    continue;
                }
                let elem_width = type_width_with_params(elem, params);
                if elem_width == 0 || elem_width > 64 {
                    continue;
                }
                // Use params-aware count (matches the field-decl path
                // at line 4091); bare eval_const_expr returns 0 for
                // param-based sizes, which would skip the trace silently.
                let count = eval_const_expr_with_params(count_expr, params);
                if count == 0 {
                    continue;
                }
                for i in 0..count {
                    sigs.push(TraceSignal {
                        vcd_name: format!("{name}[{i}]"),
                        cpp_expr: format!("_{name}[{i}]"),
                        width: elem_width,
                        is_wide: false,
                    });
                }
            } else {
                let width = type_width_with_params(&r.ty, params);
                sigs.push(TraceSignal {
                    vcd_name: name.clone(),
                    cpp_expr: format!("_{name}"),
                    width,
                    is_wide: false,
                });
            }
        }
    }

    // Let bindings and wire decls — skip Vec (C arrays) and struct/enum-typed
    // (Named), which can't be bit-shifted scalar-style. Matches the filter
    // already applied to ports and regs above.
    for item in body {
        match item {
            ModuleBodyItem::LetBinding(l) => {
                // ty=None means assignment to existing port/wire — already traced, skip
                if l.ty.is_none() {
                    continue;
                }
                let name = &l.name.name;
                if l.ty.as_ref().map_or(false, |t| {
                    matches!(t, TypeExpr::Vec(..) | TypeExpr::Named(_))
                }) {
                    continue;
                }
                let width =
                    l.ty.as_ref()
                        .map(|t| type_width_with_params(t, params))
                        .unwrap_or(widths.get(name.as_str()).copied().unwrap_or(32));
                sigs.push(TraceSignal {
                    vcd_name: name.clone(),
                    cpp_expr: format!("_let_{name}"),
                    width,
                    is_wide: false,
                });
            }
            ModuleBodyItem::WireDecl(w) => {
                if matches!(w.ty, TypeExpr::Vec(..) | TypeExpr::Named(_)) {
                    continue;
                }
                let name = &w.name.name;
                let width = type_width_with_params(&w.ty, params);
                sigs.push(TraceSignal {
                    vcd_name: name.clone(),
                    cpp_expr: format!("_let_{name}"),
                    width,
                    is_wide: false,
                });
            }
            _ => {}
        }
    }

    // Pipe regs
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            let width = widths.get(&p.source.name).copied().unwrap_or(32);
            for i in 0..p.stages {
                let stage_name = if i == p.stages - 1 {
                    p.name.name.clone()
                } else {
                    format!("{}_stg{}", p.name.name, i + 1)
                };
                sigs.push(TraceSignal {
                    vcd_name: stage_name.clone(),
                    cpp_expr: format!("_{stage_name}"),
                    width,
                    is_wide: false,
                });
            }
        }
    }

    sigs
}

/// Get the bit-width of a TypeExpr — packed total (Vec recurses and multiplies).
/// Use this when you need the *total* number of bits to fit in storage
/// (e.g. for VCD trace width, packed signal width).
/// Returns 32 for unhandled types (Named structs).
///
/// Distinguishing the three width helpers in this file:
/// - `type_width(ty)`: packed total, recurses into Vec, defaults to 32
/// - `type_width_of(ty)`: same but returns 0 (not 32) for Vec/Named — used by `--debug`
///   shadow generation where 0 signals "skip this port"
/// - `type_bits_te(ty)`: scalar-only width (does NOT recurse into Vec), defaults to 32 —
///   used for inst port width tracking where Vec is handled separately via flat fields
#[deprecated(
    note = "use `type_width_with_params(.., &params)` — the bare form silently \
            miscompiles when the type depends on enclosing-construct params \
            (UInt<PARAM>, Vec<_, PARAM>). See arch-com#447 §1 and PR #463 \
            extending #458 to the sibling helper cluster."
)]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
fn type_width(ty: &TypeExpr) -> u32 {
    type_width_with_params(ty, &[])
}

/// Param-aware variant of [`type_width`]. Resolves `UInt<PARAM>` /
/// `SInt<PARAM>` widths via param defaults. Used by trace-signal emission
/// (`build_trace_signals`) so VCD `$var wire N` widths reflect the actual
/// HDL bit width rather than the legacy 32-default. arch-com#330.
fn type_width_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width_with_params(w, params),
        TypeExpr::Bool => 1,
        TypeExpr::Bit => 1,
        TypeExpr::Clock(_) => 1,
        TypeExpr::Reset { .. } => 1,
        TypeExpr::Vec(elem, count) => {
            type_width_with_params(elem, params) * eval_width_with_params(count, params)
        }
        _ => 32,
    }
}

/// Add VCD trace support to a non-module construct (counter, fsm, ram, etc.).
/// Patches the header and cpp strings in place. Call BEFORE closing `};\n` in header
/// and AFTER all method impls in cpp.
///
/// `extra_signals`: additional internal signals to trace (name, cpp_expr, width).
fn add_trace_to_simple_construct(
    h: &mut String,
    cpp: &mut String,
    class: &str,
    construct_name: &str,
    ports: &[PortDecl],
    extra_signals: &[(&str, &str, u32)],
    params: &[ParamDecl],
) {
    // Build signal list from ports + extras.
    // Vec and bus ports are skipped (their flat fields are passed via extra_signals by caller).
    // `params` is the enclosing construct's param list, used to resolve
    // `UInt<PARAM>` / `SInt<PARAM>` widths in port VCD declarations to
    // their real bit width rather than the legacy 32-default. See
    // arch-com#447 §1 / PR following #458 for the migration that closed
    // this footgun.
    let mut signals = Vec::new();
    for p in ports {
        if matches!(p.ty, TypeExpr::Vec(..)) {
            continue;
        } // handled as flat via extra_signals
        if p.bus_info.is_some() {
            continue;
        } // bus ports flattened via extra_signals
        let width = type_width_with_params(&p.ty, params);
        signals.push(TraceSignal {
            vcd_name: p.name.name.clone(),
            cpp_expr: p.name.name.clone(),
            width,
            is_wide: false,
        });
    }
    for &(name, expr, width) in extra_signals {
        signals.push(TraceSignal {
            vcd_name: name.to_string(),
            cpp_expr: expr.to_string(),
            width,
            is_wide: false,
        });
    }

    let (trace_h, trace_cpp) = emit_trace_methods(class, construct_name, &signals);

    // Inject into header: trace methods + private members before closing };
    // We expect the header to NOT yet have };\n
    h.push_str(&trace_h);
    h.push_str("  FILE* _trace_fp = nullptr;\n");
    h.push_str("  uint64_t _trace_time = 0;\n");

    // Append trace impls to cpp
    cpp.push_str(&trace_cpp);
}

// ── Type helpers ──────────────────────────────────────────────────────────────

/// Evaluate a simple constant expression to a u32 bit-width.
fn eval_width(expr: &Expr) -> u32 {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(n)) => *n as u32,
        ExprKind::Literal(LitKind::Hex(n)) => *n as u32,
        ExprKind::Clog2(inner) => {
            let v = eval_width(inner);
            if v <= 1 {
                1
            } else {
                32 - (v - 1).leading_zeros()
            }
        }
        _ => 32,
    }
}

/// Param-aware width evaluator: folds bare `Ident` and arithmetic over
/// param defaults via `eval_const_expr_with_params`. Used in
/// width-bearing positions where `BitSlice` hi/lo or `PartSelect` width
/// may reference a param (e.g. `[CounterWidth-1:0]`). Falls back to the
/// legacy `eval_width` for shapes the const evaluator can't fold (which
/// preserves prior conservative-32 behavior).
fn eval_width_in(expr: &Expr, ctx: &Ctx) -> u32 {
    let folded = eval_const_expr_with_params(expr, ctx.params);
    if folded != 0
        || matches!(
            &expr.kind,
            ExprKind::Literal(LitKind::Dec(0)) | ExprKind::Literal(LitKind::Hex(0))
        )
    {
        folded as u32
    } else {
        eval_width(expr)
    }
}

/// Number of 32-bit words needed for `bits` bits.
fn wide_words(bits: u32) -> u32 {
    (bits + 31) / 32
}

/// True if a signal width requires a wide (VlWide) type.
fn is_wide_bits(bits: u32) -> bool {
    bits > 64
}

/// C++ type for a public port field.
#[deprecated(note = "use `cpp_port_type_with_params(.., &params)` — the bare form \
            silently buckets `UInt<PARAM>` into uint32_t even when the param \
            resolves to a wider value. See arch-com#447 §1 and PR #463 \
            extending #458 to the sibling helper cluster.")]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
fn cpp_port_type(ty: &TypeExpr) -> String {
    cpp_port_type_with_params(ty, &[])
}

/// Param-aware variant of [`cpp_port_type`]. Resolves param identifiers in
/// `UInt<W>` / `SInt<W>` widths via [`eval_const_expr_with_params`] so a
/// `UInt<ACC_WIDTH>` declaration (with `param ACC_WIDTH: const = 48`) gets
/// the right C++ bucket (e.g. `uint64_t` for 33..=64 bits). The legacy
/// `cpp_port_type` falls back to `eval_width`, which returns 32 for any
/// non-literal width and silently truncates 33..=64-bit fields to
/// `uint32_t`. arch-com#330.
fn cpp_port_type_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> String {
    match ty {
        TypeExpr::UInt(w) => {
            let b = eval_width_with_params(w, params);
            if is_wide_bits(b) {
                format!("VlWide<{}>", wide_words(b))
            } else {
                cpp_uint(b).to_string()
            }
        }
        TypeExpr::SInt(w) => {
            let b = eval_width_with_params(w, params);
            if is_wide_bits(b) {
                format!("VlWide<{}>", wide_words(b))
            } else {
                cpp_sint(b).to_string()
            }
        }
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => {
            "uint8_t".to_string()
        }
        // Floats are carried as their raw bit pattern in an unsigned integer
        // (FP32 → uint32_t, BF16 → uint16_t); arithmetic goes through the
        // `_arch_fp.h` helpers, never C++ float operators on the storage.
        TypeExpr::FP32 => "uint32_t".to_string(),
        TypeExpr::BF16 => "uint16_t".to_string(),
        TypeExpr::Named(n) => n.name.clone(),
        TypeExpr::Vec(_, _) => "uint32_t".to_string(),
    }
}

/// Param-aware width eval used by the type-emission helpers. Folds bare
/// `Ident` and basic arithmetic over `params` defaults; falls back to the
/// legacy literal-only `eval_width` for shapes the const evaluator can't
/// fold (preserving prior conservative-32 behavior). arch-com#330.
fn eval_width_with_params(expr: &Expr, params: &[ParamDecl]) -> u32 {
    let folded = eval_const_expr_with_params(expr, params);
    if folded != 0
        || matches!(
            &expr.kind,
            ExprKind::Literal(LitKind::Dec(0)) | ExprKind::Literal(LitKind::Hex(0))
        )
    {
        folded as u32
    } else {
        eval_width(expr)
    }
}

/// Substitute param idents in a TypeExpr (for bus param resolution in sim codegen).
fn subst_type_expr_sim(ty: &TypeExpr, params: &HashMap<String, &Expr>) -> TypeExpr {
    match ty {
        TypeExpr::UInt(w) => TypeExpr::UInt(Box::new(subst_expr_sim(w, params))),
        TypeExpr::SInt(w) => TypeExpr::SInt(Box::new(subst_expr_sim(w, params))),
        TypeExpr::Vec(inner, len) => TypeExpr::Vec(
            Box::new(subst_type_expr_sim(inner, params)),
            Box::new(subst_expr_sim(len, params)),
        ),
        other => other.clone(),
    }
}

fn subst_expr_sim(expr: &Expr, params: &HashMap<String, &Expr>) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(replacement) = params.get(name.as_str()) {
                return (*replacement).clone();
            } else {
                ExprKind::Ident(name.clone())
            }
        }
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            *op,
            Box::new(subst_expr_sim(l, params)),
            Box::new(subst_expr_sim(r, params)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(*op, Box::new(subst_expr_sim(e, params))),
        ExprKind::Ternary(c, t, e) => ExprKind::Ternary(
            Box::new(subst_expr_sim(c, params)),
            Box::new(subst_expr_sim(t, params)),
            Box::new(subst_expr_sim(e, params)),
        ),
        ExprKind::Clog2(e) => ExprKind::Clog2(Box::new(subst_expr_sim(e, params))),
        ExprKind::Index(b, i) => ExprKind::Index(
            Box::new(subst_expr_sim(b, params)),
            Box::new(subst_expr_sim(i, params)),
        ),
        _ => return expr.clone(),
    };
    Expr {
        kind,
        span: expr.span,
        parenthesized: expr.parenthesized,
    }
}

/// Return flattened bus port signals with direction: Vec<(flat_name, Direction, TypeExpr)>.
/// Direction is from the module's perspective (target flips initiator directions).
fn flatten_bus_port_with_dir(
    port_name: &str,
    bi: &BusPortInfo,
    symbols: &crate::resolve::SymbolTable,
    module_params: &[ParamDecl],
) -> Vec<(String, Direction, TypeExpr)> {
    let bus_name = &bi.bus_name.name;
    if let Some((crate::resolve::Symbol::Bus(info), _)) = symbols.globals.get(bus_name) {
        let mut param_map: HashMap<String, &Expr> = info
            .params
            .iter()
            .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
            .collect();
        for pa in &bi.params {
            param_map.insert(pa.name.name.clone(), &pa.value);
        }
        let eff = info.effective_signals(&param_map);
        let is_target = bi.perspective == BusPerspective::Target;
        // For Vec<Bus, N> ports, emit N copies of each signal with indexed prefix.
        // N is resolved against the enclosing module's params for the
        // param-driven `Vec<Bus, NUM_FOO>` case.
        let prefixes: Vec<String> = match bi.count.as_ref() {
            None => vec![port_name.to_string()],
            Some(count_expr) => {
                let n = eval_const_expr_with_params(count_expr, module_params) as u32;
                (0..n).map(|i| format!("{}_{}", port_name, i)).collect()
            }
        };
        let mut out = Vec::new();
        for prefix in &prefixes {
            for (sname, sdir, sty) in &eff {
                let subst_ty = subst_type_expr_sim(sty, &param_map);
                // Target perspective flips all signal directions
                let dir = if is_target {
                    match sdir {
                        Direction::In => Direction::Out,
                        Direction::Out => Direction::In,
                    }
                } else {
                    *sdir
                };
                out.push((format!("{}_{}", prefix, sname), dir, subst_ty));
            }
        }
        out
    } else {
        Vec::new()
    }
}

/// Return flattened bus port signals: Vec<(flat_name, TypeExpr)>.
/// E.g. port itcm: initiator ItcmIcb → [(itcm_cmd_valid, Bool), (itcm_cmd_addr, UInt<14>), ...]
/// Direction-discarding wrapper around `flatten_bus_port_with_dir` for callers
/// that don't need direction info (e.g. header field generation).
fn flatten_bus_port(
    port_name: &str,
    bi: &BusPortInfo,
    symbols: &crate::resolve::SymbolTable,
    module_params: &[ParamDecl],
) -> Vec<(String, TypeExpr)> {
    flatten_bus_port_with_dir(port_name, bi, symbols, module_params)
        .into_iter()
        .map(|(n, _d, t)| (n, t))
        .collect()
}

/// Expand whole-bus connections in an inst block into per-signal connections.
/// E.g. `axi_rd -> m_axi_mm2s` where axi_rd is a bus port on the target
/// construct expands to `axi_rd_ar_valid -> m_axi_mm2s_ar_valid`, etc.
/// Non-bus connections are returned unchanged.
fn expand_bus_connections(
    inst: &InstDecl,
    parent_module: &ModuleDecl,
    source: &SourceFile,
    symbols: &crate::resolve::SymbolTable,
    bus_wire_names: &HashSet<String>,
) -> Vec<Connection> {
    let m = parent_module;
    // Find the target construct's ports + params. Vec-of-bus counts are
    // resolved against the child module's params (with this inst's
    // `param NAME = ...` overrides applied) so that
    // `port chans: initiator Vec<B, N>;` with a param-driven N folds to a
    // concrete element count at the call site.
    let (target_ports, target_params): (Option<&[PortDecl]>, Vec<ParamDecl>) = source
        .items
        .iter()
        .find_map(|item| match item {
            Item::Module(m) if m.name.name == inst.module_name.name => {
                Some((Some(m.ports.as_slice()), m.params.clone()))
            }
            Item::Fsm(f) if f.name.name == inst.module_name.name => {
                Some((Some(f.ports.as_slice()), f.common.params.clone()))
            }
            _ => None,
        })
        .unwrap_or((None, Vec::new()));
    let mut child_params_overridden = target_params.clone();
    for pa in &inst.param_assigns {
        if let Some(p) = child_params_overridden
            .iter_mut()
            .find(|p| p.name.name == pa.name.name)
        {
            p.default = Some(pa.value.clone());
        }
    }
    let target_bus_ports: Vec<(String, &str, BusPerspective, &[ParamAssign])> = target_ports
        .map(|ports| {
            let mut v = Vec::new();
            for p in ports {
                if let Some(bi) = p.bus_info.as_ref() {
                    let bus = bi.bus_name.name.as_str();
                    match bi.count.as_ref() {
                        None => {
                            v.push((
                                p.name.name.clone(),
                                bus,
                                bi.perspective,
                                bi.params.as_slice(),
                            ));
                        }
                        Some(count_expr) => {
                            let n =
                                eval_const_expr_with_params(count_expr, &child_params_overridden)
                                    as u32;
                            for i in 0..n {
                                v.push((
                                    format!("{}_{}", p.name.name, i),
                                    bus,
                                    bi.perspective,
                                    bi.params.as_slice(),
                                ));
                            }
                        }
                    }
                }
            }
            v
        })
        .unwrap_or_default();
    // Whole Vec-of-bus port lookup, keyed by the bare name (no `_<i>` suffix).
    // Lets `chans -> w` (whole-vec inst connection) match against the child's
    // bare Vec<Bus,N> port name; we then expand it to N per-element bus
    // connections fed back into the standard expansion loop below.
    let target_vec_of_bus_ports: Vec<(String, u32)> = target_ports
        .map(|ports| {
            let mut v = Vec::new();
            for p in ports {
                if let Some(bi) = p.bus_info.as_ref() {
                    if let Some(count_expr) = bi.count.as_ref() {
                        let n = eval_const_expr_with_params(count_expr, &child_params_overridden)
                            as u32;
                        if n > 0 {
                            v.push((p.name.name.clone(), n));
                        }
                    }
                }
            }
            v
        })
        .unwrap_or_default();
    // Parent-side Vec<Bus,N> port and wire names → counts. Used together
    // with `target_vec_of_bus_ports` to detect a whole-vec connection
    // `chans -> w` where both sides are arrays.
    let parent_vec_of_bus_wires: HashMap<String, u32> = m
        .body
        .iter()
        .filter_map(|i| {
            if let ModuleBodyItem::WireDecl(w) = i {
                if let TypeExpr::Vec(elem, size_expr) = &w.ty {
                    if let TypeExpr::Named(id) = elem.as_ref() {
                        if matches!(
                            symbols.globals.get(&id.name),
                            Some((crate::resolve::Symbol::Bus(_), _))
                        ) {
                            let n = eval_const_expr_with_params(size_expr, &m.params) as u32;
                            if n > 0 {
                                return Some((w.name.name.clone(), n));
                            }
                        }
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();
    // 2D bus wires: `wire edges: Vec<Vec<B, N>, M>;` → (M, N).
    // Used by the whole-row inst connection expansion `outs -> edges[i]`,
    // where `edges[i]` is a row (Vec<B,N>) inside a 2D wire.
    let parent_vec_of_bus_wires_2d: HashMap<String, (u32, u32)> = m
        .body
        .iter()
        .filter_map(|i| {
            if let ModuleBodyItem::WireDecl(w) = i {
                if let TypeExpr::Vec(outer_elem, outer_size) = &w.ty {
                    if let TypeExpr::Vec(inner_elem, inner_size) = outer_elem.as_ref() {
                        if let TypeExpr::Named(id) = inner_elem.as_ref() {
                            if matches!(
                                symbols.globals.get(&id.name),
                                Some((crate::resolve::Symbol::Bus(_), _))
                            ) {
                                let m_n = eval_const_expr_with_params(outer_size, &m.params) as u32;
                                let n_n = eval_const_expr_with_params(inner_size, &m.params) as u32;
                                if m_n > 0 && n_n > 0 {
                                    return Some((w.name.name.clone(), (m_n, n_n)));
                                }
                            }
                        }
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();
    let parent_vec_of_bus_ports: HashMap<String, u32> = m
        .ports
        .iter()
        .filter_map(|p| {
            let bi = p.bus_info.as_ref()?;
            let count_expr = bi.count.as_ref()?;
            let n = eval_const_expr_with_params(count_expr, &m.params) as u32;
            if n > 0 {
                Some((p.name.name.clone(), n))
            } else {
                None
            }
        })
        .collect();
    // Pre-expand whole-vec inst connections (`chans -> w`) into N per-element
    // bus connections (`chans_0 -> w[0]; chans_1 -> w[1]; ...`). The body
    // loop below then expands each of those into per-signal connections via
    // the existing scalar+indexed paths.
    let inst_connections: Vec<crate::ast::Connection> = inst
        .connections
        .iter()
        .flat_map(|c| {
            if let Some((_, n)) = target_vec_of_bus_ports
                .iter()
                .find(|(pn, _)| pn == &c.port_name.name)
            {
                // Whole-row connection into a 2D bus wire: `outs -> edges[m]`,
                // where outs is Vec<B,N>, edges is Vec<Vec<B,N>,M>, m is a
                // literal (or static-unrolled loop var). Expand to N per-element
                // connections `outs[j] -> edges[m][j]`.
                if let ExprKind::Index(arr, idx) = &c.signal.kind {
                    if let ExprKind::Ident(parent_name) = &arr.kind {
                        if let Some((_m_n, n_n)) =
                            parent_vec_of_bus_wires_2d.get(parent_name).copied()
                        {
                            if let ExprKind::Literal(LitKind::Dec(m_idx)) = &idx.kind {
                                if (n_n as u32) == *n {
                                    return (0..*n)
                                        .map(|j| {
                                            let port_j = Ident::new(
                                                format!("{}_{}", c.port_name.name, j),
                                                c.port_name.span,
                                            );
                                            let parent_expr = Expr::new(
                                                ExprKind::Index(
                                                    Box::new(Expr::new(
                                                        ExprKind::Index(
                                                            Box::new(Expr::new(
                                                                ExprKind::Ident(
                                                                    parent_name.clone(),
                                                                ),
                                                                c.signal.span,
                                                            )),
                                                            Box::new(Expr::new(
                                                                ExprKind::Literal(LitKind::Dec(
                                                                    *m_idx,
                                                                )),
                                                                c.signal.span,
                                                            )),
                                                        ),
                                                        c.signal.span,
                                                    )),
                                                    Box::new(Expr::new(
                                                        ExprKind::Literal(LitKind::Dec(j as u64)),
                                                        c.signal.span,
                                                    )),
                                                ),
                                                c.signal.span,
                                            );
                                            crate::ast::Connection {
                                                port_name: port_j,
                                                direction: c.direction,
                                                signal: parent_expr,
                                                reset_override: None,
                                                span: c.span,
                                            }
                                        })
                                        .collect::<Vec<_>>();
                                }
                            }
                        }
                    }
                }
                if let ExprKind::Ident(parent_name) = &c.signal.kind {
                    let parent_is_vob_wire = parent_vec_of_bus_wires.contains_key(parent_name);
                    let parent_is_vob_port = parent_vec_of_bus_ports.contains_key(parent_name);
                    if parent_is_vob_wire || parent_is_vob_port {
                        return (0..*n)
                            .map(|i| {
                                let port_i = Ident::new(
                                    format!("{}_{}", c.port_name.name, i),
                                    c.port_name.span,
                                );
                                // Wire: emit Index(Ident(w), i) so downstream sees a
                                // bus-wire-array element. Port: emit Ident("w_<i>") so
                                // it lands at the flat per-element port name on the parent.
                                let parent_expr = if parent_is_vob_wire {
                                    Expr::new(
                                        ExprKind::Index(
                                            Box::new(Expr::new(
                                                ExprKind::Ident(parent_name.clone()),
                                                c.signal.span,
                                            )),
                                            Box::new(Expr::new(
                                                ExprKind::Literal(LitKind::Dec(i as u64)),
                                                c.signal.span,
                                            )),
                                        ),
                                        c.signal.span,
                                    )
                                } else {
                                    Expr::new(
                                        ExprKind::Ident(format!("{}_{}", parent_name, i)),
                                        c.signal.span,
                                    )
                                };
                                crate::ast::Connection {
                                    port_name: port_i,
                                    direction: c.direction,
                                    signal: parent_expr,
                                    reset_override: None,
                                    span: c.span,
                                }
                            })
                            .collect::<Vec<_>>();
                    }
                }
            }
            vec![c.clone()]
        })
        .collect();

    let mut expanded = Vec::new();
    for c in &inst_connections {
        if let Some((_, bus_name, perspective, bus_params)) = target_bus_ports
            .iter()
            .find(|(pn, _, _, _)| pn == &c.port_name.name)
        {
            // Bus connection — expand to individual signal connections
            if let Some((crate::resolve::Symbol::Bus(info), _)) = symbols.globals.get(*bus_name) {
                // Three shapes for the parent-side signal on a whole-bus binding:
                //   * `p -> ident`         where `ident` is a bus port or scalar bus wire
                //   * `p -> base.field`    where `base.field` is a bus port on the parent
                //   * `p -> wire[i]`       where `wire` is a Vec<Bus,N> wire — element i
                //
                // BindKind tells the per-signal emitter how to construct the parent-side
                // expression for a given signal name:
                //   FlatPort(prefix)   → emit `Ident("<prefix>_<sname>")`
                //                        (matches the flattened bus-port shape)
                //   WireStruct(name)   → emit `FieldAccess(Ident("<name>"), sname)`
                //                        (matches the C++ struct-typed bus wire)
                //   WireIndex(name, i) → emit `FieldAccess(Index(Ident("<name>"), i), sname)`
                //                        (matches a `B _let_<name>[N]` struct-array element)
                enum BindKind {
                    FlatPort(String),
                    WireStruct(String),
                    WireIndex(String, u32),
                    /// 2D bus wire element: `wire edges: Vec<Vec<B,N>,M>;` →
                    /// `edges[m][n]` lowers to `_let_edges[m][n].<sig>`.
                    Wire2DIndex(String, u32, u32),
                }
                let bind = match &c.signal.kind {
                    ExprKind::Ident(name) => {
                        if bus_wire_names.contains(name.as_str()) {
                            BindKind::WireStruct(name.clone())
                        } else {
                            BindKind::FlatPort(name.clone())
                        }
                    }
                    ExprKind::FieldAccess(base, field) => {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            BindKind::FlatPort(format!("{}_{}", base_name, field.name))
                        } else {
                            continue;
                        }
                    }
                    ExprKind::Index(arr, idx) => {
                        // 2D bus wire element: `edges[m][n]` → arr is itself
                        // an Index(Ident, literal_m), idx is literal_n.
                        if let ExprKind::Index(inner_arr, inner_idx) = &arr.kind {
                            if let (
                                ExprKind::Ident(arr_name),
                                ExprKind::Literal(LitKind::Dec(m)),
                                ExprKind::Literal(LitKind::Dec(n)),
                            ) = (&inner_arr.kind, &inner_idx.kind, &idx.kind)
                            {
                                BindKind::Wire2DIndex(arr_name.clone(), *m as u32, *n as u32)
                            } else {
                                continue;
                            }
                        } else if let (
                            ExprKind::Ident(arr_name),
                            ExprKind::Literal(LitKind::Dec(i)),
                        ) = (&arr.kind, &idx.kind)
                        {
                            if bus_wire_names.contains(arr_name.as_str())
                                || parent_vec_of_bus_wires.contains_key(arr_name.as_str())
                            {
                                // 1-D Vec-of-bus WIRE element (`wire s_int:
                                // Vec<B, N>;` → `s_int[i]`) OR a scalar bus
                                // wire indexed as a struct-array element. Both
                                // are stored as `B _let_<name>[N]`, so the
                                // parent-side ref is `_let_<name>[i].<sig>`.
                                // Without this branch a Vec-of-bus wire element
                                // fell through to `continue`, silently DROPPING
                                // the connection (inst port left undriven in the
                                // ARCH sim while the SV backend wired it) — the
                                // per-slave reg-slice `up <- s_int[j]` bug.
                                BindKind::WireIndex(arr_name.clone(), *i as u32)
                            } else if parent_vec_of_bus_ports.contains_key(arr_name.as_str()) {
                                // Vec-of-bus PORT element on the parent
                                // side (`port s: initiator Vec<B, N>`):
                                // flat name is `<port>_<i>`, mirroring
                                // the port flattening.
                                BindKind::FlatPort(format!("{}_{}", arr_name, i))
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let mut _pm = info.default_param_map();
                for pa in *bus_params {
                    _pm.insert(pa.name.name.clone(), &pa.value);
                }
                let _eff = info.effective_signals(&_pm);
                for (sname, sdir, _) in &_eff {
                    let inst_flat = format!("{}_{}", c.port_name.name, sname);
                    // Determine actual direction from the inst's bus perspective.
                    // For initiator: bus out → inst Output, bus in → inst Input.
                    // For target: bus out → inst Input (flipped), bus in → inst Output (flipped).
                    let actual_dir = match perspective {
                        BusPerspective::Initiator => *sdir,
                        BusPerspective::Target => (*sdir).flip(),
                    };
                    let dir = match actual_dir {
                        Direction::Out => ConnectDir::Output,
                        Direction::In => ConnectDir::Input,
                    };
                    let parent_signal = match &bind {
                        BindKind::FlatPort(prefix) => Expr::new(
                            ExprKind::Ident(format!("{}_{}", prefix, sname)),
                            c.signal.span,
                        ),
                        BindKind::WireStruct(name) => Expr::new(
                            ExprKind::FieldAccess(
                                Box::new(Expr::new(ExprKind::Ident(name.clone()), c.signal.span)),
                                Ident::new(sname.clone(), c.signal.span),
                            ),
                            c.signal.span,
                        ),
                        BindKind::WireIndex(name, i) => Expr::new(
                            ExprKind::FieldAccess(
                                Box::new(Expr::new(
                                    ExprKind::Index(
                                        Box::new(Expr::new(
                                            ExprKind::Ident(name.clone()),
                                            c.signal.span,
                                        )),
                                        Box::new(Expr::new(
                                            ExprKind::Literal(LitKind::Dec(*i as u64)),
                                            c.signal.span,
                                        )),
                                    ),
                                    c.signal.span,
                                )),
                                Ident::new(sname.clone(), c.signal.span),
                            ),
                            c.signal.span,
                        ),
                        BindKind::Wire2DIndex(name, m_idx, n_idx) => Expr::new(
                            ExprKind::FieldAccess(
                                Box::new(Expr::new(
                                    ExprKind::Index(
                                        Box::new(Expr::new(
                                            ExprKind::Index(
                                                Box::new(Expr::new(
                                                    ExprKind::Ident(name.clone()),
                                                    c.signal.span,
                                                )),
                                                Box::new(Expr::new(
                                                    ExprKind::Literal(LitKind::Dec(*m_idx as u64)),
                                                    c.signal.span,
                                                )),
                                            ),
                                            c.signal.span,
                                        )),
                                        Box::new(Expr::new(
                                            ExprKind::Literal(LitKind::Dec(*n_idx as u64)),
                                            c.signal.span,
                                        )),
                                    ),
                                    c.signal.span,
                                )),
                                Ident::new(sname.clone(), c.signal.span),
                            ),
                            c.signal.span,
                        ),
                    };
                    expanded.push(Connection {
                        port_name: Ident::new(inst_flat, c.port_name.span),
                        direction: dir,
                        signal: parent_signal,
                        reset_override: None,
                        span: c.span,
                    });
                }
            }
        } else {
            expanded.push(c.clone());
        }
    }
    expanded
}

/// C++ type for a private reg/let field.
/// 1–64 bits   → uint8/16/32/64_t
/// 65–128 bits → _arch_u128
/// >128 bits   → VlWide<N>  (same as port type, no conversion needed)
#[deprecated(
    note = "use `cpp_internal_type_with_params(.., &params)` — the bare form \
            silently buckets `UInt<PARAM>` regs/lets into the wrong scalar \
            type. See arch-com#447 §1 and PR #463 extending #458 to the \
            sibling helper cluster."
)]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
fn cpp_internal_type(ty: &TypeExpr) -> String {
    cpp_internal_type_with_params(ty, &[])
}

/// Param-aware variant of [`cpp_internal_type`]. See [`cpp_port_type_with_params`]
/// for rationale — without param resolution, `UInt<ACC_WIDTH>` regs/lets
/// get the wrong C++ scalar type. arch-com#330.
fn cpp_internal_type_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> String {
    match ty {
        TypeExpr::UInt(w) => {
            let b = eval_width_with_params(w, params);
            if b > 128 {
                format!("VlWide<{}>", wide_words(b))
            } else if b > 64 {
                "_arch_u128".to_string()
            } else {
                cpp_uint(b).to_string()
            }
        }
        TypeExpr::SInt(w) => {
            let b = eval_width_with_params(w, params);
            if b > 128 {
                format!("VlWide<{}>", wide_words(b))
            } else if b > 64 {
                "_arch_u128".to_string()
            } else {
                cpp_sint(b).to_string()
            }
        }
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => {
            "uint8_t".to_string()
        }
        TypeExpr::FP32 => "uint32_t".to_string(),
        TypeExpr::BF16 => "uint16_t".to_string(),
        TypeExpr::Named(n) => n.name.clone(),
        TypeExpr::Vec(_, _) => "uint32_t".to_string(),
    }
}

fn cpp_field_decl(name: &str, ty: &TypeExpr, params: &[ParamDecl]) -> String {
    if let Some((elem_ty, count)) = vec_array_info_with_params(ty, params) {
        format!("{elem_ty} {name}[{count}]")
    } else {
        format!("{} {name}", cpp_internal_type_with_params(ty, params))
    }
}

/// If `ty` is Vec<T, N>, return (elem_cpp_type, count_string).
///
/// Nested Vecs (e.g. `Vec<Vec<UInt<32>, 4>, 8>`) recurse: the innermost
/// non-Vec element type is returned as `elem_cpp_type`, and the count
/// string is the C-array dimension chain in source order separated by
/// `"]["` so a caller emitting `<elem>[<count>]` ends up with the
/// correct multi-dim C array (`uint32_t name[8][4]`).
#[deprecated(
    note = "use `vec_array_info_with_params(.., &params)` — the bare form \
            silently returns count=0 for `Vec<_, PARAM>` declarations. See \
            arch-com#447 §1 and PR #463 extending #458 to the sibling \
            helper cluster (twin of the PR #442 sites for the Vec-reg \
            storage path)."
)]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
fn vec_array_info(ty: &TypeExpr) -> Option<(String, String)> {
    // Backward-compatible wrapper: delegate to the param-aware version
    // with an empty params slice. Callers that need to resolve a
    // `Vec<_, PARAM_NAME>` count expression against an enclosing
    // construct's params must use `vec_array_info_with_params`
    // directly — see arch-com#447 §1.
    vec_array_info_with_params(ty, &[])
}

/// Evaluate a constant expression to a u64, resolving basic arithmetic.
/// Backward-compatible wrapper that doesn't resolve param identifiers —
/// see [`eval_const_expr_with_params`] for the version that does. Use
/// the param-aware version anywhere a Vec / array length needs to fold
/// across `param N: const = …;` references (otherwise the result is 0
/// and downstream code emits zero-sized C++ arrays — see the regression
/// fixed in PR #cam-zero-array).
#[deprecated(note = "use `eval_const_expr_with_params(.., &params)` — the bare \
            form silently miscompiles when the expression depends on \
            enclosing-construct params (Vec<_, PARAM>, UInt<PARAM>, \
            etc.). See arch-com#447 §1 and PRs #427, #439, #442 for \
            the bug class this guards against.")]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
fn eval_const_expr(expr: &Expr) -> u64 {
    eval_const_expr_with_params(expr, &[])
}

/// Walk `stmt` and return true if any expression has the shape
/// `Index(Ident(name), Ident(var))` where `name` is a Vec-of-bus port
/// or wire (keys of `ports` / `wires`). Used by the for-loop emitter
/// to decide whether to statically unroll the body. Recurses into all
/// sub-statements and into both sides of assignments.
fn stmt_indexes_vob_with_var(
    stmt: &Stmt,
    var: &str,
    ports: &HashMap<String, u32>,
    wires: &HashMap<String, u32>,
) -> bool {
    fn walk_expr(
        e: &Expr,
        var: &str,
        ports: &HashMap<String, u32>,
        wires: &HashMap<String, u32>,
    ) -> bool {
        if let ExprKind::Index(arr, idx) = &e.kind {
            if let (ExprKind::Ident(arr_name), ExprKind::Ident(idx_name)) = (&arr.kind, &idx.kind) {
                if idx_name == var && (ports.contains_key(arr_name) || wires.contains_key(arr_name))
                {
                    return true;
                }
            }
        }
        match &e.kind {
            ExprKind::Binary(_, l, r) => {
                walk_expr(l, var, ports, wires) || walk_expr(r, var, ports, wires)
            }
            ExprKind::Unary(_, x)
            | ExprKind::Cast(x, _)
            | ExprKind::LatencyAt(x, _)
            | ExprKind::SvaNext(_, x) => walk_expr(x, var, ports, wires),
            ExprKind::FieldAccess(b, _) => walk_expr(b, var, ports, wires),
            ExprKind::Index(b, i) | ExprKind::BitSlice(b, i, _) => {
                walk_expr(b, var, ports, wires) || walk_expr(i, var, ports, wires)
            }
            ExprKind::PartSelect(b, lo, hi, _) => {
                walk_expr(b, var, ports, wires)
                    || walk_expr(lo, var, ports, wires)
                    || walk_expr(hi, var, ports, wires)
            }
            ExprKind::Ternary(c, t, e2) => {
                walk_expr(c, var, ports, wires)
                    || walk_expr(t, var, ports, wires)
                    || walk_expr(e2, var, ports, wires)
            }
            ExprKind::Concat(parts) | ExprKind::FunctionCall(_, parts) => {
                parts.iter().any(|p| walk_expr(p, var, ports, wires))
            }
            ExprKind::MethodCall(b, _, args) => {
                walk_expr(b, var, ports, wires)
                    || args.iter().any(|a| walk_expr(a, var, ports, wires))
            }
            _ => false,
        }
    }
    match stmt {
        Stmt::Assign(a) => {
            walk_expr(&a.target, var, ports, wires) || walk_expr(&a.value, var, ports, wires)
        }
        Stmt::IfElse(ie) => {
            walk_expr(&ie.cond, var, ports, wires)
                || ie
                    .then_stmts
                    .iter()
                    .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
                || ie
                    .else_stmts
                    .iter()
                    .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
        }
        Stmt::Match(m) => {
            walk_expr(&m.scrutinee, var, ports, wires)
                || m.arms.iter().any(|arm| {
                    arm.body
                        .iter()
                        .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
                })
        }
        Stmt::For(f) => f
            .body
            .iter()
            .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires)),
        Stmt::Init(ib) => ib
            .body
            .iter()
            .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires)),
        Stmt::DoUntil { body, cond, .. } => {
            walk_expr(cond, var, ports, wires)
                || body
                    .iter()
                    .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
        }
        Stmt::WaitUntil(e, _) => walk_expr(e, var, ports, wires),
        Stmt::Log(l) => l.args.iter().any(|a| walk_expr(a, var, ports, wires)),
    }
}

/// Param-aware constant evaluator. Resolves bare identifiers against
/// `params` (regular + local) by recursing on each param's `default`
/// expression. Handles literals, `$clog2(x)`, unary `-`/`~`, and
/// binary `+`, `-`, `*`, `/`, `%`, `<<`, `>>`, `&`, `|`, `^`. Returns 0
/// for anything it can't fold (e.g. non-literal port reads), matching
/// the conservative behavior of the legacy single-arg version.
fn eval_const_expr_with_params(expr: &Expr, params: &[ParamDecl]) -> u64 {
    eval_const_expr_with_params_seen(expr, params, &mut HashSet::new())
}

/// Return true if the module body contains a preserved `Generate(For)`
/// block — these survive elaboration when the for-loop's range
/// depends on a module param and the body is shape-stable. Sim codegen
/// has no SV-genvar concept, so we run a local unroll pass before
/// walking the body.
fn module_body_has_preserved_generate(body: &[ModuleBodyItem]) -> bool {
    body.iter()
        .any(|it| matches!(it, ModuleBodyItem::Generate(GenerateDecl::For(_))))
}

/// Sim-local unroll for preserved `Generate(For)` blocks. Walks the
/// module body, expands each `For` loop's `Inst` items (and any other
/// generate-item kinds the elaborator's "shape-stable" gate may admit
/// in the future) into flat ModuleBodyItem entries. The expansion is
/// purely sim-local — the source AST is not mutated.
///
/// The elaborator's preservation gate only admits ranges that
/// `eval_const_expr_with_params` can resolve against the parent module's
/// param defaults. If we hit a range we can't evaluate, we leave the
/// generate intact (which would silently drop the body in downstream
/// sim walks); that case shouldn't occur given the preservation gate,
/// but we'd rather notice it as a broken sim than crash.
fn flatten_preserved_generates_for_sim(
    body: &[ModuleBodyItem],
    params: &[ParamDecl],
) -> Vec<ModuleBodyItem> {
    let mut out = Vec::with_capacity(body.len());
    for item in body {
        match item {
            ModuleBodyItem::Generate(GenerateDecl::For(gf)) => {
                // Resolve range bounds against the module's param defaults.
                let start = eval_const_expr_with_params(&gf.start, params) as i64;
                let end = eval_const_expr_with_params(&gf.end, params) as i64;
                if end < start {
                    // Empty range — emit nothing.
                    continue;
                }
                let var = &gf.var.name;
                for i in start..=end {
                    for git in &gf.items {
                        match git {
                            GenItem::Inst(inst) => {
                                out.push(ModuleBodyItem::Inst(crate::elaborate::subst_inst(
                                    inst, var, i,
                                )));
                            }
                            // The elaborator's preservation gate today
                            // restricts preserved generate_for bodies
                            // to inst-only (with shape-stable connections).
                            // Other GenItem kinds would have been unrolled
                            // at elaboration time. If a future expansion
                            // of the gate admits more kinds, extend here.
                            _ => {
                                // Conservative: ignore (matches pre-#399
                                // sim behavior for these kinds; the
                                // elaborator wouldn't reach here today).
                            }
                        }
                    }
                }
            }
            other => out.push(other.clone()),
        }
    }
    out
}

fn eval_const_expr_with_params_seen(
    expr: &Expr,
    params: &[ParamDecl],
    seen_params: &mut HashSet<String>,
) -> u64 {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v)) => *v,
        ExprKind::Literal(LitKind::Hex(v)) => *v,
        ExprKind::Literal(LitKind::Bin(v)) => *v,
        ExprKind::Literal(LitKind::Sized(_, v)) => *v,
        ExprKind::Ident(name) => {
            if let Some(p) = params.iter().find(|p| p.name.name == *name) {
                if let Some(d) = &p.default {
                    if !seen_params.insert(name.clone()) {
                        return 0;
                    }
                    let value = eval_const_expr_with_params_seen(d, params, seen_params);
                    seen_params.remove(name);
                    return value;
                }
            }
            0
        }
        ExprKind::Clog2(a) => {
            let v = eval_const_expr_with_params_seen(a, params, seen_params);
            if v <= 1 {
                0
            } else {
                64 - (v - 1).leading_zeros() as u64
            }
        }
        ExprKind::Unary(op, a) => {
            let v = eval_const_expr_with_params_seen(a, params, seen_params);
            match op {
                UnaryOp::Not => !v,
                UnaryOp::Neg => v.wrapping_neg(),
                _ => 0,
            }
        }
        ExprKind::Binary(op, l, r) => {
            let lv = eval_const_expr_with_params_seen(l, params, seen_params);
            let rv = eval_const_expr_with_params_seen(r, params, seen_params);
            match op {
                BinOp::Add => lv.wrapping_add(rv),
                BinOp::Sub => lv.wrapping_sub(rv),
                BinOp::Mul => lv.wrapping_mul(rv),
                BinOp::Div => {
                    if rv == 0 {
                        0
                    } else {
                        lv / rv
                    }
                }
                BinOp::Mod => {
                    if rv == 0 {
                        0
                    } else {
                        lv % rv
                    }
                }
                BinOp::Shl => lv.wrapping_shl(rv as u32),
                BinOp::Shr => lv.wrapping_shr(rv as u32),
                BinOp::BitAnd => lv & rv,
                BinOp::BitOr => lv | rv,
                BinOp::BitXor => lv ^ rv,
                _ => 0,
            }
        }
        _ => 0,
    }
}

/// Param-aware variant of [`vec_array_info`]. Uses
/// [`eval_const_expr_with_params`] so that `Vec<_, NUM_ENTRIES>` style
/// declarations whose count is a param identifier resolve to the
/// param's literal default (rather than silently degrading to 0 and
/// emitting a zero-sized C++ scratch array, which corrupts the
/// surrounding stack on memcpy/index).
fn vec_array_info_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> Option<(String, String)> {
    if let TypeExpr::Vec(elem, count_expr) = ty {
        let outer_count = eval_const_expr_with_params(count_expr, params).to_string();
        // Recursively descend nested Vecs — see vec_array_info docs.
        if let Some((inner_elem, inner_dims)) = vec_array_info_with_params(elem, params) {
            Some((inner_elem, format!("{outer_count}][{inner_dims}")))
        } else {
            let elem_type = cpp_internal_type_with_params(elem, params);
            Some((elem_type, outer_count))
        }
    } else {
        None
    }
}

/// If `expr` is a bare identifier, return its name — used for diagnostic
/// location strings in runtime bounds-check codegen.
fn base_ident_name(expr: &Expr) -> Option<&str> {
    if let ExprKind::Ident(n) = &expr.kind {
        Some(n.as_str())
    } else {
        None
    }
}

/// Local "is this expression a compile-time constant we can fold?" test.
/// Conservative: handles literals, `$clog2(const)`, and arithmetic over
/// already-reducible subtrees. Does NOT try to resolve param identifiers —
/// those are handled by the typecheck div-zero gate; here we return false
/// so the runtime `_ARCH_DCHK` still fires, which is safe (a non-zero param
/// just means the check succeeds silently).
fn is_const_reducible(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(_) => true,
        ExprKind::Clog2(a) => is_const_reducible(a),
        ExprKind::Binary(_, a, b) => is_const_reducible(a) && is_const_reducible(b),
        ExprKind::Unary(_, a) => is_const_reducible(a),
        _ => false,
    }
}

/// Smallest C++ unsigned integer type that fits `bits` (up to 64).
/// Returns true if `name` looks like a thread-lowered FSM state register.
/// Thread lowering in elaborate.rs (line ~1280) names state regs `_t{N}_state`
/// where N is the thread index. This helper is used by --debug-fsm and
/// auto-generated legal-state assertions to identify FSM state regs without
/// mis-matching user regs like `prev_state` or `state_counter`.
fn is_thread_fsm_state_reg(name: &str) -> bool {
    // Strip leading underscores (the shadow field is _t0_state, public is t0_state)
    let trimmed = name.trim_start_matches('_');
    if !trimmed.starts_with('t') {
        return false;
    }
    if !trimmed.ends_with("_state") {
        return false;
    }
    // Middle must be digits
    let mid = &trimmed[1..trimmed.len() - "_state".len()];
    !mid.is_empty() && mid.chars().all(|c| c.is_ascii_digit())
}

fn cpp_uint(bits: u32) -> &'static str {
    if bits <= 8 {
        "uint8_t"
    } else if bits <= 16 {
        "uint16_t"
    } else if bits <= 32 {
        "uint32_t"
    } else {
        "uint64_t"
    }
}

/// Return the bit-width of a TypeExpr, or 0 if indeterminate (e.g. Vec with param size).
fn type_width_of(ty: &TypeExpr) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => 1,
        TypeExpr::FP32 => 32,
        TypeExpr::BF16 => 16,
        TypeExpr::Vec(..) | TypeExpr::Named(_) => 0,
    }
}

/// Smallest C++ signed integer type that fits `bits` (up to 64).
fn cpp_sint(bits: u32) -> &'static str {
    if bits <= 8 {
        "int8_t"
    } else if bits <= 16 {
        "int16_t"
    } else if bits <= 32 {
        "int32_t"
    } else {
        "int64_t"
    }
}

/// Cast expression to `bits`-wide C++ type.
fn cast_to_bits(expr: &str, bits: u32) -> String {
    // Must mask to the exact bit-width, since C++ types are wider than the
    // HDL type (e.g. UInt<2> stored in uint8_t).
    if bits >= 64 {
        // 64-bit or wider: cast is sufficient (or use u128 path)
        format!("({})({})", cpp_uint(bits), expr)
    } else {
        let mask = (1u64 << bits) - 1;
        format!("({})((({}) & 0x{:X}ULL))", cpp_uint(bits), expr, mask)
    }
}

/// Cast expression to a signed HDL scalar width and sign-extend into the
/// selected C++ signed storage type. For example, SInt<40> uses int64_t
/// storage but bit 39 is the HDL sign bit, so the 40-bit truncated pattern
/// must be shifted through bit 63 before arithmetic use.
fn cast_to_signed_bits(expr: &str, bits: u32) -> String {
    if bits >= 64 {
        format!("({})({})", cpp_sint(bits), expr)
    } else {
        let mask = (1u64 << bits) - 1;
        let cpp_bits = if bits <= 8 {
            8
        } else if bits <= 16 {
            16
        } else if bits <= 32 {
            32
        } else {
            64
        };
        let shift = cpp_bits - bits;
        let ty = cpp_sint(bits);
        format!("(({ty})(((uint64_t)({expr}) & 0x{mask:X}ULL) << {shift}) >> {shift})")
    }
}

/// Bit-range extraction from a narrow value: `(expr >> lo) & mask`.
fn bit_range(expr: &str, hi: u32, lo: u32) -> String {
    let width = hi - lo + 1;
    let mask = if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    format!("(({} >> {}) & 0x{:X}ULL)", expr, lo, mask)
}

/// Bit-range extraction from a `_arch_u128` value.
fn bit_range_u128(expr: &str, hi: u32, lo: u32) -> String {
    let width = hi - lo + 1;
    let result_type = cpp_uint(width);
    if lo == 0 && width >= 128 {
        format!("({result_type})({})", expr)
    } else if lo == 0 {
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        format!(
            "({result_type})(((_arch_u128)({}) & (_arch_u128)0x{:X}ULL))",
            expr, mask
        )
    } else {
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        format!(
            "({result_type})(((_arch_u128)({}) >> {}) & (_arch_u128)0x{:X}ULL)",
            expr, lo, mask
        )
    }
}

/// Convert SV/ARCH format string tokens to printf equivalents.
fn sv_fmt_to_printf(s: &str) -> String {
    s.replace("%0t", "%lu")
        .replace("%0d", "%lld")
        .replace("%0h", "%llx")
        .replace("%0b", "%llu")
        .replace("%t", "%lu")
        .replace("%h", "%llx")
        .replace("%d", "%lld")
        .replace("%b", "%llu")
}

// ── Expression context ────────────────────────────────────────────────────────

struct Ctx<'a> {
    reg_names: &'a HashSet<String>,
    port_names: &'a HashSet<String>,
    let_names: &'a HashSet<String>,
    /// Map of module-scope let-binding names → their RHS expressions.
    /// Populated via `Ctx::with_let_values`. Used by `Stmt::Match` to
    /// fold `Pattern::Ident` arms into literal case labels.
    let_values: Option<&'a HashMap<String, Expr>>,
    inst_names: &'a HashSet<String>,
    /// Signals whose type is >64 bits wide (require special handling).
    wide_names: &'a HashSet<String>,
    /// Signal name → bit width for known signals (used for concat width inference).
    widths: &'a HashMap<String, u32>,
    /// Signal names whose HDL scalar type is signed.
    signed_names: &'a HashSet<String>,
    /// Signal name → floating-point format (FP32/BF16). Used to dispatch
    /// `+ - *` and comparisons to the `_arch_fp.h` helpers instead of integer
    /// operators on the bit-pattern carrier.
    float_names: &'a HashMap<String, FpFmt>,
    posedge_lhs: bool,
    /// FSM mode: regs are public members, no `_` prefix on reads
    fsm_mode: bool,
    enum_map: &'a HashMap<String, Vec<(String, u64)>>,
    /// Bus port names (for FieldAccess rewriting: itcm.cmd_valid → itcm_cmd_valid).
    bus_ports: &'a HashSet<String>,
    /// Reset port name → level, for `.asserted` polarity abstraction.
    reset_levels: &'a HashMap<String, ResetLevel>,
    /// Reg/wire names whose type is Vec<T,N> — these use C array subscript `[i]`.
    /// All other subscripts on scalar UInt/SInt use bit extraction `(x >> i) & 1`.
    vec_names: Option<&'a HashSet<String>>,
    /// Names of *2D* Vec<Vec<_,_>,_> wires/regs (today: Vec-of-Vec-of-bus
    /// `wire edges: Vec<Vec<B, N>, M>`). When the outer Index returns
    /// `_let_edges[m]`, the result is still a Vec — the inner subscript
    /// must keep using C array indexing `[n]`, NOT fall into the bit-shift
    /// path for scalar types.
    vec_2d_names: Option<&'a HashSet<String>>,
    /// Vec<T,N> sizes by name (element count). Used for runtime bounds-check codegen.
    vec_sizes: Option<&'a HashMap<String, u64>>,
    /// FSM Vec port-regs: always resolve to `_name` (internal C array), regardless of fsm_mode.
    /// These ports have flat public fields (name_0..name_N-1) but internal storage `_name[N]`.
    fsm_vec_port_regs: Option<&'a HashSet<String>>,
    /// Identifier substitutions active while emitting a Vec method predicate
    /// (e.g. "item" → "vec[3]", "index" → "3"). Checked first in the Ident
    /// branch of `cpp_expr`; None or missing key means normal resolution.
    ident_subst: Option<&'a HashMap<String, String>>,
    /// Loop-variable → integer-value substitutions pushed during static
    /// unrolling of `for` loops over Vec-of-bus indexed access. RefCell
    /// for interior mutability — the for-loop emitter mutates per
    /// iteration while emit_stmt walks the body. None = no active unroll.
    loop_var_subst: Option<&'a std::cell::RefCell<HashMap<String, u32>>>,
    /// Vec-of-bus port name → element count. Used by the for-loop emitter
    /// to decide whether the body needs static unrolling.
    vec_of_bus_port_count: Option<&'a HashMap<String, u32>>,
    /// Same as `vec_of_bus_port_count` but for `wire w: Vec<BusName, N>;`.
    vec_of_bus_wire_count: Option<&'a HashMap<String, u32>>,
    /// Branch-coverage registry for the current module. None when --coverage
    /// is off; Some(_) when on. emit_*_if_else allocates counter ids here
    /// and emits `_arch_cov[N]++;` at the start of each arm.
    coverage: Option<&'a std::cell::RefCell<CoverageRegistry>>,
    /// Module params (regular + local) for param-aware constant folding in
    /// width-bearing positions. Used by `eval_width_in` to fold expressions
    /// like `CounterWidth-1` in `BitSlice` hi/lo and `PartSelect` width.
    /// Empty by default; populated via [`Ctx::with_params`] at module entry.
    params: &'a [ParamDecl],
}

impl<'a> Ctx<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        reg_names: &'a HashSet<String>,
        port_names: &'a HashSet<String>,
        let_names: &'a HashSet<String>,
        inst_names: &'a HashSet<String>,
        wide_names: &'a HashSet<String>,
        widths: &'a HashMap<String, u32>,
        enum_map: &'a HashMap<String, Vec<(String, u64)>>,
        bus_ports: &'a HashSet<String>,
    ) -> Self {
        static EMPTY_RESET_LEVELS: std::sync::OnceLock<HashMap<String, ResetLevel>> =
            std::sync::OnceLock::new();
        static EMPTY_SIGNED_NAMES: std::sync::OnceLock<HashSet<String>> =
            std::sync::OnceLock::new();
        static EMPTY_FLOAT_NAMES: std::sync::OnceLock<HashMap<String, FpFmt>> =
            std::sync::OnceLock::new();
        let reset_levels = EMPTY_RESET_LEVELS.get_or_init(HashMap::new);
        let signed_names = EMPTY_SIGNED_NAMES.get_or_init(HashSet::new);
        let float_names = EMPTY_FLOAT_NAMES.get_or_init(HashMap::new);
        static EMPTY_PARAMS: &[ParamDecl] = &[];
        Ctx {
            reg_names,
            port_names,
            let_names,
            let_values: None,
            inst_names,
            wide_names,
            widths,
            signed_names,
            float_names,
            posedge_lhs: false,
            fsm_mode: false,
            enum_map,
            bus_ports,
            reset_levels,
            vec_names: None,
            vec_2d_names: None,
            vec_sizes: None,
            fsm_vec_port_regs: None,
            ident_subst: None,
            loop_var_subst: None,
            vec_of_bus_port_count: None,
            vec_of_bus_wire_count: None,
            coverage: None,
            params: EMPTY_PARAMS,
        }
    }

    fn with_vec_of_bus(
        mut self,
        ports: &'a HashMap<String, u32>,
        wires: &'a HashMap<String, u32>,
        subst: &'a std::cell::RefCell<HashMap<String, u32>>,
    ) -> Self {
        self.vec_of_bus_port_count = Some(ports);
        self.vec_of_bus_wire_count = Some(wires);
        self.loop_var_subst = Some(subst);
        self
    }

    fn with_signed_names(mut self, signed_names: &'a HashSet<String>) -> Self {
        self.signed_names = signed_names;
        self
    }

    fn with_float_names(mut self, float_names: &'a HashMap<String, FpFmt>) -> Self {
        self.float_names = float_names;
        self
    }

    fn with_params(mut self, params: &'a [ParamDecl]) -> Self {
        self.params = params;
        self
    }

    fn with_let_values(mut self, let_values: &'a HashMap<String, Expr>) -> Self {
        self.let_values = Some(let_values);
        self
    }

    fn with_vec_sizes(mut self, vec_sizes: &'a HashMap<String, u64>) -> Self {
        self.vec_sizes = Some(vec_sizes);
        self
    }

    fn with_reset_levels(mut self, reset_levels: &'a HashMap<String, ResetLevel>) -> Self {
        self.reset_levels = reset_levels;
        self
    }

    fn with_vec_names(mut self, vec_names: &'a HashSet<String>) -> Self {
        self.vec_names = Some(vec_names);
        self
    }

    fn with_vec_2d_names(mut self, vec_2d_names: &'a HashSet<String>) -> Self {
        self.vec_2d_names = Some(vec_2d_names);
        self
    }

    fn with_fsm_vec_port_regs(mut self, fsm_vec_port_regs: &'a HashSet<String>) -> Self {
        self.fsm_vec_port_regs = Some(fsm_vec_port_regs);
        self
    }

    fn with_ident_subst(mut self, ident_subst: &'a HashMap<String, String>) -> Self {
        self.ident_subst = Some(ident_subst);
        self
    }

    fn with_coverage(mut self, reg: Option<&'a std::cell::RefCell<CoverageRegistry>>) -> Self {
        self.coverage = reg;
        self
    }

    fn posedge(mut self) -> Self {
        self.posedge_lhs = true;
        self
    }

    /// Resolve a name to its C++ field/variable name.
    fn resolve_name(&self, name: &str, is_lhs: bool) -> String {
        // FSM Vec port-regs always use `_name` (internal C array) regardless of mode.
        if self.fsm_vec_port_regs.map_or(false, |s| s.contains(name)) {
            return format!("_{name}");
        }
        if self.reg_names.contains(name) {
            if is_lhs && self.posedge_lhs {
                format!("_n_{name}")
            } else if self.fsm_mode {
                name.to_string()
            } else {
                format!("_{name}")
            }
        } else if self.let_names.contains(name) {
            // `let port_name = expr` is a port-driver: there is no separate
            // `_let_port_name` storage. Reads (and the LHS write inside the
            // synthesized comb assign) bind to the public port field.
            if self.port_names.contains(name) {
                name.to_string()
            } else if self.fsm_mode {
                name.to_string()
            } else {
                format!("_let_{name}")
            }
        } else if self.inst_names.contains(name) {
            format!("_inst_{name}")
        } else if self.port_names.contains(name)
            && self.vec_names.map_or(false, |s| s.contains(name))
        {
            // Vec-typed port: header exposes flattened `name_0..name_N-1`
            // scalars for external access, but the body indexes into the
            // internal `_name[N]` array. Without this branch, a body that
            // reads / writes `port_name[i]` lowered to `port_name[0]` —
            // referencing a name that doesn't exist in the C++ class.
            format!("_{name}")
        } else {
            name.to_string()
        }
    }

    /// Emit a signal read.
    /// • 65–128-bit input ports: VlWide<4> → _arch_u128 conversion
    /// • >128-bit input ports:   return VlWide<N> directly (same as internal type)
    fn read_signal(&self, name: &str) -> String {
        let base = self.resolve_name(name, false);
        if self.wide_names.contains(name) && self.port_names.contains(name) {
            let bits = self.widths.get(name).copied().unwrap_or(0);
            if bits > 128 {
                // Internal and port both VlWide<N> — no conversion needed
                base
            } else {
                // 65–128 bit: port is VlWide<ceil(W/32)>, internal arithmetic
                // uses _arch_u128. Pass the real word count so a VlWide<3>
                // (66–96 bit) backing array is not read out of bounds.
                let words = wide_words(bits);
                format!("_arch_vl_to_u128({base}._data, {words})")
            }
        } else {
            base
        }
    }

    fn vec_path_of_expr(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::Ident(name) => Some(name.clone()),
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.bus_ports.contains(base_name.as_str()) {
                        Some(format!("{}_{}", base_name, field.name))
                    } else {
                        Some(format!("{}.{}", base_name, field.name))
                    }
                } else if let Some(base_path) = self.vec_path_of_expr(base) {
                    Some(format!("{}.{}", base_path, field.name))
                } else {
                    None
                }
            }
            // Outer index of a 2D Vec (e.g. `Vec<Vec<Bus,N>,M>`): the result
            // is still a Vec, so propagate the base name. Used by
            // `expr_is_vec` so the *inner* subscript stays as C array
            // indexing instead of falling into bit-shift extraction.
            ExprKind::Index(base, _) => {
                if let ExprKind::Ident(name) = &base.kind {
                    if self
                        .vec_2d_names
                        .map_or(false, |s| s.contains(name.as_str()))
                    {
                        return Some(name.clone());
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn expr_is_vec(&self, expr: &Expr) -> bool {
        self.vec_path_of_expr(expr)
            .map(|name| self.vec_names.map_or(false, |s| s.contains(name.as_str())))
            .unwrap_or(false)
    }

    fn expr_vec_size(&self, expr: &Expr) -> Option<u64> {
        self.vec_path_of_expr(expr)
            .and_then(|name| self.vec_sizes.and_then(|m| m.get(name.as_str()).copied()))
    }
}

// ── Width inference ───────────────────────────────────────────────────────────

fn infer_expr_width(expr: &Expr, ctx: &Ctx) -> u32 {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(&w) = ctx.widths.get(name.as_str()) {
                w
            } else {
                eprintln!(
                    "warning: sim codegen: width of identifier '{}' unknown; \
                     defaulting to 8 — concat / shift positions derived from \
                     this may be incorrect",
                    name
                );
                8
            }
        }
        ExprKind::LatencyAt(inner, _) | ExprKind::SvaNext(_, inner) => infer_expr_width(inner, ctx),
        ExprKind::Literal(LitKind::Sized(w, _)) => *w,
        ExprKind::Literal(_) => 32,
        ExprKind::Bool(_) => 1,
        ExprKind::MethodCall(base, method, _) if method.name == "reverse" => {
            infer_expr_width(base, ctx)
        }
        ExprKind::MethodCall(_, method, args)
            if method.name == "trunc"
                || method.name == "zext"
                || method.name == "sext"
                || method.name == "resize" =>
        {
            if let Some(w) = args.first() {
                eval_width_in(w, ctx)
            } else {
                8
            }
        }
        ExprKind::BitSlice(_, hi, lo) => {
            let h = eval_width_in(hi, ctx);
            let l = eval_width_in(lo, ctx);
            h - l + 1
        }
        ExprKind::PartSelect(_, _, width, _) => eval_width_in(width, ctx),
        ExprKind::Cast(_, ty) => match ty.as_ref() {
            TypeExpr::UInt(w) => eval_width_in(w, ctx),
            TypeExpr::SInt(w) => eval_width_in(w, ctx),
            _ => 8,
        },
        ExprKind::Concat(parts) => parts.iter().map(|p| infer_expr_width(p, ctx)).sum(),
        ExprKind::Index(base, _) => {
            // For Vec<T, N>[i] the result width is element T's width.
            // For scalar UInt/SInt[i] (bit indexing), the result is 1 bit.
            // Pre-fix: Index fell through to default 8, which broke
            // concat width inference (e.g. `{20{instr[31]}}` reported as
            // 160 bits instead of 20, blowing past the 32-bit port type
            // and emitting a VlWide<6> RHS for a uint32_t port).
            if let Some(base_name) = ctx.vec_path_of_expr(base) {
                if ctx
                    .vec_names
                    .map_or(false, |s| s.contains(base_name.as_str()))
                {
                    // Vec element width: total port/reg/field width / element count.
                    let total = ctx.widths.get(base_name.as_str()).copied().unwrap_or(0);
                    let count = ctx
                        .vec_sizes
                        .and_then(|m| m.get(base_name.as_str()))
                        .copied()
                        .unwrap_or(0);
                    if count > 0 && total > 0 {
                        return (total as u64 / count) as u32;
                    }
                }
            }
            // Scalar bit index → 1 bit.
            1
        }
        ExprKind::Repeat(count, value) => {
            let n = eval_width(count);
            let w = infer_expr_width(value, ctx);
            n * w
        }
        ExprKind::Binary(op, lhs, rhs) => {
            match op {
                // Comparison and logical ops always produce 1-bit Bool
                BinOp::Eq
                | BinOp::Neq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Lte
                | BinOp::Gte
                | BinOp::And
                | BinOp::Or => 1,
                // Bitwise ops: result width = max of operand widths
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                    let lw = infer_expr_width(lhs, ctx);
                    let rw = infer_expr_width(rhs, ctx);
                    std::cmp::max(lw, rw)
                }
                // Shift ops: result width = left operand width
                BinOp::Shl | BinOp::Shr => infer_expr_width(lhs, ctx),
                // Arithmetic ops: result width = max of operand widths
                _ => {
                    let lw = infer_expr_width(lhs, ctx);
                    let rw = infer_expr_width(rhs, ctx);
                    std::cmp::max(lw, rw)
                }
            }
        }
        ExprKind::Unary(UnaryOp::Not, _) => 1,
        ExprKind::Unary(UnaryOp::RedAnd, _)
        | ExprKind::Unary(UnaryOp::RedOr, _)
        | ExprKind::Unary(UnaryOp::RedXor, _) => 1,
        ExprKind::Ternary(_, then_expr, _) => infer_expr_width(then_expr, ctx),
        ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => infer_expr_width(inner, ctx),
        ExprKind::FieldAccess(base, field) => {
            // Struct field access: look up by "<base>.<field>" key the caller
            // populated from the struct decl. Covers two shapes:
            //   - `ctrl_r.mode`             — base is Ident
            //   - `ch_r[0].threshold`       — base is Index of Ident (Vec elem)
            // Both resolve to the same struct-field width regardless of which
            // element index is being accessed.
            let base_name = match &base.kind {
                ExprKind::Ident(name) => Some(name.as_str()),
                ExprKind::Index(b, _) => match &b.kind {
                    ExprKind::Ident(name) => Some(name.as_str()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(name) = base_name {
                let key = format!("{}.{}", name, field.name);
                if let Some(&w) = ctx.widths.get(key.as_str()) {
                    return w;
                }
            }
            // Bus field — falls through to the flattened C++ name lookup.
            let flat = cpp_expr_inner(expr, ctx, false);
            if let Some(&w) = ctx.widths.get(flat.as_str()) {
                w
            } else {
                eprintln!(
                    "warning: sim codegen: width of field access '{}' unknown; \
                     defaulting to 8 — concat / shift positions derived from \
                     this may be incorrect",
                    flat
                );
                8
            }
        }
        _ => 8,
    }
}

fn infer_expr_signed(expr: &Expr, ctx: &Ctx) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => ctx.signed_names.contains(name.as_str()),
        ExprKind::FieldAccess(base, field) => {
            if let ExprKind::Ident(base_name) = &base.kind {
                let flat = if ctx.bus_ports.contains(base_name.as_str()) {
                    format!("{}_{}", base_name, field.name)
                } else {
                    format!("{}.{}", base_name, field.name)
                };
                ctx.signed_names.contains(flat.as_str())
            } else {
                false
            }
        }
        ExprKind::Cast(_, ty) => matches!(ty.as_ref(), TypeExpr::SInt(_)),
        ExprKind::Signed(_) => true,
        ExprKind::Unsigned(_) => false,
        ExprKind::MethodCall(base, method, _)
            if matches!(
                method.name.as_str(),
                "trunc" | "sext" | "resize" | "reverse"
            ) =>
        {
            infer_expr_signed(base, ctx)
        }
        ExprKind::Unary(UnaryOp::Neg, _) => true,
        ExprKind::Unary(_, inner) => infer_expr_signed(inner, ctx),
        ExprKind::Binary(op, lhs, rhs) => match op {
            BinOp::Eq
            | BinOp::Neq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::Lte
            | BinOp::Gte
            | BinOp::And
            | BinOp::Or => false,
            _ => infer_expr_signed(lhs, ctx) || infer_expr_signed(rhs, ctx),
        },
        ExprKind::Ternary(_, then_expr, else_expr) => {
            infer_expr_signed(then_expr, ctx) || infer_expr_signed(else_expr, ctx)
        }
        _ => false,
    }
}

/// Floating-point format of a sim signal/expression. Mirrors `Ty::FP32`/`BF16`
/// but local to the sim backend so it can live in `Ctx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FpFmt {
    Fp32,
    Bf16,
}

impl FpFmt {
    /// Suffix used in the `_arch_fp.h` helper names: `_arch_f32_add` / `_arch_bf16_add`.
    fn helper_tag(self) -> &'static str {
        match self {
            FpFmt::Fp32 => "f32",
            FpFmt::Bf16 => "bf16",
        }
    }
}

/// Infer the floating-point format of an expression, or `None` if it is not a
/// float. Drives dispatch of `+ - *` / comparisons to the `_arch_fp.h` helpers.
fn infer_expr_float(expr: &Expr, ctx: &Ctx) -> Option<FpFmt> {
    match &expr.kind {
        ExprKind::Ident(name) => ctx.float_names.get(name.as_str()).copied(),
        // Float literals default to FP32.
        ExprKind::Literal(LitKind::Float(_)) => Some(FpFmt::Fp32),
        ExprKind::Cast(_, ty) => match ty.as_ref() {
            TypeExpr::FP32 => Some(FpFmt::Fp32),
            TypeExpr::BF16 => Some(FpFmt::Bf16),
            _ => None,
        },
        ExprKind::MethodCall(_, method, _) => match method.name.as_str() {
            "to_fp32" => Some(FpFmt::Fp32),
            "to_bf16" => Some(FpFmt::Bf16),
            _ => None, // to_uint/to_sint produce integers
        },
        // Arithmetic preserves the float format; comparisons are not float.
        ExprKind::Binary(op, lhs, rhs) => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                infer_expr_float(lhs, ctx).or_else(|| infer_expr_float(rhs, ctx))
            }
            _ => None,
        },
        ExprKind::Ternary(_, then_expr, else_expr) => {
            infer_expr_float(then_expr, ctx).or_else(|| infer_expr_float(else_expr, ctx))
        }
        ExprKind::FunctionCall(name, args) if name == "fma" => {
            args.first().and_then(|a| infer_expr_float(a, ctx))
        }
        _ => None,
    }
}

// ── Expression emitter ────────────────────────────────────────────────────────

/// Lower a Vec method call (any/all/count/contains/reduce_*) to an
/// unrolled C++ expression. Predicate identifier substitution for
/// `item`/`index` is done by building a fresh `Ctx` with `ident_subst`
/// pointing at the per-iteration map.
fn lower_vec_method_cpp(
    recv_b: &str,
    recv: &Expr,
    method: &Ident,
    args: &[Expr],
    ctx: &Ctx,
) -> String {
    let n = match &recv.kind {
        ExprKind::Ident(n) => ctx.vec_sizes.and_then(|m| m.get(n)).copied(),
        _ => None,
    };
    let Some(n) = n else {
        return format!("{recv_b}.{}()", method.name);
    };
    let n_usize = n as usize;

    let emit_at = |i: u64| -> String {
        let mut sub: HashMap<String, String> = HashMap::new();
        sub.insert("item".to_string(), format!("{recv_b}[{i}]"));
        sub.insert("index".to_string(), format!("{i}"));
        let sub_ctx = Ctx {
            reg_names: ctx.reg_names,
            port_names: ctx.port_names,
            let_names: ctx.let_names,
            let_values: ctx.let_values,
            inst_names: ctx.inst_names,
            wide_names: ctx.wide_names,
            widths: ctx.widths,
            signed_names: ctx.signed_names,
            float_names: ctx.float_names,
            posedge_lhs: ctx.posedge_lhs,
            fsm_mode: ctx.fsm_mode,
            enum_map: ctx.enum_map,
            bus_ports: ctx.bus_ports,
            reset_levels: ctx.reset_levels,
            vec_names: ctx.vec_names,
            vec_2d_names: ctx.vec_2d_names,
            vec_sizes: ctx.vec_sizes,
            fsm_vec_port_regs: ctx.fsm_vec_port_regs,
            ident_subst: None, // replaced below via a temporary binding
            loop_var_subst: ctx.loop_var_subst,
            vec_of_bus_port_count: ctx.vec_of_bus_port_count,
            vec_of_bus_wire_count: ctx.vec_of_bus_wire_count,
            coverage: ctx.coverage,
            params: ctx.params,
        };
        // The sub map must outlive the cpp_expr call. We keep `sub` as a
        // stack-local binding whose lifetime covers the call.
        let ctx_with_sub = Ctx {
            ident_subst: Some(&sub),
            ..sub_ctx
        };
        if let Some(pred) = args.first() {
            cpp_expr(pred, &ctx_with_sub)
        } else {
            String::new()
        }
    };

    match method.name.as_str() {
        "any" => {
            if n_usize == 0 {
                return "false".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(emit_at).collect();
            format!("({})", terms.join(" || "))
        }
        "all" => {
            if n_usize == 0 {
                return "true".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(emit_at).collect();
            format!("({})", terms.join(" && "))
        }
        "count" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64)
                .map(|i| format!("({} ? 1u : 0u)", emit_at(i)))
                .collect();
            format!("({})", terms.join(" + "))
        }
        "contains" => {
            let Some(x_expr) = args.first() else {
                return "false".to_string();
            };
            let x = cpp_expr(x_expr, ctx);
            if n_usize == 0 {
                return "false".to_string();
            }
            let terms: Vec<String> = (0..n as u64)
                .map(|i| format!("({recv_b}[{i}] == {x})"))
                .collect();
            format!("({})", terms.join(" || "))
        }
        "reduce_or" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" | "))
        }
        "reduce_and" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" & "))
        }
        "reduce_xor" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" ^ "))
        }
        _ => format!("{recv_b}.{}()", method.name),
    }
}

fn cpp_expr(expr: &Expr, ctx: &Ctx) -> String {
    cpp_expr_inner(expr, ctx, false)
}

fn cpp_condition(expr: &Expr, ctx: &Ctx) -> String {
    let cond = cpp_expr(expr, ctx);
    if is_fully_wrapped_in_parens(&cond) {
        cond
    } else {
        format!("({cond})")
    }
}

/// Check if `s` is fully wrapped in a single balanced pair of outer parens.
/// Returns true for `(!busy)` and `(a + b)`, false for `(uint8_t)(!busy)` where
/// the first `)` closes the cast, not the whole expression.
fn is_fully_wrapped_in_parens(s: &str) -> bool {
    let s = s.trim();
    if !s.starts_with('(') || !s.ends_with(')') {
        return false;
    }
    let mut depth = 0u32;
    for (i, c) in s.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 && i < s.len() - 1 {
                return false; // closed before the end — not fully wrapped
            }
        }
    }
    depth == 0
}

fn cpp_expr_lhs(expr: &Expr, ctx: &Ctx) -> String {
    cpp_expr_inner(expr, ctx, true)
}

fn cpp_expr_inner(expr: &Expr, ctx: &Ctx, is_lhs: bool) -> String {
    match &expr.kind {
        // Latency annotation: transparent to sim emission. The assignment
        // site handles directing the write to stage 0 of the pipe chain;
        // reads of `q@0` collapse to the final-output field of the pipe.
        ExprKind::LatencyAt(inner, _) => cpp_expr_inner(inner, ctx, is_lhs),
        // SVA `##N expr` is sim-irrelevant (assert/cover bodies aren't
        // lowered to runtime checks); emit the inner expression as a
        // safe fallback in case it's ever reached via a non-assert path.
        ExprKind::SvaNext(_, inner) => cpp_expr_inner(inner, ctx, is_lhs),
        // SynthIdent: emit as a plain identifier. Simulation support for
        // credit_channel (counter + FIFO mirror in C++) is separate work;
        // designs that use method dispatch today work under `arch build`
        // but not under `arch sim` — the name will reference an undefined
        // C++ symbol at sim-compile time. Intentional: we surface the gap
        // loudly rather than silently succeed.
        ExprKind::SynthIdent(name, _) => name.clone(),
        ExprKind::Literal(lit) => match lit {
            LitKind::Dec(v) => format!("{v}"),
            LitKind::Hex(v) => format!("0x{v:X}"),
            LitKind::Bin(v) => format!("{v}"),
            LitKind::Sized(_, v) => format!("{v}"),
            // Float literals are FP32 by default — emit the binary32 bit pattern
            // as an unsigned hex constant (matches the uint32_t carrier).
            LitKind::Float(bits) => format!("0x{:X}u", (f64::from_bits(*bits) as f32).to_bits()),
        },
        ExprKind::Bool(true) => "1".to_string(),
        ExprKind::Bool(false) => "0".to_string(),

        ExprKind::Ident(name) => {
            // Vec method predicate binder: `item` / `index` are rebound per
            // iteration by the enclosing `cpp_expr` Vec-method handler.
            if let Some(sub) = ctx.ident_subst.and_then(|m| m.get(name)) {
                return sub.clone();
            }
            // Static for-loop unroll binds the loop variable to a literal
            // integer (e.g. `chans[i].v` inside `for i in 0..N-1`).
            if let Some(v) = ctx
                .loop_var_subst
                .and_then(|c| c.borrow().get(name).copied())
            {
                return v.to_string();
            }
            if is_lhs {
                ctx.resolve_name(name, true)
            } else {
                ctx.read_signal(name)
            }
        }

        ExprKind::Binary(op, lhs, rhs) => {
            let l = cpp_expr(lhs, ctx);
            let r = cpp_expr(rhs, ctx);
            if *op == BinOp::Implies {
                return format!("(!{l} || {r})");
            }
            if *op == BinOp::ImpliesNext {
                // Sim shadow-reg lifting handles this at the assert site;
                // by the time it reaches expr lowering, lhs has been rewritten
                // into past-state. Treat as Implies for fallback paths.
                return format!("(!{l} || {r})");
            }
            // Floating-point operands: dispatch to the `_arch_fp.h` helpers
            // (IEEE-754 RNE) instead of integer operators on the bit pattern.
            if let Some(fmt) = infer_expr_float(lhs, ctx).or_else(|| infer_expr_float(rhs, ctx)) {
                let tag = fmt.helper_tag();
                let fop = match op {
                    BinOp::Add => Some("add"),
                    BinOp::Sub => Some("sub"),
                    BinOp::Mul => Some("mul"),
                    BinOp::Eq => Some("eq"),
                    BinOp::Neq => Some("ne"),
                    BinOp::Lt => Some("lt"),
                    BinOp::Gt => Some("gt"),
                    BinOp::Lte => Some("le"),
                    BinOp::Gte => Some("ge"),
                    _ => None,
                };
                if let Some(fop) = fop {
                    return format!("_arch_{tag}_{fop}({l}, {r})");
                }
            }
            if matches!(op, BinOp::Mul | BinOp::MulWrap) {
                // Native sim computes the product in a 128-bit intermediate
                // (`_arch_u128` / `__int128_t`). When the operation's own
                // result cannot fit in 128 bits the product is silently
                // truncated, so reject loudly instead.
                //   - plain `*`  : ARCH widens losslessly to W(lhs)+W(rhs);
                //                  the full product must fit in 128 bits.
                //   - `*%`       : result width = max(W(lhs), W(rhs)); only
                //                  unsupported when an operand itself exceeds
                //                  128 bits (a ≤128-bit modular result is
                //                  computed correctly — u128 holds its low
                //                  bits exactly).
                // `arch build` (SV) and `arch formal` (SMT) handle
                // arbitrary-width multiply correctly — only `arch sim` is
                // limited.
                let lw = infer_expr_width(lhs, ctx);
                let rw = infer_expr_width(rhs, ctx);
                let result_w = if *op == BinOp::MulWrap {
                    lw.max(rw)
                } else {
                    lw + rw
                };
                if result_w > 128 {
                    let opname = if *op == BinOp::MulWrap { "*%" } else { "*" };
                    eprintln!(
                        "error: native sim does not support `{opname}` whose result needs more than \
                         128 bits (this multiply needs {result_w} bits). The native C++ simulator \
                         computes products in a 128-bit integer; wider results are unsupported and \
                         would be silently truncated.\n  \
                         note: `arch build` (SystemVerilog) and `arch formal` (SMT-LIB2) handle \
                         this multiply correctly — only `arch sim` is affected.\n  \
                         help: keep the multiply's result within 128 bits (for a modular result \
                         use `*%`, e.g. `(a *% b).trunc<N>()`), or file an enhancement request for \
                         native-sim wide-multiply support: \
                         https://github.com/arch-hdl-lang/arch-com/issues"
                    );
                    std::process::exit(1);
                }
                let cast_ty = if infer_expr_signed(lhs, ctx) || infer_expr_signed(rhs, ctx) {
                    "__int128_t"
                } else {
                    "_arch_u128"
                };
                let product = format!("((({cast_ty})({l})) * (({cast_ty})({r})))");
                return if *op == BinOp::MulWrap {
                    let bits = infer_expr_width(expr, ctx);
                    if infer_expr_signed(expr, ctx) {
                        cast_to_signed_bits(&product, bits)
                    } else {
                        cast_to_bits(&product, bits)
                    }
                } else {
                    product
                };
            }
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
                BinOp::Shr => ">>",
                BinOp::Implies | BinOp::ImpliesNext => unreachable!(),
            };
            // Runtime divide-by-zero check for / and % when the divisor is
            // not a compile-time-reducible constant. Literal zero is already
            // rejected at typecheck; non-zero literals / param-folded consts
            // need no runtime check. Only truly-runtime divisors are wrapped.
            if matches!(op, BinOp::Div | BinOp::Mod) && !is_const_reducible(rhs) {
                let loc = base_ident_name(rhs).unwrap_or("<div>");
                let op_name = if *op == BinOp::Div { "/" } else { "%" };
                return format!("(_ARCH_DCHK(({r}), \"{loc} {op_name}\"), ({l} {op_str} {r}))");
            }
            format!("({l} {op_str} {r})")
        }

        ExprKind::Unary(op, operand) => cpp_unary(op, operand, ctx),

        ExprKind::FieldAccess(base, field) => {
            if let ExprKind::Ident(base_name) = &base.kind {
                // rst.asserted — polarity-abstracted reset active check
                if field.name == "asserted" {
                    if let Some(level) = ctx.reset_levels.get(base_name.as_str()) {
                        let resolved = ctx.resolve_name(base_name, false);
                        return if *level == ResetLevel::Low {
                            format!("(!{resolved})")
                        } else {
                            resolved
                        };
                    }
                }
                // Bus port: itcm.cmd_valid → itcm_cmd_valid
                if ctx.bus_ports.contains(base_name.as_str()) {
                    let flat = format!("{}_{}", base_name, field.name);
                    return ctx.resolve_name(&flat, is_lhs);
                }
                if ctx.inst_names.contains(base_name.as_str()) {
                    return format!("_inst_{}.{}", base_name, field.name);
                }
            }
            // Indexed bus port: m_axi[0].valid → m_axi_0_valid. The index
            // may be a literal or a loop variable bound to a literal via
            // static for-loop unroll (see `loop_var_subst`).
            if let ExprKind::Index(arr, idx) = &base.kind {
                if let ExprKind::Ident(arr_name) = &arr.kind {
                    let idx_val: Option<u64> = match &idx.kind {
                        ExprKind::Literal(LitKind::Dec(i))
                        | ExprKind::Literal(LitKind::Hex(i))
                        | ExprKind::Literal(LitKind::Bin(i))
                        | ExprKind::Literal(LitKind::Sized(_, i)) => Some(*i),
                        ExprKind::Ident(loopvar) => ctx
                            .loop_var_subst
                            .and_then(|c| c.borrow().get(loopvar).copied())
                            .map(|v| v as u64),
                        _ => None,
                    };
                    if let Some(i) = idx_val {
                        let expanded = format!("{}_{}", arr_name, i);
                        if ctx.bus_ports.contains(expanded.as_str()) {
                            return format!("{}_{}_{}", arr_name, i, field.name);
                        }
                    }
                    // Variable (non-constant) index into a Vec<Bus>.
                    //
                    // The constant path above resolves to a per-element flat
                    // field (`o_0_valid`). Those fields are reference aliases
                    // into a real C array (`o_valid[N]` for ports,
                    // `_let_o[N]` struct array for wires), so a runtime index
                    // selects the right lane directly — the same packed-array
                    // form the SV emitter uses (`o_valid[sel]`). Without this
                    // the FieldAccess fell through to the scalar bit-select
                    // path below, mis-lowering `o[sel].valid` to
                    // `((o >> sel) & 1).valid` against an undefined `o`.
                    // Bounds-checked like every other runtime index.
                    if idx_val.is_none() {
                        let fld = &field.name;
                        if let Some(n) = ctx
                            .vec_of_bus_port_count
                            .and_then(|m| m.get(arr_name).copied())
                        {
                            let i = cpp_expr(idx, ctx);
                            return format!(
                                "(_ARCH_BCHK(({i}), {n}, \"{arr_name}[i].{fld}\"), {arr_name}_{fld}[{i}])"
                            );
                        }
                        if let Some(n) = ctx
                            .vec_of_bus_wire_count
                            .and_then(|m| m.get(arr_name).copied())
                        {
                            let i = cpp_expr(idx, ctx);
                            return format!(
                                "(_ARCH_BCHK(({i}), {n}, \"{arr_name}[i].{fld}\"), _let_{arr_name}[{i}].{fld})"
                            );
                        }
                    }
                }
            }
            // Use is_lhs when evaluating base so struct reg fields get _n_ prefix on LHS
            let b = cpp_expr_inner(base, ctx, is_lhs);
            format!("{b}.{}", field.name)
        }

        ExprKind::MethodCall(base, method, args) => cpp_method_call(base, method, args, ctx),

        ExprKind::Cast(inner, ty) => {
            let e = cpp_expr(inner, ctx);
            let t = cpp_port_type_with_params(ty, ctx.params);
            // For SInt casts whose source is narrower than the target
            // C++ int, sign-extend the value: a plain `(int64_t)x`
            // bit-cast leaves the upper bits zero, which makes a
            // would-be-negative N-bit value (where N < 64) appear as
            // a large positive int64_t. Subsequent `>>` on that
            // mis-typed value zero-fills instead of sign-extending.
            //
            // Standard idiom: shift left by (W_int - W_HDL) so the
            // HDL sign bit lands at the int's MSB, then arith-shift
            // right by the same amount to sign-extend.
            //
            // For UInt casts and same-width SInt casts, the bit-cast
            // is correct and we keep the original simple form.
            if let TypeExpr::SInt(w) = &**ty {
                let w_hdl = eval_width_in(w, ctx);
                let w_cpp: u32 = if w_hdl <= 8 {
                    8
                } else if w_hdl <= 16 {
                    16
                } else if w_hdl <= 32 {
                    32
                } else if w_hdl <= 64 {
                    64
                } else {
                    0
                }; // >64: VlWide / _arch_u128 paths
                let inner_w = infer_expr_width(inner, ctx);
                if w_cpp > 0 && inner_w > 0 && inner_w < w_cpp {
                    let shift = w_cpp - inner_w;
                    return format!("(({t})({e}) << {shift}) >> {shift}");
                }
            }
            format!("({t})({e})")
        }

        ExprKind::Index(base, idx) => {
            let b = cpp_expr_inner(base, ctx, is_lhs);
            let i = cpp_expr(idx, ctx);
            // Vec-typed regs/fields use C array subscript; scalar signals use bit extraction
            let is_vec = ctx.expr_is_vec(base);
            // Runtime bounds check (hard abort) — skip when index is a compile-time literal
            // since the type checker handles constant-bounds at compile time.
            let idx_is_const = matches!(&idx.kind, ExprKind::Literal(_));
            if is_vec {
                let limit = ctx.expr_vec_size(base).unwrap_or(0);
                if limit > 0 && !idx_is_const {
                    let loc = ctx
                        .vec_path_of_expr(base)
                        .unwrap_or_else(|| "<vec>".to_string());
                    format!("(_ARCH_BCHK(({i}), {limit}, \"{loc}\"), {b}[{i}])")
                } else {
                    format!("{b}[{i}]")
                }
            } else {
                let base_w = infer_expr_width(base, ctx);
                if base_w > 0 && !idx_is_const {
                    let loc = base_ident_name(base).unwrap_or("<bitsel>");
                    format!("(_ARCH_BCHK(({i}), {base_w}, \"{loc}[i]\"), ((({b}) >> ({i})) & 1))")
                } else {
                    format!("((({b}) >> ({i})) & 1)")
                }
            }
        }

        ExprKind::BitSlice(base, hi, lo) => {
            let b = cpp_expr(base, ctx);
            let h = eval_width_in(hi, ctx);
            let l = eval_width_in(lo, ctx);
            let base_w = infer_expr_width(base, ctx);
            // Static slice: hi/lo are compile-time. Bounds checked by typecheck.
            if base_w > 128 {
                // VlWide<N>: use word-array bit extractor
                let result_w = h - l + 1;
                let result_ty = if result_w <= 64 {
                    cpp_uint(result_w)
                } else {
                    "uint64_t"
                };
                format!("({result_ty})_arch_vw_bits({b}.data(), {h}, {l})")
            } else if base_w > 64 {
                bit_range_u128(&b, h, l)
            } else {
                bit_range(&b, h, l)
            }
        }

        ExprKind::PartSelect(base, start, width, up) => {
            cpp_part_select(base, start, width, *up, ctx)
        }

        ExprKind::EnumVariant(enum_name, variant) => {
            if let Some(variants) = ctx.enum_map.get(&enum_name.name) {
                let idx = variants
                    .iter()
                    .find(|(n, _)| *n == variant.name)
                    .map(|(_, v)| *v)
                    .unwrap_or(0);
                format!("{idx}")
            } else {
                // Previously this silently emitted `0` with a C++ comment,
                // which masked genuine bugs (e.g. missing enum in enum_map).
                // Emit an undeclared identifier so the C++ compiler surfaces
                // the problem with a clear symbol to grep for, and warn at
                // codegen time so it isn't missed in the noise.
                eprintln!(
                    "warning: sim codegen: enum {}::{} not found in enum map; \
                     emitting compile-error token",
                    enum_name.name, variant.name
                );
                format!(
                    "_ARCH_CODEGEN_ERROR_unknown_enum_{}_{}",
                    enum_name.name, variant.name
                )
            }
        }

        ExprKind::StructLiteral(name, fields) => {
            // Lower to an immediately-invoked lambda so the result is a proper
            // value of the struct type. Works regardless of whether the struct
            // has a user-declared default constructor.
            let sname = &name.name;
            let mut body = String::new();
            body.push_str(&format!("[&](){{ {sname} _t; "));
            for f in fields {
                let v = cpp_expr(&f.value, ctx);
                body.push_str(&format!("_t.{} = {v}; ", f.name.name));
            }
            body.push_str("return _t; }()");
            body
        }

        ExprKind::Todo => {
            // Per the spec, `todo!` compiles but aborts at sim runtime. The
            // old lowering (`"0 /* todo! */"`) compiled AND silently ran,
            // turning a placeholder into real zero behavior. Now a
            // comma-expression that prints a diagnostic and calls abort()
            // before yielding 0, so any `todo!` reached in simulation fails
            // loudly. abort() is available via verilated.h (includes
            // <cstdlib>).
            "(fprintf(stderr, \"ARCH: todo! reached at sim runtime\\n\"), abort(), 0)".to_string()
        }

        ExprKind::Concat(parts) => cpp_concat(parts, ctx),

        ExprKind::Repeat(count, value) => {
            // {N{expr}} — replicate expr N times by shift-OR
            let c = cpp_expr(count, ctx);
            let v = cpp_expr(value, ctx);
            let val_width = infer_expr_width(value, ctx);
            // Generate: _arch_repeat(val, count, val_width)
            format!("_arch_repeat((uint64_t)({v}), {c}, {val_width})")
        }
        ExprKind::Clog2(arg) => {
            let a = cpp_expr(arg, ctx);
            format!("_arch_clog2({a})")
        }
        ExprKind::Onehot(index) => {
            let idx = cpp_expr(index, ctx);
            format!("(1ULL << {idx})")
        }
        ExprKind::Signed(inner) => {
            // signed(x) reinterprets `x`'s bit pattern as a two's-complement
            // signed value. The bit pattern is unchanged, but C++ operators
            // (notably `>>`) behave differently for signed types: signed
            // right-shift sign-extends, unsigned right-shift zero-extends.
            //
            // The cast target is the smallest C++ signed int that fits
            // the HDL width: SInt<8> → int8_t, SInt<33> → int64_t, etc.
            // When the HDL width is STRICTLY LESS than the C++ int width
            // (e.g. SInt<33> → int64_t with 31 padding bits), a plain
            // bit-cast leaves those upper bits zero, so a value that
            // should be negative in HDL terms (HDL bit W-1 = 1) appears
            // POSITIVE in the C++ int, and a chained `>>` zero-fills the
            // upper bits instead of sign-extending. Sign-extend explicitly
            // by left-shifting the HDL sign bit to the C++ MSB and then
            // arith-shifting back: `((int_W)x << (W_cpp-W_hdl)) >> (W_cpp-W_hdl)`.
            // arch-ibex `IbexAlu`'s SRA uses exactly this pattern via
            // `signed({sign_ext_msb, 32b}) >> shamt`.
            let w = infer_expr_width(inner, ctx);
            let inner_c = cpp_expr(inner, ctx);
            if w == 0 || w > 64 {
                inner_c
            } else {
                let w_cpp: u32 = if w <= 8 {
                    8
                } else if w <= 16 {
                    16
                } else if w <= 32 {
                    32
                } else {
                    64
                };
                if w < w_cpp {
                    let pad = w_cpp - w;
                    format!("((({})({}) << {pad}) >> {pad})", cpp_sint(w), inner_c)
                } else {
                    format!("(({})({}))", cpp_sint(w), inner_c)
                }
            }
        }
        ExprKind::Unsigned(inner) => {
            // unsigned(x) is the inverse cast. Emit explicit unsigned cast
            // so a chained `>>` becomes a logical shift. (For values that
            // started unsigned this is a no-op, but `unsigned(signed(x) >> n)`
            // patterns rely on the cast to bring the result back to uint
            // for further unsigned operations.)
            let w = infer_expr_width(inner, ctx);
            let inner_c = cpp_expr(inner, ctx);
            if w == 0 || w > 64 {
                inner_c
            } else {
                format!("(({})({}))", cpp_uint(w), inner_c)
            }
        }

        ExprKind::Ternary(cond, then_expr, else_expr) => {
            let c = cpp_expr(cond, ctx);
            let t = cpp_expr(then_expr, ctx);
            let e = cpp_expr(else_expr, ctx);
            format!("(({c}) ? ({t}) : ({e}))")
        }

        ExprKind::FunctionCall(name, args) if name == "fma" && args.len() == 3 => {
            let fmt = infer_expr_float(&args[0], ctx)
                .or_else(|| infer_expr_float(&args[1], ctx))
                .or_else(|| infer_expr_float(&args[2], ctx))
                .unwrap_or(FpFmt::Fp32);
            let a = cpp_expr(&args[0], ctx);
            let b = cpp_expr(&args[1], ctx);
            let c = cpp_expr(&args[2], ctx);
            format!("_arch_fma_{}({a}, {b}, {c})", fmt.helper_tag())
        }
        ExprKind::FunctionCall(name, args) if name == "is_nan" && args.len() == 1 => {
            let fmt = infer_expr_float(&args[0], ctx).unwrap_or(FpFmt::Fp32);
            let a = cpp_expr(&args[0], ctx);
            format!("_arch_{}_isnan({a})", fmt.helper_tag())
        }
        ExprKind::FunctionCall(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(|a| cpp_expr(a, ctx)).collect();
            format!("{name}({})", arg_strs.join(", "))
        }

        ExprKind::ExprMatch(scrutinee, arms) => {
            let s = cpp_expr(scrutinee, ctx);
            let mut result = "0".to_string();
            for arm in arms.iter().rev() {
                let val = cpp_expr(&arm.value, ctx);
                let cond = match &arm.pattern {
                    Pattern::Wildcard => {
                        result = val;
                        continue;
                    }
                    Pattern::Ident(id) => {
                        // Mirror Stmt::Match: if the ident names a let
                        // with a literal RHS, treat as `== <literal>`;
                        // else fall through as the ternary tail (default).
                        let folded = ctx
                            .let_values
                            .and_then(|m| m.get(&id.name))
                            .filter(|e| matches!(&e.kind, ExprKind::Literal(_)));
                        match folded {
                            Some(e) => {
                                let lit = cpp_expr(e, ctx);
                                format!("({s} == {lit})")
                            }
                            None => {
                                result = val;
                                continue;
                            }
                        }
                    }
                    Pattern::Literal(e) => {
                        let lit = cpp_expr(e, ctx);
                        format!("({s} == {lit})")
                    }
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants
                                .iter()
                                .find(|(n, _)| *n == vr.name)
                                .map(|(_, v)| *v)
                                .unwrap_or(0);
                            format!("({s} == {idx})")
                        } else {
                            format!("({s} == 0)")
                        }
                    }
                };
                result = format!("({cond} ? {val} : {result})");
            }
            result
        }

        ExprKind::Match(scrutinee, _) => {
            format!("/* match({}) */ 0", cpp_expr(scrutinee, ctx))
        }

        ExprKind::Inside(scrutinee, members) => {
            let s = cpp_expr(scrutinee, ctx);
            let parts: Vec<String> = members
                .iter()
                .map(|m| match m {
                    InsideMember::Single(e) => {
                        let v = cpp_expr(e, ctx);
                        format!("({s} == {v})")
                    }
                    InsideMember::Range(lo, hi) => {
                        let l = cpp_expr(lo, ctx);
                        let h = cpp_expr(hi, ctx);
                        format!("({s} >= {l} && {s} <= {h})")
                    }
                })
                .collect();
            if parts.is_empty() {
                "0".to_string()
            } else {
                format!("({})", parts.join(" || "))
            }
        }
    }
}

// ── Per-arm helpers for `cpp_expr_inner` ─────────────────────────────────────
// Big match arms extracted into private fns so the dispatch reads at a glance.
// No behavior change — each helper holds the original arm body verbatim.

fn cpp_unary(op: &UnaryOp, operand: &Expr, ctx: &Ctx) -> String {
    let o = cpp_expr(operand, ctx);
    match op {
        UnaryOp::Not => format!("(!{o})"),
        UnaryOp::BitNot => {
            // Use logical ! (clamped to 0/1) only for 1-bit/Bool signals.
            // For wider types use bitwise ~.
            let is_one_bit = match &operand.kind {
                ExprKind::Ident(name) => ctx.widths.get(name.as_str()).copied().unwrap_or(32) == 1,
                _ => false,
            };
            if is_one_bit {
                format!("(uint8_t)(!({o}))")
            } else {
                format!("(~({o}))")
            }
        }
        UnaryOp::Neg => format!("(-{o})"),
        UnaryOp::RedAnd => {
            // Reduction AND: all bits set → 1
            let w = infer_expr_width(operand, ctx);
            if w > 128 {
                let words = wide_words(w);
                let last_bits = w % 32;
                let last_mask = if last_bits == 0 {
                    "0xFFFFFFFFU".to_string()
                } else {
                    format!("0x{:X}U", (1u32 << last_bits) - 1)
                };
                format!("[&](){{auto& _v={o};for(int _i=0;_i<{}-1;_i++)if(_v._data[_i]!=0xFFFFFFFFU)return(uint8_t)0;return(uint8_t)(_v._data[{}]=={last_mask}?1:0);}}()", words, words-1)
            } else if w <= 1 {
                format!("({o} & 1)")
            } else {
                let mask = if w >= 64 { u64::MAX } else { (1u64 << w) - 1 };
                format!("(uint8_t)(({o} & 0x{mask:x}ULL) == 0x{mask:x}ULL)")
            }
        }
        UnaryOp::RedOr => {
            // Reduction OR: any bit set → 1
            let w = infer_expr_width(operand, ctx);
            if w > 128 {
                let words = wide_words(w);
                format!("[&](){{auto& _v={o};for(int _i=0;_i<{words};_i++)if(_v._data[_i])return(uint8_t)1;return(uint8_t)0;}}()")
            } else {
                format!("(uint8_t)(({o}) != 0)")
            }
        }
        UnaryOp::RedXor => {
            // Reduction XOR: parity
            let w = infer_expr_width(operand, ctx);
            if w > 128 {
                let words = wide_words(w);
                format!("[&](){{auto& _v={o};uint8_t _p=0;for(int _i=0;_i<{words};_i++)_p^=(uint8_t)__builtin_parity(_v._data[_i]);return _p;}}()")
            } else {
                format!("(uint8_t)(__builtin_parityll((uint64_t)({o})))")
            }
        }
    }
}

fn cpp_method_call(base: &Expr, method: &Ident, args: &[Expr], ctx: &Ctx) -> String {
    let b = cpp_expr(base, ctx);
    match method.name.as_str() {
        "trunc" => {
            if let Some(w_expr) = args.first() {
                let bits = eval_width_in(w_expr, ctx);
                let base_w = infer_expr_width(base, ctx);
                if base_w > 128 && bits <= 64 {
                    // VlWide → narrow: extract low bits via word array
                    format!(
                        "({})_arch_vw_bits({b}.data(), {}, 0)",
                        cpp_uint(bits),
                        bits - 1
                    )
                } else if infer_expr_signed(base, ctx) {
                    cast_to_signed_bits(&b, bits)
                } else {
                    cast_to_bits(&b, bits)
                }
            } else {
                b
            }
        }
        "zext" => {
            if let Some(w_expr) = args.first() {
                let bits = eval_width_in(w_expr, ctx);
                let base_w = infer_expr_width(base, ctx);
                if bits > 128 {
                    // Narrow → VlWide: use uint64_t constructor
                    let words = wide_words(bits);
                    format!("VlWide<{words}>(static_cast<uint64_t>({b}))")
                } else if base_w > 128 && bits <= 64 {
                    format!(
                        "({})_arch_vw_bits({b}.data(), {}, 0)",
                        cpp_uint(bits),
                        bits - 1
                    )
                } else {
                    format!("({})({})", cpp_uint(bits), b)
                }
            } else {
                b
            }
        }
        "sext" => {
            if let Some(w_expr) = args.first() {
                let dst_bits = eval_width_in(w_expr, ctx);
                let src_bits = infer_expr_width(base, ctx);
                if src_bits >= dst_bits || src_bits == 0 {
                    // No extension needed or unknown source width
                    format!("({})({})", cpp_uint(dst_bits), b)
                } else {
                    // Sign-extend: if MSB of source is set, fill upper bits with 1s
                    let dst_t = cpp_uint(dst_bits);
                    format!("(({b} >> {}) & 1 ? ({dst_t})({b}) | ({dst_t})(~(({dst_t})0) << {src_bits}) : ({dst_t})({b}))",
                        src_bits - 1)
                }
            } else {
                b
            }
        }
        "resize" => {
            // Direction-agnostic: sign-extend if narrowing to signed, zero-pad if widening unsigned
            if let Some(w_expr) = args.first() {
                let dst_bits = eval_width_in(w_expr, ctx);
                let src_bits = infer_expr_width(base, ctx);
                if src_bits >= dst_bits || src_bits == 0 {
                    // Narrowing or equal: just cast (C++ truncates)
                    cast_to_bits(&b, dst_bits)
                } else {
                    // Widening: zero-extend (same as zext for sim purposes)
                    format!("({})({})", cpp_uint(dst_bits), b)
                }
            } else {
                b
            }
        }
        "reverse" => {
            let base_w = infer_expr_width(base, ctx);
            let chunk = if let Some(c) = args.first() {
                eval_width_in(c, ctx)
            } else {
                1
            };
            if chunk == 1 {
                // Bit-reverse: build at compile time
                if base_w <= 64 {
                    format!("[&]() {{ {ty} v = {b}; {ty} r = 0; for (int i = 0; i < {w}; i++) r |= (({ty})((v >> i) & 1)) << ({w} - 1 - i); return r; }}()",
                        ty = cpp_uint(base_w), w = base_w)
                } else {
                    // Wide (>64 bit) reversal via VlWide
                    format!("[&]() {{ auto v = {b}; {ty} r{{}}; for (int i = 0; i < {w}; i++) {{ int sw = i / 32; int sb = i % 32; int dw = ({w} - 1 - i) / 32; int db = ({w} - 1 - i) % 32; if ((v[sw] >> sb) & 1) r[dw] |= (1u << db); }} return r; }}()",
                        ty = cpp_uint(base_w), w = base_w)
                }
            } else {
                // Chunk-reverse: reverse order of N-bit chunks
                let n_chunks = base_w / chunk;
                if base_w <= 64 {
                    format!("[&]() {{ {ty} v = {b}; {ty} r = 0; for (int i = 0; i < {nc}; i++) r |= ((v >> (i * {c})) & (({ty})((1ULL << {c}) - 1))) << (({nc} - 1 - i) * {c}); return r; }}()",
                        ty = cpp_uint(base_w), nc = n_chunks, c = chunk)
                } else {
                    // Wide chunk reverse — extract and place via bit loops
                    format!("[&]() {{ auto v = {b}; {ty} r{{}}; for (int ci = 0; ci < {nc}; ci++) for (int bi = 0; bi < {c}; bi++) {{ int si = ci * {c} + bi; int di = ({nc} - 1 - ci) * {c} + bi; int sw = si / 32; int sb = si % 32; int dw = di / 32; int db = di % 32; if ((v[sw] >> sb) & 1) r[dw] |= (1u << db); }} return r; }}()",
                        ty = cpp_uint(base_w), nc = n_chunks, c = chunk)
                }
            }
        }
        "any" | "all" | "count" | "contains" | "reduce_or" | "reduce_and" | "reduce_xor" => {
            lower_vec_method_cpp(&b, base, method, args, ctx)
        }
        // Float conversions → `_arch_fp.h` helpers.
        "to_fp32" => match infer_expr_float(base, ctx) {
            Some(FpFmt::Bf16) => format!("_arch_bf16_to_f32({b})"),
            Some(FpFmt::Fp32) => b, // no-op (typecheck rejects, but stay total)
            None => {
                if infer_expr_signed(base, ctx) {
                    format!("_arch_i_to_f32((int64_t)({b}))")
                } else {
                    format!("_arch_u_to_f32((uint64_t)({b}))")
                }
            }
        },
        "to_bf16" => match infer_expr_float(base, ctx) {
            Some(FpFmt::Fp32) => format!("_arch_f32_to_bf16({b})"),
            Some(FpFmt::Bf16) => b,
            None => {
                if infer_expr_signed(base, ctx) {
                    format!("_arch_i_to_bf16((int64_t)({b}))")
                } else {
                    format!("_arch_u_to_bf16((uint64_t)({b}))")
                }
            }
        },
        "to_uint" | "to_sint" => {
            let bits = args.first().map(|w| eval_width_in(w, ctx)).unwrap_or(32);
            let signed = method.name == "to_sint";
            // Decode bf16 to f32 bits first; then a width-aware, saturating,
            // toward-zero, NaN→type-max conversion to the N-bit integer.
            let f32bits = match infer_expr_float(base, ctx) {
                Some(FpFmt::Bf16) => format!("_arch_bf16_to_f32({b})"),
                _ => b,
            };
            let conv = if signed {
                format!("_arch_f32_to_sint({f32bits}, {bits})")
            } else {
                format!("_arch_f32_to_uint({f32bits}, {bits})")
            };
            let cast = if signed {
                cpp_sint(bits)
            } else {
                cpp_uint(bits)
            };
            format!("(({cast})({conv}))")
        }
        _ => format!("{b}.{}()", method.name),
    }
}

fn cpp_part_select(base: &Expr, start: &Expr, width: &Expr, up: bool, ctx: &Ctx) -> String {
    let b = cpp_expr(base, ctx);
    let s = cpp_expr(start, ctx);
    let w = eval_width_in(width, ctx);
    let base_w = infer_expr_width(base, ctx);
    let result_ty = cpp_uint(w);
    // Runtime bounds check for variable part-selects:
    //   [+:]: bits [start .. start+W-1] must fit, so (start + W - 1) < base_W
    //   [-:]: bits [start-W+1 .. start], so start < base_W and start >= W-1
    // Skip when start is a constant.
    let start_is_const = matches!(&start.kind, ExprKind::Literal(_));
    let bchk = if base_w > 0 && !start_is_const {
        let loc = base_ident_name(base).unwrap_or("<partsel>");
        if up {
            format!("_ARCH_BCHK((({s}) + {w} - 1), {base_w}, \"{loc}[+:{w}]\"), ")
        } else {
            // [-:W]: need start < base_W AND start >= W-1.
            // Check (start + 1 - W) as signed → unsigned wrap makes this >= base_W if invalid.
            format!("_ARCH_BCHK(({s}), {base_w}, \"{loc}[-:{w}] start\"), _ARCH_BCHK(({w} - 1), (({s}) + 1), \"{loc}[-:{w}] underflow\"), ")
        }
    } else {
        String::new()
    };
    let core = if base_w > 128 {
        // VlWide<N>: use _arch_vw_bits with runtime start
        let hi_expr = if up {
            format!("(({s}) + {w} - 1)")
        } else {
            format!("({s})")
        };
        let lo_expr = if up {
            format!("({s})")
        } else {
            format!("(({s}) - {} + 1)", w)
        };
        format!("({result_ty})_arch_vw_bits({b}.data(), {hi_expr}, {lo_expr})")
    } else if base_w > 64 {
        let mask = (1u128 << w).wrapping_sub(1);
        let mask_str = format!("0x{:x}ULL", mask as u64);
        if up {
            format!("({result_ty})(({b} >> ({s})) & {mask_str})")
        } else {
            format!("({result_ty})(({b} >> (({s}) - {} + 1)) & {mask_str})", w)
        }
    } else {
        let mask = if w >= 64 { u64::MAX } else { (1u64 << w) - 1 };
        let mask_str = format!("0x{:x}ULL", mask);
        if up {
            format!("({result_ty})((uint64_t)({b}) >> ({s}) & {mask_str})")
        } else {
            format!(
                "({result_ty})((uint64_t)({b}) >> (({s}) - {} + 1) & {mask_str})",
                w
            )
        }
    };
    if bchk.is_empty() {
        core
    } else {
        format!("({bchk}{core})")
    }
}

fn cpp_concat(parts: &[Expr], ctx: &Ctx) -> String {
    if parts.is_empty() {
        return "0".to_string();
    }
    // Compute widths for each part (MSB first)
    let part_widths: Vec<u32> = parts.iter().map(|p| infer_expr_width(p, ctx)).collect();
    let total: u32 = part_widths.iter().sum();

    if total > 128 {
        // Result is a VlWide<N>: build via OR-shifted parts in a lambda
        let words = wide_words(total);
        let mut stmts = Vec::new();
        let mut bit_offset = 0u32;
        for (i, part) in parts.iter().enumerate().rev() {
            let w = part_widths[i];
            let val = cpp_expr(part, ctx);
            // Each part is cast to uint64_t (narrow) then placed into VlWide
            stmts.push(format!(
                "_r = _r | (VlWide<{words}>(static_cast<uint64_t>({val})) << {bit_offset});"
            ));
            bit_offset += w;
        }
        format!(
            "[&]() -> VlWide<{words}> {{ VlWide<{words}> _r{{}}; {} return _r; }}()",
            stmts.join(" ")
        )
    } else {
        // Build expression: accumulate shifts from LSB (last part offset=0)
        let mut terms = Vec::new();
        let mut bit_offset = 0u32;
        for (i, part) in parts.iter().enumerate().rev() {
            let w = part_widths[i];
            let val = cpp_expr(part, ctx);
            if total > 64 {
                terms.push(format!("((_arch_u128)(uint64_t)({val}) << {bit_offset})"));
            } else {
                terms.push(format!("((uint64_t)({val}) << {bit_offset})"));
            }
            bit_offset += w;
        }
        format!("({})", terms.join(" | "))
    }
}

// ── Statement emitters ────────────────────────────────────────────────────────

fn ind(n: usize) -> String {
    "  ".repeat(n)
}

/// Sim-codegen analog of `codegen::AssignCtx`. Phase 5b part 4 — drives
/// the unified `emit_stmt` walker so seq vs comb stmt emission shares
/// one source of truth. The flag affects:
/// - **LHS resolution**: `Seq` resolves to the next-cycle shadow
///   `_n_{name}` (committed at end of cycle); `Comb` resolves to the
///   live `_{name}` (visible immediately).
/// - **Wide-output-port conversion**: only `Comb` paths apply
///   `_arch_u128_to_vl` for 65–128b output ports (>128b is a direct
///   `VlWide<N>` assignment); `Seq` writes go through `cpp_expr_lhs`
///   which handles the shadow naming uniformly.
/// - **Init / WaitUntil / DoUntil legality**: `Seq` only.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum SimAssignKind {
    Seq,
    Comb,
}

fn emit_stmts(stmts: &[Stmt], ctx: &Ctx, out: &mut String, indent: usize, k: SimAssignKind) {
    for stmt in stmts {
        emit_stmt(stmt, ctx, out, indent, k);
    }
}

fn emit_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize, k: SimAssignKind) {
    let is_seq = k == SimAssignKind::Seq;
    match stmt {
        Stmt::Assign(a) => {
            // Whole-Vec assignment: C arrays are not assignable in C++, so
            // lower `dst <= src_vec;` / `dst = src_vec;` to an element copy.
            // This is hit by TLM Vec payloads, e.g. `data <= m.read4(...)`
            // after TLM lowering becomes `data <= m_read4_rsp_data`.
            let vec_name_of_expr = |e: &Expr| -> Option<String> {
                match &e.kind {
                    ExprKind::Ident(name) => Some(name.clone()),
                    ExprKind::FieldAccess(base, field) => {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            if ctx.bus_ports.contains(base_name.as_str()) {
                                Some(format!("{}_{}", base_name, field.name))
                            } else {
                                Some(format!("{}.{}", base_name, field.name))
                            }
                        } else {
                            ctx.vec_path_of_expr(e)
                        }
                    }
                    _ => None,
                }
            };
            if let Some(dst_name) = vec_name_of_expr(&a.target) {
                if ctx
                    .vec_names
                    .map_or(false, |s| s.contains(dst_name.as_str()))
                {
                    let lhs = cpp_expr_lhs(&a.target, ctx);
                    let rhs = cpp_expr(&a.value, ctx);
                    let count = ctx
                        .vec_sizes
                        .and_then(|m| m.get(dst_name.as_str()).copied())
                        .unwrap_or(0);
                    if rhs == "0" {
                        out.push_str(&format!(
                            "{}memset({lhs}, 0, sizeof({lhs}));\n",
                            ind(indent)
                        ));
                        return;
                    }
                    if let Some(rhs_name) = vec_name_of_expr(&a.value) {
                        if ctx
                            .vec_names
                            .map_or(false, |s| s.contains(rhs_name.as_str()))
                        {
                            if count > 0 {
                                out.push_str(&format!(
                                    "{}for (size_t _i = 0; _i < {count}; ++_i) {{ {lhs}[_i] = {rhs}[_i]; }}\n",
                                    ind(indent)
                                ));
                                return;
                            }
                        }
                    }
                }
            }
            // Scalar bit-indexed LHS: name[idx] = val where name is NOT a Vec.
            // Emit mask-and-OR: base = (base & ~(1ULL << idx)) | (uint64_t(val & 1) << idx).
            // resolve_name's is_lhs flag = is_seq → seq writes hit the shadow.
            if let ExprKind::Index(base, idx_expr) = &a.target.kind {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if !ctx
                        .vec_names
                        .map_or(false, |s| s.contains(base_name.as_str()))
                    {
                        let resolved_base = ctx.resolve_name(base_name, is_seq);
                        let idx_cpp = cpp_expr(idx_expr, ctx);
                        let rhs = cpp_expr(&a.value, ctx);
                        out.push_str(&format!(
                            "{}{resolved_base} = ({resolved_base} & ~(uint64_t(1) << ({idx_cpp}))) | (uint64_t(({rhs}) & 1) << ({idx_cpp}));\n",
                            ind(indent)
                        ));
                        return;
                    }
                }
            }
            // Bit-slice LHS: name[hi:lo] = val. Lower to mask-and-OR rather
            // than the read-side `((name >> lo) & mask)` (an rvalue — gcc /
            // clang reject as "expression is not assignable"). Only the
            // bare-ident base case `name[hi:lo]` is handled here; Vec-element
            // slice LHS or wider-than-64 bases keep the generic path and
            // would need their own arms.
            if let ExprKind::BitSlice(base, hi_e, lo_e) = &a.target.kind {
                if let ExprKind::Ident(base_name) = &base.kind {
                    let base_w = infer_expr_width(base, ctx);
                    if base_w > 0 && base_w <= 64 {
                        let resolved_base = ctx.resolve_name(base_name, is_seq);
                        let hi = eval_width_in(hi_e, ctx);
                        let lo = eval_width_in(lo_e, ctx);
                        let width = hi - lo + 1;
                        let val_mask: u64 = if width >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << width) - 1
                        };
                        let rhs = cpp_expr(&a.value, ctx);
                        out.push_str(&format!(
                            "{}{resolved_base} = ({resolved_base} & ~(uint64_t(0x{val_mask:X}ULL) << {lo})) | ((uint64_t(({rhs}) & 0x{val_mask:X}ULL)) << {lo});\n",
                            ind(indent)
                        ));
                        return;
                    }
                }
            }
            let rhs = cpp_expr(&a.value, ctx);
            if is_seq {
                let lhs = cpp_expr_lhs(&a.target, ctx);
                out.push_str(&format!("{}{}  = {};\n", ind(indent), lhs, rhs));
            } else {
                // Comb: bare-ident-aware target name + wide-output-port conversion.
                let target_name = if let ExprKind::Ident(name) = &a.target.kind {
                    name.clone()
                } else {
                    cpp_expr(&a.target, ctx)
                };
                let resolved_target = ctx.resolve_name(&target_name, false);
                if ctx.wide_names.contains(target_name.as_str()) {
                    let bits = ctx.widths.get(target_name.as_str()).copied().unwrap_or(0);
                    if bits > 128 {
                        // >128 bits: both internal and port are VlWide<N> — direct assign.
                        out.push_str(&format!("{}{} = {};\n", ind(indent), target_name, rhs));
                    } else {
                        // 65–128 bits: internal is _arch_u128, port is
                        // VlWide<ceil(W/32)>. Pass the real word count so a
                        // VlWide<3> (66–96 bit) port is not written out of
                        // bounds (which clobbers the adjacent struct member).
                        out.push_str(&format!(
                            "{}  _arch_u128_to_vl({}, {}._data, {});\n",
                            ind(indent),
                            rhs,
                            target_name,
                            wide_words(bits)
                        ));
                    }
                } else {
                    out.push_str(&format!("{}{}  = {};\n", ind(indent), resolved_target, rhs));
                }
            }
        }
        Stmt::IfElse(ie) => emit_if_else(ie, ctx, out, indent, false, k),
        Stmt::Match(m) => {
            let scrut = cpp_expr(&m.scrutinee, ctx);
            out.push_str(&format!("{}switch ({}) {{\n", ind(indent), scrut));
            for arm in &m.arms {
                let (case_str, label) = match &arm.pattern {
                    Pattern::Wildcard => ("default".to_string(), "match _".to_string()),
                    Pattern::Ident(id) => {
                        // If `id` names a module-scope let-binding with a
                        // literal RHS, emit `case <literal>:` so multiple
                        // ident arms (e.g. `ALU_ADD`, `ALU_SUB`, ...) don't
                        // collapse into "multiple default labels". Falls
                        // back to `default` when the let is missing or its
                        // RHS isn't a constant — preserves wildcard-binding
                        // semantics for non-let idents.
                        let folded = ctx
                            .let_values
                            .and_then(|m| m.get(&id.name))
                            .filter(|e| matches!(&e.kind, ExprKind::Literal(_)));
                        match folded {
                            Some(e) => (
                                format!("case {}", cpp_expr(e, ctx)),
                                format!("match {}", id.name),
                            ),
                            None => ("default".to_string(), format!("match {}", id.name)),
                        }
                    }
                    Pattern::Literal(e) => (
                        format!("case {}", cpp_expr(e, ctx)),
                        "match lit".to_string(),
                    ),
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants
                                .iter()
                                .find(|(n, _)| *n == vr.name)
                                .map(|(_, v)| *v)
                                .unwrap_or(0);
                            (
                                format!("case {idx}"),
                                format!("match {}::{}", en.name, vr.name),
                            )
                        } else {
                            (
                                "default".to_string(),
                                format!("match {}::{}", en.name, vr.name),
                            )
                        }
                    }
                };
                out.push_str(&format!("{}{}: {{\n", ind(indent + 1), case_str));
                // --coverage: per match-arm counter. Use the match's
                // span.start so the report points to the match statement
                // (per-arm spans aren't tracked on MatchArm); the label
                // disambiguates which arm.
                if let Some(reg) = ctx.coverage {
                    let kind = if matches!(arm.pattern, Pattern::Wildcard | Pattern::Ident(_)) {
                        "match-default"
                    } else {
                        "match-arm"
                    };
                    let cidx = reg.borrow_mut().alloc(kind, m.span.start, label);
                    out.push_str(&format!("{}  _arch_cov[{cidx}]++;\n", ind(indent + 1)));
                }
                // Arm body: full recurse via the unified emitter — this is
                // the bug fix. Pre-collapse, the comb walker silently
                // emitted only `Stmt::Assign` arms and dropped nested
                // `if/else`, `match`, `for`, and `log` inside arm bodies.
                emit_stmts(&arm.body, ctx, out, indent + 2, k);
                out.push_str(&format!("{}  break;\n", ind(indent + 1)));
                out.push_str(&format!("{}}}\n", ind(indent + 1)));
            }
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        Stmt::Log(l) => emit_log_stmt(l, ctx, out, indent),
        Stmt::For(f) => {
            let var = &f.var.name;
            // Static unrolling for Vec-of-bus indexed access (mirror of the
            // SV-side path). The C++ struct fields are flat per-element
            // (`chans_0_v`, `chans_1_v`, ...), not arrays, so a behavioral
            // `for (int i ...; i <= N; i++) chans[i].v = ...` would emit a
            // reference to an undeclared `chans`. Detect Vec-of-bus indexed
            // writes by the loop variable, and if found AND bounds are
            // literal, statically unroll: bind the loop var to each
            // iteration value via `loop_var_subst` and emit the body N
            // times.
            if let (ForRange::Range(rs, re), Some(subst), Some(vob_ports), Some(vob_wires)) = (
                &f.range,
                ctx.loop_var_subst,
                ctx.vec_of_bus_port_count,
                ctx.vec_of_bus_wire_count,
            ) {
                // Param-driven bounds (e.g. `for i in 0..NUM-1`) fold against
                // the module's params so the unroll fires on `Vec<Bus, NUM>`
                // with a param-driven N. `eval_const_expr_with_params`
                // returns 0 for anything it can't fold; we still need a
                // signal that the bound was foldable, so guard literal-zero
                // by requiring `start <= end` AND the body actually touches
                // a Vec-of-bus.
                let folds_to = |e: &Expr| -> Option<u32> {
                    let v = eval_const_expr_with_params(e, ctx.params) as u32;
                    // Any expression that wasn't a literal-zero in disguise
                    // and that the body actually depends on counts as
                    // foldable; in practice the body-touch predicate below
                    // guards against false positives.
                    Some(v)
                };
                if let (Some(start_lit), Some(end_lit)) = (folds_to(rs), folds_to(re)) {
                    let touches = f
                        .body
                        .iter()
                        .any(|s| stmt_indexes_vob_with_var(s, var, vob_ports, vob_wires));
                    if touches {
                        for i in start_lit..=end_lit {
                            subst.borrow_mut().insert(var.clone(), i);
                            for s in &f.body {
                                emit_stmt(s, ctx, out, indent, k);
                            }
                        }
                        subst.borrow_mut().remove(var);
                        return;
                    }
                }
            }
            match &f.range {
                ForRange::Range(rs, re) => {
                    let start = cpp_expr(rs, ctx);
                    let end = cpp_expr(re, ctx);
                    out.push_str(&format!(
                        "{}for (int {var} = {start}; {var} <= {end}; {var}++) {{\n",
                        ind(indent)
                    ));
                    for s in &f.body {
                        emit_stmt(s, ctx, out, indent + 1, k);
                    }
                    out.push_str(&format!("{}}}\n", ind(indent)));
                }
                ForRange::ValueList(vals) => {
                    for v in vals {
                        let val = cpp_expr(v, ctx);
                        out.push_str(&format!("{}{{\n", ind(indent)));
                        out.push_str(&format!("{}int {var} = {val};\n", ind(indent + 1)));
                        for s in &f.body {
                            emit_stmt(s, ctx, out, indent + 1, k);
                        }
                        out.push_str(&format!("{}}}\n", ind(indent)));
                    }
                }
            }
        }
        Stmt::Init(ib) => {
            if !is_seq {
                unreachable!("Stmt::Init reached emit_stmt(Comb) — typecheck bug");
            }
            let rst_name = &ib.reset_signal.name;
            let is_low = ctx
                .reset_levels
                .get(rst_name.as_str())
                .map_or(false, |level| *level == ResetLevel::Low);
            let cond = if is_low {
                format!("(!{})", rst_name)
            } else {
                rst_name.clone()
            };
            out.push_str(&format!("{}if ({}) {{\n", ind(indent), cond));
            emit_stmts(&ib.body, ctx, out, indent + 1, k);
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => {
            if !is_seq {
                unreachable!("Stmt::WaitUntil/DoUntil reached emit_stmt(Comb) — typecheck bug");
            }
            // Pipeline wait-stage seq blocks are emitted by `gen_pipeline`;
            // the generic module stmt walker should never lower them.
            unreachable!("Stmt::WaitUntil/DoUntil reached generic sim stmt emitter");
        }
    }
}

fn emit_if_else(
    ie: &IfElse,
    ctx: &Ctx,
    out: &mut String,
    indent: usize,
    is_chain: bool,
    k: SimAssignKind,
) {
    let cond = cpp_condition(&ie.cond, ctx);
    if is_chain {
        out.push_str(&format!("{}}} else if {} {{\n", ind(indent), cond));
    } else {
        out.push_str(&format!("{}if {} {{\n", ind(indent), cond));
    }
    // --coverage: count entries to this arm. Phase 1 records branch
    // coverage for seq if/elsif/else; phase 1b/c adds comb. Counter id
    // is the alloc order in the per-class registry.
    //
    // Note: comb blocks may evaluate multiple times per cycle during
    // the settle loop — counters therefore reflect "branch entries",
    // not "cycles where branch was active". For most designs the settle
    // loop converges in 1–2 iterations so this is close to cycle count.
    if let Some(reg) = ctx.coverage {
        let kind = if is_chain { "elsif" } else { "if" };
        let idx = reg
            .borrow_mut()
            .alloc(kind, ie.cond.span.start, String::new());
        out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
    }
    emit_stmts(&ie.then_stmts, ctx, out, indent + 1, k);
    if ie.else_stmts.len() == 1 {
        if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
            emit_if_else(nested, ctx, out, indent, true, k);
            return;
        }
    }
    if !ie.else_stmts.is_empty() {
        out.push_str(&format!("{}}} else {{\n", ind(indent)));
        if let Some(reg) = ctx.coverage {
            let idx = reg.borrow_mut().alloc("else", ie.span.end, String::new());
            out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
        }
        emit_stmts(&ie.else_stmts, ctx, out, indent + 1, k);
    }
    out.push_str(&format!("{}}}\n", ind(indent)));
}

// ── Thin compatibility wrappers over `emit_stmt` / `emit_stmts` /
// `emit_if_else`. Kept so call sites read semantically (`emit_reg_*`
// for seq, `emit_comb_*` for comb).

fn emit_reg_stmts(stmts: &[Stmt], ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmts(stmts, ctx, out, indent, SimAssignKind::Seq);
}

fn emit_reg_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmt(stmt, ctx, out, indent, SimAssignKind::Seq);
}

#[allow(dead_code)]
fn emit_reg_if_else(ie: &IfElse, ctx: &Ctx, out: &mut String, indent: usize, is_chain: bool) {
    emit_if_else(ie, ctx, out, indent, is_chain, SimAssignKind::Seq);
}

fn emit_comb_stmts(stmts: &[Stmt], ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmts(stmts, ctx, out, indent, SimAssignKind::Comb);
}

#[allow(dead_code)]
fn emit_comb_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmt(stmt, ctx, out, indent, SimAssignKind::Comb);
}

#[allow(dead_code)]
fn emit_comb_if_else(ie: &IfElse, ctx: &Ctx, out: &mut String, indent: usize, is_chain: bool) {
    emit_if_else(ie, ctx, out, indent, is_chain, SimAssignKind::Comb);
}

fn emit_log_stmt(l: &LogStmt, ctx: &Ctx, out: &mut String, indent: usize) {
    let args_str: String = l
        .args
        .iter()
        .map(|a| format!(", (long long)({})", cpp_expr(a, ctx)))
        .collect();
    let fmt = sv_fmt_to_printf(&l.fmt);
    let print_line = if let Some(ref path) = l.file {
        let fd_name = log_fd_name(path);
        format!(
            "{}if ({fd_name}) fprintf({fd_name}, \"[{}][{}] {}\\n\"{});",
            ind(indent),
            l.level.name(),
            l.tag,
            fmt,
            args_str
        )
    } else {
        format!(
            "{}printf(\"[{}][{}] {}\\n\"{});",
            ind(indent),
            l.level.name(),
            l.tag,
            fmt,
            args_str
        )
    };
    if l.level == LogLevel::Always {
        out.push_str(&print_line);
        out.push('\n');
    } else {
        out.push_str(&format!(
            "{}if (Verilated::verbosity() >= {}) {{ {} }}\n",
            ind(indent),
            l.level.value(),
            print_line
        ));
    }
}

/// Generate a C++ file pointer name from a log file path.
fn log_fd_name(path: &str) -> String {
    let clean: String = path
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("_log_fd_{clean}")
}

/// Collect unique log file paths from module body (comb + seq blocks).
fn collect_log_files(body: &[ModuleBodyItem]) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    fn from_comb(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut HashSet<String>) {
        for s in stmts {
            match s {
                Stmt::Log(l) => {
                    if let Some(ref p) = l.file {
                        if seen.insert(p.clone()) {
                            files.push(p.clone());
                        }
                    }
                }
                Stmt::IfElse(ie) => {
                    from_comb(&ie.then_stmts, files, seen);
                    from_comb(&ie.else_stmts, files, seen);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        from_comb(&arm.body, files, seen);
                    }
                }
                Stmt::For(f) => from_comb(&f.body, files, seen),
                _ => {}
            }
        }
    }
    fn from_seq(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut HashSet<String>) {
        for s in stmts {
            match s {
                Stmt::Log(l) => {
                    if let Some(ref p) = l.file {
                        if seen.insert(p.clone()) {
                            files.push(p.clone());
                        }
                    }
                }
                Stmt::IfElse(ie) => {
                    from_seq(&ie.then_stmts, files, seen);
                    from_seq(&ie.else_stmts, files, seen);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        from_seq(&arm.body, files, seen);
                    }
                }
                _ => {}
            }
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::CombBlock(cb) => from_comb(&cb.stmts, &mut files, &mut seen),
            ModuleBodyItem::RegBlock(rb) => from_seq(&rb.stmts, &mut files, &mut seen),
            _ => {}
        }
    }
    files
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn collect_reg_names(body: &[ModuleBodyItem], ports: &[PortDecl]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| {
            if let ModuleBodyItem::RegDecl(r) = i {
                Some(r.name.name.clone())
            } else {
                None
            }
        })
        .chain(ports.iter().filter_map(|p| {
            if p.reg_info.is_some() {
                Some(p.name.name.clone())
            } else {
                None
            }
        }))
        .collect()
}

fn collect_port_reg_names(ports: &[PortDecl]) -> HashSet<String> {
    ports
        .iter()
        .filter_map(|p| {
            if p.reg_info.is_some() {
                Some(p.name.name.clone())
            } else {
                None
            }
        })
        .collect()
}

fn emit_port_reg_public_copy(
    cpp: &mut String,
    name: &str,
    widths: &HashMap<String, u32>,
    vec_count: Option<u64>,
    indent: &str,
) {
    if let Some(count) = vec_count {
        for i in 0..count {
            cpp.push_str(&format!("{indent}{name}_{i} = _{name}[{i}];\n"));
        }
        return;
    }

    let bits = widths.get(name).copied().unwrap_or(0);
    if bits > 64 && bits <= 128 {
        cpp.push_str(&format!(
            "{indent}_arch_u128_to_vl(_{name}, {name}._data, {});\n",
            wide_words(bits)
        ));
    } else {
        cpp.push_str(&format!("{indent}{name} = _{name};\n"));
    }
}

fn collect_let_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    for i in body {
        match i {
            ModuleBodyItem::LetBinding(l) => {
                // Destructuring: each bound field becomes a _let_ field.
                if !l.destructure_fields.is_empty() {
                    for bind in &l.destructure_fields {
                        out.insert(bind.name.clone());
                    }
                } else {
                    out.insert(l.name.name.clone());
                }
            }
            ModuleBodyItem::WireDecl(w) => {
                out.insert(w.name.name.clone());
            }
            _ => {}
        }
    }
    out
}

/// Map module-scope `let NAME: T = expr;` bindings to their RHS expr.
/// Used by `Stmt::Match` codegen to fold `Pattern::Ident(NAME)` arms
/// into `case <literal>:` labels (instead of the buggy `default:`
/// fall-through that collapses multi-let-bound match arms — see
/// memory/feedback_archsim_match_pattern_ident_default_collision.md).
/// Destructure-let bindings (`let {a, b} = ...;`) are skipped — those
/// don't have a single RHS and aren't referenceable from match patterns.
fn collect_let_values(body: &[ModuleBodyItem], params: &[ParamDecl]) -> HashMap<String, Expr> {
    let mut out = HashMap::new();
    for item in body {
        if let ModuleBodyItem::LetBinding(l) = item {
            if l.destructure_fields.is_empty() {
                out.insert(l.name.name.clone(), l.value.clone());
            }
        }
    }
    // Compile-time-constant params (`param X: const = N`, `param X[hi:lo]: const = N`,
    // `local param X: T = N`) participate in the same fold so `unique match` arms
    // whose LHS names a param resolve to `case <literal>:` rather than collapsing to
    // `default:`. Required for operator-decoder-style match blocks.
    for p in params {
        if let Some(expr) = &p.default {
            if matches!(&expr.kind, ExprKind::Literal(_)) {
                out.insert(p.name.name.clone(), expr.clone());
            }
        }
    }
    out
}

fn collect_pipe_reg_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut s = HashSet::new();
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            for i in 0..p.stages {
                if i == p.stages - 1 {
                    s.insert(p.name.name.clone());
                } else {
                    s.insert(format!("{}_stg{}", p.name.name, i + 1));
                }
            }
        }
    }
    s
}

/// Collect all identifiers read in a comb statement (RHS of assignments).
fn collect_comb_reads(stmt: &Stmt, out: &mut std::collections::BTreeSet<String>) {
    match stmt {
        Stmt::Assign(a) => collect_expr_idents(&a.value, out),
        Stmt::IfElse(ie) => {
            collect_expr_idents(&ie.cond, out);
            for s in &ie.then_stmts {
                collect_comb_reads(s, out);
            }
            for s in &ie.else_stmts {
                collect_comb_reads(s, out);
            }
        }
        Stmt::Log(_) => {}
        Stmt::Match(m) => {
            collect_expr_idents(&m.scrutinee, out);
            for arm in &m.arms {
                for s in &arm.body {
                    collect_comb_reads(s, out);
                }
            }
        }
        Stmt::For(f) => {
            for s in &f.body {
                collect_comb_reads(s, out);
            }
        }
        Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
            unreachable!("seq-only Stmt variant inside comb-context walker")
        }
    }
}

/// Collect all identifiers read in a seq (or init) statement — RHS of assignments,
/// branch conditions, loop bounds, wait predicates. Used by `--inputs-start-uninit`.
fn collect_stmt_idents(stmt: &Stmt, out: &mut std::collections::BTreeSet<String>) {
    match stmt {
        Stmt::Assign(a) => collect_expr_idents(&a.value, out),
        Stmt::IfElse(ie) => {
            collect_expr_idents(&ie.cond, out);
            for s in &ie.then_stmts {
                collect_stmt_idents(s, out);
            }
            for s in &ie.else_stmts {
                collect_stmt_idents(s, out);
            }
        }
        Stmt::Match(m) => {
            collect_expr_idents(&m.scrutinee, out);
            for arm in &m.arms {
                for s in &arm.body {
                    collect_stmt_idents(s, out);
                }
            }
        }
        Stmt::For(f) => {
            if let ForRange::Range(lo, hi) = &f.range {
                collect_expr_idents(lo, out);
                collect_expr_idents(hi, out);
            } else if let ForRange::ValueList(vs) = &f.range {
                for v in vs {
                    collect_expr_idents(v, out);
                }
            }
            for s in &f.body {
                collect_stmt_idents(s, out);
            }
        }
        Stmt::Init(ib) => {
            for s in &ib.body {
                collect_stmt_idents(s, out);
            }
        }
        Stmt::WaitUntil(e, _) => collect_expr_idents(e, out),
        Stmt::DoUntil { body, cond, .. } => {
            for s in body {
                collect_stmt_idents(s, out);
            }
            collect_expr_idents(cond, out);
        }
        Stmt::Log(_) => {}
    }
}

fn collect_expr_idents(expr: &Expr, out: &mut std::collections::BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            out.insert(name.clone());
        }
        ExprKind::Binary(_, lhs, rhs) => {
            collect_expr_idents(lhs, out);
            collect_expr_idents(rhs, out);
        }
        ExprKind::Unary(_, e) => collect_expr_idents(e, out),
        ExprKind::Index(base, idx) => {
            collect_expr_idents(base, out);
            collect_expr_idents(idx, out);
        }
        ExprKind::BitSlice(base, hi, lo) => {
            collect_expr_idents(base, out);
            collect_expr_idents(hi, out);
            collect_expr_idents(lo, out);
        }
        ExprKind::PartSelect(base, start, width, _) => {
            collect_expr_idents(base, out);
            collect_expr_idents(start, out);
            collect_expr_idents(width, out);
        }
        ExprKind::FieldAccess(base, field) => {
            collect_expr_idents(base, out);
            // For bus-style access `port.signal`, the emitted C++ reads
            // the flat name `port_signal` (matching SV bus flattening).
            // Emit that flat name as a candidate so --check-uninit and any
            // other name-indexed downstream analysis catches bus-port reads.
            // Non-bus field access (e.g. struct.field) is also a valid
            // candidate here; the downstream filter (e.g. uninit_inputs
            // membership) decides whether the name warrants action.
            if let ExprKind::Ident(b) = &base.kind {
                out.insert(format!("{}_{}", b, field.name));
            }
        }
        ExprKind::MethodCall(base, _, args) => {
            collect_expr_idents(base, out);
            for a in args {
                collect_expr_idents(a, out);
            }
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args {
                collect_expr_idents(a, out);
            }
        }
        ExprKind::Ternary(cond, then_e, else_e) => {
            collect_expr_idents(cond, out);
            collect_expr_idents(then_e, out);
            collect_expr_idents(else_e, out);
        }
        ExprKind::ExprMatch(scrut, arms) => {
            collect_expr_idents(scrut, out);
            for arm in arms {
                collect_expr_idents(&arm.value, out);
            }
        }
        _ => {}
    }
}

fn collect_inst_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| {
            if let ModuleBodyItem::Inst(inst) = i {
                Some(inst.name.name.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Collect all sub-instance output signal names (auto-declared wires).
/// Pick the C++ field prefix for a Vec signal driven by an inst output.
///
/// The native-sim emitter stores `reg` values under `_<name>`, `let`/`wire`
/// values under `_let_<name>`, and inst output wires under the bare flat
/// name. When an inst Vec output is fanned out element-by-element to a
/// parent-scope Vec signal, the destination prefix depends on which kind
/// of declaration the parent signal is.
///
/// The classification order matters: a signal can sit in both `let_names`
/// (because `wire`s are tracked there) and `inst_out` (because an inst
/// drives it). The wire's storage is `_let_<name>`, so `let_names` must
/// win over `inst_out` — otherwise the write lands on the never-read flat
/// field. See PR #438 for the original fix.
///
/// `reg_names ∩ let_names` is treated as `reg` (regs win); this case is
/// not currently reachable because a name can only be declared once, but
/// the ordering keeps that future-proof.
fn vec_storage_prefix<'a>(
    name: &str,
    reg_names: &HashSet<String>,
    let_names: &HashSet<String>,
    inst_out: &HashSet<String>,
) -> &'a str {
    if reg_names.contains(name) {
        "_"
    } else if let_names.contains(name) {
        "_let_"
    } else if inst_out.contains(name) {
        ""
    } else {
        "_let_"
    }
}

fn collect_inst_output_signals(body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut signals = HashSet::new();
    for item in body {
        if let ModuleBodyItem::Inst(inst) = item {
            for conn in &inst.connections {
                if conn.direction == ConnectDir::Output {
                    if let ExprKind::Ident(name) = &conn.signal.kind {
                        signals.insert(name.clone());
                    }
                }
            }
        }
    }
    signals
}

/// Collect all LHS targets from comb blocks (recursing into if/else/match arms).
fn collect_comb_targets(body: &[ModuleBodyItem]) -> HashSet<String> {
    fn collect_stmt_targets(stmt: &Stmt, out: &mut HashSet<String>) {
        match stmt {
            Stmt::Assign(a) => {
                if let ExprKind::Ident(name) = &a.target.kind {
                    out.insert(name.clone());
                }
            }
            Stmt::IfElse(ie) => {
                for s in &ie.then_stmts {
                    collect_stmt_targets(s, out);
                }
                for s in &ie.else_stmts {
                    collect_stmt_targets(s, out);
                }
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    for s in &arm.body {
                        collect_stmt_targets(s, out);
                    }
                }
            }
            Stmt::Log(_) => {}
            Stmt::For(f) => {
                for s in &f.body {
                    collect_stmt_targets(s, out);
                }
            }
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("seq-only Stmt variant inside comb-context walker")
            }
        }
    }
    let mut targets = HashSet::new();
    for item in body {
        if let ModuleBodyItem::CombBlock(cb) = item {
            for stmt in &cb.stmts {
                collect_stmt_targets(stmt, &mut targets);
            }
        }
    }
    targets
}

use crate::ast::extract_reset_info;

fn resolve_reg_reset_info(reset: &RegReset, ports: &[PortDecl]) -> Option<(String, bool, bool)> {
    match reset {
        RegReset::None => None,
        RegReset::Explicit(sig, kind, level, _) => Some((
            sig.name.clone(),
            *kind == ResetKind::Async,
            *level == ResetLevel::Low,
        )),
        RegReset::Inherit(sig, _) => {
            if let Some(p) = ports.iter().find(|p| p.name.name == sig.name) {
                if let TypeExpr::Reset(kind, level) = &p.ty {
                    Some((
                        sig.name.clone(),
                        *kind == ResetKind::Async,
                        *level == ResetLevel::Low,
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// Extract the reset value expression from a RegReset variant.
fn reset_value_from_reg_reset(reset: &RegReset) -> Option<&Expr> {
    match reset {
        RegReset::None => None,
        RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) => Some(val),
    }
}

/// Build enum_name → Vec<(variant_name, encoding_value)>.
fn build_enum_map(symbols: &SymbolTable) -> HashMap<String, Vec<(String, u64)>> {
    let mut m = HashMap::new();
    for (name, (sym, _)) in &symbols.globals {
        if let Symbol::Enum(info) = sym {
            let entries: Vec<(String, u64)> = info
                .variants
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let val = info.values.get(i).and_then(|v| *v).unwrap_or(i as u64);
                    (v.clone(), val)
                })
                .collect();
            m.insert(name.clone(), entries);
        }
    }
    m
}

/// Resolve an enum variant to its ordinal value.
fn resolve_enum_variant(
    enum_map: &HashMap<String, Vec<(String, u64)>>,
    enum_name: &str,
    variant_name: &str,
) -> Option<u64> {
    enum_map.get(enum_name).and_then(|variants| {
        variants
            .iter()
            .find(|(n, _)| n == variant_name)
            .map(|(_, v)| *v)
    })
}

/// Build a name→width map from module ports, regs, and lets.
fn build_widths(
    ports: &[PortDecl],
    body: &[ModuleBodyItem],
    params: &[ParamDecl],
) -> HashMap<String, u32> {
    let mut m = HashMap::new();
    for p in ports {
        m.insert(p.name.name.clone(), type_bits_te_with_params(&p.ty, params));
    }
    // Compile-time-constant params participate in width inference the same
    // way let bindings do. Without this, `infer_expr_width` falls back to
    // its 8-bit default for any concat / shift expression that names a
    // param, silently producing 1-bit-off bit positions in emitted C++.
    for p in params {
        let bits = match &p.kind {
            ParamKind::WidthConst(hi, lo) => {
                let h = eval_width(hi);
                let l = eval_width(lo);
                h - l + 1
            }
            ParamKind::Logic(ty) | ParamKind::Type(ty) => type_bits_te_with_params(ty, params),
            // `param X: const = N` (untyped). Pre-existing call sites treat
            // this as an int-typed parameter (32 bits), so match.
            ParamKind::Const => 32,
            // Enum / Vec params: width depends on the underlying type. Skip
            // — concat-of-enum isn't a valid construct; concat-of-Vec is
            // handled elsewhere via vec_array_info_with_params.
            ParamKind::EnumConst(_) | ParamKind::ConstVec(_) => continue,
        };
        m.insert(p.name.name.clone(), bits);
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                m.insert(r.name.name.clone(), type_bits_te_with_params(&r.ty, params));
            }
            ModuleBodyItem::WireDecl(w) => {
                // Wires need width registration too — without this, downstream
                // sites that consult ctx.widths (the Bool `~` masking check
                // in cpp_expr's BitNot arm, infer_expr_width's Ident default,
                // …) silently fall back to "32" and produce broken codegen.
                // Symptom: `if ~bool_wire == false` emitted as
                // `(~(uint8_t)1) == 0` → `0xFE == 0` → never true.
                m.insert(w.name.name.clone(), type_bits_te_with_params(&w.ty, params));
            }
            ModuleBodyItem::LetBinding(l) => {
                // Destructuring: widths come from struct field types; these
                // are best-effort looked up at emission time. Leave them
                // out here; widths map defaults kick in if needed.
                if !l.destructure_fields.is_empty() {
                    continue;
                }
                if let Some(ty) = &l.ty {
                    m.insert(l.name.name.clone(), type_bits_te_with_params(ty, params));
                }
            }
            _ => {}
        }
    }
    // Resolve pipe_reg widths from their sources
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            let w = m.get(&p.source.name).copied().unwrap_or(32);
            for i in 0..p.stages {
                if i == p.stages - 1 {
                    m.insert(p.name.name.clone(), w);
                } else {
                    m.insert(format!("{}_stg{}", p.name.name, i + 1), w);
                }
            }
        }
    }
    m
}

#[deprecated(note = "use `type_bits_te_with_params(.., &params)` — the bare \
            form silently miscompiles when a `UInt<W>` / `SInt<W>` \
            width is a param identifier (returns the fallback width \
            of 32 rather than the param's resolved value). See \
            arch-com#447 §1 and PRs #427, #439, #442.")]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
fn type_bits_te(ty: &TypeExpr) -> u32 {
    type_bits_te_with_params(ty, &[])
}

/// Param-aware variant of [`type_bits_te`]. Resolves param idents in
/// `UInt<W>` / `SInt<W>` width positions so that `is_wide_bits` /
/// `collect_wide_names` classification works for `param`-derived widths
/// (e.g. `UInt<W>` with `param W = 96` must be classified wide, not 32).
/// arch-com#330.
fn type_bits_te_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width_with_params(w, params),
        TypeExpr::Bool | TypeExpr::Bit => 1,
        _ => 32,
    }
}

fn type_is_signed_scalar(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::SInt(_))
}

/// Collect scalar names whose HDL type is signed. This parallels the width
/// map so fallback storage paths (implicit instance-output fields, pipe_reg
/// stages, and expression casts) can preserve signedness for 33..=64-bit
/// `SInt` values instead of treating them as unsigned bit buckets.
fn build_signed_names(ports: &[PortDecl], body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut s = HashSet::new();
    for p in ports {
        if type_is_signed_scalar(&p.ty) {
            s.insert(p.name.name.clone());
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if type_is_signed_scalar(&r.ty) {
                    s.insert(r.name.name.clone());
                }
            }
            ModuleBodyItem::WireDecl(w) => {
                if type_is_signed_scalar(&w.ty) {
                    s.insert(w.name.name.clone());
                }
            }
            ModuleBodyItem::LetBinding(l) => {
                if l.ty.as_ref().map_or(false, type_is_signed_scalar) {
                    s.insert(l.name.name.clone());
                }
            }
            _ => {}
        }
    }
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            if s.contains(&p.source.name) {
                for i in 0..p.stages {
                    if i == p.stages - 1 {
                        s.insert(p.name.name.clone());
                    } else {
                        s.insert(format!("{}_stg{}", p.name.name, i + 1));
                    }
                }
            }
        }
    }
    s
}

/// Floating-point format of a scalar TypeExpr, if any.
fn type_float_fmt(ty: &TypeExpr) -> Option<FpFmt> {
    match ty {
        TypeExpr::FP32 => Some(FpFmt::Fp32),
        TypeExpr::BF16 => Some(FpFmt::Bf16),
        _ => None,
    }
}

/// Collect scalar signal names whose HDL type is a float (FP32/BF16), mapping
/// each to its format. Parallels [`build_signed_names`]; drives float-op
/// dispatch in the expression emitter.
fn build_float_names(ports: &[PortDecl], body: &[ModuleBodyItem]) -> HashMap<String, FpFmt> {
    let mut m = HashMap::new();
    for p in ports {
        if let Some(f) = type_float_fmt(&p.ty) {
            m.insert(p.name.name.clone(), f);
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if let Some(f) = type_float_fmt(&r.ty) {
                    m.insert(r.name.name.clone(), f);
                }
            }
            ModuleBodyItem::WireDecl(w) => {
                if let Some(f) = type_float_fmt(&w.ty) {
                    m.insert(w.name.name.clone(), f);
                }
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(f) = l.ty.as_ref().and_then(type_float_fmt) {
                    m.insert(l.name.name.clone(), f);
                }
            }
            _ => {}
        }
    }
    m
}

/// Collect names whose bit width exceeds 64 (require wide handling).
fn collect_wide_names(
    ports: &[PortDecl],
    body: &[ModuleBodyItem],
    params: &[ParamDecl],
) -> HashSet<String> {
    let mut s = HashSet::new();
    for p in ports {
        if type_bits_te_with_params(&p.ty, params) > 64 {
            s.insert(p.name.name.clone());
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if type_bits_te_with_params(&r.ty, params) > 64 {
                    s.insert(r.name.name.clone());
                }
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(ty) = &l.ty {
                    if type_bits_te_with_params(ty, params) > 64 {
                        s.insert(l.name.name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    // Resolve pipe_reg wide from source
    let widths = build_widths(ports, body, params);
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            let w = widths.get(&p.source.name).copied().unwrap_or(32);
            if w > 64 {
                for i in 0..p.stages {
                    if i == p.stages - 1 {
                        s.insert(p.name.name.clone());
                    } else {
                        s.insert(format!("{}_stg{}", p.name.name, i + 1));
                    }
                }
            }
        }
    }
    s
}

// ── Function codegen ──────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    fn gen_functions(&self, fns: &[&FunctionDecl]) -> SimModel {
        let mut h = String::new();
        h.push_str("#pragma once\n#include \"verilated.h\"\n\n");

        // Hoist package- and module-level const params as `#define`s so that
        // function bodies referencing them (e.g. `x >> REGION_BITS` where
        // `REGION_BITS` is a `package` param) compile. The SV path resolves
        // this via `import Pkg::*`; the sim path has no equivalent scope —
        // module-internal functions get hoisted to free C++ functions, and
        // VFunctions.h is included from each V{Module}.h *before* the
        // per-module `#define`s, so without this block the identifier is
        // simply undeclared. `#ifndef`-guarded so re-definitions in
        // per-module headers are harmless.
        let mut emitted_param_defines: HashSet<String> = HashSet::new();
        for item in &self.source.items {
            let (params, _ctx_label): (&[ParamDecl], &str) = match item {
                Item::Package(pkg) => (&pkg.params, "package"),
                Item::Module(m) => (&m.params, "module"),
                _ => continue,
            };
            for p in params {
                if !emitted_param_defines.insert(p.name.name.clone()) {
                    continue;
                }
                match &p.kind {
                    ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::Logic(_) => {
                        if let Some(ref def) = p.default {
                            let val = eval_const_expr_with_params(def, params);
                            h.push_str(&format!(
                                "#ifndef {}\n#define {} {val}ULL\n#endif\n",
                                p.name.name, p.name.name
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        h.push('\n');

        for f in fns {
            // Free functions hoisted out of a module body. The function's
            // param-resolving context is the enclosing module's params (see
            // L4329 above where param defines are emitted from the module's
            // param list), but that slice isn't threaded into this loop yet.
            // The bare-form-equivalent `&[]` is acceptable as a residual
            // since user-written `function ... -> T` signatures typically use
            // concrete-width types; tracked as a follow-up to arch-com#463.
            let ret_ty = cpp_internal_type_with_params(&f.ret_ty, &[]);
            let args_str: Vec<String> = f
                .args
                .iter()
                .map(|a| {
                    format!(
                        "{} {}",
                        cpp_internal_type_with_params(&a.ty, &[]),
                        a.name.name
                    )
                })
                .collect();
            h.push_str(&format!(
                "inline {ret_ty} {}({}) {{\n",
                f.name.name,
                args_str.join(", ")
            ));

            let empty_regs: HashSet<String> = HashSet::new();
            let empty_lets: HashSet<String> = HashSet::new();
            let empty_insts: HashSet<String> = HashSet::new();
            let empty_wide: HashSet<String> = HashSet::new();
            let enum_map = build_enum_map(self.symbols);

            // Build arg + local-let names as bare ports (resolve_name hits
            // them via port_names → no `_let_` prefix, matching the
            // `const T name = ...;` emitted line). Their widths are
            // registered so `infer_expr_width` returns the right size
            // when the name is used inside a `Concat` — pre-fix, every
            // Concat part fell back to width=8, so a
            // `{bool, bool, UInt<3>}` concat emitted shifts at offsets
            // 0/8/16 instead of 0/3/4 and produced wildly wrong values.
            fn collect_function_locals(
                items: &[FunctionBodyItem],
                names: &mut HashSet<String>,
                widths: &mut HashMap<String, u32>,
                signed: &mut HashSet<String>,
            ) {
                for item in items {
                    match item {
                        FunctionBodyItem::Let(l) => {
                            names.insert(l.name.name.clone());
                            let w = match l.ty.as_ref() {
                                Some(TypeExpr::UInt(w)) | Some(TypeExpr::SInt(w)) => eval_width(w),
                                Some(TypeExpr::Bool) | Some(TypeExpr::Bit) => 1,
                                _ => 32,
                            };
                            widths.insert(l.name.name.clone(), w);
                            if l.ty.as_ref().map_or(false, type_is_signed_scalar) {
                                signed.insert(l.name.name.clone());
                            }
                        }
                        FunctionBodyItem::For(fl) => {
                            names.insert(fl.var.name.clone());
                            widths.insert(fl.var.name.clone(), 32);
                            collect_function_locals(&fl.body, names, widths, signed);
                        }
                        FunctionBodyItem::IfElse(ie) => {
                            collect_function_locals(&ie.then_body, names, widths, signed);
                            collect_function_locals(&ie.else_body, names, widths, signed);
                        }
                        FunctionBodyItem::Return(_) | FunctionBodyItem::Assign(_) => {}
                    }
                }
            }
            let empty_bus: HashSet<String> = HashSet::new();
            let mut local_widths: HashMap<String, u32> = HashMap::new();
            let mut local_signed_names: HashSet<String> = HashSet::new();
            let mut arg_ports: HashSet<String> =
                f.args.iter().map(|a| a.name.name.clone()).collect();
            for a in &f.args {
                local_widths.insert(
                    a.name.name.clone(),
                    match &a.ty {
                        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
                        TypeExpr::Bool | TypeExpr::Bit => 1,
                        _ => 32,
                    },
                );
                if type_is_signed_scalar(&a.ty) {
                    local_signed_names.insert(a.name.name.clone());
                }
            }
            collect_function_locals(
                &f.body,
                &mut arg_ports,
                &mut local_widths,
                &mut local_signed_names,
            );
            let function_loop_var_subst: std::cell::RefCell<HashMap<String, u32>> =
                std::cell::RefCell::new(HashMap::new());
            let ctx_base = Ctx::new(
                &empty_regs,
                &arg_ports,
                &empty_lets,
                &empty_insts,
                &empty_wide,
                &local_widths,
                &enum_map,
                &empty_bus,
            )
            .with_signed_names(&local_signed_names);
            let ctx = Ctx {
                loop_var_subst: Some(&function_loop_var_subst),
                ..ctx_base
            };

            // Recursive emitter for nested function-body items (if/elsif/else
            // with return statements inside). Pre-fix the if/for/assign arms
            // were no-ops, so a function whose entire body was an if/else
            // emitted as `inline T fn(...) { }` and called sites failed C++
            // compile with "non-void function does not return a value".
            fn emit_fn_items(
                items: &[FunctionBodyItem],
                ctx: &Ctx,
                ret_ty: &str,
                indent: &str,
                out: &mut String,
            ) {
                for item in items {
                    match item {
                        FunctionBodyItem::Let(l) => {
                            let ty =
                                l.ty.as_ref()
                                    .map(|t| cpp_internal_type_with_params(t, &[]))
                                    .unwrap_or_else(|| "uint32_t".to_string());
                            let val = cpp_expr(&l.value, ctx);
                            out.push_str(&format!("{indent}{ty} {} = {};\n", l.name.name, val));
                        }
                        FunctionBodyItem::Return(e) => {
                            let val = cpp_expr(e, ctx);
                            out.push_str(&format!("{indent}return {val};\n"));
                        }
                        FunctionBodyItem::IfElse(ie) => {
                            let cond = cpp_expr(&ie.cond, ctx);
                            out.push_str(&format!("{indent}if ({cond}) {{\n"));
                            emit_fn_items(&ie.then_body, ctx, ret_ty, &format!("{indent}  "), out);
                            out.push_str(&format!("{indent}}}"));
                            if !ie.else_body.is_empty() {
                                out.push_str(" else {\n");
                                emit_fn_items(
                                    &ie.else_body,
                                    ctx,
                                    ret_ty,
                                    &format!("{indent}  "),
                                    out,
                                );
                                out.push_str(&format!("{indent}}}\n"));
                            } else {
                                out.push_str("\n");
                            }
                        }
                        FunctionBodyItem::For(fl) => {
                            let var = &fl.var.name;
                            match &fl.range {
                                ForRange::Range(lo, hi) => {
                                    let lo_s = cpp_expr(lo, ctx);
                                    let hi_s = cpp_expr(hi, ctx);
                                    out.push_str(&format!("{indent}for (int {var} = {lo_s}; {var} <= {hi_s}; {var}++) {{\n"));
                                    emit_fn_items(
                                        &fl.body,
                                        ctx,
                                        ret_ty,
                                        &format!("{indent}  "),
                                        out,
                                    );
                                    out.push_str(&format!("{indent}}}\n"));
                                }
                                ForRange::ValueList(vals) => {
                                    for val in vals {
                                        let v = cpp_expr(val, ctx);
                                        out.push_str(&format!("{indent}{{\n"));
                                        out.push_str(&format!("{indent}  int {var} = {v};\n"));
                                        emit_fn_items(
                                            &fl.body,
                                            ctx,
                                            ret_ty,
                                            &format!("{indent}  "),
                                            out,
                                        );
                                        out.push_str(&format!("{indent}}}\n"));
                                    }
                                }
                            }
                        }
                        FunctionBodyItem::Assign(a) => {
                            let target = cpp_expr_lhs(&a.target, ctx);
                            let val = cpp_expr(&a.value, ctx);
                            out.push_str(&format!("{indent}{target} = {val};\n"));
                        }
                    }
                }
            }
            // Reuse the same recursive pattern below; legacy direct-loop is
            // kept around the existing match-as-switch shortcut for `Return`.
            for item in &f.body {
                match item {
                    FunctionBodyItem::Let(l) => {
                        let ty =
                            l.ty.as_ref()
                                .map(|t| cpp_internal_type_with_params(t, &[]))
                                .unwrap_or_else(|| "uint32_t".to_string());
                        let val = cpp_expr(&l.value, &ctx);
                        h.push_str(&format!("  {ty} {} = {};\n", l.name.name, val));
                    }
                    FunctionBodyItem::IfElse(ie) => {
                        let cond = cpp_expr(&ie.cond, &ctx);
                        h.push_str(&format!("  if ({cond}) {{\n"));
                        emit_fn_items(&ie.then_body, &ctx, &ret_ty, "    ", &mut h);
                        h.push_str("  }");
                        if !ie.else_body.is_empty() {
                            h.push_str(" else {\n");
                            emit_fn_items(&ie.else_body, &ctx, &ret_ty, "    ", &mut h);
                            h.push_str("  }\n");
                        } else {
                            h.push_str("\n");
                        }
                    }
                    FunctionBodyItem::For(_) | FunctionBodyItem::Assign(_) => {
                        emit_fn_items(std::slice::from_ref(item), &ctx, &ret_ty, "  ", &mut h);
                    }
                    FunctionBodyItem::Return(e) => {
                        // If it's a match expression, emit as switch for efficiency
                        if let ExprKind::ExprMatch(scrut, arms) = &e.kind {
                            let s = cpp_expr(scrut, &ctx);
                            h.push_str(&format!("  switch ({s}) {{\n"));
                            for arm in arms {
                                let val = cpp_expr(&arm.value, &ctx);
                                match &arm.pattern {
                                    Pattern::Wildcard | Pattern::Ident(_) => {
                                        h.push_str(&format!("    default: return {val};\n"));
                                    }
                                    Pattern::Literal(le) => {
                                        let pat = cpp_expr(le, &ctx);
                                        h.push_str(&format!("    case {pat}: return {val};\n"));
                                    }
                                    Pattern::EnumVariant(en, vr) => {
                                        if let Some(variants) = enum_map.get(&en.name) {
                                            let idx = variants
                                                .iter()
                                                .find(|(n, _)| *n == vr.name)
                                                .map(|(_, v)| *v)
                                                .unwrap_or(0);
                                            h.push_str(&format!("    case {idx}: return {val};\n"));
                                        }
                                    }
                                }
                            }
                            h.push_str("  }\n");
                            h.push_str(&format!("  return ({ret_ty})0;\n"));
                        } else {
                            let val = cpp_expr(e, &ctx);
                            h.push_str(&format!("  return {val};\n"));
                        }
                    }
                }
            }
            h.push_str("}\n\n");
        }

        SimModel {
            class_name: "VFunctions".to_string(),
            header: h,
            impl_: String::new(), // header-only
        }
    }
}

// ── Module codegen ────────────────────────────────────────────────────────────

fn collect_stmt_assigns(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign(a) => {
                // Walk the LHS unwrapping Index / BitSlice / PartSelect /
                // FieldAccess until we hit the base Ident. `counter_q[hi:lo]`,
                // `counter_q[i]`, `reg.field`, and chained forms all bind to
                // `counter_q` for reset-walk purposes — the partial-write
                // reg is still subject to its declared reset.
                let mut cursor: &Expr = &a.target;
                loop {
                    match &cursor.kind {
                        ExprKind::Ident(n) => {
                            out.insert(n.clone());
                            break;
                        }
                        ExprKind::Index(base, _)
                        | ExprKind::BitSlice(base, _, _)
                        | ExprKind::PartSelect(base, _, _, _)
                        | ExprKind::FieldAccess(base, _) => {
                            cursor = base;
                        }
                        _ => break,
                    }
                }
            }
            Stmt::IfElse(ie) => {
                collect_stmt_assigns(&ie.then_stmts, out);
                collect_stmt_assigns(&ie.else_stmts, out);
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    collect_stmt_assigns(&arm.body, out);
                }
            }
            Stmt::Log(_) => {}
            Stmt::For(f) => {
                collect_stmt_assigns(&f.body, out);
            }
            Stmt::Init(ib) => {
                collect_stmt_assigns(&ib.body, out);
            }
            Stmt::WaitUntil(_, _) => {}
            Stmt::DoUntil { body, .. } => {
                collect_stmt_assigns(body, out);
            }
        }
    }
}

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

        // Collect reset port levels for `.asserted` polarity abstraction
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

        // Collect reset-none reg names for --check-uninit + any guarded reg (regardless
        // of reset) so Check A can use _<name>_vinit to detect producer bugs.
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
        let mut inst_vec_out: HashMap<String, (String, u64)> = HashMap::new(); // sig → (elem_ty, count)
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
                                    inst_vec_out.insert(sig_name.clone(), (elem_ty, count));
                                    // Also add to vec_wire_counts so output reads expand correctly
                                    vec_wire_counts.insert(sig_name.clone(), count);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Add inst-output Vec names to vec_reg_names so Index uses [i] syntax,
        // and add their element widths to the width map for expression codegen.
        for (name, (elem_ty, count)) in &inst_vec_out {
            vec_reg_names.insert(name.clone());
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
        fn ty_references_named(ty: &TypeExpr) -> bool {
            match ty {
                TypeExpr::Named(_) => true,
                TypeExpr::Vec(inner, _) => ty_references_named(inner),
                _ => false,
            }
        }
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
        // Emit param constants as #define
        for p in &m.params {
            match &p.kind {
                ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::Logic(_) => {
                    if let Some(ref def) = p.default {
                        let val = eval_const_expr_with_params(def, &m.params);
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

        // Shadow valid bits for --check-uninit (reset-none regs + pipe_reg stages)
        if !uninit_regs.is_empty() {
            h.push_str("  // --check-uninit shadow valid bits\n");
            for name in &uninit_regs {
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
        for sig_name in &inst_out {
            if !port_names.contains(sig_name) && !reg_names.contains(sig_name)
                // Bus wires are handled via the struct-typed `_let_<name>`
                // field emitted above; a fallback `uint32_t <name>;` here
                // would shadow the bus wire with a scalar.
                && !bus_wire_names.contains(sig_name)
            {
                // Vec output ports need a C array, not a scalar
                if let Some((elem_ty, count)) = inst_vec_out.get(sig_name) {
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
        let comb_targets = collect_comb_targets(&m.body);
        for sig_name in &comb_targets {
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

        // Helper closure: emit sub-instance input assignments + eval_comb + output reads
        // Returns (input_code, comb_call, output_read_code) per inst
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
        .with_reset_levels(&reset_levels)
        .with_vec_names(&vec_reg_names)
        .with_vec_2d_names(&vec_2d_names)
        .with_vec_sizes(&vec_sizes)
        .with_let_values(&let_values)
        .with_params(&m.params);

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
            // IMPORTANT: Edge detection must happen AFTER the settle loop, not before.
            // Sub-instances may produce derived clocks (e.g. clock dividers, clock gates).
            // Those outputs are only valid after eval_comb(). Detecting edges before settle
            // would use stale clock values and miss or delay derived clock edges by one eval().
            //
            // Correct order:
            //   1. Settle loop: set sub-inst inputs → eval_comb() → read outputs → parent comb
            //   2. Edge detection on ALL clocks (port + internal derived clocks)
            //   3. If rising: parent eval_posedge() + sub-inst eval_posedge() (simultaneous)
            //   4. Re-settle: refresh sub-inst + parent comb with post-posedge state

            // Step 1 + 2: set sub-inst inputs, run comb, read outputs (pre-posedge).
            // Instances are evaluated in topological order (producers before consumers).
            // settle_depth=1 when the graph is a strict DAG with no parent comb
            // intermediates; 2 when parent comb blocks produce signals that feed
            // instance inputs (they're updated by eval_comb() at loop end, so a
            // second pass is needed to propagate them).
            // Perf: classify inst input connections by whether they
            // depend on parent comb (let bindings, comb-driven wires)
            // vs invariant within a cycle (external ports, regs whose
            // value is fixed for the whole eval()). Invariant inputs
            // are mirrored ONCE before the settle loop instead of
            // every iteration — typically saves ~80% of mirror lines
            // for thread-style designs (e.g. ThreadMm2s) where most
            // inputs are external ports.
            let is_invariant = |conn: &crate::ast::Connection| -> bool {
                if let crate::ast::ExprKind::Ident(name) = &conn.signal.kind {
                    // Parent ports: invariant within a cycle.
                    if port_names.contains(name.as_str()) {
                        return true;
                    }
                    // Parent regs: stored value, only changes at posedge.
                    if reg_names.contains(name.as_str()) {
                        return true;
                    }
                    // Vec port elements: also invariant.
                    if vec_port_names.contains(name.as_str()) {
                        return true;
                    }
                }
                // Conservative: anything else (let, wire, expr) is variant.
                false
            };

            // Pre-loop: hoisted invariant input copies.
            for &inst_idx in &inst_eval_order {
                let inst = insts[inst_idx];
                let conns = &expanded_conns[inst_idx];
                for conn in conns {
                    if conn.direction == ConnectDir::Input && is_invariant(conn) {
                        if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                            if let Some(n) = ctx.expr_vec_size(&conn.signal) {
                                let sig = cpp_expr(&conn.signal, &ctx);
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
                                let resolved = ctx.resolve_name(src_name, false);
                                cpp.push_str(&format!(
                                    "  _inst_{}.{} = {};\n",
                                    inst.name.name, conn.port_name.name, resolved
                                ));
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx);
                        cpp.push_str(&format!(
                            "  _inst_{}.{} = {};\n",
                            inst.name.name, conn.port_name.name, sig
                        ));
                    }
                }
            }

            cpp.push_str(&format!(
                "  for (int _settle = 0; _settle < {settle_depth}; _settle++) {{\n"
            ));
            for &inst_idx in &inst_eval_order {
                let inst = insts[inst_idx];
                let conns = &expanded_conns[inst_idx];
                cpp.push('\n');
                for conn in conns {
                    if conn.direction == ConnectDir::Input && !is_invariant(conn) {
                        if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                            if let Some(n) = ctx.expr_vec_size(&conn.signal) {
                                let sig = cpp_expr(&conn.signal, &ctx);
                                for i in 0..n {
                                    cpp.push_str(&format!(
                                        "    _inst_{}.{}_{i} = {sig}[{i}];\n",
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
                                        "    _inst_{}.{}_{i} = {_vec_pfx}{src_name}[{i}];\n",
                                        inst.name.name, conn.port_name.name
                                    ));
                                }
                                continue;
                            }
                            // Parent Vec PORT (input) → inst Vec port:
                            // parent's src is stored as flat fields
                            // `src_0..src_{n-1}`.
                            if vec_port_names.contains(src_name.as_str()) {
                                let n = vec_port_infos
                                    .iter()
                                    .find(|v| v.name == *src_name)
                                    .map(|v| v.count)
                                    .unwrap_or(0);
                                for i in 0..n {
                                    cpp.push_str(&format!(
                                        "    _inst_{}.{}_{i} = {src_name}_{i};\n",
                                        inst.name.name, conn.port_name.name
                                    ));
                                }
                                continue;
                            }
                            if wide_names.contains(src_name.as_str()) {
                                let resolved = ctx.resolve_name(src_name, false);
                                cpp.push_str(&format!(
                                    "    _inst_{}.{} = {};\n",
                                    inst.name.name, conn.port_name.name, resolved
                                ));
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx);
                        cpp.push_str(&format!(
                            "    _inst_{}.{} = {};\n",
                            inst.name.name, conn.port_name.name, sig
                        ));
                    }
                }
                cpp.push_str(&format!("    _inst_{}.eval_comb();\n", inst.name.name));
                for conn in conns {
                    if conn.direction == ConnectDir::Output {
                        if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                            if let Some(n) = ctx.expr_vec_size(&conn.signal) {
                                let sig = cpp_expr(&conn.signal, &ctx);
                                for i in 0..n {
                                    cpp.push_str(&format!(
                                        "    {sig}[{i}] = _inst_{}.{}_{i};\n",
                                        inst.name.name, conn.port_name.name
                                    ));
                                }
                                continue;
                            }
                        }
                        // inst Vec port → Vec wire/reg: expand element-by-element
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some(&n) = vec_wire_counts.get(sig_name.as_str()) {
                                // Parent Vec PORT (output) is stored as flat
                                // fields `name_0..name_{n-1}`, not as a C array.
                                // For all other targets (reg, wire, inst-output
                                // wire), the storage IS an array, so we emit
                                // `prefix name[i]` syntax.
                                if vec_port_names.contains(sig_name.as_str()) {
                                    // Vec OUTPUT port: write to the internal
                                    // _{name}[i] storage; the flat-field sync
                                    // emitted at the end of eval_comb copies
                                    // _{name}[i] → {name}_i. Writing the flat
                                    // field directly here would be clobbered
                                    // by that sync.
                                    for i in 0..n {
                                        cpp.push_str(&format!(
                                            "    _{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    if port_reg_names.contains(sig_name.as_str()) {
                                        emit_port_reg_public_copy(
                                            &mut cpp,
                                            sig_name,
                                            &widths,
                                            Some(n),
                                            "    ",
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
                                            "    {prefix}{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                }
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx);
                        // Wide type (>64 bits): inst port is VlWide, parent reg is _arch_u128
                        let _out_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                            widths.get(n.as_str()).copied().unwrap_or(0)
                        } else {
                            0
                        };
                        if _out_w > 64 {
                            cpp.push_str(&format!(
                                "    {} = _arch_vl_to_u128(_inst_{}.{}.data(), {});\n",
                                sig,
                                inst.name.name,
                                conn.port_name.name,
                                wide_words(_out_w)
                            ));
                        } else {
                            cpp.push_str(&format!(
                                "    {} = _inst_{}.{};\n",
                                sig, inst.name.name, conn.port_name.name
                            ));
                        }
                        if let ExprKind::Ident(name) = &conn.signal.kind {
                            if port_reg_names.contains(name.as_str()) {
                                emit_port_reg_public_copy(&mut cpp, name, &widths, None, "    ");
                            }
                        }
                        // --check-uninit: mark inst output as initialized
                        if let ExprKind::Ident(name) = &conn.signal.kind {
                            if uninit_regs.contains(name.as_str()) {
                                cpp.push_str(&format!("    _{name}_vinit = true;\n"));
                            }
                        }
                    }
                }
            }

            // Parent comb within settle loop
            cpp.push_str("    eval_comb();\n");
            cpp.push_str("  } // settle\n");

            if has_clk {
                // Step 2: eval_posedge detects edges internally (after settle, so derived clocks are valid)
                cpp.push_str("  eval_posedge();\n");

                // Step 3: refresh sub-inst comb outputs, then parent comb (with settle loop)
                // Hoist invariant inputs (post-posedge values for ports/regs)
                // out of the settle loop, mirroring the optimization above.
                for &inst_idx in &inst_eval_order {
                    let inst = insts[inst_idx];
                    let conns = &expanded_conns[inst_idx];
                    for conn in conns {
                        if conn.direction == ConnectDir::Input && is_invariant(conn) {
                            if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                                if let Some(n) = ctx.expr_vec_size(&conn.signal) {
                                    let sig = cpp_expr(&conn.signal, &ctx);
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
                                    let resolved = ctx.resolve_name(src_name, false);
                                    cpp.push_str(&format!(
                                        "  _inst_{}.{} = {};\n",
                                        inst.name.name, conn.port_name.name, resolved
                                    ));
                                    continue;
                                }
                            }
                            let sig = cpp_expr(&conn.signal, &ctx);
                            cpp.push_str(&format!(
                                "  _inst_{}.{} = {};\n",
                                inst.name.name, conn.port_name.name, sig
                            ));
                        }
                    }
                }
                cpp.push_str(&format!(
                    "  for (int _settle = 0; _settle < {settle_depth}; _settle++) {{\n"
                ));
                for &inst_idx in &inst_eval_order {
                    let inst = insts[inst_idx];
                    let conns = &expanded_conns[inst_idx];
                    // Re-set sub-inst inputs (may have changed after posedge)
                    for conn in conns {
                        if conn.direction == ConnectDir::Input && !is_invariant(conn) {
                            if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                                if let Some(n) = ctx.expr_vec_size(&conn.signal) {
                                    let sig = cpp_expr(&conn.signal, &ctx);
                                    for i in 0..n {
                                        cpp.push_str(&format!(
                                            "    _inst_{}.{}_{i} = {sig}[{i}];\n",
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
                                            "    _inst_{}.{}_{i} = {_vec_pfx}{src_name}[{i}];\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                                // Parent Vec PORT (input) → inst Vec port:
                                // parent's src is stored as flat fields
                                // `src_0..src_{n-1}`.
                                if vec_port_names.contains(src_name.as_str()) {
                                    let n = vec_port_infos
                                        .iter()
                                        .find(|v| v.name == *src_name)
                                        .map(|v| v.count)
                                        .unwrap_or(0);
                                    for i in 0..n {
                                        cpp.push_str(&format!(
                                            "    _inst_{}.{}_{i} = {src_name}_{i};\n",
                                            inst.name.name, conn.port_name.name
                                        ));
                                    }
                                    continue;
                                }
                                if wide_names.contains(src_name.as_str()) {
                                    let resolved = ctx.resolve_name(src_name, false);
                                    cpp.push_str(&format!(
                                        "    _inst_{}.{} = {};\n",
                                        inst.name.name, conn.port_name.name, resolved
                                    ));
                                    continue;
                                }
                            }
                            let sig = cpp_expr(&conn.signal, &ctx);
                            cpp.push_str(&format!(
                                "    _inst_{}.{} = {};\n",
                                inst.name.name, conn.port_name.name, sig
                            ));
                        }
                    }
                    cpp.push_str(&format!("    _inst_{}.eval_comb();\n", inst.name.name));
                    for conn in conns {
                        if conn.direction == ConnectDir::Output {
                            if !matches!(conn.signal.kind, ExprKind::Ident(_)) {
                                if let Some(n) = ctx.expr_vec_size(&conn.signal) {
                                    let sig = cpp_expr(&conn.signal, &ctx);
                                    for i in 0..n {
                                        cpp.push_str(&format!(
                                            "    {sig}[{i}] = _inst_{}.{}_{i};\n",
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
                                        // Vec OUTPUT port: write to the internal
                                        // _{name}[i] storage; the flat-field sync
                                        // emitted at the end of eval_comb copies
                                        // _{name}[i] → {name}_i. Writing the flat
                                        // field directly here would be clobbered
                                        // by that sync.
                                        for i in 0..n {
                                            cpp.push_str(&format!(
                                                "    _{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                                inst.name.name, conn.port_name.name
                                            ));
                                        }
                                        if port_reg_names.contains(sig_name.as_str()) {
                                            emit_port_reg_public_copy(
                                                &mut cpp,
                                                sig_name,
                                                &widths,
                                                Some(n),
                                                "    ",
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
                                                "    {prefix}{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                                inst.name.name, conn.port_name.name
                                            ));
                                        }
                                    }
                                    continue;
                                }
                            }
                            let sig = cpp_expr(&conn.signal, &ctx);
                            // Wide type (>64 bits): inst port is VlWide, parent reg is _arch_u128
                            let _out_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                                widths.get(n.as_str()).copied().unwrap_or(0)
                            } else {
                                0
                            };
                            if _out_w > 64 {
                                cpp.push_str(&format!(
                                    "    {} = _arch_vl_to_u128(_inst_{}.{}.data(), {});\n",
                                    sig,
                                    inst.name.name,
                                    conn.port_name.name,
                                    wide_words(_out_w)
                                ));
                            } else {
                                cpp.push_str(&format!(
                                    "    {} = _inst_{}.{};\n",
                                    sig, inst.name.name, conn.port_name.name
                                ));
                            }
                            if let ExprKind::Ident(name) = &conn.signal.kind {
                                if port_reg_names.contains(name.as_str()) {
                                    emit_port_reg_public_copy(
                                        &mut cpp, name, &widths, None, "    ",
                                    );
                                }
                            }
                            // --check-uninit: mark inst output as initialized
                            if let ExprKind::Ident(name) = &conn.signal.kind {
                                if uninit_regs.contains(name.as_str()) {
                                    cpp.push_str(&format!("    _{name}_vinit = true;\n"));
                                }
                            }
                        }
                    }
                }
                cpp.push_str("    eval_comb();\n");
                cpp.push_str("  } // settle\n");
            } // end if has_clk
        } // end else (has insts)

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
                    cpp.push_str(&format!(
                        "  {{ uint64_t _cur = (uint64_t)_inst_{inst_n}.{port_n}; \
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
            .with_vec_names(&vec_reg_names)
            .with_vec_2d_names(&vec_2d_names)
            .with_vec_sizes(&vec_sizes)
            .posedge()
            .with_coverage(cov_handle)
            .with_let_values(&let_values)
            .with_params(&m.params);

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
                                        .with_float_names(&float_names);
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
            for (reg_name, guard_sig) in &guarded_regs {
                cpp.push_str(&format!(
                    "  if ({guard_sig} && !_{reg_name}_vinit) {{\n\
                     \x20   static bool _w_{reg_name}_guard = false;\n\
                     \x20   if (!_w_{reg_name}_guard) {{\n\
                     \x20     _w_{reg_name}_guard = true;\n\
                     \x20     fprintf(stderr, \"GUARD VIOLATION: {name}.{reg_name} — \"\n\
                     \x20             \"{guard_sig}=1 but {reg_name} was never written\\n\");\n\
                     \x20   }}\n\
                     \x20 }}\n",
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
                    let target = if port_names.contains(name) {
                        // Output port — public field, plain name
                        name.clone()
                    } else {
                        // Wire — private field with _let_ prefix
                        format!("_let_{name}")
                    };
                    cpp.push_str(&format!("  {target} = {val};\n"));
                } else {
                    cpp.push_str(&format!("  _let_{} = {};\n", l.name.name, val));
                }
            }
        }

        // If there are sub-instances, re-evaluate the inst chain
        if !insts.is_empty() {
            for (inst_i, inst) in insts.iter().enumerate() {
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
                                cpp.push_str(&format!(
                                    "  _inst_{}.{} = {};\n",
                                    inst.name.name, conn.port_name.name, resolved
                                ));
                                continue;
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
                        if _out_w > 64 {
                            cpp.push_str(&format!(
                                "  {} = _arch_vl_to_u128(_inst_{}.{}.data(), {});\n",
                                sig,
                                inst.name.name,
                                conn.port_name.name,
                                wide_words(_out_w)
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

// ── Counter codegen ───────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    pub(crate) fn gen_counter(&self, c: &CounterDecl) -> SimModel {
        let name = &c.name.name;
        let class = format!("V{name}");

        let max_param = c
            .params
            .iter()
            .find(|p| p.name.name == "MAX")
            .and_then(|p| p.default.as_ref())
            .map(|e| match &e.kind {
                ExprKind::Literal(LitKind::Dec(v)) => *v,
                _ => 255,
            });

        let value_port = c.ports.iter().find(|p| p.name.name == "value");
        let count_bits = value_port
            .and_then(|vp| {
                if let TypeExpr::UInt(w) = &vp.ty {
                    Some(eval_width(w))
                } else {
                    None
                }
            })
            .unwrap_or(8);
        let count_ty = cpp_uint(count_bits);

        let has_inc = c.ports.iter().any(|p| p.name.name == "inc");
        let has_dec = c.ports.iter().any(|p| p.name.name == "dec");
        let has_clear = c.ports.iter().any(|p| p.name.name == "clear");
        let has_at_max = c.ports.iter().any(|p| p.name.name == "at_max");
        let has_at_min = c.ports.iter().any(|p| p.name.name == "at_min");
        let has_max_port = c.ports.iter().any(|p| p.name.name == "max");
        // Resolve the wrap/saturate boundary expression: port `max` takes
        // precedence (runtime-programmable), then the `param MAX = N`
        // compile-time form, falling back to all-ones for the count width.
        let bound_expr: String = if has_max_port {
            format!("({count_ty})max")
        } else if let Some(m) = max_param {
            format!("({count_ty}){m}")
        } else {
            let all_ones = (1u64 << count_bits) - 1;
            format!("({count_ty})0x{all_ones:X}ULL")
        };

        let (rst_name, _is_async, is_low) = extract_reset_info(&c.ports);
        let rst_cond = if is_low {
            format!("(!{})", rst_name)
        } else {
            rst_name.clone()
        };

        let init_val: u64 = c
            .init
            .as_ref()
            .and_then(|e| {
                if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind {
                    Some(*v)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let mut h = String::new();
        h.push_str(
            "#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n\n",
        );
        h.push_str(&format!("class {class} {{\npublic:\n"));
        for p in &c.ports {
            h.push_str(&format!(
                "  {} {};\n",
                cpp_port_type_with_params(&p.ty, &c.params),
                p.name.name
            ));
        }
        h.push('\n');

        let port_inits: Vec<String> = c
            .ports
            .iter()
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        let state_inits = vec![
            "_clk_prev(0)".to_string(),
            format!("_count_r({})", init_val),
        ];
        let all_inits: Vec<String> = port_inits.into_iter().chain(state_inits).collect();
        h.push_str(&format!("  {class}() : {} {{}}\n", all_inits.join(", ")));
        h.push_str(&format!(
            "  explicit {class}(VerilatedContext*) : {class}() {{}}\n"
        ));
        h.push_str("  void eval();\n  void final() { trace_close(); }\n");
        h.push_str("  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  {count_ty} _count_r;\n"));

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = c
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");

        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str("  eval_posedge();\n  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (!_rising) return;\n");
        cpp.push_str(&format!("  {count_ty} _n = _count_r;\n"));
        cpp.push_str(&format!(
            "  if ({rst_cond}) {{\n    _n = {init_val};\n  }} else {{\n"
        ));

        use CounterDirection::*;
        use CounterMode::*;
        match (c.direction, c.mode) {
            (Up, Wrap) => {
                let inc_cond = if has_inc { "    if (inc) {" } else { "    {" };
                cpp.push_str(&format!("{inc_cond}\n"));
                cpp.push_str(&format!(
                    "      if (_count_r == {bound_expr}) _n = {init_val};\n"
                ));
                cpp.push_str("      else _n = _count_r + 1;\n");
                cpp.push_str("    }\n");
            }
            (Down, Wrap) => {
                let dec_cond = if has_dec { "    if (dec) {" } else { "    {" };
                cpp.push_str(&format!("{dec_cond}\n"));
                cpp.push_str(&format!(
                    "      if (_count_r == {init_val}) _n = {bound_expr};\n"
                ));
                cpp.push_str("      else _n = _count_r - 1;\n");
                cpp.push_str("    }\n");
            }
            (Up, Saturate) => {
                let inc_cond = if has_inc { "    if (inc) {" } else { "    {" };
                cpp.push_str(&format!("{inc_cond}\n"));
                cpp.push_str(&format!(
                    "      if (_count_r < {bound_expr}) _n = _count_r + 1;\n"
                ));
                cpp.push_str("    }\n");
            }
            (Down, Saturate) => {
                let dec_cond = if has_dec { "    if (dec) {" } else { "    {" };
                cpp.push_str(&format!("{dec_cond}\n"));
                cpp.push_str("      if (_count_r > 0) _n = _count_r - 1;\n    }\n");
            }
            (Up, Gray) => {
                cpp.push_str("    if (inc) {\n      uint32_t _bin = _count_r + 1;\n");
                cpp.push_str(&format!(
                    "      _n = ({count_ty})(_bin ^ (_bin >> 1));\n    }}\n"
                ));
            }
            (Up, OneHot) => {
                let inc_cond = if has_inc { "    if (inc) {" } else { "    {" };
                cpp.push_str(&format!("{inc_cond}\n"));
                cpp.push_str(&format!("      _n = ({count_ty})((_count_r >> 1) | (_count_r << ({count_bits} - 1)));\n    }}\n"));
            }
            (Up, Johnson) => {
                let inc_cond = if has_inc { "    if (inc) {" } else { "    {" };
                cpp.push_str(&format!("{inc_cond}\n"));
                cpp.push_str(&format!("      _n = ({count_ty})((~_count_r & 1) << ({count_bits}-1)) | (_count_r >> 1);\n    }}\n"));
            }
            (UpDown, _) => {
                cpp.push_str("    if (inc && !dec) _n = _count_r + 1;\n");
                cpp.push_str("    else if (dec && !inc) _n = _count_r - 1;\n");
            }
            _ => {
                let inc_cond = if has_inc { "    if (inc)" } else { "" };
                cpp.push_str(&format!(
                    "    {inc_cond} _n = ({count_ty})(_count_r + 1);\n"
                ));
            }
        }
        if has_clear {
            cpp.push_str(&format!(
                "    if (clear) _n = {init_val}; // clear overrides inc\n"
            ));
        }
        cpp.push_str("  }\n  _count_r = _n;\n}\n\n");

        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        if value_port.is_some() {
            cpp.push_str("  value = _count_r;\n");
        }
        if has_at_max {
            cpp.push_str(&format!("  at_max = (_count_r == {bound_expr}) ? 1 : 0;\n"));
        }
        if has_at_min {
            cpp.push_str(&format!("  at_min = (_count_r == {init_val}) ? 1 : 0;\n"));
        }
        cpp.push_str("}\n");

        // Add trace support
        let extra_sigs: Vec<(&str, &str, u32)> = vec![("count_r", "_count_r", count_bits)];
        add_trace_to_simple_construct(
            &mut h,
            &mut cpp,
            &class,
            name,
            &c.ports,
            &extra_sigs,
            &c.params,
        );
        h.push_str("};\n");

        SimModel {
            class_name: class,
            header: h,
            impl_: cpp,
        }
    }
}

// ── FSM codegen ───────────────────────────────────────────────────────────────

// ── Regfile codegen ───────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    pub(crate) fn gen_regfile(&self, r: &RegfileDecl) -> SimModel {
        let name = &r.name.name;
        let class = format!("V{name}");

        let nregs = r.param_int("NREGS", 32) as usize;
        let nread = r
            .read_ports
            .as_ref()
            .map(|rp| r.resolve_count_expr(&rp.count_expr))
            .unwrap_or(1) as usize;
        let nwrite = r
            .write_ports
            .as_ref()
            .map(|wp| r.resolve_count_expr(&wp.count_expr))
            .unwrap_or(1) as usize;

        // C++ type for one register element (from the write data signal type)
        let elem_cpp = r
            .write_ports
            .as_ref()
            .and_then(|wp| wp.signals.iter().find(|s| s.name.name == "data"))
            .map(|s| cpp_internal_type_with_params(&s.ty, &r.params))
            .unwrap_or_else(|| "uint32_t".to_string());

        // Flat port name: "{pfx}_{sig}" when count==1, "{pfx}{i}_{sig}" otherwise
        let flat = |pfx: &str, i: usize, count: usize, sig: &str| -> String {
            if count == 1 {
                format!("{pfx}_{sig}")
            } else {
                format!("{pfx}{i}_{sig}")
            }
        };

        let clk_port = r
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let read_pfx = r
            .read_ports
            .as_ref()
            .map(|rp| rp.name.name.clone())
            .unwrap_or_else(|| "read".to_string());
        let write_pfx = r
            .write_ports
            .as_ref()
            .map(|wp| wp.name.name.clone())
            .unwrap_or_else(|| "write".to_string());

        // ── Header ────────────────────────────────────────────────────────────
        let mut h = String::new();
        h.push_str(&format!("#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\nclass {class} {{\npublic:\n"));

        for p in &r.ports {
            h.push_str(&format!(
                "  {} {};\n",
                cpp_port_type_with_params(&p.ty, &r.params),
                p.name.name
            ));
        }
        if let Some(rp) = &r.read_ports {
            for i in 0..nread {
                for s in &rp.signals {
                    h.push_str(&format!(
                        "  {} {};\n",
                        cpp_port_type_with_params(&s.ty, &r.params),
                        flat(&read_pfx, i, nread, &s.name.name)
                    ));
                }
            }
        }
        if let Some(wp) = &r.write_ports {
            for i in 0..nwrite {
                for s in &wp.signals {
                    h.push_str(&format!(
                        "  {} {};\n",
                        cpp_port_type_with_params(&s.ty, &r.params),
                        flat(&write_pfx, i, nwrite, &s.name.name)
                    ));
                }
            }
        }
        h.push('\n');

        // Constructor init list (all scalars = 0) + memset for rf array
        let mut inits: Vec<String> = r
            .ports
            .iter()
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        if let Some(rp) = &r.read_ports {
            for i in 0..nread {
                for s in &rp.signals {
                    inits.push(format!("{}(0)", flat(&read_pfx, i, nread, &s.name.name)));
                }
            }
        }
        if let Some(wp) = &r.write_ports {
            for i in 0..nwrite {
                for s in &wp.signals {
                    inits.push(format!("{}(0)", flat(&write_pfx, i, nwrite, &s.name.name)));
                }
            }
        }
        inits.push("_clk_prev(0)".to_string());
        let is_latch_init = r.kind == crate::ast::RegfileKind::Latch;
        let is_internal_init =
            is_latch_init && matches!(r.flops, crate::ast::RegfileFlops::Internal);
        if is_internal_init {
            inits.push("_we_q(0)".to_string());
            inits.push("_waddr_q(0)".to_string());
            inits.push("_wdata_q(0)".to_string());
        }

        h.push_str(&format!(
            "  {class}() : {} {{\n    memset(_rf, 0, sizeof(_rf));\n  }}\n",
            inits.join(", ")
        ));
        h.push_str("  void eval();\n  void eval_comb();\n  void eval_posedge();\n  void final() { trace_close(); }\n\nprivate:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        // Internal sample flops for kind:latch flops:internal (Ibex-style).
        // `_we_q` / `_waddr_q` / `_wdata_q` are taken on the rising edge; the
        // latch then captures during the clk-low half-cycle window (mirrors
        // the SV `always_latch if (!clk && we_q && waddr_q == k)` shape).
        let is_latch = r.kind == crate::ast::RegfileKind::Latch;
        let is_internal = is_latch && matches!(r.flops, crate::ast::RegfileFlops::Internal);
        if is_internal {
            // Single write port assumed (matches SV codegen — same restriction).
            // For a wider data type we still match what cpp_internal_type picks.
            let waddr_t = r
                .write_ports
                .as_ref()
                .and_then(|wp| wp.signals.iter().find(|s| s.name.name == "addr"))
                .map(|s| cpp_internal_type_with_params(&s.ty, &r.params))
                .unwrap_or_else(|| "uint32_t".to_string());
            h.push_str("  uint8_t _we_q;\n");
            h.push_str(&format!("  {waddr_t} _waddr_q;\n"));
            h.push_str(&format!("  {elem_cpp} _wdata_q;\n"));
        }
        h.push_str(&format!("  {elem_cpp} _rf[{nregs}];\n"));

        // ── Implementation ────────────────────────────────────────────────────
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str("  eval_comb();\n  eval_posedge();\n  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        // eval_posedge() — fork on storage kind:
        //   kind:flop                  → flop array, sampled on rising edge.
        //   kind:latch flops:external  → no posedge state; latch update lives
        //                                in eval_comb (transparent while we).
        //   kind:latch flops:internal  → sample we_q/waddr_q/wdata_q here;
        //                                latch capture lives in eval_comb.
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (!_rising) return;\n");
        if !is_latch {
            // Init-protected addresses are immutable (mirrors SV emitter:
            // `init [k] = v;` lowers to a `waddr != k` write guard).
            let guarded_addrs: Vec<u64> = r
                .inits
                .iter()
                .filter_map(|init| match &init.index.kind {
                    ExprKind::Literal(LitKind::Dec(v)) => Some(*v),
                    _ => None,
                })
                .collect();
            for wi in 0..nwrite {
                let wen = flat(&write_pfx, wi, nwrite, "en");
                let waddr = flat(&write_pfx, wi, nwrite, "addr");
                let wdata = flat(&write_pfx, wi, nwrite, "data");
                let guard = if guarded_addrs.is_empty() {
                    wen.clone()
                } else {
                    let parts: Vec<String> = guarded_addrs
                        .iter()
                        .map(|k| format!("{waddr} != {k}"))
                        .collect();
                    format!("{wen} && {}", parts.join(" && "))
                };
                cpp.push_str(&format!("  if ({guard})\n    _rf[{waddr}] = {wdata};\n"));
            }
        } else if is_internal {
            // Single-port sample (write port 0).
            let wen = flat(&write_pfx, 0, nwrite, "en");
            let waddr = flat(&write_pfx, 0, nwrite, "addr");
            let wdata = flat(&write_pfx, 0, nwrite, "data");
            cpp.push_str(&format!("  _we_q = {wen};\n"));
            cpp.push_str(&format!("  if ({wen}) {{\n"));
            cpp.push_str(&format!("    _waddr_q = {waddr};\n"));
            cpp.push_str(&format!("    _wdata_q = {wdata};\n"));
            cpp.push_str("  }\n");
        }
        // is_latch && external: nothing on posedge — latch lives in eval_comb.
        cpp.push_str("}\n\n");

        // eval_comb(): latch update (when kind:latch) + async reads (with
        // optional write-before-read bypass).
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        if is_latch {
            // Latch update runs *before* the read mux so reads in the same
            // tick see fresh data (matches SV's transparent-during-low-phase
            // semantics: read mux is comb on _rf, latch is open during clk-low).
            if is_internal {
                // Internal sample flops: latch transparent during clk-low using
                // sampled inputs. ICG-equivalent gate `!clk && we_q`.
                cpp.push_str(&format!("  if (!{clk_port} && _we_q)\n"));
                cpp.push_str("    _rf[_waddr_q] = _wdata_q;\n");
            } else {
                // External flops: latch transparent whenever we is high (the
                // SV `always_latch if (we && waddr == k)` collapses to this).
                let wen = flat(&write_pfx, 0, nwrite, "en");
                let waddr = flat(&write_pfx, 0, nwrite, "addr");
                let wdata = flat(&write_pfx, 0, nwrite, "data");
                cpp.push_str(&format!("  if ({wen})\n"));
                cpp.push_str(&format!("    _rf[{waddr}] = {wdata};\n"));
            }
        }
        for ri in 0..nread {
            let raddr = flat(&read_pfx, ri, nread, "addr");
            let rdata = flat(&read_pfx, ri, nread, "data");
            if r.forward_write_before_read && nwrite > 0 {
                let wen = flat(&write_pfx, 0, nwrite, "en");
                let waddr = flat(&write_pfx, 0, nwrite, "addr");
                let wdata = flat(&write_pfx, 0, nwrite, "data");
                cpp.push_str(&format!(
                    "  {rdata} = ({wen} && {waddr} == {raddr}) ? {wdata} : _rf[{raddr}];\n"
                ));
            } else {
                cpp.push_str(&format!("  {rdata} = _rf[{raddr}];\n"));
            }
        }
        cpp.push_str("}\n");

        let extra_sigs: Vec<(&str, &str, u32)> = vec![];
        add_trace_to_simple_construct(
            &mut h,
            &mut cpp,
            &class,
            name,
            &r.ports,
            &extra_sigs,
            &r.params,
        );
        h.push_str("};\n");

        SimModel {
            class_name: class,
            header: h,
            impl_: cpp,
        }
    }

    pub(crate) fn gen_synchronizer(&self, s: &crate::ast::SynchronizerDecl) -> SimModel {
        use crate::ast::SyncKind;

        let class = s.name.name.clone();

        let stages: usize = s
            .params
            .iter()
            .find(|p| p.name.name == "STAGES")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| {
                if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind {
                    Some(*v as usize)
                } else {
                    None
                }
            })
            .unwrap_or(2);

        let clk_ports: Vec<&crate::ast::PortDecl> = s
            .ports
            .iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .collect();
        let src_clk = &clk_ports[0].name.name;
        let dst_clk = &clk_ports[1].name.name;

        let data_in_port = s.ports.iter().find(|p| p.name.name == "data_in").unwrap();
        let data_ctype = cpp_port_type_with_params(&data_in_port.ty, &s.params);
        let data_bits: u32 = match &data_in_port.ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
            TypeExpr::Bool | TypeExpr::Bit => 1,
            _ => 32,
        };

        let rst_port = s
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(..)));
        let rst_is_low = rst_port.map_or(
            false,
            |rp| matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low),
        );
        let rst_guard = rst_port.map(|rp| {
            if rst_is_low {
                format!("!{}", rp.name.name)
            } else {
                rp.name.name.clone()
            }
        });

        let cdc_random = self.cdc_random;

        // ── Header ──
        let mut h = String::new();
        h.push_str("#pragma once\n");
        if cdc_random {
            h.push_str("#include <cstdint>\n#include <cstring>\n#include <cstdlib>\n#include \"verilated.h\"\n\n");
        } else {
            h.push_str("#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\n");
        }
        h.push_str(&format!("class {class} {{\npublic:\n"));
        for p in &s.ports {
            h.push_str(&format!(
                "  {} {};\n",
                cpp_port_type_with_params(&p.ty, &s.params),
                p.name.name
            ));
        }
        h.push_str("\n  void eval();\n  void eval_posedge();\n  void eval_comb();\n  void final() { trace_close(); }\n");
        if cdc_random {
            h.push_str(
                "  uint8_t cdc_skip_pct = 25; // 0-100: probability of +1 cycle latency per edge\n",
            );
        }
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev_src;\n  uint8_t _clk_prev_dst;\n");
        h.push_str("  bool _rising_src;\n  bool _rising_dst;\n");
        match s.kind {
            SyncKind::Ff => {
                for i in 0..stages {
                    h.push_str(&format!("  {} _stage{};\n", data_ctype, i));
                }
            }
            SyncKind::Gray => {
                for i in 0..stages {
                    h.push_str(&format!("  {} _gray_stage{};\n", data_ctype, i));
                }
            }
            SyncKind::Handshake => {
                h.push_str(&format!("  {} _data_reg;\n", data_ctype));
                h.push_str("  uint8_t _req_src;\n  uint8_t _ack_src;\n  uint8_t _ack_dst;\n");
                for i in 0..stages {
                    h.push_str(&format!(
                        "  uint8_t _req_sync{};\n  uint8_t _ack_sync{};\n",
                        i, i
                    ));
                }
            }
            SyncKind::Reset => {
                for i in 0..stages {
                    h.push_str(&format!("  uint8_t _stage{};\n", i));
                }
            }
            SyncKind::Pulse => {
                h.push_str("  uint8_t _toggle_src;\n");
                // sync_chain needs STAGES entries + previous value for edge detect
                for i in 0..stages {
                    h.push_str(&format!("  uint8_t _sync{};\n", i));
                }
                h.push_str("  uint8_t _sync_prev;\n");
            }
        }
        if cdc_random {
            h.push_str("  uint32_t _cdc_lfsr;\n");
        }

        // ── Implementation ──
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str(&format!("  _rising_src = ({src_clk} && !_clk_prev_src);\n"));
        cpp.push_str(&format!("  _rising_dst = ({dst_clk} && !_clk_prev_dst);\n"));
        cpp.push_str(&format!(
            "  _clk_prev_src = {src_clk};\n  _clk_prev_dst = {dst_clk};\n"
        ));
        if s.kind == SyncKind::Reset {
            cpp.push_str("  eval_posedge();\n  eval_comb();\n");
        } else {
            cpp.push_str("  if (_rising_src || _rising_dst) eval_posedge();\n  eval_comb();\n");
        }
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        // eval_posedge()
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        if let Some(ref cond) = rst_guard {
            cpp.push_str(&format!("  if ({cond}) {{\n"));
            match s.kind {
                SyncKind::Ff => {
                    for i in 0..stages {
                        cpp.push_str(&format!("    _stage{i} = 0;\n"));
                    }
                }
                SyncKind::Gray => {
                    for i in 0..stages {
                        cpp.push_str(&format!("    _gray_stage{i} = 0;\n"));
                    }
                }
                SyncKind::Handshake => {
                    cpp.push_str("    _data_reg = 0; _req_src = 0; _ack_src = 0; _ack_dst = 0;\n");
                    for i in 0..stages {
                        cpp.push_str(&format!("    _req_sync{i} = 0; _ack_sync{i} = 0;\n"));
                    }
                }
                SyncKind::Reset => {
                    for i in 0..stages {
                        cpp.push_str(&format!("    _stage{i} = 1;\n"));
                    }
                }
                SyncKind::Pulse => {
                    cpp.push_str("    _toggle_src = 0; _sync_prev = 0;\n");
                    for i in 0..stages {
                        cpp.push_str(&format!("    _sync{i} = 0;\n"));
                    }
                }
            }
            if cdc_random {
                cpp.push_str("    _cdc_lfsr = 0xACE1u;\n");
            }
            cpp.push_str("    return;\n  }\n");
        }
        // CDC randomization: LFSR step + skip flag
        if cdc_random {
            cpp.push_str("  // LFSR-based CDC randomization (models metastability settling)\n");
            cpp.push_str(
                "  _cdc_lfsr = (_cdc_lfsr >> 1) ^ ((_cdc_lfsr & 1) ? 0xB4BCD35Cu : 0u);\n",
            );
            cpp.push_str("  bool _cdc_skip = (_cdc_lfsr % 100) < cdc_skip_pct;\n");
        }

        // Open dst guard with optional random skip
        let dst_guard = if cdc_random {
            "  if (_rising_dst && !_cdc_skip) {\n"
        } else {
            "  if (_rising_dst) {\n"
        };

        match s.kind {
            SyncKind::Ff => {
                cpp.push_str(dst_guard);
                for i in (1..stages).rev() {
                    cpp.push_str(&format!("    _stage{i} = _stage{};\n", i - 1));
                }
                cpp.push_str("    _stage0 = data_in;\n  }\n");
            }
            SyncKind::Gray => {
                cpp.push_str(dst_guard);
                for i in (1..stages).rev() {
                    cpp.push_str(&format!("    _gray_stage{i} = _gray_stage{};\n", i - 1));
                }
                cpp.push_str("    _gray_stage0 = data_in ^ (data_in >> 1);\n  }\n");
            }
            SyncKind::Handshake => {
                cpp.push_str("  if (_rising_src) {\n");
                cpp.push_str("    if (data_in != _data_reg && _req_src == _ack_src) {\n");
                cpp.push_str("      _data_reg = data_in;\n      _req_src ^= 1;\n    }\n");
                for i in (1..stages).rev() {
                    cpp.push_str(&format!("    _ack_sync{i} = _ack_sync{};\n", i - 1));
                }
                cpp.push_str("    _ack_sync0 = _ack_dst;\n");
                cpp.push_str(&format!("    _ack_src = _ack_sync{};\n  }}\n", stages - 1));
                cpp.push_str(dst_guard);
                for i in (1..stages).rev() {
                    cpp.push_str(&format!("    _req_sync{i} = _req_sync{};\n", i - 1));
                }
                cpp.push_str("    _req_sync0 = _req_src;\n");
                cpp.push_str(&format!("    _ack_dst = _req_sync{};\n  }}\n", stages - 1));
            }
            SyncKind::Reset => {
                // Async assert is always immediate (no randomization)
                cpp.push_str("  if (data_in) {\n");
                for i in 0..stages {
                    cpp.push_str(&format!("    _stage{i} = 1;\n"));
                }
                if cdc_random {
                    cpp.push_str("  } else if (_rising_dst && !_cdc_skip) {\n");
                } else {
                    cpp.push_str("  } else if (_rising_dst) {\n");
                }
                for i in (1..stages).rev() {
                    cpp.push_str(&format!("    _stage{i} = _stage{};\n", i - 1));
                }
                cpp.push_str("    _stage0 = 0;\n  }\n");
            }
            SyncKind::Pulse => {
                // Source toggle is always immediate (no randomization)
                cpp.push_str("  if (_rising_src) {\n");
                cpp.push_str("    if (data_in) _toggle_src ^= 1;\n");
                cpp.push_str("  }\n");
                cpp.push_str(dst_guard);
                cpp.push_str(&format!("    _sync_prev = _sync{};\n", stages - 1));
                for i in (1..stages).rev() {
                    cpp.push_str(&format!("    _sync{i} = _sync{};\n", i - 1));
                }
                cpp.push_str("    _sync0 = _toggle_src;\n");
                cpp.push_str("  }\n");
            }
        }
        cpp.push_str("}\n\n");

        // eval_comb()
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        match s.kind {
            SyncKind::Ff => {
                cpp.push_str(&format!("  data_out = _stage{};\n", stages - 1));
            }
            SyncKind::Gray => {
                let last = stages - 1;
                cpp.push_str(&format!("  {data_ctype} g = _gray_stage{last};\n"));
                cpp.push_str(&format!("  {data_ctype} b = g;\n"));
                // Standard gray-to-binary: b ^= b >> 1; b ^= b >> 2; b ^= b >> 4; ...
                let mut shift = 1u32;
                while shift < data_bits {
                    cpp.push_str(&format!("  b ^= (b >> {shift});\n"));
                    shift *= 2;
                }
                cpp.push_str("  data_out = b;\n");
            }
            SyncKind::Handshake => {
                cpp.push_str("  data_out = _data_reg;\n");
            }
            SyncKind::Reset => {
                cpp.push_str(&format!("  data_out = _stage{};\n", stages - 1));
            }
            SyncKind::Pulse => {
                // Edge detect: XOR of last stage with its previous value
                cpp.push_str(&format!("  data_out = _sync{} ^ _sync_prev;\n", stages - 1));
            }
        }
        cpp.push_str("}\n");

        let extra_sigs: Vec<(&str, &str, u32)> = vec![];
        add_trace_to_simple_construct(
            &mut h,
            &mut cpp,
            &class,
            &class,
            &s.ports,
            &extra_sigs,
            &s.params,
        );
        h.push_str("};\n");

        SimModel {
            class_name: class,
            header: h,
            impl_: cpp,
        }
    }

    pub(crate) fn gen_clkgate(&self, c: &crate::ast::ClkGateDecl) -> SimModel {
        let class = format!("V{}", c.name.name);

        let clk_in = c
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::In)
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk_in");
        let clk_out = c
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::Out)
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk_out");
        let enable = "enable";
        let test_en = c
            .ports
            .iter()
            .find(|p| p.name.name == "test_en")
            .map(|p| p.name.name.as_str());

        let mut h = String::new();
        h.push_str(&format!(
            "#pragma once\n#include <cstdint>\nclass {} {{\npublic:\n",
            class
        ));

        for p in &c.ports {
            h.push_str(&format!("  uint8_t {} = 0;\n", p.name.name));
        }

        if c.kind == crate::ast::ClkGateKind::Latch {
            h.push_str("  uint8_t _en_latched = 0;\n");
        }

        h.push_str("  void eval();\n");
        h.push_str("  void eval_comb();\n");
        h.push_str("  void eval_posedge();\n");
        h.push_str("};\n");

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{}.h\"\n", class));

        let en_expr = if let Some(te) = test_en {
            format!("{enable} | {te}")
        } else {
            enable.to_string()
        };

        // eval_comb — the actual gate logic
        cpp.push_str(&format!("void {}::eval_comb() {{\n", class));
        match c.kind {
            crate::ast::ClkGateKind::Latch => {
                cpp.push_str(&format!(
                    "  if (!{clk_in}) _en_latched = ({en_expr}) ? 1 : 0;\n"
                ));
                cpp.push_str(&format!("  {clk_out} = {clk_in} & _en_latched;\n"));
            }
            crate::ast::ClkGateKind::And => {
                cpp.push_str(&format!(
                    "  {clk_out} = {clk_in} & (({en_expr}) ? 1 : 0);\n"
                ));
            }
        }
        cpp.push_str("}\n");

        // eval_posedge — no-op for clkgate
        cpp.push_str(&format!("void {}::eval_posedge() {{}}\n", class));

        // eval — calls both
        cpp.push_str(&format!("void {}::eval() {{ eval_comb(); }}\n", class));

        SimModel {
            class_name: class,
            header: h,
            impl_: cpp,
        }
    }

    // ── Structs + Enums file ─────────────────────────────────────────────────

    /// Generate VStructs.h containing C++ type definitions for all ARCH structs and enums.
    /// Collects from both file-scope and inside `package` declarations.
    fn gen_structs_file(&self) -> SimModel {
        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstring>\n\n");

        // Gather all enums and structs, whether declared at file scope or inside packages.
        let mut enums: Vec<&EnumDecl> = Vec::new();
        let mut structs: Vec<&StructDecl> = Vec::new();
        for item in &self.source.items {
            match item {
                Item::Enum(e) => enums.push(e),
                Item::Struct(s) => structs.push(s),
                Item::Package(p) => {
                    for e in &p.enums {
                        enums.push(e);
                    }
                    for s in &p.structs {
                        structs.push(s);
                    }
                }
                _ => {}
            }
        }

        for e in &enums {
            // Enums are uint32_t aliases — variants are used as integer indices
            h.push_str(&format!("typedef uint32_t {};\n", e.name.name));
            for (i, v) in e.variants.iter().enumerate() {
                h.push_str(&format!(
                    "static const uint32_t {}_{} = {}u;\n",
                    e.name.name, v.name, i
                ));
            }
            h.push('\n');
        }

        // C++ struct emission for ARCH packed structs.
        //
        // Canonical ARCH bit layout is first-declared-field = MSB, last-declared = LSB
        // (SV convention, see codegen.rs::emit_struct and the Language Specification
        // §"Packed bit layout"). The C++ struct below lays fields out in declaration
        // order in memory — this is the natural C++ idiom and what pybind11 expects
        // when we expose per-field handles via `.def_readwrite`. Per-field access is
        // bit-order-agnostic, so the C++ memory layout and the SV bit layout don't
        // need to agree structurally.
        //
        // ⚠ Future maintainers: on a little-endian host (x86_64, ARM64 in default
        // mode) a `memcpy`/`reinterpret_cast` of this C++ struct into a wide integer
        // puts the FIRST field at the LSBs — the OPPOSITE of ARCH's canonical bit
        // layout. If you add a code path that serializes a whole struct to a single
        // integer (a `struct as UInt<N>` codegen, a pybind11 `__int__` / `.value`
        // shim, a VCD compound-signal trace, etc.), you MUST explicitly concatenate
        // `first_field → MSB, last_field → LSB` — do NOT rely on `memcpy` or
        // `reinterpret_cast`.
        for s in &structs {
            h.push_str(&format!("struct {} {{\n", s.name.name));
            for f in &s.fields {
                h.push_str(&format!(
                    "  {};\n",
                    cpp_field_decl(&f.name.name, &f.ty, &[])
                ));
            }
            h.push_str(&format!(
                "  {}() {{ std::memset(this, 0, sizeof(*this)); }}\n",
                s.name.name
            ));
            h.push_str(&format!(
                "  explicit {}(uint64_t v) {{ (void)v; std::memset(this, 0, sizeof(*this)); }}\n",
                s.name.name
            ));
            h.push_str(&format!("  {}& operator=(uint64_t v) {{ (void)v; std::memset(this, 0, sizeof(*this)); return *this; }}\n", s.name.name));
            h.push_str("};\n\n");
        }

        // Bus-as-wire support: emit a plain C++ struct for every `bus`, with
        // one field per effective (flattened) signal. Direction information is
        // intentionally dropped — when a bus appears as a `wire` (not a port),
        // each signal is just a named piece of data driven by whichever
        // module's assignment reaches it. Field directions only matter at
        // port boundaries, where the perspective (initiator/target) chooses
        // which side drives which field.
        // Collect buses from both file scope AND packages. `bus` in a package
        // is equivalent to file-scope `bus` — just grouped with the types
        // that define the same package's interface.
        let mut buses: Vec<&BusDecl> = Vec::new();
        for item in &self.source.items {
            match item {
                Item::Bus(b) => buses.push(b),
                Item::Package(p) => {
                    for b in &p.buses {
                        buses.push(b);
                    }
                }
                _ => {}
            }
        }
        for b in &buses {
            // Seed with each bus param's declared default so `generate_if`
            // gates (e.g. `generate_if READ` / `WRITE`) evaluate as the
            // bus author intended for the "no overrides" struct. Without
            // this, the param_map is empty and every conditional branch
            // folds to false, producing an empty struct that breaks any
            // sim consumer that touches a bus field.
            let param_map: HashMap<String, &Expr> = b
                .params
                .iter()
                .filter_map(|p| p.default.as_ref().map(|d| (p.name.name.clone(), d)))
                .collect();
            let effective = crate::resolve::BusInfo {
                name: b.name.name.clone(),
                params: b.params.clone(),
                signals: b
                    .signals
                    .iter()
                    .map(|p| (p.name.name.clone(), p.direction, p.ty.clone()))
                    .collect(),
                generates: b.generates.clone(),
                handshakes: b.handshakes.clone(),
                credit_channels: b.credit_channels.clone(),
                tlm_methods: b.tlm_methods.clone(),
            }
            .effective_signals(&param_map);
            h.push_str(&format!("struct {} {{\n", b.name.name));
            let mut field_inits = Vec::new();
            let mut ctor_body = Vec::new();
            for (sname, _dir, sty) in &effective {
                if vec_array_info_with_params(sty, &b.params).is_some() {
                    h.push_str(&format!("  {};\n", cpp_field_decl(sname, sty, &[])));
                    ctor_body.push(format!("std::memset({}, 0, sizeof({}));", sname, sname));
                } else {
                    let ty = cpp_internal_type_with_params(sty, &b.params);
                    h.push_str(&format!("  {} {};\n", ty, sname));
                    if matches!(sty, TypeExpr::Named(_)) {
                        field_inits.push(format!("{}()", sname));
                    } else {
                        field_inits.push(format!("{}(0)", sname));
                    }
                }
            }
            if field_inits.is_empty() && ctor_body.is_empty() {
                h.push_str(&format!("  {}() {{}}\n", b.name.name));
            } else if field_inits.is_empty() {
                h.push_str(&format!(
                    "  {}() {{ {} }}\n",
                    b.name.name,
                    ctor_body.join(" ")
                ));
            } else if ctor_body.is_empty() {
                h.push_str(&format!(
                    "  {}() : {} {{}}\n",
                    b.name.name,
                    field_inits.join(", ")
                ));
            } else {
                h.push_str(&format!(
                    "  {}() : {} {{ {} }}\n",
                    b.name.name,
                    field_inits.join(", "),
                    ctor_body.join(" ")
                ));
            }
            h.push_str("};\n\n");
        }

        SimModel {
            class_name: "VStructs".to_string(),
            header: h,
            impl_: "#include \"VStructs.h\"\n".to_string(),
        }
    }

    pub(crate) fn gen_arbiter(&self, a: &ArbiterDecl) -> SimModel {
        let name = &a.name.name;
        let class = format!("V{name}");

        let num_req: u64 = a
            .params
            .iter()
            .find(|p| p.name.name == "NUM_REQ")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| {
                if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind {
                    Some(*v)
                } else {
                    None
                }
            })
            .unwrap_or(2);

        let (rst_name, _is_async, is_low) = extract_reset_info(&a.ports);
        let rst_cond = if is_low {
            format!("(!{rst_name})")
        } else {
            rst_name.clone()
        };

        let mut h = String::new();
        h.push_str(
            "#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\n",
        );
        h.push_str(&format!("class {class} {{\npublic:\n"));
        for p in &a.ports {
            let ty = cpp_port_type_with_params(&p.ty, &a.params);
            h.push_str(&format!("  {ty} {};\n", p.name.name));
        }
        for pa in &a.port_arrays {
            h.push_str(&format!("  uint64_t {}_valid;\n", pa.name.name));
            h.push_str(&format!("  uint64_t {}_ready;\n", pa.name.name));
        }
        h.push('\n');

        // Only round_robin and lru need a _last_grant pointer; priority always
        // scans from index 0 (highest priority) so no state is needed.
        let needs_rr_state = matches!(a.policy, ArbiterPolicy::RoundRobin | ArbiterPolicy::Lru);

        let mut all_port_inits: Vec<String> = a
            .ports
            .iter()
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        for pa in &a.port_arrays {
            all_port_inits.push(format!("{}_valid(0)", pa.name.name));
            all_port_inits.push(format!("{}_ready(0)", pa.name.name));
        }
        all_port_inits.push("_clk_prev(0)".to_string());
        if needs_rr_state {
            // Initialize `_last_grant` to N-1 so the first-cycle scan
            // formula `(_last_grant + 1 + _i) % N` starts at index 0,
            // matching the SV emitter's `(rr_ptr_r + arb_i) % N` with
            // `rr_ptr_r` reset to 0. Without this, the sim grants
            // index 1 on the first contending cycle while SV grants
            // index 0 — a 1-slot divergence at t=0 that only resolves
            // after the first successful grant updates `_last_grant`
            // to the actual grantee.
            all_port_inits.push(format!("_last_grant({})", num_req.saturating_sub(1)));
        }

        h.push_str(&format!(
            "  {class}() : {} {{}}\n",
            all_port_inits.join(", ")
        ));
        h.push_str("  void eval();\n  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("  void final() { trace_close(); }\n");
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        if needs_rr_state {
            h.push_str("  uint8_t _last_grant;\n");
        }
        h.push_str("  void trace_open(const char* filename);\n");
        h.push_str("  void trace_dump(uint64_t time);\n");
        h.push_str("  void trace_close();\n");
        h.push_str("  FILE* _trace_fp = nullptr;\n  uint64_t _trace_time = 0;\n");
        h.push_str("};\n");

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = a
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");

        let req_pa_name = a
            .port_arrays
            .first()
            .map(|pa| pa.name.name.as_str())
            .unwrap_or("request");

        // eval(): edge detection lives inside eval_posedge() so a parent
        // module's unconditional `_inst_arb.eval_posedge()` call only
        // advances state on actual rising edges. Without self-gating, the
        // arbiter's round-robin pointer drifts on every TB eval() — which
        // breaks designs that read `grant_requester` post-edge to drive
        // downstream signal handshakes.
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str("  eval_posedge();\n");
        cpp.push_str("  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        // eval_posedge() — self-gated so parents can call it unconditionally.
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (!_rising) return;\n");
        if needs_rr_state {
            // Reset value is N-1 (not 0): see the constructor-init
            // comment above. The scan formula treats `_last_grant + 1`
            // as the first index to test, so `_last_grant = N-1`
            // makes the first post-reset cycle scan from index 0.
            let rst_val = num_req.saturating_sub(1);
            cpp.push_str(&format!(
                "  if ({rst_cond}) {{\n    _last_grant = {rst_val};\n  }} else {{\n"
            ));
            cpp.push_str("    if (grant_valid) _last_grant = grant_requester;\n");
            cpp.push_str("  }\n");
        }
        cpp.push_str("}\n\n");

        // eval_comb() — priority scans from 0 (index 0 = highest priority);
        //               round_robin / lru rotate starting after the last grant.
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        cpp.push_str("  grant_valid = 0;\n  grant_requester = 0;\n");
        cpp.push_str(&format!(
            "  for (int _i = 0; _i < (int){num_req}; _i++) {{\n"
        ));
        if needs_rr_state {
            cpp.push_str(&format!(
                "    int _idx = (_last_grant + 1 + _i) % {num_req};\n"
            ));
        } else {
            cpp.push_str("    int _idx = _i;\n");
        }
        cpp.push_str(&format!("    if (({req_pa_name}_valid >> _idx) & 1) {{\n"));
        cpp.push_str(
            "      grant_valid = 1;\n      grant_requester = _idx;\n      break;\n    }\n  }\n",
        );
        cpp.push_str(&format!(
            "  {req_pa_name}_ready = grant_valid ? (1ULL << grant_requester) : 0;\n"
        ));
        cpp.push_str("}\n\n");

        // Trace methods
        cpp.push_str(&format!(
            "void {class}::trace_open(const char* filename) {{\n"
        ));
        cpp.push_str("  _trace_fp = fopen(filename, \"w\");\n");
        cpp.push_str("  if (!_trace_fp) return;\n");
        cpp.push_str("  fprintf(_trace_fp, \"$timescale 1ns $end\\n\");\n");
        cpp.push_str(&format!(
            "  fprintf(_trace_fp, \"$scope module {} $end\\n\");\n",
            name
        ));
        let mut sig_idx = 0usize;
        for p in &a.ports {
            if matches!(p.ty, TypeExpr::Clock(_) | TypeExpr::Reset(..)) {
                continue;
            }
            let id = vcd_id(sig_idx);
            sig_idx += 1;
            cpp.push_str(&format!(
                "  fprintf(_trace_fp, \"$var wire 1 {} {} $end\\n\");\n",
                id, p.name.name
            ));
        }
        cpp.push_str("  fprintf(_trace_fp, \"$upscope $end\\n$enddefinitions $end\\n\");\n");
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::trace_dump(uint64_t time) {{\n"));
        cpp.push_str("  if (!_trace_fp) return;\n");
        cpp.push_str("  fprintf(_trace_fp, \"#%lu\\n\", (unsigned long)time);\n");
        sig_idx = 0;
        for p in &a.ports {
            if matches!(p.ty, TypeExpr::Clock(_) | TypeExpr::Reset(..)) {
                continue;
            }
            let id = vcd_id(sig_idx);
            sig_idx += 1;
            let pname = &p.name.name;
            cpp.push_str(&format!(
                "  fprintf(_trace_fp, \"%c{}\\n\", {pname} ? '1' : '0');\n",
                id
            ));
        }
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::trace_close() {{\n"));
        cpp.push_str("  if (_trace_fp) {{ fclose(_trace_fp); _trace_fp = nullptr; }}\n");
        cpp.push_str("}\n");

        SimModel {
            class_name: class,
            header: h,
            impl_: cpp,
        }
    }
}
