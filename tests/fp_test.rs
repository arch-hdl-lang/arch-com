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
    assert!(
        sv.contains("function automatic logic [31:0] arch_f32_add"),
        "f32 add helper missing:\n{sv}"
    );
    assert!(
        sv.contains("function automatic logic [15:0] arch_bf16_add"),
        "bf16 add helper missing"
    );
    assert!(
        sv.contains("assign sum = arch_f32_add(a, b);"),
        "f32 add not dispatched:\n{sv}"
    );
    assert!(
        sv.contains("assign prod = arch_f32_mul(a, b);"),
        "f32 mul not dispatched"
    );
    assert!(
        sv.contains("assign hsum = arch_bf16_add(ha, hb);"),
        "bf16 add not dispatched"
    );
    assert!(sv.contains("arch_fma_f32(a, b, c)"), "fma not dispatched");
    assert!(
        sv.contains("arch_bf16_to_f32(ha)"),
        "bf16->f32 conversion not dispatched"
    );
    // FP32 and BF16 ports are packed bit vectors.
    assert!(sv.contains("input logic [31:0] a"), "FP32 port width wrong");
    assert!(
        sv.contains("input logic [15:0] ha"),
        "BF16 port width wrong"
    );
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
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "mixing FP32 and BF16 must be a type error"
    );
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
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "FP32 -> BF16 assignment without cast must error"
    );
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

/// Pinned characterization test for `int.to_bf16()` (issue #629, decided
/// 2026-07-12): DECLARED semantics are f32-routed —
/// `narrow_bf16(f32(i))` — the same convention as `bf16` fma's f32-accumulate
/// (PR #627), documented in doc/ARCH_HDL_Specification.md §3.8 "Rounding
/// convention". This locks the arch-sim backend's result for the witness
/// (`i=16842753` → `0x4b80`, NOT the correctly-rounded `0x4b81` — 1 bf16 ULP
/// away) plus an exact case below `2^24` where no double-rounding hazard
/// exists. If a future change makes `int.to_bf16()` correctly-rounded, this
/// test trips loudly — that would be a user-facing semantics change requiring
/// a fresh spec decision, not a silent codegen tweak (see issue #629).
#[test]
fn fp_int_to_bf16_f32_routed_witness_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/IntToBf16.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_int_to_bf16.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("2 pass / 0 fail"),
        "int.to_bf16() f32-routed witness (arch sim) should pass; got:\n{stdout}\nstderr:\n{}",
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
        let out = arch()
            .arg("check")
            .arg(&path)
            .output()
            .expect("run arch check");
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
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "integer reset for a float reg must be rejected"
    );
}

/// Operators outside the v1 float surface (`/ % << & ...`) are rejected, and
/// the diagnostic names the *actual* operator — never a `<op>` placeholder.
#[test]
fn fp_unsupported_operator_named_in_error() {
    for op in ["/", "%", "<<", "&"] {
        let src = format!(
            "module M\n  port a: in FP32;\n  port b: in FP32;\n  port o: out FP32;\n  comb o = a {op} b; end comb\nend module M\n"
        );
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("M.arch");
        std::fs::write(&path, &src).unwrap();
        let out = arch()
            .arg("check")
            .arg(&path)
            .output()
            .expect("run arch check");
        assert!(!out.status.success(), "float `{op}` must be rejected in v1");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            combined.contains(&format!("operator `{op}`")),
            "error for float `{op}` should name the operator, not a placeholder; got:\n{combined}"
        );
        assert!(
            !combined.contains("<op>"),
            "error must never contain the `<op>` placeholder; got:\n{combined}"
        );
    }
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
        .args([
            "--binary",
            "--timing",
            "-Wno-WIDTH",
            "-Wno-UNOPTFLAT",
            "-Wno-WIDTHTRUNC",
            "-Wno-WIDTHEXPAND",
            "-Wno-SHORTREAL",
            "-Wno-BLKANDNBLK",
            "-Wno-UNUSEDSIGNAL",
            "-Wno-MULTITOP",
            "--top-module",
            "tb",
            "-o",
            "sim_diff",
        ])
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

