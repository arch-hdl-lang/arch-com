use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn compile_to_sv(source: &str) -> String {
    compile_to_sv_with_opts(source, &elaborate::ThreadLowerOpts::default())
}

fn compile_to_sv_with_opts(source: &str, opts: &elaborate::ThreadLowerOpts) -> String {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator lowering error");
    let ast = elaborate::lower_threads_with_opts(ast, opts).expect("lower_threads error");
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
    assert!(sv.contains("input logic [3:0] max"));
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
    // pc is now passed forward through registered stages instead of
    // being read directly from Fetch (which would be a 3-hop bypass).
    assert!(sv.contains("assign pc_out = writeback_pc"), "missing pc output");
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
fn test_seq_for_loop_bare_assign_rejected() {
    // SV antipattern: `target <= expr;` inside a `for` loop in seq has
    // no cumulative effect — every iteration reads the same pre-block
    // value of the target and only the last write commits. Reject at
    // typecheck.
    let bad = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BadAccum
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port d: in UInt<8>;
  port o: out UInt<10>;
  reg sum: UInt<10> reset rst => 10'h0;
  comb o = sum; end comb
  seq on clk rising
    for i in 0..3
      sum <= sum + d.zext<10>();   // bug: never accumulates
    end for
  end seq
end module BadAccum
"#;
    let tokens = arch::lexer::tokenize(bad).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, bad);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let result = arch::typecheck::TypeChecker::new(&symbols, &ast).check();
    let errs = result.expect_err("expected typecheck to reject bare-ident <= in seq for-loop");
    let msg = errs.iter().map(|e| format!("{e:?}")).collect::<String>();
    assert!(msg.contains("non-blocking assignment") && msg.contains("for"),
            "expected for-loop NBA error, got: {msg}");
}

#[test]
fn test_seq_for_loop_indexed_assign_allowed() {
    // Indexed targets (`vec[i] <= ...`) write a different element per
    // iteration and stay allowed — canonical shift-register / fill-Vec
    // patterns. Verify the rule doesn't false-positive on these.
    let ok = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module ShiftFill
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port d: in UInt<8>;
  port o: out Vec<UInt<8>, 4>;
  reg shifty: Vec<UInt<8>, 4> reset rst => 0;
  comb o = shifty; end comb
  seq on clk rising
    for i in 0..3
      shifty[i] <= d;     // indexed: each iter writes a different slot — OK
    end for
  end seq
end module ShiftFill
"#;
    let tokens = arch::lexer::tokenize(ok).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, ok);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    arch::typecheck::TypeChecker::new(&symbols, &ast).check()
        .expect("indexed-target NBA in for-loop should typecheck");
}

#[test]
fn test_pipeline_flush_clear_emits_data_reset() {
    // Pipeline critique #6: `flush <Stage> when <cond> clear;` resets
    // every data register in the target stage, not just `valid_r`.
    // Default (no `clear`) is bubble-only.
    let bubble_src = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

pipeline FlushBubble
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port d: in UInt<8>;
  port abort: in Bool;
  port o: out UInt<8>;
  stage Fetch
    reg captured: UInt<8> reset rst => 8'h0;
    seq on clk rising
      captured <= d;
    end seq
    comb
      o = captured;
    end comb
  end stage Fetch
  flush Fetch when abort;
end pipeline FlushBubble
"#;
    let clear_src = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

pipeline FlushClear
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port d: in UInt<8>;
  port abort: in Bool;
  port o: out UInt<8>;
  stage Fetch
    reg captured: UInt<8> reset rst => 8'h0;
    seq on clk rising
      captured <= d;
    end seq
    comb
      o = captured;
    end comb
  end stage Fetch
  flush Fetch when abort clear;
end pipeline FlushClear
"#;
    let bubble_sv = compile_to_sv(bubble_src);
    let clear_sv = compile_to_sv(clear_src);

    // Both must reset valid_r on flush.
    assert!(bubble_sv.contains("fetch_valid_r <= 1'b0"));
    assert!(clear_sv.contains("fetch_valid_r <= 1'b0"));

    // Only the `clear` form resets the data reg in the flush branch.
    // Both forms include the always_ff reset-branch assignment (= 1
    // occurrence in bubble); clear adds a second in the abort branch.
    let bubble_count = bubble_sv.matches("fetch_captured <= 8'd0").count();
    let clear_count = clear_sv.matches("fetch_captured <= 8'd0").count();
    assert_eq!(bubble_count, 1,
        "bubble form should have 1 reset of fetch_captured (the always_ff reset branch only); got {bubble_count}\n{bubble_sv}");
    assert_eq!(clear_count, 2,
        "clear form should have 2 resets of fetch_captured (always_ff reset + flush clear); got {clear_count}\n{clear_sv}");
}

#[test]
fn test_pipeline_cross_stage_skip_rejected() {
    // Regression for #4 (pipeline_critique): a stage that reads from
    // a stage more than one hop back must error at typecheck. The
    // reference would emit a direct combinational path bypassing the
    // intermediate stages' registers.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

pipeline SkipBypass
  param XLEN: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<XLEN>;
  port out: out UInt<XLEN>;

  stage Fetch
    reg captured: UInt<XLEN> reset rst => 0;
    seq on clk rising
      captured <= data_in;
    end seq
  end stage Fetch

  stage Decode
    reg copy: UInt<XLEN> reset rst => 0;
    seq on clk rising
      copy <= Fetch.captured;
    end seq
  end stage Decode

  stage Writeback
    reg result: UInt<XLEN> reset rst => 0;
    seq on clk rising
      result <= Decode.copy;
    end seq
    comb
      // BAD: reaches back two stages — bypasses Decode's register.
      out = Fetch.captured;
    end comb
  end stage Writeback
end pipeline SkipBypass
"#;
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    let errs = result.expect_err("expected typecheck to flag the 2-hop bypass");
    let msg = errs.iter().map(|e| format!("{e:?}")).collect::<String>();
    assert!(msg.contains("bypassing the intermediate"),
            "expected bypass message, got: {msg}");
}

#[test]
fn test_pipeline_forward_reference_allowed() {
    // Forward references (Decode reading Execute) are hazard reads and
    // must NOT be flagged by the cross-stage span check — they're the
    // canonical pattern for forwarding muxes and load-use stall checks.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

pipeline ForwardRead
  param XLEN: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port data_in: in UInt<XLEN>;
  port out: out UInt<XLEN>;

  stage Decode
    reg val: UInt<XLEN> reset rst => 0;
    seq on clk rising
      val <= data_in;
    end seq
    comb
      // Forward read into Execute is allowed (hazard).
      forwarded = Execute.result;
    end comb
  end stage Decode

  stage Execute
    reg result: UInt<XLEN> reset rst => 0;
    seq on clk rising
      result <= Decode.val;
    end seq
    comb
      out = result;
    end comb
  end stage Execute
end pipeline ForwardRead
"#;
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    checker.check().expect("forward-reference pattern should typecheck");
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
fn test_linklist_multi_head_sim_shape() {
    // Phase C: sim_codegen mirror of multi-head linklist. Head/tail
    // become per-head arrays; per-head length counter drives empty
    // detection; `_ctrl_<op>_head_idx` latches req_head_idx at accept.
    let source = r#"
linklist MhQ
  param DEPTH: const = 8;
  param NUM_HEADS: const = 2;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  track tail: true;
  op insert_tail
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<3>;
  end op insert_tail
  op delete_head
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port resp_valid:   out Bool;
    port resp_data:    out UInt<8>;
  end op delete_head
end linklist MhQ
"#;
    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("uint8_t _head_r[2]"), "missing _head_r[2]:\n{sim}");
    assert!(sim.contains("uint8_t _tail_r[2]"), "missing _tail_r[2]");
    assert!(sim.contains("uint8_t _length_r[2]"), "missing _length_r[2]");
    assert!(sim.contains("_ctrl_insert_tail_head_idx"),
            "missing insert_tail head_idx latch");
    assert!(sim.contains("_ctrl_delete_head_head_idx"),
            "missing delete_head head_idx latch");
    // Delete ready gated by per-head length
    assert!(sim.contains("_length_r[delete_head_req_head_idx] != 0"),
            "missing per-head delete ready gate");
    // Busy-cycle head/tail access uses the latched idx
    assert!(sim.contains("_head_r[_ctrl_delete_head_head_idx]"),
            "missing busy-cycle head ref");
    assert!(sim.contains("_tail_r[_ctrl_insert_tail_head_idx]"),
            "missing busy-cycle tail ref");
    // Per-head length updates
    assert!(sim.contains("_length_r[_ctrl_insert_tail_head_idx]++"),
            "missing length inc in insert");
    assert!(sim.contains("_length_r[_ctrl_delete_head_head_idx]--"),
            "missing length dec in delete");
}

#[test]
fn test_linklist_multi_head_compiles() {
    // Phase B: multi-head linklist with NUM_HEADS > 1 emits per-head
    // head/tail/length arrays and latches req_head_idx per op.
    let source = r#"
linklist MhQ
  param DEPTH: const = 16;
  param NUM_HEADS: const = 4;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  track tail: true;
  op insert_tail
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<2>;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<4>;
  end op insert_tail
  op delete_head
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<2>;
    port resp_valid:   out Bool;
    port resp_data:    out UInt<8>;
  end op delete_head
end linklist MhQ
"#;
    let sv = compile_to_sv(source);
    // Module header carries NUM_HEADS param
    assert!(sv.contains("parameter int  NUM_HEADS = 4"), "missing NUM_HEADS param:\n{sv}");
    // Head/tail/length become arrays indexed by NUM_HEADS
    assert!(sv.contains("_head_r [NUM_HEADS]"), "missing head array");
    assert!(sv.contains("_tail_r [NUM_HEADS]"), "missing tail array");
    assert!(sv.contains("_length_r [NUM_HEADS]"), "missing internal length array");
    // Per-op latched head_idx register
    assert!(sv.contains("_ctrl_insert_tail_head_idx"), "missing insert_tail head_idx latch");
    assert!(sv.contains("_ctrl_delete_head_head_idx"), "missing delete_head head_idx latch");
    // Accept cycle reads head/tail by request idx directly; busy cycle
    // by the latched idx.
    assert!(sv.contains("_head_r[delete_head_req_head_idx]"), "missing accept-cycle head ref");
    assert!(sv.contains("_tail_r[_ctrl_insert_tail_head_idx]"), "missing busy-cycle tail ref");
    // req_ready for delete gated by per-head length
    assert!(sv.contains("_length_r[delete_head_req_head_idx] != '0"),
            "missing per-head delete ready gate");
    // Reset loops through NUM_HEADS
    assert!(sv.contains("for (_ll_i = 0; _ll_i < NUM_HEADS; _ll_i++)"),
            "missing NUM_HEADS reset loop");
}

#[test]
fn test_linklist_multi_head_full_ops_sv_shape() {
    // Multi-head SV codegen for the remaining head-addressed ops:
    // insert_head, insert_after, delete. Each must latch req_head_idx
    // at accept, route head/tail reads/writes through the latched idx
    // at the busy cycle, and bump _length_r[idx] ±1.
    let source = r#"
linklist MhFull
  param DEPTH: const = 16;
  param NUM_HEADS: const = 2;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind doubly;
  track tail: true;
  op insert_head
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<4>;
  end op insert_head
  op insert_after
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port req_handle:   in UInt<4>;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<4>;
  end op insert_after
  op delete
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port req_handle:   in UInt<4>;
    port resp_valid:   out Bool;
  end op delete
end linklist MhFull
"#;
    let sv = compile_to_sv(source);
    // No $fatal stub anywhere — all three ops are now wired.
    assert!(!sv.contains("not yet implemented for multi-head"),
            "stub message should be gone:\n{sv}");
    // insert_head: head_idx latch + busy-cycle uses latched idx + length++
    assert!(sv.contains("_ctrl_insert_head_head_idx  <= insert_head_req_head_idx"),
            "missing insert_head idx latch:\n{sv}");
    assert!(sv.contains("_head_r[_ctrl_insert_head_head_idx] <= _ctrl_insert_head_resp_handle"),
            "missing insert_head busy-cycle head update");
    assert!(sv.contains("_length_r[_ctrl_insert_head_head_idx] <= _length_r[_ctrl_insert_head_head_idx] + 1'b1"),
            "missing insert_head length increment");
    assert!(sv.contains("_ctrl_insert_head_was_empty <= (_length_r[insert_head_req_head_idx] == '0)"),
            "missing per-head was_empty check on insert_head");
    // insert_after: head_idx latch + length++ (pointer patches stay shared)
    assert!(sv.contains("_ctrl_insert_after_head_idx <= insert_after_req_head_idx"),
            "missing insert_after idx latch");
    assert!(sv.contains("_length_r[_ctrl_insert_after_head_idx] <= _length_r[_ctrl_insert_after_head_idx] + 1'b1"),
            "missing insert_after length increment");
    // delete: head_idx latch + length-- + per-head ready gate
    assert!(sv.contains("_ctrl_delete_head_idx <= delete_req_head_idx"),
            "missing delete idx latch");
    assert!(sv.contains("_length_r[_ctrl_delete_head_idx] <= _length_r[_ctrl_delete_head_idx] - 1'b1"),
            "missing delete length decrement");
    assert!(sv.contains("_length_r[delete_req_head_idx] != '0"),
            "missing per-head delete ready gate");
}

#[test]
fn test_linklist_multi_head_full_ops_sim_shape() {
    // Sim-codegen mirror of the same three new multi-head ops.
    let source = r#"
linklist MhFull
  param DEPTH: const = 8;
  param NUM_HEADS: const = 2;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind doubly;
  track tail: true;
  op insert_head
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<3>;
  end op insert_head
  op insert_after
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port req_handle:   in UInt<3>;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<3>;
  end op insert_after
  op delete
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<1>;
    port req_handle:   in UInt<3>;
    port resp_valid:   out Bool;
  end op delete
end linklist MhFull
"#;
    let sim = compile_to_sim_h(source, false);
    assert!(!sim.contains("is not yet implemented for multi-head"),
            "stub message should be gone:\n{sim}");
    // insert_head: head_idx latch + length++ + per-head head update
    assert!(sim.contains("_ctrl_insert_head_head_idx = insert_head_req_head_idx"),
            "missing insert_head idx latch:\n{sim}");
    assert!(sim.contains("_head_r[_ctrl_insert_head_head_idx] = _ctrl_insert_head_resp_handle"),
            "missing insert_head busy head update");
    assert!(sim.contains("_length_r[_ctrl_insert_head_head_idx]++"),
            "missing insert_head length increment");
    // insert_after: idx latch + length++
    assert!(sim.contains("_ctrl_insert_after_head_idx = insert_after_req_head_idx"),
            "missing insert_after idx latch");
    assert!(sim.contains("_length_r[_ctrl_insert_after_head_idx]++"),
            "missing insert_after length increment");
    // delete: idx latch + length-- + per-head ready gate
    assert!(sim.contains("_ctrl_delete_head_idx = delete_req_head_idx"),
            "missing delete idx latch");
    assert!(sim.contains("_length_r[_ctrl_delete_head_idx]--"),
            "missing delete length decrement");
    assert!(sim.contains("_length_r[delete_req_head_idx] != 0"),
            "missing per-head delete ready gate");
}

