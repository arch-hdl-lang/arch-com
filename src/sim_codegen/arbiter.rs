//! `arbiter` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

impl<'a> SimCodegen<'a> {
    pub(crate) fn gen_arbiter(&self, a: &ArbiterDecl) -> SimModel {
        let name = &a.name.name;
        let class = format!("V{name}");

        // Requester count. The request port array's `[N]` shape is the
        // source of truth — it is what sizes the `_valid` / `_ready`
        // vectors — and it may be spelled with any param name. Resolving
        // that count expression against the arbiter's params first fixes
        // `examples/nic400/Nic400ArbiterPolicy.arch` (`param N: const = 4;`
        // + `ports[N] req`), which used to miss the `NUM_REQ`-only lookup
        // and land on the `unwrap_or(2)` default — a 4-requester arbiter
        // simulated with a 2-slot scan, so requesters 2 and 3 could never
        // be granted. The `NUM_REQ` param lookup stays as the fallback for
        // arbiters declared without a request port array. Byte-identical
        // for the common `param NUM_REQ` + `ports[NUM_REQ]` shape.
        let num_req: u64 = a
            .port_arrays
            .first()
            .and_then(|pa| try_eval_const_expr_with_params(&pa.count_expr, &a.params))
            .or_else(|| {
                a.params
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
            })
            .unwrap_or(2);

        // `policy <FnName>` + `hook grant_select(...) = FnName(...)`: the
        // grant mask comes from the user's function, not from any built-in
        // scan. Mirrors `emit_arbiter_custom` in codegen/arbiter.rs.
        // Pre-fix this arm did not exist and every custom-policy arbiter
        // silently simulated as a fixed lowest-index-wins priority encoder
        // while its SV called the hook (arch#912).
        let custom_fn: Option<&Ident> = match &a.policy {
            ArbiterPolicy::Custom(f) => Some(f),
            _ => None,
        };
        if custom_fn.is_some() && a.hook.is_none() {
            // `check_arbiter` rejects this for every non-interface arbiter,
            // and interface stubs never reach sim codegen — so this is a
            // compiler invariant break, not user error. Refuse loudly
            // rather than fall back to a scan that models a different
            // arbiter than the one the user wrote.
            eprintln!(
                "error: internal: arbiter `{name}` has custom policy `{}` but no \
                 `hook grant_select` binding — arch sim cannot model its grant policy",
                custom_fn.unwrap().name
            );
            std::process::exit(1);
        }

        let (rst_name, _is_async, is_low) = extract_reset_info(&a.ports);
        let rst_cond = if is_low {
            format!("(!{rst_name})")
        } else {
            rst_name.clone()
        };

        let mut h = String::new();
        if self.debug {
            h.push_str(
                "#pragma once\n#include <cstdint>\n#include <cstring>\n#include <cstdio>\n#include \"verilated.h\"\n",
            );
        } else {
            h.push_str(
                "#pragma once\n#include <cstdint>\n#include <cstring>\n#include \"verilated.h\"\n",
            );
        }
        if custom_fn.is_some() {
            // The bound policy function is hoisted into VFunctions.h by
            // `gen_functions` (top-level, package-level and module-internal
            // `function` items all land there); eval_comb() calls it there.
            h.push_str("#include \"VFunctions.h\"\n");
        }
        h.push('\n');
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
        if custom_fn.is_some() {
            // Mirrors the SV emitter's `last_grant_r` / `grant_onehot`
            // pair. Both are one-hot *masks* (not indices) — the hook
            // function's contract — and `last_grant_r` resets to '0, not
            // to N-1 like the round-robin pointer.
            all_port_inits.push("_last_grant_onehot(0)".to_string());
            all_port_inits.push("_grant_onehot(0)".to_string());
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
        if custom_fn.is_some() {
            // `_grant_onehot` is a member (not an eval_comb local) because
            // eval_posedge() samples it: SV's `last_grant_r <= grant_onehot`
            // captures the combinational mask standing just before the
            // edge, which is what the previous eval_comb() left here.
            h.push_str("  uint64_t _last_grant_onehot;\n");
            h.push_str("  uint64_t _grant_onehot;\n");
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
        if custom_fn.is_some() {
            // SV: `if (rst) last_grant_r <= '0; else if (grant_valid)
            //      last_grant_r <= grant_onehot;`
            cpp.push_str(&format!(
                "  if ({rst_cond}) {{\n    _last_grant_onehot = 0;\n  }} else {{\n"
            ));
            cpp.push_str("    if (grant_valid) _last_grant_onehot = _grant_onehot;\n");
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

        // eval_comb() — custom policy calls the bound hook function;
        //               priority scans from 0 (index 0 = highest priority);
        //               round_robin / lru rotate starting after the last grant.
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        cpp.push_str("  grant_valid = 0;\n  grant_requester = 0;\n");
        if let Some(fn_ident) = custom_fn {
            self.emit_arbiter_custom_comb(&mut cpp, a, fn_ident, num_req, req_pa_name);
        } else {
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
        }
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

    /// Body of `eval_comb()` for a `policy <FnName>` arbiter — the sim
    /// mirror of `Codegen::emit_arbiter_custom` (codegen/arbiter.rs).
    ///
    /// Both emitters must agree signal-for-signal:
    ///
    /// | SV                                  | sim                                  |
    /// |-------------------------------------|--------------------------------------|
    /// | `grant_onehot = Fn(args)`           | `_grant_onehot = Fn(args) & MASK`    |
    /// | `grant_valid  = \|grant_onehot`      | `grant_valid = (_grant_onehot != 0)` |
    /// | `req_ready    = grant_onehot`       | `<req>_ready = _grant_onehot`        |
    /// | index loop, last set bit wins       | same loop                            |
    ///
    /// The mask stands in for SV's implicit truncation when the call
    /// result is assigned to `logic [NUM_REQ-1:0] grant_onehot`; C++ only
    /// truncates to the next-larger integer type, so a hook returning
    /// stray high bits would otherwise light up `grant_valid` in sim but
    /// not in SV.
    fn emit_arbiter_custom_comb(
        &self,
        cpp: &mut String,
        a: &ArbiterDecl,
        fn_ident: &Ident,
        num_req: u64,
        req_pa_name: &str,
    ) {
        // `custom_fn.is_some() && hook.is_none()` already exited above.
        let hook = a.hook.as_ref().expect("custom policy without hook");
        let fn_name = &fn_ident.name;

        // The callee's declared arg types drive per-argument truncation.
        // SV narrows each actual to the function's port width at the call
        // boundary; C++ narrows only to the parameter's integer type
        // (`UInt<4>` → `uint8_t`, i.e. 8 bits, not 4). Prefer the function
        // declaration, fall back to the hook's declared signature.
        let fn_decl = find_function_decl(self.source, fn_name);
        let args: Vec<String> = hook
            .fn_args
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                // Only `req_mask` and `last_grant` name internal signals;
                // `check_arbiter` has verified everything else is a real
                // arbiter port or param (same contract the SV emitter
                // relies on).
                let (base, is_const) = match arg.name.as_str() {
                    "req_mask" => (format!("{req_pa_name}_valid"), false),
                    "last_grant" => ("_last_grant_onehot".to_string(), false),
                    other => match a
                        .params
                        .iter()
                        .find(|p| p.name.name == other)
                        .and_then(|p| eval_param_const_value(p, &a.params))
                    {
                        // A param is an SV `parameter` but has no C++
                        // symbol in the arbiter class — substitute its
                        // folded value. Ports are public members, so their
                        // bare name is already the right C++ expression.
                        Some(v) => (format!("{v}ULL"), true),
                        None => (other.to_string(), false),
                    },
                };
                if is_const {
                    return base;
                }
                let declared = fn_decl
                    .and_then(|f| f.args.get(i))
                    .map(|fa| &fa.ty)
                    .or_else(|| hook.params.get(i).map(|p| &p.ty));
                match declared.and_then(unsigned_arg_mask) {
                    Some(m) => format!("({base} & {m})"),
                    None => base,
                }
            })
            .collect();
        let args_str = args.join(", ");
        let mask = mask_literal(num_req);

        if a.lock_hold {
            // Current owner keeps the grant while its request stays
            // asserted; the hook only arbitrates when the resource is
            // free. Mirrors the SV `hold_valid_r` fast path.
            cpp.push_str(&format!(
                "  if (_hold_valid && (({req_pa_name}_valid >> _hold_owner) & 1) && !(({req_pa_name}_release >> _hold_owner) & 1)) {{\n"
            ));
            cpp.push_str("    _grant_onehot = 1ULL << _hold_owner;\n");
            cpp.push_str(&format!(
                "  }} else {{\n    _grant_onehot = (uint64_t){fn_name}({args_str}) & {mask};\n  }}\n"
            ));
        } else {
            cpp.push_str(&format!(
                "  _grant_onehot = (uint64_t){fn_name}({args_str}) & {mask};\n"
            ));
        }
        cpp.push_str("  grant_valid = (_grant_onehot != 0);\n");
        cpp.push_str(&format!("  {req_pa_name}_ready = _grant_onehot;\n"));
        // Index of the grantee. SV's loop has no early exit, so on a
        // non-one-hot return the highest set bit wins — reproduce that
        // rather than breaking on the lowest.
        cpp.push_str(&format!(
            "  for (int _ci = 0; _ci < (int){num_req}; _ci++) {{\n"
        ));
        cpp.push_str("    if ((_grant_onehot >> _ci) & 1) grant_requester = _ci;\n  }\n");
    }
}

/// All-ones literal for the low `bits` bits, saturating at 64 (the width
/// of the sim's request/grant vectors).
fn mask_literal(bits: u64) -> String {
    if bits >= 64 {
        "~0ULL".to_string()
    } else {
        format!("0x{:X}ULL", (1u64 << bits) - 1)
    }
}

/// Truncation mask for one hook argument, or `None` when masking would
/// corrupt the value (signed types) or is a no-op (>= 64 bits). Widths
/// are folded the same way `gen_functions` picks the C++ parameter type
/// (`&[]` scope), so the two stay consistent.
fn unsigned_arg_mask(ty: &TypeExpr) -> Option<String> {
    let bits = match ty {
        TypeExpr::Bool | TypeExpr::Bit => 1,
        TypeExpr::UInt(w) => eval_width_with_params(w, &[]),
        _ => return None,
    };
    if bits == 0 || bits >= 64 {
        None
    } else {
        Some(mask_literal(bits as u64))
    }
}

/// Locate a `function` declaration by name across every scope
/// `gen_functions` hoists into `VFunctions.h`: top-level items, package
/// bodies, and module bodies.
fn find_function_decl<'s>(src: &'s SourceFile, name: &str) -> Option<&'s FunctionDecl> {
    for item in &src.items {
        match item {
            Item::Function(f) if f.name.name == name => return Some(f),
            Item::Package(p) => {
                if let Some(f) = p.functions.iter().find(|f| f.name.name == name) {
                    return Some(f);
                }
            }
            Item::Module(m) => {
                for b in &m.body {
                    if let ModuleBodyItem::Function(f) = b {
                        if f.name.name == name {
                            return Some(f);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}
