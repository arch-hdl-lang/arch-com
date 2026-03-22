# ARCH Compiler — Status & Roadmap

> Last updated: 2026-03-22
> Compiler version: 0.27.0 (VCD waveform support, ROM, file logging)

---

## Implemented

### CLI

| Command | Status |
|---------|--------|
| `arch check <file.arch>` | ✅ Parse + type-check; exits 0 on success |
| `arch build <file.arch> [-o out.sv]` | ✅ Emits deterministic SystemVerilog; **SV codegen fixes**: (1) signed cast emits `$signed(x)` instead of `logic signed [N-1:0]'(x)` for Verilator compatibility; (2) `>>` on SInt operands emits `>>>` (arithmetic shift right); (3) `.zext<N>()` emits `N'($unsigned(x))` to prevent context-dependent width expansion |
| `arch build a.arch b.arch` | ✅ Multi-file: concatenates + cross-resolves; one `.sv` per input (or single combined file with `-o`) |
| `arch sim <file.arch> --tb <tb.cpp>` | ✅ Generates Verilator-compatible C++ models (`VName.h` + `VName.cpp` + `verilated.h`), compiles with `g++`, and runs; supports `module`, `counter`, `fsm`, `linklist`, `ram`; `fifo`/`arbiter`/`regfile` pending |
| `arch sim ... --check-uninit` | ✅ Detects reads of uninitialized `reset none` registers; shadow valid bits propagate through `pipe_reg` chains; warn-once per signal to stderr |
| `arch sim ... --cdc-random` | ✅ Randomizes synchronizer chain propagation latency via LFSR; `cdc_skip_pct` (0–100, default 25) is a public member on each C++ model, controllable from testbench at runtime |
| `arch sim ... --wave out.vcd` | ✅ VCD waveform output; auto-traces all ports and registers of the top-level module/construct; also works with standalone counter, fsm, etc.; opens in GTKWave/Surfer; testbenches can also call `trace_open("file.vcd")` / `trace_dump(time)` / `trace_close()` explicitly |
| `arch sim` **sim codegen fixes** | ✅ (1) `.sext<N>()` now correctly replicates the MSB into all upper bits instead of being treated identically to `.zext<N>()` (plain C++ cast); (2) `infer_expr_width` for two-arg `.trunc<Hi,Lo>()` now returns `Hi-Lo+1` instead of `Hi`, fixing incorrect source widths for subsequent sign extension; (3) `param` constants now emitted as `#define` in generated C++ headers for both `module` and `fsm` models; (4) `reg` init values with hex/bin/sized literals now correctly emitted in both constructor initializer and reset block (previously only `Dec` literals were handled, all others defaulted to 0); (5) comb-block intermediate signals (assigned in comb, used in inst connections) now declared as class member fields; (6) `eval_comb()` for modules with sub-instances now re-evaluates the full inst chain (input→eval_comb→output) so combinational feedback loops settle correctly when called from parent modules; (7) 2-pass settle loop in `eval()` for inst chains to handle valid/ready handshake loops across inst boundaries |

---

### Language Constructs

