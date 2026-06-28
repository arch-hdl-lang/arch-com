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

end ArchFp
