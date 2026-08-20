//! CLI-level coverage for `arch build --no-auto-asserts` (issue #649):
//! suppress every compiler-generated `assert property` / `cover property`
//! in the emitted SV (bounds, divide-by-zero, FSM legal-state/
//! reachability/transition, FIFO overflow/underflow, guard contracts,
//! and `--auto-thread-asserts` thread-lowering properties).
//!
//! Scope decision (documented prominently in the PR description): the
//! issue's "Expected behavior" section only ever describes suppressing
//! *generated* SVA ("Generated `assert property` / `cover property`
//! blocks are omitted", "cover generated FSM legal-state/reachability/
//! transition coverage and any other auto-generated SVA") and explicitly
//! keeps "User-authored synthesizable RTL... emitted normally". It never
//! asks to touch user-written `assert`/`cover` items declared inside a
//! module/fsm/fifo/... body. This suite takes that narrower reading:
//! `--no-auto-asserts` suppresses ONLY compiler-generated SVA;
//! `user_assert_and_cover_survive_no_auto_asserts` below pins that a
//! user's own `assert`/`cover` is untouched either way.

use std::path::Path;
use std::process::Command;

fn run_build(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = Command::new(arch_bin)
        .current_dir(dir)
        .arg("build")
        .args(args)
        .output()
        .expect("failed to run `arch build`");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn build_sv(dir: &Path, arch_filename: &str, extra_args: &[&str]) -> String {
    let mut args: Vec<&str> = vec!["-o", "out.sv"];
    args.extend_from_slice(extra_args);
    args.push(arch_filename);
    let (ok, _out, err) = run_build(dir, &args);
    assert!(ok, "arch build failed: {err}");
    std::fs::read_to_string(dir.join("out.sv")).expect("read out.sv")
}

#[test]
fn bounds_div0_and_guard_asserts_suppressed() {
    let source = r#"
module BoundsDiv0Guard
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port idx: in UInt<2>;
  port den: in UInt<8>;
  port num: in UInt<8>;
  port valid_sig: in Bool;
  reg data: Vec<UInt<8>, 4> reset rst => 0;
  reg quot: UInt<8> reset rst => 0;
  reg guarded: UInt<8> guard valid_sig;
  seq on clk rising
    data[idx] <= num;
    quot <= num / den;
    guarded <= num;
  end seq
end module BoundsDiv0Guard
"#;
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("m.arch"), source).expect("write arch");

    let default_sv = build_sv(td.path(), "m.arch", &[]);
    assert!(
        default_sv.contains("_auto_bound_vec_"),
        "default build must emit the Vec bounds assertion:\n{default_sv}"
    );
    assert!(
        default_sv.contains("_auto_div0_"),
        "default build must emit the divide-by-zero assertion:\n{default_sv}"
    );
    assert!(
        default_sv.contains("_guard_contract"),
        "default build must emit the guard-contract assertion:\n{default_sv}"
    );
    // Functional RTL must be present regardless.
    assert!(default_sv.contains("always_ff"), "got:\n{default_sv}");

    let suppressed_sv = build_sv(td.path(), "m.arch", &["--no-auto-asserts"]);
    assert!(
        !suppressed_sv.contains("_auto_bound_vec_"),
        "--no-auto-asserts must drop the Vec bounds assertion:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("_auto_div0_"),
        "--no-auto-asserts must drop the divide-by-zero assertion:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("_guard_contract"),
        "--no-auto-asserts must drop the guard-contract assertion:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("assert property"),
        "--no-auto-asserts must leave no `assert property` text at all in a design \
         with no user-written asserts:\n{suppressed_sv}"
    );
    // Functional RTL is unaffected by the flag.
    assert!(suppressed_sv.contains("always_ff"), "got:\n{suppressed_sv}");
}

