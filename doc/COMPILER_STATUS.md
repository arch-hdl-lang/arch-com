# ARCH Compiler — Status & Roadmap

> Last updated: 2026-03-20
> Compiler version: 0.15.0 (ram sim codegen; BufMgr benchmark: 16K×128b shared buffer manager with 256 queues, 2-bank free-list, prefetch FIFO; trunc bitmask fix)

---

## Implemented

### CLI

| Command | Status |
|---------|--------|
| `arch check <file.arch>` | ✅ Parse + type-check; exits 0 on success |
| `arch build <file.arch> [-o out.sv]` | ✅ Emits deterministic SystemVerilog |
| `arch build a.arch b.arch` | ✅ Multi-file: concatenates + cross-resolves; one `.sv` per input (or single combined file with `-o`) |
| `arch sim <file.arch> --tb <tb.cpp>` | ✅ Generates Verilator-compatible C++ models (`VName.h` + `VName.cpp` + `verilated.h`), compiles with `g++`, and runs; supports `module`, `counter`, `fsm`, `linklist`, `ram`; `fifo`/`arbiter`/`regfile` pending |

---

### Language Constructs

| Construct | Status | Notes |
|-----------|--------|-------|
| `domain` | ✅ | Emitted as SV comments |
| `struct` | ✅ | `typedef struct packed` |
| `enum` | ✅ | `typedef enum logic`; auto width ⌈log₂(N)⌉ |
| `module` | ✅ | Params, ports, reg/comb/let/inst body; `always on` clocked blocks with per-reg reset (`reset <signal> sync\|async high\|low` or `reset none`); compiler auto-generates reset guards; mixed reset/no-reset partitioning; `reg default: init 0 reset rst;` wildcard default for register declarations |
| `fsm` | ✅ | State enum, `always_ff` state reg, `always_comb` next-state + output; `default expr` on output ports |
| `fifo` | ✅ | Sync (extra-bit pointers) + async (gray-code CDC, auto-detected) |
| `ram` | ✅ | `single`/`simple_dual`/`true_dual`; `async`/`sync`/`sync_out`; all write modes; `init` block |
| `counter` | ✅ | `wrap`/`saturate`/`gray`/`one_hot`/`johnson` modes; `up`/`down`/`up_down`; `at_max`/`at_min` outputs |
| `arbiter` | ✅ | `round_robin`/`priority`/`lru`/`weighted`/`custom`; `ports[N]` arrays; `grant_valid`/`grant_requester` |
| `regfile` | ✅ | Multi-read-port / multi-write-port; `forward write_before_read`; `init [i] = v` |
| `assert` / `cover` | ❌ | Lexed but skipped at parse time |
| `pipeline` | ✅ | Stages with reg/comb/let/inst body; per-stage `stall when`; `flush` directives; explicit forwarding mux via comb if/else; `valid_r` per-stage signal; cross-stage refs (`Stage.signal`); `inst` inside stages with auto-declared output wires |
| `function` | ✅ | Pure combinational; `return expr;`; `let` bindings as temporaries; **overloading** (same name, different arg types — mangled as `Name_8`, `Name_16`, etc.); emitted as SV `function automatic` inside each module that uses it |
| `log` | ✅ | Simulation logging: `log(Level, "TAG", "fmt %0d", arg)` in `always` and `comb` blocks; levels `Always`/`Low`/`Medium`/`High`/`Full`/`Debug`; per-module `_arch_verbosity` integer; runtime control via `+arch_verbosity=N`; emits `$display` with `[%0t][LEVEL][TAG]` prefix; NBA semantics: value printed is last cycle's registered value |
| `generate for/if` | ✅ | Pre-resolve elaboration pass; const/literal bounds; port + inst items |
| `ram` (multi-var store) | ⚠️ | Single store variable only; compiler-managed address layout not implemented |
| `cam` | ❌ | Not implemented |
| `crossbar` | ❌ | Not implemented |
| `scoreboard` | ❌ | Not implemented |
| `reorder_buf` | ❌ | Not implemented |
| `pqueue` | ❌ | Not implemented |
| `linklist` | ✅ | `singly`/`doubly`/`circular_singly`/`circular_doubly`; per-op FSM controllers; `insert_head`/`insert_tail`/`insert_after`/`delete_head`/`delete`/`next`/`prev`/`alloc`/`free`/`read_data`/`write_data`; doubly: `_prev_mem` updated on all insert ops; `arch sim` C++ model verified against Verilator output |
| `interface` / `socket` | ❌ | TLM only; not implemented |

