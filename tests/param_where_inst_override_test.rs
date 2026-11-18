use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn check_source(source: &str) -> (usize, Vec<String>) {
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let res = TypeChecker::new(&symbols, &ast).check();
    match res {
        Ok(_) => (0, Vec::new()),
        Err(errs) => (errs.len(), errs.iter().map(|e| format!("{e:?}")).collect()),
    }
}

#[test]
fn test_param_where_inst_override_no_phantom_default_error_998() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Cache
  param SIZE: const = 4 where (SIZE & (SIZE - 1)) == 0;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port idx: in UInt<8>;
  port y: out UInt<8>;
  comb
    y = idx +% SIZE;
  end comb
end module Cache

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port i: in UInt<8>;
  port o: out UInt<8>;
  inst c: Cache
    param SIZE = 3;
    clk <- clk;
    rst <- rst;
    idx <- i;
    y -> o;
  end inst c
end module Top
";
    let (count, msgs) = check_source(source);
    // Should produce exactly 1 error: the inst-site error
    assert_eq!(
        count, 1,
        "expected exactly 1 error (inst-site constraint violation), got {count}: {msgs:#?}"
    );
    assert!(
        msgs[0].contains("param `SIZE`=3 in inst `c` violates constraint"),
        "expected inst-site constraint violation message, got: {}",
        msgs[0]
    );
    assert!(
        !msgs.iter().any(|m| m.contains("default value SIZE=3 violates constraint")),
        "must not emit phantom definition-site error claiming default value SIZE=3 violates constraint, got: {msgs:#?}"
    );
}

#[test]
fn test_param_where_bad_declared_default_still_rejected_998() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BadCache
  param SIZE: const = 3 where (SIZE & (SIZE - 1)) == 0;
  port idx: in UInt<8>;
  port y: out UInt<8>;
  comb
    y = idx +% SIZE;
  end comb
end module BadCache
";
    let (count, msgs) = check_source(source);
    assert!(
        count >= 1,
        "declared illegal default must be rejected, got 0 errors"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("default value SIZE=3 violates constraint")),
        "expected error stating default value SIZE=3 violates constraint, got: {msgs:#?}"
    );
}
