# Thread-to-FSM Lowering Algorithm

This document describes exactly how `thread` blocks are lowered to synthesizable
SystemVerilog by `elaborate::lower_threads` / `lower_module_threads`.

---

## Overview

Every `thread` block in a module is lowered to a per-thread integer state register
(`_t{i}_state`) and associated combinational / sequential logic inside a single
auto-generated submodule (`_ModuleName_threads`).  The parent module retains only
non-thread items, and instantiates `_threads` to recover the moved signals.

```
source module M
  reg r, s             ←── thread-driven regs lifted to submodule
  thread T0 ...
  thread T1 ...
  let x = r + s        ←── stays in M; r,s now come from _threads output ports
─────────────────────────────────────────────────────────────────────────────
emitted module _M_threads            emitted module M (stripped)
  port r: out UInt<…>  (reg-port)      let x: T = r + s
  port s: out UInt<…>  (reg-port)      _M_threads _threads (…)
  _t0_state  _t1_state regs
  always_comb  (per-state enables)
  always_ff    (merged state machines)
```

---

## Phase 1 — Signal Classification

`collect_thread_signals(body)` walks every `ThreadStmt` and classifies each
referenced name into one of three sets:

| Set | Meaning |
|-----|---------|
| `comb_driven` | assigned with `=` (comb assign) in some thread |
| `seq_driven` | assigned with `<=` (seq assign) in some thread |
| `all_read` | read but not written (input) |

`default_when` conditions and statements are included in all three sets.

### Port inference

From these sets the submodule ports are inferred automatically:

| Condition | Port direction |
|-----------|---------------|
| name ∈ `all_read` only | `in` (input port) |
| name ∈ `comb_driven` | `out` (comb output, default = 0) |
| name ∈ `seq_driven` | `out` (reg-port: holds state across cycles) |
| name is `clk` or `rst` | `in` (always) |
| name starts with `_t` or is `_cnt`/`_loop_cnt` | excluded (per-thread internal) |
| name ∈ lock internal (`_{res}_req`, `_{res}_grant`) | excluded (internal) |

---

## Phase 2 — Lock Arbitration

For each `resource` declared in the parent module that appears in at least one
`lock` block, the submodule generates:

- Per-thread wires `_{res}_req_{i}` and `_{res}_grant_{i}` (Bool)
- A **priority arbiter** (combinational):

```
grant[0] = req[0]
grant[1] = req[1] && !grant[0]
grant[2] = req[2] && !grant[0] && !grant[1]
…
```

This is a simple fixed-priority (lowest thread index wins) arbiter generated
inline as a `comb` block.  The choice is deterministic and deadlock-free because
only one thread can hold a grant in any clock cycle.

---

## Phase 3 — `shared(or)` Signals

Signals declared `port x: out T shared(or)` may be driven from multiple threads.

**Comb-driven shared(or)**: `transform_shared_or_assigns` rewrites each assignment
`x = v` as `x = x | v` (OR-accumulation into the default-zero baseline).

**Seq-driven shared(or)**: generates per-thread shadow wires `_x_in_{i}` and a
reduction wire `_x_next = _x_in_0 | _x_in_1 | … | _x_in_{n-1}`.  Each thread's
seq assign `x <= v` is rewritten to the comb assign `_x_in_{i} = v`.  After all
threads, `x <= _x_next` is appended to the merged `always_ff`.

---

## Phase 4 — Per-Thread State Partitioning

This is the core algorithm. `partition_thread_body` converts a flat list of
`ThreadStmt` into a list of `ThreadFsmState` records.

### ThreadFsmState structure

```
struct ThreadFsmState {
  comb_stmts:       [CombStmt]   // outputs driven while in this state
  seq_stmts:        [Stmt]       // register updates that fire on exit edge
  transition_cond:  Option<Expr> // condition to advance (wait until / do..until)
  wait_cycles:      Option<Expr> // count-based wait (wait N cycle)
  multi_transitions: [(Expr, usize)] // conditional multi-target (fork/join, for)
}
```

