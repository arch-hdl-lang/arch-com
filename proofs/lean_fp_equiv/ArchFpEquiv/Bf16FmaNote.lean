import ArchFpEquiv.Model

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

end ArchFp
