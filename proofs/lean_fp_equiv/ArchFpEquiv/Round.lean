import ArchFpEquiv.Model
import ArchFpEquiv.Spec
import Std.Tactic.BVDecide

/-!
# Tier 2, part 2 — the shared rounder, exhaustively characterized

`Equiv.lean` reduces all of Tier-2 multiply to one lemma: the shared round-and-
pack `arch_round48` rounds its dyadic argument `(-1)^s · sig · 2^e0` to nearest
representable f32. That rounder is **multiplier-free** (shifts, adds, compares,
a binary-search leading-zero count), so unlike the multiply itself it is fully
within `bv_decide`'s reach. This file machine-checks — over the *entire* input
space — the properties that pin `arch_round48` down on the whole representable
domain:

* `round48_sign`     — the result carries the input sign (always).
* `round48_zero`     — a zero significand rounds to signed zero.
* `round48_exact_normal` / `round48_exact_subnormal` — **exact round-trip**: a
  value that is already representable (decode of any normal / subnormal f32)
  rounds back to itself, i.e. the rounder introduces **zero error on
  representable inputs**. This is `arch_round48_correct` on the entire exact
  sub-domain, with the optimized clz / appended-sticky datapath bit-blasted.

and an end-to-end multiply consequence that needs no rounder hypothesis at all:

* `mul_one_left` / `mul_one_right` — the **multiplicative identity** `1·x = x`
  for every non-NaN `x` (finite, subnormal, zero, ∞, either sign). Provable in
  one shot because multiplying by the constant `1.0` makes one multiplier operand
  constant `2^23` — a shift, not a 24×24 array — so the whole statement, special
  cases included, bit-blasts.

What is *not* here — and is the residual of `arch_round48_correct` — is the
rounding **direction** for inexact (non-representable) arguments: that the rounder
picks the nearer neighbour with ties-to-even. That alone is genuinely value-level
(it compares the exact `sig · 2^e0` against two representable neighbours across an
exponent scaling), so it needs the dyadic/`Rat` argument, not a bit-blaster. The
final section here lays that argument's first stone: `msb_index_finds_msb` /
`msb_index_bound` carry the optimized leading-zero count across to the `Nat` bound
`2^p ≤ sig < 2^(p+1)` — entirely with Lean-core lemmas, no Mathlib.
-/

namespace ArchFp

