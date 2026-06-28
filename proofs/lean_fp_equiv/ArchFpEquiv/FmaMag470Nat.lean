import ArchFpEquiv.FmaMag98Nat
import Std.Tactic.BVDecide

/-!
# `arch_fma_mag` (reference, width 470) — Nat characterization

The exact-wide reference magnitude shifts the higher operand up by `diff` (no
fold, no sticky — 470 bits hold everything exactly for finite operands). So

  same sign:  mag470 = sig_hi · 2^diff + sig_lo
  diff sign:  mag470 = | sig_hi · 2^diff − sig_lo |

reusing the same `fmaSigHi98 / fmaSigLo98 / fmaDiff98` accessors as the
sticky-fold path. The `diff ≤ 421` bound (true for finite operands) keeps the
shifted product inside 470 bits.
-/

namespace ArchFp

set_option maxRecDepth 100000

/-- A ≤470-bit-safe shift: `setWidth 470 x << d = x · 2^d` when `n + d ≤ 470`. -/
theorem setWidth470_shift_toNat {n : Nat} (x : BitVec n) (d : Nat) (hn : n + d ≤ 470) :
    (BitVec.setWidth 470 x <<< d).toNat = x.toNat * 2 ^ d := by
  have hx : x.toNat < 2 ^ n := x.isLt
  have hb : x.toNat * 2 ^ d < 2 ^ 470 := by
    have h1 : x.toNat * 2 ^ d < 2 ^ n * 2 ^ d :=
      (Nat.mul_lt_mul_right (Nat.pow_pos (by decide : 0 < 2))).mpr hx
    have h2 : (2 : Nat) ^ n * 2 ^ d = 2 ^ (n + d) := by rw [← Nat.pow_add]
    exact Nat.lt_of_lt_of_le (h2 ▸ h1) (Nat.pow_le_pow_right (by decide) hn)
  rw [BitVec.toNat_shiftLeft, BitVec.toNat_setWidth, Nat.shiftLeft_eq,
      Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le hx (Nat.pow_le_pow_right (by decide) (by omega))),
      Nat.mod_eq_of_lt hb]

/-- `setWidth 470 x = x` at the Nat level (no shift). -/
theorem setWidth470_toNat {n : Nat} (x : BitVec n) (hn : n ≤ 470) :
    (BitVec.setWidth 470 x).toNat = x.toNat := by
  rw [BitVec.toNat_setWidth,
      Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le x.isLt (Nat.pow_le_pow_right (by decide) hn))]

/-- For finite operands the exponent gap is at most 421, so the reference's
    shifted operand stays inside 470 bits (the alignment is exact, no fold). -/
theorem fma_diff98_bound (a b c : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) (hc : finiteNonzero c = true) :
    (fmaDiff98 a b c).toNat ≤ 421 := by
  have h : BitVec.ule (fmaDiff98 a b c) (BitVec.ofNat 16 421) = true := by
    unfold finiteNonzero isNaN isInf isZero expField fracField fmaDiff98 fmaSel98 fpEunb at *
    bv_decide
  rw [BitVec.ule] at h
  simpa using of_decide_eq_true h

end ArchFp
