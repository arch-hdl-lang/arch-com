//! Block-scaled (`ScaledVec`) operations — the `scaled_quantize` /
//! `scaled_dequantize` / `scaled_dot` lowering shared by `arch build` and
//! `arch sim`.
//!
//! **Why one module emits both backends.** The numerics here are *not* new:
//! every float operation used below is an existing helper with an exhaustive
//! SMT miter behind it (`arch_e8m0_to_f32`, `arch_f32_to_e8m0`,
//! `arch_f32_mul`, `arch_f32_to_<elem>`, `arch_<elem>_to_f32` — see
//! `src/fp_ops.rs` and `src/fp_smt_proof.rs`). What is new is the *glue*:
//! a magnitude reduction, an exponent subtraction, and the bit packing. Glue
//! is exactly where an SV emitter and a C++ emitter written in different
//! files drift, so both renderings live here, adjacent, generated from one
//! descriptor. `fp_block_sv_and_cpp_agree_on_shape` pins their structure and
//! the `tests/fp_v1/rtl_diff` harness pins their *values* by running the
//! emitted SV under Verilator against this same C++ as the DPI reference.
//!
//! **The glue carries no float semantics of its own.** Two deliberate
//! choices make that true:
//!
//! 1. The block maximum is an **unsigned integer** max over the sign-cleared
//!    words. For non-negative IEEE-754, bit-pattern order *is* numeric order,
//!    and every NaN sorts above `+Inf` — so one integer compare yields both
//!    the magnitude maximum and the "any non-finite in this block" test, with
//!    no float compare and therefore no NaN-ordering subtlety to get wrong.
//! 2. The per-element division by the shared scale is done as a multiply by
//!    `2^-(code-127) == 2^((254-code)-127)`, i.e. by *another E8M0 code*.
//!    A power-of-two multiply is exact in FP32, so the element narrow that
//!    follows rounds exactly once, from the true `v_i / X`. That single
//!    rounding is what makes the round-trip error bound provable (arch#884).
//!
//! `scaled_dot` (phase 3) rests on the same principle from the other side:
//! every element-pair product is *exact* in FP32 — machine-checked
//! exhaustively per format by `fp_smt_proof::MX_DOT` — so all of a block dot's
//! rounding is in the summation. That is what makes [`dot_schedule`]'s choice
//! of accumulation order the only implementation freedom left, and therefore
//! worth defining rather than leaving open.
//!
//! Layout is the one fixed in phase 2a (`fp_format::scaled_vec_width`):
//! `{ scale[SW-1:0], P[N-1], …, P[1], P[0] }` — scale in the high bits,
//! element `i` at `[i*EW +: EW]`.

use crate::ast::{RoundMode, ScalePolicy, TypeExpr};
use crate::fp_format::{by_type_expr, FpFormatId};
use std::fmt::Write as _;

/// The scale format of a block: `E8M0` (OCP MX) or `UE4M3` (NVFP4).
///
/// **Everything the lowering knows about a scale lives behind this enum.**
/// That is not decoration — it is the mechanism that makes adding a variant
/// safe. The lowerings below previously named `arch_f32_to_e8m0` and friends
/// as string literals, so adding `Ue4m3` compiled with **zero errors** and
/// would have emitted E8M0 scale decoding for an NVFP4 block: no diagnostic,
/// no compile error, and both backends wrong identically, so the SV↔sim
/// differential gate would not have caught it either (the same failure mode
/// as arch#904). Every method here is an exhaustive `match`, so a second
/// variant now fails to compile at each genuinely scale-specific decision.
///
/// The decisions that are E8M0-specific — and therefore each need a real
/// answer for `UE4M3`, not a copied one:
///
/// - **the NaN code** (`0xFF` here; `UE4M3`'s is `0x7F`, and `0xFF` would
///   additionally set the padding bit the format requires to be zero);
/// - **the "smallest scale" code** used for an all-zero block (E8M0 has no
///   zero, so `0x00` is the minimum scale 2^-127; `UE4M3`'s `0x00` IS zero);
/// - **the reciprocal**, exact here because negating a power-of-two exponent
///   is exact, with no analogue for a scale carrying a mantissa (arch#905);
/// - **deriving the scale from the block maximum**, which here is exponent
///   arithmetic on the code itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockScale {
    E8m0,
    Ue4m3,
}

impl BlockScale {
    fn tag(self) -> &'static str {
        match self {
            BlockScale::E8m0 => "e8m0",
            BlockScale::Ue4m3 => "ue4m3",
        }
    }
    fn width(self) -> u32 {
        match self {
            BlockScale::E8m0 => 8,
            // 7 significant bits, stored in 8 with the MSB padded zero (PTX).
            // The carrier is 8 bits wide, which is why the padding bit has to
            // be masked rather than assumed — see `nan_code`.
            BlockScale::Ue4m3 => 8,
        }
    }

    /// IR helper widening a scale code to FP32.
    fn widen_fn(self) -> &'static str {
        match self {
            BlockScale::E8m0 => "arch_e8m0_to_f32",
            BlockScale::Ue4m3 => "arch_ue4m3_to_f32",
        }
    }

    /// IR helper narrowing an FP32 to a scale code.
    fn narrow_fn(self) -> &'static str {
        match self {
            BlockScale::E8m0 => "arch_f32_to_e8m0",
            BlockScale::Ue4m3 => "arch_f32_to_ue4m3",
        }
    }

    /// The code that marks the whole block NaN.
    fn nan_code(self) -> u32 {
        match self {
            BlockScale::E8m0 => 0xFF,
            // NOT 0xFF: that would set the padding bit UE4M3 requires to be
            // zero. Its sole NaN is 0x7F.
            BlockScale::Ue4m3 => 0x7F,
        }
    }

    /// The code an all-zero block gets. E8M0 has no zero encoding, so this is
    /// its *minimum scale* (2^-127) rather than a zero — the element plane is
    /// what makes the block zero.
    fn zero_block_code(self) -> u32 {
        match self {
            BlockScale::E8m0 => 0x00,
            // UE4M3 *does* have a zero, and `0x00` IS it — so unlike E8M0 this
            // makes the block's scale genuinely zero. Sound because the branch
            // is only taken when every element is zero, and `0 * 0 == +0`.
            BlockScale::Ue4m3 => 0x00,
        }
    }

    /// The largest code that is still a finite scale — the clamp for a
    /// policy that rounds the scale up.
    fn max_finite_code(self) -> u32 {
        match self {
            BlockScale::E8m0 => 0xFE,
            BlockScale::Ue4m3 => 0x7E,
        }
    }

    /// Is every value of this scale a power of two?
    ///
    /// Drives two user-visible rules: the `exact` scale policy is refused for
    /// a power-of-two scale (there is nothing for it to do), and the
    /// single-rounding argument in the spec is stated differently for each.
    pub fn is_pow2(self) -> bool {
        match self {
            BlockScale::E8m0 => true,
            BlockScale::Ue4m3 => false,
        }
    }

    /// The scale policy used when the call site does not write one.
    ///
    /// Not a single global default: `floor_pow2` (proposal §9 decision #1)
    /// throws away every mantissa bit of a `UE4M3` scale, which would make an
    /// NVFP4 block numerically a power-of-two-scale format and diverge from
    /// every shipping NVFP4 implementation. Maintainer sign-off 2026-08-12.
    pub fn default_policy(self) -> ScalePolicy {
        match self {
            BlockScale::E8m0 => ScalePolicy::FloorPow2,
            BlockScale::Ue4m3 => ScalePolicy::Exact,
        }
    }

    /// Every finite value this scale can hold, ascending, indexed by code.
    ///
    /// Lives here rather than being derived from `fp_format::FORMATS` because
    /// **neither scale type has a row there** — deliberately: a format row is
    /// what makes the float-shaped paths (`is_float`, the arithmetic surface,
    /// `is_nan`) pick a type up, and a scale is a carrier for exponent-ish
    /// codes, not a float. That is the arch#837 hazard, hit five times
    /// already. So the shape is written out once, here, inside the enum that
    /// already owns every other scale-specific decision.
    fn grid(self) -> Vec<f64> {
        match self {
            // 2^(c-127) for every code but 0xFF (NaN). No zero, no sign.
            BlockScale::E8m0 => (0u32..=0xFE).map(|c| 2f64.powi(c as i32 - 127)).collect(),
            // E4M3-shaped magnitudes: 4 exponent bits, bias 7, 3 mantissa
            // bits, subnormals at 2^-9 granularity, every code but 0x7F.
            BlockScale::Ue4m3 => (0u32..=0x7E)
                .map(|c| {
                    let (e, m) = ((c >> 3) & 0xF, c & 0x7);
                    if e == 0 {
                        m as f64 * 2f64.powi(-9)
                    } else {
                        (8 + m) as f64 * 2f64.powi(e as i32 - 10)
                    }
                })
                .collect(),
        }
    }

    /// How elements are divided by the scale — the one decision that differs
    /// structurally rather than by a constant. See [`QuantKernel`].
    fn quant_kernel(self) -> QuantKernel {
        match self {
            BlockScale::E8m0 => QuantKernel::ExactReciprocal,
            BlockScale::Ue4m3 => QuantKernel::BoundaryCompare,
        }
    }

    /// SystemVerilog expression for the RECIPROCAL of the scale with code
    /// `code`, or `None` where no exact reciprocal exists.
    ///
    /// Exact for E8M0: `2^-(c-127) == 2^((254-c)-127)`, so the reciprocal is
    /// just another scale code and the element narrow downstream rounds
    /// exactly once. **A mantissa-bearing scale has no such identity** —
    /// arch#905 measured that substituting `v * fl(1/X)` for `fl(v/X)`
    /// changes the quantized code on 4.76% of tie-aligned inputs — so
    /// `UE4M3` returns `None` and uses [`QuantKernel::BoundaryCompare`].
    fn sv_reciprocal(self, code: &str) -> Option<String> {
        match self {
            BlockScale::E8m0 => Some(format!("{}(8'd254 - {code})", self.widen_fn())),
            BlockScale::Ue4m3 => None,
        }
    }

    /// C++ twin of [`BlockScale::sv_reciprocal`].
    fn cpp_reciprocal(self, code: &str) -> Option<String> {
        match self {
            BlockScale::E8m0 => Some(format!("_{}((uint8_t)(254u - {code}))", self.widen_fn())),
            BlockScale::Ue4m3 => None,
        }
    }
}

/// How `scaled_quantize` turns an FP32 element into an element code.
///
/// Both kernels are exactly correctly rounded; they differ in *how* they get
/// there, because only a power-of-two scale has an exact reciprocal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuantKernel {
    /// Multiply by `1/X`, which is itself exactly representable, then narrow.
    /// One multiply per element; the narrow is the sole rounding.
    ExactReciprocal,
    /// Never divide at all: compare `|v|` against `X × m` for each RNE
    /// decision boundary `m` of the element grid.
    ///
    /// Sound because **every such product is exact in FP32** — the scale
    /// carries 4 significand bits and a boundary at most 5, so the product
    /// needs at most 9 of FP32's 24, and the exponent range cannot overflow
    /// or go subnormal. Verified over all 126 finite `UE4M3` scales × every
    /// boundary of all five element formats: 0 inexact products, and 0
    /// mismatches against an exact-rational RNE reference.
    ///
    /// Ties-to-even needs no logic: the boundary between codes `k` and `k+1`
    /// resolves to whichever is even, and since the grid index *is* the code,
    /// that is a strict `>` at even `k` and a `>=` at odd `k` — decided at
    /// generation time, per boundary.
    BoundaryCompare,
}

/// A block's static shape: what `scaled_dequantize` needs, and the part of
/// `scaled_quantize` that is not a policy knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockShape {
    pub elem: FpFormatId,
    pub n: u32,
    pub scale: BlockScale,
}

/// One emitted helper. Ordered so the emitters can keep a `BTreeSet` and
/// produce byte-identical output run to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockHelper {
    /// `Vec<FP32, N>` → block.
    Quantize {
        shape: BlockShape,
        policy: ScalePolicy,
        round: RoundMode,
    },
    /// Block → `Vec<FP32, N>`.
    Dequantize { shape: BlockShape },
    /// Block · block → `FP32`. The one operation OCP MX defines normatively
    /// (§6.2): `X^A · X^B · Σᵢ(Pᵢ^A × Pᵢ^B)`.
    Dot { shape: BlockShape },
}

