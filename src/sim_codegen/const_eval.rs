//! `const_eval` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

/// Evaluate a constant expression to a u64, resolving basic arithmetic.
/// Backward-compatible wrapper that doesn't resolve param identifiers —
/// see [`eval_const_expr_with_params`] for the version that does. Use
/// the param-aware version anywhere a Vec / array length needs to fold
/// across `param N: const = …;` references (otherwise the result is 0
/// and downstream code emits zero-sized C++ arrays — see the regression
/// fixed in PR #cam-zero-array).
#[deprecated(note = "use `eval_const_expr_with_params(.., &params)` — the bare \
            form silently miscompiles when the expression depends on \
            enclosing-construct params (Vec<_, PARAM>, UInt<PARAM>, \
            etc.). See arch-com#447 §1 and PRs #427, #439, #442 for \
            the bug class this guards against.")]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
pub(super) fn eval_const_expr(expr: &Expr) -> u64 {
    eval_const_expr_with_params(expr, &[])
}

/// Param-aware constant evaluator. Resolves bare identifiers against
/// `params` (regular + local) by recursing on each param's `default`
/// expression. Handles literals, `$clog2(x)`, unary `-`/`~`, and
/// binary `+`, `-`, `*`, `/`, `%`, `<<`, `>>`, `&`, `|`, `^`. The public
/// wrapper preserves the historical "unknown folds to 0" behavior; callers
/// that need to distinguish a real zero from an unknown expression use the
/// `try_` variant directly.
pub(super) fn eval_const_expr_with_params(expr: &Expr, params: &[ParamDecl]) -> u64 {
    try_eval_const_expr_with_params(expr, params).unwrap_or(0)
}

pub(super) fn try_eval_const_expr_with_params(expr: &Expr, params: &[ParamDecl]) -> Option<u64> {
    try_eval_const_expr_with_params_seen(expr, params, &mut HashSet::new())
}

/// Return true if the module body contains a preserved `Generate(For)`
/// block — these survive elaboration when the for-loop's range
/// depends on a module param and the body is shape-stable. Sim codegen
/// has no SV-genvar concept, so we run a local unroll pass before
/// walking the body.
pub(super) fn module_body_has_preserved_generate(body: &[ModuleBodyItem]) -> bool {
    body.iter()
        .any(|it| matches!(it, ModuleBodyItem::Generate(GenerateDecl::For(_))))
}

/// Sim-local unroll for preserved `Generate(For)` blocks. Walks the
/// module body, expands each `For` loop's `Inst` items (and any other
/// generate-item kinds the elaborator's "shape-stable" gate may admit
/// in the future) into flat ModuleBodyItem entries. The expansion is
/// purely sim-local — the source AST is not mutated.
///
/// The elaborator's preservation gate only admits ranges that
/// `eval_const_expr_with_params` can resolve against the parent module's
/// param defaults. If we hit a range we can't evaluate, we leave the
/// generate intact (which would silently drop the body in downstream
/// sim walks); that case shouldn't occur given the preservation gate,
/// but we'd rather notice it as a broken sim than crash.
pub(super) fn flatten_preserved_generates_for_sim(
    body: &[ModuleBodyItem],
    params: &[ParamDecl],
) -> Vec<ModuleBodyItem> {
    let mut out = Vec::with_capacity(body.len());
    for item in body {
        match item {
            ModuleBodyItem::Generate(GenerateDecl::For(gf)) => {
                // Resolve range bounds against the module's param defaults.
                let start = eval_const_expr_with_params(&gf.start, params) as i64;
                let end = eval_const_expr_with_params(&gf.end, params) as i64;
                if end < start {
                    // Empty range — emit nothing.
                    continue;
                }
                let var = &gf.var.name;
                for i in start..=end {
                    for git in &gf.items {
                        match git {
                            GenItem::Inst(inst) => {
                                out.push(ModuleBodyItem::Inst(crate::elaborate::subst_inst(
                                    inst, var, i,
                                )));
                            }
                            // The elaborator's preservation gate today
                            // restricts preserved generate_for bodies
                            // to inst-only (with shape-stable connections).
                            // Other GenItem kinds would have been unrolled
                            // at elaboration time. If a future expansion
                            // of the gate admits more kinds, extend here.
                            _ => {
                                // Conservative: ignore (matches pre-#399
                                // sim behavior for these kinds; the
                                // elaborator wouldn't reach here today).
                            }
                        }
                    }
                }
            }
            other => out.push(other.clone()),
        }
    }
    out
}

