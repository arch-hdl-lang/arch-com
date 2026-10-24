//! Regression for generated TLM argument mux defaults with signed arguments.

use std::process::Command;

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

    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["check", source.to_str().unwrap()])
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "signed TLM argument mux should typecheck\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
