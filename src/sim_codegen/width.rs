//! `width` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

/// Get the bit-width of a TypeExpr — packed total (Vec recurses and multiplies).
/// Use this when you need the *total* number of bits to fit in storage
/// (e.g. for VCD trace width, packed signal width).
/// Returns 32 for unhandled types (Named structs).
///
/// Distinguishing the three width helpers in this file:
/// - `type_width(ty)`: packed total, recurses into Vec, defaults to 32
/// - `type_width_of(ty)`: same but returns 0 (not 32) for Vec/Named — used by `--debug`
///   shadow generation where 0 signals "skip this port"
/// - `type_bits_te(ty)`: scalar-only width (does NOT recurse into Vec), defaults to 32 —
///   used for inst port width tracking where Vec is handled separately via flat fields
#[deprecated(
    note = "use `type_width_with_params(.., &params)` — the bare form silently \
            miscompiles when the type depends on enclosing-construct params \
            (UInt<PARAM>, Vec<_, PARAM>). See arch-com#447 §1 and PR #463 \
            extending #458 to the sibling helper cluster."
)]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
pub(super) fn type_width(ty: &TypeExpr) -> u32 {
    type_width_with_params(ty, &[])
}

/// Param-aware variant of [`type_width`]. Resolves `UInt<PARAM>` /
/// `SInt<PARAM>` widths via param defaults. Used by trace-signal emission
/// (`build_trace_signals`) so VCD `$var wire N` widths reflect the actual
/// HDL bit width rather than the legacy 32-default. arch-com#330.
pub(super) fn type_width_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width_with_params(w, params),
        TypeExpr::Bool => 1,
        TypeExpr::Bit => 1,
        TypeExpr::Clock(_) => 1,
        TypeExpr::Reset { .. } => 1,
        TypeExpr::Vec(elem, count) => {
            type_width_with_params(elem, params) * eval_width_with_params(count, params)
        }
        _ => 32,
    }
}

/// Evaluate a simple constant expression to a u32 bit-width.
pub(super) fn eval_width(expr: &Expr) -> u32 {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(n)) => *n as u32,
        ExprKind::Literal(LitKind::Hex(n)) => *n as u32,
        ExprKind::Clog2(inner) => {
            let v = eval_width(inner);
            if v <= 1 {
                1
            } else {
                32 - (v - 1).leading_zeros()
            }
        }
        _ => 32,
    }
}

/// Param-aware width evaluator: folds bare `Ident` and arithmetic over
/// param defaults via `eval_const_expr_with_params`. Used in
/// width-bearing positions where `BitSlice` hi/lo or `PartSelect` width
/// may reference a param (e.g. `[CounterWidth-1:0]`). Falls back to the
/// legacy `eval_width` for shapes the const evaluator can't fold (which
/// preserves prior conservative-32 behavior).
pub(super) fn eval_width_in(expr: &Expr, ctx: &Ctx) -> u32 {
    try_eval_const_expr_with_params(expr, ctx.params)
        .map(|v| v as u32)
        .unwrap_or_else(|| eval_width(expr))
}

/// Number of 32-bit words needed for `bits` bits.
pub(super) fn wide_words(bits: u32) -> u32 {
    (bits + 31) / 32
}

/// True if a signal width requires a wide (VlWide) type.
pub(super) fn is_wide_bits(bits: u32) -> bool {
    bits > 64
}

/// C++ type for a public port field.
#[deprecated(note = "use `cpp_port_type_with_params(.., &params)` — the bare form \
            silently buckets `UInt<PARAM>` into uint32_t even when the param \
            resolves to a wider value. See arch-com#447 §1 and PR #463 \
            extending #458 to the sibling helper cluster.")]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
pub(super) fn cpp_port_type(ty: &TypeExpr) -> String {
    cpp_port_type_with_params(ty, &[])
}

pub(super) fn ty_references_named(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Named(_) => true,
        TypeExpr::Vec(inner, _) => ty_references_named(inner),
        _ => false,
    }
}

