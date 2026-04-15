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

Arch arithmetic follows IEEE 1800-2012 §11.6 (`a + b` on two `UInt<32>` yields `UInt<33>`), so mixing widths requires explicit `.trunc<N>()`, `.zext<N>()`, or `.sext<N>()`. The wrapping operators `+%`, `-%`, `*%` are the common shortcut for "same-width modular arithmetic" and replace the verbose `(a.zext<33>() + b.zext<33>()).trunc<32>()` pattern.

### Sequential protocols with `thread`

Thread blocks describe multi-cycle protocols as straight-line code. The compiler lowers each thread to a synthesizable FSM.

```arch
module AxiWrite
  port clk:      in Clock<SysDomain>;
  port rst_n:    in Reset<Async, Low>;
  port aw_valid: out Bool;
  port aw_addr:  out UInt<32>;
  port aw_ready: in Bool;
  port w_valid:  out Bool;
  port w_data:   out UInt<32>;
  port w_ready:  in Bool;
  port b_ready:  out Bool;
  port b_valid:  in Bool;

  thread on clk rising, rst_n low
    // AW and W channels handshake in parallel
    fork
      aw_valid = 1;
      aw_addr  = 32'hA000;
      wait until aw_ready;
      aw_valid = 0;
    and
      w_valid = 1;
      w_data  = 32'hDEAD;
      wait until w_ready;
      w_valid = 0;
    join

    // Wait for write response
    b_ready = 1;
    wait until b_valid;
  end thread
end module AxiWrite
```

Thread features:
- `wait until cond` / `wait N cycle` — state boundaries
- `fork ... and ... join` — parallel branches (product-state expansion)
- `for i in 0..N ... end for` — counted loops with waits
- `thread once` — one-shot initialization sequences
- `resource` / `lock` — shared bus arbitration with priority arbiter
- `shared(or|and)` — multi-driver signals with compiler-synthesized reduction
- `generate_for` / `generate_if` with threads — N independent FSMs

See `doc/thread_spec_section.md` and `doc/thread_multi_outstanding_spec.md` for the full thread specification.

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
