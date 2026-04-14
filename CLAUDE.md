# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`arch-com` is a compiler for **ARCH**, a purpose-built hardware description language (HDL) for micro-architecture work. The compiler ingests `.arch` source files and emits deterministic, readable SystemVerilog. The language is explicitly designed to be generated correctly by LLMs from natural-language hardware descriptions.

Full specification: `doc/ARCH_HDL_Specification.docx`
Compact AI reference: `doc/Arch_AI_Reference_Card.docx`

---

## Target CLI (from spec)

The compiler binary is `arch`. MVP commands:

```
arch check F.arch          # type-check only (no output)
arch sim Tb.arch           # simulate (single core)
arch sim --parallel Tb.arch
arch sim --tlm-lt          # max speed, no timing
arch sim --tlm-at          # ns-accurate AT timing
arch sim --tlm-rtl         # full signal fidelity
arch sim --wave out.fst    # emit waveform (GTKWave/Surfer)
arch build F.arch          # emit SystemVerilog
arch formal F.arch         # emit SMT-LIB2
```

---

## ARCH Language — Key Constructs

### Universal Block Grammar
Every construct uses **keyword Name / ... / end keyword Name** — no braces anywhere. Named endings are mandatory and must match the opening keyword and name exactly.

### First-Class Constructs
| Keyword | Purpose |
|---------|---------|
| `module` | Combinational or registered logic |
| `pipeline` | Staged datapath — compiler generates hazard logic (valid/stall propagation, flush masks, forward muxes) |
| `fsm` | Finite state machine — compiler checks exhaustive transitions; `reset_state` required |
| `fifo` | Sync or dual-clock async FIFO — two different `Clock<D>` ports auto-triggers gray-code CDC generation |
| `ram` | FPGA BRAM / ASIC SRAM / ROM — `kind: single|simple_dual|true_dual|rom`, `latency 0|1|2` (async/sync/sync_out). ROM: read-only, requires `init: [...]` or `init: file("path", hex|bin);` |
| `arbiter` | N-requester grant logic — `policy: round_robin|priority|weighted<W>|lru|custom` |
| `bus` | Reusable parameterized port bundle — `initiator` keeps signal directions, `target` flips them; flattens to individual SV ports (`axi.aw_valid` → `axi_aw_valid`) |
| `thread` | Multi-cycle sequential block — `wait until`/`wait N cycle`/`fork`-`join`; compiler lowers to synthesizable FSM; use instead of manual `fsm` for sequential protocols |
| `generate` | Compile-time port/instance generation (`for` and `if` variants) |

### Universal Block Schema (all constructs follow this layout)
```
keyword Name
  param NAME: const = value;
  param NAME: type = SomeType;
  port name: in TypeExpr;
  port name: out TypeExpr;
  socket name: initiator InterfaceName;   // TLM
  socket name: target InterfaceName;      // TLM
  generate for i in 0..N-1 ... end generate for i
  generate if PARAM > 0 ... end generate if
  assert name: expression;
  cover name: expression;
end keyword Name
```

### Signal Declarations and Assignment

Arch has three kinds of module-scope signal declarations:

| Construct | Syntax | Assigned in | SV equivalent |
|-----------|--------|-------------|---------------|
| `let` | `let x: T = expr;` | declaration (fixed combinational expr) | `logic [W-1:0] x; assign x = expr;` |
| `wire` | `wire x: T;` | `comb` block (`=`) | `logic [W-1:0] x;` (driven in `assign`/`always_comb`) |
| `reg` | `reg x: T [init V] [reset R => V];` | `seq` block (`<=`) | `logic [W-1:0] x [= V];` (driven in `always_ff`) |

