# Plan: a CompCert-style correctness harness for arch-com

Status: **prototype landed** for ARCH → SystemVerilog codegen of a module
subset (`arch build --emit-codegen-proof-lean`). This document states the
approach, what is proved today, what is trusted, and the roadmap for the rest
of the pipeline.

## 1. Approach: certified translation validation, not a verified compiler

CompCert proves the compiler once: every pass is written in Coq and proved to
preserve the semantics of its input, then extracted to OCaml. Doing that for
arch-com would mean rewriting a ~61 kLOC Rust compiler in a proof assistant.

arch-com takes the route CompCert itself takes for some passes (register
allocation, for example): **translation validation**. On every compilation,
the compiler emits a certificate. A small checker, written and **proved sound
once** in Lean 4, decides whether the output program has the same behavior as
the input. The compiler stays in Rust and stays untrusted. A miscompile shows
up as a certificate that does not check, never as a false proof.

```
           ┌────────── untrusted ──────────┐
 .arch ──► │ parse → elaborate → lower →   │ ──► .sv text
           │ typecheck → codegen (Rust)    │
           └───────────────────────────────┘
   │ checked AST                                  │ emitted text
   ▼ AST→Core (trusted, small)                    ▼ sv_reader (trusted, small,
 Core.Module ─────────► check core sv ◄──────── Sv.Module   shares no code with codegen)
                         = true  (kernel `decide`)
                            │ check_sound (proved once)
                            ▼
      ports equal ∧ clock equal ∧ ∀ trace, Core.Run ↔ Sv.Run
```

Why this choice:

* **It composes with what exists.** Thread lowering and first-class
  constructs already emit per-build Lean certificates
  (`src/thread_proof_cert.rs`, `src/construct_proof_cert.rs`).
* **It survives refactors.** The proof covers the *output*. It does not depend
  on how `codegen/` is written, so codegen can keep changing without
  re-proving anything.
* **Failures are actionable.** A rejected certificate names a module. The
  checker's normalized forms show exactly which process differs (§5).

## 2. Audit: what was proved before this work

| Asset | What it establishes | Gap |
|---|---|---|
| `proofs/lean_thread_lowering/ArchThreadLoweringProof/*` | A certified FSM table has the same trace as the source thread table. | Actions and guards are opaque `Nat`s. The theorem relates two abstract tables, not ARCH semantics to SV semantics. |
| `proofs/lean_thread_lowering/ArchConstructProof/*` | Generic FIFO/arbiter/pipeline/credit-channel step relations, instantiated per build. | Models the construct's *specification*. Nothing ties the emitted SV to that model. |
| `proofs/lean_fp_equiv/*` | FP rounding and equivalence results, sorry-free. | `ScaledDot.lean` carries 5 named `axiom`s (proof debts: `products_exact`, `scale_mul_exact`, `NoScaleOverflow`, `AllFiniteNoOverflow`, `pairwise_sum_error_bound`). |
| `doc/thread_lowering_proof.md` | Paper proof of thread lowering. | It states that codegen correctness is "a separate proof"; that proof did not exist. |
| nightly backend equivalence, `arch formal` | Testing and bounded model checking. | Evidence, not proof. |
| CI | — | No workflow installed Lean, so no Lean proof was checked in CI. |

The missing piece was a **semantics of SystemVerilog** to state correctness
against. That is what this work adds.

## 3. Semantic foundations (Lean, `proofs/lean_thread_lowering/ArchCodegenProof/`)

