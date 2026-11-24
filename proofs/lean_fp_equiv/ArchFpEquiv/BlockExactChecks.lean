import ArchFpEquiv.F32PairwiseSum

namespace ArchFp

-- Concrete witnesses keep the representability condition non-vacuous.
example : NoScaleOverflow 0x3f800000#32 0#8 := by
  refine ⟨by decide, by decide, Or.inr ⟨by decide, 0x00400000#32, by decide, by decide⟩⟩
example : NoScaleOverflow 0x00800000#32 126#8 := by
  refine ⟨by decide, by decide, Or.inr ⟨by decide, 0x00400000#32, by decide, by decide⟩⟩
example : NoScaleOverflow 1#32 128#8 := by
  refine ⟨by decide, by decide, Or.inr ⟨by decide, 2#32, by decide, by decide⟩⟩
example : NoScaleOverflow 0x80000000#32 254#8 := by
  exact ⟨by decide, by decide, Or.inl (by decide)⟩
example : ¬ NoScaleOverflow 0x3f800000#32 255#8 := by
  intro h; exact h.2.1 rfl

-- A finite result is not sufficient: half the smallest subnormal loses bits.
example : ¬ NoScaleOverflow 1#32 126#8 := by
  rintro ⟨_, _, hz | hr⟩
  · contradiction
  · obtain ⟨_, z, _, hz⟩ := hr
    change f32MagScaled z * 16777216 = 8388608 at hz
    omega
example : ¬ NoScaleOverflow 0x7f7fffff#32 128#8 := by
  rintro ⟨_, _, hz | hr⟩
  · contradiction
  · have hn : ¬ biasedFinal (mulSigNat 0x7f7fffff#32 (widenScale 128#8))
        (mulExpInt 0x7f7fffff#32 (widenScale 128#8)) ≤ 254 := by decide
    exact hn hr.1
example : ¬ FiniteElem .e4m3 0x7f#8 := by decide
example : ¬ FiniteElem .e5m2 0x7c#8 := by decide
example : FiniteElem .e4m3 0x7e#8 := by decide
example : FiniteElem .e5m2 1#8 := by decide

#print axioms products_exact
#print axioms scale_mul_exact
#print axioms scaled_dot_error_bound
end ArchFp
