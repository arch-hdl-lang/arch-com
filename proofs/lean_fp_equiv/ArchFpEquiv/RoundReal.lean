import ArchFpEquiv.RoundProof
import ArchFpEquiv.RneValue

/-!
# R2 — `roundNE_f32` against a value-level (dyadic real) specification

`roundNE_f32` (RoundProof.lean) is the IEEE-754 RNE spec the hardware rounder
is proved against — but until this file its *skeleton* (binade selection by
`log2`, the subnormal floor at −149, carry, overflow) was trusted by
inspection. Here we state what "IS round-to-nearest-even" means at the level
of values and prove `roundNE_f32` satisfies it.

**Formulation (dyadic, ℚ-free).** Every quantity in play is a dyadic
rational, so nearest/tie comparisons can be carried out exactly in `Nat`
after scaling: the input magnitude `sig · 2^e0` and every finite-f32
magnitude `f32MagScaled z · 2^{-149}` are brought to the common grid
`2^{min(e0, -149)}` by

    input  ↦ sig · 2^u        (u = max 0 (e0 + 149))
    f32 z  ↦ f32MagScaled z · 2^t   (t = max 0 (-(e0 + 149)))

`f32MagScaled z` is the standard IEEE-754 magnitude of a finite pattern in
units of `2^{-149}` (subnormal: the fraction; normal: `(2^23 + M) · 2^{E-1}`).
A reviewer checks these two definitions; everything else is proved.

This file: the spec (`scaledDist`, `f32MagScaled`, `IsFiniteF32`,
`IsNearestMag`) and the full correctness theorems:

* `roundNE_subnormal_nearest` — subnormal path (rounding grid = representable
  grid, R1 transfers directly);
* `roundNE_normal_nearest` — normal path (representables at/above the binade
  sit on the rounding grid via `f32MagScaled_gap`; those below are ≥ half an
  ulp away, beaten by `rneQuot_halfulp`), including the carry to the next
  binade;
* `roundNE_nearest` — the assembled statement: for `sig ≠ 0` in range,
  `roundNE_f32` is finite and nearest among ALL finite f32 patterns;
* `roundNE_overflow` / `biasedFinal_overflow_iff` — out of range the result
  is signed ∞, and the range bound is exactly the IEEE criterion (rounded
  magnitude reaches `2^128`);
* `roundNE_tie_even` — half-way inputs encode an even fraction (R1's
  `rneQuot_tie_even` transferred through the encoder; carry stores 0).
-/

namespace ArchFp

/-- Finite (non-NaN, non-Inf) f32 pattern. -/
def IsFiniteF32 (z : BitVec 32) : Prop := expField z ≠ 255#8

/-- Magnitude of a finite f32 pattern in units of `2^{-149}`:
    subnormal (`E = 0`): the 23-bit fraction; normal: `(2^23 + M)·2^{E-1}`. -/
def f32MagScaled (z : BitVec 32) : Nat :=
  if expField z = 0#8 then (fracField z).toNat
  else (2 ^ 23 + (fracField z).toNat) * 2 ^ ((expField z).toNat - 1)

/-- `Nat` absolute difference. -/
def scaledDist (a b : Nat) : Nat := (a - b) + (b - a)

/-- The input-side scale exponent: `sig · 2^e0 = (sig · 2^u) · 2^{min(e0,-149)}`. -/
def uScale (e0 : Int) : Nat := (e0 + 149).toNat

/-- The representable-side scale exponent:
    `2^{-149} = 2^t · 2^{min(e0,-149)}`. -/
def tScale (e0 : Int) : Nat := (-(e0 + 149)).toNat

/-- **The specification.** `y` is a round-to-nearest of `sig · 2^e0` (in
    magnitude) if no finite pattern is strictly closer on the common grid. -/
def IsNearestMag (sig : Nat) (e0 : Int) (y : BitVec 32) : Prop :=
  ∀ z : BitVec 32, IsFiniteF32 z →
    scaledDist (f32MagScaled y * 2 ^ tScale e0) (sig * 2 ^ uScale e0)
      ≤ scaledDist (f32MagScaled z * 2 ^ tScale e0) (sig * 2 ^ uScale e0)

-- ── basic facts about the encodings ─────────────────────────────────────────

/-- Every natural `v ≤ 2^23` is the scaled magnitude of a finite pattern:
    `v < 2^23` as the subnormal with fraction `v`, and `v = 2^23` as the
    smallest normal (`E = 1, M = 0`). This is the value-side content of the
    encode used by `roundNE_f32`'s subnormal branch. -/
