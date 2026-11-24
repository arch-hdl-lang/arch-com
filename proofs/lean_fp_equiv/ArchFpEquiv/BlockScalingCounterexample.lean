import ArchFpEquiv.ConcreteBlockChecks
namespace ArchFp
set_option maxRecDepth 100000
set_option maxHeartbeats 0
private theorem sparse_products : archProducts .e2m1 sparse32 sparse32 =
    0x3f800000#32 :: List.replicate 31 0#32 := by native_decide
private theorem value_zero : f32ToRat 0#32 = 0 := by
  change (f32SignedScaled 0#32 : Rat) / 2^149 = 0
  have h : f32SignedScaled 0#32 = 0 := by decide
  rw [h]; norm_num
private theorem value_one : f32ToRat 0x3f800000#32 = 1 := by
  change (f32SignedScaled 0x3f800000#32 : Rat) / 2^149 = 1
  have h : f32SignedScaled 0x3f800000#32 = 2^149 := by decide
  rw [h]; norm_num
private theorem sum_sparse : ratSum ((archProducts .e2m1 sparse32 sparse32).map f32ToRat) = 1 := by
  rw [sparse_products]
  simp [ratSum, value_zero, value_one]
private theorem abs_sum_sparse : ratSum ((archProducts .e2m1 sparse32 sparse32).map
    (fun p => ratAbs (f32ToRat p))) = 1 := by
  rw [sparse_products]
  simp [ratSum, value_zero, value_one, ratAbs]
private theorem scale0_value : f32ToRat (widenScale 0#8) = (1:Rat)/2^127 := by
  change f32R (widenScale 0#8) = _
  rw [scale_value _ (by decide)]; norm_num
private theorem sparse_exact : exactBlockDot .e2m1 0#8 0#8 sparse32 sparse32 = (1:Rat)/2^254 := by
  have hf : ∀ p ∈ sparse32.zip sparse32, FiniteElem .e2m1 p.1 ∧ FiniteElem .e2m1 p.2 := by
    have h : (sparse32.zip sparse32).all (fun p => decide (FiniteElem .e2m1 p.1 ∧ FiniteElem .e2m1 p.2)) = true := by native_decide
    simpa only [List.all_eq_true, decide_eq_true_eq] using h
  unfold exactBlockDot
  rw [← products_sum_exact .e2m1 sparse32 sparse32 hf, sum_sparse, scale0_value]
  norm_num
/-- Finite valid block inputs can violate the bound if exact-scaling conditions are omitted. -/
theorem underflow_refutes_unconditional_bound : ¬ (ratAbs (f32ToRat (archScaledDot .e2m1 0#8 0#8 sparse32 sparse32) -
    exactBlockDot .e2m1 0#8 0#8 sparse32 sparse32) ≤
    gammaPairwise 5 * ratSum ((archProducts .e2m1 sparse32 sparse32).map
      (fun p => ratAbs (f32ToRat p))) * f32ToRat (widenScale 0#8) * f32ToRat (widenScale 0#8)) := by
  rw [underflow_block_result, value_zero, sparse_exact, abs_sum_sparse, scale0_value]
  norm_num [gammaPairwise, f32u, ratAbs]
end ArchFp
