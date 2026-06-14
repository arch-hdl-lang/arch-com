# `assume` Formal Constraints — Close the Spec-to-Implementation Gap

**Date:** 2026-06-14
**Status:** Proposal — not yet implemented

---

## Problem

`arch formal` runs bounded model checking with all primary inputs fully unconstrained.
In practice every non-trivial module has an environment contract — addresses are
aligned, requests are one-hot, payloads arrive in-order, bus signals conform to a
protocol.  Without a way to encode those constraints in source, every `arch formal`
run on a real module returns a flood of spurious `REFUTED` results because the solver
explores physically impossible input combinations.

COMPILER_STATUS.md already documents this pain explicitly:

> EBMC 5.11: `UInt<4> wr_idx` into `Vec<_,4>` ⇒ **REFUTED** at the leaf module
> (unconstrained input can reach 15; **caller must constrain**).

The current workaround ("use a narrower type to constrain by structure") only works
when the constraint maps cleanly to a bit-width.  Alignment constraints, one-hot
constraints, ordering invariants, and protocol-state guards cannot be expressed as
type narrowing.

## Specification

`assume` is already specified in **ARCH HDL Specification §12.3** with the following
semantics table:

| Keyword | Simulation | Formal tool | Meaning |
|---|---|---|---|
| `assert name: expr` | Error if false | Property to prove | Invariant — must always hold |
| `cover name: expr` | Log when true | Reachability check | Confirms a scenario is reachable |
| **`assume name: expr`** | **Ignored** | **Input constraint** | **Restricts the input space for formal analysis** |

The spec example:

```arch
module FormalWrapper
  port req: in UInt<4>;

  // Constrain the formal tool: only consider one-hot inputs
  assume one_hot: req == 0 or (req & (req - 1)) == 0;
end module FormalWrapper
```

The compiler has **zero implementation** of this keyword — no lexer token, no AST
node, no type-check rule, no SV or SMT-LIB2 emission path.

## Why It Matters

1. **`arch formal` is currently too noisy to use on production designs.**
   A memory controller, AXI slave, or pipeline stage always has several environment
   assumptions (aligned addresses, legal request sequences, protocol-valid signals).
   Without `assume`, every formal run on these requires either:
   - Artificially narrow types (wrong: changes the design interface), or
   - Wrapper modules that narrow inputs combinationally (verbose, error-prone), or
   - Ignoring spurious counterexamples manually (defeats the purpose).

2. **The fix is co-located with the design, not scattered across testbenches.**
   `assume` in source makes the environment contract visible to every consumer:
   the formal tool, future reviewers, and LLMs re-generating the module.

3. **`arch build` can emit SV `assume property(...)` — consumed by EBMC, SymbiYosys,
   Jasper, and other industry tools with no extra work.**

4. **In simulation, violating an `assume` is a testbench bug.**
   Instead of silently ignoring, `arch sim` can emit a runtime check that fires when
   an assumption is violated during simulation — the same "your environment sent an
   illegal input" signal that formal would have pruned.  This is strictly more
   useful than silence and catches testbench errors early.

## Implementation Sketch

The implementation mirrors `assert` / `cover` — all three share the same grammar
position and the same expression type requirement (`Bool`).

### 1. Lexer / Parser

Add `assume` as a keyword token.  Parse:

```
assume <name>: <bool-expr>;
```

at module / construct scope, producing a new `ItemKind::Assume { name, expr }` AST
node.  Grammar position: alongside `assert` / `cover` items.

### 2. Type checker

Same check as `assert`: the expression must resolve to `Bool`.  Same "requires a
Clock port" gate — an `assume` with no clock context has no cycle-level semantics.
(Comb-only modules error, same as `assert`.)

### 3. `arch build` — SV emission

Emit inside `synopsys translate_off/on` (same as `assert` / `cover`):

```sv
// synopsys translate_off
assume_one_hot: assume property (@(posedge clk) disable iff (rst) req == 0 || (req & (req - 1)) == 0);
// synopsys translate_on
```

EBMC, Jasper, SymbiYosys, and DC all consume `assume property(...)` as solver
constraints.  Verilator treats it as a zero-cost annotation (same as the existing
`assert property` blocks).

