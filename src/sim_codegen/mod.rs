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
mod fifo;
mod fsm;
mod linklist;
mod pipeline;
mod ram;
mod cam;

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
                if local > src.len() { return None; }
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
        self.points.push(CovPoint { kind, span_start, label });
        idx
    }
}

impl<'a> SimCodegen<'a> {
    pub fn new(
        symbols: &'a SymbolTable,
        source: &'a SourceFile,
        overload_map: HashMap<usize, usize>,
    ) -> Self {
        Self { symbols, source, overload_map, check_uninit: false, inputs_start_uninit: false, check_uninit_ram: false, cdc_random: false, debug: false, debug_depth: 1, debug_fsm: false, coverage: false, source_map: None }
    }

    pub fn coverage(mut self, enabled: bool) -> Self {
        self.coverage = enabled;
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

        // Functions → VFunctions.h (header-only)
        let fn_items: Vec<&FunctionDecl> = self.source.items.iter()
            .flat_map(|i| match i {
                Item::Function(f) => vec![f],
                Item::Package(p) => p.functions.iter().collect(),
                _ => vec![],
            })
            .collect();
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
                    let children: Vec<String> = m.body.iter()
                        .filter_map(|b| if let ModuleBodyItem::Inst(inst) = b {
                            Some(inst.module_name.name.clone())
                        } else { None })
                        .collect();
                    children_map.insert(m.name.name.clone(), children);
                }
            }
            // Root = modules not instantiated by any other module
            let instantiated: std::collections::HashSet<String> = children_map.values()
                .flat_map(|v| v.iter().cloned())
                .collect();
            let roots: Vec<String> = all_module_names.into_iter()
                .filter(|n| !instantiated.contains(n))
                .collect();
            // BFS up to debug_depth levels
            let mut result: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut queue: std::collections::VecDeque<(String, u32)> = roots.into_iter()
                .map(|n| (n, 1u32))
                .collect();
            while let Some((mod_name, depth)) = queue.pop_front() {
                if depth > self.debug_depth { continue; }
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
            match item {
                Item::Module(m)      => models.push(self.gen_module(
                    m,
                    debug_module_set.contains(m.name.name.as_str()),
                    &debug_module_set,
                )),
                Item::Counter(c)     => models.push(self.gen_counter(c)),
                Item::Fsm(f)         => models.push(self.gen_fsm(f)),
                Item::Regfile(r)     => models.push(self.gen_regfile(r)),
                Item::Linklist(l)    => models.push(self.gen_linklist(l)),
                Item::Ram(r)         => models.push(self.gen_ram(r)),
                Item::Cam(c)         => models.push(self.gen_cam(c)),
                Item::Synchronizer(s) => models.push(self.gen_synchronizer(s)),
                Item::Clkgate(c)     => models.push(self.gen_clkgate(c)),
                Item::Fifo(f)        => models.push(self.gen_fifo(f)),
                Item::Arbiter(a)     => models.push(self.gen_arbiter(a)),
                Item::Pipeline(p)    => models.push(self.gen_pipeline(p)),
                _ => {}
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
                    if let Some(w) = self.emit_pybind_module(m) {
                        wrappers.push(w);
                    }
                }
                Item::Fsm(f) => {
                    if let Some(w) = self.emit_pybind_fsm(f) {
                        wrappers.push(w);
                    }
                }
                Item::Counter(c) => {
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
        for p in &m.ports { push_named(&p.ty, &mut stack); }
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                push_named(&r.ty, &mut stack);
            }
        }
        let mut used: HashSet<String> = HashSet::new();
        while let Some(name) = stack.pop() {
            if used.insert(name.clone()) {
                if let Some(sd) = all_structs.get(&name) {
                    for f in &sd.fields { push_named(&f.ty, &mut stack); }
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

        // Bus port flattening
        let mut bus_port_names: HashSet<String> = HashSet::new();
        let mut bus_flat: Vec<(String, TypeExpr)> = Vec::new();
        for p in &m.ports {
            if let Some(ref bi) = p.bus_info {
                bus_port_names.insert(p.name.name.clone());
                bus_flat.extend(flatten_bus_port(&p.name.name, bi, self.symbols));
            }
        }

        // Vec port info
        let vec_port_infos: Vec<(String, String, u64, bool)> = m.ports.iter()
            .filter(|p| p.bus_info.is_none())
            .filter_map(|p| {
                if let Some((elem_ty, count_str)) = vec_array_info_with_params(&p.ty, &m.params) {
                    let count: u64 = count_str.parse().unwrap_or(0);
                    Some((p.name.name.clone(), elem_ty, count, p.direction == Direction::In))
                } else {
                    None
                }
            })
            .collect();
        let vec_port_names: HashSet<String> = vec_port_infos.iter().map(|v| v.0.clone()).collect();

        // Wide signal names
        let wide_names = collect_wide_names(&m.ports, &m.body);

        // Regular scalar ports
        for p in &m.ports {
            if p.bus_info.is_some() { continue; }
            if vec_port_names.contains(&p.name.name) { continue; }
            let field = &p.name.name;
            let width = self.port_width(&p.ty);
            let is_signed = matches!(p.ty, TypeExpr::SInt(_));
            let is_input = p.direction == Direction::In;

            if wide_names.contains(field) {
                // VlWide — generate lambda-based get/set
                bindings.push(self.emit_wide_binding(&class, field, width));
            } else {
                bindings.push(format!("        .def_readwrite(\"{field}\", &{class}::{field})"));
            }
            port_info.push((field.clone(), width, is_signed, is_input, false, false));
        }

        // Vec port flattened fields
        for (base_name, _elem_ty, count, is_input) in &vec_port_infos {
            let width = self.vec_elem_width(&m.ports, base_name);
            for i in 0..*count {
                let field = format!("{base_name}_{i}");
                bindings.push(format!("        .def_readwrite(\"{field}\", &{class}::{field})"));
                port_info.push((field, width, false, *is_input, false, false));
            }
        }

        // Bus port flattened fields
        for (flat_name, flat_ty) in &bus_flat {
            let width = type_bits_te(flat_ty);
            let is_signed = matches!(flat_ty, TypeExpr::SInt(_));
            if wide_names.contains(flat_name) {
                bindings.push(self.emit_wide_binding(&class, flat_name, width));
            } else {
                bindings.push(format!("        .def_readwrite(\"{flat_name}\", &{class}::{flat_name})"));
            }
            port_info.push((flat_name.clone(), width, is_signed, true, false, false));
        }

        // Internal registers (exposed as readonly for testbench inspection)
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                let rname = &r.name.name;
                // Skip if it's also a port name (port regs already handled)
                if m.ports.iter().any(|p| p.name.name == *rname) { continue; }
                let width = self.reg_width(&r.ty);
                let is_signed = matches!(r.ty, TypeExpr::SInt(_));
                let cpp_field = format!("_{rname}");
                if wide_names.contains(rname) {
                    // Wide internal reg — emit lambda getter
                    bindings.push(format!(
                        "        .def_property_readonly(\"{rname}\", [](const {class}& self) {{ return self.{cpp_field}; }})"
                    ));
                } else if vec_array_info(&r.ty).is_some() {
                    // Vec reg — skip for now (complex)
                    continue;
                } else {
                    bindings.push(format!("        .def_readonly(\"{rname}\", &{class}::{cpp_field})"));
                }
                port_info.push((rname.clone(), width, is_signed, false, false, true));
            }
        }

        // Parameters
        for p in &m.params {
            if matches!(p.kind, ParamKind::Const | ParamKind::WidthConst(..)) {
                if let Some(ref def) = p.default {
                    let val = eval_const_expr(def);
                    let pname = &p.name.name;
                    bindings.push(format!(
                        "        .def_property_readonly_static(\"{pname}\", [](py::object) {{ return {val}ULL; }})"
                    ));
                    port_info.push((pname.clone(), 32, false, false, true, false));
                }
            }
        }

        // Methods
        bindings.push(format!("        .def(\"eval\", &{class}::eval)"));
        bindings.push(format!("        .def(\"eval_comb\", &{class}::eval_comb)"));
        bindings.push(format!("        .def(\"eval_posedge\", &{class}::eval_posedge)"));

        // _port_info static method
        let port_info_entries: Vec<String> = port_info.iter()
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
                Item::Struct(s) => { all_structs.insert(s.name.name.clone(), s); }
                Item::Package(p) => {
                    for s in &p.structs { all_structs.insert(s.name.name.clone(), s); }
                }
                _ => {}
            }
        }
        let used_structs = Self::collect_used_structs(m, &all_structs);
        let mut struct_bindings = String::new();
        // Iterate in source order (not HashMap order) for stable output.
        let ordered: Vec<&StructDecl> = self.source.items.iter().flat_map(|item| -> Vec<&StructDecl> {
            match item {
                Item::Struct(s) => vec![s],
                Item::Package(p) => p.structs.iter().collect(),
                _ => vec![],
            }
        }).collect();
        for s in ordered {
            let sname = &s.name.name;
            if !used_structs.contains(sname) { continue; }
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
#include "{class}.h"
namespace py = pybind11;

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
    pub fn verilated_h() -> String {
        r#"#pragma once
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

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

/// Convert VlWide<4> → 128-bit integer (bit 127 = MSB = _data[3] MSB).
static inline _arch_u128 _arch_vl_to_u128(const uint32_t* w) {
    return ((_arch_u128)w[3] << 96) | ((_arch_u128)w[2] << 64)
         | ((_arch_u128)w[1] << 32) | (_arch_u128)w[0];
}

/// Convert 128-bit integer → VlWide<4>.
static inline void _arch_u128_to_vl(const _arch_u128 v, uint32_t* w) {
    w[0] = (uint32_t)(v);
    w[1] = (uint32_t)(v >> 32);
    w[2] = (uint32_t)(v >> 64);
    w[3] = (uint32_t)(v >> 96);
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
"#.to_string()
    }

    pub fn verilated_cpp() -> String {
        r#"#include "verilated.h"
int Verilated::_s_verbosity = 1;
const char* Verilated::_s_trace_file = nullptr;
bool Verilated::_s_trace_claimed = false;
"#.to_string()
    }
}

// ── VCD Trace helpers ────────────────────────────────────────────────────────

/// A signal to be traced in VCD output.
struct TraceSignal {
    vcd_name: String,    // display name in VCD scope
    cpp_expr: String,    // C++ expression to read the value
    width: u32,          // bit width
    is_wide: bool,       // true if VlWide<N> type
}

/// Generate a short VCD identifier from a signal index.
/// Uses alphanumeric chars only (a-z, A-Z, 0-9) to avoid C string/printf conflicts.
fn vcd_id(index: usize) -> String {
    // Prefix with 's' to ensure valid VCD id, then index
    format!("s{index}")
}

/// Emit trace_open / trace_dump / trace_close C++ method implementations.
/// Returns (header_declarations, cpp_implementations).
fn emit_trace_methods(class: &str, module_name: &str, signals: &[TraceSignal]) -> (String, String) {
    let mut h = String::new();
    let mut cpp = String::new();

    h.push_str("  void trace_open(const char* filename);\n");
    h.push_str("  void trace_dump(uint64_t time);\n");
    h.push_str("  void trace_close();\n");

    // ── trace_open ──
    cpp.push_str(&format!("void {class}::trace_open(const char* filename) {{\n"));
    cpp.push_str("  _trace_fp = fopen(filename, \"w\");\n");
    cpp.push_str("  if (!_trace_fp) return;\n");
    cpp.push_str("  fprintf(_trace_fp, \"$timescale 1ns $end\\n\");\n");
    cpp.push_str(&format!("  fprintf(_trace_fp, \"$scope module {} $end\\n\");\n", module_name));
    for (i, sig) in signals.iter().enumerate() {
        let id = vcd_id(i);
        let kind = if sig.vcd_name.starts_with('_') { "reg" } else { "wire" };
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
) -> Vec<TraceSignal> {
    let mut sigs = Vec::new();

    // Ports (skip bus ports and Vec ports — flattened signals added separately;
    // also skip struct/enum-typed ports, which can't be bit-shifted scalar-style)
    for p in ports {
        if p.bus_info.is_some() { continue; }
        if matches!(p.ty, TypeExpr::Vec(..) | TypeExpr::Named(_)) { continue; }
        let name = &p.name.name;
        let width = type_width(&p.ty);
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
        let width = type_width(flat_ty);
        let is_wide = wide_names.contains(flat_name.as_str());
        sigs.push(TraceSignal {
            vcd_name: flat_name.clone(),
            cpp_expr: flat_name.clone(),
            width,
            is_wide,
        });
    }

    // Registers (skip struct/named types and Vec types — can't bit-shift)
    // Regs >64 bits use _arch_u128, not VlWide, so is_wide = false
    for item in body {
        if let ModuleBodyItem::RegDecl(r) = item {
            if matches!(r.ty, TypeExpr::Named(_) | TypeExpr::Vec(..)) { continue; }
            let name = &r.name.name;
            let width = type_width(&r.ty);
            let is_wide = false; // regs use _arch_u128, not VlWide
            sigs.push(TraceSignal {
                vcd_name: name.clone(),
                cpp_expr: format!("_{name}"),
                width,
                is_wide,
            });
        }
    }

    // Let bindings and wire decls — skip Vec (C arrays) and struct/enum-typed
    // (Named), which can't be bit-shifted scalar-style. Matches the filter
    // already applied to ports and regs above.
    for item in body {
        match item {
            ModuleBodyItem::LetBinding(l) => {
                // ty=None means assignment to existing port/wire — already traced, skip
                if l.ty.is_none() { continue; }
                let name = &l.name.name;
                if l.ty.as_ref().map_or(false,
                    |t| matches!(t, TypeExpr::Vec(..) | TypeExpr::Named(_))) { continue; }
                let width = l.ty.as_ref().map(|t| type_width(t)).unwrap_or(
                    widths.get(name.as_str()).copied().unwrap_or(32)
                );
                sigs.push(TraceSignal {
                    vcd_name: name.clone(),
                    cpp_expr: format!("_let_{name}"),
                    width,
                    is_wide: false,
                });
            }
            ModuleBodyItem::WireDecl(w) => {
                if matches!(w.ty, TypeExpr::Vec(..) | TypeExpr::Named(_)) { continue; }
                let name = &w.name.name;
                let width = type_width(&w.ty);
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
fn type_width(ty: &TypeExpr) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
        TypeExpr::Bool => 1,
        TypeExpr::Bit => 1,
        TypeExpr::Clock(_) => 1,
        TypeExpr::Reset { .. } => 1,
        TypeExpr::Vec(elem, count) => type_width(elem) * eval_width(count),
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
) {
    // Build signal list from ports + extras
    // Vec and bus ports are skipped (their flat fields are passed via extra_signals by caller).
    let mut signals = Vec::new();
    for p in ports {
        if matches!(p.ty, TypeExpr::Vec(..)) { continue; }  // handled as flat via extra_signals
        if p.bus_info.is_some() { continue; }                // bus ports flattened via extra_signals
        let width = type_width(&p.ty);
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
            if v <= 1 { 1 } else { 32 - (v - 1).leading_zeros() }
        }
        _ => 32,
    }
}

/// Number of 32-bit words needed for `bits` bits.
fn wide_words(bits: u32) -> u32 { (bits + 31) / 32 }

/// True if a signal width requires a wide (VlWide) type.
fn is_wide_bits(bits: u32) -> bool { bits > 64 }

/// C++ type for a public port field.
fn cpp_port_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::UInt(w) => {
            let b = eval_width(w);
            if is_wide_bits(b) { format!("VlWide<{}>", wide_words(b)) }
            else { cpp_uint(b).to_string() }
        }
        TypeExpr::SInt(w) => {
            let b = eval_width(w);
            if is_wide_bits(b) { format!("VlWide<{}>", wide_words(b)) }
            else { cpp_sint(b).to_string() }
        }
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => "uint8_t".to_string(),
        TypeExpr::Named(n) => n.name.clone(),
        TypeExpr::Vec(_, _) => "uint32_t".to_string(),
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
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(replacement) = params.get(name.as_str()) {
                (*replacement).clone()
            } else {
                expr.clone()
            }
        }
        _ => expr.clone(),
    }
}

/// Return flattened bus port signals with direction: Vec<(flat_name, Direction, TypeExpr)>.
/// Direction is from the module's perspective (target flips initiator directions).
fn flatten_bus_port_with_dir(
    port_name: &str,
    bi: &BusPortInfo,
    symbols: &crate::resolve::SymbolTable,
) -> Vec<(String, Direction, TypeExpr)> {
    let bus_name = &bi.bus_name.name;
    if let Some((crate::resolve::Symbol::Bus(info), _)) = symbols.globals.get(bus_name) {
        let mut param_map: HashMap<String, &Expr> = info.params.iter()
            .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
            .collect();
        for pa in &bi.params {
            param_map.insert(pa.name.name.clone(), &pa.value);
        }
        let eff = info.effective_signals(&param_map);
        let is_target = bi.perspective == BusPerspective::Target;
        eff.iter().map(|(sname, sdir, sty)| {
            let subst_ty = subst_type_expr_sim(sty, &param_map);
            // Target perspective flips all signal directions
            let dir = if is_target {
                match sdir { Direction::In => Direction::Out, Direction::Out => Direction::In }
            } else {
                *sdir
            };
            (format!("{}_{}", port_name, sname), dir, subst_ty)
        }).collect()
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
) -> Vec<(String, TypeExpr)> {
    flatten_bus_port_with_dir(port_name, bi, symbols)
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
    source: &SourceFile,
    symbols: &crate::resolve::SymbolTable,
    bus_wire_names: &HashSet<String>,
) -> Vec<Connection> {
    // Find the target construct's bus ports (with perspective info)
    let target_ports: Option<&[PortDecl]> = source.items.iter()
        .find_map(|item| match item {
            Item::Module(m) if m.name.name == inst.module_name.name => Some(m.ports.as_slice()),
            Item::Fsm(f) if f.name.name == inst.module_name.name => Some(f.ports.as_slice()),
            _ => None,
        });
    let target_bus_ports: Vec<(&str, &str, BusPerspective, &[ParamAssign])> = target_ports
        .map(|ports| ports.iter()
            .filter_map(|p| p.bus_info.as_ref().map(|bi| (p.name.name.as_str(), bi.bus_name.name.as_str(), bi.perspective, bi.params.as_slice())))
            .collect())
        .unwrap_or_default();

    let mut expanded = Vec::new();
    for c in &inst.connections {
        if let Some((_, bus_name, perspective, bus_params)) = target_bus_ports.iter().find(|(pn, _, _, _)| *pn == c.port_name.name) {
            // Bus connection — expand to individual signal connections
            if let Some((crate::resolve::Symbol::Bus(info), _)) = symbols.globals.get(*bus_name) {
                // Two shapes for the parent-side signal on a whole-bus binding:
                //   * `p -> ident` where `ident` is a bus port or a bus wire
                //   * `p -> base.field` where `base.field` is a bus port on the parent
                // For bus WIRES we keep the bus-wire name as the struct base and
                // emit FieldAccess exprs per signal so cpp_expr resolves them
                // to `_let_<wire>.<field>`. For bus PORTS we emit flat idents
                // (the port has been flattened elsewhere into `<port>_<field>`).
                let (sig_base, wire_bound) = match &c.signal.kind {
                    ExprKind::Ident(name) => {
                        let is_wire = bus_wire_names.contains(name.as_str());
                        (name.clone(), is_wire)
                    }
                    ExprKind::FieldAccess(base, field) => {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            (format!("{}_{}", base_name, field.name), false)
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let mut _pm = info.default_param_map();
                for pa in *bus_params { _pm.insert(pa.name.name.clone(), &pa.value); }
                let _eff = info.effective_signals(&_pm); for (sname, sdir, _) in &_eff {
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
                    // Bus WIRE target → struct-field access on the wire.
                    // Bus PORT target → flat `<port>_<field>` ident.
                    let parent_signal = if wire_bound {
                        Expr::new(
                            ExprKind::FieldAccess(
                                Box::new(Expr::new(ExprKind::Ident(sig_base.clone()), c.signal.span)),
                                Ident::new(sname.clone(), c.signal.span),
                            ),
                            c.signal.span,
                        )
                    } else {
                        Expr::new(
                            ExprKind::Ident(format!("{}_{}", sig_base, sname)),
                            c.signal.span,
                        )
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
fn cpp_internal_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::UInt(w) => {
            let b = eval_width(w);
            if b > 128 { format!("VlWide<{}>", wide_words(b)) }
            else if b > 64 { "_arch_u128".to_string() }
            else { cpp_uint(b).to_string() }
        }
        TypeExpr::SInt(w) => {
            let b = eval_width(w);
            if b > 128 { format!("VlWide<{}>", wide_words(b)) }
            else if b > 64 { "_arch_u128".to_string() }
            else { cpp_sint(b).to_string() }
        }
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => "uint8_t".to_string(),
        TypeExpr::Named(n) => n.name.clone(),
        TypeExpr::Vec(_, _) => "uint32_t".to_string(),
    }
}

/// If `ty` is Vec<T, N>, return (elem_cpp_type, count_string).
fn vec_array_info(ty: &TypeExpr) -> Option<(String, String)> {
    if let TypeExpr::Vec(elem, count_expr) = ty {
        let elem_type = cpp_internal_type(elem);
        let count_str = eval_const_expr(count_expr).to_string();
        Some((elem_type, count_str))
    } else {
        None
    }
}

/// Evaluate a constant expression to a u64, resolving basic arithmetic.
/// Backward-compatible wrapper that doesn't resolve param identifiers —
/// see [`eval_const_expr_with_params`] for the version that does. Use
/// the param-aware version anywhere a Vec / array length needs to fold
/// across `param N: const = …;` references (otherwise the result is 0
/// and downstream code emits zero-sized C++ arrays — see the regression
/// fixed in PR #cam-zero-array).
fn eval_const_expr(expr: &Expr) -> u64 {
    eval_const_expr_with_params(expr, &[])
}

/// Param-aware constant evaluator. Resolves bare identifiers against
/// `params` (regular + local) by recursing on each param's `default`
/// expression. Handles literals, `$clog2(x)`, unary `-`/`~`, and
/// binary `+`, `-`, `*`, `/`, `%`, `<<`, `>>`, `&`, `|`, `^`. Returns 0
/// for anything it can't fold (e.g. non-literal port reads), matching
/// the conservative behavior of the legacy single-arg version.
fn eval_const_expr_with_params(expr: &Expr, params: &[ParamDecl]) -> u64 {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v)) => *v,
        ExprKind::Literal(LitKind::Hex(v)) => *v,
        ExprKind::Literal(LitKind::Bin(v)) => *v,
        ExprKind::Literal(LitKind::Sized(_, v)) => *v,
        ExprKind::Ident(name) => {
            if let Some(p) = params.iter().find(|p| p.name.name == *name) {
                if let Some(d) = &p.default {
                    return eval_const_expr_with_params(d, params);
                }
            }
            0
        }
        ExprKind::Clog2(a) => {
            let v = eval_const_expr_with_params(a, params);
            if v <= 1 { 0 } else { 64 - (v - 1).leading_zeros() as u64 }
        }
        ExprKind::Unary(op, a) => {
            let v = eval_const_expr_with_params(a, params);
            match op {
                UnaryOp::Not => !v,
                UnaryOp::Neg => v.wrapping_neg(),
                _ => 0,
            }
        }
        ExprKind::Binary(op, l, r) => {
            let lv = eval_const_expr_with_params(l, params);
            let rv = eval_const_expr_with_params(r, params);
            match op {
                BinOp::Add => lv.wrapping_add(rv),
                BinOp::Sub => lv.wrapping_sub(rv),
                BinOp::Mul => lv.wrapping_mul(rv),
                BinOp::Div => if rv == 0 { 0 } else { lv / rv },
                BinOp::Mod => if rv == 0 { 0 } else { lv % rv },
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
        let elem_type = cpp_internal_type(elem);
        let count_str = eval_const_expr_with_params(count_expr, params).to_string();
        Some((elem_type, count_str))
    } else {
        None
    }
}

/// If `expr` is a bare identifier, return its name — used for diagnostic
/// location strings in runtime bounds-check codegen.
fn base_ident_name(expr: &Expr) -> Option<&str> {
    if let ExprKind::Ident(n) = &expr.kind { Some(n.as_str()) } else { None }
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
    if !trimmed.starts_with('t') { return false; }
    if !trimmed.ends_with("_state") { return false; }
    // Middle must be digits
    let mid = &trimmed[1..trimmed.len() - "_state".len()];
    !mid.is_empty() && mid.chars().all(|c| c.is_ascii_digit())
}

fn cpp_uint(bits: u32) -> &'static str {
    if bits <= 8  { "uint8_t" }
    else if bits <= 16 { "uint16_t" }
    else if bits <= 32 { "uint32_t" }
    else               { "uint64_t" }
}

/// Return the bit-width of a TypeExpr, or 0 if indeterminate (e.g. Vec with param size).
fn type_width_of(ty: &TypeExpr) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => 1,
        TypeExpr::Vec(..) | TypeExpr::Named(_) => 0,
    }
}

/// Smallest C++ signed integer type that fits `bits` (up to 64).
fn cpp_sint(bits: u32) -> &'static str {
    if bits <= 8  { "int8_t" }
    else if bits <= 16 { "int16_t" }
    else if bits <= 32 { "int32_t" }
    else               { "int64_t" }
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

/// Bit-range extraction from a narrow value: `(expr >> lo) & mask`.
fn bit_range(expr: &str, hi: u32, lo: u32) -> String {
    let width = hi - lo + 1;
    let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
    format!("(({} >> {}) & 0x{:X}ULL)", expr, lo, mask)
}

/// Bit-range extraction from a `_arch_u128` value.
fn bit_range_u128(expr: &str, hi: u32, lo: u32) -> String {
    let width = hi - lo + 1;
    let result_type = cpp_uint(width);
    if lo == 0 && width >= 128 {
        format!("({result_type})({})", expr)
    } else if lo == 0 {
        let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
        format!("({result_type})(((_arch_u128)({}) & (_arch_u128)0x{:X}ULL))", expr, mask)
    } else {
        let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
        format!("({result_type})(((_arch_u128)({}) >> {}) & (_arch_u128)0x{:X}ULL)", expr, lo, mask)
    }
}

/// Convert SV/ARCH format string tokens to printf equivalents.
fn sv_fmt_to_printf(s: &str) -> String {
    s.replace("%0t", "%lu")
     .replace("%0d", "%lld")
     .replace("%0h", "%llx")
     .replace("%0b", "%llu")
     .replace("%t",  "%lu")
     .replace("%h",  "%llx")
     .replace("%d",  "%lld")
     .replace("%b",  "%llu")
}

// ── Expression context ────────────────────────────────────────────────────────

struct Ctx<'a> {
    reg_names:   &'a HashSet<String>,
    port_names:  &'a HashSet<String>,
    let_names:   &'a HashSet<String>,
    inst_names:  &'a HashSet<String>,
    /// Signals whose type is >64 bits wide (require special handling).
    wide_names:  &'a HashSet<String>,
    /// Signal name → bit width for known signals (used for concat width inference).
    widths:      &'a HashMap<String, u32>,
    posedge_lhs: bool,
    /// FSM mode: regs are public members, no `_` prefix on reads
    fsm_mode:    bool,
    enum_map:    &'a HashMap<String, Vec<(String, u64)>>,
    /// Bus port names (for FieldAccess rewriting: itcm.cmd_valid → itcm_cmd_valid).
    bus_ports:   &'a HashSet<String>,
    /// Reset port name → level, for `.asserted` polarity abstraction.
    reset_levels: &'a HashMap<String, ResetLevel>,
    /// Reg/wire names whose type is Vec<T,N> — these use C array subscript `[i]`.
    /// All other subscripts on scalar UInt/SInt use bit extraction `(x >> i) & 1`.
    vec_names:   Option<&'a HashSet<String>>,
    /// Vec<T,N> sizes by name (element count). Used for runtime bounds-check codegen.
    vec_sizes:   Option<&'a HashMap<String, u64>>,
    /// FSM Vec port-regs: always resolve to `_name` (internal C array), regardless of fsm_mode.
    /// These ports have flat public fields (name_0..name_N-1) but internal storage `_name[N]`.
    fsm_vec_port_regs: Option<&'a HashSet<String>>,
    /// Identifier substitutions active while emitting a Vec method predicate
    /// (e.g. "item" → "vec[3]", "index" → "3"). Checked first in the Ident
    /// branch of `cpp_expr`; None or missing key means normal resolution.
    ident_subst: Option<&'a HashMap<String, String>>,
    /// Branch-coverage registry for the current module. None when --coverage
    /// is off; Some(_) when on. emit_*_if_else allocates counter ids here
    /// and emits `_arch_cov[N]++;` at the start of each arm.
    coverage: Option<&'a std::cell::RefCell<CoverageRegistry>>,
}

impl<'a> Ctx<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        reg_names:  &'a HashSet<String>,
        port_names: &'a HashSet<String>,
        let_names:  &'a HashSet<String>,
        inst_names: &'a HashSet<String>,
        wide_names: &'a HashSet<String>,
        widths:     &'a HashMap<String, u32>,
        enum_map:   &'a HashMap<String, Vec<(String, u64)>>,
        bus_ports:  &'a HashSet<String>,
    ) -> Self {
        static EMPTY_RESET_LEVELS: std::sync::OnceLock<HashMap<String, ResetLevel>> = std::sync::OnceLock::new();
        let reset_levels = EMPTY_RESET_LEVELS.get_or_init(HashMap::new);
        Ctx { reg_names, port_names, let_names, inst_names, wide_names,
              widths, posedge_lhs: false, fsm_mode: false, enum_map, bus_ports,
              reset_levels, vec_names: None, vec_sizes: None, fsm_vec_port_regs: None,
              ident_subst: None, coverage: None }
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

    fn with_fsm_vec_port_regs(mut self, fsm_vec_port_regs: &'a HashSet<String>) -> Self {
        self.fsm_vec_port_regs = Some(fsm_vec_port_regs);
        self
    }

    fn with_coverage(mut self, reg: Option<&'a std::cell::RefCell<CoverageRegistry>>) -> Self {
        self.coverage = reg;
        self
    }

    fn posedge(mut self) -> Self { self.posedge_lhs = true; self }

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
            if self.fsm_mode {
                name.to_string()
            } else {
                format!("_let_{name}")
            }
        } else if self.inst_names.contains(name) {
            format!("_inst_{name}")
        } else if self.port_names.contains(name)
                  && self.vec_names.map_or(false, |s| s.contains(name)) {
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
                // 65–128 bit: port is VlWide<4>, internal arithmetic uses _arch_u128
                format!("_arch_vl_to_u128({base}._data)")
            }
        } else {
            base
        }
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
        ExprKind::Literal(LitKind::Sized(w, _)) => *w,
        ExprKind::Literal(_) => 32,
        ExprKind::Bool(_) => 1,
        ExprKind::MethodCall(base, method, _) if method.name == "reverse" => {
            infer_expr_width(base, ctx)
        }
        ExprKind::MethodCall(_, method, args) if method.name == "trunc" || method.name == "zext" || method.name == "sext" || method.name == "resize" => {
            if let Some(w) = args.first() {
                eval_width(w)
            } else {
                8
            }
        }
        ExprKind::BitSlice(_, hi, lo) => {
            let h = eval_width(hi);
            let l = eval_width(lo);
            h - l + 1
        }
        ExprKind::PartSelect(_, _, width, _) => eval_width(width),
        ExprKind::Cast(_, ty) => {
            match ty.as_ref() {
                TypeExpr::UInt(w) => eval_width(w),
                TypeExpr::SInt(w) => eval_width(w),
                _ => 8,
            }
        }
        ExprKind::Concat(parts) => {
            parts.iter().map(|p| infer_expr_width(p, ctx)).sum()
        }
        ExprKind::Repeat(count, value) => {
            let n = eval_width(count);
            let w = infer_expr_width(value, ctx);
            n * w
        }
        ExprKind::Binary(op, lhs, rhs) => {
            match op {
                // Comparison and logical ops always produce 1-bit Bool
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt |
                BinOp::Lte | BinOp::Gte | BinOp::And | BinOp::Or => 1,
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
        ExprKind::Unary(UnaryOp::Not, inner) => infer_expr_width(inner, ctx),
        ExprKind::Unary(UnaryOp::RedAnd, _)
        | ExprKind::Unary(UnaryOp::RedOr, _)
        | ExprKind::Unary(UnaryOp::RedXor, _) => 1,
        ExprKind::Ternary(_, then_expr, _) => infer_expr_width(then_expr, ctx),
        ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
            infer_expr_width(inner, ctx)
        }
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
            reg_names: ctx.reg_names, port_names: ctx.port_names,
            let_names: ctx.let_names, inst_names: ctx.inst_names,
            wide_names: ctx.wide_names, widths: ctx.widths,
            posedge_lhs: ctx.posedge_lhs, fsm_mode: ctx.fsm_mode,
            enum_map: ctx.enum_map, bus_ports: ctx.bus_ports,
            reset_levels: ctx.reset_levels, vec_names: ctx.vec_names,
            vec_sizes: ctx.vec_sizes, fsm_vec_port_regs: ctx.fsm_vec_port_regs,
            ident_subst: None, // replaced below via a temporary binding
            coverage: ctx.coverage,
        };
        // The sub map must outlive the cpp_expr call. We keep `sub` as a
        // stack-local binding whose lifetime covers the call.
        let ctx_with_sub = Ctx { ident_subst: Some(&sub), ..sub_ctx };
        if let Some(pred) = args.first() {
            cpp_expr(pred, &ctx_with_sub)
        } else {
            String::new()
        }
    };

    match method.name.as_str() {
        "any" => {
            if n_usize == 0 { return "false".to_string(); }
            let terms: Vec<String> = (0..n as u64).map(emit_at).collect();
            format!("({})", terms.join(" || "))
        }
        "all" => {
            if n_usize == 0 { return "true".to_string(); }
            let terms: Vec<String> = (0..n as u64).map(emit_at).collect();
            format!("({})", terms.join(" && "))
        }
        "count" => {
            if n_usize == 0 { return "0".to_string(); }
            let terms: Vec<String> = (0..n as u64)
                .map(|i| format!("({} ? 1u : 0u)", emit_at(i)))
                .collect();
            format!("({})", terms.join(" + "))
        }
        "contains" => {
            let Some(x_expr) = args.first() else { return "false".to_string(); };
            let x = cpp_expr(x_expr, ctx);
            if n_usize == 0 { return "false".to_string(); }
            let terms: Vec<String> = (0..n as u64)
                .map(|i| format!("({recv_b}[{i}] == {x})")).collect();
            format!("({})", terms.join(" || "))
        }
        "reduce_or" => {
            if n_usize == 0 { return "0".to_string(); }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" | "))
        }
        "reduce_and" => {
            if n_usize == 0 { return "0".to_string(); }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" & "))
        }
        "reduce_xor" => {
            if n_usize == 0 { return "0".to_string(); }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" ^ "))
        }
        _ => format!("{recv_b}.{}()", method.name),
    }
}