/// Resolve a `ScaledVec<Elem, N, Scale>` surface type to a block shape.
///
/// Returns `None` for anything the block predicates reject, so a caller can
/// never silently proceed on a half-understood type — the failure mode that
/// turned `ScaledVec<UInt<8>,4,E8M0>` into a 40-bit SV port against an 8-bit
/// sim variable in phase 2a.
pub fn shape_of(elem: &TypeExpr, n: u32, scale: &TypeExpr) -> Option<BlockShape> {
    let elem = by_type_expr(elem)?.id;
    if !matches!(
        elem,
        FpFormatId::E2m1
            | FpFormatId::E2m3
            | FpFormatId::E3m2
            | FpFormatId::E4m3
            | FpFormatId::E5m2
    ) {
        return None;
    }
    let scale = match scale {
        TypeExpr::E8M0 => BlockScale::E8m0,
        TypeExpr::UE4M3 => BlockScale::Ue4m3,
        _ => return None,
    };
    Some(BlockShape { elem, n, scale })
}

/// Resolve a `ScaledVec<Elem, N, Scale>` **TypeExpr** to a block shape.
///
/// The one resolver both backends use. It was briefly duplicated per emitter,
/// which is how a block ends up sized one way in the SV and another in the
/// sim; there is nothing here worth having two copies of.
///
/// `None` — never a guess — when the type is not a block, `N` does not fold to
/// a literal, or a member type is not a legal block member. Callers turn that
/// into a panic: typecheck has already accepted the type, so a `None` is a
/// compiler bug rather than a user error.
pub fn shape_of_type(ty: &TypeExpr) -> Option<BlockShape> {
    let TypeExpr::ScaledVec(elem, size, scale) = ty else {
        return None;
    };
    let n = match &size.kind {
        crate::ast::ExprKind::Literal(crate::ast::LitKind::Dec(n))
        | crate::ast::ExprKind::Literal(crate::ast::LitKind::Hex(n)) => *n as u32,
        _ => return None,
    };
    shape_of(elem, n, scale)
}

impl BlockShape {
    fn desc(self) -> &'static crate::fp_format::FpFormat {
        crate::fp_format::FORMATS
            .iter()
            .find(|f| f.id == self.elem)
            .expect("FpFormatId always has a row — fp_format::FORMATS is total over the enum")
    }
    fn elem_tag(self) -> &'static str {
        self.desc().tag
    }
    fn elem_width(self) -> u32 {
        self.desc().width
    }
    /// Total packed width, `scale_w + N * elem_w`.
    pub fn bits(self) -> u32 {
        self.scale.width() + self.n * self.elem_width()
    }
    /// Width of the `Vec<FP32, N>` side, packed.
    fn vec_bits(self) -> u32 {
        self.n * 32
    }
    /// The element format's top normal binade exponent: `floor(log2(max))`.
    ///
    /// Derived from `max_finite` rather than hand-tabled, so it cannot drift
    /// from the format table. E2M1's max is `6.0 = 1.5 * 2^2` → 2; E5M2's is
    /// `57344 = 1.75 * 2^15` → 15. This is the amount the block's shared
    /// exponent is shifted down by, so that the largest element in the block
    /// normalizes into the format's top binade (OCP §6.3).
    fn elem_emax(self) -> u32 {
        let m = self.desc().max_finite;
        debug_assert!(m > 0.0 && m.is_finite());
        m.log2().floor() as u32
    }
}

impl BlockHelper {
    fn shape(self) -> BlockShape {
        match self {
            BlockHelper::Quantize { shape, .. }
            | BlockHelper::Dequantize { shape }
            | BlockHelper::Dot { shape } => shape,
        }
    }

    /// The SystemVerilog function name. The C++ name is this with a leading
    /// underscore, matching the existing `arch_f32_mul` / `_arch_f32_mul`
    /// convention.
    pub fn sv_name(self) -> String {
        let s = self.shape();
        match self {
            BlockHelper::Quantize { policy, round, .. } => format!(
                "arch_scaled_quantize_{}_{}_{}_{}_{}",
                s.elem_tag(),
                s.n,
                s.scale.tag(),
                policy_tag(policy),
                round_tag(round),
            ),
            BlockHelper::Dequantize { .. } => format!(
                "arch_scaled_dequantize_{}_{}_{}",
                s.elem_tag(),
                s.n,
                s.scale.tag(),
            ),
            BlockHelper::Dot { .. } => {
                format!("arch_scaled_dot_{}_{}_{}", s.elem_tag(), s.n, s.scale.tag(),)
            }
        }
    }

    pub fn cpp_name(self) -> String {
        format!("_{}", self.sv_name())
    }
}

fn policy_tag(p: ScalePolicy) -> &'static str {
    match p {
        ScalePolicy::FloorPow2 => "floor",
        ScalePolicy::CeilPow2 => "ceil",
        ScalePolicy::Exact => "exact",
    }
}

fn round_tag(r: RoundMode) -> &'static str {
    match r {
        RoundMode::Rne => "rne",
        RoundMode::Rtz => "rtz",
        RoundMode::Rna => "rna",
    }
}

/// The summation schedule for a block dot product of `n` elements.
///
/// **This is a definition, not a derivation.** OCP MX §6.2 fixes the dot's
/// *value* as `X^A · X^B · Σᵢ(Pᵢ^A × Pᵢ^B)` but leaves the accumulation
/// implementation-defined — and FP32 addition is not associative, so an
/// unstated order is an unstated result. ARCH defines it here, once, and both
/// backends render this same schedule.
///
/// The order is **balanced pairwise**: each round adds adjacent pairs, a lone
/// trailing value passes through untouched, repeat until one remains. Chosen
/// over a running serial accumulator because it is what a dot-product datapath
/// synthesizes to anyway (⌈log₂ N⌉ deep rather than N), and because pairwise
/// summation has a strictly smaller error bound — `O(log N)` growth against
/// serial's `O(N)`.
///
/// Temps `0..n` are the element-pair products; each returned `(l, r)` defines
/// the next temp in order, so temp `n + k` is `adds[k]`. Returns the adds and
/// the index of the final temp (which is `0` when `n == 1`: no adds at all).
fn dot_schedule(n: u32) -> (Vec<(usize, usize)>, usize) {
    let mut cur: Vec<usize> = (0..n as usize).collect();
    let mut adds: Vec<(usize, usize)> = Vec::new();
    let mut next_id = n as usize;
    while cur.len() > 1 {
        let mut nxt = Vec::with_capacity(cur.len().div_ceil(2));
        let mut i = 0;
        while i + 1 < cur.len() {
            adds.push((cur[i], cur[i + 1]));
            nxt.push(next_id);
            next_id += 1;
            i += 2;
        }
        if i < cur.len() {
            // Odd count: the last value skips this round rather than being
            // paired with an implicit zero. Adding zero is not a no-op in
            // FP32 (it turns -0.0 into +0.0), so a "harmless" pad would be a
            // real value change.
            nxt.push(cur[i]);
        }
        cur = nxt;
    }
    (adds, cur[0])
}

// ── Grids and decision boundaries (the division-free kernel) ────────────────
//
// Everything here is DERIVED from `fp_format::FORMATS`, never hand-tabled: a
// hand-written boundary table is a second source of truth for the rounding
// rule, and the one thing this file exists to prevent is two sources of truth
// disagreeing silently in both backends at once.

/// Bit pattern of `(sig / 2^mb) * 2^e` as an FP32, asserting the value is
/// exactly representable. Every grid point and boundary of every block
/// element format is, by construction — significands are at most `mant+2`
/// bits and the exponents sit deep inside FP32's range — so a lossy
/// conversion here means a format row is wrong, not that rounding is needed.
fn exact_f32(v: f64) -> u32 {
    let f = v as f32;
    debug_assert_eq!(
        f as f64, v,
        "block grid value {v} is not exactly representable in FP32"
    );
    f.to_bits()
}

/// The positive representable values of `f`, ascending. **The index into this
/// vector is the element code**: IEEE-style encodings are monotonic over
/// non-negative values, and every format's reserved Inf/NaN codes sit at the
/// top, so enumerating `(exp, mant)` in order yields codes `0, 1, 2, …` with
/// no gaps.
fn format_grid(f: &crate::fp_format::FpFormat) -> Vec<f64> {
    let (eb, mb) = (f.exp_bits, f.mant_bits);
    let bias = (1i32 << (eb - 1)) - 1;
    let e_top = (1u32 << eb) - 1;
    let m_top = (1u32 << mb) - 1;
    let mut out = Vec::new();
    for e in 0..(1u32 << eb) {
        for m in 0..(1u32 << mb) {
            if f.has_inf && e == e_top {
                continue; // Inf (m == 0) and every NaN payload
            }
            if !f.has_inf && f.has_nan && e == e_top && m == m_top {
                continue; // OCP E4M3-style: the sole NaN is the all-ones code
            }
            let v = if e == 0 {
                (m as f64) * 2f64.powi(1 - bias - mb as i32)
            } else {
                ((1u32 << mb) + m) as f64 * 2f64.powi(e as i32 - bias - mb as i32)
            };
            out.push(v);
        }
    }
    out
}

/// One rung of a decision ladder: `|x| > thr` (or `>=`) selects `code`.
///
/// `strict` encodes ties-to-even with no runtime logic. The boundary between
/// codes `k` and `k+1` is a tie only when `|x|` hits it exactly, and exactly
/// one of the two codes is even — so the choice is fixed at generation time:
/// strict `>` when `k` is even (the tie stays at `k`), `>=` when `k` is odd
/// (the tie moves up to the even `k+1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rung {
    /// FP32 bit pattern of the threshold, or of the *multiplier* applied to
    /// the scale when the ladder is scaled at runtime.
    thr: u32,
    strict: bool,
    code: u32,
}

/// The element ladder for format `f`, in ascending order. Thresholds are
/// multipliers: the emitted code compares `|v|` against `X × thr`.
///
/// The last rung is the overflow boundary and only exists for a format that
/// *has* somewhere to overflow to. For the all-finite sub-8-bit formats
/// (`FP4E2M1`, `FP6E2M3`, `FP6E3M2` — the OCP MX block elements, NVFP4's
/// included) there is no such code, so the ladder simply saturates at the top
/// grid entry, which is what OCP §6.3 requires anyway.
fn elem_ladder(f: &crate::fp_format::FpFormat) -> (Vec<Rung>, Option<u32>) {
    let g = format_grid(f);
    let mut rungs = Vec::with_capacity(g.len());
    for k in 0..g.len() - 1 {
        rungs.push(Rung {
            thr: exact_f32((g[k] + g[k + 1]) / 2.0),
            strict: k % 2 == 0,
            code: (k + 1) as u32,
        });
    }
    let overflows = f.has_inf || f.has_nan;
    let ovf_code = if overflows {
        let k = g.len() - 1;
        let ulp = g[k] - g[k - 1];
        rungs.push(Rung {
            thr: exact_f32(g[k] + ulp / 2.0),
            strict: k % 2 == 0,
            code: 0, // filled in by the emitter: it is the rounder's answer
        });
        Some(k as u32)
    } else {
        None
    };
    (rungs, ovf_code)
}

