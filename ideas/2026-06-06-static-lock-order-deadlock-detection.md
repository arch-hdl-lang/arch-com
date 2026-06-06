# Enhancement: Static lock-order deadlock detection for `resource`/`lock` threads

**Date:** 2026-06-06
**Status:** Proposal — ready for team discussion
**Spec reference:** §20.8.5 ("Deadlock prevention — the compiler SHOULD warn when two threads
acquire the same pair of resources in opposite order")
**Related:** #501 (thread surface gaps tracker — "Static lock-order / deadlock warning" item),
`doc/thread_spec_section.md` §20.8.5, `doc/thread_multi_outstanding_spec.md` §Open Q5

---

## Problem

ARCH's `resource`/`lock` construct enables mutual exclusion between threads.  When two (or
more) threads acquire the same set of resources but in a different order, the result is a
**classic dining-philosophers deadlock**: each thread holds one resource and waits for the
other.  In arch sim, this manifests as an infinite loop — the simulation hangs silently with
no error message and no visible progress.

Example:

```arch
module TwoChannelDma
  port clk: in Clock;
  port rst: in Reset;
  // ... DMA channel ports ...

  resource bus_a: mutex;
  resource bus_b: mutex;

  thread channel_0 on clk rising, rst high
    wait until ch0_req;
    lock bus_a           // ← acquires bus_a first
      lock bus_b         // ← then acquires bus_b while holding bus_a
        do
          bus_a_addr = ch0_addr;
          bus_b_addr = ch0_addr;
        until ch0_done;
      end lock bus_b
    end lock bus_a
  end thread channel_0

  thread channel_1 on clk rising, rst high
    wait until ch1_req;
    lock bus_b           // ← acquires bus_b first  ← OPPOSITE ORDER
      lock bus_a         // ← then acquires bus_a while holding bus_b
        do
          bus_a_addr = ch1_addr;
          bus_b_addr = ch1_addr;
        until ch1_done;
      end lock bus_a
    end lock bus_b
  end thread channel_1
end module TwoChannelDma
```

`arch check` and `arch build` currently pass this without any diagnostic.  `arch sim` hangs
when both channels request simultaneously: `channel_0` holds `bus_a` and waits for `bus_b`;
`channel_1` holds `bus_b` and waits for `bus_a`.  Neither can proceed.

The spec commits to detecting this at compile time (§20.8.5): *"If two threads lock resources
A and B in opposite order, the compiler should issue a warning."*  Issue #501 lists this as a
priority diagnostic gap.

---

## Why this is high-priority

Unlike most bugs, a deadlock does not produce a wrong answer — it produces **no answer**.
Symptoms:

- `arch sim` runs forever (or until a wall-clock timeout kills it).
- The `--debug` flag shows no port changes after the deadlock cycle.
- There is no assertion failure, no `ARCH-ERROR`, no waveform anomaly.

The debugging path is: add `printf` statements, identify the hung cycle, read the `_req`
and `_grant` wires in the waveform, realize the acquisition order, fix the source.  This
takes hours.  A compile-time warning is free by comparison.

Issue #501 triage table ranks this:

> | Gap | Severity | First action |
> |-----|----------|--------------|
> | Static lock-order / deadlock warning | **latent sim-hang risk** on user code | write the 2-thread A-then-B / B-then-A repro, add a graph analysis pass over lock blocks |

---

## Detection algorithm

The problem reduces to finding a **cycle in the lock-acquisition-order graph** (LAOG):

1. For each thread body, walk the `ThreadStmt` tree maintaining a **held-set** — the set of
   resource names currently held by the thread at each program point.

2. When a `ThreadStmt::Lock { resource: R, body, .. }` is encountered:
   - For each resource `H` already in the held-set, add a directed edge `H → R` to the LAOG
     (meaning: *"this thread acquires R while holding H"*).
   - Recursively walk `body` with `held_set ∪ {R}`.
   - After the recursion, the held-set reverts to its pre-lock state (lexical scope).

3. After processing all threads in the module, run **Tarjan's SCC algorithm** (or DFS
   reachability) on the LAOG.

4. Any SCC of size ≥ 2 (or any back-edge in DFS) indicates a cycle.  Each cycle is a
   potential deadlock.

5. Emit a warning — not an error — with:
   - The resource names in the cycle.
   - The threads that contribute the conflicting edges.
   - The source spans of the `lock` statements involved.

### Why a warning, not an error

Some designs intentionally acquire locks in a fixed priority order enforced by an outer
protocol (e.g. always lock the lower-indexed resource first).  The compiler cannot prove
liveness from static order alone — a designer may have an invariant that prevents both
threads from reaching the conflicting lock simultaneously.  A warning is the right level:
it flags the pattern, the designer confirms or suppresses.

