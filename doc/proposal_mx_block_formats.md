# Proposal: MX / NVFP4 block-scaled formats (`ScaledVec<Elem, N, Scale>`)

**Status:** **APPROVED** (maintainer sign-off recorded in §9, 2026-08-09).
Ready to implement, starting at Phase 0. This is a language-surface change
(new types, new operations).

**Sources:** OCP Microscaling Formats (MX) Specification v1.0 (2023-09-07);
OCP OFP8 v1.0 (incorporated by reference); NVIDIA PTX ISA 9.3 (normative for
`e2m1`/`ue8m0`/`ue4m3` and the block-scaled MMA kinds); NVIDIA arXiv 2509.25149
App. B (NVFP4 recipe); microsoft/microxcaling (reference emulation).
Spec modal verbs below are used exactly as MX §4.2 defines them: **must** =
required, **should** = recommended, **may** = optional.

---

## 1. Context and the governing insight

ML inference has moved to block-scaled sub-8-bit formats: OpenAI's gpt-oss ships
MXFP4 weights, NVIDIA Blackwell and AMD CDNA4/MI355X both implement MXFP4/6/8
and (NVIDIA) NVFP4 in silicon. ARCH already has a fully verified scalar float
line (FP32, BF16, FP8E4M3, FP8E5M2 — nine merged PRs through error-bound
formal). Block formats are the natural next step, and the queued tasks #15/#16
assumed "sub-8-bit scalars first, block formats second."

**That ordering was wrong, and researching the spec is what showed it.**

> **MX is an interchange format, not an arithmetic format.** MX §6 defines
> exactly one operation: `Dot(A,B) = X^A · X^B · Σᵢ(Pᵢ^A × Pᵢ^B)`. There is no
> defined add, multiply, or compare on blocks — and the element types below 8
> bits have no scalar arithmetic anywhere in any shipping ISA.

PTX states it outright: *"e2m1 values **must be used in a packed format**"* and
*"Alternate data formats **cannot be used as fundamental types.**"* Across 173
`e2m1` occurrences in PTX ISA 9.3 the format appears only in `cvt` conversions
and MMA matrix operands. AMD's path is the same shape.

So the block format is the unit of meaning, and it **dictates** what the scalars
need — which is far less than a full arithmetic surface.

---

## 2. What the spec pins vs. what it leaves to us

This section is the design's foundation: several things that look like details
are actually *required* degrees of freedom.

### 2.1 Pinned (normative)

| Item | Value |
|---|---|
| Block value rule | `X = NaN ⇒ vᵢ = NaN ∀i, element encodings ignored`; else `Pᵢ ∈ {Inf,NaN} ⇒ vᵢ = Pᵢ`; else `vᵢ = X·Pᵢ` |
| Named formats | MXFP8/6/4 and MXINT8 all fix **k = 32**, scale **E8M0 (w=8)** |
| E8M0 | bias 127, exponent range −127..127, **no Inf, no zero encoding**, NaN = `0xFF` |
| E2M1 | 1/2/1 bits, bias 1, **no Inf, no NaN**, max finite **±6.0**, min subnormal ±0.5 |
| E2M3 | 1/2/3, bias 1, no Inf/NaN, max ±7.5 | 
| E3M2 | 1/3/2, bias 3, no Inf/NaN, max ±28.0 |
| Subnormals | **must** be supported by every FP element type |
| Flush-to-zero | below min subnormal after rounding, **must** convert to zero |
| RNE | **must** be *available* as a rounding mode |
| Saturating clamp | **must** be *supported* as an overflow method |
| Block dot | `Dot` above; **must** be minimally supported |

**`E8M0` has no zero.** `0x00` is `2⁻¹²⁷`, the *minimum scale* — not zero, not
NaN. This is the single easiest thing to get wrong.

### 2.2 Deliberately open — we must choose, and expose the choice

