# FP RTL — SMT equivalence proofs (plan §8.1)

Machine-checked proofs that the emitted synthesizable FP SystemVerilog is
equivalent to the SMT-LIB `FloatingPoint` theory, which **is** IEEE-754
round-to-nearest-even:

```
emitted SV  ≡  SMT fp.* (RNE)  ≡  IEEE-754
   (proved here)   (by the theory)
```

## Single source — no transcription

The SV and the SMT are **both rendered from one in-Rust description** of each
operator's bit-logic:

- `src/fp_ops.rs` defines every operator once against the shared bit-vector IR
  (`src/fp_ir.rs`).
- `src/fp_ir.rs::render_sv` produces the `arch build` SystemVerilog.
- `src/fp_ir.rs::render_smt` produces the SMT-LIB2 `define-fun`s.
- `src/fp_smt_proof.rs::equiv_proof` wraps those with a miter against the
  `FloatingPoint` theory.

So the simulated/synthesized RTL and the formally-checked model cannot drift —
there is nothing hand-transcribed to keep in sync. (This replaced the earlier
approach of a hand-maintained SV string literal plus separately hand-written
`.smt2` files.)

## Running

```
cargo test --test fp_test fp_smt_equivalence_proofs   # auto-skips if z3 absent
```

The test generates each miter from the IR, runs z3, asserts `unsat`, and emits a
certificate. To inspect a query by hand:

```
cargo run --release --example dump_fp -- proof lt   | z3 /dev/stdin
cargo run --release --example dump_fp -- smt               # the define-funs
cargo run --release --example dump_fp                      # the SystemVerilog
```

## Coverage (no silent caps)

Proven `unsat` exhaustively (z3 4.8.12):

| op(s) | spec | input space |
|---|---|---|
| `eq ne lt le gt ge` | `fp.eq/lt/leq/gt/geq` | 2^64 |
| `narrow` (`arch_f32_to_bf16`) | RNE round to `(FloatingPoint 8 8)` | 2^32 |
| `widen` (`arch_bf16_to_f32`) | exact widen | 2^16 |
| `to_sint` / `to_uint` (N=32) | `fp.to_sbv`/`fp.to_ubv` RTZ, in-range | 2^32 |
| **`add` / `sub`** | `fp.add` / `fp.sub` | **2^64** (~80 s each) |
| `bf16_eq … bf16_ge` | `fp.eq/lt/…` on `(FloatingPoint 8 8)` | 2^32 |
| `bf16_mul` / `bf16_add` / `bf16_sub` | `fp.mul/add/sub` on `(FloatingPoint 8 8)` | 2^32 |

The BF16 arithmetic ops route through the f32 datapath, but the small input
space makes the miters solver-tractable (`fp_smt_bf16_arith_proofs`, ~minutes;
mul cross-checked with cvc5 `--fp-exp`). They are the plan's §8.1 primary target.

- **float→int** is proved in-range only — SMT-LIB `fp.to_sbv`/`fp.to_ubv` are
  *partial* (undefined for NaN / out-of-range), so the saturation / NaN→type-max
  corners are signed off by the §8.2 differential campaign, as §8.1 anticipates.
- **f32 `add`/`sub` ARE proved** (2^64) — the bounded adder keeps the datapath
  ~56-bit, so the bit-blasted miter is small enough for z3 (~80 s). Only the
  **multiplier-bearing** f32 ops remain: `mul` / `fma` (a 24×24-multiplier
  equivalence is SAT-hard at 2^64 for any bit-blaster — z3, cvc5, or Lean's
  `bv_decide` alike). They stay on the §8.2 differential Verilator campaign
  (`fp_rtl_differential_equiv_verilator`), bit-exact against a host-IEEE-754
  reference over corner + randomized + cancellation-prone vectors. A structured
  theorem prover is the natural route for the multiplier ops — see the Lean
  backend in `proofs/lean_fp_equiv/`, which renders the *same* IR to Lean
  `BitVec` defs (`fp_ir::render_lean`). It builds under Lean v4.30.0 with **zero
  `sorry`**: `bv_decide` machine-checks five structural facts about the emitted
  operators (comparator symmetry, the `sub = add∘negate` construction, and full
  f32-adder **commutativity** over the whole ~56-bit datapath), and the shared
  rounder `arch_round48` is **proved correctly-rounded** against a value-level
  IEEE-754 round-to-nearest-even spec (`arch_round48_correct`) by algebraic
  lifting rather than bit-blasting — so finite `f32_mul` is correctly rounded
  (`arch_f32_mul_finite_correct`), and the same op-independent lemma carries to
  `fma`.
- **`bf16_fma`** computes **fused f32-accumulate**, *not* correctly-rounded bf16
  fma: it widens to f32, does one correctly-rounded f32 fma (the exact `a·b+c`
  rounded once to f32 — machine-proved in `proofs/lean_fp_equiv`), then rounds
  f32→bf16. That final narrow is a second rounding, and **double rounding here is
  not innocuous**: `RNE_p(RNE_q(x)) = RNE_q(x)` is *not* guaranteed by `p ≥ 2q+2`
  for round-to-nearest (a known fallacy — fails already at `p=4, q=1`). The bf16
  result differs from the correctly-rounded `a·b+c` in **~0.37 % of finite
  inputs, always by 1 ULP**. Reproducible witness: `a=0x2a20, b=0x51a6,
  c=0x9359` → arch `0x3c50`, correctly-rounded bf16 `0x3c4f` (the f32 result
  lands exactly on a bf16 midpoint, so the narrow ties-to-even up). The earlier
  "deep-subnormal check" missed it (these are normal-range), and the §8.2
  differential harness cannot catch it — its DPI reference (`dpi_ref.cpp:50`,
  `narrow_bf16(__builtin_fmaf(...))`) is *itself* f32-accumulate, so RTL and
  reference double-round identically by construction.

  This is a sound, mainstream design, not a bug. The f32→bf16 narrow is
  **bit-identical to PyTorch's `round_to_nearest_even`**, and arch's bf16
  `mul`/`add`/`sub` match PyTorch's `c10::BFloat16` operators bit-for-bit; arch's
  *fused* fma is in fact **more accurate** than PyTorch's scalar `a*b+c` (which
  has no fma and rounds the product to bf16 first — differs from arch on ~1.2 %
  of inputs). It also mirrors the NVIDIA Tensor Core / TPU f32-accumulate
  convention. What is *not* true is "correctly-rounded bf16 fma" — no mainstream
  hardware implements that. So `bf16_fma` is correct **for f32-accumulate
  semantics** (the f32 fma is machine-proved; the narrow matches PyTorch), and is
  verified end-to-end by §8.2 against the matching f32-accumulate reference.
