//! Regression coverage for u128-band `onehot(idx)` in the native `arch sim`
//! model. `onehot(idx)` whose result width lands in 65..=128 bits was emitted
//! as `1ULL << idx` — a 64-bit shift that truncates the set bit to the low
//! word (UB for idx >= 64), while the emitted SV `(1 << idx)` is correct: a
//! silent sim != SV miscompile (#1182), sibling of the `{N{x}}` fix in
//! #1180/#1181.
//!
//! The fixture decodes into UInt<100> and UInt<128> at idx 64, 99 and 127
//! (band boundaries and the top bit), plus a UInt<48> control that must stay
//! on the unchanged `1ULL` path. All expected values were cross-checked
//! against Verilator-emitted SV.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

#[test]
fn onehot_result_in_u128_band_not_truncated_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_onehot_u128.arch")
        .arg("--tb")
        .arg("tests/sim_onehot_u128_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_onehot_u128\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("8 pass / 0 fail"),
        "expected all eight u128-band onehot checks to pass; got:\n{stdout}"
    );
}
