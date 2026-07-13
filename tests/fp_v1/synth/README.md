# FP RTL — yosys synthesis (logic depth / area proxy)

Combinational synthesis of the arch-emitted **f32** and **bf16** FP operators,
to estimate **logic depth** (a timing proxy) and **gate count** (an area proxy).

This is *not* sign-off timing — no PDK / Liberty cell library is assumed, so the
numbers are **2-input-gate levels** and **generic-cell counts**, not ns / µm².
It complements the correctness work (`tests/fp_v1/smt_proof`,
`proofs/lean_fp_equiv`): same operators, same source-of-truth IR
(`src/fp_ops.rs`), now looked at through a physical-cost lens.

## Running

```
tests/fp_v1/synth/run_synth.sh [outdir]                    # combinational sweep, all FP ops
tests/fp_v1/synth/run_synth.sh --stages N MODULE [outdir]  # staged/pipelined operator (phase 3)
```

Requires `yosys` (≥0.30) and `python3`. Builds a fresh release `arch` by default
(set `ARCH_BIN=/path/to/arch` to reuse one). The generated `.arch`/`.sv`/`.v`,
yosys scripts, and logs land in `outdir` (a temp dir unless given) — nothing is
written back into the repo tree.

### Flow (per operator)

1. Emit a single-operator `.arch` module (e.g. `comb y = a * b;`).
2. `arch build` → SystemVerilog — **the proven RTL**, rendered from the same
   `fp_ops.rs` IR as the SMT and Lean models.
3. `hoist_decls.py` → yosys-friendly Verilog. The emitted helpers are pure SSA
   `function automatic`s with decl-with-initializer locals
   (`logic [7:0] _t0 = a[30:23];`); yosys's built-in Verilog parser rejects those,
   so the pass hoists declarations and splits the initializers into blocking
   assignments (semantics-preserving; the bodies have no control flow).
4. yosys `synth -flatten` → `abc -fast -g <2-input gates>` → `ltp` (longest
   topological path = logic depth) + `stat` (cell count).

## Results (yosys 0.64, `abc -fast`, 2-input generic gates)

Regenerated 2026-07-12 (proposal-phase-3 pass over this flow) after fixing a
yosys-version-drift bug in this script: yosys 0.64's `stat`/`ltp` text no
longer matches the phrasing (`"Number of cells:"`) an older 0.33-era version
of this script parsed, so every row below silently printed `?` until the
parser was updated to match 0.64's plain `"<N> cells"` format. The absolute
numbers therefore also shifted from the previously-committed 0.33 table
(different yosys/abc version ⇒ different optimization result) — this is the
**current, reproducible-on-this-toolchain** table; re-run the script yourself
to confirm.

| operator | gate cells (area proxy) | logic depth (levels) |
|---|---:|---:|
| `bf16_to_f32` (widen) | 22 | 5 |
| `f32_to_bf16` (narrow) | 83 | 17 |
| `bf16_mul` | 1,124 | 110 |
| `f32_mul` | 4,864 | 186 |
| `f32_add` | 1,814 | 169 |
| `f32_sub` | 1,820 | 178 |
| `bf16_add` | 1,015 | 124 |
| `bf16_sub` | 1,073 | 128 |
| `bf16_fma` | 3,799 | 279 |
| `f32_fma` | 8,927 | 301 |

### Reading it

- **Conversions are nearly free** (depth 5–17) — field manipulation + one round.
- **Multiply is shallower than add** (186 vs 169–178 — note add/sub depth is
  now *below* multiply on this yosys version's optimization result, the
  opposite ranking from the 0.33-era table; see the caveats below on why
  cross-operator depth ranking is the only robust signal, not multiply-vs-add
  ordering specifically). The bounded f32 adder's serial chain (exponent
  compare → barrel-shift align → add → leading-zero count → normalize shift →
  round) and the multiplier's partial-product-tree reduction both live in the
  100–300-level range.
- **bf16 arith ≈ f32 arith in depth**, because bf16 = widen→f32 op→narrow: it
  *reuses* the f32 datapath. bf16 has fewer cells (narrower significand) but
  broadly the same critical-path structure.
