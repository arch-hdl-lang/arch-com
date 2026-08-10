//! Single source of truth for every floating-point format the compiler knows.
//!
//! Before this table, each format's facts — carrier width, exponent/mantissa
//! split, whether Inf/NaN exist, max finite magnitude, the operator-dispatch
//! tag, the user-facing type name — were re-derived by hand at roughly forty
//! sites across typecheck, elaborate, SV codegen, sim codegen and formal. Two
//! of those hand-written maps ended in a wildcard that is correct *only*
//! because exactly two 8-bit formats exist today:
//!
//! ```ignore
//! fn float_tag_width(tag: &str) -> u32 {
//!     match tag { "f32" => 32, "bf16" => 16, _ => 8 }   // a 4-bit format gets 8
//! }
//! let (name, max) = match fmt {
//!     FloatLitFmt::E4m3 => ("FP8E4M3", 448.0),
//!     _ => ("FP8E5M2", 57344.0),          // any new format misreports as E5M2
//! };
//! ```
//!
//! Both fail *silently* — wrong widths and wrong diagnostics, not crashes.
//! Routing them through [`FORMATS`] makes a new format a single table row.
//!
//! **Why the tags stay `&'static str`:** operator names are built by
//! interpolation (`arch_{tag}_add`) across three backends, so the tag must be
//! a string at the point of use. A `&str` cannot be matched exhaustively,
//! which is exactly how the wildcards above came to exist. The table closes
//! that hole from the other side: [`FpFormatId`] *is* exhaustively matchable,
//! every string lookup ([`by_tag`]) returns an `Option` rather than guessing,
//! and `fp_format_table_is_consistent` cross-checks the table against every
//! independent vocabulary in the compiler, so a format added to one and not
//! the other fails the build's tests rather than miscompiling.

use crate::ast::{FloatLitFmt, TypeExpr};

/// Canonical identifier for a floating-point format. Exhaustively matchable,
/// unlike the `&'static str` dispatch tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpFormatId {
    Fp32,
    Bf16,
    E4m3,
    E5m2,
    E2m1,
    E2m3,
    E3m2,
}

/// How a format encodes NaN.
///
/// This cannot be derived from the exponent/mantissa split alone: OCP E4M3
/// spends its would-be Inf/NaN space on finite values and reserves exactly
/// one NaN code, so it needs a different bit test from the IEEE-shaped
/// formats. Hand-written per-backend NaN tables got this right for the four
/// shipped formats but fell back to an IEEE test (at the *wrong* field
/// offsets) for anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NanRule {
    /// IEEE-shaped: exponent field all ones AND mantissa nonzero.
    IeeeExpAllOnes,
    /// OCP-shaped: a single NaN code with every magnitude bit set
    /// (`S.1111.111` for E4M3). Sign is not part of the test.
    OcpAllMagnitudeOnes,
    /// No NaN encoding exists (OCP E2M1 / E2M3 / E3M2). `is_nan` on such a
    /// format must be a compile error, never a constant `false`.
    NoNan,
}

/// Everything the compiler needs to know about one floating-point format.
#[derive(Debug, Clone, Copy)]
pub struct FpFormat {
    pub id: FpFormatId,
    /// Operator-dispatch tag: the `{tag}` in `arch_{tag}_add`, `arch_fma_{tag}`.
    pub tag: &'static str,
    /// User-facing type name, as written in source and in diagnostics.
    pub type_name: &'static str,
    /// Carrier width in bits.
    pub width: u32,
    pub exp_bits: u32,
    pub mant_bits: u32,
    /// Does the encoding reserve an infinity? (OCP E4M3 does not.)
    pub has_inf: bool,
    /// Does the encoding reserve any NaN? Sub-8-bit OCP formats do not, which
    /// is why `is_nan` must be a compile error there rather than a constant
    /// `false`.
    pub has_nan: bool,
    /// Largest finite magnitude, used for literal-overflow diagnostics.
    pub max_finite: f64,
    /// How this format encodes NaN — drives every backend's `is_nan` test.
    pub nan_rule: NanRule,
    /// Does this format carry a full arithmetic surface (`+ - *`, `fma`,
    /// compares)? A storage-only format is a carrier for conversions and
    /// literals but has no operators — see `Ty::is_float_arith`.
    pub arith: bool,
}

