# Enhancement: `arch sim --check-sv` — automated cycle-accurate native-sim / SV equivalence checking

**Date:** 2026-06-04
**Status:** Proposal — needs team discussion before implementation
**Related:** issue #244 (no dual-backend regression suite), issue #437 (SoftmaxEngine native/Verilator disagrees), `--thread-sim both` (existing FSM/coroutine cross-check), `harc-com PR #321` (`--check-backends` in the HARC testbench path)

---

## Problem

The native C++ simulator (`arch sim --tb`) and the Verilator-compiled SV backend are independent
codegen paths that share no implementation. They can — and do — silently diverge.

**Documented evidence:**

- **Issue #244** (still open): A single arch-ibex porting session found **13 distinct native sim
  codegen bugs**, none of which had regressions. The issue proposes a `tests/equiv/` framework
  but no implementation has landed.
- **Issue #437** (open): `SoftmaxEngine` native sim produces the wrong first `weight_out`; Verilator
  is correct. Root cause: registered-output timing alignment for thread-lowered modules. Neither
  the TB assertions nor `arch check` caught it — only a manual side-by-side comparison did.
- **2026-05-27 code review** (finding §2): Round-robin arbiter `grant_r` initial-value semantics
  diverge between native sim and Verilator in the cycle immediately after reset. Noted as "real
  SV↔sim divergence" but left unfixed because no mechanism existed to catch it automatically.
- **2026-05-28 code review** (finding on `RRArb3.arch`): The regression test is "Verilator-only."
  The stated ask was "byte-identical expectation under both default sim and Verilator" — no
  mechanism currently enforces this.

**What harc-com has, and what it does not cover:**

