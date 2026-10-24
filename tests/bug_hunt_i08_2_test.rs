//! Regression coverage for I08-2: native one-hot counter simulation must use
//! the same upward rotation as the SystemVerilog backend.

use std::fs;
use std::process::Command;

#[test]
fn native_one_hot_counter_matches_sv_rotation_and_wraps() {
    let td = tempfile::tempdir().expect("tempdir");
    let source = td.path().join("onehot.arch");
    let tb = td.path().join("onehot_tb.cpp");
    fs::write(
        &source,
        r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain
counter OneHotCounter
  kind one_hot;
  direction: up;
  init: 1;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port inc: in Bool;
  port value: out UInt<4>;
end counter OneHotCounter
"#,
    )
    .expect("write ARCH source");
    fs::write(
        &tb,
        r#"
#include "VOneHotCounter.h"
#include <cstdio>

int main() {
  VOneHotCounter dut;
  auto tick = [&]() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
  };

  dut.rst = 1; dut.inc = 0; tick();
  if (dut.value != 1) { std::fprintf(stderr, "reset=%u\n", dut.value); return 1; }
  dut.rst = 0; dut.inc = 0; tick();
  if (dut.value != 1) { std::fprintf(stderr, "hold=%u\n", dut.value); return 2; }

  const unsigned expected[] = {2, 4, 8, 1};
  for (unsigned i = 0; i < 4; ++i) {
    dut.inc = 1; tick();
    if (dut.value != expected[i]) {
      std::fprintf(stderr, "step=%u actual=%u expected=%u\n", i, dut.value, expected[i]);
      return 3;
    }
  }
  std::puts("PASS one-hot recurrence");
  return 0;
}
"#,
    )
    .expect("write C++ testbench");

    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("sim")
        .arg(&source)
        .arg("--tb")
        .arg(&tb)
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    assert!(
        out.status.success(),
        "one-hot native sim should match the SV recurrence\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS one-hot recurrence"),
        "testbench did not report success: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
