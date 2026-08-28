//! Integration tests for `arch formal` (SMT-LIB2 bounded model checking).
//!
//! Tests that exercise a solver are gated on `z3` being available in PATH.
//! If it's not, the test prints a skip message and returns early.

use std::process::Command;

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The in-repo Lean proof project (`ArchConstructProof` + `ArchThreadLoweringProof`).
const LEAN_PROJECT_DIR: &str = "proofs/lean_thread_lowering";

/// Locate `lake`: PATH first, then the standard elan install location.
fn find_lake() -> Option<std::path::PathBuf> {
    if Command::new("lake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(std::path::PathBuf::from("lake"));
    }
    let home = std::env::var_os("HOME")?;
    let home_lake = std::path::PathBuf::from(home).join(".elan/bin/lake");
    home_lake.exists().then_some(home_lake)
}

/// Make sure the in-repo Lean proof library is built before any replay test
/// runs, and report whether Lean testing is possible at all.
///
/// Why this exists: `proofs/lean_thread_lowering/.lake/` is build output and
/// is gitignored, so a fresh clone — or, far more often, a fresh
/// `git worktree add`, which is the standard way to work this repo — has
/// none. Every Lean replay test then fails with
///
/// ```text
/// error: unknown module prefix 'ArchConstructProof'
/// No directory 'ArchConstructProof' or file 'ArchConstructProof.olean' ...
/// ```
///
/// which reads exactly like a proof regression but is only a missing build.
/// The project has no external dependencies (no Mathlib), so a cold build is
/// ~5s — cheap enough to just do here, once per test binary, instead of
/// leaving a phantom failure for whoever runs `cargo test` next.
///
/// Returns `false` when `lake` is not installed at all — callers then skip,
/// matching the repo-wide "skip cleanly when the external tool is absent"
/// convention CI depends on (`.github/workflows/test.yml` installs no Lean
/// toolchain, so these tests skip there and run on dev machines).
fn lean_project_ready() -> bool {
    static READY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *READY.get_or_init(|| {
        let Some(lake) = find_lake() else {
            eprintln!("skipping Lean replay tests: `lake` not found on PATH or in ~/.elan/bin");
            return false;
        };
        // Already built? `lake build` is a fast no-op, but skipping the
        // process spawn keeps the warm path free.
        if std::path::Path::new(LEAN_PROJECT_DIR)
            .join(".lake/build/lib/lean/ArchConstructProof.olean")
            .exists()
        {
            return true;
        }
        eprintln!(
            "building the Lean proof library in {LEAN_PROJECT_DIR} \
             (first run in this checkout; ~5s, no external deps)"
        );
        match Command::new(&lake)
            .arg("build")
            .current_dir(LEAN_PROJECT_DIR)
            .output()
        {
            Ok(o) if o.status.success() => true,
            Ok(o) => {
                eprintln!(
                    "skipping Lean replay tests: `lake build` failed in {LEAN_PROJECT_DIR}\n\
                     stdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                false
            }
            Err(e) => {
                eprintln!("skipping Lean replay tests: could not run `lake build`: {e}");
                false
            }
        }
    })
}

fn solver_available(name: &str) -> bool {
    Command::new(name)
        .arg("--help")
        .output()
        .map(|_| true)
        .unwrap_or(false)
}

/// Run `arch formal <file> [extra...]` and return (exit_code, stdout_stderr_combined).
fn run_formal(file: &str, extra: &[&str]) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arch"));
    cmd.arg("formal").arg(file);
    for a in extra {
        cmd.arg(a);
    }
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
    for a in extra {
        cmd.arg(a);
    }
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
    for (key, value) in envs {
        cmd.env(key, value);
    }
    for a in extra {
        cmd.arg(a);
    }
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
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/counter_simple.arch", &["--bound", "5"]);
    assert_eq!(code, 0, "expected exit 0 (all PROVED); got {code}\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED in output:\n{out}");
}

