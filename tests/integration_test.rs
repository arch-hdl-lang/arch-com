use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

fn compile_to_sv(source: &str) -> String {
    compile_to_sv_with_opts(source, &elaborate::ThreadLowerOpts::default())
}

/// Strip every `// synopsys translate_off ... // synopsys translate_on`
/// block from emitted SV. Used by structural equivalence tests that
/// compare a hand-rolled arbiter (no HandshakeMeta → no auto-SVA)
/// against the `handshake_channel` form (HandshakeMeta present → Tier-2
/// SVA emitted). Both forms must match in port + arbiter-logic shape;
/// the SVA-only delta is expected. Applied to both sides so any
/// incidental translate-off blocks (auto-bounds, etc.) cancel out.
fn strip_auto_handshake_sva(sv: &str) -> String {
    let lines: Vec<&str> = sv.lines().collect();
    let mut out = String::with_capacity(sv.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed == "// synopsys translate_off" {
            // Skip the entire block (translate_off..translate_on).
            while i < lines.len()
                && lines[i].trim_start() != "// synopsys translate_on"
            {
                i += 1;
            }
            // Consume the translate_on line too.
            if i < lines.len() { i += 1; }
            // Retroactively swallow exactly one leading blank-ish line
            // (whitespace only — `Codegen::line("")` at indent>0 emits a
            // fully-indented empty line). The corresponding leading
            // blank is emitted by the auto-handshake helper just before
            // the block; the trailing blank lives outside the helper.
            let bytes = out.as_bytes();
            if !bytes.is_empty() && bytes.last() == Some(&b'\n') {
                let without_nl = &out[..out.len() - 1];
                let last_nl = without_nl.rfind('\n').map(|p| p + 1).unwrap_or(0);
                let last_line = &without_nl[last_nl..];
                if last_line.trim().is_empty() && !without_nl.is_empty() {
                    out.truncate(last_nl);
                }
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
        i += 1;
    }
    out
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
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
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

#[test]
fn test_fsm_port_named_state_errors() {
    // `state` is reserved in fsm — the codegen maps it to state_r.
    // A user port named `state` would collide.
    let source = r#"
fsm BadFsm
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port state: out UInt<2>;
  state [A, B]
  default state A;
  state A
    comb
      state = 2'd0;
    end comb
    -> B when true;
  end state A
  state B
    comb
      state = 2'd1;
    end comb
    -> A when true;
  end state B
end fsm BadFsm
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "fsm with port named 'state' should error");
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
fn test_rom_lut_type_param_ports_lower_to_data_width() {
    let source = r#"
domain CoreDomain
  freq_mhz: 100
end domain CoreDomain

ram TypedRom
  kind rom;
  latency 1;
  init: [0x00000000, 0x00000001, 0x00000002, 0x00000003];

  param DEPTH: const = 4;
  param T: type = UInt<32>;

  port clk: in Clock<CoreDomain>;

  store
    data: Vec<T, DEPTH>;
  end store

  ports rd
    addr: in UInt<2>;
    en:   in Bool;
    data: out T;
  end ports rd
end ram TypedRom
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("parameter int DATA_WIDTH = 32"));
    assert!(sv.contains("output logic [DATA_WIDTH-1:0] rd_data"));
    assert!(sv.contains("logic [DATA_WIDTH-1:0] mem [0:DEPTH-1]"));
    assert!(!sv.contains("output T rd_data"));
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
    assert!(sv.contains("QosGrant(request_valid, last_grant_r, qos_in)"));
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
fn test_arbiter_hook_param_shadows_port_errors() {
    // Hook parameter names must not shadow arbiter port names — codegen
    // emits the function inside the module, causing SV VARHIDDEN.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter BadArb
  policy ShadowFn;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port age: in UInt<8>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
  hook grant_select(req_mask: UInt<4>, age: UInt<8>) -> UInt<4>
    = ShadowFn(req_mask, age);
end arbiter BadArb
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected error for hook param shadowing port");
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

// ── Arbiter handshake_channel port-list desugaring ───────────────────────────

/// `handshake_channel name[N]: receive kind: valid_ready` in an arbiter
/// port list should desugar to the same `ports[N] name { valid: in Bool;
/// ready: out Bool; }` shape that arbiters use today. The emitted SV must
/// be byte-identical to the hand-rolled equivalent (matching
/// examples/bus_arbiter.sv shape).
#[test]
fn test_arbiter_handshake_channel_port_shape() {
    let hand_rolled = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbA
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter HsArbA
"#;
    let hs_form = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbA
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  handshake_channel request[NUM_REQ]: receive kind: valid_ready
  end handshake_channel request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter HsArbA
"#;
    let sv_h = compile_to_sv(hand_rolled);
    let sv_n = compile_to_sv(hs_form);
    // The `handshake_channel` form now additionally emits Tier-2 protocol
    // SVA (the hand-rolled `ports[N]` form has no HandshakeMeta so emits
    // nothing). Compare structural SV with the auto-SVA block elided so
    // the port-shape equivalence claim still holds.
    assert_eq!(strip_auto_handshake_sva(&sv_h), strip_auto_handshake_sva(&sv_n),
        "handshake_channel array port shape must match hand-rolled ports[N] form (modulo Tier-2 SVA)");
    // Sanity: ensure we actually went through the new path by checking shape.
    assert!(sv_n.contains("input logic [NUM_REQ-1:0] request_valid"));
    assert!(sv_n.contains("output logic [NUM_REQ-1:0] request_ready"));
}

/// A handshake_channel with a payload field should expand its payload to
/// a parallel array port alongside valid/ready, all of `[N]` width.
#[test]
fn test_arbiter_handshake_channel_with_payload() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbB
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  handshake_channel request[NUM_REQ]: receive kind: valid_ready
    qos: UInt<3>;
  end handshake_channel request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter HsArbB
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("input logic [NUM_REQ-1:0] request_valid"),
        "expected request_valid array port:\n{sv}");
    assert!(sv.contains("output logic [NUM_REQ-1:0] request_ready"),
        "expected request_ready array port:\n{sv}");
    // Payload flows in the same direction as `receive` (in to the arbiter).
    // Width comes from the field type; SV declares one wire per index slot.
    assert!(sv.contains("request_qos"),
        "expected request_qos payload port:\n{sv}");
}

// ── Tier-2 auto-SVA for arbiter handshake_channel ────────────────────────────

/// A `valid_ready` `handshake_channel[N]` in an arbiter port list must
/// emit the same `valid_stable` SVA the bus side emits, vectorized once
/// per request lane via an SV `generate for (genvar i ...)` block. The
/// block must be wrapped in `synopsys translate_off / on` so synthesis
/// tools elide it. Mirrors `test_handshake_tier2_valid_ready_assertion`.
#[test]
fn test_arbiter_handshake_channel_array_emits_sva_generate() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbSvaArr
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  handshake_channel request[NUM_REQ]: receive kind: valid_ready
  end handshake_channel request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter HsArbSvaArr
"#;
    let sv = compile_to_sv(source);
    // Wrapper + structural shape.
    assert!(sv.contains("// synopsys translate_off"),
        "expected translate_off wrapper:\n{sv}");
    assert!(sv.contains("// synopsys translate_on"),
        "expected translate_on wrapper:\n{sv}");
    assert!(sv.contains("// Auto-generated handshake protocol assertions"),
        "expected Tier-2 header comment:\n{sv}");
    assert!(sv.contains("generate for (genvar i = 0; i < NUM_REQ; i++) begin: g_auto_hs_request"),
        "expected genvar-indexed generate block over NUM_REQ:\n{sv}");
    assert!(sv.contains("end endgenerate"),
        "expected generate block close:\n{sv}");
    // Property uses lane-indexed signals + disable iff (rst) + the same
    // `(v && !r) |=> v` predicate as the bus-side emitter.
    assert!(sv.contains("_auto_hs_request__lane_valid_stable"),
        "expected per-lane valid_stable label:\n{sv}");
    assert!(sv.contains("disable iff (rst)"),
        "expected reset-disable clause:\n{sv}");
    assert!(sv.contains("(request_valid[i] && !request_ready[i]) |=> request_valid[i]"),
        "expected lane-indexed valid-stable predicate:\n{sv}");
}

/// A non-array `handshake_channel` (no `[N]` shape, e.g. an arbiter's
/// `grant` output) must emit the SVA at the top level — *not* wrapped
/// in a `generate for` block — because there's only one channel
/// instance. The bare-signal form is also what the bus-side path
/// emits today.
#[test]
fn test_arbiter_handshake_channel_non_array_emits_sva_bare() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbSvaBare
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  handshake_channel grant: send kind: valid_ready
    requester: UInt<2>;
  end handshake_channel grant
end arbiter HsArbSvaBare
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("_auto_hs_grant_valid_stable"),
        "expected bare grant valid_stable label:\n{sv}");
    // Crucially, no generate-for wrapper for the non-array channel.
    assert!(!sv.contains("g_auto_hs_grant"),
        "non-array handshake_channel must not be wrapped in generate-for:\n{sv}");
    // And the predicate uses unindexed signal names.
    assert!(sv.contains("(grant_valid && !grant_ready) |=> grant_valid"),
        "expected unindexed grant valid-stable predicate:\n{sv}");
}

/// A `valid_only` `handshake_channel` has no ready signal to gate
/// stability on. The bus-side Tier-2 emitter (current v1) emits nothing
/// for `valid_only`; the arbiter path must mirror that — no
/// `valid_stable` property, no `_auto_hs_*` label for the channel.
#[test]
fn test_arbiter_handshake_channel_valid_only_omits_ready_gate() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbSvaVOnly
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  handshake_channel grant: send kind: valid_only
    requester: UInt<2>;
  end handshake_channel grant
end arbiter HsArbSvaVOnly
"#;
    let sv = compile_to_sv(source);
    // No SVA label for `grant` should appear — mirrors the bus side's
    // v1 silent-skip for variants without a back-signal.
    assert!(!sv.contains("_auto_hs_grant"),
        "valid_only handshake_channel must not emit Tier-2 SVA:\n{sv}");
    // The Tier-2 wrapper itself must be elided too when no channel
    // produced any property (matches bus-side emit_handshake_asserts).
    assert!(!sv.contains("Auto-generated handshake protocol assertions"),
        "Tier-2 wrapper must not be emitted when no property applies:\n{sv}");
}

/// A `valid_ready` `handshake_channel` declared *without* any payload
/// fields must still emit the control-signal `valid_stable` property
/// (the protocol invariant binds on valid/ready alone). This mirrors
/// the bus side: Tier-2 v1 emits *only* the control-signal property —
/// it never emits per-payload `$stable` checks, regardless of whether
/// payload fields are present. The test pins that contract: payload
/// presence/absence doesn't change the emitted SVA shape, only the
/// declared port set.
#[test]
fn test_arbiter_handshake_channel_no_payload_emits_only_valid_stability() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbSvaNoPL
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  handshake_channel request[NUM_REQ]: receive kind: valid_ready
  end handshake_channel request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter HsArbSvaNoPL
"#;
    let sv = compile_to_sv(source);
    // Exactly one property — valid_stable — for the channel, lane-indexed.
    assert!(sv.contains("_auto_hs_request__lane_valid_stable"),
        "expected valid_stable property:\n{sv}");
    // No `$stable` payload-stability check (Tier-2 v1 scope explicitly
    // doesn't include payload-stability, matching bus-side behaviour).
    assert!(!sv.contains("$stable"),
        "Tier-2 v1 must not emit payload-stability $stable checks:\n{sv}");
}

/// Regression: the bus-side handshake_channel Tier-2 SVA path is
/// unaffected by the arbiter-side addition. The shared helper still
/// produces the exact same label / predicate / wrapper text the
/// dedicated bus path produced before the refactor.
#[test]
fn test_existing_bus_handshake_sva_unaffected() {
    // Same source as test_handshake_tier2_valid_ready_assertion.
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
    // Same checks as the original bus-side test.
    assert!(sv.contains("// Auto-generated handshake protocol assertions"));
    assert!(sv.contains("_auto_hs_bus_p_aw_valid_stable"));
    assert!(sv.contains("(bus_p_aw_valid && !bus_p_aw_ready) |=> bus_p_aw_valid"));
    assert!(sv.contains("disable iff (rst)"));
    assert!(sv.contains("synopsys translate_off"));
    assert!(sv.contains("synopsys translate_on"));
    // Bus path is non-array, so no generate-for wrapper.
    assert!(!sv.contains("g_auto_hs_aw"),
        "bus-path Tier-2 SVA must remain non-generate-wrapped:\n{sv}");
}

// ── (Existing) handshake_channel port-shape tests ────────────────────────────

/// A `valid_only` handshake_channel as a non-array port should expand to
/// just the valid wire + payload wires, both at the top level (no array
/// shape). This is the grant-output shape: `grant_valid + grant_<f>`.
#[test]
fn test_arbiter_handshake_channel_grant_output() {
    let hand_rolled = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbC
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter HsArbC
"#;
    let hs_form = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter HsArbC
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  handshake_channel request[NUM_REQ]: receive kind: valid_ready
  end handshake_channel request
  handshake_channel grant: send kind: valid_only
    requester: UInt<2>;
  end handshake_channel grant
end arbiter HsArbC
"#;
    let sv_h = compile_to_sv(hand_rolled);
    let sv_n = compile_to_sv(hs_form);
    // Same caveat as test_arbiter_handshake_channel_port_shape: the
    // handshake_channel form now emits Tier-2 SVA the hand-rolled form
    // can't see (no HandshakeMeta). Strip it before structural compare.
    assert_eq!(strip_auto_handshake_sva(&sv_h), strip_auto_handshake_sva(&sv_n),
        "valid_only handshake_channel + receive valid_ready array must match hand-rolled shape (modulo Tier-2 SVA)");
    assert!(sv_n.contains("output logic grant_valid"),
        "expected grant_valid top-level port:\n{sv_n}");
    assert!(sv_n.contains("output logic [2-1:0] grant_requester")
        || sv_n.contains("output logic [1:0] grant_requester"),
        "expected grant_requester top-level port:\n{sv_n}");
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
fn test_pipeline_stage_inst_output_wire_type_resolved_from_port() {
    // Regression: an inst inside a pipeline stage whose output
    // destination is consumed only cross-stage (no same-stage reg
    // RHS reference) needs its wire type resolved from the
    // instantiated module's port declaration, with the inst's
    // param assignments substituted in. Previously the wire was
    // declared as bare `logic` (1-bit), causing Verilator
    // WIDTHEXPAND warnings or width mismatches at the inst
    // instantiation site.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Sub
  param WIDTH: const = 32;
  port a_in: in UInt<WIDTH>;
  port b_out: out UInt<WIDTH>;
  comb
    b_out = a_in;
  end comb
end module Sub

pipeline P
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port src: in UInt<32>;
  port dst: out UInt<32>;

  stage S1
    reg latched: UInt<32> reset rst => 0;
    seq on clk rising
      latched <= src;
    end seq
    inst sub: Sub
      param WIDTH = 32;
      a_in -> sub_in;
      b_out -> sub_out;
    end inst sub
    comb
      sub_in = latched;
    end comb
  end stage S1

  stage S2
    reg captured: UInt<32> reset rst => 0;
    seq on clk rising
      captured <= S1.sub_out;
    end seq
    comb
      dst = captured;
    end comb
  end stage S2

end pipeline P
"#;
    let sv = compile_to_sv(source);
    // The inst's output wire `s1_sub_out` must be sized 32 bits,
    // resolved from `Sub.b_out: out UInt<WIDTH>` with WIDTH=32
    // substituted from the inst's param_assigns.
    assert!(
        sv.contains("logic [31:0] s1_sub_out"),
        "inst output wire should be 32-bit: {sv}"
    );
    assert!(
        !sv.contains("logic s1_sub_out;"),
        "inst output wire should NOT be bare 1-bit logic"
    );
    insta::assert_snapshot!(sv);
}

#[test]
fn test_pipeline_inst_module_dep_auto_resolved() {
    // Regression: a `pipeline` containing `inst <ExternalModule>`
    // (where ExternalModule is defined in a sibling .arch / .archi)
    // must trigger the same auto-dependency walk that
    // `module + inst` does. Previously the dep walker matched only
    // Item::Module so the .archi/.arch lookup never fired and the
    // build failed with `undefined name`.
    //
    // We test this end-to-end by having a single-file source with
    // both the sub-module and the pipeline declared, exercising the
    // resolve path through self.symbols.globals (which is what the
    // wire-type resolution and dep-walker share). A multi-file test
    // would require a real on-disk fixture; this exercises the same
    // codegen path.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Helper
  port clk_i: in Clock<SysDomain>;
  port rst_ni: in Reset<Sync, Low>;
  port d_in: in UInt<16>;
  port q_out: out UInt<16>;
  reg r: UInt<16> reset rst_ni => 0;
  seq on clk_i rising
    r <= d_in;
  end seq
  comb
    q_out = r;
  end comb
end module Helper

pipeline P2
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, Low>;
  port src: in UInt<16>;
  port dst: out UInt<16>;

  stage Only
    reg latched: UInt<16> reset rst => 0;
    seq on clk rising
      latched <= src;
    end seq
    inst h: Helper
      clk_i  <- clk;
      rst_ni <- rst;
      d_in   <- latched;
      q_out  -> h_out;
    end inst h
    comb
      dst = h_out;
    end comb
  end stage Only

end pipeline P2
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("module Helper"), "Helper module should emit");
    assert!(sv.contains("Helper h ("), "Helper inst should emit inside pipeline stage");
    assert!(
        sv.contains("logic [15:0] only_h_out"),
        "inst output wire should be 16-bit per Helper.q_out"
    );
}

#[test]
fn test_qualified_enum_param_for_external_package() {
    // Native ARCH syntax for forwarding upstream-SV typed params:
    // `param NAME: pkg::EnumName = pkg::Variant;` — the qualified
    // path on both type and default value emits unchanged into SV
    // so the param can be wired across an ARCH-to-upstream-SV
    // module boundary without a cast (e.g. ARCH IbexCore forwarding
    // RV32M to upstream-SV ibex_cs_registers, both seeing
    // `ibex_pkg::rv32m_e`). The qualified enum side isn't a known
    // ARCH enum, so codegen preserves the `pkg::Variant` form
    // verbatim instead of uppercasing the variant.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module ExternalEnumParam
  param RV32M: ibex_pkg::rv32m_e = ibex_pkg::RV32MFast;
  param RV32B: ibex_pkg::rv32b_e = ibex_pkg::RV32BNone;

  port clk_i:  in Clock<SysDomain>;
  port rst_ni: in Reset<Sync, Low>;
  port d:      in UInt<32>;
  port q:      out UInt<32>;

  comb
    q = d;
  end comb
end module ExternalEnumParam
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("parameter ibex_pkg::rv32m_e RV32M = ibex_pkg::RV32MFast"),
        "qualified enum type + default should emit verbatim: {sv}"
    );
    assert!(
        sv.contains("parameter ibex_pkg::rv32b_e RV32B = ibex_pkg::RV32BNone"),
        "second qualified-enum param should also emit verbatim"
    );
    insta::assert_snapshot!(sv);
}

#[test]
fn test_unpacked_array_param_for_external_struct() {
    // Native ARCH syntax for a `parameter <type> NAME [N] = <ident>;`
    // upstream-SV unpacked-array param. Three shapes covered here:
    //  - struct + unpacked dim:    `pkg::T [N]`
    //  - logic-packed + unpacked dim: `UInt<W> [N]`
    //  - scalar struct (no dim, exercise back-compat).
    // All three forward upstream-SV `pkg::Variant` defaults verbatim.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module ExtUnpackedParams
  param PMPRstCfg:     ibex_pkg::pmp_cfg_t [16]   = ibex_pkg::PmpCfgRst;
  param PMPRstAddr:    UInt<34> [16]              = ibex_pkg::PmpAddrRst;
  param PMPRstMsecCfg: ibex_pkg::pmp_mseccfg_t    = ibex_pkg::PmpMseccfgRst;

  port clk_i:  in Clock<SysDomain>;
  port rst_ni: in Reset<Sync, Low>;
  port d:      in UInt<32>;
  port q:      out UInt<32>;

  comb
    q = d;
  end comb
end module ExtUnpackedParams
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("parameter ibex_pkg::pmp_cfg_t PMPRstCfg [16] = ibex_pkg::PmpCfgRst"),
        "struct-typed unpacked-array param should emit verbatim with [N] post-name dim: {sv}"
    );
    assert!(
        sv.contains("parameter [33:0] PMPRstAddr [16] = ibex_pkg::PmpAddrRst"),
        "width-qualified unpacked-array param should emit `[hi:lo] NAME [N] = ...`"
    );
    assert!(
        sv.contains("parameter ibex_pkg::pmp_mseccfg_t PMPRstMsecCfg = ibex_pkg::PmpMseccfgRst"),
        "scalar struct param (no unpacked dim) should still work after the syntax extension"
    );
    insta::assert_snapshot!(sv);
}