| Construct | Status | Notes |
|-----------|--------|-------|
| `domain` | ✅ | Emitted as SV comments |
| `struct` | ✅ | `typedef struct packed` |
| `enum` | ✅ | `typedef enum logic`; auto width ⌈log₂(N)⌉ |
| `module` | ✅ | Params, ports, reg/comb/let/wire/inst body; `seq on` clocked blocks with per-reg reset (`reset <signal> sync\|async high\|low` or `reset none`); compiler auto-generates reset guards; mixed reset/no-reset partitioning; `reg default: init 0 reset rst;` wildcard default for register declarations; `wire name: T;` declares a combinational net driven in a `comb` block (type checker enforces: only `wire` and output ports are valid `comb` targets — assigning a `reg` in `comb` is a compile error); SV codegen emits `logic [N-1:0] name;` driven by `assign`/`always_comb`; sim codegen treats `wire` as a private member assigned in `eval_comb()` |
| `fsm` | ✅ | State enum, `always_ff` state reg, `always_comb` next-state + output; `default expr` on output ports; **datapath extension**: `reg` declarations and `let` bindings at FSM scope, `seq on clk rising ... end seq` blocks inside state bodies — compiler emits separate `always_ff` (state + datapath regs with reset + per-state seq) and `always_comb` (transitions + outputs); sim codegen supports FSM regs with `_n_` shadow variables and proper Bool width tracking; **implicit hold**: states default to staying in current state (`state_next = state_r`), so catch-all `transition to Self when true` is not needed — but every state must have at least one transition (dead-end states are a compile error) |
| `fifo` | ✅ | Sync (extra-bit pointers) + async (gray-code CDC, auto-detected) |
| `ram` | ✅ | `single`/`simple_dual`/`true_dual`/`rom`; `latency 0`/`1`/`2`; all write modes; `init: zero\|none\|file("path",hex\|bin)\|value\|[...]`; ROM: read-only, init required, no write signals |
| `counter` | ✅ | `wrap`/`saturate`/`gray`/`one_hot`/`johnson` modes; `up`/`down`/`up_down`; `at_max`/`at_min` outputs |
| `arbiter` | ✅ | `round_robin`/`priority`/`lru`/`weighted`; `ports[N]` arrays; `grant_valid`/`grant_requester`; **custom policy via `hook`**: `policy: FnName;` + `hook grant_select(req_mask, last_grant, ...extra) -> UInt<N> = FnName(...);` — extra args bind to user-declared ports/params; function emitted inside arbiter module |
| `synchronizer` | ✅ | CDC synchronizer; `kind ff\|gray\|handshake\|reset\|pulse` (default `ff`): `ff` = N-stage FF chain (1-bit signals), `gray` = gray-code encode→FF chain→decode (multi-bit counters/pointers), `handshake` = req/ack toggle protocol (arbitrary multi-bit data), `reset` = async-assert / sync-deassert through N-stage FF chain (Bool only, reset deassertion synchronization), `pulse` = level-toggle in src domain → FF chain → edge-detect in dst domain to regenerate single-cycle pulse (Bool only, events/interrupts/triggers); `param STAGES` (default 2); requires 2 `Clock<Domain>` ports from different domains; supports `Bool` and `UInt<N>` data; async/sync reset; compile error on same-domain clocks; **multi-bit `kind ff` warning**: warns when `kind ff` used with `UInt<N>` where N>1, suggests `kind gray` or `kind handshake`; `kind reset` and `kind pulse` error if data is not `Bool`; SV codegen emits strategy-specific logic; sim codegen generates C++ models for all 5 kinds |
| `regfile` | ✅ | Multi-read-port / multi-write-port; `forward write_before_read`; `init [i] = v` |
| `assert` / `cover` | ❌ | Lexed but skipped at parse time |
| `pipeline` | ✅ | Stages with reg/comb/let/inst body; per-stage `stall when`; `flush` directives; explicit forwarding mux via comb if/else; `valid_r` per-stage signal; cross-stage refs (`Stage.signal`); `inst` inside stages with auto-declared output wires |
| `function` | ✅ | Pure combinational; `return expr;`; `let` bindings as temporaries; **overloading** (same name, different arg types — mangled as `Name_8`, `Name_16`, etc.); emitted as SV `function automatic` inside each module that uses it |
| `log` | ✅ | Simulation logging: `log(Level, "TAG", "fmt %0d", arg)` in `seq` and `comb` blocks; levels `Always`/`Low`/`Medium`/`High`/`Full`/`Debug`; per-module `_arch_verbosity` integer; runtime control via `+arch_verbosity=N`; emits `$display` with `[%0t][LEVEL][TAG]` prefix; **file logging**: `log file("path") (Level, ...)` — auto `$fopen`/`$fclose` in `initial`/`final` |
| `generate for/if` | ✅ | Pre-resolve elaboration pass; const/literal bounds; port + inst items |
| `ram` (multi-var store) | ⚠️ | Single store variable only; compiler-managed address layout not implemented |
| `cam` | ❌ | Not implemented |
| `crossbar` | ❌ | Not implemented |
| `scoreboard` | ❌ | Not implemented |
| `reorder_buf` | ❌ | Not implemented |
| `pqueue` | ❌ | Not implemented |
| `linklist` | ✅ | `singly`/`doubly`/`circular_singly`/`circular_doubly`; per-op FSM controllers; `insert_head`/`insert_tail`/`insert_after`/`delete_head`/`delete`/`next`/`prev`/`alloc`/`free`/`read_data`/`write_data`; doubly: `_prev_mem` updated on all insert ops; `arch sim` C++ model verified against Verilator output |
| `pipe_reg` | ✅ | `pipe_reg name: source stages N;` — N-stage flip-flop delay chain; type inferred from source signal; clock/reset from `reg default`; output is read-only; works with ports, `let` bindings, reg outputs; SV emits chained `always_ff`; sim codegen uses `_n_` temporaries for correct non-blocking semantics |
| `template` | ✅ | User-defined interface contracts; `module Name implements Template` — compiler validates required params, ports, and hooks; templates emit no SV; multi-file cross-reference supported |
| `interface` / `socket` | ❌ | TLM only; not implemented |

