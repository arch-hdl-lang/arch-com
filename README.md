# arch-com

A compiler for the **ARCH** hardware description language — ingests `.arch` source files and emits deterministic, readable SystemVerilog.

**Paper**: [arXiv:2604.05983](https://arxiv.org/abs/2604.05983)

## Why a new HDL?

SystemVerilog is the industry standard, but it was designed for human experts writing RTL by hand — not for AI agents generating hardware from natural-language specs. The result is a language where silent bugs are easy to write and hard to catch:

- **Implicit width conversions**: `assign out = a + b;` silently truncates or zero-extends depending on context. Width mismatches are the #1 source of hardware bugs, and SV makes them invisible. ARCH requires every width cast to be explicit (`.trunc<N>()`, `.zext<N>()`, `.sext<N>()`), or use wrapping operators (`+%`, `-%`, `*%`) when you genuinely want modular arithmetic.

- **No clock domain safety**: crossing clock domains in SV is a convention (use a synchronizer module, hope you picked the right one). ARCH tracks `Clock<Domain>` in the type system — a signal in `Clock<SysDomain>` cannot be assigned to a `Clock<MemDomain>` register without an explicit `synchronizer` construct. The compiler auto-inserts gray-code CDC for async FIFOs and rejects unsafe crossings at compile time.

- **Boilerplate-heavy patterns**: an FSM in SV requires a typedef enum, an always_ff for state_r, an always_comb for state_next, and a separate output block — ~60 lines of mechanical code that every engineer writes slightly differently. ARCH's `fsm` keyword is 15 lines with compiler-verified exhaustive transitions and auto-generated SVA assertions.

- **AI agents produce structurally broken SV**: LLMs frequently generate unbalanced `begin/end`, drive the same signal from multiple `always` blocks, or forget sensitivity lists. ARCH's brace-free `keyword Name ... end keyword Name` grammar, mandatory named block endings, single-driver rule, and all-ports-connected check make it structurally impossible for an AI to produce invalid RTL. The `todo!` escape hatch lets an LLM emit a compilable skeleton for parts it's unsure about.

- **No built-in verification**: SV requires users to manually add assertions, coverage, and simulation infrastructure. ARCH auto-generates overflow/underflow assertions for FIFOs, legal-state + state-reachability + transition coverage for FSMs, counter-range bounds, and `guard` contract assertions — all provable with EBMC formal and checkable with Verilator simulation, with zero user effort.

ARCH compiles to clean, readable SystemVerilog that works with any existing EDA tool (Verilator, VCS, QuestaSim, Vivado, Yosys). It's not a replacement for SV — it's a safer, AI-friendly front-end that generates the SV you'd write by hand, but with compile-time guarantees that the SV spec doesn't provide.

## Quick start

```sh
cargo build

# Type-check only
cargo run -- check mymodule.arch

# Emit SystemVerilog
cargo run -- build mymodule.arch [-o mymodule.sv]

# Simulate with C++ testbench
cargo run -- sim mymodule.arch --tb mymodule_tb.cpp

# Simulate with Python testbench (cocotb-style)
cargo run -- sim mymodule.arch --pybind --test test_mymodule.py
```

## Simulation

ARCH is a **pure synthesizable design language** — it has no built-in testbench constructs, stimulus generators, or assertion libraries. Instead, it is designed to be compatible with existing open-source verification platforms: C++ testbenches (Verilator-style), Python testbenches (cocotb-style via pybind11), and formal tools (EBMC, SymbiYosys). This lets teams keep their verification methodology while adopting ARCH for design entry.

`arch sim` generates Verilator-compatible C++ models from `.arch` sources, compiles them with `g++`, and runs the simulation binary — all in one command.

**C++ testbenches** use the same API as Verilator's generated models (`VModuleName` class with public port fields, `eval()`, `final()`). Existing Verilator C++ testbenches work with minimal changes — just replace the `#include "VModuleName.h"` header:

```cpp
#include "VMyModule.h"
#include "verilated.h"

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VMyModule dut;
    dut.clk = 0; dut.rst = 1;
    for (int i = 0; i < 5; i++) { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); }
    dut.rst = 0;
    // ... drive inputs, check outputs ...
    dut.final();
    return 0;
}
```

**Python testbenches** use `--pybind` to generate a pybind11 wrapper, enabling cocotb-style testing without Verilator or a VPI shim:

```sh
arch sim --pybind --test test_mymodule.py MyModule.arch
```

A drop-in `cocotb_shim/cocotb/` package is placed on `PYTHONPATH`, so plain `import cocotb` works unchanged. The supported surface (decorators, triggers, `Clock`, signal handles) plus the behavioral deltas from real cocotb (tick-sampled scheduler, 2-state values, immediate writes) are documented in **[`doc/arch_sim_cocotb.md`](doc/arch_sim_cocotb.md)**.

**Built-in debug instrumentation** replaces manual `printf`/`$display` for diagnosing simulation failures:

```sh
arch sim --debug MyModule.arch --tb tb.cpp              # print I/O port changes
arch sim --debug+fsm --depth 2 MyModule.arch --tb tb.cpp # + FSM transitions, 2 levels deep
```

Additional flags: `--wave out.vcd` (VCD waveform), `--check-uninit` (uninitialized register detection), `--cdc-random` (CDC metastability modeling).

## Language snapshot

### Combinational logic

```arch
module Adder
  param WIDTH: const = 32;
  port a:   in UInt<WIDTH>;
  port b:   in UInt<WIDTH>;
  port sum: out UInt<WIDTH>;

  comb
    sum = a +% b;   // wrapping add — result width = max(W(a), W(b)) = 32, no overflow widening
  end comb
end module Adder
```

Arch arithmetic follows IEEE 1800-2012 §11.6 (`a + b` on two `UInt<32>` yields `UInt<33>`), so the classic same-width-result spelling is `(a + b).trunc<32>()`. The wrapping operators `+%`, `-%`, `*%` are the shortcut for "same-width modular arithmetic" — `a +% b` drops the overflow bit directly. Mixing widths still requires explicit `.trunc<N>()`, `.zext<N>()`, or `.sext<N>()`.

### Finite state machine with `fsm`

Named states, exhaustive transition checking, automatic reset, auto-generated legal-state assertion and per-state / per-transition cover properties.

```arch
fsm TrafficLight
  port clk:  in Clock<SysDomain>;
  port rst:  in Reset<Async, High>;
  port tick: in Bool;
  port reg light: out UInt<2> reset rst=>0;   // 0=Red, 1=Yellow, 2=Green

  state [Red, Green, Yellow]
  default state Red;
  default seq on clk rising;

  state Red
    seq light <= 0; end seq
    -> Green when tick;
  end state Red

  state Green
    seq light <= 2; end seq
    -> Yellow when tick;
  end state Green

  state Yellow
    seq light <= 1; end seq
    -> Red when tick;
  end state Yellow
end fsm TrafficLight
```

### FIFO with `fifo`

One keyword covers sync / async / LIFO / overflow variants. Dual-clock async is auto-detected from two distinct `Clock<D>` ports and inserts gray-code pointer CDC automatically. Overflow/underflow SVA is auto-generated.

```arch
fifo SyncFifo
  param DEPTH: const = 16;
  param T: type = UInt<32>;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;

  port push_valid: in Bool;    port push_ready: out Bool;    port push_data: in T;
  port pop_valid:  out Bool;   port pop_ready:  in Bool;     port pop_data:  out T;

  port full:  out Bool;
  port empty: out Bool;
end fifo SyncFifo
```

Swap `Clock<SysDomain>` + a second `Clock<OtherDomain>` for a gray-code CDC async FIFO — no other code changes needed.

### Reusable port bundles with `bus`

Define a bus once, instantiate as `initiator` (keeps directions) or `target` (flips all directions). Codegen flattens `m_axi.aw_valid` → `m_axi_aw_valid` in the SV port list.

```arch
bus SimpleAxi
  param DATA_W: const = 32;

  // Signals from the initiator's perspective
  aw_valid: out Bool;
  aw_ready: in  Bool;
  aw_addr:  out UInt<32>;
  w_valid:  out Bool;
  w_ready:  in  Bool;
  w_data:   out UInt<DATA_W>;
  b_valid:  in  Bool;
  b_ready:  out Bool;
end bus SimpleAxi
```

Usage in a module:

```arch
module DmaTop
  port clk:     in Clock<SysDomain>;
  port rst:     in Reset<Sync>;
  port m_axi:   initiator SimpleAxi;     // drives aw_valid, reads aw_ready, etc.
  port s_ctrl:  target SimpleAxi;        // flipped — reads aw_valid, drives aw_ready
  // ...
end module DmaTop
```

### Multi-cycle protocols with `thread`

Thread blocks describe multi-cycle protocols as straight-line code. The compiler lowers each thread to a synthesizable FSM with `wait until`, `fork/join`, counted `for` loops, and `resource`/`lock` arbitration. See `doc/thread_spec_section.md`.

## First-class constructs

| Keyword | Purpose | Status |
|---------|---------|--------|
| `module` | Combinational / registered logic | Done |
| `fsm` | Finite state machine with named states | Done |
| `pipeline` | Staged datapath with hazard logic | Done |
| `thread` | Multi-cycle sequential protocol | Done |
| `fifo` / `lifo` | Sync / async FIFO | Done |
| `ram` | BRAM / SRAM / ROM | Done |
| `arbiter` | N-requester grant logic | Done |
| `counter` | Wrap / saturate / Gray / one-hot / Johnson | Done |
| `bus` | Reusable parameterized port bundle | Done |
| `synchronizer` | CDC primitives (FF, Gray, handshake, pulse, reset) | Done |
| `regfile` | Multi-port register file | Done |

Key rules:
- Every construct uses `keyword Name ... end keyword Name` — no braces
- No implicit width conversions — use `.trunc<N>()`, `.zext<N>()`, `.sext<N>()`
- Clock domain mismatches are compile errors, not warnings

See `doc/ARCH_HDL_Specification.md` for the full language reference and `doc/COMPILER_STATUS.md` for implementation status.

## Onboarding

- Start here for a fast contributor orientation: `doc/ONBOARDING_CHEATSHEET.md`

## Tests

```sh
cargo test            # 52 snapshot + 12 error-case integration tests
```

The `tests/e203/` directory contains benchmark modules from the E203 HBirdv2 RISC-V core verified via `arch sim`:

| Module | Description | Tests |
|--------|-------------|-------|
| `e203_exu_regfile` | 2R1W register file | 5 (+ Verilator cross-check) |
| `e203_exu_wbck` | Write-back arbiter | 6 (+ Verilator cross-check) |
| `e203_ifu_litebpu` | Static branch predictor | 11 (+ Verilator cross-check) |
| `e203_exu_alu_dpath` | Shared ALU datapath | 26 |
| `e203_exu_alu_bjp` | Branch/jump unit | 25 |

The `tests/thread/` directory contains 10 thread construct tests (all Verilator-clean):

| Test | Feature |
|------|---------|
| `basic_thread` | `wait until` handshake, seq/comb assigns |
| `named_thread` | Multiple independent named threads |
| `thread_once` | One-shot calibration sequence |
| `wait_cycles` | Counter-based delay |
| `thread_if_else` | Conditional logic within a state |
| `fork_join` | AXI AW+W parallel handshake |
| `thread_for_loop` | Burst read with counted loop |
| `generate_thread` | 4-channel DMA via generate_for |
| `resource_lock` | Shared bus with priority arbiter |
| `shared_reduction` | Multi-driver `r_ready` with OR reduction |

## Editor support

- **VSCode**: `editors/vscode/` — symlink to `~/.vscode/extensions/arch-hdl`
- **Vim**: `editors/vim/syntax/arch.vim`
