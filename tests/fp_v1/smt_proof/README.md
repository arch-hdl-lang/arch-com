# FP RTL — SMT equivalence proofs (plan §8.1)

Machine-checked proofs that the emitted synthesizable FP SystemVerilog
(`src/codegen/fp.rs`) is equivalent to the SMT-LIB `FloatingPoint` theory, which
**is** IEEE-754 round-to-nearest-even. Transitively:

```
emitted SV  ≡  SMT fp.* (RNE)  ≡  IEEE-754
   (proved here)   (by the theory)
```

Each `.smt2` asserts the *negation* of the equivalence and asks z3 for a
counterexample; `unsat` means none exists — the property holds for **all**
inputs (exhaustive, not sampled). The `rtl_*` definitions in each file are a
literal transcription of the SystemVerilog bit-logic in `src/codegen/fp.rs`, so
these prove the emitted RTL directly (not a separate model).

Run via `cargo test --test fp_test fp_smt_equivalence_proofs` (auto-skips when
`z3` is not on `PATH`), or by hand: `z3 fp32_compare.smt2`.

| File | Operator(s) | Spec | Input space |
|---|---|---|---|
| `fp32_compare.smt2` | `arch_f32_{eq,ne,lt,le,gt,ge}` | `fp.eq/lt/leq/gt/geq` | 2^64 (all pairs) |
| `bf16_narrow.smt2` | `arch_f32_to_bf16` | RNE round f32→`(_ FloatingPoint 8 8)` | 2^32 |
| `bf16_widen.smt2` | `arch_bf16_to_f32` | exact widen bf16→f32 | 2^16 |
| `f32_to_sint.smt2` | `arch_f32_to_sint` (N=32) | `fp.to_sbv` RTZ, in-range | 2^32 |
| `f32_to_uint.smt2` | `arch_f32_to_uint` (N=32) | `fp.to_ubv` RTZ, in-range | 2^32 |

All currently discharge **`unsat`** under z3 4.8.12.

## Scope and deferrals (no silent caps)

- **float→int** is proved only for the **in-range** cases: SMT-LIB `fp.to_sbv` /
  `fp.to_ubv` are *partial* functions (undefined for NaN / out-of-range), so the
  saturation and NaN→type-max corners (plan §6) are signed off by the §8.2
  differential Verilator campaign instead, exactly as §8.1 anticipates.
- **RNE arithmetic** (`+ - *`, `fma`, and `int→float`) is **not** proved here.
  The emitted rounder uses packed-struct function returns and early `return` /
  `break`, which the available SV→SMT frontend (Yosys 0.33) cannot lift, and a
  from-scratch SMT rounder would be a second, unfaithful implementation. These
  operators remain backstopped by the §8.2 differential campaign
  (`fp_rtl_differential_equiv_verilator`), which checks them bit-exact against a
  host-IEEE-754 reference over corner + randomized + cancellation-prone vectors.
  A full formal sign-off of the arithmetic datapath (via EBMC or a frontend that
  accepts the emitted SV) is the remaining P3 item.
