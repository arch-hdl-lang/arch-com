//! `regfile` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

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
}
