# Enhancement: `arch formal` construct desugar pass — unlock FSM, counter, and arbiter formal verification

**Date:** 2026-06-29  
**Status:** Proposal  
**Author:** nogate-ai session  
**Scope:** `arch formal`, `src/formal.rs`, new `src/formal_desugar.rs`

---

## Problem

`arch formal` is the compiler's native bounded model checker — no external tools required.
It currently handles flat `module` constructs with scalar types and one level of sub-module
hierarchy (`reg`/`comb`/`seq`/`let`/`wire`, basic `if`/`match` statements, `assert`/`cover`).

Every other ARCH construct — `fsm`, `counter`, `arbiter`, `pipeline`, `thread`, `fifo`, `ram`
— errors immediately:

```
arch formal TrafficLight.arch --bound 20
error: unsupported construct `fsm TrafficLight` — arch formal v1 handles
       flat module only; use `arch build | ebmc` for fsm/thread/pipeline
```

This means:
- **18 of 21 FSM VerilogEval designs** cannot be formally verified natively.
- **All `thread`-based DMA / interconnect designs** fall back to the 3-step `arch build →
  EBMC / SymbiYosys` pipeline, losing the zero-dependency advantage of `arch formal`.
- The auto-emitted FSM coverage properties (`_auto_fsm_legal_state`, `_auto_fsm_state_cover`,
  `_auto_fsm_transition_cover`) are never exercised by `arch formal`, only by EBMC.
- LLM-generated FSM code gets no compile-time formal feedback; the earliest signal is a
  missing state-cover hit in `arch sim --coverage`.

---

## Root cause

The SMT encoder in `src/formal.rs` operates on ARCH AST items directly, handling only `ModuleDecl`.
It has no mechanism to lower `FsmDecl`, `CounterDecl`, or `ArbiterDecl` into the module-level
constructs (`reg`, `comb`, `seq`) it already knows how to encode.

The compiler *already performs equivalent lowering* for other backends:
- `src/sim_codegen/gen_fsm.rs` lowers `FsmDecl` → C++ state register + next-state comb
- `src/sim_codegen/gen_counter.rs` lowers `CounterDecl` → C++ counter logic
- `arch build` SV emitter lowers each construct to deterministic SV

But there is no shared "lower to mid-level module IR" pass that `arch formal` could consume.
Each backend does its own lowering to its own output format.

---

## Proposed solution: `desugar_for_formal()` pre-pass

Add a new `src/formal_desugar.rs` module with a `desugar_for_formal(items)` function that
runs **before** the SMT encoder and converts select constructs into `ModuleDecl`-equivalent form.

The SMT encoder is then unchanged — it operates on the desugared items exactly as it does today
on hand-written modules.

### Phase 1: `fsm` desugar (highest value)

Lower `FsmDecl` to `ModuleDecl`:

| FSM element | Desugared equivalent |
|---|---|
| State enum K variants | `UInt<⌈log₂(K)⌉>` state register `state_r` |
| `default state IDLE;` | `reg state_r: UInt<N> reset rst => IDLE_VAL;` |
| `state RUNNING → DONE when cond;` | comb next-state `if state_r == RUNNING and cond { state_r_next = DONE_VAL; }` |
| FSM-scope `reg x: T` | promoted to module `reg x: T` |
| FSM-scope `seq on clk ...` | promoted to module `seq on clk ...` |
| `_auto_fsm_legal_state` SVA | `assert _legal: state_r < K_VAL;` in desugared module |
| `cover state_r == S` auto-props | `cover _reach_S: state_r == S_VAL;` in desugared module |

**Result:**

```
arch formal TrafficLight.arch --bound 20
PROVED: _legal (state_r < 3 holds at all reachable cycles)
HIT:    _reach_RED (cycle 0)
HIT:    _reach_GREEN (cycle 5)
HIT:    _reach_YELLOW (cycle 8)
```

No EBMC, no `arch build`, no SV step.

### Phase 2: `counter` desugar

Lower `CounterDecl` to `ModuleDecl`:
- Count register `count_r: UInt<W>`
- Next-value expression: wrap/saturate/gray mode as a comb expression
- `at_max`/`at_min` outputs as `let` bindings
- Auto-emitted `_auto_count_range` SVA → `assert` in desugared module

### Phase 3: `arbiter` desugar (scope-limited)

Lower `ArbiterDecl` (round_robin, priority) to `ModuleDecl`:
- Grant register `grant_r: UInt<$clog2(N)>`
- Next-grant combinational logic from the policy
- `round_robin`: the existing pointer register + one-hot scan

`lru`, `weighted`, and `custom hook` policies are deferred (complex lowering) — `arch formal`
continues to error on those.

---

## Why now

### 1 — Existing lowering paves the way

`gen_fsm.rs` (sim codegen) already implements FSM→C++ lowering, which mirrors exactly what the
desugar pass needs to produce as ARCH AST nodes. The logic is already written; the desugar pass
translates it into the AST rather than C++ strings.

### 2 — FSM formal is the highest-value target

FSM verification is where bounded model checking is most effective:
- Finite state spaces → proofs terminate quickly
- Reachability properties are natural (`cover state_r == FAULT`)
- Invariants are natural (`assert state_r < K`)
- The auto-emitted `_auto_fsm_legal_state` is already the right property — it just can't be
  proved natively today