At most one of `transition_cond`, `wait_cycles`, `multi_transitions` is set per
state.

### Statement → state mapping

```
ThreadStmt              Action
──────────────────────────────────────────────────────────────────────────────
x = expr                CombAssign → append to cur_comb  (no boundary)
x <= expr               SeqAssign  → append to cur_seq   (no boundary)
if/else (no waits)      Converted to CombIfElse / IfElse → appended to cur_*
if/else (with waits)    Dispatch state + recursive partition + rejoin (see §4d)
wait until cond         Flush pending → new state with transition_cond=cond
wait N cycle            Flush pending → new state with wait_cycles=N
do { … } until cond     Flush pending → new hold-state with transition_cond=cond
for i in s..e { … }     lower_thread_for   (see §4a)
lock res { … }          lower_thread_lock  (see §4b)
fork…and…join           lower_fork_join    (see §4c)
```

"Flush pending" means: if `cur_comb` or `cur_seq` is non-empty, emit a state for
them first (unconditional advance), then clear both.

### Trailing statements

After the last `wait` in the body, any remaining `cur_comb`/`cur_seq` forms a
trailing state.  Optimisation: if the previous state is a `wait until` / `do..until`
state (has `transition_cond`) and `cur_comb` is empty, the seq assigns are merged
into the previous state guarded by `transition_cond`.  This eliminates one dead
cycle.  Similarly for `for`-loop exit transitions.

---

### 4a — `for` loop lowering (`lower_thread_for`)

```
for i in s..e { <body> }
```

The loop variable `i` is replaced by `_loop_cnt` throughout `body`.

**Counter initialisation**: `_loop_cnt <= s` is merged into the *preceding* state
(if it has an unconditional advance and no `multi_transitions`), avoiding a
dedicated init cycle.  Otherwise a flush state is created.

**Body states**: `body` is recursively partitioned.  The body must contain at least
one `wait` statement.

**Loop-back**: the counter increment and branch logic is merged into the **last**
body state's `multi_transitions`:

```
multi_transitions:
  (body_cond && _loop_cnt <  e.trunc<W>()) → loop_back_state  (state[0] of body)
  (body_cond && _loop_cnt >= e.trunc<W>()) → exit_state        (next after for)
```

The counter is incremented inside the last body state's `seq_stmts` (guarded by
`body_cond`).  Target indices initially use sentinel `usize::MAX` for "next state
after for group", resolved to absolute indices after merging into the parent list.

```
States produced for:  for b in 0..burst_len-1 { do { … } until cond; }

  S0 [from preceding context]    (counter init merged here if possible)
  S1  comb: body_comb            ← loop back target
      transition_cond: cond      (do..until)
      seq: cnt <= cnt+1          (merged into S1)
      multi: cond && cnt < end → S1
             cond && cnt >=end → S2
  S2  <next context>
```

---

### 4b — `lock` block lowering (`lower_thread_lock`)

```
lock res { <body> }
```

`body` is recursively partitioned.  Then:

1. `_{res}_req = 1` is prepended to every body state's `comb_stmts`.
2. **First body state only**:
   - All non-req comb outputs are moved inside `if (_{res}_grant) { … }`.
   - `transition_cond` is ANDed with `grant`: `grant && original_cond`.
     If there was no `transition_cond` the first state gets `transition_cond = grant`.
   - All `seq_stmts` are wrapped in `if (grant) { … }` to prevent spurious
     register updates while waiting for arbitration.
3. Subsequent body states execute unconditionally while req=1 (grant is held
   because req stays asserted and priority is fixed).

**Key property**: a thread that wins the grant enters the first body state and
immediately executes body logic in the same cycle (zero-cycle lock acquisition
if uncontended).

**Nested lock blocks are rejected at compile time** (see §Liveness and Safety
below for the proof of why they would violate mutual exclusion).

```
States produced for:  lock ar { ar_valid=1; ar_id=id; until ar_ready; }

  Sₙ   comb: _ar_req = 1
             if (_ar_grant) { ar_valid = 1; ar_id = id }
       transition_cond: _ar_grant && ar_ready
       seq: (guarded by grant)
```

