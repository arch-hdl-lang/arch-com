//! `emit_module` SV emitter and its body-item helpers — extracted from `codegen/mod.rs`.
//!
//! Lives in a child module of `codegen` so it can access `Codegen`'s
//! private fields and helpers without bumping their visibility. Mirrors
//! the per-construct submodule layout `sim_codegen/` already uses.

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_module(&mut self, m: &ModuleDecl) {
        // Interface stubs loaded from `.archi` files have no body and
        // exist only to expose the port signature to typecheck. The real
        // SV for these modules lives in a separately-built `.sv` file
        // alongside the `.archi` — emitting an empty stub here would
        // produce a duplicate module declaration that clashes with the
        // real SV at Verilator link time.
        if m.is_interface {
            return;
        }
        self.current_construct = m.name.name.clone();
        // Emit SV `import NAME::*;` only for `use NAME;` whose target is an
        // actual `package` — package contents become an SV package and need
        // the import for typedef/enum/struct visibility. Other `use` targets
        // (bus, module, fsm, ...) are pure compile-time references that the
        // ARCH compiler resolves before codegen; emitting `import` for them
        // breaks Verilator/iverilog since no corresponding SV package exists.
        for item in &self.source.items {
            if let Item::Use(u) = item {
                let is_package = self.source.items.iter().any(|i| {
                    matches!(i, Item::Package(p) if p.name.name == u.name.name)
                });
                if is_package {
                    self.out.push_str(&format!("import {}::*;\n", u.name.name));
                }
            }
        }

        // Module header with parameters
        if m.params.is_empty() {
            self.out.push_str(&format!("module {} (\n", m.name.name));
        } else {
            self.out.push_str(&format!("module {} #(\n", m.name.name));
            self.indent += 1;
            for (i, p) in m.params.iter().enumerate() {
                let comma = if i < m.params.len() - 1 { "," } else { "" };
                self.emit_param_decl(p, comma);
            }
            self.indent -= 1;
            self.line(") (");
        }

        // Ports — bus ports are flattened to individual signals
        self.bus_ports.clear();
        self.bus_wires.clear();
        self.reset_ports.clear();
        self.vec_sizes.clear();
        self.pipe_regs.clear();
        self.vec_params.clear();
        for bi in &m.body {
            if let ModuleBodyItem::PipeRegDecl(p) = bi {
                self.pipe_regs.insert(p.name.name.clone(), (p.source.name.clone(), p.stages));
            }
        }
        for p in m.params.iter() {
            if let ParamKind::ConstVec(ty) = &p.kind {
                if let TypeExpr::Vec(elem, _) = ty {
                    self.vec_params.insert(p.name.name.clone(), (**elem).clone());
                }
            }
        }
        for p in m.ports.iter() {
            if let TypeExpr::Reset(kind, level) = &p.ty {
                self.reset_ports.insert(p.name.name.clone(), (*kind, *level));
            }
            if let TypeExpr::Vec(_, size_expr) = &p.ty {
                if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                    self.vec_sizes.insert(p.name.name.clone(), n);
                }
            }
        }
        // Vec-typed regs, wires, and let bindings are also eligible receivers.
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    if let TypeExpr::Vec(_, size_expr) = &r.ty {
                        if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                            self.vec_sizes.insert(r.name.name.clone(), n);
                        }
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    if let TypeExpr::Vec(_, size_expr) = &w.ty {
                        if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                            self.vec_sizes.insert(w.name.name.clone(), n);
                        }
                    }
                }
                ModuleBodyItem::LetBinding(lb) => {
                    if let Some(TypeExpr::Vec(_, size_expr)) = &lb.ty {
                        if let Some(n) = self.eval_const_u32(size_expr, &m.params) {
                            self.vec_sizes.insert(lb.name.name.clone(), n);
                        }
                    }
                }
                _ => {}
            }
        }
        // Collect all flattened port lines first so we can add commas correctly
        let mut port_lines: Vec<String> = Vec::new();
        for p in m.ports.iter() {
            if let Some(ref bi) = p.bus_info {
                let bus_name = &bi.bus_name.name;
                self.bus_ports.insert(p.name.name.clone(), bus_name.clone());
                if let Some((crate::resolve::Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name) {
                    // Build param substitution map: start with bus defaults, override with port params
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
                let dir = match p.direction {
                    Direction::In => "input",
                    Direction::Out => "output",
                };
                // Vec types: default emission is packed multi-dim
                // (`logic [N-1:0][W-1:0] name`). When `unpacked` is set on
                // the port, switch to SV unpacked array shape
                // (`logic [W-1:0] name [N-1:0]`) for interop with external SV
                // modules whose port shape is fixed unpacked.
                if let TypeExpr::Vec(_, _) = &p.ty {
                    let init_str = p.reg_info.as_ref()
                        .and_then(|ri| ri.init.as_ref())
                        .map(|e| format!(" = {}", self.emit_expr_str(e)))
                        .unwrap_or_default();
                    if p.unpacked {
                        let (base_ty, suffix) = self.emit_type_and_unpacked_suffix(&p.ty);
                        port_lines.push(format!("{} {} {}{}{}", dir, base_ty, p.name.name, suffix, init_str));
                    } else {
                        let (base_ty, suffix) = self.emit_type_and_array_suffix(&p.ty);
                        port_lines.push(format!("{} {} {}{}{}", dir, base_ty, p.name.name, suffix, init_str));
                    }
                } else {
                    let ty_str = self.emit_port_type_str(&p.ty);
                    let init_str = p.reg_info.as_ref()
                        .and_then(|ri| ri.init.as_ref())
                        .map(|e| format!(" = {}", self.emit_expr_str(e)))
                        .unwrap_or_default();
                    port_lines.push(format!("{} {} {}{}", dir, ty_str, p.name.name, init_str));
                }
            }
        }
        self.indent += 1;
        for (i, line) in port_lines.iter().enumerate() {
            let comma = if i < port_lines.len() - 1 { "," } else { "" };
            self.line(&format!("{}{}", line, comma));
        }
        self.indent -= 1;
        self.line(");");
        self.line("");

        self.indent += 1;

        // Emit any functions defined in the same file as local `function automatic` declarations.
        let fns = std::mem::take(&mut self.pending_functions);
        for f in &fns {
            self.emit_function(f);
        }
        self.pending_functions = fns;

        // If any log() statements exist in this module, emit the per-module verbosity variable.
        // Override at simulation: +arch_verbosity=N on the simulator command line.
        if Self::module_has_log(&m.body) {
            self.line("// synopsys translate_off");
            self.line("integer _arch_verbosity = 1; // 0=Always 1=Low 2=Medium 3=High 4=Full 5=Debug");
            self.line("initial void'($value$plusargs(\"arch_verbosity=%0d\", _arch_verbosity));");
            self.line("// synopsys translate_on");
            self.line("");
        }

        // Collect names already declared as ports, regs, or lets so we can
        // auto-declare inst output wires that aren't otherwise declared.
        let mut declared_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for p in &m.ports { declared_names.insert(p.name.name.clone()); }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => { declared_names.insert(r.name.name.clone()); }
                ModuleBodyItem::LetBinding(l) => { declared_names.insert(l.name.name.clone()); }
                ModuleBodyItem::PipeRegDecl(p) => {
                    declared_names.insert(p.name.name.clone());
                    for i in 0..p.stages.saturating_sub(1) {
                        declared_names.insert(format!("{}_stg{}", p.name.name, i + 1));
                    }
                }
                ModuleBodyItem::WireDecl(w) => {
                    declared_names.insert(w.name.name.clone());
                    // For bus-typed wires, also pre-populate flattened signal
                    // names so that inst auto-wire-decl doesn't duplicate them.
                    if let TypeExpr::Named(id) = &w.ty {
                        if let Some((Symbol::Bus(info), _)) =
                            self.symbols.globals.get(&id.name)
                        {
                            let param_map = info.default_param_map();
                            for (sname, _sdir, _sty) in info.effective_signals(&param_map) {
                                declared_names.insert(format!("{}_{}", w.name.name, sname));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Single pass in source order; interleave comments by byte position.
        // We need a clone of m to satisfy the borrow checker when calling
        // emit_reg_block (which takes &ModuleDecl) while also mutating self.
        let body_items: Vec<ModuleBodyItem> = m.body.clone();
        let m_clone = m.clone();
        for item in &body_items {
            self.emit_comments_before(item.span().start);
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&r.ty);
                    if let Some(ref init_expr) = r.init {
                        let init_str = self.emit_expr_str(init_expr);
                        if arr_suffix.is_empty() {
                            self.line(&format!("{} {} = {};", ty_str, r.name.name, init_str));
                        } else {
                            // Skip declaration initializer for unpacked arrays (icarus doesn't support '{default:})
                            self.line(&format!("{} {}{};", ty_str, r.name.name, arr_suffix));
                        }
                    } else {
                        self.line(&format!("{} {}{};", ty_str, r.name.name, arr_suffix));
                    }
                }
                ModuleBodyItem::LetBinding(l) => {
                    // Destructuring: `let {a, b} = expr;` — emit one
                    // wire + assignment per bound field, using struct-field
                    // access on the original RHS. The RHS is re-emitted
                    // per binding; structurally-identical expressions are
                    // fine at this stage (synth CSE handles it).
                    if !l.destructure_fields.is_empty() {
                        // Special case: RHS is `vec.find_first(pred)`.
                        // Emit the raw OR + priority encoder directly so we
                        // don't pay for the bulky struct-literal-then-field
                        // access shape. Widths come from the synthesized
                        // __ArchFindResult_<W> name.
                        if let ExprKind::MethodCall(recv, mname, margs) = &l.value.kind {
                            if mname.name == "find_first" {
                                let recv_str = self.emit_expr_str(recv);
                                let n = match &recv.kind {
                                    ExprKind::Ident(nm) => self.vec_sizes.get(nm).copied(),
                                    // `(uint_expr as Vec<T, N>).find_first(...)`
                                    // — extract N from the cast's target type.
                                    ExprKind::Cast(_, ty) => match &**ty {
                                        TypeExpr::Vec(_, size) => match &size.kind {
                                            ExprKind::Literal(LitKind::Dec(n))
                                            | ExprKind::Literal(LitKind::Hex(n))
                                            | ExprKind::Literal(LitKind::Bin(n))
                                            | ExprKind::Literal(LitKind::Sized(_, n)) => Some(*n as u32),
                                            ExprKind::Ident(name) => {
                                                self.symbols.globals.get(name)
                                                    .and_then(|(s, _)| match s {
                                                        crate::resolve::Symbol::Param(_) => None,
                                                        _ => None,
                                                    })
                                                    .or_else(|| {
                                                        // Fall back to the param's default if it's a const literal.
                                                        for it in &self.source.items {
                                                            if let Item::Module(m) = it {
                                                                if m.name.name == self.current_construct {
                                                                    for p in &m.params {
                                                                        if p.name.name == *name {
                                                                            if let Some(d) = &p.default {
                                                                                if let ExprKind::Literal(LitKind::Dec(n)) = &d.kind {
                                                                                    return Some(*n as u32);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        None
                                                    })
                                            }
                                            _ => None,
                                        },
                                        _ => None,
                                    },
                                    _ => None,
                                };
                                if let Some(n) = n {
                                    let idx_w = crate::width::index_width(n as u64);
                                    // Record width so the typedef still emits
                                    // (field access paths may still reference
                                    // the struct type).
                                    self.find_first_widths.borrow_mut().insert(idx_w);
                                    // Emit per-iteration predicate strings.
                                    let emit_at_i = |cg: &Codegen, i: u32| -> String {
                                        let this = cg as *const Codegen as *mut Codegen;
                                        unsafe {
                                            (*this).ident_subst.insert("item".to_string(), format!("{recv_str}[{i}]"));
                                            (*this).ident_subst.insert("index".to_string(), format!("{idx_w}'d{i}"));
                                        }
                                        let s = cg.emit_expr_str(&margs[0]);
                                        unsafe {
                                            (*this).ident_subst.remove("item");
                                            (*this).ident_subst.remove("index");
                                        }
                                        s
                                    };
                                    let hits: Vec<String> = (0..n).map(|i| emit_at_i(self, i)).collect();
                                    let found_expr = hits.join(" || ");
                                    let mut idx_expr = format!("{idx_w}'d0");
                                    for i in (0..n).rev() {
                                        let hit = &hits[i as usize];
                                        idx_expr = format!("({hit}) ? {idx_w}'d{i} : {idx_expr}");
                                    }
                                    for bind in &l.destructure_fields {
                                        match bind.name.as_str() {
                                            "found" => {
                                                self.line("logic found;");
                                                self.line(&format!("assign found = {found_expr};"));
                                            }
                                            "index" => {
                                                self.line(&format!("logic [{}:0] index;", idx_w.saturating_sub(1)));
                                                self.line(&format!("assign index = {idx_expr};"));
                                            }
                                            _ => {}
                                        }
                                    }
                                    continue;
                                }
                            }
                        }
                        let rhs_ty = self.infer_expr_struct_name(&l.value);
                        let val_str = self.emit_expr_str(&l.value);
                        for bind in &l.destructure_fields {
                            if let Some(field_ty) = rhs_ty.as_ref()
                                .and_then(|sname| self.struct_field_type(sname, &bind.name))
                            {
                                let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&field_ty);
                                self.line(&format!("{} {}{};", ty_str, bind.name, arr_suffix));
                                self.line(&format!("assign {} = {}.{};", bind.name, val_str, bind.name));
                            } else {
                                // Fallback: struct type unknown at codegen — emit as logic.
                                self.line(&format!("logic {};", bind.name));
                                self.line(&format!("assign {} = {}.{};", bind.name, val_str, bind.name));
                            }
                        }
                        continue;
                    }
                    // Special case: `let x: T = match scrut ... end match;` emits as
                    // `always_comb` with `case` instead of a deeply-nested ternary.
                    // Threshold: 3+ arms makes the ternary chain unreadable.
                    if let ExprKind::ExprMatch(scrut, arms) = &l.value.kind {
                        if arms.len() >= 3 {
                            if let Some(ty) = &l.ty {
                                let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                                self.line(&format!("{} {}{};", ty_str, l.name.name, arr_suffix));
                            }
                            let scrut_str = self.emit_expr_str(scrut);
                            self.line("always_comb begin");
                            self.indent += 1;
                            // Default to '0 (covers the unmatched-pattern case explicitly)
                            self.line(&format!("{} = '0;", l.name.name));
                            self.line(&format!("case ({})", scrut_str));
                            self.indent += 1;
                            for arm in arms {
                                let pat = self.emit_pattern(&arm.pattern);
                                let val_str = self.emit_expr_str(&arm.value);
                                self.line(&format!("{}: {} = {};", pat, l.name.name, val_str));
                            }
                            self.indent -= 1;
                            self.line("endcase");
                            self.indent -= 1;
                            self.line("end");
                            continue;
                        }
                    }
                    let val_str = self.emit_expr_str(&l.value);
                    if let Some(ty) = &l.ty {
                        let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                        self.line(&format!("{} {}{};", ty_str, l.name.name, arr_suffix));
                        self.line(&format!("assign {} = {};", l.name.name, val_str));
                    } else {
                        // ty=None: assignment to existing port or wire — no logic declaration
                        self.line(&format!("assign {} = {};", l.name.name, val_str));
                    }
                }
                ModuleBodyItem::CombBlock(cb) => self.emit_comb_block(cb),
                ModuleBodyItem::RegBlock(rb) => self.emit_reg_block(rb, &m_clone),
                ModuleBodyItem::LatchBlock(lb) => self.emit_latch_block(lb),
                ModuleBodyItem::Inst(inst) => {
                    // Auto-declare output wires that aren't already declared.
                    // Track newly-emitted names so a later inst that connects
                    // to the same parent-side bus wire skips re-declaration.
                    let mut just_added: Vec<String> = Vec::new();
                    self.emit_inst_output_wire_decls(inst, &declared_names);
                    // Collect names we added so subsequent insts know.
                    if let Some((Symbol::Module(info), _)) =
                        self.symbols.globals.get(&inst.module_name.name)
                    {
                        let module_ports = info.ports.clone();
                        for conn in &inst.connections {
                            let Some(port) = module_ports.iter().find(|p| p.name.name == conn.port_name.name) else { continue; };
                            let Some(bi) = &port.bus_info else { continue; };
                            let ExprKind::Ident(parent_name) = &conn.signal.kind else { continue; };
                            let Some((Symbol::Bus(bus_info), _)) =
                                self.symbols.globals.get(&bi.bus_name.name) else { continue; };
                            let mut pm = bus_info.default_param_map();
                            for pa in &bi.params { pm.insert(pa.name.name.clone(), &pa.value); }
                            for (sname, _sdir, _ty) in bus_info.effective_signals(&pm) {
                                just_added.push(format!("{parent_name}_{sname}"));
                            }
                            // Also mark the whole-bus parent name as "claimed"
                            // so later code doesn't re-emit a scalar for it.
                            just_added.push(parent_name.clone());
                        }
                    }
                    for n in just_added { declared_names.insert(n); }
                    self.emit_inst(inst);
                }
                ModuleBodyItem::PipeRegDecl(p) => {
                    self.emit_pipe_reg(p, &m_clone);
                }
                ModuleBodyItem::WireDecl(w) => {
                    // Bus-typed wires: flatten into individual SV signals
                    // `<wire>_<field>`. No SV interface/struct is generated
                    // for the bus; the bus exists purely as a compile-time
                    // abstraction. Record the wire in `bus_wires` so field
                    // access rewrites (see emit_expr_str) produce the flat
                    // name.
                    if let TypeExpr::Named(id) = &w.ty {
                        if let Some((crate::resolve::Symbol::Bus(info), _)) =
                            self.symbols.globals.get(&id.name)
                        {
                            self.bus_wires.insert(w.name.name.clone(), id.name.clone());
                            let param_map: std::collections::HashMap<String, &Expr> =
                                info.params.iter()
                                    .filter_map(|pd| pd.default.as_ref()
                                        .map(|d| (pd.name.name.clone(), d)))
                                    .collect();
                            for (sname, _sdir, sty) in info.effective_signals(&param_map) {
                                let (ty_str, arr_suffix) =
                                    self.emit_type_and_array_suffix(&sty);
                                self.line(&format!(
                                    "{} {}_{}{};",
                                    ty_str, w.name.name, sname, arr_suffix
                                ));
                                declared_names.insert(format!("{}_{}", w.name.name, sname));
                            }
                            declared_names.insert(w.name.name.clone());
                            continue;
                        }
                    }
                    if w.unpacked {
                        // SV unpacked-array shape (mirror of unpacked port modifier).
                        // Lets this wire mate with an `unpacked Vec<T,N>` port across
                        // an `inst` connection without Verilator rejecting the
                        // packed/unpacked shape mismatch.
                        let (base_ty, suffix) = self.emit_type_and_unpacked_suffix(&w.ty);
                        self.line(&format!("{} {}{};", base_ty, w.name.name, suffix));
                    } else {
                        let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&w.ty);
                        self.line(&format!("{} {}{};", ty_str, w.name.name, arr_suffix));
                    }
                    declared_names.insert(w.name.name.clone());
                }
                ModuleBodyItem::Generate(ref gen) => {
                    self.emit_generate(gen);
                }
                ModuleBodyItem::Thread(_) | ModuleBodyItem::Resource(_) => {
                    // Threads and resources are lowered before codegen
                    unreachable!("thread/resource should have been lowered before codegen");
                }
                ModuleBodyItem::Assert(_) => {
                    // Collected and emitted as a group below (with translate_off/on)
                }
                ModuleBodyItem::Function(f) => {
                    self.emit_function(f);
                }
            }
        }

        // Emit module-level assert/cover declarations (grouped with translate_off/on)
        {
            let module_asserts: Vec<&AssertDecl> = body_items.iter()
                .filter_map(|i| if let ModuleBodyItem::Assert(a) = i { Some(a) } else { None })
                .collect();
            if !module_asserts.is_empty() {
                let clk_name = m_clone.ports.iter()
                    .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
                    .map(|p| p.name.name.clone())
                    .unwrap_or_else(|| "clk".to_string());
                let owned: Vec<AssertDecl> = module_asserts.into_iter().cloned().collect();
                self.emit_asserts_for_construct(&owned, &m_clone.name.name, &clk_name);
            }
        }

        // Emit reset-only always_ff for any reg whose decl has a
        // `reset` clause but which is never assigned in any seq block.
        // Without this, a `reg X: T reset rst => V;` paired with no
        // seq-block update vanishes from the emitted SV — the decl is
        // kept but the reset never drives the flop, leaving it at X.
        // That shape is common for spec-constant CSR fields
        // (xdebugver, mhpmevent*, vendor/arch/impl IDs, etc.).
        self.emit_orphan_reset_regs(&m_clone);

        // Emit guard-contract SVA: for each `reg ... guard <sig>`, prove that
        // whenever `<sig>` is high, the reg has been written at least once.
        // Uses a shadow `_<reg>_written` set on any seq-block commit (over-approx).
        self.emit_guard_contracts(&m_clone);

        // Emit bounds-check SVA for runtime-indexed Vec / bit-select /
        // part-select accesses in seq/latch blocks. Mirrors arch sim's
        // _ARCH_BCHK so iverilog/Verilator/formal tools see the invariant.
        self.emit_bound_asserts(&m_clone);

        // Emit per-variant handshake protocol SVA for each bus port whose
        // bus definition contains one or more `handshake` channels.
        self.emit_handshake_asserts(&m_clone);

        // Emit the synthesized credit counter + can_send wire for each
        // `send`-role credit_channel bus port on the module (PR #3b-ii).
        self.emit_credit_channel_state(&m_clone);

        // Emit the target-side FIFO for each credit_channel bus port on
        // the module where this side is the receiver (PR #3b-iii).
        self.emit_credit_channel_receiver_state(&m_clone);

        // Emit the Tier-2 credit_channel protocol assertions (PR #4).
        self.emit_credit_channel_asserts(&m_clone);

        // Emit log file descriptors: initial $fopen / final $fclose
        let log_files = Self::collect_log_files(&m_clone.body);
        if !log_files.is_empty() {
            self.line("");
            self.line("// synopsys translate_off");
            for path in &log_files {
                let fd = Self::log_fd_name(path);
                self.line(&format!("integer {fd};"));
            }
            self.line("initial begin");
            self.indent += 1;
            for path in &log_files {
                let fd = Self::log_fd_name(path);
                self.line(&format!("{fd} = $fopen(\"{path}\", \"w\");"));
            }
            self.indent -= 1;
            self.line("end");
            self.line("final begin");
            self.indent += 1;
            for path in &log_files {
                let fd = Self::log_fd_name(path);
                self.line(&format!("$fclose({fd});"));
            }
            self.indent -= 1;
            self.line("end");
            self.line("// synopsys translate_on");
        }

        self.indent -= 1;
        self.line("");
        self.line("endmodule");
        self.line("");
    }

    /// Collect all unique log file paths from module body items.
    fn collect_log_files(body: &[ModuleBodyItem]) -> Vec<String> {
        let mut files = Vec::new();
        let mut seen = std::collections::HashSet::new();
        fn collect_from_comb(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
            for stmt in stmts {
                match stmt {
                    Stmt::Log(l) => {
                        if let Some(ref path) = l.file {
                            if seen.insert(path.clone()) { files.push(path.clone()); }
                        }
                    }
                    Stmt::IfElse(ie) => {
                        collect_from_comb(&ie.then_stmts, files, seen);
                        collect_from_comb(&ie.else_stmts, files, seen);
                    }
                    Stmt::Match(m) => {
                        for arm in &m.arms { collect_from_comb(&arm.body, files, seen); }
                    }
                    Stmt::For(f) => {
                        collect_from_comb(&f.body, files, seen);
                    }
                    _ => {}
                }
            }
        }
        fn collect_from_seq(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
            for stmt in stmts {
                match stmt {
                    Stmt::Log(l) => {
                        if let Some(ref path) = l.file {
                            if seen.insert(path.clone()) { files.push(path.clone()); }
                        }
                    }
                    Stmt::IfElse(ie) => {
                        collect_from_seq(&ie.then_stmts, files, seen);
                        collect_from_seq(&ie.else_stmts, files, seen);
                    }
                    Stmt::Match(m) => {
                        for arm in &m.arms { collect_from_seq(&arm.body, files, seen); }
                    }
                    _ => {}
                }
            }
        }
        for item in body {
            match item {
                ModuleBodyItem::CombBlock(cb) => collect_from_comb(&cb.stmts, &mut files, &mut seen),
                ModuleBodyItem::RegBlock(rb) => collect_from_seq(&rb.stmts, &mut files, &mut seen),
                _ => {}
            }
        }
        files
    }

    /// Find the SV type string for a signal by looking up ports, regs, and let bindings.
    fn find_signal_sv_type(&self, name: &str, m: &ModuleDecl) -> String {
        // Check ports
        for p in &m.ports {
            if p.name.name == name {
                return self.emit_type_str(&p.ty);
            }
        }
        // Check reg decls
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) if r.name.name == name => {
                    return self.emit_type_str(&r.ty);
                }
                ModuleBodyItem::LetBinding(l) if l.name.name == name => {
                    if let Some(ty) = &l.ty {
                        return self.emit_type_str(ty);
                    }
                }
                ModuleBodyItem::PipeRegDecl(p) if p.name.name == name => {
                    // Recursively resolve from source
                    return self.find_signal_sv_type(&p.source.name, m);
                }
                ModuleBodyItem::WireDecl(w) if w.name.name == name => {
                    return self.emit_type_str(&w.ty);
                }
                _ => {}
            }
        }
        "logic".to_string() // fallback
    }

    fn emit_pipe_reg(&mut self, p: &PipeRegDecl, m: &ModuleDecl) {
        let ty_str = self.find_signal_sv_type(&p.source.name, m);

        // Build chain of names: stg1, stg2, ..., final name
        let mut chain: Vec<String> = Vec::new();
        for i in 0..p.stages {
            if i == p.stages - 1 {
                chain.push(p.name.name.clone());
            } else {
                chain.push(format!("{}_stg{}", p.name.name, i + 1));
            }
        }

        // Declare all as regs
        for name in &chain {
            self.line(&format!("{} {};", ty_str, name));
        }

        // Find clock and reset from module ports
        let clk_name = m.ports.iter()
            .find(|port| matches!(&port.ty, TypeExpr::Clock(_)))
            .map(|port| port.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());

        let rst_name = m.ports.iter()
            .find(|port| matches!(&port.ty, TypeExpr::Reset(..)))
            .map(|port| port.name.name.clone());

        self.line(&format!("always_ff @(posedge {}) begin", clk_name));
        self.indent += 1;

        if let Some(ref rst) = rst_name {
            self.line(&format!("if ({}) begin", rst));
            self.indent += 1;
            for name in &chain {
                self.line(&format!("{} <= '0;", name));
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
        }

        let mut prev = p.source.name.clone();
        for name in &chain {
            self.line(&format!("{} <= {};", name, prev));
            prev = name.clone();
        }

        if rst_name.is_some() {
            self.indent -= 1;
            self.line("end");
        }

        self.indent -= 1;
        self.line("end");
    }

    fn emit_comb_block(&mut self, cb: &CombBlock) {
        // Simple assign form only when every statement is a plain assign with
        // no match-expression RHS (those need always_comb for the case block).
        let all_simple = cb.stmts.iter().all(|s| match s {
            Stmt::Assign(a) => !matches!(a.value.kind, ExprKind::ExprMatch(..)),
            _ => false,
        });
        if all_simple {
            for stmt in &cb.stmts {
                if let Stmt::Assign(a) = stmt {
                    let val = self.emit_expr_str(&a.value);
                    let tgt = self.emit_expr_str(&a.target);
                    self.line(&format!("assign {} = {};", tgt, val));
                }
            }
        } else {
            self.line("always_comb begin");
            self.indent += 1;
            for stmt in &cb.stmts {
                self.emit_comb_stmt(stmt);
            }
            self.emit_comments_before(cb.span.end);
            self.indent -= 1;
            self.line("end");
        }
    }

    /// Return true if any log() statement exists anywhere in the module body.
    fn module_has_log(body: &[ModuleBodyItem]) -> bool {
        body.iter().any(|item| match item {
            ModuleBodyItem::RegBlock(rb) => rb.stmts.iter().any(Self::stmt_has_log),
            ModuleBodyItem::CombBlock(cb) => cb.stmts.iter().any(Self::stmt_has_log),
            _ => false,
        })
    }

    fn stmt_has_log(s: &Stmt) -> bool {
        match s {
            Stmt::Log(_) => true,
            Stmt::IfElse(ie) => ie.then_stmts.iter().any(Self::stmt_has_log)
                || ie.else_stmts.iter().any(Self::stmt_has_log),
            Stmt::Match(m) => m.arms.iter().any(|a| a.body.iter().any(Self::stmt_has_log)),
            Stmt::Assign(_) => false,
            Stmt::For(f) => f.body.iter().any(Self::stmt_has_log),
            Stmt::Init(ib) => ib.body.iter().any(Self::stmt_has_log),
            Stmt::WaitUntil(_, _) => false,
            Stmt::DoUntil { body, .. } => body.iter().any(Self::stmt_has_log),
        }
    }

    /// Emit a for-loop (Range or ValueList) as SV.
    fn emit_reg_block(&mut self, rb: &RegBlock, m: &ModuleDecl) {
        let clk_edge = match rb.clock_edge {
            ClockEdge::Rising => "posedge",
            ClockEdge::Falling => "negedge",
        };

        // Collect all assigned register names in this block
        let mut assigned = std::collections::BTreeSet::new();
        Self::collect_assigned_roots(&rb.stmts, &mut assigned);

        // Look up reset info for each assigned register from its RegDecl
        let reg_decls: Vec<&RegDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i { Some(r) } else { None })
            .collect();

        // Resolve reset info: (rst_name, is_async, is_low) for registers that have reset
        struct ResolvedReset {
            signal: String,
            is_async: bool,
            is_low: bool,
        }
        let mut reset_info: Option<ResolvedReset> = Option::None;
        let mut resets: Vec<(String, String)> = Vec::new(); // (reg_name, init_str)
        for name in &assigned {
            if name.is_empty() { continue; }
            // Look up reset info from RegDecl or port reg
            let reset_ref: Option<&RegReset> = reg_decls.iter()
                .find(|r| r.name.name == *name)
                .map(|r| &r.reset)
                .or_else(|| m.ports.iter()
                    .find(|p| p.name.name == *name && p.reg_info.is_some())
                    .and_then(|p| p.reg_info.as_ref().map(|ri| &ri.reset)));
            if let Some(reg_reset) = reset_ref {
                let resolved = self.resolve_reg_reset(reg_reset, m);
                if let Some((rst_sig, is_async, is_low)) = resolved {
                    if reset_info.is_none() {
                        reset_info = Some(ResolvedReset {
                            signal: rst_sig.clone(),
                            is_async,
                            is_low,
                        });
                    }
                    let reset_val = Self::reset_value_expr(reg_reset).unwrap();
                    let init = self.emit_expr_str(reset_val);
                    resets.push((name.clone(), init));
                }
            }
        }

        if let Some(ref ri) = reset_info {
            // Build set of register names that have reset
            let reset_reg_names: std::collections::BTreeSet<String> =
                resets.iter().map(|(n, _)| n.clone()).collect();

            // Partition top-level statements: those that assign to reset
            // registers vs. those that only assign to non-reset registers.
            let mut guarded_stmts = Vec::new();
            let mut unguarded_stmts = Vec::new();
            for stmt in &rb.stmts {
                let mut stmt_roots = std::collections::BTreeSet::new();
                Self::collect_assigned_roots(std::slice::from_ref(stmt), &mut stmt_roots);
                let any_reset = stmt_roots.iter().any(|n| reset_reg_names.contains(n));
                if any_reset {
                    guarded_stmts.push(stmt);
                } else {
                    unguarded_stmts.push(stmt);
                }
            }

            let rst_cond_str = if ri.is_low {
                format!("(!{})", ri.signal)
            } else {
                ri.signal.clone()
            };

            // Emit always_ff with reset sensitivity for resetable registers
            if ri.is_async {
                let rst_edge = if ri.is_low { "negedge" } else { "posedge" };
                self.line(&format!(
                    "always_ff @({clk_edge} {} or {rst_edge} {}) begin",
                    rb.clock.name, ri.signal
                ));
            } else {
                self.line(&format!("always_ff @({clk_edge} {}) begin", rb.clock.name));
            }
            self.indent += 1;
            self.line(&format!("if ({rst_cond_str}) begin"));
            self.indent += 1;
            for (name, init) in &resets {
                // Look up Vec depth from reg decls OR port-reg declarations
                let reg_ty = reg_decls.iter()
                    .find(|r| r.name.name == *name)
                    .map(|r| &r.ty)
                    .or_else(|| m.ports.iter()
                        .find(|p| p.name.name == *name && p.reg_info.is_some())
                        .map(|p| &p.ty));
                let vec_depth = reg_ty.map(|ty| {
                    let mut depth = 0u32;
                    let mut t = ty;
                    while let TypeExpr::Vec(inner, _) = t {
                        depth += 1;
                        t = inner;
                    }
                    depth
                }).unwrap_or(0);
                if vec_depth > 0 {
                    // Emit for-loop reset for unpacked arrays (icarus-compatible)
                    if let Some(ty) = reg_ty {
                        // Collect Vec dimensions
                        let mut dims = Vec::new();
                        let mut t = ty;
                        while let TypeExpr::Vec(inner, size) = t {
                            dims.push(self.emit_expr_str(size));
                            t = inner;
                        }
                        // Generate nested for-loops
                        let idx_vars: Vec<String> = (0..dims.len()).map(|d| format!("__ri{d}")).collect();
                        for (d, dim_size) in dims.iter().enumerate() {
                            self.line(&format!("for (int {} = 0; {} < {}; {}++) begin",
                                idx_vars[d], idx_vars[d], dim_size, idx_vars[d]));
                            self.indent += 1;
                        }
                        let idx_str: String = idx_vars.iter().map(|v| format!("[{v}]")).collect();
                        self.line(&format!("{name}{idx_str} <= {init};"));
                        for _ in 0..dims.len() {
                            self.indent -= 1;
                            self.line("end");
                        }
                    }
                } else {
                    self.line(&format!("{name} <= {init};"));
                }
            }
            self.indent -= 1;
            self.line("end else begin");
            self.indent += 1;
            for stmt in &guarded_stmts {
                if let Stmt::Init(ib) = stmt {
                    let port = m.ports.iter().find(|p| p.name.name == ib.reset_signal.name);
                    let is_low = port.map_or(false, |p| matches!(&p.ty, TypeExpr::Reset(_, ResetLevel::Low)));
                    let cond = if is_low { format!("(!{})", ib.reset_signal.name) } else { ib.reset_signal.name.clone() };
                    self.line(&format!("if ({cond}) begin"));
                    self.indent += 1;
                    for s in &ib.body { self.emit_reg_stmt(s); }
                    self.indent -= 1;
                    self.line("end");
                } else {
                    self.emit_reg_stmt(stmt);
                }
            }
            self.emit_comments_before(rb.span.end);
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");

            // Emit separate always_ff WITHOUT reset sensitivity for non-reset registers.
            // Mixing resetable and non-resetable regs in one always_ff with async reset
            // in the sensitivity list causes synthesis tools to infer unintended clock
            // gating on the reset path for the non-reset registers.
            if !unguarded_stmts.is_empty() {
                self.line(&format!("always_ff @({clk_edge} {}) begin", rb.clock.name));
                self.indent += 1;
                for stmt in &unguarded_stmts {
                    if let Stmt::Init(ib) = stmt {
                        let port = m.ports.iter().find(|p| p.name.name == ib.reset_signal.name);
                        let is_low = port.map_or(false, |p| matches!(&p.ty, TypeExpr::Reset(_, ResetLevel::Low)));
                        let cond = if is_low { format!("(!{})", ib.reset_signal.name) } else { ib.reset_signal.name.clone() };
                        self.line(&format!("if ({cond}) begin"));
                        self.indent += 1;
                        for s in &ib.body { self.emit_reg_stmt(s); }
                        self.indent -= 1;
                        self.line("end");
                    } else {
                        self.emit_reg_stmt(stmt);
                    }
                }
                self.emit_comments_before(rb.span.end);
                self.indent -= 1;
                self.line("end");
            }
        } else {
            // No registers with declared reset.
            // Check for explicit `init on rst.asserted` blocks — these drive async sensitivity.
            let init_block = rb.stmts.iter().find_map(|s| {
                if let Stmt::Init(ib) = s { Some(ib) } else { None }
            });
            let async_asserted = if let Some(ib) = init_block {
                // Determine async/sync and polarity from the referenced reset port
                m.ports.iter().find(|p| p.name.name == ib.reset_signal.name)
                    .and_then(|p| if let TypeExpr::Reset(ResetKind::Async, level) = &p.ty {
                        Some((ib.reset_signal.name.clone(), *level == ResetLevel::Low))
                    } else { None })
            } else {
                // Still check for `rst.asserted` expressions in the body — if any reference an
                // async reset port, we must add the async edge to the sensitivity list.
                Self::find_asserted_async_reset(&rb.stmts, &m.ports)
            };
            let sens = if let Some((ref rst_sig, is_low)) = async_asserted {
                let rst_edge = if is_low { "negedge" } else { "posedge" };
                format!("always_ff @({clk_edge} {} or {rst_edge} {rst_sig}) begin", rb.clock.name)
            } else {
                format!("always_ff @({clk_edge} {}) begin", rb.clock.name)
            };
            self.line(&sens);
            self.indent += 1;
            for stmt in &rb.stmts {
                if let Stmt::Init(ib) = stmt {
                    // Emit as an explicit `if (rst_cond) begin ... end` block
                    let port = m.ports.iter().find(|p| p.name.name == ib.reset_signal.name);
                    let is_low = port.map_or(false, |p| matches!(&p.ty, TypeExpr::Reset(_, ResetLevel::Low)));
                    let cond = if is_low {
                        format!("(!{})", ib.reset_signal.name)
                    } else {
                        ib.reset_signal.name.clone()
                    };
                    self.line(&format!("if ({cond}) begin"));
                    self.indent += 1;
                    for s in &ib.body {
                        self.emit_reg_stmt(s);
                    }
                    self.indent -= 1;
                    self.line("end");
                } else {
                    self.emit_reg_stmt(stmt);
                }
            }
            self.emit_comments_before(rb.span.end);
            self.indent -= 1;
            self.line("end");
        }
    }

    /// Scan statements for `name.asserted` where `name` is an async Reset port.
    /// Returns `Some((signal_name, is_low))` for the first one found.
    fn find_asserted_async_reset(stmts: &[Stmt], ports: &[PortDecl]) -> Option<(String, bool)> {
        fn scan_expr(expr: &Expr, ports: &[PortDecl]) -> Option<(String, bool)> {
            if let ExprKind::FieldAccess(base, field) = &expr.kind {
                if field.name == "asserted" {
                    if let ExprKind::Ident(name) = &base.kind {
                        if let Some(port) = ports.iter().find(|p| p.name.name == *name) {
                            if let TypeExpr::Reset(ResetKind::Async, level) = &port.ty {
                                return Some((name.clone(), *level == ResetLevel::Low));
                            }
                        }
                    }
                }
            }
            // Recurse into sub-expressions
            match &expr.kind {
                ExprKind::Binary(_, l, r) => scan_expr(l, ports).or_else(|| scan_expr(r, ports)),
                ExprKind::Unary(_, inner) | ExprKind::FieldAccess(inner, _) => scan_expr(inner, ports),
                ExprKind::Ternary(c, t, e) => scan_expr(c, ports).or_else(|| scan_expr(t, ports)).or_else(|| scan_expr(e, ports)),
                ExprKind::MethodCall(base, _, args) => scan_expr(base, ports).or_else(|| args.iter().find_map(|a| scan_expr(a, ports))),
                ExprKind::Index(base, idx) => scan_expr(base, ports).or_else(|| scan_expr(idx, ports)),
                ExprKind::Cast(inner, _) => scan_expr(inner, ports),
                _ => None,
            }
        }
        fn scan_stmt(stmt: &Stmt, ports: &[PortDecl]) -> Option<(String, bool)> {
            match stmt {
                Stmt::Assign(a) => scan_expr(&a.value, ports),
                Stmt::IfElse(ie) => scan_expr(&ie.cond, ports)
                    .or_else(|| ie.then_stmts.iter().find_map(|s| scan_stmt(s, ports)))
                    .or_else(|| ie.else_stmts.iter().find_map(|s| scan_stmt(s, ports))),
                Stmt::For(f) => f.body.iter().find_map(|s| scan_stmt(s, ports)),
                Stmt::Match(m) => m.arms.iter().find_map(|arm| arm.body.iter().find_map(|s| scan_stmt(s, ports))),
                _ => None,
            }
        }
        stmts.iter().find_map(|s| scan_stmt(s, ports))
    }

    fn emit_latch_block(&mut self, lb: &LatchBlock) {
        self.line(&format!("always_latch begin"));
        self.indent += 1;
        self.line(&format!("if ({}) begin", lb.enable.name));
        self.indent += 1;
        for stmt in &lb.stmts {
            self.emit_stmt(stmt, AssignCtx::Blocking);
        }
        self.indent -= 1;
        self.line("end");
        self.indent -= 1;
        self.line("end");
    }

    /// Resolve a register's reset info: returns Some((signal_name, is_async, is_low))
    /// or None if the register has no reset.
    /// Extract the reset value expression from a RegReset variant.
    /// Emit `always_ff` reset assignments for registers that have a
    /// `reset` clause on their decl but are never assigned in any seq
    /// block. Without this, the reset clause is silently dropped and
    /// the flop sits at X after reset (Verilator lints as UNDRIVEN).
    ///
    /// Clock source: we prefer a clock that's already the `seq on
    /// <clk>` for some RegBlock in this module (preserves clock-domain
    /// grouping with the rest of the flops). If the module has no
    /// RegBlocks at all, fall back to the first `Clock`-typed input
    /// port — by construction that's the only clock the orphan reg can
    /// belong to in a well-formed module.
    ///
    /// Orphans are grouped by (clock, reset_signal, is_async, is_low)
    /// so each group becomes exactly one `always_ff`.
    fn emit_orphan_reset_regs(&mut self, m: &ModuleDecl) {
        // 1. Collect names assigned in any RegBlock.
        let mut assigned_in_any_block = std::collections::BTreeSet::new();
        for item in &m.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                Self::collect_assigned_roots(&rb.stmts, &mut assigned_in_any_block);
            }
        }

        // 2. Find orphans: RegDecls with reset != None and not assigned.
        #[derive(Debug)]
        struct Orphan<'a> {
            name: String,
            init: String,
            reset_signal: String,
            is_async: bool,
            is_low: bool,
            ty: &'a TypeExpr,
        }

        let mut orphans: Vec<Orphan> = Vec::new();
        for item in &m.body {
            let ModuleBodyItem::RegDecl(r) = item else { continue };
            if matches!(r.reset, RegReset::None) {
                continue;
            }
            if assigned_in_any_block.contains(&r.name.name) {
                continue;
            }
            let Some((signal, is_async, is_low)) = self.resolve_reg_reset(&r.reset, m) else {
                // Reset resolution failed (malformed decl after typecheck —
                // shouldn't happen, but guard anyway).
                continue;
            };
            let val_expr = Self::reset_value_expr(&r.reset).expect("non-None reset has value");
            let init = self.emit_expr_str(val_expr);
            orphans.push(Orphan {
                name: r.name.name.clone(),
                init,
                reset_signal: signal,
                is_async,
                is_low,
                ty: &r.ty,
            });
        }

        if orphans.is_empty() {
            return;
        }

        // 3. Pick a clock. Prefer one used by an existing RegBlock so
        // the orphans land in the same clock domain as the other
        // flops. Fall back to the first Clock-typed input port.
        let rb_clock: Option<&Ident> = m.body.iter().find_map(|i| {
            if let ModuleBodyItem::RegBlock(rb) = i { Some(&rb.clock) } else { None }
        });
        let port_clock = m.ports.iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| &p.name);
        let Some(clock_ident) = rb_clock.or(port_clock) else {
            // No clock available — can't emit a clocked reset. Leave
            // the reg as-is; Verilator will still warn, but that's
            // a more fundamental design issue the user needs to fix.
            return;
        };
        let clk_name = &clock_ident.name;

        // 4. Group by (reset_signal, is_async, is_low). We collapse
        // orphans into one always_ff per group.
        let mut groups: std::collections::BTreeMap<(String, bool, bool), Vec<&Orphan>> =
            std::collections::BTreeMap::new();
        for o in &orphans {
            groups.entry((o.reset_signal.clone(), o.is_async, o.is_low))
                .or_default()
                .push(o);
        }

        // 5. Emit one always_ff per group.
        for ((reset_signal, is_async, is_low), group) in &groups {
            let rst_cond = if *is_low {
                format!("(!{})", reset_signal)
            } else {
                reset_signal.clone()
            };
            if *is_async {
                let rst_edge = if *is_low { "negedge" } else { "posedge" };
                self.line(&format!(
                    "always_ff @(posedge {} or {} {}) begin",
                    clk_name, rst_edge, reset_signal
                ));
            } else {
                self.line(&format!("always_ff @(posedge {}) begin", clk_name));
            }
            self.indent += 1;
            self.line(&format!("if ({}) begin", rst_cond));
            self.indent += 1;
            for o in group {
                self.emit_reset_only_assignment(o.name.as_str(), o.ty, o.init.as_str());
            }
            self.indent -= 1;
            self.line("end");
            self.indent -= 1;
            self.line("end");
        }
    }

    /// Emit a single `<name> <= <init>;` with for-loop unpacking if the
    /// reg is a Vec (unpacked array). Mirrors the per-name emission in
    /// `emit_reg_block`'s reset branch, just factored out for reuse.
    fn emit_reset_only_assignment(&mut self, name: &str, ty: &TypeExpr, init: &str) {
        // Collect Vec dimensions (outermost first — same order as the
        // SV unpacked-array suffix).
        let mut dims: Vec<String> = Vec::new();
        let mut t = ty;
        while let TypeExpr::Vec(inner, size) = t {
            dims.push(self.emit_expr_str(size));
            t = inner;
        }
        if dims.is_empty() {
            self.line(&format!("{name} <= {init};"));
            return;
        }
        let idx_vars: Vec<String> = (0..dims.len()).map(|d| format!("__ri{d}")).collect();
        for (d, dim_size) in dims.iter().enumerate() {
            self.line(&format!(
                "for (int {} = 0; {} < {}; {}++) begin",
                idx_vars[d], idx_vars[d], dim_size, idx_vars[d]
            ));
            self.indent += 1;
        }
        let idx_str: String = idx_vars.iter().map(|v| format!("[{v}]")).collect();
        self.line(&format!("{name}{idx_str} <= {init};"));
        for _ in 0..dims.len() {
            self.indent -= 1;
            self.line("end");
        }
    }

}
