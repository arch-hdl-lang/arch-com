import ArchFpEquiv.ConcreteBlock

namespace ArchFp
set_option maxRecDepth 100000
set_option maxHeartbeats 0

/-- Varied finite storage codes; exercises each format's actual decoder. -/
def concreteA (n : Nat) : List (BitVec 8) :=
  (List.range n).map (fun i => BitVec.ofNat 8 ((i * 7) % 120))
def concreteB (n : Nat) : List (BitVec 8) :=
  (List.range n).map (fun i => BitVec.ofNat 8 ((i * 11 + 1) % 120))

-- Complete, nonempty blocks, including odd pass-through levels and two
-- non-unit scales whose product is one. Each check discharges actual conditions.
theorem concrete_blocks_accepted : ∀ fmt : BlockElem,
    checkBlock fmt 130#8 124#8 (concreteA 32) (concreteB 32) = true ∧
    checkBlock fmt 127#8 127#8 (concreteA 31) (concreteB 31) = true ∧
    checkBlock fmt 127#8 127#8 (concreteA 1) (concreteB 1) = true := by
  native_decide

theorem concrete32_error_bound (fmt : BlockElem) :
    ratAbs (f32ToRat (archScaledDot fmt 130#8 124#8 (concreteA 32) (concreteB 32)) -
      exactBlockDot fmt 130#8 124#8 (concreteA 32) (concreteB 32)) ≤
    gammaPairwise 5 * ratSum ((archProducts fmt (concreteA 32) (concreteB 32)).map
      (fun p => ratAbs (f32ToRat p))) * f32ToRat (widenScale 130#8) *
        f32ToRat (widenScale 124#8) :=
  checked_block_error_bound fmt _ _ _ _ (concrete_blocks_accepted fmt).1

def sparse32 : List (BitVec 8) := 2#8 :: List.replicate 31 0#8

-- Valid finite elements/scales alone do NOT imply safe scaling.
theorem underflow_block_rejected : checkBlock .e2m1 0#8 0#8 sparse32 sparse32 = false := by
  native_decide
theorem underflow_block_result : archScaledDot .e2m1 0#8 0#8 sparse32 sparse32 = 0#32 := by
  native_decide

theorem overflow_block_rejected : checkBlock .e2m1 254#8 254#8 sparse32 sparse32 = false := by
  native_decide
theorem overflow_block_result : archScaledDot .e2m1 254#8 254#8 sparse32 sparse32 = 0x7f800000#32 := by
  native_decide

-- Exact cancellation, signed zero, shape checks, finite-code checks.
example : checkBlock .e2m1 127#8 127#8 [2#8, 10#8, 0#8] [2#8, 2#8, 0#8] = true := by native_decide
example : checkBlock .e2m1 127#8 127#8 [] [] = false := by decide
example : checkBlock .e2m1 127#8 127#8 [2#8] [2#8, 2#8] = false := by native_decide
example : checkBlock .e2m1 127#8 127#8 (List.replicate 33 2#8) (List.replicate 33 2#8) = false := by native_decide
example : checkBlock .e4m3 127#8 127#8 [0x7f#8] [0x38#8] = false := by native_decide
example : checkBlock .e5m2 127#8 127#8 [0x7c#8] [0x3c#8] = false := by native_decide
example : checkBlock .e2m1 255#8 127#8 [2#8] [2#8] = false := by native_decide
example : checkScale 0x00800000#32 126#8 = true := by native_decide
example : checkScale 1#32 126#8 = false := by native_decide

#print axioms checked_block_error_bound
#print axioms concrete32_error_bound
end ArchFp