#[test]
fn test_linklist_multi_head_rejects_missing_head_idx() {
    // NUM_HEADS > 1 but per-head op omits req_head_idx → typecheck error
    let source = r#"
linklist BadMh
  param DEPTH: const = 8;
  param NUM_HEADS: const = 2;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  track tail: true;
  op insert_tail
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<3>;
  end op insert_tail
end linklist BadMh
"#;
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let result = TypeChecker::new(&symbols, &ast).check();
    assert!(result.is_err(), "expected typecheck to reject per-head op without req_head_idx");
    let msg = result.unwrap_err().iter().map(|e| format!("{e:?}")).collect::<String>();
    assert!(msg.contains("req_head_idx") && msg.contains("multi-head") == false && msg.contains("NUM_HEADS"),
            "expected NUM_HEADS-specific error, got: {msg}");
}

#[test]
fn test_linklist_single_head_rejects_stray_head_idx() {
    // NUM_HEADS default=1 but op declares req_head_idx → reject
    let source = r#"
linklist BadSh
  param DEPTH: const = 4;
  param DATA: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  track tail: true;
  op insert_tail
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<0>;
    port req_data:     in UInt<8>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<2>;
  end op insert_tail
end linklist BadSh
"#;
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let result = TypeChecker::new(&symbols, &ast).check();
    assert!(result.is_err(), "expected typecheck to reject req_head_idx on single-head list");
    let msg = result.unwrap_err().iter().map(|e| format!("{e:?}")).collect::<String>();
    assert!(msg.contains("req_head_idx") && msg.contains("single-head"),
            "expected single-head-specific error, got: {msg}");
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
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target lowering");
    let ast = arch::elaborate::lower_tlm_initiator_calls(ast).expect("tlm initiator lowering");
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
            p.data.send_valid = 1'b0;
            p.data.send_data  = 16'h0;
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
            p.data.credit_return = 1'b0;
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
            p.data.send_valid = 1'b0;
            p.data.send_data  = 8'h0;
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
            p.data.credit_return = 1'b0;
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
            p.data.send_valid = 1'b0;
            p.data.send_data  = 8'h0;
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
            p.data.credit_return = 1'b0;
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
            p.data.send_valid = p.data.can_send and have_data;
            p.data.send_data  = payload;
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
            p.data.credit_return = p.data.valid and want_pop;
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
            p.data.send_valid = 1'b0;
            p.data.send_data  = 8'h0;
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
            p.data.send_valid = 1'b0;
            p.data.send_data  = 8'h0;
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
            p.data.send_valid = 1'b0;
            p.data.send_data  = 8'h0;
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
            p.data.credit_return = 1'b0;
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
            p.data.send_valid = 1'b0;       // default — overridden below
            p.data.send_data  = 8'h0;
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
            p.data.credit_return = 1'b0;
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
            out.flits.send_valid = 1'b0;
            out.flits.send_data  = 64'h0;
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
            incoming.flits.credit_return = 1'b0;
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
            p.data.send_valid = 1'b0;
            p.data.send_data  = 8'h0;
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
            p.data.credit_return = 1'b0;
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
fn test_tlm_method_out_of_order_tags_parse_and_flatten() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: out_of_order tags 3;
        end bus Mem

        use Mem;

        module Initiator
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m: initiator Mem;
          reg data: UInt<64> reset rst => 0;
          thread driver on clk rising, rst high
            data <= m.read(32'h1000);
          end thread driver
        end module Initiator

        module Target
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s: target Mem;
          reg value: UInt<64> reset rst => 0;
          thread s.read(addr) on clk rising, rst high
            return value;
          end thread s.read
        end module Target
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let bus = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Bus(b) if b.name.name == "Mem" => Some(b),
        _ => None,
    }).expect("Mem bus in AST");
    assert_eq!(bus.tlm_methods[0].mode.name, "out_of_order");
    assert!(bus.tlm_methods[0].out_of_order_tags.is_some());

    let sv = compile_to_sv(source);
    assert!(sv.contains("output logic [2:0] m_read_req_tag")
         && sv.contains("input logic [2:0] m_read_rsp_tag"),
        "initiator should expose out-of-order tag wires:\n{sv}");
    assert!(sv.contains("input logic [2:0] s_read_req_tag")
         && sv.contains("output logic [2:0] s_read_rsp_tag"),
        "target perspective should flip tag wire directions:\n{sv}");
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
fn test_tlm_initiator_call_site_end_to_end_sv() {
    // PR-tlm-4c: initiator call site now inlines to parent-module state
    // machine; end-to-end SV compiles cleanly.
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
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_init_driver_state"),
        "state reg should be emitted:\n{sv}");
    assert!(sv.contains("m_read_req_valid"),
        "SV should drive req_valid:\n{sv}");
    assert!(sv.contains("m_read_addr"),
        "SV should drive the arg:\n{sv}");
    assert!(sv.contains("m_read_rsp_ready"),
        "SV should drive rsp_ready:\n{sv}");
}

#[test]
fn test_tlm_target_sim_mirror_works_via_inlined_state_machine() {
    // PR-tlm-6: because the target lowering emits ordinary RegDecl +
    // RegBlock + CombBlock into the parent module body, sim_codegen
    // handles it via its existing reg / seq / comb mirror machinery —
    // no TLM-specific C++ emission needed.
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
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("_tlm_s_read_state"),
        "state reg should appear in sim C++:\n{out}");
    assert!(out.contains("_tlm_s_read_addr_latched"),
        "arg latch reg should appear in sim C++:\n{out}");
}

#[test]
fn test_tlm_initiator_sim_mirror_works() {
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
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("_tlm_init_driver_state"),
        "initiator state reg should appear in sim C++:\n{out}");
}

#[test]
fn test_tlm_canonical_end_to_end_initiator_plus_target() {
    // PR-tlm-7: canonical validation — a minimal Mem bus with `read`
    // and `write` methods, plus initiator + target pair exercising
    // both sides of the wire protocol.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
          tlm_method write(addr: UInt<32>, data: UInt<64>) -> Bool: blocking;
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
          thread s.write(addr, data) on clk rising, rst high
            wait until ready;
            return 1'b1;
          end thread s.write
        end module MemTarget

        module Initiator
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d0: UInt<64> reset rst => 0;
          reg   d1: UInt<64> reset rst => 0;
          reg   ack: Bool    reset rst => false;
          thread driver on clk rising, rst high
            d0  <= m.read(32'h1000);
            d1  <= m.read(32'h1004);
            ack <= m.write(32'h2000, d0);
          end thread driver
        end module Initiator
    ";
    let sv = compile_to_sv(source);

    // Target-side state machines.
    assert!(sv.contains("_tlm_s_read_state"),
        "target: read state reg should appear:\n{sv}");
    assert!(sv.contains("_tlm_s_write_state"),
        "target: write state reg should appear:\n{sv}");
    assert!(sv.contains("_tlm_s_read_addr_latched"),
        "target: read arg latch reg should appear:\n{sv}");
    assert!(sv.contains("_tlm_s_write_addr_latched")
         && sv.contains("_tlm_s_write_data_latched"),
        "target: write arg latch regs should appear:\n{sv}");

    // Initiator state machine + wire drives.
    assert!(sv.contains("_tlm_init_driver_state"),
        "initiator: driver state reg should appear:\n{sv}");
    assert!(sv.contains("m_read_req_valid")
         && sv.contains("m_write_req_valid"),
        "initiator: both methods should drive req_valid:\n{sv}");
    assert!(sv.contains("m_read_addr")
         && sv.contains("m_write_addr")
         && sv.contains("m_write_data"),
        "initiator: arg signals should appear:\n{sv}");

    // Compile to sim C++ too — same path should flow through the existing
    // reg/seq/comb sim mirror without issues.
    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("_tlm_s_read_state") && sim.contains("_tlm_init_driver_state"),
        "sim C++ should mirror the state regs for both sides");
}

#[test]
fn test_reentrant_thread_parses_with_max() {
    // PR-tlm-p1: `reentrant max N` clause parses into
    // ThreadBlock.reentrant = Some(Some(Expr::Literal(N))).
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port reg out_r: out UInt<8> reset rst => 0;
          thread driver on clk rising, rst high reentrant max 8
            out_r <= 8'h1;
          end thread driver
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let m = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Module(m) if m.name.name == "M" => Some(m),
        _ => None,
    }).expect("module M");
    let t = m.body.iter().find_map(|i| match i {
        arch::ast::ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).expect("thread");
    match &t.reentrant {
        Some(Some(_)) => {} // OK — bounded
        other => panic!("expected Some(Some(Expr)) for `reentrant max 8`, got {:?}", other),
    }
}

#[test]
fn test_reentrant_thread_parses_without_max() {
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port reg out_r: out UInt<8> reset rst => 0;
          thread driver on clk rising, rst high reentrant
            out_r <= 8'h1;
          end thread driver
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let m = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Module(m) if m.name.name == "M" => Some(m),
        _ => None,
    }).expect("module M");
    let t = m.body.iter().find_map(|i| match i {
        arch::ast::ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).expect("thread");
    assert!(matches!(t.reentrant, Some(None)),
        "expected Some(None) for unbounded reentrant, got {:?}", t.reentrant);
}

#[test]
fn test_reentrant_thread_rejected_in_lower_threads() {
    // PR-tlm-p1 scaffolding: lowering ships in PR-tlm-p2/p3.
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port reg out_r: out UInt<8> reset rst => 0;
          thread driver on clk rising, rst high reentrant max 4
            out_r <= 8'h1;
          end thread driver
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target");
    let ast = arch::elaborate::lower_tlm_initiator_calls(ast).expect("tlm init");
    let result = arch::elaborate::lower_threads(ast);
    assert!(result.is_err(), "reentrant thread should be rejected until PR-tlm-p2/p3 land");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("reentrant") && msg.contains("not yet implemented"),
        "expected scaffolding error, got: {msg}");
}

#[test]
fn test_implement_initiator_parses() {
    // PR-tlm-i1: `implement m.read()` clause populates
    // ThreadBlock.implement with kind = Initiator and empty args.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d:  UInt<64> reset rst => 0;
          thread driver implement m.read() on clk rising, rst high
            d <= m.read(32'h1000);
          end thread driver
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let m = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Module(m) if m.name.name == "M" => Some(m),
        _ => None,
    }).expect("module M");
    let t = m.body.iter().find_map(|i| match i {
        arch::ast::ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).expect("thread");
    let b = t.implement.as_ref().expect("implement should be populated");
    assert_eq!(b.kind, arch::ast::TlmImplementKind::Initiator);
    assert_eq!(b.port.name, "m");
    assert_eq!(b.method.name, "read");
    assert!(b.args.is_empty(), "initiator form requires empty args");
}

#[test]
fn test_implement_target_parses() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module T
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          port ready: in Bool;
          thread server implement target s.read(addr) on clk rising, rst high
            wait until ready;
            return 64'h42;
          end thread server
        end module T
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let m = ast.items.iter().find_map(|it| match it {
        arch::ast::Item::Module(m) if m.name.name == "T" => Some(m),
        _ => None,
    }).expect("module T");
    let t = m.body.iter().find_map(|i| match i {
        arch::ast::ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).expect("thread");
    let b = t.implement.as_ref().expect("implement should be populated");
    assert_eq!(b.kind, arch::ast::TlmImplementKind::Target);
    assert_eq!(b.args.len(), 1);
    assert_eq!(b.args[0].name, "addr");
}

#[test]
fn test_implement_target_single_compiles_end_to_end() {
    // PR-tlm-i2: `thread NAME implement target s.read(addr) ... return;`
    // is sugar for the v1 dotted-name target form. Single-implementer
    // case compiles through to SV via existing target lowering.
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
          thread server implement target s.read(addr) on clk rising, rst high
            wait until ready;
            return 64'h42;
          end thread server
        end module MemTarget
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_s_read_state"),
        "state reg should appear in SV:\n{sv}");
    assert!(sv.contains("_tlm_s_read_addr_latched"),
        "arg latch reg should appear in SV:\n{sv}");
    assert!(sv.contains("s_read_req_ready"),
        "req_ready driver should appear:\n{sv}");
}

#[test]
fn test_implement_target_multi_implementer_rejected() {
    // PR-tlm-i2: multi-implementer target case produces a targeted
    // error pointing at PR-tlm-i4.
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
          thread server0 implement target s.read(addr) on clk rising, rst high
            wait until ready;
            return 64'h42;
          end thread server0
          thread server1 implement target s.read(addr) on clk rising, rst high
            wait until ready;
            return 64'h43;
          end thread server1
        end module MemTarget
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let r = arch::elaborate::lower_tlm_target_threads(ast);
    assert!(r.is_err(), "multi-implementer target should error until PR-tlm-i4");
    let msg = format!("{:?}", r.unwrap_err());
    assert!(msg.contains("multi-implementer target") && msg.contains("s.read"),
        "expected targeted error, got: {msg}");
}

#[test]
fn test_implement_initiator_single_compiles_end_to_end() {
    // PR-tlm-i3: single-implementer initiator routes through the
    // existing v1 inline lowering. End-to-end SV compiles.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d:  UInt<64> reset rst => 0;
          thread driver implement m.read() on clk rising, rst high
            d <= m.read(32'h1000);
          end thread driver
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_init_driver_state"),
        "single-implementer initiator should use v1 inline lowering:\n{sv}");
    assert!(sv.contains("m_read_req_valid") && sv.contains("m_read_rsp_ready"),
        "bus signals should be driven:\n{sv}");
}

#[test]
fn test_implement_initiator_multi_implementer_rejected() {
    // PR-tlm-i3: multi-implementer initiator → targeted error pointing
    // at PR-tlm-i4.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d0: UInt<64> reset rst => 0;
          reg   d1: UInt<64> reset rst => 0;
          thread w0 implement m.read() on clk rising, rst high
            d0 <= m.read(32'h1000);
          end thread w0
          thread w1 implement m.read() on clk rising, rst high
            d1 <= m.read(32'h1004);
          end thread w1
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target");
    let r = arch::elaborate::lower_tlm_initiator_calls(ast);
    assert!(r.is_err(), "multi-implementer initiator should error until PR-tlm-i4");
    let msg = format!("{:?}", r.unwrap_err());
    assert!(msg.contains("multi-implementer initiator") && msg.contains("m.read"),
        "expected targeted error, got: {msg}");
}

#[test]
fn test_implement_initiator_with_args_in_parens_errors() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d:  UInt<64> reset rst => 0;
          thread driver implement m.read(addr) on clk rising, rst high
            d <= m.read(32'h1000);
          end thread driver
        end module M
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let r = parser.parse_source_file();
    assert!(r.is_err(), "initiator implement with args should be a parse error");
}

#[test]
fn test_tlm_multi_thread_direct_calls_lower_to_in_order_pool() {
    // Direct multi-thread sharing of a TLM method lowers to an in-order
    // request arbiter + response router. More complex shapes still get
    // targeted diagnostics.
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d0: UInt<64> reset rst => 0;
          reg   d1: UInt<64> reset rst => 0;
          thread w0 on clk rising, rst high
            d0 <= m.read(32'h1000);
          end thread w0
          thread w1 on clk rising, rst high
            d1 <= m.read(32'h1004);
          end thread w1
        end module Shared
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_pool_m_read_fifo"),
        "cohort lowering should emit issue-order FIFO:\n{sv}");
    assert!(sv.contains("_tlm_pool_m_read_t0_state")
         && sv.contains("_tlm_pool_m_read_t1_state"),
        "cohort lowering should emit per-thread state regs:\n{sv}");
    assert!(sv.contains("m_read_req_valid")
         && sv.contains("m_read_rsp_ready"),
        "cohort lowering should drive shared TLM handshakes:\n{sv}");
}

#[test]
fn test_tlm_generate_for_workers_share_method() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   addr: Vec<UInt<32>, 3> reset rst => 0;
          reg   data: Vec<UInt<64>, 3> reset rst => 0;
          generate_for i in 0..2
            thread w_i on clk rising, rst high
              data[i] <= m.read(addr[i]);
            end thread w_i
          end generate_for
        end module Shared
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_pool_m_read_fifo"),
        "generated worker cohort should use pooled TLM lowering:\n{sv}");
    assert!(sv.contains("data[0] <= m_read_rsp_data")
         && sv.contains("data[1] <= m_read_rsp_data")
         && sv.contains("data[2] <= m_read_rsp_data"),
        "each generated worker should capture its routed response:\n{sv}");
}

#[test]
fn test_tlm_fork_join_workers_share_method() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   addr: Vec<UInt<32>, 2> reset rst => 0;
          reg   data: Vec<UInt<64>, 2> reset rst => 0;
          thread workers on clk rising, rst high
            fork
              data[0] <= m.read(addr[0]);
            and
              data[1] <= m.read(addr[1]);
            join
          end thread workers
        end module Shared
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_pool_m_read_fifo"),
        "fork/join TLM workers should use pooled TLM lowering:\n{sv}");
    assert!(sv.contains("data[0] <= m_read_rsp_data")
         && sv.contains("data[1] <= m_read_rsp_data"),
        "fork/join worker responses should route by issue order:\n{sv}");
}