#[test]
fn formal_counter_bounded_proves() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/counter_bounded.arch", &["--bound", "30"]);
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_counter_overflow_refutes() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/counter_overflow.arch", &["--bound", "20"]);
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"), "expected REFUTED:\n{out}");
    assert!(
        out.contains("Counterexample"),
        "expected counterexample:\n{out}"
    );
}

#[test]
fn formal_cover_hit() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/cover_hit.arch", &["--bound", "20"]);
    assert_eq!(code, 0, "expected exit 0 (HIT); got {code}\n{out}");
    assert!(out.contains("HIT"));
}

#[test]
fn formal_cover_not_reached() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    // Bound 3 is too small for a 4-bit counter to reach 8 (takes 8 increments).
    let (code, out) = run_formal("tests/formal/cover_hit.arch", &["--bound", "3"]);
    assert_eq!(code, 1, "expected exit 1 (NOT REACHED); got {code}\n{out}");
    assert!(out.contains("NOT REACHED"), "expected NOT REACHED:\n{out}");
}

#[test]
fn formal_guard_pass() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/guard_pass.arch", &["--bound", "10"]);
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_guard_fail() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/guard_fail.arch", &["--bound", "10"]);
    assert_eq!(code, 1, "expected exit 1; got {code}\n{out}");
    assert!(out.contains("REFUTED"));
}

// ── Pipelined operators (`op<pipelined, N>`) ────────────────────────────────
// `arch formal` used to refuse any design containing a `<pipelined, N>` call.
// Since arch#968 proved the retimed staged datapath bit-identical to the
// single-cycle operator for all inputs (Route A SMT miter + Route B Lean
// retiming lemma), the encoder discharges it as that comb operator, fed into
// the pipe_reg the formal model already delays by N cycles.

/// The pipelined fma equals the *combinational* fma delayed through an
/// identical N-deep pipe_reg — proven for all inputs. This is the whole
/// discharge in one property: it fails (a) if `arch formal` still refuses the
/// pipelined call, (b) if the call were stubbed to anything but the fma, or
/// (c) if the pipeline latency were mismodeled (a wrong N refutes it).
#[test]
fn formal_pipelined_fma_equals_delayed_comb() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/pipelined_fma_equiv.arch", &["--bound", "20"]);
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED:\n{out}");
    assert!(
        !out.contains("not yet supported"),
        "`arch formal` must no longer refuse pipelined operators:\n{out}"
    );
}

#[test]
fn formal_emit_smt_file() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
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
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    if !solver_available("boolector") {
        eprintln!("skipping: boolector not in PATH");
        return;
    }
    let (code, out) = run_formal(
        "tests/formal/counter_simple.arch",
        &["--bound", "5", "--solver", "boolector"],
    );
    assert_eq!(code, 0, "expected exit 0 via boolector; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_solver_parity_bitwuzla() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    if !solver_available("bitwuzla") {
        eprintln!("skipping: bitwuzla not in PATH");
        return;
    }
    let (code, out) = run_formal(
        "tests/formal/counter_simple.arch",
        &["--bound", "5", "--solver", "bitwuzla"],
    );
    assert_eq!(code, 0, "expected exit 0 via bitwuzla; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_hier_adder_proves() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal(
        "tests/formal/hier_adder_proves.arch",
        &["--top", "HierTop", "--bound", "5"],
    );
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED in output:\n{out}");
}

#[test]
fn formal_hier_adder_refutes() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
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
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal(
        "tests/formal/hier_counter_proves.arch",
        &["--top", "HierCounterTop", "--bound", "25"],
    );
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(out.contains("PROVED"));
}

#[test]
fn formal_hier_multi_inst_proves() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal(
        "tests/formal/hier_multi_inst_proves.arch",
        &["--top", "HierMultiTop", "--bound", "25"],
    );
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    // Both properties should PROVE.
    assert!(
        out.matches("PROVED").count() >= 2,
        "expected 2 PROVEDs:\n{out}"
    );
}

#[test]
fn formal_credit_channel_active_proves() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
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
    assert_eq!(
        out.matches("PROVED").count(),
        3,
        "expected 3 PROVEDs (credit_balance, can_send_iff_credit, valid_iff_occ):\n{out}"
    );
}