| File | Role |
|---|---|
| `Core.lean` | **Normative ARCH-Core semantics.** This is the specification of what a module means. It implements the width table of `ARCH_HDL_Specification.md` (§"Arithmetic widths"). |
| `Sv.lean` | **IEEE 1800 subset semantics.** Context-determined sizing (§11.6.1, Table 11-21), signedness (§11.8.1), extension (§11.8.2), size casts (§6.24.1), `assign` / `always_comb` / `always_ff` with nonblocking assignment. |
| `Sir.lean` | Sized IR, where every operator carries its width. Contains the normalizer and its soundness theorem `Sir.eval_norm`. |
| `Netlist.lean` | Processes, blocking/nonblocking execution, trace semantics `Netlist.Run`, and the decidable comparison `Netlist.sameB` with `run_iff_of_sameB`. |
| `Validate.lean` | `check : Core.Module → Sv.Module → Bool` and `check_sound`. |

**Trace semantics.** A run assigns a full valuation to every cycle. In each
cycle, every combinational process (`let`, `comb`, `assign`, `always_comb`)
is *stable*: executing it changes nothing. Each register takes its
next-state value in the next cycle. Inputs and the initial state are
unconstrained, so `∀ tr, Core.Run tr ↔ Sv.Run tr` covers every input stream
and every reset or power-up state.

The fixpoint reading of combinational logic is deliberate:

* For acyclic logic it has exactly one solution per input/register valuation,
  which is what simulators compute.
* For cyclic logic both sides admit exactly the same solutions.
* The checker proves the processes equal one by one, so any process-based
  evaluation order (an event-driven simulator, for example) also agrees.

**Normalizer.** `Sir.norm` rewrites both lowerings toward one canonical form.
Each rule is proved value-preserving:

* fold literal resizes and sign extensions;
* drop resizes that cannot change a value;
* push truncation through low-bit operators (`+ - * & | ^ ~ ?:`);
* shrink an operator's width to its operands' natural range.

Nonblocking blocks to distinct registers are compared as sets (`Stmts.eqvN`).
Straight-line combinational blocks are split into single assignments
(`Stmts.split`). The checker is sound by construction. Every *incompleteness*
found so far was fixed by adding a proved rule (§5), never by changing the
Rust side to make a check pass.

## 4. Trusted computing base

| Component | Size | Why trusted / how to shrink |
|---|---|---|
| Lean kernel | — | Standard. Certificates use `decide` (kernel evaluation), not `native_decide`. The emitted theorem depends only on `propext` and `Quot.sound` (checked by test). |
| `Core.lean` semantics | ~140 lines of definitions (the lowering proofs in the same file are machine-checked, not trusted) | This *is* the ARCH spec for the subset. Review it against `ARCH_HDL_Specification.md`. One clarification it makes explicit: widening UInt `-` wraps modulo `2^(max+1)`, which the written spec does not state. |
| `Sv.lean` semantics | ~220 lines of definitions, including the `asyncShaped` guard | Hand-written from IEEE 1800-2017. The cast rule (§6.24.1, "as if assigned") is the one most worth re-reviewing. Shrink by cross-testing `Sv.evalCtx` against Verilator on random expressions (roadmap A6). |
| AST → Core + Lean printer (`src/codegen_proof_cert/mod.rs`) | ~630 lines | Mostly structural. Literals get their minimal width. Shrink by having elaboration/typecheck certificates target Core directly (roadmap B). |
| `sv_reader.rs` | ~800 lines | Shares no code with codegen. Shrink by moving the reader into Lean, i.e. printing `Sv.Module` in Lean and comparing text (roadmap A7). |

Assumptions:

* 2-state logic, matching `arch sim` (no X/Z).
* One clock, abstracted to the cycle step.
* `init` declaration initializers are excluded, since the initial state is
  unconstrained.
* SVA under `translate_off` is not modelled. The reader currently rejects any
  module that contains it.

## 5. Prototype results (this PR)

