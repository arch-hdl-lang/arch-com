# FSM Unreachable-State Detection at Compile Time

**Date:** 2026-06-16
**Status:** Proposal
**Effort:** Low (single BFS pass in `check_fsm`, ~60 lines of Rust)

---

## Problem

The ARCH spec (§27.2, `doc/ARCH_HDL_Specification.md` line 1572) explicitly promises:

> *"Missing transitions, undriven outputs in any state, and **unreachable states** are all compile-time errors."*

The compiler currently enforces two of the three:
- Missing outgoing transitions → `"state X has no transitions (dead-end state)"` — ✅ error in `typecheck.rs:5019–5023`
- Undriven outputs in a state → partially enforced via the default-block / coverage rules

The third guarantee — **unreachable states (no path from `default state` to the state)** — is **not implemented**. The compiler silently accepts FSMs with states that can never be entered at runtime.

### Concrete failure mode

```arch
fsm TrafficLight
  port clk: in Clock;
  port rst: in Reset;
  port timer_done: in Bool;
  port error: in Bool;
  port out: out UInt<2>;

  default state Red;

  state Red
    comb out = 2'd0; end comb
    -> Green when not error;
    -> Red when error;
  end state Red

  state Green
    comb out = 2'd1; end comb
    -> Yellow when timer_done;
  end state Green

  state Yellow
    comb out = 2'd2; end comb
    -> Red when true;
  end state Yellow

  // Designer added Emergency for future use but forgot the transition TO it:
  state Emergency
    comb out = 2'd3; end comb
    -> Red when true;
  end state Emergency
end fsm TrafficLight
```

`arch check` currently accepts this silently. `Emergency` wastes one encoding bit (auto-width grows from 2 to 2 bits with 4 states vs 3) and, worse, obscures the spec bug: the designer almost certainly intended to reach `Emergency` from `Green` on an error signal, but forgot the `-> Emergency when error;` transition.

The auto-emitted `cover state_r == EMERGENCY` SVA does eventually catch this at formal or coverage-driven sim — but only if the user runs those flows. A compile-time check provides instant feedback.

---

## Why It Matters

1. **Spec promise vs. compiler reality.** The spec is unambiguous. Users reading the spec trust that unreachable states are caught early; the compiler breaks that trust.

2. **Common specification bug.** Unreachable states arise regularly during incremental FSM design: a designer adds a new state but forgets to add the arc to reach it, or renames a state and leaves a dangling target in a conditional. The static graph check catches both.

3. **Encoding waste.** ARCH auto-selects minimum-width enum encoding (`⌈log₂(N)⌉`). An unreachable state inflates N, potentially adding a register bit for nothing.

4. **Faster feedback than SVA/coverage.** The auto-emitted `cover state_r == S` property already exists, but it requires running `arch formal` or `arch sim --coverage` to surface the miss. A compile-time error is zero-cost and immediate.

5. **Symmetric with dead-end detection.** Dead-end states (no *outgoing* transitions) are errors. Unreachable states (no *incoming* path from reset) are the symmetric problem; it is inconsistent to catch one and ignore the other.

---

## Proposed Implementation

**Location:** `src/typecheck.rs`, inside `check_fsm()`, immediately after the existing dead-end check at line 5019.

**Algorithm:** Forward BFS from `default_state` over the transition target graph (static; all transitions considered regardless of condition, since we are checking structural reachability, not conditional reachability).

```rust
// After dead-end check — collect reachable states via BFS from default_state.
if let Some(default_st) = &f.default_state {
    use std::collections::{HashMap, HashSet, VecDeque};

    // Build name→StateBody map.
    let state_body_map: HashMap<&str, &StateBody> = f
        .states
        .iter()
        .map(|sb| (sb.name.name.as_str(), sb))
        .collect();

    let mut reachable: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    let start = default_st.name.clone();
    reachable.insert(start.clone());
    queue.push_back(start);

    while let Some(cur) = queue.pop_front() {
        if let Some(sb) = state_body_map.get(cur.as_str()) {
            for tr in &sb.transitions {
                let t = tr.target.name.clone();
                // "Self" is a self-loop — already in reachable.
                if t != cur && reachable.insert(t.clone()) {
                    queue.push_back(t);
                }
            }
        }
    }

    // Emit an error for every declared state not reachable from reset.
    for sb in &f.states {
        if !reachable.contains(&sb.name.name) {
            self.errors.push(CompileError::general(
                &format!(
                    "FSM state `{}` is unreachable from the reset state `{}`",
                    sb.name.name, default_st.name,
                ),
                sb.name.span,
            ));
        }
    }
}
```

**Complexity:** O(V + E) where V = number of states and E = total transition count — negligible.

**Error vs. warning:** The spec says *error*. Implementing as an error is consistent with the dead-end check. A `pragma allow_unreachable_states;` escape hatch can be added later if legitimate use cases arise (e.g., deliberately unreachable "dead code" trap states in formally-verified designs).

---

## Test Plan

1. **New test: unreachable state is rejected.**
   ```arch
   fsm Bad
     default state A;
     state A -> B when true; end state A
     state B -> A when true; end state B
     state C -> A when true; end state C  // unreachable
   end fsm Bad
   ```
   → `arch check` must emit:
   `"FSM state 'C' is unreachable from the reset state 'A'"`

2. **New test: all-reachable FSM still accepted.**
   A fully-connected FSM (all states reachable from reset) must still `arch check` clean.

3. **Existing FSM tests unchanged.** Re-run `cargo test --release --test integration_test` — all existing FSM tests should remain green. Any failures indicate a false positive in the new check.

4. **Self-loop edge case:** A state that only has `-> Self when true` is technically reachable if it can be reached from another state; test that the BFS handles this correctly (the state that points to Self is reachable, Self-loop doesn't add a new state).

5. **Mutual-cycle edge case:** Two states that are only reachable from each other but not from `default_state` should both be flagged.

---

## Design Trade-offs

| Choice | Rationale |
|--------|-----------|
| Error, not warning | Matches spec; symmetric with dead-end detection; warnings tend to be ignored |
| Static BFS (all transitions) | Tractable at compile time; condition-sensitive reachability requires symbolic execution |
| No `pragma` escape initially | Keep it simple; add only when a real use case is filed |
| Trigger: after dead-end check | Dead-end check first; if a state has no outgoing transitions, it can't contribute to reachability anyway |

---

## Relationship to Existing Features

- **`cover state_r == S` auto-SVA** (runtime/formal) — complementary, not replaced. Compile-time catches the bug instantly; SVA catches it after elaboration and provides a witness cycle.
- **Dead-end state check** — this proposal is the symmetric counterpart.
- **`--coverage` FSM state-entry counters** — also complementary; coverage reports which states were *entered*, but only after simulation.
- **`arch formal` BMC** — can prove a state is unreachable as well, but requires solver invocation and a bound.

---

## Reference

- Spec: `doc/ARCH_HDL_Specification.md` line 1572
- Existing dead-end check: `src/typecheck.rs:5014–5023`
- FSM AST: `src/ast.rs:1618` (`FsmDecl.default_state`, `FsmDecl.states`, `StateBody.transitions`, `Transition.target`)
- Auto-emitted cover SVA: `src/codegen/mod.rs` (FSM state reachability cover properties)
