//! Compile-time, single-rounding-step conversion of a decimal float literal
//! (parsed to `f64` by the lexer/parser — `f64::parse` is correctly rounded,
//! so the `f64` already carries the exact-enough value of the source text)
//! down to a narrower IEEE-754-like binary format, using round-to-nearest,
//! ties-to-even (RNE).
//!
//! This is the shared helper behind arch#622 (context-typed float literals)
//! and arch#624 (BF16 `init` constant folding): every "known float type"
//! literal slot (typed `let`, `reg`/`port reg` `init`, comparisons against a
//! known-format operand, …) rounds the literal directly from its parsed
//! `f64` value to the target format's bit pattern **in one step**, instead of
//! routing through an intermediate FP32 rounding (which is what the existing
//! `(lit).to_bf16()` eval path does for the `reset` slot — see
//! `elaborate::coerce_bf16_decl_literals`). Single-step rounding is
//! unconditionally sound: for any target format with `p` significand bits
//! (including the implicit bit), rounding an `f64` (53-bit significand)
//! directly to `p` bits is exact-per-IEEE-754 as long as the source already
//! carries enough precision, which `f64` (53 bits) does for both FP32
//! (p = 24: 53 >= 2*24+2 is not required here since there's no *second*
//! rounding step — the 53 -> p step is the *only* step) and BF16 (p = 8).
//!
//! Double-rounding through an intermediate format (e.g. decimal -> f64 -> f32
//! -> bf16) is a *different*, weaker guarantee (safe only when the
//! intermediate format has enough headroom over the final one) and is
//! intentionally NOT what this module does — see the reset-path note above.
//!
//! `exp_bits`/`mant_bits` are `u32` so this same routine backs any future
//! narrow format (FP16, FP8, MX block formats, …) — see `doc/archive/plan_fp_types.md`.

