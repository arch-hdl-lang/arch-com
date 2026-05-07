# Plan: `arch sim --coverage`

## Context

ARCH already emits SVA cover/assert properties to SystemVerilog — both user-declared (`cover saw_max: cnt == 255;`) and auto-generated (FSM state reachability `_auto_reach_*`, transitions `_auto_tr_*`, FIFO overflow/underflow, counter range, legal-state). These work for Verilator `--assert` and EBMC formal, but `arch sim` (our C++ simulator) currently ignores them entirely — no runtime counters, no coverage report at sim end.

Goal: make `arch sim --coverage` print a coverage summary at the end of simulation.

## Non-goals (v1)
- **Branch coverage** (if/else, match arms) — defer to v2
- **Toggle coverage** (signal 0↔1 transitions) — defer to v2
- **Line/statement coverage** — defer indefinitely (low value for declarative HDL)
- **Depth propagation** — report top module only for v1; no `--depth` yet
- **HTML/JSON output** — stdout text table only for v1

## Output format

```
COVERAGE SUMMARY: CounterCheck
Property                              Kind     Evals  Fails  Status
------------------------------------- -------- -----  -----  ------
no_overflow                           assert     100      0  PASS
saw_max                               cover        3      -  HIT
saw_zero                              cover        0      -  MISS
_auto_reach_IDLE                      cover      200      -  HIT
_auto_no_overflow                     assert    5000      0  PASS

Totals: 5 properties, 4 HIT/PASS, 1 MISS. Coverage: 80%.
```

**Exit code**: 0 on all HIT/PASS. Non-zero on any MISS or any assert with Fails > 0 (for CI integration).

---

## Critical Files

| File | Role |
|------|------|
| `src/main.rs` | Add `--coverage` CLI flag; forward to SimCodegen |
| `src/sim_codegen.rs` | Collect cover/assert properties, emit counters, `print_coverage()`, call from `final()` |
| `src/codegen.rs` | **No changes** — SV emission is unaffected |
| `src/ast.rs` | **No changes** |

---

## Step 1 — CLI (`src/main.rs`)

Add field to `Sim` enum (after `debug_fsm`):
```rust
/// Print a functional coverage report at end of simulation
#[arg(long)]
coverage: bool,
```

Forward to `SimCodegen::new(...).coverage(coverage)` and through the `run_sim` signature.

---

## Step 2 — `SimCodegen` field + builder (`src/sim_codegen.rs` lines 30–70)

```rust
pub struct SimCodegen<'a> {
    // ... existing fields ...
    coverage: bool,
}

pub fn coverage(mut self, enabled: bool) -> Self {
    self.coverage = enabled;
    self
}
```

---

## Step 3 — Collect all cover/assert properties during `gen_module`

Build a `Vec<CovProp>` walking:
- `ModuleBodyItem::Assert(a)` — user-declared assert/cover inside module body
- Construct's `.asserts` — user-declared inside fsm/fifo/ram/counter/arbiter/regfile/pipeline/linklist
- **All auto-generated properties** emitted by codegen. Reviewer flagged this is the widest surface; enumerate them explicitly:
  - FSM: `_auto_legal_state`, `_auto_reach_<state>`, `_auto_tr_<src>_to_<tgt>`
  - FIFO: `_auto_no_overflow`, `_auto_no_underflow` (respecting `OVERFLOW` param variant at codegen.rs:3578)
  - Counter: `_auto_count_range` (when MAX present)
  - Ram, Arbiter, Regfile, Pipeline: user asserts only in v1 (no auto ones emitted today per audit)
  - Bus flattened signals: no auto-covers; skip

```rust
struct CovProp {
    label: String,
    kind: AssertKind,
    expr: Expr,           // SVA condition; reuse cpp_expr() to compile
    clk: String,          // which clock the SVA fires on (single-clk: default; multi-clk: wr_clk vs rd_clk)
    reset_guard: Option<String>,  // e.g. "!rst" — None means unguarded
}
```

Since construct codegen is in `src/codegen.rs` (SV) and sim is separate, we **regenerate** the auto-property list in sim_codegen.rs by duplicating the small bit of logic that decides labels/expressions. Acceptable because: (a) the auto-properties are a closed set of ~5 patterns, (b) the alternative (emitting a shared JSON manifest) is more code. Add a helper module `auto_covers.rs` if duplication grows.