---

### Type System

| Feature | Status | Notes |
|---------|--------|-------|
| `UInt<N>`, `SInt<N>` | ✅ | |
| `Bool`, `Bit` | ✅ | `Bool` and `UInt<1>` are treated as identical types throughout — freely assignable to each other, bitwise ops on 1-bit operands return `Bool` |
| `Clock<Domain>` | ✅ | Domain tracked for CDC detection |
| `Reset<Sync\|Async, High\|Low>` | ✅ | Optional polarity (defaults High); Async → `posedge rst` sensitivity |
| `Vec<T, N>` | ✅ | Emits as SV unpacked array `logic [W-1:0] name [0:N-1]`; init/reset uses `'{default: val}` |
| Named types (struct/enum refs) | ✅ | |
| `Token<T, id_width>` | ❌ | TLM only |
| `Future<T>` | ❌ | TLM only |
| `$clog2(expr)` in type args | ✅ | Parsed as expression, emitted as SV `$clog2(...)`, evaluated at compile time for const-folding |
| Clock domain mismatch (CDC errors) | ✅ | Compile error when a register driven in one domain is read in another domain's `seq` block **or** when a `comb` block reads a register from one domain and its output is consumed by a `seq` block in a different domain; message directs user to `synchronizer` or async `fifo` |
| Width mismatch at assignment | ✅ | Errors for any RHS wider than LHS in both `always` and `comb` blocks; arithmetic widening (`+1`) flagged with explicit hint to use `.trunc<N>()` |
| Implicit truncation prevention | ✅ | `r <= r + 1` is a compile error; write `r <= (r + 1).trunc<N>()` explicitly. `.trunc<N>()` emits SV size cast `N'(expr)`. `.trunc<N,M>()` emits bit-range select `expr[N:M]` for field extraction (e.g. `instr.trunc<11,7>()` → `instr[11:7]`). Sim codegen applies bitmask `& ((1<<N)-1)` for sub-word types (e.g. `UInt<2>` in `uint8_t`). |

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
| Field access `.field` | ✅ |
| Array index `[i]` | ✅ |
| `.trunc<N>()` / `.trunc<N,M>()` / `.zext<N>()` / `.sext<N>()` | ✅ |
| `as` cast | ✅ |
| Struct literals | ✅ |
| Enum variants `E::Variant` | ✅ |
| `todo!` | ✅ |
| `?:` ternary | ✅ Right-associative; any expression context; chains naturally for priority muxes |
| Expression-level `match` | ✅ As `CombAssign` RHS → `case` block; as inline expression → nested ternary chain |
| `$clog2(x)` | ✅ |
| Function calls `Name(args)` | ✅ Resolved at call site; overload-resolved by argument types |

---

### Statements

| Feature | Status |
|---------|--------|
| `comb` assignment | ✅ |
| `reg` assignment `<=` | ✅ |
| `if / elsif / else` | ✅ `elsif` keyword for chained conditionals (not `else if`); resolves ambiguity in brace-free syntax |
| `match` (reg and comb blocks) | ✅ |
| Wildcard `_` → `default:` | ✅ |
| `let` bindings | ✅ `logic` local in module scope; **explicit type annotation required** (e.g. `let x: UInt<32> = ...`) — omitting the type is a compile error since bit widths are semantically meaningful |
| `wire` declarations | ✅ `wire x: T;` — combinational net with explicit type, no initializer; must be driven in a `comb` block with `=`; SV codegen emits `logic [N-1:0] x;` driven in `assign`/`always_comb`; sim codegen emits private member assigned in `eval_comb()`; type checker enforces only `wire` and output ports are valid `comb` targets (`reg` in `comb` is a compile error) |
| `log(Level, "TAG", "fmt", args...)` | ✅ In `seq` and `comb` blocks; runtime verbosity via `+arch_verbosity=N`; **file logging**: `log file("path") (Level, "TAG", "fmt", args...)` writes to file via `$fwrite`/`fprintf`; auto `$fopen` in `initial`/constructor, `$fclose` in `final`/destructor |
| `reg default: init 0 reset rst;` | ✅ Sets default `init`/`reset` for all regs in scope; individual regs may override either field |
| `{a, b, c}` bit concatenation | ✅ MSB-first; emits SV `{a, b, c}`; sim codegen shift-OR with 128-bit support |
| `{N{expr}}` bit replication | ✅ Emits SV `{N{expr}}`; nestable inside concat `{{8{sign}}, data}`; sim codegen `_arch_repeat` helper |
| `assert` / `cover` | ❌ |

