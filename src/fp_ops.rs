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
        eunb: ite(&e_is_0, &cst(NEG149_16, 16), &sub(&zext(&e, 16), &cst(150, 16))),
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
    let sub_res = bor(&concat(sign, &cst(0, 31)), &concat(sign, &extract(&kept, 30, 0)));

    // normal: a carry into bit 24 bumps the exponent; >=255 overflows to inf.
    let carry = is1(&extract(&kept, 24, 24));
    let biased_n = ite(&carry, &add(&biased, &cst(1, 16)), &biased);
    let kept_n = ite(&carry, &lshr(&kept, &cst(1, 16)), &kept);
    let overflow = sge(&biased_n, &cst(255, 16));
    let inf = concat(sign, &concat(&cst(0xFF, 8), &cst(0, 23)));
    let packed = concat(sign, &concat(&extract(&biased_n, 7, 0), &extract(&kept_n, 22, 0)));
    let norm_res = ite(&overflow, &inf, &packed);

    let zero = concat(sign, &cst(0, 31));
    ite(
        &eq(sig, &cst(0, w)),
        &zero,
        &ite(&biased_le0, &sub_res, &norm_res),
    )
}

// ── predicates / simple ops (as expressions for reuse) ──────────────────────

fn isnan(x: &Bv) -> Bv {
    and(&eq(&extract(x, 30, 23), &cst(0xFF, 8)), &ne(&extract(x, 22, 0), &cst(0, 23)))
}
fn iszero(x: &Bv) -> Bv {
    eq(&extract(x, 30, 0), &cst(0, 31))
}
fn eq_expr(a: &Bv, b: &Bv) -> Bv {
    ite(&or(&isnan(a), &isnan(b)), &cst(0, 1), &or(&eq(a, b), &and(&iszero(a), &iszero(b))))
}
fn lt_expr(a: &Bv, b: &Bv) -> Bv {
    let sa = extract(a, 31, 31);
    let sb = extract(b, 31, 31);
    let ma = extract(a, 30, 0);
    let mb = extract(b, 30, 0);
    let same_sign_cmp = ite(&eq(&sa, &cst(0, 1)), &ult(&ma, &mb), &ugt(&ma, &mb));
    let diff_sign = ite(&ne(&sa, &sb), &is1(&sa), &same_sign_cmp);
    ite(&or(&isnan(a), &isnan(b)), &cst(0, 1), &ite(&and(&iszero(a), &iszero(b)), &cst(0, 1), &diff_sign))
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
            &ite(&or(&da.is_inf, &db.is_inf), &inf, &ite(&or(&da.is_zero, &db.is_zero), &zero, &rounded)),
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
        cmp_fn("arch_f32_le", or(&lt_expr(&a(), &b()), &eq_expr(&a(), &b()))),
        cmp_fn("arch_f32_ge", or(&lt_expr(&b(), &a()), &eq_expr(&a(), &b()))),
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

// Exact-wide alignment widths: large enough to hold the exact aligned
// magnitude so no sticky/borrow logic is needed (the rounder re-derives
// guard/round/sticky). add: 23 + max-exponent-spread(253) + carry. fma: 48-bit
// product + max product/addend spread. Correctness is by construction; the §8.2
// differential harness is the oracle.
const ADD_G: u32 = 30; // bounded-adder guard bits (field = 24 + ADD_G)
const FMA_W: u32 = 470;

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
    let mag = ite(&same_sign, &add(&zext(&hi_e, mw), &zext(&lo_e, mw)), &zext(&raw, mw));
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

    // align product (ep) and c (eunb_c) to the lower exponent
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
    let mag = ite(&same, &add(&pt, &ct), &ite(&pt_gt, &sub(&pt, &ct), &sub(&ct, &pt)));
    let res_sign = ite(&same, &sp, &ite(&pt_gt, &sp, &dc.sign));
    let cancel = and(&bnot(&same), &eq(&pt, &ct));
    let general = ite(&cancel, &cst(0, 32), &normround(&res_sign, &mag, &e_lo));

    // product==0 (finite): result = signed-zero(sp) + c
    let prod_zero_res = call("arch_f32_add", &[concat(&sp, &cst(0, 31)), c.clone()], 32);
    // c==0 (finite, product nonzero): round the product alone
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

// ── int <-> float ───────────────────────────────────────────────────────────

fn i64_to_f32() -> FpFn {
    let v = var("v", 64);
    let sign = extract(&v, 63, 63);
    let mag = ite(&is1(&sign), &neg(&v), &v);
    let body = ite(&eq(&v, &cst(0, 64)), &cst(0, 32), &normround(&sign, &mag, &cst(0, 16)));
    FpFn::new("arch_i64_to_f32", &[("v", 64)], 32, body)
}
fn u64_to_f32() -> FpFn {
    let v = var("v", 64);
    let body = ite(&eq(&v, &cst(0, 64)), &cst(0, 32), &normround(&cst(0, 1), &v, &cst(0, 16)));
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
            &ite(&is1(&d.sign), &cst(0, 64), &ite(&d.is_inf, &lo64(&lim), &sat)),
        ),
    );
    FpFn::new("arch_f32_to_uint", &[("x", 32), ("n", 32)], 64, body)
}

// ── bf16 arithmetic = widen -> f32 op -> narrow (calls into the f32 fns) ─────
//
// Correctness of the f32 intermediate (innocuous double rounding): f32's
// subnormal range extends exactly 16 binades below bf16's (f32 to 2^-149, bf16
// to 2^-133), so at every bf16-representable magnitude the f32 precision is
// `p_bf16 + 16` bits. The double rounding is exact when `p_f32 >= 2*p_bf16 + 2`,
// i.e. `p_bf16 + 16 >= 2*p_bf16 + 2`, i.e. `p_bf16 <= 14` — always true since
// `p_bf16 <= 8`. So mul/add/sub/fma via f32 are correctly-rounded bf16.
// `arch_bf16_{mul,add,sub}` are machine-proved `unsat` vs `fp.{mul,add,sub}` on
// `(_ FloatingPoint 8 8)` (z3); `arch_fma_bf16` is correct by the same argument
// (and an exhaustive deep-subnormal check) but its `fp.fma`-based miter is not
// dischargeable by z3 4.8.12 (incomplete `fp.fma` -> spurious `sat`).

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
    FpFn::new("arch_fma_bf16", &[("a", 16), ("b", 16), ("c", 16)], 16, body)
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
pub fn lean_extra_functions() -> Vec<FpFn> {
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
        FpFn::new("arch_round48", &[("s", 1), ("sig", 48), ("e0", 16)], 32, normround(&s, &sig, &e0))
    };
    vec![decode_mant, decode_eunb, round48]
}
