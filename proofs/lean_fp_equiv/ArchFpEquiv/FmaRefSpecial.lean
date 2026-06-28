import ArchFpEquiv.RoundFma
import ArchFpEquiv.Spec
import Std.Tactic.BVDecide

/-!
# Reference fma — special-value lattice

The mirror of `Fma.lean` for the exact-wide reference `arch_fma_f32_ref`: the
NaN / Inf / zero-product code paths produce the same results as the sticky-fold
`arch_fma_f32`, re-checked by `bv_decide` directly on the 470-bit reference. With
both lattices in hand, the special-value cases of the equivalence theorem are a
`rw` of the sticky lemma followed by the reference lemma (both to the same value).
-/

namespace ArchFp

set_option maxRecDepth 10000

/-- A NaN operand makes the reference fma NaN (canonical `0x7FC00000`). -/
theorem fma_ref_nan (a b c : BitVec 32)
    (h : isNaN a = true ∨ isNaN b = true ∨ isNaN c = true) :
    arch_fma_f32_ref a b c = BitVec.ofNat 32 2143289344 := by
  unfold isNaN expField fracField arch_fma_f32_ref at *
  bv_decide (config := { timeout := 300 })

/-- `0 · ∞ ± c` is NaN (reference). -/
theorem fma_ref_zero_times_inf (a b c : BitVec 32)
    (h : (isZero a = true ∧ isInf b = true) ∨ (isInf a = true ∧ isZero b = true)) :
    arch_fma_f32_ref a b c = BitVec.ofNat 32 2143289344 := by
  unfold isZero isInf expField fracField arch_fma_f32_ref at *
  bv_decide (config := { timeout := 300 })

/-- `∞ − ∞` is NaN (reference). -/
theorem fma_ref_inf_minus_inf (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false)
    (hpi : isInf a = true ∨ isInf b = true)
    (hci : isInf c = true) (hsgn : sgn c ≠ sgn a ^^^ sgn b) :
    arch_fma_f32_ref a b c = BitVec.ofNat 32 2143289344 := by
  unfold isNaN isInf expField fracField sgn arch_fma_f32_ref at *
  bv_decide (config := { timeout := 300 })

/-- An infinite product gives product-signed infinity (reference). -/
theorem fma_ref_inf_prod (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false) (hnc : isNaN c = false)
    (hpi : isInf a = true ∨ isInf b = true)
    (hzti : ¬((isZero a = true ∧ isInf b = true) ∨ (isInf a = true ∧ isZero b = true)))
    (hcc : isInf c = false ∨ sgn c = sgn a ^^^ sgn b) :
    arch_fma_f32_ref a b c = (sgn a ^^^ sgn b) ++ (0xFF#8 ++ 0#23) := by
  unfold isNaN isInf isZero expField fracField sgn arch_fma_f32_ref at *
  bv_decide (config := { timeout := 300 })

/-- A finite product plus an infinite addend gives the addend's infinity (reference). -/
theorem fma_ref_inf_c (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false) (hnc : isNaN c = false)
    (hpa : isInf a = false) (hpb : isInf b = false) (hci : isInf c = true) :
    arch_fma_f32_ref a b c = sgn c ++ (0xFF#8 ++ 0#23) := by
  unfold isNaN isInf expField fracField sgn arch_fma_f32_ref at *
  bv_decide (config := { timeout := 300 })

/-- **Zero product (reference).** With `a` or `b` zero and `c` finite, the
    reference fma reduces to the adder, exactly as the sticky-fold version. -/
theorem fma_ref_prod_zero (a b c : BitVec 32)
    (hna : isNaN a = false) (hnb : isNaN b = false)
    (hpa : isInf a = false) (hpb : isInf b = false)
    (hnc : isNaN c = false) (hci : isInf c = false)
    (hpz : isZero a = true ∨ isZero b = true) :
    arch_fma_f32_ref a b c = arch_f32_add ((sgn a ^^^ sgn b) ++ 0#31) c := by
  unfold isNaN isInf isZero expField fracField sgn arch_fma_f32_ref at *
  bv_decide (config := { timeout := 300 })

end ArchFp
