//! `fma<pipelined, N>` surface + latency typing (proposal phase 2) and its
//! codegen binding (proposal phase 3), `doc/proposal_pipelined_operators.md`.
//!
//! Covers: parser accept/reject, registry-miss error text, the
//! binding/consistency mismatch error, the mixed-latency-expression error,
//! comb-context rejection, the delay-line "did you mean" warning, and the
//! worked example end-to-end through `arch check` AND (phase 3) `arch
//! build`, which now binds the call to comb `fma` + the pipe_reg register
//! cascade — see the module doc comment on `src/pipelined_ops.rs`.
//! Cross-backend (sim ⇄ Verilator) lock-step equivalence and the
//! latency-exactness check live in `tests/pipelined_fma_lockstep_test.rs`.

use std::io::Write;
use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

/// Writes `src` to a temp `.arch` file and returns (tempdir, path) — the
/// tempdir must be kept alive for the path to remain valid.
fn write_arch(src: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("M.arch");
    let mut f = std::fs::File::create(&path).expect("create temp .arch");
    f.write_all(src.as_bytes()).expect("write temp .arch");
    (td, path)
}

fn run_check(src: &str) -> std::process::Output {
    let (_td, path) = write_arch(src);
    arch()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run arch check")
}

/// miette word-wraps rendered error text (inserting a `│` continuation
/// marker at the start of each wrapped line), so a literal multi-word
/// substring can straddle a line break. Drop the continuation markers and
/// collapse all whitespace runs to a single space before substring-matching
/// so assertions are wrap-insensitive.
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .filter(|tok| *tok != "│")
        .collect::<Vec<_>>()
        .join(" ")
}

const WORKED_EXAMPLE: &str = r#"
module DotProductStep
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a:   in FP32;
  port b:   in FP32;
  port acc_in: in FP32;
  port acc_out: out pipe_reg<FP32, 6>;

  seq on clk rising
    acc_out@6 <= fma<pipelined, 6>(a, b, acc_in);
  end seq
end module DotProductStep
"#;

/// The proposal's worked example: `arch check` accepts the surface,
/// resolves the registry entry, and validates the `@6` tap against the
/// declared depth `6`.
#[test]
fn worked_example_passes_check() {
    let out = run_check(WORKED_EXAMPLE);
    assert!(
        out.status.success(),
        "worked example should pass `arch check`\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Phase 3: `arch build` now binds `fma<pipelined, 6>` to the comb `fma`
/// helper (`arch_fma_f32`) feeding the 6-deep pipe_reg register cascade —
/// see `src/pipelined_ops.rs` module docs for why no bespoke staged-datapath
/// codegen is needed (the cascade already exists for ordinary `pipe_reg`
/// ports; this just supplies its stage-1 input from the comb operator
/// instead of a plain `let`/assignment).
#[test]
fn worked_example_builds_comb_plus_cascade() {
    let (_td, path) = write_arch(WORKED_EXAMPLE);
    let sv_path = path.with_extension("sv");
    let out = arch()
        .arg("build")
        .arg(&path)
        .arg("-o")
        .arg(&sv_path)
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build should bind fma<pipelined, 6> to comb+cascade (phase 3)\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(&sv_path).expect("read emitted SV");
    // Stage 1 gets the comb call; stages 2..N are pure passthrough; the
    // final `acc_out` register (declared latency-1, per
    // `elaborate::lower_pipe_reg_ports`) is the last stage of the cascade.
    assert!(
        sv.contains("acc_out_stg1 <= arch_fma_f32(a, b, acc_in);"),
        "expected comb fma feeding stage 1, got:\n{sv}"
    );
    for k in 2..=5 {
        assert!(
            sv.contains(&format!(
                "acc_out_stg{k} <= acc_out_stg{prev};",
                prev = k - 1
            )),
            "expected passthrough cascade stage {k}, got:\n{sv}"
        );
    }
    assert!(
        sv.contains("acc_out <= acc_out_stg5;"),
        "expected final tap from stage 5, got:\n{sv}"
    );
    // No leftover `pipelined` surface text should reach the SV — it must
    // have been fully lowered to the plain comb call.
    assert!(
        !sv.contains("pipelined"),
        "no trace of the `<pipelined, N>` surface should reach emitted SV, got:\n{sv}"
    );
}

/// `arch ops` — the registry entry now shows `verified` end-to-end, and
/// codegen actually exists for it (phase 3 closes the loop the registry's
/// `status` field promised).
#[test]
fn arch_ops_shows_verified_fma_row() {
    let out = arch().arg("ops").output().expect("run arch ops");
    assert!(out.status.success(), "arch ops should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fma") && stdout.contains("FP32") && stdout.contains("verified"));
}

/// Bare `fma(a, b, c)` (no `<pipelined, N>`) is completely unaffected —
/// same comb semantics as before this feature landed.
#[test]
fn bare_fma_unaffected() {
    let src = r#"
module M
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port o: out FP32;
  comb
    o = fma(a, b, c);
  end comb
end module M
"#;
    let out = run_check(src);
    assert!(
        out.status.success(),
        "bare fma should still typecheck\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Parser reject: depth must be a compile-time integer literal.
#[test]
fn parser_rejects_non_literal_depth() {
    let src = r#"
module M
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port o: out FP32;
  comb
    o = fma<pipelined, x>(a, b, c);
  end comb
end module M
"#;
    let out = run_check(src);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("requires N to be a compile-time integer literal"),
        "got:\n{stderr}"
    );
}

/// Parser/typecheck reject: `<pipelined, N>` on a callee that isn't a
/// registry-backed operator.
#[test]
fn unknown_callee_rejected() {
    let src = r#"
module M
  port a: in UInt<8>;
  port b: in UInt<8>;
  port o: out UInt<8>;
  comb
    o = foo<pipelined, 6>(a, b);
  end comb
end module M
"#;
    let out = run_check(src);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("`foo` is not a registry-backed pipelined operator"),
        "got:\n{stderr}"
    );
}

/// Registry-miss error text must match `pipelined_ops::LookupMiss`'s
/// `Display` verbatim (reused, not reformatted, per the proposal).
#[test]
fn registry_miss_error_is_verbatim() {
    let src = r#"
module M
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port acc_out: out pipe_reg<FP32, 5>;
  seq on clk rising
    acc_out@5 <= fma<pipelined, 5>(a, b, c);
  end seq
end module M
"#;
    let out = run_check(src);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("no pipelined implementation of fma<FP32> with 5 stages"),
        "got:\n{stderr}"
    );
    assert!(
        normalize(&stderr).contains("available depths: {6} (run `arch ops` to list all)"),
        "got:\n{stderr}"
    );
}

