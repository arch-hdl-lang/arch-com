use arch::codegen::Codegen;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn compile_to_sv(source: &str) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens);
    let ast = parser.parse_source_file().expect("parse error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let _warnings = checker.check().expect("type check error");
    let codegen = Codegen::new(&symbols, &ast);
    codegen.generate()
}

#[test]
fn test_top_counter_compiles() {
    let source = include_str!("top_counter.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module Counter"));
    assert!(sv.contains("module Top"));
    assert!(sv.contains("always_ff"));
    assert!(sv.contains("assign count = count_r"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_struct_and_enum() {
    let source = r#"
struct Packet
  data: UInt<32>;
  valid: Bool;
end struct Packet

enum AluOp
  Add,
  Sub,
  And,
  Or
end enum AluOp

module SimpleAlu
  port op: in AluOp;
  port a: in UInt<32>;
  port b: in UInt<32>;
  port result: out UInt<32>;
  comb
    result = a + b;
  end comb
end module SimpleAlu
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("typedef struct packed"));
    assert!(sv.contains("typedef enum logic"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_todo_placeholder() {
    let source = r#"
domain SysDomain
  freq_mhz: 100,
end domain SysDomain

module Placeholder
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port out_val: out UInt<8>;
  comb
    out_val = todo!;
  end comb
end module Placeholder
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("TODO"));
    insta::assert_snapshot!(sv);
}

// ── FSM ───────────────────────────────────────────────────────────────────────

#[test]
fn test_fsm_traffic_light() {
    let source = include_str!("traffic_light.arch");
    let sv = compile_to_sv(source);
    // State enum
    assert!(sv.contains("TrafficLight_state_t"));
    assert!(sv.contains("RED ="));
    assert!(sv.contains("GREEN ="));
    assert!(sv.contains("YELLOW ="));
    // State register FF
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("state_r <= RED")); // reset value
    // Next-state logic
    assert!(sv.contains("state_next = state_r")); // hold default
    assert!(sv.contains("state_next = GREEN"));
    assert!(sv.contains("state_next = YELLOW"));
    assert!(sv.contains("state_next = RED"));
    // Output logic
    assert!(sv.contains("red = 1'b1"));
    assert!(sv.contains("green = 1'b1"));
    assert!(sv.contains("yellow = 1'b1"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_fsm_missing_default_state_errors() {
    let source = r#"
fsm Broken
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port result: out Bool;
  state A, B;
  default state C;
  state A
    comb
      result = true;
    end comb
    transition to B when true;
  end state A
  state B
    comb
      result = false;
    end comb
    transition to A when true;
  end state B
end fsm Broken
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens);
    let ast = parser.parse_source_file().expect("parse");
    // `C` is not in {A, B} — resolve should error
    assert!(resolve::resolve(&ast).is_err());
}

// ── FIFO ──────────────────────────────────────────────────────────────────────

#[test]
fn test_sync_fifo() {
    let source = include_str!("sync_fifo.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("$clog2(DEPTH)"));
    assert!(sv.contains("assign full"));
    assert!(sv.contains("assign empty"));
    assert!(sv.contains("assign push_ready"));
    assert!(sv.contains("assign pop_valid"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    // Not async
    assert!(!sv.contains("bin2gray"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_async_fifo() {
    let source = include_str!("async_fifo.arch");
    let sv = compile_to_sv(source);
    // Gray-code CDC
    assert!(sv.contains("bin2gray"));
    assert!(sv.contains("gray2bin"));
    assert!(sv.contains("wr_ptr_gray_sync"));
    assert!(sv.contains("rd_ptr_gray_sync"));
    // Two separate clock domains
    assert!(sv.contains("posedge wr_clk"));
    assert!(sv.contains("posedge rd_clk"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_fifo_missing_port_errors() {
    let source = r#"
domain SysDomain
  freq_mhz: 100,
end domain SysDomain

fifo BadFifo
  param DEPTH: const = 8;
  param WIDTH: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in WIDTH;
  // Missing: pop_valid, pop_ready, pop_data
end fifo BadFifo
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err());
}

// ── RAM ───────────────────────────────────────────────────────────────────────

#[test]
fn test_single_port_ram() {
    let source = include_str!("single_port_ram.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module SimpleMem"));
    assert!(sv.contains("parameter int DEPTH = 256"));
    assert!(sv.contains("parameter int DATA_WIDTH = 8"));
    assert!(sv.contains("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1]"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("assign access_rdata = access_rdata_r"));
    // no_change: write and read are in mutually exclusive branches
    assert!(sv.contains("if (access_wen)"));
    // zero init
    assert!(sv.contains("for (int i = 0; i < DEPTH; i++) mem[i] = '0"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_simple_dual_ram() {
    let source = include_str!("simple_dual_ram.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module DualMem"));
    assert!(sv.contains("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1]"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    // Write port
    assert!(sv.contains("wr_port_en"));
    assert!(sv.contains("mem[wr_port_addr] <= wr_port_data"));
    // Read port
    assert!(sv.contains("rd_port_en"));
    assert!(sv.contains("assign rd_port_data = rd_port_data_r"));
    insta::assert_snapshot!(sv);
}

// ── Counter ───────────────────────────────────────────────────────────────────

#[test]
fn test_wrap_counter() {
    let source = include_str!("wrap_counter.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module WrapCounter"));
    assert!(sv.contains("parameter int MAX = 15"));
    assert!(sv.contains("logic [4-1:0] count_r"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("assign value = count_r"));
    assert!(sv.contains("assign at_max"));
    insta::assert_snapshot!(sv);
}

// ── Arbiter ───────────────────────────────────────────────────────────────────

#[test]
fn test_bus_arbiter() {
    let source = include_str!("bus_arbiter.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module BusArbiter"));
    assert!(sv.contains("parameter int NUM_REQ = 4"));
    assert!(sv.contains("logic [NUM_REQ-1:0] request_valid"));
    assert!(sv.contains("logic [NUM_REQ-1:0] request_ready"));
    assert!(sv.contains("rr_ptr_r"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("always_comb"));
    insta::assert_snapshot!(sv);
}

// ── Regfile ───────────────────────────────────────────────────────────────────

#[test]
fn test_int_regs() {
    let source = include_str!("int_regs.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module IntRegs"));
    assert!(sv.contains("parameter int NREGS = 32"));
    assert!(sv.contains("logic [DATA_WIDTH-1:0] rf_data [0:NREGS-1]"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("always_comb"));
    // forwarding (port 0 and port 1 each get their own unrolled block)
    assert!(sv.contains("write_en && write_addr == read0_addr"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_active_low_reset() {
    let source = include_str!("reset_low.arch");
    let sv = compile_to_sv(source);
    // reset condition must be inverted
    assert!(sv.contains("if ((!rst_n))"), "expected inverted reset condition, got:\n{sv}");
    // must NOT contain bare active-high check
    assert!(!sv.contains("if (rst_n)"), "unexpected active-high reset check:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_ram_missing_port_group_errors() {
    let source = r#"
domain SysDomain
  freq_mhz: 100,
end domain SysDomain

ram BadRam
  param DEPTH: const = 64;
  param WIDTH: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  kind single;
  read: sync;
  store
    data: Vec<WIDTH, DEPTH>;
  end store
  // Missing port group
end ram BadRam
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err());
}