Suppression: add `pragma allow_lock_order_cycle;` at module scope (mirrors
`pragma allow_dead_skid_feedback;` from the dead-skid lint).

---

## Implementation

The new analysis lives entirely in `src/elaborate.rs` (or a small helper in `src/typecheck.rs`)
and runs during `arch check` / `arch build` / `arch sim` after thread parsing but before
thread lowering.

### Step 1 — Build the LAOG

```rust
/// Directed edge in the lock-acquisition-order graph.
/// `holder` is acquired before `acquired`; both are resource names.
struct LockOrderEdge {
    holder:   String,
    acquired: String,
    thread:   String,    // thread name (for diagnostics)
    span:     Span,      // span of the inner `lock` statement
}

/// Walk `stmts` with `held` as the current held-resource set.
/// Appends all acquisition-order edges into `edges`.
fn collect_lock_order_edges(
    stmts:   &[ThreadStmt],
    held:    &[String],          // resources currently held (ordered by acquisition depth)
    edges:   &mut Vec<LockOrderEdge>,
    thread:  &str,
) {
    for stmt in stmts {
        match stmt {
            ThreadStmt::Lock { resource, body, span } => {
                // For each resource already held, record: held_resource → resource.
                for h in held {
                    edges.push(LockOrderEdge {
                        holder:   h.clone(),
                        acquired: resource.name.clone(),
                        thread:   thread.to_string(),
                        span:     *span,
                    });
                }
                let mut new_held = held.to_vec();
                new_held.push(resource.name.clone());
                collect_lock_order_edges(body, &new_held, edges, thread);
            }
            ThreadStmt::IfElse(ie) => {
                collect_lock_order_edges(&ie.then_stmts, held, edges, thread);
                collect_lock_order_edges(&ie.else_stmts, held, edges, thread);
            }
            ThreadStmt::ForkJoin(branches, _) => {
                for br in branches {
                    collect_lock_order_edges(br, held, edges, thread);
                }
            }
            ThreadStmt::For { body, .. }
            | ThreadStmt::DoUntil { body, .. } => {
                collect_lock_order_edges(body, held, edges, thread);
            }
            _ => {}
        }
    }
}
```

### Step 2 — Cycle detection

After collecting edges from all threads in a module, build an adjacency map and run DFS:

```rust
fn find_lock_order_cycles(
    edges: &[LockOrderEdge],
) -> Vec<Vec<&LockOrderEdge>> {
    // adjacency: resource_name → Vec<&LockOrderEdge>
    let mut adj: HashMap<&str, Vec<&LockOrderEdge>> = HashMap::new();
    for e in edges {
        adj.entry(&e.holder).or_default().push(e);
    }

    let mut visited:   HashSet<&str> = HashSet::new();
    let mut on_stack:  Vec<&str>     = Vec::new();
    let mut cycles:    Vec<Vec<&LockOrderEdge>> = Vec::new();

    for start in adj.keys() {
        if visited.contains(start) { continue; }
        dfs_find_cycles(start, &adj, &mut visited, &mut on_stack, edges, &mut cycles);
    }
    cycles
}
```

A complete DFS-based cycle finder is ~60 lines including path reconstruction.

### Step 3 — Emit warnings

```
warning: potential deadlock: lock acquisition order cycle detected

  thread `channel_0` acquires `bus_a` then `bus_b`:
    ╭─[DmaTop.arch:23:5]
 23 │     lock bus_a
    ·       ↓
 26 │       lock bus_b
    ╰────

  thread `channel_1` acquires `bus_b` then `bus_a` (opposite order):
    ╭─[DmaTop.arch:38:5]
 38 │     lock bus_b
    ·       ↓
 41 │       lock bus_a
    ╰────

  cycle: bus_a → bus_b → bus_a
  note: if this order is always prevented at runtime, add
        `pragma allow_lock_order_cycle;` to suppress this warning.
```

### Step 4 — Integration point

Call the check in `arch check`'s module-level validation pass, after all `ThreadBlock` items
are present but before `lower_threads` runs.  The check function signature is:

```rust
pub fn check_lock_order(
    module:   &ModuleDecl,
    warnings: &mut Vec<Diagnostic>,
)
```

This mirrors how `check_multi_driver` and `find_dead_skid_hazards` are called from the
shared check/build/sim pipeline.

---

## What the existing infrastructure already provides