/// Every known format. **Adding a format means adding a row here**; the
/// consistency test then forces the other vocabularies to keep up.
pub const FORMATS: &[FpFormat] = &[
    FpFormat {
        id: FpFormatId::Fp32,
        tag: "f32",
        type_name: "FP32",
        width: 32,
        exp_bits: 8,
        mant_bits: 23,
        has_inf: true,
        has_nan: true,
        max_finite: 3.402_823_466_385_288_6e38,
        nan_rule: NanRule::IeeeExpAllOnes,
        arith: true,
    },
    FpFormat {
        id: FpFormatId::Bf16,
        tag: "bf16",
        type_name: "BF16",
        width: 16,
        exp_bits: 8,
        mant_bits: 7,
        has_inf: true,
        has_nan: true,
        max_finite: 3.389_531_389_251_535_5e38,
        nan_rule: NanRule::IeeeExpAllOnes,
        arith: true,
    },
    FpFormat {
        // OCP OFP8 E4M3: no infinities, sole NaN encoding 0x7F, max 448.
        id: FpFormatId::E4m3,
        tag: "e4m3",
        type_name: "FP8E4M3",
        width: 8,
        exp_bits: 4,
        mant_bits: 3,
        has_inf: false,
        has_nan: true,
        max_finite: 448.0,
        nan_rule: NanRule::OcpAllMagnitudeOnes,
        arith: true,
    },
    FpFormat {
        // IEEE-style (5,3): ±inf 0x7C, max finite 57344.
        id: FpFormatId::E5m2,
        tag: "e5m2",
        type_name: "FP8E5M2",
        width: 8,
        exp_bits: 5,
        mant_bits: 2,
        has_inf: true,
        has_nan: true,
        max_finite: 57344.0,
        nan_rule: NanRule::IeeeExpAllOnes,
        arith: true,
    },
    FpFormat {
        // OCP MX FP4 E2M1: no Inf, NO NaN, max finite 6.0, one subnormal
        // (0.5). Storage-only — see `arith`.
        id: FpFormatId::E2m1,
        tag: "e2m1",
        type_name: "FP4E2M1",
        width: 4,
        exp_bits: 2,
        mant_bits: 1,
        has_inf: false,
        has_nan: false,
        max_finite: 6.0,
        nan_rule: NanRule::NoNan,
        // No shipping GPU/accelerator ISA exposes scalar E2M1 arithmetic;
        // PTX states e2m1 "must be used in a packed format" and that
        // alternate formats "cannot be used as fundamental types". It is a
        // block element, so it carries conversions and literals only.
        arith: false,
    },
    FpFormat {
        // OCP MX FP6 E2M3: no Inf, no NaN, max finite 7.5. Storage-only.
        id: FpFormatId::E2m3,
        tag: "e2m3",
        type_name: "FP6E2M3",
        width: 6,
        exp_bits: 2,
        mant_bits: 3,
        has_inf: false,
        has_nan: false,
        max_finite: 7.5,
        nan_rule: NanRule::NoNan,
        arith: false,
    },
    FpFormat {
        // OCP MX FP6 E3M2: no Inf, no NaN, max finite 28.0. Storage-only.
        id: FpFormatId::E3m2,
        tag: "e3m2",
        type_name: "FP6E3M2",
        width: 6,
        exp_bits: 3,
        mant_bits: 2,
        has_inf: false,
        has_nan: false,
        max_finite: 28.0,
        nan_rule: NanRule::NoNan,
        arith: false,
    },
];

impl FpFormat {
    /// `(hi, lo)` bit indices of the exponent field.
    ///
    /// Derived, not tabulated, so it cannot drift from `width`/`mant_bits`:
    /// f32 → (30, 23), bf16 → (14, 7), e4m3 → (6, 3), e5m2 → (6, 2).
    pub fn exp_field(&self) -> (u32, u32) {
        (self.width - 2, self.mant_bits)
    }

