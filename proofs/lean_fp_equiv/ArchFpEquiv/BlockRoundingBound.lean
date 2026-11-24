import ArchFpEquiv.ConcreteBlock

namespace ArchFp

set_option exponentiation.threshold 600

/-- Half the smallest FP32 subnormal, the absolute RNE underflow allowance. -/
def halfEta : Rat := 1 / 2^150

/-- Below the smallest normal, a nearest value is within half a subnormal ULP. -/
theorem nearest_subnormal_error (y : BitVec 32) (V : Int)
    (hn : IsNearestExact V y) (hv : V.natAbs < 2^172) :
    scaledDist (f32MagScaled y * 2^149) V.natAbs ≤ 2^148 := by
  have hq : rneQuot V.natAbs 149 ≤ V.natAbs / 2^149 + 1 := by
    unfold rneQuot; split <;> omega
  have hd : V.natAbs / 2^149 < 2^23 := by
    apply Nat.div_lt_of_lt_mul
    norm_num at hv ⊢
    exact hv
  have hk : rneQuot V.natAbs 149 ≤ 2^23 := by omega
  obtain ⟨hzv, hzf⟩ := f32MagScaled_encode_small 0 (rneQuot V.natAbs 149) (Or.inl rfl) hk
  have he := hn (BitVec.ofNat 32 (0 + rneQuot V.natAbs 149 % 2^31)) hzf
  rw [hzv, ← gridDist_eq_scaledDist] at he
  exact he.trans (rneQuot_halfulp V.natAbs 149 (by decide))

/-- Normal relative error plus an absolute underflow term on the fixed grid. -/
theorem nearest_mixed_grid_error (y : BitVec 32) (V : Int)
    (hn : IsNearestExact V y) (hov : biasedFinal V.natAbs (-298) ≤ 254) :
    scaledDist (f32MagScaled y * 2^149) V.natAbs * 2^24 ≤ V.natAbs + 2^172 := by
  by_cases hv : 2^172 ≤ V.natAbs
  · exact (halfulp_grid_bound y V hn hv hov).trans (Nat.le_add_right _ _)
  · have h := Nat.mul_le_mul_right (2^24) (nearest_subnormal_error y V hn (by omega))
    norm_num at h ⊢
    omega