/// The scale ladder: which scale code a block maximum selects, under `policy`.
///
/// Thresholds are **absolute FP32 constants**, not multipliers — both the
/// element format's maximum and the scale grid are known at compile time, so
/// `elem_max × scale_value` folds and the whole scale choice becomes a
/// constant-threshold priority encoder with no arithmetic at all.
///
/// Sound because those products are exact: verified over all 126 finite
/// `UE4M3` scales × all five element formats, 0 inexact and 0 overflowing.
/// Returns the code the ladder starts at (used when the block maximum is
/// below every rung) and the rungs themselves.
fn scale_ladder(scale: BlockScale, elem_max: f64, policy: ScalePolicy) -> (u32, Vec<Rung>) {
    debug_assert!(!scale.is_pow2(), "ladder is the non-pow2 path");
    let g = scale.grid();
    // Code 0 is *zero* for UE4M3, and a zero scale would erase a block whose
    // maximum is nonzero. Underflow therefore clamps to the smallest nonzero
    // scale rather than to code 0 — the same rule as proposal §9 decision #3
    // ("underflow → the minimum scale"), read against a scale format that has
    // a zero where E8M0 does not.
    let lo = 1usize;
    match policy {
        ScalePolicy::Exact => (
            lo as u32,
            (lo..g.len() - 1)
                .map(|k| Rung {
                    thr: exact_f32(elem_max * (g[k] + g[k + 1]) / 2.0),
                    strict: k % 2 == 0,
                    code: (k + 1) as u32,
                })
                .collect(),
        ),
        // floor/ceil restrict the choice to the power-of-two members of the
        // same grid. `floor` takes a value as soon as the block maximum
        // reaches it; `ceil` steps to the NEXT power of two as soon as the
        // maximum exceeds one, which is why the two ladders differ in the
        // code they select and not only in the strictness of the test.
        //
        // `ceil` clamps at the largest power of two in the grid (2^8) rather
        // than at the largest finite code (448): 448 is not a power of two,
        // so selecting it would break the policy's own contract. Blocks above
        // that saturate in the element plane instead.
        ScalePolicy::FloorPow2 | ScalePolicy::CeilPow2 => {
            let pow2: Vec<(usize, f64)> = g
                .iter()
                .copied()
                .enumerate()
                .skip(lo)
                .filter(|(_, v)| v.log2().fract() == 0.0)
                .collect();
            let base = pow2[0].0 as u32;
            if policy == ScalePolicy::FloorPow2 {
                let r = pow2
                    .iter()
                    .map(|&(k, v)| Rung {
                        thr: exact_f32(elem_max * v),
                        strict: false,
                        code: k as u32,
                    })
                    .collect();
                (base, r)
            } else {
                let r = pow2
                    .windows(2)
                    .map(|w| Rung {
                        thr: exact_f32(elem_max * w[0].1),
                        strict: true,
                        code: w[1].0 as u32,
                    })
                    .collect();
                (base, r)
            }
        }
    }
}

/// `[hi:0]` in the `logic [hi:0] ` declaration style `render_sv` uses.
fn sv_w(bits: u32) -> String {
    if bits == 1 {
        String::new()
    } else {
        format!("[{}:0] ", bits - 1)
    }
}

// ── SystemVerilog ───────────────────────────────────────────────────────────

/// The `$unit`-scope SV definition. Emitted alongside `fp::fp_sv_helpers`,
/// which supplies every `arch_*` callee referenced here.
pub fn sv_definition(h: BlockHelper) -> String {
    match h {
        BlockHelper::Quantize {
            shape,
            policy,
            round,
        } => sv_quantize(shape, policy, round),
        BlockHelper::Dequantize { shape } => sv_dequantize(shape),
        BlockHelper::Dot { shape } => sv_dot(shape),
    }
}

/// `scaled_dot` — OCP MX §6.2's `X^A · X^B · Σᵢ(Pᵢ^A × Pᵢ^B)`.
///
/// The summation tree is **unrolled at generation time** rather than emitted
/// as a loop over an array. That is what makes the two backends comparable by
/// inspection: both print the identical sequence of named temporaries from
/// the identical [`dot_schedule`], so a reviewer can diff them line for line
/// and a divergence cannot hide inside differing loop semantics.
fn sv_dot(s: BlockShape) -> String {
    let name = BlockHelper::Dot { shape: s }.sv_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let (adds, last) = dot_schedule(n);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: OCP MX \u{a7}6.2 block dot, X^A * X^B * sum(P^A_i * P^B_i)."
    );
    let _ = writeln!(
        o,
        "function automatic logic {}{name}(input logic {}a, input logic {}b);",
        sv_w(32),
        sv_w(bw),
        sv_w(bw)
    );
    for t in 0..(n as usize + adds.len()) {
        let _ = writeln!(o, "  logic {}t{t};", sv_w(32));
    }
    // Element-pair products. EXACT in FP32 for every element format — the
    // widest significand product is E4M3's 4x4 = 8 bits against FP32's 24,
    // and the widest exponent range is E5M2's 2^-32 .. 2^31.6, comfortably
    // inside FP32's normals. So every rounding in a block dot happens in the
    // summation below, none in these multiplies (`fp_block_dot_products_are_exact`).
    for i in 0..n as usize {
        let _ = writeln!(
            o,
            "  t{i} = arch_f32_mul(arch_{tag}_to_f32(a[{lo} +: {ew}]), \
             arch_{tag}_to_f32(b[{lo} +: {ew}]));",
            lo = i * ew as usize
        );
    }
    for (k, (l, r)) in adds.iter().enumerate() {
        let _ = writeln!(o, "  t{} = arch_f32_add(t{l}, t{r});", n as usize + k);
    }
    // Scales applied ONE AT A TIME, not as a pre-formed `X^A * X^B`.
    // Each is a power of two, so each multiply is exact absent over/underflow
    // — but forming `X^A * X^B` first can overflow to Inf or flush to zero
    // (the two E8M0 exponents span 2^-127..2^127, so their product spans
    // 2^-254..2^254, well outside FP32) even when the final result is
    // perfectly representable. Applying them separately has a strictly wider
    // exact domain. A NaN scale (0xFF) widens to NaN and poisons the result
    // here, which is the block value rule.
    let _ = writeln!(
        o,
        "  {name} = arch_f32_mul(arch_f32_mul(t{last}, {w}(a[{hi}:{lo}])), \
         {w}(b[{hi}:{lo}]));",
        w = s.scale.widen_fn(),
        hi = bw - 1,
        lo = bw - sw
    );
    let _ = writeln!(o, "endfunction");
    o
}

/// Group a dot reduction tree (`dot_schedule`) into dependency levels: an add
/// is in level `L` iff both its inputs are produced in a level below `L`
/// (products are level 0). Returns, per level, the list of `(dst, l, r)` adds.
fn dot_levels(n: u32) -> Vec<Vec<(usize, usize, usize)>> {
    let (adds, _last) = dot_schedule(n);
    let mut level = vec![0i32; n as usize + adds.len()];
    let mut out: Vec<Vec<(usize, usize, usize)>> = Vec::new();
    for (k, (l, r)) in adds.iter().enumerate() {
        let dst = n as usize + k;
        let lv = level[*l].max(level[*r]) + 1;
        level[dst] = lv;
        let idx = (lv - 1) as usize;
        while out.len() <= idx {
            out.push(Vec::new());
        }
        out[idx].push((dst, *l, *r));
    }
    out
}

/// Binding latency of the staged (coarse per-level) `scaled_dot`: one products
/// stage, one register per reduction-tree level, one first-scale-multiply
/// stage, and the binding output register (the second scale multiply is the
/// combinational tail). Verified against the prototype (arch#955).
pub fn staged_dot_binding_latency(n: u32) -> u32 {
    let tree_depth = dot_levels(n).len() as u32;
    1 + tree_depth + 1 + 1
}

/// Staged (coarse per-level pipelined) `scaled_dot` for an E8M0-scale block
/// (arch#955): N exact element products (stage 1), the `f32_add` reduction tree
/// one level per stage, then the two power-of-two scale multiplies. One
/// f32-op-level per stage, initiation interval one. Returns
/// `(module_name, sv_text, binding_latency)`.
pub fn sv_staged_dot(s: BlockShape) -> (String, String, u32) {
    assert_eq!(
        s.scale.quant_kernel(),
        QuantKernel::ExactReciprocal,
        "staged scaled_dot is implemented for E8M0 scales only (arch#955)"
    );
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let levels = dot_levels(n);
    let binding_latency = staged_dot_binding_latency(n);
    let scale_stages = binding_latency - 1;
    let base = BlockHelper::Dot { shape: s }.sv_name();
    let name = format!("{base}_staged{binding_latency}");
    let widen = s.scale.widen_fn();
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: staged latency-{binding_latency} scaled_dot, II=1 (arch#955)."
    );
    let _ = writeln!(o, "module {name} (");
    let _ = writeln!(o, "  input logic clk,");
    let _ = writeln!(o, "  input logic {}a,", sv_w(bw));
    let _ = writeln!(o, "  input logic {}b,", sv_w(bw));
    let _ = writeln!(o, "  output logic {}y", sv_w(32));
    let _ = writeln!(o, ");");
    // stage 1: element products
    for i in 0..n as usize {
        let _ = writeln!(o, "  logic {}pp{i};", sv_w(32));
    }
    let _ = writeln!(o, "  always_comb begin");
    for i in 0..n as usize {
        let lo = i * ew as usize;
        let _ = writeln!(
            o,
            "    pp{i} = arch_f32_mul(arch_{tag}_to_f32(a[{lo} +: {ew}]), arch_{tag}_to_f32(b[{lo} +: {ew}]));"
        );
    }
    let _ = writeln!(o, "  end");
    for i in 0..n as usize {
        let _ = writeln!(o, "  logic {}r0_{i};", sv_w(32));
    }
    // scale bytes shifted through the pipeline to reach the two scale muls
    for k in 1..=scale_stages {
        let _ = writeln!(o, "  logic {}sda{k}; logic {}sdb{k};", sv_w(sw), sv_w(sw));
    }
    let _ = writeln!(o, "  always_ff @(posedge clk) begin");
    for i in 0..n as usize {
        let _ = writeln!(o, "    r0_{i} <= pp{i};");
    }
    let _ = writeln!(
        o,
        "    sda1 <= a[{}:{}]; sdb1 <= b[{}:{}];",
        bw - 1,
        bw - sw,
        bw - 1,
        bw - sw
    );
    for k in 2..=scale_stages {
        let _ = writeln!(o, "    sda{k} <= sda{}; sdb{k} <= sdb{};", k - 1, k - 1);
    }
    let _ = writeln!(o, "  end");
    // reduction tree: one stage per level; reg_of maps a value id to its
    // current register name.
    let mut reg_of: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for i in 0..n as usize {
        reg_of.insert(i, format!("r0_{i}"));
    }
    for (li, level) in levels.iter().enumerate() {
        let lvl = (li + 1) as u32;
        for (dst, _, _) in level {
            let _ = writeln!(o, "  logic {}c{lvl}_{dst};", sv_w(32));
        }
        let _ = writeln!(o, "  always_comb begin");
        for (dst, l, r) in level {
            let lr = reg_of[l].clone();
            let rr = reg_of[r].clone();
            let _ = writeln!(o, "    c{lvl}_{dst} = arch_f32_add({lr}, {rr});");
        }
        let _ = writeln!(o, "  end");
        for (dst, _, _) in level {
            let _ = writeln!(o, "  logic {}r{lvl}_{dst};", sv_w(32));
        }
        let _ = writeln!(o, "  always_ff @(posedge clk) begin");
        for (dst, _, _) in level {
            let _ = writeln!(o, "    r{lvl}_{dst} <= c{lvl}_{dst};");
            reg_of.insert(*dst, format!("r{lvl}_{dst}"));
        }
        let _ = writeln!(o, "  end");
    }
    let (_, last) = dot_schedule(n);
    let sum_reg = reg_of[&last].clone();
    // scale multiply 1 (comb) then register. The tree output `sum_reg` lands
    // after 1 (products) + tree_depth register edges, so scale-mul-1 must read
    // the scale byte delayed by exactly that many edges — NOT `scale_stages`
    // (one too many; that misaligns the block scale by a cycle, arch#955).
    let sum_stage = 1 + dot_levels(n).len() as u32;
    let _ = writeln!(o, "  logic {}m1;", sv_w(32));
    let _ = writeln!(
        o,
        "  always_comb m1 = arch_f32_mul({sum_reg}, {widen}(sda{sum_stage}));"
    );
    let _ = writeln!(o, "  logic {}m1r;", sv_w(32));
    let _ = writeln!(o, "  always_ff @(posedge clk) m1r <= m1;");
    // scale multiply 2 (comb tail) to output
    let _ = writeln!(
        o,
        "  always_comb y = arch_f32_mul(m1r, {widen}(sdb{scale_stages}));"
    );
    let _ = writeln!(o, "endmodule");
    (name, o, binding_latency)
}