| Item | Spec status |
|---|---|
| The FP32→MX conversion algorithm | **should** (§6.3) — recommended only |
| Rounding mode beyond RNE-available | **may** |
| Overflow method beyond clamp | **may**, "with a **configurable overflow attribute**" |
| Dot accumulator precision & reduction order | **implementation-defined** (§6.1) |
| FP32 dot result | **should** (§6.2) |
| Memory layout | **"not prescribed"** (§5.1) |
| Scale for an all-zero block | silent |
| Shared-exponent clamping into E8M0 range | silent |
| Element field when `X = 0xFF` | explicitly **out of scope** |
| `X·Pᵢ` outside FP32 range | **implementation-defined**, and *reachable* |
| MXINT8 `0x80` (−2 or illegal) | **may** be left unused |

**Two conformant implementations produce different bits from the same FP32
input.** OCP §6.3 says take the largest power-of-two ≤ max|Vᵢ|; NVIDIA rounds
the scale **up** instead, because *"we typically round decode scale factors up
to prevent saturations."* Both conform.

**Overflow is the common case, not a corner.** After scaling by the §6.3 rule,
block amax lands in `[2^emax, 2^(emax+1))` — for E2M1 that is `[4, 8)` while the
max representable is **6.0**. Every block whose amax normalizes above 6
saturates. NVIDIA's ceil-scale trades this for losing the top codes (±4, ±6
unreachable, ~2.58 of 3.58 binades used). TransformerEngine ships this as a
product knob (`NVTENVFP44Over6Mode {Disabled, MinMAE, MinMSE}`).

⇒ **Parametricity on scale policy and rounding is required for conformance, not
a convenience.**

---

## 3. Proposed type surface

```arch
ScaledVec<Elem, N, Scale>
```

- `Elem` — a storage-only narrow float (`FP4E2M1`, `FP6E2M3`, `FP6E3M2`) or an
  existing `FP8E4M3` / `FP8E5M2`.
- `N` — block size, a const expression (32 for MX, 16 for NVFP4).
- `Scale` — `E8M0` (MX) or `UE4M3` (NVFP4). **A parameter, because the two
  ecosystems differ.**

Spelled-out aliases for the named formats:

```arch
type MXFP4  = ScaledVec<FP4E2M1, 32, E8M0>;
type MXFP6  = ScaledVec<FP6E3M2, 32, E8M0>;   // or FP6E2M3
type MXFP8  = ScaledVec<FP8E4M3, 32, E8M0>;
type NVFP4  = ScaledVec<FP4E2M1, 16, UE4M3>;  // + per-tensor FP32, see §7
```

**`UE4M3` is a new format, not our `FP8E4M3`.** PTX: *"a 7-bit unsigned
floating-point format … NaN value is limited to `0x7f` … MSB bit padded with
zero."* Unsigned, 7 significant bits, no Inf, NaN at `0x7F` not `0xFF`. Reusing
`FP8E4M3` here would be a real bug.

`ScaledVec<E, 16, E8M0>` is legal in the MX *framework* (block size is a free
parameter; only the named formats pin k=32) — it simply may not be called MXFP4.

### 3.1 Operations — deliberately minimal

```arch
scaled_quantize<scale_policy, rounding>(v: Vec<FP32, N>) -> ScaledVec<E, N, S>
scaled_dequantize(b: ScaledVec<E, N, S>)                     -> Vec<FP32, N>
scaled_dot(a: ScaledVec<E, N, S>, b: ScaledVec<E, N, S>)         -> FP32
```

- `scale_policy ∈ { floor_pow2 (default, = OCP §6.3), ceil_pow2 (= NVIDIA), exact }`
  — `exact` is meaningful only for a non-power-of-two `Scale` such as `UE4M3`.
- `rounding ∈ { rne (default), rtz, rna, stochastic }`.
- Overflow handling is `saturate` (the §6.3 `should`); a `wrap`/`nan` alternative
  is deliberately **not** offered — for E2M1/E2M3/E3M2 there is no NaN encoding
  to produce, so saturate is the only representable behavior.

