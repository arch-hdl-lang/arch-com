//! Regression coverage for arch#858 — Vec element width inference in the
//! sim C++ emitter.
//!
//! Pre-fix, `infer_expr_width`'s Index arm divided the widths-map entry
//! by the element count, but `build_widths` registers Vec-typed
//! regs/ports at the scalar default 32 (not the packed total), so
//! `Vec<UInt<32>,4>[i]` inferred 8 and `Vec<UInt<64>,8>[i]` inferred 4:
//! concats of Vec elements shifted by the wrong positions, and a runtime
//! bit-select on a 64-bit element aborted with a bogus `[0..4)` bounds
//! check. Also covers the wide-`let` follow-on found while fixing it:
//! `let y = {a, b};` into a 65–128-bit output port truncated through
//! uint64_t instead of converting via `_arch_u128_to_vl`.

use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// End-to-end: concat shift positions track the declared element width
/// (32- and 64-bit elements, 64- and 128-bit concat results), and legal
/// runtime bit indices up to 63 on a Vec<UInt<64>,_> element don't trip
/// the bounds check.
#[test]
fn vec_elem_width_concat_and_bitsel_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_vec_elem_width_regression.arch")
        .arg("--tb")
        .arg("tests/sim_vec_elem_width_regression_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for sim_vec_elem_width_regression\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("6 pass / 0 fail"),
        "expected all 6 element-width checks to pass; got:\n{stdout}"
    );
}

/// Codegen shape: the runtime bit-select on a Vec<UInt<64>,8> element
/// must emit an `_ARCH_BCHK` bound of 64 (the declared element width),
/// not widths[name]/count = 4, and the 128-bit concat must shift the
/// high element by 64.
#[test]
fn vec_elem_width_codegen_shape() {
    let td = tempfile::tempdir().expect("tempdir");
    let out = arch()
        .arg("sim")
        .arg("tests/sim_vec_elem_width_regression.arch")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim codegen");
    assert!(
        out.status.success(),
        "arch sim codegen should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let model = td.path().join("Vsim_vec_elem_width_regression.cpp");
    let cpp = std::fs::read_to_string(&model).expect("read model");
    assert!(
        cpp.contains(", 64, \"<bitsel>[i]\")"),
        "runtime bit-select on a 64-bit Vec element must bound-check \
         against 64:\n{cpp}"
    );
    assert!(
        cpp.contains("<< 64"),
        "128-bit concat of Vec<UInt<64>,_> elements must shift the high \
         element by 64:\n{cpp}"
    );
    assert!(
        cpp.contains("_arch_u128_to_vl"),
        "let-bound 65-128-bit output port must convert through \
         _arch_u128_to_vl, not truncate through uint64_t:\n{cpp}"
    );
}
