import ArchFpEquiv.FmaValue
import ArchFpEquiv.Round98
import ArchFpEquiv.Equiv

/-!
# Result specification for binary32 roundNearestTiesToEven

The input is the exact finite dyadic `(-1)^neg * sig * 2^e0`. Its zero
sign is supplied explicitly, as required when an operation constructs zero.
`RneF32Result` bundles sign, signed zero, nearest finite result, tie-to-even,
and signed infinity on overflow. This is a theorem about the existing Lean
rounder; comparison with SMT-LIB's `to_fp RNE` is a separate external check.
NaN/infinity operands and FMA's choice of exact-zero sign belong to the
operation's special-value dispatcher, not to rounding a finite dyadic.
-/

namespace ArchFp

/-- Dropped-bit count on the destination grid: binary32 has precision 24
and minimum quantum `2^-149`. Nonpositive counts represent exact shifts. -/
def rneF32Shift (sig : Nat) (e0 : Int) : Int :=
  if (Nat.log2 sig : Int) + e0 + 127 <= 0 then -149 - e0
  else (Nat.log2 sig : Int) - 23

/-- Exact midpoint on the applicable normal/subnormal rounding grid. -/
def RneF32Halfway (sig : Nat) (e0 : Int) : Prop :=
  0 < rneF32Shift sig e0 ∧
    sig % 2 ^ (rneF32Shift sig e0).toNat =
      2 ^ ((rneF32Shift sig e0).toNat - 1)

/-- Binary32 RNE result contract for an arbitrary finite dyadic. The
finite and overflow branches exhaust the rounded exponent range. Nearest
magnitude together with `sign` specifies the signed nearest result; the
zero sign is preserved even when a nonzero input rounds to zero. -/
structure RneF32Result (neg : Bool) (sig : Nat) (e0 : Int)
    (y : BitVec 32) : Prop where
  sign : BitVec.extractLsb 31 31 y = if neg then 1#1 else 0#1
  zero : sig = 0 -> y = BitVec.ofNat 32 (if neg then 2 ^ 31 else 0)
  finite : sig ≠ 0 -> biasedFinal sig e0 <= 254 ->
    IsFiniteF32 y ∧ IsNearestMag sig e0 y
  overflow : sig ≠ 0 -> 255 <= biasedFinal sig e0 ->
    y = BitVec.ofNat 32 ((if neg then 2 ^ 31 else 0) + 0x7F800000)
  tiesEven : sig ≠ 0 -> biasedFinal sig e0 <= 254 ->
    RneF32Halfway sig e0 -> (fracField y).toNat % 2 = 0

/-- All result branches and tie-breaking assembled without new axioms. -/
theorem roundNE_f32_rne_spec (neg : Bool) (sig : Nat) (e0 : Int) :
    RneF32Result neg sig e0 (roundNE_f32 neg sig e0) := by
  refine ⟨roundNE_extract_sign neg sig e0, ?_, ?_, ?_, ?_⟩
  · intro hz
    subst sig
    exact roundNE_zero neg e0
  · exact roundNE_nearest neg sig e0
  · intro hs ho
    have hb : biasedFinal sig e0 <= (Nat.log2 sig : Int) + e0 + 128 := by
      unfold biasedFinal
      split <;> omega
    exact roundNE_overflow neg sig e0 hs (by omega) ho
  · intro hs ho ht
    obtain ⟨hpos, htie⟩ := ht
    by_cases hb : (Nat.log2 sig : Int) + e0 + 127 <= 0
    · have hsh : rneF32Shift sig e0 = -149 - e0 := by
        simp only [rneF32Shift, if_pos hb]
      have hscale : (rneF32Shift sig e0).toNat = tScale e0 := by
        rw [hsh, tScale]
        congr 1
        omega
      rw [hsh] at hpos
      rw [hscale] at htie
      exact roundNE_tie_even_subnormal neg sig e0 hs hb (by omega) htie
    · have hsh : rneF32Shift sig e0 = (Nat.log2 sig : Int) - 23 := by
        simp only [rneF32Shift, if_neg hb]
      have hnat : (rneF32Shift sig e0).toNat = Nat.log2 sig - 23 := by
        rw [hsh]
        omega
      rw [hsh] at hpos
      rw [hnat] at htie
      exact roundNE_tie_even neg sig e0 hs (by omega) (by omega) ho htie

theorem arch_round48_rne_spec (s : BitVec 1) (sig : BitVec 48) (e0 : BitVec 16)
    (hlo : -298 <= e0.toInt) (hhi : e0.toInt <= 208) :
    RneF32Result (s == 1#1) sig.toNat e0.toInt (arch_round48 s sig e0) := by
  rw [arch_round48_correct s sig e0 hlo hhi]
  exact roundNE_f32_rne_spec _ _ _

theorem arch_round98_rne_spec (s : BitVec 1) (sig : BitVec 98) (e0 : BitVec 16)
    (hlo : -298 <= e0.toInt) (hhi : e0.toInt <= 208) :
    RneF32Result (s == 1#1) sig.toNat e0.toInt (arch_round98 s sig e0) := by
  rw [arch_round98_correct s sig e0 hlo hhi]
  exact roundNE_f32_rne_spec _ _ _

theorem arch_round470_rne_spec (s : BitVec 1) (sig : BitVec 470) (e0 : BitVec 16)
    (hlo : -298 <= e0.toInt) (hhi : e0.toInt <= 208) :
    RneF32Result (s == 1#1) sig.toNat e0.toInt (arch_round470 s sig e0) := by
  rw [arch_round470_correct s sig e0 hlo hhi]
  exact roundNE_f32_rne_spec _ _ _

end ArchFp
