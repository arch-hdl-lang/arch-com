// Regression for #1115: `arch build` emitted non-compiling SystemVerilog for a
// wrapping/widening operator (`+%`) used in a pipeline stage `comb` block.
//
// Two coupled defects, both fixed in `src/codegen/pipeline.rs`:
//
//   1. The result-width cast `($bits(lhs) > $bits(rhs) ? ...)'(...)` rendered
//      its `$bits(...)` width operands with the *bare* source names (`ra`),
//      while a pipeline stage renames stage-local signals `<stage>_ra`. The
//      cast named signals that do not exist in the module, so every SV frontend
//      rejected it (`Can't find definition of variable: 'ra'`). The value
//      operands were already prefixed; only the width operands were not.
//
//   2. The implicit comb wire `wsum` was declared 1-bit `logic` because
//      `resolve_comb_wire_type` only types a wire from a bare-ident or
//      cross-stage RHS, not an arithmetic one. Even once (1) let the SV
//      compile, the 12-bit wrapping sum was truncated into a 1-bit net — a
//      silent sim≠SV value bug. The fix mirrors the sim emitter's
//      consumer-based inference (`acc <= wsum` ⇒ `wsum` is `acc`'s width).
//
// This is the pure-arith-RHS case the #1111/#1114 test could not cover: that
// test needed a bare-ident `else wsum = ra;` branch precisely to work around
// defect (2) on the SV side. With both defects fixed, a plain `wsum = ra +% rb`
// resolves correctly on its own.
use std::process::Command;

// `freq_mhz` keeps the `Clock<SysDomain>` domain concrete without a separate
// `domain` file (SysDomain is a built-in domain name).
const PIPE_WRAP: &str = r#"
pipeline pipe_wrap_1115
  port clk:   in Clock<SysDomain>;
  port reset: in Reset<Sync, High>;
  port a: in UInt<12>;
  port b: in UInt<12>;
  port result: out UInt<12>;

  stage S0
    reg ra: UInt<12> reset reset => 0;
    reg rb: UInt<12> reset reset => 0;
    reg acc: UInt<12> reset reset => 0;
    comb
      wsum = ra +% rb;
      result = wsum;
    end comb
    seq on clk rising
      ra <= a;
      rb <= b;
      acc <= wsum;
    end seq
  end stage S0
end pipeline pipe_wrap_1115
"#;

fn verilator_available() -> bool {
    Command::new("verilator")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_sv() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("pipe_wrap_1115.arch");
    let sv_out = temp.path().join("out.sv");
    std::fs::write(&source, PIPE_WRAP).expect("write source");

    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("invoke arch build");
    assert!(
        out.status.success(),
        "arch build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (temp, sv_out)
}

#[test]
fn pipeline_stage_comb_wrap_prefixes_bits_operands_and_sizes_wire() {
    let (_temp, sv_out) = build_sv();
    let sv = std::fs::read_to_string(&sv_out).expect("read emitted SV");

    // Defect (1): the wrapping-add width cast must render stage-prefixed
    // operands, and must NOT name the bare source signals that only exist
    // pre-rewrite.
    assert!(
        !sv.contains("$bits(ra)") && !sv.contains("$bits(rb)"),
        "emitted SV still references the bare (unprefixed) operand names in the \
         width cast — the signals do not exist in the module:\n{sv}"
    );
    assert!(
        sv.contains("s0_wsum = 12'(s0_ra + s0_rb)"),
        "expected the stage-prefixed, declared-width wrapping-add cast:\n{sv}"
    );

    // Defect (2): the implicit comb wire must take the consumer register's
    // 12-bit width, not default to a 1-bit `logic`.
    assert!(
        sv.contains("logic [11:0] s0_wsum;"),
        "expected a 12-bit comb-wire declaration (consumer-based width):\n{sv}"
    );
    assert!(
        !sv.contains("logic s0_wsum;"),
        "comb wire must not default to a 1-bit net:\n{sv}"
    );
}

#[test]
fn pipeline_stage_comb_wrap_elaborates_under_verilator() {
    if !verilator_available() {
        eprintln!(
            "skipping pipeline_stage_comb_wrap_elaborates_under_verilator: verilator not found"
        );
        return;
    }
    let (_temp, sv_out) = build_sv();
    // Pre-fix this failed with "Can't find definition of variable: 'ra'".
    let verilate = Command::new("verilator")
        .args(["--lint-only", "--timing", "-Wno-DECLFILENAME"])
        .arg(&sv_out)
        .output()
        .expect("invoke verilator");
    assert!(
        verilate.status.success(),
        "verilator rejected the emitted SV:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );
}

#[test]
fn pipeline_stage_comb_wrap_native_sim_masks_to_declared_width() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("pipe_wrap_1115.arch");
    let tb = temp.path().join("tb.cpp");
    std::fs::write(&source, PIPE_WRAP).expect("write source");
    std::fs::write(
        &tb,
        r#"
#include "Vpipe_wrap_1115.h"
#include <cassert>
int main() {
  Vpipe_wrap_1115 dut;
  // Reset (active-high).
  dut.reset = 1; dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval();
  // Drive a = 4000, b = 200; two edges latch ra/rb.
  dut.reset = 0; dut.a = 4000; dut.b = 200;
  dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
  dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
  dut.clk = 0; dut.eval();
  // wsum = ra +% rb = 4200, masked to the declared 12-bit width = 104.
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
        "native sim failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
