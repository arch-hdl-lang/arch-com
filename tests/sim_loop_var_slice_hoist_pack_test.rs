//! Regression coverage for arch#1019 defect #3 — `arch sim` emitted
//! non-compiling C++ for a scalar-LHS ← whole-Vec-RHS assignment.
//!
//! `LoopVarSliceHoist` ends with `y_index = wi;` where `wi: wire Vec<Bool,4>`
//! and `y_index: out UInt<4>`. `arch build` accepts this as a packed-vector
//! assignment (element 0 → bit 0) and emits clean SV; Verilator agrees. But the
//! C++ sim decomposes the Vec into a `T[N]` array, and the whole-Vec assignment
//! arm in `emit_stmt` handled only Vec←Vec (element copy) and Vec←0 (memset),
//! not scalar←Vec. The generic fall-through then emitted `y_index = _let_wi;`
//! (`scalar = array`), which the host compiler rejects with
//! `assigning to 'uint8_t' from 'uint8_t[4]'`. Because the affected design was
//! only ever exercised through `arch build` (see the sibling equivalence test
//! `test_loop_var_slice_hoist_behavioral_equivalence_verilator_and_iverilog`,
//! which never runs `arch sim`), the sim build failure was invisible.
//!
//! The fix packs the decomposed element array back into the scalar with the
//! same little-endian layout as the SV backend. These tests exercise the
//! `arch sim` path end-to-end (build + run + value check) and pin the codegen
//! shape so the `scalar = array` regression cannot silently return.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

const FIXTURE: &str = "tests/icarus_portability/LoopVarSliceHoist.arch";

/// End-to-end: the model must build AND compute `y_index` as the packed vector,
/// matching the SV backend (element 0 → bit 0). The TB self-checks against a
/// host-side reference and prints `PASS` on success.
#[test]
fn loop_var_slice_hoist_scalar_pack_sim_matches_sv() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg(FIXTURE)
        .arg("--tb")
        .arg("tests/sim_loop_var_slice_hoist_pack_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "arch sim should build and run the scalar-pack model\nstdout:\n{stdout}\nstderr:\n{stderr}",
    );
    // Pre-fix failure mode: a raw host-compiler diagnostic on the array→scalar
    // assignment.
    assert!(
        !stderr.contains("incompatible type") && !stdout.contains("incompatible type"),
        "generated model assigned an array to a scalar:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS") && !stdout.contains("MISMATCH"),
        "sim y_index must match the SV packed-vector value\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Codegen shape: the generated comb body must pack the decomposed element
/// array into the scalar, never emit the bare `y_index = _let_wi;`
/// (`scalar = array`) that pre-fix failed to compile.
#[test]
fn loop_var_slice_hoist_scalar_pack_codegen_shape() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg(FIXTURE)
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim codegen");
    assert!(
        out.status.success(),
        "arch sim codegen should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let model = td.path().join("VLoopVarSliceHoist.cpp");
    let cpp = std::fs::read_to_string(&model).expect("read model");
    // The pack loop reads each decomposed element and ORs it into a scalar.
    assert!(
        cpp.contains("_let_wi[_i]"),
        "generated model should pack the decomposed Vec elements into the scalar:\n{cpp}"
    );
    // The bare array→scalar assignment must not be emitted.
    assert!(
        !cpp.contains("y_index  = _let_wi;") && !cpp.contains("y_index = _let_wi;"),
        "generated model must not assign the element array directly to the scalar:\n{cpp}"
    );
}
