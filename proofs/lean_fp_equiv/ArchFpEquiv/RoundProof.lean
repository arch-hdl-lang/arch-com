import ArchFpEquiv.Model
import ArchFpEquiv.Round
import ArchFpEquiv.RoundCore
import ArchFpEquiv.RoundBridge
import Std.Tactic.BVDecide

/-!
# Tier 2, part 5 — the value-level RNE spec and the assembly (in progress)

`roundNE_f32` is a concrete value-level round-to-nearest-even spec producing the
f32 bit pattern (no Mathlib — `Nat.log2`, `RoundCore.rneQuot`, integer encoding),
mirroring `arch_round48`'s output cases (sig=0 / subnormal / normal+carry /
overflow). The end goal is `arch_round48 s sig e0 = roundNE_f32 (s==1) sig.toNat
e0.toInt` (with input bounds that hold for the multiply use — out-of-range `e0`
would wrap arch's 16-bit exponent arithmetic). The `sig=0` case is discharged
here; the `sig≠0` assembly threads the proved bridges (RoundBridge / RoundCore /
Round) through the unfolded datapath and is being built up.
-/

namespace ArchFp

/-- Concrete value-level IEEE-754 RNE spec → f32 bit pattern. -/
def roundNE_f32 (neg : Bool) (sig : Nat) (e0 : Int) : BitVec 32 :=
  let sgn : Nat := if neg then 2 ^ 31 else 0
  if sig = 0 then BitVec.ofNat 32 sgn
  else
    let p : Int := (Nat.log2 sig : Int)
    let ev : Int := p + e0
    let biased : Int := ev + 127
    let k : Int := if biased ≤ 0 then -149 else ev - 23
    let sh : Int := k - e0
    let kept : Nat := if sh ≤ 0 then sig * 2 ^ (-sh).toNat else rneQuot sig sh.toNat
    if biased ≤ 0 then BitVec.ofNat 32 (sgn + kept % 2 ^ 31)
    else
      let carry : Bool := 2 ^ 24 ≤ kept
      let biased_n : Int := if carry then biased + 1 else biased
      let kept_n : Nat := if carry then kept / 2 else kept
      if 255 ≤ biased_n then BitVec.ofNat 32 (sgn + 0x7F800000)
      else BitVec.ofNat 32 (sgn + (biased_n.toNat % 256) * 2 ^ 23 + kept_n % 2 ^ 23)

/-- `roundNE_f32` on a zero significand is signed zero. -/
theorem roundNE_zero (neg : Bool) (e0 : Int) :
    roundNE_f32 neg 0 e0 = BitVec.ofNat 32 (if neg then 2 ^ 31 else 0) := by
  unfold roundNE_f32; simp

/-- **Assembly, sig=0 case.** `arch_round48` on a zero significand equals the spec. -/
theorem round48_correct_zero (s : BitVec 1) (e0 : BitVec 16) :
    arch_round48 s 0#48 e0 = roundNE_f32 (s == 1#1) (0#48).toNat e0.toInt := by
  rw [show (0#48).toNat = 0 from rfl, roundNE_zero, round48_zero, apply_ite (BitVec.ofNat 32)]
  bv_decide

end ArchFp
