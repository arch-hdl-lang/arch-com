//! Floating-point operators, defined once against the shared bit-vector IR
//! (`crate::fp_ir`). `fp_functions(profile)` returns the full helper set; the
//! same `Vec<FpFn>` renders to synthesizable SystemVerilog (the `arch build`
//! output) and to SMT-LIB2 (the `arch formal` equivalence proofs), so the two
//! cannot diverge. Profile constants (canonical NaN / NaN→int) follow
//! `FpCompat` exactly as the old text shim did.

use crate::fp_ir::*;
use crate::FpCompat;

fn nan32(p: FpCompat) -> u128 {
    match p {
        FpCompat::Riscv => 0x7FC0_0000,
        FpCompat::Cuda => 0x7FFF_FFFF,
    }
}
fn nan16(p: FpCompat) -> u128 {
    match p {
        FpCompat::Riscv => 0x7FC0,
        FpCompat::Cuda => 0x7FFF,
    }
}

// ── decode (inlined) ────────────────────────────────────────────────────────

struct Dec {
    sign: Bv,    // 1
    mant: Bv,    // 24
    eunb: Bv,    // 16, signed
    is_zero: Bv, // 1
    is_inf: Bv,  // 1
    is_nan: Bv,  // 1
}

/// Unbiased exponent `eunb` for the 16-bit signed field: a normal value is
/// `mant * 2^(e-150)`, a subnormal is `mant * 2^-149` (mant top bit 0).
const NEG149_16: u128 = (1u128 << 16) - 149;

fn decode(x: &Bv) -> Dec {
    let e = extract(x, 30, 23); // 8
    let f = extract(x, 22, 0); // 23
    let sign = extract(x, 31, 31);
    let e_is_ff = eq(&e, &cst(0xFF, 8));
    let e_is_0 = eq(&e, &cst(0, 8));
    let f_nz = ne(&f, &cst(0, 23));
    let f_z = eq(&f, &cst(0, 23));
    Dec {
        sign,
        mant: ite(&e_is_0, &concat(&cst(0, 1), &f), &concat(&cst(1, 1), &f)),
        eunb: ite(
            &e_is_0,
            &cst(NEG149_16, 16),
            &sub(&zext(&e, 16), &cst(150, 16)),
        ),
        is_zero: and(&e_is_0, &f_z),
        is_inf: and(&e_is_ff, &f_z),
        is_nan: and(&e_is_ff, &f_nz),
    }
}

// ── round-and-pack (inlined, generic in the significand width) ──────────────
//
// Rounds the value `(sig * 2^e0)` to nearest-even f32. `sign` is 1-bit; `e0` is
// 16-bit signed. The MSB search is a Rust-unrolled priority fold (no loop in the
// emitted code). Mirrors the C++ sim and the prior SV exactly; the §8.2
// differential harness is the oracle for that equivalence.

fn one1() -> Bv {
    cst(1, 1)
}
fn is1(b: &Bv) -> Bv {
    eq(b, &one1())
}

/// Index of the most-significant set bit of a non-zero value, via a log-depth
/// binary search (count-leading-zeros). Compact in the emitted code regardless
/// of width — `O(log W)` operations rather than `O(W)`.
fn msb_index(sig: &Bv) -> Bv {
    let w = sig.width();
    let mut cur = sig.clone();
    let mut clz = cst(0, 16);
    let mut step = 1u32;
    while step * 2 <= w {
        step *= 2;
    }
    loop {
        let top = extract(&cur, w - 1, w - step); // top `step` bits
        let z = eq(&top, &cst(0, step));
        cur = ite(&z, &shl(&cur, &cst(step as u128, 16)), &cur);
        clz = ite(&z, &add(&clz, &cst(step as u128, 16)), &clz);
        if step == 1 {
            break;
        }
        step /= 2;
    }
    sub(&cst((w - 1) as u128, 16), &clz) // p = (W-1) - clz
}

