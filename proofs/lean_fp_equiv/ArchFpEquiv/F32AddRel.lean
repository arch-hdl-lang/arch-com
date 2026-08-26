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
end ArchFp