fn sv_quantize(s: BlockShape, policy: ScalePolicy, round: RoundMode) -> String {
    assert_eq!(
        round,
        RoundMode::Rne,
        "only RNE narrowing is lowered today; typecheck refuses the others \
         (arch#890) — reaching here means that gate was removed \
         without adding the rounder variants"
    );
    if s.scale.quant_kernel() == QuantKernel::BoundaryCompare {
        return sv_quantize_boundary(s, policy);
    }
    let name = BlockHelper::Quantize {
        shape: s,
        policy,
        round,
    }
    .sv_name();
    let (bw, vw, ew, sw) = (s.bits(), s.vec_bits(), s.elem_width(), s.scale.width());
    let (n, emax) = (s.n, s.elem_emax());
    let tag = s.elem_tag();
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: Vec<FP32,{n}> -> {{scale, P[{}..0]}}. OCP MX \u{a7}6.3 conversion.",
        n - 1
    );
    let _ = writeln!(
        o,
        "function automatic logic {}{name}(input logic {}v);",
        sv_w(bw),
        sv_w(vw)
    );
    let _ = writeln!(o, "  logic {}amax;", sv_w(32));
    let _ = writeln!(o, "  logic {}mag;", sv_w(32));
    let _ = writeln!(o, "  logic {}inv;", sv_w(32));
    let _ = writeln!(o, "  logic {}ecode;", sv_w(8));
    let _ = writeln!(o, "  logic {}code;", sv_w(8));
    let _ = writeln!(o, "  logic {}r;", sv_w(bw));
    // 1. amax as an unsigned integer max of |v_i| — see the module header.
    // Loop iterator declared with the other locals, never inline in the
    // `for` header. A `for (int unsigned i = ...)` nested inside an if/else
    // arm leaves `i` undriven on the sibling paths, and yosys's `proc` pass
    // then infers a LATCH per bit-slice of the iterator (arch#932). The
    // emitted logic is pure combinational quantization, so those latches are
    // spurious hardware -- but they are real in the netlist, and no simulator
    // gate can see them. Hoisting the declaration alone clears it; no default
    // assignment is needed (verified against yosys 0.67).
    let _ = writeln!(o, "  int unsigned i;");
    let _ = writeln!(o, "  amax = {}'h0;", 32);
    let _ = writeln!(o, "  for (i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(o, "    mag = v[i*32 +: 32] & 32'h7FFFFFFF;");
    let _ = writeln!(o, "    if (mag > amax) amax = mag;");
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  r = {bw}'h0;");
    // 2. Non-finite anywhere in the block => NaN scale, element bits are
    //    don't-care by the block value rule and are emitted as zero.
    let _ = writeln!(o, "  if (amax >= 32'h7F800000) begin");
    let _ = writeln!(
        o,
        "    r[{}:{}] = 8'h{:02X};",
        bw - 1,
        bw - sw,
        s.scale.nan_code()
    );
    // 3. All-zero block => minimum scale, zero elements.
    let _ = writeln!(o, "  end else if (amax == 32'h0) begin");
    let _ = writeln!(
        o,
        "    r[{}:{}] = 8'h{:02X};",
        bw - 1,
        bw - sw,
        s.scale.zero_block_code()
    );
    let _ = writeln!(o, "  end else begin");
    let _ = writeln!(o, "    ecode = {}(amax);", s.scale.narrow_fn());
    if policy == ScalePolicy::CeilPow2 {
        // Round the scale UP when amax is not already an exact power of two.
        // Widening the code gives the floor power of two as an f32 bit
        // pattern; both operands are finite and positive, so the unsigned
        // compare is the numeric compare. The clamp is the format's largest
        // finite code — bumping past it would land on NaN.
        let _ = writeln!(
            o,
            "    if (amax > {}(ecode) && ecode != 8'h{:02X}) ecode = ecode + 8'h01;",
            s.scale.widen_fn(),
            s.scale.max_finite_code()
        );
    }
    // 4. Shift the shared exponent down so the block maximum normalizes into
    //    the element format's top binade; clamp underflow at the minimum
    //    scale (overflow is structurally impossible: ecode <= 0xFE, emax >= 2).
    let _ = writeln!(
        o,
        "    code = (ecode > 8'd{emax}) ? (ecode - 8'd{emax}) : 8'h00;"
    );
    // 5. Multiply by the reciprocal scale, itself an E8M0 code, so the
    //    scaling is exact and the narrow below rounds exactly once.
    let _ = writeln!(
        o,
        "    inv = {};",
        s.scale
            .sv_reciprocal("code")
            .expect("ExactReciprocal kernel implies the scale has one")
    );
    let _ = writeln!(o, "    for (i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(
        o,
        "      r[i*{ew} +: {ew}] = arch_f32_to_{tag}(arch_f32_mul(v[i*32 +: 32], inv));"
    );
    let _ = writeln!(o, "    end");
    let _ = writeln!(o, "    r[{}:{}] = code;", bw - 1, bw - sw);
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  {name} = r;");
    let _ = writeln!(o, "endfunction");
    o
}

/// Staged (pipelined) `scaled_quantize` for an ExactReciprocal (E8M0) scale
/// (arch#955). Emits an SV **module** (not a function): the 8 per-element
/// FP32 multiplies — 84% of the block quantize's critical path — run through
/// instances of the staged multiplier `F32_MUL_S4_SCHEDULE.sv_module`, while
/// the shallow scale-compute and narrow/pack stay combinational around them.
///
/// Structure (validated in iverilog against the comb form, bit-exact, before
/// this emitter was written):
///
/// ```text
/// stage A (comb): amax → ecode → code → inv, scale_byte, is_special
///   register {v, inv, scale_byte, is_special}                     [edge 1]
/// N× ArchF32MulStaged4(clk, v_r[i], inv_r) → prod[i]     [3 internal edges]
///   meta {scale_byte, is_special} carried through a 3-deep shift to align
/// stage B (comb): elem[i] = is_special ? 0 : narrow(prod[i]); pack
/// ```
///
/// Latency to the module's comb `y` is 4 (1 + the multiply's 3 register
/// layers); the binding's `y@5` output register supplies the 5th edge — so
/// this emits a latency-5 datapath. The multiplies run unconditionally; the
/// NaN-block / zero-block cases are handled by masking the narrowed elements
/// and overriding the scale byte, so no conditional datapath is needed.
///
/// Returns `(module_name, sv_text, binding_latency)`.
/// Whether a shape can be staged (pipelined) today (arch#955): only
/// ExactReciprocal (E8M0) scales, whose reciprocal-multiply datapath the
/// staged emitter reproduces. BoundaryCompare scales (UE4M3) are a follow-up.
pub fn staged_quantize_supported(s: BlockShape) -> bool {
    s.scale.quant_kernel() == QuantKernel::ExactReciprocal
}

/// The binding latency `scaled_quantize<Fmt, pipelined, N>` requires: one
/// scale-compute register stage, the staged multiply's register layers, and
/// the binding's own output register. Fixed by `F32_MUL_S4_SCHEDULE` today.
pub fn staged_quantize_binding_latency() -> u32 {
    let mul_reg_layers = crate::pipelined_ops::F32_MUL_S4_SCHEDULE.main_starts.len() as u32 - 2;
    1 + mul_reg_layers + 1
}

pub fn sv_staged_quantize(
    s: BlockShape,
    policy: ScalePolicy,
    round: RoundMode,
) -> (String, String, u32) {
    assert_eq!(
        round,
        RoundMode::Rne,
        "only RNE narrowing is lowered (arch#890)"
    );
    assert_eq!(
        s.scale.quant_kernel(),
        QuantKernel::ExactReciprocal,
        "staged quantize is implemented for ExactReciprocal (E8M0) scales only \
         today (arch#955); BoundaryCompare scales (UE4M3) are a follow-up"
    );
    let mul_module = crate::pipelined_ops::F32_MUL_S4_SCHEDULE.sv_module;
    // Register layers inside the staged multiply = stages - 1; the meta shift
    // must match so the scale byte lands with `prod`.
    let mul_reg_layers = crate::pipelined_ops::F32_MUL_S4_SCHEDULE.main_starts.len() as u32 - 2;
    let binding_latency = 1 + mul_reg_layers + 1; // stage-A reg + mul + output reg

    let base = BlockHelper::Quantize {
        shape: s,
        policy,
        round,
    }
    .sv_name();
    let name = format!("{base}_staged{binding_latency}");
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, emax, tag) = (s.n, s.elem_emax(), s.elem_tag());
    let vw = s.vec_bits();
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: staged (latency-{binding_latency}) `scaled_quantize`. The {n} FP32 \
         multiplies run through `{mul_module}`; scale-compute and narrow stay comb \
         (arch#955)."
    );
    let _ = writeln!(
        o,
        "module {name} (\n  input logic clk,\n  input logic {}v,\n  output logic {}y\n);",
        sv_w(vw),
        sv_w(bw)
    );
    // ── stage A (comb): scale compute ──
    let _ = writeln!(o, "  logic {}amax;", sv_w(32));
    let _ = writeln!(o, "  logic {}mag;", sv_w(32));
    let _ = writeln!(o, "  logic {}inv;", sv_w(32));
    let _ = writeln!(o, "  logic {}ecode;", sv_w(8));
    let _ = writeln!(o, "  logic {}code;", sv_w(8));
    let _ = writeln!(o, "  logic {}scale_byte;", sv_w(sw));
    let _ = writeln!(o, "  logic is_special;");
    let _ = writeln!(o, "  int unsigned i;");
    let _ = writeln!(o, "  always_comb begin");
    let _ = writeln!(o, "    amax = 32'h0;");
    let _ = writeln!(o, "    for (i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(o, "      mag = v[i*32 +: 32] & 32'h7FFFFFFF;");
    let _ = writeln!(o, "      if (mag > amax) amax = mag;");
    let _ = writeln!(o, "    end");
    let _ = writeln!(
        o,
        "    is_special = (amax >= 32'h7F800000) || (amax == 32'h0);"
    );
    let _ = writeln!(o, "    ecode = {}(amax);", s.scale.narrow_fn());
    if policy == ScalePolicy::CeilPow2 {
        let _ = writeln!(
            o,
            "    if (amax > {}(ecode) && ecode != 8'h{:02X}) ecode = ecode + 8'h01;",
            s.scale.widen_fn(),
            s.scale.max_finite_code()
        );
    }
    let _ = writeln!(
        o,
        "    code = (ecode > 8'd{emax}) ? (ecode - 8'd{emax}) : 8'h00;"
    );
    let _ = writeln!(
        o,
        "    inv = {};",
        s.scale
            .sv_reciprocal("code")
            .expect("ExactReciprocal has a reciprocal")
    );
    let _ = writeln!(
        o,
        "    if (amax >= 32'h7F800000) scale_byte = 8'h{:02X};",
        s.scale.nan_code()
    );
    let _ = writeln!(
        o,
        "    else if (amax == 32'h0) scale_byte = 8'h{:02X};",
        s.scale.zero_block_code()
    );
    let _ = writeln!(o, "    else scale_byte = code;");
    let _ = writeln!(o, "  end");
    // register layer A
    let _ = writeln!(o, "  logic {}v_r;", sv_w(vw));
    let _ = writeln!(o, "  logic {}inv_r;", sv_w(32));
    let _ = writeln!(o, "  logic {}sb_r;", sv_w(sw));
    let _ = writeln!(o, "  logic sp_r;");
    let _ = writeln!(o, "  always_ff @(posedge clk) begin");
    let _ = writeln!(
        o,
        "    v_r <= v; inv_r <= inv; sb_r <= scale_byte; sp_r <= is_special;"
    );
    let _ = writeln!(o, "  end");
    // ── staged multiplies ──
    let _ = writeln!(o, "  logic {}prod [0:{}];", sv_w(32), n - 1);
    let _ = writeln!(o, "  genvar g;");
    let _ = writeln!(o, "  generate for (g = 0; g < {n}; g = g + 1) begin : muls");
    let _ = writeln!(
        o,
        "    {mul_module} m (.clk(clk), .a(v_r[g*32 +: 32]), .b(inv_r), .y(prod[g]));"
    );
    let _ = writeln!(o, "  end endgenerate");
    // ── meta carry: match the multiply's register-layer count ──
    for k in 1..=mul_reg_layers {
        let _ = writeln!(o, "  logic {}sb{k}; logic sp{k};", sv_w(sw));
    }
    let _ = writeln!(o, "  always_ff @(posedge clk) begin");
    let _ = writeln!(o, "    sb1 <= sb_r; sp1 <= sp_r;");
    for k in 2..=mul_reg_layers {
        let _ = writeln!(o, "    sb{k} <= sb{}; sp{k} <= sp{};", k - 1, k - 1);
    }
    let _ = writeln!(o, "  end");
    // ── stage B (comb): narrow + mask + pack ──
    let _ = writeln!(o, "  logic {}r;", sv_w(bw));
    let _ = writeln!(o, "  int unsigned j;");
    let _ = writeln!(o, "  always_comb begin");
    let _ = writeln!(o, "    r = {bw}'h0;");
    let _ = writeln!(o, "    r[{}:{}] = sb{mul_reg_layers};", bw - 1, bw - sw);
    let _ = writeln!(o, "    for (j = 0; j < {n}; j = j + 1) begin");
    let _ = writeln!(
        o,
        "      r[j*{ew} +: {ew}] = sp{mul_reg_layers} ? {ew}'h0 : arch_f32_to_{tag}(prod[j]);"
    );
    let _ = writeln!(o, "    end");
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  assign y = r;");
    let _ = writeln!(o, "endmodule");
    (name, o, binding_latency)
}

