# arch-com

A compiler for the **ARCH** hardware description language — ingests `.arch` source files and emits deterministic, readable SystemVerilog.

## Quick start

```sh
cargo build

# Type-check only
cargo run -- check mymodule.arch

# Emit SystemVerilog
cargo run -- build mymodule.arch [-o mymodule.sv]

# Generate C++ simulation model and run with a testbench
cargo run -- sim mymodule.arch
g++ arch_sim_build/verilated.cpp arch_sim_build/V*.cpp mymodule_tb.cpp \
    -Iarch_sim_build -o sim_out
./sim_out
```

## Language snapshot

```arch
module Adder
  param WIDTH: const = 32;
  port a:   in UInt<WIDTH>;
  port b:   in UInt<WIDTH>;
  port sum: out UInt<WIDTH>;

  comb
    sum = (a.zext<33>() + b.zext<33>()).trunc<32>();
  end comb
end module Adder
```

Key rules:
- Every construct uses `keyword Name ... end keyword Name` — no braces
- All `let` bindings require an explicit type annotation (`let x: UInt<32> = ...`)
- `Bool` and `UInt<1>` are identical types; use either
- `?:` ternary is supported at any expression position
- No implicit width conversions — use `.trunc<N>()`, `.zext<N>()`, `.sext<N>()`

See `doc/ARCH_HDL_Specification.md` for the full language reference and `doc/COMPILER_STATUS.md` for implementation status.

## Tests

```sh
cargo test            # 38 snapshot + error-case integration tests
```

The `tests/e203/` directory contains benchmark modules from the E203 HBirdv2 RISC-V core verified via `arch sim`:

| Module | Description | Tests |
|--------|-------------|-------|
| `e203_exu_regfile` | 2R1W register file | 5 (+ Verilator cross-check) |
| `e203_exu_wbck` | Write-back arbiter | 6 (+ Verilator cross-check) |
| `e203_ifu_litebpu` | Static branch predictor | 11 (+ Verilator cross-check) |
| `e203_exu_alu_dpath` | Shared ALU datapath | 26 |
| `e203_exu_alu_bjp` | Branch/jump unit | 25 |

## Editor support

- **VSCode**: `editors/vscode/` — symlink to `~/.vscode/extensions/arch-hdl`
- **Vim**: `editors/vim/syntax/arch.vim`
