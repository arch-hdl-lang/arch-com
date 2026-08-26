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

/-! ## The pairwise bound, generalized over any carrier `α`

The complete Higham pairwise-summation bound (ASNA 4.6), over an abstract carrier
`α` with a value map `v : α → Rat` and add `op : α → α → α` satisfying the per-add
`(1+δ)` property. Generalizing over `α` (rather than fixing `Rat`) is what lets it
instantiate directly to the FP case (`α = BitVec 32`, `v = f32R`,
`op = arch_f32_add`). Working through `Rat`-valued `v` keeps the analysis free of
FP internals. -/

def agamma (u : Rat) (d : Nat) : Rat := (d : Rat) * u / (1 - (d : Rat) * u)

theorem agamma_nonneg (u : Rat) (hu : 0 ≤ u) (d : Nat) (hd : (d:Rat) * u < 1) :
    0 ≤ agamma u d := by unfold agamma; apply div_nonneg (by positivity); linarith

/-- The recurrence, over abstract `u` (generalises `gamma_step`). -/
theorem agamma_step (u : Rat) (hu : 0 < u) (d : Nat) (hd : ((d:Rat) + 1) * u < 1) :
    (1 + u) * agamma u d + u ≤ agamma u (d + 1) := by
  have hden1 : (0:Rat) < 1 - (d:Rat) * u := by nlinarith [hu, hd]
  have hden2 : (0:Rat) < 1 - ((d:Rat) + 1) * u := by linarith
  have hne : (1 - u * (d:Rat)) ≠ 0 := by rw [mul_comm]; exact ne_of_gt hden1
  have hLHS : (1 + u) * agamma u d + u = ((d:Rat) + 1) * u / (1 - (d:Rat) * u) := by
    unfold agamma; field_simp [hne]; ring
  rw [hLHS]; unfold agamma; push_cast; gcongr; linarith

theorem mapAbs_nonneg {α : Type} (v : α → Rat) (l : List α) :
    0 ≤ (l.map (fun x => |v x|)).sum := by
  apply List.sum_nonneg; intro y hy; simp only [List.mem_map] at hy
  obtain ⟨z, _, rfl⟩ := hy; exact abs_nonneg _

section
set_option linter.unusedSectionVars false
variable {α : Type} (op : α → α → α) (v : α → Rat)

/-- One balanced-pairwise level over `α`. -/
def gpairUp : List α → List α
  | a :: b :: rest => op a b :: gpairUp rest
  | rest => rest

theorem gpairUp_length : ∀ xs : List α, (gpairUp op xs).length = (xs.length + 1) / 2
  | [] => by simp [gpairUp]
  | [_] => by simp [gpairUp]
  | _ :: _ :: rest => by simp only [gpairUp, List.length_cons]; rw [gpairUp_length rest]; omega

/-- **Single-level error.** One level perturbs `Σ v` by at most `u · Σ|v·|`. -/
theorem gpairUp_err {u : Rat} (hu : 0 ≤ u)
    (hadd : ∀ a b, |v (op a b) - (v a + v b)| ≤ u * |v a + v b|) :
    ∀ xs : List α, |((gpairUp op xs).map v).sum - (xs.map v).sum|
      ≤ u * (xs.map (fun x => |v x|)).sum
  | [] => by simp [gpairUp]
  | [a] => by simp [gpairUp]; positivity
  | a :: b :: rest => by
      have ih := gpairUp_err hu hadd rest
      simp only [gpairUp, List.map_cons, List.sum_cons]
      calc |v (op a b) + ((gpairUp op rest).map v).sum - (v a + (v b + (rest.map v).sum))|
          = |(v (op a b) - (v a + v b)) + (((gpairUp op rest).map v).sum - (rest.map v).sum)| := by
            ring_nf
        _ ≤ |v (op a b) - (v a + v b)| + |((gpairUp op rest).map v).sum - (rest.map v).sum| :=
            abs_add_le _ _
        _ ≤ u * |v a + v b| + u * (rest.map (fun x => |v x|)).sum := by gcongr; exact hadd a b
        _ ≤ u * (|v a| + |v b|) + u * (rest.map (fun x => |v x|)).sum := by
            gcongr; exact abs_add_le _ _
        _ = u * (|v a| + (|v b| + (rest.map (fun x => |v x|)).sum)) := by ring

