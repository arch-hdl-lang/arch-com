//! FP32 / BF16 floating-point v1 integration tests.
//!
//! Covers the front-end (parse + type check), the SoftFloat-semantics
//! simulation backend (host IEEE-754 RNE), and the SystemVerilog emission
//! shape. See doc/archive/plan_fp_types.md.

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

/// v2: floats are accepted in Vec elements, struct fields, and
/// module-local function signatures — each position must CHECK clean and
/// dispatch float ops (locked end-to-end by fp_composite_sim /
/// fp_composite_verilator). Mixing float formats through a composite
/// access is still rejected (no implicit conversion), as is an integer
/// literal in a Vec-of-float reset slot.
#[test]
fn fp_composite_positions_accepted_and_guarded() {
    let ok_cases = [
        ("vec", "module M\n  port a: in Vec<FP32, 4>;\n  port o: out FP32;\n  comb o = a[0] + a[1]; end comb\nend module M\n"),
        ("struct", "struct P\n  x: FP32;\nend struct P\nmodule M\n  port p_x: in FP32;\n  port o: out FP32;\n  wire p: P;\n  comb p.x = p_x; end comb\n  comb o = p.x + p.x; end comb\nend module M\n"),
        ("function", "module M\n  function f(x: FP32) -> FP32\n    let d: FP32 = x + x;\n    return d;\n  end function f\n  port a: in FP32;\n  port o: out FP32;\n  comb o = f(a); end comb\nend module M\n"),
    ];
    for (label, src) in ok_cases {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("M.arch");
        std::fs::write(&path, src).unwrap();
        let out = arch()
            .arg("check")
            .arg(&path)
            .output()
            .expect("run arch check");
        assert!(
            out.status.success(),
            "float in {label} position must be accepted in v2\nstderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let bad_cases = [
        (
            "vec elem format mix",
            "module M\n  port a: in Vec<FP32, 4>;\n  port h: in BF16;\n  port o: out FP32;\n  comb o = a[0] + h; end comb\nend module M\n",
        ),
        (
            "vec float int reset",
            "module M\n  port clk: in Clock<S>;\n  port rst: in Reset<Sync>;\n  port o: out FP32;\n  reg r: Vec<FP32, 2> reset rst => 1;\n  seq on clk rising\n    r[0] <= r[0];\n    r[1] <= r[1];\n  end seq\n  comb o = r[0]; end comb\nend module M\n",
        ),
        (
            "fp8 reg int reset",
            "module M\n  port clk: in Clock<S>;\n  port rst: in Reset<Sync>;\n  port o: out FP8E4M3;\n  reg r: FP8E4M3 reset rst => 1;\n  seq on clk rising\n    r <= r;\n  end seq\n  comb o = r; end comb\nend module M\n",
        ),
    ];
    for (label, src) in bad_cases {
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
            "{label} must be rejected\nsrc:\n{src}"
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

/// Differential equivalence (doc/archive/plan_fp_types.md §8.2): the emitted
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

/// SMT equivalence proofs (doc/archive/plan_fp_types.md §8.1). The proof model is
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

/// RNE arithmetic equivalence (doc/archive/plan_fp_types.md §8.1), the slower miters.
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

/// `--fp-compat=cuda` (doc/archive/plan_fp_types.md §6.2) selects the CUDA special-value
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

// ── FP8 (E4M3 OCP OFP8 + E5M2) ──────────────────────────────────────────────

/// `arch check` accepts the full FP8 surface (arith fixture, profile probe,
/// context-typed literals).
#[test]
fn fp8_check_passes() {
    for f in ["Fp8Arith", "Fp8Prof", "Fp8LitCtx"] {
        let out = arch()
            .arg("check")
            .arg(format!("tests/fp_v1/{f}.arch"))
            .output()
            .expect("run arch check");
        assert!(
            out.status.success(),
            "arch check should pass for {f}.arch\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

/// End-to-end simulation: FP8 arithmetic, fma, is_nan, OCP top-binade
/// decoding, subnormals, and RNE narrowing (incl. the 448/464 boundary) all
/// match hand-computed values.
#[test]
fn fp8_sim_matches_reference() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/Fp8Arith.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_fp8.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for Fp8Arith\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("25 pass / 0 fail"),
        "expected all 25 FP8 checks to pass; got:\n{stdout}"
    );
}

/// Context-typed FP8 literals: reg init/reset, typed let, and compares
/// resolve bare float literals to fp8 at compile time (single RNE step).
#[test]
fn fp8_lit_ctx_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/Fp8LitCtx.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_fp8_lit_ctx.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("6 pass / 0 fail"),
        "expected all 6 FP8 literal-context checks to pass; got:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// The --fp-compat profile surface for FP8 narrowing: riscv is
/// non-saturating (E5M2 -> ±inf, E4M3 -> NaN with the sign dropped — OCP has
/// no infinities), cuda saturates both formats to ±max-finite (PTX
/// satfinite), including ±inf inputs. Canonical NaNs: E4M3 0x7F always;
/// E5M2 0x7E riscv / 0x7F cuda.
#[test]
fn fp8_compat_sim_profiles() {
    let run = |extra: &[&str], sub: &str| -> String {
        let td = tempfile::tempdir().expect("tempdir");
        let mut cmd = arch();
        cmd.arg("sim")
            .arg("tests/fp_v1/Fp8Prof.arch")
            .arg("--tb")
            .arg("tests/fp_v1/tb_fp8_prof.cpp")
            .arg("--outdir")
            .arg(td.path().join(sub));
        for a in extra {
            cmd.arg(a);
        }
        let out = cmd.output().expect("run arch sim");
        assert!(
            out.status.success(),
            "sim failed ({extra:?}):\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let riscv = run(&[], "r");
    for expect in [
        "povf4=0x7F povf5=0x7C",
        "novf4=0x7F novf5=0xFC",
        "pinf4=0x7F pinf5=0x7C",
        "ninf4=0x7F ninf5=0xFC",
        "nan4=0x7F nan5=0x7E",
        "b480_4=0x7F",
        "max5=0x7B",
        "tie5=0x7C",
    ] {
        assert!(
            riscv.contains(expect),
            "riscv profile wrong: missing `{expect}`\n{riscv}"
        );
    }
    let cuda = run(&["--fp-compat=cuda"], "c");
    for expect in [
        "povf4=0x7E povf5=0x7B",
        "novf4=0xFE novf5=0xFB",
        "pinf4=0x7E pinf5=0x7B",
        "ninf4=0xFE ninf5=0xFB",
        "nan4=0x7F nan5=0x7F",
        "b480_4=0x7E",
        "max5=0x7B",
        "tie5=0x7B",
    ] {
        assert!(
            cuda.contains(expect),
            "cuda profile wrong: missing `{expect}`\n{cuda}"
        );
    }
}

/// `arch build` emits the fp8 helper functions and dispatches fp8 ops/
/// conversions to them.
#[test]
fn fp8_build_emits_helpers_and_dispatch() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("Fp8Arith.arch");
    std::fs::copy("tests/fp_v1/Fp8Arith.arch", &arch_path).expect("copy arch into tempdir");
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
    let sv = std::fs::read_to_string(td.path().join("Fp8Arith.sv")).expect("read Fp8Arith.sv");
    for helper in [
        "function automatic logic [7:0] arch_e4m3_add",
        "function automatic logic [7:0] arch_e5m2_add",
        "function automatic logic [31:0] arch_e4m3_to_f32",
        "function automatic logic [7:0] arch_f32_to_e5m2",
        "function automatic logic [7:0] arch_fma_e4m3",
    ] {
        assert!(sv.contains(helper), "helper `{helper}` missing from SV");
    }
    for dispatch in [
        "arch_e4m3_add(a4, b4)",
        "arch_fma_e4m3(a4, b4, c4)",
        "arch_e5m2_to_f32(a5)",
        "arch_f32_to_e4m3(f)",
    ] {
        assert!(
            sv.contains(dispatch),
            "dispatch `{dispatch}` missing from SV:\n{sv}"
        );
    }
    // E4M3 is_nan is the sole OCP encoding, not an exponent-class test.
    assert!(
        sv.contains("a4[6:0] == 7'h7F"),
        "E4M3 is_nan should test the sole NaN encoding"
    );
}

/// An fp8 literal that overflows the format is a compile error (not a silent
/// saturation) — the profile-dependent overflow rules apply only at runtime.
#[test]
fn fp8_literal_overflow_rejected() {
    for (ty, lit, max) in [("FP8E4M3", "500.0", "448"), ("FP8E5M2", "70000.0", "57344")] {
        let src = format!(
            "module BadOvf\n  port o: out {ty};\n  let x: {ty} = {lit};\n  comb o = x; end comb\nend module BadOvf\n"
        );
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("BadOvf.arch");
        std::fs::write(&path, src).unwrap();
        let out = arch()
            .arg("check")
            .arg(&path)
            .output()
            .expect("run arch check");
        assert!(!out.status.success(), "{ty} literal {lit} must be rejected");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("overflows") && stderr.contains(max),
            "error should name the overflow and the max finite value {max}; got:\n{stderr}"
        );
    }
    // The OCP boundary literal 464 ties DOWN to 448 and is accepted.
    let src = "module OkTie\n  port o: out FP8E4M3;\n  let x: FP8E4M3 = 464.0;\n  comb o = x; end comb\nend module OkTie\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("OkTie.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "E4M3 literal 464 ties to 448 and must be accepted:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// No implicit conversions between float formats — mixing them in an
/// operator stays a type error; the conversion surface (now total: any
/// float/int source) requires explicit method calls.
#[test]
fn fp8_no_implicit_mixing() {
    let src = "module BadMix\n  port a: in FP8E4M3;\n  port b: in FP8E5M2;\n  port o: out FP8E4M3;\n  comb o = a + b; end comb\nend module BadMix\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("BadMix.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "mixing fp8 formats must be a type error"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FP8E4M3") && stderr.contains("FP8E5M2"),
        "error should name both formats; got:\n{stderr}"
    );
}

/// The fp8 conversion matrix (v2): fp8<->bf16, fp8<->int, cross-fp8 — all
/// f32-routed compositions of the proven helpers, each exact or singly
/// rounded (documented CR argument in spec §3.8). Exact-value TB runs on
/// the native sim; saturation/NaN/overflow corners included (riscv).
#[test]
fn fp8_conversion_matrix_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/Fp8Convert.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_fp8_convert.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("16 pass / 0 fail"),
        "fp8 conversion matrix failed:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// FP8 SMT equivalence proofs, BOTH --fp-compat profiles. E5M2 is checked
/// against the `(_ FloatingPoint 5 3)` theory (add/sub via the exact-wide
/// (8,53) formulation — see fp_smt_proof.rs); E4M3 is OCP OFP8, checked
/// against a hand-written two-region spec that the `e4m3_widen` miter
/// grounds against the IR. `unsat` across FP8_ARITH proves every fp8
/// binary op correctly rounded per its format under both profiles. Skips
/// cleanly when `z3` is absent. The fma is NOT here — it is fused
/// f32-accumulate by design, characterized exhaustively instead (see
/// examples/fp8_fma_char.rs; E4M3 measured 0/2^24 mismatches, E5M2
/// 18960/2^24 riscv, 15888/2^24 cuda).
#[test]
fn fp8_smt_proofs() {
    fn z3_available() -> bool {
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !z3_available() {
        eprintln!("skipping fp8_smt_proofs: z3 not in PATH");
        return;
    }
    let ops: Vec<&str> = arch::fp_smt_proof::FP8_CMP
        .iter()
        .chain(arch::fp_smt_proof::FP8_CONV.iter())
        .chain(arch::fp_smt_proof::FP8_ARITH.iter())
        // e4m3_fma_cr: the fused f32-accumulate fma proved correctly rounded
        // for E4M3 (~2 min/profile in z3). e5m2_fma_cr is expected-sat and
        // stays out (see FP8_FMA_CR docs).
        .chain(std::iter::once(&"e4m3_fma_cr"))
        .copied()
        .collect();
    let td = tempfile::tempdir().expect("tempdir");
    for profile in [arch::FpCompat::Riscv, arch::FpCompat::Cuda] {
        for op in &ops {
            let smt = arch::fp_smt_proof::equiv_proof(op, profile);
            let path = td.path().join(format!("{op}_{profile:?}.smt2"));
            std::fs::write(&path, smt).unwrap();
            let out = std::process::Command::new("z3")
                .arg("-T:900")
                .arg(&path)
                .output()
                .unwrap_or_else(|e| panic!("failed to run z3 on {op}: {e}"));
            let first = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            assert_eq!(
                first, "unsat",
                "fp8 SMT proof {op} ({profile:?}) did not discharge (got {first:?})"
            );
        }
    }
}

/// OCP MX sub-8-bit storage-format SMT proofs (FP4 E2M1, FP6 E2M3 / E3M2),
/// BOTH `--fp-compat` profiles.
///
/// These formats are all-finite — no Inf, no NaN — so there is no SMT sort for
/// them and each conversion is checked against a hand-written spec (see
/// `fp_smt_proof::MX_CONV`). Two things make this cheap and strong:
///
/// - the widen miters are **exhaustive over every encoding** (2^4 for FP4, 2^6
///   for FP6), and they *ground* the narrow miters, which decode their result
///   through the same widen;
/// - the narrow miters are **exhaustive over all 2^32 FP32 inputs**, and z3
///   discharges each in well under a second.
///
/// Running both profiles is not redundant here. Both `f32_to_e2m1` and
/// `f32_to_fp6` claim the profiles *cannot* differ — with no Inf and no NaN in
/// the encoding space there is nowhere for an overflow to go but the max
/// finite. These miters are what pins that claim: the same saturating spec is
/// asserted under each profile.
///
/// Mutation-tested when written (2026-08-10, z3 4.15.4): corrupting a
/// top-binade constant, the overflow threshold, the subnormal grid spacing, or
/// either rounding mode flips every affected miter to `sat`. Moving the
/// subnormal/normal split point to `2 * min_normal` does NOT — that is an
/// equivalent mutant, and a deliberate one: the fixed subnormal grid has the
/// same spacing as the min-normal binade, so any split inside
/// `[min_normal, 2*min_normal)` is a correct spec. Pushing it to
/// `4 * min_normal` leaves that window and is killed on all three formats.
#[test]
fn mx_storage_smt_proofs() {
    fn z3_available() -> bool {
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !z3_available() {
        eprintln!("skipping mx_storage_smt_proofs: z3 not in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    for profile in [arch::FpCompat::Riscv, arch::FpCompat::Cuda] {
        for op in arch::fp_smt_proof::MX_CONV {
            let smt = arch::fp_smt_proof::equiv_proof(op, profile);
            let path = td.path().join(format!("{op}_{profile:?}.smt2"));
            std::fs::write(&path, smt).unwrap();
            let out = std::process::Command::new("z3")
                .arg("-T:900")
                .arg(&path)
                .output()
                .unwrap_or_else(|e| panic!("failed to run z3 on {op}: {e}"));
            let first = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            assert_eq!(
                first, "unsat",
                "MX storage SMT proof {op} ({profile:?}) did not discharge (got {first:?})"
            );
        }
    }
}

/// FP8 SV-vs-sim cross-oracle sweep, both profiles. One TB source
/// (tb_fp8_sweep.cpp) runs against BOTH backends — the native sim's
/// hand-written C++ helpers and the IR-rendered synthesizable SV under
/// Verilator — dumping every output over an exhaustive 2^16 binary-op sweep
/// plus a 3*2^25 stratified narrowing sweep (every sign/exponent/high-
/// mantissa pattern with three low-bit sticky variants). The dumps must be
/// byte-identical: the two implementations are independent, so agreement
/// over the full space pins the sim helpers to the proven RTL. (Full 2^32
/// narrow sweep: ARCH_FP8_SWEEP_FULL=1, long phase.) Skips without
/// Verilator.
#[test]
fn fp8_sv_vs_sim_sweep() {
    fn verilator_available() -> bool {
        std::process::Command::new("verilator")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !verilator_available() {
        eprintln!("skipping fp8_sv_vs_sim_sweep: verilator not in PATH");
        return;
    }
    let manifest = env!("CARGO_MANIFEST_DIR");
    for profile in ["riscv", "cuda"] {
        let td = tempfile::tempdir().expect("tempdir");
        let sim_run = td.path().join("sim_run");
        let sv_run = td.path().join("sv_run");
        std::fs::create_dir_all(&sim_run).unwrap();
        std::fs::create_dir_all(&sv_run).unwrap();

        // Native sim dump.
        let out = arch()
            .arg("sim")
            .arg(format!("{manifest}/tests/fp_v1/Fp8Arith.arch"))
            .arg("--tb")
            .arg(format!("{manifest}/tests/fp_v1/tb_fp8_sweep.cpp"))
            .arg(format!("--fp-compat={profile}"))
            .arg("--outdir")
            .arg(td.path().join("sim_build"))
            .current_dir(&sim_run)
            .output()
            .expect("run arch sim");
        assert!(
            out.status.success()
                && String::from_utf8_lossy(&out.stdout).contains("ARCH_FP8_SWEEP: DONE"),
            "native sim sweep failed ({profile}):\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        // Built-SV dump under Verilator.
        let sv = td.path().join("Fp8Arith.sv");
        let out = arch()
            .arg("build")
            .arg(format!("{manifest}/tests/fp_v1/Fp8Arith.arch"))
            .arg(format!("--fp-compat={profile}"))
            .arg("-o")
            .arg(&sv)
            .output()
            .expect("run arch build");
        assert!(out.status.success(), "arch build failed ({profile})");
        let obj = td.path().join("obj");
        let vout = std::process::Command::new("verilator")
            .args([
                "--cc",
                "--exe",
                "--build",
                "-Wno-fatal",
                "--top-module",
                "Fp8Arith",
                "-o",
                "vsweep",
            ])
            .arg("-Mdir")
            .arg(&obj)
            .arg(&sv)
            .arg(format!("{manifest}/tests/fp_v1/tb_fp8_sweep.cpp"))
            .output()
            .expect("run verilator");
        assert!(
            vout.status.success(),
            "verilator build failed ({profile}):\n{}",
            String::from_utf8_lossy(&vout.stderr)
        );
        let run = std::process::Command::new(obj.join("vsweep"))
            .current_dir(&sv_run)
            .output()
            .expect("run verilated sweep");
        assert!(
            run.status.success()
                && String::from_utf8_lossy(&run.stdout).contains("ARCH_FP8_SWEEP: DONE"),
            "verilated sweep failed ({profile})"
        );

        let a = std::fs::read(sim_run.join("fp8_sweep.bin")).expect("sim dump");
        let b = std::fs::read(sv_run.join("fp8_sweep.bin")).expect("sv dump");
        assert_eq!(a.len(), b.len(), "dump size mismatch ({profile})");
        if a != b {
            let i = a.iter().zip(&b).position(|(x, y)| x != y).unwrap();
            panic!(
                "fp8 SV-vs-sim divergence ({profile}) at byte {i}: sim=0x{:02X} sv=0x{:02X}",
                a[i], b[i]
            );
        }
    }
}

/// FP v2 composite positions end-to-end (native sim): Vec<FP32> elementwise
/// ops + reg-of-Vec accumulate, BF16 struct fields (writes + reads + a
/// coerced field literal), an FP8 module-local function, and a Vec-element
/// compare against a coerced literal. Exact-value checks: any dispatch
/// regression to integer arithmetic trips loudly.
#[test]
fn fp_composite_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/fp_v1/FpComposite.arch")
        .arg("--tb")
        .arg("tests/fp_v1/tb_fp_composite.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("6 pass / 0 fail"),
        "composite float sim failed:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The built SV dispatches composite float accesses to the helper
/// functions (the pre-v2 hazard was silent integer ops on Vec elements /
/// struct fields / function params).
#[test]
fn fp_composite_build_dispatch() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("FpComposite.arch");
    std::fs::copy("tests/fp_v1/FpComposite.arch", &arch_path).unwrap();
    let out = arch()
        .arg("build")
        .arg(&arch_path)
        .output()
        .expect("run arch build");
    assert!(out.status.success(), "arch build failed");
    let sv = std::fs::read_to_string(td.path().join("FpComposite.sv")).unwrap();
    for needle in [
        "arch_f32_mul(v[i], s)",           // Vec element dispatch
        "arch_f32_add(acc[0], v[0])",      // Vec reg seq dispatch
        "arch_bf16_mul(a.re, b.re)",       // struct field dispatch
        "arch_bf16_add(b.re, 16'h3F00)",   // coerced field literal
        "arch_e4m3_add(x, x)",             // fp8 inside a function body
        "arch_f32_gt(v[3], 32'h40200000)", // Vec element compare + literal
    ] {
        assert!(
            sv.contains(needle),
            "missing dispatch `{needle}` in SV:\n{sv}"
        );
    }
}

/// Float-typed params for every format: a bare literal default is
/// context-typed to the declared param format (FP32 was always fine; BF16/
/// fp8 defaults previously stayed FP32-typed and mismatched on use), the
/// SV emits the rounded bit pattern, and expressions dispatch float ops.
#[test]
fn fp_float_typed_params_all_formats() {
    let src = "module ParamF\n  param GAIN: FP32 = 2.5;\n  param HBIAS: BF16 = 0.5;\n  param Q: FP8E4M3 = 1.5;\n  port a: in FP32;\n  port h: in BF16;\n  port q: in FP8E4M3;\n  port y: out FP32;\n  port yh: out BF16;\n  port yq: out FP8E4M3;\n  comb y = a * GAIN; end comb\n  comb yh = h + HBIAS; end comb\n  comb yq = q + Q; end comb\nend module ParamF\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("ParamF.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "float-typed params must build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(td.path().join("ParamF.sv")).unwrap();
    for needle in [
        "parameter [31:0] GAIN = 32'h40200000",
        "parameter [15:0] HBIAS = 16'h3F00",
        "parameter [7:0] Q = 8'h3C",
        "arch_bf16_add(h, HBIAS)",
        "arch_e4m3_add(q, Q)",
    ] {
        assert!(sv.contains(needle), "missing `{needle}` in SV:\n{sv}");
    }
}

/// Float dispatch inside the non-module constructs — fsm, pipeline
/// (incl. cross-stage reads), bus float fields, and thread — on the native
/// sim, with exact expected values. Pre-fix, fsm-sim / pipeline-both /
/// bus-both silently emitted integer ops on the float bit patterns
/// (2026-08-02 audit); thread was already correct and is locked here.
#[test]
fn fp_construct_positions_sim() {
    let cases: [(&[&str], &str, &str); 5] = [
        (&["FpFsm.arch"], "tb_fp_fsm.cpp", "FP_FSM: PASS"),
        (&["FpPipe.arch"], "tb_fp_pipe.cpp", "FP_PIPE: PASS"),
        (
            &["FpStreamBus.arch", "FpBusMod.arch"],
            "tb_fp_bus.cpp",
            "FP_BUS: PASS",
        ),
        (&["FpThread.arch"], "tb_fp_thread.cpp", "FP_THREAD: PASS"),
        (&["FpTlm.arch"], "tb_fp_tlm.cpp", "FP_TLM: PASS"),
    ];
    for (files, tb, expect) in cases {
        let td = tempfile::tempdir().expect("tempdir");
        let mut cmd = arch();
        cmd.arg("sim");
        for f in files {
            cmd.arg(format!("tests/fp_v1/{f}"));
        }
        let out = cmd
            .arg("--tb")
            .arg(format!("tests/fp_v1/{tb}"))
            .arg("--outdir")
            .arg(td.path())
            .output()
            .expect("run arch sim");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains(expect),
            "{tb}: expected `{expect}`\nstdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// The built SV dispatches float ops in fsm / pipeline / bus contexts.
#[test]
fn fp_construct_positions_build_dispatch() {
    let td = tempfile::tempdir().expect("tempdir");
    for (files, top, needles) in [
        (vec!["FpFsm.arch"], "FpFsm", vec!["arch_f32_add(total, x)"]),
        (
            vec!["FpPipe.arch"],
            "FpPipe",
            vec!["arch_f32_mul(a_in, b_in)", "arch_f32_add(mul_p, b_in)"],
        ),
        (
            vec!["FpStreamBus.arch", "FpBusMod.arch"],
            "FpBusMod",
            vec!["arch_f32_add(s_data, s_data)"],
        ),
    ] {
        let sv = td.path().join(format!("{top}.sv"));
        let mut cmd = arch();
        cmd.arg("build");
        for f in &files {
            cmd.arg(format!("tests/fp_v1/{f}"));
        }
        let out = cmd.arg("-o").arg(&sv).output().expect("run arch build");
        assert!(out.status.success(), "{top} build failed");
        let text = std::fs::read_to_string(&sv).unwrap();
        for n in needles {
            assert!(text.contains(n), "{top}: missing `{n}`:\n{text}");
        }
    }
}

/// Direct-assignment literal slot: a bare float literal (or ternary arm)
/// assigned to a narrow-float target context-types to the TARGET's format —
/// `h <= 0.5`, `v[i] <= 1.5`, `s.f = 0.75`, `x <= c ? 2.5 : 0.25` — the
/// last literal-slot family (let/init/reset/default/compare/binop already
/// coerced). Overflowing literals stay compile errors in this slot too.
#[test]
fn fp_assign_literal_coercion() {
    let src = "struct Duo\n  lo: BF16;\n  hi: FP8E4M3;\nend struct Duo\nmodule AsgLit\n  port clk: in Clock<Sys>;\n  port rst: in Reset<Sync>;\n  port sel: in Bool;\n  port o: out BF16;\n  port o2: out FP8E4M3;\n  port o3: out BF16;\n  reg h: BF16 reset rst => 0.0;\n  reg v: Vec<FP8E4M3, 2> reset rst => 0.0;\n  wire s: Duo;\n  seq on clk rising\n    h <= 0.5;\n    v[0] <= 1.5;\n    v[1] <= sel ? 2.5 : 0.25;\n  end seq\n  comb\n    s.lo = 0.75;\n    s.hi = 3.0;\n  end comb\n  comb o = h; end comb\n  comb o2 = v[0]; end comb\n  comb o3 = s.lo; end comb\nend module AsgLit\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("AsgLit.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "assign-literal module must build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(td.path().join("AsgLit.sv")).unwrap();
    for needle in [
        "h <= 16'h3F00",               // bf16(0.5) scalar reg
        "v[0] <= 8'h3C",               // e4m3(1.5) Vec element
        "v[1] <= sel ? 8'h42 : 8'h28", // ternary arms e4m3(2.5)/(0.25)
        "s.lo = 16'h3F40",             // bf16(0.75) struct field (comb)
        "s.hi = 8'h44",                // e4m3(3.0) struct field (comb)
    ] {
        assert!(sv.contains(needle), "missing `{needle}` in SV:\n{sv}");
    }
    // Overflow guard still fires in the assignment slot.
    let bad = "module AsgOvf\n  port clk: in Clock<Sys>;\n  port rst: in Reset<Sync>;\n  port o: out FP8E4M3;\n  reg r: FP8E4M3 reset rst => 0.0;\n  seq on clk rising\n    r <= 500.0;\n  end seq\n  comb o = r; end comb\nend module AsgOvf\n";
    let bpath = td.path().join("AsgOvf.arch");
    std::fs::write(&bpath, bad).unwrap();
    let out = arch()
        .arg("check")
        .arg(&bpath)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "overflowing assign literal must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("overflows"),
        "error should name the overflow"
    );
}

/// Float properties in `arch formal` (v2): floats ride as BV carriers and
/// operators dispatch to the same machine-proven QF_BV define-funs the SV
/// and offline SMT proofs are rendered from — user properties compose over
/// proven operators, no solver FP theory involved. Covers fp8+bf16 ops,
/// compares, fma, is_nan, an exact-widen conversion, and a float reg reset.
/// The Bad fixture locks refutation + float counterexamples. z3-gated.
#[test]
fn fp_formal_float_props() {
    fn z3_available() -> bool {
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !z3_available() {
        eprintln!("skipping fp_formal_float_props: z3 not in PATH");
        return;
    }
    let out = arch()
        .arg("formal")
        .arg("tests/fp_v1/FpFormalProps.arch")
        .arg("--solver")
        .arg("z3")
        .arg("--bound")
        .arg("3")
        .output()
        .expect("run arch formal");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "float formal props must all prove:\n{all}"
    );
    assert_eq!(
        all.matches("PROVED").count(),
        6,
        "expected 6 PROVED properties:\n{all}"
    );

    let out = arch()
        .arg("formal")
        .arg("tests/fp_v1/FpFormalBad.arch")
        .arg("--solver")
        .arg("z3")
        .arg("--bound")
        .arg("2")
        .output()
        .expect("run arch formal");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success() && all.contains("REFUTED"),
        "false float property must refute with a counterexample:\n{all}"
    );
}

/// Literals context-type against CONVERSION-RESULT operands too:
/// `a.to_bf16() > 1.0` and `h.to_fp8e5m2() + 0.5` coerce the literal to
/// the conversion's target format (closes the last literal-slot gap —
/// previously these needed a typed intermediate wire).
#[test]
fn fp_literal_coerces_against_conversion_result() {
    let src = "module LitConv\n  port a: in FP8E4M3;\n  port h: in BF16;\n  port o1: out Bool;\n  port o2: out FP8E5M2;\n  comb o1 = a.to_bf16() > 1.0; end comb\n  comb o2 = h.to_fp8e5m2() + 0.5; end comb\nend module LitConv\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("LitConv.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "literal-vs-conversion must build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(td.path().join("LitConv.sv")).unwrap();
    for needle in [
        "arch_bf16_gt(arch_f32_to_bf16(arch_e4m3_to_f32(a)), 16'h3F80)",
        "arch_e5m2_add(arch_f32_to_e5m2(arch_bf16_to_f32(h)), 8'h38)",
    ] {
        assert!(sv.contains(needle), "missing `{needle}`:\n{sv}");
    }
}

/// `assume` + `assert<bound_err>` numeric error-bound properties. The
/// error engine (gappa) proves absolute / relative / ULP bounds over
/// range-constrained comb float cones — modeling the RTL faithfully
/// (incl. the VR(f32) double-rounded narrow ops) against the real-valued
/// spec — and `assume` also constrains the QF_BV solver path. Gated on
/// BOTH z3 and gappa.
#[test]
fn fp_bound_err_props() {
    fn have(bin: &str) -> bool {
        std::process::Command::new("which")
            .arg(bin)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    let gappa_ok = have("gappa")
        || std::env::var_os("HOME")
            .map(|h| std::path::Path::new(&h).join("bin/gappa").exists())
            .unwrap_or(false);
    if !have("z3") || !gappa_ok {
        eprintln!("skipping fp_bound_err_props: z3/gappa not available");
        return;
    }
    let out = arch()
        .arg("formal")
        .arg("tests/fp_v1/FpBoundErr.arch")
        .arg("--solver")
        .arg("z3")
        .arg("--bound")
        .arg("2")
        .output()
        .expect("run arch formal");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success() && all.matches("PROVED").count() == 4,
        "expected 4 PROVED (3 bounds + 1 assumed BV prop):\n{all}"
    );
    assert!(
        all.contains("derived"),
        "proved bounds should report the derived enclosure:\n{all}"
    );

    // Honesty: over [-1,1] the dot product cancels — the ULP-relative goal
    // must come back INCONCLUSIVE (exit 2), never a false proof.
    let out = arch()
        .arg("formal")
        .arg("tests/fp_v1/FpBoundErrCancel.arch")
        .arg("--solver")
        .arg("z3")
        .arg("--bound")
        .arg("2")
        .output()
        .expect("run arch formal");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "cancelling ULP goal must be inconclusive:\n{all}"
    );
    assert!(
        all.matches("PROVED").count() == 1 && all.contains("INCONCLUSIVE"),
        "abs bound proves, ulp bound honestly refused:\n{all}"
    );
}

/// The spec builtins are fenced: exact()/abs()/ulp() outside an
/// `assert<bound_err>` property is a type error.
#[test]
fn fp_bound_err_builtins_fenced() {
    let src = "module Fence\n  port clk: in Clock<Sys>;\n  port rst: in Reset<Sync>;\n  port a: in FP32;\n  port o: out FP32;\n  comb o = exact(a); end comb\nend module Fence\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("Fence.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    assert!(
        !out.status.success(),
        "exact() outside bound_err must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("spec builtin"),
        "error should say it's a spec builtin"
    );
}

/// Negative float literals context-type in narrow slots: `-1.5` parses as
/// Neg(Float) and previously never coerced anywhere (typed let, reset,
/// compares, range assumes all errored with "expected BF16, found FP32").
#[test]
fn fp_negative_literal_coercion() {
    let src = "module NegLit\n  port clk: in Clock<Sys>;\n  port rst: in Reset<Sync>;\n  port a: in FP8E4M3;\n  port o: out BF16;\n  port g: out Bool;\n  let x: BF16 = -1.5;\n  reg r: BF16 reset rst => -0.5;\n  seq on clk rising\n    r <= r;\n  end seq\n  comb o = x + r; end comb\n  comb g = a >= -1.0; end comb\nend module NegLit\n";
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("NegLit.arch");
    std::fs::write(&path, src).unwrap();
    let out = arch()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "negative narrow literals must coerce:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(td.path().join("NegLit.sv")).unwrap();
    for needle in ["16'hBFC0", "16'hBF00", "arch_e4m3_ge(a, 8'hB8)"] {
        assert!(sv.contains(needle), "missing `{needle}`:\n{sv}");
    }
}

/// Certified no-saturation: with range assumes the E4M3 narrowing property
/// `!is_nan(y_w)` PROVES (overflow unreachable — riscv maps overflow to
/// NaN); with the assumes stripped it REFUTES. Locks the assume-constrained
/// overflow-certification pattern end-to-end. z3-gated.
#[test]
fn fp_formal_no_saturation_certified() {
    fn z3_available() -> bool {
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !z3_available() {
        eprintln!("skipping fp_formal_no_saturation_certified: z3 not in PATH");
        return;
    }
    let run = |src: &str| -> (bool, String) {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("M.arch");
        std::fs::write(&path, src).unwrap();
        let out = arch()
            .arg("formal")
            .arg(&path)
            .arg("--solver")
            .arg("z3")
            .arg("--bound")
            .arg("2")
            .output()
            .expect("run arch formal");
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), all)
    };
    let src = std::fs::read_to_string("tests/fp_v1/FpNoSat.arch").unwrap();
    let (ok, all) = run(&src);
    assert!(
        ok && all.contains("PROVED"),
        "constrained no-sat must prove:\n{all}"
    );
    let stripped: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("assume"))
        .collect::<Vec<_>>()
        .join("\n");
    let (ok, all) = run(&stripped);
    assert!(
        !ok && all.contains("REFUTED"),
        "unconstrained must refute:\n{all}"
    );
}

/// Vacuity guard (soundness): contradictory `assume` clauses empty the
/// constrained state space, so a negated-property miter is trivially unsat
/// and WOULD report false PROVED. arch formal must detect the unsatisfiable
/// assumptions and report VACUOUS (exit 1), never a pass. Regression for the
/// pre-existing hole found while writing the paper's property-layer section.
#[test]
fn fp_formal_vacuity_guard() {
    fn z3_available() -> bool {
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    if !z3_available() {
        eprintln!("skipping fp_formal_vacuity_guard: z3 not in PATH");
        return;
    }
    let out = arch()
        .arg("formal")
        .arg("tests/fp_v1/FpVacuous.arch")
        .arg("--solver")
        .arg("z3")
        .arg("--bound")
        .arg("2")
        .output()
        .expect("run arch formal");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.contains("VACUOUS") && !all.contains("PROVED"),
        "contradictory assumes must report VACUOUS, not PROVED:\n{all}"
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "vacuous proof must be a hard failure (exit 1):\n{all}"
    );
}

// ── FP4E2M1 (OCP MX FP4) — storage-only ──────────────────────────────────
//
// The contract is as much about what is REJECTED as what works: E2M1 is a
// carrier for conversions and literals, with no operator surface at all.

/// Helper: run `arch check` on inline source, returning combined output and
/// whether it succeeded.
fn check_src(name: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join("arch_fp4_tests");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join(format!("{name}.arch"));
    std::fs::write(&path, src).expect("write fixture");
    let out = arch().arg("check").arg(&path).output().expect("run check");
    let merged = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // miette word-wraps diagnostics AND prefixes continuation lines with a
    // box-drawing gutter, so a phrase can straddle a break as
    // "storage-only float | format". Strip the gutter glyphs first, then
    // collapse whitespace, before any substring matching.
    let stripped: String = merged
        .chars()
        .map(|c| {
            if "│┃|╭╰─▲·".contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let flat = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    (out.status.success(), flat)
}

/// The conversion surface works end-to-end: widen, narrow, compute-in-FP32,
/// and a context-typed literal.
#[test]
fn fp4_conversion_surface_checks_and_builds() {
    let out = arch()
        .arg("check")
        .arg("tests/fp_v1/Fp4Convert.arch")
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "Fp4Convert.arch should check\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // SV must lower conversions to the proven helpers — NOT to the integer
    // path. Before the E2M1 conversion arms existed, `a.to_fp32()` emitted
    // `arch_u64_to_f32(...)`, reinterpreting the 4-bit float as an integer,
    // and `.to_fp4e2m1()` leaked ARCH syntax into the SV verbatim.
    let dir = std::env::temp_dir().join("arch_fp4_build");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let sv_path = dir.join("Fp4Convert.sv");
    let out = arch()
        .arg("build")
        .arg("tests/fp_v1/Fp4Convert.arch")
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "build failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(&sv_path).expect("emitted SV");
    assert!(
        sv.contains("arch_e2m1_to_f32("),
        "widen must use the proven helper:\n{sv}"
    );
    assert!(
        sv.contains("arch_f32_to_e2m1("),
        "narrow must use the proven helper:\n{sv}"
    );
    assert!(
        !sv.contains("to_fp4e2m1()"),
        "ARCH method syntax must not leak into SV:\n{sv}"
    );
    assert!(
        !sv.contains("arch_u64_to_f32(64'($unsigned(a)))"),
        "a float must never be widened through the INTEGER path:\n{sv}"
    );
    assert!(sv.contains("logic [3:0]"), "E2M1 is a 4-bit carrier:\n{sv}");
}

/// Arithmetic is a clean, spanned type error — not a dangling
/// `arch_e2m1_add` discovered later by Verilator / z3 / g++.
#[test]
fn fp4_arithmetic_is_rejected_at_typecheck() {
    for (name, op) in [("add", "a + b"), ("sub", "a - b"), ("mul", "a * b")] {
        let (ok, out) = check_src(
            &format!("fp4_arith_{name}"),
            &format!(
                "module A\n  port a: in FP4E2M1;\n  port b: in FP4E2M1;\n  \
                 port y: out FP4E2M1;\n  comb y = {op}; end comb\nend module A\n"
            ),
        );
        assert!(!ok, "`{op}` on FP4E2M1 must be rejected:\n{out}");
        assert!(
            out.contains("storage-only float format"),
            "`{op}`: message should name the storage-only rule:\n{out}"
        );
        assert!(
            !out.contains("arch_e2m1_add"),
            "must fail at typecheck, not by emitting a missing operator:\n{out}"
        );
    }
}

/// `is_nan` is refused with an accurate reason. E2M1 has no NaN encoding, so
/// answering a constant `false` would mislead — and calling it "not a float"
/// would be wrong, since it is one.
#[test]
fn fp4_is_nan_is_rejected_as_unrepresentable_not_as_non_float() {
    let (ok, out) = check_src(
        "fp4_isnan",
        "module A\n  port a: in FP4E2M1;\n  port y: out Bool;\n  \
         comb y = is_nan(a); end comb\nend module A\n",
    );
    assert!(!ok, "is_nan on FP4E2M1 must be rejected:\n{out}");
    assert!(
        out.contains("no NaN encoding"),
        "reason must be the missing encoding, not 'requires a float':\n{out}"
    );
    assert!(
        !out.contains("requires a float operand"),
        "FP4E2M1 IS a float — that message would be wrong:\n{out}"
    );
}

/// An overflowing literal is a compile error, never a silently wrong finite
/// constant. The generic IEEE rounder would have packed `S.11.0` = 4.0.
#[test]
fn fp4_overflowing_literal_is_a_compile_error() {
    let (ok, out) = check_src(
        "fp4_lit_overflow",
        "module A\n  port y: out FP4E2M1;\n  comb y = 8.0; end comb\nend module A\n",
    );
    assert!(!ok, "8.0 overflows FP4E2M1 (max 6.0):\n{out}");
    assert!(
        out.contains("FP4E2M1") && out.contains("6"),
        "diagnostic should name the format and its bound:\n{out}"
    );
    // In-range literals still work.
    let (ok, out) = check_src(
        "fp4_lit_ok",
        "module A\n  port y: out FP4E2M1;\n  comb y = 1.5; end comb\nend module A\n",
    );
    assert!(ok, "1.5 is exactly representable in E2M1:\n{out}");
}

// ── FP6 E2M3 / E3M2 — storage-only, same contract as FP4E2M1 ────────────

/// The FP6 conversion surface works, and SV lowers to the proven helpers
/// rather than the integer path.
#[test]
fn fp6_conversion_surface_checks_and_builds() {
    let out = arch()
        .arg("check")
        .arg("tests/fp_v1/Fp6Convert.arch")
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "Fp6Convert.arch should check\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let dir = std::env::temp_dir().join("arch_fp6_build");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let sv_path = dir.join("Fp6Convert.sv");
    let out = arch()
        .arg("build")
        .arg("tests/fp_v1/Fp6Convert.arch")
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(out.status.success(), "build failed: {out:?}");
    let sv = std::fs::read_to_string(&sv_path).expect("emitted SV");
    for helper in [
        "arch_e2m3_to_f32(",
        "arch_e3m2_to_f32(",
        "arch_f32_to_e2m3(",
        "arch_f32_to_e3m2(",
    ] {
        assert!(sv.contains(helper), "SV must use {helper}:\n{sv}");
    }
    assert!(
        !sv.contains("to_fp6e2m3()") && !sv.contains("to_fp6e3m2()"),
        "ARCH method syntax must not leak into SV:\n{sv}"
    );
    assert!(sv.contains("logic [5:0]"), "FP6 is a 6-bit carrier:\n{sv}");
}

/// Arithmetic and `is_nan` are rejected for both FP6 formats, with the same
/// storage-only reasoning as FP4E2M1.
#[test]
fn fp6_arithmetic_and_is_nan_are_rejected() {
    for ty in ["FP6E2M3", "FP6E3M2"] {
        let (ok, out) = check_src(
            &format!("fp6_arith_{ty}"),
            &format!(
                "module A\n  port a: in {ty};\n  port b: in {ty};\n  \
                 port y: out {ty};\n  comb y = a + b; end comb\nend module A\n"
            ),
        );
        assert!(!ok, "{ty}: `+` must be rejected:\n{out}");
        assert!(
            out.contains("storage-only float format"),
            "{ty}: message should name the storage-only rule:\n{out}"
        );

        let (ok, out) = check_src(
            &format!("fp6_isnan_{ty}"),
            &format!(
                "module A\n  port a: in {ty};\n  port y: out Bool;\n  \
                 comb y = is_nan(a); end comb\nend module A\n"
            ),
        );
        assert!(!ok, "{ty}: is_nan must be rejected:\n{out}");
        assert!(
            out.contains("no NaN encoding"),
            "{ty}: reason must be the missing encoding:\n{out}"
        );
    }
}

/// Overflowing literals are compile errors at each format's own bound —
/// the generic IEEE rounder would have produced a silently wrong finite.
#[test]
fn fp6_overflowing_literals_are_compile_errors() {
    // E2M3 max 7.5 (RNE midpoint 7.75); E3M2 max 28.0 (midpoint 30.0).
    for (ty, ok_lit, bad_lit) in [("FP6E2M3", "7.5", "8.0"), ("FP6E3M2", "28.0", "32.0")] {
        let (ok, out) = check_src(
            &format!("fp6_lit_bad_{ty}"),
            &format!(
                "module A\n  port y: out {ty};\n  comb y = {bad_lit}; end comb\nend module A\n"
            ),
        );
        assert!(!ok, "{ty}: {bad_lit} overflows:\n{out}");
        assert!(
            out.contains(ty),
            "{ty}: diagnostic should name the format:\n{out}"
        );

        let (ok, out) = check_src(
            &format!("fp6_lit_ok_{ty}"),
            &format!(
                "module A\n  port y: out {ty};\n  comb y = {ok_lit}; end comb\nend module A\n"
            ),
        );
        assert!(ok, "{ty}: {ok_lit} is exactly representable:\n{out}");
    }
}

// ── E8M0 — the MX block scale type (not a float) ────────────────────────

/// The scale surface works, and each backend lowers to the E8M0 helpers
/// rather than falling through to the f32 ones.
#[test]
fn e8m0_scale_surface_checks_and_builds() {
    let out = arch()
        .arg("check")
        .arg("tests/fp_v1/E8m0Scale.arch")
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "E8m0Scale.arch should check\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let dir = std::env::temp_dir().join("arch_e8m0_build");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let sv_path = dir.join("E8m0Scale.sv");
    let out = arch()
        .arg("build")
        .arg("tests/fp_v1/E8m0Scale.arch")
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(out.status.success(), "build failed: {out:?}");
    let sv = std::fs::read_to_string(&sv_path).expect("emitted SV");
    // Assert on the MODULE BODY, never the whole file: the helper
    // DEFINITIONS live in the preamble, so `sv.contains("arch_e8m0_to_f32(")`
    // matches even when the call site lowered wrongly. An earlier version of
    // this test was vacuous for exactly that reason and let a regression
    // through — the body was emitting `arch_u64_to_f32(...)`, widening the
    // scale code as an INTEGER.
    let body = &sv[sv.find("module E8m0Scale").expect("module in SV")..];
    assert!(
        body.contains("arch_e8m0_to_f32(s)"),
        "widen must lower to the E8M0 helper, not the integer path:\n{body}"
    );
    assert!(
        body.contains("arch_f32_to_e8m0(x)"),
        "narrow must lower to the E8M0 helper:\n{body}"
    );
    assert!(
        !body.contains("arch_u64_to_f32") && !body.contains("arch_i64_to_f32"),
        "a scale must never be widened through the INTEGER path:\n{body}"
    );
    assert!(
        !body.contains(".to_e8m0()"),
        "ARCH method syntax must not leak into SV:\n{body}"
    );
    assert!(
        sv.contains("logic [7:0]"),
        "E8M0 is an 8-bit carrier:\n{sv}"
    );

    // The NaN test must be the E8M0 one. Before this was wired, `is_nan(s)`
    // fell through to the f32 test and emitted `s[30:23] == 8'hFF` on an
    // EIGHT-BIT signal — which SV zero-fills into a silent constant false
    // rather than an error.
    assert!(
        sv.contains("== 8'hFF)"),
        "is_nan must test the 0xFF code:\n{sv}"
    );
    // Scope this to the MODULE BODY: the f32 helper functions in the
    // preamble legitimately contain [30:23].
    let body = &sv[sv.find("module E8m0Scale").expect("module in SV")..];
    assert!(
        !body.contains("[30:23]"),
        "must not use the f32 NaN test on an 8-bit scale:\n{body}"
    );
}

/// E8M0 is not a float: arithmetic is rejected. `is_nan` IS allowed,
/// because the format does have a NaN code.
#[test]
fn e8m0_is_not_a_float_but_has_a_nan_code() {
    let (ok, out) = check_src(
        "e8m0_arith",
        "module A\n  port a: in E8M0;\n  port b: in E8M0;\n  port y: out E8M0;\n  \
         comb y = a + b; end comb\nend module A\n",
    );
    assert!(!ok, "arithmetic on a scale type must be rejected:\n{out}");

    // is_nan is meaningful here (0xFF) and must be accepted.
    let (ok, out) = check_src(
        "e8m0_isnan",
        "module A\n  port a: in E8M0;\n  port y: out Bool;\n  \
         comb y = is_nan(a); end comb\nend module A\n",
    );
    assert!(ok, "is_nan on E8M0 is meaningful (0xFF):\n{out}");
}

/// The E8M0 widen produces the profile's canonical NaN for the 0xFF code.
/// An early version hardcoded the riscv pattern, which leaked
/// `32'h7FC00000` into cuda builds and broke `fp_compat_build_profiles`.
#[test]
fn e8m0_nan_follows_the_fp_compat_profile() {
    let dir = std::env::temp_dir().join("arch_e8m0_profiles");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for (profile, want, forbid) in [
        ("riscv", "32'h7FC00000", "32'h7FFFFFFF"),
        ("cuda", "32'h7FFFFFFF", "32'h7FC00000"),
    ] {
        let sv_path = dir.join(format!("E8m0Scale_{profile}.sv"));
        let out = arch()
            .arg("build")
            .arg("tests/fp_v1/E8m0Scale.arch")
            .arg("--fp-compat")
            .arg(profile)
            .arg("-o")
            .arg(&sv_path)
            .output()
            .expect("run arch build");
        assert!(out.status.success(), "{profile} build failed: {out:?}");
        let sv = std::fs::read_to_string(&sv_path).expect("emitted SV");
        // The E8M0 widen helper must carry the profile's NaN.
        let widen = sv
            .find("function automatic logic [31:0] arch_e8m0_to_f32")
            .expect("arch_e8m0_to_f32 must be emitted");
        let body = &sv[widen..widen + 600.min(sv.len() - widen)];
        assert!(
            body.contains(want),
            "{profile}: arch_e8m0_to_f32 should use {want}:\n{body}"
        );
        assert!(
            !body.contains(forbid),
            "{profile}: arch_e8m0_to_f32 must not use {forbid}:\n{body}"
        );
    }
}
