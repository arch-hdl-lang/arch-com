# Scope: `scaled_dot` accumulation proof

**Goal.** Close the one formal gap that separates `scaled_dot` from FMA: FMA has
a machine-checked *whole-operation* value theorem (`arch_fma_f32 =
round-to-nearest of the exact `a·b+c``, Lean, sorry-free); `scaled_dot` today
has only a *leaf* theorem (`MX_DOT`: every element-pair product is exact in
FP32). The **accumulation** — the summation tree that reduces the N products —
has no value theorem and no error bound. This document scopes the proof that
closes it.

Status: **Phase 1 landed** (skeleton + structural miter); **Phase 2 partial**
(`ratAbs_sub_mul_le` proved in core `Rat`; the `(1+δ)` add bridge is open and
gated on an `arch_f32_add` correctly-rounded lemma that does not yet exist).
Theorem B remains (Phase 3). See §6.

---

## 1. What the datapath actually computes

From `src/fp_block.rs` (`dot_schedule`, `sv_dot`, `cpp_dot`) and the module
header, a block dot of two `ScaledVec<Elem, N, Scale>` operands lowers to:

```
d_hw = fl_pow2( X^A · X^B · S )        where  S = fl_pairwise( Σ_{i<N} P_i )
       P_i = w(a_i) × w(b_i)           (element-pair products, widened to FP32)
       X^A, X^B                        (the two blocks' shared E8M0 scales)
```

Two facts already proved elsewhere collapse everything except the sum:

- **Products are exact.** `fp_smt_proof::MX_DOT` proves each `P_i` is exact in
  FP32 (widest significand 8 bits, exponent span inside FP32 normals) — so
  `fl(w(a_i) × w(b_i)) = w(a_i)·w(b_i)` over ℝ, and `Σ_{i} P_i` has no
  per-product rounding.
- **The scale multiply is exact.** `X^A`, `X^B` are E8M0 codes = powers of two;
  their product is a power of two; multiplying an FP32 value by a power of two
  is exact (barring over/underflow). So `d_hw = X^A·X^B · S` exactly.

**Therefore the entire accuracy question is the single quantity
`S = fl_pairwise(Σ P_i)`** — the FP32 pairwise-RNE summation of exact products.
This is the crux the proof must attack, and nothing else.

`dot_schedule` fixes the summation order (OCP MX §6.2 leaves it
implementation-defined; FP32 add is non-associative, so an unstated order is an
unstated result). The order is **balanced pairwise**: each round adds adjacent
pairs, a lone trailing element passes through untouched (no zero pad — adding
`+0.0` would flip `-0.0`), repeat until one remains. Depth `d = ⌈log₂ N⌉`.

---

## 2. The two theorems (they are different; split them)

### Theorem B — accumulation value/error bound  *(the core deliverable)*

A multi-add cannot be *exactly* rounded (unlike a single FMA), so the analogue
of the FMA theorem is a **certified relative error bound**, not an equality:

> **B.** For an all-finite block with exact products `P_i`,
> `| fl_pairwise(Σ_i P_i) − Σ_ℝ P_i | ≤ γ_d · Σ_i |P_i|`,
> where `d = ⌈log₂ N⌉`, `u = 2^-24` (FP32 unit roundoff),
> `γ_d = d·u / (1 − d·u)`.
> Composed with §1's two exactness facts:
> `| d_hw − X^A X^B · Σ_ℝ (w(a_i)·w(b_i)) | ≤ X^A X^B · γ_d · Σ_i |P_i|`.

This is the standard Higham pairwise-summation bound (ASNA Thm 4.6), and it is
what makes ARCH's *choice* of the pairwise schedule meaningful: it certifies the
`O(log N)` error growth the `dot_schedule` doc claims, versus serial's `O(N)`.
It is **N-generic** (parametric in N) — the decisive reason to do it in Lean and
not SMT, which can only ever discharge one fixed N.

Engine: **Lean**, extending `proofs/lean_fp_equiv/ArchFpEquiv`.

Reuses:
- `RneValue.rneQuot_halfulp` — per-add half-ULP error. This is the atom. The
  first sub-task is lifting it from arch's integer-quotient form into a clean
  real-valued **`(1+δ)` add lemma**: `fl(a+b) = (a+b)(1+δ), |δ| ≤ u`. This
  bridge is the critical-path risk (the FMA proof uses `rneQuot` only inside a
  specific magnitude-scaled context; a standalone `f32Add_rel_error` may be new
  work rather than a citation).
- `RoundReal` grid/ULP machinery (`f32MagScaled`, `scaledDist`, half-ULP
  lemmas) for the bridge above.
- Pairwise induction over the binary tree is standard once the atom exists;
  mathlib may not carry the pairwise bound, so budget it as fresh but routine.

Side conditions that must be discharged (naming them because they are the real
work, not the induction):
1. **No overflow / all partials finite.** MX products are bounded
   (E4M3²≈448², etc.) and `N ≤ 32`, so `Σ|P_i|` is far inside FP32 range —
   provable as a format-range side condition, not an assumption to hand-wave.
