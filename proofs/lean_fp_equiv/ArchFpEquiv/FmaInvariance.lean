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

end ArchFp
