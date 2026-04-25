# Plan: Sim sub-instance inlining

> **Status**: queued. Identified during perf tuning (PR #160) as the
> real path to close the ~2.6× single-thread perf gap to Verilator
> on `tests/axi_dma_thread/ThreadMm2s.arch`. Settle-hoist + LTO
> shipped in #160 with modest gains; this doc captures the bigger
> structural refactor for a future session.

## Motivation

`arch sim` (fsm path) on ThreadMm2s: ~6.5 Mcyc/s.
Verilator on the same SV (output of `arch build`): 17 Mcyc/s.

Most of the gap comes from the way arch sim emits sub-instances
(`inst x: Sub`) — they live in their own C++ class, in their own
translation unit, with explicit mirror copies of every input/output
between the top class and the sub-class on every cycle.

For ThreadMm2s the picture per cycle:
1. Top class calls `_inst__threads.eval_comb()` (cross-TU function call).
2. Before calling: copies ~10 input ports from top fields to sub's
   input fields.
3. After calling: copies ~10 output ports back from sub fields to
   top fields.
4. This whole dance happens in a 2-iteration settle loop, then again
   after `eval_posedge()`. So **~80 mirror copies + 4 cross-TU calls
   per cycle**.

Verilator just inlines the whole thing.

## Design

Identify single-instance sub-modules (the common case for thread
lowering: each module gets one `_<TopName>_threads` sub-inst) and
**fold them into the top class entirely**:

- Sub's regs/lets/wires become **top-class fields** (with renaming
  to avoid collisions: e.g. `sub.reg_x` → `__threads__reg_x`).
- Sub's `eval_comb()` body is emitted inline at the top's
  `eval_comb()` call site (no function call).
- Sub's `eval_posedge()` body is emitted inline at the top's
  `eval_posedge()` call site (after the top's own posedge logic).
- Connection mirroring vanishes — sub reads top fields directly,
  writes top fields directly.

## When to apply

Required preconditions for safe inlining:
- Sub-module is instantiated **exactly once** in the top.
- Sub-module is not referenced by name from any TB (no public
  pybind11 wrapper, no `--debug --depth >1` consumer expecting
  a separate `[SubName.port]` namespace in traces).
- Sub-module has no `pybind` mode active.

The "exactly once" check covers the vast majority of arch designs
(single-thread-block module → one threads-sub-inst). For modules
with multiple instantiations of the same sub (e.g. RAM banks), the
sub stays as its own class — inlining doesn't apply.

## Implementation sketch

1. **AST analysis pass**: walk all module bodies, count `inst`
   declarations per module name, mark single-instance subs as
   "inlining candidates."
2. **Storage merging**: when emitting top-class fields, after
   normal regs/lets/wires, append all fields from each candidate
   sub-inst, prefixed with `_<inst_name>__`.
3. **Method inlining**: when a top method needs to invoke the
   sub-inst's `eval_comb()` / `eval_posedge()`, emit the sub's
   body inline (with name rewrites) instead of a call. The
   sub's local-name-to-field-name mapping uses the same
   `_<inst_name>__` prefix.
4. **Skip emit of separate sub class**: don't generate a
   `V_<SubName>.h` / `.cpp` if the sub is fully inlined.
5. **Debug log**: top's `_debug_log_ports` emits all merged
   regs/ports, with the prefix in the VCD names so they remain
   visible in the waveform.

## Estimated effort

500-800 LOC of emitter changes, spanning:
- `gen_module` in `src/sim_codegen/mod.rs` — biggest delta
- Field declaration, name resolution, method-call lowering all
  need to know about the merged-vs-not state
- `emit_inst` paths and their mirror loops are mostly removed

Worth a focused 2-3 sessions when picked up. Has its own design
doc + reviewer sign-off cycle before coding.

## Estimated payoff

ThreadMm2s would go from ~6.5 Mcyc/s closer to Verilator's 17 Mcyc/s.
Realistic target: 10-13 Mcyc/s (1.5-2× current), still some gap to
Verilator from sub-cycle codegen quality but most of the structural
overhead removed.

For thread-sim parallel-N=1 path: same kind of opportunity since
it inherits the same sub-instance plumbing.

## Out of scope

- Multiple-instance subs (e.g. arrays of sub-modules): keep the
  separate class.
- Sub-modules with their own pybind11 wrapper: keep the separate
  class so pybind sees the public API.
- LTO is already enabled in #160 (modest cross-TU help); this
  refactor goes further by removing the cross-TU boundary entirely.
