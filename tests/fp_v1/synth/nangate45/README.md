# Nangate45 timing flow — the paper's characterization, archived

This directory archives the exact synthesis + static-timing flow behind the
numbers in *Formally Verified Synthesizable Floating-Point Data Types in ARCH
HDL* (per-operator combinational fmax table, FMA pipeline-depth sweep, and the
`fma<pipelined, N>` registry annotations). It replaces the earlier state where
those figures came from an un-archived external run.

## Flow

Per operator (combinational):

1. Write a single-operator `.arch` module (see `../run_synth.sh` for the exact
   module texts) and `arch build` it to SystemVerilog.
2. `python3 ../hoist_decls.py < MODULE.sv > MODULE_h.v` (yosys-friendly decls).
3. Yosys with `flow_comb.ys.tmpl` (substitute `MODULE`, `LIB`): `synth
   -flatten`, then `abc -liberty LIB -script abc_buffered.script`.
4. OpenSTA with `sta_comb.tcl.tmpl`: fmax = 1000 / (worst path delay in ns)
   against a 100 ns virtual clock, zero I/O delays.

Staged (registered) modules use the same synthesis and `sta_seq.tcl.tmpl`
(real clock on port `clk`).

**The buffered ABC script is the point.** Yosys's default `abc -liberty` does
no fanout buffering or post-mapping sizing; high-fanout nets then dominate the
report and understate fmax by up to 3.5x (exact-wide FMA: 45 MHz unbuffered vs
160 MHz buffered). `abc_buffered.script` is:

```
strash
dch -f
map
buffer -N 8
upsize
dnsize
```

For two BF16 netlists (`Bf16Add`, `Bf16Sub`) ABC's `dch -f` aborts; substitute
`dc2` for the `dch -f` line for those two operators. The mapping and repair
stages, which carry the timing result, are identical for all operators.

## Library provenance (not vendored here)

The Liberty file is the Nangate Open Cell Library (45 nm), **typical corner**,
as distributed with OpenROAD-flow-scripts (`platforms/nangate45/lib/`):

- file: `NangateOpenCellLibrary_typical.lib`
- sha256: `8d540a4d4cf6d09d27c87ad067857a9c0c2eeb023ab7a56e058cd3113db4e9b1`

It is freely redistributable but third-party, so it is referenced by hash
rather than vendored. Point `LIB` at your copy.

## Tool versions used for the published numbers

- Yosys `0.67+post` (git sha1 `b8e7da6f`), Homebrew build
- OpenSTA `3.1.0`
- ABC: the copy bundled with the above Yosys

## Reproduction check (2026-07-26)

Reconstructed-flow validation against the published numbers:

- `F32FmaPipe7` (7-stage hand-staged FMA, the archived measurement source):
  **268.0 MHz** — matches the published 268 MHz to three digits.
- `F32Add` regenerated from current `main`: **329 MHz** vs the published
  320 MHz (+2.8%) — the flow is identical; the small delta is compiler-side
  RTL drift since the measurement-era commit, not flow drift.

Register placement for the staged FMA is the compiler's cut-point schedule
(`src/pipelined_ops.rs`, `FMA_F32_S6_SCHEDULE` and the sweep variants): stages
are cut at fixed linearization temp indices; internal layers are reset-free
with a 1-bit validity chain at the binding site (see the module docs).

## FP8 operators — combinational fmax (2026-08-01)

Same flow, same tool versions as the published numbers (Yosys 0.67+post
`b8e7da6f`, OpenSTA 3.1.0, ABC bundled; Nangate45 typical, hash above).
Anchors re-run in the same session for a same-machine baseline.

| operator | fmax (MHz) | delay (ns) | area (µm²) |
|---|---:|---:|---:|
| `e4m3_to_f32` (widen) | 7221 | 0.14 | 73 |
| `e5m2_to_f32` (widen) | 5893 | 0.17 | 78 |
| `f32_to_e4m3` (narrow) | 848 | 1.18 | 638 |
| `f32_to_e5m2` (narrow) | 856 | 1.17 | 493 |
| `e5m2_mul` | 1046 | 0.96 | 592 |
| `e4m3_mul` | 846 | 1.18 | 619 |
| `e4m3_sub` | 663 | 1.51 | 754 |
| `e4m3_add` | 639 | 1.56 | 728 |
| `e5m2_add` | 572 | 1.75 | 809 |
| `e5m2_sub` | 571 | 1.75 | 828 |
| `e4m3_fma` | 339 | 2.95 | 2,332 |
| `e5m2_fma` | 294 | 3.40 | 2,305 |
| *anchor* `f32_add` | 329 | 3.04 | 2,271 |
| *anchor* `bf16_fma` | 186 | 5.37 | 5,207 |
| *anchor* `f32_fma` | 199 | 5.04 | 12,361 |

Reading it:

- **Every fp8 binary op clears the fastest f32 op** (`e5m2_mul` at 1 GHz+,
  adds at 570–660 MHz vs `f32_add` 329 MHz). The RTL routes
  widen→f32-op→narrow, but synthesis constant-propagates the widen: the fp8
  operands occupy only the top 3–4 mantissa bits of the f32 datapath, so the
  24×24 multiplier collapses to a ~5×5 and the aligner shrinks with it —
  the "f32 datapath inside" costs nothing after optimization.
- **fp8 fma is ~1.6× faster and 2.2–5.3× smaller than the bf16/f32 fmas**
  (294–339 MHz, ~2.3 kµm² vs 186–199 MHz, 5.2–12.4 kµm²).
- **E4M3 arith beats E5M2** (add 639 vs 572 MHz, fma 339 vs 294): E5M2's
  wider exponent range means a wider alignment shifter — the extra exponent
  bit costs more than the extra mantissa bit saves.
- **Widens are wiring + a mux** (sub-0.2 ns): fp8→f32 is exact field
  expansion, no rounding.

ABC fragility notes (same class as the `dch -f` abort documented above for
`Bf16Add`/`Bf16Sub`):

- `E5m2Mul`: `dch -f` aborts; substitute `dc2` (as for the bf16 pair).
- `E5m2Fma`: both `dch -f` and `dc2` abort; drop the restructuring line
  entirely (script = `strash; map; buffer -N 8; upsize; dnsize`). The
  mapping and repair stages, which carry the timing result, are unchanged;
  the missing restructuring can only make its 294 MHz slightly pessimistic.
