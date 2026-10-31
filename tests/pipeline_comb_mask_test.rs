// Regression for #1111: `arch sim` did not width-mask an `Assign` in a
// pipeline stage `comb` block. The seq-block emitter
// (`emit_pipeline_sim_stmt`) masked the assigned value to the target's
// declared width (`(val) & 0x..ULL`), but the comb-block emitter
// (`emit_pipeline_sim_comb_stmt`) emitted a bare `tgt = val;`. An
// overflowing/wrapping value (`a +% b`) therefore kept its extra high bits
// in native `arch sim`, while `arch build` (SV) truncated to the declared
// width — a sim≠SV miscompile.
//
// The repro computes `wsum = ra +% rb` on `UInt<12>` (non-native width;
// stored in a `uint16_t`, so C++ does not truncate it natively — an 8-bit
// type would hide the bug behind `uint8_t` truncation). With `ra = 4000`,
// `rb = 200` the wrapping sum is `4200`, which masks to `4200 & 0xFFF = 104`
// at the declared 12-bit width. Pre-fix the native sim read `4200`.
//
// Two load-bearing pieces of the design:
//   * `acc <= wsum` in the seq block is what makes the *sim* width-map infer
//     `wsum` as 12 bits (consumer-based inference). Without a 12-bit consumer
//     the implicit wire defaults to `UInt<32>` and the mask is a no-op.
//   * the `else wsum = ra;` (bare-ident) branch is what makes *SV*
//     (`resolve_comb_wire_type`) declare `s0_wsum` as `logic [11:0]`; without
//     it SV defaults the arith-RHS comb wire to a 1-bit `logic`.
use std::process::Command;

const PIPE_MASK: &str = r#"
pipeline PipeMask
  port clk:   in Clock<SysDomain>;
  port reset: in Reset<Sync, High>;
  port a: in UInt<12>;
  port b: in UInt<12>;
  port sel: in Bool;
  port start: in Bool;
  port result: out UInt<12>;

  stage S0
    reg ra: UInt<12> reset reset => 0;
    reg rb: UInt<12> reset reset => 0;
    reg sel_r: Bool reset reset => 0;
    reg acc: UInt<12> reset reset => 0;
    comb
      if sel_r
        wsum = ra +% rb;
      else
        wsum = ra;
      end if
      result = wsum;
    end comb
    seq on clk rising
      valid_r <= start;
      ra <= a;
      rb <= b;
      sel_r <= sel;
      acc <= wsum;
    end seq
  end stage S0
end pipeline PipeMask
"#;

#[test]
fn pipeline_stage_comb_assign_masks_in_sv() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("PipeMask.arch");
    std::fs::write(&source, PIPE_MASK).expect("write source");

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

    let sv = std::fs::read_to_string(temp.path().join("PipeMask.sv")).expect("read SV");
    // SV carries the declared 12-bit width on the comb wire, so the
    // wrapping-add result is truncated to [11:0] in hardware.
    assert!(
        sv.contains("logic [11:0] s0_wsum;"),
        "expected a 12-bit comb-wire declaration in the emitted SV:\n{sv}"
    );
}

#[test]
fn pipeline_stage_comb_assign_masks_in_native_sim() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("PipeMask.arch");
    let tb = temp.path().join("tb.cpp");
    std::fs::write(&source, PIPE_MASK).expect("write source");
    std::fs::write(
        &tb,
        r#"
#include "VPipeMask.h"
#include <cassert>
int main() {
  VPipeMask dut;
  // Reset.
  dut.reset = 1; dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval();
  // Drive a = 4000, b = 200, sel = 1; two edges latch ra/rb/sel_r.
  dut.reset = 0; dut.start = 1; dut.sel = 1; dut.a = 4000; dut.b = 200;
  dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
  dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
  dut.clk = 0; dut.eval();
  // wsum = ra +% rb = 4200, masked to the declared 12-bit width = 104.
  // Pre-fix the comb block kept the extra bits and this read 4200.
  assert(dut.result == 104);
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
        "pipeline stage-comb width-mask simulation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
