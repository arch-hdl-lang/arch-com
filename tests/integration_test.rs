use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn compile_to_sv(source: &str) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator lowering error");
    let ast = elaborate::lower_threads(ast).expect("lower_threads error");
    let ast = elaborate::lower_pipe_reg_ports(ast).expect("lower_pipe_reg_ports error");
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit_channel dispatch error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let codegen = Codegen::new(&symbols, &ast, overload_map);
    codegen.generate()
}

#[test]
fn test_top_counter_compiles() {
    let source = include_str!("../examples/top_counter.arch");
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
    result = (a + b).trunc<32>();
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
  freq_mhz: 100
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

// ── Let bindings ──────────────────────────────────────────────────────────────

#[test]
fn test_let_bindings() {
    let source = include_str!("../examples/let_bindings.arch");
    let sv = compile_to_sv(source);
    // Typed let: emits declared type then a separate assign
    assert!(sv.contains("logic [7:0] mask;"), "expected typed let decl, got:\n{sv}");
    assert!(sv.contains("assign mask = a & b;"), "expected typed let assign, got:\n{sv}");
    // Untyped let: emits logic declaration + assign (same pattern as typed let)
    assert!(sv.contains("logic same;"), "expected untyped let decl, got:\n{sv}");
    assert!(sv.contains("assign same = a == b;"), "expected untyped let assign, got:\n{sv}");
    // Outputs driven from the let-bound wires
    assert!(sv.contains("assign masked = mask;"), "expected masked assign, got:\n{sv}");
    assert!(sv.contains("assign equal = same;"), "expected equal assign, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

// ── FSM ───────────────────────────────────────────────────────────────────────

#[test]
fn test_fsm_traffic_light() {
    let source = include_str!("../examples/traffic_light.arch");
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
  state [A, B]
  default state C;
  state A
    comb
      result = true;
    end comb
    -> B when true;
  end state A
  state B
    comb
      result = false;
    end comb
    -> A when true;
  end state B
end fsm Broken
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    // `C` is not in {A, B} — resolve should error
    assert!(resolve::resolve(&ast).is_err());
}

// ── FIFO ──────────────────────────────────────────────────────────────────────

#[test]
fn test_sync_fifo() {
    let source = include_str!("../examples/sync_fifo.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("$clog2(DEPTH)"));
    assert!(sv.contains("assign full"));
    assert!(sv.contains("assign empty"));
    assert!(sv.contains("assign push_ready"));
    assert!(sv.contains("assign pop_valid"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("parameter int") && sv.contains("DATA_WIDTH"));
    assert!(sv.contains("mem [0:DEPTH-1]"));
    // Not async
    assert!(!sv.contains("bin2gray"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_async_fifo() {
    let source = include_str!("../examples/async_fifo.arch");
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
  freq_mhz: 100
end domain SysDomain

fifo BadFifo
  param DEPTH: const = 8;
  param TYPE: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in TYPE;
  // Missing: pop_valid, pop_ready, pop_data
end fifo BadFifo
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err());
}

#[test]
fn test_lifo() {
    let source = include_str!("../examples/lifo.arch");
    let sv = compile_to_sv(source);
    // LIFO uses single stack pointer, not wr_ptr/rd_ptr
    assert!(sv.contains("sp"));
    assert!(!sv.contains("wr_ptr"));
    assert!(!sv.contains("rd_ptr"));
    assert!(sv.contains("$clog2(DEPTH + 1)"));
    assert!(sv.contains("assign full"));
    assert!(sv.contains("assign empty"));
    assert!(sv.contains("mem[sp - 1]"));
    assert!(sv.contains("sp <= sp + 1"));
    assert!(sv.contains("sp <= sp - 1"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_lifo_async_error() {
    let source = r#"
domain WrDomain
  freq_mhz: 100
end domain WrDomain
domain RdDomain
  freq_mhz: 50
end domain RdDomain

fifo BadLifo
  kind lifo;
  param DEPTH: const = 8;
  param TYPE: type = UInt<8>;
  port wr_clk: in Clock<WrDomain>;
  port rd_clk: in Clock<RdDomain>;
  port rst: in Reset<Async>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in TYPE;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out TYPE;
end fifo BadLifo
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err());
}

// ── RAM ───────────────────────────────────────────────────────────────────────

#[test]
fn test_single_port_ram() {
    let source = include_str!("../examples/single_port_ram.arch");
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
fn test_rom_lut() {
    let source = include_str!("../examples/rom_lut.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module RomLut"));
    // ROM has no write port — only read
    assert!(!sv.contains("wr_"), "ROM must not have a write port");
    // Init values from inline array (hex literals)
    assert!(sv.contains("mem[0] = 8'h0"));
    assert!(sv.contains("mem[4] = 8'h7F"));
    assert!(sv.contains("mem[7] = 8'h31"));
    // latency 1 — registered read
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("rd_data_r <= mem[rd_addr]"));
    assert!(sv.contains("assign rd_data = rd_data_r"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_simple_dual_ram() {
    let source = include_str!("../examples/simple_dual_ram.arch");
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
    let source = include_str!("../examples/wrap_counter.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module WrapCounter"));
    assert!(sv.contains("parameter int MAX = 15"));
    assert!(sv.contains("logic [3:0] count_r"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("assign value = count_r"));
    assert!(sv.contains("assign at_max"));
    insta::assert_snapshot!(sv);
}

// ── Arbiter ───────────────────────────────────────────────────────────────────

#[test]
fn test_bus_arbiter() {
    let source = include_str!("../examples/bus_arbiter.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module BusArbiter"));
    assert!(sv.contains("parameter int NUM_REQ = 4"));
    assert!(sv.contains("input logic [NUM_REQ-1:0] request_valid"));
    assert!(sv.contains("logic [NUM_REQ-1:0] request_ready"));
    assert!(sv.contains("rr_ptr_r"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("always_comb"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_arbiter_custom_hook() {
    let source = include_str!("../examples/arbiter_custom_hook.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module QosArbiter"));
    assert!(sv.contains("function automatic"));
    assert!(sv.contains("QosGrant(request_valid, last_grant_r, qos)"));
    assert!(sv.contains("last_grant_r"));
    assert!(sv.contains("grant_onehot"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_arbiter_custom_hook_missing_error() {
    // Custom policy without hook should error
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter BadArb
  policy MyFunc;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter BadArb
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected error for custom policy without hook");
}

#[test]
fn test_arbiter_latency2() {
    let source = include_str!("../examples/arbiter_latency2.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module LatencyArbiter"));
    // Should have _comb intermediate signals
    assert!(sv.contains("grant_valid_comb"));
    assert!(sv.contains("grant_requester_comb"));
    assert!(sv.contains("request_ready_comb"));
    // Should have pipeline register stage
    assert!(sv.contains("grant_valid <= grant_valid_comb"));
    assert!(sv.contains("grant_requester <= grant_requester_comb"));
    assert!(sv.contains("request_ready <= request_ready_comb"));
    insta::assert_snapshot!(sv);
}

// ── Template ─────────────────────────────────────────────────────────────────

#[test]
fn test_template_basic() {
    let source = include_str!("../examples/template_basic.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module MyArbiter"));
    assert!(!sv.contains("template")); // templates don't emit SV
    assert!(sv.contains("function automatic"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_template_missing_port_error() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

template MyTmpl
  port clk: in Clock<SysDomain>;
  port data_out: out UInt<8>;
end template MyTmpl

module BadModule implements MyTmpl
  port clk: in Clock<SysDomain>;
  port other: out UInt<8>;
  let other = 0;
end module BadModule
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected error for missing template port");
}

// ── Regfile ───────────────────────────────────────────────────────────────────

#[test]
fn test_int_regs() {
    let source = include_str!("../examples/int_regs.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module IntRegs"));
    assert!(sv.contains("parameter int NREGS = 32"));
    assert!(sv.contains("rf_data [0:NREGS-1]"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("always_comb"));
    // forwarding (port 0 and port 1 each get their own unrolled block)
    assert!(sv.contains("write_en && write_addr == read0_addr"));
    insta::assert_snapshot!(sv);
}

#[test]
fn test_active_low_reset() {
    let source = include_str!("../examples/reset_low.arch");
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
  freq_mhz: 100
end domain SysDomain

ram BadRam
  kind single;
  latency 1;
  param DEPTH: const = 64;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  store
    data: Vec<T, DEPTH>;
  end store
  // Missing port group
end ram BadRam
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err());
}

#[test]
fn test_implicit_truncation_is_error() {
    // `r <= r + 1` widens UInt<8> → UInt<9>; must be a compile error.
    // The fix is to write `r <= (r + 1).trunc<8>()` explicitly.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BadCounter
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port count: out UInt<8>;
  reg count_r: UInt<8> init 0 reset rst=>0;
  seq on clk rising
    count_r <= count_r + 1;
  end seq
  comb
    count = count_r;
  end comb
end module BadCounter
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected type error for implicit truncation");
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|e| format!("{e:?}").contains("width mismatch")
            || format!("{e:?}").contains("trunc")),
        "expected width-mismatch error mentioning trunc, got: {errors:?}"
    );
}

// ── Generate ──────────────────────────────────────────────────────────────────

#[test]
fn test_generate_for() {
    let source = include_str!("../examples/generate_for.arch");
    let sv = compile_to_sv(source);
    // Ports are declared as Vec<Bool, N> at module scope — the SV boundary is
    // a single packed vector per direction, not N separately-named scalars.
    assert!(sv.contains("input logic [N-1:0] req"), "expected Vec req port, got:\n{sv}");
    assert!(sv.contains("output logic [N-1:0] gnt"), "expected Vec gnt port, got:\n{sv}");
    // generate_for unrolls the insts into gen_i blocks.
    assert!(sv.contains("gen_i"), "expected gen_i block, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_generate_if_true() {
    let source = include_str!("../examples/generate_if.arch");
    let sv = compile_to_sv(source);
    // generate_if true → debug_out port is included
    assert!(sv.contains("debug_out"), "expected debug_out port, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_generate_if_param_default_true() {
    // generate_if using a param default value of 1 → port included
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module ParamDebug
  param ENABLE_DEBUG: const = 1;
  port clk: in Clock<SysDomain>;

  generate_if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate_if

  comb
    debug_out = 0;
  end comb
end module ParamDebug
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("debug_out"), "expected debug_out when ENABLE_DEBUG=1, got:\n{sv}");
}

#[test]
fn test_generate_if_param_zero_excludes_port() {
    // generate_if PARAM where PARAM default = 0 should exclude
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module NoDebug2
  param ENABLE_DEBUG: const = 0;
  port clk: in Clock<SysDomain>;

  generate_if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate_if

  comb
  end comb
end module NoDebug2
"#;
    let sv = compile_to_sv(source);
    assert!(!sv.contains("debug_out"), "debug_out should be excluded when ENABLE_DEBUG=0, got:\n{sv}");
}

#[test]
fn test_generate_if_param_comparison() {
    // generate_if PARAM > 0 style condition
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module CmpDebug
  param LOG_LEVEL: const = 2;
  port clk: in Clock<SysDomain>;

  generate_if LOG_LEVEL > 1
    port verbose_out: out UInt<8>;
  end generate_if

  comb
    verbose_out = 0;
  end comb
end module CmpDebug
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("verbose_out"), "expected verbose_out when LOG_LEVEL=2 > 1, got:\n{sv}");
}

#[test]
fn test_generate_if_inst_override_enables_port() {
    // default = 0, inst overrides to 1 → port MUST be included.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Inner
  param ENABLE_DEBUG: const = 0;
  port clk: in Clock<SysDomain>;

  generate_if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate_if

  comb
    debug_out = 0;
  end comb
end module Inner

module Outer
  port clk: in Clock<SysDomain>;
  port out_dbg: out UInt<8>;

  inst inner: Inner
    param ENABLE_DEBUG = 1;
    clk <- clk;
    debug_out -> out_dbg;
  end inst inner
end module Outer
"#;
    let sv = compile_to_sv(source);
    // The elaborated Inner module (single variant, ENABLE_DEBUG=1) must have debug_out.
    // Single inst → no name mangling, module keeps its original name.
    assert!(sv.contains("debug_out"), "Inner should have debug_out when ENABLE_DEBUG=1:\n{sv}");
    assert!(sv.contains("module Inner"), "module should keep original name for single variant:\n{sv}");
}

#[test]
fn test_generate_if_inst_override_disables_port() {
    // default = 1, inst overrides to 0 → port must be excluded.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Inner2
  param ENABLE_DEBUG: const = 1;
  port clk: in Clock<SysDomain>;

  generate_if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate_if

  comb
  end comb
end module Inner2

module Outer2
  port clk: in Clock<SysDomain>;

  inst inner2: Inner2
    param ENABLE_DEBUG = 0;
    clk <- clk;
  end inst inner2
end module Outer2
"#;
    let sv = compile_to_sv(source);
    // Single inst → no mangling, module keeps its name but is elaborated with ENABLE_DEBUG=0.
    assert!(!sv.contains("debug_out"), "Inner2 should NOT have debug_out when ENABLE_DEBUG=0:\n{sv}");
    assert!(sv.contains("module Inner2"), "module should keep original name for single variant:\n{sv}");
}

#[test]
fn test_generate_monomorphize_two_variants() {
    // Same module instantiated with ENABLE=0 and ENABLE=1 → two distinct SV modules.
    // The unconditional output avoids comb-block issues; generate only adds an inst.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Sub
  param ENABLE: const = 0;
  port clk: in Clock<SysDomain>;
  port result: out Bool;
  comb
    result = false;
  end comb
end module Sub

module Top
  port clk: in Clock<SysDomain>;
  port out_a: out Bool;
  port out_b: out Bool;

  inst sub_on: Sub
    param ENABLE = 1;
    clk <- clk;
    result -> out_a;
  end inst sub_on

  inst sub_off: Sub
    param ENABLE = 0;
    clk <- clk;
    result -> out_b;
  end inst sub_off
end module Top
"#;
    let sv = compile_to_sv(source);
    // Both variant module declarations must be emitted
    assert!(sv.contains("module Sub__ENABLE_0"), "expected Sub__ENABLE_0 module:\n{sv}");
    assert!(sv.contains("module Sub__ENABLE_1"), "expected Sub__ENABLE_1 module:\n{sv}");
    // Top's inst blocks must reference the renamed variants
    assert!(sv.contains("Sub__ENABLE_1"), "Top should reference Sub__ENABLE_1:\n{sv}");
    assert!(sv.contains("Sub__ENABLE_0"), "Top should reference Sub__ENABLE_0:\n{sv}");
    // sub_on and sub_off instance names must still appear
    assert!(sv.contains("sub_on"), "expected sub_on instance:\n{sv}");
    assert!(sv.contains("sub_off"), "expected sub_off instance:\n{sv}");
}

#[test]
fn test_generate_monomorphize_different_port_lists() {
    // Critical test: same module instantiated twice with params that produce
    // DIFFERENT port lists via `generate_if`.  Uses a conditional INPUT port
    // so the module's comb block doesn't need to reference non-existent ports.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Inner
  param ENABLE_DEBUG: const = 0;
  port clk: in Clock<SysDomain>;
  port result: out Bool;

  generate_if ENABLE_DEBUG
    port debug_in: in UInt<8>;
  end generate_if

  comb
    result = false;
  end comb
end module Inner

module Outer
  port clk: in Clock<SysDomain>;
  port out_a: out Bool;
  port out_b: out Bool;
  port dbg_val: in UInt<8>;

  inst inner_on: Inner
    param ENABLE_DEBUG = 1;
    clk <- clk;
    result -> out_a;
    debug_in <- dbg_val;
  end inst inner_on

  inst inner_off: Inner
    param ENABLE_DEBUG = 0;
    clk <- clk;
    result -> out_b;
  end inst inner_off
end module Outer
"#;
    let sv = compile_to_sv(source);
    // Two distinct SV modules emitted
    assert!(sv.contains("module Inner__ENABLE_DEBUG_0"), "missing Inner__ENABLE_DEBUG_0:\n{sv}");
    assert!(sv.contains("module Inner__ENABLE_DEBUG_1"), "missing Inner__ENABLE_DEBUG_1:\n{sv}");
    // ENABLE_DEBUG=1 variant has debug_in port; ENABLE_DEBUG=0 does not.
    // Verify by checking what each module declaration contains.
    let debug_1_block = sv.split("module Inner__ENABLE_DEBUG_1").nth(1)
        .and_then(|s| s.split("endmodule").next())
        .unwrap_or("");
    let debug_0_block = sv.split("module Inner__ENABLE_DEBUG_0").nth(1)
        .and_then(|s| s.split("endmodule").next())
        .unwrap_or("");
    assert!(debug_1_block.contains("debug_in"), "ENABLE_DEBUG=1 variant missing debug_in:\n{sv}");
    assert!(!debug_0_block.contains("debug_in"), "ENABLE_DEBUG=0 variant should not have debug_in:\n{sv}");
    // Inst sites reference the correct variants (params appear between name and instance)
    assert!(sv.contains("Inner__ENABLE_DEBUG_1") && sv.contains("inner_on"),
        "inner_on should use _1 variant:\n{sv}");
    assert!(sv.contains("Inner__ENABLE_DEBUG_0") && sv.contains("inner_off"),
        "inner_off should use _0 variant:\n{sv}");
}

#[test]
fn test_generate_if_false_excludes_port() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module NoDebug
  port clk: in Clock<SysDomain>;
  generate_if false
    port debug_out: out UInt<8>;
  end generate_if
  comb
  end comb
end module NoDebug
"#;
    let sv = compile_to_sv(source);
    assert!(!sv.contains("debug_out"), "debug_out should be excluded when condition is false, got:\n{sv}");
}

// ── Mixed reset / no-reset in always block ───────────────────────────────────

#[test]
fn test_mixed_reset_and_no_reset() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module MixedReset
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<8>;
  port count_out: out UInt<8>;
  port pipe_out: out UInt<8>;

  reg count_r: UInt<8> init 0 reset rst=>0;
  reg pipe_r:  UInt<8> init 0 reset none;

  seq on clk rising
    count_r <= (count_r + 1).trunc<8>();
    pipe_r  <= data_in;
  end seq

  comb
    count_out = count_r;
    pipe_out  = pipe_r;
  end comb
end module MixedReset
"#;
    let sv = compile_to_sv(source);
    // count_r has reset: should appear inside if(rst)/else guard in first always_ff
    assert!(sv.contains("if (rst) begin"), "expected reset guard, got:\n{sv}");
    assert!(sv.contains("count_r <= 0;"), "expected count_r reset init, got:\n{sv}");
    // pipe_r has reset none: must be in a SEPARATE always_ff block (no reset in sensitivity list).
    // Mixing resetable and non-resetable regs in one always_ff with async reset causes
    // synthesis tools to infer unintended clock gating on the reset path.
    let always_blocks: Vec<&str> = sv.split("always_ff").collect();
    assert!(always_blocks.len() >= 3, "expected at least 2 always_ff blocks (reset + no-reset), got:\n{sv}");
    // The second always_ff should contain pipe_r and NOT have reset in sensitivity
    let second_block = always_blocks[2];
    assert!(second_block.contains("pipe_r <= data_in"), "pipe_r should be in separate always_ff, got:\n{sv}");
    assert!(!second_block.contains("rst"), "no-reset always_ff should not reference rst, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_reset_only_reg_is_driven() {
    // A `reg` declared with a reset clause but never assigned in any seq
    // block should still get its reset value emitted into an always_ff.
    // Without this, Verilator lints the reg as undriven and the flop sits
    // at X after reset — which silently breaks spec-common RO-constant
    // CSRs (xdebugver, mvendorid, mhpmevent*, etc. — the pattern
    // `field { sw = r; hw = r; reset = <const>; }` compiles to a reg
    // whose reset value is the only thing that ever writes it).
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module RoConst
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port out_val: out UInt<32>;

  reg roconst_r: UInt<32> reset rst=>32'h4;

  comb
    out_val = roconst_r;
  end comb
end module RoConst
"#;
    let sv = compile_to_sv(source);
    // The reset value must be driven somewhere in an always_ff.
    assert!(sv.contains("always_ff @(posedge clk)"),
        "expected an always_ff for the reset-only reg, got:\n{sv}");
    assert!(sv.contains("if (rst)"),
        "expected reset guard, got:\n{sv}");
    // arch-com emits the literal as decimal; accept either form since
    // what matters is that the RHS is the reset value (4).
    assert!(sv.contains("roconst_r <= 32'd4;") || sv.contains("roconst_r <= 32'h4;"),
        "expected roconst_r reset-init assignment, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_reset_only_reg_alongside_seq_block() {
    // Module has an active seq block driving one reg AND an orphan
    // reset-only reg. The orphan needs its own always_ff without
    // disturbing the existing seq-block-generated one.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module MixedOrphan
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<8>;
  port ticker_out: out UInt<8>;
  port const_out: out UInt<32>;

  reg ticker_r: UInt<8> init 0 reset rst=>0;
  reg constant_r: UInt<32> reset rst=>32'd42;

  seq on clk rising
    ticker_r <= data_in;
  end seq

  comb
    ticker_out = ticker_r;
    const_out  = constant_r;
  end comb
end module MixedOrphan
"#;
    let sv = compile_to_sv(source);
    // Original seq-block always_ff resets ticker_r.
    assert!(sv.contains("ticker_r <= 0;"),
        "expected ticker_r reset in seq-block always_ff, got:\n{sv}");
    // Orphan reset always_ff fires for constant_r.
    assert!(sv.contains("constant_r <= 32'd42;"),
        "expected constant_r orphan reset assignment, got:\n{sv}");
    // Two distinct always_ff blocks — one for each.
    let always_count = sv.matches("always_ff @").count();
    assert!(always_count >= 2,
        "expected >=2 always_ff blocks (seq + orphan), got {always_count}:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_reset_consistency_error_mixed_signals() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BadMixed
  port clk: in Clock<SysDomain>;
  port rst_a: in Reset<Sync>;
  port rst_b: in Reset<Sync>;
  port out_a: out UInt<8>;
  port out_b: out UInt<8>;

  reg reg_a: UInt<8> init 0 reset rst_a=>0;
  reg reg_b: UInt<8> init 0 reset rst_b=>0;

  seq on clk rising
    reg_a <= (reg_a + 1).trunc<8>();
    reg_b <= (reg_b + 1).trunc<8>();
  end seq

  comb
    out_a = reg_a;
    out_b = reg_b;
  end comb
end module BadMixed
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected error for mixed reset signals in same always block");
}

#[test]
fn test_reset_consistency_error_mixed_sync_async() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BadSyncAsync
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port out_a: out UInt<8>;
  port out_b: out UInt<8>;

  reg reg_a: UInt<8> init 0 reset rst=>0;
  reg reg_b: UInt<8> init 0 reset rst=>0 Async high;

  seq on clk rising
    reg_a <= (reg_a + 1).trunc<8>();
    reg_b <= (reg_b + 1).trunc<8>();
  end seq

  comb
    out_a = reg_a;
    out_b = reg_b;
  end comb
end module BadSyncAsync
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected error for mixing sync and async reset in same always block");
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

#[test]
fn test_simple_pipeline() {
    let source = include_str!("../examples/simple_pipeline.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module SimplePipe"), "missing module header");
    assert!(sv.contains("fetch_valid_r"), "missing fetch valid register");
    assert!(sv.contains("writeback_valid_r"), "missing writeback valid register");
    assert!(sv.contains("fetch_captured"), "missing fetch stage register");
    assert!(sv.contains("writeback_result"), "missing writeback stage register");
    assert!(sv.contains("always_ff"), "missing always_ff block");
    assert!(sv.contains("assign data_out = writeback_result"), "missing comb output");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_pipeline_comb_only_stage_error() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

pipeline BadPipe
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<8>;
  port data_out: out UInt<8>;

  stage CombOnly
    comb
      data_out = data_in;
    end comb
  end stage CombOnly

end pipeline BadPipe
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let elaborated = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&elaborated).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &elaborated);
    let result = checker.check();
    assert!(result.is_err(), "expected error for comb-only pipeline stage");
}

#[test]
fn test_pipeline_bad_flush_target_error() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

pipeline BadFlush
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<8>;
  port data_out: out UInt<8>;

  stage Fetch
    reg captured: UInt<8> init 0 reset rst=>0;
    seq on clk rising
      captured <= data_in;
    end seq
  end stage Fetch

  stage Writeback
    reg result: UInt<8> init 0 reset rst=>0;
    seq on clk rising
      result <= Fetch.captured;
    end seq
    comb
      data_out = result;
    end comb
  end stage Writeback

  flush Nonexistent when data_in == 0;

end pipeline BadFlush
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let elaborated = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&elaborated).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &elaborated);
    let result = checker.check();
    assert!(result.is_err(), "expected error for undeclared flush target stage");
}

#[test]
fn test_cpu_pipeline() {
    let source = include_str!("../examples/cpu_pipeline.arch");
    let sv = compile_to_sv(source);
    // Module header
    assert!(sv.contains("module CpuPipe"), "missing module header");
    // Per-stage stall chain
    assert!(sv.contains("fetch_stall"), "missing fetch_stall signal");
    assert!(sv.contains("decode_stall"), "missing decode_stall signal");
    assert!(sv.contains("(!imem_valid)"), "missing Fetch stall condition");
    // Backpressure propagation
    assert!(sv.contains("fetch_stall = (!imem_valid) || decode_stall"), "missing backpressure chain");
    // Stage register updates with stall guard
    assert!(sv.contains("if (!fetch_stall)"), "missing fetch stall guard");
    assert!(sv.contains("if (!decode_stall)"), "missing decode stall guard");
    // Bubble insertion
    assert!(sv.contains("fetch_stall ? 1'b0 : fetch_valid_r"), "missing bubble insertion");
    // Flush
    assert!(sv.contains("if (branch_taken)"), "missing flush condition");
    assert!(sv.contains("fetch_valid_r <= 1'b0"), "missing fetch flush");
    assert!(sv.contains("decode_valid_r <= 1'b0"), "missing decode flush");
    // Cross-stage references rewritten
    assert!(sv.contains("fetch_instr"), "missing rewritten cross-stage ref");
    assert!(sv.contains("decode_rs1_val"), "missing rewritten decode ref");
    assert!(sv.contains("execute_alu_result"), "missing rewritten execute ref");
    // Outputs
    assert!(sv.contains("assign wb_data = writeback_result"), "missing wb output");
    assert!(sv.contains("assign pc_out = fetch_pc"), "missing pc output");
    // Explicit forwarding mux
    assert!(sv.contains("decode_rs1_fwd"), "missing forwarding mux wire");
    assert!(sv.contains("always_comb"), "missing always_comb for forwarding mux");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_trunc_bit_range() {
    // Test trunc<N,M> (bit-range extraction) alongside trunc<N> (lowest N bits)
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BitExtract
  param XLEN: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port instr: in UInt<XLEN>;
  port opcode: out UInt<7>;
  port rd: out UInt<5>;
  port funct3: out UInt<3>;

  reg opcode_r: UInt<7> init 0 reset rst=>0;
  reg rd_r: UInt<5> init 0 reset rst=>0;
  reg funct3_r: UInt<3> init 0 reset rst=>0;

  seq on clk rising
    opcode_r <= instr.trunc<7>();
    rd_r     <= instr[11:7];
    funct3_r <= instr[14:12];
  end seq

  comb
    opcode = opcode_r;
    rd = rd_r;
    funct3 = funct3_r;
  end comb
end module BitExtract
"#;
    let sv = compile_to_sv(source);
    // trunc<7>() → 7'(instr)
    assert!(sv.contains("7'(instr)"), "expected trunc<7> → 7'(instr), got:\n{sv}");
    // trunc<11,7>() → instr[11:7]
    assert!(sv.contains("instr[11:7]"), "expected trunc<11,7> → instr[11:7], got:\n{sv}");
    // trunc<14,12>() → instr[14:12]
    assert!(sv.contains("instr[14:12]"), "expected trunc<14,12> → instr[14:12], got:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_pipeline_instantiation() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

pipeline SimplePipe
  param XLEN: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<XLEN>;
  port data_out: out UInt<XLEN>;

  stage Fetch
    reg captured: UInt<XLEN> init 0 reset rst=>0;
    seq on clk rising
      captured <= data_in;
    end seq
  end stage Fetch

  stage Writeback
    reg result: UInt<XLEN> init 0 reset rst=>0;
    seq on clk rising
      result <= Fetch.captured;
    end seq
    comb
      data_out = result;
    end comb
  end stage Writeback

end pipeline SimplePipe

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port din: in UInt<32>;
  port dout: out UInt<32>;

  inst pipe0: SimplePipe
    clk <- clk;
    rst <- rst;
    data_in <- din;
    data_out -> dout;
  end inst pipe0
end module Top
"#;
    let sv = compile_to_sv(source);
    // Should contain the pipeline module
    assert!(sv.contains("module SimplePipe"), "missing pipeline module");
    // Should contain the top module with instantiation
    assert!(sv.contains("module Top"), "missing top module");
    assert!(sv.contains("SimplePipe pipe0"), "missing pipeline instantiation");
    assert!(sv.contains(".data_in(din)"), "missing data_in connection");
    assert!(sv.contains(".data_out(dout)"), "missing data_out connection");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_pipeline_stage_inst() {
    // Instantiate an ALU module inside a pipeline Execute stage
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Alu
  param T: const = 32;
  port a: in UInt<T>;
  port b: in UInt<T>;
  port result: out UInt<T>;

  comb
    result = (a + b).trunc<T>();
  end comb
end module Alu

pipeline AluPipe
  param XLEN: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port op_a: in UInt<XLEN>;
  port op_b: in UInt<XLEN>;
  port result_out: out UInt<XLEN>;

  stage Fetch
    reg a_r: UInt<XLEN> init 0 reset rst=>0;
    reg b_r: UInt<XLEN> init 0 reset rst=>0;
    seq on clk rising
      a_r <= op_a;
      b_r <= op_b;
    end seq
  end stage Fetch

  stage Execute
    reg alu_out: UInt<XLEN> init 0 reset rst=>0;
    seq on clk rising
      alu_out <= (Fetch.a_r + Fetch.b_r).trunc<XLEN>();
    end seq
    inst alu0: Alu
      a <- Fetch.a_r;
      b <- Fetch.b_r;
      result -> result_out;
    end inst alu0
  end stage Execute

end pipeline AluPipe
"#;
    let sv = compile_to_sv(source);
    // ALU module should be emitted
    assert!(sv.contains("module Alu"), "missing Alu module");
    // Pipeline should contain the inst
    assert!(sv.contains("Alu alu0"), "missing Alu instantiation inside pipeline stage");
    assert!(sv.contains(".a("), "missing port a connection");
    assert!(sv.contains(".b("), "missing port b connection");
    assert!(sv.contains(".result(result_out)"), "missing result connection");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_clog2_in_type_args() {
    // $clog2(DEPTH) in type width expressions
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module FifoCtrl
  param DEPTH: const = 16;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port wr_ptr: out UInt<$clog2(DEPTH)>;
  port rd_ptr: out UInt<$clog2(DEPTH)>;
  port count: out UInt<$clog2(DEPTH) + 1>;

  reg wr_r: UInt<$clog2(DEPTH)> init 0 reset rst=>0;
  reg rd_r: UInt<$clog2(DEPTH)> init 0 reset rst=>0;

  seq on clk rising
    wr_r <= (wr_r + 1).trunc<$clog2(DEPTH)>();
    rd_r <= rd_r;
  end seq

  comb
    wr_ptr = wr_r;
    rd_ptr = rd_r;
    count = wr_r - rd_r;
  end comb
end module FifoCtrl
"#;
    let sv = compile_to_sv(source);
    // $clog2(DEPTH) should appear in port widths
    assert!(sv.contains("$clog2(DEPTH)"), "expected $clog2(DEPTH) in SV output, got:\n{sv}");
    // $clog2(DEPTH) + 1 in count port
    assert!(sv.contains("$clog2(DEPTH) + 1"), "expected $clog2(DEPTH) + 1 in SV output, got:\n{sv}");
    // trunc<$clog2(DEPTH)>() should emit as size cast
    assert!(sv.contains("$clog2(DEPTH)'("), "expected $clog2(DEPTH)'(...) size cast, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

// ── Linklist tests ─────────────────────────────────────────────────────────

#[test]
fn test_linklist_basic_compiles() {
    let source = r#"
linklist TaskQueue
  param DEPTH: const = 8;
  param DATA: type = UInt<32>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  track tail: true;
  track length: true;
  op alloc
    latency: 1;
    port req_valid:   in Bool;
    port req_ready:   out Bool;
    port resp_valid:  out Bool;
    port resp_handle: out UInt<3>;
  end op alloc
  op free
    latency: 1;
    port req_valid:  in Bool;
    port req_ready:  out Bool;
    port req_handle: in UInt<3>;
  end op free
  op delete_head
    latency: 2;
    port req_valid:  in Bool;
    port req_ready:  out Bool;
    port resp_valid: out Bool;
    port resp_data:  out DATA;
  end op delete_head
  port empty:  out Bool;
  port full:   out Bool;
  port length: out UInt<4>;
end linklist TaskQueue
"#;
    let sv = compile_to_sv(source);
    // Module header
    assert!(sv.contains("module TaskQueue #("), "missing module header");
    assert!(sv.contains("parameter int  DEPTH = 8"), "missing DEPTH param");
    assert!(sv.contains("parameter type DATA"), "missing DATA param");
    // Infrastructure signals
    assert!(sv.contains("_fl_mem"), "missing free list memory");
    assert!(sv.contains("_next_mem"), "missing next pointer RAM");
    assert!(sv.contains("_head_r"), "missing head register");
    assert!(sv.contains("_tail_r"), "missing tail register (track_tail: true)");
    // Status outputs
    assert!(sv.contains("assign empty"), "missing empty assign");
    assert!(sv.contains("assign full"), "missing full assign");
    assert!(sv.contains("assign length"), "missing length assign");
    // Op ports
    assert!(sv.contains("alloc_req_valid"), "missing alloc port");
    assert!(sv.contains("delete_head_resp_data"), "missing delete_head resp_data port");
    // alloc FSM
    assert!(sv.contains("_fl_rdp <= _fl_rdp + 1'b1"), "missing free-list dequeue");
    // delete_head 2-cycle FSM
    assert!(sv.contains("_ctrl_delete_head_busy"), "missing delete_head busy reg");
    assert!(sv.contains("_head_r <= _next_mem"), "missing head advance");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_linklist_doubly_compiles() {
    let source = r#"
linklist SchedList
  param DEPTH: const = 4;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind doubly;
  track tail: true;
  track length: false;
  op alloc
    latency: 1;
    port req_valid:   in Bool;
    port req_ready:   out Bool;
    port resp_valid:  out Bool;
    port resp_handle: out UInt<2>;
  end op alloc
  op next
    latency: 1;
    port req_valid:   in Bool;
    port req_handle:  in UInt<2>;
    port resp_valid:  out Bool;
    port resp_handle: out UInt<2>;
  end op next
  op prev
    latency: 1;
    port req_valid:   in Bool;
    port req_handle:  in UInt<2>;
    port resp_valid:  out Bool;
    port resp_handle: out UInt<2>;
  end op prev
  port empty: out Bool;
  port full:  out Bool;
end linklist SchedList
"#;
    let sv = compile_to_sv(source);
    // Doubly-linked should have prev_mem
    assert!(sv.contains("_prev_mem"), "doubly list missing _prev_mem");
    assert!(sv.contains("_next_mem"), "missing _next_mem");
    // prev op controller
    assert!(sv.contains("_ctrl_prev_resp_handle <= _prev_mem"), "missing prev pointer follow");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_linklist_prev_on_singly_is_error() {
    let source = r#"
linklist BadList
  param DEPTH: const = 4;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  track tail: false;
  track length: false;
  op prev
    latency: 1;
    port req_valid:   in Bool;
    port req_handle:  in UInt<2>;
    port resp_valid:  out Bool;
    port resp_handle: out UInt<2>;
  end op prev
  port empty: out Bool;
  port full:  out Bool;
end linklist BadList
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected type error for prev on singly list");
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| { let s = e.to_string(); s.contains("prev") && s.contains("doubly") }),
            "expected error about prev requiring doubly, got: {:?}", errs);
}

#[test]
fn test_linklist_inst_in_module() {
    // PacketQueue wraps TaskQueue linklist as a push/pop FIFO interface.
    // Verifies that: linklist can be instantiated inside a module,
    // inst output ports are auto-declared as wires, and codegen succeeds.
    let source = std::fs::read_to_string("examples/pkt_queue.arch")
        .expect("pkt_queue.arch not found");
    let tokens = lexer::tokenize(&source).expect("lexer error");
    let mut parser = Parser::new(tokens, &source);
    let parsed = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let codegen = Codegen::new(&symbols, &ast, overload_map);
    let sv = codegen.generate();
    insta::assert_snapshot!(sv);
}

#[test]
fn test_pipe_reg() {
    let source = include_str!("../examples/pipe_reg_test.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("delayed_stg1"));
    assert!(sv.contains("delayed_stg2"));
    assert!(sv.contains("delayed <= delayed_stg2"));
    assert!(sv.contains("always_ff"));
    insta::assert_snapshot!(sv);
}

// ── Indexed member connection syntax ─────────────────────────────────────────

#[test]
fn test_connect_indexed_member() {
    // Tests `port[i].member` syntax in inst connections.
    // The parser transforms `read[0].addr` → `read0_addr`, etc.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

regfile SmallRf
  param NREGS: const = 4;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[2] read
    addr: in UInt<2>;
    data: out UInt<8>;
  end ports read
  ports[1] write
    en:   in Bool;
    addr: in UInt<2>;
    data: in UInt<8>;
  end ports write
end regfile SmallRf

module RfUser
  port clk:   in Clock<SysDomain>;
  port rst:   in Reset<Sync>;
  port sel:   in UInt<2>;
  port out_a: out UInt<8>;
  port out_b: out UInt<8>;

  inst rf: SmallRf
    clk          <- clk;
    rst          <- rst;
    read[0].addr <- sel;
    read[0].data -> out_a;
    read[1].addr <- 0;
    read[1].data -> out_b;
    write.en     <- false;
    write.addr   <- 0;
    write.data   <- 0;
  end inst rf
end module RfUser
"#;
    let sv = compile_to_sv(source);
    // Parser transforms read[0].addr → read0_addr, read[1].data → read1_data
    assert!(sv.contains(".read0_addr"), "expected .read0_addr port connection, got:\n{sv}");
    assert!(sv.contains(".read0_data"), "expected .read0_data port connection, got:\n{sv}");
    assert!(sv.contains(".read1_addr"), "expected .read1_addr port connection, got:\n{sv}");
    assert!(sv.contains(".read1_data"), "expected .read1_data port connection, got:\n{sv}");
    // Also check dot-only syntax: write.en → write_en
    assert!(sv.contains(".write_en"),   "expected .write_en port connection, got:\n{sv}");
    assert!(sv.contains(".write_addr"), "expected .write_addr port connection, got:\n{sv}");
    assert!(sv.contains(".write_data"), "expected .write_data port connection, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

/// Regression: ARCH packed-struct bit layout is *declaration-first = MSB*,
/// matching SV's `struct packed` convention. The first-declared ARCH field
/// must appear *first* inside the emitted `typedef struct packed { ... }`,
/// so it lands in the MSBs of the packed bit vector.
/// Pre-v0.41.1 the compiler reversed field order (first = LSB); this test
/// prevents that from silently returning.
#[test]
fn test_struct_packed_declaration_first_is_msb() {
    let source = r#"
struct Foo
  a: UInt<8>;
  b: UInt<4>;
end struct Foo
"#;
    let sv = compile_to_sv(source);
    let td = sv.find("typedef struct packed").expect("expected typedef struct packed in SV");
    let end = sv[td..].find("} Foo;").expect("expected `} Foo;` closing") + td;
    let body = &sv[td..end];
    let pos_a = body.find("a;").expect("expected field `a`");
    let pos_b = body.find("b;").expect("expected field `b`");
    assert!(
        pos_a < pos_b,
        "field `a` (declared first) must appear before `b` in the packed typedef \
         (SV convention: first listed = MSB). Got:\n{body}"
    );
}

/// Regression: `<=` must be accepted as less-than-or-equal in expression context
/// (assert/cover RHS, comb RHS, let RHS, if conditions, ternary branches), while
/// still working as the non-blocking-assignment token at the statement level.
/// Before the fix, `peek_binop` only listed `>=`, so `cnt <= 200` in an assert
/// would fail to parse with "expected ;, found <=".
#[test]
fn test_lte_in_expression_contexts() {
    let source = r#"
module LteExpr
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port en:  in Bool;

  reg cnt: UInt<8> reset rst => 0;

  seq on clk rising
    if en
      cnt <= (cnt +% 1);
    end if
  end seq

  assert bounded: cnt <= 200;
end module LteExpr
"#;
    let sv = compile_to_sv(source);
    // Statement-level `<=` still produces a non-blocking assignment.
    assert!(sv.contains("cnt <="), "expected seq `cnt <= ...` in SV, got:\n{sv}");
    // Expression-level `<=` appears in the assertion body.
    assert!(
        sv.contains("bounded") && sv.contains("<= 200"),
        "expected assert `cnt <= 200` in SV, got:\n{sv}"
    );
}

#[test]
fn test_handshake_valid_ready_expansion() {
    // Tier 1: a bus with three valid_ready handshake channels should
    // expand into flat valid/ready/payload ports with correct directions.
    let source = "
        bus BusLite
          handshake aw: send kind: valid_ready
            addr: UInt<32>;
            prot: UInt<3>;
          end handshake aw

          handshake b: receive kind: valid_ready
            resp: UInt<2>;
          end handshake b
        end bus BusLite

        module Producer
          port bus_p: initiator BusLite;
          comb
            bus_p.aw_valid = 1'b0;
            bus_p.aw_addr  = 32'h0;
            bus_p.aw_prot  = 3'h0;
            bus_p.b_ready  = 1'b1;
          end comb
        end module Producer
    ";
    let sv = compile_to_sv(source);
    // Send-side valid is OUTPUT, ready is INPUT.
    assert!(sv.contains("output logic bus_p_aw_valid"), "aw_valid should be output on initiator");
    assert!(sv.contains("input logic bus_p_aw_ready"), "aw_ready should be input on initiator");
    assert!(sv.contains("output logic [31:0] bus_p_aw_addr"), "aw payload out");
    // Receive-side b: valid becomes INPUT for the initiator.
    assert!(sv.contains("input logic bus_p_b_valid"), "b_valid should be input on initiator");
    assert!(sv.contains("output logic bus_p_b_ready"), "b_ready should be output on initiator");
    assert!(sv.contains("input logic [1:0] bus_p_b_resp"), "b payload in");
}

#[test]
fn test_handshake_target_flip() {
    // When a handshake-using bus is attached at a `target` port, every
    // signal (valid, ready, payload) must flip — same mechanism the bus
    // perspective flip uses today.
    let source = "
        bus BusLite
          handshake aw: send kind: valid_ready
            addr: UInt<32>;
          end handshake aw
        end bus BusLite

        module Consumer
          port bus_c: target BusLite;
          comb
            bus_c.aw_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let sv = compile_to_sv(source);
    // Target flips: producer-side 'out' becomes 'in' at the consumer.
    assert!(sv.contains("input logic bus_c_aw_valid"), "target flip: aw_valid becomes input");
    assert!(sv.contains("output logic bus_c_aw_ready"), "target flip: aw_ready becomes output");
    assert!(sv.contains("input logic [31:0] bus_c_aw_addr"), "target flip: payload becomes input");
}

#[test]
fn test_handshake_all_variants_parse() {
    // All six variants must parse + type-check and produce the expected
    // control signals with correct directions.
    let source = "
        bus BusAll
          handshake a: send kind: valid_ready  end handshake a
          handshake b: send kind: valid_only   end handshake b
          handshake c: send kind: ready_only   end handshake c
          handshake d: send kind: valid_stall  end handshake d
          handshake e: send kind: req_ack_4phase end handshake e
          handshake f: send kind: req_ack_2phase end handshake f
        end bus BusAll

        module Top
          port p: initiator BusAll;
          comb
            p.a_valid = 1'b0;
            p.b_valid = 1'b0;
            p.d_valid = 1'b0;
            p.e_req   = 1'b0;
            p.f_req   = 1'b0;
          end comb
        end module Top
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("output logic p_a_valid") && sv.contains("input logic p_a_ready"));
    assert!(sv.contains("output logic p_b_valid") && !sv.contains("p_b_ready"));
    assert!(sv.contains("input logic p_c_ready") && !sv.contains("p_c_valid"));
    assert!(sv.contains("output logic p_d_valid") && sv.contains("input logic p_d_stall"));
    assert!(sv.contains("output logic p_e_req") && sv.contains("input logic p_e_ack"));
    assert!(sv.contains("output logic p_f_req") && sv.contains("input logic p_f_ack"));
}

#[test]
fn test_handshake_tier2_valid_ready_assertion() {
    // Tier 2: a valid_ready handshake should emit a `valid_stable` SVA
    // property referencing the flattened valid/ready signals, wrapped in
    // synopsys translate_off/on, with disable iff (rst).
    let source = "
        bus BusLite
          handshake aw: send kind: valid_ready
            addr: UInt<32>;
          end handshake aw
        end bus BusLite

        module Producer
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port bus_p: initiator BusLite;
          comb
            bus_p.aw_valid = 1'b0;
            bus_p.aw_addr  = 32'h0;
          end comb
        end module Producer
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("// Auto-generated handshake protocol assertions"));
    assert!(sv.contains("_auto_hs_bus_p_aw_valid_stable"));
    assert!(sv.contains("(bus_p_aw_valid && !bus_p_aw_ready) |=> bus_p_aw_valid"));
    assert!(sv.contains("disable iff (rst)"));
    assert!(sv.contains("synopsys translate_off"));
    assert!(sv.contains("synopsys translate_on"));
}

#[test]
fn test_handshake_tier2_multiple_variants() {
    // Tier 2: a bus with mixed variants should emit exactly the
    // properties covered by v1 — valid_stable, valid_stable_while_stall,
    // and req_holds_until_ack. Other variants are silently skipped.
    let source = "
        bus BusMix
          handshake a: send kind: valid_ready  end handshake a
          handshake b: send kind: valid_only   end handshake b
          handshake c: send kind: valid_stall  end handshake c
          handshake d: send kind: req_ack_4phase end handshake d
        end bus BusMix

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p: initiator BusMix;
          comb
            p.a_valid = 1'b0;
            p.b_valid = 1'b0;
            p.c_valid = 1'b0;
            p.d_req   = 1'b0;
          end comb
        end module Top
    ";
    let sv = compile_to_sv(source);
    // Covered variants:
    assert!(sv.contains("_auto_hs_p_a_valid_stable"));
    assert!(sv.contains("_auto_hs_p_c_valid_stable_while_stall"));
    assert!(sv.contains("_auto_hs_p_d_req_holds_until_ack"));
    // valid_only has no back-signal, so no property is emitted for `b`:
    assert!(!sv.contains("_auto_hs_p_b_"));
}

#[test]
fn test_handshake_tier2_no_clock_no_assertions() {
    // A module without a Clock port can't host concurrent assertions.
    // Bus ports are still emitted; the assertion block is simply skipped.
    let source = "
        bus BusLite
          handshake aw: send kind: valid_ready end handshake aw
        end bus BusLite

        module Combo
          port bus_p: initiator BusLite;
          comb
            bus_p.aw_valid = 1'b0;
          end comb
        end module Combo
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("output logic bus_p_aw_valid"));
    assert!(!sv.contains("_auto_hs_"));
}

#[test]
fn test_use_bus_does_not_emit_sv_import() {
    // Regression: `use BusName;` referencing a bus (not a package) must
    // NOT emit `import BusName::*;` in the generated SV. Bus ports are
    // fully flattened at the port boundary, and no SV package is
    // synthesized — emitting the import breaks Verilator/iverilog.
    let source = "
        bus BusS
          handshake ch: send kind: valid_ready
            data: UInt<8>;
          end handshake ch
        end bus BusS

        use BusS;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p: initiator BusS;
          comb
            p.ch_valid = 1'b0;
            p.ch_data  = 8'h0;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(!sv.contains("import BusS"),
            "spurious SV import emitted for a bus-typed use:\n{sv}");
}

#[test]
fn test_use_package_still_emits_sv_import() {
    // The positive case: `use Foo;` of an actual `package Foo ... end`
    // still emits `import Foo::*;` because package contents become an
    // SV package whose typedefs need importing.
    let source = "
        package PkgA
          enum Op
            ADD,
            SUB,
          end enum Op
        end package PkgA

        use PkgA;

        module M
          port o: out Op;
          comb
            o = Op::ADD;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("import PkgA::*;"),
            "expected SV import for a package-typed use:\n{sv}");
}

fn compile_to_sim_h(source: &str, inputs_start_uninit: bool) -> String {
    use arch::sim_codegen::SimCodegen;
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = arch::elaborate::lower_threads(ast).expect("lower threads error");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("lower pipe_reg error");
    let ast = arch::elaborate::lower_credit_channel_dispatch(ast).expect("cc dispatch error");
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let sim = SimCodegen::new(&symbols, &ast, overload_map)
        .check_uninit(inputs_start_uninit)
        .inputs_start_uninit(inputs_start_uninit);
    let models = sim.generate();
    // Concatenate all model headers + impl files — tests can grep across them.
    models.iter()
        .map(|m| format!("{}\n// ---\n{}", m.header, m.impl_))
        .collect::<Vec<_>>()
        .join("\n// ---\n")
}

#[test]
fn test_inputs_start_uninit_bus_flattened() {
    // --inputs-start-uninit should now emit shadow vinit bits and setters
    // for each flattened INPUT signal of a bus-typed port.
    let source = "
        bus BusSimple
          data:  in UInt<8>;
          valid: in Bool;
        end bus BusSimple

        use BusSimple;

        module Reader
          port b:     initiator BusSimple;
          port out_r: out UInt<8>;
          comb
            if b.valid
              out_r = b.data;
            else
              out_r = 8'h0;
            end if
          end comb
        end module Reader
    ";
    let h = compile_to_sim_h(source, true);
    assert!(h.contains("bool _b_data_vinit = false;"),
            "expected shadow bit for flattened bus input 'b_data':\n{h}");
    assert!(h.contains("bool _b_valid_vinit = false;"),
            "expected shadow bit for flattened bus input 'b_valid':\n{h}");
    assert!(h.contains("void set_b_data("),
            "expected setter for flattened bus input 'b_data':\n{h}");
    assert!(h.contains("void set_b_valid("),
            "expected setter for flattened bus input 'b_valid':\n{h}");
}

#[test]
fn test_inputs_start_uninit_bus_skips_output_direction() {
    // Target perspective flips direction: 'in' signals on the bus
    // become 'out' from the module's side — those MUST NOT get
    // uninit tracking (they're driven by this module).
    let source = "
        bus BusSimple
          data:  in UInt<8>;
          valid: in Bool;
        end bus BusSimple

        use BusSimple;

        module Driver
          port b: target BusSimple;
          comb
            b.data  = 8'hAA;
            b.valid = 1'b1;
          end comb
        end module Driver
    ";
    let h = compile_to_sim_h(source, true);
    assert!(!h.contains("_b_data_vinit"),
            "did not expect vinit for output-side bus signal 'b_data':\n{h}");
    assert!(!h.contains("set_b_data("),
            "did not expect setter for output-side bus signal 'b_data':\n{h}");
}

#[test]
fn test_inputs_start_uninit_without_flag_emits_nothing() {
    let source = "
        bus BusSimple
          data: in UInt<8>;
        end bus BusSimple

        use BusSimple;

        module Reader
          port b:     initiator BusSimple;
          port out_r: out UInt<8>;
          comb
            out_r = b.data;
          end comb
        end module Reader
    ";
    let h = compile_to_sim_h(source, false);
    assert!(!h.contains("_b_data_vinit"),
            "no --inputs-start-uninit → no bus vinit tracking:\n{h}");
}

#[test]
fn test_handshake_tier15_payload_warning_gated_on_valid() {
    // Tier 1.5 (Option D): when a handshake payload is also a --check-uninit-
    // tracked input, its read-site warning should be gated on the channel's
    // valid signal so legitimate "valid low so payload doesn't matter" usage
    // is silent — but producer bug "valid asserted, payload never set" still
    // warns.
    use arch::sim_codegen::SimCodegen;
    let source = "
        bus BusHS
          handshake ch: send kind: valid_ready
            data: UInt<8>;
          end handshake ch
        end bus BusHS

        use BusHS;

        module Consumer
          port b: target BusHS;
          port o: out UInt<8>;
          comb
            if b.ch_valid
              o = b.ch_data;
            else
              o = 8'h0;
            end if
            b.ch_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let sim = SimCodegen::new(&symbols, &ast, overload_map)
        .check_uninit(true)
        .inputs_start_uninit(true);
    let models = sim.generate();
    let cpp = models.iter().find(|m| m.class_name == "VConsumer").unwrap().impl_.clone();

    // Payload-signal read warning must be AND'd with the handshake's valid.
    assert!(cpp.contains("!_b_ch_data_vinit && b_ch_valid"),
            "expected payload warning gated on valid signal:\n{cpp}");

    // Valid-signal itself is tracked but NOT a payload, so its warning is
    // unconditional (no extra gate).
    let valid_check_line = cpp.lines()
        .find(|l| l.contains("!_b_ch_valid_vinit"))
        .expect("expected warning for b_ch_valid signal");
    assert!(!valid_check_line.contains("&& b_ch_valid"),
            "valid signal's own warning should not self-gate:\n{valid_check_line}");
}

#[test]
fn test_handshake_tier15_req_ack_4phase_uses_req_as_guard() {
    use arch::sim_codegen::SimCodegen;
    let source = "
        bus BusRA
          handshake ch: send kind: req_ack_4phase
            payload: UInt<16>;
          end handshake ch
        end bus BusRA

        use BusRA;

        module C
          port b: target BusRA;
          port o: out UInt<16>;
          comb
            if b.ch_req
              o = b.ch_payload;
            else
              o = 16'h0;
            end if
            b.ch_ack = 1'b1;
          end comb
        end module C
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let sim = SimCodegen::new(&symbols, &ast, overload_map)
        .check_uninit(true)
        .inputs_start_uninit(true);
    let models = sim.generate();
    let cpp = models.iter().find(|m| m.class_name == "VC").unwrap().impl_.clone();
    assert!(cpp.contains("!_b_ch_payload_vinit && b_ch_req"),
            "req_ack_4phase payload should gate on b_ch_req:\n{cpp}");
}

fn warnings_from(source: &str) -> Vec<String> {
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check error");
    warnings.into_iter().map(|w| w.message).collect()
}

#[test]
fn test_handshake_tier15_unguarded_payload_warns() {
    let source = "
        bus BusHS
          handshake ch: send kind: valid_ready
            data: UInt<8>;
          end handshake ch
        end bus BusHS

        use BusHS;

        module Consumer
          port b: target BusHS;
          port o: out UInt<8>;
          comb
            o = b.ch_data;
            b.ch_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let ws = warnings_from(source);
    assert!(ws.iter().any(|m| m.contains("b.ch_data") && m.contains("if b.ch_valid")),
            "expected unguarded-payload warning; got: {:?}", ws);
}

#[test]
fn test_handshake_tier15_guarded_payload_silent() {
    let source = "
        bus BusHS
          handshake ch: send kind: valid_ready
            data: UInt<8>;
          end handshake ch
        end bus BusHS

        use BusHS;

        module Consumer
          port b: target BusHS;
          port o: out UInt<8>;
          comb
            if b.ch_valid
              o = b.ch_data;
            else
              o = 8'h0;
            end if
            b.ch_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let ws = warnings_from(source);
    assert!(!ws.iter().any(|m| m.contains("handshake payload") && m.contains("ch_data")),
            "did not expect handshake warning; got: {:?}", ws);
}

#[test]
fn test_handshake_tier15_guard_via_compound_and_silent() {
    let source = "
        bus BusHS
          handshake ch: send kind: valid_ready
            data: UInt<8>;
          end handshake ch
        end bus BusHS

        use BusHS;

        module Consumer
          port b:      target BusHS;
          port enable: in Bool;
          port o:      out UInt<8>;
          comb
            if b.ch_valid and enable
              o = b.ch_data;
            else
              o = 8'h0;
            end if
            b.ch_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let ws = warnings_from(source);
    assert!(!ws.iter().any(|m| m.contains("handshake payload") && m.contains("ch_data")),
            "AND-conjunct guard should silence the lint; got: {:?}", ws);
}

#[test]
fn test_handshake_tier15_else_branch_warns() {
    let source = "
        bus BusHS
          handshake ch: send kind: valid_ready
            data: UInt<8>;
          end handshake ch
        end bus BusHS

        use BusHS;

        module Consumer
          port b: target BusHS;
          port o: out UInt<8>;
          comb
            if b.ch_valid
              o = 8'h0;
            else
              o = b.ch_data;
            end if
            b.ch_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let ws = warnings_from(source);
    assert!(ws.iter().any(|m| m.contains("b.ch_data")),
            "read in else-branch of `if valid` is NOT guarded; should warn. got: {:?}", ws);
}

#[test]
fn test_handshake_tier15_req_ack_uses_req_as_guard() {
    let source = "
        bus BusRA
          handshake ch: send kind: req_ack_4phase
            payload: UInt<16>;
          end handshake ch
        end bus BusRA

        use BusRA;

        module C
          port b: target BusRA;
          port o: out UInt<16>;
          comb
            if b.ch_req
              o = b.ch_payload;
            else
              o = 16'h0;
            end if
            b.ch_ack = 1'b1;
          end comb
        end module C
    ";
    let ws = warnings_from(source);
    assert!(!ws.iter().any(|m| m.contains("handshake payload")),
            "if b.ch_req should guard req_ack payload; got: {:?}", ws);
}

#[test]
fn test_vec_methods_any_all_expand_to_reduction() {
    // vec.any(pred) → OR of per-element substitutions
    // vec.all(pred) → AND of per-element substitutions
    let source = "
        module M
          port vec: in Vec<UInt<8>, 4>;
          port needle: in UInt<8>;
          port any_eq: out Bool;
          port all_nz:  out Bool;
          comb
            any_eq = vec.any(item == needle);
            all_nz = vec.all(item != 0);
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // Fully unrolled, item → vec[i]:
    assert!(sv.contains("vec[0] == needle || vec[1] == needle"),
            "expected any to expand to OR of 4 compares: {sv}");
    assert!(sv.contains("vec[0] != 0 && vec[1] != 0"),
            "expected all to expand to AND of 4 compares: {sv}");
}

#[test]
fn test_vec_methods_index_binder() {
    // `index` is bound to the iteration position (sized literal).
    let source = "
        module M
          port vec: in Vec<UInt<8>, 4>;
          port needle: in UInt<8>;
          port start: in UInt<2>;
          port found: out Bool;
          comb
            found = vec.any(item == needle and index >= start);
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // index should expand to 2-bit literals 2'd0..2'd3 (clog2(4)=2).
    assert!(sv.contains("2'd0") && sv.contains("2'd3"),
            "expected index binder to emit sized literals: {sv}");
}

#[test]
fn test_vec_methods_count_and_contains() {
    let source = "
        module M
          port vec: in Vec<UInt<8>, 4>;
          port x: in UInt<8>;
          port n: out UInt<3>;
          port has: out Bool;
          comb
            n   = vec.count(item == x);
            has = vec.contains(x);
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // count emits 3-bit population-count expression (clog2(N+1)=3 for N=4).
    assert!(sv.contains("3'(vec[0] == x ? 1 : 0)"),
            "expected count to emit width-3 bool-to-bit casts: {sv}");
    // contains lowers identically to any(item == x).
    assert!(sv.contains("(vec[0] == x) || (vec[1] == x)"),
            "expected contains to OR per-element equality: {sv}");
}

#[test]
fn test_vec_methods_reduce_or_and_xor() {
    let source = "
        module M
          port flags: in Vec<Bool, 4>;
          port any_flag: out Bool;
          port all_flag: out Bool;
          port parity:   out Bool;
          comb
            any_flag = flags.reduce_or();
            all_flag = flags.reduce_and();
            parity   = flags.reduce_xor();
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("flags[0] | flags[1] | flags[2] | flags[3]"),
            "reduce_or expected: {sv}");
    assert!(sv.contains("flags[0] & flags[1] & flags[2] & flags[3]"),
            "reduce_and expected: {sv}");
    assert!(sv.contains("flags[0] ^ flags[1] ^ flags[2] ^ flags[3]"),
            "reduce_xor expected: {sv}");
}

#[test]
fn test_let_destructure_basic() {
    let source = "
        struct Point
          x: UInt<8>;
          y: UInt<8>;
        end struct Point

        module M
          port p_in: in Point;
          port ox:   out UInt<8>;
          port oy:   out UInt<8>;
          let {x, y} = p_in;
          comb
            ox = x;
            oy = y;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("assign x = p_in.x;"),
            "expected field assign for x:\n{sv}");
    assert!(sv.contains("assign y = p_in.y;"),
            "expected field assign for y:\n{sv}");
    // Per-field width comes from the struct definition.
    assert!(sv.contains("logic [7:0] x;") && sv.contains("logic [7:0] y;"),
            "expected 8-bit wire declarations:\n{sv}");
}

#[test]
fn test_let_destructure_partial() {
    // Only bind a subset of fields; the rest are ignored.
    let source = "
        struct Trio
          a: UInt<4>;
          b: UInt<4>;
          c: UInt<4>;
        end struct Trio

        module M
          port t:  in Trio;
          port oa: out UInt<4>;
          let {a} = t;
          comb
            oa = a;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("assign a = t.a;"),
            "expected partial destructure:\n{sv}");
    assert!(!sv.contains("assign b = t.b;") && !sv.contains("assign c = t.c;"),
            "did not expect unbound fields:\n{sv}");
}

#[test]
fn test_let_destructure_non_struct_errors() {
    let source = "
        module M
          port x:  in UInt<8>;
          port ox: out UInt<8>;
          let {a, b} = x;
          comb
            ox = 8'h0;
          end comb
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(),
            "expected type-check error for destructure on non-struct");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("requires a struct-typed RHS"),
            "expected specific error message, got: {msg}");
}

#[test]
fn test_let_destructure_unknown_field_errors() {
    let source = "
        struct Pair
          a: UInt<4>;
          b: UInt<4>;
        end struct Pair

        module M
          port p:  in Pair;
          port ox: out UInt<4>;
          let {a, z} = p;
          comb
            ox = a;
          end comb
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(),
            "expected type-check error for unknown field");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("has no field named `z`"),
            "expected unknown-field message, got: {msg}");
}

#[test]
fn test_bus_wire_typechecks_and_codegens() {
    // Buses as wire types: direction info on each field is ignored in wire
    // context (each signal has one driver, determined by the assignment
    // reaching it). The C++ struct is emitted at file scope so the
    // generated `FooBus _let_w;` field is a valid type, and bus-port →
    // bus-wire connections flow through struct field access.
    let source = "
        bus FooBus
          cmd:  out UInt<8>;
          resp: in  UInt<8>;
        end bus FooBus

        module Child
          port p: target FooBus;
          comb
            p.resp = (p.cmd + 8'h1).trunc<8>();
          end comb
        end module Child

        module Parent
          port x_in:  in  UInt<8>;
          port x_out: out UInt<8>;
          wire w: FooBus;
          comb
            w.cmd = x_in;
            x_out = w.resp;
          end comb
          inst c: Child
            p -> w;
          end inst c
        end module Parent
    ";
    let h = compile_to_sim_h(source, false);
    // VStructs.h should emit a plain C++ struct for the bus.
    assert!(h.contains("struct FooBus {"),
            "expected `struct FooBus {{` in generated structs header:\n{h}");
    assert!(h.contains("uint8_t cmd;") && h.contains("uint8_t resp;"),
            "expected bus fields as struct members:\n{h}");

    // VParent.h should declare the wire as a struct-typed member and must
    // NOT emit a shadow `uint32_t w;` scalar.
    let parent = h.split("// ---\n").find(|p| p.contains("class VParent"))
        .expect("no VParent header section");
    assert!(parent.contains("FooBus _let_w;"),
            "expected `FooBus _let_w;` in VParent header:\n{parent}");
    assert!(!parent.contains("uint32_t w;"),
            "unexpected shadow `uint32_t w;` in VParent header:\n{parent}");
}

#[test]
fn test_bus_wire_sv_flattens_to_individual_signals() {
    // SV codegen has no bus interface/struct: a bus-typed wire becomes N
    // individual SV wires named `<wire>_<field>`. Field access on the
    // wire rewrites to the flat name, same as for bus ports.
    use arch::codegen::Codegen;
    let source = "
        bus FooBus
          cmd:  out UInt<8>;
          resp: in  UInt<8>;
        end bus FooBus

        module Child
          port p: target FooBus;
          comb
            p.resp = (p.cmd + 8'h1).trunc<8>();
          end comb
        end module Child

        module Parent
          port x_in:  in  UInt<8>;
          port x_out: out UInt<8>;
          wire w: FooBus;
          comb
            w.cmd = x_in;
            x_out = w.resp;
          end comb
          inst c: Child
            p -> w;
          end inst c
        end module Parent
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (_w, overload_map) = checker.check().expect("type check");
    let cg = Codegen::new(&symbols, &ast, overload_map);
    let sv = cg.generate();

    // Bus wire must decompose into flat signals.
    assert!(sv.contains("logic [7:0] w_cmd;") && sv.contains("logic [7:0] w_resp;"),
            "expected flat `w_cmd` / `w_resp` wires:\n{sv}");
    // No `FooBus w;` placeholder left behind.
    assert!(!sv.contains("FooBus w"),
            "unexpected `FooBus w` decl (should be flattened):\n{sv}");
    // Field access on bus wire rewrites to flat name.
    assert!(sv.contains("assign w_cmd = x_in") || sv.contains("w_cmd = x_in"),
            "expected `w_cmd = x_in` assignment:\n{sv}");
    assert!(sv.contains("x_out = w_resp"),
            "expected `x_out = w_resp` assignment:\n{sv}");
    // Inst binding connects to the flat wires.
    assert!(sv.contains(".p_cmd(w_cmd)") && sv.contains(".p_resp(w_resp)"),
            "expected inst binding to flat wires:\n{sv}");
}

#[test]
fn test_bus_declaration_inside_package_parses() {
    // `bus` is now accepted alongside `struct`, `enum`, etc. inside a
    // `package` block. Previously the parser rejected this with
    // "unexpected token: expected param, domain, enum, struct, or function".
    let source = "
        package MyPkg
          struct Header
            tag: UInt<4>;
          end struct Header

          bus MyBus
            cmd_valid: out Bool;
            cmd_data:  out UInt<8>;
            rsp_data:  in  UInt<8>;
          end bus MyBus
        end package MyPkg
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let sf = parser.parse_source_file().expect("parse");
    let pkg = sf.items.iter().find_map(|i|
        if let arch::ast::Item::Package(p) = i { Some(p) } else { None }
    ).expect("parsed package");
    assert_eq!(pkg.structs.len(), 1, "struct should still parse alongside bus");
    assert_eq!(pkg.buses.len(), 1, "bus should be collected into package.buses");
    assert_eq!(pkg.buses[0].name.name, "MyBus");
}

#[test]
fn test_package_nested_bus_used_as_wire_and_port() {
    // End-to-end: a bus declared inside a package is (a) registered in the
    // global symbol table, (b) usable as a wire type in a consumer module,
    // (c) flattened at port sites on a target submodule, and (d) emitted as
    // a C++ struct by sim codegen + flat SV wires by SV codegen.
    let source = "
        package MyPkg
          bus MyBus
            cmd_valid: out Bool;
            cmd_data:  out UInt<8>;
            rsp_data:  in  UInt<8>;
          end bus MyBus
        end package MyPkg

        use MyPkg;

        module Top
          port x_in:  in  UInt<8>;
          port x_out: out UInt<8>;
          wire b: MyBus;
          comb
            b.cmd_valid = true;
            b.cmd_data = x_in;
            x_out = b.rsp_data;
          end comb
          inst e: Echo
            p -> b;
          end inst e
        end module Top

        module Echo
          port p: target MyBus;
          comb
            p.rsp_data = p.cmd_valid ? (p.cmd_data + 8'h1).trunc<8>() : 8'h0;
          end comb
        end module Echo
    ";
    // Sim codegen: struct MyBus lands in VStructs header.
    let sim_h = compile_to_sim_h(source, false);
    assert!(sim_h.contains("struct MyBus {"),
            "expected `struct MyBus` in sim structs header:\n{sim_h}");
    assert!(sim_h.contains("uint8_t cmd_valid;") && sim_h.contains("uint8_t cmd_data;"),
            "expected bus fields as struct members:\n{sim_h}");

    // SV codegen: package emits no bus type; wires flatten to per-signal.
    use arch::codegen::Codegen;
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (_w, overload_map) = checker.check().expect("type check");
    let cg = Codegen::new(&symbols, &ast, overload_map);
    let sv = cg.generate();
    assert!(sv.contains("logic b_cmd_valid;"),
            "bus wire should flatten to b_cmd_valid:\n{sv}");
    assert!(sv.contains("logic [7:0] b_cmd_data;"),
            "bus wire should flatten to b_cmd_data:\n{sv}");
    assert!(sv.contains(".p_cmd_data(b_cmd_data)"),
            "inst binding should use flat wire names:\n{sv}");
}

#[test]
fn test_bus_port_connected_via_per_field_bindings_no_warning() {
    // Per-field bus port bindings at inst time — `p.cmd_valid <- ...;
    // p.cmd_addr <- ...` — should count as the bus port being connected.
    // Prior to this fix, the connectivity check only recognized whole-bus
    // bindings and emitted a false-positive "output port `p` not connected"
    // warning even when every field was wired explicitly.
    let source = "
        bus TestBus
          cmd:  out UInt<8>;
          resp: in  UInt<8>;
        end bus TestBus

        module Child
          port p: target TestBus;
          comb
            p.resp = (p.cmd + 8'h1).trunc<8>();
          end comb
        end module Child

        module Parent
          port x_in:  in  UInt<8>;
          port x_out: out UInt<8>;
          inst c: Child
            p.cmd  <- x_in;
            p.resp -> x_out;
          end inst c
        end module Parent
    ";
    let ws = warnings_from(source);
    assert!(!ws.iter().any(|m| m.contains("output port `p`")
                               && m.contains("not connected")),
            "per-field bus binding should not trigger 'not connected': {:?}", ws);
}

#[test]
fn test_bus_port_unconnected_still_warns() {
    // Confirm the gap-6 fix doesn't regress the real case: an inst that
    // never mentions its bus port at all should still warn.
    let source = "
        bus TestBus
          cmd:  out UInt<8>;
          resp: in  UInt<8>;
        end bus TestBus

        module Child
          port p: target TestBus;
          comb
            p.resp = p.cmd;
          end comb
        end module Child

        module Parent
          port x: in UInt<8>;
          inst c: Child
          end inst c
        end module Parent
    ";
    let ws = warnings_from(source);
    assert!(ws.iter().any(|m| m.contains("output port `p`")
                              && m.contains("not connected")),
            "completely-unbound bus port should still warn: {:?}", ws);
}

#[test]
fn test_handshake_payload_guard_via_short_circuit_and() {
    // `p.cmd_valid and (p.cmd_op != 0)` — the right-hand side of a
    // short-circuit `and` is only evaluated when the left-hand side is
    // true, so the checker must treat `p.cmd_valid` as an enclosing
    // guard when descending into the right side.
    let source = "
        bus BusHS
          handshake cmd: send kind: valid_ready
            op: UInt<2>;
          end handshake cmd
        end bus BusHS

        module Consumer
          port b: target BusHS;
          port o: out Bool;
          comb
            o = b.cmd_valid and (b.cmd_op != 2'b00);
            b.cmd_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let ws = warnings_from(source);
    assert!(!ws.iter().any(|m| m.contains("cmd_op") && m.contains("unguarded")
                               || (m.contains("cmd_op") && m.contains("outside"))),
            "short-circuit `and` should guard cmd_op read; got: {:?}", ws);
}

#[test]
fn test_handshake_payload_guard_via_ternary_condition() {
    // Ternary: `cond ? then : else` — the then-branch only evaluates
    // when cond is true, so the checker must treat cond as an enclosing
    // guard when descending into the then-branch. Here the payload read
    // `b.cmd_op` sits inside the then-branch with a cmd_valid guard.
    let source = "
        bus BusHS
          handshake cmd: send kind: valid_ready
            op: UInt<2>;
          end handshake cmd
        end bus BusHS

        module Consumer
          port b: target BusHS;
          port o: out UInt<2>;
          comb
            o = b.cmd_valid ? b.cmd_op : 2'b00;
            b.cmd_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let ws = warnings_from(source);
    assert!(!ws.iter().any(|m| m.contains("cmd_op") && m.contains("outside")),
            "ternary condition should guard cmd_op read; got: {:?}", ws);
}

#[test]
fn test_handshake_payload_truly_unguarded_still_warns() {
    // Confirm the gap-7 fix doesn't weaken the real case: reading a
    // payload with no valid guard anywhere must still warn.
    let source = "
        bus BusHS
          handshake cmd: send kind: valid_ready
            op: UInt<2>;
          end handshake cmd
        end bus BusHS

        module Consumer
          port b: target BusHS;
          port o: out UInt<2>;
          comb
            o = b.cmd_op;
            b.cmd_ready = 1'b1;
          end comb
        end module Consumer
    ";
    let ws = warnings_from(source);
    assert!(ws.iter().any(|m| m.contains("cmd_op") && m.contains("outside")),
            "genuinely unguarded cmd_op read should still warn: {:?}", ws);
}

fn compile_to_pybind_cpps(source: &str) -> Vec<(String, String)> {
    use arch::sim_codegen::SimCodegen;
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check");
    let sim = SimCodegen::new(&symbols, &ast, overload_map);
    sim.generate_pybind().into_iter().map(|m| (m.class_name, m.impl_)).collect()
}

#[test]
fn test_pybind_struct_bindings_are_scoped_to_module() {
    // When a shared package declares structs used by only one of several
    // sibling modules, each module's pybind wrapper must bind ONLY the
    // structs that module actually references. Binding unused structs
    // fails to compile because the module's own `V{Name}.h` doesn't
    // declare them.
    let source = "
        package SharedPkg
          struct Reg1
            a: UInt<4>;
            b: UInt<4>;
          end struct Reg1
          struct Reg2
            x: UInt<8>;
          end struct Reg2
          struct PipeBus
            data: UInt<8>;
          end struct PipeBus
        end package SharedPkg

        use SharedPkg;

        // Consumes Reg1 + Reg2 as internal regs.
        module UsesStructs
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port out: out UInt<12>;
          reg r1: Reg1 reset rst => Reg1 { a: 4'h0, b: 4'h0 };
          reg r2: Reg2 reset rst => Reg2 { x: 8'h0 };
          default seq on clk rising;
          seq
            r1.a <= 4'h1;
            r2.x <= 8'h2;
          end seq
          comb
            out = {r1.a, r1.b, r2.x[3:0]};
          end comb
        end module UsesStructs

        // No struct-typed ports, no struct-typed regs — must NOT bind Reg1/Reg2.
        module PrimitivesOnly
          port a: in UInt<8>;
          port b: out UInt<8>;
          comb
            b = a;
          end comb
        end module PrimitivesOnly

        // Uses only PipeBus. Must bind PipeBus, but not Reg1/Reg2.
        module UsesOneStruct
          port bus_in:  in  PipeBus;
          port bus_out: out PipeBus;
          comb
            bus_out = bus_in;
          end comb
        end module UsesOneStruct
    ";

    let pybinds = compile_to_pybind_cpps(source);
    let prim = pybinds.iter().find(|(n, _)| n.contains("PrimitivesOnly"))
        .expect("PrimitivesOnly pybind wrapper").1.clone();
    let one = pybinds.iter().find(|(n, _)| n.contains("UsesOneStruct"))
        .expect("UsesOneStruct pybind wrapper").1.clone();
    let uses = pybinds.iter().find(|(n, _)| n.contains("UsesStructs"))
        .expect("UsesStructs pybind wrapper").1.clone();

    // PrimitivesOnly: no struct bindings at all.
    assert!(!prim.contains("py::class_<Reg1>"),
            "PrimitivesOnly must not bind Reg1:\n{prim}");
    assert!(!prim.contains("py::class_<Reg2>"),
            "PrimitivesOnly must not bind Reg2:\n{prim}");
    assert!(!prim.contains("py::class_<PipeBus>"),
            "PrimitivesOnly must not bind PipeBus:\n{prim}");

    // UsesOneStruct: binds PipeBus, nothing else.
    assert!(one.contains("py::class_<PipeBus>"),
            "UsesOneStruct must bind PipeBus:\n{one}");
    assert!(!one.contains("py::class_<Reg1>"),
            "UsesOneStruct must not bind Reg1:\n{one}");
    assert!(!one.contains("py::class_<Reg2>"),
            "UsesOneStruct must not bind Reg2:\n{one}");

    // UsesStructs: binds Reg1 and Reg2 (internal reg types).
    assert!(uses.contains("py::class_<Reg1>"),
            "UsesStructs must bind Reg1:\n{uses}");
    assert!(uses.contains("py::class_<Reg2>"),
            "UsesStructs must bind Reg2:\n{uses}");
}

#[test]
fn test_pybind_struct_bindings_transitive_closure() {
    // If a port-level struct has a field whose type is another struct,
    // the nested struct must also be bound.
    let source = "
        struct Inner
          v: UInt<8>;
        end struct Inner

        struct Outer
          a: UInt<4>;
          inner: Inner;
        end struct Outer

        module M
          port o: in Outer;
          port x: out UInt<8>;
          comb
            x = o.inner.v;
          end comb
        end module M
    ";
    let pybinds = compile_to_pybind_cpps(source);
    let m = pybinds.iter().find(|(n, _)| n.contains("VM_pybind"))
        .expect("M pybind wrapper").1.clone();
    assert!(m.contains("py::class_<Outer>"), "must bind Outer:\n{m}");
    assert!(m.contains("py::class_<Inner>"),
            "must bind Inner (transitive via Outer.inner):\n{m}");
}

#[test]
fn test_trace_skips_struct_typed_let_and_wire() {
    // Struct-typed `let` and `wire` decls must NOT appear in the VCD trace
    // emission — they can't be bit-shifted scalar-style, and previously
    // produced invalid C++ (`(hwif_w >> i) & 1` against a struct type) that
    // failed to compile. Ports and regs with struct types were already
    // filtered; this extends the same filter to let/wire.
    let source = "
        struct Payload
          a: UInt<8>;
          b: UInt<8>;
        end struct Payload

        module M
          port clk:   in Clock<SysDomain>;
          port rst:   in Reset<Sync>;
          port p_in:  in  Payload;
          port out_a: out UInt<8>;
          wire scratch: Payload;
          let  view:    Payload = p_in;
          comb
            scratch.a = p_in.a;
            scratch.b = p_in.b;
            out_a = view.a | scratch.b;
          end comb
        end module M
    ";
    let h = compile_to_sim_h(source, false);
    assert!(!h.contains("(_let_scratch >> _i)"),
            "scratch (struct wire) leaked into trace:\n{h}");
    assert!(!h.contains("(_let_view >> _i)"),
            "view (struct let) leaked into trace:\n{h}");
}

#[test]
fn test_find_first_destructure_basic() {
    let source = "
        module M
          port vec: in Vec<UInt<8>, 4>;
          port needle: in UInt<8>;
          port ok:  out Bool;
          port pos: out UInt<2>;
          let {found, index} = vec.find_first(item == needle);
          comb
            ok  = found;
            pos = index;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // Typedef emitted for the synthesized result struct.
    assert!(sv.contains("typedef struct packed { logic found; logic [1:0] index; } __ArchFindResult_2;"),
            "expected ArchFindResult typedef: {sv}");
    // Raw OR reduction for `found`, no spurious struct literal:
    assert!(sv.contains("assign found = vec[0] == needle || vec[1] == needle"),
            "expected OR reduction: {sv}");
    // Priority encoder for `index`, nested ternary:
    assert!(sv.contains("assign index = (vec[0] == needle) ? 2'd0 : (vec[1] == needle) ? 2'd1"),
            "expected priority encoder: {sv}");
    // Correct width on `index`:
    assert!(sv.contains("logic [1:0] index;"),
            "expected 2-bit index wire: {sv}");
}

#[test]
fn test_find_first_partial_destructure() {
    // Bind only `found` (don't care about where).
    let source = "
        module M
          port vec: in Vec<UInt<8>, 4>;
          port needle: in UInt<8>;
          port ok:  out Bool;
          let {found} = vec.find_first(item == needle);
          comb
            ok = found;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("logic found;") && sv.contains("assign found ="),
            "expected `found` wire + assign: {sv}");
    // No `index` wire should be emitted since the user didn't bind it.
    // (It can still appear as a module port in the future; for now, assert
    // no `logic index` declaration at module scope.)
    assert!(!sv.lines().any(|l| l.trim_start().starts_with("logic ") && l.contains(" index;")),
            "did not expect unbound `index`: {sv}");
}

#[test]
fn test_find_first_unknown_binding_errors() {
    // Binding a name that isn't `found` or `index` must error.
    let source = "
        module M
          port vec: in Vec<UInt<8>, 4>;
          port needle: in UInt<8>;
          port ok: out Bool;
          let {found, wrong_name} = vec.find_first(item == needle);
          comb
            ok = found;
          end comb
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(),
            "expected type-check error for bad destructure binding");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("find_first result has no field named `wrong_name`"),
            "expected specific error message, got: {msg}");
}

#[test]
fn test_pipe_reg_port_n1_equivalent_to_port_reg() {
    // port q: out pipe_reg<T, 1> ... emits the same SV as port reg q: out T
    let src_new = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<8>;
          port q: out pipe_reg<UInt<8>, 1> reset rst => 0;
          default seq on clk rising;
          seq
            q@1 <= a;
          end seq
        end module M
    ";
    let src_old = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<8>;
          port reg q: out UInt<8> reset rst => 0;
          default seq on clk rising;
          seq
            q <= a;
          end seq
        end module M
    ";
    assert_eq!(compile_to_sv(src_new), compile_to_sv(src_old),
        "pipe_reg<T, 1> + @1 should be byte-identical to port reg");
}

#[test]
fn test_pipe_reg_port_n3_emits_cascade() {
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<8>;
          port q: out pipe_reg<UInt<8>, 3> reset rst => 0;
          default seq on clk rising;
          seq
            q@3 <= a;
          end seq
        end module M
    ";
    let sv = compile_to_sv(source);
    // Two intermediate stage regs should be declared + cascade assigns.
    assert!(sv.contains("q_stg1") && sv.contains("q_stg2"),
        "expected 2 intermediate stages for depth=3:\n{sv}");
    assert!(sv.contains("q_stg1 <= a;"), "stage 0 write missing:\n{sv}");
    assert!(sv.contains("q_stg2 <= q_stg1;"), "stage 1 shift missing:\n{sv}");
    assert!(sv.contains("q <= q_stg2;"), "final output write missing:\n{sv}");
    // Uniform reset across all stages:
    assert!(sv.contains("q_stg1 <= 0") && sv.contains("q_stg2 <= 0") && sv.contains("q <= 0"),
        "expected uniform reset across all stages:\n{sv}");
}

#[test]
fn test_pipe_reg_port_depth_mismatch_errors() {
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<8>;
          port q: out pipe_reg<UInt<8>, 3> reset rst => 0;
          default seq on clk rising;
          seq
            q@5 <= a;
          end seq
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate base");
    let ast = arch::elaborate::lower_threads(ast).expect("lower threads");
    let result = arch::elaborate::lower_pipe_reg_ports(ast);
    assert!(result.is_err(), "expected depth-mismatch error");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("exceeds declared latency 3"),
        "expected specific error message, got: {msg}");
}

#[test]
fn test_pipe_reg_port_bare_assign_ambiguous_errors() {
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<8>;
          port q: out pipe_reg<UInt<8>, 3> reset rst => 0;
          default seq on clk rising;
          seq
            q <= a;
          end seq
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate base");
    let ast = arch::elaborate::lower_threads(ast).expect("lower threads");
    let result = arch::elaborate::lower_pipe_reg_ports(ast);
    assert!(result.is_err(), "expected ambiguous-assignment error");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("is ambiguous"),
        "expected specific error message, got: {msg}");
}

#[test]
fn test_pipe_reg_depth_zero_errors() {
    let source = "
        module M
          port q: out pipe_reg<UInt<8>, 0>;
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let result = parser.parse_source_file();
    assert!(result.is_err(), "expected depth=0 error");
}

#[test]
fn test_stdlib_bus_axi_stream_discovery() {
    // A module that `use BusAxiStream;` should pick the stdlib definition
    // up automatically — no ARCH_LIB_PATH setup required. Verified here
    // by running the real compiler binary against a stub test case.
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("Prod.arch");
    std::fs::write(&src, "\
        use BusAxiStream;\n\
        module Prod\n\
          port clk: in Clock<SysDomain>;\n\
          port rst: in Reset<Sync>;\n\
          port m_axis: initiator BusAxiStream<DATA_W=32>;\n\
          comb\n\
            m_axis.t_valid = 1'b0;\n\
            m_axis.t_data  = 32'h0;\n\
            m_axis.t_last  = 1'b0;\n\
            m_axis.t_keep  = 4'h0;\n\
          end comb\n\
        end module Prod\n\
    ").unwrap();
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("check")
        .arg(&src)
        .output()
        .expect("run arch check");
    assert!(out.status.success(),
        "arch check should succeed with stdlib discovery; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr));
}

#[test]
fn test_stdlib_disabled_via_env() {
    // ARCH_NO_STDLIB=1 should skip the stdlib search entirely — a module
    // that depends on stdlib should then fail to resolve.
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("Prod.arch");
    std::fs::write(&src, "\
        use BusAxiStream;\n\
        module Prod\n\
          port m_axis: initiator BusAxiStream<DATA_W=32>;\n\
          comb m_axis.t_valid = 1'b0; m_axis.t_data = 32'h0; m_axis.t_last = 1'b0; m_axis.t_keep = 4'h0; end comb\n\
        end module Prod\n\
    ").unwrap();
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("check")
        .arg(&src)
        .env("ARCH_NO_STDLIB", "1")
        .output()
        .expect("run arch check");
    assert!(!out.status.success(),
        "expected failure when ARCH_NO_STDLIB=1 disables stdlib resolution");
}

#[test]
fn test_handshake_generate_if_payload_toggle() {
    // `generate_if` inside a handshake payload should conditionally
    // include fields based on port-site param overrides.
    let source = "
        bus BusFlex
          param DATA_W: const = 8;
          param USE_LAST: const = 1;
          param ID_W: const = 0;

          handshake t: send kind: valid_ready
            data: UInt<DATA_W>;
            generate_if USE_LAST
              last: Bool;
            end generate_if
            generate_if ID_W > 0
              id: UInt<ID_W>;
            end generate_if
          end handshake t
        end bus BusFlex

        module Full
          port p: initiator BusFlex<DATA_W=16, USE_LAST=1, ID_W=4>;
          comb
            p.t_valid = 1'b0;
            p.t_data  = 16'h0;
            p.t_last  = 1'b0;
            p.t_id    = 4'h0;
          end comb
        end module Full

        module Bare
          port p: initiator BusFlex<DATA_W=8, USE_LAST=0>;
          comb
            p.t_valid = 1'b0;
            p.t_data  = 8'h0;
          end comb
        end module Bare
    ";
    let sv = compile_to_sv(source);
    // Full config emits all four optional ports.
    assert!(sv.contains("output logic [15:0] p_t_data"), "Full: data 16-bit expected:\n{sv}");
    assert!(sv.contains("output logic p_t_last"), "Full: t_last present:\n{sv}");
    assert!(sv.contains("output logic [3:0] p_t_id"), "Full: t_id [3:0]:\n{sv}");
    // Bare config omits t_last (USE_LAST=0) AND t_id (ID_W=0).
    // Both Full and Bare emit into the same SV string; check Bare's
    // module block specifically for the absence of those fields.
    let bare = sv.split("module Bare").nth(1).expect("Bare module present");
    let bare_until_end = bare.split("endmodule").next().unwrap_or("");
    assert!(!bare_until_end.contains("t_last"),
        "Bare config should omit t_last: {bare_until_end}");
    assert!(!bare_until_end.contains("t_id"),
        "Bare config should omit t_id: {bare_until_end}");
}

#[test]
fn test_handshake_generate_if_nested_in_bus_genif_errors() {
    // A handshake with payload generate_if placed INSIDE a bus-level
    // generate_if is not supported in v1 — must error with a clear message.
    let source = "
        bus BusBad
          param ENABLE: const = 1;
          generate_if ENABLE
            handshake t: send kind: valid_ready
              data: UInt<8>;
              generate_if ENABLE
                extra: Bool;
              end generate_if
            end handshake t
          end generate_if
        end bus BusBad
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let result = parser.parse_source_file();
    assert!(result.is_err(), "expected parser error");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("not supported when the handshake itself is nested"),
        "expected specific nesting-error message, got: {msg}");
}

#[test]
fn test_stdlib_bus_apb_discovery_apb3_minimal() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("Csr.arch");
    std::fs::write(&src, "\
        use BusApb;\n\
        module Csr\n\
          port clk: in Clock<SysDomain>;\n\
          port rst: in Reset<Sync>;\n\
          port s_apb: target BusApb<ADDR_W=12, DATA_W=32>;\n\
          comb s_apb.pready = 1'b1; s_apb.prdata = 32'h0; end comb\n\
        end module Csr\n\
    ").unwrap();
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("check").arg(&src).output().expect("run arch check");
    assert!(out.status.success(),
        "APB3 minimal should compile; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr));
}

#[test]
fn test_port_reg_deprecation_warning_fires() {
    // Legacy `port reg` should produce a deprecation warning pointing
    // users at `port q: out pipe_reg<T, 1>`.
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in_data: in UInt<8>;
          port reg q: out UInt<8> reset rst => 0;
          seq on clk rising
            q <= in_data;
          end seq
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_threads(ast).expect("lower threads");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("lower pipe_reg");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast).check().expect("typecheck");
    assert!(warnings.iter().any(|w| w.message.contains("`port reg q")
            && w.message.contains("deprecated")
            && w.message.contains("pipe_reg<T, 1>")),
        "expected deprecation warning, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>());
}

#[test]
fn test_pipe_reg_port_no_deprecation_warning() {
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in_data: in UInt<8>;
          port q: out pipe_reg<UInt<8>, 1> reset rst => 0;
          seq on clk rising
            q@1 <= in_data;
          end seq
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_threads(ast).expect("lower threads");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("lower pipe_reg");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast).check().expect("typecheck");
    assert!(!warnings.iter().any(|w| w.message.contains("deprecated")),
        "did not expect deprecation warning for pipe_reg<T,1>, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>());
}

#[test]
fn test_handshake_channel_parses_new_keyword() {
    let source = "
        bus BusNew
          handshake_channel cmd: send kind: valid_ready
            addr: UInt<32>;
          end handshake_channel cmd
        end bus BusNew

        use BusNew;

        module M
          port p: initiator BusNew;
          comb
            p.cmd_valid = 1'b0;
            p.cmd_addr  = 32'h0;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("output logic p_cmd_valid"),
        "handshake_channel should expand to the same ports as legacy `handshake`:\n{sv}");
    assert!(sv.contains("input logic p_cmd_ready"),
        "handshake_channel should emit the ready signal:\n{sv}");
    assert!(sv.contains("output logic [31:0] p_cmd_addr"),
        "handshake_channel should emit the payload:\n{sv}");
}

#[test]
fn test_handshake_legacy_keyword_emits_deprecation_warning() {
    let source = "
        bus BusLegacy
          handshake cmd: send kind: valid_ready
            addr: UInt<32>;
          end handshake cmd
        end bus BusLegacy

        use BusLegacy;

        module M
          port p: initiator BusLegacy;
          comb
            p.cmd_valid = 1'b0;
            p.cmd_addr  = 32'h0;
          end comb
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_threads(ast).expect("lower threads");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("lower pipe_reg");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast).check().expect("typecheck");
    assert!(warnings.iter().any(|w|
        w.message.contains("`handshake cmd")
        && w.message.contains("deprecated")
        && w.message.contains("handshake_channel")),
        "expected deprecation warning, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>());
}

#[test]
fn test_handshake_channel_no_deprecation_warning() {
    let source = "
        bus BusNew
          handshake_channel cmd: send kind: valid_ready
            addr: UInt<32>;
          end handshake_channel cmd
        end bus BusNew

        use BusNew;

        module M
          port p: initiator BusNew;
          comb
            p.cmd_valid = 1'b0;
            p.cmd_addr  = 32'h0;
          end comb
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_threads(ast).expect("lower threads");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("lower pipe_reg");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast).check().expect("typecheck");
    assert!(!warnings.iter().any(|w| w.message.contains("handshake_channel")
                                    && w.message.contains("deprecated")),
        "did not expect deprecation warning for handshake_channel form, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>());
}

#[test]
fn test_credit_channel_parses_as_bus_sub_construct() {
    // PR #3 scaffolding: credit_channel inside a bus parses into
    // BusDecl::credit_channels. Typecheck rejects it (not yet implemented),
    // but parser + resolve should succeed.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<64>;
            param DEPTH: const = 8;
          end credit_channel data
        end bus DmaCh
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let bus = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Bus(b) if b.name.name == "DmaCh" => Some(b),
        _ => None,
    }).expect("DmaCh bus should be in AST");
    assert_eq!(bus.credit_channels.len(), 1);
    let cc = &bus.credit_channels[0];
    assert_eq!(cc.name.name, "data");
    assert_eq!(cc.role_dir, arch::ast::Direction::Out);
    assert_eq!(cc.params.len(), 2);
    assert_eq!(cc.params[0].name.name, "T");
    assert_eq!(cc.params[1].name.name, "DEPTH");
}

#[test]
fn test_credit_channel_wires_flatten_at_bus_port() {
    // PR #3b-i: a bus with a credit_channel sub-construct flattens to three
    // wires (send_valid, send_data, credit_return) at the port use site.
    // Method dispatch (ch.send/ch.pop/ch.can_send) is still unimplemented;
    // users who drive the flattened wires directly compile cleanly.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<16>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port p: initiator DmaCh;
          comb
            p.data_send_valid = 1'b0;
            p.data_send_data  = 16'h0;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("output logic p_data_send_valid"),
        "credit_channel should emit send_valid as an initiator output:\n{sv}");
    assert!(sv.contains("output logic [15:0] p_data_send_data"),
        "credit_channel should emit send_data with the payload type:\n{sv}");
    assert!(sv.contains("input logic p_data_credit_return"),
        "credit_channel should emit credit_return as an initiator input:\n{sv}");
}

#[test]
fn test_credit_channel_wires_flip_on_target_perspective() {
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<16>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Cons
          port p: target DmaCh;
          comb
            p.data_credit_return = 1'b0;
          end comb
        end module Cons
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("input logic p_data_send_valid"),
        "on target perspective, send_valid should be an input:\n{sv}");
    assert!(sv.contains("input logic [15:0] p_data_send_data"),
        "on target perspective, send_data should be an input:\n{sv}");
    assert!(sv.contains("output logic p_data_credit_return"),
        "on target perspective, credit_return should be an output:\n{sv}");
}

#[test]
fn test_credit_channel_emits_sender_counter_state() {
    // PR #3b-ii: on the initiator side of a `send`-role credit_channel,
    // the SV output includes the credit register, can_send wire, and
    // counter-update always_ff block. Target-side fifo + method dispatch
    // are still TBD (PR #3b-iii / #3b-iv).
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          comb
            p.data_send_valid = 1'b0;
            p.data_send_data  = 8'h0;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("__p_data_credit"),
        "credit register should be declared:\n{sv}");
    assert!(sv.contains("__p_data_can_send"),
        "can_send wire should be declared:\n{sv}");
    assert!(sv.contains("__p_data_can_send = __p_data_credit != 0"),
        "can_send wire should read the credit reg:\n{sv}");
    assert!(sv.contains("p_data_send_valid && !p_data_credit_return"),
        "counter-update should decrement on pure send:\n{sv}");
    assert!(sv.contains("p_data_credit_return && !p_data_send_valid"),
        "counter-update should increment on pure credit_return:\n{sv}");
    assert!(sv.contains("always_ff"),
        "counter should update in an always_ff block:\n{sv}");
}