/// Round `x` to the nearest value representable in an IEEE-754-like binary
/// format with `exp_bits` exponent bits (bias = `2^(exp_bits-1) - 1`, and the
/// standard IEEE-754 special-value encoding: all-zero exponent = zero /
/// subnormal, all-one exponent = infinity / NaN) and `mant_bits` explicit
/// mantissa bits, using round-to-nearest-even. Returns the packed
/// `sign(1) | exponent(exp_bits) | mantissa(mant_bits)` bit pattern,
/// right-justified in the returned `u64` (so a 16-bit BF16 result occupies
/// bits `[15:0]`, a 32-bit FP32 result occupies bits `[31:0]`).
///
/// Performs exactly one rounding step directly from the `f64` bit pattern —
/// it never rounds through an intermediate lower-precision format — so it is
/// free of double-rounding artifacts. For `(exp_bits=8, mant_bits=23)` (FP32)
/// this agrees bit-for-bit with Rust's native `(x as f32).to_bits()` for
/// every finite, non-overflowing input (see `fp_lit_test` for randomized +
/// edge-case verification); `(exp_bits=8, mant_bits=7)` produces BF16.
pub fn round_f64_to_narrow(x: f64, exp_bits: u32, mant_bits: u32) -> u64 {
    debug_assert!((2..=16).contains(&exp_bits));
    debug_assert!(mant_bits <= 52);

    let bits64 = x.to_bits();
    let sign: u64 = (bits64 >> 63) & 1;
    let exp64 = ((bits64 >> 52) & 0x7FF) as i64;
    let mant64 = bits64 & 0x000F_FFFF_FFFF_FFFF; // 52 bits

    let dst_bias: i64 = (1i64 << (exp_bits - 1)) - 1;
    let dst_max_exp: i64 = (1i64 << exp_bits) - 1; // all-ones exponent field
    let sign_shift = exp_bits + mant_bits;
    let mant_mask: u64 = if mant_bits == 0 {
        0
    } else {
        (1u64 << mant_bits) - 1
    };

    let pack = |exp_field: i64, mant_field: u64| -> u64 {
        (sign << sign_shift) | ((exp_field as u64) << mant_bits) | (mant_field & mant_mask)
    };

    // NaN: canonical quiet NaN in the target format (exponent all-ones, top
    // mantissa bit set — sign and payload of the source NaN are not
    // preserved; float literals in source text never parse to NaN, so this
    // arm exists purely for totality).
    if exp64 == 0x7FF && mant64 != 0 {
        let nan_mant = if mant_bits > 0 {
            1u64 << (mant_bits - 1)
        } else {
            1
        };
        return pack(dst_max_exp, nan_mant);
    }
    // Infinity.
    if exp64 == 0x7FF {
        return pack(dst_max_exp, 0);
    }
    // Zero (+0 / -0).
    if exp64 == 0 && mant64 == 0 {
        return pack(0, 0);
    }

    // Normalize to a significand `sig53` (up to 53 bits, MSB may be below
    // bit 52 for a subnormal `f64`) and binary exponent `e` such that
    // `value = sig53 * 2^e`.
    let (sig53, e): (u64, i64) = if exp64 == 0 {
        // f64 subnormal: value = mant64 * 2^(1 - 1023 - 52).
        (mant64, 1 - 1023 - 52)
    } else {
        (mant64 | (1u64 << 52), exp64 - 1023 - 52)
    };

    // Position of the significand's MSB (0-indexed); `value ≈ 1.xxx *
    // 2^(e + k)` when normalized.
    let k = 63 - sig53.leading_zeros() as i64;
    let true_exp = e + k;

    let min_normal_exp = 1 - dst_bias;

    // Number of low bits of `sig53` being rounded away, and the destination
    // biased exponent this rounding targets. `frac_bits <= 0` means the
    // source already fits losslessly (no rounding needed, only a shift).
    let (frac_bits, exp_field0): (i64, i64) = if true_exp >= min_normal_exp {
        (k - mant_bits as i64, true_exp + dst_bias)
    } else {
        // Subnormal in the destination format: align to 2^min_normal_exp,
        // which drops `(min_normal_exp - true_exp)` additional low bits
        // relative to a normal result.
        (k - mant_bits as i64 + (min_normal_exp - true_exp), 0)
    };

    if frac_bits <= 0 {
        let shift = (-frac_bits) as u32;
        let mant_field = (sig53 << shift) & mant_mask;
        return pack(exp_field0, mant_field);
    }
    if frac_bits >= 64 {
        // Value underflows to zero even after rounding (magnitude far below
        // the smallest subnormal). Round-to-nearest-even of an all-zero
        // significand-plus-sticky is zero.
        return pack(0, 0);
    }

    let kept = sig53 >> frac_bits;
    let round_bit = (sig53 >> (frac_bits - 1)) & 1;
    let sticky = frac_bits >= 2 && (sig53 & ((1u64 << (frac_bits - 1)) - 1)) != 0;
    let round_up = round_bit == 1 && (sticky || (kept & 1) == 1);

    // Normal results keep the implicit leading 1 in `kept` (at bit
    // `mant_bits`), so overflow-into-a-new-exponent shows up one bit higher,
    // at bit `mant_bits + 1`. Subnormal results carry no implicit bit, so a
    // round-up that reaches exactly `1 << mant_bits` *is* the overflow (it
    // lands precisely on the smallest-normal boundary, mantissa field 0).
    let is_normal = exp_field0 > 0;
    let overflow_bit = if is_normal {
        1u64 << (mant_bits + 1)
    } else {
        1u64 << mant_bits
    };

    let mut kept = kept;
    let mut exp_field = exp_field0;
    if round_up {
        kept += 1;
        if kept & overflow_bit != 0 {
            exp_field += 1;
        }
    }
    if exp_field >= dst_max_exp {
        return pack(dst_max_exp, 0); // overflow to infinity
    }
    pack(exp_field, kept)
}

/// Round `x` to BF16 (1 sign + 8 exponent + 7 mantissa bits). Single rounding
/// step directly from the `f64` source — see module docs.
pub fn f64_to_bf16_bits(x: f64) -> u16 {
    round_f64_to_narrow(x, 8, 7) as u16
}