/-- **Absolute-sum growth.** One level grows `Σ|v·|` by at most `(1+u)`. -/
theorem gpairUp_absSum {u : Rat} (hu : 0 ≤ u)
    (hadd : ∀ a b, |v (op a b) - (v a + v b)| ≤ u * |v a + v b|) :
    ∀ xs : List α, ((gpairUp op xs).map (fun x => |v x|)).sum
      ≤ (1 + u) * (xs.map (fun x => |v x|)).sum
  | [] => by simp [gpairUp]
  | [a] => by simp [gpairUp]; nlinarith [abs_nonneg (v a)]
  | a :: b :: rest => by
      have ih := gpairUp_absSum hu hadd rest
      have hb : |v (op a b)| ≤ (1 + u) * (|v a| + |v b|) :=
        calc |v (op a b)| ≤ (1 + u) * |v a + v b| := by
              nlinarith [hadd a b, abs_nonneg (v a + v b),
                abs_sub_abs_le_abs_sub (v (op a b)) (v a + v b)]
          _ ≤ (1 + u) * (|v a| + |v b|) := by
              have h0 : (0:Rat) ≤ 1 + u := by linarith
              gcongr; exact abs_add_le _ _
      simp only [gpairUp, List.map_cons, List.sum_cons]
      calc |v (op a b)| + ((gpairUp op rest).map (fun x => |v x|)).sum
          ≤ (1 + u) * (|v a| + |v b|) + (1 + u) * (rest.map (fun x => |v x|)).sum := by gcongr
        _ = (1 + u) * (|v a| + (|v b| + (rest.map (fun x => |v x|)).sum)) := by ring

variable [Inhabited α]

/-- Well-founded balanced-pairwise fold over `α` (recursion eqn definitional). -/
def gpairSum : List α → α
  | [] => default
  | [x] => x
  | a :: b :: rest => gpairSum (gpairUp op (a :: b :: rest))
  termination_by xs => xs.length
  decreasing_by rw [gpairUp_length]; simp only [List.length_cons]; omega

/-- **The pairwise bound (Higham ASNA 4.6), over any carrier.** For a nonempty
    length-`≤2^d` list, the pairwise fold's value differs from the exact sum by at
    most `γ_d · Σ|v·|`. The complete mathematical content of Theorem B; instantiate
    with `op = arch_f32_add`, `v = f32R`, `u = f32u`, `hadd = add_rel_bound_normal`
    (under the FP preconditions). -/
theorem gpair_bound {u : Rat} (hu : 0 < u)
    (hadd : ∀ a b, |v (op a b) - (v a + v b)| ≤ u * |v a + v b|) :
    ∀ (d : Nat) (xs : List α), xs ≠ [] → xs.length ≤ 2 ^ d → ((d:Rat) + 1) * u < 1 →
      |v (gpairSum op xs) - (xs.map v).sum| ≤ agamma u d * (xs.map (fun x => |v x|)).sum := by
  intro d
  induction d with
  | zero =>
    intro xs hne hlen _
    match xs, hne, hlen with
    | [x], _, _ => simp [gpairSum, agamma]
  | succ d IH =>
    intro xs hne hlen hdu
    have hduD : ((d:Rat) + 1) * u < 1 := by push_cast at hdu; nlinarith [hu]
    have hgnn : 0 ≤ agamma u d := agamma_nonneg u (le_of_lt hu) d (by nlinarith [hu, hduD])
    have hgnn1 : 0 ≤ agamma u (d + 1) := by
      apply agamma_nonneg u (le_of_lt hu) (d + 1); push_cast; nlinarith [hu, hdu]
    match xs with
    | [x] => simp only [gpairSum, List.map_cons, List.map_nil, List.sum_cons,
        List.sum_nil, add_zero, sub_self, abs_zero]; exact mul_nonneg hgnn1 (by positivity)
    | a :: b :: rest =>
      have hqlen : (gpairUp op (a::b::rest)).length ≤ 2 ^ d := by
        rw [gpairUp_length]; simp only [List.length_cons] at hlen ⊢; omega
      have hqne : gpairUp op (a::b::rest) ≠ [] :=
        List.ne_nil_of_length_pos (by rw [gpairUp_length]; simp only [List.length_cons]; omega)
      have hih := IH (gpairUp op (a::b::rest)) hqne hqlen hduD
      have herr := gpairUp_err op v (le_of_lt hu) hadd (a::b::rest)
      have hgrow := gpairUp_absSum op v (le_of_lt hu) hadd (a::b::rest)
      rw [show gpairSum op (a::b::rest) = gpairSum op (gpairUp op (a::b::rest)) by rw [gpairSum]]
      calc |v (gpairSum op (gpairUp op (a::b::rest))) - ((a::b::rest).map v).sum|
          ≤ |v (gpairSum op (gpairUp op (a::b::rest))) - ((gpairUp op (a::b::rest)).map v).sum|
              + |((gpairUp op (a::b::rest)).map v).sum - ((a::b::rest).map v).sum| := abs_sub_le _ _ _
        _ ≤ agamma u d * ((gpairUp op (a::b::rest)).map (fun x => |v x|)).sum
              + u * ((a::b::rest).map (fun x => |v x|)).sum := by gcongr
        _ ≤ agamma u d * ((1 + u) * ((a::b::rest).map (fun x => |v x|)).sum)
              + u * ((a::b::rest).map (fun x => |v x|)).sum := by gcongr
        _ = ((1 + u) * agamma u d + u) * ((a::b::rest).map (fun x => |v x|)).sum := by ring
        _ ≤ agamma u (d + 1) * ((a::b::rest).map (fun x => |v x|)).sum := by
            gcongr
            · exact mapAbs_nonneg v _
            · exact agamma_step u hu d hduD

