# Plan: `bus` as the universal interface construct

*Author: session of 2026-04-22. Status: design draft; retrofits several
prior plans (handshake, credit_channel, tlm) under one conceptual
umbrella.*

## The problem

ARCH has grown four inter-module communication constructs, each
designed in isolation:

| Construct | Where it lives | Status |
|---|---|---|
| `bus` | Top-level, carries plain signals + `handshake` channels | Shipped |
| `handshake` | Sub-construct inside `bus` body | Shipped (Tier 1, 1.5, 2) |
| `credit_channel` | Standalone top-level construct (planned) | Design draft |
| `tlm` | Uses `socket` bindings, separate construct family | Planned |

Seen from outside, these look like four unrelated solutions to one
problem: **how do modules communicate?** The fragmentation makes the
language harder to teach and agents more likely to reach for the
wrong tool. A user building an AXI-like interface mixes `bus` +
`handshake` (RTL signal level) + `credit_channel` (stateful
backpressure) + `tlm` (transaction abstraction) and has four
different port-declaration spellings.

This plan unifies them.

## The underlying pattern

Every communication construct is a point on two axes:

|  | **Stateless** (wires only) | **Stateful** (compiler owns state) |
|---|---|---|
| **Signal-level** | `handshake_channel` | `credit_channel` |
| **Transaction-level** | `bus` raw signals | `tlm_method` |

`bus` isn't a fifth thing — it's the **grouping container** that every
communication primitive nests inside. Today it only carries plain
signals + handshake. The rest feels bolted on because they live outside.

**Thesis**: make `bus` the universal interface construct, and make every
communication primitive a sub-construct that nests inside it.

## Proposed shape

```
bus PeripheralCtrl
  param ADDR_W: const = 32;
  param DATA_W: const = 32;

  // 1. Plain signals (today's bus body — unchanged)
  irq:      out Bool;
  clk_gate: in  Bool;

  // 2. Handshake channel — stateless signal-level (today's `handshake`,
  //    renamed for consistency; see §Rename below)
  handshake_channel cmd: send kind: valid_ready
    addr: UInt<ADDR_W>;
    wdata: UInt<DATA_W>;
  end handshake_channel cmd

  // 3. Credit channel — stateful signal-level (was a standalone
  //    top-level construct; now nests in bus)
  credit_channel data: send
    param T:     type  = UInt<DATA_W>;
    param DEPTH: const = 16;
  end credit_channel data

  // 4. TLM method — transaction-level (planned; nests in bus from day one)
  tlm_method read(addr: UInt<ADDR_W>) -> UInt<DATA_W>: blocking;
  tlm_method write(addr: UInt<ADDR_W>, data: UInt<DATA_W>): pipelined;
end bus PeripheralCtrl
```

One `bus` declaration, four communication flavors composed, one port
type at use site:

```
port p: initiator PeripheralCtrl<ADDR_W=16, DATA_W=64>;
```

## What this buys

1. **One mental model**. "A bus is an interface bundle; pick protocol
   sub-constructs for each channel." No need to remember whether
   credit is a top-level construct or nests somewhere; answer is
   always "nests in bus, like every other communication sub-construct."
2. **Uniform composition machinery**. `credit_channel` and (future)
   `tlm_method` reuse the same `bus` flattening + `initiator` /
   `target` perspective flip + `generate_if` conditional signals +
   stdlib discovery that `handshake` already has.
3. **stdlib scales**. `BusAxiLite` today, `BusAxi4Tlm` tomorrow, both
   live in `stdlib/` with the same `use X;` discovery.
4. **Bus can be heterogeneous**. Mix RTL-level signals with TLM
   methods in one interface — useful for hybrid levels of abstraction
   in the same module port.

## Rename: `handshake` → `handshake_channel`

For consistency with the new sibling sub-constructs:

| Before | After |
|---|---|
| `handshake` | `handshake_channel` |
| `credit_channel` | `credit_channel` (unchanged) |
| `tlm` | `tlm_method` (planned — the `tlm` keyword for sockets becomes `tlm_method` inside `bus`) |

All sub-constructs read as "what kind of channel/method does this bus
carry." The `_channel` / `_method` suffix mirrors the category name.

Migration path (mechanical, backward-compat for one release):

- PR A: parser accepts both `handshake` and `handshake_channel`
  tokens → same AST node. Deprecation warning on `handshake` (same
  pattern as the `port reg` deprecation in #51).
- PR B: mechanical rename in test corpus + stdlib + docs
  (`handshake` → `handshake_channel`). Like the `WIDTH` → `T`
  rename in #53.
- PR C: remove `handshake` keyword in next minor release (e.g. v0.45.0).

Cost: tokenize-and-match is ~5 LoC; corpus rename is ~30 files
touched. Same playbook we've used twice already (port reg → pipe_reg,
WIDTH → T) and it works cleanly.

## `bus` scope once unified

After this lands, `bus` carries:

- **Plain signal declarations** (today) — `name: in|out Type;`
- **`handshake_channel`** — stateless valid/ready-style protocols
- **`credit_channel`** — stateful backpressure with compiler-owned FIFO
- **`tlm_method`** (future) — transaction-level methods with
  blocking / pipelined / out_of_order / burst concurrency modes
