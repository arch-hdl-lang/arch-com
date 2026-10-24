//! Regression coverage for formal shift amounts wider than their data operand.
//!
//! The HDL shift result is non-widening, but the shift amount is not modulo
//! the data width.  In particular, shifting a 2-bit value by 4 must produce
//! zero (or sign-fill for an arithmetic right shift).

use std::fs;
use std::process::Command;

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

const SHIFT_SOURCE: &str = r#"module WideShiftRegression
 port clk: in Clock<SysDomain>;
 port u: in UInt<2>;
 port s: in SInt<2>;
 port n: in UInt<8>;
 port m: in UInt<2>;
 port ul: out UInt<2>;
 port ur: out UInt<2>;
 port sl: out SInt<2>;
 port sr: out SInt<2>;
 port uml: out UInt<2>;
 port umr: out UInt<2>;
 port sml: out SInt<2>;
 port smr: out SInt<2>;
 comb
  ul = u << n;
  ur = u >> n;
  sl = s << n;
  sr = s >> n;
  uml = u << m;
  umr = u >> m;
  sml = s << m;
  smr = s >> m;
 end comb

 // Wide-count false identities: each has a concrete witness at n=4.
 assert false_ul: ((n == 8'd4) and (u == 2'd1)) |-> (ul == u);
 assert false_ur: ((n == 8'd4) and (u == 2'd1)) |-> (ur == u);
 assert false_sl: ((n == 8'd4) and (s == 2'b10)) |-> (sl == s);
 assert false_sr: ((n == 8'd4) and (s == 2'b10)) |-> (sr == s);

 // Wide-count overshift controls.
 assert wide_ul_zero: ((n == 8'd4) and (u == 2'd1)) |-> (ul == 2'd0);
 assert wide_ur_zero: ((n == 8'd4) and (u == 2'd1)) |-> (ur == 2'd0);
 assert wide_sl_zero: ((n == 8'd4) and (s == 2'b10)) |-> (sl == 2'b00);
 assert wide_sr_sign: ((n == 8'd4) and (s == 2'b10)) |-> (sr == 2'b11);

 // Zero, one, and exactly-data-width counts.
 assert zero_ul: (n == 8'd0) |-> (ul == u);
 assert zero_ur: (n == 8'd0) |-> (ur == u);
 assert zero_sl: (n == 8'd0) |-> (sl == s);
 assert zero_sr: (n == 8'd0) |-> (sr == s);
 assert one_ul: ((n == 8'd1) and (u == 2'd1)) |-> (ul == 2'd2);
 assert one_ur: ((n == 8'd1) and (u == 2'd2)) |-> (ur == 2'd1);
 assert one_sl: ((n == 8'd1) and (s == 2'b01)) |-> (sl == 2'b10);
 assert one_sr: ((n == 8'd1) and (s == 2'b10)) |-> (sr == 2'b11);
 assert width_ul_zero: ((n == 8'd2) and (u == 2'd1)) |-> (ul == 2'd0);
 assert width_ur_zero: ((n == 8'd2) and (u == 2'd1)) |-> (ur == 2'd0);
 assert width_sl_zero: ((n == 8'd2) and (s == 2'b01)) |-> (sl == 2'b00);
 assert width_sr_sign: ((n == 8'd2) and (s == 2'b10)) |-> (sr == 2'b11);

 // Counts no wider than the data operand still use ordinary shift widths.
 assert narrow_count_ul: ((m == 2'd1) and (u == 2'd1)) |-> (uml == 2'd2);
 assert narrow_count_ur: ((m == 2'd1) and (u == 2'd2)) |-> (umr == 2'd1);
 assert narrow_count_sl: ((m == 2'd1) and (s == 2'b01)) |-> (sml == 2'b10);
 assert narrow_count_sr: ((m == 2'd1) and (s == 2'b10)) |-> (smr == 2'b11);
end module WideShiftRegression
"#;

#[test]
fn formal_wide_shift_amounts_preserve_overshift_semantics() {
    let dir = tempfile::tempdir().expect("temporary fixture directory");
    let source = dir.path().join("wide_shift.arch");
    fs::write(&source, SHIFT_SOURCE).expect("write fixture");

    // Front-end and pure source/encoding setup must remain available on CI
    // machines without an SMT solver.
    let check = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("check")
        .arg(&source)
        .output()
        .expect("run arch check");
    assert!(
        check.status.success(),
        "wide shift regression fixture must type-check:\n{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    if !z3_available() {
        eprintln!("skipping SMT assertions: z3 not in PATH");
        return;
    }

    let formal = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("formal")
        .arg(&source)
        .arg("--bound")
        .arg("2")
        .output()
        .expect("run arch formal");
    let report = format!(
        "{}\n{}",
        String::from_utf8_lossy(&formal.stdout),
        String::from_utf8_lossy(&formal.stderr)
    );

    for name in ["false_ul", "false_ur", "false_sl", "false_sr"] {
        assert!(
            report
                .lines()
                .any(|line| line.starts_with(&format!("[Assert] {name}"))
                    && line.contains("REFUTED")),
            "{name} must be refuted by a concrete wide-count witness:\n{report}"
        );
    }
    for name in [
        "wide_ul_zero",
        "wide_ur_zero",
        "wide_sl_zero",
        "wide_sr_sign",
        "zero_ul",
        "zero_ur",
        "zero_sl",
        "zero_sr",
        "one_ul",
        "one_ur",
        "one_sl",
        "one_sr",
        "width_ul_zero",
        "width_ur_zero",
        "width_sl_zero",
        "width_sr_sign",
        "narrow_count_ul",
        "narrow_count_ur",
        "narrow_count_sl",
        "narrow_count_sr",
    ] {
        assert!(
            report.lines().any(
                |line| line.starts_with(&format!("[Assert] {name}")) && line.contains("PROVED")
            ),
            "{name} must prove:\n{report}"
        );
    }
    assert!(
        formal.status.code() == Some(1),
        "false controls must fail formal:\n{report}"
    );
    assert!(
        report.contains("0x4"),
        "counterexample must exercise n=4:\n{report}"
    );
}
