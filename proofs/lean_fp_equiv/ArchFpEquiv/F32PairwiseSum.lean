import ArchFpEquiv.ScaledDot
import ArchFpEquiv.F32AddRel
import Mathlib

/-!
# Theorem B — the pairwise-summation error bound (foundation)

Theorem B (`ScaledDot.pairwise_sum_error_bound`, currently an axiom) is the
Higham pairwise-summation bound (ASNA Thm 4.6): the balanced-pairwise FP32 sum of
a list differs from the exact sum by at most `γ_d · Σ|xᵢ|`, where `d = ⌈log₂N⌉`
and `γ_d = d·u/(1−d·u)`, `u = 2⁻²⁴`.

This module builds the **algebraic foundation** — the `γ` recurrence that makes
the tree induction compose, its non-negativity, and the base cases — all proved
over the concrete `f32R` (`F32AddRel`), sorry-free. The remaining bulk (the tree
induction over `pairUp`/`pairwiseFuel` and the FP-precondition threading) is
documented at the end.
-/

namespace ArchFp

open scoped BigOperators

set_option exponentiation.threshold 600

/-! ## The `γ_d` recurrence (the algebraic heart) -/

/-- **`γ` non-negative** (for `d·u < 1`, i.e. `d < 2²⁴` — always true for real
    tree depths). -/
theorem gammaPairwise_nonneg (d : Nat) (hd : (d : Rat) * f32u < 1) :
    0 ≤ gammaPairwise d := by
  unfold gammaPairwise
  have hu : (0:Rat) < f32u := by unfold f32u; positivity
  have : (0:Rat) < 1 - (d:Rat) * f32u := by linarith
  positivity

/-- **The pairwise recurrence.** Combining two depth-`d` subsums with one more
    rounding: `(1+u)·γ_d + u ≤ γ_{d+1}`. This is exactly the inequality the tree
    induction needs at each level — one extra add multiplies the accumulated
    relative error by `(1+u)` and adds a fresh `u`, and `γ_{d+1}` absorbs both.
    Requires `(d+1)·u < 1` (met for any real depth: `d < 2²⁴`). -/
theorem gamma_step (d : Nat) (hd : ((d:Rat) + 1) * f32u < 1) :
    (1 + f32u) * gammaPairwise d + f32u ≤ gammaPairwise (d + 1) := by
  have hu : (0:Rat) < f32u := by unfold f32u; positivity
  have hden1 : (0:Rat) < 1 - (d:Rat) * f32u := by nlinarith [hu, hd]
  have hden2 : (0:Rat) < 1 - ((d:Rat) + 1) * f32u := by linarith
  have hne : (1 - f32u * (d:Rat)) ≠ 0 := by rw [mul_comm]; exact ne_of_gt hden1
  have hLHS : (1 + f32u) * gammaPairwise d + f32u
      = ((d:Rat) + 1) * f32u / (1 - (d:Rat) * f32u) := by
    unfold gammaPairwise; field_simp [hne]; ring
  rw [hLHS]; unfold gammaPairwise; push_cast; gcongr; linarith

/-! ## Base cases of the pairwise bound (over the concrete `f32R`) -/

/-- The value of `+0.0` is `0`. -/
theorem f32R_zero : f32R 0#32 = 0 := by
  have h : f32SignedScaled 0#32 = 0 := by decide
  unfold f32R; rw [h]; simp

/-- Empty sum: both sides `0`. -/
theorem pairwise_bound_nil (d : Nat) :
    ratAbs (f32R (archPairwiseSum []) - ratSum (([] : List (BitVec 32)).map f32R))
      ≤ gammaPairwise d * ratSum (([] : List (BitVec 32)).map (fun p => ratAbs (f32R p))) := by
  simp [archPairwiseSum, pairwiseFuel, ratSum, ratAbs, f32R_zero]

