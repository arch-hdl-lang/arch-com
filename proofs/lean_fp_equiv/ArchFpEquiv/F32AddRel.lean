import ArchFpEquiv.F32AddCorrect
import Mathlib

/-!
# `arch_f32_add` relative-error `(1+δ)` layer

Builds the rational relative-error bound on top of `arch_f32_add_correct`
(`IsNearestExact` of the exact sum). This is the `Rat` layer of the Phase-2
`(1+δ)` step; it uses Mathlib for the ordered-field / division API that core
`Rat` lacks. The bit-level proofs stay Mathlib-free — only this analysis layer
imports it.

Value convention: `f32R z` is the signed real value of a finite FP32 pattern as
an exact `Rat` (finite FP32 are dyadic rationals). `f32SignedScaled z` is that
value in `2⁻¹⁴⁹` units (an `Int`), so `f32R z = f32SignedScaled z / 2¹⁴⁹`.
-/

namespace ArchFp

open scoped BigOperators

set_option exponentiation.threshold 600

/-- Signed real value of a finite FP32 pattern, as an exact rational. -/
noncomputable def f32R (z : BitVec 32) : Rat := (f32SignedScaled z : Rat) / 2 ^ 149

/-- FP32 unit roundoff `u = 2⁻²⁴`. -/
noncomputable def uF32 : Rat := 1 / 2 ^ 24

/-- The exact real sum `a + b` as a rational: the two values add. -/
theorem f32R_add (a b : BitVec 32) :
    f32R a + f32R b = ((f32SignedScaled a + f32SignedScaled b : Int) : Rat) / 2 ^ 149 := by
  unfold f32R
  push_cast
  ring

/-- **The exact sum in fma units.** `f32R a + f32R b = fmaExact a 1.0 b / 2²⁹⁸`,
    i.e. the exact rational sum equals the fma's exact value (in `2⁻²⁹⁸` units)
    rescaled. Bridges the `Rat` value layer to the integer `fmaExact` the
    `IsNearestExact` correctness is phrased against. -/
theorem f32R_add_eq_fmaExact (a b : BitVec 32) :
    f32R a + f32R b = ((fmaExact a 0x3F800000#32 b : Int) : Rat) / 2 ^ 298 := by
  rw [f32R_add, fmaExact_add]
  rw [div_eq_div_iff (by positivity) (by positivity)]
  push_cast
  ring

/-- **Algebra reduction (Mathlib).** The Nat half-ULP-relative bound
    `err · 2²⁴ ≤ V` lifts to the `Rat` relative bound `err/2²⁹⁸ ≤ u · (V/2²⁹⁸)`.
    This is the step that needs the ordered-field/division API core `Rat` lacks;
    it turns the (integer) half-ULP fact — the one remaining FP-grid obligation —
    into the `(1+δ)` form Theorem B consumes. -/
theorem rel_bound_of_scaled (e V : Nat) (hb : e * 2 ^ 24 ≤ V) :
    (e : Rat) / 2 ^ 298 ≤ uF32 * ((V : Rat) / 2 ^ 298) := by
  unfold uF32
  rw [div_mul_div_comm, one_mul, ← div_div]
  gcongr
  rw [le_div_iff₀ (by positivity)]
  exact_mod_cast hb

/-! ## Remaining for the per-add `(1+δ)` (two FP-specific facts)

With the value layer and `rel_bound_of_scaled` in place, the `(1+δ)` bound
`|f32R (add a b) − (f32R a + f32R b)| ≤ uF32 · |f32R a + f32R b|` needs exactly:

1. **Sign/magnitude bridge** — `|f32R (add a b) − (f32R a + f32R b)| =
   ↑(scaledDist (f32MagScaled (add a b) · 2¹⁴⁹) (fmaExact a 1.0 b).natAbs) / 2²⁹⁸`.
   Mechanical: combine `f32R_add_eq_fmaExact`, the sign clause of
   `arch_f32_add_correct` (result sign = sign of the exact sum), and
   `f32SignedScaled = ±f32MagScaled`; `scaledDist a b = |↑a − ↑b|`.

2. **Half-ULP grid bound (the last deep fact)** —
   `scaledDist (f32MagScaled (add a b) · 2¹⁴⁹) (fmaExact a 1.0 b).natAbs · 2²⁴
   ≤ (fmaExact a 1.0 b).natAbs`. From `IsNearestExact` (nearest among all finite
   patterns) plus the FP grid spacing: the nearest pattern is within half a ULP,
   and half a ULP is ≤ `2⁻²⁴ ·` value in the normal range. This is the
   bracketing-pattern / ULP-spacing argument — the one genuinely FP-grid piece
   left; Mathlib does not shortcut it.

Then `(1+δ)` = (1) `▸` `rel_bound_of_scaled … (2)`, and Theorem B's induction
(`ScaledDot.pairwise_sum_error_bound`) consumes it. -/

end ArchFp