    /// `(hi, lo)` bit indices of the mantissa field, or `None` for a format
    /// with no mantissa (an exponent-only scale type such as E8M0).
    pub fn mant_field(&self) -> Option<(u32, u32)> {
        if self.mant_bits == 0 {
            None
        } else {
            Some((self.mant_bits - 1, 0))
        }
    }

    /// `(hi, lo)` bit indices of the magnitude — everything but the sign.
    pub fn magnitude_field(&self) -> (u32, u32) {
        (self.width - 2, 0)
    }

    /// Width of the magnitude field in bits.
    pub fn magnitude_bits(&self) -> u32 {
        self.width - 1
    }
}

/// Descriptor for a canonical id. Total by construction.
pub fn by_id(id: FpFormatId) -> &'static FpFormat {
    match FORMATS.iter().find(|f| f.id == id) {
        Some(f) => f,
        // Unreachable while `fp_format_table_is_consistent` passes, which
        // asserts every variant has a row.
        None => unreachable!("FORMATS is missing a row for {id:?}"),
    }
}

/// Descriptor for an operator-dispatch tag. Returns `None` for an unknown
/// tag rather than guessing — the guess is what made the old
/// `float_tag_width` silently wrong for any format narrower than 8 bits.
pub fn by_tag(tag: &str) -> Option<&'static FpFormat> {
    FORMATS.iter().find(|f| f.tag == tag)
}

/// Descriptor for a literal format.
pub fn by_lit_fmt(fmt: FloatLitFmt) -> &'static FpFormat {
    by_id(match fmt {
        FloatLitFmt::Fp32 => FpFormatId::Fp32,
        FloatLitFmt::Bf16 => FpFormatId::Bf16,
        FloatLitFmt::E4m3 => FpFormatId::E4m3,
        FloatLitFmt::E5m2 => FpFormatId::E5m2,
        FloatLitFmt::E2m1 => FpFormatId::E2m1,
        FloatLitFmt::E2m3 => FpFormatId::E2m3,
        FloatLitFmt::E3m2 => FpFormatId::E3m2,
    })
}

/// Descriptor for a surface type, if it is a float at all.
pub fn by_type_expr(ty: &TypeExpr) -> Option<&'static FpFormat> {
    let id = match ty {
        TypeExpr::FP32 => FpFormatId::Fp32,
        TypeExpr::BF16 => FpFormatId::Bf16,
        TypeExpr::FP8E4M3 => FpFormatId::E4m3,
        TypeExpr::FP8E5M2 => FpFormatId::E5m2,
        TypeExpr::FP4E2M1 => FpFormatId::E2m1,
        TypeExpr::FP6E2M3 => FpFormatId::E2m3,
        TypeExpr::FP6E3M2 => FpFormatId::E3m2,
        _ => return None,
    };
    Some(by_id(id))
}

/// Carrier width for a dispatch tag; `None` for an unknown tag.
pub fn width_of_tag(tag: &str) -> Option<u32> {
    by_tag(tag).map(|f| f.width)
}

/// Storage width of a type usable as a `ScaledVec` element or scale.
///
/// This is `by_type_expr` plus `E8M0`, which is deliberately **not** a float
/// (no sign, no mantissa, no zero) and therefore has no row in the table —
/// but is the MX block scale, so it must be measurable here.
pub fn block_member_width(ty: &TypeExpr) -> Option<u32> {
    match ty {
        TypeExpr::E8M0 => Some(8),
        other => by_type_expr(other).map(|f| f.width),
    }
}