/// Round `x` to FP8 E5M2 (1 + 5 + 2, IEEE-style (5,3) instantiation).
/// Returns `None` when the rounded magnitude overflows the largest finite
/// (57344) — fp8 literal overflow is a compile error, not a fold to ±inf,
/// because runtime overflow behavior is `--fp-compat`-dependent and a source
/// constant must not depend on a build flag.
pub fn f64_to_e5m2_bits(x: f64) -> Option<u8> {
    let bits = round_f64_to_narrow(x, 5, 2);
    // Exponent field all-ones = the generic rounder's overflow-to-infinity
    // packing (or a NaN input, which source literals cannot produce).
    if (bits >> 2) & 0x1F == 0x1F {
        return None;
    }
    Some(bits as u8)
}

/// Round `x` to FP8 E4M3 with **OCP OFP8 semantics**: no infinities, the top
/// exponent value (15) encodes the normals 256..448 for mantissa 0..6 and NaN
/// at mantissa 7, max finite 448. Returns `None` on overflow (rounded
/// magnitude ≥ 480; the 464 tie goes to 448 — even kept significand).
///
/// The generic `round_f64_to_narrow(x, 4, 3)` cannot be used above 248: its
/// IEEE template treats every `exp_field == 15` result as overflow and packs
/// `S.1111.000`, which in OCP is the *finite value 256* — a silently wrong
/// constant for e.g. 448.0. The top binade is therefore handled directly.
pub fn f64_to_e4m3_bits(x: f64) -> Option<u8> {
    if !x.is_finite() {
        return None; // totality; literals cannot parse to NaN/inf
    }
    let a = x.abs();
    let sign: u8 = if x.is_sign_negative() { 0x80 } else { 0 };
    if a >= 256.0 {
        // Top OCP binade: grid step 32 (= 2^8 / 8), encodings exp=15,
        // mant = m-8 for m in 8..=14 (256..448). f64 division by 32 is
        // exact, so round-half-even on the exact quotient is exact RNE.
        let q = a / 32.0;
        let f = q.floor();
        let frac = q - f; // exact
        let m = if frac > 0.5 {
            f as u64 + 1
        } else if frac < 0.5 {
            f as u64
        } else if (f as u64) % 2 == 0 {
            f as u64
        } else {
            f as u64 + 1
        };
        if m >= 15 {
            return None; // rounded magnitude ≥ 480 (the S.1111.111 NaN slot)
        }
        return Some(sign | 0x78 | (m as u8 - 8));
    }
    // Below 256 the OCP and IEEE-(4,4) grids coincide, and the generic
    // rounder's overflow packing `S.1111.000` is exactly the encoding of 256
    // — the only value a sub-256 input can round up into. So the generic
    // result is correct verbatim here.
    Some(round_f64_to_narrow(x, 4, 3) as u8)
}

/// Round `x` to FP32 / IEEE-754 binary32 (1 + 8 + 23 bits). Agrees bit-for-
/// bit with `(x as f32).to_bits()`.
pub fn f64_to_fp32_bits(x: f64) -> u32 {
    round_f64_to_narrow(x, 8, 23) as u32
}

/// Exact decode of an FP8 E5M2 bit pattern to `f64` (IEEE-style (5,3):
/// exp all-ones = inf/NaN).
pub fn e5m2_bits_to_f64(h: u8) -> f64 {
    let sign = if h & 0x80 != 0 { -1.0 } else { 1.0 };
    let e = (h >> 2) & 0x1F;
    let f = (h & 0x3) as f64;
    if e == 0x1F {
        return if f != 0.0 { f64::NAN } else { sign * f64::INFINITY };
    }
    if e == 0 {
        return sign * f * f64::powi(2.0, -16);
    }
    sign * (4.0 + f) * f64::powi(2.0, e as i32 - 17)
}

