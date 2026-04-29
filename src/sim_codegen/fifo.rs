//! `gen_fifo` emitter — extracted from `sim_codegen/mod.rs`. Follows
//! the same submodule pattern as fsm.rs / pipeline.rs / ram.rs.

use super::{SimCodegen, SimModel};
use super::*;

impl<'a> SimCodegen<'a> {
    pub(crate) fn gen_fifo(&self, f: &FifoDecl) -> SimModel {
        let name = &f.name.name;
        let class = format!("V{name}");
        let is_lifo = f.kind == FifoKind::Lifo;

        // Resolve DEPTH and TYPE params
        let depth: u64 = f.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
            .unwrap_or(8);

        let elem_ty: String = f.params.iter()
            .find(|p| p.name.name == "TYPE")
            .and_then(|p| if let ParamKind::Type(te) = &p.kind { Some(te) } else { None })
            .map(cpp_internal_type)
            .unwrap_or_else(|| "uint32_t".to_string());

        let (rst_name, _is_async, is_low) = extract_reset_info(&f.ports);
        let rst_cond = if is_low { format!("(!{rst_name})") } else { rst_name.clone() };

        // Build type-param substitution: param name → concrete TypeExpr (from default)
        let type_param_map: std::collections::HashMap<String, TypeExpr> = f.params.iter()
            .filter_map(|p| if let ParamKind::Type(te) = &p.kind { Some((p.name.name.clone(), te.clone())) } else { None })
            .collect();
        // Resolve a port's TypeExpr, substituting Named types that match type params
        let resolve_port_ty = |ty: &TypeExpr| -> String {
            if let TypeExpr::Named(n) = ty {
                if let Some(concrete) = type_param_map.get(&n.name) {
                    return cpp_port_type(concrete);
                }
            }
            cpp_port_type(ty)
        };

        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\n");
        h.push_str(&format!("class {class} {{\npublic:\n"));
        let port_names: HashSet<&str> = f.ports.iter().map(|p| p.name.name.as_str()).collect();
        for p in &f.ports {
            let ty = resolve_port_ty(&p.ty);
            h.push_str(&format!("  {ty} {};\n", p.name.name));
        }
        // Add full/empty fields only if not already declared as ports
        if !is_lifo {
            if !port_names.contains("full") { h.push_str("  uint8_t full;\n"); }
            if !port_names.contains("empty") { h.push_str("  uint8_t empty;\n"); }
        }
        h.push('\n');

        // Constructor — use () for data ports, 0 for scalars
        let mut port_inits: Vec<String> = f.ports.iter().map(|p| {
            if matches!(p.ty, TypeExpr::Named(_)) { format!("{}()", p.name.name) }
            else { format!("{}(0)", p.name.name) }
        }).collect();
        if !is_lifo {
            if !port_names.contains("full") { port_inits.push("full(0)".to_string()); }
            if !port_names.contains("empty") { port_inits.push("empty(1)".to_string()); }
        }
        // Detect dual-clock (async FIFO): wr_clk + rd_clk on different
        // domains. Single-clock case keeps the original `_clk_prev` member.
        let clk_ports: Vec<&str> = f.ports.iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .collect();
        let is_async = clk_ports.len() >= 2
            && clk_ports.iter().any(|n| *n == "wr_clk")
            && clk_ports.iter().any(|n| *n == "rd_clk");
        if is_async {
            port_inits.push("_clk_prev_wr(0)".to_string());
            port_inits.push("_clk_prev_rd(0)".to_string());
        } else {
            port_inits.push("_clk_prev(0)".to_string());
        }
        if is_lifo {
            port_inits.push("_sp(0)".to_string());
        } else {
            port_inits.push("_wr_ptr(0)".to_string());
            port_inits.push("_rd_ptr(0)".to_string());
        }
        h.push_str(&format!("  {class}() : {} {{\n    memset(_mem, 0, sizeof(_mem));\n  }}\n",
            port_inits.join(", ")));
        h.push_str(&format!("  explicit {class}(VerilatedContext*) : {class}() {{}}\n"));
        h.push_str("  void eval();\n  void eval_posedge();\n  void eval_comb();\n");
        if is_async {
            h.push_str("  void eval_posedge_dual(bool _wr_rising, bool _rd_rising);\n");
        }
        h.push_str("  void final() { trace_close(); }\n");
        h.push_str("private:\n");
        if is_async {
            h.push_str("  uint8_t _clk_prev_wr;\n  uint8_t _clk_prev_rd;\n");
        } else {
            h.push_str("  uint8_t _clk_prev;\n");
        }
        if is_lifo {
            h.push_str("  uint32_t _sp;\n");
        } else {
            h.push_str("  uint32_t _wr_ptr;\n  uint32_t _rd_ptr;\n");
        }
        h.push_str(&format!("  {elem_ty} _mem[{depth}];\n"));
        h.push_str("  void trace_open(const char* filename);\n");
        h.push_str("  void trace_dump(uint64_t time);\n");
        h.push_str("  void trace_close();\n");
        h.push_str("  FILE* _trace_fp = nullptr;\n  uint64_t _trace_time = 0;\n");
        h.push_str("};\n");

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str()).unwrap_or("clk");

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        if is_async {
            // Async FIFO: independent edge detection per side. Push side
            // clocks on wr_clk; pop side clocks on rd_clk. We do not model
            // gray-code pointer synchroniser delay — sim is 2-state and
            // same-tick, so the synth-level CDC settles in zero time here.
            cpp.push_str("  bool _wr_rising = (wr_clk && !_clk_prev_wr);\n");
            cpp.push_str("  bool _rd_rising = (rd_clk && !_clk_prev_rd);\n");
            cpp.push_str("  _clk_prev_wr = wr_clk;\n");
            cpp.push_str("  _clk_prev_rd = rd_clk;\n");
            cpp.push_str("  if (_wr_rising || _rd_rising) eval_posedge_dual(_wr_rising, _rd_rising);\n");
        } else {
            cpp.push_str(&format!("  if ({clk_port} && !_clk_prev) eval_posedge();\n"));
            cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        }
        cpp.push_str("  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        if is_async {
            // Dual-clock posedge handler: split push/pop sides cleanly.
            cpp.push_str(&format!("void {class}::eval_posedge_dual(bool _wr_rising, bool _rd_rising) {{\n"));
            cpp.push_str(&format!("  if ({rst_cond}) {{\n"));
            cpp.push_str("    _wr_ptr = 0; _rd_ptr = 0;\n");
            cpp.push_str("    return;\n  }\n");
            cpp.push_str("  if (_wr_rising && push_valid && push_ready) {\n");
            cpp.push_str(&format!("    _mem[_wr_ptr % {depth}] = push_data;\n"));
            cpp.push_str("    _wr_ptr++;\n");
            cpp.push_str(&format!("    if (_wr_ptr >= 2u * {depth}) _wr_ptr = 0;\n  }}\n"));
            cpp.push_str("  if (_rd_rising && pop_ready && pop_valid) {\n");
            cpp.push_str("    _rd_ptr++;\n");
            cpp.push_str(&format!("    if (_rd_ptr >= 2u * {depth}) _rd_ptr = 0;\n  }}\n"));
            cpp.push_str("}\n\n");
            // Keep the standard eval_posedge symbol for ABI parity (unused).
            cpp.push_str(&format!("void {class}::eval_posedge() {{}}\n\n"));
        } else {
            // eval_posedge() — single-clock path.
            cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
            cpp.push_str(&format!("  if ({rst_cond}) {{\n"));
            if is_lifo {
                cpp.push_str("    _sp = 0;\n");
            } else {
                cpp.push_str("    _wr_ptr = 0; _rd_ptr = 0;\n");
            }
            cpp.push_str("  } else {\n");
            if is_lifo {
                cpp.push_str("    if (push_valid && push_ready) {\n");
                cpp.push_str(&format!("      _mem[_sp % {depth}] = push_data;\n"));
                cpp.push_str("      _sp++;\n    }\n");
                cpp.push_str("    if (pop_ready && pop_valid) {\n");
                cpp.push_str("      if (_sp > 0) _sp--;\n    }\n");
            } else {
                cpp.push_str("    if (push_valid && push_ready) {\n");
                cpp.push_str(&format!("      _mem[_wr_ptr % {depth}] = push_data;\n"));
                cpp.push_str("      _wr_ptr++;\n");
                cpp.push_str(&format!("      if (_wr_ptr >= 2u * {depth}) _wr_ptr = 0;\n    }}\n"));
                cpp.push_str("    if (pop_ready && pop_valid) {\n");
                cpp.push_str("      _rd_ptr++;\n");
                cpp.push_str(&format!("      if (_rd_ptr >= 2u * {depth}) _rd_ptr = 0;\n    }}\n"));
            }
            cpp.push_str("  }\n}\n\n");
        }

