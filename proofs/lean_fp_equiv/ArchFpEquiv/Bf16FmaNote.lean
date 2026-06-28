import ArchFpEquiv.Fma
import ArchFpEquiv.FmaSticky

/-!
# `bf16_fma` is fused f32-accumulate — NOT correctly-rounded bf16 fma

`fp_ops.rs::bf16_fma` composes widen→f32 fma→narrow. The f32 fma is correctly
rounded (proved in `Fma.lean`); the final f32→bf16 narrow is a *second* rounding,
and double-rounding here is not innocuous. So the result is the f32-accumulate
value, which differs from the correctly-rounded `a·b+c` on ~0.37% of finite inputs
(always 1 ULP). The witness below is pinned as a regression so this stays visible.

This matches the mainstream f32-accumulate convention; the narrow is bit-identical
to PyTorch `round_to_nearest_even`, and arch bf16 mul/add/sub match PyTorch
`c10::BFloat16` bit-for-bit. See `proofs/lean_fp_equiv/README.md` for the analysis.
-/

namespace ArchFp

/-- bf16 fma exactly as `fp_ops.rs::bf16_fma` composes it. -/
def archBf16Fma (a b c : BitVec 16) : BitVec 16 :=
  arch_f32_to_bf16 (arch_fma_f32 (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c))

-- Witness `a=0x2a20, b=0x51a6, c=0x9359`: arch gives the f32-accumulate result
-- 0x3c50; the correctly-rounded bf16 fma is 0x3c4f (the f32 result is exactly a
-- bf16 midpoint, so the narrow ties-to-even up). They differ by 1 ULP.
#guard archBf16Fma 0x2a20#16 0x51a6#16 0x9359#16 = 0x3c50#16
#guard archBf16Fma 0x2a20#16 0x51a6#16 0x9359#16 ≠ 0x3c4f#16

/-- **The *true* bf16-fma characterization (f32-accumulate).** For bf16 operands
    whose widened f32 values are finite nonzero and non-cancelling, `archBf16Fma`
    is the f32→bf16 narrowing of the **correctly-rounded** f32 fma — i.e. `bf16_fma
    = narrow(RNE_f32(a·b+c))`. This is the honest statement (derived directly from
    the proved `arch_fma_f32_finite_correct`); the stricter `= RNE_bf16(a·b+c)` is
    *false* (the `#guard` witness above). -/