2. **Subnormal underflow.** If any partial sum can land subnormal, the pure
   relative bound needs the standard additive `+ (N−1)·η/2` term (η = smallest
   subnormal). Check whether MX product magnitudes exclude the subnormal region;
   if not, carry the additive term (still a clean certified bound).
3. **Unbalanced leaves.** The odd-element pass-through means leaves flow through
   differing add counts; `d = ⌈log₂ N⌉` still upper-bounds any leaf's add depth.
   The induction must be over the actual `dot_schedule` tree shape, not an
   idealized perfect tree.
4. **Non-finite inputs.** State B conditional on an all-finite block; the
   NaN/Inf path is handled structurally by the integer-max "any non-finite"
   detection (§1 of the module header) and is out of scope for the value bound.

### Theorem A — structural faithfulness of the emitted SV  *(lighter, separate)*

> **A.** The emitted `ScaledDot` SV module computes exactly `dot_schedule`'s
> tree of `arch_f32_add` / `arch_f32_mul` nodes.

This is the renderer-faithfulness half (the analogue of the FMA
`renderer_miter.sh` row). Do **not** attempt it as a monolithic miter or Lean
proof: a full-FP32 adder *tree* miter over `2^(32N)` inputs is SAT-hard for
N ≥ 4. Instead:

- The tree is already generated from one descriptor (`dot_schedule`), rendered
  identically by both backends, and pinned by `fp_block_sv_and_cpp_agree_on_shape`
  (structure) + `tests/fp_v1/rtl_diff` (values, Verilator vs the C++ DPI ref).
  Generator uniformity carries the general-N wiring.
- Add a **small-N SMT miter** — N = 2 (a single `arch_f32_add`, trivially
  tractable), optionally N = 4 — of `render_sv(ScaledDot_N)` against a `define-fun`
  `fp.add`/`fp.mul` tree, reusing `renderer_miter.sh` machinery. This pins node
  semantics + base-case wiring formally; the generator's uniformity extends it.

A is cheap and closes the "the SV faithfully implements the *defined* schedule"
question at the base cases. B is the substance.

---

## 3. Composition — how the pieces yield an end-to-end statement

```
MX_DOT (SMT, done)          : P_i exact in FP32
pow2-scale-exact (cite)     : X^A·X^B multiply exact
Theorem B (Lean, new)       : | fl_pairwise(ΣP_i) − ΣP_i | ≤ γ_d Σ|P_i|
Theorem A (miter, new)      : emitted SV ≡ dot_schedule tree
────────────────────────────────────────────────────────────────────
⇒  end-to-end certified block-dot error bound, hardware-anchored,
   bringing scaled_dot to FMA's assurance level.
```

The end-to-end statement is intentionally a *bound*, not an equality — that is
the honest analogue of FMA's exact-rounding result for a non-associative
multi-add, and it is exactly what a numerics/paper audience wants (it justifies
the schedule).

---

## 4. Deliverables

1. `ArchFpEquiv/F32AddRel.lean` — the `(1+δ)` real-valued add lemma from
   `rneQuot_halfulp` (the bridge; critical path).
2. `ArchFpEquiv/PairwiseSum.lean` — N-generic pairwise bound over a `dot_schedule`
   tree, with side conditions 1–4 discharged.
3. `ArchFpEquiv/ScaledDot.lean` — composition with `MX_DOT` exactness + pow2
   scale ⇒ the end-to-end block-dot bound. Root import added to `ArchFpEquiv.lean`.
4. Small-N structural miter row(s) in `tests/fp_v1/smt_proof/` (Theorem A).
5. Paper §9 update: replace "accumulation is an unproven design choice" with the
   certified `O(log N)` bound; note it justifies pairwise over serial.

## 5. Effort / risk

- **Bulk:** the `(1+δ)` bridge (item 1) + side conditions 1–2. If the bridge is
  a clean citation the whole thing is ~a focused Lean push; if `rneQuot`'s
  formulation resists a standalone real-valued lemma, item 1 dominates.
- **Routine:** pairwise induction (item 2 core), the structural miter (item 4).
- **Decision to confirm before starting:** prove B against the **exact real dot
  of the widened operands** (recommended — clean, meaningful) rather than against
  "fp.dot in some canonical order" (circular — just re-pins the order).
- **Not a spec change.** This proves a property of the *already-defined*
  schedule; no user-facing syntax/semantics move. Autonomous-fixable under the
  repo rules, modulo the reference-choice confirmation above.

## 6. Suggested phasing

