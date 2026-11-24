import ArchFpEquiv.BlockExact
import ArchFpEquiv.F32AddRel

/-!
# Block-dot model and exactness obligations

The concrete pairwise schedule and rational algebra are defined here. The
summation theorem and final composition live in `F32PairwiseSum`, which imports
this model. The composition uses the proved summation bound, not an axiom.

Element and E8M0 scale conversions are concrete, generated from the shared IR.
`BlockExact` proves finite element-product exactness and representable scaling;
this file contains no custom axioms. Summation safety and exact scale
representability remain explicit hypotheses of the final error bound.
-/

namespace ArchFp

/-- Exact rational value of a **finite** FP32 — now the concrete `F32AddRel.f32R`
    (`f32SignedScaled z / 2¹⁴⁹`, a dyadic rational). Was `opaque` in Phase 1; this
    wiring is what lets Theorem B (`F32PairwiseSum`) discharge
    `pairwise_sum_error_bound`. `abbrev`, so it is definitionally `f32R`. -/
@[reducible] noncomputable def f32ToRat : BitVec 32 → Rat := f32R

/-! ## The datapath model — faithful to `dot_schedule` / `sv_dot` -/

/-- One balanced-pairwise reduction round: add adjacent pairs, a lone trailing
    element passes through untouched (never padded with `+0.0`, which would flip
    `-0.0` — the `sv_dot` invariant). -/
def pairUp : List (BitVec 32) → List (BitVec 32)
  | a :: b :: rest => arch_f32_add a b :: pairUp rest
  | rest => rest

/-- Fuel-driven pairwise fold. Structural on the `Nat` fuel, so it needs no
    termination proof; `archPairwiseSum` supplies `xs.length` fuel, always
    enough since each non-trivial round at least halves the list. -/
def pairwiseFuel : Nat → List (BitVec 32) → BitVec 32
  | _,     []      => 0#32
  | _,     [x]     => x
  | 0,     _ :: _  => 0#32          -- unreachable at the supplied fuel
  | n + 1, xs      => pairwiseFuel n (pairUp xs)

/-- Balanced-pairwise FP32 summation — the accumulation order `dot_schedule`
    fixes (⌈log₂ N⌉ deep). -/
def archPairwiseSum (xs : List (BitVec 32)) : BitVec 32 :=
  pairwiseFuel xs.length xs

/-- The N element-pair products, each exact in FP32 (`MX_DOT`). -/
def archProducts (fmt : BlockElem) (ea eb : List (BitVec 8)) : List (BitVec 32) :=
  (ea.zip eb).map (fun p => arch_f32_mul (widenElem fmt p.1) (widenElem fmt p.2))

/-- The full block dot: pairwise-sum the exact products, then apply the two
    block scales **one at a time** — `((S · Xa) · Xb)`, never `(Xa·Xb)·S`
    (pre-forming the scale product can overflow to Inf even when the result is
    representable; see `sv_dot`). -/
def archScaledDot (fmt : BlockElem) (sa sb : BitVec 8) (ea eb : List (BitVec 8)) : BitVec 32 :=
  arch_f32_mul
    (arch_f32_mul (archPairwiseSum (archProducts fmt ea eb)) (widenScale sa))
    (widenScale sb)

/-! ## Model sanity — the base cases reduce as intended (proved now) -/

/-- Empty and singleton sums. -/
@[simp] theorem archPairwiseSum_nil : archPairwiseSum [] = 0#32 := rfl
@[simp] theorem archPairwiseSum_singleton (x : BitVec 32) :
    archPairwiseSum [x] = x := rfl

/-- Two-element sum is one FP32 add — the N = 2 accumulation, matching the
    N = 2 miter shape `ScaledDotE4m3N2`. -/
theorem archPairwiseSum_pair (a b : BitVec 32) :
    archPairwiseSum [a, b] = arch_f32_add a b := rfl

/-- Four-element sum is the two-level tree `add(add(a,b), add(c,d))` — matching
    the N = 4 miter shapes (`ScaledDotE2m1N4`, `ScaledDotE4m3N4`). Pins that the
    fuel fold reproduces `dot_schedule(4) = adds [(0,1),(2,3),(4,5)]`. -/
theorem archPairwiseSum_quad (a b c d : BitVec 32) :
    archPairwiseSum [a, b, c, d]
      = arch_f32_add (arch_f32_add a b) (arch_f32_add c d) := rfl

/-! ## Rational helpers (core `Rat`, no Mathlib) -/

/-- Absolute value on `Rat`. -/
def ratAbs (x : Rat) : Rat := if x < 0 then -x else x

/-- Sum of a list of rationals. -/
def ratSum (xs : List Rat) : Rat := xs.foldr (· + ·) 0

/-- Exact real (rational) value of the whole block dot: the exact sum of the
    exact element products, times the two scale values. This is the reference
    `scaled_dot_error_bound` bounds the hardware against — the exact dot of the
    *widened* operands, not "fp.dot in some order" (which would be circular).

    The `(sum) * Xa * Xb` grouping (left-associative) is deliberate: it matches
    the hardware's one-at-a-time scale application, so the composition proof
    chains through `ratAbs_sub_mul_le` per scale with no reassociation. -/
noncomputable def exactBlockDot (fmt : BlockElem) (sa sb : BitVec 8) (ea eb : List (BitVec 8)) : Rat :=
  ratSum ((ea.zip eb).map (fun p => f32ToRat (widenElem fmt p.1) * f32ToRat (widenElem fmt p.2)))
    * f32ToRat (widenScale sa) * f32ToRat (widenScale sb)

