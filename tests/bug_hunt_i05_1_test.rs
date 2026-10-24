use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn compile_to_sv(source: &str) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed).expect("elaborate error");
    let ast = elaborate::expand_auto_connect(ast, false)
        .expect("auto-connect error")
        .0;
    let ast = elaborate::lower_tlm_target_threads(ast).expect("target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("initiator lowering error");
    let ast = elaborate::lower_threads(ast).expect("thread lowering error");
    let ast = elaborate::lower_pipe_reg_ports(ast)
        .expect("pipe-reg lowering error")
        .0;
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("dispatch lowering error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overloads) = checker.check().expect("type check error");
    Codegen::new(&symbols, &ast, overloads).generate()
}

#[test]
fn ordinary_arithmetic_keeps_arch_widening_inside_comparisons() {
    if std::process::Command::new("iverilog")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }

    let sv = compile_to_sv(
        r#"
module AddCompare
  port a: in UInt<8>;
  port b: in UInt<8>;
  port c: in UInt<8>;
  port d: in UInt<8>;
  port y: out Bool;
  port y_ref: out Bool;
  port sub: out UInt<9>;
  port sub_wide: out UInt<16>;
  port mul: out UInt<16>;
  port mul_cmp: out Bool;
  port ternary_cmp: out Bool;
  port shifted_cmp: out Bool;
  port nested: out UInt<10>;
  port wrapped: out UInt<8>;
  port wrapped_cmp: out Bool;
  wire lhs: UInt<9>;
  wire rhs: UInt<9>;
  comb
    y = (a + b) == (c + d);
    lhs = a + b;
    rhs = c + d;
    y_ref = lhs == rhs;
    sub = a - b;
    sub_wide = a - b;
    mul = a * b;
    mul_cmp = (a * b) == (c * d);
    ternary_cmp = (a > 0 ? (a + b) : (c + d)) == (c + d);
    shifted_cmp = ((a + b) << 0) == (c + d);
    nested = (a + b) + (c + d);
    wrapped = a +% b;
    wrapped_cmp = (a +% b) == (c +% d);
  end comb
end module AddCompare
"#,
    );
    assert!(
        sv.contains("9'(a + b)"),
        "ordinary addition operands must be widened before SV evaluates them:\n{sv}"
    );
    assert!(
        sv.contains("16'(a * b)"),
        "ordinary multiplication operands must be widened before SV evaluates them:\n{sv}"
    );
    assert!(
        !sv.contains("8'($unsigned(a)) +%"),
        "wrapping arithmetic should retain its existing cast-after-operation path:\n{sv}"
    );

    let td = tempfile::tempdir().unwrap();
    let dut = td.path().join("dut.sv");
    let tb = td.path().join("tb.sv");
    let exe = td.path().join("sim.vvp");
    std::fs::write(&dut, sv).unwrap();
    std::fs::write(
        &tb,
        r#"
module tb;
  logic [7:0] a, b, c, d;
  wire y, y_ref;
  wire [8:0] sub;
  wire [15:0] sub_wide;
  wire [15:0] mul;
  wire mul_cmp;
  wire ternary_cmp, shifted_cmp;
  wire [9:0] nested;
  wire [7:0] wrapped;
  wire wrapped_cmp;
  AddCompare dut(.*);
  initial begin
    a=1; b=2; c=2; d=1; #1;
    if (y !== 1 || y_ref !== 1 || ternary_cmp !== 1 || shifted_cmp !== 1) $fatal(1, "non-overflow comparison failed");
    a=255; b=1; c=0; d=0; #1;
    if (y_ref !== 0 || y !== 0 || ternary_cmp !== 0 || shifted_cmp !== 0 || wrapped_cmp !== 1) $fatal(1, "overflow comparison lost the carry");
    if (sub !== 9'd254 || mul !== 16'd255 || mul_cmp !== 0 || nested !== 10'd256 || wrapped !== 8'd0)
      $fatal(1, "arithmetic result mismatch: sub=%0d mul=%0d nested=%0d wrapped=%0d", sub, mul, nested, wrapped);
    a=0; b=1; c=0; d=0; #1;
    if (sub !== 9'd511 || sub_wide !== 16'd511) $fatal(1, "subtraction must retain its ARCH width before outer widening");
    $finish;
  end
endmodule
"#,
    )
    .unwrap();
    let build = std::process::Command::new("iverilog")
        .args(["-g2012", "-s", "tb", "-o"])
        .arg(&exe)
        .arg(&dut)
        .arg(&tb)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "iverilog failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = std::process::Command::new("vvp").arg(exe).output().unwrap();
    assert!(
        run.status.success(),
        "vvp failed:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn floating_point_comparisons_keep_float_helper_dispatch() {
    let sv = compile_to_sv(
        r#"
module FpCompare
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port d: in FP32;
  port y: out Bool;
  comb
    y = (a + b) == (c + d);
  end comb
end module FpCompare
"#,
    );
    assert!(sv.contains("arch_f32_eq(arch_f32_add("), "{sv}");
    assert!(
        !sv.contains("$unsigned(a)"),
        "float arithmetic must keep helper dispatch:\n{sv}"
    );
}

#[test]
fn signed_widening_preserves_negative_arithmetic_in_comparisons() {
    if std::process::Command::new("iverilog")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let sv = compile_to_sv(
        r#"
module SignedCompare
  port a: in SInt<8>;
  port b: in SInt<8>;
  port c: in SInt<8>;
  port d: in SInt<8>;
  port y: out Bool;
  port y_ref: out Bool;
  wire lhs: SInt<9>;
  wire rhs: SInt<9>;
  comb
    y = (a + b) == (c + d);
    lhs = a + b;
    rhs = c + d;
    y_ref = lhs == rhs;
  end comb
end module SignedCompare
"#,
    );
    let td = tempfile::tempdir().unwrap();
    let dut = td.path().join("dut.sv");
    let tb = td.path().join("tb.sv");
    let exe = td.path().join("sim.vvp");
    std::fs::write(&dut, sv).unwrap();
    std::fs::write(
        &tb,
        r#"
module tb;
  logic signed [7:0] a, b, c, d;
  wire y, y_ref;
  SignedCompare dut(.*);
  initial begin
    a=-128; b=-1; c=127; d=0; #1;
    if (y_ref !== 0 || y !== 0) $fatal(1, "signed widening comparison failed");
    a=-1; b=1; c=0; d=0; #1;
    if (y_ref !== 1 || y !== 1) $fatal(1, "signed non-overflow comparison failed");
    $finish;
  end
endmodule
"#,
    )
    .unwrap();
    let build = std::process::Command::new("iverilog")
        .args(["-g2012", "-s", "tb", "-o"])
        .arg(&exe)
        .arg(&dut)
        .arg(&tb)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "iverilog failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = std::process::Command::new("vvp").arg(exe).output().unwrap();
    assert!(
        run.status.success(),
        "vvp failed:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
