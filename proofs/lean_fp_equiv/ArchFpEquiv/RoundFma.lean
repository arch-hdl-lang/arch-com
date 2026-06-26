import ArchFpEquiv.RoundProof
import Std.Tactic.BVDecide

/-! # Tier 2, fma — width-470 rounder instantiation (in progress) -/

-- The unrolled 470-bit clz (`arch_msb_index470`) is a deep term; simp/norm_cast
-- traversals of it exceed the default recursion depth.
set_option maxRecDepth 10000

namespace ArchFp

def round470_struct (s : BitVec 1) (sig : BitVec 470) (e0 : BitVec 16) : BitVec 32 :=
  if sig == 0#470 then s ++ 0#31
  else
    let p := arch_msb_index470 sig
    let ev := p + e0
    let biased := ev + 127#16
    let isSub := BitVec.sle biased 0#16
    let k := if isSub then 65387#16 else ev - 23#16
    let sh := k - e0
    let shLe0 := BitVec.sle sh 0#16
    let zsig := BitVec.setWidth 472 sig
    let kept0 := if shLe0 then zsig <<< (BitVec.setWidth 472 (0#16 - sh)).toNat
                 else zsig >>> (BitVec.setWidth 472 sh).toNat
    let shm1 := BitVec.setWidth 472 (sh - 1#16)
    let guardRaw := BitVec.extractLsb 0 0 (zsig >>> shm1.toNat)
    let guard := if shLe0 then 0#1 else guardRaw
    let mask := ((1#472) <<< shm1.toNat) - 1#472
    let stickyRaw := BitVec.ofBool (zsig &&& mask != 0#472)
    let sticky := if shLe0 then 0#1 else stickyRaw
    let lsb := BitVec.extractLsb 0 0 kept0
    let roundup := guard &&& (sticky ||| lsb)
    let kept := kept0 + BitVec.setWidth 472 roundup
    let subRes := (s ++ 0#31) ||| (s ++ BitVec.extractLsb 30 0 kept)
    let carry := BitVec.ofBool (BitVec.extractLsb 24 24 kept == 1#1)
    let biasedN := if carry == 1#1 then biased + 1#16 else biased
    let overflow := BitVec.sle 255#16 biasedN
    let infRes := s ++ ((0xFF#8) ++ (0#23))
    let keptN := if carry == 1#1 then kept >>> (BitVec.setWidth 472 (1#16)).toNat else kept
    let normRes := s ++ (BitVec.extractLsb 7 0 biasedN ++ BitVec.extractLsb 22 0 keptN)
    let nonSub := if overflow then infRes else normRes
    if isSub then subRes else nonSub


theorem arch_eq_struct470 (s : BitVec 1) (sig : BitVec 470) (e0 : BitVec 16) :
    arch_round470 s sig e0 = round470_struct s sig e0 := by
  unfold arch_round470 round470_struct arch_msb_index470
  bv_decide (config := { timeout := 600 })

-- ── Batch 1: clz + exponent bridges at width 470 ─────────────────────────────

/-- The binary-search clz finds the true MSB (470-bit). bv_decide (~12s). -/
theorem msb_index_finds_msb470 (sig : BitVec 470) (h : sig ≠ 0#470) :
    sig >>> (arch_msb_index470 sig).toNat = 1#470 := by
  unfold arch_msb_index470
  bv_decide (config := { timeout := 300 })

/-- Value-level bracket `2^p ≤ sig < 2^(p+1)`. -/
theorem msb_index_bound470 (sig : BitVec 470) (h : sig ≠ 0#470) :
    2 ^ (arch_msb_index470 sig).toNat ≤ sig.toNat
      ∧ sig.toNat < 2 ^ ((arch_msb_index470 sig).toNat + 1) := by
  have hb := msb_index_finds_msb470 sig h
  have hdiv : sig.toNat / 2 ^ (arch_msb_index470 sig).toNat = 1 := by
    have h2 := congrArg BitVec.toNat hb
    rwa [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow, BitVec.toNat_one (by omega)] at h2
  have hpos : 0 < 2 ^ (arch_msb_index470 sig).toNat := Nat.pow_pos (by decide : 0 < 2)
  have hdm := Nat.div_add_mod sig.toNat (2 ^ (arch_msb_index470 sig).toNat)
  have hmod : sig.toNat % 2 ^ (arch_msb_index470 sig).toNat
      < 2 ^ (arch_msb_index470 sig).toNat := Nat.mod_lt _ hpos
  rw [hdiv, Nat.mul_one] at hdm
  have hps : 2 ^ ((arch_msb_index470 sig).toNat + 1)
      = 2 ^ (arch_msb_index470 sig).toNat * 2 := Nat.pow_succ 2 (arch_msb_index470 sig).toNat
  omega

/-- arch's clz is `Nat.log2`. -/
theorem msb_index_eq_log2470 (sig : BitVec 470) (h : sig ≠ 0#470) :
    (arch_msb_index470 sig).toNat = Nat.log2 sig.toNat := by
  have hn : sig.toNat ≠ 0 := by
    intro hz; exact h (BitVec.eq_of_toNat_eq (by rw [hz, BitVec.toNat_zero]))
  obtain ⟨_hlo, hhi⟩ := msb_index_bound470 sig h
  have llo := Nat.log2_self_le hn
  have lhi := Nat.lt_log2_self (n := sig.toNat)
  rcases Nat.lt_trichotomy (arch_msb_index470 sig).toNat (Nat.log2 sig.toNat) with hc | hc | hc
  · exfalso
    have hp : 2 ^ ((arch_msb_index470 sig).toNat + 1) ≤ 2 ^ Nat.log2 sig.toNat :=
      Nat.pow_le_pow_right (by decide) hc
    omega
  · exact hc
  · exfalso
    have hp : 2 ^ (Nat.log2 sig.toNat + 1) ≤ 2 ^ (arch_msb_index470 sig).toNat :=
      Nat.pow_le_pow_right (by decide) hc
    omega

/-- clz value-bridge: `p ≤ 469` and `p.toInt = log₂ sig`. -/
theorem p_facts470 (sig : BitVec 470) (h : sig ≠ 0#470) :
    (arch_msb_index470 sig).toNat ≤ 469
    ∧ (arch_msb_index470 sig).toInt = (Nat.log2 sig.toNat : Int) := by
  obtain ⟨hlo, _hhi⟩ := msb_index_bound470 sig h
  have hsig : sig.toNat < 2 ^ 470 := sig.isLt
  have h1 : 2 ^ (arch_msb_index470 sig).toNat < 2 ^ 470 := by omega
  have hple : (arch_msb_index470 sig).toNat < 470 :=
    (Nat.pow_lt_pow_iff_right (by decide : 1 < 2)).mp h1
  have hpv : (arch_msb_index470 sig).toInt = ((arch_msb_index470 sig).toNat : Int) := by
    rw [BitVec.toInt_eq_toNat_bmod, Int.bmod_eq_emod]; split <;> omega
  exact ⟨by omega, by rw [hpv, msb_index_eq_log2470 sig h]⟩

/-- Exponent chain at 470: `ev = p+e0`, `biased = ev+127` match `Int`. -/
theorem exp_facts470 (sig : BitVec 470) (e0 : BitVec 16) (h : sig ≠ 0#470)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (arch_msb_index470 sig + e0).toInt = (Nat.log2 sig.toNat : Int) + e0.toInt
    ∧ (arch_msb_index470 sig + e0 + 127#16).toInt
        = (Nat.log2 sig.toNat : Int) + e0.toInt + 127 := by
  obtain ⟨hp469, hpInt⟩ := p_facts470 sig h
  have hl469 : Nat.log2 sig.toNat ≤ 469 := by rw [← msb_index_eq_log2470 sig h]; exact hp469
  have hpge : (0 : Int) ≤ (arch_msb_index470 sig).toInt := by rw [hpInt]; exact Int.natCast_nonneg _
  have hple : (arch_msb_index470 sig).toInt ≤ 469 := by rw [hpInt]; exact_mod_cast hl469
  have hev : (arch_msb_index470 sig + e0).toInt = (arch_msb_index470 sig).toInt + e0.toInt :=
    toInt_add_of_bounds _ _ (by omega) (by omega)
  have h127 : (127#16).toInt = 127 := rfl
  have hbiased : (arch_msb_index470 sig + e0 + 127#16).toInt
      = (arch_msb_index470 sig + e0).toInt + 127 := by
    have := toInt_add_of_bounds (arch_msb_index470 sig + e0) (127#16)
      (by rw [hev, h127]; omega) (by rw [hev, h127]; omega)
    rwa [h127] at this
  exact ⟨by rw [hev, hpInt], by rw [hbiased, hev, hpInt]⟩

/-- `isSub` ⟷ `biased ≤ 0` at 470. -/
theorem isSub_iff470 (sig : BitVec 470) (e0 : BitVec 16) (h : sig ≠ 0#470)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (BitVec.sle (arch_msb_index470 sig + e0 + 127#16) 0#16 = true)
      ↔ ((Nat.log2 sig.toNat : Int) + e0.toInt + 127 ≤ 0) := by
  rw [BitVec.sle_iff_toInt_le, (exp_facts470 sig e0 h hlo hhi).2]; exact ⟨fun x => x, fun x => x⟩

/-- Shift value, normal branch: `sh = log2 - 23`. -/
theorem sh_normal470 (sig : BitVec 470) (e0 : BitVec 16) (h : sig ≠ 0#470)
    (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (arch_msb_index470 sig + e0 - 23#16 - e0).toInt = (Nat.log2 sig.toNat : Int) - 23 := by
  obtain ⟨hp469, hpInt⟩ := p_facts470 sig h
  have hl469 : Nat.log2 sig.toNat ≤ 469 := by rw [← msb_index_eq_log2470 sig h]; exact hp469
  have hev := (exp_facts470 sig e0 h hlo hhi).1
  have h23 : (23#16).toInt = 23 := rfl
  have hk : (arch_msb_index470 sig + e0 - 23#16).toInt = (Nat.log2 sig.toNat : Int) + e0.toInt - 23 := by
    rw [toInt_sub_of_bounds _ _ (by rw [hev, h23]; omega) (by rw [hev, h23]; omega), hev, h23]
  rw [toInt_sub_of_bounds _ _ (by rw [hk]; omega) (by rw [hk]; omega), hk]; omega

/-- Shift value, subnormal branch: `sh = -149 - e0`. -/
theorem sh_sub470 (e0 : BitVec 16) (hlo : -298 ≤ e0.toInt) (hhi : e0.toInt ≤ 208) :
    (65387#16 - e0).toInt = -149 - e0.toInt := by
  have hk : (65387#16).toInt = -149 := rfl
  rw [toInt_sub_of_bounds _ _ (by rw [hk]; omega) (by rw [hk]; omega), hk]

/-- A 16-bit value widened to 472 bits keeps its `toNat`. -/
theorem sw472_toNat (sh : BitVec 16) : (BitVec.setWidth 472 sh).toNat = sh.toNat := by
  rw [BitVec.toNat_setWidth]; exact Nat.mod_eq_of_lt (by have := sh.isLt; omega)

/-- `(setWidth 472 (sh - 1)).toNat = sh.toNat - 1` for `sh ≥ 1`. -/
theorem sw472_sub1 (sh : BitVec 16) (h : 1 ≤ sh.toNat) :
    (BitVec.setWidth 472 (sh - 1#16)).toNat = sh.toNat - 1 := by
  rw [sw472_toNat, BitVec.toNat_sub]; have := sh.isLt
  simp only [show (1#16).toNat = 1 from rfl]; omega

/-- The 472-bit widen of the significand keeps its `toNat`. -/
theorem zsig472_toNat (sig : BitVec 470) : (BitVec.setWidth 472 sig).toNat = sig.toNat := by
  rw [BitVec.toNat_setWidth]; exact Nat.mod_eq_of_lt (by have := sig.isLt; omega)

-- ── Batch 2: kept-value lemmas at width 472 ──────────────────────────────────

/-- Negated 16-bit shift amount, wide bound (fma `sh` reaches ~-357). -/
theorem negsh_wide (sh : BitVec 16) (h0 : sh.toInt ≤ 0) (hb : -32000 ≤ sh.toInt) :
    (0#16 - sh).toNat = (-sh.toInt).toNat := by
  have h1 : (0#16 - sh).toInt = - sh.toInt := by
    rw [toInt_sub_of_bounds] <;> simp only [show (0#16).toInt = 0 from rfl] <;> omega
  have h2 := BitVec.toInt_eq_toNat_bmod (0#16 - sh)
  have h3 := (0#16 - sh).isLt
  rw [h1, Int.bmod_eq_emod] at h2
  split at h2 <;> omega

/-- Left-shift (exact) kept value at 472, under no 472-bit overflow. -/
theorem kept_left470 (zsig : BitVec 472) (amt : Nat) (hno : zsig.toNat * 2 ^ amt < 2 ^ 472) :
    (zsig <<< amt).toNat = zsig.toNat * 2 ^ amt := by
  rw [BitVec.toNat_shiftLeft, Nat.shiftLeft_eq, Nat.mod_eq_of_lt hno]

/-- A right shift by ≥ 472 zeroes a 472-bit value. -/
theorem kept_big_zero470 (zsig : BitVec 472) (sh : Nat) (h : 472 ≤ sh) :
    zsig.toNat / 2 ^ sh = 0 := by
  have h1 : zsig.toNat < 2 ^ 472 := zsig.isLt
  have h2 : (2:Nat) ^ 472 ≤ 2 ^ sh := Nat.pow_le_pow_right (by decide) h
  exact Nat.div_eq_of_lt (by omega)

/-- Deep underflow: `rneQuot n sh = 0` for `n < 2^472` and `sh ≥ 473`. -/
theorem rneQuot_big_zero470 (n sh : Nat) (hn : n < 2 ^ 472) (h : 473 ≤ sh) :
    rneQuot n sh = 0 := by
  unfold rneQuot
  have hge : (2:Nat) ^ 472 ≤ 2 ^ sh := Nat.pow_le_pow_right (by decide) (by omega)
  have hge1 : (2:Nat) ^ 472 ≤ 2 ^ (sh - 1) := Nat.pow_le_pow_right (by decide) (by omega)
  rw [Nat.div_eq_of_lt (by omega), Nat.mod_eq_of_lt (by omega), if_neg (by omega)]

/-- arch's inlined roundup equals the generic `roundupBit` (472). -/
theorem struct_roundup_eq470 (zsig : BitVec 472) (sh : BitVec 16) (h1 : 1 ≤ sh.toNat) :
    (BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 472 (sh - 1#16)).toNat))
      &&& ((BitVec.ofBool (zsig &&& ((1#472 <<< (BitVec.setWidth 472 (sh - 1#16)).toNat) - 1#472) != 0#472))
            ||| (BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 472 sh).toNat)))
    = roundupBit zsig sh.toNat := by
  rw [sw472_sub1 sh h1, sw472_toNat]; unfold roundupBit; bv_decide

/-- Central rounding result at 472: arch's kept significand = RNE quotient, 1 ≤ sh ≤ 472. -/
theorem kept_value470 (zsig : BitVec 472) (sh : BitVec 16) (h1 : 1 ≤ sh.toNat) (h2 : sh.toNat ≤ 472) :
    ((zsig >>> (BitVec.setWidth 472 sh).toNat)
      + BitVec.setWidth 472
          ((BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 472 (sh - 1#16)).toNat))
            &&& ((BitVec.ofBool (zsig &&& ((1#472 <<< (BitVec.setWidth 472 (sh - 1#16)).toNat) - 1#472) != 0#472))
                  ||| (BitVec.extractLsb 0 0 (zsig >>> (BitVec.setWidth 472 sh).toNat))))).toNat
    = rneQuot zsig.toNat sh.toNat := by
  rw [struct_roundup_eq470 zsig sh h1, sw472_toNat]
  exact round_step zsig sh.toNat h1 (by omega)

theorem kept_unified470 (sig : BitVec 472) (sh : BitVec 16)
    (hbnd : -32000 ≤ sh.toInt) (hbnd2 : sh.toInt ≤ 32000)
    (hno : sh.toInt ≤ 0 → sig.toNat * 2 ^ (-sh.toInt).toNat < 2 ^ 472) :
    ((if BitVec.sle sh 0#16 then sig <<< (BitVec.setWidth 472 (0#16 - sh)).toNat
      else sig >>> (BitVec.setWidth 472 sh).toNat)
     + BitVec.setWidth 472
        ((if BitVec.sle sh 0#16 then 0#1
          else BitVec.extractLsb 0 0 (sig >>> (BitVec.setWidth 472 (sh - 1#16)).toNat))
          &&& ((if BitVec.sle sh 0#16 then 0#1
                else BitVec.ofBool (sig &&& ((1#472 <<< (BitVec.setWidth 472 (sh - 1#16)).toNat) - 1#472) != 0#472))
                ||| BitVec.extractLsb 0 0
                      (if BitVec.sle sh 0#16 then sig <<< (BitVec.setWidth 472 (0#16 - sh)).toNat
                       else sig >>> (BitVec.setWidth 472 sh).toNat)))).toNat
    = (if sh.toInt ≤ 0 then sig.toNat * 2 ^ (-sh.toInt).toNat else rneQuot sig.toNat sh.toNat) := by
  by_cases hle : BitVec.sle sh 0#16
  · have hsi : sh.toInt ≤ 0 := by rw [BitVec.sle_iff_toInt_le] at hle; simpa using hle
    simp only [hle, if_true, if_pos hsi]
    have hadd : ∀ x : BitVec 472, x + BitVec.setWidth 472 ((0#1 : BitVec 1) &&& (0#1 ||| BitVec.extractLsb 0 0 x)) = x := by
      intro x; bv_decide
    rw [hadd, sw472_toNat, negsh_wide sh hsi (by omega)]
    exact kept_left470 sig _ (hno hsi)
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
    by_cases hsmall : sh.toNat ≤ 472
    · exact kept_value470 sig sh hn1 hsmall
    · have hsig50 : sig.toNat < 2 ^ 472 := sig.isLt
      have hk0 : sig >>> (BitVec.setWidth 472 sh).toNat = 0#472 := by
        apply BitVec.eq_of_toNat_eq
        rw [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow, sw472_toNat]
        simp [kept_big_zero470 sig sh.toNat (by omega)]
      have hg0 : sig >>> (BitVec.setWidth 472 (sh - 1#16)).toNat = 0#472 := by
        apply BitVec.eq_of_toNat_eq
        rw [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow, sw472_sub1 sh hn1]
        have hlt : sig.toNat < 2 ^ (sh.toNat - 1) := by
          have : (2:Nat)^472 ≤ 2^(sh.toNat-1) := Nat.pow_le_pow_right (by decide) (by omega); omega
        simp [Nat.div_eq_of_lt hlt]
      rw [hk0, hg0, rneQuot_big_zero470 sig.toNat sh.toNat hsig50 (by omega)]
      bv_decide

/-- `sig=0` case for the 470 struct. -/
theorem struct_zero470 (s : BitVec 1) (e0 : BitVec 16) :
    round470_struct s 0#470 e0 = roundNE_f32 (s == 1#1) (0#470).toNat e0.toInt := by
  rw [show (0#470).toNat = 0 from BitVec.toNat_zero, roundNE_zero]
  unfold round470_struct
  simp only [show ((0#470 : BitVec 470) == 0#470) = true from by simp, if_true]
  rw [apply_ite (BitVec.ofNat 32)]; bv_decide


end ArchFp
