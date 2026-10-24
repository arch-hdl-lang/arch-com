use std::fs;
use std::process::Command;

const GRAY_COUNTER: &str = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

counter GrayCounter
  kind gray;
  direction: up;
  init: 0;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port inc: in Bool;
  port value: out UInt<4>;
end counter GrayCounter
"#;

fn assert_gray_sequence(values: &str) {
    let got: Vec<u32> = values
        .split_whitespace()
        .map(|s| s.parse().expect("numeric counter output"))
        .collect();
    let expected: Vec<u32> = (1..=16)
        .map(|n| {
            let binary = n % 16;
            binary ^ (binary >> 1)
        })
        .collect();
    assert_eq!(
        got, expected,
        "Gray counter sequence differs from binary oracle"
    );
    for pair in got.windows(2) {
        assert_eq!(
            (pair[0] ^ pair[1]).count_ones(),
            1,
            "adjacent Gray states must differ by one bit"
        );
    }
    assert_eq!(
        got[0] ^ got[15],
        1,
        "full-width Gray wrap must differ by one bit"
    );
}

#[test]
fn gray_counter_matches_oracle_in_native_and_sv() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch = td.path().join("GrayCounter.arch");
    fs::write(&arch, GRAY_COUNTER).expect("write ARCH fixture");
    let tb = td.path().join("tb.cpp");
    fs::write(
        &tb,
        r#"
#include "VGrayCounter.h"
#include <cstdio>
int main() {
  VGrayCounter dut;
  dut.rst = 1; dut.inc = 0; dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval();
  dut.rst = 0; dut.inc = 1;
  for (int i = 0; i < 16; ++i) {
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    std::printf("%u ", (unsigned)dut.value);
  }
  std::printf("\n");
  return 0;
}
"#,
    )
    .expect("write native TB");
    let native = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["sim", arch.to_str().unwrap(), "--tb", tb.to_str().unwrap()])
        .arg("--outdir")
        .arg(td.path().join("native"))
        .output()
        .expect("run native simulation");
    assert!(
        native.status.success(),
        "native simulation failed:\n{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_gray_sequence(&String::from_utf8_lossy(&native.stdout));

    if Command::new("verilator").arg("--version").output().is_err() {
        return;
    }
    let sv = td.path().join("GrayCounter.sv");
    let build = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["build", arch.to_str().unwrap(), "-o", sv.to_str().unwrap()])
        .output()
        .expect("build SV");
    assert!(
        build.status.success(),
        "SV build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sv_tb = td.path().join("sv_tb.cpp");
    fs::write(
        &sv_tb,
        r#"
#include "VGrayCounter.h"
#include <cstdio>
int main() {
  VGrayCounter dut; dut.rst = 1; dut.inc = 0; dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval(); dut.rst = 0; dut.inc = 1;
  for (int i = 0; i < 16; ++i) {
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    std::printf("%u ", (unsigned)dut.value);
  }
  std::printf("\n");
}
"#,
    )
    .expect("write SV TB");
    let obj = td.path().join("obj");
    let verilate = Command::new("verilator")
        .args([
            "--cc",
            "--exe",
            "--build",
            "--sv",
            "-Wno-fatal",
            "--top-module",
            "GrayCounter",
            "-Mdir",
        ])
        .arg(&obj)
        .arg(&sv)
        .arg(&sv_tb)
        .output()
        .expect("verilate");
    assert!(
        verilate.status.success(),
        "Verilator failed:\n{}",
        String::from_utf8_lossy(&verilate.stderr)
    );
    let run = Command::new(obj.join("VGrayCounter"))
        .output()
        .expect("run Verilator");
    assert!(
        run.status.success(),
        "Verilator simulation failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_gray_sequence(&String::from_utf8_lossy(&run.stdout));
}
