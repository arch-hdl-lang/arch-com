# Plan: `tlm_method` in-order thread cohorts

> **Current direction (2026-04-25).** Do not add a user-visible
> `Future<T>`/`await` API and do not add a separate `tlm_method:
> pipelined` mode for in-order request/response behavior.
>
> The viable path is ordinary `thread` concurrency: users express a
> finite set of workers with generated threads or `fork ... join`, and
> the compiler lowers the direct blocking calls to a shared request
> arbiter plus an issue-order response router.
>
> The first out-of-order slice is a tagged bus protocol extension using
> the same fork/join or generated-thread user surface, not `Future<T>`.
>
> `reentrant` grammar on ThreadBlock (merged in PR #86) stays as
> dead-but-parsed code; removal or repurposing is a future cleanup.
>
> Historical design below kept for context — the reentrant thread
> and `Future<T>/await` sketches led to the current framing. Treat
> `Future<T>/await` as a rejected design direction, not a dormant
> roadmap item.

> **Implemented slice in this branch.** A narrow version of the
> historical multi-thread idea is now supported: allow a finite cohort
> of ordinary workers to issue blocking calls to the same `tlm_method`,
> and synthesize only an in-order request arbiter plus response router.
> This is not a new `tlm_method: pipelined` mode and not the shelved
> `implement` pool. It is a compiler lowering of these shapes:
>
> ```arch
> generate_for i in 0..N-1
>   thread worker_i on clk rising, rst high
>     d[i] <= m.read(addr[i]);
>   end thread worker_i
> end generate_for
>
> thread workers on clk rising, rst high
>   fork
>     d[0] <= m.read(addr[0]);
>   and
>     d[1] <= m.read(addr[1]);
>   join
> end thread workers
> ```
>
> The in-order target protocol remains the v1 blocking req/rsp handshake
> and is assumed to return responses in request order. `out_of_order
> tags N` adds req/rsp tag wires and routes by response tag. AXI-style
> separate AR/R channels and bursts remain outside this feature; those
> belong in explicit protocol threads or a future beat-stream extension.

---

# (Historical) Plan: `tlm_method` pipelined via multi-thread arbitration (v2a)

*Author: session of 2026-04-22. Supersedes the earlier rejected
`Future<T>`/`await` sketch and the `reentrant thread` sketch. Builds on
`doc/plan_tlm_method.md` v1 (PRs #74–#84).*

## The final model

The two earlier pivots (`Future<T>` / `await`, then `reentrant max N`)
both tried to express pipelining inside a single thread. Both hit
semantic problems and are no longer considered viable implementation
paths:

- `Future<T>`/`await` invented a new type + keyword when the intent —
  "let the next call issue while this one waits" — is already what
  parallel threads express. It also needs lifetime tracking, await
  placement rules, completion storage, and cancellation/reset semantics
  that do not fit the current thread lowering model.
- `reentrant` is ambiguous about shared register writes: if N
  instances all `d <= m.read(addr);`, who wins the write? Needs a
  per-instance identifier the user can index by.

The cleanest model sidesteps both: **users who want pipelining write N
parallel threads** via the existing `generate for` construct, each
doing ordinary blocking TLM calls. The compiler's job shrinks to
arbitrating the shared bus.

```
generate for i in 0..4
  thread worker_i on clk rising, rst high
    d[i] <= m.read(addr[i]);    // each worker writes its own slot
  end thread worker_i
end generate for i
```

After elaboration, four threads exist. Each independently blocks on
its own `m.read(...)`. The compiler inserts a round-robin issue
arbiter on `m.read_req_valid/req_ready` and a response-routing FIFO to
dispatch each in-order response back to the thread that issued it.

Zero new user-facing syntax. Everything composes with what ARCH has
today: `generate for`, `thread`, `tlm_method blocking`, per-index
indexed regs/ports.

## Why this is the right model

| Concern | Future/await | reentrant | multi-thread via generate_for |
|---|---|---|---|
| New user syntax | rejected: `Future<T>`, `await`, maybe `await_all` | `reentrant max N`, `_instance_id`? | none |
| Per-instance state | rejected: future slot/lifetime tracking | ambiguous — new grammar needed | naturally per-thread |
| Per-instance writes | rejected: future targets still need ownership rules | multi-driver unless indexed | each thread writes `d[i]` |
| Per-instance args | `m.read(args_k)` | must index `addr[_instance_id]` | each thread has its own `addr[i]` |
| Cross-instance coordination | `await_all` primitive | shared regs + ???? | ordinary module regs |
| Compile-time N | locked by Future count | `max N` constant | `generate for i in 0..N` |
| Fires across multiple methods | arbitrary | complex per-method FIFOs | naturally — each thread blocks per call |

The whole v2a feature becomes: **teach the compiler to arbitrate a
shared `tlm_method` across multiple threads in the same module**.

## Semantics

### Multi-thread sharing of a `tlm_method`

In v1, two threads in the same module touching the same `(port,
method)` pair was a compile error (single-driver conflict). v2a lifts
this: the compiler detects N > 1 threads driving `<port>_<method>_*`
signals via TLM call lowering, and synthesizes:

1. **Issue arbiter** (round-robin grant across the N threads'
   issue-state "want-to-issue" signals).
2. **Issue-order FIFO** recording `(slot_k, thread_i)` on each
   granted issue; sized by the thread count N.
3. **Response router**: on `rsp_valid`, pop the FIFO head to learn
   which thread issued slot_k; route `rsp_data` to that thread's
   capture reg and fire its response-ready transition.

The caller-side thread FSM loses the direct `req_valid` drive;
instead it drives a per-thread `_tlm_want_<port>_<method>_<t_id>`
and waits for its own `_tlm_grant_<port>_<method>_<t_id>` before
advancing from the issue state.

Single-thread case (N = 1) keeps the existing v1 lowering untouched
— no arbiter emitted, no routing FIFO. The arbiter is strictly
pay-for-what-you-use.

### Arbitration policy

- **Round-robin** with a priority rotating pointer. Fair across
  threads, no starvation.
- Policy is fixed in v2a. If users need priority / QoS, they compose
  with an explicit `arbiter` construct outside the TLM layer.

### Response ordering

Target returns responses in issue order. The issue-order FIFO
preserves that mapping. If responses arrive while a target-side
reentrancy enables out-of-order completion (future v2b via
`out_of_order` mode), the routing logic changes — that's a v2b
concern.

### Per-thread state

Each thread keeps its existing lowered FSM from v1 (issue state +
wait-response state + optional compute states). The only delta:
- Issue state drives `_tlm_want_*` instead of `_req_valid`.
- Issue state transitions when `_tlm_grant_*` fires (not when
  `_req_ready` is high).
- Wait-response state reads from per-thread `_tlm_rsp_data_<t_id>`
  capture reg instead of the shared bus signal.

## Lowering (compiler-side machinery)

The `lower_tlm_initiator_calls` pass gains an extra step at the top:
group threads by `(port, method)` and, for each group with > 1 thread,
synthesize the arbiter + routing module-level items.

### Module-level synthesized items

Per shared `(port, method)` with N threads:

```
// Issue arbiter: round-robin grant.
reg  _arb_priority_<port>_<method>: UInt<clog2(N)> reset rst => 0;
let  _grant_<port>_<method>: Vec<Bool, N> = <round-robin comb logic>;

// Each thread's arbiter interface:
let  _tlm_want_<port>_<method>_<i>: Bool = <driven by thread i's issue state>;
let  _tlm_grant_<port>_<method>_<i>: Bool = _grant_<port>_<method>[i];

// Bus-side drives: muxed from the granted thread's pending drive.
comb
  m.<method>_req_valid = <OR of wants AND grants>;
  m.<method>_<arg>     = <mux of granted thread's latched args>;
end comb

// Issue-order FIFO: depth N, entries = thread index.
reg  _rsp_fifo_<port>_<method>: Vec<UInt<clog2(N)>, N> reset rst => 0;
reg  _rsp_fifo_head/_tail/_occ: ...

// On each granted issue, push thread index to tail.
// On each rsp_valid, pop head to learn which thread to route to.

// Per-thread response capture:
reg  _tlm_rsp_data_<t_id>: UInt<ret_width> reset rst => 0;
comb
  // When rsp_valid and fifo head matches this thread, capture.
end comb
```

### Scope for v2a

- Multiple threads sharing one `(port, method)` in the same module:
  full arbitration + routing.
- Multiple threads each touching DIFFERENT methods on the same bus:
  handled per-method independently.
- Response capture regs sized from the declared ret type. Void
  methods (no ret) just signal completion via a per-thread done bit.

### Deferred

- Tier-2 SVA for arbitration invariants.
- Cross-module multi-initiator (requires arbiter at the target side,
  out of scope for v2a).
- Out-of-order response routing (v2b).

---

# Current Implementation: In-Order Thread-Cohort Mapping

This section is the viable subset of the historical v2a plan after
reviewing the implemented `thread` construct and lowering path.

## Positioning

The feature is a compile-time convenience for **simple in-order
request/response methods**. It does not try to make TLM a replacement
for hand-written high-performance bus protocols.

Use it when:

- The source already has a `generate_for` group of worker threads, a
  small set of named worker threads, or one `fork ... join` block whose
  branches are direct TLM calls.
- Each worker can have at most one outstanding call to a given method.
- The target returns responses in the same order as accepted requests.
- Each worker writes a distinct destination, usually indexed by the
  generate variable.

Do not use it for:

- AXI read/write channels with independent address/data/response timing.
- ID-tagged or out-of-order response protocols.
- Burst protocols where one request produces multiple beats.
- Shared work counters unless the source also protects allocation with
  an explicit lock or uses static round-robin work ownership.

## User Surface

No new syntax is required:

```arch
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem

module Readers
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port m: initiator Mem;

  reg addr: Vec<UInt<32>, 4> reset rst => 0;
  reg data: Vec<UInt<64>, 4> reset rst => 0;

generate_for i in 0..3
  thread worker_i on clk rising, rst high
    data[i] <= m.read(addr[i]);
  end thread worker_i
end generate_for

thread workers on clk rising, rst high
  fork
    data[0] <= m.read(addr[0]);
  and
    data[1] <= m.read(addr[1]);
  join
end thread workers
end module Readers
```

After normal elaboration, the `generate_for` is unrolled into concrete
threads. The compiler groups threads that call the same `(port, method)`
pair and lowers that group as one in-order call pool.

Single-thread use keeps the existing v1 lowering unchanged.

## Required Restrictions

For the first implementation, reject anything outside this shape:

1. **Direct RHS only**: the call must appear as `dst <= m.method(args);`.
   Nested expressions such as `dst <= m.read(a) + 1;` stay rejected.
2. **One method call per statement**.
3. **One outstanding call per worker per method**. A worker cannot issue
   a second call to the same method until its prior response is routed
   back.
4. **In-order responses only**. The response FIFO records issue order;
   no response ID exists in the bus protocol.
5. **Thread cohort must be finite after elaboration**. This naturally
  follows from the current `generate_for` thread rule: threads are
   always unrolled before thread lowering.
6. **Distinct destinations are recommended and should be diagnosed when
   obviously violated**. `data[i]` is fine after generate substitution;
   two workers writing the same scalar should remain a multi-driver
   error.
7. **No `implement` clause required**. The explicit `implement` pool is
   still shelved; this feature is driven by ordinary thread calls.

## Where It Is Implemented

The first implementation lives in `lower_tlm_initiator_calls`, before
generic thread lowering. It intentionally recognizes only a narrow,
structurally obvious shape and replaces the worker threads with
module-level `RegDecl` / `RegBlock` / `CombBlock` items:

1. Scan concrete `ThreadBlock`s after `generate_for` expansion.
2. Recognize either a one-statement thread `dst <= m.method(args);` or
   one `fork ... join` whose branches each contain that direct call.
3. Group call sites by `(port, method)`.
4. For groups with one caller, keep the existing v1 inline lowering.
5. For groups with N > 1, emit a shared in-order pool.

This keeps the shipped slice small. A later richer implementation could
move closer to generic thread lowering if it needs arbitrary statements
around the call.

## Lowering Sketch

For each shared `(port, method)` with N caller threads:

```text
per thread i:
  _tlm_pool_<p>_<m>_t<i>_state  // 0 = ready to issue, 1 = waiting response

shared:
  fixed-priority arbiter over ready-to-issue workers
  request arg mux from granted thread
  issue-order FIFO of thread indices
  response router using FIFO head
```

Issue state behavior:

```text
want_i = (_tlm_pool_<p>_<m>_t<i>_state == 0)
method_req_valid = any grant
method_arg = mux(granted_i, arg_i)

if grant_i && method_req_ready:
  push i into issue-order FIFO
  _tlm_pool_<p>_<m>_t<i>_state <= 1
```

Wait state behavior:

```text
method_rsp_ready = FIFO nonempty

if method_rsp_valid && FIFO.head == i:
  dst_i <= method_rsp_data
  pop FIFO
  _tlm_pool_<p>_<m>_t<i>_state <= 0
```

Void-return methods omit the destination capture and still use the
per-worker state bit for completion.

## Ordering And Backpressure

The issue-order FIFO is the contract that makes this feature in-order:

- Every accepted request pushes the issuing thread index.
- Every accepted response pops one index.
- The response is delivered to the thread at the FIFO head.

The FIFO depth can be N for the first implementation, because each of
the N workers may hold at most one outstanding call. A later extension
could size it from an annotation if a single worker is allowed multiple
outstanding calls, but that reopens explicit token/lifetime semantics
and should stay out of scope unless a concrete design exists.

## Diagnostics

Prefer targeted diagnostics over falling through to multi-driver errors:

- More than one thread calls the same TLM method, but the method is used
  in unsupported control flow.
- A method group needs in-order routing but the call is not a direct RHS.
- The call arg count does not match the `tlm_method` declaration.
- The destination is an obvious scalar shared by multiple workers.
- A worker calls the same method twice without an intervening response
  state that the compiler can prove.

## Tests

Minimum regression set:

1. `generate_for i in 0..3` workers reading `data[i] <= m.read(addr[i])`
   emits one shared request drive and per-thread response capture.
2. Single worker still matches v1 output shape or behavior.
3. Two non-generated named threads sharing one method lower through the
   same in-order path, or are rejected intentionally if the first scope
   is generate-only.
4. Bad nested RHS stays rejected.
5. Wrong arg count is rejected.
6. Void method works with response completion but no data capture.
7. Verilator lint on the generated SV.
8. `fork ... join` with direct call branches lowers through the same
   pool.
9. `compile_to_sim_h` confirms the lowered reg/comb/seq form remains
   visible to sim codegen.

## Relationship To Existing Thread Idioms

This feature overlaps with `lock RESOURCE` only for simple request-side
serialization. It adds response routing for in-order TLM methods. It
does not replace explicit thread protocol code where the protocol itself
has useful structure, such as separate issue and collection threads,
ID-tagged responses, or burst beat loops.

## Current Out-Of-Order Slice

Out-of-order support keeps the same user-facing worker syntax and
changes only the method protocol/lowering contract. The compiler assigns
one small tag per worker/branch, sends that tag with the request, and
routes a response by `rsp_tag` instead of FIFO head.

Sketch:

```arch
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: out_of_order tags 4;
end bus Mem
```

Implemented lowering:

- Request channel gains `<method>_req_tag`.
- Response channel gains `<method>_rsp_tag`.
- Per-worker state is still "ready" / "waiting".
- Accepted requests mark the worker waiting by tag.
- Responses compare `rsp_tag` and complete the matching worker.
- Target-side lowering latches `req_tag` and echoes it as `rsp_tag`.

This replaces any need for `Future<T>`. The branch or generated worker
is the outstanding operation handle.

## PR roadmap (revised)

- ~~PR-tlm-p1: `reentrant` grammar~~ — merged but semantically unused
  after this pivot. Leave the grammar in place as dead-but-parsed; a
  follow-up cleanup can remove it. The `reentrant` scaffolding reject
  in `lower_threads` stays as the failure mode.
- **PR-tlm-p3**: multi-thread TLM arbitration (this plan). Teaches
  `lower_tlm_initiator_calls` to group threads by `(port, method)`
  and emit the arbiter + issue FIFO + response routing when N > 1.
- **PR-tlm-p4**: docs + canonical pipelined test (N-way `generate for`
  driver, verify SV compiles and the arbiter + routing appear).

## Open questions (need user sign-off)

1. **`reentrant` grammar cleanup**: keep as dead code (no-op parse,
   rejected at lower_threads), or remove in a cleanup PR? **Leaning
   keep** — cheap, reserved for a future "lock per body" use case.

2. **Arbitration policy choice surface**: for v2a, fixed round-robin.
   If users need priority later, add a per-bus-port annotation
   (e.g. `initiator Mem with arb: priority;`). **v2a locks to
   round-robin**; annotation is a future extension.

3. **Response capture reg** when multiple threads share one method:
   currently each thread gets its own `_tlm_rsp_data_<t_id>` reg.
   Alternative: a single shared reg + per-thread "consume this
   cycle" flag. Latter is smaller area; former is simpler
   semantics. **Leaning per-thread** for v2a.

4. **Multi-method per thread**: a single thread calls `m.read()`
   then `m.write()`. Does the arbiter need to track per-method
   independently, or does the thread hold exclusive access during
   its transition? **Leaning per-method independent** — threads
   only contend for the specific (port, method) they're on the
   issue state of.

5. **Void method capture**: methods without a return type don't need
   a per-thread capture reg, just a per-thread done-bit. Confirm the
   arbiter FIFO still tracks them (for response-ready routing).
   **Leaning yes** — uniformity wins.

All default-leaning. Confirm and I start PR-tlm-p3 (the arbiter work).