fn cpp_expr(expr: &Expr, ctx: &Ctx) -> String {
    cpp_expr_inner(expr, ctx, false)
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
        },
        ExprKind::Bool(true)  => "1".to_string(),
        ExprKind::Bool(false) => "0".to_string(),

        ExprKind::Ident(name) => {
            // Vec method predicate binder: `item` / `index` are rebound per
            // iteration by the enclosing `cpp_expr` Vec-method handler.
            if let Some(sub) = ctx.ident_subst.and_then(|m| m.get(name)) {
                return sub.clone();
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
            let op_str = match op {
                BinOp::Add | BinOp::AddWrap => "+",  BinOp::Sub | BinOp::SubWrap => "-",
                BinOp::Mul | BinOp::MulWrap => "*",  BinOp::Div   => "/",
                BinOp::Mod    => "%",
                BinOp::Eq     => "==", BinOp::Neq  => "!=",
                BinOp::Lt     => "<",  BinOp::Gt   => ">",
                BinOp::Lte    => "<=", BinOp::Gte  => ">=",
                BinOp::And    => "&&", BinOp::Or   => "||",
                BinOp::BitAnd => "&",  BinOp::BitOr => "|",
                BinOp::BitXor => "^",
                BinOp::Shl    => "<<", BinOp::Shr  => ">>",
                BinOp::Implies => unreachable!(),
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

        ExprKind::Unary(op, operand) => {
            let o = cpp_expr(operand, ctx);
            match op {
                UnaryOp::Not    => format!("(!{o})"),
                UnaryOp::BitNot => {
                    // Use logical ! (clamped to 0/1) only for 1-bit/Bool signals.
                    // For wider types use bitwise ~.
                    let is_one_bit = match &operand.kind {
                        ExprKind::Ident(name) => {
                            ctx.widths.get(name.as_str()).copied().unwrap_or(32) == 1
                        }
                        _ => false,
                    };
                    if is_one_bit {
                        format!("(uint8_t)(!({o}))")
                    } else {
                        format!("(~({o}))")
                    }
                }
                UnaryOp::Neg    => format!("(-{o})"),
                UnaryOp::RedAnd => {
                    // Reduction AND: all bits set → 1
                    let w = infer_expr_width(operand, ctx);
                    if w > 128 {
                        let words = wide_words(w);
                        let last_bits = w % 32;
                        let last_mask = if last_bits == 0 { "0xFFFFFFFFU".to_string() }
                                        else { format!("0x{:X}U", (1u32 << last_bits) - 1) };
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
                    return format!("{}_{}", base_name, field.name);
                }
                if ctx.inst_names.contains(base_name.as_str()) {
                    return format!("_inst_{}.{}", base_name, field.name);
                }
            }
            // Indexed bus port: m_axi[0].valid → m_axi_0_valid
            if let ExprKind::Index(arr, idx) = &base.kind {
                if let (ExprKind::Ident(arr_name), ExprKind::Literal(LitKind::Dec(i))) = (&arr.kind, &idx.kind) {
                    let expanded = format!("{}_{}", arr_name, i);
                    if ctx.bus_ports.contains(expanded.as_str()) {
                        return format!("{}_{}_{}", arr_name, i, field.name);
                    }
                }
            }
            // Use is_lhs when evaluating base so struct reg fields get _n_ prefix on LHS
            let b = cpp_expr_inner(base, ctx, is_lhs);
            format!("{b}.{}", field.name)
        }

        ExprKind::MethodCall(base, method, args) => {
            let b = cpp_expr(base, ctx);
            match method.name.as_str() {
                "trunc" => {
                    if let Some(w_expr) = args.first() {
                        let bits = eval_width(w_expr);
                        let base_w = infer_expr_width(base, ctx);
                        if base_w > 128 && bits <= 64 {
                            // VlWide → narrow: extract low bits via word array
                            format!("({})_arch_vw_bits({b}.data(), {}, 0)", cpp_uint(bits), bits - 1)
                        } else {
                            cast_to_bits(&b, bits)
                        }
                    } else {
                        b
                    }
                }
                "zext" => {
                    if let Some(w_expr) = args.first() {
                        let bits = eval_width(w_expr);
                        let base_w = infer_expr_width(base, ctx);
                        if bits > 128 {
                            // Narrow → VlWide: use uint64_t constructor
                            let words = wide_words(bits);
                            format!("VlWide<{words}>(static_cast<uint64_t>({b}))")
                        } else if base_w > 128 && bits <= 64 {
                            format!("({})_arch_vw_bits({b}.data(), {}, 0)", cpp_uint(bits), bits - 1)
                        } else {
                            format!("({})({})", cpp_uint(bits), b)
                        }
                    } else {
                        b
                    }
                }
                "sext" => {
                    if let Some(w_expr) = args.first() {
                        let dst_bits = eval_width(w_expr);
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
                        let dst_bits = eval_width(w_expr);
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
                    let chunk = if let Some(c) = args.first() { eval_width(c) } else { 1 };
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
                "any" | "all" | "count" | "contains"
                | "reduce_or" | "reduce_and" | "reduce_xor" => {
                    lower_vec_method_cpp(&b, base, method, args, ctx)
                }
                _ => format!("{b}.{}()", method.name),
            }
        }

        ExprKind::Cast(inner, ty) => {
            let e = cpp_expr(inner, ctx);
            let t = cpp_port_type(ty);
            format!("({t})({e})")
        }

        ExprKind::Index(base, idx) => {
            let b = cpp_expr_inner(base, ctx, is_lhs);
            let i = cpp_expr(idx, ctx);
            // Vec-typed regs use C array subscript; scalar signals use bit extraction
            let is_vec = if let ExprKind::Ident(name) = &base.kind {
                ctx.vec_names.map_or(false, |s| s.contains(name.as_str()))
            } else {
                false
            };
            // Runtime bounds check (hard abort) — skip when index is a compile-time literal
            // since the type checker handles constant-bounds at compile time.
            let idx_is_const = matches!(&idx.kind, ExprKind::Literal(_));
            if is_vec {
                let limit = base_ident_name(base)
                    .and_then(|n| ctx.vec_sizes.and_then(|m| m.get(n)).copied())
                    .unwrap_or(0);
                if limit > 0 && !idx_is_const {
                    let loc = base_ident_name(base).unwrap_or("<vec>");
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
            let h = eval_width(hi);
            let l = eval_width(lo);
            let base_w = infer_expr_width(base, ctx);
            // Static slice: hi/lo are compile-time. Bounds checked by typecheck.
            if base_w > 128 {
                // VlWide<N>: use word-array bit extractor
                let result_w = h - l + 1;
                let result_ty = if result_w <= 64 { cpp_uint(result_w) } else { "uint64_t" };
                format!("({result_ty})_arch_vw_bits({b}.data(), {h}, {l})")
            } else if base_w > 64 {
                bit_range_u128(&b, h, l)
            } else {
                bit_range(&b, h, l)
            }
        }

        ExprKind::PartSelect(base, start, width, up) => {
            let b = cpp_expr(base, ctx);
            let s = cpp_expr(start, ctx);
            let w = eval_width(width);
            let base_w = infer_expr_width(base, ctx);
            let result_ty = cpp_uint(w);
            // Runtime bounds check for variable part-selects:
            //   [+:]: bits [start .. start+W-1] must fit, so (start + W - 1) < base_W
            //   [-:]: bits [start-W+1 .. start], so start < base_W and start >= W-1
            // Skip when start is a constant.
            let start_is_const = matches!(&start.kind, ExprKind::Literal(_));
            let bchk = if base_w > 0 && !start_is_const {
                let loc = base_ident_name(base).unwrap_or("<partsel>");
                if *up {
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
                let hi_expr = if *up {
                    format!("(({s}) + {w} - 1)")
                } else {
                    format!("({s})")
                };
                let lo_expr = if *up {
                    format!("({s})")
                } else {
                    format!("(({s}) - {} + 1)", w)
                };
                format!("({result_ty})_arch_vw_bits({b}.data(), {hi_expr}, {lo_expr})")
            } else if base_w > 64 {
                let mask = (1u128 << w).wrapping_sub(1);
                let mask_str = format!("0x{:x}ULL", mask as u64);
                if *up {
                    format!("({result_ty})(({b} >> ({s})) & {mask_str})")
                } else {
                    format!("({result_ty})(({b} >> (({s}) - {} + 1)) & {mask_str})", w)
                }
            } else {
                let mask = if w >= 64 { u64::MAX } else { (1u64 << w) - 1 };
                let mask_str = format!("0x{:x}ULL", mask);
                if *up {
                    format!("({result_ty})((uint64_t)({b}) >> ({s}) & {mask_str})")
                } else {
                    format!("({result_ty})((uint64_t)({b}) >> (({s}) - {} + 1) & {mask_str})", w)
                }
            };
            if bchk.is_empty() { core } else { format!("({bchk}{core})") }
        }

        ExprKind::EnumVariant(enum_name, variant) => {
            if let Some(variants) = ctx.enum_map.get(&enum_name.name) {
                let idx = variants.iter().find(|(n, _)| *n == variant.name).map(|(_, v)| *v).unwrap_or(0);
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
                format!("_ARCH_CODEGEN_ERROR_unknown_enum_{}_{}",
                        enum_name.name, variant.name)
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

        ExprKind::Concat(parts) => {
            if parts.is_empty() { return "0".to_string(); }
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
                        "_r = _r | (VlWide<{words}>(static_cast<uint64_t>({val})) << {bit_offset});"));
                    bit_offset += w;
                }
                format!("[&]() -> VlWide<{words}> {{ VlWide<{words}> _r{{}}; {} return _r; }}()",
                        stmts.join(" "))
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
        ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
            // Same-width reinterpret — C++ sim model is bitwise, no-op
            cpp_expr(inner, ctx)
        }

        ExprKind::Ternary(cond, then_expr, else_expr) => {
            let c = cpp_expr(cond, ctx);
            let t = cpp_expr(then_expr, ctx);
            let e = cpp_expr(else_expr, ctx);
            format!("(({c}) ? ({t}) : ({e}))")
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
                    Pattern::Wildcard | Pattern::Ident(_) => { result = val; continue; }
                    Pattern::Literal(e) => {
                        let lit = cpp_expr(e, ctx);
                        format!("({s} == {lit})")
                    }
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants.iter().find(|(n, _)| *n == vr.name).map(|(_, v)| *v).unwrap_or(0);
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
            let parts: Vec<String> = members.iter().map(|m| match m {
                InsideMember::Single(e) => {
                    let v = cpp_expr(e, ctx);
                    format!("({s} == {v})")
                }
                InsideMember::Range(lo, hi) => {
                    let l = cpp_expr(lo, ctx);
                    let h = cpp_expr(hi, ctx);
                    format!("({s} >= {l} && {s} <= {h})")
                }
            }).collect();
            if parts.is_empty() { "0".to_string() } else { format!("({})", parts.join(" || ")) }
        }
    }
}

// ── Statement emitters ────────────────────────────────────────────────────────

fn ind(n: usize) -> String { "  ".repeat(n) }

fn emit_reg_stmts(stmts: &[Stmt], ctx: &Ctx, out: &mut String, indent: usize) {
    for stmt in stmts {
        emit_reg_stmt(stmt, ctx, out, indent);
    }
}

fn emit_reg_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize) {
    match stmt {
        Stmt::Assign(a) => {
            // Scalar bit-indexed LHS: name[idx] = val where name is NOT a Vec
            // Emit mask-and-OR: base = (base & ~(1ULL << idx)) | (uint64_t(val & 1) << idx)
            if let ExprKind::Index(base, idx_expr) = &a.target.kind {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if !ctx.vec_names.map_or(false, |s| s.contains(base_name.as_str())) {
                        let resolved_base = ctx.resolve_name(base_name, true);
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
            let lhs = cpp_expr_lhs(&a.target, ctx);
            // Wide reg assignment from wide port: convert VlWide → _arch_u128
            let rhs = cpp_expr(&a.value, ctx);
            out.push_str(&format!("{}{}  = {};\n", ind(indent), lhs, rhs));
        }
        Stmt::IfElse(ie) => emit_reg_if_else(ie, ctx, out, indent, false),
        Stmt::Match(m) => {
            let scrut = cpp_expr(&m.scrutinee, ctx);
            out.push_str(&format!("{}switch ({}) {{\n", ind(indent), scrut));
            for arm in &m.arms {
                let case_str = match &arm.pattern {
                    Pattern::Wildcard | Pattern::Ident(_) => "default".to_string(),
                    Pattern::Literal(e) => format!("case {}", cpp_expr(e, ctx)),
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants.iter().find(|(n, _)| *n == vr.name).map(|(_, v)| *v).unwrap_or(0);
                            format!("case {idx}")
                        } else { "default".to_string() }
                    }
                };
                out.push_str(&format!("{}{}: {{\n", ind(indent + 1), case_str));
                emit_reg_stmts(&arm.body, ctx, out, indent + 2);
                out.push_str(&format!("{}  break;\n", ind(indent + 1)));
                out.push_str(&format!("{}}}\n", ind(indent + 1)));
            }
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        Stmt::Log(l) => emit_log_stmt(l, ctx, out, indent),
        Stmt::For(f) => {
            let var = &f.var.name;
            match &f.range {
                ForRange::Range(rs, re) => {
                    let start = cpp_expr(rs, ctx);
                    let end = cpp_expr(re, ctx);
                    out.push_str(&format!("{}for (int {var} = {start}; {var} <= {end}; {var}++) {{\n", ind(indent)));
                    for s in &f.body { emit_reg_stmt(s, ctx, out, indent + 1); }
                    out.push_str(&format!("{}}}\n", ind(indent)));
                }
                ForRange::ValueList(vals) => {
                    for v in vals {
                        let val = cpp_expr(v, ctx);
                        out.push_str(&format!("{}{{\n", ind(indent)));
                        out.push_str(&format!("{}int {var} = {val};\n", ind(indent + 1)));
                        for s in &f.body { emit_reg_stmt(s, ctx, out, indent + 1); }
                        out.push_str(&format!("{}}}\n", ind(indent)));
                    }
                }
            }
        }
        Stmt::Init(ib) => {
            let rst_name = &ib.reset_signal.name;
            let is_low = ctx.reset_levels.get(rst_name.as_str())
                .map_or(false, |level| *level == ResetLevel::Low);
            let cond = if is_low {
                format!("(!{})", rst_name)
            } else {
                rst_name.clone()
            };
            out.push_str(&format!("{}if ({}) {{\n", ind(indent), cond));
            emit_reg_stmts(&ib.body, ctx, out, indent + 1);
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => {
            panic!("pipeline wait-stages not yet supported in sim")
        }
    }
}

fn emit_reg_if_else(ie: &IfElse, ctx: &Ctx, out: &mut String, indent: usize, is_chain: bool) {
    let cond = cpp_expr(&ie.cond, ctx);
    if is_chain {
        out.push_str(&format!("{}}} else if ({}) {{\n", ind(indent), cond));
    } else {
        out.push_str(&format!("{}if ({}) {{\n", ind(indent), cond));
    }
    // --coverage: count entries to this arm. Phase 1 records branch
    // coverage for seq if/elsif/else; phase 1b adds comb. Counter id is
    // the alloc order in the per-class registry.
    if let Some(reg) = ctx.coverage {
        let kind = if is_chain { "elsif" } else { "if" };
        let idx = reg.borrow_mut().alloc(kind, ie.cond.span.start, String::new());
        out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
    }
    emit_reg_stmts(&ie.then_stmts, ctx, out, indent + 1);
    if ie.else_stmts.len() == 1 {
        if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
            emit_reg_if_else(nested, ctx, out, indent, true);
            return;
        }
    }
    if !ie.else_stmts.is_empty() {
        out.push_str(&format!("{}}} else {{\n", ind(indent)));
        if let Some(reg) = ctx.coverage {
            let idx = reg.borrow_mut().alloc("else", ie.span.end, String::new());
            out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
        }
        emit_reg_stmts(&ie.else_stmts, ctx, out, indent + 1);
    }
    out.push_str(&format!("{}}}\n", ind(indent)));
}

fn emit_comb_stmts(stmts: &[CombStmt], ctx: &Ctx, out: &mut String, indent: usize) {
    for stmt in stmts {
        emit_comb_stmt(stmt, ctx, out, indent);
    }
}

fn emit_comb_stmt(stmt: &CombStmt, ctx: &Ctx, out: &mut String, indent: usize) {
    match stmt {
        CombStmt::Assign(a) => {
            // Scalar bit-indexed LHS: name[idx] = val where name is NOT a Vec
            // Emit mask-and-OR: base = (base & ~(1ULL << idx)) | (uint64_t(val & 1) << idx)
            if let ExprKind::Index(base, idx_expr) = &a.target.kind {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if !ctx.vec_names.map_or(false, |s| s.contains(base_name.as_str())) {
                        let resolved_base = ctx.resolve_name(base_name, false);
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
            let rhs = cpp_expr(&a.value, ctx);
            let target_name = if let ExprKind::Ident(name) = &a.target.kind { name.clone() } else { cpp_expr(&a.target, ctx) };
            let resolved_target = ctx.resolve_name(&target_name, false);
            // Wide output port: may need conversion depending on width
            if ctx.wide_names.contains(target_name.as_str()) {
                let bits = ctx.widths.get(target_name.as_str()).copied().unwrap_or(0);
                if bits > 128 {
                    // >128 bits: both internal and port are VlWide<N> — direct assignment
                    out.push_str(&format!("{}{} = {};\n", ind(indent), target_name, rhs));
                } else {
                    // 65–128 bits: internal is _arch_u128, port is VlWide<4>
                    out.push_str(&format!("{}  _arch_u128_to_vl({}, {}._data);\n",
                        ind(indent), rhs, target_name));
                }
            } else {
                out.push_str(&format!("{}{}  = {};\n", ind(indent), resolved_target, rhs));
            }
        }
        CombStmt::IfElse(ie) => emit_comb_if_else(ie, ctx, out, indent, false),
        CombStmt::MatchExpr(m) => {
            let scrut = cpp_expr(&m.scrutinee, ctx);
            out.push_str(&format!("{}switch ({}) {{\n", ind(indent), scrut));
            for arm in &m.arms {
                let case_str = match &arm.pattern {
                    Pattern::Wildcard | Pattern::Ident(_) => "default".to_string(),
                    Pattern::Literal(e) => format!("case {}", cpp_expr(e, ctx)),
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants.iter().find(|(n, _)| *n == vr.name).map(|(_, v)| *v).unwrap_or(0);
                            format!("case {idx}")
                        } else { "default".to_string() }
                    }
                };
                out.push_str(&format!("{}{}: {{\n", ind(indent + 1), case_str));
                for s in &arm.body {
                    if let Stmt::Assign(a) = s {
                        let rhs = cpp_expr(&a.value, ctx);
                        let lhs = cpp_expr(&a.target, ctx);
                        out.push_str(&format!("{}{} = {};\n", ind(indent + 2), lhs, rhs));
                    }
                }
                out.push_str(&format!("{}  break;\n", ind(indent + 1)));
                out.push_str(&format!("{}}}\n", ind(indent + 1)));
            }
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        CombStmt::Log(l) => emit_log_stmt(l, ctx, out, indent),
        CombStmt::For(f) => {
            let var = &f.var.name;
            match &f.range {
                ForRange::Range(rs, re) => {
                    let start = cpp_expr(rs, ctx);
                    let end = cpp_expr(re, ctx);
                    out.push_str(&format!("{}for (int {var} = {start}; {var} <= {end}; {var}++) {{\n", ind(indent)));
                    for s in &f.body { emit_reg_stmt(s, ctx, out, indent + 1); }
                    out.push_str(&format!("{}}}\n", ind(indent)));
                }
                ForRange::ValueList(vals) => {
                    for v in vals {
                        let val = cpp_expr(v, ctx);
                        out.push_str(&format!("{}{{\n", ind(indent)));
                        out.push_str(&format!("{}int {var} = {val};\n", ind(indent + 1)));
                        for s in &f.body { emit_reg_stmt(s, ctx, out, indent + 1); }
                        out.push_str(&format!("{}}}\n", ind(indent)));
                    }
                }
            }
        }
    }
}

fn emit_comb_if_else(ie: &CombIfElse, ctx: &Ctx, out: &mut String, indent: usize, is_chain: bool) {
    let cond = cpp_expr(&ie.cond, ctx);
    if is_chain {
        out.push_str(&format!("{}}} else if ({}) {{\n", ind(indent), cond));
    } else {
        out.push_str(&format!("{}if ({}) {{\n", ind(indent), cond));
    }
    // --coverage phase 1c: same instrumentation as emit_reg_if_else for
    // comb if/elsif/else arms. Note that comb blocks may evaluate
    // multiple times per cycle during the settle loop — counters
    // therefore reflect "branch entries", not "cycles where branch was
    // active". For most arch designs the settle loop converges in 1-2
    // iterations so this is close to the cycle count.
    if let Some(reg) = ctx.coverage {
        let kind = if is_chain { "elsif" } else { "if" };
        let idx = reg.borrow_mut().alloc(kind, ie.cond.span.start, String::new());
        out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
    }
    emit_comb_stmts(&ie.then_stmts, ctx, out, indent + 1);
    if ie.else_stmts.len() == 1 {
        if let CombStmt::IfElse(nested) = &ie.else_stmts[0] {
            emit_comb_if_else(nested, ctx, out, indent, true);
            return;
        }
    }
    if !ie.else_stmts.is_empty() {
        out.push_str(&format!("{}}} else {{\n", ind(indent)));
        if let Some(reg) = ctx.coverage {
            let idx = reg.borrow_mut().alloc("else", ie.span.end, String::new());
            out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
        }
        emit_comb_stmts(&ie.else_stmts, ctx, out, indent + 1);
    }
    out.push_str(&format!("{}}}\n", ind(indent)));
}

fn emit_log_stmt(l: &LogStmt, ctx: &Ctx, out: &mut String, indent: usize) {
    let args_str: String = l.args.iter()
        .map(|a| format!(", (long long)({})", cpp_expr(a, ctx)))
        .collect();
    let fmt = sv_fmt_to_printf(&l.fmt);
    let print_line = if let Some(ref path) = l.file {
        let fd_name = log_fd_name(path);
        format!(
            "{}if ({fd_name}) fprintf({fd_name}, \"[{}][{}] {}\\n\"{});",
            ind(indent), l.level.name(), l.tag, fmt, args_str
        )
    } else {
        format!(
            "{}printf(\"[{}][{}] {}\\n\"{});",
            ind(indent), l.level.name(), l.tag, fmt, args_str
        )
    };
    if l.level == LogLevel::Always {
        out.push_str(&print_line);
        out.push('\n');
    } else {
        out.push_str(&format!(
            "{}if (Verilated::verbosity() >= {}) {{ {} }}\n",
            ind(indent), l.level.value(), print_line
        ));
    }
}

/// Generate a C++ file pointer name from a log file path.
fn log_fd_name(path: &str) -> String {
    let clean: String = path.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    format!("_log_fd_{clean}")
}

/// Collect unique log file paths from module body (comb + seq blocks).
fn collect_log_files(body: &[ModuleBodyItem]) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    fn from_comb(stmts: &[CombStmt], files: &mut Vec<String>, seen: &mut HashSet<String>) {
        for s in stmts {
            match s {
                CombStmt::Log(l) => { if let Some(ref p) = l.file { if seen.insert(p.clone()) { files.push(p.clone()); } } }
                CombStmt::IfElse(ie) => { from_comb(&ie.then_stmts, files, seen); from_comb(&ie.else_stmts, files, seen); }
                CombStmt::MatchExpr(m) => { for arm in &m.arms { from_seq(&arm.body, files, seen); } }
                _ => {}
            }
        }
    }
    fn from_seq(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut HashSet<String>) {
        for s in stmts {
            match s {
                Stmt::Log(l) => { if let Some(ref p) = l.file { if seen.insert(p.clone()) { files.push(p.clone()); } } }
                Stmt::IfElse(ie) => { from_seq(&ie.then_stmts, files, seen); from_seq(&ie.else_stmts, files, seen); }
                Stmt::Match(m) => { for arm in &m.arms { from_seq(&arm.body, files, seen); } }
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
        .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i { Some(r.name.name.clone()) } else { None })
        .chain(ports.iter().filter_map(|p| {
            if p.reg_info.is_some() { Some(p.name.name.clone()) } else { None }
        }))
        .collect()
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
            ModuleBodyItem::WireDecl(w) => { out.insert(w.name.name.clone()); }
            _ => {}
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
fn collect_comb_reads(stmt: &CombStmt, out: &mut std::collections::BTreeSet<String>) {
    match stmt {
        CombStmt::Assign(a) => collect_expr_idents(&a.value, out),
        CombStmt::IfElse(ie) => {
            collect_expr_idents(&ie.cond, out);
            for s in &ie.then_stmts { collect_comb_reads(s, out); }
            for s in &ie.else_stmts { collect_comb_reads(s, out); }
        }
        CombStmt::MatchExpr(_) | CombStmt::Log(_) => {}
        CombStmt::For(f) => {
            for s in &f.body {
                if let Stmt::Assign(a) = s {
                    collect_expr_idents(&a.value, out);
                }
            }
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
            for s in &ie.then_stmts { collect_stmt_idents(s, out); }
            for s in &ie.else_stmts { collect_stmt_idents(s, out); }
        }
        Stmt::Match(m) => {
            collect_expr_idents(&m.scrutinee, out);
            for arm in &m.arms {
                for s in &arm.body { collect_stmt_idents(s, out); }
            }
        }
        Stmt::For(f) => {
            if let ForRange::Range(lo, hi) = &f.range {
                collect_expr_idents(lo, out);
                collect_expr_idents(hi, out);
            } else if let ForRange::ValueList(vs) = &f.range {
                for v in vs { collect_expr_idents(v, out); }
            }
            for s in &f.body { collect_stmt_idents(s, out); }
        }
        Stmt::Init(ib) => {
            for s in &ib.body { collect_stmt_idents(s, out); }
        }
        Stmt::WaitUntil(e, _) => collect_expr_idents(e, out),
        Stmt::DoUntil { body, cond, .. } => {
            for s in body { collect_stmt_idents(s, out); }
            collect_expr_idents(cond, out);
        }
        Stmt::Log(_) => {}
    }
}

fn collect_expr_idents(expr: &Expr, out: &mut std::collections::BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Ident(name) => { out.insert(name.clone()); }
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
            for a in args { collect_expr_idents(a, out); }
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args { collect_expr_idents(a, out); }
        }
        ExprKind::Ternary(cond, then_e, else_e) => {
            collect_expr_idents(cond, out);
            collect_expr_idents(then_e, out);
            collect_expr_idents(else_e, out);
        }
        ExprKind::ExprMatch(scrut, arms) => {
            collect_expr_idents(scrut, out);
            for arm in arms { collect_expr_idents(&arm.value, out); }
        }
        _ => {}
    }
}

fn collect_inst_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| if let ModuleBodyItem::Inst(inst) = i { Some(inst.name.name.clone()) } else { None })
        .collect()
}

/// Collect all sub-instance output signal names (auto-declared wires).
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
    fn collect_stmt_targets(stmt: &CombStmt, out: &mut HashSet<String>) {
        match stmt {
            CombStmt::Assign(a) => { if let ExprKind::Ident(name) = &a.target.kind { out.insert(name.clone()); } }
            CombStmt::IfElse(ie) => {
                for s in &ie.then_stmts { collect_stmt_targets(s, out); }
                for s in &ie.else_stmts { collect_stmt_targets(s, out); }
            }
            CombStmt::MatchExpr(m) => {
                for arm in &m.arms {
                    for s in &arm.body {
                        if let Stmt::Assign(a) = s {
                            if let ExprKind::Ident(name) = &a.target.kind {
                                out.insert(name.clone());
                            }
                        }
                    }
                }
            }
            CombStmt::Log(_) => {}
            CombStmt::For(f) => {
                for s in &f.body {
                    if let Stmt::Assign(a) = s {
                        if let ExprKind::Ident(name) = &a.target.kind {
                            out.insert(name.clone());
                        }
                    }
                }
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
                    Some((sig.name.clone(), *kind == ResetKind::Async, *level == ResetLevel::Low))
                } else { None }
            } else { None }
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
            let entries: Vec<(String, u64)> = info.variants.iter().enumerate()
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

/// Build a name→width map from module ports, regs, and lets.
fn build_widths(ports: &[PortDecl], body: &[ModuleBodyItem]) -> HashMap<String, u32> {
    let mut m = HashMap::new();
    for p in ports {
        m.insert(p.name.name.clone(), type_bits_te(&p.ty));
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => { m.insert(r.name.name.clone(), type_bits_te(&r.ty)); }
            ModuleBodyItem::LetBinding(l) => {
                // Destructuring: widths come from struct field types; these
                // are best-effort looked up at emission time. Leave them
                // out here; widths map defaults kick in if needed.
                if !l.destructure_fields.is_empty() {
                    continue;
                }
                if let Some(ty) = &l.ty {
                    m.insert(l.name.name.clone(), type_bits_te(ty));
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

fn type_bits_te(ty: &TypeExpr) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
        TypeExpr::Bool | TypeExpr::Bit => 1,
        _ => 32,
    }
}

/// Collect names whose bit width exceeds 64 (require wide handling).
fn collect_wide_names(ports: &[PortDecl], body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut s = HashSet::new();
    for p in ports {
        if type_bits_te(&p.ty) > 64 { s.insert(p.name.name.clone()); }
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if type_bits_te(&r.ty) > 64 { s.insert(r.name.name.clone()); }
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(ty) = &l.ty {
                    if type_bits_te(ty) > 64 { s.insert(l.name.name.clone()); }
                }
            }
            _ => {}
        }
    }
    // Resolve pipe_reg wide from source
    let widths = build_widths(ports, body);
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

        for f in fns {
            let ret_ty = cpp_internal_type(&f.ret_ty);
            let args_str: Vec<String> = f.args.iter()
                .map(|a| format!("{} {}", cpp_internal_type(&a.ty), a.name.name))
                .collect();
            h.push_str(&format!("inline {ret_ty} {}({}) {{\n", f.name.name, args_str.join(", ")));

            let empty_regs:  HashSet<String> = HashSet::new();
            let empty_lets:  HashSet<String> = HashSet::new();
            let empty_insts: HashSet<String> = HashSet::new();
            let empty_wide:  HashSet<String> = HashSet::new();
            let empty_w:     HashMap<String, u32> = HashMap::new();
            let enum_map    = build_enum_map(self.symbols);

            // Build arg names as "port" names (so they're used as-is)
            let arg_ports: HashSet<String> = f.args.iter().map(|a| a.name.name.clone()).collect();
            let empty_bus: HashSet<String> = HashSet::new();
            let ctx = Ctx::new(&empty_regs, &arg_ports, &empty_lets, &empty_insts,
                               &empty_wide, &empty_w, &enum_map, &empty_bus);

            for item in &f.body {
                match item {
                    FunctionBodyItem::Let(l) => {
                        let ty = l.ty.as_ref().map(|t| cpp_internal_type(t))
                            .unwrap_or_else(|| "uint32_t".to_string());
                        let val = cpp_expr(&l.value, &ctx);
                        h.push_str(&format!("  const {ty} {} = {};\n", l.name.name, val));
                    }
                    FunctionBodyItem::IfElse(_) | FunctionBodyItem::For(_) | FunctionBodyItem::Assign(_) => {
                        // TODO: emit C++ for if/for/assign in sim functions
                        // For now, these are only used in SV codegen
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
                                            let idx = variants.iter().find(|(n, _)| *n == vr.name).map(|(_, v)| *v).unwrap_or(0);
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
            impl_: String::new(),  // header-only
        }
    }
}

// ── Module codegen ────────────────────────────────────────────────────────────

fn collect_stmt_assigns(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign(a) => {
                if let ExprKind::Ident(n) = &a.target.kind { out.insert(n.clone()); }
                // Struct-field LHS (`reg.field <= ...`): collect the base reg name.
                if let ExprKind::FieldAccess(base, _) = &a.target.kind {
                    if let ExprKind::Ident(n) = &base.kind { out.insert(n.clone()); }
                }
            }
            Stmt::IfElse(ie) => {
                collect_stmt_assigns(&ie.then_stmts, out);
                collect_stmt_assigns(&ie.else_stmts, out);
            }
            Stmt::Match(m) => {
                for arm in &m.arms { collect_stmt_assigns(&arm.body, out); }
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
                Item::Module(m)       if m.name.name == module_name => Some(&m.ports),
                Item::Fsm(f)          if f.name.name == module_name => Some(&f.ports),
                Item::Fifo(f)         if f.name.name == module_name => Some(&f.ports),
                Item::Ram(r)          if r.name.name == module_name => Some(&r.ports),
                Item::Cam(c)          if c.name.name == module_name => Some(&c.ports),
                Item::Counter(c)      if c.name.name == module_name => Some(&c.ports),
                Item::Arbiter(a)      if a.name.name == module_name => Some(&a.ports),
                Item::Regfile(r)      if r.name.name == module_name => Some(&r.ports),
                Item::Pipeline(p)     if p.name.name == module_name => Some(&p.ports),
                Item::Linklist(l)     if l.name.name == module_name => Some(&l.ports),
                Item::Synchronizer(s) if s.name.name == module_name => Some(&s.ports),
                Item::Clkgate(c)      if c.name.name == module_name => Some(&c.ports),
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
                Item::Module(m)       if m.name.name == module_name => Some(&m.params),
                Item::Fsm(f)          if f.name.name == module_name => Some(&f.params),
                Item::Fifo(f)         if f.name.name == module_name => Some(&f.params),
                Item::Ram(r)          if r.name.name == module_name => Some(&r.params),
                Item::Cam(c)          if c.name.name == module_name => Some(&c.params),
                Item::Counter(c)      if c.name.name == module_name => Some(&c.params),
                Item::Arbiter(a)      if a.name.name == module_name => Some(&a.params),
                Item::Regfile(r)      if r.name.name == module_name => Some(&r.params),
                Item::Pipeline(p)     if p.name.name == module_name => Some(&p.params),
                Item::Linklist(l)     if l.name.name == module_name => Some(&l.params),
                Item::Synchronizer(s) if s.name.name == module_name => Some(&s.params),
                Item::Clkgate(c)      if c.name.name == module_name => Some(&c.params),
                _ => None,
            };
            if let Some(p) = params {
                return p.clone();
            }
        }
        Vec::new()
    }

    fn gen_module(&self, m: &ModuleDecl, emit_debug: bool, debug_module_set: &std::collections::HashSet<String>) -> SimModel {
        let name = &m.name.name;
        let class = format!("V{name}");
        let enum_map = build_enum_map(self.symbols);

        // --coverage: per-module branch-coverage registry. emit_reg_if_else
        // and (later phase 1b) emit_comb_if_else allocate counter ids here.
        // Threaded into Ctx via .with_coverage(Some(&cov_reg)).
        let cov_reg: std::cell::RefCell<CoverageRegistry> = std::cell::RefCell::new(CoverageRegistry::default());
        let cov_handle: Option<&std::cell::RefCell<CoverageRegistry>> =
            if self.coverage { Some(&cov_reg) } else { None };

        // Collect bus port names and flattened signals (with direction for debug)
        let mut bus_port_names: HashSet<String> = HashSet::new();
        let mut bus_flat: Vec<(String, TypeExpr)> = Vec::new();
        let mut bus_flat_dirs: HashMap<String, Direction> = HashMap::new();
        for p in &m.ports {
            if let Some(ref bi) = p.bus_info {
                bus_port_names.insert(p.name.name.clone());
                let with_dir = flatten_bus_port_with_dir(&p.name.name, bi, self.symbols);
                for (fname, fdir, fty) in with_dir {
                    bus_flat_dirs.insert(fname.clone(), fdir);
                    bus_flat.push((fname, fty));
                }
            }
        }

        let mut port_names: HashSet<String> = m.ports.iter()
            .filter(|p| p.bus_info.is_none())
            .map(|p| p.name.name.clone())
            .collect();
        // Add flattened bus signal names to port_names
        for (flat_name, _) in &bus_flat {
            port_names.insert(flat_name.clone());
        }

        // Collect reset port levels for `.asserted` polarity abstraction
        let reset_levels: HashMap<String, ResetLevel> = m.ports.iter()
            .filter_map(|p| if let TypeExpr::Reset(_, level) = &p.ty {
                Some((p.name.name.clone(), *level))
            } else { None })
            .collect();

        let mut reg_names = collect_reg_names(&m.body, &m.ports);
        reg_names.extend(collect_pipe_reg_names(&m.body));
        let let_names   = collect_let_names(&m.body);
        let inst_names  = collect_inst_names(&m.body);
        let inst_out    = collect_inst_output_signals(&m.body);
        let mut wide_names  = collect_wide_names(&m.ports, &m.body);
        let mut widths      = build_widths(&m.ports, &m.body);

        // Add bus flattened signals to wide_names and widths
        for (flat_name, flat_ty) in &bus_flat {
            let bits = type_bits_te(flat_ty);
            widths.insert(flat_name.clone(), bits);
            if bits > 64 { wide_names.insert(flat_name.clone()); }
        }

        // Populate widths with per-struct-field keys: "ctrl_r.mode" → 4, etc.
        // Required for concat-width inference when struct fields appear inside
        // a concat expression (the default `unwrap_or(8)` silently corrupts
        // readback shifts otherwise).
        let struct_decls: HashMap<&str, &StructDecl> = {
            let mut map: HashMap<&str, &StructDecl> = HashMap::new();
            for item in &self.source.items {
                match item {
                    Item::Struct(s) => { map.insert(s.name.name.as_str(), s); }
                    Item::Package(p) => {
                        for s in &p.structs { map.insert(s.name.name.as_str(), s); }
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
            if let ModuleBodyItem::RegDecl(r) = item {
                if let Some(n) = named_or_vec_named(&r.ty) {
                    struct_typed_names.push((r.name.name.clone(), n.name.as_str()));
                }
            }
        }
        for (instance_name, struct_name) in &struct_typed_names {
            if let Some(sd) = struct_decls.get(struct_name) {
                for f in &sd.fields {
                    widths.insert(
                        format!("{instance_name}.{}", f.name.name),
                        type_bits_te(&f.ty),
                    );
                }
            }
        }

        // Vec-typed reg names (use C array subscript `[i]` instead of bit extraction)
        let mut vec_reg_names: HashSet<String> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i {
                if matches!(r.ty, TypeExpr::Vec(..)) { Some(r.name.name.clone()) } else { None }
            } else { None })
            .collect();

        // Vec-typed wires also use C-array indexing internally
        let vec_wire_names: HashSet<String> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::WireDecl(w) = i {
                if matches!(w.ty, TypeExpr::Vec(..)) { Some(w.name.name.clone()) } else { None }
            } else { None })
            .collect();
        vec_reg_names.extend(vec_wire_names.iter().cloned());

        // Vec wire/reg name → element count (for expanding inst port connections)
        let mut vec_wire_counts: HashMap<String, u64> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::WireDecl(w) = i {
                if let TypeExpr::Vec(_, count_expr) = &w.ty {
                    Some((w.name.name.clone(), eval_const_expr(count_expr)))
                } else { None }
            } else { None })
            .collect();
        // Also include Vec regs (for inst port connections like Vec reg → inst Vec output)
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let TypeExpr::Vec(_, count_expr) = &r.ty {
                    vec_wire_counts.insert(r.name.name.clone(), eval_const_expr(count_expr));
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
        let vec_port_infos: Vec<VecPortInfo> = m.ports.iter()
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
        let vec_port_names: HashSet<String> = vec_port_infos.iter().map(|v| v.name.clone()).collect();
        // Vec ports also use C array subscript `[i]` internally
        vec_reg_names.extend(vec_port_names.iter().cloned());
        // Unified Vec<T,N> size map: wires + regs + ports. Used by bounds-check codegen.
        let mut vec_sizes: HashMap<String, u64> = vec_wire_counts.clone();
        for vi in &vec_port_infos {
            vec_sizes.insert(vi.name.clone(), vi.count);
        }

        // Collect reset-none reg names for --check-uninit + any guarded reg (regardless
        // of reset) so Check A can use _<name>_vinit to detect producer bugs.
        let mut uninit_regs: HashSet<String> = if self.check_uninit {
            m.body.iter()
                .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i {
                    if matches!(r.reset, RegReset::None) || r.guard.is_some() {
                        Some(r.name.name.clone())
                    } else { None }
                } else { None })
                .chain(m.ports.iter().filter_map(|p| {
                    if let Some(ri) = &p.reg_info {
                        if matches!(ri.reset, RegReset::None) || ri.guard.is_some() {
                            Some(p.name.name.clone())
                        } else { None }
                    } else { None }
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
                let Some(ref bi) = p.bus_info else { continue; };
                let Some(crate::resolve::Symbol::Bus(info)) =
                    self.symbols.globals.get(&bi.bus_name.name).map(|(s, _)| s)
                    else { continue; };
                // Build param map: bus defaults, overridden by port-site params.
                let mut param_map: std::collections::HashMap<String, &Expr> =
                    info.params.iter()
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
                    if !matches!(actual_dir, Direction::In) { continue; }
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
        //   ready_only                              -> no guard (silent on all reads)
        //   req_ack_2phase                          -> deferred (stateful toggle)
        let mut payload_guards: HashMap<String, String> = HashMap::new();
        if self.inputs_start_uninit {
            for p in m.ports.iter() {
                let Some(ref bi) = p.bus_info else { continue; };
                let Some(crate::resolve::Symbol::Bus(info)) =
                    self.symbols.globals.get(&bi.bus_name.name).map(|(s, _)| s)
                    else { continue; };
                for hs in &info.handshakes {
                    let guard_sig = match hs.variant.name.as_str() {
                        "valid_ready" | "valid_only" | "valid_stall" => "valid",
                        "req_ack_4phase" => "req",
                        _ => continue, // ready_only / req_ack_2phase: no guard
                    };
                    let guard_flat = format!("{}_{}_{}", p.name.name, hs.name.name, guard_sig);
                    for payload in &hs.payload_names {
                        let payload_flat = format!("{}_{}_{}", p.name.name, hs.name.name, payload.name);
                        payload_guards.insert(payload_flat, guard_flat.clone());
                    }
                }
            }
        }

        // Collect guard-annotated regs: reg_name → guard_signal_name.
        // Used for Check A (producer bug: "guard asserts but reg never written").
        let guarded_regs: HashMap<String, String> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i {
                r.guard.as_ref().map(|g| (r.name.name.clone(), g.name.clone()))
            } else { None })
            .chain(m.ports.iter().filter_map(|p| {
                p.reg_info.as_ref().and_then(|ri| {
                    ri.guard.as_ref().map(|g| (p.name.name.clone(), g.name.clone()))
                })
            }))
            .collect();

        // Also include inst_out in "known" names for the wide set and widths
        // (they come from sub-inst ports — we'll default them to uint32_t for now)

        let insts: Vec<&InstDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::Inst(inst) = i { Some(inst) } else { None })
            .collect();

        // Bus-typed wires in this module — needed by expand_bus_connections so
        // that `child_port -> bus_wire` emits struct-field-access exprs instead
        // of flat `<wire>_<field>` idents (which would dangle; bus wires are
        // declared as a C++ struct field, not as N flat fields).
        let bus_wire_names: HashSet<String> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::WireDecl(w) = i {
                if let TypeExpr::Named(id) = &w.ty {
                    if matches!(self.symbols.globals.get(&id.name),
                                Some((crate::resolve::Symbol::Bus(_), _))) {
                        return Some(w.name.name.clone());
                    }
                }
                None
            } else { None })
            .collect();

        // Pre-expand bus connections: whole-bus connections like `axi_rd -> m_axi_mm2s`
        // are expanded to per-signal connections using the bus definition.
        let expanded_conns: Vec<Vec<Connection>> = insts.iter()
            .map(|inst| expand_bus_connections(inst, self.source, self.symbols, &bus_wire_names))
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
        for conns in &expanded_conns {
            for conn in conns {
                if let ExprKind::Ident(name) = &conn.signal.kind {
                    inst_out.insert(name.clone());
                }
            }
        }
        // Also populate `widths` for implicit-bus-wire signals so the
        // private member emission picks the right C++ type (e.g. uint64_t
        // for a 64-bit `send_data` instead of the uint32_t fallback).
        for inst in insts.iter() {
            for p in &m.ports { let _ = p; }  // placate borrow-check noise
            for sub_port in self.lookup_inst_ports(&inst.module_name.name) {
                let Some(bi) = &sub_port.bus_info else { continue; };
                let Some((crate::resolve::Symbol::Bus(info), _)) =
                    self.symbols.globals.get(&bi.bus_name.name) else { continue; };
                // Find the parent-side connection name for this bus port.
                let parent_name = inst.connections.iter()
                    .find(|c| c.port_name.name == sub_port.name.name)
                    .and_then(|c| if let ExprKind::Ident(n) = &c.signal.kind {
                        Some(n.clone())
                    } else { None });
                let Some(parent_name) = parent_name else { continue; };
                let mut pm = info.default_param_map();
                for pa in &bi.params { pm.insert(pa.name.name.clone(), &pa.value); }
                for (sname, _sdir, ty) in info.effective_signals(&pm) {
                    let bits = type_bits_te(&ty);
                    widths.entry(format!("{parent_name}_{sname}")).or_insert(bits);
                }
            }
        }

        // Build map: parent_signal_name → Vec element count for inst-output Vec ports.
        // When a sub-instance has a Vec output port and the parent connects it to a scalar
        // wire (e.g. thread lowering creates `thread_complete -> thread_complete`), we need
        // to emit flat fields and element-by-element copies instead of scalar assignments.
        let mut inst_vec_out: HashMap<String, (String, u64)> = HashMap::new();  // sig → (elem_ty, count)
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
                        if let Some(port) = sub_ports.iter().find(|p| p.name.name == conn.port_name.name) {
                            if let Some((elem_ty, count_str)) = vec_array_info_with_params(&port.ty, &sub_params) {
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
                "uint8_t" => 8, "uint16_t" => 16, "uint32_t" => 32, "uint64_t" => 64,
                "int8_t" => 8, "int16_t" => 16, "int32_t" => 32, "int64_t" => 64,
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

        // Determine if there are any functions defined in the same source file
        let has_functions = self.source.items.iter().any(|i| matches!(i, Item::Function(_)));

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
        let has_structs = m.body.iter().any(|i| matches!(i, ModuleBodyItem::RegDecl(r) if ty_references_named(&r.ty)))
            || m.ports.iter().any(|p| ty_references_named(&p.ty));
        let mut h = String::new();
        h.push_str(&format!("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n"));
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
            if matches!(p.kind, ParamKind::Const | ParamKind::WidthConst(..)) {
                if let Some(ref def) = p.default {
                    let val = eval_const_expr(def);
                    h.push_str(&format!("#ifndef {}\n#define {} {val}ULL\n#endif\n", p.name.name, p.name.name));
                }
            }
        }
        h.push('\n');
        h.push_str(&format!("class {class} {{\npublic:\n"));

        // Public port fields (bus ports are flattened; Vec ports become N flat fields)
        for p in &m.ports {
            if p.bus_info.is_some() { continue; }
            if let Some(vi) = vec_port_infos.iter().find(|v| v.name == p.name.name) {
                // Emit N flat fields: name_0, name_1, ..., name_N-1
                for i in 0..vi.count {
                    h.push_str(&format!("  {} {}_{i};\n", vi.elem_ty, vi.name));
                }
            } else {
                let ty = cpp_port_type(&p.ty);
                h.push_str(&format!("  {ty} {};\n", p.name.name));
            }
        }
        for (flat_name, flat_ty) in &bus_flat {
            let ty = cpp_port_type(flat_ty);
            h.push_str(&format!("  {ty} {flat_name};\n"));
        }
        h.push('\n');

        // Constructor — build init list. Struct-typed ports get the default
        // ctor (`name()`); scalar ports get `name(0)`.
        let mut port_inits: Vec<String> = m.ports.iter()
            .filter(|p| p.bus_info.is_none() && !wide_names.contains(&p.name.name)
                        && !vec_port_names.contains(&p.name.name))
            .map(|p| {
                if matches!(p.ty, TypeExpr::Named(_)) {
                    format!("{}()", p.name.name)
                } else {
                    format!("{}(0)", p.name.name)
                }
            })
            .collect();
        // Add flat Vec port field inits (name_0(0), name_1(0), ...)
        for vi in &vec_port_infos {
            for i in 0..vi.count {
                port_inits.push(format!("{}_{i}(0)", vi.name));
            }
        }
        // Add flattened bus signal inits
        for (flat_name, _) in &bus_flat {
            if !wide_names.contains(flat_name) {
                port_inits.push(format!("{flat_name}(0)"));
            }
        }
        // Collect Vec-array regs that need memset in constructor body
        let mut vec_reg_inits: Vec<String> = m.body.iter()
            .filter_map(|i| {
                if let ModuleBodyItem::RegDecl(r) = i {
                    if vec_array_info(&r.ty).is_some() {
                        let n = &r.name.name;
                        Some(format!("    memset(_{n}, 0, sizeof(_{n}));"))
                    } else { None }
                } else { None }
            })
            .collect();
        // Add memset for Vec port internal arrays
        for vi in &vec_port_infos {
            let n = &vi.name;
            vec_reg_inits.push(format!("    memset(_{n}, 0, sizeof(_{n}));"));
        }

        let reg_inits: Vec<String> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i {
                if vec_array_info(&r.ty).is_some() {
                    None  // handled via memset in constructor body
                } else if matches!(r.ty, TypeExpr::Named(_)) {
                    Some(format!("_{}()", r.name.name))  // struct default constructor
                } else if wide_names.contains(&r.name.name) {
                    Some(format!("_{}()", r.name.name))  // VlWide or _arch_u128 zero-inits
                } else {
                    let init_val = if let Some(ref init_expr) = r.init {
                        match &init_expr.kind {
                            ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                            ExprKind::Literal(LitKind::Hex(v)) => format!("0x{:X}", v),
                            ExprKind::Literal(LitKind::Bin(v)) => v.to_string(),
                            ExprKind::Literal(LitKind::Sized(_, v)) => v.to_string(),
                            ExprKind::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                            _ => "0".to_string(),
                        }
                    } else {
                        "0".to_string()
                    };
                    Some(format!("_{}({})", r.name.name, init_val))
                }
            } else { None })
            .collect();
        // port reg shadow inits (skip Vec port-regs — they use memset in ctor body)
        let port_reg_inits: Vec<String> = m.ports.iter()
            .filter_map(|p| {
                let ri = p.reg_info.as_ref()?;
                // Vec port-regs are C arrays — can't use (0) in init list
                if vec_array_info(&p.ty).is_some() { return None; }
                let init_val = if let Some(ref init_expr) = ri.init {
                    match &init_expr.kind {
                        ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                        ExprKind::Literal(LitKind::Hex(v)) => format!("0x{:X}", v),
                        ExprKind::Literal(LitKind::Bin(v)) => v.to_string(),
                        ExprKind::Literal(LitKind::Sized(_, v)) => v.to_string(),
                        ExprKind::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                        _ => "0".to_string(),
                    }
                } else {
                    "0".to_string()
                };
                Some(format!("_{}({})", p.name.name, init_val))
            })
            .collect();
        // pipe_reg inits
        let pipe_reg_inits: Vec<String> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::PipeRegDecl(p) = i {
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
            } else { None })
            .flatten()
            .collect();
        // Collect all clock ports with domain frequency info (multi-domain support)
        let clk_ports: Vec<String> = m.ports.iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .collect();
        // Map clock port name → freq_mhz (if domain has it)
        let clk_freqs: Vec<(String, Option<u64>)> = m.ports.iter()
            .filter_map(|p| if let TypeExpr::Clock(domain) = &p.ty {
                let freq = self.symbols.globals.get(&domain.name)
                    .and_then(|(_sym, _span)| if let crate::resolve::Symbol::Domain(info) = _sym { info.freq_mhz } else { None });
                Some((p.name.name.clone(), freq))
            } else { None })
            .collect();
        // Collect internal clock wires: clocks referenced in `seq on X rising` that are
        // not port-level clocks (i.e. derived from inst outputs, like a clock divider).
        let internal_clks: Vec<String> = {
            let clk_set: std::collections::HashSet<&str> = clk_ports.iter().map(|s| s.as_str()).collect();
            let mut seen = std::collections::HashSet::new();
            m.body.iter()
                .filter_map(|i| if let ModuleBodyItem::RegBlock(rb) = i { Some(rb) } else { None })
                .filter(|rb| !clk_set.contains(rb.clock.name.as_str()))
                .filter(|rb| seen.insert(rb.clock.name.clone()))
                .map(|rb| rb.clock.name.clone())
                .collect()
        };
        // all_clks = port clocks + internal derived clocks
        let all_clks: Vec<String> = clk_ports.iter().chain(internal_clks.iter()).cloned().collect();
        let has_clk = !all_clks.is_empty();
        let clk_prev_inits: Vec<String> = all_clks.iter()
            .map(|c| format!("_clk_prev_{}(0)", c))
            .collect();
        let all_freqs_known_early = clk_freqs.len() >= 2 && clk_freqs.iter().all(|(_, f)| f.is_some());
        let time_init = if all_freqs_known_early { vec!["time_ps(0)".to_string()] } else { vec![] };
        let all_inits: Vec<String> = port_inits.into_iter()
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
        // Constructor always has a body (for auto-trace open)
        h.push_str(&format!("  {class}() : {} {{\n", all_inits.join(", ")));
        for line in &vec_reg_inits { h.push_str(&format!("{line}\n")); }
        // Zero-init credit_channel synthesized fields (DEPTH for the counter).
        crate::sim_credit_channel::emit_constructor_inits(&cc_sites, &mut h);
        for path in &log_files_for_ctor {
            h.push_str(&format!("    {} = fopen(\"{}\", \"w\");\n", log_fd_name(path), path));
        }
        // Note: VCD auto-open is deferred to first eval() call via Verilated::claimTrace()
        h.push_str("  }\n");
        // Verilator-compatible constructor: accepts VerilatedContext* but ignores it
        h.push_str(&format!("  explicit {class}(VerilatedContext*) : {class}() {{}}\n"));
        // Collect trace signals for VCD waveform support
        let trace_signals = collect_trace_signals(&m.ports, &m.body, &wide_names, &widths, &bus_flat);
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
            h.push_str("  void tick();  // advance one time step, auto-toggle clocks at correct ratio\n");
            h.push_str("  uint64_t time_ps;  // current simulation time in picoseconds\n");
        }
        // final(): close trace + log file handles
        h.push_str("  void final() {\n");
        h.push_str("    trace_close();\n");
        for path in &log_files_for_ctor {
            h.push_str(&format!("    if ({fd}) fclose({fd});\n", fd = log_fd_name(path)));
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

        // Private reg fields. Use params-aware Vec sizing — bare
        // `vec_array_info` returns 0 for params-as-length, which would
        // emit `_arr[0]` and corrupt stack on memcpy / index.
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let Some((elem_ty, count)) = vec_array_info_with_params(&r.ty, &m.params) {
                    h.push_str(&format!("  {elem_ty} _{}[{count}];\n", r.name.name));
                } else {
                    let ty = cpp_internal_type(&r.ty);
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
                    let ty = cpp_internal_type(&p.ty);
                    h.push_str(&format!("  {ty} _{};\n", p.name.name));
                }
            } else if vec_port_names.contains(&p.name.name) {
                // Vec non-reg port: also needs internal array for indexed access
                let vi = vec_port_infos.iter().find(|v| v.name == p.name.name).unwrap();
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
            h.push_str("  // --inputs-start-uninit setters (mark TB-driven inputs as initialized)\n");
            for p in &m.ports {
                // Scalar non-bus input.
                if p.bus_info.is_none() {
                    if !uninit_inputs.contains(&p.name.name) { continue; }
                    let pname = &p.name.name;
                    let ty = cpp_port_type(&p.ty);
                    h.push_str(&format!(
                        "  void set_{pname}({ty} v) {{ {pname} = v; _{pname}_vinit = true; }}\n"
                    ));
                    continue;
                }
                // Bus port: emit one setter per flattened In signal.
                let Some(ref bi) = p.bus_info else { continue; };
                let Some(crate::resolve::Symbol::Bus(info)) =
                    self.symbols.globals.get(&bi.bus_name.name).map(|(s, _)| s)
                    else { continue; };
                let mut param_map: std::collections::HashMap<String, &Expr> =
                    info.params.iter()
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
                    if !matches!(actual_dir, Direction::In) { continue; }
                    if matches!(&sty, TypeExpr::Clock(_) | TypeExpr::Reset(_, _)) {
                        continue;
                    }
                    let flat = format!("{}_{}", p.name.name, sname);
                    if !uninit_inputs.contains(&flat) { continue; }
                    let ty = cpp_port_type(&sty);
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
                            let ty = sname.as_ref()
                                .and_then(|n| self.lookup_struct_field_ty(n, &bind.name))
                                .map(|t| cpp_internal_type(&t))
                                .unwrap_or_else(|| "uint32_t".to_string());
                            h.push_str(&format!("  {ty} _let_{};\n", bind.name));
                        }
                        continue;
                    }
                    // ty=None: assignment to existing port/wire — no new field needed
                    if l.ty.is_none() { continue; }
                    let ty = l.ty.as_ref().map(|t| cpp_internal_type(t))
                        .unwrap_or_else(|| "uint32_t".to_string());
                    h.push_str(&format!("  {ty} _let_{};\n", l.name.name));
                }
                ModuleBodyItem::WireDecl(w) => {
                    if let Some((elem_ty, count)) = vec_array_info_with_params(&w.ty, &m.params) {
                        h.push_str(&format!("  {elem_ty} _let_{}[{count}];\n", w.name.name));
                    } else {
                        let ty = cpp_internal_type(&w.ty);
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
                let ty = cpp_uint(w);
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
                    let ty = widths.get(sig_name).copied()
                        .map(cpp_uint)
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
            if !port_names.contains(sig_name) && !reg_names.contains(sig_name)
                && !inst_out.contains(sig_name) && !let_names.contains(sig_name)
            {
                h.push_str(&format!("  uint32_t {sig_name};\n"));
            }
        }

        // Sub-instance private fields
        for inst in &insts {
            h.push_str(&format!("  V{} _inst_{};\n", inst.module_name.name, inst.name.name));
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
                if p.bus_info.is_some() { continue; }  // bus flat signals handled below
                if matches!(&p.ty, TypeExpr::Clock(_)) { continue; }
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
        let ctx = Ctx::new(&reg_names, &port_names, &let_names, &inst_names,
                           &wide_names, &widths, &enum_map, &bus_port_names)
                      .with_reset_levels(&reset_levels)
                      .with_vec_names(&vec_reg_names).with_vec_sizes(&vec_sizes);

        if insts.is_empty() {
            // No sub-instances: simple path
            cpp.push_str("  eval_comb();\n");
            if has_clk {
                cpp.push_str("  eval_posedge();\n");
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
            cpp.push_str(&format!("  for (int _settle = 0; _settle < {settle_depth}; _settle++) {{\n"));
            for &inst_idx in &inst_eval_order {
            let inst = insts[inst_idx];
            let conns = &expanded_conns[inst_idx];
                cpp.push('\n');
                for conn in conns {
                    if conn.direction == ConnectDir::Input {
                        if let crate::ast::ExprKind::Ident(src_name) = &conn.signal.kind {
                            // Vec wire/reg → inst Vec port: expand element-by-element
                            if let Some(&n) = vec_wire_counts.get(src_name.as_str()) {
                                for i in 0..n {
                                    cpp.push_str(&format!("    _inst_{}.{}_{i} = _let_{src_name}[{i}];\n",
                                        inst.name.name, conn.port_name.name));
                                }
                                continue;
                            }
                            // Parent Vec PORT (input) → inst Vec port:
                            // parent's src is stored as flat fields
                            // `src_0..src_{n-1}`.
                            if vec_port_names.contains(src_name.as_str()) {
                                let n = vec_port_infos.iter()
                                    .find(|v| v.name == *src_name)
                                    .map(|v| v.count).unwrap_or(0);
                                for i in 0..n {
                                    cpp.push_str(&format!("    _inst_{}.{}_{i} = {src_name}_{i};\n",
                                        inst.name.name, conn.port_name.name));
                                }
                                continue;
                            }
                            if wide_names.contains(src_name.as_str()) {
                                let resolved = ctx.resolve_name(src_name, false);
                                cpp.push_str(&format!("    _inst_{}.{} = {};\n",
                                    inst.name.name, conn.port_name.name, resolved));
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx);
                        cpp.push_str(&format!("    _inst_{}.{} = {};\n",
                            inst.name.name, conn.port_name.name, sig));
                    }
                }
                cpp.push_str(&format!("    _inst_{}.eval_comb();\n", inst.name.name));
                for conn in conns {
                    if conn.direction == ConnectDir::Output {
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
                                        cpp.push_str(&format!("    _{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name));
                                    }
                                } else {
                                    let prefix = if reg_names.contains(sig_name.as_str()) { "_" }
                                        else if inst_out.contains(sig_name.as_str()) { "" }
                                        else { "_let_" };
                                    for i in 0..n {
                                        cpp.push_str(&format!("    {prefix}{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name));
                                    }
                                }
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx);
                        // Wide type (>64 bits): inst port is VlWide, parent reg is _arch_u128
                        let _out_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                            widths.get(n.as_str()).copied().unwrap_or(0)
                        } else { 0 };
                        if _out_w > 64 {
                            cpp.push_str(&format!("    {} = _arch_vl_to_u128(_inst_{}.{}.data());\n",
                                sig, inst.name.name, conn.port_name.name));
                        } else {
                            cpp.push_str(&format!("    {} = _inst_{}.{};\n",
                                sig, inst.name.name, conn.port_name.name));
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
            cpp.push_str(&format!("  for (int _settle = 0; _settle < {settle_depth}; _settle++) {{\n"));
            for &inst_idx in &inst_eval_order {
            let inst = insts[inst_idx];
            let conns = &expanded_conns[inst_idx];
                // Re-set sub-inst inputs (may have changed after posedge)
                for conn in conns {
                    if conn.direction == ConnectDir::Input {
                        if let crate::ast::ExprKind::Ident(src_name) = &conn.signal.kind {
                            // Vec wire/reg → inst Vec port: expand element-by-element
                            if let Some(&n) = vec_wire_counts.get(src_name.as_str()) {
                                for i in 0..n {
                                    cpp.push_str(&format!("    _inst_{}.{}_{i} = _let_{src_name}[{i}];\n",
                                        inst.name.name, conn.port_name.name));
                                }
                                continue;
                            }
                            // Parent Vec PORT (input) → inst Vec port:
                            // parent's src is stored as flat fields
                            // `src_0..src_{n-1}`.
                            if vec_port_names.contains(src_name.as_str()) {
                                let n = vec_port_infos.iter()
                                    .find(|v| v.name == *src_name)
                                    .map(|v| v.count).unwrap_or(0);
                                for i in 0..n {
                                    cpp.push_str(&format!("    _inst_{}.{}_{i} = {src_name}_{i};\n",
                                        inst.name.name, conn.port_name.name));
                                }
                                continue;
                            }
                            if wide_names.contains(src_name.as_str()) {
                                let resolved = ctx.resolve_name(src_name, false);
                                cpp.push_str(&format!("    _inst_{}.{} = {};\n",
                                    inst.name.name, conn.port_name.name, resolved));
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx);
                        cpp.push_str(&format!("    _inst_{}.{} = {};\n",
                            inst.name.name, conn.port_name.name, sig));
                    }
                }
                cpp.push_str(&format!("    _inst_{}.eval_comb();\n", inst.name.name));
                for conn in conns {
                    if conn.direction == ConnectDir::Output {
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
                                        cpp.push_str(&format!("    _{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name));
                                    }
                                } else {
                                    let prefix = if reg_names.contains(sig_name.as_str()) { "_" }
                                        else if inst_out.contains(sig_name.as_str()) { "" }
                                        else { "_let_" };
                                    for i in 0..n {
                                        cpp.push_str(&format!("    {prefix}{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name));
                                    }
                                }
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx);
                        // Wide type (>64 bits): inst port is VlWide, parent reg is _arch_u128
                        let _out_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                            widths.get(n.as_str()).copied().unwrap_or(0)
                        } else { 0 };
                        if _out_w > 64 {
                            cpp.push_str(&format!("    {} = _arch_vl_to_u128(_inst_{}.{}.data());\n",
                                sig, inst.name.name, conn.port_name.name));
                        } else {
                            cpp.push_str(&format!("    {} = _inst_{}.{};\n",
                                sig, inst.name.name, conn.port_name.name));
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

        let reg_blocks: Vec<&RegBlock> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegBlock(rb) = i { Some(rb) } else { None })
            .collect();
        let reg_decls: Vec<&RegDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i { Some(r) } else { None })
            .collect();

        // Collect pipe_reg declarations for _n_ temporary handling
        let pipe_regs: Vec<&PipeRegDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::PipeRegDecl(p) = i { Some(p) } else { None })
            .collect();

        if !reg_blocks.is_empty() || !pipe_regs.is_empty() {
            // Declare _n_ temporaries for all regs. Use the param-aware
            // helper so Vec<_, PARAM_NAME> resolves to the literal default
            // (otherwise we emit `_n_arr[0]` and corrupt stack on memcpy).
            for rd in &reg_decls {
                let n = &rd.name.name;
                if let Some((elem_ty, count)) = vec_array_info_with_params(&rd.ty, &m.params) {
                    cpp.push_str(&format!("  {elem_ty} _n_{n}[{count}]; memcpy(_n_{n}, _{n}, sizeof(_{n}));\n"));
                } else {
                    let ty = cpp_internal_type(&rd.ty);
                    cpp.push_str(&format!("  {ty} _n_{n} = _{n};\n"));
                }
            }
            // Declare _n_ temporaries for port reg shadows
            for p in &m.ports {
                if p.reg_info.is_some() {
                    let n = &p.name.name;
                    if let Some(vi) = vec_port_infos.iter().find(|v| v.name == *n) {
                        // Vec port-reg: _n_ is an array, initialized by memcpy
                        cpp.push_str(&format!("  {} _n_{n}[{}]; memcpy(_n_{n}, _{n}, sizeof(_{n}));\n", vi.elem_ty, vi.count));
                    } else {
                        let ty = cpp_internal_type(&p.ty);
                        cpp.push_str(&format!("  {ty} _n_{n} = _{n};\n"));
                    }
                }
            }
            // Declare _n_ temporaries for pipe_reg stages
            for p in &pipe_regs {
                let w = widths.get(&p.source.name).copied().unwrap_or(32);
                let ty = cpp_uint(w);
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

            let ctx = Ctx::new(&reg_names, &port_names, &let_names, &inst_names,
                               &wide_names, &widths, &enum_map, &bus_port_names)
                          .with_vec_names(&vec_reg_names).with_vec_sizes(&vec_sizes).posedge()
                          .with_coverage(cov_handle);

            for rb in &reg_blocks {
                let mut assigned = std::collections::BTreeSet::new();
                collect_stmt_assigns(&rb.stmts, &mut assigned);

                let mut reset_sig: Option<(String, bool, bool)> = None;
                let mut reset_regs: Vec<(&str, String)> = Vec::new();

                for name in &assigned {
                    // Look up reset from RegDecl or port reg
                    let reset_ref: Option<&RegReset> = reg_decls.iter()
                        .find(|r| r.name.name == *name)
                        .map(|r| &r.reset)
                        .or_else(|| m.ports.iter()
                            .find(|p| p.name.name == *name && p.reg_info.is_some())
                            .and_then(|p| p.reg_info.as_ref().map(|ri| &ri.reset)));
                    if let Some(reg_reset) = reset_ref {
                        if let Some(info) = resolve_reg_reset_info(reg_reset, &m.ports) {
                            if reset_sig.is_none() { reset_sig = Some(info.clone()); }
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
                                    ExprKind::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                                    // Everything else — struct literals, enum
                                    // variants, idents, calls, casts — lowers
                                    // via the normal expression path. Previously
                                    // this default silently emitted "0", which
                                    // could corrupt non-literal reset values
                                    // (see #6 struct-literal reset bug).
                                    _ => {
                                        let tmp_ctx = Ctx::new(&reg_names, &port_names,
                                            &let_names, &inst_names, &wide_names, &widths,
                                            &enum_map, &bus_port_names);
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

                // Guard each seq block on its specific clock's rising edge
                cpp.push_str(&format!("  if (_rising_{}) {{\n", rb.clock.name));
                let base_indent: usize = 2;
                // --coverage phase 2: count seq-block entries (rising
                // edges seen). One counter per top-level seq block;
                // catches dead clock domains where branch coverage
                // shows 0/0 trivially.
                if let Some(reg) = cov_handle {
                    let idx = reg.borrow_mut().alloc("seq", rb.span.start, format!("seq @{}", rb.clock.name));
                    cpp.push_str(&format!("{}_arch_cov[{idx}]++;\n", "  ".repeat(base_indent)));
                }

                if let Some((rst_name, _is_async, is_low)) = &reset_sig {
                    let cond = if *is_low { format!("(!{})", rst_name) } else { rst_name.clone() };
                    cpp.push_str(&format!("{}if ({cond}) {{\n", "  ".repeat(base_indent)));
                    for (reg_name, init) in &reset_regs {
                        if wide_names.contains(*reg_name) {
                            let bits = widths.get(*reg_name).copied().unwrap_or(0);
                            if bits > 128 {
                                let words = wide_words(bits);
                                cpp.push_str(&format!("{}_n_{reg_name} = VlWide<{words}>({init});\n", "  ".repeat(base_indent + 1)));
                            } else {
                                cpp.push_str(&format!("{}_n_{reg_name} = (_arch_u128){init};\n", "  ".repeat(base_indent + 1)));
                            }
                        } else {
                            cpp.push_str(&format!("{}_n_{reg_name} = {init};\n", "  ".repeat(base_indent + 1)));
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

            // pipe_reg chain assignments — write to _n_ temporaries (before commit)
            {
                let rst_info = m.ports.iter()
                    .find(|p| matches!(&p.ty, TypeExpr::Reset(..)))
                    .map(|p| {
                        let is_low = matches!(&p.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low);
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
                    let ctx_pe = Ctx::new(&reg_names, &port_names, &let_names, &inst_names,
                                           &wide_names, &widths, &enum_map, &bus_port_names)
                                       .with_vec_names(&vec_reg_names).with_vec_sizes(&vec_sizes);
                    let src = ctx_pe.resolve_name(&p.source.name, false);
                    if let Some((ref rst_name, is_low)) = rst_info {
                        let cond = if is_low { format!("(!{})", rst_name) } else { rst_name.clone() };
                        cpp.push_str(&format!("  if ({cond}) {{\n"));
                        for name in &chain {
                            cpp.push_str(&format!("    _n_{name} = 0;\n"));
                        }
                        cpp.push_str("  } else {\n");
                        for name in &chain {
                            let prev = if *name == chain[0] { src.clone() } else {
                                let idx = chain.iter().position(|n| n == name).unwrap();
                                format!("_{}", chain[idx - 1])
                            };
                            cpp.push_str(&format!("    _n_{name} = {prev};\n"));
                        }
                        cpp.push_str("  }\n");
                    } else {
                        for name in &chain {
                            let prev = if *name == chain[0] { src.clone() } else {
                                let idx = chain.iter().position(|n| n == name).unwrap();
                                format!("_{}", chain[idx - 1])
                            };
                            cpp.push_str(&format!("  _n_{name} = {prev};\n"));
                        }
                    }
                }
            }

            // --debug-fsm: save old state values before commit
            if self.debug_fsm {
                for rd in &reg_decls {
                    let n = &rd.name.name;
                    if is_thread_fsm_state_reg(n) {
                        let ty = cpp_internal_type(&rd.ty);
                        cpp.push_str(&format!("  {ty} _dbg_old_{n} = _{n};\n"));
                    }
                }
            }

            // Commit all _n_ temporaries (regs + pipe_regs)
            cpp.push('\n');
            for rd in &reg_decls {
                let n = &rd.name.name;
                if vec_array_info(&rd.ty).is_some() {
                    cpp.push_str(&format!("  memcpy(_{n}, _n_{n}, sizeof(_{n}));\n"));
                } else {
                    // --coverage phase 4: toggle counter — popcount of
                    // (prev XOR new) sums all bits that flipped this
                    // posedge. Skip Vec / wide regs in v1 (Vec needs
                    // per-element handling; wide needs split popcount).
                    // Skip enums — toggle on a state reg is mostly
                    // noise, FSM coverage is more useful there.
                    if let Some(reg) = cov_handle {
                        let bits = type_bits_te(&rd.ty);
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
                            module_name = name, label = label, n = n,
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
                    guard_sig = guard_sig, reg_name = reg_name, name = name,
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
            let rst_expr = m.ports.iter()
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
        let ctx_comb = Ctx::new(&reg_names, &port_names, &let_names, &inst_names,
                                &wide_names, &widths, &enum_map, &bus_port_names)
                           .with_vec_names(&vec_reg_names).with_vec_sizes(&vec_sizes)
                           .with_coverage(cov_handle);

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
                                ExprKind::Ident(nm) => ctx_comb.vec_sizes
                                    .and_then(|s| s.get(nm)).copied(),
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
                                        reg_names: ctx_comb.reg_names, port_names: ctx_comb.port_names,
                                        let_names: ctx_comb.let_names, inst_names: ctx_comb.inst_names,
                                        wide_names: ctx_comb.wide_names, widths: ctx_comb.widths,
                                        posedge_lhs: ctx_comb.posedge_lhs, fsm_mode: ctx_comb.fsm_mode,
                                        enum_map: ctx_comb.enum_map, bus_ports: ctx_comb.bus_ports,
                                        reset_levels: ctx_comb.reset_levels, vec_names: ctx_comb.vec_names,
                                        vec_sizes: ctx_comb.vec_sizes, fsm_vec_port_regs: ctx_comb.fsm_vec_port_regs,
                                        ident_subst: Some(&sub),
                                        coverage: ctx_comb.coverage,
                                    };
                                    hits.push(cpp_expr(&margs[0], &sub_ctx));
                                }
                                let found_expr: String = hits.iter()
                                    .map(|h| format!("({h})"))
                                    .collect::<Vec<_>>().join(" || ");
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
                        if let crate::ast::ExprKind::Ident(src_name) = &conn.signal.kind {
                            // Vec wire/reg → inst Vec port: expand element-by-element
                            if let Some(&n) = vec_wire_counts.get(src_name.as_str()) {
                                for i in 0..n {
                                    cpp.push_str(&format!("  _inst_{}.{}_{i} = _let_{src_name}[{i}];\n",
                                        inst.name.name, conn.port_name.name));
                                }
                                continue;
                            }
                            // Parent Vec PORT (input) → inst Vec port: flat field syntax
                            if vec_port_names.contains(src_name.as_str()) {
                                let n = vec_port_infos.iter()
                                    .find(|v| v.name == *src_name)
                                    .map(|v| v.count).unwrap_or(0);
                                for i in 0..n {
                                    cpp.push_str(&format!("  _inst_{}.{}_{i} = {src_name}_{i};\n",
                                        inst.name.name, conn.port_name.name));
                                }
                                continue;
                            }
                            if wide_names.contains(src_name.as_str()) {
                                let resolved = ctx_comb.resolve_name(src_name, false);
                                cpp.push_str(&format!("  _inst_{}.{} = {};\n",
                                    inst.name.name, conn.port_name.name, resolved));
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx_comb);
                        // Wide type (>64 bits): parent _arch_u128 → inst VlWide
                        let _in_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                            widths.get(n.as_str()).copied().unwrap_or(0)
                        } else { 0 };
                        if _in_w > 64 {
                            cpp.push_str(&format!("  _arch_u128_to_vl({}, _inst_{}.{}.data());\n",
                                sig, inst.name.name, conn.port_name.name));
                        } else {
                            cpp.push_str(&format!("  _inst_{}.{} = {};\n",
                                inst.name.name, conn.port_name.name, sig));
                        }
                    }
                }
                cpp.push_str(&format!("  _inst_{}.eval_comb();\n", inst.name.name));
                for conn in conns {
                    if conn.direction == ConnectDir::Output {
                        // inst Vec port → Vec wire/reg: expand element-by-element
                        if let ExprKind::Ident(sig_name) = &conn.signal.kind {
                            if let Some(&n) = vec_wire_counts.get(sig_name.as_str()) {
                                if vec_port_names.contains(sig_name.as_str()) {
                                    // See note in the input-wiring case: write
                                    // to internal _{name}[i] storage, not flat
                                    // field, so the eval_comb-tail sync isn't
                                    // overwritten.
                                    for i in 0..n {
                                        cpp.push_str(&format!("  _{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name));
                                    }
                                } else {
                                    let prefix = if reg_names.contains(sig_name.as_str()) { "_" }
                                        else if inst_out.contains(sig_name.as_str()) { "" }
                                        else { "_let_" };
                                    for i in 0..n {
                                        cpp.push_str(&format!("  {prefix}{sig_name}[{i}] = _inst_{}.{}_{i};\n",
                                            inst.name.name, conn.port_name.name));
                                    }
                                }
                                continue;
                            }
                        }
                        let sig = cpp_expr(&conn.signal, &ctx_comb);
                        let _out_w = if let ExprKind::Ident(n) = &conn.signal.kind {
                            widths.get(n.as_str()).copied().unwrap_or(0)
                        } else { 0 };
                        if _out_w > 64 {
                            cpp.push_str(&format!("  {} = _arch_vl_to_u128(_inst_{}.{}.data());\n",
                                sig, inst.name.name, conn.port_name.name));
                        } else {
                            cpp.push_str(&format!("  {} = _inst_{}.{};\n",
                                sig, inst.name.name, conn.port_name.name));
                        }
                    }
                }
            }
        }

        // --check-uninit: warn if any uninit reg/pipe_reg output is read in comb
        if !uninit_regs.is_empty() {
            // Collect all signal names read in comb blocks AND in let bindings
            // (let values are lowered into eval_comb too).
            let mut comb_reads: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
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
                        let gate = payload_guards.get(name)
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
                    let idx = reg.borrow_mut().alloc("comb", cb.span.start, "comb".to_string());
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
            let freqs: Vec<(String, u64)> = clk_freqs.iter()
                .map(|(name, f)| (name.clone(), f.unwrap()))
                .collect();

            // Compute half-periods in picoseconds: half_period = 1e6 / (2 * freq_mhz)
            // To avoid floating point, use: half_period_ps = 500_000 / freq_mhz
            let half_periods: Vec<(String, u64)> = freqs.iter()
                .map(|(name, f)| (name.clone(), 500_000 / f))
                .collect();

            // Find GCD of all half-periods for the time step
            fn gcd(a: u64, b: u64) -> u64 {
                if b == 0 { a } else { gcd(b, a % b) }
            }
            let step_ps = half_periods.iter().map(|(_, hp)| *hp).reduce(|a, b| gcd(a, b)).unwrap();

            cpp.push_str(&format!("\nvoid {class}::tick() {{\n"));
            cpp.push_str(&format!("  // Auto-generated clock driver (step = {} ps)\n", step_ps));
            for (name, hp) in &half_periods {
                cpp.push_str(&format!("  // {name}: half-period = {hp} ps ({} MHz)\n",
                    500_000 / hp));
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
        let cyc_arg = if multi_clk { "_dbg_hdr" } else { "(unsigned long long)_dbg_cycle" };
        if emit_debug {
            cpp.push_str(&format!("void {class}::_debug_log_ports() {{\n"));
            if multi_clk {
                // For multi-clock modules, build a header string like "[42@wr_clk]"
                cpp.push_str("  char _dbg_hdr[80];\n");
                cpp.push_str("  snprintf(_dbg_hdr, sizeof(_dbg_hdr), \"[%llu@%s]\", (unsigned long long)_dbg_cycle, _dbg_last_clk);\n");
            }
            for p in &m.ports {
                if p.bus_info.is_some() { continue; }
                let pname = &p.name.name;
                let dir_str = match p.direction { Direction::In => "in", Direction::Out => "out" };
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
                        cpp.push_str(&format!(
                            "  if ({pname}_{i} != _dbg_prev_{pname}_{i}) {{\n"
                        ));
                        cpp.push_str(&format!(
                            "    printf(\"{cyc_fmt}[{name}.{pname}[{i}]]({dir}) 0x%llx -> 0x%llx\\n\",\n",
                            dir = dir_str
                        ));
                        cpp.push_str(&format!(
                            "           {cyc_arg},\n"
                        ));
                        cpp.push_str(&format!(
                            "           (unsigned long long)_dbg_prev_{pname}_{i},\n"
                        ));
                        cpp.push_str(&format!(
                            "           (unsigned long long){pname}_{i});\n"
                        ));
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
                        cpp.push_str(&format!(
                            "  if ({pname} != _dbg_prev_{pname}) {{\n"
                        ));
                        cpp.push_str(&format!(
                            "    printf(\"{cyc_fmt}[{name}.{pname}]({dir}) 0x%llx -> 0x%llx\\n\",\n",
                            dir = dir_str
                        ));
                        cpp.push_str(&format!(
                            "           {cyc_arg},\n"
                        ));
                        cpp.push_str(&format!(
                            "           (unsigned long long)_dbg_prev_{pname},\n"
                        ));
                        cpp.push_str(&format!(
                            "           (unsigned long long){pname});\n"
                        ));
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
                    cpp.push_str(&format!(
                        "  if ({flat_name} != _dbg_prev_{flat_name}) {{\n"
                    ));
                    cpp.push_str(&format!(
                        "    printf(\"{cyc_fmt}[{name}.{flat_name}]({dir_str}) 0x%llx -> 0x%llx\\n\",\n"
                    ));
                    cpp.push_str(&format!(
                        "           {cyc_arg},\n"
                    ));
                    cpp.push_str(&format!(
                        "           (unsigned long long)_dbg_prev_{flat_name},\n"
                    ));
                    cpp.push_str(&format!(
                        "           (unsigned long long){flat_name});\n"
                    ));
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
                    if i > 0 { cpp.push_str(" else "); }
                    cpp.push_str(&format!("if (_rising_{c}) {{ _dbg_cycle++; _dbg_last_clk = \"{c}\"; }}"));
                }
                cpp.push_str("\n");
            }
            cpp.push_str("}\n\n");
        }

        // --coverage: now that all seq emission is done, the registry has
        // its final point count. Patch the header / impl placeholders.
        let n_cov = cov_reg.borrow().points.len();
        let header_decl = if self.coverage && n_cov > 0 {
            format!("public:\n  static uint64_t _arch_cov[{n_cov}];\n  static bool _arch_cov_dumped;\n")
        } else { String::new() };
        let impl_defn = if self.coverage && n_cov > 0 {
            format!("uint64_t {class}::_arch_cov[{n_cov}] = {{}};\nbool {class}::_arch_cov_dumped = false;\n\n")
        } else { String::new() };
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
            for (i, p) in cov_reg.borrow().points.iter().enumerate() {
                let location = if let Some(sm) = &self.source_map {
                    sm.locate(p.span_start)
                        .map(|(f, l)| format!("{}:{}", f, l))
                        .unwrap_or_else(|| format!("branch[{i}]"))
                } else {
                    format!("branch[{i}]")
                };
                cpp.push_str(&format!(
                    "  fprintf(stderr, \"  {location} ({}): %llu hits%s\\n\", (unsigned long long){class}::_arch_cov[{i}], {class}::_arch_cov[{i}] ? \"\" : \" *NOT HIT*\");\n",
                    p.kind
                ));
            }
            cpp.push_str("}\n");
            cpp.push_str("struct _ArchCovInit { _ArchCovInit() { atexit(_arch_cov_dump); } };\n");
            cpp.push_str("static _ArchCovInit _arch_cov_init;\n");
            cpp.push_str("} // namespace\n\n");
        }

        SimModel { class_name: class.clone(), header: h, impl_: cpp }
    }
}

// ── Counter codegen ───────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    fn gen_counter(&self, c: &CounterDecl) -> SimModel {
        let name = &c.name.name;
        let class = format!("V{name}");

        let max_param = c.params.iter()
            .find(|p| p.name.name == "MAX")
            .and_then(|p| p.default.as_ref())
            .map(|e| match &e.kind {
                ExprKind::Literal(LitKind::Dec(v)) => *v,
                _ => 255,
            });

        let value_port = c.ports.iter().find(|p| p.name.name == "value");
        let count_bits = value_port
            .and_then(|vp| if let TypeExpr::UInt(w) = &vp.ty { Some(eval_width(w)) } else { None })
            .unwrap_or(8);
        let count_ty = cpp_uint(count_bits);

        let has_inc    = c.ports.iter().any(|p| p.name.name == "inc");
        let has_dec    = c.ports.iter().any(|p| p.name.name == "dec");
        let has_clear  = c.ports.iter().any(|p| p.name.name == "clear");
        let has_at_max = c.ports.iter().any(|p| p.name.name == "at_max");
        let has_at_min = c.ports.iter().any(|p| p.name.name == "at_min");

        let (rst_name, _is_async, is_low) = extract_reset_info(&c.ports);
        let rst_cond = if is_low { format!("(!{})", rst_name) } else { rst_name.clone() };

        let init_val: u64 = c.init.as_ref()
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
            .unwrap_or(0);

        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n\n");
        h.push_str(&format!("class {class} {{\npublic:\n"));
        for p in &c.ports { h.push_str(&format!("  {} {};\n", cpp_port_type(&p.ty), p.name.name)); }
        h.push('\n');

        let port_inits: Vec<String> = c.ports.iter().map(|p| format!("{}(0)", p.name.name)).collect();
        let state_inits = vec!["_clk_prev(0)".to_string(), format!("_count_r({})", init_val)];
        let all_inits: Vec<String> = port_inits.into_iter().chain(state_inits).collect();
        h.push_str(&format!("  {class}() : {} {{}}\n", all_inits.join(", ")));
        h.push_str(&format!("  explicit {class}(VerilatedContext*) : {class}() {{}}\n"));
        h.push_str("  void eval();\n  void final() { trace_close(); }\n");
        h.push_str("  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  {count_ty} _count_r;\n"));

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str()).unwrap_or("clk");

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
        cpp.push_str(&format!("  if ({rst_cond}) {{\n    _n = {init_val};\n  }} else {{\n"));

        use CounterDirection::*; use CounterMode::*;
        match (c.direction, c.mode) {
            (Up, Wrap) => {
                let inc_cond = if has_inc { "    if (inc) {" } else { "    {" };
                cpp.push_str(&format!("{inc_cond}\n"));
                if let Some(max) = max_param {
                    cpp.push_str(&format!("      if (_count_r == ({count_ty}){max}) _n = {init_val};\n"));
                    cpp.push_str("      else _n = _count_r + 1;\n");
                } else {
                    cpp.push_str(&format!("      _n = ({count_ty})(_count_r + 1);\n"));
                }
                cpp.push_str("    }\n");
            }
            (Down, Wrap) => {
                let dec_cond = if has_dec { "    if (dec) {" } else { "    {" };
                cpp.push_str(&format!("{dec_cond}\n"));
                if let Some(max) = max_param {
                    cpp.push_str(&format!("      if (_count_r == {init_val}) _n = ({count_ty}){max};\n"));
                    cpp.push_str("      else _n = _count_r - 1;\n");
                } else {
                    cpp.push_str(&format!("      _n = ({count_ty})(_count_r - 1);\n"));
                }
                cpp.push_str("    }\n");
            }
            (Up, Saturate) => {
                let inc_cond = if has_inc { "    if (inc) {" } else { "    {" };
                cpp.push_str(&format!("{inc_cond}\n"));
                if let Some(max) = max_param {
                    cpp.push_str(&format!("      if (_count_r < ({count_ty}){max}) _n = _count_r + 1;\n"));
                } else {
                    let max_val = (1u64 << count_bits) - 1;
                    cpp.push_str(&format!("      if (_count_r < ({count_ty})0x{max_val:X}ULL) _n = _count_r + 1;\n"));
                }
                cpp.push_str("    }\n");
            }
            (Down, Saturate) => {
                let dec_cond = if has_dec { "    if (dec) {" } else { "    {" };
                cpp.push_str(&format!("{dec_cond}\n"));
                cpp.push_str("      if (_count_r > 0) _n = _count_r - 1;\n    }\n");
            }
            (Up, Gray) => {
                cpp.push_str("    if (inc) {\n      uint32_t _bin = _count_r + 1;\n");
                cpp.push_str(&format!("      _n = ({count_ty})(_bin ^ (_bin >> 1));\n    }}\n"));
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
                cpp.push_str(&format!("    {inc_cond} _n = ({count_ty})(_count_r + 1);\n"));
            }
        }
        if has_clear {
            cpp.push_str(&format!("    if (clear) _n = {init_val}; // clear overrides inc\n"));
        }
        cpp.push_str("  }\n  _count_r = _n;\n}\n\n");

        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        if value_port.is_some() { cpp.push_str("  value = _count_r;\n"); }
        if has_at_max {
            if let Some(max) = max_param {
                cpp.push_str(&format!("  at_max = (_count_r == ({count_ty}){max}) ? 1 : 0;\n"));
            } else {
                let all_ones = (1u64 << count_bits) - 1;
                cpp.push_str(&format!("  at_max = (_count_r == 0x{all_ones:X}ULL) ? 1 : 0;\n"));
            }
        }
        if has_at_min {
            cpp.push_str(&format!("  at_min = (_count_r == {init_val}) ? 1 : 0;\n"));
        }
        cpp.push_str("}\n");

        // Add trace support
        let extra_sigs: Vec<(&str, &str, u32)> = vec![("count_r", "_count_r", count_bits)];
        add_trace_to_simple_construct(&mut h, &mut cpp, &class, name, &c.ports, &extra_sigs);
        h.push_str("};\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }
}

// ── FSM codegen ───────────────────────────────────────────────────────────────


// ── Regfile codegen ───────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    fn gen_regfile(&self, r: &RegfileDecl) -> SimModel {
        use crate::ast::{ExprKind, LitKind};

        let name  = &r.name.name;
        let class = format!("V{name}");

        // Resolve a param by name to its default integer value
        let param_int = |pname: &str, default: u64| -> u64 {
            r.params.iter()
                .find(|p| p.name.name == pname)
                .and_then(|p| p.default.as_ref())
                .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
                .unwrap_or(default)
        };
        let resolve_count = |expr: &Expr| -> u64 {
            match &expr.kind {
                ExprKind::Literal(LitKind::Dec(v)) => *v,
                ExprKind::Ident(n) => param_int(n, 1),
                _ => 1,
            }
        };

        let nregs  = param_int("NREGS", 32) as usize;
        let nread  = r.read_ports.as_ref().map(|rp| resolve_count(&rp.count_expr)).unwrap_or(1) as usize;
        let nwrite = r.write_ports.as_ref().map(|wp| resolve_count(&wp.count_expr)).unwrap_or(1) as usize;

        // C++ type for one register element (from the write data signal type)
        let elem_cpp = r.write_ports.as_ref()
            .and_then(|wp| wp.signals.iter().find(|s| s.name.name == "data"))
            .map(|s| cpp_internal_type(&s.ty))
            .unwrap_or_else(|| "uint32_t".to_string());

        // Flat port name: "{pfx}_{sig}" when count==1, "{pfx}{i}_{sig}" otherwise
        let flat = |pfx: &str, i: usize, count: usize, sig: &str| -> String {
            if count == 1 { format!("{pfx}_{sig}") } else { format!("{pfx}{i}_{sig}") }
        };

        let clk_port  = r.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let read_pfx  = r.read_ports.as_ref().map(|rp| rp.name.name.clone()).unwrap_or_else(|| "read".to_string());
        let write_pfx = r.write_ports.as_ref().map(|wp| wp.name.name.clone()).unwrap_or_else(|| "write".to_string());

        // ── Header ────────────────────────────────────────────────────────────
        let mut h = String::new();
        h.push_str(&format!("#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\nclass {class} {{\npublic:\n"));

        for p in &r.ports {
            h.push_str(&format!("  {} {};\n", cpp_port_type(&p.ty), p.name.name));
        }
        if let Some(rp) = &r.read_ports {
            for i in 0..nread {
                for s in &rp.signals {
                    h.push_str(&format!("  {} {};\n", cpp_port_type(&s.ty), flat(&read_pfx, i, nread, &s.name.name)));
                }
            }
        }
        if let Some(wp) = &r.write_ports {
            for i in 0..nwrite {
                for s in &wp.signals {
                    h.push_str(&format!("  {} {};\n", cpp_port_type(&s.ty), flat(&write_pfx, i, nwrite, &s.name.name)));
                }
            }
        }
        h.push('\n');

        // Constructor init list (all scalars = 0) + memset for rf array
        let mut inits: Vec<String> = r.ports.iter().map(|p| format!("{}(0)", p.name.name)).collect();
        if let Some(rp) = &r.read_ports {
            for i in 0..nread { for s in &rp.signals { inits.push(format!("{}(0)", flat(&read_pfx, i, nread, &s.name.name))); } }
        }
        if let Some(wp) = &r.write_ports {
            for i in 0..nwrite { for s in &wp.signals { inits.push(format!("{}(0)", flat(&write_pfx, i, nwrite, &s.name.name))); } }
        }
        inits.push("_clk_prev(0)".to_string());

        h.push_str(&format!("  {class}() : {} {{\n    memset(_rf, 0, sizeof(_rf));\n  }}\n", inits.join(", ")));
        h.push_str("  void eval();\n  void eval_comb();\n  void eval_posedge();\n  void final() { trace_close(); }\n\nprivate:\n");
        h.push_str("  uint8_t _clk_prev;\n");
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

        // eval_posedge(): write ports
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (!_rising) return;\n");
        for wi in 0..nwrite {
            let wen   = flat(&write_pfx, wi, nwrite, "en");
            let waddr = flat(&write_pfx, wi, nwrite, "addr");
            let wdata = flat(&write_pfx, wi, nwrite, "data");
            cpp.push_str(&format!("  if ({wen})\n    _rf[{waddr}] = {wdata};\n"));
        }
        cpp.push_str("}\n\n");

        // eval_comb(): async reads, optional write-before-read bypass
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        for ri in 0..nread {
            let raddr = flat(&read_pfx, ri, nread, "addr");
            let rdata = flat(&read_pfx, ri, nread, "data");
            if r.forward_write_before_read && nwrite > 0 {
                let wen   = flat(&write_pfx, 0, nwrite, "en");
                let waddr = flat(&write_pfx, 0, nwrite, "addr");
                let wdata = flat(&write_pfx, 0, nwrite, "data");
                cpp.push_str(&format!("  {rdata} = ({wen} && {waddr} == {raddr}) ? {wdata} : _rf[{raddr}];\n"));
            } else {
                cpp.push_str(&format!("  {rdata} = _rf[{raddr}];\n"));
            }
        }
        cpp.push_str("}\n");

        let extra_sigs: Vec<(&str, &str, u32)> = vec![];
        add_trace_to_simple_construct(&mut h, &mut cpp, &class, name, &r.ports, &extra_sigs);
        h.push_str("};\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }


    fn gen_synchronizer(&self, s: &crate::ast::SynchronizerDecl) -> SimModel {
        use crate::ast::SyncKind;

        let class = s.name.name.clone();

        let stages: usize = s.params.iter()
            .find(|p| p.name.name == "STAGES")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v as usize) } else { None })
            .unwrap_or(2);

        let clk_ports: Vec<&crate::ast::PortDecl> = s.ports.iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .collect();
        let src_clk = &clk_ports[0].name.name;
        let dst_clk = &clk_ports[1].name.name;

        let data_in_port = s.ports.iter().find(|p| p.name.name == "data_in").unwrap();
        let data_ctype = cpp_port_type(&data_in_port.ty);
        let data_bits: u32 = match &data_in_port.ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
            TypeExpr::Bool | TypeExpr::Bit => 1,
            _ => 32,
        };

        let rst_port = s.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Reset(..)));
        let rst_is_low = rst_port.map_or(false, |rp| matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low));
        let rst_guard = rst_port.map(|rp| {
            if rst_is_low { format!("!{}", rp.name.name) } else { rp.name.name.clone() }
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
            h.push_str(&format!("  {} {};\n", cpp_port_type(&p.ty), p.name.name));
        }
        h.push_str("\n  void eval();\n  void eval_posedge();\n  void eval_comb();\n  void final() { trace_close(); }\n");
        if cdc_random {
            h.push_str("  uint8_t cdc_skip_pct = 25; // 0-100: probability of +1 cycle latency per edge\n");
        }
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev_src;\n  uint8_t _clk_prev_dst;\n");
        h.push_str("  bool _rising_src;\n  bool _rising_dst;\n");
        match s.kind {
            SyncKind::Ff => {
                for i in 0..stages { h.push_str(&format!("  {} _stage{};\n", data_ctype, i)); }
            }
            SyncKind::Gray => {
                for i in 0..stages { h.push_str(&format!("  {} _gray_stage{};\n", data_ctype, i)); }
            }
            SyncKind::Handshake => {
                h.push_str(&format!("  {} _data_reg;\n", data_ctype));
                h.push_str("  uint8_t _req_src;\n  uint8_t _ack_src;\n  uint8_t _ack_dst;\n");
                for i in 0..stages {
                    h.push_str(&format!("  uint8_t _req_sync{};\n  uint8_t _ack_sync{};\n", i, i));
                }
            }
            SyncKind::Reset => {
                for i in 0..stages { h.push_str(&format!("  uint8_t _stage{};\n", i)); }
            }
            SyncKind::Pulse => {
                h.push_str("  uint8_t _toggle_src;\n");
                // sync_chain needs STAGES entries + previous value for edge detect
                for i in 0..stages { h.push_str(&format!("  uint8_t _sync{};\n", i)); }
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
        cpp.push_str(&format!("  _clk_prev_src = {src_clk};\n  _clk_prev_dst = {dst_clk};\n"));
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
                    for i in 0..stages { cpp.push_str(&format!("    _stage{i} = 0;\n")); }
                }
                SyncKind::Gray => {
                    for i in 0..stages { cpp.push_str(&format!("    _gray_stage{i} = 0;\n")); }
                }
                SyncKind::Handshake => {
                    cpp.push_str("    _data_reg = 0; _req_src = 0; _ack_src = 0; _ack_dst = 0;\n");
                    for i in 0..stages { cpp.push_str(&format!("    _req_sync{i} = 0; _ack_sync{i} = 0;\n")); }
                }
                SyncKind::Reset => {
                    for i in 0..stages { cpp.push_str(&format!("    _stage{i} = 1;\n")); }
                }
                SyncKind::Pulse => {
                    cpp.push_str("    _toggle_src = 0; _sync_prev = 0;\n");
                    for i in 0..stages { cpp.push_str(&format!("    _sync{i} = 0;\n")); }
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
            cpp.push_str("  _cdc_lfsr = (_cdc_lfsr >> 1) ^ ((_cdc_lfsr & 1) ? 0xB4BCD35Cu : 0u);\n");
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
                for i in (1..stages).rev() { cpp.push_str(&format!("    _stage{i} = _stage{};\n", i - 1)); }
                cpp.push_str("    _stage0 = data_in;\n  }\n");
            }
            SyncKind::Gray => {
                cpp.push_str(dst_guard);
                for i in (1..stages).rev() { cpp.push_str(&format!("    _gray_stage{i} = _gray_stage{};\n", i - 1)); }
                cpp.push_str("    _gray_stage0 = data_in ^ (data_in >> 1);\n  }\n");
            }
            SyncKind::Handshake => {
                cpp.push_str("  if (_rising_src) {\n");
                cpp.push_str("    if (data_in != _data_reg && _req_src == _ack_src) {\n");
                cpp.push_str("      _data_reg = data_in;\n      _req_src ^= 1;\n    }\n");
                for i in (1..stages).rev() { cpp.push_str(&format!("    _ack_sync{i} = _ack_sync{};\n", i - 1)); }
                cpp.push_str("    _ack_sync0 = _ack_dst;\n");
                cpp.push_str(&format!("    _ack_src = _ack_sync{};\n  }}\n", stages - 1));
                cpp.push_str(dst_guard);
                for i in (1..stages).rev() { cpp.push_str(&format!("    _req_sync{i} = _req_sync{};\n", i - 1)); }
                cpp.push_str("    _req_sync0 = _req_src;\n");
                cpp.push_str(&format!("    _ack_dst = _req_sync{};\n  }}\n", stages - 1));
            }
            SyncKind::Reset => {
                // Async assert is always immediate (no randomization)
                cpp.push_str("  if (data_in) {\n");
                for i in 0..stages { cpp.push_str(&format!("    _stage{i} = 1;\n")); }
                if cdc_random {
                    cpp.push_str("  } else if (_rising_dst && !_cdc_skip) {\n");
                } else {
                    cpp.push_str("  } else if (_rising_dst) {\n");
                }
                for i in (1..stages).rev() { cpp.push_str(&format!("    _stage{i} = _stage{};\n", i - 1)); }
                cpp.push_str("    _stage0 = 0;\n  }\n");
            }
            SyncKind::Pulse => {
                // Source toggle is always immediate (no randomization)
                cpp.push_str("  if (_rising_src) {\n");
                cpp.push_str("    if (data_in) _toggle_src ^= 1;\n");
                cpp.push_str("  }\n");
                cpp.push_str(dst_guard);
                cpp.push_str(&format!("    _sync_prev = _sync{};\n", stages - 1));
                for i in (1..stages).rev() { cpp.push_str(&format!("    _sync{i} = _sync{};\n", i - 1)); }
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
        add_trace_to_simple_construct(&mut h, &mut cpp, &class, &class, &s.ports, &extra_sigs);
        h.push_str("};\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }

    fn gen_clkgate(&self, c: &crate::ast::ClkGateDecl) -> SimModel {
        let class = format!("V{}", c.name.name);

        let clk_in = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::In)
            .map(|p| p.name.name.as_str()).unwrap_or("clk_in");
        let clk_out = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::Out)
            .map(|p| p.name.name.as_str()).unwrap_or("clk_out");
        let enable = "enable";
        let test_en = c.ports.iter().find(|p| p.name.name == "test_en").map(|p| p.name.name.as_str());

        let mut h = String::new();
        h.push_str(&format!("#pragma once\n#include <cstdint>\nclass {} {{\npublic:\n", class));

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
                cpp.push_str(&format!("  if (!{clk_in}) _en_latched = ({en_expr}) ? 1 : 0;\n"));
                cpp.push_str(&format!("  {clk_out} = {clk_in} & _en_latched;\n"));
            }
            crate::ast::ClkGateKind::And => {
                cpp.push_str(&format!("  {clk_out} = {clk_in} & (({en_expr}) ? 1 : 0);\n"));
            }
        }
        cpp.push_str("}\n");

        // eval_posedge — no-op for clkgate
        cpp.push_str(&format!("void {}::eval_posedge() {{}}\n", class));

        // eval — calls both
        cpp.push_str(&format!("void {}::eval() {{ eval_comb(); }}\n", class));

        SimModel { class_name: class, header: h, impl_: cpp }
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
                    for e in &p.enums { enums.push(e); }
                    for s in &p.structs { structs.push(s); }
                }
                _ => {}
            }
        }

        for e in &enums {
            // Enums are uint32_t aliases — variants are used as integer indices
            h.push_str(&format!("typedef uint32_t {};\n", e.name.name));
            for (i, v) in e.variants.iter().enumerate() {
                h.push_str(&format!("static const uint32_t {}_{} = {}u;\n", e.name.name, v.name, i));
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
            let mut field_inits = Vec::new();
            for f in &s.fields {
                let ty = cpp_internal_type(&f.ty);
                h.push_str(&format!("  {} {};\n", ty, f.name.name));
                // Struct fields use default init for non-trivial types
                if matches!(f.ty, TypeExpr::Named(_)) {
                    field_inits.push(format!("{}()", f.name.name));
                } else {
                    field_inits.push(format!("{}(0)", f.name.name));
                }
            }
            h.push_str(&format!("  {}() : {} {{}}\n", s.name.name, field_inits.join(", ")));
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
                Item::Package(p) => { for b in &p.buses { buses.push(b); } }
                _ => {}
            }
        }
        for b in &buses {
            let param_map: HashMap<String, &Expr> = HashMap::new();
            let effective = crate::resolve::BusInfo {
                name: b.name.name.clone(),
                params: b.params.clone(),
                signals: b.signals.iter()
                    .map(|p| (p.name.name.clone(), p.direction, p.ty.clone()))
                    .collect(),
                generates: b.generates.clone(),
                handshakes: b.handshakes.clone(),
                credit_channels: b.credit_channels.clone(),
                tlm_methods: b.tlm_methods.clone(),
            }.effective_signals(&param_map);
            h.push_str(&format!("struct {} {{\n", b.name.name));
            let mut field_inits = Vec::new();
            for (sname, _dir, sty) in &effective {
                let ty = cpp_internal_type(sty);
                h.push_str(&format!("  {} {};\n", ty, sname));
                if matches!(sty, TypeExpr::Named(_)) {
                    field_inits.push(format!("{}()", sname));
                } else {
                    field_inits.push(format!("{}(0)", sname));
                }
            }
            if field_inits.is_empty() {
                h.push_str(&format!("  {}() {{}}\n", b.name.name));
            } else {
                h.push_str(&format!("  {}() : {} {{}}\n", b.name.name, field_inits.join(", ")));
            }
            h.push_str("};\n\n");
        }

        SimModel {
            class_name: "VStructs".to_string(),
            header: h,
            impl_: "#include \"VStructs.h\"\n".to_string(),
        }
    }


    fn gen_arbiter(&self, a: &ArbiterDecl) -> SimModel {
        let name = &a.name.name;
        let class = format!("V{name}");

        let num_req: u64 = a.params.iter()
            .find(|p| p.name.name == "NUM_REQ")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
            .unwrap_or(2);

        let (rst_name, _is_async, is_low) = extract_reset_info(&a.ports);
        let rst_cond = if is_low { format!("(!{rst_name})") } else { rst_name.clone() };

        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\n");
        h.push_str(&format!("class {class} {{\npublic:\n"));
        for p in &a.ports {
            let ty = cpp_port_type(&p.ty);
            h.push_str(&format!("  {ty} {};\n", p.name.name));
        }
        for pa in &a.port_arrays {
            h.push_str(&format!("  uint64_t {}_valid;\n", pa.name.name));
            h.push_str(&format!("  uint64_t {}_ready;\n", pa.name.name));
        }
        h.push('\n');

        let mut all_port_inits: Vec<String> = a.ports.iter()
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        for pa in &a.port_arrays {
            all_port_inits.push(format!("{}_valid(0)", pa.name.name));
            all_port_inits.push(format!("{}_ready(0)", pa.name.name));
        }
        all_port_inits.push("_clk_prev(0)".to_string());
        all_port_inits.push("_last_grant(0)".to_string());

        h.push_str(&format!("  {class}() : {} {{}}\n", all_port_inits.join(", ")));
        h.push_str("  void eval();\n  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("  void final() { trace_close(); }\n");
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n  uint8_t _last_grant;\n");
        h.push_str("  void trace_open(const char* filename);\n");
        h.push_str("  void trace_dump(uint64_t time);\n");
        h.push_str("  void trace_close();\n");
        h.push_str("  FILE* _trace_fp = nullptr;\n  uint64_t _trace_time = 0;\n");
        h.push_str("};\n");

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = a.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str()).unwrap_or("clk");

        let req_pa_name = a.port_arrays.first()
            .map(|pa| pa.name.name.as_str()).unwrap_or("request");

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str(&format!("  if ({clk_port} && !_clk_prev) eval_posedge();\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        // eval_posedge()
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  if ({rst_cond}) {{\n    _last_grant = 0;\n  }} else {{\n"));
        cpp.push_str("    if (grant_valid) _last_grant = grant_requester;\n");
        cpp.push_str("  }\n}\n\n");

        // eval_comb() — round-robin
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        cpp.push_str("  grant_valid = 0;\n  grant_requester = 0;\n");
        cpp.push_str(&format!("  for (int _i = 0; _i < (int){num_req}; _i++) {{\n"));
        cpp.push_str(&format!("    int _idx = (_last_grant + 1 + _i) % {num_req};\n"));
        cpp.push_str(&format!("    if (({req_pa_name}_valid >> _idx) & 1) {{\n"));
        cpp.push_str("      grant_valid = 1;\n      grant_requester = _idx;\n      break;\n    }\n  }\n");
        cpp.push_str(&format!("  {req_pa_name}_ready = grant_valid ? (1ULL << grant_requester) : 0;\n"));
        cpp.push_str("}\n\n");

        // Trace methods
        cpp.push_str(&format!("void {class}::trace_open(const char* filename) {{\n"));
        cpp.push_str("  _trace_fp = fopen(filename, \"w\");\n");
        cpp.push_str("  if (!_trace_fp) return;\n");
        cpp.push_str("  fprintf(_trace_fp, \"$timescale 1ns $end\\n\");\n");
        cpp.push_str(&format!("  fprintf(_trace_fp, \"$scope module {} $end\\n\");\n", name));
        let mut sig_idx = 0usize;
        for p in &a.ports {
            if matches!(p.ty, TypeExpr::Clock(_) | TypeExpr::Reset(..)) { continue; }
            let id = vcd_id(sig_idx); sig_idx += 1;
            cpp.push_str(&format!("  fprintf(_trace_fp, \"$var wire 1 {} {} $end\\n\");\n", id, p.name.name));
        }
        cpp.push_str("  fprintf(_trace_fp, \"$upscope $end\\n$enddefinitions $end\\n\");\n");
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::trace_dump(uint64_t time) {{\n"));
        cpp.push_str("  if (!_trace_fp) return;\n");
        cpp.push_str("  fprintf(_trace_fp, \"#%lu\\n\", (unsigned long)time);\n");
        sig_idx = 0;
        for p in &a.ports {
            if matches!(p.ty, TypeExpr::Clock(_) | TypeExpr::Reset(..)) { continue; }
            let id = vcd_id(sig_idx); sig_idx += 1;
            let pname = &p.name.name;
            cpp.push_str(&format!("  fprintf(_trace_fp, \"%c{}\\n\", {pname} ? '1' : '0');\n", id));
        }
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::trace_close() {{\n"));
        cpp.push_str("  if (_trace_fp) {{ fclose(_trace_fp); _trace_fp = nullptr; }}\n");
        cpp.push_str("}\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }

}
