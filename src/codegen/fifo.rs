//! `emit_fifo` SV emitter (sync / lifo / async bodies) — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(super) fn emit_fifo(&mut self, f: &FifoDecl) {
        use crate::resolve::detect_async_fifo;
        let is_async = detect_async_fifo(&f.ports);

        // Resolve DEPTH and TYPE from params
        let depth_expr = f.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "16".to_string());

        // Find the type parameter (any name) and compute its bit-width for DATA_WIDTH
        let type_param_name = f.params.iter()
            .find(|p| matches!(p.kind, crate::ast::ParamKind::Type(_)))
            .map(|p| p.name.name.clone());
        let data_width_str = f.params.iter()
            .find(|p| matches!(p.kind, crate::ast::ParamKind::Type(_)))
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => self.type_expr_data_width(ty),
                _ => None,
            })
            .unwrap_or_else(|| "8".to_string());

        // Check for OVERFLOW param (0 = block when full, 1 = overwrite oldest)
        let overflow_expr = f.params.iter()
            .find(|p| p.name.name == "OVERFLOW")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());
        let has_overflow_param = f.params.iter().any(|p| p.name.name == "OVERFLOW");

        // Collect port names to know what's declared
        let port_names: Vec<&str> = f.ports.iter().map(|p| p.name.name.as_str()).collect();

        let n = &f.name.name;

        // ── Module header ────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        self.line(&format!("parameter int  DEPTH      = {depth_expr},"));
        if has_overflow_param {
            self.line(&format!("parameter int  OVERFLOW   = {overflow_expr},"));
        }
        self.line(&format!("parameter int  DATA_WIDTH = {data_width_str}"));
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Emit declared ports
        for (i, p) in f.ports.iter().enumerate() {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            // Type param references → use DATA_WIDTH
            let ty_str = self.emit_fifo_port_type(&p.ty, &type_param_name);
            let comma = if i < f.ports.len() - 1 { "," } else { "" };
            self.line(&format!("{dir} {ty_str} {}{comma}", p.name.name));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        if is_async {
            self.emit_fifo_async_body(f, &port_names, has_overflow_param);
        } else if f.kind == FifoKind::Lifo {
            self.emit_fifo_lifo_body(f, &port_names);
        } else {
            self.emit_fifo_sync_body(f, &port_names, has_overflow_param);
        }

        // Auto-generated safety assertions for FIFO invariants
        {
            let clk_names: Vec<String> = f.ports.iter()
                .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone())
                .collect();
            let clk = clk_names.first().cloned().unwrap_or_else(|| "clk".to_string());
            let n = &f.name.name;

            self.line("");
            self.line("// synopsys translate_off");
            if is_async {
                // Async FIFO: assertions in write domain (wr_clk) and read domain (rd_clk)
                let wr_clk = clk_names.first().map(|s| s.as_str()).unwrap_or("wr_clk");
                let rd_clk = clk_names.get(1).map(|s| s.as_str()).unwrap_or("rd_clk");
                self.line(&format!(
                    "_auto_no_overflow: assert property (@(posedge {wr_clk}) !(push_valid && push_ready && full_r))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO OVERFLOW: {n}.push while full\");"
                ));
                self.line(&format!(
                    "_auto_no_underflow: assert property (@(posedge {rd_clk}) !(pop_valid && pop_ready && empty_r))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO UNDERFLOW: {n}.pop while empty\");"
                ));
            } else if f.kind == FifoKind::Lifo {
                // LIFO: sp-based full/empty
                self.line(&format!(
                    "_auto_no_overflow: assert property (@(posedge {clk}) !(push_valid && push_ready && full))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO OVERFLOW: {n}.push while full\");"
                ));
                self.line(&format!(
                    "_auto_no_underflow: assert property (@(posedge {clk}) !(pop_valid && pop_ready && empty))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO UNDERFLOW: {n}.pop while empty\");"
                ));
            } else {
                // Sync FIFO: pointer-based full/empty
                if has_overflow_param {
                    // Only assert overflow when OVERFLOW mode is disabled
                    self.line(&format!(
                        "_auto_no_overflow: assert property (@(posedge {clk}) OVERFLOW || !(push_valid && push_ready && full))"
                    ));
                } else {
                    self.line(&format!(
                        "_auto_no_overflow: assert property (@(posedge {clk}) !(push_valid && push_ready && full))"
                    ));
                }
                self.line(&format!(
                    "  else $fatal(1, \"FIFO OVERFLOW: {n}.push while full\");"
                ));
                self.line(&format!(
                    "_auto_no_underflow: assert property (@(posedge {clk}) !(pop_valid && pop_ready && empty))"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"FIFO UNDERFLOW: {n}.pop while empty\");"
                ));
            }
            self.line("// synopsys translate_on");
        }

        if !f.asserts.is_empty() {
            let clk = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = f.asserts.clone();
            let fname = f.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &fname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    fn emit_fifo_port_type(&self, ty: &TypeExpr, type_param_name: &Option<String>) -> String {
        if let Some(tpn) = type_param_name {
            if let TypeExpr::Named(ident) = ty {
                if ident.name == *tpn {
                    return "logic [DATA_WIDTH-1:0]".to_string();
                }
            }
        }
        self.emit_port_type_str(ty)
    }

    fn emit_fifo_sync_body(&mut self, f: &FifoDecl, port_names: &[&str], has_overflow_param: bool) {
        self.line("localparam int PTR_W = $clog2(DEPTH) + 1;");
        self.line("");
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");
        self.line("logic [PTR_W-1:0]     wr_ptr;");
        self.line("logic [PTR_W-1:0]     rd_ptr;");
        if !port_names.contains(&"full") {
            self.line("logic                 full;");
        }
        if !port_names.contains(&"empty") {
            self.line("logic                 empty;");
        }
        self.line("");
        self.line("// Full when MSBs differ and lower bits match");
        self.line("assign full        = (wr_ptr[PTR_W-1] != rd_ptr[PTR_W-1]) &&");
        self.line("                     (wr_ptr[PTR_W-2:0] == rd_ptr[PTR_W-2:0]);");
        self.line("assign empty       = (wr_ptr == rd_ptr);");
        if has_overflow_param {
            self.line("// OVERFLOW mode: push_ready always high; overwrite oldest when full");
            self.line("assign push_ready  = (OVERFLOW != 0) ? 1'b1 : !full;");
        } else {
            self.line("assign push_ready  = !full;");
        }
        self.line("assign pop_valid   = !empty;");
        self.line("assign pop_data    = mem[rd_ptr[PTR_W-2:0]];");
        self.line("");

        // Determine reset port info
        let (rst, is_async, is_low) = Self::extract_reset_info(&f.ports);
        let clk = f.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");
        let ff_sens = Self::ff_sensitivity(clk, &rst, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line("wr_ptr <= '0;");
        self.line("rd_ptr <= '0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        if has_overflow_param {
            self.line("if (push_valid && push_ready) begin");
            self.indent += 1;
            self.line("mem[wr_ptr[PTR_W-2:0]] <= push_data;");
            self.line("wr_ptr <= wr_ptr + 1;");
            self.line("// In overflow mode, advance rd_ptr when writing to a full FIFO");
            self.line("if ((OVERFLOW != 0) && full && !(pop_ready)) rd_ptr <= rd_ptr + 1;");
            self.indent -= 1;
            self.line("end");
        } else {
            self.line("if (push_valid && push_ready) begin");
            self.indent += 1;
            self.line("mem[wr_ptr[PTR_W-2:0]] <= push_data;");
            self.line("wr_ptr <= wr_ptr + 1;");
            self.indent -= 1;
            self.line("end");
        }
        self.line("if (pop_valid && pop_ready) begin");
        self.indent += 1;
        self.line("rd_ptr <= rd_ptr + 1;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_fifo_lifo_body(&mut self, f: &FifoDecl, port_names: &[&str]) {
        self.line("localparam int PTR_W = $clog2(DEPTH + 1);");
        self.line("");
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");
        self.line("logic [PTR_W-1:0]     sp;");
        if !port_names.contains(&"full") {
            self.line("logic                 full;");
        }
        if !port_names.contains(&"empty") {
            self.line("logic                 empty;");
        }
        self.line("");
        self.line("assign full        = (sp == DEPTH[PTR_W-1:0]);");
        self.line("assign empty       = (sp == '0);");
        self.line("assign push_ready  = !full;");
        self.line("assign pop_valid   = !empty;");
        self.line("assign pop_data    = mem[sp - 1];");
        self.line("");

        let (rst, is_async, is_low) = Self::extract_reset_info(&f.ports);
        let clk = f.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");
        let ff_sens = Self::ff_sensitivity(clk, &rst, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line("sp <= '0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("if (push_valid && push_ready && pop_valid && pop_ready) begin");
        self.indent += 1;
        self.line("// Simultaneous push+pop: replace top of stack");
        self.line("mem[sp - 1] <= push_data;");
        self.indent -= 1;
        self.line("end else if (push_valid && push_ready) begin");
        self.indent += 1;
        self.line("mem[sp] <= push_data;");
        self.line("sp <= sp + 1;");
        self.indent -= 1;
        self.line("end else if (pop_valid && pop_ready) begin");
        self.indent += 1;
        self.line("sp <= sp - 1;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    fn emit_fifo_async_body(&mut self, f: &FifoDecl, port_names: &[&str], has_overflow_param: bool) {
        // Find wr_clk, rd_clk, rst port names
        let clock_ports: Vec<&PortDecl> = f.ports.iter()
            .filter(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .collect();
        let wr_clk = clock_ports.get(0).map(|p| p.name.name.as_str()).unwrap_or("wr_clk");
        let rd_clk = clock_ports.get(1).map(|p| p.name.name.as_str()).unwrap_or("rd_clk");
        let (rst, _is_async_rst, is_low) = Self::extract_reset_info(&f.ports);
        // Async FIFOs always use async reset (reset in sensitivity list for all FF blocks)
        let rst_cond = Self::rst_condition(&rst, is_low);
        let rst_edge = if is_low { "negedge" } else { "posedge" };

        self.line("localparam int PTR_W = $clog2(DEPTH) + 1;");
        self.line("");
        self.line("// Gray-code helper functions");
        self.line("function automatic logic [PTR_W-1:0] bin2gray(input logic [PTR_W-1:0] b);");
        self.indent += 1;
        self.line("return b ^ (b >> 1);");
        self.indent -= 1;
        self.line("endfunction");
        self.line("function automatic logic [PTR_W-1:0] gray2bin(input logic [PTR_W-1:0] g);");
        self.indent += 1;
        self.line("logic [PTR_W-1:0] b;");
        self.line("b[PTR_W-1] = g[PTR_W-1];");
        self.line("for (int i = PTR_W-2; i >= 0; i--) b[i] = b[i+1] ^ g[i];");
        self.line("return b;");
        self.indent -= 1;
        self.line("endfunction");
        self.line("");
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");
        self.line("logic [PTR_W-1:0] wr_ptr_bin, rd_ptr_bin;");
        self.line("logic [PTR_W-1:0] wr_ptr_gray, rd_ptr_gray;");
        self.line("// Two-stage synchronizers");
        self.line("logic [PTR_W-1:0] wr_ptr_gray_s1, wr_ptr_gray_sync; // in rd domain");
        self.line("logic [PTR_W-1:0] rd_ptr_gray_s1, rd_ptr_gray_sync; // in wr domain");
        self.line("");
        self.line("assign wr_ptr_gray = bin2gray(wr_ptr_bin);");
        self.line("assign rd_ptr_gray = bin2gray(rd_ptr_bin);");
        self.line("");
        self.line(&format!("// Sync wr_ptr into rd domain ({rd_clk})"));
        self.line(&format!("always_ff @(posedge {rd_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin wr_ptr_gray_s1 <= '0; wr_ptr_gray_sync <= '0; end"));
        self.line("else begin wr_ptr_gray_s1 <= wr_ptr_gray; wr_ptr_gray_sync <= wr_ptr_gray_s1; end");
        self.indent -= 1;
        self.line("end");
        self.line(&format!("// Sync rd_ptr into wr domain ({wr_clk})"));
        self.line(&format!("always_ff @(posedge {wr_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin rd_ptr_gray_s1 <= '0; rd_ptr_gray_sync <= '0; end"));
        self.line("else begin rd_ptr_gray_s1 <= rd_ptr_gray; rd_ptr_gray_sync <= rd_ptr_gray_s1; end");
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("// Write domain: full detection using synced rd_ptr");
        self.line("logic full_r;");
        self.line("logic [PTR_W-1:0] rd_ptr_bin_wr;");
        self.line("assign rd_ptr_bin_wr = gray2bin(rd_ptr_gray_sync);");
        self.line("assign full_r  = (wr_ptr_bin[PTR_W-1] != rd_ptr_bin_wr[PTR_W-1]) &&");
        self.line("                 (wr_ptr_bin[PTR_W-2:0] == rd_ptr_bin_wr[PTR_W-2:0]);");
        if has_overflow_param {
            self.line("assign push_ready = (OVERFLOW != 0) ? 1'b1 : !full_r;");
        } else {
            self.line("assign push_ready = !full_r;");
        }
        self.line(&format!("always_ff @(posedge {wr_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) wr_ptr_bin <= '0;"));
        self.line("else if (push_valid && push_ready) begin");
        self.indent += 1;
        self.line("mem[wr_ptr_bin[PTR_W-2:0]] <= push_data;");
        self.line("wr_ptr_bin <= wr_ptr_bin + 1;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");
        self.line("// Read domain: empty detection using synced wr_ptr");
        self.line("logic empty_r;");
        self.line("logic [PTR_W-1:0] wr_ptr_bin_rd;");
        self.line("assign wr_ptr_bin_rd = gray2bin(wr_ptr_gray_sync);");
        self.line("assign empty_r = (rd_ptr_bin == wr_ptr_bin_rd);");
        self.line("assign pop_valid = !empty_r;");
        self.line("assign pop_data  = mem[rd_ptr_bin[PTR_W-2:0]];");
        if port_names.contains(&"full") {
            self.line("assign full  = full_r;");
        }
        if port_names.contains(&"empty") {
            self.line("assign empty = empty_r;");
        }
        self.line(&format!("always_ff @(posedge {rd_clk} or {rst_edge} {rst}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) rd_ptr_bin <= '0;"));
        self.line("else if (pop_valid && pop_ready) rd_ptr_bin <= rd_ptr_bin + 1;");
        self.indent -= 1;
        self.line("end");
    }

}
