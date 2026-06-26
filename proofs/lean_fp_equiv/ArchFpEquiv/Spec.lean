import ArchFpEquiv.Model
import Std.Tactic.BVDecide

/-!
# Tier 2, part 1 — the multiply special-value lattice (machine-checked)

The Tier-2 goal is IEEE-754 correctness of the *arithmetic* operators. It splits
cleanly in two:

1. **Special values** — the NaN/Inf/Zero lattice (NaN propagation, `∞·0 = NaN`,
   `∞·x = ∞`, `0·x = 0`). These are pure bit-logic: under the governing
   hypothesis `bv_decide`'s preprocessing prunes the arithmetic `ite` branch, so
   the 24×24 multiplier is **never bit-blasted** and each law closes in well under
   a second. This file proves the complete multiply lattice.

2. **The finite product** — for two finite, nonzero operands the result is
   `round_to_nearest_even(a · b)`. Because `decode` exposes the exact value
   `mant · 2^eunb` and `f32_mul` forms `mp = mant_a · mant_b` (an *exact* 48-bit
   integer product) with `e0 = eunb_a + eunb_b`, the whole finite case reduces to
   *the shared rounder is correctly rounding*. That rounder crux is op-independent
   (mul/add/fma share `normround`) and is the one piece a bit-blaster cannot do;
   it is stated in `Equiv.lean` and needs the algebraic-lifting argument. This
   file records the reduction precisely (see `Reduction` section).

Notably, these special-value laws are exactly the corners the SMT backend could
*not* cover for `mul` — there `mul` is entirely on the §8.2 differential backstop.
Here they are machine-checked.

Profile: the generated `Model` uses the `riscv` `--fp-compat` profile, so the
canonical NaN is `0x7FC00000`.
-/

namespace ArchFp

/-- Canonical quiet NaN (riscv profile). -/
def cNaN : BitVec 32 := 0x7FC00000

-- ── decode predicates (mirror `decode`/`isnan`/`iszero` in src/fp_ops.rs) ──

/-- Biased exponent field `x[30:23]`. -/
def expField (x : BitVec 32) : BitVec 8 := x.extractLsb' 23 8
/-- Trailing significand field `x[22:0]`. -/
def fracField (x : BitVec 32) : BitVec 23 := x.extractLsb' 0 23
/-- Sign bit `x[31]`. -/
def sgn (x : BitVec 32) : BitVec 1 := x.extractLsb' 31 1

