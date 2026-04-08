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

### Phase 1: First CVDP tests (2026-03-26)

- Added first CVDP `.arch` file and `run_cvdp.py` test harness
- Relaxed naming conventions (PascalCase not enforced) to match CVDP module names exactly
- **7 problems passing cocotb on first attempt** (no fixes needed): priority_encoder, signed_unsigned_comparator, nbit_swizzling, caesar_cipher, sync_pos_neg_edge_detector, convolutional_encoder, reverse_bits
- **22 problems passing** by end of day — 15 more first-attempt passes: SetBitStreamCalculator, barrel_shifter_8bit, bcd_counter, bcd_to_excess_3, binary_to_one_hot_decoder, complex_multiplier, digital_dice_roller, fibonacci_series, gf_multiplier, hamming_code_receiver, hamming_code_tx_for_4bit, palindrome_detect, perfect_squares_generator, piso_8bit, serial_in_parallel_out_8bit
- Found nested for-loop codegen bug (workaround applied in nbit_swizzling)
- Found cascaded_adder needs indexed part-select in for loops (deferred)

**First-attempt pass rate for initial batch: 22/22 (100%)** — all modules written from CVDP specs passed cocotb without any .arch fixes.

### Phase 2: Bulk conversion (2026-03-27 – 2026-03-29)

- Added `inside` set membership operator and `for i in {list}` value-list iteration to support more CVDP patterns
- Fixed latch codegen (blocking `=` instead of `<=` in `always_latch`)
- **Mass conversion (commit d21ab38):** 157 new `.arch` files + 16 modified — changed reset syntax from `=` to `=>` across all files (530 files, 43K insertions)
- **Additional batch (commit 8e991c2):** 40 more `.arch` files added with reset-type override support
- Removed redundant reset branches in 9 files
- FSM refactors: unconditional transitions, default-block seq fix, Clock output ports
- Synchronizer improvements: gray decode fix, custom param emission
- Pipeline MAC counter logic simplification
- MCP server updates: missing keywords, inside/for-list hints, trunc/zext width validation
- Added `hw_task_queue` linklist construct benchmark with cocotb testbench

### Phase 3: Mass fix pass (2026-04-03 – 2026-04-04)

Full sweep of all 231 `.arch` files with `arch check`:
- **Before:** 188/231 passing (81%)
- **After fixes:** 213/231 passing (92%)

**25 files fixed** across 4 error categories:
- 18 files: `.trunc<N>()`/`.zext<N>()` on wrong-width values (replace with correct method or remove no-op same-width calls)
- 5 files: reset syntax `reset rst = 0` → `reset rst => 0`
- 1 file: ambiguous `&` vs `==` precedence — added parentheses
- 1 file: `let` inside FSM `default` block — moved to `comb` block

**18 remaining `arch check` failures:** all multi-file designs with undefined sub-module names (cannot fix without missing source files)

### Phase 4: Cocotb validation of fixed files (2026-04-04)

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

### Phase 5: Full cocotb sweep (2026-04-04)

Ran all 191 testable modules (those with matching CVDP JSONL entries) through cocotb. Results saved to `tests/cvdp/cocotb_results.log`.

**Result: 133/191 pass (70%)**

58 failures include:
- Multi-file designs with missing sub-module SV (cocotb only copies one file)
- Logic bugs in the `.arch` implementations
- Test harness timeouts (e.g. `Binary2BCD`, `floor_to_seven_segment`)
- Parameterized test edge cases

40 additional `.arch` files have no matching CVDP JSONL entry and cannot be cocotb-tested.

---

### Phase 6: Binary2BCD root-cause fix (2026-04-05)

Investigated `FAIL Binary2BCD — TIMEOUT` from `tests/cvdp/cocotb_results.log`.

Root cause in `tests/cvdp/Binary2BCD.arch`:
- Final BCD nibble extraction was shifted by one nibble:
  - old: `thousand=sh8[19:16], hundred=sh8[15:12], ten=sh8[11:8], one=sh8[7:4]`
  - correct for this 8-bit double-dabble implementation: `thousand=0, hundred=sh8[19:16], ten=sh8[15:12], one=sh8[11:8]`

Fix applied:
- Updated `tests/cvdp/Binary2BCD.arch`
- Regenerated `tests/cvdp/Binary2BCD.sv`

Validation:
- `cargo run -- check tests/cvdp/Binary2BCD.arch` → pass
- `cargo run -- build tests/cvdp/Binary2BCD.arch -o tests/cvdp/Binary2BCD.sv` → pass
- Brute-force check over all 256 input values (0..255) → 0 mismatches

Note:
- CVDP problem selection for `Binary2BCD` maps to an elevator-system integration harness (`TOPLEVEL=elevator_control_system`) that includes multiple RTL files. The fixed `Binary2BCD` logic removes a confirmed functional bug in this module; a full cocotb re-sweep is still required to update aggregate pass/fail counts.

---

### Phase 7: Filename mismatch discovery (2026-04-07)

Discovered 10 `.arch` files where the ARCH module name differs from the file name (e.g., `16qam_mapper.arch` contains `module qam16_mapper_interpolated`). The cocotb runner looks for `{module_name}.sv` but the compiler outputs `{arch_file_name}.sv`, causing false negatives.

Created SV copies with module-name filenames and re-tested all 10:

| File | Module | Result |
|------|--------|--------|
| 16qam_mapper | qam16_mapper_interpolated | **PASS** (15/15) |
| 16qam_demapper | qam16_demapper_interpolated | **PASS** (60/60) |
| decimator_and_peak_detector | advanced_decimator_with_adaptive_peak_detection | **PASS** |
| restore_division | restoring_division | **PASS** |
| sprite_fsm | sprite_controller_fsm | **PASS** |
| cvdp_convolutional_encoder_RTL_comp | convolutional_encoder | **PASS** |
| signed_comparator | signed_unsigned_comparator | **PASS** |
| sync_serial_communication_top | sync_serial_communication_tx_rx | **PASS** |
| pic_starvation_prevention | interrupt_controller | FAIL (logic bug) |
| programmable_interrupt_controller | interrupt_controller | (shares module name, not re-tested) |

**8 false negatives recovered.** Updated count: 133 + 8 = **141/191 (74%)**.

---

## Current Status (2026-04-07)

| Metric | Value |
|--------|-------|
| Total `.arch` files | 231 |
| Pass `arch check` | 213 (92%) |
| Fail `arch check` (multi-file) | 18 |
| Testable via cocotb (has JSONL entry) | 191 |
| **Cocotb PASS** | **141 (74%)** |
| Cocotb FAIL | 50 |
| No JSONL entry (untestable) | 40 |

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

# Full cocotb sweep (slow — ~30 min)
python3 /tmp/run_all_cvdp.py
# Results saved to tests/cvdp/cocotb_results.log
```