/// Param-aware variant of [`cpp_port_type`]. Resolves param identifiers in
/// `UInt<W>` / `SInt<W>` widths via [`eval_const_expr_with_params`] so a
/// `UInt<ACC_WIDTH>` declaration (with `param ACC_WIDTH: const = 48`) gets
/// the right C++ bucket (e.g. `uint64_t` for 33..=64 bits). The legacy
/// `cpp_port_type` falls back to `eval_width`, which returns 32 for any
/// non-literal width and silently truncates 33..=64-bit fields to
/// `uint32_t`. arch-com#330.
pub(super) fn cpp_port_type_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> String {
    match ty {
        TypeExpr::UInt(w) => {
            let b = eval_width_with_params(w, params);
            if is_wide_bits(b) {
                format!("VlWide<{}>", wide_words(b))
            } else {
                cpp_uint(b).to_string()
            }
        }
        TypeExpr::SInt(w) => {
            let b = eval_width_with_params(w, params);
            if is_wide_bits(b) {
                format!("VlWide<{}>", wide_words(b))
            } else {
                cpp_sint(b).to_string()
            }
        }
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => {
            "uint8_t".to_string()
        }
        // Floats are carried as their raw bit pattern in an unsigned integer
        // (FP32 → uint32_t, BF16 → uint16_t); arithmetic goes through the
        // `_arch_fp.h` helpers, never C++ float operators on the storage.
        TypeExpr::FP32 => "uint32_t".to_string(),
        TypeExpr::BF16 => "uint16_t".to_string(),
        TypeExpr::FP8E4M3
        | TypeExpr::FP8E5M2
        | TypeExpr::FP4E2M1
        | TypeExpr::FP6E2M3
        | TypeExpr::FP6E3M2
        | TypeExpr::E8M0 => "uint8_t".to_string(),
        TypeExpr::Named(n) => n.name.clone(),
        TypeExpr::Vec(_, _) => "uint32_t".to_string(),
    }
}

/// Param-aware width eval used by the type-emission helpers. Folds bare
/// `Ident` and basic arithmetic over `params` defaults; falls back to the
/// legacy literal-only `eval_width` for shapes the const evaluator can't
/// fold (preserving prior conservative-32 behavior). arch-com#330.
pub(super) fn eval_width_with_params(expr: &Expr, params: &[ParamDecl]) -> u32 {
    try_eval_const_expr_with_params(expr, params)
        .map(|v| v as u32)
        .unwrap_or_else(|| eval_width(expr))
}

/// C++ type for a private reg/let field.
/// 1–64 bits   → uint8/16/32/64_t
/// 65–128 bits → _arch_u128
/// >128 bits   → VlWide<N>  (same as port type, no conversion needed)
#[deprecated(
    note = "use `cpp_internal_type_with_params(.., &params)` — the bare form \
            silently buckets `UInt<PARAM>` regs/lets into the wrong scalar \
            type. See arch-com#447 §1 and PR #463 extending #458 to the \
            sibling helper cluster."
)]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
pub(super) fn cpp_internal_type(ty: &TypeExpr) -> String {
    cpp_internal_type_with_params(ty, &[])
}

/// Param-aware variant of [`cpp_internal_type`]. See [`cpp_port_type_with_params`]
/// for rationale — without param resolution, `UInt<ACC_WIDTH>` regs/lets
/// get the wrong C++ scalar type. arch-com#330.
pub(super) fn cpp_internal_type_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> String {
    match ty {
        TypeExpr::UInt(w) => {
            let b = eval_width_with_params(w, params);
            if b > 128 {
                format!("VlWide<{}>", wide_words(b))
            } else if b > 64 {
                "_arch_u128".to_string()
            } else {
                cpp_uint(b).to_string()
            }
        }
        TypeExpr::SInt(w) => {
            let b = eval_width_with_params(w, params);
            if b > 128 {
                format!("VlWide<{}>", wide_words(b))
            } else if b > 64 {
                "_arch_u128".to_string()
            } else {
                cpp_sint(b).to_string()
            }
        }
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => {
            "uint8_t".to_string()
        }
        TypeExpr::FP32 => "uint32_t".to_string(),
        TypeExpr::BF16 => "uint16_t".to_string(),
        TypeExpr::FP8E4M3
        | TypeExpr::FP8E5M2
        | TypeExpr::FP4E2M1
        | TypeExpr::FP6E2M3
        | TypeExpr::FP6E3M2
        | TypeExpr::E8M0 => "uint8_t".to_string(),
        TypeExpr::Named(n) => n.name.clone(),
        TypeExpr::Vec(_, _) => "uint32_t".to_string(),
    }
}

