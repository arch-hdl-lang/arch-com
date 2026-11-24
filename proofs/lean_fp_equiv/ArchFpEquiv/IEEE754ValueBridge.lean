import ArchFpEquiv.F32AddRel

/-!
# Binary32 value bridge to a separately stated textbook model

`textbookBinary32` states the binary32 value using the usual fractional
significand and biased-exponent equations, rather than `FmaValue`'s integer
`2⁻¹⁴⁹` grid.  The equations correspond to IEEE 754-2019 §§3.3–3.4 and
Table 3.5 (binary32).  They are reproduced here, not imported from an
independently maintained formal library; the equalities below are checked by
Lean, while fidelity of these equations to the published standard remains a
human audit obligation.

The function is intended only for finite encodings: an all-ones exponent is
NaN or infinity, and has no rational value.
-/

namespace ArchFp

set_option exponentiation.threshold 600

/-- Textbook value of a finite binary32 encoding: subnormal fraction times
`2⁻¹²⁶`, or normal `(1 + fraction/2²³) · 2^(E−127)`, with sign applied. -/
noncomputable def textbookBinary32 (z : BitVec 32) : Rat :=
  let e := (BitVec.extractLsb 30 23 z).toNat
  let f := (BitVec.extractLsb 22 0 z).toNat
  let s : Rat := if BitVec.extractLsb 31 31 z = 1#1 then -1 else 1
  if e = 0 then s * (f : Rat) / 2 ^ 149
  else s * ((2 ^ 23 + f : Nat) : Rat) * 2 ^ e / 2 ^ 150

/-- The exact fused result in rational units, with exactly one final rounding
left to the output specification. -/
noncomputable def textbookFmaExact (a b c : BitVec 32) : Rat :=
  textbookBinary32 a * textbookBinary32 b + textbookBinary32 c

/-- The textbook encoding equation and the integer-grid decoder agree for
every finite binary32 bit pattern (and algebraically even at reserved fields,
though neither side is interpreted as a value there). -/
theorem textbookBinary32_eq_scaled (z : BitVec 32) :
    textbookBinary32 z = (f32SignedScaled z : Rat) / 2 ^ 149 := by
  have he : BitVec.extractLsb 30 23 z = expField z := rfl
  have hf : BitVec.extractLsb 22 0 z = fracField z := rfl
  simp only [textbookBinary32, he, hf, f32SignedScaled, f32MagScaled]
  by_cases h : expField z = 0#8
  · simp [h]
  · have hpos : 0 < (expField z).toNat := by
      have hz : (expField z).toNat ≠ 0 := by
        intro hz
        exact h (BitVec.eq_of_toNat_eq (by simp [hz]))
      omega
    obtain ⟨e, heq⟩ := Nat.exists_eq_succ_of_ne_zero (by omega :
      (expField z).toNat ≠ 0)
    simp only [h, ↓reduceIte, heq, pow_succ]
    push_cast
    ring

/-- The separately defined exact-real FMA expression coincides with the
`2⁻²⁹⁸` integer grid used by the Lean hardware theorem. -/
theorem textbookFmaExact_eq_scaled (a b c : BitVec 32) :
    textbookFmaExact a b c = (fmaExact a b c : Rat) / 2 ^ 298 := by
  have hp : (2 : Rat) ^ 298 = (2 ^ 149) * (2 ^ 149) := by
    rw [show (298 : Nat) = 149 + 149 by omega, pow_add]
  rw [hp]
  simp only [textbookFmaExact, textbookBinary32_eq_scaled, fmaExact]
  push_cast
  ring

/-- Absolute value of the textbook encoding is exactly the finite-pattern
magnitude on the `2⁻¹⁴⁹` grid. -/
theorem textbookBinary32_abs_eq (z : BitVec 32) :
    |textbookBinary32 z| = (f32MagScaled z : Rat) / 2 ^ 149 := by
  rw [textbookBinary32_eq_scaled, abs_div,
    abs_of_pos (by positivity : (0 : Rat) < 2 ^ 149)]
  unfold f32SignedScaled
  by_cases hs : BitVec.extractLsb 31 31 z = 1#1
  · simp [hs]
  · simp [hs]

/-- Absolute value of the independently stated exact FMA expression. -/
theorem textbookFmaExact_abs_eq (a b c : BitVec 32) :
    |textbookFmaExact a b c| = ((fmaExact a b c).natAbs : Rat) / 2 ^ 298 := by
  rw [textbookFmaExact_eq_scaled, abs_div,
    abs_of_pos (by positivity : (0 : Rat) < 2 ^ 298), abs_intCast_eq_natAbs]