/// Packed width of `ScaledVec<Elem, N, Scale>` = `scale_w + N * elem_w`.
///
/// The canonical layout is `{ scale[w-1:0], P[N-1], …, P[1], P[0] }` — scale
/// in the high bits, element 0 in the low bits, matching ARCH's existing `Vec`
/// convention (proposal §3.2, decision #8). MXFP4 (`FP4E2M1`, 32, `E8M0`) is
/// therefore 8 + 32*4 = 136 bits.
///
/// `n` is the already-const-evaluated block size: every pass owns its own
/// constant folder, so callers evaluate `N` and pass the number in. Returns
/// `None` if either member type cannot live in a block, or on overflow.
pub fn scaled_vec_width(elem: &TypeExpr, n: u32, scale: &TypeExpr) -> Option<u32> {
    let elem_w = block_member_width(elem)?;
    let scale_w = block_member_width(scale)?;
    n.checked_mul(elem_w)?.checked_add(scale_w)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is only a single source of truth if it AGREES with every
    /// vocabulary that still derives these facts independently. This test is
    /// the mechanism that makes a half-added format fail loudly instead of
    /// miscompiling: add a row here without teaching `FloatLitFmt`, or vice
    /// versa, and this fails.
    #[test]
    fn fp_format_table_is_consistent() {
        // 1. Every canonical id has exactly one row.
        for id in [
            FpFormatId::Fp32,
            FpFormatId::Bf16,
            FpFormatId::E4m3,
            FpFormatId::E5m2,
            FpFormatId::E2m1,
            FpFormatId::E2m3,
            FpFormatId::E3m2,
        ] {
            let rows = FORMATS.iter().filter(|f| f.id == id).count();
            assert_eq!(rows, 1, "expected exactly one row for {id:?}, got {rows}");
        }

        // 2. Tags and type names are unique — they are lookup keys.
        for f in FORMATS {
            assert_eq!(
                FORMATS.iter().filter(|g| g.tag == f.tag).count(),
                1,
                "duplicate tag {}",
                f.tag
            );
            assert_eq!(
                FORMATS
                    .iter()
                    .filter(|g| g.type_name == f.type_name)
                    .count(),
                1,
                "duplicate type_name {}",
                f.type_name
            );
        }

        // 3. Width == 1 sign + exp + mantissa, for every row.
        for f in FORMATS {
            assert_eq!(
                f.width,
                1 + f.exp_bits + f.mant_bits,
                "{}: width {} != 1+{}+{}",
                f.tag,
                f.width,
                f.exp_bits,
                f.mant_bits
            );
        }

        // 4. Agreement with `FloatLitFmt`, which carries its own copies.
        for fmt in [
            FloatLitFmt::Fp32,
            FloatLitFmt::Bf16,
            FloatLitFmt::E4m3,
            FloatLitFmt::E5m2,
            FloatLitFmt::E2m1,
            FloatLitFmt::E2m3,
            FloatLitFmt::E3m2,
        ] {
            let f = by_lit_fmt(fmt);
            assert_eq!(f.width, fmt.width(), "{}: width disagrees", f.tag);
            assert_eq!(
                (f.exp_bits, f.mant_bits),
                fmt.exp_mant_bits(),
                "{}: exp/mant disagrees",
                f.tag
            );
        }

        // 5. Agreement with the surface types.
        for (ty, tag) in [
            (TypeExpr::FP32, "f32"),
            (TypeExpr::BF16, "bf16"),
            (TypeExpr::FP8E4M3, "e4m3"),
            (TypeExpr::FP8E5M2, "e5m2"),
            (TypeExpr::FP4E2M1, "e2m1"),
            (TypeExpr::FP6E2M3, "e2m3"),
            (TypeExpr::FP6E3M2, "e3m2"),
        ] {
            let f = by_type_expr(&ty).expect("surface float type must have a row");
            assert_eq!(f.tag, tag);
        }
        assert!(
            by_type_expr(&TypeExpr::Bool).is_none(),
            "non-float type must not resolve to a format"
        );

        // 6. Tag lookup is total over known tags and honest about the rest.
        for f in FORMATS {
            assert_eq!(width_of_tag(f.tag), Some(f.width));
        }
        assert_eq!(
            width_of_tag("nosuchfmt"),
            None,
            "an unknown tag must be None, never a guessed width — guessing is \
             what made the old float_tag_width silently wrong"
        );
    }

    /// Phase 0 must be INERT for the four formats that ship today: the table
    /// has to reproduce the hand-written maps it replaced, bit for bit.
    /// These are the previous implementations, transcribed, so a table edit
    /// that would have changed compiler output fails here.
    #[test]
    fn fp_format_reproduces_the_maps_it_replaced() {
        // Old `formal::float_tag_width`:
        //     match tag { "f32" => 32, "bf16" => 16, _ => 8 }
        let old_tag_width = |tag: &str| -> u32 {
            match tag {
                "f32" => 32,
                "bf16" => 16,
                _ => 8,
            }
        };
        // Only the four formats that existed when the maps were written.
        // A format added AFTER them is precisely what the old wildcards got
        // wrong, so it must NOT agree — see the E2M1 assertion below.
        for tag in ["f32", "bf16", "e4m3", "e5m2"] {
            assert_eq!(
                width_of_tag(tag),
                Some(old_tag_width(tag)),
                "{tag}: table width must match the map it replaced"
            );
        }
        // The whole point of the table: the old `_ => 8` wildcard would have
        // given E2M1 an 8-bit carrier. It is 4.
        assert_eq!(width_of_tag("e2m1"), Some(4));
        assert_eq!(old_tag_width("e2m1"), 8, "the bug this table removed");

        // Old `elaborate` literal-overflow table:
        //     match fmt { E4m3 => ("FP8E4M3", 448.0), _ => ("FP8E5M2", 57344.0) }
        // Only E4m3/E5m2 could reach it (fp32/bf16 literal encoders never
        // return None), so only those two are pinned as behavior.
        for (fmt, name, max) in [
            (FloatLitFmt::E4m3, "FP8E4M3", 448.0_f64),
            (FloatLitFmt::E5m2, "FP8E5M2", 57344.0_f64),
        ] {
            let d = by_lit_fmt(fmt);
            assert_eq!(d.type_name, name, "overflow diagnostic name changed");
            assert_eq!(d.max_finite, max, "overflow diagnostic bound changed");
        }

        // Old `Ty::is_float` and the new `Ty::is_float_arith` must agree
        // while every shipped format is arithmetic-capable — that is what
        // makes routing the `+ - *` / `fma` / `is_nan` gates through the new
        // predicate a no-op today.
        // is_float() and is_float_arith() now genuinely differ, which is
        // what makes `a + b` on E2M1 a clean type error.
        assert!(
            FORMATS.iter().any(|f| !f.arith),
            "the storage-only path is unexercised"
        );
    }

    /// Pins the facts that drive user-visible diagnostics, so a typo in the
    /// table shows up here rather than in an error message.
    #[test]
    fn fp_format_specials_and_maxima() {
        let e4m3 = by_id(FpFormatId::E4m3);
        assert!(!e4m3.has_inf, "OCP E4M3 has no infinity");
        assert!(e4m3.has_nan, "OCP E4M3 has the sole NaN 0x7F");
        assert_eq!(e4m3.max_finite, 448.0);

        let e5m2 = by_id(FpFormatId::E5m2);
        assert!(e5m2.has_inf && e5m2.has_nan);
        assert_eq!(e5m2.max_finite, 57344.0);

        // FP4E2M1 is the first storage-only format: a carrier for
        // conversions and literals with no operator surface at all.
        let e2m1 = by_id(FpFormatId::E2m1);
        assert!(!e2m1.arith, "E2M1 is storage-only");
        assert!(!e2m1.has_nan, "E2M1 has no NaN encoding");
        assert!(!e2m1.has_inf, "E2M1 has no infinity");
        assert_eq!(e2m1.max_finite, 6.0);
        assert_eq!(e2m1.nan_rule, NanRule::NoNan);
        // Every 8-bit-or-wider format still carries full arithmetic.
        assert!(FORMATS.iter().filter(|f| f.width >= 8).all(|f| f.arith));
    }
}
