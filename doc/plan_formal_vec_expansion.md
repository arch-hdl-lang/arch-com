# `arch formal` Vec<T, N> Support via Bitvector Expansion

**Filed:** 2026-06-26  
**Author:** scheduled research session  
**Status:** proposal — not yet implemented  
**Repos:** arch-com  
**Related:** `doc/plan_hierarchical_formal.md` (mentions Vec as deferred),  
`src/formal.rs:1598` (`TypeExpr::Vec` hard-errors), `doc/COMPILER_STATUS.md`
("Scope v1: flat module, scalar types…no Vec/struct/enum")

---

## Problem

`arch formal` today rejects any module that has a `Vec<T, N>` register or port:

```
error: Vec types are not supported by `arch formal` v1 — use scalars
```

This blocks formal verification of the entire class of designs where Vec is the
natural representation:

| Design | Vec usage |
|---|---|
| N-requester arbiter | `port req: in Vec<Bool, N>` request mask; `reg grant: Vec<Bool, N>` |
| Pipeline with per-stage valids | `reg valid_r: Vec<Bool, STAGES>` chain |
| Cache tag array | `reg tags: Vec<UInt<TAG_W>, WAYS>` |
| Round-robin pointer | `reg last_grant: Vec<Bool, N>` one-hot |
| Priority encoder | `let first_valid: ... = valids.find_first(item)` |

Because these designs use Vec, the only formal verification path today is the
external EBMC/SymbiYosys toolchain consuming `arch build` SV output. That
defeats the direct-SMT advantage of minimum toolchain dependency and makes
`arch formal` impractical for the most common single-module verification tasks.

`arch formal` for flat modules with scalar types is solid (PROVED/REFUTED/HIT
verified for z3, boolector, bitwuzla). Extending it to Vec is the single highest-
leverage change to make it usable on real designs.

---

## Proposed Enhancement: Bitvector Array Expansion

Expand each `Vec<T, N>` signal into **N independent scalar BitVec declarations**,
one per element. Every downstream encoder (declarations, comb equations, register
transitions, property encoding) works over the expanded names.

### Naming convention

A signal `foo: Vec<UInt<8>, 4>` expands to four scalar slots:

```
foo__0   (BitVec 8)
foo__1   (BitVec 8)
foo__2   (BitVec 8)
foo__3   (BitVec 8)
```

Double-underscore keeps the slot names collision-free with any user-declared
signal named `foo_0` (single underscore). The compiler already manges names
with `_`; `__` is reserved for generated slots.

### Index encoding

**Constant index** (`v[2]` where 2 is a compile-time literal): directly reference
the corresponding slot — `foo__2_{t}`. The typecheck/elaboration pass already
resolves constant indices, so this is the common case.

**Variable index** (`v[i]` where `i` is a run-time signal): encode as an ITE
chain over all N possible values:

```smt2
; v[i]  where v : Vec<UInt<8>, 4>, i : UInt<2>
(ite (= i_t #b00) foo__0_t
  (ite (= i_t #b01) foo__1_t
    (ite (= i_t #b10) foo__2_t
      foo__3_t)))
```

The chain is always bounded (N elements, N known at compile time). For typical
Vec sizes in hardware (N ≤ 64 for arbiters, N ≤ 16 for pipeline stages, N ≤ 8
for cache ways) the ITE depth is negligible.

### Register transitions with variable-index writes

`vec_r[i] <= expr` in a `seq` block must update exactly one slot. Each slot's
next state is:

```
vec_r__k_next = ite(k == i, expr, vec_r__k)   for k = 0 .. N-1
```

This is the standard "conditional register update" pattern; it adds N ITE nodes
per variable-index write but remains tractable for small N.

### For-loop unrolling

`for i in 0..N-1 ... end for` in a `comb` or `seq` block is already elaborated
at compile time for constant bounds. In `arch formal` the loop body is unrolled
into N copies with constant `i` substituted — same as what the SV emitter does
for generate-for. This means most Vec-loop patterns produce constant-index
accesses in the expanded form.

### Vec methods in formal context

Vec predicate methods (`find_first`, `any`, `all`, `count`, `contains`) are
already lowered by `typecheck.rs` to internal synthetic result records. The
formal encoder needs to handle those synthetic results as additional scalar
signals derived from N parallel comparators. For example, `valids.any(item)` 
expands to:

```smt2
(declare-fun valids_any_result_t () (_ BitVec 1))
(assert (= valids_any_result_t
  (ite (or (= valids__0_t #b1)
           (= valids__1_t #b1)
           ...
           (= valids__n1_t #b1))
       #b1 #b0)))
```

`find_first` expands to the N-compare priority-encoder ITE sequence the SV
emitter already generates, rewritten in SMT-LIB2.

---

## Scope

### In scope for v1

- `Vec<T, N>` **registers** (`reg x: Vec<T, N> reset rst => 0`) — declaration,
  reset constraints, transition, and property encoding.
- `Vec<T, N>` **input ports** (unconstrained free-choice per cycle, same as
  scalar inputs).
- `Vec<T, N>` **output ports** and **wires** (comb-equation constrained).
- Constant-index reads and writes (`v[2]`, `v[N-1]`).
- Variable-index reads (`v[i]`) via ITE chain.
- Variable-index writes (`v[i] <= expr`) via per-slot ITE update.
- For-loop unrolling over constant bounds (already done at elaboration).
- Vec predicate methods (`any`, `all`, `find_first`, `count`, `contains`) where
  the receiver is a Vec of scalars.
