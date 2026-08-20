//! `synchronizer` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

impl<'a> SimCodegen<'a> {
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
}
