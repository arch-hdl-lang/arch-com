# k-Induction for `arch formal` — Unbounded Safety Proofs

**Date:** 2026-06-27  
**Status:** Proposal  
**Area:** `arch formal` backend (`src/formal.rs`)

---

## Problem

`arch formal` currently does bounded model checking (BMC): it unrolls the
design for `--bound N` cycles and asks the solver "is there a trace of length
≤ N that violates this assertion?" When no counterexample is found, the tool
reports:

```
assert no_overflow: PROVED (up to bound 10)
```

This is **not** a proof. It means "no bug found in 10 cycles." The bound is
a user-chosen heuristic; if the shortest counterexample requires 11 cycles,
BMC silently misses it. For a FIFO occupancy invariant on a design with a
32-deep buffer, you would need `--bound 32` at minimum — and even then the
proof is only as strong as the bound. The `Proved` label in the output
misleads reviewers into thinking the property is established.

The limitation is structural: BMC is a refutation technique, not a proof
technique. Every "PROVED" from BMC today is actually "INCONCLUSIVE — no bug
found yet."

---

## Solution: k-Induction

k-Induction is the standard extension to BMC that produces genuine unbounded
safety proofs. It adds one extra solver call per property:

### Base case (existing BMC)
From the reset state, no violation in k steps. Same as today.

### Inductive step (new)
For an arbitrary initial state (not constrained to be the reset state),
if the property holds for k consecutive steps, does it necessarily hold
in step k+1?

Formally, for a safety property `P`:
- Hypothesis: `P(s_0) ∧ T(s_0,s_1) ∧ P(s_1) ∧ T(s_1,s_2) ∧ … ∧ P(s_{k-1})`
- Goal: prove `P(s_k)`

If the solver answers **UNSAT** (goal is always true given the hypotheses),
the property is proved for **all time**, regardless of bound. Combined with a
passing base case at depth k, the result is a genuine proof.

### Why ARCH designs are good candidates

ARCH modules have properties that make k-induction succeed quickly:

1. **Explicit clean reset**: every `reg` with `reset rst => 0` starts at a
   known state; the type checker enforces this. Reset gives the base case a
   tightly constrained starting point.
2. **No implicit latches**: the compiler already rejects designs where signals
   might retain unintended state. This limits the reachable state space.
3. **Strong typing**: `UInt<N>` width constraints bound the integer ranges.
   Many overflow/underflow properties fold into `k=1` induction because the
   state space is already bounded by type.
4. **Auto-emitted invariants**: `arch build` already emits `_auto_bound_*`,
   `_auto_div0_*`, and `_auto_cc_*` SVA assertions. These same properties,
   when fed to `arch formal --induction`, would give genuine proofs rather
   than bounded guesses.

For simple modules (counters, FIFOs, arbiters), `k=1` is typically sufficient.
For thread-bearing FSMs, `k` equal to the maximum state-chain length proves
liveness-free safety across the full FSM.

---

## CLI Shape

```bash
# Prove safety for all time (k=1 default — often sufficient)
arch formal --induction Counter.arch

# Increase induction depth if k=1 inductive step fails
arch formal --induction --k 3 Fifo.arch

# Combine: base at bound 20, inductive step at k=2
arch formal --bound 20 --induction --k 2 ArbitratedBus.arch

# Existing BMC-only mode unchanged (no --induction flag)
arch formal --bound 10 Counter.arch
```

Output when induction succeeds:

```
assert no_overflow:     PROVED ✓  (base k=1, inductive k=1 — unbounded)
assert no_underflow:    PROVED ✓  (base k=1, inductive k=1 — unbounded)
cover count_reaches_8:  HIT    ✓  (cycle 8)
```

Output when the inductive step fails to close (but BMC did not find a bug):

```
assert no_overflow:     INCONCLUSIVE  (base k=5 clean; inductive step open at k=1 — try --k 3 or add assumptions)
```

---

## Implementation Approach

