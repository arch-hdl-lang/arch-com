//! Integration tests for `arch formal` (SMT-LIB2 bounded model checking).
//!
//! Tests that exercise a solver are gated on `z3` being available in PATH.
//! If it's not, the test prints a skip message and returns early.

use std::process::Command;

fn z3_available() -> bool {
    Command::new("z3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn solver_available(name: &str) -> bool {
    Command::new(name).arg("--help").output().map(|_| true).unwrap_or(false)
}

/// Run `arch formal <file> [extra...]` and return (exit_code, stdout_stderr_combined).
fn run_formal(file: &str, extra: &[&str]) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arch"));
    cmd.arg("formal").arg(file);
    for a in extra { cmd.arg(a); }
    let out = cmd.output().expect("failed to spawn arch");
    let merged = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    (out.status.code().unwrap_or(-1), merged)
}

#[test]
fn formal_counter_simple_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/counter_simple.arch", &["--bound", "5"]);
    assert_eq!(code, 0, "expected exit 0 (all PROVED); got {code}\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED in output:\n{out}");
}

#[test]
fn formal_counter_bounded_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/counter_bounded.arch", &["--bound", "30"]);
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_counter_overflow_refutes() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/counter_overflow.arch", &["--bound", "20"]);
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"), "expected REFUTED:\n{out}");
    assert!(out.contains("Counterexample"), "expected counterexample:\n{out}");
}

#[test]
fn formal_cover_hit() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/cover_hit.arch", &["--bound", "20"]);
    assert_eq!(code, 0, "expected exit 0 (HIT); got {code}\n{out}");
    assert!(out.contains("HIT"));
}

#[test]
fn formal_cover_not_reached() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    // Bound 3 is too small for a 4-bit counter to reach 8 (takes 8 increments).
    let (code, out) = run_formal("tests/formal/cover_hit.arch", &["--bound", "3"]);
    assert_eq!(code, 1, "expected exit 1 (NOT REACHED); got {code}\n{out}");
    assert!(out.contains("NOT REACHED"), "expected NOT REACHED:\n{out}");
}

#[test]
fn formal_guard_pass() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/guard_pass.arch", &["--bound", "10"]);
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_guard_fail() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/guard_fail.arch", &["--bound", "10"]);
    assert_eq!(code, 1, "expected exit 1; got {code}\n{out}");
    assert!(out.contains("REFUTED"));
}

#[test]
fn formal_emit_smt_file() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let out_path = std::env::temp_dir().join("arch_formal_emit_test.smt2");
    let _ = std::fs::remove_file(&out_path);
    let (_code, _out) = run_formal(
        "tests/formal/counter_simple.arch",
        &["--bound", "3", "--emit-smt", out_path.to_str().unwrap()],
    );
    let smt = std::fs::read_to_string(&out_path).expect("smt file should exist");
    assert!(smt.contains("(set-logic QF_BV)"));
    assert!(smt.contains("declare-fun cnt_0"));
    assert!(smt.contains("declare-fun cnt_3"));
}

#[test]
fn formal_solver_parity_boolector() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    if !solver_available("boolector") { eprintln!("skipping: boolector not in PATH"); return; }
    let (code, out) = run_formal(
        "tests/formal/counter_simple.arch",
        &["--bound", "5", "--solver", "boolector"],
    );
    assert_eq!(code, 0, "expected exit 0 via boolector; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_solver_parity_bitwuzla() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    if !solver_available("bitwuzla") { eprintln!("skipping: bitwuzla not in PATH"); return; }
    let (code, out) = run_formal(
        "tests/formal/counter_simple.arch",
        &["--bound", "5", "--solver", "bitwuzla"],
    );
    assert_eq!(code, 0, "expected exit 0 via bitwuzla; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_hier_adder_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal(
        "tests/formal/hier_adder_proves.arch",
        &["--top", "HierTop", "--bound", "5"],
    );
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED in output:\n{out}");
}

#[test]
fn formal_hier_adder_refutes() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal(
        "tests/formal/hier_adder_refutes.arch",
        &["--top", "HierTopBad", "--bound", "5"],
    );
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"));
    assert!(out.contains("Counterexample"));
}

#[test]
fn formal_hier_counter_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal(
        "tests/formal/hier_counter_proves.arch",
        &["--top", "HierCounterTop", "--bound", "25"],
    );
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_hier_multi_inst_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal(
        "tests/formal/hier_multi_inst_proves.arch",
        &["--top", "HierMultiTop", "--bound", "25"],
    );
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    // Both properties should PROVE.
    assert!(out.matches("PROVED").count() >= 2, "expected 2 PROVEDs:\n{out}");
}