---

### Type Checking

| Check | Status |
|-------|--------|
| PascalCase (types), snake_case (signals), UPPER_SNAKE (params) | ✅ |
| Duplicate definitions | ✅ |
| Undefined name references | ✅ |
| Output ports must be driven | ✅ |
| Single driver per signal | ✅ |
| `todo!` site warning | ✅ |
| Binary op result widths (IEEE 1800-2012 §11.6) | ✅ |
| Width mismatch at assignment | ✅ Any RHS wider than LHS errors in both `always` and `comb` blocks; arithmetic widening hint included |
| Clock domain crossing errors | ✅ | seq→seq and comb→seq crossings detected; extends across `inst` boundaries (compiler traces clock port connections to map child domains to parent domains) |
| Exhaustive match arm checking | ✅ Enum matches must cover all variants or include a wildcard `_`; missing variants named in error |
| Hierarchical instance references forbidden | ✅ `inst_name.port_name` in expressions is a compile error; must use `connect port -> wire_name` in the inst block instead |
| Const param evaluation (complex exprs) | ⚠️ Literals + simple arithmetic only |

---

### Tests

- 42 integration tests (snapshot + error-case), including `let` binding, `generate for`, `generate if`, mixed reset/no-reset partitioning, reset consistency validation, pipeline (simple, CPU 4-stage, instantiation, stage inst, bit-range trunc), `$clog2` in type args, function overloading, width mismatch errors, exhaustive match checking, linklist (basic singly + doubly)
- 9 Verilator simulations: Counter, TrafficLight FSM, TxQueue sync FIFO, AsyncBridge async FIFO, SimpleMem RAM, WrapCounter, BusArbiter (round-robin), IntRegs (regfile + forwarding), CpuPipe 4-stage pipeline (reset, flow, stall, flush, forwarding), BufMgr (16K×128b, 256 queues, 19 tests — multi-file split SV verified)
- 11 `arch sim` native C++ simulations verified: WrapCounter (`counter`), TrafficLight (`fsm`), Top+Counter (`module` with sub-instance), AesCipherTop (AES-128 full cipher with sub-instance + wide signals + functions), AesKeyExpand128 (key expansion with sub-instance timing), e203_exu_alu_dpath (26 tests), e203_exu_alu_bjp (25 tests — first clock-free module in test suite), linklist_basic (singly FIFO; arch sim output identical to Verilator), linklist_doubly (doubly list with next/prev/insert_after; arch sim output identical to Verilator), buf_mgr_sm (16×32b shared buffer manager; 4 queues; 17 tests), buf_mgr (16K×128b shared buffer manager; 256 queues; 2-bank free-list with prefetch; 19 tests)
- **BufMgr benchmark** (shared-memory buffer manager): 16K entries × 128-bit data pool, 256 dynamically-sharing queues, simultaneous enqueue + dequeue every cycle; all RAMs `sync_out` (2-cycle read latency); 2-bank free-list interleaving with 4-entry prefetch FIFO to sustain 1 alloc/cycle; 3-stage enqueue/dequeue pipelines with tail/head bypass forwarding; small variant (`buf_mgr_sm`, 16×32b, 4 queues, 17 tests) and full variant (`buf_mgr`, 16K×128b, 256 queues, 19 tests); exercises `ram` sim codegen with `module` hierarchical instantiation
- `arch sim` supports **multi-clock domain** modules: each `Clock<Domain>` port gets independent `_rising_X` edge detection; `eval_posedge()` guards each `seq` block on its specific clock's rising edge; auto-generates `tick()` method from domain `freq_mhz` declarations (computes half-periods via GCD for correct clock ratio); single-clock modules unchanged; verified with 200MHz/50MHz dual-clock testbench (MultiClockSync, 80 ticks, 4:1 ratio, 0 errors)
- `arch sim` supports purely combinational modules (no `Clock<>` port): generated `eval()` skips `_rising` edge detection — testbenches call `eval()` directly without toggling a clock signal
- AES-128 cipher benchmark (NIST FIPS-197 test vectors verified via `arch sim`): AesSbox + Xtime as pure combinational functions; AesCipherTop + AesKeyExpand128 using inline function calls replacing 32 `inst` blocks; wide `UInt<128>` ports via `VlWide<4>`; correct hierarchical posedge simultaneity (all `always_ff` blocks across parent + sub-instance fire atomically)
- **E203 HBirdv2 benchmark suite** (21 modules from nuclei-sw E203 RISC-V core, full core integration):
  - `e203_exu_regfile`: 2R1W register file using `regfile` construct; `init [0] = 0` write guard; `forward write_before_read: false`; 5 sim tests; verified against Verilator
  - `e203_exu_wbck`: Priority write-back arbiter (alu vs long-latency); pure `comb` block with `if/else`; 6 sim tests; verified against Verilator
  - `e203_ifu_litebpu`: Static branch prediction unit; JAL/JALR always-taken, Bxx backward-taken; JALR-x1/xN hazard detection; `rs1xn_rdrf_r` state register; `let` intermediates + async reset + `comb` `if/elsif/else`; 11 sim tests; verified against Verilator
  - `e203_exu_alu_dpath`: Shared ALU datapath; BJP/AGU/ALU operand mux; 33-bit carry-extended adder; two's-complement subtraction for comparison; `?:` ternary chaining; `SInt<32>` cast for signed comparison; `reset none` registers; 26 sim tests
  - `e203_exu_alu_bjp`: Branch/jump unit; BEQ/BNE/BLT/BGE/BLTU/BGEU; JAL/JALR unconditional jump; target address, link address (PC+4); XOR-based equality, carry-out subtraction for BLTU/BGEU, `SInt<32>` cast for BLT/BGE; purely combinational (no clock port); 25 sim tests
  - `e203_exu_alu`: ALU top-level module (6th E203 module); first to use `inst` for hierarchical composition — instantiates AluDpath + BjpUnit; 20 sim tests; verified against Verilator
  - `e203_exu_decode`: RV32IM instruction decoder (7th E203 module); pure combinational; decodes all RV32I formats (R, I, S, B, U, J) plus RV32M (MUL/MULH/MULHSU/MULHU/DIV/DIVU/REM/REMU); produces one-hot ALU/BJP/MulDiv ops, register indices/enables, sign-extended immediates, unit select, load/store flags; 30 `arch sim` tests + 22 Verilator cross-check tests
  - `e203_exu_muldiv`: Iterative multiply/divide unit (8th E203 module); RV32M MUL/MULH/MULHSU/MULHU/DIV/DIVU/REM/REMU; 32-cycle shift-add multiply, 32-cycle restoring divide; signed operand conversion + result negation; divide-by-zero handling; valid/ready handshake; written as both `module` (manual state encoding) and `fsm` (named states with `reg`/`seq` datapath extension); 24 `arch sim` tests + 12 Verilator cross-check tests; uses `elsif` for chained conditionals
  - `e203_exu_commit`: Execution commit unit (9th E203 module); 2-input priority arbiter (ALU wins over long-pipe muldiv); data mux + valid/ready handshake backpressure; pure combinational; 38 `arch sim` tests + 20 Verilator cross-check tests
  - `e203_ifu_ifetch`: Instruction fetch mini-controller (10th E203 module); FSM with datapath regs (`reg`/`seq` extension); 4 states (Idle, WaitGnt, WaitRsp, Abort); PC generation with `{a,b}` concat and `{N{expr}}` repeat for alignment; branch redirect handling; async low reset; 23 `arch sim` tests + 10 Verilator cross-check tests
  - `e203_lsu_ctrl`: Load-store unit controller (11th E203 module); byte/halfword/word access with alignment; store byte-enable and data lane shifting; load sign-extension using `{N{sign_bit}}` repeat; pure combinational; 34 `arch sim` tests + 16 Verilator cross-check tests
  - `e203_clint_timer`: CLINT timer (12th E203 module); 64-bit `mtime` counter with `mtimecmp` comparison; `{hi, lo}` concat for 64-bit assembly; APB-like register read/write with `elsif` chains; timer interrupt generation; 18 `arch sim` tests + 8 Verilator cross-check tests
  - `e203_exu_disp`: Execution dispatch unit (13th E203 module); routes decoded ops to ALU/MulDiv/LSU based on decode flags; all 8 RV32M ops passed through to MulDiv; valid/ready handshake with backpressure; pure combinational; 28 `arch sim` tests
  - `e203_exu_oitf`: Outstanding Instruction Track FIFO (16th E203 module); 2-entry circular FIFO tracking in-flight long-pipe ops; RAW/WAW hazard detection against new instruction's rs1/rs2/rd; dispatch stall on dependency; 6 `arch sim` tests
  - `e203_exu_top`: Execution unit top-level (14th E203 module); hierarchical integration of ExuDecode + ExuDisp + ExuAlu (AluDpath + BjpUnit) + ExuMuldiv + ExuOitf + ExuWbck + ExuRegfile; full RV32IM pipeline with OITF hazard stall, MulDiv long-pipe writeback through priority arbiter; 6-level deep `inst` hierarchy
  - `e203_core_top`: Core top-level integration; ties ExuTop (with MulDiv + OITF + Wbck) + IfuTop + LsuCtrl + Biu + ITCM + DTCM + ClintTimer; full RV32IM core with 21 integrated modules through 4 levels of hierarchy; 8 `arch sim` tests + 6 Verilator integration tests

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
| 4 | **Const param evaluation at instantiation** — `UInt<WIDTH*2>` with param override | Medium |
| 5 | **Function type-parametric overloads** — type parameters on functions (e.g. `function Foo<T>(a: T) -> T`) | High |

