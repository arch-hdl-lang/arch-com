<!-- GENERATED FILE. DO NOT EDIT BY HAND.
     Regenerate with `arch ops --markdown > doc/generated/pipelined_ops.md`
     (or `scripts/gen_pipelined_ops_doc.sh`).
     Source of truth: src/pipelined_ops.rs::BUILTIN_REGISTRY. -->

# Pipelined-operator registry

Generated listing of the compiler's pipelined-operator implementation registry (`doc/proposal_pipelined_operators.md`). This enumerates what `<pipelined, N>` call sites can resolve today; it is intentionally kept out of the normative spec because it churns as implementations are added (phase 5 generalizes beyond `fma`).

| operator | profile | stages | status | fmax (ng45, typ) | impl | notes |
|---|---|---|---|---|---|---|
| `fma` | FP32 | 6 | verified | ~260 MHz | `builtin:fma_f32_s6` | sticky-fold FMA, buffered (Yosys abc: buffer -N 8; upsize; dnsize); 6-stage is the characterized knee vs. 7/10 stages |