---

### Type System

| Feature | Status | Notes |
|---------|--------|-------|
| `UInt<N>`, `SInt<N>` | ✅ | |
| `Bool`, `Bit` | ✅ | `Bool` and `UInt<1>` are treated as identical types throughout — freely assignable to each other, bitwise ops on 1-bit operands return `Bool` |
| `Clock<Domain>` | ✅ | Domain tracked for CDC detection |
| `Reset<Sync\|Async, High\|Low>` | ✅ | Optional polarity (defaults High); Async → `posedge rst` sensitivity |
| `Vec<T, N>` | ✅ | |
| Named types (struct/enum refs) | ✅ | |
| `Token<T, id_width>` | ❌ | TLM only |
| `Future<T>` | ❌ | TLM only |
| `$clog2(expr)` in type args | ✅ | Parsed as expression, emitted as SV `$clog2(...)`, evaluated at compile time for const-folding |
| Clock domain mismatch (CDC errors) | ❌ | No cross-domain assignment checking |
| Width mismatch at assignment | ✅ | Errors for any RHS wider than LHS in both `always` and `comb` blocks; arithmetic widening (`+1`) flagged with explicit hint to use `.trunc<N>()` |
| Implicit truncation prevention | ✅ | `r <= r + 1` is a compile error; write `r <= (r + 1).trunc<N>()` explicitly. `.trunc<N>()` emits SV size cast `N'(expr)`. `.trunc<N,M>()` emits bit-range select `expr[N:M]` for field extraction (e.g. `instr.trunc<11,7>()` → `instr[11:7]`). |

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
| `if / else if / else` | ✅ |
| `match` (reg and comb blocks) | ✅ |
| Wildcard `_` → `default:` | ✅ |
| `let` bindings | ✅ `logic` local in module scope; **explicit type annotation required** (e.g. `let x: UInt<32> = ...`) — omitting the type is a compile error since bit widths are semantically meaningful |
| `log(Level, "TAG", "fmt", args...)` | ✅ In `always` and `comb` blocks; runtime verbosity via `+arch_verbosity=N` |
| `reg default: init 0 reset rst;` | ✅ Sets default `init`/`reset` for all regs in scope; individual regs may override either field |
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
| Clock domain crossing errors | ❌ |
| Exhaustive match arm checking | ✅ Enum matches must cover all variants or include a wildcard `_`; missing variants named in error |
| Const param evaluation (complex exprs) | ⚠️ Literals + simple arithmetic only |

---

### Tests

