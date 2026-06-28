import ArchFpEquiv.RoundCore
import Std.Tactic.BVDecide

/-!
# The guard/round/sticky collapse lemma

The mathematical heart of the sticky-fold rounding invariance: two magnitudes
that agree above a bit position `g` and carry the same "any low bit set" sticky
below `g` round to the same value under RNE — provided `g` is strictly below the
round shift `sh` (so the collapsed bits only ever feed the sticky, never the
kept significand or the guard bit).

`rneQuot_sticky_collapse` proves this for the rounding kernel `rneQuot`. It is
width-independent and value-level; the fma invariance instantiates it with the
fold position and the (shared) high part of the two aligned magnitudes.
-/

namespace ArchFp

set_option maxRecDepth 10000

/-- Agreeing above bit `g` implies agreeing above any higher bit `k`. -/
theorem div_pow_eq_of_div_pow_eq (m1 m2 g k : Nat) (hk : g ≤ k) (h : m1 / 2 ^ g = m2 / 2 ^ g) :
    m1 / 2 ^ k = m2 / 2 ^ k := by
  have e1 : m1 / 2 ^ k = m1 / 2 ^ g / 2 ^ (k - g) := by
    rw [Nat.div_div_eq_div_mul, ← Nat.pow_add, show g + (k - g) = k from by omega]
  have e2 : m2 / 2 ^ k = m2 / 2 ^ g / 2 ^ (k - g) := by
    rw [Nat.div_div_eq_div_mul, ← Nat.pow_add, show g + (k - g) = k from by omega]
  rw [e1, e2, h]

/-- Mixed-radix split of a residue: low `k` bits = (mid bits)·2^g + (low `g` bits). -/
theorem mod_pow_decomp (m g k : Nat) (hk : g ≤ k) :
    m % 2 ^ k = (m / 2 ^ g % 2 ^ (k - g)) * 2 ^ g + m % 2 ^ g := by
  have hsplit : (2 : Nat) ^ k = 2 ^ g * 2 ^ (k - g) := by rw [← Nat.pow_add]; congr 1; omega
  rw [hsplit, Nat.mod_mul, Nat.mul_comm, Nat.add_comm]

/-- **GRS collapse for the rounding kernel.** If `m1` and `m2` agree above bit `g`
    and have the same sticky (`= 0`) status in their low `g` bits, and `g < sh`,
    then `rneQuot m1 sh = rneQuot m2 sh`. -/
theorem rneQuot_sticky_collapse (m1 m2 g sh : Nat) (hg : g < sh)
    (hhi : m1 / 2 ^ g = m2 / 2 ^ g) (hst : (m1 % 2 ^ g = 0) ↔ (m2 % 2 ^ g = 0)) :
    rneQuot m1 sh = rneQuot m2 sh := by
  have hsh : 1 ≤ sh := by omega
  have hg2 : (2 : Nat) ^ g ≠ 0 := Nat.pos_iff_ne_zero.mp (Nat.pow_pos (by decide))
  have hdivsh : m1 / 2 ^ sh = m2 / 2 ^ sh := div_pow_eq_of_div_pow_eq m1 m2 g sh (by omega) hhi
  have hmodzero : ∀ m, (m % 2 ^ (sh - 1) = 0) ↔ (m / 2 ^ g % 2 ^ (sh - 1 - g) = 0 ∧ m % 2 ^ g = 0) := by
    intro m
    rw [mod_pow_decomp m g (sh - 1) (by omega), Nat.add_eq_zero_iff, Nat.mul_eq_zero]
    simp only [hg2, or_false]
  have ha : (2 ^ (sh - 1) ≤ m1 % 2 ^ sh) ↔ (2 ^ (sh - 1) ≤ m2 % 2 ^ sh) := by
    have hd : m1 / 2 ^ (sh - 1) = m2 / 2 ^ (sh - 1) :=
      div_pow_eq_of_div_pow_eq m1 m2 g (sh - 1) (by omega) hhi
    have e1 := guard_bit_eq m1 (sh - 1); have e2 := guard_bit_eq m2 (sh - 1)
    rw [show sh - 1 + 1 = sh from by omega] at e1 e2
    rw [← e1, ← e2, hd]
  have hb : (m1 % 2 ^ sh % 2 ^ (sh - 1) ≠ 0) ↔ (m2 % 2 ^ sh % 2 ^ (sh - 1) ≠ 0) := by
    rw [Nat.mod_mod_of_dvd m1 (Nat.pow_dvd_pow 2 (by omega : sh - 1 ≤ sh)),
        Nat.mod_mod_of_dvd m2 (Nat.pow_dvd_pow 2 (by omega : sh - 1 ≤ sh)),
        ne_eq, ne_eq, hmodzero m1, hmodzero m2, hhi, hst]
  have hc : ((m1 / 2 ^ sh) % 2 = 1) ↔ ((m2 / 2 ^ sh) % 2 = 1) := by rw [hdivsh]
  have hgsu : guardStickyUp m1 sh = guardStickyUp m2 sh := by
    unfold guardStickyUp; simp only [ha, hb, hc]
  rw [← rne_matches m1 sh hsh, ← rne_matches m2 sh hsh, hdivsh, hgsu]

end ArchFp
