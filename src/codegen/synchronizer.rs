//! `emit_synchronizer` SV emitter — extracted from `codegen/mod.rs` to
//! mirror the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(super) fn emit_synchronizer(&mut self, s: &SynchronizerDecl) {
        let n = &s.name.name;

        // Resolve STAGES (default 2)
        let stages = s.params.iter()
            .find(|p| p.name.name == "STAGES")
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v as usize) } else { None })
            .unwrap_or(2);

        // Find clock ports (first = source clock, second = destination clock)
        let clk_ports: Vec<&PortDecl> = s.ports.iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .collect();
        let src_clk = &clk_ports[0].name.name;
        let dst_clk = &clk_ports[1].name.name;

        // Find data ports
        let data_in_port = s.ports.iter().find(|p| p.name.name == "data_in").unwrap();
        let data_ty = self.emit_port_type_str(&data_in_port.ty);

        // Check for reset port
        let rst_port = s.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Reset(..)));

        // Module header — emit all declared params as SV parameters
        self.line(&format!("module {n} #("));
        self.indent += 1;
        let param_strs: Vec<String> = s.params.iter().map(|p| {
            let val = p.default.as_ref()
                .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(v.to_string()) } else { None })
                .unwrap_or_else(|| "0".to_string());
            format!("parameter int {} = {}", p.name.name, val)
        }).collect();
        // Always include STAGES if not already declared
        let has_stages = s.params.iter().any(|p| p.name.name == "STAGES");
        let mut all_param_strs = param_strs;
        if !has_stages {
            all_param_strs.push(format!("parameter int STAGES = {stages}"));
        }
        for (i, ps) in all_param_strs.iter().enumerate() {
            let comma = if i < all_param_strs.len() - 1 { "," } else { "" };
            self.line(&format!("{ps}{comma}"));
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;
        // Emit ports
        let port_strs: Vec<String> = s.ports.iter().map(|p| {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty = self.emit_port_type_str(&p.ty);
            format!("{dir} {ty} {}", p.name.name)
        }).collect();
        for (i, ps) in port_strs.iter().enumerate() {
            let comma = if i < port_strs.len() - 1 { "," } else { "" };
            self.line(&format!("{ps}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        match s.kind {
            SyncKind::Ff => self.emit_sync_ff(dst_clk, &data_ty, rst_port, stages),
            SyncKind::Gray => self.emit_sync_gray(src_clk, dst_clk, &data_ty, rst_port, stages),
            SyncKind::Handshake => self.emit_sync_handshake(src_clk, dst_clk, &data_ty, rst_port, stages),
            SyncKind::Reset => self.emit_sync_reset(dst_clk, rst_port, stages),
            SyncKind::Pulse => self.emit_sync_pulse(src_clk, dst_clk, rst_port, stages),
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Synchronizer kind helpers ────────────────────────────────────────────

    fn emit_sync_reset_begin(&mut self, dst_clk: &str, rst_port: Option<&PortDecl>) -> Option<String> {
        if let Some(rp) = rst_port {
            let is_low = matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low);
            let is_async = matches!(&rp.ty, TypeExpr::Reset(sync_type, _) if *sync_type == ResetKind::Async);
            let sensitivity = if is_async {
                let edge = if is_low { "negedge" } else { "posedge" };
                format!(" or {edge} {}", rp.name.name)
            } else {
                String::new()
            };
            self.line(&format!("always_ff @(posedge {dst_clk}{sensitivity}) begin"));
            let cond = if is_low { format!("!{}", rp.name.name) } else { rp.name.name.clone() };
            Some(cond)
        } else {
            self.line(&format!("always_ff @(posedge {dst_clk}) begin"));
            None
        }
    }

    fn emit_sync_ff(&mut self, dst_clk: &str, data_ty: &str, rst_port: Option<&PortDecl>, stages: usize) {
        self.line(&format!("// {stages}-stage FF synchronizer chain (destination clock: {dst_clk})"));
        self.line(&format!("{data_ty} sync_chain [0:STAGES-1];"));
        self.line("");

        let rst_cond = self.emit_sync_reset_begin(dst_clk, rst_port);
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            self.line("for (int i = 0; i < STAGES; i++) sync_chain[i] <= '0;");
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }
        self.line("sync_chain[0] <= data_in;");
        self.line("for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];");
        if rst_cond.is_some() {
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("assign data_out = sync_chain[STAGES-1];");
    }

    fn emit_sync_gray(&mut self, src_clk: &str, dst_clk: &str, data_ty: &str, rst_port: Option<&PortDecl>, stages: usize) {
        self.line(&format!("// Gray-code synchronizer ({stages} stages, {src_clk} → {dst_clk})"));
        self.line(&format!("{data_ty} bin_to_gray;"));
        self.line(&format!("{data_ty} gray_chain [0:STAGES-1];"));
        self.line(&format!("{data_ty} gray_to_bin;"));
        self.line("");

        // Binary-to-gray encode (combinational, source domain)
        self.line("assign bin_to_gray = data_in ^ (data_in >> 1);");
        self.line("");

        // FF chain on destination clock
        let rst_cond = self.emit_sync_reset_begin(dst_clk, rst_port);
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            self.line("for (int i = 0; i < STAGES; i++) gray_chain[i] <= '0;");
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }
        self.line("gray_chain[0] <= bin_to_gray;");
        self.line("for (int i = 1; i < STAGES; i++) gray_chain[i] <= gray_chain[i-1];");
        if rst_cond.is_some() {
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Gray-to-binary decode: binary[i] = XOR of all gray bits from MSB down to i
        // Computed as: b = g ^ (g>>1) ^ (g>>2) ^ ... — no self-reference, no ordering issue.
        self.line("// Gray-to-binary decode (prefix XOR — no self-reference)");
        self.line(&format!("always_comb begin"));
        self.indent += 1;
        self.line("gray_to_bin = gray_chain[STAGES-1];");
        self.line(&format!("for (int i = 1; i < $bits({data_ty}); i++)"));
        self.indent += 1;
        self.line("gray_to_bin ^= gray_chain[STAGES-1] >> i;");
        self.indent -= 1;
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("assign data_out = gray_to_bin;");
    }

    fn emit_sync_handshake(&mut self, src_clk: &str, dst_clk: &str, data_ty: &str, rst_port: Option<&PortDecl>, stages: usize) {
        self.line(&format!("// Handshake synchronizer ({stages} stages, {src_clk} → {dst_clk})"));
        self.line(&format!("{data_ty} data_reg;"));
        self.line("logic req_src, ack_src;");
        self.line(&format!("logic req_sync [0:STAGES-1];  // req synchronized to {dst_clk}"));
        self.line(&format!("logic ack_sync [0:STAGES-1];  // ack synchronized to {src_clk}"));
        self.line("logic ack_dst;");
        self.line("");

        let rst_name = rst_port.map(|rp| rp.name.name.as_str()).unwrap_or("1'b0");
        let is_low = rst_port.map_or(false, |rp| matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low));
        let rst_active = if is_low { format!("!{rst_name}") } else { rst_name.to_string() };

        // Source domain: latch data and toggle req
        self.line(&format!("// Source domain ({src_clk}): latch data, manage req/ack"));
        self.line(&format!("always_ff @(posedge {src_clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_active}) begin"));
        self.indent += 1;
        self.line("req_src <= 1'b0;");
        self.line("data_reg <= '0;");
        self.indent -= 1;
        self.line("end else if (data_in !== data_reg && req_src == ack_src) begin");
        self.indent += 1;
        self.line("data_reg <= data_in;");
        self.line("req_src <= ~req_src;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Synchronize req into destination domain
        self.line(&format!("// Synchronize req into {dst_clk}"));
        self.line(&format!("always_ff @(posedge {dst_clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_active}) begin"));
        self.indent += 1;
        self.line("for (int i = 0; i < STAGES; i++) req_sync[i] <= 1'b0;");
        self.line("ack_dst <= 1'b0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("req_sync[0] <= req_src;");
        self.line("for (int i = 1; i < STAGES; i++) req_sync[i] <= req_sync[i-1];");
        self.line("ack_dst <= req_sync[STAGES-1];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Synchronize ack back into source domain
        self.line(&format!("// Synchronize ack back into {src_clk}"));
        self.line(&format!("always_ff @(posedge {src_clk}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_active}) begin"));
        self.indent += 1;
        self.line("for (int i = 0; i < STAGES; i++) ack_sync[i] <= 1'b0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("ack_sync[0] <= ack_dst;");
        self.line("for (int i = 1; i < STAGES; i++) ack_sync[i] <= ack_sync[i-1];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        self.line("assign ack_src = ack_sync[STAGES-1];");
        self.line("assign data_out = data_reg;");
    }

    fn emit_sync_reset(&mut self, dst_clk: &str, _rst_port: Option<&PortDecl>, _stages: usize) {
        // Reset synchronizer: data_in is the async reset input (active high).
        // Assert immediately (async), deassert through N-stage FF chain (sync to dst_clk).
        self.line(&format!("// Reset synchronizer: async assert, sync deassert on {dst_clk}"));
        self.line("logic sync_chain [0:STAGES-1];");
        self.line("");

        self.line(&format!("always_ff @(posedge {dst_clk} or posedge data_in) begin"));
        self.indent += 1;
        self.line("if (data_in) begin");
        self.indent += 1;
        self.line("for (int i = 0; i < STAGES; i++) sync_chain[i] <= 1'b1;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("sync_chain[0] <= 1'b0;");
        self.line("for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("assign data_out = sync_chain[STAGES-1];");
    }

    fn emit_sync_pulse(&mut self, src_clk: &str, dst_clk: &str, rst_port: Option<&PortDecl>, _stages: usize) {
        let rst_name = rst_port.map(|rp| rp.name.name.as_str());
        let is_low = rst_port.map_or(false, |rp| matches!(&rp.ty, TypeExpr::Reset(_, level) if *level == ResetLevel::Low));
        let rst_cond = rst_name.map(|n| if is_low { format!("!{n}") } else { n.to_string() });

        self.line(&format!("// Pulse synchronizer: {src_clk} → {dst_clk}"));
        self.line("// Source: pulse → toggle; Destination: sync toggle → edge detect → pulse");
        self.line("logic toggle_src;");
        self.line("logic sync_chain [0:STAGES-1];");
        self.line("logic pulse_dst;");
        self.line("");

        // Source domain: toggle on input pulse
        self.line(&format!("always_ff @(posedge {src_clk}) begin"));
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) toggle_src <= 1'b0;"));
            self.line("else if (data_in) toggle_src <= ~toggle_src;");
        } else {
            self.line("if (data_in) toggle_src <= ~toggle_src;");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Destination domain: sync the toggle through FF chain
        self.line(&format!("always_ff @(posedge {dst_clk}) begin"));
        self.indent += 1;
        if let Some(ref cond) = rst_cond {
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            self.line("for (int i = 0; i < STAGES; i++) sync_chain[i] <= 1'b0;");
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }
        self.line("sync_chain[0] <= toggle_src;");
        self.line("for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];");
        if rst_cond.is_some() {
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.line("");

        // Edge detect: XOR of last two stages → single-cycle pulse
        self.line("assign data_out = sync_chain[STAGES-1] ^ sync_chain[STAGES-2];");
    }
}