#[test]
fn formal_credit_channel_invariant_proves() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    // PR-hf4 Phase 1 end-to-end: the credit_channel occupancy invariant
    // (`credit + occ == DEPTH`) proves on a 2-module hierarchical design
    // where flatten_for_formal carries the channel state across the
    // inst boundary and merges the handshake signals.
    let (code, out) = run_formal(
        "tests/formal/credit_channel_invariant.arch",
        &["--top", "CreditPair", "--bound", "8"],
    );
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert!(
        out.contains("credit_balance"),
        "expected credit_balance label:\n{out}"
    );
    assert!(out.contains("PROVED"), "expected PROVED in output:\n{out}");
}

#[test]
fn construct_proof_smt_fifo_and_arbiter_checks() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
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
    assert_eq!(
        code, 0,
        "expected construct SMT check to pass; got {code}\n{out}"
    );
    assert!(
        out.contains("Construct SMT proof OK"),
        "expected solver check output:\n{out}"
    );
    let smt = std::fs::read_to_string(&smt_path).expect("read smt");
    assert_eq!(
        smt.matches("(check-sat)").count(),
        2,
        "expected FIFO+arbiter queries:\n{smt}"
    );
    assert!(smt.contains("; fifo TxQueue"));
    assert!(smt.contains("TxQueue_fifo_0_next_wr_ptr"));
    assert!(smt.contains("TxQueue_fifo_0_write_index"));
    assert!(smt.contains("; arbiter BusArbiter"));
}

#[test]
fn construct_proof_lean_finds_home_elan_when_lake_not_on_path() {
    if !lean_project_ready() {
        return;
    }
    // This test is specifically about the ~/.elan fallback, so it needs lake
    // installed *there* — a PATH-only lake would not exercise it.
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
    if !lean_project_ready() {
        return;
    }
    let lake = find_lake().expect("lean_project_ready() already found lake");

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
    assert!(
        proof.contains(
            "Fifo.SyncParametricProof NonPow2Queue_fifo NonPow2Queue_fifo_sync_equations"
        ),
        "expected DEPTH=3 FIFO certificate to include parametric FIFO proof:\n{proof}"
    );
    let bad_proof = proof
        .replace(
            "(wrPtr + 1) % Fifo.ptrMod NonPow2Queue_fifo",
            "(wrPtr + 1) % NonPow2Queue_fifo.depth",
        )
        .replace(
            "(rdPtr + 1) % Fifo.ptrMod NonPow2Queue_fifo",
            "(rdPtr + 1) % NonPow2Queue_fifo.depth",
        );
    assert_ne!(
        proof, bad_proof,
        "expected proof mutation to change pointer wrap"
    );
    std::fs::write(&bad_proof_path, bad_proof).expect("write bad proof");

    let output = Command::new(&lake)
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
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/sva_temporal_proves.arch", &["--bound", "5"]);
    assert_eq!(code, 0, "expected exit 0; got {code}\n{out}");
    assert!(
        out.contains("gnt_follows_req"),
        "missing property name in output:\n{out}"
    );
    assert!(
        out.contains("req_implies_next_gnt"),
        "missing |=> property:\n{out}"
    );
    // Both asserts should PROVE; cover should HIT.
    let proved = out.matches("PROVED").count();
    assert!(proved >= 2, "expected ≥2 PROVED (got {proved}):\n{out}");
    assert!(out.contains("HIT"), "expected cover HIT:\n{out}");
}

#[test]
fn formal_sva_temporal_refutes() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/sva_temporal_refutes.arch", &["--bound", "5"]);
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"), "expected REFUTED:\n{out}");
}

#[test]
fn formal_sva_phase2_proves() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
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
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/sva_phase2_refutes.arch", &["--bound", "8"]);
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"), "expected REFUTED:\n{out}");
}

