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

- **float→int** is proved in-range only — SMT-LIB `fp.to_sbv`/`fp.to_ubv` are
  *partial* (undefined for NaN / out-of-range), so the saturation / NaN→type-max
  corners are signed off by the §8.2 differential campaign, as §8.1 anticipates.
- **RNE arithmetic** (`mul add sub fma`) is generated from the same IR (run
  `dump_fp -- proof mul`), but its 2^64 / fused miter is not solver-tractable
  (z3 times out). It stays on the §8.2 differential Verilator campaign
  (`fp_rtl_differential_equiv_verilator`), which checks it bit-exact against a
  host-IEEE-754 reference over corner + randomized + cancellation-prone vectors.
  A tractable arithmetic proof (narrower datapath encodings, or a dedicated FP
  equivalence checker) is the remaining P3 item.
