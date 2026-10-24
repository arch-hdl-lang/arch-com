//! Regression for generated TLM argument mux defaults with signed arguments.

use std::process::Command;

fn arch_check(source: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["check", source.to_str().unwrap()])
        .output()
        .expect("run arch check")
}

#[test]
fn signed_tlm_arguments_typecheck_with_generated_mux_default() {
    let td = tempfile::tempdir().expect("tempdir");
    let source = td.path().join("SignedTlm.arch");
    std::fs::write(
        &source,
        r#"bus SignedMem
  tlm_method scale(x: SInt<8>, k: SInt<8>) -> SInt<8>: blocking;
end bus SignedMem

use SignedMem;

module Target
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port s: target SignedMem;
  thread s.scale(x, k) on clk rising, rst high
    return x;
  end thread s.scale
end module Target

module Initiator
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port m: initiator SignedMem;
  port x: in SInt<8>;
  port y: out SInt<8>;
  reg r: SInt<8> reset rst => signed(0);
  thread main on clk rising, rst high
    r <= m.scale(x, signed(3));
  end thread main
  comb y = r; end comb
end module Initiator
"#,
    )
    .expect("write regression source");

    let out = arch_check(&source);
    assert!(
        out.status.success(),
        "signed TLM argument mux should typecheck\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn narrow_float_and_scale_tlm_defaults_typecheck() {
    let td = tempfile::tempdir().expect("tempdir");
    let source = td.path().join("NarrowTlm.arch");
    std::fs::write(
        &source,
        r#"bus NarrowMem
  tlm_method fp4(x: FP4E2M1) -> FP4E2M1: blocking;
  tlm_method fp6(x: FP6E2M3) -> FP6E2M3: blocking;
  tlm_method scale(x: E8M0) -> E8M0: blocking;
end bus NarrowMem

use NarrowMem;

module Target
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port s: target NarrowMem;
  thread s.fp4(x) on clk rising, rst high
    return x;
  end thread s.fp4
  thread s.fp6(x) on clk rising, rst high
    return x;
  end thread s.fp6
  thread s.scale(x) on clk rising, rst high
    return x;
  end thread s.scale
end module Target
"#,
    )
    .expect("write narrow TLM source");

    let out = arch_check(&source);
    assert!(
        out.status.success(),
        "narrow FP and scale TLM defaults should typecheck\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn overwide_signed_tlm_runtime_argument_is_rejected() {
    let td = tempfile::tempdir().expect("tempdir");
    let source = td.path().join("OverwideTlm.arch");
    std::fs::write(
        &source,
        r#"bus SignedMem
  tlm_method scale(x: SInt<8>) -> SInt<8>: blocking;
end bus SignedMem
use SignedMem;
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port m: initiator SignedMem;
  port x: in SInt<16>;
  port y: out SInt<8>;
  let narrowed: SInt<8> = x;
  thread main on clk rising, rst high
    y <= m.scale(x);
  end thread main
end module M
"#,
    )
    .expect("write overwide TLM source");

    let out = arch_check(&source);
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success() && report.contains("SInt<8>"),
        "expected width rejection:\n{report}"
    );
}

#[test]
fn ordinary_signed_ternary_width_mismatch_is_rejected() {
    let td = tempfile::tempdir().expect("tempdir");
    let source = td.path().join("BadTernary.arch");
    std::fs::write(
        &source,
        r#"module M
  port sel: in Bool;
  port a: in SInt<2>;
  port b: in SInt<8>;
  port y: out SInt<8>;
  let picked: SInt<8> = sel ? a : b;
  comb y = picked; end comb
end module M
"#,
    )
    .expect("write bad ternary source");

    let out = arch_check(&source);
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success() && report.contains("ternary branches"),
        "expected ternary rejection:\n{report}"
    );
}