pub(super) fn try_eval_const_expr_with_params_seen(
    expr: &Expr,
    params: &[ParamDecl],
    seen_params: &mut HashSet<String>,
) -> Option<u64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v)) => Some(*v),
        ExprKind::Literal(LitKind::Hex(v)) => Some(*v),
        ExprKind::Literal(LitKind::Bin(v)) => Some(*v),
        ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v),
        ExprKind::Ident(name) => {
            if let Some(p) = params.iter().find(|p| p.name.name == *name) {
                if let Some(d) = &p.default {
                    if !seen_params.insert(name.clone()) {
                        return None;
                    }
                    let value = try_eval_const_expr_with_params_seen(d, params, seen_params);
                    seen_params.remove(name);
                    return value;
                }
            }
            None
        }
        ExprKind::Clog2(a) => {
            let v = try_eval_const_expr_with_params_seen(a, params, seen_params)?;
            Some(if v <= 1 {
                0
            } else {
                64 - (v - 1).leading_zeros() as u64
            })
        }
        ExprKind::Unary(op, a) => {
            let v = try_eval_const_expr_with_params_seen(a, params, seen_params)?;
            match op {
                UnaryOp::Not => Some(!v),
                UnaryOp::Neg => Some(v.wrapping_neg()),
                _ => None,
            }
        }
        ExprKind::Binary(op, l, r) => {
            let lv = try_eval_const_expr_with_params_seen(l, params, seen_params)?;
            let rv = try_eval_const_expr_with_params_seen(r, params, seen_params)?;
            match op {
                BinOp::Add => Some(lv.wrapping_add(rv)),
                BinOp::Sub => Some(lv.wrapping_sub(rv)),
                BinOp::Mul => Some(lv.wrapping_mul(rv)),
                BinOp::Div => (rv != 0).then_some(lv / rv),
                BinOp::Mod => (rv != 0).then_some(lv % rv),
                BinOp::Shl => Some(lv.wrapping_shl(rv as u32)),
                BinOp::Shr => Some(lv.wrapping_shr(rv as u32)),
                BinOp::BitAnd => Some(lv & rv),
                BinOp::BitOr => Some(lv | rv),
                BinOp::BitXor => Some(lv ^ rv),
                BinOp::Eq => Some(if lv == rv { 1 } else { 0 }),
                BinOp::Neq => Some(if lv != rv { 1 } else { 0 }),
                BinOp::Lt => Some(if lv < rv { 1 } else { 0 }),
                BinOp::Gt => Some(if lv > rv { 1 } else { 0 }),
                BinOp::Lte => Some(if lv <= rv { 1 } else { 0 }),
                BinOp::Gte => Some(if lv >= rv { 1 } else { 0 }),
                BinOp::And => Some(if lv != 0 && rv != 0 { 1 } else { 0 }),
                BinOp::Or => Some(if lv != 0 || rv != 0 { 1 } else { 0 }),
                _ => None,
            }
        }
        ExprKind::Ternary(cond, then_expr, else_expr) => {
            let c = try_eval_const_expr_with_params_seen(cond, params, seen_params)?;
            if c != 0 {
                try_eval_const_expr_with_params_seen(then_expr, params, seen_params)
            } else {
                try_eval_const_expr_with_params_seen(else_expr, params, seen_params)
            }
        }
        _ => None,
    }
}

