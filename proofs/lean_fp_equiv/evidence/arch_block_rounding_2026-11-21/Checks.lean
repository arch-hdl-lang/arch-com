import ArchFpEquiv.BlockExecutionBound
import ArchFpEquiv.ConcreteBlockChecks
namespace ArchFp
set_option exponentiation.threshold 600
set_option maxRecDepth 100000

-- The previous underflow counterexample now satisfies the general bound,
-- despite rejection by the old exact-scaling checker.
theorem underflow_general_bound :
    |f32R (archScaledDot .e2m1 0#8 0#8 sparse32 sparse32) -
      exactBlockDot .e2m1 0#8 0#8 sparse32 sparse32| ≤
      mixedBlockAllowance 5 (archProducts .e2m1 sparse32 sparse32)
        (f32R (widenScale 0#8)) (f32R (widenScale 0#8)) := by
  have hf : ∀ p ∈ sparse32.zip sparse32, FiniteElem .e2m1 p.1 ∧ FiniteElem .e2m1 p.2 := by
    have h : (sparse32.zip sparse32).all (fun p => decide (FiniteElem .e2m1 p.1 ∧ FiniteElem .e2m1 p.2)) = true := by native_decide
    simpa only [List.all_eq_true, decide_eq_true_eq] using h
  exact arch_block_mixed_bound .e2m1 _ _ _ _ rfl (by simp [sparse32])
    (by simp [sparse32]) hf (by decide) (by decide)
    (by rw [underflow_block_result]; decide)

-- Explicit witness for the nonfinite branch at the second scale.
example : BlockOverflow 5 (archProducts .e2m1 sparse32 sparse32)
    (widenScale 254#8) (widenScale 254#8) := by
  apply Or.inr; apply Or.inr
  constructor
  · native_decide
  constructor
  · exact scale_finite _ (by decide)
  · change isInf (archScaledDot .e2m1 254#8 254#8 sparse32 sparse32) = true
    rw [overflow_block_result]; decide

-- First-scale overflow cannot be repaired by a subsequent tiny scale.
-- The exact value here is small; the actual staged result is infinity.
example : archScaledDot .e2m1 254#8 0#8 [4#8] [2#8] = 0x7f800000#32 := by native_decide

-- Signed-zero addition, exact cancellation, and multiplication underflow.
example : |f32R (arch_f32_add 0x80000000#32 0x80000000#32) -
    (f32R 0x80000000#32 + f32R 0x80000000#32)| ≤
    f32u * |f32R 0x80000000#32 + f32R 0x80000000#32| + halfEta :=
  arch_add_mixed _ _ (by decide) (by decide) (by native_decide)
example : arch_f32_add 0x3f800000#32 0xbf800000#32 = 0#32 := by native_decide
example : |f32R (arch_f32_mul 1#32 0x3f000000#32) - f32R 1#32*f32R 0x3f000000#32| ≤
    f32u*|f32R 1#32*f32R 0x3f000000#32| + halfEta :=
  arch_mul_mixed _ _ (by decide) (by decide) (by native_decide)

-- Opposite-sign overflow at the first level can produce NaN at the next.
-- This is a generic FP32 tree test, not a claim that low-precision leaves
-- of a <=32-element block can reach this magnitude.
def overflowTree : List (BitVec 32) :=
  [0x7f7fffff#32, 0x7f7fffff#32, 0xff7fffff#32, 0xff7fffff#32]
example : isNaN (archPairwiseSum overflowTree) = true := by native_decide
example : TreeOverflow 2 overflowTree := by
  apply Or.inl; apply Or.inl
  exact ⟨by decide, by decide, by native_decide⟩

#print axioms arch_add_mixed
#print axioms arch_mul_mixed
#print axioms arch_block_mixed_bound
#print axioms arch_block_result_cases
end ArchFp