/-- FP32 unit roundoff, `u = 2⁻²⁴`. -/
def f32u : Rat := 1 / (2 ^ 24)

/-- The standard pairwise-summation growth factor `γ_d = d·u / (1 − d·u)` for a
    tree of depth `d = ⌈log₂ N⌉` (Higham, ASNA Thm 4.6). -/
def gammaPairwise (d : Nat) : Rat := (d * f32u) / (1 - d * f32u)

/-! ### Rational-algebra facts (Phase 2 — proved, no Mathlib)

The monotone-multiply step the composition needs, discharged in core `Rat`
(`Rat.mul_le_mul_of_nonneg_right`, `Rat.abs_of_nonneg/nonpos`, `Rat.mul_nonneg`,
`Rat.neg_mul` — no Mathlib required). What was `axiom ratAbs_sub_mul_le` in
Phase 1 is now `theorem ratAbs_sub_mul_le`. -/

/-- `ratAbs` agrees with core `Rat.abs` (which branches on `0 ≤ x`). -/
theorem ratAbs_eq_abs (x : Rat) : ratAbs x = x.abs := by
  unfold ratAbs Rat.abs
  by_cases h : 0 ≤ x
  · rw [if_pos h, if_neg (by rw [Rat.not_lt]; exact h)]
  · rw [if_neg h, if_pos ((Rat.not_le).mp h)]

/-- Right-distributivity of `*` over `-` on `Rat`. -/
theorem rat_sub_mul (a b c : Rat) : (a - b) * c = a * c - b * c := by
  rw [Rat.sub_eq_add_neg, Rat.add_mul, Rat.neg_mul, ← Rat.sub_eq_add_neg]

theorem rat_neg_nonneg {z : Rat} (h : z ≤ 0) : 0 ≤ -z := by
  have := Rat.neg_le_neg h; simpa using this

theorem rat_mul_nonpos {z c : Rat} (hz : z ≤ 0) (hc : 0 ≤ c) : z * c ≤ 0 := by
  have h2 : 0 ≤ (-z) * c := Rat.mul_nonneg (rat_neg_nonneg hz) hc
  rw [Rat.neg_mul] at h2
  have := Rat.neg_le_neg h2; simpa using this

/-- `|z · c| = |z| · c` for `c ≥ 0`. -/
theorem ratAbs_mul_nonneg (z : Rat) {c : Rat} (hc : 0 ≤ c) :
    ratAbs (z * c) = ratAbs z * c := by
  rw [ratAbs_eq_abs, ratAbs_eq_abs]
  by_cases hz : 0 ≤ z
  · rw [Rat.abs_of_nonneg (Rat.mul_nonneg hz hc), Rat.abs_of_nonneg hz]
  · have hz0 : z ≤ 0 := Rat.le_of_lt ((Rat.not_le).mp hz)
    rw [Rat.abs_of_nonpos (rat_mul_nonpos hz0 hc), Rat.abs_of_nonpos hz0, Rat.neg_mul]

/-- **Monotone multiply (was an axiom in Phase 1).** For `c ≥ 0`, a bound on
    `|x − y|` scales to a bound on `|x·c − y·c|`. This is the one algebra step the
    end-to-end composition needs to push the pairwise bound through the two
    (non-negative) block scales. -/
theorem ratAbs_sub_mul_le (x y bound c : Rat) (hc : 0 ≤ c)
    (h : ratAbs (x - y) ≤ bound) :
    ratAbs (x * c - y * c) ≤ bound * c := by
  rw [← rat_sub_mul, ratAbs_mul_nonneg (x - y) hc]
  exact Rat.mul_le_mul_of_nonneg_right h hc

/-! ## Composition of the proved exactness facts -/

/-- Finite widened element products are exact in rational arithmetic. -/
theorem products_sum_exact (fmt : BlockElem) (ea eb : List (BitVec 8))
    (hfinite : ∀ p ∈ ea.zip eb, FiniteElem fmt p.1 ∧ FiniteElem fmt p.2) :
    ratSum ((archProducts fmt ea eb).map f32ToRat)
      = ratSum ((ea.zip eb).map
          (fun p => f32ToRat (widenElem fmt p.1) * f32ToRat (widenElem fmt p.2))) := by
  unfold archProducts
  rw [List.map_map]
  apply congrArg ratSum
  apply List.map_congr_left
  intro p hp
  exact products_exact fmt p.1 p.2 (hfinite p hp).1 (hfinite p hp).2

/-- The two block scales pull out of the FP32 result exactly (one at a time,
    left-associated as the hardware applies them). Just the scale-exactness
    theorem twice — no algebra. -/
theorem archScaledDot_scale_pull (fmt : BlockElem) (sa sb : BitVec 8) (ea eb : List (BitVec 8))
    (hov1 : NoScaleOverflow (archPairwiseSum (archProducts fmt ea eb)) sa)
    (hov2 : NoScaleOverflow
      (arch_f32_mul (archPairwiseSum (archProducts fmt ea eb)) (widenScale sa)) sb) :
    f32ToRat (archScaledDot fmt sa sb ea eb)
      = f32ToRat (archPairwiseSum (archProducts fmt ea eb))
          * f32ToRat (widenScale sa) * f32ToRat (widenScale sb) := by
  unfold archScaledDot f32ToRat
  rw [scale_mul_exact _ sb hov2, scale_mul_exact _ sa hov1]

/-! The final composition is in `F32PairwiseSum`, after the proved summation bound. -/

end ArchFp