#[test]
fn test_tlm_rhs_fork_join_all_workers_share_method() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   addr: Vec<UInt<32>, 2> reset rst => 0;
          reg   data: Vec<UInt<64>, 2> reset rst => 0;
          thread workers on clk rising, rst high
            data[0] <= fork m.read(addr[0]);
            wait 1 cycle;
            data[1] <= fork m.read(addr[1]);
            join all;
          end thread workers
        end module Shared
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_fork_workers_m_read_age"),
        "forked RHS TLM lowering should emit an issue-age counter:\n{sv}");
    assert!(sv.contains("_tlm_fork_workers_m_read_fifo"),
        "blocking forked RHS TLM should route responses by issue-order FIFO:\n{sv}");
    assert!(sv.contains("data[0] <= m_read_rsp_data")
         && sv.contains("data[1] <= m_read_rsp_data"),
        "forked RHS worker responses should capture routed data:\n{sv}");
}

#[test]
fn test_tlm_rhs_fork_requires_join_all() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d0: UInt<64> reset rst => 0;
          thread workers on clk rising, rst high
            d0 <= fork m.read(32'h1000);
          end thread workers
        end module Shared
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target");
    let r = arch::elaborate::lower_tlm_initiator_calls(ast);
    assert!(r.is_err(), "forked RHS TLM calls should require join all");
    let msg = format!("{:?}", r.unwrap_err());
    assert!(msg.contains("join all"), "expected join-all diagnostic, got: {msg}");
}

#[test]
fn test_tlm_cohort_multi_arg_method() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>, len: UInt<4>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d0: UInt<64> reset rst => 0;
          reg   d1: UInt<64> reset rst => 0;
          thread w0 on clk rising, rst high
            d0 <= m.read(32'h1000, 4'd4);
          end thread w0
          thread w1 on clk rising, rst high
            d1 <= m.read(32'h2000, 4'd8);
          end thread w1
        end module Shared
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_pool_m_read_fifo"),
        "multi-arg method should use pooled TLM lowering:\n{sv}");
    assert!(sv.contains("m_read_addr")
         && sv.contains("m_read_len"),
        "cohort lowering should mux every method arg:\n{sv}");
}

#[test]
fn test_tlm_unsupported_fork_join_call_errors() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module Bad
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d0: UInt<64> reset rst => 0;
          reg   d1: UInt<64> reset rst => 0;
          thread workers on clk rising, rst high
            fork
              d0 <= m.read(32'h1000) + 1;
            and
              d1 <= m.read(32'h1004);
            join
          end thread workers
        end module Bad
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target");
    let r = arch::elaborate::lower_tlm_initiator_calls(ast);
    assert!(r.is_err(), "unsupported fork/join TLM shape should error");
    let msg = format!("{:?}", r.unwrap_err());
    assert!(msg.contains("multi-thread sharing") || msg.contains("TLM initiator thread body"),
        "expected targeted TLM fork/join error, got: {msg}");
}

#[test]
fn test_tlm_out_of_order_fork_join_routes_by_tag() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   addr: Vec<UInt<32>, 2> reset rst => 0;
          reg   data: Vec<UInt<64>, 2> reset rst => 0;
          thread workers on clk rising, rst high
            fork
              data[0] <= m.read(addr[0]);
            and
              data[1] <= m.read(addr[1]);
            join
          end thread workers
        end module Shared
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("m_read_req_tag"),
        "out-of-order cohort should drive request tags:\n{sv}");
    assert!(sv.contains("m_read_rsp_tag == 2'd0")
         && sv.contains("m_read_rsp_tag == 2'd1"),
        "out-of-order cohort should route responses by rsp_tag:\n{sv}");
    assert!(sv.contains("data[0] <= m_read_rsp_data")
         && sv.contains("data[1] <= m_read_rsp_data"),
        "tag-routed responses should capture into each worker destination:\n{sv}");
}

#[test]
fn test_tlm_rhs_fork_out_of_order_routes_by_tag() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
        end bus Mem

        use Mem;

        module Shared
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg   d0: UInt<64> reset rst => 0;
          reg   d1: UInt<64> reset rst => 0;
          thread workers on clk rising, rst high
            d0 <= fork m.read(32'h1000);
            wait 1 cycle;
            d1 <= fork m.read(32'h1004);
            join all;
          end thread workers
        end module Shared
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("m_read_req_tag")
         && sv.contains("m_read_rsp_tag"),
        "OOO forked RHS TLM should drive and consume tag wires:\n{sv}");
    assert!(sv.contains("m_read_rsp_tag == 2'd0")
         && sv.contains("m_read_rsp_tag == 2'd1"),
        "OOO forked RHS responses should route by worker tag:\n{sv}");
}

#[test]
fn test_tlm_out_of_order_target_echoes_tag() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          reg value: UInt<64> reset rst => 0;
          thread s.read(addr) on clk rising, rst high
            return value;
          end thread s.read
        end module MemTarget
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_s_read_tag_latched"),
        "target should latch accepted request tag:\n{sv}");
    assert!(sv.contains("assign s_read_rsp_tag = _tlm_s_read_tag_latched"),
        "target should echo the latched tag on response:\n{sv}");
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
fn test_tlm_method_unsupported_modes_rejected() {
    for mode in ["pipelined", "burst"] {
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

#[test]
fn test_temporal_sva_past_emits_dollar_past() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          port b: in Bool;
          assert eq_past: a == past(b, 2);
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("$past(b, 2)"),
        "past(b, 2) should emit SV $past(b, 2):\n{sv}");
}

#[test]
fn test_temporal_sva_implies_next_emits_pipe_arrow() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          port b: in Bool;
          assert next_implies: a |=> b;
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("a |=> b"),
        "a |=> b should emit SV a |=> b:\n{sv}");
}

#[test]
fn test_past_outside_assert_rejected() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          port out: out Bool;
          comb
            out = past(a, 1);
          end comb
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "past() outside assert should be rejected");
    let errs = result.err().unwrap();
    assert!(errs.iter().any(|e| format!("{e:?}").contains("past")),
        "error should mention past: {errs:?}");
}

#[test]
fn test_implies_next_outside_assert_rejected() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          port out: out Bool;
          comb
            out = a |=> a;
          end comb
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "|=> outside assert should be rejected");
}

#[test]
fn test_past_arity_errors() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          assert bad: past(a);
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err(), "past with wrong arity should error");
}

#[test]
fn test_past_n_must_be_positive_const() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          assert bad: past(a, 0);
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err(), "past(_, 0) should error");
}

#[test]
fn test_phase2_rose_emits_dollar_rose() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          assert e: rose(a);
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("$rose(a)"), "expected $rose:\n{sv}");
}

#[test]
fn test_phase2_fell_emits_dollar_fell() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          assert e: fell(a);
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("$fell(a)"), "expected $fell:\n{sv}");
}

#[test]
fn test_phase2_hashhash_emits_sva_delay() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          port b: in Bool;
          assert e: a |=> ##2 b;
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("##2 b") || sv.contains("##2b"), "expected ##2 in SV:\n{sv}");
}

#[test]
fn test_phase2_rose_outside_assert_rejected() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          port out: out Bool;
          comb out = rose(a); end comb
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err(), "rose() outside assert should be rejected");
}

#[test]
fn test_phase2_hashhash_outside_assert_rejected() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in Bool;
          port out: out Bool;
          comb out = ##1 a; end comb
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_err(), "##N outside assert should be rejected");
}

#[test]
fn test_inst_site_type_param_override_translates_to_data_width() {
    // Parent module instantiates a fifo whose `T: type` param is overridden
    // at the inst site. SV codegen translates the type override into the
    // fifo's synthesized `DATA_WIDTH` int param.
    let source = r#"
        fifo Stack
          kind lifo;
          param DEPTH: const = 8;
          param T: type = UInt<8>;
          port clk: in Clock<SysDomain>;
          port reset: in Reset<Sync>;
          port push_valid: in Bool;
          port push_ready: out Bool;
          port push_data: in T;
          port pop_valid: out Bool;
          port pop_ready: in Bool;
          port pop_data: out T;
        end fifo Stack

        module Wrapper
          param W: const = 16;
          port clk: in Clock<SysDomain>;
          port reset: in Reset<Sync>;
          port d_in: in UInt<W>;
          port d_out: out UInt<W>;

          wire pr: Bool;
          wire pv: Bool;
          inst s: Stack
            param DEPTH = 4;
            param T = UInt<W>;
            clk        <- clk;
            reset      <- reset;
            push_valid <- false;
            push_ready -> pr;
            push_data  <- d_in;
            pop_valid  -> pv;
            pop_ready  <- false;
            pop_data   -> d_out;
          end inst s
        end module Wrapper
    "#;
    let sv = compile_to_sv(source);
    // Type param `T = UInt<W>` should translate to `.DATA_WIDTH(W)` in the SV inst.
    assert!(sv.contains(".DATA_WIDTH(W)"),
        "type override `T = UInt<W>` should emit `.DATA_WIDTH(W)`:\n{sv}");
    assert!(sv.contains(".DEPTH(4)"),
        "value override `DEPTH = 4` should emit `.DEPTH(4)`:\n{sv}");
    // Sanity: no `.T(...)` raw type in the inst — the fifo doesn't expose `T` at SV level.
    assert!(!sv.contains(".T(logic"),
        "should not emit raw `.T(logic ...)` for fifo whose T was synthesized to DATA_WIDTH:\n{sv}");
}

#[test]
fn test_pipe_reg_tap_reads_q_at_k() {
    // `q@K` on RHS reads the K-th tap of a pipe_reg chain.
    // K=0 is the source comb; K=N is the bare q (final output).
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port reset: in Reset<Async, High>;
          port a: in UInt<8>;
          port o0: out UInt<8>;
          port o1: out UInt<8>;
          port o2: out UInt<8>;
          port o3: out UInt<8>;

          pipe_reg q: a stages 3;

          comb
            o0 = q@0;   // source = a
            o1 = q@1;   // q_stg1
            o2 = q@2;   // q_stg2
            o3 = q@3;   // bare q (final flop)
          end comb
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("assign o0 = a;"), "q@0 should be source `a`:\n{sv}");
    assert!(sv.contains("assign o1 = q_stg1;"), "q@1 should be q_stg1:\n{sv}");
    assert!(sv.contains("assign o2 = q_stg2;"), "q@2 should be q_stg2:\n{sv}");
    assert!(sv.contains("assign o3 = q;"), "q@3 should be bare q:\n{sv}");
}

#[test]
fn test_pipe_reg_tap_out_of_range_errors() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port reset: in Reset<Async, High>;
          port a: in UInt<8>;
          port o: out UInt<8>;
          pipe_reg q: a stages 3;
          comb
            o = q@4;   // out of range (max is 3)
          end comb
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse");
    let result = elaborate::lower_pipe_reg_ports(parsed_ast);
    assert!(result.is_err(), "q@4 should be rejected for stages=3");
    let errs = result.err().unwrap();
    assert!(errs.iter().any(|e| format!("{e:?}").contains("exceeds pipe_reg depth")),
        "error should mention depth: {errs:?}");
}

#[test]
fn test_fsm_sint_uses_arithmetic_shift() {
    // SInt regs/lets declared inside an `fsm` should have `>>` emit `>>>`
    // (arithmetic shift right) — same as inside a `module`. Regression
    // test for the missing fsm scope in module_scopes.
    let source = r#"
        fsm F
          port clk: in Clock<SysDomain>;
          port reset: in Reset<Async, High>;
          port a: in SInt<8>;
          port y: out SInt<8>;

          reg buf_y: SInt<8> reset reset=>0;

          state [Idle]
          default state Idle;
          default seq on clk rising;
          default
            comb y = buf_y; end comb
          end default
          state Idle
            seq buf_y <= a >> 1; end seq
            -> Idle;
          end state Idle
        end fsm F
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("a >>> 1"),
        "fsm-scope SInt port `a` shifted right should emit `>>>` (arithmetic):\n{sv}");
    assert!(!sv.contains("a >> 1"),
        "should not emit `>>` (logical) for SInt:\n{sv}");
}

#[test]
fn test_vec_of_const_param_emits_packed_and_indexes() {
    // `param NAME: Vec<T, N> = {a, b, c, ...};` emits a packed SV
    // `parameter logic [N*W-1:0] NAME = {chunks_reversed}` plus `NAME[i]`
    // reads rewrite to `NAME[(i)*(W) +: (W)]` part-selects.
    let source = r#"
        module M
          param coeffs: Vec<UInt<8>, 4> = {1, 2, 3, 4};
          port out: out UInt<8>;
          let out = coeffs[0] + coeffs[1] + coeffs[2] + coeffs[3];
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("parameter logic [(4)*(8)-1:0] coeffs"),
        "expected packed parameter:\n{sv}");
    // Default packed in reverse so coeffs[0] = parts[0] = 1 (LSB).
    assert!(sv.contains("(8)'(4), (8)'(3), (8)'(2), (8)'(1)"),
        "expected reversed default chunks (MSB-first packing):\n{sv}");
    // Indexing rewritten to part-select.
    assert!(sv.contains("coeffs[(0) * (8) +: (8)]"),
        "expected coeffs[0] → coeffs[(0) * (8) +: (8)]:\n{sv}");
}

#[test]
fn test_uint_as_vec_cast_for_find_first() {
    // `as Vec<T, N>` is a typecheck-only view that lets Vec methods
    // (find_first, etc.) operate on a UInt directly without a manual
    // for-loop bit unpack. The generated SV indexes the UInt's bits
    // directly with no intermediate Vec wire.
    let source = r#"
        module M
          port d: in UInt<8>;
          port idx: out UInt<3>;
          port hit: out Bool;

          let { found, index } = (d as Vec<Bool, 8>).find_first(item);
          let idx = found ? index : 0.zext<3>();
          let hit = found;
        end module M
    "#;
    let sv = compile_to_sv(source);
    // No bit-unpack `for` loop or intermediate Vec wire.
    assert!(!sv.contains("for ("),
        "should not synthesize a for-loop bit unpack:\n{sv}");
    // Should index `d[i]` directly in the priority encoder.
    assert!(sv.contains("d[0]") && sv.contains("d[7]"),
        "expected direct bit indexing of `d`:\n{sv}");
}

