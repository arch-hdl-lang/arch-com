//! `emit_fsm` SV emitter — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_fsm(&mut self, f: &FsmDecl) {
        self.current_construct = f.name.name.clone();
        // Built-in `state` identifier inside fsm scope: read of the current
        // encoded state register. SV emission lowers to `state_r` (the enum
        // register), which implicitly casts to the user-declared output port
        // width. Cleared at the end of emit_fsm.
        self.ident_subst.insert("state".to_string(), "state_r".to_string());
        let n = f.name.name.clone();
        let n_states = f.state_names.len();
        let state_bits = enum_width(n_states);

        // ── Module header ────────────────────────────────────────────────────
        if f.params.is_empty() {
            self.line(&format!("module {n} ("));
        } else {
            self.line(&format!("module {n} #("));
            self.indent += 1;
            for (i, p) in f.params.iter().enumerate() {
                let comma = if i < f.params.len() - 1 { "," } else { "" };
                self.emit_param_decl(p, comma);
            }
            self.indent -= 1;
            self.line(") (");
        }
        // Collect all port lines (bus ports are flattened, same as module codegen)
        self.bus_ports.clear();
        let mut port_lines: Vec<String> = Vec::new();
        for p in f.ports.iter() {
            if let Some(ref bi) = p.bus_info {
                let bus_name = &bi.bus_name.name;
                self.bus_ports.insert(p.name.name.clone(), bus_name.clone());
                if let Some((crate::resolve::Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name) {
                    let mut param_map: std::collections::HashMap<String, &Expr> = info.params.iter()
                        .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                        .collect();
                    for pa in &bi.params {
                        param_map.insert(pa.name.name.clone(), &pa.value);
                    }
                    let eff_signals = info.effective_signals(&param_map);
                    for (sname, sdir, sty) in &eff_signals {
                        let actual_dir = match bi.perspective {
                            BusPerspective::Initiator => *sdir,
                            BusPerspective::Target => (*sdir).flip(),
                        };
                        let dir_str = match actual_dir {
                            Direction::In => "input",
                            Direction::Out => "output",
                        };
                        let subst_ty = Self::subst_type_expr(sty, &param_map);
                        let ty_str = self.emit_port_type_str(&subst_ty);
                        port_lines.push(format!("{} {} {}_{}", dir_str, ty_str, p.name.name, sname));
                    }
                }
            } else {
                let dir = match p.direction { Direction::In => "input", Direction::Out => "output" };
                if let TypeExpr::Vec(_, _) = &p.ty {
                    let (base_ty, suffix) = self.emit_type_and_array_suffix(&p.ty);
                    port_lines.push(format!("{dir} {base_ty} {}{suffix}", p.name.name));
                } else {
                    let ty = self.emit_port_type_str(&p.ty);
                    port_lines.push(format!("{dir} {ty} {}", p.name.name));
                }
            }
        }
        self.indent += 1;
        for (i, line) in port_lines.iter().enumerate() {
            let comma = if i < port_lines.len() - 1 { "," } else { "" };
            self.line(&format!("{line}{comma}"));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");
        self.indent += 1;

        // ── State type ───────────────────────────────────────────────────────
        self.line(&format!("typedef enum logic [{}:0] {{", state_bits.saturating_sub(1)));
        self.indent += 1;
        for (i, sn) in f.state_names.iter().enumerate() {
            let comma = if i < f.state_names.len() - 1 { "," } else { "" };
            self.line(&format!("{} = {state_bits}'d{i}{comma}", sn.name.to_uppercase()));
        }
        self.indent -= 1;
        self.line(&format!("}} {n}_state_t;"));
        self.line("");

        // ── State register ───────────────────────────────────────────────────
        self.line(&format!("{n}_state_t state_r, state_next;"));
        self.line("");

        // ── Datapath register declarations ───────────────────────────────────
        for reg in &f.regs {
            let (ty, arr_suffix) = self.emit_type_and_array_suffix(&reg.ty);
            self.line(&format!("{ty} {}{arr_suffix};", reg.name.name));
        }
        if !f.regs.is_empty() {
            self.line("");
        }

        // ── Let wire declarations ────────────────────────────────────────────
        let port_names_in_fsm: std::collections::HashSet<&str> =
            f.ports.iter().map(|p| p.name.name.as_str()).collect();
        for lb in &f.lets {
            let val = self.emit_expr_str(&lb.value);
            let aliases_port = lb.ty.is_none() && port_names_in_fsm.contains(lb.name.name.as_str());
            if !aliases_port {
                let ty = if let Some(t) = &lb.ty {
                    self.emit_type_str(t)
                } else {
                    "logic".to_string()
                };
                self.line(&format!("{ty} {};", lb.name.name));
            }
            self.line(&format!("assign {} = {};", lb.name.name, val));
        }
        if !f.lets.is_empty() {
            self.line("");
        }

        // ── Wire declarations ───────────────────────────────────────────────
        for w in &f.wires {
            let (ty, arr_suffix) = self.emit_type_and_array_suffix(&w.ty);
            self.line(&format!("{ty} {}{arr_suffix};", w.name.name));
        }
        if !f.wires.is_empty() {
            self.line("");
        }

        // Identify clock and reset port names
        let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
        let clk_name = clk_port.map(|p| p.name.name.as_str()).unwrap_or("clk");
        let (rst_name, is_async, is_low) = Self::extract_reset_info(&f.ports);
        let ff_sens = Self::ff_sensitivity(clk_name, &rst_name, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst_name, is_low);

        // ── State register FF ────────────────────────────────────────────────
        let has_seq = f.states.iter().any(|s| !s.seq_stmts.is_empty());
        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        self.line(&format!("state_r <= {};", f.default_state.name.to_uppercase()));
        // Reset datapath registers
        for reg in &f.regs {
            let reset_expr = Self::reset_value_expr(&reg.reset)
                .or(reg.init.as_ref());
            if let Some(val_expr) = reset_expr {
                let init_str = self.emit_expr_str(val_expr);
                if let TypeExpr::Vec(_, size_expr) = &reg.ty {
                    let sz = self.emit_expr_str(size_expr);
                    let ri = format!("__ri_{}", reg.name.name);
                    self.line(&format!("for (int {ri} = 0; {ri} < {sz}; {ri}++) begin"));
                    self.indent += 1;
                    self.line(&format!("{}[{ri}] <= {init_str};", reg.name.name));
                    self.indent -= 1;
                    self.line("end");
                } else {
                    self.line(&format!("{} <= {init_str};", reg.name.name));
                }
            }
        }
        // Reset port-reg outputs (ports with reg_info)
        for p in &f.ports {
            if let Some(ri) = &p.reg_info {
                let reset_expr = Self::reset_value_expr(&ri.reset)
                    .or(ri.init.as_ref());
                if let Some(val_expr) = reset_expr {
                    let init_str = self.emit_expr_str(val_expr);
                    self.line(&format!("{} <= {init_str};", p.name.name));
                }
            }
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;
        self.line("state_r <= state_next;");
        // Default sequential assignments (before state case)
        for stmt in &f.default_seq {
            self.emit_reg_stmt(stmt);
        }
        // Per-state sequential logic
        if has_seq || !f.default_seq.is_empty() {
            self.line("case (state_r)");
            self.indent += 1;
            for sb in &f.states {
                if sb.seq_stmts.is_empty() {
                    continue;
                }
                self.line(&format!("{}: begin", sb.name.name.to_uppercase()));
                self.indent += 1;
                for stmt in &sb.seq_stmts {
                    self.emit_reg_stmt(stmt);
                }
                self.indent -= 1;
                self.line("end");
            }
            self.line("default: ;");
            self.indent -= 1;
            self.line("endcase");
        }
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Next-state logic ─────────────────────────────────────────────────
        self.line("always_comb begin");
        self.indent += 1;
        self.line("state_next = state_r; // hold by default");
        self.line("case (state_r)");
        self.indent += 1;
        for sb in &f.states {
            self.line(&format!("{}: begin", sb.name.name.to_uppercase()));
            self.indent += 1;
            let cond_strs: Vec<String> = sb.transitions.iter()
                .map(|tr| self.emit_expr_str(&tr.condition))
                .collect();
            // Single unconditional transition — emit plain assignment.
            if cond_strs.len() == 1 && (cond_strs[0] == "1'b1" || cond_strs[0] == "1") {
                self.line(&format!("state_next = {};",
                    sb.transitions[0].target.name.to_uppercase()));
            } else {
                for (i, tr) in sb.transitions.iter().enumerate() {
                    let is_true = cond_strs[i] == "1'b1" || cond_strs[i] == "1";
                    if i == 0 && is_true {
                        // First and unconditional — plain assignment
                        self.line(&format!("state_next = {};",
                            tr.target.name.to_uppercase()));
                    } else if i > 0 && is_true {
                        // Last catch-all — emit as else
                        self.line(&format!("else state_next = {};",
                            tr.target.name.to_uppercase()));
                    } else {
                        let kw = if i == 0 { "if" } else { "else if" };
                        self.line(&format!("{kw} ({}) state_next = {};",
                            cond_strs[i], tr.target.name.to_uppercase()));
                    }
                }
            }
            self.indent -= 1;
            self.line("end");
        }
        self.line("default: state_next = state_r;");
        self.indent -= 1;
        self.line("endcase");
        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Output logic ─────────────────────────────────────────────────────
        // Emit default zeros for all outputs
        let out_ports: Vec<&PortDecl> = f.ports.iter()
            .filter(|p| p.direction == Direction::Out)
            .collect();
        let has_comb = !out_ports.is_empty() || !f.default_comb.is_empty()
            || f.states.iter().any(|s| !s.comb_stmts.is_empty());
        if has_comb {
            self.line("always_comb begin");
            self.indent += 1;
            // Per-port defaults (from `port name: out T default V`). Emitted
            // before the case so any state that doesn't assign the port
            // still produces the declared default instead of latching.
            for p in &f.ports {
                if p.direction != Direction::Out { continue; }
                if p.reg_info.is_some() { continue; }
                if let Some(def) = &p.default {
                    let val = self.emit_expr_str(def);
                    self.line(&format!("{} = {};", p.name.name, val));
                }
            }
            // Default combinational assignments (from explicit `default` block)
            for stmt in &f.default_comb {
                self.emit_comb_stmt(stmt);
            }
            self.line("case (state_r)");
            self.indent += 1;
            for sb in &f.states {
                self.line(&format!("{}: begin", sb.name.name.to_uppercase()));
                self.indent += 1;
                for stmt in &sb.comb_stmts {
                    self.emit_comb_stmt(stmt);
                }
                self.indent -= 1;
                self.line("end");
            }
            self.line("default: ;");
            self.indent -= 1;
            self.line("endcase");
            self.indent -= 1;
            self.line("end");
        }

        // Auto-generated FSM safety assertions and coverage
        {
            let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
            let clk = clk_port.map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            let (rst_name, _, is_low) = Self::extract_reset_info(&f.ports);
            let rst_inactive = if is_low { rst_name.clone() } else { format!("!{rst_name}") };
            let n = &f.name.name;
            let n_states = f.state_names.len();
            self.line("");
            self.line("// synopsys translate_off");

            // Assert: no illegal state (reset-guarded)
            self.line(&format!(
                "_auto_legal_state: assert property (@(posedge {clk}) {rst_inactive} |-> state_r < {n_states})"
            ));
            self.line(&format!(
                "  else $fatal(1, \"FSM ILLEGAL STATE: {n}.state_r = %0d\", state_r);"
            ));

            // Cover: each state is reachable
            for sn in &f.state_names {
                let su = sn.name.to_uppercase();
                self.line(&format!(
                    "_auto_reach_{}: cover property (@(posedge {clk}) state_r == {su});",
                    sn.name
                ));
            }

            // Cover: each declared transition can fire
            // Use a counter to disambiguate duplicate src→tgt transitions
            {
                let mut tr_counts: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
                for sb in &f.states {
                    let src = sb.name.name.to_uppercase();
                    for tr in &sb.transitions {
                        let tgt = tr.target.name.to_uppercase();
                        let key = (src.clone(), tgt.clone());
                        let count = tr_counts.entry(key).or_insert(0);
                        let suffix = if *count > 0 { format!("_{count}") } else { String::new() };
                        *count += 1;
                        self.line(&format!(
                            "_auto_tr_{src}_to_{tgt}{suffix}: cover property (@(posedge {clk}) state_r == {src} && state_next == {tgt});",
                        ));
                    }
                }
            }

            self.line("// synopsys translate_on");
        }

        // ── Assert / cover SVA ───────────────────────────────────────────────
        if !f.asserts.is_empty() {
            let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
            let clk = clk_port.map(|p| p.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = f.asserts.clone();
            let fname = f.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &fname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
        self.ident_subst.remove("state");
    }

    // ── Pipeline ──────────────────────────────────────────────────────────────

}