**No `+`, `-`, `*`, or comparison on `ScaledVec`.** There is no spec meaning for
them, no hardware, and inventing semantics here would be exactly the kind of
unforced divergence the FP work has avoided so far.

### 3.2 Memory layout — ours to define, so define it loudly

The spec does not prescribe layout, and the three production implementations all
disagree: ggml packs scale-first with **split-halves nibbles** (byte *j* holds
elements *j* and *j+16*); NVIDIA keeps scales in a **separate 128×4 swizzled
plane**; PTX's `cvt…e2m1x2` packs consecutive pairs with **`a` in the high
nibble**.

Proposed canonical form — packed, scale in the high bits, **element 0 in the low
bits** (matching ARCH's existing `Vec` convention, spec §326):

```
{ scale[w-1:0], P[N-1], …, P[1], P[0] }      // width = w + N*d
```

MXFP4 ⇒ 8 + 128 = **136 bits**. Additionally expose a **split form** (scale port
+ element-plane port) because that is what real datapaths want — the scale feeds
the exponent path and the elements feed the mantissa path, so packing and
unpacking a 136-bit struct per block is pure overhead. Interop with any external
layout is a lowering concern, not a type concern.

---

## 4. What this dictates for the scalars (revising task #15)

| Type | Role | Surface |
|---|---|---|
| `FP4E2M1` | MX/NVFP4 element | **storage-only**: literals + `.to_fp32()` / `.to_fp4e2m1()`. No arithmetic, no compares, no `is_nan` (unrepresentable) |
| `FP6E2M3`, `FP6E3M2` | MXFP6 elements | storage-only, same |
| `E8M0` | MX block scale | **separate scale type, NOT `is_float`** — no sign, no mantissa, no zero; `is_nan` = `== 0xFF`; conversions only |
| `UE4M3` | NVFP4 block scale | unsigned 7-bit scale type; NaN `0x7F` |

`is_nan` on E2M1/E2M3/E3M2 must be a **compile error**, not a constant `false` —
the concept does not exist in those formats, and silently answering `false`
would mislead.

### 4.1 Why storage-only needs real work (not just omission)

`Ty::is_float()` (`src/typecheck.rs:32-36`) is a **single uniform gate** for both
conversion and arithmetic. Add a format to it and `a + b` type-checks, then dies
at Verilator / z3 / g++ **with no source span**, because all three dispatch sites
build operator names by string interpolation with no existence check
(`formal.rs` `encode_binary`, `codegen/mod.rs` binary arm,
`sim_codegen/expr_codegen.rs`). Leave it *out* of `is_float()` and
`check_width_compatible`'s trailing `_ => {}` lets `let x: E8M0 = <anything>;`
pass silently.

Required: split `is_float()` into a carrier predicate and an
arithmetic-capability predicate; route storage-only `+ - *` to the existing
"unsupported operator on float type" diagnostic; make the three dispatch sites
total (return `Option`, error on miss).

### 4.2 Why E8M0 cannot ride the float machinery

- `fp8_round` **panics** with `mb = 0`: `extract(&kept_n, mb - 1, 0)` underflows
  `u32`, and `cst((1<<mb)-1, mb)` builds a width-0 constant.
- Three `concat`s and `round_f64_to_narrow`'s `sign_shift` assume a sign bit.
- All three `is_nan` implementations test `exp==all-ones && mant!=0` — not
  E8M0's shape (`== 0xFF`, no mantissa).
- E8M0 has no zero, which nothing in the float path anticipates.

Model it as a distinct type carried as `UInt<8>`-like storage with two dedicated
intrinsics, plus its own `is_nan`.

---

## 5. Phase 0 — prerequisite hardening (blocking, and independently valuable)

Several dispatch sites are correct **only because exactly two 8-bit formats
exist**. Any 4- or 6-bit format trips them *silently* — wrong answers, not
crashes. These must be fixed before any new format lands:

```rust
fn float_tag_width(tag: &str) -> u32 {
    match tag { "f32" => 32, "bf16" => 16, _ => 8 }   // FP4/FP6 silently get 8
}
```
```rust
let (name, max) = match fmt {
    FloatLitFmt::E4m3 => ("FP8E4M3", 448.0),
    _ => ("FP8E5M2", 57344.0),      // any new format ⇒ WRONG overflow diagnostic
};
```
plus three `unwrap_or("f32")` tag fallbacks (`formal.rs:2518`, `:5133`,
`codegen/mod.rs:7238`) and the `is_nan` bitfield tables' `_ =>` f32 arms.

**The deeper hazard:** adding a `TypeExpr` variant produces compiler errors only
at *exhaustive* matches. The non-exhaustive `_ =>` sites — `width.rs:443`,
`pybind.rs:665`, `codegen/mod.rs:4041`, `type_alias.rs:184`,
`elaborate/mod.rs:394`, `:2219`, `codegen/mod.rs:7357` — compile silently and
default to integer/32-bit behavior. **That is the true miscompile surface**, and
it is the same class as the #770 audit (float positions emitting integer
arithmetic).

