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

## Renderer faithfulness — the Yosys-to-SMT miter (`renderer_miter.sh`)

The proofs above check `render_smt`'s model; the shipped artifact is
`render_sv`'s SystemVerilog. `renderer_miter.sh` closes that gap mechanically:
Yosys — an independent implementation of SystemVerilog semantics — reads the
emitted SV (structure-preserving: `read` + `proc` + `flatten`, no
optimization) and exports SMT2; a solver then checks that export equivalent
to `render_smt`'s `define-fun` of the same IR. `unsat` means a divergence
between the two renderers would have to be a bug shared with Yosys's
independent SV frontend.

The fma miters are SAT-hard monolithically (the renderers implement the
variable alignment/normalize shifts as different circuits, with the 48-bit
product in every shifted bit's cone), so the harness case-splits them on the
alignment gap `diff = |eunb(a)+eunb(b) − eunb(c)|`: 509 constant cases plus a
range catchall, each near-structural. Coverage is by construction — every
input satisfies exactly one case — so the split predicate itself need not be
trusted.

Three mechanical transformations sit inside this path and are part of its
trust base: `../synth/hoist_decls.py` (syntactic declaration hoisting — yosys
cannot parse declaration-with-initializer inside functions), yosys's
`opt_clean` dead-wire removal after flattening, and the state-sort
specialization of the SMT export (instantiating yosys's state-parametric
functions at the single state constant). Each is local and reviewable; modulo
those, a `render_sv`/`render_smt` divergence would have to be a bug shared
with yosys's independent frontend.

Result (2026-07-26, **all 24 operators `unsat`**; bitwuzla 0.9 / z3 4.15,
Yosys 0.67; times on an 8-core M-series):

| operator group | verdict | time |
|---|---|---|
| f32 add / sub | unsat | ~12 s each |
| f32 mul | unsat | 14 s (z3; bitwuzla stalls — solver variance, auto-fallback) |
| f32 fma | unsat | 471 s (510-way split, 8-way parallel) |
| bf16 add / sub / mul | unsat | 4–23 s |
| bf16 fma | unsat | 245 s (510-way split on converted operands) |
| all 12 comparisons | unsat | <1 s each |
| widen / narrow conversions | unsat | <1 s each |
| f32→s64 / u64 (saturating) | unsat | <1 s each |

Not covered: the Lean renderer, whose check remains the byte-identical
regeneration audit.

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

## FP8 (E4M3 / E5M2) — 2026-08-01

Both OCP OFP8 formats are covered by the same generated-miter machinery
(`fp8_smt_proofs`, both `--fp-compat` profiles, all `unsat` in z3 4.15):

| op(s) | spec | input space |
|---|---|---|
| `e5m2_eq … e5m2_ge` | `fp.eq/lt/…` on `(_ FloatingPoint 5 3)` | 2^16 |
| `e5m2_widen` / `e5m2_narrow` | exact widen / RNE round to `(5,3)` (+ cuda satfinite wrapper) | 2^8 / 2^32 |
| `e5m2_mul` | `fp.mul` on `(5,3)` | 2^16 |
| **`e5m2_add` / `e5m2_sub`** | exact result in `(_ FloatingPoint 8 53)`, ONE `(5,3)` rounding | 2^16 |
| `e4m3_widen` | hand two-region OCP spec (IEEE `(4,4)` below exp 15 + 7 top-binade constants) | 2^8 |
| `e4m3_eq … e4m3_ge` | IEEE compares on the (grounded) widened values | 2^16 |
| `e4m3_narrow` + `e4m3_mul/add/sub` | two-region OCP round: `(_ FloatingPoint 8 4)` normals (≥480 overflows, profile result), scaled `fp.roundToIntegral` subnormals | 2^32 / 2^16 |
| **`e4m3_fma_cr`** | exact fma in `(_ FloatingPoint 8 37)`, ONE OCP rounding | **2^24 — `unsat`, ~2 min/profile** |

Notes:

- **E4M3 is not an IEEE format** (no infinities; the top exponent is finite
  except at an all-ones mantissa), so there is no direct SMT sort for it. The
  hand-written two-region spec is grounded by the `e4m3_widen` miter (the IR
  widen equals the spec over all 2^8 encodings); every other e4m3 proof then
  decodes results through that proven widen.
- **`e5m2_add`/`e5m2_sub` use the exact-wide formulation** — z3 4.15 returns
  `unknown` on `fp.add` miters at `(5,3)` even with pinned operands (a
  rewriter incompleteness; `fp.mul` and `to_fp` on the same sort discharge
  fine, and bitwuzla is built without `--fpexp` so it cannot parse the sort
  at all). Computing the exact sum in `(_ FloatingPoint 8 53)` and rounding
  once into `(5,3)` is the same correctly-rounded spec and avoids `(5,3)`
  `fp.add` entirely. This also means the E5M2 add/sub double rounding
  (f32 first, then fp8) is **proved** innocuous — not assumed from a
  `p ≥ 2q+2` margin, which is a fallacy for round-to-nearest (see the
  bf16_fma note above).
- **`e4m3_fma_cr` upgrades the E4M3 fma to proven correctly rounded**: the
  fused f32-accumulate equals a true CR reference (exact fma in `(8,37)`,
  one OCP rounding) over all 2^24 triples, both profiles. Confirmed
  independently by the exhaustive characterization
  (`examples/fp8_fma_char.rs`: 0 mismatches).
- **`e5m2_fma_cr` is `sat`, as expected** — E5M2 fma keeps the fused-f32
  convention and deviates from CR on 18960/2^24 inputs (0.113%, riscv) /
  15888/2^24 (0.095%, cuda), always by 1 ULP. Witness
  `fma(0x1E,0x7A,0x01)` = 288 + 2^-16: CR is 320, fused gives 256 (the f32
  step ties-to-even onto exactly 288 — the e5m2 midpoint — losing the
  sticky). Characterized, documented in §3.8, not asserted.
- The renderer miter gains 24 fp8 rows (arith, fma via the alignment-gap
  split with fp8-widen conversion, compares, conversions), and
  `fp8_sv_vs_sim_sweep` byte-compares the native sim's C++ helpers against
  the Verilated SV over the exhaustive 2^16 binary-op space plus a 3·2^25
  stratified narrowing sweep, both profiles.

### FP8 long-verification results (2026-08-01)

- **Renderer miter: 48/48 `unsat`** (bitwuzla 0.9 / z3 fallback, 1800 s cap,
  M-series 8-core) — the 24 fp8 rows plus a full regression of all 24
  pre-existing f32/bf16 rows. fp8 fma splits: E4m3Fma 138 s, E5m2Fma 133 s
  (vs 619 s F32Fma / 365 s Bf16Fma).
- **Full 2^32 narrowing sweep** (`ARCH_FP8_SWEEP_FULL=1`): native sim and
  Verilated SV report identical FNV-1a output hashes over all 4.3 G f32
  inputs for both `f32→e4m3` and `f32→e5m2` — riscv `ce51cf2a0d9d99ab`,
  cuda `e2b1a81a28dd99c3` — and the exhaustive 2^16 binary-op dumps are
  byte-identical under both profiles.

## OCP MX sub-8-bit storage formats (FP4 E2M1, FP6 E2M3 / E3M2) — 2026-08-10

These three are **all-finite**: no infinities, no NaN, every encoding is a
value. ARCH models them as carriers — arithmetic and `is_nan` are compile
errors, and a value must be widened to FP32 to compute — so the only operators
to prove are the two conversions per format (`mx_storage_smt_proofs`, both
`--fp-compat` profiles, all `unsat` in z3 4.15).

| op(s) | spec | input space |
|---|---|---|
| `e2m1_widen` | IEEE `(2,2)` below exp 3 + 2 top-binade constants (4.0, 6.0) | **2^4 — exhaustive** |
| `e2m1_narrow` | 2-region round: `(_ FloatingPoint 8 2)` normals (≥8.0 saturates), 0.5-grid `fp.roundToIntegral` subnormals | 2^32 |
| `e2m3_widen` | IEEE `(2,4)` below exp 3 + 8 top-binade constants (4.0 … 7.5) | **2^6 — exhaustive** |
| `e2m3_narrow` | as above at `(_ FloatingPoint 8 4)` / ≥8.0 / 0.125-grid | 2^32 |
| `e3m2_widen` | IEEE `(3,3)` below exp 7 + 4 top-binade constants (16, 20, 24, 28) | **2^6 — exhaustive** |
| `e3m2_narrow` | as above at `(_ FloatingPoint 8 3)` / ≥32.0 / 0.0625-grid | 2^32 |

Notes:

- **The top-binade constants are transcribed from the OCP value tables, not
  recomputed from `(eb, mb)`.** A spec derived by the same arithmetic the IR
  uses would agree with a wrong IR; the published numbers give a sign, bias or
  shift error nothing to hide behind. Same structure as `e4m3_widen`, minus its
  NaN arm — all-finite formats have no encoding to except.
- **Both profiles are asserted, and that is not redundant.** `f32_to_e2m1` and
  `f32_to_fp6` claim the two `--fp-compat` profiles *cannot* differ: with no
  Inf and no NaN in the encoding space, an overflow has nowhere to go but the
  max finite. These miters pin that claim rather than assuming it — the same
  saturating spec is asserted under each profile.
- **Mutation-tested when written** (z3 4.15.4, 18 mutants). Corrupting a
  top-binade constant, deleting the top-binade arm entirely, moving the
  overflow threshold, changing the subnormal grid spacing, or switching either
  rounding mode to RTZ flips every affected miter to `sat` — 15 killed.
- The 3 survivors are **equivalent mutants, and predicted**: moving the
  subnormal/normal split point from `min_normal` to `2 * min_normal` leaves the
  spec correct, because the fixed subnormal grid has the same spacing as the
  min-normal binade. Any split inside `[min_normal, 2*min_normal)` is sound —
  the same window `e4m3_narrow` documents. Pushing it to `4 * min_normal`
  leaves the window and is killed on all three formats, which is what makes the
  survival evidence of the window rather than of a gap.
### E8M0 — the block scale

E8M0 is not a float and not all-finite: no sign, no mantissa, no infinity, and
**no zero** — `0x00` is the minimum scale 2^-127, `0xFF` is NaN. Nothing about
it rounds, so it has no round spec. It gets an equivalence miter anyway, because
it is the format where every bug found while landing it was a float-shaped path
silently taking the f32 branch.

| op | spec | input space |
|---|---|---|
| `e8m0_widen` | anchor `w(0x7F)=1.0` + step `w(e+1)=2*w(e)` + `w(0xFF)` NaN + all scales positive & finite | **2^8 — exhaustive** |
| `e8m0_narrow` | `w(rr) <= |x| < 2*w(rr)` (floor power of two) + MX clamps | 2^32 |

- **Characterized, not transcribed.** The widen spec never mentions an exponent
  field — it states the defining multiplicative property, so a layout error has
  nothing to agree with. Anchor plus step determines all 255 scales by induction
  in both directions. A 255-entry constant table would have been the obvious
  alternative and a worse one: transcription at that length invites exactly the
  errors the spec exists to catch.
- **The step pins the no-zero subtlety for free.** `w(0x01) = 2 * w(0x00)` is
  only satisfiable if `w(0x00)` really is 2^-127 — an f32 *subnormal* — rather
  than the zero every other format would put there. Confirmed by three direct
  queries: `w(0x00)` equals `#x00400000`, is not zero, and is subnormal.
- **9 mutants, 9 killed**: moving the anchor, the step stride, the doubling
  factor, the NaN code, or the sign of the scales; and on the narrow, either
  clamp or either side of the floor bound.

- The renderer miter gains 8 rows (widen + narrow for all four MX formats).

## Retimed staged operators — latency equivalence (`staged_ops_miter.sh`) — 2026-08-28

`arch build --staged-ops` cuts a combinational FP operator's dataflow graph into
a registered pipeline so the design clocks faster (`fma<pipelined, 6>` ~ 260 MHz
vs ~180 MHz single-cycle). The proof obligation is that the staged datapath,
sampled at its latency `L`, computes exactly the single-cycle operator's
correctly-rounded result **for all inputs**. `arch formal` used to *refuse* any
design containing a `<pipelined, N>` call rather than encode it as an unverified
pipeline; with this proof discharged, the encoder now treats the pipelined call
as the single-cycle operator it is proven equal to, fed into the pipe_reg the
formal model already delays by `N` cycles (see `formal_pipelined_fma_*` in
`tests/formal_test.rs`, which proves the pipelined fma equals the combinational
fma delayed `N` cycles, inside `arch formal` itself). Until now that rested on a prose "by construction"
argument plus randomized lockstep *simulation*
(`tests/pipelined_fma_lockstep_test.rs`) — samples, not a proof. Simulation had
already missed one bug of this class (the scaled_dot scale-byte off-by-one,
arch#955 follow-up), which is invisible under stable/low-entropy stimulus.

The obligation decomposes into two independently-checkable lemmas:

| Lemma | What | How | Cost |
|---|---|---|---|
| **B — timing** | staged module is a *balanced* feed-forward pipeline: every input→output path crosses the same `L` registers | levelize the netlist, assert min FF-depth = max FF-depth = `L` for every output bit (`tests/fp_v1/synth/pipeline_balance.py`) | structural, **no solver** |
| **A — arithmetic** | register-shorted transfer function (`Q:=D`) equals the operator's `render_smt` model | combinational miter vs `arch_fma_f32`, reusing the single-cycle fma alignment case-split | SMT, 510 cases |

Balanced-at-`L` (B) plus register-shorted-function-equals-op (A) give, for a
feed-forward pipeline, `output[t+L] == op(input[t])` for all inputs. The
"balanced feed-forward ⟹ transfer = `L`-delayed comb function" step is the
standard retiming fact; the Lean development (`proofs/lean_fp_equiv`, Route B)
formalizes it once so A+B become a complete machine-checked proof for every
shape. Lemma B directly rules out the skew class Lemma A alone cannot see (a
pipeline that computes the right function but delivers a bit one cycle early
passes A and fails B).

**Results (`fma<pipelined, 6>`, 2026-08-28):**
- Lemma B: `ArchF32FmaStaged6` is **balanced at latency 5** (the 6th cycle to the
  user-visible output is the wrapper's reset/valid register — an ordinary
  `pipe_reg`, covered by the flat renderer miters). All 32 output bits: min = max
  FF-depth = 5.
- Lemma A: **all 510 alignment cases `unsat`** — the register-shorted staged fma
  is bit-identical to `arch_fma_f32`.
- **Non-vacuity**: the register-shorted miter goes `sat` for a wrong-op spec
  (`arch_f32_mul`) and for a single-bit corruption of `y`; `pipeline_balance.py`
  reports `UNBALANCED [1,2]` for a hand-skewed two-stage pipeline and the correct
  latency for the balanced one; sampling at the wrong depth (L=4 or L=6) is `sat`.

The staged block operators (`scaled_dot`, `scaled_quantize`, arch#955 / PR #960)
have rows too. Both run Lemma B (balance) — the check that catches the
non-power-of-two skew of arch#960, and skew generally.

**`scaled_dot` also gets a full Lemma A (arithmetic), via uninterpreted-function
abstraction** (mode `uf-dot`). Bit-blasting the register-shorted staged dot
against the comb dot times out even at N=2: the reduction tree is seven
`arch_f32_add`s with independent alignment gaps and no single collapsing split
(unlike fma's one gap). But the tree is a *straight line* of `arch_f32_add` /
`arch_f32_mul` / `arch_*_to_f32` calls, so `tests/fp_v1/synth/uf_datapath.py`
translates both datapaths to SMT with those primitives **declared uninterpreted**
— the miter then reduces to congruence over the wiring, `unsat` in milliseconds
for all inputs. That soundly proves the staged datapath applies the *identical
composition* of primitives as the comb operator; the primitives' own correctness
(that `arch_f32_add`/`arch_f32_mul` implement IEEE at full FP32) is discharged
separately by the renderer miters above, at native 32-bit width. Non-vacuity is
self-checked: corrupting one tree add flips the verdict to `sat`.

**The shared staged-multiply leaf `ArchF32MulStaged4`** (row `mulstaged4`) gets
its own Lemma A + Lemma B: it is balanced at latency 3, and its register-shorted
function equals `arch_f32_mul` — a *single* multiply, so a direct bit-blast miter
clears in seconds (no split). Every staged block op that instantiates it (the
eight parallel multiplies of `scaled_quantize`, etc.) therefore builds on a
proven-correct pipelined multiply.

**`scaled_quantize` stays `balance-only`** at the whole-operator level (Lemma A
deferred). With the multiply leaf now proven, the remaining gap is only its
*composition* — a `for`-loop max-reduction to pick the block scale, a `generate`
array of the (proven) multiply instances, and `? :` special-case narrowing.
That's beyond the straight-line UF extractor (which handles the `scaled_dot`
tree); covering it would need loop/generate/conditional elaboration. Balance +
the throughput lockstep + the proven multiply leaf cover it in the meantime.
`scaled_dot` is emittable only for power-of-two block sizes (the type checker
rejects the rest, arch#960).

Composing the pieces, the staged `scaled_dot` equivalence is now machine-checked
end to end: Lemma A (UF — same composition) ∘ leaf primitive correctness
(renderer miters) ∘ Lemma B (balanced latency) ∘ the Lean retiming lemma
(balanced feed-forward ⟹ `L`-delayed comb function).

A bounded slice — fma Lemma B + a Lemma-A smoke, both block-op balance checks,
and the `scaled_dot` UF Lemma A (with its mutation) — runs under `cargo test`
(`tests/staged_fma_equivalence_miter_test.rs`) when yosys/z3/python are present;
the full 510-case fma proof is the manual long-verification.
