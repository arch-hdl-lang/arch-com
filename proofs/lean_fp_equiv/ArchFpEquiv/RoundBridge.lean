import ArchFpEquiv.RoundCore
/-!
# Tier 2, part 4 — the rounding step, bridged bit ⟷ value

Connects arch_round48's *actual* BitVec rounding datapath to the proved
round-to-nearest-even kernel (`RoundCore.rne_matches`). `roundupBit` mirrors the
`_t60.._t70` lets in `arch_round48` (guard bit, sticky via the `(1<<<g)-1` mask,
round-up = guard ∧ (sticky ∨ lsb)); `roundupBit_toNat` proves it equals
`guardStickyUp` on the Nat value, and `round_step` proves the whole step
`(v >>> sh) + roundup` equals `rneQuot v.toNat sh`. Pure core (no Mathlib): the
`BitVec.toNat_*` bridges plus the `RoundCore` Nat lemmas.
-/

namespace ArchFp

def roundupBit (v : BitVec 50) (sh : Nat) : BitVec 1 :=
  ((v >>> (sh - 1)).extractLsb' 0 1)
    &&& ((BitVec.ofBool (v &&& ((1#50 <<< (sh - 1)) - 1#50) != 0#50))
          ||| ((v >>> sh).extractLsb' 0 1))

theorem roundupBit_toNat (v : BitVec 50) (sh : Nat) (h1 : 1 ≤ sh) (h2 : sh - 1 < 50) :
    (roundupBit v sh).toNat = (if guardStickyUp v.toNat sh then 1 else 0) := by
  have hsh : sh - 1 + 1 = sh := by omega
  have hpos : 0 < 2 ^ (sh - 1) := Nat.pow_pos (by decide : 0 < 2)
  have hlt : 2 ^ (sh - 1) < 2 ^ 50 := Nat.pow_lt_pow_right (by decide : 1 < 2) h2
  have hG : (((v >>> (sh - 1)).extractLsb' 0 1)).toNat = (v.toNat / 2 ^ (sh - 1)) % 2 := by
    simp [BitVec.extractLsb'_toNat, BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
  have hL : (((v >>> sh).extractLsb' 0 1)).toNat = (v.toNat / 2 ^ sh) % 2 := by
    simp [BitVec.extractLsb'_toNat, BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
  have hsl : ((1#50 <<< (sh - 1))).toNat = 2 ^ (sh - 1) := by
    have e : ((1#50 : BitVec 50) <<< (sh-1)).toNat = (1 <<< (sh-1)) % 2^50 := by
      simp [BitVec.toNat_shiftLeft]
    rw [e, Nat.shiftLeft_eq, Nat.one_mul, Nat.mod_eq_of_lt hlt]
  have hmask : ((1#50 <<< (sh - 1)) - 1#50).toNat = 2 ^ (sh - 1) - 1 := by
    rw [BitVec.toNat_sub, hsl]; simp only [BitVec.toNat_ofNat]; omega
  have hsticky : (v &&& ((1#50 <<< (sh - 1)) - 1#50)).toNat = v.toNat % 2 ^ (sh - 1) := by
    rw [BitVec.toNat_and, hmask, Nat.and_two_pow_sub_one_eq_mod]
  have hSt : (BitVec.ofBool (v &&& ((1#50 <<< (sh - 1)) - 1#50) != 0#50)).toNat
      = (if v.toNat % 2 ^ (sh - 1) ≠ 0 then 1 else 0) := by
    rw [BitVec.toNat_ofBool]
    by_cases hz : v.toNat % 2 ^ (sh - 1) = 0
    · have h0 : v &&& ((1#50 <<< (sh - 1)) - 1#50) = 0#50 :=
        BitVec.eq_of_toNat_eq (by rw [hsticky, hz]; rfl)
      simp [h0, hz]
    · have h0 : v &&& ((1#50 <<< (sh - 1)) - 1#50) ≠ 0#50 := by
        intro hc; apply hz; rw [← hsticky, hc]; rfl
      simp [h0, hz]
  have pG : ((v.toNat / 2 ^ (sh - 1)) % 2 = 1) ↔ (2 ^ (sh - 1) ≤ v.toNat % 2 ^ sh) := by
    have := guard_bit_eq v.toNat (sh - 1); rwa [hsh] at this
  have pS : (v.toNat % 2 ^ sh % 2 ^ (sh - 1) ≠ 0) ↔ (v.toNat % 2 ^ (sh - 1) ≠ 0) := by
    rw [Nat.mod_mod_of_dvd v.toNat (Nat.pow_dvd_pow 2 (by omega : sh - 1 ≤ sh))]
  unfold roundupBit guardStickyUp
  rw [BitVec.toNat_and, BitVec.toNat_or, hG, hL, hSt]
  simp only [← pG, pS]
  rcases (by omega : (v.toNat/2^(sh-1))%2 = 0 ∨ (v.toNat/2^(sh-1))%2 = 1) with hg | hg <;>
  rcases (by omega : (v.toNat/2^sh)%2 = 0 ∨ (v.toNat/2^sh)%2 = 1) with hl | hl <;>
  by_cases hs : v.toNat % 2 ^ (sh - 1) = 0 <;>
  simp [hg, hl, hs]

theorem round_step (v : BitVec 50) (sh : Nat) (h1 : 1 ≤ sh) (h2 : sh - 1 < 50) :
    ((v >>> sh) + BitVec.setWidth 50 (roundupBit v sh)).toNat = rneQuot v.toNat sh := by
  have hv : v.toNat < 2 ^ 50 := v.isLt
  have hq : v.toNat / 2 ^ sh ≤ v.toNat / 2 := by
    apply Nat.div_le_div_left _ (by decide)
    calc (2:Nat) = 2 ^ 1 := rfl
      _ ≤ 2 ^ sh := Nat.pow_le_pow_right (by decide) h1
  have hno : v.toNat / 2 ^ sh + (if guardStickyUp v.toNat sh then 1 else 0) < 2 ^ 50 := by
    split <;> omega
  rw [BitVec.toNat_add, BitVec.toNat_ushiftRight, BitVec.toNat_setWidth,
    Nat.shiftRight_eq_div_pow, roundupBit_toNat v sh h1 h2]
  have hif : (if guardStickyUp v.toNat sh then 1 else 0) % 2 ^ 50
      = (if guardStickyUp v.toNat sh then 1 else 0) := by
    apply Nat.mod_eq_of_lt; split <;> omega
  rw [hif, Nat.mod_eq_of_lt hno]
  exact rne_matches v.toNat sh h1

end ArchFp
