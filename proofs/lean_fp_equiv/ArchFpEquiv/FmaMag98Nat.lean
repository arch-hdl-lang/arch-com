import ArchFpEquiv.Model
import Std.Tactic.BVDecide

/-!
# `arch_fma_mag98` — BitVec→Nat field bridges

The sticky-fold magnitude `arch_fma_mag98` is a 98-bit alignment network over a
48-bit high significand `sig_hi`, a 48-bit low significand `sig_lo`, and a 16-bit
exponent gap `diff`. This file proves the value (`toNat`) of each field of that
network, generically over the inputs, using the core `BitVec.toNat_*` bridges.
These feed `fma_mag98_same_nat` (the same-sign magnitude as a Nat formula).

* `loExt96_toNat`  — `sig_lo << 48`  is exactly `sig_lo · 2^48` (no truncation).
* `loField96_toNat` — `(sig_lo << 48) >> d`  is `(sig_lo · 2^48) / 2^d`.
* `mask96_and_toNat` — `x & ((1<<d) − 1)`  is `x mod 2^d` **for every `d`** (the
  `d ≥ 96` case saturates: the mask becomes all-ones and `x < 2^96 ≤ 2^d`).
* `hiField98_toNat` — `(sig_hi << 48) ++ 0`  widened is `sig_hi · 2^49`.
-/

namespace ArchFp

/-- `2^a · 2^b = 2^(a+b)` as a rewrite-friendly fact. -/
private theorem pow_mul_pow (a b : Nat) : (2 : Nat) ^ a * 2 ^ b = 2 ^ (a + b) := by
  rw [← Nat.pow_add]

/-- `a < 2^p ⇒ a·2^q < 2^(p+q)`. -/
private theorem mul_pow_lt {a p q : Nat} (h : a < 2 ^ p) : a * 2 ^ q < 2 ^ (p + q) := by
  rw [Nat.pow_add]
  exact (Nat.mul_lt_mul_right (Nat.pow_pos (by decide : 0 < 2))).mpr h

/-- `sig_lo << 48` carries the exact value `sig_lo · 2^48` (fits in 96 bits). -/
theorem loExt96_toNat (x : BitVec 48) :
    (BitVec.setWidth 96 x <<< (48 : Nat)).toNat = x.toNat * 2 ^ 48 := by
  have hx : x.toNat < 2 ^ 48 := x.isLt
  have hb : x.toNat * 2 ^ 48 < 2 ^ 96 :=
    Nat.lt_of_lt_of_le (mul_pow_lt hx) (Nat.pow_le_pow_right (by decide) (by omega))
  rw [BitVec.toNat_shiftLeft, BitVec.toNat_setWidth, Nat.shiftLeft_eq,
      Nat.mod_eq_of_lt (by omega : x.toNat < 2 ^ 96), Nat.mod_eq_of_lt hb]

/-- `(sig_lo << 48) >> d` is the integer `(sig_lo · 2^48) / 2^d`. -/
theorem loField96_toNat (x : BitVec 48) (d : Nat) :
    ((BitVec.setWidth 96 x <<< (48 : Nat)) >>> d).toNat = (x.toNat * 2 ^ 48) / 2 ^ d := by
  rw [BitVec.toNat_ushiftRight, loExt96_toNat, Nat.shiftRight_eq_div_pow]

/-- The low-`d`-bit mask, applied to any 96-bit `v`, extracts `v mod 2^d` — for
    every shift amount `d` (saturating correctly when `d ≥ 96`). -/
theorem mask96_and_toNat (v : BitVec 96) (d : Nat) :
    (v &&& ((1#96 <<< d) - 1#96)).toNat = v.toNat % 2 ^ d := by
  have hvlt : v.toNat < 2 ^ 96 := v.isLt
  have hone : ((1#96 : BitVec 96)).toNat = 1 := BitVec.toNat_one (by omega)
  have hpd : 0 < 2 ^ d := Nat.pow_pos (by decide)
  rw [BitVec.toNat_and]
  by_cases hd : d < 96
  · have hlt : (2 : Nat) ^ d < 2 ^ 96 := Nat.pow_lt_pow_right (by decide) hd
    have h1 : ((1#96 : BitVec 96) <<< d).toNat = 2 ^ d := by
      rw [BitVec.toNat_shiftLeft, hone, Nat.shiftLeft_eq, Nat.one_mul, Nat.mod_eq_of_lt hlt]
    have hmask : (((1#96 : BitVec 96) <<< d) - 1#96).toNat = 2 ^ d - 1 := by
      rw [BitVec.toNat_sub, h1, hone,
          show 2 ^ 96 - 1 + 2 ^ d = 2 ^ 96 + (2 ^ d - 1) from by omega, Nat.add_mod_left]
      exact Nat.mod_eq_of_lt (by omega)
    rw [hmask, Nat.and_two_pow_sub_one_eq_mod]
  · have hdge : 96 ≤ d := by omega
    have hle : (2 : Nat) ^ 96 ≤ 2 ^ d := Nat.pow_le_pow_right (by decide) hdge
    have h1 : ((1#96 : BitVec 96) <<< d).toNat = 0 := by
      rw [BitVec.toNat_shiftLeft, hone, Nat.shiftLeft_eq, Nat.one_mul,
          show (2 : Nat) ^ d = 2 ^ (d - 96) * 2 ^ 96 from by rw [pow_mul_pow]; congr 1; omega,
          Nat.mul_mod_left]
    have hmask : (((1#96 : BitVec 96) <<< d) - 1#96).toNat = 2 ^ 96 - 1 := by
      rw [BitVec.toNat_sub, h1, hone]
    rw [hmask, Nat.and_two_pow_sub_one_eq_mod, Nat.mod_eq_of_lt hvlt,
        Nat.mod_eq_of_lt (by omega : v.toNat < 2 ^ d)]

/-- The high field: `(sig_hi << 48) ++ 0`, widened to 98, is `sig_hi · 2^49`. -/
theorem hiField98_toNat (x : BitVec 48) :
    (BitVec.setWidth 98 ((BitVec.setWidth 96 x <<< (48 : Nat)) ++ (0#1 : BitVec 1))).toNat
      = x.toNat * 2 ^ 49 := by
  have hx : x.toNat < 2 ^ 48 := x.isLt
  have hb2 : x.toNat * 2 ^ 49 < 2 ^ 98 :=
    Nat.lt_of_lt_of_le (mul_pow_lt hx) (Nat.pow_le_pow_right (by decide) (by omega))
  rw [BitVec.toNat_setWidth, BitVec.toNat_append, loExt96_toNat,
      show ((0#1 : BitVec 1).toNat) = 0 from by decide, Nat.or_zero, Nat.shiftLeft_eq,
      show x.toNat * 2 ^ 48 * 2 ^ 1 = x.toNat * 2 ^ 49 from by rw [Nat.mul_assoc, pow_mul_pow]]
  exact Nat.mod_eq_of_lt hb2

end ArchFp
