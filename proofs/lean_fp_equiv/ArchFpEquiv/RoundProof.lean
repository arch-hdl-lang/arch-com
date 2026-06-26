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

/-- `sig=0` case for the named-stage struct. -/
theorem struct_zero (s : BitVec 1) (e0 : BitVec 16) :
    round48_struct s 0#48 e0 = roundNE_f32 (s == 1#1) (0#48).toNat e0.toInt := by
  rw [show (0#48).toNat = 0 from rfl, roundNE_zero]
  unfold round48_struct
  simp only [show ((0#48 : BitVec 48) == 0#48) = true from rfl, if_true]
  rw [apply_ite (BitVec.ofNat 32)]; bv_decide

/-- Shift value, normal branch (`k = ev-23`): `sh = log2 - 23`. -/
theorem sh_normal (sig : BitVec 48) (e0 : BitVec 16) (h : sig ≠ 0#48)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (arch_msb_index48 sig + e0 - 23#16 - e0).toInt = (Nat.log2 sig.toNat : Int) - 23 := by
  obtain ⟨hp47, hpInt⟩ := p_facts sig h
  have hl47 : Nat.log2 sig.toNat ≤ 47 := by rw [← msb_index_eq_log2 sig h]; exact hp47
  have hev := (exp_facts sig e0 h hlo hhi).1
  have h23 : (23#16).toInt = 23 := rfl
  have hk : (arch_msb_index48 sig + e0 - 23#16).toInt = (Nat.log2 sig.toNat : Int) + e0.toInt - 23 := by
    rw [toInt_sub_of_bounds _ _ (by rw [hev, h23]; omega) (by rw [hev, h23]; omega), hev, h23]
  rw [toInt_sub_of_bounds _ _ (by rw [hk]; omega) (by rw [hk]; omega), hk]; omega

/-- Shift value, subnormal branch (`k = -149`): `sh = -149 - e0`. -/
theorem sh_sub (e0 : BitVec 16) (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (65387#16 - e0).toInt = -149 - e0.toInt := by
  have hk : (65387#16).toInt = -149 := rfl
  rw [toInt_sub_of_bounds _ _ (by rw [hk]; omega) (by rw [hk]; omega), hk]

/-- Subnormal-field packing identity. -/
theorem combine_sub (s : BitVec 1) (kept : BitVec 50) :
    (s ++ (0#31)) ||| (s ++ BitVec.extractLsb 30 0 kept)
    = BitVec.ofNat 32 (s.toNat * 2 ^ 31 + kept.toNat % 2 ^ 31) := by
  have hor : (s ++ (0#31)) ||| (s ++ BitVec.extractLsb 30 0 kept)
      = s ++ BitVec.extractLsb 30 0 kept := by bv_decide
  have hext : (BitVec.extractLsb 30 0 kept).toNat = kept.toNat % 2 ^ 31 := by
    simp [BitVec.extractLsb, BitVec.extractLsb'_toNat]
  rw [hor]
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_append, BitVec.toNat_ofNat, Nat.shiftLeft_eq, hext]
  have hk : kept.toNat % 2 ^ 31 < 2 ^ 31 := Nat.mod_lt _ (by decide)
  rw [Nat.mul_comm s.toNat (2 ^ 31), ← Nat.two_pow_add_eq_or_of_lt hk]
  omega

/-- The 50-bit widen of the significand keeps its `toNat`. -/
theorem zsig_toNat (sig : BitVec 48) : (BitVec.setWidth 50 sig).toNat = sig.toNat := by
  rw [BitVec.toNat_setWidth]; exact Nat.mod_eq_of_lt (by have := sig.isLt; omega)

/-- Negated-shift amount bridge for the left-shift (exact) path. -/
theorem negsh_toNat (sh : BitVec 16) (h0 : sh.toInt ≤ 0) (hb : -200 ≤ sh.toInt) :
    (0#16 - sh).toNat = (-sh.toInt).toNat := by
  have h1 : (0#16 - sh).toInt = - sh.toInt := by
    rw [toInt_sub_of_bounds] <;> simp only [show (0#16).toInt = 0 from rfl] <;> omega
  have h2 := BitVec.toInt_eq_toNat_bmod (0#16 - sh)
  have h3 := (0#16 - sh).isLt
  rw [h1, Int.bmod_eq_emod] at h2
  split at h2 <;> omega

/-- `toInt ≥ 0` (16-bit) ⟹ `toNat = toInt.toNat`. -/
theorem toNat_of_toInt_nonneg (x : BitVec 16) (h : 0 ≤ x.toInt) : x.toNat = x.toInt.toNat := by
  have h2 := BitVec.toInt_eq_toNat_bmod x
  have h3 := x.isLt
  rw [Int.bmod_eq_emod] at h2; split at h2 <;> omega

/-- `guardStickyUp` is false for a deep shift (`sh ≥ 51`) of a 50-bit value. -/
theorem gsu_big_false (n sh : Nat) (hn : n < 2 ^ 50) (h : 51 ≤ sh) :
    guardStickyUp n sh = false := by
  unfold guardStickyUp
  have h1 : n % 2 ^ sh = n := Nat.mod_eq_of_lt (by
    have : (2:Nat)^50 ≤ 2^sh := Nat.pow_le_pow_right (by decide) (by omega); omega)
  have h2 : n < 2 ^ (sh - 1) := by
    have : (2:Nat)^50 ≤ 2^(sh-1) := Nat.pow_le_pow_right (by decide) (by omega); omega
  simp only [h1, Nat.not_le.mpr h2, decide_false, Bool.false_and]

/-- Deep underflow: `rneQuot n sh = 0` for `n < 2^50` and `sh ≥ 51`. -/
theorem rneQuot_big_zero (n sh : Nat) (hn : n < 2 ^ 50) (h : 51 ≤ sh) :
    rneQuot n sh = 0 := by
  unfold rneQuot
  have hge : (2:Nat) ^ 50 ≤ 2 ^ sh := Nat.pow_le_pow_right (by decide) (by omega)
  have hge1 : (2:Nat) ^ 50 ≤ 2 ^ (sh - 1) := Nat.pow_le_pow_right (by decide) (by omega)
  rw [Nat.div_eq_of_lt (by omega), Nat.mod_eq_of_lt (by omega), if_neg (by omega)]

/-- **Unified kept value.** arch's kept significand (the whole `if shLe0 … + roundup`
    expression as it appears in `round48_struct`) equals the value-level `kept`:
    `sig·2^(-sh)` on the exact left-shift path (`sh ≤ 0`), `rneQuot sig sh` on the
    rounding right-shift path (`sh > 0`), covering the deep-underflow tail
    (`sh > 50` ⟹ 0) too. The single lemma the kept-significand cases reduce to. -/
theorem kept_unified (sig : BitVec 50) (sh : BitVec 16)
    (hbnd : -200 ≤ sh.toInt) (hbnd2 : sh.toInt ≤ 200)
    (hno : sh.toInt ≤ 0 → sig.toNat * 2 ^ (-sh.toInt).toNat < 2 ^ 50) :
    ((if BitVec.sle sh 0#16 then sig <<< (BitVec.setWidth 50 (0#16 - sh)).toNat
      else sig >>> (BitVec.setWidth 50 sh).toNat)
     + BitVec.setWidth 50
        ((if BitVec.sle sh 0#16 then 0#1
          else BitVec.extractLsb 0 0 (sig >>> (BitVec.setWidth 50 (sh - 1#16)).toNat))
          &&& ((if BitVec.sle sh 0#16 then 0#1
                else BitVec.ofBool (sig &&& ((1#50 <<< (BitVec.setWidth 50 (sh - 1#16)).toNat) - 1#50) != 0#50))
                ||| BitVec.extractLsb 0 0
                      (if BitVec.sle sh 0#16 then sig <<< (BitVec.setWidth 50 (0#16 - sh)).toNat
                       else sig >>> (BitVec.setWidth 50 sh).toNat)))).toNat
    = (if sh.toInt ≤ 0 then sig.toNat * 2 ^ (-sh.toInt).toNat else rneQuot sig.toNat sh.toNat) := by
  by_cases hle : BitVec.sle sh 0#16
  · have hsi : sh.toInt ≤ 0 := by rw [BitVec.sle_iff_toInt_le] at hle; simpa using hle
    simp only [hle, if_true, if_pos hsi]
    have hadd : ∀ x : BitVec 50, x + BitVec.setWidth 50 ((0#1 : BitVec 1) &&& (0#1 ||| BitVec.extractLsb 0 0 x)) = x := by
      intro x; bv_decide
    rw [hadd, sw50_toNat, negsh_toNat sh hsi (by omega)]
    exact kept_left sig _ (hno hsi)
  · rw [Bool.not_eq_true] at hle
    have hsi : 0 < sh.toInt := by
      have h := hle
      rw [BitVec.sle_eq_decide, decide_eq_false_iff_not] at h
      have hz : (0#16).toInt = 0 := by decide
      rw [hz] at h
      omega
    have hn1 : 1 ≤ sh.toNat := by rw [toNat_of_toInt_nonneg sh (by omega)]; omega
    have hnle : ¬ sh.toInt ≤ 0 := by omega
    rw [hle]
    simp only [Bool.false_eq_true, if_false, if_neg hnle]
    by_cases hsmall : sh.toNat ≤ 50
    · exact kept_value sig sh hn1 hsmall
    · have hsig50 : sig.toNat < 2 ^ 50 := sig.isLt
      have hk0 : sig >>> (BitVec.setWidth 50 sh).toNat = 0#50 := by
        apply BitVec.eq_of_toNat_eq
        rw [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow, sw50_toNat]
        simp [kept_big_zero sig sh.toNat (by omega)]
      have hg0 : sig >>> (BitVec.setWidth 50 (sh - 1#16)).toNat = 0#50 := by
        apply BitVec.eq_of_toNat_eq
        rw [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow, sw50_sub1 sh hn1]
        have hlt : sig.toNat < 2 ^ (sh.toNat - 1) := by
          have : (2:Nat)^50 ≤ 2^(sh.toNat-1) := Nat.pow_le_pow_right (by decide) (by omega); omega
        simp [Nat.div_eq_of_lt hlt]
      rw [hk0, hg0, rneQuot_big_zero sig.toNat sh.toNat hsig50 (by omega)]
      bv_decide

-- ── normal / overflow packing helpers ────────────────────────────────────────

/-- Append packing with raw BitVec pieces: `s ++ exp8 ++ mant23` as `ofNat`. -/
theorem combine' (x : BitVec 1) (y : BitVec 8) (z : BitVec 23) :
    x ++ (y ++ z) = BitVec.ofNat 32 (x.toNat * 2 ^ 31 + y.toNat * 2 ^ 23 + z.toNat) := by
  have hx : x.toNat < 2 := x.isLt
  have hy : y.toNat < 256 := y.isLt
  have hz : z.toNat < 2 ^ 23 := z.isLt
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_append, BitVec.toNat_ofNat, Nat.shiftLeft_eq]
  rw [Nat.mul_comm y.toNat (2 ^ 23), ← Nat.two_pow_add_eq_or_of_lt hz,
      Nat.mul_comm x.toNat (2 ^ 31), ← Nat.two_pow_add_eq_or_of_lt
        (show 2 ^ 23 * y.toNat + z.toNat < 2 ^ 31 by omega),
      show (1 + (8 + 23)) = 32 from rfl,
      Nat.mod_eq_of_lt (show 2 ^ 31 * x.toNat + 2 ^ 23 * y.toNat + z.toNat < 2 ^ 32 by omega)]
  omega

/-- The low-8 exponent field as a `Nat` mod. -/
theorem extract8_toNat (b : BitVec 16) : (BitVec.extractLsb 7 0 b).toNat = b.toNat % 256 := by
  rw [BitVec.extractLsb_toNat, Nat.shiftRight_zero]

/-- The low-23 mantissa field as a `Nat` mod. -/
theorem extract23_toNat (k : BitVec 50) : (BitVec.extractLsb 22 0 k).toNat = k.toNat % 2 ^ 23 := by
  rw [BitVec.extractLsb_toNat, Nat.shiftRight_zero]

/-- The carry bit `kept[24]` decides `2^24 ≤ kept` (given `kept ≤ 2^24`). -/
theorem carry_bit (K : BitVec 50) (h : K.toNat ≤ 2 ^ 24) :
    (BitVec.extractLsb 24 24 K == 1#1) = decide (2 ^ 24 ≤ K.toNat) := by
  have hpos : 0 < 2 ^ 24 := Nat.two_pow_pos _
  have hval : (BitVec.extractLsb 24 24 K).toNat = K.toNat / 2 ^ 24 % 2 := by
    rw [BitVec.extractLsb_toNat, Nat.shiftRight_eq_div_pow]
  have hq2 : K.toNat / 2 ^ 24 < 2 := (Nat.div_lt_iff_lt_mul hpos).mpr (by omega)
  have hiff : (BitVec.extractLsb 24 24 K == 1#1) = true ↔ 2 ^ 24 ≤ K.toNat := by
    rw [beq_iff_eq, ← BitVec.toNat_inj, hval, show (1#1 : BitVec 1).toNat = 1 from rfl]
    constructor
    · intro hh; exact (div_eq_one_of_lt_two_mul K.toNat (2 ^ 24) hpos (by omega)).mp (by omega)
    · intro hh
      have : K.toNat / 2 ^ 24 = 1 := (div_eq_one_of_lt_two_mul K.toNat (2 ^ 24) hpos (by omega)).mpr hh
      omega
  cases hb : (BitVec.extractLsb 24 24 K == 1#1)
  · have hnp : ¬ 2 ^ 24 ≤ K.toNat := by
      intro hp; rw [hiff.mpr hp] at hb; exact absurd hb (by decide)
    rw [decide_eq_false hnp]
  · rw [decide_eq_true (hiff.mp hb)]

/-- `BitVec.ofBool b == 1#1` is `b`. -/
theorem ofBool_beq_one (b : Bool) : (BitVec.ofBool b == 1#1) = b := by cases b <;> decide

/-- `rneQuot` of a normalized 24-bit-window dividend lands in `[2^23, 2^24]`. -/
theorem rneQuot_bounds (n sh : Nat) (h1 : 1 ≤ sh)
    (hlo : 2 ^ (sh + 23) ≤ n) (hhi : n < 2 ^ (sh + 24)) :
    2 ^ 23 ≤ rneQuot n sh ∧ rneQuot n sh ≤ 2 ^ 24 := by
  have hpos : 0 < 2 ^ sh := Nat.two_pow_pos _
  have hqlo : 2 ^ 23 ≤ n / 2 ^ sh := by
    rw [Nat.le_div_iff_mul_le hpos, ← Nat.pow_add]
    have : 23 + sh = sh + 23 := by omega
    rw [this]; exact hlo
  have hqhi : n / 2 ^ sh < 2 ^ 24 := by
    rw [Nat.div_lt_iff_lt_mul hpos, ← Nat.pow_add]
    have : 24 + sh = sh + 24 := by omega
    rw [this]; exact hhi
  unfold rneQuot
  split <;> constructor <;> omega

/-- The normalized kept significand lies in `[2^23, 2^24]` (sign of carry depends
    only on whether it reaches `2^24`). -/
theorem kept_range (sig L : Nat) (hlo : 2 ^ L ≤ sig) (hhi : sig < 2 ^ (L + 1))
    (SH : BitVec 16) (hSH : SH.toInt = (L : Int) - 23) :
    2 ^ 23 ≤ (if SH.toInt ≤ 0 then sig * 2 ^ (-SH.toInt).toNat else rneQuot sig SH.toNat)
    ∧ (if SH.toInt ≤ 0 then sig * 2 ^ (-SH.toInt).toNat else rneQuot sig SH.toNat) ≤ 2 ^ 24 := by
  by_cases hc : SH.toInt ≤ 0
  · rw [if_pos hc]
    have hsh : (-SH.toInt).toNat = 23 - L := by rw [hSH]; omega
    rw [hsh]
    constructor
    · calc 2 ^ 23 = 2 ^ L * 2 ^ (23 - L) := by
            rw [← Nat.pow_add]; congr 1; rw [hSH] at hc; omega
        _ ≤ sig * 2 ^ (23 - L) := Nat.mul_le_mul_right _ hlo
    · apply Nat.le_of_lt
      calc sig * 2 ^ (23 - L) < 2 ^ (L + 1) * 2 ^ (23 - L) :=
            (Nat.mul_lt_mul_right (Nat.two_pow_pos _)).mpr hhi
        _ = 2 ^ 24 := by rw [← Nat.pow_add]; congr 1; rw [hSH] at hc; omega
  · rw [if_neg hc]
    have hshn : SH.toNat = L - 23 := by
      rw [toNat_of_toInt_nonneg SH (by rw [hSH] at hc ⊢; omega), hSH]; omega
    rw [hshn]
    rw [hSH] at hc
    exact rneQuot_bounds sig (L - 23) (by omega)
      (by rw [show L - 23 + 23 = L by omega]; exact hlo)
      (by rw [show L - 23 + 24 = L + 1 by omega]; exact hhi)

/-- Clean-Nat form of the normalized kept range, keyed on `L ≤ 23` (left-shift)
    vs `L > 23` (rounding right-shift). -/
theorem kept_clean_range (sig L : Nat) (hlo : 2 ^ L ≤ sig) (hhi : sig < 2 ^ (L + 1)) :
    2 ^ 23 ≤ (if L ≤ 23 then sig * 2 ^ (23 - L) else rneQuot sig (L - 23))
    ∧ (if L ≤ 23 then sig * 2 ^ (23 - L) else rneQuot sig (L - 23)) ≤ 2 ^ 24 := by
  by_cases hc : L ≤ 23
  · rw [if_pos hc]
    constructor
    · calc 2 ^ 23 = 2 ^ L * 2 ^ (23 - L) := by rw [← Nat.pow_add]; congr 1; omega
        _ ≤ sig * 2 ^ (23 - L) := Nat.mul_le_mul_right _ hlo
    · apply Nat.le_of_lt
      calc sig * 2 ^ (23 - L) < 2 ^ (L + 1) * 2 ^ (23 - L) :=
            (Nat.mul_lt_mul_right (Nat.two_pow_pos _)).mpr hhi
        _ = 2 ^ 24 := by rw [← Nat.pow_add]; congr 1; omega
  · rw [if_neg hc]
    exact rneQuot_bounds sig (L - 23) (by omega)
      (by rw [show L - 23 + 23 = L by omega]; exact hlo)
      (by rw [show L - 23 + 24 = L + 1 by omega]; exact hhi)

/-- The sign field: `s.toNat · 2^31` equals the spec's `if s==1 then 2^31 else 0`. -/
theorem sgn_eq (s : BitVec 1) :
    s.toNat * 2 ^ 31 = (if (s == 1#1) = true then 2 ^ 31 else 0) := by
  by_cases h : s = 1#1
  · subst h; rfl
  · have hf : (s == 1#1) = false := beq_eq_false_iff_ne.mpr h
    have h0 : s.toNat = 0 := by
      have hb : s.toNat < 2 := s.isLt
      have h1 : s.toNat ≠ 1 := fun hc => h (BitVec.eq_of_toNat_eq (by rw [hc]; rfl))
      omega
    rw [hf, h0]; simp

theorem struct_eq_spec (s : BitVec 1) (sig : BitVec 48) (e0 : BitVec 16)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    round48_struct s sig e0 = roundNE_f32 (s == 1#1) sig.toNat e0.toInt := by
  by_cases hsig : sig = 0#48
  · subst hsig; exact struct_zero s e0
  · have hne : (sig == 0#48) = false := by simp only [beq_eq_false_iff_ne]; exact hsig
    have hsn : sig.toNat ≠ 0 := by
      intro h; exact hsig (BitVec.eq_of_toNat_eq (by simpa using h))
    obtain ⟨hp47, hpInt⟩ := p_facts sig hsig
    have hl47 : Nat.log2 sig.toNat ≤ 47 := by rw [← msb_index_eq_log2 sig hsig]; exact hp47
    have hlog_nn : (0:Int) ≤ (Nat.log2 sig.toNat : Int) := Int.natCast_nonneg _
    have hsig_hi : sig.toNat < 2 ^ (Nat.log2 sig.toNat + 1) := Nat.lt_log2_self
    unfold round48_struct roundNE_f32
    rw [if_neg hsn, hne]
    simp only [Bool.false_eq_true, if_false]
    by_cases hsub : BitVec.sle (arch_msb_index48 sig + e0 + 127#16) 0#16 = true
    · -- subnormal branch: biased ≤ 0
      have hb0 : (Nat.log2 sig.toNat : Int) + e0.toInt + 127 ≤ 0 :=
        (isSub_iff sig e0 hsig hlo hhi).mp hsub
      have he0hi : e0.toInt ≤ -127 := by omega
      simp only [hsub, if_true, if_pos hb0]
      have hsh : (65387#16 - e0).toInt = -149 - e0.toInt := sh_sub e0 hlo hhi
      have hbnd : (-200:Int) ≤ (65387#16 - e0).toInt := by rw [hsh]; omega
      have hbnd2 : (65387#16 - e0).toInt ≤ 200 := by rw [hsh]; omega
      have hno : (65387#16 - e0).toInt ≤ 0 →
          (BitVec.setWidth 50 sig).toNat * 2 ^ (-(65387#16 - e0).toInt).toNat < 2 ^ 50 := by
        intro hle
        rw [zsig_toNat, hsh] at *
        -- hle : -149 - e0.toInt ≤ 0  ⟹ e0 ≥ -149 ; with hb0 ⟹ log2 ≤ 22
        have hmle : (-(-149 - e0.toInt)).toNat ≤ 22 := by
          have : (-(-149 - e0.toInt)) ≤ 22 := by omega
          omega
        have hlog22 : Nat.log2 sig.toNat ≤ 22 := by
          have : (Nat.log2 sig.toNat : Int) ≤ 22 := by omega
          omega
        calc sig.toNat * 2 ^ (-(-149 - e0.toInt)).toNat
            < 2 ^ (Nat.log2 sig.toNat + 1) * 2 ^ (-(-149 - e0.toInt)).toNat :=
              (Nat.mul_lt_mul_right (Nat.two_pow_pos _)).mpr hsig_hi
          _ = 2 ^ (Nat.log2 sig.toNat + 1 + (-(-149 - e0.toInt)).toNat) :=
              (Nat.pow_add 2 _ _).symm
          _ ≤ 2 ^ 50 := Nat.pow_le_pow_right (by decide) (by omega)
      rw [combine_sub, kept_unified (BitVec.setWidth 50 sig) (65387#16 - e0) hbnd hbnd2 hno,
          zsig_toNat, hsh, sgn_eq]
      -- now match the kept if-expressions
      by_cases hc : -149 - e0.toInt ≤ 0
      · rw [if_pos hc, if_pos hc]
      · rw [if_neg hc, if_neg hc]
        have hpos : (0:Int) ≤ (65387#16 - e0).toInt := by rw [hsh]; omega
        rw [show (65387#16 - e0).toNat = (-149 - e0.toInt).toNat by
              rw [toNat_of_toInt_nonneg (65387#16 - e0) hpos, hsh]]
    · -- normal / overflow branch: biased > 0
      rw [Bool.not_eq_true] at hsub
      have hbpos : ¬ ((Nat.log2 sig.toNat : Int) + e0.toInt + 127 ≤ 0) := by
        intro hcon
        have : BitVec.sle (arch_msb_index48 sig + e0 + 127#16) 0#16 = true :=
          (isSub_iff sig e0 hsig hlo hhi).mpr hcon
        rw [hsub] at this; exact absurd this (by decide)
      simp only [hsub, Bool.false_eq_true, if_false, if_neg hbpos, ofBool_beq_one]
      have hSH : (arch_msb_index48 sig + e0 - 23#16 - e0).toInt = (Nat.log2 sig.toNat : Int) - 23 :=
        sh_normal sig e0 hsig hlo hhi
      have hsig_lo : 2 ^ Nat.log2 sig.toNat ≤ sig.toNat :=
        Nat.log2_self_le (by omega)
      have hbndSH : (-200:Int) ≤ (arch_msb_index48 sig + e0 - 23#16 - e0).toInt := by rw [hSH]; omega
      have hbndSH2 : (arch_msb_index48 sig + e0 - 23#16 - e0).toInt ≤ 200 := by rw [hSH]; omega
      have hno : (arch_msb_index48 sig + e0 - 23#16 - e0).toInt ≤ 0 →
          (BitVec.setWidth 50 sig).toNat *
            2 ^ (-(arch_msb_index48 sig + e0 - 23#16 - e0).toInt).toNat < 2 ^ 50 := by
        intro hle
        rw [zsig_toNat, hSH] at *
        have : sig.toNat * 2 ^ (-((Nat.log2 sig.toNat : Int) - 23)).toNat
              < 2 ^ (Nat.log2 sig.toNat + 1) * 2 ^ (-((Nat.log2 sig.toNat : Int) - 23)).toNat :=
          (Nat.mul_lt_mul_right (Nat.two_pow_pos _)).mpr hsig_hi
        have heq : 2 ^ (Nat.log2 sig.toNat + 1) * 2 ^ (-((Nat.log2 sig.toNat : Int) - 23)).toNat
              = 2 ^ (Nat.log2 sig.toNat + 1 + (-((Nat.log2 sig.toNat : Int) - 23)).toNat) :=
          (Nat.pow_add 2 _ _).symm
        have hbound : Nat.log2 sig.toNat + 1 + (-((Nat.log2 sig.toNat : Int) - 23)).toNat ≤ 24 := by
          omega
        calc sig.toNat * 2 ^ (-((Nat.log2 sig.toNat : Int) - 23)).toNat
            < 2 ^ (Nat.log2 sig.toNat + 1 + (-((Nat.log2 sig.toNat : Int) - 23)).toNat) := by
              rw [← heq]; exact this
          _ ≤ 2 ^ 50 := Nat.pow_le_pow_right (by decide) (by omega)
      -- biased (BitVec) value and +1
      have hB : (arch_msb_index48 sig + e0 + 127#16).toInt
          = (Nat.log2 sig.toNat : Int) + e0.toInt + 127 := (exp_facts sig e0 hsig hlo hhi).2
      have h1i : (1#16 : BitVec 16).toInt = 1 := rfl
      have h255i : (255#16 : BitVec 16).toInt = 255 := rfl
      have hBp1 : (arch_msb_index48 sig + e0 + 127#16 + 1#16).toInt
          = (Nat.log2 sig.toNat : Int) + e0.toInt + 127 + 1 := by
        rw [toInt_add_of_bounds _ _ (by rw [hB, h1i]; omega) (by rw [hB, h1i]; omega), hB, h1i]
      -- kept value in clean Nat form
      have hKclean : ((if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then
              BitVec.setWidth 50 sig <<< (BitVec.setWidth 50 (0#16 - (arch_msb_index48 sig + e0 - 23#16 - e0))).toNat
            else BitVec.setWidth 50 sig >>> (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0)).toNat)
           + BitVec.setWidth 50
              ((if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then 0#1
                else BitVec.extractLsb 0 0 (BitVec.setWidth 50 sig >>> (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0 - 1#16)).toNat))
                &&& ((if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then 0#1
                      else BitVec.ofBool (BitVec.setWidth 50 sig &&& ((1#50 <<< (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0 - 1#16)).toNat) - 1#50) != 0#50))
                      ||| BitVec.extractLsb 0 0
                            (if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then
                              BitVec.setWidth 50 sig <<< (BitVec.setWidth 50 (0#16 - (arch_msb_index48 sig + e0 - 23#16 - e0))).toNat
                             else BitVec.setWidth 50 sig >>> (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0)).toNat)))).toNat
          = (if Nat.log2 sig.toNat ≤ 23 then sig.toNat * 2 ^ (23 - Nat.log2 sig.toNat)
             else rneQuot sig.toNat (Nat.log2 sig.toNat - 23)) := by
        rw [kept_unified (BitVec.setWidth 50 sig) (arch_msb_index48 sig + e0 - 23#16 - e0) hbndSH hbndSH2 hno, zsig_toNat]
        by_cases hc : (arch_msb_index48 sig + e0 - 23#16 - e0).toInt ≤ 0
        · rw [if_pos hc, if_pos (show Nat.log2 sig.toNat ≤ 23 by rw [hSH] at hc; omega),
              show (-(arch_msb_index48 sig + e0 - 23#16 - e0).toInt).toNat = 23 - Nat.log2 sig.toNat from by rw [hSH]; omega]
        · rw [if_neg hc, if_neg (show ¬ Nat.log2 sig.toNat ≤ 23 by rw [hSH] at hc; omega),
              show (arch_msb_index48 sig + e0 - 23#16 - e0).toNat = Nat.log2 sig.toNat - 23 from by
                rw [toNat_of_toInt_nonneg _ (by omega), hSH]; omega]
      -- spec kept value to the same clean Nat form
      have hspec_clean : (if (Nat.log2 sig.toNat : Int) + e0.toInt - 23 - e0.toInt ≤ 0 then
              sig.toNat * 2 ^ (-((Nat.log2 sig.toNat : Int) + e0.toInt - 23 - e0.toInt)).toNat
            else rneQuot sig.toNat ((Nat.log2 sig.toNat : Int) + e0.toInt - 23 - e0.toInt).toNat)
          = (if Nat.log2 sig.toNat ≤ 23 then sig.toNat * 2 ^ (23 - Nat.log2 sig.toNat)
             else rneQuot sig.toNat (Nat.log2 sig.toNat - 23)) := by
        by_cases hc : Nat.log2 sig.toNat ≤ 23
        · rw [if_pos (show (Nat.log2 sig.toNat : Int) + e0.toInt - 23 - e0.toInt ≤ 0 by omega), if_pos hc,
              show (-((Nat.log2 sig.toNat : Int) + e0.toInt - 23 - e0.toInt)).toNat = 23 - Nat.log2 sig.toNat from by omega]
        · rw [if_neg (show ¬ (Nat.log2 sig.toNat : Int) + e0.toInt - 23 - e0.toInt ≤ 0 by omega), if_neg hc,
              show ((Nat.log2 sig.toNat : Int) + e0.toInt - 23 - e0.toInt).toNat = Nat.log2 sig.toNat - 23 from by omega]
      have hKrange := kept_clean_range sig.toNat (Nat.log2 sig.toNat) hsig_lo hsig_hi
      have hKle : ((if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then
              BitVec.setWidth 50 sig <<< (BitVec.setWidth 50 (0#16 - (arch_msb_index48 sig + e0 - 23#16 - e0))).toNat
            else BitVec.setWidth 50 sig >>> (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0)).toNat)
           + BitVec.setWidth 50
              ((if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then 0#1
                else BitVec.extractLsb 0 0 (BitVec.setWidth 50 sig >>> (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0 - 1#16)).toNat))
                &&& ((if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then 0#1
                      else BitVec.ofBool (BitVec.setWidth 50 sig &&& ((1#50 <<< (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0 - 1#16)).toNat) - 1#50) != 0#50))
                      ||| BitVec.extractLsb 0 0
                            (if BitVec.sle (arch_msb_index48 sig + e0 - 23#16 - e0) 0#16 then
                              BitVec.setWidth 50 sig <<< (BitVec.setWidth 50 (0#16 - (arch_msb_index48 sig + e0 - 23#16 - e0))).toNat
                             else BitVec.setWidth 50 sig >>> (BitVec.setWidth 50 (arch_msb_index48 sig + e0 - 23#16 - e0)).toNat)))).toNat ≤ 2 ^ 24 := by
        rw [hKclean]; exact hKrange.2
      rw [hspec_clean, carry_bit _ hKle, hKclean]
      simp only [decide_eq_true_eq]
      have hBnn : (0:Int) ≤ (arch_msb_index48 sig + e0 + 127#16).toInt := by rw [hB]; omega
      by_cases hcarry : 2 ^ 24 ≤ (if Nat.log2 sig.toNat ≤ 23 then sig.toNat * 2 ^ (23 - Nat.log2 sig.toNat)
                                  else rneQuot sig.toNat (Nat.log2 sig.toNat - 23))
      · -- carry: exponent +1, kept >> 1
        simp only [if_pos hcarry]
        rw [BitVec.sle_eq_decide, h255i, hBp1]
        simp only [decide_eq_true_eq]
        by_cases hov : (255:Int) ≤ (Nat.log2 sig.toNat : Int) + e0.toInt + 127 + 1
        · rw [if_pos hov, if_pos hov, combine']
          simp only [show (255#8 : BitVec 8).toNat = 255 from rfl, show (0#23 : BitVec 23).toNat = 0 from rfl]
          rw [sgn_eq]
        · rw [if_neg hov, if_neg hov, combine', extract8_toNat, extract23_toNat,
              BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow,
              show (BitVec.setWidth 50 1#16).toNat = 1 from by simp,
              Nat.pow_one, hKclean, sgn_eq,
              show (arch_msb_index48 sig + e0 + 127#16 + 1#16).toNat
                  = ((Nat.log2 sig.toNat : Int) + e0.toInt + 127 + 1).toNat from by
                rw [toNat_of_toInt_nonneg _ (by rw [hBp1]; omega), hBp1]]
      · -- no carry
        simp only [if_neg hcarry]
        rw [BitVec.sle_eq_decide, h255i, hB]
        simp only [decide_eq_true_eq]
        by_cases hov : (255:Int) ≤ (Nat.log2 sig.toNat : Int) + e0.toInt + 127
        · rw [if_pos hov, if_pos hov, combine']
          simp only [show (255#8 : BitVec 8).toNat = 255 from rfl, show (0#23 : BitVec 23).toNat = 0 from rfl]
          rw [sgn_eq]
        · rw [if_neg hov, if_neg hov, combine', extract8_toNat, extract23_toNat, hKclean, sgn_eq,
              show (arch_msb_index48 sig + e0 + 127#16).toNat
                  = ((Nat.log2 sig.toNat : Int) + e0.toInt + 127).toNat from by
                rw [toNat_of_toInt_nonneg _ hBnn, hB]]


end ArchFp