#[test]
fn test_counter_runtime_max_port() {
    // `counter` with a `port max: in UInt<W>` overrides the compile-time
    // MAX param. Wrap target, saturate ceiling, and `at_max` all consult
    // the runtime port instead of the const.
    let source = r#"
        counter ProgCounter
          kind wrap;
          direction: up;
          init: 0;
          port clk:    in Clock<SysDomain>;
          port rst:    in Reset<Async, Low>;
          port inc:    in Bool;
          port max:    in UInt<8>;
          port value:  out UInt<8>;
          port at_max: out Bool;
        end counter ProgCounter
    "#;
    let sv = compile_to_sv(source);
    // Wrap compare uses the runtime `max` port, not a const.
    assert!(sv.contains("count_r == max"),
        "expected wrap compare against `max` port:\n{sv}");
    // at_max output mirrors the same compare.
    assert!(sv.contains("assign at_max = (count_r == max)"),
        "expected at_max against `max` port:\n{sv}");
    // No const MAX appears (no MAX param declared).
    assert!(!sv.contains("'(MAX)"),
        "should not emit const MAX comparator when port is present:\n{sv}");
}

// ── Auto-emitted SVA from thread lowering ─────────────────────────────────────

#[test]
fn test_auto_thread_asserts_off_by_default() {
    let source = include_str!("../tests/thread/wait_cycles.arch");
    let sv = compile_to_sv(source);
    assert!(!sv.contains("_auto_thread_"),
        "default lowering must not emit auto-thread asserts:\n{sv}");
}

#[test]
fn test_auto_thread_asserts_wait_cycles_and_until() {
    // DelayPulse thread covers both wait_until (state 0: `wait until start`)
    // and wait N cycle (states 1 and 3). Verify both property classes
    // emit, wrapped in `synopsys translate_off/on`, with reset-guarded
    // antecedents.
    let source = include_str!("../tests/thread/wait_cycles.arch");
    let opts = elaborate::ThreadLowerOpts { auto_asserts: true };
    let sv = compile_to_sv_with_opts(source, &opts);

    // Wait-until: state 0 transitions on `start`.
    assert!(sv.contains("_auto_thread_t0_wait_until_s0:"),
        "expected wait_until property at state 0:\n{sv}");
    assert!(sv.contains("|=> _t0_state == 1"),
        "expected next-cycle implication to state 1:\n{sv}");

    // Wait-cycles: stay + done assertions.
    assert!(sv.contains("_auto_thread_t0_wait_stay_s1:"),
        "expected wait-cycles stay assertion:\n{sv}");
    assert!(sv.contains("_auto_thread_t0_wait_done_s1:"),
        "expected wait-cycles done assertion:\n{sv}");

    // Reset guard: rst_n is active-low, so `not_in_reset == rst_n`.
    assert!(sv.contains("rst_n &&"),
        "expected reset guard `rst_n && ...` in antecedent:\n{sv}");

    // SVA wrapped in translate_off/on (so synth ignores it).
    assert!(sv.contains("// synopsys translate_off"),
        "expected translate_off wrapping:\n{sv}");
    assert!(sv.contains("// synopsys translate_on"),
        "expected translate_on wrapping:\n{sv}");
}

#[test]
fn test_auto_thread_asserts_fork_join_branches() {
    // fork/join produces multi_transitions. Each branch transition gets
    // an `_auto_thread_t{i}_branch_s{s}_b{b}` assertion.
    let source = include_str!("../tests/thread/fork_join.arch");
    let opts = elaborate::ThreadLowerOpts { auto_asserts: true };
    let sv = compile_to_sv_with_opts(source, &opts);
    assert!(sv.contains("_auto_thread_t0_branch_s"),
        "expected at least one fork/join branch assertion:\n{sv}");
}

#[test]
fn test_auto_thread_asserts_active_high_reset() {
    // Active-high reset (`Reset<Sync>` defaults to High) must produce a
    // `!rst` guard, not `rst`.
    let source = r#"
        module M
          port clk:   in Clock<SysDomain>;
          port rst:   in Reset<Sync>;
          port go:    in Bool;
          port done:  out Bool;
          thread on clk rising, rst high
            wait until go;
            done = 1;
            wait 1 cycle;
          end thread
        end module M
    "#;
    let opts = elaborate::ThreadLowerOpts { auto_asserts: true };
    let sv = compile_to_sv_with_opts(source, &opts);
    assert!(sv.contains("!rst &&"),
        "expected `!rst` guard for active-high reset:\n{sv}");
    assert!(!sv.contains("(rst) &&"),
        "should not use bare `rst` as guard for active-high:\n{sv}");
}

// ── If/else with internal waits — dispatch-and-rejoin ─────────────────────────

#[test]
fn test_if_wait_then_only() {
    // Wait inside the then-branch only. else-branch is empty (vacuous else).
    // Expected lowering: dispatch -> [then states] -> rejoin; cond false jumps
    // straight to rejoin (per §II.10.4 empty-branch rule).
    let source = r#"
        module M
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port grant:in Bool;
          port done: out Bool;
          thread on clk rising, rst low
            wait until go;
            if grant
              wait until grant;
              done = 1;
              wait 1 cycle;
            end if
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    // Compiles without rejection.
    assert!(sv.contains("module _M_threads"),
        "merged thread module should be emitted:\n{sv}");
}

#[test]
fn test_if_wait_else_only() {
    let source = r#"
        module M
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port skip: in Bool;
          port done: out Bool;
          thread on clk rising, rst low
            wait until go;
            if skip
              done = 1;
            else
              wait 2 cycle;
              done = 1;
            end if
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("module _M_threads"),
        "merged thread module should be emitted:\n{sv}");
}

#[test]
fn test_if_wait_both_branches() {
    // Both branches have waits, of different lengths. Dispatch picks one,
    // each branch redirects to a common rejoin.
    let source = r#"
        module M
          port clk:    in Clock<SysDomain>;
          port rst:    in Reset<Async, Low>;
          port is_wr:  in Bool;
          port aw_rdy: in Bool;
          port w_rdy:  in Bool;
          port b_vld:  in Bool;
          port ar_rdy: in Bool;
          port r_vld:  in Bool;
          port aw_v:   out Bool;
          port w_v:    out Bool;
          port ar_v:   out Bool;
          port r_r:    out Bool;
          port done:   out Bool;
          thread on clk rising, rst low
            if is_wr
              aw_v = 1;
              wait until aw_rdy;
              w_v = 1;
              wait until w_rdy;
              wait until b_vld;
            else
              ar_v = 1;
              wait until ar_rdy;
              r_r = 1;
              wait until r_vld;
            end if
            done = 1;
            wait 1 cycle;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("module _M_threads"),
        "merged thread module should be emitted:\n{sv}");
    // The dispatch state's transition table negates the if condition for
    // the else branch. Verify both arms land at distinct branch bases.
    assert!(sv.contains("is_wr") && sv.contains("!"),
        "expected dispatch to use `is_wr` and `!is_wr` arms:\n{sv}");
}

#[test]
fn test_if_wait_with_auto_asserts() {
    // Verify --auto-thread-asserts still emits a coherent set of properties
    // when the thread contains a wait-bearing if/else. The dispatch state's
    // multi_transitions surface as `_auto_thread_t0_branch_*` covers.
    let source = r#"
        module M
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port f:    in Bool;
          port done: out Bool;
          thread on clk rising, rst low
            wait until go;
            if f
              wait until f;
            else
              wait 1 cycle;
            end if
            done = 1;
            wait 1 cycle;
          end thread
        end module M
    "#;
    let opts = elaborate::ThreadLowerOpts { auto_asserts: true };
    let sv = compile_to_sv_with_opts(source, &opts);
    assert!(sv.contains("_auto_thread_t0_branch_"),
        "expected dispatch-state branch assertions:\n{sv}");
}

// ── resource arbiter policies ─────────────────────────────────────────────────

#[test]
fn test_resource_lock_priority_default() {
    // No explicit `resource` decl — implicit fallback to priority. The
    // synthesized arbiter must wire correctly and emit the priority logic.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go0: in Bool;
          port go1: in Bool;
          port go2: in Bool;
          port done: out Bool shared(or);

          thread on clk rising, rst low
            wait until go0;
            lock shared_lk
              done = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread

          thread on clk rising, rst low
            wait until go1;
            lock shared_lk
              done = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread

          thread on clk rising, rst low
            wait until go2;
            lock shared_lk
              done = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    // Synthesized arbiter is named _arb_<Mod>_<res>.
    assert!(sv.contains("module _arb_M_shared_lk"),
        "expected synthesized arbiter module:\n{sv}");
    // Default = priority arbiter (linear pri_i loop).
    assert!(sv.contains("for (int pri_i = 0; pri_i < 3"),
        "default policy should be priority:\n{sv}");
    // Inst inside the merged module.
    assert!(sv.contains("_arb_M_shared_lk _arb_inst_shared_lk"),
        "expected arbiter instance inside merged module:\n{sv}");
}

#[test]
fn test_resource_lock_round_robin() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go0: in Bool;
          port go1: in Bool;
          port done: out Bool shared(or);

          resource shared_lk: mutex<round_robin>;

          thread on clk rising, rst low
            wait until go0;
            lock shared_lk
              done = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread

          thread on clk rising, rst low
            wait until go1;
            lock shared_lk
              done = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("logic [0:0] rr_ptr_r;"),
        "round_robin should emit rr_ptr_r register:\n{sv}");
    assert!(sv.contains("rr_ptr_r <= rr_ptr_r + 1"),
        "round_robin should increment pointer on grant:\n{sv}");
}

#[test]
fn test_resource_lock_custom_policy_with_hook() {
    let source = r#"
        function PickHigh(req_mask: UInt<2>, _last: UInt<2>) -> UInt<2>
          return req_mask & 2'b10;
        end function PickHigh

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go0: in Bool;
          port go1: in Bool;
          port done: out Bool shared(or);

          resource shared_lk: mutex<PickHigh>
            hook grant_select(req_mask: UInt<2>, last_grant: UInt<2>) -> UInt<2>
                 = PickHigh(req_mask, last_grant);
          end resource shared_lk

          thread on clk rising, rst low
            wait until go0;
            lock shared_lk
              done = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread

          thread on clk rising, rst low
            wait until go1;
            lock shared_lk
              done = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    // Custom policy emits the user's function inside the synthesized arbiter.
    assert!(sv.contains("function automatic"),
        "custom-policy arbiter should embed the user function:\n{sv}");
    assert!(sv.contains("PickHigh"),
        "expected user function name in arbiter:\n{sv}");
    // last_grant_r register comes from the custom-arbiter codegen.
    assert!(sv.contains("last_grant_r"),
        "custom-policy arbiter should track last_grant_r:\n{sv}");
}

#[test]
fn test_if_wait_nested() {
    // Nesting: inner if-with-wait inside outer if-with-wait. The recursive
    // partition_thread_body call is what enables nesting per §II.10.4.
    let source = r#"
        module M
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port a:    in Bool;
          port b:    in Bool;
          port done: out Bool;
          thread on clk rising, rst low
            wait until go;
            if a
              wait 1 cycle;
              if b
                wait 1 cycle;
              else
                wait 2 cycle;
              end if
            else
              wait 3 cycle;
            end if
            done = 1;
            wait 1 cycle;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("module _M_threads"),
        "nested if/else with waits should compile:\n{sv}");
}

#[test]
fn test_if_wait_for_in_then_branch() {
    // Regression test for the for-loop-in-then-branch asymmetry bug
    // (see doc/thread_lowering_proof.md §II.10.4).
    //
    // Before the fix, the for-loop's exit-sentinel (usize::MAX) resolved
    // to `then_base + then_len = else_base`, and `redirect_fallthrough_to`
    // then appended `(true, rejoin_idx)` which always overrode the
    // for-loop's loop-back arm under last-write-wins. The for-loop body
    // executed exactly once instead of N times.
    //
    // After the fix, any target equal to `else_base` in the then-branch
    // states is rewritten to `rejoin_idx` before the redirect, so the
    // for-exit naturally lands at rejoin_idx and no spurious append
    // occurs.
    let source = r#"
        module M
          param burst_len: const = 4;
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port doit: in Bool;
          port ack:  in Bool;
          port done: out Bool;
          thread on clk rising, rst low
            wait until go;
            if doit
              for i in 0..burst_len-1
                wait until ack;
                done = 1;
              end for
            else
              wait 1 cycle;
            end if
            wait 1 cycle;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("module _M_threads"),
        "for-loop in then-branch should compile:\n{sv}");
    // Bug witness: the buggy lowering emitted `if (1'b1) _t0_state <= <rejoin>`
    // inside the for-loop's last state, causing the body to execute exactly once.
    // The fix removes this unconditional override, so the only state-write
    // arms inside state 4 (for-loop last) should be the loop-back and exit
    // arms — both guarded by `_t0_loop_cnt` comparisons against `burst_len - 1`.
    assert!(!sv.contains("if (1'b1) begin\n          _t0_state"),
        "buggy unconditional override should not be emitted:\n{sv}");
    // The for-loop's exit arm should land at the rejoin state (post-if
    // wait_cycles), not at the start of the else branch.
    // The else branch is `wait 1 cycle` (one state); the rejoin is the
    // post-if `wait 1 cycle` (one state). With the fix, `_t0_loop_cnt >=
    // (burst_len - 1)` should write the rejoin state, not else_base.
    let exit_arm = sv.contains("if (_t0_loop_cnt >= 16'(burst_len - 1)) begin")
        || sv.contains("if (_t0_loop_cnt >= 16'(burst_len-1)) begin");
    assert!(exit_arm,
        "for-loop exit arm should compare loop_cnt against burst_len-1:\n{sv}");
}

// ── Doc comments and frontmatter (V1) ─────────────────────────────────────────

fn parse_to_ast(source: &str) -> arch::ast::SourceFile {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    parser.parse_source_file().expect("parse error")
}

#[test]
fn test_doc_outer_attaches_to_module() {
    let source = "
        /// Saturating up-counter.
        ///
        /// Wraps to MAX and never overflows.
        module Sat
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port v: out UInt<4>;
        end module Sat
    ";
    let ast = parse_to_ast(source);
    let m = match &ast.items[0] {
        arch::ast::Item::Module(m) => m,
        _ => panic!("expected module"),
    };
    let doc = m.doc.as_ref().expect("module should have outer doc");
    assert!(doc.contains("Saturating up-counter."),
        "outer doc text missing first line: {doc:?}");
    assert!(doc.contains("Wraps to MAX"),
        "outer doc text missing third line: {doc:?}");
}

#[test]
fn test_doc_inner_attaches_to_module() {
    let source = "
        module M
          //! This module guards CSR access with a 2-cycle pipeline.

          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port q: out Bool;
        end module M
    ";
    let ast = parse_to_ast(source);
    let m = match &ast.items[0] {
        arch::ast::Item::Module(m) => m,
        _ => panic!("expected module"),
    };
    let inner = m.inner_doc.as_ref().expect("module should have inner doc");
    assert!(inner.contains("CSR access"), "inner doc text missing: {inner:?}");
    assert!(m.doc.is_none(), "outer doc should be None, got: {:?}", m.doc);
}

