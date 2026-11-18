use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;

#[test]
fn test_self_recursive_inst_rejected_1024() {
    let source = "
module Rec
  port a: in UInt<8>;
  port y: out UInt<8>;
  inst inner: Rec
    a <- a;
    y -> y;
  end inst inner
end module Rec
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let res = elaborate::elaborate(ast);
    assert!(
        res.is_err(),
        "self-recursive module instantiation must be rejected"
    );
    let errors = res.err().unwrap();
    let err_str = format!("{errors:?}");
    assert!(
        err_str.contains("recursive module instantiation: Rec -> Rec"),
        "expected error mentioning recursive module instantiation, got: {err_str}"
    );
}

#[test]
fn test_mutual_recursive_inst_rejected_1024() {
    let source = "
module Ping
  port a: in UInt<8>;
  port y: out UInt<8>;
  inst p: Pong
    a <- a;
    y -> y;
  end inst p
end module Ping

module Pong
  port a: in UInt<8>;
  port y: out UInt<8>;
  inst q: Ping
    a <- a;
    y -> y;
  end inst q
end module Pong
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let res = elaborate::elaborate(ast);
    assert!(
        res.is_err(),
        "mutually recursive module instantiation must be rejected"
    );
    let errors = res.err().unwrap();
    let err_str = format!("{errors:?}");
    assert!(
        err_str.contains("recursive module instantiation: Ping -> Pong -> Ping")
            || err_str.contains("recursive module instantiation: Pong -> Ping -> Pong"),
        "expected error mentioning recursive module instantiation, got: {err_str}"
    );
}

#[test]
fn test_three_module_cycle_rejected_1024() {
    let source = "
module ModA
  port a: in UInt<8>;
  port y: out UInt<8>;
  inst b: ModB
    a <- a;
    y -> y;
  end inst b
end module ModA

module ModB
  port a: in UInt<8>;
  port y: out UInt<8>;
  inst c: ModC
    a <- a;
    y -> y;
  end inst c
end module ModB

module ModC
  port a: in UInt<8>;
  port y: out UInt<8>;
  inst a_inst: ModA
    a <- a;
    y -> y;
  end inst a_inst
end module ModC
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let res = elaborate::elaborate(ast);
    assert!(
        res.is_err(),
        "three-module recursive instantiation must be rejected"
    );
    let errors = res.err().unwrap();
    let err_str = format!("{errors:?}");
    assert!(
        err_str.contains("recursive module instantiation")
            && err_str.contains("ModA")
            && err_str.contains("ModB")
            && err_str.contains("ModC"),
        "expected error mentioning recursive 3-module cycle, got: {err_str}"
    );
}

#[test]
fn test_pipeline_module_cycle_rejected_1024() {
    let source = "
module Leaf
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;
  inst p: Pipe
    clk <- clk;
    rst <- rst;
    a <- a;
    y -> y;
  end inst p
end module Leaf

pipeline Pipe
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;
  stage S0
    inst leaf: Leaf
      clk <- clk;
      rst <- rst;
      a <- a;
      y -> y;
    end inst leaf
  end stage S0
end pipeline Pipe
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let res = elaborate::elaborate(ast);
    assert!(
        res.is_err(),
        "pipeline-to-module cyclic instantiation must be rejected"
    );
    let errors = res.err().unwrap();
    let err_str = format!("{errors:?}");
    assert!(
        err_str.contains("recursive module instantiation")
            && err_str.contains("Leaf")
            && err_str.contains("Pipe"),
        "expected error mentioning pipeline/module instantiation cycle, got: {err_str}"
    );
}

#[test]
fn test_param_decrementing_generate_if_tree_rejected_1024() {
    // Failure mode A from issue #1024: recursive instantiation inside generate_if
    // must terminate promptly and be rejected with a diagnostic, not spin forever.
    let source = "
module Tree
  param N: const = 3;
  port a: in UInt<8>;
  port y: out UInt<8>;
  wire t: UInt<8>;
  generate_if N > 0
    inst child: Tree
      param N = N - 1;
      a <- a;
      y -> t;
    end inst child
  end generate_if
  comb
    y = t +% 1;
  end comb
end module Tree
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let res = elaborate::elaborate(ast);
    assert!(
        res.is_err(),
        "recursive instantiation in generate_if must be rejected without hanging"
    );
    let errors = res.err().unwrap();
    let err_str = format!("{errors:?}");
    assert!(
        err_str.contains("recursive module instantiation: Tree -> Tree"),
        "expected cycle error for Tree -> Tree, got: {err_str}"
    );
}
