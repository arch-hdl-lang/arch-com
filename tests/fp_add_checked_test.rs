//! Verifies `arch_f32_add_checked` — the value is bit-identical to
//! `arch_f32_add`, and the four IEEE flags fire exactly on the right cases
//! (overflow, invalid, benign vs. real underflow), via the FP-IR evaluator.

use std::collections::HashMap;

// Re-expose the IR evaluator + builders through the crate.
use arch::fp_ir::{call, cst, eval_bv};

fn checked(a: u32, b: u32) -> (u32, bool, bool, bool, bool) {
    let fns = arch::fp_ops::fp_functions(arch::FpCompat::default());
    let env = HashMap::new();
    let got = eval_bv(
        &call(
            "arch_f32_add_checked",
            &[cst(a as u128, 32), cst(b as u128, 32)],
            36,
        ),
        &env,
        &fns,
    )
    .expect("add_checked evaluable");
    let value = (got & 0xFFFF_FFFF) as u32;
    let overflow = (got >> 32) & 1 == 1;
    let underflow = (got >> 33) & 1 == 1;
    let invalid = (got >> 34) & 1 == 1;
    let inexact = (got >> 35) & 1 == 1;
    (value, overflow, underflow, invalid, inexact)
}

fn plain_add(a: u32, b: u32) -> u32 {
    let fns = arch::fp_ops::fp_functions(arch::FpCompat::default());
    let env = HashMap::new();
    eval_bv(
        &call(
            "arch_f32_add",
            &[cst(a as u128, 32), cst(b as u128, 32)],
            32,
        ),
        &env,
        &fns,
    )
    .expect("add evaluable") as u32
}

#[test]
fn value_is_bit_identical_to_arch_f32_add() {
    let pats: [u32; 8] = [
        0x0000_0000,
        0x3F80_0000,
        0x0000_0001,
        0x0080_0000,
        0x7F7F_FFFF,
        0x4049_0FDB,
        0xC2C8_0000,
        0x3400_0000,
    ];
    for &a in &pats {
        for &b in &pats {
            let (v, ..) = checked(a, b);
            assert_eq!(v, plain_add(a, b), "value mismatch at {a:#010X}+{b:#010X}");
        }
    }
}

#[test]
fn flag_cases() {
    // exact: 1.0 + 1.0 = 2.0, no flags.
    let (v, ovf, unf, inv, inx) = checked(0x3F80_0000, 0x3F80_0000);
    assert_eq!(v, 0x4000_0000);
    assert!(
        !ovf && !unf && !inv && !inx,
        "1+1 should be exact, got {ovf} {unf} {inv} {inx}"
    );

    // overflow: MAX + MAX = +Inf, overflow + inexact, no underflow/invalid.
    let (v, ovf, unf, inv, inx) = checked(0x7F7F_FFFF, 0x7F7F_FFFF);
    assert_eq!(v, 0x7F80_0000, "MAX+MAX should be +Inf");
    assert!(
        ovf && inx && !unf && !inv,
        "MAX+MAX flags: {ovf} {unf} {inv} {inx}"
    );

    // benign: 1.0 + min_subnormal = 1.0, inexact (tiny lost) but NOT underflow
    // (result is normal). This is the add_negligible case.
    let (v, ovf, unf, inv, inx) = checked(0x3F80_0000, 0x0000_0001);
    assert_eq!(v, 0x3F80_0000, "1 + tiny should round to 1");
    assert!(!unf, "benign: result normal, underflow must be false");
    assert!(
        inx && !ovf && !inv,
        "benign: inexact only, got {ovf} {unf} {inv} {inx}"
    );

    // invalid: (+Inf) + (-Inf) = NaN, invalid; no over/underflow/inexact.
    let (_, ovf, unf, inv, inx) = checked(0x7F80_0000, 0xFF80_0000);
    assert!(inv, "Inf + (-Inf) must set invalid");
    assert!(
        !ovf && !unf && !inx,
        "invalid case: only invalid, got {ovf} {unf} {inv} {inx}"
    );
}
