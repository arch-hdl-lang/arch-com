# Plan: `tlm_method` pipelined via reentrant `thread` (v2a)

*Author: session of 2026-04-22. Supersedes the earlier Future<T>/await
sketch. Builds on `doc/plan_tlm_method.md` v1 (PRs #74–#84).*

## The core idea

v1 blocking mode runs one TLM call at a time per thread. To pipeline,
**don't invent new types or keywords** — make the existing `thread`
construct **reentrant**. A reentrant thread is a single body
definition whose invocations can overlap: a new invocation starts as
soon as the previous one hits its first `wait` (or blocking TLM call),
even if the previous invocation hasn't finished.

The user writes ordinary blocking call code. The compiler synthesizes
up to N concurrent instances of the FSM, sharing the thread body but
each with its own per-instance state (args, locals, program counter).
Pipelining emerges naturally: instance 0 issues → instance 1 issues →
... → responses come back in order and the compiler routes each to
the instance that issued it.

```
thread driver on clk rising, rst high reentrant max 8
  let addr = next_addr;
  next_addr <= (next_addr + 4).trunc<32>();    // allow next instance to proceed
  d <= m.read(addr);                            // blocks THIS instance
end thread driver
```

This is the same code the user would have written for a single-threaded
blocking driver — just with `reentrant max 8` added. Up to 8
invocations are in flight concurrently, each blocked on its own
`m.read(...)` return.

## Why this is the right model

| Concern | Future<T>/await version | Reentrant thread version |
|---|---|---|
| User-visible new syntax | `Future<T>` + `await` keyword | `reentrant max N` on the thread |
| New AST types | Yes (`TypeExpr::Future`) | No |
| User code reads as | Explicit issue + defer + await | Straight-line blocking — what the user means |
| Multiple methods per thread | Needs multiplexed Future handling | Trivially works (thread body blocks on each) |
| Cross-thread coordination | `await_all` / `await_any` primitives | `lock` for resource conflicts (already exists) |
| Non-TLM pipelining (e.g. RAM read latency) | N/A — TLM-specific | Works uniformly for any multi-cycle op |

The reentrant model also **generalizes past TLM**: any thread that
waits on any multi-cycle operation (RAM read, FIFO drain, handshake)
gets the same "let the next instance start while I wait" semantics
for free.

## Semantics

### Reentrancy clause

```
ThreadDecl := 'thread' ('once')? Ident?
              ('.' Ident '(' ArgList ')')?                // TLM target binding (v1)
              'on' Ident (rising|falling) ',' Ident (high|low)
              ('reentrant' ('max' IntLit)?)?              // new
              ('default' 'when' Expr body 'end' 'default')?
              body
              'end' 'thread' Ident?
```

- `reentrant` alone: unbounded concurrent instances (synthesizes as
  many as there are live yield points in the body).
- `reentrant max N`: at most N concurrent instances. Issuing a new
  instance stalls when N are already live.
- Absence (default): current v1 semantics — exactly one instance, new
  invocations wait for the previous to complete.

### What an "instance" is

Each instance has:
- **Its own program counter** — current FSM state index.
- **Its own args** if the thread is a TLM target (already latched per-call in v1).
- **Its own locals** — `let` bindings inside the body are per-instance.

Each instance **shares**:
- **Module regs** declared outside the thread — readable and writable
  by any instance.
- **Thread-scoped regs** declared inside the thread but NOT per-`let`
  — readable and writable by any instance (race-safe via seq semantics;
  two instances writing the same cycle = last-writer-wins per-port
  arbitration, same as cross-thread races today).

### Yield points

An "instance yields" and frees up for the next instance to start at:
- Any `wait until` / `wait N cycle`.
- Any blocking TLM call (v1 blocking methods).
- `do ... until` waiting for its condition.

Ordering of invocations:
- **In-flight instances advance independently** — each on its own
  FSM state index, each served by the next clock edge.
- **New invocations** start when the entry state has capacity
  (instance count < max).

### Response routing (for TLM)

When a reentrant thread contains `x <= m.method(...);` and multiple
instances are in-flight, they share the one bus port. The compiler:
1. Arbitrates issue: at most one instance drives `req_valid` per
   cycle. Round-robin priority among ready-to-issue instances.
2. Records a routing FIFO: "slot k on the bus was issued by instance
   I." In-order responses fill in-order slots.
3. Routes each response back to its issuing instance — the instance
   resumes from its WAIT state with `rsp_data` captured into its
   per-instance destination reg.

This is *exactly* the v2a pipelined machinery but driven by the
thread model instead of a separate Future abstraction.

### Interaction with `lock`

`lock RESOURCE ... end lock RESOURCE` inside a thread body already
serializes access to a shared resource across threads. With reentrancy,
the same rule applies across **instances**: only one instance holds a
given lock at a time. A reentrant thread doing lock-heavy work will
serialize internally — that's the user's choice.

## Grammar and AST

### Keyword additions

- `reentrant` — contextual keyword parsed after the clock/reset clause.
- `max N` — optional bound, defaults to a compile-time const (say 4)
  when omitted. `max <ident>` allows const-param references.

### AST additions

```rust
pub struct ThreadBlock {
    // ... existing fields
    /// None = not reentrant (v1 semantics).
    /// Some(None) = reentrant with default max instances.
    /// Some(Some(Expr)) = reentrant max N (Expr must reduce to const).
    pub reentrant: Option<Option<Expr>>,
}
```

Parser reads the optional clause after reset clause; typecheck
validates the max expression is const-reducible.

## Lowering

### Internal model: N parallel state FSMs

A reentrant thread with `max N` lowers to:
1. **N state regs**: `_thread_X_state_0 ... _thread_X_state_{N-1}`.
2. **N per-instance reg banks**: each `let` binding and arg latch
   replicated N times with `_inst_<i>` suffix.
3. **Shared reg banks**: module-scope regs and thread-scope regs NOT
   introduced by `let` stay single-copy.
4. **Instance scheduler**: round-robin pointer picking the next
   free-slot to start a new invocation.
5. **Issue arbiter** (for TLM calls): round-robin over instances
   ready to drive `req_valid`; response-routing FIFO records
   (slot, instance) pairs.

Each instance's FSM is a copy of the v1 lowered FSM — entry state,
wait states, respond states — with state references rewritten to the
per-instance state reg.

### v2a scope

- Support `reentrant` on any thread, including TLM target and
  initiator-using threads.
- Implement the issue arbiter + response routing for the initiator
  case.
- Single-bus sharing: multiple instances on one method go through the
  arbiter.
- Defer: multiple reentrant threads sharing the same method (thread-
  thread arbitration); `reentrant` on TLM target threads with
  multiple concurrent request services (needs per-request body state
  replication on the target side — bigger refactor).

## v2a PR roadmap

### PR-tlm-p1: `reentrant` grammar + AST

- Lexer: `reentrant`, `max` as contextual keywords.
- AST: `ThreadBlock.reentrant: Option<Option<Expr>>`.
- Parser: accept `... rst high reentrant`, `... rst high reentrant max 8`.
- Typecheck: const-reducibility of the max expression.
- Scaffolding reject: any `reentrant` thread fails typecheck with
  "reentrant lowering not yet implemented" until PR-tlm-p2/p3 land.

### PR-tlm-p2: reentrant lowering for NON-TLM threads

- Start with the simpler case: reentrant thread without TLM calls
  (e.g. a thread that `wait N cycle;`s between operations, or issues
  to a RAM with latency). Validates the N-instance FSM cloning +
  per-instance locals without the TLM-routing complexity.
- Module-body regs still single-copy; per-instance locals get the
  `_inst_<i>` rewrite.
- No issue arbiter yet (none needed without TLM calls).

### PR-tlm-p3: reentrant lowering with TLM initiator calls

- Build on PR-tlm-p2. Add the issue arbiter + response-routing FIFO.
- Each instance's TLM call lowers to an issue state (gated by arbiter
  grant) + wait state (gated by its slot's response arriving).
- Response FIFO stores (slot_idx, instance_idx) on issue; on response,
  dispatches `rsp_data` to the instance's destination reg.

### PR-tlm-p4: docs + canonical pipelined test

- Spec §7 (thread) gets a `reentrant` subsection.
- `tlm_method` spec gets a note: "for pipelining, mark the issuing
  thread `reentrant`."
- Reference card entry.
- Canonical test: Mem initiator with a reentrant driver thread
  issuing N reads in sequence; verify SV has the N-state FSMs +
  response routing.

### Deferred to later

- Reentrant TLM target threads (multiple concurrent request services).
- Cross-thread arbitration on a single method.
- Tier-2 SVA for reentrant invariants.
- `reentrant` on a `thread` that contains `fork`/`join` or complex
  control flow.

## Open questions

1. **Default max count** when `reentrant` is unbounded. Options:
   - (a) Reject unbounded reentrant in v2a — user must specify `max N`.
   - (b) Default to 4 and emit a warning.
   - (c) Unbounded = up to the outstanding count the bus can support
     (inferred from connected method's MAX_OUTSTANDING param).
   
   **Leaning (a)** — explicit is better for hardware resource sizing.

2. **Per-instance state of thread-scope regs.** When a user writes
   `reg counter: UInt<8> reset rst => 0;` *inside* a reentrant thread,
   is `counter` per-instance or shared?
   - Per-instance: more like "procedure locals."
   - Shared: more like "thread-owned regs, instance access is racy."
   
   **Leaning shared** — matches existing `reg` semantics in threads.
   Users who want per-instance locals use `let` (which binds only for
   the current instance's cycle-path). Could revisit if this trips
   users up.

3. **`let` inside a reentrant thread body.** Today `let` inside a
   thread is block-scoped. For reentrant, `let x = expr;` needs a
   per-instance shadow reg if `x` is referenced across a wait point.
   Single-cycle `let` needs no shadow.
   
   Proposal: compiler lifts `let`-bindings crossing wait points into
   auto-generated per-instance regs. Transparent to the user.

4. **Ordering guarantees on issue.** If instance 0 and instance 1
   both want to issue on the same cycle, which wins? Proposal:
   **round-robin priority** — deterministic, fair. Instance-id
   allocated in spawn order.

5. **Max concurrency vs bus MAX_OUTSTANDING.** The thread's `max N`
   and the method's implicit outstanding budget may mismatch. If the
   thread declares `max 8` but the bus wire protocol only tracks
   which response is in flight implicitly... actually since responses
   are in-order and the arbiter sizes its routing FIFO per-instance
   count, `max N` of the thread dictates everything. No separate
   MAX_OUTSTANDING channel param needed; drop that from the original
   plan.

6. **Reset semantics.** On reset, all instances drop to their entry
   state simultaneously. The in-flight bus transactions are
   abandoned (req_valid goes low, rsp_ready goes low). This matches
   existing thread reset behavior — no new rules.

7. **Observability.** `arch sim --debug-fsm` should print transitions
   per instance: `[cycle][Mod._thread_driver_inst_3] S1 -> S2 (rsp_valid)`.
   Nice to have in v2a, not blocking.

All default-leaning. Confirm and I'll start PR-tlm-p1 (grammar).

## Migration path

Since `reentrant` is a new opt-in keyword, zero existing code changes.
Users get pipelining by adding `reentrant max N` to any thread that
currently does blocking calls in a loop. Fold the earlier `Future<T>`
plan into this doc — Future/await is no longer pursued.
