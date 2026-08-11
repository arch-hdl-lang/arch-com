//! `stmt_codegen` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

pub(super) fn ind(n: usize) -> String {
    "  ".repeat(n)
}

/// Sim-codegen analog of `codegen::AssignCtx`. Phase 5b part 4 — drives
/// the unified `emit_stmt` walker so seq vs comb stmt emission shares
/// one source of truth. The flag affects:
/// - **LHS resolution**: `Seq` resolves to the next-cycle shadow
///   `_n_{name}` (committed at end of cycle); `Comb` resolves to the
///   live `_{name}` (visible immediately).
/// - **Wide-output-port conversion**: only `Comb` paths apply
///   `_arch_u128_to_vl` for 65–128b output ports (>128b is a direct
///   `VlWide<N>` assignment); `Seq` writes go through `cpp_expr_lhs`
///   which handles the shadow naming uniformly.
/// - **Init / WaitUntil / DoUntil legality**: `Seq` only.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum SimAssignKind {
    Seq,
    Comb,
}

/// Resolve a `ScaledVec<Elem, N, Scale>` TypeExpr to the block shape the
/// helper emitters are keyed on. `None` — never a guess — when `N` does not
/// fold to a literal or a member type is not a legal block member; the caller
/// turns that into a panic, because typecheck has already accepted the type.
fn block_shape_of_type(ty: &TypeExpr) -> Option<crate::fp_block::BlockShape> {
    let TypeExpr::ScaledVec(elem, size, scale) = ty else {
        return None;
    };
    let n = match &size.kind {
        ExprKind::Literal(LitKind::Dec(n)) | ExprKind::Literal(LitKind::Hex(n)) => *n as u32,
        _ => return None,
    };
    crate::fp_block::shape_of(elem, n, scale)
}

/// Record `h` as needed by this module and return its C++ name.
fn use_block_helper(h: crate::fp_block::BlockHelper, ctx: &Ctx) -> String {
    match ctx.block_helpers {
        Some(reg) => {
            reg.borrow_mut().insert(h);
        }
        // A context that never registered a collector cannot emit the
        // definition, so a call would compile to an undeclared identifier.
        // Fail loudly at compile time rather than at the user's C++ build.
        None => panic!(
            "`{}` is needed here but this sim context has no block-helper \
             registry, so its definition would never be emitted (arch#884). \
             The construct emitting this statement must thread \
             `Ctx::with_block_helpers` through.",
            h.cpp_name()
        ),
    }
    h.cpp_name()
}

/// Emit `dst = scaled_quantize<Fmt,...>(v)` or `dst = scaled_dequantize(b)`
/// as a call statement. Returns `Some(())` when this statement was one of
/// those and has been emitted, `None` to fall through to normal assignment.
fn emit_scaled_block_assign(
    a: &crate::ast::RegAssign,
    ctx: &Ctx,
    out: &mut String,
    indent: usize,
) -> Option<()> {
    match &a.value.kind {
        ExprKind::ScaledQuantize(v, fmt, policy, round) => {
            let shape = block_shape_of_type(fmt.as_ref()).unwrap_or_else(|| {
                panic!(
                    "scaled_quantize format has no resolvable block shape — typecheck \
                     accepts only `ScaledVec` formats, so this means the block size did \
                     not fold to a literal (arch#884)"
                )
            });
            let name = use_block_helper(
                crate::fp_block::BlockHelper::Quantize {
                    shape,
                    policy: *policy,
                    round: *round,
                },
                ctx,
            );
            let src = cpp_expr(v.as_ref(), ctx);
            let dst = cpp_expr_lhs(&a.target, ctx);
            out.push_str(&format!("{}{name}({src}, {dst});\n", ind(indent)));
            Some(())
        }
        ExprKind::FunctionCall(fname, args) if fname == "scaled_dequantize" && args.len() == 1 => {
            let shape = ctx
                .decl_types
                .and_then(|m| match &args[0].kind {
                    ExprKind::Ident(n) => m.get(n.as_str()),
                    _ => None,
                })
                .and_then(block_shape_of_type)
                .unwrap_or_else(|| {
                    panic!(
                        "scaled_dequantize operand has no resolvable block shape — typecheck \
                         accepts only `ScaledVec` operands, so this means the operand is not a \
                         plain signal name or its block size did not fold to a literal (arch#884)"
                    )
                });
            let name = use_block_helper(crate::fp_block::BlockHelper::Dequantize { shape }, ctx);
            let src = cpp_expr(&args[0], ctx);
            let dst = cpp_expr_lhs(&a.target, ctx);
            out.push_str(&format!("{}{name}({src}, {dst});\n", ind(indent)));
            Some(())
        }
        _ => None,
    }
}