#[test]
fn test_credit_channel_emits_target_fifo() {
    // PR #3b-iii: target-side module gets a synthesized FIFO with push on
    // send_valid and pop on user-driven credit_return.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Cons
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   target DmaCh;
          comb
            p.data_credit_return = 1'b0;
          end comb
        end module Cons
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("__p_data_buf"),
        "target FIFO buffer array should be declared:\n{sv}");
    assert!(sv.contains("__p_data_head"),
        "FIFO head pointer should be declared:\n{sv}");
    assert!(sv.contains("__p_data_tail"),
        "FIFO tail pointer should be declared:\n{sv}");
    assert!(sv.contains("__p_data_occ"),
        "FIFO occupancy should be declared:\n{sv}");
    assert!(sv.contains("__p_data_valid = __p_data_occ != 0"),
        "valid wire should report non-empty:\n{sv}");
    assert!(sv.contains("__p_data_data = __p_data_buf[__p_data_head]"),
        "data wire should read the head slot:\n{sv}");
    assert!(sv.contains("if (p_data_send_valid)"),
        "push path should be gated on send_valid:\n{sv}");
    assert!(sv.contains("p_data_credit_return && __p_data_valid"),
        "pop should fire on user-driven credit_return when FIFO non-empty:\n{sv}");
}

