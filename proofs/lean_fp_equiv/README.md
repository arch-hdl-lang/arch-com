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
ArchFpEquiv.lean         root: imports Model + Equiv
ArchFpEquiv/Model.lean   GENERATED snapshot of every operator (regen below)
ArchFpEquiv/Equiv.lean   the correctness statements (sorry-stubbed scaffold)
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

**Open (`sorry`, Tier 2 — the multiplier frontier):** `arch_f32_mul_correct`,
`arch_fma_f32_correct`, `arch_f32_add_correct`. These compare against `opaque
f32_spec_*`, placeholders for an IEEE-754 semantics Lean core does not provide. A
real development supplies it from Mathlib/Flocq (or by porting the SMT
`FloatingPoint` theory), then discharges `mul`/`fma` by algebraic lifting rather
than bit-blasting. `add` is the cross-check (already machine-proved over 2^64 by
the SMT backend) and the simplest worked example of that lifting.

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