#[test]
fn test_pipeline_stage_inst_in_wait_until() {
    // Regression: a `wait until` condition inside a stage that also
    // hosts an `inst` block must reference the inst's output via the
    // stage-prefixed wire name (the same name the inst lowering
    // emits), not the bare local name. Previously the wait FSM
    // codegen used the cross-stage `emit_pipeline_expr_str`, which
    // did not apply the current stage's prefix to local idents,
    // emitting a dangling `worker_done` reference that Verilator
    // rejected with "Can't find definition of variable".
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Worker
  port clk_i: in Clock<SysDomain>;
  port rst_ni: in Reset<Sync, Low>;
  port req_i: in Bool;
  port done_o: out Bool;
  reg state_q: UInt<2> reset rst_ni => 0;
  seq on clk_i rising
    if state_q == 2'd0
      if req_i
        state_q <= 2'd1;
      end if
    else
      if state_q == 2'd2
        state_q <= 2'd0;
      else
        state_q <= (state_q + 1).trunc<2>();
      end if
    end if
  end seq
  comb
    done_o = state_q == 2'd2;
  end comb
end module Worker

pipeline WorkerPipe
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, Low>;
  port go: in Bool;
  port out_valid: out Bool;

  stage Capture
    reg seen: Bool reset rst => false;
    seq on clk rising
      seen <= go;
    end seq
  end stage Capture

  stage Process
    reg result: Bool reset rst => false;
    seq on clk rising
      wait until worker_done;
      result <= true;
    end seq
    inst worker: Worker
      clk_i <- clk;
      rst_ni <- rst;
      req_i <- Capture.seen;
      done_o -> worker_done;
    end inst worker
    comb
      out_valid = result;
    end comb
  end stage Process

end pipeline WorkerPipe
"#;
    let sv = compile_to_sv(source);
    // The `wait until worker_done` inside Process stage must resolve
    // to the stage-prefixed wire name. Both the `inst` connection
    // and the wait-FSM condition must agree on `process_worker_done`.
    assert!(
        sv.contains(".done_o(process_worker_done)"),
        "inst output connection should target stage-prefixed wire"
    );
    // The bare unprefixed name must NOT appear as a free variable
    // reference (it would cause Verilator UNDEFINED). The prefixed
    // form `process_worker_done` is the only legal reference.
    assert!(
        !sv.contains("if (worker_done)"),
        "wait condition must use stage-prefixed reference, not bare ident"
    );
    assert!(
        sv.contains("if (process_worker_done)"),
        "wait condition must reference the stage-prefixed inst output wire"
    );
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
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
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

/// Regression for issue #277: when a struct literal is emitted as a positional
/// concatenation `{a, b}` (the form chosen so iverilog accepts struct-literal
/// RHS in continuous-assignment contexts), each numeric-literal field value
/// must be sized to the field's declared width. Bare unsized `0` inside a
/// concat is illegal per IEEE 1800 §11.4.12 and Verilator rejects it with
/// `WIDTHCONCAT`.
#[test]
fn test_struct_literal_concat_sizes_field_literals() {
    let source = r#"
struct PriorityReg
  value:    UInt<3>;
  reserved: UInt<29>;
end struct PriorityReg

module ReproStruct
  port clk: in Clock<MyDomain>;
  port rst: in Reset<Async, Low>;
  port out_data: out Vec<PriorityReg, 2>;

  reg priority_r: Vec<PriorityReg, 2> reset rst => PriorityReg { value: 0, reserved: 0 };

  default seq on clk rising;
  seq
    out_data[0] <= priority_r[0];
    out_data[1] <= priority_r[1];
  end seq

end module ReproStruct
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("{3'd0, 29'd0}"),
        "expected sized literals `{{3'd0, 29'd0}}` in struct-literal concat (issue #277), got:\n{sv}"
    );
    assert!(
        !sv.contains("{0, 0}"),
        "unsized `{{0, 0}}` is illegal inside an SV concat (Verilator WIDTHCONCAT); got:\n{sv}"
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

/// `extern package` declares opaque types from an SV-side package.
/// `use Pkg;` with an extern package emits `import Pkg::*;` and codegen
/// drops the `Pkg::` qualifier from enum variant references.
#[test]
fn test_extern_package_emits_sv_import_and_bare_names() {
    let source = r#"
extern package ibex_pkg
  type rv32m_e;
  type rv32b_e;
end extern package ibex_pkg

use ibex_pkg;

module IbexCore
  param RV32M: rv32m_e = rv32m_e::RV32MFast;
  param RV32B: rv32b_e = rv32b_e::RV32BNone;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
end module IbexCore
"#;
    let sv = compile_to_sv(source);
    // extern packages emit per-TYPE imports (`import Pkg::T;`)
    // rather than wildcard `import Pkg::*;` so unrelated enum
    // items / parameters from the upstream SV package don't pollute
    // the compilation unit and conflict with locally-named signals.
    assert!(sv.contains("import ibex_pkg::rv32m_e;"),
            "expected SV `import ibex_pkg::rv32m_e;` for extern package:\n{sv}");
    assert!(sv.contains("import ibex_pkg::rv32b_e;"),
            "expected SV `import ibex_pkg::rv32b_e;` for extern package:\n{sv}");
    assert!(!sv.contains("import ibex_pkg::*;"),
            "extern packages must NOT emit wildcard `import ibex_pkg::*;`:\n{sv}");
    assert!(sv.contains("parameter rv32m_e RV32M = RV32MFast"),
            "expected bare `rv32m_e` type and `RV32MFast` variant:\n{sv}");
    assert!(sv.contains("parameter rv32b_e RV32B = RV32BNone"),
            "expected bare `rv32b_e` type and `RV32BNone` variant:\n{sv}");
    // No extern package body should be emitted (SV package lives upstream).
    assert!(!sv.contains("extern package") && !sv.contains("endpackage"),
            "extern package must not emit SV package body:\n{sv}");
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

fn compile_to_thread_sim_h(source: &str) -> String {
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target lowering");
    let ast = arch::elaborate::lower_tlm_initiator_calls(ast).expect("tlm initiator lowering");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("lower pipe_reg error");
    let ast = arch::elaborate::lower_credit_channel_dispatch(ast).expect("cc dispatch error");
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    checker.check().expect("type check error");

    ast.items.iter()
        .filter_map(|item| match item {
            arch::ast::Item::Module(m)
                if m.body.iter().any(|i| matches!(i, arch::ast::ModuleBodyItem::Thread(_))) =>
            {
                Some(arch::sim_codegen::thread_sim::gen_module_thread(m, false, false, 1)
                    .expect("thread sim codegen"))
            }
            _ => None,
        })
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
fn test_vec_of_bus_port_flattens_to_n_indexed_copies() {
    // `port chans: initiator Vec<BusName, N>;` declares N copies of the bus
    // on the module signature. SV codegen emits `chans_0_<sig>`, `chans_1_<sig>`,
    // ..., `chans_{N-1}_<sig>` (N copies × #signals each), and inst-site
    // bracket-dot indexing `chans[i].sig` resolves to the i-th copy.
    let source = "
        bus B
          v: out Bool;
          r: in  Bool;
          d: out UInt<8>;
        end bus B

        module Prod
          port clk: in Clock<SysDomain>;
          port chans: initiator Vec<B, 3>;
          comb
            chans[0].v = true;
            chans[0].d = 8'h11;
            chans[1].v = false;
            chans[1].d = 8'h22;
            chans[2].v = true;
            chans[2].d = 8'h33;
          end comb
        end module Prod
    ";
    let sv = compile_to_sv(source);
    // All N copies appear on the module signature, each carrying every signal.
    for i in 0..3 {
        assert!(sv.contains(&format!("output logic chans_{i}_v")),
                "missing `output logic chans_{i}_v` in SV:\n{sv}");
        assert!(sv.contains(&format!("input logic chans_{i}_r")),
                "missing `input logic chans_{i}_r` in SV:\n{sv}");
        assert!(sv.contains(&format!("output logic [7:0] chans_{i}_d")),
                "missing `output logic [7:0] chans_{i}_d` in SV:\n{sv}");
    }
    // Bracket-dot access resolves to the indexed flat name.
    assert!(sv.contains("chans_0_v = 1'b1") || sv.contains("assign chans_0_v = 1'b1"),
            "expected `chans_0_v` assignment in SV:\n{sv}");
    assert!(sv.contains("chans_1_v = 1'b0") || sv.contains("assign chans_1_v = 1'b0"),
            "expected `chans_1_v` assignment in SV:\n{sv}");
    assert!(sv.contains("chans_2_d = 8'd51") || sv.contains("assign chans_2_d = 8'd51"),
            "expected `chans_2_d = 8'd51` assignment in SV:\n{sv}");
}

#[test]
fn test_vec_of_bus_port_rejects_zero_count() {
    // Literal-zero N is rejected at parse time. Param-driven and other
    // non-literal N expressions are allowed; they fold against module
    // params at typecheck/codegen time (see `test_vec_of_bus_port_param_driven_n`).
    let src_zero = r#"
        bus B
          v: out Bool;
        end bus B
        module M
          port chans: initiator Vec<B, 0>;
        end module M
    "#;
    let tokens = arch::lexer::tokenize(src_zero).expect("lex");
    let mut parser = arch::parser::Parser::new(tokens, src_zero);
    let err = parser.parse_source_file().expect_err("Vec<B, 0> should fail to parse");
    assert!(format!("{err:?}").contains("N must be >= 1"),
            "expected `N must be >= 1` diagnostic, got: {err:?}");
}

#[test]
fn test_vec_of_bus_port_param_driven_n() {
    // `port chans: initiator Vec<B, NUM_CHANS>;` where NUM_CHANS is a
    // module param — N folds to the param's default at SV emission time.
    // The same param drives the for-loop bound, and both static-unroll
    // paths (Vec-of-bus port count + for-loop bounds) fold against the
    // module's params.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B
        module M
          param NUM_CHANS: const = 3;
          port chans: initiator Vec<B, NUM_CHANS>;
          port idx:   in UInt<8>;
          comb
            for i in 0..NUM_CHANS-1
              chans[i].v = true;
              chans[i].d = (idx + i).trunc<8>();
            end for
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // 3 flat copies materialized (resolved from default NUM_CHANS = 3).
    for i in 0..3 {
        assert!(sv.contains(&format!("output logic chans_{i}_v")),
                "missing `output logic chans_{i}_v` in SV:\n{sv}");
    }
    // for-loop should be statically unrolled even though the upper bound
    // is `NUM_CHANS-1` (param expression).
    assert!(!sv.contains("for (int i ="),
            "expected param-driven for-loop bounds to fold + unroll:\n{sv}");
    assert!(sv.contains("chans_2_d = 8'(idx + 2)") || sv.contains("chans_2_d = 8'((idx + 2))"),
            "missing unrolled last-element assignment:\n{sv}");
}

#[test]
fn test_vec_of_bus_port_typecheck_drives_all_indexed_outputs() {
    // The driver-completeness check expands a Vec<Bus,N> port into N copies
    // and demands every output signal of every copy be driven. Forgetting
    // an index = compile error.
    let src_missing = r#"
        bus B
          v: out Bool;
        end bus B
        module M
          port chans: initiator Vec<B, 2>;
          comb
            chans[0].v = true;
            // chans[1].v intentionally undriven
          end comb
        end module M
    "#;
    let tokens = arch::lexer::tokenize(src_missing).expect("lex");
    let mut parser = arch::parser::Parser::new(tokens, src_missing);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let result = arch::typecheck::TypeChecker::new(&symbols, &ast).check();
    let errs = result.expect_err("missing index drive should error");
    let msg = errs.iter().map(|e| format!("{e:?}")).collect::<String>();
    // Diagnostic Display = "output port `chans_1_v` is not driven";
    // Debug = `UndriveOutput { name: "chans_1_v", ... }`. Accept either.
    assert!(msg.contains("chans_1_v") && (msg.contains("not driven") || msg.contains("UndriveOutput")),
            "expected `chans_1_v not driven` diagnostic, got: {msg}");
}

#[test]
fn test_vec_of_bus_for_loop_static_unroll() {
    // `chans[i].sig = ...;` inside `for i in 0..N-1` over a `Vec<Bus, N>`
    // port has no SV-level array (the signature exposes only the flattened
    // `<port>_<i>_<sig>` names). The codegen statically unrolls the loop
    // when its bounds are literal, binding the loop variable to each
    // iteration value so the body becomes N straight-line per-element
    // assignments.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B
        module M
          port chans: initiator Vec<B, 4>;
          port idx: in UInt<8>;
          comb
            for i in 0..3
              chans[i].v = true;
              chans[i].d = (idx + i).trunc<8>();
            end for
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // The loop should be statically unrolled: no behavioral `for (int i ...`,
    // and per-element flat assignments for each index.
    assert!(!sv.contains("for (int i ="),
            "expected for-loop to be statically unrolled (no behavioral SV for-loop):\n{sv}");
    for i in 0..4 {
        assert!(sv.contains(&format!("chans_{i}_v = 1'b1")),
                "missing unrolled `chans_{i}_v = 1'b1`:\n{sv}");
        // RHS must reference the literal i (not the loop variable).
        assert!(sv.contains(&format!("chans_{i}_d = 8'(idx + {i})"))
                || sv.contains(&format!("chans_{i}_d = 8'((idx + {i}))")),
                "missing unrolled `chans_{i}_d = 8'(idx + {i})`:\n{sv}");
    }
}

#[test]
fn test_vec_of_bus_inst_whole_vec_connection() {
    // `chans -> w;` (whole-vec) where both child port and parent wire/port
    // are `Vec<Bus, N>` expands to N per-element per-signal named-port
    // connections in SV — saves the user from writing
    // `chans[0] -> w[0]; chans[1] -> w[1]; ...` N times.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B
        module Producer
          port chans: initiator Vec<B, 3>;
          comb
            chans[0].v = true;  chans[0].d = 8'h11;
            chans[1].v = false; chans[1].d = 8'h22;
            chans[2].v = true;  chans[2].d = 8'h33;
          end comb
        end module Producer
        module Parent
          port out_d0: out UInt<8>;
          port out_d2: out UInt<8>;
          wire w: Vec<B, 3>;
          inst p: Producer
            chans -> w;
          end inst p
          comb
            out_d0 = w[0].d;
            out_d2 = w[2].d;
          end comb
        end module Parent
    ";
    let sv = compile_to_sv(source);
    for i in 0..3 {
        assert!(sv.contains(&format!(".chans_{i}_v(w_{i}_v)"))
                || sv.contains(&format!(".chans_{i}_v (w_{i}_v)")),
                "missing `.chans_{i}_v(w_{i}_v)` named-port connection:\n{sv}");
        assert!(sv.contains(&format!(".chans_{i}_d(w_{i}_d)"))
                || sv.contains(&format!(".chans_{i}_d (w_{i}_d)")),
                "missing `.chans_{i}_d(w_{i}_d)` named-port connection:\n{sv}");
    }
    // Should not leave a `.chans(w)` (illegal SV) artifact in the output.
    assert!(!sv.contains(".chans(w)"),
            "whole-vec connection must expand, not emit `.chans(w)`:\n{sv}");
}

#[test]
fn test_vec_of_bus_wire_flattens_to_n_indexed_signals() {
    // `wire w: Vec<BusName, N>;` is type-expression composition over the
    // existing `wire X: BusName;` form. SV codegen emits N flat
    // `w_0_<sig>`, ..., `w_{N-1}_<sig>` and `w[i].sig` access resolves
    // to the corresponding flat name. No new construct.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B

        module M
          port o_v0: out Bool;
          port o_d1: out UInt<8>;
          wire w: Vec<B, 2>;
          comb
            w[0].v = true;  w[0].d = 8'h11;
            w[1].v = false; w[1].d = 8'h22;
            o_v0 = w[0].v;
            o_d1 = w[1].d;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // The wire becomes N flat per-signal declarations.
    for (i, expected_d) in [(0u32, "8'd17"), (1u32, "8'd34")] {
        assert!(sv.contains(&format!("logic w_{i}_v;")),
                "missing `logic w_{i}_v;` in SV:\n{sv}");
        assert!(sv.contains(&format!("logic [7:0] w_{i}_d;")),
                "missing `logic [7:0] w_{i}_d;` in SV:\n{sv}");
        assert!(sv.contains(&format!("w_{i}_d = {expected_d}")),
                "missing `w_{i}_d = {expected_d}` assignment in SV:\n{sv}");
    }
    // Reading uses the same flat names.
    assert!(sv.contains("o_v0 = w_0_v"),
            "expected `o_v0 = w_0_v` in SV:\n{sv}");
    assert!(sv.contains("o_d1 = w_1_d"),
            "expected `o_d1 = w_1_d` in SV:\n{sv}");
}

#[test]
fn test_vec_of_bus_wire_carries_inst_output_through_indexed_connection() {
    // The producer drives a `Vec<B, N>` port; the parent declares a
    // `Vec<B, N>` wire and connects each element via `chans[i] -> w[i];`.
    // SV codegen must emit per-signal named-port connections with
    // `w_<i>_<sig>` on the parent side, and downstream reads on `w[i].sig`
    // must hit those same flat signals.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B

        module Producer
          port clk: in Clock<SysDomain>;
          port chans: initiator Vec<B, 2>;
          comb
            chans[0].v = true;  chans[0].d = 8'hAA;
            chans[1].v = true;  chans[1].d = 8'h55;
          end comb
        end module Producer

        module Parent
          port clk:   in  Clock<SysDomain>;
          port o_v0:  out Bool;
          port o_d1:  out UInt<8>;
          wire w: Vec<B, 2>;
          inst p: Producer
            clk <- clk;
            chans[0] -> w[0];
            chans[1] -> w[1];
          end inst p
          comb
            o_v0 = w[0].v;
            o_d1 = w[1].d;
          end comb
        end module Parent
    ";
    let sv = compile_to_sv(source);
    // The Vec-of-bus wire flattens to per-index per-signal storage.
    for i in 0..2 {
        assert!(sv.contains(&format!("logic w_{i}_v;")),
                "missing `logic w_{i}_v;` in Parent SV:\n{sv}");
    }
    // The Producer instance's bus port elements connect via per-signal
    // named ports against those flat wire signals.
    assert!(sv.contains(".chans_0_v(w_0_v)") || sv.contains(".chans_0_v (w_0_v)"),
            "expected `.chans_0_v(w_0_v)` named-port connection in SV:\n{sv}");
    assert!(sv.contains(".chans_1_d(w_1_d)") || sv.contains(".chans_1_d (w_1_d)"),
            "expected `.chans_1_d(w_1_d)` named-port connection in SV:\n{sv}");
    // The downstream reads land on the same flat names.
    assert!(sv.contains("o_v0 = w_0_v"),
            "expected `o_v0 = w_0_v` in SV:\n{sv}");
    assert!(sv.contains("o_d1 = w_1_d"),
            "expected `o_d1 = w_1_d` in SV:\n{sv}");
}

#[test]
fn test_vec_of_bus_inst_connection_uses_bracket_index_syntax() {
    // Parent instantiates a Child whose port is `Vec<B, N>`. Each index is
    // connected via the bracket form `chans[i] -> wire;`, matching the
    // declaration `Vec<_, N>` and the expression form `chans[i].sig`. The
    // codegen expands to per-signal named-port connections in SV.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B

        module Child
          port clk: in Clock<SysDomain>;
          port chans: initiator Vec<B, 2>;
          comb
            chans[0].v = true;  chans[0].d = 8'hAA;
            chans[1].v = false; chans[1].d = 8'h55;
          end comb
        end module Child

        module Parent
          port clk: in Clock<SysDomain>;
          port out_v0: out Bool;
          port out_d1: out UInt<8>;
          wire w0: B;
          wire w1: B;
          inst c: Child
            clk <- clk;
            chans[0] -> w0;
            chans[1] -> w1;
          end inst c
          comb
            out_v0 = w0.v;
            out_d1 = w1.d;
          end comb
        end module Parent
    ";
    let sv = compile_to_sv(source);
    // Child has 4 flat ports for the Vec<B,2>.
    for i in 0..2 {
        assert!(sv.contains(&format!("output logic chans_{i}_v")),
                "child should expose chans_{i}_v:\n{sv}");
        assert!(sv.contains(&format!("output logic [7:0] chans_{i}_d")),
                "child should expose chans_{i}_d:\n{sv}");
    }
    // Parent's inst connects each index by its underscore-suffixed name.
    assert!(sv.contains(".chans_0_v(w0_v)") || sv.contains(".chans_0_v (w0_v)"),
            "expected `.chans_0_v(w0_v)` named-port connection in SV:\n{sv}");
    assert!(sv.contains(".chans_1_d(w1_d)") || sv.contains(".chans_1_d (w1_d)"),
            "expected `.chans_1_d(w1_d)` named-port connection in SV:\n{sv}");
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
    let mut cg = Codegen::new(&symbols, &ast, overload_map);
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
    let mut cg = Codegen::new(&symbols, &ast, overload_map);
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
fn test_tlm_method_param_widths_substitute_in_implicit_bus_wires() {
    // arch-com#352: TLM method arg/ret/tag widths may be bus params.
    // A parent instantiating a child bus port through an undeclared
    // parent-side bus signal exercises the implicit bus-wire declaration
    // path; pre-fix it emitted `logic [TILE_W-1:0] link_read_tile;`
    // without declaring TILE_W in the parent SV module.
    let source = "
        bus Mem
          param KV_HEAD_W: const = 2;
          param TILE_W: const = 4;
          param TOKEN_W: const = 8;
          tlm_method read(tile: UInt<TILE_W>) -> UInt<TOKEN_W>: out_of_order tags KV_HEAD_W;
        end bus Mem

        use Mem;

        module Child
          port m: initiator Mem<KV_HEAD_W=3, TILE_W=5, TOKEN_W=17>;
          comb
            m.read_req_valid = 1'b0;
            m.read_req_tag = 3'h0;
            m.read_tile = 5'h0;
            m.read_rsp_ready = 1'b0;
          end comb
        end module Child

        module Parent
          inst c: Child
            m -> link;
          end inst c
        end module Parent
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("logic [2:0] link_read_req_tag;"),
        "implicit req tag wire should use overridden KV_HEAD_W:\n{sv}");
    assert!(sv.contains("logic [4:0] link_read_tile;"),
        "implicit arg wire should use overridden TILE_W:\n{sv}");
    assert!(sv.contains("logic [16:0] link_read_rsp_data;"),
        "implicit response wire should use overridden TOKEN_W:\n{sv}");
    assert!(!sv.contains("KV_HEAD_W") && !sv.contains("TILE_W") && !sv.contains("TOKEN_W"),
        "generated SV should not leak bus param identifiers into Parent plumbing:\n{sv}");
}

#[test]
fn test_tlm_method_param_widths_substitute_in_lowered_target_and_initiator() {
    // arch-com#352 original shape: TLM target/initiator lowering creates
    // latch regs and request mux plumbing from method arg/return types.
    // Those generated declarations must see bus params too, not emit
    // dangling identifiers such as TILE_W or TOKEN_W into SV.
    let source = "
        bus Mem
          param TILE_W: const = 4;
          param TOKEN_W: const = 8;
          tlm_method read(tile: UInt<TILE_W>) -> UInt<TOKEN_W>: blocking;
        end bus Mem

        use Mem;

        module Target
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m: target Mem<TILE_W=5, TOKEN_W=17>;

          thread m.read(tile) on clk rising, rst high
            wait 1 cycle;
            return {12'd0, tile};
          end thread m.read
        end module Target

        module Driver
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m: initiator Mem<TILE_W=5, TOKEN_W=17>;
          port data_out: out UInt<17>;

          reg data: UInt<17> reset rst => 0;

          thread on clk rising, rst high
            data <= m.read(5'd3);
          end thread

          comb
            data_out = data;
          end comb
        end module Driver

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port data_out: out UInt<17>;

          inst d: Driver
            clk <- clk;
            rst <- rst;
            m -> link;
            data_out -> data_out;
          end inst d

          inst t: Target
            clk <- clk;
            rst <- rst;
            m -> link;
          end inst t
        end module Top
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("logic [4:0] _tlm_m_read_tile_latched;"),
        "target latch reg should use substituted TILE_W:\n{sv}");
    assert!(sv.contains("output logic [16:0] m_read_rsp_data"),
        "target response data should use substituted TOKEN_W:\n{sv}");
    assert!(sv.contains("assign m_read_tile = _tlm_init_m_read_grant_0 ? 5'd3 : 0;"),
        "initiator request drive should use substituted TILE_W:\n{sv}");
    assert!(!sv.contains("TILE_W") && !sv.contains("TOKEN_W"),
        "lowered generated SV should not leak bus param identifiers:\n{sv}");
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
fn test_tlm_vec_return_sim_mirror_uses_array_copy() {
    let source = "
        bus BurstMem
          tlm_method read4(addr: UInt<32>) -> Vec<UInt<32>, 4>: out_of_order tags 1;
        end bus BurstMem

        use BurstMem;

        module VecBurstCaller
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m: initiator BurstMem;
          reg data: Vec<UInt<32>, 4> reset rst => 0;
          thread driver on clk rising, rst high
            data <= m.read4(32'h1000);
          end thread driver
        end module VecBurstCaller
    ";
    let out = compile_to_sim_h(source, false);
    assert!(
        out.contains("uint32_t m_read4_rsp_data[4];")
            && out.contains("uint32_t& m_read4_rsp_data_0;"),
        "bus Vec payload should expose a C++ array with flat compatibility aliases:\n{out}"
    );
    assert!(out.contains("uint32_t _m_read4_rsp_data[4];"),
        "flattened bus Vec payload should have an internal array:\n{out}");
    assert!(out.contains("_m_read4_rsp_data[0] = m_read4_rsp_data_0;"),
        "input bridge should copy flat fields into the internal array:\n{out}");
    assert!(out.contains("for (size_t _i = 0; _i < 4; ++_i) { _n_data[_i] = _m_read4_rsp_data[_i]; }"),
        "whole-Vec TLM response assignment should lower to element copy:\n{out}");
}

#[test]
fn test_struct_vec_field_sim_mirror_uses_array_field() {
    let source = "
        struct BoundedVecResp32x4
          data: Vec<UInt<32>, 4>;
          len: UInt<3>;
          resp: UInt<2>;
        end struct BoundedVecResp32x4

        module StructVecFieldsSmoke
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in0: in UInt<32>;
          port out0: out UInt<32>;
          port out_len: out UInt<3>;
          reg r: BoundedVecResp32x4 reset rst => 0;

          comb
            out0 = r.data[0];
            out_len = r.len;
          end comb

          seq on clk rising
            r.data[0] <= in0;
            r.data[1] <= 32'h22;
            r.len <= 3'd2;
          end seq
        end module StructVecFieldsSmoke
    ";
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("uint32_t data[4];"),
        "struct Vec field should emit as a C++ array:\n{out}");
    assert!(out.contains("out0  = _r.data[0];"),
        "struct Vec field read should use array indexing, not bit extraction:\n{out}");
    assert!(out.contains("_n_r.data[0]  = in0;"),
        "struct Vec field write should use array indexing:\n{out}");
    assert!(!out.contains("((_n_r.data) >> (0)) & 1"),
        "struct Vec field must not be treated as scalar bit indexing:\n{out}");
}

#[test]
fn test_tlm_struct_vec_response_sim_mirror_compiles_shape() {
    let source = "
        struct BoundedVecResp32x4
          data: Vec<UInt<32>, 4>;
          len: UInt<3>;
          resp: UInt<2>;
        end struct BoundedVecResp32x4

        bus MemBurst
          tlm_method read_burst(addr: UInt<32>, len: UInt<3>) -> BoundedVecResp32x4: out_of_order tags 1;
        end bus MemBurst

        use MemBurst;

        module StructVecTlmCaller
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m: initiator MemBurst;
          port out0: out UInt<32>;
          port out_len: out UInt<3>;
          reg r: BoundedVecResp32x4 reset rst => 0;

          comb
            out0 = r.data[0];
            out_len = r.len;
          end comb

          thread driver on clk rising, rst high
            r <= m.read_burst(32'h1000, 3'd2);
          end thread driver
        end module StructVecTlmCaller
    ";
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("BoundedVecResp32x4 m_read_burst_rsp_data;"),
        "TLM struct response should expose the struct payload in sim:\n{out}");
    assert!(out.contains("_n_r  = m_read_burst_rsp_data;"),
        "TLM struct response should copy into the destination register:\n{out}");
    assert!(!out.contains("m_read_burst_rsp_data >>"),
        "struct response should not be emitted as a scalar trace expression:\n{out}");
}

#[test]
fn test_tlm_canonical_end_to_end_initiator_plus_target() {
    // PR-tlm-7: canonical validation — a minimal Mem bus with `read`
    // and `write` methods, plus initiator + target pair exercising
    // both sides of the wire protocol.
    let source = include_str!("axi_dma_tlm/TlmOneToOne.arch");
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
    assert!(sv.contains("module TlmOneToOneTop")
         && sv.contains("link_read_req_valid"),
        "top-level one-to-one bus wire connection should appear:\n{sv}");

    // Compile to sim C++ too — same path should flow through the existing
    // reg/seq/comb sim mirror without issues.
    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("_tlm_s_read_state") && sim.contains("_tlm_init_driver_state"),
        "sim C++ should mirror the state regs for both sides");
}

#[test]
fn test_tlm_connect_one_to_one_sugar_lowers_to_bus_wire() {
    let source = include_str!("axi_dma_tlm/TlmConnectOneToOne.arch");
    let sv = compile_to_sv(source);

    assert!(sv.contains("module TlmConnectOneToOneTop"),
        "connect-sugar top should build:\n{sv}");
    assert!(sv.contains("_tlm_conn_i_m_t_s_read_req_valid")
         && sv.contains("_tlm_conn_i_m_t_s_write_req_valid"),
        "connect sugar should synthesize a private flattened TLM bus wire:\n{sv}");
    assert!(sv.contains(".m_read_req_valid(_tlm_conn_i_m_t_s_read_req_valid)")
         && sv.contains(".s_read_req_valid(_tlm_conn_i_m_t_s_read_req_valid)"),
        "connect sugar should wire initiator and target endpoints together:\n{sv}");

    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("class VTlmConnectOneToOneTop"),
        "sim C++ should include the connect-sugar top");
}

#[test]
fn test_tlm_connect_inside_generate_for_lowers_to_per_iteration_wires() {
    let source = include_str!("axi_dma_tlm/TlmConnectGenerate.arch");
    let sv = compile_to_sv(source);

    assert!(sv.contains("module TlmConnectGenerateTop"),
        "generate-for connect-sugar top should build:\n{sv}");
    assert!(sv.contains("_tlm_conn_src_0_m_dst_0_s_read_req_valid")
         && sv.contains("_tlm_conn_src_1_m_dst_1_s_read_req_valid"),
        "generate-for connect sugar should synthesize one private TLM bus per iteration:\n{sv}");
    assert!(sv.contains(".m_read_req_valid(_tlm_conn_src_0_m_dst_0_s_read_req_valid)")
         && sv.contains(".s_read_req_valid(_tlm_conn_src_0_m_dst_0_s_read_req_valid)")
         && sv.contains(".m_read_req_valid(_tlm_conn_src_1_m_dst_1_s_read_req_valid)")
         && sv.contains(".s_read_req_valid(_tlm_conn_src_1_m_dst_1_s_read_req_valid)"),
        "unrolled initiator and target endpoints should be wired pairwise:\n{sv}");

    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("class VTlmConnectGenerateTop"),
        "sim C++ should include the generate-for connect-sugar top");
}

fn tlm_connect_elaborate_error(source: &str) -> String {
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let err = arch::elaborate::elaborate(ast).expect_err("expected elaborate error");
    err.iter().map(|e| format!("{e:?}")).collect::<String>()
}

#[test]
fn test_tlm_connect_unknown_instance_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port m: initiator Mem;
end module Initiator
module Target
  port s: target Mem;
end module Target
module Top
  inst t: Target
  end inst t
  connect missing.m -> t.s;
end module Top
"#);
    assert!(msg.contains("unknown TLM connect instance `missing`"),
        "expected unknown-instance diagnostic, got: {msg}");
}

#[test]
fn test_tlm_connect_unknown_port_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port m: initiator Mem;
end module Initiator
module Target
  port s: target Mem;
end module Target
module Top
  inst i: Initiator
  end inst i
  inst t: Target
  end inst t
  connect i.nope -> t.s;
end module Top
"#);
    assert!(msg.contains("module `Initiator` has no port `nope`"),
        "expected unknown-port diagnostic, got: {msg}");
}

