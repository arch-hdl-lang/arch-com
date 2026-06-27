import ArchFpEquiv.Model
import ArchFpEquiv.Spec
import ArchFpEquiv.RoundProof
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

Elaborated and built under Lean v4.30.0 (`lake build` clean, **zero `sorry`**).
Two tiers:

* **Structural model lemmas** (no IEEE spec needed) — **machine-checked by
  `bv_decide`**: comparator symmetry/mirror, the `sub = add∘negate` construction
  identity, and full f32-adder **commutativity** (the heaviest, ~45 s in
  `cadical`). These prove `bv_decide` reasons about the *real* emitted operators,
  and that the bit-blast is genuine (commutativity is non-symmetric, so an
  abstraction shortcut could not have faked it).
* **IEEE arithmetic correctness** — for `mul`, the special-value lattice and the
  finite-product reduction are **proved** in `Spec.lean`, and the shared rounder
  lemma `arch_round48_correct` is now **fully proved** (`RoundProof.struct_eq_spec`):
  `arch_round48` is transcribed bit-exact (`bv_decide`) to a named-stage struct,
  which is then proved equal to the value-level RNE spec `roundNE_f32` via a stack
  of core-only kernels and bridges — no Mathlib, no floating-point library. The one
  precondition is the multiply-relevant exponent window `-298 ≤ e0 ≤ 208` (outside
  it arch's 16-bit exponent arithmetic genuinely wraps), discharged at the use site
  from `finiteNonzero` by `Spec.e0_bounds`. `arch_f32_mul_finite_correct` is then
  *derived* — not bit-blasted. Being op-independent, the rounder lemma also unlocks
  `add`/`fma` once they are reduced like `mul`; `add`/`sub` are moreover already
  machine-proved over all 2^64 inputs by the SMT backend.

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

-- `roundNE_f32` is now a concrete value-level RNE spec (`RoundProof.lean`), no
-- longer opaque; the `sig=0` case is discharged there (`round48_correct_zero`).

/-- **The rounder crux — now proved.** The shared round-and-pack at the multiply
    width rounds its dyadic argument `(-1)^s · sig · 2^e0` to nearest-even.

    This was the single `sorry` gating Tier-2 multiply; it is now discharged
    (`rw [arch_eq_struct]; exact struct_eq_spec`). `Round.lean` machine-checks,
    exhaustively, that `arch_round48`:
    preserves sign (`round48_sign`), sends a zero significand to signed zero
    (`round48_zero`), and is the **identity on every representable value**
    (`round48_exact_normal` / `round48_exact_subnormal`) — i.e. this equation
    already holds on the entire *exact* sub-domain (no rounding error), with the
    optimized clz / appended-sticky datapath bit-blasted. The residual is only the
    rounding **direction** for *inexact* arguments (nearer neighbour, ties-to-
    even), and that residual is now reduced to a stack of **proved** kernels and
    bridges (all core-only, no Mathlib):
      • `Round.msb_index_bound` / `msb_index_eq_log2` — normalization: arch's clz
        finds the true MSB and equals `Nat.log2`.
      • `RoundCore.rne_matches` — rounding: guard/round/sticky = round-to-nearest-
        even integer division, with `guard_bit_eq` / `div_eq_one_of_lt_two_mul`.
      • `RoundBridge.roundupBit_toNat` / `round_step` — arch's *actual* BitVec
        rounding datapath `(v >>> sh) + roundup` equals `rneQuot v.toNat sh`.
      • `RoundBridge.toInt_add_of_bounds` / `toInt_sub_of_bounds` — the 16-bit
        signed exponent arithmetic (`ev`, `biased`, `k`, `sh`) matches `Int`.
    The **final assembly** is now complete (`RoundProof.struct_eq_spec`): the proof
    transcribes `arch_round48` to a named-stage `round48_struct` (validated bit-exact
    by `bv_decide`, `arch_eq_struct`), then threads the bridges through every
    sig=0 / subnormal / normal / overflow packing case to match `roundNE_f32`. The
    one genuine precondition is the multiply-relevant exponent window
    `-298 ≤ e0 ≤ 208`: outside it arch's 16-bit exponent arithmetic wraps and the
    equation genuinely fails, so the bound is a hypothesis here and is discharged at
    the use site (`Spec.e0_bounds`) from `finiteNonzero`. Op-independent, so it also
    unlocks `add`/`fma` once they are reduced like `mul`. -/
theorem arch_round48_correct (s : BitVec 1) (sig : BitVec 48) (e0 : BitVec 16)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    arch_round48 s sig e0 = roundNE_f32 (s == 1#1) sig.toNat e0.toInt := by
  rw [arch_eq_struct]
  exact struct_eq_spec s sig e0 hlo hhi

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
  obtain ⟨hlo, hhi⟩ := e0_bounds a b ha hb
  exact arch_round48_correct _ _ _ hlo hhi

end ArchFp
