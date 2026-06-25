# Plan: Floating-Point Types for LLM Inference (`FP32`, `BF16`)

Status: **v1 implemented** (front-end + simulation + **synthesizable** SV
emission, tested). P2 RTL has landed: the SV helpers are synthesizable and the
§8.2 differential Verilator campaign is wired in as a regression test. See §12
for the implementation status and the deltas from this design.
Tracking issue: #609 (sibling of #605).

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
   A `--fp-compat=riscv|cuda` flag (default `riscv`) switches the two
   GPU-divergent corners — NaN pattern and NaN→int — to CUDA semantics via a thin
   output shim over the same arithmetic core (§6.2).
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
  SoftFloat in the compiler** (linking SoftFloat into the compiler itself — see
  §9.6) so the rounded bit pattern is identical to what sim/RTL would produce. A bare `1.5` with no FP context is a type error (no
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
gives `+` and `fma` uniformly, so we use it for all three.) The argument relies
on the f64 intermediate being the **correctly-rounded-to-f64 result of the exact
operation on exact operands** — which holds because BF16 operands convert to f64
exactly and SoftFloat's `f64_*` ops are correctly rounded. The theorem rounds the
exact real result twice regardless of which operation produced it, so `fma`
(fused, single f64 rounding via `f64_mulAdd`) is covered identically to `+`/`*`.

`round_bf16(float64_t)`: SoftFloat narrow `f64_to_f32` is *not* directly reusable
(wrong target precision); instead implement RNE narrowing of the f64 to the
8-bit BF16 significand with guard/round/sticky from the f64 mantissa. The helper
must handle the full range-mapping, not just mantissa GRS: f64's 11-bit exponent
(bias 1023) re-mapped to BF16's 8-bit exponent (bias 127), **overflow → ±Inf**,
**underflow into BF16 subnormals / signed zero**, and Inf/NaN passthrough (NaN →
the canonical BF16 pattern of §6). The exponent re-bias and the over/underflow
boundaries are the error-prone part and get dedicated corner vectors.

Because BF16 is absent from SoftFloat, this entire BF16 special-value path
(canonicalization, sNaN-quieting, saturation) is **hand-built, not inherited** —
so unlike the FP32 path it needs its own corner vectors in §8.2 rather than
getting them for free from SoftFloat. The helper is self-tested against a
corner + sampled BF16 × BF16 sweep (the *exhaustive* sign-off for BF16 operators
is the formal proof in §8.1; the C-helper sweep need only be sampled + corners).

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
- **float→int out-of-range / NaN**: saturating per the RISC-V spec, **to the
  target type's own min/max** — `+Inf` / positive overflow → type max (signed
  `INT_MAX` = `0x7FFFFFFF`, **unsigned `UINT_MAX` = `0xFFFFFFFF`**), `−Inf` /
  negative overflow → type min (signed `INT_MIN`, unsigned `0`), and **NaN →
  type max** (signed `INT_MAX`, unsigned `UINT_MAX`). RISC-V `fcvt` convention;
  SoftFloat `i32_fromNaN` / `ui32_fromNaN`, `f32_to_ui32_r_minMag` saturating at
  `UINT_MAX`. Toward-zero rounding (`_r_minMag`).
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
and CUDA's canonical NaN is `0x7FFFFFFF` (all-ones-ish), not `0x7FC00000`. RISC-V
is the **default** for the provability reasons above — but because users
targeting GPU-derived reference models will want bit-exact GPU parity, we expose
the alternative behind a flag (§6.2).

### 6.2 Compatibility profiles — `--fp-compat=riscv|cuda` (default `riscv`)

**Status: landed.** `--fp-compat=riscv|cuda` is implemented on `arch build`,
`arch sim`, and `arch formal` (validated there but inert, as FP is rejected by
the formal backend). The SV backend (`src/codegen/fp.rs`) and the sim prelude
(`src/sim_codegen` `verilated_h`) each apply the same thin constant
substitution; covered by `fp_compat_build_profiles` / `fp_compat_sim_profiles`
in `tests/fp_test.rs`.

A single flag selects the special-value profile. The crucial design point is
that the two profiles **share an identical arithmetic core** — both compute
IEEE-754 RNE results and both canonicalize to a *single* NaN. They differ only
in two output constants:

| Profile | canonical NaN (f32 / bf16) | NaN → int |
|---|---|---|
| `riscv` (default) | `0x7FC00000` / `0x7FC0` | int max |
| `cuda` | `0x7FFFFFFF` / `0x7FFF` | `0` |

What the flag does **not** touch: the add/mul/fma datapath, RNE rounding,
subnormal handling, in-range conversions, and the toward-zero conversion mode —
all bit-identical across profiles. (CUDA's per-instruction `cvt.rni/.rzi/...`
modes and its FTZ flag are *orthogonal* future knobs; `--fp-compat=cuda` does
**not** imply FTZ, matching CUDA's own `-ftz=false` default.)

Because the difference is so contained, the implementation is a **thin
output-canonicalization shim**, not a second arithmetic path:

- **Sim**: SoftFloat stays built with the RISC-V specialization (the provable
  core). A post-op shim remaps the NaN bit pattern and the NaN→int result when
  the profile is `cuda`. Cheap, deterministic, and keeps one arithmetic
  implementation.
- **RTL**: the profile is a **compile-time** selection — the emitter substitutes
  the NaN-mux constant and the conversion-saturation constant. We do **not** add
  a runtime mode input to every FP unit (that would be un-hardware-like and
  wasteful); a design is built for one profile.
- The flag is therefore a **compile-time flag on the `arch` invocation**
  (`arch build`/`arch sim`/`arch formal`), honored identically by sim and RTL so
  the two never disagree. Default `riscv` if omitted.

**Proof impact: none on the core.** The §8 formal proof already (a) canonicalizes
NaN before comparison rather than asserting a specific payload, and (b) treats
out-of-range/NaN conversion as outside the SMT `fp.to_sbv` partial function
(§8.1) — so the only thing a profile switch changes is *which constant the
differential campaign (§8.2) expects* at those two corners. The
arithmetic-equivalence proof is profile-independent and is not re-run per
profile.

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

**Status (partial, landed) — single source.** The emitted SystemVerilog and the
SMT proof model are now **both rendered from one in-Rust description** of each
operator's bit-logic: `src/fp_ops.rs` defines the operators against a shared
bit-vector IR (`src/fp_ir.rs`); `render_sv` emits the `arch build` RTL and
`render_smt` emits the SMT-LIB2 used here. They cannot drift — there is nothing
hand-transcribed to keep in sync (this replaced the earlier hand-maintained SV
string literal + separately hand-written `.smt2`). `src/fp_smt_proof.rs` wraps
the rendered model in a miter against the `FloatingPoint` theory; the test
`fp_smt_equivalence_proofs` generates each miter and discharges it with z3
(auto-skips if absent). Proven `unsat` over the **entire** input space: FP32
comparisons ×6 (2^64), `f32→bf16` narrow RNE (2^32), `bf16→f32` widen exact
(2^16), and `f32→{sint,uint}` toward-zero in-range (vs the partial `fp.to_sbv` /
`fp.to_ubv`). The **RNE arithmetic** (`+ - *`, `fma`) is generated identically
from the same IR (`dump_fp -- proof mul`), but its 2^64 / fused miter is not
solver-tractable (z3 times out), so it stays on the §8.2 differential backstop.
A tractable arithmetic proof (narrower datapath encodings, or a dedicated FP
equivalence checker) is the remaining P3 item. See
`tests/fp_v1/smt_proof/README.md`.

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
6. ~~Const-eval engine~~ **Decided** (relied on by §3.2): link SoftFloat into the
   compiler itself, not just the generated sim, so literal folding is
   bit-identical to runtime — one source of truth.

## 10. File-by-file work breakdown

| Area | Files | Work |
|---|---|---|
| AST | `src/ast.rs` | `TypeExpr::Float{exp,mant}`; float-literal expr node |
| Lex/Parse | `src/lexer.rs`, `src/parser.rs` | `FP32`/`BF16` keywords, float literals |
| Resolve | `src/resolve.rs`, `src/type_alias.rs` | format keyword → `Float{..}` |
| Typecheck | `src/typecheck.rs`, `src/width.rs` | op typing, no-implicit-conv, conversions, `fma` |
| Sim | `src/sim_codegen/` + `third_party/softfloat` + `build.rs` | link SoftFloat **built with RISC-V specialization** (gives canonical NaN / saturating conv for free — no wrapper), FP32 direct, BF16-via-f64 helper, `--fp-compat` output shim |
| RTL | `src/codegen/fp.rs` (new), `codegen/mod.rs` | emit `arch_fp32_*`/`arch_bf16_*` SV modules; profile-select NaN/conv constants |
| CLI | `src/main.rs` | `--fp-compat=riscv\|cuda` (default `riscv`) on build/sim/formal, threaded to both backends |
| Formal | `src/formal.rs`, new `fp_proof_cert.rs` | SMT miter vs `fp.*`, proof certs (profile-independent core) |
| Const-eval | compiler-side SoftFloat link | bit-exact literal folding |
| Tests | `tests/` | per-op golden, corner vectors, Verilator co-sim, formal harness |
| Docs | `doc/ARCH_HDL_Specification.md`, `Arch_AI_Reference_Card.md` | document FP32/BF16, operators, conversions |

## 11. Phasing

1. **P1 — types & sim**: AST/parse/typecheck for `FP32`/`BF16`, literals, ops
   (`+ - * fma`, compares, conversions), SoftFloat-backed sim, const-eval.
   Deliverable: `arch sim` runs FP designs correctly.
2. **P2 — RTL** ✅ *(landed)*: emit synthesizable `arch_f32_*`/`arch_bf16_*` SV;
   differential co-sim campaign (§8.2) green against the host-IEEE-754 reference
   under Verilator. Remaining P2 stretch: pipelined latency-N variant.
3. **P3 — formal sign-off** *(partial)*: SMT equivalence (§8.1) is wired for the
   non-rounding operators (compares, bf16 widen/narrow, float→int in-range),
   proven exhaustively under z3; the RNE arithmetic awaits an SV→SMT frontend
   that accepts the emitted rounder. Original target — SMT equivalence: full proof for all BF16
   ops, FP32 where tractable, documented gaps backstopped by P2. Proof certs.
4. **P4 — docs & examples**: spec sections, AI reference card entry, a small
   GEMM/attention example exercising `fma`.

Future (out of scope): FP16/FP8/MX block formats (reuse the generic
`Float{exp,mant}` + a `Block<Elem,K,Scale>` wrapper), pipelined latency-N FP
units, exception flags, additional rounding modes, FTZ.

## 12. Implementation status (v1)

Landed and tested (`cargo test --test fp_test`, plus the full suite green):

- **Front-end**: `FP32`/`BF16` type keywords, float literals (`1.5`, `3.0e-2`),
  and the operator/conversion surface parse and type-check. No-implicit-float-
  conversion is enforced at assignment and in operators (mixing `FP32`/`BF16`,
  or float↔int without a cast, is a compile error). Built-ins `fma(a,b,c)` and
  `is_nan(x)` are typed. `Ty::FP32`/`Ty::BF16` carry 32/16-bit widths.
- **Operators**: built-in combinational `+ - *` and ordered compares (`==` `!=`
  `<` `>` `<=` `>=` → `Bool`); `fma` (fused); conversions `.to_fp32()`,
  `.to_bf16()`, `.to_uint<N>()`, `.to_sint<N>()` (float↔float, int↔float).
- **Simulation** (`arch sim`): floats carried as bit patterns
  (`uint32_t`/`uint16_t`); ops dispatch to `_arch_*` helpers in the generated
  prelude. NaN canonicalized to `0x7FC00000`/`0x7FC0`; sNaN quieted;
  float→int toward-zero, saturating, NaN→type-max. Verified bit-exact against
  host IEEE-754 across arithmetic, fma, is_nan, NaN/sNaN, and all conversions.
- **SystemVerilog** (`arch build`): `FP32`→`logic [31:0]`, `BF16`→`logic [15:0]`;
  ops dispatch to emitted `arch_f32_*`/`arch_bf16_*` `function automatic`
  helpers, prepended once per file when FP is used. These are now **synthesizable
  RTL generated from a shared bit-vector IR** (`src/fp_ops.rs` over
  `src/fp_ir.rs`; `src/codegen/fp.rs` just renders it) — decode, integer-mantissa
  arithmetic, binary-search normalization, RNE guard/round/sticky, pack — no
  `$bitstoshortreal`/`$rtoi`. The same IR renders the §8.1 SMT model.
  A single shared rounder backs mul/add/sub/fma and int→float; BF16 arithmetic is
  `widen → f32 op → narrow`. **Differentially verified** bit-exact under Verilator
  against a host-IEEE-754 DPI reference over §8.2 corner + randomized +
  cancellation-prone vectors (`fp_rtl_differential_equiv_verilator`, auto-skips
  when Verilator is absent).

### Deltas from the design above (deliberate v1 simplifications)

1. **Sim uses the host FPU, not a linked Berkeley SoftFloat** (§5). Native
   `float`/`double` arithmetic is IEEE-754 RNE and therefore bit-identical to
   SoftFloat for `+ - * fma` and the conversions; this avoids vendoring/building
   the C library for v1. BF16 goes through an f32 intermediate (innocuous double
   rounding, 24 ≥ 2·8+2 — a tighter but equally valid form of §5.3's f64 route).
   Swapping in the RISC-V-spec SoftFloat build behind the same helper names is a
   drop-in hardening step.
2. ~~**SV helpers are behavioral, not synthesizable**~~ **Resolved (P2).** The SV
   helpers are now synthesizable RTL (`src/codegen/fp.rs`), CVFPU-referenced in
   datapath structure but written as deterministic, commented ARCH-owned SV. The
   §8.2 differential Verilator campaign is wired in as a regression test. What
   remains of §7 is the *pipelined latency-N* variant (v1 is combinational, by
   design) and the §8.1 SMT equivalence proof.
3. **`--fp-compat=riscv|cuda` (§6.2) is wired** on `arch build`/`sim`/`formal`
   (default `riscv`), honored identically by the SV and sim backends as a thin
   output-constant shim (canonical NaN pattern + NaN→int result); the arithmetic
   core is untouched. `arch formal` still rejects FP types in a design, so the
   flag is validated-but-inert there. The **§8.1 SMT equivalence proof is
   partially wired** (compares + conversions proven exhaustively under z3; RNE
   arithmetic deferred — see §8.1 status), and the **§8.2 differential campaign
   is wired** (delta #2).
4. **Float literals default to `FP32`**; `BF16` immediates use `.to_bf16()`
   (§3.3, resolved open question — keeps no-implicit-conversion uniform).

### v1 scope restrictions (enforced — rejected, never miscompiled)

Floats are supported only as **scalar** module signals (ports, `reg`, `wire`,
`let`) and the operations on them. The float-op dispatch in the backends keys
off a per-signal name→format map, so positions it can't resolve are **rejected
at type-check** rather than silently emitting integer arithmetic on the bit
pattern:

- **`Vec<FP32/BF16, N>`** — rejected (element access can't be float-dispatched).
- **Float `struct` fields** — rejected (field access can't be float-dispatched).
- **Floats in module-local `function` signatures/bodies** — rejected (function
  scope isn't threaded into the dispatch map).

Each has a clear error and is a natural follow-up once dispatch is driven off the
type-checker's resolved-type map instead of a backend-rebuilt name set. Also: a
float `reg`'s reset value must be a **float literal** (`reset rst => 0.0`); an
integer literal is rejected (it would store a bit pattern, not the value).

### Conversion semantics (sim, validated)

`float→int` (`.to_uint<N>()` / `.to_sint<N>()`) is toward-zero, **saturating to
the N-bit target range**, NaN→type-max — verified for N<64 and at the bound. The
SV emission now matches: `arch_f32_to_sint`/`arch_f32_to_uint` implement full
per-N saturation up to 64-bit, differentially verified under Verilator at
N ∈ {8,16,24,32,53,64}. int→float (`arch_i64_to_f32`/`arch_u64_to_f32`) is RNE
via the shared rounder.

These deltas keep v1 faithful to the *semantics* (IEEE-754 RNE, the special-value
policy, no implicit conversions). The synthesizable FP datapath has now landed
(delta #2); the remaining deferred infrastructure is the SoftFloat vendor build,
the §8.1 SMT equivalence proof, `--fp-compat=cuda`, and pipelined latency-N FP
units.
