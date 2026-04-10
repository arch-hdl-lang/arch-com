# ARCH Onboarding Cheat Sheet

Quick orientation for contributors working on the `arch` compiler.

## 1) One-screen mental model

`arch` compiles `.arch` HDL sources into deterministic SystemVerilog, and can also generate/run C++ simulation models.

Main pipeline (see [`src/main.rs`](/Users/shuqingzhao/github/arch-com/src/main.rs)):

1. `lex` (`src/lexer.rs`)
2. `parse` (`src/parser.rs`)
3. `elaborate` generate/variants (`src/elaborate.rs`)
4. `lower_threads` thread → FSM + inst (`src/elaborate.rs`)
5. `resolve` symbols/scopes (`src/resolve.rs`)
6. `typecheck` semantics + widths + CDC + drivers (`src/typecheck.rs`)
7. Emit:
   - SystemVerilog (`src/codegen.rs`) for `build`
   - C++ simulator models (`src/sim_codegen.rs`) for `sim`

CLI commands:

- `arch check <files...>`
- `arch build <files...> [-o out.sv]`
- `arch sim <arch_files...> --tb <tb.cpp...> [--outdir DIR] [--check-uninit] [--cdc-random] [--wave out.vcd]`

## 2) Core subsystem map

- **AST model**: [`src/ast.rs`](/Users/shuqingzhao/github/arch-com/src/ast.rs)
  - Canonical language IR for all constructs.
- **Parser**: [`src/parser.rs`](/Users/shuqingzhao/github/arch-com/src/parser.rs)
  - `parse_source_file`, construct parsers (`parse_module`, `parse_fsm`, `parse_ram`, etc.).
- **Elaboration + thread lowering**: [`src/elaborate.rs`](/Users/shuqingzhao/github/arch-com/src/elaborate.rs)
  - Expands `generate for/if`; handles module-variant naming for param/reset override combos.
  - `lower_threads()`: lowers `thread` blocks to FSM + inst at AST level. Handles `fork/join` (product-state expansion), `for` loops (counter register), `resource`/`lock` (arbiter + stall), `shared(or|and)` (reduction assigns). Generated FSMs flow through the normal resolve → typecheck → codegen pipeline.
- **Resolver / symbol table**: [`src/resolve.rs`](/Users/shuqingzhao/github/arch-com/src/resolve.rs)
  - Global + module scopes; first-pass registration + duplicate/name checks.
- **Type checker**: [`src/typecheck.rs`](/Users/shuqingzhao/github/arch-com/src/typecheck.rs)
  - Type inference/checks, driven-port checks, width safety, CDC checks, construct-specific validation.
- **SV codegen**: [`src/codegen.rs`](/Users/shuqingzhao/github/arch-com/src/codegen.rs)
  - Emits deterministic SV from AST + symbols.
- **Native sim codegen**: [`src/sim_codegen.rs`](/Users/shuqingzhao/github/arch-com/src/sim_codegen.rs)
  - Emits `V*.h/.cpp` models and Verilator-compat shims.

## 3) Where to change code (common tasks)

### Add syntax / new construct

1. Add AST nodes in [`src/ast.rs`](/Users/shuqingzhao/github/arch-com/src/ast.rs).
2. Add lexer tokens in [`src/lexer.rs`](/Users/shuqingzhao/github/arch-com/src/lexer.rs).
3. Parse it in [`src/parser.rs`](/Users/shuqingzhao/github/arch-com/src/parser.rs) (`parse_item` + new `parse_*`).
4. Register symbols in [`src/resolve.rs`](/Users/shuqingzhao/github/arch-com/src/resolve.rs) if globally visible.
5. Add semantic checks in [`src/typecheck.rs`](/Users/shuqingzhao/github/arch-com/src/typecheck.rs).
6. Add emit path:
   - SV: [`src/codegen.rs`](/Users/shuqingzhao/github/arch-com/src/codegen.rs)
   - sim (if supported): [`src/sim_codegen.rs`](/Users/shuqingzhao/github/arch-com/src/sim_codegen.rs)
7. Add tests in [`tests/integration_test.rs`](/Users/shuqingzhao/github/arch-com/tests/integration_test.rs) + fixture `.arch` files in `tests/`.

### Add a feature inside `thread` blocks

Thread blocks live inside modules and lower to FSM + inst before resolve/typecheck/codegen. The lowering is entirely in `src/elaborate.rs`.