### Missing Constructs (in spec order)

| # | Construct | Complexity | What it generates |
|---|-----------|------------|-------------------|
| ~~1~~ | ~~**`$clog2(expr)` in type args**~~ | ~~Low~~ | **DONE** |
| ~~2~~ | ~~**`generate for/if`**~~ | ~~Medium~~ | **DONE** — elaboration pass expands before resolve |
| ~~3~~ | ~~**`pipeline`**~~ | ~~High~~ | **DONE** — valid/stall propagation, flush masks, explicit forwarding mux, `valid_r` gating, cross-stage refs, inst inside stages |
| ~~4~~ | ~~**`function`**~~ | ~~Medium~~ | **DONE** — pure combinational, `return`, `let` bindings, overloading by argument type; emits `function automatic` in SV |
| 5 | **`assert` / `cover`** | Low | `assert property` / `cover property` in SV |
| 6 | **`ram` multi-var store** | Medium | Compiler-managed address layout across multiple logical variables |
| 7 | **`cam`** | High | Content-addressable memory with match/miss logic |
| 8 | **`crossbar`** | High | N×M switch fabric with arbitration |
| 9 | **`scoreboard`** | High | Issue/complete tracking, hazard detection |
| 10 | **`reorder_buf`** | High | Out-of-order completion, in-order retirement |
| 11 | **`pqueue`** | High | Priority queue with enqueue/dequeue |
| ~~12~~ | ~~**`linklist`**~~ | ~~High~~ | **DONE** — singly/doubly/circular variants; all standard ops; prev-pointer maintenance; arch sim C++ model |

### CLI & Backend

| # | Feature | Notes |
|---|---------|-------|
| ~~1~~ | ~~**Multi-file compilation**~~ | **DONE** — `arch build a.arch b.arch` concatenates and cross-resolves; `arch build a.arch b.arch` without `-o` emits one `.sv` per input |
| ~~2~~ | ~~**`arch sim`**~~ | **DONE** — `arch sim Foo.arch --tb Foo_tb.cpp`; generates Verilator-compatible C++ models for `module`, `counter`, `fsm`; compiles with `g++`; runs binary; verified with counter, FSM, and top-level module testbenches |
| 3 | **`arch formal`** | Emit SMT-LIB2 for bounded model checking |
| 4 | **`interface` / `socket`** | TLM interfaces with `blocking`, `pipelined`, `out_of_order`, `burst`; `await`/`await_all`/`await_any` |
| 5 | **Waveform output** | FST/VCD compatible with GTKWave/Surfer |