`harc-com` v0.60+ ships `--check-backends` (`harc-com/src/check_backends.rs`, PR #321), which
compares harc-generated C++ traces against Verilator-compiled SV for HARC testbenches. This closed
the arch-com#437 regression *class* for designs using the HARC testbench path. However, **the
`arch sim --tb` C++ testbench path has no analogous check.** Designers who write raw C++ TBs —
the primary path for rapid bring-up, all the `tests/` unit tests, and all LLM-generated TBs —
get no divergence detection.

**Why this matters:**

ARCH's value proposition includes two independent backends: native sim for fast iteration,
Verilator/SV for tape-out confidence. If they disagree, at least one has a bug — but designers
currently have no way to know until symptoms appear in synthesis or tape-out. The learning system
(`arch advise`) captures type-error→fix pairs; it does not capture behavioral-correctness
divergences of this class.

---

## Proposed flag

```
arch sim --check-sv Module.arch --tb tb.cpp
```

If Verilator is in `PATH`, runs both the native sim and Verilator-compiled SV against identical
input stimulus and compares their output-port traces. Exits 1 on any divergence. If Verilator is
not found, prints a warning and exits 0 (the native run still completes — non-fatal degradation).

Composable with existing flags:

```bash
arch sim --check-sv --wave out.vcd Module.arch --tb tb.cpp   # waveform + equivalence check
arch sim --check-sv --coverage Module.arch --tb tb.cpp       # coverage + equivalence check
arch sim --check-sv --debug Module.arch --tb tb.cpp          # verbose + equivalence check
```

---

## How it works

The comparison uses the `--debug`-format trace as the exchange medium, directly following the
precedent of `--thread-sim both`, which diffs FSM-lowered vs coroutine traces line-by-line.

### Step 1 — Record native sim trace

Run native sim with forced `--debug` output captured to a temp file. The `--debug` format records
every input and output port change on each cycle:

```
[1][Module.in_a](in)  0x0 -> 0x5
[1][Module.out_b](out) 0x0 -> 0xa
[2][Module.in_a](in)  0x5 -> 0x7
[2][Module.out_b](out) 0xa -> 0xe
```

This captures both **inputs driven by the TB** (the stimulus sequence) and **outputs produced by
the DUT** (the behavioral trace). Both are needed: inputs to reconstruct the replay stimulus,
outputs to compare against the Verilator run.

### Step 2 — Derive replay stimulus from native trace

From the captured native trace, extract the per-cycle value of every input port. This becomes
the canonical stimulus sequence. It is by construction identical to what the original TB drove —
no re-running of the TB is needed.

Generate a minimal Verilator replay TB (`arch_sv_replay_tb.cpp`, ~100 lines, auto-generated in
the temp directory) that:

- On each cycle, drives each input port to the recorded native value.
- Accumulates the `--debug`-format output-port trace to a second temp file.
- Does **not** re-execute the original `tb.cpp` assertions (avoids Verilator API mismatch and
  keeps the comparison focused on DUT behavior, not TB logic).

### Step 3 — Compile and run Verilator replay

```bash
# arch already emits SV via arch build; --check-sv reuses that path
arch build Module.arch -o /tmp/arch_sv_replay.sv

verilator --cc /tmp/arch_sv_replay.sv \
          --exe /tmp/arch_sv_replay_tb.cpp \
          --build -o arch_sv_replay
/tmp/arch_sv_replay_build/arch_sv_replay
```

Verilator is invoked with the same SV file that `arch build` would normally emit. No separate
SV emission pass is needed — the `--check-sv` flow runs `arch build` as an implicit step before
the Verilator compilation.

### Step 4 — Diff output-port traces

Compare native output-port lines against Verilator output-port lines. Input-port lines are
present in the native trace (they were driven by the TB) but are excluded from comparison —
both backends receive the same inputs by construction, so only outputs are in scope.

Divergence report format (matches the style of `--thread-sim both` divergence output):

```
arch sim --check-sv: 1000 cycles, 4 output ports, 3 divergences:
  [7][Module.out_b]  native=0x01  sv=0x00
  [7][Module.out_c]  native=0x05  sv=0x00
  [8][Module.out_b]  native=0x00  sv=0x01
```

Clean pass:

```
arch sim --check-sv: 1000 cycles, 4 output ports — PASS
```

---

## Precedent in the codebase

| Mechanism | Location | What it compares |
|---|---|---|
| `--thread-sim both` | `src/sim_codegen/thread_sim.rs` | FSM-lowered native sim vs coroutine native sim |
| `--check-backends` (harc-com) | `harc-com/src/check_backends.rs` | HARC-generated C++ vs Verilator SV (HARC TB path only) |
| **`--check-sv`** (proposed) | `src/main.rs` | Native C++ sim vs Verilator SV (`--tb` path) |

The `--thread-sim both` code already demonstrates the full diff infrastructure needed:

- Captures `--debug`-format traces from both paths to temp files.
- `diff_trace_strings` (existing, per 2026-05-29 review notes on `harc-com/src/check_backends.rs`)
  walks both traces by cycle/line, collects divergences, and formats the report.
- Divergence exit code and summary are already wired in the `--thread-sim both` path.

Adapting this for `--check-sv` requires three new pieces:

1. **Verilator invocation helper** in `src/main.rs`: `fn invoke_verilator(sv_path, tb_path, out_dir) -> Result<()>` — shells out to `verilator` with `--cc`, `--exe`, `--build`; returns `Err` if Verilator is absent (degrades gracefully).
2. **Replay TB generator** (~100 lines in `src/main.rs` or a new `src/equiv.rs`): reads the native `--debug` trace, emits a C++ TB that replays input-port values per cycle and writes the output-port trace.
3. **`--check-sv` flag** in the `arch sim` arg parser: triggers steps 1–4 after the normal native run completes.

The `--debug` trace format is already stable and used in integration tests; it does not need to
change. The replay TB generator can be kept minimal — it only needs to handle scalar, Vec, and
bus input ports, matching the set `--debug` already instruments.

---

## Implementation sketch

```rust
// src/main.rs — alongside existing sim flags

#[arg(long, default_value_t = false)]
check_sv: bool,   // run Verilator cross-check after native sim
```

In `run_sim()` (after the native sim completes successfully):

```rust
if opts.check_sv {
    let sv_path = emit_sv_to_temp(design)?;        // arch build → temp .sv
    let native_trace = capture_debug_trace();       // already captured in step 1
    let replay_tb = gen_replay_tb(&native_trace)?;  // derive stimulus, emit .cpp
    let sv_trace = invoke_verilator_and_run(&sv_path, &replay_tb)?;
    let diffs = diff_output_traces(&native_trace, &sv_trace);
    report_and_exit(diffs);
}
```

`gen_replay_tb` emits a fixed C++ template (no ARCH-language parsing needed at this stage —
the trace is already in the decoded `[cycle][port](dir) old -> new` format):

```cpp
// auto-generated replay TB for arch sim --check-sv
#include "VModule.h"
#include <fstream>
// ... replay stimulus from embedded per-cycle table ...
int main() {
    VModule dut;
    // for each cycle: drive recorded inputs, eval, dump output ports
}
```

The per-cycle stimulus table is a static C++ array emitted directly into the replay TB source,
avoiding any file I/O at replay time.

---

## Tests

| Test | Expected result |
|---|---|
| Simple counter — both agree | `PASS`, 0 divergences |
| Bool `not` lowering mutation (reproduces closed issue #492 — byte-wide `0xff` vs 1-bit `0x01`) | `FAIL`: `[N][Module.out] native=0x01 sv=0xff` (catches the class of issue #492 before it is fixed) |
| RR arbiter first-cycle post-reset (open 2026-05-27 finding) | `FAIL`: specific cycle/port report; provides the regression fixture the 2026-05-27 review asked for |
| `SoftmaxEngine` first-output timing (issue #437) | `FAIL`: `[7][SoftmaxEngine.weight_out] native=0x0 sv=<correct>` (documents the open divergence) |
| `async reset module` — both agree | `PASS` |
| Multi-module design (`arch sim a.arch b.arch --tb tb.cpp --check-sv`) | Top-level DUT ports compared; sub-modules not separately checked |
| Verilator not in PATH | `WARNING: --check-sv skipped: 'verilator' not found in PATH`; exit 0; native run result stands |
| Module with `--inputs-start-uninit` (some ports undriven) | Undriven ports absent from native trace; replay TB omits them; comparison covers only driven ports |

---

## What this does not do

- Does not compare sub-instance internal signals — only the top-level DUT's output ports appear in
  both `--debug` traces. Internal register divergences are detectable only when they manifest on
  output ports.
- Does not replace formal verification — simulation covers only the stimulus exercised by the TB.
- Does not re-run TB assertions against Verilator — the replay TB drives stimulus only. A design
  where TB assertions pass on native sim but fail on Verilator would not be caught unless those
  assertion failures manifest as output-port divergences. (Complementary to, not a replacement for,
  running the full TB against Verilator directly.)
- Does not cover `--thread-sim parallel` — non-deterministic when multiple TBs could drive the same
  inputs differently; out of scope for v1.
- Does not cover `--pybind --test` — the Python cocotb path is a separate sim surface; a
  complementary `--check-sv` for that path is deferred.

---

## Why this matters

The 13 bugs found in issue #244's arch-ibex session were each found reactively — a designer
noticed unexpected behavior and traced it to a sim_codegen bug. `--check-sv` makes that process
proactive: any `arch sim --tb` run can be upgraded to a cross-check with one flag.

For LLM-generated designs specifically, this is high-leverage: an LLM iterating on an ARCH design
uses native sim as the primary feedback loop. If native sim disagrees with Verilator, the LLM
sees a "passing" design that will fail in synthesis. `--check-sv` surfaces that class of false
confidence without requiring the LLM or designer to manually manage two separate test runs.

The implementation cost is low — the `--thread-sim both` diff infrastructure already exists, the
`--debug` trace format is stable, and the Verilator invocation is a straightforward shell-out. The
net result is a single flag that turns the existing "two separate test runs" workflow into an
automated, exit-code-gated equivalence check.
