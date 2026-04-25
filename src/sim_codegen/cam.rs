//! `gen_cam` emitter — Phase C of the CAM construct rollout.
//!
//! Mirrors the SV codegen in `codegen.rs::emit_cam` for the C++ sim.
//! Storage: `entry_valid_r` (packed bitmask, uint`DEPTH`) and
//! `entry_key_r[DEPTH]` (per-entry key). Comb match recomputes
//! `search_mask` / `search_any` / `search_first` every cycle.

use super::{SimCodegen, SimModel};
use super::*;

impl<'a> SimCodegen<'a> {
    pub(super) fn gen_cam(&self, c: &CamDecl) -> SimModel {
        let name = &c.name.name;
        let class = format!("V{name}");

        // DEPTH and KEY_W are required const params (typecheck enforces).
        let depth: u32 = c.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v as u32) } else { None })
            .unwrap_or(32);
        let key_w: u32 = c.params.iter()
            .find(|p| p.name.name == "KEY_W")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v as u32) } else { None })
            .unwrap_or(8);
        // v3: optional value payload.
        let has_value = c.params.iter().any(|p| p.name.name == "VAL_W");
        let val_w: u32 = c.params.iter()
            .find(|p| p.name.name == "VAL_W")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v as u32) } else { None })
            .unwrap_or(8);

        let key_ty = cpp_uint(key_w);
        let val_ty = cpp_uint(val_w);
        let mask_ty = cpp_uint(depth);
        let idx_w = if depth <= 1 { 1 } else { 32 - (depth - 1).leading_zeros() };
        let idx_ty = cpp_uint(idx_w);

        let (rst_name, _is_async, is_low) = extract_reset_info(&c.ports);
        let rst_cond = if is_low { format!("(!{})", rst_name) } else { rst_name.clone() };
        let clk_port = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());

        // ── Header ──
        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include <cstring>\n#include \"verilated.h\"\n\n");
        h.push_str(&format!("class {class} {{\npublic:\n"));
        for p in &c.ports {
            h.push_str(&format!("  {} {};\n", cpp_port_type(&p.ty), p.name.name));
        }
        h.push('\n');

        // Ctor: zero ports + state.
        let port_inits: Vec<String> = c.ports.iter().map(|p| format!("{}(0)", p.name.name)).collect();
        let state_inits = vec!["_clk_prev(0)".to_string(), "_entry_valid_r(0)".to_string()];
        let all_inits: Vec<String> = port_inits.into_iter().chain(state_inits).collect();
        h.push_str(&format!("  {class}() : {} {{\n", all_inits.join(", ")));
        h.push_str("    memset(_entry_key_r, 0, sizeof(_entry_key_r));\n");
        if has_value {
            h.push_str("    memset(_entry_value_r, 0, sizeof(_entry_value_r));\n");
        }
        h.push_str("  }\n");
        h.push_str(&format!("  explicit {class}(VerilatedContext*) : {class}() {{}}\n"));
        h.push_str("  void eval();\n  void eval_posedge();\n  void eval_comb();\n  void final() { trace_close(); }\n");
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  {mask_ty} _entry_valid_r;\n"));
        h.push_str(&format!("  {key_ty} _entry_key_r[{depth}];\n"));
        if has_value {
            h.push_str(&format!("  {val_ty} _entry_value_r[{depth}];\n"));
        }

        // ── Implementation ──
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str("  eval_posedge();\n  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        // posedge: write port (set or clear). v2: optional second write port,
        // applied after port 1, so port 2 wins on same-index conflict.
        let has_w2 = c.ports.iter().any(|p| p.name.name == "write2_valid");

        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (!_rising) return;\n");
        cpp.push_str(&format!("  if ({rst_cond}) {{\n"));
        cpp.push_str("    _entry_valid_r = 0;\n");
        cpp.push_str("    return;\n");
        cpp.push_str("  }\n");
        // Port 1
        cpp.push_str("  if (write_valid) {\n");
        cpp.push_str(&format!("    {mask_ty} _bit = ({mask_ty})1 << write_idx;\n"));
        cpp.push_str("    if (write_set) {\n");
        cpp.push_str("      _entry_valid_r |= _bit;\n");
        cpp.push_str("      _entry_key_r[write_idx] = write_key;\n");
        if has_value {
            cpp.push_str("      _entry_value_r[write_idx] = write_value;\n");
        }
        cpp.push_str("    } else {\n");
        cpp.push_str("      _entry_valid_r &= ~_bit;\n");
        cpp.push_str("    }\n");
        cpp.push_str("  }\n");
        // Port 2 (v2)
        if has_w2 {
            cpp.push_str("  if (write2_valid) {\n");
            cpp.push_str(&format!("    {mask_ty} _bit2 = ({mask_ty})1 << write2_idx;\n"));
            cpp.push_str("    if (write2_set) {\n");
            cpp.push_str("      _entry_valid_r |= _bit2;\n");
            cpp.push_str("      _entry_key_r[write2_idx] = write2_key;\n");
            if has_value {
                cpp.push_str("      _entry_value_r[write2_idx] = write2_value;\n");
            }
            cpp.push_str("    } else {\n");
            cpp.push_str("      _entry_valid_r &= ~_bit2;\n");
            cpp.push_str("    }\n");
            cpp.push_str("  }\n");
        }
        cpp.push_str("}\n\n");

        // comb: recompute search_mask / search_any / search_first.
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        cpp.push_str(&format!("  {mask_ty} _m = 0;\n"));
        cpp.push_str(&format!("  for (uint32_t i = 0; i < {depth}; i++) {{\n"));
        cpp.push_str(&format!("    if ((_entry_valid_r >> i) & 1) {{\n"));
        cpp.push_str("      if (_entry_key_r[i] == search_key) {\n");
        cpp.push_str(&format!("        _m |= ({mask_ty})1 << i;\n"));
        cpp.push_str("      }\n");
        cpp.push_str("    }\n");
        cpp.push_str("  }\n");
        cpp.push_str("  search_mask = _m;\n");
        cpp.push_str("  search_any = (_m != 0) ? 1 : 0;\n");
        cpp.push_str(&format!("  {idx_ty} _first = 0;\n"));
        cpp.push_str(&format!("  for (uint32_t i = 0; i < {depth}; i++) {{\n"));
        cpp.push_str("    if ((_m >> i) & 1) {\n");
        cpp.push_str(&format!("      _first = ({idx_ty})i;\n"));
        cpp.push_str("      break;\n");
        cpp.push_str("    }\n");
        cpp.push_str("  }\n");
        cpp.push_str("  search_first = _first;\n");
        if has_value {
            // Mux into entry_value_r at search_first; caller qualifies with search_any.
            cpp.push_str("  read_value = _entry_value_r[_first];\n");
        }
        cpp.push_str("}\n");

        // Suppress unused-variable warning when val_ty isn't referenced.
        let _ = val_ty;

        // Trace support — track entry_valid_r and search outputs.
        let extra_sigs: Vec<(&str, &str, u32)> = vec![
            ("entry_valid_r", "_entry_valid_r", depth),
        ];
        add_trace_to_simple_construct(&mut h, &mut cpp, &class, name, &c.ports, &extra_sigs);
        h.push_str("};\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }
}