/-- Singleton sum: exact, error `0` (needs `d·u < 1`, so `γ_d ≥ 0`). -/
theorem pairwise_bound_singleton (x : BitVec 32) (d : Nat) (hd : (d : Rat) * f32u < 1) :
    ratAbs (f32R (archPairwiseSum [x]) - ratSum ([x].map f32R))
      ≤ gammaPairwise d * ratSum ([x].map (fun p => ratAbs (f32R p))) := by
  have hg : 0 ≤ gammaPairwise d := gammaPairwise_nonneg d hd
  have hnn : 0 ≤ ratAbs (f32R x) := by unfold ratAbs; split <;> linarith
  simp only [archPairwiseSum, pairwiseFuel, List.map_cons, List.map_nil, ratSum,
    List.foldr, add_zero, sub_self]
  rw [show ratAbs (0 : Rat) = 0 by simp [ratAbs]]
  exact mul_nonneg hg hnn

/-! ## Abstract single-level bounds (the induction step, over `Rat`)

The mathematical core of the tree induction, parameterized over an abstract
rounded-add `radd` with the per-add `(1+δ)` property `|radd a b − (a+b)| ≤
u·|a+b|` — this is exactly what `F32AddRel.add_rel_bound_normal` supplies (with
`radd = f32R ∘ arch_f32_add` and `u = 2⁻²⁴`). One `pairUp` level halves the list
and its two effects are bounded here; the depth induction then folds them with
`gamma_step`. Working over `Rat` (Mathlib `List.sum` / `|·|`) keeps this free of
FP details. -/

/-- One balanced-pairwise level over `Rat` (abstract add). -/
def rpairUp (radd : Rat → Rat → Rat) : List Rat → List Rat
  | a :: b :: rest => radd a b :: rpairUp radd rest
  | rest => rest

/-- A level exactly halves (rounding up). -/
theorem rpairUp_length {radd : Rat → Rat → Rat} :
    ∀ xs : List Rat, (rpairUp radd xs).length = (xs.length + 1) / 2
  | [] => rfl
  | [_] => by simp [rpairUp]
  | _ :: _ :: rest => by
      simp only [rpairUp, List.length_cons]; rw [rpairUp_length rest]; omega

/-- **Single-level error.** One `pairUp` level perturbs the sum by at most
    `u · Σ|xᵢ|`. -/
theorem rpairUp_err {radd : Rat → Rat → Rat} {u : Rat} (hu : 0 ≤ u)
    (hadd : ∀ a b, |radd a b - (a + b)| ≤ u * |a + b|) :
    ∀ xs : List Rat, |(rpairUp radd xs).sum - xs.sum| ≤ u * (xs.map (|·|)).sum
  | [] => by simp [rpairUp]
  | [a] => by simp [rpairUp]; positivity
  | a :: b :: rest => by
      have ih := rpairUp_err hu hadd rest
      simp only [rpairUp, List.sum_cons, List.map_cons]
      calc |radd a b + (rpairUp radd rest).sum - (a + (b + rest.sum))|
          = |(radd a b - (a + b)) + ((rpairUp radd rest).sum - rest.sum)| := by ring_nf
        _ ≤ |radd a b - (a + b)| + |(rpairUp radd rest).sum - rest.sum| := abs_add_le _ _
        _ ≤ u * |a + b| + u * (rest.map (|·|)).sum := by gcongr; exact hadd a b
        _ ≤ u * (|a| + |b|) + u * (rest.map (|·|)).sum := by gcongr; exact abs_add_le a b
        _ = u * (|a| + (|b| + (rest.map (|·|)).sum)) := by ring

/-- **Absolute-sum growth.** One level grows `Σ|·|` by at most a factor `(1+u)`.
    This is what turns the depth-`d` accumulation into the `(1+u)^d`-flavoured
    `γ_d`. -/
