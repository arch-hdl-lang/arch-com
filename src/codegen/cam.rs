//! `emit_cam` SV emitter — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(super) fn emit_cam(&mut self, c: &crate::ast::CamDecl) {
        let n = c.name.name.clone();

        // Required params (validated by typecheck): DEPTH, KEY_W
        let depth_default = c.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());
        let key_w_default = c.params.iter()
            .find(|p| p.name.name == "KEY_W")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());
        // v3: optional VAL_W param activates the value-payload bundle.
        let has_value = c.params.iter().any(|p| p.name.name == "VAL_W");
        let val_w_default = c.params.iter()
            .find(|p| p.name.name == "VAL_W")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "0".to_string());

        let clk = c.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&c.ports);

        // ── Module header ─────────────────────────────────────────────────────
        self.line(&format!("module {n} #("));
        self.indent += 1;
        if has_value {
            self.line(&format!("parameter int DEPTH = {depth_default},"));
            self.line(&format!("parameter int KEY_W = {key_w_default},"));
            self.line(&format!("parameter int VAL_W = {val_w_default}"));
        } else {
            self.line(&format!("parameter int DEPTH = {depth_default},"));
            self.line(&format!("parameter int KEY_W = {key_w_default}"));
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        let mut all_ports: Vec<String> = Vec::new();
        for p in &c.ports {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
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

        // ── Storage ──────────────────────────────────────────────────────────
        self.line("logic [DEPTH-1:0]      entry_valid_r;");
        self.line("logic [KEY_W-1:0]      entry_key_r [DEPTH];");
        if has_value {
            self.line("logic [VAL_W-1:0]      entry_value_r [DEPTH];");
        }
        self.line("");

        // ── Combinational match ──────────────────────────────────────────────
        // search_mask[i] = entry_valid_r[i] && (entry_key_r[i] == search_key)
        self.line("always_comb begin");
        self.indent += 1;
        self.line("for (int i = 0; i < DEPTH; i++) begin");
        self.indent += 1;
        self.line("search_mask[i] = entry_valid_r[i] && (entry_key_r[i] == search_key);");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("assign search_any = |search_mask;");
        self.line("");

        // ── Priority encoder for search_first (LSB-first) ────────────────────
        self.line("always_comb begin");
        self.indent += 1;
        self.line("search_first = '0;");
        self.line("for (int i = DEPTH-1; i >= 0; i--) begin");
        self.indent += 1;
        self.line("if (search_mask[i]) search_first = i[$clog2(DEPTH)-1:0];");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Value-payload read (v3) ──────────────────────────────────────────
        // read_value reflects the entry at search_first; the caller should
        // qualify with search_any (it reads as 0 when there is no match).
        if has_value {
            self.line("assign read_value = entry_value_r[search_first];");
            self.line("");
        }

        // ── Sequential write port(s) ─────────────────────────────────────────
        // v2: if write2_* ports exist, emit two sequential write blocks back-
        // to-back (write1 first, then write2) so write2 wins on same-index
        // conflict (last-write semantics). Different-index writes both commit.
        let has_w2 = c.ports.iter().any(|p| p.name.name == "write2_valid");

        let ff_sens = Self::ff_sensitivity(&clk, &rst, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line("entry_valid_r <= '0;");
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        // Port 1
        self.line("if (write_valid) begin");
        self.indent += 1;
        self.line("if (write_set) begin");
        self.indent += 1;
        self.line("entry_valid_r[write_idx] <= 1'b1;");
        self.line("entry_key_r[write_idx] <= write_key;");
        if has_value {
            self.line("entry_value_r[write_idx] <= write_value;");
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("entry_valid_r[write_idx] <= 1'b0;");
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        // Port 2 (v2 only) — placed AFTER port 1 so it wins on same-index conflict.
        if has_w2 {
            self.line("if (write2_valid) begin");
            self.indent += 1;
            self.line("if (write2_set) begin");
            self.indent += 1;
            self.line("entry_valid_r[write2_idx] <= 1'b1;");
            self.line("entry_key_r[write2_idx] <= write2_key;");
            if has_value {
                self.line("entry_value_r[write2_idx] <= write2_value;");
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            self.line("entry_valid_r[write2_idx] <= 1'b0;");
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        if !c.asserts.is_empty() {
            let asserts = c.asserts.clone();
            self.emit_asserts_for_construct(&asserts, &n, &clk);
        }

        self.indent -= 1;
        self.line("endmodule");
        self.line("");
    }

}
