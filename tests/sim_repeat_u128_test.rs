//! Regression coverage for u128-band bit replication in the native `arch
//! sim` model. `{N{x}}` whose replicated result width lands in 65..=128 bits
//! was emitted through a uint64-only `_arch_repeat` helper, truncating the
//! top bits to 64 (a wrong value, no warning, no abort) while the emitted SV
//! `{N{x}}` was correct — a silent sim != SV miscompile.
//!
//! The fixture replicates into 65, 120, 128 and 104 bits (band boundaries and a wide
//! operand), plus 48 bits (the <=64 control that must stay on the unchanged
//! uint64 path). All expected values were cross-checked against
//! Verilator-emitted SV.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

#[test]
fn repeat_result_in_u128_band_not_truncated_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_repeat_u128.arch")
        .arg("--tb")
        .arg("tests/sim_repeat_u128_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_repeat_u128\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("9 pass / 0 fail"),
        "expected all nine u128-band replication checks to pass; got:\n{stdout}"
    );
}
