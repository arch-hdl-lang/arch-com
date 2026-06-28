import ArchFpEquiv.FmaMag470Nat
import ArchFpEquiv.FmaStickyInvariance

/-!
# Sticky-fold = reference, the `diff ≤ 48` case

Assembles the pieces for the no-fold regime: the sticky-fold fma and the
exact-wide reference round to the same f32 when the operands are within the
fold window (`diff ≤ 48`), same sign. The magnitudes differ by an exact power
of two (`mag98 = mag470·2^(49−diff)`) and the exponents by exactly `49−diff`,
so `roundNE_scale` collapses the difference.
-/

namespace ArchFp

set_option maxRecDepth 10000

theorem fma_eq_ref_same_small (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hd48 : (fmaDiff98 a b c).toNat ≤ 48) (hnc : arch_fma_mag98 a b c ≠ 0#98) :
    arch_fma_f32 a b c = arch_fma_f32_ref a b c := by
  -- the scaled identity, and that it forces both magnitudes nonzero
  have hscale := mag98_eq_mag470_scaled_same a b c hsame hd48
  have hm98 : (arch_fma_mag98 a b c).toNat ≠ 0 := fun h => hnc (BitVec.eq_of_toNat_eq (by simp [h]))
  have hm470 : (arch_fma_mag a b c).toNat ≠ 0 := by
    intro h; apply hm98; rw [hscale, h, Nat.zero_mul]
  have hnc470 : arch_fma_mag a b c ≠ 0#470 := fun h => hm470 (by rw [h]; rfl)
  -- the exponent obligation: e98 + (49−diff) = e_lo
  have hdint : (fmaDiff98 a b c).toInt = ((fmaDiff98 a b c).toNat : Int) :=
    BitVec.toInt_eq_toNat_of_lt (by
      have : (fmaDiff98 a b c).toNat ≤ 48 := hd48
      have h16 : (2 ^ 16 : Nat) = 65536 := by decide
      omega)
  have hexp : (arch_fma_elo98 a b c).toInt + ((49 - (fmaDiff98 a b c).toNat : Nat) : Int)
      = (arch_fma_elo a b c).toInt := by
    rw [fma_elo_toInt_rel a b c ha hb hc, hdint]
    have : (fmaDiff98 a b c).toNat ≤ 48 := hd48
    push_cast
    omega
  -- the sign obligation
  have hsign : (arch_fma_sign98 a b c == 1#1) = (arch_fma_sign a b c == 1#1) := by
    unfold finiteNonzero isNaN isInf isZero expField fracField
      arch_fma_sign98 arch_fma_sign at *
    bv_decide
  rw [arch_fma_f32_sticky_finite a b c ha hb hc hnc,
      arch_fma_f32_ref_finite a b c ha hb hc hnc470, hscale,
      roundNE_scale _ _ _ _ (Nat.pos_of_ne_zero hm470), hexp, hsign]

/-- **Opposite-sign, `diff ≤ 48`.** The sibling of `fma_eq_ref_same_small`: the
    abs-difference magnitudes scale by exactly `2^(49−diff)` and the exponents by
    `49−diff`, so `roundNE_scale` again collapses the difference. Everything past
    the magnitude characterization (sticky/ref reductions, exponent identity,
    sign equality) is sign-agnostic. -/
theorem fma_eq_ref_diff_small (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
      == BitVec.extractLsb 31 31 c) = false)
    (hd48 : (fmaDiff98 a b c).toNat ≤ 48) (hnc : arch_fma_mag98 a b c ≠ 0#98) :
    arch_fma_f32 a b c = arch_fma_f32_ref a b c := by
  -- the sign obligation FIRST, while the context is free of the 470-bit
  -- magnitude hypotheses bv_decide cannot reduce. The diff-sign result sign is
  -- magnitude-dependent (sign of the larger aligned operand), so we feed
  -- bv_decide both the bit-precise non-cancellation fact and the fold-window
  -- bound, under which the 98-bit and 470-bit magnitude comparisons agree.
  have hb48 : BitVec.ule (fmaDiff98 a b c) (48#16) = true := by
    rw [BitVec.ule_eq_decide, show (48#16 : BitVec 16).toNat = 48 from by decide]
    exact decide_eq_true hd48
  have hsign : (arch_fma_sign98 a b c == 1#1) = (arch_fma_sign a b c == 1#1) := by
    unfold finiteNonzero isNaN isInf isZero expField fracField
      arch_fma_sign98 arch_fma_sign arch_fma_mag98 fmaDiff98 fmaSel98 fpEunb at *
    bv_decide
  have hscale := mag98_eq_mag470_scaled_diff a b c hdiff hd48
  have hm98 : (arch_fma_mag98 a b c).toNat ≠ 0 := fun h => hnc (BitVec.eq_of_toNat_eq (by simp [h]))
  have hm470 : (arch_fma_mag a b c).toNat ≠ 0 := by
    intro h; apply hm98; rw [hscale, h, Nat.zero_mul]
  have hnc470 : arch_fma_mag a b c ≠ 0#470 := fun h => hm470 (by rw [h]; rfl)
  have hdint : (fmaDiff98 a b c).toInt = ((fmaDiff98 a b c).toNat : Int) :=
    BitVec.toInt_eq_toNat_of_lt (by
      have : (fmaDiff98 a b c).toNat ≤ 48 := hd48
      have h16 : (2 ^ 16 : Nat) = 65536 := by decide
      omega)
  have hexp : (arch_fma_elo98 a b c).toInt + ((49 - (fmaDiff98 a b c).toNat : Nat) : Int)
      = (arch_fma_elo a b c).toInt := by
    rw [fma_elo_toInt_rel a b c ha hb hc, hdint]
    have : (fmaDiff98 a b c).toNat ≤ 48 := hd48
    push_cast
    omega
  rw [arch_fma_f32_sticky_finite a b c ha hb hc hnc,
      arch_fma_f32_ref_finite a b c ha hb hc hnc470, hscale,
      roundNE_scale _ _ _ _ (Nat.pos_of_ne_zero hm470), hexp, hsign]

/-- **Same sign, `diff > 48`, normal result.** The high significand dominates
    (`sig_hi ≥ 2^23`, normal), so the result is normal. Align exponents with
    `roundNE_scale` (scaling `mag98` up by `2^(diff−49)`), then collapse to the
    reference at `g = diff − 1` via `roundNE_sticky_collapse_normal`. `hbig` (result
    normal) is taken as a hypothesis, discharged downstream from operand exponents. -/
theorem fma_eq_ref_same_big (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hdlo : 49 ≤ (fmaDiff98 a b c).toNat) (hdhi : (fmaDiff98 a b c).toNat ≤ 421)
    (hsig23 : 2 ^ 23 ≤ (fmaSigHi98 a b c).toNat)
    (hnc : arch_fma_mag98 a b c ≠ 0#98)
    (hbig : 0 < (Nat.log2 ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)) : Int)
      + (arch_fma_elo a b c).toInt + 127) :
    arch_fma_f32 a b c = arch_fma_f32_ref a b c := by
  have hHpos : 1 ≤ (fmaSigHi98 a b c).toNat := Nat.le_trans Nat.one_le_two_pow hsig23
  have hmag98ge : 2 ^ 49 ≤ (arch_fma_mag98 a b c).toNat := by
    rw [fma_mag98_same_nat a b c hsame]; unfold fmaHiNat
    exact Nat.le_trans (Nat.le_mul_of_pos_left _ hHpos) (Nat.le_add_right _ _)
  have hmag98pos : 0 < (arch_fma_mag98 a b c).toNat :=
    Nat.lt_of_lt_of_le (Nat.pow_pos (by decide)) hmag98ge
  have hm1 : 2 ^ ((fmaDiff98 a b c).toNat - 1)
      ≤ (arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49) := by
    have e1 : 2 ^ 49 * 2 ^ ((fmaDiff98 a b c).toNat - 49) = 2 ^ (fmaDiff98 a b c).toNat := by
      rw [← Nat.pow_add]; congr 1; omega
    have h2 : 2 ^ (fmaDiff98 a b c).toNat
        ≤ (arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49) := by
      rw [← e1]; exact Nat.mul_le_mul_right _ hmag98ge
    exact Nat.le_trans (Nat.pow_le_pow_right (by decide) (by omega)) h2
  have hlog23 : 23 ≤ Nat.log2 (fmaSigHi98 a b c).toNat :=
    (Nat.le_log2 (Nat.pos_iff_ne_zero.mp hHpos)).mpr hsig23
  have hsh : (fmaDiff98 a b c).toNat - 1 + 24
      ≤ Nat.log2 ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)) := by
    rw [log2_mag98_scaled_same a b c hsame hdlo hHpos]; omega
  have hm470pos : 1 ≤ (arch_fma_mag a b c).toNat := by
    rw [fma_mag470_same_nat a b c hsame (by omega)]
    exact Nat.le_trans Nat.one_le_two_pow
      (Nat.le_trans (Nat.le_mul_of_pos_left _ hHpos) (Nat.le_add_right _ _))
  have hnc470 : arch_fma_mag a b c ≠ 0#470 := by
    intro h; rw [h] at hm470pos; simp at hm470pos
  have hdint : (fmaDiff98 a b c).toInt = ((fmaDiff98 a b c).toNat : Int) :=
    BitVec.toInt_eq_toNat_of_lt (by
      have h15 : (2 ^ 15 : Nat) = 32768 := by decide
      omega)
  have hexp2 : (arch_fma_elo98 a b c).toInt
      = (arch_fma_elo a b c).toInt + (((fmaDiff98 a b c).toNat - 49 : Nat) : Int) := by
    rw [fma_elo_toInt_rel a b c ha hb hc, hdint]; omega
  have hsign : (arch_fma_sign98 a b c == 1#1) = (arch_fma_sign a b c == 1#1) := by
    unfold finiteNonzero isNaN isInf isZero expField fracField at ha hb hc
    unfold arch_fma_sign98 arch_fma_sign
    bv_decide
  obtain ⟨hchi, hcst⟩ := mag98_scaled_collapse_same_pred a b c hsame hdlo hdhi
  rw [arch_fma_f32_sticky_finite a b c ha hb hc hnc,
      arch_fma_f32_ref_finite a b c ha hb hc hnc470,
      hexp2,
      ← roundNE_scale (arch_fma_sign98 a b c == 1#1) (arch_fma_mag98 a b c).toNat
        (arch_fma_elo a b c).toInt ((fmaDiff98 a b c).toNat - 49) hmag98pos,
      roundNE_sticky_collapse_normal (arch_fma_sign98 a b c == 1#1)
        ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49))
        (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt ((fmaDiff98 a b c).toNat - 1)
        hm1 hchi hcst hbig hsh,
      hsign]

end ArchFp