end

/-! ## Domain-restricted bound (handles the FP preconditions)

`gpair_bound`'s `hadd` is unconditional, but `arch_f32_add` meets the `(1+δ)`
only on a "good" domain. This variant threads a predicate `P` — closed under
`op`, with `hadd` holding on `P` — and the invariant that every list element
satisfies `P`, preserved down the tree. Instantiating `P` with the FP
preconditions (`finiteNonzero`, normal-range, no-overflow) gives the FP bound. -/

section
set_option linter.unusedSectionVars false
variable {α : Type} (op : α → α → α) (v : α → Rat) (P : α → Prop)

/-- `P`-membership is preserved by one level (needs closure of `P` under `op`). -/
theorem gpairUp_forall (Pcl : ∀ a b, P a → P b → P (op a b)) :
    ∀ xs : List α, (∀ x ∈ xs, P x) → ∀ y ∈ gpairUp op xs, P y
  | [], _ => by simp [gpairUp]
  | [a], h => by simpa [gpairUp] using h
  | a :: b :: rest, h => by
      have hrest : ∀ x ∈ rest, P x := fun x hx => h x (by simp [hx])
      have ih := gpairUp_forall Pcl rest hrest
      intro y hy; simp only [gpairUp, List.mem_cons] at hy
      rcases hy with rfl | hy
      · exact Pcl a b (h a (by simp)) (h b (by simp))
      · exact ih y hy

theorem gpairUp_err_dom {u : Rat} (hu : 0 ≤ u)
    (haddP : ∀ a b, P a → P b → |v (op a b) - (v a + v b)| ≤ u * |v a + v b|) :
    ∀ xs : List α, (∀ x ∈ xs, P x) →
      |((gpairUp op xs).map v).sum - (xs.map v).sum| ≤ u * (xs.map (fun x => |v x|)).sum
  | [], _ => by simp [gpairUp]
  | [a], _ => by simp [gpairUp]; positivity
  | a :: b :: rest, h => by
      have hrest : ∀ x ∈ rest, P x := fun x hx => h x (by simp [hx])
      have ih := gpairUp_err_dom hu haddP rest hrest
      simp only [gpairUp, List.map_cons, List.sum_cons]
      calc |v (op a b) + ((gpairUp op rest).map v).sum - (v a + (v b + (rest.map v).sum))|
          = |(v (op a b) - (v a + v b)) + (((gpairUp op rest).map v).sum - (rest.map v).sum)| := by
            ring_nf
        _ ≤ |v (op a b) - (v a + v b)| + |((gpairUp op rest).map v).sum - (rest.map v).sum| :=
            abs_add_le _ _
        _ ≤ u * |v a + v b| + u * (rest.map (fun x => |v x|)).sum := by
            gcongr; exact haddP a b (h a (by simp)) (h b (by simp))
        _ ≤ u * (|v a| + |v b|) + u * (rest.map (fun x => |v x|)).sum := by
            gcongr; exact abs_add_le _ _
        _ = u * (|v a| + (|v b| + (rest.map (fun x => |v x|)).sum)) := by ring