---

### 4c — `fork / join` lowering (`lower_fork_join`)

```
fork
  <branch_0>
and
  <branch_1>
[and <branch_k> …]
join
```

Each branch is independently partitioned.  A synthetic "done" state is appended to
each branch (unconditional advance, no comb/seq).  The Cartesian product of all
branch state indices is enumerated:

```
flat_idx = i0 + i1*L0 + i2*L0*L1 + …
```

For each product-state the algorithm:

1. **Merges** the comb/seq of all branches' current states.  Branch seq assigns are
   guarded by that branch's `transition_cond` (to fire only when that branch advances).
2. **Builds `multi_transitions`**: iterates over all `2^n − 1` non-empty subsets of
   active (not-yet-done) branches from high to low.  Any subset that includes all
   unconditional branches yields a transition to the encoded next-index.  The
   transition condition is the AND of all active branches' conditions (positive) and
   NOT of all inactive branches' conditions (negative).

**All-done product state** (all indices at last-1) is given an unconditional advance
— transitions to the next main-line state.

**Size guard**: product > 64 is rejected at compile time.

### 4d — `if/else` with internal waits — dispatch-and-rejoin

When an `if/else` body contains a `wait` (any form), the conditional cannot be
folded into a single combinational `if/else` — control has to split across
multiple cycles. The lowering emits:

```
S_pre   : (flush of pending comb/seq before the if)
S_disp  : empty comb/seq, M = [(cond, then_base), (¬cond, else_base)]
[then_states] : recursive partition of then_stmts (offset then_base)
[else_states] : recursive partition of else_stmts (offset else_base)
S_rejoin: (the next state after the if/else, or the post-if chain)
```

Each branch's last state is then *redirected* so its natural fallthrough lands
at `S_rejoin` instead of falling through to the other branch's first state.
`redirect_fallthrough_to` handles four shapes of last state:

| Last state | Edit |
|---|---|
| `M = ∅, τ = ⊥, w = ⊥` (unconditional) | replace with `M = [(true, rejoin)]` |
| `M = ∅, τ = c` (wait_until) | replace with `M = [(c, rejoin)]` |
| `M = ∅, w = n` (wait_cycles) | replace with `M = [(cnt == 0, rejoin)]` (counter decrement is hoisted out so the `M`-arm doesn't suppress it) |
| `M ≠ ∅` (e.g. for-loop exit) | append `(true, rejoin)` only if no entry already targets `rejoin` |

Empty branches (`then_stmts == []` or `else_stmts == []`) skip the recursive
call and the dispatch points that arm directly at `rejoin`.

Soundness: see `doc/thread_lowering_proof.md` §II.10 (Lemma I).

---

## Phase 5 — State Register and Code Generation

For each thread `ti` with `n_states` states:

```
state_bits = ⌈log₂(n_states)⌉  (minimum 1)
reg _t{ti}_state: UInt<state_bits> init 0 reset rst => 0;
```

### always_comb (per-state enables)

For each state `si` with non-empty `comb_stmts`:

```sv
if (_t{ti}_state == si) begin
  <comb_stmts, shared(or) transforms applied>
end
```

All outputs default to `0` before the per-thread blocks.

### always_ff (merged, single block)

Per-thread per-state:

```sv
if (_t{ti}_state == si) begin
  <seq_stmts>               // register updates
  <transition logic>        // see below
end
```

**Transition logic** — determined by state kind:

| State kind | Generated logic |
|------------|-----------------|
| `transition_cond = Some(c)` | `if (c) _state <= si+1` |
| `wait_cycles = Some(n)` | `_cnt <= _cnt - 1; if (_cnt==0) _state <= si+1` (counter pre-loaded by preceding state) |
| `multi_transitions = [(c0,t0),(c1,t1),…]` | `if (c0) _state <= t0; if (c1) _state <= t1; …` |
| none (unconditional) | `_state <= next` |

`next` = `si+1` for non-final states, `0` for the final state of a repeating
thread, `si` (hold) for `thread once` final state.

All threads share one `always_ff` block to avoid multi-driver conflicts on shared
registers.

### `default when` wrapping

If a thread has `default when cond { <assigns> }`:

```
if (cond) {
  <seq assigns from default_when body>
  _state <= 0          // unconditional state reset
} else {
  <normal per-state if chain>
}
```

This wraps the **entire** per-thread state chain, giving the default condition
absolute priority.

---

## Phase 6 — Counter Registers

After all thread comb/seq is emitted, per-thread counter registers are added to
the merged module body:

| Register | When created | Width |
|----------|-------------|-------|
| `_t{ti}_cnt` | thread has `wait N cycle` | 32 bits |
| `_t{ti}_loop_cnt` | thread has `for` loop | `infer_for_cnt_width` result |

`infer_for_cnt_width` walks the `for` end-expressions and picks the smallest
UInt width that fits all bounds (minimum 8 bits).

---

## Phase 7 — Merged Module and Parent Wiring

The generated `_ModuleName_threads` module contains:

```
ports: clk, rst, <inputs>, <comb-outputs>, <seq-reg-outputs>
body:
  [resource arbiter comb blocks]
  [shared(or) wire decls + let reductions]
  [state reg decls]
  [merged always_comb]
  [merged always_ff]
  [counter reg decls]
```

The **parent module** is modified:
- `RegDecl` items for all thread-driven regs are **removed** (they live in the submodule now)
- `Resource` declarations are consumed (lock logic is inline)
- An `InstDecl` for `_threads` is appended; every submodule port connects by name to a same-named signal in the parent

---

## Complete Example

```arch
thread ArIssuer on clk rising, rst high
  default when start and not active_r
    total_xfers_r <= total_xfers;
    active_r      <= true;
    _state        <= 0;   // implicit
  end default
  wait until active and xfer_ctr_r < total_xfers_r;   // S0
  do
    ar_valid = 1;
    ar_addr  = next_addr_r;
  until ar_ready;                                       // S1
  xfer_ctr_r <= xfer_ctr_r + 1;                       // trailing seq → merged into S1
end thread ArIssuer
```

Produces two states:

```
S0  comb: (none)
    transition_cond: active && xfer_ctr_r < total_xfers_r

S1  comb: ar_valid = 1; ar_addr = next_addr_r
    transition_cond: ar_ready
    seq (trailing, merged): xfer_ctr_r <= xfer_ctr_r + 1  (guarded by ar_ready)
```

Generated always_ff (simplified):

```sv
if (start && !active_r) begin           // default when
  total_xfers_r <= total_xfers;
  active_r      <= 1'b1;
  _t0_state     <= 0;
end else begin
  if (_t0_state == 0) begin             // S0: wait
    if (active && xfer_ctr_r < total_xfers_r)
      _t0_state <= 1;
  end
  if (_t0_state == 1) begin             // S1: do..until
    if (ar_ready)
      xfer_ctr_r <= xfer_ctr_r + 1;    // merged trailing seq
    if (ar_ready)
      _t0_state <= 0;                   // wrap (repeating thread)
  end
end
```

---

## Liveness and Safety: Lock Correctness

### Arbiter structure

For each resource the compiler generates a **fixed-priority combinational arbiter**:

```
grant[0] = req[0]
grant[1] = req[1] && !grant[0]
grant[i] = req[i] && !grant[0] && … && !grant[i-1]
```

Thread index 0 has the highest priority.  The arbiter is purely combinational —
it resolves in the same cycle with no flops of its own.

### Deadlock freedom (proof)

**Definition**: thread Ti *waits-for* Tj when Ti is blocked at lock body state 0
(`grant_i && cond` is false) and Tj has `req_j = 1` with `j < i`, causing
`grant_i = 0`.

**Claim**: the waits-for relation is acyclic, so no deadlock can form.

**Proof**: from the arbiter equations, `grant[i] = 0` only when some `grant[j]`
with `j < i` is 1.  Therefore:

> Ti waits-for Tj  ⟹  index(Tj) < index(Ti)

All waits-for edges point from higher-indexed threads toward lower-indexed ones.
A cycle would require some thread to appear on both ends of the edge ordering —
its index would need to be both strictly less than and strictly greater than
itself, which is impossible.  No cycle ⟹ no deadlock. ∎

Corollary: thread 0 always makes progress (no thread can block it), which
unblocks thread 1, which unblocks thread 2, etc. — the system is **starvation-free**
for all threads as long as every thread's lock body eventually terminates.

### Mutual exclusion

Mutual exclusion requires that at most one thread executes inside a given lock's
critical section at any time.

**Non-nested (sequential) locks — guaranteed**: while thread Ti is inside a lock
body, its `req_i = 1` throughout all body states.  The arbiter yields `grant_i =
1` (if Ti is the highest-priority requester) or prevents a lower-priority thread
Tj from getting `grant_j`.  A lower-priority thread at body state 0 is blocked
because `grant_j && cond` is false while `grant_i = 1`.  Tj cannot enter body
state 1 until Ti exits and drops req_i.

**Nested locks — not safe (and rejected)**:
The grant check only gates **body state 0**.  States 1, 2, … execute
unconditionally.  Consider:

```
Thread T1 enters lock B (body state 0 → advances to state 1)
Thread T0 (higher priority) arrives and requests lock B
grant_B_0 = 1 (T0 wins), grant_B_1 = 0 (T1 is outbid)
T0 enters B's body state 0 — executes B's critical section
T1 is in B's body state 1 — no grant check — also executing B's critical section
⟹ mutual exclusion violated
```

This scenario can only arise when a thread is already past state 0 of a lock
when a contender arrives — which can happen with nested locks (where the outer
lock keeps a thread resident in the inner lock's states while another thread
arrives).  For non-nested sequential usage, a thread is at state 0 (the entry
gate) when it first requests the lock, so the grant check is effective.

**Compiler enforcement**: `partition_thread_body` calls `collect_locked_resources`
on the lock body and **rejects any lock block whose body contains another lock
block** with a compile error.  This makes the non-nested invariant a compile-time
guarantee.

---

## Worked Sequence Example

This section traces a complete execution of two threads from the `ThreadMm2s`
module to show exactly how ARCH thread source maps to FSM state transitions.

**Parameters**: `total_xfers = 1`, `burst_len = 2`, `base_addr = 0x1000`

Only two threads are active:  ArIssuer (`t0`) and RCollect_0 (`t1`).
Threads `t2`/`t3`/`t4` remain in state 0 throughout (no xfers with ID 1/2/3).

---

### Step 1 — State partition

```
ArIssuer thread source               │  States produced
─────────────────────────────────────┼─────────────────────────────────────────
  wait until active                  │  S0  transition_cond:
        and xfer_ctr_r               │        active && xfer_ctr_r < total_xfers_r
            < total_xfers_r;  ───────┤
  do                                 │  S1  comb: ar_valid=1, ar_addr=next_ar_addr_r
    ar_valid = 1;              ──────┤        ar_id=xfer_ctr_r[1:0], ar_len=burst_len_r-1
    ar_addr  = next_ar_addr_r;       │        ar_size=2, ar_burst=1
    ar_id    = xfer_ctr_r[1:0];      │      transition_cond: ar_ready
    ar_len   = burst_len_r - 1;      │      seq (trailing, merged under ar_ready):
    ar_size  = 3'd2;                 │        xfer_ctr_r <= xfer_ctr_r + 1
    ar_burst = 2'd1;                 │        next_ar_addr_r <= next_ar_addr_r + stride
  until ar_ready;  ─────────────────┘

RCollect_0 thread source             │  States produced
─────────────────────────────────────┼─────────────────────────────────────────
  wait until active and              │  S0  seq (every cycle in S0):
    (tc[0]<<2)+0 < xfer_ctr_r; ─────┤        _t1_loop_cnt <= 0     ← counter init
                                     │      transition_cond:
                                     │        active && tc[0]<<2 < xfer_ctr_r
  for b in 0..burst_len_r-1          │  S1  comb: r_ready=1
    do                        ───────┤        push_valid = r_valid && r_id==0
      r_ready    = 1;                │      multi_transitions (merged from for+do..until):
      push_valid = r_valid           │        cond && loop_cnt <  end → S1  (loop back)
                 and r_id==0;        │        cond && loop_cnt >= end → S0  (exit)
    until r_valid and r_id==0        │      seq (guarded by cond):
          and push_ready;     ───────┤        loop_cnt <= loop_cnt + 1
  end for                            │        if loop_cnt >= end: tc[0] <= tc[0] + 1
  tc[0] <= tc[0] + 1;  ─────────────┘   (trailing seq merged under exit condition)
```

`cond` = `r_valid && r_id==0 && push_ready`,  `end` = `burst_len_r - 1` = 1

---

### Step 2 — Timing diagram

Columns show registered state at the **start** of each cycle (after the previous
posedge).  Combinational signals show their value **during** the cycle.
Arrows (←) mark the register update that fires at the cycle's posedge.

```
        ┌──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────┐
Cycle   │  1   │  2   │  3   │  4   │  5   │  6   │  7   │  8   │  9   │
        ├──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┤
 INPUTS │      │      │      │      │      │      │      │      │      │
  start │  1   │  0   │  0   │  0   │  0   │  0   │  0   │  0   │  0   │
ar_ready│  0   │  0   │  0   │  1   │  0   │  0   │  0   │  0   │  0   │
r_valid │  0   │  0   │  0   │  0   │  0   │  0   │  1   │  1   │  0   │
p_ready │  -   │  -   │  -   │  -   │  -   │  1   │  1   │  1   │  -   │
        ├──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┤
 REGS (value at start of cycle)                                         │
active_r│  0   │  1←  │  1   │  1   │  1   │  1   │  1   │  1   │  0←  │
xfer_ctr│  0   │  0   │  0   │  0   │  1←  │  1   │  1   │  1   │  1   │
        ├──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┤
 THREAD ArIssuer (_t0_state)                                            │
 _t0_st │  0   │  0   │  1←  │  1   │  0←  │  0   │  0   │  0   │  0   │
        │defwhn│ S0:  │ S1:  │ S1:  │ S1:  │ S0:  │ S0:  │ S0:  │ S0:  │
        │fires │→S1   │wait  │→S0   │1<1=N │stay  │stay  │stay  │stay  │
        │      │      │      │ar_rdy│      │      │      │      │      │
        ├──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┤
 COMB t0│      │      │      │      │      │      │      │      │      │
ar_valid│  0   │  0   │  1   │  1   │  0   │  0   │  0   │  0   │  0   │
 ar_addr│  -   │  -   │1000h │1000h │  -   │  -   │  -   │  -   │  -   │
   ar_id│  -   │  -   │  0   │  0   │  -   │  -   │  -   │  -   │  -   │
  ar_len│  -   │  -   │  1   │  1   │  -   │  -   │  -   │  -   │  -   │
        ├──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┤
 THREAD RCollect_0 (_t1_state)                                          │
 _t1_st │  0   │  0   │  0   │  0   │  0   │  1←  │  1   │  1   │  0←  │
loop_cnt│  0   │  0←  │  0←  │  0←  │  0←  │  0   │  0   │  1←  │  2←  │
  tc[0] │  0   │  0   │  0   │  0   │  0   │  0   │  0   │  0   │  1←  │
        │defwhn│ S0:  │ S0:  │ S0:  │ S0:  │ S0:  │ S1:  │ S1:  │ S1:  │
        │fires │0<0=N │0<0=N │0<0=N │0<1=Y │→S1   │wait  │beat0 │beat1 │
        │      │      │      │      │      │      │      │→S1   │→S0   │
        ├──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┼──────┤
 COMB t1│      │      │      │      │      │      │      │      │      │
 r_ready│  0   │  0   │  0   │  0   │  0   │  0   │  1   │  1   │  1   │
p_valid │  0   │  0   │  0   │  0   │  0   │  0   │  0   │  1   │  1   │
        └──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────┘
         ↑defwh  ↑t0:S0  ↑t0:S1  ↑t0:S1    ↑t1:S0  ↑t1:S1  beat0   beat1
                  →S1    stalls  →S0+xfer   →S1             loop    exit
                         no rdy   accepted   unlocks
                                             RCollect
```

Legend: `←` = register update happens at this posedge.  `defwhn` = `default when` fires.
`p_ready` = push_ready.  `p_valid` = push_valid.

---

### Step 3 — Key observations

**ArIssuer and RCollect_0 are structurally decoupled.**
ArIssuer advances `xfer_ctr_r` (cycles 2→3→4→5); RCollect_0 checks `xfer_ctr_r`
to know when an AR has been issued for its ID (cycle 5: `0 < 1` becomes true).
There is no direct signal between the two threads — the counter is the implicit handoff.

**RCollect_0 cannot start until ArIssuer has issued xfer 0** (cycle 5 unlock).
But once it starts, the two threads are fully independent: ArIssuer is idle in S0
(no more xfers to issue) while RCollect_0 runs through cycles 6→7→8.

**The for-loop body collapses to one state (S1)** with `multi_transitions` encoding
the loop-back vs exit decision.  No separate LOOP_CHECK state exists — the counter
comparison is merged into the last body state's transition logic.

**`_t1_loop_cnt` is reset every cycle while in S0** (line `_t1_loop_cnt <= 0` fires
unconditionally when `_t1_state == 0`).  This is a deliberate simplification: the
counter is cheap and the always-reset ensures a clean start when S0 re-enters S1.

**all_done fires at cycle 9 posedge**: after cycle 8 sets `tc[0]=1`, the
combinational `total_complete == total_xfers_r` becomes true → the parent module's
`seq on clk` block sets `active_r <= 0`.

---

## Simulation Pipeline Note

The `lower_threads` pass currently runs for **both** `arch build` (SV codegen)
and `arch sim`.  This means SimCodegen never sees `ThreadBlock` AST nodes — it
only sees the generated `_ModuleName_threads` modules with plain state registers.

**Consequence**: the natural per-thread parallelism expressed in the source is
invisible to the simulator.  Each `thread once`/`thread` block could in principle
execute on its own OS thread or coroutine, synchronized at the clock edge barrier.
With `lower_threads` applied first, all threads are merged into a single
`eval_seq()` body and cannot be distributed to multiple cores.

**Desired architecture**: fork the pipeline so `lower_threads` is only applied
for `arch build`.  SimCodegen receives the pre-lowered AST and has two paths:

```
arch build:  parse → elaborate → lower_threads → Codegen (SV)
arch sim:    parse → elaborate ─────────────────→ SimCodegen
                                                   ├── single-core: lower inline
                                                   └── --parallel: threads as coroutines
```

This document — specifically the state partitioning rules and the lock/fork/for
sub-lowerers — defines the **semantic contract** that both paths must satisfy: at
every clock edge, both must produce identical values for every signal.  The
lowering algorithm is the ground truth for verifying that equivalence.

---

## Invariants (correctness properties)

1. **Single driver**: every register / comb signal is written by exactly one
   `if (_t{i}_state == ...)` block; shared(or) signals use OR-accumulation.
2. **No multi-driver**: all threads share one `always_ff` block; state regs are
   private per-thread.
3. **No latch**: every comb output defaults to 0 before the per-state enables.
4. **Reset completeness**: all state regs have an explicit reset via `RegReset::Inherit`
   pointing to the module's reset port.
5. **Lock deadlock freedom**: waits-for graph is a DAG by fixed-priority construction
   (proved above).
6. **Lock mutual exclusion**: holds for non-nested sequential lock blocks; nested
   lock blocks are rejected at compile time.
7. **Fork/join completeness**: the all-done product state always exists and transitions
   unconditionally to the next main-line state.
8. **For-loop termination**: the exit arm of the loop (`cnt >= end`) always has a
   higher-priority transition than the loop-back arm in `multi_transitions` ordering.
