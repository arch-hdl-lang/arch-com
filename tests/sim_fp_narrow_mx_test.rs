//! Regression coverage for arch#867 — native `arch sim` narrowing
//! conversions to the sub-8-bit MX formats (`to_fp4e2m1`,
//! `to_fp6e2m3`, `to_fp6e3m2`).
//!
//! Pre-fix the sim C++ method emitter had no dispatch arm for these
//! three narrows, so the method call was emitted verbatim into the
//! generated C++ (`x.to_fp4e2m1()` on a `uint32_t`) and the sim model
//! failed to compile. `arch build` (SV) already handled all three.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// End-to-end: the sim model compiles AND every FP32 -> sub-8-bit MX
/// narrow round-trips to the reference code.
#[test]
fn sim_fp32_to_mx_narrows_roundtrip() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_fp_narrow_mx_regression.arch")
        .arg("--tb")
        .arg("tests/sim_fp_narrow_mx_regression_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_fp_narrow_mx_regression\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("7 pass / 0 fail"),
        "expected all 7 narrow checks to pass; got:\n{stdout}"
    );
    // Guard the codegen shape: the narrows must lower to the runtime
    // helpers, never fall through to a verbatim `.to_fp4e2m1()` call.
    let model = td.path().join("Vsim_fp_narrow_mx_regression.cpp");
    let cpp = std::fs::read_to_string(&model).expect("read sim model");
    assert!(
        cpp.contains("_arch_f32_to_e2m1(")
            && cpp.contains("_arch_f32_to_e2m3(")
            && cpp.contains("_arch_f32_to_e3m2("),
        "narrows must lower to the _arch_f32_to_* helpers:\n{cpp}"
    );
    assert!(
        !cpp.contains(".to_fp4e2m1()")
            && !cpp.contains(".to_fp6e2m3()")
            && !cpp.contains(".to_fp6e3m2()"),
        "no narrow may be emitted verbatim as a C++ method call:\n{cpp}"
    );
}
