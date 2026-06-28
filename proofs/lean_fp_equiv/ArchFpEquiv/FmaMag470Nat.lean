import ArchFpEquiv.FmaMag98Nat
import Std.Tactic.BVDecide

/-!
# `arch_fma_mag` (reference, width 470) — Nat characterization

The exact-wide reference magnitude shifts the higher operand up by `diff` (no
fold, no sticky — 470 bits hold everything exactly for finite operands). So

  same sign:  mag470 = sig_hi · 2^diff + sig_lo
  diff sign:  mag470 = | sig_hi · 2^diff − sig_lo |

reusing the same `fmaSigHi98 / fmaSigLo98 / fmaDiff98` accessors as the
sticky-fold path. The `diff ≤ 421` bound (true for finite operands) keeps the
shifted product inside 470 bits.
-/

namespace ArchFp

set_option maxRecDepth 100000

/-- A ≤470-bit-safe shift: `setWidth 470 x << d = x · 2^d` when `n + d ≤ 470`. -/
theorem setWidth470_shift_toNat {n : Nat} (x : BitVec n) (d : Nat) (hn : n + d ≤ 470) :
    (BitVec.setWidth 470 x <<< d).toNat = x.toNat * 2 ^ d := by
  have hx : x.toNat < 2 ^ n := x.isLt
  have hb : x.toNat * 2 ^ d < 2 ^ 470 := by
    have h1 : x.toNat * 2 ^ d < 2 ^ n * 2 ^ d :=
      (Nat.mul_lt_mul_right (Nat.pow_pos (by decide : 0 < 2))).mpr hx
    have h2 : (2 : Nat) ^ n * 2 ^ d = 2 ^ (n + d) := by rw [← Nat.pow_add]
    exact Nat.lt_of_lt_of_le (h2 ▸ h1) (Nat.pow_le_pow_right (by decide) hn)
  rw [BitVec.toNat_shiftLeft, BitVec.toNat_setWidth, Nat.shiftLeft_eq,
      Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le hx (Nat.pow_le_pow_right (by decide) (by omega))),
      Nat.mod_eq_of_lt hb]

/-- `setWidth 470 x = x` at the Nat level (no shift). -/
theorem setWidth470_toNat {n : Nat} (x : BitVec n) (hn : n ≤ 470) :
    (BitVec.setWidth 470 x).toNat = x.toNat := by
  rw [BitVec.toNat_setWidth,
      Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le x.isLt (Nat.pow_le_pow_right (by decide) hn))]

/-- For finite operands the exponent gap is at most 421, so the reference's
    shifted operand stays inside 470 bits (the alignment is exact, no fold). -/
theorem fma_diff98_bound (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true) :
    (fmaDiff98 a b c).toNat ≤ 421 := by
  have h : BitVec.ule (fmaDiff98 a b c) (BitVec.ofNat 16 421) = true := by
    unfold finiteNonzero isNaN isInf isZero expField fracField fmaDiff98 fmaSel98 fpEunb at *
    bv_decide
  rw [BitVec.ule] at h
  simpa using of_decide_eq_true h

/-- `x·2^d < 2^469` for `x < 2^48`, `d ≤ 421` (the shifted-operand bound). -/
private theorem mul_pow_lt469 (x d : Nat) (hx : x < 2 ^ 48) (hd : d ≤ 421) : x * 2 ^ d < 2 ^ 469 := by
  have h1 : x * 2 ^ d < 2 ^ 48 * 2 ^ d := (Nat.mul_lt_mul_right (Nat.pow_pos (by decide))).mpr hx
  have h2 : (2 : Nat) ^ 48 * 2 ^ d = 2 ^ (48 + d) := by rw [← Nat.pow_add]
  exact Nat.lt_of_lt_of_le (h2 ▸ h1) (Nat.pow_le_pow_right (by decide) (by omega))