* **Covered subset:**
  * Ports: `Clock`, synchronous or asynchronous `Reset<_, High|Low>`,
    `UInt<N>`/`Bool`.
  * Parameters: integer `param NAME: const` (and `local param`), certified
    at their **default values**, i.e. the module exactly as `arch build`
    emits it. Instances that override parameters are separate elaborations
    and are not covered (roadmap A5). The SV reader evaluates the emitted
    `parameter int` defaults itself; it does not take the compiler's values.
  * Body: `reg` with a reset, `wire`, typed `let`, `comb`, and one
    `seq on clk rising`.
  * Statements: assignment and `if`/`elsif`/`else`.
  * Operators: `+ - * +% -% *% & | ^ ~`, comparisons, `and or not`, `?:`,
    `.trunc/.zext/.resize`, constant slices (emitted as `[hi:lo]` or
    `[base +: w]`).
* **Asynchronous reset semantics.** ARCH-Core: in a cycle where an async
  reset is asserted, every register shows its reset value without a clock
  edge; the clock edge itself sees the pre-edge reset level. SV: an
  `or posedge|negedge rst` entry runs the `always_ff` body on the cycle's
  own state when the entry is at its active level. That reading of IEEE
  event semantics is exact only for the shape `if (<active level>)
  <constant assignments> else …`, which `Sv.Module.asyncShaped` enforces
  inside `check`.
* **Fixtures** (`tests/codegen_proof/*.arch`): all six certify.
* **Negative controls** (`tests/codegen_proof_cert_test.rs`): 18 hand-made
  miscompiles are all rejected by the kernel.
  * 11 core cases: a dropped carry, an inverted reset, a wrong reset value,
    a wrong operator, a narrowed port, a wrong flop clock, a wrong literal,
    a truncated difference, a wrong slice, a flipped `else if`, and a
    narrowed product.
  * 7 for parameters and async reset: a wrong parameter default, a wrong
    reset parameter, a shifted part-select, an async reset made synchronous
    and the reverse, and an inverted async edge or level.
  * 6 semantics-preserving rewrites are accepted: a redundant cast, a
    different-but-equivalent cast width, reordered resets, a sized vs
    unsized literal, a parameter vs literal cast width, and `+:` vs `[hi:lo]`.
* **Corpus sweep** (`scripts/codegen_cert_sweep.sh`, 964 `.arch` files):
  * **111 files certify, with 0 failures**, including `tests/e203/e203_core`
    and `e203_cpu`. The count was 78 before parameters and async reset.
  * Earlier sweeps exposed only validator incompleteness, each fixed with a
    proved rule: a 32-bit signed `?:` vs a 1-bit ARCH mux, a 32-bit vs
    6-bit product under a 4-bit truncation, and reordered resets
    (`Stmts.eqvN`).
  * The two e203 certificates first exceeded the elaborator's `maxRecDepth`
    under `decide`. Certificates now use `decide +kernel`, which evaluates
    in the kernel directly (same trusted base).
  * **No miscompile was found in the covered subset.**
* **Cost:** most certificates replay in 1–2 s; `e203_core` takes about 18 s.
  The whole sweep takes about 20 s with 8 jobs.

Why modules are skipped (first reason per module, 884 module-level reasons
over the sweep): bus ports 118, untyped `let` 115, `port reg`/`pipe_reg`
ports 95, `inst` 85, unsupported expression forms 75, `reg … init` 58,
non-integer parameters 57, `Vec`/struct/enum data types ≈ 119 (ports, regs
and wires together), more than one `seq` block 19, `assert`/`cover` 15,
signed types 15, `<<` 14.

## 6. Roadmap

Sizes are rough relative estimates (S < 1 week, M 1–3 weeks, L > 3 weeks of
focused work). They are not commitments.

### A. ARCH → SV codegen (extend the prototype)

Ordered by the skip-reason evidence in §5.

1. **Parameters — done for integer `const` parameters at default values.**
   Non-integer parameters (type, enum, `Vec`, width-qualified) remain.
2. **Asynchronous reset — done** (semantics in §5).
3. **Types (M).** `SInt`: signed compare (`Sv.cmpInt` already exists), `sext`,
   `$signed`. Packed `Vec`/struct/enum: flatten to bit ranges in Core, with
   SV part-selects.
