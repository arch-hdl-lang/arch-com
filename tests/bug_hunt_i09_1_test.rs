//! Regression coverage for formal encoding and counterexample replay of
//! unsigned unary negation.

use std::process::Command;

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn formal_unsigned_neg_widens_before_negation_and_replays_counterexample() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args([
            "formal",
            "tests/formal/unsigned_neg_bounds.arch",
            "--bound",
            "2",
        ])
        .output()
        .expect("failed to spawn arch formal");
    let status = output.status.code().unwrap_or(-1);
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(status, 1, "one assertion should be refuted:\n{text}");
    assert!(
        text.lines()
            .any(|line| line.contains("lower_bound") && line.contains("REFUTED")),
        "unsigned UInt<2> value 3 must refute o >= -2:\n{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains("non_positive") && line.contains("PROVED")),
        "-UInt<2> must remain non-positive:\n{text}"
    );
    assert!(
        !text.contains("encoding unsound") && !text.contains("EncodingUnsound"),
        "counterexample replay must confirm the refutation:\n{text}"
    );
}