#[test]
fn test_tlm_connect_non_bus_port_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port scalar: out Bool;
  port m: initiator Mem;
end module Initiator
module Target
  port s: target Mem;
end module Target
module Top
  inst i: Initiator
  end inst i
  inst t: Target
  end inst t
  connect i.scalar -> t.s;
end module Top
"#);
    assert!(msg.contains("non-bus port `scalar`"),
        "expected non-bus-port diagnostic, got: {msg}");
}

#[test]
fn test_tlm_connect_direction_mismatch_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port m: initiator Mem;
end module Initiator
module Target
  port s: target Mem;
end module Target
module Top
  inst i: Initiator
  end inst i
  inst t: Target
  end inst t
  connect t.s -> i.m;
end module Top
"#);
    assert!(msg.contains("requires `connect initiator_inst.initiator_port -> target_inst.target_port;`")
         && msg.contains("t.s") && msg.contains("Target")
         && msg.contains("i.m") && msg.contains("Initiator"),
        "expected direction-mismatch diagnostic, got: {msg}");
}

#[test]
fn test_tlm_connect_bus_mismatch_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus MemA
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus MemA
bus MemB
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus MemB
use MemA;
use MemB;
module Initiator
  port m: initiator MemA;
end module Initiator
module Target
  port s: target MemB;
end module Target
module Top
  inst i: Initiator
  end inst i
  inst t: Target
  end inst t
  connect i.m -> t.s;
end module Top
"#);
    assert!(msg.contains("TLM connect bus mismatch")
         && msg.contains("MemA") && msg.contains("MemB"),
        "expected bus-mismatch diagnostic, got: {msg}");
}

#[test]
fn test_tlm_connect_duplicate_explicit_connection_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port m: initiator Mem;
end module Initiator
module Target
  port s: target Mem;
end module Target
module Top
  wire explicit: Mem;
  inst i: Initiator
    m -> explicit;
  end inst i
  inst t: Target
  end inst t
  connect i.m -> t.s;
end module Top
"#);
    assert!(msg.contains("duplicates an explicit connection")
         && msg.contains("i.m"),
        "expected duplicate-explicit-connection diagnostic, got: {msg}");
}

#[test]
fn test_tlm_connect_endpoint_reuse_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port m: initiator Mem;
end module Initiator
module Target
  port s: target Mem;
end module Target
module Top
  inst i: Initiator
  end inst i
  inst t0: Target
  end inst t0
  inst t1: Target
  end inst t1
  connect i.m -> t0.s;
  connect i.m -> t1.s;
end module Top
"#);
    assert!(msg.contains("TLM connect endpoint `i.m` is connected more than once"),
        "expected endpoint-reuse diagnostic, got: {msg}");
}

#[test]
fn test_tlm_connect_endpoint_reuse_after_generate_for_diagnostic() {
    let msg = tlm_connect_elaborate_error(r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port m: initiator Mem;
end module Initiator
module Target
  port s: target Mem;
end module Target
module Top
  inst i: Initiator
  end inst i
  generate_for n in 0..1
    inst t_n: Target
    end inst t_n
    connect i.m -> t_n.s;
  end generate_for
end module Top
"#);
    assert!(msg.contains("TLM connect endpoint `i.m` is connected more than once"),
        "expected endpoint-reuse-after-generate diagnostic, got: {msg}");
}

#[test]
fn test_tlm_one_initiator_many_targets_router_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmOneToMany.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmAddrRouter2"),
        "one-to-many TLM router should build:\n{sv}");
    assert!(sv.contains("lo_read_req_valid = up_read_req_valid && read_to_hi == 1'b0")
         && sv.contains("hi_read_req_valid = up_read_req_valid && read_to_hi"),
        "router should decode request valid to exactly one downstream target:\n{sv}");
    assert!(sv.contains("up_read_rsp_data = read_sel_hi ? hi_read_rsp_data : lo_read_rsp_data")
         && sv.contains("up_write_rsp_data = write_sel_hi ? hi_write_rsp_data : lo_write_rsp_data"),
        "router should mux responses through the latched request target:\n{sv}");
    assert!(sv.contains("module TlmOneToManyTop")
         && sv.contains("cpu_link_read_req_valid")
         && sv.contains("lo_link_read_req_valid")
         && sv.contains("hi_link_read_req_valid"),
        "top should connect one initiator through router to two target links:\n{sv}");

    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("class VTlmAddrRouter2")
         && sim.contains("class VTlmOneToManyTop"),
        "sim C++ should include router and top mirrors");
}

#[test]
fn test_tlm_one_initiator_many_targets_ooo_router_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmOneToManyOoo.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmOooAddrRouter2"),
        "OOO one-to-many TLM router should build:\n{sv}");
    assert!(sv.contains("logic [3:0] read_route_hi;")
         && sv.contains("read_route_hi[up_read_req_tag] <= read_to_hi"),
        "OOO router should record downstream route per upstream tag:\n{sv}");
    assert!(sv.contains("lo_read_req_tag = up_read_req_tag")
         && sv.contains("hi_read_req_tag = up_read_req_tag"),
        "OOO router should forward upstream tags unchanged downstream:\n{sv}");
    assert!(sv.contains("up_read_rsp_tag = choose_hi_rsp ? hi_read_rsp_tag : lo_read_rsp_tag")
         && sv.contains("hi_read_rsp_ready = up_read_rsp_ready && choose_hi_rsp"),
        "OOO router should mux responses by saved route and downstream response tag:\n{sv}");
    assert!(sv.contains("module TlmOneToManyOooTop")
         && sv.contains("cpu_link_read_req_tag")
         && sv.contains("lo_link_read_req_tag")
         && sv.contains("hi_link_read_req_tag"),
        "OOO top should connect tag signals through one-to-many links:\n{sv}");

    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("class VTlmOooAddrRouter2")
         && sim.contains("class VTlmOneToManyOooTop"),
        "sim C++ should include OOO router and top mirrors");
}

#[test]
fn test_tlm_one_initiator_many_targets_response_router_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmOneToManyResp.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmAddrRouter4Resp"),
        "response-typed one-to-many TLM router should build:\n{sv}");
    assert!(sv.contains("typedef struct packed")
         && sv.contains("MemResp64"),
        "struct response payload should be emitted:\n{sv}");
    assert!(sv.contains("read_err_valid")
         && sv.contains("up_read_rsp_data = read_err_valid ?"),
        "router should synthesize its own decode-error response:\n{sv}");
    assert!(sv.contains("s_read_rsp_data = {64'd1152921504606846976, 2'd0}")
         && sv.contains("up_read_rsp_data = read_err_valid ? {64'd0, 2'd1}"),
        "SV codegen should emit packed struct literals as iverilog-friendly concatenations:\n{sv}");
    assert!(sv.contains("_auto_tlm_m_read_req_stable")
         && sv.contains("$stable(m_read_addr)")
         && sv.contains("_auto_tlm_m_read_rsp_stable")
         && sv.contains("$stable(m_read_rsp_data)"),
        "TLM protocol assertions should track request args and struct response payloads:\n{sv}");
    assert!(sv.contains("t0_read_req_valid = up_read_req_valid && read_to_0")
         && sv.contains("t3_read_req_valid = up_read_req_valid && read_to_3"),
        "router should decode requests across four downstream targets:\n{sv}");

    let sim = compile_to_sim_h(source, false);
    assert!(sim.contains("class VTlmAddrRouter4Resp")
         && sim.contains("MemResp64 up_read_rsp_data"),
        "sim C++ should include struct response router mirror:\n{sim}");
    assert!(sim.contains("if (_let_read_mapped == 0)"),
        "sim C++ if conditions should not double-wrap comparison expressions:\n{sim}");
    assert!(!sim.contains("if ((_let_read_mapped == 0))"),
        "sim C++ should avoid Clang -Wparentheses-equality noise:\n{sim}");
}

#[test]
fn test_tlm_ooo_protocol_asserts_track_tags() {
    let source = include_str!("axi_dma_tlm/TlmOneToManyOoo.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("_auto_tlm_m_read_req_stable")
         && sv.contains("$stable(m_read_req_tag)")
         && sv.contains("$stable(m_read_addr)"),
        "OOO request assertion should track req_tag under backpressure:\n{sv}");
    assert!(sv.contains("_auto_tlm_m_read_rsp_stable")
         && sv.contains("$stable(m_read_rsp_tag)")
         && sv.contains("$stable(m_read_rsp_data)"),
        "OOO response assertion should track rsp_tag under backpressure:\n{sv}");
}

#[test]
fn test_tlm_one_initiator_many_targets_router_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmOneToMany.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_one_to_many.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for one-to-many router");
    assert!(out.status.success(),
        "one-to-many router sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS one-to-many blocking"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout));
}

#[test]
fn test_tlm_one_initiator_many_targets_ooo_router_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmOneToManyOoo.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_one_to_many_ooo.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for OOO one-to-many router");
    assert!(out.status.success(),
        "OOO one-to-many router sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS one-to-many OOO"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout));
}

#[test]
fn test_tlm_one_initiator_many_targets_response_router_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmOneToManyResp.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_one_to_many_resp.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for response-typed one-to-many router");
    assert!(out.status.success(),
        "response-typed one-to-many router sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS one-to-many response router"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout));
}

#[test]
fn test_reentrant_thread_keyword_rejected_by_parser() {
    // Reentrant thread syntax was a historical TLM pipelining pivot.
    // Parallel copies are expressed with `generate_for` threads instead,
    // so the parser no longer accepts the keyword.
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
    let result = parser.parse_source_file();
    assert!(result.is_err(), "reentrant thread syntax should no longer parse");
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
    // Non-indexed multiple target implementers are ambiguous. Use indexed
    // target lanes on an `out_of_order tags N` method for this shape.
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
    assert!(r.is_err(), "non-indexed multi-implementer target should error");
    let msg = format!("{:?}", r.unwrap_err());
    assert!(msg.contains("multi-implementer target") && msg.contains("s.read"),
        "expected targeted error, got: {msg}");
}

#[test]
fn test_tlm_indexed_target_generate_for_lowers_tag_lanes() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;

          generate_for t in 0..3
            thread s.read[t](addr) on clk rising, rst high
              return 64'h42;
            end thread s.read
          end generate_for
        end module MemTarget
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_s_read_tag0_state")
         && sv.contains("_tlm_s_read_tag3_state"),
        "indexed target lanes should lower to independent lane FSMs:\n{sv}");
    assert!(sv.contains("_tlm_s_read_tag0_req_ready")
         && sv.contains("_tlm_s_read_tag3_rsp_valid"),
        "indexed target lanes should use private endpoint wires:\n{sv}");
    assert!(sv.contains("s_read_req_tag == 2'd0")
         && sv.contains("s_read_req_tag == 2'd3"),
        "shared target endpoint should route requests by tag lane:\n{sv}");
    assert!(sv.contains("s_read_rsp_tag = _tlm_s_read_tag0_rsp_tag")
         && sv.contains("s_read_rsp_data = _tlm_s_read_tag0_rsp_data"),
        "shared response endpoint should mux lane responses:\n{sv}");
}

#[test]
fn test_tlm_indexed_target_response_lock_uses_resource_policy() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;

          resource read_rsp: mutex<round_robin>;

          generate_for t in 0..3
            thread s.read[t](addr) on clk rising, rst high
              lock read_rsp
                return 64'h42;
              end lock read_rsp
            end thread s.read
          end generate_for
        end module MemTarget
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("module _arb_MemTarget_read_rsp"),
        "response lock should synthesize a policy arbiter module:\n{sv}");
    assert!(sv.contains("rr_ptr_r <= rr_ptr_r + 1"),
        "response arbiter should use the resource's round-robin policy:\n{sv}");
    assert!(sv.contains("_tlm_s_read_rsp_arb_req_packed[0] = !_tlm_s_read_rsp_arb_hold_valid_r && _tlm_s_read_tag0_rsp_valid")
         && sv.contains("_tlm_s_read_rsp_arb_req_packed[3] = !_tlm_s_read_rsp_arb_hold_valid_r && _tlm_s_read_tag3_rsp_valid"),
        "lane response valids should feed the response arbiter:\n{sv}");
    assert!(sv.contains("_tlm_s_read_rsp_arb_hold_idx_r == 2'd0 || _tlm_s_read_rsp_arb_grant_packed[0]")
         && sv.contains("_tlm_s_read_rsp_arb_hold_idx_r == 2'd3 || _tlm_s_read_rsp_arb_grant_packed[3]"),
        "shared response mux should be gated by the granted lane:\n{sv}");
    assert!(sv.contains("_tlm_s_read_rsp_arb_hold_valid_r <= 1'd1")
         && sv.contains("_tlm_s_read_rsp_arb_hold_idx_r <= _tlm_s_read_rsp_arb_grant_requester"),
        "backpressured response selection should be held stable:\n{sv}");
    assert!(sv.contains("_tlm_s_read_tag0_rsp_ready = s_read_rsp_ready"),
        "only the selected lane should receive shared response ready:\n{sv}");
}

#[test]
fn test_axi_dma_tlm_indexed_burst_target_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmIndexedBurstTarget.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmIndexedBurstTarget"),
        "indexed burst target example should build:\n{sv}");
    assert!(sv.contains("BoundedVecResp32x4 _tlm_s_read_burst_tag0_rsp_data"),
        "bounded Vec response should stay struct-typed through target lane lowering:\n{sv}");
    assert!(sv.contains("s_read_burst_req_tag == 2'd3"),
        "generated target lanes should route by request tag:\n{sv}");
}

#[test]
fn test_axi_dma_tlm_indexed_burst_target_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmIndexedBurstTarget.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_indexed_burst_target.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for indexed burst target response arbiter");
    assert!(out.status.success(),
        "indexed burst target arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS indexed response arb"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout));
}

#[test]
fn test_axi_dma_tlm_indexed_burst_target_verilator_behavior() {
    if std::process::Command::new("verilator").arg("--version").output().is_err() {
        eprintln!("skipping Verilator indexed burst target smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("TlmIndexedBurstTarget.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/axi_dma_tlm/TlmIndexedBurstTarget.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build indexed burst target SV");
    assert!(build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr));

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("TlmIndexedBurstTarget")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_tlm/tb_tlm_indexed_burst_target.cpp")
        .output()
        .expect("verilate indexed burst target");
    assert!(verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr));

    let exe = obj_dir.join("VTlmIndexedBurstTarget");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator indexed burst target");
    assert!(run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr));
    assert!(String::from_utf8_lossy(&run.stdout).contains("PASS indexed response arb"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout));
}

#[test]
fn test_axi_read_beat_interleave_example_compiles() {
    let source = include_str!("axi_dma_thread/ThreadAxiReadBeatInterleave.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module ThreadAxiReadBeatInterleave"),
        "beat-interleaving thread example should build:\n{sv}");
    assert!(sv.contains("_arb_ThreadAxiReadBeatInterleave_r_ch"),
        "response channel mutex should lower to a generated arbiter:\n{sv}");
    assert!(sv.contains("r_id = 1"),
        "generate_for lanes should become concrete response IDs:\n{sv}");
}

#[test]
fn test_axi_read_beat_interleave_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_thread/ThreadAxiReadBeatInterleave.arch")
        .arg("--tb")
        .arg("tests/axi_dma_thread/tb_axi_read_beat_interleave.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for beat-interleaving response target");
    assert!(out.status.success(),
        "beat-interleaving arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS beat interleave alternating"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout));
}

#[test]
fn test_axi_read_beat_interleave_verilator_behavior() {
    if std::process::Command::new("verilator").arg("--version").output().is_err() {
        eprintln!("skipping Verilator beat-interleaving smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("ThreadAxiReadBeatInterleave.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/axi_dma_thread/ThreadAxiReadBeatInterleave.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build beat-interleaving SV");
    assert!(build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr));

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("ThreadAxiReadBeatInterleave")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_thread/tb_axi_read_beat_interleave.cpp")
        .output()
        .expect("verilate beat-interleaving response target");
    assert!(verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr));

    let exe = obj_dir.join("VThreadAxiReadBeatInterleave");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator beat-interleaving response target");
    assert!(run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr));
    assert!(String::from_utf8_lossy(&run.stdout).contains("PASS beat interleave alternating"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout));
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
fn test_implement_initiator_multi_implementer_compiles_end_to_end() {
    // Initiator-side `implement m.read()` is now an annotation over the
    // ordinary call-site/cohort lowering. Multiple implementer threads
    // share the same generated method driver instead of being rejected.
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
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_init_w0_state")
         && sv.contains("_tlm_init_w1_state"),
        "multi-implementer initiator should lower both workers:\n{sv}");
    assert!(sv.contains("m_read_req_valid")
         && sv.contains("m_read_rsp_ready"),
        "shared method driver should still drive req/rsp handshake:\n{sv}");
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
fn test_tlm_rhs_fork_tail_rejects_wait_after_join_all() {
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
            join all;
            wait 1 cycle;
          end thread workers
        end module Shared
    ";
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = arch::elaborate::elaborate(ast).expect("elaborate");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target");
    let r = arch::elaborate::lower_tlm_initiator_calls(ast);
    assert!(r.is_err(), "RHS-fork TLM tail should reject waits");
    let msg = format!("{:?}", r.unwrap_err());
    assert!(
        msg.contains("compute-only") && msg.contains("wait"),
        "expected compute-only wait diagnostic, got: {msg}"
    );
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
fn test_axi_dma_tlm_read_pair_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmMm2sReadPair.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmMm2sReadPair"));
    assert!(sv.contains("mem_read_req_tag"));
    assert!(sv.contains("mem_read_rsp_tag"));
    assert!(sv.contains("_tlm_fork_issue_pair_mem_read"));
}

#[test]
fn test_axi_dma_tlm_read_pair_tail_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmMm2sReadPairTail.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmMm2sReadPairTail"));
    assert!(sv.contains("_tlm_fork_issue_pair_mem_read_tail_done"));
    assert!(
        sv.contains("checksum_r <=") && sv.contains("done_r <= 1'b1"),
        "RHS-fork compute tail should lower into sequential assignments:\n{sv}"
    );
}

#[test]
fn test_tlm_rhs_fork_tail_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmMm2sReadPairTail.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_mm2s_read_pair_tail.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for RHS-fork TLM tail");
    assert!(
        out.status.success(),
        "RHS-fork tail arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS TlmMm2sReadPairTail"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_tlm_rhs_fork_tail_verilator_behavior() {
    if std::process::Command::new("verilator").arg("--version").output().is_err() {
        eprintln!("skipping Verilator RHS-fork tail smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("TlmMm2sReadPairTail.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/axi_dma_tlm/TlmMm2sReadPairTail.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build RHS-fork tail SV");
    assert!(
        build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("TlmMm2sReadPairTail")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_tlm/tb_tlm_mm2s_read_pair_tail.cpp")
        .output()
        .expect("verilate RHS-fork tail");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VTlmMm2sReadPairTail");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator RHS-fork tail");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS TlmMm2sReadPairTail"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
}

#[test]
fn test_axi_dma_tlm_burst_vec_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmMm2sBurstVec.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmMm2sBurstVec"));
    assert!(sv.contains("input logic [3:0] [31:0] mem_read_burst_rsp_data"));
    assert!(sv.contains("mem_read_burst_len"));
    assert!(sv.contains("mem_read_burst_req_tag"));
    assert!(sv.contains("mem_read_burst_rsp_tag"));
    assert!(sv.contains("_tlm_fork_issue_bursts_mem_read_burst"));
    assert!(sv.contains("mem_read_burst_rsp_tag == 2'd0")
        && sv.contains("mem_read_burst_rsp_tag == 2'd1"),
        "burst Vec OOO responses should route by worker tag:\n{sv}");

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("uint32_t mem_read_burst_rsp_data[4];")
            && sim.contains("uint32_t& mem_read_burst_rsp_data_0;"),
        "sim API should preserve Vec response lanes as an array with flat aliases:\n{sim}"
    );
    assert!(sim.contains("uint32_t _mem_read_burst_rsp_data[4];"),
        "sim model should mirror the flattened Vec response as an internal array:\n{sim}");
    assert!(sim.contains("for (size_t _i = 0; _i < 4; ++_i) { _n_burst0_r[_i] = _mem_read_burst_rsp_data[_i]; }")
        && sim.contains("for (size_t _i = 0; _i < 4; ++_i) { _n_burst1_r[_i] = _mem_read_burst_rsp_data[_i]; }"),
        "burst Vec responses should copy into both destination arrays:\n{sim}");
}

#[test]
fn test_axi_dma_tlm_burst_vec_bfm_connect_compiles() {
    let source = include_str!("axi_dma_tlm/TlmMm2sBurstVecBfm.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module TlmMm2sBurstVecBfmTop"));
    assert!(
        sv.contains("_tlm_conn_dut_i_mem_bfm_i_s_read_burst_rsp_data"),
        "SV connect should materialize Vec response bus wire:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("uint32_t read_burst_rsp_data[4];"),
        "sim bus-as-wire struct should keep Vec payload fields as arrays:\n{sim}"
    );
    assert!(
        sim.contains("uint32_t mem_read_burst_rsp_data[4];")
            && sim.contains("uint32_t& mem_read_burst_rsp_data_0;")
            && sim.contains("uint32_t s_read_burst_rsp_data[4];")
            && sim.contains("uint32_t& s_read_burst_rsp_data_0;"),
        "sim APIs should preserve Vec TLM ports as arrays while keeping flat lane aliases:\n{sim}"
    );
}

