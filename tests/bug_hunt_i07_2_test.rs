//! Regression coverage for native-sim widening of plain integer arithmetic.

use std::fs;
use std::process::Command;

#[test]
fn native_sim_preserves_widened_add_sub_and_mul() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("bug_hunt_i07_2.arch");
    let tb_path = td.path().join("bug_hunt_i07_2_tb.cpp");
    fs::write(
        &arch_path,
        r#"module BugHuntI072
  port a: in UInt<32>;
  port b: in UInt<32>;
  port add_result: out UInt<33>;
  port sub_result: out UInt<33>;
  port c: in UInt<64>;
  port d: in UInt<64>;
  port add64_result: out UInt<65>;
  port sub64_result: out UInt<65>;
  port add_wrap_result: out UInt<32>;
  port sub_wrap_result: out UInt<32>;
  port mul_result: out UInt<64>;
  port sa: in SInt<32>;
  port sb: in SInt<32>;
  port sadd_result: out SInt<33>;
  port ssub_result: out SInt<33>;
  comb
    add_result = a + b;
    sub_result = a - b;
    add64_result = c + d;
    sub64_result = c - d;
    add_wrap_result = a +% b;
    sub_wrap_result = a -% b;
    mul_result = a * b;
    sadd_result = sa + sb;
    ssub_result = sa - sb;
  end comb
end module BugHuntI072
"#,
    )
    .expect("write ARCH fixture");
    fs::write(
        &tb_path,
        r#"#include "VBugHuntI072.h"
#include <cassert>

int main() {
  VBugHuntI072 d;
  d.a = 0u;
  d.b = 1u;
  d.eval();
  assert(d.add_result == 1u);
  assert(d.sub_result == 0x1ffffffffULL);
  assert(d.add_wrap_result == 1u);
  assert(d.sub_wrap_result == 0xffffffffu);
  assert(d.mul_result == 0u);
  d.c = 0xffffffffffffffffULL;
  d.d = 1u;
  d.eval();
  assert(d.add64_result.data()[0] == 0u);
  assert(d.add64_result.data()[1] == 0u);
  assert(d.add64_result.data()[2] == 1u);
  assert(d.sub64_result.data()[0] == 0xfffffffeu);
  assert(d.sub64_result.data()[1] == 0xffffffffu);
  assert(d.sub64_result.data()[2] == 0u);

  d.a = 0xffffffffu;
  d.b = 1u;
  d.eval();
  assert(d.add_result == 0x100000000ULL);
  assert(d.sub_result == 0xfffffffeULL);
  assert(d.add_wrap_result == 0u);
  assert(d.sub_wrap_result == 0xfffffffeu);
  assert(d.mul_result == 0xffffffffULL);

  d.sa = -1;
  d.sb = -2;
  d.eval();
  assert(d.sadd_result == -3);
  assert(d.ssub_result == 1);

  d.a = 7u;
  d.b = 5u;
  d.eval();
  assert(d.add_result == 12u);
  assert(d.sub_result == 2u);
  assert(d.mul_result == 35u);
}
"#,
    )
    .expect("write C++ testbench");

    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path().join("out"))
        .output()
        .expect("run arch sim");
    assert!(
        out.status.success(),
        "native sim widening regression failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