#[test]
fn formal_vacuity_guard_rejects_contradictory_assumes() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    // Contradictory assumes → VACUOUS (exit 1), never a false PROVED. This
    // is a general arch-formal soundness guard, exercised here on an
    // integer-only design to make its flow-level scope explicit.
    let (code, out) = run_formal("tests/formal/vacuous_assumes.arch", &["--bound", "2"]);
    assert!(out.contains("VACUOUS"), "expected VACUOUS:\n{out}");
    assert!(!out.contains("PROVED"), "must not report PROVED:\n{out}");
    assert_eq!(
        code, 1,
        "vacuous proof must be a hard failure (exit 1); got {code}\n{out}"
    );

    // A satisfiable assume must still prove normally — no false vacuity.
    let (code, out) = run_formal("tests/formal/satisfiable_assumes.arch", &["--bound", "2"]);
    assert_eq!(
        code, 0,
        "satisfiable-assume proof should pass; got {code}\n{out}"
    );
    assert!(out.contains("PROVED"), "expected PROVED:\n{out}");
    assert!(
        !out.contains("VACUOUS"),
        "satisfiable assume must not be flagged vacuous:\n{out}"
    );
}

#[test]
fn formal_vacuity_guard_rejects_unreachable_antecedent() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    // Implication with an unreachable antecedent proves vacuously — flagged
    // VACUOUS (exit 1), even though it needs no `assume` and its consequent
    // is false. This is a distinct vacuity class from unsatisfiable assumes.
    let (code, out) = run_formal("tests/formal/vacuous_implication.arch", &["--bound", "2"]);
    assert!(out.contains("VACUOUS"), "expected VACUOUS:\n{out}");
    assert!(
        out.contains("antecedent is unreachable"),
        "reason should name the cause:\n{out}"
    );
    assert!(!out.contains("PROVED"), "must not report PROVED:\n{out}");
    assert_eq!(
        code, 1,
        "vacuous implication must be a hard failure (exit 1); got {code}\n{out}"
    );

    // Reachable antecedent + true consequent proves normally.
    let (code, out) = run_formal("tests/formal/reachable_implication.arch", &["--bound", "2"]);
    assert_eq!(
        code, 0,
        "reachable-antecedent proof should pass; got {code}\n{out}"
    );
    assert!(
        out.contains("PROVED") && !out.contains("VACUOUS"),
        "expected clean PROVED:\n{out}"
    );
}

#[test]
fn formal_replay_confirms_genuine_refutations() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    // Counterexample replay (the sat-side dual of the vacuity guard) runs on
    // every REFUTED result. On a genuine violation it must CONFIRM: the
    // report stays a plain REFUTED (exit 1) with no inconclusive note and no
    // ENCODING UNSOUND flag. The CONTRADICTED verdict can only be exercised
    // by unit tests (src/formal.rs) — no fixture can make the real encoder
    // emit an unsound query.
    for (fixture, bound) in [
        ("tests/formal/sva_phase2_refutes.arch", "8"),
        ("tests/formal/replay_float_refutes.arch", "2"),
    ] {
        let (code, out) = run_formal(fixture, &["--bound", bound]);
        assert_eq!(
            code, 1,
            "{fixture}: expected exit 1 (REFUTED); got {code}\n{out}"
        );
        assert!(
            out.contains("REFUTED"),
            "{fixture}: expected REFUTED:\n{out}"
        );
        assert!(
            !out.contains("ENCODING UNSOUND"),
            "{fixture}: replay must not false-flag a genuine refutation:\n{out}"
        );
        assert!(
            !out.contains("replay could not decide"),
            "{fixture}: replay should CONFIRM (decidable property), not go inconclusive:\n{out}"
        );
    }

    // Kill-switch: ARCH_FORMAL_NO_REPLAY=1 skips replay entirely — same
    // REFUTED verdict, pre-replay behavior.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arch"));
    cmd.arg("formal")
        .arg("tests/formal/replay_float_refutes.arch")
        .args(["--bound", "2"])
        .env("ARCH_FORMAL_NO_REPLAY", "1");
    let out = cmd.output().expect("failed to spawn arch");
    let merged = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(
        code, 1,
        "kill-switch run should still REFUTE; got {code}\n{merged}"
    );
    assert!(merged.contains("REFUTED"), "expected REFUTED:\n{merged}");
    assert!(
        !merged.contains("ENCODING UNSOUND") && !merged.contains("replay"),
        "kill-switch must disable all replay output:\n{merged}"
    );
}