- **`generate_if`** — conditional inclusion of any of the above
- **`param`** declarations — parameterizes everything above

Bus does NOT carry:

- Modules, fsms, threads, or any other top-level construct — those
  are higher-level designs, not communication primitives.
- Storage (`ram`, `fifo`) — those live inside modules, not in
  interfaces.
- Arbitration (`arbiter`) — same reason.

The rule is: **`bus` carries interface definitions, not hardware
implementations.** The implementation of each sub-construct is
synthesized *at the bus port use site* — per port instance on the
initiator/target sides.

## Interaction with already-shipped features

### `bus` + `handshake` (today)

No behavior change — `handshake` becomes `handshake_channel` via the
rename but keeps identical semantics (Tier 1 port shape, Tier 2 SVA,
Tier 1.5 payload correctness).

### `bus` + `credit_channel` (this plan's real addition)

Credit channel moves from its own top-level construct to a bus
sub-construct. Users write:

```
bus DmaCh
  credit_channel data: send
    param T:     type  = UInt<64>;
    param DEPTH: const = 8;
  end credit_channel data
end bus DmaCh

module Producer
  port out: initiator DmaCh;
  // out.data.can_send, out.data.send(x), etc.
end module Producer
```

The `out.data.send()` method name uses the nested path consistent with
existing bus member access (`axi.aw_valid`). On the target side,
`in.data.valid` / `in.data.pop()` work the same way.