theorem rpairUp_absSum {radd : Rat → Rat → Rat} {u : Rat} (hu : 0 ≤ u)
    (hadd : ∀ a b, |radd a b - (a + b)| ≤ u * |a + b|) :
    ∀ xs : List Rat, ((rpairUp radd xs).map (|·|)).sum ≤ (1 + u) * (xs.map (|·|)).sum
  | [] => by simp [rpairUp]
  | [a] => by simp [rpairUp]; nlinarith [abs_nonneg a]
  | a :: b :: rest => by
      have ih := rpairUp_absSum hu hadd rest
      have hb : |radd a b| ≤ (1 + u) * (|a| + |b|) :=
        calc |radd a b| ≤ (1 + u) * |a + b| := by
              nlinarith [hadd a b, abs_nonneg (a+b), abs_sub_abs_le_abs_sub (radd a b) (a+b)]
          _ ≤ (1 + u) * (|a| + |b|) := by
              have h0 : (0:Rat) ≤ 1 + u := by linarith
              gcongr; exact abs_add_le a b
      simp only [rpairUp, List.map_cons, List.sum_cons]
      calc |radd a b| + ((rpairUp radd rest).map (|·|)).sum
          ≤ (1 + u) * (|a| + |b|) + (1 + u) * (rest.map (|·|)).sum := by gcongr
        _ = (1 + u) * (|a| + (|b| + (rest.map (|·|)).sum)) := by ring

/-! ## Abstract `γ` (matches `gammaPairwise` at `u = f32u`) and the depth fold -/

/-- Abstract Higham factor `γ_d = d·u/(1−d·u)`. `agamma f32u d = gammaPairwise d`. -/
def agamma (u : Rat) (d : Nat) : Rat := (d : Rat) * u / (1 - (d : Rat) * u)

theorem agamma_nonneg (u : Rat) (hu : 0 ≤ u) (d : Nat) (hd : (d:Rat) * u < 1) :
    0 ≤ agamma u d := by
  unfold agamma; apply div_nonneg (by positivity); linarith

/-- The recurrence, over abstract `u` (generalises `gamma_step`). -/
theorem agamma_step (u : Rat) (hu : 0 < u) (d : Nat) (hd : ((d:Rat) + 1) * u < 1) :
    (1 + u) * agamma u d + u ≤ agamma u (d + 1) := by
  have hden1 : (0:Rat) < 1 - (d:Rat) * u := by nlinarith [hu, hd]
  have hden2 : (0:Rat) < 1 - ((d:Rat) + 1) * u := by linarith
  have hne : (1 - u * (d:Rat)) ≠ 0 := by rw [mul_comm]; exact ne_of_gt hden1
  have hLHS : (1 + u) * agamma u d + u = ((d:Rat) + 1) * u / (1 - (d:Rat) * u) := by
    unfold agamma; field_simp [hne]; ring
  rw [hLHS]; unfold agamma; push_cast; gcongr; linarith

/-- Well-founded pairwise sum over `Rat` (recursion equation is definitional). -/
def rpairSum (radd : Rat → Rat → Rat) : List Rat → Rat
  | [] => 0
  | [x] => x
  | a :: b :: rest => rpairSum radd (rpairUp radd (a :: b :: rest))
  termination_by xs => xs.length
  decreasing_by simp only [rpairUp_length, List.length_cons]; omega

theorem mapAbs_nonneg (l : List Rat) : 0 ≤ (l.map (|·|)).sum := by
  apply List.sum_nonneg
  intro y hy; simp only [List.mem_map] at hy
  obtain ⟨z, _, rfl⟩ := hy; exact abs_nonneg z

/-- **Abstract pairwise-summation error bound (Higham ASNA 4.6).** For any
    rounded-add `radd` with the per-add `(1+δ)` property, the balanced-pairwise
    sum of a length-`≤2^d` list differs from the exact sum by at most
    `γ_d · Σ|xᵢ|`. This is the complete mathematical content of Theorem B; the
    depth induction folds `rpairUp_err` / `rpairUp_absSum` with `agamma_step`.
    (`(d+1)·u < 1` — met for any real depth, `d < 2²⁴`.) -/
