//! Formal latency-equivalence of the retimed staged fma (`fma<pipelined, 6>`,
//! `arch build --staged-ops`) to the single-cycle `arch_fma_f32`.
//!
//! `tests/pipelined_fma_lockstep_test.rs` is the *empirical* half — randomized
//! Verilator-vs-native lockstep, i.e. equivalence on the inputs it happens to
//! sample. This file is the *formal* half, run under `cargo test` whenever the
//! external solver tooling is present (skips cleanly otherwise, like the
//! lockstep test skips without Verilator). It decomposes the equivalence into
//! the two lemmas that `tests/fp_v1/smt_proof/staged_ops_miter.sh` discharges:
//!
//!   * Lemma B (timing) — the staged datapath is a BALANCED feed-forward
//!     pipeline of uniform latency 5 (the extra cycle to the user-visible
//!     latency-6 output is the wrapper's reset/valid register). Structural,
//!     solver-free; this is the property the skew/off-by-one bug class violates.
//!     Always checked here when `yosys` is available.
//!
//!   * Lemma A (arithmetic) — the register-shorted transfer function equals
//!     `arch_fma_f32`. SMT, via the alignment case-split. The full 510-way
//!     split is the manual long-verification (run the script directly); here we
//!     run a bounded SMOKE slice under `cargo test` so a regression in the
//!     emitter or the harness is caught quickly.
//!
//! The full proof (all 510 cases) is intentionally NOT in the default test run
//! — it takes many minutes even parallelized. Run it with:
//!   tests/fp_v1/smt_proof/staged_ops_miter.sh

use std::path::{Path, PathBuf};
use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

fn tool_ok(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const STAGED_FMA_SRC: &str = r#"
module StagedFmaMiter
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port y: out pipe_reg<FP32, 6> reset rst => 0.0;

  seq on clk rising
    y@6 <= fma<pipelined, 6>(a, b, c);
  end seq
end module StagedFmaMiter
"#;

/// Emit the staged fma SV; returns the path to the `.sv`.
fn emit_staged(td: &Path) -> PathBuf {
    let src = td.join("staged_fma.arch");
    std::fs::write(&src, STAGED_FMA_SRC).unwrap();
    let sv = td.join("staged_fma.sv");
    let out = arch()
        .args(["build", "--staged-ops"])
        .arg(&src)
        .arg("-o")
        .arg(&sv)
        .output()
        .expect("run arch build --staged-ops");
    assert!(
        out.status.success(),
        "arch build --staged-ops failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    sv
}

/// Lemma B — the staged fma submodule is a balanced feed-forward pipeline of
/// uniform latency 5. Requires yosys (for the netlist JSON) + python3.
#[test]
fn staged_fma_is_balanced_latency_5() {
    if !tool_ok("yosys") || !tool_ok("python3") {
        eprintln!("skipping: yosys/python3 not available");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let sv = emit_staged(td.path());
    let json = td.path().join("staged.json");

    let ys = format!(
        "read_verilog -sv {}; hierarchy -top ArchF32FmaStaged6; \
         proc; flatten; opt_clean; write_json {}",
        sv.display(),
        json.display()
    );
    let y = Command::new("yosys")
        .args(["-q", "-p", &ys])
        .output()
        .unwrap();
    assert!(
        y.status.success(),
        "yosys failed:\n{}",
        String::from_utf8_lossy(&y.stderr)
    );

    let balance = repo_root().join("tests/fp_v1/synth/pipeline_balance.py");
    let out = Command::new("python3")
        .arg(&balance)
        .arg(&json)
        .arg("ArchF32FmaStaged6")
        .args(["--expect", "5"])
        .output()
        .expect("run pipeline_balance.py");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "Lemma B FAILED — staged fma is not a balanced latency-5 pipeline:\n{}\n{}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("BALANCED: uniform pipeline latency = 5"),
        "unexpected balance output:\n{stdout}"
    );
}

/// Lemma A (smoke) — the register-shorted staged fma equals `arch_fma_f32` on a
/// bounded slice of the alignment split. Requires yosys + z3 + python3 + the
/// `dump_fp` example binary. The full 510-way proof is the manual
/// long-verification (`staged_ops_miter.sh` with no MITER_SMOKE).
#[test]
fn staged_fma_arithmetic_miter_smoke() {
    if !tool_ok("yosys") || !tool_ok("z3") || !tool_ok("python3") {
        eprintln!("skipping: yosys/z3/python3 not available");
        return;
    }
    let dump_fp = PathBuf::from(env!("CARGO_BIN_EXE_arch"))
        .parent()
        .unwrap()
        .join("examples")
        .join("dump_fp");
    if !dump_fp.exists() {
        eprintln!("skipping: dump_fp example not built (cargo build --example dump_fp)");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let script = repo_root().join("tests/fp_v1/smt_proof/staged_ops_miter.sh");
    let out = Command::new("bash")
        .arg(&script)
        .arg(td.path())
        .env("ARCH_BIN", env!("CARGO_BIN_EXE_arch"))
        .env("DUMP_FP_BIN", &dump_fp)
        .env("MITER_SMOKE", "4") // diffs 0..4 + catch-all
        .env("MITER_TIMEOUT", "120")
        .output()
        .expect("run staged_ops_miter.sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "staged_ops_miter.sh (smoke) FAILED:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("balanced@5") && stdout.contains("unsat"),
        "unexpected miter output:\n{stdout}"
    );
}
