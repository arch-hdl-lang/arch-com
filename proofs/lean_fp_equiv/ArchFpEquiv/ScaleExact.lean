import ArchFpEquiv.BlockWidenModel
import ArchFpEquiv.F32AddRel
import ArchFpEquiv.Equiv

namespace ArchFp

def mulSigNat (a b : BitVec 32) : Nat := (fpMant24 a).toNat * (fpMant24 b).toNat
def mulExpInt (a b : BitVec 32) : Int := (fpEunb a).toInt + (fpEunb b).toInt

/-- Input-only representability condition, independent of the implementation's
output. Includes the rounder's explicit finite-range condition and a finite
FP32 witness on the exact common integer grid. -/
def ExactMulRange (a b : BitVec 32) : Prop :=
  biasedFinal (mulSigNat a b) (mulExpInt a b) ≤ 254 ∧
  ∃ z : BitVec 32, IsFiniteF32 z ∧
    f32MagScaled z * 2 ^ tScale (mulExpInt a b) =
      mulSigNat a b * 2 ^ uScale (mulExpInt a b)

private theorem mulSig_bits (a b : BitVec 32) :
    ((BitVec.setWidth 48 (arch_decode_mant a) *
      BitVec.setWidth 48 (arch_decode_mant b)).toNat) = mulSigNat a b := by
  have ha := (fpMant24 a).isLt
  have hb := (fpMant24 b).isLt
  have hma : arch_decode_mant a = fpMant24 a := rfl
  have hmb : arch_decode_mant b = fpMant24 b := rfl
  rw [hma, hmb, BitVec.toNat_mul, BitVec.toNat_setWidth, BitVec.toNat_setWidth]
  norm_num at ha hb ⊢
  rw [Nat.mod_eq_of_lt (by nlinarith)]
  rfl

private theorem mulExp_bits (a b : BitVec 32) :
    (arch_decode_eunb a + arch_decode_eunb b).toInt = mulExpInt a b :=
  fpEunb_add_toInt a b

private theorem mant_nonzero (a : BitVec 32) (ha : finiteNonzero a = true) :
    fpMant24 a ≠ 0#24 := by
  unfold finiteNonzero isNaN isInf isZero expField fracField fpMant24 at *
  bv_decide

private theorem product_magnitude (a b : BitVec 32) :
    f32MagScaled a * f32MagScaled b =
      mulSigNat a b * 2 ^ (mulExpInt a b + 298).toNat := by
  obtain ⟨ha, _⟩ := fpEunb_range a
  obtain ⟨hb, _⟩ := fpEunb_range b
  rw [f32MagScaled_eq_decode, f32MagScaled_eq_decode]
  have he : (mulExpInt a b + 298).toNat =
      ((fpEunb a).toInt + 149).toNat + ((fpEunb b).toInt + 149).toNat := by
    unfold mulExpInt; omega
  rw [he, pow_add]
  unfold mulSigNat
  ring

private theorem exact_mul_mag (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hr : ExactMulRange a b) :
    f32MagScaled (arch_f32_mul a b) * 2^149 = f32MagScaled a * f32MagScaled b := by
  have hsig : mulSigNat a b ≠ 0 := by
    unfold mulSigNat
    apply Nat.mul_ne_zero
    · intro h; exact mant_nonzero a ha (BitVec.eq_of_toNat_eq (by simpa using h))
    · intro h; exact mant_nonzero b hb (BitVec.eq_of_toNat_eq (by simpa using h))
  have hm := arch_f32_mul_finite_correct a b ha hb
  rw [mulSig_bits, mulExp_bits] at hm
  obtain ⟨_, hn⟩ := roundNE_nearest ((mulSign a b) == 1#1) _ _ hsig hr.1
  rw [← hm] at hn
  obtain ⟨z, hz, he⟩ := hr.2
  have hnz := hn z hz
  rw [he] at hnz
  have heq : f32MagScaled (arch_f32_mul a b) * 2 ^ tScale (mulExpInt a b) =
      mulSigNat a b * 2 ^ uScale (mulExpInt a b) := by
    unfold scaledDist at hnz; omega
  have hlo : -298 ≤ mulExpInt a b := by
    obtain ⟨ha, _⟩ := fpEunb_range a
    obtain ⟨hb, _⟩ := fpEunb_range b
    unfold mulExpInt; omega
  have hexp : uScale (mulExpInt a b) + 149 =
      (mulExpInt a b + 298).toNat + tScale (mulExpInt a b) := by
    unfold uScale tScale; omega
  have h := congrArg (fun n : Nat => n * 2^149) heq
  rw [product_magnitude]
  apply Nat.eq_of_mul_eq_mul_right (Nat.two_pow_pos (tScale (mulExpInt a b)))
  calc
    f32MagScaled (arch_f32_mul a b) * 2^149 * 2^tScale (mulExpInt a b)
      = (f32MagScaled (arch_f32_mul a b) * 2^tScale (mulExpInt a b)) * 2^149 := by ring
    _ = (mulSigNat a b * 2^uScale (mulExpInt a b)) * 2^149 := h
    _ = mulSigNat a b * 2^(mulExpInt a b + 298).toNat * 2^tScale (mulExpInt a b) := by
      rw [Nat.mul_assoc, ← pow_add, hexp, pow_add]; ring

private theorem mul_sign_value (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) :
    (if BitVec.extractLsb 31 31 (arch_f32_mul a b) = 1#1 then (-1 : Int) else 1) =
      (if BitVec.extractLsb 31 31 a = 1#1 then (-1 : Int) else 1) *
      (if BitVec.extractLsb 31 31 b = 1#1 then (-1 : Int) else 1) := by
  have hs : sgn (arch_f32_mul a b) = mulSign a b := by
    rw [mul_finite_reduces a b ha hb]
    exact round48_sign _ _ _
  change (if sgn (arch_f32_mul a b) = 1#1 then _ else _) = _
  rw [hs]
  have hsa := (sgn a).isLt
  have hsb := (sgn b).isLt
  have ea : sgn a = 0#1 ∨ sgn a = 1#1 := by bv_decide
  have eb : sgn b = 0#1 ∨ sgn b = 1#1 := by bv_decide
  change (if sgn a ^^^ sgn b = 1#1 then _ else _) =
    (if sgn a = 1#1 then _ else _) * (if sgn b = 1#1 then _ else _)
  rcases ea with ea | ea <;> rcases eb with eb | eb <;> simp [ea, eb]

/-- Exact representable finite products have zero rational error. -/
theorem mul_exact_of_representable (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true)
    (hr : ExactMulRange a b) : f32R (arch_f32_mul a b) = f32R a * f32R b := by
  have hm := exact_mul_mag a b ha hb hr
  have hs := mul_sign_value a b ha hb
  have hi : f32SignedScaled (arch_f32_mul a b) * 2^149 =
      f32SignedScaled a * f32SignedScaled b := by
    unfold f32SignedScaled
    rw [hs]
    have hm' := congrArg (fun n : Nat => (n : Int)) hm
    push_cast at hm'
    have hh := congrArg (fun z : Int =>
      (if BitVec.extractLsb 31 31 a = 1#1 then (-1 : Int) else 1) *
      (if BitVec.extractLsb 31 31 b = 1#1 then (-1 : Int) else 1) * z) hm'
    convert hh using 1 <;> ring
  unfold f32R
  have hi' := congrArg (fun n : Int => (n : Rat)) hi
  push_cast at hi'
  field_simp
  nlinarith [hi']

end ArchFp