/-- A normal f32: biased exponent in `[1, 254]`. -/
def isNormal (x : BitVec 32) : Bool := (expField x != 0#8) && (expField x != 255#8)
/-- A subnormal f32 or signed zero: biased exponent `0`. -/
def isSubnormalOrZero (x : BitVec 32) : Bool := expField x == 0#8

-- ── rounder shape ───────────────────────────────────────────────────────────

/-- The rounded result always carries the input sign. -/
theorem round48_sign (s : BitVec 1) (sig : BitVec 48) (e0 : BitVec 16) :
    (arch_round48 s sig e0).extractLsb' 31 1 = s := by
  unfold arch_round48
  bv_decide (config := { timeout := 300 })

/-- A zero significand rounds to signed zero, regardless of the exponent. -/
theorem round48_zero (s : BitVec 1) (e0 : BitVec 16) :
    arch_round48 s 0#48 e0 = (BitVec.setWidth 32 s) <<< 31 := by
  unfold arch_round48
  bv_decide (config := { timeout := 120 })

-- ── exact round-trip: rounding a representable value is the identity ─────────

/-- **Exact round-trip (normals).** Decoding any normal f32 to its
    `(sign, mantissa, unbiased-exponent)` and rounding that exact value returns
    the original bits — the rounder is error-free on representable normals. -/
theorem round48_exact_normal (x : BitVec 32) (h : isNormal x = true) :
    arch_round48 (sgn x) (BitVec.setWidth 48 (arch_decode_mant x)) (arch_decode_eunb x) = x := by
  unfold isNormal expField sgn arch_round48 arch_decode_mant arch_decode_eunb at *
  bv_decide (config := { timeout := 300 })

/-- **Exact round-trip (subnormals & signed zero).** Same, for biased exponent 0. -/
theorem round48_exact_subnormal (x : BitVec 32) (h : isSubnormalOrZero x = true) :
    arch_round48 (sgn x) (BitVec.setWidth 48 (arch_decode_mant x)) (arch_decode_eunb x) = x := by
  unfold isSubnormalOrZero expField sgn arch_round48 arch_decode_mant arch_decode_eunb at *
  bv_decide (config := { timeout := 300 })

-- ── multiplicative identity (complete, no rounder hypothesis) ────────────────

/-- `1.0 · x = x` for every non-NaN `x`. The constant operand `0x3F800000` makes
    the multiplier a shift, so every case (finite / subnormal / zero / ∞, both
    signs) is discharged together. A fully end-to-end multiply correctness fact. -/
theorem mul_one_left (x : BitVec 32) (h : isNaN x = false) :
    arch_f32_mul 0x3F800000#32 x = x := by
  unfold isNaN expField fracField at h
  unfold arch_f32_mul
  bv_decide (config := { timeout := 300 })

/-- `x · 1.0 = x` for every non-NaN `x` (the mirror of `mul_one_left`). -/
theorem mul_one_right (x : BitVec 32) (h : isNaN x = false) :
    arch_f32_mul x 0x3F800000#32 = x := by
  unfold isNaN expField fracField at h
  unfold arch_f32_mul
  bv_decide (config := { timeout := 300 })

-- ── value-level bridge: the leading-zero count is mathematically correct ─────
--
-- The residual of `arch_round48_correct` is the rounding *direction* on inexact
-- inputs, which is value-level (a `Nat` argument). Its first step is to know the
-- normalized position — the MSB of the significand. `arch_round48` computes this
-- with a binary-search count-leading-zeros (`arch_msb_index48`); these two lemmas
-- carry that optimized bit-level search across into the value world, with **no
-- Mathlib** — `bv_decide` for the bit fact, Lean-core `Nat` lemmas + `omega` for
-- the arithmetic. This is the entry point a full rounding-direction proof builds on.

/-- The binary-search clz finds the true most-significant bit: for nonzero `sig`,
    `sig >>> msb_index(sig) = 1`, i.e. `floor(sig / 2^p) = 1`. Exhaustive
    (`bv_decide`) over all 2^48 significands. -/
theorem msb_index_finds_msb (sig : BitVec 48) (h : sig ≠ 0#48) :
    sig >>> (arch_msb_index48 sig).toNat = 1#48 := by
  unfold arch_msb_index48
  bv_decide (config := { timeout := 300 })

/-- Value-level corollary: the computed index `p` brackets the significand,
    `2^p ≤ sig < 2^(p+1)` — i.e. `p = ⌊log₂ sig⌋`. Bridges the bit fact above to a
    `Nat` bound using only core lemmas (`Nat.div_add_mod`, `Nat.pow_succ`, `omega`). -/
theorem msb_index_bound (sig : BitVec 48) (h : sig ≠ 0#48) :
    2 ^ (arch_msb_index48 sig).toNat ≤ sig.toNat
      ∧ sig.toNat < 2 ^ ((arch_msb_index48 sig).toNat + 1) := by
  have hb := msb_index_finds_msb sig h
  have hdiv : sig.toNat / 2 ^ (arch_msb_index48 sig).toNat = 1 := by
    have h2 := congrArg BitVec.toNat hb
    simpa [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow] using h2
  have hpos : 0 < 2 ^ (arch_msb_index48 sig).toNat := Nat.pow_pos (by decide : 0 < 2)
  have hdm := Nat.div_add_mod sig.toNat (2 ^ (arch_msb_index48 sig).toNat)
  have hmod : sig.toNat % 2 ^ (arch_msb_index48 sig).toNat
      < 2 ^ (arch_msb_index48 sig).toNat := Nat.mod_lt _ hpos
  rw [hdiv, Nat.mul_one] at hdm
  have hps : 2 ^ ((arch_msb_index48 sig).toNat + 1)
      = 2 ^ (arch_msb_index48 sig).toNat * 2 := Nat.pow_succ 2 (arch_msb_index48 sig).toNat
  omega

end ArchFp