/// Exact decode of an FP8 E4M3 bit pattern to `f64` (OCP OFP8: no
/// infinities; only S.1111.111 is NaN; exp=15, mant<7 are the normals
/// 256..448).
pub fn e4m3_bits_to_f64(h: u8) -> f64 {
    let sign = if h & 0x80 != 0 { -1.0 } else { 1.0 };
    if h & 0x7F == 0x7F {
        return f64::NAN;
    }
    let e = (h >> 3) & 0xF;
    let f = (h & 0x7) as f64;
    if e == 0 {
        return sign * f * f64::powi(2.0, -9);
    }
    sign * (8.0 + f) * f64::powi(2.0, e as i32 - 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e5m2_known_values() {
        assert_eq!(f64_to_e5m2_bits(0.0), Some(0x00));
        assert_eq!(f64_to_e5m2_bits(-0.0), Some(0x80));
        assert_eq!(f64_to_e5m2_bits(1.0), Some(0x3C));
        assert_eq!(f64_to_e5m2_bits(1.5), Some(0x3E));
        assert_eq!(f64_to_e5m2_bits(57344.0), Some(0x7B)); // max finite
        assert_eq!(f64_to_e5m2_bits(-57344.0), Some(0xFB));
        // min subnormal 2^-16
        assert_eq!(f64_to_e5m2_bits(f64::powi(2.0, -16)), Some(0x01));
        // half the min subnormal ties to even -> zero
        assert_eq!(f64_to_e5m2_bits(f64::powi(2.0, -17)), Some(0x00));
    }

    #[test]
    fn e5m2_overflow_is_none() {
        // 61440 is the tie between 57344 and the (would-be) 65536: kept
        // significand 7 (odd) vs carry to even -> rounds up -> overflow.
        assert_eq!(f64_to_e5m2_bits(61440.0), None);
        assert_eq!(f64_to_e5m2_bits(61439.9), Some(0x7B));
        assert_eq!(f64_to_e5m2_bits(1.0e9), None);
        assert_eq!(f64_to_e5m2_bits(-61440.0), None);
    }

    #[test]
    fn e4m3_known_values() {
        assert_eq!(f64_to_e4m3_bits(0.0), Some(0x00));
        assert_eq!(f64_to_e4m3_bits(1.0), Some(0x38));
        assert_eq!(f64_to_e4m3_bits(1.5), Some(0x3C));
        assert_eq!(f64_to_e4m3_bits(240.0), Some(0x77));
        assert_eq!(f64_to_e4m3_bits(256.0), Some(0x78));
        assert_eq!(f64_to_e4m3_bits(448.0), Some(0x7E)); // max finite
        assert_eq!(f64_to_e4m3_bits(-448.0), Some(0xFE));
        // min subnormal 2^-9; half of it ties to even -> zero
        assert_eq!(f64_to_e4m3_bits(f64::powi(2.0, -9)), Some(0x01));
        assert_eq!(f64_to_e4m3_bits(f64::powi(2.0, -10)), Some(0x00));
    }

    #[test]
    fn e4m3_ocp_top_binade_boundaries() {
        // 248 ties between 240 (kept 15, odd) and 256 (even) -> 256.
        assert_eq!(f64_to_e4m3_bits(248.0), Some(0x78));
        assert_eq!(f64_to_e4m3_bits(247.9), Some(0x77));
        // 449..464 round to 448 (464 ties: kept 14 even vs 15 odd -> 448).
        assert_eq!(f64_to_e4m3_bits(449.0), Some(0x7E));
        assert_eq!(f64_to_e4m3_bits(464.0), Some(0x7E));
        // Above the tie: rounds toward the NaN slot 480 -> overflow.
        assert_eq!(f64_to_e4m3_bits(464.1), None);
        assert_eq!(f64_to_e4m3_bits(480.0), None);
        assert_eq!(f64_to_e4m3_bits(500.0), None);
        assert_eq!(f64_to_e4m3_bits(-465.0), None);
        // The silent-wrong-constant trap: 448 must NOT encode as 256.
        assert_ne!(f64_to_e4m3_bits(448.0), Some(0x78));
    }

    #[test]
    fn fp32_matches_native_cast_known_values() {
        let vals: &[f64] = &[
            0.0,
            -0.0,
            1.0,
            1.5,
            -1.5,
            0.5,
            -0.5,
            std::f64::consts::PI,
            100.0,
            -100.0,
            0.1,
            0.2,
            1.0 / 3.0,
            1e30,
            1e-30,
            f64::MIN_POSITIVE,
            f64::MAX,
            123456789.123456,
            2.0f64.powi(-149), // smallest f32 subnormal magnitude
            2.0f64.powi(-126), // smallest f32 normal
            (2.0f64 - 2.0f64.powi(-23)) * 2.0f64.powi(127), // near f32 max
        ];
        for &v in vals {
            assert_eq!(
                f64_to_fp32_bits(v),
                (v as f32).to_bits(),
                "mismatch for {v}"
            );
            assert_eq!(
                f64_to_fp32_bits(-v),
                (-v as f32).to_bits(),
                "mismatch for {}",
                -v
            );
        }
    }

    #[test]
    fn fp32_matches_native_cast_randomized() {
        // Deterministic xorshift PRNG (no extra dev-dependency) over a wide
        // range of f64 bit patterns, restricted to finite values, checked
        // against Rust's native f64->f32 cast.
        let mut state: u64 = 0x9E3779B97F4A7C15;
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let v = f64::from_bits(state);
            if !v.is_finite() {
                continue;
            }
            assert_eq!(
                f64_to_fp32_bits(v),
                (v as f32).to_bits(),
                "mismatch for {v:e} (bits {:#018x})",
                state
            );
        }
    }

    #[test]
    fn fp32_infinity_and_nan() {
        assert_eq!(f64_to_fp32_bits(f64::INFINITY), f32::INFINITY.to_bits());
        assert_eq!(
            f64_to_fp32_bits(f64::NEG_INFINITY),
            f32::NEG_INFINITY.to_bits()
        );
        // Overflow to infinity: a finite f64 too large for f32.
        assert_eq!(f64_to_fp32_bits(1e40), f32::INFINITY.to_bits());
        assert_eq!(f64_to_fp32_bits(-1e40), f32::NEG_INFINITY.to_bits());
    }

    #[test]
    fn bf16_known_values() {
        // bf16 = top 16 bits of the IEEE-754 binary32 pattern, rounded (not
        // truncated) to nearest-even.
        assert_eq!(f64_to_bf16_bits(1.5), 0x3FC0);
        assert_eq!(f64_to_bf16_bits(-1.5), 0xBFC0);
        assert_eq!(f64_to_bf16_bits(0.5), 0x3F00);
        assert_eq!(f64_to_bf16_bits(1.0), 0x3F80);
        assert_eq!(f64_to_bf16_bits(2.0), 0x4000);
        assert_eq!(f64_to_bf16_bits(0.0), 0x0000);
        assert_eq!(f64_to_bf16_bits(-0.0), 0x8000);
        // pi's f32 pattern is 0x40490FDB; rounding the top 16 bits, the guard
        // bit (0x0FDB's top bit) is 0, so it stays 0x4049 (verified against
        // the f32_to_bf16 SV/sim helper's rounding in the existing #623 test,
        // which reads back the same constant via the eval path).
        assert_eq!(f64_to_bf16_bits(std::f64::consts::PI), 0x4049);
    }

    #[test]
    fn bf16_rounds_up_on_tie_to_even() {
        // Construct an f32 value whose low 16 bits are exactly 0x8000 (the
        // tie point) with kept-mantissa LSB = 1, so RNE rounds up.
        // f32 pattern: sign=0 exp=01111111 (127, so value in [1,2)) mantissa
        // = 0000000_1000000000000000 (top 7 bits 0000000, bit 15 = 1, rest 0)
        // -> value = 1.0 + 2^-16, tie between bf16(0000000) and bf16(0000001);
        // kept LSB (bit position 16 of the 23-bit mantissa, i.e. the top
        // kept bit) is 0 here, so ties-to-even keeps it: 0x3F80.
        let f32_bits: u32 = 0x3F80_8000;
        let v = f32::from_bits(f32_bits) as f64;
        // Tie, kept mantissa LSB is 0 (even) -> stays.
        assert_eq!(f64_to_bf16_bits(v), 0x3F80);

        // Now the same tie but with kept LSB = 1 (odd) -> rounds up.
        let f32_bits2: u32 = 0x3F81_8000; // mantissa top7 = 0000001, bit15=1
        let v2 = f32::from_bits(f32_bits2) as f64;
        assert_eq!(f64_to_bf16_bits(v2), 0x3F82);
    }

    #[test]
    fn bf16_subnormal_and_overflow() {
        // Smallest bf16 subnormal magnitude: 2^-133 (exp field 0, mantissa
        // 0000001). bf16 has the same exponent range as f32, so its min
        // subnormal is 2^(-126-7) = 2^-133.
        let smallest = 2.0f64.powi(-133);
        assert_eq!(f64_to_bf16_bits(smallest), 0x0001);
        // Half of that underflows to zero.
        assert_eq!(f64_to_bf16_bits(smallest / 2.0), 0x0000);
        // A value too large for bf16 (same max exponent as f32, so anything
        // representable in f64 above f32::MAX also overflows bf16 — but even
        // an in-f32-range value can round *up* into bf16 infinity near the
        // top of the range).
        assert_eq!(f64_to_bf16_bits(1e40), 0x7F80); // +inf
        assert_eq!(f64_to_bf16_bits(-1e40), 0xFF80); // -inf
    }

    /// Mirror of the runtime f32 -> bf16 RNE narrow (`arch_f32_to_bf16` /
    /// `_arch_f2bf16`): round the top 16 bits with ties-to-even via the
    /// add-and-shift trick.
    fn f32_to_bf16_rne(f: f32) -> u16 {
        let u = f.to_bits();
        let lsb = (u >> 16) & 1;
        (u.wrapping_add(0x7FFF + lsb) >> 16) as u16
    }

    /// The witness that the direct decimal -> f64 -> bf16 single-rounding fold
    /// and the legacy f32-routed path (decimal -> f64 -> f32 -> bf16, the
    /// pre-unification #623 reset behavior) are NOT the same function: a
    /// decimal that lands strictly between a bf16 rounding midpoint m and
    /// m + half-f32-ulp collapses onto m in the f32 step, and the second
    /// rounding then ties-to-even the wrong way.
    ///
    /// x = 1 + 2^-8 + 2^-30 (exactly `1.003906250931322574615478515625`):
    ///  - direct:  x is strictly above the midpoint between bf16 1.0 (0x3F80)
    ///             and 1+2^-7 (0x3F81) -> correctly rounds UP to 0x3F81.
    ///  - via f32: RN_f32(x) = 1 + 2^-8 exactly (x is within half an f32-ulp
    ///             of it) = the bf16 midpoint -> tie -> even significand ->
    ///             0x3F80. Off by 1 bf16 ULP from the correctly-rounded value.
    ///
    /// This is why the reset slot's f32-routed rewrite was superseded
    /// (maintainer decision on arch#622/#624): all compile-time float
    /// constants now take the single-rounding path, which is the correctly-
    /// rounded one.
    #[test]
    fn double_rounding_via_f32_diverges_on_witness() {
        let x: f64 = "1.003906250931322574615478515625".parse().unwrap();
        // The decimal is exact: it is 1 + 2^-8 + 2^-30, representable in f64.
        assert_eq!(x, 1.0 + 2.0f64.powi(-8) + 2.0f64.powi(-30));
        // Direct single-step fold (the shipped semantics): correctly rounded.
        assert_eq!(f64_to_bf16_bits(x), 0x3F81);
        // Legacy f32-routed double rounding: collapses to the midpoint, then
        // ties down to even — 1 ULP below the correctly-rounded result.
        assert_eq!(f32_to_bf16_rne(x as f32), 0x3F80);
    }

    /// For every *ordinary* literal (anything not engineered to sit within
    /// half an f32-ulp of a bf16 midpoint), the two paths agree — randomized
    /// lock that the reset-slot unification is invisible outside the
    /// witness class above.
    #[test]
    fn double_rounding_via_f32_agrees_off_midpoints() {
        let mut state: u64 = 0x243F6A8885A308D3;
        let mut checked = 0u32;
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let v = f64::from_bits(state);
            if !v.is_finite() {
                continue;
            }
            // Skip the divergence class: values whose f32 rounding lands
            // exactly on a bf16 rounding boundary tie (low 16 bits of the
            // f32 pattern == 0x8000). Everything else must agree.
            let f = v as f32;
            if !f.is_finite() || (f.to_bits() & 0xFFFF) == 0x8000 {
                continue;
            }
            assert_eq!(
                f64_to_bf16_bits(v),
                f32_to_bf16_rne(f),
                "paths diverged off-midpoint for {v:e} (bits {:#018x})",
                state
            );
            checked += 1;
        }
        assert!(checked > 100_000, "sample too small: {checked}");
    }
}