/// Declare a struct/bus-as-wire field. `Vec<T,N>` fields (including nested
/// `Vec<Vec<T,M>,N>`) emit as `std::array<T,N>` — see `cpp_std_array_type`
/// — rather than a raw C array; every other type is unaffected.
pub(super) fn cpp_field_decl(name: &str, ty: &TypeExpr, params: &[ParamDecl]) -> String {
    if let Some(arr_ty) = cpp_std_array_type(ty, params) {
        format!("{arr_ty} {name}")
    } else {
        format!("{} {name}", cpp_internal_type_with_params(ty, params))
    }
}

/// If `ty` is `Vec<T, N>` (possibly nested), return the C++
/// `std::array<...>` type string for a Vec-typed **struct/bus field**
/// (`cpp_field_decl`'s Vec branch — arch-com#500 Gap 3 / #759).
///
/// This is the "harc-style" carrier: `std::array` mirrors harc-com's
/// `HarcWide<N>::words` convention (`runtime/harc_thread_rt.h`) of storing
/// fixed-size hardware vectors as `std::array` rather than a raw C array.
/// Unlike a raw C array, `std::array` is copy-assignable, and pybind11's
/// `<pybind11/stl.h>` (already included by every generated `*_pybind.cpp`)
/// binds it to/from a Python list automatically — that's what makes
/// `.def_readwrite` on a `Vec<T,N>` struct field compile (it doesn't for a
/// raw array: "array type ... is not assignable"). See
/// `doc/proposal_vec_payload_interop.md` §2.3 / §4 and arch-com#759.
///
/// Nested Vecs (`Vec<Vec<T,M>,N>`) recurse outer-to-inner, matching the
/// nesting direction of the old `T name[N][M]` C-array form:
/// `std::array<std::array<T,M>,N>`.
///
/// Scope: this only changes the C++ *storage representation* of Vec-typed
/// fields inside generated `struct`/bus-as-wire C++ types
/// (`gen_structs_file`). It intentionally does NOT touch
/// `vec_array_info_with_params` (still returns the C-array-style
/// `"N][M"` count-string format used pervasively elsewhere for Vec-typed
/// module ports/regs) — those are a separate, much larger surface not in
/// scope here, and every element-indexed access pattern (`x.field[i]`)
/// already reads and writes identically for `std::array` and a raw array
/// via `operator[]`, so leaving them as C arrays is not a hazard.
pub(super) fn cpp_std_array_type(ty: &TypeExpr, params: &[ParamDecl]) -> Option<String> {
    if let TypeExpr::Vec(elem, count_expr) = ty {
        let n = eval_const_expr_with_params(count_expr, params);
        let inner = cpp_std_array_type(elem, params)
            .unwrap_or_else(|| cpp_internal_type_with_params(elem, params));
        Some(format!("std::array<{inner}, {n}>"))
    } else {
        None
    }
}

/// If `ty` is Vec<T, N>, return (elem_cpp_type, count_string).
///
/// Nested Vecs (e.g. `Vec<Vec<UInt<32>, 4>, 8>`) recurse: the innermost
/// non-Vec element type is returned as `elem_cpp_type`, and the count
/// string is the C-array dimension chain in source order separated by
/// `"]["` so a caller emitting `<elem>[<count>]` ends up with the
/// correct multi-dim C array (`uint32_t name[8][4]`).
#[deprecated(
    note = "use `vec_array_info_with_params(.., &params)` — the bare form \
            silently returns count=0 for `Vec<_, PARAM>` declarations. See \
            arch-com#447 §1 and PR #463 extending #458 to the sibling \
            helper cluster (twin of the PR #442 sites for the Vec-reg \
            storage path)."
)]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
pub(super) fn vec_array_info(ty: &TypeExpr) -> Option<(String, String)> {
    // Backward-compatible wrapper: delegate to the param-aware version
    // with an empty params slice. Callers that need to resolve a
    // `Vec<_, PARAM_NAME>` count expression against an enclosing
    // construct's params must use `vec_array_info_with_params`
    // directly — see arch-com#447 §1.
    vec_array_info_with_params(ty, &[])
}

