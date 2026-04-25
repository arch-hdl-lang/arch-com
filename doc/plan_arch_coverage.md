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

### Phase 2 — line coverage

Counter per \`<=\` and \`=\` statement in seq/comb. Often subsumes branch coverage but distinct: a branch may be entered without all its statements firing (early-return-style patterns).

### Phase 3 — FSM state + transition coverage

For each \`fsm\`: a counter per state (entries) and per transition arc. Output: which states never entered, which transitions never taken.

### Phase 4 — toggle coverage

For each scalar wire/reg, count 0→1 and 1→0 transitions. Useful for catching tied-off signals. Costly (per-bit per-cycle), so opt-in via \`--coverage=toggle\` separate from default \`--coverage\`.

### Phase 5 — Verilator-compatible \`coverage.dat\`

Emit Verilator's \`# SystemC: …\`-prefixed format alongside the text report so users can run \`verilator_coverage --annotate-min 1 --annotate annot/ coverage.dat\` and get HTML annotation against the *.sv files. (The arch source lines won't match SV lines exactly, but the text report from phases 1-4 gives the arch-source view; the .dat gives the SV view.)

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
