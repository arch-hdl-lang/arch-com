import ArchFpEquiv.RoundFma
import ArchFpEquiv.Spec
import Std.Tactic.BVDecide

/-!
# Tier 2, fma — special-value lattice

`arch_fma_f32` now rounds the **bounded sticky-fold** aligned magnitude at width 98
(see `FmaSticky.lean` for the finite correctly-rounded reduction `fma_reduce98` /
`arch_fma_f32_sticky_finite`, and the width-98 rounder `Round98.arch_round98_correct`).

This file keeps the **special-value lattice** — the NaN / Inf / zero-product cases,
whose code paths are identical to the exact-wide implementation, so they are
re-checked by `bv_decide` directly on `arch_fma_f32`. (The finite-rounding and
exact-cancellation reductions moved to `FmaSticky.lean` at the new rounder width.)
-/

namespace ArchFp

set_option maxRecDepth 10000

-- ── special-value lattice (machine-checked by bv_decide) ─────────────────────

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

-- (`fma_c_zero_correct` — the `c = 0` path rounds the 48-bit product — moves to
--  `FmaSticky.lean` as `fma_c_zero98` against the new prod-only rounder. The
--  general finite path is `FmaSticky.arch_fma_f32_sticky_finite`.)

/-- **Zero product: fma reduces to the (proved) adder.** With `a` or `b` zero and
    `c` finite, `arch_fma_f32 a b c = arch_f32_add (±0) c`. -/
theorem fma_prod_zero (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false)
    (hpa : isInf a = false) (hpb : isInf b = false)
    (hnc : isNaN c = false) (hci : isInf c = false)
    (hpz : isZero a = true ∨ isZero b = true) :
    arch_fma_f32 a b c = arch_f32_add ((sgn a ^^^ sgn b) ++ 0#31) c := by
  unfold isNaN isInf isZero expField fracField sgn arch_fma_f32 at *
  bv_decide (config := { timeout := 300 })

end ArchFp
