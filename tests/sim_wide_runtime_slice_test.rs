//! Regression coverage for arch#868 — a runtime-bound bit-slice whose
//! derivable width exceeds 64 bits, taken from a base wider than 64 bits,
//! was silently truncated to 64 bits in the native `arch sim` model
//! (a wrong value, no warning, no abort). This is a gap in the arch#847
//! runtime-part-select fix: two independent 64-bit caps in
//! `runtime_bit_slice` — a `u64::MAX` mask and a `uint64_t` result cast
//! (plus `_arch_vw_bits` / `_arch_slice_mask`, both 64-bit-only) — zeroed
//! slice bits 64+.
//!
//! `v[(i + 79) : i]` has a runtime lo `i` and a derivable width of 80.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// 65..128-bit base (`_arch_u128` carrier): a width-80 runtime slice must
/// preserve all 80 bits (bits 64..79 were the ones being dropped).
#[test]
fn runtime_wide_slice_u128_base_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_wide_runtime_slice_u128.arch")
        .arg("--tb")
        .arg("tests/sim_wide_runtime_slice_u128_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_wide_runtime_slice_u128\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("2 pass / 0 fail"),
        "expected both width-80 u128-base slice checks to pass; got:\n{stdout}"
    );
}

/// >128-bit base (`VlWide` carrier): a width-80 runtime slice must route
/// through `_arch_vw_bits128` (not the width-capping `_arch_vw_bits`) so
/// bits 64..79 survive. Covers both a sub-word lo (`i=16`) and a
/// word-aligned lo (`i=64`).
#[test]
fn runtime_wide_slice_vlwide_base_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_wide_runtime_slice_vlwide.arch")
        .arg("--tb")
        .arg("tests/sim_wide_runtime_slice_vlwide_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_wide_runtime_slice_vlwide\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("2 pass / 0 fail"),
        "expected both width-80 VlWide-base slice checks to pass; got:\n{stdout}"
    );
}
