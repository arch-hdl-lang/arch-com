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

fn run_build(file: &std::path::Path, extra: &[String]) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arch"));
    cmd.arg("build").arg(file);
    for a in extra { cmd.arg(a); }
    let out = cmd.output().expect("failed to spawn arch");
    let merged = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    (out.status.code().unwrap_or(-1), merged)
}

fn run_build_with_env(
    file: &std::path::Path,
    extra: &[String],
    envs: &[(&str, &str)],
) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arch"));
    cmd.arg("build").arg(file);
    for (key, value) in envs { cmd.env(key, value); }
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

#[test]
fn formal_credit_channel_active_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    // Active-traffic version of the occupancy invariant: sender drives
    // send_valid via can_send gating, receiver drives credit_return via
    // valid gating. Also asserts the derived-signal equivalences
    // (`can_send ⇔ credit != 0`, `valid ⇔ occ != 0`) which use the
    // newly-resolvable SynthIdents added on top of PR-hf4 Phase 1.
    let (code, out) = run_formal(
        "tests/formal/credit_channel_active.arch",
        &["--top", "CreditPairActive", "--bound", "8"],
    );
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert_eq!(out.matches("PROVED").count(), 3,
               "expected 3 PROVEDs (credit_balance, can_send_iff_credit, valid_iff_occ):\n{out}");
}

#[test]
fn formal_credit_channel_invariant_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    // PR-hf4 Phase 1 end-to-end: the credit_channel occupancy invariant
    // (`credit + occ == DEPTH`) proves on a 2-module hierarchical design
    // where flatten_for_formal carries the channel state across the
    // inst boundary and merges the handshake signals.
    let (code, out) = run_formal(
        "tests/formal/credit_channel_invariant.arch",
        &["--top", "CreditPair", "--bound", "8"],
    );
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert!(out.contains("credit_balance"), "expected credit_balance label:\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED in output:\n{out}");
}

#[test]
fn construct_proof_smt_fifo_and_arbiter_checks() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("Constructs.arch");
    let sv_path = td.path().join("Constructs.sv");
    let smt_path = td.path().join("Constructs.construct-proof.smt2");
    std::fs::write(
        &arch_path,
        r#"
domain SysDomain
end domain SysDomain

fifo TxQueue
  param DEPTH: const = 8;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo TxQueue

arbiter BusArbiter
  policy round_robin;
  param NUM_REQ: const = 3;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter BusArbiter
"#,
    )
    .expect("write arch");
    let args = vec![
        "-o".to_string(),
        sv_path.to_string_lossy().to_string(),
        format!("--emit-construct-proof-smt={}", smt_path.display()),
        "--check-construct-proof-smt".to_string(),
        "--construct-proof-smt-solver=z3".to_string(),
    ];
    let (code, out) = run_build(&arch_path, &args);
    assert_eq!(code, 0, "expected construct SMT check to pass; got {code}\n{out}");
    assert!(out.contains("Construct SMT proof OK"), "expected solver check output:\n{out}");
    let smt = std::fs::read_to_string(&smt_path).expect("read smt");
    assert_eq!(smt.matches("(check-sat)").count(), 2, "expected FIFO+arbiter queries:\n{smt}");
    assert!(smt.contains("; fifo TxQueue"));
    assert!(smt.contains("; arbiter BusArbiter"));
}