#[test]
fn test_credit_channel_no_target_fifo_on_send_role() {
    // Sender-side module on a `send` channel is the producer — it gets the
    // credit counter (PR #3b-ii), NOT the target FIFO. Guard against cross-
    // contamination.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          comb
            p.data_send_valid = 1'b0;
            p.data_send_data  = 8'h0;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    assert!(!sv.contains("__p_data_buf"),
        "sender-role module must not emit target FIFO buffer:\n{sv}");
}

#[test]
fn test_credit_channel_no_counter_on_receive_role() {
    // A `send`-role channel where this module is the target should NOT
    // emit a sender counter — this module is the receiver. (The target
    // fifo lands in PR #3b-iii; for now no helper state at all on the
    // target side.)
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Cons
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   target DmaCh;
          comb
            p.data_credit_return = 1'b0;
          end comb
        end module Cons
    ";
    let sv = compile_to_sv(source);
    assert!(!sv.contains("__p_data_credit"),
        "target-role module should not emit sender counter:\n{sv}");
}

#[test]
fn test_credit_channel_can_send_method_dispatch() {
    // PR #3b-v-β: sender reads port.ch.can_send; dispatch rewrites it to
    // the synthesized SV wire __<port>_<ch>_can_send.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          port have_data: in Bool;
          port payload:   in UInt<8>;
          comb
            p.data_send_valid = p.data.can_send and have_data;
            p.data_send_data  = payload;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("__p_data_can_send"),
        "dispatch should rewrite p.data.can_send → __p_data_can_send:\n{sv}");
    assert!(sv.contains("p_data_send_valid = __p_data_can_send"),
        "valid assignment should reference the rewritten name:\n{sv}");
}