/// `scaled_quantize` for a scale with no exact reciprocal — see

/// [`QuantKernel::BoundaryCompare`].
///
/// Two constant-threshold ladders and not one division. Both compare FP32 bit
/// patterns as unsigned integers, which is the numeric comparison here because
/// every operand is non-negative (the block maximum is sign-cleared, and every
/// threshold is positive by construction) — the same fact the block-maximum
/// scan above already rests on.
fn sv_quantize_boundary(s: BlockShape, policy: ScalePolicy) -> String {
    let name = BlockHelper::Quantize {
        shape: s,
        policy,
        round: RoundMode::Rne,
    }
    .sv_name();
    let (bw, vw, ew, sw) = (s.bits(), s.vec_bits(), s.elem_width(), s.scale.width());
    let n = s.n;
    let tag = s.elem_tag();
    let (scale_base, scale_rungs) = scale_ladder(s.scale, s.desc().max_finite, policy);
    let (elem_rungs, ovf) = elem_ladder(s.desc());
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: Vec<FP32,{n}> -> {{scale, P[{}..0]}}. Division-free: the \
         scale ladder is",
        n - 1
    );
    let _ = writeln!(
        o,
        "// constant thresholds, the element ladder is `X * boundary`, and every such \
         product"
    );
    let _ = writeln!(
        o,
        "// is exact in FP32 — so each element rounds exactly once. See src/fp_block.rs."
    );
    let _ = writeln!(
        o,
        "function automatic logic {}{name}(input logic {}v);",
        sv_w(bw),
        sv_w(vw)
    );
    let _ = writeln!(o, "  logic {}amax;", sv_w(32));
    let _ = writeln!(o, "  logic {}mag;", sv_w(32));
    let _ = writeln!(o, "  logic {}a;", sv_w(32));
    let _ = writeln!(o, "  logic {}x;", sv_w(32));
    let _ = writeln!(o, "  logic {}code;", sv_w(sw));
    let _ = writeln!(o, "  logic {}m;", sv_w(ew));
    let _ = writeln!(o, "  logic {}p;", sv_w(ew));
    let _ = writeln!(o, "  logic sgn;");
    let _ = writeln!(o, "  logic {}r;", sv_w(bw));
    // Loop iterator declared with the other locals, never inline in the
    // `for` header. A `for (int unsigned i = ...)` nested inside an if/else
    // arm leaves `i` undriven on the sibling paths, and yosys's `proc` pass
    // then infers a LATCH per bit-slice of the iterator (arch#932). The
    // emitted logic is pure combinational quantization, so those latches are
    // spurious hardware -- but they are real in the netlist, and no simulator
    // gate can see them. Hoisting the declaration alone clears it; no default
    // assignment is needed (verified against yosys 0.67).
    let _ = writeln!(o, "  int unsigned i;");
    let _ = writeln!(o, "  amax = 32'h0;");
    let _ = writeln!(o, "  for (i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(o, "    mag = v[i*32 +: 32] & 32'h7FFFFFFF;");
    let _ = writeln!(o, "    if (mag > amax) amax = mag;");
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  r = {bw}'h0;");
    let _ = writeln!(o, "  if (amax >= 32'h7F800000) begin");
    let _ = writeln!(
        o,
        "    r[{}:{}] = {sw}'h{:02X};",
        bw - 1,
        bw - sw,
        s.scale.nan_code()
    );
    let _ = writeln!(o, "  end else if (amax == 32'h0) begin");
    let _ = writeln!(
        o,
        "    r[{}:{}] = {sw}'h{:02X};",
        bw - 1,
        bw - sw,
        s.scale.zero_block_code()
    );
    let _ = writeln!(o, "  end else begin");
    let _ = writeln!(o, "    code = {sw}'h{scale_base:02X};");
    for rg in &scale_rungs {
        let op = if rg.strict { ">" } else { ">=" };
        let _ = writeln!(
            o,
            "    if (amax {op} 32'h{:08X}) code = {sw}'h{:02X};",
            rg.thr, rg.code
        );
    }
    let _ = writeln!(o, "    x = {}(code);", s.scale.widen_fn());
    let _ = writeln!(o, "    for (i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(o, "      sgn = v[i*32 + 31];");
    let _ = writeln!(o, "      a = v[i*32 +: 32] & 32'h7FFFFFFF;");
    let _ = writeln!(o, "      m = {ew}'h0;");
    let cmp_rungs = if ovf.is_some() {
        &elem_rungs[..elem_rungs.len() - 1]
    } else {
        &elem_rungs[..]
    };
    for rg in cmp_rungs {
        let op = if rg.strict { ">" } else { ">=" };
        let _ = writeln!(
            o,
            "      if (a {op} arch_f32_mul(x, 32'h{:08X})) m = {ew}'h{:02X};",
            rg.thr, rg.code
        );
    }
    let _ = writeln!(o, "      p = {{sgn, m[{}:0]}};", ew - 2);
    if ovf.is_some() {
        let top = elem_rungs[elem_rungs.len() - 1];
        let op = if top.strict { ">" } else { ">=" };
        // Overflow is delegated to the element format's own rounder rather
        // than re-derived: feeding it a value that unambiguously overflows
        // reproduces the `--fp-compat` profile's rule (riscv NaN/Inf vs cuda
        // satfinite) with no second copy of it to drift.
        let _ = writeln!(
            o,
            "      if (a {op} arch_f32_mul(x, 32'h{:08X})) p = arch_f32_to_{tag}({{sgn, 31'h7F7FFFFF}});",
            top.thr
        );
    }
    let _ = writeln!(o, "      r[i*{ew} +: {ew}] = p;");
    let _ = writeln!(o, "    end");
    let _ = writeln!(o, "    r[{}:{}] = code;", bw - 1, bw - sw);
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  {name} = r;");
    let _ = writeln!(o, "endfunction");
    o
}

fn sv_dequantize(s: BlockShape) -> String {
    let name = BlockHelper::Dequantize { shape: s }.sv_name();
    let (bw, vw, ew, sw) = (s.bits(), s.vec_bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let mut o = String::new();
    let _ = writeln!(o, "// {name}: block -> Vec<FP32,{n}>, scale applied.");
    let _ = writeln!(
        o,
        "function automatic logic {}{name}(input logic {}b);",
        sv_w(vw),
        sv_w(bw)
    );
    let _ = writeln!(o, "  logic {}x;", sv_w(32));
    let _ = writeln!(o, "  logic {}w;", sv_w(32));
    let _ = writeln!(o, "  logic {}p;", sv_w(32));
    let _ = writeln!(o, "  logic {}r;", sv_w(vw));
    // Loop iterator declared with the other locals, never inline in the
    // `for` header. A `for (int unsigned i = ...)` nested inside an if/else
    // arm leaves `i` undriven on the sibling paths, and yosys's `proc` pass
    // then infers a LATCH per bit-slice of the iterator (arch#932). The
    // emitted logic is pure combinational quantization, so those latches are
    // spurious hardware -- but they are real in the netlist, and no simulator
    // gate can see them. Hoisting the declaration alone clears it; no default
    // assignment is needed (verified against yosys 0.67).
    let _ = writeln!(o, "  int unsigned i;");
    // A NaN scale (code 0xFF) widens to a NaN f32, so every product below is
    // NaN and the element bits are ignored: the block value rule falls out of
    // the multiply rather than needing a branch.
    let _ = writeln!(
        o,
        "  x = {}(b[{}:{}]);",
        s.scale.widen_fn(),
        bw - 1,
        bw - sw
    );
    let _ = writeln!(o, "  r = {vw}'h0;");
    let _ = writeln!(o, "  for (i = 0; i < {n}; i = i + 1) begin");
    let _ = writeln!(o, "    w = arch_{tag}_to_f32(b[i*{ew} +: {ew}]);");
    let _ = writeln!(o, "    p = arch_f32_mul(x, w);");
    // A finite element scaled by a finite scale saturates to +-FP32_MAX
    // rather than overflowing to infinity. An element that is ITSELF Inf or
    // NaN (E4M3/E5M2 can be) keeps its own non-finite result — hence the
    // guard on `w` and not just on the product.
    let _ = writeln!(
        o,
        "    if (p[30:23] == 8'hFF && p[22:0] == 23'h0 && w[30:23] != 8'hFF)"
    );
    let _ = writeln!(o, "      p = {{p[31], 31'h7F7FFFFF}};");
    let _ = writeln!(o, "    r[i*32 +: 32] = p;");
    let _ = writeln!(o, "  end");
    let _ = writeln!(o, "  {name} = r;");
    let _ = writeln!(o, "endfunction");
    o
}

// ── C++ (arch sim) ──────────────────────────────────────────────────────────

/// Bit insert/extract over a little-endian `uint32_t` word array, matching
/// `VlWide`'s layout (`_data[0]` is bits 31:0).
///
/// Bit-at-a-time on purpose: element widths are 4, 6 and 8 bits, and 6 does
/// not divide 32, so a field can straddle a word boundary. A width-agnostic
/// loop is obviously correct where a shift-and-mask fast path would need a
/// straddle case that only FP6 blocks exercise.
/// Bit and word access over a little-endian `uint32_t` array, matching
/// `VlWide`'s layout (`_data[0]` is bits 31:0).
///
/// `_arch_blk_ins`/`_arch_blk_ext` work a bit at a time on purpose: element
/// widths are 4, 6 and 8 bits, and 6 does not divide 32, so a field can
/// straddle a word boundary. A width-agnostic loop is obviously correct where
/// a shift-and-mask fast path would need a straddle case that only FP6 blocks
/// exercise.
///
/// `_arch_blk_get`/`_arch_blk_put` are overloaded rather than switched on a
/// width, because the sim gives one block type TWO different C++
/// representations: a 72-bit block is `VlWide<3>` as a port but `_arch_u128`
/// as an internal wire. Any width table here would therefore be right for
/// ports and wrong for wires — the `_ => 32` failure shape again. Letting the
/// destination's own type select the overload means there is nothing to keep
/// in sync: the scalar overload takes `unsigned __int128`, which every
/// `uint8/16/32/64_t` and `_arch_u128` promotes to, and the `VlWide<W>`
/// template is the more specialized match for wide storage.
pub const CPP_PRELUDE: &str = "\
// ── ScaledVec block bit access (see src/fp_block.rs) ──
static inline void _arch_blk_ins(uint32_t* w, unsigned lo, unsigned width, uint32_t val) {
  for (unsigned k = 0; k < width; ++k) {
    unsigned b = lo + k;
    w[b >> 5] = (w[b >> 5] & ~(1u << (b & 31))) | (((val >> k) & 1u) << (b & 31));
  }
}
static inline uint32_t _arch_blk_ext(const uint32_t* w, unsigned lo, unsigned width) {
  uint32_t r = 0;
  for (unsigned k = 0; k < width; ++k) {
    unsigned b = lo + k;
    r |= ((w[b >> 5] >> (b & 31)) & 1u) << k;
  }
  return r;
}
static inline void _arch_blk_get(uint32_t* w, int nw, unsigned __int128 v) {
  for (int k = 0; k < nw; ++k) w[k] = (k < 4) ? (uint32_t)(v >> (32 * k)) : 0u;
}
template <int W> static inline void _arch_blk_get(uint32_t* w, int nw, const VlWide<W>& v) {
  for (int k = 0; k < nw; ++k) w[k] = (k < W) ? v.data()[k] : 0u;
}
template <typename T> static inline void _arch_blk_put(T& dst, const uint32_t* w, int nw) {
  unsigned __int128 acc = 0;
  for (int k = 0; k < nw && k < 4; ++k) acc |= ((unsigned __int128)w[k]) << (32 * k);
  dst = (T)acc;
}
template <int W> static inline void _arch_blk_put(VlWide<W>& dst, const uint32_t* w, int nw) {
  for (int k = 0; k < W; ++k) dst.data()[k] = (k < nw) ? w[k] : 0u;
}
";

fn cpp_words(bits: u32) -> u32 {
    bits.div_ceil(32)
}

/// The C++ definition. Signature is `(input, output&)` rather than a return
/// value because both sides are aggregates in the sim: a wide block is a
/// `VlWide` or an `_arch_u128`, and a `Vec<FP32,N>` is a plain C array,
/// neither of which composes into an expression the way the SV function
/// return does. The block side is a template parameter so the caller's own
/// storage type selects the access overload — see [`CPP_PRELUDE`].
pub fn cpp_definition(h: BlockHelper) -> String {
    match h {
        BlockHelper::Quantize {
            shape,
            policy,
            round,
        } => cpp_quantize(shape, policy, round),
        BlockHelper::Dequantize { shape } => cpp_dequantize(shape),
        BlockHelper::Dot { shape } => cpp_dot(shape),
    }
}

/// C++ twin of [`sv_dot`]. Same schedule, same temp numbering, same order of
/// scale application — compare the two side by side.
///
/// Returns by value (unlike quantize/dequantize): the result is a scalar
/// `FP32`, so it composes as an ordinary expression on both backends. Both
/// block operands are template parameters because the same block type has
/// different C++ storage as a port and as an internal wire, and a dot can mix
/// the two.
fn cpp_dot(s: BlockShape) -> String {
    let name = BlockHelper::Dot { shape: s }.cpp_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let nw = cpp_words(bw);
    let (adds, last) = dot_schedule(n);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: OCP MX §6.2 block dot. Mirrors sv_dot in src/fp_block.rs."
    );
    let _ = writeln!(o, "template <typename BA, typename BB>");
    let _ = writeln!(
        o,
        "static inline uint32_t {name}(const BA& a, const BB& b) {{"
    );
    let _ = writeln!(o, "  uint32_t _wa[{nw}], _wb[{nw}];");
    let _ = writeln!(o, "  _arch_blk_get(_wa, {nw}, a);");
    let _ = writeln!(o, "  _arch_blk_get(_wb, {nw}, b);");
    // Exact products — see the note in sv_dot.
    for i in 0..n as usize {
        let _ = writeln!(
            o,
            "  uint32_t t{i} = _arch_f32_mul(_arch_{tag}_to_f32((uint8_t)_arch_blk_ext(_wa, {lo}u, {ew}u)), \
             _arch_{tag}_to_f32((uint8_t)_arch_blk_ext(_wb, {lo}u, {ew}u)));",
            lo = i * ew as usize
        );
    }
    for (k, (l, r)) in adds.iter().enumerate() {
        let _ = writeln!(
            o,
            "  uint32_t t{} = _arch_f32_add(t{l}, t{r});",
            n as usize + k
        );
    }
    // Scales one at a time — see the note in sv_dot.
    let _ = writeln!(
        o,
        "  uint32_t xa = _{w}((uint8_t)_arch_blk_ext(_wa, {lo}u, {sw}u));",
        w = s.scale.widen_fn(),
        lo = bw - sw
    );
    let _ = writeln!(
        o,
        "  uint32_t xb = _{w}((uint8_t)_arch_blk_ext(_wb, {lo}u, {sw}u));",
        w = s.scale.widen_fn(),
        lo = bw - sw
    );
    let _ = writeln!(o, "  return _arch_f32_mul(_arch_f32_mul(t{last}, xa), xb);");
    let _ = writeln!(o, "}}");
    o
}

fn cpp_quantize(s: BlockShape, policy: ScalePolicy, round: RoundMode) -> String {
    assert_eq!(
        round,
        RoundMode::Rne,
        "only RNE narrowing is lowered today; typecheck refuses the others \
         (arch#890) — reaching here means that gate was removed \
         without adding the rounder variants"
    );
    if s.scale.quant_kernel() == QuantKernel::BoundaryCompare {
        return cpp_quantize_boundary(s, policy);
    }
    let name = BlockHelper::Quantize {
        shape: s,
        policy,
        round,
    }
    .cpp_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, emax, tag) = (s.n, s.elem_emax(), s.elem_tag());
    let nw = cpp_words(bw);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: Vec<FP32,{n}> -> block ({bw} bits). Mirrors sv_quantize in src/fp_block.rs."
    );
    let _ = writeln!(o, "template <typename BT>");
    let _ = writeln!(
        o,
        "static inline void {name}(const uint32_t* v, BT& out) {{"
    );
    let _ = writeln!(o, "  uint32_t _w[{nw}];");
    let _ = writeln!(o, "  for (int k = 0; k < {nw}; ++k) _w[k] = 0u;");
    let _ = writeln!(o, "  uint32_t amax = 0u;");
    let _ = writeln!(o, "  for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(o, "    uint32_t mag = v[i] & 0x7FFFFFFFu;");
    let _ = writeln!(o, "    if (mag > amax) amax = mag;");
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "  if (amax >= 0x7F800000u) {{");
    let _ = writeln!(
        o,
        "    _arch_blk_ins(_w, {}u, {sw}u, 0x{:02X}u);",
        bw - sw,
        s.scale.nan_code()
    );
    let _ = writeln!(o, "  }} else if (amax == 0u) {{");
    let _ = writeln!(
        o,
        "    _arch_blk_ins(_w, {}u, {sw}u, 0x{:02X}u);",
        bw - sw,
        s.scale.zero_block_code()
    );
    let _ = writeln!(o, "  }} else {{");
    let _ = writeln!(o, "    uint8_t ecode = _{}(amax);", s.scale.narrow_fn());
    if policy == ScalePolicy::CeilPow2 {
        let _ = writeln!(
            o,
            "    if (amax > _{}(ecode) && ecode != 0x{:02X}u) ecode = (uint8_t)(ecode + 1u);",
            s.scale.widen_fn(),
            s.scale.max_finite_code()
        );
    }
    let _ = writeln!(
        o,
        "    uint8_t code = (ecode > {emax}u) ? (uint8_t)(ecode - {emax}u) : (uint8_t)0u;"
    );
    let _ = writeln!(
        o,
        "    uint32_t inv = {};",
        s.scale
            .cpp_reciprocal("code")
            .expect("ExactReciprocal kernel implies the scale has one")
    );
    let _ = writeln!(o, "    for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(
        o,
        "      uint32_t p = (uint32_t)_arch_f32_to_{tag}(_arch_f32_mul(v[i], inv));"
    );
    let _ = writeln!(o, "      _arch_blk_ins(_w, i * {ew}u, {ew}u, p);");
    let _ = writeln!(o, "    }}");
    let _ = writeln!(
        o,
        "    _arch_blk_ins(_w, {}u, {sw}u, (uint32_t)code);",
        bw - sw
    );
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "  _arch_blk_put(out, _w, {nw});");
    let _ = writeln!(o, "}}");
    o
}