#[test]
fn test_axi_dma_tlm_burst_vec_bfm_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmMm2sBurstVecBfm.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_mm2s_burst_vec_bfm.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for Vec TLM BFM");
    assert!(
        out.status.success(),
        "Vec TLM BFM arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS TlmMm2sBurstVecBfm"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_axi_dma_tlm_burst_vec_bfm_thread_sim_both() {
    run_tlm_thread_sim_both(
        "tests/axi_dma_tlm/TlmMm2sBurstVecBfm.arch",
        "tests/axi_dma_tlm/tb_tlm_mm2s_burst_vec_bfm.cpp",
        "PASS TlmMm2sBurstVecBfm",
    );
}

#[test]
fn test_axi_dma_tlm_burst_vec_bfm_verilator_behavior() {
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping Verilator Vec TLM BFM smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("TlmMm2sBurstVecBfm.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/axi_dma_tlm/TlmMm2sBurstVecBfm.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build Vec TLM BFM SV");
    assert!(
        build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("TlmMm2sBurstVecBfmTop")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_tlm/tb_tlm_mm2s_burst_vec_bfm.cpp")
        .output()
        .expect("verilate Vec TLM BFM");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VTlmMm2sBurstVecBfmTop");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator Vec TLM BFM");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS TlmMm2sBurstVecBfm"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
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
    assert!(sv.contains("s_read_rsp_tag = _tlm_s_read_tag_latched"),
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
fn test_tlm_initiator_call_inside_lock_lowers() {
    // TLM calls wrapped in `lock` should still be consumed by initiator
    // lowering instead of falling through to generic thread lowering.
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

          resource mem_ch: mutex<round_robin>;

          thread driver on clk rising, rst high
            lock mem_ch
              d <= m.read(32'h1000);
            end lock mem_ch
          end thread driver
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_init_driver_state"),
        "locked TLM call should lower to the inline initiator FSM:\n{sv}"
    );
    assert!(
        sv.contains("m_read_req_valid"),
        "locked TLM call should still drive request valid:\n{sv}"
    );
    assert!(
        sv.contains("m_read_rsp_ready"),
        "locked TLM call should still drive response ready:\n{sv}"
    );
}

#[test]
fn test_tlm_initiator_compute_only_if_lowers_between_calls() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<8>) -> UInt<32>: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg d: UInt<32> reset rst => 0;
          reg acc0: UInt<32> reset rst => 0;
          reg acc1: UInt<32> reset rst => 0;

          thread driver on clk rising, rst high
            for i in 0..3
              d <= m.read(i.zext<8>());
              if i[0] == 1'b0
                acc0 <= acc0 +% d;
              else
                acc1 <= acc1 +% d;
              end if
            end for
          end thread driver
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_init_driver_state"),
        "TLM initiator should still lower to a state machine:\n{sv}"
    );
    assert!(
        sv.contains("acc0 <= 32'(acc0 + d);") && sv.contains("acc1 <= 32'(acc1 + d);"),
        "compute-only if branches should remain in the lowered thread FSM:\n{sv}"
    );
}

#[test]
fn test_tlm_initiator_if_branches_with_calls_compile() {
    let source = include_str!("axi_dma_tlm/TlmConditionalInitiator.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_init_driver_state"),
        "conditional TLM initiator should lower to a state machine:\n{sv}"
    );
    assert!(
        sv.contains("if (sel)"),
        "branch state should sample the source-level if condition:\n{sv}"
    );
    assert!(
        sv.contains("32'h1000") || sv.contains("32'd4096"),
        "then branch TLM call should drive the 0x1000 request address:\n{sv}"
    );
    assert!(
        sv.contains("32'h2000") || sv.contains("32'd8192"),
        "else branch TLM call should drive the 0x2000 request address:\n{sv}"
    );
}

#[test]
fn test_tlm_conditional_initiator_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmConditionalInitiator.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_conditional_initiator.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for conditional TLM initiator");
    assert!(out.status.success(),
        "conditional TLM initiator arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS TlmConditionalInitiator"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout));
}

#[test]
fn test_tlm_conditional_initiator_verilator_behavior() {
    if std::process::Command::new("verilator").arg("--version").output().is_err() {
        eprintln!("skipping Verilator conditional TLM smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("TlmConditionalInitiator.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/axi_dma_tlm/TlmConditionalInitiator.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build conditional TLM initiator SV");
    assert!(build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr));

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("TlmConditionalInitiator")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_tlm/tb_tlm_conditional_initiator.cpp")
        .output()
        .expect("verilate conditional TLM initiator");
    assert!(verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr));

    let exe = obj_dir.join("VTlmConditionalInitiator");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator conditional TLM initiator");
    assert!(run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr));
    assert!(String::from_utf8_lossy(&run.stdout).contains("PASS TlmConditionalInitiator"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout));
}

#[test]
fn test_fpt26_runtime_loop_tlm_initiator_compiles() {
    let source = include_str!("fpt26_tlm/Fpt26RuntimeLoopTlm.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_init_driver_loop_cnt_0"),
        "runtime TLM for loop should allocate a loop counter:\n{sv}"
    );
    assert!(
        sv.contains("assign hbm_read_k_req_valid")
            && sv.contains("assign qk_qk_tile_req_valid"),
        "runtime loop should still drive both serialized TLM request channels:\n{sv}"
    );
}

#[test]
fn test_fpt26_runtime_loop_tlm_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/fpt26_tlm/Fpt26RuntimeLoopTlm.arch")
        .arg("--tb")
        .arg("tests/fpt26_tlm/tb_fpt26_runtime_loop_tlm.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for FPT26 runtime-loop TLM");
    assert!(out.status.success(),
        "FPT26 runtime-loop TLM arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS Fpt26RuntimeLoopTlm"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout));
}

#[test]
fn test_fpt26_runtime_loop_tlm_verilator_behavior() {
    if std::process::Command::new("verilator").arg("--version").output().is_err() {
        eprintln!("skipping Verilator runtime-loop TLM smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("Fpt26RuntimeLoopTlm.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/fpt26_tlm/Fpt26RuntimeLoopTlm.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build FPT26 runtime-loop TLM SV");
    assert!(build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr));

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("--timing")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("Fpt26RuntimeLoopTlm")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/fpt26_tlm/tb_fpt26_runtime_loop_tlm.cpp")
        .output()
        .expect("verilate FPT26 runtime-loop TLM");
    assert!(verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr));

    let exe = obj_dir.join("VFpt26RuntimeLoopTlm");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator FPT26 runtime-loop TLM");
    assert!(run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr));
    assert!(String::from_utf8_lossy(&run.stdout).contains("PASS Fpt26RuntimeLoopTlm"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout));
}

#[test]
fn test_locked_tlm_generated_workers_share_one_method_driver() {
    let source = "
        bus Mem
          tlm_method read(tile: UInt<4>) -> Bool: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg ack: Vec<Bool, 3> reset rst => 0;

          resource mem_ch: mutex<round_robin>;

          generate_for tile in 0..2
            thread Worker_tile on clk rising, rst high
              lock mem_ch
                ack[tile] <= m.read(tile.zext<4>());
              end lock mem_ch
            end thread Worker_tile
          end generate_for
        end module M
    ";
    let sv = compile_to_sv(source);
    let req_valid_drives = sv.matches("assign m_read_req_valid").count();
    let tile_drives = sv.matches("assign m_read_tile").count();
    let rsp_ready_drives = sv.matches("assign m_read_rsp_ready").count();
    assert_eq!(
        req_valid_drives, 1,
        "expected one shared req_valid driver:\n{sv}"
    );
    assert_eq!(tile_drives, 1, "expected one shared payload driver:\n{sv}");
    assert_eq!(
        rsp_ready_drives, 1,
        "expected one shared rsp_ready driver:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_init_Worker_0_state")
            && sv.contains("_tlm_init_Worker_1_state")
            && sv.contains("_tlm_init_Worker_2_state"),
        "each generated worker should keep its own state register:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_init_m_read_rr_ptr"),
        "round-robin locked TLM sharing should emit a rotating grant pointer:\n{sv}"
    );
}

#[test]
fn test_round_robin_tlm_grants_split_into_intermediate_wires() {
    let source = "
        bus Mem
          tlm_method read(tile: UInt<5>) -> Bool: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg ack: Vec<Bool, 16> reset rst => 0;

          resource mem_ch: mutex<round_robin>;

          generate_for tile in 0..15
            thread Worker_tile on clk rising, rst high
              lock mem_ch
                ack[tile] <= m.read(tile.zext<5>());
              end lock mem_ch
            end thread Worker_tile
          end generate_for
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_init_m_read_rr_s0_g"),
        "round-robin grant terms should be emitted as intermediate wires:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_init_m_read_rr_grant_0_or_l0_"),
        "round-robin per-grant OR reductions should be chunked:\n{sv}"
    );
    let longest = sv.lines().map(str::len).max().unwrap_or(0);
    assert!(
        longest < 6000,
        "round-robin TLM grants should not emit Verilator-hostile long lines; longest was {longest}"
    );
}

#[test]
fn test_looped_tlm_initiator_muxes_split_into_intermediate_wires() {
    let source = "
        bus Mem
          tlm_method read(a: UInt<8>, b: UInt<8>) -> Bool: blocking;
        end bus Mem

        use Mem;

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port m:   initiator Mem;
          reg ack: Bool reset rst => false;

          thread driver on clk rising, rst high
            for i in 0..63
              ack <= m.read(i.zext<8>(), i[2:0].zext<8>());
            end for
          end thread driver
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_init_m_read_req_valid_or_l0_"),
        "large request-valid reduction should be chunked into intermediate wires:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_init_m_read_a_mux_data_l0_"),
        "large payload mux should be chunked into intermediate wires:\n{sv}"
    );
    let longest = sv.lines().map(str::len).max().unwrap_or(0);
    assert!(
        longest < 6000,
        "looped TLM initiator should not emit Verilator-hostile long lines; longest was {longest}"
    );
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
fn test_tlm_target_thread_accepts_wait_cycles_before_return() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          thread s.read(addr) on clk rising, rst high
            wait 7 cycle;
            return 64'h42;
          end thread s.read
        end module MemTarget
    ";
    let sv = compile_to_sv(source);
    assert!(sv.contains("_tlm_s_read_wait_cnt"),
        "wait-cycle target should allocate a counter:\n{sv}");
    assert!(sv.contains("32'd6"),
        "wait 7 cycle should initialize the counter to 6:\n{sv}");
}

#[test]
fn test_tlm_target_thread_rich_body_compiles() {
    let source = include_str!("tlm_target_body/TlmTargetRichBody.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_s_read_wait_cnt"),
        "target body with waits should allocate a wait counter:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_read_state"),
        "rich target body should still lower inline to a TLM target FSM:\n{sv}"
    );
}

#[test]
fn test_tlm_target_thread_rich_body_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/tlm_target_body/TlmTargetRichBody.arch")
        .arg("--tb")
        .arg("tests/tlm_target_body/tb_tlm_target_rich_body.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for rich TLM target body");
    assert!(
        out.status.success(),
        "rich target body arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS TlmTargetRichBody"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

fn run_tlm_thread_sim_both(arch_file: &str, tb_file: &str, pass_marker: &str) {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("--thread-sim")
        .arg("both")
        .arg(arch_file)
        .arg("--tb")
        .arg(tb_file)
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim --thread-sim both");
    assert!(
        out.status.success(),
        "arch sim --thread-sim both should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(pass_marker),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Cross-check PASS"),
        "expected thread-sim cross-check marker in stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_tlm_target_thread_rich_body_thread_sim_both() {
    run_tlm_thread_sim_both(
        "tests/tlm_target_body/TlmTargetRichBody.arch",
        "tests/tlm_target_body/tb_tlm_target_rich_body.cpp",
        "PASS TlmTargetRichBody",
    );
}

#[test]
fn test_tlm_target_thread_rich_body_verilator_behavior() {
    if std::process::Command::new("verilator").arg("--version").output().is_err() {
        eprintln!("skipping Verilator rich TLM target smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("TlmTargetRichBody.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/tlm_target_body/TlmTargetRichBody.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build rich TLM target SV");
    assert!(
        build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("--timing")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("TlmTargetRichBody")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/tlm_target_body/tb_tlm_target_rich_body.cpp")
        .output()
        .expect("verilate rich TLM target body");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VTlmTargetRichBody");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator rich TLM target body");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS TlmTargetRichBody"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
}

#[test]
fn test_tlm_target_thread_early_return_compiles() {
    let source = include_str!("tlm_target_body/TlmTargetEarlyReturn.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_tlm_s_read_state"),
        "early-return target body should lower inline to a TLM target FSM:\n{sv}"
    );
    assert!(
        sv.contains("2'd0") && sv.contains("2'd1"),
        "early-return response states should be present in emitted SV:\n{sv}"
    );
}

#[test]
fn test_tlm_target_thread_exhaustive_branch_return_without_terminal_fallback() {
    let source = "
        bus Mem
          tlm_method read(sel: UInt<1>) -> UInt<32>: blocking;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          thread s.read(sel) on clk rising, rst high
            if sel == 1'b0
              return 32'd10;
            else
              return 32'd20;
            end if
          end thread s.read
        end module MemTarget
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("32'd10") && sv.contains("32'd20"),
        "exhaustive branch-local returns should not require a terminal fallback:\n{sv}"
    );
}

#[test]
fn test_tlm_target_thread_early_return_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/tlm_target_body/TlmTargetEarlyReturn.arch")
        .arg("--tb")
        .arg("tests/tlm_target_body/tb_tlm_target_early_return.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for early-return TLM target body");
    assert!(
        out.status.success(),
        "early-return target body arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS TlmTargetEarlyReturn"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_tlm_target_thread_early_return_thread_sim_both() {
    run_tlm_thread_sim_both(
        "tests/tlm_target_body/TlmTargetEarlyReturn.arch",
        "tests/tlm_target_body/tb_tlm_target_early_return.cpp",
        "PASS TlmTargetEarlyReturn",
    );
}

#[test]
fn test_tlm_target_thread_early_return_verilator_behavior() {
    if std::process::Command::new("verilator").arg("--version").output().is_err() {
        eprintln!("skipping Verilator early-return TLM target smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("TlmTargetEarlyReturn.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/tlm_target_body/TlmTargetEarlyReturn.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build early-return TLM target SV");
    assert!(
        build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("--timing")
        .arg("-Wno-fatal")
        .arg("-Wno-WIDTH")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("TlmTargetEarlyReturn")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/tlm_target_body/tb_tlm_target_early_return.cpp")
        .output()
        .expect("verilate early-return TLM target body");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VTlmTargetEarlyReturn");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator early-return TLM target body");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS TlmTargetEarlyReturn"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
}

#[test]
fn test_tlm_target_thread_rejects_statements_after_return() {
    let source = "
        bus Mem
          tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
        end bus Mem

        use Mem;

        module MemTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port s:   target Mem;
          reg acc: UInt<64> reset rst => 0;
          thread s.read(addr) on clk rising, rst high
            return 64'h42;
            acc <= 64'h1;
          end thread s.read
        end module MemTarget
    ";
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let err = elaborate::lower_tlm_target_threads(parsed_ast)
        .expect_err("target TLM statements after return should be rejected");
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("statements after `return`"),
        "diagnostic should explain terminal return restriction, got: {msg}"
    );
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
    // The thread includes a `wait until` so the regular thread-without-
    // wait check doesn't shadow the more specific return-outside-TLM
    // diagnostic this test is targeting.
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port go:  in Bool;
          port reg out_r: out UInt<8> reset rst => 0;
          thread stray on clk rising, rst high
            wait until go;
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

// ── Wait-1-cycle elision optimisation ─────────────────────────────────────────

#[test]
fn test_wait_1_cycle_between_seq_writes_takes_one_cycle() {
    // `phase <= a; wait 1 cycle; phase <= b;` should produce two
    // consecutive thread states (one per seq write), with no extra
    // counter-stall state in between. The natural state transition
    // X → X+1 already provides the 1-cycle wait.
    //
    // Regression for the bug where `wait 1 cycle` emitted a dedicated
    // wait_cycles state (load cnt=0, decrement, check cnt==0,
    // transition), making each phase advance take 2 cycles instead of
    // 1. Surfaced by arch-ibex D1 (FillBufferCtrl phase tracker).
    let source = r#"
        module M
          port clk:   in Clock<SysDomain>;
          port rst_n: in Reset<Async, Low>;
          port go:    in Bool;
          port reg phase: out UInt<2> reset rst_n => 2'd0;
          thread on clk rising, rst_n low
            wait until go;
            phase <= 2'd1;
            wait 1 cycle;
            phase <= 2'd2;
            wait 1 cycle;
            phase <= 2'd3;
            wait 1 cycle;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    // The defining property of the elision: no counter-decrement state.
    // The dedicated wait_cycles state's body decrements a counter and
    // checks `_t0_cnt == 0`; its absence means `wait 1 cycle` produced
    // no extra state between the phase writes.
    assert!(!sv.contains("_t0_cnt <= 32'(_t0_cnt - 32'd1);"),
        "should not emit counter-decrement state for `wait 1 cycle`:\n{sv}");
    // Three phase writes appear, each transitioning to the next state.
    // State numbering after elision: 0=initial wait, 1=phase=1,
    // 2=phase=2, 3=phase=3, then loop back to 0. Issue #247 changed
    // state assignments to reference per-state `localparam` names
    // (`_t0_S<N>_<role>`) instead of bare numeric literals.
    assert!(sv.contains("phase <= 2'd1;") && sv.contains("phase <= 2'd2;")
            && sv.contains("phase <= 2'd3;"),
        "expected three phase writes:\n{sv}");
    assert!(sv.contains("_t0_state <= _t0_S2_action;")
            && sv.contains("_t0_state <= _t0_S3_action;"),
        "expected state transitions 1->2 and 2->3 via state-name localparams:\n{sv}");
}

#[test]
fn test_wait_1_cycle_in_else_branch_kept() {
    // When `wait 1 cycle` is the entire body of an if/else branch
    // (no preceding seq/comb to flush), the wait state must NOT be
    // elided — the branch needs at least one state to anchor the
    // dispatch-and-rejoin pattern, and the 1-cycle delay is the
    // semantic the user wrote.
    //
    // Regression for the over-aggressive elision that broke the
    // `test_if_wait_for_in_then_branch` style test. Pairs with
    // `test_wait_1_cycle_between_seq_writes_takes_one_cycle`.
    let source = r#"
        module M
          port clk:   in Clock<SysDomain>;
          port rst_n: in Reset<Async, Low>;
          port go:    in Bool;
          port doit:  in Bool;
          port ack:   in Bool;
          port done:  out Bool;
          thread on clk rising, rst_n low
            wait until go;
            if doit
              wait until ack;
              done = 1;
            else
              wait 1 cycle;
            end if
            wait 1 cycle;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("module _M_threads"),
        "thread with wait-1-cycle in else branch should compile:\n{sv}");
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

    // Wait-until: state 0 transitions on `start`. Issue #247 changed
    // state comparisons in auto-asserts to reference per-state
    // `localparam` names (`_t0_S<N>_<role>`) instead of bare literals.
    assert!(sv.contains("_auto_thread_t0_wait_until_s0:"),
        "expected wait_until property at state 0:\n{sv}");
    assert!(sv.contains("|=> _t0_state == _t0_S1_wait_cycles"),
        "expected next-cycle implication to state 1 (wait_cycles) via state-name localparam:\n{sv}");

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
fn test_thread_without_wait_or_do_until_errors_with_seq_hint() {
    // A `thread` body with no `wait` / `wait until` / `do until` (anywhere —
    // directly or nested in if/else/for/lock/fork) collapses to a single FSM
    // state and is structurally indistinguishable from a `seq on clk` block.
    // The elaborator must surface this loudly with a hint that points at the
    // right construct, instead of silently emitting the single-state thread
    // (which wastes a state-register flop for no benefit and obscures intent).
    //
    // Project memory: feedback_thread_single_state_idiom.md (A9 lesson).
    let cases = [
        // 1. Single seq assign, no wait at all.
        r#"
            module M
              port clk: in Clock<SysDomain>;
              port rst: in Reset<Sync, High>;
              port reg flag: out Bool reset rst => false;
              thread on clk rising, rst high
                flag <= true;
              end thread
            end module M
        "#,
        // 2. Seq assign inside an `if/else` — still no wait.
        r#"
            module M
              port clk: in Clock<SysDomain>;
              port rst: in Reset<Sync, High>;
              port sel: in Bool;
              port reg flag: out Bool reset rst => false;
              thread on clk rising, rst high
                if sel
                  flag <= true;
                else
                  flag <= false;
                end if
              end thread
            end module M
        "#,
    ];
    for source in cases.iter() {
        let tokens = arch::lexer::tokenize(source).expect("lexer error");
        let mut parser = arch::parser::Parser::new(tokens, source);
        let parsed_ast = parser.parse_source_file().expect("parse error");
        let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
        let ast = arch::elaborate::lower_tlm_target_threads(ast)
            .expect("tlm target lowering");
        let ast = arch::elaborate::lower_tlm_initiator_calls(ast)
            .expect("tlm initiator lowering");
        let result = arch::elaborate::lower_threads(ast);
        let errs = result.expect_err(
            "thread with no wait / do until should fail lower_threads"
        );
        assert!(!errs.is_empty(), "expected at least one error");
        let msg = errs[0].to_string();
        assert!(msg.contains("must contain at least one `wait` or `do until`"),
            "error should mention wait + do until: {msg}");
        assert!(msg.contains("seq on clk"),
            "error should suggest `seq on clk` alternative: {msg}");
    }
}

#[test]
fn test_thread_with_do_until_only_is_accepted() {
    // `do { ... } until cond;` is a valid yield boundary on its own — the
    // body produces ≥1 state, distinct from a single-state seq block. The
    // no-wait error from the companion test above must NOT fire here.
    let source = r#"
        module M
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Sync, High>;
          port go:   in Bool;
          port done: in Bool;
          port reg flag: out Bool reset rst => false;
          thread on clk rising, rst high
            do
              flag <= go;
            until done;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("always_ff"),
        "do/until thread should still compile to SV:\n{sv}");
}

#[test]
fn test_thread_wait_ifelse_fuses_dispatch_and_first_branch_action() {
    // Micro-architecture pattern from arch-ibex multdiv: a thread waits for
    // a request, dispatches on an opcode, then performs the first cycle of
    // branch-specific work before the next wait boundary. The lowering should
    // match a hand-written FSM shape by doing branch selection and the first
    // seq action on the same edge that exits the wait state.
    let source = r#"
        module M
          port clk:    in Clock<SysDomain>;
          port rst:    in Reset<Sync, High>;
          port req:    in Bool;
          port is_mul: in Bool;
          port reg phase: out UInt<4> reset rst => 4'd0;

          thread on clk rising, rst high
            wait until req;
            if is_mul
              phase <= 4'd1;
              wait 1 cycle;
              phase <= 4'd2;
              wait 1 cycle;
            else
              phase <= 4'd3;
              wait 1 cycle;
              phase <= 4'd4;
              wait 1 cycle;
            end if
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);

    assert!(sv.contains("if (req && is_mul) begin\n          phase <= 4'd1;"),
        "then-branch first action should be hoisted onto the wait-exit edge:\n{sv}");
    assert!(sv.contains("if (req && !is_mul) begin\n          phase <= 4'd3;"),
        "else-branch first action should be hoisted onto the wait-exit edge:\n{sv}");
    assert!(!sv.contains("_t0_state == 3"),
        "fused lowering should not emit old wait->dispatch->prefix state chain:\n{sv}");
}

#[test]
fn test_thread_wait_elsif_chain_fuses_to_single_dispatch() {
    // An `elsif` parses as a nested `else { if ... }`. The wait-dispatch
    // fusion should flatten that chain so later arms do not pay an extra
    // dispatch-only state before their first seq action.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port req: in Bool;
          port sel: in UInt<2>;
          port reg phase: out UInt<4> reset rst => 4'd0;

          thread on clk rising, rst high
            wait until req;
            if sel == 2'd0
              phase <= 4'd1;
              wait 1 cycle;
            elsif sel == 2'd1
              phase <= 4'd2;
              wait 1 cycle;
            else
              phase <= 4'd3;
              wait 1 cycle;
            end if
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);

    assert!(sv.contains("phase <= 4'd1;"),
        "first arm should keep its first action:\n{sv}");
    assert!(sv.contains("phase <= 4'd2;"),
        "elsif arm should keep its first action:\n{sv}");
    assert!(sv.contains("phase <= 4'd3;"),
        "else arm should keep its first action:\n{sv}");
    assert!(sv.contains("req && sel == 2'd0"),
        "first arm guard should include the wait condition:\n{sv}");
    assert!(sv.contains("req && !(sel == 2'd0) && sel == 2'd1"),
        "elsif guard should be flattened onto the original wait state:\n{sv}");
    assert!(sv.contains("req && !(sel == 2'd0) && !(sel == 2'd1)"),
        "else guard should be flattened onto the original wait state:\n{sv}");
    assert!(!sv.contains("_t0_state == 3"),
        "flattened three-arm dispatch should not leave a nested dispatch state:\n{sv}");
}

#[test]
fn test_thread_default_comb_applies_before_state_comb_and_collects_reads() {
    let source = r#"
        module M
          port clk:     in Clock<SysDomain>;
          port rst:     in Reset<Async, Low>;
          port start:   in Bool;
          port ready:   in Bool;
          port done_i:  in Bool;
          port kill:    in Bool;
          port payload: in UInt<8>;
          port valid:   out Bool;
          port data:    out UInt<8>;
          port reg phase: out UInt<4> reset rst => 4'd0;

          thread on clk rising, rst low
            default comb
              valid = false;
              data = payload;
            end default
            default when kill
              phase <= 4'd0;
            end default

            wait until start;
            phase <= 4'd1;
            wait until ready;
            valid = true;
            data = 8'hff;
            wait until done_i;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("input logic [7:0] payload"),
        "`default comb` RHS-only signal must be wired into the lowered thread module:\n{sv}");
    let default_pos = sv.find("data = payload;")
        .expect("expected unconditional default data assignment");
    let state_pos = sv.find("if (_t0_state")
        .expect("expected state-guarded comb assignments");
    assert!(default_pos < state_pos,
        "`default comb` assignments must precede state-specific comb assignments:\n{sv}");
    assert!(sv.contains("data = 8'd255;"),
        "state-specific comb assignment should still override the default later in the block:\n{sv}");
}

