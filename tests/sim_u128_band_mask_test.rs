//! Regression coverage for 65..=128-bit (`_arch_u128` carrier) `+%` / `-%` /
//! `*%` width-correctness in the native `arch sim` model (#1184, row 1).
//!
//! The wrap paths emitted `cast_to_(signed_)bits`, whose C++ type caps at
//! (u)int64_t. A 65..=128-bit wrapping result was therefore both computed
//! without its high carry/borrow (`+%`/`-%` left their operands in 64-bit C++
//! arithmetic) and truncated to 64 bits on store, while the emitted
//! SystemVerilog is width-correct — a silent `sim != SV` miscompile.
//!
//! The fixture covers the 65-bit lower boundary (carry into bit 64), the 96-bit
//! mid-band, mixed operand widths (`UInt<96> +% UInt<40>`), a signed wrap in
//! the band (`SInt<96>`, read through a slice so the non-native signed-compare
//! gap #952 cannot interfere), the 127-bit case, the 128-bit upper boundary,
//! and a `+%` inside a seq block. All expected values were cross-checked
//! against the emitted SystemVerilog under Verilator 5.048.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

#[test]
fn u128_band_wrap_results_full_width_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_u128_band_mask.arch")
        .arg("--tb")
        .arg("tests/sim_u128_band_mask_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_u128_band_mask\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("14 pass / 0 fail"),
        "expected all fourteen u128-band wrap checks to pass; got:\n{stdout}"
    );
}
