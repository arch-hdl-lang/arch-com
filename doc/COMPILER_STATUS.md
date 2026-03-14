# ARCH Compiler — Status & Roadmap

> Last updated: 2026-03-13
> Compiler version: 0.4.0 (FSM + FIFO + RAM + Counter + Arbiter + Regfile)

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
| `module` | ✅ | Params, ports, reg/comb/let/inst body |
| `fsm` | ✅ | State enum, `always_ff` state reg, `always_comb` next-state + output |
| `fifo` | ✅ | Sync (extra-bit pointers) + async (gray-code CDC, auto-detected) |
| `ram` | ✅ | `single`/`simple_dual`/`true_dual`; `async`/`sync`/`sync_out`; all write modes; `init` block |
| `counter` | ✅ | `wrap`/`saturate`/`gray`/`one_hot`/`johnson` modes; `up`/`down`/`up_down`; `at_max`/`at_min` outputs |
| `arbiter` | ✅ | `round_robin`/`priority`/`lru`/`weighted`/`custom`; `ports[N]` arrays; `grant_valid`/`grant_requester` |
| `regfile` | ✅ | Multi-read-port / multi-write-port; `forward write_before_read`; `init [i] = v` |
| `assert` / `cover` | ❌ | Lexed but skipped at parse time |
| `pipeline` | ❌ | Not implemented |
| `generate for/if` | ❌ | Not implemented |
| `ram` (multi-var store) | ⚠️ | Single store variable only; compiler-managed address layout not implemented |
| `cam` | ❌ | Not implemented |
| `crossbar` | ❌ | Not implemented |
| `scoreboard` | ❌ | Not implemented |
| `reorder_buf` | ❌ | Not implemented |
| `counter` | ❌ | Not implemented |
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
| `Reset<Sync\|Async>` | ✅ | Async → `posedge rst` sensitivity |
| `Vec<T, N>` | ✅ | |
| Named types (struct/enum refs) | ✅ | |
| `Token<T, id_width>` | ❌ | TLM only |
| `Future<T>` | ❌ | TLM only |
| `$clog2(expr)` in type args | ❌ | Lexer has no `$` token; users write explicit widths |
| Clock domain mismatch (CDC errors) | ❌ | No cross-domain assignment checking |
| Width mismatch at assignment | ❌ | Silently passes |
| Implicit truncation prevention | ❌ | |

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
| `.trunc<N>()` / `.zext<N>()` / `.sext<N>()` | ✅ |
| `as` cast | ✅ |
| Struct literals | ✅ |
| Enum variants `E::Variant` | ✅ |
| `todo!` | ✅ |
| Expression-level `match` | ⚠️ Parsed; emits `'0` stub |
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
| Width mismatch at assignment | ❌ |
| Clock domain crossing errors | ❌ |
| Exhaustive match arm checking | ❌ |
| Const param evaluation (complex exprs) | ⚠️ Literals + simple arithmetic only |

---

### Tests

- 14 integration tests (snapshot + error-case)
- 7 Verilator simulations: Counter, TrafficLight FSM, TxQueue sync FIFO, AsyncBridge async FIFO, SimpleMem RAM, WrapCounter, BusArbiter (round-robin), IntRegs (regfile + forwarding)

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
| 8 | **`generate for/if`** | Medium | Unrolled port/instance arrays; compile-time conditional blocks |
| 9 | **`pipeline`** | High | Valid/stall propagation, flush masks, forwarding muxes — auto-generated from `stall when`, `flush`, `forward` directives |
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
