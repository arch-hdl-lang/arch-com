//! Regression coverage for arch#847 — seq assignment to a ranged
//! part-select with runtime (loop-variable) slice bounds.
//!
//! Pre-fix, the sim C++ emitter const-folded slice bounds through a
//! 32-default, so `mem[addr][(i*8+7):(i*8)] <= din[(i*8+7):(i*8)];`
//! under `if wem[i:i]` inside `for i in 0..3` emitted a non-assignable
//! read expression (the sim model did not even compile), a 1-bit lane
//! mask, and a `wem >> 32` condition. Found on the e203 ITCM/DTCM RAM
//! fixtures during PR #843's long verify.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// End-to-end: the sim model compiles AND byte-masked write semantics
/// are correct (masked lanes written, unmasked lanes preserved).
#[test]
fn runtime_ranged_slice_seq_write_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_ranged_slice_regression.arch")
        .arg("--tb")
        .arg("tests/sim_ranged_slice_regression_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_ranged_slice_regression\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("4 pass / 0 fail"),
        "expected all 4 masked-write checks to pass; got:\n{stdout}"
    );
}

/// The e203 ITCM RAM — the original arch#847 reproducer — must produce
/// a sim model the host C++ compiler accepts. Guard the codegen shape:
/// no rvalue-assignment, byte-wide lane mask, per-bit `wem` shift.
#[test]
fn e203_itcm_ram_sim_model_compiles() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/e203/e203_itcm_ram.arch")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim codegen");
    assert!(
        out.status.success(),
        "arch sim codegen should succeed for e203_itcm_ram\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let model = td.path().join("Ve203_itcm_ram.cpp");
    let cc = Command::new("c++")
        .arg("-std=c++17")
        .arg("-fsyntax-only")
        .arg("-I")
        .arg(td.path())
        .arg(&model)
        .output()
        .expect("run host c++ syntax check");
    assert!(
        cc.status.success(),
        "generated sim model must be valid C++\n{}",
        String::from_utf8_lossy(&cc.stderr),
    );
    let cpp = std::fs::read_to_string(&model).expect("read model");
    assert!(
        cpp.contains("& 0xFFULL"),
        "byte lane writes should use an 8-bit mask:\n{cpp}"
    );
    assert!(
        !cpp.contains("wem >> 32"),
        "wem lane-enable must shift by the loop var, not a folded 32:\n{cpp}"
    );
}
