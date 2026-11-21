use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn compile_to_sv(source: &str) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(ast).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
    codegen.generate()
}

#[test]
fn test_generate_assert_without_suffix_unrolls_unique_labels_1078() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module GenAssertDup
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port din: in Vec<UInt<8>, 3>;
  generate_for i in 0..2
    assert a_bound: din[i] < 200;
  end generate_for
end module GenAssertDup
";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("a_bound_0"),
        "expected a_bound_0 in generated SV, got:\n{sv}"
    );
    assert!(
        sv.contains("a_bound_1"),
        "expected a_bound_1 in generated SV, got:\n{sv}"
    );
    assert!(
        sv.contains("a_bound_2"),
        "expected a_bound_2 in generated SV, got:\n{sv}"
    );
    // Ensure bare duplicate label a_bound: without index suffix does not exist
    assert!(
        !sv.contains("a_bound:"),
        "expected no duplicate bare a_bound: in generated SV, got:\n{sv}"
    );
}

#[test]
fn test_generate_wire_without_suffix_unrolls_unique_names_1078() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module GenWireDup
  port clk: in Clock<SysDomain>;
  generate_for i in 0..2
    wire w: UInt<8>;
  end generate_for
end module GenWireDup
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let elaborated = elaborate::elaborate(ast).expect("elaborate");
    let module = match &elaborated.items[1] {
        arch::ast::Item::Module(m) => m,
        _ => panic!("expected module"),
    };
    let wire_names: Vec<_> = module
        .body
        .iter()
        .filter_map(|item| match item {
            arch::ast::ModuleBodyItem::WireDecl(w) => Some(w.name.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(wire_names, vec!["w_0", "w_1", "w_2"]);
}
