<!-- GENERATED FILE. DO NOT EDIT BY HAND.
     Regenerate with `arch ops --markdown > doc/generated/pipelined_ops.md`
     (or `scripts/gen_pipelined_ops_doc.sh`).
     Source of truth: src/pipelined_ops.rs::BUILTIN_REGISTRY. -->

# Pipelined-operator registry

Generated listing of the compiler's pipelined-operator implementation registry (`doc/proposal_pipelined_operators.md`). This enumerates what `<pipelined, N>` call sites can resolve today; it is intentionally kept out of the normative spec because it churns as implementations are added (phase 5 generalizes beyond `fma`).

| operator | profile | stages | status | fmax (ng45, typ) | impl | notes |
|---|---|---|---|---|---|---|
| `fma` | FP32 | 6 | verified | ~260 MHz (external run — see notes) | `builtin:fma_f32_s6` | sticky-fold FMA; EXTERNAL Nangate45 (typ.) Yosys+OpenSTA+Liberty characterization, buffered abc flow (buffer -N 8; upsize; dnsize) — not reproducible by this repo's checked-in flow (no Liberty/OpenSTA in the dev/CI sandbox); full depth sweep: 5/6/7 stages form a 260-268 MHz plateau with the best point at 7 stages (268 MHz) — a 7-stage schedule row is a phase-5 candidate; this 6-stage row measures ~260 MHz. TWO emission forms (proposal §4): the default comb+cascade (retime-friendly RTL; also what `arch sim` runs) measures ~113 MHz on Yosys/ABC, which does NOT retime it (flops never move); `arch build --staged-ops` emits the hand-staged datapath this row's ~260 MHz characterizes. Staged↔cascade equivalence is discharged by the randomized lock-step regression (tests/pipelined_fma_lockstep_test.rs). Reproducible logic-depth proxy (not fmax): tests/fp_v1/synth/run_synth.sh --stages 6 F32Fma (tests/fp_v1/synth/README.md 'Staged/pipelined operators') |