        // eval_comb()
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        if is_lifo {
            cpp.push_str(&format!("  push_ready = (_sp < {depth}u);\n"));
            cpp.push_str("  pop_valid  = (_sp > 0);\n");
            cpp.push_str(&format!("  pop_data   = _mem[(_sp > 0 ? _sp - 1 : 0) % {depth}];\n"));
        } else {
            cpp.push_str(&format!("  uint32_t _depth = {depth};\n"));
            cpp.push_str("  uint32_t _count = (_wr_ptr >= _rd_ptr)\n");
            cpp.push_str("    ? (_wr_ptr - _rd_ptr)\n");
            cpp.push_str("    : (2u * _depth - _rd_ptr + _wr_ptr);\n");
            cpp.push_str("  full       = (_count >= _depth);\n");
            cpp.push_str("  empty      = (_count == 0);\n");
            cpp.push_str("  push_ready = !full;\n");
            cpp.push_str("  pop_valid  = !empty;\n");
            cpp.push_str("  pop_data = _mem[_rd_ptr % _depth];\n");
        }
        cpp.push_str("}\n\n");

        // Trace methods
        cpp.push_str(&format!("void {class}::trace_open(const char* filename) {{\n"));
        cpp.push_str("  _trace_fp = fopen(filename, \"w\");\n");
        cpp.push_str("  if (!_trace_fp) return;\n");
        cpp.push_str("  fprintf(_trace_fp, \"$timescale 1ns $end\\n\");\n");
        cpp.push_str(&format!("  fprintf(_trace_fp, \"$scope module {} $end\\n\");\n", name));
        let mut sig_idx = 0usize;
        for p in &f.ports {
            let w = type_width(&p.ty);
            let id = vcd_id(sig_idx); sig_idx += 1;
            let pname = &p.name.name;
            cpp.push_str(&format!("  fprintf(_trace_fp, \"$var wire {w} {id} {pname} $end\\n\");\n"));
        }
        cpp.push_str("  fprintf(_trace_fp, \"$upscope $end\\n$enddefinitions $end\\n\");\n");
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::trace_dump(uint64_t time) {{\n"));
        cpp.push_str("  if (!_trace_fp) return;\n");
        cpp.push_str("  fprintf(_trace_fp, \"#%lu\\n\", (unsigned long)time);\n");
        sig_idx = 0;
        for p in &f.ports {
            let w = type_width(&p.ty);
            let id = vcd_id(sig_idx); sig_idx += 1;
            let pname = &p.name.name;
            if w == 1 {
                cpp.push_str(&format!("  fprintf(_trace_fp, \"%c{}\\n\", {pname} ? '1' : '0');\n", id));
            } else {
                cpp.push_str("  fprintf(_trace_fp, \"b\");\n");
                cpp.push_str(&format!("  for (int _i = {w} - 1; _i >= 0; _i--) fprintf(_trace_fp, \"%c\", (int)(({pname} >> _i) & 1) ? '1' : '0');\n"));
                cpp.push_str(&format!("  fprintf(_trace_fp, \" {}\\n\");\n", id));
            }
        }
        cpp.push_str("}\n\n");
        cpp.push_str(&format!("void {class}::trace_close() {{\n"));
        cpp.push_str("  if (_trace_fp) {{ fclose(_trace_fp); _trace_fp = nullptr; }}\n");
        cpp.push_str("}\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }

}
