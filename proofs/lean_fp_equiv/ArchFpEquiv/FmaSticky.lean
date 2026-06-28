import ArchFpEquiv.RoundFma
import ArchFpEquiv.Round98
import ArchFpEquiv.Spec
import Std.Tactic.BVDecide

/-!
# Tier 2, sticky-fold fma — finite correctness (width-98 rounder)

`arch_fma_f32` now rounds the **bounded sticky-fold** aligned magnitude at
`mw = (48 + FMA_G) + 2 = 98` bits (FMA_G = 48), instead of the exact-wide 470.
This file reduces the new fma to the width-98 rounder, mirroring `Fma.fma_reduce`
at the smaller width.

* `fma_reduce98` — on the finite non-cancelling path, `arch_fma_f32` *is* the
  shared rounder `arch_round98` applied to the sticky-fold magnitude
  `arch_fma_mag98` at exponent `arch_fma_elo98`. Structural `bv_decide`: the
  24×24 multiplier and the alignment shift/fold occur identically on both sides
  (the model inlines them; `arch_fma_mag98` recomputes them), so this is an
  identity, not a multiplier-equivalence — and at 98 bits it stays well inside
  `bv_decide`'s reach.

Remaining (in progress): `arch_round98_correct` (the width-98 rounder is
correctly-rounded — the width-98 analogue of `arch_round470_correct`) and the
sticky-fold rounding-invariance lemma `RNE(mag98·2^e0) = RNE(exact a·b+c)`.
-/

namespace ArchFp

set_option maxRecDepth 10000

/-- **The finite sticky-fold fma reduction.** Non-cancelling finite `fma` = the
    width-98 rounder on the bounded sticky-fold magnitude. Structural `bv_decide`. -/
theorem fma_reduce98 (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag98 a b c ≠ 0#98) :
    arch_fma_f32 a b c
      = arch_round98 (arch_fma_sign98 a b c) (arch_fma_mag98 a b c) (arch_fma_elo98 a b c) := by
  unfold finiteNonzero isNaN isInf isZero expField fracField
    arch_fma_f32 arch_fma_mag98 arch_fma_elo98 arch_fma_sign98 arch_round98 at *
  bv_decide (config := { timeout := 600 })

/-- `e0 = e_hi - (FMA_G+1) ∈ [-298, 208]` for finite operands — the new fma's
    actual range is `[-198, 159] ⊂ [-298, 208]`, so it meets `arch_round98_correct`'s
    window. -/
theorem fma_elo98_bounds (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true) :
    -298 ≤ (arch_fma_elo98 a b c).toInt ∧ (arch_fma_elo98 a b c).toInt ≤ 208 := by
  have h1 : BitVec.sle (BitVec.ofNat 16 65238) (arch_fma_elo98 a b c) = true := by
    unfold finiteNonzero isNaN isInf isZero expField fracField arch_fma_elo98 at *
    bv_decide
  have h2 : BitVec.sle (arch_fma_elo98 a b c) (BitVec.ofNat 16 208) = true := by
    unfold finiteNonzero isNaN isInf isZero expField fracField arch_fma_elo98 at *
    bv_decide
  rw [BitVec.sle_iff_toInt_le] at h1 h2
  rw [show (BitVec.ofNat 16 65238).toInt = -298 from by decide] at h1
  rw [show (BitVec.ofNat 16 208).toInt = 208 from by decide] at h2
  exact ⟨h1, h2⟩

/-- **The sticky-fold fma correctly rounds its computed 98-bit magnitude.** Finite
    non-cancelling: `arch_fma_f32 a b c = RNE( (sign)·mag98·2^e0 )`. The structured
    analogue of the old `arch_fma_f32_finite_correct` at the new rounder width. (The
    remaining obligation — that `mag98·2^e0` rounds like the *exact* `a·b+c` — is the
    sticky-fold invariance lemma.) -/
theorem arch_fma_f32_sticky_finite (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag98 a b c ≠ 0#98) :
    arch_fma_f32 a b c
      = roundNE_f32 (arch_fma_sign98 a b c == 1#1)
          (arch_fma_mag98 a b c).toNat (arch_fma_elo98 a b c).toInt := by
  rw [fma_reduce98 a b c ha hb hc hnc]
  obtain ⟨hlo, hhi⟩ := fma_elo98_bounds a b c ha hb hc
  exact arch_round98_correct _ _ _ hlo hhi

/-- Exact cancellation (`mag98 = 0`) of a finite sticky-fold fma is `+0`
    (the width-98 analogue of the old `Fma.fma_cancel`). -/
theorem fma_cancel98 (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hcanc : arch_fma_mag98 a b c = 0#98) :
    arch_fma_f32 a b c = 0#32 := by
  unfold finiteNonzero isNaN isInf isZero expField fracField arch_fma_f32 arch_fma_mag98 at *
  bv_decide (config := { timeout := 300 })

end ArchFp