pub(super) fn try_eval_fp_const_expr_with_params_seen(
    expr: &Expr,
    params: &[ParamDecl],
    seen_params: &mut HashSet<String>,
) -> Option<f64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Float(f64_bits)) => Some(f64::from_bits(*f64_bits)),
        ExprKind::Literal(LitKind::TypedFloat(fmt, bits)) => {
            // Typed float already has target bits, convert back to f64 via its value?
            // For simplicity, treat as f64 from bits if FP32, or approximate for BF16
            match fmt {
                crate::ast::FloatLitFmt::Fp32 => {
                    let f32_bits = *bits as u32;
                    Some(f32::from_bits(f32_bits) as f64)
                }
                crate::ast::FloatLitFmt::Bf16 => {
                    let bf16_bits = *bits as u16;
                    // BF16 -> FP32 -> f64: {bf16, 16'b0} with NaN handling
                    let f32_bits = (bf16_bits as u32) << 16;
                    // Canonicalize NaN like arch_bf16_to_f32 does: if exponent all 1 and mantissa non-zero, return NaN
                    let exp = (f32_bits >> 23) & 0xFF;
                    let mant = f32_bits & 0x7FFFFF;
                    let canon = if exp == 0xFF && mant != 0 {
                        f32::from_bits(0x7FC00000) as f64
                    } else {
                        f32::from_bits(f32_bits) as f64
                    };
                    Some(canon)
                }
                crate::ast::FloatLitFmt::E4m3 => Some(crate::fp_lit::e4m3_bits_to_f64(*bits as u8)),
                crate::ast::FloatLitFmt::E5m2 => Some(crate::fp_lit::e5m2_bits_to_f64(*bits as u8)),
                crate::ast::FloatLitFmt::E2m1 => Some(crate::fp_lit::e2m1_bits_to_f64(*bits as u8)),
                crate::ast::FloatLitFmt::E2m3 => Some(crate::fp_lit::e2m3_bits_to_f64(*bits as u8)),
                crate::ast::FloatLitFmt::E3m2 => Some(crate::fp_lit::e3m2_bits_to_f64(*bits as u8)),
            }
        }
        ExprKind::Ident(name) => {
            if let Some(p) = params.iter().find(|p| p.name.name == *name) {
                if let Some(d) = &p.default {
                    if !seen_params.insert(name.clone()) {
                        return None;
                    }
                    // Try FP eval first, then fall back to int eval converted to f64
                    let value = try_eval_fp_const_expr_with_params_seen(d, params, seen_params)
                        .or_else(|| {
                            try_eval_const_expr_with_params_seen(d, params, seen_params)
                                .map(|v| v as f64)
                        });
                    seen_params.remove(name);
                    return value;
                }
            }
            None
        }
        ExprKind::MethodCall(receiver, method, _args) => {
            let method_name = method.name.as_str();
            if method_name == "to_fp32" {
                if let Some(int_val) =
                    try_eval_const_expr_with_params_seen(receiver, params, seen_params)
                {
                    return Some(int_val as f64);
                }
                if let Some(fp_val) =
                    try_eval_fp_const_expr_with_params_seen(receiver, params, seen_params)
                {
                    return Some(fp_val);
                }
            } else if method_name == "to_bf16" {
                // int -> bf16 is f32-routed per spec (double rounding): int -> f32 (RNE) -> bf16 (RNE)
                if let Some(int_val) =
                    try_eval_const_expr_with_params_seen(receiver, params, seen_params)
                {
                    let f32_bits = crate::fp_lit::f64_to_fp32_bits(int_val as f64);
                    let f32_val = f32::from_bits(f32_bits) as f64;
                    return Some(f32_val);
                }
                if let Some(fp_val) =
                    try_eval_fp_const_expr_with_params_seen(receiver, params, seen_params)
                {
                    return Some(fp_val);
                }
            }
            None
        }
        ExprKind::Unary(op, a) => {
            let v = try_eval_fp_const_expr_with_params_seen(a, params, seen_params)?;
            match op {
                UnaryOp::Neg => Some(-v),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn try_eval_fp_const_expr_with_params(expr: &Expr, params: &[ParamDecl]) -> Option<f64> {
    try_eval_fp_const_expr_with_params_seen(expr, params, &mut HashSet::new())
}

/// Evaluate a param's default value as a u64 bit pattern for C++ `#define`
/// emission. Handles both integer params (direct int eval) and FP params
/// (FP32/BF16 via FP evaluator + bit pattern conversion, with BF16 double-rounding).
/// Returns None if the param has no default or cannot be evaluated.
pub(super) fn eval_param_const_value(p: &ParamDecl, params: &[ParamDecl]) -> Option<u64> {
    let def = p.default.as_ref()?;
    // Check if this param is FP typed
    let is_bf16 = matches!(&p.kind, ParamKind::Logic(ty) if matches!(ty, TypeExpr::BF16));
    let is_fp32 = matches!(&p.kind, ParamKind::Logic(ty) if matches!(ty, TypeExpr::FP32));
    let is_e4m3 = matches!(&p.kind, ParamKind::Logic(ty) if matches!(ty, TypeExpr::FP8E4M3));
    let is_e5m2 = matches!(&p.kind, ParamKind::Logic(ty) if matches!(ty, TypeExpr::FP8E5M2));
    let is_fp = is_bf16 || is_fp32 || is_e4m3 || is_e5m2;

    if is_fp {
        if let Some(fval) = try_eval_fp_const_expr_with_params(def, params) {
            let bits = if is_bf16 {
                // fval already accounts for double-rounding for int->bf16 via FP evaluator
                crate::fp_lit::f64_to_bf16_bits(fval) as u64
            } else if is_e4m3 {
                // Overflowing fp8 literals are a compile error upstream
                // (elaborate coercion); None here means the default wasn't a
                // representable constant — fall through to the int path.
                match crate::fp_lit::f64_to_e4m3_bits(fval) {
                    Some(b) => b as u64,
                    None => return None,
                }
            } else if is_e5m2 {
                match crate::fp_lit::f64_to_e5m2_bits(fval) {
                    Some(b) => b as u64,
                    None => return None,
                }
            } else {
                crate::fp_lit::f64_to_fp32_bits(fval) as u64
            };
            return Some(bits);
        }
        // Fallback: int eval then convert. For BF16, must double-round via f32
        // per spec (int -> f32 RNE -> bf16 RNE), not single-round f64->bf16.
        let int_val = eval_const_expr_with_params(def, params);
        // Only fallback if int eval looks plausible (non-zero or int-like expr)
        if int_val != 0
            || matches!(
                &def.kind,
                ExprKind::Literal(_)
                    | ExprKind::Ident(_)
                    | ExprKind::Binary(_, _, _)
                    | ExprKind::Clog2(_)
            )
        {
            if is_bf16 {
                // Double-round: int -> f32 (RNE) -> bf16 (RNE)
                let f32_bits = crate::fp_lit::f64_to_fp32_bits(int_val as f64);
                let f32_val = f32::from_bits(f32_bits);
                // f32 -> bf16 RNE via add trick (mirrors arch_f32_to_bf16)
                let u = f32_val.to_bits();
                let lsb = (u >> 16) & 1;
                let bf16_bits = (u.wrapping_add(0x7FFF + lsb) >> 16) as u16;
                return Some(bf16_bits as u64);
            } else if is_e4m3 {
                // In-range ints are exact in f32, so single-rounding the f64
                // is identical to the f32-routed double rounding here.
                return crate::fp_lit::f64_to_e4m3_bits(int_val as f64).map(|b| b as u64);
            } else if is_e5m2 {
                return crate::fp_lit::f64_to_e5m2_bits(int_val as f64).map(|b| b as u64);
            } else {
                let bits = crate::fp_lit::f64_to_fp32_bits(int_val as f64) as u64;
                return Some(bits);
            }
        }
        None
    } else {
        Some(eval_const_expr_with_params(def, params))
    }
}

/// If `expr` is a bare identifier, return its name — used for diagnostic
/// location strings in runtime bounds-check codegen.
pub(super) fn base_ident_name(expr: &Expr) -> Option<&str> {
    if let ExprKind::Ident(n) = &expr.kind {
        Some(n.as_str())
    } else {
        None
    }
}

/// Local "is this expression a compile-time constant we can fold?" test.
/// Conservative: handles literals, `$clog2(const)`, and arithmetic over
/// already-reducible subtrees. Does NOT try to resolve param identifiers —
/// those are handled by the typecheck div-zero gate; here we return false
/// so the runtime `_ARCH_DCHK` still fires, which is safe (a non-zero param
/// just means the check succeeds silently).
pub(super) fn is_const_reducible(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(_) => true,
        ExprKind::Clog2(a) => is_const_reducible(a),
        ExprKind::Binary(_, a, b) => is_const_reducible(a) && is_const_reducible(b),
        ExprKind::Unary(_, a) => is_const_reducible(a),
        _ => false,
    }
}
