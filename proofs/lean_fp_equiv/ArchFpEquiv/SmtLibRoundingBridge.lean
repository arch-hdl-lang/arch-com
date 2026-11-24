import ArchFpEquiv.IEEE754ValueBridge
import ArchFpEquiv.RneF32Spec

/-!
# Exact wider-format input used by the SMT-LIB rounding comparison

SMT-LIB `(fp s E F)` has magnitude `(2^(precision-1) + F) *
2^(E-bias-(precision-1))` on the normal path. The external checker uses
precision = significand width and exponent width = 12. These lemmas prove
that the leading-bit partition is exhaustive, normalization preserves all
bits, the exponent is strictly normal, and the decoded magnitude is the
original dyadic. They do not import SMT-LIB solver semantics into Lean.
-/

namespace ArchFp

/-- Shift the most-significant bit into the implicit-one position. -/
def smtWideMantissa (sig width p : Nat) : Nat := sig * 2 ^ (width - 1 - p)

/-- Every nonzero finite bit vector falls into a checked leading-bit case. -/
theorem smtRounding_partition (width : Nat) (sig : BitVec width) (hs : sig ≠ 0) :
    ∃ p : Nat, p < width ∧ 2 ^ p <= sig.toNat ∧ sig.toNat < 2 ^ (p + 1) := by
  have hn : sig.toNat ≠ 0 := by
    intro h
    apply hs
    exact BitVec.eq_of_toNat_eq (by simpa using h)
  have hlo := Nat.log2_self_le hn
  have hhi := Nat.lt_log2_self (n := sig.toNat)
  refine ⟨Nat.log2 sig.toNat, ?_, hlo, hhi⟩
  by_contra h
  have hp : width <= Nat.log2 sig.toNat := by omega
  have hpow : (2 : Nat) ^ width <= 2 ^ Nat.log2 sig.toNat :=
    Nat.pow_le_pow_right (by decide) hp
  have hbound := sig.isLt
  omega

/-- The widened mantissa is normal and fits the chosen precision exactly. -/
theorem smtWideMantissa_bounds (sig width p : Nat) (hp : p < width)
    (hlo : 2 ^ p <= sig) (hhi : sig < 2 ^ (p + 1)) :
    2 ^ (width - 1) <= smtWideMantissa sig width p ∧
      smtWideMantissa sig width p < 2 ^ width := by
  have ha : p + (width - 1 - p) = width - 1 := by omega
  have hb : p + 1 + (width - 1 - p) = width := by omega
  constructor
  · calc
      2 ^ (width - 1) = 2 ^ p * 2 ^ (width - 1 - p) := by rw [← Nat.pow_add, ha]
      _ <= smtWideMantissa sig width p := Nat.mul_le_mul_right _ hlo
  · calc
      smtWideMantissa sig width p < 2 ^ (p + 1) * 2 ^ (width - 1 - p) :=
        Nat.mul_lt_mul_of_pos_right hhi (Nat.pow_pos (by decide))
      _ = 2 ^ width := by rw [← Nat.pow_add, hb]

/-- A 12-bit exponent with bias 2047 never reaches zero or all ones in
any of the 48/98/470-bit rounder domains. -/
theorem smtWideExponent_bounds (width p : Nat) (e0 : Int)
    (hp : p < width) (hw : width <= 470)
    (hlo : -298 <= e0) (hhi : e0 <= 208) :
    0 < (p : Int) + e0 + 2047 ∧ (p : Int) + e0 + 2047 < 4095 := by
  omega

/-- Under normal decoding, the wider FP constructor denotes exactly
`sig * 2^e0`, without rounding, subnormal loss, or overflow. -/
theorem smtWideDecode_exact (sig width p : Nat) (e0 : Int) (hp : p < width) :
    (smtWideMantissa sig width p : Rat) *
        (2 : Rat) ^ ((p : Int) + e0 - (width - 1 : Nat)) =
      (sig : Rat) * (2 : Rat) ^ e0 := by
  have hbalance : ((width - 1 - p : Nat) : Int) +
      ((p : Int) + e0 - (width - 1 : Nat)) = e0 := by omega
  simp only [smtWideMantissa, Nat.cast_mul, Nat.cast_pow, Nat.cast_ofNat]
  rw [← zpow_natCast, mul_assoc, ← zpow_add₀ (by norm_num : (2 : Rat) ≠ 0), hbalance]

end ArchFp
