import ArchFpEquiv.Spec
import ArchFpEquiv.FmaValue
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
bound is the one the Phase-2 `(1+δ)` step needs. **It is now proved** — not by
redeveloping the adder's magnitude reasoning, but by *reducing add to fma*.

The observation: `fma(a, 1.0, b) = a·1 + b = a + b`, with a single rounding, so a
correctly-rounded adder must return exactly `arch_fma_f32 a 1.0 b`. `bv_decide`
confirms that bit-for-bit (the constant `1.0` multiplicand collapses the fma
multiplier, so the bit-blast is tractable — no SAT-hard 24×24). Then the *already
proved* `arch_fma_f32_correct` (`FmaValue`, the ~3.4k-line development) transfers
verbatim: `arch_f32_add a b` is nearest to `fmaExact a 1.0 b`, which is exactly
the exact sum. No new magnitude development. -/

/-- **`arch_f32_add a b = fma(a, 1.0, b)`** for finite-nonzero operands. The
    constant `1.0 = 0x3F800000` makes the fma multiplier collapse, so `bv_decide`
    discharges the whole adder-vs-fma equivalence (~18 s). This is the bridge
    that lets add inherit the fma correctness theorem. -/
theorem add_eq_fma_one (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) :
    arch_f32_add a b = arch_fma_f32 a 0x3F800000#32 b := by
  unfold finiteNonzero isNaN isInf isZero expField fracField at *
  unfold arch_f32_add arch_fma_f32
  bv_decide (config := { timeout := 600 })

/-- `1.0` is finite and non-zero. -/
theorem finiteNonzero_one : finiteNonzero 0x3F800000#32 = true := by decide

/-- `f32SignedScaled 1.0 = 2¹⁴⁹` (`1.0 = 2⁰`, in `2⁻¹⁴⁹` units). -/
theorem sscaled_one : f32SignedScaled 0x3F800000#32 = 2 ^ 149 := by decide

/-- **`fmaExact a 1.0 b` is the exact sum.** In the fma's `2⁻²⁹⁸` units,
    `fmaExact a 1.0 b = (f32SignedScaled a + f32SignedScaled b)·2¹⁴⁹`, i.e. the
    exact real `a + b` (the `f32SignedScaled` values are in `2⁻¹⁴⁹` units). So the
    `IsNearestExact (fmaExact a 1.0 b) …` below is nearness to `a + b`. -/
theorem fmaExact_add (a b : BitVec 32) :
    fmaExact a 0x3F800000#32 b = (f32SignedScaled a + f32SignedScaled b) * 2 ^ 149 := by
  unfold fmaExact; rw [sscaled_one]; exact (Int.add_mul _ _ _).symm

/-- **`arch_f32_add` is correctly rounded (finite region).** For finite-nonzero
    `a`, `b` with a non-zero, non-overflowing exact sum, `arch_f32_add a b` is the
    finite FP32 pattern nearest to the exact real sum (as `fmaExact a 1.0 b`,
    which equals `(f32SignedScaled a + f32SignedScaled b)·2¹⁴⁹` — the exact
    `a + b`), carrying its exact sign. Proved by reducing to `arch_fma_f32_correct`
    through `add_eq_fma_one`. This is the `IsNearestExact` statement the Phase-2
    `(1+δ)` bound rests on; the only step left to Theorem B is the
    `IsNearestExact → Rat` relative-error conversion. -/
theorem arch_f32_add_correct (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hnz : fmaExact a 0x3F800000#32 b ≠ 0)
    (hovf : biasedFinal (arch_fma_mag a 0x3F800000#32 b).toNat
      (arch_fma_elo a 0x3F800000#32 b).toInt ≤ 254) :
    IsFiniteF32 (arch_f32_add a b)
      ∧ IsNearestExact (fmaExact a 0x3F800000#32 b) (arch_f32_add a b)
      ∧ BitVec.extractLsb 31 31 (arch_f32_add a b)
          = (if fmaExact a 0x3F800000#32 b < 0 then 1#1 else 0#1) := by
  rw [add_eq_fma_one a b ha hb]
  exact arch_fma_f32_correct a 0x3F800000#32 b ha finiteNonzero_one hb hnz hovf

end ArchFp