#[test]
fn fsm_auto_asserts_suppressed() {
    let source = r#"
fsm Tri
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port go: in Bool;
  port q: out UInt<2>;
  state [A, B, C]
  default state A;
  default seq on clk rising;
  state A
    comb
      q = 0;
    end comb
    -> B when go;
  end state A
  state B
    comb
      q = 1;
    end comb
    -> C when go;
  end state B
  state C
    comb
      q = 2;
    end comb
    -> A when go;
  end state C
end fsm Tri
"#;
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("m.arch"), source).expect("write arch");

    let default_sv = build_sv(td.path(), "m.arch", &[]);
    assert!(
        default_sv.contains("_auto_legal_state"),
        "got:\n{default_sv}"
    );
    assert!(default_sv.contains("_auto_reach_"), "got:\n{default_sv}");
    assert!(default_sv.contains("_auto_tr_"), "got:\n{default_sv}");

    let suppressed_sv = build_sv(td.path(), "m.arch", &["--no-auto-asserts"]);
    assert!(
        !suppressed_sv.contains("_auto_legal_state"),
        "got:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("_auto_reach_"),
        "got:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("_auto_tr_"),
        "got:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("assert property") && !suppressed_sv.contains("cover property"),
        "got:\n{suppressed_sv}"
    );
    // The state machine logic itself is unaffected.
    assert!(suppressed_sv.contains("state_r"), "got:\n{suppressed_sv}");
}

#[test]
fn fifo_auto_asserts_suppressed() {
    let source = r#"
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
"#;
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("m.arch"), source).expect("write arch");

    let default_sv = build_sv(td.path(), "m.arch", &[]);
    assert!(
        default_sv.contains("_auto_no_overflow"),
        "got:\n{default_sv}"
    );
    assert!(
        default_sv.contains("_auto_no_underflow"),
        "got:\n{default_sv}"
    );

    let suppressed_sv = build_sv(td.path(), "m.arch", &["--no-auto-asserts"]);
    assert!(
        !suppressed_sv.contains("_auto_no_overflow"),
        "got:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("_auto_no_underflow"),
        "got:\n{suppressed_sv}"
    );
    assert!(
        !suppressed_sv.contains("assert property"),
        "got:\n{suppressed_sv}"
    );
}

#[test]
fn user_assert_and_cover_survive_no_auto_asserts() {
    // Narrower-reading pin: --no-auto-asserts must NOT touch user-written
    // assert/cover items.
    let source = r#"
module UserAssertCover
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port a: in Bool;
  port b: in Bool;
  assert ab_consistent: a |-> b;
  cover seen_a: a;
end module UserAssertCover
"#;
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("m.arch"), source).expect("write arch");

    let default_sv = build_sv(td.path(), "m.arch", &[]);
    assert!(
        default_sv.contains("ab_consistent: assert property"),
        "got:\n{default_sv}"
    );
    assert!(
        default_sv.contains("seen_a: cover property"),
        "got:\n{default_sv}"
    );

    let suppressed_sv = build_sv(td.path(), "m.arch", &["--no-auto-asserts"]);
    assert!(
        suppressed_sv.contains("ab_consistent: assert property"),
        "user-written assert must survive --no-auto-asserts:\n{suppressed_sv}"
    );
    assert!(
        suppressed_sv.contains("seen_a: cover property"),
        "user-written cover must survive --no-auto-asserts:\n{suppressed_sv}"
    );
}

#[test]
fn no_auto_asserts_overrides_auto_thread_asserts() {
    // `--no-auto-asserts` is the stronger, more general knob: passing both
    // flags together must still suppress thread-lowering SVA even though
    // --auto-thread-asserts on its own asked for it.
    let source = include_str!("thread/wait_cycles.arch");
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(td.path().join("m.arch"), source).expect("write arch");

    let with_thread_asserts = build_sv(td.path(), "m.arch", &["--auto-thread-asserts"]);
    assert!(
        with_thread_asserts.contains("_auto_thread_"),
        "--auto-thread-asserts alone must emit thread SVA:\n{with_thread_asserts}"
    );

    let both = build_sv(
        td.path(),
        "m.arch",
        &["--auto-thread-asserts", "--no-auto-asserts"],
    );
    assert!(
        !both.contains("_auto_thread_"),
        "--no-auto-asserts must override --auto-thread-asserts:\n{both}"
    );
}