#[test]
fn test_thread_default_comb_rejects_seq_driven_target() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port reg done: out Bool reset rst => false;

          thread on clk rising, rst low
            default comb
              done = false;
            end default
            done <= true;
            wait 1 cycle;
          end thread
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let err = elaborate::lower_threads(ast).expect_err("default comb must not drive seq target");
    let msg = format!("{err:?}");
    assert!(msg.contains("default comb") && msg.contains("done") && msg.contains("<="),
        "expected targeted diagnostic for default-comb/seq-driver conflict, got: {msg}");
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
fn test_sim_codegen_match_arm_let_bound_ident_emits_case_with_literal() {
    // Regression: pre-fix, archsim's Stmt::Match emitted EVERY
    // `Pattern::Ident` arm as `default:`. With multiple let-bound
    // operator constants used as match arms (e.g. `ALU_ADD => ...;
    // ALU_SUB => ...;`), C++ rejected with "multiple default labels
    // in one switch". Post-fix, an Ident pattern naming a module-scope
    // let-binding with a literal RHS folds to `case <literal>:`.
    //
    // Bug origin: arch-ibex IbexAlu unique-match conversion attempt;
    // see memory/feedback_archsim_match_pattern_ident_default_collision.md.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          let OP_A: UInt<3> = 3'd0;
          let OP_B: UInt<3> = 3'd1;
          let OP_C: UInt<3> = 3'd2;
          port opc: in UInt<3>;
          port out: out UInt<8>;
          comb
            unique match opc
              OP_A => out = 8'hAA;
              OP_B => out = 8'hBB;
              OP_C => out = 8'hCC;
              _    => out = 8'h00;
            end match
          end comb
        end module M
    ";
    let cpp = compile_to_sim_h(source, false);
    // Each let-bound ident arm should produce a real `case` label.
    // Post-fix the case values are the let RHS literals (folded by
    // cpp_expr) — exact textual form depends on cpp_expr's literal
    // emit, but it must NOT be `default:` for the three arms, and
    // must contain the three values 0/1/2.
    let default_count = cpp.matches("default:").count();
    assert!(default_count <= 1,
        "fix means only the wildcard arm emits `default:`; got {default_count}\n{cpp}");
    // All three case values should appear as case labels.
    for (n, lit) in [(0, "case 0"), (1, "case 1"), (2, "case 2")] {
        assert!(cpp.contains(lit) || cpp.contains(&format!("case {n}u")) ,
            "let-bound ident arm should emit `case {n}` for value {n}:\n{cpp}");
    }
    // The arm bodies should be paired with their case values, not
    // collapsed onto a single default. Search for the body marker.
    assert!(cpp.contains("0xAA") || cpp.contains("170"));
    assert!(cpp.contains("0xBB") || cpp.contains("187"));
    assert!(cpp.contains("0xCC") || cpp.contains("204"));
}

#[test]
fn test_sim_codegen_match_arm_local_param_const_emits_case_with_literal() {
    // Companion to the let-bound regression above. Operator-decoder
    // constants that need to be SV `localparam` (so caller can't
    // override) are declared as `local param X[hi:lo]: const = N`,
    // not `let`. Pre-fix, archsim's Stmt::Match emit folded only
    // module-scope let bindings — params with literal defaults
    // collapsed back to `default:`, re-introducing the multi-default
    // C++ error. Post-fix the same fold applies to literal-default
    // params, so `local param` operator decoders compile cleanly.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          local param OP_A[2:0]: const = 3'd0;
          local param OP_B[2:0]: const = 3'd1;
          local param OP_C[2:0]: const = 3'd2;
          port opc: in UInt<3>;
          port out: out UInt<8>;
          comb
            unique match opc
              OP_A => out = 8'hAA;
              OP_B => out = 8'hBB;
              OP_C => out = 8'hCC;
              _    => out = 8'h00;
            end match
          end comb
        end module M
    ";
    let cpp = compile_to_sim_h(source, false);
    let default_count = cpp.matches("default:").count();
    assert!(default_count <= 1,
        "literal-default `local param` arms must fold to `case <lit>:`, not `default:`; got {default_count}\n{cpp}");
    for (n, lit) in [(0, "case 0"), (1, "case 1"), (2, "case 2")] {
        assert!(cpp.contains(lit) || cpp.contains(&format!("case {n}u")),
            "param-bound ident arm should emit `case {n}`:\n{cpp}");
    }
    assert!(cpp.contains("0xAA") || cpp.contains("170"));
    assert!(cpp.contains("0xBB") || cpp.contains("187"));
    assert!(cpp.contains("0xCC") || cpp.contains("204"));
}

#[test]
fn test_sim_codegen_concat_with_local_param_uses_declared_width() {
    // Companion to the match-arm `local param` test. Pre-fix, `build_widths`
    // and `collect_wide_names` only walked ports + regs + wires + let
    // bindings — module-level params were excluded. `infer_expr_width`
    // then fell back to its 8-bit default for any concat / shift
    // expression that named a param, emitting bit offsets one position
    // wider than the param's declared width. The result was a silent
    // 1-bit gap in the emitted C++ where the next concat element
    // should start.
    //
    // Bug origin: arch-ibex IbexCompressedDecoder OPCODE_* conversion
    // from `let` to `local param` produced wrong `instr_o` values for
    // every compressed-instruction expansion that placed an opcode in
    // an `instr_d = {imm, rs1, funct3, rd, OPCODE_X}` concat.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          local param OPCODE[6:0]: const = 7'h13;
          port instr_o: out UInt<32>;
          let instr_d: UInt<32> = {20'h12345, 5'h7, OPCODE};
          let instr_o = instr_d;
        end module M
    ";
    let cpp = compile_to_sim_h(source, false);
    // The next concat element after OPCODE (7 bits) must shift by 7,
    // not 8. Look for `<< 7` in the concat — pre-fix it was `<< 8`.
    assert!(cpp.contains("<< 7"),
        "param OPCODE[6:0] must be treated as 7 bits wide in concat offsets; cpp lacks `<< 7`:\n{cpp}");
    // And the 7+5=12-bit boundary should produce `<< 12` for the 20-bit
    // imm field, not `<< 13`.
    assert!(cpp.contains("<< 12"),
        "7-bit OPCODE + 5-bit rd must place 20-bit imm at offset 12, not 13:\n{cpp}");
}

#[test]
fn test_lower_threads_clones_parent_params_into_threads_submodule() {
    // Regression: pre-fix, `lower_module_threads` built the synthetic
    // `_<mod>_threads` submodule with `params: Vec::new()`, so parent-
    // module `local param`s were invisible inside thread bodies. Any
    // thread reference to a parent constant (match arm, comparison,
    // concat) emitted as `use of undeclared identifier <NAME>` in the
    // standalone threads compilation unit.
    //
    // Bug origin: arch-ibex IbexMultdivFast attempting `local param
    // MD_OP_DIV[1:0]: const = 2'd2` — every thread-body `operator_i ==
    // MD_OP_DIV` comparison broke the threads cpp build. The let-form
    // sidesteps this because lets are local to a body and get inlined
    // before any submodule lift.
    //
    // Post-fix the lowering pass clones parent params into the
    // submodule's `params`, so SV emit and archsim see them.
    let source = r#"
        module M
          local param OP_GO[1:0]: const = 2'd2;
          port clk:    in Clock<SysDomain>;
          port rst:    in Reset<Sync, High>;
          port op_i:   in UInt<2>;
          port reg phase: out UInt<4> reset rst => 4'd0;

          thread on clk rising, rst high
            wait until op_i == OP_GO;
            phase <= 4'd1;
            wait 1 cycle;
          end thread
        end module M
    "#;
    // The full module compiles end-to-end. Pre-fix this raised a
    // compile error because the synthetic `_M_threads` SV referenced
    // `OP_GO` without a declaration. Post-fix the submodule has its
    // own `localparam [1:0] OP_GO = 2'd2`.
    let sv = compile_to_sv(source);
    // Parent module still declares OP_GO.
    assert!(sv.contains("localparam [1:0] OP_GO = 2'd2"),
        "parent module must keep its localparam OP_GO:\n{sv}");
    // Synthetic threads submodule should ALSO declare OP_GO, not refer
    // to an undeclared identifier. The emit places submodule decls
    // ahead of the parent module, but both must contain it.
    let occurrences = sv.matches("localparam [1:0] OP_GO = 2'd2").count();
    assert!(occurrences >= 2,
        "both parent and `_M_threads` submodule should declare OP_GO; only {occurrences} occurrence(s):\n{sv}");
    // And the thread body's predicate must reference OP_GO (proves the
    // identifier survived lowering).
    assert!(sv.contains("OP_GO") && sv.matches("OP_GO").count() >= 3,
        "thread body should compare op_i against OP_GO:\n{sv}");
}

#[test]
fn test_thread_sim_declares_module_params_used_by_thread_body() {
    // arch-com#352: the ARCH-native coroutine thread sim path skips
    // lower_threads, so parent params are not cloned into a synthetic FSM
    // module. The thread sim emitter must make them visible to generated
    // C++ methods instead of emitting undeclared identifiers like
    // SCHEDULED_CORE_CYCLES.
    let source = r#"
        module M
          param SCHEDULED_CORE_CYCLES: const = 7;
          param DONE_W: const = SCHEDULED_CORE_CYCLES + 1;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port done: out UInt<DONE_W>;

          thread on clk rising, rst high
            wait SCHEDULED_CORE_CYCLES cycle;
            done = DONE_W;
          end thread
        end module M
    "#;
    let h = compile_to_thread_sim_h(source);
    assert!(h.contains("static constexpr uint64_t SCHEDULED_CORE_CYCLES = 7ULL;"),
        "thread sim header should declare module param constants:\n{h}");
    assert!(h.contains("static constexpr uint64_t DONE_W = 8ULL;"),
        "derived module params should be folded for C++ visibility:\n{h}");
    assert!(h.contains("co_await arch_rt::wait_cycles(&_slot_0, SCHEDULED_CORE_CYCLES);"),
        "wait-cycle expression should keep using the declared constexpr param:\n{h}");
    assert!(h.contains("done = DONE_W;"),
        "thread body should use the declared constexpr param:\n{h}");
    assert!(h.contains("uint8_t done = 0;"),
        "param-derived port widths should resolve in thread sim C++ types:\n{h}");
}

#[test]
fn test_sim_codegen_declares_typed_module_params_used_by_lowered_thread_body() {
    // Typed value params (`param X: UInt<W> = ...`) are valid ARCH params and
    // SV codegen emits them, but the native C++ sim header also has to expose
    // them because lowered thread/TLM bodies may reference the param name.
    let source = r#"
        module M
          param SCHEDULED_CORE_CYCLES: UInt<32> = 32'd7;
          param DONE_W: UInt<32> = SCHEDULED_CORE_CYCLES + 32'd1;
          param CALLS: UInt<16> = 16'd3;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port done: out UInt<DONE_W>;
          reg calls_r: UInt<16> reset rst => 0;

          thread on clk rising, rst high
            wait SCHEDULED_CORE_CYCLES cycle;
            done = 8'd8;
            calls_r <= CALLS;
          end thread
        end module M
    "#;
    let cpp = compile_to_sim_h(source, false);
    assert!(cpp.contains("#define SCHEDULED_CORE_CYCLES 7ULL"),
        "sim header should define typed module params for lowered thread bodies:\n{cpp}");
    assert!(cpp.contains("#define DONE_W 8ULL"),
        "derived typed module params should be folded for C++ visibility:\n{cpp}");
    assert!(cpp.contains("#define CALLS 3ULL"),
        "narrow typed module params should also be emitted:\n{cpp}");
    assert!(cpp.contains("uint8_t done;"),
        "param-derived port widths should resolve in normal sim C++ types:\n{cpp}");
    assert!(cpp.contains("_n_calls_r  = CALLS;"),
        "lowered thread body should keep using the declared C++ param constant:\n{cpp}");
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
fn test_unpacked_ascending_emits_ascending_sv_dim() {
    // `unpacked ascending Vec<T,N>` flips the SV unpacked dim to `[0:N-1]`
    // (vs the default `[N-1:0]` for `unpacked`). Required for interop with
    // upstream SV declared as `logic [W-1:0] x [N]` shorthand (= `[0:N-1]`)
    // — without this, IEEE 1800-2017 §10.10 element-by-position port
    // mapping silently reverses indices at the connection. See arch-com#307.
    let source = r#"
module unpacked_asc_demo
  port asc_in:    in  unpacked ascending Vec<UInt<6>, 4>;
  port asc_out:   out unpacked ascending Vec<UInt<6>, 4>;
  port desc_in:   in  unpacked Vec<UInt<6>, 4>;
  comb
    asc_out[0] = asc_in[0];
    asc_out[3] = asc_in[3];
  end comb
end module unpacked_asc_demo
"#;
    let sv = compile_to_sv(source);
    // `ascending` flips both directions.
    assert!(sv.contains("input logic [5:0] asc_in [0:3]"),
            "ascending unpacked input should emit [0:N-1], got: {sv}");
    assert!(sv.contains("output logic [5:0] asc_out [0:3]"),
            "ascending unpacked output should emit [0:N-1], got: {sv}");
    // Plain `unpacked` (no `ascending`) keeps default descending.
    assert!(sv.contains("input logic [5:0] desc_in [3:0]"),
            "plain unpacked stays descending, got: {sv}");
    // ARCH-side indexing is unchanged — `asc_in[0]` is still the first
    // element regardless of SV dim direction.
    assert!(sv.contains("assign asc_out[0] = asc_in[0]"));
    assert!(sv.contains("assign asc_out[3] = asc_in[3]"));
}

#[test]
fn test_unpacked_ascending_wire_emits_ascending() {
    // `wire ... unpacked ascending Vec<T,N>;` — same flip on a wire
    // declaration so the wire mates with an ascending port (or upstream
    // SV `[N]` array) by-index without reversal. arch-com#307.
    let source = r#"
module asc_wire_demo
  port out_o: out unpacked ascending Vec<UInt<6>, 4>;
  wire w: unpacked ascending Vec<UInt<6>, 4>;
  comb
    w[0]     = 6'd1;
    w[1]     = 6'd2;
    w[2]     = 6'd3;
    w[3]     = 6'd4;
    out_o[0] = w[0];
    out_o[1] = w[1];
    out_o[2] = w[2];
    out_o[3] = w[3];
  end comb
end module asc_wire_demo
"#;
    let sv = compile_to_sv(source);
    assert!(sv.contains("logic [5:0] w [0:3]"),
            "ascending unpacked wire should emit [0:N-1], got: {sv}");
}

#[test]
fn test_unpacked_ascending_archi_emit() {
    // `.archi` interface stub must preserve the `ascending` keyword so
    // downstream consumers see the same shape as the .sv. arch-com#307.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module AscIface
  port asc_in: in unpacked ascending Vec<UInt<6>, 4>;
end module AscIface
";
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let item = parsed.items.iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("expected a module item");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(body.contains("port asc_in: in unpacked ascending Vec"),
            ".archi should preserve `unpacked ascending`: {body}");
}

#[test]
fn test_archi_registered_output_emits_pipe_reg_signature() {
    // `.archi` is the cross-file contract, so registered output latency should
    // be visible there even when the source used the deprecated `port reg`
    // spelling.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module PipeIface
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port reg legacy_q: out UInt<8> reset rst => 0;
  port modern_q: out pipe_reg<SInt<16>, 2> reset rst => 0;
end module PipeIface
";
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let item = parsed.items.iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("expected a module item");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(
        body.contains("port legacy_q: out pipe_reg<UInt<8>, 1> reset rst => 0;"),
        ".archi should canonicalize legacy port reg to pipe_reg<T,1>: {body}"
    );
    assert!(
        body.contains("port modern_q: out pipe_reg<SInt<16>, 2> reset rst => 0;"),
        ".archi should preserve pipe_reg latency: {body}"
    );
    assert!(
        !body.contains("port reg"),
        ".archi should not emit deprecated port reg spelling: {body}"
    );
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
fn test_rdc_guard_waiver_async_same_domain_passes() {
    // Issue #260: a reset-none data register annotated with `guard
    // VALID_SIG`, where VALID_SIG is async-reset on the same domain
    // whose data the reset-none reg captures, should NOT raise an
    // RDC violation. Downstream readers structurally ignore the data
    // when VALID_SIG is low, so the metastability hazard during reset
    // deassertion is contained.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Async, Low>;
  port d:   in UInt<8>;
  port q:   out UInt<8>;
  port v_in: in Bool;
  // valid_q is async-reset to false (off-value); data_q is unreset
  // and guarded by valid_q. RDC waiver applies.
  reg valid_q: Bool reset rst => false;
  reg data_q: UInt<8> guard valid_q reset none;
  seq on clk rising
    valid_q <= v_in;
    data_q  <= d;
  end seq
  let q = data_q;
end module M
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_ok(),
        "expected guard-waivered RDC to pass, got: {:?}", result.err());
}

#[test]
fn test_rdc_guard_waiver_does_not_apply_when_guard_is_sync_reset() {
    // The guard waiver requires the guard signal to be ASYNC-reset.
    // A sync-reset guard doesn't structurally gate the deassertion
    // window, so the metastability hazard remains. The error should
    // include a hint pointing at the qualifying-guard form.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst_async: in Reset<Async, Low>;
  port rst_sync:  in Reset<Sync,  Low>;
  port d:   in UInt<8>;
  port q:   out UInt<8>;
  port v_in: in Bool;
  // Async source flop; reset-none data with a SYNC-reset guard ⇒
  // waiver shouldn't apply.
  reg src_q: UInt<8> reset rst_async => 0;
  reg valid_q: Bool reset rst_sync => false;
  reg data_q: UInt<8> guard valid_q reset none;
  seq on clk rising
    src_q <= d;
    valid_q <= v_in;
    data_q  <= src_q;
  end seq
  let q = data_q;
end module M
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected RDC error (sync guard doesn't qualify)");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("RDC violation") && s.contains("data_q")
                && s.contains("not async-reset")
        }),
        "expected hint about non-async guard, got: {:?}", errs
    );
}

#[test]
fn test_rdc_guard_waiver_does_not_apply_when_guard_is_port_input() {
    // The guard waiver requires the guard signal to be a register in
    // THIS module — a port input doesn't carry a known reset behavior
    // for the local checker (cross-module verification is out of
    // scope per issue #260). Should still fail with a hint.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Async, Low>;
  port d:   in UInt<8>;
  port valid_in: in Bool;
  port q:   out UInt<8>;
  reg src_q: UInt<8> reset rst => 0;
  reg data_q: UInt<8> guard valid_in reset none;
  seq on clk rising
    src_q  <= d;
    data_q <= src_q;
  end seq
  let q = data_q;
end module M
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected RDC error (port guard doesn't qualify)");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("RDC violation") && s.contains("data_q")
                && s.contains("not a register in this module")
        }),
        "expected hint about port-input guard, got: {:?}", errs
    );
}

#[test]
fn test_rdc_guard_waiver_does_not_apply_when_guard_is_diff_domain() {
    // The guard waiver waives only the SAME async reset domain. If
    // the data path crosses a different async domain, the local
    // guard's reset doesn't help — should still fail.
    let source = r#"
domain DomA
  freq_mhz: 100
end domain DomA
domain DomB
  freq_mhz: 200
end domain DomB
module M
  port clk_a: in Clock<DomA>;
  port clk_b: in Clock<DomB>;
  port rst_a: in Reset<Async, Low>;
  port rst_b: in Reset<Async, Low>;
  port d:     in UInt<8>;
  port v_in:  in Bool;
  port q:     out UInt<8>;
  // src_q is in rst_b's domain; valid_q is async-reset on rst_a;
  // data_q reads src_q (rst_b domain) → guard rst_a doesn't cover
  // this crossing.
  reg src_q: UInt<8> reset rst_b => 0;
  reg valid_q: Bool reset rst_a => false;
  reg data_q: UInt<8> guard valid_q reset none;
  seq on clk_b rising
    src_q <= d;
  end seq
  seq on clk_a rising
    valid_q <= v_in;
    data_q  <= src_q;
  end seq
  let q = data_q;
end module M
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected RDC error (cross-domain guard doesn't waive)");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("RDC violation") && s.contains("data_q")
        }),
        "expected RDC error on data_q, got: {:?}", errs
    );
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
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
    let sv = codegen.generate();
    assert!(sv.contains("module Parent"), "parent module should be emitted");
    assert!(!sv.contains("module ChildStub"),
        "interface stub must not be emitted to SV (real impl lives in a separately-built file)");
}

#[test]
fn test_archi_interface_stub_for_fsm_skips_body_only_passes() {
    // Same scenario as `test_archi_interface_stub_skips_body_only_passes`
    // but for an `fsm`-typed sub-instance (B5 arch-ibex case: a parent
    // module instantiates a previously-ported `fsm`, so the dep loader
    // pulls in the `fsm Name ... end fsm Name` body-less stub from
    // `<name>.archi`). Pre-fix, the parser rejected the stub with
    // "fsm requires `default state Name;`" (the rule only the real fsm
    // needs); post-fix the parser accepts a missing default_state, the
    // post-parse tagger sets `is_interface = true`, and resolve /
    // typecheck / codegen / sim_codegen all skip body-only passes.
    let source = r#"
domain Sys
  freq_mhz: 100
end domain Sys

fsm ChildFsm
  port clk_i: in Clock<Sys>;
  port rst_ni: in Reset<Async, Low>;
  port in_i: in UInt<8>;
  port out_o: out UInt<8>;
end fsm ChildFsm

module Parent
  port clk_i: in Clock<Sys>;
  port rst_ni: in Reset<Async, Low>;
  port result: out UInt<8>;

  wire w: UInt<8>;
  inst c: ChildFsm
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
    let mut parsed_ast = parser.parse_source_file()
        .expect("parser must accept body-less fsm (interface stub)");
    // Mimic main.rs's post-parse tagger: items loaded from `.archi` get
    // is_interface = true. Here we tag the FSM by name to simulate
    // "loaded from <name>.archi".
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Fsm(f) = item {
            if f.name.name == "ChildFsm" { f.common.is_interface = true; }
        }
    }
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target lowering");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator lowering");
    let ast = elaborate::lower_threads_with_opts(ast, &elaborate::ThreadLowerOpts::default())
        .expect("lower_threads");
    let ast = elaborate::lower_pipe_reg_ports(ast).expect("lower_pipe_reg_ports");
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit_channel dispatch");
    let symbols = resolve::resolve(&ast)
        .expect("resolve must accept fsm interface stub (no default_state validation)");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check()
        .expect("typecheck must skip body checks on fsm interface stub");
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
    let sv = codegen.generate();
    assert!(sv.contains("module Parent"), "parent module should be emitted");
    assert!(!sv.contains("module ChildFsm"),
        "fsm interface stub must not be emitted to SV (real impl lives in a separately-built file)");
}

#[test]
fn test_real_fsm_still_requires_default_state() {
    // Mirror to the stub test: a *real* (non-interface) fsm without
    // `default state Name;` must still be rejected. Pre-fix the error
    // came from parser; post-fix it comes from `resolve.rs`. The user
    // diagnostic is preserved verbatim.
    let source = r#"
domain Sys
  freq_mhz: 100
end domain Sys

fsm BrokenFsm
  port clk_i: in Clock<Sys>;
  port rst_ni: in Reset<Async, Low>;
  state [Idle, Run]
  state Idle
    -> Run when 1'b1;
  end state Idle
  state Run
    -> Idle when 1'b1;
  end state Run
end fsm BrokenFsm
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file()
        .expect("parser accepts body-less fsm now; default-state check moved to resolve");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let resolve_err = resolve::resolve(&ast).err()
        .expect("real fsm without default_state must still error");
    let msg = format!("{resolve_err:?}");
    assert!(msg.contains("default state"),
        "expected `default state` diagnostic, got: {msg}");
}

