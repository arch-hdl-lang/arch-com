# Static Lock-Order / Deadlock Detection for `resource`/`lock` Threads

**Date:** 2026-06-13
**Status:** Proposal — not started
**Related issues:** #501 (thread surface gaps — "no static lock-order/deadlock warning")
**Blocks/unblocks:** Complements #383 (arch formal + resource/lock threads); independent of it

---

## Problem

ARCH's `resource`/`lock` construct (shipped v0.46.0+) lets threads acquire and release named
mutexes. When two threads acquire the same two resources in opposite order, the design
contains a potential deadlock: each thread can reach a state where it holds one resource and
is waiting forever for the other. In synthesized hardware this manifests as a silent stall —
the lowered FSM simply stops advancing, with no runtime error, no warning, and no `$display`.

Today `arch check` catches structural type errors but says nothing about lock ordering. The
only way to discover a deadlock is to simulate long enough to hit it (unreliable — depends on
stimulus) or run `arch formal` (blocked for `resource`/`lock` designs by #383). Neither is
available as a fast, zero-setup pre-check at the command line.

**Concrete example of the bug this would catch:**

```arch
module DualPort
  port clk: in Clock<SysDomain>
  port rst: in Reset<Sync>

  resource bus_mutex: mutex<round_robin>;
  resource mem_mutex: mutex<priority>;

  // Thread A: acquires bus_mutex, then mem_mutex
  thread issue_unit on clk rising, rst high
    wait until start_a;
    lock bus_mutex
      lock mem_mutex        // <-- holds bus, wants mem
        reg data_a: UInt<32> reset rst => 0;
        data_a <= fetch();
      end lock mem_mutex
    end lock bus_mutex
  end thread issue_unit

  // Thread B: acquires mem_mutex, then bus_mutex — DEADLOCK RISK
  thread writeback on clk rising, rst high
    wait until start_b;
    lock mem_mutex
      lock bus_mutex        // <-- holds mem, wants bus
        reg data_b: UInt<32> reset rst => 0;
        data_b <= commit();
      end lock bus_mutex
    end lock mem_mutex
  end thread writeback

end module DualPort
```

Running `arch check DualPort.arch` today: no warnings. After this change:

```
DualPort.arch:16:7: WARNING[DEADLOCK-RISK]: potential deadlock between threads 'issue_unit' and 'writeback'
  issue_unit:  holds 'bus_mutex' (line 13), then waits for 'mem_mutex' (line 14)
  writeback:   holds 'mem_mutex' (line 24), then waits for 'bus_mutex' (line 25)
  Fix: acquire both mutexes in the same order in every thread, or restructure to avoid nested locks.
```

---

## Why This Matters

1. **Silent failure mode.** A deadlocked synthesized FSM looks identical to a stalled design
   waiting for valid input. Post-silicon diagnosis is very hard; pre-silicon detection is free.

2. **ARCH's LLM-generation use case amplifies the risk.** LLMs generating `thread`/`lock`
   code from natural language have no inherent notion of lock ordering. A "write the issue
   and writeback threads sharing a bus and a memory" prompt will plausibly produce the
   deadlock pattern above. A static check closes this gap without requiring any LLM change.

3. **Faster than simulation or formal.** The analysis is an O(T × R²) graph walk over the
   thread bodies at elaboration time — a millisecond check that runs inside `arch check`.
   Simulation might miss it under typical stimuli; `arch formal` is currently blocked for
   `resource`/`lock` designs (#383) and is slower anyway.

4. **Builds on shipped infrastructure.** `resource`/`lock` landed in v0.46.0. The thread
   body AST already carries the `lock`/`unlock` structure. No new language surface is needed.

---

## Proposed CLI Surface

The check runs automatically as part of `arch check` and `arch build`. No new flag required.
A `pragma allow_lock_order_inversion;` annotation in a module body suppresses the warning for
that module (parallel to the existing `pragma allow_dead_skid_feedback;` pattern) for cases
where the designer has externally guaranteed ordering (e.g., via a higher-level protocol that
prevents two threads from running simultaneously).

```
arch check DualPort.arch
DualPort.arch:16:7: WARNING[DEADLOCK-RISK]: ...
```

Exit code: 0 (warning, not error) in v1. Can be promoted to error via a future
`--deadlock-error` flag or added to a lint-level config.

---

## Algorithm

### Step 1 — Build per-thread lock acquisition sequences

Walk each `thread` body's AST before lowering, collecting lock events in execution order.
The result for each thread is a sequence of `(hold_set, acquire)` pairs: "while holding this
set of resources, the thread attempts to acquire this resource."

Handle control flow conservatively:
- Sequential statements: concatenate.
- `if`/`elsif`/`else`: collect pairs from all branches (worst case = union).
- `fork`/`join`: collect pairs from all branches; branches execute concurrently so each branch
  sees the hold-set from the enclosing scope.
- `for` loops: treat as one pass (the hold-set does not grow across iterations for well-formed
  nested locks; if it does, flag separately).
- Nested `lock`: the inner acquire's hold-set includes all outer locks.

This produces a list of `LockEdge { holder: ResourceName, waiter: ResourceName, thread: ThreadName, site: Span }`.

### Step 2 — Build the resource wait-for graph

Nodes = resources. Directed edge R1 → R2 means "some thread holds R1 and waits for R2."

Multiple threads can create the same edge (idempotent).

### Step 3 — Detect cycles

Run a DFS cycle-detection on the wait-for graph. A cycle `R1 → R2 → ... → R1` means threads
holding those resources in a cycle cannot all make progress simultaneously.

### Step 4 — Emit diagnostics

For each cycle, report the participating threads and the specific `lock` sites that form the
cycle, with a suggested fix (establish a global lock ordering by name, e.g., alphabetical, or
restructure to avoid nested locks).

### Scope v1

- Statically visible `lock NAME ... end lock NAME` nesting — covers the common case.
- Intra-module only. Cross-module lock sharing (via `resource` passed through ports) deferred.
- `fork`/`join` branches treated conservatively (any branch can hold any lock in the fork).
- `default_when` threads: warn that deadlock analysis is skipped (soft-reset can break cycles,
  but not guaranteed; flag for manual review).
- `shared(or)`/`shared(and)` threads: out of scope (they don't use `lock`).

---

## Implementation Sketch

### Where in the compiler

New pass `src/lock_order.rs`, invoked from `src/typecheck.rs` after the thread map is built
(post-parse, pre-lowering). This mirrors the existing `src/signal_flow.rs` (dead-skid hazard
check), which also runs pre-lowering on the thread AST.

### Key data structures

```rust
struct LockEdge {
    thread: String,
    hold_set: BTreeSet<String>,  // resources held at the point of acquisition
    waiter: String,              // resource being acquired
    site: Span,
}

struct LockOrderGraph {
    edges: Vec<LockEdge>,        // one per statically visible acquisition pair
}
```

### Traversal

Recursive `collect_lock_edges(stmt: &Stmt, hold_set: &BTreeSet<String>) -> Vec<LockEdge>`:
- `Stmt::Lock { name, body }`: emit edge for each existing member of `hold_set` (hold → name),
  then recurse into `body` with `hold_set ∪ {name}`.
- `Stmt::If { branches }`: union of edges from all branches at same hold_set.
- `Stmt::Fork { branches }`: same as If.
- Other statements: recurse, passthrough hold_set.

### Cycle detection

Standard Tarjan's SCC or simple DFS with coloring on the resource graph (not thread graph),
O(V + E) where V = number of distinct resources, E = number of distinct (hold, wait) pairs.

---

## What It Does Not Catch

- **Livelock** (threads keep running but make no overall progress) — not expressible statically.
- **Priority inversion** (lower-priority thread holds a resource needed by a higher-priority
  thread, stalling it indefinitely) — a separate class of problem; the `policy:` on `resource`
  affects which thread gets the lock, not the order of acquisition.
- **Dynamic lock ordering** (order depends on runtime values) — the conservative approximation
  above may produce false positives for designs that are dynamically safe; the pragma suppresses.
- **Cross-module resource sharing** — deferred to v2.

---

## Alternatives Considered

**A. Require global lock-ordering annotation on `resource` declarations.**
E.g., `resource bus_mutex: mutex<round_robin> order 0;` and the compiler rejects any thread
that acquires order-N while holding order-M where M > N. Simpler to check; forces the designer
to specify ordering upfront. Downside: requires new language syntax and changes every existing
`resource` declaration. The graph analysis approach is zero-syntax and catches the same cycles.

**B. Rely on `arch formal` when #383 is fixed.**
Formal will prove or refute deadlock-freedom more rigorously. But it's bounded, slower, and
requires the design to be fully connected. The static check is a fast pre-screen that complements
formal rather than replacing it.

**C. Emit a runtime `ARCH-ERROR` in sim when a deadlock is detected.**
`arch sim --thread-sim parallel` could detect a cycle in the wait-for graph at runtime (all
threads waiting, none runnable). This is valuable and should be added independently (one line
in the scheduler's "no progress" path). But it requires simulation to trigger; the static
check is faster and catches it without running any testbench.

---

## Open Questions

1. Should the check be a warning or an error by default? Warning is safer for existing designs
   that might rely on higher-level ordering guarantees. Could add a future `--Werror=deadlock`
   flag.

2. For `fork`/`join` with `lock` inside branches: the conservative over-approximation may fire
   on designs where only one branch ever runs at a time (e.g., guarded by the same condition
   that prevents both threads from being live). Accept the false positive for v1; the pragma
   suppresses.

3. `default_when` threads can escape from any state on their guard condition, which can break
   a deadlock cycle dynamically. Should the analysis skip these threads entirely, or warn that
   the check was skipped? Recommend: emit a lower-severity note rather than a full warning,
   since `default_when` is a rare escape hatch.