- **Phase 1 (fast win) — DONE.**
  - *Theorem A miter* (`tests/fp_v1/smt_proof/scaled_dot_miter.sh`): the emitted
    SV is machine-checked (`unsat`) to implement exactly the balanced-pairwise
    schedule with one-at-a-time scale application, against a hand-composed
    define-fun tree built from the already-checked atomic nodes. Proven for
    `ScaledDotE4m3N2` (25 s) and `ScaledDotE2m1N4` (94 s, a genuine two-level
    tree). `ScaledDotE4m3N4` (widest significand into a variable-alignment add)
    is confirmed SAT-hard and — unlike fma — a case-split does NOT rescue it (see
    the case-split investigation below); wide/large shapes are covered by the
    wiring test instead.
  - *Theorem A, wiring leg* (`tests/scaled_dot_wiring_test.rs`, runs in CI):
    checks the emitted `arch_scaled_dot_*` function IS `dot_schedule`'s
    composition of the atomic nodes, by comparing its assignment sequence to an
    independent balanced-pairwise reimplementation — for all N (incl. odd
    pass-through) and all element formats. With node faithfulness from
    `renderer_miter.sh`, this discharges Theorem A for the shapes SMT cannot
    reach. Mutation-checked non-vacuous (swapped add operands fail).
  - *Lean frame* (`ArchFpEquiv/ScaledDot.lean`, sorry-free, dependency-free over
    `Rat`): faithful `archPairwiseSum`/`archProducts`/`archScaledDot` models with
    proved base cases (N=1,2,4); `products_sum_exact` and `archScaledDot_scale_pull`
    proved from the exactness axioms; and the end-to-end `scaled_dot_error_bound`
    proved as a real theorem resting on the three named obligations
    (`pairwise_sum_error_bound` = Theorem B, the two exactness axioms already
    backed by SMT, and one `Rat`-algebra fact).
- **Phase 2 — PARTIALLY DONE (2026-08-24).**
  - *`ratAbs_sub_mul_le`: DONE.* Now a proved `theorem` in core `Rat` (no
    Mathlib): the `Rat.` namespace supplies enough
    (`mul_le_mul_of_nonneg_right`, `abs_of_nonneg/nonpos`, `mul_nonneg`,
    `neg_mul`, `le_iff_sub_nonneg`) — the generic `mul_comm`/`mul_assoc`/
    `mul_le_mul_*` that are missing turned out not to be needed once every
    grouping is kept left-associative. **So the Mathlib decision does NOT bind
    for the algebra.**
  - *The `(1+δ)` per-add bridge: OPEN, and larger than first scoped.* Two
    prerequisites, discovered while surveying the dev:
    1. **`arch_f32_add` is not yet proved correctly-rounded.** The dev has only
       `Equiv.arch_f32_add_comm`; there is no `arch_f32_add a b =
       round-to-nearest(val a + val b)` lemma. The value/nearest machinery
       (`RoundReal.f32MagScaled`, `IsNearestMag`, `RneValue.rneQuot_halfulp`) is
       all in **Nat-scaled magnitude**, and the only end-to-end value proof
       chained from it is the *fused* fma. Proving the bare adder correctly
       rounds (alignment produces the exact sum significand, then the proven
       round kernel) is an fma-scale sub-project.
    2. **Nat→Rat bridge.** `rneQuot_halfulp` is a half-ULP bound in Nat-scaled
       units; the `(1+δ)` form needs it as a `Rat` *relative* error, i.e. a
       concrete `f32ToRat` and its link to `f32MagScaled`.
  - **Mathlib decision (revised):** not needed for the algebra; still open for
    the analytic `(1+δ)` step and Theorem B's induction. Recommendation: keep the
    development Mathlib-free through the Nat-magnitude layer where the existing
    proofs live, and only reach for `require mathlib` if the summation induction
    proves unwieldy in `Rat` alone. Sequence prerequisite (1) first — it is the
    real gate, and it is reusable well beyond this proof.
- **Phase 3:** discharge Theorem B (`pairwise_sum_error_bound`) by induction over
  the `pairUp` tree (using the Phase-2 `(1+δ)` lemma); import the `MX_DOT` /
  `MX_SCALE_CONV` SMT results to retire `products_exact` / `scale_mul_exact`.

## 7. Case-split investigation (2026-08-24) — why the wide-shape miter is not SMT

The Phase-1 plan floated an fma-style alignment case-split to bring
`ScaledDotE4m3N4` under the SMT miter. Investigated and **rejected on evidence**:

- The fma miter's split works because fma has exactly **one** alignment gap
  (product vs. addend): 510 constant-gap sub-miters, each near-structural.
- A block dot is a reduction **tree**: one alignment gap *per add*, and they are
  coupled (the top add's gap depends on the leaf sums' exponents). Measurements:
  - monolithic `ScaledDotE4m3N4`: bitwuzla + z3 both time out (2401 s total);
  - the **pure** 4-way FP32 add-tree, no multipliers, also times out at 1200 s —
    so the adder barrel shifters alone are already the wall;
  - splitting on just the **top** add's gap still times out (900 s/case);
  - splitting on **all** gaps is ~35^depth cases — infeasible.
- There is no single splitting variable for a tree, so monolithic SMT does not
  scale here regardless of splitting.

Resolution: Theorem A is discharged **compositionally** for wide/large shapes —
node faithfulness (`renderer_miter.sh`, already `unsat`) + wiring faithfulness
(`tests/scaled_dot_wiring_test.rs`). Pure SV functions compose without context,
so the two legs give Theorem A for every shape; the small-N end-to-end miters
remain the bit-exact cross-check that the decomposition is sound.
