import ArchFpEquiv.TreeRoundingBound
#print axioms ArchFp.nearest_subnormal_error
#print axioms ArchFp.nearest_mixed_error
#print axioms ArchFp.add_mixed_error
#print axioms ArchFp.two_rounded_scales_error
#print axioms ArchFp.general_scaled_tree_bound
#print axioms ArchFp.magnitude_weight_sum
#print axioms ArchFp.roundNE_finite_or_overflow
#print axioms ArchFp.mul_finite_or_overflow
example : ArchFp.f32u * (1 / 2^126 : Rat) = ArchFp.halfEta := by
  norm_num [ArchFp.f32u, ArchFp.halfEta]
example : ((5 : Rat) + 1) * ArchFp.f32u < 1 := by norm_num [ArchFp.f32u]
