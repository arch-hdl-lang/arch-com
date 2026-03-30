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
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let codegen = Codegen::new(&symbols, &ast, overload_map);
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
    let source = include_str!("let_bindings.arch");
    let sv = compile_to_sv(source);
    // Typed let: emits declared type then a separate assign
    assert!(sv.contains("logic [8-1:0] mask;"), "expected typed let decl, got:\n{sv}");
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
    let source = include_str!("sync_fifo.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("$clog2(DEPTH)"));
    assert!(sv.contains("assign full"));
    assert!(sv.contains("assign empty"));
    assert!(sv.contains("assign push_ready"));
    assert!(sv.contains("assign pop_valid"));
    assert!(sv.contains("always_ff @(posedge clk)"));
    assert!(sv.contains("parameter type TYPE"));
    assert!(sv.contains("TYPE                  mem [0:DEPTH-1]"));
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
    let source = include_str!("lifo.arch");
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

#[test]
fn test_arbiter_custom_hook() {
    let source = include_str!("arbiter_custom_hook.arch");
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
    let source = include_str!("arbiter_latency2.arch");
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
    let source = include_str!("template_basic.arch");
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
  comb other = 0;
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
    let source = include_str!("int_regs.arch");
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
  freq_mhz: 100
end domain SysDomain

ram BadRam
  kind single;
  latency 1;
  param DEPTH: const = 64;
  param WIDTH: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  store
    data: Vec<WIDTH, DEPTH>;
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
  reg count_r: UInt<8> init 0 reset rst=0;
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
    let source = include_str!("generate_for.arch");
    let sv = compile_to_sv(source);
    // After elaboration, generate for 0..1 should expand to 2 ports each
    assert!(sv.contains("req_0"), "expected req_0 port, got:\n{sv}");
    assert!(sv.contains("req_1"), "expected req_1 port, got:\n{sv}");
    assert!(sv.contains("gnt_0"), "expected gnt_0 port, got:\n{sv}");
    assert!(sv.contains("gnt_1"), "expected gnt_1 port, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_generate_if_true() {
    let source = include_str!("generate_if.arch");
    let sv = compile_to_sv(source);
    // generate if true → debug_out port is included
    assert!(sv.contains("debug_out"), "expected debug_out port, got:\n{sv}");
    insta::assert_snapshot!(sv);
}

#[test]
fn test_generate_if_param_default_true() {
    // generate if using a param default value of 1 → port included
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module ParamDebug
  param ENABLE_DEBUG: const = 1;
  port clk: in Clock<SysDomain>;

  generate if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate if

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
    // generate if PARAM where PARAM default = 0 should exclude
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module NoDebug2
  param ENABLE_DEBUG: const = 0;
  port clk: in Clock<SysDomain>;

  generate if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate if

  comb
  end comb
end module NoDebug2
"#;
    let sv = compile_to_sv(source);
    assert!(!sv.contains("debug_out"), "debug_out should be excluded when ENABLE_DEBUG=0, got:\n{sv}");
}

#[test]
fn test_generate_if_param_comparison() {
    // generate if PARAM > 0 style condition
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module CmpDebug
  param LOG_LEVEL: const = 2;
  port clk: in Clock<SysDomain>;

  generate if LOG_LEVEL > 1
    port verbose_out: out UInt<8>;
  end generate if

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

  generate if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate if

  comb
    debug_out = 0;
  end comb
end module Inner

module Outer
  port clk: in Clock<SysDomain>;
  port out_dbg: out UInt<8>;

  inst inner: Inner
    param ENABLE_DEBUG = 1;
    connect clk <- clk;
    connect debug_out -> out_dbg;
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

  generate if ENABLE_DEBUG
    port debug_out: out UInt<8>;
  end generate if

  comb
  end comb
end module Inner2

module Outer2
  port clk: in Clock<SysDomain>;

  inst inner2: Inner2
    param ENABLE_DEBUG = 0;
    connect clk <- clk;
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
    connect clk <- clk;
    connect result -> out_a;
  end inst sub_on

  inst sub_off: Sub
    param ENABLE = 0;
    connect clk <- clk;
    connect result -> out_b;
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
    // DIFFERENT port lists via `generate if`.  Uses a conditional INPUT port
    // so the module's comb block doesn't need to reference non-existent ports.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Inner
  param ENABLE_DEBUG: const = 0;
  port clk: in Clock<SysDomain>;
  port result: out Bool;

  generate if ENABLE_DEBUG
    port debug_in: in UInt<8>;
  end generate if

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
    connect clk <- clk;
    connect result -> out_a;
    connect debug_in <- dbg_val;
  end inst inner_on

  inst inner_off: Inner
    param ENABLE_DEBUG = 0;
    connect clk <- clk;
    connect result -> out_b;
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
  generate if false
    port debug_out: out UInt<8>;
  end generate if
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

  reg count_r: UInt<8> init 0 reset rst=0;
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

  reg reg_a: UInt<8> init 0 reset rst_a=0;
  reg reg_b: UInt<8> init 0 reset rst_b=0;

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

  reg reg_a: UInt<8> init 0 reset rst=0;
  reg reg_b: UInt<8> init 0 reset rst=0 Async high;

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
    let source = include_str!("simple_pipeline.arch");
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
    reg captured: UInt<8> init 0 reset rst=0;
    seq on clk rising
      captured <= data_in;
    end seq
  end stage Fetch

  stage Writeback
    reg result: UInt<8> init 0 reset rst=0;
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
    let source = include_str!("cpu_pipeline.arch");
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

  reg opcode_r: UInt<7> init 0 reset rst=0;
  reg rd_r: UInt<5> init 0 reset rst=0;
  reg funct3_r: UInt<3> init 0 reset rst=0;

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
    reg captured: UInt<XLEN> init 0 reset rst=0;
    seq on clk rising
      captured <= data_in;
    end seq
  end stage Fetch

  stage Writeback
    reg result: UInt<XLEN> init 0 reset rst=0;
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
    connect clk <- clk;
    connect rst <- rst;
    connect data_in <- din;
    connect data_out -> dout;
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
  param WIDTH: const = 32;
  port a: in UInt<WIDTH>;
  port b: in UInt<WIDTH>;
  port result: out UInt<WIDTH>;

  comb
    result = (a + b).trunc<WIDTH>();
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
    reg a_r: UInt<XLEN> init 0 reset rst=0;
    reg b_r: UInt<XLEN> init 0 reset rst=0;
    seq on clk rising
      a_r <= op_a;
      b_r <= op_b;
    end seq
  end stage Fetch

  stage Execute
    reg alu_out: UInt<XLEN> init 0 reset rst=0;
    seq on clk rising
      alu_out <= (Fetch.a_r + Fetch.b_r).trunc<XLEN>();
    end seq
    inst alu0: Alu
      connect a <- Fetch.a_r;
      connect b <- Fetch.b_r;
      connect result -> result_out;
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

  reg wr_r: UInt<$clog2(DEPTH)> init 0 reset rst=0;
  reg rd_r: UInt<$clog2(DEPTH)> init 0 reset rst=0;

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
    let source = std::fs::read_to_string("tests/pkt_queue.arch")
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
    let source = include_str!("pipe_reg_test.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("delayed_stg1"));
    assert!(sv.contains("delayed_stg2"));
    assert!(sv.contains("delayed <= delayed_stg2"));
    assert!(sv.contains("always_ff"));
    insta::assert_snapshot!(sv);
}