/-- Value-level mixed bound for a nearest, correctly signed FP32 result. -/
theorem nearest_mixed_error (y : BitVec 32) (V : Int)
    (hn : IsNearestExact V y)
    (hsg : BitVec.extractLsb 31 31 y = (if V < 0 then 1#1 else 0#1))
    (hov : biasedFinal V.natAbs (-298) ≤ 254) :
    |f32R y - (V : Rat) / 2^298| ≤ f32u * |(V : Rat) / 2^298| + halfEta := by
  have h := nearest_mixed_grid_error y V hn hov
  rw [show f32R y = (f32SignedScaled y : Rat) / 2^149 from rfl, add_err_eq _ _ hsg]
  rw [abs_div, abs_of_pos (by positivity : (0 : Rat) < 2^298), abs_intCast_eq_natAbs]
  have hr : (scaledDist (f32MagScaled y * 2^149) V.natAbs : Rat) * 2^24 ≤
      (V.natAbs : Rat) + 2^172 := by exact_mod_cast h
  norm_num [f32u, halfEta] at hr ⊢
  linarith

/-- The existing implementation correctness theorem supplies the mixed bound
for finite nonzero, noncancelling addition; no normal-result assumption remains. -/
theorem add_mixed_error (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hnz : fmaExact a 0x3F800000#32 b ≠ 0)
    (hov : biasedFinal (arch_fma_mag a 0x3F800000#32 b).toNat
      (arch_fma_elo a 0x3F800000#32 b).toInt ≤ 254)
    (hgrid : biasedFinal (fmaExact a 0x3F800000#32 b).natAbs (-298) ≤ 254) :
    |f32R (arch_f32_add a b) - (f32R a + f32R b)| ≤
      f32u * |f32R a + f32R b| + halfEta := by
  obtain ⟨_, hn, hs⟩ := arch_f32_add_correct a b ha hb hnz hov
  rw [f32R_add_eq_fmaExact]
  exact nearest_mixed_error _ _ hn hs hgrid

/-- One rounded scale application, allowing underflow error. -/
theorem rounded_scale_error (s S t X u eta E : Rat)
    (hu : 0 ≤ u) (hX : 0 ≤ X)
    (hs : |s - S| ≤ E) (ht : |t - s * X| ≤ u * |s * X| + eta) :
    |t - S * X| ≤ (1 + u) * X * E + u * X * |S| + eta := by
  have hsabs : |s| ≤ |S| + E := by
    have h := abs_sub_abs_le_abs_sub s S
    linarith
  calc
    |t - S * X| = |(t - s * X) + (s - S) * X| := by ring_nf
    _ ≤ |t - s * X| + |(s - S) * X| := abs_add_le _ _
    _ = |t - s * X| + |s - S| * X := by rw [abs_mul, abs_of_nonneg hX]
    _ ≤ (u * |s * X| + eta) + E * X := by gcongr
    _ = (u * (|s| * X) + eta) + E * X := by rw [abs_mul, abs_of_nonneg hX]
    _ ≤ (u * ((|S| + E) * X) + eta) + E * X := by gcongr
    _ = (1 + u) * X * E + u * X * |S| + eta := by ring

/-- Two rounded scales. Unlike the old bound this includes both rounding errors
and amplification of first-stage underflow by the second scale. -/
theorem two_rounded_scales_error (s S t q X Y u eta E : Rat)
    (hu : 0 ≤ u) (hX : 0 ≤ X) (hY : 0 ≤ Y)
    (hs : |s - S| ≤ E)
    (ht : |t - s * X| ≤ u * |s * X| + eta)
    (hq : |q - t * Y| ≤ u * |t * Y| + eta) :
    |q - S * X * Y| ≤
      (1 + u)^2 * X * Y * E + (2*u + u^2) * X * Y * |S| +
        eta * ((1 + u) * Y + 1) := by
  have h1 := rounded_scale_error s S t X u eta E hu hX hs ht
  have h2 := rounded_scale_error t (S * X) q Y u eta
    ((1 + u) * X * E + u * X * |S| + eta) hu hY h1 hq
  rw [abs_mul, abs_of_nonneg hX] at h2
  convert h2 using 1 <;> ring


/-- Explicit finite/overflow split for the mathematical RNE rounder. Infinity
is a bit-pattern result, never interpreted as a finite rational error. -/
theorem roundNE_finite_or_overflow (neg : Bool) (sig : Nat) (e : Int)
    (hsig : sig ≠ 0) :
    (biasedFinal sig e ≤ 254 ∧ IsFiniteF32 (roundNE_f32 neg sig e) ∧
      IsNearestMag sig e (roundNE_f32 neg sig e)) ∨
    (255 ≤ biasedFinal sig e ∧ roundNE_f32 neg sig e =
      BitVec.ofNat 32 ((if neg then 2^31 else 0) + 0x7F800000)) := by
  by_cases h : biasedFinal sig e ≤ 254
  · exact Or.inl ⟨h, roundNE_nearest neg sig e hsig h⟩
  · have ho : 255 ≤ biasedFinal sig e := by omega
    have hb : 0 < (Nat.log2 sig : Int) + e + 127 := by
      unfold biasedFinal at ho
      split at ho <;> omega
    exact Or.inr ⟨ho, roundNE_overflow neg sig e hsig hb ho⟩

/-- The same overflow split applies to the actual finite-nonzero multiplier.
The significand-nonzero premise is explicit; zero-input handling is separate. -/
theorem mul_finite_or_overflow (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hsig : (BitVec.setWidth 48 (arch_decode_mant a) *
      BitVec.setWidth 48 (arch_decode_mant b)).toNat ≠ 0) :
    let sig := (BitVec.setWidth 48 (arch_decode_mant a) *
      BitVec.setWidth 48 (arch_decode_mant b)).toNat
    let e := (arch_decode_eunb a + arch_decode_eunb b).toInt
    (biasedFinal sig e ≤ 254 ∧ IsFiniteF32 (arch_f32_mul a b) ∧
      IsNearestMag sig e (arch_f32_mul a b)) ∨
    (255 ≤ biasedFinal sig e ∧ arch_f32_mul a b =
      BitVec.ofNat 32 ((if (mulSign a b) == 1#1 then 2^31 else 0) + 0x7F800000)) := by
  dsimp only
  rw [arch_f32_mul_finite_correct a b ha hb]
  exact roundNE_finite_or_overflow _ _ _ hsig

end ArchFp
