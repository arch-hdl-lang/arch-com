import ArchFpEquiv.Model
import ArchFpEquiv.FmaSticky
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

set_option maxRecDepth 100000

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

/-- Appending a single bit doubles and adds: `(x ++ b).toNat = x.toNat·2 + b.toNat`. -/
theorem append_bit_toNat {n : Nat} (x : BitVec n) (b : BitVec 1) :
    (x ++ b).toNat = x.toNat * 2 + b.toNat := by
  have hb : b.toNat < 2 ^ 1 := by simpa using b.isLt
  rw [BitVec.toNat_append, ← Nat.shiftLeft_add_eq_or_of_lt hb, Nat.shiftLeft_eq, Nat.pow_one]

/-- A 16-bit value widened to 96 bits keeps its `toNat` (used for shift amounts). -/
theorem setWidth96_toNat16 (y : BitVec 16) : (BitVec.setWidth 96 y).toNat = y.toNat := by
  rw [BitVec.toNat_setWidth,
      Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le y.isLt (Nat.pow_le_pow_right (by decide) (by omega)))]

/-- The sticky bit `(v & mask ≠ 0)` as a Nat: `1` iff any of `v`'s low `d` bits is set. -/
theorem sticky_ofBool_toNat (v : BitVec 96) (d : Nat) :
    (BitVec.ofBool ((v &&& ((1#96 <<< d) - 1#96)) != 0#96)).toNat
      = (if v.toNat % 2 ^ d ≠ 0 then 1 else 0) := by
  rw [BitVec.toNat_ofBool]
  by_cases h : v.toNat % 2 ^ d = 0
  · have hz : (v &&& ((1#96 <<< d) - 1#96)) = 0#96 :=
      BitVec.eq_of_toNat_eq (by rw [mask96_and_toNat, h]; rfl)
    simp [hz, h]
  · have hnz : (v &&& ((1#96 <<< d) - 1#96)) ≠ 0#96 := by
      intro hc
      exact h (by have := congrArg BitVec.toNat hc; rwa [mask96_and_toNat, BitVec.toNat_zero] at this)
    simp [hnz, h]

-- ── accessor defs (defeq to `arch_fma_mag98`'s inlined lets) ──────────────────

/-- LSB-exponent of an f32 significand (`exp − 150`, or the subnormal floor). -/
def fpEunb (x : BitVec 32) : BitVec 16 :=
  if (BitVec.ofBool (BitVec.extractLsb 30 23 x == BitVec.ofNat 8 0)) == BitVec.ofNat 1 1 then
    BitVec.ofNat 16 65387
  else BitVec.setWidth 16 (BitVec.extractLsb 30 23 x) - BitVec.ofNat 16 150

/-- 24-bit significand (implicit bit + fraction), normal or subnormal. -/
def fpMant24 (x : BitVec 32) : BitVec 24 :=
  if (BitVec.ofBool (BitVec.extractLsb 30 23 x == BitVec.ofNat 8 0)) == BitVec.ofNat 1 1 then
    (BitVec.ofNat 1 0) ++ BitVec.extractLsb 22 0 x
  else (BitVec.ofNat 1 1) ++ BitVec.extractLsb 22 0 x

/-- Selector: the product `a·b` is the higher operand (its LSB-exponent ≥ `c`'s). -/
def fmaSel98 (a b c : BitVec 32) : BitVec 1 :=
  BitVec.ofBool (BitVec.sle (fpEunb c) (fpEunb a + fpEunb b))

/-- The higher (less-shifted) significand, widened to 48 bits. -/
def fmaSigHi98 (a b c : BitVec 32) : BitVec 48 :=
  if fmaSel98 a b c == BitVec.ofNat 1 1 then
    BitVec.setWidth 48 (fpMant24 a) * BitVec.setWidth 48 (fpMant24 b)
  else BitVec.setWidth 48 (fpMant24 c)

/-- The lower (more-shifted) significand, widened to 48 bits. -/
def fmaSigLo98 (a b c : BitVec 32) : BitVec 48 :=
  if fmaSel98 a b c == BitVec.ofNat 1 1 then BitVec.setWidth 48 (fpMant24 c)
  else BitVec.setWidth 48 (fpMant24 a) * BitVec.setWidth 48 (fpMant24 b)

/-- The exponent gap between the two operands (`e_hi − e_lo`). -/
def fmaDiff98 (a b c : BitVec 32) : BitVec 16 :=
  (if fmaSel98 a b c == BitVec.ofNat 1 1 then fpEunb a + fpEunb b else fpEunb c)
    - (if fmaSel98 a b c == BitVec.ofNat 1 1 then fpEunb c else fpEunb a + fpEunb b)

/-- High significand at weight `2^49` (the aligned magnitude's high part). -/
def fmaHiNat (a b c : BitVec 32) : Nat := (fmaSigHi98 a b c).toNat * 2 ^ 49

/-- Low significand `/2^diff`, doubled for the guard bit, plus the sticky bit. -/
def fmaLoNat (a b c : BitVec 32) : Nat :=
  (fmaSigLo98 a b c).toNat * 2 ^ 48 / 2 ^ (fmaDiff98 a b c).toNat * 2
    + (if (fmaSigLo98 a b c).toNat * 2 ^ 48 % 2 ^ (fmaDiff98 a b c).toNat ≠ 0 then 1 else 0)

/-- **The same-sign sticky-fold magnitude as a Nat formula.** When the product
    and the addend have the same effective sign (`sign(a·b) = sign(c)`), the
    98-bit aligned magnitude is the GRS form: high significand at weight `2^49`,
    plus the shifted low significand (`/2^diff`) doubled to make room for the
    guard bit, plus the sticky bit (any bit dropped by the `>>diff`). -/
theorem fma_mag98_same_nat (a b c : BitVec 32)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b) :
    (arch_fma_mag98 a b c).toNat = fmaHiNat a b c + fmaLoNat a b c := by
  -- bounds (from the operand significand widths) that drop the final `% 2^98`
  have hHi : (fmaSigHi98 a b c).toNat * 2 ^ 49 < 2 ^ 97 :=
    Nat.lt_of_lt_of_le (mul_pow_lt (fmaSigHi98 a b c).isLt)
      (Nat.pow_le_pow_right (by decide) (by omega))
  have hLoX : (fmaSigLo98 a b c).toNat * 2 ^ 48 < 2 ^ 96 :=
    Nat.lt_of_lt_of_le (mul_pow_lt (fmaSigLo98 a b c).isLt)
      (Nat.pow_le_pow_right (by decide) (by omega))
  have hField : (fmaSigLo98 a b c).toNat * 2 ^ 48 / 2 ^ (fmaDiff98 a b c).toNat < 2 ^ 96 :=
    Nat.lt_of_le_of_lt (Nat.div_le_self _ _) hLoX
  have hstk : (if (fmaSigLo98 a b c).toNat * 2 ^ 48 % 2 ^ (fmaDiff98 a b c).toNat ≠ 0 then 1 else 0)
      ≤ 1 := by
    by_cases h : (fmaSigLo98 a b c).toNat * 2 ^ 48 % 2 ^ (fmaDiff98 a b c).toNat ≠ 0 <;> simp [h]
  have h9697 : (2 : Nat) ^ 97 = 2 ^ 96 * 2 := by rw [show (97 : Nat) = 96 + 1 from rfl, Nat.pow_succ]
  have h9798 : (2 : Nat) ^ 98 = 2 ^ 97 + 2 ^ 97 := by
    rw [show (98 : Nat) = 97 + 1 from rfl, Nat.pow_succ, Nat.mul_two]
  simp only [fmaHiNat, fmaLoNat, fmaSigHi98, fmaSigLo98, fmaDiff98, fmaSel98, fpEunb, fpMant24]
    at hHi hLoX hField hstk ⊢
  unfold arch_fma_mag98
  simp only [hsame, beq_self_eq_true, BitVec.ofBool_true, ite_self]
  rw [if_pos (by decide : ((1 : BitVec 1) == 1#1) = true),
      show (BitVec.setWidth 96 (48#16) : BitVec 96).toNat = 48 from by
        rw [setWidth96_toNat16]; rfl,
      setWidth96_toNat16, BitVec.toNat_add, hiField98_toNat, BitVec.toNat_setWidth,
      Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le (BitVec.isLt _)
        (Nat.pow_le_pow_right (by decide) (by omega : (97 : Nat) ≤ 98))),
      append_bit_toNat, loField96_toNat, sticky_ofBool_toNat, loExt96_toNat]
  -- `(SIG_HI·2^49 + (field·2 + sticky)) % 2^98 = SIG_HI·2^49 + field·2 + sticky`.
  -- Abstract the giant raw atoms into `H F S` so `omega` never traverses them.
  have key : ∀ H F S : Nat, H < 2 ^ 97 → F < 2 ^ 96 → S ≤ 1 → H + (F * 2 + S) < 2 ^ 98 := by
    intro H F S h1 h2 h3; omega
  rw [Nat.mod_eq_of_lt (key _ _ _ hHi hField hstk), ← Nat.add_assoc]

-- ── diff-sign branch (cancellation: `|hi_e − lo_e|`) ──────────────────────────

/-- The 97-bit high field `(sig_hi << 48) ++ 0` carries `sig_hi · 2^49`. -/
theorem hiField97_toNat (x : BitVec 48) :
    ((BitVec.setWidth 96 x <<< (48 : Nat)) ++ (0#1 : BitVec 1)).toNat = x.toNat * 2 ^ 49 := by
  rw [append_bit_toNat, loExt96_toNat, show (0#1 : BitVec 1).toNat = 0 from by decide, Nat.add_zero,
      Nat.mul_assoc, ← Nat.pow_succ]

/-- The 97-bit low composite (shifted field ++ sticky) as the GRS Nat value. -/
theorem loComposite97_toNat (x : BitVec 48) (d : Nat) :
    ((BitVec.setWidth 96 x <<< (48 : Nat)) >>> d ++
        BitVec.ofBool ((BitVec.setWidth 96 x <<< (48 : Nat)) &&& ((1#96 <<< d) - 1#96) != 0#96)).toNat
      = x.toNat * 2 ^ 48 / 2 ^ d * 2 + (if x.toNat * 2 ^ 48 % 2 ^ d ≠ 0 then 1 else 0) := by
  rw [append_bit_toNat, loField96_toNat, sticky_ofBool_toNat, loExt96_toNat]

/-- For a single bit, swapping the branches of a shared `if` flips nothing in the
    equality test: `(ite c X Y == ite c Y X) = (X == Y)`. -/
theorem ite_beq_ite_swap {c : Prop} [Decidable c] (X Y : BitVec 1) :
    ((if c then X else Y) == (if c then Y else X)) = (X == Y) := by
  by_cases h : c
  · rw [if_pos h, if_pos h]
  · rw [if_neg h, if_neg h]; revert X Y; decide

/-- The diff-sign magnitude: `setWidth 98 |X − Y|` is the Nat absolute difference. -/
theorem setWidth98_absdiff_toNat (X Y : BitVec 97) :
    (BitVec.setWidth 98 (if BitVec.ule Y X = true then X - Y else Y - X)).toNat
      = (if Y.toNat ≤ X.toNat then X.toNat - Y.toNat else Y.toNat - X.toNat) := by
  have hX : X.toNat < 2 ^ 97 := X.isLt
  have hY : Y.toNat < 2 ^ 97 := Y.isLt
  have hle98 : (2 : Nat) ^ 97 ≤ 2 ^ 98 := Nat.pow_le_pow_right (by decide) (by omega)
  rw [BitVec.toNat_setWidth]
  by_cases h : Y.toNat ≤ X.toNat
  · have hu : BitVec.ule Y X = true := by rw [BitVec.ule]; exact decide_eq_true h
    rw [if_pos hu, if_pos h, BitVec.toNat_sub,
        show 2 ^ 97 - Y.toNat + X.toNat = 2 ^ 97 + (X.toNat - Y.toNat) from by omega, Nat.add_mod_left,
        Nat.mod_eq_of_lt (by omega), Nat.mod_eq_of_lt (by omega)]
  · have hu : ¬ (BitVec.ule Y X = true) := by
      rw [BitVec.ule]; simp only [decide_eq_true_eq]; omega
    rw [if_neg hu, if_neg h, BitVec.toNat_sub,
        show 2 ^ 97 - X.toNat + Y.toNat = 2 ^ 97 + (Y.toNat - X.toNat) from by omega, Nat.add_mod_left,
        Nat.mod_eq_of_lt (by omega), Nat.mod_eq_of_lt (by omega)]

/-- **The diff-sign sticky-fold magnitude as a Nat formula.** When the product and
    the addend have opposing effective signs, the aligned magnitude is the absolute
    difference of the high part and the (GRS-folded) low part. -/
theorem fma_mag98_diff_nat (a b c : BitVec 32)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b == BitVec.extractLsb 31 31 c)
      = false) :
    (arch_fma_mag98 a b c).toNat
      = if fmaLoNat a b c ≤ fmaHiNat a b c then fmaHiNat a b c - fmaLoNat a b c
        else fmaLoNat a b c - fmaHiNat a b c := by
  simp only [fmaHiNat, fmaLoNat, fmaSigHi98, fmaSigLo98, fmaDiff98, fmaSel98, fpEunb, fpMant24]
  unfold arch_fma_mag98
  simp only [ofBool_beq_one, ite_beq_ite_swap, hdiff, Bool.false_eq_true, if_false]
  rw [show (BitVec.setWidth 96 (48#16) : BitVec 96).toNat = 48 from by rw [setWidth96_toNat16]; rfl,
      setWidth96_toNat16, setWidth98_absdiff_toNat, hiField97_toNat, loComposite97_toNat]

-- ── capstone: the sticky-fold fma rounds an explicit GRS magnitude ────────────

/-- **Same-sign capstone.** A finite non-cancelling same-sign fma is the RNE
    rounding of the explicit guard/round/sticky magnitude `fmaHiNat + fmaLoNat`
    (combining the proved rounding reduction with the magnitude characterization). -/
theorem arch_fma_f32_same_grs (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag98 a b c ≠ 0#98)
    (hsame : BitVec.extractLsb 31 31 c = BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b) :
    arch_fma_f32 a b c
      = roundNE_f32 (arch_fma_sign98 a b c == 1#1) (fmaHiNat a b c + fmaLoNat a b c)
          (arch_fma_elo98 a b c).toInt := by
  rw [arch_fma_f32_sticky_finite a b c ha hb hc hnc, fma_mag98_same_nat a b c hsame]

/-- **Diff-sign capstone.** A finite non-cancelling opposing-sign fma is the RNE
    rounding of the absolute-difference magnitude. -/
theorem arch_fma_f32_diff_grs (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true)
    (hnc : arch_fma_mag98 a b c ≠ 0#98)
    (hdiff : (BitVec.extractLsb 31 31 a ^^^ BitVec.extractLsb 31 31 b == BitVec.extractLsb 31 31 c)
      = false) :
    arch_fma_f32 a b c
      = roundNE_f32 (arch_fma_sign98 a b c == 1#1)
          (if fmaLoNat a b c ≤ fmaHiNat a b c then fmaHiNat a b c - fmaLoNat a b c
           else fmaLoNat a b c - fmaHiNat a b c)
          (arch_fma_elo98 a b c).toInt := by
  rw [arch_fma_f32_sticky_finite a b c ha hb hc hnc, fma_mag98_diff_nat a b c hdiff]

end ArchFp