| Component | File | Reuse |
|-----------|------|-------|
| `ThreadStmt::Lock { resource, body, span }` AST node | `src/ast.rs:707` | Walk pattern directly |
| `collect_locked_resources(stmts)` | `src/elaborate.rs:6587` | New function `collect_lock_order_edges` is a strict generalization |
| `Span` / diagnostic emission helpers | `src/diagnostics.rs` | Unchanged |
| `pragma` parsing | `src/parser.rs` | `allow_lock_order_cycle` needs one new pragma variant |

The `collect_locked_resources` function already recursively walks the `Lock/IfElse/ForkJoin/
For/DoUntil` tree, collecting resource names into a `HashSet<String>`.  The new
`collect_lock_order_edges` is the same walk, with the held-set threading through the
recursion.  Diff from the existing function: ~30 lines.

---

## Test cases

| Test | Expected |
|------|---------|
| Two threads: `lock A then lock B` / `lock B then lock A` | Warning with cycle `A → B → A` |
| Two threads: both `lock A then lock B` | No warning (same order) |
| Three-resource cycle: A→B, B→C, C→A across three threads | Warning with cycle A→B→C→A |
| Nested `fork`/`join`: branches acquire locks in opposite order | Warning (each branch analyzed independently against held-set at fork entry) |
| Single thread: `lock A then lock B` with no other threads touching these | No warning (no cross-thread conflict) |
| Module with `pragma allow_lock_order_cycle;` | Warning suppressed |
| Module where one lock is inside an `if/else` branch (path-conditional acquisition) | Warning emitted (conservative: compiler cannot prove the conflicting branches are mutually exclusive at runtime) |
| TLM-generated thread bodies | No warning (TLM inline threads are synthesized; they acquire locks through index-gated state — no cross-resource nesting) |

---

## Scope and what this does not do

- **Detects structural lock-order inversions only.** The compiler does not analyze
  runtime conditions or prove liveness — if two `if` branches are mutually exclusive at
  runtime, the warning will still fire.  The designer can suppress with the pragma.
- **Does not detect livelock.** A design where thread A always releases lock X before
  thread B requests it is safe, but the compiler does not reason about timing.  This is
  the right tradeoff: the false-positive rate for the order-inversion check is very low
  (most order inversions are bugs), while liveness analysis is undecidable in general.
- **Does not cover semaphore (N>1 count-based) resources.** Semaphore support is tracked
  separately in #501.  Once semaphore lowering is implemented, the order-inversion check
  should be extended to it — same algorithm, slightly different "held" semantics for
  counting resources.
- **Does not fire on TLM-generated lock uses.** The TLM indexed-target lowering generates
  `lock`-equivalent gating internally, but these are compiler-synthesized (already tracked
  via the `synthesized` flag proposal in `ideas/2026-05-30-sfg-synthesized-block-tagging.md`).
  Once synthesized-block tagging lands, these can be trivially excluded from the LAOG walk.

---

## Implementation estimate

| Task | File | Lines |
|------|------|-------|
| `collect_lock_order_edges` + `LockOrderEdge` struct | `src/elaborate.rs` | ~80 LoC |
| `find_lock_order_cycles` (DFS + path reconstruction) | `src/elaborate.rs` | ~70 LoC |
| `check_lock_order` (driver + warning emission) | `src/elaborate.rs` | ~50 LoC |
| Call site in check/build/sim pipeline | `src/typecheck.rs` or `src/main.rs` | ~15 LoC |
| `allow_lock_order_cycle` pragma variant + parser | `src/parser.rs`, `src/ast.rs` | ~20 LoC |
| Integration tests (7 cases above) | `tests/integration_test.rs` | ~7 test cases |
| **Total** | | **~235 LoC** |

---

## Relationship to issue #501

Issue #501 tracks the full thread surface gap inventory.  This document is the **standalone
implementation proposal** for the deadlock-warning item in that tracker.  The suggested triage
order in #501 ranks it as:

> write the 2-thread A-then-B / B-then-A repro, add a graph analysis pass over lock blocks

This proposal is that analysis pass, fully specified.  It can land independently of the other
items in #501 (semaphore, `shared(and)`, pipeline wait-stage sim) since it touches only
`elaborate.rs` + the check pipeline and produces no SV output change.

---

## Why this matters for LLM-generated designs

ARCH is explicitly designed for LLM code generation.  An LLM generating a multi-channel DMA,
NoC router, or cache controller with multiple shared resources will naturally produce lock
statements — and will frequently generate them in inconsistent orders across different
threads (the LLM has no global lock-ordering discipline).  Without this check, every such
generated design silently hangs on the first contention cycle.

The check is a low-cost backstop: it runs in O(T × R²) time (T = threads, R = resources
per module) and emits a precise, actionable diagnostic pointing to the two conflicting `lock`
statements.  The designer or the LLM can fix it in one edit (reorder the lock statements in
one thread to match the other).
