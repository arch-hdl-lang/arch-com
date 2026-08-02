//! `counter` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

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
