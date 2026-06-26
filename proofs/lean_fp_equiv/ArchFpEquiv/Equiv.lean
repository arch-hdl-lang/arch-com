import ArchFpEquiv.Model
import ArchFpEquiv.Spec
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
* **IEEE arithmetic correctness** — for `mul`, the special-value lattice and the
  finite-product reduction are **proved** in `Spec.lean`; what remains is a single
  shared rounder lemma `arch_round48_correct` (the one `sorry`), stated against the
  value-level `roundNE_f32`. `arch_f32_mul_finite_correct` is then *derived* from
  the reduction and that lemma — not bit-blasted. The crux needs a floating-point
  *semantics* Lean core lacks (a dyadic/`Rat` model from Mathlib/Flocq, or a port
  of the SMT `FloatingPoint` theory); it is op-independent, so it also unlocks
  `add`/`fma`. `add`/`sub` are moreover already machine-proved over all 2^64 inputs
  by the SMT backend.

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

/-! ## Tier 2 — IEEE-754 arithmetic correctness

The arithmetic-correctness goal splits into a *special-value lattice* and a
*finite case*, and `Spec.lean` discharges everything in that split for `mul`
except a single shared rounder lemma:

* **Special values** (`Spec.mul_nan_left … mul_zero_right`) — the full IEEE
  multiply lattice (NaN propagation, `∞·0 = NaN`, `∞·x = ∞`, `0·x = 0`), each
  **machine-checked by `bv_decide`**. (These are exactly the corners the SMT
  backend left on the §8.2 differential backstop — `mul` is unproved there.)
* **Finite reduction** (`Spec.mul_finite_reduces`) — for two finite nonzero
  operands, **proved** `arch_f32_mul a b = arch_round48 sy (mant_a·mant_b) e0`:
  the model's multiply *is* the shared rounder applied to the exact 48-bit
  integer significand product. `bv_decide` closes it structurally (the 24×24
  multiplier sits identically on both sides — no SAT-hard multiplier-equivalence).

So all of Tier-2 multiply collapses onto one question: does `arch_round48` round
correctly? That is the lone remaining crux — op-independent (`mul`/`add`/`fma`
share the rounder), value-level, and the one place a bit-blaster cannot help.
It is stated below against `roundNE_f32` and needs the algebraic-lifting argument
(decode → real value → round). `add`/`sub` are additionally already
machine-proved against IEEE `fp.add`/`fp.sub` over all 2^64 inputs by the SMT
backend; `fma` reduces the same way once its wide aligned product is named. -/

/-- IEEE-754 round-to-nearest-even, value level: the correctly-rounded binary32
    bit pattern of the real number `(-1)^neg · sig · 2^e0`. Left `opaque` — a full
    development defines it via a dyadic/`Rat` model (Mathlib/Flocq, or a port of
    the SMT `FloatingPoint` theory). It is the *only* abstract object remaining in
    Tier-2 multiply; everything else is proved. -/
opaque roundNE_f32 : (neg : Bool) → (sig : Nat) → (e0 : Int) → BitVec 32

/-- **The rounder crux.** The shared round-and-pack at the multiply width rounds
    its dyadic argument `(-1)^s · sig · 2^e0` to nearest-even. This is the single
    `sorry` gating Tier-2 multiply: `mul`'s special values and its reduction to
    this function are both proven (`Spec`). Discharging it is the algebraic-lifting
    work (it is *not* bit-blastable: a 56-bit rounder against a value-level spec).
    Being op-independent, it also unlocks `add`/`fma` once they are reduced. -/
theorem arch_round48_correct (s : BitVec 1) (sig : BitVec 48) (e0 : BitVec 16) :
    arch_round48 s sig e0 = roundNE_f32 (s == 1#1) sig.toNat e0.toInt := by
  sorry

/-- **Finite multiply is correctly rounded** — derived, not bit-blasted. Combines
    the proved finite reduction (`Spec.mul_finite_reduces`) with the rounder crux:
    for finite nonzero `a b`, `arch_f32_mul a b` equals the RNE rounding of the
    exact real product `(mant_a · mant_b) · 2^(eunb_a + eunb_b)`. The only
    unproved input is `arch_round48_correct`; the multiplier itself is never
    re-examined here. -/
theorem arch_f32_mul_finite_correct (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) :
    arch_f32_mul a b
      = roundNE_f32 ((mulSign a b) == 1#1)
          ((BitVec.setWidth 48 (arch_decode_mant a)
              * BitVec.setWidth 48 (arch_decode_mant b)).toNat)
          ((arch_decode_eunb a + arch_decode_eunb b).toInt) := by
  rw [mul_finite_reduces a b ha hb, archMulFinite]
  exact arch_round48_correct _ _ _

end ArchFp
