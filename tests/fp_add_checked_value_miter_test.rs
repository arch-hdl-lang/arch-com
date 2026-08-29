//! Value-equivalence of the CHECKED f32 add (`arch_f32_add_checked`, PR #966) to
//! plain `arch_f32_add`, over ALL inputs.
//!
//! `arch_f32_add_checked(a,b)[31:0]` is a copy-paste of `arch_f32_add`'s body
//! (and it routes rounding through `normround_flags`, whose `result` is a
//! copy-paste of `normround`). `tests/fp_add_checked_test.rs` only samples ~64
//! input pairs, so a future rounding fix that lands in one copy but not the
//! other would slip through as a silent divergence on the value path users
//! actually read. This test closes that trap: it drives
//! `tests/fp_v1/smt_proof/add_checked_miter.sh`, which proves
//!
//!     arch_f32_add_checked(a, b)[31:0] == arch_f32_add(a, b)   for all a, b
//!
//! as a pure QF_BV miter over the two `render_smt` define-funs. Both sides carry
//! no multiplier, so — unlike the fma/staged miters that need an alignment
//! case-split and are gated to a smoke slice — z3 discharges the FULL proof in
//! well under a second, so this runs the whole thing under `cargo test`.
//!
//! Skips cleanly when z3 or the `dump_fp` example binary is absent (like the
//! staged-miter and lockstep tests skip without their tools).

use std::path::PathBuf;
use std::process::Command;

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

fn dump_fp_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_arch"))
        .parent()
        .unwrap()
        .join("examples")
        .join("dump_fp")
}

/// Run the miter script with `env` overrides, returning (success, stdout, stderr).
fn run_miter(extra_env: &[(&str, &str)]) -> Option<(bool, String, String)> {
    if !tool_ok("z3") {
        eprintln!("skipping: z3 not available");
        return None;
    }
    let dump_fp = dump_fp_bin();
    if !dump_fp.exists() {
        eprintln!("skipping: dump_fp example not built (cargo build --example dump_fp)");
        return None;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let script = repo_root().join("tests/fp_v1/smt_proof/add_checked_miter.sh");
    let mut cmd = Command::new("bash");
    cmd.arg(&script)
        .arg(td.path())
        .env("ARCH_BIN", env!("CARGO_BIN_EXE_arch"))
        .env("DUMP_FP_BIN", &dump_fp)
        .env("MITER_TIMEOUT", "300");
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run add_checked_miter.sh");
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// The full proof: `arch_f32_add_checked(a,b)[31:0] == arch_f32_add(a,b)` for
/// all inputs is `unsat`.
#[test]
fn checked_value_equals_plain_add_all_inputs() {
    let Some((ok, stdout, stderr)) = run_miter(&[]) else {
        return;
    };
    assert!(
        ok && stdout.contains("unsat"),
        "value-equivalence miter FAILED — arch_f32_add_checked value path has \
         diverged from arch_f32_add (the copy-paste trap PR #966 warns about):\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Non-vacuity: flip one bit of the checked value and the same miter must become
/// `sat`. Proves the `unsat` above is a real equivalence, not a vacuously-unsat
/// setup that would swallow a genuine divergence.
#[test]
fn checked_value_miter_is_non_vacuous() {
    let Some((ok, stdout, stderr)) = run_miter(&[("MITER_NONVACUITY", "1")]) else {
        return;
    };
    assert!(
        ok && stdout.contains("sat") && !stdout.contains("unsat"),
        "non-vacuity check FAILED — a one-bit corruption of the checked value \
         did not make the miter sat; the miter may be vacuously unsat:\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// The underflow flag (bit 33) is NEVER set, for any (a, b). For f32 ADD a
/// subnormal result is always exact — every finite f32 is an integer multiple of
/// 2^-149 and the subnormal grid is 2^-149, so an exact sum landing in the
/// subnormal range is exactly representable (Hauser/Sterbenz "subnormal add is
/// exact"). The inexact-gated underflow therefore cannot fire: it is dead logic
/// for the ADD checked op. This test pins that property (the reviewer expected a
/// "subnormal rounds up to min-normal, inexact" case to set underflow — no such
/// input exists for add), and would flag any future change that reuses
/// `normround_flags` in a way that makes ADD's underflow bit reachable.
#[test]
fn underflow_flag_is_unreachable_for_add() {
    let Some((ok, stdout, stderr)) = run_miter(&[("MITER_UNDERFLOW_UNREACHABLE", "1")]) else {
        return;
    };
    assert!(
        ok && stdout.contains("unsat"),
        "underflow-unreachability proof FAILED — some (a,b) sets the underflow \
         flag on arch_f32_add_checked, which should be impossible for f32 add:\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
