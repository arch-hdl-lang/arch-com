# FSM Unreachable-State Warning in `arch check`

**Status**: Proposal — not yet designed or implemented.
**Date**: 2026-06-18
**Area**: `arch check`, FSM type-checker

---

## Problem

`arch check` currently catches two classes of broken FSM states:

| Class | Detection | Mechanism |
|---|---|---|
| Dead-end state (no outgoing transitions) | ✅ compile error | FSM type-checker, parser |
| Unreachable state (never entered from `reset_state`) | ❌ **not checked** | — |

A state is *unreachable* if no sequence of declared transitions leads to it from
`reset_state`. It has outgoing transitions and parses cleanly, so the compiler
today emits no diagnostic. The auto-emitted `cover state_r == S` SVA properties
surface the same issue at formal/simulation time, but those require running EBMC
or Verilator `--assert` — `arch check` is a sub-second no-tool scan.

### When this bug appears

The pattern recurs when:
1. A developer adds a new state for a refactored protocol variant but wires only
   its outgoing edge, forgetting the incoming path.
2. A `generate_if PARAM > 0 ... end generate_if` encloses the only transition
   into a state; when the param is 0 the state exists in the encoding but no
   transition leads to it.
3. LLM-generated code emits a guard state for a not-yet-implemented feature but
   leaves it disconnected.

In all three cases the state wastes encoding bits, confuses synthesis, and is
invisible to `arch check` today.

### Relation to existing checks

The `cover state_r == S` auto-properties that `arch build` emits (see
COMPILER_STATUS.md, `assert/cover` row) provide *runtime/formal* reachability
evidence.  They are not a substitute for compile-time diagnosis:

- Running EBMC with enough `--bound` cycles adds minutes; `arch check` takes
  milliseconds.
- An unreachable state may not show up in a bounded model check unless the bound
  is large enough to reach it — or provably can't.
- A developer doing a quick `arch check` before committing wants the error now,
  not after a formal run.

---

## Proposed change

### Behaviour

After FSM transitions are validated by the existing type-checker, add a
reachability pass:

1. Build a directed graph: nodes = declared states, edges = `from_state ->
   to_state` for every `-> TargetState [when <expr>];` declaration. Conditional
   transitions (`when expr`) are included as edges — the transition exists; it
   may or may not fire at runtime, but the state is *structurally* reachable.
2. DFS/BFS from `reset_state`.
3. Any state not in the reachable set emits:

```
warning[W0031]: FSM state `Draining` in `TxQueue` is unreachable from reset state `Idle`
  --> fifo_ctrl.arch:47:3
   |
47 |   state Draining
   |   ^^^^^ declared here but no transition leads here from `Idle`
   |
   = help: add a `-> Draining when <condition>;` inside the state that should
           enter `Draining`, or remove the state if it is no longer needed
```

4. The check is a **warning** (not an error) to match the same posture as Rust's
   `dead_code` warning — unreachable states are likely bugs but not impossible to
   want (e.g., a placeholder behind `todo!`). A future `#[deny(unreachable_states)]`
   pragma can upgrade it.

### Scope

- `fsm` constructs only. Thread-lowered FSMs are compiler-internal (not user-declared
  states), so they are exempt.
- `generate_if`-gated transitions: after `generate_if` elaboration, transitions that
  survive into the resolved AST contribute edges. Transitions inside an unresolved
  (param-dependent) `generate_if` are included conservatively — the state is treated as
  reachable as long as *some* param value would make it reachable. A fully
  param-constant `generate_if` that resolves to false contributes no edge.
- Self-transitions (`-> Self when cond;`) don't add reachability to other states.
- The `reset_state` is trivially reachable by definition; no warning for it.

---

## Implementation sketch

The FSM type-checker lives in `src/typecheck/fsm.rs` (or equivalent). The
reachability pass slots in after the existing "every state must have at least one
transition" check:

```rust
// After validating transitions, compute reachable set.
let mut reachable: HashSet<&str> = HashSet::new();
let mut stack: Vec<&str> = vec![fsm.reset_state];
while let Some(s) = stack.pop() {
    if reachable.insert(s) {
        for edge in transition_targets_of(s, &fsm.transitions) {
            stack.push(edge);
        }
    }
}
for state in &fsm.states {
    if !reachable.contains(state.name.as_str()) {
        ctx.warn(W0031_UNREACHABLE_STATE, state.span,
            format!("`{}` is unreachable from reset state `{}`",
                    state.name, fsm.reset_state));
    }
}
```

Effort: ~1–2 days (graph construction, diagnostic, tests). No new AST nodes,
no SV emission changes, no CLI flags. The warning ID `W0031` (or whatever the
next available slot is) should be registerable in the existing warning registry.

---

## Example

```arch
fsm TxQueue
  port clk: in Clock;
  port rst: in Reset;
  port push: in Bool;
  port pop: in Bool;
  port full: out Bool;
  port empty: out Bool;

  reset_state Idle

  state Idle
    -> Filling when push;
    comb
      full = false; empty = true;
    end comb
  end state Idle

  state Filling
    -> Idle when (not push and pop);
    -> Full when push;
    comb
      full = false; empty = false;
    end comb
  end state Filling

  state Full
    -> Filling when pop;
    comb
      full = true; empty = false;
    end comb
  end state Full

  // Accidentally orphaned — developer meant to add `-> Draining` in Full
  // but only wrote the outgoing side.
  state Draining
    -> Idle when pop;
    comb
      full = false; empty = false;
    end comb
  end state Draining

end fsm TxQueue
```

Current `arch check`: passes silently.

After this change:
```
warning[W0031]: FSM state `Draining` in `TxQueue` is unreachable from reset state `Idle`
  --> tx_queue.arch:35:3
   |
35 |   state Draining
   |   ^^^^^ no declared transition leads here from `Idle`
```

---

## Why now

- **LLM-generated code** is ARCH's primary use case. LLMs regularly generate
  placeholder states with only outgoing transitions; the current silence is
  misleading.
- **Zero user-facing syntax changes** — this is a pure diagnostic addition.
- **Complements the existing check set**: dead-end → error (outgoing missing),
  unreachable → warning (incoming missing). Together they close both directions
  of the FSM structural-correctness surface.
- **No overlap** with any existing plan doc, roadmap row, or open issue. The
  closest existing work is the auto-emitted `cover state_r == S` SVA, which
  addresses the formal-time manifestation of the same bug class, not the
  compile-time one.

---

## Future extensions

- **`#[deny(unreachable_fsm_states)]` pragma** inside a `module` or at the file
  level to promote warnings to errors in disciplined codebases.
- **`--unreachable-states=error|warn|allow` CLI flag** on `arch check` /
  `arch build` for project-wide policy enforcement.
- **`arch advise` integration**: when the warning fires, retrieve past
  error→fix pairs from `~/.arch/learn/` that involved unreachable states
  and print the relevant fix hint alongside the diagnostic.
- **Thread-lowered FSM coverage**: after `lower_threads` runs, a secondary
  pass could apply the same analysis to compiler-generated thread state
  machines. This is lower value (compiler-generated structure is expected to
  be well-formed) but could catch lowering bugs.
