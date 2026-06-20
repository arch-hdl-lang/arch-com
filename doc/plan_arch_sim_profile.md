# Plan: `arch sim --profile` — Source-level simulation execution profiling

> **Status:** Proposal — 2026-06-20. Not yet branched or scoped.

---

## Motivation

When iterating on performance-sensitive microarchitecture designs, engineers need
to know where simulation cycles are actually being spent.  Today the options are:

1. **`arch sim --coverage`** — tells you WHETHER a block ever executed (binary
   hit/miss), not HOW OFTEN.  Designed for DV sign-off, not performance analysis.
2. **`gprof` / `perf` on the generated C++** — gives counts at the C++ function
   or line level.  A thread FSM lowered to 14 C++ functions gives no indication
   of which user-level `wait until` clause is the bottleneck.
3. **Manual `log` statements** — requires modifying source, adds noise, does not
   scale past a handful of sites.

Locating hot paths (a thread spinning on backpressure, a stall-heavy pipeline
stage, a tight polling loop) today requires either guesswork or a tedious
C++-level profiling session that must then be manually mapped back to ARCH
constructs.  `--profile` closes that gap directly.

---

## User-facing surface

```
arch sim MyDesign.arch --tb tb.cpp --profile
arch sim MyDesign.arch --tb tb.cpp --profile --profile-out profile.json
```

After the testbench exits, a hotness-sorted table is printed to stderr:

```
ARCH Simulation Profile — 200,000 eval() calls (100,000 cycles)

  Rank  Source location                                       Calls       % evals
  ────  ────────────────────────────────────────────────────  ──────────  ───────
  1     MyDesign.arch:88  [seq FetchUnit t0 wait-until]       98,432      49.2 %
  2     MyDesign.arch:51  [comb FetchUnit.stage_out]          99,001      49.5 %
  3     MyDesign.arch:120 [seq TxCtrl t0 state 4 wait-N]      45,102      22.6 %
  4     MyDesign.arch:33  [seq AluPipe.stage2]                99,800      49.9 %
  ...
  --    MyDesign.arch:76  [seq AluPipe.stage3]                     0       0.0 % (never executed)
```

`--profile-out profile.json` emits a machine-readable form for IDE tooling or
post-processing pipelines.

---

## Why this matters

**Thread designs.**  Thread FSMs execute multiple C++ state-machine arms per
clock.  The source-level "hot state" is the `wait until` or `wait N cycle` clause
where a thread spends most of its time — but after lowering that becomes an opaque
state number.  `--profile` maps FSM-arm execution counts back to the originating
ARCH source span (reusing the same span table already computed for
`--debug+fsm` and `--auto-thread-asserts`).

**Pipeline designs.**  A pipeline with a `stall when` stage may be bottlenecked
by one stage's stall condition.  `--profile` shows which stage's `comb` and `seq`
blocks are actually exercised each cycle vs. which are gated by valid propagation.

**Testbench quality.**  If 80 % of eval calls hit one comb block, the stimulus is
not exercising the rest of the design.  Combining `--profile` (quantitative) with
`--coverage` (binary hit/miss) and the planned `pipe_reg` valid tracking gives a
complete picture of both what ran and how often.

**`--thread-sim parallel` load balance.**  Per-OS-thread counter aggregation can
surface load imbalance: which user-thread is the rate-limiter when running with
`--threads N`.

---

## Relationship to existing `--coverage`

| Dimension | `--coverage` | `--profile` |
|-----------|-------------|-------------|
| Primary use | DV sign-off (did every block run?) | Perf analysis (which blocks ran most?) |
| Granularity | Hit / miss (boolean) | Count + % of total eval() calls |
| Output | `coverage.txt` + Verilator-compatible `coverage.dat` | sorted hotlist to stderr + optional `profile.json` |
| Block labelling | Opaque `block N` | ARCH source span + construct context string |
| Thread state mapping | No (just "block") | Yes — maps FSM state arm to `wait until` / `wait N cycle` source |