- 42 integration tests (snapshot + error-case), including `let` binding, `generate for`, `generate if`, mixed reset/no-reset partitioning, reset consistency validation, pipeline (simple, CPU 4-stage, instantiation, stage inst, bit-range trunc), `$clog2` in type args, function overloading, width mismatch errors, exhaustive match checking, linklist (basic singly + doubly)
- 8 Verilator simulations: Counter, TrafficLight FSM, TxQueue sync FIFO, AsyncBridge async FIFO, SimpleMem RAM, WrapCounter, BusArbiter (round-robin), IntRegs (regfile + forwarding), CpuPipe 4-stage pipeline (reset, flow, stall, flush, forwarding)
- 11 `arch sim` native C++ simulations verified: WrapCounter (`counter`), TrafficLight (`fsm`), Top+Counter (`module` with sub-instance), AesCipherTop (AES-128 full cipher with sub-instance + wide signals + functions), AesKeyExpand128 (key expansion with sub-instance timing), e203_exu_alu_dpath (26 tests), e203_exu_alu_bjp (25 tests — first clock-free module in test suite), linklist_basic (singly FIFO; arch sim output identical to Verilator), linklist_doubly (doubly list with next/prev/insert_after; arch sim output identical to Verilator), buf_mgr_sm (16×32b shared buffer manager; 4 queues; 17 tests), buf_mgr (16K×128b shared buffer manager; 256 queues; 2-bank free-list with prefetch; 19 tests)
- **BufMgr benchmark** (shared-memory buffer manager): 16K entries × 128-bit data pool, 256 dynamically-sharing queues, simultaneous enqueue + dequeue every cycle; all RAMs `sync_out` (2-cycle read latency); 2-bank free-list interleaving with 4-entry prefetch FIFO to sustain 1 alloc/cycle; 3-stage enqueue/dequeue pipelines with tail/head bypass forwarding; small variant (`buf_mgr_sm`, 16×32b, 4 queues, 17 tests) and full variant (`buf_mgr`, 16K×128b, 256 queues, 19 tests); exercises `ram` sim codegen with `module` hierarchical instantiation
- `arch sim` supports purely combinational modules (no `Clock<>` port): generated `eval()` skips `_rising` edge detection — testbenches call `eval()` directly without toggling a clock signal
- AES-128 cipher benchmark (NIST FIPS-197 test vectors verified via `arch sim`): AesSbox + Xtime as pure combinational functions; AesCipherTop + AesKeyExpand128 using inline function calls replacing 32 `inst` blocks; wide `UInt<128>` ports via `VlWide<4>`; correct hierarchical posedge simultaneity (all `always_ff` blocks across parent + sub-instance fire atomically)
- **E203 HBirdv2 benchmark suite** (5 modules from nuclei-sw E203 RISC-V core):
  - `e203_exu_regfile`: 2R1W register file using `regfile` construct; `init [0] = 0` write guard; `forward write_before_read: false`; 5 sim tests; verified against Verilator
  - `e203_exu_wbck`: Priority write-back arbiter (alu vs long-latency); pure `comb` block with `if/else`; 6 sim tests; verified against Verilator
  - `e203_ifu_litebpu`: Static branch prediction unit; JAL/JALR always-taken, Bxx backward-taken; JALR-x1/xN hazard detection; `rs1xn_rdrf_r` state register; `let` intermediates + async reset + `comb` `if/else if/else`; 11 sim tests; verified against Verilator
  - `e203_exu_alu_dpath`: Shared ALU datapath; BJP/AGU/ALU operand mux; 33-bit carry-extended adder; two's-complement subtraction for comparison; `?:` ternary chaining; `SInt<32>` cast for signed comparison; `reset none` registers; 26 sim tests
  - `e203_exu_alu_bjp`: Branch/jump unit; BEQ/BNE/BLT/BGE/BLTU/BGEU; JAL/JALR unconditional jump; target address, link address (PC+4); XOR-based equality, carry-out subtraction for BLTU/BGEU, `SInt<32>` cast for BLT/BGE; purely combinational (no clock port); 25 sim tests

---

### Tooling

| Tool | Status |
|------|--------|
| VSCode syntax extension | ✅ TextMate grammar (`editors/vscode/`); install: symlink to `~/.vscode/extensions/arch-hdl`; covers all keywords, types, operators, numeric literals, comments |
| Vim syntax | ✅ `editors/vim/syntax/arch.vim` |

---

## Remaining Features

### Correctness Gaps (no new constructs needed)

| # | Feature | Effort |
|---|---------|--------|
| ~~1~~ | ~~**Width mismatch at assignment**~~ | **DONE** — any width delta errors in `always` and `comb` |
| ~~2~~ | ~~**Exhaustive `match` checking**~~ | **DONE** — missing variants named in error; wildcard `_` suppresses |
| 3 | **CDC error detection** — cross-domain signal assignment → compile error | Medium |
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
