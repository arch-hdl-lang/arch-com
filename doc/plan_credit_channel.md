# Plan: `credit_channel` — first-class credit-based flow control

*Author: session of 2026-04-22. Status: design draft; not yet implemented.*

**Update (2026-04-22)**: `credit_channel` nests inside `bus` as a
sub-construct, mirroring `handshake_channel` — see
[`plan_bus_unification.md`](plan_bus_unification.md) for the broader
story. The nested-in-bus form is the **primary** spelling; the
standalone `credit_channel X ... end` form kept below remains valid
as parse-time sugar that desugars to a one-channel anonymous bus.
Semantics, lowering, and SVA are identical between the two forms.

## Motivation

Credit-based flow control is the dominant backpressure pattern anywhere
that a valid/ready round-trip is too slow — NoCs, PCIe, chiplets, long
clock domains, high-throughput streaming. The protocol is simple:

1. The sender holds a **credit counter** representing available slots on
   the receiver.
2. Send is conditional on `counter > 0`. Sending decrements the counter.
3. When the receiver frees a slot, it pulses a **credit-return** signal.
   The sender increments on that pulse.

Unlike valid/ready, this is fundamentally **stateful**: the compiler
owns a counter register, a receive buffer, and the timing of
credit-return events.

`handshake` (plan_handshake_construct.md) deliberately excludes this —
it covers port-shape sugar only, and every stateful variant belongs in
its own construct. Users today hand-roll credit counters + FIFOs +
return logic, which is 150–200 lines of repetitive infrastructure per
channel and a common source of off-by-one / lost-credit bugs.

A first-class `credit_channel` construct removes this — one declaration
ships the counter, the FIFO, the return path, and formal-verification
assertions.

## Why not a library

A library version can define the wire protocol (send_valid, send_data,
credit_return) but cannot synthesize the sender counter or the receiver
FIFO — those are per-instance hardware that the compiler has to own.
Every user of a "library credit channel" would end up copy-pasting the
same 200-line wrapper. That's worse than no primitive.

By contrast, `credit_channel` sits alongside the language's other
stateful first-class constructs:

| Construct | Category | State owned by compiler |
|---|---|---|
| `handshake` | port sugar | none — wires + compile-time SVA only |
| `fifo` | first-class | buffer + pointers + full/empty flags |
| `ram` | first-class | storage + read/write control |
| `arbiter` | first-class | grant FSM |
| `synchronizer` | first-class | CDC flop chain |
| **`credit_channel`** *(this plan)* | **first-class** | **counter + buffer + credit-return pulse** |

## Semantics

### The three roles

A `credit_channel` port has three observable roles depending on which
side of the wire it's on:

- **Sender** (`initiator` perspective): writes `data` when it has
  credit. Exposes `can_send: Bool` (true when counter > 0). Sending
  decrements the internal counter.
- **Receiver** (`target` perspective): reads from a built-in FIFO.
  Exposes `valid: Bool` (buffer nonempty) and `data: T`. When the user
  calls `pop`, the front of the buffer is removed and a credit-return
  pulse is sent to the sender.
- **Wire** (between): `send_valid`, `send_data`, `credit_return` —
  synthesized automatically, never user-visible.

### Send: non-blocking by default

The sender side is **non-blocking**: `can_send` is an output from the
credit machinery, and user logic gates its writes on it:

```
if ch.can_send and have_data_to_send
  ch.send(next_data);       // decrements counter; wire send_valid pulses
end if
```

This fits ARCH's existing single-cycle semantics. Attempting to send
when `can_send` is low is a simulation-time error (aborts with a clear
message) and a synthesized SVA assert at build time. There is no
runtime stalling in the normal `seq`/`comb` path.

**Blocking semantics via `thread`**. Users who want declarative "wait
for credit, then send" write it inside a `thread` block using the
existing `wait until` primitive:

```
thread sender
  wait until ch.can_send;
  ch.send(next_data);
end thread sender
```

