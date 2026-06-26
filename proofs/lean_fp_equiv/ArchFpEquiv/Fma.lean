import ArchFpEquiv.RoundFma
import ArchFpEquiv.Spec
import Std.Tactic.BVDecide

/-!
# Tier 2, fma — finite correctness (derived from the width-470 rounder)

`arch_fma_f32` rounds the **exact** aligned product±addend at `FMA_W = 470` bits
(exact-wide alignment, so no sticky-fold approximation). This file binds it to the
proved fma-width rounder (`RoundFma.arch_round470_correct`):

* `fma_reduce` — on the finite non-cancelling path, the model's fma *is* the shared
  rounder applied to the exact aligned magnitude `mag` at exponent `e_lo`. Proved by
  `bv_decide`: the 24×24 multiplier and the alignment shifts occur identically on
  both sides (the model inlines them; `arch_fma_mag` recomputes them), so this is a
  structural identity, **not** a multiplier-equivalence. (`mag = 0 ⟺ exact
  cancellation`, so `mag ≠ 0` selects the rounded branch.)
* `fma_elo_bounds` — `e_lo ∈ [-298, 208]` for finite operands (the window the
  rounder needs), discharged from `finiteNonzero` via the signed `sle` form.
* `arch_fma_f32_finite_correct` — derived: finite, non-cancelling `fma` is the RNE
  rounding of the exact `(sign) · mag · 2^e_lo`. Only unproved input is the rounder
  crux, now closed at width 470.
-/

namespace ArchFp

set_option maxRecDepth 10000

/-- **The finite fma reduction.** Non-cancelling finite `fma` = the shared rounder
    on the exact aligned magnitude. `bv_decide`, structural (multiplier identical
    on both sides). -/
theorem fma_reduce (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag a b c ≠ 0#470) :
    arch_fma_f32 a b c
      = arch_round470 (arch_fma_sign a b c) (arch_fma_mag a b c) (arch_fma_elo a b c) := by
  unfold finiteNonzero isNaN isInf isZero expField fracField
    arch_fma_f32 arch_fma_mag arch_fma_elo arch_fma_sign arch_round470 at *
  bv_decide (config := { timeout := 540 })

/-- `e_lo = min(eunb_a+eunb_b, eunb_c) ∈ [-298, 208]` for finite operands. -/
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

/-- **Finite fma is correctly rounded** — derived from the reduction and the
    fma-width rounder crux. For finite `a b c` with non-cancelling magnitude,
    `arch_fma_f32 a b c` is the RNE rounding of the exact `(sign)·mag·2^e_lo`. -/
theorem arch_fma_f32_finite_correct (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag a b c ≠ 0#470) :
    arch_fma_f32 a b c
      = roundNE_f32 (arch_fma_sign a b c == 1#1)
          (arch_fma_mag a b c).toNat (arch_fma_elo a b c).toInt := by
  rw [fma_reduce a b c ha hb hc hnc]
  obtain ⟨hlo, hhi⟩ := fma_elo_bounds a b c ha hb hc
  exact arch_round470_correct _ _ _ hlo hhi

-- ── special-value lattice (machine-checked by bv_decide) ─────────────────────

/-- Exact cancellation (`mag = 0`) of a finite fma is `+0`. Completes the finite
    case alongside `arch_fma_f32_finite_correct` (`mag ≠ 0`). -/
theorem fma_cancel (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hcanc : arch_fma_mag a b c = 0#470) :
    arch_fma_f32 a b c = 0#32 := by
  unfold finiteNonzero isNaN isInf isZero expField fracField arch_fma_f32 arch_fma_mag at *
  bv_decide (config := { timeout := 300 })

/-- A NaN operand makes the fma NaN (canonical `0x7FC00000`). -/
theorem fma_nan (a b c : BitVec 32)
    (h : isNaN a = true ∨ isNaN b = true ∨ isNaN c = true) :
    arch_fma_f32 a b c = BitVec.ofNat 32 2143289344 := by
  unfold isNaN expField fracField arch_fma_f32 at *
  bv_decide (config := { timeout := 300 })

/-- `0 · ∞ ± c` is NaN. -/
theorem fma_zero_times_inf (a b c : BitVec 32)
    (h : (isZero a = true ∧ isInf b = true) ∨ (isInf a = true ∧ isZero b = true)) :
    arch_fma_f32 a b c = BitVec.ofNat 32 2143289344 := by
  unfold isZero isInf expField fracField arch_fma_f32 at *
  bv_decide (config := { timeout := 300 })

/-- An infinite addend whose sign opposes an infinite product gives NaN
    (`∞ − ∞`). -/
theorem fma_inf_minus_inf (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false)
    (hpi : isInf a = true ∨ isInf b = true)
    (hci : isInf c = true) (hsgn : sgn c ≠ sgn a ^^^ sgn b) :
    arch_fma_f32 a b c = BitVec.ofNat 32 2143289344 := by
  unfold isNaN isInf expField fracField sgn arch_fma_f32 at *
  bv_decide (config := { timeout := 300 })

/-- An infinite product (not `0·∞`, addend not the opposite infinity) gives the
    product-signed infinity. -/
theorem fma_inf_prod (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false) (hnc : isNaN c = false)
    (hpi : isInf a = true ∨ isInf b = true)
    (hzti : ¬((isZero a = true ∧ isInf b = true) ∨ (isInf a = true ∧ isZero b = true)))
    (hcc : isInf c = false ∨ sgn c = sgn a ^^^ sgn b) :
    arch_fma_f32 a b c = (sgn a ^^^ sgn b) ++ (0xFF#8 ++ 0#23) := by
  unfold isNaN isInf isZero expField fracField sgn arch_fma_f32 at *
  bv_decide (config := { timeout := 300 })

/-- A finite product plus an infinite addend gives the addend's infinity. -/
theorem fma_inf_c (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false) (hnc : isNaN c = false)
    (hpa : isInf a = false) (hpb : isInf b = false) (hci : isInf c = true) :
    arch_fma_f32 a b c = sgn c ++ (0xFF#8 ++ 0#23) := by
  unfold isNaN isInf expField fracField sgn arch_fma_f32 at *
  bv_decide (config := { timeout := 300 })

end ArchFp
