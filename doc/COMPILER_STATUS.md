# ARCH Compiler — Status & Roadmap

> Last updated: 2026-03-15
> Compiler version: 0.8.0 (pipeline construct, trunc<N,M> bit-range, inst inside stages)

---

## Implemented

### CLI

| Command | Status |
|---------|--------|
| `arch check <file.arch>` | ✅ Parse + type-check; exits 0 on success |
| `arch build <file.arch> [-o out.sv]` | ✅ Emits deterministic SystemVerilog |

Single-file compilation only.

---

### Language Constructs

| Construct | Status | Notes |
|-----------|--------|-------|
| `domain` | ✅ | Emitted as SV comments |
| `struct` | ✅ | `typedef struct packed` |
| `enum` | ✅ | `typedef enum logic`; auto width ⌈log₂(N)⌉ |
| `module` | ✅ | Params, ports, reg/comb/let/inst body; `always on` clocked blocks with per-reg reset (`reset <signal> sync\|async high\|low` or `reset none`); compiler auto-generates reset guards; mixed reset/no-reset partitioning |
| `fsm` | ✅ | State enum, `always_ff` state reg, `always_comb` next-state + output; `default expr` on output ports |
| `fifo` | ✅ | Sync (extra-bit pointers) + async (gray-code CDC, auto-detected) |
| `ram` | ✅ | `single`/`simple_dual`/`true_dual`; `async`/`sync`/`sync_out`; all write modes; `init` block |
| `counter` | ✅ | `wrap`/`saturate`/`gray`/`one_hot`/`johnson` modes; `up`/`down`/`up_down`; `at_max`/`at_min` outputs |
| `arbiter` | ✅ | `round_robin`/`priority`/`lru`/`weighted`/`custom`; `ports[N]` arrays; `grant_valid`/`grant_requester` |
| `regfile` | ✅ | Multi-read-port / multi-write-port; `forward write_before_read`; `init [i] = v` |
| `assert` / `cover` | ❌ | Lexed but skipped at parse time |
| `pipeline` | ✅ | Stages with reg/comb/let/inst body; per-stage `stall when`; `flush` directives; explicit forwarding mux via comb if/else; `valid_r` per-stage signal; cross-stage refs (`Stage.signal`); `inst` inside stages with auto-declared output wires |
| `generate for/if` | ✅ | Pre-resolve elaboration pass; const/literal bounds; port + inst items |
| `ram` (multi-var store) | ⚠️ | Single store variable only; compiler-managed address layout not implemented |
| `cam` | ❌ | Not implemented |
| `crossbar` | ❌ | Not implemented |
| `scoreboard` | ❌ | Not implemented |
| `reorder_buf` | ❌ | Not implemented |
| `pqueue` | ❌ | Not implemented |
| `linklist` | ❌ | Not implemented |
| `interface` / `socket` | ❌ | TLM only; not implemented |

---

### Type System

| Feature | Status | Notes |
|---------|--------|-------|
| `UInt<N>`, `SInt<N>` | ✅ | |
| `Bool`, `Bit` | ✅ | |
| `Clock<Domain>` | ✅ | Domain tracked for CDC detection |
| `Reset<Sync\|Async, High\|Low>` | ✅ | Optional polarity (defaults High); Async → `posedge rst` sensitivity |
| `Vec<T, N>` | ✅ | |
| Named types (struct/enum refs) | ✅ | |
| `Token<T, id_width>` | ❌ | TLM only |
| `Future<T>` | ❌ | TLM only |
| `$clog2(expr)` in type args | ❌ | Lexer has no `$` token; users write explicit widths |
| Clock domain mismatch (CDC errors) | ❌ | No cross-domain assignment checking |
| Width mismatch at assignment | ⚠️ | Errors when reg assignment RHS is exactly 1 bit wider than LHS due to arithmetic widening; full width-error checking (arbitrary width delta) not yet implemented |
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
| Expression-level `match` | ✅ As `CombAssign` RHS → `case` block; as inline expression → nested ternary chain |
| `$clog2(x)` / `$bytes(x)` system calls | ❌ |