#[test]
fn test_credit_channel_valid_and_data_method_dispatch_on_receiver() {
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Cons
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   target DmaCh;
          port want_pop: in Bool;
          port latest:   out UInt<8>;
          comb
            latest = p.data.data;
            p.data_credit_return = p.data.valid and want_pop;
          end comb
        end module Cons
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("latest = __p_data_data"),
        "receiver read of p.data.data should rewrite to __p_data_data:\n{sv}");
    assert!(sv.contains("__p_data_valid"),
        "p.data.valid should rewrite to __p_data_valid:\n{sv}");
}

#[test]
fn test_credit_channel_can_send_registered_emits_flop() {
    // PR #3b-iv: CAN_SEND_REGISTERED=1 flops can_send off the next-state
    // counter (option b — full throughput preserved; fan-out comes off a
    // register).
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:                   type  = UInt<8>;
            param DEPTH:               const = 4;
            param CAN_SEND_REGISTERED: const = 1;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          comb
            p.data_send_valid = 1'b0;
            p.data_send_data  = 8'h0;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    // Registered form declares can_send as `logic` (register), not `wire`.
    assert!(sv.contains("logic __p_data_can_send;"),
        "CAN_SEND_REGISTERED=1 should declare can_send as a register:\n{sv}");
    // And assigns it inside the always_ff block.
    assert!(sv.contains("__p_data_can_send <="),
        "registered can_send should be updated via non-blocking assign:\n{sv}");
    // No `wire` form for can_send.
    assert!(!sv.contains("wire  __p_data_can_send"),
        "CAN_SEND_REGISTERED=1 must not emit the combinational wire form:\n{sv}");
}