theorem archBf16Fma_eq_narrow_roundNE (a b c : BitVec 16)
    (ha : finiteNonzero (arch_bf16_to_f32 a) = true)
    (hb : finiteNonzero (arch_bf16_to_f32 b) = true)
    (hc : finiteNonzero (arch_bf16_to_f32 c) = true)
    (hnc : arch_fma_mag98 (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c) ≠ 0#98) :
    archBf16Fma a b c
      = arch_f32_to_bf16
          (roundNE_f32
            (arch_fma_sign98 (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c) == 1#1)
            (arch_fma_mag98 (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c)).toNat
            (arch_fma_elo98 (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c)).toInt) := by
  unfold archBf16Fma
  rw [arch_fma_f32_sticky_finite _ _ _ ha hb hc hnc]

/-! ## Special-value composition (closes the non-finite path)

The finite characterization above covers the rounded path. The remaining
NaN / Inf / Zero-cancel cases compose the **proved** f32 special-value lattice
(`Fma.lean`) with the **exhaustively-proved** `narrow` (`arch_f32_to_bf16`).
Hypotheses are stated on the *widened* operands (`arch_bf16_to_f32 _`), exactly
matching the f32 lattice — and widening preserves NaN/Inf/Zero, so a bf16
NaN/Inf/Zero operand satisfies them. Concrete bf16 outputs are pinned where the
result is a constant; the ±∞ cases keep the (single, free) sign bit symbolic.
Together with `archBf16Fma_eq_narrow_roundNE` this is the full bf16-fma lattice. -/

/-- A NaN (widened) operand ⇒ the canonical bf16 NaN `0x7FC0`. -/
theorem archBf16Fma_nan (a b c : BitVec 16)
    (h : isNaN (arch_bf16_to_f32 a) = true ∨ isNaN (arch_bf16_to_f32 b) = true
         ∨ isNaN (arch_bf16_to_f32 c) = true) :
    archBf16Fma a b c = 0x7FC0#16 := by
  unfold archBf16Fma
  rw [fma_nan _ _ _ h]
  unfold arch_f32_to_bf16
  bv_decide

/-- Exact cancellation of finite-nonzero (widened) operands ⇒ bf16 `+0`. -/
theorem archBf16Fma_cancel (a b c : BitVec 16)
    (ha : finiteNonzero (arch_bf16_to_f32 a) = true)
    (hb : finiteNonzero (arch_bf16_to_f32 b) = true)
    (hc : finiteNonzero (arch_bf16_to_f32 c) = true)
    (hcanc : arch_fma_mag98 (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c) = 0#98) :
    archBf16Fma a b c = 0#16 := by
  unfold archBf16Fma
  rw [fma_cancel98 _ _ _ ha hb hc hcanc]
  unfold arch_f32_to_bf16
  bv_decide

/-- `0 · ∞ ± c` (widened) ⇒ the canonical bf16 NaN `0x7FC0`. -/
theorem archBf16Fma_zero_times_inf (a b c : BitVec 16)
    (h : (isZero (arch_bf16_to_f32 a) = true ∧ isInf (arch_bf16_to_f32 b) = true)
       ∨ (isInf (arch_bf16_to_f32 a) = true ∧ isZero (arch_bf16_to_f32 b) = true)) :
    archBf16Fma a b c = 0x7FC0#16 := by
  unfold archBf16Fma
  rw [fma_zero_times_inf _ _ _ h]
  unfold arch_f32_to_bf16
  bv_decide

/-- `∞ − ∞` (an infinite addend opposing an infinite product) ⇒ bf16 NaN `0x7FC0`. -/
theorem archBf16Fma_inf_minus_inf (a b c : BitVec 16)
    (hna : isNaN (arch_bf16_to_f32 a) = false) (hnb : isNaN (arch_bf16_to_f32 b) = false)
    (hpi : isInf (arch_bf16_to_f32 a) = true ∨ isInf (arch_bf16_to_f32 b) = true)
    (hci : isInf (arch_bf16_to_f32 c) = true)
    (hsgn : sgn (arch_bf16_to_f32 c)
              ≠ sgn (arch_bf16_to_f32 a) ^^^ sgn (arch_bf16_to_f32 b)) :
    archBf16Fma a b c = 0x7FC0#16 := by
  unfold archBf16Fma
  rw [fma_inf_minus_inf _ _ _ hna hnb hpi hci hsgn]
  unfold arch_f32_to_bf16
  bv_decide

/-- An infinite product (not `0·∞`, addend not the opposite ∞) ⇒ product-signed
    bf16 infinity. The sign bit is left symbolic (`bv_decide` after generalizing). -/
theorem archBf16Fma_inf_prod (a b c : BitVec 16)
    (hna : isNaN (arch_bf16_to_f32 a) = false) (hnb : isNaN (arch_bf16_to_f32 b) = false)
    (hnc : isNaN (arch_bf16_to_f32 c) = false)
    (hpi : isInf (arch_bf16_to_f32 a) = true ∨ isInf (arch_bf16_to_f32 b) = true)
    (hzti : ¬((isZero (arch_bf16_to_f32 a) = true ∧ isInf (arch_bf16_to_f32 b) = true)
            ∨ (isInf (arch_bf16_to_f32 a) = true ∧ isZero (arch_bf16_to_f32 b) = true)))
    (hcc : isInf (arch_bf16_to_f32 c) = false
            ∨ sgn (arch_bf16_to_f32 c) = sgn (arch_bf16_to_f32 a) ^^^ sgn (arch_bf16_to_f32 b)) :
    archBf16Fma a b c
      = (sgn (arch_bf16_to_f32 a) ^^^ sgn (arch_bf16_to_f32 b)) ++ (0xFF#8 ++ 0#7) := by
  unfold archBf16Fma
  rw [fma_inf_prod _ _ _ hna hnb hnc hpi hzti hcc]
  generalize (sgn (arch_bf16_to_f32 a) ^^^ sgn (arch_bf16_to_f32 b)) = s
  unfold arch_f32_to_bf16
  bv_decide

/-- A finite product plus an infinite (widened) addend ⇒ the addend's bf16 ∞. -/
theorem archBf16Fma_inf_c (a b c : BitVec 16)
    (hna : isNaN (arch_bf16_to_f32 a) = false) (hnb : isNaN (arch_bf16_to_f32 b) = false)
    (hnc : isNaN (arch_bf16_to_f32 c) = false)
    (hpa : isInf (arch_bf16_to_f32 a) = false) (hpb : isInf (arch_bf16_to_f32 b) = false)
    (hci : isInf (arch_bf16_to_f32 c) = true) :
    archBf16Fma a b c = sgn (arch_bf16_to_f32 c) ++ (0xFF#8 ++ 0#7) := by
  unfold archBf16Fma
  rw [fma_inf_c _ _ _ hna hnb hnc hpa hpb hci]
  generalize sgn (arch_bf16_to_f32 c) = s
  unfold arch_f32_to_bf16
  bv_decide

end ArchFp