### 3 — Complements existing formal coverage

The existing auto-emitted SVA path (`arch build | ebmc`) continues to work. The desugar pass
adds a **native** path that:
- Requires zero external tools
- Is testable in CI without EBMC installed
- Gives the same results on the same properties (verifiable by running both paths in a regression)

### 4 — LLM-generated FSM code gets instant feedback

With `arch check` catching unreachable states (issue #602) and `arch formal` now verifying FSM
invariants and coverage in one step, the feedback loop for LLM-generated `fsm` constructs
becomes: write → `arch check` (structural, fast) → `arch formal` (semantic, bounded). No
external toolchain needed.

---

## Interaction with existing features

| Feature | Interaction |
|---|---|
| `--auto-thread-asserts` SVA | Formal on `thread` constructs remains deferred (thread lowering is more complex; use `arch build --auto-thread-asserts \| ebmc` for now) |
| `arch build \| ebmc` path | Complementary; desugar produces same properties; use both for cross-checking |
| Hierarchical formal (`plan_hierarchical_formal.md`) | Orthogonal; desugar flattens a single top-level construct to module; hierarchy remains separate |
| `arch sim --coverage` FSM counters | Orthogonal; desugar produces formal coverage properties not simulation counters |
| FSM unreachable-state check (#602) | Complementary; #602 is structural (static BFS), desugar enables formal (bounded model check) |

---

## Non-goals (do not implement in Phase 1)

- `pipeline` desugar — pipeline hazard logic and stall chains produce complex comb that may
  exceed current SMT encoder scope (no `Vec`/`struct`); tracked separately.
- `thread` desugar — thread lowering exists in `src/elaborate.rs` but produces a module with
  an embedded sub-module; requires hierarchical formal, which is still limited (#383).
- `ram`/`fifo`/`cam`/`linklist` — array-based state; SMT encoder's v1 scalar restriction
  would need lifting first (`Vec` / multi-word support).
- `lru`/`weighted`/`custom` arbiter policies — complex policy logic; defer to Phase 3+.
- New CLI flags — `arch formal FsmFile.arch` just works; no new flags needed for Phase 1.

---

## Implementation sketch

### Files to add/modify

- **New:** `src/formal_desugar.rs` — `desugar_for_formal(items: &mut Vec<Item>)`
- **Modify:** `src/formal.rs` — call `desugar_for_formal(&mut items)` before `encode()`
- **Modify:** `src/main.rs` — no change (desugar is transparent to the CLI)
- **New fixtures:** `tests/formal/fsm_traffic_light.arch`, `tests/formal/fsm_counter_reachable.arch`,
  `tests/formal/counter_range.arch`, `tests/formal/arbiter_grant_valid.arch`

### Core desugar shape (pseudocode)

```rust
pub fn desugar_for_formal(items: &mut Vec<Item>) -> Result<(), Vec<CompileError>> {
    let mut out = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        match item {
            Item::Fsm(fsm)     => out.push(Item::Module(lower_fsm_to_module(fsm)?)),
            Item::Counter(ctr) => out.push(Item::Module(lower_counter_to_module(ctr)?)),
            Item::Arbiter(arb) if is_simple_policy(&arb) =>
                                  out.push(Item::Module(lower_arbiter_to_module(arb)?)),
            other              => out.push(other),
        }
    }
    *items = out;
    Ok(())
}
```

The `lower_fsm_to_module` function mirrors `gen_fsm` logic but emits `ModuleDecl` AST nodes
instead of C++ strings, making the desugar output directly consumable by the existing encoder.

### Estimated effort

| Phase | Effort | Unlocks |
|---|---|---|
| Phase 1 — `fsm` | ~400 LoC + 4 test fixtures | 18/21 FSM VerilogEval designs; TrafficLight, all `e203` FSMs |
| Phase 2 — `counter` | ~150 LoC + 2 fixtures | `wrap`/`saturate`/`gray`/`one_hot`/`johnson` formally verified |
| Phase 3 — `arbiter` (priority + round_robin) | ~200 LoC + 2 fixtures | Round-robin grant fairness provable in bounded steps |

---

## Acceptance criteria

- `arch formal TrafficLight.arch --bound 20` produces PROVED for `_legal` and HIT for all
  three state cover properties; no external tooling required.
- `arch formal WrapCounter.arch --bound 300` produces PROVED for `_auto_count_range`
  (`count_r <= MAX`); matches what EBMC produces on the same SV + SVA.
- Existing `arch formal` regression suite (flat module, hierarchical module, counter module
  written as plain `module` today) is byte-identical.
- `arch formal` on a design using `lru` arbiter policy still errors clearly ("desugar:
  `lru` policy not yet supported — use `arch build | ebmc`").

---

## Related issues and plan docs

- `doc/plan_hierarchical_formal.md` — hierarchical module formal (orthogonal)
- Issue #383 — formal rejects thread sub-module (separate, thread desugar is future work)
- Issue #602 — FSM unreachable-state detection in `arch check` (complementary static check)
- `COMPILER_STATUS.md` §CLI & Backend — "arch formal v1: flat module only; errors clearly on
  unsupported constructs" (the baseline this proposal extends)
