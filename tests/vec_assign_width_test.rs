use arch::diagnostics::CompileError;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn check_source(source: &str) -> Result<(), Vec<CompileError>> {
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    TypeChecker::new(&symbols, &ast).check().map(|_| ())
}

#[test]
fn test_vec_assign_length_mismatch_rejected_886() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module VLen
  port clk: in Clock<SysDomain>;
  port a: in Vec<UInt<8>, 4>;
  port y: out Vec<UInt<8>, 2>;
  comb
    y = a;
  end comb
end module VLen
";
    let res = check_source(source);
    assert!(
        res.is_err(),
        "expected error for Vec length mismatch (assigning Vec<UInt<8>, 4> to Vec<UInt<8>, 2>), got Ok"
    );
    let errors = res.err().unwrap();
    let err_str = format!("{errors:?}");
    assert!(
        err_str.contains("width mismatch")
            && err_str.contains("Vec<UInt<8>, 2>")
            && err_str.contains("Vec<UInt<8>, 4>"),
        "expected diagnostic mentioning width mismatch and Vec types, got: {err_str}"
    );
}

#[test]
fn test_vec_assign_elem_width_mismatch_rejected_886() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module VElem
  port a: in Vec<UInt<8>, 4>;
  port y: out Vec<UInt<4>, 4>;
  comb
    y = a;
  end comb
end module VElem
";
    let res = check_source(source);
    assert!(
        res.is_err(),
        "expected error for Vec element width mismatch (assigning Vec<UInt<8>, 4> to Vec<UInt<4>, 4>), got Ok"
    );
    let errors = res.err().unwrap();
    let err_str = format!("{errors:?}");
    assert!(
        err_str.contains("width mismatch")
            && err_str.contains("Vec<UInt<4>, 4>")
            && err_str.contains("Vec<UInt<8>, 4>"),
        "expected diagnostic mentioning width mismatch and Vec types, got: {err_str}"
    );
}

#[test]
fn test_vec_assign_seq_mismatch_rejected_886() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module VSeq
  port clk: in Clock<SysDomain>;
  port a: in Vec<UInt<8>, 4>;
  port reg r: out Vec<UInt<8>, 2>;
  seq on clk rising
    r <= a;
  end seq
end module VSeq
";
    let res = check_source(source);
    assert!(
        res.is_err(),
        "expected error for seq Vec length mismatch, got Ok"
    );
}

#[test]
fn test_vec_assign_matching_accepted_886() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module VMatch
  port clk: in Clock<SysDomain>;
  port a: in Vec<UInt<8>, 4>;
  port y: out Vec<UInt<8>, 4>;
  port reg r: out Vec<UInt<8>, 4>;
  comb
    y = a;
  end comb
  seq on clk rising
    r <= a;
  end seq
end module VMatch
";
    let res = check_source(source);
    assert!(
        res.is_ok(),
        "expected matching Vec assignment to succeed, got errors: {res:?}"
    );
}