/// Param-aware variant of [`vec_array_info`]. Uses
/// [`eval_const_expr_with_params`] so that `Vec<_, NUM_ENTRIES>` style
/// declarations whose count is a param identifier resolve to the
/// param's literal default (rather than silently degrading to 0 and
/// emitting a zero-sized C++ scratch array, which corrupts the
/// surrounding stack on memcpy/index).
pub(super) fn vec_array_info_with_params(
    ty: &TypeExpr,
    params: &[ParamDecl],
) -> Option<(String, String)> {
    if let TypeExpr::Vec(elem, count_expr) = ty {
        let outer_count = eval_const_expr_with_params(count_expr, params).to_string();
        // Recursively descend nested Vecs — see vec_array_info docs.
        if let Some((inner_elem, inner_dims)) = vec_array_info_with_params(elem, params) {
            Some((inner_elem, format!("{outer_count}][{inner_dims}")))
        } else {
            let elem_type = cpp_internal_type_with_params(elem, params);
            Some((elem_type, outer_count))
        }
    } else {
        None
    }
}

pub(super) fn cpp_uint(bits: u32) -> &'static str {
    if bits <= 8 {
        "uint8_t"
    } else if bits <= 16 {
        "uint16_t"
    } else if bits <= 32 {
        "uint32_t"
    } else {
        "uint64_t"
    }
}

/// Bit-width of a *scalar* TypeExpr, param-aware. Returns `None` for
/// aggregate / non-value types (Vec, Named, Clock, Reset) instead of
/// inventing a default, so callers can fall back explicitly. Unlike
/// `type_bits_te_with_params` (whose `_ => 32` arm is exactly the trap
/// arch#858 fell into for Vec), a `None` here can never be mistaken for
/// a real width.
pub(super) fn scalar_type_bits_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> Option<u32> {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => Some(eval_width_with_params(w, params)),
        TypeExpr::Bool | TypeExpr::Bit => Some(1),
        TypeExpr::FP32 => Some(32),
        TypeExpr::BF16 => Some(16),
        TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 | TypeExpr::E8M0 => Some(8),
        TypeExpr::FP6E2M3 | TypeExpr::FP6E3M2 => Some(6),
        TypeExpr::FP4E2M1 => Some(4),
        TypeExpr::Vec(..) | TypeExpr::Named(_) | TypeExpr::Clock(_) | TypeExpr::Reset(..) => None,
    }
}

/// Return the bit-width of a TypeExpr, or 0 if indeterminate (e.g. Vec with param size).
pub(super) fn type_width_of(ty: &TypeExpr) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => 1,
        TypeExpr::FP32 => 32,
        TypeExpr::BF16 => 16,
        TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 => 8,
        TypeExpr::FP4E2M1 => 4,
        TypeExpr::FP6E2M3 | TypeExpr::FP6E3M2 => 6,
        TypeExpr::E8M0 => 8,
        TypeExpr::Vec(..) | TypeExpr::Named(_) => 0,
    }
}

/// Smallest C++ signed integer type that fits `bits` (up to 64).
pub(super) fn cpp_sint(bits: u32) -> &'static str {
    if bits <= 8 {
        "int8_t"
    } else if bits <= 16 {
        "int16_t"
    } else if bits <= 32 {
        "int32_t"
    } else {
        "int64_t"
    }
}

