//! Regression coverage for unary negation on non-arithmetic float carriers.
//!
//! FP4/FP6 are storage-only formats, while E8M0 and UE4M3 are unsigned block
//! scales. None has a scalar arithmetic surface, so `-value` must be rejected
//! during type checking. Arithmetic float formats and integer negation remain
//! valid.

use std::process::Command;

fn check_source(name: &str, source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("create temporary directory");
    let path = dir.path().join(format!("{name}.arch"));
    std::fs::write(&path, source).expect("write source");
    let output = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

fn unary_neg_source(input: &str, output: &str) -> String {
    format!(
        "module UnaryNeg\n  port a: in {input};\n  port y: out {output};\n  comb y = -a; end comb\nend module UnaryNeg\n"
    )
}

#[test]
fn unary_neg_rejects_storage_only_floats_and_unsigned_scales() {
    for ty in ["FP4E2M1", "FP6E2M3", "FP6E3M2", "E8M0", "UE4M3"] {
        let (ok, output) =
            check_source(&format!("unary_neg_reject_{ty}"), &unary_neg_source(ty, ty));
        assert!(!ok, "unary negation on {ty} must be rejected:\n{output}");
        assert!(
            output.contains("operator `-` is not supported") && output.contains(ty),
            "diagnostic should identify the unsupported unary operator and type:\n{output}"
        );
    }
}

#[test]
fn unary_neg_preserves_arithmetic_float_and_integer_behavior() {
    for ty in ["FP32", "BF16", "FP8E4M3", "FP8E5M2", "SInt<8>"] {
        let (ok, output) =
            check_source(&format!("unary_neg_accept_{ty}"), &unary_neg_source(ty, ty));
        assert!(ok, "unary negation on {ty} should remain valid:\n{output}");
    }

    // Unsigned negation widens to a signed result, preserving the existing
    // integer rule rather than treating UInt as an arithmetic float carrier.
    let (ok, output) = check_source(
        "unary_neg_accept_uint",
        &unary_neg_source("UInt<8>", "SInt<9>"),
    );
    assert!(ok, "unary negation on UInt should widen to SInt:\n{output}");
}
