# CVDP Benchmark Work Log

> Tracking ARCH compiler validation against the CVDP (Copilot Verilog Design Problems) cocotb benchmark suite.

---

## Overview

The CVDP benchmark tests whether ARCH-generated SystemVerilog is functionally correct by compiling `.arch` source files to `.sv` and running them against the CVDP cocotb testbenches. Each problem provides a natural-language spec, a reference SV implementation, and a cocotb test harness with parameterized test cases.

**Dataset:** CVDP v1.0.4 (non-agentic, non-commercial) — JSONL at `~/github/cvdp_benchmark/full_dataset/`  
**Test runner:** `tests/cvdp/run_cvdp.py`  
**Files:** 231 `.arch` + 235 `.sv` in `tests/cvdp/`, 37 spec files in `tests/cvdp/medium_specs/`

---

## Timeline

### Phase 1: First CVDP test (2026-03-26)

- Added first CVDP `.arch` file and test infrastructure
- Relaxed naming conventions (PascalCase not enforced) to match CVDP module names exactly

### Phase 2: Initial benchmark run (2026-03-26)

- **7 problems passing:** priority_encoder, comparator, nbit_swizzling, caesar_cipher, edge_detector, convolutional_encoder, reverse_bits
- **22 problems passing** by end of day — batch of combinational and simple sequential modules

### Phase 3: Broader coverage (2026-03-27 – 2026-03-30)

- Scaled to ~121+ `.arch` files passing `arch check`
- Fixed reset `=>` syntax across multiple files
- Removed redundant reset branches in 9 files
- FSM refactors for cocotb compatibility
- Added `hw_task_queue` linklist construct benchmark with cocotb testbench
- Pipeline MAC counter logic simplification
- MCP server updates: missing keywords, inside/for-list hints, trunc/zext width validation

### Phase 4: Mass fix pass (2026-04-03 – 2026-04-04)

Full sweep of all 231 `.arch` files with `arch check`:
- **Before:** 188/231 passing (81%)
- **After fixes:** 213/231 passing (92%)

**25 files fixed** across 4 error categories:
- 18 files: `.trunc<N>()`/`.zext<N>()` on wrong-width values (replace with correct method or remove no-op same-width calls)
- 5 files: reset syntax `reset rst = 0` → `reset rst => 0`
- 1 file: ambiguous `&` vs `==` precedence — added parentheses
- 1 file: `let` inside FSM `default` block — moved to `comb` block

**18 remaining `arch check` failures:** all multi-file designs with undefined sub-module names (cannot fix without missing source files)

### Phase 5: Cocotb validation of fixed files (2026-04-04)

Regenerated SV for all 25 fixed files and ran cocotb tests in 3 parallel batches.

**Initial results: 19/25 passing**

Failures investigated and fixed:

| Module | Issue | Fix | Result |
|--------|-------|-----|--------|
| **cache_mshr** | Was a stub (zero-value outputs) | Full ARCH implementation: linked-list MSHR with inline priority encoders, allocate/finalize/fill/dequeue interfaces | 10/10 pass |
| **ping_pong_buffer** | `run_cvdp.py` passed SV file to Icarus twice → duplicate module error | Added deduplication of `VERILOG_SOURCES` list | 1/1 pass |
| **low_pass_filter** | Derived param `NBW_MULT = DATA_WIDTH + COEFF_WIDTH` evaluated to literal `32` at compile time; failed when test overrode widths | **Compiler fix** (`src/elaborate.rs`): preserve expressions for params whose default references other params | 128/128 pass |
| **fsm_seq_detector** | `run_cvdp.py` called `runner()` with no args instead of using pytest for multi-test-function runners | Added `num_test_fns > 1` check to use `pytest.main` | 5/5 pass |
| **fsm_linear_reg** | `port reg` outputs caused 2-stage pipeline (test expects 1-stage); narrow-width overflow at `DATA_WIDTH=2` | Changed to plain ports + comb block; added explicit sign-extension before addition | 35/35 pass |
| **dig_stopwatch** | Pause-resume tick loss: `start_stop` deasserted between `one_sec_pulse` generation and counter processing | Added `prev_start` register for 1-cycle grace period; also fixed odd clock period in `run_cvdp.py` | 3/3 pass |

**Final results: 24/25 passing**

Only remaining failure: `cvdp_copilot_decode_firstbit` — cocotb 2.0 harness incompatibility (`from cocotb.result import TestFailure` removed in cocotb 2.0). Not an ARCH/SV issue.

### Compiler fix: derived param expressions (2026-04-04)

**Problem:** The elaboration pass in `src/elaborate.rs` replaced all param defaults with their compile-time evaluated literals. For derived params like `param NBW_MULT: const = DATA_WIDTH + COEFF_WIDTH;`, this emitted `parameter int NBW_MULT = 32` — correct for the default values but wrong when a parent param is overridden at instantiation.

**Fix:** Added `expr_references_params()` helper that checks whether a default expression references any param name. Params with derived expressions preserve the original expression in SV output; literal-only params still get evaluated. This produces correct SV like `parameter int NBW_MULT = DATA_WIDTH + COEFF_WIDTH`.

**Impact:** Fixed `low_pass_filter` (7/8 → 128/128 cocotb) and eliminated the need for manual SV patching on `cache_mshr`.

### Test harness fixes (`run_cvdp.py`)

Three bugs found and fixed in the test runner:

1. **Duplicate VERILOG_SOURCES** — some CVDP `.env` files list the same SV source twice, causing Icarus `module already declared` errors. Fix: deduplicate with `dict.fromkeys()`.

2. **pytest detection** — the `__main__` rewriter only used pytest when `@pytest.mark.parametrize` was present. Some test files have multiple `def test_*()` functions that each call `runner()` with specific args. Fix: also use pytest when `num_test_fns > 1`.

3. **Odd clock period** — `PERIOD // 2` can produce odd values rejected by cocotb 2.0. Fix: round up to even.

---

## Current Status (2026-04-04)

| Metric | Value |
|--------|-------|
| Total `.arch` files | 231 |
| Pass `arch check` | 213 (92%) |
| Fail `arch check` (multi-file) | 18 |
| Cocotb tested (from fixed batch) | 25 |
| Cocotb passing | 24 (96%) |
| Cocotb failing (harness issue) | 1 |

---

## How to Run

```bash
# Type-check a single file
cargo run --release -- check tests/cvdp/MODULE.arch

# Build SV
cargo run --release -- build tests/cvdp/MODULE.arch

# Run cocotb test (requires CVDP JSONL dataset)
python3 tests/cvdp/run_cvdp.py MODULE_NAME [tests/cvdp/MODULE.sv]

# Full arch check sweep
for f in tests/cvdp/*.arch; do cargo run --release --quiet -- check "$f" 2>/dev/null || echo "FAIL: $f"; done
```