#[test]
fn test_file_inner_doc_and_frontmatter() {
    let source = "//! ---
//! spec_md: doc/specs/dma_engine.md
//! tags: [dma, axi]
//! refs:
//!   - \"AXI4 §A3.3.1\"
//! ---
//!
//! Multi-channel DMA engine. See spec_md for the channel state diagram.

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
end module Top
";
    let ast = parse_to_ast(source);
    let inner = ast.inner_doc.as_ref().expect("file should carry inner_doc");
    assert!(inner.contains("Multi-channel DMA engine"),
        "file inner_doc should preserve the prose summary:\n{inner}");
    assert!(inner.contains("---"),
        "file inner_doc should retain the frontmatter delimiters verbatim:\n{inner}");
    let fm = ast.frontmatter.as_ref().expect("file should carry frontmatter");
    assert!(fm.contains("spec_md: doc/specs/dma_engine.md"),
        "frontmatter should preserve spec_md field:\n{fm}");
    assert!(fm.contains("tags: [dma, axi]"),
        "frontmatter should preserve tags:\n{fm}");
    let open_close: Vec<_> = fm.lines().filter(|l| l.trim() == "---").collect();
    assert_eq!(open_close.len(), 2,
        "frontmatter should contain exactly 2 `---` delimiter lines:\n{fm}");
}

#[test]
fn test_doc_outer_on_counter() {
    let source = "
        /// 4-bit wrap counter, used by the simple watchdog test.
        counter Wd
          kind wrap;
          init: 0;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port inc: in Bool;
          port max: in UInt<4>;
          port value: out UInt<4>;
        end counter Wd
    ";
    let ast = parse_to_ast(source);
    let c = match &ast.items[0] {
        arch::ast::Item::Counter(c) => c,
        _ => panic!("expected counter"),
    };
    let doc = c.common.doc.as_ref().expect("counter should have outer doc");
    assert!(doc.contains("watchdog"), "outer doc text missing: {doc:?}");
}

#[test]
fn test_four_slashes_dropped_as_regular_comment() {
    // `////` is the documented escape hatch — must NOT attach as a doc.
    let source = "
        //// This is a banner, not documentation.
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
        end module M
    ";
    let ast = parse_to_ast(source);
    let m = match &ast.items[0] {
        arch::ast::Item::Module(m) => m,
        _ => panic!("expected module"),
    };
    assert!(m.doc.is_none(),
        "4-slash banner should not attach as a doc comment, got: {:?}", m.doc);
}

// ── PR-doc-1.5: outer-doc + inner-doc on every top-level item kind ────────────