theorem gpairUp_absSum_dom {u : Rat} (hu : 0 ≤ u)
    (haddP : ∀ a b, P a → P b → |v (op a b) - (v a + v b)| ≤ u * |v a + v b|) :
    ∀ xs : List α, (∀ x ∈ xs, P x) →
      ((gpairUp op xs).map (fun x => |v x|)).sum ≤ (1 + u) * (xs.map (fun x => |v x|)).sum
  | [], _ => by simp [gpairUp]
  | [a], _ => by simp [gpairUp]; nlinarith [abs_nonneg (v a)]
  | a :: b :: rest, h => by
      have hrest : ∀ x ∈ rest, P x := fun x hx => h x (by simp [hx])
      have ih := gpairUp_absSum_dom hu haddP rest hrest
      have hb : |v (op a b)| ≤ (1 + u) * (|v a| + |v b|) :=
        calc |v (op a b)| ≤ (1 + u) * |v a + v b| := by
              nlinarith [haddP a b (h a (by simp)) (h b (by simp)), abs_nonneg (v a + v b),
                abs_sub_abs_le_abs_sub (v (op a b)) (v a + v b)]
          _ ≤ (1 + u) * (|v a| + |v b|) := by
              have h0 : (0:Rat) ≤ 1 + u := by linarith
              gcongr; exact abs_add_le _ _
      simp only [gpairUp, List.map_cons, List.sum_cons]
      calc |v (op a b)| + ((gpairUp op rest).map (fun x => |v x|)).sum
          ≤ (1 + u) * (|v a| + |v b|) + (1 + u) * (rest.map (fun x => |v x|)).sum := by gcongr
        _ = (1 + u) * (|v a| + (|v b| + (rest.map (fun x => |v x|)).sum)) := by ring

variable [Inhabited α]

/-- **Domain-restricted pairwise bound.** The Higham bound under a domain
    predicate `P` (closed under `op`, `hadd` on `P`) with every element in `P`.
    The general form that instantiates to FP: `P` supplies `add_rel_bound_normal`'s
    preconditions. -/
theorem gpair_bound_dom {u : Rat} (hu : 0 < u)
    (Pcl : ∀ a b, P a → P b → P (op a b))
    (haddP : ∀ a b, P a → P b → |v (op a b) - (v a + v b)| ≤ u * |v a + v b|) :
    ∀ (d : Nat) (xs : List α), xs ≠ [] → (∀ x ∈ xs, P x) → xs.length ≤ 2 ^ d →
      ((d:Rat) + 1) * u < 1 →
      |v (gpairSum op xs) - (xs.map v).sum| ≤ agamma u d * (xs.map (fun x => |v x|)).sum := by
  intro d
  induction d with
  | zero =>
    intro xs hne _ hlen _
    match xs, hne, hlen with
    | [x], _, _ => simp [gpairSum, agamma]
  | succ d IH =>
    intro xs hne hmem hlen hdu
    have hduD : ((d:Rat) + 1) * u < 1 := by push_cast at hdu; nlinarith [hu]
    have hgnn1 : 0 ≤ agamma u (d + 1) := by
      apply agamma_nonneg u (le_of_lt hu) (d + 1); push_cast; nlinarith [hu, hdu]
    match xs with
    | [x] => simp only [gpairSum, List.map_cons, List.map_nil, List.sum_cons,
        List.sum_nil, add_zero, sub_self, abs_zero]; exact mul_nonneg hgnn1 (by positivity)
    | a :: b :: rest =>
      have hgnn : 0 ≤ agamma u d := agamma_nonneg u (le_of_lt hu) d (by nlinarith [hu, hduD])
      have hqlen : (gpairUp op (a::b::rest)).length ≤ 2 ^ d := by
        rw [gpairUp_length]; simp only [List.length_cons] at hlen ⊢; omega
      have hqne : gpairUp op (a::b::rest) ≠ [] :=
        List.ne_nil_of_length_pos (by rw [gpairUp_length]; simp only [List.length_cons]; omega)
      have hqmem := gpairUp_forall op P Pcl (a::b::rest) hmem
      have hih := IH (gpairUp op (a::b::rest)) hqne hqmem hqlen hduD
      have herr := gpairUp_err_dom op v P (le_of_lt hu) haddP (a::b::rest) hmem
      have hgrow := gpairUp_absSum_dom op v P (le_of_lt hu) haddP (a::b::rest) hmem
      rw [show gpairSum op (a::b::rest) = gpairSum op (gpairUp op (a::b::rest)) by rw [gpairSum]]
      calc |v (gpairSum op (gpairUp op (a::b::rest))) - ((a::b::rest).map v).sum|
          ≤ |v (gpairSum op (gpairUp op (a::b::rest))) - ((gpairUp op (a::b::rest)).map v).sum|
              + |((gpairUp op (a::b::rest)).map v).sum - ((a::b::rest).map v).sum| := abs_sub_le _ _ _
        _ ≤ agamma u d * ((gpairUp op (a::b::rest)).map (fun x => |v x|)).sum
              + u * ((a::b::rest).map (fun x => |v x|)).sum := by gcongr
        _ ≤ agamma u d * ((1 + u) * ((a::b::rest).map (fun x => |v x|)).sum)
              + u * ((a::b::rest).map (fun x => |v x|)).sum := by gcongr
        _ = ((1 + u) * agamma u d + u) * ((a::b::rest).map (fun x => |v x|)).sum := by ring
        _ ≤ agamma u (d + 1) * ((a::b::rest).map (fun x => |v x|)).sum := by
            gcongr
            · exact mapAbs_nonneg v _
            · exact agamma_step u hu d hduD