- `comb y = expr; end comb` — combinational, uses `=`. Valid targets are `wire` declarations, output ports, and indexed expressions (`out[i]`); assigning to a `reg` in `comb` is a compile error.
- `wire x: T;` — declares a combinational net driven inside a `comb` block. No initializer. SV codegen emits `logic [N-1:0] x;`. Sim codegen treats it as a private member in `eval_comb()`.
- `reg r: T reset rst => 0;` + `seq on clk rising ... end seq` — registered, uses `<=`. Reset value is specified after `=>` in the reset clause. `init` is optional (SV declaration initializer only). Reset is declared per register; compiler auto-generates reset guards.
- `for i in 0..N ... end for` — loop in `comb` or `seq` blocks; range is inclusive; emits SV `for` loop.
- No implicit latches (error). Single driver per signal (error). All ports must be connected.
- **Output timing:** `port reg o: out T` (driven in `seq` with `<=`) adds 1-cycle latency — output reflects state from the previous clock edge. `port o: out T` (driven in `comb` with `=` or via `let`) is combinational — output reflects current state same cycle. For FSM outputs where testbenches expect immediate (same-cycle) response to state changes, use plain `port` + `comb`, not `port reg`.

### Type System
- **Primitive types:** `UInt<N>`, `SInt<N>`, `Bool`, `Bit`, `Clock<Domain>`, `Reset<Sync|Async, High|Low>` (polarity defaults High), `Vec<T,N>`, `struct`, `enum`, `Token`, `Future<T>`, `Token<T, id_width: N>`
- **No implicit conversions.** All width casts are explicit: `.trunc<N>()`, `.zext<N>()`, `.sext<N>()`. Same-width signedness reinterpret: `signed(x)`, `unsigned(x)`
- Arithmetic result widths follow IEEE 1800-2012 §11.6 (e.g. `UInt<8> + UInt<8>` → `UInt<9>`)
- **Wrapping operators** `+%`, `-%`, `*%` give result width = `max(W(a), W(b))` (no widening); prefer over `.trunc<N>()` for modular arithmetic: `let x: UInt<8> = a +% b;`
- Clock domain mismatches are **compile errors**, not warnings

### Naming Conventions (recommended, not compiler-enforced)
| Category | Convention | Example |
|---|---|---|
| Modules, interfaces, structs, enums | PascalCase | `FetchUnit`, `AluOp` |
| Signals, registers, ports, locals | snake_case | `pc_next`, `req_valid` |
| Parameters and constants | UPPER_SNAKE | `XLEN`, `CACHE_DEPTH` |
| Clock ports | `Clock<Domain>` | `clk: in Clock<SysDomain>` |
| Reset ports | `Reset<Sync\|Async, High\|Low>` | `rst: in Reset<Sync>` (High default) |

### `todo!` Escape Hatch
Any expression or block body may be replaced with `todo!` to produce a compilable, type-checked skeleton. The compiler emits a warning per site; simulation aborts if a `todo!` site is reached at runtime.

### TLM Concurrency Modes
| Mode | Return | Use case |
|---|---|---|
| `blocking` | `ret: T` | Caller suspends — APB/MMIO |
| `pipelined` | `ret: Future<T>` | Issue many, await later — AXI in-order |
| `out_of_order` | `ret: Token<T, id: N>` | Any-order response by ID — Full AXI |
| `burst` | `ret: Future<Vec<T,L>>` | One AR, N data beats — AXI INCR |

`await f`, `await_all(f0,f1,f2)`, `await_any(t0,t1)` for synchronization.

---

## Compiler Architecture (to build)

The compiler pipeline should follow a classical structure:

1. **Lexer/Parser** — tokenize `.arch` source into an AST. The grammar is regular: `keyword Name`, params, ports, bodies, `end keyword Name`.
2. **Type checker** — enforce bit-width safety, clock domain tracking, naming conventions, single-driver, all-ports-connected, exhaustive FSM transitions.
3. **IR / elaboration** — expand `generate` constructs, resolve params, instantiate modules.
4. **Backend: SystemVerilog emitter** (`arch build`) — one Arch construct → one deterministic SV structure.
5. **Backend: SMT-LIB2 emitter** (`arch formal`) — for formal verification.
6. **Simulator** (`arch sim`) — TLM modes: `--tlm-lt`, `--tlm-at`, `--tlm-rtl`; waveform output via `--wave`.

Special compiler responsibilities:
- Auto-detect dual-clock FIFOs and insert gray-code pointer synchronization.
- Generate pipeline hazard logic (stall propagation, flush masks, forwarding muxes) from `stall when`, `flush`, and `forward` directives.
- Auto-select minimum-width encoding for `enum` types.
- Emit warnings for every `todo!` site; abort simulation if one is reached.
