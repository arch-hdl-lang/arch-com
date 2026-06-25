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
  theorem prover (Lean/Coq) is the natural route for the multiplier ops.
- **`bf16_fma`** is *correct* — via f32 the double rounding is innocuous (f32
  keeps a 16-bit precision lead over bf16 at every magnitude, ≥ the `2p+2`
  margin since `p ≤ 8`; confirmed by an exhaustive deep-subnormal check) — but
  its proof needs `fp.fma`, whose z3 4.8.12 support is incomplete (it returns a
  *spurious* `sat` whose own witness satisfies the equivalence). cvc5 `--fp-exp`
  handles `(8,8)` but times out. So `bf16_fma` is verified by the theorem +
  §8.2, not yet machine-discharged; a solver with sound `fp.fma` at `(8,8)`
  would close it.
