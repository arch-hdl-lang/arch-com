import ArchFpEquiv.RoundReal
import ArchFpEquiv.FmaMag470Nat
import ArchFpEquiv.FmaStickyInvariance
import ArchFpEquiv.FmaEquiv

/-!
# R3 — end-to-end value semantics for the fma

The chain so far: the bounded sticky-fold datapath is bit-identical to the
exact-wide reference (`arch_fma_f32_eq_ref`), the reference reduces to
`roundNE_f32 sign mag470 elo` (`arch_fma_f32_ref_finite`), and `roundNE_f32`
is proved round-to-nearest-even of its input value (`roundNE_nearest`, R2).
The link still taken "by construction" is that `mag470 · 2^elo` IS the exact
`|a·b + c|` of the decoded operands. This file proves it.

**Spec additions (trusted by reading, everything else proved).**
`f32SignedScaled x` = the signed magnitude of a bit pattern in units of
`2^{-149}`, reusing R2's `f32MagScaled`. `fmaExact a b c` = the exact value
`a·b + c` in units of `2^{-298}`:

    fmaExact a b c = sA·sB + sC·2^149     (sX = f32SignedScaled X)

Deliverables: the decode lemma (`f32MagScaled_eq_decode` — the datapath's
`fpMant24`/`fpEunb` denote the same magnitude), the magnitude identity
(`fma_mag_exact` — `mag470·2^{elo+298} = |fmaExact|`), the sign identity,
and the composed end-to-end theorem: for finite inputs with a nonzero
in-range result, `arch_fma_f32 a b c` is a finite pattern nearest to the
exact value `a·b+c` among ALL finite f32 patterns, with the correct sign.
-/

namespace ArchFp