/-- The rational nearest-distance and the integer-grid distance are the same
after applying their common positive scale. -/
theorem textbookMagError_eq (a b c z : BitVec 32) :
    abs (abs (textbookBinary32 z) - abs (textbookFmaExact a b c)) =
      (scaledDist (f32MagScaled z * 2 ^ 149) (fmaExact a b c).natAbs : Rat) / 2 ^ 298 := by
  rw [textbookBinary32_abs_eq, textbookFmaExact_abs_eq]
  have hcore :
      (f32MagScaled z : Rat) / 2 ^ 149 - ((fmaExact a b c).natAbs : Rat) / 2 ^ 298 =
      (((((f32MagScaled z * 2 ^ 149 : Nat) : Int) - (fmaExact a b c).natAbs) : Int) : Rat) /
        2 ^ 298 := by
    push_cast
    field_simp
    rw [abs_intCast_eq_natAbs]
    ring
  rw [hcore, abs_div, abs_of_pos (by positivity : (0 : Rat) < 2 ^ 298),
    abs_intCast_eq_natAbs, ← scaledDist_eq_natAbs]

/-- Independent rational formulation of nearest *magnitude*.  The existing
sign theorem below supplies the sign of the rounded result. -/
def TextbookNearestMag (a b c y : BitVec 32) : Prop :=
  ∀ z : BitVec 32, IsFiniteF32 z →
    abs (abs (textbookBinary32 y) - abs (textbookFmaExact a b c)) ≤
      abs (abs (textbookBinary32 z) - abs (textbookFmaExact a b c))

/-- Main ARCH FMA theorem restated against the separately specified rational
binary32 model, for finite nonzero operands and a nonzero in-range result. -/
theorem arch_fma_f32_correct_textbook (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true)
    (hnz : fmaExact a b c ≠ 0)
    (hovf : biasedFinal (arch_fma_mag a b c).toNat
      (arch_fma_elo a b c).toInt ≤ 254) :
    IsFiniteF32 (arch_fma_f32 a b c) ∧
      TextbookNearestMag a b c (arch_fma_f32 a b c) ∧
      BitVec.extractLsb 31 31 (arch_fma_f32 a b c) =
        (if fmaExact a b c < 0 then 1#1 else 0#1) := by
  obtain ⟨hfin, hnear, hsign⟩ := arch_fma_f32_correct a b c ha hb hc hnz hovf
  refine ⟨hfin, ?_, hsign⟩
  intro z hz
  rw [textbookMagError_eq, textbookMagError_eq]
  exact div_le_div_of_nonneg_right (by exact_mod_cast hnear z hz) (by positivity)

/-- Full rational nearest-value statement, including competitors of the
opposite sign.  This follows from nearest magnitude plus the proved sign bit:
for the selected result signed and magnitude errors agree, while a wrong-sign
competitor can only be farther away. -/
def TextbookNearest (a b c y : BitVec 32) : Prop :=
  ∀ z : BitVec 32, IsFiniteF32 z →
    |textbookBinary32 y - textbookFmaExact a b c| ≤
      |textbookBinary32 z - textbookFmaExact a b c|

theorem arch_fma_f32_correct_textbook_signed (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true)
    (hnz : fmaExact a b c ≠ 0)
    (hovf : biasedFinal (arch_fma_mag a b c).toNat
      (arch_fma_elo a b c).toInt ≤ 254) :
    IsFiniteF32 (arch_fma_f32 a b c) ∧
      TextbookNearest a b c (arch_fma_f32 a b c) := by
  obtain ⟨hfin, hnear, hsign⟩ :=
    arch_fma_f32_correct_textbook a b c ha hb hc hnz hovf
  refine ⟨hfin, ?_⟩
  intro z hz
  have hy :
      |textbookBinary32 (arch_fma_f32 a b c) - textbookFmaExact a b c| =
        (scaledDist (f32MagScaled (arch_fma_f32 a b c) * 2 ^ 149)
          (fmaExact a b c).natAbs : Rat) / 2 ^ 298 := by
    rw [textbookBinary32_eq_scaled, textbookFmaExact_eq_scaled]
    exact add_err_eq _ _ hsign
  rw [hy, ← textbookMagError_eq a b c (arch_fma_f32 a b c)]
  exact (hnear z hz).trans (abs_abs_sub_abs_le_abs_sub _ _)

end ArchFp
