//! Regression coverage for native counter load priority (I08-1).

use std::process::Command;

#[test]
fn native_counter_load_clear_and_reset_priority() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("LoadCounter.arch");
    let tb_path = td.path().join("load_tb.cpp");
    std::fs::write(
        &arch_path,
        r#"domain SysDomain
  freq_mhz: 100
end domain SysDomain
counter LoadCounter
  kind wrap;
  direction: up;
  init: 0;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port inc: in Bool;
  port clear: in Bool;
  port load: in Bool;
  port load_data: in UInt<4>;
  port value: out UInt<4>;
end counter LoadCounter
"#,
    )
    .expect("write counter fixture");
    std::fs::write(
        &tb_path,
        r#"#include "VLoadCounter.h"
#include <cstdio>

int main() {
  VLoadCounter dut;
  auto tick = [&]() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); };
  auto expect = [&](unsigned value, const char *label) {
    if (dut.value != value) {
      std::fprintf(stderr, "%s: got %u expected %u\n", label,
                   static_cast<unsigned>(dut.value), value);
      return 1;
    }
    return 0;
  };

  // Reset has highest priority over every other control.
  dut.rst = 1; dut.clear = 1; dut.load = 1; dut.load_data = 9; dut.inc = 1;
  tick(); if (expect(0, "reset priority")) return 1;

  dut.rst = 0;
  // Clear wins over load and increment.
  tick(); if (expect(0, "clear priority")) return 1;

  // A load must work without increment, and load wins over increment.
  dut.clear = 0; dut.load = 1; dut.load_data = 9; dut.inc = 0;
  tick(); if (expect(9, "load without inc")) return 1;
  dut.load_data = 3; dut.inc = 1;
  tick(); if (expect(3, "load with inc")) return 1;

  dut.load = 0; dut.inc = 1;
  tick(); if (expect(4, "count after load")) return 1;
  return 0;
}
"#,
    )
    .expect("write testbench");

    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path().join("out"))
        .output()
        .expect("run native counter simulation");
    assert!(
        out.status.success(),
        "native counter load regression failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
