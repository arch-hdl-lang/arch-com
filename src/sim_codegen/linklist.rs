//! `gen_linklist` emitter — extracted from `sim_codegen/mod.rs`. Follows
//! the same submodule pattern as fsm.rs / pipeline.rs / ram.rs / fifo.rs.

use super::{SimCodegen, SimModel};
use super::*;

impl<'a> SimCodegen<'a> {
    pub(super) fn gen_linklist(&self, l: &crate::ast::LinklistDecl) -> SimModel {
        use crate::ast::{ExprKind, LitKind, LinklistKind, Direction};

        let name  = &l.name.name;
        let class = format!("V{name}");

        let param_int = |pname: &str, default: u64| -> u64 {
            l.params.iter()
                .find(|p| p.name.name == pname)
                .and_then(|p| p.default.as_ref())
                .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
                .unwrap_or(default)
        };
        let depth = param_int("DEPTH", 8) as usize;
        let handle_mask = (1u64 << ((depth as f64).log2().ceil() as u32)) - 1;
        let cnt_mask    = (1u64 << (((depth + 1) as f64).log2().ceil() as u32)) - 1;

        let data_cpp: String = l.params.iter()
            .find(|p| p.name.name == "DATA")
            .map(|p| match &p.kind {
                crate::ast::ParamKind::Type(te) => cpp_port_type(te),
                _ => "uint32_t".to_string(),
            })
            .unwrap_or_else(|| "uint32_t".to_string());

        let has_doubly = matches!(l.kind, LinklistKind::Doubly | LinklistKind::CircularDoubly);

        let is_out_data = |p: &crate::ast::PortDecl| {
            p.direction == Direction::Out
                && p.name.name != "req_ready"
                && p.name.name != "resp_valid"
        };

        // ── Header ────────────────────────────────────────────────────────────
        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\n");
        h.push_str(&format!("class {class} {{\npublic:\n"));
        h.push_str("  uint8_t clk;\n  uint8_t rst;\n");
        for op in &l.ops {
            for p in &op.ports {
                h.push_str(&format!("  {} {}_{};\n", cpp_port_type(&p.ty), op.name.name, p.name.name));
            }
        }
        for p in &l.ports {
            match p.name.name.as_str() {
                "clk" | "rst" => {}
                _ => { h.push_str(&format!("  {} {};\n", cpp_port_type(&p.ty), p.name.name)); }
            }
        }
        h.push('\n');

        let mut ctor_inits: Vec<String> = vec!["clk(0)".into(), "rst(0)".into()];
        for op in &l.ops {
            for p in &op.ports {
                ctor_inits.push(format!("{}_{} (0)", op.name.name, p.name.name));
            }
        }
        for p in &l.ports {
            match p.name.name.as_str() {
                "clk" | "rst" => {}
                _ => { ctor_inits.push(format!("{}(0)", p.name.name)); }
            }
        }
        ctor_inits.extend([
            "_clk_prev(0)".into(), "_fl_rdp(0)".into(), "_fl_wrp(0)".into(),
            format!("_fl_cnt({depth})"), "_head_r(0)".into(), "_tail_r(0)".into(),
        ]);
        for op in &l.ops {
            let on = &op.name.name;
            if op.latency > 1 { ctor_inits.push(format!("_ctrl_{on}_busy(0)")); }
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                ctor_inits.push(format!("_ctrl_{on}_resp_v(0)"));
            }
            for p in op.ports.iter().filter(|p| is_out_data(p)) {
                ctor_inits.push(format!("_ctrl_{on}_{}(0)", p.name.name));
            }
            if on == "delete_head" || on == "delete" {
                ctor_inits.push(format!("_ctrl_{on}_slot(0)"));
            }
            if on == "insert_tail" || on == "insert_head" {
                ctor_inits.push(format!("_ctrl_{on}_was_empty(0)"));
            }
            if on == "insert_after" {
                ctor_inits.push(format!("_ctrl_{on}_after_handle(0)"));
            }
        }
        h.push_str(&format!("  {class}() : {} {{\n", ctor_inits.join(", ")));
        h.push_str(&format!("    for (int _i = 0; _i < {depth}; _i++) _fl_mem[_i] = (uint8_t)_i;\n"));
        h.push_str("    memset(_data_mem, 0, sizeof(_data_mem));\n");
        h.push_str("    memset(_next_mem, 0, sizeof(_next_mem));\n");
        if has_doubly { h.push_str("    memset(_prev_mem, 0, sizeof(_prev_mem));\n"); }
        h.push_str("  }\n");
        h.push_str("  void eval();\n  void eval_comb();\n  void eval_posedge();\n  void final() { trace_close(); }\n\nprivate:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  uint8_t _fl_mem[{depth}];\n"));
        h.push_str(&format!("  {data_cpp} _data_mem[{depth}];\n"));
        h.push_str(&format!("  uint8_t _next_mem[{depth}];\n"));
        if has_doubly { h.push_str(&format!("  uint8_t _prev_mem[{depth}];\n")); }
        h.push_str("  uint8_t _fl_rdp, _fl_wrp;\n  uint8_t _fl_cnt;\n  uint8_t _head_r, _tail_r;\n");
        for op in &l.ops {
            let on = &op.name.name;
            if op.latency > 1 { h.push_str(&format!("  uint8_t _ctrl_{on}_busy;\n")); }
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                h.push_str(&format!("  uint8_t _ctrl_{on}_resp_v;\n"));
            }
            for p in op.ports.iter().filter(|p| is_out_data(p)) {
                h.push_str(&format!("  {} _ctrl_{on}_{};\n", cpp_port_type(&p.ty), p.name.name));
            }
            if on == "delete_head" || on == "delete" {
                h.push_str(&format!("  uint8_t _ctrl_{on}_slot;\n"));
            }
            if on == "insert_tail" || on == "insert_head" {
                h.push_str(&format!("  uint8_t _ctrl_{on}_was_empty;\n"));
            }
            if on == "insert_after" {
                h.push_str(&format!("  uint8_t _ctrl_{on}_after_handle;\n"));
            }
        }

        // ── Implementation ────────────────────────────────────────────────────
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));
        cpp.push_str(&format!(
            "void {class}::eval() {{\n\
             \n  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n\
             \n    trace_open(Verilated::traceFile());\n\
             \n  bool _rising = (clk && !_clk_prev);\n\
             \n  _clk_prev = clk;\n\
             \n  if (_rising) eval_posedge();\n\
             \n  eval_comb();\n\
             \n  if (_trace_fp) trace_dump(_trace_time++);\n}}\n\n"
        ));

        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        cpp.push_str(&format!("  empty  = (_fl_cnt == {depth});\n"));
        cpp.push_str("  full   = (_fl_cnt == 0);\n");
        cpp.push_str(&format!("  length = (uint8_t)(({depth} - _fl_cnt) & {cnt_mask:#x});\n"));
        for op in &l.ops {
            let on = &op.name.name;
            // req_ready — only if the op declares it
            if op.ports.iter().any(|p| p.name.name == "req_ready") {
                let rdy: String = match on.as_str() {
                    "alloc"  => "(_fl_cnt != 0)".into(),
                    "free"   => format!("(_fl_cnt != {depth})"),
                    "insert_tail" | "insert_head" | "insert_after" =>
                        format!("(!_ctrl_{on}_busy && _fl_cnt != 0)"),
                    "delete_head" | "delete" =>
                        format!("(!_ctrl_{on}_busy && _fl_cnt != {depth})"),
                    _ => "1".into(),
                };
                cpp.push_str(&format!("  {on}_req_ready = {rdy};\n"));
            }
            // Route controller regs → output ports (always, regardless of req_ready)
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                cpp.push_str(&format!("  {on}_resp_valid = _ctrl_{on}_resp_v;\n"));
            }
            for p in op.ports.iter().filter(|p| is_out_data(p)) {
                cpp.push_str(&format!("  {on}_{} = _ctrl_{on}_{};\n", p.name.name, p.name.name));
            }
        }
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str("  if (rst) {\n");
        cpp.push_str(&format!("    for (int _i = 0; _i < {depth}; _i++) _fl_mem[_i] = (uint8_t)_i;\n"));
        cpp.push_str("    _fl_rdp = 0; _fl_wrp = 0;\n");
        cpp.push_str(&format!("    _fl_cnt = {depth};\n"));
        cpp.push_str("    _head_r = 0; _tail_r = 0;\n");
        for op in &l.ops {
            let on = &op.name.name;
            if op.latency > 1 { cpp.push_str(&format!("    _ctrl_{on}_busy = 0;\n")); }
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                cpp.push_str(&format!("    _ctrl_{on}_resp_v = 0;\n"));
            }
        }
        cpp.push_str("  } else {\n");
        for op in &l.ops {
            let on = &op.name.name;
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                cpp.push_str(&format!("    _ctrl_{on}_resp_v = 0;\n"));
            }
        }
        for op in &l.ops {
            let on = &op.name.name;
            cpp.push_str(&format!("    // ── {on}\n"));
            match on.as_str() {
                "alloc" => cpp.push_str(&format!(
                    "    if ({on}_req_valid && _fl_cnt != 0) {{\n\
                     \n      uint8_t _slot = _fl_mem[_fl_rdp & {handle_mask:#x}];\n\
                     \n      _fl_rdp = (uint8_t)((_fl_rdp + 1) & {cnt_mask:#x});\n\
                     \n      _fl_cnt--; _ctrl_{on}_resp_v = 1; _ctrl_{on}_resp_handle = _slot;\n    }}\n"
                )),
                "free" => cpp.push_str(&format!(
                    "    if ({on}_req_valid) {{\n\
                     \n      _fl_mem[_fl_wrp & {handle_mask:#x}] = {on}_req_handle;\n\
                     \n      _fl_wrp = (uint8_t)((_fl_wrp + 1) & {cnt_mask:#x}); _fl_cnt++;\n    }}\n"
                )),
                "insert_tail" => cpp.push_str(&format!(
                    "    if (!_ctrl_{on}_busy && {on}_req_valid && _fl_cnt != 0) {{\n\
                     \n      uint8_t _slot = _fl_mem[_fl_rdp & {handle_mask:#x}];\n\
                     \n      _ctrl_{on}_resp_handle = _slot; _data_mem[_slot] = {on}_req_data;\n\
                     \n      _ctrl_{on}_was_empty = (_fl_cnt == {depth});\n\
                     \n      _fl_rdp = (uint8_t)((_fl_rdp + 1) & {cnt_mask:#x}); _fl_cnt--; _ctrl_{on}_busy = 1;\n\
                     \n    }} else if (_ctrl_{on}_busy) {{\n\
                     \n      if (!_ctrl_{on}_was_empty) _next_mem[_tail_r] = _ctrl_{on}_resp_handle;\n\
                     \n      {doubly_insert_tail}\
                     \n      _tail_r = _ctrl_{on}_resp_handle;\n\
                     \n      if (_ctrl_{on}_was_empty) _head_r = _ctrl_{on}_resp_handle;\n\
                     \n      _ctrl_{on}_resp_v = 1; _ctrl_{on}_busy = 0;\n    }}\n",
                    doubly_insert_tail = if has_doubly {
                        format!("_prev_mem[_ctrl_{on}_resp_handle] = _tail_r;\n      ")
                    } else { String::new() }
                )),
                "insert_head" => cpp.push_str(&format!(
                    "    if (!_ctrl_{on}_busy && {on}_req_valid && _fl_cnt != 0) {{\n\
                     \n      uint8_t _slot = _fl_mem[_fl_rdp & {handle_mask:#x}];\n\
                     \n      _ctrl_{on}_resp_handle = _slot; _data_mem[_slot] = {on}_req_data;\n\
                     \n      _ctrl_{on}_was_empty = (_fl_cnt == {depth});\n\
                     \n      _fl_rdp = (uint8_t)((_fl_rdp + 1) & {cnt_mask:#x}); _fl_cnt--; _ctrl_{on}_busy = 1;\n\
                     \n    }} else if (_ctrl_{on}_busy) {{\n\
                     \n      _next_mem[_ctrl_{on}_resp_handle] = _head_r;\n\
                     \n      {doubly_insert_head}\
                     \n      _head_r = _ctrl_{on}_resp_handle;\n\
                     \n      if (_ctrl_{on}_was_empty) _tail_r = _ctrl_{on}_resp_handle;\n\
                     \n      _ctrl_{on}_resp_v = 1; _ctrl_{on}_busy = 0;\n    }}\n",
                    doubly_insert_head = if has_doubly {
                        format!("_prev_mem[_head_r] = _ctrl_{on}_resp_handle;\n      ")
                    } else { String::new() }
                )),
                "insert_after" => cpp.push_str(&format!(
                    "    if (!_ctrl_{on}_busy && {on}_req_valid && _fl_cnt != 0) {{\n\
                     \n      uint8_t _slot = _fl_mem[_fl_rdp & {handle_mask:#x}];\n\
                     \n      _ctrl_{on}_resp_handle = _slot; _data_mem[_slot] = {on}_req_data;\n\
                     \n      _ctrl_{on}_after_handle = {on}_req_handle;\n\
                     \n      _next_mem[_slot] = _next_mem[{on}_req_handle];\n\
                     \n      _fl_rdp = (uint8_t)((_fl_rdp + 1) & {cnt_mask:#x}); _fl_cnt--; _ctrl_{on}_busy = 1;\n\
                     \n    }} else if (_ctrl_{on}_busy) {{\n\
                     \n      uint8_t _after = _ctrl_{on}_after_handle;\n\
                     \n      _next_mem[_after] = _ctrl_{on}_resp_handle;\n\
                     \n      {doubly_insert_after}\
                     \n      _ctrl_{on}_resp_v = 1; _ctrl_{on}_busy = 0;\n    }}\n",
                    doubly_insert_after = if has_doubly {
                        format!(
                            "_prev_mem[_ctrl_{on}_resp_handle] = _after;\n\
                             \n      _prev_mem[_next_mem[_ctrl_{on}_resp_handle]] = _ctrl_{on}_resp_handle;\n      "
                        )
                    } else { String::new() }
                )),
                "delete_head" => cpp.push_str(&format!(
                    "    if (!_ctrl_{on}_busy && {on}_req_valid && _fl_cnt != {depth}) {{\n\
                     \n      _ctrl_{on}_resp_data = _data_mem[_head_r]; _ctrl_{on}_slot = _head_r; _ctrl_{on}_busy = 1;\n\
                     \n    }} else if (_ctrl_{on}_busy) {{\n\
                     \n      _fl_mem[_fl_wrp & {handle_mask:#x}] = _ctrl_{on}_slot;\n\
                     \n      _fl_wrp = (uint8_t)((_fl_wrp + 1) & {cnt_mask:#x}); _fl_cnt++;\n\
                     \n      _head_r = _next_mem[_ctrl_{on}_slot];\n\
                     \n      _ctrl_{on}_resp_v = 1; _ctrl_{on}_busy = 0;\n    }}\n"
                )),
                "read_data" => cpp.push_str(&format!(
                    "    if ({on}_req_valid) {{\n\
                     \n      _ctrl_{on}_resp_data = _data_mem[{on}_req_handle]; _ctrl_{on}_resp_v = 1;\n    }}\n"
                )),
                "write_data" => cpp.push_str(&format!(
                    "    if ({on}_req_valid) {{\n\
                     \n      _data_mem[{on}_req_handle] = {on}_req_data; _ctrl_{on}_resp_v = 1;\n    }}\n"
                )),
                "next" => cpp.push_str(&format!(
                    "    if ({on}_req_valid) {{\n\
                     \n      _ctrl_{on}_resp_handle = _next_mem[{on}_req_handle]; _ctrl_{on}_resp_v = 1;\n    }}\n"
                )),
                "prev" if has_doubly => cpp.push_str(&format!(
                    "    if ({on}_req_valid) {{\n\
                     \n      _ctrl_{on}_resp_handle = _prev_mem[{on}_req_handle]; _ctrl_{on}_resp_v = 1;\n    }}\n"
                )),
                _ => {}
            }
        }
        cpp.push_str("  }\n}\n");

        let extra_sigs: Vec<(&str, &str, u32)> = vec![];
        add_trace_to_simple_construct(&mut h, &mut cpp, &class, name, &l.ports, &extra_sigs);
        h.push_str("};\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }
}
