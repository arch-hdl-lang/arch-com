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

def round48_struct (s : BitVec 1) (sig : BitVec 48) (e0 : BitVec 16) : BitVec 32 :=
  if sig == 0#48 then s ++ 0#31
  else
    let p := arch_msb_index48 sig
    let ev := p + e0
    let biased := ev + 127#16
    let isSub := BitVec.sle biased 0#16
    let k := if isSub then 65387#16 else ev - 23#16
    let sh := k - e0
    let shLe0 := BitVec.sle sh 0#16
    let zsig := BitVec.setWidth 50 sig
    let kept0 := if shLe0 then zsig <<< (BitVec.setWidth 50 (0#16 - sh)).toNat
                 else zsig >>> (BitVec.setWidth 50 sh).toNat
    let shm1 := BitVec.setWidth 50 (sh - 1#16)
    let guardRaw := BitVec.extractLsb 0 0 (zsig >>> shm1.toNat)
    let guard := if shLe0 then 0#1 else guardRaw
    let mask := ((1#50) <<< shm1.toNat) - 1#50
    let stickyRaw := BitVec.ofBool (zsig &&& mask != 0#50)
    let sticky := if shLe0 then 0#1 else stickyRaw
    let lsb := BitVec.extractLsb 0 0 kept0
    let roundup := guard &&& (sticky ||| lsb)
    let kept := kept0 + BitVec.setWidth 50 roundup
    let subRes := (s ++ 0#31) ||| (s ++ BitVec.extractLsb 30 0 kept)
    let carry := BitVec.ofBool (BitVec.extractLsb 24 24 kept == 1#1)
    let biasedN := if carry == 1#1 then biased + 1#16 else biased
    let overflow := BitVec.sle 255#16 biasedN
    let infRes := s ++ ((0xFF#8) ++ (0#23))
    let keptN := if carry == 1#1 then kept >>> (BitVec.setWidth 50 (1#16)).toNat else kept
    let normRes := s ++ (BitVec.extractLsb 7 0 biasedN ++ BitVec.extractLsb 22 0 keptN)
    let nonSub := if overflow then infRes else normRes
    if isSub then subRes else nonSub

theorem arch_eq_struct (s : BitVec 1) (sig : BitVec 48) (e0 : BitVec 16) :
    arch_round48 s sig e0 = round48_struct s sig e0 := by
  unfold arch_round48 round48_struct arch_msb_index48
  bv_decide (config := { timeout := 600 })

-- ── assembly helpers (sig ≠ 0) ───────────────────────────────────────────────

/-- The clz index, value-bridged: `p ≤ 47` and `p.toInt = log₂ sig` (sig ≠ 0). -/
theorem p_facts (sig : BitVec 48) (h : sig ≠ 0#48) :
    (arch_msb_index48 sig).toNat ≤ 47
    ∧ (arch_msb_index48 sig).toInt = (Nat.log2 sig.toNat : Int) := by
  obtain ⟨hlo, _hhi⟩ := msb_index_bound sig h
  have hsig : sig.toNat < 2 ^ 48 := sig.isLt
  have h1 : 2 ^ (arch_msb_index48 sig).toNat < 2 ^ 48 := by omega
  have hple : (arch_msb_index48 sig).toNat < 48 :=
    (Nat.pow_lt_pow_iff_right (by decide : 1 < 2)).mp h1
  have hpv : (arch_msb_index48 sig).toInt = ((arch_msb_index48 sig).toNat : Int) := by
    rw [BitVec.toInt_eq_toNat_bmod, Int.bmod_eq_emod]; split <;> omega
  exact ⟨by omega, by rw [hpv, msb_index_eq_log2 sig h]⟩

/-- Normal-field packing: `sign ++ exp8 ++ mant23` equals the `ofNat` encoding. -/
theorem combine (a b c : Nat) (ha : a < 2) (hb : b < 256) (hc : c < 2 ^ 23) :
    (BitVec.ofNat 1 a) ++ ((BitVec.ofNat 8 b) ++ (BitVec.ofNat 23 c))
    = BitVec.ofNat 32 (a * 2 ^ 31 + b * 2 ^ 23 + c) := by
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_append, BitVec.toNat_ofNat, Nat.shiftLeft_eq]
  rw [Nat.mod_eq_of_lt ha, Nat.mod_eq_of_lt hb, Nat.mod_eq_of_lt hc,
      Nat.mul_comm b (2 ^ 23), ← Nat.two_pow_add_eq_or_of_lt hc,
      Nat.mul_comm a (2 ^ (8 + 23)),
      ← Nat.two_pow_add_eq_or_of_lt (show 2 ^ 23 * b + c < 2 ^ (8 + 23) by omega)]
  omega

/-- Exponent chain: arch's `ev = p+e0` and `biased = ev+127` (BitVec 16) match the
    value-level `Int` exponents, given the multiply-relevant `e0` bounds (so the
    16-bit arithmetic does not wrap). -/
theorem exp_facts (sig : BitVec 48) (e0 : BitVec 16) (h : sig ≠ 0#48)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (arch_msb_index48 sig + e0).toInt = (Nat.log2 sig.toNat : Int) + e0.toInt
    ∧ (arch_msb_index48 sig + e0 + 127#16).toInt
        = (Nat.log2 sig.toNat : Int) + e0.toInt + 127 := by
  obtain ⟨hp47, hpInt⟩ := p_facts sig h
  have hl47 : Nat.log2 sig.toNat ≤ 47 := by rw [← msb_index_eq_log2 sig h]; exact hp47
  have hpge : (0 : Int) ≤ (arch_msb_index48 sig).toInt := by rw [hpInt]; exact Int.natCast_nonneg _
  have hple : (arch_msb_index48 sig).toInt ≤ 47 := by rw [hpInt]; exact_mod_cast hl47
  have hev : (arch_msb_index48 sig + e0).toInt = (arch_msb_index48 sig).toInt + e0.toInt :=
    toInt_add_of_bounds _ _ (by omega) (by omega)
  have h127 : (127#16).toInt = 127 := rfl
  have hbiased : (arch_msb_index48 sig + e0 + 127#16).toInt
      = (arch_msb_index48 sig + e0).toInt + 127 := by
    have := toInt_add_of_bounds (arch_msb_index48 sig + e0) (127#16)
      (by rw [hev, h127]; omega) (by rw [hev, h127]; omega)
    rwa [h127] at this
  exact ⟨by rw [hev, hpInt], by rw [hbiased, hev, hpInt]⟩

/-- `isSub` (the `sle biased 0` test) corresponds to the value condition. -/
theorem isSub_iff (sig : BitVec 48) (e0 : BitVec 16) (h : sig ≠ 0#48)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (BitVec.sle (arch_msb_index48 sig + e0 + 127#16) 0#16 = true)
      ↔ ((Nat.log2 sig.toNat : Int) + e0.toInt + 127 ≤ 0) := by
  rw [BitVec.sle_iff_toInt_le, (exp_facts sig e0 h hlo hhi).2]; exact ⟨fun x => x, fun x => x⟩

/-- A 16-bit value widened to 50 bits keeps its `toNat`. -/
theorem sw50_toNat (sh : BitVec 16) : (BitVec.setWidth 50 sh).toNat = sh.toNat := by
  rw [BitVec.toNat_setWidth]; exact Nat.mod_eq_of_lt (by have := sh.isLt; omega)

/-- `(setWidth 50 (sh - 1)).toNat = sh.toNat - 1` for `sh ≥ 1` (no underflow). -/
theorem sw50_sub1 (sh : BitVec 16) (h : 1 ≤ sh.toNat) :
    (BitVec.setWidth 50 (sh - 1#16)).toNat = sh.toNat - 1 := by
  rw [sw50_toNat, BitVec.toNat_sub]; have := sh.isLt
  simp only [show (1#16).toNat = 1 from rfl]; omega

/-- arch's inlined roundup (with `setWidth 50`-converted shift amounts) equals the
    generic `roundupBit` at the matching Nat shift — so `round_step` applies. -/
theorem struct_roundup_eq (zsig : BitVec 50) (sh : BitVec 16) (h1 : 1 ≤ sh.toNat) :
    (BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 50 (sh - 1#16)).toNat))
      &&& ((BitVec.ofBool (zsig &&& ((1#50 <<< (BitVec.setWidth 50 (sh - 1#16)).toNat) - 1#50) != 0#50))
            ||| (BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 50 sh).toNat)))
    = roundupBit zsig sh.toNat := by
  rw [sw50_sub1 sh h1, sw50_toNat]; unfold roundupBit; bv_decide

/-- Central rounding result: arch's kept significand (right-shift path) equals
    the round-to-nearest-even quotient, for `1 ≤ sh ≤ 50`. -/
theorem kept_value (zsig : BitVec 50) (sh : BitVec 16) (h1 : 1 ≤ sh.toNat) (h2 : sh.toNat ≤ 50) :
    ((zsig >>> (BitVec.setWidth 50 sh).toNat)
      + BitVec.setWidth 50
          ((BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 50 (sh - 1#16)).toNat))
            &&& ((BitVec.ofBool (zsig &&& ((1#50 <<< (BitVec.setWidth 50 (sh - 1#16)).toNat) - 1#50) != 0#50))
                  ||| (BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 50 sh).toNat))))).toNat
    = rneQuot zsig.toNat sh.toNat := by
  rw [struct_roundup_eq zsig sh h1, sw50_toNat]
  exact round_step zsig sh.toNat h1 (by omega)

/-- Left-shift (exact, no rounding) kept value, under no 50-bit overflow. -/
theorem kept_left (zsig : BitVec 50) (amt : Nat) (hno : zsig.toNat * 2 ^ amt < 2 ^ 50) :
    (zsig <<< amt).toNat = zsig.toNat * 2 ^ amt := by
  rw [BitVec.toNat_shiftLeft, Nat.shiftLeft_eq, Nat.mod_eq_of_lt hno]

/-- A right shift by ≥ 50 zeroes a 50-bit value. -/
theorem kept_big_zero (zsig : BitVec 50) (sh : Nat) (h : 50 ≤ sh) :
    zsig.toNat / 2 ^ sh = 0 := by
  have h1 : zsig.toNat < 2 ^ 50 := zsig.isLt
  have h2 : (2:Nat) ^ 50 ≤ 2 ^ sh := Nat.pow_le_pow_right (by decide) h
  exact Nat.div_eq_of_lt (by omega)

end ArchFp