/-- Signed magnitude of a bit pattern, in units of `2^{-149}`. -/
def f32SignedScaled (x : BitVec 32) : Int :=
  (if BitVec.extractLsb 31 31 x = 1#1 then -1 else 1) * (f32MagScaled x : Int)

/-- The exact value `a·b + c`, in units of `2^{-298}`. -/
def fmaExact (a b c : BitVec 32) : Int :=
  f32SignedScaled a * f32SignedScaled b + f32SignedScaled c * 2 ^ 149

-- ── the decode lemma ─────────────────────────────────────────────────────────

/-- `extractLsb 30 23` (the datapath's exponent field) is `expField`. -/
private theorem extract_expField (x : BitVec 32) :
    BitVec.extractLsb 30 23 x = expField x := rfl

/-- `extractLsb 22 0` (the datapath's fraction field) is `fracField`. -/
private theorem extract_fracField (x : BitVec 32) :
    BitVec.extractLsb 22 0 x = fracField x := rfl

/-- `ofBool b = 1#1` iff `b`. -/
private theorem ofBool_eq_one (b : Bool) : (BitVec.ofBool b = 1#1) ↔ b = true := by
  cases b <;> decide

/-- `fpEunb`'s `toInt`, as plain `Int` arithmetic on the exponent field. -/
private theorem fpEunb_toInt (x : BitVec 32) :
    (fpEunb x).toInt
      = if expField x = 0#8 then (-149 : Int)
        else ((expField x).toNat : Int) - 150 := by
  rw [fpEunb, extract_expField]
  by_cases he : expField x = 0#8
  · simp only [he, if_pos]
    rw [if_pos (by simp)]
    decide
  · rw [if_neg (by simp [ofBool_eq_one, he]), if_neg he]
    have hE : (expField x).toNat < 256 := (expField x).isLt
    have hsw : (BitVec.setWidth 16 (expField x)).toNat = (expField x).toNat := by
      rw [BitVec.toNat_setWidth, Nat.mod_eq_of_lt (by omega)]
    rw [BitVec.toInt_eq_toNat_bmod, BitVec.toNat_sub, hsw]
    rw [Int.bmod_eq_emod]
    have h150 : (150#16 : BitVec 16).toNat = 150 := rfl
    rw [h150]
    split <;> omega

/-- `fpEunb` sits in `[-149, 105]` for every pattern. -/
theorem fpEunb_range (x : BitVec 32) :
    (-149 : Int) ≤ (fpEunb x).toInt ∧ (fpEunb x).toInt ≤ 105 := by
  rw [fpEunb_toInt]
  have hE : (expField x).toNat < 256 := (expField x).isLt
  have hne : expField x ≠ 0#8 → 1 ≤ (expField x).toNat := by
    intro h
    by_cases hz : (expField x).toNat = 0
    · exact absurd (BitVec.eq_of_toNat_eq (by rw [hz]; rfl)) h
    · omega
  split
  · omega
  · rename_i h
    have := hne h
    omega

/-- **Decode.** The datapath's 24-bit significand and LSB-exponent denote the
    same magnitude as R2's `f32MagScaled`: for every pattern,
    `f32MagScaled x = fpMant24 x · 2^{eunb+149}` (in `2^{-149}` units). -/
theorem f32MagScaled_eq_decode (x : BitVec 32) :
    f32MagScaled x = (fpMant24 x).toNat * 2 ^ ((fpEunb x).toInt + 149).toNat := by
  have hfr : (fracField x).toNat < 2 ^ 23 := (fracField x).isLt
  by_cases he : expField x = 0#8
  · -- subnormal: mant24 = 0 ++ frac, eunb = -149, scale 2^0
    have hm : (fpMant24 x).toNat = (fracField x).toNat := by
      rw [fpMant24, extract_expField, extract_fracField, if_pos (by simp [he])]
      rw [BitVec.toNat_append]
      simp [Nat.shiftLeft_eq]
    have hexp0 : ((fpEunb x).toInt + 149).toNat = 0 := by
      rw [fpEunb_toInt, if_pos he]; rfl
    rw [f32MagScaled, if_pos he, hm, hexp0]
    omega
  · -- normal: mant24 = 1 ++ frac = 2^23 + frac, eunb = E - 150
    have hE1 : 1 ≤ (expField x).toNat := by
      by_cases hz : (expField x).toNat = 0
      · exact absurd (BitVec.eq_of_toNat_eq (by rw [hz]; rfl)) he
      · omega
    have hm : (fpMant24 x).toNat = 2 ^ 23 + (fracField x).toNat := by
      rw [fpMant24, extract_expField, extract_fracField, if_neg (by simp [ofBool_eq_one, he])]
      rw [BitVec.toNat_append]
      have h1 : (1#1 : BitVec 1).toNat = 1 := rfl
      rw [h1, Nat.shiftLeft_eq, Nat.mul_comm 1 (2 ^ 23),
        ← Nat.two_pow_add_eq_or_of_lt hfr]
    have hexpv : ((fpEunb x).toInt + 149).toNat = (expField x).toNat - 1 := by
      rw [fpEunb_toInt, if_neg he]
      omega
    rw [f32MagScaled, if_neg he, hm, hexpv]

-- ── exponent bookkeeping (Int level) ─────────────────────────────────────────

/-- `Int.bmod` by `2^16` is the identity on the signed range. -/
private theorem bmod16_id' (x : Int) (h1 : -(2 ^ 15) ≤ x) (h2 : x < 2 ^ 15) :
    Int.bmod x (2 ^ 16) = x := by
  rw [Int.bmod_def]
  have e1 : ((2 ^ 16 : Nat) : Int) = 65536 := by decide
  have e2 : (2 : Int) ^ 15 = 32768 := by decide
  omega

/-- The product exponent `eunb a + eunb b` adds without 16-bit wrap. -/
theorem fpEunb_add_toInt (a b : BitVec 32) :
    (fpEunb a + fpEunb b).toInt = (fpEunb a).toInt + (fpEunb b).toInt := by
  obtain ⟨hla, hha⟩ := fpEunb_range a
  obtain ⟨hlb, hhb⟩ := fpEunb_range b
  rw [BitVec.toInt_add, bmod16_id' _ (by omega) (by omega)]

/-- The selector fires iff the product exponent is at least `c`'s. -/
theorem fmaSel98_iff (a b c : BitVec 32) :
    (fmaSel98 a b c = 1#1)
      ↔ (fpEunb c).toInt ≤ (fpEunb a).toInt + (fpEunb b).toInt := by
  rw [fmaSel98, ofBool_eq_one, BitVec.sle_iff_toInt_le, fpEunb_add_toInt]

/-- `arch_fma_elo` in accessor form: the smaller of the two LSB-exponents. -/
private theorem arch_fma_elo_acc (a b c : BitVec 32) :
    arch_fma_elo a b c
      = if fmaSel98 a b c = 1#1 then fpEunb c else fpEunb a + fpEunb b := by
  unfold arch_fma_elo fmaSel98 fpEunb
  bv_decide

/-- **The reference's alignment exponent is the min** of the product's and
    the addend's LSB-exponents, at the `Int` level. -/
theorem fma_elo_toInt_min (a b c : BitVec 32) :
    (arch_fma_elo a b c).toInt
      = if (fpEunb c).toInt ≤ (fpEunb a).toInt + (fpEunb b).toInt
        then (fpEunb c).toInt
        else (fpEunb a).toInt + (fpEunb b).toInt := by
  rw [arch_fma_elo_acc]
  by_cases hs : fmaSel98 a b c = 1#1
  · rw [if_pos hs, if_pos ((fmaSel98_iff a b c).mp hs)]
  · rw [if_neg hs, if_neg (fun h => hs ((fmaSel98_iff a b c).mpr h)),
      fpEunb_add_toInt]

/-- **The exponent gap** is the absolute difference of the two LSB-exponents,
    at the `Int` level (`toNat` cast — the gap is the shift amount). -/
theorem fmaDiff98_toInt (a b c : BitVec 32) :
    ((fmaDiff98 a b c).toNat : Int)
      = if (fpEunb c).toInt ≤ (fpEunb a).toInt + (fpEunb b).toInt
        then (fpEunb a).toInt + (fpEunb b).toInt - (fpEunb c).toInt
        else (fpEunb c).toInt - ((fpEunb a).toInt + (fpEunb b).toInt) := by
  obtain ⟨hla, hha⟩ := fpEunb_range a
  obtain ⟨hlb, hhb⟩ := fpEunb_range b
  obtain ⟨hlc, hhc⟩ := fpEunb_range c
  have hd : fmaDiff98 a b c
      = (if fmaSel98 a b c = 1#1 then fpEunb a + fpEunb b else fpEunb c)
        - (if fmaSel98 a b c = 1#1 then fpEunb c else fpEunb a + fpEunb b) := by
    unfold fmaDiff98
    by_cases hs : fmaSel98 a b c = 1#1 <;> simp [hs]
  by_cases hs : fmaSel98 a b c = 1#1
  · have hle := (fmaSel98_iff a b c).mp hs
    rw [if_pos hle]
    have hIv : (fmaDiff98 a b c).toInt
        = (fpEunb a).toInt + (fpEunb b).toInt - (fpEunb c).toInt := by
      rw [hd, if_pos hs, if_pos hs, BitVec.toInt_sub, fpEunb_add_toInt,
        bmod16_id' _ (by omega) (by omega)]
    have h1 := BitVec.toInt_eq_toNat_bmod (fmaDiff98 a b c)
    rw [Int.bmod_eq_emod] at h1
    have h2 := (fmaDiff98 a b c).isLt
    split at h1 <;> omega
  · have hgt : ¬ ((fpEunb c).toInt ≤ (fpEunb a).toInt + (fpEunb b).toInt) :=
      fun h => hs ((fmaSel98_iff a b c).mpr h)
    rw [if_neg hgt]
    have hIv : (fmaDiff98 a b c).toInt
        = (fpEunb c).toInt - ((fpEunb a).toInt + (fpEunb b).toInt) := by
      rw [hd, if_neg hs, if_neg hs, BitVec.toInt_sub, fpEunb_add_toInt,
        bmod16_id' _ (by omega) (by omega)]
    have h1 := BitVec.toInt_eq_toNat_bmod (fmaDiff98 a b c)
    rw [Int.bmod_eq_emod] at h1
    have h2 := (fmaDiff98 a b c).isLt
    split at h1 <;> omega

-- ── the magnitude identity ───────────────────────────────────────────────────

private theorem bv1_cases (s : BitVec 1) : s = 0#1 ∨ s = 1#1 := by
  by_cases h : s = 0#1
  · exact Or.inl h
  · right
    apply BitVec.eq_of_toNat_eq
    have h0 : s.toNat ≠ 0 := fun hz => h (BitVec.eq_of_toNat_eq (by rw [hz]; rfl))
    have h2 : s.toNat < 2 := s.isLt
    show s.toNat = 1
    omega

/-- `fmaExact` split into a signed product term and a signed addend term. -/
theorem fmaExact_sign_split (a b c : BitVec 32) :
    fmaExact a b c
      = (if BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b = 1#1
         then -(((f32MagScaled a * f32MagScaled b : Nat)) : Int)
         else ((f32MagScaled a * f32MagScaled b : Nat) : Int))
        + (if BitVec.extractLsb 31 31 c = 1#1
           then -(((f32MagScaled c * 2 ^ 149 : Nat)) : Int)
           else ((f32MagScaled c * 2 ^ 149 : Nat) : Int)) := by
  rw [fmaExact, f32SignedScaled, f32SignedScaled, f32SignedScaled]
  rcases bv1_cases (BitVec.extractLsb 31 31 a) with hsa | hsa <;>
    rcases bv1_cases (BitVec.extractLsb 31 31 b) with hsb | hsb <;>
      rcases bv1_cases (BitVec.extractLsb 31 31 c) with hsc | hsc <;>
        simp [hsa, hsb, hsc, Int.neg_mul, Int.mul_neg, Int.one_mul,
          Int.neg_neg, Int.natCast_mul] <;> omega

/-- Same effective signs: `|a·b + c| = P + C` (magnitudes add). -/
theorem fmaExact_natAbs_same (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c
      = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b) :
    (fmaExact a b c).natAbs
      = f32MagScaled a * f32MagScaled b + f32MagScaled c * 2 ^ 149 := by
  rw [fmaExact_sign_split, hsame]
  by_cases h : BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b = 1#1
  · rw [if_pos h, if_pos h]; omega
  · rw [if_neg h, if_neg h]; omega

/-- Opposite effective signs: `|a·b + c| = |P − C|` (magnitudes cancel). -/
theorem fmaExact_natAbs_diff (a b c : BitVec 32)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
      == BitVec.extractLsb 31 31 c) = false) :
    (fmaExact a b c).natAbs
      = if f32MagScaled c * 2 ^ 149 ≤ f32MagScaled a * f32MagScaled b
        then f32MagScaled a * f32MagScaled b - f32MagScaled c * 2 ^ 149
        else f32MagScaled c * 2 ^ 149 - f32MagScaled a * f32MagScaled b := by
  have hne : ¬ (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
      = BitVec.extractLsb 31 31 c) := by
    intro h
    rw [h] at hdiff
    simp at hdiff
  rw [fmaExact_sign_split]
  rcases bv1_cases (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    with hab | hab <;> rcases bv1_cases (BitVec.extractLsb 31 31 c) with hsc | hsc
  · exact absurd (hab.trans hsc.symm) hne
  · rw [if_neg (by rw [hab]; decide), if_pos hsc]
    split <;> omega
  · rw [if_pos hab, if_neg (by rw [hsc]; decide)]
    split <;> omega
  · exact absurd (hab.trans hsc.symm) hne

/-- The 48-bit significand product does not wrap. -/
private theorem sigProd_toNat (a b : BitVec 32) :
    (BitVec.setWidth 48 (fpMant24 a) * BitVec.setWidth 48 (fpMant24 b)).toNat
      = (fpMant24 a).toNat * (fpMant24 b).toNat := by
  have hsw : ∀ x : BitVec 32, (BitVec.setWidth 48 (fpMant24 x)).toNat
      = (fpMant24 x).toNat := fun x => by
    rw [BitVec.toNat_setWidth, Nat.mod_eq_of_lt
      (Nat.lt_of_lt_of_le (fpMant24 x).isLt
        (Nat.pow_le_pow_right (by decide) (by omega)))]
  have ha := (fpMant24 a).isLt
  have hb := (fpMant24 b).isLt
  have hprod : (fpMant24 a).toNat * (fpMant24 b).toNat < 2 ^ 48 := by
    have h1 : (fpMant24 a).toNat * (fpMant24 b).toNat
        ≤ (2 ^ 24 - 1) * (2 ^ 24 - 1) :=
      Nat.mul_le_mul (by omega) (by omega)
    omega
  rw [BitVec.toNat_mul, hsw, hsw, Nat.mod_eq_of_lt hprod]

/-- Products of decoded magnitudes recombine as `m₁·m₂·2^{x₁+x₂}`. -/
private theorem mul_pow_shuffle (m1 m2 x1 x2 : Nat) :
    m1 * 2 ^ x1 * (m2 * 2 ^ x2) = m1 * m2 * 2 ^ (x1 + x2) := by
  rw [Nat.pow_add]
  ac_rfl

/-- Scaling an `≤`-phrased absolute difference. -/
private theorem absdiff_scale (H L P C k : Nat) (hH : H * k = P) (hL : L * k = C) :
    (if L ≤ H then H - L else L - H) * k = if C ≤ P then P - C else C - P := by
  by_cases h : L ≤ H
  · rw [if_pos h, Nat.sub_mul, hH, hL]
    have hm := Nat.mul_le_mul_right k h
    rw [hH, hL] at hm
    rw [if_pos hm]
  · rw [if_neg h, Nat.sub_mul, hH, hL]
    have hm := Nat.mul_le_mul_right k (Nat.le_of_lt (Nat.lt_of_not_le h))
    rw [hH, hL] at hm
    split <;> omega

/-- **The magnitude identity (R3's core).** For finite operands, the exact-wide
    reference's aligned magnitude at its alignment exponent IS the exact value:
    `mag470 · 2^{elo+298} = |a·b + c|` in `2^{-298}` units. -/
theorem fma_mag_exact (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) :
    (arch_fma_mag a b c).toNat * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat
      = (fmaExact a b c).natAbs := by
  obtain ⟨hla, hha⟩ := fpEunb_range a
  obtain ⟨hlb, hhb⟩ := fpEunb_range b
  obtain ⟨hlc, hhc⟩ := fpEunb_range c
  have hd421 := fma_diff98_bound a b c ha hb hc
  have hswc : ∀ x : BitVec 32, (BitVec.setWidth 48 (fpMant24 x)).toNat
      = (fpMant24 x).toNat := fun x => by
    rw [BitVec.toNat_setWidth, Nat.mod_eq_of_lt
      (Nat.lt_of_lt_of_le (fpMant24 x).isLt
        (Nat.pow_le_pow_right (by decide) (by omega)))]
  by_cases hs : fmaSel98 a b c = 1#1
  · -- product is the higher operand: elo = ec, diff = eab − ec
    have hle := (fmaSel98_iff a b c).mp hs
    have hdI : ((fmaDiff98 a b c).toNat : Int)
        = (fpEunb a).toInt + (fpEunb b).toInt - (fpEunb c).toInt := by
      rw [fmaDiff98_toInt, if_pos hle]
    have heI : (arch_fma_elo a b c).toInt = (fpEunb c).toInt := by
      rw [fma_elo_toInt_min, if_pos hle]
    have hHi : (fmaSigHi98 a b c).toNat
        = (fpMant24 a).toNat * (fpMant24 b).toNat := by
      rw [fmaSigHi98, if_pos (by simp [hs])]
      exact sigProd_toNat a b
    have hLo : (fmaSigLo98 a b c).toNat = (fpMant24 c).toNat := by
      rw [fmaSigLo98, if_pos (by simp [hs])]
      exact hswc c
    have hP : (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat
          * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat
        = f32MagScaled a * f32MagScaled b := by
      rw [hHi, Nat.mul_assoc, ← Nat.pow_add,
          f32MagScaled_eq_decode a, f32MagScaled_eq_decode b, mul_pow_shuffle]
      rw [show (fmaDiff98 a b c).toNat
            + ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb a).toInt + 149).toNat + ((fpEunb b).toInt + 149).toNat
        from by omega]
    have hC : (fmaSigLo98 a b c).toNat
          * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat
        = f32MagScaled c * 2 ^ 149 := by
      rw [hLo, f32MagScaled_eq_decode c, Nat.mul_assoc, ← Nat.pow_add]
      rw [show ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb c).toInt + 149).toNat + 149 from by omega]
    by_cases hsame : BitVec.extractLsb 31 31 c
        = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
    · rw [fma_mag470_same_nat a b c hsame hd421,
          fmaExact_natAbs_same a b c hsame, Nat.add_mul]
      omega
    · have hdiffb : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
          == BitVec.extractLsb 31 31 c) = false := by
        rw [beq_eq_false_iff_ne]
        exact fun h => hsame h.symm
      rw [fma_mag470_diff_nat a b c hdiffb hd421,
          fmaExact_natAbs_diff a b c hdiffb]
      exact absdiff_scale _ _ _ _ _ hP hC
  · -- addend is the higher operand: elo = eab, diff = ec − eab
    have hgt : ¬ ((fpEunb c).toInt ≤ (fpEunb a).toInt + (fpEunb b).toInt) :=
      fun h => hs ((fmaSel98_iff a b c).mpr h)
    have hdI : ((fmaDiff98 a b c).toNat : Int)
        = (fpEunb c).toInt - ((fpEunb a).toInt + (fpEunb b).toInt) := by
      rw [fmaDiff98_toInt, if_neg hgt]
    have heI : (arch_fma_elo a b c).toInt
        = (fpEunb a).toInt + (fpEunb b).toInt := by
      rw [fma_elo_toInt_min, if_neg hgt]
    have hHi : (fmaSigHi98 a b c).toNat = (fpMant24 c).toNat := by
      rw [fmaSigHi98, if_neg (by simp [hs])]
      exact hswc c
    have hLo : (fmaSigLo98 a b c).toNat
        = (fpMant24 a).toNat * (fpMant24 b).toNat := by
      rw [fmaSigLo98, if_neg (by simp [hs])]
      exact sigProd_toNat a b
    have hP : (fmaSigHi98 a b c).toNat * 2 ^ (fmaDiff98 a b c).toNat
          * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat
        = f32MagScaled c * 2 ^ 149 := by
      rw [hHi, Nat.mul_assoc, ← Nat.pow_add,
          f32MagScaled_eq_decode c, Nat.mul_assoc, ← Nat.pow_add]
      rw [show (fmaDiff98 a b c).toNat
            + ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb c).toInt + 149).toNat + 149 from by omega]
    have hC : (fmaSigLo98 a b c).toNat
          * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat
        = f32MagScaled a * f32MagScaled b := by
      rw [hLo, f32MagScaled_eq_decode a, f32MagScaled_eq_decode b,
          mul_pow_shuffle]
      rw [show ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb a).toInt + 149).toNat + ((fpEunb b).toInt + 149).toNat
        from by omega]
    by_cases hsame : BitVec.extractLsb 31 31 c
        = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
    · rw [fma_mag470_same_nat a b c hsame hd421,
          fmaExact_natAbs_same a b c hsame, Nat.add_mul]
      omega
    · have hdiffb : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
          == BitVec.extractLsb 31 31 c) = false := by
        rw [beq_eq_false_iff_ne]
        exact fun h => hsame h.symm
      rw [fma_mag470_diff_nat a b c hdiffb hd421,
          fmaExact_natAbs_diff a b c hdiffb,
          absdiff_scale _ _ _ _ _ hP hC]
      split <;> split <;> omega

-- ── the sign identity ────────────────────────────────────────────────────────

/-- The aligned product term, as the datapath's 470-bit value. -/
private def pAlign470 (a b c : BitVec 32) : BitVec 470 :=
  if BitVec.sle (fpEunb c) (fpEunb a + fpEunb b)
  then BitVec.setWidth 470
      (BitVec.setWidth 48 (fpMant24 a) * BitVec.setWidth 48 (fpMant24 b))
      <<< (BitVec.setWidth 470 (fmaDiff98 a b c)).toNat
  else BitVec.setWidth 470
      (BitVec.setWidth 48 (fpMant24 a) * BitVec.setWidth 48 (fpMant24 b))

/-- The aligned addend term, as the datapath's 470-bit value. -/
private def cAlign470 (a b c : BitVec 32) : BitVec 470 :=
  if BitVec.sle (fpEunb c) (fpEunb a + fpEunb b)
  then BitVec.setWidth 470 (fpMant24 c)
  else BitVec.setWidth 470 (fpMant24 c)
      <<< (BitVec.setWidth 470 (fmaDiff98 a b c)).toNat

/-- `arch_fma_sign` in accessor form: the common sign when effective signs
    agree, else the sign of the strictly larger aligned operand (ties to the
    addend's sign — irrelevant since ties mean an exact-zero result). -/
private theorem arch_fma_sign_acc (a b c : BitVec 32) :
    arch_fma_sign a b c
      = if (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
            == BitVec.extractLsb 31 31 c
        then BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
        else if BitVec.ult (cAlign470 a b c) (pAlign470 a b c)
          then BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b
          else BitVec.extractLsb 31 31 c := by
  unfold arch_fma_sign pAlign470 cAlign470 fmaDiff98 fmaSel98 fpEunb fpMant24
  bv_decide (config := { timeout := 540 })

/-- The aligned terms at the `Nat` level (finite operands: no 470-bit wrap). -/
private theorem align470_toNat (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) :
    (pAlign470 a b c).toNat
        = (if fmaSel98 a b c = 1#1
           then (fpMant24 a).toNat * (fpMant24 b).toNat
              * 2 ^ (fmaDiff98 a b c).toNat
           else (fpMant24 a).toNat * (fpMant24 b).toNat)
      ∧ (cAlign470 a b c).toNat
        = (if fmaSel98 a b c = 1#1
           then (fpMant24 c).toNat
           else (fpMant24 c).toNat * 2 ^ (fmaDiff98 a b c).toNat) := by
  have hd421 := fma_diff98_bound a b c ha hb hc
  have hdsw : (BitVec.setWidth 470 (fmaDiff98 a b c)).toNat
      = (fmaDiff98 a b c).toNat := setWidth470_toNat _ (by omega)
  have hsel : (fmaSel98 a b c = 1#1)
      ↔ (BitVec.sle (fpEunb c) (fpEunb a + fpEunb b) = true) := by
    rw [fmaSel98, ofBool_eq_one]
  constructor
  · rw [pAlign470]
    by_cases hs : fmaSel98 a b c = 1#1
    · rw [if_pos (hsel.mp hs), if_pos hs, hdsw,
        setWidth470_shift_toNat _ _ (by omega), sigProd_toNat]
    · rw [if_neg (fun h => hs (hsel.mpr h)), if_neg hs,
        setWidth470_toNat _ (by omega), sigProd_toNat]
  · rw [cAlign470]
    by_cases hs : fmaSel98 a b c = 1#1
    · rw [if_pos (hsel.mp hs), if_pos hs, setWidth470_toNat _ (by omega)]
    · rw [if_neg (fun h => hs (hsel.mpr h)), if_neg hs, hdsw,
        setWidth470_shift_toNat _ _ (by omega)]

/-- The aligned terms scale to the exact product/addend magnitudes. -/
private theorem align_scale (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) :
    (pAlign470 a b c).toNat * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat
        = f32MagScaled a * f32MagScaled b
      ∧ (cAlign470 a b c).toNat * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat
        = f32MagScaled c * 2 ^ 149 := by
  obtain ⟨hla, hha⟩ := fpEunb_range a
  obtain ⟨hlb, hhb⟩ := fpEunb_range b
  obtain ⟨hlc, hhc⟩ := fpEunb_range c
  obtain ⟨hpN, hcN⟩ := align470_toNat a b c ha hb hc
  by_cases hs : fmaSel98 a b c = 1#1
  · have hle := (fmaSel98_iff a b c).mp hs
    have hdI : ((fmaDiff98 a b c).toNat : Int)
        = (fpEunb a).toInt + (fpEunb b).toInt - (fpEunb c).toInt := by
      rw [fmaDiff98_toInt, if_pos hle]
    have heI : (arch_fma_elo a b c).toInt = (fpEunb c).toInt := by
      rw [fma_elo_toInt_min, if_pos hle]
    constructor
    · rw [hpN, if_pos hs, Nat.mul_assoc, ← Nat.pow_add,
          f32MagScaled_eq_decode a, f32MagScaled_eq_decode b, mul_pow_shuffle]
      rw [show (fmaDiff98 a b c).toNat
            + ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb a).toInt + 149).toNat + ((fpEunb b).toInt + 149).toNat
        from by omega]
    · rw [hcN, if_pos hs, f32MagScaled_eq_decode c, Nat.mul_assoc, ← Nat.pow_add]
      rw [show ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb c).toInt + 149).toNat + 149 from by omega]
  · have hgt : ¬ ((fpEunb c).toInt ≤ (fpEunb a).toInt + (fpEunb b).toInt) :=
      fun h => hs ((fmaSel98_iff a b c).mpr h)
    have hdI : ((fmaDiff98 a b c).toNat : Int)
        = (fpEunb c).toInt - ((fpEunb a).toInt + (fpEunb b).toInt) := by
      rw [fmaDiff98_toInt, if_neg hgt]
    have heI : (arch_fma_elo a b c).toInt
        = (fpEunb a).toInt + (fpEunb b).toInt := by
      rw [fma_elo_toInt_min, if_neg hgt]
    constructor
    · rw [hpN, if_neg hs, f32MagScaled_eq_decode a, f32MagScaled_eq_decode b,
          mul_pow_shuffle]
      rw [show ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb a).toInt + 149).toNat + ((fpEunb b).toInt + 149).toNat
        from by omega]
    · rw [hcN, if_neg hs, Nat.mul_assoc, ← Nat.pow_add,
          f32MagScaled_eq_decode c, Nat.mul_assoc, ← Nat.pow_add]
      rw [show (fmaDiff98 a b c).toNat
            + ((arch_fma_elo a b c).toInt + 298).toNat
          = ((fpEunb c).toInt + 149).toNat + 149 from by omega]

