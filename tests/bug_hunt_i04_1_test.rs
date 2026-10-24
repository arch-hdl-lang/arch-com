use std::process::Command;

const NESTED: &str = r#"
module Nested
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port done: out Bool;
  port count: out UInt<8>;
  reg n: UInt<8> reset rst => 0;
  reg done_r: Bool reset rst => false;
  let done = done_r;
  let count = n;
  thread once on clk rising, rst high
    for i in 0..1
      for j in 0..7
        n <= n +% 8'd1;
        wait 1 cycle;
      end for
    end for
    done_r <= true;
    wait until false;
  end thread once
end module Nested
"#;

#[test]
fn nested_for_counter_width_covers_inner_bound() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("Nested.arch");
    std::fs::write(&source, NESTED).expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["build"])
        .arg(&source)
        .output()
        .expect("run arch build");
    assert!(
        output.status.success(),
        "arch build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let sv = std::fs::read_to_string(temp.path().join("Nested.sv")).expect("read SV");
    assert_eq!(sv.matches("logic [2:0] _t0_loop_cnt_").count(), 2, "{sv}");
}

#[test]
fn nested_for_native_sim_counts_all_iterations() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("Nested.arch");
    let tb = temp.path().join("tb.cpp");
    std::fs::write(&source, NESTED).expect("write source");
    std::fs::write(
        &tb,
        r#"
#include "VNested.h"
#include <cassert>
int main() {
  VNested dut;
  dut.rst = 1; dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval();
  dut.rst = 0;
  for (int k = 0; k < 200; ++k) {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
  }
  assert(dut.done);
  assert(dut.count == 16);
}
"#,
    )
    .expect("write testbench");

    let output = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["sim"])
        .arg(&source)
        .args(["--tb"])
        .arg(&tb)
        .arg("--outdir")
        .arg(temp.path().join("sim"))
        .output()
        .expect("run arch sim");
    assert!(
        output.status.success(),
        "nested loop simulation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