/// Build a name→width map from module ports, regs, and lets.
pub(super) fn build_widths(
    ports: &[PortDecl],
    body: &[ModuleBodyItem],
    params: &[ParamDecl],
) -> HashMap<String, u32> {
    let mut m = HashMap::new();
    for p in ports {
        m.insert(p.name.name.clone(), type_bits_te_with_params(&p.ty, params));
    }
    // Compile-time-constant params participate in width inference the same
    // way let bindings do. Without this, `infer_expr_width` falls back to
    // its 8-bit default for any concat / shift expression that names a
    // param, silently producing 1-bit-off bit positions in emitted C++.
    for p in params {
        let bits = match &p.kind {
            ParamKind::WidthConst(hi, lo) => {
                let h = eval_width(hi);
                let l = eval_width(lo);
                h - l + 1
            }
            ParamKind::Logic(ty) | ParamKind::Type(ty) => type_bits_te_with_params(ty, params),
            // `param X: const = N` (untyped). Pre-existing call sites treat
            // this as an int-typed parameter (32 bits), so match.
            ParamKind::Const => 32,
            // Enum / Vec params: width depends on the underlying type. Skip
            // — concat-of-enum isn't a valid construct; concat-of-Vec is
            // handled elsewhere via vec_array_info_with_params.
            ParamKind::EnumConst(_) | ParamKind::ConstVec(_) => continue,
        };
        m.insert(p.name.name.clone(), bits);
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                m.insert(r.name.name.clone(), type_bits_te_with_params(&r.ty, params));
            }
            ModuleBodyItem::WireDecl(w) => {
                // Wires need width registration too — without this, downstream
                // sites that consult ctx.widths (the Bool `~` masking check
                // in cpp_expr's BitNot arm, infer_expr_width's Ident default,
                // …) silently fall back to "32" and produce broken codegen.
                // Symptom: `if ~bool_wire == false` emitted as
                // `(~(uint8_t)1) == 0` → `0xFE == 0` → never true.
                m.insert(w.name.name.clone(), type_bits_te_with_params(&w.ty, params));
            }
            ModuleBodyItem::LetBinding(l) => {
                // Destructuring: widths come from struct field types; these
                // are best-effort looked up at emission time. Leave them
                // out here; widths map defaults kick in if needed.
                if !l.destructure_fields.is_empty() {
                    continue;
                }
                if let Some(ty) = &l.ty {
                    m.insert(l.name.name.clone(), type_bits_te_with_params(ty, params));
                }
            }
            _ => {}
        }
    }
    // Resolve pipe_reg widths from their sources
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            let w = m.get(&p.source.name).copied().unwrap_or(32);
            for i in 0..p.stages {
                if i == p.stages - 1 {
                    m.insert(p.name.name.clone(), w);
                } else {
                    m.insert(format!("{}_stg{}", p.name.name, i + 1), w);
                }
            }
        }
    }
    m
}

#[deprecated(note = "use `type_bits_te_with_params(.., &params)` — the bare \
            form silently miscompiles when a `UInt<W>` / `SInt<W>` \
            width is a param identifier (returns the fallback width \
            of 32 rather than the param's resolved value). See \
            arch-com#447 §1 and PRs #427, #439, #442.")]
#[allow(dead_code)] // intentional landmine: present so new callers
                    // surface a deprecation warning at PR review time.
pub(super) fn type_bits_te(ty: &TypeExpr) -> u32 {
    type_bits_te_with_params(ty, &[])
}

/// Param-aware variant of [`type_bits_te`]. Resolves param idents in
/// `UInt<W>` / `SInt<W>` width positions so that `is_wide_bits` /
/// `collect_wide_names` classification works for `param`-derived widths
/// (e.g. `UInt<W>` with `param W = 96` must be classified wide, not 32).
/// arch-com#330.
pub(super) fn type_bits_te_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> u32 {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width_with_params(w, params),
        TypeExpr::Bool | TypeExpr::Bit => 1,
        TypeExpr::FP32 => 32,
        TypeExpr::BF16 => 16,
        TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 => 8,
        TypeExpr::FP4E2M1 => 4,
        _ => 32,
    }
}

pub(super) fn type_is_signed_scalar(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::SInt(_))
}

/// Floating-point format of a scalar TypeExpr, if any.
pub(super) fn type_float_fmt(ty: &TypeExpr) -> Option<FpFmt> {
    // Membership comes from the canonical table so a float type cannot be
    // a float everywhere except here; the FpFmt mapping stays explicit
    // because FpFmt is the sim backend's own vocabulary.
    let id = crate::fp_format::by_type_expr(ty)?.id;
    Some(match id {
        crate::fp_format::FpFormatId::Fp32 => FpFmt::Fp32,
        crate::fp_format::FpFormatId::Bf16 => FpFmt::Bf16,
        crate::fp_format::FpFormatId::E4m3 => FpFmt::E4m3,
        crate::fp_format::FpFormatId::E5m2 => FpFmt::E5m2,
        crate::fp_format::FpFormatId::E2m1 => FpFmt::E2m1,
        crate::fp_format::FpFormatId::E2m3 => FpFmt::E2m3,
        crate::fp_format::FpFormatId::E3m2 => FpFmt::E3m2,
    })
}