Standalone `credit_channel CmdCh ... end` form still parses and is
syntactic sugar that expands to an anonymous one-channel bus. Same
pattern as "handshake-at-port-level" could be (but wasn't) for
handshake. Preserving this shorthand keeps simple single-channel
cases readable:

```
// Sugar form (expands to an anonymous bus):
credit_channel CmdCh
  param T:     type  = UInt<64>;
  param DEPTH: const = 8;
end credit_channel CmdCh

// Used like a normal top-level construct:
port out: initiator CmdCh;
```

### `bus` + `tlm_method` (future)

When TLM lands, methods nest inside `bus` (not inside their own
separate `interface` construct as SV does). Modules attach as
`initiator` (call the methods) or `target` (implement them), same
perspective-flip rule that handshake already uses.

## Implementation roadmap

Five focused PRs, ordered so each depends only on the prior:

### PR #1 — unification plan merge (this doc)

Lock down the model. No code changes.

### PR #2 — `handshake` → `handshake_channel` rename

- Lexer: new `handshake_channel` token. Keep `handshake` working with
  a deprecation warning.
- Parser: accept both as alternatives to the same AST node.
- Warning: `handshake` triggers "use `handshake_channel` instead; will
  be removed in v0.45.0."
- Test corpus: sed-style rename (same playbook as WIDTH → T).
- Spec + reference card: update to `handshake_channel` as primary
  spelling.

### PR #3 — `credit_channel` inside `bus`

- Parser: recognize `credit_channel` as a bus sub-construct.
- Resolve: a bus with credit_channel sub-constructs carries extra
  metadata (like `BusInfo.handshakes` does today).
- Elaboration: at each port use site, synthesize the counter + fifo
  per credit_channel sub-construct — reusing the same per-port-site
  expansion pattern handshake uses.
- The standalone `credit_channel X ... end` form desugars at parse
  time to `bus X { credit_channel __default ... }`.

### PR #4 — credit_channel codegen + SVA

Implements the Tier-2 invariants from the credit_channel plan.
Independent of the bus-nesting story but blocked on PR #3 for the
per-port-site expansion hook.

### PR #5 — docs + NoC test

Spec chapter on the unified bus construct. Reference card refresh.
The NoC flit credit validation test from `plan_credit_channel.md`
§Validation plan.

### PR #6 (future, not in this plan) — `tlm_method`

Land TLM as a bus sub-construct from day one. Separate plan required.

## Non-goals

- **Not renaming `bus` to `interface`.** SV-aligned naming is
  tempting, but the migration cost is larger than the educational
  win. Keep `bus` as the universal name; document it as "the
  interface grouping."
- **Not unifying with `socket`.** Today's planned `tlm` design uses
  `socket` for binding. When TLM lands, `socket` becomes a bus port
  binding mechanism; the `bus` carries the `tlm_method` declarations.
  No standalone `socket` construct.
- **Not touching module-internal constructs.** `fifo` / `ram` /
  `arbiter` stay where they are; they're implementations,
  not interfaces.
- **Not deprecating `thread`.** But its role sharpens — see next
  section. `thread` is the implementation vocabulary for the
  target side of `tlm_method`, AND it remains the form for
  genuinely autonomous internal sequencers (heartbeat timers,
  housekeeping state machines) that don't cross an interface.

## `thread` and `tlm_method`: the relationship

`thread` and the planned `tlm_method` overlap substantially — both
express multi-cycle sequential behavior, both lower to an FSM, and
both are compiler-owned state. The cleanest framing, and the one
this plan commits to:

> `tlm_method` is the **interface-level abstraction**; `thread` is
> the **implementation vocabulary** that the compiler uses to lower
> `tlm_method` targets — and that remains available for autonomous
> internal sequencers that don't cross an interface.

|  | `thread` | `tlm_method` |
|---|---|---|
| Scope | Module-internal, no required interface | Crosses a bus boundary (method call) |
| User writes | Explicit FSM scripting (`wait until`, `wait N cycle`, `fork`/`join`) | Function signature + concurrency mode |
| Initiator side | N/A | Compiler auto-generates thread-like FSM for each call site |
| Target side | (This IS the implementation) | Implemented as a `thread` body bound to the method |
| Example use | Timer that ticks every 1000 cycles | `axi.read(addr) -> data: blocking` |

Concrete shape under this plan (when TLM lands):

```
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
  tlm_method write(addr: UInt<32>, data: UInt<64>): pipelined;
end bus Mem

module Initiator
  port m: initiator Mem;
  thread driver
    let val = m.read(0x1000);       // compiler generates the wait-state FSM
    m.write(0x2000, val + 1);       // pipelined — returns a future
  end thread driver
end module Initiator

module Target
  port s: target Mem;
  // thread bodies ARE the method implementations.
  thread s.read(addr)
    wait until internal_ready;
    return mem[addr];               // compiler wires to the response channel
  end thread s.read
end module Target

module InternalTimer               // no interface — plain autonomous thread, unchanged
  thread heartbeat
    wait 1000 cycle;
    tick <= tick + 1;
  end thread heartbeat
end module InternalTimer
```

### Migration target for validating TLM

Existing DMA test corpus contains `ThreadMm2s` and `ThreadS2mm` —
multi-cycle sequencers that shepherd AXI signaling across cycles.
These are exactly the "thread crossing an interface" pattern that
`tlm_method` is designed for. When TLM ships, those tests migrate
from hand-written AXI shepherding threads to:

```
bus Axi4
  tlm_method read(addr: UInt<32>, len: UInt<8>) -> Future<Vec<UInt<64>, len>>: burst;
  tlm_method write(addr: UInt<32>, data: Vec<UInt<64>>): pipelined;
end bus Axi4

module Mm2s
  port m_axi:  initiator Axi4;
  port m_axis: initiator BusAxiStream;
  thread dma
    let data = m_axi.read(src_addr, beats);   // was: manual ar-channel FSM
    for beat in data
      m_axis.t.send('{data: beat, last: ...});
    end for
  end thread dma
end module Mm2s
```

Side-by-side comparing the hand-rolled thread against the TLM-backed
thread is the concrete evaluation of whether TLM earns its keep. If
the migrated form is shorter, clearer, and generates equivalent SV,
TLM is validated. If the hand-rolled form is still preferable (e.g.
user needs fine control over signal timing that TLM abstracts away),
TLM needs a lower-level escape hatch.

### Consequence for the plan

- **PR #6 (future TLM)** design starts from this framing: `tlm_method`
  lives as a `bus` sub-construct; target implementations use `thread`;
  initiator call sites auto-generate thread-like FSMs.
- The existing DMA tests (`tests/axi_dma_thread/`) are the canonical
  TLM validation workload.
- No breaking change to `thread` — the existing explicit form stays
  valid for autonomous sequencers and for users who need
  lower-than-TLM control.

## Open questions

1. **Member access for credit_channel inside a bus.** `port.ch.send(x)`
   or `port.ch_send(x)` (bus-flattened form)? Handshake uses the
   flattened form for wires (`port.aw_valid`). Credit channel has
   methods (`send()`, `pop()`) that flatten less naturally. Leaning
   **dotted access** for methods (`port.ch.send(x)`), **flattened**
   for wires (`port.aw_valid`). Decide concretely in PR #3.

2. **Ordering of sub-constructs within a bus body.** Today's bus
   allows plain signals + handshake in any order. Does the unified
   body require any ordering (e.g. params first, then channels, then
   plain signals)? Leaning **no ordering constraint** — the compiler
   doesn't care, and users mix and match naturally.

3. **Does standalone `credit_channel` / `handshake_channel` stay?**
   Two answers:
   - Yes, as parse-time sugar for `bus { one channel }`. Keeps single-
     channel cases readable.
   - No, force users into a wrapping `bus` always. More consistent but
     more ceremony for the common single-channel case.
   
   Leaning **yes**, with the sugar. Same pattern as Rust's `use x` for
   a single item inside a `mod` block.

4. **Rename timeline.** Two-release deprecation cycle for `handshake`
   → `handshake_channel` (same as `port reg` → `pipe_reg`), or aggressive
   one-release cutover? Leaning **two-release** for consistency with
   the existing deprecation pattern.

## What this plan does NOT say

- Does not commit to when `tlm_method` lands. Its own plan, later.
- Does not pick final wire-protocol names for credit_channel
  (`send_valid` vs `push_valid`, etc.). PR #3 locks them down.
- Does not address cross-clock-domain buses. `credit_channel` v1 is
  single-clock (per credit_channel plan); CDC is v2 for every
  sub-construct.
