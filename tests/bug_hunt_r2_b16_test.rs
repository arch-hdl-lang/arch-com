//! Regression for ceil_pow2 at the top finite FP32 binade (B16/I09).
//!
//! The preliminary exponent used to choose an E8M0 scale may be 0xFF even
//! though the final scale, after subtracting the element top-binade exponent,
//! is finite.  Keep the emitted arithmetic visible here so this regression
//! remains active even on machines without a native simulator or Icarus.

use std::fs;
use std::path::Path;
use std::process::Command;

const SRC: &str = r#"
package Formats
  type B = ScaledVec<FP4E2M1, 1, E8M0>;
end package Formats
module CeilTop
  port v: in Vec<FP32, 1>;
  port q: out B;
  port back: out Vec<FP32, 1>;
  comb
    q = scaled_quantize<B, ceil_pow2, rne>(v);
    back = scaled_dequantize(q);
  end comb
end module CeilTop
"#;

const CPP_TB: &str = r#"
#include "VCeilTop.h"
#include <cstdint>
int main() {
  VCeilTop dut;
  const uint32_t inputs[] = {
      0x7effffffu, // below the top binade
      0x7f000000u, // exact 2^127
      0x7f000001u, // first non-power value in the top binade
      0x7f600000u, // 3.5 after scaling: ties to even code 6
      0x7f7fffffu, // largest finite FP32 value
  };
  const uint32_t expected[] = {0xfc6u, 0xfc6u, 0xfd4u, 0xfd6u, 0xfd6u};
  for (unsigned i = 0; i < sizeof(inputs) / sizeof(inputs[0]); ++i) {
    dut.v[0] = inputs[i];
    dut.eval_comb();
    if (static_cast<uint32_t>(dut.q) != expected[i]) return static_cast<int>(i + 1);
  }
  return 0;
}
"#;

const SV_TB: &str = r#"
module tb;
  logic [31:0] v;
  wire [11:0] q;
  wire [31:0] back;
  CeilTop dut(.v(v), .q(q), .back(back));
  initial begin
    v=32'h7f600000;
    #1;
    if(q !== 12'hfd6) $fatal(1, "wrong ceil_pow2 scale");
    if(back !== 32'h7f7fffff) $fatal(1, "wrong saturated dequantization");
    $finish;
  end
endmodule
"#;

const STAGED_SRC: &str = r#"
package Formats
  type B = ScaledVec<FP4E2M1, 1, E8M0>;
end package Formats
module CeilTopStaged
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port v: in Vec<FP32, 1>;
  port q: out pipe_reg<B, 5> reset rst => 0;
  seq on clk rising
    q@5 <= scaled_quantize<B, pipelined, 5>(v);
  end seq
end module CeilTopStaged
"#;

fn available(tool: &str, version_arg: &str) -> bool {
    Command::new(tool)
        .arg(version_arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn write_fixture(dir: &Path) {
    fs::write(dir.join("CeilTop.arch"), SRC).unwrap();
}

#[test]
fn b16_ceil_pow2_top_binade_emission_and_controls() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let arch = env!("CARGO_BIN_EXE_arch");
    let sv = dir.path().join("CeilTop.sv");
    let build = Command::new(arch)
        .args(["build"])
        .arg(dir.path().join("CeilTop.arch"))
        .args(["-o"])
        .arg(&sv)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "arch build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let emitted = fs::read_to_string(&sv).unwrap();
    assert!(
        emitted.contains("if (amax > arch_e8m0_to_f32(ecode)) ecode = ecode + 8'h01;"),
        "ceil must operate on the preliminary exponent without a 0xFE guard"
    );
    assert!(
        emitted.contains("if (code > 8'hFE) code = 8'hFE;"),
        "the final E8M0 scale must be clamped after element-exponent subtraction"
    );

    let staged_src = dir.path().join("CeilTopStaged.arch");
    let staged_sv = dir.path().join("CeilTopStaged.sv");
    fs::write(&staged_src, STAGED_SRC).unwrap();
    let staged_build = Command::new(arch)
        .args(["build"])
        .arg(&staged_src)
        .args(["-o"])
        .arg(&staged_sv)
        .arg("--staged-ops")
        .output()
        .unwrap();
    assert!(
        staged_build.status.success(),
        "staged arch build failed:\n{}",
        String::from_utf8_lossy(&staged_build.stderr)
    );
    let staged_emitted = fs::read_to_string(&staged_sv).unwrap();
    assert!(
        staged_emitted.contains("if (code > 8'hFE) code = 8'hFE;"),
        "staged final scale must be clamped (the pipelined form currently uses its default floor policy)"
    );

    if available("iverilog", "-V") && available("vvp", "-V") {
        fs::write(dir.path().join("tb.sv"), SV_TB).unwrap();
        let compile = Command::new("iverilog")
            .args(["-g2012", "-s", "tb", "-o"])
            .arg(dir.path().join("sv-sim"))
            .arg(&sv)
            .arg(dir.path().join("tb.sv"))
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "iverilog failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new("vvp")
            .arg(dir.path().join("sv-sim"))
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "Icarus regression failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    } else {
        eprintln!("iverilog/vvp not available; skipping executable-SV B16 regression");
    }

    if available("c++", "--version") {
        fs::write(dir.path().join("tb.cpp"), CPP_TB).unwrap();
        let run = Command::new(arch)
            .args(["sim"])
            .arg(dir.path().join("CeilTop.arch"))
            .args(["--tb"])
            .arg(dir.path().join("tb.cpp"))
            .args(["--outdir"])
            .arg(dir.path().join("native"))
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "native B16 controls failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
    } else {
        eprintln!("c++ not in PATH; skipping native B16 controls");
    }
}