#[test]
fn test_credit_channel_can_send_default_is_combinational() {
    // Default (CAN_SEND_REGISTERED omitted / 0) keeps the existing
    // combinational wire — unchanged from PR #3b-ii.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          comb
            p.data_send_valid = 1'b0;
            p.data_send_data  = 8'h0;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("wire  __p_data_can_send = __p_data_credit != 0"),
        "default (unregistered) can_send should stay combinational:\n{sv}");
}

#[test]
fn test_credit_channel_tier2_sender_assertions() {
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          comb
            p.data_send_valid = 1'b0;
            p.data_send_data  = 8'h0;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_auto_cc_p_data_credit_bounds"),
        "credit_bounds assertion label should be present on sender:\n{sv}");
    assert!(sv.contains("__p_data_credit <= (4)"),
        "credit_bounds property should compare credit reg to DEPTH:\n{sv}");
    assert!(sv.contains("_auto_cc_p_data_send_requires_credit"),
        "send_requires_credit assertion should be present:\n{sv}");
    assert!(sv.contains("p_data_send_valid |-> __p_data_credit > 0"),
        "send_requires_credit property should encode valid-implies-credit:\n{sv}");
}

#[test]
fn test_credit_channel_tier2_receiver_assertion() {
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Cons
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   target DmaCh;
          comb
            p.data_credit_return = 1'b0;
          end comb
        end module Cons
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_auto_cc_p_data_credit_return_requires_buffered"),
        "receiver-side assertion should be present:\n{sv}");
    assert!(sv.contains("p_data_credit_return |-> __p_data_valid"),
        "credit_return should imply buffer-non-empty:\n{sv}");
}

