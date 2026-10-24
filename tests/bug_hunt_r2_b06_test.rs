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
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overloads) = checker.check().expect("type check error");
    Codegen::new(&symbols, &ast, overloads).generate()
}

#[test]
fn typed_integer_params_drive_generate_if_and_preserve_types() {
    let source = r#"
module Child
  param N: UInt<8> = 8'd2;
  param S: SInt<8> = 8'hff;
  param B: Bool = true;
  port clk: in Clock<SysDomain>;
  generate_if N > 0
    assert active: true;
  generate_else
    assert inactive: true;
  end generate_if
  generate_if S < 0
    assert signed_negative: true;
  generate_else
    assert signed_nonnegative: true;
  end generate_if
end module Child

module Top
  port clk: in Clock<SysDomain>;
  inst enabled: Child
    clk <- clk;
  end inst enabled
  inst disabled: Child
    param N = 0;
    param S = 1;
    param B = false;
    clk <- clk;
  end inst disabled
end module Top
"#;

    let sv = compile_to_sv(source);
    assert!(
        sv.contains("active"),
        "default typed UInt param should select true branch:\n{sv}"
    );
    assert!(
        sv.contains("inactive"),
        "typed UInt override should select false branch:\n{sv}"
    );
    assert!(
        sv.contains("signed_negative"),
        "8-bit all-ones SInt default should fold as -1:\n{sv}"
    );
    assert!(
        sv.contains("signed_nonnegative"),
        "SInt override should select the nonnegative branch:\n{sv}"
    );
    assert!(
        sv.contains("parameter [7:0] N = 8'd2"),
        "UInt width should be preserved:\n{sv}"
    );
    assert!(
        sv.contains("parameter signed [7:0] S = 8'd255"),
        "SInt width/sign and bit pattern should be preserved:\n{sv}"
    );
    assert!(
        sv.contains("parameter B = 1'd1"),
        "Bool width should be preserved:\n{sv}"
    );
}

#[test]
fn float_logic_params_remain_outside_integer_constant_environment() {
    let source = r#"
module FloatParam
  param F: FP32 = 1.0;
  port clk: in Clock<SysDomain>;
  generate_if F > 0
    assert active: true;
  end generate_if
end module FloatParam
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let err =
        elaborate::elaborate(parsed).expect_err("float generate condition must remain unsupported");
    assert!(
        err.iter()
            .any(|e| e.to_string().contains("compile-time constant boolean")),
        "unexpected error: {err:?}"
    );
}
