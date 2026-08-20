//! `pybind` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

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
            pipeline_float_names: std::cell::RefCell::new(HashMap::new()),
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
            TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 => 8,
            TypeExpr::FP4E2M1 => 4,
            TypeExpr::FP6E2M3 | TypeExpr::FP6E3M2 => 6,
            TypeExpr::E8M0 | TypeExpr::UE4M3 => 8,
            TypeExpr::ScaledVec(elem, n, scale) => {
                crate::fp_format::scaled_vec_width(elem, eval_width(n), scale).unwrap_or(0)
            }
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
    /// Construct from an integer (zero-extends into MSB words). The parameter
    /// is 128-bit so a >64-bit input (unsigned __int128) fills words 0..3
    /// instead of narrowing through a uint64_t overload and silently dropping
    /// bits 64+ (arch#868); a uint64_t or literal argument promotes cleanly,
    /// so there is a single integer overload and no ambiguity.
    explicit VlWide(unsigned __int128 v) { memset(_data, 0, sizeof(_data));
        for (int i = 0; i < WORDS && i < 4; i++) _data[i] = (uint32_t)(v >> (32 * i)); }
    VlWide& operator=(const VlWide& o) { memcpy(_data, o._data, sizeof(_data)); return *this; }
    VlWide& operator=(unsigned __int128 v) { memset(_data, 0, sizeof(_data));
        for (int i = 0; i < WORDS && i < 4; i++) _data[i] = (uint32_t)(v >> (32 * i)); return *this; }
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

/// Mask covering bits [hi:lo] of a <=64-bit value. Used by runtime-bound
/// bit-slices (arch#847) when the slice width itself is not compile-time
/// derivable.
static inline uint64_t _arch_slice_mask(uint64_t hi, uint64_t lo) {
    uint64_t w = hi - lo + 1;
    return (w >= 64) ? ~0ULL : ((1ULL << w) - 1ULL);
}

/// 128-bit sibling of `_arch_slice_mask`: mask covering bits [hi:lo] of a
/// value up to 128 bits wide. Used by runtime-bound bit-slices (arch#868)
/// whose derivable width is not known but whose base exceeds 64 bits, so a
/// 64-bit mask would silently zero slice bits 64+.
static inline _arch_u128 _arch_slice_mask128(uint64_t hi, uint64_t lo) {
    uint64_t w = hi - lo + 1;
    return (w >= 128) ? ~(_arch_u128)0 : (((_arch_u128)1 << w) - 1);
}