#[test]
fn test_fsm_use_package_emits_sv_import() {
    // `emit_fsm` mirrors `emit_module`'s import-emission block so that an
    // `fsm` consuming a `package` via `use Pkg;` produces the SV
    // `import Pkg::*;` line. Without it, the emitted SV references the
    // package's typedefs/enums by bare name with no import, and the
    // downstream Verilator job can't resolve them. (Showed up in
    // arch-ibex B5: `IbexController` is an `fsm` that `use IbexPkg;`
    // for shared `ExcCause` / `Irqs` types.)
    let source = r#"
package SharedPkg
  enum Color
    Red, Green, Blue
  end enum Color
end package SharedPkg

use SharedPkg;

domain Sys
  freq_mhz: 100
end domain Sys

fsm Painter
  port clk_i: in Clock<Sys>;
  port rst_ni: in Reset<Async, Low>;
  port hue_o: out Color;

  state [Idle, Working]

  default state Idle;

  state Idle
    comb
      hue_o = Red;
    end comb
    -> Working when 1'b1;
  end state Idle

  state Working
    comb
      hue_o = Green;
    end comb
    -> Idle when 1'b1;
  end state Working
end fsm Painter
"#;
    let sv = compile_to_sv(source);
    // Package is emitted up front (no change here — that's package codegen).
    assert!(sv.contains("package SharedPkg;"),
        "package SharedPkg should be emitted to SV");
    assert!(sv.contains("import SharedPkg::*;"),
        "fsm consumer of `use SharedPkg;` must emit `import SharedPkg::*;` so the module body can reference `Color` without a fully-qualified name");
    // Sanity: the import comes BEFORE the module declaration so the type
    // ref inside the module header resolves cleanly.
    let import_pos = sv.find("import SharedPkg::*;").expect("import present");
    let module_pos = sv.find("module Painter").expect("module present");
    assert!(import_pos < module_pos,
        "`import SharedPkg::*;` must precede `module Painter` (otherwise the port-list type ref doesn't resolve)");
}

#[test]
fn test_fsm_use_non_package_does_not_emit_import() {
    // Symmetric to `test_use_bus_does_not_emit_sv_import` for fsm. `use`
    // targets that are NOT packages (bus, module, fsm, ...) are
    // compile-time references; emitting `import` for them would yield
    // SV that doesn't compile because no SV `package <Name>;` exists.
    let source = r#"
domain Sys
  freq_mhz: 100
end domain Sys

bus SimpleBus
  valid_o: out Bool;
  data_o: out UInt<8>;
end bus SimpleBus

use SimpleBus;

fsm Sender
  port clk_i: in Clock<Sys>;
  port rst_ni: in Reset<Async, Low>;
  port out_o: out Bool;
  state [Idle]
  default state Idle;
  state Idle
    comb
      out_o = 1'b0;
    end comb
    -> Idle when 1'b1;
  end state Idle
end fsm Sender
"#;
    let sv = compile_to_sv(source);
    assert!(!sv.contains("import SimpleBus::*;"),
        "fsm `use` of a bus (not a package) must NOT emit a SV import: there's no package to import from. Got SV:\n{sv}");
}

#[test]
fn test_thread_inter_yield_seq_assigns_get_dead_skid() {
    // Spec §7a.2 line 1677: only TRAILING seq assigns (after the last wait
    // in the body) may merge into the preceding state's exit logic.
    // Inter-yield seq assigns — assigns sitting BETWEEN two yield
    // statements — are NOT trailing and must each get their own dead-skid
    // state with unconditional advance.
    //
    // Pre-fix: the WaitUntil handler in lower_threads merged ALL pending
    // seq assigns (not just trailing) into the next wait state, guarded
    // by that wait's condition. This was a documented-as-intentional but
    // spec-incompatible behavior — the "intent" was to support
    // `if cond { reg <= ...; } wait until cond;` (capture-on-cond-edge),
    // but it changed the semantics of every plain inter-yield assign,
    // making them fire one cycle late AND conditionally on the next
    // wait's cond.
    //
    // Post-fix: the inter-yield assigns get a dead-skid state with
    // unconditional advance. For capture-on-edge of a wait condition,
    // users should use `do { if cond { reg <= ...; } } until cond;`
    // (the do-until body runs while waiting, including the exit cycle).
    let source = r#"
domain Sys
  freq_mhz: 100
end domain Sys

module M
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port cond_a: in Bool;
  port cond_b: in Bool;
  port out_b: out Bool shared(or);
  reg x: UInt<8> reset rst => 8'd0;
  reg y: UInt<8> reset rst => 8'd0;

  thread T on clk rising, rst high
    do
      out_b = false;
    until cond_a;
    x <= 8'd42;
    y <= 8'd99;
    wait until cond_b;
    out_b = true;
  end thread T
end module M
"#;
    let sv = compile_to_sv(source);

    // The inter-yield seq assigns must lower as an unconditional dead-skid
    // (NOT guarded by cond_b). Pre-fix the assign was wrapped in
    // `if (cond_b) begin x <= 8'd42; ... end`.
    //
    // Find the assign and look at the immediately enclosing `begin` block
    // by scanning backwards for the most recent `begin` token. Whichever
    // line that `begin` sits on must NOT contain `if (cond_b)` for the
    // assign to be unconditional.
    let pos = sv.find("x <= 8'd42").unwrap_or_else(|| {
        panic!("assign `x <= 8'd42` not found in SV:\n{}", sv);
    });
    let preceding = &sv[..pos];
    let last_begin_at = preceding.rfind("begin").unwrap_or_else(|| {
        panic!("no `begin` precedes the assign; SV malformed:\n{}", sv);
    });
    // Extract the line containing that `begin`.
    let line_start = preceding[..last_begin_at].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let begin_line = &preceding[line_start..last_begin_at + "begin".len()];
    assert!(!begin_line.contains("if (cond_b)"),
        "inter-yield seq assign `x <= 8'd42` must NOT be wrapped in \
         `if (cond_b) begin ... end` — that's the pre-fix merge-into-wait-state \
         behavior, which conflicts with spec §7a.2 (only TRAILING assigns merge). \
         Enclosing begin-line was: {:?}\nFull SV:\n{}", begin_line, sv);
}

#[test]
fn test_unpacked_wire_modifier_emits_unpacked_sv() {
    // Issue #267: `unpacked` modifier on internal wire/let declarations.
    // A `wire foo: unpacked Vec<T,N>` mirrors the existing `unpacked Vec`
    // port modifier (§3.7) — emits SV unpacked-array shape so the wire can
    // mate with an `unpacked Vec` port across an `inst` connection without
    // Verilator rejecting the packed/unpacked shape mismatch.
    let source = "
        module Leaf
          port q: in unpacked Vec<UInt<32>, 2>;
          port o: out UInt<32>;
          comb
            o = q[0] ^ q[1];
          end comb
        end module Leaf

        module Parent
          port pq: in unpacked Vec<UInt<32>, 2>;
          port po: out UInt<32>;
          wire bridge: unpacked Vec<UInt<32>, 2>;
          comb
            bridge[0] = pq[0];
            bridge[1] = pq[1];
          end comb
          inst leaf: Leaf
            q <- bridge;
            o -> po;
          end inst leaf
        end module Parent
    ";
    let sv = compile_to_sv(source);
    // Wire emits unpacked-array shape, not packed multi-dim.
    assert!(sv.contains("logic [31:0] bridge [1:0]") || sv.contains("logic [31:0] bridge [0:1]"),
        "expected unpacked wire shape `logic [31:0] bridge [N-1:0]`, got:\n{}", sv);
    assert!(!sv.contains("logic [1:0][31:0] bridge"),
        "must NOT emit packed multi-dim for `unpacked` wire, got:\n{}", sv);
    // Parent's port still uses unpacked (sanity).
    assert!(sv.contains("input logic [31:0] pq [1:0]") || sv.contains("input logic [31:0] pq [0:1]"),
        "expected unpacked port shape on parent, got:\n{}", sv);
}

#[test]
fn test_cam_emits_archi_interface() {
    // Regression: arch-ibex Phase D spike surfaced that CamDecl had
    // no `iface` clause in its impl_construct_via_common, so cam
    // declarations never produced a `.archi` interface stub. Other
    // first-class constructs (fsm, fifo, ram, counter, arbiter,
    // regfile, pipeline, linklist) all have one. Adds parity.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

cam TestCam
  param DEPTH: const = 8;
  param KEY_W: const = 12;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port write_valid: in Bool;
  port write_idx:   in UInt<3>;
  port write_key:   in UInt<12>;
  port write_set:   in Bool;
  port search_key:   in  UInt<12>;
  port search_mask:  out UInt<8>;
  port search_any:   out Bool;
  port search_first: out UInt<3>;
end cam TestCam
";
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let item = parsed.items.iter()
        .find(|i| matches!(i, arch::ast::Item::Cam(_)))
        .expect("expected a cam item");
    let body = arch::interface::emit_interface(item)
        .expect("cam should now emit an .archi interface");
    assert!(body.starts_with("cam TestCam\n"), "body: {body}");
    assert!(body.contains("param DEPTH: const = 8;"), "body: {body}");
    assert!(body.contains("port search_first: out UInt<3>;"), "body: {body}");
    assert!(body.ends_with("end cam TestCam\n"), "body: {body}");
}

#[test]
fn test_arbiter_archi_reflects_per_requester_ports_array() {
    // Regression: arbiter `.archi` previously dropped the `ports[N]`
    // group, so a downstream consumer reading the .archi to write an
    // inst connection couldn't see what the per-requester signals
    // are. Phase D's icache port relies on arbiter inst — exposing
    // the group in the .archi is required.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter TestArb
  policy round_robin;
  param NUM_REQ: const = 3;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter TestArb
";
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let item = parsed.items.iter()
        .find(|i| matches!(i, arch::ast::Item::Arbiter(_)))
        .expect("expected an arbiter item");
    let body = arch::interface::emit_interface(item)
        .expect("arbiter should emit an .archi interface");
    assert!(body.contains("ports[NUM_REQ] request"),
            ".archi must include the ports[N] group: {body}");
    assert!(body.contains("    valid: in Bool;"),
            ".archi must include per-requester valid signal: {body}");
    assert!(body.contains("    ready: out Bool;"),
            ".archi must include per-requester ready signal: {body}");
    assert!(body.contains("  end ports request"),
            ".archi ports group must be properly closed: {body}");
}

#[test]
fn test_arbiter_inst_synthesizes_per_requester_vector_wire() {
    // Regression: when a parent instantiates an arbiter and writes
    // `request[0].valid <- a;` per-index connections, the parser flat-
    // tens to `port_name = "request0_valid"`. The arbiter's SV port
    // is a vector `request_valid [N-1:0]`, so the inst-site previously
    // emitted `.request0_valid(...)` — a non-existent SV port name.
    //
    // Fix: emit_inst now detects the per-index pattern, synthesizes a
    // hidden vector wire `__<inst>_<group>_<sig>`, drives each bit
    // from the user's expression, and connects the whole wire to the
    // module's vector port.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter MyArb
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter MyArb

module Parent
  port clk_i: in Clock<SysDomain>;
  port rst_ni: in Reset<Sync, High>;
  port req0: in Bool;
  port req1: in Bool;
  port req2: in Bool;
  port req3: in Bool;
  port grant_valid_o: out Bool;
  port grant_idx_o: out UInt<2>;

  inst arb: MyArb
    clk <- clk_i;
    rst <- rst_ni;
    request[0].valid <- req0;
    request[1].valid <- req1;
    request[2].valid <- req2;
    request[3].valid <- req3;
    grant_valid <- false;
    grant_requester <- 2'd0;
  end inst arb

  let _u_grant_v: Bool = false;
  let _u_grant_i: UInt<2> = 2'd0;
  comb
    grant_valid_o = false;
    grant_idx_o = 2'd0;
  end comb
end module Parent
";
    let sv = compile_to_sv(source);
    // Synthesized wire is declared.
    assert!(sv.contains("logic [3:0] __arb_request_valid;"),
            "expected synthesized vector wire: {sv}");
    // Each bit of the wire is driven from the user's per-index
    // expression.
    assert!(sv.contains("assign __arb_request_valid[0] = req0;"),
            "expected per-index drive [0]: {sv}");
    assert!(sv.contains("assign __arb_request_valid[3] = req3;"),
            "expected per-index drive [3]: {sv}");
    // The whole vector is connected to the inst's `request_valid` port.
    assert!(sv.contains(".request_valid(__arb_request_valid)"),
            "expected whole-vector connection: {sv}");
    // The non-existent flattened port names must NOT appear.
    assert!(!sv.contains(".request0_valid("),
            "must not emit per-index port name: {sv}");
    assert!(!sv.contains(".request3_valid("),
            "must not emit per-index port name: {sv}");
}

#[test]
fn test_ram_user_param_propagates_to_sv_header() {
    // Regression: ram codegen previously emitted only DEPTH +
    // DATA_WIDTH parameters in the SV header, dropping any user-
    // declared `param FOO: const = N;` declarations. But port type
    // expressions like `wdata: in UInt<TagW>` continued to emit
    // `[TagW-1:0]` — referencing an undeclared SV variable.
    //
    // Fix: emit user params (Const / WidthConst) alongside the
    // standard pair, AND derive DATA_WIDTH from the store_var's
    // element type when WIDTH isn't a `type` param.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

ram TagRam
  kind single;
  latency 1;
  param DEPTH: const = 64;
  param TagW: const = 22;
  port clk: in Clock<SysDomain>;
  store
    buf: Vec<UInt<TagW>, DEPTH>;
  end store
  ports rw
    en:    in Bool;
    wen:   in Bool;
    addr:  in UInt<6>;
    wdata: in UInt<TagW>;
    rdata: out UInt<TagW>;
  end ports rw
end ram TagRam
";
    let sv = compile_to_sv(source);
    // SV header includes the user param.
    assert!(sv.contains("parameter int TagW = 22"),
            "user param TagW must appear in SV header: {sv}");
    // DATA_WIDTH is derived from the store element type. Default may
    // be the symbolic param `TagW` (forward-resolves via the user
    // param decl above) or the literal `22`; either is correct SV.
    assert!(sv.contains("parameter int DATA_WIDTH = TagW")
         || sv.contains("parameter int DATA_WIDTH = 22"),
            "DATA_WIDTH should follow the store element width: {sv}");
    // Port refs to TagW now resolve.
    assert!(sv.contains("[TagW-1:0]"),
            "port type must keep referencing TagW: {sv}");
}

#[test]
fn test_doc_comment_above_local_param_parses() {
    // Regression: arch-ibex C2 surfaced that a `///` doc comment
    // immediately preceding a `local param` declaration confused the
    // parser's `check_param` lookahead — it didn't skip doc-comment
    // tokens when looking past `local` for the `param` keyword, so
    // the body-item dispatcher rejected `local` as an unknown item.
    // After the fix, doc comments attach cleanly to `local param`s
    // the same way they attach to plain `param`s.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module DocLocal
  param Foo: const = 32;
  /// Doc comment that should attach to the `local param` below.
  /// Multi-line is also fine.
  local param Bar: const = 64;
  port clk: in Clock<SysDomain>;
  port d: in UInt<32>;
  port q: out UInt<32>;
  comb
    q = d;
  end comb
end module DocLocal
";
    let sv = compile_to_sv(source);
    assert!(sv.contains("parameter int Foo = 32"),
            "regular param should still emit: {sv}");
    assert!(sv.contains("localparam int Bar = 64"),
            "local param after doc comment should still emit: {sv}");
}

#[test]
fn test_unpacked_modifier_preserved_in_archi_emit() {
    // Regression: arch-ibex C2 surfaced that the `.archi` interface
    // emit for `port name: in unpacked Vec<T, N>` dropped the
    // `unpacked` keyword, so downstream consumers reading the
    // `.archi` to resolve port shape silently saw packed-Vec when
    // the source was unpacked. The .sv emit was correct; the .archi
    // was the one that was wrong.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module UpkPort
  param N: const = 4;
  param W: const = 8;
  port a: in  unpacked Vec<UInt<W>, N>;
  port b: out unpacked Vec<UInt<W>, N>;
  port c: in  Vec<UInt<W>, N>;
  comb
    b = a;
  end comb
end module UpkPort
";
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let item = parsed.items.iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("expected a module item");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(body.contains("port a: in unpacked Vec<UInt<W>, N>;"),
            "unpacked input port should round-trip into .archi: {body}");
    // Issue #246 Phase 2: output ports may pick up a `comb_dep_on(...)`
    // suffix listing the precise input ports that feed each output.
    assert!(body.contains("port b: out unpacked Vec<UInt<W>, N>"),
            "unpacked output port should round-trip into .archi: {body}");
    // Packed Vec port (no `unpacked` modifier) still emits without it.
    assert!(body.contains("port c: in Vec<UInt<W>, N>;"),
            "packed Vec port must NOT gain the `unpacked` keyword: {body}");
    assert!(!body.contains("port c: in unpacked Vec"),
            "packed Vec port must NOT gain the `unpacked` keyword: {body}");
}

#[test]
fn test_unpacked_wire_rejected_on_non_vec() {
    // The modifier is only valid on `Vec<T,N>`. Other types must error.
    let source = "
        module M
          wire bad: unpacked UInt<32>;
        end module M
    ";
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let err = parser.parse_source_file().expect_err("must reject `unpacked UInt<32>`");
    let msg = format!("{:?}", err);
    assert!(msg.contains("unpacked") && msg.contains("Vec"),
        "error should mention the `unpacked` + Vec constraint, got: {}", msg);
}

#[test]
fn test_emit_bound_asserts_elides_for_loop_iterator_index() {
    // Regression: pre-fix, a `for fb in 0..3` with `vec[fb] <= ...`
    // inside a seq block emitted a module-scope concurrent assertion
    // `_auto_bound_vec_0: assert property (... int'(fb) < (4))` —
    // referencing the for-loop iterator `fb` outside the for-loop's
    // SV scope. Verilator rejected the SV with "Can't find definition
    // of variable: 'fb'".
    //
    // Post-fix, indices that are bare identifiers naming an in-scope
    // for-loop iterator skip the bound assertion (the iterator is
    // statically bounded by the loop range, so the check is redundant
    // and the auto-emitted SV doesn't compile).
    //
    // Origin: arch-ibex IbexIcache 1-FSM rewrite (2026-05-07
    // followup), `for fb in 0..3 phase_q[fb] <= ...; addr_q[fb] <= ...`.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          port clk:    in Clock<SysDomain>;
          port rst_ni: in Reset<Async, Low>;
          port we:     in Bool;
          port d:      in UInt<8>;
          reg q: Vec<UInt<8>, 4> reset rst_ni => 8'd0;
          seq on clk rising
            if we
              for fb in 0..3
                q[fb] <= d;
              end for
            end if
          end seq
        end module M
    ";
    let sv = compile_to_sv(source);
    // No bound assertion should reference `fb`. Pre-fix the SV had
    // `_auto_bound_vec_0: assert property (... int'(fb) < (4))`.
    assert!(!sv.contains("int'(fb)"),
        "for-loop iterator `fb` should NOT appear in any bound assertion (lives in inner scope only):\n{sv}");
    // Sanity: bound-assertion block should be absent entirely (no
    // other Vec writes here), confirming the for-loop iterator was
    // the only candidate and it was correctly elided.
    assert!(!sv.contains("_auto_bound_vec_"),
        "no bound assertion expected when the only Vec index is a for-loop iterator:\n{sv}");
}

#[test]
fn test_codegen_vec_uint1_collapses_inner_zero_dim() {
    // Regression: pre-fix, `Vec<UInt<1>, N>` ports emitted as
    // `logic [N-1:0] [0:0] x` (multi-dim packed). When such a port
    // connects to an upstream-SV `logic [N-1:0]` (single-dim packed),
    // yosys-slang's elaboration could mis-resolve the multi-dim
    // form's bit-by-position mapping at module boundaries, leading
    // to silent constant-propagation that DCE'd downstream cells.
    //
    // Origin: arch-ibex IbexIcache `ic_tag_req_o` / `ic_data_req_o`
    // (`Vec<UInt<1>, 2>`) connecting to IbexTop wires of upstream
    // shape `logic [IC_NUM_WAYS-1:0]`. After full SoC `synth -flatten`,
    // 4 prim_ram_1p RAM banks (~22k FFs of memory) were eliminated
    // entirely from the netlist.
    //
    // Fix: when the inner element of a Vec is 1-bit (`UInt<1>`), the
    // redundant `[0:0]` inner dim is collapsed. `Vec<UInt<1>, N>` now
    // emits as `logic [N-1:0] x`. Same bit-level meaning, cleaner
    // SV form, no mis-resolution at boundaries.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          port en_v: out Vec<UInt<1>, 4>;
          port mask: out Vec<Bool, 4>;
          comb
            en_v[0] = 1'd1;
            en_v[1] = 1'd0;
            en_v[2] = 1'd1;
            en_v[3] = 1'd0;
            mask[0] = true;
            mask[1] = false;
            mask[2] = true;
            mask[3] = false;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // `Vec<UInt<1>, 4>` should emit as single-dim packed `logic [3:0]`.
    assert!(sv.contains("output logic [3:0] en_v"),
        "Vec<UInt<1>, 4> should emit as `logic [3:0]` (no inner [0:0]):\n{sv}");
    assert!(!sv.contains("[3:0] [0:0]"),
        "no `[N-1:0] [0:0]` multi-dim form expected for Vec<UInt<1>, _>:\n{sv}");
    // Vec<Bool, 4> behaves the same (Bool is 1-bit) — sanity check the
    // emission stays single-packed (was always `logic [3:0]` pre-fix
    // because Bool's emit_type_str returns just `logic`).
    assert!(sv.contains("output logic [3:0] mask"),
        "Vec<Bool, 4> should still emit as `logic [3:0]`:\n{sv}");
}

/// Loading both an `.archi` interface stub *and* the real `.arch`
/// definition for the same module name in a single compilation unit
/// must not error with `duplicate definition`. This covers the
/// natural workflow `arch sim sub.arch top.arch --tb top_tb.cpp`,
/// where `top.arch` references `sub` via `inst` and the inst
/// resolver auto-loads `sub.archi` on top of the explicitly-passed
/// `sub.arch` — both register as `Item::Module`, only one as
/// interface stub.
#[test]
fn test_archi_stub_dedupes_with_real_arch() {
    use arch::ast::Item;

    // Concatenated source: interface stub followed by real definition.
    // Both define `module Sub` with identical port signatures; the
    // first will be tagged as an interface stub before resolve, just
    // like main.rs does for items loaded from `.archi` files.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Sub
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port b: out UInt<8>;
end module Sub

module Sub
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port b: out UInt<8>;

  comb
    b = a;
  end comb
end module Sub

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port x: in UInt<8>;
  port y: out UInt<8>;

  inst s: Sub
    clk <- clk;
    rst <- rst;
    a   <- x;
    b   -> y;
  end inst s
end module Top
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let mut parsed_ast = parser.parse_source_file().expect("parse error");

    // Tag the first `Sub` as an interface stub (mirrors main.rs's
    // filename-based tagging when items are parsed from a `.archi`).
    let mut tagged_one = false;
    for item in parsed_ast.items.iter_mut() {
        if let Item::Module(m) = item {
            if m.name.name == "Sub" && !tagged_one {
                item.set_is_interface(true);
                tagged_one = true;
            }
        }
    }
    assert!(tagged_one, "expected to tag the first Sub as interface");

    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let _symbols = resolve::resolve(&ast)
        .expect("resolve should succeed when an .archi stub coexists with the real .arch");
}

// ── arch-com#330: 33..=64-bit UInt fields must emit uint64_t (not uint32_t) ──
//
// Pre-fix, `cpp_internal_type` / `cpp_port_type` evaluated UInt widths via
// `eval_width`, which falls back to 32 for any non-literal expression (such
// as a bare param ident `ACC_WIDTH`). That silently truncated 33..=64-bit
// fields to `uint32_t` storage and applied a 32-bit `0xFFFFFFFFULL` mask in
// truncating arithmetic — corrupting upper bits of accumulators / wide
// data paths.
//
// The fix threads params through the type-emission helpers so a width
// expression that references a `param N: const = ...;` resolves to the
// param's literal default. These tests pin the four boundary cases plus
// the original issue's `UInt<ACC_WIDTH>` shape.

