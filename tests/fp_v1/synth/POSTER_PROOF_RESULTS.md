# Poster proof scope and reproducible checks

Post-freeze follow-ups; frozen manuscript unchanged. Results below describe
specific Lean models and frozen RTL fixtures, not a compiler-wide theorem.
Run artifacts are intentionally omitted from version control. The commands
regenerate proof checks, statuses, logs and mutation counterexamples.

## Reference toolchain

- Lean `leanprover/lean4:v4.30.0`; Mathlib/dependencies pinned by
  `proofs/lean_fp_equiv/lake-manifest.json` (Mathlib v4.30.0).
- Yosys 0.67+post, commit `b8e7da6f40ae8f552c116bf6c359b07c6533e159`.
- SymbiYosys v0.67; Bitwuzla 0.9.0; Python 3.
- Cargo for the optional shared-IR/model freshness check; pdflatex for the poster.

Other tool versions are not asserted equivalent. Each pipeline replay records
actual tool versions, runner/input hashes, generated task hashes and results.
The local reference run used AppleClang 21.0.0.21000099 for Yosys.

## Concrete block-dot theorem

`ArchFp.arch_block_result_cases` covers equal-length nonempty blocks with
1–32 elements in E2M1, E2M3, E3M2, E4M3 or E5M2. Elements must be finite;
E8M0 scale codes must differ from 255. Narrow storage padding is ignored.

- Finite results obey a mixed absolute error bound, including tree reduction,
  both sequential scale roundings and gradual underflow.
- Nonfinite results are classified as infinity or NaN and have an actual
  finite-input overflow witness in the tree or either scale stage.
- The final theorem does not assume `SummationSafe`, `checkBlock`, exact
  scaling, normal output, or externally supplied local rounding laws.
- No finite error claim is made for nonfinite output. NaN propagation through
  opposite-sign overflow is tested on a generic FP32 tree, without asserting
  that every such branch is reachable by the concrete low-precision formats.

## Bound

For exact products p_i, S = sum p_i, A = sum |p_i|, n the number of products,
and nonnegative scales X,Y, let

    u = 2^-24, eta = 2^-150, k = eta/u = 2^-126,
    gamma_d = d*u/(1-d*u).

The bound is

    |q - S*X*Y| <= (1+u)^2 * X*Y * gamma_d * (A+n*k)
                    + (2*u+u^2) * X*Y * |S|
                    + eta * ((1+u)*Y+1).

The public concrete theorem uses d=5 for any 1–32-element block, including
odd lengths. `scaled_tree_mixed_of_finite` is the more general depth-d theorem,
requiring n <= 2^d and (d+1)*u < 1. The weight-sum form in Lean is equal to
A+n*k by `magnitude_weight_sum`.

This is conservative: it charges an underflow allowance even at exact nodes.
The first scaling error is amplified by the second scale. Staged overflow
cannot be repaired by a later small scale, even if the exact S*X*Y is finite.

## Lean replay

From `proofs/lean_fp_equiv`, after installing the pinned elan toolchain:

```sh
lake exe cache get
lake build
lake build ArchFpEquiv.BlockExactChecks ArchFpEquiv.ConcreteBlockChecks ArchFpEquiv.BlockScalingCounterexample
lake env lean evidence/general_block_rounding_2026-11-21/Audit.lean
lake env lean evidence/arch_block_rounding_2026-11-21/Checks.lean
```

The last two commands print axiom dependencies and run additional edge cases.
Expect successful elaboration, no `sorryAx`, and no handwritten modeling
axioms. Existing native `bv_decide`/`native_decide` dependencies remain trusted;
this is not a claim of a minimal-kernel-only arithmetic development.
Third-party dependency caches can be reused. To check a fresh ARCH build,
remove only this project's `.lake/build` in a disposable checkout before
`lake build`; do not reuse ARCH oleans from another checkout.

From the repository root, the separate model freshness check is:

```sh
python3 proofs/lean_fp_equiv/scripts/regen_block_widen_model.py --check
```

It regenerates the six widening functions and compares FP32 add/multiply
with the current shared IR. It does not prove the Rust renderer correct.

## Independent temporal checks

| Fixture | Interface latency | Reset | Harness | Baseline tasks | Injected faults |
|---|---:|---|---|---:|---:|
| FMA6 | 6 | high | strengthened | 4 PASS | 6 FAIL + VCD |
| FMA7 | 7 | low | strengthened | 4 PASS | 6 FAIL + VCD |
| E2M1 block dot, N=3 | 5 | high | strengthened | 4 PASS | 6 FAIL + VCD |
| E5M2 block dot, N=3 | 5 | low | strengthened | 4 PASS | 6 FAIL + VCD |
| E5M2 block dot, N=32 | 8 | low | history | 4 PASS | 6 FAIL + VCD |

These are the historical November 23 results: 20 baseline tasks and 30
mutation tasks. The four baseline tasks are startup BMC, induction,
arbitrary-state window, and reset/refill cover. Only startup plus induction
on the same design establish the stated all-cycles result after initial reset.
Mutations exercise skew, extra latency, reset polarity, reset value,
fill-valid reset and premature valid gating.

Follow [SBY_PIPELINE.md](SBY_PIPELINE.md) for the three replay commands.
Concrete bit-vector arithmetic is used. The combinational reference shares
compiler-generated arithmetic with the DUT; this independently checks time,
wiring and reset behavior, not IEEE arithmetic correctness. The scope excludes
stalls, asynchronous reset, external ready/valid and arbitrary parameters.
Yosys, solver and reference harness remain trusted. No SMT result is imported
as a Lean certificate.

## Poster

`output/pdf/neurips_verified_fp_poster.tex` and the accompanying PDF contain
post-freeze asterisks for these follow-ups. Build with `pdflatex` from that
directory. Generated TeX auxiliary files and rendered previews are not required
publication inputs.

## Pruned-package validation (23 November 2026)

A separate checkout of base `3db727d7` plus the 46-file publication patch
passed the fresh Lean project build, three regression-module builds, both
axiom/edge-case audits, shared-IR model freshness check and five runner tests.
Only third-party Lake packages/oleans were reused; all nine package revisions
matched `lake-manifest.json`, with clean tracked dependency sources. No ARCH
proof outputs were copied. The axiom audits contain no `sorryAx`; native
proof-evaluation trust remains as described above.

All 50 SymbiYosys tasks were regenerated and rerun using the three documented
commands: 20 baseline PASS, 30 mutation FAIL with VCD counterexamples, and
all five startup/induction pairs passed on identical designs. Cover tasks
also produced witnesses. Every generated design matches its historical
hash. Forty SBY configurations match exactly; the ten E2M1/3 tasks differ
only by a 180-second timeout in place of the historical 90 seconds. The
historical configurations were separately regenerated with exact hashes.
All ten 32-element designs and SBY configurations match exactly, including
the 30-minute timeout. This fresh replay does not depend on any of the
omitted historical run outputs.