def isNaN  (x : BitVec 32) : Bool := (expField x == 255#8) && (fracField x != 0#23)
def isInf  (x : BitVec 32) : Bool := (expField x == 255#8) && (fracField x == 0#23)
def isZero (x : BitVec 32) : Bool := x.extractLsb' 0 31 == 0#31

/-- Result sign of a product: `sign a ⊕ sign b`. -/
def mulSign (a b : BitVec 32) : BitVec 1 := sgn a ^^^ sgn b

/-- Signed infinity with the product's sign: `sy ++ 0xFF ++ 0`. -/
def infOf (a b : BitVec 32) : BitVec 32 :=
  ((BitVec.setWidth 32 (mulSign a b)) <<< 31) ||| 0x7F800000#32
/-- Signed zero with the product's sign: `sy ++ 0…0`. -/
def zeroOf (a b : BitVec 32) : BitVec 32 :=
  (BitVec.setWidth 32 (mulSign a b)) <<< 31

-- ── the multiply special-value lattice ──────────────────────────────────────
-- Each `unfold … ; bv_decide`; the governing hypothesis prunes the rounder/
-- multiplier branch so nothing heavy is bit-blasted.

/-- NaN in the first operand propagates to the canonical NaN. -/
theorem mul_nan_left (a b : BitVec 32) (h : isNaN a = true) :
    arch_f32_mul a b = cNaN := by
  unfold isNaN expField fracField cNaN at *
  unfold arch_f32_mul
  bv_decide

/-- NaN in the second operand propagates to the canonical NaN. -/
theorem mul_nan_right (a b : BitVec 32) (h : isNaN b = true) :
    arch_f32_mul a b = cNaN := by
  unfold isNaN expField fracField cNaN at *
  unfold arch_f32_mul
  bv_decide

/-- `∞ · 0 = NaN` (invalid operation). -/
theorem mul_inf_zero (a b : BitVec 32) (ha : isInf a = true) (hb : isZero b = true) :
    arch_f32_mul a b = cNaN := by
  unfold isInf isZero expField fracField cNaN at *
  unfold arch_f32_mul
  bv_decide

/-- `0 · ∞ = NaN` (invalid operation). -/
theorem mul_zero_inf (a b : BitVec 32) (ha : isZero a = true) (hb : isInf b = true) :
    arch_f32_mul a b = cNaN := by
  unfold isInf isZero expField fracField cNaN at *
  unfold arch_f32_mul
  bv_decide

/-- `∞ · x = ±∞` for `x` neither NaN nor zero. -/
theorem mul_inf_left (a b : BitVec 32)
    (ha : isInf a = true) (hn : isNaN b = false) (hz : isZero b = false) :
    arch_f32_mul a b = infOf a b := by
  unfold isInf isNaN isZero expField fracField infOf mulSign sgn at *
  unfold arch_f32_mul
  bv_decide

/-- `x · ∞ = ±∞` for `x` neither NaN nor zero. -/
theorem mul_inf_right (a b : BitVec 32)
    (hb : isInf b = true) (hn : isNaN a = false) (hz : isZero a = false) :
    arch_f32_mul a b = infOf a b := by
  unfold isInf isNaN isZero expField fracField infOf mulSign sgn at *
  unfold arch_f32_mul
  bv_decide

/-- `0 · x = ±0` for finite `x` (neither NaN nor ∞). -/
theorem mul_zero_left (a b : BitVec 32)
    (ha : isZero a = true) (hn : isNaN b = false) (hi : isInf b = false) :
    arch_f32_mul a b = zeroOf a b := by
  unfold isZero isNaN isInf expField fracField zeroOf mulSign sgn at *
  unfold arch_f32_mul
  bv_decide

/-- `x · 0 = ±0` for finite `x` (neither NaN nor ∞). -/
theorem mul_zero_right (a b : BitVec 32)
    (hb : isZero b = true) (hn : isNaN a = false) (hi : isInf a = false) :
    arch_f32_mul a b = zeroOf a b := by
  unfold isZero isNaN isInf expField fracField zeroOf mulSign sgn at *
  unfold arch_f32_mul
  bv_decide

-- ── Reduction of the finite product to the shared rounder ────────────────────

/-- A finite, nonzero f32: not NaN, not ∞, not zero. -/
def finiteNonzero (x : BitVec 32) : Bool := !isNaN x && !isInf x && !isZero x

/-- The model's finite multiply, written with the exposed pieces:
    round-to-nearest-even of the **exact** significand product
    `mant_a · mant_b` (48-bit, no truncation) scaled by `eunb_a + eunb_b`.
    `arch_decode_mant`/`arch_decode_eunb`/`arch_round48` are the very functions
    `f32_mul` inlines (generated from one IR), so this is `f32_mul` on the finite
    path by construction — `mul_finite_reduces` proves exactly that. -/
def archMulFinite (a b : BitVec 32) : BitVec 32 :=
  arch_round48 (mulSign a b)
    (BitVec.setWidth 48 (arch_decode_mant a) * BitVec.setWidth 48 (arch_decode_mant b))
    (arch_decode_eunb a + arch_decode_eunb b)

/-- **The finite-product reduction.** For two finite nonzero operands, the model's
    multiply *is* the shared rounder applied to the exact integer significand
    product and the summed exponents. Proved by `bv_decide`: the 24×24 multiplier
    occurs identically on both sides (`f32_mul` inlines it; `archMulFinite` names
    it through `arch_decode_mant`), so this is a structural identity, **not** a
    multiplier-equivalence (which would be SAT-hard). It collapses all of Tier-2
    multiply, save the special values above, onto one question: does `arch_round48`
    round correctly? That is the op-independent rounder crux (`Equiv.lean`), the
    sole place algebraic lifting is still required. -/
theorem mul_finite_reduces (a b : BitVec 32)
    (ha : finiteNonzero a = true) (hb : finiteNonzero b = true) :
    arch_f32_mul a b = archMulFinite a b := by
  unfold finiteNonzero isNaN isInf isZero expField fracField at *
  unfold archMulFinite arch_round48 arch_decode_mant arch_decode_eunb mulSign sgn arch_f32_mul
  bv_decide (config := { timeout := 120 })

end ArchFp
