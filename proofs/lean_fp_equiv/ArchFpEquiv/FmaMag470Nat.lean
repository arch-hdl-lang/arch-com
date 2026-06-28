import ArchFpEquiv.FmaMag98Nat
import ArchFpEquiv.FmaGRS
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

/-- Symmetry of the `≤`-phrased absolute difference (`split_ifs` is Mathlib-only). -/
private theorem ite_absdiff_comm (P Q : Nat) :
    (if P ≤ Q then Q - P else P - Q) = if Q ≤ P then P - Q else Q - P := by
  rcases Nat.lt_trichotomy P Q with h | h | h
  · rw [if_pos (Nat.le_of_lt h), if_neg (by omega)]
  · subst h; simp
  · rw [if_neg (by omega), if_pos (Nat.le_of_lt h)]

/-- The reference's abs-difference selector, at the `Nat` level. The hardware
    picks `X−Y` vs `Y−X` by `ult Y X`; either way the `toNat` is the symmetric
    `max−min`, which we phrase with a `≤` test to line up with the mag98 form. -/
private theorem bv470_absdiff_toNat (X Y : BitVec 470) :
    (if BitVec.ofBool (BitVec.ult Y X) == 1#1 then X - Y else Y - X).toNat
      = if Y.toNat ≤ X.toNat then X.toNat - Y.toNat else Y.toNat - X.toNat := by
  rw [ofBool_beq_one]
  rcases Nat.lt_trichotomy Y.toNat X.toNat with h | h | h
  · rw [if_pos (by rw [BitVec.ult_eq_decide]; exact decide_eq_true h),
        if_pos (Nat.le_of_lt h), BitVec.toNat_sub_of_le (Nat.le_of_lt h)]
  · rw [if_neg (by rw [BitVec.ult_eq_decide]; simp only [decide_eq_true_eq]; omega),
        if_pos (Nat.le_of_eq h), BitVec.toNat_sub_of_le (Nat.le_of_eq h.symm)]
    omega
  · rw [if_neg (by rw [BitVec.ult_eq_decide]; simp only [decide_eq_true_eq]; omega),
        if_neg (by omega), BitVec.toNat_sub_of_le (Nat.le_of_lt h)]

/-- **Reference magnitude, opposite sign.** `mag470 = |sig_hi·2^diff − sig_lo|`,
    expressed with the same `≤` test the mag98 abs-difference uses, so the two
    compose under scaling. -/
theorem fma_mag470_diff_nat (a b c : BitVec 32)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
      == BitVec.extractLsb 31 31 c) = false)
    (hd : (fmaDiff98 a b c).toNat ≤ 421) :
    (arch_fma_mag a b c).toNat
      = if (fmaSigLo98 a b c).toNat ≤ (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat
        then (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat - (fmaSigLo98 a b c).toNat
        else (fmaSigLo98 a b c).toNat - (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat := by
  have hm24 : ∀ x : BitVec 24, x.toNat < 2 ^ 48 :=
    fun x => Nat.lt_of_lt_of_le x.isLt (Nat.pow_le_pow_right (by decide) (by omega))
  have hsw48 : ∀ x : BitVec 24, (BitVec.setWidth 48 x).toNat = x.toNat := fun x => by
    rw [BitVec.toNat_setWidth, Nat.mod_eq_of_lt (hm24 x)]
  simp only [fmaSigHi98, fmaSigLo98, fmaDiff98, fmaSel98, fpEunb, fpMant24] at hd ⊢
  unfold arch_fma_mag
  simp only [hdiff, BitVec.ofBool_false]
  rw [if_neg (by decide), bv470_absdiff_toNat]
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
    rw [setWidth470_toNat _ (by omega : (16 : Nat) ≤ 470),
        setWidth470_toNat _ (by omega : (48 : Nat) ≤ 470),
        setWidth470_shift_toNat _ _ (by omega), hsw48]
    exact ite_absdiff_comm _ _
  · -- sb = true: the product is the higher operand (product shifted, c unshifted)
    simp only [reduceIte] at hd ⊢
    rw [setWidth470_toNat _ (by omega : (16 : Nat) ≤ 470),
        setWidth470_shift_toNat _ _ (by omega),
        setWidth470_toNat _ (by omega : (24 : Nat) ≤ 470), hsw48]

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

/-- Scaling an abs-difference by `k>0` distributes through the `≤`-selector. -/
private theorem absdiff_mul (P Q k : Nat) (hk : 0 < k) :
    (if Q ≤ P then P - Q else Q - P) * k
      = if Q * k ≤ P * k then P * k - Q * k else Q * k - P * k := by
  by_cases h : Q ≤ P
  · rw [if_pos h, if_pos (Nat.mul_le_mul_right k h), Nat.sub_mul]
  · rw [if_neg h, if_neg (fun hc => h (Nat.le_of_mul_le_mul_right hc hk)), Nat.sub_mul]

/-- **The `diff ≤ 48` (no-fold) identity, opposite sign.** Same scaling as the
    same-sign case: within the fold window the sticky-fold abs-difference is the
    reference abs-difference scaled by `2^(49−diff)`, so they round identically. -/
theorem mag98_eq_mag470_scaled_diff (a b c : BitVec 32)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
      == BitVec.extractLsb 31 31 c) = false)
    (hd48 : (fmaDiff98 a b c).toNat ≤ 48) :
    (arch_fma_mag98 a b c).toNat
      = (arch_fma_mag a b c).toNat * 2 ^ (49 - (fmaDiff98 a b c).toNat) := by
  rw [fma_mag98_diff_nat a b c hdiff,
      fma_mag470_diff_nat a b c hdiff (Nat.le_trans hd48 (by decide : (48 : Nat) ≤ 421))]
  unfold fmaHiNat fmaLoNat
  generalize hD : (fmaDiff98 a b c).toNat = D at hd48 ⊢
  generalize (fmaSigHi98 a b c).toNat = H
  generalize (fmaSigLo98 a b c).toNat = L
  have hsplit : (2 : Nat) ^ 48 = 2 ^ (48 - D) * 2 ^ D := by rw [← Nat.pow_add]; congr 1; omega
  have hdiv : L * 2 ^ 48 / 2 ^ D = L * 2 ^ (48 - D) := by
    rw [hsplit, ← Nat.mul_assoc, Nat.mul_div_cancel _ (Nat.pow_pos (by decide))]
  have hmod : L * 2 ^ 48 % 2 ^ D = 0 := by
    rw [hsplit, ← Nat.mul_assoc]; exact Nat.mul_mod_left _ _
  have hk : (2 : Nat) ^ (48 - D) * 2 = 2 ^ (49 - D) := by
    rw [show 49 - D = (48 - D) + 1 from by omega, Nat.pow_succ]
  have hH : H * 2 ^ 49 = H * 2 ^ D * 2 ^ (49 - D) := by
    rw [Nat.mul_assoc, ← Nat.pow_add]; congr 2; omega
  rw [hdiv, hmod, if_neg (by decide : ¬((0 : Nat) ≠ 0)), Nat.add_zero,
      Nat.mul_assoc L (2 ^ (48 - D)) 2, hk, hH,
      absdiff_mul (H * 2 ^ D) L (2 ^ (49 - D)) (Nat.pow_pos (by decide))]

/-- The folded low significand vanishes exactly when the low operand does — the
    GRS fold drops no information about *whether* the tail is nonzero. -/
private theorem foldedlow_eq_zero_iff (L D : Nat) (hpos : 0 < 2 ^ D) :
    (L * 2 ^ 48 / 2 ^ D * 2 + (if L * 2 ^ 48 % 2 ^ D ≠ 0 then 1 else 0) = 0) ↔ L = 0 := by
  have h48 : (2 : Nat) ^ 48 ≠ 0 := Nat.pos_iff_ne_zero.mp (Nat.pow_pos (by decide))
  rcases Nat.eq_zero_or_pos (L * 2 ^ 48 % 2 ^ D) with hm | hm
  · rw [if_neg (by omega), Nat.add_zero]
    constructor
    · intro h
      have hfloor : L * 2 ^ 48 / 2 ^ D = 0 := by omega
      have hlt : L * 2 ^ 48 < 2 ^ D :=
        (Nat.div_eq_zero_iff.mp hfloor).resolve_left (Nat.pos_iff_ne_zero.mp hpos)
      rw [Nat.mod_eq_of_lt hlt] at hm
      exact (Nat.mul_eq_zero.mp hm).resolve_right h48
    · intro h; rw [h, Nat.zero_mul, Nat.zero_div, Nat.zero_mul]
  · rw [if_pos (by omega)]
    constructor
    · intro h; omega
    · intro h; rw [h, Nat.zero_mul, Nat.zero_mod] at hm; omega

/-- The folded low part is below `2^49`: the guard-doubled shifted significand
    (`< 2^48`) plus the sticky bit. -/
private theorem foldedlow_lt (L D : Nat) (hL : L < 2 ^ 48) (hD : 49 ≤ D) :
    L * 2 ^ 48 / 2 ^ D * 2 + (if L * 2 ^ 48 % 2 ^ D ≠ 0 then 1 else 0) < 2 ^ 49 := by
  have hq : L * 2 ^ 48 / 2 ^ D < 2 ^ 47 := by
    rw [Nat.div_lt_iff_lt_mul (Nat.pow_pos (by decide)), ← Nat.pow_add]
    calc L * 2 ^ 48 < 2 ^ 48 * 2 ^ 48 :=
            (Nat.mul_lt_mul_right (Nat.pow_pos (by decide))).mpr hL
      _ = 2 ^ 96 := by rw [← Nat.pow_add]
      _ ≤ 2 ^ (47 + D) := Nat.pow_le_pow_right (by decide) (by omega)
  have p47 : (2 : Nat) ^ 48 = 2 ^ 47 * 2 := by rw [← Nat.pow_succ]
  have p48 : (2 : Nat) ^ 49 = 2 ^ 48 * 2 := by rw [← Nat.pow_succ]
  have hBpos : 0 < (2 : Nat) ^ 48 := Nat.pow_pos (by decide)
  by_cases hs : L * 2 ^ 48 % 2 ^ D ≠ 0
  · rw [if_pos hs]; omega
  · rw [if_neg hs]; omega

/-- The two `roundNE_sticky_collapse` hypotheses for `g = diff`, abstracted over the
    significands: `m1 = (H·2^49 + F)·2^(D−49)` and `m2 = H·2^D + L` agree above bit
    `D` (both quotient `H`) and have the same low-zero status, given `F < 2^49`,
    `L < 2^48`, `D ≥ 49`, and that `F` vanishes iff `L` does. -/
private theorem collapse_hyps_core (H F L D : Nat)
    (hF : F < 2 ^ 49) (hL : L < 2 ^ 48) (hD : 49 ≤ D) (hFL : F = 0 ↔ L = 0) :
    (H * 2 ^ 49 + F) * 2 ^ (D - 49) / 2 ^ D = (H * 2 ^ D + L) / 2 ^ D
    ∧ ((H * 2 ^ 49 + F) * 2 ^ (D - 49) % 2 ^ D = 0 ↔ (H * 2 ^ D + L) % 2 ^ D = 0) := by
  have hpos : 0 < (2 : Nat) ^ D := Nat.pow_pos (by decide)
  have e49 : (2 : Nat) ^ 49 * 2 ^ (D - 49) = 2 ^ D := by rw [← Nat.pow_add]; congr 1; omega
  have hF' : F * 2 ^ (D - 49) < 2 ^ D := by
    have := (Nat.mul_lt_mul_right (Nat.pow_pos (by decide) : (0:Nat) < 2 ^ (D - 49))).mpr hF
    rwa [e49] at this
  have hL' : L < 2 ^ D := Nat.lt_of_lt_of_le hL (Nat.pow_le_pow_right (by decide) (by omega))
  have hr1 : (H * 2 ^ 49 + F) * 2 ^ (D - 49) = 2 ^ D * H + F * 2 ^ (D - 49) := by
    rw [Nat.add_mul, Nat.mul_assoc, e49, Nat.mul_comm H (2 ^ D)]
  refine ⟨?_, ?_⟩
  · rw [hr1, Nat.mul_add_div hpos, Nat.div_eq_of_lt hF', Nat.add_zero,
        Nat.mul_comm H (2 ^ D), Nat.mul_add_div hpos, Nat.div_eq_of_lt hL', Nat.add_zero]
  · rw [hr1, Nat.mul_add_mod, Nat.mod_eq_of_lt hF', Nat.mul_comm H (2 ^ D),
        Nat.mul_add_mod, Nat.mod_eq_of_lt hL']
    constructor
    · intro h
      exact hFL.mp ((Nat.mul_eq_zero.mp h).resolve_right
        (Nat.pos_iff_ne_zero.mp (Nat.pow_pos (by decide))))
    · intro h; rw [hFL.mpr h, Nat.zero_mul]

/-- `H·M − x` for `x < M`, `H ≥ 1`: quotient by `M` is `H` (no borrow) when `x = 0`,
    else `H − 1`; remainder is `0` resp. `M − x`. The borrow bookkeeping the
    opposite-sign collapse needs. -/
private theorem sub_quot_mod (H M x : Nat) (hM : 0 < M) (hx : x < M) (hH : 1 ≤ H) :
    (H * M - x) / M = (if x = 0 then H else H - 1)
    ∧ (H * M - x) % M = (if x = 0 then 0 else M - x) := by
  rcases Nat.eq_zero_or_pos x with hx0 | hxp
  · subst hx0
    rw [Nat.sub_zero, if_pos rfl, if_pos rfl]
    exact ⟨Nat.mul_div_left H hM, Nat.mul_mod_left H M⟩
  · rw [if_neg (by omega), if_neg (by omega)]
    have hMP : M ≤ H * M := Nat.le_mul_of_pos_left M hH
    have hHM : H * M = (H - 1) * M + M := by rw [Nat.sub_one_mul]; omega
    have hrw : H * M - x = (H - 1) * M + (M - x) := by rw [hHM]; omega
    rw [hrw]
    refine ⟨?_, ?_⟩
    · rw [Nat.add_comm, Nat.add_mul_div_right _ _ hM, Nat.div_eq_of_lt (by omega : M - x < M),
          Nat.zero_add]
    · rw [Nat.add_comm, Nat.add_mul_mod_self_right, Nat.mod_eq_of_lt (by omega : M - x < M)]

/-- The opposite-sign analog of `collapse_hyps_core`: `m1 = (H·2^49 − F)·2^(D−49)`
    and `m2 = H·2^D − L` (the `hi − lo` form the abs-difference takes for
    `diff > 48`) agree above bit `D` and share low-zero status. Needs `H ≥ 1`. -/
private theorem collapse_hyps_core_sub (H F L D : Nat)
    (hF : F < 2 ^ 49) (hL : L < 2 ^ 48) (hD : 49 ≤ D) (hH : 1 ≤ H) (hFL : F = 0 ↔ L = 0) :
    (H * 2 ^ 49 - F) * 2 ^ (D - 49) / 2 ^ D = (H * 2 ^ D - L) / 2 ^ D
    ∧ ((H * 2 ^ 49 - F) * 2 ^ (D - 49) % 2 ^ D = 0 ↔ (H * 2 ^ D - L) % 2 ^ D = 0) := by
  have hpos : 0 < (2 : Nat) ^ D := Nat.pow_pos (by decide)
  have e49 : (2 : Nat) ^ 49 * 2 ^ (D - 49) = 2 ^ D := by rw [← Nat.pow_add]; congr 1; omega
  have hF' : F * 2 ^ (D - 49) < 2 ^ D := by
    have := (Nat.mul_lt_mul_right (Nat.pow_pos (by decide) : (0:Nat) < 2 ^ (D - 49))).mpr hF
    rwa [e49] at this
  have hL' : L < 2 ^ D := Nat.lt_of_lt_of_le hL (Nat.pow_le_pow_right (by decide) (by omega))
  have hr1 : (H * 2 ^ 49 - F) * 2 ^ (D - 49) = H * 2 ^ D - F * 2 ^ (D - 49) := by
    rw [Nat.sub_mul, Nat.mul_assoc, e49]
  have hr0 : F * 2 ^ (D - 49) = 0 ↔ L = 0 := by
    rw [Nat.mul_eq_zero]
    constructor
    · rintro (h | h)
      · exact hFL.mp h
      · exact absurd h (Nat.pos_iff_ne_zero.mp (Nat.pow_pos (by decide)))
    · intro h; exact Or.inl (hFL.mpr h)
  rw [hr1]
  obtain ⟨hq1, hm1⟩ := sub_quot_mod H (2 ^ D) (F * 2 ^ (D - 49)) hpos hF' hH
  obtain ⟨hq2, hm2⟩ := sub_quot_mod H (2 ^ D) L hpos hL' hH
  rw [hq1, hq2, hm1, hm2]
  simp only [hr0]
  refine ⟨trivial, ?_⟩
  rcases Nat.eq_zero_or_pos L with hL0 | hLp
  · simp [hL0]
  · rw [if_neg (by omega), if_neg (by omega)]
    exact ⟨fun h => by omega, fun h => by omega⟩

/-- **Generic additive collapse.** Two numbers sharing a high quotient `Q` at scale
    `2^G` and whose low parts (`< 2^G`) vanish together agree mod `2^G`. The
    scale-agnostic core: pick `G = diff` or `G = diff − 1` by how you decompose. -/
private theorem collapse_of_decomp (Q G r1 r2 : Nat)
    (hr1 : r1 < 2 ^ G) (hr2 : r2 < 2 ^ G) (hz : r1 = 0 ↔ r2 = 0) :
    (2 ^ G * Q + r1) / 2 ^ G = (2 ^ G * Q + r2) / 2 ^ G
    ∧ ((2 ^ G * Q + r1) % 2 ^ G = 0 ↔ (2 ^ G * Q + r2) % 2 ^ G = 0) := by
  have hpos : 0 < (2 : Nat) ^ G := Nat.pow_pos (by decide)
  refine ⟨?_, ?_⟩
  · rw [Nat.mul_add_div hpos, Nat.mul_add_div hpos, Nat.div_eq_of_lt hr1, Nat.div_eq_of_lt hr2]
  · rw [Nat.mul_add_mod, Nat.mul_add_mod, Nat.mod_eq_of_lt hr1, Nat.mod_eq_of_lt hr2]; exact hz

/-- **Generic borrow collapse.** The opposite-sign analog: `Q·2^G − r₁` and
    `Q·2^G − r₂` (with `Q ≥ 1`, low parts `< 2^G` vanishing together) agree mod
    `2^G` — both quotients drop to `Q − 1` exactly when the low part is nonzero. -/
private theorem collapse_of_decomp_sub (Q G r1 r2 : Nat)
    (hr1 : r1 < 2 ^ G) (hr2 : r2 < 2 ^ G) (hQ : 1 ≤ Q) (hz : r1 = 0 ↔ r2 = 0) :
    (Q * 2 ^ G - r1) / 2 ^ G = (Q * 2 ^ G - r2) / 2 ^ G
    ∧ ((Q * 2 ^ G - r1) % 2 ^ G = 0 ↔ (Q * 2 ^ G - r2) % 2 ^ G = 0) := by
  have hpos : 0 < (2 : Nat) ^ G := Nat.pow_pos (by decide)
  obtain ⟨hq1, hm1⟩ := sub_quot_mod Q (2 ^ G) r1 hpos hr1 hQ
  obtain ⟨hq2, hm2⟩ := sub_quot_mod Q (2 ^ G) r2 hpos hr2 hQ
  rw [hq1, hq2, hm1, hm2]
  simp only [hz]
  refine ⟨trivial, ?_⟩
  rcases Nat.eq_zero_or_pos r2 with h0 | hp
  · simp [h0]
  · rw [if_neg (by omega), if_neg (by omega)]
    exact ⟨fun h => by omega, fun h => by omega⟩

/-- `log2` pinned by a power-of-two bracket. -/
theorem log2_eq_of_range (n e : Nat) (h1 : 2 ^ e ≤ n) (h2 : n < 2 ^ (e + 1)) :
    Nat.log2 n = e := by
  have hn : n ≠ 0 := by have : 0 < (2 : Nat) ^ e := Nat.pow_pos (by decide); omega
  rw [Nat.log2_eq_iff hn]; exact ⟨h1, h2⟩

/-- `log2 (m·2^k) = log2 m + k` for `m ≥ 1` — shifting up by `k` bits adds `k`. -/
theorem log2_mul_pow (m k : Nat) (hm : 1 ≤ m) : Nat.log2 (m * 2 ^ k) = Nat.log2 m + k := by
  have h2k : 2 ^ k ≤ m * 2 ^ k := Nat.le_mul_of_pos_left _ hm
  rw [log2_div_pow (m * 2 ^ k) k h2k, Nat.mul_div_left m (Nat.pow_pos (by decide))]

/-- `log2 (H·2^49 + F) = log2 H + 49` for `H ≥ 1`, `F < 2^49`: the additive low part
    stays below the `2^49` weight so it cannot reach the next bit. Pure-Nat — keep
    `H`,`F` abstract (omega chokes evaluating `2^49` unarily; bound `hge` via `le_trans`). -/
private theorem log2_HF (H F : Nat) (hF : F < 2 ^ 49) (hH : 1 ≤ H) :
    Nat.log2 (H * 2 ^ 49 + F) = Nat.log2 H + 49 := by
  have hge : 2 ^ 49 ≤ H * 2 ^ 49 + F :=
    Nat.le_trans (Nat.le_mul_of_pos_left _ hH) (Nat.le_add_right _ _)
  have hdiv : (H * 2 ^ 49 + F) / 2 ^ 49 = H := by
    rw [Nat.mul_comm H (2 ^ 49), Nat.mul_add_div (Nat.pow_pos (by decide)),
        Nat.div_eq_of_lt hF, Nat.add_zero]
  rw [log2_div_pow _ 49 hge, hdiv]

/-- The tight bound: the folded low part is below `2^48` (guard-doubled shifted
    significand `< 2^48`, since the floor is `< 2^47`, plus the sticky bit fits in
    the freed-up parity). Needed to place the collapse at `g = diff − 1`. -/
private theorem foldedlow_lt_48 (L D : Nat) (hL : L < 2 ^ 48) (hD : 49 ≤ D) :
    L * 2 ^ 48 / 2 ^ D * 2 + (if L * 2 ^ 48 % 2 ^ D ≠ 0 then 1 else 0) < 2 ^ 48 := by
  have hq : L * 2 ^ 48 / 2 ^ D < 2 ^ 47 := by
    rw [Nat.div_lt_iff_lt_mul (Nat.pow_pos (by decide)), ← Nat.pow_add]
    calc L * 2 ^ 48 < 2 ^ 48 * 2 ^ 48 :=
            (Nat.mul_lt_mul_right (Nat.pow_pos (by decide))).mpr hL
      _ = 2 ^ 96 := by rw [← Nat.pow_add]
      _ ≤ 2 ^ (47 + D) := Nat.pow_le_pow_right (by decide) (by omega)
  have p47 : (2 : Nat) ^ 48 = 2 ^ 47 * 2 := by rw [← Nat.pow_succ]
  by_cases hs : L * 2 ^ 48 % 2 ^ D ≠ 0
  · rw [if_pos hs]; omega
  · rw [if_neg hs]; omega

/-- Shared `2^(D−49)·F < 2^(D−1)` and `2^D = 2^(D−1)·2` plumbing for the
    `g = D − 1` cores. -/
private theorem pred_aux (F D : Nat) (hF : F < 2 ^ 48) (hD : 49 ≤ D) :
    F * 2 ^ (D - 49) < 2 ^ (D - 1) ∧ (2 : Nat) ^ D = 2 ^ (D - 1) * 2 := by
  refine ⟨?_, by rw [← Nat.pow_succ]; congr 1; omega⟩
  have e48 : (2 : Nat) ^ 48 * 2 ^ (D - 49) = 2 ^ (D - 1) := by rw [← Nat.pow_add]; congr 1; omega
  have := (Nat.mul_lt_mul_right (Nat.pow_pos (by decide) : (0:Nat) < 2 ^ (D - 49))).mpr hF
  rwa [e48] at this

/-- The `g = D − 1` additive core: `(H·2^49 + F)·2^(D−49)` and `H·2^D + L`, both
    decomposed as `2^(D−1)·(H·2) + (low < 2^(D−1))`, agree mod `2^(D−1)`. This is
    the right scale for the normal case (covers `sig_hi ≥ 2^23`, where `g = diff`
    would violate `g + 24 ≤ log2`). Needs the tight `F < 2^48`. -/
private theorem collapse_hyps_core_pred (H F L D : Nat)
    (hF : F < 2 ^ 48) (hL : L < 2 ^ 48) (hD : 49 ≤ D) (hFL : F = 0 ↔ L = 0) :
    (H * 2 ^ 49 + F) * 2 ^ (D - 49) / 2 ^ (D - 1) = (H * 2 ^ D + L) / 2 ^ (D - 1)
    ∧ ((H * 2 ^ 49 + F) * 2 ^ (D - 49) % 2 ^ (D - 1) = 0 ↔ (H * 2 ^ D + L) % 2 ^ (D - 1) = 0) := by
  have e49 : (2 : Nat) ^ 49 * 2 ^ (D - 49) = 2 ^ D := by rw [← Nat.pow_add]; congr 1; omega
  obtain ⟨hr1, eD⟩ := pred_aux F D hF hD
  have step2 : H * 2 ^ D = 2 ^ (D - 1) * (H * 2) := by rw [eD, Nat.mul_left_comm]
  have hr2 : L < 2 ^ (D - 1) := Nat.lt_of_lt_of_le hL (Nat.pow_le_pow_right (by decide) (by omega))
  have hz : F * 2 ^ (D - 49) = 0 ↔ L = 0 := by
    rw [Nat.mul_eq_zero]
    constructor
    · rintro (h | h)
      · exact hFL.mp h
      · exact absurd h (Nat.pos_iff_ne_zero.mp (Nat.pow_pos (by decide)))
    · intro h; exact Or.inl (hFL.mpr h)
  have em1 : (H * 2 ^ 49 + F) * 2 ^ (D - 49) = 2 ^ (D - 1) * (H * 2) + F * 2 ^ (D - 49) := by
    rw [Nat.add_mul, Nat.mul_assoc, e49, step2]
  rw [em1, step2]
  exact collapse_of_decomp (H * 2) (D - 1) (F * 2 ^ (D - 49)) L hr1 hr2 hz

/-- The `g = D − 1` borrow core (opposite sign): `(H·2^49 − F)·2^(D−49)` and
    `H·2^D − L` decomposed as `(H·2)·2^(D−1) − (low < 2^(D−1))`. Needs `H ≥ 1`. -/
private theorem collapse_hyps_core_sub_pred (H F L D : Nat)
    (hF : F < 2 ^ 48) (hL : L < 2 ^ 48) (hD : 49 ≤ D) (hH : 1 ≤ H) (hFL : F = 0 ↔ L = 0) :
    (H * 2 ^ 49 - F) * 2 ^ (D - 49) / 2 ^ (D - 1) = (H * 2 ^ D - L) / 2 ^ (D - 1)
    ∧ ((H * 2 ^ 49 - F) * 2 ^ (D - 49) % 2 ^ (D - 1) = 0 ↔ (H * 2 ^ D - L) % 2 ^ (D - 1) = 0) := by
  have e49 : (2 : Nat) ^ 49 * 2 ^ (D - 49) = 2 ^ D := by rw [← Nat.pow_add]; congr 1; omega
  obtain ⟨hr1, eD⟩ := pred_aux F D hF hD
  have step2 : H * 2 ^ D = H * 2 * 2 ^ (D - 1) := by
    rw [eD, Nat.mul_assoc, Nat.mul_comm (2 ^ (D - 1)) 2, ← Nat.mul_assoc]
  have hr2 : L < 2 ^ (D - 1) := Nat.lt_of_lt_of_le hL (Nat.pow_le_pow_right (by decide) (by omega))
  have hQ : 1 ≤ H * 2 := by omega
  have hz : F * 2 ^ (D - 49) = 0 ↔ L = 0 := by
    rw [Nat.mul_eq_zero]
    constructor
    · rintro (h | h)
      · exact hFL.mp h
      · exact absurd h (Nat.pos_iff_ne_zero.mp (Nat.pow_pos (by decide)))
    · intro h; exact Or.inl (hFL.mpr h)
  have em1 : (H * 2 ^ 49 - F) * 2 ^ (D - 49) = H * 2 * 2 ^ (D - 1) - F * 2 ^ (D - 49) := by
    rw [Nat.sub_mul, Nat.mul_assoc, e49, step2]
  rw [em1, step2]
  exact collapse_of_decomp_sub (H * 2) (D - 1) (F * 2 ^ (D - 49)) L hr1 hr2 hQ hz

/-- **Collapse hypotheses, same sign.** For `diff > 48` the scaled sticky-fold
    magnitude `mag98·2^(diff−49)` and the reference `mag470` agree above bit `diff`
    (both quotient `sig_hi`) and share the low-zero status — the two preconditions
    of `roundNE_sticky_collapse_normal` at `g = diff`. -/
theorem mag98_scaled_collapse_same (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hdlo : 49 ≤ (fmaDiff98 a b c).toNat) (hdhi : (fmaDiff98 a b c).toNat ≤ 421) :
    (arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        / 2 ^ (fmaDiff98 a b c).toNat
      = (arch_fma_mag a b c).toNat / 2 ^ (fmaDiff98 a b c).toNat
    ∧ ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        % 2 ^ (fmaDiff98 a b c).toNat = 0
      ↔ (arch_fma_mag a b c).toNat % 2 ^ (fmaDiff98 a b c).toNat = 0) := by
  rw [fma_mag98_same_nat a b c hsame, fma_mag470_same_nat a b c hsame hdhi]
  unfold fmaHiNat fmaLoNat
  have hLlt : (fmaSigLo98 a b c).toNat < 2 ^ 48 :=
    Nat.lt_of_lt_of_le (fmaSigLo98 a b c).isLt (Nat.pow_le_pow_right (by decide) (by omega))
  exact collapse_hyps_core (fmaSigHi98 a b c).toNat _ (fmaSigLo98 a b c).toNat
    (fmaDiff98 a b c).toNat (foldedlow_lt _ _ hLlt hdlo) hLlt hdlo
    (foldedlow_eq_zero_iff _ _ (Nat.pow_pos (by decide)))

/-- The higher significand of a finite-nonzero fma is at least 1 (a nonzero
    product of nonzero significands, or a nonzero addend significand). -/
theorem fmaSigHi98_pos (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true) :
    1 ≤ (fmaSigHi98 a b c).toNat := by
  have hne : fmaSigHi98 a b c ≠ 0#48 := by
    unfold finiteNonzero isNaN isInf isZero expField fracField
      fmaSigHi98 fmaSel98 fpEunb fpMant24 at *
    bv_decide
  exact Nat.one_le_iff_ne_zero.mpr (fun h0 => hne (BitVec.eq_of_toNat_eq (by simp [h0])))

/-- **Collapse hypotheses, opposite sign.** The diff-sign analog: for `diff > 48`
    the abs-difference resolves to `hi − lo` (the higher significand dominates),
    so the scaled `mag98` and the reference `mag470` again share quotient and
    low-zero status above bit `diff`. Needs finiteness to know `sig_hi ≥ 1`. -/
theorem mag98_scaled_collapse_diff (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
      == BitVec.extractLsb 31 31 c) = false)
    (hdlo : 49 ≤ (fmaDiff98 a b c).toNat) (hdhi : (fmaDiff98 a b c).toNat ≤ 421) :
    (arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        / 2 ^ (fmaDiff98 a b c).toNat
      = (arch_fma_mag a b c).toNat / 2 ^ (fmaDiff98 a b c).toNat
    ∧ ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        % 2 ^ (fmaDiff98 a b c).toNat = 0
      ↔ (arch_fma_mag a b c).toNat % 2 ^ (fmaDiff98 a b c).toNat = 0) := by
  have hHpos := fmaSigHi98_pos a b c ha hb hc
  have hLlt : (fmaSigLo98 a b c).toNat < 2 ^ 48 :=
    Nat.lt_of_lt_of_le (fmaSigLo98 a b c).isLt (Nat.pow_le_pow_right (by decide) (by omega))
  rw [fma_mag98_diff_nat a b c hdiff, fma_mag470_diff_nat a b c hdiff hdhi]
  unfold fmaHiNat fmaLoNat
  have hFlt := foldedlow_lt (fmaSigLo98 a b c).toNat (fmaDiff98 a b c).toNat hLlt hdlo
  -- the higher operand dominates, so both abs-difference `if`s take the `hi − lo` branch
  have hle98 : (fmaSigLo98 a b c).toNat * 2 ^ 48 / 2 ^ (fmaDiff98 a b c).toNat * 2
      + (if (fmaSigLo98 a b c).toNat * 2 ^ 48 % 2 ^ (fmaDiff98 a b c).toNat ≠ 0 then 1 else 0)
      ≤ (fmaSigHi98 a b c).toNat * 2 ^ 49 := by
    have : 2 ^ 49 ≤ (fmaSigHi98 a b c).toNat * 2 ^ 49 := Nat.le_mul_of_pos_left _ hHpos
    omega
  have hle470 : (fmaSigLo98 a b c).toNat
      ≤ (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat := by
    have h1 : (fmaSigLo98 a b c).toNat < 2 ^ (fmaDiff98 a b c).toNat :=
      Nat.lt_of_lt_of_le hLlt (Nat.pow_le_pow_right (by decide) (by omega))
    have h2 : 2 ^ (fmaDiff98 a b c).toNat
        ≤ (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat := Nat.le_mul_of_pos_left _ hHpos
    omega
  rw [if_pos hle98, if_pos hle470]
  exact collapse_hyps_core_sub (fmaSigHi98 a b c).toNat _ (fmaSigLo98 a b c).toNat
    (fmaDiff98 a b c).toNat hFlt hLlt hdlo hHpos
    (foldedlow_eq_zero_iff _ _ (Nat.pow_pos (by decide)))

/-- **Collapse hypotheses at `g = diff − 1`, same sign.** The scale the normal
    case actually needs: valid whenever `sig_hi ≥ 2^23` (so `(diff−1)+24 ≤ log2`),
    which covers a normal higher operand (`mantc ∈ [2^23, 2^24)`) where `g = diff`
    would fail. Uses the tight `foldedlow < 2^48`. -/
theorem mag98_scaled_collapse_same_pred (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hdlo : 49 ≤ (fmaDiff98 a b c).toNat) (hdhi : (fmaDiff98 a b c).toNat ≤ 421) :
    (arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        / 2 ^ ((fmaDiff98 a b c).toNat - 1)
      = (arch_fma_mag a b c).toNat / 2 ^ ((fmaDiff98 a b c).toNat - 1)
    ∧ ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        % 2 ^ ((fmaDiff98 a b c).toNat - 1) = 0
      ↔ (arch_fma_mag a b c).toNat % 2 ^ ((fmaDiff98 a b c).toNat - 1) = 0) := by
  rw [fma_mag98_same_nat a b c hsame, fma_mag470_same_nat a b c hsame hdhi]
  unfold fmaHiNat fmaLoNat
  have hLlt : (fmaSigLo98 a b c).toNat < 2 ^ 48 :=
    Nat.lt_of_lt_of_le (fmaSigLo98 a b c).isLt (Nat.pow_le_pow_right (by decide) (by omega))
  exact collapse_hyps_core_pred (fmaSigHi98 a b c).toNat _ (fmaSigLo98 a b c).toNat
    (fmaDiff98 a b c).toNat (foldedlow_lt_48 _ _ hLlt hdlo) hLlt hdlo
    (foldedlow_eq_zero_iff _ _ (Nat.pow_pos (by decide)))

/-- **Collapse hypotheses at `g = diff − 1`, opposite sign.** The diff-sign analog
    of `mag98_scaled_collapse_same_pred`, via the borrow core. -/
theorem mag98_scaled_collapse_diff_pred (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
      == BitVec.extractLsb 31 31 c) = false)
    (hdlo : 49 ≤ (fmaDiff98 a b c).toNat) (hdhi : (fmaDiff98 a b c).toNat ≤ 421) :
    (arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        / 2 ^ ((fmaDiff98 a b c).toNat - 1)
      = (arch_fma_mag a b c).toNat / 2 ^ ((fmaDiff98 a b c).toNat - 1)
    ∧ ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49)
        % 2 ^ ((fmaDiff98 a b c).toNat - 1) = 0
      ↔ (arch_fma_mag a b c).toNat % 2 ^ ((fmaDiff98 a b c).toNat - 1) = 0) := by
  have hHpos := fmaSigHi98_pos a b c ha hb hc
  have hLlt : (fmaSigLo98 a b c).toNat < 2 ^ 48 :=
    Nat.lt_of_lt_of_le (fmaSigLo98 a b c).isLt (Nat.pow_le_pow_right (by decide) (by omega))
  rw [fma_mag98_diff_nat a b c hdiff, fma_mag470_diff_nat a b c hdiff hdhi]
  unfold fmaHiNat fmaLoNat
  have hFlt := foldedlow_lt_48 (fmaSigLo98 a b c).toNat (fmaDiff98 a b c).toNat hLlt hdlo
  have hle98 : (fmaSigLo98 a b c).toNat * 2 ^ 48 / 2 ^ (fmaDiff98 a b c).toNat * 2
      + (if (fmaSigLo98 a b c).toNat * 2 ^ 48 % 2 ^ (fmaDiff98 a b c).toNat ≠ 0 then 1 else 0)
      ≤ (fmaSigHi98 a b c).toNat * 2 ^ 49 := by
    have : 2 ^ 49 ≤ (fmaSigHi98 a b c).toNat * 2 ^ 49 := Nat.le_mul_of_pos_left _ hHpos
    omega
  have hle470 : (fmaSigLo98 a b c).toNat
      ≤ (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat := by
    have h1 : (fmaSigLo98 a b c).toNat < 2 ^ (fmaDiff98 a b c).toNat :=
      Nat.lt_of_lt_of_le hLlt (Nat.pow_le_pow_right (by decide) (by omega))
    have h2 : 2 ^ (fmaDiff98 a b c).toNat
        ≤ (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat := Nat.le_mul_of_pos_left _ hHpos
    omega
  rw [if_pos hle98, if_pos hle470]
  exact collapse_hyps_core_sub_pred (fmaSigHi98 a b c).toNat _ (fmaSigLo98 a b c).toNat
    (fmaDiff98 a b c).toNat hFlt hLlt hdlo hHpos
    (foldedlow_eq_zero_iff _ _ (Nat.pow_pos (by decide)))

/-- **Same-sign magnitude log2.** `log2(mag98) = log2(sig_hi) + 49` — the high
    significand sits at weight `2^49` and the additive folded low part (`< 2^49`)
    cannot reach the next power. Feeds the normal sub-case's `hsh`/`hbig`. -/
theorem log2_mag98_same (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hdlo : 49 ≤ (fmaDiff98 a b c).toNat) (hHpos : 1 ≤ (fmaSigHi98 a b c).toNat) :
    Nat.log2 (arch_fma_mag98 a b c).toNat = Nat.log2 (fmaSigHi98 a b c).toNat + 49 := by
  rw [fma_mag98_same_nat a b c hsame]
  unfold fmaHiNat fmaLoNat
  have hLlt : (fmaSigLo98 a b c).toNat < 2 ^ 48 :=
    Nat.lt_of_lt_of_le (fmaSigLo98 a b c).isLt (Nat.pow_le_pow_right (by decide) (by omega))
  exact log2_HF (fmaSigHi98 a b c).toNat _
    (foldedlow_lt (fmaSigLo98 a b c).toNat (fmaDiff98 a b c).toNat hLlt hdlo) hHpos

/-- The scaled same-sign magnitude log2: `log2(mag98·2^(diff−49)) = log2(sig_hi) + diff`.
    The `hsh`/`hbig` ingredient for the same-sign normal `diff > 48` sub-case. -/
theorem log2_mag98_scaled_same (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    (hdlo : 49 ≤ (fmaDiff98 a b c).toNat) (hHpos : 1 ≤ (fmaSigHi98 a b c).toNat) :
    Nat.log2 ((arch_fma_mag98 a b c).toNat * 2 ^ ((fmaDiff98 a b c).toNat - 49))
      = Nat.log2 (fmaSigHi98 a b c).toNat + (fmaDiff98 a b c).toNat := by
  have hmpos : 1 ≤ (arch_fma_mag98 a b c).toNat := by
    rw [fma_mag98_same_nat a b c hsame]; unfold fmaHiNat
    refine Nat.le_trans Nat.one_le_two_pow
      (Nat.le_trans (Nat.le_mul_of_pos_left _ hHpos) (Nat.le_add_right _ _))
  rw [log2_mul_pow _ _ hmpos, log2_mag98_same a b c hsame hdlo hHpos]
  omega

/-- `Int.bmod` by `2^16` is the identity on the signed range. -/
private theorem bmod16_id (x : Int) (h1 : -(2 ^ 15) ≤ x) (h2 : x < 2 ^ 15) :
    Int.bmod x (2 ^ 16) = x := by
  rw [Int.bmod_def]
  have e1 : ((2 ^ 16 : Nat) : Int) = 65536 := by decide
  have e2 : (2 : Int) ^ 15 = 32768 := by decide
  omega

/-- **The exponent identity.** `e_lo = e98 + 49 − diff` at the `Int` (signed) level
    — the reference's alignment exponent vs the sticky-fold's, differing by exactly
    `49 − diff`. Closes the `roundNE_scale` exponent obligation. -/
theorem fma_elo_toInt_rel (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true) :
    (arch_fma_elo a b c).toInt
      = (arch_fma_elo98 a b c).toInt + 49 - (fmaDiff98 a b c).toInt := by
  have hbv : arch_fma_elo a b c = arch_fma_elo98 a b c + 49#16 - fmaDiff98 a b c := by
    unfold arch_fma_elo arch_fma_elo98 fmaDiff98 fmaSel98 fpEunb; bv_decide
  obtain ⟨h98lo, h98hi⟩ := fma_elo98_bounds a b c ha hb hc
  have hdlo : 0 ≤ (fmaDiff98 a b c).toInt := by
    have h : BitVec.sle 0#16 (fmaDiff98 a b c) = true := by
      unfold finiteNonzero isNaN isInf isZero expField fracField fmaDiff98 fmaSel98 fpEunb at *
      bv_decide
    rw [BitVec.sle_iff_toInt_le] at h; simpa using h
  have hdhi : (fmaDiff98 a b c).toInt ≤ 421 := by
    have h : BitVec.sle (fmaDiff98 a b c) (BitVec.ofNat 16 421) = true := by
      unfold finiteNonzero isNaN isInf isZero expField fracField fmaDiff98 fmaSel98 fpEunb at *
      bv_decide
    rw [BitVec.sle_iff_toInt_le] at h
    rw [show (BitVec.ofNat 16 421).toInt = 421 from by decide] at h; exact h
  have p15 : (2 : Int) ^ 15 = 32768 := by decide
  rw [hbv, BitVec.toInt_sub, BitVec.toInt_add,
      show (49#16 : BitVec 16).toInt = 49 from by decide,
      bmod16_id ((arch_fma_elo98 a b c).toInt + 49) (by omega) (by omega),
      bmod16_id _ (by omega) (by omega)]

end ArchFp
