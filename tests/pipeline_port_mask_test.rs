// Regression for the #1114 residual: `arch sim` did not width-mask a pipeline
// stage `comb`/`seq` `Assign` whose target is an *output port*. Both emitter
// arms keyed the width-map lookup as `format!("{prefix}_{n}")` (e.g.
// `s0_result`), but output ports are stored in the width map under their
// *bare* name (`result`) and the emitted target is bare for a port too. So the
// mask lookup missed and a wrapping/overflowing value assigned straight to a
// `UInt<W>` output port kept its extra high bits in native sim, while `arch
// build` (SV) truncates it via the result-width cast — a sim≠SV miscompile.
//
// #1114 fixed the wire case (keyed `{prefix}_{n}`, which hits); the output-port
// case fell through because the port key is bare. #1114's own PR body flagged
// this as a known residual ("a direct `result = a +% b;` to an output port
// stays unmasked even after this fix").
//
// The repro computes `result = ra +% rb` on `UInt<12>` (non-native width;
// `result` is stored in a `uint16_t`, so C++ does not truncate it natively).
// `ra`/`rb` are stage registers (so the pipeline accepts the output coming from
// a registered path, not a bare input), driven from `a = 4000`, `b = 200`. The
// wrapping sum is `4200`, which masks to `4200 & 0xFFF = 104` at the declared
// 12-bit width. Pre-fix the native sim read `4200`.
use std::process::Command;

const PIPE_PORT_MASK: &str = r#"
pipeline PipePortMask
  port clk:   in Clock<SysDomain>;
  port reset: in Reset<Sync, High>;
  port a: in UInt<12>;
  port b: in UInt<12>;
  port result: out UInt<12>;

  stage S0
    reg ra: UInt<12> reset reset => 0;
    reg rb: UInt<12> reset reset => 0;
    comb
      result = ra +% rb;
    end comb
    seq on clk rising
      ra <= a;
      rb <= b;
    end seq
  end stage S0
end pipeline PipePortMask
"#;

#[test]
fn pipeline_output_port_comb_assign_masks_in_native_sim() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("PipePortMask.arch");
    let tb = temp.path().join("tb.cpp");
    std::fs::write(&source, PIPE_PORT_MASK).expect("write source");
    std::fs::write(
        &tb,
        r#"
#include "VPipePortMask.h"
#include <cassert>
int main() {
  VPipePortMask dut;
  // Reset.
  dut.reset = 1; dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval();
  // Drive a = 4000, b = 200; one edge latches ra/rb.
  dut.reset = 0; dut.a = 4000; dut.b = 200;
  dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
  dut.clk = 0; dut.eval();
  // result = ra +% rb = 4200, masked to the declared 12-bit width = 104.
  // Pre-fix the output-port assign kept the extra bits and this read 4200.
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
        "pipeline output-port width-mask simulation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
