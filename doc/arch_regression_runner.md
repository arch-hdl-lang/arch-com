# ARCH Regression Runner

`tools/run_arch_regression.py` is a broad backend regression gate for `.arch`
files under `tests/`.

The runner first copies `tests/` into a scratch directory, because
`arch build` emits `.archi` side files next to the input sources. It then
discovers regression units:

- directories whose `.arch` files pass `arch check` together are run as one
  unit, which covers multi-file designs such as AXI DMA;
- directories that do not pass as a group fall back to one unit per `.arch`;
- units that fail `arch check` are reported as `skip_check_failed`;
- package/domain-only units with no generated `module` skip Verilator and
  arch-sim smoke compile after `arch build` succeeds.

For each passing unit, the runner executes:

1. `arch check`
2. `arch build -o <unit>.sv`
3. `verilator --lint-only --sv --assert ... <unit>.sv`
4. `arch sim --tb ...` when the unit has an entry in
   `tests/arch_sim_manifest.json`; otherwise `arch sim --outdir ...`
5. a generated C++ smoke compile of the arch-sim models when no real C++ TB
   exists for that unit

The smoke compile is intentionally not a behavioral testbench. Its job is to
catch simulator-codegen regressions where generated C++ no longer compiles.
Add entries to `tests/arch_sim_manifest.json` when an existing C++ testbench
should be run as part of the broad backend gate.

## Common Commands

Run the normal Rust test baseline:

```bash
tools/run_cargo_test_baseline.py
```

Run the current known-good baseline:

```bash
tools/run_arch_regression.py \
  --baseline tests/arch_regression_baseline.json \
  --work-dir /tmp/arch-regression \
  --jobs 8
```

Refresh the baseline after intentionally improving backend coverage:

```bash
tools/run_arch_regression.py \
  --work-dir /tmp/arch-regression-refresh \
  --jobs 8 \
  --update-baseline tests/arch_regression_baseline.json \
  --allow-failures
```

Run one area while developing:

```bash
tools/run_arch_regression.py \
  --pattern 'axi_dma_tlm/*.arch' \
  --work-dir /tmp/arch-regression-tlm \
  --jobs 4
```

Run top-level single-file tests with their manifest TBs instead of the default
grouped `tests_root` unit:

```bash
tools/run_arch_regression.py \
  --pattern 'cam_*.arch' \
  --no-group-dirs \
  --work-dir /tmp/arch-regression-cam \
  --jobs 4
```

List discovered units without running backend checks:

```bash
tools/run_arch_regression.py --list
```

Every run writes `summary.json` plus per-step stdout/stderr logs under the
work directory.

## Rust Test Inventory Baseline

`tools/run_cargo_test_baseline.py` records the current `cargo test -- --list`
inventory in `tests/cargo_test_baseline.json`, then runs `cargo test`. This is
useful because a plain `cargo test` still exits successfully if a test was
accidentally deleted or renamed.

Refresh the Rust test baseline after intentionally adding, deleting, or renaming
tests:

```bash
tools/run_cargo_test_baseline.py --update-baseline --list-only
```