- Multi-dimensional `Vec<Vec<T, N>, M>`: flatten to M×N named slots (e.g.
  `foo__0__0`, `foo__0__1`, ..., `foo__1__0`, ...).
- Hierarchical formal: propagate expansion through `flatten_for_formal` so
  sub-instance Vec ports and registers are also expanded.

### Deferred

- `Vec<struct, N>` or `Vec<enum, N>` — deferred to when struct/enum support
  lands; struct/enum are still rejected by `check_scalar_type`.
- Counter-examples with Vec values in the printed witness — the SMT model
  returns N scalar bitvectors; the counter-example printer needs to reassemble
  them into a human-readable `[v0, v1, ...]` format. Doable but separate.
- FP32/BF16 element types in Vec — also deferred; FP formal encoding is its own
  project.

---

## Why This Matters for Real Designs

The three most common ARCH single-module verification use-cases that `arch
formal` cannot touch today, but would work after this change:

### 1. Arbiter mutual-exclusion proof

```arch
module Arb
  param N: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port req:   in Vec<Bool, N>;
  port grant: out Vec<Bool, N>;
  reg last_r: Vec<Bool, N> reset rst => 0'bN;
  ...
  assert at_most_one_grant: (grant as UInt<N>).count(item) <= 1;
end module Arb
```

Currently rejected because `req`, `grant`, and `last_r` are all Vec.
After this change: `arch formal Arb.arch` can PROVE the mutual-exclusion
assertion up to bound B — no EBMC/SymbiYosys needed.

### 2. Pipeline valid-bit chain correctness

```arch
module Pipe
  param STAGES: const = 4;
  ...
  reg valid_r: Vec<Bool, STAGES> reset rst => 0'b4;
  assert no_valid_skid: valid_r[0] or not valid_r[1];
end module Pipe
```

### 3. Cache replacement policy

```arch
module LruCache
  param WAYS: const = 4;
  ...
  reg age: Vec<UInt<2>, WAYS> reset rst => 0'b8;
  assert ages_distinct: age[0] != age[1]; // can be proved for reset + transitions
end module LruCache
```

All three are currently out-of-reach for `arch formal`. All three become
tractable with Vec expansion.

---

## Implementation Sketch

### 1. Expand Vec declarations (in `FormalCtx::preprocess`)

When a signal's type is `Vec<T, N>`:
- Fold `N` to a compile-time constant (error if non-const).
- Instead of calling `check_scalar_type` (which rejects Vec), iterate 0..N:
  add `{name}__{k}` as a scalar of type `T` to the appropriate signal list
  (inputs/outputs/regs/wires).
- Store a mapping: `original_name → (elem_type, N, [slot_names])`.

### 2. Expr encoder handles `ExprKind::Index`

In `encode_expr`, when the receiver's type is Vec and we see `v[i]`:
- If `i` is constant: emit `{name}__{k}_{t}` directly.
- If `i` is variable: build ITE chain over all slots.

For `ExprKind::VecMethod` results (the synthetic records from `find_first` etc.):
- Emit the N-comparator encoding inline, or declare a helper wire per method call site.

### 3. Seq block: variable-index write → slot ITE update

In `encode_seq_stmt` for `Stmt::Assign { target: v[i], value }`:
- For each slot k: add a synthetic "next_r" equation:
  `v__k_next = ite(i == k, value, v__k_current)`.
- Feed these into the transition encoder as N parallel reg-next constraints.

### 4. Reset constraints for Vec regs

`reset rst => 0` on a `Vec<T, N>` reg sets every slot to 0 at t=0.
`reset rst => {a, b, c, d}` on `Vec<T, 4>` sets slot k to the k-th literal.

### 5. Counter-example printing (stretch goal)

When the SMT model is SAT and the tool prints a witness, reassemble slot
values into `[v0, v1, ..., vN-1]` for readability.

### 6. Tests

- `tests/formal/formal_arbiter_mutual_exclusion.arch` — PROVES N=4 arbiter
  never grants twice; `Vec<Bool, 4>` request/grant ports.
- `tests/formal/formal_pipeline_valid_chain.arch` — PROVES valid bits
  propagate correctly for a 3-stage pipeline; `Vec<Bool, 3>` register.
- `tests/formal/formal_vec_variable_index.arch` — REFUTES a design with an
  unconstrained write index (can corrupt adjacent slots).

---

## Effort Estimate

| Component | Estimated LoC (Rust) |
|---|---|
| Vec signal expansion in `preprocess` | ~80 |
| `encode_expr` ITE chain for variable index | ~60 |
| Seq-block variable-index write → slot ITE update | ~80 |
| Reset constraint expansion | ~30 |
| Vec predicate method encoding | ~120 |
| Hierarchical flatten propagation | ~40 |
| Tests (`.arch` files + integration assertions) | ~200 |
| **Total** | **~610 LoC** |

Estimated effort: 2–3 focused sessions. Can land as two PRs:
- **PR 1**: scalar-index Vec expansion only (common path, no ITE chains) — ~250 LoC + 2 tests
- **PR 2**: variable-index reads/writes + Vec method encoding — ~360 LoC + additional tests

---

## Non-Goals

- Struct / enum formal support — separate proposal when needed.
- FP32/BF16 in formal — covered by the FP formal encoding work.
- SMT array-theory encoding (`(Array Index Elem)`) — bitvector expansion
  is simpler, avoids solver differences in array support, and handles ARCH's
  fixed-size-Vec contract with no overhead for typical N ≤ 64.
- Unbounded model checking — this extends BMC, same as the existing formal backend.
