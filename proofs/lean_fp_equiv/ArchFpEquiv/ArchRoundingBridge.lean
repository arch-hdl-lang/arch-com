import ArchFpEquiv.BlockRoundingBound
import ArchFpEquiv.FmaRneComposition
namespace ArchFp
set_option exponentiation.threshold 600
set_option maxHeartbeats 4000000

 theorem round_fixed_finite_bound (V : Int) (hV : V ≠ 0)
    (hf : IsFiniteF32 (roundNE_f32 (decide (V < 0)) V.natAbs (-298))) :
    |f32R (roundNE_f32 (decide (V < 0)) V.natAbs (-298)) - (V : Rat)/2^298| ≤
      f32u * |(V : Rat)/2^298| + halfEta := by
  have hn : V.natAbs ≠ 0 := by omega
  have hr : biasedFinal V.natAbs (-298) ≤ 254 := by
    rcases roundNE_finite_or_overflow (decide (V < 0)) V.natAbs (-298) hn with h | h
    · exact h.1
    · rw [h.2] at hf
      by_cases hs : V < 0 <;> norm_num [hs, IsFiniteF32, expField] at hf <;> exact (hf (by decide)).elim
  have near := (roundNE_nearest (decide (V < 0)) V.natAbs (-298) hn hr).2
  have ne := isNearestExact_of_isNearestMag V.natAbs (-298) V _ (by omega)
    (by norm_num) near
  apply nearest_mixed_error _ _ ne _ hr
  simpa using roundNE_extract_sign (decide (V < 0)) V.natAbs (-298)

 theorem add_nonzero_fixed (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hnz : fmaExact a 0x3F800000#32 b ≠ 0) :
    arch_f32_add a b = roundNE_f32 (decide (fmaExact a 0x3F800000#32 b < 0))
      (fmaExact a 0x3F800000#32 b).natAbs (-298) := by
  have hm : arch_fma_mag a 0x3F800000#32 b ≠ 0#470 := by
    intro h; exact hnz ((fma_mag_zero_iff a _ b ha finiteNonzero_one hb).mp h)
  have hn : 0 < (arch_fma_mag a 0x3F800000#32 b).toNat := by
    have : (arch_fma_mag a 0x3F800000#32 b).toNat ≠ 0 := by
      intro h; exact hm (BitVec.eq_of_toNat_eq (by simpa using h))
    omega
  have he := (fma_elo_bounds a _ b ha finiteNonzero_one hb).1
  have hscale := roundNE_scale (decide (fmaExact a 0x3F800000#32 b < 0))
    (arch_fma_mag a 0x3F800000#32 b).toNat (-298)
    ((arch_fma_elo a 0x3F800000#32 b).toInt + 298).toNat hn
  rw [fma_mag_exact a _ b ha finiteNonzero_one hb] at hscale
  have hex : (-298 : Int) + ↑((arch_fma_elo a 0x3F800000#32 b).toInt + 298).toNat =
    (arch_fma_elo a 0x3F800000#32 b).toInt := by omega
  rw [hex] at hscale
  rw [add_eq_fma_one a b ha hb, arch_fma_f32_eq_ref,
    arch_fma_f32_ref_finite a _ b ha finiteNonzero_one hb hm, hscale]
  rw [fma_sign_exact a _ b ha finiteNonzero_one hb hnz]
  by_cases h : fmaExact a 0x3F800000#32 b < 0 <;> simp [h]

 theorem zero_rat (v : BitVec 32) (hz : isZero v = true) : f32R v = 0 := by
  have he : expField v = 0#8 := by unfold isZero expField at *; bv_decide
  have hf : fracField v = 0#23 := by unfold isZero fracField at *; bv_decide
  simp [f32R, f32SignedScaled, f32MagScaled, he, hf]

 theorem finite_nonzero_of_not_zero (a : BitVec 32) (ha : IsFiniteF32 a)
    (hz : isZero a ≠ true) : finiteNonzero a = true := by
  unfold IsFiniteF32 at ha
  simp [finiteNonzero, isNaN, isInf, ha, hz]

 theorem add_both_zero (a b : BitVec 32) (ha : isZero a = true) (hb : isZero b = true) :
    isZero (arch_f32_add a b) = true := by
  unfold isZero arch_f32_add at *
  bv_decide

/-- All finite inputs, including zeros, cancellation and subnormal results.
Only finiteness of the actual output is required, not exact scaling or a
per-input numerical-error check. -/
 theorem arch_add_mixed (a b : BitVec 32) (ha : IsFiniteF32 a) (hb : IsFiniteF32 b)
    (hr : IsFiniteF32 (arch_f32_add a b)) :
    |f32R (arch_f32_add a b) - (f32R a + f32R b)| ≤
      f32u * |f32R a + f32R b| + halfEta := by
  by_cases za : isZero a = true
  · by_cases zb : isZero b = true
    · rw [zero_rat _ (add_both_zero a b za zb), zero_rat a za, zero_rat b zb]
      norm_num [f32u, halfEta]
    · rw [add_zero_left a b za (finite_nonzero_of_not_zero b hb zb), zero_rat a za]
      simp only [zero_add, sub_self, abs_zero]
      unfold f32u halfEta; positivity
  · have hna := finite_nonzero_of_not_zero a ha za
    by_cases zb : isZero b = true
    · rw [add_zero_right a b hna zb, zero_rat b zb]
      simp only [add_zero, sub_self, abs_zero]; unfold f32u halfEta; positivity
    · have hnb := finite_nonzero_of_not_zero b hb zb
      by_cases hz : fmaExact a 0x3F800000#32 b = 0
      · have hc : arch_f32_add a b = 0#32 := by
          rw [add_eq_fma_one a b hna hnb]
          exact arch_fma_exact_cancellation a _ b hna finiteNonzero_one hnb hz
        rw [hc, f32R_add_eq_fmaExact, hz]
        norm_num [f32R, f32SignedScaled, f32MagScaled, expField, fracField, halfEta]
      · rw [add_nonzero_fixed a b hna hnb hz] at hr ⊢
        rw [f32R_add_eq_fmaExact]
        exact round_fixed_finite_bound _ hz hr

private theorem bridge_mulSig_bits (a b : BitVec 32) :
    ((BitVec.setWidth 48 (arch_decode_mant a) *
      BitVec.setWidth 48 (arch_decode_mant b)).toNat) = mulSigNat a b := by
  have ha := (fpMant24 a).isLt
  have hb := (fpMant24 b).isLt
  have hma : arch_decode_mant a = fpMant24 a := rfl
  have hmb : arch_decode_mant b = fpMant24 b := rfl
  rw [hma, hmb, BitVec.toNat_mul, BitVec.toNat_setWidth, BitVec.toNat_setWidth]
  norm_num at ha hb ⊢
  rw [Nat.mod_eq_of_lt (by nlinarith)]
  rfl

private theorem bridge_mulExp_bits (a b : BitVec 32) :
    (arch_decode_eunb a + arch_decode_eunb b).toInt = mulExpInt a b :=
  fpEunb_add_toInt a b

private theorem bridge_mant_nonzero (a : BitVec 32) (ha : finiteNonzero a = true) :
    fpMant24 a ≠ 0#24 := by
  unfold finiteNonzero isNaN isInf isZero expField fracField fpMant24 at *
  bv_decide

private theorem bridge_product_magnitude (a b : BitVec 32) :
    f32MagScaled a * f32MagScaled b =
      mulSigNat a b * 2 ^ (mulExpInt a b + 298).toNat := by
  obtain ⟨ha, _⟩ := fpEunb_range a
  obtain ⟨hb, _⟩ := fpEunb_range b
  rw [f32MagScaled_eq_decode, f32MagScaled_eq_decode]
  have he : (mulExpInt a b + 298).toNat =
      ((fpEunb a).toInt + 149).toNat + ((fpEunb b).toInt + 149).toNat := by
    unfold mulExpInt; omega
  rw [he, pow_add]
  unfold mulSigNat
  ring


def mulExact (a b : BitVec 32) : Int := f32SignedScaled a * f32SignedScaled b

 theorem mulExact_abs (a b : BitVec 32) :
    (mulExact a b).natAbs = f32MagScaled a * f32MagScaled b := by
  unfold mulExact f32SignedScaled
  split <;> split <;> simp [Int.natAbs_mul]

 theorem mul_nonzero_fixed (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) :
    mulExact a b ≠ 0 ∧ arch_f32_mul a b =
      roundNE_f32 (decide (mulExact a b < 0)) (mulExact a b).natAbs (-298) := by
  have hsig : mulSigNat a b ≠ 0 := by
    unfold mulSigNat
    apply Nat.mul_ne_zero
    · intro h; exact bridge_mant_nonzero a ha (BitVec.eq_of_toNat_eq (by simpa using h))
    · intro h; exact bridge_mant_nonzero b hb (BitVec.eq_of_toNat_eq (by simpa using h))
  have hval : mulSigNat a b * 2^(mulExpInt a b + 298).toNat = (mulExact a b).natAbs := by
    rw [mulExact_abs, bridge_product_magnitude]
  have hnz : mulExact a b ≠ 0 := by
    have hp := Nat.mul_ne_zero hsig (Nat.two_pow_pos (mulExpInt a b + 298).toNat).ne'
    omega
  have hp : (0 : Int) < (f32MagScaled a : Int) * (f32MagScaled b : Int) := by
    have h : 0 < f32MagScaled a * f32MagScaled b := by rw [bridge_product_magnitude]; positivity
    exact_mod_cast h
  have hs : ((mulSign a b) == 1#1) = decide (mulExact a b < 0) := by
    have ea : sgn a = 0#1 ∨ sgn a = 1#1 := by bv_decide
    have eb : sgn b = 0#1 ∨ sgn b = 1#1 := by bv_decide
    rcases ea with ea | ea <;> rcases eb with eb | eb <;>
      simp only [sgn] at ea eb <;>
      simp [mulSign, sgn, mulExact, f32SignedScaled, BitVec.extractLsb, ea, eb] <;> nlinarith
  have he : -298 ≤ mulExpInt a b := by
    obtain ⟨ha, _⟩ := fpEunb_range a
    obtain ⟨hb, _⟩ := fpEunb_range b
    unfold mulExpInt; omega
  have hscale := roundNE_scale (decide (mulExact a b < 0)) (mulSigNat a b)
    (-298) (mulExpInt a b + 298).toNat (by omega)
  rw [hval, show (-298 : Int) + ↑(mulExpInt a b + 298).toNat = mulExpInt a b by omega] at hscale
  refine ⟨hnz, ?_⟩
  rw [arch_f32_mul_finite_correct a b ha hb, bridge_mulSig_bits, bridge_mulExp_bits, hs, hscale]

 theorem finite_not_nan_inf (a : BitVec 32) (ha : IsFiniteF32 a) :
    isNaN a = false ∧ isInf a = false := by
  unfold IsFiniteF32 at ha
  simp [isNaN, isInf, ha]

 theorem zeroOf_zero (a b : BitVec 32) : isZero (zeroOf a b) = true := by
  unfold isZero zeroOf; bv_decide

 theorem mulExact_value (a b : BitVec 32) :
    (mulExact a b : Rat)/2^298 = f32R a * f32R b := by
  unfold mulExact f32R
  push_cast
  ring

/-- Total finite-input mixed multiplication bound, including signed zero and
rounding into subnormals or zero. Overflow is excluded solely by result bits. -/
 theorem arch_mul_mixed (a b : BitVec 32) (ha : IsFiniteF32 a) (hb : IsFiniteF32 b)
    (hr : IsFiniteF32 (arch_f32_mul a b)) :
    |f32R (arch_f32_mul a b) - f32R a * f32R b| ≤
      f32u * |f32R a * f32R b| + halfEta := by
  by_cases za : isZero a = true
  · rw [mul_zero_left a b za (finite_not_nan_inf b hb).1 (finite_not_nan_inf b hb).2,
      zero_rat _ (zeroOf_zero a b), zero_rat a za]
    norm_num [halfEta]
  · by_cases zb : isZero b = true
    · rw [mul_zero_right a b zb (finite_not_nan_inf a ha).1 (finite_not_nan_inf a ha).2,
        zero_rat _ (zeroOf_zero a b), zero_rat b zb]
      norm_num [halfEta]
    · obtain ⟨hn, he⟩ := mul_nonzero_fixed a b
        (finite_nonzero_of_not_zero a ha za) (finite_nonzero_of_not_zero b hb zb)
      rw [he] at hr ⊢
      rw [← mulExact_value]
      exact round_fixed_finite_bound _ hn hr

end ArchFp