Thread lowering already emits a state machine; the `wait until`
condition naturally accommodates `ch.can_send`. No new threading
machinery is needed.

### Receive: FIFO-backed

The receiver side is a FIFO read interface. Internally, the compiler
instantiates a depth-N FIFO (N = declared credit depth) and wires
`send_valid` + `send_data` to its push side. Reading uses the
`valid` / `data` / `pop` trio:

```
if ch.valid
  latest <= ch.data;        // combinational access to current front
  ch.pop();                 // dequeue — emits credit-return pulse
end if
```

`valid` and `data` are *combinational outputs* of the buffer;
`pop` is an action (like a method call) that takes effect on the
current clock edge.

### Credit lifecycle

| Event | Counter action | Buffer action |
|---|---|---|
| Reset | counter = DEPTH | buffer emptied |
| Sender calls `ch.send(x)` | counter -= 1 | buffer appends x next cycle |
| Receiver calls `ch.pop()` | *(later)* counter += 1 | buffer pops front; wire asserts `credit_return` |
| `credit_return` arrives at sender | counter += 1 | — |

**Invariants** (enforced by auto-SVA):
- `0 ≤ counter ≤ DEPTH` always.
- Buffer occupancy + sender credit = DEPTH always.
- `credit_return` fires iff the receiver called `pop` that cycle.
- Sender's `send_valid` implies `counter > 0`.

### Clock domain

v1 scope: single-clock. `credit_channel` is declared in one domain;
both sender and receiver are in the same clock. Cross-domain credit
channels (useful for CDC in NoCs) are a v2 topic — they'd wrap the
buffer in `synchronizer` and add gray-coded pointers, which is its
own research problem.

## Syntax

`credit_channel` carries an opaque payload of type `T`, parameterized
exactly like `fifo`:

```
credit_channel CmdCh
  param T:     type  = UInt<64>;    // payload type — use a struct for multi-field
  param DEPTH: const = 4;           // receiver buffer / initial credit pool
end credit_channel CmdCh
```

Multi-field payloads use a user-declared struct as the type argument —
same pattern as `fifo` and every other ARCH construct that carries
polymorphic data:

```
struct CmdBeat
  data: UInt<64>;
  last: Bool;
end struct CmdBeat

credit_channel CmdCh
  param T:     type  = CmdBeat;
  param DEPTH: const = 4;
end credit_channel CmdCh
```

Used via ordinary ports:

```
use CmdCh;

module Producer
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port out: initiator CmdCh<T=CmdBeat, DEPTH=8>;

  // simple example: send a rising counter whenever credit is available
  reg tick: UInt<64> init 0 reset rst => 0;
  seq on clk rising
    if out.can_send
      out.send('{data: tick, last: 1'b0});
      tick <= tick + 1;
    end if
  end seq
end module Producer

module Consumer
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port in:  target CmdCh<T=CmdBeat, DEPTH=8>;

  // drain as fast as possible
  seq on clk rising
    if in.valid
      in.pop();          // emits credit_return
    end if
  end seq
end module Consumer
```

**Why no custom `payload` block**: the original sketch had one —
dropped for consistency with `fifo`, which already handles arbitrary
data shapes via the type parameter. Users never need to remember two
different payload-declaration spellings. Multi-field payloads piggyback
on structs, which the compiler already knows how to pack, emit, and
destructure.

Grammar addition:
```
CreditChannelDecl := 'credit_channel' Ident
                       ParamDecl*
                     'end' 'credit_channel' Ident
```

No `handshake`-style protocol variants — credit is one protocol, not
six. The two knobs the user has are the payload type `T` and the
credit depth `DEPTH`.

## Lowering

A `credit_channel CmdCh` with `DEPTH=N` lowers to:

### Wire protocol (between initiator and target ports)

- `send_valid: Bool`    *(initiator → target)*
- `send_data: T`        *(initiator → target)* — single typed payload
- `credit_return: Bool` *(target → initiator)*

