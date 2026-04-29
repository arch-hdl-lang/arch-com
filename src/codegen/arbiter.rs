//! `emit_arbiter` SV emitter (with policy-specific helpers) — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_arbiter(&mut self, a: &crate::ast::ArbiterDecl) {
        use crate::ast::ArbiterPolicy;

        let n = &a.name.name.clone();

        // Find NUM_REQ param
        let num_req_default = a.params.iter()
            .find(|p| p.name.name == "NUM_REQ")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "4".to_string());

        // Parse NUM_REQ as integer for bit width calculations
        let num_req_int: u64 = num_req_default.parse().unwrap_or(4);
        let req_width = crate::width::index_width(num_req_int as u64);

        let clk = a.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&a.ports);

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        if a.params.is_empty() {
            self.line("parameter int NUM_REQ = 4");
        } else {
            for (i, p) in a.params.iter().enumerate() {
                let comma = if i < a.params.len() - 1 { "," } else { "" };
                let default_str = p.default.as_ref()
                    .map(|e| format!(" = {}", self.emit_expr_str(e)))
                    .unwrap_or_default();
                self.line(&format!("parameter int {}{default_str}{comma}", p.name.name));
            }
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Scalar ports (clk, rst)
        let mut all_ports: Vec<String> = Vec::new();
        for p in &a.ports {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        // Port arrays
        for pa in &a.port_arrays {
            let count_str = self.emit_expr_str(&pa.count_expr);
            for s in &pa.signals {
                let dir = match s.direction { Direction::In => "input", Direction::Out => "output" };
                let port_name = format!("{}_{}", pa.name.name, s.name.name);
                let port_str = self.emit_port_array_signal_str(dir, &s.ty, &port_name, &count_str);
                all_ports.push(port_str);
            }
        }
        let port_count = all_ports.len();
        for (i, p) in all_ports.iter().enumerate() {
            let comma = if i < port_count - 1 { "," } else { "" };
            self.line(&format!("{p}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // Emit hook function inside arbiter module if custom policy
        if let ArbiterPolicy::Custom(ref fn_ident) = a.policy {
            let fns = std::mem::take(&mut self.pending_functions);
            for f in &fns {
                if f.name.name == fn_ident.name {
                    self.emit_function(f);
                }
            }
            self.pending_functions = fns;
        }

        // ── Detect request/grant signal names from port arrays ─────────────────
        // The first port array is assumed to be the request ports.
        // Input signal in it → req_valid_sig; output signal → req_ready_sig
        let (req_valid_sig, req_ready_sig) = if let Some(pa) = a.port_arrays.first() {
            let pfx = &pa.name.name;
            let valid = pa.signals.iter()
                .find(|s| s.direction == Direction::In)
                .map(|s| format!("{pfx}_{}", s.name.name))
                .unwrap_or_else(|| format!("{pfx}_valid"));
            let ready = pa.signals.iter()
                .find(|s| s.direction == Direction::Out)
                .map(|s| format!("{pfx}_{}", s.name.name))
                .unwrap_or_else(|| format!("{pfx}_ready"));
            (valid, ready)
        } else {
            ("request_valid".to_string(), "request_ready".to_string())
        };

        let latency = a.latency;
        let policy = a.policy.clone();

        // When latency > 1, grant logic targets intermediate _comb signals
        // which are then pipelined to the actual output ports.
        let (gv_sig, gr_sig, rr_sig) = if latency > 1 {
            let num_req_str = self.emit_expr_str(
                &a.params.iter().find(|p| p.name.name == "NUM_REQ")
                    .and_then(|p| p.default.clone())
                    .unwrap_or(crate::ast::Expr {
                        kind: crate::ast::ExprKind::Literal(crate::ast::LitKind::Dec(num_req_int)),
                        span: a.span,
                        parenthesized: false,
                    })
            );
            self.line(&format!("logic grant_valid_comb;"));
            self.line(&format!("logic [{}:0] grant_requester_comb;", req_width - 1));
            self.line(&format!("logic [{}] {req_ready_sig}_comb;", Self::fold_width_str(&num_req_str)));
            self.line("");
            ("grant_valid_comb".to_string(), "grant_requester_comb".to_string(), format!("{req_ready_sig}_comb"))
        } else {
            ("grant_valid".to_string(), "grant_requester".to_string(), req_ready_sig.clone())
        };

        // ── Arbiter logic ─────────────────────────────────────────────────────
        match policy {
            ArbiterPolicy::RoundRobin => {
                self.emit_arbiter_round_robin(&clk, &rst, is_async, is_low, req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Priority => {
                self.emit_arbiter_priority(req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Lru => {
                self.emit_arbiter_round_robin(&clk, &rst, is_async, is_low, req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Weighted(_) => {
                self.emit_arbiter_priority(req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
            ArbiterPolicy::Custom(ref fn_ident) => {
                self.emit_arbiter_custom(a, fn_ident, &clk, &rst, is_async, is_low, req_width, num_req_int, &req_valid_sig, &rr_sig, &gv_sig, &gr_sig);
            }
        }

        // ── Pipeline registers for latency > 1 ───────────────────────────────
        if latency > 1 {
            let stages = latency - 1;
            let ff_sens = Self::ff_sensitivity(&clk, &rst, is_async, is_low);
            let rst_cond = Self::rst_condition(&rst, is_low);
            let num_req_str = &num_req_default;
            self.line("");

            for s in 0..stages {
                let src_gv = if s == 0 { gv_sig.clone() } else { format!("grant_valid_p{s}") };
                let src_gr = if s == 0 { gr_sig.clone() } else { format!("grant_requester_p{s}") };
                let src_rr = if s == 0 { rr_sig.clone() } else { format!("{req_ready_sig}_p{s}") };
                let dst_suffix = if s == stages - 1 {
                    // Last stage drives the output ports directly
                    String::new()
                } else {
                    format!("_p{}", s + 1)
                };

                let dst_gv = if dst_suffix.is_empty() { "grant_valid".to_string() } else { format!("grant_valid{dst_suffix}") };
                let dst_gr = if dst_suffix.is_empty() { "grant_requester".to_string() } else { format!("grant_requester{dst_suffix}") };
                let dst_rr = if dst_suffix.is_empty() { req_ready_sig.clone() } else { format!("{req_ready_sig}{dst_suffix}") };

                // Declare intermediate regs (not needed for last stage which drives output ports)
                if !dst_suffix.is_empty() {
                    self.line(&format!("logic {dst_gv};"));
                    self.line(&format!("logic [{}:0] {dst_gr};", req_width - 1));
                    self.line(&format!("logic [{}] {dst_rr};", Self::fold_width_str(&num_req_str)));
                }

                self.line(&format!("always_ff @({ff_sens}) begin"));
                self.indent += 1;
                self.line(&format!("if ({rst_cond}) begin"));
                self.indent += 1;
                self.line(&format!("{dst_gv} <= 1'b0;"));
                self.line(&format!("{dst_gr} <= '0;"));
                self.line(&format!("{dst_rr} <= '0;"));
                self.indent -= 1;
                self.line("end else begin");
                self.indent += 1;
                self.line(&format!("{dst_gv} <= {src_gv};"));
                self.line(&format!("{dst_gr} <= {src_gr};"));
                self.line(&format!("{dst_rr} <= {src_rr};"));
                self.indent -= 1;
                self.line("end");
                self.indent -= 1;
                self.line("end");
            }
        }

        if !a.asserts.is_empty() {
            let clk = a.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = a.asserts.clone();
            let aname = a.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &aname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Emit `dir type name` or `dir type [N-1:0] name` / `dir type name [0:N-1]`
    /// depending on type and whether count is 1.
    fn emit_port_array_signal_str(
        &self,
        dir: &str,
        ty: &TypeExpr,
        name: &str,
        count_str: &str,
    ) -> String {
        let is_scalar = count_str == "1";
        match ty {
            TypeExpr::Bool => {
                if is_scalar {
                    format!("{dir} logic {name}")
                } else {
                    format!("{dir} logic [{}] {name}", Self::fold_width_str(&count_str))
                }
            }
            _ => {
                let base = self.emit_port_type_str(ty);
                if is_scalar {
                    format!("{dir} {base} {name}")
                } else {
                    format!("{dir} {base} {name} [0:{count_str}-1]")
                }
            }
        }
    }


    fn emit_arbiter_round_robin(
        &mut self,
        clk: &str,
        rst: &str,
        is_async: bool,
        is_low: bool,
        req_width: u32,
        num_req: u64,
        req_valid: &str,
        req_ready: &str,
        grant_valid_sig: &str,
        grant_requester_sig: &str,
    ) {
        self.line(&format!("logic [{}:0] rr_ptr_r;", req_width - 1));
        self.line("integer arb_i;");
        self.line("logic arb_found;");
        self.line("");

        let ff_sens = Self::ff_sensitivity(clk, rst, is_async, is_low);
        let rst_cond = Self::rst_condition(rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) rr_ptr_r <= '0;"));
        self.line(&format!("else if ({grant_valid_sig}) rr_ptr_r <= rr_ptr_r + 1;"));
        self.indent -= 1;
        self.line("end");
        self.line("");
        // Use a shared integer index to avoid width-expansion warnings
        self.line("always_comb begin");
        self.indent += 1;
        self.line(&format!("{grant_valid_sig} = 1'b0;"));
        self.line(&format!("{req_ready} = '0;"));
        self.line(&format!("{grant_requester_sig} = '0;"));
        self.line("arb_found = 1'b0;");
        self.line(&format!("for (arb_i = 0; arb_i < {num_req}; arb_i++) begin"));
        self.indent += 1;
        // All index arithmetic in integer domain; only cast at use sites
        self.line(&format!(
            "if (!arb_found && {req_valid}[(int'(rr_ptr_r) + arb_i) % {num_req}]) begin"
        ));
        self.indent += 1;
        self.line("arb_found = 1'b1;");
        self.line(&format!("{grant_valid_sig} = 1'b1;"));
        self.line(&format!(
            "{grant_requester_sig} = {req_width}'((int'(rr_ptr_r) + arb_i) % {num_req});"
        ));
        self.line(&format!(
            "{req_ready}[(int'(rr_ptr_r) + arb_i) % {num_req}] = 1'b1;"
        ));
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_arbiter_priority(&mut self, req_width: u32, num_req: u64, req_valid: &str, req_ready: &str, grant_valid_sig: &str, grant_requester_sig: &str) {
        self.line("always_comb begin");
        self.indent += 1;
        self.line(&format!("{grant_valid_sig} = 1'b0;"));
        self.line(&format!("{req_ready} = '0;"));
        self.line(&format!("{grant_requester_sig} = '0;"));
        self.line(&format!("for (int pri_i = 0; pri_i < {num_req}; pri_i++) begin"));
        self.indent += 1;
        self.line(&format!("if (!{grant_valid_sig} && {req_valid}[pri_i]) begin"));
        self.indent += 1;
        self.line(&format!("{grant_valid_sig} = 1'b1;"));
        self.line(&format!("{grant_requester_sig} = {req_width}'(pri_i);"));
        self.line(&format!("{req_ready}[pri_i] = 1'b1;"));
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_arbiter_custom(
        &mut self,
        a: &crate::ast::ArbiterDecl,
        fn_ident: &crate::ast::Ident,
        clk: &str,
        rst: &str,
        is_async: bool,
        is_low: bool,
        req_width: u32,
        num_req: u64,
        req_valid: &str,
        req_ready: &str,
        grant_valid_sig: &str,
        grant_requester_sig: &str,
    ) {
        let fn_name = &fn_ident.name;

        // last_grant_r register for fairness state
        self.line(&format!("logic [{}:0] last_grant_r;", num_req - 1));
        self.line("");

        let ff_sens = Self::ff_sensitivity(clk, rst, is_async, is_low);
        let rst_cond = Self::rst_condition(rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) last_grant_r <= '0;"));
        self.line(&format!("else if ({grant_valid_sig}) last_grant_r <= grant_onehot;"));
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Build function call arguments from hook bindings
        let hook = a.hook.as_ref().unwrap();
        let args: Vec<String> = hook.fn_args.iter().map(|arg| {
            let name = &arg.name;
            // Map hook formal param names to actual SV signals
            let hook_param = hook.params.iter().find(|p| p.name.name == *name);
            if hook_param.is_some() {
                // This is a hook formal param — map known names to SV signals
                match name.as_str() {
                    "req_mask" => req_valid.to_string(),
                    "last_grant" => "last_grant_r".to_string(),
                    _ => name.clone(),
                }
            } else {
                // Must be a port or param name on the arbiter — use as-is
                name.clone()
            }
        }).collect();
        let args_str = args.join(", ");

        // Call the function to get one-hot grant mask
        self.line(&format!("logic [{}:0] grant_onehot;", num_req - 1));
        self.line("");
        self.line("always_comb begin");
        self.indent += 1;
        self.line(&format!("grant_onehot = {fn_name}({args_str});"));
        self.line(&format!("{grant_valid_sig} = |grant_onehot;"));
        self.line(&format!("{req_ready} = grant_onehot;"));
        // Priority encode one-hot to index
        self.line(&format!("{grant_requester_sig} = '0;"));
        self.line(&format!("for (int ci = 0; ci < {num_req}; ci++) begin"));
        self.indent += 1;
        self.line(&format!("if (grant_onehot[ci]) {grant_requester_sig} = {req_width}'(ci);"));
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }
}
