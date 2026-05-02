# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commit conventions

**Do not add `Co-Authored-By:` trailers for AI agents** (Claude, Copilot, etc.)
when creating commits. The human author of record takes full ownership of the
change, and AI co-author trailers break the CLA Assistant (which requires every
listed author to individually sign the CLA — `noreply@anthropic.com` can't).
Just write a normal commit message with no AI attribution trailer.

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
arch sim Tb.arch --thread-sim parallel
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

### Runtime bounds checking (always on)

Out-of-range access is a hard-abort in arch sim, no flag needed. Three cases:

- `Vec<T, N>` indexing: `vec[i]` with `i >= N` → `ARCH-ERROR: <vec>: index N out of bounds [0..N)` + `abort()`.
- Bit-select on scalar: `val[i]` on `UInt<W>`/`SInt<W>` with `i >= W` → same pattern.
- Variable part-select: `val[start +: W]` with `start + W > W_base`, or `val[start -: W]` with `start < W-1` or `start >= W_base` → same.

Compile-time constant indices are verified by the type checker, so only runtime indices carry the check. The abort is unconditional (not flag-gated) because out-of-bounds is an error, not a lint.

**SV-level mirror.** `arch build` also auto-emits concurrent SVA bounds assertions for the same access sites (inside `synopsys translate_off/on`), labeled `_auto_bound_<kind>_<n>`. Format:

```sv
_auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) (idx) < (4))
  else $fatal(1, "BOUNDS VIOLATION: Mod._auto_bound_vec_0");
```

Consumed by Verilator (`--assert`), iverilog (`-gsupported-assertions`), and formal tools (EBMC, SymbiYosys). **Scope**: seq and latch contexts only. Accesses in `comb` blocks or `let` bindings are not mirrored to SV in v1 — concurrent assertions can't catch sub-cycle glitches, and wrapping `always_comb` with immediate assertions is deferred. The arch-sim runtime check still fires for those paths. Reset polarity is inferred from the module's `Reset<Kind,Polarity>` port. Modules with no clock emit no SV assertion (assertion would have no evaluation context).

**Verified end-to-end (2026-04-17):**
- *Verilator 5.034 + `--assert`*: in-bounds runs silently; OOB trips `$fatal(1, "BOUNDS VIOLATION: ...")`.
- *EBMC 5.11*: `UInt<4> wr_idx` into `Vec<_,4>` ⇒ **REFUTED** at the leaf module (unconstrained input can reach 15; caller must constrain). Same access with `UInt<2> wr_idx` ⇒ **PROVED up to bound 10** (structurally safe by type width). Confirms the SVA is syntactically correct, solver-reducible, and actionable.

### Runtime divide-by-zero checking

`/` and `%` are runtime-checked whenever the divisor is **not** a compile-time-reducible constant (literals, const-param references, folded arithmetic are treated as constants).

- **Compile-time gate** (`arch check`): if any constant expression — param default, const let initializer — reduces to `A / 0` or `A % 0`, the compiler refuses with `divide by zero in constant expression: divisor evaluates to 0`, pointing at the offending divisor.
- **Runtime abort in arch sim**: non-const divisor ⇒ wrap in `_ARCH_DCHK(divisor, "loc")`. On zero, `ARCH-ERROR: <loc>: division by zero` + `abort()`.
- **SV assertion mirror**: seq/latch contexts emit `_auto_div0_div0_<n>: assert property (@(posedge clk) disable iff (rst) (divisor) != 0) else $fatal(1, "DIV-BY-ZERO VIOLATION: ...")`, wrapped in `translate_off/on`. Comb/let contexts are not mirrored in v1 — arch sim's runtime check still fires for those.
- **Constant-divisor elision**: `num / N` where `N: const = 4` produces zero runtime check and zero SVA — the type checker already proved it safe.

**Verified end-to-end (2026-04-17):**
- *arch sim*: `num=100, den=4` prints `25`; `den=0` aborts with `ARCH-ERROR: den /: division by zero`.
- *Verilator 5.034 `--assert`*: in-bounds runs; `den=0` trips `$fatal(1, "DIV-BY-ZERO VIOLATION: DivRt._auto_div0_div0_0")`.
- *EBMC 5.11*: unconstrained `den: UInt<8>` ⇒ **REFUTED** (caller must gate). With `den_safe = den_raw | 1` ⇒ **PROVED up to bound 5** (divisor is structurally odd, hence non-zero).

### Auto-emitted thread spec-contract SVA (`--auto-thread-asserts`)

Off-by-default flag on `arch build`, `arch sim`, `arch formal`. When set, `lower_threads` emits SVA properties anchored to the lowered state register and counter, capturing the thread's source-level intent that the lowered comb+seq blob has otherwise lost. Wrapped in `synopsys translate_off/on`. Reset polarity is honored — active-low → bare `rst` as the not-in-reset guard, active-high → `!rst`.

| Source construct | Property class | SVA shape |
|---|---|---|
| `wait until <cond>` | `_auto_thread_t{i}_wait_until_s{si}` | `(rst_inactive && state==si && cond) \|=> state==next` |
| `wait <N> cycle` | `_auto_thread_t{i}_wait_stay_s{si}` + `_..._wait_done_s{si}` | stay (`cnt!=0` ⇒ stay) and done (`cnt==0` ⇒ advance) |
| `fork`/`join` branches | `_auto_thread_t{i}_branch_s{si}_b{bi}` | per-(cond, target) `(rst_inactive && state==si && cond) \|=> state==target` |

Thread-level (not FSM-level) auto-gen: once lowered, the source structure is gone — no downstream pass can recover that a transition came from `wait until` vs an arbitrary user-condition. Reachability/state-range cover are out of scope here; they belong with FSM-construct auto-gen and lowered threads will inherit them.

Skipped for terminal once-states (`thread once` last state — vacuous) and threads with `default_when` (the soft-reset escape can preempt any state).

**Verified end-to-end (2026-04-26):** `arch build wait_cycles.arch --auto-thread-asserts | verilator --binary --assert` runs silently for 24 cycles across ~5 thread-state-machine loops; mutating the `wait_until` consequent (`state==1` → `state==7`) trips `$fatal(1, "ASSERTION FAILED: _auto_thread_t0_wait_until_s0")` mid-sim.

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
  port name: in unpacked Vec<T, N>;       // SV unpacked-array port (interop hatch)
  port name: initiator BusName;           // bus / TLM method initiator
  port name: target BusName;              // bus / TLM method target
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
- **Primitive types:** `UInt<N>`, `SInt<N>`, `Bool`, `Bit`, `Clock<Domain>`, `Reset<Sync|Async, High|Low>` (polarity defaults High), `Vec<T,N>`, `struct`, `enum`. There is no current `Future<T>` / `await` / user-visible `Token<T>` API.
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

### TLM Method Support

Current syntax lives directly inside `bus`:

```arch
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
  tlm_method read_ooo(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
end bus Mem
```

Initiators call as a direct RHS inside a thread: `d <= m.read(addr);`. Targets implement with a dotted-name thread: `thread s.read(addr) on clk rising, rst high ... return data; end thread s.read`.

Concurrency is structural: multiple direct worker threads, `generate_for` workers, or one direct-call `fork ... and ... join` thread lower to a request arbiter and response router. Blocking routes by issue-order FIFO; `out_of_order tags N` routes by hidden tag wires. Do not generate `Future<T>`, `await`, `Token<T>`, `pipelined`, or `burst` syntax.

---

## Compiler Architecture (to build)

The compiler pipeline should follow a classical structure:

1. **Lexer/Parser** — tokenize `.arch` source into an AST. The grammar is regular: `keyword Name`, params, ports, bodies, `end keyword Name`.
2. **Type checker** — enforce bit-width safety, clock domain tracking, naming conventions, single-driver, all-ports-connected, exhaustive FSM transitions.
3. **IR / elaboration** — expand `generate` constructs, resolve params, instantiate modules.
4. **Backend: SystemVerilog emitter** (`arch build`) — one Arch construct → one deterministic SV structure.
5. **Backend: SMT-LIB2 emitter** (`arch formal`) — for formal verification.
6. **Simulator** (`arch sim`) — default C++ sim, optional `--thread-sim parallel`, waveform output via `--wave`. TLM-lowered modules use the regular reg/seq/comb sim mirror; ordinary non-TLM thread modules can use the coroutine parallel thread emitter. Sim emitter lives under `src/sim_codegen/` — `mod.rs` holds shared helpers + `gen_module`; per-construct emitters (`gen_fsm`, `gen_pipeline`, …) live in sibling submodule files and extend `impl SimCodegen` via `pub(super) fn`.

Special compiler responsibilities:
- Auto-detect dual-clock FIFOs and insert gray-code pointer synchronization.
- Generate pipeline hazard logic (stall propagation, flush masks, forwarding muxes) from `stall when`, `flush`, and `forward` directives.
- Auto-select minimum-width encoding for `enum` types.
- Emit warnings for every `todo!` site; abort simulation if one is reached.
