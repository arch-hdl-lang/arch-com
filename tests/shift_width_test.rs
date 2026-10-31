//! Regression coverage for arch#937 and arch#1106 — `<<` is non-widening: `a << n` has
//! the width of `a` (spec §"Shift operators: non-widening").
//!
//! Before the fix the three passes disagreed:
//! - `arch sim` never masked a shift to `W(a)`, so a 2-bit `let`, wire,
//!   output port or register assigned `a << 1` held 6 (the emitted SV holds
//!   2);
//! - the emitted SV let a surrounding wider expression size the shift's
//!   operand (IEEE 1800 context-determined sizing), so `(a << 1) | wide`
//!   kept the shifted-out bit, unlike the same shift bound to a `let`;
//! - the type checker typed the shift `W(a)` but only rejected a shift
//!   assigned directly to a wider target.
//!
//! Now both backends compute `W(a)`, and `arch check` rejects every shift
//! whose shifted-out bits a design could have been relying on, so no design
//! changes value silently.

use std::fs;
use std::path::Path;
use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

fn write(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    fs::write(&p, text).expect("write file");
    p
}

/// Every place a shift result is stored or consumed at its own width.
const STORED: &str = r#"
module ShiftStore
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port a: in UInt<2>;
  port y_let: out UInt<2>;
  port y_wire: out UInt<2>;
  port y_port: out UInt<2>;
  port y_reg: out UInt<2>;
  port y_shr: out UInt<2>;
  port y_zext: out UInt<3>;
  let t: UInt<2> = a << 1;
  wire w: UInt<2>;
  reg r: UInt<2> reset rst => 0;
  seq on clk rising
    r <= a << 1;
  end seq
  comb
    w = a << 1;
    y_let = t;
    y_wire = w;
    y_port = a << 1;
    y_reg = r;
    y_shr = (a << 1) >> 1;
    y_zext = (a << 1).zext<3>();
  end comb
end module ShiftStore
"#;

const STORED_TB: &str = r#"
#include "VShiftStore.h"
#include <cstdio>
int main() {
  VShiftStore d;
  d.rst = 1; d.clk = 0; d.eval(); d.clk = 1; d.eval(); d.rst = 0;
  d.a = 3; d.clk = 0; d.eval(); d.clk = 1; d.eval(); d.clk = 0; d.eval();
  std::printf("let=%u wire=%u port=%u reg=%u shr=%u zext=%u\n",
    (unsigned)d.y_let, (unsigned)d.y_wire, (unsigned)d.y_port,
    (unsigned)d.y_reg, (unsigned)d.y_shr, (unsigned)d.y_zext);
  return 0;
}
"#;

/// `3 << 1` at 2 bits is 2 everywhere, and `(3 << 1) >> 1` is 1.
const STORED_EXPECT: &str = "let=2 wire=2 port=2 reg=2 shr=1 zext=2";

#[test]
fn sim_masks_shift_to_operand_width() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = write(td.path(), "ShiftStore.arch", STORED);
    let tb = write(td.path(), "tb.cpp", STORED_TB);
    let out = arch()
        .arg("sim")
        .arg(&src)
        .arg("--tb")
        .arg(&tb)
        .arg("--outdir")
        .arg(td.path().join("sim"))
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains(STORED_EXPECT), "arch sim: {stdout}");
}

#[test]
fn emitted_sv_agrees_with_sim_under_verilator() {
    if Command::new("verilator").arg("--version").output().is_err() {
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let src = write(td.path(), "ShiftStore.arch", STORED);
    let tb = write(td.path(), "tb.cpp", STORED_TB);
    let sv = td.path().join("ShiftStore.sv");
    let build = arch()
        .args(["build", src.to_str().unwrap(), "-o", sv.to_str().unwrap()])
        .output()
        .expect("arch build");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let obj = td.path().join("obj");
    let verilate = Command::new("verilator")
        .args([
            "--cc",
            "--exe",
            "--build",
            "-Wno-fatal",
            "--top-module",
            "ShiftStore",
            "-Mdir",
        ])
        .arg(&obj)
        .arg(&sv)
        .arg(&tb)
        .output()
        .expect("verilate");
    assert!(
        verilate.status.success(),
        "{}",
        String::from_utf8_lossy(&verilate.stderr)
    );
    let run = Command::new(obj.join("VShiftStore")).output().expect("run");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains(STORED_EXPECT), "verilator: {stdout}");
}