#[test]
fn test_uint_48_param_width_emits_uint64_storage() {
    // Repro from arch-com#330.
    let source = r#"
        module QkDotEngine
          param ACC_WIDTH: const = 48;

          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port score_out: out UInt<ACC_WIDTH>;
          port inc_in: in UInt<ACC_WIDTH>;

          reg accumulator: UInt<ACC_WIDTH> reset rst => 0;
          reg score_reg:   UInt<ACC_WIDTH> reset rst => 0;

          comb
            score_out = score_reg;
          end comb

          seq on clk rising
            accumulator <= (accumulator + inc_in).trunc<ACC_WIDTH>();
            score_reg <= accumulator;
          end seq
        end module QkDotEngine
    "#;
    let out = compile_to_sim_h(source, false);

    // Storage types: ports + internal regs all 48 bits → uint64_t.
    assert!(out.contains("uint64_t score_out"),
            "score_out port should be uint64_t for UInt<48>; got:\n{out}");
    assert!(out.contains("uint64_t inc_in"),
            "inc_in port should be uint64_t for UInt<48>; got:\n{out}");
    assert!(out.contains("uint64_t _accumulator"),
            "accumulator reg should be uint64_t for UInt<48>; got:\n{out}");
    assert!(out.contains("uint64_t _score_reg"),
            "score_reg reg should be uint64_t for UInt<48>; got:\n{out}");

    // _n_ shadow should match.
    assert!(out.contains("uint64_t _n_accumulator"),
            "_n_accumulator shadow should be uint64_t; got:\n{out}");

    // Truncating arithmetic should mask to 48 bits (12 F's), not 32.
    assert!(out.contains("0xFFFFFFFFFFFFULL"),
            "expected 48-bit mask 0xFFFFFFFFFFFFULL; got:\n{out}");
    assert!(!out.contains(" 0xFFFFFFFFULL"),
            "must not emit 32-bit mask 0xFFFFFFFFULL for 48-bit accumulator; got:\n{out}");

    // The seq-assign cast must be (uint64_t), not (uint32_t).
    assert!(out.contains("(uint64_t)((((_accumulator + inc_in))"),
            "trunc cast should be (uint64_t)(...); got:\n{out}");
}

#[test]
fn test_uint_width_boundary_buckets_with_param() {
    // Boundary check: 32 → uint32_t, 33 → uint64_t, 64 → uint64_t, 65 → wide.
    // Use param-derived widths to exercise the param-aware path.
    let source = r#"
        module W
          param W32:  const = 32;
          param W33:  const = 33;
          param W64:  const = 64;
          param W65:  const = 65;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a32: out UInt<W32>;
          port a33: out UInt<W33>;
          port a64: out UInt<W64>;
          port a65: out UInt<W65>;
          comb
            a32 = 0;
            a33 = 0;
            a64 = 0;
            a65 = (0).zext<W65>();
          end comb
        end module W
    "#;
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("uint32_t a32"),
            "UInt<32> port should be uint32_t; got:\n{out}");
    assert!(out.contains("uint64_t a33"),
            "UInt<33> port should be uint64_t; got:\n{out}");
    assert!(out.contains("uint64_t a64"),
            "UInt<64> port should be uint64_t; got:\n{out}");
    // 65 bits → wide (VlWide). Don't pin the exact word count here — just
    // assert it isn't the legacy uint32_t bucket.
    assert!(out.contains("VlWide") && out.contains("a65"),
            "UInt<65> port should be VlWide<...>; got:\n{out}");
    assert!(!out.contains("uint32_t a65"),
            "UInt<65> must not be uint32_t; got:\n{out}");
}

#[test]
fn test_uint_48_literal_width_emits_uint64_storage() {
    // Same as the boundary test, but with a literal `UInt<48>` (no param)
    // to confirm both code paths share the same fix.
    let source = r#"
        module Acc
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: out UInt<48>;
          port inc: in UInt<48>;
          reg r: UInt<48> reset rst => 0;
          comb
            a = r;
          end comb
          seq on clk rising
            r <= (r + inc).trunc<48>();
          end seq
        end module Acc
    "#;
    let out = compile_to_sim_h(source, false);
    assert!(out.contains("uint64_t a"),
            "UInt<48> port should be uint64_t; got:\n{out}");
    assert!(out.contains("uint64_t inc"),
            "UInt<48> port should be uint64_t; got:\n{out}");
    assert!(out.contains("uint64_t _r"),
            "UInt<48> reg should be uint64_t; got:\n{out}");
    assert!(out.contains("0xFFFFFFFFFFFFULL"),
            "expected 48-bit mask; got:\n{out}");
}

#[test]
fn test_sint_40_param_width_emits_int64_storage_and_signed_trunc() {
    let source = r#"
        module Bf16DotLike
          param ACC_WIDTH: const = 40;

          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port score_out: out SInt<ACC_WIDTH>;
          port inc_in: in SInt<ACC_WIDTH>;

          reg accumulator: SInt<ACC_WIDTH> reset rst => 0;
          reg score_reg:   SInt<ACC_WIDTH> reset rst => 0;

          comb
            score_out = score_reg;
          end comb

          seq on clk rising
            accumulator <= (accumulator + inc_in).trunc<ACC_WIDTH>();
            score_reg <= accumulator;
          end seq
        end module Bf16DotLike
    "#;
    let out = compile_to_sim_h(source, false);

    assert!(out.contains("int64_t score_out"),
            "SInt<40> output port should use int64_t storage; got:\n{out}");
    assert!(out.contains("int64_t inc_in"),
            "SInt<40> input port should use int64_t storage; got:\n{out}");
    assert!(out.contains("int64_t _accumulator"),
            "SInt<40> internal reg should use int64_t storage; got:\n{out}");
    assert!(out.contains("int64_t _score_reg"),
            "SInt<40> internal reg should use int64_t storage; got:\n{out}");
    assert!(out.contains("int64_t _n_accumulator"),
            "SInt<40> _n_ shadow should use int64_t storage; got:\n{out}");
    assert!(!out.contains("uint32_t _accumulator"),
            "SInt<40> accumulator must not fall into uint32_t storage; got:\n{out}");
    assert!(!out.contains("uint64_t _accumulator"),
            "SInt<40> accumulator must not use unsigned 64-bit storage; got:\n{out}");

    assert!(out.contains("0xFFFFFFFFFFULL"),
            "SInt<40> trunc should still mask to exactly 40 bits; got:\n{out}");
    assert!(out.contains("((int64_t)(((uint64_t)((_accumulator + inc_in)) & 0xFFFFFFFFFFULL) << 24) >> 24)"),
            "SInt<40> trunc should sign-extend from bit 39 into int64_t; got:\n{out}");
}

#[test]
fn test_thread_driven_sint_reg_keeps_parent_wire_type() {
    let source = r#"
        domain SysDomain
          freq_mhz: 250
        end domain SysDomain

        module SignedThread
          local param A: const = 21;
          local param B: const = 16;
          local param P: const = A + B;
          local param W: const = P + 8;

          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port start: in Bool;
          port weight: in UInt<A>;
          port val: in SInt<B>;
          port y: out SInt<W>;

          reg acc: SInt<W> reset rst => 0;
          let weight_signed: SInt<A> = signed(weight);
          let product_raw: SInt<P> = weight_signed * val;
          let product_ext: SInt<W> = product_raw.sext<W>();
          let next_acc: SInt<W> = (acc + product_ext).trunc<W>();

          thread T on clk rising, rst high
            wait until start;
            acc <= next_acc;
          end thread T

          comb
            y = next_acc;
          end comb
        end module SignedThread
    "#;

    let sv = compile_to_sv(source);
    assert!(sv.contains("logic signed [W-1:0] acc;"),
            "parent-side wire for thread-driven SInt reg should keep signedness/width:\n{sv}");
    assert!(sv.contains("assign next_acc = W'(acc + product_ext);"),
            "parent expression should see typed signed acc/product_ext and emit the trunc assignment:\n{sv}");
}

#[test]
fn test_sint_40_inst_output_wire_keeps_signed_storage() {
    let source = r#"
        module Bf16DotEngine
          param ACC_WIDTH: const = 40;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port score_out: out SInt<ACC_WIDTH>;
          port inc_in: in SInt<ACC_WIDTH>;
          reg accumulator: SInt<ACC_WIDTH> reset rst => 0;
          reg score_reg: SInt<ACC_WIDTH> reset rst => 0;
          comb
            score_out = score_reg;
          end comb
          seq on clk rising
            accumulator <= (accumulator + inc_in).trunc<ACC_WIDTH>();
            score_reg <= accumulator;
          end seq
        end module Bf16DotEngine

        module Wrapper
          param ACC_WIDTH: const = 40;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port inc: in SInt<ACC_WIDTH>;
          port score: out SInt<ACC_WIDTH>;
          wire score_wire: SInt<ACC_WIDTH>;
          inst dot: Bf16DotEngine
            param ACC_WIDTH = 40;
            clk <- clk;
            rst <- rst;
            inc_in <- inc;
            score_out -> score_wire;
          end inst dot
          comb
            score = score_wire;
          end comb
        end module Wrapper
    "#;
    let out = compile_to_sim_h(source, false);

    assert!(out.contains("int64_t _let_score_wire"),
            "SInt<40> child output wire should use int64_t storage in wrapper/native sim; got:\n{out}");
    assert!(!out.contains("uint32_t _let_score_wire"),
            "SInt<40> child output wire must not use uint32_t storage; got:\n{out}");
    assert!(!out.contains("uint64_t _let_score_wire"),
            "SInt<40> child output wire must not use unsigned storage; got:\n{out}");
    assert!(out.contains("score  = _let_score_wire"),
            "wrapper should forward the signed child output to its public port; got:\n{out}");
}

#[test]
fn test_sint_40_lowered_thread_regs_keep_signed_storage() {
    let source = r#"
        module ThreadBf16DotLike
          param ACC_WIDTH: const = 40;
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port start: in Bool;
          port inc_in: in SInt<ACC_WIDTH>;
          port score_out: out SInt<ACC_WIDTH>;

          reg accumulator: SInt<ACC_WIDTH> reset rst => 0;
          reg score_reg: SInt<ACC_WIDTH> reset rst => 0;

          comb
            score_out = score_reg;
          end comb

          thread on clk rising, rst high
            wait until start;
            accumulator <= (accumulator + inc_in).trunc<ACC_WIDTH>();
            score_reg <= accumulator;
          end thread
        end module ThreadBf16DotLike
    "#;
    let out = compile_to_sim_h(source, false);

    assert!(out.contains("int64_t accumulator"),
            "lowered thread submodule ports for SInt<40> regs should use int64_t; got:\n{out}");
    assert!(out.contains("int64_t _accumulator"),
            "parent/native sim SInt<40> reg storage should use int64_t after thread lowering; got:\n{out}");
    assert!(out.contains("int64_t _n_accumulator"),
            "lowered thread/native sim _n_ temporaries should use int64_t for SInt<40>; got:\n{out}");
    assert!(!out.contains("uint32_t _accumulator"),
            "lowered thread/native sim must not use uint32_t for SInt<40> accumulator; got:\n{out}");
    assert!(!out.contains("uint64_t _accumulator"),
            "lowered thread/native sim must not use unsigned storage for SInt<40> accumulator; got:\n{out}");
    assert!(out.contains("((int64_t)(((uint64_t)((_accumulator + inc_in)) & 0xFFFFFFFFFFULL) << 24) >> 24)")
            || out.contains("((int64_t)(((uint64_t)((accumulator + inc_in)) & 0xFFFFFFFFFFULL) << 24) >> 24)"),
            "lowered thread/native sim trunc should sign-extend SInt<40>; got:\n{out}");
}

// ── Thread state-name localparams (issue #247) ──────────────────────────────

#[test]
fn test_thread_state_localparams_emitted() {
    // Issue #247: thread lowering emits one `localparam [W-1:0] _t{ti}_S{N}_<role>`
    // per state and rewrites every state comparison / state-register assignment
    // to reference the name instead of a bare numeric literal.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port req: in Bool;
          port reg done: out Bool reset rst => false;
          thread on clk rising, rst high
            wait until req;
            done <= true;
            wait 2 cycle;
            done <= false;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);

    // (1) One `localparam` per state with the expected role suffix.
    //     S0 = wait_until (transition_cond = `req`), S1 = action (seq write),
    //     S2 = wait_cycles (wait 2 cycle), S3 = action (final seq write).
    assert!(sv.contains("localparam [1:0] _t0_S0_wait_until = 0"),
        "expected S0 wait_until localparam:\n{sv}");
    assert!(sv.contains("localparam [1:0] _t0_S1_action = 1"),
        "expected S1 action localparam:\n{sv}");
    assert!(sv.contains("localparam [1:0] _t0_S2_wait_cycles = 2"),
        "expected S2 wait_cycles localparam:\n{sv}");
    assert!(sv.contains("localparam [1:0] _t0_S3_action = 3"),
        "expected S3 action localparam:\n{sv}");

    // (2) Localparams declared in the merged threads module's parameter list,
    //     not inside the procedural block.
    assert!(sv.contains("module _M_threads #("),
        "merged threads module should have a parameter list:\n{sv}");

    // (3) State comparisons use the name, not a bare literal.
    assert!(sv.contains("_t0_state == _t0_S0_wait_until"),
        "expected name-form state comparison for S0:\n{sv}");
    assert!(sv.contains("_t0_state == _t0_S2_wait_cycles"),
        "expected name-form state comparison for S2:\n{sv}");

    // (4) State-register assignments use the name, not a bare literal.
    assert!(sv.contains("_t0_state <= _t0_S1_action"),
        "expected name-form state assignment to S1:\n{sv}");
    assert!(sv.contains("_t0_state <= _t0_S2_wait_cycles"),
        "expected name-form state assignment to S2:\n{sv}");

    // (5) No bare `_t0_state == N` or `_t0_state <= N` numeric-literal forms
    //     should remain. The synchronous-reset path emits `_t0_state <= 0`
    //     as the reset value (not a state-transition); that one stays as 0.
    for n in 0..4 {
        let bad_cmp = format!("_t0_state == {}", n);
        let bad_assign = format!("_t0_state <= {};", n);
        // Reset assigns to literal 0 (acceptable). All other uses must be name-form.
        if n != 0 {
            assert!(!sv.contains(&bad_cmp),
                "state comparison should use name-form, found bare `{}`:\n{}", bad_cmp, sv);
            assert!(!sv.contains(&bad_assign),
                "state assignment should use name-form, found bare `{}`:\n{}", bad_assign, sv);
        }
    }
}

#[test]
fn test_thread_state_names_distinguish_wait_until_vs_wait_cycles() {
    // Issue #247: the structural classification must produce distinct role
    // suffixes for a `wait until cond` state vs a `wait 1 cycle` state, so
    // the localparam names are diagnostic (not just unique).
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port go: in Bool;
          port done: out Bool;
          thread on clk rising, rst high
            wait until go;
            done = 1;
            wait 2 cycle;
            done = 0;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    // The wait-until state and the wait-cycles state must carry distinct
    // role suffixes so a waveform reader can tell at a glance what kind
    // of wait the FSM is sitting in.
    assert!(sv.contains("_t0_S0_wait_until"),
        "expected wait_until role suffix on S0:\n{sv}");
    assert!(sv.contains("_wait_cycles"),
        "expected wait_cycles role suffix on the wait-cycles state:\n{sv}");
    // Sanity: the two roles are NOT collapsed to the same name.
    assert!(sv.matches("_t0_S0_wait_until").count() >= 1
            && sv.matches("_wait_cycles =").count() >= 1,
        "wait_until and wait_cycles must produce distinct localparam decls:\n{sv}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #246: whole-design combinational feedback-loop detection (MVP).
// ─────────────────────────────────────────────────────────────────────────────

fn comb_loop_warnings(source: &str) -> Vec<String> {
    warnings_from(source)
        .into_iter()
        .filter(|m| m.contains("combinational feedback cycle")
                 || m.starts_with("arch check:"))
        .collect()
}

#[test]
fn test_comb_loop_within_module_detected() {
    // Self-driving comb cycle inside a single module: a depends on b,
    // b depends on a. The whole-design check should surface it as a
    // warning.
    let source = r#"
        module M
          port i: in UInt<1>;
          port o: out UInt<1>;
          wire a: UInt<1>;
          wire b: UInt<1>;
          comb
            a = b or i;
            b = a;
            o = a;
          end comb
        end module M
    "#;
    let ws = comb_loop_warnings(source);
    assert!(ws.iter().any(|m| m.contains("combinational feedback cycle")),
        "expected a comb-loop warning, got: {:?}", ws);
}

#[test]
fn test_comb_loop_across_two_instances_detected() {
    // Cross-instance loop: A.out -> B.in -> B.out -> A.in.
    // The current per-module analyzer silently absorbs this as
    // settle_depth=2; the new whole-design analyzer should warn.
    let source = r#"
        module Cell
          port i: in UInt<1>;
          port o: out UInt<1>;
          comb
            o = i;
          end comb
        end module Cell

        module Top
          port s: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;

          inst a: Cell
            i <- w2;
            o -> w1;
          end inst a

          inst b: Cell
            i <- w1;
            o -> w2;
          end inst b

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    assert!(ws.iter().any(|m| m.contains("combinational feedback cycle")),
        "expected a comb-loop warning, got: {:?}", ws);
}

#[test]
fn test_comb_loop_suppressed_by_pragma() {
    // Same setup as cross-instance test, but parent has the bless pragma.
    let source = r#"
        module Cell
          port i: in UInt<1>;
          port o: out UInt<1>;
          comb
            o = i;
          end comb
        end module Cell

        module Top
          pragma comb_loops_allowed;
          port s: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;

          inst a: Cell
            i <- w2;
            o -> w1;
          end inst a

          inst b: Cell
            i <- w1;
            o -> w2;
          end inst b

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    // No cycle warnings should remain; the summary line MAY still fire
    // mentioning suppression — but no "combinational feedback cycle (…)"
    // node-listing warning should be present.
    let cycle_msgs: Vec<_> = ws.iter().filter(|m| m.contains("combinational feedback cycle (")).collect();
    assert!(cycle_msgs.is_empty(),
        "expected pragma to suppress cycle warning, got: {:?}", ws);
    // Sanity: the summary should report 1 SCC found / 1 suppressed.
    let summary: Vec<_> = ws.iter().filter(|m| m.starts_with("arch check:")).collect();
    assert!(summary.iter().any(|m| m.contains("1 comb SCC(s) found") && m.contains("1 suppressed")),
        "expected suppression-summary line, got: {:?}", ws);
}

#[test]
fn test_comb_loop_through_register_not_flagged() {
    // Cycle goes a -> reg -> b -> a. The register breaks the comb path,
    // so no warning should fire.
    let source = r#"
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain

        module M
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port i:    in UInt<1>;
          port o:    out UInt<1>;
          wire a: UInt<1>;
          wire b: UInt<1>;
          reg r: UInt<1> reset rst => 0;
          comb
            a = r or i;
            b = a;
            o = b;
          end comb
          seq on clk rising
            r <= b;
          end seq
        end module M
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter().filter(|m| m.contains("combinational feedback cycle (")).collect();
    assert!(cycle_msgs.is_empty(),
        "register should break the cycle, but got: {:?}", ws);
}

#[test]
fn test_interface_module_treated_as_opaque() {
    // A module loaded purely as an `.archi` interface stub (no body) is
    // treated as opaque: every output assumed to depend on every input.
    // We simulate that here by setting `is_interface` post-parse on the
    // stub-like declaration.
    //
    // Setup: Stub has ports (i, o). The user's "stub" module body is
    // present in source but we will mark it as interface to mimic the
    // .archi path. With Stub opaque, the wire path through it closes a
    // cycle: a -> Stub.in -> Stub.out -> a.
    let source = r#"
        module Stub
          port i: in UInt<1>;
          port o: out UInt<1>;
        end module Stub

        module Top
          port s: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;

          inst a: Stub
            i <- w2;
            o -> w1;
          end inst a

          inst b: Stub
            i <- w1;
            o -> w2;
          end inst b

          comb
            q = w1;
          end comb
        end module Top
    "#;
    // Manually run the pipeline and mark Stub as interface.
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let mut parsed_ast = parser.parse_source_file().expect("parse error");
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name == "Stub" {
                m.is_interface = true;
            }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    // Re-mark after elaborate (variant rewrites may have renamed).
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") {
                m.is_interface = true;
            }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check error");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    assert!(ws.iter().any(|m| m.contains("combinational feedback cycle")),
        "expected opaque-interface module to participate in a detected cycle; warnings: {:?}", ws);
}

#[test]
fn test_opaque_interface_pipe_reg_output_does_not_close_cycle() {
    // Regression: in the opaque-stub path the MVP treated EVERY output of
    // an interface stub as combinationally driven, even outputs that
    // carry `reg_info: Some(_)` (i.e. `port reg out`/`port out pipe_reg<T,N>`).
    // Two stubs with `pipe_reg` outputs cross-wired through their inputs
    // do NOT form a comb cycle — the registers break the path at the seq
    // boundary. The filter on `registered_outs` excludes them from the
    // parent-level comb-edge synthesis.
    //
    // Without the filter, every arch-ibex module that exposes pipe_reg
    // outputs through an `.archi` stub (IbexCore, IbexIdStage, IbexTop)
    // gets flagged with massive over-approximation SCCs (126 nodes etc.).
    // With the filter, only modules with actually-comb outputs participate.
    let source = r#"
        module Stub
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port i:   in  UInt<1>;
          port reg o: out UInt<1> reset rst => 1'd0;
        end module Stub

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port s:   in  UInt<1>;
          port q:   out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;

          inst a: Stub
            clk <- clk;
            rst <- rst;
            i   <- w2;
            o   -> w1;
          end inst a

          inst b: Stub
            clk <- clk;
            rst <- rst;
            i   <- w1;
            o   -> w2;
          end inst b

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let mut parsed_ast = parser.parse_source_file().expect("parse error");
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name == "Stub" {
                m.is_interface = true;
            }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") {
                m.is_interface = true;
            }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check error");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs.is_empty(),
        "pipe_reg / port reg outputs on an opaque stub must not close a comb cycle; got: {:?}",
        cycle_msgs);
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #246 Phase 2: per-output `comb_dep_on(...)` annotation.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_archi_comb_dep_annotation_parses() {
    // A `.archi`-shaped module declaration carrying `comb_dep_on(...)`
    // on an output port. The parser should populate the new
    // `PortDecl::comb_deps` field with the listed input idents.
    let source = "
        module Stub
          port a: in  UInt<8>;
          port b: in  UInt<8>;
          port x: out UInt<8> comb_dep_on(a);
          port y: out UInt<8> comb_dep_on(a, b);
          port z: out UInt<8> comb_dep_on();
          port w: out UInt<8>;
        end module Stub
    ";
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let m = parsed.items.iter().find_map(|i| match i {
        arch::ast::Item::Module(m) => Some(m),
        _ => None,
    }).expect("module");

    let by_name: std::collections::HashMap<&str, &arch::ast::PortDecl> =
        m.ports.iter().map(|p| (p.name.name.as_str(), p)).collect();

    let x_deps: Vec<&str> = by_name["x"].comb_deps.as_ref()
        .expect("x must carry comb_deps")
        .iter().map(|i| i.name.as_str()).collect();
    assert_eq!(x_deps, vec!["a"], "x deps");

    let y_deps: Vec<&str> = by_name["y"].comb_deps.as_ref()
        .expect("y must carry comb_deps")
        .iter().map(|i| i.name.as_str()).collect();
    assert_eq!(y_deps, vec!["a", "b"], "y deps");

    let z_deps: &Vec<arch::ast::Ident> = by_name["z"].comb_deps.as_ref()
        .expect("z must carry comb_deps (empty list = pure)");
    assert!(z_deps.is_empty(), "z must be pure (empty deps)");

    assert!(by_name["w"].comb_deps.is_none(),
        "w must carry no annotation (opaque fallback)");
}

#[test]
fn test_archi_comb_dep_annotation_round_trip() {
    // Compile a module body that drives `x` only from input `a` and
    // `y` from both `a` and `b`. The `.archi` emit should reflect that
    // precise per-output dependency shape via `comb_dep_on(...)`.
    let source = "
        module M
          port a: in  UInt<8>;
          port b: in  UInt<8>;
          port x: out UInt<8>;
          port y: out UInt<8>;
          port z: out UInt<8>;
          comb
            x = a;
            y = a + b;
            z = 8'd0;
          end comb
        end module M
    ";
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let item = parsed.items.iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("module");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(body.contains("port x: out UInt<8> comb_dep_on(a);"),
        "x must depend only on a: {body}");
    assert!(body.contains("port y: out UInt<8> comb_dep_on(a, b);"),
        "y must depend on a and b: {body}");
    assert!(body.contains("port z: out UInt<8> comb_dep_on();"),
        "z is pure (constant): {body}");
}

#[test]
fn test_archi_comb_dep_precise_eliminates_false_positive() {
    // Two stubs cross-wired through TWO separate input/output port pairs.
    // The `comb_dep_on(...)` annotation says `out_x` depends only on `in_a`
    // (NOT in_b). The cross-wired cycle would only close THROUGH `in_b`,
    // so no SCC should fire. Without the annotation (opaque fallback)
    // the analyzer would treat every out as depending on every in and
    // would flag this as a comb cycle.
    let source = r#"
        module Stub
          port in_a: in  UInt<1>;
          port in_b: in  UInt<1>;
          port out_x: out UInt<1> comb_dep_on(in_a);
          port out_y: out UInt<1> comb_dep_on(in_a);
        end module Stub

        module Top
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;
          wire w3: UInt<1>;
          wire w4: UInt<1>;

          inst u1: Stub
            in_a  <- a;
            in_b  <- w2;
            out_x -> w1;
            out_y -> w3;
          end inst u1

          inst u2: Stub
            in_a  <- a;
            in_b  <- w1;
            out_x -> w2;
            out_y -> w4;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let mut parsed_ast = parser.parse_source_file().expect("parse");
    // Mark `Stub` as an interface to mimic the .archi path.
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name == "Stub" { m.is_interface = true; }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") { m.is_interface = true; }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs.is_empty(),
        "comb_dep_on(in_a) should restrict edges so no cycle fires; got: {:?}", cycle_msgs);
}