- **FMA is the critical operator** — depth ~279–301, the deepest of any op —
  from `fma()`'s bounded sticky-fold alignment (`src/fp_ops.rs::fma_f32`,
  *not* the exact-wide 470-bit reference kept only for the Lean/SMT proof
  miter — see that function's doc comment) plus normalization/rounding. This
  is exactly the operator the pipelined-operator registry
  (`doc/proposal_pipelined_operators.md`) targets for retiming — see
  "Staged/pipelined operators" below.

## Staged/pipelined operators (`--stages N MODULE`, proposal phase 3)

`doc/proposal_pipelined_operators.md`'s registry carries a characterized fmax
per `(operator, profile, stages)` row — today just `fma<FP32, 6>`, noted as
"~260 MHz (Yosys abc: `buffer -N 8; upsize; dnsize`)". That figure comes from
an **external** run: Yosys + OpenSTA against a Nangate45 (typ.) Liberty file,
which this repo's checked-in flow cannot reproduce — neither a Liberty file
nor OpenSTA is available in this repo's dev/CI sandboxes, only open-source
`yosys`/`abc` with generic 2-input gates.

`run_synth.sh --stages 6 F32Fma` emits the same shape `arch build` binds
`fma<pipelined, 6>` to (comb `arch_fma_f32` feeding the 6-deep `pipe_reg`
cascade — see `src/pipelined_ops.rs` module docs), then runs it through
`abc -fast -g <gates>`, whose default script always includes ABC's `dretime`
sequential-retiming pass (`yosys -h abc`) — the generic-gate-mapping analogue
of the registry note's recipe, minus the `-liberty`/`-constr`-driven
`buffer`/`upsize`/`dnsize` steps (those require a cell library).

**Reproduced here (yosys 0.64, generic gates, 2026-07-12):**

| module | cells | dff bits | `ltp -noff` (logic-depth proxy) |
|---|---:|---:|---:|
| `F32FmaS6` (`fma<pipelined, 6>`, 6 stages) | 10,640 | 384 | 485 |

**This is not the ~260 MHz figure, and is not claimed to be.** Two honest
findings from reproducing this locally:

1. Open-source `abc`'s `dretime` (no `-liberty`, no `-D` delay target) does
   **not** redistribute the register cascade across the comb cone in this
   environment — `ltp -noff`'s reported longest path (485 levels) runs from
   a primary input straight into the *first* cascade register
   (`y_stg1`), i.e. still the un-rebalanced full comb depth, matching the
   un-staged `f32_fma` combinational depth (301, `F32Fma`, table above) plus
   the DFF-insertion overhead from `synth -flatten`'s technology mapping.
   Passing `-D <ps>` to `abc -fast` (which the `abc` help says substitutes
   `dretime` for `dretime; retime -o {D}`) was tried and made no observed
   difference either, in this yosys/abc build.
2. Achieving the registry's ~260 MHz figure requires the `-liberty`/`-constr`
   ABC script variant (`strash; dretime; map {D}; buffer; upsize {D}; dnsize
   {D}; stime -p`) against a real cell library plus OpenSTA for the
   post-map static timing report — neither of which ships in this repo's
   sandbox. The ~260 MHz number in the registry (`src/pipelined_ops.rs`)
   is retained as the external-run characterization it always was; this
   section documents, rather than silently omits, that the checked-in flow
   does not reproduce it, and reports what it *does* reproduce (a
   logic-depth proxy) instead of fabricating a substitute fmax number.

If a Liberty file and OpenSTA become available in a future environment, wire
`abc -liberty <cells.lib> -constr <constr>` and an OpenSTA `report_checks`
pass into the `--stages` branch of `run_synth.sh` and replace this section
with the real, reproducible fmax.

## Caveats

1. **Depth is gate levels, not ns.** No standard-cell library, so no ns / Fmax.
   Absolute numbers are *pessimistic* — a real library with complex gates and
   dedicated carry chains collapses them; only the cross-operator *ranking* is
   robust.
2. **`abc -fast`** favors runtime over optimization quality; depths are
   upper-ish estimates, not the minimum achievable.
3. **All combinational, unpipelined.** Depths of 150–235 gate-levels are far too
   deep for a high clock in one cycle — the arithmetic operators (especially
   `fma`) want pipelining (the `pipeline` construct) for a real frequency target.
   Conversions and compares are single-cycle-cheap.

To get real ns/Fmax, point yosys at a Liberty file (`abc -liberty cells.lib`,
e.g. a free Sky130 / Nangate `.lib`) instead of the `-g` generic-gate mapping.