The two flags are orthogonal and composable (`--coverage --profile`).

---

## Implementation sketch

### Counter injection in sim codegen (`src/sim_codegen/`)

For each instrumented region, inject a `uint64_t _arch_prof_<id>;` counter in the
generated C++ class (alongside the existing coverage `uint64_t _arch_cov_<id>;`
counters) and increment it at the entry of each:

- `seq` block body (after the clock-edge guard, before any statements)
- `comb` block body (at the top of the `eval_comb()` section for that block)
- Per-thread-state FSM arm (the `case` arm inside the thread state switch)
- Per-pipeline-stage `seq`/`comb` sections

A side-table of `{ id, file, line, kind, context_string }` is emitted as a
`constexpr std::array` in the generated header, so the runtime report needs no
external metadata.

### `final()` report

In the generated `final()`:

```cpp
if (_arch_profile_enabled) {
    arch_profile_report(_arch_prof_table, _arch_prof_counters,
                        _arch_eval_count, stderr);
    if (_arch_profile_out) {
        arch_profile_json(_arch_prof_table, _arch_prof_counters,
                          _arch_eval_count, _arch_profile_out_path);
    }
}
```

`arch_profile_report` sorts descending by count and prints the table (including a
final row for any blocks with count == 0 — these are already flagged by
`--coverage` as dead, but showing them in a profile makes them visually salient).

### Thread-state context string

The thread lowering pass already stores the source span and human-readable label
for each FSM state (same data used for `--auto-thread-asserts` SVA labels and
`--debug+fsm` transition output).  The profile counter for state arm `S` reuses
that `(span, label)` pair with no new data collection needed.

### `--thread-sim parallel` extension

Under `--threads N`, counter updates inside OS-thread-local coroutine schedulers
use thread-local storage (no atomic overhead on the hot path); the values are
summed into the global counter array inside `final()`, which runs single-threaded.
The JSON output optionally includes the per-OS-thread breakdown as a sub-table.

---

## Phasing

**Phase 1 — MVP**
- `--profile` flag; counter injection for `module` `seq` and `comb` blocks only.
- Counter injection in `src/sim_codegen/mod.rs` (reuses the skeleton of the
  existing coverage counter path).
- Stderr report at `final()`, sorted by count descending.
- Zero-count blocks listed last with "(never executed)" tag.

**Phase 2**
- Per-thread-state counters with source-span label; per-pipeline-stage counters.
- `--profile-out path.json` machine-readable output.

**Phase 3 (optional)**
- `--thread-sim parallel` per-OS-thread breakdown in JSON.
- VS Code extension gutter annotation (source span + count → inline decoration).

---

## Files that would need to change

| File | Change |
|------|--------|
| `src/sim_codegen/mod.rs` | Counter injection per seq/comb block; `final()` report call |
| `src/sim_codegen/gen_fsm.rs` | Per-FSM-arm counter (Phase 2) |
| `src/sim_codegen/gen_pipeline.rs` | Per-stage counter (Phase 2) |
| `src/main.rs` | `--profile` / `--profile-out` CLI flags |
| `src/sim_compile.rs` | Pass `profile_enabled` / `profile_out` through to codegen |
| `runtime/arch_profile.h` (new) | `arch_profile_report()` / `arch_profile_json()` helpers |

---

## Novelty check (as of 2026-06-20)

- `--profile` does not appear in `doc/COMPILER_STATUS.md`, any existing plan doc,
  or any open/closed issue/PR title.
- `--coverage` is implemented but binary (hit/miss); it serves DV sign-off, not
  performance analysis.
- The coverage counter infrastructure in `src/sim_codegen/` is reusable but
  produces a different output shape.
- No open branches touch `--profile`.  Verify with
  `scripts/claim_check.sh --grep "profile"` before starting work.
