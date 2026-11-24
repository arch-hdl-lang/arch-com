import ArchFpEquiv.BlockWidenModel
import ArchFpEquiv.F32AddRel
import ArchFpEquiv.Equiv
import ArchFpEquiv.ScaleExact

namespace ArchFp

instance (x : BitVec 32) : Decidable (IsFiniteF32 x) :=
  inferInstanceAs (Decidable (expField x ≠ 255#8))

inductive BlockElem where
  | e2m1 | e2m3 | e3m2 | e4m3 | e5m2
  deriving DecidableEq, Fintype

/-- Storage padding is ignored, as by the narrow element ports. -/
def widenElem (fmt : BlockElem) (x : BitVec 8) : BitVec 32 :=
  match fmt with
  | .e2m1 => arch_e2m1_to_f32 (x.setWidth 4)
  | .e2m3 => arch_e2m3_to_f32 (x.setWidth 6)
  | .e3m2 => arch_e3m2_to_f32 (x.setWidth 6)
  | .e4m3 => arch_e4m3_to_f32 x
  | .e5m2 => arch_e5m2_to_f32 x

def widenScale : BitVec 8 → BitVec 32 := arch_e8m0_to_f32

def FiniteElem (fmt : BlockElem) (x : BitVec 8) : Prop :=
  expField (widenElem fmt x) ≠ 255#8
instance (fmt : BlockElem) (x : BitVec 8) : Decidable (FiniteElem fmt x) :=
  inferInstanceAs (Decidable (_ ≠ _))

-- Exhaustive finite domains; integer values avoid any floating-point oracle.
set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
private theorem products_scaled_exact : ∀ fmt : BlockElem, ∀ x y : BitVec 8,
    FiniteElem fmt x → FiniteElem fmt y →
    IsFiniteF32 (arch_f32_mul (widenElem fmt x) (widenElem fmt y)) ∧
    f32SignedScaled (arch_f32_mul (widenElem fmt x) (widenElem fmt y)) * 2^149 =
      f32SignedScaled (widenElem fmt x) * f32SignedScaled (widenElem fmt y) := by
  native_decide

/-- Finite element products cannot overflow to infinity or produce NaN. -/
theorem products_finite (fmt : BlockElem) (x y : BitVec 8)
    (hx : FiniteElem fmt x) (hy : FiniteElem fmt y) :
    IsFiniteF32 (arch_f32_mul (widenElem fmt x) (widenElem fmt y)) :=
  (products_scaled_exact fmt x y hx hy).1

/-- Exact products, guarded against NaN and infinity encodings. -/
theorem products_exact (fmt : BlockElem) (x y : BitVec 8)
    (hx : FiniteElem fmt x) (hy : FiniteElem fmt y) :
    f32R (arch_f32_mul (widenElem fmt x) (widenElem fmt y)) =
      f32R (widenElem fmt x) * f32R (widenElem fmt y) := by
  have h := (products_scaled_exact fmt x y hx hy).2
  unfold f32R
  have hr := congrArg (fun z : Int => (z : Rat)) h
  push_cast at hr
  field_simp
  nlinarith [hr]


/-- Exact scaling requires finite operands and exact representability. The
witness/range test in `ExactMulRange` contains no implementation result. Zero
is handled separately. This includes exact subnormal results, not just normals. -/
def NoScaleOverflow (v : BitVec 32) (s : BitVec 8) : Prop :=
  IsFiniteF32 v ∧ s ≠ 255#8 ∧
    (isZero v = true ∨ ExactMulRange v (widenScale s))

private theorem scale_finite_nonzero (s : BitVec 8) (hs : s ≠ 255#8) :
    finiteNonzero (widenScale s) = true := by
  unfold finiteNonzero isNaN isInf isZero expField fracField widenScale arch_e8m0_to_f32
  bv_decide

private theorem zero_value (v : BitVec 32) (hz : isZero v = true) : f32R v = 0 := by
  have he : expField v = 0#8 := by unfold isZero expField at *; bv_decide
  have hf : fracField v = 0#23 := by unfold isZero fracField at *; bv_decide
  simp [f32R, f32SignedScaled, f32MagScaled, he, hf]

/-- E8M0's minimum code is 2^-127, not zero. -/
theorem scale_min_bits : widenScale 0#8 = 0x00400000#32 := by decide
/-- E8M0's unity code is 127. -/
theorem scale_one_bits : widenScale 127#8 = 0x3f800000#32 := by decide
/-- The exceptional code is a canonical NaN, excluded from rational claims. -/
theorem scale_nan_bits : widenScale 255#8 = 0x7fc00000#32 := by decide

/-- Scaling exactness follows from the proved multiplication/rounding theorem
and an explicit input-side representability condition. No modeling axiom. -/
theorem scale_mul_exact (v : BitVec 32) (s : BitVec 8) (h : NoScaleOverflow v s) :
    f32R (arch_f32_mul v (widenScale s)) = f32R v * f32R (widenScale s) := by
  obtain ⟨hv, hs, hr⟩ := h
  have hb := scale_finite_nonzero s hs
  by_cases hz : isZero v = true
  · have hn : isNaN (widenScale s) = false := by
      unfold finiteNonzero at hb
      cases h : isNaN (widenScale s) <;> simp_all
    have hi : isInf (widenScale s) = false := by
      unfold finiteNonzero at hb; cases h : isInf (widenScale s) <;> simp_all
    have ho : isZero (arch_f32_mul v (widenScale s)) = true := by
      rw [mul_zero_left v _ hz hn hi]
      unfold isZero zeroOf
      bv_decide
    rw [zero_value _ ho, zero_value _ hz, zero_mul]
  · have ha : finiteNonzero v = true := by
      unfold IsFiniteF32 at hv
      unfold finiteNonzero isNaN isInf
      simp [hv, hz]
    exact mul_exact_of_representable v _ ha hb (hr.resolve_left hz)


/-- Exact E8M0 value in units of the smallest FP32 subnormal. This includes
code zero (2^-127); only the NaN code is excluded. -/
private theorem scale_scaled_value : ∀ s : BitVec 8, s ≠ 255#8 →
    f32SignedScaled (widenScale s) = (2 : Int) ^ (s.toNat + 22) := by
  native_decide

theorem scale_value (s : BitVec 8) (hs : s ≠ 255#8) :
    f32R (widenScale s) = (2 : Rat) ^ (s.toNat + 22) / 2^149 := by
  unfold f32R
  rw [scale_scaled_value s hs]
  push_cast
  rfl

end ArchFp
