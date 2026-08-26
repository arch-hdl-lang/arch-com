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

/-! ## Remaining: the tree induction + FP-precondition threading

With `gamma_step` (the recurrence) and the base cases in hand, the full bound

  `ratAbs (f32R (archPairwiseSum xs) − ratSum (xs.map f32R))
     ≤ gammaPairwise d · ratSum (xs.map (ratAbs ∘ f32R))`  for `xs.length ≤ 2^d`

reduces to two remaining pieces:

1. **Single-level `pairUp` bound.** `pairUp xs = arch_f32_add` of adjacent pairs
   (lone element passes through). Each pair contributes one rounding, bounded by
   the per-add `(1+δ)` (`F32AddRel.add_rel_bound_normal`): so
   `ratSum ((pairUp xs).map f32R)` differs from `ratSum (xs.map f32R)` by at most
   `u · ratSum (xs.map (ratAbs ∘ f32R))`, and the `|·|`-sum is non-increasing.

2. **The fuel induction.** `archPairwiseSum xs = pairwiseFuel xs.length xs`, and
   `pairwiseFuel (n+1) xs = pairwiseFuel n (pairUp xs)`. Induct on `d`: the depth
   drops by one per `pairUp` level (`(pairUp xs).length ≤ ⌈xs.length/2⌉ ≤ 2^{d-1}`),
   the IH gives `γ_{d-1}` on the halved list, and `gamma_step` combines the
   level-1 error with the IH error into `γ_d`.

The genuine difficulty is **precondition threading**: `add_rel_bound_normal`
requires each add's operands to be `finiteNonzero` and the partial sum to be in
the normal range (`2¹⁷² ≤ |·|`) and non-overflowing — the standard no-underflow /
no-overflow summation hypotheses. A concrete `AllFiniteNoOverflow`-style
predicate over the tree, and a proof it is preserved by `pairUp`, is the
substantial remaining work. Discharging `ScaledDot.pairwise_sum_error_bound` then
also requires wiring `ScaledDot`'s opaque `f32ToRat` to this concrete `f32R`. -/

end ArchFp
