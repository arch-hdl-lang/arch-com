import ArchFpEquiv.Spec
import Std.Tactic.BVDecide

/-!
# `arch_f32_add` — IEEE-754 special-value correctness (first module)

The correctly-rounded correctness of `arch_f32_add` splits, exactly like the
`mul` proof in `Spec.lean`, into a **special-value lattice** (this file) and a
**finite-region "= nearest"** proof (future work — the analogue of the FMA value
development). Each special-value corner is machine-checked by `bv_decide`
bit-blasting the full adder (tractable: no multiplier; cf. `arch_f32_add_comm`).

This is the foundation for the Phase-2 `(1+δ)` per-add bound of
`proofs/lean_fp_equiv/SCALED_DOT_ACCUMULATION_SCOPE.md`: the finite lemma to
come feeds the relative-error bound, and these special cases pin the NaN/Inf
propagation the block value rule relies on.
-/

namespace ArchFp

set_option maxHeartbeats 4000000

/-- Signed infinity `s ++ 11111111 ++ 0…0`. -/
def sInf (s : BitVec 1) : BitVec 32 := ((BitVec.setWidth 32 s) <<< 31) ||| 0x7F800000#32

/-- Signed zero `s ++ 0…0`. -/
def sZero (s : BitVec 1) : BitVec 32 := (BitVec.setWidth 32 s) <<< 31

/-! ## NaN propagation -/

theorem add_nan_left (a b : BitVec 32) (h : isNaN a = true) :
    arch_f32_add a b = cNaN := by
  unfold isNaN expField fracField cNaN at *; unfold arch_f32_add; bv_decide

theorem add_nan_right (a b : BitVec 32) (h : isNaN b = true) :
    arch_f32_add a b = cNaN := by
  unfold isNaN expField fracField cNaN at *; unfold arch_f32_add; bv_decide

/-! ## Infinities -/

/-- `∞ + (−∞) = NaN` (opposite-sign infinities). -/
theorem add_inf_inf_opp (a b : BitVec 32)
    (ha : isInf a = true) (hb : isInf b = true) (hs : sgn a ≠ sgn b) :
    arch_f32_add a b = cNaN := by
  unfold isInf sgn expField fracField cNaN at *; unfold arch_f32_add; bv_decide

/-- `∞ + ∞ = ∞` (same-sign infinities). -/
theorem add_inf_inf_same (a b : BitVec 32)
    (ha : isInf a = true) (hb : isInf b = true) (hs : sgn a = sgn b) :
    arch_f32_add a b = sInf (sgn a) := by
  unfold isInf sgn expField fracField sInf at *; unfold arch_f32_add; bv_decide

/-- `∞ + finite = ∞`. -/
theorem add_inf_finite (a b : BitVec 32)
    (ha : isInf a = true) (hnn : isNaN b = false) (hni : isInf b = false) :
    arch_f32_add a b = sInf (sgn a) := by
  unfold isInf isNaN sgn expField fracField sInf at *; unfold arch_f32_add; bv_decide

/-- `finite + ∞ = ∞`. -/
theorem add_finite_inf (a b : BitVec 32)
    (hb : isInf b = true) (hna : isNaN a = false) (hia : isInf a = false) :
    arch_f32_add a b = sInf (sgn b) := by
  unfold isInf isNaN sgn expField fracField sInf at *; unfold arch_f32_add; bv_decide

/-! ## Zero identities -/

/-- `x + 0 = x` for finite non-zero `x` (either signed zero addend). -/
theorem add_zero_right (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : isZero b = true) :
    arch_f32_add a b = a := by
  unfold finiteNonzero isNaN isInf isZero expField fracField at *
  unfold arch_f32_add; bv_decide

/-- `0 + x = x` for finite non-zero `x`. -/
theorem add_zero_left (a b : BitVec 32)
    (ha : isZero a = true) (hb : finiteNonzero b = true) :
    arch_f32_add a b = b := by
  unfold finiteNonzero isNaN isInf isZero expField fracField at *
  unfold arch_f32_add; bv_decide

