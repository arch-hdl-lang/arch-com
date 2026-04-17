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

## Debugging Simulation Failures

**IMPORTANT: Use `--debug` instead of manually adding printf/`$display` to testbenches.**

When a simulation test fails or produces unexpected results, use the built-in debug instrumentation:

```bash
# Print all I/O port changes (top module only)
arch sim --debug Module.arch --tb tb.cpp

# Also print sub-module port changes (2 levels deep)
arch sim --debug --depth 2 Module.arch Sub.arch --tb tb.cpp

# Also print FSM state transitions with triggering conditions
arch sim --debug+fsm Module.arch --tb tb.cpp

# Combine: ports + FSM + 2 levels
arch sim --debug+fsm --depth 2 Module.arch Sub.arch --tb tb.cpp
```

The debug output covers:
- **Scalar ports**: `[cycle][Mod.port](in/out) 0x0 -> 0x1`
- **Vec ports**: `[cycle][Mod.data[2]](out) 0x0 -> 0xff` (per-element)
- **Bus ports**: `[cycle][Mod.s_axil_aw_valid](in) 0x0 -> 0x1` (with correct direction)
- **Wide ports (>64b)**: hex dump of all 32-bit words
- **FSM transitions**: `[FSM][Mod] IDLE -> RUN (start && ready)` (with condition)
- **Reset events**: `[cycle][Mod.rst](in) 0x0 -> 0x1`

Do NOT add `printf` or `$display` to C++ testbenches for debugging — `--debug` provides the same information automatically with zero code changes. Only add manual prints for test-specific protocol checking (e.g., verifying handshake sequences).

### Running Python/cocotb-style testbenches (`--pybind --test`)

`arch sim --pybind --test test_foo.py Foo.arch` compiles the ARCH design through pybind11 and runs a cocotb-compatible Python TB against it, with no Verilator/iverilog/VPI in the loop. A `cocotb_shim/cocotb/` package is on `PYTHONPATH` so plain `import cocotb` works. Supported surface: `@cocotb.test()`, `cocotb.start_soon`, `cocotb.start`, `cocotb.utils.get_sim_time`, and triggers `RisingEdge` / `FallingEdge` / `Timer` / `ClockCycles` / `Clock`. Signals expose `.value` that read/write as integers via `ArchSignalValue`.

Key behavioral deltas from real cocotb — scheduler is 1-tick-at-a-time (default 1 ns/tick), writes take effect immediately on the next `eval()` (no NBA region), logic is 2-state (no `X`/`Z`), edge detection is sampled per tick. See **[`doc/arch_sim_cocotb.md`](doc/arch_sim_cocotb.md)** for the complete API, portability rules, and a troubleshooting guide.

### Catching X-propagation from undriven inputs (`--inputs-start-uninit`)

When porting an SV design to ARCH, it's easy for a testbench to forget to drive a port. Under 4-state sim (Verilator/iverilog) this shows up as X-propagation; ARCH's native sim is 2-state so the bug is invisible unless you opt in:

```bash
arch sim --inputs-start-uninit Module.arch --tb tb.cpp
```

This implies `--check-uninit`. Every scalar input port starts in an "uninitialized" state; a TB opts each port in by calling the generated setter:

```cpp
dut.set_port_name(value);   // marks the port as driven
dut.port_name = value;      // does NOT mark it — will warn on read
```

If the design reads an input that the TB never drove (in a comb block, let binding, seq block, or latch), you get:

```
WARNING: read of uninitialized input 'port_name' — TB never called set_port_name()
```

Clock and Reset input ports are excluded (they're driven by the test harness lifecycle, not by the setter API). Bus ports and Vec ports are also excluded in v1.

A sibling flag `--check-uninit-ram` does the same for RAM cells — per-cell valid bitmap, `init:` cells pre-marked, warns once per RAM on the first read of an address that was never written. ROMs are exempt (they require `init:` at compile time). Both `--inputs-start-uninit` and `--check-uninit-ram` imply `--check-uninit`.

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
- **`guard` clause** — `reg data: UInt<32> guard valid_sig;` marks a register as intentionally `reset none` but valid-gated by `valid_sig`. Clause goes right after the type, before any `init`/`reset`. `valid_sig` is a single identifier (combine predicates via `let` if needed). Effect: `--check-uninit` stays silent at the consumer read site (consumers qualify with `if valid_sig`), but still fires if `valid_sig` ever asserts while the data reg was never written — the producer bug this annotation is designed to catch. Canonical use: wide AXI/FIFO data paths where resetting the data would waste area and power. `port reg` supports the same clause.
- `for i in 0..N ... end for` — loop in `comb` or `seq` blocks; range is inclusive; emits SV `for` loop.
- No implicit latches (error). Single driver per signal (error). All ports must be connected.
- **Output timing:** `port reg o: out T` (driven in `seq` with `<=`) adds 1-cycle latency — output reflects state from the previous clock edge. `port o: out T` (driven in `comb` with `=` or via `let`) is combinational — output reflects current state same cycle. For FSM outputs where testbenches expect immediate (same-cycle) response to state changes, use plain `port` + `comb`, not `port reg`.
- **Built-in functions:** `onehot(index)` → one-hot decode (`1 << index`), width inferred from context. `$clog2(expr)` → ceiling log2. `signed(expr)` / `unsigned(expr)` → same-width reinterpret.
- **Module-local functions:** `function name(args) -> RetType ... end function name` can be declared inside a module body. Emits SV `function automatic` inside the module block. Supports `let`, `return`, `if/elsif/else`, `for` loops, and assignment (`=`). **No-latch rule:** every code path must reach a `return`.
- **Separate compilation:** `arch build` emits `.archi` interface files alongside `.sv`. When `inst sub: SubModule` references an undefined module, the compiler auto-discovers `SubModule.archi` in the input directory or `ARCH_LIB_PATH`.

### Type System
- **Primitive types:** `UInt<N>`, `SInt<N>`, `Bool`, `Bit`, `Clock<Domain>`, `Reset<Sync|Async, High|Low>` (polarity defaults High), `Vec<T,N>`, `struct`, `enum`, `Token`, `Future<T>`, `Token<T, id_width: N>`
- **No implicit conversions.** All width casts are explicit: `.trunc<N>()`, `.zext<N>()`, `.sext<N>()`. Same-width signedness reinterpret: `signed(x)`, `unsigned(x)`
- Arithmetic result widths follow IEEE 1800-2012 §11.6 (e.g. `UInt<8> + UInt<8>` → `UInt<9>`)
- **Wrapping operators** `+%`, `-%`, `*%` give result width = `max(W(a), W(b))` (no widening); prefer over `.trunc<N>()` for modular arithmetic: `let x: UInt<8> = a +% b;`
- Clock domain mismatches are **compile errors**, not warnings

### Operator Precedence (compiler-enforced parens)

`arch check` rejects four common precedence foot-guns. Always parenthesize when mixing these operator classes:

| Mixing | Bad | Good |
|---|---|---|
| Bitwise + comparison | `a & mask == 0` | `(a & mask) == 0` |
| Bitwise + logical | `a and b & c` | `a and (b & c)` |
| Shift + arithmetic | `1 << bit + 1` | `1 << (bit + 1)` |
| Ternary branch with binary | `en ? a : b + 1` | `en ? a : (b + 1)` or `(en ? a : b) + 1` |

The parse in the "Bad" column is what Verilog/ARCH produces, but rarely what the user means. Use parens even when the parse happens to be correct — it makes intent explicit.

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