---

### Statements

| Feature | Status |
|---------|--------|
| `comb` assignment | ✅ |
| `reg` assignment `<=` | ✅ |
| `if / else` | ✅ |
| `match` (reg and comb blocks) | ✅ |
| Wildcard `_` → `default:` | ✅ |
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
| Width mismatch at assignment | ⚠️ Reg assignments error when RHS is exactly 1 bit wider (arithmetic widening); full width-checking (arbitrary delta) not yet implemented |
| Clock domain crossing errors | ❌ |
| Exhaustive match arm checking | ❌ |
| Const param evaluation (complex exprs) | ⚠️ Literals + simple arithmetic only |

---

### Tests

- 37 integration tests (snapshot + error-case), including `let` binding, `generate for`, `generate if`, mixed reset/no-reset partitioning, reset consistency validation, pipeline (simple, CPU 4-stage, instantiation, stage inst, bit-range trunc)
- 8 Verilator simulations: Counter, TrafficLight FSM, TxQueue sync FIFO, AsyncBridge async FIFO, SimpleMem RAM, WrapCounter, BusArbiter (round-robin), IntRegs (regfile + forwarding), CpuPipe 4-stage pipeline (reset, flow, stall, flush, forwarding)

---

## Remaining Features

### Correctness Gaps (no new constructs needed)

| # | Feature | Effort |
|---|---------|--------|
| 1 | **Width mismatch at assignment** — `UInt<16>` → `UInt<8>` should error | Low |
| 2 | **Exhaustive `match` checking** — enum match must cover all variants or have `_` | Low |
| 3 | **Expression-level `match` codegen** — currently emits `'0` stub | Medium |
| 4 | **`$clog2(expr)` in type args** — add `$`-prefixed system calls to lexer/parser | Low |
| 5 | **CDC error detection** — cross-domain signal assignment → compile error | Medium |
| 6 | **Const param evaluation at instantiation** — `UInt<WIDTH*2>` with param override | Medium |

### Missing Constructs (in spec order)

| # | Construct | Complexity | What it generates |
|---|-----------|------------|-------------------|
| 7 | **`assert` / `cover`** | Low | `assert property` / `cover property` in SV |
| 8 | ~~**`generate for/if`**~~ | ~~Medium~~ | **DONE** — elaboration pass expands before resolve |
| 9 | ~~**`pipeline`**~~ | ~~High~~ | **DONE** — valid/stall propagation, flush masks, explicit forwarding mux, `valid_r` gating, cross-stage refs, inst inside stages |
| 12 | **`ram` multi-var store** | Medium | Compiler-managed address layout across multiple logical variables |
| 13 | **`cam`** | High | Content-addressable memory with match/miss logic |
| 14 | **`crossbar`** | High | N×M switch fabric with arbitration |
| 15 | **`scoreboard`** | High | Issue/complete tracking, hazard detection |
| 16 | **`reorder_buf`** | High | Out-of-order completion, in-order retirement |
| 18 | **`pqueue`** | High | Priority queue with enqueue/dequeue |
| 19 | **`linklist`** | High | Linked-list manager |

### CLI & Backend

| # | Feature | Notes |
|---|---------|-------|
| 20 | **`arch sim`** | TLM simulation: `--tlm-lt`, `--tlm-at`, `--tlm-rtl`; `--wave out.fst` waveform output |
| 21 | **`arch formal`** | Emit SMT-LIB2 for bounded model checking |
| 22 | **Multi-file compilation** | Cross-file type/module resolution |
| 23 | **`interface` / `socket`** | TLM interfaces with `blocking`, `pipelined`, `out_of_order`, `burst`; `await`/`await_all`/`await_any` |
| 24 | **Waveform output** | FST/VCD compatible with GTKWave/Surfer |
