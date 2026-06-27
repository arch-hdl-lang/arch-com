//! `emit_ram` SV emitter (with kind-specific helpers) — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_ram(&mut self, r: &RamDecl) {
        use crate::ast::{RamInit, RamKind};
        use std::collections::HashMap;

        let type_params: HashMap<String, TypeExpr> = r
            .params
            .iter()
            .filter_map(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => Some((p.name.name.clone(), ty.clone())),
                _ => None,
            })
            .collect();

        let resolve_type_param = |ty: TypeExpr| -> TypeExpr {
            match ty {
                TypeExpr::Named(ident) => type_params
                    .get(&ident.name)
                    .cloned()
                    .unwrap_or(TypeExpr::Named(ident)),
                other => other,
            }
        };

        // Resolve DATA_WIDTH. Three sources, in priority order:
        //   1. `param WIDTH: type = T` — legacy explicit element type.
        //   2. First store_var's element width (when store is
        //      `Vec<UInt<W>, DEPTH>`). This matches the user's intent
        //      when they declared the store with a typed element.
        //   3. Fallback to `[7:0]` / "8" so legacy ram declarations
        //      that omit both still emit something compilable.
        let store_elem_ty: Option<TypeExpr> = r.store_vars.first().and_then(|sv| match &sv.ty {
            TypeExpr::Vec(elem, _) => Some(resolve_type_param((**elem).clone())),
            _ => None,
        });
        let data_width_ty = r
            .params
            .iter()
            .find(|p| p.name.name == "WIDTH")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => {
                    Some(self.emit_port_type_str(&resolve_type_param(ty.clone())))
                }
                _ => None,
            })
            .or_else(|| store_elem_ty.as_ref().map(|ty| self.emit_port_type_str(ty)))
            .unwrap_or_else(|| "logic [7:0]".to_string());
        // Compute the bit-width number directly from the TypeExpr to avoid
        // fragile string parsing of the emitted type (e.g. "logic [7:0]").
        let data_width_num = r
            .params
            .iter()
            .find(|p| p.name.name == "WIDTH")
            .and_then(|p| match &p.kind {
                crate::ast::ParamKind::Type(ty) => {
                    self.type_expr_data_width(&resolve_type_param(ty.clone()))
                }
                _ => None,
            })
            .or_else(|| {
                store_elem_ty
                    .as_ref()
                    .and_then(|ty| self.type_expr_data_width(ty))
            })
            .unwrap_or_else(|| "8".to_string());

        // Resolve DEPTH from param default
        let depth_expr = r
            .params
            .iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| self.emit_expr_str(e))
            .unwrap_or_else(|| "256".to_string());

        let n = &r.name.name.clone();

        // ── Module header ────────────────────────────────────────────────────
        // Standard params first (DEPTH + DATA_WIDTH), then any
        // user-declared params (Const / WidthConst) that aren't
        // covered by the standard pair. Without this, user-typed
        // ports like `wdata: in UInt<TagW>` reference an undeclared
        // SV variable when the ram is elaborated.
        let mut header_params: Vec<String> = Vec::new();
        header_params.push(format!("parameter int DEPTH = {depth_expr}"));
        // Emit user-declared params BEFORE DATA_WIDTH so that the
        // DATA_WIDTH default expression (which may reference user
        // params like `TagW`) resolves cleanly in the SV elaboration
        // order.
        for p in &r.params {
            if p.name.name == "DEPTH" || p.name.name == "WIDTH" || p.name.name == "T" {
                continue;
            }
            match &p.kind {
                crate::ast::ParamKind::Const => {
                    let default_str = p
                        .default
                        .as_ref()
                        .map(|d| format!(" = {}", self.emit_expr_str(d)))
                        .unwrap_or_default();
                    header_params.push(format!("parameter int {}{default_str}", p.name.name));
                }
                crate::ast::ParamKind::WidthConst(hi, lo) => {
                    let hi_s = self.emit_expr_str(hi);
                    let lo_s = self.emit_expr_str(lo);
                    let default_str = p
                        .default
                        .as_ref()
                        .map(|d| format!(" = {}", self.emit_expr_str(d)))
                        .unwrap_or_default();
                    header_params.push(format!(
                        "parameter [{hi_s}:{lo_s}] {}{default_str}",
                        p.name.name
                    ));
                }
                // Logic / EnumConst / ConstVec / Type are uncommon for ram
                // params; punt for now (would emit nothing → port refs
                // broken, but no current Ibex use-case hits this).
                _ => {}
            }
        }
        header_params.push(format!("parameter int DATA_WIDTH = {data_width_num}"));
        self.line(&format!("module {n} #("));
        self.indent += 1;
        let count = header_params.len();
        for (i, p) in header_params.iter().enumerate() {
            let comma = if i < count - 1 { "," } else { "" };
            self.line(&format!("{p}{comma}"));
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Top-level ports (clk, rst)
        let mut all_ports: Vec<String> = Vec::new();
        for p in &r.ports {
            let dir = match p.direction {
                Direction::In => "input",
                Direction::Out => "output",
            };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        // Port group signals flattened: {group}_{signal}
        for pg in &r.port_groups {
            for s in &pg.signals {
                let dir = match s.direction {
                    Direction::In => "input",
                    Direction::Out => "output",
                };
                let ty_str = self.emit_ram_signal_type(&s.ty, &type_params);
                all_ports.push(format!("{dir} {ty_str} {}_{}", pg.name.name, s.name.name));
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

        // ── Memory array ─────────────────────────────────────────────────────
        self.line("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];");

        // Find the clock signal name
        let clk_name = r
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());

        match r.kind {
            RamKind::Single => self.emit_ram_single(r, &clk_name, &data_width_ty),
            RamKind::SimpleDual => self.emit_ram_simple_dual(r, &clk_name, &data_width_ty),
            RamKind::TrueDual => self.emit_ram_true_dual(r, &clk_name, &data_width_ty),
            RamKind::Rom => self.emit_ram_rom(r, &clk_name),
        }

        // ── Init block ───────────────────────────────────────────────────────
        if let Some(init) = &r.init {
            self.line("");
            match init {
                RamInit::Zero => {
                    self.line("initial begin");
                    self.indent += 1;
                    self.line("for (int i = 0; i < DEPTH; i++) mem[i] = '0;");
                    self.indent -= 1;
                    self.line("end");
                }
                RamInit::None => {}
                RamInit::File(path, format) => {
                    let func = match format {
                        FileFormat::Hex => "$readmemh",
                        FileFormat::Bin => "$readmemb",
                    };
                    self.line(&format!("initial {func}(\"{path}\", mem);"));
                }
                RamInit::Array(values) => {
                    self.line("initial begin");
                    self.indent += 1;
                    for (i, v) in values.iter().enumerate() {
                        self.line(&format!("mem[{i}] = {data_width_num}'h{v:X};"));
                    }
                    self.indent -= 1;
                    self.line("end");
                }
                RamInit::Value(expr) => {
                    let v = self.emit_expr_str(expr);
                    self.line("initial begin");
                    self.indent += 1;
                    self.line(&format!("for (int i = 0; i < DEPTH; i++) mem[i] = {v};"));
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }

        if !r.asserts.is_empty() {
            let clk = r
                .ports
                .iter()
                .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone())
                .unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = r.asserts.clone();
            let rname = r.name.name.clone();
            let rst_active = Codegen::rst_active_from_ports(&r.ports);
            self.emit_asserts_for_construct(&asserts, &rname, &clk, rst_active.as_deref());
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    // ── Counter ───────────────────────────────────────────────────────────────

    fn emit_ram_signal_type(
        &self,
        ty: &TypeExpr,
        type_params: &std::collections::HashMap<String, TypeExpr>,
    ) -> String {
        match ty {
            TypeExpr::Named(ident) if ident.name == "WIDTH" => "logic [DATA_WIDTH-1:0]".to_string(),
            TypeExpr::Named(ident) if type_params.contains_key(&ident.name) => {
                "logic [DATA_WIDTH-1:0]".to_string()
            }
            other => self.emit_port_type_str(other),
        }
    }

    fn emit_ram_single(&mut self, r: &RamDecl, clk: &str, _data_width_ty: &str) {
        use crate::ast::RamWriteMode;
        // The single port group
        let pg = &r.port_groups[0];
        let pfx = &pg.name.name.clone();

        // Detect signal names
        let has_wen = pg.signals.iter().any(|s| s.name.name == "wen");
        // The chip enable is optional: a port without `en` is always enabled
        // (reads every cycle, writes whenever `wen`). Without this guard the
        // registered-read path (latency 1|2) emits `if ({pfx}_en)` referencing
        // an undeclared signal. Mirrors the optional-`en` handling in
        // `emit_ram_rom` / `emit_ram_simple_dual`.
        let has_en = pg.signals.iter().any(|s| s.name.name == "en");
        let out_sig = pg
            .signals
            .iter()
            .find(|s| s.direction == Direction::Out)
            .cloned();

        match r.latency {
            0 => {
                // Combinational read
                if let Some(ref os) = out_sig {
                    self.line("");
                    self.line(&format!("assign {pfx}_{} = mem[{pfx}_addr];", os.name.name));
                }
                self.line("");
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                // Write-gating condition: `en && wen`, dropping whichever of
                // the two enables the port omits. Both omitted ⇒ write every
                // cycle (unconditional).
                let mut wr_conds: Vec<String> = Vec::new();
                if has_en {
                    wr_conds.push(format!("{pfx}_en"));
                }
                if has_wen {
                    wr_conds.push(format!("{pfx}_wen"));
                }
                if !wr_conds.is_empty() {
                    self.line(&format!("if ({})", wr_conds.join(" && ")));
                    self.indent += 1;
                }
                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                if !wr_conds.is_empty() {
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.line("end");
            }
            1 | 2 => {
                if let Some(ref os) = out_sig {
                    let rdata_r = format!("{pfx}_{}_r", os.name.name);
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                    self.line("");
                    let write_mode = r.write_mode.unwrap_or(RamWriteMode::NoChange);
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    if has_en {
                        self.line(&format!("if ({pfx}_en) begin"));
                        self.indent += 1;
                    }
                    match write_mode {
                        RamWriteMode::WriteFirst => {
                            if has_wen {
                                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                            }
                            self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                        }
                        RamWriteMode::ReadFirst => {
                            self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                            if has_wen {
                                self.line(&format!("if ({pfx}_wen)"));
                                self.indent += 1;
                                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                                self.indent -= 1;
                            }
                        }
                        RamWriteMode::NoChange => {
                            if has_wen {
                                self.line(&format!("if ({pfx}_wen)"));
                                self.indent += 1;
                                self.line(&format!("mem[{pfx}_addr] <= {pfx}_wdata;"));
                                self.indent -= 1;
                                self.line("else");
                                self.indent += 1;
                                self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                                self.indent -= 1;
                            } else {
                                self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                            }
                        }
                    }
                    if has_en {
                        self.indent -= 1;
                        self.line("end");
                    }
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("assign {pfx}_{} = {rdata_r};", os.name.name));
                    // latency 2 adds an extra output register stage
                    if r.latency == 2 {
                        let rdata_r2 = format!("{pfx}_{}_r2", os.name.name);
                        self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r2};"));
                        self.line(&format!(
                            "always_ff @(posedge {clk}) {rdata_r2} <= {rdata_r};"
                        ));
                        self.line(&format!("assign {pfx}_{} = {rdata_r2};", os.name.name));
                    }
                }
            }
            _ => {}
        }
    }

    fn emit_ram_simple_dual(&mut self, r: &RamDecl, clk: &str, _data_width_ty: &str) {
        // Identify read port (has output signal) and write port (all inputs)
        let read_pg = r
            .port_groups
            .iter()
            .find(|pg| pg.signals.iter().any(|s| s.direction == Direction::Out));
        let write_pg = r
            .port_groups
            .iter()
            .find(|pg| pg.signals.iter().all(|s| s.direction == Direction::In));

        let (rpfx, wpfx) = match (read_pg, write_pg) {
            (Some(rp), Some(wp)) => (rp.name.name.clone(), wp.name.name.clone()),
            _ => return, // malformed
        };
        let out_sig = read_pg
            .unwrap()
            .signals
            .iter()
            .find(|s| s.direction == Direction::Out)
            .map(|s| s.name.name.clone())
            .unwrap_or_else(|| "data".to_string());

        // The read enable is optional: a `rd_port` without an `en` signal reads
        // every cycle. Without this guard the registered-read path (latency 1|2)
        // emits `if ({rpfx}_en)` referencing an undeclared signal. Mirrors the
        // optional-`en` handling already in `emit_ram_rom`.
        let has_rd_en = read_pg.unwrap().signals.iter().any(|s| s.name.name == "en");

        // Find write data signal (input data in write port)
        let wdata_sig = write_pg
            .unwrap()
            .signals
            .iter()
            .find(|s| {
                s.direction == Direction::In
                    && !["en", "addr", "mask", "wen"].contains(&s.name.name.as_str())
            })
            .map(|s| s.name.name.clone())
            .unwrap_or_else(|| "data".to_string());

        match r.latency {
            0 => {
                self.line("");
                self.line(&format!("assign {rpfx}_{out_sig} = mem[{rpfx}_addr];"));
                self.line("");
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                self.line(&format!("if ({wpfx}_en)"));
                self.indent += 1;
                self.line(&format!("mem[{wpfx}_addr] <= {wpfx}_{wdata_sig};"));
                self.indent -= 1;
                self.indent -= 1;
                self.line("end");
            }
            1 | 2 => {
                let rdata_r = format!("{rpfx}_{out_sig}_r");
                self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                self.line("");
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                self.line(&format!("if ({wpfx}_en)"));
                self.indent += 1;
                self.line(&format!("mem[{wpfx}_addr] <= {wpfx}_{wdata_sig};"));
                self.indent -= 1;
                if has_rd_en {
                    self.line(&format!("if ({rpfx}_en)"));
                    self.indent += 1;
                }
                self.line(&format!("{rdata_r} <= mem[{rpfx}_addr];"));
                if has_rd_en {
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.line("end");
                if r.latency == 2 {
                    let rdata_r2 = format!("{rpfx}_{out_sig}_r2");
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r2};"));
                    self.line(&format!(
                        "always_ff @(posedge {clk}) {rdata_r2} <= {rdata_r};"
                    ));
                    self.line(&format!("assign {rpfx}_{out_sig} = {rdata_r2};"));
                } else {
                    self.line(&format!("assign {rpfx}_{out_sig} = {rdata_r};"));
                }
            }
            _ => {}
        }
    }

    fn emit_ram_true_dual(&mut self, r: &RamDecl, clk: &str, _data_width_ty: &str) {
        // Both port groups can read and write
        let pa = &r.port_groups[0];
        let pb = &r.port_groups[1];
        let pfx_a = pa.name.name.clone();
        let pfx_b = pb.name.name.clone();

        // Per-port chip enable is optional: a port without `en` is always
        // enabled. Without these guards the registered-read path emits
        // `if ({pfx}_en)` referencing an undeclared signal (see the same
        // optional-`en` handling in `emit_ram_single` / `emit_ram_simple_dual`).
        let has_en_a = pa.signals.iter().any(|s| s.name.name == "en");
        let has_en_b = pb.signals.iter().any(|s| s.name.name == "en");

        let rdata_a = pa
            .signals
            .iter()
            .find(|s| s.direction == Direction::Out)
            .map(|s| s.name.name.clone())
            .unwrap_or_else(|| "rdata".to_string());
        let rdata_b = pb
            .signals
            .iter()
            .find(|s| s.direction == Direction::Out)
            .map(|s| s.name.name.clone())
            .unwrap_or_else(|| "rdata".to_string());

        let rdata_a_r = format!("{pfx_a}_{rdata_a}_r");
        let rdata_b_r = format!("{pfx_b}_{rdata_b}_r");
        match r.latency {
            0 => {
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                let cond_a = if has_en_a {
                    format!("{pfx_a}_en && {pfx_a}_wen")
                } else {
                    format!("{pfx_a}_wen")
                };
                self.line(&format!("if ({cond_a})"));
                self.indent += 1;
                self.line(&format!("mem[{pfx_a}_addr] <= {pfx_a}_wdata;"));
                self.indent -= 1;
                let cond_b = if has_en_b {
                    format!("{pfx_b}_en && {pfx_b}_wen")
                } else {
                    format!("{pfx_b}_wen")
                };
                self.line(&format!("if ({cond_b})"));
                self.indent += 1;
                self.line(&format!("mem[{pfx_b}_addr] <= {pfx_b}_wdata;"));
                self.indent -= 1;
                self.indent -= 1;
                self.line("end");
                self.line(&format!("assign {pfx_a}_{rdata_a} = mem[{pfx_a}_addr];"));
                self.line(&format!("assign {pfx_b}_{rdata_b} = mem[{pfx_b}_addr];"));
            }
            1 | 2 => {
                self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_a_r};"));
                self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_b_r};"));
                self.line("");
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                if has_en_a {
                    self.line(&format!("if ({pfx_a}_en) begin"));
                    self.indent += 1;
                }
                self.line(&format!("if ({pfx_a}_wen)"));
                self.indent += 1;
                self.line(&format!("mem[{pfx_a}_addr] <= {pfx_a}_wdata;"));
                self.indent -= 1;
                self.line("else");
                self.indent += 1;
                self.line(&format!("{rdata_a_r} <= mem[{pfx_a}_addr];"));
                self.indent -= 1;
                if has_en_a {
                    self.indent -= 1;
                    self.line("end");
                }
                if has_en_b {
                    self.line(&format!("if ({pfx_b}_en) begin"));
                    self.indent += 1;
                }
                self.line(&format!("if ({pfx_b}_wen)"));
                self.indent += 1;
                self.line(&format!("mem[{pfx_b}_addr] <= {pfx_b}_wdata;"));
                self.indent -= 1;
                self.line("else");
                self.indent += 1;
                self.line(&format!("{rdata_b_r} <= mem[{pfx_b}_addr];"));
                self.indent -= 1;
                if has_en_b {
                    self.indent -= 1;
                    self.line("end");
                }
                self.indent -= 1;
                self.line("end");
                if r.latency == 2 {
                    let rdata_a_r2 = format!("{pfx_a}_{rdata_a}_r2");
                    let rdata_b_r2 = format!("{pfx_b}_{rdata_b}_r2");
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_a_r2};"));
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_b_r2};"));
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!("{rdata_a_r2} <= {rdata_a_r};"));
                    self.line(&format!("{rdata_b_r2} <= {rdata_b_r};"));
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("assign {pfx_a}_{rdata_a} = {rdata_a_r2};"));
                    self.line(&format!("assign {pfx_b}_{rdata_b} = {rdata_b_r2};"));
                } else {
                    self.line(&format!("assign {pfx_a}_{rdata_a} = {rdata_a_r};"));
                    self.line(&format!("assign {pfx_b}_{rdata_b} = {rdata_b_r};"));
                }
            }
            _ => {}
        }
    }

    fn emit_ram_rom(&mut self, r: &RamDecl, clk: &str) {
        let pg = &r.port_groups[0];
        let pfx = &pg.name.name;
        let out_sig = pg.signals.iter().find(|s| s.direction == Direction::Out);

        match r.latency {
            0 => {
                // Combinational read
                if let Some(os) = out_sig {
                    self.line("");
                    self.line(&format!("assign {pfx}_{} = mem[{pfx}_addr];", os.name.name));
                }
            }
            1 => {
                if let Some(os) = out_sig {
                    let rdata_r = format!("{pfx}_{}_r", os.name.name);
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                    self.line("");
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    let has_en = pg.signals.iter().any(|s| s.name.name == "en");
                    if has_en {
                        self.line(&format!("if ({pfx}_en)"));
                        self.indent += 1;
                    }
                    self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                    if has_en {
                        self.indent -= 1;
                    }
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("assign {pfx}_{} = {rdata_r};", os.name.name));
                }
            }
            2 => {
                if let Some(os) = out_sig {
                    let rdata_r = format!("{pfx}_{}_r", os.name.name);
                    let rdata_r2 = format!("{pfx}_{}_r2", os.name.name);
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r};"));
                    self.line(&format!("logic [DATA_WIDTH-1:0] {rdata_r2};"));
                    self.line("");
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!("{rdata_r} <= mem[{pfx}_addr];"));
                    self.line(&format!("{rdata_r2} <= {rdata_r};"));
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!("assign {pfx}_{} = {rdata_r2};", os.name.name));
                }
            }
            _ => {}
        }
    }

    // ── Linklist ─────────────────────────────────────────────────────────────
}
