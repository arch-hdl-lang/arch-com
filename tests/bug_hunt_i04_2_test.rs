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
    let ast = elaborate::lower_tlm_target_threads(ast).expect("TLM target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("TLM initiator lowering error");
    let ast = elaborate::lower_threads(ast).expect("thread lowering error");
    let ast = elaborate::lower_pipe_reg_ports(ast)
        .expect("pipe-reg lowering error")
        .0;
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit-channel lowering error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    Codegen::new(&symbols, &ast, overload_map).generate()
}

#[test]
fn once_thread_final_action_is_one_shot_and_keeps_terminal_comb() {
    let source = r#"
module OnceTerminal
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port done: out Bool;
  reg count: UInt<8> reset rst => 0;
  thread once run on clk rising, rst high
    wait 1 cycle;
    done = true;
    count <= count +% 8'd1;
  end thread run
end module OnceTerminal
"#;
    let sv = compile_to_sv(source);

    // The action advances into a separate terminal hold state, so its seq
    // assignment cannot repeat on later edges.
    assert!(sv.contains("count <= 8'(count + 8'd1);"));
    assert!(sv.contains("_t0_state <= _t0_S2_action;"));
    assert!(sv.contains("_t0_S2_action = 2"));

    // The final comb assignment is present in the terminal state as well as
    // the action state, preserving the output after the once thread stops.
    assert_eq!(sv.matches("done = 1'b1;").count(), 2);
}

#[test]
fn once_wait_until_action_keeps_terminal_comb_output() {
    let source = r#"
module OnceWaitUntil
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port ready: in Bool;
  port done: out Bool;
  reg count: UInt<8> reset rst => 0;
  thread once run on clk rising, rst high
    wait until ready;
    done = true;
    count <= count +% 8'd1;
  end thread run
end module OnceWaitUntil
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("count <= 8'(count + 8'd1);"));
    // The folded action state itself is retained as the terminal hold state,
    // so the comb assignment remains reachable after the wait resolves.
    assert_eq!(sv.matches("done = 1'b1;").count(), 1);
}
