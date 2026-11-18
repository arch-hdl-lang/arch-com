use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

#[test]
fn test_pipe_reg_tap_latency_alignment_fir_997() {
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module fir_tap_smoke
  port clk_i: in Clock<SysDomain>;
  port rst_ni: in Reset<Async, Low>;
  port s_i: in UInt<8>;
  port c1: in UInt<8>;
  port y_o: out UInt<16>;
  pipe_reg sp: s_i stages 3;
  comb
    y_o = sp@1 * c1;
  end comb
end module fir_tap_smoke
";
    let tokens = lexer::tokenize(source).expect("tokenize");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    checker
        .check()
        .expect("expected pipe_reg tap to combine with cycle-0 signal without latency error");
}