/// C++ twin of [`sv_quantize_boundary`]. Same two ladders, same order, same
/// constants — the two are meant to be diffable line for line.
fn cpp_quantize_boundary(s: BlockShape, policy: ScalePolicy) -> String {
    let name = BlockHelper::Quantize {
        shape: s,
        policy,
        round: RoundMode::Rne,
    }
    .cpp_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let nw = cpp_words(bw);
    let (scale_base, scale_rungs) = scale_ladder(s.scale, s.desc().max_finite, policy);
    let (elem_rungs, ovf) = elem_ladder(s.desc());
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: Vec<FP32,{n}> -> block ({bw} bits). Mirrors sv_quantize_boundary in src/fp_block.rs."
    );
    let _ = writeln!(o, "template <typename BT>");
    let _ = writeln!(
        o,
        "static inline void {name}(const uint32_t* v, BT& out) {{"
    );
    let _ = writeln!(o, "  uint32_t _w[{nw}];");
    let _ = writeln!(o, "  for (int k = 0; k < {nw}; ++k) _w[k] = 0u;");
    let _ = writeln!(o, "  uint32_t amax = 0u;");
    let _ = writeln!(o, "  for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(o, "    uint32_t mag = v[i] & 0x7FFFFFFFu;");
    let _ = writeln!(o, "    if (mag > amax) amax = mag;");
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "  if (amax >= 0x7F800000u) {{");
    let _ = writeln!(
        o,
        "    _arch_blk_ins(_w, {}u, {sw}u, 0x{:02X}u);",
        bw - sw,
        s.scale.nan_code()
    );
    let _ = writeln!(o, "  }} else if (amax == 0u) {{");
    let _ = writeln!(
        o,
        "    _arch_blk_ins(_w, {}u, {sw}u, 0x{:02X}u);",
        bw - sw,
        s.scale.zero_block_code()
    );
    let _ = writeln!(o, "  }} else {{");
    let _ = writeln!(o, "    uint8_t code = 0x{scale_base:02X}u;");
    for rg in &scale_rungs {
        let op = if rg.strict { ">" } else { ">=" };
        let _ = writeln!(
            o,
            "    if (amax {op} 0x{:08X}u) code = 0x{:02X}u;",
            rg.thr, rg.code
        );
    }
    let _ = writeln!(o, "    uint32_t x = _{}(code);", s.scale.widen_fn());
    let _ = writeln!(o, "    for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(o, "      uint32_t sgn = v[i] >> 31;");
    let _ = writeln!(o, "      uint32_t a = v[i] & 0x7FFFFFFFu;");
    let _ = writeln!(o, "      uint32_t m = 0u;");
    let cmp_rungs = if ovf.is_some() {
        &elem_rungs[..elem_rungs.len() - 1]
    } else {
        &elem_rungs[..]
    };
    for rg in cmp_rungs {
        let op = if rg.strict { ">" } else { ">=" };
        let _ = writeln!(
            o,
            "      if (a {op} _arch_f32_mul(x, 0x{:08X}u)) m = 0x{:02X}u;",
            rg.thr, rg.code
        );
    }
    let _ = writeln!(
        o,
        "      uint32_t p = (sgn << {}) | (m & 0x{:X}u);",
        ew - 1,
        (1u32 << (ew - 1)) - 1
    );
    if ovf.is_some() {
        let top = elem_rungs[elem_rungs.len() - 1];
        let op = if top.strict { ">" } else { ">=" };
        let _ = writeln!(
            o,
            "      if (a {op} _arch_f32_mul(x, 0x{:08X}u)) p = (uint32_t)_arch_f32_to_{tag}((sgn << 31) | 0x7F7FFFFFu);",
            top.thr
        );
    }
    let _ = writeln!(o, "      _arch_blk_ins(_w, i * {ew}u, {ew}u, p);");
    let _ = writeln!(o, "    }}");
    let _ = writeln!(
        o,
        "    _arch_blk_ins(_w, {}u, {sw}u, (uint32_t)code);",
        bw - sw
    );
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "  _arch_blk_put(out, _w, {nw});");
    let _ = writeln!(o, "}}");
    o
}

