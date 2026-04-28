# ARCH Compiler — Status & Roadmap

> Last updated: 2026-04-25
> Compiler version: 0.43.0
>
> **Recent (post-0.43.0, unversioned):**
> - **`arch sim --coverage`** — full 6-phase coverage rollout: (1) branch coverage, (2) block-execution coverage for branchless seq/comb, (3) FSM state + transition coverage, (4) toggle coverage for scalar regs, (4b) Vec reg toggle, (5) Verilator-compatible `coverage.dat` alongside text report, (6) construct port toggle (sub-instance interface ports counted from the consumer side via `_prev_<inst>_<port>` shadow + popcount XOR).
> - **`arch sim --thread-sim parallel --threads N`** — pre-lowering thread sim with multi-OS-thread parallel execution. Per-user-thread C++20 coroutine scheduler; cache-line-padded atomic spin-wait `Barrier`; deterministic for owned-output designs (10-run trace identity verified, TSan-clean). `--thread-sim both` runs fsm-lowered + native paths and diffs `--debug` traces for cross-check. Cycle batching API `dut.run_cycles(K)` cuts per-tick overhead — `named_thread` at N=2/batch hits 14.3 Mcyc/s (~2.2× the fsm path; ~15× Verilator at N=2 per-cycle).
> - **Sim perf tuning** — settle-loop input hoist (invariant port/reg copies hoisted out of the 2-iteration settle); default `-O2 -flto` for sim builds; `ARCH_OPT` env override; `ARCH_TSAN=1` adds `-fsanitize=thread`.
> - **RealBench runner** — per-module build via auto-discovery of `.archi` interfaces (stale wrapper modules no longer take down the batch); macOS-portable timeout via `perl alarm`; e203 suite at 9/40 PASS (baseline 15/40 — gap is per-module SV/sim issues, not infra).
>
> **2026-04-24:** `arch formal` gains hierarchical v1 — one level of sub-inst nesting with bottom-up flattening (let / comb / reg / seq sub-modules; Assign + IfElse stmts). Multi-inst of the same sub-module works via prefix mangling. See CLI row for full scope + what's deferred.
>
> Previous: 0.43.0 (**`handshake` sub-construct inside `bus`**: compile-time sum-type primitive with 6 variants, `send`/`receive` role keywords, auto-emitted protocol SVA, runtime payload-uninit guard + compile-time unguarded-read lint. `--inputs-start-uninit` covers bus-flattened input signals.)
> Previous: 0.42.0 (always-on local learning capture: error→fix pairs recorded to `~/.arch/learn/` for `arch advise` retrieval. Opt-out `ARCH_NO_LEARN=1`; 100 MB default cap via `ARCH_LEARN_MAX_MB`.)
> Previous: 0.41.1 (`arch formal` direct SMT-LIB2 BMC; `<=` parses as less-than-or-equal in expressions; packed-struct bit layout = SV convention, declaration-first = MSB)

---

## Implemented

### CLI

