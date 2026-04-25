# Plan: arch sim coverage

> **Status**: design + phased rollout. MVP (Phase 1) ships branch coverage with text report.

## Motivation

Verilator's `--coverage` instrument-then-replay model is the gold standard for HDL sim coverage: enables it at compile, runs unchanged, dumps `coverage.dat` at exit, post-processes with `verilator_coverage --annotate`. Arch sim should match that ergonomics: `arch sim --coverage Tb.arch --tb tb.cpp` and at exit you get a coverage report keyed to *.arch source lines (not the generated SV/C++).

## Phased rollout

### Phase 1 (MVP) — branch coverage

Each \`if\` / \`elsif\` / \`else\` arm in seq and comb blocks gets a counter. At sim exit, dump a per-arm hit count keyed to the source line. Smallest useful slice: tells you whether every branch in the design was exercised by your testbench.

- CLI: \`arch sim --coverage Tb.arch ...\`
- Instrument: in [src/sim_codegen/mod.rs], when emitting \`if (cond) { … } else if … else …\`, prefix each arm with \`_arch_cov[N]++;\` where N is a globally unique counter index. Maintain a sidecar map from N → (file, line, arm_kind).
- Emit a per-class \`_arch_cov\` array of \`uint64_t\` and a static initialization-time registration so \`final()\` (or atexit) can dump.
- Output: \`coverage.txt\` in the working directory:
  ```
  cache_mshr.arch:111 if fill_valid              : hit 14
  cache_mshr.arch:114 elsif dq_valid_r & ready   : hit 6
  cache_mshr.arch:116 if entry_has_next[dq_idx]  : hit 2
  cache_mshr.arch:118 else                       : hit 4
  ...
  Summary: 12/14 branches hit (85.7%)
  ```

### Phase 2 — block-execution coverage (NOT full line coverage)

Originally scoped as "line coverage", but full per-statement counters are mostly redundant with branch coverage in arch:
- Statements inside a branch arm are guaranteed to execute when the arm is hit (no early returns in seq/comb).
- Unconditional top-of-block statements are trivially hit if the block runs.
- For-loop bodies are bounded compile-time; if the enclosing branch ran, all iterations ran.

**The one case branch coverage doesn't catch**: a seq or comb block with no branches at all, where you'd want to know "did this block ever run?". Example:
```
seq on rare_clk
  counter <= counter + 1;
end seq
```
Branch coverage says \`0/0 = N/A\`. Useful coverage would say \`seq @rare_clk: 0 ticks\` so a wedged clock or a never-instantiated module shows up.

So Phase 2 ships **block-execution coverage**: one counter per top-level seq/comb block, incremented on every entry. Cheap, catches dead-block bugs, and stays semantically distinct from branch coverage. Full per-statement line coverage stays out of scope.

### Phase 3 — FSM state + transition coverage

For each \`fsm\`: a counter per state (entries) and per transition arc. Output: which states never entered, which transitions never taken.

### Phase 4 — toggle coverage

For each scalar wire/reg, count 0→1 and 1→0 transitions. Useful for catching tied-off signals. Costly (per-bit per-cycle), so opt-in via \`--coverage=toggle\` separate from default \`--coverage\`.

### Phase 5 — Verilator-compatible \`coverage.dat\`

Emit Verilator's \`# SystemC: …\`-prefixed format alongside the text report so users can run \`verilator_coverage --annotate-min 1 --annotate annot/ coverage.dat\` and get HTML annotation against the *.sv files. (The arch source lines won't match SV lines exactly, but the text report from phases 1-4 gives the arch-source view; the .dat gives the SV view.)

### Phase 6 — construct port toggle coverage (TODO)

Today toggle coverage (Phase 4/4b) instruments scalar/Vec \`reg\` declarations inside \`gen_module\` only. The non-module construct emitters (\`fifo\`, \`arbiter\`, \`ram\`, \`cam\`, \`linklist\`, \`pipeline\`, plus \`fsm\` datapath regs) install no \`CoverageRegistry\`, so their internals contribute zero coverage. For most designs this is fine — the producing reg in the wrapping module is already toggled — but at black-box construct boundaries (e.g. an \`inst sub: SomeFifo\`) there is no signal we can attribute toggles to.

Phase 6 fills that gap by toggling the **interface ports** of every construct from the *consumer* side: in \`gen_module\`, when emitting an instance, declare a \`_prev_<inst>_<port>\` shadow per output port and bump a popcount counter at the end of each \`eval()\`. Counter category \`v_toggle\`, comment \`toggle <inst>.<port>\`. This treats the construct as opaque (correct, since we don't own its internals) while still surfacing dead lanes / tied-off interfaces.

Skip in v1: bus ports (already flatten to multiple scalars — nice-to-have but the per-leaf shadow set gets verbose); wide ports >64b (split popcount).

Promote when a real consumer asks (most likely: arbiter grant-fairness audits, or fifo data-bus dead-bit hunting).

## Non-goals (v1)

- **Cross-test merging**: each \`arch sim\` run overwrites \`coverage.txt\`. \`verilator_coverage --merge\` semantics deferred until users ask.
- **Functional coverage** (\`covergroup\` / \`coverpoint\`): out of scope. Arch's \`cover\` blocks already lower to SVA \`cover property\`; hooking those into a runtime hit-counter is a separate feature.
- **Source-position-perfect spans**: phase 1 records line numbers. Column-accurate spans + multi-line if conditions deferred.

## Risks

| Risk | Mitigation |
|---|---|
| Counter increments slow down sim by 10×+ for hot loops | Counters are `uint64_t` increments — single instruction. Measure on cache_mshr; if regression > 5%, gate behind `--coverage` (which we already do — it's opt-in). |
| Sidecar map breaks when source files change | Embed (file, line, span) directly in the dump-time text-format emit. No sidecar lookup at sim time. |
| Atexit dump gets lost on crash / abort() | `final()` already exists for trace close; reuse it. For abort paths (bounds check fail), call dump from the abort handler too. |
| Per-class storage doesn't compose across multiple instances | Counters are per-class (static), so 100 instances of cache_mshr share one counter set. Matches Verilator. Per-instance is a future opt-in. |
