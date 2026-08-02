//! `trace` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

/// A signal to be traced in VCD output.
pub(crate) struct TraceSignal {
    pub(crate) vcd_name: String, // display name in VCD scope
    pub(crate) cpp_expr: String, // C++ expression to read the value
    pub(crate) width: u32,       // bit width
    pub(crate) is_wide: bool,    // true if VlWide<N> type
}

/// Generate a short VCD identifier from a signal index.
/// Uses alphanumeric chars only (a-z, A-Z, 0-9) to avoid C string/printf conflicts.
pub(super) fn vcd_id(index: usize) -> String {
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
pub(super) fn collect_trace_signals(
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

/// Add VCD trace support to a non-module construct (counter, fsm, ram, etc.).
/// Patches the header and cpp strings in place. Call BEFORE closing `};\n` in header
/// and AFTER all method impls in cpp.
///
/// `extra_signals`: additional internal signals to trace (name, cpp_expr, width).
pub(super) fn add_trace_to_simple_construct(
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
    // Named struct/enum ports are not scalar-shiftable in the C++ native sim.
    // `params` is the enclosing construct's param list, used to resolve
    // `UInt<PARAM>` / `SInt<PARAM>` widths in port VCD declarations to
    // their real bit width rather than the legacy 32-default. See
    // arch-com#447 §1 / PR following #458 for the migration that closed
    // this footgun.
    let mut signals = Vec::new();
    for p in ports {
        if ty_references_named(&p.ty) {
            continue;
        } // handled elsewhere or intentionally untraced when struct-typed
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

#[derive(Clone, Debug)]
pub(crate) struct SimpleDebugPort {
    pub name: String,
    pub cpp_ty: String,
    pub bits: u32,
    pub dir: String,
}

pub(crate) fn collect_simple_debug_ports(
    ports: &[PortDecl],
    params: &[ParamDecl],
) -> Vec<SimpleDebugPort> {
    let mut out = Vec::new();
    for p in ports {
        // Skip Clock (and Reset is kept? module skips only Clock)
        if matches!(&p.ty, TypeExpr::Clock(_)) {
            continue;
        }
        if p.bus_info.is_some() {
            // Bus ports are flattened elsewhere; skip here for simple case
            // Caller can extend with bus_flat if needed.
            continue;
        }
        // Skip named struct/enum ports (not scalar)
        if ty_references_named(&p.ty) {
            continue;
        }
        let dir = match p.direction {
            Direction::In => "in".to_string(),
            Direction::Out => "out".to_string(),
        };
        // Vec handling
        if let Some((elem_ty_str, _count_str)) = vec_array_info_with_params(&p.ty, params) {
            // Resolve actual count via param-aware eval
            let count_val = if let TypeExpr::Vec(_, cnt_expr) = &p.ty {
                eval_const_expr_with_params(cnt_expr, params) as u64
            } else {
                0
            };
            let bits = if let TypeExpr::Vec(elem, _) = &p.ty {
                type_bits_te_with_params(elem, params)
            } else {
                32
            };
            for i in 0..count_val {
                out.push(SimpleDebugPort {
                    name: format!("{}_{}", p.name.name, i),
                    cpp_ty: elem_ty_str.clone(),
                    bits,
                    dir: dir.clone(),
                });
            }
        } else {
            let bits = type_bits_te_with_params(&p.ty, params);
            let cpp_ty = cpp_port_type_with_params(&p.ty, params);
            out.push(SimpleDebugPort {
                name: p.name.name.clone(),
                cpp_ty,
                bits,
                dir,
            });
        }
    }
    out
}

pub(crate) fn emit_simple_debug_header(h: &mut String, ports: &[SimpleDebugPort]) {
    if ports.is_empty() {
        return;
    }
    h.push_str("  // --debug port shadow copies\n");
    for p in ports {
        if p.bits > 64 {
            // Use same wide type as port for shadow
            h.push_str(&format!("  {} _dbg_prev_{};\n", p.cpp_ty, p.name));
        } else {
            h.push_str(&format!("  {} _dbg_prev_{} = 0;\n", p.cpp_ty, p.name));
        }
    }
    h.push_str("  uint64_t _dbg_cycle = 0;\n");
}

pub(crate) fn emit_simple_debug_impl(
    cpp: &mut String,
    class: &str,
    construct_name: &str,
    ports: &[SimpleDebugPort],
    clk_port: Option<&str>,
) {
    if ports.is_empty() {
        return;
    }
    cpp.push_str(&format!("void {class}::_debug_log_ports() {{\n"));
    for p in ports {
        let dir = &p.dir;
        if p.bits > 64 {
            let words = wide_words(p.bits);
            cpp.push_str(&format!(
                "  if (memcmp(&{name}, &_dbg_prev_{name}, sizeof({name})) != 0) {{\n",
                name = p.name
            ));
            cpp.push_str(&format!(
                "    printf(\"[%llu][{cname}.{pname}]({dir}) 0x\");\n",
                cname = construct_name,
                pname = p.name,
                dir = dir
            ));
            cpp.push_str(&format!(
                "    for (int _w = {words} - 1; _w >= 0; _w--) printf(\"%08x\", _dbg_prev_{name}.data()[_w]);\n",
                name = p.name,
                words = words
            ));
            cpp.push_str("    printf(\" -> 0x\");\n");
            cpp.push_str(&format!(
                "    for (int _w = {words} - 1; _w >= 0; _w--) printf(\"%08x\", {name}.data()[_w]);\n",
                name = p.name,
                words = words
            ));
            cpp.push_str("    printf(\"\\n\");\n");
            cpp.push_str(&format!("    _dbg_prev_{name} = {name};\n", name = p.name));
            cpp.push_str("  }\n");
        } else {
            cpp.push_str(&format!(
                "  if ({name} != _dbg_prev_{name}) {{\n",
                name = p.name
            ));
            cpp.push_str(&format!(
                "    printf(\"[%llu][{cname}.{pname}]({dir}) 0x%llx -> 0x%llx\\n\", (unsigned long long)_dbg_cycle, (unsigned long long)_dbg_prev_{name}, (unsigned long long){name});\n",
                cname = construct_name,
                pname = p.name,
                dir = dir,
                name = p.name
            ));
            cpp.push_str(&format!("    _dbg_prev_{name} = {name};\n", name = p.name));
            cpp.push_str("  }\n");
        }
    }
    // Cycle increment
    if let Some(clk) = clk_port {
        cpp.push_str(&format!("  if ({clk} && !_clk_prev) _dbg_cycle++;\n"));
    } else {
        cpp.push_str("  _dbg_cycle++;\n");
    }
    cpp.push_str("}\n\n");
}
