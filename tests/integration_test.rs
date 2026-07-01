use arch::codegen::Codegen;
use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;

mod common;

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
            while i < lines.len() && lines[i].trim_start() != "// synopsys translate_on" {
                i += 1;
            }
            // Consume the translate_on line too.
            if i < lines.len() {
                i += 1;
            }
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

fn warnings_after_full_lower(source: &str) -> Vec<String> {
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
    let (warnings, _) = checker.check().expect("type check error");
    warnings.into_iter().map(|w| w.message).collect()
}

fn collect_thread_map(source: &str) -> arch::thread_map::ThreadMap {
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target lowering error");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator lowering error");
    let map = std::rc::Rc::new(std::cell::RefCell::new(
        arch::thread_map::ThreadMap::default(),
    ));
    let opts = elaborate::ThreadLowerOpts {
        thread_map: Some(map.clone()),
        ..Default::default()
    };
    let _ast = elaborate::lower_threads_with_opts(ast, &opts).expect("lower_threads error");
    let collected = map.borrow().clone();
    collected
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

// ── Operators ─────────────────────────────────────────────────────────────────

#[test]
fn test_bang_prefix_is_logical_not_alias() {
    // arch#496: `!` is a symbolic alias for the `not` keyword (logical-not),
    // exactly parallel to `&&`==`and` / `||`==`or` (#493). It must lower to
    // SV `!`, and `!=` must remain a distinct, unaffected operator.
    let bang_src = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BangAlias
  port a: in Bool;
  port b: in Bool;
  port y: out Bool;
  port impl_out: out Bool;
  port nested: out Bool;
  port ne: out Bool;
  comb
    y = !a;
    impl_out = (!a) || b;
    nested = !(a and b);
    ne = a != b;
  end comb
end module BangAlias
"#;
    let bang = compile_to_sv(bang_src);
    assert!(bang.contains("assign y = !a;"), "got:\n{bang}");
    assert!(bang.contains("assign impl_out = !a || b;"), "got:\n{bang}");
    assert!(bang.contains("assign nested = !(a && b);"), "got:\n{bang}");
    // `!=` is a distinct token (BangEq) — prefix `!` must not disturb it.
    assert!(bang.contains("assign ne = a != b;"), "got:\n{bang}");

    // The keyword spelling of the same module must lower byte-identically:
    // `!`/`||`/`and` and `not`/`or`/`and` are exact aliases.
    let kw_src = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module BangAlias
  port a: in Bool;
  port b: in Bool;
  port y: out Bool;
  port impl_out: out Bool;
  port nested: out Bool;
  port ne: out Bool;
  comb
    y = not a;
    impl_out = (not a) or b;
    nested = not (a and b);
    ne = a != b;
  end comb
end module BangAlias
"#;
    let kw = compile_to_sv(kw_src);
    assert_eq!(
        bang, kw,
        "`!`/`||` must lower identically to `not`/`or`:\n--- bang ---\n{bang}\n--- kw ---\n{kw}"
    );
}

// ── Let bindings ──────────────────────────────────────────────────────────────

#[test]
fn test_let_bindings() {
    let source = include_str!("../examples/let_bindings.arch");
    let sv = compile_to_sv(source);
    // Typed let: emits declared type then a separate assign
    assert!(
        sv.contains("logic [7:0] mask;"),
        "expected typed let decl, got:\n{sv}"
    );
    assert!(
        sv.contains("assign mask = a & b;"),
        "expected typed let assign, got:\n{sv}"
    );
    // Untyped let: emits logic declaration + assign (same pattern as typed let)
    assert!(
        sv.contains("logic same;"),
        "expected untyped let decl, got:\n{sv}"
    );
    assert!(
        sv.contains("assign same = a == b;"),
        "expected untyped let assign, got:\n{sv}"
    );
    // Outputs driven from the let-bound wires
    assert!(
        sv.contains("assign masked = mask;"),
        "expected masked assign, got:\n{sv}"
    );
    assert!(
        sv.contains("assign equal = same;"),
        "expected equal assign, got:\n{sv}"
    );
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
fn test_fsm_legal_state_assert_skipped_for_power_of_two_state_count() {
    // The auto `_auto_legal_state: ... state_r < N` assertion is vacuous when N
    // is a power of two (every encoding is a legal state) AND it width-mismatches
    // (the N literal needs one more bit than `state_r`) → Verilator WIDTHEXPAND.
    // It must be SKIPPED for power-of-two state counts and KEPT (and width-clean)
    // for non-power-of-two counts where unused encodings exist.
    let two_state = r#"
fsm Toggle
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port flip: in Bool;
  port q: out Bool;
  state [A, B]
  default state A;
  default seq on clk rising;
  state A
    comb
      q = false;
    end comb
    -> B when flip;
  end state A
  state B
    comb
      q = true;
    end comb
    -> A when flip;
  end state B
end fsm Toggle
"#;
    let sv2 = compile_to_sv(two_state);
    assert!(
        !sv2.contains("_auto_legal_state"),
        "a 2-state (power-of-two) FSM must NOT emit the vacuous, width-mismatched \
         legal-state assertion:\n{sv2}"
    );

    let three_state = r#"
fsm Tri
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port go: in Bool;
  port q: out UInt<2>;
  state [A, B, C]
  default state A;
  default seq on clk rising;
  state A
    comb
      q = 0;
    end comb
    -> B when go;
  end state A
  state B
    comb
      q = 1;
    end comb
    -> C when go;
  end state B
  state C
    comb
      q = 2;
    end comb
    -> A when go;
  end state C
end fsm Tri
"#;
    let sv3 = compile_to_sv(three_state);
    assert!(
        sv3.contains("_auto_legal_state") && sv3.contains("state_r < 3"),
        "a 3-state (non-power-of-two) FSM must KEEP the legal-state assertion \
         (`state_r < 3`):\n{sv3}"
    );
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

#[test]
fn test_fsm_port_named_state_r_errors() {
    // `state_r` is compiler-owned FSM state storage in both SV and native sim.
    // User declarations with that name would collide with generated state.
    let source = r#"
fsm BadFsm
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port state_r: out UInt<2>;
  state [A, B]
  default state A;
  state A
    comb
      state_r = 2'd0;
    end comb
    -> B when true;
  end state A
  state B
    comb
      state_r = 2'd1;
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
    assert!(
        result.is_err(),
        "fsm with port named 'state_r' should error"
    );
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

/// Regression: a dual-clock (async) FIFO used as a SUB-INSTANCE must still
/// advance its read/write pointers in arch sim. The sim-codegen convention is
/// that a parent module drives each sub-instance by calling
/// `_inst_X.eval_posedge()` — it never calls the sub-instance's `eval()` or
/// `eval_posedge_dual()` directly. The async FIFO previously emitted an EMPTY
/// `eval_posedge() {}` (with all the sequential logic only reachable from
/// `eval()`), so a sub-instanced async FIFO never pushed/popped: push side
/// looked ready, but pop never fired and no data ever crossed the boundary.
/// `eval_posedge()` must self-gate on its own per-side clock edges and
/// dispatch to `eval_posedge_dual`.
///
/// Also guards the `_mem` element-type width: the payload type param is named
/// by the user (here `T`), not literally "TYPE", so a wide payload must lower
/// to the matching C++ integer type (`uint64_t` for `UInt<64>`), not a
/// silently-truncating `uint32_t`.
#[test]
fn test_async_fifo_subinstance_eval_posedge_and_mem_width() {
    let source = r#"
domain MDom
  freq_mhz: 200
end domain MDom

domain SDom
  freq_mhz: 100
end domain SDom

fifo WideAsyncFifo
  param DEPTH: const = 8;
  param T: type = UInt<64>;
  port wr_clk: in Clock<MDom>;
  port rd_clk: in Clock<SDom>;
  port rst: in Reset<Async, Low>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo WideAsyncFifo
"#;
    let sim = compile_to_sim_h(source, false);

    // eval_posedge() must dispatch to the dual-clock handler, NOT be an empty
    // stub. (Match flexibly on whitespace by checking the dispatch call is
    // present and an empty body is absent.)
    assert!(
        sim.contains("eval_posedge_dual(_wr_rising, _rd_rising)"),
        "async FIFO eval_posedge_dual dispatch missing from sim codegen:\n{sim}"
    );
    assert!(
        !sim.contains("::eval_posedge() {}"),
        "async FIFO eval_posedge() must not be an empty stub (sub-instances \
         would never advance their pointers):\n{sim}"
    );
    // The self-gating edge detection must live inside eval_posedge() so a
    // parent's unconditional `_inst_X.eval_posedge()` call works.
    assert!(
        sim.contains("void VWideAsyncFifo::eval_posedge()"),
        "expected a non-trivial eval_posedge() for the async FIFO:\n{sim}"
    );

    // _mem must be the full-width payload type, not a truncating uint32_t.
    assert!(
        sim.contains("uint64_t _mem["),
        "async FIFO _mem must use uint64_t for a UInt<64> payload (the type \
         param is named `T`, not \"TYPE\"); got truncating storage:\n{sim}"
    );
    assert!(
        !sim.contains("uint32_t _mem["),
        "async FIFO _mem must NOT fall back to uint32_t for a UInt<64> \
         payload:\n{sim}"
    );
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

#[test]
fn test_simple_dual_ram_latency1_no_read_enable() {
    // Regression: a latency-1 simple_dual RAM whose read port omits `en`
    // must NOT reference an undeclared `rd_port_en` in the registered-read
    // path. The read enable is optional (read every cycle), mirroring ROM.
    let source = r#"
ram NoRdEnMem
  kind simple_dual;
  latency 1;
  param DEPTH: const = 256;
  port clk: in Clock<SysDomain>;
  store
    mem: Vec<UInt<8>, DEPTH>;
  end store
  ports rd_port
    addr: in UInt<8>;
    data: out UInt<8>;
  end ports rd_port
  ports wr_port
    en:   in Bool;
    addr: in UInt<8>;
    data: in UInt<8>;
  end ports wr_port
end ram NoRdEnMem
"#;
    let sv = compile_to_sv(source);
    // No `en` declared on the read port → no `rd_port_en` anywhere.
    assert!(
        !sv.contains("rd_port_en"),
        "registered read referenced undeclared rd_port_en:\n{sv}"
    );
    // Registered read is still emitted, just unconditionally.
    assert!(sv.contains("rd_port_data_r <= mem[rd_port_addr]"));
    assert!(sv.contains("assign rd_port_data = rd_port_data_r"));
    // Write port enable is unaffected.
    assert!(sv.contains("if (wr_port_en)"));
}

#[test]
fn test_single_ram_latency1_no_chip_enable() {
    // Regression (sibling of the simple_dual fix): a latency-1 single-port RAM
    // whose port omits `en` must NOT reference an undeclared `access_en` in the
    // registered read/write path. The chip enable is optional (port always
    // enabled: reads every cycle, writes whenever `wen`).
    let source = r#"
ram NoEnSingle
  kind single;
  latency 1;
  write: no_change;
  param DEPTH: const = 256;
  port clk: in Clock<SysDomain>;
  store
    data: Vec<UInt<32>, DEPTH>;
  end store
  ports access
    wen:   in Bool;
    addr:  in UInt<8>;
    wdata: in UInt<32>;
    rdata: out UInt<32>;
  end ports access
end ram NoEnSingle
"#;
    let sv = compile_to_sv(source);
    assert!(
        !sv.contains("access_en"),
        "single-port registered path referenced undeclared access_en:\n{sv}"
    );
    // Write still gated by wen; read still emitted.
    assert!(sv.contains("if (access_wen)"));
    assert!(sv.contains("access_rdata_r <= mem[access_addr]"));
    assert!(sv.contains("assign access_rdata = access_rdata_r"));
}

#[test]
fn test_true_dual_ram_latency1_no_chip_enable() {
    // Regression (sibling of the simple_dual fix): a latency-1 true_dual RAM
    // whose ports omit `en` must NOT reference undeclared `a_en` / `b_en` in
    // the registered read/write paths. Each port is always enabled.
    let source = r#"
ram NoEnTdp
  kind true_dual;
  latency 1;
  param DEPTH: const = 256;
  port clk: in Clock<SysDomain>;
  store
    data: Vec<UInt<32>, DEPTH>;
  end store
  ports a
    wen:   in Bool;
    addr:  in UInt<8>;
    wdata: in UInt<32>;
    rdata: out UInt<32>;
  end ports a
  ports b
    wen:   in Bool;
    addr:  in UInt<8>;
    wdata: in UInt<32>;
    rdata: out UInt<32>;
  end ports b
end ram NoEnTdp
"#;
    let sv = compile_to_sv(source);
    assert!(
        !sv.contains("a_en") && !sv.contains("b_en"),
        "true_dual registered path referenced undeclared a_en/b_en:\n{sv}"
    );
    // Both ports still write-gated by their own wen and read on the else path.
    assert!(sv.contains("if (a_wen)"));
    assert!(sv.contains("if (b_wen)"));
    assert!(sv.contains("a_rdata_r <= mem[a_addr]"));
    assert!(sv.contains("b_rdata_r <= mem[b_addr]"));
}

#[test]
fn test_true_dual_ram_runs_in_native_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("TrueDualMem.arch");
    let tb_path = td.path().join("tb_true_dual.cpp");
    std::fs::write(
        &arch_path,
        r#"
//! ---
//! tags: [ram, true_dual, native_sim]
//! ---
//!
//! Regression fixture for true-dual RAM native C++ simulation.

/// 16x8 true-dual RAM with independent read/write ports.
ram TrueDualMem
  kind true_dual;
  latency 1;
  init: zero;
  param DEPTH: const = 16;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  store
    data: Vec<T, DEPTH>;
  end store
  ports a
    en: in Bool;
    wen: in Bool;
    addr: in UInt<4>;
    wdata: in T;
    rdata: out T;
  end ports a
  ports b
    en: in Bool;
    wen: in Bool;
    addr: in UInt<4>;
    wdata: in T;
    rdata: out T;
  end ports b
end ram TrueDualMem
"#,
    )
    .expect("write arch");
    std::fs::write(
        &tb_path,
        r#"
#include "VTrueDualMem.h"
#include <cstdio>

static void tick(VTrueDualMem& dut) {
  dut.clk = 0;
  dut.eval();
  dut.clk = 1;
  dut.eval();
  dut.clk = 0;
  dut.eval();
}

int main() {
  VTrueDualMem dut;

  dut.a_en = 1;
  dut.a_wen = 1;
  dut.a_addr = 3;
  dut.a_wdata = 0x2a;
  dut.b_en = 1;
  dut.b_wen = 1;
  dut.b_addr = 5;
  dut.b_wdata = 0x55;
  tick(dut);

  dut.a_wen = 0;
  dut.a_addr = 3;
  dut.b_wen = 0;
  dut.b_addr = 5;
  tick(dut);
  if (dut.a_rdata != 0x2a || dut.b_rdata != 0x55) {
    std::printf("FAIL first read a=%u b=%u\n",
                (unsigned)dut.a_rdata, (unsigned)dut.b_rdata);
    return 1;
  }

  dut.a_en = 1;
  dut.a_wen = 1;
  dut.a_addr = 7;
  dut.a_wdata = 0xc3;
  dut.b_en = 1;
  dut.b_wen = 0;
  dut.b_addr = 3;
  tick(dut);
  if (dut.b_rdata != 0x2a) {
    std::printf("FAIL read while other port writes b=%u\n",
                (unsigned)dut.b_rdata);
    return 1;
  }

  dut.a_wen = 0;
  dut.a_addr = 7;
  dut.b_en = 0;
  tick(dut);
  if (dut.a_rdata != 0xc3) {
    std::printf("FAIL read back port-a write a=%u\n",
                (unsigned)dut.a_rdata);
    return 1;
  }

  std::printf("PASS true_dual_ram_native_sim\n");
  return 0;
}
"#,
    )
    .expect("write tb");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path().join("build"))
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("PASS true_dual_ram_native_sim"),
        "true-dual RAM native sim should compile and run\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn test_true_dual_ram_sv_honors_latency() {
    let source = |latency: u32| {
        format!(
            r#"
ram TrueDualLat{latency}
  kind true_dual;
  latency {latency};
  param DEPTH: const = 16;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  store
    data: Vec<T, DEPTH>;
  end store
  ports a
    en: in Bool;
    wen: in Bool;
    addr: in UInt<4>;
    wdata: in T;
    rdata: out T;
  end ports a
  ports b
    en: in Bool;
    wen: in Bool;
    addr: in UInt<4>;
    wdata: in T;
    rdata: out T;
  end ports b
end ram TrueDualLat{latency}
"#
        )
    };

    let sv0 = compile_to_sv(&source(0));
    assert!(
        sv0.contains("assign a_rdata = mem[a_addr];")
            && sv0.contains("assign b_rdata = mem[b_addr];"),
        "latency-0 true-dual RAM should emit async read assigns:\n{sv0}"
    );
    assert!(
        !sv0.contains("a_rdata_r"),
        "latency-0 true-dual RAM should not emit read pipeline regs:\n{sv0}"
    );

    let sv2 = compile_to_sv(&source(2));
    assert!(
        sv2.contains("logic [DATA_WIDTH-1:0] a_rdata_r2;")
            && sv2.contains("logic [DATA_WIDTH-1:0] b_rdata_r2;")
            && sv2.contains("assign a_rdata = a_rdata_r2;")
            && sv2.contains("assign b_rdata = b_rdata_r2;"),
        "latency-2 true-dual RAM should emit output pipeline regs:\n{sv2}"
    );
}

#[test]
fn test_ram_latency_out_of_range_errors() {
    // RAM codegen only handles latency 0/1/2; a higher value would
    // silently emit an undriven `rdata` output. Reject it at check time.
    let source = r#"
ram BadLatRam
  kind single;
  latency 3;
  param DEPTH: const = 16;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  store
    data: Vec<T, DEPTH>;
  end store
  ports a
    en: in Bool;
    wen: in Bool;
    addr: in UInt<4>;
    wdata: in T;
    rdata: out T;
  end ports a
end ram BadLatRam
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "latency 3 RAM should be rejected");
    let errs = result.err().unwrap();
    assert!(
        errs.iter()
            .any(|e| format!("{e:?}").contains("latency 3 is out of range")),
        "error should name the out-of-range latency: {errs:?}"
    );
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
    assert!(
        result.is_err(),
        "expected error for custom policy without hook"
    );
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
    assert!(
        result.is_err(),
        "expected error for hook param shadowing port"
    );
}

/// Round-robin arbiter sim must grant index 0 on the first
/// post-reset cycle when all requesters contend, matching the SV
/// emitter (which starts the scan AT `rr_ptr_r = 0`).
///
/// Pre-fix: the sim initialized `_last_grant = 0` and scanned from
/// `(_last_grant + 1 + _i) % N`, so it started at index 1 on cycle 1
/// — diverging from SV by one slot. Steady-state matched because
/// both designs advance the pointer to `(grant + 1) % N` after a
/// grant; only the first cycle was off.
///
/// Fix: initialize `_last_grant = N - 1` at reset, so
/// `(N-1 + 1 + 0) % N = 0` and the first scan matches SV.
///
/// Surfaced in arch-hdl-lang/arch-com#447 §2.
#[test]
fn test_arbiter_round_robin_sim_inits_last_grant_to_n_minus_one() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter RRArb4
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
end arbiter RRArb4
"#;
    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("_last_grant(3)"),
        "expected `_last_grant(3)` (NUM_REQ - 1 = 3) in the emitted \
         arbiter constructor init list so the first-cycle scan starts \
         at index 0; got:\n{sim}"
    );
    assert!(
        !sim.contains("_last_grant(0)"),
        "found pre-fix `_last_grant(0)` init in the emitted arbiter; \
         the first-cycle scan would start at index 1 and diverge \
         from the SV emitter:\n{sim}"
    );
    // Steady-state behaviour unchanged: still set _last_grant to the
    // current grantee on every successful grant. The reset clause is
    // what we changed.
    assert!(
        sim.contains("if (grant_valid) _last_grant = grant_requester;"),
        "expected steady-state pointer update to remain `if (grant_valid) \
         _last_grant = grant_requester;`; got:\n{sim}"
    );
}

/// Round-robin SV pointer-advance must wrap explicitly at NUM_REQ and
/// advance from the actual grantee — not from the scan-start `rr_ptr_r`.
/// Two prior bugs combined for non-power-of-2 NUM_REQ:
///
/// 1. `rr_ptr_r <= rr_ptr_r + 1` (no explicit `% NUM_REQ`). The
///    `clog2(NUM_REQ)`-bit register could hold values >= NUM_REQ, e.g.
///    `rr_ptr_r=3` with NUM_REQ=3 — the scan formula's explicit
///    `% NUM_REQ` masked the comparison, so idx 0 won twice in a row
///    whenever the pointer happened to land on 3.
///
/// 2. Advancing the scan-start, not the grantee. When the scan walked
///    past non-asserting requesters, `grant_requester` could be > the
///    starting `rr_ptr_r`; the next-cycle scan would then re-start
///    earlier than the just-granted slot and re-prioritize it.
///
/// For NUM_REQ=3 with all reqs asserted: pre-fix grants the sequence
/// `0,1,2,0,0,1,2,0,...` (idx 0 = 50%, idxs 1,2 = 25% each). After
/// fix: strict `0,1,2,0,1,2,...` round-robin, idx-fair.
///
/// Power-of-2 NUM_REQ was unaffected — bit-width truncation
/// coincidentally implemented the right thing.
///
/// See arch-hdl-lang/arch-com#451 (cycle-1 sim fix) and #447 §2.
#[test]
fn test_arbiter_round_robin_sv_advances_from_grantee_with_explicit_wrap() {
    // NUM_REQ=3 is the canonical non-power-of-2 case that exposes both
    // sub-bugs simultaneously.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter RRArb3
  policy round_robin;
  param NUM_REQ: const = 3;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter RRArb3
"#;
    let sv = compile_to_sv(source);
    // Pointer advance must reference grant_requester (the actual grantee),
    // not rr_ptr_r (the scan-start). With explicit wrap at NUM_REQ - 1.
    assert!(
        sv.contains("rr_ptr_r <= (grant_requester == 2'(3 - 1)) ? '0 : grant_requester + 1'b1;"),
        "expected grantee-based pointer advance with explicit NUM_REQ wrap; got:\n{sv}"
    );
    // The pre-fix shape must be gone.
    assert!(
        !sv.contains("rr_ptr_r <= rr_ptr_r + 1;"),
        "found pre-fix `rr_ptr_r <= rr_ptr_r + 1;` — the scan-start-based \
         advance is incorrect for non-power-of-2 NUM_REQ:\n{sv}"
    );
}

/// End-to-end Verilator test for the SV round-robin fairness fix.
///
/// Builds RRArb3 (NUM_REQ=3, the canonical non-power-of-2 case) to SV,
/// runs it under Verilator with all three requesters always asserted,
/// and checks that the grant_requester sequence is strict
/// A construct already defined in the input files must not also be pulled in
/// from a stale `.archi` by auto-discovery — doing so emitted the construct
/// twice (the stub copy missing its port-array ports → broken SV). The
/// `.archi`-discovery defined-name set tracked module/fsm/fifo/ram/arbiter/...
/// but omitted `regfile` (and cam/clkgate/linklist), so an in-source `regfile`
/// + a present `Rf1.archi` (e.g. from a prior build) duplicate-emitted it.
#[test]
fn test_inscope_construct_not_duplicated_by_stale_archi() {
    use std::fs;
    let td = tempfile::tempdir().expect("tempdir");
    let dir = td.path();
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let regfile = "\
regfile Rf1
  param NREGS: const = 4;
  param T: type = UInt<32>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[1] read
    addr: in UInt<2>;
    data: out UInt<32>;
  end ports read
  ports[1] write
    en:   in Bool;
    addr: in UInt<2>;
    data: in UInt<32>;
  end ports write
end regfile Rf1
";
    // 1) Build the regfile alone so its `Rf1.archi` lands in `dir`.
    let rf_path = dir.join("Rf1.arch");
    fs::write(&rf_path, regfile).unwrap();
    let b1 = std::process::Command::new(arch_bin)
        .arg("build")
        .arg(&rf_path)
        .arg("-o")
        .arg(dir.join("Rf1.sv"))
        .output()
        .expect("build regfile");
    assert!(
        b1.status.success(),
        "regfile build failed: {}",
        String::from_utf8_lossy(&b1.stderr)
    );
    assert!(
        dir.join("Rf1.archi").exists(),
        "Rf1.archi should have been written"
    );

    // 2) Build a combined file that DEFINES Rf1 in-source AND insts it, in the
    //    same dir as the stale Rf1.archi. The construct must emit exactly once.
    let combined = format!(
        "{regfile}
module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<2>;
  port d: out UInt<32>;
  wire dw: UInt<32>;
  inst rf: Rf1
    clk <- clk;
    rst <- rst;
    read.addr <- a;
    read.data -> dw;
    write.en  <- false;
    write.addr <- 0;
    write.data <- 0;
  end inst rf
  comb
    d = dw;
  end comb
end module Top
"
    );
    let top_path = dir.join("Top.arch");
    fs::write(&top_path, combined).unwrap();
    let sv_out = dir.join("Top.sv");
    let b2 = std::process::Command::new(arch_bin)
        .arg("build")
        .arg(&top_path)
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build combined");
    assert!(
        b2.status.success(),
        "combined build failed: {}",
        String::from_utf8_lossy(&b2.stderr)
    );
    let sv = fs::read_to_string(&sv_out).unwrap();
    let n = sv.matches("\nmodule Rf1").count() + sv.starts_with("module Rf1") as usize;
    assert_eq!(
        n, 1,
        "regfile defined in-source must emit exactly once even with a stale \
         Rf1.archi present (got {n}):\n{sv}"
    );
}

/// round-robin (each idx wins exactly 1/3 of cycles).
///
/// Pre-fix grant pattern was `0,1,2,0,0,1,2,0,...` (idx 0 at 50%).
/// After fix: `0,1,2,0,1,2,...`, idx-fair.
#[test]
fn test_arbiter_round_robin_sv_nonpow2_verilator_behavior() {
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping Verilator RR NUM_REQ=3 fairness smoke: verilator not found");
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("RRArb3.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/arbiter_rr_nonpow2/RRArb3.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build RRArb3 SV");
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
        .arg("RRArb3")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/arbiter_rr_nonpow2/tb_rr_arb3.cpp")
        .output()
        .expect("verilate RRArb3");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VRRArb3");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator RRArb3");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS rr_arb3"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
}

/// arch-sim cross-check for the same RR fairness fixture, closing the §3
/// gap from `ideas/2026-05-28-code-review-findings.md`: the prior PR #452
/// only ran Verilator against RRArb3, leaving the arch-sim path covered
/// only by a substring grep on the emitted header. This test runs the same
/// `tb_rr_arb3.cpp` driver through the arch-sim backend so any future
/// divergence between the SV scheduler and the sim scheduler trips both
/// tests (or neither), not just Verilator.
#[test]
fn test_arbiter_round_robin_arch_sim_nonpow2_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/arbiter_rr_nonpow2/RRArb3.arch")
        .arg("--tb")
        .arg("tests/arbiter_rr_nonpow2/tb_rr_arb3.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for RRArb3");
    assert!(
        out.status.success(),
        "arch sim should pass for RRArb3\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PASS rr_arb3"),
        "expected `PASS rr_arb3` (strict round-robin, idx-fair) in arch \
         sim stdout — the same fairness contract Verilator verifies; got:\n{stdout}"
    );
}

// ── ARCH_CXX env override selects the C++ compiler for `arch sim` ──────────────
//
// `arch sim` compiles the generated C++ testbench/sim with a C++ compiler that
// defaults to `g++`. On Linux, real GCC miscompiles harc's C++20 coroutine
// testbench scheduler, so harc-driven testbenches need `ARCH_CXX=clang++`
// (mirrors harc's own `HARC_CXX` knob). These two tests pin the override:
//   - a bogus `ARCH_CXX` must make the sim build fail (proves the var is
//     actually consulted, independent of which compilers are installed);
//   - `ARCH_CXX=clang++` must still build and pass when clang++ is available.
#[test]
fn test_arch_cxx_override_bogus_compiler_fails_sim_build() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .env("ARCH_CXX", "arch-no-such-cxx-compiler-xyz")
        .arg("sim")
        .arg("tests/arbiter_rr_nonpow2/RRArb3.arch")
        .arg("--tb")
        .arg("tests/arbiter_rr_nonpow2/tb_rr_arb3.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim with bogus ARCH_CXX");
    assert!(
        !out.status.success(),
        "arch sim must fail when ARCH_CXX names a non-existent compiler — \
         proves the override is consulted at the compile site\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_arch_cxx_override_clang_builds_and_passes() {
    // Skip cleanly if clang++ is not on PATH (e.g. a minimal CI image).
    let have_clang = std::process::Command::new("clang++")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have_clang {
        eprintln!("skipping: clang++ not available on PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .env("ARCH_CXX", "clang++")
        .arg("sim")
        .arg("tests/arbiter_rr_nonpow2/RRArb3.arch")
        .arg("--tb")
        .arg("tests/arbiter_rr_nonpow2/tb_rr_arb3.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim with ARCH_CXX=clang++");
    assert!(
        out.status.success(),
        "arch sim should pass with ARCH_CXX=clang++\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PASS rr_arb3"),
        "expected `PASS rr_arb3` under ARCH_CXX=clang++; got:\n{stdout}"
    );
}

// ── Variable-index Vec<Bus> element access — backend equivalence ──────────────
//
// Regression for the silent backend-capability divergence where `arch build`
// lowered `o[sel].valid` / `wait until o[sel].ready` correctly but `arch sim`
// (and, for the thread case, both backends) mis-lowered the variable index.
// See tests/backend_equiv/Fx3bVarIndexVecBusBug.arch (comb) and
// Fx3bVarIndexVecBusThread.arch (thread `wait until`).

/// Codegen-level guard (no Verilator needed): the comb-block variable index
/// must flatten to the packed-array form in BOTH backends, never the scalar
/// bit-select against an undefined bus name.
#[test]
fn test_var_index_vec_bus_comb_lowering_matches_backends() {
    let source = concat!(
        include_str!("backend_equiv/BusVr.arch"),
        "\n",
        include_str!("backend_equiv/Fx3bVarIndexVecBusBug.arch")
    );
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("o_valid[sel]"),
        "SV must select lane `sel`:\n{sv}"
    );
    assert!(
        sv.contains("o_data[sel]"),
        "SV must select lane `sel`:\n{sv}"
    );
    assert!(
        !sv.contains("o[sel]"),
        "SV must not leave the un-flattened Vec<Bus> index `o[sel]`:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("o_valid[sel]") && sim.contains("o_data[sel]"),
        "sim C++ must select lane `sel` from the packed array, mirroring SV:\n{sim}"
    );
    assert!(
        !sim.contains("(o) >> (sel)"),
        "sim C++ must not mis-lower the Vec<Bus> element to a scalar bit-select \
         against an undefined `o`:\n{sim}"
    );
}

/// Codegen-level guard for the `thread` `wait until` mirror: the variable
/// index lowers to a runtime mux over per-lane flattened sub-module ports in
/// both backends.
#[test]
fn test_var_index_vec_bus_thread_lowering_matches_backends() {
    let source = include_str!("backend_equiv/Fx3bVarIndexVecBusThread.arch");
    let sv = compile_to_sv(source);
    // Per-lane flattened sub-module input ports + the runtime lane mux.
    for lane in ["o_0_ready", "o_1_ready", "o_2_ready", "o_3_ready"] {
        assert!(
            sv.contains(lane),
            "SV thread sub-module needs port `{lane}`:\n{sv}"
        );
    }
    assert!(
        !sv.contains("o[sel]"),
        "SV must not leave the un-flattened Vec<Bus> index `o[sel]` in the \
         thread sub-module:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("o_0_ready") && sim.contains("o_3_ready"),
        "sim C++ thread sub-module must read the flattened per-lane signals:\n{sim}"
    );
    assert!(
        !sim.contains("(o) >> (sel)"),
        "sim C++ must not mis-lower the thread-condition Vec<Bus> read:\n{sim}"
    );
}

/// End-to-end value parity: run the comb and thread fixtures through `arch
/// sim` (always) and `arch build` + Verilator (when available), asserting the
/// same PASS markers from both backends. A regression makes the mis-lowered
/// backend fail to compile, so the PASS assertion is the real guard.
#[test]
fn test_var_index_vec_bus_backend_equivalence_e2e() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    // Helper: run `arch sim <arch> --tb <tb>` and assert a PASS marker.
    let run_arch_sim = |archs: &[&str], tb: &str, marker: &str| {
        let td = tempfile::tempdir().expect("tempdir");
        let mut cmd = std::process::Command::new(arch_bin);
        cmd.arg("sim");
        for arch in archs {
            cmd.arg(arch);
        }
        let out = cmd
            .arg("--tb")
            .arg(tb)
            .arg("--outdir")
            .arg(td.path())
            .output()
            .expect("run arch sim");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success() && stdout.contains(marker),
            "arch sim should pass for {archs:?}\nwant marker: {marker}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    };

    run_arch_sim(
        &[
            "tests/backend_equiv/BusVr.arch",
            "tests/backend_equiv/Fx3bVarIndexVecBusBug.arch",
        ],
        "tests/backend_equiv/Vsel_arch_tb.cpp",
        "PASS vsel_varidx",
    );
    run_arch_sim(
        &["tests/backend_equiv/Fx3bVarIndexVecBusThread.arch"],
        "tests/backend_equiv/VselThread_arch_tb.cpp",
        "PASS vsel_thread_varidx",
    );

    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping Verilator parity leg: verilator not found");
        return;
    }

    // Helper: build SV, verilate with the matching TB, run, assert PASS marker.
    let run_verilator = |archs: &[&str], tb: &str, top: &str, marker: &str| {
        let td = tempfile::tempdir().expect("tempdir");
        let sv_out = td.path().join(format!("{top}.sv"));
        let obj_dir = td.path().join("obj_dir");
        let mut cmd = std::process::Command::new(arch_bin);
        cmd.arg("build");
        for arch in archs {
            cmd.arg(arch);
        }
        let build = cmd.arg("-o").arg(&sv_out).output().expect("arch build");
        assert!(
            build.status.success(),
            "arch build should pass for {archs:?}\nstderr:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let tb_abs = std::fs::canonicalize(tb).expect("tb path");
        let verilate = std::process::Command::new("verilator")
            .args([
                "--cc",
                "--exe",
                "--build",
                "-Wno-WIDTH",
                "-Wno-UNOPTFLAT",
                "-Wno-DECLFILENAME",
                "--top-module",
                top,
                "-Mdir",
            ])
            .arg(&obj_dir)
            .arg(&sv_out)
            .arg(&tb_abs)
            .output()
            .expect("verilate");
        assert!(
            verilate.status.success(),
            "Verilator build should pass for {archs:?}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&verilate.stdout),
            String::from_utf8_lossy(&verilate.stderr)
        );
        let run = std::process::Command::new(obj_dir.join(format!("V{top}")))
            .output()
            .expect("run verilator sim");
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            run.status.success() && stdout.contains(marker),
            "Verilator sim should pass for {archs:?}\nwant marker: {marker}\nstdout:\n{stdout}"
        );
    };

    run_verilator(
        &[
            "tests/backend_equiv/BusVr.arch",
            "tests/backend_equiv/Fx3bVarIndexVecBusBug.arch",
        ],
        "tests/backend_equiv/Vsel_vl_tb.cpp",
        "Vsel",
        "PASS vsel_varidx",
    );
    run_verilator(
        &["tests/backend_equiv/Fx3bVarIndexVecBusThread.arch"],
        "tests/backend_equiv/VselThread_vl_tb.cpp",
        "VselThread",
        "PASS vsel_thread_varidx",
    );
}

/// Vec<Bus> *wire* mirror: a variable index into a `wire w: Vec<B,N>;`
/// must resolve to the per-element struct-array storage in the sim backend
/// (`_let_w[sel].v`), matching the SV packed-slice form (`w_v[sel]`).
#[test]
fn test_var_index_vec_bus_wire_lowering_matches_backends() {
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B

        module M
          port sel: in UInt<1>;
          port o_v: out Bool;
          port o_d: out UInt<8>;
          wire w: Vec<B, 2>;
          comb
            w[0].v = true;  w[0].d = 8'h11;
            w[1].v = false; w[1].d = 8'h22;
            o_v = w[sel].v;
            o_d = w[sel].d;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("w_v[sel]") && sv.contains("w_d[sel]"),
        "SV must slice the packed wire:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("_let_w[sel].v") && sim.contains("_let_w[sel].d"),
        "sim C++ must index the struct-array wire storage:\n{sim}"
    );
    assert!(
        !sim.contains("(w) >> (sel)"),
        "sim C++ must not mis-lower the Vec<Bus> wire element to a scalar bit-select:\n{sim}"
    );
}

/// Variable-index Vec<Bus> *write* inside a thread: codegen-level guard.
/// The write must expand to a per-lane demux (`if (sel == i) o_i_field …`)
/// driving all flattened lanes in BOTH backends — never a dangling `o[sel]`.
#[test]
fn test_var_index_vec_bus_thread_write_lowering_matches_backends() {
    let source = include_str!("backend_equiv/Fx3bVarIndexVecBusThreadWrite.arch");
    let sv = compile_to_sv(source);
    for lane in ["o_0_valid", "o_1_valid", "o_2_valid", "o_3_valid"] {
        assert!(
            sv.contains(lane),
            "SV thread sub-module must drive lane `{lane}`:\n{sv}"
        );
    }
    assert!(
        sv.contains("if (sel == 0)") && sv.contains("o_0_valid <= 1'b1"),
        "SV must expand the variable-index write to a per-lane demux:\n{sv}"
    );
    assert!(
        !sv.contains("o[sel]"),
        "SV must not leave the un-flattened Vec<Bus> write `o[sel]`:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("if (sel == 0)") && sim.contains("o_0_valid"),
        "sim C++ must expand the variable-index write to a per-lane demux:\n{sim}"
    );
}

/// Variable-index Vec<Bus> *write* end-to-end value parity: `arch sim` and
/// (when available) `arch build` + Verilator must agree that lane `sel`
/// carries the written (valid, data) and all other lanes stay cleared.
#[test]
fn test_var_index_vec_bus_thread_write_backend_equivalence_e2e() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let arch = "tests/backend_equiv/Fx3bVarIndexVecBusThreadWrite.arch";

    // arch sim leg (always).
    let td = tempfile::tempdir().expect("tempdir");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg(arch)
        .arg("--tb")
        .arg("tests/backend_equiv/VselWr_arch_tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("PASS vselwr_varidx"),
        "arch sim should pass for the variable-index write\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verilator leg (when available).
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping Verilator parity leg: verilator not found");
        return;
    }
    let td2 = tempfile::tempdir().expect("tempdir");
    let sv_out = td2.path().join("VselWr.sv");
    let obj_dir = td2.path().join("obj_dir");
    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg(arch)
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("arch build");
    assert!(
        build.status.success(),
        "arch build should pass\nstderr:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let tb_abs = std::fs::canonicalize("tests/backend_equiv/VselWr_vl_tb.cpp").expect("tb path");
    let verilate = std::process::Command::new("verilator")
        .args([
            "--cc",
            "--exe",
            "--build",
            "-Wno-WIDTH",
            "-Wno-UNOPTFLAT",
            "-Wno-DECLFILENAME",
            "--top-module",
            "VselWr",
            "-Mdir",
        ])
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg(&tb_abs)
        .output()
        .expect("verilate");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );
    let run = std::process::Command::new(obj_dir.join("VVselWr"))
        .output()
        .expect("run verilator sim");
    let vl_stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success() && vl_stdout.contains("PASS vselwr_varidx"),
        "Verilator sim should pass for the variable-index write\nstdout:\n{vl_stdout}"
    );
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
    assert!(
        sv.contains("input logic [NUM_REQ-1:0] request_valid"),
        "expected request_valid array port:\n{sv}"
    );
    assert!(
        sv.contains("output logic [NUM_REQ-1:0] request_ready"),
        "expected request_ready array port:\n{sv}"
    );
    // Payload flows in the same direction as `receive` (in to the arbiter).
    // Width comes from the field type; SV declares one wire per index slot.
    assert!(
        sv.contains("request_qos"),
        "expected request_qos payload port:\n{sv}"
    );
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
    assert!(
        sv.contains("// synopsys translate_off"),
        "expected translate_off wrapper:\n{sv}"
    );
    assert!(
        sv.contains("// synopsys translate_on"),
        "expected translate_on wrapper:\n{sv}"
    );
    assert!(
        sv.contains("// Auto-generated handshake protocol assertions"),
        "expected Tier-2 header comment:\n{sv}"
    );
    assert!(
        sv.contains("generate for (genvar i = 0; i < NUM_REQ; i++) begin: g_auto_hs_request"),
        "expected genvar-indexed generate block over NUM_REQ:\n{sv}"
    );
    assert!(
        sv.contains("end endgenerate"),
        "expected generate block close:\n{sv}"
    );
    // Property uses lane-indexed signals + disable iff (rst) + the same
    // `(v && !r) |=> v` predicate as the bus-side emitter.
    assert!(
        sv.contains("_auto_hs_request__lane_valid_stable"),
        "expected per-lane valid_stable label:\n{sv}"
    );
    assert!(
        sv.contains("disable iff (rst)"),
        "expected reset-disable clause:\n{sv}"
    );
    assert!(
        sv.contains("(request_valid[i] && !request_ready[i]) |=> request_valid[i]"),
        "expected lane-indexed valid-stable predicate:\n{sv}"
    );
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
    assert!(
        sv.contains("_auto_hs_grant_valid_stable"),
        "expected bare grant valid_stable label:\n{sv}"
    );
    // Crucially, no generate-for wrapper for the non-array channel.
    assert!(
        !sv.contains("g_auto_hs_grant"),
        "non-array handshake_channel must not be wrapped in generate-for:\n{sv}"
    );
    // And the predicate uses unindexed signal names.
    assert!(
        sv.contains("(grant_valid && !grant_ready) |=> grant_valid"),
        "expected unindexed grant valid-stable predicate:\n{sv}"
    );
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
    assert!(
        !sv.contains("_auto_hs_grant"),
        "valid_only handshake_channel must not emit Tier-2 SVA:\n{sv}"
    );
    // The Tier-2 wrapper itself must be elided too when no channel
    // produced any property (matches bus-side emit_handshake_asserts).
    assert!(
        !sv.contains("Auto-generated handshake protocol assertions"),
        "Tier-2 wrapper must not be emitted when no property applies:\n{sv}"
    );
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
    assert!(
        sv.contains("_auto_hs_request__lane_valid_stable"),
        "expected valid_stable property:\n{sv}"
    );
    // No `$stable` payload-stability check (Tier-2 v1 scope explicitly
    // doesn't include payload-stability, matching bus-side behaviour).
    assert!(
        !sv.contains("$stable"),
        "Tier-2 v1 must not emit payload-stability $stable checks:\n{sv}"
    );
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
    assert!(
        !sv.contains("g_auto_hs_aw"),
        "bus-path Tier-2 SVA must remain non-generate-wrapped:\n{sv}"
    );
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
    assert!(
        sv_n.contains("output logic grant_valid"),
        "expected grant_valid top-level port:\n{sv_n}"
    );
    assert!(
        sv_n.contains("output logic [2-1:0] grant_requester")
            || sv_n.contains("output logic [1:0] grant_requester"),
        "expected grant_requester top-level port:\n{sv_n}"
    );
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
    assert!(
        sv.contains("if ((!rst_n))"),
        "expected inverted reset condition, got:\n{sv}"
    );
    // must NOT contain bare active-high check
    assert!(
        !sv.contains("if (rst_n)"),
        "unexpected active-high reset check:\n{sv}"
    );
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
    assert!(
        result.is_err(),
        "expected type error for implicit truncation"
    );
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| format!("{e:?}").contains("width mismatch")
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
    assert!(
        sv.contains("input logic [N-1:0] req"),
        "expected Vec req port, got:\n{sv}"
    );
    assert!(
        sv.contains("output logic [N-1:0] gnt"),
        "expected Vec gnt port, got:\n{sv}"
    );
    // generate_for over `inst` items with shape-stable connections (scalar
    // child ports + `Index(Ident, loop_var)` against a Vec parent port,
    // no Vec-of-bus shapes) preserves the SV genvar `for begin gen_i end`
    // form. One compact block, scales to any N.
    assert!(
        sv.contains("genvar i;"),
        "expected `genvar i;` for shape-stable inst-bearing generate_for, got:\n{sv}"
    );
    assert!(
        sv.contains("begin : gen_i"),
        "expected `begin : gen_i` block, got:\n{sv}"
    );
    // Inside the gen_i block the inst is named `pt_i` (the loop-var-
    // bearing source name) — substitution to `pt_0`/`pt_1` happens at
    // SV-elaboration time, not at arch-com elaboration.
    assert!(
        sv.contains("PassThrough pt_i"),
        "expected `PassThrough pt_i` instance in the gen_i block, got:\n{sv}"
    );
    assert!(
        !sv.contains("PassThrough pt_0"),
        "shape-stable case should NOT emit flat `pt_0`, got:\n{sv}"
    );
    insta::assert_snapshot!(sv);
}

#[test]
fn test_generate_for_inst_genvar_sim_behavior() {
    // arch sim's local unroll of preserved Generate(For) blocks must
    // produce the same wire-through behavior as the pre-#399 unroll-at-
    // elaboration path. Without the sim-side flatten pass, eval_comb()
    // would silently skip the Generate block and gnt would never assert.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("examples/generate_for.arch")
        .arg("--tb")
        .arg("examples/tb_generate_for_genvar.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for generate_for genvar probe");
    assert!(
        out.status.success(),
        "generate_for genvar sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS generate_for genvar sim"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_generate_for_inst_genvar_large_n() {
    // Probe at N=8 to demonstrate compact SV genvar form. Without the
    // shape-stable preservation this would emit 8 flat inst blocks.
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module PassThrough
  port a: in  Bool;
  port b: out Bool;

  comb
    b = a;
  end comb
end module PassThrough

module Big
  param N: const = 8;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port req: in  Vec<Bool, N>;
  port gnt: out Vec<Bool, N>;

  generate_for i in 0..N-1
    inst pt_i: PassThrough
      a <- req[i];
      b -> gnt[i];
    end inst pt_i
  end generate_for
end module Big
"#;
    let sv = compile_to_sv(source);
    // Exactly ONE genvar block, not 8 flat insts.
    assert!(sv.contains("genvar i;"), "expected `genvar i;`, got:\n{sv}");
    assert!(
        sv.contains("for (i = 0; i <= N - 1; i = i + 1) begin : gen_i"),
        "expected SV genvar `for` loop, got:\n{sv}"
    );
    // No literal-i instance names — substitution is deferred to SV elaboration.
    assert!(
        !sv.contains("PassThrough pt_0"),
        "did not expect literal `pt_0` flat inst, got:\n{sv}"
    );
    assert!(
        !sv.contains("PassThrough pt_7"),
        "did not expect literal `pt_7` flat inst, got:\n{sv}"
    );
    // Single instantiation in the source body.
    let pt_count = sv.matches("PassThrough pt_i").count();
    assert_eq!(
        pt_count, 1,
        "expected exactly one `PassThrough pt_i`, got {pt_count}:\n{sv}"
    );
}

#[test]
fn test_generate_if_true() {
    let source = include_str!("../examples/generate_if.arch");
    let sv = compile_to_sv(source);
    // generate_if true → debug_out port is included
    assert!(
        sv.contains("debug_out"),
        "expected debug_out port, got:\n{sv}"
    );
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
    assert!(
        sv.contains("debug_out"),
        "expected debug_out when ENABLE_DEBUG=1, got:\n{sv}"
    );
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
    assert!(
        !sv.contains("debug_out"),
        "debug_out should be excluded when ENABLE_DEBUG=0, got:\n{sv}"
    );
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
    assert!(
        sv.contains("verbose_out"),
        "expected verbose_out when LOG_LEVEL=2 > 1, got:\n{sv}"
    );
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
    assert!(
        sv.contains("debug_out"),
        "Inner should have debug_out when ENABLE_DEBUG=1:\n{sv}"
    );
    assert!(
        sv.contains("module Inner"),
        "module should keep original name for single variant:\n{sv}"
    );
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
    assert!(
        !sv.contains("debug_out"),
        "Inner2 should NOT have debug_out when ENABLE_DEBUG=0:\n{sv}"
    );
    assert!(
        sv.contains("module Inner2"),
        "module should keep original name for single variant:\n{sv}"
    );
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
    assert!(
        sv.contains("module Sub__ENABLE_0"),
        "expected Sub__ENABLE_0 module:\n{sv}"
    );
    assert!(
        sv.contains("module Sub__ENABLE_1"),
        "expected Sub__ENABLE_1 module:\n{sv}"
    );
    // Top's inst blocks must reference the renamed variants
    assert!(
        sv.contains("Sub__ENABLE_1"),
        "Top should reference Sub__ENABLE_1:\n{sv}"
    );
    assert!(
        sv.contains("Sub__ENABLE_0"),
        "Top should reference Sub__ENABLE_0:\n{sv}"
    );
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
    assert!(
        sv.contains("module Inner__ENABLE_DEBUG_0"),
        "missing Inner__ENABLE_DEBUG_0:\n{sv}"
    );
    assert!(
        sv.contains("module Inner__ENABLE_DEBUG_1"),
        "missing Inner__ENABLE_DEBUG_1:\n{sv}"
    );
    // ENABLE_DEBUG=1 variant has debug_in port; ENABLE_DEBUG=0 does not.
    // Verify by checking what each module declaration contains.
    let debug_1_block = sv
        .split("module Inner__ENABLE_DEBUG_1")
        .nth(1)
        .and_then(|s| s.split("endmodule").next())
        .unwrap_or("");
    let debug_0_block = sv
        .split("module Inner__ENABLE_DEBUG_0")
        .nth(1)
        .and_then(|s| s.split("endmodule").next())
        .unwrap_or("");
    assert!(
        debug_1_block.contains("debug_in"),
        "ENABLE_DEBUG=1 variant missing debug_in:\n{sv}"
    );
    assert!(
        !debug_0_block.contains("debug_in"),
        "ENABLE_DEBUG=0 variant should not have debug_in:\n{sv}"
    );
    // Inst sites reference the correct variants (params appear between name and instance)
    assert!(
        sv.contains("Inner__ENABLE_DEBUG_1") && sv.contains("inner_on"),
        "inner_on should use _1 variant:\n{sv}"
    );
    assert!(
        sv.contains("Inner__ENABLE_DEBUG_0") && sv.contains("inner_off"),
        "inner_off should use _0 variant:\n{sv}"
    );
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
    assert!(
        !sv.contains("debug_out"),
        "debug_out should be excluded when condition is false, got:\n{sv}"
    );
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
    assert!(
        sv.contains("if (rst) begin"),
        "expected reset guard, got:\n{sv}"
    );
    assert!(
        sv.contains("count_r <= 0;"),
        "expected count_r reset init, got:\n{sv}"
    );
    // pipe_r has reset none: must be in a SEPARATE always_ff block (no reset in sensitivity list).
    // Mixing resetable and non-resetable regs in one always_ff with async reset causes
    // synthesis tools to infer unintended clock gating on the reset path.
    let always_blocks: Vec<&str> = sv.split("always_ff").collect();
    assert!(
        always_blocks.len() >= 3,
        "expected at least 2 always_ff blocks (reset + no-reset), got:\n{sv}"
    );
    // The second always_ff should contain pipe_r and NOT have reset in sensitivity
    let second_block = always_blocks[2];
    assert!(
        second_block.contains("pipe_r <= data_in"),
        "pipe_r should be in separate always_ff, got:\n{sv}"
    );
    assert!(
        !second_block.contains("rst"),
        "no-reset always_ff should not reference rst, got:\n{sv}"
    );
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
    assert!(
        sv.contains("always_ff @(posedge clk)"),
        "expected an always_ff for the reset-only reg, got:\n{sv}"
    );
    assert!(sv.contains("if (rst)"), "expected reset guard, got:\n{sv}");
    // arch-com emits the literal as decimal; accept either form since
    // what matters is that the RHS is the reset value (4).
    assert!(
        sv.contains("roconst_r <= 32'd4;") || sv.contains("roconst_r <= 32'h4;"),
        "expected roconst_r reset-init assignment, got:\n{sv}"
    );
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
    assert!(
        sv.contains("ticker_r <= 0;"),
        "expected ticker_r reset in seq-block always_ff, got:\n{sv}"
    );
    // Orphan reset always_ff fires for constant_r.
    assert!(
        sv.contains("constant_r <= 32'd42;"),
        "expected constant_r orphan reset assignment, got:\n{sv}"
    );
    // Two distinct always_ff blocks — one for each.
    let always_count = sv.matches("always_ff @").count();
    assert!(
        always_count >= 2,
        "expected >=2 always_ff blocks (seq + orphan), got {always_count}:\n{sv}"
    );
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
    assert!(
        result.is_err(),
        "expected error for mixed reset signals in same always block"
    );
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
    assert!(
        result.is_err(),
        "expected error for mixing sync and async reset in same always block"
    );
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

#[test]
fn test_simple_pipeline() {
    let source = include_str!("../examples/simple_pipeline.arch");
    let sv = compile_to_sv(source);
    assert!(sv.contains("module SimplePipe"), "missing module header");
    assert!(sv.contains("fetch_valid_r"), "missing fetch valid register");
    assert!(
        sv.contains("writeback_valid_r"),
        "missing writeback valid register"
    );
    assert!(
        sv.contains("fetch_captured"),
        "missing fetch stage register"
    );
    assert!(
        sv.contains("writeback_result"),
        "missing writeback stage register"
    );
    assert!(sv.contains("always_ff"), "missing always_ff block");
    assert!(
        sv.contains("assign data_out = writeback_result"),
        "missing comb output"
    );
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
    assert!(
        result.is_err(),
        "expected error for comb-only pipeline stage"
    );
}

/// `arch check` accepts `source` (returns Ok) iff there are no type errors.
fn pipeline_checks_ok(source: &str) -> bool {
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let elaborated = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&elaborated).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &elaborated);
    checker.check().is_ok()
}

// A `pipeline` output must come from a stage register, never a combinational
// path through an input port. These three tests are differential: the ALLOW
// case (output from a register) and the two REJECT cases (direct + via-let
// comb passthrough) differ only in how the output is driven, so the delta
// isolates exactly the new rule. Rationale: a comb input→output path inside a
// pipeline is hidden from the whole-design comb-loop detector (which models a
// pipeline inst as registered/PURE), so a feedback loop through it would go
// undetected — Verilator flags the same SV `UNOPTFLAT`.

#[test]
fn test_pipeline_allows_comb_output_from_register() {
    // `y = held` reads a stage REGISTER — no comb path from input `a` to
    // output `y`. Must be accepted.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline RegPipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;
  stage S
    reg held: UInt<8> reset rst => 0;
    seq on clk rising
      held <= a;
    end seq
    comb
      y = held;
    end comb
  end stage S
end pipeline RegPipe
"#;
    assert!(
        pipeline_checks_ok(source),
        "a pipeline output driven from a stage register must be accepted"
    );
}

#[test]
fn test_pipeline_sim_codegen_sign_extends_sext() {
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline SextPipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out SInt<16>;
  stage S
    reg held: SInt<16> reset rst => 0;
    seq on clk rising
      held <= a.sext<16>();
    end seq
    comb
      y = held;
    end comb
  end stage S
end pipeline SextPipe
"#;
    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("((a >> 7) & 1)")
            && sim.contains("~((uint16_t)0) << 8")
            && sim.contains("(uint16_t)(a)"),
        "pipeline simulator .sext<16>() should sign-extend from bit 7:\n{sim}"
    );
}

#[test]
fn test_pipeline_comb_match_emits_case() {
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline MatchPipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port sel: in UInt<2>;
  port y: out UInt<8>;
  stage S
    reg sel_r: UInt<2> reset rst => 0;
    seq on clk rising
      sel_r <= sel;
    end seq
    comb
      match sel_r
        2'd0 => y = 8'h11;
        2'd1 => y = 8'h22;
        _ => y = 8'h33;
      end match
    end comb
  end stage S
end pipeline MatchPipe
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("case (s_sel_r)")
            && sv.contains("2'd0: begin")
            && sv.contains("y = 8'd17;")
            && sv.contains("default: begin")
            && sv.contains("y = 8'd51;")
            && sv.contains("endcase"),
        "pipeline comb match should emit a stage-aware case statement:\n{sv}"
    );
}

#[test]
fn test_pipeline_rejects_comb_input_to_output_passthrough() {
    // `y = a` drives an output straight from an input — direct comb passthrough.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline CombPipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;
  stage S
    reg dummy: UInt<8> reset rst => 0;
    seq on clk rising
      dummy <= a;
    end seq
    comb
      y = a;
    end comb
  end stage S
end pipeline CombPipe
"#;
    assert!(
        !pipeline_checks_ok(source),
        "a pipeline output driven combinationally from an input must be rejected"
    );
}

#[test]
fn test_pipeline_rejects_comb_input_to_output_via_let() {
    // Transitive: `let t = a + 1; ... y = t` — `t` carries the input
    // combinationally into output `y`. Must be rejected (taint through let).
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline LetPipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port y: out UInt<8>;
  stage S
    let t: UInt<8> = a +% 1;
    reg dummy: UInt<8> reset rst => 0;
    seq on clk rising
      dummy <= a;
    end seq
    comb
      y = t;
    end comb
  end stage S
end pipeline LetPipe
"#;
    assert!(
        !pipeline_checks_ok(source),
        "a pipeline output driven from a let that reads an input must be rejected"
    );
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
    assert!(
        result.is_err(),
        "expected error for undeclared flush target stage"
    );
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
    assert!(
        sv.contains("(!imem_valid)"),
        "missing Fetch stall condition"
    );
    // Backpressure propagation
    assert!(
        sv.contains("fetch_stall = (!imem_valid) || decode_stall"),
        "missing backpressure chain"
    );
    // Stage register updates with stall guard
    assert!(
        sv.contains("if (!fetch_stall)"),
        "missing fetch stall guard"
    );
    assert!(
        sv.contains("if (!decode_stall)"),
        "missing decode stall guard"
    );
    // Bubble insertion
    assert!(
        sv.contains("fetch_stall ? 1'b0 : fetch_valid_r"),
        "missing bubble insertion"
    );
    // Flush
    assert!(sv.contains("if (branch_taken)"), "missing flush condition");
    assert!(sv.contains("fetch_valid_r <= 1'b0"), "missing fetch flush");
    assert!(
        sv.contains("decode_valid_r <= 1'b0"),
        "missing decode flush"
    );
    // Cross-stage references rewritten
    assert!(
        sv.contains("fetch_instr"),
        "missing rewritten cross-stage ref"
    );
    assert!(
        sv.contains("decode_rs1_val"),
        "missing rewritten decode ref"
    );
    assert!(
        sv.contains("execute_alu_result"),
        "missing rewritten execute ref"
    );
    // Outputs
    assert!(
        sv.contains("assign wb_data = writeback_result"),
        "missing wb output"
    );
    // pc is now passed forward through registered stages instead of
    // being read directly from Fetch (which would be a 3-hop bypass).
    assert!(
        sv.contains("assign pc_out = writeback_pc"),
        "missing pc output"
    );
    // Explicit forwarding mux
    assert!(sv.contains("decode_rs1_fwd"), "missing forwarding mux wire");
    assert!(
        sv.contains("always_comb"),
        "missing always_comb for forwarding mux"
    );
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
    assert!(
        sv.contains("7'(instr)"),
        "expected trunc<7> → 7'(instr), got:\n{sv}"
    );
    // trunc<11,7>() → instr[11:7]
    assert!(
        sv.contains("instr[11:7]"),
        "expected trunc<11,7> → instr[11:7], got:\n{sv}"
    );
    // trunc<14,12>() → instr[14:12]
    assert!(
        sv.contains("instr[14:12]"),
        "expected trunc<14,12> → instr[14:12], got:\n{sv}"
    );
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
    assert!(
        msg.contains("non-blocking assignment") && msg.contains("for"),
        "expected for-loop NBA error, got: {msg}"
    );
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
    arch::typecheck::TypeChecker::new(&symbols, &ast)
        .check()
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
    assert!(
        msg.contains("bypassing the intermediate"),
        "expected bypass message, got: {msg}"
    );
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
    checker
        .check()
        .expect("forward-reference pattern should typecheck");
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
    assert!(
        sv.contains("SimplePipe pipe0"),
        "missing pipeline instantiation"
    );
    assert!(sv.contains(".data_in(din)"), "missing data_in connection");
    assert!(
        sv.contains(".data_out(dout)"),
        "missing data_out connection"
    );
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
    assert!(
        sv.contains("Alu alu0"),
        "missing Alu instantiation inside pipeline stage"
    );
    assert!(sv.contains(".a("), "missing port a connection");
    assert!(sv.contains(".b("), "missing port b connection");
    assert!(
        sv.contains(".result(result_out)"),
        "missing result connection"
    );
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
    assert!(
        sv.contains("Helper h ("),
        "Helper inst should emit inside pipeline stage"
    );
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
fn test_pipeline_wait_until_do_until_runs_in_sim() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("WaitPipe.arch");
    let tb_path = td.path().join("tb_wait_pipe.cpp");
    std::fs::write(
        &arch_path,
        r#"
pipeline WaitPipe
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port go: in Bool;
  port ready: in Bool;
  port out_flag: out Bool;
  port out_done: out Bool;
  port out_cnt: out UInt<3>;

  stage Work
    reg flag: Bool reset rst => false;
    reg done: Bool reset rst => false;
    reg cnt: UInt<3> reset rst => 0;

    seq on clk rising
      wait until go;
      flag <= true;
      do
        cnt <= (cnt + 1).trunc<3>();
      until ready;
      done <= true;
    end seq

    comb
      out_flag = flag;
      out_done = done;
      out_cnt = cnt;
    end comb
  end stage Work
end pipeline WaitPipe
"#,
    )
    .expect("write arch");
    std::fs::write(
        &tb_path,
        r#"
#include "VWaitPipe.h"
#include <cstdio>

static void tick(VWaitPipe& dut) {
  dut.clk = 0;
  dut.eval();
  dut.clk = 1;
  dut.eval();
  dut.clk = 0;
  dut.eval();
}

int main() {
  VWaitPipe dut;
  dut.rst = 1;
  dut.go = 0;
  dut.ready = 0;
  tick(dut);
  if (dut.out_flag || dut.out_done || dut.out_cnt != 0) {
    std::printf("FAIL reset outputs flag=%u done=%u cnt=%u\n",
                (unsigned)dut.out_flag, (unsigned)dut.out_done,
                (unsigned)dut.out_cnt);
    return 1;
  }

  dut.rst = 0;
  tick(dut);        // enter the wait-until state while go is low
  dut.go = 1;
  tick(dut);        // wait-until completes, pre-do assignment fires
  if (!dut.out_flag || dut.out_done) {
    std::printf("FAIL after go flag=%u done=%u cnt=%u\n",
                (unsigned)dut.out_flag, (unsigned)dut.out_done,
                (unsigned)dut.out_cnt);
    return 1;
  }

  dut.go = 0;
  tick(dut);        // do-until body runs while ready is low
  if (dut.out_cnt != 1 || dut.out_done) {
    std::printf("FAIL waiting cnt=%u done=%u\n",
                (unsigned)dut.out_cnt, (unsigned)dut.out_done);
    return 1;
  }

  dut.ready = 1;
  tick(dut);        // do-until exit cycle runs body and trailing done assignment
  if (!dut.out_done || dut.out_cnt != 2) {
    std::printf("FAIL ready cnt=%u done=%u\n",
                (unsigned)dut.out_cnt, (unsigned)dut.out_done);
    return 1;
  }

  std::printf("PASS pipeline wait/do-until sim\n");
  return 0;
}
"#,
    )
    .expect("write tb");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path().join("build"))
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("PASS pipeline wait/do-until sim"),
        "pipeline wait/do-until sim should compile and run\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
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
    assert!(
        sv.contains("$clog2(DEPTH)"),
        "expected $clog2(DEPTH) in SV output, got:\n{sv}"
    );
    // $clog2(DEPTH) + 1 in count port
    assert!(
        sv.contains("$clog2(DEPTH) + 1"),
        "expected $clog2(DEPTH) + 1 in SV output, got:\n{sv}"
    );
    // trunc<$clog2(DEPTH)>() should emit as size cast
    assert!(
        sv.contains("$clog2(DEPTH)'("),
        "expected $clog2(DEPTH)'(...) size cast, got:\n{sv}"
    );
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
    assert!(
        sv.contains("parameter int  DEPTH = 8"),
        "missing DEPTH param"
    );
    assert!(sv.contains("parameter type DATA"), "missing DATA param");
    // Infrastructure signals
    assert!(sv.contains("_fl_mem"), "missing free list memory");
    assert!(sv.contains("_next_mem"), "missing next pointer RAM");
    assert!(sv.contains("_head_r"), "missing head register");
    assert!(
        sv.contains("_tail_r"),
        "missing tail register (track_tail: true)"
    );
    // Status outputs
    assert!(sv.contains("assign empty"), "missing empty assign");
    assert!(sv.contains("assign full"), "missing full assign");
    assert!(sv.contains("assign length"), "missing length assign");
    // Op ports
    assert!(sv.contains("alloc_req_valid"), "missing alloc port");
    assert!(
        sv.contains("delete_head_resp_data"),
        "missing delete_head resp_data port"
    );
    // alloc FSM
    assert!(
        sv.contains("_fl_rdp <= _fl_rdp + 1'b1"),
        "missing free-list dequeue"
    );
    // delete_head 2-cycle FSM
    assert!(
        sv.contains("_ctrl_delete_head_busy"),
        "missing delete_head busy reg"
    );
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
    assert!(
        sv.contains("_ctrl_prev_resp_handle <= _prev_mem"),
        "missing prev pointer follow"
    );
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
    assert!(
        sim.contains("uint8_t _head_r[2]"),
        "missing _head_r[2]:\n{sim}"
    );
    assert!(sim.contains("uint8_t _tail_r[2]"), "missing _tail_r[2]");
    assert!(sim.contains("uint8_t _length_r[2]"), "missing _length_r[2]");
    assert!(
        sim.contains("_ctrl_insert_tail_head_idx"),
        "missing insert_tail head_idx latch"
    );
    assert!(
        sim.contains("_ctrl_delete_head_head_idx"),
        "missing delete_head head_idx latch"
    );
    // Delete ready gated by per-head length
    assert!(
        sim.contains("_length_r[delete_head_req_head_idx] != 0"),
        "missing per-head delete ready gate"
    );
    // Busy-cycle head/tail access uses the latched idx
    assert!(
        sim.contains("_head_r[_ctrl_delete_head_head_idx]"),
        "missing busy-cycle head ref"
    );
    assert!(
        sim.contains("_tail_r[_ctrl_insert_tail_head_idx]"),
        "missing busy-cycle tail ref"
    );
    // Per-head length updates
    assert!(
        sim.contains("_length_r[_ctrl_insert_tail_head_idx]++"),
        "missing length inc in insert"
    );
    assert!(
        sim.contains("_length_r[_ctrl_delete_head_head_idx]--"),
        "missing length dec in delete"
    );
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
    assert!(
        sv.contains("parameter int  NUM_HEADS = 4"),
        "missing NUM_HEADS param:\n{sv}"
    );
    // Head/tail/length become arrays indexed by NUM_HEADS
    assert!(sv.contains("_head_r [NUM_HEADS]"), "missing head array");
    assert!(sv.contains("_tail_r [NUM_HEADS]"), "missing tail array");
    assert!(
        sv.contains("_length_r [NUM_HEADS]"),
        "missing internal length array"
    );
    // Per-op latched head_idx register
    assert!(
        sv.contains("_ctrl_insert_tail_head_idx"),
        "missing insert_tail head_idx latch"
    );
    assert!(
        sv.contains("_ctrl_delete_head_head_idx"),
        "missing delete_head head_idx latch"
    );
    // Accept cycle reads head/tail by request idx directly; busy cycle
    // by the latched idx.
    assert!(
        sv.contains("_head_r[delete_head_req_head_idx]"),
        "missing accept-cycle head ref"
    );
    assert!(
        sv.contains("_tail_r[_ctrl_insert_tail_head_idx]"),
        "missing busy-cycle tail ref"
    );
    // req_ready for delete gated by per-head length
    assert!(
        sv.contains("_length_r[delete_head_req_head_idx] != '0"),
        "missing per-head delete ready gate"
    );
    // Reset loops through NUM_HEADS
    assert!(
        sv.contains("for (_ll_i = 0; _ll_i < NUM_HEADS; _ll_i++)"),
        "missing NUM_HEADS reset loop"
    );
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
    assert!(
        !sv.contains("not yet implemented for multi-head"),
        "stub message should be gone:\n{sv}"
    );
    // insert_head: head_idx latch + busy-cycle uses latched idx + length++
    assert!(
        sv.contains("_ctrl_insert_head_head_idx  <= insert_head_req_head_idx"),
        "missing insert_head idx latch:\n{sv}"
    );
    assert!(
        sv.contains("_head_r[_ctrl_insert_head_head_idx] <= _ctrl_insert_head_resp_handle"),
        "missing insert_head busy-cycle head update"
    );
    assert!(
        sv.contains(
            "_length_r[_ctrl_insert_head_head_idx] <= _length_r[_ctrl_insert_head_head_idx] + 1'b1"
        ),
        "missing insert_head length increment"
    );
    assert!(
        sv.contains("_ctrl_insert_head_was_empty <= (_length_r[insert_head_req_head_idx] == '0)"),
        "missing per-head was_empty check on insert_head"
    );
    // insert_after: head_idx latch + length++ (pointer patches stay shared)
    assert!(
        sv.contains("_ctrl_insert_after_head_idx <= insert_after_req_head_idx"),
        "missing insert_after idx latch"
    );
    assert!(sv.contains("_length_r[_ctrl_insert_after_head_idx] <= _length_r[_ctrl_insert_after_head_idx] + 1'b1"),
            "missing insert_after length increment");
    // delete: head_idx latch + length-- + per-head ready gate
    assert!(
        sv.contains("_ctrl_delete_head_idx <= delete_req_head_idx"),
        "missing delete idx latch"
    );
    assert!(
        sv.contains("_length_r[_ctrl_delete_head_idx] <= _length_r[_ctrl_delete_head_idx] - 1'b1"),
        "missing delete length decrement"
    );
    assert!(
        sv.contains("_length_r[delete_req_head_idx] != '0"),
        "missing per-head delete ready gate"
    );
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
    assert!(
        !sim.contains("is not yet implemented for multi-head"),
        "stub message should be gone:\n{sim}"
    );
    // insert_head: head_idx latch + length++ + per-head head update
    assert!(
        sim.contains("_ctrl_insert_head_head_idx = insert_head_req_head_idx"),
        "missing insert_head idx latch:\n{sim}"
    );
    assert!(
        sim.contains("_head_r[_ctrl_insert_head_head_idx] = _ctrl_insert_head_resp_handle"),
        "missing insert_head busy head update"
    );
    assert!(
        sim.contains("_length_r[_ctrl_insert_head_head_idx]++"),
        "missing insert_head length increment"
    );
    // insert_after: idx latch + length++
    assert!(
        sim.contains("_ctrl_insert_after_head_idx = insert_after_req_head_idx"),
        "missing insert_after idx latch"
    );
    assert!(
        sim.contains("_length_r[_ctrl_insert_after_head_idx]++"),
        "missing insert_after length increment"
    );
    // delete: idx latch + length-- + per-head ready gate
    assert!(
        sim.contains("_ctrl_delete_head_idx = delete_req_head_idx"),
        "missing delete idx latch"
    );
    assert!(
        sim.contains("_length_r[_ctrl_delete_head_idx]--"),
        "missing delete length decrement"
    );
    assert!(
        sim.contains("_length_r[delete_req_head_idx] != 0"),
        "missing per-head delete ready gate"
    );
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
    assert!(
        result.is_err(),
        "expected typecheck to reject per-head op without req_head_idx"
    );
    let msg = result
        .unwrap_err()
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<String>();
    assert!(
        msg.contains("req_head_idx")
            && msg.contains("multi-head") == false
            && msg.contains("NUM_HEADS"),
        "expected NUM_HEADS-specific error, got: {msg}"
    );
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
    assert!(
        result.is_err(),
        "expected typecheck to reject req_head_idx on single-head list"
    );
    let msg = result
        .unwrap_err()
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<String>();
    assert!(
        msg.contains("req_head_idx") && msg.contains("single-head"),
        "expected single-head-specific error, got: {msg}"
    );
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
    assert!(
        result.is_err(),
        "expected type error for prev on singly list"
    );
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("prev") && s.contains("doubly")
        }),
        "expected error about prev requiring doubly, got: {:?}",
        errs
    );
}

#[test]
fn test_linklist_unknown_op_is_type_error() {
    let source = r#"
linklist BadList
  param DEPTH: const = 8;
  param DATA: type = UInt<32>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  track tail: false;
  track length: false;
  op compact
    latency: 1;
    port req_valid: in Bool;
  end op compact
end linklist BadList
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(
        result.is_err(),
        "expected type error for unknown linklist op"
    );
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("linklist `BadList`: unknown op `compact`; known ops:")
                && s.contains("insert_tail")
                && s.contains("read_data")
        }),
        "expected unknown-op diagnostic with known ops list, got: {:?}",
        errs
    );
}

#[test]
fn test_linklist_op_length_is_type_error_not_codegen_panic() {
    // `length` is a status port (`port length: out ...` maintained by
    // `track length:`), never an op. It was mistakenly listed in the
    // typechecker's known-ops allow-list, so `op length` type-checked and then
    // panicked at codegen via the `unreachable!` in emit_ll_op_controller
    // (which has no `length` dispatch arm). It must be rejected at typecheck.
    let source = r#"
linklist LenList
  param DEPTH: const = 16;
  param DATA: type = UInt<32>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  kind singly;
  op length
    latency: 1;
    port req_valid:  in Bool;
    port req_ready:  out Bool;
    port resp_valid: out Bool;
    port resp_data:  out UInt<8>;
  end op length
end linklist LenList
"#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected type error for `op length`");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("linklist `LenList`: unknown op `length`; known ops:")
        }),
        "expected unknown-op diagnostic for `length`, got: {:?}",
        errs
    );
}

#[test]
fn test_linklist_inst_in_module() {
    // PacketQueue wraps TaskQueue linklist as a push/pop FIFO interface.
    // Verifies that: linklist can be instantiated inside a module,
    // inst output ports are auto-declared as wires, and codegen succeeds.
    let source =
        std::fs::read_to_string("examples/pkt_queue.arch").expect("pkt_queue.arch not found");
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
    assert!(
        sv.contains(".read0_addr"),
        "expected .read0_addr port connection, got:\n{sv}"
    );
    assert!(
        sv.contains(".read0_data"),
        "expected .read0_data port connection, got:\n{sv}"
    );
    assert!(
        sv.contains(".read1_addr"),
        "expected .read1_addr port connection, got:\n{sv}"
    );
    assert!(
        sv.contains(".read1_data"),
        "expected .read1_data port connection, got:\n{sv}"
    );
    // Also check dot-only syntax: write.en → write_en
    assert!(
        sv.contains(".write_en"),
        "expected .write_en port connection, got:\n{sv}"
    );
    assert!(
        sv.contains(".write_addr"),
        "expected .write_addr port connection, got:\n{sv}"
    );
    assert!(
        sv.contains(".write_data"),
        "expected .write_data port connection, got:\n{sv}"
    );
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
    let td = sv
        .find("typedef struct packed")
        .expect("expected typedef struct packed in SV");
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
    assert!(
        sv.contains("cnt <="),
        "expected seq `cnt <= ...` in SV, got:\n{sv}"
    );
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
    assert!(
        sv.contains("output logic bus_p_aw_valid"),
        "aw_valid should be output on initiator"
    );
    assert!(
        sv.contains("input logic bus_p_aw_ready"),
        "aw_ready should be input on initiator"
    );
    assert!(
        sv.contains("output logic [31:0] bus_p_aw_addr"),
        "aw payload out"
    );
    // Receive-side b: valid becomes INPUT for the initiator.
    assert!(
        sv.contains("input logic bus_p_b_valid"),
        "b_valid should be input on initiator"
    );
    assert!(
        sv.contains("output logic bus_p_b_ready"),
        "b_ready should be output on initiator"
    );
    assert!(
        sv.contains("input logic [1:0] bus_p_b_resp"),
        "b payload in"
    );
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
    assert!(
        sv.contains("input logic bus_c_aw_valid"),
        "target flip: aw_valid becomes input"
    );
    assert!(
        sv.contains("output logic bus_c_aw_ready"),
        "target flip: aw_ready becomes output"
    );
    assert!(
        sv.contains("input logic [31:0] bus_c_aw_addr"),
        "target flip: payload becomes input"
    );
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
    // Tier 2: a bus with mixed variants should emit per-variant protocol
    // properties where the variant has a temporal safety contract.
    let source = "
        bus BusMix
          handshake a: send kind: valid_ready  end handshake a
          handshake b: send kind: valid_only   end handshake b
          handshake c: send kind: valid_stall  end handshake c
          handshake d: send kind: req_ack_4phase end handshake d
          handshake e: send kind: req_ack_2phase end handshake e
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
            p.e_req   = 1'b0;
          end comb
        end module Top
    ";
    let sv = compile_to_sv(source);
    // Covered variants:
    assert!(sv.contains("_auto_hs_p_a_valid_stable"));
    assert!(sv.contains("_auto_hs_p_c_valid_stable_while_stall"));
    assert!(sv.contains("_auto_hs_p_d_req_holds_until_ack"));
    assert!(sv.contains("_auto_hs_p_e_req_toggles_only_when_idle"));
    assert!(sv.contains("_auto_hs_p_e_ack_toggles_only_when_pending"));
    assert!(sv.contains("(p_e_req != $past(p_e_req)) |-> ($past(p_e_req) == $past(p_e_ack))"));
    assert!(sv.contains("(p_e_ack != $past(p_e_ack)) |-> ($past(p_e_req) != $past(p_e_ack))"));
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
    assert!(
        !sv.contains("import BusS"),
        "spurious SV import emitted for a bus-typed use:\n{sv}"
    );
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
    assert!(
        sv.contains("import PkgA::*;"),
        "expected SV import for a package-typed use:\n{sv}"
    );
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
    assert!(
        sv.contains("import ibex_pkg::rv32m_e;"),
        "expected SV `import ibex_pkg::rv32m_e;` for extern package:\n{sv}"
    );
    assert!(
        sv.contains("import ibex_pkg::rv32b_e;"),
        "expected SV `import ibex_pkg::rv32b_e;` for extern package:\n{sv}"
    );
    assert!(
        !sv.contains("import ibex_pkg::*;"),
        "extern packages must NOT emit wildcard `import ibex_pkg::*;`:\n{sv}"
    );
    assert!(
        sv.contains("parameter rv32m_e RV32M = RV32MFast"),
        "expected bare `rv32m_e` type and `RV32MFast` variant:\n{sv}"
    );
    assert!(
        sv.contains("parameter rv32b_e RV32B = RV32BNone"),
        "expected bare `rv32b_e` type and `RV32BNone` variant:\n{sv}"
    );
    // No extern package body should be emitted (SV package lives upstream).
    assert!(
        !sv.contains("extern package") && !sv.contains("endpackage"),
        "extern package must not emit SV package body:\n{sv}"
    );
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
    models
        .iter()
        .map(|m| format!("{}\n// ---\n{}", m.header, m.impl_))
        .collect::<Vec<_>>()
        .join("\n// ---\n")
}

fn compile_to_thread_sim_h(source: &str) -> String {
    compile_to_thread_sim_result(source).expect("thread sim codegen")
}

fn compile_to_thread_sim_result(source: &str) -> Result<String, String> {
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

    let models = ast
        .items
        .iter()
        .filter_map(|item| match item {
            arch::ast::Item::Module(m)
                if m.body
                    .iter()
                    .any(|i| matches!(i, arch::ast::ModuleBodyItem::Thread(_))) =>
            {
                Some(arch::sim_codegen::thread_sim::gen_module_thread(
                    m, false, false, 1,
                ))
            }
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(models
        .iter()
        .map(|m| format!("{}\n// ---\n{}", m.header, m.impl_))
        .collect::<Vec<_>>()
        .join("\n// ---\n"))
}

/// Mirror of `compile_to_thread_sim_h` that also collects the warnings
/// the thread-sim emitter pushes. Returns just the warnings — the header
/// text is not useful for warning-shape assertions.
fn compile_to_thread_sim_collect_warnings(source: &str) -> Vec<arch::diagnostics::CompileWarning> {
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

    let mut warnings: Vec<arch::diagnostics::CompileWarning> = Vec::new();
    for item in &ast.items {
        if let arch::ast::Item::Module(m) = item {
            if m.body
                .iter()
                .any(|i| matches!(i, arch::ast::ModuleBodyItem::Thread(_)))
            {
                arch::sim_codegen::thread_sim::gen_module_thread_with_warnings(
                    m,
                    false,
                    false,
                    1,
                    &mut warnings,
                )
                .expect("thread sim codegen");
            }
        }
    }
    warnings
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
    assert!(
        h.contains("bool _b_data_vinit = false;"),
        "expected shadow bit for flattened bus input 'b_data':\n{h}"
    );
    assert!(
        h.contains("bool _b_valid_vinit = false;"),
        "expected shadow bit for flattened bus input 'b_valid':\n{h}"
    );
    assert!(
        h.contains("void set_b_data("),
        "expected setter for flattened bus input 'b_data':\n{h}"
    );
    assert!(
        h.contains("void set_b_valid("),
        "expected setter for flattened bus input 'b_valid':\n{h}"
    );
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
    assert!(
        !h.contains("_b_data_vinit"),
        "did not expect vinit for output-side bus signal 'b_data':\n{h}"
    );
    assert!(
        !h.contains("set_b_data("),
        "did not expect setter for output-side bus signal 'b_data':\n{h}"
    );
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
    assert!(
        !h.contains("_b_data_vinit"),
        "no --inputs-start-uninit → no bus vinit tracking:\n{h}"
    );
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
    let cpp = models
        .iter()
        .find(|m| m.class_name == "VConsumer")
        .unwrap()
        .impl_
        .clone();

    // Payload-signal read warning must be AND'd with the handshake's valid.
    assert!(
        cpp.contains("!_b_ch_data_vinit && b_ch_valid"),
        "expected payload warning gated on valid signal:\n{cpp}"
    );

    // Valid-signal itself is tracked but NOT a payload, so its warning is
    // unconditional (no extra gate).
    let valid_check_line = cpp
        .lines()
        .find(|l| l.contains("!_b_ch_valid_vinit"))
        .expect("expected warning for b_ch_valid signal");
    assert!(
        !valid_check_line.contains("&& b_ch_valid"),
        "valid signal's own warning should not self-gate:\n{valid_check_line}"
    );
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
    let cpp = models
        .iter()
        .find(|m| m.class_name == "VC")
        .unwrap()
        .impl_
        .clone();
    assert!(
        cpp.contains("!_b_ch_payload_vinit && b_ch_req"),
        "req_ack_4phase payload should gate on b_ch_req:\n{cpp}"
    );
}

#[test]
fn test_handshake_tier15_req_ack_2phase_pending_guards_payload_lint() {
    let source = "
        bus BusRA2
          handshake ch: send kind: req_ack_2phase
            payload: UInt<16>;
          end handshake ch
        end bus BusRA2

        use BusRA2;

        module C
          port b: target BusRA2;
          port o: out UInt<16>;
          comb
            if b.ch_req != b.ch_ack
              o = b.ch_payload;
            else
              o = 16'h0;
            end if
            b.ch_ack = b.ch_req;
          end comb
        end module C
    ";
    let ws = warnings_from(source);
    assert!(
        !ws.iter().any(|m| m.contains("handshake payload")),
        "if b.ch_req != b.ch_ack should guard req_ack_2phase payload; got: {:?}",
        ws
    );
}

#[test]
fn test_handshake_tier15_req_ack_2phase_uninit_uses_pending_guard() {
    use arch::sim_codegen::SimCodegen;
    let source = "
        bus BusRA2
          handshake ch: send kind: req_ack_2phase
            payload: UInt<16>;
          end handshake ch
        end bus BusRA2

        use BusRA2;

        module C
          port b: target BusRA2;
          port o: out UInt<16>;
          comb
            if b.ch_req != b.ch_ack
              o = b.ch_payload;
            else
              o = 16'h0;
            end if
            b.ch_ack = b.ch_req;
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
    let cpp = models
        .iter()
        .find(|m| m.class_name == "VC")
        .unwrap()
        .impl_
        .clone();
    assert!(
        cpp.contains("!_b_ch_payload_vinit && (b_ch_req != b_ch_ack)"),
        "req_ack_2phase payload should gate on pending transfer:\n{cpp}"
    );
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
    assert!(
        ws.iter()
            .any(|m| m.contains("b.ch_data") && m.contains("if b.ch_valid")),
        "expected unguarded-payload warning; got: {:?}",
        ws
    );
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
    assert!(
        !ws.iter()
            .any(|m| m.contains("handshake payload") && m.contains("ch_data")),
        "did not expect handshake warning; got: {:?}",
        ws
    );
}

#[test]
fn test_check_port_reg_timing_fires_on_legacy_port_reg() {
    // Positive control: when a user *writes* the deprecated `port reg`
    // form and assigns it inside a state-dependent if/elsif branch in a
    // seq block, the timing-mismatch warning must fire. The implicit
    // 1-cycle latency the legacy form hides is exactly the foot-gun the
    // warning was designed to catch — testbench models that expect a
    // same-cycle output from the state register see one-cycle-old data.
    let source = "
        module Foo
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port reg out_reg: out UInt<8> reset rst => 0;
          reg state: UInt<2> reset rst => 0;
          default seq on clk rising;
          seq
            if state == 0
              out_reg <= 8'd1;
            end if
          end seq
        end module Foo
    ";
    let ws = warnings_from(source);
    assert!(
        ws.iter().any(|m| m.contains("out_reg")
            && m.contains("deprecated `port reg`")
            && m.contains("state-dependent")),
        "expected check_port_reg_timing warning for legacy `port reg` \
         output; got: {:?}",
        ws,
    );
}

#[test]
fn test_check_port_reg_timing_skips_modern_pipe_reg() {
    // Negative control: when a user writes the modern `port: out pipe_reg<T, N>`
    // form, they have *explicitly* opted into the N-cycle latency by
    // declaring it in the port signature. The foot-gun the warning is
    // designed to catch (latency hidden behind the `port reg` keyword)
    // doesn't apply — the latency is visible in the port type. The
    // warning must NOT fire, even when the port is assigned inside a
    // state-dependent branch.
    let source = "
        module Foo
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port out_reg: out pipe_reg<UInt<8>, 1> reset rst => 0;
          reg state: UInt<2> reset rst => 0;
          default seq on clk rising;
          seq
            if state == 0
              out_reg@1 <= 8'd1;
            end if
          end seq
        end module Foo
    ";
    let ws = warnings_from(source);
    assert!(
        !ws.iter().any(|m| m.contains("out_reg")
            && (m.contains("state-dependent") || m.contains("deprecated `port reg`"))),
        "did not expect check_port_reg_timing warning for modern \
         `pipe_reg<T, N>` output (user opted into the N-cycle latency \
         explicitly); got: {:?}",
        ws,
    );
}

#[test]
fn test_check_port_reg_timing_skips_synthesized_thread_lowered_regs() {
    // Negative control: when thread lowering manufactures port regs on
    // the synthesized `_threads` submodule, the user did NOT write those
    // port regs — the warning would point at a declaration that doesn't
    // exist in user source. The synthesized port regs use
    // `legacy_port_reg: false` (see `src/elaborate.rs::lower_threads`),
    // which is the same gate the modern `pipe_reg<T, N>` form satisfies,
    // so the timing check is silent on them automatically — no separate
    // `synthesized` flag needed. Repro pattern matches
    // `Nic400WidthAdapter.arch`: a thread with `wait until` / `wait`
    // (creating FSM states) that does seq assignments inside
    // state-dependent branches.
    let source = "
        module Foo
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port start: in Bool;
          port reg out_data: out UInt<8> reset rst => 0;
          thread on clk rising, rst high
            wait until start;
            out_data <= 8'd1;
            wait 1 cycle;
            out_data <= 8'd2;
          end thread
        end module Foo
    ";
    // Drive the full lowering pipeline (threads → port regs synthesized
    // on the merged threads submodule) before typecheck, matching the
    // `arch check` driver.
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target lowering");
    let ast = arch::elaborate::lower_tlm_initiator_calls(ast).expect("tlm initiator lowering");
    let ast = arch::elaborate::lower_threads(ast).expect("thread lowering");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("pipe_reg lowering");
    let ast = arch::elaborate::lower_credit_channel_dispatch(ast).expect("credit_channel lowering");
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("typecheck");
    let msgs: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    assert!(
        !msgs
            .iter()
            .any(|m| m.contains("state-dependent") || m.contains("deprecated `port reg`")),
        "expected NO check_port_reg_timing warning on synthesized \
         thread-lowered port regs; got: {:?}",
        msgs,
    );
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
    assert!(
        !ws.iter()
            .any(|m| m.contains("handshake payload") && m.contains("ch_data")),
        "AND-conjunct guard should silence the lint; got: {:?}",
        ws
    );
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
    assert!(
        ws.iter().any(|m| m.contains("b.ch_data")),
        "read in else-branch of `if valid` is NOT guarded; should warn. got: {:?}",
        ws
    );
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
    assert!(
        !ws.iter().any(|m| m.contains("handshake payload")),
        "if b.ch_req should guard req_ack payload; got: {:?}",
        ws
    );
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
    assert!(
        sv.contains("vec[0] == needle || vec[1] == needle"),
        "expected any to expand to OR of 4 compares: {sv}"
    );
    assert!(
        sv.contains("vec[0] != 0 && vec[1] != 0"),
        "expected all to expand to AND of 4 compares: {sv}"
    );
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
    assert!(
        sv.contains("2'd0") && sv.contains("2'd3"),
        "expected index binder to emit sized literals: {sv}"
    );
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
    assert!(
        sv.contains("3'(vec[0] == x ? 1 : 0)"),
        "expected count to emit width-3 bool-to-bit casts: {sv}"
    );
    // contains lowers identically to any(item == x).
    assert!(
        sv.contains("(vec[0] == x) || (vec[1] == x)"),
        "expected contains to OR per-element equality: {sv}"
    );
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
    assert!(
        sv.contains("flags[0] | flags[1] | flags[2] | flags[3]"),
        "reduce_or expected: {sv}"
    );
    assert!(
        sv.contains("flags[0] & flags[1] & flags[2] & flags[3]"),
        "reduce_and expected: {sv}"
    );
    assert!(
        sv.contains("flags[0] ^ flags[1] ^ flags[2] ^ flags[3]"),
        "reduce_xor expected: {sv}"
    );
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
    assert!(
        sv.contains("assign x = p_in.x;"),
        "expected field assign for x:\n{sv}"
    );
    assert!(
        sv.contains("assign y = p_in.y;"),
        "expected field assign for y:\n{sv}"
    );
    // Per-field width comes from the struct definition.
    assert!(
        sv.contains("logic [7:0] x;") && sv.contains("logic [7:0] y;"),
        "expected 8-bit wire declarations:\n{sv}"
    );
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
    assert!(
        sv.contains("assign a = t.a;"),
        "expected partial destructure:\n{sv}"
    );
    assert!(
        !sv.contains("assign b = t.b;") && !sv.contains("assign c = t.c;"),
        "did not expect unbound fields:\n{sv}"
    );
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
    assert!(
        result.is_err(),
        "expected type-check error for destructure on non-struct"
    );
    let msg = format!("{:?}", result.unwrap_err());
    assert!(
        msg.contains("requires a struct-typed RHS"),
        "expected specific error message, got: {msg}"
    );
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
    assert!(
        result.is_err(),
        "expected type-check error for unknown field"
    );
    let msg = format!("{:?}", result.unwrap_err());
    assert!(
        msg.contains("has no field named `z`"),
        "expected unknown-field message, got: {msg}"
    );
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
    assert!(
        h.contains("struct FooBus {"),
        "expected `struct FooBus {{` in generated structs header:\n{h}"
    );
    assert!(
        h.contains("uint8_t cmd;") && h.contains("uint8_t resp;"),
        "expected bus fields as struct members:\n{h}"
    );

    // VParent.h should declare the wire as a struct-typed member and must
    // NOT emit a shadow `uint32_t w;` scalar.
    let parent = h
        .split("// ---\n")
        .find(|p| p.contains("class VParent"))
        .expect("no VParent header section");
    assert!(
        parent.contains("FooBus _let_w;"),
        "expected `FooBus _let_w;` in VParent header:\n{parent}"
    );
    assert!(
        !parent.contains("uint32_t w;"),
        "unexpected shadow `uint32_t w;` in VParent header:\n{parent}"
    );
}

#[test]
fn test_vec_of_bus_port_flattens_to_n_indexed_copies() {
    // `port chans: initiator Vec<BusName, N>;` emits one unpacked-array
    // SV port per bus signal (D2 shape):
    //     output logic       chans_v [N]
    //     input  logic       chans_r [N]
    //     output logic [7:0] chans_d [N]
    // Inst-site bracket-dot indexing `chans[i].sig` resolves to the
    // SV array-indexed name `chans_sig[i]`.
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
    // One port per signal, with `[N-1:0]` *packed* outer dim. Packed shape
    // works with both Verilator and Yosys's built-in read_verilog and lets
    // inst connection sites use SV concat literals for column gather.
    assert!(
        sv.contains("output logic [2:0] chans_v"),
        "missing `output logic [2:0] chans_v` (packed 3-element) in SV:\n{sv}"
    );
    assert!(
        sv.contains("input logic [2:0] chans_r"),
        "missing `input logic [2:0] chans_r` in SV:\n{sv}"
    );
    assert!(
        sv.contains("output logic [2:0] [7:0] chans_d"),
        "missing `output logic [2:0] [7:0] chans_d` in SV:\n{sv}"
    );
    // Bracket-dot access lowers to packed indexed SV refs.
    assert!(
        sv.contains("chans_v[0] = 1'b1") || sv.contains("assign chans_v[0] = 1'b1"),
        "expected `chans_v[0] = 1'b1` assignment in SV:\n{sv}"
    );
    assert!(
        sv.contains("chans_v[1] = 1'b0") || sv.contains("assign chans_v[1] = 1'b0"),
        "expected `chans_v[1] = 1'b0` assignment in SV:\n{sv}"
    );
    assert!(
        sv.contains("chans_d[2] = 8'd51") || sv.contains("assign chans_d[2] = 8'd51"),
        "expected `chans_d[2] = 8'd51` assignment in SV:\n{sv}"
    );
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
    let err = parser
        .parse_source_file()
        .expect_err("Vec<B, 0> should fail to parse");
    assert!(
        format!("{err:?}").contains("N must be >= 1"),
        "expected `N must be >= 1` diagnostic, got: {err:?}"
    );
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
    // Packed Vec-of-bus port: single decl with `[N-1:0]` packed outer dim
    // (param folded to 3).
    assert!(
        sv.contains("output logic [2:0] chans_v"),
        "missing `output logic [2:0] chans_v` (packed) in SV:\n{sv}"
    );
    assert!(
        sv.contains("output logic [2:0] [7:0] chans_d"),
        "missing `output logic [2:0] [7:0] chans_d` in SV:\n{sv}"
    );
    // for-loop should be statically unrolled even though the upper bound
    // is `NUM_CHANS-1` (param expression).
    assert!(
        !sv.contains("for (int i ="),
        "expected param-driven for-loop bounds to fold + unroll:\n{sv}"
    );
    assert!(
        sv.contains("chans_d[2] = 8'(idx + 2)") || sv.contains("chans_d[2] = 8'((idx + 2))"),
        "missing unrolled last-element assignment:\n{sv}"
    );
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
    assert!(
        msg.contains("chans_1_v") && (msg.contains("not driven") || msg.contains("UndriveOutput")),
        "expected `chans_1_v not driven` diagnostic, got: {msg}"
    );
}

#[test]
fn test_fast_gate_do_until_mealy_fusion() {
    // `if not X; wait until X; end if` immediately followed by
    // `do BODY until Y;` fuses into a single Mealy-style state. The SV
    // codegen output for this design should:
    //   * NOT emit a separate entry-wait state.
    //   * Gate the body's comb drives by `X` (`if (X) { ... }`).
    //   * Transition with `X && Y` as the guard.
    // Result: same-cycle handshake when both conditions hold at posedge,
    // eliminating the entry-wait bubble that standard `wait until`
    // imposes.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
          r: in  Bool;
        end bus B
        module Drv
          port clk:   in  Clock<SysDomain>;
          port rst:   in  Reset<Async, Low>;
          port m_val: in  Bool;
          port m_dat: in  UInt<8>;
          port m_rdy: out Bool;
          port b:     initiator B;
          thread T on clk rising, rst low
            default comb
              b.v = false;
              b.d = 0;
              m_rdy = false;
            end default
            if not (m_val)
              wait until m_val;
            end if
            do
              b.v   = true;
              b.d   = m_dat;
              m_rdy = b.r;
            until b.r;
          end thread T
        end module Drv
    ";
    let sv = compile_to_sv(source);
    // Exactly one wait_until state (S0) — no separate entry state.
    let state_decls = sv.matches("_t0_S").count();
    assert!(
        state_decls <= 4,
        "expected at most 2 `_t0_Sx` references (state localparam + one use); \
             single-state Mealy fusion should not generate extra states. Got {state_decls}:\n{sv}"
    );
    // The do-body comb is wrapped in `if (m_val)`.
    assert!(
        sv.contains("if (m_val)"),
        "expected `if (m_val)` gating the Mealy body's comb drives:\n{sv}"
    );
    // Transition guard ANDs the wait + do-until conditions.
    assert!(
        sv.contains("if (m_val && b_r)") || sv.contains("m_val && b_r"),
        "expected `m_val && b_r` as the fused transition guard:\n{sv}"
    );
}

#[test]
fn test_fast_gate_do_until_paren_and_bare_not_lower_the_same() {
    let paren_source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
          r: in  Bool;
        end bus B
        module Drv
          port clk:   in  Clock<SysDomain>;
          port rst:   in  Reset<Async, Low>;
          port m_val: in  Bool;
          port m_dat: in  UInt<8>;
          port m_rdy: out Bool;
          reg started: Bool reset rst => false;
          port b:     initiator B;
          thread T on clk rising, rst low
            default comb
              b.v = false;
              b.d = 0;
              m_rdy = false;
            end default
            if not (m_val)
              wait until m_val;
            end if
            do
              b.v   = true;
              b.d   = m_dat;
              m_rdy = b.r;
              started <= true;
            until b.r;
          end thread T
        end module Drv
    ";
    let bare_source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
          r: in  Bool;
        end bus B
        module Drv
          port clk:   in  Clock<SysDomain>;
          port rst:   in  Reset<Async, Low>;
          port m_val: in  Bool;
          port m_dat: in  UInt<8>;
          port m_rdy: out Bool;
          reg started: Bool reset rst => false;
          port b:     initiator B;
          thread T on clk rising, rst low
            default comb
              b.v = false;
              b.d = 0;
              m_rdy = false;
            end default
            if not m_val
              wait until m_val;
            end if
            do
              b.v   = true;
              b.d   = m_dat;
              m_rdy = b.r;
              started <= true;
            until b.r;
          end thread T
        end module Drv
    ";

    let paren_sv = compile_to_sv(paren_source);
    let bare_sv = compile_to_sv(bare_source);
    assert_eq!(
        paren_sv, bare_sv,
        "`if not (X); wait until X; end if` should lower like \
         `if not X; wait until X; end if` before a `do ... until` body"
    );
}

#[test]
fn test_fast_gate_lock_do_until_paren_and_bare_not_lower_the_same() {
    let paren_source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port req: in Bool;
          port ack: in Bool;
          port out_v: out Bool shared(or);
          reg captured: Bool reset rst => false;

          resource bus_lk: mutex<priority>;

          thread T on clk rising, rst low
            default comb
              out_v = false;
            end default
            if not (req)
              wait until req;
            end if
            lock bus_lk
              do
                out_v = true;
                captured <= true;
              until ack;
            end lock bus_lk
          end thread T
        end module M
    ";
    let bare_source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port req: in Bool;
          port ack: in Bool;
          port out_v: out Bool shared(or);
          reg captured: Bool reset rst => false;

          resource bus_lk: mutex<priority>;

          thread T on clk rising, rst low
            default comb
              out_v = false;
            end default
            if not req
              wait until req;
            end if
            lock bus_lk
              do
                out_v = true;
                captured <= true;
              until ack;
            end lock bus_lk
          end thread T
        end module M
    ";

    let paren_sv = compile_to_sv(paren_source);
    let bare_sv = compile_to_sv(bare_source);
    assert_eq!(
        paren_sv, bare_sv,
        "`if not (X); wait until X; end if` should lower like \
         `if not X; wait until X; end if` before a `lock R do ... until` body"
    );
}

#[test]
fn test_nested_for_in_thread_uses_distinct_loop_counters() {
    // Regression for issue #414: nested `for` loops in a thread used to
    // share a single `_loop_cnt` register, so the inner loop's increment
    // clobbered the outer loop's running index and the outer exited
    // after the very first inner iteration completed. The fix allocates
    // a distinct `_loop_cnt_{id}` per `for` instance and threads
    // outer-loop transitions through the inner-loop exit condition so
    // the outer counter only ticks once per completed inner iteration.
    let source = "
        module NestedFor
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go: in Bool;
          port outer_visits: out UInt<8>;
          port inner_visits: out UInt<8>;

          reg outer_r: UInt<8> reset rst => 0;
          reg inner_r: UInt<8> reset rst => 0;

          thread T on clk rising, rst low
            default comb
              outer_visits = outer_r;
              inner_visits = inner_r;
            end default
            wait until go;
            for c in 0..2
              for b in 0..3
                wait 1 cycle;
                inner_r <= (inner_r + 1).trunc<8>();
              end for
              outer_r <= (outer_r + 1).trunc<8>();
            end for
          end thread T
        end module NestedFor
    ";
    let sv = compile_to_sv(source);
    // Two distinct loop-counter regs must be declared.
    assert!(
        sv.contains("_t0_loop_cnt_0"),
        "expected outer loop counter `_t0_loop_cnt_0`:\n{sv}"
    );
    assert!(
        sv.contains("_t0_loop_cnt_1"),
        "expected inner loop counter `_t0_loop_cnt_1`:\n{sv}"
    );
    // No leftover shared `_t0_loop_cnt` (without an `_{id}` suffix) — a
    // grep for `_t0_loop_cnt;` or `_t0_loop_cnt ` (followed by anything
    // other than `_`) would catch the old shared-counter shape.
    let bad_shared = sv.contains("_t0_loop_cnt;")
        || sv.contains("_t0_loop_cnt <=")
        || sv.contains("_t0_loop_cnt +")
        || sv.contains("_t0_loop_cnt =")
        || sv.contains("_t0_loop_cnt >=")
        || sv.contains("_t0_loop_cnt <");
    assert!(
        !bad_shared,
        "found references to shared `_t0_loop_cnt` (issue #414 regression):\n{sv}"
    );
}

#[test]
fn test_vec_bus_param_forward_no_stack_overflow() {
    // Regression: a `target Vec<Bus, N>` forwarded across an inst boundary
    // where the count N is itself an inst-forwarded param (`param NUM = NUM`).
    //
    //  * Whole-vector (`mm <- m`): `arch check` passed but `arch build`
    //    stack-overflowed (abort 134). `emit_inst` copied the inst override
    //    RHS verbatim into the child param list, so `param NUM = NUM` became a
    //    self-referential child default `NUM => NUM`, and `eval_const_u32`
    //    (resolving the Vec<Bus> count) recursed forever. Fixed by folding the
    //    override in the parent scope before substitution, plus a depth guard.
    //
    //  * Per-element (`mm[k] <- m[k]`): never crashed but failed single-driver
    //    checking ("output port m_0_ready is not driven") — the parser-
    //    flattened LHS `mm_<k>` didn't match the child's Vec-of-bus port `mm`
    //    in the inst driver-tracking, so the reverse (ready) direction went
    //    uncredited. Fixed by stripping the `_<idx>` suffix to the base.
    //
    // Both shapes must now build to Verilator-clean SV (lint verified in CI
    // via the standing repro; here we assert the structural SV shape).
    let source = include_str!("regression/issues/vec_bus_param_forward/VecBusParamForward.arch");
    let sv = compile_to_sv(source);
    // All three modules emit.
    for m in [
        "module CrashSink",
        "module CrashWhole",
        "module CrashPerElem",
    ] {
        assert!(sv.contains(m), "expected `{m}` in SV:\n{sv}");
    }
    // Whole-vector forward packs each bus signal whole.
    assert!(
        sv.contains(".mm_ready(m_ready)") && sv.contains(".mm_valid(m_valid)"),
        "expected whole-Vec packed forwarding in CrashWhole:\n{sv}"
    );
    // Per-element forward packs element-wise via a concat over the Vec count.
    assert!(
        sv.contains(".mm_ready({m_ready[1], m_ready[0]})")
            && sv.contains(".mm_valid({m_valid[1], m_valid[0]})"),
        "expected per-element packed forwarding in CrashPerElem:\n{sv}"
    );
}

#[test]
fn test_vec_bus_whole_vec_forward_to_child_inst() {
    // Regression for issue #424: forwarding a `target Vec<Bus, N>`
    // parent port to a child instance via a whole-Vec connection
    // (`m <- m_top`) failed with `output port m_top_0_<sig> is not
    // driven`, even though the child's body drives every `m[i].<sig>`.
    //
    // The undriven-port check expands a parent `Vec<Bus,N>` port into N
    // per-element prefixes `m_top_0_<sig>`, …, `m_top_{N-1}_<sig>` —
    // but the inst driver-tracking only credited the bare `m_top_<sig>`
    // for the `Ident("m_top")` parent expression. The fix detects
    // whole-Vec forwarding (child port and parent port both `Vec<Bus,N>`
    // with matching N) and seeds the per-element prefixes.
    //
    // SV codegen sibling fix: declared_names now includes the packed
    // `<port>_<sig>` form for Vec-of-bus ports so the inst auto-wire-decl
    // pass doesn't emit redundant `logic <port>_<sig>;` lines that would
    // shadow the actual SV port declarations.
    let source = include_str!("regression/issues/vec_bus_forward/VecBusForward.arch");
    let sv = compile_to_sv(source);
    // Both modules must be emitted.
    assert!(
        sv.contains("module Inner"),
        "expected Inner module in SV:\n{sv}"
    );
    assert!(
        sv.contains("module Wrapper"),
        "expected Wrapper module in SV:\n{sv}"
    );
    // The Vec-of-bus port should emit as packed signals on Wrapper.
    assert!(
        sv.contains("m_top_v_valid"),
        "expected m_top_v_valid port:\n{sv}"
    );
    assert!(
        sv.contains("m_top_v_ready"),
        "expected m_top_v_ready port:\n{sv}"
    );
    assert!(
        sv.contains("m_top_v_data"),
        "expected m_top_v_data port:\n{sv}"
    );
    // Inner inst should forward each bus signal whole.
    assert!(
        sv.contains("Inner inner"),
        "expected inst `Inner inner` in SV:\n{sv}"
    );
    assert!(
        sv.contains(".m_v_ready(m_top_v_ready)"),
        "expected whole-Vec packed forwarding `.m_v_ready(m_top_v_ready)`:\n{sv}"
    );
    assert!(
        sv.contains(".m_v_valid(m_top_v_valid)"),
        "expected whole-Vec packed forwarding `.m_v_valid(m_top_v_valid)`:\n{sv}"
    );
    // The pre-fix bug emitted shadowing wires like `logic m_top_v_ready;` —
    // assert none of those slipped in.
    assert!(
        !sv.contains("logic m_top_v_ready;"),
        "redundant `logic m_top_v_ready;` declaration shadows the SV port (issue #424):\n{sv}"
    );
    assert!(
        !sv.contains("logic m_top_v_valid;"),
        "redundant `logic m_top_v_valid;` declaration shadows the SV port (issue #424):\n{sv}"
    );
}

#[test]
fn test_type_alias_bus_params_propagate_into_generate_if() {
    // Regression for issue #423: a module-scope `type` alias that binds a
    // bus's params used to lose those bindings when the alias was
    // referenced from a `wire` inside a `generate_if` body. The bus then
    // expanded with its declared param defaults instead of the alias's
    // bound values — silently producing wrong-width signals in the
    // generated SV.
    //
    // The fix propagates the alias's stored `bus_params` onto each
    // `GenItem::Wire` the same way the top-level `WireDecl` substitution
    // path already did. After the fix, both the module-scope wire and the
    // generate_if wire must emit with the alias's `ID_W = 5`.
    let source = include_str!("regression/issues/alias_in_generate_if/AliasInGenif.arch");
    let sv = compile_to_sv(source);
    // Both wires must be 5 bits wide. Before the fix, `bad_bus_ar_id` was
    // 1 bit (BusAxi4's default ID_W).
    assert!(
        sv.contains("logic [4:0] ok_bus_ar_id"),
        "module-scope alias use should expand with ID_W=5 (5-bit ar_id):\n{sv}",
    );
    assert!(
        sv.contains("logic [4:0] bad_bus_ar_id"),
        "alias use inside generate_if should also expand with ID_W=5 \
         (issue #423 regression — used to fall back to bus default ID_W=1):\n{sv}",
    );
    // Defensive: the buggy shape must not reappear.
    assert!(
        !sv.contains("logic [0:0] bad_bus_ar_id") && !sv.contains("logic bad_bus_ar_id"),
        "found buggy 1-bit / scalar `bad_bus_ar_id` (issue #423 regression):\n{sv}",
    );
}

#[test]
fn test_outer_for_advances_when_inner_if_else_lock_terminates() {
    // Regression for issue #422: when an inner `for` loop's body ends in an
    // `if`/`else` where each branch contains its own `lock do ... until ...`
    // (with DIFFERENT bodies so the codegen doesn't fuse them into one
    // terminal state), the codegen used to attach the outer-loop
    // continuation cascade only to the else-branch's terminal state. The
    // if-branch's terminal state still carried a bare `(true → past-the-end)`
    // transition, which after outer-frame translation jumped unconditionally
    // back to the thread entry state — resetting both inner and outer loop
    // counters instead of advancing the outer iteration.
    //
    // The fix replicates the loop-continuation cascade (counter increment,
    // loop-back, exit) onto every state that has an "off-the-end" transition
    // (i.e. target == result.len()), not just `result.last_mut()`. The
    // trailing-seq-merge that attaches the outer-block's trailing assigns
    // (e.g. `outer_r <= +1`) to the exit arm is also extended to fire on
    // every sibling terminal arm.
    //
    // The repro is committed at
    // `tests/regression/issues/nested_if_lock_outer_for_continuation/`. The
    // companion C++ TB exercises the full simulator end-to-end and is
    // invoked via `arch sim` (out of scope for the SV-level integration
    // test); here we assert on the generated thread SV directly.
    let source = include_str!(
        "regression/issues/nested_if_lock_outer_for_continuation/IfLockOuterForRepro.arch"
    );
    let sv = compile_to_sv(source);

    // The inner-for's terminal arms live in two separate states (one per
    // branch of the if/else). Both must advance the outer loop:
    //
    //   - increment `_t0_loop_cnt_0` (outer counter) when the inner-for
    //     completes its last sub-beat,
    //   - bump `outer_r` (trailing-seq from the outer-for body),
    //   - transition either back to the inner-for dispatch (more outer
    //     iters to do) or to the thread-entry state (outer-for done).
    //
    // Before the fix, only ONE of the two terminal arm states carried this
    // cascade — the other fell through to the thread-entry state, so the
    // outer counter never ticked when the if-branch took the last sub-beat.
    //
    // We count occurrences of the outer-counter increment expression in
    // the per-state SV blocks; the bug shape has exactly one, the fix has
    // two.
    let outer_cnt_inc_hits = sv.matches("_t0_loop_cnt_0 + 2'd1").count();
    assert!(
        outer_cnt_inc_hits >= 2,
        "expected the outer-counter increment `_t0_loop_cnt_0 + 2'd1` to \
         appear in BOTH terminal arms of the if/else (issue #422 regression — \
         only one arm carried the cascade in the buggy shape); got \
         {outer_cnt_inc_hits} occurrence(s):\n{sv}",
    );

    // The outer-block trailing assign `outer_r <= +1` must also fire on
    // both terminal arms.
    let outer_r_inc_hits = sv.matches("outer_r + 1").count();
    assert!(
        outer_r_inc_hits >= 2,
        "expected the outer-block trailing assign `outer_r + 1` to \
         appear in BOTH terminal arms of the if/else (issue #422 regression); \
         got {outer_r_inc_hits} occurrence(s):\n{sv}",
    );

    // Defensive: the if-branch's terminal state must NOT have a bare
    // unconditional jump back to the thread-entry state `_t0_S0_wait_until`
    // — that's the buggy shape (the arm bypassed the loop-continuation
    // cascade entirely). The fixed shape only jumps to S0_wait_until under
    // the outer-exit guard `cnt_0 >= 2`.
    //
    // Use a simple shape check: every transition to `_t0_S0_wait_until`
    // (other than from S0 itself) must be guarded by a `>=` test on
    // `_t0_loop_cnt_0`.
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    // After whitespace-collapse, every `_t0_state <= _t0_S0_wait_until`
    // should be preceded somewhere upstream by the outer-exit guard.
    // Count bare unguarded jumps: pattern `1'b1 begin _t0_state <= _t0_S0_wait_until`.
    assert!(
        !trimmed.contains("&& 1'b1) begin _t0_state <= _t0_S0_wait_until"),
        "found an UNGUARDED jump back to _t0_S0_wait_until from an inner \
         terminal arm (issue #422 regression — the if-branch lock-exit \
         state used to jump to thread-entry, resetting both loop counters \
         instead of advancing the outer loop):\n{sv}",
    );
}

#[test]
fn test_concat_bus_field_width_uses_module_param_binding() {
    // Regression for issue #427: sim codegen's Concat pack expression
    // used wrong shift offsets when operands were bus-port FieldAccess
    // and one of the bus's per-signal widths was bound by a
    // module-param-substituted bus-alias param.
    //
    // The minimal repro declares `bus MiniAxi` with `param ID_W = 1` and
    // `ar_id: out UInt<ID_W>`, then a module that does
    // `port up: target MiniAxi<ID_W=ID_W>` with `param ID_W: const = 3`.
    // Inside the module a concat `{up.ar_addr, up.ar_id}` packs the two
    // fields. Before the fix the bus-flat width entry for `up_ar_id` was
    // built by `type_bits_te` with no module-param context, so the
    // substituted `UInt<ID_W>` width Ident fell through the param-aware
    // fold to the legacy conservative `eval_width = 32` — and the concat
    // shifted `up_ar_addr` by 32 bits instead of 3.
    //
    // After the fix, the bus-flat width fold uses
    // `type_bits_te_with_params(&m.params)`, so `up_ar_id`'s width
    // resolves correctly to 3 and the concat shift is `<< 3`.
    let source = include_str!("regression/issues/concat_bus_field_width/ConcatBusFieldWidth.arch");
    let cpp = compile_to_sim_h(source, false);
    // The pack expression must shift `up_ar_addr` by 3 (the width of
    // `up_ar_id`), not 32.
    assert!(
        cpp.contains("((uint64_t)(up_ar_addr) << 3)"),
        "expected pack shift `(up_ar_addr) << 3` (width of up.ar_id = 3) \
         in sim cpp (issue #427 regression):\n{cpp}"
    );
    // The buggy 32-bit shift must be gone.
    assert!(
        !cpp.contains("((uint64_t)(up_ar_addr) << 32)"),
        "found buggy `(up_ar_addr) << 32` pack shift (issue #427 regression); \
         the bus-flat width lookup for `up_ar_id` resolved to 32 instead \
         of 3:\n{cpp}"
    );
}

#[test]
fn test_pybind_bus_flat_width_uses_module_param_binding() {
    // Regression for issue #427 sibling site (PR #428 covered the
    // main-sim path at sim_codegen/mod.rs:4750; the pybind binding
    // emitter at sim_codegen/mod.rs:449 had the same latent bug).
    //
    // When a bus's per-signal width is bound through a module-param-
    // substituted bus-alias param, the pybind `_port_info` tuple must
    // report the real width, not the legacy `eval_width` 32-bit
    // fallback that bare `type_bits_te` produced for an unresolved
    // module-param Ident.
    let source =
        include_str!("regression/issues/pybind_bus_flat_param_width/PybindBusFlatParamWidth.arch");
    let pybinds = compile_to_pybind_cpps(source);
    let cpp = pybinds
        .iter()
        .find(|(n, _)| n.contains("PybindBusFlatParamWidth"))
        .expect("PybindBusFlatParamWidth pybind wrapper")
        .1
        .clone();
    // _port_info tuple for `up_data` must carry the param-substituted
    // width (128), not the conservative 32-bit fallback.
    assert!(
        cpp.contains("py::make_tuple(\"up_data\", 128,"),
        "expected `_port_info` tuple `(\"up_data\", 128, ...)` (param-bound \
         width); bus-flat width fold dropped back to the legacy 32-bit \
         fallback:\n{cpp}"
    );
    assert!(
        !cpp.contains("py::make_tuple(\"up_data\", 32,"),
        "found buggy `_port_info` tuple `(\"up_data\", 32, ...)`; the \
         bus-flat width lookup for `up_data` resolved to 32 instead of the \
         param-bound 128:\n{cpp}"
    );
}

#[test]
fn test_fsm_bus_flat_trace_width_uses_construct_param_binding() {
    // Regression for issue #427 sibling site: `src/sim_codegen/fsm.rs`
    // also used bare `type_bits_te` to derive per-signal widths for the
    // waveform trace of FSM-owned bus ports. When the bus's per-signal
    // width is bound through an FSM-param-substituted bus-alias param,
    // the VCD `$var wire <width> ... <name> $end` header line must
    // announce the real lane width.
    let source =
        include_str!("regression/issues/fsm_bus_flat_param_width/FsmBusFlatParamWidth.arch");
    let sim = compile_to_sim_h(source, false);
    // VCD declaration for `up_data` must carry the param-substituted
    // width (96), not the conservative 32-bit fallback.
    let up_data_lines: Vec<&str> = sim
        .lines()
        .filter(|l| l.contains("up_data $end") && l.contains("$var wire"))
        .collect();
    assert_eq!(
        up_data_lines.len(),
        1,
        "expected exactly one VCD `$var wire ... up_data $end` declaration \
         line; found {} in:\n{sim}",
        up_data_lines.len()
    );
    let line = up_data_lines[0];
    assert!(
        line.contains("$var wire 96 "),
        "expected VCD declaration for `up_data` to be 96 bits wide \
         (param-bound DATA_W=96); got:\n  {line}\nin:\n{sim}"
    );
    assert!(
        !line.contains("$var wire 32 "),
        "found buggy 32-bit VCD declaration for `up_data`; the FSM \
         bus-flat trace width lookup resolved to 32 instead of the \
         param-bound 96:\n  {line}"
    );
}

#[test]
fn test_fsm_param_vec_scalar_widths_resolve_through_sibling_helpers() {
    // Combined regression for arch-com#447 §1 (extended by PR after #458).
    // PR #458 deprecated `type_bits_te` / `eval_const_expr` and migrated
    // their call sites to the param-aware form. The follow-up extends the
    // same migration to the sibling cluster (`type_width`,
    // `cpp_port_type`, `cpp_internal_type`, `vec_array_info`).
    //
    // The fixture exercises three of those helpers in one FSM with
    // `param ACC: const = 48`:
    //
    //   - `out_word: out UInt<ACC>` → `cpp_port_type` ⇒ `uint64_t`
    //   - `vec_word: out Vec<UInt<ACC>, 2>` → `cpp_port_type` (per flat
    //     field) AND `vec_array_info` (count=2) ⇒ two `uint64_t
    //     vec_word_<i>` fields plus a `_vec_word[2]` internal array.
    //   - `reg buf: Vec<UInt<ACC>, 4>` → `vec_array_info` (count=4) AND
    //     `cpp_internal_type` ⇒ `uint64_t buf[4]`.
    //
    // Under the bare-form fallback the port bucket would collapse to
    // `uint32_t`, the Vec count would fold to 0 (`buf[0]`,
    // `_vec_word[0]`), and the per-element VCD trace declarations would
    // announce width 32 instead of 48.
    let source =
        include_str!("regression/issues/fsm_param_vec_scalar_widths/FsmParamVecScalarWidths.arch");
    let sim = compile_to_sim_h(source, false);

    // 1. Scalar UInt<ACC> port: bucket must be uint64_t.
    assert!(
        sim.contains("uint64_t out_word"),
        "scalar `out_word` (UInt<48>) must bucket into uint64_t — \
         bare-form `cpp_port_type` would emit uint32_t. sim header:\n{sim}"
    );
    assert!(
        !sim.contains("uint32_t out_word"),
        "found buggy `uint32_t out_word` declaration; param-aware \
         `cpp_port_type_with_params` did not resolve `UInt<ACC>` when \
         ACC=48:\n{sim}"
    );

    // 2. Vec<UInt<ACC>, 2> port: per-element flat fields must be uint64_t,
    //    count must be 2 (not 0).
    assert!(
        sim.contains("uint64_t vec_word_0") && sim.contains("uint64_t vec_word_1"),
        "flat fields `vec_word_0`/`vec_word_1` must be uint64_t (UInt<48>) \
         — bare-form `cpp_port_type` would emit uint32_t or omit them \
         entirely (count=0):\n{sim}"
    );
    assert!(
        sim.contains("uint64_t _vec_word[2]") || sim.contains("uint64_t _vec_word [2]"),
        "internal Vec storage array `_vec_word[2]` must be uint64_t × 2 — \
         bare-form `vec_array_info` would fold the count to 0:\n{sim}"
    );

    // 3. Vec<UInt<ACC>, 4> reg: storage array must be `uint64_t buf[4]`.
    assert!(
        sim.contains("uint64_t buf[4]") || sim.contains("uint64_t buf [4]"),
        "Vec reg `buf` must declare `uint64_t buf[4]` — bare-form \
         `vec_array_info` would emit `uint32_t buf[0]` (count=0, wrong \
         scalar bucket):\n{sim}"
    );
    assert!(
        !sim.contains("uint32_t buf[4]"),
        "found buggy `uint32_t buf[4]` — `cpp_internal_type` bucketed \
         `UInt<48>` to uint32_t instead of uint64_t:\n{sim}"
    );
    assert!(
        !sim.contains("uint64_t buf[0]") && !sim.contains("uint32_t buf[0]"),
        "found buggy `buf[0]` declaration — `vec_array_info` folded the \
         Vec count to 0:\n{sim}"
    );

    // 4. VCD trace dump per-bit loop for `out_word` must run 48 iterations,
    //    not 32. This catches the `add_trace_to_simple_construct` callsite
    //    that previously passed `&[]` for params (free-function helper
    //    didn't take a params slice). When `add_trace_to_simple_construct`
    //    was migrated to accept `params: &[ParamDecl]`, the FSM callsite
    //    started passing `&f.common.params` so `UInt<ACC>` resolves to 48.
    let out_word_trace_line = sim
        .lines()
        .find(|l| l.contains("(out_word >> _i) & 1"))
        .unwrap_or("(out_word trace dump line not found)");
    assert!(
        out_word_trace_line.contains("_i = 48 - 1"),
        "VCD trace dump loop for `out_word` must iterate 48 times \
         (UInt<ACC> with ACC=48); got:\n  {out_word_trace_line}\n\nFull sim:\n{sim}"
    );
}

#[test]
fn test_fast_gate_do_until_mealy_fusion_gates_seq_assigns() {
    // Regression for issue #412: the Mealy-fusion lowering for
    //   if not X
    //     wait until X;
    //   end if
    //   do
    //     <seq>r <= value;
    //   until Y;
    // must gate the do-body's seq assigns by X, just like it already
    // gates the comb assigns. Before the fix, only the comb side and
    // the state transition were gated; the seq assigns fired every
    // cycle the FSM sat in the wait state, regardless of X. That
    // turned "pulse r when X asserts" into "drive r every cycle until
    // X asserts".
    let source = "
        module Drv
          port clk:   in  Clock<SysDomain>;
          port rst:   in  Reset<Async, Low>;
          port go:    in  Bool;
          port done:  in  Bool;
          reg started: Bool reset rst => false;
          thread T on clk rising, rst low
            default comb
            end default
            if not (go)
              wait until go;
            end if
            do
              started <= true;
            until done;
          end thread T
        end module Drv
    ";
    let sv = compile_to_sv(source);
    // The seq assign for `started` must appear under an `if (go)` guard.
    // Find the assignment to `_n_started` (or `started <=` in SV) and
    // verify it sits within the `if (go)` block. Search for the gate
    // followed by the assignment within a few lines.
    // Generated SV uses `if (go) begin ... started <= 1'b1 ... end`.
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        trimmed.contains("if (go) begin started <= 1'b1")
            || trimmed.contains("if (go) started <= 1'b1"),
        "expected do-body seq assign `started <= true` to be wrapped in `if (go)` \
         (issue #412 Mealy-fusion seq-gating). Got SV:\n{sv}",
    );
}

#[test]
fn test_wait_0plus_is_retired_with_migration_hint() {
    // The legacy Mealy spelling is retired. The parser should reject it
    // directly and tell users to spell the fast path with `if not X` plus
    // a normal `wait until X`.
    let source = r#"
        bus B
          v: out Bool;
        end bus B
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go: in Bool;
          port b: initiator B;
          thread T on clk rising, rst low
            default comb
              b.v = false;
            end default
            wait 0+ cycle until go;
            do
              b.v = true;
            until go;
          end thread T
        end module M
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lex");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let err = parser
        .parse_source_file()
        .expect_err("retired wait 0+ syntax should be a parse error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("retired") && msg.contains("if not") && msg.contains("wait until"),
        "expected wait-0+ retirement diagnostic with migration hint, got: {msg}"
    );
}

#[test]
fn test_wait_0plus_requires_no_space_between_0_and_plus() {
    // `0+` is a single user-facing token (no whitespace allowed between
    // the `0` and the `+`). `wait 0 + cycle until X;` (with a space) is
    // NOT a Mealy wait — it must not silently parse as one.
    //
    // With the space, the parser falls through to the numeric `wait N
    // cycle;` form, which expects `cycle` immediately after the expr —
    // `0 + cycle until go` parses `cycle` as an identifier in the binary
    // expression `0 + cycle`, leaving `until` where `cycle` is expected.
    // So we just assert a parse error rather than match a specific msg.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go: in Bool;
          port out: out Bool;
          thread T on clk rising, rst low
            default comb
              out = false;
            end default
            wait 0 + cycle until go;
            do
              out = true;
            until go;
          end thread T
        end module M
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lex");
    let mut parser = arch::parser::Parser::new(tokens, source);
    assert!(
        parser.parse_source_file().is_err(),
        "wait `0 + cycle` (with space) should not parse as Mealy wait"
    );
}

#[test]
fn test_comb_graph_treats_bus_wires_as_intermediates() {
    // Regression: when a parent module wires two instances together through
    // a bus wire (scalar or Vec-of-bus), the cross-instance comb dependency
    // must drive settle_depth = 2. Previously the comb-graph dependency
    // tracker only saw `Ident(wire)` signals, missed `Index(Ident, Lit)`
    // forms (Vec-of-bus wire element references), and computed settle_depth
    // = 1 — which left the sim's eval() pass with stale instance outputs
    // for one cycle and broke handshakes that propagated through bus wires.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B
        module Drv
          port clk: in Clock<SysDomain>;
          port out: initiator B;
          comb
            out.v = true;
            out.d = 8'h5A;
          end comb
        end module Drv
        module Use
          port clk:   in Clock<SysDomain>;
          port inp:   target B;
          port outv:  out Bool;
          comb
            outv = inp.v;
          end comb
        end module Use
        module Top
          port clk:  in Clock<SysDomain>;
          port outv: out Bool;
          wire w: B;
          inst d: Drv  clk <- clk;  out -> w;  end inst d
          inst u: Use  clk <- clk;  inp <- w;  outv -> outv;  end inst u
        end module Top
    ";
    let sv = compile_to_sv(source);
    // The fix lives in `comb_graph::parent_has_comb_intermediates`. With
    // it the SV elaborator does its own bus-wire chain settling, but the
    // dependency is the *sim* settle_depth that doesn't surface in the
    // SV output. So this test exercises the path indirectly by checking
    // the SV still emits valid bus-wire flattening and a clean inst
    // chain — combined with the v2 NIC-400 smoke test (arch sim path)
    // already covered above.
    assert!(
        sv.contains("logic w_v") && sv.contains("logic [7:0] w_d"),
        "expected flattened bus wire signals:\n{sv}"
    );
    assert!(
        sv.contains(".inp_v(w_v)") || sv.contains(".inp_v (w_v)"),
        "expected `.inp_v(w_v)` inst connection:\n{sv}"
    );
}

#[test]
fn test_thread_writing_bus_signals_lowers_correctly() {
    // Regression: previously a `thread T ... b.v = ...; ... end thread T`
    // where `b: initiator B;` is a bus port would lower without exposing
    // `b_v` as an output of the synthesized `_<mod>_threads` sub-module.
    // The parent's driver-completeness check then reported
    // `output port "b_v" is not driven`. Fixed by teaching the
    // signal-collection + body-rewrite passes about bus-port FieldAccess
    // targets, and by widening `declared_names` to include flattened bus
    // signals so the inst auto-wire-decl pass doesn't duplicate them.
    let source = "
        bus B
          v: out Bool;
          d: out UInt<8>;
          r: in  Bool;
        end bus B
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port b: initiator B;
          port start: in Bool;
          thread T on clk rising, rst low
            default comb
              b.v = false;
              b.d = 0;
            end default
            wait until start;
            do
              b.v = true;
              b.d = 8'hA5;
            until b.r;
          end thread T
        end module M
    ";
    let sv = compile_to_sv(source);
    // Sub-module exposes flat bus signals as ports.
    assert!(
        sv.contains("output logic b_v")
            && sv.contains("output logic [7:0] b_d")
            && sv.contains("input logic b_r"),
        "expected sub-module flat bus signals in SV:\n{sv}"
    );
    // Sub-inst connects them through to parent's flat ports — and the
    // parent's signature must NOT have duplicate `logic b_v;` decls.
    assert!(
        sv.matches("logic b_v").count() <= 2,
        "expected at most one `logic b_v` decl per module:\n{sv}"
    );
    assert!(
        sv.contains(".b_v(b_v)") || sv.contains(".b_v (b_v)"),
        "expected `.b_v(b_v)` inst-output connection:\n{sv}"
    );
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
    // and per-element array-indexed assignments (D2 shape).
    assert!(
        !sv.contains("for (int i ="),
        "expected for-loop to be statically unrolled (no behavioral SV for-loop):\n{sv}"
    );
    for i in 0..4 {
        assert!(
            sv.contains(&format!("chans_v[{i}] = 1'b1")),
            "missing unrolled `chans_v[{i}] = 1'b1`:\n{sv}"
        );
        // RHS must reference the literal i (not the loop variable).
        assert!(
            sv.contains(&format!("chans_d[{i}] = 8'(idx + {i})"))
                || sv.contains(&format!("chans_d[{i}] = 8'((idx + {i}))")),
            "missing unrolled `chans_d[{i}] = 8'(idx + {i})`:\n{sv}"
        );
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
    // Whole-vec `chans -> w` now emits ONE packed connection per bus signal —
    // both sides have the same packed shape. No per-element split needed.
    assert!(
        sv.contains(".chans_v(w_v)") || sv.contains(".chans_v (w_v)"),
        "missing `.chans_v(w_v)` packed whole-vec connection:\n{sv}"
    );
    assert!(
        sv.contains(".chans_d(w_d)") || sv.contains(".chans_d (w_d)"),
        "missing `.chans_d(w_d)` packed whole-vec connection:\n{sv}"
    );
    // The parent's bus wire `w` is also packed, so reads like `w[0].d`
    // lower to `w_d[0]`.
    assert!(
        sv.contains("w_d[0]"),
        "expected `w_d[0]` indexed wire read:\n{sv}"
    );
}

#[test]
fn test_vec_of_bus_wire_flattens_to_n_indexed_signals() {
    // `wire w: Vec<BusName, N>;` becomes one packed per-signal storage
    // (`logic [N-1:0]` or `logic [N-1:0][W-1:0]`) at the SV layer.
    // `w[i].sig` access resolves to `w_<sig>[i]`.
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
    // Packed wire: one decl per bus signal with `[N-1:0]` outer dim.
    assert!(
        sv.contains("logic [1:0] w_v;"),
        "missing `logic [1:0] w_v;` packed wire in SV:\n{sv}"
    );
    assert!(
        sv.contains("logic [1:0] [7:0] w_d;"),
        "missing `logic [1:0] [7:0] w_d;` packed wire in SV:\n{sv}"
    );
    // Indexed writes/reads use SV packed slicing.
    for (i, expected_d) in [(0u32, "8'd17"), (1u32, "8'd34")] {
        assert!(
            sv.contains(&format!("w_d[{i}] = {expected_d}")),
            "missing `w_d[{i}] = {expected_d}` assignment in SV:\n{sv}"
        );
    }
    assert!(
        sv.contains("o_v0 = w_v[0]"),
        "expected `o_v0 = w_v[0]` in SV:\n{sv}"
    );
    assert!(
        sv.contains("o_d1 = w_d[1]"),
        "expected `o_d1 = w_d[1]` in SV:\n{sv}"
    );
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
    // Packed Vec-of-bus wire: one decl per bus signal with [N-1:0] outer dim.
    assert!(
        sv.contains("logic [1:0] w_v;"),
        "missing `logic [1:0] w_v;` packed wire in Parent SV:\n{sv}"
    );
    assert!(
        sv.contains("logic [1:0] [7:0] w_d;"),
        "missing `logic [1:0] [7:0] w_d;` packed wire in Parent SV:\n{sv}"
    );
    // Producer inst's Vec-of-bus port connects via packed concat from the
    // per-element parent expressions.
    // `chans[0] -> w[0]; chans[1] -> w[1];` gathers into `.chans_<sig>({w[1].sig, w[0].sig})`.
    assert!(
        sv.contains(".chans_v({w_v[1], w_v[0]})") || sv.contains(".chans_v ({w_v[1], w_v[0]})"),
        "expected `.chans_v({{w_v[1], w_v[0]}})` packed concat connection in SV:\n{sv}"
    );
    assert!(
        sv.contains(".chans_d({w_d[1], w_d[0]})") || sv.contains(".chans_d ({w_d[1], w_d[0]})"),
        "expected `.chans_d({{w_d[1], w_d[0]}})` packed concat connection in SV:\n{sv}"
    );
    // Downstream reads use packed indexed access.
    assert!(
        sv.contains("o_v0 = w_v[0]"),
        "expected `o_v0 = w_v[0]` in SV:\n{sv}"
    );
    assert!(
        sv.contains("o_d1 = w_d[1]"),
        "expected `o_d1 = w_d[1]` in SV:\n{sv}"
    );
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
    // Child exposes one packed port per Vec-of-bus signal.
    assert!(
        sv.contains("output logic [1:0] chans_v"),
        "child should expose `output logic [1:0] chans_v` (packed):\n{sv}"
    );
    assert!(
        sv.contains("output logic [1:0] [7:0] chans_d"),
        "child should expose `output logic [1:0] [7:0] chans_d`:\n{sv}"
    );
    // Per-element scalar wire connections gather into a packed concat
    // at the inst boundary — big-endian, so chans[1] is the MSB.
    assert!(
        sv.contains(".chans_v({w1_v, w0_v})") || sv.contains(".chans_v ({w1_v, w0_v})"),
        "expected `.chans_v({{w1_v, w0_v}})` packed concat connection in SV:\n{sv}"
    );
    assert!(
        sv.contains(".chans_d({w1_d, w0_d})") || sv.contains(".chans_d ({w1_d, w0_d})"),
        "expected `.chans_d({{w1_d, w0_d}})` packed concat connection in SV:\n{sv}"
    );
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
    assert!(
        sv.contains("logic [7:0] w_cmd;") && sv.contains("logic [7:0] w_resp;"),
        "expected flat `w_cmd` / `w_resp` wires:\n{sv}"
    );
    // No `FooBus w;` placeholder left behind.
    assert!(
        !sv.contains("FooBus w"),
        "unexpected `FooBus w` decl (should be flattened):\n{sv}"
    );
    // Field access on bus wire rewrites to flat name.
    assert!(
        sv.contains("assign w_cmd = x_in") || sv.contains("w_cmd = x_in"),
        "expected `w_cmd = x_in` assignment:\n{sv}"
    );
    assert!(
        sv.contains("x_out = w_resp"),
        "expected `x_out = w_resp` assignment:\n{sv}"
    );
    // Inst binding connects to the flat wires.
    assert!(
        sv.contains(".p_cmd(w_cmd)") && sv.contains(".p_resp(w_resp)"),
        "expected inst binding to flat wires:\n{sv}"
    );
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
    let pkg = sf
        .items
        .iter()
        .find_map(|i| {
            if let arch::ast::Item::Package(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("parsed package");
    assert_eq!(
        pkg.structs.len(),
        1,
        "struct should still parse alongside bus"
    );
    assert_eq!(
        pkg.buses.len(),
        1,
        "bus should be collected into package.buses"
    );
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
    assert!(
        sim_h.contains("struct MyBus {"),
        "expected `struct MyBus` in sim structs header:\n{sim_h}"
    );
    assert!(
        sim_h.contains("uint8_t cmd_valid;") && sim_h.contains("uint8_t cmd_data;"),
        "expected bus fields as struct members:\n{sim_h}"
    );

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
    assert!(
        sv.contains("logic b_cmd_valid;"),
        "bus wire should flatten to b_cmd_valid:\n{sv}"
    );
    assert!(
        sv.contains("logic [7:0] b_cmd_data;"),
        "bus wire should flatten to b_cmd_data:\n{sv}"
    );
    assert!(
        sv.contains(".p_cmd_data(b_cmd_data)"),
        "inst binding should use flat wire names:\n{sv}"
    );
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
    assert!(
        !ws.iter()
            .any(|m| m.contains("output port `p`") && m.contains("not connected")),
        "per-field bus binding should not trigger 'not connected': {:?}",
        ws
    );
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
    assert!(
        ws.iter()
            .any(|m| m.contains("output port `p`") && m.contains("not connected")),
        "completely-unbound bus port should still warn: {:?}",
        ws
    );
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
    assert!(
        !ws.iter()
            .any(|m| m.contains("cmd_op") && m.contains("unguarded")
                || (m.contains("cmd_op") && m.contains("outside"))),
        "short-circuit `and` should guard cmd_op read; got: {:?}",
        ws
    );
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
    assert!(
        !ws.iter()
            .any(|m| m.contains("cmd_op") && m.contains("outside")),
        "ternary condition should guard cmd_op read; got: {:?}",
        ws
    );
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
    assert!(
        ws.iter()
            .any(|m| m.contains("cmd_op") && m.contains("outside")),
        "genuinely unguarded cmd_op read should still warn: {:?}",
        ws
    );
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
    sim.generate_pybind()
        .into_iter()
        .map(|m| (m.class_name, m.impl_))
        .collect()
}

#[test]
fn test_pybind_wrapper_uses_thread_sim_api_compat_shims() {
    // `--thread-sim parallel` models intentionally expose an edge-sensitive
    // `eval()` API instead of the normal sim model's separate
    // `eval_comb()` / `eval_posedge()` methods. The pybind wrapper must use
    // compile-time shims so the same wrapper compiles against both model
    // shapes; otherwise `arch sim --pybind --thread-sim parallel` fails at
    // C++ compile time even after the CLI accepts the option combination.
    let source = "
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port start: in Bool;
          port done: out Bool;

          thread on clk rising, rst high
            wait until start;
            done = true;
            wait 1 cycle;
          end thread
        end module M
    ";

    let pybinds = compile_to_pybind_cpps(source);
    let (_, wrapper) = pybinds
        .iter()
        .find(|(n, _)| n == "VM_pybind")
        .expect("M pybind wrapper");
    assert!(
        wrapper.contains("namespace arch_pybind_detail")
            && wrapper.contains("requires(T& t) { t.eval_comb(); }")
            && wrapper.contains("requires(T& t) { t.eval_posedge(); }")
            && wrapper.contains("requires(T& t, uint64_t n) { t.run_cycles(n); }"),
        "pybind wrapper should provide compile-time API compatibility shims:\n{wrapper}"
    );
    assert!(
        wrapper.contains(".def(\"eval_comb\", &arch_pybind_detail::eval_comb<VM>)")
            && wrapper.contains(".def(\"eval_posedge\", &arch_pybind_detail::eval_posedge<VM>)")
            && wrapper.contains(".def(\"run_cycles\", &arch_pybind_detail::run_cycles<VM>)")
            && !wrapper.contains("&VM::eval_comb")
            && !wrapper.contains("&VM::eval_posedge"),
        "pybind wrapper should not directly bind normal-sim-only methods:\n{wrapper}"
    );

    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let module = ast
        .items
        .iter()
        .find_map(|item| match item {
            arch::ast::Item::Module(m) => Some(m),
            _ => None,
        })
        .expect("module");
    let thread_model = arch::sim_codegen::thread_sim::gen_module_thread(module, false, false, 1)
        .expect("thread sim model");
    assert!(
        thread_model.header.contains("void eval()")
            && !thread_model.header.contains("eval_comb")
            && !thread_model.header.contains("eval_posedge"),
        "test fixture should exercise the thread-sim-only eval API:\n{}",
        thread_model.header
    );
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
    let prim = pybinds
        .iter()
        .find(|(n, _)| n.contains("PrimitivesOnly"))
        .expect("PrimitivesOnly pybind wrapper")
        .1
        .clone();
    let one = pybinds
        .iter()
        .find(|(n, _)| n.contains("UsesOneStruct"))
        .expect("UsesOneStruct pybind wrapper")
        .1
        .clone();
    let uses = pybinds
        .iter()
        .find(|(n, _)| n.contains("UsesStructs"))
        .expect("UsesStructs pybind wrapper")
        .1
        .clone();

    // PrimitivesOnly: no struct bindings at all.
    assert!(
        !prim.contains("py::class_<Reg1>"),
        "PrimitivesOnly must not bind Reg1:\n{prim}"
    );
    assert!(
        !prim.contains("py::class_<Reg2>"),
        "PrimitivesOnly must not bind Reg2:\n{prim}"
    );
    assert!(
        !prim.contains("py::class_<PipeBus>"),
        "PrimitivesOnly must not bind PipeBus:\n{prim}"
    );

    // UsesOneStruct: binds PipeBus, nothing else.
    assert!(
        one.contains("py::class_<PipeBus>"),
        "UsesOneStruct must bind PipeBus:\n{one}"
    );
    assert!(
        !one.contains("py::class_<Reg1>"),
        "UsesOneStruct must not bind Reg1:\n{one}"
    );
    assert!(
        !one.contains("py::class_<Reg2>"),
        "UsesOneStruct must not bind Reg2:\n{one}"
    );

    // UsesStructs: binds Reg1 and Reg2 (internal reg types).
    assert!(
        uses.contains("py::class_<Reg1>"),
        "UsesStructs must bind Reg1:\n{uses}"
    );
    assert!(
        uses.contains("py::class_<Reg2>"),
        "UsesStructs must bind Reg2:\n{uses}"
    );
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
    let m = pybinds
        .iter()
        .find(|(n, _)| n.contains("VM_pybind"))
        .expect("M pybind wrapper")
        .1
        .clone();
    assert!(m.contains("py::class_<Outer>"), "must bind Outer:\n{m}");
    assert!(
        m.contains("py::class_<Inner>"),
        "must bind Inner (transitive via Outer.inner):\n{m}"
    );
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
    assert!(
        !h.contains("(_let_scratch >> _i)"),
        "scratch (struct wire) leaked into trace:\n{h}"
    );
    assert!(
        !h.contains("(_let_view >> _i)"),
        "view (struct let) leaked into trace:\n{h}"
    );
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
    assert!(
        sv.contains(
            "typedef struct packed { logic found; logic [1:0] index; } __ArchFindResult_2;"
        ),
        "expected ArchFindResult typedef: {sv}"
    );
    // Raw OR reduction for `found`, no spurious struct literal:
    assert!(
        sv.contains("assign found = vec[0] == needle || vec[1] == needle"),
        "expected OR reduction: {sv}"
    );
    // Priority encoder for `index`, nested ternary:
    assert!(
        sv.contains("assign index = (vec[0] == needle) ? 2'd0 : (vec[1] == needle) ? 2'd1"),
        "expected priority encoder: {sv}"
    );
    // Correct width on `index`:
    assert!(
        sv.contains("logic [1:0] index;"),
        "expected 2-bit index wire: {sv}"
    );
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
    assert!(
        sv.contains("logic found;") && sv.contains("assign found ="),
        "expected `found` wire + assign: {sv}"
    );
    // No `index` wire should be emitted since the user didn't bind it.
    // (It can still appear as a module port in the future; for now, assert
    // no `logic index` declaration at module scope.)
    assert!(
        !sv.lines()
            .any(|l| l.trim_start().starts_with("logic ") && l.contains(" index;")),
        "did not expect unbound `index`: {sv}"
    );
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
    assert!(
        result.is_err(),
        "expected type-check error for bad destructure binding"
    );
    let msg = format!("{:?}", result.unwrap_err());
    assert!(
        msg.contains("find_first result has no field named `wrong_name`"),
        "expected specific error message, got: {msg}"
    );
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
    assert_eq!(
        compile_to_sv(src_new),
        compile_to_sv(src_old),
        "pipe_reg<T, 1> + @1 should be byte-identical to port reg"
    );
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
    assert!(
        sv.contains("q_stg1") && sv.contains("q_stg2"),
        "expected 2 intermediate stages for depth=3:\n{sv}"
    );
    assert!(sv.contains("q_stg1 <= a;"), "stage 0 write missing:\n{sv}");
    assert!(
        sv.contains("q_stg2 <= q_stg1;"),
        "stage 1 shift missing:\n{sv}"
    );
    assert!(
        sv.contains("q <= q_stg2;"),
        "final output write missing:\n{sv}"
    );
    // Uniform reset across all stages:
    assert!(
        sv.contains("q_stg1 <= 0") && sv.contains("q_stg2 <= 0") && sv.contains("q <= 0"),
        "expected uniform reset across all stages:\n{sv}"
    );
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
    assert!(
        msg.contains("exceeds declared latency 3"),
        "expected specific error message, got: {msg}"
    );
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
    assert!(
        msg.contains("is ambiguous"),
        "expected specific error message, got: {msg}"
    );
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
    std::fs::write(
        &src,
        "\
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
    ",
    )
    .unwrap();
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("check")
        .arg(&src)
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "arch check should succeed with stdlib discovery; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
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
    assert!(
        !out.status.success(),
        "expected failure when ARCH_NO_STDLIB=1 disables stdlib resolution"
    );
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
    assert!(
        sv.contains("output logic [15:0] p_t_data"),
        "Full: data 16-bit expected:\n{sv}"
    );
    assert!(
        sv.contains("output logic p_t_last"),
        "Full: t_last present:\n{sv}"
    );
    assert!(
        sv.contains("output logic [3:0] p_t_id"),
        "Full: t_id [3:0]:\n{sv}"
    );
    // Bare config omits t_last (USE_LAST=0) AND t_id (ID_W=0).
    // Both Full and Bare emit into the same SV string; check Bare's
    // module block specifically for the absence of those fields.
    let bare = sv.split("module Bare").nth(1).expect("Bare module present");
    let bare_until_end = bare.split("endmodule").next().unwrap_or("");
    assert!(
        !bare_until_end.contains("t_last"),
        "Bare config should omit t_last: {bare_until_end}"
    );
    assert!(
        !bare_until_end.contains("t_id"),
        "Bare config should omit t_id: {bare_until_end}"
    );
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
    assert!(
        msg.contains("not supported when the handshake itself is nested"),
        "expected specific nesting-error message, got: {msg}"
    );
}

#[test]
fn test_stdlib_bus_apb_discovery_apb3_minimal() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("Csr.arch");
    std::fs::write(
        &src,
        "\
        use BusApb;\n\
        module Csr\n\
          port clk: in Clock<SysDomain>;\n\
          port rst: in Reset<Sync>;\n\
          port s_apb: target BusApb<ADDR_W=12, DATA_W=32>;\n\
          comb s_apb.pready = 1'b1; s_apb.prdata = 32'h0; end comb\n\
        end module Csr\n\
    ",
    )
    .unwrap();
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("check")
        .arg(&src)
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "APB3 minimal should compile; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_stdlib_bus_apb_pslverr_decoupled_from_pprot() {
    // pslverr (APB3 baseline, IHI 0024B 2008) used to be gated under
    // USE_PPROT (APB4 protection, IHI 0024C 2010). They're now
    // independent toggles. This test exercises the four combinations
    // and asserts the generated SV port list matches each.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    // Helper: build a target-side stub for each toggle pair, run
    // `arch build`, return the generated SV text.
    let cases: &[(u32, u32)] = &[(0, 0), (1, 0), (0, 1), (1, 1)];
    for (use_pslverr, use_pprot) in cases {
        let name = format!("Stub_v{use_pslverr}_p{use_pprot}");
        let src = td.path().join(format!("{name}.arch"));
        // Target side: pslverr direction flips to `out`, so we must
        // drive it whenever USE_PSLVERR=1.
        let pslverr_drive = if *use_pslverr == 1 {
            "s_apb.pslverr = 1'b0;\n            "
        } else {
            ""
        };
        std::fs::write(&src, format!("\
            use BusApb;\n\
            module {name}\n\
              port clk: in Clock<SysDomain>;\n\
              port rst: in Reset<Sync>;\n\
              port s_apb: target BusApb<ADDR_W=12, DATA_W=32, USE_PSLVERR={use_pslverr}, USE_PPROT={use_pprot}>;\n\
              comb\n\
                s_apb.pready = 1'b1;\n\
                s_apb.prdata = 32'h0;\n\
                {pslverr_drive}\
              end comb\n\
            end module {name}\n\
        ")).unwrap();

        // arch check should succeed for every combination.
        let chk = std::process::Command::new(arch_bin)
            .arg("check")
            .arg(&src)
            .output()
            .expect("run arch check");
        assert!(
            chk.status.success(),
            "arch check failed for USE_PSLVERR={use_pslverr} USE_PPROT={use_pprot}; stderr:\n{}",
            String::from_utf8_lossy(&chk.stderr)
        );

        // Build SV and inspect the generated port list.
        let sv_out = td.path().join(format!("{name}.sv"));
        let bld = std::process::Command::new(arch_bin)
            .arg("build")
            .arg(&src)
            .arg("-o")
            .arg(&sv_out)
            .output()
            .expect("run arch build");
        assert!(
            bld.status.success(),
            "arch build failed for USE_PSLVERR={use_pslverr} USE_PPROT={use_pprot}; stderr:\n{}",
            String::from_utf8_lossy(&bld.stderr)
        );
        let sv = std::fs::read_to_string(&sv_out).expect("read sv");

        // Baseline APB v2 signals always present.
        for s in [
            "s_apb_psel",
            "s_apb_penable",
            "s_apb_pwrite",
            "s_apb_paddr",
            "s_apb_pwdata",
            "s_apb_pready",
            "s_apb_prdata",
        ] {
            assert!(sv.contains(s),
                "missing baseline signal {s} for USE_PSLVERR={use_pslverr} USE_PPROT={use_pprot}\nSV:\n{sv}");
        }
        // pslverr gated solely by USE_PSLVERR.
        let has_pslverr = sv.contains("s_apb_pslverr");
        assert_eq!(has_pslverr, *use_pslverr == 1,
            "pslverr presence mismatch for USE_PSLVERR={use_pslverr} USE_PPROT={use_pprot}\nSV:\n{sv}");
        // pprot gated solely by USE_PPROT.
        let has_pprot = sv.contains("s_apb_pprot");
        assert_eq!(has_pprot, *use_pprot == 1,
            "pprot presence mismatch for USE_PSLVERR={use_pslverr} USE_PPROT={use_pprot}\nSV:\n{sv}");
    }
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
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast)
        .check()
        .expect("typecheck");
    assert!(
        warnings.iter().any(|w| w.message.contains("`port reg q")
            && w.message.contains("deprecated")
            && w.message.contains("pipe_reg<T, 1>")),
        "expected deprecation warning, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
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
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast)
        .check()
        .expect("typecheck");
    assert!(
        !warnings.iter().any(|w| w.message.contains("deprecated")),
        "did not expect deprecation warning for pipe_reg<T,1>, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
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
    assert!(
        sv.contains("output logic p_cmd_valid"),
        "handshake_channel should expand to the same ports as legacy `handshake`:\n{sv}"
    );
    assert!(
        sv.contains("input logic p_cmd_ready"),
        "handshake_channel should emit the ready signal:\n{sv}"
    );
    assert!(
        sv.contains("output logic [31:0] p_cmd_addr"),
        "handshake_channel should emit the payload:\n{sv}"
    );
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
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast)
        .check()
        .expect("typecheck");
    assert!(
        warnings.iter().any(|w| w.message.contains("`handshake cmd")
            && w.message.contains("deprecated")
            && w.message.contains("handshake_channel")),
        "expected deprecation warning, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
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
    let (warnings, _) = arch::typecheck::TypeChecker::new(&symbols, &ast)
        .check()
        .expect("typecheck");
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("handshake_channel") && w.message.contains("deprecated")),
        "did not expect deprecation warning for handshake_channel form, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
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
    let bus = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Bus(b) if b.name.name == "DmaCh" => Some(b),
            _ => None,
        })
        .expect("DmaCh bus should be in AST");
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
    assert!(
        sv.contains("output logic p_data_send_valid"),
        "credit_channel should emit send_valid as an initiator output:\n{sv}"
    );
    assert!(
        sv.contains("output logic [15:0] p_data_send_data"),
        "credit_channel should emit send_data with the payload type:\n{sv}"
    );
    assert!(
        sv.contains("input logic p_data_credit_return"),
        "credit_channel should emit credit_return as an initiator input:\n{sv}"
    );
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
    assert!(
        sv.contains("input logic p_data_send_valid"),
        "on target perspective, send_valid should be an input:\n{sv}"
    );
    assert!(
        sv.contains("input logic [15:0] p_data_send_data"),
        "on target perspective, send_data should be an input:\n{sv}"
    );
    assert!(
        sv.contains("output logic p_data_credit_return"),
        "on target perspective, credit_return should be an output:\n{sv}"
    );
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
    assert!(
        sv.contains("__p_data_credit"),
        "credit register should be declared:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_can_send"),
        "can_send wire should be declared:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_can_send = __p_data_credit != 0"),
        "can_send wire should read the credit reg:\n{sv}"
    );
    assert!(
        sv.contains("p_data_send_valid && !p_data_credit_return"),
        "counter-update should decrement on pure send:\n{sv}"
    );
    assert!(
        sv.contains("p_data_credit_return && !p_data_send_valid"),
        "counter-update should increment on pure credit_return:\n{sv}"
    );
    assert!(
        sv.contains("always_ff"),
        "counter should update in an always_ff block:\n{sv}"
    );
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
    assert!(
        sv.contains("__p_data_buf"),
        "target FIFO buffer array should be declared:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_head"),
        "FIFO head pointer should be declared:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_tail"),
        "FIFO tail pointer should be declared:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_occ"),
        "FIFO occupancy should be declared:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_valid = __p_data_occ != 0"),
        "valid wire should report non-empty:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_data = __p_data_buf[__p_data_head]"),
        "data wire should read the head slot:\n{sv}"
    );
    assert!(
        sv.contains("if (p_data_send_valid)"),
        "push path should be gated on send_valid:\n{sv}"
    );
    assert!(
        sv.contains("p_data_credit_return && __p_data_valid"),
        "pop should fire on user-driven credit_return when FIFO non-empty:\n{sv}"
    );
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
    assert!(
        !sv.contains("__p_data_buf"),
        "sender-role module must not emit target FIFO buffer:\n{sv}"
    );
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
    assert!(
        !sv.contains("__p_data_credit"),
        "target-role module should not emit sender counter:\n{sv}"
    );
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
    assert!(
        sv.contains("__p_data_can_send"),
        "dispatch should rewrite p.data.can_send → __p_data_can_send:\n{sv}"
    );
    assert!(
        sv.contains("p_data_send_valid = __p_data_can_send"),
        "valid assignment should reference the rewritten name:\n{sv}"
    );
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
    assert!(
        sv.contains("latest = __p_data_data"),
        "receiver read of p.data.data should rewrite to __p_data_data:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_valid"),
        "p.data.valid should rewrite to __p_data_valid:\n{sv}"
    );
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
    assert!(
        sv.contains("logic __p_data_can_send;"),
        "CAN_SEND_REGISTERED=1 should declare can_send as a register:\n{sv}"
    );
    // And assigns it inside the always_ff block.
    assert!(
        sv.contains("__p_data_can_send <="),
        "registered can_send should be updated via non-blocking assign:\n{sv}"
    );
    // No `wire` form for can_send.
    assert!(
        !sv.contains("wire  __p_data_can_send"),
        "CAN_SEND_REGISTERED=1 must not emit the combinational wire form:\n{sv}"
    );
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
    assert!(
        sv.contains("wire  __p_data_can_send = __p_data_credit != 0"),
        "default (unregistered) can_send should stay combinational:\n{sv}"
    );
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
    assert!(
        sv.contains("_auto_cc_p_data_credit_bounds"),
        "credit_bounds assertion label should be present on sender:\n{sv}"
    );
    assert!(
        sv.contains("__p_data_credit <= (4)"),
        "credit_bounds property should compare credit reg to DEPTH:\n{sv}"
    );
    assert!(
        sv.contains("_auto_cc_p_data_send_requires_credit"),
        "send_requires_credit assertion should be present:\n{sv}"
    );
    assert!(
        sv.contains("p_data_send_valid |-> __p_data_credit > 0"),
        "send_requires_credit property should encode valid-implies-credit:\n{sv}"
    );
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
    assert!(
        sv.contains("_auto_cc_p_data_credit_return_requires_buffered"),
        "receiver-side assertion should be present:\n{sv}"
    );
    assert!(
        sv.contains("p_data_credit_return |-> __p_data_valid"),
        "credit_return should imply buffer-non-empty:\n{sv}"
    );
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
    assert!(
        sv.contains("p_data_send_valid = 1'd1") || sv.contains("p_data_send_valid = 1'b1"),
        ".send() should set send_valid to 1:\n{sv}"
    );
    assert!(
        sv.contains("p_data_send_data = payload"),
        ".send(payload) should set send_data to payload:\n{sv}"
    );
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
    assert!(
        sv.contains("p_data_credit_return = 1'd1") || sv.contains("p_data_credit_return = 1'b1"),
        ".pop() should assert credit_return:\n{sv}"
    );
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
    assert!(
        sv.contains("__out_flits_credit"),
        "sender credit reg:\n{sv}"
    );
    assert!(
        sv.contains("__out_flits_can_send"),
        "sender can_send:\n{sv}"
    );
    assert!(
        sv.contains("_auto_cc_out_flits_credit_bounds"),
        "sender SVA:\n{sv}"
    );
    assert!(
        sv.contains("_auto_cc_out_flits_send_requires_credit"),
        "sender SVA:\n{sv}"
    );

    // .send(x) sugar must materialize both signals
    assert!(
        sv.contains("out_flits_send_valid = 1'd1"),
        "send sugar valid:\n{sv}"
    );
    assert!(
        sv.contains("out_flits_send_data = seq_no"),
        "send sugar data:\n{sv}"
    );

    // Receiver-side checks
    assert!(
        sv.contains("__incoming_flits_buf"),
        "receiver buffer:\n{sv}"
    );
    assert!(
        sv.contains("__incoming_flits_valid"),
        "receiver valid wire:\n{sv}"
    );
    assert!(
        sv.contains("__incoming_flits_data"),
        "receiver data wire:\n{sv}"
    );
    assert!(
        sv.contains("_auto_cc_incoming_flits_credit_return_requires_buffered"),
        "receiver SVA:\n{sv}"
    );

    // .pop() sugar
    assert!(
        sv.contains("incoming_flits_credit_return = 1'd1"),
        "pop sugar credit_return:\n{sv}"
    );

    // Read-side dispatch in seq and comb contexts
    assert!(
        sv.contains("last_seq <= __incoming_flits_data"),
        "read-side dispatch in seq:\n{sv}"
    );
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
    assert!(
        out.contains("uint32_t __p_data_credit;"),
        "sender credit field should be declared:\n{out}"
    );
    assert!(
        out.contains("uint8_t  __p_data_can_send;"),
        "sender can_send field should be declared:\n{out}"
    );
    assert!(
        out.contains("__p_data_credit = 4;"),
        "constructor should initialize credit to DEPTH:\n{out}"
    );
    assert!(
        out.contains("__p_data_can_send = (__p_data_credit != 0)"),
        "eval_comb should assign can_send combinationally:\n{out}"
    );
    assert!(
        out.contains("__p_data_credit--"),
        "eval_posedge should decrement on pure send:\n{out}"
    );
    assert!(
        out.contains("__p_data_credit++"),
        "eval_posedge should increment on pure credit_return:\n{out}"
    );
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
    assert!(
        out.contains("uint8_t __p_data_buf[4];"),
        "receiver buffer array should be declared with correct width + depth:\n{out}"
    );
    assert!(
        out.contains("__p_data_head;")
            && out.contains("__p_data_tail;")
            && out.contains("__p_data_occ;"),
        "head/tail/occ pointers should be declared:\n{out}"
    );
    assert!(
        out.contains("__p_data_valid = (__p_data_occ != 0)"),
        "valid should be computed in eval_comb:\n{out}"
    );
    assert!(
        out.contains("__p_data_data  = __p_data_buf[__p_data_head]"),
        "data should read front of buffer in eval_comb:\n{out}"
    );
    assert!(
        out.contains("p_data_credit_return && __p_data_valid"),
        "pop should fire on user-driven credit_return when valid:\n{out}"
    );
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
    let bus = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Bus(b) if b.name.name == "Mem" => Some(b),
            _ => None,
        })
        .expect("Mem bus in AST");
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
    let bus = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Bus(b) if b.name.name == "Mem" => Some(b),
            _ => None,
        })
        .expect("Mem bus in AST");
    assert_eq!(bus.tlm_methods[0].mode.name, "out_of_order");
    assert!(bus.tlm_methods[0].out_of_order_tags.is_some());

    let sv = compile_to_sv(source);
    assert!(
        sv.contains("output logic [2:0] m_read_req_tag")
            && sv.contains("input logic [2:0] m_read_rsp_tag"),
        "initiator should expose out-of-order tag wires:\n{sv}"
    );
    assert!(
        sv.contains("input logic [2:0] s_read_req_tag")
            && sv.contains("output logic [2:0] s_read_rsp_tag"),
        "target perspective should flip tag wire directions:\n{sv}"
    );
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
    assert!(
        sv.contains("output logic m_read_req_valid"),
        "req_valid should be an initiator output:\n{sv}"
    );
    assert!(
        sv.contains("output logic [31:0] m_read_addr"),
        "arg should appear as initiator output with its declared type:\n{sv}"
    );
    assert!(
        sv.contains("input logic m_read_req_ready"),
        "req_ready should flow back to initiator:\n{sv}"
    );
    assert!(
        sv.contains("input logic m_read_rsp_valid"),
        "rsp_valid should be an initiator input:\n{sv}"
    );
    assert!(
        sv.contains("input logic [63:0] m_read_rsp_data"),
        "rsp_data should appear with declared ret type:\n{sv}"
    );
    assert!(
        sv.contains("output logic m_read_rsp_ready"),
        "rsp_ready flows back from initiator to target:\n{sv}"
    );
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
    assert!(
        sv.contains("logic [2:0] link_read_req_tag;"),
        "implicit req tag wire should use overridden KV_HEAD_W:\n{sv}"
    );
    assert!(
        sv.contains("logic [4:0] link_read_tile;"),
        "implicit arg wire should use overridden TILE_W:\n{sv}"
    );
    assert!(
        sv.contains("logic [16:0] link_read_rsp_data;"),
        "implicit response wire should use overridden TOKEN_W:\n{sv}"
    );
    assert!(
        !sv.contains("KV_HEAD_W") && !sv.contains("TILE_W") && !sv.contains("TOKEN_W"),
        "generated SV should not leak bus param identifiers into Parent plumbing:\n{sv}"
    );
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
    assert!(
        sv.contains("logic [4:0] _tlm_m_read_tile_latched;"),
        "target latch reg should use substituted TILE_W:\n{sv}"
    );
    assert!(
        sv.contains("output logic [16:0] m_read_rsp_data"),
        "target response data should use substituted TOKEN_W:\n{sv}"
    );
    assert!(
        sv.contains("assign m_read_tile = _tlm_init_m_read_grant_0 ? 5'd3 : 0;"),
        "initiator request drive should use substituted TILE_W:\n{sv}"
    );
    assert!(
        !sv.contains("TILE_W") && !sv.contains("TOKEN_W"),
        "lowered generated SV should not leak bus param identifiers:\n{sv}"
    );
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
    assert!(
        !sv.contains("poke_rsp_data"),
        "void methods must not emit rsp_data:\n{sv}"
    );
    assert!(
        sv.contains("poke_rsp_valid"),
        "void methods still need rsp_valid/ready for back-pressure:\n{sv}"
    );
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
    let m = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Module(m) if m.name.name == "MemTarget" => Some(m),
            _ => None,
        })
        .expect("MemTarget in AST");
    let t = m
        .body
        .iter()
        .find_map(|i| match i {
            arch::ast::ModuleBodyItem::Thread(t) => Some(t),
            _ => None,
        })
        .expect("thread in MemTarget body");
    let binding = t
        .tlm_target
        .as_ref()
        .expect("tlm_target should be populated");
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
    assert!(
        sv.contains("_tlm_init_driver_state"),
        "state reg should be emitted:\n{sv}"
    );
    assert!(
        sv.contains("m_read_req_valid"),
        "SV should drive req_valid:\n{sv}"
    );
    assert!(sv.contains("m_read_addr"), "SV should drive the arg:\n{sv}");
    assert!(
        sv.contains("m_read_rsp_ready"),
        "SV should drive rsp_ready:\n{sv}"
    );
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
    assert!(
        out.contains("_tlm_s_read_state"),
        "state reg should appear in sim C++:\n{out}"
    );
    assert!(
        out.contains("_tlm_s_read_addr_latched"),
        "arg latch reg should appear in sim C++:\n{out}"
    );
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
    assert!(
        out.contains("_tlm_init_driver_state"),
        "initiator state reg should appear in sim C++:\n{out}"
    );
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
    assert!(
        out.contains("uint32_t _m_read4_rsp_data[4];"),
        "flattened bus Vec payload should have an internal array:\n{out}"
    );
    assert!(
        out.contains("_m_read4_rsp_data[0] = m_read4_rsp_data_0;"),
        "input bridge should copy flat fields into the internal array:\n{out}"
    );
    assert!(
        out.contains("for (size_t _i = 0; _i < 4; ++_i) { _n_data[_i] = _m_read4_rsp_data[_i]; }"),
        "whole-Vec TLM response assignment should lower to element copy:\n{out}"
    );
}

#[test]
fn test_whole_vec_scalar_zero_seq_assign_uses_memset() {
    let source = "
        module VecClear
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          reg data: Vec<SInt<32>, 4> reset rst => 0;

          thread driver on clk rising, rst high
            data <= 0;
            wait 1 cycle;
          end thread driver
        end module VecClear
    ";
    let out = compile_to_sim_h(source, false);
    assert!(
        out.contains("memset(_n_data, 0, sizeof(_n_data));"),
        "whole-Vec scalar zero assignment should lower to memset, not C++ array assignment:\n{out}"
    );
    assert!(
        !out.contains("_n_data  = 0") && !out.contains("_n_data = 0"),
        "whole-Vec scalar zero assignment must not emit array assignment:\n{out}"
    );
}

#[test]
fn test_thread_loop_vec_bounds_assertion_is_state_guarded() {
    let source = "
        module ThreadLoopVecBounds
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port start: in Bool;
          port value: in UInt<8>;
          port done: out pipe_reg<Bool, 1> reset rst => false;
          reg data: Vec<UInt<8>, 4> reset rst => 0;

          thread driver on clk rising, rst high
            wait until start;
            for pos in 0..3
              data[pos] <= value;
              wait 1 cycle;
            end for
            done <= true;
            wait 1 cycle;
            done <= false;
          end thread driver
        end module ThreadLoopVecBounds
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_auto_bound_vec_0: assert property"),
        "expected generated Vec bounds assertion:\n{sv}"
    );
    assert!(
        sv.contains("|-> (int'(_t0_loop_cnt_0) < (4))"),
        "thread-loop Vec bounds assertion should be guarded by the active state, not always-on:\n{sv}"
    );
    assert!(
        sv.contains("_t0_state == _t0_S"),
        "thread-loop Vec bounds assertion should mention the lowering state guard:\n{sv}"
    );
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
    assert!(
        out.contains("uint32_t data[4];"),
        "struct Vec field should emit as a C++ array:\n{out}"
    );
    assert!(
        out.contains("out0  = _r.data[0];"),
        "struct Vec field read should use array indexing, not bit extraction:\n{out}"
    );
    assert!(
        out.contains("_n_r.data[0]  = in0;"),
        "struct Vec field write should use array indexing:\n{out}"
    );
    assert!(
        !out.contains("((_n_r.data) >> (0)) & 1"),
        "struct Vec field must not be treated as scalar bit indexing:\n{out}"
    );
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
    assert!(
        out.contains("BoundedVecResp32x4 m_read_burst_rsp_data;"),
        "TLM struct response should expose the struct payload in sim:\n{out}"
    );
    assert!(
        out.contains("_n_r  = m_read_burst_rsp_data;"),
        "TLM struct response should copy into the destination register:\n{out}"
    );
    assert!(
        !out.contains("m_read_burst_rsp_data >>"),
        "struct response should not be emitted as a scalar trace expression:\n{out}"
    );
}

#[test]
fn test_tlm_canonical_end_to_end_initiator_plus_target() {
    // PR-tlm-7: canonical validation — a minimal Mem bus with `read`
    // and `write` methods, plus initiator + target pair exercising
    // both sides of the wire protocol.
    let source = include_str!("axi_dma_tlm/TlmOneToOne.arch");
    let sv = compile_to_sv(source);

    // Target-side state machines.
    assert!(
        sv.contains("_tlm_s_read_state"),
        "target: read state reg should appear:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_write_state"),
        "target: write state reg should appear:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_read_addr_latched"),
        "target: read arg latch reg should appear:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_write_addr_latched") && sv.contains("_tlm_s_write_data_latched"),
        "target: write arg latch regs should appear:\n{sv}"
    );

    // Initiator state machine + wire drives.
    assert!(
        sv.contains("_tlm_init_driver_state"),
        "initiator: driver state reg should appear:\n{sv}"
    );
    assert!(
        sv.contains("m_read_req_valid") && sv.contains("m_write_req_valid"),
        "initiator: both methods should drive req_valid:\n{sv}"
    );
    assert!(
        sv.contains("m_read_addr") && sv.contains("m_write_addr") && sv.contains("m_write_data"),
        "initiator: arg signals should appear:\n{sv}"
    );
    assert!(
        sv.contains("module TlmOneToOneTop") && sv.contains("link_read_req_valid"),
        "top-level one-to-one bus wire connection should appear:\n{sv}"
    );

    // Compile to sim C++ too — same path should flow through the existing
    // reg/seq/comb sim mirror without issues.
    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("_tlm_s_read_state") && sim.contains("_tlm_init_driver_state"),
        "sim C++ should mirror the state regs for both sides"
    );
}

#[test]
fn test_tlm_connect_one_to_one_sugar_lowers_to_bus_wire() {
    let source = include_str!("axi_dma_tlm/TlmConnectOneToOne.arch");
    let sv = compile_to_sv(source);

    assert!(
        sv.contains("module TlmConnectOneToOneTop"),
        "connect-sugar top should build:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_conn_i_m_t_s_read_req_valid")
            && sv.contains("_tlm_conn_i_m_t_s_write_req_valid"),
        "connect sugar should synthesize a private flattened TLM bus wire:\n{sv}"
    );
    assert!(
        sv.contains(".m_read_req_valid(_tlm_conn_i_m_t_s_read_req_valid)")
            && sv.contains(".s_read_req_valid(_tlm_conn_i_m_t_s_read_req_valid)"),
        "connect sugar should wire initiator and target endpoints together:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("class VTlmConnectOneToOneTop"),
        "sim C++ should include the connect-sugar top"
    );
}

#[test]
fn test_tlm_connect_inside_generate_for_lowers_to_per_iteration_wires() {
    let source = include_str!("axi_dma_tlm/TlmConnectGenerate.arch");
    let sv = compile_to_sv(source);

    assert!(
        sv.contains("module TlmConnectGenerateTop"),
        "generate-for connect-sugar top should build:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_conn_src_0_m_dst_0_s_read_req_valid")
            && sv.contains("_tlm_conn_src_1_m_dst_1_s_read_req_valid"),
        "generate-for connect sugar should synthesize one private TLM bus per iteration:\n{sv}"
    );
    assert!(
        sv.contains(".m_read_req_valid(_tlm_conn_src_0_m_dst_0_s_read_req_valid)")
            && sv.contains(".s_read_req_valid(_tlm_conn_src_0_m_dst_0_s_read_req_valid)")
            && sv.contains(".m_read_req_valid(_tlm_conn_src_1_m_dst_1_s_read_req_valid)")
            && sv.contains(".s_read_req_valid(_tlm_conn_src_1_m_dst_1_s_read_req_valid)"),
        "unrolled initiator and target endpoints should be wired pairwise:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("class VTlmConnectGenerateTop"),
        "sim C++ should include the generate-for connect-sugar top"
    );
}

#[test]
fn test_tlm_connect_decode_lowers_to_generated_router_logic() {
    let source = include_str!("axi_dma_tlm/TlmConnectDecode.arch");
    let sv = compile_to_sv(source);

    assert!(
        sv.contains("module TlmConnectDecodeTop"),
        "decoded connect top should build:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_conn_i_m_decode_up_read_req_valid")
            && sv.contains("_tlm_conn_i_m_decode_t0_read_req_valid")
            && sv.contains("_tlm_conn_i_m_decode_t1_read_req_valid"),
        "decoded connect should synthesize upstream and per-target private bus wires:\n{sv}"
    );
    assert!(
        sv.contains(
            "_tlm_conn_i_m_decode_t0_read_req_valid = _tlm_conn_i_m_decode_up_read_req_valid",
        ) && sv.contains(
            "_tlm_conn_i_m_decode_t1_read_req_valid = _tlm_conn_i_m_decode_up_read_req_valid",
        ),
        "decoded connect should gate request valid to target wires:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_conn_i_m_decode_read_route")
            && sv.contains("_tlm_conn_i_m_decode_write_route"),
        "decoded connect should remember response route per blocking method:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("class VTlmConnectDecodeTop"),
        "sim C++ should include decoded connect top"
    );
}

#[test]
fn test_tlm_connect_decode_allows_target_method_subset() {
    let source = include_str!("axi_dma_tlm/TlmConnectSubtype.arch");
    let sv = compile_to_sv(source);

    assert!(
        sv.contains("module TlmConnectSubtypeTop"),
        "decoded subtype connect top should build:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_conn_i_m_decode_t0_write_req_valid")
            && !sv.contains("_tlm_conn_i_m_decode_t1_write_req_valid"),
        "router should drive write only to the read/write target wire:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_conn_i_m_decode_write_err_valid")
            && sv.contains("_tlm_conn_i_m_decode_up_write_rsp_data = _tlm_conn_i_m_decode_write_err_valid ? 1'b0"),
        "router should synthesize a failed Bool response for writes decoded to the read-only target:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("class VTlmConnectSubtypeTop"),
        "sim C++ should include decoded subtype connect top"
    );
}

#[test]
fn test_tlm_connect_decode_does_not_emit_false_comb_cycle_warning() {
    let ws = warnings_after_full_lower(include_str!("axi_dma_tlm/TlmConnectDecode.arch"));
    assert!(
        ws.iter()
            .all(|m| !m.contains("combinational feedback cycle")),
        "decoded connect router should not fabricate bus-wire comb-loop warnings: {ws:?}"
    );
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
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains("unknown TLM connect instance `missing`"),
        "expected unknown-instance diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_unknown_port_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains("module `Initiator` has no port `nope`"),
        "expected unknown-port diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_non_bus_port_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains("non-bus port `scalar`"),
        "expected non-bus-port diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_direction_mismatch_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains(
            "requires `connect initiator_inst.initiator_port -> target_inst.target_port;`"
        ) && msg.contains("t.s")
            && msg.contains("Target")
            && msg.contains("i.m")
            && msg.contains("Initiator"),
        "expected direction-mismatch diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_bus_mismatch_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains("TLM connect bus mismatch") && msg.contains("MemA") && msg.contains("MemB"),
        "expected bus-mismatch diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_bus_shape_mismatch_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
bus BusRw
  param READ: const = 1;
  param WRITE: const = 1;
  generate_if READ
    ar_valid: out Bool;
  end generate_if
  generate_if WRITE
    aw_valid: out Bool;
  end generate_if
end bus BusRw
use BusRw;
module Initiator
  port m: initiator BusRw<WRITE=0>;
end module Initiator
module Target
  port s: target BusRw<READ=0>;
end module Target
module Top
  inst i: Initiator
  end inst i
  inst t: Target
  end inst t
  connect i.m -> t.s;
end module Top
"#,
    );
    assert!(
        msg.contains("TLM connect bus-shape mismatch")
            && msg.contains("ar_valid")
            && msg.contains("aw_valid"),
        "expected bus-shape mismatch diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_duplicate_explicit_connection_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains("duplicates an explicit connection") && msg.contains("i.m"),
        "expected duplicate-explicit-connection diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_endpoint_reuse_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains("requires target inst `t0` to override `param SLAVE_START_ADDR = ...;`")
            && msg
                .contains("requires target inst `t1` to override `param SLAVE_START_ADDR = ...;`"),
        "expected missing-address-map-param diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_endpoint_reuse_after_generate_for_diagnostic() {
    let msg = tlm_connect_elaborate_error(
        r#"
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
"#,
    );
    assert!(
        msg.contains("requires target inst `t_0` to override `param SLAVE_START_ADDR = ...;`")
            && msg
                .contains("requires target inst `t_1` to override `param SLAVE_START_ADDR = ...;`"),
        "expected missing-address-map-param-after-generate diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_connect_decode_requires_exhaustive_ranges_or_default() {
    let msg = tlm_connect_elaborate_error(
        r#"
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
use Mem;
module Initiator
  port m: initiator Mem;
end module Initiator
module Target
  param SLAVE_START_ADDR: const = 0;
  param SLAVE_END_ADDR: const = 0;
  port s: target Mem;
end module Target
module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  inst i: Initiator
  end inst i
  inst t0: Target
    param SLAVE_START_ADDR = 32'h0000_0000;
    param SLAVE_END_ADDR = 32'h0000_ffff;
  end inst t0
  inst t1: Target
    param SLAVE_START_ADDR = 32'h8000_0000;
    param SLAVE_END_ADDR = 32'h8000_ffff;
  end inst t1
  connect i.m -> t0.s;
  connect i.m -> t1.s;
end module Top
"#,
    );
    assert!(
        msg.contains("requires literal ranges that cover the full decode address space"),
        "expected non-exhaustive decoded-connect diagnostic, got: {msg}"
    );
}

#[test]
fn test_tlm_one_initiator_many_targets_router_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmOneToMany.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("module TlmAddrRouter2"),
        "one-to-many TLM router should build:\n{sv}"
    );
    assert!(
        sv.contains("lo_read_req_valid = up_read_req_valid && read_to_hi == 1'b0")
            && sv.contains("hi_read_req_valid = up_read_req_valid && read_to_hi"),
        "router should decode request valid to exactly one downstream target:\n{sv}"
    );
    assert!(
        sv.contains("up_read_rsp_data = read_sel_hi ? hi_read_rsp_data : lo_read_rsp_data")
            && sv.contains(
                "up_write_rsp_data = write_sel_hi ? hi_write_rsp_data : lo_write_rsp_data"
            ),
        "router should mux responses through the latched request target:\n{sv}"
    );
    assert!(
        sv.contains("module TlmOneToManyTop")
            && sv.contains("cpu_link_read_req_valid")
            && sv.contains("lo_link_read_req_valid")
            && sv.contains("hi_link_read_req_valid"),
        "top should connect one initiator through router to two target links:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("class VTlmAddrRouter2") && sim.contains("class VTlmOneToManyTop"),
        "sim C++ should include router and top mirrors"
    );
}

#[test]
fn test_tlm_one_initiator_many_targets_ooo_router_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmOneToManyOoo.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("module TlmOooAddrRouter2"),
        "OOO one-to-many TLM router should build:\n{sv}"
    );
    assert!(
        sv.contains("logic [3:0] read_route_hi;")
            && sv.contains("read_route_hi[up_read_req_tag] <= read_to_hi"),
        "OOO router should record downstream route per upstream tag:\n{sv}"
    );
    assert!(
        sv.contains("lo_read_req_tag = up_read_req_tag")
            && sv.contains("hi_read_req_tag = up_read_req_tag"),
        "OOO router should forward upstream tags unchanged downstream:\n{sv}"
    );
    assert!(
        sv.contains("up_read_rsp_tag = choose_hi_rsp ? hi_read_rsp_tag : lo_read_rsp_tag")
            && sv.contains("hi_read_rsp_ready = up_read_rsp_ready && choose_hi_rsp"),
        "OOO router should mux responses by saved route and downstream response tag:\n{sv}"
    );
    assert!(
        sv.contains("module TlmOneToManyOooTop")
            && sv.contains("cpu_link_read_req_tag")
            && sv.contains("lo_link_read_req_tag")
            && sv.contains("hi_link_read_req_tag"),
        "OOO top should connect tag signals through one-to-many links:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("class VTlmOooAddrRouter2") && sim.contains("class VTlmOneToManyOooTop"),
        "sim C++ should include OOO router and top mirrors"
    );
}

#[test]
fn test_tlm_one_initiator_many_targets_response_router_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmOneToManyResp.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("module TlmAddrRouter4Resp"),
        "response-typed one-to-many TLM router should build:\n{sv}"
    );
    assert!(
        sv.contains("typedef struct packed") && sv.contains("MemResp64"),
        "struct response payload should be emitted:\n{sv}"
    );
    assert!(
        sv.contains("read_err_valid") && sv.contains("up_read_rsp_data = read_err_valid ?"),
        "router should synthesize its own decode-error response:\n{sv}"
    );
    assert!(
        sv.contains("s_read_rsp_data = {64'd1152921504606846976, 2'd0}")
            && sv.contains("up_read_rsp_data = read_err_valid ? {64'd0, 2'd1}"),
        "SV codegen should emit packed struct literals as iverilog-friendly concatenations:\n{sv}"
    );
    assert!(
        sv.contains("_auto_tlm_m_read_req_stable")
            && sv.contains("$stable(m_read_addr)")
            && sv.contains("_auto_tlm_m_read_rsp_stable")
            && sv.contains("$stable(m_read_rsp_data)"),
        "TLM protocol assertions should track request args and struct response payloads:\n{sv}"
    );
    assert!(
        sv.contains("t0_read_req_valid = up_read_req_valid && read_to_0")
            && sv.contains("t3_read_req_valid = up_read_req_valid && read_to_3"),
        "router should decode requests across four downstream targets:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("class VTlmAddrRouter4Resp") && sim.contains("MemResp64 up_read_rsp_data"),
        "sim C++ should include struct response router mirror:\n{sim}"
    );
    assert!(
        sim.contains("if (_let_read_mapped == 0)"),
        "sim C++ if conditions should not double-wrap comparison expressions:\n{sim}"
    );
    assert!(
        !sim.contains("if ((_let_read_mapped == 0))"),
        "sim C++ should avoid Clang -Wparentheses-equality noise:\n{sim}"
    );
}

#[test]
fn test_tlm_ooo_protocol_asserts_track_tags() {
    let source = include_str!("axi_dma_tlm/TlmOneToManyOoo.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("_auto_tlm_m_read_req_stable")
            && sv.contains("$stable(m_read_req_tag)")
            && sv.contains("$stable(m_read_addr)"),
        "OOO request assertion should track req_tag under backpressure:\n{sv}"
    );
    assert!(
        sv.contains("_auto_tlm_m_read_rsp_stable")
            && sv.contains("$stable(m_read_rsp_tag)")
            && sv.contains("$stable(m_read_rsp_data)"),
        "OOO response assertion should track rsp_tag under backpressure:\n{sv}"
    );
}

#[test]
fn test_tlm_connect_decode_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmConnectDecode.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_connect_decode.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for decoded connect");
    assert!(
        out.status.success(),
        "decoded connect sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS decoded connect"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_tlm_connect_decode_subtype_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmConnectSubtype.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_connect_subtype.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for decoded subtype connect");
    assert!(
        out.status.success(),
        "decoded subtype connect sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS decoded subtype connect"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_tlm_connect_decode_three_way_arch_sim_behavior() {
    // The two-target decode fixture exercises only a single-bit route register
    // and a two-way selector. A third target makes the synthesized
    // response-route register multi-bit (route_w == 2) and forces the middle
    // target through the priority-decode `raw && !any_previous` selector path.
    // Each target returns a distinct tag, so a mis-route surfaces as a wrong
    // value at the corresponding output port.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/axi_dma_tlm/TlmConnectDecodeThree.arch")
        .arg("--tb")
        .arg("tests/axi_dma_tlm/tb_tlm_connect_decode_three.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for 3-way decoded connect");
    assert!(
        out.status.success(),
        "3-way decoded connect sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS decoded connect 3way"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
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
    assert!(
        out.status.success(),
        "one-to-many router sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS one-to-many blocking"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
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
    assert!(
        out.status.success(),
        "OOO one-to-many router sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS one-to-many OOO"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
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
    assert!(
        out.status.success(),
        "response-typed one-to-many router sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS one-to-many response router"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
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
    assert!(
        result.is_err(),
        "reentrant thread syntax should no longer parse"
    );
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
    let m = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Module(m) if m.name.name == "M" => Some(m),
            _ => None,
        })
        .expect("module M");
    let t = m
        .body
        .iter()
        .find_map(|i| match i {
            arch::ast::ModuleBodyItem::Thread(t) => Some(t),
            _ => None,
        })
        .expect("thread");
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
    let m = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Module(m) if m.name.name == "T" => Some(m),
            _ => None,
        })
        .expect("module T");
    let t = m
        .body
        .iter()
        .find_map(|i| match i {
            arch::ast::ModuleBodyItem::Thread(t) => Some(t),
            _ => None,
        })
        .expect("thread");
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
    assert!(
        sv.contains("_tlm_s_read_state"),
        "state reg should appear in SV:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_read_addr_latched"),
        "arg latch reg should appear in SV:\n{sv}"
    );
    assert!(
        sv.contains("s_read_req_ready"),
        "req_ready driver should appear:\n{sv}"
    );
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
    assert!(
        r.is_err(),
        "non-indexed multi-implementer target should error"
    );
    let msg = format!("{:?}", r.unwrap_err());
    assert!(
        msg.contains("multi-implementer target") && msg.contains("s.read"),
        "expected targeted error, got: {msg}"
    );
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
    assert!(
        sv.contains("_tlm_s_read_tag0_state") && sv.contains("_tlm_s_read_tag3_state"),
        "indexed target lanes should lower to independent lane FSMs:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_read_tag0_req_ready") && sv.contains("_tlm_s_read_tag3_rsp_valid"),
        "indexed target lanes should use private endpoint wires:\n{sv}"
    );
    assert!(
        sv.contains("s_read_req_tag == 2'd0") && sv.contains("s_read_req_tag == 2'd3"),
        "shared target endpoint should route requests by tag lane:\n{sv}"
    );
    assert!(
        sv.contains("s_read_rsp_tag = _tlm_s_read_tag0_rsp_tag")
            && sv.contains("s_read_rsp_data = _tlm_s_read_tag0_rsp_data"),
        "shared response endpoint should mux lane responses:\n{sv}"
    );
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
    assert!(
        sv.contains("module _arb_MemTarget_read_rsp"),
        "response lock should synthesize a policy arbiter module:\n{sv}"
    );
    // Round-robin pointer-advance shape inside the synthesized
    // `_arb_MemTarget_read_rsp` arbiter module — uses the bare
    // `grant_requester` port. The bare `rr_ptr_r + 1` form was incorrect
    // for non-power-of-2 NUM_REQ.
    assert!(
        sv.contains("rr_ptr_r <= (grant_requester ==")
            && sv.contains("? '0 : grant_requester + 1'b1;"),
        "response arbiter should use the resource's round-robin policy:\n{sv}"
    );
    assert!(sv.contains("_tlm_s_read_rsp_arb_req_packed[0] = !_tlm_s_read_rsp_arb_hold_valid_r && _tlm_s_read_tag0_rsp_valid")
         && sv.contains("_tlm_s_read_rsp_arb_req_packed[3] = !_tlm_s_read_rsp_arb_hold_valid_r && _tlm_s_read_tag3_rsp_valid"),
        "lane response valids should feed the response arbiter:\n{sv}");
    assert!(
        sv.contains(
            "_tlm_s_read_rsp_arb_hold_idx_r == 2'd0 || _tlm_s_read_rsp_arb_grant_packed[0]"
        ) && sv.contains(
            "_tlm_s_read_rsp_arb_hold_idx_r == 2'd3 || _tlm_s_read_rsp_arb_grant_packed[3]"
        ),
        "shared response mux should be gated by the granted lane:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_read_rsp_arb_hold_valid_r <= 1'd1")
            && sv.contains("_tlm_s_read_rsp_arb_hold_idx_r <= _tlm_s_read_rsp_arb_grant_requester"),
        "backpressured response selection should be held stable:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_read_tag0_rsp_ready = s_read_rsp_ready"),
        "only the selected lane should receive shared response ready:\n{sv}"
    );
}

#[test]
fn test_axi_dma_tlm_indexed_burst_target_example_compiles() {
    let source = include_str!("axi_dma_tlm/TlmIndexedBurstTarget.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("module TlmIndexedBurstTarget"),
        "indexed burst target example should build:\n{sv}"
    );
    assert!(
        sv.contains("BoundedVecResp32x4 _tlm_s_read_burst_tag0_rsp_data"),
        "bounded Vec response should stay struct-typed through target lane lowering:\n{sv}"
    );
    assert!(
        sv.contains("s_read_burst_req_tag == 2'd3"),
        "generated target lanes should route by request tag:\n{sv}"
    );
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
    assert!(
        out.status.success(),
        "indexed burst target arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS indexed response arb"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_axi_dma_tlm_indexed_burst_target_verilator_behavior() {
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
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
        .arg("TlmIndexedBurstTarget")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_tlm/tb_tlm_indexed_burst_target.cpp")
        .output()
        .expect("verilate indexed burst target");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VTlmIndexedBurstTarget");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator indexed burst target");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS indexed response arb"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
}

#[test]
fn test_axi_read_beat_interleave_example_compiles() {
    let source = include_str!("axi_dma_thread/ThreadAxiReadBeatInterleave.arch");
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("module ThreadAxiReadBeatInterleave"),
        "beat-interleaving thread example should build:\n{sv}"
    );
    assert!(
        sv.contains("_arb_ThreadAxiReadBeatInterleave_r_ch"),
        "response channel mutex should lower to a generated arbiter:\n{sv}"
    );
    assert!(
        sv.contains("r_id = 1"),
        "generate_for lanes should become concrete response IDs:\n{sv}"
    );
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
    assert!(
        out.status.success(),
        "beat-interleaving arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS beat interleave alternating"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_axi_read_beat_interleave_verilator_behavior() {
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
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
        .arg("ThreadAxiReadBeatInterleave")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_thread/tb_axi_read_beat_interleave.cpp")
        .output()
        .expect("verilate beat-interleaving response target");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VThreadAxiReadBeatInterleave");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator beat-interleaving response target");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS beat interleave alternating"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
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
    assert!(
        sv.contains("_tlm_init_driver_state"),
        "single-implementer initiator should use v1 inline lowering:\n{sv}"
    );
    assert!(
        sv.contains("m_read_req_valid") && sv.contains("m_read_rsp_ready"),
        "bus signals should be driven:\n{sv}"
    );
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
    assert!(
        sv.contains("_tlm_init_w0_state") && sv.contains("_tlm_init_w1_state"),
        "multi-implementer initiator should lower both workers:\n{sv}"
    );
    assert!(
        sv.contains("m_read_req_valid") && sv.contains("m_read_rsp_ready"),
        "shared method driver should still drive req/rsp handshake:\n{sv}"
    );
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
    assert!(
        r.is_err(),
        "initiator implement with args should be a parse error"
    );
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
    assert!(
        sv.contains("_tlm_pool_m_read_fifo"),
        "cohort lowering should emit issue-order FIFO:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_pool_m_read_t0_state") && sv.contains("_tlm_pool_m_read_t1_state"),
        "cohort lowering should emit per-thread state regs:\n{sv}"
    );
    assert!(
        sv.contains("m_read_req_valid") && sv.contains("m_read_rsp_ready"),
        "cohort lowering should drive shared TLM handshakes:\n{sv}"
    );
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
    assert!(
        sv.contains("_tlm_pool_m_read_fifo"),
        "generated worker cohort should use pooled TLM lowering:\n{sv}"
    );
    assert!(
        sv.contains("data[0] <= m_read_rsp_data")
            && sv.contains("data[1] <= m_read_rsp_data")
            && sv.contains("data[2] <= m_read_rsp_data"),
        "each generated worker should capture its routed response:\n{sv}"
    );
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
    assert!(
        sv.contains("_tlm_pool_m_read_fifo"),
        "fork/join TLM workers should use pooled TLM lowering:\n{sv}"
    );
    assert!(
        sv.contains("data[0] <= m_read_rsp_data") && sv.contains("data[1] <= m_read_rsp_data"),
        "fork/join worker responses should route by issue order:\n{sv}"
    );
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
    assert!(
        sv.contains("_tlm_fork_workers_m_read_age"),
        "forked RHS TLM lowering should emit an issue-age counter:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_fork_workers_m_read_fifo"),
        "blocking forked RHS TLM should route responses by issue-order FIFO:\n{sv}"
    );
    assert!(
        sv.contains("data[0] <= m_read_rsp_data") && sv.contains("data[1] <= m_read_rsp_data"),
        "forked RHS worker responses should capture routed data:\n{sv}"
    );
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
    assert!(
        msg.contains("join all"),
        "expected join-all diagnostic, got: {msg}"
    );
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
    assert!(
        sv.contains("_tlm_pool_m_read_fifo"),
        "multi-arg method should use pooled TLM lowering:\n{sv}"
    );
    assert!(
        sv.contains("m_read_addr") && sv.contains("m_read_len"),
        "cohort lowering should mux every method arg:\n{sv}"
    );
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
    assert!(
        msg.contains("multi-thread sharing") || msg.contains("TLM initiator thread body"),
        "expected targeted TLM fork/join error, got: {msg}"
    );
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
    assert!(
        sv.contains("m_read_req_tag"),
        "out-of-order cohort should drive request tags:\n{sv}"
    );
    assert!(
        sv.contains("m_read_rsp_tag == 2'd0") && sv.contains("m_read_rsp_tag == 2'd1"),
        "out-of-order cohort should route responses by rsp_tag:\n{sv}"
    );
    assert!(
        sv.contains("data[0] <= m_read_rsp_data") && sv.contains("data[1] <= m_read_rsp_data"),
        "tag-routed responses should capture into each worker destination:\n{sv}"
    );
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
    assert!(
        sv.contains("m_read_req_tag") && sv.contains("m_read_rsp_tag"),
        "OOO forked RHS TLM should drive and consume tag wires:\n{sv}"
    );
    assert!(
        sv.contains("m_read_rsp_tag == 2'd0") && sv.contains("m_read_rsp_tag == 2'd1"),
        "OOO forked RHS responses should route by worker tag:\n{sv}"
    );
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
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
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
    assert!(
        sv.contains("mem_read_burst_rsp_tag == 2'd0")
            && sv.contains("mem_read_burst_rsp_tag == 2'd1"),
        "burst Vec OOO responses should route by worker tag:\n{sv}"
    );

    let sim = compile_to_sim_h(source, false);
    assert!(
        sim.contains("uint32_t mem_read_burst_rsp_data[4];")
            && sim.contains("uint32_t& mem_read_burst_rsp_data_0;"),
        "sim API should preserve Vec response lanes as an array with flat aliases:\n{sim}"
    );
    assert!(
        sim.contains("uint32_t _mem_read_burst_rsp_data[4];"),
        "sim model should mirror the flattened Vec response as an internal array:\n{sim}"
    );
    assert!(
        sim.contains(
            "for (size_t _i = 0; _i < 4; ++_i) { _n_burst0_r[_i] = _mem_read_burst_rsp_data[_i]; }"
        ) && sim.contains(
            "for (size_t _i = 0; _i < 4; ++_i) { _n_burst1_r[_i] = _mem_read_burst_rsp_data[_i]; }"
        ),
        "burst Vec responses should copy into both destination arrays:\n{sim}"
    );
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
    assert!(
        sv.contains("_tlm_s_read_tag_latched"),
        "target should latch accepted request tag:\n{sv}"
    );
    assert!(
        sv.contains("s_read_rsp_tag = _tlm_s_read_tag_latched"),
        "target should echo the latched tag on response:\n{sv}"
    );
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
    assert!(
        msg.contains("direct right-hand side") || msg.contains("direct"),
        "expected direct-RHS error, got: {msg}"
    );
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
    assert!(
        out.status.success(),
        "conditional TLM initiator arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS TlmConditionalInitiator"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_tlm_conditional_initiator_verilator_behavior() {
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
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
        .arg("TlmConditionalInitiator")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/axi_dma_tlm/tb_tlm_conditional_initiator.cpp")
        .output()
        .expect("verilate conditional TLM initiator");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VTlmConditionalInitiator");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator conditional TLM initiator");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS TlmConditionalInitiator"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
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
        sv.contains("assign hbm_read_k_req_valid") && sv.contains("assign qk_qk_tile_req_valid"),
        "runtime loop should still drive both serialized TLM request channels:\n{sv}"
    );
}

#[test]
fn test_thread_loop_const_bit_select_folds_after_unroll() {
    // Regression for issue #444: TLM initiator lowering unrolls literal
    // thread `for` loops by substituting the loop variable with a literal.
    // A single-bit select on that variable (`kv_group[0]`) used to survive
    // as invalid SV (`0[0]`, `1[0]`, ...). Literal bit-selects must fold
    // to valid sized constants.
    let source = include_str!(
        "regression/issues/thread_loop_const_bit_select/ThreadLoopConstBitSelect.arch"
    );
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("module ThreadLoopConstBitSelect"),
        "expected issue #444 regression module:\n{sv}"
    );
    for i in 0..=3 {
        let bad = format!("{i}[0] == 1'd0");
        assert!(
            !sv.contains(&bad),
            "literal bit-select should have folded instead of emitting `{bad}`:\n{sv}",
        );
    }
    assert!(
        sv.contains("1'd0 == 1'd0") && sv.contains("1'd1 == 1'd0"),
        "expected folded single-bit constants in the unrolled branch conditions:\n{sv}",
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
    assert!(
        out.status.success(),
        "FPT26 runtime-loop TLM arch sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS Fpt26RuntimeLoopTlm"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_fpt26_runtime_loop_tlm_verilator_behavior() {
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
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
        .arg("Fpt26RuntimeLoopTlm")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("tests/fpt26_tlm/tb_fpt26_runtime_loop_tlm.cpp")
        .output()
        .expect("verilate FPT26 runtime-loop TLM");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let exe = obj_dir.join("VFpt26RuntimeLoopTlm");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run Verilator FPT26 runtime-loop TLM");
    assert!(
        run.status.success(),
        "Verilator sim should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS Fpt26RuntimeLoopTlm"),
        "expected PASS marker in Verilator stdout:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
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
    assert!(
        sv.contains("_tlm_s_read_state"),
        "state register should appear in SV:\n{sv}"
    );
    assert!(
        sv.contains("_tlm_s_read_addr_latched"),
        "arg latch reg should appear in SV:\n{sv}"
    );
    assert!(
        sv.contains("s_read_req_ready"),
        "req_ready driver should appear in SV:\n{sv}"
    );
    assert!(
        sv.contains("s_read_rsp_valid"),
        "rsp_valid driver should appear in SV:\n{sv}"
    );
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
    assert!(
        sv.contains("_tlm_s_read_wait_cnt"),
        "wait-cycle target should allocate a counter:\n{sv}"
    );
    assert!(
        sv.contains("32'd6"),
        "wait 7 cycle should initialize the counter to 6:\n{sv}"
    );
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
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
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
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
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
    let m = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Module(m) if m.name.name == "MemTarget" => Some(m),
            _ => None,
        })
        .expect("MemTarget in AST");
    let t = m
        .body
        .iter()
        .find_map(|i| match i {
            arch::ast::ModuleBodyItem::Thread(t) => Some(t),
            _ => None,
        })
        .expect("thread in body");
    let has_return = t
        .body
        .iter()
        .any(|s| matches!(s, arch::ast::ThreadStmt::Return(_, _)));
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
    assert!(
        msg.contains("return") && msg.contains("TLM method target thread"),
        "expected targeted error, got: {msg}"
    );
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
    assert!(
        parser.parse_source_file().is_err(),
        "mismatched closing method name should be a parse error"
    );
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
    assert!(
        sv.contains("input logic s_read_req_valid"),
        "target perspective: req_valid should be an input:\n{sv}"
    );
    assert!(
        sv.contains("output logic s_read_req_ready"),
        "target perspective: req_ready flows back as output:\n{sv}"
    );
    assert!(
        sv.contains("output logic s_read_rsp_valid"),
        "target perspective: rsp_valid is output:\n{sv}"
    );
    assert!(
        sv.contains("output logic [63:0] s_read_rsp_data"),
        "target perspective: rsp_data is output:\n{sv}"
    );
    assert!(
        sv.contains("input logic s_read_rsp_ready"),
        "target perspective: rsp_ready flows back as input:\n{sv}"
    );
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
        assert!(
            result.is_err(),
            "mode `{mode}` should be rejected in v1: source={source}"
        );
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
    assert!(
        parser.parse_source_file().is_err(),
        "mismatched credit_channel close should be a parse error"
    );
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
    assert!(
        result.is_err(),
        "expected parse error for mismatched opening/closing keyword"
    );
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
    assert!(
        sv.contains("$past(b, 2)"),
        "past(b, 2) should emit SV $past(b, 2):\n{sv}"
    );
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
    assert!(
        sv.contains("a |=> b"),
        "a |=> b should emit SV a |=> b:\n{sv}"
    );
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
    assert!(
        errs.iter().any(|e| format!("{e:?}").contains("past")),
        "error should mention past: {errs:?}"
    );
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
    assert!(
        checker.check().is_err(),
        "past with wrong arity should error"
    );
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
    assert!(
        sv.contains("##2 b") || sv.contains("##2b"),
        "expected ##2 in SV:\n{sv}"
    );
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
    assert!(
        checker.check().is_err(),
        "rose() outside assert should be rejected"
    );
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
    assert!(
        checker.check().is_err(),
        "##N outside assert should be rejected"
    );
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
    assert!(
        sv.contains(".DATA_WIDTH(W)"),
        "type override `T = UInt<W>` should emit `.DATA_WIDTH(W)`:\n{sv}"
    );
    assert!(
        sv.contains(".DEPTH(4)"),
        "value override `DEPTH = 4` should emit `.DEPTH(4)`:\n{sv}"
    );
    // Sanity: no `.T(...)` raw type in the inst — the fifo doesn't expose `T` at SV level.
    assert!(
        !sv.contains(".T(logic"),
        "should not emit raw `.T(logic ...)` for fifo whose T was synthesized to DATA_WIDTH:\n{sv}"
    );
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
    assert!(
        sv.contains("assign o0 = a;"),
        "q@0 should be source `a`:\n{sv}"
    );
    assert!(
        sv.contains("assign o1 = q_stg1;"),
        "q@1 should be q_stg1:\n{sv}"
    );
    assert!(
        sv.contains("assign o2 = q_stg2;"),
        "q@2 should be q_stg2:\n{sv}"
    );
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
    assert!(
        errs.iter()
            .any(|e| format!("{e:?}").contains("exceeds pipe_reg depth")),
        "error should mention depth: {errs:?}"
    );
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
    assert!(
        sv.contains("a >>> 1"),
        "fsm-scope SInt port `a` shifted right should emit `>>>` (arithmetic):\n{sv}"
    );
    assert!(
        !sv.contains("a >> 1"),
        "should not emit `>>` (logical) for SInt:\n{sv}"
    );
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
    assert!(
        sv.contains("parameter logic [(4)*(8)-1:0] coeffs"),
        "expected packed parameter:\n{sv}"
    );
    // Default packed in reverse so coeffs[0] = parts[0] = 1 (LSB).
    assert!(
        sv.contains("(8)'(4), (8)'(3), (8)'(2), (8)'(1)"),
        "expected reversed default chunks (MSB-first packing):\n{sv}"
    );
    // Indexing rewritten to part-select.
    assert!(
        sv.contains("coeffs[(0) * (8) +: (8)]"),
        "expected coeffs[0] → coeffs[(0) * (8) +: (8)]:\n{sv}"
    );
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
    assert!(
        !sv.contains("for ("),
        "should not synthesize a for-loop bit unpack:\n{sv}"
    );
    // Should index `d[i]` directly in the priority encoder.
    assert!(
        sv.contains("d[0]") && sv.contains("d[7]"),
        "expected direct bit indexing of `d`:\n{sv}"
    );
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
    assert!(
        sv.contains("count_r == max"),
        "expected wrap compare against `max` port:\n{sv}"
    );
    // at_max output mirrors the same compare.
    assert!(
        sv.contains("assign at_max = (count_r == max)"),
        "expected at_max against `max` port:\n{sv}"
    );
    // No const MAX appears (no MAX param declared).
    assert!(
        !sv.contains("'(MAX)"),
        "should not emit const MAX comparator when port is present:\n{sv}"
    );
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
    assert!(
        !sv.contains("_t0_cnt <= 32'(_t0_cnt - 32'd1);"),
        "should not emit counter-decrement state for `wait 1 cycle`:\n{sv}"
    );
    // Three phase writes appear, each transitioning to the next state.
    // State numbering after elision: 0=initial wait, 1=phase=1,
    // 2=phase=2, 3=phase=3, then loop back to 0. Issue #247 changed
    // state assignments to reference per-state `localparam` names
    // (`_t0_S<N>_<role>`) instead of bare numeric literals.
    assert!(
        sv.contains("phase <= 2'd1;")
            && sv.contains("phase <= 2'd2;")
            && sv.contains("phase <= 2'd3;"),
        "expected three phase writes:\n{sv}"
    );
    assert!(
        sv.contains("_t0_state <= _t0_S2_action;") && sv.contains("_t0_state <= _t0_S3_action;"),
        "expected state transitions 1->2 and 2->3 via state-name localparams:\n{sv}"
    );
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
    assert!(
        sv.contains("module _M_threads"),
        "thread with wait-1-cycle in else branch should compile:\n{sv}"
    );
}

#[test]
fn test_lowered_threads_codegen_has_no_extra_blank_separator_or_eof_blank() {
    // Regression: `arch build` briefly emitted `endmodule\n\nmodule M`
    // between the synthetic `_M_threads` helper and the public wrapper,
    // plus `endmodule\n\n` at EOF. That produced blank-line-only churn in
    // downstream generated SV such as fpt26 AttentionTileShell.sv.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port go: in Bool;
          port done: out Bool;
          thread on clk rising, rst high
            wait until go;
            done = 1;
            wait 1 cycle;
          end thread
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("module _M_threads"),
        "expected lowered helper:\n{sv}"
    );
    assert!(
        sv.contains("endmodule\nmodule M"),
        "expected tight helper/public boundary:\n{sv}"
    );
    assert!(
        !sv.contains("endmodule\n\nmodule M"),
        "helper/public boundary must not contain a blank separator:\n{sv}"
    );
    assert!(
        sv.ends_with("endmodule\n") && !sv.ends_with("endmodule\n\n"),
        "generated SV must end with one newline after final endmodule:\n{sv:?}"
    );
}

// ── Auto-emitted SVA from thread lowering ─────────────────────────────────────

#[test]
fn test_auto_thread_asserts_off_by_default() {
    let source = include_str!("../tests/thread/wait_cycles.arch");
    let sv = compile_to_sv(source);
    assert!(
        !sv.contains("_auto_thread_"),
        "default lowering must not emit auto-thread asserts:\n{sv}"
    );
}

#[test]
fn test_auto_thread_asserts_wait_cycles_and_until() {
    // DelayPulse thread covers both wait_until (state 0: `wait until start`)
    // and wait N cycle (states 1 and 3). Verify both property classes
    // emit, wrapped in `synopsys translate_off/on`, with reset-guarded
    // antecedents.
    let source = include_str!("../tests/thread/wait_cycles.arch");
    let opts = elaborate::ThreadLowerOpts {
        auto_asserts: true,
        ..Default::default()
    };
    let sv = compile_to_sv_with_opts(source, &opts);

    // Wait-until: state 0 transitions on `start`. Issue #247 changed
    // state comparisons in auto-asserts to reference per-state
    // `localparam` names (`_t0_S<N>_<role>`) instead of bare literals.
    assert!(
        sv.contains("_auto_thread_t0_wait_until_s0:"),
        "expected wait_until property at state 0:\n{sv}"
    );
    assert!(
        sv.contains("|=> _t0_state == _t0_S1_wait_cycles"),
        "expected next-cycle implication to state 1 (wait_cycles) via state-name localparam:\n{sv}"
    );

    // Wait-cycles: stay + done assertions.
    assert!(
        sv.contains("_auto_thread_t0_wait_stay_s1:"),
        "expected wait-cycles stay assertion:\n{sv}"
    );
    assert!(
        sv.contains("_auto_thread_t0_wait_done_s1:"),
        "expected wait-cycles done assertion:\n{sv}"
    );

    // Reset guard: rst_n is active-low, so `not_in_reset == rst_n`.
    assert!(
        sv.contains("rst_n &&"),
        "expected reset guard `rst_n && ...` in antecedent:\n{sv}"
    );

    // SVA wrapped in translate_off/on (so synth ignores it).
    assert!(
        sv.contains("// synopsys translate_off"),
        "expected translate_off wrapping:\n{sv}"
    );
    assert!(
        sv.contains("// synopsys translate_on"),
        "expected translate_on wrapping:\n{sv}"
    );
}

#[test]
fn test_auto_thread_asserts_fork_join_branches() {
    // fork/join produces multi_transitions. Each branch transition gets
    // an `_auto_thread_t{i}_branch_s{s}_b{b}` assertion.
    let source = include_str!("../tests/thread/fork_join.arch");
    let opts = elaborate::ThreadLowerOpts {
        auto_asserts: true,
        ..Default::default()
    };
    let sv = compile_to_sv_with_opts(source, &opts);
    assert!(
        sv.contains("_auto_thread_t0_branch_s"),
        "expected at least one fork/join branch assertion:\n{sv}"
    );
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
    let opts = elaborate::ThreadLowerOpts {
        auto_asserts: true,
        ..Default::default()
    };
    let sv = compile_to_sv_with_opts(source, &opts);
    assert!(
        sv.contains("!rst &&"),
        "expected `!rst` guard for active-high reset:\n{sv}"
    );
    assert!(
        !sv.contains("(rst) &&"),
        "should not use bare `rst` as guard for active-high:\n{sv}"
    );
}

// ── Thread map HTML sidecar ──────────────────────────────────────────────────

fn thread_map_smoke_source(module_name: &str) -> String {
    format!(
        r#"
        module {module_name}
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port done: out Bool;
          thread T on clk rising, rst low
            done = 0;
            wait until go;
            done = 1;
            wait 2 cycle;
          end thread T
        end module {module_name}
    "#
    )
}

fn thread_map_control_flow_source(module_name: &str) -> String {
    format!(
        r#"
        module {module_name}
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port sel:  in Bool;
          port ack:  in Bool;
          port done: out Bool;
          thread T on clk rising, rst low
            wait until go;
            if sel
              wait until ack;
            else
              wait 2 cycle;
            end if
            done = 1;
            wait 1 cycle;
          end thread T
        end module {module_name}
    "#
    )
}

fn thread_proof_fold_source(module_name: &str) -> String {
    format!(
        r#"
        module {module_name}
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port done: out Bool;
          reg done_r: Bool reset rst => false;
          thread T on clk rising, rst low
            done_r <= false;
            wait until go;
            done_r <= true;
            wait 2 cycle;
            done_r <= false;
          end thread T
          comb
            done = done_r;
          end comb
        end module {module_name}
    "#
    )
}

fn thread_proof_multi_seq_source(module_name: &str) -> String {
    format!(
        r#"
        module {module_name}
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port a: out Bool;
          port b: out Bool;
          reg a_r: Bool reset rst => false;
          reg b_r: Bool reset rst => false;
          thread T on clk rising, rst low
            a_r <= true;
            b_r <= false;
            wait until go;
            a_r <= false;
            b_r <= true;
            wait 1 cycle;
          end thread T
          comb
            a = a_r;
            b = b_r;
          end comb
        end module {module_name}
    "#
    )
}

fn thread_proof_once_source(module_name: &str) -> String {
    format!(
        r#"
        module {module_name}
          port clk:  in Clock<SysDomain>;
          port rst:  in Reset<Async, Low>;
          port go:   in Bool;
          port done: out Bool;
          reg done_r: Bool reset rst => false;
          thread once T on clk rising, rst low
            wait until go;
            wait 2 cycle;
            done_r <= true;
          end thread T
          comb
            done = done_r;
          end comb
        end module {module_name}
    "#
    )
}

fn thread_proof_once_folded_terminal_source(module_name: &str) -> String {
    format!(
        r#"
        module {module_name}
          port clk:       in Clock<SysDomain>;
          port rst:       in Reset<Async, Low>;
          port go:        in Bool;
          port start:     out Bool;
          port done:      out Bool;
          reg done_r: Bool reset rst => false;
          thread once T on clk rising, rst low
            start = true;
            wait until go;
            start = false;
            done_r <= true;
          end thread T
          comb
            done = done_r;
          end comb
        end module {module_name}
    "#
    )
}

#[test]
fn test_build_emit_thread_map_bare_path() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("Flow.arch");
    let sv_out = td.path().join("Flow.sv");
    let html_out = td.path().join("Flow.thread.html");
    std::fs::write(&src, thread_map_smoke_source("Flow")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-map")
        .output()
        .expect("run arch build --emit-thread-map");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        html_out.exists(),
        "expected bare flag to write {}",
        html_out.display()
    );
    let html = std::fs::read_to_string(&html_out).expect("read thread map");
    assert!(
        html.contains("_t0_S0_wait_until"),
        "expected generated state name in HTML:\n{html}"
    );
    assert!(
        html.contains("thread-flow-chart"),
        "expected flow chart in HTML:\n{html}"
    );
    assert!(
        html.contains("wait until go"),
        "expected source line in HTML:\n{html}"
    );
    assert!(
        html.contains("done = 1"),
        "expected source assignment in HTML:\n{html}"
    );
}

#[test]
fn test_build_emit_thread_map_explicit_path() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("Flow.arch");
    let sv_out = td.path().join("Flow.sv");
    let html_out = td.path().join("custom_map.html");
    std::fs::write(&src, thread_map_smoke_source("Flow")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg(format!("--emit-thread-map={}", html_out.display()))
        .output()
        .expect("run arch build --emit-thread-map=path");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        html_out.exists(),
        "expected explicit map path to be written"
    );
}

#[test]
fn test_build_emit_thread_proof_records_folded_target() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofFlow.arch");
    let sv_out = td.path().join("ProofFlow.sv");
    let proof_out = td.path().join("ProofFlow.thread-proof.json");
    std::fs::write(&src, thread_proof_fold_source("ProofFlow")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof")
        .output()
        .expect("run arch build --emit-thread-proof");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json = std::fs::read_to_string(&proof_out).expect("read proof certificate");
    assert!(
        json.contains("\"schema\": \"arch.thread_lowering_proof.v5\""),
        "expected schema in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"state_name\": \"_t0_S1_wait_until\""),
        "expected wait_until state in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"target_index\": 3"),
        "folded wait state should target the emitted wait_cycles state:\n{json}"
    );
    assert!(
        json.contains("\"source_next_index\": 3"),
        "folded wait source-next should skip the absorbed action state:\n{json}"
    );
    assert!(
        json.contains("\"source_next_name\": \"_t0_S3_wait_cycles\""),
        "folded wait source-next name should identify the emitted wait state:\n{json}"
    );
    assert!(
        json.contains("\"state_name\": \"_t0_S2_action\""),
        "expected absorbed action state to remain documented:\n{json}"
    );
    assert!(
        json.contains("\"emitted\": false"),
        "expected absorbed action state to be marked non-emitted:\n{json}"
    );
    assert!(
        json.contains("\"wait_cycles_count\": \"2\""),
        "expected structured wait-cycle count in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"seq_updates\": [\"done_r <= false\"]"),
        "expected entry state's ordinary update in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"folded_exit_updates\": [\"done_r <= true\"]"),
        "expected folded wait-exit update in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"seq_assignments\": [{\"target\": \"done_r\", \"value\": \"false\"}]"),
        "expected structured ordinary assignment in proof certificate:\n{json}"
    );
    assert!(
        json.contains(
            "\"folded_exit_assignments\": [{\"target\": \"done_r\", \"value\": \"true\"}]"
        ),
        "expected structured folded assignment in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"source_transitions\": [{\"condition\": \"go\""),
        "expected compacted pre-fold source transition in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"condition_guard\": {\"kind\":\"atom\",\"name\":\"go\"}"),
        "expected structured source transition guard in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"target_index\": 3")
            && json.contains("\"target_name\": \"_t0_S3_wait_cycles\""),
        "expected compacted pre-fold source transition target in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"source_transition_origin\": \"pre_fold_snapshot\""),
        "expected source transition provenance in proof certificate:\n{json}"
    );

    let sv = std::fs::read_to_string(&sv_out).expect("read sv");
    assert!(
        sv.contains("_t0_state <= _t0_S3_wait_cycles"),
        "SV should skip folded S2 and jump to S3:\n{sv}"
    );
}

#[test]
fn test_build_emit_thread_proof_lean_proves_multi_assignment_store_effects() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofMulti.arch");
    let sv_out = td.path().join("ProofMulti.sv");
    let lean_out = td.path().join("ProofMulti.thread-proof.lean");
    std::fs::write(&src, thread_proof_multi_seq_source("ProofMulti")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof-lean")
        .output()
        .expect("run arch build --emit-thread-proof-lean");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let lean = std::fs::read_to_string(&lean_out).expect("read Lean proof certificate");
    assert!(
        lean.contains("[Arch.ThreadLoweringProof.FoldedExit.setVar 0 0, Arch.ThreadLoweringProof.FoldedExit.setVar 1 1]"),
        "Lean artifact should model both assignments in one action update list:\n{lean}"
    );
    assert!(
        lean.contains("Arch.ThreadLoweringProof.FoldedExit.applyUpdates ProofMulti_T_0__t0_S0_entry_seq_0Updates store 0 = 0"),
        "Lean artifact should prove the first assignment's final store effect:\n{lean}"
    );
    assert!(
        lean.contains("Arch.ThreadLoweringProof.FoldedExit.applyUpdates ProofMulti_T_0__t0_S0_entry_seq_0Updates store 1 = 1"),
        "Lean artifact should prove the second assignment's final store effect:\n{lean}"
    );
    assert!(
        lean.contains("((Arch.ThreadLoweringProof.FoldedExit.sourceStep ProofMulti_T_0__t0_S1_wait_until_folded_0Source env natEnv cfg).store 0 = 1)")
            && lean.contains("((Arch.ThreadLoweringProof.FoldedExit.sourceStep ProofMulti_T_0__t0_S1_wait_until_folded_0Source env natEnv cfg).store 1 = 0)"),
        "Lean artifact should prove folded multi-assignment source store effects:\n{lean}"
    );
    assert!(
        lean.contains("((Arch.ThreadLoweringProof.FoldedExit.fsmStep ProofMulti_T_0__t0_S1_wait_until_folded_0Fsm env natEnv cfg).store 0 = 1)")
            && lean.contains("((Arch.ThreadLoweringProof.FoldedExit.fsmStep ProofMulti_T_0__t0_S1_wait_until_folded_0Fsm env natEnv cfg).store 1 = 0)"),
        "Lean artifact should prove folded multi-assignment FSM store effects:\n{lean}"
    );
}

#[test]
fn test_build_emit_thread_proof_lean_records_replay_artifact() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofFlow.arch");
    let sv_out = td.path().join("ProofFlow.sv");
    let lean_out = td.path().join("ProofFlow.thread-proof.lean");
    std::fs::write(&src, thread_proof_fold_source("ProofFlow")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof-lean")
        .output()
        .expect("run arch build --emit-thread-proof-lean");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        lean_out.exists(),
        "expected bare flag to write {}",
        lean_out.display()
    );

    let lean = std::fs::read_to_string(&lean_out).expect("read Lean proof certificate");
    assert!(
        lean.contains("import ArchThreadLoweringProof.CountedWait"),
        "Lean artifact should import the CountedWait model:\n{lean}"
    );
    assert!(
        lean.contains(
            "forall t, sourceTraceObs ProofFlow_T_0Source inputs natInputs cfg0 t = fsmTraceObs ProofFlow_T_0Fsm inputs natInputs cfg0 t"
        ),
        "Lean artifact should include an unbounded trace-equivalence theorem:\n{lean}"
    );
    assert!(
        lean.contains("example : StepEffectFaithful ProofFlow_T_0Source ProofFlow_T_0Fsm"),
        "Lean artifact should prove one-step generated FSM state effects match the source thread:\n{lean}"
    );
    assert!(
        lean.contains("Control.waitUntil (GuardExpr.atom 0)"),
        "Lean artifact should carry structured CountedWait guard expressions:\n{lean}"
    );
    assert!(
        lean.contains("Arch.ThreadLoweringProof.FoldedExit.Control.waitUntil (Arch.ThreadLoweringProof.FoldedExit.GuardExpr.atom 0)"),
        "Lean artifact should carry structured FoldedExit guard expressions:\n{lean}"
    );
    assert!(
        lean.contains("ProofFlow_T_0__t0_S0_entry_seq_0Updates"),
        "Lean artifact should define ordinary seq-assignment update proofs:\n{lean}"
    );
    assert!(
        lean.contains("Arch.ThreadLoweringProof.FoldedExit.applyUpdates ProofFlow_T_0__t0_S0_entry_seq_0Updates store 0 = 0"),
        "Lean artifact should prove the ordinary assignment's store effect:\n{lean}"
    );
    assert!(
        !lean.contains("Control.waitUntil 0"),
        "Lean artifact should not use opaque numeric guard IDs in controls:\n{lean}"
    );
}

#[test]
fn test_build_check_thread_proof_lean_missing_project_fails_cleanly() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofFlow.arch");
    let sv_out = td.path().join("ProofFlow.sv");
    let missing_project = td.path().join("missing_lean_project");
    std::fs::write(&src, thread_proof_fold_source("ProofFlow")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--check-thread-proof-lean")
        .arg(format!(
            "--thread-proof-lean-project={}",
            missing_project.display()
        ))
        .output()
        .expect("run arch build --check-thread-proof-lean");
    assert!(
        !out.status.success(),
        "build should fail when Lean project is missing\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Lean thread proof project not found"),
        "expected clean missing-project diagnostic, got:\n{stderr}"
    );
    assert!(
        td.path().join("ProofFlow.thread-proof.lean").exists(),
        "check mode should still emit the replay artifact before checking"
    );
}

#[test]
fn test_formal_emit_thread_proof_lean_writes_artifact_before_smt_run() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofEq.arch");
    let proof_out = td.path().join("ProofEq.thread-proof.lean");
    std::fs::write(
        &src,
        r#"
module ProofEq
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port idx: in UInt<3>;
  port done: out pipe_reg<Bool, 1> reset rst => false;

  thread T on clk rising, rst high
    wait until idx == 2;
    wait until idx != 1;
    done <= true;
  end thread T
end module ProofEq
"#,
    )
    .expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("formal")
        .arg(&src)
        .arg("--bound")
        .arg("2")
        .arg("--timeout")
        .arg("5")
        .arg(format!(
            "--emit-thread-proof-lean={}",
            proof_out.to_string_lossy()
        ))
        .output()
        .expect("run arch formal --emit-thread-proof-lean");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!("Wrote {}", proof_out.display())),
        "formal command should write the Lean proof before SMT handoff\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        stderr
    );
    let lean = std::fs::read_to_string(&proof_out).expect("read Lean proof");
    assert!(
        lean.contains("GuardExpr.eq") && lean.contains("GuardExpr.ne"),
        "formal-emitted Lean proof should preserve structured equality guards:\n{lean}"
    );
}

#[test]
fn test_formal_thread_proof_only_skips_smt_backend() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofEq.arch");
    let proof_out = td.path().join("ProofEq.thread-proof.lean");
    std::fs::write(
        &src,
        r#"
module ProofEq
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port idx: in UInt<3>;
  port done: out pipe_reg<Bool, 1> reset rst => false;

  thread T on clk rising, rst high
    wait until idx == 2;
    wait until idx != 1;
    done <= true;
  end thread T
end module ProofEq
"#,
    )
    .expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("formal")
        .arg(&src)
        .arg(format!(
            "--emit-thread-proof-lean={}",
            proof_out.to_string_lossy()
        ))
        .arg("--thread-proof-only")
        .output()
        .expect("run arch formal --thread-proof-only");

    assert!(
        out.status.success(),
        "thread-proof-only should skip the SMT backend\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("unknown identifier"),
        "thread-proof-only should not reach the current SMT thread limitation:\n{stderr}"
    );
    let lean = std::fs::read_to_string(&proof_out).expect("read Lean proof");
    assert!(
        lean.contains("GuardExpr.eq") && lean.contains("GuardExpr.ne"),
        "proof-only formal mode should emit the Lean certificate:\n{lean}"
    );
}

#[test]
fn test_formal_thread_proof_only_requires_lean_output() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("Plain.arch");
    std::fs::write(
        &src,
        r#"
module Plain
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<4>;
  port b: out pipe_reg<UInt<4>, 1> reset rst => 0;

  seq on clk rising
    b <= a;
  end seq
end module Plain
"#,
    )
    .expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("formal")
        .arg(&src)
        .arg("--thread-proof-only")
        .output()
        .expect("run arch formal --thread-proof-only");

    assert!(
        !out.status.success(),
        "thread-proof-only without Lean output/check should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--thread-proof-only requires --emit-thread-proof-lean"),
        "expected thread-proof-only guardrail diagnostic, got:\n{stderr}"
    );
}

#[test]
fn test_formal_check_thread_proof_lean_missing_project_fails_cleanly() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofFlow.arch");
    let proof_out = td.path().join("ProofFlow.thread-proof.lean");
    let missing_project = td.path().join("missing_lean_project");
    std::fs::write(&src, thread_proof_fold_source("ProofFlow")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("formal")
        .arg(&src)
        .arg("--bound")
        .arg("2")
        .arg("--timeout")
        .arg("5")
        .arg(format!(
            "--emit-thread-proof-lean={}",
            proof_out.to_string_lossy()
        ))
        .arg("--check-thread-proof-lean")
        .arg(format!(
            "--thread-proof-lean-project={}",
            missing_project.to_string_lossy()
        ))
        .output()
        .expect("run arch formal --check-thread-proof-lean");

    assert!(
        !out.status.success(),
        "missing Lean project should fail before SMT run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Lean thread proof project not found"),
        "expected clean missing-project diagnostic, got:\n{stderr}"
    );
    assert!(
        proof_out.exists(),
        "Lean proof should still be written before replay failure"
    );
}

#[test]
fn test_build_emit_thread_proof_records_once_terminal_hold() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofOnce.arch");
    let sv_out = td.path().join("ProofOnce.sv");
    let proof_out = td.path().join("ProofOnce.thread-proof.json");
    std::fs::write(&src, thread_proof_once_source("ProofOnce")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof")
        .output()
        .expect("run arch build --emit-thread-proof");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json = std::fs::read_to_string(&proof_out).expect("read proof certificate");
    assert!(
        json.contains("\"once\": true"),
        "expected one-shot thread flag in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"state_name\": \"_t0_S2_action\""),
        "expected terminal action state in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"target_index\": 2"),
        "thread once terminal state should hold instead of wrapping:\n{json}"
    );
    assert!(
        json.contains("\"source_next_index\": 2"),
        "thread once terminal source-next should hold instead of wrapping:\n{json}"
    );

    let sv = std::fs::read_to_string(&sv_out).expect("read sv");
    assert!(
        sv.contains("_t0_state <= _t0_S2_action"),
        "SV terminal state should hold in S2:\n{sv}"
    );
}

#[test]
fn test_build_emit_thread_proof_lean_accepts_once_folded_terminal_action() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofOnceFold.arch");
    let sv_out = td.path().join("ProofOnceFold.sv");
    let lean_out = td.path().join("ProofOnceFold.thread-proof.lean");
    std::fs::write(
        &src,
        thread_proof_once_folded_terminal_source("ProofOnceFold"),
    )
    .expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof-lean")
        .output()
        .expect("run arch build --emit-thread-proof-lean");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let lean = std::fs::read_to_string(&lean_out).expect("read Lean proof certificate");
    assert!(
        lean.contains("once := true"),
        "Lean artifact should model one-shot thread hold semantics:\n{lean}"
    );
    assert!(
        lean.contains(
            "forall t, sourceTraceObs ProofOnceFold_T_0Source inputs natInputs cfg0 t = fsmTraceObs ProofOnceFold_T_0Fsm inputs natInputs cfg0 t"
        ),
        "Lean artifact should include the once trace-equivalence theorem:\n{lean}"
    );
    assert!(
        lean.contains("example : StepEffectFaithful ProofOnceFold_T_0Source ProofOnceFold_T_0Fsm"),
        "Lean artifact should include the once thread one-step state-effect theorem:\n{lean}"
    );
    assert!(
        lean.contains(
            "Arch.ThreadLoweringProof.FoldedExit.sourceStep ProofOnceFold_T_0__t0_S0_wait_until_folded_0Source"
        ),
        "Lean artifact should include the folded terminal action proof:\n{lean}"
    );
}

#[test]
fn test_build_emit_thread_proof_records_branch_guarded_transitions() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofBranch.arch");
    let sv_out = td.path().join("ProofBranch.sv");
    let proof_out = td.path().join("ProofBranch.thread-proof.json");
    std::fs::write(&src, thread_map_control_flow_source("ProofBranch")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof")
        .output()
        .expect("run arch build --emit-thread-proof");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json = std::fs::read_to_string(&proof_out).expect("read proof certificate");
    assert!(
        json.contains("\"state_name\": \"_t0_S0_dispatch\""),
        "expected fused go/sel dispatch state in proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"source_transitions\": [{\"condition\": \"go && sel\""),
        "dispatch source transitions should preserve both branch targets:\n{json}"
    );
    assert!(
        json.contains("\"condition\": \"go && !sel\""),
        "dispatch source transitions should preserve both branch guards:\n{json}"
    );
    assert!(json.contains("\"condition_guard\": {\"kind\":\"and\",\"lhs\":{\"kind\":\"atom\",\"name\":\"go\"},\"rhs\":{\"kind\":\"atom\",\"name\":\"sel\"}}"),
        "dispatch source transition should carry structured positive guard:\n{json}");
    assert!(json.contains("\"condition_guard\": {\"kind\":\"and\",\"lhs\":{\"kind\":\"atom\",\"name\":\"go\"},\"rhs\":{\"kind\":\"not\",\"expr\":{\"kind\":\"atom\",\"name\":\"sel\"}}}"),
        "dispatch source transition should carry structured negated guard:\n{json}");
    assert!(
        json.contains("\"target_index\": 1") && json.contains("\"target_name\": \"_t0_S1_action\""),
        "dispatch source transitions should preserve first branch target:\n{json}"
    );
    assert!(
        json.contains("\"target_index\": 2")
            && json.contains("\"target_name\": \"_t0_S2_wait_cycles\""),
        "dispatch source transitions should preserve second branch target:\n{json}"
    );
    assert!(json.contains("\"condition\": \"ack\", \"condition_guard\": {\"kind\":\"atom\",\"name\":\"ack\"}, \"target_index\": 3, \"target_name\": \"_t0_S3_action\""),
        "single guarded branch wait should preserve its non-natural target:\n{json}");
    assert!(
        json.contains("\"source_next_index\": 2"),
        "single guarded branch state's natural fallback should remain source-next:\n{json}"
    );
}

#[test]
fn test_build_emit_thread_proof_records_fork_join_rejoin_jumps() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("AxiWrite.arch");
    let sv_out = td.path().join("AxiWrite.sv");
    let proof_out = td.path().join("AxiWrite.thread-proof.json");
    std::fs::write(&src, include_str!("../tests/thread/fork_join.arch")).expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof")
        .output()
        .expect("run arch build --emit-thread-proof");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json = std::fs::read_to_string(&proof_out).expect("read proof certificate");
    assert!(
        json.contains("\"state_name\": \"_t0_S0_dispatch\""),
        "fork/join should begin with a product-state dispatch:\n{json}"
    );
    assert!(json.contains("\"condition\": \"aw_ready && w_ready\", \"condition_guard\": {\"kind\":\"and\",\"lhs\":{\"kind\":\"atom\",\"name\":\"aw_ready\"},\"rhs\":{\"kind\":\"atom\",\"name\":\"w_ready\"}}, \"target_index\": 4"),
        "fork/join dispatch should record the both-branches-done transition:\n{json}");
    assert!(json.contains("\"condition\": \"true\", \"condition_guard\": {\"kind\":\"true\"}, \"target_index\": 8"),
        "completed fork branch states should record unconditional non-natural rejoin jumps:\n{json}");
    assert!(json.contains("\"source_next_index\": 5"),
        "unconditional rejoin jump state should still preserve natural source-next fallback:\n{json}");
}

#[test]
fn test_build_emit_thread_proof_records_loop_bound_constants_as_structured_nat() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("NestedForRepro.arch");
    let sv_out = td.path().join("NestedForRepro.sv");
    let proof_out = td.path().join("NestedForRepro.thread-proof.json");
    std::fs::write(
        &src,
        include_str!("regression/issues/nested_for_threads/NestedForRepro.arch"),
    )
    .expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof")
        .output()
        .expect("run arch build --emit-thread-proof");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json = std::fs::read_to_string(&proof_out).expect("read proof certificate");
    assert!(
        json.contains("\"kind\":\"lt\",\"lhs\":{\"kind\":\"var\",\"name\":\"_t0_loop_cnt_1\"},\"rhs\":{\"kind\":\"const\",\"value\":3}"),
        "inner loop bound should remain a structured Nat constant:\n{json}"
    );
    assert!(
        json.contains("\"kind\":\"ge\",\"lhs\":{\"kind\":\"var\",\"name\":\"_t0_loop_cnt_0\"},\"rhs\":{\"kind\":\"const\",\"value\":2}"),
        "outer loop bound should remain a structured Nat constant:\n{json}"
    );
}

#[test]
fn test_build_emit_thread_proof_records_equality_as_structured_nat() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("ProofEq.arch");
    let sv_out = td.path().join("ProofEq.sv");
    let proof_out = td.path().join("ProofEq.thread-proof.json");
    std::fs::write(
        &src,
        r#"
module ProofEq
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port idx: in UInt<3>;
  port done: out pipe_reg<Bool, 1> reset rst => false;

  thread T on clk rising, rst high
    wait until idx == 2;
    wait until idx != 1;
    done <= true;
  end thread T
end module ProofEq
"#,
    )
    .expect("write source");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&sv_out)
        .arg("--emit-thread-proof")
        .output()
        .expect("run arch build --emit-thread-proof");
    assert!(
        out.status.success(),
        "build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json = std::fs::read_to_string(&proof_out).expect("read proof certificate");
    assert!(
        json.contains("\"kind\":\"eq\",\"lhs\":{\"kind\":\"var\",\"name\":\"idx\"},\"rhs\":{\"kind\":\"const\",\"value\":2}"),
        "equality guard should remain structured in the proof certificate:\n{json}"
    );
    assert!(
        json.contains("\"kind\":\"ne\",\"lhs\":{\"kind\":\"var\",\"name\":\"idx\"},\"rhs\":{\"kind\":\"const\",\"value\":1}"),
        "inequality guard should remain structured in the proof certificate:\n{json}"
    );
}

#[test]
fn test_build_emit_thread_map_multifile_paths_and_explicit_error() {
    let td = tempfile::tempdir().expect("tempdir");
    let a = td.path().join("A.arch");
    let b = td.path().join("B.arch");
    std::fs::write(&a, thread_map_smoke_source("A")).expect("write A");
    std::fs::write(&b, thread_map_smoke_source("B")).expect("write B");

    let ok = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&a)
        .arg(&b)
        .arg("--emit-thread-map")
        .output()
        .expect("run multi-file build");
    assert!(
        ok.status.success(),
        "multi-file bare map should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(
        td.path().join("A.thread.html").exists(),
        "expected A.thread.html"
    );
    assert!(
        td.path().join("B.thread.html").exists(),
        "expected B.thread.html"
    );

    let explicit = td.path().join("all.html");
    let err = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("ARCH_NO_LEARN", "1")
        .arg("build")
        .arg(&a)
        .arg(&b)
        .arg(format!("--emit-thread-map={}", explicit.display()))
        .output()
        .expect("run multi-file explicit map build");
    assert!(!err.status.success(), "explicit map without -o should fail");
    let stderr = String::from_utf8_lossy(&err.stderr);
    assert!(
        stderr.contains("--emit-thread-map=PATH requires"),
        "expected explicit multi-file diagnostic, got:\n{stderr}"
    );
}

#[test]
fn test_thread_map_metadata_records_dispatch_and_wait_roles() {
    let simple = collect_thread_map(&thread_map_smoke_source("Simple"));
    let simple_thread = &simple.modules[0].threads[0];
    assert!(
        simple_thread
            .states
            .iter()
            .any(|s| s.role == "wait_until" && s.labels.iter().any(|l| l.contains("go"))),
        "expected simple wait_until state labelled with go: {simple_thread:#?}"
    );

    let source = thread_map_control_flow_source("M");
    let map = collect_thread_map(&source);
    let thread = &map.modules[0].threads[0];
    assert!(
        thread.states.iter().any(|s| s.role == "wait_cycles"),
        "expected wait_cycles state: {thread:#?}"
    );
    assert!(
        thread
            .states
            .iter()
            .any(|s| s.role == "dispatch"
                && s.transitions.iter().any(|t| t.condition.contains("sel"))),
        "expected dispatch state with sel transition: {thread:#?}"
    );
}

#[test]
fn test_thread_map_html_renders_control_flow_chart() {
    let source = thread_map_control_flow_source("Branchy");
    let map = collect_thread_map(&source);
    let sources = vec![arch::thread_map::ThreadMapSource {
        start: 0,
        end: source.len(),
        filename: "Branchy.arch".to_string(),
        source,
    }];
    let html = arch::thread_map::render_html(&map, &sources, "Branchy.thread.html");
    assert!(
        html.contains("<h2>Thread Flow</h2>"),
        "expected right pane to be a flow-chart pane:\n{html}"
    );
    assert!(
        html.contains("thread-flow-chart"),
        "expected generated graph-style flow chart:\n{html}"
    );
    assert!(
        html.contains("graph-edge"),
        "expected graph transition edges:\n{html}"
    );
    assert!(
        html.contains("marker-end"),
        "expected arrowheads on graph edges:\n{html}"
    );
    assert!(
        html.contains("then") && html.contains("else"),
        "expected source-like branch labels in graph:\n{html}"
    );
    assert!(
        html.contains("data-state=\"S"),
        "expected state nodes in flow chart:\n{html}"
    );
    assert!(
        html.contains("dispatch"),
        "expected branch dispatch state in chart/table:\n{html}"
    );
    assert!(
        html.contains("sel"),
        "expected branch condition label in chart/table:\n{html}"
    );
    assert!(
        html.contains("wait until ack"),
        "expected then-branch wait label in chart/table:\n{html}"
    );
}

#[test]
fn test_thread_map_source_bands_anchor_broad_control_spans() {
    let source = include_str!(
        "regression/issues/nested_if_lock_outer_for_continuation/IfLockOuterForRepro.arch"
    );
    let map = collect_thread_map(source);
    let sources = vec![arch::thread_map::ThreadMapSource {
        start: 0,
        end: source.len(),
        filename: "IfLockOuterForRepro.arch".to_string(),
        source: source.to_string(),
    }];
    let html = arch::thread_map::render_html(&map, &sources, "IfLockOuterForRepro.thread.html");
    let row_for_line = |line: usize| {
        let needle = format!("<span class=\"ln\">{line}</span>");
        let pos = html
            .find(&needle)
            .unwrap_or_else(|| panic!("missing source line {line}"));
        let row_start = html[..pos].rfind("<div class=\"src-line\">").unwrap();
        let row_end = html[pos..].find("</div>").map(|off| pos + off).unwrap();
        &html[row_start..row_end]
    };

    assert!(
        row_for_line(30).contains(">S0<"),
        "wait-until state should mark the wait line:\n{}",
        row_for_line(30)
    );
    assert!(
        row_for_line(32).contains(">S3<") || row_for_line(32).contains(">S4<"),
        "broad loop/dispatch states should anchor to the loop header:\n{}",
        row_for_line(32)
    );
    assert!(
        !row_for_line(33).contains("class=\"band "),
        "comment lines inside a broad span should not be painted:\n{}",
        row_for_line(33)
    );
    assert!(
        !row_for_line(39).contains("class=\"band "),
        "nested comment lines inside a broad span should not be painted:\n{}",
        row_for_line(39)
    );
}

#[test]
fn test_thread_map_spans_ignore_generated_counter_loads() {
    let source = include_str!("../tests/thread/wait_cycles.arch");
    let map = collect_thread_map(source);
    let thread = &map.modules[0].threads[0];
    let line_range = |span: lexer::Span| {
        let start_line = source[..span.start].bytes().filter(|b| *b == b'\n').count() + 1;
        let end_line = source[..span.end].bytes().filter(|b| *b == b'\n').count() + 1;
        (start_line, end_line)
    };

    let wait_until = thread
        .states
        .iter()
        .find(|s| s.role == "wait_until")
        .expect("wait_until state");
    assert_eq!(
        line_range(wait_until.span),
        (8, 8),
        "S0 should render only on the wait-until source line: {wait_until:#?}"
    );
    let wait_until_src = &source[wait_until.span.start..wait_until.span.end];
    assert!(
        !wait_until_src.contains("wait 5 cycle"),
        "S0 span should stop before the following wait-cycle source: {wait_until_src:?}"
    );
    assert!(
        !wait_until_src.contains("end module"),
        "S0 span should not stretch to the whole module: {wait_until_src:?}"
    );

    let wait_cycles = thread
        .states
        .iter()
        .find(|s| s.role == "wait_cycles")
        .expect("wait_cycles state");
    assert_eq!(
        line_range(wait_cycles.span),
        (9, 9),
        "S1 should render only on the wait-cycle source line: {wait_cycles:#?}"
    );
    let wait_cycles_src = &source[wait_cycles.span.start..wait_cycles.span.end];
    assert!(
        !wait_cycles_src.contains("pulse = 1"),
        "wait_cycles span should stop before the action source: {wait_cycles_src:?}"
    );
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
    assert!(
        sv.contains("module _M_threads"),
        "merged thread module should be emitted:\n{sv}"
    );
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
    assert!(
        sv.contains("module _M_threads"),
        "merged thread module should be emitted:\n{sv}"
    );
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
    assert!(
        sv.contains("module _M_threads"),
        "merged thread module should be emitted:\n{sv}"
    );
    // The dispatch state's transition table negates the if condition for
    // the else branch. Verify both arms land at distinct branch bases.
    assert!(
        sv.contains("is_wr") && sv.contains("!"),
        "expected dispatch to use `is_wr` and `!is_wr` arms:\n{sv}"
    );
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
        let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target lowering");
        let ast = arch::elaborate::lower_tlm_initiator_calls(ast).expect("tlm initiator lowering");
        let result = arch::elaborate::lower_threads(ast);
        let errs = result.expect_err("thread with no wait / do until should fail lower_threads");
        assert!(!errs.is_empty(), "expected at least one error");
        let msg = errs[0].to_string();
        assert!(
            msg.contains("must contain at least one `wait` or `do until`"),
            "error should mention wait + do until: {msg}"
        );
        assert!(
            msg.contains("seq on clk"),
            "error should suggest `seq on clk` alternative: {msg}"
        );
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
    assert!(
        sv.contains("always_ff"),
        "do/until thread should still compile to SV:\n{sv}"
    );
}

#[test]
fn test_do_until_rejects_nested_lock() {
    // Regression for issue #410: a `do … until` body that contained a
    // nested `lock` (or `for` / `wait` / `fork` / `do…until` / `return`)
    // was previously silently dropped at lowering and produced an
    // infinite-loop FSM. The elaborator must now reject this with a
    // diagnostic pointing at the offending inner construct and at the
    // enclosing `do … until`.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port start:       in Bool;
          port should_loop: in Bool;
          port body_count: out UInt<8>;
          resource lk: mutex<priority>;
          reg body_count_r: UInt<8> reset rst => 0;
          thread T on clk rising, rst low
            default comb
              body_count = body_count_r;
            end default
            wait until start;
            do
              lock lk
                do
                until true;
              end lock lk
              body_count_r <= (body_count_r + 1).trunc<8>();
            until not should_loop;
          end thread T
        end module M
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lex");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let err = arch::elaborate::lower_threads(ast).expect_err("expected lowering error");
    let msg = err.iter().map(|e| format!("{e:?}")).collect::<String>();
    assert!(
        msg.contains("`lock`") && msg.contains("`do ... until`"),
        "expected do-until-rejects-lock diagnostic, got: {msg}"
    );
}

#[test]
fn test_do_until_rejects_nested_wait() {
    // Companion to the lock case: a `do … until` body containing a
    // `wait until` must also be rejected — the wait cannot lower as
    // a hold-state body.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go: in Bool;
          port hit: in Bool;
          port reg flag: out Bool reset rst => false;
          thread T on clk rising, rst low
            do
              wait until hit;
              flag <= true;
            until go;
          end thread T
        end module M
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lex");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let err = arch::elaborate::lower_threads(ast).expect_err("expected lowering error");
    let msg = err.iter().map(|e| format!("{e:?}")).collect::<String>();
    assert!(
        msg.contains("`wait until`") && msg.contains("`do ... until`"),
        "expected do-until-rejects-nested-wait diagnostic, got: {msg}"
    );
}

#[test]
fn test_do_until_rejects_nested_for() {
    // `for` inside `do … until` body — same silent-drop trap pre-#410.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go: in Bool;
          port done: in Bool;
          port reg ctr: out UInt<8> reset rst => 0;
          thread T on clk rising, rst low
            wait until go;
            do
              for i in 0..3
                ctr <= (ctr + 1).trunc<8>();
              end for
            until done;
          end thread T
        end module M
    "#;
    let tokens = arch::lexer::tokenize(source).expect("lex");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let err = arch::elaborate::lower_threads(ast).expect_err("expected lowering error");
    let msg = err.iter().map(|e| format!("{e:?}")).collect::<String>();
    assert!(
        msg.contains("`for`") && msg.contains("`do ... until`"),
        "expected do-until-rejects-nested-for diagnostic, got: {msg}"
    );
}

#[test]
fn test_do_until_allows_nested_ifelse_with_simple_assigns() {
    // Regression boundary: `if/else` inside a `do … until` body is fine
    // as long as every branch contains only comb/seq assigns and `log`
    // — i.e. nothing that would need a fresh FSM state. This keeps the
    // legitimate per-cycle protocol-drive use case working.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go:   in Bool;
          port pick: in Bool;
          port done: in Bool;
          port reg a: out Bool reset rst => false;
          port reg b: out Bool reset rst => false;
          thread T on clk rising, rst low
            wait until go;
            do
              if pick
                a <= true;
              else
                b <= true;
              end if
            until done;
          end thread T
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("always_ff"),
        "do/until with simple if/else body should compile:\n{sv}"
    );
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

    assert!(
        sv.contains("if (req && is_mul) begin\n          phase <= 4'd1;"),
        "then-branch first action should be hoisted onto the wait-exit edge:\n{sv}"
    );
    assert!(
        sv.contains("if (req && !is_mul) begin\n          phase <= 4'd3;"),
        "else-branch first action should be hoisted onto the wait-exit edge:\n{sv}"
    );
    assert!(
        !sv.contains("_t0_state == 3"),
        "fused lowering should not emit old wait->dispatch->prefix state chain:\n{sv}"
    );
}

#[test]
fn test_thread_wait_ifelse_thread_sim_both_branch_latency() {
    run_tlm_thread_sim_both(
        "tests/thread/if_wait_thread_sim_both.arch",
        "tests/thread/tb_if_wait_thread_sim_both.cpp",
        "PASS IfWaitThreadSimBoth",
    );
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

    assert!(
        sv.contains("phase <= 4'd1;"),
        "first arm should keep its first action:\n{sv}"
    );
    assert!(
        sv.contains("phase <= 4'd2;"),
        "elsif arm should keep its first action:\n{sv}"
    );
    assert!(
        sv.contains("phase <= 4'd3;"),
        "else arm should keep its first action:\n{sv}"
    );
    assert!(
        sv.contains("req && sel == 2'd0"),
        "first arm guard should include the wait condition:\n{sv}"
    );
    assert!(
        sv.contains("req && !(sel == 2'd0) && sel == 2'd1"),
        "elsif guard should be flattened onto the original wait state:\n{sv}"
    );
    assert!(
        sv.contains("req && !(sel == 2'd0) && !(sel == 2'd1)"),
        "else guard should be flattened onto the original wait state:\n{sv}"
    );
    assert!(
        !sv.contains("_t0_state == 3"),
        "flattened three-arm dispatch should not leave a nested dispatch state:\n{sv}"
    );
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
    assert!(
        sv.contains("input logic [7:0] payload"),
        "`default comb` RHS-only signal must be wired into the lowered thread module:\n{sv}"
    );
    let default_pos = sv
        .find("data = payload;")
        .expect("expected unconditional default data assignment");
    let state_pos = sv
        .find("if (_t0_state")
        .expect("expected state-guarded comb assignments");
    assert!(
        default_pos < state_pos,
        "`default comb` assignments must precede state-specific comb assignments:\n{sv}"
    );
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
    assert!(
        msg.contains("default comb") && msg.contains("done") && msg.contains("<="),
        "expected targeted diagnostic for default-comb/seq-driver conflict, got: {msg}"
    );
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
    let opts = elaborate::ThreadLowerOpts {
        auto_asserts: true,
        ..Default::default()
    };
    let sv = compile_to_sv_with_opts(source, &opts);
    assert!(
        sv.contains("_auto_thread_t0_branch_"),
        "expected dispatch-state branch assertions:\n{sv}"
    );
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
    assert!(
        sv.contains("module _arb_M_shared_lk"),
        "expected synthesized arbiter module:\n{sv}"
    );
    // Default = priority arbiter (linear pri_i loop).
    assert!(
        sv.contains("for (int pri_i = 0; pri_i < 3"),
        "default policy should be priority:\n{sv}"
    );
    // Inst inside the merged module.
    assert!(
        sv.contains("_arb_M_shared_lk _arb_inst_shared_lk"),
        "expected arbiter instance inside merged module:\n{sv}"
    );
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
    assert!(
        sv.contains("logic [0:0] rr_ptr_r;"),
        "round_robin should emit rr_ptr_r register:\n{sv}"
    );
    // Pointer advances from the actual grantee (not the scan start) and wraps
    // explicitly at NUM_REQ; the bare `rr_ptr_r + 1` form was incorrect for
    // non-power-of-2 NUM_REQ. See `emit_arbiter_round_robin`.
    assert!(
        sv.contains("rr_ptr_r <= (grant_requester ==")
            && sv.contains("? '0 : grant_requester + 1'b1;"),
        "round_robin should advance from grant_requester with explicit wrap:\n{sv}"
    );
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
    assert!(
        sv.contains("function automatic"),
        "custom-policy arbiter should embed the user function:\n{sv}"
    );
    assert!(
        sv.contains("PickHigh"),
        "expected user function name in arbiter:\n{sv}"
    );
    // last_grant_r register comes from the custom-arbiter codegen.
    assert!(
        sv.contains("last_grant_r"),
        "custom-policy arbiter should track last_grant_r:\n{sv}"
    );
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
    assert!(
        sv.contains("module _M_threads"),
        "nested if/else with waits should compile:\n{sv}"
    );
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
    assert!(
        sv.contains("module _M_threads"),
        "for-loop in then-branch should compile:\n{sv}"
    );
    // Bug witness: the buggy lowering emitted `if (1'b1) _t0_state <= <rejoin>`
    // inside the for-loop's last state, causing the body to execute exactly once.
    // The fix removes this unconditional override, so the only state-write
    // arms inside state 4 (for-loop last) should be the loop-back and exit
    // arms — both guarded by `_t0_loop_cnt_0` comparisons against `burst_len - 1`.
    // (The counter is named `_t0_loop_cnt_0` since issue #414: each `for`
    // instance in a thread allocates its own `_loop_cnt_{id}` register.)
    assert!(
        !sv.contains("if (1'b1) begin\n          _t0_state"),
        "buggy unconditional override should not be emitted:\n{sv}"
    );
    // The for-loop's exit arm should land at the rejoin state (post-if
    // wait_cycles), not at the start of the else branch.
    // The else branch is `wait 1 cycle` (one state); the rejoin is the
    // post-if `wait 1 cycle` (one state). With the fix, `_t0_loop_cnt_0 >=
    // (burst_len - 1)` should write the rejoin state, not else_base.
    let exit_arm = sv.contains("if (_t0_loop_cnt_0 >= 16'(burst_len - 1)) begin")
        || sv.contains("if (_t0_loop_cnt_0 >= 16'(burst_len-1)) begin");
    assert!(
        exit_arm,
        "for-loop exit arm should compare loop_cnt_0 against burst_len-1:\n{sv}"
    );
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
    assert!(
        doc.contains("Saturating up-counter."),
        "outer doc text missing first line: {doc:?}"
    );
    assert!(
        doc.contains("Wraps to MAX"),
        "outer doc text missing third line: {doc:?}"
    );
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
    assert!(
        inner.contains("CSR access"),
        "inner doc text missing: {inner:?}"
    );
    assert!(
        m.doc.is_none(),
        "outer doc should be None, got: {:?}",
        m.doc
    );
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
    assert!(
        inner.contains("Multi-channel DMA engine"),
        "file inner_doc should preserve the prose summary:\n{inner}"
    );
    assert!(
        inner.contains("---"),
        "file inner_doc should retain the frontmatter delimiters verbatim:\n{inner}"
    );
    let fm = ast
        .frontmatter
        .as_ref()
        .expect("file should carry frontmatter");
    assert!(
        fm.contains("spec_md: doc/specs/dma_engine.md"),
        "frontmatter should preserve spec_md field:\n{fm}"
    );
    assert!(
        fm.contains("tags: [dma, axi]"),
        "frontmatter should preserve tags:\n{fm}"
    );
    let open_close: Vec<_> = fm.lines().filter(|l| l.trim() == "---").collect();
    assert_eq!(
        open_close.len(),
        2,
        "frontmatter should contain exactly 2 `---` delimiter lines:\n{fm}"
    );
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
    let doc = c
        .common
        .doc
        .as_ref()
        .expect("counter should have outer doc");
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
    assert!(
        m.doc.is_none(),
        "4-slash banner should not attach as a doc comment, got: {:?}",
        m.doc
    );
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
    assert!(
        s.doc
            .as_ref()
            .map_or(false, |d| d.contains("Cache line state")),
        "struct should have outer doc, got {:?}",
        s.doc
    );
    let e = match &ast.items[1] {
        arch::ast::Item::Enum(e) => e,
        _ => panic!("expected enum"),
    };
    assert!(
        e.doc
            .as_ref()
            .map_or(false, |d| d.contains("Branch direction")),
        "enum should have outer doc, got {:?}",
        e.doc
    );
    let f = match &ast.items[2] {
        arch::ast::Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert!(
        f.doc
            .as_ref()
            .map_or(false, |d| d.contains("Saturating add")),
        "function should have outer doc, got {:?}",
        f.doc
    );
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
    assert!(
        b.doc.as_ref().map_or(false, |d| d.contains("AXI4")),
        "bus should have outer doc, got {:?}",
        b.doc
    );
    let s = match &ast.items[1] {
        arch::ast::Item::Synchronizer(s) => s,
        _ => panic!("expected synchronizer"),
    };
    assert!(
        s.doc.as_ref().map_or(false, |d| d.contains("2-FF")),
        "synchronizer should have outer doc, got {:?}",
        s.doc
    );
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
    assert!(
        c.common
            .inner_doc
            .as_ref()
            .map_or(false, |d| d.contains("watchdog timer")),
        "counter inner_doc missing, got {:?}",
        c.common.inner_doc
    );
    let a = match &ast.items[1] {
        arch::ast::Item::Arbiter(a) => a,
        _ => panic!("expected arbiter"),
    };
    assert!(
        a.common
            .inner_doc
            .as_ref()
            .map_or(false, |d| d.contains("Round-robin")),
        "arbiter inner_doc missing, got {:?}",
        a.common.inner_doc
    );
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
    assert!(
        u.doc.as_ref().map_or(false, |d| d.contains("cache-line")),
        "use decl should have outer doc, got {:?}",
        u.doc
    );
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
    assert!(
        m.doc.is_none(),
        "module doc should be None, got {:?}",
        m.doc
    );
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
    let has_reg = m
        .body
        .iter()
        .any(|i| matches!(i, arch::ast::ModuleBodyItem::RegDecl(_)));
    let has_wire = m
        .body
        .iter()
        .any(|i| matches!(i, arch::ast::ModuleBodyItem::WireDecl(_)));
    assert!(has_reg, "reg should be present despite leading doc comment");
    assert!(
        has_wire,
        "wire should be present despite leading doc comment"
    );
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
    assert!(
        m.inner_doc.is_none(),
        "stray //! mid-body should not attach to inner_doc, got: {:?}",
        m.inner_doc
    );
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
fn test_count1_port_array_inst_connection_uses_unindexed_name() {
    // A `ports[1]` (count-1) regfile group flattens its member ports WITHOUT an
    // index in the module declaration (`read_addr`, not `read0_addr`). An inst
    // connection written with an explicit `read[0].addr` is parser-flattened to
    // `read0_addr`, which mismatched the declaration → Verilator PINNOTFOUND and
    // a sim `no member named 'read0_addr'`. Both `read.addr` and `read[0].addr`
    // must resolve to the un-indexed `read_addr`.
    let source = r#"
regfile Rf1
  param NREGS: const = 4;
  param T: type = UInt<32>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[1] read
    addr: in UInt<2>;
    data: out UInt<32>;
  end ports read
  ports[1] write
    en:   in Bool;
    addr: in UInt<2>;
    data: in UInt<32>;
  end ports write
end regfile Rf1

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port a: in UInt<2>;
  port d: out UInt<32>;
  wire dw: UInt<32>;
  inst rf: Rf1
    clk <- clk;
    rst <- rst;
    read[0].addr <- a;
    read[0].data -> dw;
    write.en     <- false;
    write.addr   <- 0;
    write.data   <- 0;
  end inst rf
  comb
    d = dw;
  end comb
end module Top
"#;
    let sv = compile_to_sv(source);
    // The inst connection must use the un-indexed pin name matching the count-1
    // declaration.
    assert!(
        sv.contains(".read_addr(") && sv.contains(".read_data("),
        "count-1 `read[0]` connection must emit `.read_addr(`/`.read_data(`:\n{sv}"
    );
    assert!(
        !sv.contains(".read0_addr(") && !sv.contains(".read0_data("),
        "count-1 `read[0]` connection must NOT emit the indexed `.read0_addr(`:\n{sv}"
    );
    // The declaration side (always un-indexed for count-1) must agree.
    assert!(
        sv.contains("read_addr") && !sv.contains("read0_addr"),
        "count-1 regfile declaration + connection names must both be un-indexed:\n{sv}"
    );
}

#[test]
fn test_regfile_latch_emits_always_latch_per_row() {
    let source = format!(
        "{LATCH_RF_DECL}
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
    "
    );
    let sv = compile_to_sv(&source);
    let n = sv.matches("always_latch").count();
    assert_eq!(
        n, 4,
        "expected 4 always_latch blocks (NREGS=4), got {n}:\n{sv}"
    );
    assert!(
        sv.contains("write_addr == 2'd0"),
        "row 0 enable missing:\n{sv}"
    );
    assert!(
        sv.contains("write_addr == 2'd3"),
        "row 3 enable missing:\n{sv}"
    );
    let module_body: String = sv
        .split("module LatchRf")
        .nth(1)
        .unwrap_or(&sv)
        .split("endmodule")
        .next()
        .unwrap_or(&sv)
        .to_string();
    assert!(
        !module_body.contains("always_ff"),
        "latch RF body should not contain always_ff:\n{module_body}"
    );
}

#[test]
fn test_regfile_latch_rejects_let_source() {
    let source = format!(
        "{LATCH_RF_DECL}
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
    "
    );
    let tokens = lexer::tokenize(&source).expect("lex");
    let mut parser = Parser::new(tokens, &source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let ast = elaborate::lower_threads(ast).expect("lower threads");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let result = TypeChecker::new(&symbols, &ast).check();
    assert!(result.is_err(), "latch RF with `let` source should error");
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(
        err_msg.contains("kind: latch regfile") && err_msg.contains("flop"),
        "diagnostic should explain the flop-source requirement; got: {err_msg}"
    );
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
    let source = format!(
        "{LATCH_RF_INTERNAL_DECL}
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
    "
    );
    let sv = compile_to_sv(&source);
    let body: String = sv
        .split("module LatchRfInt")
        .nth(1)
        .unwrap_or(&sv)
        .split("endmodule")
        .next()
        .unwrap_or(&sv)
        .to_string();
    assert!(body.contains("we_q"), "expected we_q sample flop:\n{body}");
    assert!(
        body.contains("waddr_q"),
        "expected waddr_q sample flop:\n{body}"
    );
    assert!(
        body.contains("wdata_q"),
        "expected wdata_q sample flop:\n{body}"
    );
    assert!(
        body.contains("always_ff @(posedge clk)"),
        "expected sample flop always_ff:\n{body}"
    );
    let n_latch = body.matches("always_latch").count();
    assert_eq!(
        n_latch, 4,
        "expected 4 always_latch blocks (NREGS=4):\n{body}"
    );
    // ICG-equivalent gating: latch transparent only when clk is low.
    assert!(
        body.contains("!clk"),
        "expected `!clk` gating in latch enable for ICG-equivalent path:\n{body}"
    );
    assert!(
        body.contains("we_q && waddr_q == 2'd0"),
        "row 0 enable should use sampled (q) signals:\n{body}"
    );
}

#[test]
fn test_regfile_latch_internal_skips_flop_source_check() {
    // With flops: internal the regfile owns its own sample flops, so the
    // caller is allowed to drive write pins from a `let` (combinational
    // expression). This is the static-check skip property.
    let source = format!(
        "{LATCH_RF_INTERNAL_DECL}
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
    "
    );
    let tokens = lexer::tokenize(&source).expect("lex");
    let mut parser = Parser::new(tokens, &source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let ast = elaborate::lower_threads(ast).expect("lower threads");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let result = TypeChecker::new(&symbols, &ast).check();
    assert!(
        result.is_ok(),
        "flops: internal should skip flop-source check; got error: {:?}",
        result.err()
    );
}

#[test]
fn test_regfile_latch_internal_sim_codegen_emits_sample_flops_and_gated_capture() {
    // sim_codegen must mirror the SV semantics for `flops: internal`:
    // sample we_q/waddr_q/wdata_q on rising edge, then capture into _rf
    // during clk-low (the half-cycle latch transparency window).
    let source = format!("{LATCH_RF_INTERNAL_DECL}");
    let cpp = compile_to_sim_h(&source, false);
    assert!(
        cpp.contains("_we_q"),
        "expected _we_q sample flop in sim:\n{cpp}"
    );
    assert!(
        cpp.contains("_waddr_q"),
        "expected _waddr_q sample flop in sim:\n{cpp}"
    );
    assert!(
        cpp.contains("_wdata_q"),
        "expected _wdata_q sample flop in sim:\n{cpp}"
    );
    // Posedge: sample.
    assert!(
        cpp.contains("_we_q = write_en;"),
        "sim should sample _we_q from write_en on posedge:\n{cpp}"
    );
    // Comb: latch transparency gated by `!clk && _we_q`.
    assert!(
        cpp.contains("if (!clk && _we_q)"),
        "sim should gate latch capture with `!clk && _we_q`:\n{cpp}"
    );
    assert!(
        cpp.contains("_rf[_waddr_q] = _wdata_q;"),
        "sim should capture _wdata_q into _rf[_waddr_q]:\n{cpp}"
    );
    // Posedge must NOT contain a direct flop-style write — that would
    // collapse the latch into a flop and lose the 1-cycle latency.
    let posedge = cpp
        .split("eval_posedge() {")
        .nth(1)
        .unwrap_or("")
        .split("}")
        .next()
        .unwrap_or("");
    assert!(
        !posedge.contains("_rf[write_addr]"),
        "eval_posedge must NOT do a flop-style _rf write under flops:internal:\n{posedge}"
    );
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
    assert!(
        !cpp.contains("_we_q"),
        "external flops should NOT emit _we_q sample flop:\n{cpp}"
    );
    // Comb-time latch update gated by write_en.
    assert!(
        cpp.contains("if (write_en)") && cpp.contains("_rf[write_addr] = write_data;"),
        "external flops sim should be a comb-gated _rf write:\n{cpp}"
    );
    // Posedge eval must not directly sample _rf (that's flop semantics).
    let posedge = cpp
        .split("eval_posedge() {")
        .nth(1)
        .unwrap_or("")
        .split("}")
        .next()
        .unwrap_or("");
    assert!(
        !posedge.contains("_rf[write_addr] = write_data"),
        "eval_posedge must not be the only writer under kind:latch:\n{posedge}"
    );
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
    assert!(
        sv.contains("always_ff @(posedge clk)"),
        "default kind:flop should emit always_ff:\n{sv}"
    );
    assert!(
        !sv.contains("always_latch"),
        "default kind:flop should not emit always_latch:\n{sv}"
    );
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
    assert!(
        case_0_body.contains("0xAA") || case_0_body.contains("170"),
        "for-body assign of 0xAA should reach the C++ sim:\n{cpp}"
    );
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
    assert!(
        default_count <= 1,
        "fix means only the wildcard arm emits `default:`; got {default_count}\n{cpp}"
    );
    // All three case values should appear as case labels.
    for (n, lit) in [(0, "case 0"), (1, "case 1"), (2, "case 2")] {
        assert!(
            cpp.contains(lit) || cpp.contains(&format!("case {n}u")),
            "let-bound ident arm should emit `case {n}` for value {n}:\n{cpp}"
        );
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
        assert!(
            cpp.contains(lit) || cpp.contains(&format!("case {n}u")),
            "param-bound ident arm should emit `case {n}`:\n{cpp}"
        );
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
    assert!(
        cpp.contains("<< 12"),
        "7-bit OPCODE + 5-bit rd must place 20-bit imm at offset 12, not 13:\n{cpp}"
    );
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
    assert!(
        sv.contains("localparam [1:0] OP_GO = 2'd2"),
        "parent module must keep its localparam OP_GO:\n{sv}"
    );
    // Synthetic threads submodule should ALSO declare OP_GO, not refer
    // to an undeclared identifier. The emit places submodule decls
    // ahead of the parent module, but both must contain it.
    let occurrences = sv.matches("localparam [1:0] OP_GO = 2'd2").count();
    assert!(occurrences >= 2,
        "both parent and `_M_threads` submodule should declare OP_GO; only {occurrences} occurrence(s):\n{sv}");
    // And the thread body's predicate must reference OP_GO (proves the
    // identifier survived lowering).
    assert!(
        sv.contains("OP_GO") && sv.matches("OP_GO").count() >= 3,
        "thread body should compare op_i against OP_GO:\n{sv}"
    );
}

#[test]
fn test_lower_threads_forwards_parent_params_to_threads_inst() {
    // arch-com#507: the synthetic `_<mod>_threads` submodule repeats
    // overridable parent params so thread bodies can reference them, but
    // the wrapper must also pass through the wrapper's current parameter
    // values. Otherwise an inst-site override affects parent logic while
    // the lifted thread body silently keeps the helper defaults.
    let source = r#"
        module ParamThread
          param DONE_VALUE: const = 5;
          param OUT_W: const = 4;
          local param LOCAL_DONE: const = DONE_VALUE + 1;

          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port go: in Bool;
          port reg result: out UInt<OUT_W> reset rst => 0;

          thread on clk rising, rst high
            wait until go;
            result <= DONE_VALUE;
            wait 1 cycle;
          end thread
        end module ParamThread

        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port go: in Bool;
          port result: out UInt<8>;

          inst u: ParamThread
            param DONE_VALUE = 9;
            param OUT_W = 8;
            clk <- clk;
            rst <- rst;
            go <- go;
            result -> result;
          end inst u
        end module Top
    "#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("ParamThread #(.DONE_VALUE(9), .OUT_W(8)) u ("),
        "top-level override should still be emitted:\n{sv}"
    );
    assert!(
        sv.contains("_ParamThread_threads #(.DONE_VALUE(DONE_VALUE), .OUT_W(OUT_W)) _threads ("),
        "wrapper must forward overridable params into the synthesized threads helper:\n{sv}"
    );
    assert!(
        !sv.contains(".LOCAL_DONE(LOCAL_DONE)"),
        "localparams are cloned into the helper but must not be overridden at the inst site:\n{sv}"
    );
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
    assert!(
        h.contains("static constexpr uint64_t SCHEDULED_CORE_CYCLES = 7ULL;"),
        "thread sim header should declare module param constants:\n{h}"
    );
    assert!(
        h.contains("static constexpr uint64_t DONE_W = 8ULL;"),
        "derived module params should be folded for C++ visibility:\n{h}"
    );
    assert!(
        h.contains("co_await arch_rt::wait_cycles(&_slot_0, SCHEDULED_CORE_CYCLES);"),
        "wait-cycle expression should keep using the declared constexpr param:\n{h}"
    );
    assert!(
        h.contains("done = DONE_W;"),
        "thread body should use the declared constexpr param:\n{h}"
    );
    assert!(
        h.contains("uint8_t done = 0;"),
        "param-derived port widths should resolve in thread sim C++ types:\n{h}"
    );
}

#[test]
fn test_thread_sim_supports_1024_bit_thread_storage() {
    // The pre-lowering coroutine thread sim used to reject UInt widths above
    // 64 bits even though the normal sim path has VlWide storage. Keep this
    // regression at the 1kb boundary requested for wide thread payloads.
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port data_in: in UInt<1024>;
          port data_out: out UInt<1024>;
          reg payload_r: UInt<1024> reset rst => 0;
          let mirrored: UInt<1024> = payload_r;

          thread on clk rising, rst high
            payload_r <= data_in;
            wait 1 cycle;
            data_out = mirrored;
            wait 1 cycle;
          end thread
        end module M
    "#;

    let h = compile_to_thread_sim_h(source);
    assert!(
        h.contains("VlWide<32> data_in{};"),
        "1024-bit input port should use VlWide<32> storage:\n{h}"
    );
    assert!(
        h.contains("VlWide<32> data_out{};"),
        "1024-bit output port should use VlWide<32> storage:\n{h}"
    );
    assert!(
        h.contains("VlWide<32> payload_r{};"),
        "1024-bit thread-written reg should use VlWide<32> storage:\n{h}"
    );
    assert!(
        h.contains("VlWide<32> mirrored{};"),
        "1024-bit let binding should use VlWide<32> storage:\n{h}"
    );
    assert!(
        h.contains("payload_r = data_in;") && h.contains("data_out = mirrored;"),
        "wide thread assignments should emit direct VlWide copies:\n{h}"
    );
}

#[test]
fn test_thread_sim_runs_1024_bit_payload_copy() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("--thread-sim")
        .arg("parallel")
        .arg("tests/thread_sim_wide_1024/ThreadWide1024.arch")
        .arg("--tb")
        .arg("tests/thread_sim_wide_1024/tb_thread_wide_1024.cpp")
        .output()
        .expect("run arch sim --thread-sim parallel for 1024-bit thread payload");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("PASS thread_wide_1024"),
        "1024-bit thread-sim payload copy should compile and run\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn test_thread_sim_rejects_wide_arithmetic_until_codegen_supports_it() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port a: in UInt<128>;
          port y: out UInt<128>;
          reg r: UInt<128> reset rst => 0;

          thread on clk rising, rst high
            r <= a;
            wait 1 cycle;
            y = r + 128'd1;
            wait 1 cycle;
          end thread
        end module M
    "#;

    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let ast = arch::elaborate::lower_tlm_target_threads(ast).expect("tlm target lowering");
    let ast = arch::elaborate::lower_tlm_initiator_calls(ast).expect("tlm initiator lowering");
    let ast = arch::elaborate::lower_pipe_reg_ports(ast).expect("lower pipe_reg error");
    let ast = arch::elaborate::lower_credit_channel_dispatch(ast).expect("cc dispatch error");
    let m = ast
        .items
        .iter()
        .find_map(|item| match item {
            arch::ast::Item::Module(m) => Some(m),
            _ => None,
        })
        .expect("module");
    let err = match arch::sim_codegen::thread_sim::gen_module_thread(m, false, false, 1) {
        Ok(_) => panic!(
            "wide arithmetic should be rejected until the thread-sim emitter lowers VlWide ops"
        ),
        Err(err) => err,
    };
    assert!(
        err.contains("currently supports wide (>64-bit) values only as direct copies")
            && err.contains("assignment value"),
        "expected a direct-copy-only diagnostic for unsupported wide arithmetic:\n{err}"
    );
}

#[test]
fn test_thread_sim_rejects_unsupported_comb_expr_before_cpp_codegen() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port hi: in UInt<8>;
          port lo: in UInt<8>;
          port out: out UInt<16>;

          thread on clk rising, rst high
            out = {hi, lo};
            wait 1 cycle;
          end thread
        end module M
    "#;

    let err = compile_to_thread_sim_result(source)
        .expect_err("unsupported thread-sim expression should be a codegen error");
    assert!(
        err.contains("expr shape not supported"),
        "expected unsupported expression diagnostic, got: {err}"
    );
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
    assert!(
        cpp.contains("#define SCHEDULED_CORE_CYCLES 7ULL"),
        "sim header should define typed module params for lowered thread bodies:\n{cpp}"
    );
    assert!(
        cpp.contains("#define DONE_W 8ULL"),
        "derived typed module params should be folded for C++ visibility:\n{cpp}"
    );
    assert!(
        cpp.contains("#define CALLS 3ULL"),
        "narrow typed module params should also be emitted:\n{cpp}"
    );
    assert!(
        cpp.contains("uint8_t done;"),
        "param-derived port widths should resolve in normal sim C++ types:\n{cpp}"
    );
    assert!(
        cpp.contains("_n_calls_r  = CALLS;"),
        "lowered thread body should keep using the declared C++ param constant:\n{cpp}"
    );
}

#[test]
fn test_sim_codegen_inst_override_of_derived_default_param_wins_in_define() {
    // Regression (NIC-400 sparse-connectivity decoder): a sub-module param
    // whose *default* references another param (`CONNECT_MASK = (1 << N) - 1`)
    // but which is EXPLICITLY OVERRIDDEN at the inst site must bake the
    // overridden value — not the default expression's value — into the sim
    // backend's `#define`.
    //
    // Pre-fix: monomorphize_module preserved the derived-default expression
    // for any param referencing other params, even when the inst site
    // overrode it. The SV backend masked this (it re-applies the override as
    // an inst param), but the C++ sim backend emits `#define CONNECT_MASK
    // <default>`, so the override was silently dropped — two instances with
    // different masks both evaluated against the default value. The two
    // backends then diverged under harc --check-backends.
    let source = r#"
        module Sub
          param N: const = 4;
          param MASK: const = (1 << N) - 1;
          port sel:  in  UInt<2>;
          port hit:  out Bool;
          let bit_v: UInt<1> = ((MASK >> sel) & 1).trunc<1>();
          comb
            hit = (bit_v == 1);
          end comb
        end module Sub

        module Top
          port sel:   in  UInt<2>;
          port hit_a: out Bool;
          port hit_b: out Bool;
          inst a: Sub
            param N    = 4;
            param MASK = 3;
            sel <- sel;
            hit -> hit_a;
          end inst a
          inst b: Sub
            param N    = 4;
            param MASK = 6;
            sel <- sel;
            hit -> hit_b;
          end inst b
        end module Top
    "#;
    let cpp = compile_to_sim_h(source, false);
    // The two specialized variants must carry their OVERRIDDEN mask values,
    // not the derived default `(1 << 4) - 1 == 15`.
    assert!(
        cpp.contains("#define MASK 3ULL"),
        "overridden derived-default param must bake the override (3), not the default (15):\n{cpp}"
    );
    assert!(
        cpp.contains("#define MASK 6ULL"),
        "second instance's overridden mask (6) must also be baked, not the default (15):\n{cpp}"
    );
    assert!(
        !cpp.contains("#define MASK 15ULL"),
        "the derived default (15) must NOT leak into either specialized sim header:\n{cpp}"
    );
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
    assert!(
        !cpp.contains(") =") || cpp.contains("== "),
        "slice-LHS regression: rvalue form must not appear:\n{cpp}"
    );
    // The clear-mask must be 32 bits (0xFFFFFFFFULL), not 33 (0x1FFFFFFFFULL).
    assert!(
        cpp.contains("0xFFFFFFFFULL"),
        "slice-LHS should use a 32-bit mask for [CounterWidth-1:0] when CounterWidth=32:\n{cpp}"
    );
    assert!(!cpp.contains("0x1FFFFFFFFULL"),
        "slice-LHS must not use a 33-bit mask for [CounterWidth-1:0] (param folding regression):\n{cpp}");
    // Sanity: the mask-and-OR shape includes `& ~(uint64_t(0x...` for the clear
    // and `| ((uint64_t(...` for the set.
    assert!(
        cpp.contains("_n_counter_q = (_n_counter_q & ~"),
        "expected mask-and-OR LHS shape:\n{cpp}"
    );
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
    let pe_start = cpp
        .find(start_marker)
        .unwrap_or_else(|| panic!("expected eval_posedge body in:\n{cpp}"));
    let pe_body = &cpp[pe_start + start_marker.len()..];
    let pe_body = pe_body.split("\n}\n").next().unwrap_or("");
    let async_pos = pe_body.find("if ((!rst_ni))");
    let rising_pos = pe_body.find("if (_rising_clk)");
    assert!(
        async_pos.is_some(),
        "async reset arm should be emitted in eval_posedge:\n{pe_body}"
    );
    assert!(
        rising_pos.is_some(),
        "rising-edge guard should be emitted in eval_posedge:\n{pe_body}"
    );
    assert!(
        async_pos.unwrap() < rising_pos.unwrap(),
        "async reset arm must precede the rising-edge gate:\n{pe_body}"
    );
    // The async arm writes to both `_q_r` (live) and `_n_q_r` (shadow).
    assert!(
        pe_body.contains("_q_r = 0;") && pe_body.contains("_n_q_r = 0;"),
        "async reset must write both live and shadow regs:\n{pe_body}"
    );
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
    assert!(
        cpp.contains("_q_r = 0;"),
        "indexed-LHS reg should still get its async reset arm:\n{cpp}"
    );
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
    assert!(
        sv.contains("case (__p_data_data)"),
        "seq-block match scrutinee should rewrite to __p_data_data:\n{sv}"
    );
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
    assert!(
        result.is_err(),
        "assigning to a `reg` in a comb-block for-loop body must be a typecheck error"
    );
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(
        err_msg.contains("`arr_r` is a reg") && err_msg.contains("seq"),
        "diagnostic should explain reg-vs-seq rule; got: {err_msg}"
    );
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
    assert!(
        sv.contains("input logic [1:0] [31:0] packed_in"),
        "packed Vec port should keep packed multi-dim shape, got: {sv}"
    );
    // unpacked Vec port flips to SV unpacked-array shape.
    assert!(
        sv.contains("input logic [31:0] unpacked_in [1:0]"),
        "unpacked Vec port should emit SV unpacked array, got: {sv}"
    );
    assert!(
        sv.contains("output logic [31:0] unpacked_out [1:0]"),
        "unpacked Vec output port should emit SV unpacked array, got: {sv}"
    );
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
    assert!(
        sv.contains("input logic [5:0] asc_in [0:3]"),
        "ascending unpacked input should emit [0:N-1], got: {sv}"
    );
    assert!(
        sv.contains("output logic [5:0] asc_out [0:3]"),
        "ascending unpacked output should emit [0:N-1], got: {sv}"
    );
    // Plain `unpacked` (no `ascending`) keeps default descending.
    assert!(
        sv.contains("input logic [5:0] desc_in [3:0]"),
        "plain unpacked stays descending, got: {sv}"
    );
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
    assert!(
        sv.contains("logic [5:0] w [0:3]"),
        "ascending unpacked wire should emit [0:N-1], got: {sv}"
    );
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
    let item = parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("expected a module item");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(
        body.contains("port asc_in: in unpacked ascending Vec"),
        ".archi should preserve `unpacked ascending`: {body}"
    );
}

#[test]
fn test_archi_bus_port_param_assignments_round_trip() {
    // A port-level bus param override (`port s: target BusRw<WRITE=0>`) must
    // survive `.archi` emit, and round-trip back through the parser into the
    // same BusPortInfo.params. Before this, the emitter dropped the override
    // (a `// TODO: bus param assignments`), so a consumer reading the `.archi`
    // (e.g. harc modeling the DUT interface) couldn't see which `generate_if`
    // channels the flattened port set actually omitted — a cross-interface
    // divergence from what `arch build` emits.
    let source = "
domain SysDomain
  freq_mhz: 100
end domain SysDomain

bus BusRw
  param READ: const = 1;
  param WRITE: const = 1;
  generate_if WRITE
    aw_valid: out Bool;
  end generate_if
  ar_valid: out Bool;
end bus BusRw

module Dut
  port s: target BusRw<WRITE=0>;
  port chans: initiator Vec<BusRw<READ=0>, 2>;
end module Dut
";
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse error");
    let dut = parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Module(m) if m.name.name == "Dut"))
        .expect("expected Dut module");
    let body = arch::interface::emit_interface(dut).expect("emit_interface");
    assert!(
        body.contains("port s: target BusRw<WRITE=0>;"),
        ".archi must record the scalar bus-port param override: {body}"
    );
    assert!(
        body.contains("port chans: initiator Vec<BusRw<READ=0>, 2>;"),
        ".archi must record params inside a Vec-of-bus port: {body}"
    );

    // Round-trip: parse the emitted `.archi` module stub back and confirm the
    // BusPortInfo.params survived (so harc / re-emit see the same override).
    let rt_src = format!("domain SysDomain\n  freq_mhz: 100\nend domain SysDomain\n\n{body}");
    let rt_tokens = lexer::tokenize(&rt_src).expect("re-lex emitted .archi");
    let mut rt_parser = Parser::new(rt_tokens, &rt_src);
    let rt_parsed = rt_parser
        .parse_source_file()
        .expect("re-parse emitted .archi");
    let rt_dut = rt_parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("module in re-parsed .archi");
    let arch::ast::Item::Module(m) = rt_dut else {
        unreachable!()
    };
    let s_port = m.ports.iter().find(|p| p.name.name == "s").expect("port s");
    let bi = s_port.bus_info.as_ref().expect("s is a bus port");
    assert!(
        bi.params.iter().any(|pa| pa.name.name == "WRITE"),
        "WRITE override must round-trip into BusPortInfo.params"
    );
    // Re-emit must be byte-identical — proves the override (incl. its value)
    // survives a full emit → parse → emit cycle, not just appears once.
    assert_eq!(
        arch::interface::emit_interface(rt_dut).unwrap(),
        body,
        ".archi emit must be idempotent across a parse round-trip"
    );
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
    let item = parsed
        .items
        .iter()
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
    assert!(
        result.is_err(),
        "unpacked on non-Vec should be a parse error"
    );
    let msg = format!("{:?}", result.err().unwrap());
    assert!(
        msg.contains("`unpacked` is only valid on `Vec<T,N>` ports"),
        "diagnostic should explain Vec-only restriction, got: {msg}"
    );
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
    assert!(
        result.is_err(),
        "unpacked + port reg should be a parse error"
    );
    let msg = format!("{:?}", result.err().unwrap());
    assert!(
        msg.contains("`unpacked` is not allowed on `port reg`"),
        "diagnostic should explain port-reg restriction, got: {msg}"
    );
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
            s.contains("RDC violation")
                && s.contains("rst")
                && s.contains("DomA")
                && s.contains("DomB")
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
    assert!(
        result.is_ok(),
        "expected no RDC error, got: {:?}",
        result.err()
    );
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
    assert!(
        result.is_ok(),
        "expected guard-waivered RDC to pass, got: {:?}",
        result.err()
    );
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
    assert!(
        result.is_err(),
        "expected RDC error (sync guard doesn't qualify)"
    );
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("RDC violation") && s.contains("data_q") && s.contains("not async-reset")
        }),
        "expected hint about non-async guard, got: {:?}",
        errs
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
    assert!(
        result.is_err(),
        "expected RDC error (port guard doesn't qualify)"
    );
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("RDC violation")
                && s.contains("data_q")
                && s.contains("not a register in this module")
        }),
        "expected hint about port-input guard, got: {:?}",
        errs
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
    assert!(
        result.is_err(),
        "expected RDC error (cross-domain guard doesn't waive)"
    );
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("RDC violation") && s.contains("data_q")
        }),
        "expected RDC error on data_q, got: {:?}",
        errs
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
    assert!(
        r.is_ok(),
        "[{label}] expected no RDC error, got: {:?}",
        r.err()
    );
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
    assert!(
        any_match,
        "[{label}] expected RDC/CDC error containing all of {:?}, got: {:?}",
        must_contain, errs
    );
}

// ── Group A: direct edges (1-hop) ───────────────────────────────────────────

#[test]
fn rdc_a1_same_async_direct_ok() {
    // ra (rst_a, async) → rb (rst_a, async); same domain → no violation.
    assert_rdc_ok(
        "A1",
        r#"
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
"#,
    );
}

#[test]
fn rdc_reset_type_cast_at_inst_is_direct_reset_ok() {
    // `rst <- rst_async_n as Reset<Async, Low>` is a reset type override at
    // the inst boundary. It should not be classified as reset-combining logic.
    assert_rdc_ok(
        "reset-cast-inst",
        include_str!("../examples/param_reset.arch"),
    );
}

#[test]
fn rdc_a2_diff_async_direct_fails() {
    // ra (rst_a, async) → rb (rst_b, async); different async domains → FAIL.
    assert_rdc_fails(
        "A2",
        r#"
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
"#,
        &["rst_a", "rst_b"],
    );
}

#[test]
fn rdc_a3_async_to_sync_fails() {
    // ra (rst_a, async) → rb (rst_b, sync). Strict rule: sync is
    // transparent for propagation but cannot gate its data input on the
    // upstream's async reset event; mid-deassert transients on `ra`
    // metastabilise `rb` and propagate downstream. Flag.
    assert_rdc_fails(
        "A3",
        r#"
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
"#,
        &["rst_a", "rb"],
    );
}

#[test]
fn rdc_a4_async_to_none_fails() {
    // ra (rst_a, async) → rb (reset none). Strict rule: a reset-less
    // flop cannot gate its data input on the source's async reset
    // event; mid-deassert transients on `ra` metastabilise `rb`. Flag.
    assert_rdc_fails(
        "A4",
        r#"
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
"#,
        &["rst_a", "rb"],
    );
}

#[test]
fn rdc_a5_sync_source_ok() {
    // ra (rst_a, sync) sourced from a port has reach[ra]=∅. Then rb
    // (rst_b, async) reads ra → reach[rb's src]=∅ → no violation.
    assert_rdc_ok(
        "A5",
        r#"
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
"#,
    );
}

// ── Group B: 2-hop chains (the canonical reset-less / sync-bridge bug) ─────

#[test]
fn rdc_b1_async_none_async_diff_fails() {
    // ra (rst_a) → rx (none) → rb (rst_b). reach[rx]={rst_a};
    // reach[rb's src]={rst_a} ≠ rb.reset=rst_b → FAIL at rb.
    assert_rdc_fails(
        "B1",
        r#"
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
"#,
        &["rst_a", "rst_b"],
    );
}

#[test]
fn rdc_b2_async_none_async_same_fails() {
    // ra (rst_a) → rx (reset none) → rb (rst_a). Strict rule: the
    // intermediate reset-less `rx` captures async-domain data without
    // being gated on the upstream reset; even though both async flops
    // share rst_a, the middle hop is the metastability propagator.
    // Fix is to also reset `rx` by rst_a (or add a synchroniser).
    assert_rdc_fails(
        "B2",
        r#"
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
"#,
        &["rst_a", "rx"],
    );
}

#[test]
fn rdc_b3_async_sync_async_diff_fails() {
    // Sync rx is transparent like none → still flagged.
    assert_rdc_fails(
        "B3",
        r#"
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
"#,
        &["rst_a", "rst_b"],
    );
}

// ── Group C: convergence at non-async flop ─────────────────────────────────

#[test]
fn rdc_c1_two_async_converge_at_none_fails() {
    // ra (rst_a) and rb (rst_b) both feed rx (none).
    // reach[rx]={rst_a, rst_b} → FAIL at rx.
    assert_rdc_fails(
        "C1",
        r#"
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
"#,
        &["rst_a", "rst_b"],
    );
}

#[test]
fn rdc_c2_two_same_domain_converge_fails() {
    // Both async sources are rst_a, converging at rx (reset none).
    // Strict rule: rx is reset-less, captures async-domain data without
    // gating on the upstream reset event → flag, even though only one
    // async domain reaches it. The fix is to also reset `rx` by rst_a.
    assert_rdc_fails(
        "C2",
        r#"
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
"#,
        &["rst_a", "rx"],
    );
}

#[test]
fn rdc_c3_async_plus_port_at_none_fails() {
    // ra (rst_a) + port input → rx (reset none). Port contributes no
    // async, but rx still captures async-domain data from `ra` without
    // a reset gate — flag.
    assert_rdc_fails(
        "C3",
        r#"
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
"#,
        &["rst_a", "rx"],
    );
}

// ── Group D: multi-clock-domain interactions ───────────────────────────────

#[test]
fn rdc_d1_same_async_two_clocks_no_data_path_phase1_flags() {
    // Phase 1 (currently shipped) flags this — shared async reset across
    // two clock domains, regardless of whether a data path exists. Phase
    // 2's data-path rule alone would let this pass; we keep phase 1 as a
    // structural backstop so the test pins the union of both checks.
    assert_rdc_fails(
        "D1",
        r#"
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
"#,
        &["rst", "DA", "DB"],
    );
}

#[test]
fn rdc_d2_diff_async_diff_clocks_with_path_fails() {
    // Two clocks, two async resets, data path between them → FAIL.
    // Module marks itself `cdc_safe` to opt out of the CDC check (which
    // would otherwise also fire on this design); RDC must still flag.
    assert_rdc_fails(
        "D2",
        r#"
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
"#,
        &["rst_a", "rst_b"],
    );
}

// ── Group E: feedback loops (require fixpoint) ─────────────────────────────

#[test]
fn rdc_e1_self_loop_same_domain_ok() {
    assert_rdc_ok(
        "E1",
        r#"
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
"#,
    );
}

#[test]
fn rdc_e2_mutual_feedback_diff_domains_fails() {
    // ra ↔ rb across different async domains. Fixpoint converges with
    // reach[rb's src]={rst_a} and reach[ra's src]={rst_b}; both flagged.
    assert_rdc_fails(
        "E2",
        r#"
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
"#,
        &["rst_a", "rst_b"],
    );
}

// ── Group F: trivial / sanity ──────────────────────────────────────────────

#[test]
fn rdc_f1_single_async_domain_ok() {
    // Several flops all reset by rst_a → no violation.
    assert_rdc_ok(
        "F1",
        r#"
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
"#,
    );
}

#[test]
fn rdc_f2_no_async_flops_ok() {
    // All sync — phase-2 rule originates no domain → no violation.
    assert_rdc_ok(
        "F2",
        r#"
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
"#,
    );
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
    let src =
        std::fs::read_to_string("tests/rdc/rdc_h1_reconvergent_two_syncs_same_domain_fail.arch")
            .expect("read H1");
    assert_rdc_fails("H1", &src, &["raw_rst", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_h2_single_reset_sync_ok() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_h2_single_reset_sync_ok.arch").expect("read H2");
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
    let src =
        std::fs::read_to_string("tests/rdc/rdc_h4_reconvergent_three_syncs_same_domain_fail.arch")
            .expect("read H4");
    assert_rdc_fails("H4", &src, &["raw_rst", "sync_1", "Dst"]);
}

// ── Group J: reconvergent CDC (and mixed) — same generalised check ────────
// Same hazard shape as group H but with non-reset synchroniser kinds.

#[test]
fn rdc_j1_cdc_reconvergent_two_ff_syncs_same_domain_fails() {
    let src = std::fs::read_to_string(
        "tests/rdc/rdc_j1_cdc_reconvergent_two_ff_syncs_same_domain_fail.arch",
    )
    .expect("read J1");
    assert_rdc_fails("J1", &src, &["CDC", "flag", "sync_a", "sync_b", "Dst"]);
}

#[test]
fn rdc_j2_cdc_single_ff_sync_ok() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_j2_cdc_single_ff_sync_ok.arch").expect("read J2");
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
    let src = std::fs::read_to_string(
        "tests/rdc/rdc_j4_mixed_reset_and_data_sync_same_source_same_domain_fail.arch",
    )
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
    assert!(
        sv.contains("localparam int NARROW = 42;"),
        "untyped const must keep `int`:\n{sv}"
    );
    // Width-qualified params must keep the [hi:lo] qualifier.
    assert!(
        sv.contains("localparam [31:0] WIDE32 = 42;"),
        "32-bit width qualifier dropped:\n{sv}"
    );
    assert!(
        sv.contains("localparam [63:0] WIDE64 = 24314014034;"),
        "64-bit width qualifier dropped (would truncate):\n{sv}"
    );
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
    let typedef_pos = sv.find("typedef enum").expect("typedef enum missing");
    let param_pos = sv
        .find("DEFAULT_OP")
        .expect("DEFAULT_OP localparam missing");
    assert!(
        typedef_pos < param_pos,
        "enum typedef must precede localparam that references it:\n{sv}"
    );
    assert!(
        sv.contains("localparam Op DEFAULT_OP = "),
        "EnumConst must emit typed `localparam Op …`:\n{sv}"
    );
}

// ── Group K: Phase 2d — combiner-derived reset glitches at inst boundaries
// A sub-module Reset input wired by a combinational expression (rst_a | rst_b,
// not rst_a, etc.) sees glitches on edge skew and can trigger partial resets.

#[test]
fn rdc_k1_combiner_or_at_inst_fails() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_k1_combiner_or_at_inst_fail.arch").expect("read K1");
    assert_rdc_fails("K1", &src, &["sub", "rst", "combinational"]);
}

#[test]
fn rdc_k2_negation_at_inst_fails() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_k2_negation_at_inst_fail.arch").expect("read K2");
    assert_rdc_fails("K2", &src, &["sub", "rst", "combinational"]);
}

#[test]
fn rdc_k3_direct_reset_at_inst_ok() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_k3_direct_reset_at_inst_ok.arch").expect("read K3");
    assert_rdc_ok("K3", &src);
}

#[test]
fn rdc_k4_sync_output_to_reset_ok() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_k4_sync_output_to_reset_ok.arch").expect("read K4");
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
    let src =
        std::fs::read_to_string("tests/rdc/rdc_m5_cdc_distinct_sources_ok.arch").expect("read M5");
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
    let src =
        std::fs::read_to_string("tests/rdc/rdc_l1_pragma_rdc_safe_suppresses_phase2a_ok.arch")
            .expect("read L1");
    assert_rdc_ok("L1", &src);
}

#[test]
fn rdc_l2_pragma_rdc_safe_suppresses_phase2c() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_l2_pragma_rdc_safe_suppresses_phase2c_ok.arch")
            .expect("read L2");
    assert_rdc_ok("L2", &src);
}

#[test]
fn rdc_l3_pragma_rdc_safe_suppresses_phase2d() {
    let src =
        std::fs::read_to_string("tests/rdc/rdc_l3_pragma_rdc_safe_suppresses_phase2d_ok.arch")
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
    assert!(
        msg.contains("unknown pragma") && msg.contains("totally_unsafe"),
        "expected unknown-pragma diagnostic, got: {msg}"
    );
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
            if m.name.name == "ChildStub" {
                m.is_interface = true;
            }
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
    let (_warnings, overload_map) = checker
        .check()
        .expect("typecheck must not report 'output port out_o is not driven' on interface stub");
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
    let sv = codegen.generate();
    assert!(
        sv.contains("module Parent"),
        "parent module should be emitted"
    );
    assert!(
        !sv.contains("module ChildStub"),
        "interface stub must not be emitted to SV (real impl lives in a separately-built file)"
    );
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
    let mut parsed_ast = parser
        .parse_source_file()
        .expect("parser must accept body-less fsm (interface stub)");
    // Mimic main.rs's post-parse tagger: items loaded from `.archi` get
    // is_interface = true. Here we tag the FSM by name to simulate
    // "loaded from <name>.archi".
    for item in parsed_ast.items.iter_mut() {
        if let arch::ast::Item::Fsm(f) = item {
            if f.name.name == "ChildFsm" {
                f.common.is_interface = true;
            }
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
    let (_warnings, overload_map) = checker
        .check()
        .expect("typecheck must skip body checks on fsm interface stub");
    let mut codegen = Codegen::new(&symbols, &ast, overload_map);
    let sv = codegen.generate();
    assert!(
        sv.contains("module Parent"),
        "parent module should be emitted"
    );
    assert!(
        !sv.contains("module ChildFsm"),
        "fsm interface stub must not be emitted to SV (real impl lives in a separately-built file)"
    );
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
    let parsed_ast = parser
        .parse_source_file()
        .expect("parser accepts body-less fsm now; default-state check moved to resolve");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate");
    let resolve_err = resolve::resolve(&ast)
        .err()
        .expect("real fsm without default_state must still error");
    let msg = format!("{resolve_err:?}");
    assert!(
        msg.contains("default state"),
        "expected `default state` diagnostic, got: {msg}"
    );
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
    assert!(
        sv.contains("package SharedPkg;"),
        "package SharedPkg should be emitted to SV"
    );
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
    let line_start = preceding[..last_begin_at]
        .rfind('\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let begin_line = &preceding[line_start..last_begin_at + "begin".len()];
    assert!(
        !begin_line.contains("if (cond_b)"),
        "inter-yield seq assign `x <= 8'd42` must NOT be wrapped in \
         `if (cond_b) begin ... end` — that's the pre-fix merge-into-wait-state \
         behavior, which conflicts with spec §7a.2 (only TRAILING assigns merge). \
         Enclosing begin-line was: {:?}\nFull SV:\n{}",
        begin_line,
        sv
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_fuses_next_action() {
    // Canonical fast-path idiom:
    //
    //   if not start
    //     wait until start;
    //   end if
    //   phase <= 1;
    //
    // If `start` is already high while the thread is in S0, the assignment
    // must fire on that same edge. Pre-fix lowering skipped the wait state but
    // still emitted a separate S1 action state, inserting a one-cycle bubble.
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port phase: out UInt<8>;
  reg phase_r: UInt<8> reset rst => 8'd0;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    phase_r <= 8'd1;
    wait 1 cycle;
    phase_r <= 8'd2;
  end thread

  comb
    phase = phase_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);

    assert!(
        !sv.contains("_t0_S0_dispatch"),
        "fast-path if/wait should lower as a wait state, not dispatch around a wait:\n{sv}"
    );
    assert!(
        sv.contains("localparam [0:0] _t0_S0_wait_until = 0"),
        "expected canonical fast-path state to remain a wait_until state:\n{sv}"
    );
    assert!(
        sv.contains("localparam [0:0] _t0_S1_action = 1"),
        "expected post-wait action state for work after `wait 1 cycle`:\n{sv}"
    );

    let s0_marker = "if (_t0_state == _t0_S0_wait_until) begin";
    let s0_start = sv.find(s0_marker).unwrap_or_else(|| {
        panic!("missing S0 wait branch in SV:\n{sv}");
    });
    let after_s0 = &sv[s0_start + s0_marker.len()..];
    let s1_rel = after_s0
        .find("if (_t0_state == _t0_S1_action) begin")
        .unwrap_or_else(|| {
            panic!("missing S1 action branch after S0 in SV:\n{sv}");
        });
    let s0_branch = &sv[s0_start..s0_start + s0_marker.len() + s1_rel];

    assert!(
        s0_branch.contains("if (start) begin") && s0_branch.contains("phase_r <= 8'd1"),
        "phase_r <= 1 must fire in the S0/start transition branch:\n{s0_branch}\nFull SV:\n{sv}"
    );
    assert!(
        !s0_branch.contains("phase_r <= 8'd2"),
        "`wait 1 cycle` after the fast-path action must keep the next action out of S0:\n{s0_branch}\nFull SV:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_wait_cycle() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port phase: out UInt<8>;
  reg phase_r: UInt<8> reset rst => 8'd0;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    wait 1 cycle;
    phase_r <= 8'd1;
  end thread

  comb
    phase = phase_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        sv.contains("_t0_S0_wait_until") && sv.contains("_t0_S1_action"),
        "fast gate followed by `wait 1 cycle` should keep the fast wait and emit a later action state:\n{sv}"
    );
    assert!(
        !trimmed.contains("_t0_state == _t0_S0_wait_until) begin if (start) begin phase_r <= 8'd1"),
        "`wait 1 cycle` after the fast gate must prevent the trailing assign from merging into S0:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_plain_wait_until() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port ready: in Bool;
  port phase: out UInt<8>;
  reg phase_r: UInt<8> reset rst => 8'd0;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    wait until ready;
    phase_r <= 8'd1;
  end thread

  comb
    phase = phase_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);

    assert!(
        sv.contains("localparam [1:0] _t0_S0_wait_until = 0")
            || sv.contains("localparam [0:0] _t0_S0_wait_until = 0"),
        "expected S0 fast wait state:\n{sv}"
    );
    assert!(
        sv.contains("_t0_S1_wait_until"),
        "fast gate followed by an ordinary `wait until` should emit a second wait state:\n{sv}"
    );
    assert!(
        sv.contains("if (ready) begin\n          phase_r <= 8'd1"),
        "trailing assignment should merge into the second wait's ready edge:\n{sv}"
    );
    assert!(
        !sv.contains("start && ready"),
        "the following ordinary `wait until ready` should not be fused into the fast gate:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_same_state_if() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port sel: in Bool;
  port phase: out UInt<8>;
  reg phase_r: UInt<8> reset rst => 8'd0;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    if sel
      phase_r <= 8'd1;
    else
      phase_r <= 8'd2;
    end if
    wait 1 cycle;
  end thread

  comb
    phase = phase_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        trimmed.contains("_t0_state == _t0_S0_wait_until) begin if (start) begin if (sel) begin phase_r <= 8'd1")
            && trimmed.contains("end else begin phase_r <= 8'd2"),
        "same-state if/else after fast gate should execute in the S0/start transition branch:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_comb_assign() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port done: out Bool shared(or);

  thread on clk rising, rst high
    default comb
      done = false;
    end default
    if not start
      wait until start;
    end if
    done = true;
    wait 1 cycle;
  end thread
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        trimmed.contains(
            "if (_t0_state == _t0_S0_wait_until) begin if (start) begin done = done | 1'b1"
        ),
        "comb assign after fast gate should be gated by start in the S0 comb block:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_if_with_waits() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port sel: in Bool;
  port phase: out UInt<8>;
  reg phase_r: UInt<8> reset rst => 8'd0;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    if sel
      phase_r <= 8'd1;
      wait 1 cycle;
    else
      phase_r <= 8'd2;
      wait 1 cycle;
    end if
    phase_r <= 8'd3;
  end thread

  comb
    phase = phase_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);

    assert!(
        sv.contains("start && sel") && sv.contains("phase_r <= 8'd1"),
        "then-branch first action should fuse onto the start edge under start && sel:\n{sv}"
    );
    assert!(
        sv.contains("start && !sel") && sv.contains("phase_r <= 8'd2"),
        "else-branch first action should fuse onto the start edge under start && !sel:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_second_fast_gate_do_until() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port go: in Bool;
  port done: in Bool;
  port phase: out UInt<8>;
  reg phase_r: UInt<8> reset rst => 8'd0;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    if not (go)
      wait until go;
    end if
    do
      phase_r <= 8'd1;
    until done;
  end thread

  comb
    phase = phase_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        trimmed.contains(
            "_t0_state == _t0_S0_wait_until) begin if (start) begin _t0_state <= _t0_S1_wait_until"
        ),
        "S0 should wait for start before entering the following fast-gate fused state:\n{sv}"
    );
    assert!(
        trimmed.contains("_t0_state == _t0_S1_wait_until) begin if (go) begin phase_r <= 8'd1")
            && sv.contains("go && done"),
        "following fast-gate/do-until should keep its Mealy gating after the fast start gate:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_for_loop() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port phase: out UInt<8>;
  reg phase_r: UInt<8> reset rst => 8'd0;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    for i in 0..1
      phase_r <= i.zext<8>();
      wait 1 cycle;
    end for
    phase_r <= 8'd9;
  end thread

  comb
    phase = phase_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        trimmed.contains("_t0_state == _t0_S0_wait_until) begin if (start) begin _t0_loop_cnt_0 <="),
        "for-loop counter init after fast gate should happen on the S0/start transition edge:\n{sv}"
    );
    assert!(
        !trimmed.contains("_t0_state == _t0_S0_wait_until) begin if (start) begin phase_r <="),
        "for-loop body should remain in later loop states, not collapse into the fast gate:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_lock() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port start: in Bool;
  port done: out Bool shared(or);

  resource shared_lk: mutex<priority>;

  thread on clk rising, rst low
    if not start
      wait until start;
    end if
    lock shared_lk
      done = true;
      wait 1 cycle;
    end lock shared_lk
  end thread
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        sv.contains("_t0_S0_wait_until") && sv.contains("_t0_S1_wait_until"),
        "lock after fast gate should preserve the start wait before entering lock arbitration:\n{sv}"
    );
    assert!(
        trimmed.contains(
            "_t0_state == _t0_S0_wait_until) begin if (start) begin _t0_state <= _t0_S1_wait_until"
        ),
        "S0 should transition into the lock state only when start is true:\n{sv}"
    );
}

#[test]
fn test_thread_if_not_wait_until_fast_path_followed_by_fork_join() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port start: in Bool;
  port out_a: out Bool;
  port out_b: out Bool;
  reg a_r: Bool reset rst => false;
  reg b_r: Bool reset rst => false;

  thread on clk rising, rst high
    if not start
      wait until start;
    end if
    fork
      a_r <= true;
      wait 1 cycle;
    and
      b_r <= true;
      wait 1 cycle;
    join
  end thread

  comb
    out_a = a_r;
    out_b = b_r;
  end comb
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        sv.contains("_t0_S0_wait_until") && sv.contains("_t0_S1_action"),
        "fork/join after fast gate should preserve the start wait and enter fork product states after it:\n{sv}"
    );
    assert!(
        trimmed.contains(
            "_t0_state == _t0_S0_wait_until) begin if (start) begin _t0_state <= _t0_S1_action"
        ),
        "S0 should transition into fork/join lowering only when start is true:\n{sv}"
    );
    assert!(
        !trimmed.contains("_t0_state == _t0_S0_wait_until) begin if (start) begin a_r <= 1'b1")
            && !trimmed
                .contains("_t0_state == _t0_S0_wait_until) begin if (start) begin b_r <= 1'b1"),
        "fork branch bodies should remain in fork product states, not collapse into S0:\n{sv}"
    );
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
    assert!(
        sv.contains("logic [31:0] bridge [1:0]") || sv.contains("logic [31:0] bridge [0:1]"),
        "expected unpacked wire shape `logic [31:0] bridge [N-1:0]`, got:\n{}",
        sv
    );
    assert!(
        !sv.contains("logic [1:0][31:0] bridge"),
        "must NOT emit packed multi-dim for `unpacked` wire, got:\n{}",
        sv
    );
    // Parent's port still uses unpacked (sanity).
    assert!(
        sv.contains("input logic [31:0] pq [1:0]") || sv.contains("input logic [31:0] pq [0:1]"),
        "expected unpacked port shape on parent, got:\n{}",
        sv
    );
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
    let item = parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Cam(_)))
        .expect("expected a cam item");
    let body =
        arch::interface::emit_interface(item).expect("cam should now emit an .archi interface");
    assert!(body.starts_with("cam TestCam\n"), "body: {body}");
    assert!(body.contains("param DEPTH: const = 8;"), "body: {body}");
    assert!(
        body.contains("port search_first: out UInt<3>;"),
        "body: {body}"
    );
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
    let item = parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Arbiter(_)))
        .expect("expected an arbiter item");
    let body =
        arch::interface::emit_interface(item).expect("arbiter should emit an .archi interface");
    assert!(
        body.contains("ports[NUM_REQ] request"),
        ".archi must include the ports[N] group: {body}"
    );
    assert!(
        body.contains("    valid: in Bool;"),
        ".archi must include per-requester valid signal: {body}"
    );
    assert!(
        body.contains("    ready: out Bool;"),
        ".archi must include per-requester ready signal: {body}"
    );
    assert!(
        body.contains("  end ports request"),
        ".archi ports group must be properly closed: {body}"
    );
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
    assert!(
        sv.contains("logic [3:0] __arb_request_valid;"),
        "expected synthesized vector wire: {sv}"
    );
    // Each bit of the wire is driven from the user's per-index
    // expression.
    assert!(
        sv.contains("assign __arb_request_valid[0] = req0;"),
        "expected per-index drive [0]: {sv}"
    );
    assert!(
        sv.contains("assign __arb_request_valid[3] = req3;"),
        "expected per-index drive [3]: {sv}"
    );
    // The whole vector is connected to the inst's `request_valid` port.
    assert!(
        sv.contains(".request_valid(__arb_request_valid)"),
        "expected whole-vector connection: {sv}"
    );
    // The non-existent flattened port names must NOT appear.
    assert!(
        !sv.contains(".request0_valid("),
        "must not emit per-index port name: {sv}"
    );
    assert!(
        !sv.contains(".request3_valid("),
        "must not emit per-index port name: {sv}"
    );
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
    assert!(
        sv.contains("parameter int TagW = 22"),
        "user param TagW must appear in SV header: {sv}"
    );
    // DATA_WIDTH is derived from the store element type. Default may
    // be the symbolic param `TagW` (forward-resolves via the user
    // param decl above) or the literal `22`; either is correct SV.
    assert!(
        sv.contains("parameter int DATA_WIDTH = TagW")
            || sv.contains("parameter int DATA_WIDTH = 22"),
        "DATA_WIDTH should follow the store element width: {sv}"
    );
    // Port refs to TagW now resolve.
    assert!(
        sv.contains("[TagW-1:0]"),
        "port type must keep referencing TagW: {sv}"
    );
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
    assert!(
        sv.contains("parameter int Foo = 32"),
        "regular param should still emit: {sv}"
    );
    assert!(
        sv.contains("localparam int Bar = 64"),
        "local param after doc comment should still emit: {sv}"
    );
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
    let item = parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("expected a module item");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(
        body.contains("port a: in unpacked Vec<UInt<W>, N>;"),
        "unpacked input port should round-trip into .archi: {body}"
    );
    // Issue #246 Phase 2: output ports may pick up a `comb_dep_on(...)`
    // suffix listing the precise input ports that feed each output.
    assert!(
        body.contains("port b: out unpacked Vec<UInt<W>, N>"),
        "unpacked output port should round-trip into .archi: {body}"
    );
    // Packed Vec port (no `unpacked` modifier) still emits without it.
    assert!(
        body.contains("port c: in Vec<UInt<W>, N>;"),
        "packed Vec port must NOT gain the `unpacked` keyword: {body}"
    );
    assert!(
        !body.contains("port c: in unpacked Vec"),
        "packed Vec port must NOT gain the `unpacked` keyword: {body}"
    );
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
    let err = parser
        .parse_source_file()
        .expect_err("must reject `unpacked UInt<32>`");
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("unpacked") && msg.contains("Vec"),
        "error should mention the `unpacked` + Vec constraint, got: {}",
        msg
    );
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
    assert!(
        !sv.contains("_auto_bound_vec_"),
        "no bound assertion expected when the only Vec index is a for-loop iterator:\n{sv}"
    );
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
    assert!(
        sv.contains("output logic [3:0] en_v"),
        "Vec<UInt<1>, 4> should emit as `logic [3:0]` (no inner [0:0]):\n{sv}"
    );
    assert!(
        !sv.contains("[3:0] [0:0]"),
        "no `[N-1:0] [0:0]` multi-dim form expected for Vec<UInt<1>, _>:\n{sv}"
    );
    // Vec<Bool, 4> behaves the same (Bool is 1-bit) — sanity check the
    // emission stays single-packed (was always `logic [3:0]` pre-fix
    // because Bool's emit_type_str returns just `logic`).
    assert!(
        sv.contains("output logic [3:0] mask"),
        "Vec<Bool, 4> should still emit as `logic [3:0]`:\n{sv}"
    );
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
    assert!(
        out.contains("uint64_t score_out"),
        "score_out port should be uint64_t for UInt<48>; got:\n{out}"
    );
    assert!(
        out.contains("uint64_t inc_in"),
        "inc_in port should be uint64_t for UInt<48>; got:\n{out}"
    );
    assert!(
        out.contains("uint64_t _accumulator"),
        "accumulator reg should be uint64_t for UInt<48>; got:\n{out}"
    );
    assert!(
        out.contains("uint64_t _score_reg"),
        "score_reg reg should be uint64_t for UInt<48>; got:\n{out}"
    );

    // _n_ shadow should match.
    assert!(
        out.contains("uint64_t _n_accumulator"),
        "_n_accumulator shadow should be uint64_t; got:\n{out}"
    );

    // Truncating arithmetic should mask to 48 bits (12 F's), not 32.
    assert!(
        out.contains("0xFFFFFFFFFFFFULL"),
        "expected 48-bit mask 0xFFFFFFFFFFFFULL; got:\n{out}"
    );
    assert!(
        !out.contains(" 0xFFFFFFFFULL"),
        "must not emit 32-bit mask 0xFFFFFFFFULL for 48-bit accumulator; got:\n{out}"
    );

    // The seq-assign cast must be (uint64_t), not (uint32_t).
    assert!(
        out.contains("(uint64_t)((((_accumulator + inc_in))"),
        "trunc cast should be (uint64_t)(...); got:\n{out}"
    );
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
    assert!(
        out.contains("uint32_t a32"),
        "UInt<32> port should be uint32_t; got:\n{out}"
    );
    assert!(
        out.contains("uint64_t a33"),
        "UInt<33> port should be uint64_t; got:\n{out}"
    );
    assert!(
        out.contains("uint64_t a64"),
        "UInt<64> port should be uint64_t; got:\n{out}"
    );
    // 65 bits → wide (VlWide). Don't pin the exact word count here — just
    // assert it isn't the legacy uint32_t bucket.
    assert!(
        out.contains("VlWide") && out.contains("a65"),
        "UInt<65> port should be VlWide<...>; got:\n{out}"
    );
    assert!(
        !out.contains("uint32_t a65"),
        "UInt<65> must not be uint32_t; got:\n{out}"
    );
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
    assert!(
        out.contains("uint64_t a"),
        "UInt<48> port should be uint64_t; got:\n{out}"
    );
    assert!(
        out.contains("uint64_t inc"),
        "UInt<48> port should be uint64_t; got:\n{out}"
    );
    assert!(
        out.contains("uint64_t _r"),
        "UInt<48> reg should be uint64_t; got:\n{out}"
    );
    assert!(
        out.contains("0xFFFFFFFFFFFFULL"),
        "expected 48-bit mask; got:\n{out}"
    );
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

    assert!(
        out.contains("int64_t score_out"),
        "SInt<40> output port should use int64_t storage; got:\n{out}"
    );
    assert!(
        out.contains("int64_t inc_in"),
        "SInt<40> input port should use int64_t storage; got:\n{out}"
    );
    assert!(
        out.contains("int64_t _accumulator"),
        "SInt<40> internal reg should use int64_t storage; got:\n{out}"
    );
    assert!(
        out.contains("int64_t _score_reg"),
        "SInt<40> internal reg should use int64_t storage; got:\n{out}"
    );
    assert!(
        out.contains("int64_t _n_accumulator"),
        "SInt<40> _n_ shadow should use int64_t storage; got:\n{out}"
    );
    assert!(
        !out.contains("uint32_t _accumulator"),
        "SInt<40> accumulator must not fall into uint32_t storage; got:\n{out}"
    );
    assert!(
        !out.contains("uint64_t _accumulator"),
        "SInt<40> accumulator must not use unsigned 64-bit storage; got:\n{out}"
    );

    assert!(
        out.contains("0xFFFFFFFFFFULL"),
        "SInt<40> trunc should still mask to exactly 40 bits; got:\n{out}"
    );
    assert!(
        out.contains(
            "((int64_t)(((uint64_t)((_accumulator + inc_in)) & 0xFFFFFFFFFFULL) << 24) >> 24)"
        ),
        "SInt<40> trunc should sign-extend from bit 39 into int64_t; got:\n{out}"
    );
}

#[test]
fn test_native_sim_signed_multiply_widens_before_cpp_promotion() {
    let source = r#"
        module SignedMulNative
          port a: in UInt<16>;
          port b: in UInt<16>;
          port out: out UInt<34>;
          port wrap_a: in UInt<32>;
          port wrap_b: in UInt<32>;
          port wrap_out: out UInt<32>;

          function Mul17(sign_a: Bool, op_a: UInt<16>,
                         sign_b: Bool, op_b: UInt<16>) -> UInt<34>
            let a17: SInt<17> = signed({sign_a, op_a});
            let b17: SInt<17> = signed({sign_b, op_b});
            let prod: SInt<34> = a17 * b17;
            return unsigned(prod);
          end function Mul17

          comb
            out = Mul17(false, a, false, b);
            wrap_out = wrap_a *% wrap_b;
          end comb
        end module SignedMulNative
    "#;

    let out = compile_to_sim_h(source, false);
    assert!(
        out.contains("(((__int128_t)(a17)) * ((__int128_t)(b17)))"),
        "native sim must widen signed multiply operands before C++ promotion; got:\n{out}"
    );
    assert!(
        !out.contains("int64_t prod = (a17 * b17);"),
        "native sim must not multiply SInt<17> operands in 32-bit C++ int; got:\n{out}"
    );
    assert!(
        out.contains("0xFFFFFFFFULL")
            && out.contains("(((_arch_u128)(wrap_a)) * ((_arch_u128)(wrap_b)))"),
        "native sim *% must widen operands and wrap back to the result width; got:\n{out}"
    );
}

#[test]
fn test_native_sim_all_wide_port_module_ctor_is_valid_cpp() {
    // Regression: a pure-comb module whose only members are wide (VlWide)
    // ports has an empty member-init list. The native-sim constructor must
    // omit the `:` entirely — a bare `Class() :  {` is a C++ syntax error
    // (dangling colon with no initializers). VlWide members self-init via
    // VlWide's default constructor, so no explicit init is needed.
    let source = r#"
        module WidePass
          port a: in UInt<70>;
          port p: out UInt<70>;
          comb
            p = a;
          end comb
        end module WidePass
    "#;

    let out = compile_to_sim_h(source, false);
    assert!(
        !out.contains("VWidePass() :  {") && !out.contains("VWidePass() : {"),
        "native sim must not emit a dangling-colon constructor for all-wide modules; got:\n{out}"
    );
    assert!(
        out.contains("VWidePass() {"),
        "native sim must emit a bare `VWidePass() {{` when there are no scalar inits; got:\n{out}"
    );
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
    assert!(
        sv.contains("logic signed [W-1:0] acc;"),
        "parent-side wire for thread-driven SInt reg should keep signedness/width:\n{sv}"
    );
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

    assert!(
        out.contains("int64_t _let_score_wire"),
        "SInt<40> child output wire should use int64_t storage in wrapper/native sim; got:\n{out}"
    );
    assert!(
        !out.contains("uint32_t _let_score_wire"),
        "SInt<40> child output wire must not use uint32_t storage; got:\n{out}"
    );
    assert!(
        !out.contains("uint64_t _let_score_wire"),
        "SInt<40> child output wire must not use unsigned storage; got:\n{out}"
    );
    assert!(
        out.contains("score  = _let_score_wire"),
        "wrapper should forward the signed child output to its public port; got:\n{out}"
    );
}

#[test]
fn test_local_param_names_are_scoped_per_instantiated_module() {
    let source = r#"
        module MulA
          local param A_WIDTH: const = 16;
          local param B_WIDTH: const = 16;
          local param PRODUCT_WIDTH: const = A_WIDTH + B_WIDTH;

          port x: in SInt<A_WIDTH>;
          port y: in SInt<B_WIDTH>;
          port z: out SInt<PRODUCT_WIDTH>;

          let product_raw: SInt<PRODUCT_WIDTH> = x * y;
          let z = product_raw;
        end module MulA

        module MulB
          local param A_WIDTH: const = 22;
          local param B_WIDTH: const = 16;
          local param PRODUCT_WIDTH: const = A_WIDTH + B_WIDTH;

          port x: in SInt<A_WIDTH>;
          port y: in SInt<B_WIDTH>;
          port z: out SInt<PRODUCT_WIDTH>;

          let product_raw: SInt<PRODUCT_WIDTH> = x * y;
          let z = product_raw;
        end module MulB

        module Top
          port a_x: in SInt<16>;
          port a_y: in SInt<16>;
          port b_x: in SInt<22>;
          port b_y: in SInt<16>;
          port a_z: out SInt<32>;
          port b_z: out SInt<38>;

          inst mul_a: MulA
            x <- a_x;
            y <- a_y;
            z -> a_z;
          end inst mul_a

          inst mul_b: MulB
            x <- b_x;
            y <- b_y;
            z -> b_z;
          end inst mul_b
        end module Top
    "#;

    let sv = compile_to_sv(source);
    assert!(
        sv.contains("output logic signed [37:0] b_z"),
        "Top's MulB output should remain 38 bits after MulB's PRODUCT_WIDTH resolves locally:\n{sv}"
    );
}

#[test]
fn test_native_sim_does_not_emit_fields_for_param_inst_inputs() {
    let source = r#"
        module Child
          param LEN_WIDTH: const = 8;
          port len: in UInt<LEN_WIDTH>;
          port out: out Bool;

          comb
            out = false;
          end comb
        end module Child

        module Parent
          param HEAD_DIM: const = 16;
          port out: out Bool;

          inst child: Child
            len <- HEAD_DIM;
            out -> out;
          end inst child
        end module Parent
    "#;

    let sim = compile_to_sim_h(source, false);
    assert!(
        !sim.contains("uint32_t HEAD_DIM;"),
        "param-valued scalar inst inputs must not be emitted as simulator fields:\n{sim}"
    );
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

    assert!(
        out.contains("int64_t accumulator"),
        "lowered thread submodule ports for SInt<40> regs should use int64_t; got:\n{out}"
    );
    assert!(out.contains("int64_t _accumulator"),
            "parent/native sim SInt<40> reg storage should use int64_t after thread lowering; got:\n{out}");
    assert!(
        out.contains("int64_t _n_accumulator"),
        "lowered thread/native sim _n_ temporaries should use int64_t for SInt<40>; got:\n{out}"
    );
    assert!(
        !out.contains("uint32_t _accumulator"),
        "lowered thread/native sim must not use uint32_t for SInt<40> accumulator; got:\n{out}"
    );
    assert!(!out.contains("uint64_t _accumulator"),
            "lowered thread/native sim must not use unsigned storage for SInt<40> accumulator; got:\n{out}");
    assert!(
        out.contains(
            "((int64_t)(((uint64_t)((_accumulator + inc_in)) & 0xFFFFFFFFFFULL) << 24) >> 24)"
        ) || out.contains(
            "((int64_t)(((uint64_t)((accumulator + inc_in)) & 0xFFFFFFFFFFULL) << 24) >> 24)"
        ),
        "lowered thread/native sim trunc should sign-extend SInt<40>; got:\n{out}"
    );
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
    //     S0 = wait_until (transition_cond = `req`), S1 = action (seq write,
    //     now folded into S0's cond-exit arm by issue #306), S2 = wait_cycles
    //     (wait 2 cycle), S3 = action (final seq write — not folded because
    //     the preceding state is wait_cycles, not wait_until).
    //     Localparams are still emitted for all states including folded ones.
    assert!(
        sv.contains("localparam [1:0] _t0_S0_wait_until = 0"),
        "expected S0 wait_until localparam:\n{sv}"
    );
    assert!(
        sv.contains("localparam [1:0] _t0_S1_action = 1"),
        "expected S1 action localparam (folded but still declared):\n{sv}"
    );
    assert!(
        sv.contains("localparam [1:0] _t0_S2_wait_cycles = 2"),
        "expected S2 wait_cycles localparam:\n{sv}"
    );
    assert!(
        sv.contains("localparam [1:0] _t0_S3_action = 3"),
        "expected S3 action localparam:\n{sv}"
    );

    // (2) Localparams declared in the merged threads module's parameter list,
    //     not inside the procedural block.
    assert!(
        sv.contains("module _M_threads #("),
        "merged threads module should have a parameter list:\n{sv}"
    );

    // (3) State comparisons use the name, not a bare literal.
    assert!(
        sv.contains("_t0_state == _t0_S0_wait_until"),
        "expected name-form state comparison for S0:\n{sv}"
    );
    assert!(
        sv.contains("_t0_state == _t0_S2_wait_cycles"),
        "expected name-form state comparison for S2:\n{sv}"
    );

    // (4) State-register assignments use the name, not a bare literal.
    //     Issue #306: S0's cond-exit arm now folds S1's `done <= true` and
    //     transitions directly to S2 (skipping S1).  So `_t0_S1_action` no
    //     longer appears as a transition target; `_t0_S2_wait_cycles` still
    //     does (from the folded S0 exit arm).
    assert!(
        !sv.contains("_t0_state <= _t0_S1_action"),
        "S1 was folded into S0's exit; S0 must jump directly to S2:\n{sv}"
    );
    assert!(
        sv.contains("_t0_state <= _t0_S2_wait_cycles"),
        "expected name-form state assignment to S2 (folded from S0 exit arm):\n{sv}"
    );
    // The folded seq assign must appear inside the if(req) block.
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        trimmed.contains("if (req) begin done <= 1'b1; _t0_state <= _t0_S2_wait_cycles;")
            || trimmed.contains("if (req) begin done <= 1'b1; _t0_state <= _t0_S2_wait_cycles"),
        "expected `done <= true` folded into S0's if(req) arm (issue #306):\n{sv}",
    );

    // (5) No bare `_t0_state == N` or `_t0_state <= N` numeric-literal forms
    //     should remain. The synchronous-reset path emits `_t0_state <= 0`
    //     as the reset value (not a state-transition); that one stays as 0.
    for n in 0..4 {
        let bad_cmp = format!("_t0_state == {}", n);
        let bad_assign = format!("_t0_state <= {};", n);
        // Reset assigns to literal 0 (acceptable). All other uses must be name-form.
        if n != 0 {
            assert!(
                !sv.contains(&bad_cmp),
                "state comparison should use name-form, found bare `{}`:\n{}",
                bad_cmp,
                sv
            );
            assert!(
                !sv.contains(&bad_assign),
                "state assignment should use name-form, found bare `{}`:\n{}",
                bad_assign,
                sv
            );
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
    assert!(
        sv.contains("_t0_S0_wait_until"),
        "expected wait_until role suffix on S0:\n{sv}"
    );
    assert!(
        sv.contains("_wait_cycles"),
        "expected wait_cycles role suffix on the wait-cycles state:\n{sv}"
    );
    // Sanity: the two roles are NOT collapsed to the same name.
    assert!(
        sv.matches("_t0_S0_wait_until").count() >= 1 && sv.matches("_wait_cycles =").count() >= 1,
        "wait_until and wait_cycles must produce distinct localparam decls:\n{sv}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #246: whole-design combinational feedback-loop detection (MVP).
// ─────────────────────────────────────────────────────────────────────────────

fn comb_loop_warnings(source: &str) -> Vec<String> {
    warnings_from(source)
        .into_iter()
        .filter(|m| m.contains("combinational feedback cycle") || m.starts_with("arch check:"))
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
    assert!(
        ws.iter()
            .any(|m| m.contains("combinational feedback cycle")),
        "expected a comb-loop warning, got: {:?}",
        ws
    );
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
    assert!(
        ws.iter()
            .any(|m| m.contains("combinational feedback cycle")),
        "expected a comb-loop warning, got: {:?}",
        ws
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "expected pragma to suppress cycle warning, got: {:?}",
        ws
    );
    // Sanity: the summary should report 1 SCC found / 1 suppressed.
    let summary: Vec<_> = ws.iter().filter(|m| m.starts_with("arch check:")).collect();
    assert!(
        summary
            .iter()
            .any(|m| m.contains("1 comb SCC(s) found") && m.contains("1 suppressed")),
        "expected suppression-summary line, got: {:?}",
        ws
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "register should break the cycle, but got: {:?}",
        ws
    );
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
    assert!(
        ws.iter()
            .any(|m| m.contains("combinational feedback cycle")),
        "expected opaque-interface module to participate in a detected cycle; warnings: {:?}",
        ws
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "pipe_reg / port reg outputs on an opaque stub must not close a comb cycle; got: {:?}",
        cycle_msgs
    );
}

// ── FIFO transparency regression (false-positive comb cycle) ──────────────────
//
// A `fifo` inst is a known first-class construct that is NOT a Module/Fsm decl,
// so its `child_is_interface` is `None`. The old code conflated that `None`
// with "unknown extern" and modeled the FIFO as combinationally transparent
// (every input → every output), manufacturing a spurious comb cycle when
// signals are routed input → fifo → output (e.g. an AXI4 CDC bridge). FIFO
// outputs (push_ready/pop_valid/pop_data) are pure functions of internal
// registered pointer/memory state — there is NO comb path across a FIFO. The
// fix uses the construct-aware `CombInfo` (empty for a fifo) instead of the
// opaque every-in→every-out fallback. These tests pin the corrected behavior
// AND prove the detector is not disabled (a real comb cycle still fires, and a
// latency-0 RAM in a real cycle is still flagged).

fn whole_design_from(source: &str) -> arch::comb_graph::WholeDesignAnalysis {
    let tokens = arch::lexer::tokenize(source).expect("lexer error");
    let mut parser = arch::parser::Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = arch::resolve::resolve(&ast).expect("resolve error");
    arch::comb_graph::analyze_whole_design(&ast, &symbols)
}

#[test]
fn test_fifo_inst_does_not_close_comb_cycle() {
    // Route signals input → async fifo → output, with NO pragma present.
    // Two differing Clock<...> domains make this a dual-clock async FIFO
    // (gray-code CDC). Every FIFO output is registered through the pointer
    // synchronisers — there is no combinational input→output path — so the
    // whole-design detector must report ZERO SCCs.
    let source = r#"
        domain MClkDom
          freq_mhz: 200
        end domain MClkDom

        domain SClkDom
          freq_mhz: 100
        end domain SClkDom

        fifo Cdc
          param DEPTH: const = 4;
          param T: type = UInt<8>;
          port wr_clk: in Clock<MClkDom>;
          port rd_clk: in Clock<SClkDom>;
          port rst: in Reset<Async, Low>;
          port push_valid: in Bool;
          port push_ready: out Bool;
          port push_data: in T;
          port pop_valid: out Bool;
          port pop_ready: in Bool;
          port pop_data: out T;
        end fifo Cdc

        module Top
          port wr_clk: in Clock<MClkDom>;
          port rd_clk: in Clock<SClkDom>;
          port rst: in Reset<Async, Low>;
          port in_valid: in Bool;
          port in_data: in UInt<8>;
          port in_ready: out Bool;
          port out_valid: out Bool;
          port out_data: out UInt<8>;
          port out_ready: in Bool;

          inst f: Cdc
            wr_clk     <- wr_clk;
            rd_clk     <- rd_clk;
            rst        <- rst;
            push_valid <- in_valid;
            push_data  <- in_data;
            push_ready -> in_ready;
            pop_valid  -> out_valid;
            pop_data   -> out_data;
            pop_ready  <- out_ready;
          end inst f
        end module Top
    "#;
    let wd = whole_design_from(source);
    assert_eq!(
        wd.total_sccs,
        0,
        "FIFO inst must not manufacture a comb cycle; SCCs found: {:?}",
        wd.sccs
            .iter()
            .map(|s| &s.owning_modules)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        wd.suppressed, 0,
        "no pragma present, nothing should be suppressed"
    );
}

#[test]
fn test_real_comb_cycle_still_detected_after_fifo_fix() {
    // Guard against over-correction: a genuine cross-instance comb loop
    // (A.out → B.in → B.out → A.in through two purely-combinational Cells)
    // must STILL be reported as exactly one SCC.
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
    let wd = whole_design_from(source);
    assert_eq!(
        wd.total_sccs, 1,
        "a real comb cycle through bodied modules must still be detected"
    );
}

#[test]
fn test_latency0_ram_in_real_cycle_still_flagged() {
    // No under-approximation: a latency-0 (async) RAM combinationally drives
    // its read data from addr/data inputs (comb_info_for_ram reports it as
    // comb-coupled). Wiring that read data back into the RAM's write data
    // through a parent comb wire forms a REAL comb cycle that must STILL be
    // detected after the FIFO fix — the fix only suppresses constructs whose
    // construct-aware CombInfo is empty (registered), and latency-0 RAM's is
    // not empty.
    let source = r#"
        domain SysDomain
          freq_mhz: 100
        end domain SysDomain

        ram AsyncMem
          kind single;
          latency 0;
          param DEPTH: const = 16;
          param T: type = UInt<8>;
          port clk: in Clock<SysDomain>;
          store
            data: Vec<T, DEPTH>;
          end store
          port en: in Bool;
          port wen: in Bool;
          port addr: in UInt<4>;
          port wdata: in T;
          port rdata: out T;
        end ram AsyncMem

        module Top
          port clk: in Clock<SysDomain>;
          port en: in Bool;
          port wen: in Bool;
          port addr: in UInt<4>;
          port q: out UInt<8>;
          wire loop_data: UInt<8>;

          inst m: AsyncMem
            clk   <- clk;
            en    <- en;
            wen   <- wen;
            addr  <- addr;
            wdata <- loop_data;
            rdata -> loop_data;
          end inst m

          comb
            q = loop_data;
          end comb
        end module Top
    "#;
    let wd = whole_design_from(source);
    assert_eq!(
        wd.total_sccs, 1,
        "a latency-0 RAM in a real comb cycle must still be flagged (no under-approximation)"
    );
}

#[test]
fn test_arbiter_inst_comb_grant_loop_still_detected() {
    // Soundness guard for the comb-loop detector (regression for the over-
    // correction in #545). An `arbiter`'s grant outputs — `grant_valid`,
    // `grant_requester`, and the per-requester `ready` — are driven in
    // `always_comb` from the request `valid` inputs (priority + round-robin
    // policies). So there IS a real combinational path request → grant.
    //
    // #545 broadened "model fifo/ram as non-opaque" to ALL non-module/non-fsm
    // constructs, which made `arbiter` use its empty (PURE) `CombInfo` and
    // silently DROPPED real comb loops routed through an arbiter's grant — a
    // false negative (Verilator flags the same design `UNOPTFLAT: Circular
    // combinational logic`). This wires the priority arbiter's `ready` (grant)
    // straight back into its `valid` request, forming a genuine 1-cycle comb
    // loop that the whole-design detector must report as exactly one SCC.
    let source = r#"
        domain D
          freq_mhz: 100
        end domain D

        arbiter PrioArb
          policy priority;
          param NUM_REQ: const = 2;
          port clk: in Clock<D>;
          port rst: in Reset<Sync>;
          ports[NUM_REQ] request
            valid: in Bool;
            ready: out Bool;
          end ports request
          port grant_valid:     out Bool;
          port grant_requester: out UInt<1>;
        end arbiter PrioArb

        module Top
          port clk: in Clock<D>;
          port rst: in Reset<Sync>;
          port seed: in UInt<2>;
          port gv: out Bool;
          port gr: out UInt<1>;

          wire req: UInt<2>;
          wire rdy: UInt<2>;

          inst a: PrioArb
            clk             <- clk;
            rst             <- rst;
            request.valid   <- req;
            request.ready   -> rdy;
            grant_valid     -> gv;
            grant_requester -> gr;
          end inst a

          comb
            // req -> arb.valid -> arb.ready (comb grant) -> rdy -> req
            req = rdy | seed;
          end comb
        end module Top
    "#;
    let wd = whole_design_from(source);
    assert_eq!(
        wd.total_sccs,
        1,
        "a real comb loop through an arbiter's combinational grant must be \
         detected (no under-approximation); SCCs found: {:?}",
        wd.sccs
            .iter()
            .map(|s| &s.owning_modules)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_bus_ready_eq_valid_is_not_a_false_comb_cycle() {
    // The universal AXI-style handshake: a target drives `ready` from `valid`
    // (an input). `p.ready = f(p.valid)` is acyclic (valid is a primary input).
    // The whole-design detector must NOT conflate the bus port `p` into one
    // node — reading `p.valid` and driving `p.ready` are DISTINCT signals.
    // Pre-fix this fabricated a self-cycle `fire -> p -> fire`; Verilator
    // always reported the design loop-free.
    let source = r#"
        bus Hsk
          valid: out Bool;
          ready: in  Bool;
          data:  out UInt<8>;
        end bus Hsk

        module HskTarget
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port p: target Hsk;
          reg busy: Bool reset rst => false;
          let fire: Bool = p.valid and (not busy);
          comb
            p.ready = fire;
          end comb
          seq on clk rising
            if fire
              busy <= true;
            end if
          end seq
        end module HskTarget
    "#;
    let wd = whole_design_from(source);
    assert_eq!(
        wd.total_sccs,
        0,
        "a target's `ready = f(valid)` handshake must not be a comb cycle \
         (bus members are distinct nodes); SCCs found: {:?}",
        wd.sccs
            .iter()
            .map(|s| &s.owning_modules)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_real_comb_cycle_through_bus_member_still_detected() {
    // Soundness guard for the per-member bus-node fix: a genuine combinational
    // loop routed through ONE bus member must still be caught. Here output
    // member `p.ready` feeds `x` which drives `p.ready` — a real 1-cycle loop
    // (`p.ready -> x -> p.ready`). Per-member granularity keeps it as a single
    // node, so the SCC is still found (no under-approximation).
    let source = r#"
        bus Hsk
          valid: out Bool;
          ready: in  Bool;
          data:  out UInt<8>;
        end bus Hsk

        module HskLoop
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync>;
          port en: in Bool;
          port p: target Hsk;
          let x: Bool = p.ready and en;
          comb
            p.ready = x;
          end comb
        end module HskLoop
    "#;
    let wd = whole_design_from(source);
    assert_eq!(
        wd.total_sccs,
        1,
        "a real comb loop through a single bus member must still be detected; \
         SCCs found: {:?}",
        wd.sccs
            .iter()
            .map(|s| &s.owning_modules)
            .collect::<Vec<_>>()
    );
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
    let m = parsed
        .items
        .iter()
        .find_map(|i| match i {
            arch::ast::Item::Module(m) => Some(m),
            _ => None,
        })
        .expect("module");

    let by_name: std::collections::HashMap<&str, &arch::ast::PortDecl> =
        m.ports.iter().map(|p| (p.name.name.as_str(), p)).collect();

    let x_deps: Vec<&str> = by_name["x"]
        .comb_deps
        .as_ref()
        .expect("x must carry comb_deps")
        .iter()
        .map(|i| i.name.as_str())
        .collect();
    assert_eq!(x_deps, vec!["a"], "x deps");

    let y_deps: Vec<&str> = by_name["y"]
        .comb_deps
        .as_ref()
        .expect("y must carry comb_deps")
        .iter()
        .map(|i| i.name.as_str())
        .collect();
    assert_eq!(y_deps, vec!["a", "b"], "y deps");

    let z_deps: &Vec<arch::ast::Ident> = by_name["z"]
        .comb_deps
        .as_ref()
        .expect("z must carry comb_deps (empty list = pure)");
    assert!(z_deps.is_empty(), "z must be pure (empty deps)");

    assert!(
        by_name["w"].comb_deps.is_none(),
        "w must carry no annotation (opaque fallback)"
    );
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
    let item = parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Module(_)))
        .expect("module");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(
        body.contains("port x: out UInt<8> comb_dep_on(a);"),
        "x must depend only on a: {body}"
    );
    assert!(
        body.contains("port y: out UInt<8> comb_dep_on(a, b);"),
        "y must depend on a and b: {body}"
    );
    assert!(
        body.contains("port z: out UInt<8> comb_dep_on();"),
        "z is pure (constant): {body}"
    );
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
            if m.name.name == "Stub" {
                m.is_interface = true;
            }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") {
                m.is_interface = true;
            }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "comb_dep_on(in_a) should restrict edges so no cycle fires; got: {:?}",
        cycle_msgs
    );
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
            if m.name.name == "Stub" {
                m.is_interface = true;
            }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") {
                m.is_interface = true;
            }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "comb_dep_on() (pure) must produce no incoming comb edges; got: {:?}",
        cycle_msgs
    );
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
            if m.name.name == "Stub" {
                m.is_interface = true;
            }
        }
    }
    let ast = arch::elaborate::elaborate(parsed_ast).expect("elaborate");
    let mut ast = ast;
    for item in ast.items.iter_mut() {
        if let arch::ast::Item::Module(m) = item {
            if m.name.name.starts_with("Stub") {
                m.is_interface = true;
            }
        }
    }
    let symbols = arch::resolve::resolve(&ast).expect("resolve");
    let checker = arch::typecheck::TypeChecker::new(&symbols, &ast);
    let (warnings, _) = checker.check().expect("type check");
    let ws: Vec<String> = warnings.into_iter().map(|w| w.message).collect();
    assert!(
        ws.iter()
            .any(|m| m.contains("combinational feedback cycle")),
        "absent annotation must keep opaque fallback that fires cycle: {:?}",
        ws
    );
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
    let err = parser
        .parse_source_file()
        .expect_err("must reject comb_dep_on on registered output");
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("comb_dep_on") && msg.contains("registered"),
        "error should mention comb_dep_on + registered; got: {}",
        msg
    );
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
    let err = parser
        .parse_source_file()
        .expect_err("must reject comb_dep_on on input port");
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("comb_dep_on"),
        "error should mention comb_dep_on; got: {}",
        msg
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "per-output precision must eliminate the aggregate-only phantom \
         cycle; got: {:?}",
        cycle_msgs
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        !cycle_msgs.is_empty(),
        "real cycle (u1.out_a ← w2; u2.out_b ← w1) must still fire; \
         warnings: {:?}",
        ws
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "pure output (per-output map empty) must not close a comb cycle; \
         got: {:?}",
        cycle_msgs
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "per-output precision must hold across 3 cross-wired insts; got: {:?}",
        cycle_msgs
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs.is_empty(),
        "fsm per-output precision must eliminate the aggregate-only \
         phantom cycle; got: {:?}",
        cycle_msgs
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        !cycle_msgs.is_empty(),
        "real cycle (u1.out_a ← w2; u2.out_b ← w1) through fsm \
         must still fire; warnings: {:?}",
        ws
    );
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
    let item = parsed
        .items
        .iter()
        .find(|i| matches!(i, arch::ast::Item::Fsm(_)))
        .expect("fsm");
    let body = arch::interface::emit_interface(item).expect("emit_interface");
    assert!(
        body.contains("port x: out UInt<8> comb_dep_on(a);"),
        "x must depend only on a: {body}"
    );
    assert!(
        body.contains("port y: out UInt<8> comb_dep_on(a, b);"),
        "y must depend on a and b: {body}"
    );
    assert!(
        body.contains("port z: out UInt<8> comb_dep_on();"),
        "z is pure (constant): {body}"
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        !cycle_msgs.is_empty(),
        "default_comb reads in_z driving out_a; w1 → in_z → out_a → w1 \
         must fire as a comb cycle; warnings: {:?}",
        ws
    );
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
    let cycle_msgs: Vec<_> = ws
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        !cycle_msgs.is_empty(),
        "out_a's default expression reads in_y; w1 → in_y → out_a → w1 \
         must fire as a comb cycle; warnings: {:?}",
        ws
    );

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
    let cycle_msgs2: Vec<_> = ws2
        .iter()
        .filter(|m| m.contains("combinational feedback cycle ("))
        .collect();
    assert!(
        cycle_msgs2.is_empty(),
        "out_a's default reads only in_x; w1 → in_y is NOT a dep, \
         so no cycle should fire; got: {:?}",
        cycle_msgs2
    );
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
    assert_eq!(
        sv_ctrl, sv_mc,
        "multicycle annotation must not alter SV emission"
    );
    assert!(sdc_ctrl.is_none(), "control case: no .sdc expected");
    assert!(sdc_mc.is_some(), "multicycle case: .sdc expected");
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
    assert!(
        sdc.contains("set_multicycle_path 3 -setup -to [get_cells -hierarchical {*result_reg*}]"),
        "expected setup constraint with N=3; got:\n{}",
        sdc
    );
    assert!(
        sdc.contains("set_multicycle_path 2 -hold -to [get_cells -hierarchical {*result_reg*}]"),
        "expected hold constraint with N-1=2; got:\n{}",
        sdc
    );
    assert!(
        sdc.contains("Module M: multicycle reg result"),
        "expected per-reg header comment; got:\n{}",
        sdc
    );
    // The leading `*` in the glob is load-bearing: it lets the constraint
    // attach under both flat synth (no instance prefix) and hierarchical
    // synth (any number of `top/.../<Module>/` levels). A regression that
    // re-introduces the `<Module>/` prefix would silently fail to attach
    // under flat / standalone synth (OpenSTA warns `instance not found`).
    assert!(
        sdc.contains("[get_cells -hierarchical {*result_reg*}]"),
        "expected wildcard-prefix glob `*result_reg*` with -hierarchical; got:\n{}",
        sdc
    );
    assert!(
        !sdc.contains("{M/result_reg"),
        "expected NO hierarchical `M/result_reg` prefix in glob; got:\n{}",
        sdc
    );
    // `-hierarchical` is mandatory: under hierarchical synth (parent +
    // child module), OpenSTA's `get_cells` is non-recursive by default, so
    // the `*` glob does not descend into instance subhierarchies. Without
    // the flag the multicycle constraint silently attaches to zero cells
    // and the path is treated as single-cycle (verified with the
    // MultdivMulticycleHier two-pass example).
    assert!(
        sdc.contains("get_cells -hierarchical"),
        "expected `-hierarchical` flag on get_cells; got:\n{}",
        sdc
    );
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
        msg.contains("multicycle")
            && (msg.contains(">= 1") || msg.contains("N=0") || msg.contains("meaningless")),
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
    assert!(
        sdc.contains("set_multicycle_path 4 -setup -to [get_cells -hierarchical {*_unused_reg*}]"),
        "got:\n{}",
        sdc
    );
    assert!(
        sdc.contains("set_multicycle_path 3 -hold -to [get_cells -hierarchical {*_unused_reg*}]"),
        "got:\n{}",
        sdc
    );
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
    assert!(
        sdc.is_none(),
        "no multicycle annotation → `emit_sdc` must return None so the driver \
         skips writing a `.sdc` companion file"
    );
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
    assert!(
        sdc.contains("set_multicycle_path 5 -setup -to [get_cells -hierarchical {*slow_r_reg*}]"),
        "got:\n{}",
        sdc
    );
    assert!(
        sdc.contains("set_multicycle_path 4 -hold -to [get_cells -hierarchical {*slow_r_reg*}]"),
        "got:\n{}",
        sdc
    );
}

/// Regression: a module-internal `function` whose body references a
/// `package` param (e.g. `x >> REGION_BITS`) used to fail C++ compile in
/// arch_sim_build/VFunctions.h with `use of undeclared identifier
/// 'REGION_BITS'` — module-internal functions are hoisted to free
/// functions in VFunctions.h, which is included from V{Module}.h
/// *before* the per-module `#define`s. The fix hoists package- and
/// module-level const params as `#define`s at the top of VFunctions.h.
#[test]
fn test_sim_function_uses_package_param() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/pkg_param_in_function/PkgFoo.arch")
        .arg("tests/pkg_param_in_function/Probe.arch")
        .arg("--tb")
        .arg("tests/pkg_param_in_function/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for pkg_param_in_function repro");
    assert!(
        out.status.success(),
        "pkg_param_in_function sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS pkg_param_in_function"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_function_value_list_for_unroll_substitutes_loop_var() {
    let source = r#"
module FunctionValueListProbe
  port a: in UInt<8>;
  port y: out UInt<8>;

  function last_value(seed: UInt<8>) -> UInt<8>
    let acc: UInt<8> = seed;
    for i in {1, 2, 3}
      acc = i.zext<8>();
    end for
    return acc;
  end function last_value

  comb
    y = last_value(a);
  end comb
end module FunctionValueListProbe
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("acc = 8'($unsigned(1));")
            && sv.contains("acc = 8'($unsigned(2));")
            && sv.contains("acc = 8'($unsigned(3));"),
        "value-list function for-loop should emit each substituted body:\n{sv}"
    );
    assert!(
        !sv.contains("// i ="),
        "function value-list loop must not emit placeholder-only comments:\n{sv}"
    );
}

#[test]
fn test_sim_function_emits_for_and_assign_body_items() {
    let source = r#"
module FunctionSimForAssignProbe
  port a: in UInt<8>;
  port y: out UInt<8>;

  function last_range(seed: UInt<8>) -> UInt<8>
    let acc: UInt<8> = seed;
    for i in 0..2
      acc = i.zext<8>();
    end for
    return acc;
  end function last_range

  comb
    y = last_range(a);
  end comb
end module FunctionSimForAssignProbe
"#;
    let cpp = compile_to_sim_h(source, false);
    assert!(
        cpp.contains("uint8_t acc = seed;"),
        "function local let should be emitted as a mutable C++ local:\n{cpp}"
    );
    assert!(
        cpp.contains("for (int i = 0; i <= 2; i++)"),
        "function for-loop body should be emitted in sim C++:\n{cpp}"
    );
    assert!(
        cpp.contains("acc = (uint8_t)(i);"),
        "function assignment body should be emitted in sim C++:\n{cpp}"
    );
}

#[test]
fn test_inst_for_loop_unrolls_connections() {
    // A `for k in 0..N-1 ... end for` block inside an inst body unrolls at
    // elaboration into N flat `Connection`s — AST-identical to the hand-
    // enumerated form, so the generated SV must contain the per-index
    // named-port connections produced by the loop body. This is the core
    // NIC-400 Fabric use case: scaling a master/slave dimension without
    // hand-enumerating connections.
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
          port clk: in Clock<SysDomain>;
          port o0: out Bool;
          port o1: out Bool;
          wire w_0: B;
          wire w_1: B;
          inst p: Producer
            clk <- clk;
            for k in 0..1
              chans[k] -> w_k;
            end for
          end inst p
          comb
            o0 = w_0.v;
            o1 = w_1.v;
          end comb
        end module Parent
    ";
    let sv = compile_to_sv(source);
    // The for-loop body `chans[k] -> w_k;` unrolls to:
    //   chans[0] -> w_0;   chans[1] -> w_1;
    // which gathers into the packed-concat form at the inst boundary
    // (big-endian, so chans[1] is the MSB).
    assert!(sv.contains(".chans_v({w_1_v, w_0_v})") || sv.contains(".chans_v ({w_1_v, w_0_v})"),
            "expected `.chans_v({{w_1_v, w_0_v}})` packed concat from unrolled inst-for-loop in SV:\n{sv}");
    assert!(
        sv.contains(".chans_d({w_1_d, w_0_d})") || sv.contains(".chans_d ({w_1_d, w_0_d})"),
        "expected `.chans_d({{w_1_d, w_0_d}})` packed concat in SV:\n{sv}"
    );
}

#[test]
fn test_inst_for_loop_matches_hand_enumerated_form() {
    // The contract of inst-body `for k in 0..N-1` unroll is: the resulting
    // AST must be byte-identical to the hand-enumerated form. We compile
    // both shapes and compare the emitted SV directly. Exercises:
    //   - 2-D bus wire (`Vec<Vec<B, NS>, NM>`) — the NIC-400 Fabric shape.
    //   - Outer `generate_for j` substitution flowing into the inst-body
    //     for-loop's body expressions (`edges[k][j]`).
    //   - Range bound `0..NM-1` that references an enclosing param.
    let bus_and_slave = "
        bus B
          v: out Bool;
          d: out UInt<8>;
        end bus B

        module Slave
          param J: const = 0;
          param NMC: const = 2;
          port clk: in Clock<SysDomain>;
          port ins: target Vec<B, NMC>;
          port o_v: out Bool;
          comb
            o_v = ins[0].v;
          end comb
        end module Slave
    ";

    let fabric_handw = format!(
        "{bus_and_slave}
        module Fabric
          param NM: const = 2;
          param NS: const = 2;
          port clk: in Clock<SysDomain>;
          port o_0: out Bool;
          port o_1: out Bool;
          wire edges: Vec<Vec<B, NS>, NM>;
          comb
            edges[0][0].v = true;  edges[0][0].d = 8'h22;
            edges[0][1].v = true;  edges[0][1].d = 8'h33;
            edges[1][0].v = true;  edges[1][0].d = 8'h44;
            edges[1][1].v = true;  edges[1][1].d = 8'h55;
          end comb
          generate_for j in 0..NS-1
            inst sp_j: Slave
              param J = j;
              param NMC = NM;
              clk <- clk;
              o_v -> o_j;
              ins[0] <- edges[0][j];
              ins[1] <- edges[1][j];
            end inst sp_j
          end generate_for
        end module Fabric
    "
    );

    let fabric_loop = format!(
        "{bus_and_slave}
        module Fabric
          param NM: const = 2;
          param NS: const = 2;
          port clk: in Clock<SysDomain>;
          port o_0: out Bool;
          port o_1: out Bool;
          wire edges: Vec<Vec<B, NS>, NM>;
          comb
            edges[0][0].v = true;  edges[0][0].d = 8'h22;
            edges[0][1].v = true;  edges[0][1].d = 8'h33;
            edges[1][0].v = true;  edges[1][0].d = 8'h44;
            edges[1][1].v = true;  edges[1][1].d = 8'h55;
          end comb
          generate_for j in 0..NS-1
            inst sp_j: Slave
              param J = j;
              param NMC = NM;
              clk <- clk;
              o_v -> o_j;
              for k in 0..NM-1
                ins[k] <- edges[k][j];
              end for
            end inst sp_j
          end generate_for
        end module Fabric
    "
    );

    let sv_handw = compile_to_sv(&fabric_handw);
    let sv_loop = compile_to_sv(&fabric_loop);
    assert_eq!(
        sv_handw, sv_loop,
        "inst-body for-loop must produce byte-identical SV to the \
         hand-enumerated form. Diff:\n\
         === hand-enumerated ===\n{sv_handw}\n\
         === loop-unrolled ===\n{sv_loop}"
    );

    // Sanity: the SV actually contains the expected per-index connections
    // (so this isn't just `equal-but-empty`).
    assert!(
        sv_loop.contains("sp_0") && sv_loop.contains("sp_1"),
        "expected sp_0 and sp_1 instances in SV:\n{sv_loop}"
    );
    // The `param NMC = NM` forwarding makes the child's `ins: Vec<B, NMC>`
    // count resolve to 2 in the parent scope, so each `ins[k] <- edges[k][j]`
    // packs into the per-bus-signal concat form `.ins_v({edges_v[1][j],
    // edges_v[0][j]})` / `.ins_d(...)`. (Before the inst param-forwarding fix
    // the count was unresolved and codegen fell back to the invalid pins
    // `.ins_0(edges[0][0])` — non-existent on the child and referencing the
    // bus-typed name `edges` that SV had already split into `edges_v`/`edges_d`;
    // Verilator rejected it with "Pin not found: 'ins_1'".)
    assert!(
        sv_loop.contains(".ins_v({edges_v[1][0], edges_v[0][0]})")
            && sv_loop.contains(".ins_d({edges_d[1][1], edges_d[0][1]})"),
        "expected packed per-bus-signal Vec-of-bus forwarding refs in SV:\n{sv_loop}"
    );
}

#[test]
fn test_type_alias_uint_scalar() {
    // Simplest case: alias for a UInt<W> primitive, used as a port type.
    let source = r#"
        module M
          type Word = UInt<32>;
          port a:   in  Word;
          port out: out Word;
          comb
            out = a +% 32'd1;
          end comb
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("input logic [31:0] a"),
        "alias should expand to UInt<32> at the port: {sv}"
    );
    assert!(
        sv.contains("output logic [31:0] out"),
        "alias should expand to UInt<32> at the output port: {sv}"
    );
}

#[test]
fn test_type_alias_struct() {
    // Alias for a struct type. Use site must produce the same SV as the
    // inline struct reference.
    let source = r#"
        struct Pair
          lo: UInt<8>;
          hi: UInt<8>;
        end struct Pair
        module M
          type P = Pair;
          port in_p:  in  P;
          port out_lo: out UInt<8>;
          comb
            out_lo = in_p.lo;
          end comb
        end module M
    "#;
    let sv = compile_to_sv(source);
    // Aliased struct port should look identical to an inline Pair port.
    assert!(
        sv.contains("in_p") && sv.contains("struct"),
        "struct alias should expand at port site: {sv}"
    );
}

#[test]
fn test_type_alias_bus_parameterized_port() {
    // Original motivation: alias a parameterized bus, use it on a port —
    // SV emission should match the inline-parameterized form.
    let source = r#"
        bus Axi
          param ADDR_W: const = 32;
          v: out Bool;
          addr: out UInt<ADDR_W>;
        end bus Axi
        module M
          type Edge = Axi<ADDR_W=16>;
          port m: initiator Edge;
          comb
            m.v = true;
            m.addr = 16'd0;
          end comb
        end module M
    "#;
    let sv = compile_to_sv(source);
    // ADDR_W should expand to 16 via the alias, so the port carries
    // [15:0] not [31:0].
    assert!(
        sv.contains("output logic [15:0] m_addr")
            || sv.contains("output logic [ADDR_W-1:0] m_addr"),
        "bus alias with ADDR_W=16 override should produce 16-bit addr port: {sv}"
    );
}

#[test]
fn test_type_alias_chain() {
    // Alias referencing an earlier alias.
    let source = r#"
        module M
          type Byte = UInt<8>;
          type Word = Byte;
          port a:   in  Word;
          port out: out Byte;
          comb
            out = a;
          end comb
        end module M
    "#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("input logic [7:0] a"),
        "chained alias should resolve through to UInt<8>: {sv}"
    );
    assert!(
        sv.contains("output logic [7:0] out"),
        "chained alias should resolve through to UInt<8>: {sv}"
    );
}

#[test]
fn test_type_alias_undeclared_errors() {
    // Reference to a type that was declared neither as an alias nor as a
    // struct/enum/bus. Should error somewhere in the pipeline — at the
    // alias resolver, in resolve, or in typecheck. (The exact stage isn't
    // load-bearing; the contract is "compile fails with a clear msg".)
    let source = r#"
        module M
          type Alias = Frobnitz;
          port x: in Alias;
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    // Try the full pipeline; any stage erroring out is acceptable.
    let pipeline_errors_out = match arch::type_alias::resolve_type_aliases(ast) {
        Err(_) => true,
        Ok(ast2) => match elaborate::elaborate(ast2) {
            Err(_) => true,
            Ok(ast3) => match resolve::resolve(&ast3) {
                Err(_) => true,
                Ok(symbols) => {
                    let checker = TypeChecker::new(&symbols, &ast3);
                    checker.check().is_err()
                }
            },
        },
    };
    assert!(
        pipeline_errors_out,
        "unknown type name 'Frobnitz' should error somewhere in the compile pipeline"
    );
}

#[test]
fn test_type_alias_circular_errors() {
    // type A = B; type B = A; — must be detected as a cycle.
    let source = r#"
        module M
          type A = B;
          type B = A;
          port x: in A;
        end module M
    "#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let r = arch::type_alias::resolve_type_aliases(ast);
    assert!(r.is_err(), "circular alias should error: {r:?}");
    let errs = r.unwrap_err();
    let msg: String = errs.iter().map(|e| format!("{e:?}")).collect();
    assert!(
        msg.to_lowercase().contains("circular")
            || msg.to_lowercase().contains("cycle")
            || msg.to_lowercase().contains("recursive"),
        "expected circular/cycle/recursive in diagnostic, got: {msg}"
    );
}

#[test]
fn test_generate_for_wire_decls() {
    // `wire w_i: T;` inside `generate_for i in 0..N-1` unrolls at
    // elaboration: each iteration substitutes the loop var into the
    // wire name (`w_i` → `w_0`, `w_1`, ..., `w_{N-1}`) and emits a
    // distinct wire per iteration.
    let source = "
        module M
          param N: const = 3;
          port out: out UInt<8>;
          generate_for i in 0..N-1
            wire w_i: UInt<8>;
          end generate_for
          comb
            out = w_0 +% w_1 +% w_2;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // Three flat wires named w_0, w_1, w_2 — one per iteration value.
    for i in 0..3 {
        assert!(
            sv.contains(&format!("logic [7:0] w_{i};")),
            "missing `logic [7:0] w_{i};` in SV:\n{sv}"
        );
    }
}

#[test]
fn test_generate_for_wire_inst_loopback() {
    // Combined wire + inst inside the same generate_for. The inst
    // drives the wire from this iteration; downstream module-scope
    // code reads the per-iteration wire by its substituted flat name.
    let source = "
        module Doubler
          port a: in  UInt<8>;
          port b: out UInt<8>;
          comb
            b = a +% a;
          end comb
        end module Doubler

        module Top
          param N: const = 2;
          port in0:  in  UInt<8>;
          port in1:  in  UInt<8>;
          port out0: out UInt<8>;
          port out1: out UInt<8>;
          generate_for i in 0..N-1
            wire stage_i: UInt<8>;
          end generate_for
          inst d0: Doubler a <- in0; b -> stage_0; end inst d0
          inst d1: Doubler a <- in1; b -> stage_1; end inst d1
          comb
            out0 = stage_0;
            out1 = stage_1;
          end comb
        end module Top
    ";
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("logic [7:0] stage_0;"),
        "missing `logic [7:0] stage_0;` (wire from generate_for iter 0):\n{sv}"
    );
    assert!(
        sv.contains("logic [7:0] stage_1;"),
        "missing `logic [7:0] stage_1;` (wire from generate_for iter 1):\n{sv}"
    );
    assert!(
        sv.contains("assign out0 = stage_0;"),
        "expected `out0 = stage_0` read of per-iter wire:\n{sv}"
    );
    assert!(
        sv.contains("assign out1 = stage_1;"),
        "expected `out1 = stage_1` read of per-iter wire:\n{sv}"
    );
}

#[test]
fn test_generate_for_wire_param_size() {
    // The wire's type may reference a module param (independent of the
    // loop variable). Substitution should leave that param ident
    // intact — only the loop var gets rewritten.
    let source = "
        module M
          param N: const = 2;
          param W: const = 16;
          port out_lo: out UInt<W>;
          port out_hi: out UInt<W>;
          generate_for i in 0..N-1
            wire bus_i: UInt<W>;
          end generate_for
          comb
            bus_0 = 16'd0;
            bus_1 = 16'd0;
            out_lo = bus_0;
            out_hi = bus_1;
          end comb
        end module M
    ";
    let sv = compile_to_sv(source);
    // Wire width should fold to 16 bits via the W param's default.
    assert!(
        sv.contains("logic [W-1:0] bus_0;") || sv.contains("logic [15:0] bus_0;"),
        "missing `logic [W-1:0] bus_0;` (param-driven width wire):\n{sv}"
    );
    assert!(
        sv.contains("logic [W-1:0] bus_1;") || sv.contains("logic [15:0] bus_1;"),
        "missing `logic [W-1:0] bus_1;` (param-driven width wire):\n{sv}"
    );
}

#[test]
fn test_generate_if_variant_discovery_with_param_expr() {
    // Regression: when `generate_if` contains an inst whose param value
    // references the enclosing module's params (rather than a literal),
    // variant discovery must evaluate the value against those params.
    // Before the fix, every inst in a `generate_if` silently landed on
    // the default-param variant — a silent miscompile.
    //
    // This source sets `param I = TEN` and `param I = TWENTY` on two
    // Inner insts via different `generate_if 1` blocks. Each should
    // produce its own specialized VInner__I_10 / VInner__I_20 in the
    // native sim. Behavior check: out0 = idx + 10, out1 = idx + 20.
    let source = "
        module Inner
          param I: const = 8;
          port idx: in  UInt<8>;
          port out: out UInt<8>;
          let pad: UInt<8> = I.zext<8>();
          comb
            out = idx +% pad;
          end comb
        end module Inner

        module Top
          param TEN: const = 10;
          param TWENTY: const = 20;
          port idx: in UInt<8>;
          port out0: out UInt<8>;
          port out1: out UInt<8>;
          generate_if 1
            inst inner_a: Inner
              param I = TEN;
              idx <- idx;
              out -> out0;
            end inst inner_a
          end generate_if
          generate_if 1
            inst inner_b: Inner
              param I = TWENTY;
              idx <- idx;
              out -> out1;
            end inst inner_b
          end generate_if
        end module Top
    ";
    let sv = compile_to_sv(source);
    // Two specialized variants must be emitted, one per distinct
    // `param I = <expr>` value resolved against enclosing module params.
    assert!(
        sv.contains("module Inner__I_10"),
        "missing `Inner__I_10` specialized variant — variant discovery \
             didn't resolve `param I = TEN` against enclosing module's params:\n{sv}"
    );
    assert!(
        sv.contains("module Inner__I_20"),
        "missing `Inner__I_20` specialized variant — variant discovery \
             didn't resolve `param I = TWENTY` against enclosing module's params:\n{sv}"
    );
    // Inst sites should reference the specialized variant by name.
    assert!(
        sv.contains("Inner__I_10 inner_a")
            || sv.contains("Inner__I_10 #") && sv.contains("inner_a"),
        "expected sp_0 to reference Inner__I_10 variant:\n{sv}"
    );
    assert!(
        sv.contains("Inner__I_20 inner_b")
            || sv.contains("Inner__I_20 #") && sv.contains("inner_b"),
        "expected sp_1 to reference Inner__I_20 variant:\n{sv}"
    );
}

#[test]
fn test_generate_if_variant_discovery_with_default_branch_values() {
    // Even when both then- and else-branches of `generate_if` contain
    // insts with module-param-referencing values, variant discovery
    // walks both conservatively and records each. Over-recording is
    // benign — extra unused variants are deduped at codegen time.
    let source = "
        module Inner
          param I: const = 0;
          port idx: in  UInt<8>;
          port out: out UInt<8>;
          let pad: UInt<8> = I.zext<8>();
          comb
            out = idx +% pad;
          end comb
        end module Inner

        module Top
          param SEVEN: const = 7;
          param NINE:  const = 9;
          port idx: in UInt<8>;
          port out: out UInt<8>;
          generate_if 1
            inst chosen: Inner
              param I = SEVEN;
              idx <- idx;
              out -> out;
            end inst chosen
          end generate_if
          generate_if 0
            inst unused: Inner
              param I = NINE;
              idx <- idx;
              out -> out;
            end inst unused
          end generate_if
        end module Top
    ";
    let sv = compile_to_sv(source);
    // Both branches' values get discovered. The cond=1 branch's inst
    // actually survives; the cond=0 branch's variant is emitted but
    // never instantiated — synthesis tools dead-code it.
    assert!(
        sv.contains("module Inner__I_7"),
        "missing `Inner__I_7` variant (active branch's param):\n{sv}"
    );
}

#[test]
fn test_sim_nested_vec_reg_round_trips_all_cells() {
    // Regression: `reg rf: Vec<Vec<UInt<W>, N>, M>;` used to emit
    // `uint32_t _rf[M]` — silently truncating the inner dim to a
    // 32-bit scalar. Reads/writes via `rf[i][j]` then aliased into
    // bit-positions of the outer element. The fix recursively
    // descends vec_array_info to emit the proper multi-dim C array
    // (`uint32_t _rf[M][N]`) and updates the expr emitter to chain
    // C subscripts for the inner index.
    //
    // This test writes 32 distinct values across an 8×4 Vec-of-Vec
    // reg, then reads every cell back. Any silent aliasing would
    // scramble values — a pre-fix run would fail the readback.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/nested_vec_sim/Probe.arch")
        .arg("--tb")
        .arg("tests/nested_vec_sim/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for nested_vec_sim probe");
    assert!(
        out.status.success(),
        "nested-Vec sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS nested-Vec storage"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_sim_vec_reg_forwarded_to_thread_inst_uses_reg_storage() {
    // Regression: a module that contains BOTH threads AND a module-scope
    // `Vec<T, N>` reg lowers the threads into an `_inst__threads`
    // sub-instance; the parent forwards each Vec-reg element into the
    // thread's flattened scalar inputs. The forwarding path used to
    // hardcode the `_let_` (wire) prefix — emitting `_inst__threads.foo_0
    // = _let_foo[0];` — but a Vec *reg* stores under `_foo[0]`, so the
    // generated C++ referenced an undeclared `_let_foo` and failed to
    // compile. Fix: resolve the prefix via `vec_storage_prefix` (reg → `_`,
    // wire/let → `_let_`, inst-output → ``).
    let src = r#"
module VecRegThread
  param N: const = 2;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port go:  in Bool;
  port done: out Bool;

  reg tag: Vec<UInt<8>, N> reset rst => 0;

  seq on clk rising
    for i in 0..N-1
      if go
        tag[i] <= 7;
      end if
    end for
  end seq

  thread Worker on clk rising, rst low
    default comb
      done = false;
    end default
    if not go
      wait until go;
    end if
    do
      done = (tag[0] == 7);
    until tag[0] == 7;
  end thread Worker
end module VecRegThread
"#;
    let h = compile_to_sim_h(src, false);
    // The forwarding must use reg storage `_tag[`, never the wire form.
    assert!(
        !h.contains("_let_tag["),
        "Vec reg forwarded to thread inst must not use `_let_` (wire) prefix:\n{h}"
    );
    assert!(
        h.contains("_inst__threads.tag_0 = _tag[0]")
            || h.contains("_inst__threads.tag_0  = _tag[0]"),
        "expected Vec reg forwarded as `_tag[0]` into thread inst:\n{h}"
    );
}

#[test]
fn test_native_sim_vec_inst_output_wire_feeds_indexed_let() {
    // Regression for arch-com#437: a sub-instance Vec output connected to a
    // declared Vec wire must update the wire's `_let_` storage, because parent
    // expressions such as `values[idx]` read that storage. The broken native
    // sim path wrote a separate implicit inst-output array instead, so indexed
    // lets observed stale zeroes.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_vec_inst_output_wire/Probe.arch")
        .arg("--tb")
        .arg("tests/native_vec_inst_output_wire/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native Vec inst output wire probe");
    assert!(
        out.status.success(),
        "native Vec inst output wire sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native Vec inst output wire"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_native_sim_vec_inst_input_wire_param_sized_fanout() {
    // Regression for arch-com#432 (Nic400 PMU integration). A parent
    // `wire Vec<T, PARAM>` driven in `comb` and connected to a sub-instance
    // Vec input port must emit per-element fan-out assignments. The native
    // sim codegen built its vec-wire-count map with the non-param-aware
    // `eval_const_expr`, which collapsed param-sized vecs to count=0; the
    // input fan-out loop then iterated zero times, silently leaving the
    // sub-instance's inputs default-constructed. Symptom that surfaced this:
    // Nic400Pmu counters stuck at zero in Nic400System integration even
    // though pulses were correctly computed in the parent's `comb` block.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_vec_inst_input_wire/Probe.arch")
        .arg("--tb")
        .arg("tests/native_vec_inst_input_wire/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native Vec inst input wire probe");
    assert!(
        out.status.success(),
        "native Vec inst input wire sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native Vec inst input wire"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_native_sim_bool_not_pipe_reg_outputs_and_ampamp() {
    // Regression for arch-com#492. Native sim used to tokenize `&&` as
    // bitwise `&` plus reduction `&` on the RHS, then infer
    // `not result_valid_out@0` as 8 bits. That emitted a reduction-AND
    // over `!result_valid_out`, so `not false` collapsed back to false
    // for byte-backed Bool values.
    //
    // Also covers the `||` sibling alias (review 2026-06-03): #493 added both
    // `&&` and `||` tokens but only exercised `&&`. `||` must lower to C++
    // logical `||`, not bitwise/reduction glue, on the same Bool pipe_reg path.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_bool_not/Probe.arch")
        .arg("--tb")
        .arg("tests/native_bool_not/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native Bool not pipe_reg probe");
    assert!(
        out.status.success(),
        "native Bool not pipe_reg sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PASS native Bool not pipe_reg"),
        "expected PASS marker in stdout:\n{stdout}"
    );

    let generated_cpp = std::fs::read_to_string(td.path().join("VNativeBoolNotProbe.cpp"))
        .expect("read generated native sim C++");
    assert!(
        generated_cpp.contains("idle_ampamp")
            && generated_cpp.contains("((!_busy_out) && (!_result_valid_out))"),
        "symbolic `&&` should lower to C++ logical &&, not bitwise/reduction glue:\n{generated_cpp}"
    );
    assert!(
        generated_cpp.contains("busy_pipebar")
            && generated_cpp.contains("(_busy_out || _result_valid_out)"),
        "symbolic `||` should lower to C++ logical ||, not bitwise/reduction glue:\n{generated_cpp}"
    );
    assert!(
        !generated_cpp.contains("0xffULL") && !generated_cpp.contains("0xFFULL"),
        "Bool `not` should not be reduced as an 8-bit all-ones value:\n{generated_cpp}"
    );
}

#[test]
fn test_native_sim_fsm_state_r_is_public_and_synced() {
    // HARC and SV-style white-box probes read `dut.state_r` for FSMs. Native
    // sim already traced this name, but only generated a private `_state_r`,
    // so generated C++ testbenches failed to compile when probing state.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_fsm_state_r/Probe.arch")
        .arg("--tb")
        .arg("tests/native_fsm_state_r/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native FSM state_r probe");
    assert!(
        out.status.success(),
        "native FSM state_r probe should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native FSM state_r probe"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_native_sim_thread_driven_top_pipe_reg_output_is_public() {
    // Regression for arch-com#472: lower_threads rewrites a thread-driven
    // top-level `port q: out pipe_reg<T,1>` into a registered output on the
    // synthesized `_threads` submodule, connected back to the parent's
    // registered output port. Native sim already copied the submodule value
    // into the parent's private shadow `_q`, but failed to update public
    // field `q`, so C++ testbenches polling `dut.q` saw stale zeroes.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_pipe_reg_thread_output/Probe.arch")
        .arg("--tb")
        .arg("tests/native_pipe_reg_thread_output/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native pipe_reg thread output probe");
    assert!(
        out.status.success(),
        "native pipe_reg thread output sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native pipe_reg thread output"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_native_sim_wait_until_fold_target_reaches_post_action_state() {
    // Regression for the folded wait-until exit target: after
    // `wait until go; phase <= 1; wait 2 cycle; phase <= 2;`, default native
    // sim must update phase on the go-detection edge and then continue into
    // the counted-wait state. If the folded wait targets the absorbed action
    // state instead, the native sim state machine gets stuck and never reaches
    // phase=2.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_wait_until_fold_target/Probe.arch")
        .arg("--tb")
        .arg("tests/native_wait_until_fold_target/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native wait-until folded target probe");
    assert!(
        out.status.success(),
        "native wait-until folded target sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native wait-until folded target"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_nic400_master_port_marks_non_power_of_two_decode_holes_oor() {
    // PR #487 added a default-slave DECERR path for out-of-range accesses,
    // but the original OOR predicate only checked high address bits above
    // REGION_BITS+NS_W. That misses decode holes when NUM_SLAVES is not a
    // power of two: with NUM_SLAVES=3 and NS_W=2, slave index 3 has no
    // backing thread and must still route to the default slave.
    let bus = include_str!("../examples/nic400/BusAxi4.arch");
    let master = include_str!("../examples/nic400/Nic400MasterPort.arch").replace(
        "param NUM_SLAVES:    const = 4;",
        "param NUM_SLAVES:    const = 3;",
    );
    let sv = compile_to_sv(&format!("{bus}\n{master}"));
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        trimmed.contains("assign m_ar_oor = m_ar_addr[ADDR_WIDTH - 1:REGION_BITS + NS_W] != 0 || m_ar_slv >= NUM_SLAVES;")
            || trimmed.contains("assign m_ar_oor = (m_ar_addr[ADDR_WIDTH - 1:REGION_BITS + NS_W] != 0) || (m_ar_slv >= NUM_SLAVES);"),
        "read-side OOR decode must treat hole indices >= NUM_SLAVES as DECERR:\n{sv}",
    );
    assert!(
        trimmed.contains("assign m_aw_oor = m_aw_addr[ADDR_WIDTH - 1:REGION_BITS + NS_W] != 0 || m_aw_slv >= NUM_SLAVES;")
            || trimmed.contains("assign m_aw_oor = (m_aw_addr[ADDR_WIDTH - 1:REGION_BITS + NS_W] != 0) || (m_aw_slv >= NUM_SLAVES);"),
        "write-side OOR decode must treat hole indices >= NUM_SLAVES as DECERR:\n{sv}",
    );
}

#[test]
fn test_expect_fatal_harness_catches_bounds_violation() {
    // Smoke test for the `expect_verilator_fatal` helper in
    // `tests/common/mod.rs`. The Probe.arch fixture writes to a
    // `Vec<UInt<8>, 4>` from a 3-bit input index — values 4..7 are
    // out of bounds, so the codegen-emitted SVA `_auto_bound_vec_0`
    // must trip under Verilator `--assert`. We pin the substring to
    // the specific label so a future rename / regression of the
    // bounds-emission code regresses this test loudly rather than
    // silently passing on some unrelated fatal.
    common::expect_verilator_fatal(
        "tests/expect_fatal_smoke/Probe.arch",
        "tests/expect_fatal_smoke/tb.cpp",
        "Probe",
        "BOUNDS VIOLATION: Probe._auto_bound_vec_0",
    );
}

/// Verifies the concurrent SVA at Nic400WidthAdapter.arch:299-300
/// (`ar_burst_supported`) actually fires under Verilator `--assert`
/// when a FIXED-burst AR handshake reaches the adapter's wide master
/// port. Consumes the `expect_verilator_fatal_multi` harness landed
/// in arch-com#453 (multi-source variant — the WidthAdapter depends
/// on `BusAxi4.arch`, which arch build resolves on the same command
/// line because `.archi` artifacts are gitignored).
///
/// Surfaced as the missing CI coverage in arch-com#447 §4 follow-up
/// to arch-com#441 (which added the SVAs) and arch-com#450 (which
/// shipped only a manual repro because no expect-fatal harness
/// existed yet).
#[test]
fn test_nic400_width_adapter_fixed_burst_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400WidthAdapter.arch",
            "examples/nic400/BusAxi4.arch",
        ],
        "examples/nic400/tb_nic400_width_adapter_fixed_reject.cpp",
        "Nic400WidthAdapter",
        // Matches the $fatal string emitted by `assert ar_burst_supported:`
        // at Nic400WidthAdapter.arch:299. Verifying the exact SV codegen
        // shape ("ar_burst_supported: assert property (...) else $fatal(1,
        // \"ASSERTION FAILED: Nic400WidthAdapter.ar_burst_supported\")")
        // means a rename or codegen regression trips this test loudly.
        "ASSERTION FAILED: Nic400WidthAdapter.ar_burst_supported",
    );
}

// ────────────────────────────────────────────────────────────────────
// PR #440 — three `src/elaborate.rs` tree-walker arms (issue #447 §5)
// ────────────────────────────────────────────────────────────────────
//
// PR #440 landed three tree-walker completeness fixes inside a nic400
// demo PR without isolated test coverage. Each fix added a missing
// recursive arm to a walker that synthesizes lowered-thread SV.
// arch-com#447 §5 flagged the missing isolated tests; these three
// regressions pin the exact shapes so a future tree-walker regression
// trips a dedicated test rather than a nic400 system test failure.
//
// All three were verified against the pre-#440 commit (1973d92) — each
// fixture distinguishes pre/post #440 behaviour by inspecting a
// specific token in the lowered SV (see per-test fingerprint).

#[test]
fn test_elaborate_440_bus_port_type_param_binary() {
    // Site 1: `src/elaborate.rs::subst_type_expr_for_lower` (around line
    // 3623) gained recursion into Binary/Unary/Ternary/Clog2 so that
    // bus-port widths like `UInt<DATA_W / 8>` substitute every operand
    // when `lower_threads` synthesizes the `_<mod>_threads` sub-module.
    // Pre-#440, the inner `DATA_W` ident leaked into the sub-module's
    // port list (which only knows the outer module's `DATA_WIDTH`).
    let source =
        include_str!("regression/issues/elaborate_440/bus_port_type_param_binary/Probe.arch");
    let sv = compile_to_sv(source);

    // The lowered `_Probe_threads` sub-module is the one that exercises
    // `subst_type_expr_for_lower`. Locate its body and verify the strb
    // port width substituted the bus's local `DATA_W` to the outer
    // module's caller-bound `DATA_WIDTH`.
    let threads_start = sv.find("module _Probe_threads").expect(
        "expected lowered sub-module `_Probe_threads` in SV (the thread \
         lowering is what exercises `subst_type_expr_for_lower`):\n{sv}",
    );
    let threads_end = sv[threads_start..]
        .find("endmodule")
        .map(|e| threads_start + e)
        .unwrap_or(sv.len());
    let threads_body = &sv[threads_start..threads_end];

    assert!(
        threads_body.contains("[DATA_WIDTH / 8-1:0] up_strb"),
        "expected lowered `_Probe_threads` sub-module to declare \
         `up_strb` with the outer module's param-bound width \
         (DATA_WIDTH / 8); `subst_type_expr_for_lower` must recurse \
         into the Binary expression `DATA_W / 8` so the inner ident \
         substitutes. Got sub-module body:\n{threads_body}"
    );
    assert!(
        !threads_body.contains("[DATA_W / 8-1:0]")
            && !threads_body.contains("[DATA_W /8-1:0]")
            && !threads_body.contains("DATA_W-1:0] up_strb"),
        "found buggy unresolved `DATA_W` in `_Probe_threads` strb port \
         width — `subst_type_expr_for_lower` failed to recurse into the \
         Binary arithmetic shape. Sub-module body:\n{threads_body}"
    );
}

#[test]
fn test_elaborate_440_for_loop_iter_in_function_call() {
    // Site 2: `src/elaborate.rs::rewrite_var_expr` (around line 6274)
    // gained an `ExprKind::FunctionCall` arm so a for-loop iter
    // referenced inside a function-call argument substitutes to the
    // per-loop counter ident. Pre-#440, the raw `b` leaked into the
    // lowered SV as an undeclared variable.
    let source =
        include_str!("regression/issues/elaborate_440/for_loop_iter_in_function_call/Probe.arch");
    let sv = compile_to_sv(source);

    // Single thread → ti=0 → counter ident = `_t0_loop_cnt_0`.
    // The function call `step(b.zext<8>())` must substitute `b` into
    // `_t0_loop_cnt_0` inside the argument.
    assert!(
        sv.contains("step_8(8'($unsigned(_t0_loop_cnt_0)))"),
        "expected `step_8` function call to receive the per-thread loop \
         counter `_t0_loop_cnt_0` as its argument — `rewrite_var_expr` \
         must recurse into FunctionCall args to substitute the iter var \
         `b`. Got SV:\n{sv}"
    );
    // The raw iter ident `b` must NOT appear as a function-call arg.
    // Look for the specific buggy shape from pre-#440.
    assert!(
        !sv.contains("step_8(8'($unsigned(b)))"),
        "found buggy unresolved iter var `b` in `step_8(...)` argument \
         — `rewrite_var_expr` failed to recurse into FunctionCall args. \
         SV:\n{sv}"
    );
}

#[test]
fn test_elaborate_440_rename_ident_in_function_call() {
    // Site 3: `src/elaborate.rs::rename_ident_in_expr` (around line
    // 6464) gained an `ExprKind::FunctionCall` arm so the per-thread
    // counter rename (`_loop_cnt_{id}` → `_t{ti}_loop_cnt_{id}`)
    // descends into function-call args. Sites 2 and 3 form a pair: 2
    // substitutes the user-written iter var into `_loop_cnt_{id}`, 3
    // renames `_loop_cnt_{id}` to its per-thread form. A multi-thread
    // fixture exercises the rename for both ti=0 and ti=1.
    let source =
        include_str!("regression/issues/elaborate_440/rename_ident_in_function_call/Probe.arch");
    let sv = compile_to_sv(source);

    // Both threads' function-call args must carry the per-thread
    // renamed counter, not the bare `_loop_cnt_0` (rename failure) and
    // not the raw user `a`/`b` (substitution failure).
    assert!(
        sv.contains("step_8(8'($unsigned(_t0_loop_cnt_0)))"),
        "expected thread 0 to call `step_8(_t0_loop_cnt_0)` — \
         `rename_ident_in_expr` must descend into FunctionCall args to \
         rename `_loop_cnt_0` for ti=0. Got SV:\n{sv}"
    );
    assert!(
        sv.contains("step_8(8'($unsigned(_t1_loop_cnt_0)))"),
        "expected thread 1 to call `step_8(_t1_loop_cnt_0)` — \
         `rename_ident_in_expr` must descend into FunctionCall args to \
         rename `_loop_cnt_0` for ti=1. Got SV:\n{sv}"
    );
    // Pre-#440 buggy fingerprints: raw `a`/`b` (site 2 missing the
    // FunctionCall arm) or bare `_loop_cnt_0` (site 3 missing it).
    assert!(
        !sv.contains("step_8(8'($unsigned(a)))") && !sv.contains("step_8(8'($unsigned(b)))"),
        "found buggy raw iter var (`a` or `b`) in `step_8(...)` \
         argument — `rewrite_var_expr` failed to recurse into \
         FunctionCall args (site 2 paired with site 3). SV:\n{sv}"
    );
    assert!(
        !sv.contains("step_8(8'($unsigned(_loop_cnt_0)))"),
        "found bare `_loop_cnt_0` in `step_8(...)` argument — \
         `rename_ident_in_expr` failed to recurse into FunctionCall \
         args so the per-thread rename didn't fire (site 3). SV:\n{sv}"
    );
}

// ─── --thread-sim mutex policy arbitration ────────────────────────────────

#[test]
fn test_thread_sim_honors_round_robin_mutex_policy_without_warning() {
    let source = r#"
        module SharedBus
          port clk:      in Clock<SysDomain>;
          port rst_n:    in Reset<Async, Low>;
          port bus_valid: out Bool;
          port bus_addr:  out UInt<32>;
          port bus_ready: in Bool;
          port done_0:   out Bool;
          port done_1:   out Bool;

          resource shared_bus : mutex<round_robin>;

          thread Writer_0 on clk rising, rst_n low
            lock shared_bus
              bus_valid = 1;
              bus_addr  = 32'h1000;
              wait until bus_ready;
              bus_valid = 0;
            end lock shared_bus
            done_0 = 1;
            wait until bus_ready;
          end thread Writer_0

          thread Writer_1 on clk rising, rst_n low
            lock shared_bus
              bus_valid = 1;
              bus_addr  = 32'h2000;
              wait until bus_ready;
              bus_valid = 0;
            end lock shared_bus
            done_1 = 1;
            wait until bus_ready;
          end thread Writer_1
        end module SharedBus
    "#;
    let warnings = compile_to_thread_sim_collect_warnings(source);
    assert!(
        warnings
            .iter()
            .all(|w| !w.message.contains("--thread-sim ignores mutex policy")),
        "thread-sim should implement mutex policy instead of warning; got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>(),
    );

    let h = compile_to_thread_sim_h(source);
    assert!(
        h.contains("_resource_shared_bus_last_grant")
            && h.contains("_resource_shared_bus_select()")
            && h.contains("(_resource_shared_bus_last_grant + 1 + _step) % 2"),
        "round-robin mutex should emit per-resource rotation state and selector:\n{h}",
    );
}

#[test]
fn test_thread_sim_emits_lru_and_weighted_mutex_policy_state() {
    let source = r#"
        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go: in Bool;
          port done_0: out Bool;
          port done_1: out Bool;

          resource lru_lk: mutex<lru>;
          resource weighted_lk: mutex<weighted<3> >;

          thread on clk rising, rst low
            wait until go;
            lock lru_lk
              done_0 = 1;
              wait 1 cycle;
            end lock lru_lk
            lock weighted_lk
              wait 1 cycle;
            end lock weighted_lk
          end thread

          thread on clk rising, rst low
            wait until go;
            lock lru_lk
              done_1 = 1;
              wait 1 cycle;
            end lock lru_lk
            lock weighted_lk
              wait 1 cycle;
            end lock weighted_lk
          end thread
        end module M
    "#;

    let warnings = compile_to_thread_sim_collect_warnings(source);
    assert!(
        warnings
            .iter()
            .all(|w| !w.message.contains("--thread-sim ignores mutex policy")),
        "lru/weighted mutex policies should not be downgraded to warnings: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>(),
    );

    let h = compile_to_thread_sim_h(source);
    assert!(
        h.contains("_resource_lru_lk_lru_order[2] = {0, 1}")
            && h.contains("_resource_lru_lk_lru_order[_rank]")
            && h.contains("_resource_lru_lk_lru_order[2 - 1] = _tid"),
        "lru mutex should emit per-resource recency stack:\n{h}",
    );
    assert!(
        h.contains("_resource_weighted_lk_credits[2] = {3, 3}")
            && h.contains("_resource_weighted_lk_credits[_idx] = 3")
            && h.contains("_resource_weighted_lk_credits[_tid]--"),
        "weighted mutex should emit per-resource credit counters:\n{h}",
    );
}

#[test]
fn test_thread_sim_no_warning_on_priority_mutex_policy() {
    // `mutex<priority>` is the scheduler's native ordering — no warning.
    // This pins the absence-of-warning property so a future tweak that
    // makes the warning unconditional would fail this test.
    let source = r#"
        module SharedBus
          port clk:      in Clock<SysDomain>;
          port rst_n:    in Reset<Async, Low>;
          port bus_valid: out Bool;
          port bus_addr:  out UInt<32>;
          port bus_ready: in Bool;
          port done_0:   out Bool;
          port done_1:   out Bool;

          resource shared_bus : mutex<priority>;

          thread Writer_0 on clk rising, rst_n low
            lock shared_bus
              bus_valid = 1;
              bus_addr  = 32'h1000;
              wait until bus_ready;
              bus_valid = 0;
            end lock shared_bus
            done_0 = 1;
            wait until bus_ready;
          end thread Writer_0

          thread Writer_1 on clk rising, rst_n low
            lock shared_bus
              bus_valid = 1;
              bus_addr  = 32'h2000;
              wait until bus_ready;
              bus_valid = 0;
            end lock shared_bus
            done_1 = 1;
            wait until bus_ready;
          end thread Writer_1
        end module SharedBus
    "#;
    let warnings = compile_to_thread_sim_collect_warnings(source);
    let policy_warnings: Vec<&str> = warnings
        .iter()
        .filter(|w| w.message.contains("--thread-sim ignores mutex policy"))
        .map(|w| w.message.as_str())
        .collect();
    assert!(
        policy_warnings.is_empty(),
        "expected no policy-downgrade warning for mutex<priority>; got: {:?}",
        policy_warnings,
    );
}

#[test]
fn test_thread_sim_honors_custom_mutex_policy_with_hook() {
    let source = r#"
        function PickHigh(req_mask: UInt<2>, _last: UInt<2>) -> UInt<2>
          return req_mask & 2'b10;
        end function PickHigh

        module M
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Async, Low>;
          port go0: in Bool;
          port go1: in Bool;
          port done_0: out Bool;
          port done_1: out Bool;

          resource shared_lk: mutex<PickHigh>
            hook grant_select(req_mask: UInt<2>, last_grant: UInt<2>) -> UInt<2>
                 = PickHigh(req_mask, last_grant);
          end resource shared_lk

          thread on clk rising, rst low
            wait until go0;
            lock shared_lk
              done_0 = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread

          thread on clk rising, rst low
            wait until go1;
            lock shared_lk
              done_1 = 1;
              wait 1 cycle;
            end lock shared_lk
          end thread
        end module M
    "#;
    let warnings = compile_to_thread_sim_collect_warnings(source);
    assert!(
        warnings
            .iter()
            .all(|w| !w.message.contains("--thread-sim ignores mutex policy")),
        "custom mutex policy should dispatch its hook instead of warning; got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>(),
    );

    let h = compile_to_thread_sim_h(source);
    assert!(
        h.contains("#include \"VFunctions.h\"")
            && h.contains("PickHigh(_req, _resource_shared_lk_last_grant_onehot)")
            && h.contains("_resource_shared_lk_last_grant_onehot = (1ULL << _tid)"),
        "custom mutex policy should include VFunctions and call the hook selector:\n{h}",
    );
}

/// Verifies the `ar_wrap_len_legal_apb` concurrent SVA at
/// Nic400ApbBridge.arch fires under Verilator `--assert` when a WRAP
/// burst is issued with an illegal 3-beat `ar_len = 2` (AXI4 §A3.4.1
/// requires WRAP `ax_len ∈ {1, 3, 7, 15}`). Follow-up to arch-com#447
/// §4 — earlier WRAP support (#440) only checked the reserved
/// `ar_burst == 3` code; legal-shape checks beyond that were missing.
#[test]
fn test_nic400_apb_bridge_wrap_illegal_len_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400ApbBridge.arch",
            "examples/nic400/BusAxi4.arch",
            "stdlib/BusApb.arch",
        ],
        "examples/nic400/tb_nic400_apb_bridge_wrap_len_illegal.cpp",
        "Nic400ApbBridge",
        "ASSERTION FAILED: Nic400ApbBridge.ar_wrap_len_legal_apb",
    );
}

/// Verifies the `ar_wrap_addr_aligned_apb` concurrent SVA at
/// Nic400ApbBridge.arch fires under Verilator `--assert` when a WRAP
/// burst is issued with a base address misaligned to `(1 << ar_size)`
/// (AXI4 §A3.4.1). We drive `ar_addr = 0x8003, ar_size = 2` — the
/// low 2 bits must be clear for a 4-byte access. Follow-up to
/// arch-com#447 §4.
#[test]
fn test_nic400_apb_bridge_wrap_unaligned_addr_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400ApbBridge.arch",
            "examples/nic400/BusAxi4.arch",
            "stdlib/BusApb.arch",
        ],
        "examples/nic400/tb_nic400_apb_bridge_wrap_addr_unaligned.cpp",
        "Nic400ApbBridge",
        "ASSERTION FAILED: Nic400ApbBridge.ar_wrap_addr_aligned_apb",
    );
}

/// Verifies the `ar_wrap_len_legal_widthadapter` concurrent SVA at
/// Nic400WidthAdapter.arch fires under Verilator `--assert` when a
/// WRAP burst is issued with an illegal 3-beat `ar_len = 2`. The
/// width-adapter's downsizing path would otherwise forward an
/// out-of-spec slave burst whose byte-count-preserved scaling no
/// longer wraps at the expected window boundary. Follow-up to
/// arch-com#447 §4 — earlier WRAP support (#441) only checked the
/// burst-type code, not the burst shape.
#[test]
fn test_nic400_width_adapter_wrap_illegal_len_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400WidthAdapter.arch",
            "examples/nic400/BusAxi4.arch",
        ],
        "examples/nic400/tb_nic400_width_adapter_wrap_len_illegal.cpp",
        "Nic400WidthAdapter",
        "ASSERTION FAILED: Nic400WidthAdapter.ar_wrap_len_legal_widthadapter",
    );
}

/// Verifies the `ar_wrap_addr_aligned_widthadapter` concurrent SVA
/// at Nic400WidthAdapter.arch fires under Verilator `--assert` when a
/// WRAP burst is issued with a base address misaligned to
/// `(1 << ar_size)` (AXI4 §A3.4.1). We drive
/// `ar_addr = 0x8003, ar_size = 2`. Follow-up to arch-com#447 §4.
#[test]
fn test_nic400_width_adapter_wrap_unaligned_addr_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400WidthAdapter.arch",
            "examples/nic400/BusAxi4.arch",
        ],
        "examples/nic400/tb_nic400_width_adapter_wrap_addr_unaligned.cpp",
        "Nic400WidthAdapter",
        "ASSERTION FAILED: Nic400WidthAdapter.ar_wrap_addr_aligned_widthadapter",
    );
}

/// Verifies the `ar_incr_no_4k_cross_widthadapter` concurrent SVA at
/// Nic400WidthAdapter.arch fires under Verilator `--assert` when an
/// INCR burst's footprint crosses a 4 KB address boundary (AXI4
/// §A3.4.1). PR #466 added the SVA but argued that the APB bridge's
/// matching SVA was sufficient CI coverage; arch-com PR #477's
/// Finding 6 asked for an explicit WidthAdapter-side TB that pins the
/// behaviour against future refactors (e.g. accidental switch from
/// pre-scaling `m.ar_*` to post-scaling `s.ar_*` operands in the
/// boundary computation).
///
/// Stimulus: `M_DATA_W=64` default, `ar_addr=0x0FF8, ar_size=3,
/// ar_len=7` ⇒ 8 master beats × 8 B = 64 B span from 0x0FF8 to
/// 0x1037, straddling the 4 KB boundary at 0x1000. RATIO=2 ⇒ slave
/// sees axlen=15, axsize=2 over the same byte span — confirming the
/// "byte-count-preserved" identity that makes the master-side SVA
/// equivalent to a hypothetical slave-side one.
#[test]
fn test_nic400_width_adapter_incr_4k_cross_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400WidthAdapter.arch",
            "examples/nic400/BusAxi4.arch",
        ],
        "examples/nic400/tb_nic400_width_adapter_incr_4k_cross.cpp",
        "Nic400WidthAdapter",
        "ASSERTION FAILED: Nic400WidthAdapter.ar_incr_no_4k_cross_widthadapter",
    );
}

// ── Multi-driver detection (SFG Check 1, issue #375) ─────────────────────────

/// Run through all compile stages up to typecheck and return any errors.
fn typecheck_source(source: &str) -> Result<(), Vec<arch::diagnostics::CompileError>> {
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let ast = elaborate::lower_tlm_target_threads(ast).expect("tlm_target");
    let ast = elaborate::lower_tlm_initiator_calls(ast).expect("tlm_initiator");
    let ast = elaborate::lower_threads_with_opts(ast, &elaborate::ThreadLowerOpts::default())
        .expect("lower_threads");
    let ast = elaborate::lower_pipe_reg_ports(ast).expect("lower_pipe_reg");
    let ast = elaborate::lower_credit_channel_dispatch(ast).expect("credit_channel");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    checker.check().map(|_| ())
}

fn has_multi_driver_error(source: &str, signal: &str) -> bool {
    match typecheck_source(source) {
        Err(errs) => errs.iter().any(|e| {
            matches!(e, arch::diagnostics::CompileError::MultipleDrivers { name, .. }
                if name == signal)
        }),
        Ok(_) => false,
    }
}

/// Repro C2: two separate `comb` blocks driving the same output wire.
/// Each `comb` becomes a distinct `always_comb` in SV — a genuine multi-driver.
#[test]
fn test_multi_driver_two_comb_blocks_same_output() {
    let source = r#"
module TwoCombBlocks
  port a: in Bool;
  port out: out Bool;
  comb
    out = a;
  end comb
  comb
    out = false;
  end comb
end module TwoCombBlocks
"#;
    assert!(
        has_multi_driver_error(source, "out"),
        "expected MultipleDrivers for `out` (two comb blocks)"
    );
}

/// Within a single `comb` block, multiple conditional writes to the same
/// wire (default + if-override) are ONE logical driver — no error.
#[test]
fn test_multi_driver_single_comb_block_conditional_no_error() {
    let source = r#"
module SingleCombConditional
  port sel: in Bool;
  port out: out Bool;
  comb
    out = false;
    if sel
      out = true;
    end if
  end comb
end module SingleCombConditional
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "single comb block with conditional write should NOT be a multi-driver"
    );
}

/// Repro C7: two separate `inst` scalar outputs connected to the same parent wire.
/// Both child instances drive `result` — a genuine multi-driver.
#[test]
fn test_multi_driver_two_inst_scalar_outputs_same_wire() {
    let source = r#"
module DriverA
  port out_val: out Bool;
  comb
    out_val = true;
  end comb
end module DriverA

module DriverB
  port out_val: out Bool;
  comb
    out_val = false;
  end comb
end module DriverB

module ParentC7
  port result: out Bool;
  inst a: DriverA
    out_val -> result;
  end inst a
  inst b: DriverB
    out_val -> result;
  end inst b
end module ParentC7
"#;
    assert!(
        has_multi_driver_error(source, "result"),
        "expected MultipleDrivers for `result` (two inst scalar outputs)"
    );
}

/// A `comb` block and an `inst` scalar output both driving the same wire.
#[test]
fn test_multi_driver_comb_and_inst_same_wire() {
    let source = r#"
module Src
  port val: out Bool;
  comb
    val = true;
  end comb
end module Src

module ParentCombInst
  port out: out Bool;
  comb
    out = false;
  end comb
  inst s: Src
    val -> out;
  end inst s
end module ParentCombInst
"#;
    assert!(
        has_multi_driver_error(source, "out"),
        "expected MultipleDrivers for `out` (comb block + inst output)"
    );
}

/// `shared(or)` ports are intentionally multi-driven and must be exempt.
#[test]
fn test_multi_driver_shared_or_port_exempt() {
    let source = r#"
module SharedOrPort
  port a: in Bool;
  port b: in Bool;
  port out: out Bool shared(or);
  comb
    out = a;
  end comb
  comb
    out = b;
  end comb
end module SharedOrPort
"#;
    // shared(or) ports are exempt — typecheck should succeed
    assert!(
        typecheck_source(source).is_ok(),
        "shared(or) port driven from two comb blocks should NOT be a multi-driver error"
    );
}

/// `lhs_base_name` collapses `vec[i]` and `vec[j]` to the same underlying
/// signal — by design, because two `comb` blocks become two distinct
/// `always_comb` blocks driving the same `logic` array, and SV's
/// multi-driver rule treats any cross-block write into the array as a
/// conflict on the array as a whole. Two `comb` blocks writing different
/// indices of one Vec is therefore a real multi-driver and must error.
///
/// This test pins that design intent so a future change to
/// `lhs_base_name` (e.g. mistakenly granularizing per-index) breaks
/// loudly. Surfaced in the 2026-05-29 daily code-review pass — Finding
/// 2 in `ideas/2026-05-29-code-review-findings.md`.
#[test]
fn test_multi_driver_vec_index_from_two_blocks_errors() {
    let source = r#"
module VecIndexTwoBlocks
  port a: in UInt<8>;
  port b: in UInt<8>;
  port out0: out UInt<8>;
  port out1: out UInt<8>;
  wire data: Vec<UInt<8>, 2>;
  comb
    data[0] = a;
    out0 = data[0];
  end comb
  comb
    data[1] = b;
    out1 = data[1];
  end comb
end module VecIndexTwoBlocks
"#;
    assert!(
        has_multi_driver_error(source, "data"),
        "two comb blocks writing different Vec indices must trigger \
         MultipleDrivers — Vec is one SV array, two always_comb blocks \
         is a real conflict"
    );
}

/// Sister positive case: ONE `comb` block writing multiple Vec indices
/// must NOT error. This is the legitimate combinational fan-out pattern.
/// Surfaced in the 2026-05-29 daily code-review pass — Finding 2 in
/// `ideas/2026-05-29-code-review-findings.md`.
#[test]
fn test_multi_driver_vec_index_in_single_block_no_error() {
    let source = r#"
module VecIndexSingleBlock
  port a: in UInt<8>;
  port b: in UInt<8>;
  port out0: out UInt<8>;
  port out1: out UInt<8>;
  wire data: Vec<UInt<8>, 2>;
  comb
    data[0] = a;
    data[1] = b;
    out0 = data[0];
    out1 = data[1];
  end comb
end module VecIndexSingleBlock
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "one comb block writing multiple Vec indices is legal — \
         one driver, not many"
    );
}

/// POSITIVE regression: a `generate_for` over a Vec-of-bus port whose body
/// also forwards a sibling non-bus output element (`last -> out[i]`) must
/// type-check.  The Vec-of-bus connection (`b <- m[i]`) forces the
/// generate_for to unroll at elaboration time into N separate `inst`
/// blocks, each driving a DISTINCT element `out[<const i>]`.  Before the
/// fix, `lhs_base_name` collapsed every `out[0..N-1]` to a bare `out`, so
/// the inst-output driver tracker counted N drivers on `out` and reported a
/// phantom "multiple drivers" — rejecting valid code.  Each iteration
/// drives a different element, so this is legal.
#[test]
fn test_multi_driver_genfor_vecbus_sibling_element_no_error() {
    let source = r#"
bus BusVr
  param DATA_W: const = 8;
  valid: out Bool;
  ready: in  Bool;
  data:  out UInt<DATA_W>;
end bus BusVr

module VrTapScalar
  port b:    target BusVr<DATA_W=8>;
  port en:   in Bool;
  port last: out UInt<8>;
  comb
    b.ready = en;
    last    = b.data;
  end comb
end module VrTapScalar

module Fx1bMultiDriver
  param LANES: const = 3;
  port m:   target      Vec<BusVr<DATA_W=8>, LANES>;
  port en:  in unpacked Vec<Bool, LANES>;
  port out: out unpacked Vec<UInt<8>, LANES>;
  generate_for i in 0..LANES-1
    inst tap_i: VrTapScalar
      b    <- m[i];
      en   <- en[i];
      last -> out[i];
    end inst tap_i
  end generate_for
end module Fx1bMultiDriver
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "generate_for over Vec-of-bus forwarding distinct sibling elements \
         (out[i] per iteration) must NOT be a multi-driver — each iteration \
         drives a different element"
    );
}

/// NEGATIVE regression: even with the same Vec-of-bus generate_for shape,
/// if every iteration forwards to the SAME constant element (`last ->
/// out[0]`, not `out[i]`), that IS a genuine multiple-driver of `out[0]`
/// and must still error.  This pins that the fix distinguishes "distinct
/// element per iteration" (legal) from "same element driven N times"
/// (illegal) — it must not blanket-suppress the inst-output multi-driver
/// check for Vec elements.
#[test]
fn test_multi_driver_genfor_same_const_element_errors() {
    let source = r#"
bus BusVr
  param DATA_W: const = 8;
  valid: out Bool;
  ready: in  Bool;
  data:  out UInt<DATA_W>;
end bus BusVr

module VrTapScalar
  port b:    target BusVr<DATA_W=8>;
  port en:   in Bool;
  port last: out UInt<8>;
  comb
    b.ready = en;
    last    = b.data;
  end comb
end module VrTapScalar

module GenConstDrive
  param LANES: const = 3;
  port m:   target      Vec<BusVr<DATA_W=8>, LANES>;
  port en:  in unpacked Vec<Bool, LANES>;
  port out: out unpacked Vec<UInt<8>, LANES>;
  generate_for i in 0..LANES-1
    inst tap_i: VrTapScalar
      b    <- m[i];
      en   <- en[i];
      last -> out[0];
    end inst tap_i
  end generate_for
end module GenConstDrive
"#;
    assert!(
        has_multi_driver_error(source, "out[0]"),
        "all generate_for iterations driving the same constant element \
         out[0] must still trigger MultipleDrivers"
    );
}

/// NEGATIVE regression (no generate_for): two plain `inst` items both wired
/// to the same constant element `out[0]` must error.  Pins that the
/// per-constant-index inst-output granularity still catches a same-element
/// double drive.
#[test]
fn test_multi_driver_two_inst_same_vec_element_errors() {
    let source = r#"
module Src
  port v: out UInt<8>;
  comb
    v = 8'd1;
  end comb
end module Src

module ParentDoubleElem
  port out: out unpacked Vec<UInt<8>, 2>;
  inst a: Src
    v -> out[0];
  end inst a
  inst b: Src
    v -> out[0];
  end inst b
end module ParentDoubleElem
"#;
    assert!(
        has_multi_driver_error(source, "out[0]"),
        "two inst outputs driving the same Vec element out[0] must trigger \
         MultipleDrivers"
    );
}

/// POSITIVE sister case: two plain `inst` items wired to DISTINCT constant
/// elements (`out[0]` and `out[1]`) must NOT error — each drives a separate
/// element via its own continuous driver, which is legal in SV.
#[test]
fn test_multi_driver_two_inst_distinct_vec_elements_no_error() {
    let source = r#"
module Src
  port v: out UInt<8>;
  comb
    v = 8'd1;
  end comb
end module Src

module ParentDistinctElem
  port out: out unpacked Vec<UInt<8>, 2>;
  inst a: Src
    v -> out[0];
  end inst a
  inst b: Src
    v -> out[1];
  end inst b
end module ParentDistinctElem
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "two inst outputs driving distinct Vec elements out[0]/out[1] must \
         NOT be a multi-driver"
    );
}

/// POSITIVE regression (extends #528 to nested vectors): a `generate_for`
/// over a 2-D `Vec<Vec<Bus>>` wire whose body forwards inst outputs to
/// `edges[r][c]` (two const indices) must type-check.  The producer
/// generate_for unrolls into separate inst blocks each driving a DISTINCT
/// nested element (`edges[0][0]`, `edges[0][1]`, `edges[1][0]`,
/// `edges[1][1]`).  #528 retained only ONE trailing constant index, so
/// `edges[r][c]` collapsed to the key `edges[<c>]` and every row aliased
/// onto the same key — `edges[1][0]` and `edges[1][1]` both became
/// `edges[1]` and looked double-driven.  Retaining ALL leading constant
/// indices keeps the four elements distinct.
#[test]
fn test_multi_driver_genfor_nested2d_vecbus_distinct_elements_no_error() {
    let source = r#"
bus BusVr
  param DATA_W: const = 8;
  valid: out Bool;
  ready: in  Bool;
  data:  out UInt<DATA_W>;
end bus BusVr

module Fx6Prod
  param COLS:   const = 2;
  param DATA_W: const = 8;
  port v: in Bool;
  port d: in UInt<DATA_W>;
  port o: initiator Vec<BusVr<DATA_W=DATA_W>, COLS>;
  comb
    o[0].valid = v; o[0].data = d;
    o[1].valid = v; o[1].data = d;
  end comb
end module Fx6Prod

module Fx6Cons
  param ROWS:   const = 2;
  param DATA_W: const = 8;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port ins: target    Vec<BusVr<DATA_W=DATA_W>, ROWS>;
  port acc: initiator BusVr<DATA_W=DATA_W>;
  reg acc_r: UInt<DATA_W> reset rst => 0;
  comb
    ins[0].ready = true;
    ins[1].ready = true;
    acc.valid = true;
    acc.data  = acc_r;
  end comb
  seq on clk rising
    if ins[0].valid and ins[1].valid
      acc_r <= acc_r +% (ins[0].data +% ins[1].data);
    elsif ins[0].valid
      acc_r <= acc_r +% ins[0].data;
    elsif ins[1].valid
      acc_r <= acc_r +% ins[1].data;
    end if
  end seq
end module Fx6Cons

module Fx6Nested2D
  param ROWS:   const = 2;
  param COLS:   const = 2;
  param DATA_W: const = 8;
  type EdgeBus = BusVr<DATA_W=DATA_W>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port v:   in unpacked Vec<Bool, ROWS>;
  port d:   in unpacked Vec<UInt<DATA_W>, ROWS>;
  port acc: initiator Vec<EdgeBus, COLS>;
  wire edges: Vec<Vec<EdgeBus, COLS>, ROWS>;
  generate_for r in 0..ROWS-1
    inst prod_r: Fx6Prod
      param COLS = COLS;
      param DATA_W = DATA_W;
      v <- v[r];
      d <- d[r];
      for c in 0..COLS-1
        o[c] -> edges[r][c];
      end for
    end inst prod_r
  end generate_for
  generate_for c in 0..COLS-1
    inst cons_c: Fx6Cons
      param ROWS = ROWS;
      param DATA_W = DATA_W;
      clk <- clk;
      rst <- rst;
      for k in 0..ROWS-1
        ins[k] <- edges[k][c];
      end for
      acc -> acc[c];
    end inst cons_c
  end generate_for
end module Fx6Nested2D
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "generate_for over a 2-D Vec<Vec<Bus>> forwarding distinct nested \
         elements (edges[r][c] per iteration) must NOT be a multi-driver — \
         each (r,c) is a separate element"
    );
}

/// NEGATIVE regression (nested-vector sibling of
/// `test_multi_driver_two_inst_same_vec_element_errors`): two `inst` items
/// both forwarding to the SAME nested constant element `edges[0][0]` IS a
/// genuine multiple-driver and must still error.  This pins that retaining
/// all leading constant indices keeps "distinct nested element" legal while
/// "same nested element twice" stays illegal — the fix must not weaken
/// multi-driver detection for nested vectors.
#[test]
fn test_multi_driver_two_inst_same_nested2d_element_errors() {
    let source = r#"
bus BusVr
  param DATA_W: const = 8;
  valid: out Bool;
  ready: in  Bool;
  data:  out UInt<DATA_W>;
end bus BusVr

module Fx6Prod
  param COLS:   const = 2;
  param DATA_W: const = 8;
  port v: in Bool;
  port d: in UInt<DATA_W>;
  port o: initiator Vec<BusVr<DATA_W=DATA_W>, COLS>;
  comb
    o[0].valid = v; o[0].data = d;
    o[1].valid = v; o[1].data = d;
  end comb
end module Fx6Prod

module Fx6Bad2D
  param COLS:   const = 2;
  param DATA_W: const = 8;
  type EdgeBus = BusVr<DATA_W=DATA_W>;
  port v:   in Bool;
  port d:   in UInt<DATA_W>;
  wire edges: Vec<Vec<EdgeBus, COLS>, 2>;
  inst prod_a: Fx6Prod
    param COLS = COLS;
    param DATA_W = DATA_W;
    v <- v;
    d <- d;
    o[0] -> edges[0][0];
    o[1] -> edges[0][1];
  end inst prod_a
  inst prod_b: Fx6Prod
    param COLS = COLS;
    param DATA_W = DATA_W;
    v <- v;
    d <- d;
    o[0] -> edges[0][0];
    o[1] -> edges[1][1];
  end inst prod_b
end module Fx6Bad2D
"#;
    assert!(
        has_multi_driver_error(source, "edges[0][0]"),
        "two inst outputs driving the same nested Vec element edges[0][0] \
         must still trigger MultipleDrivers"
    );
}

/// Bus-typed wires connected from both initiator and target insts must NOT
/// trigger multi-driver.  This is the canonical TLM bus wire pattern.
#[test]
fn test_multi_driver_bus_wire_two_inst_connections_no_error() {
    let source = r#"
bus Msg
  data: out UInt<8>;
  ack:  in  Bool;
end bus Msg

module Sender
  port m: initiator Msg;
  comb
    m.data = 8'h42;
  end comb
end module Sender

module Receiver
  port m: target Msg;
  port ack_out: out Bool;
  comb
    m.ack   = true;
    ack_out = m.data[0];
  end comb
end module Receiver

module BusWireTop
  port ack_out: out Bool;
  wire link: Msg;
  inst tx: Sender
    m -> link;
  end inst tx
  inst rx: Receiver
    m    -> link;
    ack_out -> ack_out;
  end inst rx
end module BusWireTop
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "bus wire connected from initiator + target insts must NOT be a multi-driver error"
    );
}

// ── Dead-skid comb-feedback analysis (issue #245) ────────────────────────────

/// Run the dead-skid analysis on the PRE-thread-lowering AST (so
/// `ModuleBodyItem::Thread` is still present) for module `module`.
fn dead_skid_hazards(source: &str, module: &str) -> Vec<arch::signal_flow::DeadSkidHazard> {
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let m = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Module(m) if m.name.name == module => Some(m),
            _ => None,
        })
        .expect("module not found");
    arch::signal_flow::find_dead_skid_hazards(m, &ast)
}

/// Cross-module (one boundary deep): a thread combinationally drives the ALU's
/// operand wires and reads back the ALU's `is_zero` output through a parent
/// wire.  During dead-skid cycles the operand drives drop to 0, so `is_zero`
/// asserts spuriously — the canonical arch-ibex pitfall #11.
#[test]
fn test_dead_skid_cross_module_hazard() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Alu
  port a: in UInt<8>;
  port b: in UInt<8>;
  port is_zero: out Bool;
  comb
    is_zero = (a + b) == 0;
  end comb
end module Alu

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port reg done: out Bool reset rst => false;
  wire a_drv: UInt<8>;
  wire b_drv: UInt<8>;
  wire is_zero_w: Bool;
  inst alu: Alu
    a <- a_drv;
    b <- b_drv;
    is_zero -> is_zero_w;
  end inst alu
  thread Worker on clk rising, rst high
    a_drv = 8'd5;
    b_drv = 8'd3;
    wait until is_zero_w;
    done <= true;
  end thread Worker
end module Top
"#;
    let hz = dead_skid_hazards(source, "Top");
    assert!(
        hz.iter().any(|h| h.read_signal == "is_zero_w"
            && (h.driven_signal == "a_drv" || h.driven_signal == "b_drv")),
        "expected a cross-module dead-skid hazard on `is_zero_w`, got {hz:?}"
    );
}

/// Negative control: the thread reads an upstream INPUT (`go`) rather than the
/// routed combinational output — no dead-skid hazard.
#[test]
fn test_dead_skid_reads_upstream_input_no_hazard() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Alu
  port a: in UInt<8>;
  port b: in UInt<8>;
  port is_zero: out Bool;
  comb
    is_zero = (a + b) == 0;
  end comb
end module Alu

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port go: in Bool;
  port reg done: out Bool reset rst => false;
  wire a_drv: UInt<8>;
  wire b_drv: UInt<8>;
  wire is_zero_w: Bool;
  inst alu: Alu
    a <- a_drv;
    b <- b_drv;
    is_zero -> is_zero_w;
  end inst alu
  thread Worker on clk rising, rst high
    a_drv = 8'd5;
    b_drv = 8'd3;
    wait until go;
    done <= true;
  end thread Worker
end module Top
"#;
    let hz = dead_skid_hazards(source, "Top");
    assert!(
        hz.is_empty(),
        "thread reading an upstream input must NOT be a dead-skid hazard, got {hz:?}"
    );
}

/// Intra-module variant: thread comb-drives `x`, a parent comb block routes
/// `fb = x`, and the thread reads `fb`.
#[test]
fn test_dead_skid_intra_module_hazard() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module IntraTop
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port reg done: out Bool reset rst => false;
  wire x: Bool;
  wire fb: Bool;
  comb
    fb = x;
  end comb
  thread W on clk rising, rst high
    x = true;
    wait until fb;
    done <= true;
  end thread W
end module IntraTop
"#;
    let hz = dead_skid_hazards(source, "IntraTop");
    assert!(
        hz.iter()
            .any(|h| h.read_signal == "fb" && h.driven_signal == "x"),
        "expected intra-module dead-skid hazard `x -> fb`, got {hz:?}"
    );
}

/// The read can also live in `default comb`, not just in `wait until` or an
/// RHS inside the thread body. During dead-skid cycles the comb-driven source
/// still collapses to its default, so a default output that mirrors the routed
/// comb feedback is hazardous and must be reported.
#[test]
fn test_dead_skid_default_comb_read_hazard() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module Alu
  port a: in UInt<8>;
  port z: out Bool;
  comb
    z = (a == 0);
  end comb
end module Alu

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port done: out Bool;
  port go: in Bool;
  wire a_drv: UInt<8>;
  wire z_w: Bool;
  inst alu: Alu
    a <- a_drv;
    z -> z_w;
  end inst alu
  thread Worker on clk rising, rst high
    default comb
      done = z_w;
    end default
    a_drv = 8'd1;
    wait until go;
  end thread Worker
end module Top
"#;
    let hz = dead_skid_hazards(source, "Top");
    assert!(
        hz.iter()
            .any(|h| h.read_signal == "z_w" && h.driven_signal == "a_drv"),
        "expected default-comb read hazard `a_drv -> z_w`, got {hz:?}"
    );
}

/// A module with no threads yields no hazards (cheap early-out path).
#[test]
fn test_dead_skid_no_threads_no_hazard() {
    let source = r#"
module Pure
  port a: in Bool;
  port y: out Bool;
  comb
    y = not a;
  end comb
end module Pure
"#;
    assert!(dead_skid_hazards(source, "Pure").is_empty());
}

/// Negative control (registered mirror): the thread drives a REGISTER (`<=`),
/// a comb block mirrors it (`active = active_r`), and the thread reads the
/// mirror.  Registered values hold across dead-skid cycles, so this is NOT a
/// hazard.  (Mirrors the axi_dma_thread `ThreadMm2s` shape that surfaced the
/// false-positive during the Stage 2 sweep.)
#[test]
fn test_dead_skid_registered_mirror_no_hazard() {
    let source = r#"
domain SysDomain
  freq_mhz: 100
end domain SysDomain

module RegMirror
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port go: in Bool;
  port reg done: out Bool reset rst => false;
  reg active_r: Bool reset rst => false;
  wire active: Bool;
  comb
    active = active_r;
  end comb
  thread Worker on clk rising, rst high
    active_r <= true;
    wait until active;
    done <= true;
  end thread Worker
end module RegMirror
"#;
    let hz = dead_skid_hazards(source, "RegMirror");
    assert!(
        hz.is_empty(),
        "comb mirror of a registered thread output must NOT be a dead-skid hazard, got {hz:?}"
    );
}

/// Thread-map HTML overlay: a thread carrying a dead-skid hazard renders the
/// ⚠ badge table, the escaped comb path, and highlights the read source line.
#[test]
fn test_thread_map_html_renders_hazard_overlay() {
    use arch::lexer::Span;
    use arch::thread_map::*;
    let src = ThreadMapSource {
        start: 0,
        end: 18,
        filename: "f.arch".into(),
        source: "line1\nline2\nline3\n".into(),
    };
    let map = ThreadMap {
        modules: vec![ThreadMapModule {
            module_name: "Top".into(),
            generated_module_name: "_Top_threads".into(),
            span: Span::new(0, 18),
            threads: vec![ThreadMapThread {
                name: "Worker".into(),
                index: 0,
                once: false,
                span: Span::new(0, 18),
                states: vec![],
                hazards: vec![CombFeedbackHazard {
                    read_signal: "is_zero_w".into(),
                    driven_signal: "a_drv".into(),
                    path_summary: "a_drv -> is_zero_w".into(),
                    read_span: Span::new(7, 10), // inside "line2"
                }],
            }],
        }],
    };
    let html = render_html(&map, &[src], "t");
    assert!(
        html.contains("⚠ dead-skid comb feedback"),
        "expected the hazard badge table"
    );
    assert!(
        html.contains("a_drv -&gt; is_zero_w"),
        "expected the escaped comb path"
    );
    assert!(
        html.contains("src-line hazard"),
        "expected the read source line to be highlighted"
    );
}

/// A clean thread (no hazards) renders no badge and no highlight.
#[test]
fn test_thread_map_html_no_hazard_no_overlay() {
    use arch::lexer::Span;
    use arch::thread_map::*;
    let map = ThreadMap {
        modules: vec![ThreadMapModule {
            module_name: "Top".into(),
            generated_module_name: "_Top_threads".into(),
            span: Span::new(0, 10),
            threads: vec![ThreadMapThread {
                name: "W".into(),
                index: 0,
                once: false,
                span: Span::new(0, 10),
                states: vec![],
                hazards: vec![],
            }],
        }],
    };
    let html = render_html(&map, &[], "t");
    // Note: the CSS block always contains the literal "⚠ dead-skid read"
    // (source-line annotation rule), so match the table-specific header.
    assert!(
        !html.contains("⚠ dead-skid comb feedback"),
        "clean thread must not render a hazard badge"
    );
    assert!(!html.contains("class=\"src-line hazard\""));
}

/// The `pragma allow_dead_skid_feedback;` suppression knob parses and sets the
/// module flag the lint consults.
#[test]
fn test_pragma_allow_dead_skid_feedback_parses() {
    let source = r#"
module M
  pragma allow_dead_skid_feedback;
  port a: in Bool;
  port y: out Bool;
  comb
    y = a;
  end comb
end module M
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let m = ast
        .items
        .iter()
        .find_map(|it| match it {
            arch::ast::Item::Module(m) if m.name.name == "M" => Some(m),
            _ => None,
        })
        .expect("module");
    assert!(
        m.allow_dead_skid_feedback,
        "pragma allow_dead_skid_feedback should set the module flag"
    );
}

/// Unit: `comb_reachable_from` follows forward edges transitively and excludes
/// seeds unless reached via a cycle.
#[test]
fn test_comb_reachable_from_transitive() {
    use std::collections::{HashMap, HashSet};
    let mut fwd: HashMap<String, HashSet<String>> = HashMap::new();
    fwd.entry("a".into()).or_default().insert("b".into());
    fwd.entry("b".into()).or_default().insert("c".into());
    let seeds: HashSet<String> = ["a".to_string()].into_iter().collect();
    let reached = arch::signal_flow::comb_reachable_from(&seeds, &fwd);
    assert!(reached.contains("b") && reached.contains("c"));
    assert!(
        !reached.contains("a"),
        "seed must not appear unless via a cycle"
    );
}

/// AW-side mirror of `test_nic400_apb_bridge_wrap_illegal_len_is_rejected_by_sva`.
///
/// Closes §4 from `ideas/2026-05-28-code-review-findings.md`: the AW
/// `aw_wrap_len_legal_apb` SVA was added in PR #456 but had no
/// expect-fatal TB exercising it. A codegen bug that broke the AW SVA
/// label or condition would not have been caught by CI.
#[test]
fn test_nic400_apb_bridge_wrap_illegal_aw_len_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400ApbBridge.arch",
            "examples/nic400/BusAxi4.arch",
            "stdlib/BusApb.arch",
        ],
        "examples/nic400/tb_nic400_apb_bridge_wrap_len_illegal_aw.cpp",
        "Nic400ApbBridge",
        "ASSERTION FAILED: Nic400ApbBridge.aw_wrap_len_legal_apb",
    );
}

/// AW-side mirror of `test_nic400_apb_bridge_wrap_unaligned_addr_is_rejected_by_sva`.
///
/// Closes §4 from `ideas/2026-05-28-code-review-findings.md`: AW
/// alignment SVA also needed expect-fatal coverage so the per-size
/// alignment check is verified to fire on the write path, not just the
/// read path.
#[test]
fn test_nic400_apb_bridge_wrap_unaligned_aw_addr_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400ApbBridge.arch",
            "examples/nic400/BusAxi4.arch",
            "stdlib/BusApb.arch",
        ],
        "examples/nic400/tb_nic400_apb_bridge_wrap_addr_unaligned_aw.cpp",
        "Nic400ApbBridge",
        "ASSERTION FAILED: Nic400ApbBridge.aw_wrap_addr_aligned_apb",
    );
}

/// Verifies the `ar_incr_no_4k_cross_apb` concurrent SVA at
/// Nic400ApbBridge.arch fires under Verilator `--assert` when an INCR
/// burst crosses a 4 KB page boundary (AXI4 §A3.4.1). We drive
/// `ar_addr = 0x0FF8, ar_size = 2, ar_len = 3` — 4 beats × 4 bytes =
/// 16 bytes, starting 8 short of the 0x1000 boundary, so the burst
/// crosses by 8 bytes. Closes §5 from arch-com#463.
#[test]
fn test_nic400_apb_bridge_incr_4k_cross_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400ApbBridge.arch",
            "examples/nic400/BusAxi4.arch",
            "stdlib/BusApb.arch",
        ],
        "examples/nic400/tb_nic400_apb_bridge_incr_4k_cross.cpp",
        "Nic400ApbBridge",
        "ASSERTION FAILED: Nic400ApbBridge.ar_incr_no_4k_cross_apb",
    );
}

/// Verifies the `ar_excl_len_legal_apb` concurrent SVA at
/// Nic400ApbBridge.arch fires under Verilator `--assert` when an
/// EXCLUSIVE burst is issued with `ar_len > 15` (AXI4 §A7.2.4 —
/// exclusive accesses are capped at 16 beats). We drive `ar_lock = 1,
/// ar_len = 16`. Closes §5 from arch-com#463 (cardinality half; the
/// pow-2-byte and base-alignment halves of §A7.2.4 are deferred).
#[test]
fn test_nic400_apb_bridge_excl_len_illegal_is_rejected_by_sva() {
    common::expect_verilator_fatal_multi(
        &[
            "examples/nic400/Nic400ApbBridge.arch",
            "examples/nic400/BusAxi4.arch",
            "stdlib/BusApb.arch",
        ],
        "examples/nic400/tb_nic400_apb_bridge_excl_len_illegal.cpp",
        "Nic400ApbBridge",
        "ASSERTION FAILED: Nic400ApbBridge.ar_excl_len_legal_apb",
    );
}

// ────────────────────────────────────────────────────────────────────
// NIC-400 §16.1 — AHB-Lite slave-side (mirrored) bridge
// ────────────────────────────────────────────────────────────────────
//
// Nic400AhbSlaveBridge.arch is the MIRROR of Nic400AhbBridge.arch: it
// flips both bus roles so the AXI4 side is a `target` (driven by the
// fabric) and the AHB-Lite side is an `initiator` (driving an external
// AHB peripheral). This exercises bus target/initiator direction-flip
// and thread lowering in the reverse direction from the shipped
// master-side bridge. End-to-end behaviour (AXI read/write through to
// the AHB peripheral, plus HRESP→AXI-resp SLVERR mapping) is verified by
// examples/nic400/Nic400AhbSlaveBridge_test.harc under
// `harc sim --check-backends` (ARCH native sim ≡ Verilator).
//
// This regression pins the direction-flip in the lowered SV: a rename or
// a target/initiator regression in the bus flatten / thread-lowering path
// would change these port directions and trip the test.
#[test]
fn test_nic400_ahb_slave_bridge_flips_bus_roles_in_lowered_sv() {
    let source = concat!(
        include_str!("../examples/nic400/Nic400AhbSlaveBridge.arch"),
        "\n",
        include_str!("../examples/nic400/BusAxi4.arch"),
        "\n",
        include_str!("../examples/nic400/BusAhbLite.arch"),
    );
    let sv = compile_to_sv(source);

    // AXI4 is the `target`: the bridge SEES AR/AW/W as inputs and DRIVES
    // the R/B channels + the *_ready backpressure as outputs. (On the
    // master-side bridge these directions are reversed.)
    assert!(
        sv.contains("input logic axi_ar_valid,"),
        "AXI4 target: ar_valid must be an INPUT to the bridge:\n{sv}"
    );
    assert!(
        sv.contains("output logic axi_ar_ready,"),
        "AXI4 target: ar_ready must be an OUTPUT of the bridge:\n{sv}"
    );
    assert!(
        sv.contains("output logic axi_r_valid,") && sv.contains("input logic axi_r_ready,"),
        "AXI4 target: r_valid is an OUTPUT, r_ready an INPUT:\n{sv}"
    );

    // AHB-Lite is the `initiator` (master): the bridge DRIVES the address/
    // control/HWDATA as outputs and SAMPLES HRDATA/HREADY/HRESP as inputs.
    assert!(
        sv.contains("output logic h_hsel,") && sv.contains("output logic h_hwrite,"),
        "AHB initiator: HSEL/HWRITE must be OUTPUTs of the bridge:\n{sv}"
    );
    assert!(
        sv.contains("output logic [1:0] h_htrans,"),
        "AHB initiator: HTRANS must be an OUTPUT of the bridge:\n{sv}"
    );
    assert!(
        sv.contains("input logic h_hready") && sv.contains("input logic h_hresp"),
        "AHB initiator: HREADY/HRESP must be INPUTs to the bridge:\n{sv}"
    );
    assert!(
        sv.contains("input logic [DATA_WIDTH-1:0] h_hrdata,")
            && sv.contains("output logic [DATA_WIDTH-1:0] h_hwdata,"),
        "AHB initiator: HRDATA in, HWDATA out:\n{sv}"
    );

    // Two independent threads (read + write) lower to two state machines
    // in the `_threads` sub-module.
    assert!(
        sv.contains("module _Nic400AhbSlaveBridge_threads"),
        "expected lowered `_Nic400AhbSlaveBridge_threads` sub-module:\n{sv}"
    );
    assert!(
        sv.contains("_t0_state") && sv.contains("_t1_state"),
        "expected two lowered thread state registers (read + write):\n{sv}"
    );
}

// ────────────────────────────────────────────────────────────────────
// NIC-400 read+write async-clock CDC bridge (GPV ring)
// ────────────────────────────────────────────────────────────────────
//
// `examples/nic400/Nic400CdcAxi4Rw.arch` extends the read-only
// `Nic400CdcAxi4` to a full AXI4 read+write async-clock bridge: it adds
// the AW/W (M->S) and B (S->M) channel-crossing FIFOs alongside the
// existing AR (M->S) and R (S->M) ones, so a config access can write AND
// read a GPV register across an unrelated clock-domain boundary. This
// closes the write-path-CDC gap the read-only bridge left open.
//
// This regression pins the structural shape: the bridge must instantiate
// all FIVE channel-crossing FIFOs. A lowering regression that drops a
// write-path channel (e.g. an `inst`-emission or bus-port-direction bug
// affecting only AW/W/B) trips this loudly rather than silently shipping
// a read-only bridge under the read+write name.
#[test]
fn test_nic400_cdc_axi4_rw_bridge_crosses_all_five_axi_channels() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("gpv_ring.sv");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("examples/nic400/BusAxi4.arch")
        .arg("examples/nic400/Nic400GpvArCdcFifo.arch")
        .arg("examples/nic400/Nic400GpvRCdcFifo.arch")
        .arg("examples/nic400/Nic400GpvAwCdcFifo.arch")
        .arg("examples/nic400/Nic400GpvWCdcFifo.arch")
        .arg("examples/nic400/Nic400GpvBCdcFifo.arch")
        .arg("examples/nic400/Nic400CdcAxi4Rw.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("invoke arch build");
    assert!(
        build.status.success(),
        "arch build of the read+write CDC bridge should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let sv = std::fs::read_to_string(&sv_out).expect("read emitted SV");

    // The bridge module body must instantiate every one of the five
    // channel-crossing FIFOs. Verilator flattens the instance names, so
    // each appears as a `<Fifo> <inst> (` instantiation header in the SV.
    for fifo_module in [
        "Nic400GpvArCdcFifo", // AR: M->S (read address)
        "Nic400GpvRCdcFifo",  // R:  S->M (read data)
        "Nic400GpvAwCdcFifo", // AW: M->S (write address)  ← write-path
        "Nic400GpvWCdcFifo",  // W:  M->S (write data)      ← write-path
        "Nic400GpvBCdcFifo",  // B:  S->M (write response)  ← write-path
    ] {
        assert!(
            sv.contains(fifo_module),
            "read+write CDC bridge SV is missing the {fifo_module} crossing FIFO \
             (a dropped channel would silently regress the bridge to read-only):\n{sv}"
        );
    }
}

// ────────────────────────────────────────────────────────────────────
// Compiler fix: derived params re-evaluate under inst-level overrides,
// and variant discovery is transitive through nested instantiation.
// ────────────────────────────────────────────────────────────────────
//
// `Mid` has a base param `W` and derived params `DA = W+2`, `DB = W+4`.
// `Top` instantiates `Mid` with `W = 5`. The derived params must re-evaluate
// (DA=7, DB=9) so `Mid`'s ports are sized correctly, AND the nested `Leaf`
// instances (param `PW = DA` / `PW = DB`) must rewrite to variants that
// transitive variant discovery actually created (`Leaf__PW_7`, `Leaf__PW_9`).
// Before the fix this failed two ways: a type-check width mismatch on the
// derived-param-sized port, and an "undefined module Leaf" for the variant
// that default-param discovery never produced.
#[test]
fn test_derived_param_override_reevaluates_and_creates_nested_variants() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("dpo.sv");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("tests/derived_param_override/Leaf.arch")
        .arg("tests/derived_param_override/Mid.arch")
        .arg("tests/derived_param_override/Top.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("invoke arch build");
    assert!(
        build.status.success(),
        "arch build with derived-param override should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let sv = std::fs::read_to_string(&sv_out).expect("read emitted SV");

    // Both nested-Leaf variants — sized by the *re-evaluated* derived params
    // (DA = 5+2 = 7, DB = 5+4 = 9) — must exist.
    assert!(
        sv.contains("module Leaf__PW_7"),
        "missing Leaf__PW_7 variant (derived param DA=W+2 must re-evaluate to 7 \
         under the W=5 override):\n{sv}"
    );
    assert!(
        sv.contains("module Leaf__PW_9"),
        "missing Leaf__PW_9 variant (derived param DB=W+4 must re-evaluate to 9 \
         under the W=5 override):\n{sv}"
    );
}

#[test]
fn test_variant_name_with_bit63_param_is_valid_identifier() {
    // A `const` param value with bit 63 set is a negative `i64`. The variant
    // name mangler used to format it directly, splicing a bare `-` into the
    // emitted module/class name (`Leaf__TAG_-9223372036854775807`) — illegal in
    // both SystemVerilog and C++ identifiers, so the design would emit
    // uncompilable SV and the sim model would not build. The minus is now
    // rendered as `n` (`TAG_n9223...`); positive values stay pure digits, so the
    // mapping is collision-free. Two distinct high-bit variants exercise it.
    let source = r#"
module Leaf
  param TAG: const = 64'h0;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port o: out UInt<64>;
  reg r: UInt<64> reset rst => 0;
  comb o = r; end comb
  seq on clk rising
    r <= TAG;
  end seq
end module Leaf

module Top
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port o0: out UInt<64>;
  port o1: out UInt<64>;
  inst l0: Leaf
    param TAG = 64'h0000_0000_0000_0001;
    clk <- clk; rst <- rst; o -> o0;
  end inst l0
  inst l1: Leaf
    param TAG = 64'h8000_0000_0000_0001;
    clk <- clk; rst <- rst; o -> o1;
  end inst l1
end module Top
"#;
    let sv = compile_to_sv(source);
    // The high-bit variant must appear with an `n`-rendered suffix, never a
    // bare `-` that would break the SV/C++ identifier.
    assert!(
        sv.contains("module Leaf__TAG_n9223372036854775807"),
        "bit-63 param variant must mangle the leading minus to `n`:\n{sv}"
    );
    assert!(
        !sv.contains("Leaf__TAG_-"),
        "variant name must not contain a bare `-` (invalid identifier):\n{sv}"
    );
    // The low-bit variant is unchanged (pure digits).
    assert!(
        sv.contains("module Leaf__TAG_1"),
        "positive param variant naming must be unchanged:\n{sv}"
    );
}

// ────────────────────────────────────────────────────────────────────
// Compiler fix: VlWide<->_arch_u128 conversion respects the real word
// count, so a 65–96-bit (VlWide<3>) payload is not written out of bounds.
// ────────────────────────────────────────────────────────────────────
//
// A 66-bit payload reg maps to `VlWide<3>` in the ARCH sim. The conversion
// helpers used to assume a 4-word `VlWide<4>` and touched `w[3]`, overrunning
// the 3-word backing array and clobbering the adjacent 1-bit `dn_valid`
// member (read back as 64 instead of 1). This test runs the slice in the
// ARCH sim and asserts the valid bit and the wide payload survive.
#[test]
fn test_wide_payload_slice_conversion_does_not_clobber_adjacent_member() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let td = tempfile::tempdir().expect("tempdir");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/wide_payload_slice/WidePayloadSlice.arch")
        .arg("--tb")
        .arg("tests/wide_payload_slice/tb_wide_payload_slice.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for WidePayloadSlice");
    assert!(
        out.status.success(),
        "arch sim of the 66-bit-payload slice should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PASS wide_payload_slice"),
        "expected `PASS wide_payload_slice` (dn_valid==1, payload intact) in \
         arch sim stdout — a VlWide<3> conversion overrun would corrupt \
         dn_valid; got:\n{stdout}"
    );
}

// ────────────────────────────────────────────────────────────────────
// NIC-400 per-SLAVE register-slice fabric (AMIB timing isolation)
// ────────────────────────────────────────────────────────────────────
//
// `Nic400FabricRsSlave` drops a `Nic400EdgeRegSlice` on every (fabric →
// slave) edge — the AMIB / master-IF position, the slave-side mirror of
// `Nic400FabricRs1`'s per-master ASIB slices. The slave-side AXI4 id width
// is `SLAVE_ID_W = 5`, so each slice is instantiated with `ID_W =
// SLAVE_ID_W`, which forces the derived per-channel payload params
// (AR/AW=66, R=40, W=37, B=7) to re-evaluate and creates distinct
// `RegSliceChannel` variants for those widths.
//
// This test pins three things that broke real compiler bugs while building
// this IP (all fixed in the same PR):
//   1. The `RegSliceChannel__PAYLOAD_W_{66,40,37,7}` variants exist — i.e.
//      derived params re-evaluate under the inst-level `ID_W` override and
//      variant discovery is transitive through the nested instantiation.
//   2. The per-slave slice's whole-bus `up <- s_int[j]` connection to the
//      `Vec<SlaveBus, N>` *wire* `s_int` emits a packed indexed reference
//      `s_int_<sig>[j]`, NOT the broken flattened `s_int_<j>_<sig>` that
//      referenced a non-existent net.
//   3. Four slices are instantiated, one per slave, each with
//      `.ID_W(SLAVE_ID_W)`.
#[test]
fn test_nic400_fabric_rs_slave_inserts_per_slave_reg_slices_with_packed_wire_conns() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("rs_slave.sv");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("examples/nic400/BusAxi4.arch")
        .arg("examples/nic400/RegSliceChannel.arch")
        .arg("examples/nic400/Nic400EdgeRegSlice.arch")
        .arg("examples/nic400/Nic400MasterPort.arch")
        .arg("examples/nic400/Nic400SlavePort.arch")
        .arg("examples/nic400/Nic400DefaultSlave.arch")
        .arg("examples/nic400/Nic400Fabric.arch")
        .arg("examples/nic400/Nic400FabricRsSlave.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("invoke arch build");
    assert!(
        build.status.success(),
        "arch build of the per-slave reg-slice fabric should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let sv = std::fs::read_to_string(&sv_out).expect("read emitted SV");

    // (1) Derived params re-evaluated under ID_W=5: the slave-side payload
    // widths produce these RegSliceChannel variants. PAYLOAD_W_66 (= AR/AW
    // with ID_W=5) is the one that does NOT exist under the default ID_W=3,
    // so its presence proves the override flowed through the derived params
    // and transitive variant discovery.
    for variant in [
        "RegSliceChannel__PAYLOAD_W_66", // AR / AW : addr+id(5)+len+size+burst+lock+cache+prot+qos+region
        "RegSliceChannel__PAYLOAD_W_40", // R       : data+id(5)+resp+last
        "RegSliceChannel__PAYLOAD_W_37", // W       : data+strb+last
        "RegSliceChannel__PAYLOAD_W_7",  // B       : id(5)+resp
    ] {
        assert!(
            sv.contains(variant),
            "per-slave reg-slice fabric SV is missing the {variant} variant — the \
             inst-level `ID_W = SLAVE_ID_W` override must re-evaluate the derived \
             payload params and create this variant:\n{sv}"
        );
    }

    // (2) The whole-bus `up <- s_int[j]` connection to the Vec-of-bus *wire*
    // `s_int` must emit a packed indexed reference, e.g. `s_int_ar_valid[0]`.
    // The pre-fix codegen emitted the flattened `s_int_0_ar_valid`, which
    // referenced a net that was never declared (Verilator IMPLICIT, and a
    // dropped connection in the ARCH sim).
    assert!(
        sv.contains("s_int_ar_valid[0]"),
        "per-slave slice `up <- s_int[j]` must connect to the packed Vec-of-bus \
         wire element `s_int_ar_valid[0]`, not a flattened `s_int_0_ar_valid`:\n{sv}"
    );
    assert!(
        !sv.contains("s_int_0_ar_valid"),
        "per-slave slice connection must NOT emit the broken flattened \
         `s_int_0_ar_valid` net name (Vec-of-bus *wire* indexed-connection bug):\n{sv}"
    );

    // (3) Exactly four EdgeRegSlice instances (one per slave), each overriding
    // ID_W with SLAVE_ID_W.
    let slice_insts = sv
        .matches("Nic400EdgeRegSlice #(.ID_W(SLAVE_ID_W))")
        .count();
    assert_eq!(
        slice_insts, 4,
        "expected exactly 4 per-slave Nic400EdgeRegSlice instances each with \
         .ID_W(SLAVE_ID_W); found {slice_insts}:\n{sv}"
    );
}

// ────────────────────────────────────────────────────────────────────
// User-written `assert` SVA reset gating
// ────────────────────────────────────────────────────────────────────
//
// `doc/ARCH_HDL_Specification.md:7783` states that user-written
// `assert`/`cover` bodies are evaluated "at every clock edge under
// the construct's `posedge clk` with `disable iff (rst)`". The
// auto-emitted SVA family (`_auto_bound_*`, `_auto_div0_*`,
// `_auto_hs_*`, `_auto_thread_*`) already honours this — emission
// goes through the same `rst_active` extraction logic. User-written
// `assert` bodies were the outlier: `emit_assert_sva` ignored the
// module's reset polarity and produced bare `@(posedge clk)` SVAs.
//
// These tests pin the spec-aligned behaviour. Surfaced in the
// 2026-05-29 daily code-review pass — Finding 7 in
// `ideas/2026-05-29-code-review-findings.md` (originally
// mis-described as a nic400-local gap; the real fix is here in the
// compiler).

/// Active-low reset (`Reset<Async, Low>` → `!rst` in `disable iff`).
/// This is the nic400 convention — every nic400 module uses it.
#[test]
fn test_user_assert_sva_disables_iff_active_low_reset() {
    let source = r#"
module UserAssertActiveLow
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port a: in Bool;
  port b: in Bool;
  assert ab_consistent: a |-> b;
end module UserAssertActiveLow
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("ab_consistent: assert property (@(posedge clk) disable iff (!rst) a |-> b)"),
        "expected user assert to carry `disable iff (!rst)` (active-low \
         reset); got:\n{sv}"
    );
}

/// Active-high reset (`Reset<Sync>` defaults to High → bare `rst` in
/// `disable iff`).
#[test]
fn test_user_assert_sva_disables_iff_active_high_reset() {
    let source = r#"
module UserAssertActiveHigh
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port a: in Bool;
  port b: in Bool;
  assert ab_consistent: a |-> b;
end module UserAssertActiveHigh
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("ab_consistent: assert property (@(posedge clk) disable iff (rst) a |-> b)"),
        "expected user assert to carry `disable iff (rst)` (active-high \
         reset); got:\n{sv}"
    );
}

/// `cover` bodies must be gated the same way as `assert` — both share
/// the same lowering path.
#[test]
fn test_user_cover_sva_disables_iff_reset() {
    let source = r#"
module UserCoverReset
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port a: in Bool;
  cover seen_a: a;
end module UserCoverReset
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("seen_a: cover property (@(posedge clk) disable iff (!rst) a);"),
        "expected user cover to carry `disable iff (!rst)`; got:\n{sv}"
    );
}

/// A clock-only module with no reset port must NOT emit a bogus
/// `disable iff` clause. The existing `_auto_bound_*` emitters skip
/// `disable iff` when no reset port is found; user asserts must follow
/// suit.
#[test]
fn test_user_assert_sva_no_reset_no_disable_iff() {
    let source = r#"
module UserAssertNoReset
  port clk: in Clock<SysDomain>;
  port a: in Bool;
  port b: in Bool;
  assert ab_consistent: a |-> b;
end module UserAssertNoReset
"#;
    let sv = compile_to_sv(source);
    assert!(
        sv.contains("ab_consistent: assert property (@(posedge clk) a |-> b)"),
        "expected user assert to omit `disable iff` when no reset port \
         is declared; got:\n{sv}"
    );
    assert!(
        !sv.contains("disable iff"),
        "no `disable iff` should appear anywhere in a reset-less module's \
         user-assert SVA; got:\n{sv}"
    );
}

/// `arch check Foo.arch` for a module that references a `bus` type via a
/// port (`port m: initiator|target BusName`) must auto-discover the bus
/// definition (`BusName.arch` / `.archi`) from the same directory, the
/// same way `inst SubModule` auto-discovers `SubModule.archi`. Before this
/// was wired up, the dep scan only inspected `inst` nodes, so a single-file
/// check of a bus-consuming module failed with "unknown bus type".
#[test]
fn bus_port_type_auto_discovers_sibling_definition() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    std::fs::write(
        td.path().join("MyBus.arch"),
        "bus MyBus\n  cmd: out UInt<8>;\n  resp: in UInt<8>;\nend bus MyBus\n",
    )
    .unwrap();
    // NOTE: no `use MyBus;` — resolution must come purely from the
    // bus-port dependency scan.
    let consumer = td.path().join("Consumer.arch");
    std::fs::write(
        &consumer,
        "module Consumer\n  port clk: in Clock<SysDomain>;\n  \
         port m: initiator MyBus;\n  comb\n    m.cmd = 8'h0;\n  end comb\n\
         end module Consumer\n",
    )
    .unwrap();

    let out = std::process::Command::new(arch_bin)
        .arg("check")
        .arg(&consumer)
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "single-file check of a bus-consuming module should auto-discover \
         the sibling bus definition; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Control: with the bus definition absent, the check must still fail
    // with the unresolved-bus diagnostic (auto-discovery is additive, not a
    // silent pass).
    let td2 = tempfile::tempdir().expect("tempdir");
    let lonely = td2.path().join("Consumer.arch");
    std::fs::copy(&consumer, &lonely).unwrap();
    let out2 = std::process::Command::new(arch_bin)
        .arg("check")
        .arg(&lonely)
        .output()
        .expect("run arch check");
    assert!(
        !out2.status.success(),
        "check should fail when the referenced bus cannot be found"
    );
    assert!(
        String::from_utf8_lossy(&out2.stderr).contains("unknown bus type"),
        "expected 'unknown bus type' diagnostic; stderr:\n{}",
        String::from_utf8_lossy(&out2.stderr)
    );
}

/// The `.archi` bus interface emitted by `arch build` must parse back in —
/// `emit_bus_interface` previously wrote `port name: ...` members, but a
/// `bus` body is parsed with bare `name: dir Type;` members, so the
/// emitted interface tripped "'port' is a reserved keyword" on read-back.
/// This guards the round-trip now that bus interfaces are auto-discovered.
#[test]
fn bus_interface_archi_round_trips() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    std::fs::write(
        td.path().join("MyBus.arch"),
        "bus MyBus\n  cmd: out UInt<8>;\n  resp: in UInt<8>;\nend bus MyBus\n",
    )
    .unwrap();
    let consumer = td.path().join("Consumer.arch");
    std::fs::write(
        &consumer,
        "module Consumer\n  port clk: in Clock<SysDomain>;\n  \
         port m: initiator MyBus;\n  comb\n    m.cmd = 8'h0;\n  end comb\n\
         end module Consumer\n",
    )
    .unwrap();

    // Build the consumer — this emits `MyBus.archi` alongside the SV.
    let sv_out = td.path().join("Consumer.sv");
    let bld = std::process::Command::new(arch_bin)
        .arg("build")
        .arg(&consumer)
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("run arch build");
    assert!(
        bld.status.success(),
        "arch build failed; stderr:\n{}",
        String::from_utf8_lossy(&bld.stderr)
    );

    let bus_archi = td.path().join("MyBus.archi");
    assert!(
        bus_archi.exists(),
        "arch build should emit MyBus.archi next to the generated SV"
    );
    let archi_text = std::fs::read_to_string(&bus_archi).unwrap();
    assert!(
        archi_text.contains("cmd: out UInt<8>") && !archi_text.contains("port cmd"),
        "bus interface members must be bare `name: dir Type;`, not \
         `port`-prefixed; got:\n{archi_text}"
    );

    // The emitted interface must parse standalone (the round-trip).
    let chk = std::process::Command::new(arch_bin)
        .arg("check")
        .arg(&bus_archi)
        .output()
        .expect("run arch check");
    assert!(
        chk.status.success(),
        "emitted bus .archi must parse back in; stderr:\n{}",
        String::from_utf8_lossy(&chk.stderr)
    );
}

// ── Issue #306: wait-until exit assignment fold ───────────────────────────────

/// Basic fold: `wait until go; phase <= 2'd1;` — the register assignment
/// must appear inside the `if (go)` arm of state S0, not in a separate S1.
#[test]
fn test_wait_until_fold_basic() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port go: in Bool;
  port phase: out UInt<2>;
  reg phase_r: UInt<2> reset rst => 0;
  let phase = phase_r;
  thread on clk rising, rst high
    wait until go;
    phase_r <= 2'd1;
    wait until go;
  end thread
end module M
"#;
    let sv = compile_to_sv(source);
    // The assignment must be inside the if (go) block at state 0.
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        trimmed.contains("if (go) begin phase_r <= 2'd1;")
            || trimmed.contains("if (go) begin phase_r <= 2'(2'd1)"),
        "phase_r <= 2'd1 must be folded into the if(go) arm of the \
         wait_until state (issue #306):\n{sv}",
    );
    // The S1 action state should be is_folded — it must NOT appear as a
    // standalone `_t0_state == _t0_S1_action` check in the always_ff block.
    assert!(
        !sv.contains("_t0_state == _t0_S1_action"),
        "S1 must be folded (unreachable); no standalone S1 check \
         should appear in always_ff (issue #306):\n{sv}",
    );
}

/// Multi-assign fold: `wait until go; X <= A; Y <= B;` — both assignments
/// must appear in the same if(go) arm of state S0.
#[test]
fn test_wait_until_fold_multi_assign() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port go: in Bool;
  port x_out: out UInt<8>;
  port y_out: out UInt<8>;
  reg x_r: UInt<8> reset rst => 0;
  reg y_r: UInt<8> reset rst => 0;
  let x_out = x_r;
  let y_out = y_r;
  thread on clk rising, rst high
    wait until go;
    x_r <= 8'd10;
    y_r <= 8'd20;
    wait until go;
  end thread
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    // Both assigns must be inside the same if(go) arm.
    assert!(
        trimmed.contains("if (go) begin x_r <= 8'd10; y_r <= 8'd20;")
            || trimmed.contains("if (go) begin y_r <= 8'd20; x_r <= 8'd10;"),
        "both x_r and y_r assigns must fold into the same if(go) arm \
         (issue #306 multi-assign):\n{sv}",
    );
}

/// Fold correctness: `wait until go; X <= A; wait until done; Y <= B;` —
/// X folds into state 0's go-exit arm; Y folds into state 1's done-exit arm.
/// Verify state ordering is correct and no extra intermediate state exists.
#[test]
fn test_wait_until_fold_no_fold_past_second_wait() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port go: in Bool;
  port done: in Bool;
  port x_out: out UInt<8>;
  port y_out: out UInt<8>;
  reg x_r: UInt<8> reset rst => 0;
  reg y_r: UInt<8> reset rst => 0;
  let x_out = x_r;
  let y_out = y_r;
  thread on clk rising, rst high
    wait until go;
    x_r <= 8'd1;
    wait until done;
    y_r <= 8'd2;
  end thread
end module M
"#;
    let sv = compile_to_sv(source);
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    // x_r folds into state 0's go-exit arm.
    assert!(
        trimmed.contains("if (go) begin x_r <= 8'd1;"),
        "x_r must fold into state 0's if(go) arm (issue #306):\n{sv}",
    );
    // y_r folds into state 1's done-exit arm (the second wait_until).
    assert!(
        trimmed.contains("if (done) begin y_r <= 8'd2;"),
        "y_r must fold into the wait_until(done) state's if(done) arm \
         (issue #306 second-wait fold):\n{sv}",
    );
    // Both wait_until localparams must be declared (no states collapsed
    // beyond the fold).
    assert!(
        sv.contains("_wait_until"),
        "expected wait_until role localparam(s) in emitted SV:\n{sv}",
    );
}

/// Wait-N-cycle unaffected: `wait 3 cycle; X <= A;` — the counter states
/// must NOT be folded.  The fold applies only to `wait until`, not `wait N cycle`.
#[test]
fn test_wait_until_fold_wait_n_cycle_not_folded() {
    let source = r#"
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port go: in Bool;
  port x_out: out UInt<8>;
  reg x_r: UInt<8> reset rst => 0;
  let x_out = x_r;
  thread on clk rising, rst high
    wait until go;
    wait 3 cycle;
    x_r <= 8'd42;
  end thread
end module M
"#;
    let sv = compile_to_sv(source);
    // A wait_cycles localparam must be present (counter state kept).
    assert!(
        sv.contains("_wait_cycles"),
        "wait_cycles localparam must be emitted (counter states must not \
         be folded — issue #306):\n{sv}",
    );
    // x_r must NOT appear inside an if(go) arm — it fires after the counter
    // expires, not on the go-detection edge.
    let trimmed: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !trimmed.contains("if (go) begin x_r <= 8'd42"),
        "x_r must NOT be folded into state 0's if(go) arm when separated \
         by a wait N cycle (issue #306 wait-N-cycle unaffected):\n{sv}",
    );
}

// ────────────────────────────────────────────────────────────────────
// Inst-boundary clock-domain checking: rebind-OK vs cross-domain-data-NG
// ────────────────────────────────────────────────────────────────────
//
// `check_inst_cdc` (src/typecheck.rs) implements the spec rule documented
// at doc/ARCH_HDL_Specification.md §"CDC checking extends across `inst`
// boundaries": the compiler traces *clock* port connections to map a
// child's declared domains onto the parent's domains, then verifies that
// each *data* connection respects those boundaries.
//
// The subtle, deliberate consequence is **clock domain rebind**: a
// reusable child may declare a placeholder clock domain (commonly
// `Clock<SysDomain>`) and be instantiated under a *different* parent
// clock. That is NOT a violation — the connected clock fixes the
// instance's domain at the boundary, and there is no crossing *inside*
// the instance (it is clocked entirely by the one connected clock).
// `examples/nic400/Nic400GpvRing.arch` relies on exactly this (a
// `Clock<SysDomain>` GPV instantiated under `SClkDom`).
//
// What IS a violation is feeding a parent signal from domain A into a
// child *data* port that the child clocks in domain B. These tests pin
// both halves so a regression to `check_inst_cdc` cannot silently either
// (a) start rejecting legitimate clock rebind, or (b) stop catching a
// genuine cross-domain data crossing. The behaviour was previously
// exercised only by the un-wired `examples/cdc_inst_violation.arch`.

/// REBIND-OK: a `Clock<SysDomain>` child instantiated under a named
/// parent domain (`SClkDom`), fed only by same-(parent-)domain data, must
/// type-check clean. This is the `Nic400GpvRing` pattern in miniature.
#[test]
fn test_inst_clock_domain_rebind_is_not_a_violation() {
    let source = r#"
domain MClkDom
  freq_mhz: 200
end domain MClkDom

domain SClkDom
  freq_mhz: 150
end domain SClkDom

module GpvLike
  port clk:   in Clock<SysDomain>;
  port rst:   in Reset<Sync>;
  port wdata: in UInt<8>;
  port rdata: out UInt<8>;

  reg r: UInt<8> reset rst => 0;

  seq on clk rising
    r <= wdata;
  end seq

  comb
    rdata = r;
  end comb
end module GpvLike

module RingLike
  port m_clk: in Clock<MClkDom>;
  port s_clk: in Clock<SClkDom>;
  port rst:   in Reset<Sync>;
  port wdata: in UInt<8>;
  port rdata: out UInt<8>;

  reg sd: UInt<8> reset rst => 0;

  seq on s_clk rising
    sd <= wdata;
  end seq

  inst gpv: GpvLike
    clk   <- s_clk;
    rst   <- rst;
    wdata <- sd;
    rdata -> rdata;
  end inst gpv
end module RingLike
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(
        result.is_ok(),
        "rebinding a Clock<SysDomain> child to a named parent clock domain, \
         fed only by same-domain data, must NOT be a CDC violation \
         (this is the GPV-ring pattern). Got: {:?}",
        result.err()
    );
}

/// CROSS-DOMAIN-DATA-NG: a parent register clocked in domain A wired into
/// a child *data* port that the child clocks in domain B is a genuine CDC
/// hazard and must be rejected at the inst boundary — the clock rebind is
/// fine, the data crossing is not. Mirrors `examples/cdc_inst_violation.arch`.
#[test]
fn test_inst_cross_domain_data_is_a_violation() {
    let source = r#"
domain DomainA
  freq_mhz: 100
end domain DomainA

domain DomainB
  freq_mhz: 200
end domain DomainB

module Consumer
  port clk:      in Clock<DomainB>;
  port rst:      in Reset<Sync>;
  port data_in:  in UInt<8>;
  port data_out: out UInt<8>;

  reg r: UInt<8> reset rst => 0;

  seq on clk rising
    r <= data_in;
  end seq

  let data_out = r;
end module Consumer

module TopBad
  port clk_a:  in Clock<DomainA>;
  port clk_b:  in Clock<DomainB>;
  port rst:    in Reset<Sync>;
  port result: out UInt<8>;

  reg counter_a: UInt<8> reset rst => 0;

  seq on clk_a rising
    counter_a <= (counter_a + 1).trunc<8>();
  end seq

  inst cons: Consumer
    clk      <- clk_b;
    rst      <- rst;
    data_in  <- counter_a;
    data_out -> result;
  end inst cons
end module TopBad
"#;
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let parsed = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(parsed).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    let result = checker.check();
    assert!(result.is_err(), "expected an inst-boundary CDC violation");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| {
            let s = e.to_string();
            s.contains("CDC violation at instance `cons`")
                && s.contains("counter_a")
                && s.contains("DomainA")
        }),
        "expected a CDC-violation error naming instance `cons`, signal \
         `counter_a`, and its source domain `DomainA`, got: {:?}",
        errs
    );
}

// ────────────────────────────────────────────────────────────────────
// NIC-400 AXI3 endpoint shim + AXI4↔AXI3 protocol conversion
// (nic400_interconnect_spec.md §16.1, TRM DDI 0475E §1.2/§2.3.1)
// ────────────────────────────────────────────────────────────────────
//
// Three new examples: examples/nic400/BusAxi3.arch (AXI3 bus bundle),
// Nic400Axi4ToAxi3.arch (AXI4→AXI3 long-burst splitter), and
// Nic400Axi3ToAxi4.arch (AXI3→AXI4 programmable burst limiter).
//
// End-to-end behaviour is verified by the pre/post-edge C++ TBs
// tb_nic400_axi4_to_axi3_split.cpp and tb_nic400_axi3_to_axi4_limit.cpp
// (PASS under BOTH `arch sim` and Verilator — see PR description) and by
// the HARC TB Nic400Axi4ToAxi3_test.harc (`--check-backends` reports no
// divergence). These cargo tests pin the load-bearing SV codegen of the
// burst-split arithmetic so a future thread-lowering / width regression
// trips a dedicated test.

#[test]
fn test_nic400_axi4_to_axi3_burst_split_arithmetic_sv() {
    // The AXI4→AXI3 splitter lowers to a `_threads` sub-module that must:
    //   1. compute num_sub = ceil((ar_len+1)/16) as (ar_len + 16) >> 4;
    //   2. step each sub-burst start address by (i*16) << size;
    //   3. truncate the per-sub-burst AXI3 ar_len to 4 bits (max 16 beats);
    //   4. suppress the merged RLAST except on the final sub-burst
    //      (loop_cnt == num_sub-1).
    let bus4 = include_str!("../examples/nic400/BusAxi4.arch");
    let bus3 = include_str!("../examples/nic400/BusAxi3.arch");
    let dut = include_str!("../examples/nic400/Nic400Axi4ToAxi3.arch");
    let sv = compile_to_sv(&format!("{bus4}\n{bus3}\n{dut}"));
    let flat: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    // 1. ceil-div sub-burst count: (ar_len + 16) >> 4.
    assert!(
        flat.contains("num_sub_r <= 5'(9'(9'($unsigned(m_ar_len)) + 16) >> 4)"),
        "num_sub must be ceil((ar_len+1)/16) = (ar_len+16)>>4:\n{sv}",
    );
    // 2. INCR-stepped sub-burst start address: base + (beats_done << size).
    assert!(
        flat.contains("logic [ADDR_W-1:0] stepped = done << size;")
            && flat.contains("return ADDR_W'(base + stepped);"),
        "sub-burst start address must step by (i*16)<<size:\n{sv}",
    );
    // 3. AXI3 ar_len is 4-bit and is sub_beats-1.
    assert!(
        flat.contains("output logic [3:0] s_ar_len"),
        "AXI3 s_ar_len must be 4-bit (max 16-beat burst):\n{sv}",
    );
    assert!(
        flat.contains("s_ar_len = 4'(sub_beats_5_9(_t0_loop_cnt_0, total_r) - 1)"),
        "AXI3 ar_len must be (sub_beats - 1) truncated to 4 bits:\n{sv}",
    );
    // min(16, remaining) clamp inside sub_beats().
    assert!(
        flat.contains("if (remaining > 16) begin return 9'd16;"),
        "sub_beats must clamp to min(16, remaining):\n{sv}",
    );
    // 4. merged RLAST only on the final sub-burst.
    assert!(
        flat.contains("m_r_last = _t0_loop_cnt_0 == num_sub_r - 1 && s_r_last"),
        "merged RLAST must fire only on the final sub-burst:\n{sv}",
    );
}

#[test]
fn test_nic400_axi3_to_axi4_limiter_field_mapping_sv() {
    // The AXI3→AXI4 limiter forwards (limits) AXI3 reads onto AXI4. It must:
    //   1. zero AXI4 QoS / REGION (AXI3 has neither);
    //   2. map AXI3 2-bit AxLOCK (EXCLUSIVE=01) to AXI4 1-bit lock (==1);
    //   3. take MAX_BURST as the programmable onward-AxLEN cap.
    let bus3 = include_str!("../examples/nic400/BusAxi3.arch");
    let bus4 = include_str!("../examples/nic400/BusAxi4.arch");
    let dut = include_str!("../examples/nic400/Nic400Axi3ToAxi4.arch");
    let sv = compile_to_sv(&format!("{bus3}\n{bus4}\n{dut}"));
    let flat: String = sv.split_whitespace().collect::<Vec<_>>().join(" ");

    // 1. AXI3 AxLEN is 4-bit on the master-facing port (target flips dir →
    //    input on this converter), AXI4 AxLEN is 8-bit on the slave port.
    assert!(
        flat.contains("input logic [3:0] m_ar_len"),
        "AXI3 master-facing ar_len must be 4-bit:\n{sv}",
    );
    assert!(
        flat.contains("output logic [7:0] s_ar_len"),
        "AXI4 slave-facing ar_len must be 8-bit:\n{sv}",
    );
    // 2. AXI4 QoS / REGION held at 0.
    assert!(
        flat.contains("s_ar_qos = 0") && flat.contains("s_ar_region = 0"),
        "AXI4 QoS/REGION must be tied to 0 (AXI3 has neither):\n{sv}",
    );
    // 3. AXI3 2-bit lock → AXI4 1-bit lock via (l == 1).
    assert!(
        flat.contains("return l == 1;"),
        "AXI3 2-bit AxLOCK must map EXCLUSIVE(01)→1, else 0:\n{sv}",
    );
    // 4. MAX_BURST is the programmable cap parameter, default 16.
    assert!(
        flat.contains("parameter int MAX_BURST = 16"),
        "MAX_BURST must be the GPV-programmable onward-AxLEN cap (default 16):\n{sv}",
    );
}

#[test]
fn test_graph_index_query_callers_impact_context() {
    let td = tempfile::tempdir().expect("tempdir");
    let src = td.path().join("GraphProbe.arch");
    std::fs::write(
        &src,
        r#"
/// Shared increment helper.
function inc(x: UInt<8>) -> UInt<8>
  return x +% 1;
end function inc

/// Uses inc so graph callers has an exact edge.
module UseInc
  port a: in UInt<8>;
  port y: out UInt<8>;
  comb
    y = inc(a);
  end comb
end module UseInc

/// Width docs must not break </script><script>bad()</script> graph HTML.
module WidthUser
  param WIDTH: const = 8;
  port x: in UInt<WIDTH>;
  port y: out UInt<WIDTH>;
  comb
    y = x;
  end comb
end module WidthUser
"#,
    )
    .expect("write source");
    let graph = td.path().join("graph");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let index = std::process::Command::new(arch_bin)
        .args(["graph", "index"])
        .arg(&src)
        .args(["--out"])
        .arg(&graph)
        .arg("--clean")
        .output()
        .expect("graph index");
    assert!(
        index.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&index.stderr)
    );
    assert!(graph.join("manifest.json").exists());
    for name in ["files.jsonl", "nodes.jsonl", "edges.jsonl"] {
        let raw = std::fs::read_to_string(graph.join(name)).expect(name);
        for line in raw.lines().filter(|line| !line.trim().is_empty()) {
            serde_json::from_str::<serde_json::Value>(line).expect(line);
        }
    }
    let edges = std::fs::read_to_string(graph.join("edges.jsonl")).expect("edges");
    assert!(
        !edges.contains("symbol:<unresolved>:WIDTH"),
        "width params used inside types should resolve to local param nodes:\n{edges}"
    );

    let query = std::process::Command::new(arch_bin)
        .args(["graph", "query", "UseInc", "--index"])
        .arg(&graph)
        .output()
        .expect("graph query");
    assert!(
        query.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&query.stderr)
    );
    let query_stdout = String::from_utf8_lossy(&query.stdout);
    assert!(
        query_stdout.contains("module  UseInc"),
        "stdout:\n{query_stdout}"
    );

    let callers = std::process::Command::new(arch_bin)
        .args(["graph", "callers", "inc", "--index"])
        .arg(&graph)
        .output()
        .expect("graph callers");
    assert!(
        callers.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&callers.stderr)
    );
    let callers_stdout = String::from_utf8_lossy(&callers.stdout);
    assert!(
        callers_stdout.contains("UseInc -> inc"),
        "stdout:\n{callers_stdout}"
    );
    assert!(
        callers_stdout.contains(":12"),
        "callers should report call-site line, stdout:\n{callers_stdout}"
    );

    let impact = std::process::Command::new(arch_bin)
        .args(["graph", "impact", "UseInc", "--depth", "1", "--index"])
        .arg(&graph)
        .output()
        .expect("graph impact");
    assert!(
        impact.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&impact.stderr)
    );
    let impact_stdout = String::from_utf8_lossy(&impact.stdout);
    assert!(
        impact_stdout.contains("via=calls"),
        "stdout:\n{impact_stdout}"
    );

    let context = std::process::Command::new(arch_bin)
        .args(["graph", "context", "increment helper", "--index"])
        .arg(&graph)
        .output()
        .expect("graph context");
    assert!(
        context.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context_stdout = String::from_utf8_lossy(&context.stdout);
    assert!(context_stdout.contains("inc"), "stdout:\n{context_stdout}");

    let html_out = td.path().join("graph.html");
    let html = std::process::Command::new(arch_bin)
        .args(["graph", "html", "--index"])
        .arg(&graph)
        .args(["--out"])
        .arg(&html_out)
        .args(["--title", "UseInc graph"])
        .output()
        .expect("graph html");
    assert!(
        html.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&html.stderr)
    );
    let html = std::fs::read_to_string(&html_out).expect("read graph html");
    assert!(
        html.contains("arch-graph-viewer"),
        "expected graph viewer shell:\n{html}"
    );
    assert!(
        html.contains("UseInc graph") && html.contains("UseInc"),
        "expected title and graph node in HTML:\n{html}"
    );
    assert!(
        html.contains("data-node-id")
            && html.contains("Neighborhood")
            && html.contains("Outgoing")
            && html.contains("Incoming"),
        "expected clickable node/edge panes:\n{html}"
    );
    assert!(
        !html.contains("</script><script>bad()</script>"),
        "embedded graph data must not be able to close the viewer script:\n{html}"
    );
    assert!(
        html.contains("\\u003c/script\\u003e"),
        "expected script-breaking doc text to be escaped in embedded JSON:\n{html}"
    );
}

#[test]
fn test_graph_tlm_method_callers_resolve_through_bus_port_type() {
    let td = tempfile::tempdir().expect("tempdir");
    let graph = td.path().join("graph");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let src =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/axi_dma_tlm/TlmOneToOne.arch");

    let index = std::process::Command::new(arch_bin)
        .args(["graph", "index"])
        .arg(&src)
        .args(["--root"])
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["--out"])
        .arg(&graph)
        .arg("--clean")
        .output()
        .expect("graph index tlm");
    assert!(
        index.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&index.stderr)
    );

    let callers = std::process::Command::new(arch_bin)
        .args(["graph", "callers", "Mem.read", "--index"])
        .arg(&graph)
        .output()
        .expect("graph callers tlm");
    assert!(
        callers.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&callers.stderr)
    );
    let callers_stdout = String::from_utf8_lossy(&callers.stdout);
    assert!(
        callers_stdout.contains("Initiator -> read"),
        "qualified bus method callers should include m.read call sites:\n{callers_stdout}"
    );
}

#[test]
fn test_graph_directory_index_is_best_effort_but_explicit_file_is_strict() {
    let td = tempfile::tempdir().expect("tempdir");
    let good = td.path().join("Good.arch");
    let bad = td.path().join("Bad.arch");
    std::fs::write(
        &good,
        r#"
module Good
  port a: in Bool;
  port y: out Bool;
  comb
    y = a;
  end comb
end module Good
"#,
    )
    .expect("write good");
    std::fs::write(
        &bad,
        r#"
module Bad
  port y: out Bool;
  comb
    y = @;
  end comb
end module Bad
"#,
    )
    .expect("write bad");
    let graph = td.path().join("graph");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let dir_index = std::process::Command::new(arch_bin)
        .args(["graph", "index"])
        .arg(td.path())
        .args(["--out"])
        .arg(&graph)
        .arg("--clean")
        .output()
        .expect("directory graph index");
    assert!(
        dir_index.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&dir_index.stderr)
    );
    assert!(
        String::from_utf8_lossy(&dir_index.stderr).contains("skipped graph root"),
        "stderr:\n{}",
        String::from_utf8_lossy(&dir_index.stderr)
    );

    let explicit_bad = std::process::Command::new(arch_bin)
        .args(["graph", "index"])
        .arg(&bad)
        .args(["--out"])
        .arg(td.path().join("bad_graph"))
        .arg("--clean")
        .output()
        .expect("explicit bad graph index");
    assert!(
        !explicit_bad.status.success(),
        "explicit bad file should fail"
    );
}

#[test]
fn test_graph_index_paths_are_stable_from_outside_indexed_directory() {
    let td = tempfile::tempdir().expect("tempdir");
    let project = td.path().join("proj");
    std::fs::create_dir(&project).expect("create project");
    let src = project.join("Demo.arch");
    std::fs::write(
        &src,
        r#"
module Demo
  port a: in Bool;
  port y: out Bool;
  comb
    y = a;
  end comb
end module Demo
"#,
    )
    .expect("write source");

    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let inside_graph = td.path().join("inside_graph");
    let outside_graph = td.path().join("outside_graph");

    let inside = std::process::Command::new(arch_bin)
        .current_dir(&project)
        .args(["graph", "index", "Demo.arch", "--out"])
        .arg(&inside_graph)
        .arg("--clean")
        .output()
        .expect("inside graph index");
    assert!(
        inside.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&inside.stderr)
    );

    let outside = std::process::Command::new(arch_bin)
        .current_dir(td.path())
        .args(["graph", "index"])
        .arg(&src)
        .args(["--out"])
        .arg(&outside_graph)
        .arg("--clean")
        .output()
        .expect("outside graph index");
    assert!(
        outside.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&outside.stderr)
    );

    for name in ["manifest.json", "files.jsonl", "nodes.jsonl", "edges.jsonl"] {
        let inside_raw = std::fs::read_to_string(inside_graph.join(name)).expect(name);
        let outside_raw = std::fs::read_to_string(outside_graph.join(name)).expect(name);
        assert_eq!(
            inside_raw, outside_raw,
            "{name} should not depend on caller cwd"
        );
    }

    let nodes = std::fs::read_to_string(outside_graph.join("nodes.jsonl")).expect("nodes");
    assert!(nodes.contains("\"file\":\"Demo.arch\""), "nodes:\n{nodes}");
    assert!(
        !nodes.contains("proj/Demo.arch"),
        "paths should be index-root relative:\n{nodes}"
    );
}

// The original #549 trio covers value-taint (direct + via-let) and the
// register-read allow case. These three extend coverage to two distinct code
// paths the trio doesn't touch: (1) the cross-stage register read (the key
// "no false positive" guarantee — `Stage.reg` surfaces as the stage ident, so
// it must not taint), and (2) the if-guard / mux-select taint threaded by
// `collect_pipeline_comb_drivers` (an input used only as a mux SELECT still
// drives the output combinationally, so it must be rejected — and a register
// used the same way must NOT be).

#[test]
fn test_pipeline_allows_cross_stage_register_read() {
    // `o = Fetch.captured` reads a register in ANOTHER stage. `Fetch.captured`
    // surfaces as the stage ident `Fetch` (never an input/tainted name), so
    // this must be accepted — the canonical pipeline data-flow pattern.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline XStagePipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port a: in UInt<8>;
  port o: out UInt<8>;
  stage Fetch
    reg captured: UInt<8> reset rst => 0;
    seq on clk rising
      captured <= a;
    end seq
  end stage Fetch
  stage WB
    reg r2: UInt<8> reset rst => 0;
    seq on clk rising
      r2 <= Fetch.captured;
    end seq
    comb
      o = Fetch.captured;
    end comb
  end stage WB
end pipeline XStagePipe
"#;
    assert!(
        pipeline_checks_ok(source),
        "a pipeline output driven from a cross-stage register read must be accepted"
    );
}

#[test]
fn test_pipeline_rejects_input_used_as_comb_mux_select() {
    // `o = r0; if sel: o = r1;` — `sel` is an INPUT used only as a mux select,
    // yet it drives `o` combinationally (the output flips sub-cycle with the
    // input). Must be rejected: the if-guard idents are threaded into the
    // driver's read set, so `sel` taints `o`.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline GuardSelPipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port sel: in Bool;
  port o: out UInt<8>;
  stage S
    reg r0: UInt<8> reset rst => 0;
    reg r1: UInt<8> reset rst => 1;
    seq on clk rising
      r0 <= r0;
      r1 <= r1;
    end seq
    comb
      o = r0;
      if sel
        o = r1;
      end if
    end comb
  end stage S
end pipeline GuardSelPipe
"#;
    assert!(
        !pipeline_checks_ok(source),
        "an input used as a combinational mux select on an output must be rejected"
    );
}

#[test]
fn test_pipeline_allows_register_used_as_comb_mux_select() {
    // Same shape as the reject case, but the mux select is a REGISTER
    // (`sel_r`), so there is no comb path from any input to `o`. Reading a
    // register as a guard must NOT taint — guards against an over-broad rule
    // that would flag every conditional output.
    let source = r#"
domain D
  freq_mhz: 100
end domain D
pipeline GuardRegPipe
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port sel: in Bool;
  port o: out UInt<8>;
  stage S
    reg sel_r: Bool reset rst => false;
    reg r0: UInt<8> reset rst => 0;
    reg r1: UInt<8> reset rst => 1;
    seq on clk rising
      sel_r <= sel;
      r0 <= r0;
      r1 <= r1;
    end seq
    comb
      o = r0;
      if sel_r
        o = r1;
      end if
    end comb
  end stage S
end pipeline GuardRegPipe
"#;
    assert!(
        pipeline_checks_ok(source),
        "a register used as a combinational mux select on an output must be accepted"
    );
}

// ────────────────────────────────────────────────────────────────────
// NIC-400 GPV (PR #551) + C-channel clock-gate (PR #555) — coverage
// ────────────────────────────────────────────────────────────────────
//
// Both designs landed as example .arch files with only harc-side
// `_test.harc` coverage — nothing in arch-com's own integration suite
// exercised them, so an arch-com codegen regression could silently break
// either design (the harc tests run through a different repo's harness).
// These tests close that gap: a behavioral cross-check of the C-channel
// handshake on both the arch-sim and Verilator backends, and a
// build + lint-clean + structure pin for the GPV AXI4-target regfile.

/// C-channel low-power clock-gating handshake — behavioral check on the
/// arch-sim backend. Mirrors examples/nic400/Nic400CChannelClockGate_test.harc
/// (gate / wake / busy-hold / regate / rerun) so the FSM's same-cycle
/// combinational outputs (csysack / cactive / clk_en) are verified through
/// arch-com's own sim, not only the harc harness.
#[test]
fn test_nic400_cchannel_clockgate_arch_sim_behavior() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("examples/nic400/Nic400CChannelClockGate.arch")
        .arg("examples/nic400/BusCChannel.arch")
        .arg("--tb")
        .arg("examples/nic400/tb_nic400_cchannel_clockgate.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for Nic400CChannelClockGate");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for Nic400CChannelClockGate\nstdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("PASS cchannel_clockgate"),
        "expected PASS marker in arch-sim stdout:\n{stdout}"
    );
}

/// Verilator cross-check for the same C-channel handshake fixture — runs the
/// identical TB through the SV backend so any divergence between the arch-sim
/// scheduler and the generated SV trips both tests (or neither).
#[test]
fn test_nic400_cchannel_clockgate_verilator_behavior() {
    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping Nic400CChannelClockGate Verilator behavior: verilator not found");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("cc.sv");
    let obj_dir = td.path().join("obj_dir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("examples/nic400/Nic400CChannelClockGate.arch")
        .arg("examples/nic400/BusCChannel.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build Nic400CChannelClockGate SV");
    assert!(
        build.status.success(),
        "arch build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    // A 2-state FSM fills its 1-bit encoding completely, so PR #554's fix
    // must have suppressed the (now vacuous) legal-state assertion. Pin it:
    // if a future change re-introduces a power-of-2 legal-state assertion,
    // this catches it before Verilator even runs.
    let sv = std::fs::read_to_string(&sv_out).expect("read cc.sv");
    assert!(
        !sv.contains("legal_state"),
        "a 2-state FSM must NOT emit a vacuous legal-state assertion (PR #554):\n{sv}"
    );

    let verilate = std::process::Command::new("verilator")
        .arg("--cc")
        .arg("--exe")
        .arg("--build")
        .arg("--sv")
        .arg("--assert")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("Nic400CChannelClockGate")
        .arg("-Mdir")
        .arg(&obj_dir)
        .arg(&sv_out)
        .arg("examples/nic400/tb_nic400_cchannel_clockgate.cpp")
        .output()
        .expect("verilate Nic400CChannelClockGate");
    assert!(
        verilate.status.success(),
        "Verilator build should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verilate.stdout),
        String::from_utf8_lossy(&verilate.stderr)
    );

    let run = std::process::Command::new(obj_dir.join("VNic400CChannelClockGate"))
        .output()
        .expect("run Verilator Nic400CChannelClockGate");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success() && stdout.contains("PASS cchannel_clockgate"),
        "expected PASS marker in Verilator stdout:\n{stdout}"
    );
}

/// GPV AXI4-target config register file (PR #551) — build + Verilator
/// lint-clean + module-structure pin. The GPV is a multi-file design
/// (Nic400Gpv + Nic400GpvRegs + BusAxi4); behavioral correctness is
/// covered by the harc-side `_test.harc`, so this guards specifically
/// against an arch-com codegen / elaboration regression that would make
/// the design fail to build or lint clean.
#[test]
fn test_nic400_gpv_builds_and_verilator_lint_clean() {
    let td = tempfile::tempdir().expect("tempdir");
    let sv_out = td.path().join("gpv.sv");
    let arch_bin = env!("CARGO_BIN_EXE_arch");

    let build = std::process::Command::new(arch_bin)
        .arg("build")
        .arg("examples/nic400/Nic400Gpv.arch")
        .arg("examples/nic400/Nic400GpvRegs.arch")
        .arg("examples/nic400/BusAxi4.arch")
        .arg("-o")
        .arg(&sv_out)
        .output()
        .expect("build Nic400Gpv SV");
    assert!(
        build.status.success(),
        "arch build should pass for Nic400Gpv\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let sv = std::fs::read_to_string(&sv_out).expect("read gpv.sv");
    assert!(
        sv.contains("module Nic400Gpv") && sv.contains("module Nic400GpvRegs"),
        "both GPV modules must be emitted:\n{sv}"
    );
    // The bus flattens to individual AXI ports — pin a representative one so a
    // bus-flattening regression on a `target` AXI4 port trips here.
    assert!(
        sv.contains("s_ar_addr") && sv.contains("s_r_resp"),
        "GPV `target BusAxi4` ports must flatten (s_ar_addr / s_r_resp):\n{sv}"
    );

    if std::process::Command::new("verilator")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping Nic400Gpv Verilator lint: verilator not found");
        return;
    }
    let lint = std::process::Command::new("verilator")
        .arg("--lint-only")
        .arg("--sv")
        .arg("-Wno-DECLFILENAME")
        .arg("--top-module")
        .arg("Nic400Gpv")
        .arg(&sv_out)
        .output()
        .expect("verilator lint Nic400Gpv");
    assert!(
        lint.status.success(),
        "Nic400Gpv must lint clean under Verilator\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&lint.stdout),
        String::from_utf8_lossy(&lint.stderr)
    );
}

#[test]
fn test_coverage_merge_sums_verilator_dat_records() {
    let td = tempfile::tempdir().expect("tempdir");
    let seed1 = td.path().join("seed1.dat");
    let seed2 = td.path().join("seed2.dat");
    let merged = td.path().join("merged.dat");

    std::fs::write(
        &seed1,
        "# SystemC::Coverage-3\n\
C '\u{1}file\u{2}Foo.arch\u{1}line\u{2}10\u{1}page\u{2}v_branch\u{1}comment\u{2}if ' 2\n\
C '\u{1}file\u{2}Foo.arch\u{1}line\u{2}11\u{1}page\u{2}v_line\u{1}comment\u{2}comb comb' 0\n",
    )
    .expect("write seed1 coverage");
    std::fs::write(
        &seed2,
        "# SystemC::Coverage-3\n\
C '\u{1}file\u{2}Foo.arch\u{1}line\u{2}10\u{1}page\u{2}v_branch\u{1}comment\u{2}if ' 5\n\
C '\u{1}file\u{2}Foo.arch\u{1}line\u{2}12\u{1}page\u{2}v_toggle\u{1}comment\u{2}toggle r' 7\n",
    )
    .expect("write seed2 coverage");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("coverage")
        .arg("merge")
        .arg("--out")
        .arg(&merged)
        .arg(&seed1)
        .arg(&seed2)
        .output()
        .expect("run arch coverage merge");
    assert!(
        out.status.success(),
        "coverage merge should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let merged_text = std::fs::read_to_string(&merged).expect("read merged coverage");
    assert!(
        merged_text.contains(
            "C '\u{1}file\u{2}Foo.arch\u{1}line\u{2}10\u{1}page\u{2}v_branch\u{1}comment\u{2}if ' 7\n"
        ),
        "matching records should be summed:\n{merged_text}"
    );
    assert!(
        merged_text.contains(
            "C '\u{1}file\u{2}Foo.arch\u{1}line\u{2}11\u{1}page\u{2}v_line\u{1}comment\u{2}comb comb' 0\n"
        ),
        "seed1-only records should be preserved:\n{merged_text}"
    );
    assert!(
        merged_text.contains(
            "C '\u{1}file\u{2}Foo.arch\u{1}line\u{2}12\u{1}page\u{2}v_toggle\u{1}comment\u{2}toggle r' 7\n"
        ),
        "seed2-only records should be preserved:\n{merged_text}"
    );
}

#[test]
fn test_native_sim_rejects_multiply_wider_than_128_bits() {
    // Native sim computes products in a 128-bit intermediate (`_arch_u128`).
    // A `*` whose ARCH-widened result (W(lhs)+W(rhs)) exceeds 128 bits cannot
    // be represented and would be silently truncated — reject it loudly with
    // an actionable message instead. `arch build`/`arch formal` are unaffected.
    let td = tempfile::tempdir().expect("tempdir");
    let src_path = td.path().join("WideMul140.arch");
    std::fs::write(
        &src_path,
        r#"
module WideMul140
  port a: in UInt<70>;
  port b: in UInt<70>;
  port p: out UInt<140>;
  comb
    p = a * b;
  end comb
end module WideMul140
"#,
    )
    .expect("write fixture");

    let tb_path = td.path().join("tb.cpp");
    std::fs::write(
        &tb_path,
        "#include \"VWideMul140.h\"\nint main(){ VWideMul140 d; d.eval(); return 0; }\n",
    )
    .expect("write tb");

    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg(&src_path)
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim");

    assert!(
        !out.status.success(),
        "native sim must reject a >128-bit multiply result, but it succeeded"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("result needs more than 128 bits") && stderr.contains("140 bits"),
        "expected a loud >128-bit multiply diagnostic naming the width; got stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("github.com/arch-hdl-lang/arch-com/issues"),
        "diagnostic should point users at filing an enhancement request; got stderr:\n{stderr}"
    );
}

#[test]
fn test_arch_sim_param_override_reaches_parametric_slices() {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join("ParamSliceRegress.arch");
    let tb_path = td.path().join("ParamSliceRegress_tb.cpp");
    let outdir = td.path().join("sim");

    std::fs::write(
        &arch_path,
        r#"
domain D
  freq_mhz: 100
end domain D

module ParamSliceRegress
  param CounterWidth: const = 32;

  port clk: in Clock<D>;
  port rst_n: in Reset<Async, Low>;
  port inc: in Bool;
  port low_we: in Bool;
  port high_we: in Bool;
  port data: in UInt<32>;
  port val: out UInt<64>;

  reg counter_q: UInt<64> reset rst_n => 0;

  default seq on clk rising;

  seq
    if high_we
      counter_q[CounterWidth-1:0] <= {data, counter_q[31:0]}[CounterWidth-1:0];
    elsif low_we
      counter_q[CounterWidth-1:0] <= {counter_q[63:32], data}[CounterWidth-1:0];
    elsif inc
      counter_q[CounterWidth-1:0] <= (counter_q[CounterWidth-1:0] + 1).trunc<CounterWidth>();
    end if
  end seq

  let val = counter_q[CounterWidth-1:0].resize<64>();
end module ParamSliceRegress
"#,
    )
    .expect("write arch fixture");

    std::fs::write(
        &tb_path,
        r#"
#include "VParamSliceRegress.h"
#include <cstdio>

static VParamSliceRegress dut;

static void tick() {
  dut.clk = 0; dut.eval();
  dut.clk = 1; dut.eval();
  dut.clk = 0; dut.eval();
}

static bool expect_val(unsigned long long want, const char* label) {
  dut.eval();
  unsigned long long got = (unsigned long long)dut.val;
  if (got != want) {
    std::printf("FAIL %s got=%llu want=%llu\n", label, got, want);
    return false;
  }
  return true;
}

int main() {
  bool ok = true;
  dut.rst_n = 0;
  dut.inc = 0;
  dut.low_we = 0;
  dut.high_we = 0;
  dut.data = 0;
  tick();
  dut.rst_n = 1;
  tick();

  dut.low_we = 1;
  dut.data = 1;
  tick();
  dut.low_we = 0;
  ok &= expect_val(1, "write low bit");

  dut.inc = 1;
  tick();
  dut.inc = 0;
  ok &= expect_val(0, "CounterWidth=1 increment wraps");

  dut.high_we = 1;
  dut.data = 0xffffffffu;
  tick();
  dut.high_we = 0;
  ok &= expect_val(0, "CounterWidth=1 high write stays outside active width");

  std::printf(ok ? "PASS param_slice_override\n" : "FAIL param_slice_override\n");
  return ok ? 0 : 1;
}
"#,
    )
    .expect("write tb fixture");

    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg(&arch_path)
        .arg("--param")
        .arg("CounterWidth=1")
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(&outdir)
        .output()
        .expect("run arch sim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("PASS param_slice_override"),
        "arch sim --param must affect parametric slice/trunc codegen\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn test_comb_graph_cycles_with_parent_intermediates_need_three_settle_passes() {
    // Regression: a direct cyclic instance graph normally needs two settle
    // passes, but if parent comb wires also feed an instance input, one pass
    // is consumed refreshing those parent intermediates. ibex_ex_block has
    // this shape: parent bridge wires feed multdiv, multdiv feeds ALU, and
    // ALU feeds multdiv. The native simulator therefore needs settle_depth=3.
    let source = r#"
        module A
          port i: in UInt<1>;
          port o: out UInt<1>;
          comb
            o = i;
          end comb
        end module A

        module B
          port i: in UInt<1>;
          port bridge: in UInt<1>;
          port o: out UInt<1>;
          comb
            o = i ^ bridge;
          end comb
        end module B

        module Top
          port seed: in UInt<1>;
          port q: out UInt<1>;
          wire wa: UInt<1>;
          wire wb: UInt<1>;
          wire bridged: UInt<1>;

          inst a: A
            i <- wb;
            o -> wa;
          end inst a

          inst b: B
            i <- wa;
            bridge <- bridged;
            o -> wb;
          end inst b

          comb
            bridged = seed;
            q = wa;
          end comb
        end module Top
    "#;
    let tokens = lexer::tokenize(source).expect("lexer error");
    let mut parser = Parser::new(tokens, source);
    let parsed_ast = parser.parse_source_file().expect("parse error");
    let ast = elaborate::elaborate(parsed_ast).expect("elaborate error");
    let symbols = resolve::resolve(&ast).expect("resolve error");
    let checker = TypeChecker::new(&symbols, &ast);
    let _ = checker.check().expect("type check error");
    let top = ast
        .items
        .iter()
        .find_map(|item| match item {
            arch::ast::Item::Module(m) if m.name.name == "Top" => Some(m),
            _ => None,
        })
        .expect("Top module");
    let analysis =
        arch::comb_graph::analyze_module(top, &symbols, &ast).expect("comb graph analysis");
    assert_eq!(analysis.settle_depth, 3);
}

#[test]
fn test_sim_guard_check_uses_internal_names_for_guarded_reset_none_regs() {
    // A guarded reset-none reg gets a native-sim vinit check. Internal regs in
    // generated C++ are stored with leading underscores, so the guard check
    // must render `_valid_q`, not bare `valid_q`.
    let source = r#"
domain D
  freq_mhz: 100
end domain D

module M
  port clk: in Clock<D>;
  port rst: in Reset<Async, Low>;
  port d:   in UInt<8>;
  port v_in: in Bool;
  port q:   out UInt<8>;

  reg valid_q: Bool reset rst => false;
  reg data_q: UInt<8> guard valid_q reset none;

  seq on clk rising
    valid_q <= v_in;
    data_q <= d;
  end seq

  let q = data_q;
end module M
"#;
    let cpp = compile_to_sim_h(source, false);
    assert!(
        cpp.contains("bool _data_q_vinit = false;"),
        "guarded reset-none native sim check should declare a shadow valid bit:\n{cpp}"
    );
    assert!(
        cpp.contains("_data_q_vinit = true;"),
        "seq writes to guarded reset-none regs should mark the shadow valid bit:\n{cpp}"
    );
    assert!(
        cpp.contains("if (_valid_q && !_data_q_vinit)"),
        "guarded reset-none native sim check should use internal storage names:\n{cpp}"
    );
    assert!(
        !cpp.contains("if (valid_q && !_data_q_vinit)"),
        "guarded reset-none native sim check must not emit bare internal reg names:\n{cpp}"
    );
}

#[test]
fn test_native_sim_vec_inst_output_packs_scalar_parent_port() {
    // Regression from arch-ibex IbexIfStage: a child Vec<UInt<1>, N> output
    // connected to a packed parent UInt<N> output must pack lanes into bits.
    // The broken native sim path treated the packed parent as C-array storage
    // and emitted `packed_out[i] = ...`, which failed C++ compilation.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_vec_inst_output_packed_port/Probe.arch")
        .arg("--tb")
        .arg("tests/native_vec_inst_output_packed_port/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native Vec inst output packed port probe");
    assert!(
        out.status.success(),
        "native Vec inst output packed port sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native Vec inst output packed port"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_native_sim_arbiter_indexed_handshake_packs_request_array() {
    // Regression from arch-ibex IbexIcache/FbAgeArb: arbiter indexed
    // handshake connections such as `request[0].valid <- req0` flatten to
    // `request0_valid` in the AST, but the native arbiter model stores the
    // request array as packed `request_valid/request_ready` fields.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_arbiter_indexed_handshake/Probe.arch")
        .arg("--tb")
        .arg("tests/native_arbiter_indexed_handshake/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native arbiter indexed handshake probe");
    assert!(
        out.status.success(),
        "native arbiter indexed handshake sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native arbiter indexed handshake"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_native_sim_vec_element_bit_slice_assignment_updates_element() {
    // Regression from arch-ibex IbexIcache: assigning to a bit slice of a Vec
    // element, e.g. `fill_data_q[0][31:0] <= instr`, must lower to a
    // mask-and-OR update of the element. Emitting the read-side slice
    // expression on the LHS is invalid C++.
    let td = tempfile::tempdir().expect("tempdir");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_vec_element_slice_assign/Probe.arch")
        .arg("--tb")
        .arg("tests/native_vec_element_slice_assign/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .output()
        .expect("run arch sim for native Vec element slice assignment probe");
    assert!(
        out.status.success(),
        "native Vec element slice assignment sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native Vec element slice assign"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_native_sim_package_struct_fsm_ports_compile_with_coverage() {
    // Regression from arch-ibex IbexIdStage: package struct signals crossing a
    // parent module into an FSM child must include VStructs.h, default-construct
    // struct ports/regs, skip scalar-style struct traces, and avoid output-toggle
    // coverage casts of struct instance outputs.
    let td = tempfile::tempdir().expect("tempdir");
    let coverage_dat = td.path().join("coverage.dat");
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = std::process::Command::new(arch_bin)
        .arg("sim")
        .arg("tests/native_pkg_struct_fsm/Probe.arch")
        .arg("--tb")
        .arg("tests/native_pkg_struct_fsm/tb.cpp")
        .arg("--outdir")
        .arg(td.path())
        .arg("--coverage")
        .arg("--coverage-dat")
        .arg(&coverage_dat)
        .output()
        .expect("run arch sim for native package struct FSM coverage probe");
    assert!(
        out.status.success(),
        "native package struct FSM coverage sim should compile + run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PASS native package struct FSM coverage"),
        "expected PASS marker in stdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let coverage_text = std::fs::read_to_string(&coverage_dat).expect("read coverage.dat");
    assert!(
        coverage_text.contains("tests/native_pkg_struct_fsm/Probe.arch")
            && coverage_text.contains("state Idle")
            && coverage_text.contains("trans Idle -> Seen"),
        "coverage records should include the struct/FSM fixture state and transition hits:\n{coverage_text}"
    );
}

#[test]
fn test_arch_sim_coverage_dat_records_ternary_expression_arms() {
    let td = tempfile::tempdir().expect("tempdir");
    let arch = td.path().join("TernaryCov.arch");
    let tb = td.path().join("tb_ternary_cov.cpp");
    let coverage = td.path().join("coverage.dat");

    std::fs::write(
        &arch,
        "/// Ternary expression coverage smoke test.\n\
module TernaryCov\n\
  port a: in UInt<1>;\n\
  port b: in UInt<1>;\n\
  port y: out UInt<1>;\n\
\n\
  let y = a ? b : 0;\n\
end module TernaryCov\n",
    )
    .expect("write TernaryCov.arch");

    std::fs::write(
        &tb,
        "#include \"VTernaryCov.h\"\n\
#include <cstdio>\n\
\n\
int main() {\n\
  VTernaryCov dut;\n\
  dut.a = 0;\n\
  dut.b = 0;\n\
  dut.eval();\n\
  if (dut.y != 0) return 1;\n\
\n\
  dut.a = 1;\n\
  dut.b = 1;\n\
  dut.eval();\n\
  if (dut.y != 1) return 2;\n\
\n\
  std::puts(\"PASS ternary coverage\");\n\
  return 0;\n\
}\n",
    )
    .expect("write tb_ternary_cov.cpp");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("sim")
        .arg(&arch)
        .arg("--tb")
        .arg(&tb)
        .arg("--outdir")
        .arg(td.path())
        .arg("--coverage-dat")
        .arg(&coverage)
        .output()
        .expect("run arch sim with coverage");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "arch sim should pass for ternary coverage\nstdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("PASS ternary coverage"),
        "expected PASS marker in arch-sim stdout:\n{stdout}"
    );

    let coverage_text = std::fs::read_to_string(&coverage).expect("read coverage.dat");
    assert!(
        coverage_text.contains("\u{1}page\u{2}v_expr")
            && coverage_text.contains("\u{1}comment\u{2}expr-then")
            && coverage_text.contains("\u{1}comment\u{2}expr-else"),
        "ternary arms should be emitted as expression coverage records:\n{coverage_text}"
    );
    assert!(
        coverage_text
            .lines()
            .any(|line| line.contains("\u{1}comment\u{2}expr-then") && !line.ends_with(" 0"))
            && coverage_text
                .lines()
                .any(|line| line.contains("\u{1}comment\u{2}expr-else") && !line.ends_with(" 0")),
        "both ternary arms should be hit by the smoke bench:\n{coverage_text}"
    );
}

#[test]
fn test_match_wildcard_not_last_is_unreachable_error() {
    // `_` before a later arm: that arm is unreachable (priority semantics).
    let source = r#"
enum Color
  Red,
  Green,
  Blue,
end enum Color

module BadOrder
  port c: in Color;
  port o: out UInt<8>;
  comb
    match c
      Color::Red  => o = 1;
      _           => o = 0;
      Color::Blue => o = 3;
    end match
  end comb
end module BadOrder
"#;
    let errs = typecheck_source(source).expect_err("expected an unreachable-arm error");
    let msg = format!("{errs:?}");
    assert!(
        msg.contains("unreachable match arm"),
        "expected unreachable-wildcard-arm diagnostic, got: {msg}"
    );
}

#[test]
fn test_match_wildcard_last_is_accepted() {
    let source = r#"
enum Color
  Red,
  Green,
  Blue,
end enum Color

module GoodOrder
  port c: in Color;
  port o: out UInt<8>;
  comb
    match c
      Color::Red  => o = 1;
      Color::Blue => o = 3;
      _           => o = 0;
    end match
  end comb
end module GoodOrder
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "a match with `_` as the final arm must type-check"
    );
}

#[test]
fn test_match_duplicate_wildcard_is_error() {
    let source = r#"
module DupWild
  port c: in UInt<2>;
  port o: out UInt<8>;
  comb
    match c
      0 => o = 1;
      _ => o = 2;
      _ => o = 3;
    end match
  end comb
end module DupWild
"#;
    let errs = typecheck_source(source).expect_err("expected a duplicate-wildcard error");
    assert!(
        format!("{errs:?}").contains("duplicate wildcard arm"),
        "expected duplicate-wildcard diagnostic, got: {:?}",
        typecheck_source(source)
    );
}

#[test]
fn test_match_wildcard_not_last_errors_for_non_enum_too() {
    // The rule applies to every match, not only enum matches: this integer
    // match places `_` before a literal arm.
    let source = r#"
module IntBadOrder
  port c: in UInt<2>;
  port o: out UInt<8>;
  comb
    match c
      0 => o = 1;
      _ => o = 0;
      1 => o = 2;
    end match
  end comb
end module IntBadOrder
"#;
    let errs =
        typecheck_source(source).expect_err("expected unreachable-arm error for non-enum match");
    assert!(
        format!("{errs:?}").contains("unreachable match arm"),
        "non-enum match must also enforce wildcard-last"
    );
}

#[test]
fn test_match_wildcard_not_last_errors_in_expression_match() {
    // The wildcard-last rule must also fire for the *expression* match form
    // (`let x: T = match ... end match`), which lowers to `ExprKind::ExprMatch`
    // and is checked at a separate call site from statement matches. #634 wired
    // this path but shipped tests only for statement (`comb`) matches.
    let source = r#"
module ExprBadOrder
  port c: in UInt<2>;
  port o: out UInt<8>;
  let o_val: UInt<8> = match c
      0 => 10,
      _ => 0,
      1 => 20,
    end match;
  comb
    o = o_val;
  end comb
end module ExprBadOrder
"#;
    let errs = typecheck_source(source)
        .expect_err("expected unreachable-arm error for expression match with `_` not last");
    assert!(
        format!("{errs:?}").contains("unreachable match arm"),
        "expression match must also enforce wildcard-last"
    );
}

#[test]
fn test_match_wildcard_last_accepted_in_expression_match() {
    // Companion to the above: `_` as the final arm of an expression match
    // type-checks cleanly.
    let source = r#"
module ExprGoodOrder
  port c: in UInt<2>;
  port o: out UInt<8>;
  let o_val: UInt<8> = match c
      0 => 10,
      1 => 20,
      _ => 0,
    end match;
  comb
    o = o_val;
  end comb
end module ExprGoodOrder
"#;
    assert!(
        typecheck_source(source).is_ok(),
        "an expression match with `_` as the final arm must type-check"
    );
}