#[test]
fn test_credit_channel_send_sugar() {
    // PR #3b-vi: `p.data.send(x);` desugars to two assignments that set
    // send_valid and send_data.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          port payload: in UInt<8>;
          comb
            p.data_send_valid = 1'b0;       // default — overridden below
            p.data_send_data  = 8'h0;
            if p.data.can_send
              p.data.send(payload);
            end if
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("p_data_send_valid = 1'd1")
          || sv.contains("p_data_send_valid = 1'b1"),
        ".send() should set send_valid to 1:\n{sv}");
    assert!(sv.contains("p_data_send_data = payload"),
        ".send(payload) should set send_data to payload:\n{sv}");
}

#[test]
fn test_credit_channel_pop_sugar() {
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Cons
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   target DmaCh;
          port want_pop: in Bool;
          comb
            p.data_credit_return = 1'b0;
            if p.data.valid and want_pop
              p.data.pop();
            end if
          end comb
        end module Cons
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("p_data_credit_return = 1'd1")
          || sv.contains("p_data_credit_return = 1'b1"),
        ".pop() should assert credit_return:\n{sv}");
}

#[test]
fn test_credit_channel_end_to_end_noc_producer_consumer() {
    // PR #5: canonical credit_channel validation — one producer, one
    // consumer, one shared credit_channel. Exercises the full stack:
    //  * Wire flattening at both port perspectives.
    //  * Sender-side credit counter + can_send dispatch.
    //  * Target-side FIFO + pop/credit_return wiring.
    //  * Read-side dispatch (can_send / valid / data).
    //  * Write-side sugar (.send(x) / .pop()).
    //  * Tier-2 SVA on both sides.
    let source = "
        bus NocChannel
          credit_channel flits: send
            param T:     type  = UInt<64>;
            param DEPTH: const = 8;
          end credit_channel flits
        end bus NocChannel

        use NocChannel;

        module NocProducer
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port out: initiator NocChannel;
          port gen_pressure: in UInt<8>;
          reg seq_no: UInt<64> init 0 reset rst => 0;
          reg lfsr:   UInt<8>  init 8'h5A reset rst => 8'h5A;
          comb
            out.flits_send_valid = 1'b0;
            out.flits_send_data  = 64'h0;
            if out.flits.can_send
              out.flits.send(seq_no);
            end if
          end comb
          seq on clk rising
            if (lfsr[0] == 1'b1)
              lfsr <= (lfsr >> 1) ^ 8'hB8;
            else
              lfsr <= lfsr >> 1;
            end if
            if out.flits.can_send and (lfsr < gen_pressure)
              seq_no <= (seq_no + 1).trunc<64>();
            end if
          end seq
        end module NocProducer

        module NocConsumer
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port incoming: target NocChannel;
          port pop_pressure:      in  UInt<8>;
          port reg popped_count:  out UInt<64> reset rst => 0;
          port reg last_seq:      out UInt<64> reset rst => 0;
          reg lfsr: UInt<8> init 8'hC3 reset rst => 8'hC3;
          comb
            incoming.flits_credit_return = 1'b0;
            if incoming.flits.valid and (lfsr < pop_pressure)
              incoming.flits.pop();
            end if
          end comb
          seq on clk rising
            if (lfsr[0] == 1'b1)
              lfsr <= (lfsr >> 1) ^ 8'hB8;
            else
              lfsr <= lfsr >> 1;
            end if
            if incoming.flits.valid and (lfsr < pop_pressure)
              popped_count <= (popped_count + 1).trunc<64>();
              last_seq     <= incoming.flits.data;
            end if
          end seq
        end module NocConsumer
    ";
    let sv = compile_to_sv(source);

    // Sender-side checks
    assert!(sv.contains("__out_flits_credit"), "sender credit reg:\n{sv}");
    assert!(sv.contains("__out_flits_can_send"), "sender can_send:\n{sv}");
    assert!(sv.contains("_auto_cc_out_flits_credit_bounds"), "sender SVA:\n{sv}");
    assert!(sv.contains("_auto_cc_out_flits_send_requires_credit"), "sender SVA:\n{sv}");

    // .send(x) sugar must materialize both signals
    assert!(sv.contains("out_flits_send_valid = 1'd1"), "send sugar valid:\n{sv}");
    assert!(sv.contains("out_flits_send_data = seq_no"), "send sugar data:\n{sv}");

    // Receiver-side checks
    assert!(sv.contains("__incoming_flits_buf"), "receiver buffer:\n{sv}");
    assert!(sv.contains("__incoming_flits_valid"), "receiver valid wire:\n{sv}");
    assert!(sv.contains("__incoming_flits_data"), "receiver data wire:\n{sv}");
    assert!(sv.contains("_auto_cc_incoming_flits_credit_return_requires_buffered"),
        "receiver SVA:\n{sv}");

    // .pop() sugar
    assert!(sv.contains("incoming_flits_credit_return = 1'd1"),
        "pop sugar credit_return:\n{sv}");

    // Read-side dispatch in seq and comb contexts
    assert!(sv.contains("last_seq <= __incoming_flits_data"),
        "read-side dispatch in seq:\n{sv}");
}

#[test]
fn test_credit_channel_sim_emits_sender_state() {
    // PR-sim-1: arch sim --pybind --test path now mirrors the sender-side
    // credit counter. Verifies the C++ model contains the field, the
    // constructor init, eval_comb's can_send assignment, and the
    // eval_posedge counter update.
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Prod
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   initiator DmaCh;
          comb
            p.data_send_valid = 1'b0;
            p.data_send_data  = 8'h0;
          end comb
        end module Prod
    ";
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("uint32_t __p_data_credit;"),
        "sender credit field should be declared:\n{out}");
    assert!(out.contains("uint8_t  __p_data_can_send;"),
        "sender can_send field should be declared:\n{out}");
    assert!(out.contains("__p_data_credit = 4;"),
        "constructor should initialize credit to DEPTH:\n{out}");
    assert!(out.contains("__p_data_can_send = (__p_data_credit != 0)"),
        "eval_comb should assign can_send combinationally:\n{out}");
    assert!(out.contains("__p_data_credit--"),
        "eval_posedge should decrement on pure send:\n{out}");
    assert!(out.contains("__p_data_credit++"),
        "eval_posedge should increment on pure credit_return:\n{out}");
}

#[test]
fn test_credit_channel_sim_emits_receiver_state() {
    let source = "
        bus DmaCh
          credit_channel data: send
            param T:     type  = UInt<8>;
            param DEPTH: const = 4;
          end credit_channel data
        end bus DmaCh

        use DmaCh;

        module Cons
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p:   target DmaCh;
          comb
            p.data_credit_return = 1'b0;
          end comb
        end module Cons
    ";
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("uint8_t __p_data_buf[4];"),
        "receiver buffer array should be declared with correct width + depth:\n{out}");
    assert!(out.contains("__p_data_head;") && out.contains("__p_data_tail;")
         && out.contains("__p_data_occ;"),
        "head/tail/occ pointers should be declared:\n{out}");
    assert!(out.contains("__p_data_valid = (__p_data_occ != 0)"),
        "valid should be computed in eval_comb:\n{out}");
    assert!(out.contains("__p_data_data  = __p_data_buf[__p_data_head]"),
        "data should read front of buffer in eval_comb:\n{out}");
    assert!(out.contains("p_data_credit_return && __p_data_valid"),
        "pop should fire on user-driven credit_return when valid:\n{out}");
}