The label prefix convention: `assume_<name>` (parallel to the current `assert_<name>`
convention for auto-generated and user-named assertions).

### 4. `arch formal` — SMT-LIB2 encoding

In the current encoder, each cycle step asserts `(assert (=> (= cycle t) property_t))`
for each user-written `assert`.  For `assume`, the encoding flips: instead of adding
to the property conjunction that must be *proved*, add to the *precondition* that
the solver assumes:

```smt2
;; assume one_hot at cycle t: solver only explores inputs satisfying this
(assert (=> (and (>= cycle t) (< cycle bound)) one_hot_expr_t))
```

This is the standard assume-guarantee encoding for BMC.  The constraint holds at
every cycle inside the bound, pruning the input space before the solver explores it.

Implementation diff: in `src/formal.rs` (or wherever the SMT-LIB2 encoder lives),
distinguish `ItemKind::Assume` from `ItemKind::Assert` when iterating module items.
Assume → add to `constraint_list`; assert → add to `property_list`.  The final
`check-sat` form becomes:

```
(assert (and <constraints> (not (and <properties>))))
```

instead of the current `(assert (not (and <properties>)))`.

### 5. `arch sim` — Runtime environment-contract check

The spec says "Ignored" but there is a better choice: emit a runtime check that
fires when the assumption is violated.  This catches testbench errors that formal
verification would have pruned:

```cpp
// In eval_posedge() after all seq blocks:
if (!(_arch_assume_one_hot)) {
    fprintf(stderr, "ARCH-WARNING: assume 'one_hot' violated at cycle %llu "
                    "— testbench is exercising an input state outside the "
                    "formal contract\n", _cycle);
}
```

This is a **warning, not an abort** (unlike `assert` failures) because the design
itself is not broken — the testbench is.  Guarded by a `--check-assumes` flag so
simulation-only harnesses that intentionally stress invalid inputs aren't penalized.

---

## Scope and Effort

| Phase | Work | Effort |
|---|---|---|
| Parse + AST + typecheck | Add token, AST node, Bool check, Clock gate | ~1 day |
| `arch build` SV emission | One extra case in the assert/cover emitter | ~0.5 day |
| `arch formal` SMT-LIB2 | Constraint list vs property list split | ~1 day |
| `arch sim` runtime check | Warning emit behind `--check-assumes` flag | ~0.5 day |
| Tests | 3–4 integration tests covering each backend | ~1 day |

**Total estimated effort: ~4–5 days** for a complete, tested implementation.

---

## What This Unlocks

Before:
```
$ arch formal MemController.arch --bound 10
REFUTED: _auto_bound_vec_0 (unconstrained wr_idx reaches 15)
REFUTED: aligned_access (unconstrained addr is not 4-byte aligned)
```

After, with constraints co-located in source:
```arch
module MemController
  port wr_idx: in UInt<4>;
  port addr:   in UInt<32>;

  assume wr_in_range: wr_idx < 12;
  assume addr_aligned: (addr & 3) == 0;
  // ...
end module MemController
```

```
$ arch formal MemController.arch --bound 10
PROVED: _auto_bound_vec_0 (up to bound 10)
PROVED: write_after_read_hazard (up to bound 10)
```

---

## Relationship to Existing Work

- **Not the same as `pragma rdc_safe;` / `pragma cdc_safe;`** — those are per-module
  opt-outs for the RDC/CDC checker.  `assume` is a formal-semantics primitive that
  constrains the input space.
- **Not the same as `arch formal --bound N`** — the bound controls time depth;
  `assume` controls the input space at each time step.
- **Extends the construct-proof certificate work** — the Lean / SMT certificates
  already generated for FIFO, arbiter, and credit_channel could be annotated with
  the `assume` constraints they rely on, making the certificate dependency explicit.

## References

- ARCH HDL Specification §12.3 ("assume --- Formal Constraints")
- COMPILER_STATUS.md, `arch formal` row ("caller must constrain" note)
- COMPILER_STATUS.md, bounds check row (EBMC REFUTED on `UInt<4> wr_idx`)
- `doc/plan_hierarchical_formal.md` — hierarchical formal plan (mentions `assume`
  bindings in the v2 deferred section)
