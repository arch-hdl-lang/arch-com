//! Regression coverage for contextual `onehot` typing (I02-2).

use std::process::Command;

use std::io::Write;
use tempfile::NamedTempFile;

fn arch_check(source: &str) -> std::process::Output {
    let mut file = NamedTempFile::new().expect("create ARCH fixture");
    file.write_all(source.as_bytes())
        .expect("write ARCH fixture");
    Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["check", file.path().to_str().unwrap()])
        .output()
        .expect("run arch check")
}

#[test]
fn onehot_uses_assignment_context_and_handles_wide_indices() {
    let source = r#"
module OnehotRegression
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port idx: in UInt<32>;
  port y: out UInt<8>;
  reg q: UInt<8> reset rst => 0;
  let typed: UInt<8> = onehot(idx);
  comb
    y = onehot(idx);
  end comb
  seq on clk rising
    q <= onehot(idx);
  end seq
end module OnehotRegression
"#;
    let output = arch_check(source);
    assert!(
        output.status.success(),
        "contextual onehot should check successfully:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn onehot_preserves_index_name_diagnostics() {
    let source = r#"
module OnehotBadIndex
  port y: out UInt<8>;
  comb
    y = onehot(unknown_index);
  end comb
end module OnehotBadIndex
"#;
    let output = arch_check(source);
    assert!(!output.status.success(), "undefined onehot index must fail");
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostics.contains("unknown_index"),
        "diagnostic should retain the index name:\n{diagnostics}"
    );
}
