import ArchFpEquiv.Fma

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
    (hnc : arch_fma_mag (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c) ≠ 0#470) :
    archBf16Fma a b c
      = arch_f32_to_bf16
          (roundNE_f32
            (arch_fma_sign (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c) == 1#1)
            (arch_fma_mag (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c)).toNat
            (arch_fma_elo (arch_bf16_to_f32 a) (arch_bf16_to_f32 b) (arch_bf16_to_f32 c)).toInt) := by
  unfold archBf16Fma
  rw [arch_fma_f32_finite_correct _ _ _ ha hb hc hnc]

end ArchFp
