//! `emit_clkgate` SV emitter — extracted from `codegen/mod.rs` to mirror
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(super) fn emit_clkgate(&mut self, c: &crate::ast::ClkGateDecl) {
        let n = &c.name.name;

        // Find port names
        let clk_in = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::In)
            .map(|p| p.name.name.as_str()).unwrap_or("clk_in");
        let clk_out = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::Out)
            .map(|p| p.name.name.as_str()).unwrap_or("clk_out");
        let enable = c.ports.iter().find(|p| p.name.name == "enable")
            .map(|p| p.name.name.as_str()).unwrap_or("enable");
        let test_en = c.ports.iter().find(|p| p.name.name == "test_en")
            .map(|p| p.name.name.as_str());

        // Module header
        self.line(&format!("module {n} ("));
        self.indent += 1;
        let port_strs: Vec<String> = c.ports.iter().map(|p| {
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

        let en_expr = if let Some(te) = test_en {
            format!("{enable} | {te}")
        } else {
            enable.to_string()
        };

        match c.kind {
            crate::ast::ClkGateKind::Latch => {
                self.line("logic en_latched;");
                self.line(&format!("always_latch if (!{clk_in}) en_latched = {en_expr};"));
                self.line(&format!("assign {clk_out} = {clk_in} & en_latched;"));
            }
            crate::ast::ClkGateKind::And => {
                self.line(&format!("assign {clk_out} = {clk_in} & ({en_expr});"));
            }
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }
}