/-- **Reference magnitude, same sign.** `mag470 = sig_hi·2^diff + sig_lo` (exact). -/
theorem fma_mag470_same_nat (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hd : (fmaDiff98 a b c).toNat ≤ 421) :
    (arch_fma_mag a b c).toNat
      = (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat + (fmaSigLo98 a b c).toNat := by
  have h470 : (2 : Nat) ^ 470 = 2 ^ 469 + 2 ^ 469 := by
    rw [show (470 : Nat) = 469 + 1 from rfl, Nat.pow_succ, Nat.mul_two]
  have h48 : (2 : Nat) ^ 48 ≤ 2 ^ 469 := Nat.pow_le_pow_right (by decide) (by omega)
  have hm24 : ∀ x : BitVec 24, x.toNat < 2 ^ 48 :=
    fun x => Nat.lt_of_lt_of_le x.isLt (Nat.pow_le_pow_right (by decide) (by omega))
  have hsw48 : ∀ x : BitVec 24, (BitVec.setWidth 48 x).toNat = x.toNat := fun x => by
    rw [BitVec.toNat_setWidth, Nat.mod_eq_of_lt (hm24 x)]
  have key : ∀ A B : Nat, A < 2 ^ 469 → B < 2 ^ 469 → A + B < 2 ^ 470 := by
    intro A B hA hB; omega
  simp only [fmaSigHi98, fmaSigLo98, fmaDiff98, fmaSel98, fpEunb, fpMant24] at hd ⊢
  unfold arch_fma_mag
  simp only [hsame, beq_self_eq_true, BitVec.ofBool_true, ite_self]
  rw [if_pos (by decide : ((1 : BitVec 1) == 1#1) = true)]
  generalize hsb : BitVec.sle
      (if (BitVec.ofBool (BitVec.extractLsb 30 23 c == 0#8) == 1#1) = true then 65387#16
       else BitVec.setWidth 16 (BitVec.extractLsb 30 23 c) - 150#16)
      ((if (BitVec.ofBool (BitVec.extractLsb 30 23 a == 0#8) == 1#1) = true then 65387#16
        else BitVec.setWidth 16 (BitVec.extractLsb 30 23 a) - 150#16) +
       if (BitVec.ofBool (BitVec.extractLsb 30 23 b == 0#8) == 1#1) = true then 65387#16
       else BitVec.setWidth 16 (BitVec.extractLsb 30 23 b) - 150#16) = sb at hd ⊢
  simp only [ofBool_beq_one] at hd ⊢
  cases sb
  · -- sb = false: c is the higher operand (its field is shifted, product unshifted)
    simp only [reduceIte, Bool.false_eq_true, if_false] at hd ⊢
    rw [setWidth470_toNat _ (by omega : (16 : Nat) ≤ 470), BitVec.toNat_add,
        setWidth470_toNat _ (by omega : (48 : Nat) ≤ 470), setWidth470_shift_toNat _ _ (by omega),
        hsw48, Nat.mod_eq_of_lt
          (key _ _ (Nat.lt_of_lt_of_le (BitVec.isLt _) h48) (mul_pow_lt469 _ _ (hm24 _) hd))]
    omega
  · -- sb = true: the product is the higher operand (product shifted, c unshifted)
    simp only [reduceIte] at hd ⊢
    rw [setWidth470_toNat _ (by omega : (16 : Nat) ≤ 470), BitVec.toNat_add,
        setWidth470_shift_toNat _ _ (by omega), setWidth470_toNat _ (by omega : (24 : Nat) ≤ 470),
        hsw48, Nat.mod_eq_of_lt
          (key _ _ (mul_pow_lt469 _ _ (BitVec.isLt _) hd) (Nat.lt_of_lt_of_le (hm24 _) h48))]

/-- **The `diff ≤ 48` (no-fold) identity.** When the operands are within the fold
    window, the sticky-fold magnitude is exactly the reference scaled by
    `2^(49−diff)` — so they round identically (the `roundNE_scale` case). -/
theorem mag98_eq_mag470_scaled_same (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hd48 : (fmaDiff98 a b c).toNat ≤ 48) :
    (arch_fma_mag98 a b c).toNat
      = (arch_fma_mag a b c).toNat * 2 ^ (49 - (fmaDiff98 a b c).toNat) := by
  rw [fma_mag98_same_nat a b c hsame, fma_mag470_same_nat a b c hsame (by omega)]
  unfold fmaHiNat fmaLoNat
  generalize hD : (fmaDiff98 a b c).toNat = D at hd48 ⊢
  generalize (fmaSigHi98 a b c).toNat = H
  generalize (fmaSigLo98 a b c).toNat = L
  have hsplit : (2 : Nat) ^ 48 = 2 ^ (48 - D) * 2 ^ D := by rw [← Nat.pow_add]; congr 1; omega
  have hdiv : L * 2 ^ 48 / 2 ^ D = L * 2 ^ (48 - D) := by
    rw [hsplit, ← Nat.mul_assoc, Nat.mul_div_cancel _ (Nat.pow_pos (by decide))]
  have hmod : L * 2 ^ 48 % 2 ^ D = 0 := by
    rw [hsplit, ← Nat.mul_assoc]; exact Nat.mul_mod_left _ _
  have p49 : (2 : Nat) ^ D * 2 ^ (49 - D) = 2 ^ 49 := by rw [← Nat.pow_add]; congr 1; omega
  have p49' : (2 : Nat) ^ (49 - D) = 2 ^ (48 - D) * 2 := by
    rw [show 49 - D = (48 - D) + 1 from by omega, Nat.pow_succ]
  rw [hdiv, hmod, if_neg (by decide : ¬((0 : Nat) ≠ 0)), Nat.add_zero,
      Nat.add_mul, Nat.mul_assoc H (2 ^ D) (2 ^ (49 - D)), p49, p49',
      ← Nat.mul_assoc L (2 ^ (48 - D)) 2]

end ArchFp
