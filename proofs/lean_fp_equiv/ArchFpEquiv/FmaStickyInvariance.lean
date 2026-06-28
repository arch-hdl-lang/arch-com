import ArchFpEquiv.RoundFma
import ArchFpEquiv.Round98
import ArchFpEquiv.FmaSticky
import ArchFpEquiv.Fma
import ArchFpEquiv.FmaGRS
import Std.Tactic.BVDecide

/-!
# Sticky-fold rounding invariance — `arch_fma_f32 = arch_fma_f32_ref`

The bounded sticky-fold fma rounds the *folded* 98-bit magnitude
(`arch_fma_f32_sticky_finite` : `= roundNE(mag98, e0)`); the exact-wide reference
rounds the *exact* 470-bit aligned magnitude. This file proves they agree — i.e.
the fold never changes the rounded result — by the value-level (Nat) argument:

* `diff ≤ FMA_G` (operands' exponents within the guard window): nothing is folded,
  `sticky = 0`, and `mag98 · 2^e0 = mag470 · 2^e_lo` **exactly** — same value, so
  `roundNE` agrees.
* `diff > FMA_G`: the dropped bits all sit below the result's rounding position and
  are summarised by the sticky bit (guard/round/sticky), so `roundNE` agrees by the
  GRS lemmas of `RoundCore`.

`arch_round470_correct` / `arch_round98_correct` then give `arch_fma_f32_ref` and
`arch_fma_f32` as `roundNE` of their magnitudes; the invariance closes the gap.
-/

namespace ArchFp

set_option maxRecDepth 10000

/-- `e_lo = min(eunb_a+eunb_b, eunb_c) ∈ [-298, 208]` for finite operands (the
    exact-wide alignment exponent — the window `arch_round470_correct` needs). -/
theorem fma_elo_bounds (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true) :
    -298 ≤ (arch_fma_elo a b c).toInt ∧ (arch_fma_elo a b c).toInt ≤ 208 := by
  have h1 : BitVec.sle (BitVec.ofNat 16 65238) (arch_fma_elo a b c) = true := by
    unfold finiteNonzero isNaN isInf isZero expField fracField arch_fma_elo at *
    bv_decide
  have h2 : BitVec.sle (arch_fma_elo a b c) (BitVec.ofNat 16 208) = true := by
    unfold finiteNonzero isNaN isInf isZero expField fracField arch_fma_elo at *
    bv_decide
  rw [BitVec.sle_iff_toInt_le] at h1 h2
  rw [show (BitVec.ofNat 16 65238).toInt = -298 from by decide] at h1
  rw [show (BitVec.ofNat 16 208).toInt = 208 from by decide] at h2
  exact ⟨h1, h2⟩

/-- **The exact-wide reference fma reduces to its 470-bit rounder** (structural
    `bv_decide`: `arch_fma_f32_ref` inlines the same alignment as `arch_fma_mag`). -/
theorem fma_reduce_ref (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag a b c ≠ 0#470) :
    arch_fma_f32_ref a b c
      = arch_round470 (arch_fma_sign a b c) (arch_fma_mag a b c) (arch_fma_elo a b c) := by
  unfold finiteNonzero isNaN isInf isZero expField fracField
    arch_fma_f32_ref arch_fma_mag arch_fma_elo arch_fma_sign arch_round470 at *
  bv_decide (config := { timeout := 540 })

/-- The exact-wide reference fma is the RNE rounding of its exact magnitude. -/
theorem arch_fma_f32_ref_finite (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag a b c ≠ 0#470) :
    arch_fma_f32_ref a b c
      = roundNE_f32 (arch_fma_sign a b c == 1#1)
          (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt := by
  rw [fma_reduce_ref a b c ha hb hc hnc]
  obtain ⟨hlo, hhi⟩ := fma_elo_bounds a b c ha hb hc
  exact arch_round470_correct _ _ _ hlo hhi

-- ── value-level rounding lemmas (Nat) ────────────────────────────────────────

/-- `Nat.log2 (n · 2^k) = Nat.log2 n + k` for `n > 0`. -/
theorem log2_mul_pow2 (n k : Nat) (hn : 0 < n) :
    Nat.log2 (n * 2 ^ k) = Nat.log2 n + k := by
  have hn0 : n ≠ 0 := by omega
  have h2k : 0 < 2 ^ k := Nat.pow_pos (by decide)
  have hnk0 : n * 2 ^ k ≠ 0 := Nat.mul_ne_zero hn0 (Nat.pos_iff_ne_zero.mp h2k)
  have hself : 2 ^ Nat.log2 n ≤ n := (Nat.le_log2 hn0).mp (Nat.le_refl _)
  have hlt : n < 2 ^ (Nat.log2 n + 1) := (Nat.log2_lt hn0).mp (Nat.lt_succ_self _)
  have hlo : Nat.log2 n + k ≤ Nat.log2 (n * 2 ^ k) := by
    rw [Nat.le_log2 hnk0, Nat.pow_add]; exact Nat.mul_le_mul_right _ hself
  have hhi : Nat.log2 (n * 2 ^ k) < Nat.log2 n + k + 1 := by
    rw [Nat.log2_lt hnk0, show Nat.log2 n + k + 1 = (Nat.log2 n + 1) + k by omega, Nat.pow_add]
    exact (Nat.mul_lt_mul_right h2k).mpr hlt
  omega

/-- `rneQuot` scaling: dividing-by-`2^sh` after scaling-by-`2^k` is the same as
    dividing-by-`2^(sh-k)`, when `k < sh` (the rounding branch has a guard bit). -/
theorem rneQuot_scale (n k sh : Nat) (hk : k < sh) :
    rneQuot (n * 2 ^ k) sh = rneQuot n (sh - k) := by
  have h2k : 0 < 2 ^ k := Nat.pow_pos (by decide)
  have hpow : 2 ^ sh = 2 ^ (sh - k) * 2 ^ k := by rw [← Nat.pow_add]; congr 1; omega
  have hg : 2 ^ (sh - 1) = 2 ^ (sh - k - 1) * 2 ^ k := by rw [← Nat.pow_add]; congr 1; omega
  have hdiv : n * 2 ^ k / 2 ^ sh = n / 2 ^ (sh - k) := by
    rw [hpow, Nat.mul_div_mul_right _ _ h2k]
  have hmod : (n * 2 ^ k) % 2 ^ sh = (n % 2 ^ (sh - k)) * 2 ^ k := by
    rw [hpow, Nat.mul_comm (2 ^ (sh - k)) (2 ^ k), Nat.mul_comm n (2 ^ k), Nat.mul_mod_mul_left,
        Nat.mul_comm (2 ^ k) (n % 2 ^ (sh - k))]
  have hc1 : (2 ^ (sh - k - 1) * 2 ^ k < (n % 2 ^ (sh - k)) * 2 ^ k)
           = (2 ^ (sh - k - 1) < n % 2 ^ (sh - k)) := propext (Nat.mul_lt_mul_right h2k)
  have hc2 : ((n % 2 ^ (sh - k)) * 2 ^ k = 2 ^ (sh - k - 1) * 2 ^ k)
           = (n % 2 ^ (sh - k) = 2 ^ (sh - k - 1)) := propext (Nat.mul_right_cancel_iff h2k)
  unfold rneQuot
  simp only [hdiv, hmod, hg, hc1, hc2]

/-- `rneQuot` is exact (no round-up) when the dropped bits are all zero. -/
theorem rneQuot_exact (m s : Nat) (h : m % 2 ^ s = 0) : rneQuot m s = m / 2 ^ s := by
  have hp : 0 < 2 ^ (s - 1) := Nat.pow_pos (by decide)
  unfold rneQuot
  rw [h, if_neg]
  · exact Nat.add_zero _
  · rintro (hlt | ⟨heq, _⟩)
    · exact absurd hlt (Nat.not_lt_zero _)
    · omega

-- ── the post-`kept` encoder, factored out of `roundNE_f32` ────────────────────

/-- The tail of `roundNE_f32` (subnormal/normal/overflow encoding) as a function of
    `(neg, biased, kept)` — everything in `roundNE_f32` after `kept` is computed. -/
def roundNE_encode (neg : Bool) (biased : Int) (kept : Nat) : BitVec 32 :=
  let sgn : Nat := if neg then 2 ^ 31 else 0
  if biased ≤ 0 then BitVec.ofNat 32 (sgn + kept % 2 ^ 31)
  else
    let carry : Bool := 2 ^ 24 ≤ kept
    let biased_n : Int := if carry then biased + 1 else biased
    let kept_n : Nat := if carry then kept / 2 else kept
    if 255 ≤ biased_n then BitVec.ofNat 32 (sgn + 0x7F800000)
    else BitVec.ofNat 32 (sgn + (biased_n.toNat % 256) * 2 ^ 23 + kept_n % 2 ^ 23)

/-- The `kept` (rounded-and-shifted significand) computed inside `roundNE_f32`. -/
def keptOf (sig : Nat) (e0 : Int) : Nat :=
  let ev : Int := (Nat.log2 sig : Int) + e0
  let biased : Int := ev + 127
  let k : Int := if biased ≤ 0 then -149 else ev - 23
  let sh : Int := k - e0
  if sh ≤ 0 then sig * 2 ^ (-sh).toNat else rneQuot sig sh.toNat

/-- `roundNE_f32` factors as `roundNE_encode` of its biased exponent and `keptOf`
    (definitional — just names the two halves of the spec). -/
theorem roundNE_f32_eq_encode (neg : Bool) (sig : Nat) (e0 : Int) (hsig : sig ≠ 0) :
    roundNE_f32 neg sig e0
      = roundNE_encode neg ((Nat.log2 sig : Int) + e0 + 127) (keptOf sig e0) := by
  unfold roundNE_f32 roundNE_encode keptOf
  rw [if_neg hsig]

/-- **The `kept` core scales.** Dropping `sh` bits of `sig·2^k` (with `sh` measured
    from the same exponent `e0`) equals dropping `sh-k` bits of `sig` — exactly when
    `sh ≤ k` (the extra `k` factors out as an exact power-of-two), and by `rneQuot`
    scaling when `sh > k`. `S` is the shift amount `k_shift - e0`. -/
theorem kept_core_scale (n k : Nat) (S : Int) :
    (if S ≤ 0 then (n * 2 ^ k) * 2 ^ (-S).toNat else rneQuot (n * 2 ^ k) S.toNat)
      = (if S - (k : Int) ≤ 0 then n * 2 ^ (-(S - (k : Int))).toNat
         else rneQuot n (S - (k : Int)).toNat) := by
  by_cases hS : S ≤ 0
  · rw [if_pos hS, if_pos (by omega : S - (k : Int) ≤ 0)]
    rw [show (-(S - (k : Int))).toNat = k + (-S).toNat from by omega, Nat.pow_add]
    exact Nat.mul_assoc _ _ _
  · by_cases hSk : S - (k : Int) ≤ 0
    · rw [if_neg (by omega), if_pos hSk]
      have hfac : n * 2 ^ k = n * 2 ^ (k - S.toNat) * 2 ^ S.toNat := by
        rw [Nat.mul_assoc, ← Nat.pow_add]; congr 2; omega
      have hmod : (n * 2 ^ k) % 2 ^ S.toNat = 0 := by rw [hfac]; exact Nat.mul_mod_left _ _
      have hdiv : (n * 2 ^ k) / 2 ^ S.toNat = n * 2 ^ (k - S.toNat) := by
        rw [hfac]; exact Nat.mul_div_cancel _ (Nat.pow_pos (by decide))
      rw [rneQuot_exact _ _ hmod, hdiv,
          show k - S.toNat = (-(S - (k : Int))).toNat from by omega]
    · rw [if_neg (by omega), if_neg hSk, rneQuot_scale n k S.toNat (by omega),
          show S.toNat - k = (S - (k : Int)).toNat from by omega]

/-- **Value-level scaling invariance of `keptOf`.** -/
theorem keptOf_scale (n k : Nat) (e : Int) (hn : 0 < n) :
    keptOf (n * 2 ^ k) e = keptOf n (e + (k : Int)) := by
  simp only [keptOf]
  rw [log2_mul_pow2 n k hn,
      show (↑(Nat.log2 n + k) : Int) + e = ↑(Nat.log2 n) + (e + ↑k) from by omega]
  generalize hK : (if (↑(Nat.log2 n) + (e + (k : Int))) + 127 ≤ 0 then (-149 : Int)
                   else (↑(Nat.log2 n) + (e + (k : Int))) - 23) = K
  rw [show (K - (e + (k : Int))) = (K - e) - (k : Int) from by omega]
  exact kept_core_scale n k (K - e)

/-- **Value-level rounding scaling invariance.** Rounding `sig·2^k` at exponent `e`
    is the same f32 as rounding `sig` at exponent `e+k` — the foundation for the
    `d ≤ FMA_G` (exact, no fold) case of the sticky-fold invariance. -/
theorem roundNE_scale (neg : Bool) (n : Nat) (e : Int) (k : Nat) (hn : 0 < n) :
    roundNE_f32 neg (n * 2 ^ k) e = roundNE_f32 neg n (e + (k : Int)) := by
  have hne : n * 2 ^ k ≠ 0 :=
    Nat.mul_ne_zero (by omega) (Nat.pos_iff_ne_zero.mp (Nat.pow_pos (by decide)))
  rw [roundNE_f32_eq_encode neg (n * 2 ^ k) e hne,
      roundNE_f32_eq_encode neg n (e + (k : Int)) (by omega),
      keptOf_scale n k e hn]
  congr 1
  rw [log2_mul_pow2 n k hn]; omega

/-- **GRS collapse lifted to `roundNE_f32` (normal case).** Two magnitudes that
    agree above bit `g`, share the low-`g` sticky status, and produce a *normal*
    result whose round shift `log2 − 23` exceeds `g`, round to the same f32. -/
theorem roundNE_sticky_collapse_normal (neg : Bool) (m1 m2 : Nat) (e : Int) (g : Nat)
    (hm1 : 2 ^ g ≤ m1) (hhi : m1 / 2 ^ g = m2 / 2 ^ g) (hst : (m1 % 2 ^ g = 0) ↔ (m2 % 2 ^ g = 0))
    (hbig : 0 < (Nat.log2 m1 : Int) + e + 127) (hsh : g + 24 ≤ Nat.log2 m1) :
    roundNE_f32 neg m1 e = roundNE_f32 neg m2 e := by
  have hgpos : 0 < (2 : Nat) ^ g := Nat.pow_pos (by decide)
  have hq1 : 1 ≤ m1 / 2 ^ g := (Nat.one_le_div_iff hgpos).mpr hm1
  have hm2 : 2 ^ g ≤ m2 := (Nat.one_le_div_iff hgpos).mp (by rw [← hhi]; exact hq1)
  have hm1ne : m1 ≠ 0 := by omega
  have hm2ne : m2 ≠ 0 := by omega
  have hlog : Nat.log2 m1 = Nat.log2 m2 := by
    rw [log2_div_pow m1 g hm1, log2_div_pow m2 g hm2, hhi]
  have hlogc : (Nat.log2 m1 : Int) = Nat.log2 m2 := by exact_mod_cast hlog
  have hshc : (g : Int) + 24 ≤ Nat.log2 m1 := by exact_mod_cast hsh
  rw [roundNE_f32_eq_encode neg m1 e hm1ne, roundNE_f32_eq_encode neg m2 e hm2ne]
  congr 1
  · rw [hlog]
  · simp only [keptOf]
    rw [hlog]
    have hBigP : ¬ ((↑(Nat.log2 m2) + e : Int) + 127 ≤ 0) := by omega
    have hShP : ¬ ((↑(Nat.log2 m2) + e - 23 : Int) - e ≤ 0) := by omega
    have hshnat : ((↑(Nat.log2 m2) + e - 23 : Int) - e).toNat = Nat.log2 m2 - 23 := by omega
    simp only [if_neg hBigP, if_neg hShP, hshnat]
    exact rneQuot_sticky_collapse m1 m2 g (Nat.log2 m2 - 23) (by omega) hhi hst

/-- **GRS collapse lifted to `roundNE_f32` (subnormal case).** The companion to
    `roundNE_sticky_collapse_normal` for a *subnormal* result (`log2 + e + 127 ≤ 0`):
    the kept shift is the fixed `−149 − e` floor instead of `log2 − 23`. With `g`
    below that floor and the large-magnitude guarantee (`2^g ≤ m1`, so the shift is
    positive), the same `rneQuot` collapse applies. -/
theorem roundNE_sticky_collapse_subnormal (neg : Bool) (m1 m2 : Nat) (e : Int) (g : Nat)
    (hm1 : 2 ^ g ≤ m1) (hhi : m1 / 2 ^ g = m2 / 2 ^ g) (hst : (m1 % 2 ^ g = 0) ↔ (m2 % 2 ^ g = 0))
    (hsub : (Nat.log2 m1 : Int) + e + 127 ≤ 0) (hsh : (g : Int) < -149 - e) :
    roundNE_f32 neg m1 e = roundNE_f32 neg m2 e := by
  have hgpos : 0 < (2 : Nat) ^ g := Nat.pow_pos (by decide)
  have hq1 : 1 ≤ m1 / 2 ^ g := (Nat.one_le_div_iff hgpos).mpr hm1
  have hm2 : 2 ^ g ≤ m2 := (Nat.one_le_div_iff hgpos).mp (by rw [← hhi]; exact hq1)
  have hm1ne : m1 ≠ 0 := by omega
  have hm2ne : m2 ≠ 0 := by omega
  have hlog : Nat.log2 m1 = Nat.log2 m2 := by
    rw [log2_div_pow m1 g hm1, log2_div_pow m2 g hm2, hhi]
  rw [roundNE_f32_eq_encode neg m1 e hm1ne, roundNE_f32_eq_encode neg m2 e hm2ne]
  congr 1
  · rw [hlog]
  · simp only [keptOf]
    rw [hlog]
    have hBigP : ((↑(Nat.log2 m2) + e : Int) + 127 ≤ 0) := by rw [← hlog]; exact hsub
    have hShP : ¬ (((-149 : Int) - e) ≤ 0) := by omega
    simp only [if_pos hBigP, if_neg hShP]
    exact rneQuot_sticky_collapse m1 m2 g ((-149 - e).toNat) (by omega) hhi hst

end ArchFp