#[test]
fn construct_proof_lean_finds_home_elan_when_lake_not_on_path() {
    let Some(home) = std::env::var_os("HOME") else {
        eprintln!("skipping: HOME not set");
        return;
    };
    let home_lake = std::path::PathBuf::from(home).join(".elan/bin/lake");
    if !home_lake.exists() {
        eprintln!("skipping: ~/.elan/bin/lake not installed");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("ConstructLean.arch");
    let sv_path = td.path().join("ConstructLean.sv");
    std::fs::write(
        &arch_path,
        r#"
domain SysDomain
end domain SysDomain

fifo TxQueue
  param DEPTH: const = 4;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo TxQueue
"#,
    )
    .expect("write arch");

    let args = vec![
        "-o".to_string(),
        sv_path.to_string_lossy().to_string(),
        "--check-construct-proof-lean".to_string(),
        "--construct-proof-lean-project=proofs/lean_thread_lowering".to_string(),
    ];
    let (code, out) = run_build_with_env(&arch_path, &args, &[("PATH", "/usr/bin:/bin")]);
    assert_eq!(
        code, 0,
        "expected Lean replay fallback to ~/.elan/bin/lake; got {code}\n{out}"
    );
    assert!(
        out.contains("Lean construct proof replay OK"),
        "expected Lean replay output:\n{out}"
    );
}

#[test]
fn construct_proof_lean_non_power_two_fifo_catches_depth_wrap_bug() {
    let Some(home) = std::env::var_os("HOME") else {
        eprintln!("skipping: HOME not set");
        return;
    };
    let home_lake = std::path::PathBuf::from(home).join(".elan/bin/lake");
    if !home_lake.exists() {
        eprintln!("skipping: ~/.elan/bin/lake not installed");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("NonPow2Fifo.arch");
    let sv_path = td.path().join("NonPow2Fifo.sv");
    let proof_path = td.path().join("NonPow2Fifo.construct-proof.lean");
    let bad_proof_path = td.path().join("NonPow2Fifo.bad-wrap.construct-proof.lean");
    std::fs::write(
        &arch_path,
        r#"
domain SysDomain
end domain SysDomain

fifo NonPow2Queue
  param DEPTH: const = 3;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo NonPow2Queue
"#,
    )
    .expect("write arch");

    let args = vec![
        "-o".to_string(),
        sv_path.to_string_lossy().to_string(),
        format!("--emit-construct-proof-lean={}", proof_path.display()),
        "--check-construct-proof-lean".to_string(),
        "--construct-proof-lean-project=proofs/lean_thread_lowering".to_string(),
    ];
    let (code, out) = run_build(&arch_path, &args);
    assert_eq!(
        code, 0,
        "expected valid DEPTH=3 FIFO Lean replay to pass; got {code}\n{out}"
    );

    let proof = std::fs::read_to_string(&proof_path).expect("read proof");
    let bad_proof = proof
        .replace(
            "(wrPtr + 1) % Fifo.ptrMod NonPow2Queue_fifo",
            "(wrPtr + 1) % NonPow2Queue_fifo.depth",
        )
        .replace(
            "(rdPtr + 1) % Fifo.ptrMod NonPow2Queue_fifo",
            "(rdPtr + 1) % NonPow2Queue_fifo.depth",
        );
    assert_ne!(proof, bad_proof, "expected proof mutation to change pointer wrap");
    std::fs::write(&bad_proof_path, bad_proof).expect("write bad proof");

    let output = Command::new(&home_lake)
        .arg("env")
        .arg("lean")
        .arg(&bad_proof_path)
        .current_dir("proofs/lean_thread_lowering")
        .output()
        .expect("run lake env lean");
    assert!(
        !output.status.success(),
        "expected Lean to reject DEPTH wrap bug\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostics.contains("Fifo.ptrMod NonPow2Queue_fifo"),
        "expected failure to mention expected ptrMod equation:\n{diagnostics}"
    );
}

#[test]
fn formal_sva_temporal_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/sva_temporal_proves.arch", &["--bound", "5"]);
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(out.contains("gnt_follows_req"), "missing property name in output:\n{out}");
    assert!(out.contains("req_implies_next_gnt"), "missing |=> property:\n{out}");
    // Both asserts should PROVE; cover should HIT.
    let proved = out.matches("PROVED").count();
    assert!(proved >= 2, "expected ≥2 PROVED (got {proved}):\n{out}");
    assert!(out.contains("HIT"), "expected cover HIT:\n{out}");
}

#[test]
fn formal_sva_temporal_refutes() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/sva_temporal_refutes.arch", &["--bound", "5"]);
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"), "expected REFUTED:\n{out}");
}

#[test]
fn formal_sva_phase2_proves() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/sva_phase2_proves.arch", &["--bound", "8"]);
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    for prop in ["rose_implies_a_edge", "fell_implies_a_edge", "next_chain"] {
        assert!(out.contains(prop), "missing property `{prop}`:\n{out}");
    }
    let proved = out.matches("PROVED").count();
    assert_eq!(proved, 3, "expected 3 PROVED, got {proved}:\n{out}");
}

#[test]
fn formal_sva_phase2_refutes() {
    if !z3_available() { eprintln!("skipping: z3 not in PATH"); return; }
    let (code, out) = run_formal("tests/formal/sva_phase2_refutes.arch", &["--bound", "8"]);
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"), "expected REFUTED:\n{out}");
}