pub(super) fn emit_stmts(
    stmts: &[Stmt],
    ctx: &Ctx,
    out: &mut String,
    indent: usize,
    k: SimAssignKind,
) {
    for stmt in stmts {
        emit_stmt(stmt, ctx, out, indent, k);
    }
}

pub(super) fn assigned_base_ident(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.as_str()),
        ExprKind::Index(base, _) | ExprKind::BitSlice(base, _, _) => assigned_base_ident(base),
        _ => None,
    }
}

pub(super) fn emit_vinit_mark_for_target(
    target: &Expr,
    ctx: &Ctx,
    out: &mut String,
    indent: usize,
) {
    let Some(vinit_regs) = ctx.vinit_regs else {
        return;
    };
    let Some(name) = assigned_base_ident(target) else {
        return;
    };
    if vinit_regs.contains(name) {
        out.push_str(&format!("{}_{name}_vinit = true;\n", ind(indent)));
    }
}

pub(super) fn emit_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize, k: SimAssignKind) {
    let is_seq = k == SimAssignKind::Seq;
    match stmt {
        Stmt::Assign(a) => {
            // `ScaledVec` block ops. Emitted as a STATEMENT, not an
            // expression: both sides are aggregates in the sim (a block wider
            // than 64 bits is a `VlWide`, a `Vec<FP32,N>` is a C array), so
            // the generated helper takes the destination by reference. The SV
            // backend can use a plain function return because a packed array
            // and a packed vector are assignment-compatible there; C++ has no
            // such equivalence, which is why the two backends differ in shape
            // here and nowhere else.
            if let Some(call) = emit_scaled_block_assign(a, ctx, out, indent) {
                let _ = call;
                if is_seq {
                    emit_vinit_mark_for_target(&a.target, ctx, out, indent);
                }
                return;
            }
            // Whole-Vec assignment: C arrays are not assignable in C++, so
            // lower `dst <= src_vec;` / `dst = src_vec;` to an element copy.
            // This is hit by TLM Vec payloads, e.g. `data <= m.read4(...)`
            // after TLM lowering becomes `data <= m_read4_rsp_data`.
            let vec_name_of_expr = |e: &Expr| -> Option<String> {
                match &e.kind {
                    ExprKind::Ident(name) => Some(name.clone()),
                    ExprKind::FieldAccess(base, field) => {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            if ctx.bus_ports.contains(base_name.as_str()) {
                                Some(format!("{}_{}", base_name, field.name))
                            } else {
                                Some(format!("{}.{}", base_name, field.name))
                            }
                        } else {
                            ctx.vec_path_of_expr(e)
                        }
                    }
                    _ => None,
                }
            };
            if let Some(dst_name) = vec_name_of_expr(&a.target) {
                if ctx
                    .vec_names
                    .map_or(false, |s| s.contains(dst_name.as_str()))
                {
                    let lhs = cpp_expr_lhs(&a.target, ctx);
                    let rhs = cpp_expr(&a.value, ctx);
                    let count = ctx
                        .vec_sizes
                        .and_then(|m| m.get(dst_name.as_str()).copied())
                        .unwrap_or(0);
                    if rhs == "0" {
                        out.push_str(&format!(
                            "{}memset({lhs}, 0, sizeof({lhs}));\n",
                            ind(indent)
                        ));
                        if is_seq {
                            emit_vinit_mark_for_target(&a.target, ctx, out, indent);
                        }
                        return;
                    }
                    if let Some(rhs_name) = vec_name_of_expr(&a.value) {
                        if ctx
                            .vec_names
                            .map_or(false, |s| s.contains(rhs_name.as_str()))
                        {
                            if count > 0 {
                                out.push_str(&format!(
                                    "{}for (size_t _i = 0; _i < {count}; ++_i) {{ {lhs}[_i] = {rhs}[_i]; }}\n",
                                    ind(indent)
                                ));
                                if is_seq {
                                    emit_vinit_mark_for_target(&a.target, ctx, out, indent);
                                }
                                return;
                            }
                        }
                    }
                }
            }
            // Scalar bit-indexed LHS: name[idx] = val where name is NOT a Vec.
            // Emit mask-and-OR: base = (base & ~(1ULL << idx)) | (uint64_t(val & 1) << idx).
            // resolve_name's is_lhs flag = is_seq → seq writes hit the shadow.
            if let ExprKind::Index(base, idx_expr) = &a.target.kind {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if !ctx
                        .vec_names
                        .map_or(false, |s| s.contains(base_name.as_str()))
                    {
                        let resolved_base = ctx.resolve_name(base_name, is_seq);
                        let idx_cpp = cpp_expr(idx_expr, ctx);
                        let rhs = cpp_expr(&a.value, ctx);
                        out.push_str(&format!(
                            "{}{resolved_base} = ({resolved_base} & ~(uint64_t(1) << ({idx_cpp}))) | (uint64_t(({rhs}) & 1) << ({idx_cpp}));\n",
                            ind(indent)
                        ));
                        if is_seq {
                            emit_vinit_mark_for_target(&a.target, ctx, out, indent);
                        }
                        return;
                    }
                }
            }
            // Bit-slice LHS: name[hi:lo] = val (or name[idx][hi:lo] for a
            // Vec-element base). Lower to mask-and-OR (read-modify-write)
            // rather than the read-side `((name >> lo) & mask)` (an rvalue —
            // gcc/clang reject as "expression is not assignable"). Only the
            // narrow scalar and Vec-element base cases are handled here;
            // wider-than-64 bases keep the generic path and would need their
            // own arms. Slice bounds may be runtime expressions (loop vars,
            // arch#847): the shift is emitted symbolically and the mask
            // comes from the structural slice width.
            if let ExprKind::BitSlice(base, hi_e, lo_e) = &a.target.kind {
                // A Vec-element base (`mem[addr][hi:lo]`, arch#847) resolves
                // through infer_expr_width's Index arm, which reads the
                // declared element type since arch#858 (the former local
                // decl_types workaround here folded into that shared path).
                let base_w = infer_expr_width(base, ctx);
                if base_w > 0 && base_w <= 64 {
                    let resolved_base = match &base.kind {
                        ExprKind::Ident(base_name) => ctx.resolve_name(base_name, is_seq),
                        ExprKind::Index(_, _) => cpp_expr_lhs(base, ctx),
                        _ => String::new(),
                    };
                    if !resolved_base.is_empty() {
                        let lo_str = match try_eval_const_expr_with_params(lo_e, ctx.params) {
                            Some(v) => v.to_string(),
                            None => format!("({})", cpp_expr(lo_e, ctx)),
                        };
                        // Runtime-bound slices can't be verified by typecheck —
                        // abort on an out-of-range MSB like every other
                        // runtime access.
                        let hi_is_const =
                            try_eval_const_expr_with_params(hi_e, ctx.params).is_some();
                        if !hi_is_const {
                            let hi_cpp = cpp_expr(hi_e, ctx);
                            let loc = assigned_base_ident(&a.target).unwrap_or("<slice>");
                            out.push_str(&format!(
                                "{}_ARCH_BCHK(({hi_cpp}), {base_w}, \"{loc}[hi:lo]\");\n",
                                ind(indent)
                            ));
                        }
                        let rhs = cpp_expr(&a.value, ctx);
                        match try_slice_const_width(hi_e, lo_e, ctx) {
                            Some(width) => {
                                let val_mask: u64 = if width >= 64 {
                                    u64::MAX
                                } else {
                                    (1u64 << width) - 1
                                };
                                out.push_str(&format!(
                                    "{}{resolved_base} = ({resolved_base} & ~(uint64_t(0x{val_mask:X}ULL) << {lo_str})) | ((uint64_t(({rhs}) & 0x{val_mask:X}ULL)) << {lo_str});\n",
                                    ind(indent)
                                ));
                            }
                            None => {
                                // Width itself depends on a runtime value:
                                // compute the mask at runtime.
                                let hi_cpp = cpp_expr(hi_e, ctx);
                                out.push_str(&format!(
                                    "{}{resolved_base} = ({resolved_base} & ~(_arch_slice_mask(({hi_cpp}), {lo_str}) << {lo_str})) | ((uint64_t({rhs}) & _arch_slice_mask(({hi_cpp}), {lo_str})) << {lo_str});\n",
                                    ind(indent)
                                ));
                            }
                        }
                        if is_seq {
                            emit_vinit_mark_for_target(&a.target, ctx, out, indent);
                        }
                        return;
                    }
                }
            }
            let rhs = cpp_expr(&a.value, ctx);
            if is_seq {
                let lhs = cpp_expr_lhs(&a.target, ctx);
                out.push_str(&format!("{}{}  = {};\n", ind(indent), lhs, rhs));
                emit_vinit_mark_for_target(&a.target, ctx, out, indent);
            } else {
                // Comb: bare-ident-aware target name + wide-output-port conversion.
                let target_name = if let ExprKind::Ident(name) = &a.target.kind {
                    name.clone()
                } else {
                    cpp_expr(&a.target, ctx)
                };
                let resolved_target = ctx.resolve_name(&target_name, false);
                if ctx.wide_names.contains(target_name.as_str()) {
                    let bits = ctx.widths.get(target_name.as_str()).copied().unwrap_or(0);
                    if bits > 128 {
                        // >128 bits: both internal and port are VlWide<N> — direct assign.
                        out.push_str(&format!("{}{} = {};\n", ind(indent), target_name, rhs));
                    } else {
                        // 65–128 bits: internal is _arch_u128, port is
                        // VlWide<ceil(W/32)>. Pass the real word count so a
                        // VlWide<3> (66–96 bit) port is not written out of
                        // bounds (which clobbers the adjacent struct member).
                        out.push_str(&format!(
                            "{}  _arch_u128_to_vl({}, {}._data, {});\n",
                            ind(indent),
                            rhs,
                            target_name,
                            wide_words(bits)
                        ));
                    }
                } else {
                    out.push_str(&format!("{}{}  = {};\n", ind(indent), resolved_target, rhs));
                }
            }
        }
        Stmt::IfElse(ie) => emit_if_else(ie, ctx, out, indent, false, k),
        Stmt::Match(m) => {
            let scrut = cpp_expr(&m.scrutinee, ctx);
            out.push_str(&format!("{}switch ({}) {{\n", ind(indent), scrut));
            for arm in &m.arms {
                let (case_str, label) = match &arm.pattern {
                    Pattern::Wildcard => ("default".to_string(), "match _".to_string()),
                    Pattern::Ident(id) => {
                        // If `id` names a module-scope let-binding with a
                        // literal RHS, emit `case <literal>:` so multiple
                        // ident arms (e.g. `ALU_ADD`, `ALU_SUB`, ...) don't
                        // collapse into "multiple default labels". Falls
                        // back to `default` when the let is missing or its
                        // RHS isn't a constant — preserves wildcard-binding
                        // semantics for non-let idents.
                        let folded = ctx
                            .let_values
                            .and_then(|m| m.get(&id.name))
                            .filter(|e| matches!(&e.kind, ExprKind::Literal(_)));
                        match folded {
                            Some(e) => (
                                format!("case {}", cpp_expr(e, ctx)),
                                format!("match {}", id.name),
                            ),
                            None => ("default".to_string(), format!("match {}", id.name)),
                        }
                    }
                    Pattern::Literal(e) => (
                        format!("case {}", cpp_expr(e, ctx)),
                        "match lit".to_string(),
                    ),
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants
                                .iter()
                                .find(|(n, _)| *n == vr.name)
                                .map(|(_, v)| *v)
                                .unwrap_or(0);
                            (
                                format!("case {idx}"),
                                format!("match {}::{}", en.name, vr.name),
                            )
                        } else {
                            (
                                "default".to_string(),
                                format!("match {}::{}", en.name, vr.name),
                            )
                        }
                    }
                };
                out.push_str(&format!("{}{}: {{\n", ind(indent + 1), case_str));
                // --coverage: per match-arm counter. Use the match's
                // span.start so the report points to the match statement
                // (per-arm spans aren't tracked on MatchArm); the label
                // disambiguates which arm.
                if let Some(reg) = ctx.coverage {
                    let kind = if matches!(arm.pattern, Pattern::Wildcard | Pattern::Ident(_)) {
                        "match-default"
                    } else {
                        "match-arm"
                    };
                    let cidx = reg.borrow_mut().alloc(kind, m.span.start, label);
                    out.push_str(&format!("{}  _arch_cov[{cidx}]++;\n", ind(indent + 1)));
                }
                // Arm body: full recurse via the unified emitter — this is
                // the bug fix. Pre-collapse, the comb walker silently
                // emitted only `Stmt::Assign` arms and dropped nested
                // `if/else`, `match`, `for`, and `log` inside arm bodies.
                emit_stmts(&arm.body, ctx, out, indent + 2, k);
                out.push_str(&format!("{}  break;\n", ind(indent + 1)));
                out.push_str(&format!("{}}}\n", ind(indent + 1)));
            }
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        Stmt::Log(l) => emit_log_stmt(l, ctx, out, indent),
        Stmt::For(f) => {
            let var = &f.var.name;
            // Static unrolling for Vec-of-bus indexed access (mirror of the
            // SV-side path). The C++ struct fields are flat per-element
            // (`chans_0_v`, `chans_1_v`, ...), not arrays, so a behavioral
            // `for (int i ...; i <= N; i++) chans[i].v = ...` would emit a
            // reference to an undeclared `chans`. Detect Vec-of-bus indexed
            // writes by the loop variable, and if found AND bounds are
            // literal, statically unroll: bind the loop var to each
            // iteration value via `loop_var_subst` and emit the body N
            // times.
            if let (ForRange::Range(rs, re), Some(subst), Some(vob_ports), Some(vob_wires)) = (
                &f.range,
                ctx.loop_var_subst,
                ctx.vec_of_bus_port_count,
                ctx.vec_of_bus_wire_count,
            ) {
                // Param-driven bounds (e.g. `for i in 0..NUM-1`) fold against
                // the module's params so the unroll fires on `Vec<Bus, NUM>`
                // with a param-driven N. `eval_const_expr_with_params`
                // returns 0 for anything it can't fold; we still need a
                // signal that the bound was foldable, so guard literal-zero
                // by requiring `start <= end` AND the body actually touches
                // a Vec-of-bus.
                let folds_to = |e: &Expr| -> Option<u32> {
                    let v = eval_const_expr_with_params(e, ctx.params) as u32;
                    // Any expression that wasn't a literal-zero in disguise
                    // and that the body actually depends on counts as
                    // foldable; in practice the body-touch predicate below
                    // guards against false positives.
                    Some(v)
                };
                if let (Some(start_lit), Some(end_lit)) = (folds_to(rs), folds_to(re)) {
                    let touches = f
                        .body
                        .iter()
                        .any(|s| stmt_indexes_vob_with_var(s, var, vob_ports, vob_wires));
                    if touches {
                        for i in start_lit..=end_lit {
                            subst.borrow_mut().insert(var.clone(), i);
                            for s in &f.body {
                                emit_stmt(s, ctx, out, indent, k);
                            }
                        }
                        subst.borrow_mut().remove(var);
                        return;
                    }
                }
            }
            match &f.range {
                ForRange::Range(rs, re) => {
                    let start = cpp_expr(rs, ctx);
                    let end = cpp_expr(re, ctx);
                    out.push_str(&format!(
                        "{}for (int {var} = {start}; {var} <= {end}; {var}++) {{\n",
                        ind(indent)
                    ));
                    for s in &f.body {
                        emit_stmt(s, ctx, out, indent + 1, k);
                    }
                    out.push_str(&format!("{}}}\n", ind(indent)));
                }
                ForRange::ValueList(vals) => {
                    for v in vals {
                        let val = cpp_expr(v, ctx);
                        out.push_str(&format!("{}{{\n", ind(indent)));
                        out.push_str(&format!("{}int {var} = {val};\n", ind(indent + 1)));
                        for s in &f.body {
                            emit_stmt(s, ctx, out, indent + 1, k);
                        }
                        out.push_str(&format!("{}}}\n", ind(indent)));
                    }
                }
            }
        }
        Stmt::Init(ib) => {
            if !is_seq {
                unreachable!("Stmt::Init reached emit_stmt(Comb) — typecheck bug");
            }
            let rst_name = &ib.reset_signal.name;
            let is_low = ctx
                .reset_levels
                .get(rst_name.as_str())
                .map_or(false, |level| *level == ResetLevel::Low);
            let cond = if is_low {
                format!("(!{})", rst_name)
            } else {
                rst_name.clone()
            };
            out.push_str(&format!("{}if ({}) {{\n", ind(indent), cond));
            emit_stmts(&ib.body, ctx, out, indent + 1, k);
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        Stmt::WaitUntil(_, _) | Stmt::DoUntil { .. } => {
            if !is_seq {
                unreachable!("Stmt::WaitUntil/DoUntil reached emit_stmt(Comb) — typecheck bug");
            }
            // Pipeline wait-stage seq blocks are emitted by `gen_pipeline`;
            // the generic module stmt walker should never lower them.
            unreachable!("Stmt::WaitUntil/DoUntil reached generic sim stmt emitter");
        }
    }
}

