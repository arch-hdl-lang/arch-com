//! FP32 / BF16 floating-point v1 integration tests.
//!
//! Covers the front-end (parse + type check), the SoftFloat-semantics
//! simulation backend (host IEEE-754 RNE), and the SystemVerilog emission
//! shape. See doc/plan_fp_types.md.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// `arch check` accepts the full FP surface.
#[test]
fn fp_check_passes() {
    let out = arch()
        .arg("check")
        .arg("tests/fp_v1/FpArith.arch")
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "arch check should pass for FpArith.arch\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// End-to-end simulation: FP32/BF16 arithmetic, fma, is_nan, NaN
/// canonicalization, and the conversion surface all match host IEEE-754.
#[test]
fn fp_sim_matches_host_ieee754() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/FpArith.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_fp.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for FpArith\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("13 pass / 0 fail"),
        "expected all 13 FP checks to pass; got:\n{stdout}"
    );
}

/// `arch build` dispatches FP ops to the emitted helper functions and
/// prepends the helper package.
#[test]
fn fp_build_emits_helpers_and_dispatch() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("FpArith.arch");
    std::fs::copy("tests/fp_v1/FpArith.arch", &arch_path).expect("copy arch into tempdir");
    let out = arch()
        .arg("build")
        .arg(&arch_path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let sv = std::fs::read_to_string(td.path().join("FpArith.sv")).expect("read FpArith.sv");
    assert!(sv.contains("function automatic logic [31:0] arch_f32_add"), "f32 add helper missing:\n{sv}");
    assert!(sv.contains("function automatic logic [15:0] arch_bf16_add"), "bf16 add helper missing");
    assert!(sv.contains("assign sum = arch_f32_add(a, b);"), "f32 add not dispatched:\n{sv}");
    assert!(sv.contains("assign prod = arch_f32_mul(a, b);"), "f32 mul not dispatched");
    assert!(sv.contains("assign hsum = arch_bf16_add(ha, hb);"), "bf16 add not dispatched");
    assert!(sv.contains("arch_fma_f32(a, b, c)"), "fma not dispatched");
    assert!(sv.contains("arch_bf16_to_f32(ha)"), "bf16->f32 conversion not dispatched");
    // FP32 and BF16 ports are packed bit vectors.
    assert!(sv.contains("input logic [31:0] a"), "FP32 port width wrong");
    assert!(sv.contains("input logic [15:0] ha"), "BF16 port width wrong");
}

/// The no-implicit-conversion rule: mixing FP32 and BF16 in an operator,
/// and assigning across float types without an explicit cast, are errors.
#[test]
fn fp_no_implicit_conversion_errors() {
    let src = r#"module Bad
  port a: in FP32;
  port h: in BF16;
  port o: out FP32;
  comb o = a + h; end comb
end module Bad
"#;
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("Bad.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch().arg("check").arg(&path).output().expect("run arch check");
    assert!(!out.status.success(), "mixing FP32 and BF16 must be a type error");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("float") || combined.contains("FP32") || combined.contains("BF16"),
        "error should mention the float type mismatch; got:\n{combined}"
    );
}

/// Assigning an FP32 value into a BF16 target without `.to_bf16()` is rejected.
#[test]
fn fp_assign_across_types_errors() {
    let src = r#"module Bad2
  port a: in FP32;
  port o: out BF16;
  comb o = a; end comb
end module Bad2
"#;
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("Bad2.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch().arg("check").arg(&path).output().expect("run arch check");
    assert!(!out.status.success(), "FP32 -> BF16 assignment without cast must error");
}
