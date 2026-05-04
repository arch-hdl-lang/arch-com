//! `emit_pipeline` SV emitter (with stage / wait / expr / inst helpers) — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    fn emit_pipeline_inst(
        &mut self,
        inst: &InstDecl,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        let header = if inst.param_assigns.is_empty() {
            format!("{} {} (", inst.module_name.name, inst.name.name)
        } else {
            let params: Vec<String> = inst
                .param_assigns
                .iter()
                .map(|p| self.emit_param_override(&inst.module_name.name, p))
                .collect();
            format!(
                "{} #({}) {} (",
                inst.module_name.name,
                params.join(", "),
                inst.name.name,
            )
        };

        let connections: Vec<String> = inst
            .connections
            .iter()
            .map(|c| {
                let sig = self.emit_pipeline_stage_expr_str(
                    &c.signal, current_prefix, current_stage_idx,
                    stage_names, stage_regs, port_names,
                );
                format!(".{}({})", c.port_name.name, sig)
            })
            .collect();

        self.line(&header);
        self.indent += 1;
        for (i, conn) in connections.iter().enumerate() {
            if i < connections.len() - 1 {
                self.line(&format!("{},", conn));
            } else {
                self.line(conn);
            }
        }
        self.indent -= 1;
        self.line(");");
    }

    // ── FSM ───────────────────────────────────────────────────────────────────

    pub(crate) fn emit_pipeline(&mut self, p: &PipelineDecl) {
        self.current_construct = p.name.name.clone();
        let n = &p.name.name;

        // ── Module header ────────────────────────────────────────────────────
        if p.params.is_empty() {
            self.out.push_str(&format!("module {} (\n", n));
        } else {
            self.out.push_str(&format!("module {} #(\n", n));
            self.indent += 1;
            for (i, param) in p.params.iter().enumerate() {
                let comma = if i < p.params.len() - 1 { "," } else { "" };
                self.emit_param_decl(param, comma);
            }
            self.indent -= 1;
            self.line(") (");
        }

        self.indent += 1;
        for (i, port) in p.ports.iter().enumerate() {
            let dir = match port.direction {
                Direction::In => "input",
                Direction::Out => "output",
            };
            let ty_str = self.emit_port_type_str(&port.ty);
            let comma = if i < p.ports.len() - 1 { "," } else { "" };
            self.line(&format!("{} {} {}{}", dir, ty_str, port.name.name, comma));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");

        self.indent += 1;

        // Collect port names for name resolution
        let port_names: std::collections::HashSet<String> = p.ports.iter()
            .map(|pt| pt.name.name.clone())
            .collect();

        // Collect stage names (in order) and signal names per stage
        let stage_names: Vec<&str> = p.stages.iter().map(|s| s.name.name.as_str()).collect();

        // Build map: stage_name -> Vec<(signal_name, type_str, init_str)> for registers
        // Comb wire entries have init_str="" to distinguish from real registers.
        let mut stage_regs: Vec<Vec<(String, String, String)>> = Vec::new();
        for stage in &p.stages {
            let mut regs = Vec::new();
            for item in &stage.body {
                if let ModuleBodyItem::RegDecl(r) = item {
                    let ty_str = self.emit_logic_type_str(&r.ty);
                    let init_str = if let Some(reset_val) = Self::reset_value_expr(&r.reset) {
                        self.emit_expr_str(reset_val)
                    } else if let Some(ref init_expr) = r.init {
                        self.emit_expr_str(init_expr)
                    } else {
                        "0".to_string()
                    };
                    regs.push((r.name.name.clone(), ty_str, init_str));
                }
                // LetBindings in stages are combinational wires — add to stage_regs
                // so they get declared as `logic` and their names get stage-prefixed.
                if let ModuleBodyItem::LetBinding(l) = item {
                    let ty_str = if let Some(ref te) = l.ty {
                        self.emit_logic_type_str(te)
                    } else {
                        "logic".to_string()
                    };
                    regs.push((l.name.name.clone(), ty_str, String::new())); // empty init = comb wire
                }
            }
            stage_regs.push(regs);
        }

        // ── Stage valid registers ────────────────────────────────────────────
        self.line("// ── Stage valid registers ──");
        for sn in &stage_names {
            self.line(&format!("logic {}_valid_r;", sn.to_lowercase()));
        }
        self.line("");

        // ── Collect comb wire declarations per stage ──────────────────────────
        // Scan comb blocks for assign targets that aren't ports or regs.
        // These need explicit `logic` declarations. Type is resolved from
        // assignment sources (register or cross-stage reference).
        // Comb wires are added to stage_regs with init_str="" so name rewriting
        // automatically prefixes them.
        for (si, stage) in p.stages.iter().enumerate() {
            let mut wires: Vec<(String, String)> = Vec::new();
            for item in &stage.body {
                if let ModuleBodyItem::CombBlock(cb) = item {
                    let targets = Self::collect_comb_targets(&cb.stmts);
                    for target in targets {
                        if port_names.contains(&target) {
                            continue;
                        }
                        if stage_regs[si].iter().any(|(rn, _, _)| rn == &target) {
                            continue;
                        }
                        let ty = Self::resolve_comb_wire_type(
                            &target, &cb.stmts, si, &stage_regs, &stage_names,
                        ).unwrap_or_else(|| "logic".to_string());
                        if !wires.iter().any(|(n, _)| n == &target) {
                            wires.push((target, ty));
                        }
                    }
                }
            }
            for (name, ty) in wires {
                stage_regs[si].push((name, ty, String::new())); // empty init = comb wire
            }

            // Collect inst output connection targets as wires.
            // Resolve type by finding the register this wire is assigned to in
            // the stage's seq block (e.g. `alu_result <= alu_out` → use alu_result's type).
            for item in &stage.body {
                if let ModuleBodyItem::Inst(inst) = item {
                    for conn in &inst.connections {
                        if conn.direction != ConnectDir::Output {
                            continue;
                        }
                        if let ExprKind::Ident(target) = &conn.signal.kind {
                            if port_names.contains(target) {
                                continue;
                            }
                            if stage_regs[si].iter().any(|(rn, _, _)| rn == target) {
                                continue;
                            }
                            // Find type from the register that reads this wire
                            let ty = Self::resolve_inst_wire_type_from_consumers(
                                target, &stage.body, &stage_regs[si],
                            ).unwrap_or_else(|| "logic".to_string());
                            stage_regs[si].push((target.clone(), ty, String::new()));
                        }
                    }
                }
            }
        }

        // ── Stage data registers ─────────────────────────────────────────────
        self.line("// ── Stage data registers ──");
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();
            for (sig_name, ty_str, init_str) in &stage_regs[si] {
                if !init_str.is_empty() {
                    // Real register with initial value
                    self.line(&format!("{} {}_{} = {};", ty_str, prefix, sig_name, init_str));
                } else {
                    // Comb wire (forwarding mux, etc.)
                    self.line(&format!("{} {}_{};", ty_str, prefix, sig_name));
                }
            }
        }
        self.line("");

        // ── Detect wait-stages (variable-latency with wait until / do..until) ─
        let wait_stage_flags: Vec<bool> = p.stages.iter().map(|s| Self::stage_has_wait(s)).collect();
        let has_any_wait_stage = wait_stage_flags.iter().any(|f| *f);

        // Declare FSM state registers for wait-stages
        if has_any_wait_stage {
            self.line("// ── Wait-stage FSM registers ──");
            for (si, stage) in p.stages.iter().enumerate() {
                if !wait_stage_flags[si] { continue; }
                let prefix = stage.name.name.to_lowercase();
                let n_states = Self::count_wait_fsm_states(stage);
                let bits = crate::width::index_width(n_states as u64) as usize;
                self.line(&format!("logic [{}:0] {prefix}_fsm_state;", bits - 1));
                self.line(&format!("logic {prefix}_fsm_busy;"));
            }
            self.line("");
        }

        // ── Per-stage stall signals ──────────────────────────────────────────
        // Determine whether any stage or the pipeline has stall conditions.
        let has_per_stage_stall = p.stages.iter().any(|s| s.stall_cond.is_some());
        let has_global_stall = !p.stall_conds.is_empty();
        let has_any_stall = has_per_stage_stall || has_global_stall || has_any_wait_stage;

        if has_any_stall {
            self.line("// ── Stall signals ──");

            // Global stall (top-level `stall when`)
            if has_global_stall {
                let stall_parts: Vec<String> = p.stall_conds.iter()
                    .map(|s| self.emit_pipeline_expr_str(&s.condition, &stage_names, &stage_regs, &port_names))
                    .collect();
                self.line("logic pipeline_stall;");
                self.line(&format!("assign pipeline_stall = {};", stall_parts.join(" | ")));
            }

            // Per-stage stall wires: stall_N = local_stall_N || stall_{N+1}
            // (backpressure: downstream stall propagates upstream)
            // Last stage only has its local condition (no downstream).
            let n = p.stages.len();
            for si in 0..n {
                let prefix = stage_names[si].to_lowercase();
                self.line(&format!("logic {prefix}_stall;"));
            }

            // Build assigns in reverse order (last stage first)
            for si in (0..n).rev() {
                let prefix = stage_names[si].to_lowercase();
                let mut parts: Vec<String> = Vec::new();

                // Local stall condition from `stage X stall when <expr>`
                if let Some(ref cond) = p.stages[si].stall_cond {
                    parts.push(self.emit_pipeline_expr_str(cond, &stage_names, &stage_regs, &port_names));
                }

                // Wait-stage FSM busy signal
                if wait_stage_flags[si] {
                    let pfx = stage_names[si].to_lowercase();
                    parts.push(format!("{pfx}_fsm_busy"));
                }

                // Global stall contributes to every stage
                if has_global_stall {
                    parts.push("pipeline_stall".to_string());
                }

                // Backpressure from downstream stage
                if si + 1 < n {
                    let next_prefix = stage_names[si + 1].to_lowercase();
                    parts.push(format!("{next_prefix}_stall"));
                }

                if parts.is_empty() {
                    self.line(&format!("assign {prefix}_stall = 1'b0;"));
                } else {
                    self.line(&format!("assign {prefix}_stall = {};", parts.join(" || ")));
                }
            }
            self.line("");
        }

        // ── Wait-stage FSM busy assignments ─────────────────────────────────
        if has_any_wait_stage {
            for (si, stage) in p.stages.iter().enumerate() {
                if !wait_stage_flags[si] { continue; }
                let prefix = stage.name.name.to_lowercase();
                // FSM is busy when not in idle state (state 0)
                self.line(&format!("assign {prefix}_fsm_busy = ({prefix}_fsm_state != '0);"));
            }
            self.line("");
        }

        // ── Forward mux wires ────────────────────────────────────────────────
        for fwd in &p.forward_directives {
            let dest_str = self.emit_pipeline_expr_str(&fwd.dest, &stage_names, &stage_regs, &port_names);
            let src_str = self.emit_pipeline_expr_str(&fwd.source, &stage_names, &stage_regs, &port_names);
            let cond_str = self.emit_pipeline_expr_str(&fwd.condition, &stage_names, &stage_regs, &port_names);
            self.line(&format!("// Forward: {} from {} when {}", dest_str, src_str, cond_str));
        }
        if !p.forward_directives.is_empty() {
            self.line("");
        }

        // ── Identify clock and reset ─────────────────────────────────────────
        let clk_name = p.ports.iter()
            .find(|pt| matches!(&pt.ty, TypeExpr::Clock(_)))
            .map(|pt| pt.name.name.as_str())
            .unwrap_or("clk");
        let (rst_name, is_async, is_low) = Self::extract_reset_info(&p.ports);
        let ff_sens = Self::ff_sensitivity(clk_name, &rst_name, is_async, is_low);
        let rst_cond = Self::rst_condition(&rst_name, is_low);

        // ── always_ff block ──────────────────────────────────────────────────
        self.line("// ── Stage register updates ──");
        self.line(&format!("always_ff @({ff_sens}) begin"));
        self.indent += 1;

        // Reset branch
        self.line(&format!("if ({rst_cond}) begin"));
        self.indent += 1;
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();
            self.line(&format!("{}_valid_r <= 1'b0;", prefix));
            if wait_stage_flags[si] {
                self.line(&format!("{prefix}_fsm_state <= '0;"));
            }
            for (sig_name, _ty_str, init_str) in &stage_regs[si] {
                if !init_str.is_empty() {
                    self.line(&format!("{}_{} <= {};", prefix, sig_name, init_str));
                }
            }
        }
        self.indent -= 1;
        self.line("end else begin");
        self.indent += 1;

        // Per-stage update logic
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();

            if wait_stage_flags[si] {
                // ── Wait-stage: generate FSM transition logic ────────────
                self.emit_pipeline_wait_stage_ff(
                    stage, &prefix, si, &stage_names, &stage_regs, &port_names,
                );
            } else if has_any_stall {
                // When this stage is not stalled, it accepts new data
                self.line(&format!("if (!{prefix}_stall) begin"));
                self.indent += 1;

                // Valid propagation:
                //   If upstream is stalled, insert bubble (valid=0)
                //   Otherwise, accept upstream's valid
                if si == 0 {
                    self.line(&format!("{prefix}_valid_r <= 1'b1;"));
                } else {
                    let prev_prefix = p.stages[si - 1].name.name.to_lowercase();
                    self.line(&format!("{prefix}_valid_r <= {prev_prefix}_stall ? 1'b0 : {prev_prefix}_valid_r;"));
                }

                // Register assignments from seq blocks
                for item in &stage.body {
                    if let ModuleBodyItem::RegBlock(rb) = item {
                        for stmt in &rb.stmts {
                            self.emit_pipeline_reg_stmt(stmt, &prefix, si, &stage_names, &stage_regs, &port_names);
                        }
                    }
                }

                self.indent -= 1;
                self.line("end");
            } else {
                // No stall logic — unconditional advancement
                if si == 0 {
                    self.line(&format!("{prefix}_valid_r <= 1'b1;"));
                } else {
                    let prev_prefix = p.stages[si - 1].name.name.to_lowercase();
                    self.line(&format!("{prefix}_valid_r <= {prev_prefix}_valid_r;"));
                }

                for item in &stage.body {
                    if let ModuleBodyItem::RegBlock(rb) = item {
                        for stmt in &rb.stmts {
                            self.emit_pipeline_reg_stmt(stmt, &prefix, si, &stage_names, &stage_regs, &port_names);
                        }
                    }
                }
            }
        }

        // Flush overrides
        for flush in &p.flush_directives {
            let target_prefix = flush.target_stage.name.to_lowercase();
            let cond_str = self.emit_pipeline_expr_str(&flush.condition, &stage_names, &stage_regs, &port_names);
            self.line(&format!("if ({}) begin", cond_str));
            self.indent += 1;
            self.line(&format!("{}_valid_r <= 1'b0;", target_prefix));
            // Reset FSM state on flush for wait-stages
            let flush_si = stage_names.iter().position(|n| n.to_lowercase() == target_prefix);
            if let Some(si) = flush_si {
                if wait_stage_flags[si] {
                    self.line(&format!("{target_prefix}_fsm_state <= '0;"));
                }
                // `flush ... clear`: also reset every data reg in the
                // target stage to its declared reset value. Comb wires
                // (init_str empty) are skipped — they're not registers.
                if flush.clear {
                    for (sig_name, _ty, init_str) in &stage_regs[si] {
                        if init_str.is_empty() { continue; }
                        self.line(&format!("{target_prefix}_{sig_name} <= {init_str};"));
                    }
                }
            }
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("end");

        self.indent -= 1;
        self.line("end");
        self.line("");

        // ── Combinational outputs ────────────────────────────────────────────
        self.line("// ── Combinational outputs ──");
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = stage.name.name.to_lowercase();
            for item in &stage.body {
                if let ModuleBodyItem::CombBlock(cb) = item {
                    let all_simple = cb.stmts.iter().all(|s| matches!(s, Stmt::Assign(_)));
                    if all_simple {
                        for stmt in &cb.stmts {
                            if let Stmt::Assign(a) = stmt {
                                let val = self.emit_pipeline_stage_expr_str(&a.value, &prefix, si, &stage_names, &stage_regs, &port_names);
                                let target = if let ExprKind::Ident(name) = &a.target.kind {
                                    if port_names.contains(name) {
                                        name.clone()
                                    } else {
                                        format!("{}_{}", prefix, name)
                                    }
                                } else {
                                    self.emit_expr_str(&a.target)
                                };
                                self.line(&format!("assign {} = {};", target, val));
                            }
                        }
                    } else {
                        // Use always_comb for blocks with if/else or match
                        self.line("always_comb begin");
                        self.indent += 1;
                        for stmt in &cb.stmts {
                            self.emit_pipeline_comb_stmt(stmt, &prefix, si, &stage_names, &stage_regs, &port_names);
                        }
                        self.indent -= 1;
                        self.line("end");
                    }
                }
                if let ModuleBodyItem::LetBinding(l) = item {
                    let val = self.emit_pipeline_stage_expr_str(&l.value, &prefix, si, &stage_names, &stage_regs, &port_names);
                    self.line(&format!("assign {}_{} = {};", prefix, l.name.name, val));
                }
                if let ModuleBodyItem::Inst(inst) = item {
                    self.emit_pipeline_inst(inst, &prefix, si, &stage_names, &stage_regs, &port_names);
                }
            }
        }

        // ── Assert / cover SVA ───────────────────────────────────────────────
        if !p.asserts.is_empty() {
            let clk = p.ports.iter().find(|pt| matches!(&pt.ty, TypeExpr::Clock(_)))
                .map(|pt| pt.name.name.clone()).unwrap_or_else(|| "clk".to_string());
            self.line("");
            let asserts = p.asserts.clone();
            let pname = p.name.name.clone();
            self.emit_asserts_for_construct(&asserts, &pname, &clk);
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
        // Skip comments that fall within the pipeline body — they were already
        // incorporated into the SV output or are not meaningful after codegen.
        while self.comment_idx < self.comments.len()
            && self.comments[self.comment_idx].0.start < p.span.end
        {
            self.comment_idx += 1;
        }
    }

    // ── Pipeline wait-stage helpers ─────────────────────────────────────────

    /// Check if a pipeline stage contains `wait until` or `do..until` in its seq block.
    fn stage_has_wait(stage: &StageDecl) -> bool {
        stage.body.iter().any(|item| {
            if let ModuleBodyItem::RegBlock(rb) = item {
                Self::stmts_contain_wait(&rb.stmts)
            } else {
                false
            }
        })
    }

    fn stmts_contain_wait(stmts: &[Stmt]) -> bool {
        stmts.iter().any(|s| match s {
            Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => true,
            Stmt::IfElse(ie) => Self::stmts_contain_wait(&ie.then_stmts) || Self::stmts_contain_wait(&ie.else_stmts),
            Stmt::For(f) => Self::stmts_contain_wait(&f.body),
            _ => false,
        })
    }

    /// Count FSM states needed for a wait-stage.
    /// State 0 = idle. Each `wait until` / `do..until` adds one wait state.
    /// Pre-wait assigns and trailing assigns are merged into adjacent states.
    fn count_wait_fsm_states(stage: &StageDecl) -> usize {
        let mut wait_count = 0;
        for item in &stage.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                for s in &rb.stmts {
                    match s {
                        Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => { wait_count += 1; }
                        _ => {}
                    }
                }
            }
        }
        wait_count + 1 // +1 for idle state 0
    }

    /// Emit the always_ff logic for a wait-stage: FSM transitions + register updates.
    ///
    /// State 0 is idle: checks upstream valid, fast-paths if wait condition already met.
    /// Wait states loop until their condition is satisfied, then advance.
    /// Trailing assigns execute when the last wait condition fires, returning to idle.
    fn emit_pipeline_wait_stage_ff(
        &mut self,
        stage: &StageDecl,
        prefix: &str,
        si: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        // Collect seq stmts from the stage's RegBlock
        let mut seq_stmts: &[Stmt] = &[];
        for item in &stage.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                seq_stmts = &rb.stmts;
                break;
            }
        }

        // Partition into groups: [pre-wait assigns, wait, post-wait assigns, wait, ...]
        // Each wait creates a wait-state. Pre-wait assigns execute on entry.
        // Trailing assigns execute when the last wait completes.
        struct WaitGroup<'a> {
            pre_assigns: Vec<&'a Stmt>,   // assigns before the wait
            cond: &'a Expr,               // wait condition
            hold_assigns: Vec<&'a Stmt>,  // do..until body (empty for wait until)
        }

        let mut groups: Vec<WaitGroup> = Vec::new();
        let mut cur_assigns: Vec<&Stmt> = Vec::new();

        for stmt in seq_stmts {
            match stmt {
                Stmt::WaitUntil(cond, _) => {
                    groups.push(WaitGroup {
                        pre_assigns: std::mem::take(&mut cur_assigns),
                        cond,
                        hold_assigns: Vec::new(),
                    });
                }
                Stmt::DoUntil { body, cond, .. } => {
                    groups.push(WaitGroup {
                        pre_assigns: std::mem::take(&mut cur_assigns),
                        cond,
                        hold_assigns: body.iter().collect(),
                    });
                }
                other => {
                    cur_assigns.push(other);
                }
            }
        }
        let trailing = std::mem::take(&mut cur_assigns);

        let upstream_valid = if si > 0 {
            format!("{}_valid_r", stage_names[si - 1].to_lowercase())
        } else {
            "1'b1".to_string()
        };

        // For each wait group, we generate:
        //   - State N (wait): loops checking condition; on true, runs trailing/next pre-assigns
        // State 0 is special: checks upstream valid, fast-paths.
        // Total states = 1 (idle) + number of wait groups
        let n_states = 1 + groups.len();
        let bits = crate::width::index_width(n_states as u64) as usize;

        self.line(&format!("// Wait-stage FSM: {prefix}"));
        self.line(&format!("case ({prefix}_fsm_state)"));
        self.indent += 1;

        // State 0: idle — check upstream valid, optionally fast-path first wait
        self.line(&format!("{bits}'d0: begin"));
        self.indent += 1;
        if let Some(g) = groups.first() {
            let cond = self.emit_pipeline_stage_expr_str(g.cond, prefix, si, stage_names, stage_regs, port_names);

            // Run pre-assigns (fire once on entry to the wait)
            // For state 0 these only fire when upstream has valid data
            self.line(&format!("if ({upstream_valid}) begin"));
            self.indent += 1;
            for a in &g.pre_assigns {
                self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
            }
            // Fast path: condition already met
            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;
            if groups.len() == 1 {
                // Only one wait group: run trailing assigns and stay idle
                for a in &trailing {
                    self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
                }
                // Propagate valid
                self.line(&format!("{prefix}_valid_r <= {upstream_valid};"));
            } else {
                // Advance to next wait state
                self.line(&format!("{prefix}_fsm_state <= {bits}'d2;"));
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            // Slow path: enter wait state 1
            self.line(&format!("{prefix}_fsm_state <= {bits}'d1;"));
            for a in &g.hold_assigns {
                self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
            }
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");
        }
        self.indent -= 1;
        self.line("end");

        // States 1..N: wait states (one per wait group)
        for (gi, g) in groups.iter().enumerate() {
            let state_num = gi + 1;
            self.line(&format!("{bits}'d{state_num}: begin"));
            self.indent += 1;

            let cond = self.emit_pipeline_stage_expr_str(g.cond, prefix, si, stage_names, stage_regs, port_names);

            // Emit hold assigns (for do..until, every cycle)
            for a in &g.hold_assigns {
                self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
            }

            self.line(&format!("if ({cond}) begin"));
            self.indent += 1;

            let is_last = gi + 1 >= groups.len();
            if is_last {
                // Last wait: run trailing assigns, return to idle
                for a in &trailing {
                    self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
                }
                self.line(&format!("{prefix}_fsm_state <= '0;"));
                self.line(&format!("{prefix}_valid_r <= 1'b1;"));
            } else {
                // Not last: run next group's pre-assigns, advance to next wait state
                let next_g = &groups[gi + 1];
                for a in &next_g.pre_assigns {
                    self.emit_pipeline_reg_stmt(a, prefix, si, stage_names, stage_regs, port_names);
                }
                self.line(&format!("{prefix}_fsm_state <= {bits}'d{};", state_num + 1));
            }

            self.indent -= 1;
            self.line("end");

            self.indent -= 1;
            self.line("end");
        }

        // Default case
        self.line("default: begin");
        self.indent += 1;
        self.line(&format!("{prefix}_fsm_state <= '0;"));
        self.indent -= 1;
        self.line("end");

        self.indent -= 1;
        self.line("endcase");
    }

    /// Emit a register statement with pipeline name rewriting.
    fn emit_pipeline_reg_stmt(
        &mut self,
        stmt: &Stmt,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        match stmt {
            Stmt::Assign(a) => {
                let target = self.emit_pipeline_lhs_str(&a.target, current_prefix, port_names);
                let val = self.emit_pipeline_stage_expr_str(&a.value, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                self.line(&format!("{} <= {};", target, val));
            }
            Stmt::IfElse(ie) => {
                self.emit_pipeline_reg_if_else(ie, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, false);
            }
            Stmt::Match(_) => {
                // MVP: basic pipeline doesn't need match in seq blocks
            }
            Stmt::Log(l) => { self.emit_log_stmt(l); }
            Stmt::For(f) => {
                let var = &f.var.name;
                match &f.range {
                    ForRange::Range(rs, re) => {
                        let start = self.emit_expr_str(rs);
                        let end = self.emit_expr_str(re);
                        self.line(&format!("for (int {var} = {start}; {var} <= {end}; {var}++) begin"));
                        self.indent += 1;
                        for s in &f.body { self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names); }
                        self.indent -= 1;
                        self.line("end");
                    }
                    ForRange::ValueList(vals) => {
                        for v in vals {
                            let val = self.emit_expr_str(v);
                            self.line(&format!("for (int {var} = {val}; {var} == {val}; {var}++) begin"));
                            self.indent += 1;
                            for s in &f.body { self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names); }
                            self.indent -= 1;
                            self.line("end");
                        }
                    }
                }
            }
            Stmt::Init(_) => unreachable!("Stmt::Init should not appear in pipeline reg stmt context"),
            Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => {
                // Pipeline wait-stages handled separately by pipeline codegen
                unreachable!("WaitUntil/DoUntil handled by pipeline stage codegen, not emit_pipeline_reg_stmt")
            }
        }
    }

    /// Rewrite a LHS expression (assignment target) with pipeline prefixing.
    fn emit_pipeline_lhs_str(
        &self,
        expr: &Expr,
        current_prefix: &str,
        port_names: &std::collections::HashSet<String>,
    ) -> String {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if port_names.contains(name) {
                    name.clone()
                } else {
                    format!("{}_{}", current_prefix, name)
                }
            }
            _ => self.emit_expr_str(expr),
        }
    }

    /// Collect all unique comb assign targets from a list of comb statements (recursive).
    fn collect_comb_targets(stmts: &[Stmt]) -> Vec<String> {
        let mut targets = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    if let ExprKind::Ident(name) = &a.target.kind {
                        if !targets.contains(name) {
                            targets.push(name.clone());
                        }
                    }
                }
                Stmt::IfElse(ie) => {
                    for t in Self::collect_comb_targets(&ie.then_stmts) {
                        if !targets.contains(&t) { targets.push(t); }
                    }
                    for t in Self::collect_comb_targets(&ie.else_stmts) {
                        if !targets.contains(&t) { targets.push(t); }
                    }
                }
                Stmt::Match(_) | Stmt::Log(_) => {}
                Stmt::For(f) => {
                    for s in &f.body {
                        if let Stmt::Assign(a) = s {
                            if let ExprKind::Ident(name) = &a.target.kind {
                                if !targets.contains(name) { targets.push(name.clone()); }
                            }
                        }
                    }
                }
                Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                    unreachable!("seq-only Stmt variant inside comb-context walker");
                }
            }
        }
        targets
    }

    /// Resolve the type of an inst output wire by finding which register reads it
    /// in the stage's seq block (e.g. `alu_result <= alu_out` → use alu_result's type).
    fn resolve_inst_wire_type_from_consumers(
        wire_name: &str,
        body: &[ModuleBodyItem],
        regs: &[(String, String, String)],
    ) -> Option<String> {
        for item in body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                for stmt in &rb.stmts {
                    if let Stmt::Assign(a) = stmt {
                        // Check if RHS references the wire name
                        if let ExprKind::Ident(rhs) = &a.value.kind {
                            if rhs == wire_name {
                                // LHS is the register — find its type
                                if let ExprKind::Ident(lhs) = &a.target.kind {
                                    if let Some(r) = regs.iter().find(|(rn, _, _)| rn == lhs) {
                                        return Some(r.1.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Resolve the type of a comb wire by inspecting assignment sources.
    /// Looks for known registers (local or cross-stage) in assignment RHS.
    fn resolve_comb_wire_type(
        target: &str,
        stmts: &[Stmt],
        current_stage_idx: usize,
        stage_regs: &[Vec<(String, String, String)>],
        stage_names: &[&str],
    ) -> Option<String> {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) if matches!(&a.target.kind, ExprKind::Ident(n) if n == target) => {
                    // Check if RHS is a bare identifier (local register)
                    if let ExprKind::Ident(name) = &a.value.kind {
                        if let Some(r) = stage_regs[current_stage_idx].iter()
                            .find(|(rn, _, _)| rn == name)
                        {
                            return Some(r.1.clone());
                        }
                    }
                    // Check if RHS is a cross-stage reference: Stage.signal
                    if let ExprKind::FieldAccess(base, field) = &a.value.kind {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            if let Some(si) = stage_names.iter().position(|&sn| sn == base_name) {
                                if let Some(r) = stage_regs[si].iter()
                                    .find(|(rn, _, _)| rn == &field.name)
                                {
                                    return Some(r.1.clone());
                                }
                            }
                        }
                    }
                }
                Stmt::IfElse(ie) => {
                    if let Some(ty) = Self::resolve_comb_wire_type(target, &ie.then_stmts, current_stage_idx, stage_regs, stage_names) {
                        return Some(ty);
                    }
                    if let Some(ty) = Self::resolve_comb_wire_type(target, &ie.else_stmts, current_stage_idx, stage_regs, stage_names) {
                        return Some(ty);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Emit a comb statement within a pipeline stage context (inside always_comb).
    /// Handles Assign, IfElse with pipeline name rewriting.
    fn emit_pipeline_comb_stmt(
        &mut self,
        stmt: &Stmt,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) {
        match stmt {
            Stmt::Assign(a) => {
                let val = self.emit_pipeline_stage_expr_str(&a.value, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let target = if let ExprKind::Ident(name) = &a.target.kind {
                    if port_names.contains(name) {
                        name.clone()
                    } else {
                        format!("{}_{}", current_prefix, name)
                    }
                } else {
                    self.emit_expr_str(&a.target)
                };
                self.line(&format!("{} = {};", target, val));
            }
            Stmt::IfElse(ie) => {
                self.emit_pipeline_comb_if_else(ie, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, false);
            }
            Stmt::Match(_) => {} // TODO if needed
            Stmt::Log(l) => { self.emit_log_stmt(l); }
            Stmt::For(f) => {
                self.emit_for_loop_sv(f, |s, stmt| {
                    s.emit_pipeline_comb_stmt(stmt, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                });
            }
                Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => unreachable!("seq-only Stmt variant inside comb-context walker"),
        }
    }

    fn emit_pipeline_reg_if_else(
        &mut self,
        ie: &IfElse,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
        is_chain: bool,
    ) {
        let cond = self.emit_pipeline_stage_expr_str(&ie.cond, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("if ({}) begin", cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_pipeline_reg_if_else(nested, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_pipeline_reg_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    fn emit_pipeline_comb_if_else(
        &mut self,
        ie: &IfElse,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
        is_chain: bool,
    ) {
        let cond = self.emit_pipeline_stage_expr_str(&ie.cond, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("if ({}) begin", cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_pipeline_comb_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_pipeline_comb_if_else(nested, current_prefix, current_stage_idx, stage_names, stage_regs, port_names, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_pipeline_comb_stmt(s, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    /// Emit an expression within a specific stage context (knows which stage it's in,
    /// so bare identifiers that are stage registers get prefixed).
    fn emit_pipeline_stage_expr_str(
        &self,
        expr: &Expr,
        current_prefix: &str,
        current_stage_idx: usize,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) -> String {
        match &expr.kind {
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if let Some(si) = stage_names.iter().position(|&sn| sn == base_name) {
                        let prefix = stage_names[si].to_lowercase();
                        return format!("{}_{}", prefix, field.name);
                    }
                }
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("{}.{}", b, field.name)
            }
            ExprKind::Ident(name) => {
                if port_names.contains(name) {
                    return name.clone();
                }
                // Check if it's a register in the current stage
                if let Some(regs) = stage_regs.get(current_stage_idx) {
                    if regs.iter().any(|(rn, _, _)| rn == name) {
                        return format!("{}_{}", current_prefix, name);
                    }
                }
                // Compiler-generated stage signals (valid_r)
                if name == "valid_r" {
                    return format!("{}_valid_r", current_prefix);
                }
                name.clone()
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let l = self.emit_pipeline_stage_expr_str(lhs, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let r = self.emit_pipeline_stage_expr_str(rhs, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                if *op == BinOp::Implies {
                    return format!("({l} |-> {r})");
                }
                if *op == BinOp::ImpliesNext {
                    return format!("({l} |=> {r})");
                }
                let op_str = match op {
                    BinOp::Add | BinOp::AddWrap => "+", BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*",
                    BinOp::Div => "/", BinOp::Mod => "%", BinOp::Eq => "==",
                    BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "&&",
                    BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::Shl => "<<", BinOp::Shr => ">>",
                    BinOp::Implies | BinOp::ImpliesNext => unreachable!(),
                };
                if matches!(op, BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap) {
                    let lw = self.infer_sv_width_str(lhs);
                    let rw = self.infer_sv_width_str(rhs);
                    let w = if lw == rw { lw } else { format!("({lw} > {rw} ? {lw} : {rw})") };
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({l} {op_str} {r})")
                } else {
                    format!("({l} {op_str} {r})")
                }
            }
            ExprKind::Unary(op, operand) => {
                let o = self.emit_pipeline_stage_expr_str(operand, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match op {
                    UnaryOp::Not => format!("(!{o})"),
                    UnaryOp::BitNot => format!("(~{o})"),
                    UnaryOp::Neg => format!("(-{o})"),
                    UnaryOp::RedAnd => format!("(&{o})"),
                    UnaryOp::RedOr => format!("(|{o})"),
                    UnaryOp::RedXor => format!("(^{o})"),
                }
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match method.name.as_str() {
                    "trunc" | "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            format!("{wp}'({b})")
                        } else {
                            b
                        }
                    }
                    "sext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{{{{({w}-$bits({b})){{{b}[$bits({b})-1]}}}}, {b}}}")
                        } else {
                            b
                        }
                    }
                    "resize" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            if self.expr_is_signed(base) {
                                format!("{wp}'($signed({b}))")
                            } else {
                                format!("{wp}'($unsigned({b}))")
                            }
                        } else {
                            b
                        }
                    }
                    "reverse" => {
                        if let Some(chunk) = args.first() {
                            let c = self.emit_expr_str(chunk);
                            format!("{{<<{c}{{{b}}}}}")
                        } else {
                            b
                        }
                    }
                    "any" | "all" | "count" | "contains"
                    | "reduce_or" | "reduce_and" | "reduce_xor"
                    | "find_first" => {
                        self.emit_vec_method(&b, base, method, args)
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let i = self.emit_pipeline_stage_expr_str(idx, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("{b}[{i}]")
            }
            ExprKind::BitSlice(base, hi, lo) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                if let Some(width) = Self::try_indexed_part_select(hi, lo) {
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{l} +: {width}]")
                } else {
                    let h = self.emit_expr_str(hi);
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{h}:{l}]")
                }
            }
            ExprKind::PartSelect(base, start, width, up) => {
                let b = self.emit_pipeline_stage_expr_str(base, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let s = self.emit_expr_str(start);
                let w = self.emit_expr_str(width);
                let op = if *up { "+:" } else { "-:" };
                format!("{b}[{s} {op} {w}]")
            }
            ExprKind::Concat(parts) => {
                let parts_str: Vec<String> = parts.iter()
                    .map(|p| self.emit_pipeline_stage_expr_str(p, current_prefix, current_stage_idx, stage_names, stage_regs, port_names))
                    .collect();
                format!("{{{}}}", parts_str.join(", "))
            }
            ExprKind::Cast(inner, ty) => {
                let e = self.emit_pipeline_stage_expr_str(inner, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                match &**ty {
                    TypeExpr::SInt(_) => format!("$signed({e})"),
                    TypeExpr::UInt(w) => {
                        let ws = self.emit_expr_str(w);
                        format!("{ws}'($unsigned({e}))")
                    }
                    _ => {
                        let t = self.emit_type_str(ty);
                        format!("{t}'({e})")
                    }
                }
            }
            ExprKind::Signed(inner) => {
                let e = self.emit_pipeline_stage_expr_str(inner, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("$signed({e})")
            }
            ExprKind::Unsigned(inner) => {
                let e = self.emit_pipeline_stage_expr_str(inner, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("$unsigned({e})")
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                let c = self.emit_pipeline_stage_expr_str(cond, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let t = self.emit_pipeline_stage_expr_str(then_expr, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                let e = self.emit_pipeline_stage_expr_str(else_expr, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("({c}) ? ({t}) : ({e})")
            }
            ExprKind::Bool(b) => if *b { "1'b1".to_string() } else { "1'b0".to_string() },
            ExprKind::Clog2(arg) => {
                let a = self.emit_pipeline_stage_expr_str(arg, current_prefix, current_stage_idx, stage_names, stage_regs, port_names);
                format!("$clog2({a})")
            }
            _ => self.emit_expr_str(expr),
        }
    }

    /// Emit an expression with pipeline name rewriting:
    /// - `Stage.signal` → `stage_signal`
    /// - Bare signal in stage context → preserved (caller handles prefix)
    /// - Port names → kept as-is
    fn emit_pipeline_expr_str(
        &self,
        expr: &Expr,
        stage_names: &[&str],
        stage_regs: &[Vec<(String, String, String)>],
        port_names: &std::collections::HashSet<String>,
    ) -> String {
        match &expr.kind {
            ExprKind::FieldAccess(base, field) => {
                // Check if base is a stage name → rewrite to stage_signal
                if let ExprKind::Ident(base_name) = &base.kind {
                    if let Some(si) = stage_names.iter().position(|&sn| sn == base_name) {
                        let prefix = stage_names[si].to_lowercase();
                        return format!("{}_{}", prefix, field.name);
                    }
                }
                // Otherwise use default emission
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                format!("{}.{}", b, field.name)
            }
            ExprKind::Ident(name) => {
                // Port names stay as-is
                if port_names.contains(name) {
                    return name.clone();
                }
                // Check if it's a stage name itself (shouldn't appear bare normally)
                // Otherwise it's a local — keep as-is (the caller adds prefix if needed)
                name.clone()
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let l = self.emit_pipeline_expr_str(lhs, stage_names, stage_regs, port_names);
                let r = self.emit_pipeline_expr_str(rhs, stage_names, stage_regs, port_names);
                if *op == BinOp::Implies {
                    return format!("({l} |-> {r})");
                }
                if *op == BinOp::ImpliesNext {
                    return format!("({l} |=> {r})");
                }
                let op_str = match op {
                    BinOp::Add | BinOp::AddWrap => "+", BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*",
                    BinOp::Div => "/", BinOp::Mod => "%", BinOp::Eq => "==",
                    BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "&&",
                    BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::Shl => "<<", BinOp::Shr => ">>",
                    BinOp::Implies | BinOp::ImpliesNext => unreachable!(),
                };
                if matches!(op, BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap) {
                    let lw = self.infer_sv_width_str(lhs);
                    let rw = self.infer_sv_width_str(rhs);
                    let w = if lw == rw { lw } else { format!("({lw} > {rw} ? {lw} : {rw})") };
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({l} {op_str} {r})")
                } else {
                    format!("({l} {op_str} {r})")
                }
            }
            ExprKind::Unary(op, operand) => {
                let o = self.emit_pipeline_expr_str(operand, stage_names, stage_regs, port_names);
                match op {
                    UnaryOp::Not => format!("(!{o})"),
                    UnaryOp::BitNot => format!("(~{o})"),
                    UnaryOp::Neg => format!("(-{o})"),
                    UnaryOp::RedAnd => format!("(&{o})"),
                    UnaryOp::RedOr => format!("(|{o})"),
                    UnaryOp::RedXor => format!("(^{o})"),
                }
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                match method.name.as_str() {
                    "trunc" | "zext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            format!("{wp}'({b})")
                        } else {
                            b
                        }
                    }
                    "sext" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            format!("{{{{({w}-$bits({b})){{{b}[$bits({b})-1]}}}}, {b}}}")
                        } else {
                            b
                        }
                    }
                    "resize" => {
                        if let Some(width) = args.first() {
                            let w = self.emit_expr_str(width);
                            let wp = Self::paren_width(&w);
                            if self.expr_is_signed(base) {
                                format!("{wp}'($signed({b}))")
                            } else {
                                format!("{wp}'($unsigned({b}))")
                            }
                        } else {
                            b
                        }
                    }
                    "reverse" => {
                        if let Some(chunk) = args.first() {
                            let c = self.emit_expr_str(chunk);
                            format!("{{<<{c}{{{b}}}}}")
                        } else {
                            b
                        }
                    }
                    "any" | "all" | "count" | "contains"
                    | "reduce_or" | "reduce_and" | "reduce_xor"
                    | "find_first" => {
                        self.emit_vec_method(&b, base, method, args)
                    }
                    _ => format!("{b}.{}()", method.name),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                let i = self.emit_pipeline_expr_str(idx, stage_names, stage_regs, port_names);
                format!("{b}[{i}]")
            }
            ExprKind::BitSlice(base, hi, lo) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                if let Some(width) = Self::try_indexed_part_select(hi, lo) {
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{l} +: {width}]")
                } else {
                    let h = self.emit_expr_str(hi);
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{h}:{l}]")
                }
            }
            ExprKind::PartSelect(base, start, width, up) => {
                let b = self.emit_pipeline_expr_str(base, stage_names, stage_regs, port_names);
                let s = self.emit_expr_str(start);
                let w = self.emit_expr_str(width);
                let op = if *up { "+:" } else { "-:" };
                format!("{b}[{s} {op} {w}]")
            }
            // For everything else, fall back to regular emit
            _ => self.emit_expr_str(expr),
        }
    }

    // ── FIFO ──────────────────────────────────────────────────────────────────

}