pub(super) fn emit_if_else(
    ie: &IfElse,
    ctx: &Ctx,
    out: &mut String,
    indent: usize,
    is_chain: bool,
    k: SimAssignKind,
) {
    let cond = cpp_condition(&ie.cond, ctx);
    if is_chain {
        out.push_str(&format!("{}}} else if {} {{\n", ind(indent), cond));
    } else {
        out.push_str(&format!("{}if {} {{\n", ind(indent), cond));
    }
    // --coverage: count entries to this arm. Phase 1 records branch
    // coverage for seq if/elsif/else; phase 1b/c adds comb. Counter id
    // is the alloc order in the per-class registry.
    //
    // Note: comb blocks may evaluate multiple times per cycle during
    // the settle loop — counters therefore reflect "branch entries",
    // not "cycles where branch was active". For most designs the settle
    // loop converges in 1–2 iterations so this is close to cycle count.
    if let Some(reg) = ctx.coverage {
        let kind = if is_chain { "elsif" } else { "if" };
        let idx = reg
            .borrow_mut()
            .alloc(kind, ie.cond.span.start, String::new());
        out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
    }
    emit_stmts(&ie.then_stmts, ctx, out, indent + 1, k);
    if ie.else_stmts.len() == 1 {
        if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
            emit_if_else(nested, ctx, out, indent, true, k);
            return;
        }
    }
    if !ie.else_stmts.is_empty() {
        out.push_str(&format!("{}}} else {{\n", ind(indent)));
        if let Some(reg) = ctx.coverage {
            let idx = reg.borrow_mut().alloc("else", ie.span.end, String::new());
            out.push_str(&format!("{}  _arch_cov[{idx}]++;\n", ind(indent)));
        }
        emit_stmts(&ie.else_stmts, ctx, out, indent + 1, k);
    }
    out.push_str(&format!("{}}}\n", ind(indent)));
}

