import ArchFpEquiv.F32AddCorrect
import ArchFpEquiv.RoundReal
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

/-! ## Sign/magnitude bridge (proved) -/

/-- `scaledDist` is the integer absolute difference. -/
theorem scaledDist_eq_natAbs (m n : Nat) : scaledDist m n = ((m : Int) - n).natAbs := by
  unfold scaledDist; omega

/-- `|(n : Rat)| = (n.natAbs : Rat)`. -/
theorem abs_intCast_eq_natAbs (n : Int) : |(n : Rat)| = (n.natAbs : Rat) := by
  rw [← Int.cast_abs, Int.abs_eq_natAbs, Int.cast_natCast]

/-- **Sign/magnitude bridge.** For a pattern `y` whose sign bit matches the sign
    of the exact value `V`, the rational error equals the (unsigned) scaled
    distance over `2²⁹⁸`. Combines `f32SignedScaled = ±f32MagScaled` with the sign
    clause of `arch_f32_add_correct`; `scaledDist = |↑m − ↑n|`. -/
theorem add_err_eq (y : BitVec 32) (V : Int)
    (hsign : BitVec.extractLsb 31 31 y = (if V < 0 then 1#1 else 0#1)) :
    |(f32SignedScaled y : Rat) / 2 ^ 149 - (V : Rat) / 2 ^ 298|
      = (↑(scaledDist (f32MagScaled y * 2 ^ 149) V.natAbs) : Rat) / 2 ^ 298 := by
  have hmag : (f32SignedScaled y : Int)
      = (if BitVec.extractLsb 31 31 y = 1#1 then -1 else 1) * (f32MagScaled y : Int) := by
    unfold f32SignedScaled; rfl
  have hcast : ((f32MagScaled y * 2 ^ 149 : Nat) : Int) = (f32MagScaled y : Int) * 2 ^ 149 := by
    push_cast; ring
  have hkey : (f32SignedScaled y * 2 ^ 149 - V).natAbs
      = scaledDist (f32MagScaled y * 2 ^ 149) V.natAbs := by
    rw [scaledDist_eq_natAbs, hcast]
    by_cases hV : V < 0
    · rw [if_pos hV] at hsign; rw [hsign, if_pos rfl] at hmag
      have hVe : V = -(V.natAbs : Int) := by omega
      rw [hmag, hVe]; simp only [neg_mul, one_mul]
      generalize (f32MagScaled y : Int) * 2 ^ 149 = P
      simp only [Int.natAbs_neg, Int.natAbs_natCast]; omega
    · rw [if_neg hV] at hsign; rw [hsign] at hmag
      simp only [show ((0#1 : BitVec 1) = 1#1) = False by simp, if_false, one_mul] at hmag
      have hVe : V = (V.natAbs : Int) := by omega
      rw [hmag, hVe]; simp only [Int.natAbs_natCast]
  have hcd : (f32SignedScaled y : Rat) / 2 ^ 149 - (V : Rat) / 2 ^ 298
      = (((f32SignedScaled y * 2 ^ 149 - V : Int)) : Rat) / 2 ^ 298 := by
    push_cast; field_simp; ring
  rw [hcd, abs_div, abs_of_pos (by positivity : (0:Rat) < 2 ^ 298),
    ← Int.cast_abs, Int.abs_eq_natAbs, hkey]
  push_cast; ring

/-! ## The per-add `(1+δ)` bound — modulo the half-ULP grid bound -/

/-- **Per-add `(1+δ)` (conditional).** For finite-nonzero, non-overflowing
    operands, GIVEN the half-ULP-relative grid bound `hgrid`, the `arch_f32_add`
    relative error is ≤ `uF32 = 2⁻²⁴`. Everything but `hgrid` is now proved: the
    sign/magnitude bridge (`add_err_eq`), the value identities, and the `Rat`
    algebra (`rel_bound_of_scaled`) are all discharged, so `hgrid` is the SINGLE
    remaining obligation for the per-add bound. -/
theorem add_rel_bound (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hnz : fmaExact a 0x3F800000#32 b ≠ 0)
    (hovf : biasedFinal (arch_fma_mag a 0x3F800000#32 b).toNat
      (arch_fma_elo a 0x3F800000#32 b).toInt ≤ 254)
    (hgrid : scaledDist (f32MagScaled (arch_f32_add a b) * 2 ^ 149)
        (fmaExact a 0x3F800000#32 b).natAbs * 2 ^ 24
      ≤ (fmaExact a 0x3F800000#32 b).natAbs) :
    |f32R (arch_f32_add a b) - (f32R a + f32R b)| ≤ uF32 * |f32R a + f32R b| := by
  obtain ⟨_, _, hsg⟩ := arch_f32_add_correct a b ha hb hnz hovf
  have hval : f32R (arch_f32_add a b) - (f32R a + f32R b)
      = (f32SignedScaled (arch_f32_add a b) : Rat) / 2 ^ 149
        - (fmaExact a 0x3F800000#32 b : Rat) / 2 ^ 298 := by
    rw [f32R_add_eq_fmaExact]; unfold f32R; norm_num
  have habs : |f32R a + f32R b| = (↑(fmaExact a 0x3F800000#32 b).natAbs : Rat) / 2 ^ 298 := by
    rw [f32R_add_eq_fmaExact, abs_div, abs_of_pos (by positivity : (0:Rat) < 2 ^ 298),
      abs_intCast_eq_natAbs]
  rw [hval, add_err_eq _ _ hsg, habs]
  exact rel_bound_of_scaled _ _ hgrid

/-! ## The half-ULP grid bound (proved) — discharges `hgrid` -/

/-- **Half-ULP grid bound.** For a nearest-in-magnitude pattern `y` to a
    normal-range exact value `V` (`2¹⁷² ≤ |V|`, i.e. `|V| ≥` smallest normal, in
    `2⁻²⁹⁸` units), the scaled distance obeys the half-ULP-relative bound
    `scaledDist · 2²⁴ ≤ |V|`.

    Proof: use `roundNE_f32 false |V| (-298)` — itself a nearest pattern to `|V|`
    (`roundNE`) — as the competitor. Its magnitude is `keptNorm |V| · 2^(log₂|V|−23)`
    (`roundNE_normal_value`; the exponents align, `E'+149 = log₂|V|−23`), and
    `keptNorm |V| = rneQuot |V| (log₂|V|−23)`, so `rneQuot_halfulp` +
    `gridDist_eq_scaledDist` bound its distance by `2^(log₂|V|−24)`, whence
    `· 2²⁴ = 2^(log₂|V|) ≤ |V|`. `IsNearestExact` makes `y` at least as close. -/
theorem halfulp_grid_bound (y : BitVec 32) (V : Int)
    (hnear : IsNearestExact V y)
    (hW : 2 ^ 172 ≤ V.natAbs)
    (hovf : biasedFinal V.natAbs (-298) ≤ 254) :
    scaledDist (f32MagScaled y * 2 ^ 149) V.natAbs * 2 ^ 24 ≤ V.natAbs := by
  have hW0 : V.natAbs ≠ 0 := by positivity
  have hlog : 172 ≤ Nat.log2 V.natAbs := (Nat.le_log2 hW0).mpr hW
  set zr := roundNE_f32 false V.natAbs (-298) with hzr
  obtain ⟨hzrfin, hzrval, _⟩ :=
    roundNE_normal_value false V.natAbs (-298) hW0 (by omega) hovf
  have hsh : Nat.log2 V.natAbs - 23
      = ((Nat.log2 V.natAbs : Int) + (-298) + 126).toNat + 149 := by omega
  have hkept : keptNorm V.natAbs = rneQuot V.natAbs (Nat.log2 V.natAbs - 23) := by
    rw [keptNorm, if_neg (by omega)]
  have hdist0 : scaledDist (f32MagScaled zr * 2 ^ 149) V.natAbs
      ≤ 2 ^ (Nat.log2 V.natAbs - 23 - 1) := by
    rw [hzrval, Nat.mul_assoc, ← Nat.pow_add, ← hsh, hkept, ← gridDist_eq_scaledDist]
    exact rneQuot_halfulp V.natAbs (Nat.log2 V.natAbs - 23) (by omega)
  have hdist : scaledDist (f32MagScaled zr * 2 ^ 149) V.natAbs * 2 ^ 24 ≤ V.natAbs :=
    calc scaledDist (f32MagScaled zr * 2 ^ 149) V.natAbs * 2 ^ 24
        ≤ 2 ^ (Nat.log2 V.natAbs - 23 - 1) * 2 ^ 24 := Nat.mul_le_mul_right _ hdist0
      _ = 2 ^ (Nat.log2 V.natAbs) := by rw [← Nat.pow_add]; congr 1; omega
      _ ≤ V.natAbs := Nat.log2_self_le hW0
  calc scaledDist (f32MagScaled y * 2 ^ 149) V.natAbs * 2 ^ 24
      ≤ scaledDist (f32MagScaled zr * 2 ^ 149) V.natAbs * 2 ^ 24 :=
        Nat.mul_le_mul_right _ (hnear zr hzrfin)
    _ ≤ V.natAbs := hdist

/-- **Per-add `(1+δ)` — normal range (unconditional in `hgrid`).** For
    finite-nonzero operands whose exact sum is in the normal range
    (`2¹⁷² ≤ |exact sum|`) and does not overflow, the `arch_f32_add` relative
    error is ≤ `2⁻²⁴`. The half-ULP grid bound is discharged by
    `halfulp_grid_bound`, so this is the fully-assembled per-add relative-error
    theorem — the `(1+δ)` model Theorem B's pairwise induction consumes. The
    normal-range / no-overflow side conditions are exactly the standard `(1+δ)`
    preconditions (no underflow, no overflow). -/
theorem add_rel_bound_normal (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hnz : fmaExact a 0x3F800000#32 b ≠ 0)
    (hovf : biasedFinal (arch_fma_mag a 0x3F800000#32 b).toNat
      (arch_fma_elo a 0x3F800000#32 b).toInt ≤ 254)
    (hW : 2 ^ 172 ≤ (fmaExact a 0x3F800000#32 b).natAbs)
    (hovf298 : biasedFinal (fmaExact a 0x3F800000#32 b).natAbs (-298) ≤ 254) :
    |f32R (arch_f32_add a b) - (f32R a + f32R b)| ≤ uF32 * |f32R a + f32R b| := by
  obtain ⟨_, hnear, _⟩ := arch_f32_add_correct a b ha hb hnz hovf
  exact add_rel_bound a b ha hb hnz hovf (halfulp_grid_bound _ _ hnear hW hovf298)

end ArchFp
