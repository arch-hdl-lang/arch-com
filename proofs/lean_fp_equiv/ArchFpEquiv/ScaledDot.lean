import ArchFpEquiv.Model

/-!
# `scaled_dot` accumulation — the value/error frame (Phase 1 skeleton)

This file states the accumulation half of the `scaled_dot` correctness story and
composes it into an end-to-end block-dot error bound. It is the Lean companion
of the structural miter `tests/fp_v1/smt_proof/scaled_dot_miter.sh` (Theorem A,
which machine-checks that the *emitted SV* implements exactly the schedule this
file's `archScaledDot` encodes).

See `proofs/lean_fp_equiv/SCALED_DOT_ACCUMULATION_SCOPE.md` for the full plan.

**Phase status.**
- The computable models (`archPairwiseSum`, `archProducts`, `archScaledDot`)
  are faithful to `src/fp_block.rs::dot_schedule` / `sv_dot` and are *proved*
  here to reduce correctly on the base cases.
- The two exactness facts (`products_exact` — the `MX_DOT` SMT gate; and
  `scale_mul_exact` — power-of-two scale multiply) are stated as `axiom`s here;
  each already has an exhaustive SMT proof (`fp_smt_proof::MX_DOT`,
  `fp_smt_proof::MX_SCALE_CONV`) — Phase 3 imports them as lemmas.
- **Theorem B** (`pairwise_sum_error_bound`) — the O(log N) pairwise-summation
  bound — is the open obligation, stated here as an `axiom` placeholder. Phase 2
  discharges it, lifting `RneValue.rneQuot_halfulp` into the per-add `(1+δ)`
  lemma and inducting over the tree.
- The composition (`scaled_dot_error_bound`) is a real `theorem`, sorry-free,
  reducing the end-to-end bound to Theorem B via the two exactness axioms and
  one named rational-algebra fact (`ratAbs_sub_mul_le`, also a Phase-2 lemma).

Everything is over `Rat`, not `ℝ`: a finite FP32 value is a dyadic rational, so
`Rat` represents it exactly and keeps this development Mathlib-free, matching the
rest of `ArchFpEquiv`.
-/

namespace ArchFp

/-! ## Element / scale widening (opaque in Phase 1)

`arch_e<elem>_to_f32` and `arch_e8m0_to_f32` are not yet ported into `Model.lean`
(only the f32/bf16/fp8 *scalar* ops are). Phase 3 replaces these `opaque`
declarations with the ported definitions; the frame below depends only on their
existence and their exactness axioms, not their internals. -/

/-- Widen a block *element* (E2M1…E4M3, ≤8 bits) to FP32. -/
opaque widenElem : BitVec 8 → BitVec 32

/-- Widen an E8M0 block *scale* to FP32 (a power of two, or NaN at `0xFF`). -/
opaque widenScale : BitVec 8 → BitVec 32

/-- Exact rational value of a **finite** FP32. Opaque in Phase 1; Phase 2
    defines it (finite FP32 are dyadic rationals). On non-finite inputs the
    value is unconstrained — every theorem below carries an all-finite
    hypothesis, exactly as the block value rule scopes it. -/
opaque f32ToRat : BitVec 32 → Rat

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
def archProducts (ea eb : List (BitVec 8)) : List (BitVec 32) :=
  (ea.zip eb).map (fun p => arch_f32_mul (widenElem p.1) (widenElem p.2))

/-- The full block dot: pairwise-sum the exact products, then apply the two
    block scales **one at a time** — `((S · Xa) · Xb)`, never `(Xa·Xb)·S`
    (pre-forming the scale product can overflow to Inf even when the result is
    representable; see `sv_dot`). -/
def archScaledDot (sa sb : BitVec 8) (ea eb : List (BitVec 8)) : BitVec 32 :=
  arch_f32_mul
    (arch_f32_mul (archPairwiseSum (archProducts ea eb)) (widenScale sa))
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
def exactBlockDot (sa sb : BitVec 8) (ea eb : List (BitVec 8)) : Rat :=
  ratSum ((ea.zip eb).map (fun p => f32ToRat (widenElem p.1) * f32ToRat (widenElem p.2)))
    * f32ToRat (widenScale sa) * f32ToRat (widenScale sb)

/-- FP32 unit roundoff, `u = 2⁻²⁴`. -/
def f32u : Rat := 1 / (2 ^ 24)

/-- The standard pairwise-summation growth factor `γ_d = d·u / (1 − d·u)` for a
    tree of depth `d = ⌈log₂ N⌉` (Higham, ASNA Thm 4.6). -/
def gammaPairwise (d : Nat) : Rat := (d * f32u) / (1 - d * f32u)

/-! ## The obligations

Each `axiom` below is a *named proof debt*, discharged in a later phase. They are
axioms rather than `sorry` so the file stays sorry-free and each obligation is
greppable and individually citable. -/

/-- **Products are exact** — the `MX_DOT` SMT gate, stated at the rational level:
    a widened-element product carries no rounding. Proved exhaustively per format
    by `fp_smt_proof::MX_DOT`; Phase 3 imports it. -/
axiom products_exact (x y : BitVec 8) :
    f32ToRat (arch_f32_mul (widenElem x) (widenElem y))
      = f32ToRat (widenElem x) * f32ToRat (widenElem y)

/-- **The scale multiply is exact** — a power-of-two E8M0 scale multiplies a
    finite FP32 value exactly, absent over/underflow (the `NoScaleOverflow`
    side condition, discharged from the MX magnitude ranges in Phase 2). -/
axiom NoScaleOverflow : BitVec 32 → BitVec 8 → Prop
axiom scale_mul_exact (v : BitVec 32) (s : BitVec 8) (h : NoScaleOverflow v s) :
    f32ToRat (arch_f32_mul v (widenScale s)) = f32ToRat v * f32ToRat (widenScale s)

/-- **Theorem B — the pairwise-summation error bound (open obligation).**
    For an all-finite, overflow-free product list of length `≤ 2^d`, the FP32
    balanced-pairwise sum differs from the exact rational sum by at most
    `γ_d · Σ|Pᵢ|`. Phase 2 discharges this via the `(1+δ)` per-add lemma
    (`RneValue.rneQuot_halfulp`) and induction over the `pairUp` tree.

    `AllFiniteNoOverflow` abstracts the side conditions §2 of the scope doc
    names (all partial sums finite; subnormal handling folded in). -/
axiom AllFiniteNoOverflow : List (BitVec 32) → Prop
axiom pairwise_sum_error_bound (ps : List (BitVec 32)) (d : Nat)
    (hlen : ps.length ≤ 2 ^ d) (hfin : AllFiniteNoOverflow ps) :
    ratAbs (f32ToRat (archPairwiseSum ps) - ratSum (ps.map f32ToRat))
      ≤ gammaPairwise d * ratSum (ps.map (fun p => ratAbs (f32ToRat p)))

/-- **Rational-algebra fact (Phase-2 lemma).** For `c ≥ 0`,
    `|x − y| · c = |x·c − y·c|` and `≤` is preserved under multiplying by `c`.
    Packaged as the single monotone-multiply step the composition needs; pure
    `Rat`, no FP content. Phase 2 proves it (trivial with Mathlib's ordered-field
    lemmas; by hand otherwise). -/
axiom ratAbs_sub_mul_le (x y bound c : Rat) (hc : 0 ≤ c)
    (h : ratAbs (x - y) ≤ bound) :
    ratAbs (x * c - y * c) ≤ bound * c

/-! ## Provable now — product-exactness lifts to the sum, and the scale pull

These two lemmas need no rational *algebra* (no comm/assoc/distrib), only the
exactness axioms and structural list/rewrite steps, so they are discharged here
rather than deferred. Together with the base-case model lemmas they are the
proved core of Phase 1; the composition below rests on them plus the three
obligation axioms. -/

/-- Product-exactness lifts through the summation: the rational sum of the FP32
    products equals the exact rational sum of the mathematical products. Pure
    structural induction over `products_exact` — the `MX_DOT` fact under a sum. -/
theorem products_sum_exact (ea eb : List (BitVec 8)) :
    ratSum ((archProducts ea eb).map f32ToRat)
      = ratSum ((ea.zip eb).map
          (fun p => f32ToRat (widenElem p.1) * f32ToRat (widenElem p.2))) := by
  unfold archProducts ratSum
  generalize ea.zip eb = l
  induction l with
  | nil => rfl
  | cons hd tl ih =>
    simp only [List.map_cons, List.foldr_cons]
    rw [products_exact hd.1 hd.2, ih]

/-- The two block scales pull out of the FP32 result exactly (one at a time,
    left-associated as the hardware applies them). Just the scale-exactness
    axiom twice — no algebra. -/
theorem archScaledDot_scale_pull (sa sb : BitVec 8) (ea eb : List (BitVec 8))
    (hov1 : NoScaleOverflow (archPairwiseSum (archProducts ea eb)) sa)
    (hov2 : NoScaleOverflow
      (arch_f32_mul (archPairwiseSum (archProducts ea eb)) (widenScale sa)) sb) :
    f32ToRat (archScaledDot sa sb ea eb)
      = f32ToRat (archPairwiseSum (archProducts ea eb))
          * f32ToRat (widenScale sa) * f32ToRat (widenScale sb) := by
  unfold archScaledDot
  rw [scale_mul_exact _ sb hov2, scale_mul_exact _ sa hov1]

/-! ## The end-to-end block-dot error bound (composition) -/

/-- **`scaled_dot` accumulation correctness — assembled.**

    The hardware block dot differs from the exact rational dot of the widened
    operands by at most `γ_d · (Σ|Pᵢ|) · Xa · Xb`, i.e. the pairwise-summation
    bound scaled by the (non-negative) block scales. This is the block-dot
    analogue of the FMA value theorem — a certified bound rather than an
    equality, since a multi-add is not exactly rounded.

    Sorry-free, resting on exactly the three named obligations
    (`pairwise_sum_error_bound`, the two exactness axioms, and the one
    rational-algebra fact) — all discharged in Phases 2–3. -/
theorem scaled_dot_error_bound
    (sa sb : BitVec 8) (ea eb : List (BitVec 8)) (d : Nat)
    (hlen : (archProducts ea eb).length ≤ 2 ^ d)
    (hfin : AllFiniteNoOverflow (archProducts ea eb))
    (hov1 : NoScaleOverflow (archPairwiseSum (archProducts ea eb)) sa)
    (hov2 : NoScaleOverflow
      (arch_f32_mul (archPairwiseSum (archProducts ea eb)) (widenScale sa)) sb)
    (hsa : 0 ≤ f32ToRat (widenScale sa)) (hsb : 0 ≤ f32ToRat (widenScale sb)) :
    ratAbs (f32ToRat (archScaledDot sa sb ea eb) - exactBlockDot sa sb ea eb)
      ≤ gammaPairwise d
          * ratSum ((archProducts ea eb).map (fun p => ratAbs (f32ToRat p)))
          * f32ToRat (widenScale sa) * f32ToRat (widenScale sb) := by
  -- B, at the product list: |sum_fp − Σ f32ToRat Pᵢ| ≤ γ_d · Σ|Pᵢ|.
  have hB := pairwise_sum_error_bound (archProducts ea eb) d hlen hfin
  -- Multiply the bound through by Xa (≥0), then Xb (≥0) — one scale at a time.
  have h1 := ratAbs_sub_mul_le
    (f32ToRat (archPairwiseSum (archProducts ea eb)))
    (ratSum ((archProducts ea eb).map f32ToRat))
    (gammaPairwise d * ratSum ((archProducts ea eb).map (fun p => ratAbs (f32ToRat p))))
    (f32ToRat (widenScale sa)) hsa hB
  have h2 := ratAbs_sub_mul_le
    (f32ToRat (archPairwiseSum (archProducts ea eb)) * f32ToRat (widenScale sa))
    (ratSum ((archProducts ea eb).map f32ToRat) * f32ToRat (widenScale sa))
    (gammaPairwise d * ratSum ((archProducts ea eb).map (fun p => ratAbs (f32ToRat p)))
      * f32ToRat (widenScale sa))
    (f32ToRat (widenScale sb)) hsb h1
  -- Rewrite the goal's two sides into h2's shape: the hardware side via the
  -- scale pull, the exact side via products-sum-exactness. Both groupings are
  -- left-associative by construction, so this is definitional — no algebra.
  rw [archScaledDot_scale_pull sa sb ea eb hov1 hov2, exactBlockDot,
      ← products_sum_exact ea eb]
  exact h2

end ArchFp
