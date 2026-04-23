# Plan: `tlm_method` pipelined mode (v2a)

*Author: session of 2026-04-22. Builds on `doc/plan_tlm_method.md` v1 which
shipped blocking mode only (PRs #74–#84).*

## Motivation

v1 `blocking` mode makes every call a round-trip stall: issue, wait for
response, advance. Any back-to-back calls are serialized end-to-end even
when the target could happily overlap multiple in-flight transactions.
`pipelined` mode unblocks this: the caller gets a `Future<T>` handle
*immediately* on issue, keeps issuing, and retrieves the actual value
later by awaiting the Future. The target can also overlap its work —
pop a request, start servicing, pop another, service that, push the
first response, etc. Responses arrive **in issue order** (the "FIFO
semantics" that make pipelined simpler than out-of-order).

This matches AXI in-order reads, in-order write responses, and the
general "issue N, collect N" pattern that's pervasive in memory and
interconnect interfaces.

## Scope

Pipelined mode only. Out-of-order (`Token<T, id_width: N>` — responses
tagged by ID, any arrival order) and burst (single AR, N data beats)
remain deferred to v2b/v2c.

## Semantics

### Initiator view

```
thread driver on clk rising, rst high
  let f0 = m.read(0x1000);      // non-blocking issue; returns Future<UInt<64>>
  let f1 = m.read(0x1004);      // overlaps with f0 in flight
  let f2 = m.read(0x1008);
  d0 <= await f0;               // blocks until f0's response arrives
  d1 <= await f1;               // already arrived? returns immediately. Else blocks.
  d2 <= await f2;
end thread driver
```

- `m.read(args)` on a `pipelined` method returns `Future<RetType>` immediately.
- `Future<T>` is an opaque handle — user can only pass it around, store it,
  or `await` it.
- `await f` inside a thread body: suspends the thread until `f` has a
  value, then yields the value.
- Issue is gated on `req_ready` (target slot available).
- Up to `MAX_OUTSTANDING` (channel-level const param) requests may be
  in flight concurrently; beyond that, issue stalls.
- Responses arrive in issue order — internal response FIFO dispatches
  them to the correct Future slot.

### Target view

Unchanged from v1: `thread port.method(args) ... return expr; end`
with the lowering's natural loop-back reaccepting the next request once
the current response is drained. Pipelined is strictly a *caller-side*
property — the target doesn't know or care whether the caller is
issuing serially or pipelined. The only target-side change is allowing
the next request to be accepted on the same cycle a response drains
(compositional throughput), which falls out of the existing target
FSM by construction.

### `Future<T>` is a handle, not a type a user names

`Future<T>` is inferred by the compiler from the `pipelined` method's
return type. The user never writes `let f: Future<UInt<64>> = ...` — the
`let` binding takes the inferred type. Parsing `await f` requires the
binding to be a Future handle (checked at typecheck).

Under the hood: a `Future<T>` handle is two signals — a small tag
(slot index into the initiator's outstanding-response FIFO) plus the
static T type for expression typing. The slot's payload is read when
the Future is `await`'d.

### Concurrency mode declaration

Grammar already parses `pipelined` (v1 rejects it at parse). The v1
`blocking` keyword gate becomes two-armed:

```
Mode := 'blocking' | 'pipelined'
```

Each keyword drives a separate elaboration path. `out_of_order` and
`burst` stay rejected at parse until v2b / v2c.

## Wire protocol

Same flattened signals as v1 blocking — the wire-level handshake is
identical. What changes is *who can have how many in flight*:

| Signal | v1 blocking | v2a pipelined |
|---|---|---|
| `<name>_req_valid`       | 1-outstanding | up to MAX_OUTSTANDING |
| `<name>_req_ready`       | high when target can accept | same |
| `<name>_<arg>`           | held valid until req_ready | same |
| `<name>_rsp_valid`       | target drives when response ready | same |
| `<name>_rsp_data`        | target drives with response value | same |
| `<name>_rsp_ready`       | initiator drives when able to accept | same |

No new physical signals. The "pipelining" is entirely in the initiator
(and, implicitly, the target) internal state.

## Knobs

Channel-level const param:

```
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: pipelined
    param MAX_OUTSTANDING: const = 8;
end bus Mem
```

Grammar addition: optional trailing params after the mode keyword.

`MAX_OUTSTANDING` sizes the initiator's response FIFO. Default 4.
Target-side buffering is the target's own concern (e.g., the target
may have its own internal FIFO or pipeline; not related to this param).

## Initiator-side lowering (where the real work is)

`tlm_method read(addr: UInt<32>) -> UInt<64>: pipelined` + three calls
in a thread body lowers roughly to:

```
// Response FIFO (depth MAX_OUTSTANDING):
reg  _tlm_init_driver_rsp_fifo: Vec<UInt<64>, MAX_OUTSTANDING> reset rst => 0;
reg  _tlm_init_driver_rsp_head: UInt<clog2(MAX_OUTSTANDING)> reset rst => 0;
reg  _tlm_init_driver_rsp_tail: UInt<clog2(MAX_OUTSTANDING)> reset rst => 0;
reg  _tlm_init_driver_rsp_occ:  UInt<clog2(MAX_OUTSTANDING+1)> reset rst => 0;

// Per-Future metadata: which FIFO slot, resolved-yet bit.
// Since responses are in-order, slot_idx === issue_seq_no mod MAX_OUTSTANDING.
// Resolved means: rsp_tail has advanced past the Future's slot.

// Issue FSM state — walks user stmts, stalling on max-outstanding.
// await FSM state — stalls until the awaited Future's slot is resolved.
```

Concretely:

- **Issue**: on `let f_k = m.method(args);`, enter an ISSUE state that
  drives `req_valid` + args, waits for `req_ready && (rsp_occ < MAX_OUTSTANDING)`,
  then advances. Stores `rsp_tail` at the issue cycle as the Future's
  slot index.
- **Response collection**: a background state or comb block observes
  `rsp_valid`, writes `rsp_data` into `_rsp_fifo[rsp_tail]`, bumps
  `rsp_tail`, bumps `rsp_occ`. Drives `rsp_ready = 1` whenever the
  FIFO has space.
- **Await**: `<dest> <= await f_k;` enters a WAIT state that stalls until
  `f_k.slot_idx < rsp_tail`, then reads `_rsp_fifo[f_k.slot_idx]` and
  pops (increments rsp_head, decrements rsp_occ).

Because responses are in-order, awaits on Futures *must also* be done
in-order (if the user awaits f1 before f0, the response data for f0
is consumed when f1's await fires, which is wrong). Two handling
choices:

1. **Strict in-order await**: typecheck rejects out-of-order await.
2. **Buffered await**: compiler tracks per-Future "consumed" bits and
   pops only when earlier Futures are consumed. More expensive.

v2a choice: **strict in-order await**. Users who need reordered
awaits graduate to out_of_order mode (v2b).

### Typecheck rule

Futures bound by `let f_i = m.read(...)` in a thread body must be
awaited in the same lexical order they were issued. The compiler
tracks issue order per-channel and rejects `await f_earlier` after
`await f_later`.

### Multiple threads sharing a method

v1 rejected multiple threads in the same module touching the same
method (implicit serialization via single-thread issue). v2a extends
this: one thread per method call site. Within that thread, up to
MAX_OUTSTANDING pipelined calls. Two threads still can't share.

Cross-thread sharing with arbitration is a future extension; the
thread-per-method constraint keeps the initiator FSM deterministic
in v2a.

## Target-side lowering

Unchanged from v1. The target FSM already:
1. Accepts req_valid → latches args → advances.
2. Runs user body.
3. `return expr;` → drives rsp_valid + rsp_data → waits for rsp_ready
   → loops back to entry.

Pipelined mode works through this FSM without any modification.
Multiple in-flight is a function of: how fast the target can
re-complete its body loop vs the initiator's issue rate.

> **Stretch**: targets could additionally declare `pipelined` on their
> implementation to say "I can have up to N concurrent user-body
> instances, each serving a separate request". That requires a
> fundamentally different target-side FSM (per-request state table).
> Deferred past v2a.

## v2a roadmap

Mirror the v1 PR split:

### PR-tlm-p0: this plan doc (no code)

Lock design decisions before coding.

### PR-tlm-p1: grammar extension

- Parser: `pipelined` mode keyword accepted (not just `blocking`).
- AST: optional trailing `param` block after the mode keyword.
- Typecheck: reject any initiator call site on a pipelined method
  until PR-tlm-p3/p4 land (scaffolding pattern).

### PR-tlm-p2: `Future<T>` type + `await` grammar

- AST: `TypeExpr::Future(Box<TypeExpr>)` variant.
- AST: `ThreadStmt::Let(Ident, Expr)` or reuse the existing `let`
  shape inside threads (decision: check whether threads already
  support `let` bindings — if not, add it).
- Parser: `let f = m.method(args);` inside a thread body.
- Parser: `await <expr>` as an Expr / ThreadStmt.
- Typecheck: Future<T> propagation; `await f` only valid in thread
  body; in-order-await check.
- Not wired to any lowering yet — rejected as "pipelined lowering
  not yet implemented" with a targeted message.

### PR-tlm-p3: initiator-side pipelined lowering

- Replace blocking-only initiator pass with one that handles both
  modes. For pipelined calls:
  - Emit response FIFO (head/tail/occ/buf).
  - Walk user body: each `let f = m.method(args);` becomes an ISSUE
    state (with max-outstanding gate). Each `await f` becomes a WAIT
    state.
  - Ambient background state / comb collects responses.
- Lift the PR-tlm-p1 scaffolding reject.

### PR-tlm-p4: docs pass + canonical pipelined test

Spec §18d addition with pipelined semantics. Reference card entry.
End-to-end test with a pipelined initiator issuing 3 calls and
awaiting in order.

### Skipped (for v2a, like v1 skipped Tier-2 SVA)

- Tier-2 SVA for pipelined invariants (`req_valid |-> rsp_occ <
  MAX_OUTSTANDING`, response ordering, etc.).
- Cross-thread method sharing.
- Out-of-order / burst modes (v2b / v2c).

## Open questions (need user sign-off before PR-tlm-p1)

1. **Future type naming**: `Future<T>` as in the bus_spec_section
   notes, or something more ARCH-flavored like `Pending<T>`? Leaning
   **Future<T>** for consistency with the existing spec draft + SystemC
   precedent.

2. **`await` syntax: expression or statement?**
   - Expression form: `d <= await f;` — await inside RHS.
   - Statement form: `await f -> d;` — dedicated statement syntax.
   
   Expression form is more natural. Typecheck enforces await only
   appears as top-level RHS of a SeqAssign inside a thread (not
   composed in expressions). **Leaning expression form.**

3. **In-order-await enforcement**: strict compile-time check, OR
   buffered runtime handling?
   - Strict: simpler compiler, clearer user contract, zero runtime cost.
   - Buffered: more flexible, costs per-slot consumed-bits + logic.
   
   **Leaning strict** — the out_of_order mode exists for users who
   need reordered awaits. Pipelined stays cheap and deterministic.

4. **MAX_OUTSTANDING default**: 4 or 8? Plan draft says 4. AXI in-order
   lanes commonly handle 8–16. **Leaning 4** as the conservative
   default; users explicitly bump for higher concurrency.

5. **`let f` lifetime**: does the Future handle need to be awaited
   exactly once, or can it be dropped (issue a request you never read)?
   - Exactly-once: straightforward invariant, catches silent deadlocks.
   - Drop-allowed: user can intentionally issue side-effecting ops
     whose response doesn't matter.
   
   Side-effecting pipelined issue is rare. **Leaning exactly-once**
   in v2a; users who want fire-and-forget issue a blocking method
   whose ret they ignore.

6. **Channel param syntax**: `tlm_method read(args) -> T: pipelined
   param MAX_OUTSTANDING = 8;` (trailing `param` clause), OR a block
   form `tlm_method read ... end tlm_method read`? Single-line form
   is terser but feels cramped. Block form is verbose but composes
   with multiple params if we add more later. **Leaning trailing
   single-line `param` clause** — grammar: `Mode Params? ';'` where
   `Params := 'param' Ident '=' Expr (',' 'param' Ident '=' Expr)*`.

7. **Multiple methods in the same thread**: user issues
   `let a = m.read(x);` and `let b = m.write(y, z);` in the same
   thread. These share the same thread's issue FSM but target
   different methods → different response FIFOs. v2a: allow,
   each method gets its own FIFO + per-method issue state. Rejection
   semantics: no typecheck restriction beyond single-thread-per-method.

All answers default-leaning. Confirm "all defaults" to proceed with
PR-tlm-p1, or redirect on any specific question.