#[test]
fn test_archi_comb_dep_empty_marks_output_pure() {
    // A stub whose output port is marked `comb_dep_on()` (empty) should
    // contribute NO incoming comb edges. Cross-wiring two such stubs
    // through their inputs cannot close a cycle.
    let source = r#"
        module Stub
          port i: in  UInt<1>;
          port o: out UInt<1> comb_dep_on();
        end module Stub

        module Top
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;

          inst u1: Stub
            i <- w2;
            o -> w1;
          end inst u1
          inst u2: Stub
            i <- w1;
            o -> w2;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let mut parsed_ast = parser.parse_source_file().expect("parse");
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name == "Stub" { m.is_interface = true; }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") { m.is_interface = true; }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs.is_empty(),
        "comb_dep_on() (pure) must produce no incoming comb edges; got: {:?}", cycle_msgs);
}

#[test]
fn test_archi_comb_dep_absent_falls_back_to_opaque() {
    // A stub WITHOUT the annotation must keep today's opaque "every
    // output depends on every input" behavior. Two such stubs cross-
    // wired should fire the cycle warning (regression guard for the
    // pre-annotation behavior).
    let source = r#"
        module Stub
          port i: in  UInt<1>;
          port o: out UInt<1>;
        end module Stub

        module Top
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;

          inst u1: Stub
            i <- w2;
            o -> w1;
          end inst u1
          inst u2: Stub
            i <- w1;
            o -> w2;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lexer");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let mut parsed_ast = parser.parse_source_file().expect("parse");
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name == "Stub" { m.is_interface = true; }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") { m.is_interface = true; }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    assert!(ws.iter().any(|m| m.contains("combinational feedback cycle")),
        "absent annotation must keep opaque fallback that fires cycle: {:?}", ws);
}

#[test]
fn test_archi_comb_dep_on_registered_output_rejected_at_parse() {
    // `port reg out_x: ... comb_dep_on(in_a);` is illegal — registered
    // outputs are not combinationally driven. The parser must reject.
    let source = "
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain
        module M
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port in_a: in UInt<1>;
          port reg out_x: out UInt<1> reset rst => 1'd0 comb_dep_on(in_a);
        end module M
    ";
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let err = parser.parse_source_file()
        .expect_err("must reject comb_dep_on on registered output");
    let msg = format!("{:?}", err);
    assert!(msg.contains("comb_dep_on") && msg.contains("registered"),
        "error should mention comb_dep_on + registered; got: {}", msg);
}

#[test]
fn test_archi_comb_dep_on_input_port_rejected_at_parse() {
    // `comb_dep_on(...)` is only legal on output ports.
    let source = "
        module M
          port in_a: in UInt<1> comb_dep_on(in_a);
        end module M
    ";
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let err = parser.parse_source_file()
        .expect_err("must reject comb_dep_on on input port");
    let msg = format!("{:?}", err);
    assert!(msg.contains("comb_dep_on"),
        "error should mention comb_dep_on; got: {}", msg);
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #246 Phase 3: per-output precision for BODIED (non-opaque) children.
//
// Phase 2 (#338) gave per-output precision across `.archi` boundaries via the
// `comb_dep_on(...)` annotation. Phase 3 extends the same precision to bodied
// children walked by the whole-design analyzer, replacing the aggregate
// `cartesian_product(comb_outputs, comb_dep_inputs)` over-approximation with
// the precise map produced by `per_output_comb_deps`.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_comb_loop_bodied_per_output_precision_eliminates_false_positive() {
    // `Cell` has two independent comb paths: out_a only depends on in_x,
    // out_b only depends on in_y. The aggregate analyzer treats every
    // comb output as depending on every comb input and would flag a
    // spurious cycle through `w1 → w2 → w1`. The per-output analyzer
    // keeps the two paths disjoint and should NOT flag a cycle.
    let source = r#"
        module Cell
          port in_x: in  UInt<1>;
          port in_y: in  UInt<1>;
          port out_a: out UInt<1>;
          port out_b: out UInt<1>;
          comb
            out_a = in_x;
            out_b = in_y;
          end comb
        end module Cell

        module Top
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;
          wire dead1: UInt<1>;
          wire dead2: UInt<1>;

          inst u1: Cell
            in_x  <- a;
            in_y  <- w2;
            out_a -> w1;
            out_b -> dead1;
          end inst u1

          inst u2: Cell
            in_x  <- w1;
            in_y  <- a;
            out_a -> dead2;
            out_b -> w2;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs.is_empty(),
        "per-output precision must eliminate the aggregate-only phantom \
         cycle; got: {:?}", cycle_msgs);
}

#[test]
fn test_comb_loop_bodied_real_cycle_still_detected() {
    // Same shape as the false-positive test, but now `out_b` legitimately
    // depends on BOTH in_x and in_y. Per-output precision must preserve
    // the real cycle: u1.out_a (← in_x = a is fine) and u2.out_b (← in_x
    // = w1, in_y = a) close w1 → w2 → w1 via u2.out_b's true dep on in_x.
    let source = r#"
        module Cell
          port in_x: in  UInt<1>;
          port in_y: in  UInt<1>;
          port out_a: out UInt<1>;
          port out_b: out UInt<1>;
          comb
            out_a = in_x;
            out_b = in_x or in_y;
          end comb
        end module Cell

        module Top
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;
          wire dead1: UInt<1>;
          wire dead2: UInt<1>;

          inst u1: Cell
            in_x  <- w2;
            in_y  <- a;
            out_a -> w1;
            out_b -> dead1;
          end inst u1

          inst u2: Cell
            in_x  <- w1;
            in_y  <- a;
            out_a -> dead2;
            out_b -> w2;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(!cycle_msgs.is_empty(),
        "real cycle (u1.out_a ← w2; u2.out_b ← w1) must still fire; \
         warnings: {:?}", ws);
}

#[test]
fn test_comb_loop_bodied_output_missing_from_per_output_map_falls_back() {
    // Option C fallback contract: when an output port is present in the
    // aggregate `info.comb_outputs` (it appears as LHS in a comb block)
    // but the per-output walker can't trace it back to ANY input — e.g.
    // it's driven by a constant or by a non-input intermediate — the
    // per-output map records an empty dep set. We treat empty-set as
    // "pure for this output port at the body level".
    //
    // The test asserts:
    //   (a) A truly pure output (driven by constant) does NOT introduce
    //       spurious edges that close a cycle through it.
    //   (b) Compare to opaque-fallback (no `.archi` annotation on an
    //       interface stub of the same shape) where every output IS
    //       assumed to depend on every input.
    //
    // The fallback "treat as opaque every input" path written into the
    // expander would only fire if the walker omits an output entry; with
    // `per_output_comb_deps`'s current always-emit-an-entry contract,
    // this branch is a safety net rather than a live code path. We pin
    // the empty-deps-as-pure semantics here so a future walker change
    // that DOES start omitting entries falls back to opaque rather than
    // silently dropping edges.
    let source = r#"
        module Cell
          port i: in  UInt<1>;
          port o: out UInt<1>;
          comb
            o = 1'd0;
          end comb
        end module Cell

        module Top
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;

          inst u1: Cell
            i <- w2;
            o -> w1;
          end inst u1
          inst u2: Cell
            i <- w1;
            o -> w2;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs.is_empty(),
        "pure output (per-output map empty) must not close a comb cycle; \
         got: {:?}", cycle_msgs);
}

#[test]
fn test_comb_loop_bodied_per_output_cache_handles_repeated_insts() {
    // Cache invariance: the same bodied child instantiated 3× must not
    // re-walk the body per inst. Functionally we verify the SAME false-
    // positive elimination as the first test, but with 3 cross-wired
    // instances — exercises the memoization path under repeated use.
    let source = r#"
        module Cell
          port in_x: in  UInt<1>;
          port in_y: in  UInt<1>;
          port out_a: out UInt<1>;
          port out_b: out UInt<1>;
          comb
            out_a = in_x;
            out_b = in_y;
          end comb
        end module Cell

        module Top
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;
          wire w3: UInt<1>;
          wire d1: UInt<1>;
          wire d2: UInt<1>;
          wire d3: UInt<1>;

          inst u1: Cell
            in_x  <- a;
            in_y  <- w3;
            out_a -> w1;
            out_b -> d1;
          end inst u1
          inst u2: Cell
            in_x  <- w1;
            in_y  <- a;
            out_a -> w2;
            out_b -> d2;
          end inst u2
          inst u3: Cell
            in_x  <- w2;
            in_y  <- a;
            out_a -> w3;
            out_b -> d3;
          end inst u3

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs.is_empty(),
        "per-output precision must hold across 3 cross-wired insts; got: {:?}",
        cycle_msgs);
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #246 Phase 4: per-output comb-dep precision for `fsm` bodies.
// Phase 2 (PR #338) added the precision for `module` body emit + .archi parse
// of `comb_dep_on(...)`. Phase 3 (PR #339) wired the bodied-module branch of
// the whole-design analyzer to consume the same per-output map. Phase 4
// extends both pieces to FSMs (bodied + .archi emit) so an FSM child no
// longer collapses to the aggregate-every-out-feeds-every-in shape that
// was closing arch-ibex's residual `ibex_id_stage` SCC through the
// `ibex_controller` fsm.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_comb_loop_fsm_per_output_precision_eliminates_false_positive() {
    // FSM with two independent comb paths: out_a depends only on in_x;
    // out_b depends only on in_y. Aggregate-only analysis would close a
    // phantom cycle through `w1 → w2 → w1` because every out is treated
    // as depending on every in. Per-output FSM precision must eliminate
    // it.
    let source = r#"
        fsm Cell
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in_x: in  UInt<1>;
          port in_y: in  UInt<1>;
          port out_a: out UInt<1> default 1'd0;
          port out_b: out UInt<1> default 1'd0;
          state [S0]
          default state S0;
          state S0
            comb
              out_a = in_x;
              out_b = in_y;
            end comb
            -> S0 when true;
          end state S0
        end fsm Cell

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;
          wire dead1: UInt<1>;
          wire dead2: UInt<1>;

          inst u1: Cell
            clk   <- clk;
            rst   <- rst;
            in_x  <- a;
            in_y  <- w2;
            out_a -> w1;
            out_b -> dead1;
          end inst u1

          inst u2: Cell
            clk   <- clk;
            rst   <- rst;
            in_x  <- w1;
            in_y  <- a;
            out_a -> dead2;
            out_b -> w2;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs.is_empty(),
        "fsm per-output precision must eliminate the aggregate-only \
         phantom cycle; got: {:?}", cycle_msgs);
}

#[test]
fn test_comb_loop_fsm_real_cycle_still_detected() {
    // Same shape, but now out_b genuinely reads in_x too — the real
    // cycle u1.out_b (← in_y = w2) and u2.out_b (← in_x = w1, in_y = a)
    // closes w1 → w2 → w1 via u2.out_b's true dep on in_x.
    let source = r#"
        fsm Cell
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in_x: in  UInt<1>;
          port in_y: in  UInt<1>;
          port out_a: out UInt<1> default 1'd0;
          port out_b: out UInt<1> default 1'd0;
          state [S0]
          default state S0;
          state S0
            comb
              out_a = in_x;
              out_b = in_x or in_y;
            end comb
            -> S0 when true;
          end state S0
        end fsm Cell

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;
          wire w2: UInt<1>;
          wire dead1: UInt<1>;
          wire dead2: UInt<1>;

          inst u1: Cell
            clk   <- clk;
            rst   <- rst;
            in_x  <- w2;
            in_y  <- a;
            out_a -> w1;
            out_b -> dead1;
          end inst u1

          inst u2: Cell
            clk   <- clk;
            rst   <- rst;
            in_x  <- w1;
            in_y  <- a;
            out_a -> dead2;
            out_b -> w2;
          end inst u2

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(!cycle_msgs.is_empty(),
        "real cycle (u1.out_a ← w2; u2.out_b ← w1) through fsm \
         must still fire; warnings: {:?}", ws);
}

#[test]
fn test_archi_fsm_emit_includes_comb_dep_on() {
    // Compile a small FSM body and assert `.archi` carries per-output
    // `comb_dep_on(...)` annotations (mirror of the module emit path).
    let source = r#"
        fsm M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in  UInt<8>;
          port b: in  UInt<8>;
          port x: out UInt<8> default 8'd0;
          port y: out UInt<8> default 8'd0;
          port z: out UInt<8> default 8'd0;
          state [S0]
          default state S0;
          state S0
            comb
              x = a;
              y = a + b;
              z = 8'd0;
            end comb
            -> S0 when true;
          end state S0
        end fsm M
    "#;
    let tokens = lexer::tokenize(source).expect("lexer");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let item = parsed.items.iter()
        .find(|i| matches!(i, arch::ast::Item::Fsm(_)))
        .expect("fsm");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(body.contains("port x: out UInt<8> comb_dep_on(a);"),
        "x must depend only on a: {body}");
    assert!(body.contains("port y: out UInt<8> comb_dep_on(a, b);"),
        "y must depend on a and b: {body}");
    assert!(body.contains("port z: out UInt<8> comb_dep_on();"),
        "z is pure (constant): {body}");
}

#[test]
fn test_comb_loop_fsm_default_comb_deps_included() {
    // `default_comb` is applied before the state case (the FSM codegen
    // emits it as part of the comb block). Any input reads in
    // `default_comb` must contribute to per-output deps.
    //
    // Here out_a's only assignment is in `default_comb`, sourced from
    // in_z. A cross-wire from out_a → another inst's in_y_other_inst
    // does NOT close a cycle (out_a doesn't read in_y). But a cross-
    // wire through in_z DOES — exercise the legitimate path.
    let source = r#"
        fsm Cell
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in_y: in  UInt<1>;
          port in_z: in  UInt<1>;
          port out_a: out UInt<1> default 1'd0;
          default
            comb
              out_a = in_z;
            end comb
          end default
          state [S0]
          default state S0;
          state S0
            -> S0 when true;
          end state S0
        end fsm Cell

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;

          inst u1: Cell
            clk   <- clk;
            rst   <- rst;
            in_y  <- a;
            in_z  <- w1;
            out_a -> w1;
          end inst u1

          comb
            q = w1;
          end comb
        end module Top
    "#;
    // The cross-wire w1 → in_z → out_a → w1 is a legitimate cycle.
    // Per-output FSM walker must include in_z in out_a's dep set.
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(!cycle_msgs.is_empty(),
        "default_comb reads in_z driving out_a; w1 → in_z → out_a → w1 \
         must fire as a comb cycle; warnings: {:?}", ws);
}

#[test]
fn test_comb_loop_fsm_output_default_expr_deps_included() {
    // FSM output port default expression (`default in_x + in_y`) is
    // emitted by the FSM codegen as the comb-block default before the
    // state case. Identifier reads in the default expression are real
    // comb deps for that output and must appear in the per-output map.
    //
    // Here out_a's value is the default expression `in_x + in_y` (and
    // S0 has no per-state assignment to out_a). Wire w1 → in_y → out_a
    // → w1 must close as a real cycle.
    let source = r#"
        fsm Cell
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in_x: in  UInt<1>;
          port in_y: in  UInt<1>;
          port out_a: out UInt<1> default in_x or in_y;
          state [S0]
          default state S0;
          state S0
            -> S0 when true;
          end state S0
        end fsm Cell

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;

          inst u1: Cell
            clk   <- clk;
            rst   <- rst;
            in_x  <- a;
            in_y  <- w1;
            out_a -> w1;
          end inst u1

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws = comb_loop_warnings(source);
    let cycle_msgs: Vec<_> = ws.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(!cycle_msgs.is_empty(),
        "out_a's default expression reads in_y; w1 → in_y → out_a → w1 \
         must fire as a comb cycle; warnings: {:?}", ws);

    // And the dual: with cross-wire through in_a (NOT a dep), no cycle.
    let source2 = r#"
        fsm Cell
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port in_x: in  UInt<1>;
          port in_y: in  UInt<1>;
          port out_a: out UInt<1> default in_x;
          state [S0]
          default state S0;
          state S0
            -> S0 when true;
          end state S0
        end fsm Cell

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port a: in UInt<1>;
          port q: out UInt<1>;
          wire w1: UInt<1>;

          inst u1: Cell
            clk   <- clk;
            rst   <- rst;
            in_x  <- a;
            in_y  <- w1;
            out_a -> w1;
          end inst u1

          comb
            q = w1;
          end comb
        end module Top
    "#;
    let ws2 = comb_loop_warnings(source2);
    let cycle_msgs2: Vec<_> = ws2.iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(cycle_msgs2.is_empty(),
        "out_a's default reads only in_x; w1 → in_y is NOT a dep, \
         so no cycle should fire; got: {:?}", cycle_msgs2);
}

// ─── multicycle reg annotation (Phase A) ─────────────────────────────────────
//
// Parse + AST + SDC emission only. Phase B will add input-feeding-tree
// analysis for the `--check-uninit` valid-tracking codegen pass.
//
// Phase A invariants:
//   - SV emission is byte-identical to a control case without the annotation
//     (a multicycle reg is still a single flop).
//   - An adjacent `.sdc` file (returned by `Codegen::emit_sdc`) carries one
//     matched setup/hold pair per multicycle reg.
//   - `multicycle 0` is a parse/typecheck error (N must be >= 1).
//   - Modules with no multicycle regs produce `None` SDC (no file written).

/// Compile `.arch` source → (SV string, optional SDC string). Same pipeline
/// as `compile_to_sv`, exposed because `emit_sdc` is the only way to observe
/// the multicycle reg's effect from a test.
fn compile_to_sv_with_sdc(source: &str) -> (String, Option<String>) {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator lowering error");
    let ast = elaborate::lower_threads_with_opts(ast, &elaborate::ThreadLowerOpts::default())
        .expect("lower_threads error");
    let ast = elaborate::lower_pipe_reg_ports(ast).expect("lower_pipe_reg_ports error");
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit_channel dispatch error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let (_warnings, overload_map) = checker.check().expect("type check error");
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
    let sv = codegen.generate();
    let sdc = codegen.emit_sdc("test_source.arch");
    (sv, sdc)
}

#[test]
fn test_multicycle_reg_parses_and_emits_sv_unchanged() {
    // Control: same module, no annotation. Used to byte-compare the SV
    // body — adding `multicycle 3` must not change any emitted flop logic.
    let control = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port en: in Bool;
  port y: out UInt<32>;
  reg x: UInt<32> reset rst => 0;
  seq on clk rising
    if en
      x <= (x + 1).trunc<32>();
    end if
  end seq
  comb
    y = x;
  end comb
end module M
"#;
    let mc = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port en: in Bool;
  port y: out UInt<32>;
  reg x: UInt<32> multicycle 3 reset rst => 0;
  seq on clk rising
    if en
      x <= (x + 1).trunc<32>();
    end if
  end seq
  comb
    y = x;
  end comb
end module M
"#;
    let (sv_ctrl, sdc_ctrl) = compile_to_sv_with_sdc(control);
    let (sv_mc, sdc_mc) = compile_to_sv_with_sdc(mc);
    assert_eq!(sv_ctrl, sv_mc,
        "multicycle annotation must not alter SV emission");
    assert!(sdc_ctrl.is_none(), "control case: no .sdc expected");
    assert!(sdc_mc.is_some(),  "multicycle case: .sdc expected");
}

#[test]
fn test_multicycle_reg_emits_sdc_file() {
    let source = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port en: in Bool;
  port y: out UInt<32>;
  reg result: UInt<32> multicycle 3 reset rst => 0;
  seq on clk rising
    if en
      result <= (result + 1).trunc<32>();
    end if
  end seq
  comb
    y = result;
  end comb
end module M
"#;
    let (_sv, sdc) = compile_to_sv_with_sdc(source);
    let sdc = sdc.expect(".sdc expected when multicycle reg is present");
    assert!(sdc.contains("set_multicycle_path 3 -setup -to [get_cells -hierarchical {*result_reg*}]"),
        "expected setup constraint with N=3; got:\n{}", sdc);
    assert!(sdc.contains("set_multicycle_path 2 -hold -to [get_cells -hierarchical {*result_reg*}]"),
        "expected hold constraint with N-1=2; got:\n{}", sdc);
    assert!(sdc.contains("Module M: multicycle reg result"),
        "expected per-reg header comment; got:\n{}", sdc);
    // The leading `*` in the glob is load-bearing: it lets the constraint
    // attach under both flat synth (no instance prefix) and hierarchical
    // synth (any number of `top/.../<Module>/` levels). A regression that
    // re-introduces the `<Module>/` prefix would silently fail to attach
    // under flat / standalone synth (OpenSTA warns `instance not found`).
    assert!(sdc.contains("[get_cells -hierarchical {*result_reg*}]"),
        "expected wildcard-prefix glob `*result_reg*` with -hierarchical; got:\n{}", sdc);
    assert!(!sdc.contains("{M/result_reg"),
        "expected NO hierarchical `M/result_reg` prefix in glob; got:\n{}", sdc);
    // `-hierarchical` is mandatory: under hierarchical synth (parent +
    // child module), OpenSTA's `get_cells` is non-recursive by default, so
    // the `*` glob does not descend into instance subhierarchies. Without
    // the flag the multicycle constraint silently attaches to zero cells
    // and the path is treated as single-cycle (verified with the
    // MultdivMulticycleHier two-pass example).
    assert!(sdc.contains("get_cells -hierarchical"),
        "expected `-hierarchical` flag on get_cells; got:\n{}", sdc);
}

#[test]
fn test_multicycle_reg_zero_rejected_at_parse_or_typecheck() {
    let source = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port y: out UInt<32>;
  reg result: UInt<32> multicycle 0 reset rst => 0;
  comb
    y = result;
  end comb
end module M
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let err = parser.parse_source_file().err();
    // Parser rejects at the literal — verify the error text mentions the
    // N >= 1 requirement so the diagnostic is actionable.
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("multicycle") && (msg.contains(">= 1") || msg.contains("N=0") || msg.contains("meaningless")),
        "expected an N >= 1 diagnostic mentioning `multicycle`; got: {}",
        msg
    );
}

#[test]
fn test_multicycle_reg_with_no_consumers_still_emits_sdc() {
    // Unused multicycle reg — common during incremental development. The
    // SDC constraint is structural (it pins the path through the flop) and
    // must be emitted even when nothing reads the reg.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port y: out UInt<32>;
  reg _unused: UInt<32> multicycle 4 reset rst => 0;
  comb
    y = 0;
  end comb
end module M
"#;
    let (_sv, sdc) = compile_to_sv_with_sdc(source);
    let sdc = sdc.expect("unused multicycle reg still emits SDC");
    assert!(sdc.contains("set_multicycle_path 4 -setup -to [get_cells -hierarchical {*_unused_reg*}]"),
        "got:\n{}", sdc);
    assert!(sdc.contains("set_multicycle_path 3 -hold -to [get_cells -hierarchical {*_unused_reg*}]"),
        "got:\n{}", sdc);
}

#[test]
fn test_module_without_multicycle_reg_does_not_write_sdc() {
    let source = r#"
domain D
  freq_mhz: 100
end domain D
module M
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port y: out UInt<32>;
  reg x: UInt<32> reset rst => 0;
  seq on clk rising
    x <= (x + 1).trunc<32>();
  end seq
  comb
    y = x;
  end comb
end module M
"#;
    let (sv, sdc) = compile_to_sv_with_sdc(source);
    assert!(sv.contains("module M"));
    assert!(sdc.is_none(),
        "no multicycle annotation → `emit_sdc` must return None so the driver \
         skips writing a `.sdc` companion file");
}

#[test]
fn test_multicycle_reg_in_fsm_body() {
    // `fsm` bodies declare regs in `regs: Vec<RegDecl>`; the multicycle
    // annotation must propagate there too. Test name picks `result` to
    // make the SDC target path human-checkable.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
fsm F
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port en: in Bool;
  port y: out UInt<32> default 0;
  reg result: UInt<32> reset rst => 0;
  reg slow_r: UInt<32> multicycle 5 reset rst => 0;
  state [Idle, Run]
  default state Idle;
  state Idle
    comb
      y = 0;
    end comb
    seq on clk rising
      if en
        slow_r <= (result + 1).trunc<32>();
      end if
    end seq
    -> Run when en;
  end state Idle
  state Run
    comb
      y = slow_r;
    end comb
    -> Idle when true;
  end state Run
end fsm F
"#;
    let (_sv, sdc) = compile_to_sv_with_sdc(source);
    let sdc = sdc.expect(".sdc expected for multicycle reg inside fsm");
    assert!(sdc.contains("set_multicycle_path 5 -setup -to [get_cells -hierarchical {*slow_r_reg*}]"),
        "got:\n{}", sdc);
    assert!(sdc.contains("set_multicycle_path 4 -hold -to [get_cells -hierarchical {*slow_r_reg*}]"),
        "got:\n{}", sdc);
}