`T` is whatever type was passed at the port site. For struct `T`, the
packed struct crosses the port boundary as-is (matching how the
existing `bus` + `handshake` mechanism flattens struct-typed signals).

### Initiator module body (synthesized)

```
// Synthesized by credit_channel codegen
reg __ch_credit: UInt<clog2(DEPTH+1)> reset rst => DEPTH;
// can_send is combinational:
let ch_can_send: Bool = __ch_credit > 0;
// send_valid wired to the user's ch.send() calls (same cycle)
// counter update:
seq on clk rising
  if credit_return and not send_valid
    __ch_credit <= __ch_credit + 1;
  elsif send_valid and not credit_return
    __ch_credit <= __ch_credit - 1;
  end if   // both same cycle: no change (already in flight)
end seq
```

### Target module body (synthesized)

```
// Instantiated FIFO from existing `fifo` construct:
inst __ch_buf: fifo
  DEPTH = DEPTH
  WIDTH = T                     // same type parameter as the credit_channel
end inst __ch_buf

// push wiring:
comb
  __ch_buf.push_valid = send_valid;
  __ch_buf.push_data  = send_data;            // typed payload (T)
end comb

// consumer-facing view:
let ch_valid: Bool = __ch_buf.pop_valid;
let ch_data:  T    = __ch_buf.pop_data;

// pop + credit_return wired when user calls ch.pop()
```

### Auto-emitted SVA (Tier 2)

```
_auto_cc_<port>_credit_bounds:
  assert property (@(posedge clk) disable iff (rst)
    __ch_credit <= DEPTH);

_auto_cc_<port>_send_requires_credit:
  assert property (@(posedge clk) disable iff (rst)
    send_valid |-> __ch_credit > 0);

_auto_cc_<port>_buffer_occupancy:
  assert property (@(posedge clk) disable iff (rst)
    (DEPTH - __ch_credit) == __ch_buf.occupancy);

_auto_cc_<port>_credit_return_implies_pop:
  assert property (@(posedge clk) disable iff (rst)
    credit_return |-> __ch_buf.pop_valid_deasserted_next);
```

Labels follow the existing `_auto_*_<rule>` convention for EBMC /
Verilator `--assert` consumption.

## Error matrix

| Misuse | Compile-time or runtime | Message |
|---|---|---|
| `ch.send(...)` on receiver-side port | compile error | "`send` is not valid on target-perspective port `ch`" |
| `ch.pop()` on sender-side port | compile error | "`pop` is not valid on initiator-perspective port `ch`" |
| `ch.send(...)` when `can_send` is false (observed in sim) | runtime abort | "credit-underflow on `ch`: send attempted with counter = 0" |
| `DEPTH` < 1 | compile error | "credit_channel depth must be ≥ 1" |
| Payload referenced as `ch.payload.field` (SV-style) | compile error suggesting `ch.data` | Use clean form |
| Multiple `send` calls to same channel in same cycle | compile error | "multiple driver of `ch.send`; use a single mux" |

## Interaction with existing constructs

- **`fifo`**: credit_channel reuses `fifo` for the receiver buffer.
  No new storage implementation.
- **`thread`**: `wait until ch.can_send` works naturally; credit_channel
  shows up as a normal port condition.
- **`bus`**: a `credit_channel` port CAN be declared inside a bus,
  providing a composable bundle with other signals. (Stretch goal;
  verify in v1.)
- **`arch formal`**: the four auto-emitted SVA properties should be
  PROVED for a standalone credit_channel and REFUTED for an explicit
  bug (e.g., send counter underflow). Validates the Tier 2 path.
- **`--check-uninit`**: inapplicable — the synthesized counter + FIFO
  have deterministic reset, no uninit state.

## v1 scope (what ships first)

Ship the minimum useful thing. Four pieces:

1. **Parser**: `credit_channel Name ... payload ... end payload ... end credit_channel Name`.
2. **Resolve**: register as a new `Symbol::CreditChannel(info)`.
3. **Elaboration**: expand the initiator and target sides into:
   - Sender: one counter reg + `can_send` let + counter-update seq.
   - Receiver: one `fifo` inst + `valid`/`data`/`pop` wiring.
4. **Codegen + sim**: reuse existing reg / fifo lowering. New SVA
   emission for the four credit invariants.

## Non-goals in v1

- **Cross-clock credit channels** (CDC). Needs gray-coded counters +
  credit synchronizers; a v2 design point.
- **Credit-return pacing / batching**. The v1 rule is 1-pop = 1-return.
  Batched returns (e.g., "return 4 credits at once") are a future
  extension.
- **Multiple initiators on one channel**. v1 is point-to-point.
  Many-to-one requires an arbiter at the receiver side, which users
  can compose today via `arbiter`.
- **Priority/QoS lanes**. Future extension; v1 is single-lane.
- **Runtime-parameterizable depth**. DEPTH is compile-time const in
  v1, matching other constructs.
- **`send` with simultaneous `pop`**. Handled — concurrent events
  cancel in the counter update (counter stays the same). Callers
  shouldn't need to reason about this.

## Implementation roadmap

Four focused PRs:

### PR #1 — grammar + AST + resolve
- Lexer: `credit_channel` token. `payload` as contextual keyword.
- AST: `CreditChannelDecl` with `params`, `payload_fields`, `span`.
- Parser: grammar for the block.
- Resolve: `Symbol::CreditChannel(CreditChannelInfo)`.
- No codegen yet; verify the structure parses and type-checks.

### PR #2 — elaboration (initiator + target expansion)
- Walk each module's ports. For every `credit_channel` port, inject
  the synthesized reg / fifo / wiring into the module body.
- Sender side: counter reg + `can_send` let + counter-update seq.
- Receiver side: fifo inst + valid/data/pop wiring.
- Tie the wire protocol to the bus-port flattening mechanism so SV
  emission gets the right port list without any new codegen.

### PR #3 — SVA + formal
- Auto-emitted assertions (the four invariants in §Lowering above).
- `arch formal` test: a standalone credit_channel should PROVE the
  four properties; an intentionally buggy sender (e.g., removes the
  `can_send` gate) should REFUTE.

### PR #4 — docs + stdlib example
- Spec §N `credit_channel` section covering semantics, lowering, SVA.
- Reference card entry.
- `stdlib/` sample: `CcAxiCmd.arch` — a credit-channel definition
  shaped like an AXI-Full command channel, as a worked example.

## Open questions

1. **Syntax for `ch.send(...)`** — is it a method call expression or
   a new statement? Leaning toward **method call** to fit the
   existing `.trunc`/`.zext`/`.reverse` shape. Whether it lives in
   `comb` or `seq` depends on whether `send_valid` is combinational
   or registered — leaning combinational (sender issues a single-
   cycle pulse; counter updates next cycle).

2. **`can_send` = current credit or one-cycle-ahead?** Leaning
   current. Means a sender that sends every cycle at max rate sees
   `can_send` deasserting one cycle after the counter hits zero.
   Matches standard credit-return semantics.

3. **Buffer implementation: `fifo` (synchronous) vs a simpler
   `pipe_reg` chain for depth=1/2?** Leaning `fifo` always — keeps
   lowering uniform and the depth=1/2 special case isn't worth the
   branching complexity.

4. **Payload field access**: `ch.data.field` or flat access
   `ch.<field>`? Leaning flat access for consistency with how
   `handshake` payload works today.

## Non-goals for the plan itself

- Not drafting the CDC / cross-clock extension. Those are separate.
- Not committing to multi-initiator. Separate future work.
- Not locking down the `send`/`pop` method naming — final choice
  happens in PR #1 once the grammar is being drafted.
