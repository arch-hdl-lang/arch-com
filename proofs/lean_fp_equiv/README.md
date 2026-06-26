# ARCH FP — Lean equivalence backend (prototype)

A third renderer of the **one** in-Rust description of each FP operator. The
operators are defined once in `src/fp_ops.rs` against the shared bit-vector IR
(`src/fp_ir.rs`); that IR renders to:

| renderer | output | consumer |
|---|---|---|
| `render_sv` | synthesizable SystemVerilog | `arch build` |
| `render_smt` | SMT-LIB2 `define-fun`s | `arch formal` / z3 / cvc5 (`tests/fp_test.rs`) |
| **`render_lean`** | **Lean 4 `BitVec` `def`s** | **this project** |

Because all three come from the same source, a Lean proof here transfers to the
synthesized RTL with no hand-transcription — the same guarantee the SMT backend
already gives, extended to a structured prover.

## Why add Lean to a working SMT campaign

The SMT backend discharges, *exhaustively*, everything that has no multiplier:
f32 compares / conversions / `add` / `sub` over all 2^64 inputs, and all bf16
arithmetic over 2^32 (see `tests/fp_v1/smt_proof/README.md`). The wall is the
**multiplier-bearing** f32 ops, `mul` and `fma`: a 24×24 multiplier-equivalence
miter is SAT-hard for any bit-blaster — z3, cvc5, and Lean's `bv_decide` alike.

A structured prover clears that wall by *not bit-blasting*. Following the FLoPS /
Flocq "triangle of correctness", the proof lifts the bit pattern to an algebraic
`(sign, significand, exponent)` view with a rational/real value and shows the
operator computes `RoundToNearestEven(a·b)` structurally — the 24×24 array
collapses to a single `Nat`/`Int` multiply whose algebra Mathlib already knows.
That reasoning has no natural home in QF_BV; it does in Lean.

## Layout

```
lakefile.toml            dependency-free (Lean core BitVec + bv_decide; no Mathlib)
lean-toolchain           leanprover/lean4:v4.30.0  (matches proofs/lean_thread_lowering)
ArchFpEquiv.lean         root: imports Model + Spec + Equiv
ArchFpEquiv/Model.lean   GENERATED snapshot of every operator (regen below)
ArchFpEquiv/Spec.lean    Tier-2 multiply: special-value lattice + finite reduction (proved)
ArchFpEquiv/Round.lean   Tier-2 rounder: sign/zero/exact-round-trip + 1·x=x + msb bridge (proved)
ArchFpEquiv/RoundCore.lean  Tier-2 rounding kernel: guard/sticky = nearest-even (proved, pure Nat)
ArchFpEquiv/Equiv.lean   Tier-1 structural lemmas (proved) + the rounder crux + derived mul
scripts/regen_model.sh   re-render Model.lean from the IR
```

## Regenerate the model

```
proofs/lean_fp_equiv/scripts/regen_model.sh
# == cargo run --release --example dump_fp -- lean  (+ provenance header)
```

`Model.lean` is checked in as a snapshot so the project reads standalone, but it
is generated — never edit it by hand; change `src/fp_ops.rs` and regenerate.

## Status

Built clean under **Lean v4.30.0** (`lake build`). The model elaborates and five
real theorems are **machine-checked by `bv_decide`** (driving the bundled
`cadical`); the only remaining `sorry`s are the three Tier-2 IEEE theorems, which
need a floating-point semantics for the spec side.

**Proven (`bv_decide`, no IEEE spec needed — pure `BitVec` facts about the real
emitted operators):**

| theorem | what it establishes | solver |
|---|---|---|
| `arch_f32_eq_comm` | equality comparator is symmetric (incl. NaN) | instant |
| `arch_f32_lt_gt_mirror` | `lt a b = gt b a` (the swapped-operand construction) | instant |
| `arch_bf16_eq_comm` | bf16 compare symmetry through the widen path | instant |
| `arch_f32_sub_is_add_neg` | `sub a b = add a (b ⊕ sign)` — the shared adder core was wired right | ~34 s |
| `arch_f32_add_comm` | **full f32-adder datapath is commutative** (align + rounder) | ~45 s |

`add_comm` is the load-bearing one: it bit-blasts the *entire* ~56-bit adder, and
because commutativity is non-symmetric in the operands, no abstraction shortcut
could fake it — so it also proves the bit-blast is genuine. Getting it to go
through is what forced the comparison encoding to be `BitVec.ofBool` rather than a
`Prop`-conditioned `if` (which `bv_decide` abstracts as an opaque variable).

**Tier 2 — IEEE arithmetic (`Spec.lean` + `Equiv.lean`).** The multiply case is
carried most of the way:

| result | status |
|---|---|
| multiply special-value lattice (NaN prop, `∞·0=NaN`, `∞·x=∞`, `0·x=0`) — 8 laws | **proved** (`bv_decide`; the rounder/multiplier branch is pruned, so each is sub-second) |
| `mul_finite_reduces`: `mul a b = round48(sy, mant_a·mant_b, e0)` for finite nonzero | **proved** (`bv_decide`, structural — the 24×24 multiplier is identical on both sides, *not* a multiplier-equivalence) |
| `arch_f32_mul_finite_correct`: finite `mul` is RNE of the exact product | **derived** from the reduction + the rounder crux below |
| rounder shape: `round48_sign` (sign preserved), `round48_zero` (0 → ±0) | **proved** (`bv_decide`, `Round.lean`) |
| `round48_exact_normal` / `round48_exact_subnormal`: rounding any representable value is the identity | **proved** — `arch_round48_correct` on the entire *exact* sub-domain |
| `mul_one_left` / `mul_one_right`: `1·x = x` for every non-NaN `x` | **proved** end-to-end (constant operand → no variable multiplier) |
| `msb_index_finds_msb`: the binary-search clz finds the true MSB (`sig >>> p = 1`) | **proved** (`bv_decide`, exhaustive over 2^48) |
| `msb_index_bound`: value-level `2^p ≤ sig < 2^(p+1)` | **proved** (bit→`Nat` bridge, Lean-core lemmas only — no Mathlib) |
| `RoundCore.rne_matches`: guard/round/sticky = round-to-nearest-even integer division | **proved** (pure `Nat`, the rounding kernel) |
| `arch_round48_correct`: the shared rounder rounds correctly | **open** (the one `sorry`) — only bit-level plumbing into the two kernels remains |

### The residual, and why it is now small

The inexact rounding direction — all that is left of `arch_round48_correct` — has
been reduced to two arithmetic kernels, **both proved with Lean core alone** (the
Mathlib olean cache is egress-blocked here, so this was done without it):

* `Round.msb_index_bound` — normalization: arch's binary-search clz finds the true
  MSB, giving `2^p ≤ sig < 2^(p+1)` (`bv_decide` for the bit fact, `Nat` lemmas +
  `omega` for the bound).
* `RoundCore.rne_matches` — rounding: arch's `guard ∧ (sticky ∨ odd)` equals
  `rneQuot` (round-to-nearest-even of `n / 2^sh`), where `guard ⟺ half ≤ r`,
  `sticky ⟺ r % half ≠ 0`, ties-to-even — proved in pure `Nat`.

What remains is the bit-level *plumbing* that threads `arch_round48`'s concrete
shifts and exponent computation into these two kernels (e.g. `(zsig >>> sh).toNat
= zsig.toNat / 2^sh` via `BitVec.toNat_ushiftRight`, already used in
`msb_index_bound`). That is mechanical bridge work, not new mathematics.

This is the whole point of the algebraic-lifting approach landing concretely: the
multiplier theorem no longer faces a SAT wall. `mul`'s special values are proved,
and its finite case is *reduced* — `bv_decide` handles the reduction because the
multiplier occurs identically on both sides (like `sub_is_add_neg`), so it never
solves multiplier-equivalence. Everything collapses onto a single op-independent
lemma, `arch_round48_correct`: the shared `normround` (exposed to Lean as
`arch_round48`) rounds its dyadic argument `(-1)^s · sig · 2^e0` to nearest-even.
That lemma is value-level (not bit-blastable) and is where a dyadic/`Rat`
semantics (Mathlib/Flocq, or a port of the SMT `FloatingPoint` theory) is needed.
Being shared, discharging it also unlocks `add`/`fma` once their pre-rounding
values are named the same way. (`add`/`sub` are independently already machine-
proved over all 2^64 inputs by the SMT backend.)

`arch_round48`/`arch_decode_mant`/`arch_decode_eunb` are emitted into the Lean
`Model` only (via `fp_ops::lean_extra_functions`); the `arch build` SV and `arch
formal` SMT are byte-for-byte unchanged.

**Also verified (`cargo test`):** the `render_lean` renderer and its output
(`src/fp_ir.rs` unit tests `renders_both_dialects`, `lean_renders_every_op_kind`).

## Build

```
cd proofs/lean_fp_equiv && lake build
```

Requires the `leanprover/lean4:v4.30.0` toolchain (elan reads `./lean-toolchain`).
No external packages — Lean core `BitVec` + the bundled `bv_decide`/`cadical`.
`add_comm` (~45 s) and `sub_is_add_neg` (~34 s) carry raised `bv_decide`
`timeout`s; the rest are instant.