| Command | Status |
|---------|--------|
| `arch check <file.arch>` | ✅ Parse + type-check; exits 0 on success; **operator precedence ambiguity check** rejects four common foot-guns where the parse is surprising: (1) bitwise `& \| ^` mixed with comparison `== != < > <= >=` e.g. `a & mask == 0`; (2) bitwise mixed with logical `and or` e.g. `a and b & c`; (3) shift `<< >>` mixed with arithmetic `+ - *` e.g. `1 << bit + 1`; (4) ternary `? :` with a naked binary branch e.g. `en ? a : b + 1`. Users must explicitly parenthesize one side to disambiguate. Check runs pre-elaboration so thread-lowering-generated reductions don't trigger false positives. |
| `arch build <file.arch> [-o out.sv]` | ✅ Emits deterministic SystemVerilog; **SV codegen fixes**: (1) signed cast emits `$signed(x)` instead of `logic signed [N-1:0]'(x)` for Verilator compatibility; (2) `>>` on SInt operands emits `>>>` (arithmetic shift right); (3) `.zext<N>()` emits `N'($unsigned(x))` to prevent context-dependent width expansion |
| `arch build a.arch b.arch` | ✅ Multi-file: concatenates + cross-resolves; one `.sv` per input (or single combined file with `-o`) |
| `arch sim <file.arch> --tb <tb.cpp>` | ✅ Generates Verilator-compatible C++ models (`VName.h` + `VName.cpp` + `verilated.h`), compiles with `g++`, and runs; supports `module`, `counter`, `fsm`, `linklist`, `ram`, `fifo`, `arbiter`, `regfile`, `cam`, `pipeline`, `thread` |
| `arch sim ... --check-uninit` | ✅ Detects reads of uninitialized `reset none` registers and `guard`-annotated regs; shadow valid bits propagate through `pipe_reg` chains; warn-once per signal to stderr. |
| `arch sim ... --inputs-start-uninit` | ✅ Treats every scalar primary input port as uninitialized until the TB calls the generated `dut.set_<port>(v)` setter. Warns once per undriven port when read in comb/let/seq/latch. Bus, Vec, Clock, and Reset ports are excluded in v1. Implies `--check-uninit`. |
| `arch sim ... --check-uninit-ram` | ✅ Per-cell valid bitmap (1 bit per RAM word). Read of a cell never written (and not covered by `init:`) warns once per RAM with the first offending address. ROMs exempt (their `init:` pre-marks all cells). Implies `--check-uninit`. |
| `arch sim` / `arch build` runtime bounds checks | ✅ **Always on**. Out-of-range `Vec<T,N>` indexing, bit-select `val[i]`, and variable part-select `val[start +:W]` / `val[start -:W]` → `ARCH-ERROR: <loc>: index N out of bounds [0..L)` + `abort()` (C++ sim). `arch build` also auto-emits concurrent SVA `_auto_bound_<kind>_<n>: assert property` for seq/latch contexts (comb/let deferred), wrapped in `synopsys translate_off/on`. Verified with Verilator 5.034 `--assert` and EBMC 5.11 (PROVED for structurally-safe indices like `UInt<2>` into `Vec<_,4>`). |
| `arch sim` / `arch build` divide-by-zero checks | ✅ Three layers. (1) `arch check`: compile-time error on `A / 0` / `A % 0` in any constant expression (param defaults, const let initializers) — includes transitively folded cases. (2) `arch sim`: `_ARCH_DCHK(divisor, loc)` wraps runtime `/` and `%`; hard abort on zero. (3) `arch build`: auto-emits `_auto_div0_<op>_<n>: assert property ((divisor) != 0)` for non-const divisors in seq/latch. Const divisors (literals, const-param refs, folded arithmetic) are elided from all three layers. EBMC 5.11: PROVED when divisor is structurally non-zero (`den \| 1`). |
| `arch build` / `arch sim` / `arch formal` `--auto-thread-asserts` | ✅ **v0.44.0+**. Off-by-default flag that emits SVA spec-contract properties at thread-lowering time, anchored to the lowered state register and per-thread counter. Three property classes: `_auto_thread_t{i}_wait_until_s{si}` (`(rst_inactive && state==si && cond) \|=> state==next`), `_auto_thread_t{i}_wait_stay_s{si}` + `_..._wait_done_s{si}` (counter-driven `wait N cycle` stay/advance), `_auto_thread_t{i}_branch_s{si}_b{bi}` (per-(cond,target) `fork/join` branches). Reset polarity inverted correctly (active-low → bare `rst` guard, active-high → `!rst`). Wrapped in `synopsys translate_off/on`. Skipped for terminal `thread once` last states (vacuous) and threads with `default_when`. **Why thread-level vs FSM-level**: the source-level structure (`wait until`, `wait N cycle`, fork/join) is gone after lowering — no downstream pass can recover it. Reachability/state-range cover are FSM-construct work, not in scope here. Verified with Verilator 5.034 `--assert` end-to-end on `tests/thread/wait_cycles.arch` (DelayPulse): 24-cycle golden run silent; mutating `wait_until` consequent fires `$fatal(1, "ASSERTION FAILED: ...")` mid-sim. |
| `arch sim --pybind --test test.py` | ✅ Cocotb-compatible Python testbench flow. Generates a pybind11 wrapper, compiles as a Python module, runs the Python test under an asyncio-driven tick scheduler (`python/arch_cocotb/`). Drop-in `cocotb_shim/cocotb/` re-exports so `import cocotb` works unchanged. Supports `@cocotb.test`, `RisingEdge`/`FallingEdge`/`Timer`/`ClockCycles`/`Clock`, `start_soon`, `start`, `cocotb.utils.get_sim_time`, and `dut.sig.value` access. See `doc/arch_sim_cocotb.md` for the full API and deltas from real cocotb (2-state logic, tick-sampled edges, immediate writes — no NBA region). |
| `arch sim --coverage` | ✅ Verilator-style instrument-then-replay. Six categories: (1) **branch** — per-arm counters on `if`/`elsif`/`else` in seq + comb; (2) **block** — entry counter for branchless seq/comb (catches dead clocks/blocks); (3) **FSM** state-entry + transition-arc counters per `fsm`; (4) **toggle** — scalar reg 0→1 / 1→0 (4b extends to `Vec<T,N>` element-wise); (5) Verilator-format `coverage.dat` alongside text report — `verilator_coverage --annotate` works against the generated SV; (6) **construct port toggle** — sub-instance interface ports counted from the consumer side (`_prev_<inst>_<port>` shadow + popcount XOR at end of `eval()`), surfaces dead lanes / tied-off interfaces at black-box construct boundaries (fifo/arbiter/ram/cam/linklist/pipeline/fsm). Counters are per-class `uint64_t`; `final()` dumps to `coverage.txt` + `coverage.dat`. See `doc/plan_arch_coverage.md`. |
| `arch sim --thread-sim {fsm\|parallel\|both} [--threads N]` | ✅ **Pre-lowering thread simulator.** `fsm` (default) lowers `thread` to FSM and reuses the standard sim emitter. `parallel` keeps threads as C++20 coroutines, one scheduler per user-thread; `--threads N` (1 ≤ N ≤ 64) maps user-threads to N OS threads via cache-line-padded atomic spin-wait `Barrier`. Two-region tick: posedge-first then comb-settle, both barrier-synchronized. Naturally race-free for owned-output designs (10-run trace identity, TSan-clean). `both` builds and runs both paths and diffs `--debug` traces line-by-line as a cross-check. **Cycle batching**: `dut.run_cycles(K)` amortizes per-tick overhead — `named_thread` at N=2/batch reaches 14.3 Mcyc/s (~2.2× fsm, ~15× Verilator N=2). See `doc/plan_thread_parallel_sim_phase3.md`. |
| Sim build perf | ✅ Default `-O2 -flto` for generated sim binaries (~10–20% over `-O2` alone on inst-heavy designs). Settle-loop input hoist: invariant port/reg copies pulled out of the 2-iteration settle loop, leaving only let/wire-typed (variant) connections inside. Override the optimization flags with `ARCH_OPT="-O3"`, enable ThreadSanitizer with `ARCH_TSAN=1`. Single-thread perf gap to Verilator (~2.6× on `tests/axi_dma_thread/ThreadMm2s.arch`) is now bounded structurally — closing it further requires sub-module inst inlining, queued in `doc/plan_sim_inst_inlining.md`. |
| `arch sim ... --cdc-random` | ✅ Randomizes synchronizer chain propagation latency via LFSR; `cdc_skip_pct` (0–100, default 25) is a public member on each C++ model, controllable from testbench at runtime |
| `arch sim ... --wave out.vcd` | ✅ VCD waveform output; auto-traces all ports and registers of the top-level module/construct; also works with standalone counter, fsm, etc.; opens in GTKWave/Surfer; testbenches can also call `trace_open("file.vcd")` / `trace_dump(time)` / `trace_close()` explicitly |
| `arch sim --debug [--depth N]` | ✅ Auto-instruments I/O port value changes; prints `[cycle][Mod.port](dir) old -> new` on every change; `--depth N` controls module hierarchy depth (default 1 = top only); cycle counter increments on posedge only; supports all port types: scalar, Vec (per-element `port[i]`), >64-bit (hex dump), bus (flattened with correct in/out direction including target flip); Clock ports excluded, Reset included |
| `arch sim --debug+fsm` | ✅ Additionally prints FSM state transitions: explicit FSMs show `[FSM][Name] STATE_A -> STATE_B (condition_expr)`; thread-lowered FSMs show `[FSM][Name.t0_state] S0 -> S1`; composable with `--debug` |
| `arch sim` pipeline support | ✅ Generates C++ models for `pipeline` constructs; stage-prefixed registers, valid propagation, reverse-order evaluation (NBA semantics), let bindings, flush directives |
| `arch check`/`build`/`sim`/`formal` learning capture | ✅ **Always on (v0.42.0+)**. Every compiler invocation records error→fix pairs into `~/.arch/learn/events.jsonl` — on failure, the current source + classified error is stashed as a pending entry; the next successful compile on the same file pairs them into an `error_fix` event and prints `📚 Learned: [<code>] <diff>`. Data never leaves the machine. **Controls**: `ARCH_NO_LEARN=1` disables all capture; `ARCH_LEARN_MAX_MB=<n>` sets the store cap (default 100 MB — warns once at ≥90%, hard-stops writes at 100%); `arch learn-clear` wipes the store. **Retrieval**: `arch learn-index` builds a BM25 lexical index, `arch advise <query>` prints top-K past error→fix pairs. No network, no telemetry, no external deps. See `doc/plan_arch_learning_system.md` for the full roadmap (idiom capture, shared contributor corpus, lint promotion). |
| `///` / `//!` doc comments + `//! ---` frontmatter | ✅ **v0.47.1+** (PR-doc-1, PR-doc-1.5). Lexer recognizes `///` (outer doc — attaches to next construct) and `//!` (inner doc — attaches to enclosing item) as distinct tokens; `////+` (4 or more slashes) remains a regular comment as documented escape hatch. Parser accumulates `DocOuter` / `DocInner` runs and attaches to AST. **Outer doc** is now supported on **every top-level item kind**: `module`, `fsm`, `fifo`, `ram`, `counter`, `arbiter`, `pipeline`, `cam`, `linklist`, `regfile`, `synchronizer`, `clkgate`, `domain`, `struct`, `enum`, `function`, `package`, `use`, `bus`, `template`. **Inner doc** is supported on every construct that has a body — consumed immediately after the opening keyword + name. **File-level**: `SourceFile.inner_doc` for the leading `//!` block; `SourceFile.frontmatter` for the YAML-style `//! ---\n…\n//! ---` block (stored verbatim — compiler does NOT parse YAML; downstream tooling does). PR-doc-1.6 (deferred) extends to member-level decls (port/reg/wire/let/inst/resource). Spec: `doc/plan_arch_doc_comments.md`. |
| `arch sim` **sim codegen fixes** | ✅ (1) `.sext<N>()` now correctly replicates the MSB into all upper bits instead of being treated identically to `.zext<N>()` (plain C++ cast); (2) `infer_expr_width` for `expr[Hi:Lo]` bit-slice now returns `Hi-Lo+1`, fixing incorrect source widths for subsequent sign extension; (3) `param` constants now emitted as `#define` in generated C++ headers for both `module` and `fsm` models; (4) `reg` init values with hex/bin/sized literals now correctly emitted in both constructor initializer and reset block (previously only `Dec` literals were handled, all others defaulted to 0); (5) comb-block intermediate signals (assigned in comb, used in inst connections) now declared as class member fields; (6) `eval_comb()` for modules with sub-instances now re-evaluates the full inst chain (input→eval_comb→output) so combinational feedback loops settle correctly when called from parent modules; (7) 2-pass settle loop in `eval()` for inst chains to handle valid/ready handshake loops across inst boundaries; (8) **derived clock eval ordering fix**: edge detection moved from `eval()` into `eval_posedge()` — derived clocks from sub-instances (clock dividers, clock gates) are now settled before edges are detected; internal clock wires from `seq on` blocks get proper edge trackers; sub-instance `eval_posedge()` self-detects clock edges so they work correctly when called from parent hierarchy |

