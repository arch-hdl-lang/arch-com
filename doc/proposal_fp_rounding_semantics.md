# Proposal: a normative FP rounding-semantics contract for ARCH

Status: research note / discussion. No implementation in this note.

## Motivation

ARCH's floating-point support (FP32 / BF16, #609) is built on a clean idea:
BF16 arithmetic is `widen → f32 op → narrow`, and a single shared rounder backs
every op. The trouble is that the *numeric contract* of each op and conversion —
**"is the result correctly-rounded, or merely convention-rounded via an f32
intermediate?"** — was never written down as a normative rule. It lived only in
code comments, and those comments asserted a uniform "correctly-rounded /
innocuous double rounding" story that is **not actually uniform**.

Two latent defects of the *same class* have now surfaced, both because the
`p₁ ≥ 2·p₂ + 2` "innocuous double rounding" argument was applied where it does
not hold:

1. **`bf16_fma` is not correctly-rounded** (PR #627). It is fused
   f32-accumulate; the final f32→bf16 narrow is a second, non-innocuous rounding.
   Differs from correctly-rounded `a·b+c` on ~0.37 % of finite inputs.
2. **`int.to_bf16()` is not correctly-rounded** (issue #629). Same `int→f32→bf16`
   double rounding; differs for `|i| ≥ 2²⁴` (witness `16842753 → 0x4b80`,
   correct `0x4b81`; 8064 cases in `[1, 2³⁰)`).

Both were found by ad-hoc audit, not by any structural guard. The
`p₁ ≥ 2·p₂ + 2` rule is a **known fallacy** for round-to-nearest applied to an
arbitrary real (it fails already at `p₁ = 4, p₂ = 1`:
`RNE₁(RNE₄(1.4375)) = 2` but `RNE₁(1.4375) = 1`), yet it was used as blanket
justification across `fp_ops.rs`, `sim_codegen`, and `doc/archive/plan_fp_types.md`.

The risk is open-ended: every *future* narrow format (fp16, fp8 E4M3/E5M2 — the
natural next step for LLM inference, cf. #622) re-introduces the same hazard for
every op and conversion routed through a wider intermediate. Without a written
contract, each one is a fresh place to silently assume "correctly-rounded."

## Proposal

Add one normative subsection to the spec (and the AI reference card) — **"FP
rounding semantics"** — that, for every FP op and conversion, classifies the
result as exactly one of:

- **CR** — *correctly-rounded* to the destination format (RNE), bit-exact to the
  IEEE-754 / SoftFloat single-rounding result.
- **VR(f32)** — *convention-rounded via an f32 intermediate*: defined as
  `narrow_dst(arch_*_f32(widen(...)))`, i.e. one f32 rounding then a destination
  narrow. May differ from CR by ≤ 1 ULP on inputs where the double rounding is
  not innocuous.

A first cut of the table from today's implementation:

| Op / conversion | Class | Notes |
|---|---|---|
| f32 `add` `sub` `mul` `fma` | **CR** | mul/fma Lean-proved (#625/#626); add/sub exhaustively SMT-proved |
| f32 compares, f32↔int | **CR** | exhaustive SMT |
| bf16 `add` `sub` `mul` | **CR** | exhaustively SMT-proved vs `fp.{add,sub,mul}` on (8,8) |
| bf16 `fma` | **VR(f32)** | fused f32-accumulate (#627); NVIDIA/TPU convention |
| `int → bf16` | **VR(f32)** | #629; not CR for `\|i\| ≥ 2²⁴` |
| `f32 → bf16` narrow | **CR** | RNE, bit-identical to PyTorch `c10::BFloat16` |

The classification is **load-bearing for verification**: a CR op is a legitimate
target for an `fp.*` SMT miter or a Lean `roundNE` proof; a VR(f32) op is **not**
(its miter has a genuine `sat`, as #627 demonstrated — the failing bf16_fma miter
was mistaken for a z3 soundness gap for exactly this reason). Writing the class
down stops the next person from chasing a "sound `fp.fma` solver" to close a
miter that should never close.

## Optional enforcement (later, not required by this note)

1. **A characterization test per VR(f32) op** pinning it to
   `narrow(arch_*_f32(...))` and asserting it is *not* CR via a stored witness —
   exactly the shape #627 added for `bf16_fma` (`archBf16Fma_eq_narrow_roundNE`).
   #629 should get the int→bf16 analogue.
2. **A `--strict-fp` build mode** that lowers VR(f32) ops to their CR form where
   one exists (direct int→bf16 rounding; a true bf16 fma), for users who need
   IEEE correctly-rounded results over hardware-convention accuracy. Default
   stays the hardware-realistic VR(f32) path.
3. **A lint** (`arch check`) that flags any *new* `narrow(wide_op(widen(...)))`
   lowering not present in the classification table, so the next narrow format
   can't add an unclassified VR op silently.

## Why not just "make everything correctly-rounded"

Because VR(f32) is frequently the *intended, hardware-faithful* behavior, not a
bug: NVIDIA Tensor Cores and TPUs do f32-accumulate bf16 fma, and no mainstream
ISA has a direct int→bf16 instruction. Forcing CR everywhere would make ARCH's
sim/RTL *diverge from the hardware it models*. The goal is not to eliminate
VR(f32) but to **name it**, so "correctly-rounded" is a claim the spec makes
deliberately and verification can rely on, never an unexamined default.

## Related

- #609 FP types (v1); #622 context-typed float literals (fp8/fp16 horizon)
- #627 bf16_fma f32-accumulate characterization; #629 int→bf16 double-rounding
- PR #628 corrected the stale "correctly-rounded / innocuous" comments this note
  generalizes from