/// Issue #821: a sub-module `port reg` output was silently dropped during
/// flattening, leaving the parent wire declared but unconstrained — a free
/// variable that produced a SPURIOUS REFUTED on a trivially-true property.
///
/// The pair matters more than either half: a fix that modelled the carried
/// register as a constant, or that declared it without landing the `seq`
/// write, would make the "proves" case pass while silently breaking the
/// "refutes" case. Both directions are asserted.
#[test]
fn formal_hier_port_reg_is_modelled_not_dropped() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    // `o` holds 0 (reset) or 7, so `w <= 7` is true and must PROVE.
    let (code, out) = run_formal(
        "tests/formal/hier_port_reg_proves.arch",
        &["--top", "HierPortReg", "--bound", "4"],
    );
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED:\n{out}");

    // The register genuinely reaches 7, so `w <= 6` is false and must
    // REFUTE — at cycle 1, since cycle 0 is held at the reset value 0.
    // This is the half that catches a fix which merely flips the verdict.
    let (code, out) = run_formal(
        "tests/formal/hier_port_reg_refutes.arch",
        &["--top", "HierPortRegBad", "--bound", "4"],
    );
    assert_eq!(code, 1, "expected exit 1 (REFUTED); got {code}\n{out}");
    assert!(out.contains("REFUTED"), "expected REFUTED:\n{out}");
    assert!(
        out.contains("at cycle 1"),
        "expected the violation at cycle 1 (cycle 0 is reset-held):\n{out}"
    );
}

/// Issue #818: a write to a plain (non-credit_channel) bus signal used to
/// panic in `emit_base` on `self.sigs[tgt]` (exit 101). It must now be a
/// clean "unsupported in v1" compile error.
///
/// Deliberately NOT z3-gated: `preprocess` rejects the design before any
/// solver is invoked, so this runs everywhere.
#[test]
fn formal_plain_bus_field_write_errors_cleanly() {
    let (code, out) = run_formal("tests/formal/bus_field_unsupported.arch", &["--bound", "4"]);
    assert_ne!(code, 101, "arch formal must not panic (issue #818):\n{out}");
    assert!(
        !out.contains("panicked") && !out.contains("no entry found for key"),
        "expected a clean diagnostic, not a panic:\n{out}"
    );
    assert_eq!(
        code, 1,
        "expected exit 1 (compile error); got {code}\n{out}"
    );
    // miette word-wraps the message, so normalize whitespace before
    // matching rather than depending on the wrap points.
    let flat: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("assignment to bus signal `m.valid`"),
        "error should name the FIRST offending write in source order:\n{out}"
    );
    assert!(
        flat.contains("is not supported by `arch formal` v1"),
        "error should name the v1 scope:\n{out}"
    );
    assert!(
        flat.contains("only `credit_channel` signals on a bus port are modelled"),
        "error should say what IS supported:\n{out}"
    );
}

/// E8M0 in `arch formal`: the scale type is not a float, so every
/// float-shaped path must recognise it explicitly. This property is a
/// semantic cross-check — it only holds if `is_nan(s)` tests the 0xFF code
/// AND `s.to_fp32()` widens to a genuine NaN there and to a finite scale
/// everywhere else. Three separate dispatch bugs were caught by it:
/// the FP helper preamble not being requested, `to_fp32` returning the raw
/// 8 bits, and `is_nan` falling through to the f32 bit test.
#[test]
fn formal_e8m0_nan_scale_proves() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let (code, out) = run_formal("tests/formal/e8m0_nan_scale.arch", &["--bound", "2"]);
    assert_eq!(code, 0, "expected exit 0 (PROVED); got {code}\n{out}");
    assert!(out.contains("PROVED"), "expected PROVED:\n{out}");
}
