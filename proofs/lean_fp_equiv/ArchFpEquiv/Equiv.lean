import ArchFpEquiv.Model
import Std.Tactic.BVDecide

/-!
# ARCH FP — Lean equivalence backend (scaffold)

This file states the IEEE-754 correctness goals for the emitted FP operators
*against the real generated model* (`ArchFpEquiv.Model`, rendered from the same
`src/fp_ops.rs` IR as the synthesized SystemVerilog and the SMT-LIB2 proofs).

## Why a Lean backend at all

The SMT campaign (`tests/fp_test.rs`, z3/cvc5) already discharges everything
*without a multiplier* exhaustively: f32 compares / conversions / `add` / `sub`
(over all 2^64 inputs), and all bf16 arithmetic (2^32). What it cannot close is
the **multiplier-bearing** f32 ops — `mul` and `fma`. A 24×24 multiplier
equivalence is SAT-hard for *any* bit-blaster (z3, cvc5, and Lean's `bv_decide`
alike): the CNF blows up and the solver times out.

The escape is to stop bit-blasting. Following the FLoPS / Flocq methodology
(the "triangle of correctness"): lift the bit pattern to an algebraic
`(sign, significand, exponent)` view, give it a rational/real value, and prove
the operator computes `RoundToNearestEven (a_val * b_val)` *structurally* — the
multiplier becomes one `Nat`/`Int` multiplication whose properties Mathlib
already knows, never a bit array to enumerate. That reasoning lives naturally in
Lean, not in a QF_BV solver.

## Status

Elaborated and built under Lean v4.30.0 (`lake build` clean; the only warnings
are the three Tier-2 `sorry`s below). Two tiers:

* **Structural model lemmas** (no IEEE spec needed) — **machine-checked by
  `bv_decide`**: comparator symmetry/mirror, the `sub = add∘negate` construction
  identity, and full f32-adder **commutativity** (the heaviest, ~45 s in
  `cadical`). These prove `bv_decide` reasons about the *real* emitted operators,
  and that the bit-blast is genuine (commutativity is non-symmetric, so an
  abstraction shortcut could not have faked it).
* **IEEE correctness theorems** (`sorry`) — need a floating-point *semantics* for
  the spec side. Lean core has none, so `f32_spec_*` below is `opaque`, standing
  in for the value a real development imports from Mathlib (or ports from the SMT
  `FloatingPoint` theory / Flocq). `mul`/`fma` are the multiplier frontier; `add`
  is the cross-check (already machine-proved over 2^64 by the SMT backend) and
  the simplest worked example of the lifting argument.

The Rust renderer (`fp_ir::render_lean`) and its emitted output are additionally
covered by `cargo test`.
-/

namespace ArchFp

/-! ## Tier 1 — structural model lemmas (machine-checked by `bv_decide`)

These need no floating-point spec: they are pure `BitVec` facts about the
emitted operators, so `bv_decide` bit-blasts them against the generated `Model`
defs and `cadical` discharges each. They prove the tactic is wired to the *real*
operators, not a paraphrase. -/

/-- The emitted equality comparator is symmetric (holds even at NaN, since the
    `mant ≠ 0 ∧ exp = 0xFF` NaN test makes `a == b` false both ways). -/
theorem arch_f32_eq_comm (a b : BitVec 32) :
    arch_f32_eq a b = arch_f32_eq b a := by
  unfold arch_f32_eq
  bv_decide

/-- `<` and `>` are mirror images: `arch_f32_lt a b = arch_f32_gt b a`, by
    construction (`gt` is defined as `lt` with operands swapped in `fp_ops.rs`). -/
theorem arch_f32_lt_gt_mirror (a b : BitVec 32) :
    arch_f32_lt a b = arch_f32_gt b a := by
  unfold arch_f32_lt arch_f32_gt
  bv_decide

/-- bf16 equality is symmetric too (it routes through the f32 comparator after
    an exact widen). -/
theorem arch_bf16_eq_comm (a b : BitVec 16) :
    arch_bf16_eq a b = arch_bf16_eq b a := by
  unfold arch_bf16_eq arch_bf16_to_f32 arch_f32_eq
  bv_decide