---

### Language Constructs

| Construct | Status | Notes |
|-----------|--------|-------|
| `domain` | ✅ | Emitted as SV comments; **`SysDomain` is built-in** — no explicit `domain SysDomain end domain SysDomain` declaration needed; can be overridden by user |
| `struct` | ✅ | `typedef struct packed`; **packed bit layout: declaration-first = MSB** (SV convention, v0.41.1+). Earlier versions packed declaration-first = LSB — regenerate any pre-v0.41.1 `.sv` before mixing with new output |
| `enum` | ✅ | `typedef enum logic`; auto width ⌈log₂(N)⌉ |
| `module` | ✅ | Params, ports, reg/comb/let/wire/inst body; `seq on` clocked blocks with per-reg reset; **`default seq on clk rising\|falling;`** sets module-level default clock — enables multi-line `seq ... end seq` without explicit clock; **register syntax**: `reg x: UInt<8> [init VALUE] [reset SIGNAL=VALUE [sync\|async high\|low]];` — `init` (optional) sets SV declaration initializer, `reset SIGNAL=VALUE` (optional) sets async/sync reset with explicit reset value (value is **required** after `=`); `reset none` for no reset; `reg default:` applies defaults; compiler auto-generates reset guards; mixed reset/no-reset partitioning; **`let` two forms**: `let x: T = expr;` declares a new combinational wire (type required); `let x = expr;` (no type) assigns to an already-declared output port or wire — replaces the former `comb x = expr;` one-liner; `wire name: T;` declares a combinational net driven by `let x = expr;` or inside a `comb ... end comb` block (type checker enforces: only `wire` and output ports are valid comb targets; assigning a `reg` in `comb` is a compile error); **`comb` one-liner removed**: `comb x = expr;` is no longer valid — use `let x = expr;` instead; `comb ... end comb` block form still required for conditional assignments; **for loops**: `for VAR in START..END ... end for` in both `comb` and `seq` blocks — emits SV `for (int VAR = START; VAR <= END; VAR++)`; **indexed comb targets**: `port[i] = expr` in `comb` blocks is correctly detected as driving the port for driven-port and multiple-driver checks; **comb same-block reassignment**: multiple assignments to the same signal within a single `comb` block are allowed (default + override in if/elsif/else branches — standard latch-free combinational pattern) |
| `latch` block | ✅ | `latch on ENABLE ... end latch` — level-sensitive storage; enable signal must be `Bool` or `Clock`; body uses `<=` assignments to `reg` targets; emits SV `always_latch begin if (enable) ... end` |
| `fsm` | ✅ | State enum, `always_ff` state reg, `always_comb` next-state + output; **transition syntax**: `-> TargetState [when <expr>];` inside state bodies — omit `when` for unconditional; **`default ... end default` block**: contains `comb ... end comb` and/or `seq ... end seq` sub-blocks that provide default assignments emitted before the state `case` statement (so you don't repeat assignments in every state — states only override what differs); **datapath extension**: `reg` declarations and `let` bindings at FSM scope, `seq on clk rising ... end seq` blocks inside state bodies — compiler emits separate `always_ff` (state + datapath regs with reset + per-state seq) and `always_comb` (transitions + outputs); sim codegen supports FSM regs with `_n_` shadow variables and proper Bool width tracking; **implicit hold**: states default to staying in current state (`state_next = state_r`), so catch-all `-> Self when true` is not needed — but every state must have at least one transition (dead-end states are a compile error) |
| `fifo` | ✅ | Sync (extra-bit pointers) + async (gray-code CDC, auto-detected); `kind lifo` for stack; **`latency 0` only** (combinational read from memory array); `latency 1` (registered output + FWFT prefetch) planned — see spec §8.2b |
| `ram` | ✅ | `single`/`simple_dual`/`true_dual`/`rom`; `latency 0`/`1`/`2`; all write modes; `init: zero\|none\|file("path",hex\|bin)\|value\|[...]`; ROM: read-only, init required, no write signals; **SV codegen**: inline array → `initial begin mem[i] = val; ... end`, file → `$readmemh`/`$readmemb`; **sim codegen**: inline array → constructor initializer list, file → `fopen`/`fgets`/`strtoull`/`fclose` in constructor |
| `counter` | ✅ | `wrap`/`saturate`/`gray`/`one_hot`/`johnson` modes; `up`/`down`/`up_down`; `at_max`/`at_min` outputs |
| `arbiter` | ✅ | `round_robin`/`priority`/`lru`/`weighted`; `ports[N]` arrays; `grant_valid`/`grant_requester`; **custom policy via `hook`**: `policy: FnName;` + `hook grant_select(req_mask, last_grant, ...extra) -> UInt<N> = FnName(...);` — extra args bind to user-declared ports/params; function emitted inside arbiter module |
| `synchronizer` | ✅ | CDC synchronizer; `kind ff\|gray\|handshake\|reset\|pulse` (default `ff`): `ff` = N-stage FF chain (1-bit signals), `gray` = gray-code encode→FF chain→decode (multi-bit counters/pointers), `handshake` = req/ack toggle protocol (arbitrary multi-bit data), `reset` = async-assert / sync-deassert through N-stage FF chain (Bool only, reset deassertion synchronization), `pulse` = level-toggle in src domain → FF chain → edge-detect in dst domain to regenerate single-cycle pulse (Bool only, events/interrupts/triggers); `param STAGES` (default 2); requires 2 `Clock<Domain>` ports from different domains; supports `Bool` and `UInt<N>` data; async/sync reset; compile error on same-domain clocks; **multi-bit `kind ff` warning**: warns when `kind ff` used with `UInt<N>` where N>1, suggests `kind gray` or `kind handshake`; `kind reset` and `kind pulse` error if data is not `Bool`; SV codegen emits strategy-specific logic; sim codegen generates C++ models for all 5 kinds |
| `clkgate` | ✅ | First-class ICG (Integrated Clock Gating) cell; `kind latch` (default, ASIC: latch-based `always_latch`) or `kind and` (FPGA: simple AND gate); ports: `clk_in: in Clock<D>`, `enable: in Bool`, optional `test_en: in Bool`, `clk_out: out Clock<D>`; type checker enforces matching clock domains; SV + sim codegen |
| `as Clock<D>` cast | ✅ | Type cast: `Bool` or `UInt<1>` → `Clock<Domain>` via standard `as` syntax (e.g. `toggle as Clock<SysDomain>`); identity in SV (1-bit logic used as clock); enables clock dividers and custom clock generation in `module` without requiring a first-class construct |
| `regfile` | ✅ | Multi-read-port / multi-write-port; `forward write_before_read`; `init [i] = v` |
| `bus` | ✅ | Reusable port bundles with `initiator`/`target` perspectives; parameterized; signals have explicit `in`/`out` from initiator's perspective, `target` flips all directions; late flattening at codegen: `axi.aw_valid` → `axi_aw_valid` in SV; inst connections via `axi.signal <- wire` (initiator) and `axi.signal -> wire` (target); per-signal driven-port check in type checker (each bus signal treated as an individual port for drive coverage); sim codegen emits flattened C++ struct fields (`uint32_t axi_aw_valid`) and auto-traces all bus signals in VCD waveform output; clean Verilator lint |
| `tlm_method` (inside `bus`) | ✅ (v1 blocking, linear bodies) | **v0.44.16+**. Transaction-level method sub-construct inside `bus`. Wire protocol flattens to `<name>_req_valid`, `<name>_<arg>` per arg, `<name>_req_ready`, `<name>_rsp_valid`, `<name>_rsp_data` (omitted for void), `<name>_rsp_ready`. Target and initiator lowering both emit RegDecl + RegBlock + CombBlock directly into the parent module (bypassing generic thread lowering), so bus-port-member drives resolve naturally and `arch sim --pybind --test` handles state machines via existing reg/seq/comb C++ mirror. v1 restrictions: blocking mode only, linear SeqAssign-only initiator thread bodies, target bodies of `<SeqAssign/CombAssign/WaitUntil>* return expr;`, nested TLM calls rejected. Pipelined / out_of_order / burst are v2. Tier-2 SVA is designed but not yet emitted. See `doc/plan_tlm_method.md`. |
| `credit_channel` (inside `bus`) | ✅ | **v0.44.8+**. Stateful credit-based flow control. Compiler owns the sender counter, the receiver FIFO, and the Tier-2 protocol SVA. Declaration: `credit_channel data: send; param T: type = UInt<64>; param DEPTH: const = 8; param CAN_SEND_REGISTERED: const = 0; end credit_channel data`. Three flattened wires: `send_valid`, `send_data`, `credit_return`. Read-side method dispatch (`port.ch.can_send`, `port.ch.valid`, `port.ch.data`) rewrites in elaborate to `ExprKind::SynthIdent` pointing at codegen-emitted SV wires. Write-side sugar: `port.ch.send(x);` (drives valid+data), `port.ch.pop();` (drives credit_return), plus default-idle `port.ch.no_send();` and `port.ch.no_pop();`. **All source-level access is dotted** (`port.ch.send_valid`, `port.ch.credit_return`); the underscored form (`port.ch_send_valid`) is rejected by the elaborate pass with a suggestion to switch to dots — SV wire names retain underscores since SV has no nested namespacing. `CAN_SEND_REGISTERED=1` flops can_send off the next-state counter (option b: full throughput preserved, flop buys the downstream timing slack). Auto-emitted SVA: `_auto_cc_<port>_<ch>_{credit_bounds, send_requires_credit, credit_return_requires_buffered}`. Sim: `arch sim --pybind --test` mirrors both the sender counter and the receiver FIFO (module construct; pipeline/thread/arbiter emitters will inherit the same hook when needed). Implementation lives in `src/sim_credit_channel.rs`. See spec §18c and `doc/plan_credit_channel.md`. |
| `handshake` (inside `bus`) | ✅ | **v0.43.0+**. Compile-time sum-type sub-construct that collapses one repetitive valid/ready/payload channel into a single declaration. Six variants: `valid_ready`, `valid_only`, `ready_only`, `valid_stall`, `req_ack_4phase`, `req_ack_2phase`. Role keywords `send` / `receive` name the payload role — compiler derives every individual wire direction, eliminating the "I flipped valid and ready" bug class. **Tier 2** auto-emits per-variant protocol SVA (`_auto_hs_<port>_<ch>_valid_stable`, `_..._valid_stable_while_stall`, `_..._req_holds_until_ack`), verified on Verilator 5.034 `--assert` and EBMC 5.11. **Tier 1.5** catches payload correctness bugs across two layers: (a) runtime — `--inputs-start-uninit` warning is gated on the channel's valid/req so the producer bug "valid asserted, payload never set" fires while the legitimate "TB hasn't driven valid" case stays silent; (b) compile-time — `arch check` warns when a payload signal is read outside an `if <port>.<valid>` scope (direct match or AND-conjunct accepted; let-indirection deferred). See `doc/plan_handshake_construct.md` for variant timing diagrams and references (ARM IHI 0022, Sparsø & Furber). |
| `package` / `use` | ✅ | `package PkgName ... end package PkgName` groups enums, structs, functions, params; `use PkgName;` imports all names; emits SV `package`/`endpackage` + `import PkgName::*;` before module; file resolution: `PkgName.arch` in same directory; cycle detection; each file parsed once |
| `assert` / `cover` | ✅ | Concurrent SVA: `assert property (@(posedge clk) expr)` / `cover property (...)`; `implies` binary operator lowers to `(!a \|\| b)`; optional label; requires Clock port; typecheck verifies expr is Bool; emitted inside module + all 9 constructs; generate-for/if bodies supported; all SVA wrapped in `translate_off/on` for Yosys/synthesis compatibility; **auto-generated properties**: FIFO no-overflow/no-underflow, Counter count≤MAX, FSM legal-state (`!rst \|-> state_r < N`), FSM state reachability (`cover state_r == S` for each state), FSM transition coverage (`cover state_r == S && state_next == T` for each declared transition); verified with EBMC formal (`ebmc --top Mod --bound N --reset "rst==1" file.sv`) and Verilator `--assert` |
| `pipeline` | ✅ | Stages with reg/comb/let/inst body; per-stage `stall when`; `flush` directives; explicit forwarding mux via comb if/else; `valid_r` per-stage signal; cross-stage refs (`Stage.signal`); `inst` inside stages with auto-declared output wires |
| `function` | ✅ | Pure combinational; `return expr;`; `let` bindings as temporaries; **overloading** (same name, different arg types — mangled as `Name_8`, `Name_16`, etc.); emitted as SV `function automatic` inside each module that uses it |
| `log` | ✅ | Simulation logging: `log(Level, "TAG", "fmt %0d", arg)` in `seq`, `comb`, and `thread` blocks; levels `Always`/`Low`/`Medium`/`High`/`Full`/`Debug`; per-module `_arch_verbosity` integer; runtime control via `+arch_verbosity=N`; emits `$display` with `[%0t][LEVEL][TAG]` prefix; **file logging**: `log file("path") (Level, ...)` — auto `$fopen`/`$fclose` in `initial`/`final`; **in threads**: lowered into FSM state seq stmts; all `$display`/`$fwrite`/`$value$plusargs` and file I/O wrapped in `translate_off/on` for Yosys/synthesis compatibility |
| `generate for/if` | ✅ | Pre-resolve elaboration pass expands blocks when condition/bounds are compile-time constants; param-dependent `generate for` and `generate if` fall through to SV codegen as `generate for`/`if` blocks; port + inst items |
| `ram` (multi-var store) | ⚠️ | Single store variable only; compiler-managed address layout not implemented |
| `cam` | ✅ | Content-addressable memory; single + dual write port (v2); value payload (v3); posedge writes + combinational search; `arch sim` C++ model + SV codegen; tests under `tests/cam_*.arch` |
| `crossbar` | ❌ | Not implemented |
| `scoreboard` | ❌ | Not implemented |
| `reorder_buf` | ❌ | Not implemented |
| `pqueue` | ❌ | Not implemented |
| `linklist` | ✅ | `singly`/`doubly`/`circular_singly`/`circular_doubly`; per-op FSM controllers; `insert_head`/`insert_tail`/`insert_after`/`delete_head`/`delete`/`next`/`prev`/`alloc`/`free`/`read_data`/`write_data`; doubly: `_prev_mem` updated on all insert ops; `arch sim` C++ model verified against Verilator output |
| `pipe_reg` | ✅ | `pipe_reg name: source stages N;` — N-stage flip-flop delay chain; type inferred from source signal; clock/reset from `reg default`; output is read-only; works with ports, `let` bindings, reg outputs; SV emits chained `always_ff`; sim codegen uses `_n_` temporaries for correct non-blocking semantics |
| `template` | ✅ | User-defined interface contracts; `module Name implements Template` — compiler validates required params, ports, and hooks; templates emit no SV; multi-file cross-reference supported |
| `thread` | ✅ | Multi-cycle sequential block lowered to FSM + inst; `wait until`, `wait N cycle`, `thread once`, named/anonymous, `if/elsif/else` (waits in branches now supported via dispatch-and-rejoin lowering, v0.45.0+), `fork`/`join` (product-state expansion), `for` loops with `wait` (loop counter width inferred from end expression type — e.g. `burst_len_r: UInt<8>` → 8-bit counter), `generate for/if` with threads, `resource`/`lock` (per-resource arbiter synthesised as a real `arbiter` Item; full policy support — `mutex<round_robin\|priority\|lru\|weighted<W>\|MyFn>` with optional `hook grant_select(...)` for custom — v0.46.0+), `shared(or\|and)` (multi-driver reduction), `log` statements (lowered to FSM state seq stmts); spec: `doc/thread_spec_section.md`, `doc/thread_multi_outstanding_spec.md`; equivalence proof: `doc/thread_lowering_proof.md` (Lemma I §II.10 covers the if/else-with-waits case) |
| `bus` (TLM methods, v2: pipelined / out_of_order / burst) | ❌ | v1 `tlm_method` blocking shipped (see row above). v2 modes — `pipelined` (`Future<T>`), `out_of_order` (`Token<T,id>`), `burst` (`Future<Vec<T,L>>`) — still planned; spec: `doc/bus_spec_section.md` §19.2.2 |

---

### Type System

| Feature | Status | Notes |
|---------|--------|-------|
| `UInt<N>`, `SInt<N>` | ✅ | |
| `Bool`, `Bit` | ✅ | `Bool` and `UInt<1>` are treated as identical types throughout — freely assignable to each other, bitwise ops on 1-bit operands return `Bool` |
| `Clock<Domain>` | ✅ | Domain tracked for CDC detection |
| `Reset<Sync\|Async, High\|Low>` | ✅ | Optional polarity (defaults High); Async → `posedge rst` sensitivity |
| `Vec<T, N>` | ✅ | Emits as SV unpacked array `logic [W-1:0] name [0:N-1]`; init/reset uses `'{default: val}`; **multi-dimensional**: nested `Vec<Vec<T,N>,M>` supported — emits `logic [W-1:0] name [0:M-1][0:N-1]` with nested `'{default: '{default: val}}` reset; arbitrary nesting depth; multi-level indexing `arr[i][j]`; **indexed `seq` assignment type check**: `vec[i] <= expr` correctly checks against the element type (e.g. `UInt<32>`) — width mismatch like `vec[i] <= vec[i] + 1` (UInt<33> into UInt<32>) is now caught as an error |
| Named types (struct/enum refs) | ✅ | |
| `Token<T, id_width>` | ❌ | TLM only |
| `Future<T>` | ❌ | TLM only |
| `$clog2(expr)` in type args | ✅ | Parsed as expression, emitted as SV `$clog2(...)`, evaluated at compile time for const-folding |
| Clock domain mismatch (CDC errors) | ✅ | Compile error when a register driven in one domain is read in another domain's `seq` block **or** when a `comb` block reads a register from one domain and its output is consumed by a `seq` block in a different domain; message directs user to `synchronizer` or async `fifo`; warns on multi-bit `kind ff` synchronizers (suggests `kind gray` or `kind handshake`) |
| Reconvergent CDC path detection | ❌ | **Planned** — detect when bits of the same source-domain register cross through independent synchronizers and recombine in the destination domain; trace signal origins back to source register through bit-slices and combinational logic; see spec §5.2a |
| Reset domain crossing (RDC errors) | ❌ | **Planned** — `Reset<Kind, Polarity, Domain>` third parameter parsed but not enforced; will mirror CDC infrastructure to flag cross-reset-domain register reads, async reset deassertion ordering, and reset glitch propagation; see spec §5.4 |
| `Tristate<T>` / bidirectional I/O | ❌ | **Planned** — `tristate` port direction + `tristate ... end tristate` block for pad-level bidirectional I/O (I2C, GPIO); SV emits `inout` + ternary-Z; sim decomposes to `_out/_oe/_in` (2-state); restricted to top-level modules; see spec §5.5 |
| Width mismatch at assignment | ✅ | Errors for any RHS wider than LHS in both `always` and `comb` blocks; arithmetic widening (`+1`) flagged with explicit hint to use `.trunc<N>()` |
| Implicit truncation prevention | ✅ | `r <= r + 1` is a compile error; write `r <= (r + 1).trunc<N>()` explicitly. `.trunc<N>()` emits SV size cast `N'(expr)`. `expr[hi:lo]` bit-slice emits `expr[hi:lo]` for field extraction (e.g. `instr[11:7]`). Sim codegen applies bitmask `& ((1<<N)-1)` for sub-word types (e.g. `UInt<2>` in `uint8_t`). |

---

### Expressions & Operators

| Feature | Status |
|---------|--------|
| Literals (dec, hex, bin, sized) | ✅ |
| `true` / `false` | ✅ |
| Arithmetic `+ - * / %` | ✅ |
| Comparison `== != < > <= >=` | ✅ |
| Logical `and` / `or` / `not` | ✅ |
| Bitwise `& \| ^ ~ << >>` | ✅ |
| Reduction `&x \|x ^x` | ✅ Unary prefix on `UInt<N>`/`SInt<N>`; result is `Bool`; emits SV `&expr`, `\|expr`, `^expr` |
| Field access `.field` | ✅ |
| Array index `[i]` | ✅ |
| `.trunc<N>()` / `.zext<N>()` / `.sext<N>()` / `expr[hi:lo]` bit-slice / `.reverse(N)` | ✅ | `.reverse(N)` reverses in N-bit chunks; emits SV `{<<N{expr}}`; type checker enforces width divisible by N |
| `signed(expr)` / `unsigned(expr)` | ✅ | Same-width reinterpret cast: `signed(UInt<8>)` → `SInt<8>`, `unsigned(SInt<8>)` → `UInt<8>`; emits `$signed()`/`$unsigned()` in SV; eliminates `.sext<N>()` when entering signed arithmetic chains |
| `as` cast | ✅ | Width-checked: source and target must have same total bit width; emits SV `Type'(expr)`; struct-to-struct casts supported for same-width packed structs |
| Struct literals | ✅ |
| Enum variants `E::Variant` | ✅ |
| `todo!` | ✅ |
| `?:` ternary | ✅ Right-associative; any expression context; chains naturally for priority muxes |
| Expression-level `match` | ✅ As `CombAssign` RHS → `case` block; as inline expression → nested ternary chain |
| `$clog2(x)` | ✅ |
| Function calls `Name(args)` | ✅ Resolved at call site; overload-resolved by argument types |
| `inside` set membership | ✅ `expr inside {val1, val2, lo..hi}` — returns `Bool`; emits SV `inside` operator; supports individual values and inclusive ranges |

---

### Statements

| Feature | Status |
|---------|--------|
| `comb` block | ✅ `comb ... end comb` block for conditional assignments; one-liner `comb y = expr;` removed — use `let y = expr;` instead |
| `reg` assignment `<=` | ✅ |
| `if / elsif / else` | ✅ `elsif` keyword for chained conditionals (not `else if`); resolves ambiguity in brace-free syntax |
| `unique if` / `unique match` | ✅ `unique if cond ...` and `unique match expr ...` assert mutual exclusivity to the synthesis tool; emits SV `unique if (...)` and `unique case (...)`; enables parallel mux optimization |
| `match` (reg and comb blocks) | ✅ |
| Wildcard `_` → `default:` | ✅ |
| `let` bindings | ✅ Two forms: `let x: T = expr;` declares a new combinational wire (type required); `let x = expr;` (no type) assigns to an already-declared output port or wire — replaces the former `comb x = expr;` one-liner |
| `wire` declarations | ✅ `wire x: T;` — combinational net with explicit type, no initializer; must be driven in a `comb` block with `=`; SV codegen emits `logic [N-1:0] x;` driven in `assign`/`always_comb`; sim codegen emits private member assigned in `eval_comb()`; type checker enforces only `wire` and output ports are valid `comb` targets (`reg` in `comb` is a compile error) |
| `port reg` declarations | ✅ `port reg name: out T [init V] [reset R=V];` — output port that is also a register; assigned with `<=` in `seq` blocks; eliminates `reg r` + `comb out = r;` boilerplate; inherits from `reg default:` if present; `in` direction is a compile error; SV codegen emits `output logic` driven in `always_ff`; sim codegen uses private shadow register with commit-to-port |
| `log(Level, "TAG", "fmt", args...)` | ✅ In `seq`, `comb`, and `thread` blocks; runtime verbosity via `+arch_verbosity=N`; **file logging**: `log file("path") (Level, "TAG", "fmt", args...)` writes to file via `$fwrite`/`fprintf`; auto `$fopen` in `initial`/constructor, `$fclose` in `final`/destructor |
| `reg default: init 0 reset rst;` | ✅ Sets default `init`/`reset` for all regs in scope; individual regs may override either field |
| `{a, b, c}` bit concatenation | ✅ MSB-first; emits SV `{a, b, c}`; sim codegen shift-OR with 128-bit support |
| `{N{expr}}` bit replication | ✅ Emits SV `{N{expr}}`; nestable inside concat `{{8{sign}}, data}`; sim codegen `_arch_repeat` helper |
| `for i in {list}` value-list iteration | ✅ `for i in {10, 20, 30} ... end for` — compile-time unrolled; each value gets its own block; works in `comb` and `seq` blocks |
| `assert` / `cover` | ✅ | Concurrent SVA; `implies` operator; optional label; requires Clock port |

---

### Type Checking

| Check | Status |
|-------|--------|
| PascalCase (types), snake_case (signals), UPPER_SNAKE (params) | ⚪ Recommended, not compiler-enforced |
| `in`, `out`, `state` as contextual keywords | ✅ | Can be used as port/signal names; only act as keywords in their specific grammar positions |
| Duplicate definitions | ✅ |
| Undefined name references | ✅ |
| Output ports must be driven | ✅ |
| Single driver per signal | ✅ |
| `todo!` site warning | ✅ |
| Binary op result widths (IEEE 1800-2012 §11.6) | ✅ |
| Width mismatch at assignment | ✅ Any RHS wider than LHS errors in both `always` and `comb` blocks; arithmetic widening hint included |
| Clock domain crossing errors | ✅ | seq→seq and comb→seq crossings detected; extends across `inst` boundaries (compiler traces clock port connections to map child domains to parent domains) |
| Exhaustive match arm checking | ✅ Enum matches must cover all variants or include a wildcard `_`; missing variants named in error |
| Hierarchical instance references forbidden | ✅ `inst_name.port_name` in expressions is a compile error; must use `port -> wire_name` in the inst block instead |
| Unconnected inst ports | ✅ Missing input port in an `inst` block → compile error; missing output port → warning. Clock/Reset ports are exempt (may be wired implicitly via domain). |
| Const param evaluation (complex exprs) | ✅ Derived params (default expr references other params) preserve expressions in SV output; non-derived params evaluate to literals |

---

### Tests

- **VerilogEval benchmark**: 154/156 problems passing (combinational, sequential, latches, counters, shift registers, LFSRs, edge detectors, BCD counters, rotators, muxes, vector ops, cellular automata, branch predictors, dual-edge FF, FSMs — Moore, Mealy, one-hot, serial protocol, PS/2, lemmings, timers, arbiters, reservoir controllers); 18 of 21 FSM problems now use the first-class `fsm` construct (3 remain as `module`: Prob137/Prob146 serial receivers, Prob155 lemmings4 — complex datapath interactions); 2 dataset bugs skipped (Prob099: test/ref port mismatch, Prob118: ref Verilator incompatibility); **98.7% coverage** of the 156-problem NVIDIA/HDLBits spec-to-RTL dataset; covers Prob001–Prob156 from the NVIDIA/HDLBits spec-to-RTL dataset; each solution is an `.arch` file compiled to SV and verified against golden reference via Verilator
- 52 integration tests (snapshot + error-case), including `let` binding, `generate for`, `generate if`, mixed reset/no-reset partitioning, reset consistency validation, pipeline (simple, CPU 4-stage, instantiation, stage inst, bit-range trunc), `$clog2` in type args, function overloading, width mismatch errors, exhaustive match checking, linklist (basic singly + doubly), ROM (`kind: rom` with inline hex array)
- 9 Verilator simulations: Counter, TrafficLight FSM, TxQueue sync FIFO, AsyncBridge async FIFO, SimpleMem RAM, WrapCounter, BusArbiter (round-robin), IntRegs (regfile + forwarding), CpuPipe 4-stage pipeline (reset, flow, stall, flush, forwarding), BufMgr (16K×128b, 256 queues, 19 tests — multi-file split SV verified)
- 13 `arch sim` native C++ simulations verified: WrapCounter (`counter`), TrafficLight (`fsm`), Top+Counter (`module` with sub-instance), AesCipherTop (AES-128 full cipher with sub-instance + wide signals + functions), AesKeyExpand128 (key expansion with sub-instance timing), e203_exu_alu_dpath (26 tests), e203_exu_alu_bjp (25 tests — first clock-free module in test suite), linklist_basic (singly FIFO; arch sim output identical to Verilator), linklist_doubly (doubly list with next/prev/insert_after; arch sim output identical to Verilator), buf_mgr_sm (16×32b shared buffer manager; 4 queues; 17 tests), buf_mgr (16K×128b shared buffer manager; 256 queues; 2-bank free-list with prefetch; 19 tests), RomLut (ROM inline hex array; 5 tests), RomLutFile (ROM `init: file(...)` hex; 9 tests — verifies `$readmemh` / `fopen` file-load path)
- **BufMgr benchmark** (shared-memory buffer manager): 16K entries × 128-bit data pool, 256 dynamically-sharing queues, simultaneous enqueue + dequeue every cycle; all RAMs `sync_out` (2-cycle read latency); 2-bank free-list interleaving with 4-entry prefetch FIFO to sustain 1 alloc/cycle; 3-stage enqueue/dequeue pipelines with tail/head bypass forwarding; small variant (`buf_mgr_sm`, 16×32b, 4 queues, 17 tests) and full variant (`buf_mgr`, 16K×128b, 256 queues, 19 tests); exercises `ram` sim codegen with `module` hierarchical instantiation
- `arch sim` supports **multi-clock domain** modules: each `Clock<Domain>` port gets independent `_rising_X` edge detection; `eval_posedge()` guards each `seq` block on its specific clock's rising edge; auto-generates `tick()` method from domain `freq_mhz` declarations (computes half-periods via GCD for correct clock ratio); single-clock modules unchanged; verified with 200MHz/50MHz dual-clock testbench (MultiClockSync, 80 ticks, 4:1 ratio, 0 errors)
- `arch sim` supports purely combinational modules (no `Clock<>` port): generated `eval()` skips `_rising` edge detection — testbenches call `eval()` directly without toggling a clock signal
- AES-128 cipher benchmark (NIST FIPS-197 test vectors verified via `arch sim`): AesSbox + Xtime as pure combinational functions; AesCipherTop + AesKeyExpand128 using inline function calls replacing 32 `inst` blocks; wide `UInt<128>` ports via `VlWide<4>`; correct hierarchical posedge simultaneity (all `always_ff` blocks across parent + sub-instance fire atomically)
- **E203 HBirdv2 benchmark suite** (39 modules — full RISC-V SoC with peripherals + clock gating):
  - **Core pipeline** (21 modules):
  - `e203_exu_regfile`: 2R1W register file using `regfile` construct; 5 sim tests
  - `e203_exu_wbck`: Priority write-back arbiter; 6 sim tests
  - `e203_ifu_litebpu`: Static branch prediction unit; 11 sim tests
  - `e203_exu_alu_dpath`: Shared ALU datapath; 26 sim tests
  - `e203_exu_alu_bjp`: Branch/jump unit; purely combinational; 25 sim tests
  - `e203_exu_alu`: ALU top-level (AluDpath + BjpUnit); 20 sim tests
  - `e203_exu_decode`: RV32IM instruction decoder; 30 sim tests + 22 Verilator tests
  - `e203_exu_muldiv`: Iterative multiply/divide (`module` + `fsm` variants); 24 sim tests + 12 Verilator tests
  - `e203_exu_commit`: Execution commit unit; 38 sim tests + 20 Verilator tests
  - `e203_ifu_ifetch`: Instruction fetch FSM; 23 sim tests + 10 Verilator tests
  - `e203_lsu_ctrl`: Load-store unit; 34 sim tests + 16 Verilator tests
  - `e203_clint_timer`: CLINT timer; 18 sim tests + 8 Verilator tests
  - `e203_exu_disp`: Execution dispatch; 28 sim tests
  - `e203_exu_oitf`: Outstanding Instruction Track FIFO; 6 sim tests
  - `e203_exu_agu`: Address generation unit; rs1+imm address, byte-enable, store alignment, load sign-extension; 20 sim tests
  - `e203_exu_csr`: CSR register file; mstatus/mie/mtvec/mepc/mcause/mtval/mip/mscratch/mcycle/minstret; trap entry/exit; 14 sim tests
  - `e203_exu_longpwbck`: Long-pipe writeback collector; LSU > MulDiv priority; 16 sim tests
  - `e203_ifu_litedec`: Instruction length detector + quick decode; 16/32-bit detection, JAL/branch immediate extraction; 24 sim tests
  - `e203_exu_top`: Execution unit top-level; 6-level deep `inst` hierarchy; 12 sim tests
  - `e203_core_top`: Core top-level (IFU + EXU + LSU + BIU + ITCM + DTCM + CLINT); 11 sim tests
  - **Bus fabric** (3 modules):
  - `e203_icb_arbt`: 2-master ICB round-robin arbiter; 15 sim tests
  - `e203_icb2apb`: ICB-to-APB bridge; FSM IDLE→SETUP→ACCESS; 20 sim tests
  - `e203_sram_ctrl`: SRAM controller with `ram SramBank` instance; 8 sim tests
  - **Peripheral subsystem** (7 modules):
  - `e203_ppi`: Private peripheral interface; ICB→APB 4-slave address decode; 12 sim tests
  - `e203_fio`: Fast I/O port; 16-register ICB slave; 7 sim tests
  - `e203_gpio`: GPIO peripheral; 32-bit I/O, rise/fall edge interrupt, W1C pending; 8 sim tests
  - `e203_uart`: UART peripheral; shift-register TX/RX, configurable baud divider; 12 sim tests
  - `e203_spi`: SPI master; configurable CPOL/CPHA, clock divider; 13 sim tests
  - `e203_irq_ctrl`: Interrupt controller; MEI/MSI/MTI priority per RISC-V spec; 11 sim tests
  - `e203_debug_module`: Debug module (RISC-V Debug Spec 0.13); dmcontrol/dmstatus/data0/command; 16 sim tests
  - **Clock infrastructure** (2 modules):
  - `e203_clkgate`: Latch-based ICG cell using `clkgate` construct
  - `e203_clk_ctrl`: Clock controller — 4 ICG instances (IFU/EXU/LSU/BIU gating)
  - **SoC top-level integration** (1 module):
  - `e203_soc_top`: Full SoC — CoreTop + ICB arbiter + SRAM + PPI (GPIO + UART + SPI) + FIO + IrqCtrl + DebugModule; 37 .arch files, 39 SV modules; `wire` bus interconnect; latched peripheral select registers for response mux; Verilator lint clean; 11 arch sim tests + 11 Verilator tests (VCD waveform verified)

---

### Tooling

| Tool | Status |
|------|--------|
| VSCode syntax extension | ✅ TextMate grammar (`editors/vscode/`); install: symlink to `~/.vscode/extensions/arch-hdl`; covers all keywords, types, operators, numeric literals, comments |
| Vim syntax | ✅ `editors/vim/syntax/arch.vim` |
| ARCH MCP server | ✅ Tools: `get_construct_syntax(construct)` — syntax template + reserved keywords; `write_and_check(path, content)` — write + type-check in one call; `arch_build_and_lint(files, top_module)` — build SV + Verilator lint in one call; server instructions guide AI workflow: fetch syntax → write_and_check → build_and_lint |

---

## Remaining Features

### Correctness Gaps (no new constructs needed)

| # | Feature | Effort |
|---|---------|--------|
| ~~1~~ | ~~**Width mismatch at assignment**~~ | **DONE** — any width delta errors in `seq` and `comb` |
| ~~2~~ | ~~**Exhaustive `match` checking**~~ | **DONE** — missing variants named in error; wildcard `_` suppresses |
| ~~3~~ | ~~**CDC error detection**~~ | **DONE** — cross-domain register read → compile error (seq→seq and comb→seq paths); `synchronizer` and async `fifo` are the legal CDC crossing mechanisms |
| ~~4~~ | ~~**Const param evaluation at instantiation**~~ | **DONE** — derived params preserve expressions in SV; `UInt<WIDTH*2>` works when parent param overridden |
| 5 | **Function type-parametric overloads** — type parameters on functions (e.g. `function Foo<T>(a: T) -> T`) | High |

### Missing Constructs (in spec order)

| # | Construct | Complexity | What it generates |
|---|-----------|------------|-------------------|
| ~~1~~ | ~~**`$clog2(expr)` in type args**~~ | ~~Low~~ | **DONE** |
| ~~2~~ | ~~**`generate for/if`**~~ | ~~Medium~~ | **DONE** — elaboration pass expands before resolve |
| ~~3~~ | ~~**`pipeline`**~~ | ~~High~~ | **DONE** — valid/stall propagation, flush masks, explicit forwarding mux, `valid_r` gating, cross-stage refs, inst inside stages |
| ~~4~~ | ~~**`function`**~~ | ~~Medium~~ | **DONE** — pure combinational, `return`, `let` bindings, overloading by argument type; emits `function automatic` in SV |
| ~~5~~ | ~~**`assert` / `cover`**~~ | ~~Low~~ | **DONE** — concurrent SVA with `implies`, auto-generated FIFO/Counter/FSM properties, EBMC + Verilator `--assert` verified |
| 6 | **`ram` multi-var store** | Medium | Compiler-managed address layout across multiple logical variables |
| ~~7~~ | ~~**`cam`**~~ | ~~High~~ | **DONE** — single + dual write port + value payload variants; `arch sim` C++ model + SV codegen |
| 8 | **`crossbar`** | High | N×M switch fabric with arbitration |
| 9 | **`scoreboard`** | High | Issue/complete tracking, hazard detection |
| 10 | **`reorder_buf`** | High | Out-of-order completion, in-order retirement |
| 11 | **`pqueue`** | High | Priority queue with enqueue/dequeue |
| ~~12~~ | ~~**`linklist`**~~ | ~~High~~ | **DONE** — singly/doubly/circular variants; all standard ops; prev-pointer maintenance; arch sim C++ model |

### Planned Language Features

| # | Feature | Description |
|---|---------|-------------|
| 1 | **Reset Domain Crossing (RDC) checking** | `Reset<Kind, Polarity, Domain>` — extend type checker to build `reg_reset_domain` map alongside existing `reg_domain`; flag cross-reset-domain register reads, deassertion ordering violations, and unsynchronized reset glitches; require `reset_synchronizer` or `rdc_safe` annotation; mirrors existing CDC check infrastructure in `typecheck.rs` |
| 2 | **Tristate / bidirectional I/O** | `Tristate<T>` type + `tristate` block for pad-level I/O; SV codegen emits `inout` with Z-driver; sim decomposes to `_out/_oe/_in` (2-state); type checker restricts to top-level modules; supports open-drain (wire-AND) resolution for I2C/GPIO |
| 3 | **FIFO `latency 1` (registered output + FWFT)** | Registered `pop_data` with first-word fall-through prefetch; consumer interface identical to `latency 0` but `pop_data` comes from a flop, not memory mux — timing-clean for deep FIFOs; explicit designer choice, no auto-selection; see spec §8.2b |
| 4 | **Package-scoped modules** | Allow hardware constructs (module, fsm, etc.) inside `package`; `inst a: PkgName::ModuleName` for namespace-qualified instantiation; SV codegen flattens to `PkgName_ModuleName`; resolves name collisions without tool-specific library mapping (SV limitation: modules are always global) |
| ~~5~~ | ~~**`--check-uninit` extended coverage**~~ | **DONE (2026-04-17)** — (1) RAM cells → `--check-uninit-ram` (per-cell valid bitmap, `init:` cells pre-marked, ROMs exempt); (2) Vec index / bit-select / variable part-select → always-on `_ARCH_BCHK` hard abort + auto-emitted concurrent SVA; (3) division-by-zero → three-layer check (compile error on const /0, runtime abort on non-const /0, auto-emitted SVA). Primary inputs → `--inputs-start-uninit` (bonus addition). EBMC 5.11 + Verilator 5.034 verified. |
| 6 | **`pipe_reg` built-in valid tracking** | Optional `valid` clause: `pipe_reg product_d2: product stages 2 valid valid_in;` — compiler generates a parallel valid chain at the same depth, auto-named `product_d2_valid`; guarantees data/valid alignment by construction; enables `--check-uninit` to flag reads when valid is low; future: `flush` signal support to clear valid chain independently |
| 7 | **`generate_if` in bus bodies** | Allow conditional signals in bus definitions via `generate_if`: `bus BusAxi4 param READ: const = 1; param WRITE: const = 1; generate_if READ ... end generate_if`. Enables single parameterized bus definition for read-only, write-only, and full variants. Requires bus-level generate expansion during elaboration. |
| 8 | **Bus subset casting** | Explicit narrowing: `axi <- axi_full as BusAxi4<READ=1, WRITE=0>` connects only the read signals from a full bus. Remaining signals (write half) must be connected separately or tied off explicitly — compiler errors on unconnected signals, no implicit tie-off. Widening (read-only → full) is an error. Uses existing `as` cast syntax — no new grammar needed. |
| 9 | **`multicycle` reg annotation** | `reg result: UInt<32> multicycle 3 reset rst=0;` — declares that the combinational path feeding this register has a multi-cycle timing budget. No extra flops are inserted (unlike `pipe_reg`); the register remains a single flop. Saves area and power for slow-settling paths (multipliers, dividers, complex ALU ops). **Sim** (`--check-uninit`): compiler auto-detects all input signals feeding the reg (via expression tree walk), inserts hidden valid tracking with change detection and latency counter — reads before the counter expires return poison/X. **Synthesis**: emits SDC constraints (`set_multicycle_path N -to result`). **Formal**: optional `assert property` to verify the multicycle assumption holds. |
| ~~10~~ | ~~**Temporal assert / cover sugar — Phases 1 + 2**~~ | **DONE** — `past(expr, N)`, `a \|=> b`, `rose(a)`, `fell(a)`, and `##N expr` (SVA forward cycle-shift) are all accepted inside `assert`/`cover` bodies; using any of them outside that scope is a typecheck error. `arch build` emits SV `$past` / `$rose` / `$fell` / `\|=>` / `##N` directly. `arch formal` BMC encoder uses cycle-shifted term references for all five — `past(_, N)` shifts back, `rose`/`fell` are depth-1 past edges, `##N expr` shifts forward, top-level `\|=>` adds `+1 + future_depth(lhs)` to the RHS sample. Cycles outside the well-defined window (`[max_past_depth, bound − max_future_depth − \|=>_extra]`) are skipped under SVA vacuous-true / vacuous-no-hit semantics. Verilator 5.x doesn't yet accept `##N` ("unsupported sequence expression"); use EBMC for property checking with `##N`. Sequence composition (`##[a:b]`, `[*n]`, `throughout`, `within`, `first_match`) and unbounded liveness (`s_eventually`, strong/weak operators) remain intentionally out of scope. |

### CLI & Backend

| # | Feature | Notes |
|---|---------|-------|
| ~~1~~ | ~~**Multi-file compilation**~~ | **DONE** — `arch build a.arch b.arch` concatenates and cross-resolves; `arch build a.arch b.arch` without `-o` emits one `.sv` per input |
| ~~2~~ | ~~**`arch sim`**~~ | **DONE** — `arch sim Foo.arch --tb Foo_tb.cpp`; generates Verilator-compatible C++ models for `module`, `counter`, `fsm`; compiles with `g++`; runs binary; verified with counter, FSM, and top-level module testbenches |
| ~~3~~ | ~~**`arch formal`**~~ | **DONE (2026-04-17; hierarchical v1 2026-04-24)** — direct AST→SMT-LIB2 bounded model checking, no Yosys/sby in the loop. `arch formal F.arch [--top Name] [--bound N] [--solver z3\|boolector\|bitwuzla] [--emit-smt out.smt2] [--timeout S]`. Encodes registers as per-cycle BV variables, next-state as an `ite` chain, and asserts/covers as per-cycle disjunctions; one `check-sat` per property. Exit codes: 0 all PROVED/HIT, 1 any REFUTED/NOT-REACHED, 2 any INCONCLUSIVE. Scope v1: flat module, scalar types, single clock, no Vec/struct/enum; errors clearly on unsupported constructs. **Hierarchical v1 (2026-04-24)**: one level of `inst` nesting is now supported via a bottom-up flattening pre-pass. Sub-modules may contain `let` bindings, `comb` blocks (Assign + IfElse stmts), `reg` decls, and `seq` blocks (Assign + IfElse stmts). Sub-ports must bind to simple identifiers in parent connections. Multi-inst of the same sub-module is supported — prefix mangling (`<inst>_<local>`) keeps state separate. Deferred: nested hierarchy (`inst` inside an inst), multi-clock composition, Vec/struct/enum across boundaries, more complex comb/seq statement kinds (Match/For/Log/Init/WaitUntil/DoUntil). **Verified end-to-end**: counter (`cnt <= 200` → PROVED), 4-bit overflow (`cnt != 15` → REFUTED with cex), cover (`cnt == 8` → HIT at witness cycle, NOT REACHED at low bound), guard contract (`valid implies written` → PROVED; missing write → REFUTED). Hierarchical: Adder inst invariant (PROVED), SubCounter inst bounded (PROVED), two independent SubCounter insts (both PROVED, prefix mangling). Solver parity on z3 + boolector + bitwuzla (14 integration tests). Complements the SV-SVA path consumed by EBMC and Verilator `--assert` — both paths ship. |
| 4 | **`bus` TLM methods** | `methods ... end methods` inside `bus`; `implement BusName.method rtl` with `wait until`/`fork`-`join` → synthesizable FSM; all four modes (`blocking`/`pipelined`/`out_of_order`/`burst`) synthesizable with declared bounds; spec in `doc/bus_spec_section.md` §19.2.2 |
| ~~5~~ | ~~**Waveform output**~~ | **DONE** — `arch sim ... --wave out.vcd` emits VCD auto-tracing all top-level ports/regs + flattened bus signals; opens in GTKWave/Surfer; testbenches can also call `trace_open/trace_dump/trace_close` explicitly |