**Reset-gate reconstruction** (reviewer issue #3): every auto-assert in codegen.rs emits `{rst_inactive} |-> expr`. The sim side must replicate: call the same `extract_reset_info()` (now shared via `ast::extract_reset_info`) and build `!rst` or `rst` depending on reset polarity.

---

## Step 4 — Per-property counter fields

When `emit_coverage`, for each CovProp:
```cpp
uint64_t _cov_<label>_evals = 0;
uint64_t _cov_<label>_fails = 0;   // only for asserts; always 0 for covers
```

Fields go in the existing `public:` section (same spot as `_dbg_cycle`).

---

## Step 5 — Increment in `eval_posedge()`

After register commit, and only when `self.coverage` is enabled:

```cpp
// Coverage: check each property when its clock rose and reset inactive
if (_rising_<clk>) {
    if (<reset_inactive>) {
        if (<expr>) _cov_<label>_evals++;
        else _cov_<label>_fails++;   // assert only; for cover, else is "no hit" not a fail
    }
}
```

For `cover`: increment `_evals` when expression is true (i.e. the cover point hit). `_fails` stays 0 and is printed as `-`.
For `assert`: increment `_evals` when true, `_fails` when false. Assert with Fails > 0 prints as `FAIL`.

This resolves reviewer issue #1 (proper assert semantics): we count evaluations **and** failures separately. An UNCHECKED assert has `_evals == 0`.

**Reviewer issue #4 (multi-clock)**: each CovProp stores `clk`. For async FIFO, write-domain covers fire on `_rising_wclk`, read-domain on `_rising_rclk`. The generator picks the right clock per property.

**Expression translation** (reviewer issue #5): use existing `cpp_expr(&prop.expr, &ctx)`. Test with: `(cnt < 256)`, `cnt == 255`, `state_r == IDLE`, `a.trunc<4>() == 0`, `$clog2(N)` in bounds. Add a smoke test that builds a module using each form under `--coverage` to confirm translation works; if `$clog2` or cross-module refs break, scope it out in v1.

---

## Step 6 — `print_coverage()` method

Generate a per-module `print_coverage()`:
```cpp
void <Class>::print_coverage() {
    printf("\nCOVERAGE SUMMARY: <ModuleName>\n");
    printf("%-37s %-8s %5s  %5s  %s\n", "Property", "Kind", "Evals", "Fails", "Status");
    printf("------------------------------------- -------- -----  -----  ------\n");
    int hit = 0, total = 0;
    // one printf line per CovProp; compute status from counters
    // ...
    printf("\nTotals: %d properties, %d HIT/PASS, %d MISS. Coverage: %d%%\n",
           total, hit, total - hit, (hit * 100) / total);
}
```

For assert: `PASS` if evals > 0 && fails == 0; `FAIL` if fails > 0; `UNCHECKED` if evals == 0.
For cover: `HIT` if evals > 0; `MISS` if evals == 0. Dash in `fails` column.

Declare in header; implement in `.cpp`.

---

## Step 7 — Auto-call from `final()`

Modify `final()` to call `print_coverage()` when coverage was enabled at compile time. Can use `#ifdef ARCH_COVERAGE` with a macro set by compile flag, but simpler: only emit `print_coverage()` at all when `self.coverage` is set — and unconditionally call from `final()` in that case.

**Reviewer issue #6 (sub-instance wiring)**: grep confirmed top-level `final()` does **not** propagate to sub-instances today. For v1, keep scope to top module only — document the limitation. Sub-module coverage is a v1.1 extension (add `_inst_X.print_coverage()` calls in parent's `final()` mirroring the `--debug+depth` pattern).

---

## Step 8 — Exit code for CI

The top module's testbench calls `final()` which prints the report. To signal CI failures:
- Track global counts in a static (e.g. `static int _cov_failures = 0`) incremented by `print_coverage()` for each MISS or assert FAIL.
- The testbench's `main()` returns `_cov_failures > 0 ? 1 : 0`.

For v1, simpler: just print the summary; exit code non-zero on fail is a v1.1 polish.

---

## Verification

1. **Unit-level**: regenerate `tests/assert_cover/counter_check.sv` with `--coverage`, run under the Verilator-compatible testbench. Expect:
   - `no_overflow` assert: PASS N
   - `saw_max` cover: HIT
   - `saw_zero` cover: HIT
2. **FSM**: run `vending_machine.arch` under `arch sim --coverage` with an exercising testbench. Expect all 7 state `_auto_reach_*` to HIT, most transitions to HIT.
3. **FIFO**: run `Mm2sFifo` sim. Expect `_auto_no_overflow` PASS, `_auto_no_underflow` PASS.
4. **Counter**: run `setup_counter` sim. Expect `_auto_count_range` PASS.
5. **Multi-clock**: run `sd_tx_fifo` (dual-clock async FIFO) with separate wr/rd testbench. Expect properties on both domains count correctly.
6. **Negative test**: write a module with a `cover` property that's never reachable. Expect MISS in report.
7. **Combined flags**: `arch sim --debug+fsm --coverage module.arch` — should produce both debug traces and coverage summary without interference.

---

## Future (v1.1+, not in this plan)

- **Sub-instance coverage**: parent's `final()` calls `_inst_X.print_coverage()` for each instrumented sub-instance
- **Exit code non-zero** on MISS/FAIL
- **Depth flag** `--coverage-depth N` (mirror `--depth` for `--debug`)
- **Branch coverage**: auto-emit `cover property` for each if-else branch taken + each match arm taken
- **Toggle coverage**: per-signal 0→1 / 1→0 transitions, reusing `_dbg_prev_*` shadow pattern
- **JSON / HTML output** for CI dashboards
- **File output** via `--coverage-file <path>`
- **Cumulative runs** (merge coverage across multiple test files)