#[test]
fn test_tlm_method_parses_as_bus_sub_construct() {
    // PR-tlm-1 scaffolding: parser recognizes `tlm_method` inside a bus,
    // captures name, args, ret type, and `blocking` mode.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
          tlm_method write(addr: UInt<32>, data: UInt<64>) -> Bool: blocking;
          tlm_method poke(addr: UInt<32>): blocking;
        end bus Mem
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let bus = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Bus(b) if b.name.name == "Mem" => Some(b),
        _ => None,
    }).expect("Mem bus in AST");
    assert_eq!(bus.tlm_methods.len(), 3);

    let r = &bus.tlm_methods[0];
    assert_eq!(r.name.name, "read");
    assert_eq!(r.args.len(), 1);
    assert_eq!(r.args[0].0.name, "addr");
    assert!(r.ret.is_some());
    assert_eq!(r.mode.name, "blocking");

    let w = &bus.tlm_methods[1];
    assert_eq!(w.name.name, "write");
    assert_eq!(w.args.len(), 2);

    let p = &bus.tlm_methods[2];
    assert_eq!(p.name.name, "poke");
    assert!(p.ret.is_none(), "void methods should have ret=None");
}

#[test]
fn test_tlm_method_wires_flatten_at_bus_port() {
    // PR-tlm-2: tlm_method declarations flatten to a request channel
    // (valid/args/ready) plus response channel (valid/data/ready) at
    // the bus port use site. Method dispatch / FSM lowering are still
    // unimplemented; users who drive the flattened wires directly
    // compile cleanly.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Initiator
          port m: initiator Mem;
          comb
            m.read_req_valid = 1'b0;
            m.read_addr      = 32'h0;
            m.read_rsp_ready = 1'b0;
          end comb
        end module Initiator
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("output logic m_read_req_valid"),
        "req_valid should be an initiator output:\n{sv}");
    assert!(sv.contains("output logic [31:0] m_read_addr"),
        "arg should appear as initiator output with its declared type:\n{sv}");
    assert!(sv.contains("input logic m_read_req_ready"),
        "req_ready should flow back to initiator:\n{sv}");
    assert!(sv.contains("input logic m_read_rsp_valid"),
        "rsp_valid should be an initiator input:\n{sv}");
    assert!(sv.contains("input logic [63:0] m_read_rsp_data"),
        "rsp_data should appear with declared ret type:\n{sv}");
    assert!(sv.contains("output logic m_read_rsp_ready"),
        "rsp_ready flows back from initiator to target:\n{sv}");
}

