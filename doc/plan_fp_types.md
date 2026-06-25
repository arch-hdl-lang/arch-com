# Plan: Floating-Point Types for LLM Inference (`FP32`, `BF16`)

Status: **draft / design** — no code yet.
Scope owner: TBD. Tracking issue: TBD (sibling of #605).

## 1. Goal and scope

Add first-class IEEE-754-style floating-point types to ARCH so that LLM
inference datapaths (GEMM, attention, layernorm, activation) can be expressed
directly instead of being hand-rolled out of `UInt`/`SInt`.

**v1 supports exactly two formats:**

| ARCH type | sign | exp | mant | total | bias | notes |
|---|---|---|---|---|---|---|
| `FP32` | 1 | 8 | 23 | 32 | 127 | IEEE-754 binary32 |
| `BF16` | 1 | 8 | 7 | 16 | 127 | bfloat16 (binary32 with the low 16 mantissa bits dropped) |

Both share the same 8-bit exponent, which makes `BF16 ↔ FP32` widening exact
and narrowing a single round — a property we lean on throughout.

**Non-goals for v1** (explicitly deferred, but the design must not preclude
them):

- FP16, FP8 (E4M3/E5M2), FP6/FP4, MX/NVFP4 block-scaled formats. The internal
  representation is parameterized on `(exp, mant)` so these are additive later.
- Rounding modes other than round-to-nearest-even (RNE).
- IEEE exception/status flags (invalid, overflow, inexact, …). No FP status
  register in v1.
- User-defined operator overloading (see §4 — built-in operators only).
- Flush-to-zero / denormals-are-zero. v1 implements **full subnormal support**
  (it is what SoftFloat does and what we can prove); FTZ is a future flag.

## 2. Design decisions locked for v1

1. **Two formats only**: `FP32`, `BF16`.
2. **Simulation = Berkeley SoftFloat as a linked library** (the 754 reference
   implementation). The native C++ sim does not hand-roll FP arithmetic.
3. **RTL = ARCH emits its own readable SystemVerilog**, using CVFPU / FPnew as
   the *algorithmic* reference (normalization, LZC, round logic). We do **not**
   instantiate or vendor CVFPU RTL — that would violate the "one construct → one
   deterministic, readable SV structure" philosophy and pull in external IP.
4. **The emitted RTL must be proven equivalent to SoftFloat** (i.e. to IEEE-754
   RNE). See §8 — this is the load-bearing part of the plan.
5. **RNE only**, **full subnormals**, **single canonical quiet NaN**. These are
   not hand-rolled: we build SoftFloat with its **RISC-V specialization**, which
   natively gives a canonical default NaN (`0x7FC00000`, no payload
   propagation), full subnormals, and saturating round-toward-zero float→int
   conversions. Sim and RTL both follow that spec (payloads/policy in §6).
6. **Operators `+ - *` and `fma` are built-in and combinational**; users
   pipeline explicitly with `reg`/`seq`/`pipeline`. No hidden latency.

## 3. Type-system changes

### 3.1 AST (`src/ast.rs`)

Extend `TypeExpr` (currently at `src/ast.rs:1040`):

```rust
pub enum TypeExpr {
    UInt(Box<Expr>),
    SInt(Box<Expr>),
    Bool,
    Bit,
    Clock(Ident),
    Reset(ResetKind, ResetLevel),
    Vec(Box<TypeExpr>, Box<Expr>),
    Named(Ident),
    Float { exp: u32, mant: u32 },   // NEW: internal generic representation
}
```

`FP32` and `BF16` are surface keywords (or aliases resolved in `resolve.rs`)
that desugar to `Float { exp: 8, mant: 23 }` and `Float { exp: 8, mant: 7 }`.
Keeping the generic `{exp, mant}` shape — rather than a two-variant enum — is
what lets FP16/FP8/MX drop in later without touching the type representation.

### 3.2 Lexer / parser (`src/lexer.rs`, `src/parser.rs`)

- New keywords `FP32`, `BF16` in the type-expression position.
- **Float literals**: `1.5`, `3.0`, `1.0e-3`. The literal is type-inferred from
  context (the assignment/operator target) and **const-evaluated through
  SoftFloat in the compiler** so the rounded bit pattern is identical to what
  sim/RTL would produce. A bare `1.5` with no FP context is a type error (no
  implicit numeric type — consistent with ARCH's "no implicit conversions").
- Optional: hex-float literals `0x1.8p0` (defer if it complicates the lexer).

### 3.3 Type checker (`src/typecheck.rs`, `src/width.rs`)

- **No implicit conversions**, matching the existing integer rules. `FP32` and
  `BF16` are distinct types; mixing them in an operator is an error.
- **Operator typing** (`width.rs`): `FP32 ⊕ FP32 → FP32`, `BF16 ⊕ BF16 → BF16`
  for `⊕ ∈ {+, -, *}`. No width-growth rule (unlike integer `+` which widens) —
  FP result width equals operand width.
- **Comparisons** `== != < <= > >=` on matching FP types → `Bool`. Semantics are
  IEEE ordered compares; `NaN` compares false/unordered (document precisely).
- **Conversions** (explicit, method-style, mirroring `.trunc/.zext/.sext`):
  - `.to_fp32()` on `BF16` — exact widen.
  - `.to_bf16()` on `FP32` — RNE narrow.
  - `.to_fp32()` / `.to_bf16()` on `UInt<N>`/`SInt<N>` — int→float, RNE.
  - `.to_uint<N>()` / `.to_sint<N>()` on a float — float→int. **Open question
    (§9)**: round-toward-zero (C-cast convention) vs RNE. Default proposal:
    round-toward-zero, documented loudly.
- **`fma(a, b, c)`** intrinsic: `a*b + c` with a *single* rounding (true fused
  multiply-add), all three operands the same FP type. This is the one place
  where users get the numerically-superior fused form explicitly.

## 4. Operator & latency policy

Decision recap (from prior discussion):

- **Built-in operators, not user-defined overloading.** The compiler knows
  `FP32 * FP32`, exactly as it already knows `SInt * SInt` differs from
  `UInt * UInt`. Users cannot define `operator*` on a struct.
- **Operators are combinational.** `c = a * b` lowers to a single-cycle
  combinational FP unit. This preserves "one line = one comb structure" and
  keeps timing visible. An FP32 combinational multiply is large; that is the
  user's signal to pipeline it themselves with `reg`/`pipeline`. We do **not**
  silently insert pipeline stages.
- **`fma` is the explicit fused primitive.** Provided because separate `a*b`
  then `+c` double-rounds; accumulation loops want the single-rounded form.
- Integer quantized formats (INT8/INT4) need nothing here — they are already
  `UInt`/`SInt` and use existing operators. This plan is float-only.

## 5. Simulation backend — SoftFloat

The C++ sim (`src/sim_codegen/`) links **Berkeley SoftFloat** (John Hauser's
754 reference; permissive BSD-3). Vendored as a git submodule under
`third_party/softfloat/` (or fetched + built via `build.rs`); the generated sim
links against it.

### 5.1 Value representation

FP signals are stored as their **bit patterns** in the existing integer carrier
(`uint32_t` for `FP32`, `uint16_t` for `BF16`) — no change to the signal/port
plumbing, debug printing, or waveform dump. `--debug` prints the hex pattern as
it does today; a future nicety can pretty-print the decoded float.

### 5.2 FP32 operations

Direct SoftFloat calls on `float32_t`:

| ARCH op | SoftFloat |
|---|---|
| `a + b` | `f32_add(a, b)` |
| `a - b` | `f32_sub(a, b)` |
| `a * b` | `f32_mul(a, b)` |
| `fma(a,b,c)` | `f32_mulAdd(a, b, c)` |
| compares | `f32_eq`, `f32_lt`, `f32_le` (+ derived) |
| `bf16.to_fp32()` | exact bit expansion (no SoftFloat needed) |
| `fp32.to_bf16()` | see §5.3 |
| int↔fp32 | `i32_to_f32`, `ui32_to_f32`, `f32_to_i32_r_minMag`, … |

`softfloat_roundingMode = softfloat_round_near_even` globally. SoftFloat is
**built with the RISC-V specialization** (`source/RISCV/specialize.h`), which
fixes the platform-dependent corners we care about — see §6. Exception flags are
computed by SoftFloat but ignored in v1 (cleared each op). `softfloat_detectTininess`
only affects the (ignored) underflow flag, not result values.

The toward-zero float→int conversions use the `_r_minMag` ("round to minimum
magnitude") named variants — `f32_to_i32_r_minMag`, `f32_to_ui32_r_minMag`,
etc. — which is the C-cast / `fcvt`-rtz convention. Out-of-range and NaN inputs
saturate per the RISC-V spec (§6).

### 5.3 BF16 operations — provably single-rounded via f64

SoftFloat has no native bfloat16. Naively doing a BF16 op by promoting to
`float32`, operating, and narrowing **double-rounds** and can be wrong. We avoid
this with a clean, provable construction:

> **BF16 op `⊕`** = `round_bf16( f64_⊕( to_f64(a), to_f64(b) ) )`
> where `to_f64` is exact (BF16 ⊂ binary64) and `round_bf16` is RNE.

This is **provably equal to the directly-rounded BF16 result**. By the standard
double-rounding result (Figueroa / "innocuous double rounding"), rounding
through an intermediate format of precision `p₁` then to a target of precision
`p₂` equals direct rounding when `p₁ ≥ 2·p₂ + 2`. Here `p₂ = 8` (BF16 significand
incl. implicit bit) needs `p₁ ≥ 18`, and binary64 gives `p₁ = 53`. So
`f64_add`/`f64_mul`/`f64_mulAdd` followed by a single RNE narrowing to BF16 is
correctly rounded for every input. (For `*` an even simpler argument holds — the
exact product of two 8-bit significands fits in 16 < 53 bits — but the f64 route
gives `+` and `fma` uniformly, so we use it for all three.)

`round_bf16(float64_t)`: SoftFloat narrow `f64_to_f32` is *not* directly reusable
(wrong target precision); instead implement RNE narrowing of the f64 to the
8-bit BF16 significand with guard/round/sticky from the f64 mantissa. This helper
is small, self-contained, and itself unit-tested against an exhaustive BF16 ×
BF16 sweep (2³² pairs is large but `+`/`*` are cheap; a sampled + corner sweep
plus the formal proof in §8 covers it).

`BF16 → FP32` widening is exact (shift mantissa, copy sign/exp; NaN/Inf
preserved). `FP32 → BF16` narrowing is RNE on the 23→7 mantissa.

## 6. Special-value policy (shared by sim, RTL, formal)

These must be identical across all three backends or equivalence is meaningless.

- **Subnormals**: fully supported (gradual underflow), both formats.
- **Signed zero**: preserved; `-0.0 == +0.0` is true; `1.0/(-0.0)` is `-Inf`
  (no division in v1, but the sign rule stands for future).
- **Infinity**: standard 754 arithmetic.
- **NaN — canonicalization**: any operation that produces NaN emits a **single
  canonical quiet NaN**: sign 0, exponent all-ones, mantissa MSB set, rest 0
  (`0x7FC00000` for FP32, `0x7FC0` for BF16). Input signaling NaNs are quieted
  to this pattern. This matters because 754 leaves NaN payloads unspecified,
  SoftFloat has its own propagation rules, and SMT `FloatingPoint` has a single
  NaN — pinning a canonical pattern is what makes the bit-exact equivalence in
  §8 well-defined. **We do not write a canonicalization wrapper — we build
  SoftFloat with the RISC-V specialization**, whose `defaultNaNF32UI` is exactly
  `0x7FC00000` and whose `softfloat_propagateNaNF32UI` ignores input payloads and
  always returns the default NaN. The RTL is built to match; BF16 (absent from
  SoftFloat) mirrors the same convention with `0x7FC0`.
- **float→int out-of-range / NaN**: saturating per the RISC-V spec — `+Inf` /
  overflow → integer max, `−Inf` / underflow → integer min, **NaN → integer
  max** (RISC-V `fcvt` convention; SoftFloat RISCV `i32_fromNaN`). Toward-zero
  rounding (`_r_minMag`).
- **Rounding**: RNE for arithmetic; toward-zero for float→int. No mode plumbing.

### 6.1 Why RISC-V, and how this differs from NVIDIA CUDA / x86

The platform-dependent corners (NaN pattern, NaN→int, FTZ) genuinely differ
between vendors, so "follow IEEE" is not specific enough — we must name one. We
pick **RISC-V** because it is the only mainstream profile that is *both* a clean
single-canonical-NaN / full-subnormal model (so it lines up with the SMT
`FloatingPoint` theory used for the §8 proof) *and* directly available as a
SoftFloat build with no wrapper. For reference, the same three points elsewhere:

| Point | ARCH v1 (RISC-V / SoftFloat) | NVIDIA CUDA / PTX | x86 (SSE / SoftFloat `8086`) |
|---|---|---|---|
| Canonical NaN (f32) | `0x7FC00000` | `0x7FFFFFFF` (PTX canonical) | `0xFFC00000` (sign set), propagates input payloads |
| Subnormals | full (no FTZ) | supported; **FTZ is a compile flag** (`-ftz`, default off; `--use_fast_math` ⇒ on) | full |
| float→int, out of range | saturate; **NaN → int max** | `cvt` saturates to min/max; **NaN → 0** | "integer indefinite" `0x80000000` |
| float→int rounding | toward zero (`_r_minMag`); other modes deferred | explicit per-instruction (`cvt.rni/.rzi/.rmi/.rpi`); C cast ⇒ `rzi` | toward zero for C cast |

Notable: CUDA converts **NaN → 0** for float→int while RISC-V gives **int max**,
and CUDA's canonical NaN is `0x7FFFFFFF` (all-ones-ish), not `0x7FC00000`. We are
deliberately *not* CUDA-compatible at these corners; the design value is
provable sim/RTL/SMT agreement, not bug-for-bug GPU parity. (CUDA's FTZ-as-a-flag
model is, however, a good template for the future FTZ knob.)

## 7. RTL backend — emit SV, reference CVFPU

New emitter `src/codegen/fp.rs` (sibling of `codegen/pipeline.rs` etc.). For
each FP operator used in a design, emit a combinational SV module:

```
arch_fp32_add, arch_fp32_sub, arch_fp32_mul, arch_fp32_fma,
arch_bf16_add, arch_bf16_sub, arch_bf16_mul, arch_bf16_fma,
arch_fp_cmp_*, arch_cvt_bf16_fp32, arch_cvt_fp32_bf16, arch_cvt_int_*
```

- **CVFPU/FPnew is the algorithmic reference**, not a dependency: we mirror its
  datapath structure (operand unpack → align/normalize → core op → leading-zero
  count → RNE round → pack, with subnormal and NaN/Inf handling) but write our
  own deterministic, commented SV so the output stays readable and IP-clean.
- Emitted as a small library of `module arch_fp32_add(...)` definitions, emitted
  once and instantiated where `+`/`*`/`fma` appear — same pattern as other
  generated helper modules.
- Combinational only in v1 (matches §4). A `latency`-parameterized pipelined
  variant is a clean future extension (CVFPU's pipe structure is the reference
  there too).

## 8. Equivalence: emitted RTL ≡ SoftFloat (the crux)

We need RTL output to match the SoftFloat sim bit-for-bit. Two complementary
methods; we want **both**, with formal as the primary guarantee where tractable.

### 8.1 Formal equivalence via SMT FloatingPoint theory (primary)

ARCH already has a formal backend (`src/formal.rs`, SMT-LIB2, EBMC) and a
proof-certificate culture (`construct_proof_cert.rs`, `thread_proof_cert.rs`).
We exploit that the **SMT-LIB `FloatingPoint` theory is exactly IEEE-754 RNE** —
the same specification Berkeley SoftFloat implements. So instead of proving
"RTL ≡ SoftFloat" directly, we prove "RTL ≡ SMT `fp.add/fp.mul/fp.fma`", and
transitively get equivalence to SoftFloat:

```
emitted SV  ≡  SMT fp.* (RNE)  ≡  IEEE-754  ≡  Berkeley SoftFloat
   (proved)        (by theory)     (by defn)     (reference impl)
```

Concretely, for `arch_fp32_mul`, assert the miter:

```smt
(fp.eq  (canonical (sv_result a b))
        (fp.to_ieee_bv (fp.mul RNE (fp.from_bv a) (fp.from_bv b))))
```

over all bit-vector inputs `a, b`, with NaN handled by the §6 canonicalization
on both sides. Discharge with a solver supporting `QF_FPBV` (Bitwuzla, cvc5, or
z3) driven through the existing formal flow / EBMC.

- **BF16**: input space is 2³² (op of two 16-bit values) — formal is very
  tractable; aim for a **full machine-checked proof** of every BF16 operator as
  the primary sign-off.
- **FP32**: 2⁶⁴ input pairs. `add` and conversions are usually solver-tractable;
  `mul`/`fma` are harder. Where a full proof times out, fall back to §8.2 for
  that operator and record the gap explicitly (the project's
  "no silent caps" rule — say what was and wasn't proven).
- **Conversions are a partial-function caveat**: SMT-LIB `fp.to_sbv` / `fp.to_ubv`
  are *partial* — the result for NaN / out-of-range inputs is unspecified by the
  theory. So the float→int **saturation and NaN→int-max behavior (§6) cannot be
  proven against `fp.to_sbv`**; only the in-range cases can. The out-of-range
  conversion corners are signed off by the differential campaign (§8.2) instead,
  with that boundary documented in the proof cert rather than silently claimed as
  formally verified.
- Emit an FP **proof certificate** alongside, consistent with the existing
  `*_proof_cert.rs` pattern, recording solver, version, formats, and result.

### 8.2 Differential / co-simulation campaign (backstop + FP32 mul/fma)

A directed + randomized vector campaign run through **both** arch-sim (SoftFloat)
and the emitted SV under Verilator (`--binary`), asserting bit-equality of the
result and the canonical NaN:

- **Corner vectors**: ±0, ±Inf, qNaN, sNaN, smallest/largest subnormal, min/max
  normal, powers of two, exact ties (RNE tie-to-even cases), overflow and
  underflow boundaries, mantissa all-ones (carry-out of rounding).
- **Randomized**: large uniform-over-bit-patterns campaign (millions of pairs)
  plus exponent-near-equal pairs (the alignment-cancellation cases adders get
  wrong).
- Wired into the regression runner (`doc/arch_regression_runner.md`) so any RTL
  change re-checks against SoftFloat.

### 8.3 SoftFloat ≡ IEEE anchor

We take Berkeley SoftFloat as the definition of correct (it is *the* 754
reference). To guard against our own integration bugs (rounding-mode globals,
NaN-canonicalization wrapper), run SoftFloat's bundled `testfloat_gen`/`testfloat_ver`
vectors against our configured instance once, in CI.

## 9. Open questions (resolve before/at implementation)

1. ~~float→int rounding~~ **Decided**: toward-zero, saturating, NaN → int max —
   inherited from the SoftFloat RISC-V spec (`_r_minMag`, §6). Still open: do we
   *also* expose a `.to_*_rne()` variant? Defer unless a workload needs it.
2. **Surface for `fma`**: free function `fma(a,b,c)` vs method `a.fma(b,c)`.
   Proposal: free function, reads like the math.
3. **NaN canonicalization cost in RTL**: forcing a canonical NaN adds a mux on
   the output. Acceptable? (Needed for clean equivalence — yes.)
4. **Comparisons & unordered**: do we expose an `is_nan()` / unordered predicate
   in v1, or only the six ordered compares? Proposal: add `is_nan(x)` — it's
   cheap and tests need it.
5. **Vec of float**: `Vec<FP32, N>` should "just work" (Vec is element-type
   agnostic) — confirm no special-casing in `sim_codegen`/`codegen` Vec paths.
6. **Const-eval engine in the compiler**: link SoftFloat into the compiler
   itself (not just the generated sim) so literal folding is bit-identical.
   Proposal: yes — one source of truth.

## 10. File-by-file work breakdown

| Area | Files | Work |
|---|---|---|
| AST | `src/ast.rs` | `TypeExpr::Float{exp,mant}`; float-literal expr node |
| Lex/Parse | `src/lexer.rs`, `src/parser.rs` | `FP32`/`BF16` keywords, float literals |
| Resolve | `src/resolve.rs`, `src/type_alias.rs` | format keyword → `Float{..}` |
| Typecheck | `src/typecheck.rs`, `src/width.rs` | op typing, no-implicit-conv, conversions, `fma` |
| Sim | `src/sim_codegen/` + `third_party/softfloat` + `build.rs` | link SoftFloat **built with RISC-V specialization** (gives canonical NaN / saturating conv for free — no wrapper), FP32 direct, BF16-via-f64 helper |
| RTL | `src/codegen/fp.rs` (new), `codegen/mod.rs` | emit `arch_fp32_*`/`arch_bf16_*` SV modules |
| Formal | `src/formal.rs`, new `fp_proof_cert.rs` | SMT miter vs `fp.*`, proof certs |
| Const-eval | compiler-side SoftFloat link | bit-exact literal folding |
| Tests | `tests/` | per-op golden, corner vectors, Verilator co-sim, formal harness |
| Docs | `doc/ARCH_HDL_Specification.md`, `Arch_AI_Reference_Card.md` | document FP32/BF16, operators, conversions |

## 11. Phasing

1. **P1 — types & sim**: AST/parse/typecheck for `FP32`/`BF16`, literals, ops
   (`+ - * fma`, compares, conversions), SoftFloat-backed sim, const-eval.
   Deliverable: `arch sim` runs FP designs correctly.
2. **P2 — RTL**: emit `arch_fp*`/`arch_bf16*` SV; differential co-sim campaign
   (§8.2) green against the P1 sim.
3. **P3 — formal sign-off**: SMT equivalence (§8.1) — full proof for all BF16
   ops, FP32 where tractable, documented gaps backstopped by P2. Proof certs.
4. **P4 — docs & examples**: spec sections, AI reference card entry, a small
   GEMM/attention example exercising `fma`.

Future (out of scope): FP16/FP8/MX block formats (reuse the generic
`Float{exp,mant}` + a `Block<Elem,K,Scale>` wrapper), pipelined latency-N FP
units, exception flags, additional rounding modes, FTZ.