fn check(body: &str) -> (bool, String) {
    let td = tempfile::tempdir().expect("tempdir");
    let src = format!(
        "module C\n  param N: const = 3;\n  port a: in UInt<2>;\n  port b: in UInt<3>;\n  \
         port c: in UInt<8>;\n  port i: in UInt<3>;\n{body}\nend module C\n"
    );
    let p = write(td.path(), "C.arch", &src);
    let out = arch().arg("check").arg(&p).output().expect("arch check");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn comb(width: u32, expr: &str) -> String {
    format!("  port y: out UInt<{width}>;\n  comb\n    y = {expr};\n  end comb")
}

#[test]
fn shift_whose_dropped_bits_are_used_is_rejected() {
    for (width, expr) in [
        // A wider operand of a bitwise operator.
        (3, "(a << 1) | b"),
        // An unsized literal is 1 bit wide: write `1.zext<8>() << i`.
        (8, "c | (1 << i)"),
        // Widening arithmetic observes the shifted-out bit.
        (4, "(b << 1) + b"),
        // SV sizes a comparison with an unsized literal at 32 bits.
        (1, "(a << 1) == 0"),
        (1, "(a << 1) == b"),
        // A concatenation operand is sized by itself, i.e. by the `|`.
        (4, "{(a << 1) | b, a[0]}"),
        // A right shift brings the dropped bit back down.
        (3, "((a << 1) | b) >> 1"),
    ] {
        let (ok, stderr) = check(&comb(width, expr));
        assert!(!ok, "`{expr}` should be rejected");
        assert!(
            stderr.contains("shifts do not") && stderr.contains("widen"),
            "`{expr}`: unexpected diagnostic:\n{stderr}"
        );
    }
    // The same rule applies to a typed `let`.
    let (ok, stderr) = check(
        "  port y: out UInt<3>;\n  let t: UInt<3> = (a << 1) | b;\n  comb\n    y = t;\n  end comb",
    );
    assert!(
        !ok && stderr.contains("shift result is UInt<2>"),
        "{stderr}"
    );
}

#[test]
fn shift_whose_dropped_bits_are_unused_is_accepted() {
    for (width, expr) in [
        // Widening the operand first keeps the bits.
        (3, "(a.zext<3>() << 1) | b"),
        (8, "c | (1.zext<8>() << i)"),
        // Same width throughout.
        (2, "(a << 1) | a"),
        // `.zext` evaluates its operand at the operand's own width.
        (3, "(a << 1).zext<3>()"),
        (3, "((a << 1) & a).zext<3>() | b"),
        // The truncation drops the extra bits anyway.
        (3, "((b << 1) + b).trunc<3>()"),
        (2, "((a << 1) | b).trunc<2>()"),
        // A constant shift is integer arithmetic.
        (8, "c & ((1 << N) - 1)"),
        (1, "(a << 1) == a"),
        (3, "{a << 1, a[0]}"),
    ] {
        let (ok, stderr) = check(&comb(width, expr));
        assert!(ok, "`{expr}` should be accepted:\n{stderr}");
    }
    // A `for` iterator is a compile-time value, so `1 << j` is integer
    // arithmetic (the parity-bit idiom).
    let (ok, stderr) = check(
        "  port y: out Bool;\n  comb\n    y = false;\n    for j in 0..2\n      \
         if (c & (1 << j)) != 0\n        y = true;\n      end if\n    end for\n  end comb",
    );
    assert!(
        ok,
        "`c & (1 << j)` with a for iterator should be accepted:\n{stderr}"
    );
}

#[test]
fn sv_pins_shift_width_and_leaves_constant_shifts_alone() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = write(
        td.path(),
        "P.arch",
        "module P\n  param N: const = 3;\n  port a: in UInt<2>;\n  port c: in UInt<8>;\n  \
         port y: out UInt<2>;\n  port m: out UInt<8>;\n  comb\n    y = (a << 1) | a;\n    \
         m = c & ((1 << N) - 1);\n  end comb\nend module P\n",
    );
    let sv = td.path().join("P.sv");
    let out = arch()
        .args(["build", src.to_str().unwrap(), "-o", sv.to_str().unwrap()])
        .output()
        .expect("arch build");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&sv).expect("read SV");
    assert!(
        text.contains("2'(a << 1)"),
        "shift should be pinned:\n{text}"
    );
    assert!(
        text.contains("(1 << N) - 1") && !text.contains("'(1 << N)"),
        "constant shift should stay unsized:\n{text}"
    );
}