A float-format registration audit found **~46 sites**, not the three recorded in
the project notes: `codegen/mod.rs`'s `param_float_fmt` alone contains three
copies of the same 4-arm map, and there are **six independent `float_names`
builders**, one per construct.

Phase 0 deliverable: replace the ad-hoc maps with a single format descriptor
table (bits, bias, has_inf, has_nan, max_finite, tag, arithmetic-capable) that
every site consults, so a new format is one table row plus compiler-enforced
exhaustiveness.

---

## 6. Lowering

**SV.** Packed `logic [w+N*d-1:0]`, or split scale/element ports. `scaled_dot`
lowers to widen-each-element → FP32 multiply → defined-order accumulate →
scale-apply, reusing the existing proven `arch_*_to_f32` helpers.

**Sim.** Element plane as a C array + scale scalar, mirroring `Vec`'s
representation (`vec_array_info_with_params`). Must register in all six
`float_names` builders and both `decl_types` resolvers.

**Formal.** `ScaledVec` is a flat bit-vector, so it fits the existing
`(_ BitVec W)`-per-signal model better than `Vec` does. Element select is an
`extract`. Needs `check_scalar_type` and `type_width_signed` arms.

---

## 7. Verification — where ARCH can be genuinely differentiated

1. **Exhaustive proofs become nearly free at 4 bits.** E2M1 has 16 encodings:
   the full binary-op space is 256 pairs, unary 16, fma 4096 — versus 2²⁴ for
   the E4M3 fma characterization already done. Every element-level property can
   be proven *exhaustively* rather than sampled.
2. **`f32 → E2M1` narrowing is exhaustively provable by SMT** over all 2³² FP32
   inputs, as the existing `_narrow` miters already do for fp8.
3. **Round-trip properties**: `scaled_dequantize(scaled_quantize(v))` error bounds; scale
   selection optimality; saturation counts per policy.
4. **`bound_err` quantization analysis (the differentiator).** The gappa path
   from #788 can bound *quantization error of a whole block* — including the
   floor-vs-ceil scale-policy trade-off — as a machine-checked numeric bound. No
   other toolchain offers a formal error bound on MX quantization.
5. **Dot-product accumulation is implementation-defined by the spec**, so we
   define ours *and prove* the implementation matches the spec's factored form
   `X^A·X^B·Σ(Pᵢ×Pᵢ)`. That is a property nobody else states, let alone proves.

Non-IEEE formats need a hand-written SMT round spec (~60–100 lines each, in the
existing `round_spec_in` / `e4m3_widen` style) because FP4/FP6 have no
`(_ FloatingPoint e s)` counterpart. Storage-only roughly halves this (widen +
narrow miters only, no arith/fma/cmp).

---

## 8. Phasing

