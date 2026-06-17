# K-Induction for `arch formal` — Unconditional Safety Proofs

**Date:** 2026-06-17
**Status:** Proposal — not yet started
**Area:** Formal verification backend (`src/formal.rs`)

---

## Problem

`arch formal` today does pure **bounded model checking (BMC)**: it unrolls the
design for `--bound N` cycles and asks a solver whether any `assert` can be
violated within that window.  A "PROVED" verdict means "no violation was found
in cycles 0..N" — not that the property holds forever.

This is fine for finding bugs fast, but it leaves a gap for safety-critical
properties that need to hold unconditionally:

- "This FSM state register is always in {0, 1, 2}" — BMC proves it for 10
  cycles but can't prove it for cycle 10,000.
- "The occupancy counter of a credit channel is always ≤ DEPTH" — BMC proves
  it for 20 cycles; a real proof needs induction.
- "No two arbiters ever grant simultaneously" — engineers filing formal results
  need `PROVED`, not `PROVED UP TO BOUND 15`.

The limitation affects every team that uses `arch formal` as a first-class
verification sign-off, not just a bug-hunt tool.  Today they work around it by
running external tools (EBMC in induction mode, SymbiYosys) which re-introduces
the toolchain dependency `arch formal` was designed to eliminate.

---

## What k-Induction Adds

K-induction is the standard complement to BMC.  For a property P and induction
depth k, it proves P holds *for all cycles* by checking two conditions:

1. **Base case** (already done): P holds at cycles 0..k.
2. **Inductive step** (new): if P holds for k *consecutive* cycles starting
   from any reachable state, then P holds at cycle k+1.

If both pass, P is an invariant — unconditionally.  The "reachable state"
qualifier is key: the unconstrained initial state in the step case can include
states the design can never actually reach, which causes the step case to fail
for some properties even though they are true.  The canonical fix is to
strengthen the invariant (add lemmas that constrain what states are reachable),
but that requires user guidance.  V1 leaves strengthening to the user and
surfaces the failure clearly.

---

## Proposed CLI Surface

```bash
# BMC only (current behavior, unchanged)
arch formal Counter.arch --bound 20

# BMC + inductive step (depth-1 induction, default)
arch formal Counter.arch --induct

# Explicit induction depth
arch formal Counter.arch --induct --induct-depth 3
```

### New output lines

```
assert count_safe: PROVED (inductive, k=1) — holds for all cycles
assert count_safe: STEP CASE FAILED — property may need invariant strengthening
    Hint: try adding a `cover` for intermediate states or add a strengthening assert.
cover state_hit:  HIT at cycle 4
```

Exit codes remain the same: 0 all proved/hit, 1 any refuted/not-reached, 2 inconclusive.
`STEP CASE FAILED` maps to exit 2 (inconclusive) — not a counterexample, not a proof.

---

## Implementation Approach

All changes are local to `src/formal.rs` (~3129 lines).

### 1. New CLI flag

In `FormalArgs`:
```rust
pub induct: bool,
pub induct_depth: u32, // default 1
```

In `src/main.rs` arg parsing: `--induct` sets `induct = true`; `--induct-depth
N` (only meaningful with `--induct`).

### 2. New `PropertyStatus` variants

```rust
pub enum PropertyStatus {
    Proved(u32),             // BMC bound
    ProvedInductive(u32),    // induction depth k
    StepFailed(u32),         // induction depth k — step case open
    Refuted(u32),
    Hit(u32),
    NotReached(u32),
    Inconclusive(String),
}
```

### 3. The inductive step encoder

The existing BMC encoder (`emit_bmc_formula`) already:
- Declares register state vars `reg_t` for each cycle t
- Encodes the next-state function as SMT assertions
- Emits `(assert (or neg_prop_0 neg_prop_1 ...))` for the bad-cycle check

The inductive step needs a second encoding that:
1. Declares **unconstrained** initial state vars `reg_s0` (no init-value
   constraints — the key difference from BMC).
2. Unrolls the next-state function for k+1 cycles, naming states `reg_s0 ..
   reg_sk`.
3. Asserts `(assert prop_si)` for i = 0..k-1 (the induction hypothesis).
4. Adds `(assert (not prop_sk))` (the step goal).
5. Runs `(check-sat)`.

If UNSAT → step case holds → `ProvedInductive`.
If SAT → step case fails → `StepFailed` (with a model to inspect).

This is ~100-150 new lines of SMT-LIB2 emission, reusing the existing helper
functions (`emit_next_state`, `emit_prop_at`, etc.) that BMC already calls.

### 4. Interaction with `--auto-thread-asserts` and construct SVA

The SVA properties auto-emitted by `--auto-thread-asserts` (wait_until,
wait_stay, wait_done, branch) and construct-level SVA (FIFO no-overflow, FSM
legal-state) are good candidates for inductive proofs — they are designed to
hold globally.  `arch formal --induct` will attempt induction on every `assert`
in the design, just as BMC does.  No special-casing is needed.

### 5. What stays out of scope for v1

- **Automatic invariant strengthening** (IC3/PDR-style): if the step case
  fails, the user must add a strengthening `assert` or `cover` manually.  This
  is well-understood by formal users and avoids a major complexity spike.
- **Vec / struct / enum / multi-clock**: same restrictions as current BMC.
- **Hierarchical designs**: same flat-module restriction as current BMC v1.

---

## Rationale and Value

| Stakeholder | Current pain | After this |
|---|---|---|
| HDL engineer filing a formal sign-off | "PROVED UP TO BOUND 20" is not a sign-off | "PROVED (inductive, k=1)" is |
| LLM generating ARCH designs | Relies on BMC; deep bugs invisible | Inductive step catches invariant violations BMC misses |
| CI pipeline | Must set a magic `--bound` high enough | `--induct` is bound-independent |

This is a natural, low-risk extension of the existing infrastructure. It
doesn't change any language surface, doesn't affect SV codegen, and doesn't
require external tools beyond the solver already used for BMC.

---

## Estimated Scope

- **`src/formal.rs`**: ~200-300 new lines for the step encoder + new status
  variants + updated result printer.
- **`src/main.rs`**: ~10 lines for the new CLI args.
- **Integration tests**: 4-6 new test cases:
  - A counter where `count <= MAX` is proved inductively.
  - A counter where `count < MAX` is refuted (base case fails) — existing test.
  - A property where the step case fails without a strengthening lemma (expected).
  - The same property after adding a strengthening `assert` — proved inductively.
  - A `cover` property still resolves as HIT/NOT_REACHED even with `--induct`.

Total estimated effort: 1-2 focused sessions.

---

## References

- [Bradley & Manna, "Checking Safety by Inductive Generalization"](https://link.springer.com/chapter/10.1007/978-3-540-24730-2_2)
- Existing `src/formal.rs` BMC encoder (lines 1-3129) — all helpers are reusable.
- `doc/plan_hierarchical_formal.md` — related but orthogonal (hierarchy vs. induction).
- EBMC's `--k-induction` flag — the external-tool equivalent this feature would replace.