end

/-! ## Structure wiring: `archPairwiseSum = gpairSum arch_f32_add`

`ScaledDot.archPairwiseSum` is the fuel-based `BitVec` tree; `gpairSum` is the
well-founded one. They compute the same balanced-pairwise fold — proved here — so
`gpair_bound_dom` (instantiated at `op = arch_f32_add`, `v = f32R`) transfers to
`f32R (archPairwiseSum xs)`. -/

theorem pairUp_eq : ∀ xs : List (BitVec 32), pairUp xs = gpairUp arch_f32_add xs
  | [] => rfl
  | [_] => rfl
  | a :: b :: rest => by simp only [pairUp, gpairUp]; rw [pairUp_eq rest]

/-- Fuel-based fold with enough fuel equals the well-founded fold. -/
theorem pairwiseFuel_eq : ∀ (n : Nat) (xs : List (BitVec 32)), xs.length ≤ n →
    pairwiseFuel n xs = gpairSum arch_f32_add xs := by
  intro n
  induction n using Nat.strong_induction_on with
  | _ n IH =>
    intro xs hlen
    match xs with
    | [] => simp only [pairwiseFuel, gpairSum]; rfl
    | [x] => simp only [pairwiseFuel, gpairSum]
    | a :: b :: rest =>
      have hn : 1 ≤ n := by simp only [List.length_cons] at hlen; omega
      obtain ⟨m, rfl⟩ : ∃ m, n = m + 1 := ⟨n - 1, by omega⟩
      have hqlen : (gpairUp arch_f32_add (a::b::rest)).length ≤ m := by
        rw [gpairUp_length]; simp only [List.length_cons] at hlen ⊢; omega
      simp only [pairwiseFuel, pairUp_eq]
      rw [IH m (by omega) _ hqlen,
          show gpairSum arch_f32_add (a::b::rest)
            = gpairSum arch_f32_add (gpairUp arch_f32_add (a::b::rest)) from by rw [gpairSum]]

/-- **`archPairwiseSum` is the abstract fold at `arch_f32_add`.** -/
theorem archPairwiseSum_eq (xs : List (BitVec 32)) :
    archPairwiseSum xs = gpairSum arch_f32_add xs := by
  unfold archPairwiseSum; exact pairwiseFuel_eq xs.length xs (le_refl _)

/-! ## Remaining: FP domain and the `f32ToRat` wiring

With `archPairwiseSum_eq` and `gpair_bound_dom`, the bound on `f32R
(archPairwiseSum xs)` follows once two items are supplied:

1. **The FP domain `P`** — giving `haddP` (from `add_rel_bound_normal`) and `Pcl`
   (closure). As noted, `Pcl` carries the standard no-underflow / no-overflow
   summation assumption (cancellation), so `P` encodes a genuine well-scaled
   hypothesis, not a naive interval.
2. **`f32ToRat` wiring.** `ScaledDot.pairwise_sum_error_bound` is stated against
   the *opaque* `f32ToRat`; discharging it needs `f32ToRat = f32R` (a small edit
   to the merged `ScaledDot` frame), or restating the target against `f32R`. -/

end ArchFp
