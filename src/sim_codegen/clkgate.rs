//! `clkgate` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

impl<'a> SimCodegen<'a> {
    pub(crate) fn gen_clkgate(&self, c: &crate::ast::ClkGateDecl) -> SimModel {
        let class = format!("V{}", c.name.name);

        let clk_in = c
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::In)
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk_in");
        let clk_out = c
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)) && p.direction == Direction::Out)
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk_out");
        let enable = "enable";
        let test_en = c
            .ports
            .iter()
            .find(|p| p.name.name == "test_en")
            .map(|p| p.name.name.as_str());

        let mut h = String::new();
        h.push_str(&format!(
            "#pragma once\n#include <cstdint>\nclass {} {{\npublic:\n",
            class
        ));

        for p in &c.ports {
            h.push_str(&format!("  uint8_t {} = 0;\n", p.name.name));
        }

        if c.kind == crate::ast::ClkGateKind::Latch {
            h.push_str("  uint8_t _en_latched = 0;\n");
        }

        h.push_str("  void eval();\n");
        h.push_str("  void eval_comb();\n");
        h.push_str("  void eval_posedge();\n");
        h.push_str("};\n");

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{}.h\"\n", class));

        let en_expr = if let Some(te) = test_en {
            format!("{enable} | {te}")
        } else {
            enable.to_string()
        };

        // eval_comb — the actual gate logic
        cpp.push_str(&format!("void {}::eval_comb() {{\n", class));
        match c.kind {
            crate::ast::ClkGateKind::Latch => {
                cpp.push_str(&format!(
                    "  if (!{clk_in}) _en_latched = ({en_expr}) ? 1 : 0;\n"
                ));
                cpp.push_str(&format!("  {clk_out} = {clk_in} & _en_latched;\n"));
            }
            crate::ast::ClkGateKind::And => {
                cpp.push_str(&format!(
                    "  {clk_out} = {clk_in} & (({en_expr}) ? 1 : 0);\n"
                ));
            }
        }
        cpp.push_str("}\n");

        // eval_posedge — no-op for clkgate
        cpp.push_str(&format!("void {}::eval_posedge() {{}}\n", class));

        // eval — calls both
        cpp.push_str(&format!("void {}::eval() {{ eval_comb(); }}\n", class));

        SimModel {
            class_name: class,
            header: h,
            impl_: cpp,
        }
    }
}
