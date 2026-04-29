//! `emit_counter` SV emitter — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(super) fn emit_counter(&mut self, c: &crate::ast::CounterDecl) {
        use crate::ast::{CounterMode, CounterDirection};

        let n = &c.name.name.clone();

        // Optional max signal. Declared as `port max: in UInt<W>` on the
        // counter; used for wrap target, saturate ceiling, and the `at_max`
        // comparator. Callers tie it off to a constant for fixed-max counters
        // (synthesis folds it). When absent, the counter wraps at the natural
        // width bound (all-ones).
        let max_port = c.ports.iter()
            .find(|p| p.name.name == "max" && matches!(p.direction, Direction::In))
            .map(|_| "max".to_string());

        // Determine counter width
        // Use MAX param if present, else look for WIDTH
        let width_param = c.params.iter()
            .find(|p| p.name.name == "WIDTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e));

        // Find ports to determine direction
        let has_inc  = c.ports.iter().any(|p| p.name.name == "inc");
        let has_dec  = c.ports.iter().any(|p| p.name.name == "dec");
        let has_load = c.ports.iter().any(|p| p.name.name == "load");
        let has_clear= c.ports.iter().any(|p| p.name.name == "clear");
        let value_port = c.ports.iter().find(|p| p.name.name == "value");

        // Compute width from value port type or fallback
        let count_width = if let Some(vp) = value_port {
            match &vp.ty {
                TypeExpr::UInt(w) => self.emit_expr_str(w),
                _ => width_param.clone().unwrap_or_else(|| "8".to_string()),
            }
        } else {
            width_param.clone().unwrap_or_else(|| "8".to_string())
        };

        let clk = c.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&c.ports);

        // ── Module header ─────────────────────────────────────────────────────
        if c.params.is_empty() {
            self.line(&format!("module {n} ("));
        } else {
            self.line(&format!("module {n} #("));
            self.indent += 1;
            for (i, p) in c.params.iter().enumerate() {
                let comma = if i < c.params.len() - 1 { "," } else { "" };
                let default_str = p.default.as_ref()
                    .map(|e| format!(" = {}", self.emit_expr_str(e)))
                    .unwrap_or_default();
                self.line(&format!("parameter int {}{default_str}{comma}", p.name.name));
            }
            self.indent -= 1;
            self.line(") (");
        }
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

        let init_val = c.init.as_ref()
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "'0".to_string());

        // ── Internal register ─────────────────────────────────────────────────
        self.line(&format!("logic [{}] count_r;", Self::fold_width_str(&count_width)));

        // ── Determine FF sensitivity list ─────────────────────────────────────
        let ff_sens = Self::ff_sensitivity(&clk, &rst, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst, is_low);

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;

        // Reset branch
        self.line(&format!("if ({rst_cond}) count_r <= {init_val};"));

        // Load/clear
        if has_clear {
            self.line(&format!("else if (clear) count_r <= {init_val};"));
        }
        if has_load {
            self.line("else if (load) count_r <= load_data;");
        }

        match (c.direction, c.mode) {
            (CounterDirection::Up, CounterMode::Wrap) => {
                let max_cond = if let Some(mp) = &max_port {
                    format!("count_r == {mp}")
                } else {
                    format!("&count_r")  // all bits set
                };
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("if ({max_cond}) count_r <= {init_val};"));
                self.line("else count_r <= count_r + 1;");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Down, CounterMode::Wrap) => {
                let min_cond = "count_r == '0";
                let max_val = if let Some(mp) = &max_port {
                    mp.clone()
                } else {
                    format!("'1")
                };
                let dec_cond = if has_dec { "else if (dec) begin" } else { "else begin" };
                self.line(dec_cond);
                self.indent += 1;
                self.line(&format!("if ({min_cond}) count_r <= {max_val};"));
                self.line("else count_r <= count_r - 1;");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::UpDown, CounterMode::Wrap) => {
                self.line("else if (inc && !dec) count_r <= count_r + 1;");
                self.line("else if (dec && !inc) count_r <= count_r - 1;");
            }
            (CounterDirection::Up, CounterMode::Saturate) => {
                let max_cond = if let Some(mp) = &max_port {
                    format!("count_r < {mp}")
                } else {
                    format!("!(&count_r)")
                };
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("if ({max_cond}) count_r <= count_r + 1;"));
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Down, CounterMode::Saturate) => {
                let dec_cond = if has_dec { "else if (dec) begin" } else { "else begin" };
                self.line(dec_cond);
                self.indent += 1;
                self.line("if (count_r > '0) count_r <= count_r - 1;");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Up, CounterMode::Gray) => {
                // Gray: increment binary then apply bin→gray
                self.line("else if (inc) begin");
                self.indent += 1;
                self.line("count_r <= (count_r + 1) ^ ((count_r + 1) >> 1);");
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Up, CounterMode::OneHot) => {
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("count_r <= {{count_r[{count_width}-2:0], count_r[{count_width}-1]}};"));
                self.indent -= 1;
                self.line("end");
            }
            (CounterDirection::Up, CounterMode::Johnson) => {
                let inc_cond = if has_inc { "else if (inc) begin" } else { "else begin" };
                self.line(inc_cond);
                self.indent += 1;
                self.line(&format!("count_r <= {{~count_r[0], count_r[{count_width}-1:1]}};"));
                self.indent -= 1;
                self.line("end");
            }
            // Default: simple up wrap
            _ => {
                let inc_cond = if has_inc { "else if (inc)" } else { "else" };
                self.line(&format!("{inc_cond} count_r <= count_r + 1;"));
            }
        }

        self.indent -= 1;
        self.line("end");

        // ── Output assignments ────────────────────────────────────────────────
        if value_port.is_some() {
            self.line("assign value = count_r;");
        }
        // at_max
        if c.ports.iter().any(|p| p.name.name == "at_max") {
            let max_expr = if let Some(mp) = &max_port {
                format!("count_r == {mp}")
            } else {
                format!("&count_r")
            };
            self.line(&format!("assign at_max = ({max_expr});"));
        }
        // at_min
        if c.ports.iter().any(|p| p.name.name == "at_min") {
            self.line("assign at_min = (count_r == '0);");
        }

        // Auto-generated safety assertions for counter invariants
        {
            self.line("");
            self.line("// synopsys translate_off");
            if let Some(mp) = &max_port {
                self.line(&format!(
                    "_auto_count_range: assert property (@(posedge {clk}) count_r <= {mp})"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"COUNTER OVERFLOW: {n}.count_r > {mp}\");"
                ));
            }
            self.line("// synopsys translate_on");
        }

        if !c.asserts.is_empty() {
            let clk = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = c.asserts.clone();
            let cname = c.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &cname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Arbiter ───────────────────────────────────────────────────────────────

}