| Phase | Content | Gate |
|---|---|---|
| **0** | Format descriptor table; fix the silent wildcards; make dispatch total; split `is_float()` | No behavior change; existing tests green |
| **1** | `E8M0` scale type + storage-only `FP4E2M1`; exhaustive SMT + literal encoders | Exhaustive narrow/widen miters unsat |
| **2** | `ScaledVec<Elem,N,Scale>` type, layout, `scaled_quantize`/`scaled_dequantize` with policy+rounding params | Round-trip tests; SV↔sim byte-identical |
| **3** | `scaled_dot` with defined accumulation order + proof vs the spec's factored form | SMT equivalence |
| **4** | `FP6E2M3`/`FP6E3M2`, MXFP6/MXFP8 aliases, `bound_err` quantization analysis | gappa bounds recorded |
| **5** | NVFP4: `UE4M3` scale + two-level (per-tensor FP32) scaling | vs TransformerEngine vectors |

`fp8_round` is already width-generic (its bias/anchor/`fw` formulas compute
correctly for E2M1/E2M3/E3M2); only `ocp_top: bool` must become a 3-way enum,
since FP4/FP6 are **all-finite** — a third overflow rule. The machine-proven
`normround` stays byte-identical. `fp8_bin`/`fma`/`cmp` need width
parameterization (currently hardcoded 8).

`round_f64_to_narrow`'s documented gotcha applies to **all three** new formats:
its IEEE template treats an all-ones exponent as overflow-to-infinity, which for
an all-finite format is a silently wrong finite constant. E2M1 is the extreme —
the generic rounder is wrong for `|x| ≥ 3.0`, a third of the format. Each needs
a dedicated top-binade path.

---

## 9. Decisions — SETTLED (maintainer sign-off 2026-08-09)

Each item below is a spec silence or a genuine fork. All are now decided; the
implementation must follow these and document them in the spec section that
lands with Phase 2.

| # | Decision | Resolution |
|---|---|---|
| 1 | Default scale policy | **`floor_pow2`** (OCP §6.3), with `ceil_pow2` and `exact` also offered |
| 2 | Scale for an all-zero block | **`0x00`** (= 2⁻¹²⁷), matching microxcaling |
| 3 | Shared-exponent clamping | underflow → **`0x00`**; overflow → **`0xFF`** (NaN) |
| 4 | Element field when `X = 0xFF` | **preserve on store, ignore on load**; documented |
| 5 | `X·Pᵢ` outside FP32 range | **saturate to ±FP32 max** |
| 6 | Stored scale | **decode scale** (multiply on read), matching `vᵢ = X·Pᵢ` |
| 7 | Dot accumulator | **FP32, defined left-to-right order**, and proven |
| 8 | Layout | **packed canonical + split form**, per §3.2 |
| 9 | MXINT8 | **deferred** — integer format, shares none of the float machinery |
| 10 | Type name | **`ScaledVec`** (not `MXVec`) — brand-neutral |

**Consequence of #10 (inferred, easily overridden):** the operations are named
`scaled_quantize` / `scaled_dequantize` / `scaled_dot` for consistency with the
neutral type name. The *format aliases* keep their real-world brand names —
`MXFP4`, `MXFP6`, `MXFP8`, `NVFP4` — because those name specific published
formats rather than the generic mechanism.

**Consequence of #7:** left-to-right FP32 accumulation is *not* associative, so
the defined order is part of the contract and the proof obligation in §7.5 is
against that specific order, not against a mathematical sum.

**Consequence of #1 + #5:** with `floor_pow2` default, element saturation is
routine (§2.2), and `saturate` is the only representable overflow behavior for
E2M1/E2M3/E3M2 anyway. Users wanting NVIDIA-equivalent numerics select
`ceil_pow2` explicitly.

---

## 10. Related work in the repo

- Scalar float line: PRs #760/#763/#767/#770/#772/#774/#778/#779/#782/#788/#791.
- `bound_err` + `assume`: PR #788 — the substrate for §7.4.
- Vacuity guards PR #795 and counterexample replay PRs #816/#817 — the formal
  soundness guards any new proof obligation inherits.
- Composite precedent: PR #770 (floats in `Vec`/struct/function positions) and
  fixture `tests/fp_v1/FpComposite.arch`.
