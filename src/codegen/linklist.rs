//! `emit_linklist` SV emitter (with op-controller helpers) — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_linklist(&mut self, l: &crate::ast::LinklistDecl) {
        use crate::ast::LinklistKind;
        let n = &l.name.name;
        let is_doubly = matches!(l.kind, LinklistKind::Doubly | LinklistKind::CircularDoubly);
        let is_circular = matches!(l.kind, LinklistKind::CircularSingly | LinklistKind::CircularDoubly);

        // Multi-head linklist support. NUM_HEADS defaults to 1; when set
        // to N > 1, the head / tail / length registers become arrays
        // indexed by a per-op `req_head_idx` port (validated at typecheck).
        // The node pool and free list stay shared across all heads.
        let num_heads = crate::typecheck::linklist_num_heads(l);
        let multi_head = num_heads > 1;
        let num_heads_expr = l.params.iter()
            .find(|p| p.name.name == "NUM_HEADS")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "1".to_string());

        // Resolve DEPTH default expression and DATA SV type
        let depth_expr = l.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "16".to_string());

        let data_default_sv = l.params.iter()
            .find(|p| p.name.name == "DATA")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => Some(self.emit_port_type_str(ty)),
                _ => None,
            })
            .unwrap_or_else(|| "logic [7:0]".to_string());

        // Operations that touch a specific head (need `req_head_idx` when
        // multi-head). Shared-pool ops (alloc, free) and slot-addressed
        // ops (read_data, write_data, next, prev) don't.
        let head_addressed_op = |name: &str| matches!(
            name,
            "insert_head" | "insert_tail" | "insert_after" | "delete_head" | "delete"
        );

        // Find clk/rst port names
        let clk_name = l.ports.iter()
            .find(|p| matches!(&p.ty, crate::ast::TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");
        let rst_name = l.ports.iter()
            .find(|p| matches!(&p.ty, crate::ast::TypeExpr::Reset(_, _)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("rst");

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        if multi_head {
            self.line(&format!("parameter int  NUM_HEADS = {num_heads_expr},"));
        }
        self.line(&format!("parameter int  DEPTH = {depth_expr},"));
        self.line(&format!("parameter type DATA  = {data_default_sv}"));
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // clk / rst ports
        self.line(&format!("input  logic {clk_name},"));
        self.line(&format!("input  logic {rst_name},"));

        // Op ports — one group per declared op
        let all_ops = &l.ops;
        let status_ports: Vec<&crate::ast::PortDecl> = l.ports.iter()
            .filter(|p| !matches!(&p.ty, crate::ast::TypeExpr::Clock(_) | crate::ast::TypeExpr::Reset(_, _)))
            .collect();

        // Collect all port lines then emit with trailing comma logic
        let mut port_lines: Vec<String> = Vec::new();
        for op in all_ops {
            for p in &op.ports {
                let dir = match p.direction { Direction::In => "input ", Direction::Out => "output" };
                let ty_str = self.emit_ll_port_type(&p.ty);
                port_lines.push(format!("{dir} {ty_str} {}_{}", op.name.name, p.name.name));
            }
        }
        for p in &status_ports {
            let dir = match p.direction { Direction::In => "input ", Direction::Out => "output" };
            let ty_str = self.emit_ll_port_type(&p.ty);
            port_lines.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        for (i, line) in port_lines.iter().enumerate() {
            let comma = if i < port_lines.len() - 1 { "," } else { "" };
            self.line(&format!("{line}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // ── Internal constants ────────────────────────────────────────────────
        self.line("localparam int HANDLE_W = $clog2(DEPTH);");
        self.line("localparam int CNT_W    = $clog2(DEPTH + 1);");
        if multi_head {
            self.line("localparam int HEAD_IDX_W = $clog2(NUM_HEADS);");
        }
        self.line("");

        // ── Free list: circular FIFO of slot indices ──────────────────────────
        self.line("// Free list — circular FIFO of available slot indices");
        self.line("logic [HANDLE_W-1:0] _fl_mem  [0:DEPTH-1];");
        self.line("logic [CNT_W-1:0]    _fl_rdp;");
        self.line("logic [CNT_W-1:0]    _fl_wrp;");
        self.line("logic [CNT_W-1:0]    _fl_cnt;");
        self.line("");

        // ── Payload and link RAMs ─────────────────────────────────────────────
        self.line("// Payload and link RAMs");
        self.line("DATA                 _data_mem [0:DEPTH-1];");
        self.line("logic [HANDLE_W-1:0] _next_mem [0:DEPTH-1];");
        if is_doubly {
            self.line("logic [HANDLE_W-1:0] _prev_mem [0:DEPTH-1];");
        }
        self.line("");

        // ── Head / tail / length registers ───────────────────────────────────
        self.line("// Head / tail registers");
        if multi_head {
            self.line("logic [HANDLE_W-1:0] _head_r [NUM_HEADS];");
            if l.track_tail {
                self.line("logic [HANDLE_W-1:0] _tail_r [NUM_HEADS];");
            }
            // Internal per-head occupancy counter — used for "this head
            // is empty" detection (insert vs. append branch, delete
            // req_ready gating). Always emitted in multi-head mode;
            // not user-visible.
            self.line("logic [CNT_W-1:0]    _length_r [NUM_HEADS];");
        } else {
            self.line("logic [HANDLE_W-1:0] _head_r;");
            if l.track_tail {
                self.line("logic [HANDLE_W-1:0] _tail_r;");
            }
        }
        self.line("");

        // ── Per-op controller registers ───────────────────────────────────────
        for op in all_ops {
            let on = &op.name.name;
            // Every op gets a busy flag (for latency > 1) and resp_valid pipeline
            self.line(&format!("// {on} controller registers"));
            if op.latency > 1 {
                self.line(&format!("logic _ctrl_{on}_busy;"));
            }
            // resp_valid output register
            let has_resp_valid = op.ports.iter().any(|p| p.name.name == "resp_valid");
            if has_resp_valid {
                self.line(&format!("logic _ctrl_{on}_resp_v;"));
            }
            // latch any output data ports
            for p in op.ports.iter().filter(|p| p.direction == Direction::Out && p.name.name != "req_ready" && p.name.name != "resp_valid") {
                let ty = self.emit_ll_port_type(&p.ty);
                self.line(&format!("{ty} _ctrl_{on}_{};", p.name.name));
            }
            // Op-specific internal temporaries
            match on.as_str() {
                "delete_head" | "delete" => {
                    self.line(&format!("logic [HANDLE_W-1:0] _ctrl_{on}_slot;"));
                }
                "insert_tail" | "insert_head" => {
                    self.line(&format!("logic _ctrl_{on}_was_empty;"));
                }
                "insert_after" => {
                    self.line(&format!("logic [HANDLE_W-1:0] _ctrl_{on}_after_handle;"));
                }
                _ => {}
            }
            // Multi-head: latch the requested head idx at accept cycle so
            // the busy cycle can reuse it without re-reading the live port.
            if multi_head && head_addressed_op(on) && op.latency > 1 {
                self.line(&format!("logic [HEAD_IDX_W-1:0] _ctrl_{on}_head_idx;"));
            }
            self.line("");
        }

        // ── Status assigns ────────────────────────────────────────────────────
        self.line("// Status outputs");
        // empty: free list count == DEPTH (all slots available = list is empty)
        if status_ports.iter().any(|p| p.name.name == "empty") {
            self.line("assign empty  = (_fl_cnt == CNT_W'(DEPTH));");
        }
        // full: free list count == 0 (no slots available = list is full)
        if status_ports.iter().any(|p| p.name.name == "full") {
            self.line("assign full   = (_fl_cnt == '0);");
        }
        // length: occupied slots = DEPTH - free count
        if status_ports.iter().any(|p| p.name.name == "length") {
            self.line("assign length = CNT_W'(DEPTH) - _fl_cnt;");
        }

        // req_ready assigns (combinational: not busy and not full/empty as applicable)
        self.line("");
        self.line("// req_ready: combinational");
        for op in all_ops {
            let on = &op.name.name;
            let is_head_addr = head_addressed_op(on);
            if op.ports.iter().any(|p| p.name.name == "req_ready") {
                let guard = if op.latency > 1 {
                    format!("!_ctrl_{on}_busy && ")
                } else {
                    String::new()
                };
                // Multi-head delete: gate on "this head has entries"
                // rather than "pool has any entries". Insert ops still
                // gate on the shared free list (full pool = stall).
                let cond = match on.as_str() {
                    "alloc" | "insert_head" | "insert_tail" | "insert_after" => {
                        format!("{guard}!(_fl_cnt == '0)")
                    }
                    "free" => {
                        format!("{guard}!(_fl_cnt == CNT_W'(DEPTH))")
                    }
                    "delete_head" | "delete" if multi_head && is_head_addr => {
                        format!("{guard}(_length_r[{on}_req_head_idx] != '0)")
                    }
                    "delete_head" | "delete" => {
                        format!("{guard}!(_fl_cnt == CNT_W'(DEPTH))")
                    }
                    _ => format!("{guard}1'b1"),
                };
                self.line(&format!("assign {on}_req_ready = {cond};"));
            }
            // wire resp_valid output from register
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                self.line(&format!("assign {on}_resp_valid = _ctrl_{on}_resp_v;"));
            }
            // wire other output data ports
            for p in op.ports.iter().filter(|p| p.direction == Direction::Out && p.name.name != "req_ready" && p.name.name != "resp_valid") {
                self.line(&format!("assign {}_{} = _ctrl_{on}_{};", on, p.name.name, p.name.name));
            }
        }
        self.line("");

        // ── Reset + free-list init + op controllers ───────────────────────────
        self.line(&format!("integer _ll_i;"));
        self.line(&format!("always_ff @(posedge {clk_name}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_name}) begin"));
        self.indent += 1;
        self.line("for (_ll_i = 0; _ll_i < DEPTH; _ll_i++)");
        self.indent += 1;
        self.line("_fl_mem[_ll_i] <= HANDLE_W'(_ll_i);");
        self.indent -= 1;
        self.line("_fl_rdp <= '0;");
        self.line("_fl_wrp <= '0;");
        self.line("_fl_cnt <= CNT_W'(DEPTH);");
        if multi_head {
            self.line("for (_ll_i = 0; _ll_i < NUM_HEADS; _ll_i++) begin");
            self.indent += 1;
            self.line("_head_r[_ll_i] <= '0;");
            if l.track_tail { self.line("_tail_r[_ll_i] <= '0;"); }
            self.line("_length_r[_ll_i] <= '0;");
            self.indent -= 1;
            self.line("end");
        } else {
            self.line("_head_r <= '0;");
            if l.track_tail { self.line("_tail_r <= '0;"); }
        }
        for op in all_ops {
            let on = &op.name.name;
            if op.latency > 1 { self.line(&format!("_ctrl_{on}_busy <= 1'b0;")); }
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                self.line(&format!("_ctrl_{on}_resp_v <= 1'b0;"));
            }
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;

        // Clear resp_valid by default each cycle (pulse behaviour)
        for op in all_ops {
            if op.ports.iter().any(|p| p.name.name == "resp_valid") {
                self.line(&format!("_ctrl_{}_resp_v <= 1'b0;", op.name.name));
            }
        }
        self.line("");

        // Per-op logic
        for op in all_ops {
            self.emit_ll_op_controller(op, l.track_tail, is_doubly, is_circular, num_heads);
        }

        self.indent -= 1;
        self.line("end"); // else
        self.indent -= 1;
        self.line("end"); // always_ff
        self.line("");

        if !l.asserts.is_empty() {
            let clk = l.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = l.asserts.clone();
            let lname = l.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &lname, &clk);
        }

        self.indent -= 1;
        self.line("endmodule");
        self.line("");
    }

    /// Emit SV type string for a linklist port — DATA named type → "DATA".
    fn emit_ll_port_type(&self, ty: &crate::ast::TypeExpr) -> String {
        match ty {
            crate::ast::TypeExpr::Named(id) if id.name == "DATA" => "DATA".to_string(),
            crate::ast::TypeExpr::Bool => "logic".to_string(),
            other => self.emit_port_type_str(other),
        }
    }

    /// Emit the always_ff body for one declared op.
    fn emit_ll_op_controller(
        &mut self,
        op: &crate::ast::OpDecl,
        track_tail: bool,
        is_doubly: bool,
        _is_circular: bool,
        num_heads: u32,
    ) {
        let on = &op.name.name;
        let has_req_valid   = op.ports.iter().any(|p| p.name.name == "req_valid");
        let has_resp_valid  = op.ports.iter().any(|p| p.name.name == "resp_valid");
        let has_req_handle  = op.ports.iter().any(|p| p.name.name == "req_handle");
        let has_req_data    = op.ports.iter().any(|p| p.name.name == "req_data");
        let multi_head = num_heads > 1;
        let is_head_addr = matches!(
            on.as_str(),
            "insert_head" | "insert_tail" | "insert_after" | "delete_head" | "delete"
        );
        // Head-register access expressions.
        // - `_accept` variant is used in the accept cycle (latency==1 or
        //   the first branch of a latency>1 op). Reads the live
        //   `req_head_idx` port directly.
        // - `_busy` variant is used in subsequent busy cycles. Reads the
        //   latched `_ctrl_<op>_head_idx`.
        // For single-head lists both resolve to bare `_head_r` / `_tail_r`
        // so the emitted SV stays byte-identical with the pre-multi-head
        // compiler.
        let head_r_accept = if multi_head && is_head_addr {
            format!("_head_r[{on}_req_head_idx]")
        } else { "_head_r".to_string() };
        let head_r_busy = if multi_head && is_head_addr {
            format!("_head_r[_ctrl_{on}_head_idx]")
        } else { "_head_r".to_string() };
        // _tail_r is only read at the busy cycle (post-accept). The
        // accept-cycle variant would be `_tail_r[<op>_req_head_idx]`
        // if an op ever needed it.
        let tail_r_busy = if multi_head && is_head_addr {
            format!("_tail_r[_ctrl_{on}_head_idx]")
        } else { "_tail_r".to_string() };

        self.line(&format!("// ── {on} ─────────────────────────────────────────"));

        // Phase-B scope: multi-head supports insert_tail + delete_head
        // only. Other head-addressed ops need wiring the latched head_idx
        // into their branching / pointer-patch paths; deferred to a
        // follow-up phase.
        if multi_head && matches!(on.as_str(), "insert_head" | "insert_after" | "delete") {
            self.line(&format!(
                "// NOTE: op `{on}` is not yet supported for multi-head linklist (Phase B)."
            ));
            self.line(&format!(
                "initial $fatal(1, \"linklist: op `{on}` not yet implemented for multi-head (NUM_HEADS > 1)\");"
            ));
            return;
        }

        match on.as_str() {
            "alloc" => {
                // Latency-1: dequeue one slot from free list
                let guard = if has_req_valid { format!("{on}_req_valid && !(_fl_cnt == '0)") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                if has_resp_valid {
                    self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;"));
                    self.line(&format!("_ctrl_{on}_resp_handle <= _fl_mem[_fl_rdp[HANDLE_W-1:0]];"));
                }
                self.indent -= 1;
                self.line("end");
            }
            "free" => {
                // Latency-1: enqueue slot back onto free list
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_fl_mem[_fl_wrp[HANDLE_W-1:0]] <= {on}_req_handle;"));
                }
                self.line("_fl_wrp <= _fl_wrp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt + 1'b1;");
                self.indent -= 1;
                self.line("end");
            }
            "insert_head" => {
                // Latency-2: alloc slot, write data, update head
                if op.latency >= 2 {
                    let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && !(_fl_cnt == '0)");
                    self.line(&format!("if ({guard}) begin"));
                    self.indent += 1;
                    let slot = format!("_fl_mem[_fl_rdp[HANDLE_W-1:0]]");
                    self.line(&format!("_ctrl_{on}_resp_handle <= {slot};"));
                    if has_req_data {
                        self.line(&format!("_data_mem[{slot}] <= {on}_req_data;"));
                    }
                    self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                    self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                    self.line(&format!("_ctrl_{on}_was_empty <= (_fl_cnt == CNT_W'(DEPTH));"));
                    self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                    self.indent -= 1;
                    self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                    self.indent += 1;
                    self.line(&format!("_next_mem[_ctrl_{on}_resp_handle] <= _head_r;"));
                    if is_doubly {
                        // old head.prev = new node; new node.prev = sentinel (0)
                        self.line(&format!("_prev_mem[_head_r] <= _ctrl_{on}_resp_handle;"));
                    }
                    self.line(&format!("_head_r <= _ctrl_{on}_resp_handle;"));
                    if track_tail {
                        self.line(&format!("if (_ctrl_{on}_was_empty) _tail_r <= _ctrl_{on}_resp_handle;"));
                    }
                    if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                    self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                    self.indent -= 1;
                    self.line("end");
                } else {
                    // Latency-1 shortcut (caller's responsibility to allow 2-cycle settling)
                    let slot = "_fl_mem[_fl_rdp[HANDLE_W-1:0]]";
                    self.line(&format!("if ({on}_req_valid && !(_fl_cnt == '0)) begin"));
                    self.indent += 1;
                    if has_req_data { self.line(&format!("_data_mem[{slot}] <= {on}_req_data;")); }
                    self.line(&format!("_next_mem[{slot}] <= _head_r;"));
                    self.line(&format!("_head_r <= {slot};"));
                    self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                    self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                    if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                    self.indent -= 1;
                    self.line("end");
                }
            }
            "insert_tail" => {
                // Latency-2: alloc, write data, patch tail's next, update tail
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && !(_fl_cnt == '0)");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                let slot = "_fl_mem[_fl_rdp[HANDLE_W-1:0]]";
                self.line(&format!("_ctrl_{on}_resp_handle <= {slot};"));
                if has_req_data { self.line(&format!("_data_mem[{slot}] <= {on}_req_data;")); }
                self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                // Empty check: single-head uses pool occupancy; multi-head
                // uses the per-head length counter so chains from other
                // heads don't mask this head's emptiness.
                if multi_head {
                    self.line(&format!("_ctrl_{on}_was_empty <= (_length_r[{on}_req_head_idx] == '0);"));
                    self.line(&format!("_ctrl_{on}_head_idx  <= {on}_req_head_idx;"));
                } else {
                    self.line(&format!("_ctrl_{on}_was_empty <= (_fl_cnt == CNT_W'(DEPTH));"));
                }
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                if track_tail {
                    self.line(&format!("if (!_ctrl_{on}_was_empty) _next_mem[{tail_r_busy}] <= _ctrl_{on}_resp_handle;"));
                    if is_doubly {
                        // new node.prev = old tail
                        self.line(&format!("_prev_mem[_ctrl_{on}_resp_handle] <= {tail_r_busy};"));
                    }
                    self.line(&format!("{tail_r_busy} <= _ctrl_{on}_resp_handle;"));
                    self.line(&format!("if (_ctrl_{on}_was_empty) {head_r_busy} <= _ctrl_{on}_resp_handle;"));
                } else {
                    self.line(&format!("if (!_ctrl_{on}_was_empty) _next_mem[{head_r_busy}] <= _ctrl_{on}_resp_handle;"));
                    self.line(&format!("if (_ctrl_{on}_was_empty) {head_r_busy} <= _ctrl_{on}_resp_handle;"));
                }
                if multi_head {
                    self.line(&format!("_length_r[_ctrl_{on}_head_idx] <= _length_r[_ctrl_{on}_head_idx] + 1'b1;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            "delete_head" => {
                // Latency-2: read head data, advance head, free old head slot
                let pool_gate = if multi_head {
                    format!("(_length_r[{on}_req_head_idx] != '0)")
                } else {
                    "!(_fl_cnt == CNT_W'(DEPTH))".to_string()
                };
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && {pool_gate}");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                self.line(&format!("_ctrl_delete_head_resp_data <= _data_mem[{head_r_accept}];"));
                self.line(&format!("_ctrl_delete_head_slot      <= {head_r_accept};"));
                if multi_head {
                    self.line(&format!("_ctrl_{on}_head_idx          <= {on}_req_head_idx;"));
                }
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                // Free the old head slot
                self.line("_fl_mem[_fl_wrp[HANDLE_W-1:0]] <= _ctrl_delete_head_slot;");
                self.line("_fl_wrp <= _fl_wrp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt + 1'b1;");
                // Advance head
                self.line(&format!("{head_r_busy} <= _next_mem[_ctrl_delete_head_slot];"));
                if multi_head {
                    self.line(&format!("_length_r[_ctrl_{on}_head_idx] <= _length_r[_ctrl_{on}_head_idx] - 1'b1;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            "read_data" => {
                // Latency-1: RAM read (registered output)
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_resp_data <= _data_mem[{on}_req_handle];"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "write_data" => {
                // Latency-1: RAM write
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle && has_req_data {
                    self.line(&format!("_data_mem[{on}_req_handle] <= {on}_req_data;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "next" => {
                // Latency-1: follow next pointer
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_resp_handle <= _next_mem[{on}_req_handle];"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "prev" => {
                // Latency-1: follow prev pointer (doubly only)
                let guard = if has_req_valid { format!("{on}_req_valid") } else { "1'b1".into() };
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_resp_handle <= _prev_mem[{on}_req_handle];"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.indent -= 1;
                self.line("end");
            }
            "insert_after" => {
                // Latency-2: alloc, write data+next link; cycle 2 patches after.next (and prev ptrs)
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid && !(_fl_cnt == '0)");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                let slot = "_fl_mem[_fl_rdp[HANDLE_W-1:0]]";
                self.line(&format!("_ctrl_{on}_resp_handle <= {slot};"));
                if has_req_data { self.line(&format!("_data_mem[{slot}] <= {on}_req_data;")); }
                // Latch after_handle so cycle 2 doesn't read live port
                self.line(&format!("_ctrl_{on}_after_handle <= {on}_req_handle;"));
                // new.next = after.next (the successor)
                self.line(&format!("_next_mem[{slot}] <= _next_mem[{on}_req_handle];"));
                self.line("_fl_rdp <= _fl_rdp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt - 1'b1;");
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                // after.next = new
                self.line(&format!("_next_mem[_ctrl_{on}_after_handle] <= _ctrl_{on}_resp_handle;"));
                if is_doubly {
                    // new.prev = after
                    self.line(&format!("_prev_mem[_ctrl_{on}_resp_handle] <= _ctrl_{on}_after_handle;"));
                    // successor.prev = new  (new.next is already committed from cycle 1)
                    self.line(&format!("_prev_mem[_next_mem[_ctrl_{on}_resp_handle]] <= _ctrl_{on}_resp_handle;"));
                }
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            "delete" => {
                // Latency-2 (doubly): unlink by patching prev.next and next.prev
                let guard = format!("!_ctrl_{on}_busy && {on}_req_valid");
                self.line(&format!("if ({guard}) begin"));
                self.indent += 1;
                if has_req_handle {
                    self.line(&format!("_ctrl_{on}_slot <= {on}_req_handle;"));
                }
                self.line(&format!("_ctrl_{on}_busy <= 1'b1;"));
                self.indent -= 1;
                self.line(&format!("end else if (_ctrl_{on}_busy) begin"));
                self.indent += 1;
                self.line(&format!("_fl_mem[_fl_wrp[HANDLE_W-1:0]] <= _ctrl_{on}_slot;"));
                self.line("_fl_wrp <= _fl_wrp + 1'b1;");
                self.line("_fl_cnt <= _fl_cnt + 1'b1;");
                if has_resp_valid { self.line(&format!("_ctrl_{on}_resp_v <= 1'b1;")); }
                self.line(&format!("_ctrl_{on}_busy <= 1'b0;"));
                self.indent -= 1;
                self.line("end");
            }
            _ => {
                // Unknown op — emit a comment placeholder
                self.line(&format!("// op `{on}` — not implemented"));
            }
        }
        self.line("");
    }
}