/// Binding mismatch: `acc@6 <= fma<pipelined, 4>(...)` — call depth (4)
/// must equal the tap depth (6).
#[test]
fn binding_latency_mismatch_is_rejected() {
    let src = r#"
module M
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port acc_out: out pipe_reg<FP32, 6>;
  seq on clk rising
    acc_out@6 <= fma<pipelined, 4>(a, b, c);
  end seq
end module M
"#;
    let out = run_check(src);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("latency-4 result bound at @6"),
        "got:\n{stderr}"
    );
}

/// Mixed-expression mismatch: combining a latency-6 tap read with a
/// latency-0 operand in one expression is rejected, naming both cycles.
#[test]
fn mixed_latency_expression_is_rejected() {
    let src = r#"
module M
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port x: in FP32;
  port acc_out: out pipe_reg<FP32, 6>;
  port bad: out FP32;
  seq on clk rising
    acc_out@6 <= fma<pipelined, 6>(a, b, c);
  end seq
  comb
    bad = acc_out@6 + x;
  end comb
end module M
"#;
    let out = run_check(src);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("operands at cycle 6 and cycle 0"),
        "got:\n{stderr}"
    );
}

/// Direct comb-context consumption of a latency-N result is rejected.
#[test]
fn comb_context_consumption_is_rejected() {
    let src = r#"
module M
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port out1: out FP32;
  comb
    out1 = fma<pipelined, 6>(a, b, c);
  end comb
end module M
"#;
    let out = run_check(src);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("cannot be used in a `comb` block"),
        "got:\n{stderr}"
    );
}

/// `let` bindings are always combinational — a latency-N `<pipelined, N>`
/// result cannot be bound there either.
#[test]
fn let_binding_consumption_is_rejected() {
    let src = r#"
module M
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port out1: out FP32;
  let x: FP32 = fma<pipelined, 6>(a, b, c);
  comb
    out1 = x;
  end comb
end module M
"#;
    let out = run_check(src);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("cannot be used in a `let` binding"),
        "got:\n{stderr}"
    );
}

/// Delay-line trap: `acc@6 <= fma(a,b,c)` (comb fma, *not* pipelined) still
/// compiles (unchanged pipe_reg delay-line semantics) but warns.
#[test]
fn delay_line_trap_warns_not_errors() {
    let src = r#"
module M
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port acc_out: out pipe_reg<FP32, 6>;
  seq on clk rising
    acc_out@6 <= fma(a, b, c);
  end seq
end module M
"#;
    let out = run_check(src);
    assert!(
        out.status.success(),
        "delay-line form should still compile\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("did you mean `fma<pipelined, 6>(...)`?"),
        "got:\n{stderr}"
    );
}