theorem rpair_bound {radd : Rat → Rat → Rat} {u : Rat} (hu : 0 < u)
    (hadd : ∀ a b, |radd a b - (a + b)| ≤ u * |a + b|) :
    ∀ (d : Nat) (xs : List Rat), xs.length ≤ 2 ^ d → ((d:Rat) + 1) * u < 1 →
      |rpairSum radd xs - xs.sum| ≤ agamma u d * (xs.map (|·|)).sum := by
  intro d
  induction d with
  | zero =>
    intro xs hlen _
    match xs, hlen with
    | [], _ => simp [rpairSum]
    | [x], _ => simp [rpairSum, agamma]
  | succ d IH =>
    intro xs hlen hdu
    have hduD : ((d:Rat) + 1) * u < 1 := by push_cast at hdu; nlinarith [hu]
    have hgnn : 0 ≤ agamma u d := agamma_nonneg u (le_of_lt hu) d (by nlinarith [hu, hduD])
    have hgnn1 : 0 ≤ agamma u (d + 1) := by
      apply agamma_nonneg u (le_of_lt hu) (d + 1); push_cast; nlinarith [hu, hdu]
    match xs with
    | [] => simp [rpairSum]
    | [x] => simp only [rpairSum, List.map_cons, List.map_nil, List.sum_cons,
        List.sum_nil, add_zero, sub_self, abs_zero]; exact mul_nonneg hgnn1 (by positivity)
    | a :: b :: rest =>
      have hqlen : (rpairUp radd (a::b::rest)).length ≤ 2 ^ d := by
        rw [rpairUp_length]; simp only [List.length_cons] at hlen ⊢; omega
      have hih := IH (rpairUp radd (a::b::rest)) hqlen hduD
      have herr := rpairUp_err (le_of_lt hu) hadd (a::b::rest)
      have hgrow := rpairUp_absSum (le_of_lt hu) hadd (a::b::rest)
      rw [show rpairSum radd (a::b::rest)
          = rpairSum radd (rpairUp radd (a::b::rest)) by rw [rpairSum]]
      calc |rpairSum radd (rpairUp radd (a::b::rest)) - (a::b::rest).sum|
          ≤ |rpairSum radd (rpairUp radd (a::b::rest)) - (rpairUp radd (a::b::rest)).sum|
              + |(rpairUp radd (a::b::rest)).sum - (a::b::rest).sum| := abs_sub_le _ _ _
        _ ≤ agamma u d * ((rpairUp radd (a::b::rest)).map (|·|)).sum
              + u * ((a::b::rest).map (|·|)).sum := by gcongr
        _ ≤ agamma u d * ((1 + u) * ((a::b::rest).map (|·|)).sum)
              + u * ((a::b::rest).map (|·|)).sum := by gcongr
        _ = ((1 + u) * agamma u d + u) * ((a::b::rest).map (|·|)).sum := by ring
        _ ≤ agamma u (d + 1) * ((a::b::rest).map (|·|)).sum := by
            gcongr
            · exact mapAbs_nonneg _
            · exact agamma_step u hu d hduD

/-! ## Remaining: FP instantiation of `rpair_bound`

The abstract bound `rpair_bound` is the full Higham result. Discharging
`ScaledDot.pairwise_sum_error_bound` now needs only the **FP instantiation**:
`radd := ` the value-level rounded add (`f32R ∘ arch_f32_add`), `u := f32u`, with
`hadd` supplied by `F32AddRel.add_rel_bound_normal`. Two plumbing tasks remain:

1. **Precondition threading.** `add_rel_bound_normal` needs each add's operands
   `finiteNonzero` and the partial sum normal-range / non-overflowing (the
   standard no-underflow / no-overflow summation hypotheses). A concrete
   `AllFiniteNoOverflow` predicate that yields `hadd` for the terms in play, and
   is preserved down the tree, replaces the abstract obligation.
2. **Structure wiring.** Relate `f32R (archPairwiseSum xs)` (the fuel-based
   `BitVec` tree) to `rpairSum radd (xs.map f32R)`, and `ScaledDot`'s opaque
   `f32ToRat` to `f32R`. -/

end ArchFp
