// Regression for #1108: `arch sim` silently dropped a `for` loop inside a
// pipeline stage `comb` block (`emit_pipeline_sim_comb_stmt` had no `For`
// arm), so the loop body never ran and the driven signal kept its pre-loop
// value — while `arch build` (SV) emitted the loop. A sim≠SV miscompile.
//
// The loop below computes `sum = 4 * base`. Pre-fix the sim held `sum == 0`.
use std::process::Command;

const PIPE_FOR: &str = r#"
pipeline PipeFor
  port clk:   in Clock<SysDomain>;
  port reset: in Reset<Sync, High>;
  port x: in UInt<8>;
  port start: in Bool;
  port sum: out UInt<8>;

  stage S0
    reg base: UInt<8> reset reset => 0;
    comb
      sum = 0;
      for i in 0..3
        sum = sum +% base;
      end for
    end comb
    seq on clk rising
      valid_r <= start;
      base <= x;
    end seq
  end stage S0
end pipeline PipeFor
"#;

#[test]
fn pipeline_stage_comb_for_loop_emitted_in_sv() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("PipeFor.arch");
    std::fs::write(&source, PIPE_FOR).expect("write source");

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

    let sv = std::fs::read_to_string(temp.path().join("PipeFor.sv")).expect("read SV");
    assert!(
        sv.contains("for (int i = 0; i <= 3; i++)"),
        "expected the stage-comb for loop in the emitted SV:\n{sv}"
    );
}

#[test]
fn pipeline_stage_comb_for_loop_runs_in_native_sim() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("PipeFor.arch");
    let tb = temp.path().join("tb.cpp");
    std::fs::write(&source, PIPE_FOR).expect("write source");
    std::fs::write(
        &tb,
        r#"
#include "VPipeFor.h"
#include <cassert>
int main() {
  VPipeFor dut;
  // Reset.
  dut.reset = 1; dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval();
  // Drive x = 5, release reset; two edges latch base <= 5.
  dut.reset = 0; dut.start = 1; dut.x = 5;
  dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
  dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
  // The stage-comb loop computes sum = 4 * base = 20. Pre-fix it was 0.
  assert(dut.sum == 20);
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
        "pipeline stage-comb for-loop simulation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
