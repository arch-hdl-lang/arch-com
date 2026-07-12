//! `fma<pipelined, N>` surface + latency typing (proposal phase 2,
//! `doc/proposal_pipelined_operators.md`).
//!
//! Covers: parser accept/reject, registry-miss error text, the
//! binding/consistency mismatch error, the mixed-latency-expression error,
//! comb-context rejection, the delay-line "did you mean" warning, and the
//! worked example end-to-end through `arch check` (codegen is deferred —
//! see the module doc comment on `src/pipelined_ops.rs` — so `arch build`
//! is asserted to fail with the explicit deferred-codegen error, not to
//! succeed).

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

fn run_build(src: &str) -> std::process::Output {
    let (td, path) = write_arch(src);
    let out_sv = td.path().join("M.sv");
    arch()
        .arg("build")
        .arg(&path)
        .arg("-o")
        .arg(&out_sv)
        .output()
        .expect("run arch build")
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

/// Codegen is explicitly deferred (proposal phase 3 — no staged RTL exists
/// yet for `builtin:fma_f32_s6`, only a synthesis-retiming characterization
/// the compiler never sees). `arch build` must refuse loudly, not silently
/// fall back to a comb cone.
#[test]
fn worked_example_build_is_explicitly_deferred() {
    let out = run_build(WORKED_EXAMPLE);
    assert!(
        !out.status.success(),
        "arch build must refuse pipelined-call codegen until phase 3 lands"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        normalize(&stderr).contains("codegen for pipelined operators is not yet implemented"),
        "expected explicit deferred-codegen error, got:\n{stderr}"
    );
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