1. Add `ThreadStmt` variant in `src/ast.rs`.
2. Parse it in `parse_thread_stmt()` in `src/parser.rs`.
3. Handle it in `src/elaborate.rs`:
   - `collect_thread_signals()` — signal analysis (what's read/written)
   - `contains_wait()`, `thread_has_for()` — recursive predicates
   - `partition_thread_body()` — state partitioning (each `wait` = state boundary)
   - `subst_thread_stmt()` — generate-for variable substitution
   - `rewrite_loop_var()` — for-loop variable replacement
4. No changes needed in resolve/typecheck/codegen — the lowered FSM handles it.
5. Add `.arch` + verify `.sv` in `tests/thread/`, run `verilator --lint-only`.

### Add/adjust semantic or type rule

1. Implement rule in [`src/typecheck.rs`](/Users/shuqingzhao/github/arch-com/src/typecheck.rs).
2. If rule needs symbol visibility updates, edit [`src/resolve.rs`](/Users/shuqingzhao/github/arch-com/src/resolve.rs).
3. Add positive + negative tests in [`tests/integration_test.rs`](/Users/shuqingzhao/github/arch-com/tests/integration_test.rs).

### Change emitted SystemVerilog

1. Update emission logic in [`src/codegen.rs`](/Users/shuqingzhao/github/arch-com/src/codegen.rs).
2. Update snapshots (`insta`) and targeted assertions in [`tests/integration_test.rs`](/Users/shuqingzhao/github/arch-com/tests/integration_test.rs) and `tests/snapshots/`.

### Change generated simulator behavior

1. Update model generation in [`src/sim_codegen.rs`](/Users/shuqingzhao/github/arch-com/src/sim_codegen.rs).
2. Verify with `arch sim` flows and C++ testbenches in `tests/*.cpp`.
3. Keep Verilator-compat behavior intact (`verilated.h/.cpp` stubs generated by sim codegen).

## 4) Test corpus layout

- **Core integration snapshots/error tests**:
  - [`tests/integration_test.rs`](/Users/shuqingzhao/github/arch-com/tests/integration_test.rs)
  - `tests/snapshots/`
- **Construct-heavy suites**:
  - `tests/e203/` (RISC-V SoC modules + TBs)
  - `tests/axi_dma/`
  - `tests/l1d/`
  - `tests/cvdp/`
  - `tests/verilog_eval/` (benchmark corpus + runners)
  - `tests/thread/` (10 tests: basic, named, once, wait_cycles, if_else, fork_join, for_loop, generate, resource_lock, shared_reduction)

## 5) Practical command loop (validated in this workspace)

```bash
# Build compiler
cargo build -q

# Full Rust tests (integration snapshots + checks)
cargo test -q

# Type-check one file
cargo run -q -- check tests/top_counter.arch

# Build SystemVerilog
cargo run -q -- build tests/top_counter.arch -o /tmp/top_counter.sv

# Generate + compile + run C++ sim with testbench
cargo run -q -- sim tests/top_counter.arch --tb tests/top_counter_tb.cpp --outdir /tmp/arch_sim_build
```

Observed baseline on 2026-04-05:

- `cargo build -q`: pass
- `cargo test -q`: pass (`12 + 52` tests)
- Representative `check/build/sim` commands above: pass

Expected artifacts:

- `build`: `.sv` output at `-o` path (or `<input>.sv` by default)
- `sim`: generated `V*.h/.cpp`, `verilated.h/.cpp`, compiled sim binary (`sim_out`) in output dir

## 6) Fast references

- Language spec: [`doc/ARCH_HDL_Specification.md`](/Users/shuqingzhao/github/arch-com/doc/ARCH_HDL_Specification.md)
- Thread spec: [`doc/thread_spec_section.md`](/Users/shuqingzhao/github/arch-com/doc/thread_spec_section.md) (§20)
- Multi-outstanding spec: [`doc/thread_multi_outstanding_spec.md`](/Users/shuqingzhao/github/arch-com/doc/thread_multi_outstanding_spec.md)
- Implementation status/roadmap: [`doc/COMPILER_STATUS.md`](/Users/shuqingzhao/github/arch-com/doc/COMPILER_STATUS.md)
- Project entry README: [`README.md`](/Users/shuqingzhao/github/arch-com/README.md)