/// 128-bit sibling of `_arch_vw_bits`: extract up to 128 bits [hi:lo] from a
/// VlWide `_data` array into an `_arch_u128`. Used by runtime-bound bit-slices
/// (arch#868) from a >128-bit base whose derivable width exceeds 64 bits —
/// `_arch_vw_bits` caps the width at 64 and returns `uint64_t`, silently
/// truncating such a result.
///
/// Words are shifted into place individually (the first by a right shift of
/// `b0`, the rest by a positive left shift) so no `_arch_u128` shift ever
/// reaches 128 (which would be UB): for width <= 128 the largest left shift
/// is `32*(wtop-w0) - b0 < 128`. Reads are bounded to `[w0 .. wtop]`, the
/// exact source words spanning the (capped) window, so no read runs past the
/// meaningful payload.
static inline _arch_u128 _arch_vw_bits128(const uint32_t* data, uint32_t hi, uint32_t lo) {
    uint32_t width = hi - lo + 1; if (width > 128) width = 128;
    uint32_t w0 = lo >> 5, b0 = lo & 31;
    uint32_t wtop = (lo + width - 1) >> 5;
    _arch_u128 v = (_arch_u128)data[w0] >> b0;
    for (uint32_t wi = w0 + 1; wi <= wtop; wi++) {
        v |= (_arch_u128)data[wi] << (32 * (wi - w0) - b0);
    }
    _arch_u128 mask = (width >= 128) ? ~(_arch_u128)0 : (((_arch_u128)1 << width) - 1);
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
// through an f32 intermediate then round once to bf16. For mul/add/sub the bf16
// result is correctly rounded (exhaustively SMT-proved vs fp.{mul,add,sub} on
// (8,8)). BF16 fma is fused f32-accumulate (one f32 fma via fmaf, then narrow),
// NOT correctly-rounded bf16 — the narrow is a second, non-innocuous rounding;
// it matches the RTL and the NVIDIA/TPU convention but differs from a
// correctly-rounded bf16 fma by 1 ULP on ~0.37% of inputs (see fp_ops.rs and
// proofs/lean_fp_equiv, PR #627). int.to_bf16() is DECLARED as the same
// f32-routed convention (narrow_bf16(f32(i))) — also a double rounding, also
// NOT correctly-rounded for |i| >= 2^24, off by 1 ULP on 8064/2^30 inputs (see
// issue #629, doc/ARCH_HDL_Specification.md §3.8 "Rounding convention"). NaN
// results are canonicalized to the RISC-V default pattern (0x7FC00000 /
// 0x7FC0); float→int is toward-zero, saturating, NaN→type-max (RISC-V
// profile, §6).
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

// ── FP8 (E4M3 / E5M2) runtime ────────────────────────────────────────────────
// E5M2 is IEEE-style (5,3): ±inf at 0x7C, NaN class above it, max finite
// 0x7B (57344). E4M3 is OCP OFP8: NO infinities, the sole NaN encoding is
// S.1111.111 (0x7F), exponent 15 with mantissa < 7 are normal values
// 256..448. All fp8 values are exact in f32, so widen is exact; ops widen
// to f32, run on the host FPU, then round once to fp8 (mirroring the RTL).
// Overflow on narrow is profile-dependent (--fp-compat): riscv -> E5M2 ±inf
// / E4M3 NaN (sign dropped); cuda -> saturate to ±max-finite (PTX satfinite).
static inline float _arch_e5m2f(uint8_t h){
    uint32_t s=h>>7, e=(h>>2)&0x1Fu, f=h&3u;
    float v;
    if(e==31u){ v = f? nanf("") : INFINITY; }
    else if(e==0u){ v = ldexpf((float)f, -16); }
    else { v = ldexpf((float)(4u+f), (int)e-17); }
    return s? -v : v;
}
static inline float _arch_e4m3f(uint8_t h){
    uint32_t s=h>>7, e=(h>>3)&0xFu, f=h&7u;
    float v;
    if((h&0x7Fu)==0x7Fu){ v = nanf(""); }  // OCP: sole NaN, no infinities
    else if(e==0u){ v = ldexpf((float)f, -9); }
    else { v = ldexpf((float)(8u+f), (int)e-10); }
    return s? -v : v;
}
static inline uint32_t _arch_e5m2_to_f32(uint8_t h){ return _arch_f32_canon(_arch_b32f(_arch_e5m2f(h))); }
static inline uint32_t _arch_e4m3_to_f32(uint8_t h){ return _arch_f32_canon(_arch_b32f(_arch_e4m3f(h))); }
// f32 -> fp8 narrow (RNE), mirroring the IR `fp8_round` bit-exactly.
// `ocp`: OCP top binade (finite through mant==all-ones-1; the all-ones slot
// is NaN, so overflow triggers at max-finite + half-ULP after rounding).
// `ovf`: overflow magnitude byte; `ovf_signed`: whether overflow keeps the
// sign (riscv E4M3 overflows to NaN with the sign dropped); `nan8`:
// canonical NaN byte. Input ±inf takes the overflow result in all profiles.
static inline uint8_t _arch_f32_to_fp8(uint32_t x, int eb, int mb, int ocp,
                                       uint8_t ovf, int ovf_signed, uint8_t nan8){
    uint32_t s = x>>31, e = (x>>23)&0xFFu, m = x & 0x7FFFFFu;
    uint8_t sgn = (uint8_t)(s<<7);
    if (e==0xFFu && m!=0u) return nan8;                 // NaN (sign dropped)
    if (e==0xFFu) return ovf_signed ? (uint8_t)(sgn|ovf) : ovf;  // ±inf
    if (e==0u) return sgn;  // f32 zero/subnormal: far below fp8 min subnormal
    int bias = (1<<(eb-1))-1;
    int emax = (1<<eb)-1;                               // all-ones exponent field
    int t = (int)e - 127 + bias;                        // target biased exponent
    uint64_t sig = (uint64_t)(m | 0x800000u);           // 24-bit significand
    int shift = (t >= 1) ? (23-mb) : (23-mb) + (1-t);   // denormalize if t<1
    if (shift > 26) shift = 26;                         // keep=0, rem<half: ->0
    uint64_t keep = sig >> shift;
    uint64_t rem  = sig & (((uint64_t)1<<shift)-1);
    uint64_t half = (uint64_t)1<<(shift-1);
    if (rem > half || (rem == half && (keep & 1u))) keep++;
    if (t >= 1) {
        if (keep == (uint64_t)2<<mb) { t++; keep >>= 1; }   // carry out of rounding
        uint32_t mant = (uint32_t)keep & ((1u<<mb)-1u);
        int over = ocp ? (t > emax || (t == emax && mant == (1u<<mb)-1u))
                       : (t >= emax);
        if (over) return ovf_signed ? (uint8_t)(sgn|ovf) : ovf;
        return (uint8_t)(sgn | ((uint32_t)t<<mb) | mant);
    }
    // Subnormal result: keep is in min-subnormal ULPs; keep==2^mb encodes
    // naturally as the minimum normal (exponent field 1, mantissa 0).
    return (uint8_t)(sgn | (uint32_t)keep);
}
// ── OCP MX FP4 E2M1 (storage-only) ──
// No Inf, no NaN, max finite 6.0. The shared _arch_f32_to_fp8 cannot be
// reused: it packs the sign at bit 7, while E2M1's sign is bit 3. Written
// as a direct nearest-ties-to-even search over the eight magnitudes, which
// mirrors fp_lit::f64_to_e2m1_bits exactly.
//
// Runtime overflow SATURATES for both --fp-compat profiles, unlike fp8
// where the profiles differ: E2M1 has neither a NaN nor an infinity to
// produce, so saturation is the only representable behavior. (Overflowing
// LITERALS are still a compile error - a source constant must not depend
// on runtime saturation.)
static const float _ARCH_E2M1_MAG[8] = {0.0f,0.5f,1.0f,1.5f,2.0f,3.0f,4.0f,6.0f};
static inline float _arch_e2m1f(uint8_t b){
  float m = _ARCH_E2M1_MAG[b & 7u];
  return (b & 8u) ? -m : m;
}
static inline uint32_t _arch_e2m1_to_f32(uint8_t b){ return _arch_b32f(_arch_e2m1f(b)); }
static inline uint8_t _arch_f32_to_e2m1(uint32_t x){
  float f = _arch_f32b(x);
  uint8_t sgn = (uint8_t)((x>>31) ? 8u : 0u);
  if (std::isnan(f)) return (uint8_t)(sgn|7u);
  float a = fabsf(f);
  if (a >= 7.0f) return (uint8_t)(sgn|7u);
  int best = 0; float best_err = 1e30f;
  for (int i = 0; i < 8; i++) {
    float e = fabsf(a - _ARCH_E2M1_MAG[i]);
    if (e < best_err || (e == best_err && (i & 1) == 0)) { best = i; best_err = e; }
  }
  return (uint8_t)(sgn | (uint8_t)best);
}
// ── OCP MX FP6 E2M3 / E3M2 (storage-only) ──
// Same shape as E2M1: all-finite, so overflow saturates under BOTH
// --fp-compat profiles. Generic over the field split; mirrors
// fp_lit::f64_to_all_finite_bits exactly.
static inline float _arch_fp6_mag(int idx, int eb, int mb){
  int bias = (1 << (eb - 1)) - 1;
  int mant_n = 1 << mb;
  int e = idx / mant_n, m = idx % mant_n;
  if (e == 0) return (float)(ldexp(1.0, 1 - bias) * ((double)m / mant_n));
  return (float)(ldexp(1.0, e - bias) * (1.0 + (double)m / mant_n));
}
static inline float _arch_fp6f(uint8_t b, int eb, int mb){
  int sign_bit = 1 << (eb + mb);
  float m = _arch_fp6_mag(b & (sign_bit - 1), eb, mb);
  return (b & sign_bit) ? -m : m;
}
static inline uint8_t _arch_f32_to_fp6(uint32_t x, int eb, int mb){
  int sign_bit = 1 << (eb + mb);
  int n = sign_bit;
  float f = _arch_f32b(x);
  uint8_t sgn = (uint8_t)((x>>31) ? sign_bit : 0);
  float maxf = _arch_fp6_mag(n - 1, eb, mb);
  float top_ulp = maxf - _arch_fp6_mag(n - 2, eb, mb);
  if (std::isnan(f)) return (uint8_t)(sgn | (n - 1));
  float a = fabsf(f);
  if (a >= maxf + top_ulp / 2.0f) return (uint8_t)(sgn | (n - 1));
  int best = 0; float best_err = 1e30f;
  for (int i = 0; i < n; i++) {
    float e = fabsf(a - _arch_fp6_mag(i, eb, mb));
    if (e < best_err || (e == best_err && (i & 1) == 0)) { best = i; best_err = e; }
  }
  return (uint8_t)(sgn | (uint8_t)best);
}
static inline uint32_t _arch_e2m3_to_f32(uint8_t b){ return _arch_b32f(_arch_fp6f(b,2,3)); }
static inline uint32_t _arch_e3m2_to_f32(uint8_t b){ return _arch_b32f(_arch_fp6f(b,3,2)); }
static inline uint8_t _arch_f32_to_e2m3(uint32_t x){ return _arch_f32_to_fp6(x,2,3); }
static inline uint8_t _arch_f32_to_e3m2(uint32_t x){ return _arch_f32_to_fp6(x,3,2); }
// ── OCP MX E8M0 — the block SCALE type ──
// 2^(e-127); NO sign, NO mantissa, NO zero (0x00 IS the minimum scale
// 2^-127), 0xFF = NaN. Shares FP32's bias, so codes 1..254 map to the f32
// exponent field directly.
static inline uint32_t _arch_e8m0_to_f32(uint8_t e){
  if (e == 0xFFu) return 0x7FC00000u;          // NaN
  if (e == 0x00u) return 0x00400000u;          // 2^-127 (f32 subnormal)
  return ((uint32_t)e) << 23;                  // 2^(e-127)
}
static inline uint8_t _arch_e8m0_isnan(uint8_t e){ return (e == 0xFFu) ? 1 : 0; }
static inline uint8_t _arch_f32_to_e8m0(uint32_t x){
  uint32_t ef = (x >> 23) & 0xFFu;
  if (ef == 0xFFu) return 0xFFu;               // inf/NaN -> NaN scale
  if (ef == 0u)    return 0x00u;               // zero/subnormal -> min scale
  return (uint8_t)ef;
}
static inline uint8_t _arch_f32_to_e5m2(uint32_t x){ return _arch_f32_to_fp8(x,5,2,0,0x7Cu,1,0x7Eu); }
static inline uint8_t _arch_f32_to_e4m3(uint32_t x){ return _arch_f32_to_fp8(x,4,3,1,0x7Fu,0,0x7Fu); }
// ── NVFP4 UE4M3 — the NVIDIA block SCALE type ──
// 7-bit unsigned, MSB padded with zero, sole NaN 0x7F. Numerically E4M3
// restricted to sign 0, so both directions route through the E4M3 helpers
// rather than duplicating a rounder. The mask is not cosmetic: a stray high
// bit would otherwise be read as an E4M3 SIGN and negate the scale. The
// narrow takes the magnitude, since a scale is non-negative (matching
// _arch_f32_to_e8m0, which ignores the sign bit).
static inline uint32_t _arch_ue4m3_to_f32(uint8_t u){ return _arch_e4m3_to_f32(u & 0x7Fu); }
static inline uint8_t _arch_f32_to_ue4m3(uint32_t x){ return _arch_f32_to_e4m3(x & 0x7FFFFFFFu); }
static inline uint8_t _arch_ue4m3_isnan(uint8_t u){ return ((u & 0x7Fu) == 0x7Fu) ? 1 : 0; }
static inline uint8_t _arch_e5m2_add(uint8_t a,uint8_t b){ return _arch_f32_to_e5m2(_arch_b32f(_arch_e5m2f(a)+_arch_e5m2f(b))); }
static inline uint8_t _arch_e5m2_sub(uint8_t a,uint8_t b){ return _arch_f32_to_e5m2(_arch_b32f(_arch_e5m2f(a)-_arch_e5m2f(b))); }
static inline uint8_t _arch_e5m2_mul(uint8_t a,uint8_t b){ return _arch_f32_to_e5m2(_arch_b32f(_arch_e5m2f(a)*_arch_e5m2f(b))); }
static inline uint8_t _arch_fma_e5m2(uint8_t a,uint8_t b,uint8_t c){ return _arch_f32_to_e5m2(_arch_b32f(fmaf(_arch_e5m2f(a),_arch_e5m2f(b),_arch_e5m2f(c)))); }
static inline uint8_t _arch_e5m2_eq(uint8_t a,uint8_t b){ return _arch_e5m2f(a)==_arch_e5m2f(b); }
static inline uint8_t _arch_e5m2_ne(uint8_t a,uint8_t b){ return _arch_e5m2f(a)!=_arch_e5m2f(b); }
static inline uint8_t _arch_e5m2_lt(uint8_t a,uint8_t b){ return _arch_e5m2f(a)< _arch_e5m2f(b); }
static inline uint8_t _arch_e5m2_gt(uint8_t a,uint8_t b){ return _arch_e5m2f(a)> _arch_e5m2f(b); }
static inline uint8_t _arch_e5m2_le(uint8_t a,uint8_t b){ return _arch_e5m2f(a)<=_arch_e5m2f(b); }
static inline uint8_t _arch_e5m2_ge(uint8_t a,uint8_t b){ return _arch_e5m2f(a)>=_arch_e5m2f(b); }
static inline uint8_t _arch_e5m2_isnan(uint8_t a){ return (((a>>2)&0x1Fu)==0x1Fu && (a&3u)!=0u) ? 1 : 0; }
static inline uint8_t _arch_e4m3_add(uint8_t a,uint8_t b){ return _arch_f32_to_e4m3(_arch_b32f(_arch_e4m3f(a)+_arch_e4m3f(b))); }
static inline uint8_t _arch_e4m3_sub(uint8_t a,uint8_t b){ return _arch_f32_to_e4m3(_arch_b32f(_arch_e4m3f(a)-_arch_e4m3f(b))); }
static inline uint8_t _arch_e4m3_mul(uint8_t a,uint8_t b){ return _arch_f32_to_e4m3(_arch_b32f(_arch_e4m3f(a)*_arch_e4m3f(b))); }
static inline uint8_t _arch_fma_e4m3(uint8_t a,uint8_t b,uint8_t c){ return _arch_f32_to_e4m3(_arch_b32f(fmaf(_arch_e4m3f(a),_arch_e4m3f(b),_arch_e4m3f(c)))); }
static inline uint8_t _arch_e4m3_eq(uint8_t a,uint8_t b){ return _arch_e4m3f(a)==_arch_e4m3f(b); }
static inline uint8_t _arch_e4m3_ne(uint8_t a,uint8_t b){ return _arch_e4m3f(a)!=_arch_e4m3f(b); }
static inline uint8_t _arch_e4m3_lt(uint8_t a,uint8_t b){ return _arch_e4m3f(a)< _arch_e4m3f(b); }
static inline uint8_t _arch_e4m3_gt(uint8_t a,uint8_t b){ return _arch_e4m3f(a)> _arch_e4m3f(b); }
static inline uint8_t _arch_e4m3_le(uint8_t a,uint8_t b){ return _arch_e4m3f(a)<=_arch_e4m3f(b); }
static inline uint8_t _arch_e4m3_ge(uint8_t a,uint8_t b){ return _arch_e4m3f(a)>=_arch_e4m3f(b); }
static inline uint8_t _arch_e4m3_isnan(uint8_t a){ return ((a&0x7Fu)==0x7Fu) ? 1 : 0; }
"#.to_string();
        // Profile shim (doc/archive/plan_fp_types.md §6.2): the `cuda` profile differs
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
                )
                // fp8 narrow: cuda = PTX satfinite (overflow and input ±inf
                // saturate to ±max-finite) + cuda canonical E5M2 NaN 0x7F.
                .replace(
                    "_arch_f32_to_fp8(x,5,2,0,0x7Cu,1,0x7Eu)",
                    "_arch_f32_to_fp8(x,5,2,0,0x7Bu,1,0x7Fu)",
                )
                .replace(
                    "_arch_f32_to_fp8(x,4,3,1,0x7Fu,0,0x7Fu)",
                    "_arch_f32_to_fp8(x,4,3,1,0x7Eu,1,0x7Fu)",
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
