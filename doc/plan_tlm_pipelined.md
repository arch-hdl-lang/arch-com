# Plan: `tlm_method` pipelined — **SHELVED** (2026-04-23)

> **Shelved per design review (2026-04-23).** The concurrency-mode
> label `pipelined` doesn't buy capability beyond what users already
> express with `generate for threads` + `lock RESOURCE` (for request
> serialization) + ID-tagged responses (for routing). See
> `tests/axi_dma_thread/ThreadMm2s.arch` for the canonical
> multi-outstanding AXI pattern — it uses these building blocks
> without any dedicated TLM "pipelined" support.
>
> The genuinely new capability is **`out_of_order`** mode (auto-route
> responses by ID tag); that's the next real v2 target. Tracking in
> a future `plan_tlm_out_of_order.md`.
>
> `reentrant` grammar on ThreadBlock (merged in PR #86) stays as
> dead-but-parsed code; removal or repurposing is a future cleanup.
> A targeted diagnostic (PR-tlm-p3) points users at the
> `lock RESOURCE` idiom for multi-thread TLM sharing today.
>
> Historical design below kept for context — the reentrant thread
> and Future<T>/await sketches led to the current framing.

---

# (Historical) Plan: `tlm_method` pipelined via multi-thread arbitration (v2a)

*Author: session of 2026-04-22. Supersedes the earlier `Future<T>`/
`await` sketch and the `reentrant thread` sketch. Builds on
`doc/plan_tlm_method.md` v1 (PRs #74–#84).*

## The final model

The two earlier pivots (`Future<T>` / `await`, then `reentrant max N`)
both tried to express pipelining inside a single thread. Both hit
semantic problems:

- `Future<T>`/`await` invented a new type + keyword when the intent —
  "let the next call issue while this one waits" — is already what
  parallel threads express.
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
| New user syntax | `Future<T>`, `await`, maybe `await_all` | `reentrant max N`, `_instance_id`? | none |
| Per-instance state | Future slot tracking | ambiguous — new grammar needed | naturally per-thread |
| Per-instance writes | Future targets are regs | multi-driver unless indexed | each thread writes `d[i]` |
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
