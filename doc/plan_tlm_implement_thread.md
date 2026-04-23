# Plan: `implement` — glue TLM methods to threads — **SHELVED** (2026-04-23)

> **TLM's positioning in the language (lesson from this design review,
> 2026-04-23):**
>
> `tlm_method` is a **fast-prototyping** abstraction. The call-site
> form `d <= m.read(addr)` lets a user express "read a word from
> this interface" in one line, accept blocking semantics, and iterate
> quickly. It is *not* a high-performance design primitive.
>
> Multi-outstanding AXI, pipelined memory interfaces, and other
> throughput-critical patterns intrinsically exploit protocol
> structure (separate channels, ID-tagged responses, burst beats)
> that the TLM atomic-call abstraction collapses. Optimizing those
> patterns belongs to either:
>
> 1. **Hand-rolled threads** — the current path. The user writes
>    explicit issuer + collector threads with `shared(or)` and
>    `lock`, as `tests/axi_dma_thread/ThreadMm2s.arch` demonstrates.
> 2. **A future HLS pass** — takes TLM-flavored source and generates
>    the equivalent multi-outstanding implementation. That's a
>    separate compiler project, not a TLM extension.
>
> With this framing, TLM v1 (blocking, single-thread per method) is
> feature-complete for its role. The `implement` pool, `reentrant`,
> `Future<T>/await`, and the `pipelined`/`out_of_order`/`burst`
> modes all tried to blur TLM into the high-performance role — none
> of them succeeded cleanly, and all were shelved.

> **Shelved per AXI DMA side-by-side analysis (2026-04-23).** We wrote
> a TLM-Model-B version of `tests/axi_dma_thread/ThreadMm2s.arch` (kept
> for reference at `doc/examples/TlmMm2s_shelved.arch`) and compared
> against the shipped hand-rolled thread version.
>
> **Findings**:
>
> | Dimension | Hand-rolled thread | TLM Model B |
> |---|---|---|
> | Lines                     | 116   | 111  |
> | Threads                   | 5 (1 ArIssuer + 4 RCollect) | 8 (4 workers + 4 callers) |
> | AR/R channel split exploit| yes   | no — atomic `m.read()` collapses both |
> | AXI burst efficiency      | 1 AR per N beats | 1 AR per beat (without v2c burst) |
> | `xfer_ctr` ownership      | single-writer | N-way race on callers |
>
> The TLM call-site abstraction **collapses separate AR/R channels
> into one atomic unit**, which is a structural mismatch for
> AXI-style multi-channel protocols. The hand-rolled version is
> *clearer* and *more efficient* because it exploits the protocol's
> intrinsic parallelism.
>
> **Decision**: ship nothing further on `implement` pools. TLM stays
> at v1 (single-thread blocking). Advanced patterns compose via
> existing `thread` + `generate for` + `lock` + `shared(or)` — as
> `ThreadMm2s` already demonstrates. This is option (B) from the
> design review — "TLM v1 blocking + single thread is the sweet
> spot; complex patterns deserve the explicit thread-level treatment
> the corpus already uses well."
>
> **What stays shipped** (from PR-tlm-i1 through i3):
>
> - `implement` grammar + AST on `ThreadBlock`. Harmless dead code
>   parallel to `reentrant`. Future-compat only; no plans to extend.
> - `implement target` as sugar for v1 dotted-name target syntax
>   (single implementer only; multi-implementer target is a permanent
>   compile error).
> - Single-thread `implement m.method()` on the initiator side (equivalent
>   to v1; the annotation is allowed but does nothing extra).
>
> **What is permanently closed** (no follow-up PRs planned):
>
> - Multi-thread `implement` arbitration + dispatch (original PR-tlm-i4).
> - `Future<T>` / `await` (earlier pivot).
> - `reentrant [max N]` on threads (prior pivot; dead grammar remains).
> - TLM `pipelined` / `out_of_order` / `burst` concurrency modes.
>
> Historical design below retained for context.
>
> ---

*Original v2 plan (2026-04-23). Supersedes the shelved pipelined plan
(`plan_tlm_pipelined.md`) and the prior Future<T>/reentrant sketches.*

## The idea in one paragraph

TLM methods declare a wire-level contract (req/rsp handshake with typed
args and return). Threads are the universal mechanism for expressing
multi-cycle agents in ARCH — single-threaded, parallel via `generate
for`, coordinated via `lock` and `resource`. **Glue the two together
with an `implement` clause on the thread header**, and the compiler
handles the ID allocation + arbitration + response routing that turns
N threads into an N-way concurrent agent for a single TLM method.
No new TLM mode keywords, no new types (`Future<T>`), no reentrant
semantics — existing constructs compose.

