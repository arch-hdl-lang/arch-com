# Enhancement: module-level signal flow graph for `arch check`

**Date:** 2026-05-28
**Status:** Proposal — needs team discussion before implementation
**Related issues:** #375 (multi-driver), #245 (dead-skid comb-feedback lint),
  #306 (thread wait-exit timing), #368 (coverage suppression), #383 (formal wire-to-let)

---

## Problem

Three open issues stem from the same root gap: `arch check` has no unified model
of *who drives what, and who reads what* across a module's constructs.

| Issue | Symptom | Why the current checker misses it |
|-------|---------|----------------------------------|
| **#375** | Two `seq` blocks / two `thread` bodies / two `inst` outputs driving the same signal — passes silently | No per-signal write-set is maintained |
| **#245** | Thread reads a comb function of its own outputs; silent wrong values during dead-skid cycles | No transitive comb-reachability from thread output set |
| **#306** | `wait until cond; X <= Y;` fires `X` one cycle late | Lowering can't safely fold the assignment into the wait-exit arm without knowing whether the thread is the sole writer to `X` |

Each could be fixed with a dedicated ad-hoc check. But they share a common
need: a directed dataflow graph where nodes are signals and edges carry
context (which `comb`/`seq`/`thread`/`inst` block produced this drive). Building
that graph once, as a reusable compiler pass, makes all three tractable
simultaneously and avoids tripling the maintenance surface as new constructs are
added (e.g. `rule` from issue #379).

---

## Proposed signal flow graph (SFG)

```rust
/// One directed edge: a named signal drives another, in a specific block context.
pub struct DriveEdge {
    pub source: SignalId,
    pub target: SignalId,
    pub context: DriveContext,
    pub span:    Span,
}

pub enum DriveContext {
    CombBlock(usize),                              // index into module.body
    SeqBlock(usize),
    ThreadState { thread_idx: usize, state: u32 },
    InstConn { inst: String, port: String },       // child-module output drives a wire
    LetBinding,
    RegDefault,                                    // reset value
}

pub struct SignalFlowGraph {
    pub signals: IndexMap<SignalId, SignalInfo>,
    pub drives:  Vec<DriveEdge>,
}
```

### Build pass

One traversal of `module.body` after elaboration:

- **`CombBlock(b)`** — for each `lhs = rhs` statement: add `DriveEdge { CombBlock(i) }` from each
  signal mentioned in `rhs` to `lhs`; also record a reverse "uses" edge (needed for check 2).
- **`SeqBlock(b)`** — same, with `SeqBlock(i)` context.
- **Thread-lowered states** — walk each synthesized FSM state; emit `ThreadState` edges.
- **`InstConn`** — for each `.port(wire)`: input connections add `wire → port`; output connections add
  `port → wire`.

This mirrors what Verilator does when it builds its elaborated netlist, but at the ARCH IR level.

---

## Checks enabled

### Check 1 — Multi-driver detection (fixes #375)

For each signal, collect all `DriveEdge` records targeting it. If there are 2+
edges from **distinct source identities** (different `CombBlock` indices, different
`SeqBlock` indices, different `ThreadState.thread_idx`, different `InstConn.inst`
names), report a multi-driver error with all contributing source spans.

Exception: multiple edges from the *same* block (e.g. a `comb` block with a
`default` and an `if`-override) are a single logical driver — no error.

This directly covers the repro patterns in issue #375:

- **C1–C2** Two `comb` blocks / two statements in one `comb` block driving the same wire → caught
- **C3** Two unconditional `<=` to one `reg` in the same `seq` block → caught
- **C7** Two `inst` outputs connected to the same `wire` → caught
- **C-seq** Two separate `seq on clk rising` blocks writing the same reg → caught
- **C-thread** Two `tlm_method` thread bodies writing the same reg → caught

### Check 2 — Dead-skid comb-feedback lint (enables #245)

For each thread `t`:

1. `writes_t` = all signals with a `DriveEdge` from any `ThreadState { thread_idx: t.idx }`.
2. **Comb-only reachability**: BFS through `CombBlock` edges only, starting from `writes_t`.
3. `reads_t` = all signals referenced in read position within thread states.
4. If `comb_reachable(writes_t) ∩ reads_t ≠ ∅`, emit a warning:

   ```
   warning: thread `T` reads `X`, which is a combinational function of `Y`
            that `T` itself drives. During dead-skid cycles `T`'s outputs
            fall to their default value; `X` may see a stale result.
   
   note: drive path: T writes Y → comb block → ... → X
   note: workaround: read the upstream input directly instead of the
         routed combinational output (see issue #245 for examples)
   ```

This surface is exactly the "most expensive single class of bug" in the
arch-ibex Phase A work documented in #245.

### Check 3 — Thread sole-writer query (unlocks exit-fold for #306)

The fix for issue #306 (fold `wait until cond; X <= Y;` into a single
`always_ff` arm rather than an extra state) is **safe only when the thread is
the sole writer to `X`**. With the SFG, that becomes a simple query:

```
is_sole_writer(thread_idx, signal) =
    drives.filter(|e| e.target == signal).all(|e| matches!(e.context, ThreadState { thread_idx }))
```

If true, the lowering pass can fold the assignment into the wait-exit guard.
This turns an "unclear if it's safe to change semantics" problem into a
mechanically checkable condition.

### Future checks the SFG enables

| Check | Uses | Benefit |
|-------|------|---------|
| Wire-to-let promotion for formal (#383) | Are all `DriveEdge`s into this `wire` from `CombBlock`? | Promotes wire to `let`, unblocking hierarchical formal v1 for lock-based designs |
| Exhaustive-match coverage suppression (#368) | Is the `match` arm set a partition of the scrutinee type domain? | Emit `/* verilator coverage_off */` on compiler-generated default only |
| Future UNOPTFLAT guidance | Detect comb cycles the designer intends (e.g. ring buffers) vs accidental ones | Suggest `/* verilator lint_off UNOPTFLAT */` at the right scope |

---

## Implementation plan

Two PRs, in order:

### PR A — SFG builder + multi-driver check (closes #375)

| Sub-task | File(s) | Estimate |
|----------|---------|----------|
| Define `SignalId`, `SignalInfo`, `DriveEdge`, `DriveContext`, `SignalFlowGraph` | new `src/signal_flow.rs` | ~120 LoC |
| Build pass: `CombBlock`, `SeqBlock`, `LetBinding`, `RegDefault` | `src/signal_flow.rs` | ~250 LoC |
| Build pass: `InstConn` (output ports drive parent wires) | `src/signal_flow.rs` | ~100 LoC |
| Build pass: thread-lowered states | `src/signal_flow.rs` | ~150 LoC |
| Call `build_signal_flow_graph` from `arch check` | `src/typecheck.rs` | ~20 LoC |
| Multi-driver check + error reporting | `src/typecheck.rs` | ~60 LoC |
| Tests: 7 repros from #375 (C1–C7, C-seq, C-thread) | `tests/` | ~7 test cases |
| **Total** | | **~700 LoC** |

### PR B — Comb-feedback lint (enables #245)

| Sub-task | File(s) | Estimate |
|----------|---------|----------|
| BFS comb-only reachability from thread write-set | `src/signal_flow.rs` | ~80 LoC |
| Thread read-set collection | `src/signal_flow.rs` | ~50 LoC |
| Intersection check + warning emission with path | `src/typecheck.rs` | ~80 LoC |
| Tests: 2–3 repros from #245 + one arch-ibex-style example | `tests/` | ~3 test cases |
| **Total** | | **~210 LoC** |

The sole-writer query (Check 3) feeds the thread lowering fix for #306 and is a
follow-up to PR A — no additional graph infrastructure needed.

---

## What this does not do

- Does not change language semantics or the emitted SV.
- Does not replace Verilator's net elaboration.
- Does not require `arch build` to run — lint only.
- Does not track dataflow *inside* expressions (signal-level granularity only).
  Bit-level tracking is a separate, much larger effort.
- Check 3 enables, but does not implement, the actual timing fix for #306. That
  fix is a separate PR to the thread lowering pass.

---

## Rationale: unified graph vs. three ad-hoc passes

Adding three independent ad-hoc passes would work, but:

1. **Maintenance surface multiplies.** A new construct (`rule`, `tlm_method :
   atomic`, pipeline blocks) needs to be handled in each pass separately. With a
   shared SFG, only the build pass grows.
2. **Incomplete coverage by construction.** Each ad-hoc check sees only the
   constructs its author thought to enumerate. The SFG build pass iterates all
   `ModuleBodyItem` variants exhaustively — a missing variant is a compile-time
   pattern-match error in Rust, not a silent false-negative.
3. **Reuse.** The formal wire-to-let promotion (#383) and coverage suppression
   (#368) both want answers to "what drives this signal and how?" — the same
   graph query, already built.

The cost is ~700 LoC up front for the builder. The payoff is that #375, #245,
and the prerequisite for #306 all land in two focused PRs instead of three
independent feature slabs.