fn normround(sign: &Bv, sig: &Bv, e0: &Bv) -> Bv {
    let w = sig.width();
    let w2 = w + 2;
    let zsig = zext(sig, w2);

    let p = msb_index(sig); // index of the MSB (sig is non-zero on this path)

    let ev = add(&p, e0); // E (16 signed)
    let biased = add(&ev, &cst(127, 16));
    let biased_le0 = sle(&biased, &cst(0, 16));
    let k = ite(&biased_le0, &cst(NEG149_16, 16), &sub(&ev, &cst(23, 16)));
    let sh = sub(&k, e0); // low bits to drop (16 signed)
    let sh_le0 = sle(&sh, &cst(0, 16));

    let kept_left = shl(&zsig, &neg(&sh));
    let kept_right = lshr(&zsig, &sh);
    let kept0 = ite(&sh_le0, &kept_left, &kept_right);

    let gpos = sub(&sh, &cst(1, 16)); // sh-1 (only used when sh>=1)
    let guard = ite(&sh_le0, &cst(0, 1), &extract(&lshr(&zsig, &gpos), 0, 0));
    let mask = sub(&shl(&cst(1, w2), &gpos), &cst(1, w2));
    let sticky = ite(&sh_le0, &cst(0, 1), &ne(&band(&zsig, &mask), &cst(0, w2)));

    let roundup = and(&guard, &or(&sticky, &extract(&kept0, 0, 0)));
    let kept = add(&kept0, &zext(&roundup, w2));

    // subnormal: {exp,frac} encoding carries up to the smallest normal for free.
    let sub_res = bor(
        &concat(sign, &cst(0, 31)),
        &concat(sign, &extract(&kept, 30, 0)),
    );

    // normal: a carry into bit 24 bumps the exponent; >=255 overflows to inf.
    let carry = is1(&extract(&kept, 24, 24));
    let biased_n = ite(&carry, &add(&biased, &cst(1, 16)), &biased);
    let kept_n = ite(&carry, &lshr(&kept, &cst(1, 16)), &kept);
    let overflow = sge(&biased_n, &cst(255, 16));
    let inf = concat(sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let packed = concat(
        sign,
        &concat(&extract(&biased_n, 7, 0), &extract(&kept_n, 22, 0)),
    );
    let norm_res = ite(&overflow, &inf, &packed);

    let zero = concat(sign, &cst(0, 31));
    ite(
        &eq(sig, &cst(0, w)),
        &zero,
        &ite(&biased_le0, &sub_res, &norm_res),
    )
}

/// Rounding result **and** the three IEEE flags computable at round-and-pack:
/// `(result, overflow, underflow, inexact)`. The `result` is bit-identical to
/// [`normround`] (same formula) — this is a *separate* function so the
/// machine-proved `normround` stays byte-identical; the flags reuse its existing
/// `guard`/`sticky`/`overflow`/`biased_le0` signals:
/// - `inexact`   = a nonzero bit was dropped (`guard | sticky`),
/// - `overflow`  = normal-branch result rounded to `±Inf`,
/// - `underflow` = subnormal-branch result **and** inexact (IEEE tininess +
///   inexactness — this excludes the benign flush of a negligible addend, whose
///   result stays normal, and exact subnormal results).
fn normround_flags(sign: &Bv, sig: &Bv, e0: &Bv) -> (Bv, Bv, Bv, Bv) {
    let w = sig.width();
    let w2 = w + 2;
    let zsig = zext(sig, w2);
    let p = msb_index(sig);
    let ev = add(&p, e0);
    let biased = add(&ev, &cst(127, 16));
    let biased_le0 = sle(&biased, &cst(0, 16));
    let k = ite(&biased_le0, &cst(NEG149_16, 16), &sub(&ev, &cst(23, 16)));
    let sh = sub(&k, e0);
    let sh_le0 = sle(&sh, &cst(0, 16));
    let kept_left = shl(&zsig, &neg(&sh));
    let kept_right = lshr(&zsig, &sh);
    let kept0 = ite(&sh_le0, &kept_left, &kept_right);
    let gpos = sub(&sh, &cst(1, 16));
    let guard = ite(&sh_le0, &cst(0, 1), &extract(&lshr(&zsig, &gpos), 0, 0));
    let mask = sub(&shl(&cst(1, w2), &gpos), &cst(1, w2));
    let sticky = ite(&sh_le0, &cst(0, 1), &ne(&band(&zsig, &mask), &cst(0, w2)));
    let roundup = and(&guard, &or(&sticky, &extract(&kept0, 0, 0)));
    let kept = add(&kept0, &zext(&roundup, w2));
    let sub_res = bor(
        &concat(sign, &cst(0, 31)),
        &concat(sign, &extract(&kept, 30, 0)),
    );
    let carry = is1(&extract(&kept, 24, 24));
    let biased_n = ite(&carry, &add(&biased, &cst(1, 16)), &biased);
    let kept_n = ite(&carry, &lshr(&kept, &cst(1, 16)), &kept);
    let overflow = sge(&biased_n, &cst(255, 16));
    let inf = concat(sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let packed = concat(
        sign,
        &concat(&extract(&biased_n, 7, 0), &extract(&kept_n, 22, 0)),
    );
    let norm_res = ite(&overflow, &inf, &packed);
    let zero = concat(sign, &cst(0, 31));
    let sig_nz = ne(sig, &cst(0, w));
    let result = ite(
        &eq(sig, &cst(0, w)),
        &zero,
        &ite(&biased_le0, &sub_res, &norm_res),
    );
    let inexact_raw = or(&guard, &sticky);
    let f_overflow = and(&sig_nz, &and(&bnot(&biased_le0), &overflow));
    // IEEE: overflow (result → ±Inf) is always inexact, even when the mantissa
    // was exact (e.g. MAX+MAX), since Inf ≠ the true finite value.
    let f_inexact = and(&sig_nz, &or(&inexact_raw, &f_overflow));
    let f_underflow = and(&sig_nz, &and(&biased_le0, &inexact_raw));
    (result, f_overflow, f_underflow, f_inexact)
}

// ── predicates / simple ops (as expressions for reuse) ──────────────────────

fn isnan(x: &Bv) -> Bv {
    and(
        &eq(&extract(x, 30, 23), &cst(0xFF, 8)),
        &ne(&extract(x, 22, 0), &cst(0, 23)),
    )
}
fn iszero(x: &Bv) -> Bv {
    eq(&extract(x, 30, 0), &cst(0, 31))
}
fn eq_expr(a: &Bv, b: &Bv) -> Bv {
    ite(
        &or(&isnan(a), &isnan(b)),
        &cst(0, 1),
        &or(&eq(a, b), &and(&iszero(a), &iszero(b))),
    )
}
fn lt_expr(a: &Bv, b: &Bv) -> Bv {
    let sa = extract(a, 31, 31);
    let sb = extract(b, 31, 31);
    let ma = extract(a, 30, 0);
    let mb = extract(b, 30, 0);
    let same_sign_cmp = ite(&eq(&sa, &cst(0, 1)), &ult(&ma, &mb), &ugt(&ma, &mb));
    let diff_sign = ite(&ne(&sa, &sb), &is1(&sa), &same_sign_cmp);
    ite(
        &or(&isnan(a), &isnan(b)),
        &cst(0, 1),
        &ite(&and(&iszero(a), &iszero(b)), &cst(0, 1), &diff_sign),
    )
}

// ── f32 operators ───────────────────────────────────────────────────────────

fn f32_mul(p: FpCompat) -> FpFn {
    let a = var("a", 32);
    let b = var("b", 32);
    let da = decode(&a);
    let db = decode(&b);
    let sy = bxor(&da.sign, &db.sign);
    let mp = mul(&zext(&da.mant, 48), &zext(&db.mant, 48));
    let e0 = add(&da.eunb, &db.eunb);
    let rounded = normround(&sy, &mp, &e0);
    let inf = concat(&sy, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let zero = concat(&sy, &cst(0, 31));
    let n = cst(nan32(p), 32);
    let body = ite(
        &or(&da.is_nan, &db.is_nan),
        &n,
        &ite(
            &or(&and(&da.is_inf, &db.is_zero), &and(&db.is_inf, &da.is_zero)),
            &n,
            &ite(
                &or(&da.is_inf, &db.is_inf),
                &inf,
                &ite(&or(&da.is_zero, &db.is_zero), &zero, &rounded),
            ),
        ),
    );
    FpFn::new("arch_f32_mul", &[("a", 32), ("b", 32)], 32, body)
}

fn f32_canon(p: FpCompat) -> FpFn {
    let x = var("x", 32);
    let body = ite(&isnan(&x), &cst(nan32(p), 32), &x);
    FpFn::new("arch_f32_canon", &[("x", 32)], 32, body)
}

fn cmp_fn(name: &str, body: Bv) -> FpFn {
    FpFn::new(name, &[("a", 32), ("b", 32)], 1, body)
}
fn f32_compares() -> Vec<FpFn> {
    let a = || var("a", 32);
    let b = || var("b", 32);
    vec![
        cmp_fn("arch_f32_eq", eq_expr(&a(), &b())),
        cmp_fn("arch_f32_ne", bnot(&eq_expr(&a(), &b()))),
        cmp_fn("arch_f32_lt", lt_expr(&a(), &b())),
        cmp_fn("arch_f32_gt", lt_expr(&b(), &a())),
        cmp_fn(
            "arch_f32_le",
            or(&lt_expr(&a(), &b()), &eq_expr(&a(), &b())),
        ),
        cmp_fn(
            "arch_f32_ge",
            or(&lt_expr(&b(), &a()), &eq_expr(&a(), &b())),
        ),
    ]
}

fn bf16_to_f32(p: FpCompat) -> FpFn {
    let h = var("h", 16);
    let z = concat(&h, &cst(0, 16));
    let body = ite(&isnan(&z), &cst(nan32(p), 32), &z);
    FpFn::new("arch_bf16_to_f32", &[("h", 16)], 32, body)
}

fn f32_to_bf16(p: FpCompat) -> FpFn {
    let x = var("x", 32);
    let lsb = extract(&x, 16, 16);
    let rbit = extract(&x, 15, 15);
    let sticky = ne(&extract(&x, 14, 0), &cst(0, 15));
    let roundup = and(&rbit, &or(&sticky, &lsb));
    let sum = add(&x, &ite(&is1(&roundup), &cst(0x0001_0000, 32), &cst(0, 32)));
    let body = ite(&isnan(&x), &cst(nan16(p), 16), &extract(&sum, 31, 16));
    FpFn::new("arch_f32_to_bf16", &[("x", 32)], 16, body)
}

// ── fp8 (FP8E4M3 / FP8E5M2) ─────────────────────────────────────────────────
//
// Two 8-bit formats, routed through the f32 datapath exactly like bf16
// (widen -> f32 op -> narrow):
//   E5M2 = IEEE-style (5,3): bias 15, infinities, NaN class, max finite 57344.
//   E4M3 = OCP OFP8:         bias 7, NO infinities, the top exponent value
//          (15) encodes normals 256..448 for mantissa 0..6 and NaN only at
//          S.1111.111 (0x7F/0xFF), max finite 448.
// f32->fp8 narrowing overflow is profile-dependent (--fp-compat):
//   riscv: non-saturating — E5M2 overflows to ±inf, E4M3 to canonical NaN
//          0x7F (sign dropped; OFP8 non-saturating conversion); input ±inf
//          maps the same way.
//   cuda:  saturate to ±max-finite for both formats, including ±inf inputs
//          (PTX cvt.rn.satfinite / Transformer-Engine convention).
// Widening is exact for every finite input and reuses `normround` (a <=4-bit
// significand packs into f32 with no rounding). The narrow uses `fp8_round`,
// a sibling of `normround` parameterized for 8-bit targets — deliberately a
// SEPARATE function so the machine-proved f32 rounder stays byte-identical.

/// Canonical quiet NaN, E4M3: `0x7F` is the only NaN encoding OFP8 has
/// (canonicalization drops the sign), so there is no profile choice.
fn nan8_e4m3(_p: FpCompat) -> u128 {
    0x7F
}
/// Canonical quiet NaN, E5M2 (mirrors `nan16`/`nan32`): riscv `0x7E`
/// (quiet-bit-set, zero payload), cuda `0x7F` (all-ones mantissa).
fn nan8_e5m2(p: FpCompat) -> u128 {
    match p {
        FpCompat::Riscv => 0x7E,
        FpCompat::Cuda => 0x7F,
    }
}

/// Round `(sig * 2^e0)` (sign-magnitude, `e0` 16-bit signed, `sig` up to 24
/// bits) to nearest-even in an 8-bit float format with `eb` exponent and `mb`
/// mantissa bits. Structure mirrors `normround` with the f32 constants
/// parameterized: bias `2^(eb-1)-1`, subnormal anchor `-(bias-1+mb)`, carry
/// bit at `mb+1`. `ocp_top` selects the OFP8 overflow rule (the top exponent
/// value is still finite except at an all-ones mantissa) vs the IEEE rule
/// (any result reaching the top exponent value overflows). `ovf_res` is the
/// profile-dependent overflow result.
/// How a narrow format spends its top exponent value — the only part of the
/// rounder that differs between the formats it serves.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TopBinade {
    /// IEEE: the all-ones exponent is Inf/NaN, so reaching it is overflow.
    Ieee,
    /// OCP OFP8 E4M3: the all-ones exponent is finite EXCEPT for the
    /// all-ones mantissa, which is the sole NaN. Overflow needs the
    /// rounded magnitude to reach that slot.
    OcpNanTop,
    /// OCP FP4/FP6: no Inf and no NaN at all — the top binade is entirely
    /// finite, so only exceeding it is overflow. Without this arm a 4-bit
    /// format would take the IEEE rule and treat its two largest finite
    /// values (4.0 and 6.0 for E2M1) as overflow.
    AllFinite,
}

fn fp8_round(eb: u32, mb: u32, top: TopBinade, sign: &Bv, sig: &Bv, e0: &Bv, ovf_res: &Bv) -> Bv {
    let w = sig.width();
    let w2 = w + 2;
    let zsig = zext(sig, w2);

    let p_msb = msb_index(sig); // sig is non-zero on this path

    let bias: u128 = (1u128 << (eb - 1)) - 1;
    let anchor_neg: u128 = (1u128 << 16) - (bias + mb as u128 - 1); // -(bias-1+mb) as 16-bit
    let max_expf: u128 = (1u128 << eb) - 1;

    let ev = add(&p_msb, e0);
    let biased = add(&ev, &cst(bias, 16));
    let biased_le0 = sle(&biased, &cst(0, 16));
    let k = ite(
        &biased_le0,
        &cst(anchor_neg, 16),
        &sub(&ev, &cst(mb as u128, 16)),
    );
    let sh = sub(&k, e0);
    let sh_le0 = sle(&sh, &cst(0, 16));

    let kept_left = shl(&zsig, &neg(&sh));
    let kept_right = lshr(&zsig, &sh);
    let kept0 = ite(&sh_le0, &kept_left, &kept_right);

    let gpos = sub(&sh, &cst(1, 16));
    let guard = ite(&sh_le0, &cst(0, 1), &extract(&lshr(&zsig, &gpos), 0, 0));
    let mask = sub(&shl(&cst(1, w2), &gpos), &cst(1, w2));
    let sticky = ite(&sh_le0, &cst(0, 1), &ne(&band(&zsig, &mask), &cst(0, w2)));

    let roundup = and(&guard, &or(&sticky, &extract(&kept0, 0, 0)));
    let kept = add(&kept0, &zext(&roundup, w2));

    let fw = eb + mb; // 7 for both fp8 formats
                      // subnormal: {exp,frac} encoding carries up to the smallest normal for free.
    let sub_res = bor(
        &concat(sign, &cst(0, fw)),
        &concat(sign, &extract(&kept, fw - 1, 0)),
    );

    // normal: a carry into bit mb+1 bumps the exponent.
    let carry = is1(&extract(&kept, mb + 1, mb + 1));
    let biased_n = ite(&carry, &add(&biased, &cst(1, 16)), &biased);
    let kept_n = ite(&carry, &lshr(&kept, &cst(1, 16)), &kept);
    let overflow = if top == TopBinade::OcpNanTop {
        // OFP8: exponent value 15 is finite except with an all-ones mantissa
        // (that slot is NaN => rounded magnitude >= 480 overflows).
        or(
            &sge(&biased_n, &cst(max_expf + 1, 16)),
            &and(
                &eq(&biased_n, &cst(max_expf, 16)),
                &eq(&extract(&kept_n, mb - 1, 0), &cst((1u128 << mb) - 1, mb)),
            ),
        )
    } else if top == TopBinade::AllFinite {
        // Every exponent value is finite; only going past the top overflows.
        sge(&biased_n, &cst(max_expf + 1, 16))
    } else {
        sge(&biased_n, &cst(max_expf, 16))
    };
    let packed = concat(
        sign,
        &concat(&extract(&biased_n, eb - 1, 0), &extract(&kept_n, mb - 1, 0)),
    );
    let norm_res = ite(&overflow, ovf_res, &packed);

    let zero = concat(sign, &cst(0, fw));
    ite(
        &eq(sig, &cst(0, w)),
        &zero,
        &ite(&biased_le0, &sub_res, &norm_res),
    )
}

fn e5m2_to_f32(p: FpCompat) -> FpFn {
    let h = var("h", 8);
    let s = extract(&h, 7, 7);
    let e = extract(&h, 6, 2);
    let f = extract(&h, 1, 0);
    let e_top = eq(&e, &cst(0x1F, 5));
    let e_z = eq(&e, &cst(0, 5));
    let f_z = eq(&f, &cst(0, 2));
    // value = sig3 * 2^e0: normal (4+f)*2^(e-17), subnormal f*2^-16.
    // Zero-extend the significand to 48 bits (value-preserving) — normround
    // internals need sig wide enough for its f32-field extracts and 16-bit
    // shift constants; 48 matches the established mul-product call sites.
    let sig3 = ite(&e_z, &concat(&cst(0, 1), &f), &concat(&cst(1, 1), &f));
    let sig24 = zext(&sig3, 48);
    let e0 = ite(
        &e_z,
        &cst((1u128 << 16) - 16, 16),
        &sub(&zext(&e, 16), &cst(17, 16)),
    );
    let widened = normround(&s, &sig24, &e0); // exact: 3-bit sig, no rounding
    let zero32 = concat(&s, &cst(0, 31));
    let inf32 = concat(&s, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let body = ite(
        &and(&e_top, &bnot(&f_z)),
        &cst(nan32(p), 32),
        &ite(
            &e_top,
            &inf32,
            &ite(&eq(&sig3, &cst(0, 3)), &zero32, &widened),
        ),
    );
    FpFn::new("arch_e5m2_to_f32", &[("h", 8)], 32, body)
}

fn e4m3_to_f32(p: FpCompat) -> FpFn {
    let h = var("h", 8);
    let s = extract(&h, 7, 7);
    let e = extract(&h, 6, 3);
    let f = extract(&h, 2, 0);
    let is_nan8 = eq(&extract(&h, 6, 0), &cst(0x7F, 7)); // the ONLY NaN (OFP8)
    let e_z = eq(&e, &cst(0, 4));
    // No infinity arm: exp=15 with mantissa <7 are the normals 256..448.
    // value = sig4 * 2^e0: normal (8+f)*2^(e-10), subnormal f*2^-9.
    // Zero-extend the significand to 48 bits (value-preserving) — normround
    // internals need sig wide enough for its f32-field extracts and 16-bit
    // shift constants; 48 matches the established mul-product call sites.
    let sig4 = ite(&e_z, &concat(&cst(0, 1), &f), &concat(&cst(1, 1), &f));
    let sig24 = zext(&sig4, 48);
    let e0 = ite(
        &e_z,
        &cst((1u128 << 16) - 9, 16),
        &sub(&zext(&e, 16), &cst(10, 16)),
    );
    let widened = normround(&s, &sig24, &e0); // exact: 4-bit sig, no rounding
    let zero32 = concat(&s, &cst(0, 31));
    let body = ite(
        &is_nan8,
        &cst(nan32(p), 32),
        &ite(&eq(&sig4, &cst(0, 4)), &zero32, &widened),
    );
    FpFn::new("arch_e4m3_to_f32", &[("h", 8)], 32, body)
}

fn f32_to_e5m2(p: FpCompat) -> FpFn {
    let x = var("x", 32);
    let d = decode(&x);
    let s = &d.sign;
    let inf8 = concat(s, &cst(0x7C, 7)); // ±inf: S.11111.00
    let max8 = concat(s, &cst(0x7B, 7)); // ±max finite 57344
    let (inf_res, ovf_res) = match p {
        FpCompat::Riscv => (inf8.clone(), inf8.clone()),
        FpCompat::Cuda => (max8.clone(), max8.clone()),
    };
    let rounded = fp8_round(5, 2, TopBinade::Ieee, s, &d.mant, &d.eunb, &ovf_res);
    let zero = concat(s, &cst(0, 7));
    let body = ite(
        &d.is_nan,
        &cst(nan8_e5m2(p), 8),
        &ite(&d.is_inf, &inf_res, &ite(&d.is_zero, &zero, &rounded)),
    );
    FpFn::new("arch_f32_to_e5m2", &[("x", 32)], 8, body)
}

// ── OCP MX FP4 E2M1 (storage-only) ───────────────────────────────────────
// 1 sign + 2 exp (bias 1) + 1 mantissa. NO Inf, NO NaN — every one of the
// 16 encodings is a finite value, and the whole set is
// ±{0, 0.5, 1, 1.5, 2, 3, 4, 6}. Only widen/narrow are defined: no shipping
// ISA exposes scalar E2M1 arithmetic (PTX: e2m1 "must be used in a packed
// format"), so `Ty::is_float_arith` rejects operators at the type checker
// and no arch_e2m1_{add,mul,...} exists to dispatch to.

fn e2m1_to_f32() -> FpFn {
    let h = var("h", 4);
    let s = extract(&h, 3, 3);
    let e = extract(&h, 2, 1);
    let f = extract(&h, 0, 0);
    let e_z = eq(&e, &cst(0, 2));
    // value = sig2 * 2^e0, with sig2 = (e==0 ? 0f : 1f) as a 2-bit integer:
    //   normal    (e>=1): (2+f) * 2^(e-2)
    //   subnormal (e==0):     f * 2^-1
    // Verified against all eight magnitudes.
    let sig2 = ite(&e_z, &concat(&cst(0, 1), &f), &concat(&cst(1, 1), &f));
    // normround's internals extract f32 fields and use 16-bit shift
    // constants, so the significand must be widened the same way the fp8
    // widens do (48 bits, value-preserving).
    let sig48 = zext(&sig2, 48);
    let e0 = ite(
        &e_z,
        &cst((1u128 << 16) - 1, 16), // -1
        &sub(&zext(&e, 16), &cst(2, 16)),
    );
    let widened = normround(&s, &sig48, &e0); // exact: 2-bit significand
    let zero32 = concat(&s, &cst(0, 31));
    // No e_top arms: the top binade is finite, so there is nothing to
    // special-case. Only ±0 needs its own answer.
    let body = ite(&eq(&sig2, &cst(0, 2)), &zero32, &widened);
    FpFn::new("arch_e2m1_to_f32", &[("h", 4)], 32, body)
}

fn f32_to_e2m1(p: FpCompat) -> FpFn {
    let x = var("x", 32);
    let d = decode(&x);
    let s = &d.sign;
    let max4 = concat(s, &cst(0x7, 3)); // ±6.0, the largest finite
                                        // E2M1 has no NaN and no Inf to produce, so BOTH --fp-compat profiles
                                        // saturate. This is the one place the profiles cannot differ: the
                                        // encoding space simply has nowhere else to go. `p` is accepted for
                                        // signature symmetry with the fp8 narrows and to keep the call site
                                        // uniform.
    let _ = p;
    let rounded = fp8_round(2, 1, TopBinade::AllFinite, s, &d.mant, &d.eunb, &max4);
    let zero = concat(s, &cst(0, 3));
    let body = ite(
        &or(&d.is_nan, &d.is_inf),
        &max4,
        &ite(&d.is_zero, &zero, &rounded),
    );
    FpFn::new("arch_f32_to_e2m1", &[("x", 32)], 4, body)
}

// ── OCP MX FP6 E2M3 / E3M2 (storage-only) ────────────────────────────────
// Same shape as FP4 E2M1: all-finite (no Inf, no NaN), conversions only.
// Generic over the field split, since nothing here is format-specific once
// TopBinade::AllFinite exists.

fn fp6_to_f32(name: &str, eb: u32, mb: u32) -> FpFn {
    let w = 1 + eb + mb;
    let h = var("h", w);
    let s = extract(&h, w - 1, w - 1);
    let e = extract(&h, w - 2, mb);
    let f = extract(&h, mb - 1, 0);
    let e_z = eq(&e, &cst(0, eb));
    let bias: u128 = (1u128 << (eb - 1)) - 1;
    // value = sig * 2^e0 with sig = (e==0 ? 0f : 1f), an (mb+1)-bit integer:
    //   normal    (e>=1): (2^mb + f) * 2^(e - bias - mb)
    //   subnormal (e==0):          f * 2^(1 - bias - mb)
    let sig = ite(&e_z, &concat(&cst(0, 1), &f), &concat(&cst(1, 1), &f));
    let sig48 = zext(&sig, 48);
    let sub_e0 = (1u128 << 16) - (bias + mb as u128 - 1); // 1 - bias - mb
    let e0 = ite(
        &e_z,
        &cst(sub_e0, 16),
        &sub(&zext(&e, 16), &cst(bias + mb as u128, 16)),
    );
    let widened = normround(&s, &sig48, &e0); // exact: <=4-bit significand
    let zero32 = concat(&s, &cst(0, 31));
    let body = ite(&eq(&sig, &cst(0, mb + 1)), &zero32, &widened);
    FpFn::new(name, &[("h", w)], 32, body)
}

fn f32_to_fp6(name: &str, eb: u32, mb: u32) -> FpFn {
    let w = 1 + eb + mb;
    let x = var("x", 32);
    let d = decode(&x);
    let s = &d.sign;
    let maxmag = (1u128 << (w - 1)) - 1;
    let max6 = concat(s, &cst(maxmag, w - 1));
    // No NaN and no Inf exist, so both --fp-compat profiles saturate.
    let rounded = fp8_round(eb, mb, TopBinade::AllFinite, s, &d.mant, &d.eunb, &max6);
    let zero = concat(s, &cst(0, w - 1));
    let body = ite(
        &or(&d.is_nan, &d.is_inf),
        &max6,
        &ite(&d.is_zero, &zero, &rounded),
    );
    FpFn::new(name, &[("x", 32)], w, body)
}

// ── OCP MX E8M0 — the block SCALE type ───────────────────────────────────
// 8 bits of unsigned biased exponent (bias 127) denoting 2^(e-127). NO
// sign, NO mantissa, NO infinity, and NO ZERO: 0x00 is the MINIMUM SCALE
// 2^-127, not zero. 0xFF is NaN (at block level, a NaN block).
//
// E8M0 deliberately does NOT go through fp8_round / normround: those assume
// a sign bit and a mantissa field, and `mb = 0` underflows their extracts.
// It does not need them — E8M0 shares FP32's bias, so for codes 1..=254 the
// f32 bit pattern is exactly `e << 23`, and the reverse is just the f32
// exponent field. Both directions are pure bit surgery.

fn e8m0_to_f32(p: FpCompat) -> FpFn {
    let e = var("e", 8);
    let is_nan = eq(&e, &cst(0xFF, 8));
    let is_min = eq(&e, &cst(0, 8));
    // e in 1..=254: value 2^(e-127) == f32 with exponent field e, mant 0.
    let normal = concat(&cst(0, 1), &concat(&e, &cst(0, 23)));
    // e == 0: 2^-127, below f32's min normal (2^-126) but exactly
    // representable as the subnormal with mantissa bit 22 set.
    let min_scale = cst(0x0040_0000, 32);
    let body = ite(
        &is_nan,
        // Canonical NaN follows --fp-compat, like every other widen.
        // Hardcoding riscv here leaked 32'h7FC00000 into cuda builds.
        &cst(nan32(p), 32),
        &ite(&is_min, &min_scale, &normal),
    );
    FpFn::new("arch_e8m0_to_f32", &[("e", 8)], 32, body)
}

fn f32_to_e8m0() -> FpFn {
    let x = var("x", 32);
    let ef = extract(&x, 30, 23); // f32 exponent field — same bias as E8M0
    let is_special = eq(&ef, &cst(0xFF, 8)); // inf or NaN
    let is_sub = eq(&ef, &cst(0, 8)); // zero or subnormal
                                      // Clamping follows the MX reference (microxcaling): underflow to the
                                      // minimum scale 0x00, overflow/non-finite to NaN 0xFF. E8M0 cannot
                                      // represent zero, so a zero input becomes the minimum scale — the
                                      // closest thing the format has.
    let body = ite(&is_special, &cst(0xFF, 8), &ite(&is_sub, &cst(0, 8), &ef));
    FpFn::new("arch_f32_to_e8m0", &[("x", 32)], 8, body)
}

// ── NVFP4 UE4M3 — the NVIDIA block SCALE type ────────────────────────────
// PTX: a 7-bit unsigned float, MSB padded with zero, NaN limited to 0x7F.
//
// It is NOT FP8E4M3 — unsigned, and its sole NaN is 0x7F rather than E4M3's
// sign-agnostic all-magnitude-ones — but it IS numerically E4M3 restricted to
// sign 0: all 128 codes denote the same value as the E4M3 code with the same
// bits. So both directions reuse the already-proven E4M3 helpers instead of
// adding a second rounder, exactly as the block ops reuse the element ones.
//
// Two ways it differs from E8M0, both load-bearing downstream: it HAS a zero
// (0x00), and its value is NOT a power of two, so dividing by an NVFP4 scale
// is not exact.

fn ue4m3_to_f32() -> FpFn {
    let u = var("u", 8);
    // Mask the padding bit rather than trusting it: a stray high bit would
    // otherwise be read as an E4M3 SIGN and silently negate the scale.
    let mag = band(&u, &cst(0x7F, 8));
    let body = call("arch_e4m3_to_f32", &[mag], 32);
    FpFn::new("arch_ue4m3_to_f32", &[("u", 8)], 32, body)
}

fn f32_to_ue4m3() -> FpFn {
    let x = var("x", 32);
    // A scale is non-negative, so narrow the MAGNITUDE. This matches
    // arch_f32_to_e8m0, which reads the exponent field regardless of sign.
    // Clearing the sign first also guarantees the result's bit 7 is 0, i.e.
    // the padding bit the format requires.
    //
    // Written as "zero, then the low 31 bits" rather than an AND with
    // 0x7FFFFFFF: it says *clear the sign* structurally instead of via a
    // magic constant, and it keeps that constant out of the emitted helper
    // block, where it is indistinguishable from the cuda canonical-NaN
    // pattern that `fp_compat_build_profiles` greps for.
    let mag = concat(&cst(0, 1), &extract(&x, 30, 0));
    let body = call("arch_f32_to_e4m3", &[mag], 8);
    FpFn::new("arch_f32_to_ue4m3", &[("x", 32)], 8, body)
}

fn f32_to_e4m3(p: FpCompat) -> FpFn {
    let x = var("x", 32);
    let d = decode(&x);
    let s = &d.sign;
    let max8 = concat(s, &cst(0x7E, 7)); // ±max finite 448
    let nan8 = cst(nan8_e4m3(p), 8); // 0x7F, sign dropped (OFP8 canonical)
    let (inf_res, ovf_res) = match p {
        FpCompat::Riscv => (nan8.clone(), nan8.clone()),
        FpCompat::Cuda => (max8.clone(), max8.clone()),
    };
    let rounded = fp8_round(4, 3, TopBinade::OcpNanTop, s, &d.mant, &d.eunb, &ovf_res);
    let zero = concat(s, &cst(0, 7));
    let body = ite(
        &d.is_nan,
        &nan8,
        &ite(&d.is_inf, &inf_res, &ite(&d.is_zero, &zero, &rounded)),
    );
    FpFn::new("arch_f32_to_e4m3", &[("x", 32)], 8, body)
}

// fp8 arithmetic/compares = widen -> f32 op -> narrow, like bf16. Binary ops
// are single-rounding-correct candidates (E4M3 add/sub are exact in f32 and
// both muls fit 24 bits; exhaustively machine-checked in fp_smt_proof); fma
// is fused f32-accumulate like arch_fma_bf16, NOT correctly-rounded fp8 (the
// second rounding is characterized exhaustively — see tests/fp_v1).
fn fp8_bin(name: &str, widen: &str, narrow: &str, f32fn: &str) -> FpFn {
    let a = var("a", 8);
    let b = var("b", 8);
    let wa = call(widen, &[a.clone()], 32);
    let wb = call(widen, &[b.clone()], 32);
    let r = call(f32fn, &[wa, wb], 32);
    let body = call(narrow, &[r], 8);
    FpFn::new(name, &[("a", 8), ("b", 8)], 8, body)
}
fn fp8_fma(name: &str, widen: &str, narrow: &str) -> FpFn {
    let a = var("a", 8);
    let b = var("b", 8);
    let c = var("c", 8);
    let wa = call(widen, &[a.clone()], 32);
    let wb = call(widen, &[b.clone()], 32);
    let wc = call(widen, &[c.clone()], 32);
    let r = call("arch_fma_f32", &[wa, wb, wc], 32);
    let body = call(narrow, &[r], 8);
    FpFn::new(name, &[("a", 8), ("b", 8), ("c", 8)], 8, body)
}
fn fp8_cmp(name: &str, widen: &str, f32fn: &str) -> FpFn {
    let a = var("a", 8);
    let b = var("b", 8);
    let wa = call(widen, &[a.clone()], 32);
    let wb = call(widen, &[b.clone()], 32);
    let body = call(f32fn, &[wa, wb], 1);
    FpFn::new(name, &[("a", 8), ("b", 8)], 1, body)
}

// Exact-wide alignment widths: large enough to hold the exact aligned
// magnitude so no sticky/borrow logic is needed (the rounder re-derives
// guard/round/sticky). add: 23 + max-exponent-spread(253) + carry. fma: 48-bit
// product + max product/addend spread. Correctness is by construction; the §8.2
// differential harness is the oracle.
const ADD_G: u32 = 30; // bounded-adder guard bits (field = 24 + ADD_G)
const FMA_W: u32 = 470;
// FMA bounded-adder guard bits (field = 48-bit product + FMA_G). Catastrophic
// cancellation in a*b+c forces the product/addend LSB gap to <= 47, so FMA_G>=47
// guarantees no significant bit is ever folded into sticky; 48 gives one bit of
// margin. Field 96 vs the exact-wide 470 -> ~5x narrower datapath.
const FMA_G: u32 = 48;

fn f32_add_core(name: &str, flip_b_sign: bool, p: FpCompat) -> FpFn {
    let a = var("a", 32);
    let b0 = var("b", 32);
    let b = if flip_b_sign {
        concat(&bnot(&extract(&b0, 31, 31)), &extract(&b0, 30, 0))
    } else {
        b0.clone()
    };
    let da = decode(&a);
    let db = decode(&b);
    let n = cst(nan32(p), 32);

    // order by exponent: hi has the larger (>=) eunb
    let hi_is_a = sge(&da.eunb, &db.eunb);
    let pick = |fa: &Bv, fb: &Bv| ite(&hi_is_a, fa, fb);
    let mant_hi = pick(&da.mant, &db.mant);
    let mant_lo = pick(&db.mant, &da.mant);
    let eunb_hi = pick(&da.eunb, &db.eunb);
    let eunb_lo = pick(&db.eunb, &da.eunb);
    let sign_hi = pick(&da.sign, &db.sign);
    let sign_lo = pick(&db.sign, &da.sign);

    // Bounded alignment: keep G guard bits below the larger significand's LSB
    // and fold everything past that into one sticky bit. Catastrophic
    // cancellation needs the exponents within ~1, where no bits are dropped
    // (exact); otherwise the larger operand dominates and the sticky carries the
    // rest. The sticky is appended as the LOW bit of each aligned operand, so the
    // magnitude compare and the subtraction handle the borrow automatically (and
    // resolve the HI==LO tie). Far narrower than exact-wide -> compact SV and a
    // solver-tractable miter.
    let diff = sub(&eunb_hi, &eunb_lo); // >= 0
    let fw = 24 + ADD_G; // aligned-field width
    let hi_field = shl(&zext(&mant_hi, fw), &cst(ADD_G as u128, 16));
    let lo_ext = shl(&zext(&mant_lo, fw), &cst(ADD_G as u128, 16));
    let lo_field = lshr(&lo_ext, &diff);
    let mask = sub(&shl(&cst(1, fw), &diff), &cst(1, fw)); // (1<<diff)-1
    let sticky = ne(&band(&lo_ext, &mask), &cst(0, fw));

    let hi_e = concat(&hi_field, &cst(0, 1)); // fw+1
    let lo_e = concat(&lo_field, &sticky); // fw+1
    let same_sign = eq(&sign_hi, &sign_lo);
    let ge = uge(&hi_e, &lo_e);
    let raw = ite(&ge, &sub(&hi_e, &lo_e), &sub(&lo_e, &hi_e)); // fw+1
    let mw = fw + 2; // add-carry headroom
    let mag = ite(
        &same_sign,
        &add(&zext(&hi_e, mw), &zext(&lo_e, mw)),
        &zext(&raw, mw),
    );
    let res_sign = ite(&same_sign, &sign_hi, &ite(&ge, &sign_hi, &sign_lo));
    let e0 = sub(&eunb_hi, &cst((ADD_G + 1) as u128, 16)); // LSB exponent of mag
    let rounded = normround(&res_sign, &mag, &e0);
    // exact cancellation (opposite signs, equal magnitude incl. sticky) -> +0
    let cancel = and(&bnot(&same_sign), &eq(&raw, &cst(0, fw + 1)));
    let finite = ite(&cancel, &cst(0, 32), &rounded);

    // specials
    let both_inf = and(&da.is_inf, &db.is_inf);
    let inf_a = concat(&da.sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let inf_b = concat(&db.sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let body = ite(
        &or(&da.is_nan, &db.is_nan),
        &n,
        &ite(
            &both_inf,
            &ite(&eq(&da.sign, &db.sign), &inf_a, &n), // inf + (-inf) = NaN
            &ite(&da.is_inf, &inf_a, &ite(&db.is_inf, &inf_b, &finite)),
        ),
    );
    FpFn::new(name, &[("a", 32), ("b", 32)], 32, body)
}

/// Checked `f32` add: the ordinary result **plus** the four IEEE flags, packed
/// into a 36-bit value `{inexact[35], invalid[34], underflow[33], overflow[32],
/// value[31:0]}`. Backs the surface `checked(a + b) : FpResult<FP32>`. The 32-bit
/// value is bit-identical to `arch_f32_add` (shares the same body); flags are
/// gated to the finite path (a propagated `Inf`/`NaN` input is not an overflow),
/// and `invalid` is the `∞ + (−∞)` case.
fn f32_add_checked(p: FpCompat) -> FpFn {
    let a = var("a", 32);
    let b = var("b", 32);
    let da = decode(&a);
    let db = decode(&b);
    let n = cst(nan32(p), 32);
    let hi_is_a = sge(&da.eunb, &db.eunb);
    let pick = |fa: &Bv, fb: &Bv| ite(&hi_is_a, fa, fb);
    let mant_hi = pick(&da.mant, &db.mant);
    let mant_lo = pick(&db.mant, &da.mant);
    let eunb_hi = pick(&da.eunb, &db.eunb);
    let eunb_lo = pick(&db.eunb, &da.eunb);
    let sign_hi = pick(&da.sign, &db.sign);
    let sign_lo = pick(&db.sign, &da.sign);
    let diff = sub(&eunb_hi, &eunb_lo);
    let fw = 24 + ADD_G;
    let hi_field = shl(&zext(&mant_hi, fw), &cst(ADD_G as u128, 16));
    let lo_ext = shl(&zext(&mant_lo, fw), &cst(ADD_G as u128, 16));
    let lo_field = lshr(&lo_ext, &diff);
    let mask = sub(&shl(&cst(1, fw), &diff), &cst(1, fw));
    let sticky = ne(&band(&lo_ext, &mask), &cst(0, fw));
    let hi_e = concat(&hi_field, &cst(0, 1));
    let lo_e = concat(&lo_field, &sticky);
    let same_sign = eq(&sign_hi, &sign_lo);
    let ge = uge(&hi_e, &lo_e);
    let raw = ite(&ge, &sub(&hi_e, &lo_e), &sub(&lo_e, &hi_e));
    let mw = fw + 2;
    let mag = ite(
        &same_sign,
        &add(&zext(&hi_e, mw), &zext(&lo_e, mw)),
        &zext(&raw, mw),
    );
    let res_sign = ite(&same_sign, &sign_hi, &ite(&ge, &sign_hi, &sign_lo));
    let e0 = sub(&eunb_hi, &cst((ADD_G + 1) as u128, 16));
    let (rounded, ovf, unf, inx) = normround_flags(&res_sign, &mag, &e0);
    let cancel = and(&bnot(&same_sign), &eq(&raw, &cst(0, fw + 1)));
    let finite = ite(&cancel, &cst(0, 32), &rounded);
    let both_inf = and(&da.is_inf, &db.is_inf);
    let inf_a = concat(&da.sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let inf_b = concat(&db.sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let value = ite(
        &or(&da.is_nan, &db.is_nan),
        &n,
        &ite(
            &both_inf,
            &ite(&eq(&da.sign, &db.sign), &inf_a, &n),
            &ite(&da.is_inf, &inf_a, &ite(&db.is_inf, &inf_b, &finite)),
        ),
    );
    // flags fire only on the genuine finite (non-cancelling) datapath
    let is_special = or(&or(&da.is_nan, &db.is_nan), &or(&da.is_inf, &db.is_inf));
    let finite_path = and(&bnot(&is_special), &bnot(&cancel));
    let f_overflow = and(&finite_path, &ovf);
    let f_underflow = and(&finite_path, &unf);
    let f_inexact = and(&finite_path, &inx);
    let f_invalid = and(&both_inf, &ne(&da.sign, &db.sign)); // ∞ + (−∞)
    let packed = concat(
        &f_inexact,
        &concat(
            &f_invalid,
            &concat(&f_underflow, &concat(&f_overflow, &value)),
        ),
    );
    FpFn::new("arch_f32_add_checked", &[("a", 32), ("b", 32)], 36, packed)
}

fn fma_f32(p: FpCompat) -> FpFn {
    let a = var("a", 32);
    let b = var("b", 32);
    let c = var("c", 32);
    let da = decode(&a);
    let db = decode(&b);
    let dc = decode(&c);
    let n = cst(nan32(p), 32);
    let sp = bxor(&da.sign, &db.sign); // product sign
    let prod_inf = or(&da.is_inf, &db.is_inf);
    let prod_zero = or(&da.is_zero, &db.is_zero);

    // product significand (48-bit) and exponent
    let mp = mul(&zext(&da.mant, 48), &zext(&db.mant, 48));
    let ep = add(&da.eunb, &db.eunb);

    // ── Bounded sticky-fold alignment (mirrors f32_add_core) ──────────────
    // Anchor at the operand with the higher LSB-exponent and shift the lower
    // one down, folding everything past the FMA_G guard region into one sticky
    // bit (appended as the low bit so the subtraction borrows correctly). The
    // product is 48-bit and c is 24-bit; FMA_G keeps the full product through
    // catastrophic cancellation, which can only occur when c is the higher
    // operand (an LSB gap of <= 47).
    let c_mant48 = zext(&dc.mant, 48);
    let hi_is_p = sge(&ep, &dc.eunb);
    let psel = |fp_: &Bv, fc: &Bv| ite(&hi_is_p, fp_, fc);
    let sig_hi = psel(&mp, &c_mant48);
    let sig_lo = psel(&c_mant48, &mp);
    let e_hi = psel(&ep, &dc.eunb);
    let sign_hi = psel(&sp, &dc.sign);
    let sign_lo = psel(&dc.sign, &sp);

    let diff = sub(&e_hi, &psel(&dc.eunb, &ep)); // e_hi - e_lo >= 0
    let fw = 48 + FMA_G; // aligned-field width
    let hi_field = shl(&zext(&sig_hi, fw), &cst(FMA_G as u128, 16));
    let lo_ext = shl(&zext(&sig_lo, fw), &cst(FMA_G as u128, 16));
    let lo_field = lshr(&lo_ext, &diff);
    let mask = sub(&shl(&cst(1, fw), &diff), &cst(1, fw)); // (1<<diff)-1
    let sticky = ne(&band(&lo_ext, &mask), &cst(0, fw));

    let hi_e = concat(&hi_field, &cst(0, 1)); // fw+1
    let lo_e = concat(&lo_field, &sticky); // fw+1
    let same = eq(&sign_hi, &sign_lo);
    let ge = uge(&hi_e, &lo_e);
    let raw = ite(&ge, &sub(&hi_e, &lo_e), &sub(&lo_e, &hi_e)); // fw+1
    let mw = fw + 2; // add-carry headroom
    let mag = ite(
        &same,
        &add(&zext(&hi_e, mw), &zext(&lo_e, mw)),
        &zext(&raw, mw),
    );
    let res_sign = ite(&same, &sign_hi, &ite(&ge, &sign_hi, &sign_lo));
    let e0 = sub(&e_hi, &cst((FMA_G + 1) as u128, 16)); // LSB exponent of mag
                                                        // exact cancellation (opposite signs, equal magnitude incl. sticky) -> +0
    let cancel = and(&bnot(&same), &eq(&raw, &cst(0, fw + 1)));
    let general = ite(&cancel, &cst(0, 32), &normround(&res_sign, &mag, &e0));

    // product==0 (finite): result = signed-zero(sp) + c
    let prod_zero_res = call("arch_f32_add", &[concat(&sp, &cst(0, 31)), c.clone()], 32);
    // c==0 (finite, product nonzero): round the product alone
    let prod_only = normround(&sp, &mp, &ep);

    let inf_p = concat(&sp, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let inf_c = concat(&dc.sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let zero_times_inf = or(&and(&da.is_inf, &db.is_zero), &and(&da.is_zero, &db.is_inf));

    let body = ite(
        &or(&or(&da.is_nan, &db.is_nan), &dc.is_nan),
        &n,
        &ite(
            &zero_times_inf,
            &n,
            &ite(
                &prod_inf,
                &ite(&and(&dc.is_inf, &ne(&dc.sign, &sp)), &n, &inf_p), // inf - inf
                &ite(
                    &dc.is_inf,
                    &inf_c,
                    &ite(
                        &prod_zero,
                        &prod_zero_res,
                        &ite(&dc.is_zero, &prod_only, &general),
                    ),
                ),
            ),
        ),
    );
    FpFn::new("arch_fma_f32", &[("a", 32), ("b", 32), ("c", 32)], 32, body)
}

/// The pre-sticky-fold **exact-wide (FMA_W=470)** FMA, kept only as the proof
/// reference for the new-vs-old equivalence miter (`equiv_proof("fma_equiv")`).
/// Identical to `fma_f32` except the `general` path aligns into a 470-bit field
/// (no sticky), so the shared `mul`/specials cancel and z3 discharges the miter
/// without a multiplier-equivalence — transferring the existing machine-checked
/// correctness of this exact-wide FMA to the bounded sticky-fold one.
pub fn fma_f32_ref(p: FpCompat) -> FpFn {
    let a = var("a", 32);
    let b = var("b", 32);
    let c = var("c", 32);
    let da = decode(&a);
    let db = decode(&b);
    let dc = decode(&c);
    let n = cst(nan32(p), 32);
    let sp = bxor(&da.sign, &db.sign);
    let prod_inf = or(&da.is_inf, &db.is_inf);
    let prod_zero = or(&da.is_zero, &db.is_zero);
    let mp = mul(&zext(&da.mant, 48), &zext(&db.mant, 48));
    let ep = add(&da.eunb, &db.eunb);
    let p_ge_c = sge(&ep, &dc.eunb);
    let e_lo = ite(&p_ge_c, &dc.eunb, &ep);
    let pt = ite(
        &p_ge_c,
        &shl(&zext(&mp, FMA_W), &sub(&ep, &dc.eunb)),
        &zext(&mp, FMA_W),
    );
    let ct = ite(
        &p_ge_c,
        &zext(&dc.mant, FMA_W),
        &shl(&zext(&dc.mant, FMA_W), &sub(&dc.eunb, &ep)),
    );
    let same = eq(&sp, &dc.sign);
    let pt_gt = ugt(&pt, &ct);
    let mag = ite(
        &same,
        &add(&pt, &ct),
        &ite(&pt_gt, &sub(&pt, &ct), &sub(&ct, &pt)),
    );
    let res_sign = ite(&same, &sp, &ite(&pt_gt, &sp, &dc.sign));
    let cancel = and(&bnot(&same), &eq(&pt, &ct));
    let general = ite(&cancel, &cst(0, 32), &normround(&res_sign, &mag, &e_lo));
    let prod_zero_res = call("arch_f32_add", &[concat(&sp, &cst(0, 31)), c.clone()], 32);
    let prod_only = normround(&sp, &zext(&mp, FMA_W), &ep);
    let inf_p = concat(&sp, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let inf_c = concat(&dc.sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let zero_times_inf = or(&and(&da.is_inf, &db.is_zero), &and(&da.is_zero, &db.is_inf));
    let body = ite(
        &or(&or(&da.is_nan, &db.is_nan), &dc.is_nan),
        &n,
        &ite(
            &zero_times_inf,
            &n,
            &ite(
                &prod_inf,
                &ite(&and(&dc.is_inf, &ne(&dc.sign, &sp)), &n, &inf_p),
                &ite(
                    &dc.is_inf,
                    &inf_c,
                    &ite(
                        &prod_zero,
                        &prod_zero_res,
                        &ite(&dc.is_zero, &prod_only, &general),
                    ),
                ),
            ),
        ),
    );
    FpFn::new(
        "arch_fma_f32_ref",
        &[("a", 32), ("b", 32), ("c", 32)],
        32,
        body,
    )
}

/// FMA body parameterized on a **free** 48-bit product `mp` (instead of
/// `mul(mant_a, mant_b)`), for the multiply-abstracted equivalence proof. With
/// `mp` a free input there is no multiplier in the query at all, so the
/// new-vs-ref miter is a pure shift/add/round equivalence — solver-tractable
/// like the f32 add. `sticky=true` is the bounded sticky-fold general path;
/// `false` is the exact-wide (470-bit) reference. Proving them equal for *all*
/// `mp` (a superset of real products) is sufficient and avoids any
/// multiplier-equivalence. Used only by `equiv_proof("fma_equiv_abs")`.
pub fn fma_param(sticky: bool, p: FpCompat) -> FpFn {
    let a = var("a", 32);
    let b = var("b", 32);
    let c = var("c", 32);
    let mp = var("mp", 48); // free product (abstracted)
    let da = decode(&a);
    let db = decode(&b);
    let dc = decode(&c);
    let n = cst(nan32(p), 32);
    let sp = bxor(&da.sign, &db.sign);
    let prod_inf = or(&da.is_inf, &db.is_inf);
    let prod_zero = or(&da.is_zero, &db.is_zero);
    let ep = add(&da.eunb, &db.eunb);

    let (general, prod_only) = if sticky {
        let c_mant48 = zext(&dc.mant, 48);
        let hi_is_p = sge(&ep, &dc.eunb);
        let psel = |fp_: &Bv, fc: &Bv| ite(&hi_is_p, fp_, fc);
        let sig_hi = psel(&mp, &c_mant48);
        let sig_lo = psel(&c_mant48, &mp);
        let e_hi = psel(&ep, &dc.eunb);
        let sign_hi = psel(&sp, &dc.sign);
        let sign_lo = psel(&dc.sign, &sp);
        let diff = sub(&e_hi, &psel(&dc.eunb, &ep));
        let fw = 48 + FMA_G;
        let hi_field = shl(&zext(&sig_hi, fw), &cst(FMA_G as u128, 16));
        let lo_ext = shl(&zext(&sig_lo, fw), &cst(FMA_G as u128, 16));
        let lo_field = lshr(&lo_ext, &diff);
        let mask = sub(&shl(&cst(1, fw), &diff), &cst(1, fw));
        let sticky_b = ne(&band(&lo_ext, &mask), &cst(0, fw));
        let hi_e = concat(&hi_field, &cst(0, 1));
        let lo_e = concat(&lo_field, &sticky_b);
        let same = eq(&sign_hi, &sign_lo);
        let ge = uge(&hi_e, &lo_e);
        let raw = ite(&ge, &sub(&hi_e, &lo_e), &sub(&lo_e, &hi_e));
        let mw = fw + 2;
        let mag = ite(
            &same,
            &add(&zext(&hi_e, mw), &zext(&lo_e, mw)),
            &zext(&raw, mw),
        );
        let res_sign = ite(&same, &sign_hi, &ite(&ge, &sign_hi, &sign_lo));
        let e0 = sub(&e_hi, &cst((FMA_G + 1) as u128, 16));
        let cancel = and(&bnot(&same), &eq(&raw, &cst(0, fw + 1)));
        let gen = ite(&cancel, &cst(0, 32), &normround(&res_sign, &mag, &e0));
        (gen, normround(&sp, &mp, &ep))
    } else {
        let p_ge_c = sge(&ep, &dc.eunb);
        let e_lo = ite(&p_ge_c, &dc.eunb, &ep);
        let pt = ite(
            &p_ge_c,
            &shl(&zext(&mp, FMA_W), &sub(&ep, &dc.eunb)),
            &zext(&mp, FMA_W),
        );
        let ct = ite(
            &p_ge_c,
            &zext(&dc.mant, FMA_W),
            &shl(&zext(&dc.mant, FMA_W), &sub(&dc.eunb, &ep)),
        );
        let same = eq(&sp, &dc.sign);
        let pt_gt = ugt(&pt, &ct);
        let mag = ite(
            &same,
            &add(&pt, &ct),
            &ite(&pt_gt, &sub(&pt, &ct), &sub(&ct, &pt)),
        );
        let res_sign = ite(&same, &sp, &ite(&pt_gt, &sp, &dc.sign));
        let cancel = and(&bnot(&same), &eq(&pt, &ct));
        let gen = ite(&cancel, &cst(0, 32), &normround(&res_sign, &mag, &e_lo));
        (gen, normround(&sp, &zext(&mp, FMA_W), &ep))
    };

    let prod_zero_res = call("arch_f32_add", &[concat(&sp, &cst(0, 31)), c.clone()], 32);
    let inf_p = concat(&sp, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let inf_c = concat(&dc.sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let zero_times_inf = or(&and(&da.is_inf, &db.is_zero), &and(&da.is_zero, &db.is_inf));
    let body = ite(
        &or(&or(&da.is_nan, &db.is_nan), &dc.is_nan),
        &n,
        &ite(
            &zero_times_inf,
            &n,
            &ite(
                &prod_inf,
                &ite(&and(&dc.is_inf, &ne(&dc.sign, &sp)), &n, &inf_p),
                &ite(
                    &dc.is_inf,
                    &inf_c,
                    &ite(
                        &prod_zero,
                        &prod_zero_res,
                        &ite(&dc.is_zero, &prod_only, &general),
                    ),
                ),
            ),
        ),
    );
    let nm = if sticky {
        "arch_fma_param_new"
    } else {
        "arch_fma_param_ref"
    };
    FpFn::new(nm, &[("a", 32), ("b", 32), ("c", 32), ("mp", 48)], 32, body)
}

// ── int <-> float ───────────────────────────────────────────────────────────

fn i64_to_f32() -> FpFn {
    let v = var("v", 64);
    let sign = extract(&v, 63, 63);
    let mag = ite(&is1(&sign), &neg(&v), &v);
    let body = ite(
        &eq(&v, &cst(0, 64)),
        &cst(0, 32),
        &normround(&sign, &mag, &cst(0, 16)),
    );
    FpFn::new("arch_i64_to_f32", &[("v", 64)], 32, body)
}
fn u64_to_f32() -> FpFn {
    let v = var("v", 64);
    let body = ite(
        &eq(&v, &cst(0, 64)),
        &cst(0, 32),
        &normround(&cst(0, 1), &v, &cst(0, 16)),
    );
    FpFn::new("arch_u64_to_f32", &[("v", 64)], 32, body)
}

// float -> int magnitude (128-bit, toward zero), shared by sint/uint.
fn f2i_mag(d: &Dec) -> Bv {
    let m = zext(&d.mant, 128);
    let e = d.eunb.clone();
    let big = sge(&e, &cst(64, 16));
    let nonneg = sge(&e, &cst(0, 16));
    let sh = neg(&e);
    ite(
        &big,
        &bnot(&cst(0, 128)),
        &ite(&nonneg, &shl(&m, &e), &lshr(&m, &sh)),
    )
}

fn f32_to_sint(p: FpCompat) -> FpFn {
    let x = var("x", 32);
    let n = var("n", 32);
    let d = decode(&x);
    let n128 = zext(&n, 128);
    let one = cst(1, 128);
    let lim_pos = sub(&shl(&one, &sub(&n128, &cst(1, 128))), &one); // 2^(n-1)-1
    let lim_neg = shl(&one, &sub(&n128, &cst(1, 128))); // 2^(n-1)
    let mag = f2i_mag(&d);
    let lo64 = |b: &Bv| extract(b, 63, 0);
    let neg_lim_neg = lo64(&neg(&lim_neg)); // INT_MIN (two's complement, 64-bit)
    let inf_res = ite(&is1(&d.sign), &neg_lim_neg, &lo64(&lim_pos));
    let pos_sat = ite(&ugt(&mag, &lim_pos), &lo64(&lim_pos), &lo64(&mag));
    let neg_sat = ite(&ugt(&mag, &lim_neg), &neg_lim_neg, &lo64(&neg(&mag)));
    let finite = ite(&bnot(&d.sign), &pos_sat, &neg_sat);
    let nan_res = match p {
        FpCompat::Riscv => lo64(&lim_pos),
        FpCompat::Cuda => cst(0, 64),
    };
    let body = ite(
        &d.is_nan,
        &nan_res,
        &ite(&d.is_zero, &cst(0, 64), &ite(&d.is_inf, &inf_res, &finite)),
    );
    FpFn::new("arch_f32_to_sint", &[("x", 32), ("n", 32)], 64, body)
}

fn f32_to_uint(p: FpCompat) -> FpFn {
    let x = var("x", 32);
    let n = var("n", 32);
    let d = decode(&x);
    let n128 = zext(&n, 128);
    let one = cst(1, 128);
    let lim = sub(&shl(&one, &n128), &one); // 2^n - 1
    let mag = f2i_mag(&d);
    let lo64 = |b: &Bv| extract(b, 63, 0);
    let sat = ite(&ugt(&mag, &lim), &lo64(&lim), &lo64(&mag));
    let nan_res = match p {
        FpCompat::Riscv => lo64(&lim),
        FpCompat::Cuda => cst(0, 64),
    };
    let body = ite(
        &d.is_nan,
        &nan_res,
        &ite(
            &d.is_zero,
            &cst(0, 64),
            &ite(
                &is1(&d.sign),
                &cst(0, 64),
                &ite(&d.is_inf, &lo64(&lim), &sat),
            ),
        ),
    );
    FpFn::new("arch_f32_to_uint", &[("x", 32), ("n", 32)], 64, body)
}

// ── bf16 arithmetic = widen -> f32 op -> narrow (calls into the f32 fns) ─────
//
// `arch_bf16_{mul,add,sub}` are correctly-rounded bf16: each is machine-proved
// `unsat` vs `fp.{mul,add,sub}` on `(_ FloatingPoint 8 8)` (z3), exhaustively
// over all 2^32 inputs. (For `mul` the f32 intermediate is *exact* — two 8-bit
// significands multiply to <=16 bits, which fit f32's 24-bit field with no
// rounding — so the narrow is the only rounding. For add/sub the f32 step does
// round, but the exhaustive miter still closes.)
//
// `arch_fma_bf16` is NOT correctly-rounded bf16 — it is fused f32-accumulate:
// widen -> one correctly-rounded f32 fma (the exact `a*b+c`) -> round f32->bf16.
// That final narrow is a SECOND rounding and double rounding here is NOT
// innocuous (the "`p_f32 >= 2*p_bf16 + 2` margin" reasoning is a known fallacy
// for round-to-nearest). `arch_fma_bf16` differs from the correctly-rounded
// `a*b+c` on ~0.37% of finite inputs, always by 1 ULP (witness a=0x2a20,
// b=0x51a6, c=0x9359 -> arch 0x3c50 vs correctly-rounded 0x3c4f). Its `fp.fma`
// miter on (8,8) therefore returns a GENUINE `sat` (a real counterexample), not
// a z3 4.8.12 soundness gap as previously assumed. This f32-accumulate behavior
// is intentional (the NVIDIA Tensor Core / TPU convention, strictly more
// accurate than a non-fused bf16 fma) and machine-characterized as
// `archBf16Fma_eq_narrow_roundNE` in proofs/lean_fp_equiv (PR #627).

fn bf16_bin(name: &str, f32fn: &str) -> FpFn {
    let a = var("a", 16);
    let b = var("b", 16);
    let wa = call("arch_bf16_to_f32", &[a.clone()], 32);
    let wb = call("arch_bf16_to_f32", &[b.clone()], 32);
    let r = call(f32fn, &[wa, wb], 32);
    let body = call("arch_f32_to_bf16", &[r], 16);
    FpFn::new(name, &[("a", 16), ("b", 16)], 16, body)
}
fn bf16_fma() -> FpFn {
    let a = var("a", 16);
    let b = var("b", 16);
    let c = var("c", 16);
    let wa = call("arch_bf16_to_f32", &[a.clone()], 32);
    let wb = call("arch_bf16_to_f32", &[b.clone()], 32);
    let wc = call("arch_bf16_to_f32", &[c.clone()], 32);
    let r = call("arch_fma_f32", &[wa, wb, wc], 32);
    let body = call("arch_f32_to_bf16", &[r], 16);
    FpFn::new(
        "arch_fma_bf16",
        &[("a", 16), ("b", 16), ("c", 16)],
        16,
        body,
    )
}
fn bf16_cmp(name: &str, f32fn: &str) -> FpFn {
    let a = var("a", 16);
    let b = var("b", 16);
    let wa = call("arch_bf16_to_f32", &[a.clone()], 32);
    let wb = call("arch_bf16_to_f32", &[b.clone()], 32);
    let body = call(f32fn, &[wa, wb], 1);
    FpFn::new(name, &[("a", 16), ("b", 16)], 1, body)
}

/// All FP helper functions for the given profile, single source for SV + SMT.
pub fn fp_functions(p: FpCompat) -> Vec<FpFn> {
    let mut v = vec![
        f32_canon(p),
        f32_mul(p),
        f32_add_core("arch_f32_add", false, p),
        f32_add_core("arch_f32_sub", true, p),
        f32_add_checked(p),
        fma_f32(p),
        bf16_to_f32(p),
        f32_to_bf16(p),
        i64_to_f32(),
        u64_to_f32(),
        f32_to_sint(p),
        f32_to_uint(p),
    ];
    v.extend(f32_compares());
    v.push(bf16_bin("arch_bf16_add", "arch_f32_add"));
    v.push(bf16_bin("arch_bf16_sub", "arch_f32_sub"));
    v.push(bf16_bin("arch_bf16_mul", "arch_f32_mul"));
    v.push(bf16_fma());
    v.push(bf16_cmp("arch_bf16_eq", "arch_f32_eq"));
    v.push(bf16_cmp("arch_bf16_ne", "arch_f32_ne"));
    v.push(bf16_cmp("arch_bf16_lt", "arch_f32_lt"));
    v.push(bf16_cmp("arch_bf16_gt", "arch_f32_gt"));
    v.push(bf16_cmp("arch_bf16_le", "arch_f32_le"));
    v.push(bf16_cmp("arch_bf16_ge", "arch_f32_ge"));
    v.push(e5m2_to_f32(p));
    v.push(f32_to_e5m2(p));
    v.push(e4m3_to_f32(p));
    v.push(f32_to_e4m3(p));
    // Storage-only: widen/narrow only, no arithmetic wrappers.
    v.push(e2m1_to_f32());
    v.push(f32_to_e2m1(p));
    v.push(fp6_to_f32("arch_e2m3_to_f32", 2, 3));
    v.push(f32_to_fp6("arch_f32_to_e2m3", 2, 3));
    v.push(e8m0_to_f32(p));
    v.push(f32_to_e8m0());
    v.push(ue4m3_to_f32());
    v.push(f32_to_ue4m3());
    v.push(fp6_to_f32("arch_e3m2_to_f32", 3, 2));
    v.push(f32_to_fp6("arch_f32_to_e3m2", 3, 2));
    for (tag, widen, narrow) in [
        ("e5m2", "arch_e5m2_to_f32", "arch_f32_to_e5m2"),
        ("e4m3", "arch_e4m3_to_f32", "arch_f32_to_e4m3"),
    ] {
        v.push(fp8_bin(
            &format!("arch_{tag}_add"),
            widen,
            narrow,
            "arch_f32_add",
        ));
        v.push(fp8_bin(
            &format!("arch_{tag}_sub"),
            widen,
            narrow,
            "arch_f32_sub",
        ));
        v.push(fp8_bin(
            &format!("arch_{tag}_mul"),
            widen,
            narrow,
            "arch_f32_mul",
        ));
        v.push(fp8_fma(&format!("arch_fma_{tag}"), widen, narrow));
        v.push(fp8_cmp(&format!("arch_{tag}_eq"), widen, "arch_f32_eq"));
        v.push(fp8_cmp(&format!("arch_{tag}_ne"), widen, "arch_f32_ne"));
        v.push(fp8_cmp(&format!("arch_{tag}_lt"), widen, "arch_f32_lt"));
        v.push(fp8_cmp(&format!("arch_{tag}_gt"), widen, "arch_f32_gt"));
        v.push(fp8_cmp(&format!("arch_{tag}_le"), widen, "arch_f32_le"));
        v.push(fp8_cmp(&format!("arch_{tag}_ge"), widen, "arch_f32_ge"));
    }
    v
}

/// Extra helpers exposed to the **Lean** backend only (not part of `arch build`
/// SV or `arch formal` SMT — they would be dead there). They surface the pieces
/// that `f32_mul` inlines — the decode fields and the shared round-and-pack at
/// the multiply width — as named functions, so the Lean proof can state the
/// reduction `mul (finite) = round48(sign, mant_a·mant_b, e0)`. Because they are
/// built from the *same* `decode`/`normround` as `f32_mul`, the multiplier
/// appears identically on both sides of that equation and `bv_decide` discharges
/// it structurally (no SAT-hard multiplier-equivalence). This isolates the entire
/// remaining Tier-2 crux into one function: `arch_round48`.
pub fn lean_extra_functions(p: FpCompat) -> Vec<FpFn> {
    let decode_mant = {
        let x = var("x", 32);
        FpFn::new("arch_decode_mant", &[("x", 32)], 24, decode(&x).mant)
    };
    let decode_eunb = {
        let x = var("x", 32);
        FpFn::new("arch_decode_eunb", &[("x", 32)], 16, decode(&x).eunb)
    };
    let round48 = {
        let s = var("s", 1);
        let sig = var("sig", 48);
        let e0 = var("e0", 16);
        FpFn::new(
            "arch_round48",
            &[("s", 1), ("sig", 48), ("e0", 16)],
            32,
            normround(&s, &sig, &e0),
        )
    };
    let msb48 = {
        let sig = var("sig", 48);
        FpFn::new("arch_msb_index48", &[("sig", 48)], 16, msb_index(&sig))
    };
    // FMA-width instances (the fma correctness proof rounds at FMA_W = 470).
    let round470 = {
        let s = var("s", 1);
        let sig = var("sig", 470);
        let e0 = var("e0", 16);
        FpFn::new(
            "arch_round470",
            &[("s", 1), ("sig", 470), ("e0", 16)],
            32,
            normround(&s, &sig, &e0),
        )
    };
    // Sticky-fold FMA rounder width: mag is mw = (48 + FMA_G) + 2 = 98 bits.
    // These are the pieces the *new* fma correctness proof reduces to (the
    // tractable width-98 analogue of round470/msb470).
    let round98 = {
        let s = var("s", 1);
        let sig = var("sig", 98);
        let e0 = var("e0", 16);
        FpFn::new(
            "arch_round98",
            &[("s", 1), ("sig", 98), ("e0", 16)],
            32,
            normround(&s, &sig, &e0),
        )
    };
    let msb98 = {
        let sig = var("sig", 98);
        FpFn::new("arch_msb_index98", &[("sig", 98)], 16, msb_index(&sig))
    };
    let msb470 = {
        let sig = var("sig", 470);
        FpFn::new("arch_msb_index470", &[("sig", 470)], 16, msb_index(&sig))
    };
    // fma's pre-rounding pieces (the aligned product±addend magnitude, its LSB
    // exponent, and the result sign) exposed for the Lean fma reduction proof.
    let fma_part = |which: &str| -> Bv {
        let a = var("a", 32);
        let b = var("b", 32);
        let c = var("c", 32);
        let da = decode(&a);
        let db = decode(&b);
        let dc = decode(&c);
        let sp = bxor(&da.sign, &db.sign);
        let mp = mul(&zext(&da.mant, 48), &zext(&db.mant, 48));
        let ep = add(&da.eunb, &db.eunb);
        let p_ge_c = sge(&ep, &dc.eunb);
        let e_lo = ite(&p_ge_c, &dc.eunb, &ep);
        let pt = ite(
            &p_ge_c,
            &shl(&zext(&mp, FMA_W), &sub(&ep, &dc.eunb)),
            &zext(&mp, FMA_W),
        );
        let ct = ite(
            &p_ge_c,
            &zext(&dc.mant, FMA_W),
            &shl(&zext(&dc.mant, FMA_W), &sub(&dc.eunb, &ep)),
        );
        let same = eq(&sp, &dc.sign);
        let pt_gt = ugt(&pt, &ct);
        let mag = ite(
            &same,
            &add(&pt, &ct),
            &ite(&pt_gt, &sub(&pt, &ct), &sub(&ct, &pt)),
        );
        let res_sign = ite(&same, &sp, &ite(&pt_gt, &sp, &dc.sign));
        match which {
            "mag" => mag,
            "elo" => e_lo,
            "sign" => res_sign,
            _ => unreachable!(),
        }
    };
    let fma_mag = FpFn::new(
        "arch_fma_mag",
        &[("a", 32), ("b", 32), ("c", 32)],
        FMA_W,
        fma_part("mag"),
    );
    let fma_elo = FpFn::new(
        "arch_fma_elo",
        &[("a", 32), ("b", 32), ("c", 32)],
        16,
        fma_part("elo"),
    );
    let fma_sign = FpFn::new(
        "arch_fma_sign",
        &[("a", 32), ("b", 32), ("c", 32)],
        1,
        fma_part("sign"),
    );

    // ── Sticky-fold (new) fma pre-rounding pieces, width 98 ──────────────────
    // The 98-bit magnitude, its LSB exponent e0, and the result sign of the
    // *new* bounded sticky-fold general path — the reduction target for the new
    // fma correctness proof (analogue of fma_mag/elo/sign at the 98-bit rounder).
    let fma_part_new = |which: &str| -> Bv {
        let a = var("a", 32);
        let b = var("b", 32);
        let c = var("c", 32);
        let da = decode(&a);
        let db = decode(&b);
        let dc = decode(&c);
        let sp = bxor(&da.sign, &db.sign);
        let mp = mul(&zext(&da.mant, 48), &zext(&db.mant, 48));
        let ep = add(&da.eunb, &db.eunb);
        let c_mant48 = zext(&dc.mant, 48);
        let hi_is_p = sge(&ep, &dc.eunb);
        let psel = |fp_: &Bv, fc: &Bv| ite(&hi_is_p, fp_, fc);
        let sig_hi = psel(&mp, &c_mant48);
        let sig_lo = psel(&c_mant48, &mp);
        let e_hi = psel(&ep, &dc.eunb);
        let sign_hi = psel(&sp, &dc.sign);
        let sign_lo = psel(&dc.sign, &sp);
        let diff = sub(&e_hi, &psel(&dc.eunb, &ep));
        let fw = 48 + FMA_G;
        let hi_field = shl(&zext(&sig_hi, fw), &cst(FMA_G as u128, 16));
        let lo_ext = shl(&zext(&sig_lo, fw), &cst(FMA_G as u128, 16));
        let lo_field = lshr(&lo_ext, &diff);
        let mask = sub(&shl(&cst(1, fw), &diff), &cst(1, fw));
        let sticky = ne(&band(&lo_ext, &mask), &cst(0, fw));
        let hi_e = concat(&hi_field, &cst(0, 1));
        let lo_e = concat(&lo_field, &sticky);
        let same = eq(&sign_hi, &sign_lo);
        let ge = uge(&hi_e, &lo_e);
        let raw = ite(&ge, &sub(&hi_e, &lo_e), &sub(&lo_e, &hi_e));
        let mw = fw + 2;
        let mag = ite(
            &same,
            &add(&zext(&hi_e, mw), &zext(&lo_e, mw)),
            &zext(&raw, mw),
        );
        let res_sign = ite(&same, &sign_hi, &ite(&ge, &sign_hi, &sign_lo));
        let e0 = sub(&e_hi, &cst((FMA_G + 1) as u128, 16));
        match which {
            "mag" => mag,
            "elo" => e0,
            "sign" => res_sign,
            _ => unreachable!(),
        }
    };
    let fma_mag98 = FpFn::new(
        "arch_fma_mag98",
        &[("a", 32), ("b", 32), ("c", 32)],
        98,
        fma_part_new("mag"),
    );
    let fma_elo98 = FpFn::new(
        "arch_fma_elo98",
        &[("a", 32), ("b", 32), ("c", 32)],
        16,
        fma_part_new("elo"),
    );
    let fma_sign98 = FpFn::new(
        "arch_fma_sign98",
        &[("a", 32), ("b", 32), ("c", 32)],
        1,
        fma_part_new("sign"),
    );

    vec![
        decode_mant,
        decode_eunb,
        round48,
        msb48,
        round470,
        msb470,
        round98,
        msb98,
        fma_mag,
        fma_elo,
        fma_sign,
        fma_mag98,
        fma_elo98,
        fma_sign98,
        // exact-wide reference FMA, for the sticky-fold equivalence theorem
        fma_f32_ref(p),
    ]
}

#[cfg(test)]
mod e2m1_ir_tests {
    use super::*;
    use crate::fp_ir::{cst, eval_bv};
    use std::collections::HashMap;

    fn call1(fns: &[FpFn], name: &str, arg: u128, aw: u32, rw: u32) -> u128 {
        let node = crate::fp_ir::call(name, &[cst(arg, aw)], rw);
        eval_bv(&node, &HashMap::new(), fns).expect("E2M1 helpers must be decidable")
    }

    /// The IR widen must agree with `fp_lit`'s reference decoder on every
    /// one of the 16 encodings. These are two independent implementations —
    /// a bit-vector DAG rendered to SV/SMT, and a table in Rust — so
    /// agreement is real evidence rather than a tautology.
    #[test]
    fn e2m1_widen_matches_the_reference_on_all_16_encodings() {
        let fns = fp_functions(crate::FpCompat::default());
        for enc in 0u128..16 {
            let got = call1(&fns, "arch_e2m1_to_f32", enc, 4, 32) as u32;
            let want = crate::fp_lit::e2m1_bits_to_f64(enc as u8) as f32;
            assert_eq!(
                f32::from_bits(got),
                want,
                "widen({enc:#X}): IR gave {} want {want}",
                f32::from_bits(got)
            );
            // -0.0 must keep its sign bit, not collapse to +0.0.
            if enc == 0x8 {
                assert_eq!(got, 0x8000_0000, "-0.0 must stay negative");
            }
        }
    }

    /// Narrowing is exhaustive over the value set plus the boundaries that
    /// the all-finite overflow rule governs. With the IEEE rule instead,
    /// 4.0 and 6.0 — E2M1's two largest FINITE values — would be treated as
    /// overflow, which is exactly what `TopBinade::AllFinite` prevents.
    #[test]
    fn e2m1_narrow_saturates_and_keeps_the_top_binade_finite() {
        let fns = fp_functions(crate::FpCompat::default());
        let nar = |v: f32| call1(&fns, "arch_f32_to_e2m1", v.to_bits() as u128, 32, 4) as u8;
        // Every representable value narrows to itself.
        for enc in 0u8..16 {
            let v = crate::fp_lit::e2m1_bits_to_f64(enc) as f32;
            assert_eq!(nar(v), enc, "narrow({v}) should be {enc:#X}");
        }
        // The top binade is FINITE — the whole point of AllFinite.
        assert_eq!(nar(4.0), 0x6);
        assert_eq!(nar(6.0), 0x7);
        // Round-to-nearest, ties-to-even, matching fp_lit.
        assert_eq!(nar(2.5), 0x4, "tie 2/3 -> 2.0");
        assert_eq!(nar(3.5), 0x6, "tie 3/4 -> 4.0");
        assert_eq!(nar(0.25), 0x0, "tie 0/0.5 -> 0.0");
        // 5.0 is EQUIDISTANT between 4.0 and 6.0; ties-to-even picks the
        // even mantissa bit, i.e. 4.0. (An early draft of this test
        // asserted 6.0 "nearer than 4" — it is not.)
        assert_eq!(nar(5.0), 0x6);
        // Runtime overflow SATURATES (no Inf/NaN exists to produce).
        assert_eq!(nar(1e30), 0x7);
        assert_eq!(nar(-1e30), 0xF);
        assert_eq!(nar(f32::INFINITY), 0x7);
        assert_eq!(nar(f32::NEG_INFINITY), 0xF);
        assert_eq!(nar(f32::NAN), 0x7, "no NaN encoding: saturate");
        // Underflow flushes to a signed zero.
        assert_eq!(nar(1e-30), 0x0);
        assert_eq!(nar(-1e-30), 0x8);
    }

    /// widen∘narrow is the identity on every encoding — the property a
    /// block format depends on when it round-trips element data.
    #[test]
    fn e2m1_round_trips_through_f32() {
        let fns = fp_functions(crate::FpCompat::default());
        for enc in 0u128..16 {
            let f32b = call1(&fns, "arch_e2m1_to_f32", enc, 4, 32);
            let back = call1(&fns, "arch_f32_to_e2m1", f32b, 32, 4);
            assert_eq!(back, enc, "round-trip failed for {enc:#X}");
        }
    }
}

#[cfg(test)]
mod fp6_ir_tests {
    use super::*;
    use crate::fp_ir::{cst, eval_bv};
    use std::collections::HashMap;

    fn call1(fns: &[FpFn], name: &str, arg: u128, aw: u32, rw: u32) -> u128 {
        let node = crate::fp_ir::call(name, &[cst(arg, aw)], rw);
        eval_bv(&node, &HashMap::new(), fns).expect("FP6 helpers must be decidable")
    }

    /// Both FP6 formats: the IR widen must agree with `fp_lit`'s independent
    /// reference on every one of the 64 encodings, and widen∘narrow must be
    /// the identity. Two separate implementations — a bit-vector DAG and a
    /// Rust table — so agreement is evidence, not tautology.
    #[test]
    fn fp6_ir_matches_reference_and_round_trips() {
        let fns = fp_functions(crate::FpCompat::default());
        for (widen, narrow, dec, max_enc) in [
            (
                "arch_e2m3_to_f32",
                "arch_f32_to_e2m3",
                crate::fp_lit::e2m3_bits_to_f64 as fn(u8) -> f64,
                0x3Fu128,
            ),
            (
                "arch_e3m2_to_f32",
                "arch_f32_to_e3m2",
                crate::fp_lit::e3m2_bits_to_f64 as fn(u8) -> f64,
                0x3F,
            ),
        ] {
            for enc in 0..=max_enc {
                let got = call1(&fns, widen, enc, 6, 32) as u32;
                let want = dec(enc as u8) as f32;
                assert_eq!(
                    f32::from_bits(got),
                    want,
                    "{widen}({enc:#X}) = {} want {want}",
                    f32::from_bits(got)
                );
                // widen -> narrow is the identity.
                let back = call1(&fns, narrow, got as u128, 32, 6);
                assert_eq!(back, enc, "{narrow} round-trip failed for {enc:#X}");
            }
            // -0.0 keeps its sign through the widen.
            assert_eq!(call1(&fns, widen, 0x20, 6, 32), 0x8000_0000);
        }
    }

    /// The top binade is FINITE in both formats — under the IEEE rule its
    /// largest values would be treated as overflow. Runtime narrowing
    /// saturates, since neither format has a NaN or an infinity.
    #[test]
    fn fp6_top_binade_is_finite_and_overflow_saturates() {
        let fns = fp_functions(crate::FpCompat::default());
        let nar = |n: &str, v: f32| call1(&fns, n, v.to_bits() as u128, 32, 6) as u8;
        // E2M3 max 7.5, E3M2 max 28.0 — both must narrow to the max code.
        assert_eq!(nar("arch_f32_to_e2m3", 7.5), 0x1F);
        assert_eq!(nar("arch_f32_to_e3m2", 28.0), 0x1F);
        // Saturation, not Inf/NaN.
        assert_eq!(nar("arch_f32_to_e2m3", 1e30), 0x1F);
        assert_eq!(nar("arch_f32_to_e3m2", 1e30), 0x1F);
        assert_eq!(nar("arch_f32_to_e2m3", f32::INFINITY), 0x1F);
        assert_eq!(nar("arch_f32_to_e2m3", f32::NAN), 0x1F);
        assert_eq!(nar("arch_f32_to_e3m2", -1e30), 0x3F, "sign preserved");
        // Underflow flushes to a signed zero.
        assert_eq!(nar("arch_f32_to_e2m3", 1e-30), 0x00);
        assert_eq!(nar("arch_f32_to_e2m3", -1e-30), 0x20);
    }
}

#[cfg(test)]
mod e8m0_ir_tests {
    use super::*;
    use crate::fp_ir::{cst, eval_bv};
    use std::collections::HashMap;

    fn call1(fns: &[FpFn], name: &str, arg: u128, aw: u32, rw: u32) -> u128 {
        let node = crate::fp_ir::call(name, &[cst(arg, aw)], rw);
        eval_bv(&node, &HashMap::new(), fns).expect("E8M0 helpers must be decidable")
    }

    /// UE4M3 is E4M3 restricted to sign 0. Checked over ALL 128 valid codes
    /// against an independent `f64` reconstruction of the encoding, plus the
    /// two facts that distinguish it from its neighbours: it HAS a zero
    /// (E8M0 does not) and its NaN is 0x7F (E4M3's is sign-agnostic).
    #[test]
    fn ue4m3_widen_matches_e4m3_with_sign_zero() {
        let fns = fp_functions(crate::FpCompat::default());
        for c in 0u128..=0x7F {
            let got = f32::from_bits(call1(&fns, "arch_ue4m3_to_f32", c, 8, 32) as u32);
            if c == 0x7F {
                assert!(got.is_nan(), "UE4M3 0x7F is the sole NaN");
                continue;
            }
            // Independent reconstruction: bias 7, 3 mantissa bits.
            let e = ((c >> 3) & 0xF) as i32;
            let m = (c & 7) as f64;
            let want = if e == 0 {
                (m / 8.0) * 2f64.powi(-6)
            } else {
                (1.0 + m / 8.0) * 2f64.powi(e - 7)
            } as f32;
            assert_eq!(got, want, "ue4m3 {c:#04X}");
            // A scale is never negative.
            assert!(got >= 0.0, "ue4m3 {c:#04X} widened negative");
        }
        // Unlike E8M0, UE4M3 HAS a zero.
        assert_eq!(
            f32::from_bits(call1(&fns, "arch_ue4m3_to_f32", 0, 8, 32) as u32),
            0.0,
            "UE4M3 0x00 is zero — unlike E8M0, whose 0x00 is 2^-127"
        );
        // Largest finite is 448, same as E4M3.
        assert_eq!(
            f32::from_bits(call1(&fns, "arch_ue4m3_to_f32", 0x7E, 8, 32) as u32),
            448.0
        );
        // The padding bit is MASKED, not trusted: a stray high bit must not
        // be read as an E4M3 sign and negate the scale.
        for c in 0u128..=0x7F {
            assert_eq!(
                call1(&fns, "arch_ue4m3_to_f32", c, 8, 32),
                call1(&fns, "arch_ue4m3_to_f32", c | 0x80, 8, 32),
                "code {c:#04X}: the padding bit must not change the value"
            );
        }
    }

    /// The narrow takes the MAGNITUDE (a scale is non-negative, matching
    /// `arch_f32_to_e8m0`), always clears the padding bit, and round-trips
    /// every finite code.
    #[test]
    fn ue4m3_narrow_is_magnitude_and_round_trips() {
        let fns = fp_functions(crate::FpCompat::default());
        let nar = |v: f32| call1(&fns, "arch_f32_to_ue4m3", v.to_bits() as u128, 32, 8) as u8;
        for c in 0u8..=0x7E {
            let v = f32::from_bits(call1(&fns, "arch_ue4m3_to_f32", c as u128, 8, 32) as u32);
            assert_eq!(nar(v), c, "code {c:#04X} must round-trip");
            // Sign is irrelevant to a scale.
            assert_eq!(
                nar(-v),
                c,
                "code {c:#04X}: negated input must give the same scale"
            );
        }
        // Every result has the padding bit clear.
        for v in [0.0f32, 1.0, 448.0, -448.0, 1e30, -1e30, f32::MIN_POSITIVE] {
            assert_eq!(nar(v) & 0x80, 0, "padding bit set for {v}");
        }
        assert_eq!(nar(0.0), 0x00, "zero narrows to the zero code");
    }

    /// Every one of the 256 E8M0 codes widens to exactly 2^(e-127), with
    /// 0xFF as NaN. Checked against `f64::powi`, an entirely separate
    /// computation from the bit surgery the IR performs.
    #[test]
    fn e8m0_widen_is_two_to_the_e_minus_127() {
        let fns = fp_functions(crate::FpCompat::default());
        for e in 0u128..=254 {
            let got = f32::from_bits(call1(&fns, "arch_e8m0_to_f32", e, 8, 32) as u32);
            let want = 2f64.powi(e as i32 - 127) as f32;
            assert_eq!(got, want, "e8m0 {e:#04X} should be 2^{}", e as i32 - 127);
        }
        // 0x00 is the MINIMUM SCALE 2^-127 — emphatically not zero.
        let min = f32::from_bits(call1(&fns, "arch_e8m0_to_f32", 0, 8, 32) as u32);
        assert_eq!(min, 2f32.powi(-127));
        assert_ne!(min, 0.0, "E8M0 has NO zero encoding");
        // 0x7F is the identity scale, 0xFE the maximum.
        assert_eq!(
            f32::from_bits(call1(&fns, "arch_e8m0_to_f32", 0x7F, 8, 32) as u32),
            1.0
        );
        assert_eq!(
            f32::from_bits(call1(&fns, "arch_e8m0_to_f32", 0xFE, 8, 32) as u32),
            2f32.powi(127)
        );
        // 0xFF is NaN.
        assert!(f32::from_bits(call1(&fns, "arch_e8m0_to_f32", 0xFF, 8, 32) as u32).is_nan());
    }

    /// Narrowing extracts the f32 exponent, with MX-reference clamping:
    /// underflow to the minimum scale, non-finite to NaN.
    #[test]
    fn e8m0_narrow_extracts_the_exponent_and_clamps() {
        let fns = fp_functions(crate::FpCompat::default());
        let nar = |v: f32| call1(&fns, "arch_f32_to_e8m0", v.to_bits() as u128, 32, 8) as u8;
        assert_eq!(nar(1.0), 0x7F);
        assert_eq!(nar(2.0), 0x80);
        assert_eq!(nar(0.5), 0x7E);
        assert_eq!(nar(2f32.powi(127)), 0xFE);
        // Floors to the power of two, as a scale must.
        assert_eq!(nar(1.5), 0x7F, "1.5 -> 2^0");
        assert_eq!(nar(3.0), 0x80, "3.0 -> 2^1");
        // Sign is irrelevant: E8M0 has none.
        assert_eq!(nar(-1.0), 0x7F);
        assert_eq!(nar(-3.0), 0x80);
        // Underflow clamps to the MINIMUM SCALE, not to a zero that does
        // not exist in this format.
        assert_eq!(nar(0.0), 0x00);
        assert_eq!(nar(-0.0), 0x00);
        assert_eq!(nar(1e-45), 0x00, "f32 subnormal clamps down");
        // Non-finite becomes the NaN code.
        assert_eq!(nar(f32::INFINITY), 0xFF);
        assert_eq!(nar(f32::NEG_INFINITY), 0xFF);
        assert_eq!(nar(f32::NAN), 0xFF);
    }

    /// Round-trip over the representable scale range.
    #[test]
    fn e8m0_round_trips_every_scale() {
        let fns = fp_functions(crate::FpCompat::default());
        for e in 0u128..=254 {
            let f32b = call1(&fns, "arch_e8m0_to_f32", e, 8, 32);
            let back = call1(&fns, "arch_f32_to_e8m0", f32b, 32, 8);
            assert_eq!(back, e, "round-trip failed for scale {e:#04X}");
        }
    }
}
