import ArchFpEquiv.RoundFma
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

end ArchFp