theorem f32MagScaled_encode_small (sgnN v : Nat) (hs : sgnN = 0 ∨ sgnN = 2 ^ 31)
    (hv : v ≤ 2 ^ 23) :
    f32MagScaled (BitVec.ofNat 32 (sgnN + v % 2 ^ 31)) = v ∧
      IsFiniteF32 (BitVec.ofNat 32 (sgnN + v % 2 ^ 31)) := by
  have hvm : v % 2 ^ 31 = v := Nat.mod_eq_of_lt (by omega)
  rw [hvm]
  have hlt : sgnN + v < 2 ^ 32 := by rcases hs with h | h <;> omega
  -- compute the fields by Nat arithmetic on toNat
  have hexp : (expField (BitVec.ofNat 32 (sgnN + v))).toNat
      = (sgnN + v) / 2 ^ 23 % 2 ^ 8 := by
    simp [expField, BitVec.extractLsb'_toNat, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt hlt, Nat.shiftRight_eq_div_pow]
  have hfrac : (fracField (BitVec.ofNat 32 (sgnN + v))).toNat
      = (sgnN + v) % 2 ^ 23 := by
    simp [fracField, BitVec.extractLsb'_toNat, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt hlt, Nat.shiftRight_eq_div_pow]
  rcases Nat.lt_or_ge v (2 ^ 23) with hvlt | hveq
  · -- subnormal: exponent field 0, fraction v
    have he0 : (sgnN + v) / 2 ^ 23 % 2 ^ 8 = 0 := by
      rcases hs with h | h
      · subst h; simp; omega
      · subst h
        have : (2 ^ 31 + v) / 2 ^ 23 = 2 ^ 8 := by omega
        rw [this]
    have hf : (sgnN + v) % 2 ^ 23 = v := by
      rcases hs with h | h
      · subst h; simp; omega
      · subst h
        have h1 : (2 ^ 31 + v) % 2 ^ 23 = v % 2 ^ 23 := by
          have : (2 ^ 31 : Nat) = 2 ^ 8 * 2 ^ 23 := by decide
          omega
        rw [h1]; omega
    constructor
    · rw [f32MagScaled, if_pos, hfrac, hf]
      · have : expField (BitVec.ofNat 32 (sgnN + v)) = 0#8 := by
          apply BitVec.eq_of_toNat_eq; rw [hexp, he0]; rfl
        exact this
    · rw [IsFiniteF32]
      intro hcon
      have := congrArg BitVec.toNat hcon
      rw [hexp, he0] at this
      simp at this
  · -- boundary v = 2^23: encodes the smallest normal, value 2^23 · 2^0
    have hv23 : v = 2 ^ 23 := by omega
    subst hv23
    have he1 : (sgnN + 2 ^ 23) / 2 ^ 23 % 2 ^ 8 = 1 := by
      rcases hs with h | h
      · subst h; simp
      · subst h
        have : (2 ^ 31 + 2 ^ 23) / 2 ^ 23 = 2 ^ 8 + 1 := by omega
        rw [this]
    have hf : (sgnN + 2 ^ 23) % 2 ^ 23 = 0 := by
      rcases hs with h | h
      · subst h; simp
      · subst h
        have : (2 ^ 31 + 2 ^ 23 : Nat) = (2 ^ 8 + 1) * 2 ^ 23 := by decide
        rw [this]
    have hexpne : expField (BitVec.ofNat 32 (sgnN + 2 ^ 23)) ≠ 0#8 := by
      intro hcon
      have := congrArg BitVec.toNat hcon
      rw [hexp, he1] at this
      simp at this
    constructor
    · rw [f32MagScaled, if_neg hexpne, hfrac, hf, hexp, he1]
    · rw [IsFiniteF32]
      intro hcon
      have := congrArg BitVec.toNat hcon
      rw [hexp, he1] at this
      simp at this

/-- Every finite pattern's scaled magnitude is a multiple of `1` (trivially) —
    and, crucially for the subnormal case, the magnitudes of ALL finite
    patterns are `Nat`s, i.e. multiples of the subnormal grid step. This
    lemma packages the fact used there: the scaled magnitude is `m · 1` for
    `m = f32MagScaled z`. (Stated for symmetry with the binade lemmas to
    come; the content is that no finite value falls strictly between
    consecutive multiples of `2^{-149}`.) -/
theorem f32MagScaled_on_grid (z : BitVec 32) : ∃ m : Nat, f32MagScaled z = m :=
  ⟨_, rfl⟩

-- ── the subnormal path ───────────────────────────────────────────────────────

/-- **Subnormal-path correctness.** When the value lands at or below the
    subnormal/normal boundary (`log2 sig + e0 + 127 ≤ 0`), `roundNE_f32`
    produces a finite pattern that is nearest to `sig · 2^e0` among ALL
    finite f32 patterns. The key structural fact: in this range the rounding
    grid (`2^{-149}`) IS the representable grid, so R1's `rneQuot_nearest`
    transfers with no binade bookkeeping. -/
theorem roundNE_subnormal_nearest (neg : Bool) (sig : Nat) (e0 : Int)
    (hsig : sig ≠ 0) (hbias : (Nat.log2 sig : Int) + e0 + 127 ≤ 0) :
    IsFiniteF32 (roundNE_f32 neg sig e0)
      ∧ IsNearestMag sig e0 (roundNE_f32 neg sig e0) := by
  have hlo := Nat.log2_self_le hsig
  have hhi := Nat.lt_log2_self (n := sig)
  have hsgn : (if neg then 2 ^ 31 else 0 : Nat) = 0
      ∨ (if neg then 2 ^ 31 else 0 : Nat) = 2 ^ 31 := by
    split <;> simp
  by_cases he : (-149 : Int) ≤ e0
  · -- exact-shift regime: e0 ≥ -149, sh ≤ 0, kept = sig · 2^(e0+149)
    have hsh : (-149 : Int) - e0 ≤ 0 := by omega
    have hux : (-((-149 : Int) - e0)).toNat = uScale e0 := by
      rw [uScale]; omega
    have hy : roundNE_f32 neg sig e0
        = BitVec.ofNat 32 ((if neg then 2 ^ 31 else 0)
            + (sig * 2 ^ uScale e0) % 2 ^ 31) := by
      rw [roundNE_f32, if_neg hsig]
      simp only [if_pos hbias, if_pos hsh, hux]
    -- kept ≤ 2^23 from the log2 bracket
    have hexp : Nat.log2 sig + 1 + uScale e0 ≤ 23 := by
      have : (uScale e0 : Int) = e0 + 149 := by rw [uScale]; omega
      omega
    have hkb : sig * 2 ^ uScale e0 ≤ 2 ^ 23 := by
      have h1 : sig * 2 ^ uScale e0 ≤ 2 ^ (Nat.log2 sig + 1) * 2 ^ uScale e0 :=
        Nat.mul_le_mul_right _ (Nat.le_of_lt hhi)
      have h2 : 2 ^ (Nat.log2 sig + 1) * 2 ^ uScale e0
          = 2 ^ (Nat.log2 sig + 1 + uScale e0) := by rw [← Nat.pow_add]
      have h3 : (2 : Nat) ^ (Nat.log2 sig + 1 + uScale e0) ≤ 2 ^ 23 :=
        Nat.pow_le_pow_right (by decide) hexp
      omega
    obtain ⟨hval, hfin⟩ :=
      f32MagScaled_encode_small (if neg then 2 ^ 31 else 0)
        (sig * 2 ^ uScale e0) hsgn hkb
    rw [hy]
    refine ⟨hfin, ?_⟩
    intro z hz
    rw [hval]
    have ht0 : tScale e0 = 0 := by rw [tScale]; omega
    rw [ht0]
    simp [scaledDist]
  · -- rounded regime: e0 < -149, sh ≥ 1, kept = rneQuot sig (tScale e0)
    have hsh : ¬ ((-149 : Int) - e0 ≤ 0) := by omega
    have htx : ((-149 : Int) - e0).toNat = tScale e0 := by
      rw [tScale]; omega
    have hy : roundNE_f32 neg sig e0
        = BitVec.ofNat 32 ((if neg then 2 ^ 31 else 0)
            + rneQuot sig (tScale e0) % 2 ^ 31) := by
      rw [roundNE_f32, if_neg hsig]
      simp only [if_pos hbias, if_neg hsh, htx]
    have hts : 1 ≤ tScale e0 := by rw [tScale]; omega
    -- kept ≤ 2^23: rneQuot ≤ floor + 1 and the floor is < 2^23
    have hq : rneQuot sig (tScale e0) ≤ sig / 2 ^ tScale e0 + 1 := by
      rw [rneQuot]; split <;> omega
    have hdiv : sig / 2 ^ tScale e0 < 2 ^ 23 := by
      apply Nat.div_lt_of_lt_mul
      have hle : Nat.log2 sig + 1 ≤ 23 + tScale e0 := by
        have h : (tScale e0 : Int) = -(e0 + 149) := by rw [tScale]; omega
        omega
      have h1 : (2 : Nat) ^ (Nat.log2 sig + 1) ≤ 2 ^ (23 + tScale e0) :=
        Nat.pow_le_pow_right (by decide) hle
      have h2 : (2 : Nat) ^ (23 + tScale e0) = 2 ^ 23 * 2 ^ tScale e0 := by
        rw [← Nat.pow_add]
      omega
    have hkb : rneQuot sig (tScale e0) ≤ 2 ^ 23 := by omega
    obtain ⟨hval, hfin⟩ :=
      f32MagScaled_encode_small (if neg then 2 ^ 31 else 0)
        (rneQuot sig (tScale e0)) hsgn hkb
    rw [hy]
    refine ⟨hfin, ?_⟩
    intro z hz
    rw [hval]
    have hu0 : uScale e0 = 0 := by rw [uScale]; omega
    have hn := rneQuot_nearest sig (tScale e0) hts (f32MagScaled z)
    rw [gridDist, gridDist] at hn
    rw [hu0]
    simpa [scaledDist] using hn

-- ── the normal path: helper definitions ─────────────────────────────────────

/-- The normal-path rounded significand. Independent of `e0`: on the normal
    path `roundNE_f32`'s shift is `k - e0 = (p + e0 - 23) - e0 = p - 23`,
    a function of `sig` alone (`p = log2 sig`). -/
def keptNorm (sig : Nat) : Nat :=
  if Nat.log2 sig ≤ 23 then sig * 2 ^ (23 - Nat.log2 sig)
  else rneQuot sig (Nat.log2 sig - 23)

/-- The post-carry biased exponent the normal path will encode. Overflow to
    `∞` happens exactly when this exceeds 254 (proved below to coincide with
    the IEEE criterion: the correctly-rounded magnitude reaches `2^128`). -/
def biasedFinal (sig : Nat) (e0 : Int) : Int :=
  ((Nat.log2 sig : Int) + e0 + 127) + (if 2 ^ 24 ≤ keptNorm sig then 1 else 0)

/-- `scaledDist` is homogeneous: scaling both points scales the distance. -/
theorem scaledDist_scale (a b c : Nat) :
    scaledDist (a * c) (b * c) = scaledDist a b * c := by
  rw [scaledDist, scaledDist, Nat.add_mul, Nat.sub_mul, Nat.sub_mul]

/-- `gridDist` (R1's distance) is `scaledDist` against the grid point. -/
theorem gridDist_eq_scaledDist (n m sh : Nat) :
    gridDist n m sh = scaledDist (m * 2 ^ sh) n := rfl

-- ── the binade structure of the representables ──────────────────────────────

/-- **Binade gap.** Against the rounding grid `2^K` (scaled), every finite
    pattern's magnitude either lies ON the grid (all patterns at or above the
    binade `2^{K+23}` — coarser binades are sub-grids), or sits at least half
    a grid step BELOW the binade start. The largest representable below
    `2^{K+23}` is `(2^24 - 1)·2^{K-1} = 2^{K+23} - 2^{K-1}` — the top of the
    previous binade — so nothing representable lands strictly inside the last
    half-step below the binade. -/
theorem f32MagScaled_gap (K : Nat) (z : BitVec 32) :
    (∃ m, f32MagScaled z = m * 2 ^ K)
      ∨ (1 ≤ K ∧ f32MagScaled z + 2 ^ (K - 1) ≤ 2 ^ (K + 23)) := by
  by_cases hK : K = 0
  · subst hK
    exact Or.inl ⟨f32MagScaled z, by simp⟩
  · have hK1 : 1 ≤ K := by omega
    rw [f32MagScaled]
    by_cases he : expField z = 0#8
    · rw [if_pos he]
      right
      refine ⟨hK1, ?_⟩
      have hf : (fracField z).toNat < 2 ^ 23 := (fracField z).isLt
      have hy : (2 : Nat) ^ (K + 23) = 2 ^ 24 * 2 ^ (K - 1) := by
        rw [← Nat.pow_add]; congr 1; omega
      have hy1 : 0 < 2 ^ (K - 1) := Nat.pow_pos (by decide)
      omega
    · rw [if_neg he]
      by_cases hEK : K ≤ (expField z).toNat - 1
      · left
        refine ⟨(2 ^ 23 + (fracField z).toNat)
            * 2 ^ ((expField z).toNat - 1 - K), ?_⟩
        rw [Nat.mul_assoc, ← Nat.pow_add]
        congr 2
        omega
      · right
        refine ⟨hK1, ?_⟩
        have hf : (fracField z).toNat < 2 ^ 23 := (fracField z).isLt
        have hEle : (2 : Nat) ^ ((expField z).toNat - 1) ≤ 2 ^ (K - 1) :=
          Nat.pow_le_pow_right (by decide) (by omega)
        have h1 : (2 ^ 23 + (fracField z).toNat) * 2 ^ ((expField z).toNat - 1)
            ≤ (2 ^ 23 + (fracField z).toNat) * 2 ^ (K - 1) :=
          Nat.mul_le_mul (Nat.le_refl _) hEle
        have h3 : (2 ^ 23 + (fracField z).toNat + 1) * 2 ^ (K - 1)
            ≤ 2 ^ 24 * 2 ^ (K - 1) :=
          Nat.mul_le_mul (by omega) (Nat.le_refl _)
        rw [Nat.add_mul] at h3
        have h4 : (2 : Nat) ^ 24 * 2 ^ (K - 1) = 2 ^ (K + 23) := by
          rw [← Nat.pow_add]; congr 1; omega
        omega

/-- The normal-path significand is a genuine 24-bit-plus-round value:
    `2^23 ≤ keptNorm sig ≤ 2^24` (for `sig ≠ 0`). -/
theorem keptNorm_bounds (sig : Nat) (hsig : sig ≠ 0) :
    2 ^ 23 ≤ keptNorm sig ∧ keptNorm sig ≤ 2 ^ 24 := by
  have hlo := Nat.log2_self_le hsig
  have hhi := Nat.lt_log2_self (n := sig)
  rw [keptNorm]
  by_cases hp : Nat.log2 sig ≤ 23
  · rw [if_pos hp]
    have h1 : (2 : Nat) ^ Nat.log2 sig * 2 ^ (23 - Nat.log2 sig) = 2 ^ 23 := by
      rw [← Nat.pow_add]; congr 1; omega
    have h2 : (2 : Nat) ^ Nat.log2 sig * 2 ^ (23 - Nat.log2 sig)
        ≤ sig * 2 ^ (23 - Nat.log2 sig) :=
      Nat.mul_le_mul_right _ hlo
    have h3 : sig * 2 ^ (23 - Nat.log2 sig)
        ≤ 2 ^ (Nat.log2 sig + 1) * 2 ^ (23 - Nat.log2 sig) :=
      Nat.mul_le_mul_right _ (Nat.le_of_lt hhi)
    have h4 : (2 : Nat) ^ (Nat.log2 sig + 1) * 2 ^ (23 - Nat.log2 sig)
        = 2 ^ 24 := by
      rw [← Nat.pow_add]; congr 1; omega
    omega
  · rw [if_neg hp]
    have hq : sig / 2 ^ (Nat.log2 sig - 23) ≤ rneQuot sig (Nat.log2 sig - 23)
        ∧ rneQuot sig (Nat.log2 sig - 23) ≤ sig / 2 ^ (Nat.log2 sig - 23) + 1 := by
      rw [rneQuot]; split <;> omega
    have hdm := Nat.div_add_mod sig (2 ^ (Nat.log2 sig - 23))
    rw [Nat.mul_comm] at hdm
    have hrm : sig % 2 ^ (Nat.log2 sig - 23) < 2 ^ (Nat.log2 sig - 23) :=
      Nat.mod_lt _ (Nat.pow_pos (by decide))
    have h1 : (2 : Nat) ^ 23 * 2 ^ (Nat.log2 sig - 23) = 2 ^ Nat.log2 sig := by
      rw [← Nat.pow_add]; congr 1; omega
    have h2 : (2 : Nat) ^ 24 * 2 ^ (Nat.log2 sig - 23)
        = 2 ^ (Nat.log2 sig + 1) := by
      rw [← Nat.pow_add]; congr 1; omega
    -- floor bounds transfer through the div/mod split
    have hfl : 2 ^ 23 ≤ sig / 2 ^ (Nat.log2 sig - 23) := by
      by_cases hc : 2 ^ 23 ≤ sig / 2 ^ (Nat.log2 sig - 23)
      · exact hc
      · exfalso
        have hlt : sig / 2 ^ (Nat.log2 sig - 23) + 1 ≤ 2 ^ 23 := by omega
        have := Nat.mul_le_mul_right (2 ^ (Nat.log2 sig - 23)) hlt
        rw [Nat.add_mul] at this
        omega
    have hfu : sig / 2 ^ (Nat.log2 sig - 23) < 2 ^ 24 := by
      by_cases hc : sig / 2 ^ (Nat.log2 sig - 23) < 2 ^ 24
      · exact hc
      · exfalso
        have := Nat.mul_le_mul_right (2 ^ (Nat.log2 sig - 23))
          (show 2 ^ 24 ≤ sig / 2 ^ (Nat.log2 sig - 23) by omega)
        omega
    -- upper: rneQuot ≤ floor + 1 ≤ 2^24; and if floor + 1 = 2^24 hits the
    -- top exactly, the remainder is 0 there is no issue: ≤ suffices.
    omega

/-- Field extraction on a generic packed word, as `Nat` arithmetic. Kept
    generic in `n`: instantiating the packed sum FIRST and simp-computing the
    fields SECOND sends simp's arith normalization into a kernel-deep-recursion
    blowup; on an atomic `n` the same simp set is cheap. -/
theorem fields_toNat (n : Nat) (hn : n < 2 ^ 32) :
    (expField (BitVec.ofNat 32 n)).toNat = n / 2 ^ 23 % 2 ^ 8
      ∧ (fracField (BitVec.ofNat 32 n)).toNat = n % 2 ^ 23 := by
  constructor
  · simp [expField, BitVec.extractLsb'_toNat, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt hn, Nat.shiftRight_eq_div_pow]
  · simp [fracField, BitVec.extractLsb'_toNat, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt hn, Nat.shiftRight_eq_div_pow]

/-- Normal-form encode: for `1 ≤ E ≤ 254`, `M < 2^23`, the packed word
    decodes to magnitude `(2^23 + M)·2^{E-1}`, is finite, and stores exactly
    `M` in the fraction field. -/
theorem f32MagScaled_encode_normal (sgnN E M : Nat)
    (hs : sgnN = 0 ∨ sgnN = 2 ^ 31) (hE1 : 1 ≤ E) (hE2 : E ≤ 254)
    (hM : M < 2 ^ 23) :
    f32MagScaled (BitVec.ofNat 32 (sgnN + E * 2 ^ 23 + M))
        = (2 ^ 23 + M) * 2 ^ (E - 1)
      ∧ IsFiniteF32 (BitVec.ofNat 32 (sgnN + E * 2 ^ 23 + M))
      ∧ (fracField (BitVec.ofNat 32 (sgnN + E * 2 ^ 23 + M))).toNat = M := by
  have hlt : sgnN + E * 2 ^ 23 + M < 2 ^ 32 := by rcases hs with h | h <;> omega
  obtain ⟨hexp, hfrac⟩ := fields_toNat (sgnN + E * 2 ^ 23 + M) hlt
  have hdiv : (sgnN + E * 2 ^ 23 + M) / 2 ^ 23 % 2 ^ 8 = E := by
    rcases hs with h | h <;> subst h <;> omega
  have hmod : (sgnN + E * 2 ^ 23 + M) % 2 ^ 23 = M := by
    rcases hs with h | h <;> subst h <;> omega
  have hexpne : expField (BitVec.ofNat 32 (sgnN + E * 2 ^ 23 + M)) ≠ 0#8 := by
    intro hcon
    have h := congrArg BitVec.toNat hcon
    rw [hexp, hdiv] at h
    simp at h
    omega
  refine ⟨?_, ?_, by rw [hfrac, hmod]⟩
  · rw [f32MagScaled, if_neg hexpne, hfrac, hmod, hexp, hdiv]
  · rw [IsFiniteF32]
    intro hcon
    have h := congrArg BitVec.toNat hcon
    rw [hexp, hdiv] at h
    simp at h
    omega

-- ── the normal path: unfolding roundNE_f32 ──────────────────────────────────

/-- **Normal-path value.** For `sig ≠ 0`, `biased > 0`, no overflow
    (`biasedFinal ≤ 254`): `roundNE_f32` is finite, its scaled magnitude is
    `keptNorm sig · 2^{biased-1}` (the rounded significand placed at the
    binade — the carry case lands on the next binade's grid at the same
    value), and an even `keptNorm` stores an even fraction (for tie-even
    transfer). -/
theorem roundNE_normal_value (neg : Bool) (sig : Nat) (e0 : Int)
    (hsig : sig ≠ 0) (hbias : 0 < (Nat.log2 sig : Int) + e0 + 127)
    (hovf : biasedFinal sig e0 ≤ 254) :
    IsFiniteF32 (roundNE_f32 neg sig e0)
      ∧ f32MagScaled (roundNE_f32 neg sig e0)
          = keptNorm sig * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat
      ∧ (keptNorm sig % 2 = 0
          → (fracField (roundNE_f32 neg sig e0)).toNat % 2 = 0) := by
  have hkb := keptNorm_bounds sig hsig
  have hbn : ¬ ((Nat.log2 sig : Int) + e0 + 127 ≤ 0) := by omega
  have hsgn : (if neg then 2 ^ 31 else 0 : Nat) = 0
      ∨ (if neg then 2 ^ 31 else 0 : Nat) = 2 ^ 31 := by
    split <;> simp
  -- step 1: reduce `roundNE_f32` to a canonical encoded word
  have hy : roundNE_f32 neg sig e0
      = BitVec.ofNat 32 ((if neg then 2 ^ 31 else 0)
          + (biasedFinal sig e0).toNat * 2 ^ 23
          + (if 2 ^ 24 ≤ keptNorm sig then keptNorm sig / 2
             else keptNorm sig) % 2 ^ 23) := by
    rw [roundNE_f32, if_neg hsig]
    by_cases hp : Nat.log2 sig ≤ 23
    · have hsh : (Nat.log2 sig : Int) + e0 - 23 - e0 ≤ 0 := by omega
      have hnt : (-((Nat.log2 sig : Int) + e0 - 23 - e0)).toNat
          = 23 - Nat.log2 sig := by omega
      have hkeq : sig * 2 ^ (23 - Nat.log2 sig) = keptNorm sig := by
        rw [keptNorm, if_pos hp]
      simp only [if_neg hbn, if_pos hsh, hnt, hkeq, decide_eq_true_eq]
      by_cases hc : 2 ^ 24 ≤ keptNorm sig
      · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 128 := by
          rw [biasedFinal, if_pos hc]; omega
        simp only [if_pos hc]
        rw [if_neg (show ¬ (255 ≤ (Nat.log2 sig : Int) + e0 + 127 + 1) by omega)]
        apply congrArg (BitVec.ofNat 32)
        simp only [if_pos hc, hbfc]
        omega
      · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 127 := by
          rw [biasedFinal, if_neg hc]; omega
        simp only [if_neg hc]
        rw [if_neg (show ¬ (255 ≤ (Nat.log2 sig : Int) + e0 + 127) by omega)]
        apply congrArg (BitVec.ofNat 32)
        simp only [if_neg hc, hbfc]
        omega
    · have hsh : ¬ ((Nat.log2 sig : Int) + e0 - 23 - e0 ≤ 0) := by omega
      have hnt : ((Nat.log2 sig : Int) + e0 - 23 - e0).toNat
          = Nat.log2 sig - 23 := by omega
      have hkeq : rneQuot sig (Nat.log2 sig - 23) = keptNorm sig := by
        rw [keptNorm, if_neg hp]
      simp only [if_neg hbn, if_neg hsh, hnt, hkeq, decide_eq_true_eq]
      by_cases hc : 2 ^ 24 ≤ keptNorm sig
      · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 128 := by
          rw [biasedFinal, if_pos hc]; omega
        simp only [if_pos hc]
        rw [if_neg (show ¬ (255 ≤ (Nat.log2 sig : Int) + e0 + 127 + 1) by omega)]
        apply congrArg (BitVec.ofNat 32)
        simp only [if_pos hc, hbfc]
        omega
      · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 127 := by
          rw [biasedFinal, if_neg hc]; omega
        simp only [if_neg hc]
        rw [if_neg (show ¬ (255 ≤ (Nat.log2 sig : Int) + e0 + 127) by omega)]
        apply congrArg (BitVec.ofNat 32)
        simp only [if_neg hc, hbfc]
        omega
  -- step 2: decode the canonical word via the normal-form encode lemma
  by_cases hc : 2 ^ 24 ≤ keptNorm sig
  · have hkc : keptNorm sig = 2 ^ 24 := by omega
    have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 128 := by
      rw [biasedFinal, if_pos hc]; omega
    have hM0 : (if 2 ^ 24 ≤ keptNorm sig then keptNorm sig / 2
        else keptNorm sig) % 2 ^ 23 = 0 := by
      rw [if_pos hc]; omega
    rw [hy, hM0]
    obtain ⟨hval, hfin, hfracv⟩ :=
      f32MagScaled_encode_normal _ (biasedFinal sig e0).toNat 0
        hsgn (by omega) (by omega) (by decide)
    refine ⟨hfin, ?_, fun _ => by rw [hfracv]⟩
    rw [hval, hkc]
    simp only [Nat.add_zero]
    rw [← Nat.pow_add, ← Nat.pow_add]
    congr 1
    omega
  · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 127 := by
      rw [biasedFinal, if_neg hc]; omega
    have hMv : (if 2 ^ 24 ≤ keptNorm sig then keptNorm sig / 2
        else keptNorm sig) % 2 ^ 23 = keptNorm sig - 2 ^ 23 := by
      rw [if_neg hc]; omega
    rw [hy, hMv]
    obtain ⟨hval, hfin, hfracv⟩ :=
      f32MagScaled_encode_normal _ (biasedFinal sig e0).toNat
        (keptNorm sig - 2 ^ 23) hsgn (by omega) (by omega) (by omega)
    refine ⟨hfin, ?_, ?_⟩
    · rw [hval]
      have h23 : 2 ^ 23 + (keptNorm sig - 2 ^ 23) = keptNorm sig := by omega
      rw [h23]
      have hX : (biasedFinal sig e0).toNat - 1
          = ((Nat.log2 sig : Int) + e0 + 126).toNat := by omega
      rw [hX]
    · intro heven
      rw [hfracv]
      omega

/-- **Normal-path correctness.** For `sig ≠ 0` above the subnormal boundary
    (`biased > 0`) and below overflow (`biasedFinal ≤ 254`), `roundNE_f32`
    produces a finite pattern nearest to `sig · 2^e0` among ALL finite f32
    patterns. Structure: representables at or above the binade sit ON the
    rounding grid (R1's `rneQuot_nearest` transfers); representables below
    the binade are at least half an ulp away (`f32MagScaled_gap`), which
    R1's `rneQuot_halfulp` beats. -/
theorem roundNE_normal_nearest (neg : Bool) (sig : Nat) (e0 : Int)
    (hsig : sig ≠ 0) (hbias : 0 < (Nat.log2 sig : Int) + e0 + 127)
    (hovf : biasedFinal sig e0 ≤ 254) :
    IsFiniteF32 (roundNE_f32 neg sig e0)
      ∧ IsNearestMag sig e0 (roundNE_f32 neg sig e0) := by
  obtain ⟨hfin, hval, _⟩ := roundNE_normal_value neg sig e0 hsig hbias hovf
  refine ⟨hfin, ?_⟩
  intro z hz
  rw [hval]
  have hlo := Nat.log2_self_le hsig
  have hhi := Nat.lt_log2_self (n := sig)
  by_cases hp : Nat.log2 sig ≤ 23
  · -- exact regime: the encoded value IS the input, distance 0
    have hkeq : keptNorm sig = sig * 2 ^ (23 - Nat.log2 sig) := by
      rw [keptNorm, if_pos hp]
    have harg : keptNorm sig * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat
          * 2 ^ tScale e0
        = sig * 2 ^ uScale e0 := by
      rw [hkeq, Nat.mul_assoc, Nat.mul_assoc, ← Nat.pow_add, ← Nat.pow_add]
      have hexp : 23 - Nat.log2 sig
            + (((Nat.log2 sig : Int) + e0 + 126).toNat + tScale e0)
          = uScale e0 := by
        rw [uScale, tScale]; omega
      rw [hexp]
    rw [harg]
    simp only [scaledDist]
    omega
  · -- rounded regime: R1 transfers through the scale bridge K + t = u + sh
    have hts : 1 ≤ Nat.log2 sig - 23 := by omega
    have hkeq : keptNorm sig = rneQuot sig (Nat.log2 sig - 23) := by
      rw [keptNorm, if_neg hp]
    have hL : ∀ m : Nat, m * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat
          * 2 ^ tScale e0
        = m * 2 ^ (Nat.log2 sig - 23) * 2 ^ uScale e0 := by
      intro m
      rw [Nat.mul_assoc, Nat.mul_assoc, ← Nat.pow_add, ← Nat.pow_add]
      have hexp : ((Nat.log2 sig : Int) + e0 + 126).toNat + tScale e0
          = Nat.log2 sig - 23 + uScale e0 := by
        rw [uScale, tScale]; omega
      rw [hexp]
    rw [hL (keptNorm sig), hkeq]
    rcases f32MagScaled_gap ((Nat.log2 sig : Int) + e0 + 126).toNat z
      with ⟨m, hm⟩ | ⟨hK1, hbelow⟩
    · -- on the rounding grid: direct R1 nearest
      rw [hm, hL m, scaledDist_scale, scaledDist_scale]
      apply Nat.mul_le_mul_right
      rw [← gridDist_eq_scaledDist, ← gridDist_eq_scaledDist]
      exact rneQuot_nearest sig _ hts m
    · -- below the binade: at least half an ulp away, half-ulp bound wins
      rw [scaledDist_scale]
      have hhalf : scaledDist (rneQuot sig (Nat.log2 sig - 23)
            * 2 ^ (Nat.log2 sig - 23)) sig * 2 ^ uScale e0
          ≤ 2 ^ (Nat.log2 sig - 23 - 1) * 2 ^ uScale e0 :=
        Nat.mul_le_mul_right _ (by
          rw [← gridDist_eq_scaledDist]
          exact rneQuot_halfulp sig _ hts)
      have hup : (2 : Nat) ^ Nat.log2 sig * 2 ^ uScale e0
          ≤ sig * 2 ^ uScale e0 :=
        Nat.mul_le_mul_right _ hlo
      have heq1 : (2 : Nat) ^ Nat.log2 sig * 2 ^ uScale e0
          = 2 ^ (((Nat.log2 sig : Int) + e0 + 126).toNat + 23)
            * 2 ^ tScale e0 := by
        rw [← Nat.pow_add, ← Nat.pow_add]
        congr 1
        rw [uScale, tScale]; omega
      have hb2 : (f32MagScaled z
            + 2 ^ (((Nat.log2 sig : Int) + e0 + 126).toNat - 1)) * 2 ^ tScale e0
          ≤ 2 ^ (((Nat.log2 sig : Int) + e0 + 126).toNat + 23) * 2 ^ tScale e0 :=
        Nat.mul_le_mul_right _ hbelow
      rw [Nat.add_mul] at hb2
      have heq2 : (2 : Nat) ^ (((Nat.log2 sig : Int) + e0 + 126).toNat - 1)
            * 2 ^ tScale e0
          = 2 ^ (Nat.log2 sig - 23 - 1) * 2 ^ uScale e0 := by
        rw [← Nat.pow_add, ← Nat.pow_add]
        congr 1
        rw [uScale, tScale]; omega
      simp only [scaledDist] at hhalf ⊢
      omega

-- ── assembly: the full R2 statement ─────────────────────────────────────────

/-- **R2, main theorem.** For any `sig ≠ 0` whose round-to-nearest result
    stays in range (`biasedFinal ≤ 254` — see `biasedFinal_overflow_iff` for
    the value-level reading), `roundNE_f32` returns a finite pattern that is
    nearest in magnitude to `sig · 2^e0` among ALL finite f32 patterns. -/
theorem roundNE_nearest (neg : Bool) (sig : Nat) (e0 : Int) (hsig : sig ≠ 0)
    (hovf : biasedFinal sig e0 ≤ 254) :
    IsFiniteF32 (roundNE_f32 neg sig e0)
      ∧ IsNearestMag sig e0 (roundNE_f32 neg sig e0) := by
  by_cases hb : (Nat.log2 sig : Int) + e0 + 127 ≤ 0
  · exact roundNE_subnormal_nearest neg sig e0 hsig hb
  · exact roundNE_normal_nearest neg sig e0 hsig (by omega) hovf

/-- **Overflow result.** Past the range bound the rounder returns signed
    infinity (the IEEE RNE overflow behavior). -/
theorem roundNE_overflow (neg : Bool) (sig : Nat) (e0 : Int) (hsig : sig ≠ 0)
    (hbias : 0 < (Nat.log2 sig : Int) + e0 + 127)
    (hovf : 255 ≤ biasedFinal sig e0) :
    roundNE_f32 neg sig e0
      = BitVec.ofNat 32 ((if neg then 2 ^ 31 else 0) + 0x7F800000) := by
  have hbn : ¬ ((Nat.log2 sig : Int) + e0 + 127 ≤ 0) := by omega
  rw [roundNE_f32, if_neg hsig]
  by_cases hp : Nat.log2 sig ≤ 23
  · have hsh : (Nat.log2 sig : Int) + e0 - 23 - e0 ≤ 0 := by omega
    have hnt : (-((Nat.log2 sig : Int) + e0 - 23 - e0)).toNat
        = 23 - Nat.log2 sig := by omega
    have hkeq : sig * 2 ^ (23 - Nat.log2 sig) = keptNorm sig := by
      rw [keptNorm, if_pos hp]
    simp only [if_neg hbn, if_pos hsh, hnt, hkeq, decide_eq_true_eq]
    by_cases hc : 2 ^ 24 ≤ keptNorm sig
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 128 := by
        rw [biasedFinal, if_pos hc]; omega
      simp only [if_pos hc]
      rw [if_pos (show (255:Int) ≤ (Nat.log2 sig : Int) + e0 + 127 + 1 by omega)]
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 127 := by
        rw [biasedFinal, if_neg hc]; omega
      simp only [if_neg hc]
      rw [if_pos (show (255:Int) ≤ (Nat.log2 sig : Int) + e0 + 127 by omega)]
  · have hsh : ¬ ((Nat.log2 sig : Int) + e0 - 23 - e0 ≤ 0) := by omega
    have hnt : ((Nat.log2 sig : Int) + e0 - 23 - e0).toNat
        = Nat.log2 sig - 23 := by omega
    have hkeq : rneQuot sig (Nat.log2 sig - 23) = keptNorm sig := by
      rw [keptNorm, if_neg hp]
    simp only [if_neg hbn, if_neg hsh, hnt, hkeq, decide_eq_true_eq]
    by_cases hc : 2 ^ 24 ≤ keptNorm sig
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 128 := by
        rw [biasedFinal, if_pos hc]; omega
      simp only [if_pos hc]
      rw [if_pos (show (255:Int) ≤ (Nat.log2 sig : Int) + e0 + 127 + 1 by omega)]
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 127 := by
        rw [biasedFinal, if_neg hc]; omega
      simp only [if_neg hc]
      rw [if_pos (show (255:Int) ≤ (Nat.log2 sig : Int) + e0 + 127 by omega)]

/-- **Overflow is exactly the IEEE criterion.** `biasedFinal > 254` holds iff
    the correctly-rounded magnitude (`keptNorm · 2^{biased-1}` in `2^{-149}`
    units) reaches `2^128 = 2^277 · 2^{-149}` — i.e. the rounder overflows
    exactly when round-to-nearest with unbounded exponent exceeds the largest
    finite f32. -/
theorem biasedFinal_overflow_iff (sig : Nat) (e0 : Int) (hsig : sig ≠ 0)
    (hbias : 0 < (Nat.log2 sig : Int) + e0 + 127) :
    255 ≤ biasedFinal sig e0
      ↔ 2 ^ 277 ≤ keptNorm sig * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat := by
  have hkb := keptNorm_bounds sig hsig
  constructor
  · intro h255
    by_cases hc : 2 ^ 24 ≤ keptNorm sig
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 128 := by
        rw [biasedFinal, if_pos hc]; omega
      have hK : 253 ≤ ((Nat.log2 sig : Int) + e0 + 126).toNat := by omega
      have hmono : (2 : Nat) ^ 24 * 2 ^ 253
          ≤ 2 ^ 24 * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat :=
        Nat.mul_le_mul (Nat.le_refl _) (Nat.pow_le_pow_right (by decide) hK)
      have hstep : (2 : Nat) ^ 24 * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat
          ≤ keptNorm sig * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat :=
        Nat.mul_le_mul_right _ hc
      have h277 : (2 : Nat) ^ 24 * 2 ^ 253 = 2 ^ 277 := by
        rw [← Nat.pow_add]
      omega
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 127 := by
        rw [biasedFinal, if_neg hc]; omega
      have hK : 254 ≤ ((Nat.log2 sig : Int) + e0 + 126).toNat := by omega
      have hmono : (2 : Nat) ^ 23 * 2 ^ 254
          ≤ 2 ^ 23 * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat :=
        Nat.mul_le_mul (Nat.le_refl _) (Nat.pow_le_pow_right (by decide) hK)
      have hstep : (2 : Nat) ^ 23 * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat
          ≤ keptNorm sig * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat :=
        Nat.mul_le_mul_right _ hkb.1
      have h277 : (2 : Nat) ^ 23 * 2 ^ 254 = 2 ^ 277 := by
        rw [← Nat.pow_add]
      omega
  · intro hmag
    by_cases h255 : 255 ≤ biasedFinal sig e0
    · exact h255
    exfalso
    by_cases hc : 2 ^ 24 ≤ keptNorm sig
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 128 := by
        rw [biasedFinal, if_pos hc]; omega
      have hKb : ((Nat.log2 sig : Int) + e0 + 126).toNat ≤ 252 := by omega
      have hbound : keptNorm sig * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat
          ≤ 2 ^ 24 * 2 ^ 252 :=
        Nat.mul_le_mul (by omega) (Nat.pow_le_pow_right (by decide) hKb)
      have h276 : (2 : Nat) ^ 24 * 2 ^ 252 = 2 ^ 276 := by
        rw [← Nat.pow_add]
      have h277 : (2 : Nat) ^ 276 * 2 = 2 ^ 277 := by
        rw [← Nat.pow_succ]
      omega
    · have hbfc : biasedFinal sig e0 = (Nat.log2 sig : Int) + e0 + 127 := by
        rw [biasedFinal, if_neg hc]; omega
      have hKb : ((Nat.log2 sig : Int) + e0 + 126).toNat ≤ 253 := by omega
      have hbound : keptNorm sig * 2 ^ ((Nat.log2 sig : Int) + e0 + 126).toNat
          ≤ (2 ^ 24 - 1) * 2 ^ 253 :=
        Nat.mul_le_mul (by omega) (Nat.pow_le_pow_right (by decide) hKb)
      have h277 : (2 : Nat) ^ 24 * 2 ^ 253 = 2 ^ 277 := by
        rw [← Nat.pow_add]
      omega

/-- **Ties to even.** On the rounded normal path (`p ≥ 24`), when the input
    is exactly halfway between two adjacent rounding-grid points, the encoded
    fraction is even — the f32-level transfer of R1's `rneQuot_tie_even`
    (the carry case stores fraction 0, which is even). -/
theorem roundNE_tie_even (neg : Bool) (sig : Nat) (e0 : Int)
    (hsig : sig ≠ 0) (hp : 24 ≤ Nat.log2 sig)
    (hbias : 0 < (Nat.log2 sig : Int) + e0 + 127)
    (hovf : biasedFinal sig e0 ≤ 254)
    (htie : sig % 2 ^ (Nat.log2 sig - 23) = 2 ^ (Nat.log2 sig - 23 - 1)) :
    (fracField (roundNE_f32 neg sig e0)).toNat % 2 = 0 := by
  obtain ⟨_, _, htrans⟩ := roundNE_normal_value neg sig e0 hsig hbias hovf
  apply htrans
  have hkeq : keptNorm sig = rneQuot sig (Nat.log2 sig - 23) := by
    rw [keptNorm, if_neg (by omega)]
  rw [hkeq]
  exact rneQuot_tie_even sig _ (by omega) htie

end ArchFp
