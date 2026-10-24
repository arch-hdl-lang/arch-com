//! Regression for native-sim type recovery through E8M0 ternary receivers.

use std::process::Command;

#[test]
fn e8m0_ternary_receiver_uses_scale_conversion() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("VerifyE8Ternary.arch");
    let tb_path = td.path().join("tb.cpp");
    std::fs::write(
        &arch_path,
        r#"module VerifyE8Ternary
  port c: in Bool;
  port d: in Bool;
  port a: in E8M0;
  port b: in E8M0;
  port selected: out FP32;
  port nested: out FP32;
  port direct: out FP32;
  comb
    selected = (c ? a : b).to_fp32();
    nested = (c ? (d ? a : b) : b).to_fp32();
    direct = c ? a.to_fp32() : b.to_fp32();
  end comb
end module VerifyE8Ternary
"#,
    )
    .unwrap();
    std::fs::write(
        &tb_path,
        r#"#include "VVerifyE8Ternary.h"
#include <cassert>

static void check(VVerifyE8Ternary& d, unsigned selected, unsigned nested, unsigned direct) {
  d.eval();
  assert(d.selected == selected);
  assert(d.nested == nested);
  assert(d.direct == direct);
}

int main() {
  VVerifyE8Ternary d;
  // E8M0 127 is 2^(127-127) = 1.0; 128 is 2^(128-127) = 2.0.
  d.a = 127;
  d.b = 128;
  d.c = 1;
  d.d = 1;
  check(d, 0x3f800000u, 0x3f800000u, 0x3f800000u);
  d.d = 0;
  check(d, 0x3f800000u, 0x40000000u, 0x3f800000u);
  d.c = 0;
  check(d, 0x40000000u, 0x40000000u, 0x40000000u);
  return 0;
}
"#,
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["sim", arch_path.to_str().unwrap(), "--tb"])
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path().join("sim"))
        .output()
        .expect("run arch sim");
    assert!(
        out.status.success(),
        "E8M0 ternary receiver simulation should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
