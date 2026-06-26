# ARCH FP — Lean equivalence backend (prototype)

A third renderer of the **one** in-Rust description of each FP operator. The
operators are defined once in `src/fp_ops.rs` against the shared bit-vector IR
(`src/fp_ir.rs`); that IR renders to:

| renderer | output | consumer |
|---|---|---|
| `render_sv` | synthesizable SystemVerilog | `arch build` |
| `render_smt` | SMT-LIB2 `define-fun`s | `arch formal` / z3 / cvc5 (`tests/fp_test.rs`) |
| **`render_lean`** | **Lean 4 `BitVec` `def`s** | **this project** |

Because all three come from the same source, a Lean proof here transfers to the
synthesized RTL with no hand-transcription — the same guarantee the SMT backend
already gives, extended to a structured prover.

## Why add Lean to a working SMT campaign

The SMT backend discharges, *exhaustively*, everything that has no multiplier:
f32 compares / conversions / `add` / `sub` over all 2^64 inputs, and all bf16
arithmetic over 2^32 (see `tests/fp_v1/smt_proof/README.md`). The wall is the
**multiplier-bearing** f32 ops, `mul` and `fma`: a 24×24 multiplier-equivalence
miter is SAT-hard for any bit-blaster — z3, cvc5, and Lean's `bv_decide` alike.

A structured prover clears that wall by *not bit-blasting*. Following the FLoPS /
Flocq "triangle of correctness", the proof lifts the bit pattern to an algebraic
`(sign, significand, exponent)` view with a rational/real value and shows the
operator computes `RoundToNearestEven(a·b)` structurally — the 24×24 array
collapses to a single `Nat`/`Int` multiply whose algebra Mathlib already knows.
That reasoning has no natural home in QF_BV; it does in Lean.

## Layout

```
lakefile.toml            dependency-free (Lean core BitVec + bv_decide; no Mathlib)
lean-toolchain           leanprover/lean4:v4.30.0  (matches proofs/lean_thread_lowering)
ArchFpEquiv.lean         root: imports Model + Equiv
ArchFpEquiv/Model.lean   GENERATED snapshot of every operator (regen below)
ArchFpEquiv/Equiv.lean   the correctness statements (sorry-stubbed scaffold)
scripts/regen_model.sh   re-render Model.lean from the IR
```

## Regenerate the model

```
proofs/lean_fp_equiv/scripts/regen_model.sh
# == cargo run --release --example dump_fp -- lean  (+ provenance header)
```

`Model.lean` is checked in as a snapshot so the project reads standalone, but it
is generated — never edit it by hand; change `src/fp_ops.rs` and regenerate.

## Status — honest

- **Verified here (`cargo test`):** the `render_lean` renderer and its output
  (`src/fp_ir.rs` unit tests `renders_both_dialects`, `lean_renders_every_op_kind`).
- **Pending a Lean toolchain run:** no `lake`/`lean` is available in the
  environment that authored this, so `Model.lean` has not been elaborated and the
  `Equiv.lean` proofs are `sorry`-stubbed (each carries its intended tactic).
  Two tiers of goal:
  - *Structural model lemmas* (comparator symmetry, `lt`/`gt` mirror) — decidable,
    should fall to `bv_decide` directly; they validate the model is usable.
  - *IEEE correctness* (`mul`/`fma`/`add`) — need a floating-point semantics for
    the spec side (`opaque f32_spec_*`), supplied by a future Mathlib/Flocq
    development. `mul`/`fma` are the multiplier frontier; `add` is the cross-check
    (already machine-proved in SMT) and the simplest worked example of lifting.

To build once a toolchain is present:

```
cd proofs/lean_fp_equiv && lake build
```
