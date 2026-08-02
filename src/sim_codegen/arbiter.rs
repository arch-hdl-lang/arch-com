//! `arbiter` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

impl<'a> SimCodegen<'a> {
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
        if self.debug {
            h.push_str(
                "#pragma once\n#include <cstdint>\n#include <cstring>\n#include <cstdio>\n#include \"verilated.h\"\n\n",
            );
        } else {
            h.push_str(
                "#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n\n",
            );
        }
        h.push_str(&format!("class {class} {{\npublic:\n"));
        for p in &a.ports {
            let ty = cpp_port_type_with_params(&p.ty, &a.params);
            h.push_str(&format!("  {ty} {};\n", p.name.name));
        }
        for pa in &a.port_arrays {
            h.push_str(&format!("  uint64_t {}_valid;\n", pa.name.name));
            h.push_str(&format!("  uint64_t {}_ready;\n", pa.name.name));
            if a.lock_hold {
                h.push_str(&format!("  uint64_t {}_release;\n", pa.name.name));
            }
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
            if a.lock_hold {
                all_port_inits.push(format!("{}_release(0)", pa.name.name));
            }
        }
        all_port_inits.push("_clk_prev(0)".to_string());
        if a.lock_hold {
            all_port_inits.push("_hold_valid(0)".to_string());
            all_port_inits.push("_hold_owner(0)".to_string());
        }
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
        if self.debug {
            h.push_str("  void _debug_log_ports();\n");
        }
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        if a.lock_hold {
            // Owner latch mirroring the SV emitter's hold_valid_r /
            // hold_owner_r: a lock holder keeps the grant while its
            // request stays asserted.
            h.push_str("  uint8_t _hold_valid;\n");
            h.push_str("  uint8_t _hold_owner;\n");
        }
        if needs_rr_state {
            h.push_str("  uint8_t _last_grant;\n");
        }
        if self.debug {
            let debug_ports = collect_simple_debug_ports(&a.ports, &a.params);
            emit_simple_debug_header(&mut h, &debug_ports);
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
        if self.debug {
            cpp.push_str("  _debug_log_ports();\n");
        }
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        // eval_posedge() — self-gated so parents can call it unconditionally.
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (!_rising) return;\n");
        if a.lock_hold {
            // Mirrors the SV hold ff: release pulse from the owner's last
            // lock-body state clears the hold even when the owner's
            // request stays asserted (back-to-back re-lock).
            cpp.push_str(&format!(
                "  if ({rst_cond}) {{\n    _hold_valid = 0;\n    _hold_owner = 0;\n  }} else {{\n"
            ));
            cpp.push_str(&format!(
                "    _hold_valid = grant_valid && !(({req_pa_name}_release >> grant_requester) & 1);\n"
            ));
            cpp.push_str("    if (grant_valid) _hold_owner = grant_requester;\n");
            cpp.push_str("  }\n");
        }
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
        if a.lock_hold {
            // Current owner keeps the grant while its request stays
            // asserted and no registered release event is active. The
            // policy scan below then re-arbitrates immediately after the
            // release edge, matching the SV arbiter without a bubble.
            cpp.push_str(&format!(
                "  if (_hold_valid && (({req_pa_name}_valid >> _hold_owner) & 1) && !(({req_pa_name}_release >> _hold_owner) & 1)) {{\n"
            ));
            cpp.push_str("    grant_valid = 1;\n    grant_requester = _hold_owner;\n  }\n");
        }
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
        cpp.push_str(&format!(
            "    if (!grant_valid && (({req_pa_name}_valid >> _idx) & 1)) {{\n"
        ));
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
        cpp.push_str("}\n\n");
        if self.debug {
            let debug_ports = collect_simple_debug_ports(&a.ports, &a.params);
            emit_simple_debug_impl(&mut cpp, &class, name, &debug_ports, Some(clk_port));
        }

        SimModel {
            class_name: class,
            header: h,
            impl_: cpp,
        }
    }
}
