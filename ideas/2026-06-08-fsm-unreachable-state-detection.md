# FSM Unreachable-State Detection at `arch check` Time

> **Status**: proposal — 2026-06-08
> **Area**: type checker / static analysis
> **Effort**: low (~150–250 LOC, no new syntax, no new IR)

---

## Problem

The ARCH compiler currently enforces two structural FSM correctness rules:

1. **Dead-end states** — a state with no outgoing `->` transitions is a **compile error**
   (`"state 'S' has no outgoing transitions"`).
2. **Runtime coverage** — `arch build` auto-emits `cover state_r == S` for every
   state; if a state is never entered during simulation it shows up as `NOT REACHED`.

There is a gap between these two: a state can have outgoing *and* incoming transitions
declared in the ARCH source but still be **structurally unreachable from `reset_state`**
— meaning no sequence of transitions from the FSM's initial state can ever reach it.
The compiler accepts the design silently. The only signal is a `NOT REACHED` cover
result after running a simulation, which is slow feedback and easy to overlook.

### Concrete failure scenario

```arch
fsm DmaCtrl
  reset_state IDLE
  state IDLE
    -> FETCH when start;
  end state IDLE

  state FETCH
    -> WRITEBACK when done;
    -> IDLE when error;
  end state FETCH

  state WRITEBACK              // spelled correctly in the declaration
    -> IDLE when true;
  end state WRITEBACK

  state DRAIN                  // intended as an error-drain path —
    -> IDLE when drained;      // but no transition points here from
  end state DRAIN              // FETCH or anywhere reachable from IDLE
end fsm DmaCtrl
```

`DRAIN` passes the dead-end check (it has an outgoing `-> IDLE`). It also does
not cause a CDC or width error. It compiles cleanly. In synthesis it consumes
an encoding slot; in simulation it is never entered; the auto-emitted cover
property fires `NOT REACHED` — but only after the designer runs a simulation
with full coverage enabled.

A more dangerous variant: the designer *intended* `FETCH -> DRAIN when error`
but wrote `FETCH -> IDLE when error` by accident, silently dropping the
drain-on-error path. `DRAIN` becoming unreachable is the compile-time symptom
of that logic bug.

**LLM relevance**: ARCH is designed for LLM generation. Misspelled transition
targets and accidentally omitted transitions are exactly the class of bug LLMs
produce — the generated state machine is syntactically correct and type-safe but
functionally incomplete.

---

## Proposed Enhancement

Add **static unreachable-state detection** to `arch check` (and transitively to
`arch build` / `arch sim` / `arch formal`), emitting a **warning** for every FSM
state that cannot be reached by any path of declared transitions starting from
`reset_state`.

```
warning[W-FSM-001]: FSM 'DmaCtrl': state 'DRAIN' is unreachable from reset state 'IDLE'
  --> drain_dma.arch:18:9
   |
18 |   state DRAIN
   |         ^^^^^ declared here but no transition reaches it
   |
   = note: all transitions into this state are missing or point to unreachable states
   = help: add a transition `-> DRAIN when <cond>;` inside a reachable state, or remove 'DRAIN'
```

### Distinction from existing checks

| Check | What it catches | When |
|---|---|---|
| Dead-end state (existing) | State with **no outgoing** transitions | Compile error |
| **Unreachable state (proposed)** | State with **no incoming path from `reset_state`** | **Compile warning** |
| Auto-cover `cover state_r == S` (existing) | State never entered at runtime | NOT REACHED after sim |
| `arch formal` (existing) | Solver-proven reachability / cover | Bounded proof run |

The three approaches are orthogonal and complementary. Static analysis is
immediate and zero-cost; runtime covers need simulation traffic to all corners;
formal proves correctness under all inputs.

---

## Implementation Approach

### 1. Graph construction (in `typecheck.rs` or new `fsm_analysis.rs`)

After the existing FSM body parsing and dead-end check, build a directed graph:

```
nodes  = set of all declared state names
edges  = { (src, dst) | `-> dst` appears in src's body }
```

`when` guard expressions are **ignored** — this is a structural reachability
check, not a semantic one. A state is considered reachable if *any* declared
transition points to it (regardless of whether the guard can actually be
satisfied). This is conservative: it never false-positives on reachable states,
and it catches the common case where the designer forgot to wire the transition
at all.

### 2. BFS / DFS from `reset_state`

```rust
let mut visited = HashSet::new();
let mut queue = VecDeque::from([fsm.reset_state.clone()]);
while let Some(s) = queue.pop_front() {
    if visited.insert(s.clone()) {
        for dst in outgoing_transitions(&s) {
            queue.push_back(dst);
        }
    }
}
```

### 3. Emit warnings for unvisited nodes

For each declared state not in `visited`, emit `W-FSM-001` warning pointing at
the state declaration site in the source. The message includes:

- State name and FSM name.
- Which reset state is the root.
- Hint to either add an incoming transition from a reachable state or remove the dead state.

### 4. Suppression mechanism

Two suppression paths:

```arch
// Per-state: mark as intentionally unreachable (e.g. future placeholder)
pragma allow_unreachable_state DRAIN;
```

```
arch check --no-warn-unreachable-states Foo.arch
```

`pragma allow_unreachable_state S;` goes in the FSM body (same position as
existing pragmas). An unknown pragma is already a parse error, so this is
safe to introduce without risking silent suppression of other warnings.

### 5. `--strict` promotion to error

```
arch check --strict Foo.arch   // promotes W-FSM-001 to E-FSM-001
```

Consistent with the planned `--strict-naming` flag for naming convention
enforcement. CI pipelines can opt in.

### 6. Interaction with `generate for/if`

FSM states defined inside `generate for/if` blocks are already expanded by
the elaboration pass before type checking runs. The reachability graph is built
on the post-expansion state list, so generated states are handled uniformly.

---

## What This Does NOT Detect

- **Semantic unreachability**: a state that *has* an incoming transition but
  whose guard is a tautological false (`when 1'b0`). Detecting this requires
  symbolic analysis. Not in scope — the auto-emitted cover property handles it.
- **Liveness / deadlock**: a reachable state the FSM can enter but never exit
  toward a productive next state. Orthogonal; deadlock detection is a planned
  thread feature.
- **Thread-lowered FSMs**: the `thread` construct lowers to an FSM sub-instance
  at codegen time; the reachability check applies to the source-level `fsm`
  construct only. Thread safety is handled separately.

---

## Estimated Effort

| Phase | LOC | Notes |
|---|---|---|
| Graph construction + BFS | ~80 | Pure Rust, no new AST nodes |
| Warning emission + source span | ~50 | Reuse existing `Diagnostic` infrastructure |
| Pragma parsing + `--no-warn` flag | ~40 | Extend existing pragma table |
| Tests | ~80 | 4–6 integration test cases (unreachable, reachable, generate-expanded, pragma-suppressed) |
| **Total** | **~250** | Single focused session |

---

## Rationale Summary

- **Zero new syntax** — no language additions, just a new compiler warning.
- **Zero false positives on well-formed designs** — the analysis is sound
  (structural over-approximation: if any declared transition points to a state,
  it is considered reachable).
- **Immediate feedback** — fires at `arch check` with a source span, not
  after a simulation run.
- **High signal for LLM-generated code** — misspelled transition targets and
  accidentally omitted arcs are exactly the class of error LLMs make; this
  surfaces them at the earliest possible point.
- **Completes the correctness triangle** — dead-end check (no outgoing) +
  unreachable check (no incoming path) + auto-cover (runtime/formal) cover
  the three distinct structural FSM failure modes.
