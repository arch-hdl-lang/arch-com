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