/-! ## Finite region — exact and round-off cases (proved)

The general finite `(1+δ)` bound needs the magnitude development (below), but
several finite cases are exact or structural and fall to `bv_decide` directly.
They are the base cases of that bound — the exact ones give `δ = 0`, and
`add_negligible` is the round-off fact the *summation* error bound rests on
(a term ≥ 2²⁵× smaller than the running sum vanishes, so a length-N sum can only
accumulate error from the ⌈log₂N⌉ additions that actually shift). -/

/-- **Self-addition is exact.** For normal `x` (`1 ≤ exp < 254`), `x + x = 2x`
    bit-exactly: same sign and significand, exponent incremented. `δ = 0`. -/
theorem add_self_exact (a : BitVec 32)
    (hlo : BitVec.ult 0#8 (expField a) = true)
    (hhi : BitVec.ult (expField a) 254#8 = true) :
    arch_f32_add a a = a + 0x00800000#32 := by
  unfold expField at *; unfold arch_f32_add; bv_decide

/-- **Sign preservation.** A same-sign finite-nonzero add cannot spuriously
    cancel: the result carries the operands' sign. -/
theorem add_same_sign_sign (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hs : sgn a = sgn b) : sgn (arch_f32_add a b) = sgn a := by
  unfold finiteNonzero isNaN isInf isZero sgn expField fracField at *
  unfold arch_f32_add; bv_decide

/-- **Negligible addend rounds away.** For same-sign normals with `b` at least
    `2²⁵×` smaller than `a` (exponent gap ≥ 25, `a` finite, `b` exp ≤ 229 so the
    gap test cannot wrap), `b` lands entirely below `a`'s round bit and
    `a + b = a`. This is the fact that makes a length-N summation accumulate
    error only from the additions that genuinely align — the crux of the
    `O(log N)` pairwise bound (Theorem B). -/
theorem add_negligible (a b : BitVec 32)
    (hna : BitVec.ult 0#8 (expField a) = true) (hha : BitVec.ult (expField a) 255#8 = true)
    (hnb : BitVec.ult 0#8 (expField b) = true) (hhb : BitVec.ule (expField b) 229#8 = true)
    (hs : sgn a = sgn b)
    (hgap : BitVec.ule (expField b + 25#8) (expField a) = true) :
    arch_f32_add a b = a := by
  unfold sgn expField at *; unfold arch_f32_add; bv_decide

/-! ## Finite region — the general `(1+δ)` bound (remaining target)

The exact/round-off cases above are the base cases; the general finite–finite
bound is the one the Phase-2 `(1+δ)` step needs:

> **Target.** For `finiteNonzero a`, `finiteNonzero b`, with the exact sum in the
> normal range (no overflow to ∞, no flush to a subnormal), `arch_f32_add a b`
> is the FP32 value nearest to the exact real sum `val a + val b` — i.e.
> `IsNearestMag` of the exact aligned-sum magnitude (`RoundReal`), from which
> `|val (arch_f32_add a b) − (val a + val b)| ≤ u · |val a + val b|` follows.

This mirrors the FMA value development (`FmaValue`/`FmaInvariance`/`RneValue`,
~3.4k lines) but is materially smaller: no 24×24 product, so the pre-round
significand is a single aligned add (≤ 56 bits, exact), and the existing round
kernel (`RneValue.rneQuot_halfulp`) supplies the half-ULP bound directly. The
proof plan: (1) show the adder's pre-round 56-bit significand equals the exact
aligned sum; (2) show the normalize + `_t149` round step equals `rneQuot` of it;
(3) apply `rneQuot_halfulp`; (4) convert the Nat half-ULP bound to the `Rat`
relative `(1+δ)`. Steps (1)–(2) are `bv_decide`-shaped (bounded, no multiplier);
(3)–(4) are the algebraic bridge. This is multi-session and is tracked as the
open Phase-2 item in the scope doc. -/

end ArchFp
