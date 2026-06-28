import ArchFpEquiv.RoundFma
import ArchFpEquiv.Round98
import ArchFpEquiv.FmaSticky
import ArchFpEquiv.Fma
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

end ArchFp
