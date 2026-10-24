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
fn parameterized_generate_for_references_emitted_child_variants() {
    let source = r#"
module Child
  param W: const = 1;
  port a: in UInt<8>;
  port y: out UInt<8>;
  let y = a;
end module Child

module Top
  param N: const = 2;
  port a: in Vec<UInt<8>, N>;
  port y: out Vec<UInt<8>, N>;
  generate_for i in 0..N-1
    inst leaves: Child
      param W = i + 1;
      a <- a[i];
      y -> y[i];
    end inst leaves
  end generate_for
end module Top
"#;

    let sv = compile_to_sv(source);
    assert!(sv.contains("module Child__W_1"), "missing W=1 child:\n{sv}");
    assert!(sv.contains("module Child__W_2"), "missing W=2 child:\n{sv}");
    assert!(
        !sv.contains("Child #(.W(i + 1))"),
        "unspecialized child remains:\n{sv}"
    );
    assert!(
        sv.contains("Child__W_1 #(.W(0 + 1)) leaves_0"),
        "missing first specialized instance:\n{sv}"
    );
    assert!(
        sv.contains("Child__W_2 #(.W(1 + 1)) leaves_1"),
        "missing second specialized instance:\n{sv}"
    );
}