fn cpp_dequantize(s: BlockShape) -> String {
    let name = BlockHelper::Dequantize { shape: s }.cpp_name();
    let (bw, ew, sw) = (s.bits(), s.elem_width(), s.scale.width());
    let (n, tag) = (s.n, s.elem_tag());
    let nw = cpp_words(bw);
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// {name}: block ({bw} bits) -> Vec<FP32,{n}>. Mirrors sv_dequantize in src/fp_block.rs."
    );
    let _ = writeln!(o, "template <typename BT>");
    let _ = writeln!(
        o,
        "static inline void {name}(const BT& b, uint32_t* out) {{"
    );
    let _ = writeln!(o, "  uint32_t _w[{nw}];");
    let _ = writeln!(o, "  _arch_blk_get(_w, {nw}, b);");
    let _ = writeln!(
        o,
        "  uint32_t x = _{}((uint8_t)_arch_blk_ext(_w, {}u, {sw}u));",
        s.scale.widen_fn(),
        bw - sw
    );
    let _ = writeln!(o, "  for (unsigned i = 0; i < {n}u; ++i) {{");
    let _ = writeln!(
        o,
        "    uint32_t w = _arch_{tag}_to_f32((uint8_t)_arch_blk_ext(_w, i * {ew}u, {ew}u));"
    );
    let _ = writeln!(o, "    uint32_t p = _arch_f32_mul(x, w);");
    let _ = writeln!(
        o,
        "    if ((p & 0x7FFFFFFFu) == 0x7F800000u && (w & 0x7F800000u) != 0x7F800000u)"
    );
    let _ = writeln!(o, "      p = (p & 0x80000000u) | 0x7F7FFFFFu;");
    let _ = writeln!(o, "    out[i] = p;");
    let _ = writeln!(o, "  }}");
    let _ = writeln!(o, "}}");
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fp_format::FORMATS;

    fn shape(elem: FpFormatId, n: u32) -> BlockShape {
        BlockShape {
            elem,
            n,
            scale: BlockScale::E8m0,
        }
    }

    /// `elem_emax` is derived from `max_finite` rather than hand-tabled, so
    /// pin every value it can produce. These are the amounts the shared
    /// exponent is shifted by; an off-by-one here is a silent 2x scale error
    /// across the whole block.
    #[test]
    fn fp_block_emax_matches_each_format_top_binade() {
        for (id, want, why) in [
            (FpFormatId::E2m1, 2u32, "max 6.0 = 1.5 * 2^2"),
            (FpFormatId::E2m3, 2, "max 7.5 = 1.875 * 2^2"),
            (FpFormatId::E3m2, 4, "max 28 = 1.75 * 2^4"),
            (FpFormatId::E4m3, 8, "max 448 = 1.75 * 2^8"),
            (FpFormatId::E5m2, 15, "max 57344 = 1.75 * 2^15"),
        ] {
            assert_eq!(shape(id, 32).elem_emax(), want, "{id:?}: {why}");
        }
    }

    /// The packed width must agree with `fp_format::scaled_vec_width`, which
    /// is what the type checker and both backends size ports from. Two
    /// independent width computations for one layout is exactly the phase-2a
    /// divergence this keeps closed.
    #[test]
    fn fp_block_bits_agree_with_scaled_vec_width() {
        for (id, ty) in [
            (FpFormatId::E2m1, TypeExpr::FP4E2M1),
            (FpFormatId::E2m3, TypeExpr::FP6E2M3),
            (FpFormatId::E3m2, TypeExpr::FP6E3M2),
            (FpFormatId::E4m3, TypeExpr::FP8E4M3),
            (FpFormatId::E5m2, TypeExpr::FP8E5M2),
        ] {
            for n in [1u32, 4, 16, 32] {
                assert_eq!(
                    Some(shape(id, n).bits()),
                    crate::fp_format::scaled_vec_width(&ty, n, &TypeExpr::E8M0),
                    "{id:?} x {n}"
                );
            }
        }
        // MXFP4 is the headline number from the proposal.
        assert_eq!(shape(FpFormatId::E2m1, 32).bits(), 136);
    }

    /// `shape_of` must reject exactly what the type checker rejects. A `None`
    /// here is a refusal; a `Some` for a non-block type would resurrect the
    /// "measured fine in one backend, `unwrap_or(0)` in the other" divergence.
    #[test]
    fn fp_block_shape_of_rejects_non_block_members() {
        assert!(shape_of(&TypeExpr::FP4E2M1, 32, &TypeExpr::E8M0).is_some());
        assert!(shape_of(&TypeExpr::FP32, 32, &TypeExpr::E8M0).is_none());
        assert!(shape_of(&TypeExpr::BF16, 32, &TypeExpr::E8M0).is_none());
        assert!(shape_of(&TypeExpr::Bool, 32, &TypeExpr::E8M0).is_none());
        // FP8E4M3 is a legal ELEMENT but NVFP4's scale is UE4M3, a different
        // format — so it must not be accepted in the scale slot.
        assert!(shape_of(&TypeExpr::FP4E2M1, 32, &TypeExpr::FP8E4M3).is_none());
    }

    /// The two renderings are generated from one descriptor, so what has to
    /// be pinned is that they stay *structurally* parallel: same constants,
    /// same branch order, same helper callees. Values are pinned separately
    /// by the Verilator differential harness, which runs the emitted SV
    /// against this same C++ as the DPI reference.
    #[test]
    fn fp_block_sv_and_cpp_agree_on_shape() {
        for policy in [ScalePolicy::FloorPow2, ScalePolicy::CeilPow2] {
            for (id, tag) in [
                (FpFormatId::E2m1, "e2m1"),
                (FpFormatId::E2m3, "e2m3"),
                (FpFormatId::E3m2, "e3m2"),
                (FpFormatId::E4m3, "e4m3"),
                (FpFormatId::E5m2, "e5m2"),
            ] {
                let s = shape(id, 32);
                let q = BlockHelper::Quantize {
                    shape: s,
                    policy,
                    round: RoundMode::Rne,
                };
                let (sv, cpp) = (sv_definition(q), cpp_definition(q));
                // Both must call the SAME numeric helpers — the whole point
                // of the design is that neither invents arithmetic.
                for callee in ["f32_to_e8m0", "e8m0_to_f32", "f32_mul"] {
                    assert!(sv.contains(callee), "SV {tag} must call {callee}");
                    assert!(cpp.contains(callee), "C++ {tag} must call {callee}");
                }
                assert!(sv.contains(&format!("arch_f32_to_{tag}(")));
                assert!(cpp.contains(&format!("_arch_f32_to_{tag}(")));
                // Same shared constants on both sides.
                assert!(sv.contains("7F800000") && cpp.contains("7F800000u"));
                assert!(sv.contains("8'd254 - code") && cpp.contains("254u - code"));
                let emax = s.elem_emax();
                assert!(sv.contains(&format!("8'd{emax}")));
                assert!(cpp.contains(&format!("{emax}u")));
                // The ceil bump is present iff the policy asks for it.
                assert_eq!(
                    sv.contains("ecode + 8'h01"),
                    policy == ScalePolicy::CeilPow2
                );
                assert_eq!(cpp.contains("ecode + 1u"), policy == ScalePolicy::CeilPow2);

                let d = BlockHelper::Dequantize { shape: s };
                let (dsv, dcpp) = (sv_definition(d), cpp_definition(d));
                assert!(dsv.contains(&format!("arch_{tag}_to_f32(")));
                assert!(dcpp.contains(&format!("_arch_{tag}_to_f32(")));
                // Saturation to +-FP32_MAX, guarded on a finite element.
                assert!(dsv.contains("7F7FFFFF") && dcpp.contains("7F7FFFFFu"));
            }
        }
    }

    /// The emitted C++ must NOT name a concrete block storage type. The sim
    /// gives one block type two different C++ representations — a 72-bit
    /// block is `VlWide<3>` as a port but `_arch_u128` as an internal wire —
    /// so any storage type baked in here would be right for one and a silent
    /// truncation for the other. The destination's own type must select the
    /// access overload.
    #[test]
    fn fp_block_cpp_is_generic_over_block_storage() {
        for bits_case in [
            shape(FpFormatId::E2m1, 4),  // 24 bits — a scalar either way
            shape(FpFormatId::E4m3, 8),  // 72 bits — VlWide as a port, _arch_u128 as a wire
            shape(FpFormatId::E2m1, 32), // 136 bits — VlWide either way
        ] {
            for cpp in [
                cpp_definition(BlockHelper::Quantize {
                    shape: bits_case,
                    policy: ScalePolicy::FloorPow2,
                    round: RoundMode::Rne,
                }),
                cpp_definition(BlockHelper::Dequantize { shape: bits_case }),
            ] {
                assert!(
                    cpp.contains("template <typename BT>"),
                    "block side must be a template parameter:\n{cpp}"
                );
                for banned in ["VlWide<", "_arch_u128", "uint64_t& ", "uint32_t& "] {
                    assert!(
                        !cpp.contains(banned),
                        "emitted C++ names the concrete storage type `{banned}`, which is \
                         right for only one of port/wire storage:\n{cpp}"
                    );
                }
                // …and it must route through the overloaded accessors.
                assert!(cpp.contains("_arch_blk_get(") || cpp.contains("_arch_blk_put("));
            }
        }
    }

    /// **The invariant this refactor exists to create**: the scale helper
    /// names appear only inside `impl BlockScale`, never in a lowering.
    ///
    /// Before, they were string literals in the emitters, so adding a
    /// `BlockScale` variant compiled with ZERO errors and would have emitted
    /// E8M0 scale decoding for an NVFP4 block — no diagnostic, and both
    /// backends wrong identically, so the SV↔sim differential gate would not
    /// have caught it either (arch#904's shape). Adding a variant now
    /// produces nine compile errors, one per scale-specific decision.
    ///
    /// This is deliberately a **source** check. The obvious version — strip
    /// the names the enum supplies from the emitted text and look for
    /// leftovers — is vacuous, because the stripped string *is* the literal
    /// a leak would use, so it removes the very thing it is hunting. With one
    /// variant, only the source can distinguish "came from the enum" from
    /// "hardcoded the same characters". Verified to fail when a literal is
    /// reintroduced.
    #[test]
    fn fp_block_scale_helpers_named_only_in_the_enum() {
        let src = include_str!("fp_block.rs");
        let impl_start = src
            .find("impl BlockScale {")
            .expect("impl BlockScale must exist");
        // Walk to the matching brace so the region is exact rather than
        // guessed from the next `}` at column 0.
        let mut depth = 0usize;
        let mut impl_end = impl_start;
        for (i, c) in src[impl_start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        impl_end = impl_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(
            impl_end > impl_start,
            "unbalanced braces in impl BlockScale"
        );

        // Stop at the test module: this test names the helpers in its own
        // literal, and the lowering is what the invariant is about.
        let scan_end = src.find("#[cfg(test)]").unwrap_or(src.len());
        for (line_no, line) in src[..scan_end].lines().enumerate() {
            let t = line.trim_start();
            // Prose may name the helpers freely — that is how the reasoning
            // gets recorded.
            if t.starts_with("//") {
                continue;
            }
            for helper in ["arch_e8m0_to_f32", "arch_f32_to_e8m0"] {
                if !line.contains(helper) {
                    continue;
                }
                let off = src[..scan_end]
                    .lines()
                    .take(line_no)
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                assert!(
                    off > impl_start && off < impl_end,
                    "src/fp_block.rs:{}: names `{helper}` outside `impl BlockScale`.\n\
                     Route it through `BlockScale::widen_fn()` / `narrow_fn()`, or a \
                     second scale variant silently inherits E8M0's helpers (arch#905).\n\
                     offending line: {line}",
                    line_no + 1
                );
            }
        }
    }

    /// Names must be unique per (op, element, N, scale, policy, rounding) —
    /// a collision would silently give two different designs one definition.
    #[test]
    fn fp_block_names_are_injective() {
        let mut seen = std::collections::BTreeSet::new();
        for id in [
            FpFormatId::E2m1,
            FpFormatId::E2m3,
            FpFormatId::E3m2,
            FpFormatId::E4m3,
            FpFormatId::E5m2,
        ] {
            for n in [4u32, 16, 32] {
                for policy in [ScalePolicy::FloorPow2, ScalePolicy::CeilPow2] {
                    let s = shape(id, n);
                    assert!(seen.insert(
                        BlockHelper::Quantize {
                            shape: s,
                            policy,
                            round: RoundMode::Rne
                        }
                        .sv_name()
                    ));
                    // Dequantize does not depend on policy, so the second
                    // insert is expected to collide — assert it does, which
                    // is what makes the emitters' dedup correct.
                    let d = BlockHelper::Dequantize { shape: s }.sv_name();
                    assert_eq!(seen.insert(d), policy == ScalePolicy::FloorPow2);
                }
            }
        }
    }

    // ── UE4M3 / NVFP4 (phase 5b, arch#905) ────────────────────────────────

    /// Run a ladder the way both backends do: ascending, later rungs win.
    fn run_ladder(rungs: &[Rung], base: u32, x: f32, scaled: bool, a: f32) -> u32 {
        let mut c = base;
        for rg in rungs {
            let thr = if scaled {
                x * f32::from_bits(rg.thr)
            } else {
                f32::from_bits(rg.thr)
            };
            let hit = if rg.strict { a > thr } else { a >= thr };
            if hit {
                c = rg.code;
            }
        }
        c
    }

    /// Reference RNE onto an ascending grid, INDEPENDENT of the ladder: it
    /// divides (in f64, correctly rounded to 53 bits) where the ladder only
    /// ever compares.
    ///
    /// Double rounding 53 → ≤4 significand bits is innocuous everywhere
    /// except exactly on a tie, so ties are settled separately by an exact
    /// test — `x * mid` is exact in FP32, hence exact in f64, and `v` is an
    /// FP32 value, so `v == x * mid` is a decision and not an approximation.
    fn ref_code(grid: &[f64], v: f32, x: f32) -> usize {
        let q = v as f64 / x as f64;
        for k in 0..grid.len() - 1 {
            let mid = (grid[k] + grid[k + 1]) / 2.0;
            let exact_tie = (v as f64) == (x as f64) * mid;
            if exact_tie {
                return if k % 2 == 0 { k } else { k + 1 };
            }
            if q < mid {
                return k;
            }
        }
        grid.len() - 1
    }

    /// The heart of arch#905: the division-free element ladder must agree
    /// with a real division on every scale, for every element format.
    ///
    /// Swept over all 126 finite UE4M3 scales × the values that can actually
    /// disagree — every decision boundary exactly, both neighbours one ULP
    /// away, and every grid point — which is where the reciprocal shortcut
    /// this replaces fails 4.76% of the time.
    #[test]
    fn fp_block_ue4m3_elem_ladder_matches_a_real_divide() {
        let scale_grid = BlockScale::Ue4m3.grid();
        for id in [
            FpFormatId::E2m1,
            FpFormatId::E2m3,
            FpFormatId::E3m2,
            FpFormatId::E4m3,
            FpFormatId::E5m2,
        ] {
            let f = FORMATS.iter().find(|f| f.id == id).unwrap();
            let g = format_grid(f);
            let (rungs, ovf) = elem_ladder(f);
            // The overflow rung is not a grid code, so it is excluded here
            // and covered by `..._overflow_rung_matches_the_rounder`.
            let cmp = if ovf.is_some() {
                &rungs[..rungs.len() - 1]
            } else {
                &rungs[..]
            };
            let top = (g.len() - 1) as u32;
            for (code, &xv) in scale_grid.iter().enumerate().skip(1) {
                let x = xv as f32;
                let mut probes: Vec<f32> = Vec::new();
                for k in 0..g.len() - 1 {
                    let mid = ((g[k] + g[k + 1]) / 2.0) as f32;
                    let t = x * mid;
                    probes.push(t);
                    probes.push(f32::from_bits(t.to_bits() - 1));
                    probes.push(f32::from_bits(t.to_bits() + 1));
                }
                for &gv in &g {
                    probes.push(x * gv as f32);
                }
                for a in probes {
                    if !a.is_finite() || a == 0.0 {
                        continue;
                    }
                    let want = ref_code(&g, a, x).min(top as usize) as u32;
                    // Values past the top grid entry belong to the overflow
                    // rung, not to this ladder.
                    if a as f64 > g[g.len() - 1] * x as f64 {
                        continue;
                    }
                    let got = run_ladder(cmp, 0, x, true, a);
                    assert_eq!(
                        got, want,
                        "{} scale code {code:#04X} (x={x}) value {a:e}: ladder {got:#X} \
                         vs divide {want:#X}",
                        f.type_name
                    );
                }
            }
        }
    }

    /// Every `scale × boundary` product the element ladder compares against
    /// must be EXACT in FP32 — that is the whole reason the ladder is
    /// correctly rounded, and it is a property of the format tables rather
    /// than of the emitted code, so it is worth pinning on its own.
    #[test]
    fn fp_block_ue4m3_ladder_products_are_exact() {
        for id in [
            FpFormatId::E2m1,
            FpFormatId::E2m3,
            FpFormatId::E3m2,
            FpFormatId::E4m3,
            FpFormatId::E5m2,
        ] {
            let f = FORMATS.iter().find(|f| f.id == id).unwrap();
            let (rungs, _) = elem_ladder(f);
            for (code, &xv) in BlockScale::Ue4m3.grid().iter().enumerate().skip(1) {
                for rg in &rungs {
                    let (x, m) = (xv as f32, f32::from_bits(rg.thr));
                    let p = x * m;
                    assert!(
                        p.is_finite() && p != 0.0,
                        "{} code {code:#04X}",
                        f.type_name
                    );
                    assert_eq!(
                        p as f64,
                        x as f64 * m as f64,
                        "{} scale code {code:#04X}: {x} * {m} rounds in FP32",
                        f.type_name
                    );
                }
            }
        }
    }

    /// `exact` selects `RNE(amax / elem_max)`. Same construction as the
    /// element ladder, so the same independent check applies.
    #[test]
    fn fp_block_ue4m3_exact_scale_ladder_matches_a_real_divide() {
        let g = BlockScale::Ue4m3.grid();
        for id in [
            FpFormatId::E2m1,
            FpFormatId::E2m3,
            FpFormatId::E3m2,
            FpFormatId::E4m3,
            FpFormatId::E5m2,
        ] {
            let f = FORMATS.iter().find(|f| f.id == id).unwrap();
            let cmax = f.max_finite;
            let (base, rungs) = scale_ladder(BlockScale::Ue4m3, cmax, ScalePolicy::Exact);
            for k in 1..g.len() - 1 {
                let mid = (g[k] + g[k + 1]) / 2.0;
                let t = (cmax * mid) as f32;
                for amax in [
                    t,
                    f32::from_bits(t.to_bits() - 1),
                    f32::from_bits(t.to_bits() + 1),
                    (cmax * g[k]) as f32,
                ] {
                    if !amax.is_finite() || amax <= 0.0 {
                        continue;
                    }
                    let want = ref_code(&g, amax, cmax as f32).max(1) as u32;
                    let got = run_ladder(&rungs, base, 1.0, false, amax);
                    assert_eq!(
                        got, want,
                        "{} amax {amax:e}: scale ladder {got:#04X} vs divide {want:#04X}",
                        f.type_name
                    );
                }
            }
        }
    }

    /// A UE4M3 block must never select scale code `0x00`. That code is a
    /// genuine ZERO (unlike E8M0's `0x00`, which is its minimum scale), so
    /// selecting it for a block whose maximum is nonzero would erase the
    /// whole block on dequantize.
    #[test]
    fn fp_block_ue4m3_scale_ladder_never_selects_zero() {
        for policy in [
            ScalePolicy::Exact,
            ScalePolicy::FloorPow2,
            ScalePolicy::CeilPow2,
        ] {
            for id in [FpFormatId::E2m1, FpFormatId::E4m3] {
                let f = FORMATS.iter().find(|f| f.id == id).unwrap();
                let (base, rungs) = scale_ladder(BlockScale::Ue4m3, f.max_finite, policy);
                assert_ne!(base, 0, "{policy:?}/{} base is the zero code", f.type_name);
                for rg in &rungs {
                    assert_ne!(rg.code, 0, "{policy:?}/{} rung selects zero", f.type_name);
                }
            }
        }
    }

    /// The three special scale codes, pinned per scale with their reasons.
    ///
    /// These are constants consumed by BOTH backends from one table, so a
    /// wrong value is wrong identically in the SystemVerilog and the C++ and
    /// the cross-backend byte-compare cannot see it — the arch#904 shape.
    /// This test is the only thing standing between a typo here and a silent
    /// miscompile, so it states the value AND why it is that value.
    #[test]
    fn fp_block_special_scale_codes() {
        // E8M0: no zero encoding, NaN is the all-ones code, so the largest
        // finite is one below it.
        assert_eq!(BlockScale::E8m0.nan_code(), 0xFF);
        assert_eq!(BlockScale::E8m0.zero_block_code(), 0x00);
        assert_eq!(BlockScale::E8m0.max_finite_code(), 0xFE);

        // UE4M3: 7 significant bits with the MSB padded zero, so the NaN is
        // 0x7F and NOT 0xFF — 0xFF would set the padding bit the format
        // requires to be zero. 0x00 is a genuine zero (E8M0's is 2^-127).
        assert_eq!(BlockScale::Ue4m3.nan_code(), 0x7F);
        assert_eq!(BlockScale::Ue4m3.zero_block_code(), 0x00);
        assert_eq!(BlockScale::Ue4m3.max_finite_code(), 0x7E);

        // Cross-checks against the grid, so the codes cannot drift from the
        // format they describe: the NaN code is one past the last finite
        // one, and UE4M3's code 0 really is zero while E8M0's really is not.
        for s in [BlockScale::E8m0, BlockScale::Ue4m3] {
            let g = s.grid();
            assert_eq!(g.len() as u32 - 1, s.max_finite_code(), "{s:?}");
            assert_eq!(s.max_finite_code() + 1, s.nan_code(), "{s:?}");
            assert!(g.iter().all(|v| *v > 0.0) == s.is_pow2(), "{s:?}");
        }
        assert_eq!(BlockScale::Ue4m3.grid()[0], 0.0);
        assert_ne!(BlockScale::E8m0.grid()[0], 0.0);
    }

    /// The scale-dependent default (maintainer sign-off 2026-08-12): a
    /// `floor_pow2` default under UE4M3 would discard all three mantissa bits
    /// and quietly emit a power-of-two-scale block where NVFP4 was asked for.
    #[test]
    fn fp_block_default_policy_is_scale_dependent() {
        assert_eq!(BlockScale::E8m0.default_policy(), ScalePolicy::FloorPow2);
        assert_eq!(BlockScale::Ue4m3.default_policy(), ScalePolicy::Exact);
        assert!(BlockScale::E8m0.is_pow2());
        assert!(!BlockScale::Ue4m3.is_pow2());
    }

    /// Only a scale WITHOUT an exact reciprocal takes the division-free path,
    /// and only a scale WITH one offers a reciprocal. Pins the two halves
    /// together so a future variant cannot claim both or neither.
    #[test]
    fn fp_block_quant_kernel_agrees_with_reciprocal_availability() {
        for s in [BlockScale::E8m0, BlockScale::Ue4m3] {
            let has_recip = s.sv_reciprocal("c").is_some();
            assert_eq!(has_recip, s.cpp_reciprocal("c").is_some());
            assert_eq!(
                has_recip,
                s.quant_kernel() == QuantKernel::ExactReciprocal,
                "{s:?}"
            );
            assert_eq!(has_recip, s.is_pow2(), "{s:?}");
        }
    }

    /// The emitted UE4M3 quantizer must contain no divide and no reciprocal
    /// helper — the entire point of arch#905's kernel.
    #[test]
    fn fp_block_ue4m3_quantize_emits_no_division() {
        let s = BlockShape {
            elem: FpFormatId::E2m1,
            n: 16,
            scale: BlockScale::Ue4m3,
        };
        for policy in [
            ScalePolicy::Exact,
            ScalePolicy::FloorPow2,
            ScalePolicy::CeilPow2,
        ] {
            let sv = sv_quantize(s, policy, RoundMode::Rne);
            let cpp = cpp_quantize(s, policy, RoundMode::Rne);
            for (what, text) in [("sv", &sv), ("cpp", &cpp)] {
                // Comments carry `/` in file paths and `->` arrows, so the
                // check is against code lines only.
                let code: String = text
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("//"))
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(!code.contains('/'), "{what}/{policy:?} contains a divide");
                assert!(
                    !code.contains("254"),
                    "{what}/{policy:?} reuses E8M0's reciprocal identity"
                );
                assert!(
                    code.contains("arch_f32_mul"),
                    "{what}/{policy:?} lost the scaled-threshold multiply"
                );
            }
        }
    }
}