/-- **The sign identity.** For a nonzero exact result, the datapath's sign bit
    is the sign of the exact value `a·b + c`. -/
theorem fma_sign_exact (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) (hnz : fmaExact a b c ≠ 0) :
    arch_fma_sign a b c = (if fmaExact a b c < 0 then 1#1 else 0#1) := by
  obtain ⟨hPs, hCs⟩ := align_scale a b c ha hb hc
  have hK : 0 < 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat :=
    Nat.pow_pos (by decide)
  have hlt_iff : (BitVec.ult (cAlign470 a b c) (pAlign470 a b c) = true)
      ↔ f32MagScaled c * 2 ^ 149 < f32MagScaled a * f32MagScaled b := by
    rw [BitVec.ult_eq_decide, decide_eq_true_eq, ← hPs, ← hCs]
    exact (Nat.mul_lt_mul_right hK).symm
  rw [arch_fma_sign_acc]
  rcases bv1_cases (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b)
    with hab | hab <;>
    rcases bv1_cases (BitVec.extractLsb 31 31 c) with hsc | hsc
  · -- both positive: P + C ≥ 0, sign 0
    have hval : fmaExact a b c
        = ((f32MagScaled a * f32MagScaled b : Nat) : Int)
          + ((f32MagScaled c * 2 ^ 149 : Nat) : Int) := by
      rw [fmaExact_sign_split, if_neg (by rw [hab]; decide),
          if_neg (by rw [hsc]; decide)]
    rw [hab, hsc, if_pos (by decide), hval, if_neg (by omega)]
  · -- product ≥ 0, addend < 0: magnitude comparison decides
    have hval : fmaExact a b c
        = ((f32MagScaled a * f32MagScaled b : Nat) : Int)
          + -((f32MagScaled c * 2 ^ 149 : Nat) : Int) := by
      rw [fmaExact_sign_split, if_neg (by rw [hab]; decide), if_pos hsc]
    rw [hval] at hnz
    rw [hab, hsc, if_neg (by decide), hval]
    by_cases hu : BitVec.ult (cAlign470 a b c) (pAlign470 a b c) = true
    · have hlt := hlt_iff.mp hu
      rw [if_pos hu, if_neg (by omega)]
    · have hge : ¬ (f32MagScaled c * 2 ^ 149
          < f32MagScaled a * f32MagScaled b) := fun h => hu (hlt_iff.mpr h)
      rw [if_neg hu, if_pos (by omega)]
  · -- product < 0, addend ≥ 0: mirror
    have hval : fmaExact a b c
        = -((f32MagScaled a * f32MagScaled b : Nat) : Int)
          + ((f32MagScaled c * 2 ^ 149 : Nat) : Int) := by
      rw [fmaExact_sign_split, if_pos hab, if_neg (by rw [hsc]; decide)]
    rw [hval] at hnz
    rw [hab, hsc, if_neg (by decide), hval]
    by_cases hu : BitVec.ult (cAlign470 a b c) (pAlign470 a b c) = true
    · have hlt := hlt_iff.mp hu
      rw [if_pos hu, if_pos (by omega)]
    · have hge : ¬ (f32MagScaled c * 2 ^ 149
          < f32MagScaled a * f32MagScaled b) := fun h => hu (hlt_iff.mpr h)
      rw [if_neg hu, if_neg (by omega)]
  · -- both negative: sign 1
    have hval : fmaExact a b c
        = -((f32MagScaled a * f32MagScaled b : Nat) : Int)
          + -((f32MagScaled c * 2 ^ 149 : Nat) : Int) := by
      rw [fmaExact_sign_split, if_pos hab, if_pos hsc]
    rw [hval] at hnz
    rw [hab, hsc, if_pos (by decide), hval, if_pos (by omega)]

/-- The datapath magnitude is zero exactly when the exact value is zero. -/
theorem fma_mag_zero_iff (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true) :
    (arch_fma_mag a b c = 0#470) ↔ fmaExact a b c = 0 := by
  have hme := fma_mag_exact a b c ha hb hc
  have hK : 0 < 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat :=
    Nat.pow_pos (by decide)
  constructor
  · intro h
    have h0 : (arch_fma_mag a b c).toNat = 0 := by rw [h]; rfl
    rw [h0] at hme
    omega
  · intro h
    rw [h] at hme
    apply BitVec.eq_of_toNat_eq
    show (arch_fma_mag a b c).toNat = 0
    rcases Nat.mul_eq_zero.mp
        (by omega : (arch_fma_mag a b c).toNat
          * 2 ^ ((arch_fma_elo a b c).toInt + 298).toNat = 0) with h0 | h0
    · exact h0
    · omega

-- ── the sign bit of the rounder output ───────────────────────────────────────

private theorem extract31_toNat (n : Nat) (hn : n < 2 ^ 32) :
    (BitVec.extractLsb 31 31 (BitVec.ofNat 32 n)).toNat = n / 2 ^ 31 % 2 ^ 1 := by
  simp [BitVec.extractLsb, BitVec.extractLsb'_toNat, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt hn, Nat.shiftRight_eq_div_pow]

private theorem extract31_sign (sgnN X : Nat) (hs : sgnN = 0 ∨ sgnN = 2 ^ 31)
    (hX : X < 2 ^ 31) :
    BitVec.extractLsb 31 31 (BitVec.ofNat 32 (sgnN + X))
      = if sgnN = 2 ^ 31 then 1#1 else 0#1 := by
  apply BitVec.eq_of_toNat_eq
  rw [extract31_toNat _ (by omega)]
  rcases hs with h | h <;> subst h
  · rw [if_neg (by omega)]
    show (0 + X) / 2 ^ 31 % 2 ^ 1 = 0
    omega
  · rw [if_pos rfl]
    show (2 ^ 31 + X) / 2 ^ 31 % 2 ^ 1 = 1
    omega

private theorem extract31_neg (neg : Bool) (X : Nat) (hX : X < 2 ^ 31) :
    BitVec.extractLsb 31 31 (BitVec.ofNat 32 ((if neg then 2 ^ 31 else 0) + X))
      = if neg then 1#1 else 0#1 := by
  rw [extract31_sign _ X (by split <;> simp) hX]
  cases neg <;> decide

private theorem extract31_neg3 (neg : Bool) (X Y : Nat) (hXY : X + Y < 2 ^ 31) :
    BitVec.extractLsb 31 31 (BitVec.ofNat 32 ((if neg then 2 ^ 31 else 0) + X + Y))
      = if neg then 1#1 else 0#1 := by
  have h : (if neg then 2 ^ 31 else 0) + X + Y
      = (if neg then 2 ^ 31 else 0) + (X + Y) := by omega
  rw [h]
  exact extract31_neg neg (X + Y) hXY

/-- The rounder's output carries exactly the requested sign bit — every branch
    of `roundNE_f32` packs `sgn + X` with `X < 2^31`. -/
theorem roundNE_extract_sign (neg : Bool) (sig : Nat) (e0 : Int) :
    BitVec.extractLsb 31 31 (roundNE_f32 neg sig e0)
      = if neg then 1#1 else 0#1 := by
  rw [roundNE_f32]
  by_cases hsig : sig = 0
  · rw [if_pos hsig]
    cases neg <;> decide
  · rw [if_neg hsig]
    by_cases hbias : (Nat.log2 sig : Int) + e0 + 127 ≤ 0
    · simp only [if_pos hbias]
      exact extract31_neg _ _ (Nat.mod_lt _ (by decide))
    · have hkb := keptNorm_bounds sig hsig
      by_cases hp : Nat.log2 sig ≤ 23
      · have hsh : (Nat.log2 sig : Int) + e0 - 23 - e0 ≤ 0 := by omega
        have hnt : (-((Nat.log2 sig : Int) + e0 - 23 - e0)).toNat
            = 23 - Nat.log2 sig := by omega
        have hkeq : sig * 2 ^ (23 - Nat.log2 sig) = keptNorm sig := by
          rw [keptNorm, if_pos hp]
        simp only [if_neg hbias, if_pos hsh, hnt, hkeq, decide_eq_true_eq]
        by_cases hcar : 2 ^ 24 ≤ keptNorm sig
        · simp only [if_pos hcar]
          by_cases ho : (255 : Int) ≤ (Nat.log2 sig : Int) + e0 + 127 + 1
          · rw [if_pos ho]
            exact extract31_neg _ _ (by decide)
          · rw [if_neg ho]
            exact extract31_neg3 _ _ _ (by omega)
        · simp only [if_neg hcar]
          by_cases ho : (255 : Int) ≤ (Nat.log2 sig : Int) + e0 + 127
          · rw [if_pos ho]
            exact extract31_neg _ _ (by decide)
          · rw [if_neg ho]
            exact extract31_neg3 _ _ _ (by omega)
      · have hsh : ¬ ((Nat.log2 sig : Int) + e0 - 23 - e0 ≤ 0) := by omega
        have hnt : ((Nat.log2 sig : Int) + e0 - 23 - e0).toNat
            = Nat.log2 sig - 23 := by omega
        have hkeq : rneQuot sig (Nat.log2 sig - 23) = keptNorm sig := by
          rw [keptNorm, if_neg hp]
        simp only [if_neg hbias, if_neg hsh, hnt, hkeq, decide_eq_true_eq]
        by_cases hcar : 2 ^ 24 ≤ keptNorm sig
        · simp only [if_pos hcar]
          by_cases ho : (255 : Int) ≤ (Nat.log2 sig : Int) + e0 + 127 + 1
          · rw [if_pos ho]
            exact extract31_neg _ _ (by decide)
          · rw [if_neg ho]
            exact extract31_neg3 _ _ _ (by omega)
        · simp only [if_neg hcar]
          by_cases ho : (255 : Int) ≤ (Nat.log2 sig : Int) + e0 + 127
          · rw [if_pos ho]
            exact extract31_neg _ _ (by decide)
          · rw [if_neg ho]
            exact extract31_neg3 _ _ _ (by omega)

-- ── the end-to-end theorem ───────────────────────────────────────────────────

/-- **The end-to-end specification.** `y` is nearest to the exact value
    `V · 2^{-298}` among ALL finite f32 patterns: distances are compared
    exactly, in magnitude, on the fixed `2^{-298}` grid. -/
def IsNearestExact (V : Int) (y : BitVec 32) : Prop :=
  ∀ z : BitVec 32, IsFiniteF32 z →
    scaledDist (f32MagScaled y * 2 ^ 149) V.natAbs
      ≤ scaledDist (f32MagScaled z * 2 ^ 149) V.natAbs

/-- Rescaling bridge: R2's `IsNearestMag` at `(sig, e0)` becomes
    `IsNearestExact V` once `sig·2^{e0+298} = |V|` — both sides of every
    distance inequality are scaled by `2^{298+min(e0,−149)}`. -/
theorem isNearestExact_of_isNearestMag (sig : Nat) (e0 : Int) (V : Int)
    (y : BitVec 32) (he0 : -298 ≤ e0)
    (hval : sig * 2 ^ (e0 + 298).toNat = V.natAbs)
    (hnear : IsNearestMag sig e0 y) : IsNearestExact V y := by
  intro z hz
  have h := Nat.mul_le_mul_right
    (2 ^ (if (-149 : Int) ≤ e0 then 149 else (e0 + 298).toNat)) (hnear z hz)
  rw [← scaledDist_scale, ← scaledDist_scale] at h
  have hty : ∀ m : Nat, m * 2 ^ tScale e0
        * 2 ^ (if (-149 : Int) ≤ e0 then 149 else (e0 + 298).toNat)
      = m * 2 ^ 149 := by
    intro m
    rw [Nat.mul_assoc, ← Nat.pow_add,
      show tScale e0 + (if (-149 : Int) ≤ e0 then 149 else (e0 + 298).toNat)
        = 149 from by rw [tScale]; split <;> omega]
  have hu : sig * 2 ^ uScale e0
        * 2 ^ (if (-149 : Int) ≤ e0 then 149 else (e0 + 298).toNat)
      = V.natAbs := by
    rw [Nat.mul_assoc, ← Nat.pow_add,
      show uScale e0 + (if (-149 : Int) ≤ e0 then 149 else (e0 + 298).toNat)
        = (e0 + 298).toNat from by rw [uScale]; split <;> omega, hval]
  rw [hty, hty, hu] at h
  exact h

/-- **R3, end-to-end correctness of the ARCH fma.** For finite nonzero
    operands whose exact result `a·b + c` is nonzero and in range, the
    sticky-fold datapath returns a finite pattern that is nearest to the
    exact value among ALL finite f32 patterns, carrying the exact sign.
    (`fmaExact` is the exact value in `2^{-298}` units; the range bound is
    the IEEE overflow criterion via `biasedFinal_overflow_iff`.) -/
theorem arch_fma_f32_correct (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true)
    (hnz : fmaExact a b c ≠ 0)
    (hovf : biasedFinal (arch_fma_mag a b c).toNat
      (arch_fma_elo a b c).toInt ≤ 254) :
    IsFiniteF32 (arch_fma_f32 a b c)
      ∧ IsNearestExact (fmaExact a b c) (arch_fma_f32 a b c)
      ∧ BitVec.extractLsb 31 31 (arch_fma_f32 a b c)
          = (if fmaExact a b c < 0 then 1#1 else 0#1) := by
  have hncbv : arch_fma_mag a b c ≠ 0#470 :=
    fun h => hnz ((fma_mag_zero_iff a b c ha hb hc).mp h)
  have hy : arch_fma_f32 a b c
      = roundNE_f32 (arch_fma_sign a b c == 1#1)
          (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt := by
    rw [arch_fma_f32_eq_ref, arch_fma_f32_ref_finite a b c ha hb hc hncbv]
  have hsig0 : (arch_fma_mag a b c).toNat ≠ 0 :=
    fun h => hncbv (BitVec.eq_of_toNat_eq (by rw [h]; rfl))
  obtain ⟨hfin, hnear⟩ := roundNE_nearest (arch_fma_sign a b c == 1#1)
    (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt hsig0 hovf
  obtain ⟨hlo298, _⟩ := fma_elo_bounds a b c ha hb hc
  refine ⟨by rw [hy]; exact hfin, ?_, ?_⟩
  · rw [hy]
    exact isNearestExact_of_isNearestMag _ _ _ _ (by omega)
      (fma_mag_exact a b c ha hb hc) hnear
  · rw [hy, roundNE_extract_sign, fma_sign_exact a b c ha hb hc hnz]
    by_cases hlt : fmaExact a b c < 0
    · rw [if_pos hlt, if_pos (by decide)]
    · rw [if_neg hlt, if_neg (by decide)]

/-- **Overflow companion.** Past the range bound the fma returns infinity
    with the exact sign. -/
theorem arch_fma_f32_overflow (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hc : finiteNonzero c = true)
    (hnz : fmaExact a b c ≠ 0)
    (hovf : 255 ≤ biasedFinal (arch_fma_mag a b c).toNat
      (arch_fma_elo a b c).toInt) :
    arch_fma_f32 a b c
      = BitVec.ofNat 32 ((if fmaExact a b c < 0 then 2 ^ 31 else 0)
          + 0x7F800000) := by
  have hncbv : arch_fma_mag a b c ≠ 0#470 :=
    fun h => hnz ((fma_mag_zero_iff a b c ha hb hc).mp h)
  have hy : arch_fma_f32 a b c
      = roundNE_f32 (arch_fma_sign a b c == 1#1)
          (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt := by
    rw [arch_fma_f32_eq_ref, arch_fma_f32_ref_finite a b c ha hb hc hncbv]
  have hsig0 : (arch_fma_mag a b c).toNat ≠ 0 :=
    fun h => hncbv (BitVec.eq_of_toNat_eq (by rw [h]; rfl))
  have hb01 : biasedFinal (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt
      ≤ ((Nat.log2 (arch_fma_mag a b c).toNat : Int)
          + (arch_fma_elo a b c).toInt + 127) + 1 := by
    rw [biasedFinal]; split <;> omega
  have hbias : 0 < (Nat.log2 (arch_fma_mag a b c).toNat : Int)
      + (arch_fma_elo a b c).toInt + 127 := by omega
  rw [hy, roundNE_overflow _ _ _ hsig0 hbias hovf,
      fma_sign_exact a b c ha hb hc hnz]
  by_cases hlt : fmaExact a b c < 0
  · rw [if_pos hlt, if_pos (by decide), if_pos hlt]
  · rw [if_neg hlt, if_neg (by decide), if_neg hlt]

end ArchFp