4. **Untyped `let`, `port reg`/`pipe_reg` ports, `reg … init`, shifts,
   concatenation/replication, `Index` (S each).** Untyped `let` (115) and
   `port reg` (95) are the largest single items after bus ports.
5. **Hierarchy (L).** `inst` with a compositional theorem: if every module
   certifies, the flattened design certifies. Also bus-port flattening.
6. **SV semantics cross-check (S).** Differential-test `Sv.evalCtx` (it is
   executable) against Verilator on generated expressions, to raise
   confidence in the trusted SV model.
7. **Remove `sv_reader` from the TCB (M).** Pretty-print `Sv.Module` in Lean
   and compare it with the emitted text, or write a Lean parser.
8. **First-class constructs (L).** fsm, fifo, ram, counter, arbiter,
   pipeline: certify their codegen by lowering each to Core inside the
   certificate. Then re-base the existing `ArchConstructProof` models on Core
   semantics, so the construct spec, Core and SV form one chain.
9. **SVA regions (S).** Prove that `translate_off` blocks contain only
   `assert`/`cover`, and drop them from the semantics.

### B. Elaboration and lowering

Every lowering pass has ARCH-Core (or Core plus the construct being lowered)
as its target. Each gets a validator of the same shape:
`check_pass input output = true → ∀ tr, Run input tr ↔ Run output tr`.

1. **Thread lowering (M).** Replace the opaque `Action := Nat` in
   `ArchThreadLoweringProof` with Core statements. That makes the existing
   per-build certificate a real link: thread source ≈ lowered Core ≈ SV
   (via A).
2. **generate / param / auto-connect (M).** Validate the elaborated module
   against an unrolled reference elaboration in Lean (generate-for is
   structural).
3. **TLM lowering, pipe_reg / credit-channel lowering (L).**

### C. Type-checker soundness

1. **(S) Per-program.** A decidable `Core.wellTyped` in Lean, asserted by
   every certificate. It requires: no truncating assignment; ARCH width rules
   satisfied; single driver; complete assignment in `comb` (no latch);
   acyclic combinational dependencies. This catches typechecker
   unsoundness on each compiled program.
2. **(M) Determinism theorem.** `wellTyped → ∃! settled state` per cycle.
   This bridges the relational trace semantics to the deterministic
   simulator semantics.
3. **(L) Full theorem.** An ARCH typing relation with progress/preservation
   for the elaborated language.

### D. Simulator agreement (`arch sim`)

1. **(S) Reference simulator.** `Core.Module.edge` and the comb processes are
   executable. A Lean-compiled reference simulator can be run
   differentially against `arch sim` on random stimulus. This is testing,
   not proof, but it is cheap and immediately useful.
2. **(L) Sim-codegen validation.** Read back a C++ subset into a Lean model,
   as `sv_reader` does for SV, or emit the sim from Core inside Lean.

### E. Infrastructure

* `.github/workflows/lean-proofs.yml` (this PR) builds the Lean library and
  runs the certificate tests on PRs that touch proofs or codegen, plus a
  nightly corpus sweep. It is opt-in and path-filtered, per the dependency
  policy in `test.yml`.
* Track the sweep's CERTIFIED count over time as the coverage metric.
* Any FAIL is triaged as either a miscompile (file a bug) or a validator gap
  (add a proved rule).

## 7. Usage

```sh
scripts/cloud_setup.sh                                 # Lean + Verilator 5.034 (fresh box / cloud session)
(cd proofs/lean_thread_lowering && lake build)        # once per checkout
arch build Foo.arch --check-codegen-proof-lean         # emit + replay
arch build Foo.arch --emit-codegen-proof-lean=Foo.lean # emit only
scripts/codegen_cert_sweep.sh                          # whole corpus
```

Modules outside the subset are listed in the certificate header and on
stderr as **not covered**. The flag fails only if no module is covered.
