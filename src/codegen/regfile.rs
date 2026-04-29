//! `emit_regfile` SV emitter — extracted from `codegen/mod.rs` to mirror
//! the per-construct submodule layout `sim_codegen/` already uses.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields (`indent`, etc.) and helpers without bumping their
//! visibility. The single entry point `emit_regfile` is `pub(super)` so
//! `mod.rs`'s dispatch can call it; the `emit_regfile_port_scalar`
//! helper stays private to this module.

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_regfile(&mut self, r: &crate::ast::RegfileDecl) {
        use crate::ast::ParamKind;
        let n = &r.name.name.clone();

        let nregs = r.param_int("NREGS", 32);

        // Data width: prefer the UInt<N> type of the data signal in the write port,
        // then fall back to XLEN/WIDTH/DATA_WIDTH params.
        let data_width_num: String = r.write_ports.as_ref()
            .and_then(|wp| wp.signals.iter().find(|s| s.name.name == "data"))
            .and_then(|s| if let TypeExpr::UInt(w) = &s.ty { Some(self.emit_expr_str(w)) } else { None })
            .or_else(|| {
                r.params.iter()
                    .find(|p| matches!(p.name.name.as_str(), "XLEN" | "WIDTH" | "DATA_WIDTH"))
                    .and_then(|p| match &p.kind {
                        ParamKind::Const | ParamKind::WidthConst(..) => p.default.as_ref().map(|e| self.emit_expr_str(e)),
                        _ => None,
                    })
            })
            .unwrap_or_else(|| "32".to_string());

        // Determine addr width: ceil(log2(NREGS))
        let addr_width = crate::width::index_width(nregs);

        // Read/write port counts — resolve param references
        let nread = r.read_ports.as_ref()
            .map(|rp| r.resolve_count_expr(&rp.count_expr))
            .unwrap_or(1);
        let nwrite = r.write_ports.as_ref()
            .map(|wp| r.resolve_count_expr(&wp.count_expr))
            .unwrap_or(1);

        let clk = r.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst, is_async, is_low) = Self::extract_reset_info(&r.ports);

        // ── Module header ─────────────────────────────────────────────────────
        // Emit one SV parameter per ARCH param declaration
        self.line(&format!("module {n} #("));
        self.indent += 1;
        let param_count = r.params.len();
        for (i, p) in r.params.iter().enumerate() {
            let comma = if i < param_count - 1 { "," } else { "" };
            let val = p.default.as_ref().map(|e| self.emit_expr_str(e)).unwrap_or_else(|| "0".to_string());
            self.line(&format!("parameter int {} = {}{}", p.name.name, val, comma));
        }
        self.indent -= 1;
        self.line(") (");
        self.indent += 1;

        // Build port list — multi-port groups are flattened to scalar ports
        // named {pfx}{i}_{signal} (e.g. read0_addr, read1_addr) so that each
        // can be connected individually via a `connect` statement.
        let mut all_ports: Vec<String> = Vec::new();
        for p in &r.ports {
            let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
            let ty_str = self.emit_port_type_str(&p.ty);
            all_ports.push(format!("{dir} {ty_str} {}", p.name.name));
        }
        // Read ports — one set of scalars per port index
        if let Some(rp) = &r.read_ports {
            let pfx = &rp.name.name;
            for i in 0..nread {
                for s in &rp.signals {
                    let dir = match s.direction { Direction::In => "input", Direction::Out => "output" };
                    let flat_name = if nread == 1 {
                        format!("{pfx}_{}", s.name.name)
                    } else {
                        format!("{pfx}{i}_{}", s.name.name)
                    };
                    let port_str = self.emit_regfile_port_scalar(dir, &s.ty, &flat_name, addr_width, &data_width_num);
                    all_ports.push(port_str);
                }
            }
        }
        // Write ports — one set of scalars per port index
        if let Some(wp) = &r.write_ports {
            let pfx = &wp.name.name;
            for i in 0..nwrite {
                for s in &wp.signals {
                    let dir = match s.direction { Direction::In => "input", Direction::Out => "output" };
                    let flat_name = if nwrite == 1 {
                        format!("{pfx}_{}", s.name.name)
                    } else {
                        format!("{pfx}{i}_{}", s.name.name)
                    };
                    let port_str = self.emit_regfile_port_scalar(dir, &s.ty, &flat_name, addr_width, &data_width_num);
                    all_ports.push(port_str);
                }
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

        // ── Register array ────────────────────────────────────────────────────
        self.line(&format!("logic [{}] rf_data [0:NREGS-1];", Self::fold_width_str(&data_width_num)));
        self.line("");

        // ── Determine read/write port signal names (flat) ─────────────────────
        let write_pfx = r.write_ports.as_ref().map(|wp| wp.name.name.clone()).unwrap_or_else(|| "write".to_string());
        let read_pfx  = r.read_ports.as_ref().map(|rp| rp.name.name.clone()).unwrap_or_else(|| "read".to_string());

        // Flat name helper: "{pfx}{i}_{sig}" when count>1, else "{pfx}_{sig}"
        let flat = |pfx: &str, i: u64, count: u64, sig: &str| -> String {
            if count == 1 { format!("{pfx}_{sig}") } else { format!("{pfx}{i}_{sig}") }
        };

        // ── Write storage ─────────────────────────────────────────────────────
        // Collect init-guarded addresses: init[k]=v means addr k is immutable
        // (implemented as a write guard), not as a reset.
        let guarded_addrs: Vec<String> = r.inits.iter()
            .map(|init| self.emit_expr_str(&init.index))
            .collect();

        // Latch storage path — emit per-row `always_latch` with one-hot
        // write decoding. Saves area / power on ASIC; FPGA tools will flag
        // this at mapping (intentional — ARCH is target-agnostic).
        if r.kind == crate::ast::RegfileKind::Latch {
            let wen_in   = flat(&write_pfx, 0, nwrite, "en");
            let waddr_in = flat(&write_pfx, 0, nwrite, "addr");
            let wdata_in = flat(&write_pfx, 0, nwrite, "data");

            // `flops: internal` (Ibex-style) — regfile auto-emits its own
            // sample flops + per-row ICG-equivalent gating, so the caller can
            // drive write pins combinationally. Adds 1-cycle write latency:
            // a write asserted in cycle N is captured into rf_data[k] during
            // cycle N+1's clk-low phase. The flop->latch pipeline keeps
            // wdata stable across the latch transparency window without any
            // contract on the caller.
            let internal_flops = matches!(r.flops, crate::ast::RegfileFlops::Internal);
            let (wen_eff, waddr_eff, wdata_eff) = if internal_flops {
                let aw_msb = addr_width.saturating_sub(1);
                let dw = Self::fold_width_str(&data_width_num);
                self.line("// Internal sample flops (regfile flops: internal)");
                self.line("logic                 we_q;");
                self.line(&format!("logic [{aw_msb}:0]        waddr_q;"));
                self.line(&format!("logic [{dw}]            wdata_q;"));
                self.line(&format!("always_ff @(posedge {clk}) begin"));
                self.indent += 1;
                self.line(&format!("we_q <= {wen_in};"));
                self.line(&format!("if ({wen_in}) begin"));
                self.indent += 1;
                self.line(&format!("waddr_q <= {waddr_in};"));
                self.line(&format!("wdata_q <= {wdata_in};"));
                self.indent -= 1;
                self.line("end");
                self.indent -= 1;
                self.line("end");
                self.line("");
                ("we_q".to_string(), "waddr_q".to_string(), "wdata_q".to_string())
            } else {
                (wen_in, waddr_in, wdata_in)
            };

            for k in 0..nregs {
                let k_lit = format!("{addr_width}'d{k}");
                let init_for_k = r.inits.iter()
                    .find_map(|init| match &init.index.kind {
                        crate::ast::ExprKind::Literal(crate::ast::LitKind::Dec(v)) if *v == k => {
                            Some(self.emit_expr_str(&init.value))
                        }
                        _ => None,
                    });
                if let Some(val) = init_for_k {
                    // Address with `init[k]=v` is immutable — drive a constant.
                    self.line(&format!("assign rf_data[{k}] = {val};"));
                } else {
                    self.line(&format!("always_latch begin"));
                    self.indent += 1;
                    if internal_flops {
                        // ICG-equivalent gating: latch transparent only during
                        // clk-low of the cycle after the sample, when we_q /
                        // waddr_q / wdata_q are stable.
                        self.line(&format!("if (!{clk} && {wen_eff} && {waddr_eff} == {k_lit})"));
                    } else {
                        self.line(&format!("if ({wen_eff} && {waddr_eff} == {k_lit})"));
                    }
                    self.indent += 1;
                    self.line(&format!("rf_data[{k}] = {wdata_eff};"));
                    self.indent -= 1;
                    self.indent -= 1;
                    self.line("end");
                }
            }
            self.line("");
        } else {

        // Only include reset sensitivity when a reset port is actually present
        // and there are reset-driven init entries. For register files that use
        // write guards for x0 (not reset), emit plain posedge-only always_ff.
        let has_reset_port = !rst.is_empty();
        let use_reset = has_reset_port && r.inits.iter().any(|_| false); // reserved for future explicit reset-on-init

        let ff_sens = if use_reset {
            Self::ff_sensitivity(&clk, &rst, is_async, is_low)
        } else {
            format!("posedge {clk}")
        };

        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;

        if use_reset {
            let rst_cond = Self::rst_condition(&rst, is_low);
            self.line(&format!("if ({rst_cond}) begin"));
            self.indent += 1;
            for init in &r.inits {
                let idx = self.emit_expr_str(&init.index);
                let val = self.emit_expr_str(&init.value);
                self.line(&format!("rf_data[{idx}] <= {val};"));
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }

        // Unroll write ports; add address guards for init[k] entries
        for wi in 0..nwrite {
            let wen   = flat(&write_pfx, wi, nwrite, "en");
            let waddr = flat(&write_pfx, wi, nwrite, "addr");
            let wdata = flat(&write_pfx, wi, nwrite, "data");
            // Build guard: skip writes to any init-protected address
            let addr_guards: Vec<String> = guarded_addrs.iter()
                .map(|a| format!("{waddr} != {a}"))
                .collect();
            let guard = if addr_guards.is_empty() {
                wen.clone()
            } else {
                format!("{wen} && {}", addr_guards.join(" && "))
            };
            self.line(&format!("if ({guard})"));
            self.indent += 1;
            self.line(&format!("rf_data[{waddr}] <= {wdata};"));
            self.indent -= 1;
        }

        if use_reset {
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("end");
        self.line("");
        }  // end flop branch (else of `kind == Latch`)

        // ── Read always_comb — unrolled per read port ─────────────────────────
        self.line("always_comb begin");
        self.indent += 1;
        for ri in 0..nread {
            let raddr = flat(&read_pfx, ri, nread, "addr");
            let rdata = flat(&read_pfx, ri, nread, "data");
            if r.forward_write_before_read {
                // Forward from the first write port (write port 0)
                let wen   = flat(&write_pfx, 0, nwrite, "en");
                let waddr = flat(&write_pfx, 0, nwrite, "addr");
                let wdata = flat(&write_pfx, 0, nwrite, "data");
                self.line(&format!("if ({wen} && {waddr} == {raddr})"));
                self.indent += 1;
                self.line(&format!("{rdata} = {wdata};"));
                self.indent -= 1;
                self.line("else");
                self.indent += 1;
                self.line(&format!("{rdata} = rf_data[{raddr}];"));
                self.indent -= 1;
            } else {
                self.line(&format!("{rdata} = rf_data[{raddr}];"));
            }
        }
        self.indent -= 1;
        self.line("end");

        if !r.asserts.is_empty() {
            let clk = r.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                .map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = r.asserts.clone();
            let rname = r.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &rname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Emit a single scalar regfile port signal declaration.
    fn emit_regfile_port_scalar(
        &self,
        dir: &str,
        ty: &TypeExpr,
        name: &str,
        addr_width: u32,
        data_width: &str,
    ) -> String {
        let phy_ty = match ty {
            TypeExpr::Bool => "logic".to_string(),
            TypeExpr::Named(id) if id.name == "WIDTH" || id.name == "DATA_WIDTH" => {
                format!("logic [{}]", Self::fold_width_str(data_width))
            }
            TypeExpr::Named(id) if id.name == "ADDR_WIDTH" || id.name.to_lowercase().contains("addr") => {
                format!("logic [{}:0]", addr_width - 1)
            }
            TypeExpr::UInt(w) => {
                let ws = self.emit_expr_str(w);
                format!("logic [{}]", Self::fold_width_str(&ws))
            }
            _ => self.emit_port_type_str(ty),
        };
        format!("{dir} {phy_ty} {name}")
    }
}
