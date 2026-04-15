# arch-com

A compiler for the **ARCH** hardware description language — ingests `.arch` source files and emits deterministic, readable SystemVerilog.

**Paper**: [arXiv:2604.05983](https://arxiv.org/abs/2604.05983)

## Quick start

```sh
cargo build

# Type-check only
cargo run -- check mymodule.arch

# Emit SystemVerilog
cargo run -- build mymodule.arch [-o mymodule.sv]

# Simulate with C++ testbench
cargo run -- sim mymodule.arch --tb mymodule_tb.cpp
```

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