#[test]
fn test_tlm_method_void_omits_rsp_data() {
    // Void methods (no -> RetType) should NOT materialize rsp_data.
    let source = "
        bus Mem
          tlm_method poke(addr: UInt<32>): blocking;
        end bus Mem

        use Mem;

        module I
          port m: initiator Mem;
          comb
            m.poke_req_valid = 1'b0;
            m.poke_addr = 32'h0;
            m.poke_rsp_ready = 1'b0;
          end comb
        end module I
    ";
    let sv = compile_to_sv(source);
    assert!(!sv.contains("poke_rsp_data"),
        "void methods must not emit rsp_data:\n{sv}");
    assert!(sv.contains("poke_rsp_valid"),
        "void methods still need rsp_valid/ready for back-pressure:\n{sv}");
}

#[test]
fn test_tlm_target_thread_parses_with_dotted_name() {
    // PR-tlm-3 scaffolding: parser recognizes
    //   `thread port.method(arg1, arg2, ...) on clk rising, rst high`
    // and stores the TLM target binding on the ThreadBlock.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          port ready: in Bool;
          thread s.read(addr) on clk rising, rst high
            wait until ready;
          end thread s.read
        end module MemTarget
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let m = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Module(m) if m.name.name == "MemTarget" => Some(m),
        _ => None,
    }).expect("MemTarget in AST");
    let t = m.body.iter().find_map(|i| match i {
        arch::ast::ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).expect("thread in MemTarget body");
    let binding = t.tlm_target.as_ref().expect("tlm_target should be populated");
    assert_eq!(binding.port.name, "s");
    assert_eq!(binding.method.name, "read");
    assert_eq!(binding.args.len(), 1);
    assert_eq!(binding.args[0].name, "addr");
}

#[test]
fn test_tlm_initiator_call_site_expands_in_ast() {
    // PR-tlm-4: verified at the AST level. End-to-end SV still blocked
    // on thread lowering bridging bus-port-member drives into the
    // extracted sub-module's output ports — see plan_tlm_method.md.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Initiator
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d:  UInt<64> reset rst => 0;
          thread driver on clk rising, rst high
            d <= m.read(32'h1000);
          end thread driver
        end module Initiator
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target");
    let ast = arch::elaborate::lower_tlm_initiator_calls(ast).expect("tlm init");
    let m = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Module(m) if m.name.name == "Initiator" => Some(m),
        _ => None,
    }).expect("Initiator module");
    let t = m.body.iter().find_map(|i| match i {
        arch::ast::ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).expect("thread");
    assert!(t.body.len() >= 5,
        "initiator call site should expand to >=5 stmts, got {}: {:?}",
        t.body.len(), t.body);
    let wait_count = t.body.iter()
        .filter(|s| matches!(s, arch::ast::ThreadStmt::WaitUntil(_, _)))
        .count();
    assert!(wait_count >= 2,
        "expected >=2 WaitUntils (req_ready + rsp_valid), got {wait_count}");
}

#[test]
fn test_tlm_call_rejected_outside_seq_assign_rhs() {
    // Arithmetic on a TLM call RHS is not supported in v1 — must be
    // direct.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Bad
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d:  UInt<64> reset rst => 0;
          thread driver on clk rising, rst high
            d <= m.read(32'h1000) + 64'h1;
          end thread driver
        end module Bad
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target");
    let r = arch::elaborate::lower_tlm_initiator_calls(ast);
    assert!(r.is_err(), "nested TLM call in RHS should be rejected");
    let msg = format!("{:?}", r.unwrap_err());
    assert!(msg.contains("direct right-hand side") || msg.contains("direct"),
        "expected direct-RHS error, got: {msg}");
}

#[test]
fn test_tlm_target_thread_lowers_inline_to_state_machine() {
    // PR-tlm-4b: TLM target thread lowers in-place to a state-reg +
    // RegBlock + CombBlock in the parent module body (no sub-module
    // extraction). End-to-end SV compiles cleanly.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          port ready: in Bool;
          thread s.read(addr) on clk rising, rst high
            wait until ready;
            return 64'h42;
          end thread s.read
        end module MemTarget
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_s_read_state"),
        "state register should appear in SV:\n{sv}");
    assert!(sv.contains("_tlm_s_read_addr_latched"),
        "arg latch reg should appear in SV:\n{sv}");
    assert!(sv.contains("s_read_req_ready"),
        "req_ready driver should appear in SV:\n{sv}");
    assert!(sv.contains("s_read_rsp_valid"),
        "rsp_valid driver should appear in SV:\n{sv}");
}

#[test]
fn test_tlm_target_thread_parses_return_stmt() {
    // PR-tlm-3b: `return expr;` inside a thread body is now a valid
    // ThreadStmt::Return variant. (lower_threads still rejects TLM
    // target threads; FSM rewrite lands in PR-tlm-3c.)
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          port ready: in Bool;
          thread s.read(addr) on clk rising, rst high
            wait until ready;
            return 64'h42;
          end thread s.read
        end module MemTarget
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let m = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Module(m) if m.name.name == "MemTarget" => Some(m),
        _ => None,
    }).expect("MemTarget in AST");
    let t = m.body.iter().find_map(|i| match i {
        arch::ast::ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).expect("thread in body");
    let has_return = t.body.iter().any(|s| matches!(s, arch::ast::ThreadStmt::Return(_, _)));
    assert!(has_return, "body should contain a Return stmt");
}

#[test]
fn test_return_in_regular_thread_errors_in_lower_threads() {
    // `return` outside a TLM target thread is a user error — hit by
    // lower_threads with a targeted message.
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port reg out_r: out UInt<8> reset rst => 0;
          thread stray on clk rising, rst high
            out_r <= 8'h1;
            return 8'h2;
          end thread stray
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let result = arch::elaborate::lower_threads(ast);
    assert!(result.is_err(), "regular thread with return should error");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("return") && msg.contains("TLM method target thread"),
        "expected targeted error, got: {msg}");
}

#[test]
fn test_tlm_target_thread_accepts_matched_closing_method_name() {
    // Closing `end thread port.method` should match the opening.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          port ready: in Bool;
          thread s.read(addr) on clk rising, rst high
            wait until ready;
          end thread s.wrong_method
        end module MemTarget
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    assert!(parser.parse_source_file().is_err(),
        "mismatched closing method name should be a parse error");
}

#[test]
fn test_tlm_method_target_perspective_flips() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Target
          port s: target Mem;
          comb
            s.read_req_ready = 1'b0;
            s.read_rsp_valid = 1'b0;
            s.read_rsp_data  = 64'h0;
          end comb
        end module Target
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("input logic s_read_req_valid"),
        "target perspective: req_valid should be an input:\n{sv}");
    assert!(sv.contains("output logic s_read_req_ready"),
        "target perspective: req_ready flows back as output:\n{sv}");
    assert!(sv.contains("output logic s_read_rsp_valid"),
        "target perspective: rsp_valid is output:\n{sv}");
    assert!(sv.contains("output logic [63:0] s_read_rsp_data"),
        "target perspective: rsp_data is output:\n{sv}");
    assert!(sv.contains("input logic s_read_rsp_ready"),
        "target perspective: rsp_ready flows back as input:\n{sv}");
}

#[test]
fn test_tlm_method_v2_modes_rejected_in_v1() {
    for mode in ["pipelined", "out_of_order", "burst"] {
        let source = format!(
            "bus Mem
              tlm_method read(addr: UInt<32>) -> UInt<64>: {mode};
            end bus Mem"
        );
        let tokens = arch::lexer::tokenize(&source).expect("lexer");
        let mut parser = arch::parser::Parser::new(tokens, &source);
        let result = parser.parse_source_file();
        assert!(result.is_err(),
            "mode `{mode}` should be rejected in v1: source={source}");
    }
}

#[test]
fn test_credit_channel_mismatched_closing_keyword_errors() {
    let source = "
        bus B
          credit_channel data: send
            param T:     type  = UInt<64>;
            param DEPTH: const = 8;
          end credit_channel wrong_name
        end bus B
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    assert!(parser.parse_source_file().is_err(),
        "mismatched credit_channel close should be a parse error");
}

#[test]
fn test_handshake_mismatched_closing_keyword_errors() {
    // Opening with `handshake_channel` but closing with `end handshake`
    // (or vice versa) should be a parse error.
    let source = "
        bus B
          handshake_channel cmd: send kind: valid_ready
            addr: UInt<32>;
          end handshake cmd
        end bus B
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let result = parser.parse_source_file();
    assert!(result.is_err(), "expected parse error for mismatched opening/closing keyword");
}