/// Pinned characterization test for `int.to_bf16()` on the built-SV backend
/// (issue #629, decided 2026-07-12) — the built-SV counterpart to
/// `fp_int_to_bf16_f32_routed_witness_sim`. Calls the emitted synthesizable
/// helpers directly (`arch_f32_to_bf16(arch_i64_to_f32(i))`, the exact
/// lowering `arch build` uses — see `src/codegen/mod.rs` `"to_bf16"` arm) and
/// locks the same f32-routed witness (`i=16842753` → `0x4b80`, not the
/// correctly-rounded `0x4b81`) plus the same exact case below `2^24`. Skips
/// cleanly when Verilator is not installed.
#[test]
fn fp_int_to_bf16_f32_routed_witness_verilator() {
    fn verilator_available() -> bool {
        std::process::Command::new("verilator")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !verilator_available() {
        eprintln!("skipping fp_int_to_bf16_f32_routed_witness_verilator: verilator not in PATH");
        return;
    }

    let manifest = env!("CARGO_MANIFEST_DIR");
    let td = tempfile::tempdir().expect("tempdir");
    let sv = td.path().join("FpArith.sv");

    // `arch build` emits the full FP helper block (arch_i64_to_f32,
    // arch_f32_to_bf16, ...) ahead of any module using FP; FpArith.arch pulls
    // in the whole package so we can call the helpers directly from the tb.
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
    let tb = format!("{manifest}/tests/fp_v1/rtl_diff/tb_int_to_bf16_witness.sv");
    let vout = std::process::Command::new("verilator")
        .args([
            "--binary",
            "--timing",
            "-Wno-WIDTH",
            "-Wno-UNOPTFLAT",
            "-Wno-WIDTHTRUNC",
            "-Wno-WIDTHEXPAND",
            "-Wno-SHORTREAL",
            "-Wno-BLKANDNBLK",
            "-Wno-UNUSEDSIGNAL",
            "-Wno-MULTITOP",
            "--top-module",
            "tb",
            "-o",
            "sim_int_to_bf16",
        ])
        .arg("-Mdir")
        .arg(&obj)
        .arg(&sv)
        .arg(&tb)
        .output()
        .expect("run verilator");
    assert!(
        vout.status.success(),
        "verilator build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&vout.stdout),
        String::from_utf8_lossy(&vout.stderr)
    );

    let run = std::process::Command::new(obj.join("sim_int_to_bf16"))
        .output()
        .expect("run verilated sim");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("ARCH_INT_TO_BF16_WITNESS: ALL PASS"),
        "int.to_bf16() f32-routed witness (built SV) failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// SMT equivalence proofs (doc/plan_fp_types.md §8.1). The proof model is
/// rendered from the SAME shared IR as the emitted SystemVerilog
/// (`arch::fp_smt_proof::equiv_proof` over `arch::fp_ops`), so the RTL and the
/// formally-checked model are one source — they cannot drift. Each generated
/// miter asserts the negation of equivalence to the IEEE-754 `FloatingPoint`
/// theory; z3 returning `unsat` proves the operator over its whole input space.
///
/// Covers FP32 comparisons, BF16 widen/narrow, and float->int (in-range). The
/// RNE arithmetic (`mul`/`add`/`sub`/`fma`) is generated identically but its
/// 2^64 miter is not solver-tractable, so it stays on the §8.2 differential
/// backstop. Emits a proof certificate. Skips cleanly when `z3` is absent.
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
        let o = std::process::Command::new("z3")
            .arg("--version")
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };

    let mut cert = String::new();
    cert.push_str("ARCH FP RTL — SMT equivalence proof certificate (plan §8.1)\n");
    cert.push_str(&format!("solver: {z3ver}\n"));
    cert.push_str(
        "property: emitted RTL ≡ SMT FloatingPoint theory (IEEE-754 RNE)\n\
         model: generated from the shared IR (src/fp_ops.rs) — same source as the SV\n\n",
    );

    let td = tempfile::tempdir().expect("tempdir");
    let ops: Vec<&str> = arch::fp_smt_proof::TRACTABLE
        .iter()
        .chain(arch::fp_smt_proof::BF16_CMP.iter())
        .copied()
        .collect();
    for op in ops {
        let smt = arch::fp_smt_proof::equiv_proof(op, arch::FpCompat::Riscv);
        let path = td.path().join(format!("{op}.smt2"));
        std::fs::write(&path, smt).unwrap();
        let out = std::process::Command::new("z3")
            .arg("-T:600")
            .arg(&path)
            .output()
            .unwrap_or_else(|e| panic!("failed to run z3 on {op}: {e}"));
        let res = String::from_utf8_lossy(&out.stdout);
        let first = res.lines().next().unwrap_or("").trim();
        cert.push_str(&format!("{op}: {first}\n"));
        assert_eq!(
            first,
            "unsat",
            "generated SMT proof {op} did not discharge as unsat (got {first:?})\nstderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    cert.push_str("result: ALL PROVED (unsat)\n");
    eprintln!("\n{cert}");
}

/// RNE arithmetic equivalence (doc/plan_fp_types.md §8.1), the slower miters.
///
/// - **f32 `add`/`sub`** are proved `unsat` vs `fp.add`/`fp.sub` over all 2^64
///   inputs (~80 s each). Tractable because the bounded adder keeps the datapath
///   ~56-bit (no multiplier) so the SAT instance stays small — the 280-bit
///   exact-wide version used to time out.
/// - **bf16 `mul`/`add`/`sub`** are proved `unsat` vs `fp.{mul,add,sub}` on
///   `(_ FloatingPoint 8 8)` (2^32) — the §8.1 primary target.
///
/// Not here: f32 `mul`/`fma` (24x24-multiplier equivalence is SAT-hard at 2^64,
/// z3 times out) and `bf16_fma` (correct, but its `fp.fma` miter trips a z3
/// 4.8.12 incompleteness — spurious `sat`). Both on the §8.2 backstop; see
/// fp_ops.rs. Slower (~minutes total); z3-gated.
#[test]
fn fp_smt_arith_proofs() {
    fn z3_available() -> bool {
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !z3_available() {
        eprintln!("skipping fp_smt_arith_proofs: z3 not in PATH");
        return;
    }
    let ops: Vec<&str> = arch::fp_smt_proof::F32_ADD
        .iter()
        .chain(arch::fp_smt_proof::BF16_ARITH.iter())
        .copied()
        .collect();
    let td = tempfile::tempdir().expect("tempdir");
    for op in ops {
        let smt = arch::fp_smt_proof::equiv_proof(op, arch::FpCompat::Riscv);
        let path = td.path().join(format!("{op}.smt2"));
        std::fs::write(&path, smt).unwrap();
        let out = std::process::Command::new("z3")
            .arg("-T:600")
            .arg(&path)
            .output()
            .unwrap();
        let first = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        eprintln!("arith proof {op}: {first}");
        assert_eq!(
            first, "unsat",
            "arith proof {op} did not discharge as unsat (got {first:?})"
        );
    }
}

/// `--fp-compat=cuda` (doc/plan_fp_types.md §6.2) selects the CUDA special-value
/// profile in the emitted SystemVerilog: canonical NaN 0x7FFFFFFF / 0x7FFF and
/// NaN->int = 0. The default `riscv` profile keeps 0x7FC00000 / 0x7FC0 and
/// NaN->type-max. The arithmetic datapath is identical across profiles.
#[test]
fn fp_compat_build_profiles() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let arch_src = format!("{manifest}/tests/fp_v1/FpArith.arch");

    // default = riscv
    let td = tempfile::tempdir().unwrap();
    let sv = td.path().join("d.sv");
    let out = arch()
        .arg("build")
        .arg(&arch_src)
        .arg("-o")
        .arg(&sv)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "default build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let d = std::fs::read_to_string(&sv).unwrap();
    assert!(
        d.contains("32'h7FC00000") && d.contains("16'h7FC0"),
        "riscv NaN constants missing"
    );
    assert!(
        !d.contains("32'h7FFFFFFF"),
        "default must not use the cuda NaN pattern"
    );

    // cuda
    let sv2 = td.path().join("c.sv");
    let out2 = arch()
        .arg("build")
        .arg(&arch_src)
        .arg("--fp-compat=cuda")
        .arg("-o")
        .arg(&sv2)
        .output()
        .unwrap();
    assert!(
        out2.status.success(),
        "cuda build failed: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
    let c = std::fs::read_to_string(&sv2).unwrap();
    assert!(
        c.contains("32'h7FFFFFFF") && c.contains("16'h7FFF"),
        "cuda NaN constants missing"
    );
    assert!(
        !c.contains("32'h7FC00000"),
        "cuda must not use the riscv NaN pattern"
    );
    // (NaN->int = 0 under cuda is checked behaviorally by fp_compat_sim_profiles)

    // invalid profile rejected
    let bad = arch()
        .arg("build")
        .arg(&arch_src)
        .arg("--fp-compat=nvidia")
        .arg("-o")
        .arg(td.path().join("x.sv"))
        .output()
        .unwrap();
    assert!(
        !bad.status.success(),
        "invalid --fp-compat must be rejected"
    );
    assert!(String::from_utf8_lossy(&bad.stderr).contains("expected `riscv` or `cuda`"));
}

/// The sim backend honors `--fp-compat` identically to the SV backend: a NaN
/// result and a NaN->int conversion follow the selected profile.
#[test]
fn fp_compat_sim_profiles() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let arch_src = format!("{manifest}/tests/fp_v1/NanProf.arch");
    let tb = format!("{manifest}/tests/fp_v1/tb_nanprof.cpp");

    let run = |extra: &[&str], dir: &str| -> String {
        let td = tempfile::tempdir().unwrap();
        let mut c = arch();
        c.arg("sim")
            .arg(&arch_src)
            .arg("--tb")
            .arg(&tb)
            .arg("--outdir")
            .arg(td.path().join(dir));
        for a in extra {
            c.arg(a);
        }
        let o = c.output().unwrap();
        assert!(
            o.status.success(),
            "sim failed: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).to_string()
    };

    let riscv = run(&[], "r");
    assert!(
        riscv.contains("nan_out=0x7FC00000 nan_to_int=2147483647"),
        "riscv profile wrong:\n{riscv}"
    );

    let cuda = run(&["--fp-compat=cuda"], "c");
    assert!(
        cuda.contains("nan_out=0x7FFFFFFF nan_to_int=0"),
        "cuda profile wrong:\n{cuda}"
    );
}

/// A bare float literal in a BF16 reset value or a typed-BF16 `let` is rounded
/// to bf16 **at compile time** and emitted as the exact 16-bit constant, not
/// as a 32-bit FP32 constant truncated into the 16-bit storage (arch#620).
///
/// Locked SV shape updated with the reset-slot unification (arch#622/#624,
/// maintainer-authorized): reset previously lowered through a runtime
/// `arch_f32_to_bf16(32'h3FC00000)` call (#623); it now folds to `16'h3FC0`
/// like init/let. For 1.5 (and every non-pathological literal) the resulting
/// bits are identical — only the emission shape changed.
#[test]
fn fp_bf16_literal_coerced_in_reset_and_let() {
    let src = "module Bf16Lit\n\
        \x20 port clk: in Clock<Sys>;\n\
        \x20 port rst: in Reset<Sync>;\n\
        \x20 port o_rst: out BF16;\n\
        \x20 port o_let: out BF16;\n\
        \x20 reg r: BF16 reset rst => 1.5;\n\
        \x20 let k: BF16 = 1.5;\n\
        \x20 seq on clk rising r <= r; end seq\n\
        \x20 comb o_rst = r; end comb\n\
        \x20 comb o_let = k; end comb\n\
        end module Bf16Lit\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("Bf16Lit.arch");
    std::fs::write(&path, src).unwrap();

    // `arch check` accepts the bare BF16 literals.
    let chk = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        chk.status.success(),
        "BF16 reset/let with a bare float literal should type-check\nstderr:\n{}",
        String::from_utf8_lossy(&chk.stderr),
    );

    // The emitted SV carries the compile-time-rounded 16-bit constant and
    // never assigns a 32-bit constant into the 16-bit reg/wire.
    let out = arch()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run arch build");
    assert!(out.status.success(), "arch build should succeed");
    let sv = std::fs::read_to_string(td.path().join("Bf16Lit.sv")).expect("read sv");
    assert!(
        sv.contains("r <= 16'h3FC0;"),
        "BF16 reset must fold to the exact 16-bit constant (reset unification, #622/#624), got:\n{sv}"
    );
    assert!(
        sv.contains("assign k = 16'h3FC0;"),
        "BF16 let must fold to the exact 16-bit constant, got:\n{sv}"
    );
    assert!(
        !sv.contains("32'h3FC00000"),
        "no 32-bit FP32 pattern of 1.5 should remain anywhere (the #620 truncation shape):\n{sv}"
    );
}

// ── arch#622 / arch#624: context-typed float literals ──────────────────────

/// `arch build`: a bare BF16-context float literal in `let`/`init`/comparison
/// slots emits the exact rounded width-correct constant directly — no
/// `arch_f32_to_bf16(...)` runtime helper call, no 32-bit constant anywhere
/// near the 16-bit storage (the arch#620/#624 truncation shape).
#[test]
fn fp_bf16_context_typed_literals_sv_shape() {
    let td = tempfile::tempdir().expect("tempdir");
    let src_path = std::path::Path::new("tests/fp_v1/Bf16LitCtx.arch");
    let sv_path = td.path().join("Bf16LitCtx.sv");
    let out = arch()
        .arg("build")
        .arg(src_path)
        .arg("--o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(&sv_path).expect("read sv");

    // init: bf16(1.5) = 0x3FC0, folded straight into the declaration
    // initializer (fixes the arch#624 "sim constructor can't fold to_bf16"
    // gap — this is the SV half of that fix).
    assert!(
        sv.contains("logic [15:0] r = 16'h3FC0;"),
        "BF16 `init` must emit the exact 16-bit constant, got:\n{sv}"
    );
    // let: bf16(pi) = 0x4049, bf16(0.1) = 0x3DCD (RNE, not truncation of
    // 0x3DCC).
    assert!(
        sv.contains("assign k = 16'h4049;"),
        "BF16 `let` must emit the exact 16-bit constant, got:\n{sv}"
    );
    assert!(
        sv.contains("assign k2 = 16'h3DCD;"),
        "BF16 `let` of 0.1 must round (RNE) to 0x3DCD, not truncate to 0x3DCC, got:\n{sv}"
    );
    // comparison: `a > 0.5` must call arch_bf16_gt with a 16-bit 0.5
    // constant (0x3F00), never the 32-bit FP32 pattern (0x3F000000), which
    // would be a width mismatch feeding a `uint16_t`-shaped SV helper arg.
    assert!(
        sv.contains("arch_bf16_gt(a, 16'h3F00)"),
        "BF16 comparison literal must be the 16-bit bf16 pattern, got:\n{sv}"
    );
    assert!(
        !sv.contains("32'h3F000000") && !sv.contains("32'h3FC00000") && !sv.contains("32'h4049"),
        "no 32-bit FP32 constant should appear in this BF16-only design, got:\n{sv}"
    );
    // Double-rounding witness (1 + 2^-8 + 2^-30): the SAME literal in the
    // reset, init, and let slots must fold to the SAME, correctly-rounded
    // 16-bit constant 0x3F81 (reset unification, arch#622/#624). The
    // superseded f32-routed reset path (#623) produced 0x3F80 here.
    assert!(
        sv.contains("rw_rst <= 16'h3F81;"),
        "witness reset must fold to the correctly-rounded 16'h3F81, got:\n{sv}"
    );
    assert!(
        sv.contains("logic [15:0] rw_init = 16'h3F81;"),
        "witness init must fold to the correctly-rounded 16'h3F81, got:\n{sv}"
    );
    assert!(
        sv.contains("assign kw = 16'h3F81;"),
        "witness let must fold to the correctly-rounded 16'h3F81, got:\n{sv}"
    );
    assert!(
        !sv.contains("16'h3F80"),
        "the f32-routed (double-rounded) witness value 0x3F80 must not appear:\n{sv}"
    );
}

/// `arch sim` end-to-end: the context-typed BF16 literals read back the
/// correctly-rounded bit patterns, and the comparison against a BF16 port
/// behaves correctly — sim and SV (previous test) agree on the same
/// constants.
#[test]
fn fp_bf16_context_typed_literals_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/Bf16LitCtx.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_bf16_lit_ctx.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("9 pass / 0 fail"),
        "BF16 context-typed literal sim should pass; got:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Standalone / ambiguous float literals are unaffected: they still default
/// to FP32 exactly as before context-typing landed.
#[test]
fn fp_standalone_literal_still_defaults_fp32() {
    let src = r#"module StandaloneLit
  port o: out FP32;
  let k: FP32 = 1.5;
  comb o = k; end comb
end module StandaloneLit
"#;
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("StandaloneLit.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run arch build");
    assert!(out.status.success(), "arch build should succeed");
    let sv = std::fs::read_to_string(td.path().join("StandaloneLit.sv")).expect("read sv");
    assert!(
        sv.contains("32'h3FC00000"),
        "standalone FP32 literal should still emit the 32-bit FP32 pattern, got:\n{sv}"
    );
}

/// An integer literal in a known-BF16 `let` slot is rejected (never silently
/// accepted-and-miscompiled), consistent with the existing `reset` rule.
#[test]
fn fp_bf16_let_integer_literal_rejected() {
    let src = r#"module BadLetInt
  port o: out BF16;
  let k: BF16 = 1;
  comb o = k; end comb
end module BadLetInt
"#;
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("BadLetInt.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "integer literal in a BF16 `let` slot must be rejected"
    );
}

/// An integer literal in a BF16 `reg init` slot is rejected with a clear
/// message pointing at the float spelling (arch#624 acceptance criterion:
/// "decide reject-vs-accept consistently with the reset rule" — reject).
#[test]
fn fp_bf16_init_integer_literal_rejected() {
    let src = r#"module BadInitInt
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync>;
  port o: out BF16;
  reg r: BF16 init 1;
  seq on clk rising
    r <= r;
  end seq
  comb o = r; end comb
end module BadInitInt
"#;
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("BadInitInt.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "integer literal in a BF16 `reg init` slot must be rejected"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("float literal") && stderr.contains("integer literal"),
        "error should point at the float-literal-required rule; got:\n{stderr}"
    );
}