/-- The bounded f32 adder is **commutative**: `a + b = b + a`. A genuine
    correctness property of the full datapath (operand-order pick, alignment,
    rounder), needing no IEEE spec — pure `BitVec`, so `bv_decide` bit-blasts the
    *entire* ~56-bit adder and `cadical` proves it (~45 s, hence the raised
    `timeout`). This is the heaviest goal discharged here, and the reason the
    comparison encoding had to be `BitVec.ofBool`, not a `Prop`-conditioned `ite`
    (which `bv_decide` abstracts, yielding spurious counterexamples on this
    non-symmetric goal). The multiplier ops resist exactly this bit-blast — they
    are why Tier 2 needs algebraic lifting. -/
theorem arch_f32_add_comm (a b : BitVec 32) :
    arch_f32_add a b = arch_f32_add b a := by
  unfold arch_f32_add
  bv_decide (config := { timeout := 600 })

/-- `sub` is `add` with the subtrahend's sign flipped — the exact construction
    in `fp_ops.rs` (`f32_add_core(.., flip_b_sign = true)`). Proving the two
    emitted functions satisfy this identity bit-for-bit validates that the shared
    adder core was instantiated correctly for subtraction. -/
theorem arch_f32_sub_is_add_neg (a b : BitVec 32) :
    arch_f32_sub a b = arch_f32_add a (b ^^^ (BitVec.ofNat 32 0x80000000)) := by
  unfold arch_f32_sub arch_f32_add
  bv_decide (config := { timeout := 180 })

/-! ## Tier 2 — IEEE-754 correctness (the multiplier frontier)

The spec side requires a floating-point semantics that Lean core does not
provide. `f32_spec_mul`/`_add`/`_fma` are `opaque` placeholders for
"the IEEE-754 round-to-nearest-even result, as a bit pattern (canonical NaN per
the active `--fp-compat` profile)". A real development replaces each with a
definition over a `Float32` algebraic model (Mathlib's `Rat`/`Real` rounding, or
a port of the SMT `FloatingPoint` theory / Flocq), at which point the theorems
below become provable — `mul`/`fma` by the algebraic-lifting argument above,
`add`/`sub`/compares by the same route (and already machine-checked in SMT). -/

/-- IEEE-754 binary32 RNE multiply, as a 32-bit pattern. Placeholder for a
    Mathlib/Flocq-backed semantics; see the module note. -/
opaque f32_spec_mul : BitVec 32 → BitVec 32 → BitVec 32

/-- IEEE-754 binary32 RNE add, as a 32-bit pattern. Placeholder. -/
opaque f32_spec_add : BitVec 32 → BitVec 32 → BitVec 32

/-- IEEE-754 binary32 RNE fused multiply-add `a*b + c`, as a 32-bit pattern.
    Placeholder. -/
opaque f32_spec_fma : BitVec 32 → BitVec 32 → BitVec 32 → BitVec 32

/-- **The multiplier frontier.** The emitted f32 multiplier equals IEEE-754 RNE
    over the entire input space. SAT-hard to bit-blast (z3/cvc5/`bv_decide` all
    time out); the route is to lift `arch_f32_mul` to `(sig, exp)` and reduce the
    24×24 array to one `Nat.mul`, then invoke a correct-rounding lemma. -/
theorem arch_f32_mul_correct (a b : BitVec 32) :
    arch_f32_mul a b = f32_spec_mul a b := by
  sorry

/-- The emitted FMA equals IEEE-754 RNE `a*b + c` (single rounding). Harder than
    `mul`: the exact product feeds a wide aligned add before the one rounding. -/
theorem arch_fma_f32_correct (a b c : BitVec 32) :
    arch_fma_f32 a b c = f32_spec_fma a b c := by
  sorry

/-- The bounded f32 adder equals IEEE-754 RNE add. Already machine-checked over
    2^64 inputs by the SMT backend (`F32_ADD`); kept here as the Lean cross-check
    and as the simplest worked example of the lifting argument. -/
theorem arch_f32_add_correct (a b : BitVec 32) :
    arch_f32_add a b = f32_spec_add a b := by
  sorry

end ArchFp