pub(super) fn emit_reg_stmts(stmts: &[Stmt], ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmts(stmts, ctx, out, indent, SimAssignKind::Seq);
}

pub(super) fn emit_reg_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmt(stmt, ctx, out, indent, SimAssignKind::Seq);
}

#[allow(dead_code)]
pub(super) fn emit_reg_if_else(
    ie: &IfElse,
    ctx: &Ctx,
    out: &mut String,
    indent: usize,
    is_chain: bool,
) {
    emit_if_else(ie, ctx, out, indent, is_chain, SimAssignKind::Seq);
}

pub(super) fn emit_comb_stmts(stmts: &[Stmt], ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmts(stmts, ctx, out, indent, SimAssignKind::Comb);
}

#[allow(dead_code)]
pub(super) fn emit_comb_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize) {
    emit_stmt(stmt, ctx, out, indent, SimAssignKind::Comb);
}

#[allow(dead_code)]
pub(super) fn emit_comb_if_else(
    ie: &IfElse,
    ctx: &Ctx,
    out: &mut String,
    indent: usize,
    is_chain: bool,
) {
    emit_if_else(ie, ctx, out, indent, is_chain, SimAssignKind::Comb);
}

pub(super) fn emit_log_stmt(l: &LogStmt, ctx: &Ctx, out: &mut String, indent: usize) {
    let args_str: String = l
        .args
        .iter()
        .map(|a| format!(", (long long)({})", cpp_expr(a, ctx)))
        .collect();
    let fmt = sv_fmt_to_printf(&l.fmt);
    let print_line = if let Some(ref path) = l.file {
        let fd_name = log_fd_name(path);
        format!(
            "{}if ({fd_name}) fprintf({fd_name}, \"[{}][{}] {}\\n\"{});",
            ind(indent),
            l.level.name(),
            l.tag,
            fmt,
            args_str
        )
    } else {
        format!(
            "{}printf(\"[{}][{}] {}\\n\"{});",
            ind(indent),
            l.level.name(),
            l.tag,
            fmt,
            args_str
        )
    };
    if l.level == LogLevel::Always {
        out.push_str(&print_line);
        out.push('\n');
    } else {
        out.push_str(&format!(
            "{}if (Verilated::verbosity() >= {}) {{ {} }}\n",
            ind(indent),
            l.level.value(),
            print_line
        ));
    }
}

/// Generate a C++ file pointer name from a log file path.
pub(super) fn log_fd_name(path: &str) -> String {
    let clean: String = path
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("_log_fd_{clean}")
}
