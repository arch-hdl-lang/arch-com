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

/// Registered FP32 accumulator simulates correctly, including a float-literal
/// reg reset value driving the seq float path.
#[test]
fn fp_reg_accumulator_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/FpAcc.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_acc.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("2 pass / 0 fail"),
        "FP32 accumulator sim should pass; got:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// float→int conversions are toward-zero, per-N saturating, NaN→type-max.
#[test]
fn fp_to_int_saturation_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/FpSat.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_sat.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("7 pass / 0 fail"),
        "float->int saturation sim should pass; got:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// v1 rejects floats in positions the float-op dispatch can't resolve:
/// inside `Vec`, in `struct` fields, and in module-local `function`
/// signatures — rather than silently emitting integer arithmetic.
#[test]
fn fp_unsupported_positions_rejected() {
    let cases = [
        ("vec", "module M\n  port a: in Vec<FP32, 4>;\n  port o: out FP32;\n  comb o = a[0]; end comb\nend module M\n"),
        ("struct", "struct P\n  x: FP32;\nend struct P\nmodule M\n  port p: in P;\n  port o: out FP32;\n  comb o = p.x; end comb\nend module M\n"),
        ("function", "module M\n  function f(x: FP32) -> FP32\n    return x;\n  end function f\n  port a: in FP32;\n  port o: out FP32;\n  comb o = f(a); end comb\nend module M\n"),
    ];
    for (label, src) in cases {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("M.arch");
        std::fs::write(&path, src).unwrap();
        let out = arch().arg("check").arg(&path).output().expect("run arch check");
        assert!(
            !out.status.success(),
            "float in {label} position must be rejected in v1\nsrc:\n{src}"
        );
    }
}

/// A float `reg` reset value must be a float literal, not an integer literal
/// (which would store a bit pattern, not the numeric value).
#[test]
fn fp_reg_integer_reset_rejected() {
    let src = "module M\n  port clk: in Clock<S>;\n  port rst: in Reset<Sync>;\n  reg r: FP32 reset rst => 1;\n  seq on clk rising\n    r <= r;\n  end seq\nend module M\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("M.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch().arg("check").arg(&path).output().expect("run arch check");
    assert!(!out.status.success(), "integer reset for a float reg must be rejected");
}

/// Differential equivalence (doc/plan_fp_types.md §8.2): the emitted
/// synthesizable FP helpers, verilated and run against a host IEEE-754 (DPI-C)
/// reference over corner + randomized + cancellation-prone vectors, must be
/// bit-exact for every op / compare / conversion / BF16 wrapper.
///
/// Skips cleanly when Verilator is not installed. The helper functions are
/// `$unit`-scope in the `arch build` output, so the emitted `.sv` is verilated
/// alongside the testbench (which calls them) and the DPI reference.
#[test]
fn fp_rtl_differential_equiv_verilator() {
    fn verilator_available() -> bool {
        std::process::Command::new("verilator")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !verilator_available() {
        eprintln!("skipping fp_rtl_differential_equiv_verilator: verilator not in PATH");
        return;
    }

    let manifest = env!("CARGO_MANIFEST_DIR");
    let td = tempfile::tempdir().expect("tempdir");
    let sv = td.path().join("FpArith.sv");

    // `arch build` emits the full FP helper block (all ops + conversions + BF16)
    // ahead of the module whenever a design uses FP.
    let out = arch()
        .arg("build")
        .arg(format!("{manifest}/tests/fp_v1/FpArith.arch"))
        .arg("-o")
        .arg(&sv)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build failed\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let obj = td.path().join("obj");
    let tb = format!("{manifest}/tests/fp_v1/rtl_diff/tb_fp_diff.sv");
    let dpi = format!("{manifest}/tests/fp_v1/rtl_diff/dpi_ref.cpp");
    let vout = std::process::Command::new("verilator")
        .args(["--binary", "--timing", "-Wno-WIDTH", "-Wno-UNOPTFLAT",
               "-Wno-WIDTHTRUNC", "-Wno-WIDTHEXPAND", "-Wno-SHORTREAL",
               "-Wno-BLKANDNBLK", "-Wno-UNUSEDSIGNAL", "-Wno-MULTITOP",
               "--top-module", "tb", "-o", "sim_diff"])
        .arg("-Mdir")
        .arg(&obj)
        .arg(&sv)
        .arg(&tb)
        .arg(&dpi)
        .output()
        .expect("run verilator");
    assert!(
        vout.status.success(),
        "verilator build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&vout.stdout),
        String::from_utf8_lossy(&vout.stderr)
    );

    let run = std::process::Command::new(obj.join("sim_diff"))
        .output()
        .expect("run verilated sim");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("ARCH_FP_RTL_DIFF: ALL PASS"),
        "RTL differential check failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// SMT equivalence proofs (doc/plan_fp_types.md §8.1): the emitted FP RTL
/// (`src/codegen/fp.rs`), transcribed bit-for-bit into SMT-LIB, is proven
/// equivalent to the IEEE-754 `FloatingPoint` theory by z3. Each `.smt2` in
/// `tests/fp_v1/smt_proof/` must discharge `unsat` (no counterexample over the
/// entire input space). Covers FP32 comparisons, BF16 widen/narrow, and
/// float->int (in-range); the RNE arithmetic stays on the §8.2 differential
/// backstop (see the directory README). Emits a small proof certificate.
///
/// Skips cleanly when `z3` is not installed.
#[test]
fn fp_smt_equivalence_proofs() {
    fn z3_available() -> bool {
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !z3_available() {
        eprintln!("skipping fp_smt_equivalence_proofs: z3 not in PATH");
        return;
    }
    let z3ver = {
        let o = std::process::Command::new("z3").arg("--version").output().unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };

    let manifest = env!("CARGO_MANIFEST_DIR");
    let dir = format!("{manifest}/tests/fp_v1/smt_proof");
    let proofs = [
        "fp32_compare.smt2",
        "bf16_narrow.smt2",
        "bf16_widen.smt2",
        "f32_to_sint.smt2",
        "f32_to_uint.smt2",
    ];

    let mut cert = String::new();
    cert.push_str("ARCH FP RTL — SMT equivalence proof certificate (plan §8.1)\n");
    cert.push_str(&format!("solver: {z3ver}\n"));
    cert.push_str("property: emitted RTL (src/codegen/fp.rs) ≡ SMT FloatingPoint theory (IEEE-754 RNE)\n\n");

    for p in proofs {
        let path = format!("{dir}/{p}");
        let out = std::process::Command::new("z3")
            .arg("-T:600") // per-query 600s wall-clock cap
            .arg(&path)
            .output()
            .unwrap_or_else(|e| panic!("failed to run z3 on {p}: {e}"));
        let res = String::from_utf8_lossy(&out.stdout);
        let first = res.lines().next().unwrap_or("").trim();
        cert.push_str(&format!("{p}: {first}\n"));
        assert_eq!(
            first, "unsat",
            "SMT proof {p} did not discharge as unsat (got {first:?})\nstdout:\n{res}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    cert.push_str("\nresult: ALL PROVED (unsat)\n");
    eprintln!("\n{cert}");
}