The existing `FormalCtx` in `src/formal.rs` already emits per-cycle variables
`foo_t` for each register and runs one solver call per property. The changes
are localized:

### 1. Add `induction: bool` and `k: u32` to `FormalArgs`

Parse `--induction` and `--k N` in `src/main.rs` (same argument block as
`--bound`). Default k=1.

### 2. Add `emit_inductive_step()` method to `FormalCtx`

Mirror `emit_base()` but without the reset-state constraint. Instead:
- Declare fresh state variables for k+1 cycles (unconstrained initial state)
- Assert transition relations hold for each step (same transition emission as BMC)
- Assert `P(s_0) ∧ … ∧ P(s_{k-1})` as hypotheses
- The goal `P(s_k)` is the property to check

The solver is invoked with `(check-sat)` after negating the goal:
`(assert (not P_k))`. If UNSAT → inductive step holds.

### 3. Extend `run_property()` to run two solver calls when `--induction`

```
Step 1 (base):      same BMC run as today (up to bound k)
Step 2 (inductive): new inductive SMT problem
```

`PropertyStatus` gets a new variant:

```rust
ProvedUnbounded { k: u32 },  // base + inductive step both closed
```

Existing `Proved(u32)` is renamed `ProvedBounded(u32)` for clarity (it was
never a real proof — the renaming makes this honest in the output).

### 4. No new solver dependencies

All three supported backends (z3, boolector, bitwuzla) handle the inductive
step's QF_BV formula identically — it is structurally the same as a BMC
problem with different initial constraints.

---

## Scope Restrictions (v1)

Same restrictions as the current formal backend:
- Scalar types only (Vec/struct/enum error out — same as today)
- Single clock
- Hierarchical support: same one-level flattening as implemented

Thread-bearing modules (which lower to FSMs) already work with BMC today;
they work with induction too — the lowered state register and transition
function are scalar-typed.

---

## What This Unblocks

1. **Auto-emitted SVA properties become formally provable, not just bounded-checked.**
   `_auto_bound_vec_0`, `_auto_div0_div0_0`, `_auto_cc_<ch>_credit_bounds` —
   all already in the `assert` AST. With `--induction` these graduate from
   "no bug in N cycles" to "proved for all time."

2. **User-written invariants get genuine proofs.** A FIFO's occupancy
   invariant (`0 <= count <= DEPTH`) proves in k=1 induction without needing
   `--bound DEPTH`. Currently users set `--bound 100` and call it good.

3. **Honest reporting.** The output stops saying `PROVED (up to bound 10)`.
   A genuinely proved property says `PROVED (unbounded)`. A bounded result
   says `NO CEX (bound 10)`. The terminology shift matters for sign-off.

4. **Construct proof certificates get stronger.** The `--emit-check-construct-proof-smt`
   path (arbiter round-robin, FIFO overflow, credit_channel occupancy) can
   emit a k-induction witness alongside the BMC trace — a stronger artifact
   for formal sign-off.

---

## Rationale for Novelty

- Not tracked in any open issue (arch-com or harc-com).
- Not mentioned in `doc/plan_hierarchical_formal.md` (which focuses on
  hierarchy, not proof strength).
- The COMPILER_STATUS.md explicitly defers "unbounded liveness (`s_eventually`,
  strong/weak operators)" — but that is liveness, not safety. k-Induction is
  a safety technique and is orthogonal to the deferred liveness work.
- The current `Proved` label is technically misleading; fixing the semantics
  is independently motivated.

---

## Rough Effort Estimate

| Step | Effort |
|------|--------|
| CLI flag parsing (`--induction`, `--k`) | 1–2 h |
| `emit_inductive_step()` in FormalCtx | 1 day |
| Two-phase `run_property()` | half day |
| `PropertyStatus` renaming + output rendering | 2–3 h |
| Integration tests (counter proves, FIFO proves, regression) | 1 day |
| **Total** | **~3 days** |

The implementation is self-contained to `src/formal.rs` and `src/main.rs`.
No parser, type-checker, or codegen changes required.