#[test]
fn test_doc_outer_on_struct_enum_function() {
    let source = "
        /// Cache line state struct.
        struct CacheLine
          tag: UInt<20>;
          data: UInt<512>;
        end struct CacheLine

        /// Branch direction predictor outcome.
        enum Predict
          Taken,
          NotTaken
        end enum Predict

        /// Saturating add helper.
        function sat_add(a: UInt<8>, b: UInt<8>) -> UInt<8>
          return a +% b;
        end function sat_add
    ";
    let ast = parse_to_ast(source);
    let s = match &ast.items[0] {
        arch::ast::Item::Struct(s) => s,
        _ => panic!("expected struct"),
    };
    assert!(s.doc.as_ref().map_or(false, |d| d.contains("Cache line state")),
        "struct should have outer doc, got {:?}", s.doc);
    let e = match &ast.items[1] {
        arch::ast::Item::Enum(e) => e,
        _ => panic!("expected enum"),
    };
    assert!(e.doc.as_ref().map_or(false, |d| d.contains("Branch direction")),
        "enum should have outer doc, got {:?}", e.doc);
    let f = match &ast.items[2] {
        arch::ast::Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert!(f.doc.as_ref().map_or(false, |d| d.contains("Saturating add")),
        "function should have outer doc, got {:?}", f.doc);
}

#[test]
fn test_doc_outer_on_bus_synchronizer_clkgate() {
    let source = "
        /// Reusable AXI4 bus port bundle.
        bus AxiB
          aw_valid: out Bool;
          aw_ready: in  Bool;
          aw_addr:  out UInt<32>;
        end bus AxiB

        /// 2-FF synchronizer for a CDC `start` strobe.
        synchronizer Sync2
          kind ff;
          param STAGES: const = 2;
          port src_clk: in Clock<SysDomain>;
          port dst_clk: in Clock<SysDomain>;
          port d:       in Bool;
          port q:       out Bool;
        end synchronizer Sync2
    ";
    let ast = parse_to_ast(source);
    let b = match &ast.items[0] {
        arch::ast::Item::Bus(b) => b,
        _ => panic!("expected bus"),
    };
    assert!(b.doc.as_ref().map_or(false, |d| d.contains("AXI4")),
        "bus should have outer doc, got {:?}", b.doc);
    let s = match &ast.items[1] {
        arch::ast::Item::Synchronizer(s) => s,
        _ => panic!("expected synchronizer"),
    };
    assert!(s.doc.as_ref().map_or(false, |d| d.contains("2-FF")),
        "synchronizer should have outer doc, got {:?}", s.doc);
}

#[test]
fn test_inner_doc_on_counter_and_arbiter() {
    let source = "
        counter Wd
          //! 4-bit watchdog timer used by the test harness.
          kind wrap;
          init: 0;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port inc: in Bool;
          port max: in UInt<4>;
          port value: out UInt<4>;
        end counter Wd

        arbiter A
          //! Round-robin arbiter — fairness for the 4 DMA channels.
          policy round_robin;
          param NUM_REQ: const = 4;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          ports[NUM_REQ] request
            valid: in Bool;
            ready: out Bool;
          end ports request
          port grant_valid: out Bool;
          port grant_requester: out UInt<2>;
        end arbiter A
    ";
    let ast = parse_to_ast(source);
    let c = match &ast.items[0] {
        arch::ast::Item::Counter(c) => c,
        _ => panic!("expected counter"),
    };
    assert!(c.common.inner_doc.as_ref().map_or(false, |d| d.contains("watchdog timer")),
        "counter inner_doc missing, got {:?}", c.common.inner_doc);
    let a = match &ast.items[1] {
        arch::ast::Item::Arbiter(a) => a,
        _ => panic!("expected arbiter"),
    };
    assert!(a.common.inner_doc.as_ref().map_or(false, |d| d.contains("Round-robin")),
        "arbiter inner_doc missing, got {:?}", a.common.inner_doc);
}

#[test]
fn test_doc_outer_on_use_decl() {
    // `use` is a single-line decl — only outer-doc applies.
    let source = "
        /// Pull in the cache-line definition from the shared package.
        use Pkg;
    ";
    let ast = parse_to_ast(source);
    let u = match &ast.items[0] {
        arch::ast::Item::Use(u) => u,
        _ => panic!("expected use"),
    };
    assert!(u.doc.as_ref().map_or(false, |d| d.contains("cache-line")),
        "use decl should have outer doc, got {:?}", u.doc);
}

// ── Member-level doc tokens are silently ignored (not yet attached) ──────────

#[test]
fn test_doc_above_port_silently_ignored() {
    // `///` above a port (member-level) is *parsed but discarded* — until
    // PR-doc-1.6 wires up member-level attachment, the parser treats stray
    // doc tokens as transparent whitespace rather than producing the
    // confusing "unexpected token: ///" error.
    let source = "
        module M
          /// stall signal — high while upstream must wait
          port stall_o: out Bool;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          comb
            stall_o = 0;
          end comb
        end module M
    ";
    let ast = parse_to_ast(source);
    let m = match &ast.items[0] {
        arch::ast::Item::Module(m) => m,
        _ => panic!("expected module"),
    };
    // Port still parsed correctly — three ports total.
    assert_eq!(m.ports.len(), 3, "expected 3 ports, got {}", m.ports.len());
    // Module-level outer doc is still None (the `///` was member-level
    // and got silently dropped, NOT promoted to module).
    assert!(m.doc.is_none(), "module doc should be None, got {:?}", m.doc);
}

#[test]
fn test_doc_above_reg_wire_inst_silently_ignored() {
    // `///` above reg / wire / inst (and member docs in general) is
    // tolerated. The parse must succeed and produce the expected body
    // shape.
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port q: out UInt<8>;

          /// data register
          reg val_r: UInt<8> reset rst => 0;

          /// combinational tap
          wire tap: UInt<8>;

          comb
            tap = val_r;
          end comb
          let q = tap;
        end module M
    ";
    let ast = parse_to_ast(source);
    let m = match &ast.items[0] {
        arch::ast::Item::Module(m) => m,
        _ => panic!("expected module"),
    };
    let has_reg = m.body.iter().any(|i| matches!(i, arch::ast::ModuleBodyItem::RegDecl(_)));
    let has_wire = m.body.iter().any(|i| matches!(i, arch::ast::ModuleBodyItem::WireDecl(_)));
    assert!(has_reg, "reg should be present despite leading doc comment");
    assert!(has_wire, "wire should be present despite leading doc comment");
}

#[test]
fn test_inner_doc_inside_body_silently_ignored() {
    // `//!` deep inside a module body (not at the legal post-name position)
    // should also be silently dropped, not error.
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port q: out Bool;

          //! mid-body inner doc (not at the legal position) — silently dropped
          comb
            q = 0;
          end comb
        end module M
    ";
    let ast = parse_to_ast(source);
    let m = match &ast.items[0] {
        arch::ast::Item::Module(m) => m,
        _ => panic!("expected module"),
    };
    // A `//!` not immediately after `module M` is a stray — should NOT
    // attach to inner_doc (which only catches the post-name position).
    assert!(m.inner_doc.is_none(),
        "stray //! mid-body should not attach to inner_doc, got: {:?}", m.inner_doc);
}

// ── regfile `kind: latch` (PR #200) ───────────────────────────────────────────

const LATCH_RF_DECL: &str = "
    domain SysDomain
      freq_mhz: 100
    end domain SysDomain

    regfile LatchRf
      kind latch;
      param NREGS: const = 4;
      param T: type = UInt<8>;
      port clk: in Clock<SysDomain>;
      ports[1] read
        addr: in UInt<2>;
        data: out UInt<8>;
      end ports read
      ports[1] write
        en:   in Bool;
        addr: in UInt<2>;
        data: in UInt<8>;
      end ports write
    end regfile LatchRf
";

#[test]
fn test_regfile_latch_emits_always_latch_per_row() {
    let source = format!("{LATCH_RF_DECL}
        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port we_in:    in Bool;
          port waddr_in: in UInt<2>;
          port wdata_in: in UInt<8>;
          port raddr_in: in UInt<2>;
          port q:        out UInt<8>;

          reg waddr_r: UInt<2> reset rst => 0;
          reg wdata_r: UInt<8> reset rst => 0;
          reg we_r:    Bool    reset rst => false;

          seq on clk rising
            waddr_r <= waddr_in;
            wdata_r <= wdata_in;
            we_r    <= we_in;
          end seq

          inst rf: LatchRf
            clk        <- clk;
            write_en   <- we_r;
            write_addr <- waddr_r;
            write_data <- wdata_r;
            read_addr  <- raddr_in;
            read_data  -> q;
          end inst rf
        end module Top
    ");
    let sv = compile_to_sv(&source);
    let n = sv.matches("always_latch").count();
    assert_eq!(n, 4, "expected 4 always_latch blocks (NREGS=4), got {n}:\n{sv}");
    assert!(sv.contains("write_addr == 2'd0"), "row 0 enable missing:\n{sv}");
    assert!(sv.contains("write_addr == 2'd3"), "row 3 enable missing:\n{sv}");
    let module_body: String = sv.split("module LatchRf").nth(1).unwrap_or(&sv)
        .split("endmodule").next().unwrap_or(&sv).to_string();
    assert!(!module_body.contains("always_ff"),
        "latch RF body should not contain always_ff:\n{module_body}");
}

#[test]
fn test_regfile_latch_rejects_let_source() {
    let source = format!("{LATCH_RF_DECL}
        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<8>;
          port b: in UInt<8>;
          port we_in: in Bool;
          port waddr_in: in UInt<2>;
          port raddr_in: in UInt<2>;
          port q: out UInt<8>;

          let bad_data: UInt<8> = (a + b).trunc<8>();

          inst rf: LatchRf
            clk        <- clk;
            write_en   <- we_in;
            write_addr <- waddr_in;
            write_data <- bad_data;
            read_addr  <- raddr_in;
            read_data  -> q;
          end inst rf
        end module Top
    ");
    let tokens = lexer::tokenize(&source).expect("lex");
    let mut parser = Parser::new(tokens, &source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let ast = elaborate::lower_threads(ast).expect("lower threads");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let result = TypeChecker::new(&symbols, &ast).check();
    assert!(result.is_err(), "latch RF with `let` source should error");
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(err_msg.contains("kind: latch regfile") && err_msg.contains("flop"),
        "diagnostic should explain the flop-source requirement; got: {err_msg}");
}

const LATCH_RF_INTERNAL_DECL: &str = "
    domain SysDomain
      freq_mhz: 100
    end domain SysDomain

    regfile LatchRfInt
      kind latch;
      flops: internal;
      param NREGS: const = 4;
      param T: type = UInt<8>;
      port clk: in Clock<SysDomain>;
      ports[1] read
        addr: in UInt<2>;
        data: out UInt<8>;
      end ports read
      ports[1] write
        en:   in Bool;
        addr: in UInt<2>;
        data: in UInt<8>;
      end ports write
    end regfile LatchRfInt
";

#[test]
fn test_regfile_latch_internal_emits_sample_flops_and_gated_latches() {
    let source = format!("{LATCH_RF_INTERNAL_DECL}
        module Top
          port clk: in Clock<SysDomain>;
          port we_in:    in Bool;
          port waddr_in: in UInt<2>;
          port wdata_in: in UInt<8>;
          port raddr_in: in UInt<2>;
          port q:        out UInt<8>;

          inst rf: LatchRfInt
            clk        <- clk;
            write_en   <- we_in;
            write_addr <- waddr_in;
            write_data <- wdata_in;
            read_addr  <- raddr_in;
            read_data  -> q;
          end inst rf
        end module Top
    ");
    let sv = compile_to_sv(&source);
    let body: String = sv.split("module LatchRfInt").nth(1).unwrap_or(&sv)
        .split("endmodule").next().unwrap_or(&sv).to_string();
    assert!(body.contains("we_q"),     "expected we_q sample flop:\n{body}");
    assert!(body.contains("waddr_q"),  "expected waddr_q sample flop:\n{body}");
    assert!(body.contains("wdata_q"),  "expected wdata_q sample flop:\n{body}");
    assert!(body.contains("always_ff @(posedge clk)"),
        "expected sample flop always_ff:\n{body}");
    let n_latch = body.matches("always_latch").count();
    assert_eq!(n_latch, 4, "expected 4 always_latch blocks (NREGS=4):\n{body}");
    // ICG-equivalent gating: latch transparent only when clk is low.
    assert!(body.contains("!clk"),
        "expected `!clk` gating in latch enable for ICG-equivalent path:\n{body}");
    assert!(body.contains("we_q && waddr_q == 2'd0"),
        "row 0 enable should use sampled (q) signals:\n{body}");
}

#[test]
fn test_regfile_latch_internal_skips_flop_source_check() {
    // With flops: internal the regfile owns its own sample flops, so the
    // caller is allowed to drive write pins from a `let` (combinational
    // expression). This is the static-check skip property.
    let source = format!("{LATCH_RF_INTERNAL_DECL}
        module Top
          port clk: in Clock<SysDomain>;
          port a: in UInt<8>;
          port b: in UInt<8>;
          port we_in: in Bool;
          port waddr_in: in UInt<2>;
          port raddr_in: in UInt<2>;
          port q: out UInt<8>;

          let combinational_data: UInt<8> = (a + b).trunc<8>();

          inst rf: LatchRfInt
            clk        <- clk;
            write_en   <- we_in;
            write_addr <- waddr_in;
            write_data <- combinational_data;
            read_addr  <- raddr_in;
            read_data  -> q;
          end inst rf
        end module Top
    ");
    let tokens = lexer::tokenize(&source).expect("lex");
    let mut parser = Parser::new(tokens, &source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let ast = elaborate::lower_threads(ast).expect("lower threads");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let result = TypeChecker::new(&symbols, &ast).check();
    assert!(result.is_ok(),
        "flops: internal should skip flop-source check; got error: {:?}", result.err());
}

#[test]
fn test_regfile_latch_internal_sim_codegen_emits_sample_flops_and_gated_capture() {
    // sim_codegen must mirror the SV semantics for `flops: internal`:
    // sample we_q/waddr_q/wdata_q on rising edge, then capture into _rf
    // during clk-low (the half-cycle latch transparency window).
    let source = format!("{LATCH_RF_INTERNAL_DECL}");
    let cpp = compile_to_sim_h(&source, false);
    assert!(cpp.contains("_we_q"),    "expected _we_q sample flop in sim:\n{cpp}");
    assert!(cpp.contains("_waddr_q"), "expected _waddr_q sample flop in sim:\n{cpp}");
    assert!(cpp.contains("_wdata_q"), "expected _wdata_q sample flop in sim:\n{cpp}");
    // Posedge: sample.
    assert!(cpp.contains("_we_q = write_en;"),
        "sim should sample _we_q from write_en on posedge:\n{cpp}");
    // Comb: latch transparency gated by `!clk && _we_q`.
    assert!(cpp.contains("if (!clk && _we_q)"),
        "sim should gate latch capture with `!clk && _we_q`:\n{cpp}");
    assert!(cpp.contains("_rf[_waddr_q] = _wdata_q;"),
        "sim should capture _wdata_q into _rf[_waddr_q]:\n{cpp}");
    // Posedge must NOT contain a direct flop-style write — that would
    // collapse the latch into a flop and lose the 1-cycle latency.
    let posedge = cpp.split("eval_posedge() {").nth(1).unwrap_or("")
        .split("}").next().unwrap_or("");
    assert!(!posedge.contains("_rf[write_addr]"),
        "eval_posedge must NOT do a flop-style _rf write under flops:internal:\n{posedge}");
}

#[test]
fn test_regfile_latch_external_sim_codegen_uses_comb_latch() {
    // For `flops: external`, the SV is `always_latch if (we && waddr == k)`,
    // i.e. transparent whenever we is high. The sim mirror is a comb-time
    // _rf update gated by we (no clk gating, no sample flops).
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        regfile LatchExtRf
          kind latch;
          flops: external;
          param NREGS: const = 4;
          param T: type = UInt<8>;
          port clk: in Clock<SysDomain>;
          ports[1] read
            addr: in UInt<2>;
            data: out UInt<8>;
          end ports read
          ports[1] write
            en:   in Bool;
            addr: in UInt<2>;
            data: in UInt<8>;
          end ports write
        end regfile LatchExtRf
    ";
    let cpp = compile_to_sim_h(source, false);
    assert!(!cpp.contains("_we_q"),
        "external flops should NOT emit _we_q sample flop:\n{cpp}");
    // Comb-time latch update gated by write_en.
    assert!(cpp.contains("if (write_en)") && cpp.contains("_rf[write_addr] = write_data;"),
        "external flops sim should be a comb-gated _rf write:\n{cpp}");
    // Posedge eval must not directly sample _rf (that's flop semantics).
    let posedge = cpp.split("eval_posedge() {").nth(1).unwrap_or("")
        .split("}").next().unwrap_or("");
    assert!(!posedge.contains("_rf[write_addr] = write_data"),
        "eval_posedge must not be the only writer under kind:latch:\n{posedge}");
}

#[test]
fn test_regfile_flop_default_unchanged() {
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        regfile FlopRf
          param NREGS: const = 4;
          param T: type = UInt<8>;
          port clk: in Clock<SysDomain>;
          ports[1] read
            addr: in UInt<2>;
            data: out UInt<8>;
          end ports read
          ports[1] write
            en:   in Bool;
            addr: in UInt<2>;
            data: in UInt<8>;
          end ports write
        end regfile FlopRf
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("always_ff @(posedge clk)"),
        "default kind:flop should emit always_ff:\n{sv}");
    assert!(!sv.contains("always_latch"),
        "default kind:flop should not emit always_latch:\n{sv}");
}

#[test]
fn test_sim_codegen_comb_match_arm_recurses_into_nested_for() {
    // Regression for the sim_codegen comb-walker shortcut bug: pre-fix,
    // `emit_comb_stmt::Match` only emitted assigns inside arm bodies and
    // silently dropped nested for-loops, if/else, log, and nested matches.
    // So a comb match where one arm has a `for` would compile to a C++ sim
    // that diverges from `arch build`'s SV output.
    //
    // The repro uses a `for` inside a comb match arm writing to a Vec
    // element. The for-loop should expand to a C++ `for (...)` with a
    // bit-indexed assignment in the body. Pre-fix, the for-loop was
    // silently dropped and the arm emitted nothing.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          port sel: in UInt<2>;
          port q:   out Vec<UInt<8>, 4>;
          comb
            match sel
              0 =>
                for i in 0..3
                  q[i] = 8'hAA;
                end for
              1 =>
                for i in 0..3
                  q[i] = 8'hBB;
                end for
              _ =>
                for i in 0..3
                  q[i] = 8'h00;
                end for
            end match
          end comb
        end module M
    ";
    let cpp = compile_to_sim_h(source, false);
    // Find the comb-eval body for module M and ensure case-0 emits the
    // nested for-loop assigning 0xAA, not nothing.
    let case_0_section = cpp.split("case 0:").nth(1).unwrap_or("");
    let case_0_body = case_0_section.split("break;").next().unwrap_or("");
    assert!(case_0_body.contains("for (int i ="),
        "comb match arm with nested for should emit the for loop (post-fix); pre-fix the comb walker dropped it:\n{cpp}");
    assert!(case_0_body.contains("0xAA") || case_0_body.contains("170"),
        "for-body assign of 0xAA should reach the C++ sim:\n{cpp}");
}

#[test]
fn test_sim_codegen_bit_slice_lhs_compiles_and_uses_param_width() {
    // Regression: pre-fix, `name[hi:lo] = val` in a seq block lowered to
    // the read-side bit-slice form `((name >> lo) & MASK) = val`, an
    // rvalue that gcc/clang reject as "expression is not assignable".
    // Post-fix the slice-LHS arm emits a mask-and-OR analogous to the
    // existing bit-indexed (`name[i] = val`) handling.
    //
    // Additionally, the slice width must be folded against module params
    // — bare `eval_width` returns 32 for `CounterWidth-1`, which would
    // make the LHS clear-mask 33 bits and leak bit[CounterWidth] across
    // writes. Param-aware width evaluation through `eval_width_in` keeps
    // the mask at exactly `CounterWidth` bits.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          param CounterWidth: const = 32;
          port clk:    in Clock<SysDomain>;
          port rst_ni: in Reset<Async, Low>;
          port we:     in Bool;
          port d:      in UInt<32>;
          port q:      out UInt<64>;

          reg counter_q: UInt<64> reset rst_ni => 0;

          default seq on clk rising;

          seq
            if we
              counter_q[CounterWidth-1:0] <= {counter_q[63:32], d}[CounterWidth-1:0];
            end if
          end seq

          let q = counter_q;
        end module M
    ";
    let cpp = compile_to_sim_h(source, false);
    // Slice-LHS must lower to a mask-and-OR write — not the pre-fix rvalue
    // form `((_n_counter_q >> 0) & MASK) = ...`.
    assert!(!cpp.contains(") =") || cpp.contains("== "),
        "slice-LHS regression: rvalue form must not appear:\n{cpp}");
    // The clear-mask must be 32 bits (0xFFFFFFFFULL), not 33 (0x1FFFFFFFFULL).
    assert!(cpp.contains("0xFFFFFFFFULL"),
        "slice-LHS should use a 32-bit mask for [CounterWidth-1:0] when CounterWidth=32:\n{cpp}");
    assert!(!cpp.contains("0x1FFFFFFFFULL"),
        "slice-LHS must not use a 33-bit mask for [CounterWidth-1:0] (param folding regression):\n{cpp}");
    // Sanity: the mask-and-OR shape includes `& ~(uint64_t(0x...` for the clear
    // and `| ((uint64_t(...` for the set.
    assert!(cpp.contains("_n_counter_q = (_n_counter_q & ~"),
        "expected mask-and-OR LHS shape:\n{cpp}");
}

#[test]
fn test_sim_codegen_async_reset_fires_outside_rising_edge() {
    // Regression: pre-fix the seq-block reset arm was emitted INSIDE the
    // `if (_rising_clk)` gate even for async resets, so under
    // `arch sim --pybind` an async-asserted reset only took effect on
    // the next clock edge. Tests asserting reset and observing within
    // the same tick (without spanning a rising edge) read stale state.
    //
    // Fix: emit the reset arm OUTSIDE the rising-edge gate when the
    // reset is async, and write to BOTH `_q` (the live, user-visible
    // value) and `_n_q` (the shadow) so the end-of-cycle commit doesn't
    // restore stale state.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          port clk:    in Clock<SysDomain>;
          port rst_ni: in Reset<Async, Low>;
          port we:     in Bool;
          port d:      in UInt<32>;
          port q:      out UInt<32>;

          reg q_r: UInt<32> reset rst_ni => 0;

          default seq on clk rising;

          seq
            if we
              q_r <= d;
            end if
          end seq

          let q = q_r;
        end module M
    ";
    let cpp = compile_to_sim_h(source, false);
    // The async-reset arm must appear OUTSIDE the rising-edge gate.
    // Anchor on the eval_posedge function definition and slice through
    // the function body (closes at the next `\n}\n` after the open).
    let start_marker = "void VM::eval_posedge() {";
    let pe_start = cpp.find(start_marker)
        .unwrap_or_else(|| panic!("expected eval_posedge body in:\n{cpp}"));
    let pe_body = &cpp[pe_start + start_marker.len()..];
    let pe_body = pe_body.split("\n}\n").next().unwrap_or("");
    let async_pos = pe_body.find("if ((!rst_ni))");
    let rising_pos = pe_body.find("if (_rising_clk)");
    assert!(async_pos.is_some(), "async reset arm should be emitted in eval_posedge:\n{pe_body}");
    assert!(rising_pos.is_some(), "rising-edge guard should be emitted in eval_posedge:\n{pe_body}");
    assert!(async_pos.unwrap() < rising_pos.unwrap(),
        "async reset arm must precede the rising-edge gate:\n{pe_body}");
    // The async arm writes to both `_q_r` (live) and `_n_q_r` (shadow).
    assert!(pe_body.contains("_q_r = 0;") && pe_body.contains("_n_q_r = 0;"),
        "async reset must write both live and shadow regs:\n{pe_body}");
}

#[test]
fn test_sim_codegen_collect_assigns_walks_indexed_lhs() {
    // Regression: `collect_stmt_assigns` only handled `Ident` and
    // `FieldAccess` LHS forms, so `q[hi:lo] <= ...` and `q[i] <= ...`
    // never registered `q` as an assigned reg. Side effect: `reset_sig`
    // ended up None, the seq-block reset arm wasn't emitted at all,
    // and the user got a zero-reset register that drifted to whatever
    // the seq body wrote. The fix walks Index / BitSlice / PartSelect /
    // FieldAccess down to the base Ident.
    //
    // This test triggers the path indirectly: an async-reset register
    // written only via a bit-slice LHS should still get its reset arm
    // emitted in eval_posedge.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          param W: const = 16;
          port clk:    in Clock<SysDomain>;
          port rst_ni: in Reset<Async, Low>;
          port we:     in Bool;
          port d:      in UInt<16>;
          port q:      out UInt<32>;

          reg q_r: UInt<32> reset rst_ni => 0;

          default seq on clk rising;

          seq
            if we
              q_r[W-1:0] <= d;
            end if
          end seq

          let q = q_r;
        end module M
    ";
    let cpp = compile_to_sim_h(source, false);
    assert!(cpp.contains("_q_r = 0;"),
        "indexed-LHS reg should still get its async reset arm:\n{cpp}");
}

#[test]
fn test_cc_dispatch_rewrites_seq_match_scrutinee() {
    // Regression for the elaborate CC-dispatch asymmetry: the reg-block
    // walker used to skip `Stmt::Match` scrutinees (only the comb walker
    // rewrote them), so `match port.ch.data { ... }` inside a `seq` block
    // would slip through to the resolver, which fails with the misleading
    // "bus has no signal X" error. After the fix, both contexts rewrite
    // every expression position uniformly.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain

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
          port classified: out UInt<2>;

          reg cls_r: UInt<2> reset rst => 0;

          comb
            p.data.credit_return = p.data.valid;
            classified = cls_r;
          end comb

          seq on clk rising
            if p.data.valid
              match p.data.data
                0 => cls_r <= 2'd0;
                1 => cls_r <= 2'd1;
                2 => cls_r <= 2'd2;
                _ => cls_r <= 2'd3;
              end match
            end if
          end seq
        end module Cons
    ";
    let sv = compile_to_sv(source);
    // Scrutinee must be rewritten — pre-fix this would compile-error in the
    // resolver because `p.data.data` was untouched.
    assert!(sv.contains("case (__p_data_data)"),
        "seq-block match scrutinee should rewrite to __p_data_data:\n{sv}");
}

#[test]
fn test_typecheck_branch_aware_driven_tracking_in_comb() {
    // Regression for the unified `check_stmt(BlockKind::Comb)` path: a
    // signal driven on both branches of an if/elsif chain in a comb block
    // must be considered fully driven for any downstream multiple-driver
    // analysis, not just on one branch. The parallel-walker era used a
    // clone-and-merge pattern in `check_comb_stmt::IfElse` to track this
    // correctly; the unified `check_stmt` keeps that behavior gated on
    // BlockKind::Comb (Seq path uses simple shared-driven recursion).
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module BranchDriven
          port sel: in Bool;
          port a:   in UInt<8>;
          port b:   in UInt<8>;
          port q:   out UInt<8>;
          comb
            if sel
              q = a;
            else
              q = b;
            end if
          end comb
        end module BranchDriven
    ";
    // Should typecheck cleanly — both branches drive `q`.
    let sv = compile_to_sv(source);
    assert!(sv.contains("module BranchDriven"));
}

#[test]
fn test_comb_for_loop_body_type_checked_as_comb() {
    // Regression for the ForLoop<S> generalization: previously CombStmt::For's
    // body was Vec<Stmt> (lossily cast from Vec<CombStmt>), so the typecheck
    // walked comb for-loop bodies via `check_reg_stmt` and missed the
    // "reg assigned in comb block" rule. After making ForLoop generic, the
    // comb For body stays as Vec<CombStmt> and `check_comb_stmt` runs — which
    // catches reg assigns in comb context as the spec requires.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module Bad
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          reg arr_r: Vec<UInt<8>, 4> reset rst => 0;
          comb
            for i in 0..3
              arr_r[i] = 8'h00;
            end for
          end comb
        end module Bad
    ";
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let ast = elaborate::lower_threads(ast).expect("lower threads");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let result = TypeChecker::new(&symbols, &ast).check();
    assert!(result.is_err(),
        "assigning to a `reg` in a comb-block for-loop body must be a typecheck error");
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(err_msg.contains("`arr_r` is a reg") && err_msg.contains("seq"),
        "diagnostic should explain reg-vs-seq rule; got: {err_msg}");
}

// ── unpacked-array port emission ─────────────────────────────────────────

#[test]
fn test_unpacked_port_emits_unpacked_sv_shape() {
    // Default Vec port: SV packed multi-dim. With `unpacked` modifier:
    // SV unpacked-array shape (`logic [W-1:0] name [N-1:0]`). Used for
    // interop with external SV modules whose port shape is fixed unpacked
    // (e.g. ibex_alu's `imd_val_q_i [2]`). ARCH-internal indexing on the
    // port body works the same in both shapes.
    let source = r#"
module unpacked_demo
  port packed_in:    in Vec<UInt<32>, 2>;
  port unpacked_in:  in unpacked Vec<UInt<32>, 2>;
  port unpacked_out: out unpacked Vec<UInt<32>, 2>;
  comb
    unpacked_out[0] = unpacked_in[0];
    unpacked_out[1] = unpacked_in[1];
  end comb
end module unpacked_demo
"#;
    let sv = compile_to_sv(source);
    // packed Vec port stays packed (default).
    assert!(sv.contains("input logic [1:0] [31:0] packed_in"),
            "packed Vec port should keep packed multi-dim shape, got: {sv}");
    // unpacked Vec port flips to SV unpacked-array shape.
    assert!(sv.contains("input logic [31:0] unpacked_in [1:0]"),
            "unpacked Vec port should emit SV unpacked array, got: {sv}");
    assert!(sv.contains("output logic [31:0] unpacked_out [1:0]"),
            "unpacked Vec output port should emit SV unpacked array, got: {sv}");
    // Internal body indexing is identical regardless of port shape.
    assert!(sv.contains("assign unpacked_out[0] = unpacked_in[0]"));
}

#[test]
fn test_unpacked_on_non_vec_is_rejected() {
    let source = r#"
module unpacked_neg
  port bad: in unpacked UInt<32>;
end module unpacked_neg
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let result = parser.parse_source_file();
    assert!(result.is_err(), "unpacked on non-Vec should be a parse error");
    let msg = format!("{:?}", result.err().unwrap());
    assert!(msg.contains("`unpacked` is only valid on `Vec<T,N>` ports"),
            "diagnostic should explain Vec-only restriction, got: {msg}");
}

#[test]
fn test_unpacked_on_port_reg_is_rejected() {
    let source = r#"
module unpacked_neg2
  port reg bad: out unpacked Vec<UInt<32>, 2>;
end module unpacked_neg2
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let result = parser.parse_source_file();
    assert!(result.is_err(), "unpacked + port reg should be a parse error");
    let msg = format!("{:?}", result.err().unwrap());
    assert!(msg.contains("`unpacked` is not allowed on `port reg`"),
            "diagnostic should explain port-reg restriction, got: {msg}");
}

#[test]
fn test_rdc_violation_one_reset_two_domains() {
    // RDC v1 (phase 1, in-module): a reset signal used by registers in
    // two different clock domains is unsafe — the deassertion edge of
    // the receiving domain isn't synchronised to the source domain. The
    // type checker should flag the second domain's register decl.
    let source = r#"
domain DomA
  freq_mhz: 100
end domain DomA

domain DomB
  freq_mhz: 200
end domain DomB

module BadRdc
  port clk_a: in Clock<DomA>;
  port clk_b: in Clock<DomB>;
  port rst:   in Reset<Async>;
  port a_in:  in UInt<8>;
  port b_in:  in UInt<8>;
  port a_out: out UInt<8>;
  port b_out: out UInt<8>;

  reg ra: UInt<8> reset rst => 0;
  reg rb: UInt<8> reset rst => 0;

  seq on clk_a rising
    ra <= a_in;
  end seq

  seq on clk_b rising
    rb <= b_in;
  end seq

  let a_out = ra;
  let b_out = rb;
end module BadRdc
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected RDC violation");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("RDC violation") && s.contains("rst")
                && s.contains("DomA") && s.contains("DomB")
        }),
        "expected RDC error naming both domains, got: {:?}",
        errs
    );
}

#[test]
fn test_rdc_clean_two_resets_per_domain() {
    // Two clock domains, two distinct reset signals — each reset is only
    // used in its own domain → no RDC violation.
    let source = r#"
domain DomA
  freq_mhz: 100
end domain DomA

domain DomB
  freq_mhz: 200
end domain DomB

module GoodRdc
  port clk_a: in Clock<DomA>;
  port clk_b: in Clock<DomB>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Async>;
  port a_in:  in UInt<8>;
  port b_in:  in UInt<8>;
  port a_out: out UInt<8>;
  port b_out: out UInt<8>;

  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_b => 0;

  seq on clk_a rising
    ra <= a_in;
  end seq

  seq on clk_b rising
    rb <= b_in;
  end seq

  let a_out = ra;
  let b_out = rb;
end module GoodRdc
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_ok(), "expected no RDC error, got: {:?}", result.err());
}

#[test]
fn test_rdc_single_domain_no_violation() {
    // Single clock domain — RDC check is gated on multi-domain, so even
    // multiple registers sharing a reset must not trigger.
    let source = r#"
domain D
  freq_mhz: 100
end domain D

module SingleDomain
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port d:   in UInt<8>;
  port q:   out UInt<8>;

  reg r1: UInt<8> reset rst => 0;
  reg r2: UInt<8> reset rst => 0;

  seq on clk rising
    r1 <= d;
    r2 <= r1;
  end seq

  let q = r2;
end module SingleDomain
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    assert!(checker.check().is_ok());
}


// ─── RDC phase 2a: data-path reset domain crossing tests ────────────────────
// Rule (option 1, sync flops are transparent):
//   reach[f] = { f.reset } if f.reset_kind == Async, else union of reach[srcs]
//   violation: f.Async with src reaching some domain ≠ f.reset, OR
//              f.{Sync,None} with |reach[f]| > 1.
//
// Tests use one seq block per reset signal (the pre-existing rule
// "all regs in a seq block must share their reset signal" still applies).

fn rdc_check(source: &str) -> Result<(), Vec<arch::diagnostics::CompileError>> {
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    checker.check().map(|_| ())
}

fn assert_rdc_ok(label: &str, source: &str) {
    let r = rdc_check(source);
    assert!(r.is_ok(), "[{label}] expected no RDC error, got: {:?}", r.err());
}

fn assert_rdc_fails(label: &str, source: &str, must_contain: &[&str]) {
    let r = rdc_check(source);
    assert!(r.is_err(), "[{label}] expected RDC violation, got: ok");
    let errs = r.unwrap_err();
    let any_match = errs.iter().any(|e| {
        let s = e.to_string();
        // Accepts "RDC" or "CDC" prefix — the reconvergent-sync check
        // shares its diagnostic shape across both hazard classes.
        (s.contains("RDC") || s.contains("CDC")) && must_contain.iter().all(|m| s.contains(m))
    });
    assert!(any_match,
        "[{label}] expected RDC/CDC error containing all of {:?}, got: {:?}",
        must_contain, errs);
}

// ── Group A: direct edges (1-hop) ───────────────────────────────────────────

#[test]
fn rdc_a1_same_async_direct_ok() {
    // ra (rst_a, async) → rb (rst_a, async); same domain → no violation.
    assert_rdc_ok("A1", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_a => 0;
  seq on clk rising
    ra <= d;
    rb <= ra;
  end seq
  let q = rb;
end module M
"#);
}

#[test]
fn rdc_reset_type_cast_at_inst_is_direct_reset_ok() {
    // `rst <- rst_async_n as Reset<Async, Low>` is a reset type override at
    // the inst boundary. It should not be classified as reset-combining logic.
    assert_rdc_ok("reset-cast-inst", include_str!("../examples/param_reset.arch"));
}

#[test]
fn rdc_a2_diff_async_direct_fails() {
    // ra (rst_a, async) → rb (rst_b, async); different async domains → FAIL.
    assert_rdc_fails("A2", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Async>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_b => 0;
  seq on clk rising
    ra <= d;
  end seq
  seq on clk rising
    rb <= ra;
  end seq
  let q = rb;
end module M
"#, &["rst_a", "rst_b"]);
}

#[test]
fn rdc_a3_async_to_sync_fails() {
    // ra (rst_a, async) → rb (rst_b, sync). Strict rule: sync is
    // transparent for propagation but cannot gate its data input on the
    // upstream's async reset event; mid-deassert transients on `ra`
    // metastabilise `rb` and propagate downstream. Flag.
    assert_rdc_fails("A3", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Sync>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_b => 0;
  seq on clk rising
    ra <= d;
  end seq
  seq on clk rising
    rb <= ra;
  end seq
  let q = rb;
end module M
"#, &["rst_a", "rb"]);
}

#[test]
fn rdc_a4_async_to_none_fails() {
    // ra (rst_a, async) → rb (reset none). Strict rule: a reset-less
    // flop cannot gate its data input on the source's async reset
    // event; mid-deassert transients on `ra` metastabilise `rb`. Flag.
    assert_rdc_fails("A4", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> init 0 reset none;
  seq on clk rising
    ra <= d;
  end seq
  seq on clk rising
    rb <= ra;
  end seq
  let q = rb;
end module M
"#, &["rst_a", "rb"]);
}

#[test]
fn rdc_a5_sync_source_ok() {
    // ra (rst_a, sync) sourced from a port has reach[ra]=∅. Then rb
    // (rst_b, async) reads ra → reach[rb's src]=∅ → no violation.
    assert_rdc_ok("A5", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Sync>;
  port rst_b: in Reset<Async>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_b => 0;
  seq on clk rising
    ra <= d;
  end seq
  seq on clk rising
    rb <= ra;
  end seq
  let q = rb;
end module M
"#);
}

// ── Group B: 2-hop chains (the canonical reset-less / sync-bridge bug) ─────

#[test]
fn rdc_b1_async_none_async_diff_fails() {
    // ra (rst_a) → rx (none) → rb (rst_b). reach[rx]={rst_a};
    // reach[rb's src]={rst_a} ≠ rb.reset=rst_b → FAIL at rb.
    assert_rdc_fails("B1", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Async>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rx: UInt<8> init 0 reset none;
  reg rb: UInt<8> reset rst_b => 0;
  seq on clk rising
    ra <= d;
  end seq
  seq on clk rising
    rx <= ra;
  end seq
  seq on clk rising
    rb <= rx;
  end seq
  let q = rb;
end module M
"#, &["rst_a", "rst_b"]);
}

#[test]
fn rdc_b2_async_none_async_same_fails() {
    // ra (rst_a) → rx (reset none) → rb (rst_a). Strict rule: the
    // intermediate reset-less `rx` captures async-domain data without
    // being gated on the upstream reset; even though both async flops
    // share rst_a, the middle hop is the metastability propagator.
    // Fix is to also reset `rx` by rst_a (or add a synchroniser).
    assert_rdc_fails("B2", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rx: UInt<8> init 0 reset none;
  reg rb: UInt<8> reset rst_a => 0;
  seq on clk rising
    ra <= d;
    rb <= rx;
  end seq
  seq on clk rising
    rx <= ra;
  end seq
  let q = rb;
end module M
"#, &["rst_a", "rx"]);
}

#[test]
fn rdc_b3_async_sync_async_diff_fails() {
    // Sync rx is transparent like none → still flagged.
    assert_rdc_fails("B3", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Async>;
  port rst_c: in Reset<Sync>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rx: UInt<8> reset rst_c => 0;
  reg rb: UInt<8> reset rst_b => 0;
  seq on clk rising
    ra <= d;
  end seq
  seq on clk rising
    rx <= ra;
  end seq
  seq on clk rising
    rb <= rx;
  end seq
  let q = rb;
end module M
"#, &["rst_a", "rst_b"]);
}

// ── Group C: convergence at non-async flop ─────────────────────────────────

#[test]
fn rdc_c1_two_async_converge_at_none_fails() {
    // ra (rst_a) and rb (rst_b) both feed rx (none).
    // reach[rx]={rst_a, rst_b} → FAIL at rx.
    assert_rdc_fails("C1", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Async>;
  port da: in UInt<8>;
  port db: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_b => 0;
  reg rx: UInt<8> init 0 reset none;
  seq on clk rising
    ra <= da;
  end seq
  seq on clk rising
    rb <= db;
  end seq
  seq on clk rising
    rx <= (ra + rb).trunc<8>();
  end seq
  let q = rx;
end module M
"#, &["rst_a", "rst_b"]);
}

#[test]
fn rdc_c2_two_same_domain_converge_fails() {
    // Both async sources are rst_a, converging at rx (reset none).
    // Strict rule: rx is reset-less, captures async-domain data without
    // gating on the upstream reset event → flag, even though only one
    // async domain reaches it. The fix is to also reset `rx` by rst_a.
    assert_rdc_fails("C2", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port da: in UInt<8>;
  port db: in UInt<8>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_a => 0;
  reg rx: UInt<8> init 0 reset none;
  seq on clk rising
    ra <= da;
    rb <= db;
  end seq
  seq on clk rising
    rx <= (ra + rb).trunc<8>();
  end seq
  let q = rx;
end module M
"#, &["rst_a", "rx"]);
}

#[test]
fn rdc_c3_async_plus_port_at_none_fails() {
    // ra (rst_a) + port input → rx (reset none). Port contributes no
    // async, but rx still captures async-domain data from `ra` without
    // a reset gate — flag.
    assert_rdc_fails("C3", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port da: in UInt<8>;
  port p:  in UInt<8>;
  port q:  out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rx: UInt<8> init 0 reset none;
  seq on clk rising
    ra <= da;
  end seq
  seq on clk rising
    rx <= (ra + p).trunc<8>();
  end seq
  let q = rx;
end module M
"#, &["rst_a", "rx"]);
}

// ── Group D: multi-clock-domain interactions ───────────────────────────────

#[test]
fn rdc_d1_same_async_two_clocks_no_data_path_phase1_flags() {
    // Phase 1 (currently shipped) flags this — shared async reset across
    // two clock domains, regardless of whether a data path exists. Phase
    // 2's data-path rule alone would let this pass; we keep phase 1 as a
    // structural backstop so the test pins the union of both checks.
    assert_rdc_fails("D1", r#"
domain DA
  freq_mhz: 100
end domain DA
domain DB
  freq_mhz: 200
end domain DB
module M
  port clk_a: in Clock<DA>;
  port clk_b: in Clock<DB>;
  port rst:   in Reset<Async>;
  port da: in UInt<8>;
  port db: in UInt<8>;
  port qa: out UInt<8>;
  port qb: out UInt<8>;
  reg ra: UInt<8> reset rst => 0;
  reg rb: UInt<8> reset rst => 0;
  seq on clk_a rising
    ra <= da;
  end seq
  seq on clk_b rising
    rb <= db;
  end seq
  let qa = ra;
  let qb = rb;
end module M
"#, &["rst", "DA", "DB"]);
}

#[test]
fn rdc_d2_diff_async_diff_clocks_with_path_fails() {
    // Two clocks, two async resets, data path between them → FAIL.
    // Module marks itself `cdc_safe` to opt out of the CDC check (which
    // would otherwise also fire on this design); RDC must still flag.
    assert_rdc_fails("D2", r#"
domain DA
  freq_mhz: 100
end domain DA
domain DB
  freq_mhz: 200
end domain DB
module M
  pragma cdc_safe;
  port clk_a: in Clock<DA>;
  port clk_b: in Clock<DB>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Async>;
  port da: in UInt<8>;
  port q:  out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_b => 0;
  seq on clk_a rising
    ra <= da;
  end seq
  seq on clk_b rising
    rb <= ra;
  end seq
  let q = rb;
end module M
"#, &["rst_a", "rst_b"]);
}

// ── Group E: feedback loops (require fixpoint) ─────────────────────────────

#[test]
fn rdc_e1_self_loop_same_domain_ok() {
    assert_rdc_ok("E1", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  seq on clk rising
    ra <= (ra + 1).trunc<8>();
  end seq
  let q = ra;
end module M
"#);
}

#[test]
fn rdc_e2_mutual_feedback_diff_domains_fails() {
    // ra ↔ rb across different async domains. Fixpoint converges with
    // reach[rb's src]={rst_a} and reach[ra's src]={rst_b}; both flagged.
    assert_rdc_fails("E2", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port rst_b: in Reset<Async>;
  port q: out UInt<8>;
  reg ra: UInt<8> reset rst_a => 0;
  reg rb: UInt<8> reset rst_b => 0;
  seq on clk rising
    ra <= rb;
  end seq
  seq on clk rising
    rb <= ra;
  end seq
  let q = ra;
end module M
"#, &["rst_a", "rst_b"]);
}

// ── Group F: trivial / sanity ──────────────────────────────────────────────

#[test]
fn rdc_f1_single_async_domain_ok() {
    // Several flops all reset by rst_a → no violation.
    assert_rdc_ok("F1", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_a: in Reset<Async>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg r1: UInt<8> reset rst_a => 0;
  reg r2: UInt<8> reset rst_a => 0;
  reg r3: UInt<8> reset rst_a => 0;
  seq on clk rising
    r1 <= d;
    r2 <= r1;
    r3 <= r2;
  end seq
  let q = r3;
end module M
"#);
}

#[test]
fn rdc_f2_no_async_flops_ok() {
    // All sync — phase-2 rule originates no domain → no violation.
    assert_rdc_ok("F2", r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port d: in UInt<8>;
  port q: out UInt<8>;
  reg r1: UInt<8> reset rst => 0;
  reg r2: UInt<8> reset rst => 0;
  seq on clk rising
    r1 <= d;
    r2 <= r1;
  end seq
  let q = r2;
end module M
"#);
}

// ── Group G: Phase 2b — clock-gating cell enable from async reset ─────────
// An async-reset flop driving a `clkgate` enable causes the gate to glitch
// on async reset events → partial clock pulses on the gated output.

#[test]
fn rdc_g1_clkgate_enable_from_async_flop_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_g1_clkgate_enable_from_async_flop_fail.arch")
        .expect("read G1");
    assert_rdc_fails("G1", &src, &["clkgate", "icg", "rst_a"]);
}

#[test]
fn rdc_g2_clkgate_enable_from_port_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_g2_clkgate_enable_from_port_ok.arch")
        .expect("read G2");
    assert_rdc_ok("G2", &src);
}

#[test]
fn rdc_g3_clkgate_enable_from_sync_flop_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_g3_clkgate_enable_from_sync_flop_ok.arch")
        .expect("read G3");
    assert_rdc_ok("G3", &src);
}

// ── Group H: Phase 2c — reconvergent RDC through reset synchronisers ──────
// One async reset routed through two reset synchronisers landing in the
// same destination clock domain → flops reset by the two outputs can be in
// inconsistent state during the deassertion window.

#[test]
fn rdc_h1_reconvergent_two_syncs_same_domain_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_h1_reconvergent_two_syncs_same_domain_fail.arch")
        .expect("read H1");
    assert_rdc_fails("H1", &src, &["raw_rst", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_h2_single_reset_sync_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_h2_single_reset_sync_ok.arch")
        .expect("read H2");
    assert_rdc_ok("H2", &src);
}

#[test]
fn rdc_h3_reset_syncs_to_diff_domains_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_h3_reset_syncs_to_diff_domains_ok.arch")
        .expect("read H3");
    assert_rdc_ok("H3", &src);
}

#[test]
fn rdc_h4_reconvergent_three_syncs_same_domain_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_h4_reconvergent_three_syncs_same_domain_fail.arch")
        .expect("read H4");
    assert_rdc_fails("H4", &src, &["raw_rst", "sync_1", "Dst"]);
}

// ── Group J: reconvergent CDC (and mixed) — same generalised check ────────
// Same hazard shape as group H but with non-reset synchroniser kinds.

#[test]
fn rdc_j1_cdc_reconvergent_two_ff_syncs_same_domain_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_j1_cdc_reconvergent_two_ff_syncs_same_domain_fail.arch")
        .expect("read J1");
    assert_rdc_fails("J1", &src, &["CDC", "flag", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_j2_cdc_single_ff_sync_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_j2_cdc_single_ff_sync_ok.arch")
        .expect("read J2");
    assert_rdc_ok("J2", &src);
}

#[test]
fn rdc_j3_cdc_syncs_to_diff_domains_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_j3_cdc_syncs_to_diff_domains_ok.arch")
        .expect("read J3");
    assert_rdc_ok("J3", &src);
}

#[test]
fn rdc_j4_mixed_reset_and_data_sync_same_source_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_j4_mixed_reset_and_data_sync_same_source_same_domain_fail.arch")
        .expect("read J4");
    assert_rdc_fails("J4", &src, &["RDC/CDC", "shared", "Dst"]);
}

#[test]
fn package_width_qualified_param_emits_bracket_form() {
    // Regression: package-scoped `param NAME[hi:lo]: const = …;` must emit
    // `localparam [hi:lo] NAME = …;`, not `localparam int NAME = …;`.
    // The latter silently truncates values wider than 32 bits (Verilator
    // WIDTHTRUNC). Spec §29.1 + lines 1101–1105.
    let source = "
        package WidthPkg
          param NARROW: const = 42;
          param WIDE32[31:0]: const = 42;
          param WIDE64[63:0]: const = 24314014034;
        end package WidthPkg

        use WidthPkg;

        module M
          port o: out UInt<32>;
          comb
            o = NARROW;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // Untyped const stays `localparam int`.
    assert!(sv.contains("localparam int NARROW = 42;"),
            "untyped const must keep `int`:\n{sv}");
    // Width-qualified params must keep the [hi:lo] qualifier.
    assert!(sv.contains("localparam [31:0] WIDE32 = 42;"),
            "32-bit width qualifier dropped:\n{sv}");
    assert!(sv.contains("localparam [63:0] WIDE64 = 24314014034;"),
            "64-bit width qualifier dropped (would truncate):\n{sv}");
}

#[test]
fn package_enum_typed_param_emits_typedef_before_localparam() {
    // Regression: an EnumConst package param references its enum type,
    // which SV requires forward-declared. The package's `typedef enum`
    // must appear before the `localparam` that uses it.
    let source = "
        package OpPkg
          enum Op
            ADD,
            SUB,
          end enum Op
          param DEFAULT_OP: Op = Op::ADD;
        end package OpPkg

        use OpPkg;

        module M
          port o: out Op;
          comb
            o = DEFAULT_OP;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    let typedef_pos = sv.find("typedef enum")
        .expect("typedef enum missing");
    let param_pos = sv.find("DEFAULT_OP")
        .expect("DEFAULT_OP localparam missing");
    assert!(typedef_pos < param_pos,
        "enum typedef must precede localparam that references it:\n{sv}");
    assert!(sv.contains("localparam Op DEFAULT_OP = "),
        "EnumConst must emit typed `localparam Op …`:\n{sv}");
}

// ── Group K: Phase 2d — combiner-derived reset glitches at inst boundaries
// A sub-module Reset input wired by a combinational expression (rst_a | rst_b,
// not rst_a, etc.) sees glitches on edge skew and can trigger partial resets.

#[test]
fn rdc_k1_combiner_or_at_inst_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_k1_combiner_or_at_inst_fail.arch")
        .expect("read K1");
    assert_rdc_fails("K1", &src, &["sub", "rst", "combinational"]);
}

#[test]
fn rdc_k2_negation_at_inst_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_k2_negation_at_inst_fail.arch")
        .expect("read K2");
    assert_rdc_fails("K2", &src, &["sub", "rst", "combinational"]);
}

#[test]
fn rdc_k3_direct_reset_at_inst_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_k3_direct_reset_at_inst_ok.arch")
        .expect("read K3");
    assert_rdc_ok("K3", &src);
}

#[test]
fn rdc_k4_sync_output_to_reset_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_k4_sync_output_to_reset_ok.arch")
        .expect("read K4");
    assert_rdc_ok("K4", &src);
}

// ── Group M: Reconvergent CDC source-tracing (Aldec article 2140) ─────────
// The phase 2c reconvergence check walks each synchroniser's `data_in`
// expression through bit-slice / part-select / concat / unary+binary ops /
// ternary / let-binding indirection to find the *terminal* source
// register(s). Two synchronisers in the same destination clock domain whose
// inputs trace back to the same source — even via different combinational
// paths — produce a reconvergent-CDC violation.

#[test]
fn rdc_m1_cdc_bit_slice_same_source_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_m1_cdc_bit_slice_same_source_fail.arch")
        .expect("read M1");
    assert_rdc_fails("M1", &src, &["CDC", "flags", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_m2_cdc_part_select_same_source_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_m2_cdc_part_select_same_source_fail.arch")
        .expect("read M2");
    assert_rdc_fails("M2", &src, &["CDC", "word", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_m3_cdc_common_source_via_comb_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_m3_cdc_common_source_via_comb_fail.arch")
        .expect("read M3");
    assert_rdc_fails("M3", &src, &["CDC", "src_flag", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_m4_cdc_let_alias_same_source_fails() {
    let src = std::fs::read_to_string("tests/rdc/rdc_m4_cdc_let_alias_same_source_fail.arch")
        .expect("read M4");
    assert_rdc_fails("M4", &src, &["CDC", "src_flag", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_m5_cdc_distinct_sources_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_m5_cdc_distinct_sources_ok.arch")
        .expect("read M5");
    assert_rdc_ok("M5", &src);
}

#[test]
fn rdc_m6_cdc_bit_slice_distinct_vecs_ok() {
    let src = std::fs::read_to_string("tests/rdc/rdc_m6_cdc_bit_slice_distinct_vecs_ok.arch")
        .expect("read M6");
    assert_rdc_ok("M6", &src);
}

// ── Group L: Phase polish — `pragma rdc_safe;` per-module opt-out ─────────
// Mirror of `pragma cdc_safe;` for the RDC-specific phases. Either pragma
// alone suppresses phase 1 (the structural cross-clock rule). Phases 2a-2d
// are gated only by `rdc_safe`.

#[test]
fn rdc_l1_pragma_rdc_safe_suppresses_phase2a() {
    let src = std::fs::read_to_string("tests/rdc/rdc_l1_pragma_rdc_safe_suppresses_phase2a_ok.arch")
        .expect("read L1");
    assert_rdc_ok("L1", &src);
}

#[test]
fn rdc_l2_pragma_rdc_safe_suppresses_phase2c() {
    let src = std::fs::read_to_string("tests/rdc/rdc_l2_pragma_rdc_safe_suppresses_phase2c_ok.arch")
        .expect("read L2");
    assert_rdc_ok("L2", &src);
}

#[test]
fn rdc_l3_pragma_rdc_safe_suppresses_phase2d() {
    let src = std::fs::read_to_string("tests/rdc/rdc_l3_pragma_rdc_safe_suppresses_phase2d_ok.arch")
        .expect("read L3");
    assert_rdc_ok("L3", &src);
}

#[test]
fn rdc_l4_pragma_rdc_safe_suppresses_phase1() {
    let src = std::fs::read_to_string("tests/rdc/rdc_l4_pragma_rdc_safe_suppresses_phase1_ok.arch")
        .expect("read L4");
    assert_rdc_ok("L4", &src);
}

#[test]
fn rdc_l5_unknown_pragma_rejected() {
    // Defensive: a typo or unknown pragma name still errors at parse
    // time, so users notice when they mistype `rdc_safe`.
    let src = r#"
domain D
  freq_mhz: 100
end domain D
module M
  pragma totally_unsafe;
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port d:   in UInt<8>;
  port q:   out UInt<8>;
  reg r: UInt<8> reset rst => 0;
  seq on clk rising
    r <= d;
  end seq
  let q = r;
end module M
"#;
    let tokens = lexer::tokenize(src).expect("lex");
    let mut parser = Parser::new(tokens, src);
    let result = parser.parse_source_file();
    assert!(result.is_err(), "unknown pragma should be a parse error");
    let msg = format!("{:?}", result.err().unwrap());
    assert!(msg.contains("unknown pragma") && msg.contains("totally_unsafe"),
        "expected unknown-pragma diagnostic, got: {msg}");
}

#[test]
fn test_archi_interface_stub_skips_body_only_passes() {
    // Mimics the multi-file dep-loader case: a parent module instantiates
    // a child whose source came from a `.archi` interface stub (port-only,
    // no body). Pre-fix, typecheck reported "output port `out_o` is not
    // driven" because `check_module` ran the body-driven check on the
    // empty stub. Codegen would also emit a duplicate empty `module
    // ChildStub` clashing with the real SV at link time. With the
    // `is_interface` flag set (post-parse, normally done in main.rs from
    // the source-file extension), both passes skip the stub.
    let source = r#"
domain Sys
  freq_mhz: 100
end domain Sys

module ChildStub
  port clk_i: in Clock<Sys>;
  port rst_ni: in Reset<Async, Low>;
  port in_i: in UInt<8>;
  port out_o: out UInt<8>;
end module ChildStub

module Parent
  port clk_i: in Clock<Sys>;
  port rst_ni: in Reset<Async, Low>;
  port result: out UInt<8>;

  wire w: UInt<8>;
  inst c: ChildStub
    clk_i <- clk_i;
    rst_ni <- rst_ni;
    in_i <- 8'd0;
    out_o -> w;
  end inst c

  let result = w;
end module Parent
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let mut parsed_ast = parser.parse_source_file().expect("parse");
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name == "ChildStub" { m.is_interface = true; }
        }
    }
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target lowering");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator lowering");
    let ast = elaborate::lower_threads_with_opts(ast, &elaborate::ThreadLowerOpts::default())
        .expect("lower_threads");
    let ast = elaborate::lower_pipe_reg_ports(ast).expect("lower_pipe_reg_ports");
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit_channel dispatch");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check()
        .expect("typecheck must not report 'output port out_o is not driven' on interface stub");
    let codegen = Codegen::new(&symbols, &ast, overload_map);
    let sv = codegen.generate();
    assert!(sv.contains("module Parent"), "parent module should be emitted");
    assert!(!sv.contains("module ChildStub"),
        "interface stub must not be emitted to SV (real impl lives in a separately-built file)");
}
