//! Regression tests for source-order priority in `arch formal` assignments.

use std::process::Command;

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_formal(path: &std::path::Path, bound: &str) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("formal")
        .arg(path)
        .arg("--bound")
        .arg(bound)
        .output()
        .expect("failed to spawn arch");
    (
        output.status.code().unwrap_or(-1),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

fn assert_status(output: &str, name: &str, status: &str) {
    assert!(
        output
            .lines()
            .any(|line| line.contains(name) && line.contains(status)),
        "expected {name} to be {status}:\n{output}"
    );
}

#[test]
fn formal_comb_assignments_keep_last_executed_write() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("CombPriority.arch");
    std::fs::write(
        &path,
        r#"module CombPriority
 port clk: in Clock<SysDomain>;
 port enable: in Bool;
 port nested: in Bool;
 port o: out UInt<8>;
 port overlap: out UInt<8>;
 port final_override: out UInt<8>;
 comb
  o = 8'd1;
  if enable
   if nested
    o = 8'd2;
   end if
  end if
  overlap = 8'd1;
  if enable
   overlap = 8'd2;
  end if
  if nested
   overlap = 8'd3;
  end if
  final_override = 8'd1;
  if enable
   final_override = 8'd2;
  end if
  final_override = 8'd3;
 end comb
 assert wrong_earliest: o == 8'd1;
 assert nested_override: (enable and nested) |-> (o == 8'd2);
 assert overlap_last: (enable and nested) |-> (overlap == 8'd3);
 assert final_unconditional: final_override == 8'd3;
end module CombPriority
"#,
    )
    .expect("write comb fixture");
    let (code, out) = run_formal(&path, "3");
    assert_eq!(code, 1, "expected wrong_earliest to refute:\n{out}");
    assert_status(&out, "wrong_earliest", "REFUTED");
    assert_status(&out, "nested_override", "PROVED");
    assert_status(&out, "overlap_last", "PROVED");
    assert_status(&out, "final_unconditional", "PROVED");
}

#[test]
fn formal_seq_assignments_keep_last_write_and_reset_gate() {
    if !z3_available() {
        eprintln!("skipping: z3 not in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let path = td.path().join("SeqPriority.arch");
    std::fs::write(
        &path,
        r#"module SeqPriority
 port clk: in Clock<SysDomain>;
 port rst: in Reset<Sync>;
 port enable: in Bool;
 reg r: UInt<8> reset rst => 0;
 seq on clk rising
  r <= 8'd1;
  if enable
   r <= 8'd2;
  end if
 end seq
 assert wrong_earliest: r != 8'd2;
 assert enabled_override: (not rst and enable) |=> (r == 8'd2);
 assert reset_wins: rst |=> (r == 8'd0);
end module SeqPriority
"#,
    )
    .expect("write seq fixture");
    let (code, out) = run_formal(&path, "3");
    assert_eq!(code, 1, "expected wrong_earliest to refute:\n{out}");
    assert_status(&out, "wrong_earliest", "REFUTED");
    assert_status(&out, "enabled_override", "PROVED");
    assert_status(&out, "reset_wins", "PROVED");
}
