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
tests/fp_v1/synth/run_synth.sh [outdir]
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

## Results (yosys 0.33, `abc -fast`, 2-input generic gates)

| operator | gate cells (area proxy) | logic depth (levels) |
|---|---:|---:|
| `bf16_to_f32` (widen) | 30 | 5 |
| `f32_to_bf16` (narrow) | 90 | 9 |
| `bf16_mul` | 2,242 | 143 |
| `f32_mul` | 5,948 | 147 |
| `f32_add` | 4,270 | 183 |
| `f32_sub` | 4,269 | 183 |
| `bf16_add` | 3,797 | 192 |
| `bf16_sub` | 3,764 | 194 |
| `bf16_fma` | 32,077 | 236 |
| `f32_fma` | 38,064 | 235 |

### Reading it

- **Conversions are nearly free** (depth 5–9) — field manipulation + one round.
- **Multiply is shallower than add** (147 vs 183). The bounded f32 adder's serial
  chain (exponent compare → barrel-shift align → add → leading-zero count →
  normalize shift → round) is the long pole; the multiplier's partial-product
  tree reduces in ~log depth.
- **bf16 arith ≈ f32 arith in depth**, because bf16 = widen→f32 op→narrow: it
  *reuses* the f32 datapath. bf16 has fewer cells (narrower significand) but the
  same critical-path structure, and add/sub edge slightly deeper than f32 from
  the extra narrow stage.
- **FMA is the critical operator** — depth ~235, ~32–38k gates — from the
  exact-wide 470-bit alignment + normalization (the design trades area/depth for
  not needing sticky-fold logic; the same width that keeps the Lean proof clean).

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