```
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;   // unchanged
end bus Mem

generate for i in 0..4
  thread driver_i implement m.read() on clk rising, rst high
    d[i] <= m.read(addr[i]);
  end thread driver_i
end generate for i
```

Compiler sees 4 threads, all with `implement m.read()` → auto-allocates
4 IDs, emits the id-tagged request arbiter, the response-routing FIFO
filter, and per-thread destination routing. Method author doesn't pick
concurrency for callers.

## Why `implement` over the alternatives

| Approach | User syntax | Compiler machinery |
|---|---|---|
| TLM `pipelined` mode | `tlm_method read: pipelined max 8;` | auto-allocates issue FIFO + routing per-method |
| TLM `out_of_order` mode | `tlm_method read: out_of_order id_width: 3;` | user-visible ID tag on Token<T, id:N> |
| `reentrant` thread | `thread X reentrant max N` | N-way FSM cloning per-thread |
| Future<T> + await | `let f = m.read(x); await f;` | new type, new keyword, issue FIFO |
| **`implement`** (this plan) | `thread X implement m.read() …` | auto id allocation across N threads, existing thread machinery |

`implement` wins on:

- **Zero new TLM-side vocabulary**: methods stay `blocking`. The
  concurrency choice lives on the consumer side, one clause.
- **Leverages existing `generate for threads`**: the pattern users are
  already using for multi-outstanding AXI (ThreadMm2s) becomes the
  canonical shape.
- **Opt-in**: without `implement`, bare multi-thread sharing of a
  method remains a compile error (existing PR-tlm-p3 diagnostic).
  `implement` is the explicit grant that permits compiler glue.
- **No new AST variants at the method layer**: `TlmMethodMeta` stays
  unchanged; the extension is a single optional clause on
  `ThreadBlock`.

## Surface syntax

### Initiator side (new)

```
thread NAME implement <port>.<method>() on clk <edge>, rst <level>
  [default when ... end default]
  <body>
end thread NAME
```

- `<port>.<method>()` is a method reference — no args here (args are
  supplied per-call in the body).
- Body uses `<port>.<method>(args)` calls normally. Compiler treats
  each call site as an issue-point instrumented with the auto-allocated
  ID tag for this thread.
- A single thread with `implement m.read()` is legal — it's equivalent
  to v1 blocking (1 outstanding, 1 ID), but makes the intent explicit.

### Target side (migrating the v1 dotted form)

v1 target syntax:
```
thread s.read(addr) on clk rising, rst high
  return mem[addr];
end thread s.read
```

Under this plan, the v1 form becomes sugar for:
```
thread s_read_impl implement target s.read(addr) on clk rising, rst high
  return mem[addr];
end thread s_read_impl
```

The `implement target` form lets users name the thread explicitly
(useful for debugging / `arch sim --debug-fsm` output). The v1 dotted
form keeps working; both lower to the same AST.

Multiple target-side threads implementing the same method → compiler
allocates them as ID-servers:

```
generate for i in 0..4
  thread server_i implement target s.read(addr) on clk rising, rst high
    return mem[addr];
  end thread server_i
end generate for i
```

Each server_i handles requests matching ID = i. Response tagged with
the same ID. Initiator's ID-routing finds the corresponding thread.

## Auto-ID allocation

When N threads in one module carry `implement <port>.<method>()`
(initiator side) or `implement target <port>.<method>(...)` (target
side):

1. Compiler assigns IDs 0, 1, …, N-1 to them (in source / generate-for
   order). Deterministic for reproducible SV.
2. Wire protocol: the bus flattens an extra `<method>_req_id` + `<method>_rsp_id`
   pair per method declaration that has > 1 implementer *or* is
   explicitly marked with an id-bearing pattern. Width = `clog2(N)`.
3. Response routing: each initiator thread filters `rsp_valid &&
   rsp_id == my_id`; each target thread accepts `req_valid && req_id
   == my_id`.

### id_width on the bus

For round-tripping across module boundaries (the "outer" module that
instantiates the bus might not know how many implementers each side
has), we add an optional `param ID_W: const = 0;` to the bus
declaration. When 0, no id tag is materialized (single-implementer
case, current v1 behavior). When > 0, id tags appear in both req and
rsp directions.

```
bus Mem
  param ID_W: const = 2;         // up to 4 concurrent requests per method
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
end bus Mem
```

The compiler checks that the number of `implement` threads per method
fits in `1 << ID_W` and errors otherwise. Omitting `ID_W` means
single-implementer and the existing tag-free wire protocol.

## Compiler machinery

The initiator-side arbiter + routing was already sketched in
`plan_tlm_pipelined.md` (shelved). Reuse that sketch but driven by
`implement` annotations instead of auto-detecting multi-thread sharing:

1. Group `implement m.method()` threads by (port, method).
2. For each group with N > 1:
   - Synthesize round-robin request arbiter at module scope.
   - Materialize `<method>_req_id` drive as the grant winner's ID.
   - Per-thread response capture: `if (rsp_valid && rsp_id == my_id)
     { rsp_data_mine <= rsp_data; }`.
3. For groups with N = 1:
   - No arbiter, no routing — existing v1 single-thread lowering.

Target side: mirrors this. Per-thread `req_id == my_id` gate at the
entry state; `rsp_id <= my_id` drive on the response state.

## Reuses, doesn't replace

- `lock RESOURCE` — still the right tool for request-side arbitration
  when the user wants *serialized* multi-thread (no id tags, serialize
  end-to-end). `implement` is for *concurrent* multi-thread. Diagnostic
  from PR #89 stays: bare multi-thread without either `lock` or
  `implement` is an error.
- `resource <name>: mutex<policy>;` — unchanged.
- Existing v1 target-side `thread s.read(args) ...` — kept as sugar.
- `generate for threads` idiom — the canonical composition shape.

## v2 PR roadmap

### PR-tlm-i1: grammar + AST + scaffolding reject

- Lexer: `implement` as a contextual keyword (already exists? check).
- Parser: accept `implement <port>.<method>()` clause on initiator
  threads; accept `implement target <port>.<method>(args)` on targets.
- AST: extend `ThreadBlock` with `implement: Option<ImplementBinding>`
  (tagged union over initiator vs target).
- Typecheck / lower_threads: reject `implement` with targeted "not yet
  implemented" error until PR-tlm-i2/i3 land.

### PR-tlm-i2: target-side `implement target` as sugar for v1

- `thread NAME implement target s.read(args) ... end thread NAME`
  lowers exactly like v1's `thread s.read(args) ... end thread s.read`.
- No multi-thread yet — single target implementer only (matches v1
  constraint).
- Validates the grammar + binding + lowering plumbing on the easy case
  before adding arbitration.

### PR-tlm-i3: initiator-side `implement m.method()` — single thread

- Single thread with `implement m.read() ...` lowers like v1 inline
  initiator (no arbitration needed).
- Validates the parse + AST for the initiator case.

### PR-tlm-i4: multi-thread arbitration + id routing

- Detect N > 1 threads with `implement m.read()` in a module.
- Synthesize id-tagged wire protocol (`<method>_req_id`, `<method>_rsp_id`
  from the bus's `ID_W` param).
- Emit round-robin request arbiter + per-thread response capture.
- Same for target side multi-implementers.

### PR-tlm-i5: docs + canonical test

- Spec §18d update with `implement` subsection.
- Reference card entry.
- Canonical end-to-end test: 4-way generate_for driver + 4-way
  generate_for target + verify SV compiles with expected arbiter /
  routing structures.

### Deferred

- Burst mode (`Vec<T, L>` returns, single AR multi-beat response).
  Separate v3 plan.
- Tier-2 SVA on the arbitration invariants.
- `implement` on threads that also have `default when`, `once`, or
  `reentrant` modifiers (might compose, might not — resolve when a
  user hits it).

## Open questions

1. **Target-side multi-implementer ID mapping**: does each target thread
   "claim" a specific ID (user-visible) or does the compiler assign
   IDs matching the generate_for loop variable? **Leaning
   compiler-assigned**, with the loop variable accessible to the user
   for arg filtering if needed.

2. **Mixed `implement` + non-`implement` threads** on the same method:
   hard error — one module must pick one style per (port, method).
   Confirm.

3. **ID tag width management**: require `ID_W` to be declared on the
   bus if any method has multi-implementers? Or infer from
   implementer count? **Leaning require explicit** — keeps the bus
   contract stable across module boundaries.

4. **Single-implementer optimization**: when exactly 1 thread implements
   a method, do we still materialize the id wires? **Leaning no** —
   tag-free wire protocol when `ID_W = 0` (backward compatible with
   v1).

5. **Initiator-side call-site outside the `implement` thread**: if
   module has `thread A implement m.read() ...` and elsewhere
   `thread B` calls `m.read(...)` without `implement`, what happens?
   **Leaning hard error** — a method either has implementers or raw
   callers, not both. Implementers are the arbitrated gateway.

6. **`lock` + `implement`**: is it sensible to lock *inside* an
   implement thread? Locks are for resources outside the method
   itself (say, a shared RAM the target reads from). Should work
   through existing semantics; no new rules.

7. **Single-thread single-implementer sugar**: does `thread X implement
   m.read() { d <= m.read(addr); }` collapse trivially to `thread X {
   d <= m.read(addr); }`? Yes — the only added value is forward
   compatibility with the multi-thread case.

Confirm all defaults and I start PR-tlm-i1 (grammar).
