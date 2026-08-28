# Proposal: FP exception flags — `checked(...)` → `FpResult<T>`

**Status:** design (surface confirmed); implementation pending.
**Motivation:** the pairwise-summation error bound (Theorem B,
`proofs/lean_fp_equiv/ArchFpEquiv/F32PairwiseSum.lean`) holds only under a
*no-overflow / no-underflow throughout* assumption (`SummationSafe`). Overflow to
`±Inf` and precision-losing underflow are exactly the conditions that **void the
accuracy guarantee**. Today ARCH returns the IEEE result but exposes no status,
so a design cannot detect or react to these, and the formal precondition cannot
be checked at the RTL. This adds per-op FP exception flags, serving both runtime
(the design branches on them) and formal (auto-assertions discharge
`SummationSafe`).

## Surface

A built-in generic struct and a `checked(...)` form wrapping one FP op:

```arch
let r: FpResult<FP32> = checked(a + b);
comb
  sum      = r.value;       // FP32 — the ordinary IEEE result
  ovf_flag = r.overflow;    // Bool
  und_flag = r.underflow;   // Bool
  inv_flag = r.invalid;     // Bool
  inx_flag = r.inexact;     // Bool
end comb
```

- `FpResult<T>` is a built-in struct `{ value: T, overflow, underflow, invalid,
  inexact : Bool }`. Fields are ordinary `Bool`/`T` — wire them anywhere.
- `checked(e)` requires `e` to be a single FP op (`a + b`, `a - b`, `a * b`,
  `fma(a,b,c)`) on a supported float type (FP32 first; BF16/FP8 follow the same
  lowering). Uncomposed: `checked((a+b)*c)` is a type error — wrap each op.
- Non-`checked` FP ops are unchanged (pure, no flags) — zero cost when unused.

## Flag semantics (IEEE 754, and why they avoid the benign case)

The four flags are computed from signals **already present** in `normround`
(`src/fp_ops.rs`) — the shared round-and-pack function behind add/mul/fma:

| flag | formula (from `normround` internals) | meaning |
|---|---|---|
| `inexact` | `guard \| sticky` (the existing rounding bits) | result was rounded — lost information |
| `overflow` | `¬biased_le0 ∧ sig≠0 ∧ overflow` (the existing `overflow = biased_n ≥ 255`) | result rounded to `±Inf` |
| `underflow` | `biased_le0 ∧ sig≠0 ∧ inexact` | result is subnormal **and** inexact |
| `invalid` | op-level: a non-NaN input produced NaN (`∞+(−∞)`, `0·∞`) | invalid operation |

**Underflow = subnormal-result ∧ inexact is exactly the non-benign case.** With
this definition, the benign situations produce **no flag** — verified against the
proof's own cases:

- `big + tiny = big` (the `add_negligible` case): result is `big` (normal),
  `biased_le0` false → **no underflow** ✓.
- `2⁻¹⁴⁹ + 2⁻¹⁴⁹ = 2⁻¹⁴⁸` (subnormal but *exact*): `inexact` false → **no
  underflow** ✓.
- `a + (−a·(1+ε)) = tiny_subnormal` (catastrophic cancellation): subnormal **and**
  inexact → **underflow fires** ✓ — precisely the `SummationSafe`-violating case.

No heuristics: "avoid benign" falls out of using *result*-tininess+inexactness,
the standard IEEE underflow definition, rather than flagging tiny operands.

## Lowering

1. **FP IR** (`src/fp_ops.rs`) — add `normround_flags` returning
   `(result, overflow, underflow, inexact)` alongside the byte-identical
   `normround` (the machine-proved rounder stays untouched — its SMT miter and the
   Lean `arch_f32_add`/`arch_fma_f32` proofs depend on byte-identity). Add
   `f32_add_checked` / `f32_mul_checked` / `fma_f32_checked` that call
   `normround_flags`, compute `invalid` from the special-value lattice, and pack
   `{inexact, invalid, underflow, overflow, value[32]}` into a 36-bit result.
2. **AST / parser** (`src/ast.rs`, `src/parser.rs`) — `checked(<fpop>)` expression
   form; `FpResult<T>` type; `r.field` reuses struct field access.
3. **Typecheck** (`src/typecheck.rs`) — `checked` on an FP op → `FpResult<T>`;
   field types.
4. **SV codegen** (`src/codegen/`) — emit `arch_f32_add_checked` (returns the
   36-bit struct); `FpResult` is an SV `struct packed`; field access → bit slices.
5. **Sim** (`src/sim_codegen/`) — C++ helper computing the same flags.
6. **Formal** (`src/formal.rs`, `src/fp_smt_proof.rs`) — SMT model of the checked
   op; a renderer miter row proving the checked SV ≡ the model.

## Formal tie-in — discharging `SummationSafe` (mechanism #1)

Independently of the runtime struct, an **opt-in** `--fp-checks` flag auto-emits
concurrent SVA (in `translate_off/on`, reusing the FIFO `_auto_no_overflow` /
bounds / div0 machinery):

```sv
_auto_no_fp_overflow_<n>:  assert property (@(posedge clk) disable iff (rst) !<overflow>);
_auto_no_fp_underflow_<n>: assert property (@(posedge clk) disable iff (rst) !<underflow>);
```

on every FP op. A design (or formal run) that keeps these low has, at the RTL
level, established the `SummationSafe` precondition — so the Lean pairwise error
bound becomes conditional on a **machine-checked hardware property**. This is the
clean bridge between the proof and the emitted hardware.

## PR sequencing

1. **Core numerics** — `normround_flags` + `*_checked` FP-IR fns + a differential
   test (overflow→Inf, cancellation→subnormal, benign cases) vs IEEE. Internal,
   verifiable, de-risks the semantics. (No surface yet.)
2. **Surface** — `FpResult<T>` type + `checked(...)` (parser/typecheck/codegen/
   sim), FP32 add end-to-end + `.arch` fixture. **Spec update in the same PR**
   (grammar-surface → `doc-drift` CI).
3. **Coverage** — mul/fma/sub; BF16/FP8.
4. **Formal** — SMT model + renderer miter; the `--fp-checks` auto-assert
   (discharges `SummationSafe`).

## Notes

- `divzero` omitted (no FP division op in ARCH today); trivially added with one.
- Flags are **sticky per op-instance** in the struct sense (the op's own flags),
  not a global accumulating status register — matching the pure, wire-everywhere
  ARCH style. A design that wants an `fcsr`-style accumulator ORs the flags itself.
