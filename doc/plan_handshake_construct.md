# Plan: `handshake` primitive (Tier 1 + Tier 2)

*Author: session of 2026-04-18. Status: Tier 1 + Tier 2 shipped as of
v0.43.0 (PRs #21, #23, #25, #26, #27).*

**Update (2026-04-22)**: `handshake` will be renamed to
`handshake_channel` for consistency with its sibling sub-constructs
(`credit_channel`, future `tlm_method`) — see
[`plan_bus_unification.md`](plan_bus_unification.md). Both names will
be accepted during the deprecation window; `handshake` will emit a
deprecation warning and be removed in a future minor release.

## Motivation

AXI4, APB, AHB, Avalon-ST, Wishbone, and every internal streaming interface
the ARCH spec touches ultimately reduces to a small vocabulary of
*flow-control shapes* wrapping a payload. Today users (and LLMs) hand-write
`foo_valid: out Bool; foo_ready: in Bool;` pairs per channel — verbose,
direction-error prone, and repeated identically across every protocol.

A `handshake` primitive collapses this to one line per channel, parameterized
over a closed set of flow-control *kinds*. Conceptually this is a compile-time
sum type (ADT) — each variant produces a different port shape; the compiler
picks at elaboration. Tier 2 additionally auto-emits protocol-correctness SVA
assertions; Tier 1.5 auto-applies the payload `guard` clause so the existing
`--check-uninit` infrastructure catches producer bugs for free.

## Out of scope (Tier 3)

This plan covers **port shape** (Tier 1) and **protocol assertions** (Tier 2).
It does NOT generate the protocol state machine for stateful kinds
(credit-based, 2-phase toggle req/ack). Those belong in a separate
first-class construct (`credit_channel`, analogous to `fifo`) because they
imply the compiler owns counter logic and credit-return paths, not just
ports.

## Syntax

`handshake` is a sub-construct that appears **only inside a `bus` body**
(and, pragmatically, perhaps in module port lists as a convenience).

```
bus BusAxi4
  param ADDR_W: const = 32;
  param DATA_W: const = 32;
  param ID_W:   const = 1;

  handshake aw: send kind: valid_ready
    addr: UInt<ADDR_W>;
    id:   UInt<ID_W>;
    len:  UInt<8>;
    size: UInt<3>;
    burst: UInt<2>;
  end handshake aw

  handshake w: send kind: valid_ready
    data: UInt<DATA_W>;
    strb: UInt<DATA_W/8>;
    last: Bool;
  end handshake w

  handshake b: receive kind: valid_ready
    id:   UInt<ID_W>;
    resp: UInt<2>;
  end handshake b
end bus BusAxi4
```

Grammar additions:

```
HandshakePort := 'handshake' Ident ':' Direction 'kind' ':' Variant NEWLINE
                  PayloadField* 
                'end' 'handshake' Ident
PayloadField  := Ident ':' TypeExpr ';'
Direction     := 'send' | 'receive'
Variant       := 'valid_ready' | 'valid_only' | 'ready_only'
               | 'valid_stall' | 'req_ack_4phase' | 'req_ack_2phase'
```

`Direction` names the channel's **payload role**, NOT any individual
wire's direction. Using role keywords (not `in`/`out`) avoids the
ambiguity of naming a single wire direction in a construct that
produces signals in both directions:
- `send`:    this side is the *producer* (drives valid/req and payload, receives ready/ack)
- `receive`: this side is the *consumer* (receives valid/req and payload, drives ready/ack)

This is the one rule the user has to remember; all individual wire
directions are derived by the compiler, eliminating the "I flipped valid
and ready" bug class.

## Variant catalog (Tier 1 — port expansion)

Given `handshake X: <dir> kind: <V>` with payload fields `f1, f2, …`, the
following flat ports are synthesized. All appear with `X_` prefix at the
SV level, matching today's `bus` flattening convention.

| Variant | Producer side (`send`) | Consumer side (`receive`) | Payload direction |
|---|---|---|---|
| `valid_ready`   | `X_valid: out Bool; X_ready: in Bool;`  | `X_valid: in Bool; X_ready: out Bool;`  | same as channel dir |
| `valid_only`    | `X_valid: out Bool;`                    | `X_valid: in Bool;`                      | same |
| `ready_only`    | `X_ready: in Bool;`                     | `X_ready: out Bool;`                     | same |
| `valid_stall`   | `X_valid: out Bool; X_stall: in Bool;`  | `X_valid: in Bool; X_stall: out Bool;`   | same |
| `req_ack_4phase`| `X_req: out Bool; X_ack: in Bool;`      | `X_req: in Bool; X_ack: out Bool;`       | same |
| `req_ack_2phase`| `X_req: out Bool; X_ack: in Bool;`      | `X_req: in Bool; X_ack: out Bool;`       | same (but toggle semantics) |

Payload fields emit as individual flat ports with the same direction as the
handshake (`<X>_<field>: <dir> <type>;`). The `target` keyword on the bus
port still flips everything — including valid/ready — as today.

Example expansion of `handshake aw: send kind: valid_ready { addr: UInt<32>; id: UInt<1>; }`:

```
aw_valid: out Bool;
aw_ready: in  Bool;
aw_addr:  out UInt<32>;
aw_id:    out UInt<1>;
```

## Variant semantics (timing diagrams)

Legend: `H` = high, `.` = low, `X` = don't-care, `^` = transfer (fire) on
this cycle's rising edge. One column per clock cycle.

### 1. `valid_ready` — bidirectional backpressure

Transfer occurs on cycles where `valid && ready` both hold at the rising
clock edge. Either side may stall. Payload must stay stable while `valid`
is high until `ready` is observed.

```
cycle:    0 1 2 3 4 5 6 7 8 9
valid:    . H H H H . . H H .
ready:    . . H . H . . H . .
payload:  X A A A A X X B X X
fire:         ^   ^     ^
```

Canonical handshake used by AMBA AXI4/ACE, AXI4-Stream, and most on-chip
streaming pipelines. Reference: **ARM IHI 0022**, *AMBA AXI and ACE
Protocol Specification*, §A3 "Single Interface Requirements".

### 2. `valid_only` — fire-and-forget

Every cycle with `valid == H` is an unconditional transfer. The consumer
cannot stall the producer; it must be ready to sample whenever `valid`
asserts.

```
cycle:    0 1 2 3 4 5 6 7
valid:    . H . H H . H .
payload:  X A X B C X D X
fire:       ^   ^ ^   ^
```

Used for strobes, interrupts, and data paths fronted by a FIFO that
absorbs burstiness. Reference: AXI4-Stream simplified-mode; Intel Avalon
Streaming `valid-only` variant.

### 3. `ready_only` — pull model

Producer drives payload continuously; consumer pulses `ready` on the
cycle it consumes a sample. Every cycle with `ready == H` is a transfer.

```
cycle:    0 1 2 3 4 5 6 7
ready:    . H . . H H . .
payload:  A B C D E F G H
fire:       ^     ^ ^
consumed:   B     E F
```

Rare on-chip; shows up in register files read by a consumer that knows
values are always valid. Reference: Intel Avalon MM pull-read mode;
classical combinational register file.

### 4. `valid_stall` — inverted backpressure

Same as `valid_ready` with the polarity of the back-signal inverted:
transfer occurs when `valid && !stall`. Stall, when asserted, freezes the
payload. Common in custom pipeline designs where the stall network is
derived from downstream full-flags.

```
cycle:    0 1 2 3 4 5 6 7 8
valid:    . H H H H H . . .
stall:    . . H H . . . . .
payload:  X A A A A B X X X
fire:         -   ^ ^
               (stalled)
```

Equivalent information content to `valid_ready`; offered because real
pipelines sometimes have a natural "stall" signal and inverting it at
every channel boundary is noise. Reference: Cortex-A pipeline interlock
conventions; Chisel3 `Decoupled` with `stall` wrapper.

### 5. `req_ack_4phase` — return-to-zero handshake

One transfer per `req/ack` pair. Sequence: producer raises `req` with
payload, consumer raises `ack`, producer drops `req`, consumer drops
`ack`. All four transitions happen before the next transfer can start.

```
cycle:    0 1 2 3 4 5 6 7 8 9
req:      . H H H . . H H H .
ack:      . . H H . . . H H .
payload:  X A A A X X B B B X
fire:         ^         ^
```

Classical asynchronous handshake (also usable synchronously for
GALS bridges). Reference: **Sparsø & Furber**, *Principles of
Asynchronous Circuit Design*, ch. 2, "Handshake Protocols — Four-Phase
Bundled-Data."

### 6. `req_ack_2phase` — non-return-to-zero (NRZ) handshake

Each *toggle* of `req` (regardless of direction) signals a new transfer;
a matching toggle of `ack` confirms it. Half the transitions per
transfer compared to 4-phase — faster on links where transitions are
expensive.

```
cycle:    0 1 2 3 4 5 6 7 8 9
req:      . H H H H . . . . .     (toggle ↑ at cycle 1)
ack:      . . H H H H H H H H     (toggle ↑ at cycle 2)
payload:  X A A A A A A A A A
fire:         ^

req:      H H H H H . . . . .     (toggle ↓ at cycle 4 — new transfer)
ack:      H H H H H H H . . .     (toggle ↓ at cycle 7)
payload:  A A A A B B B B B B
fire:             ^
```

Used in high-speed async links and off-chip serdes where transition
count dominates power. Subtle to verify — the compiler's auto-emitted
`X_req_toggle_exactly_once` assertion (Tier 2) catches the usual
implementation bugs. Reference: same Sparsø & Furber, ch. 2,
"Two-Phase (NRZ) Handshake."

### Quick-pick guidance

| Need | Pick |
|---|---|
| AMBA-family interfaces, most on-chip streaming | `valid_ready` |
| Strobes, interrupts, anything a FIFO fronts | `valid_only` |
| Pipeline interlock where "stall" is the natural signal | `valid_stall` |
| Async / GALS bridge, low complexity | `req_ack_4phase` |
| Async / GALS bridge, low power per transfer | `req_ack_2phase` |
| Read port of a combinational register file | `ready_only` |

## Tier 1.5 — auto-guard on payload (SHIPPED)

*Status: both PRs merged. Prerequisite (bus-input `--check-uninit` tracking)
landed in #23; Option D producer-side runtime guard in #25; Option A
consumer-side compile-time lint in #26.*

**End-to-end handshake correctness coverage** as of v0.43.0:

| Axis | Mechanism | Stage | PR |
|---|---|---|---|
| Port shape | Compile-time expansion into flat `PortDecl`s | `arch check` | #21 (Tier 1) |
| Protocol wire-timing | Auto-emitted concurrent SVA | `arch build` → Verilator / EBMC | #21 (Tier 2) |
| Producer payload bug | `--inputs-start-uninit` warning gated on valid/req | `arch sim` | #25 (Option D) |
| Consumer payload bug | `arch check` lint for unguarded payload reads | `arch check` | #26 (Option A) |

The rest of this section documents the design as it stood pre-merge; kept
for historical context.

Tier 1.5 splits into two logically independent detection paths. Each targets
a different bug class and runs at a different stage. Shipping both covers the
full "handshake payload correctness" story without silencing legitimate
patterns.

### Bug classes to catch

1. **Producer bug — "valid asserted, payload never driven"**
   The producer module or its testbench sets `X_valid` (or `X_req`) high on
   cycle N without ever writing the payload reg that feeds `X_data`. At
   simulation time, the consumer reads garbage. In 4-state simulators this
   shows up as X-propagation; in ARCH's 2-state sim today it's silent.

2. **Consumer bug — "payload read without checking valid"**
   The consumer module reads `port.X_data` unconditionally — or in a branch
   whose condition isn't derived from `port.X_valid`. Even when the producer
   behaves perfectly, the consumer's logic now depends on stale/undefined
   payload contents during cycles when valid is low. This is the contract
   violation Tier 2 SVA cannot catch (it's a consumer-side semantic bug,
   not a protocol-level wire-timing bug).

### Detection options considered

| Option | Class caught | Where it runs | Cost | False-positive risk |
|---|---|---|---|---|
| **A. Compile-time lint** on payload reads outside an `if <port>.<valid>` scope | Consumer | `arch check` | Small — AST walk | Low — miss only when guard is carried through a wire (`let g = b.valid; if g ...`) |
| **B. Runtime shadow bit** "valid has been high since reset"; warn on payload read while bit is false | Consumer | `arch sim` (stateful) | Medium — per-handshake counter + read-site instrumentation | Medium — false positive when consumer reads unconditionally then gates downstream (legal but flagged) |
| **C. Formal SVA** "consumer's state change depending on payload implies valid was high at the source cycle" | Consumer | `arch formal` / EBMC | High — property hard to formulate generically | High — easy to write a property that's either trivially true or trivially false |
| **D. Guarded `--check-uninit` warning** (reuse existing machinery): uninit warning on payload input fires only when the channel's valid is asserted | **Producer** | `arch sim --inputs-start-uninit` | Small — reuses #23 shadow-bit infra | Very low — only silences the exact "valid low so payload doesn't matter" case |

### Decision

- **Ship D first (producer-side, runtime).** Smallest diff, zero risk, directly
  reuses the machinery from #23. Catches the producer-bug class when TB is
  driving the inputs. Implementation: for every bus-flattened In signal that
  belongs to a handshake payload, gate the existing warning emission on the
  channel's valid signal — warn only when `valid && !_vinit`. Silences the
  legitimate case (TB didn't drive payload but also isn't asserting valid)
  without weakening the producer-bug detection.

- **Ship A next (consumer-side, compile-time).** Catches the consumer-bug
  class at `arch check` time with zero runtime overhead. Simple AST walk:
  for each comb/seq block inside a module with a handshake-using bus port,
  flag any read of `<port>.<payload_field>` whose enclosing conditional
  context does not include `<port>.<valid_field>` (directly, or via a
  single-let indirection). Emits a warning, not an error — a user who
  reads payload unconditionally and gates downstream can suppress via
  an explicit comment or we extend to track let-bindings.

- **Skip B.** Its false-positive profile is worse than A's false-negative
  profile. A covers the same bug class earlier (compile-time) and more
  precisely. B would only earn its keep if A's tracking-through-lets became
  unmaintainable, which is unlikely at this scale.

- **Skip C.** Too expensive to design a generic property; Tier 2 already
  covers the protocol-level wire-timing properties formally; payload-use
  correctness is structural, not temporal, and belongs in the compile-time
  lint.

### Variants covered

D (runtime guard on uninit warning):
- `valid_ready`, `valid_only`, `valid_stall` → guard on `<port>_<ch>_valid`
- `req_ack_4phase` → guard on `<port>_<ch>_req`
- `ready_only` → no guard (no valid/req signal); warning fires unconditionally
- `req_ack_2phase` → skipped; stateful toggle guard (`req ^ req_d`) deferred

A (compile-time lint):
- All variants that have a valid/req signal use it as the expected guard.
- `ready_only` is semantically "producer drives continuously" — reads are
  always legitimate, lint skips the variant.
- `req_ack_2phase` skipped for the same stateful-guard reason.

### Implementation roadmap

**PR #1 — Option D (producer-side runtime guard)**
1. In `sim_codegen.rs`, when emitting the --check-uninit warning for a
   flattened bus input signal, look up whether the signal is part of a
   handshake payload via the bus's `HandshakeMeta` list.
2. If so, and the variant has a valid/req signal, wrap the warning
   condition: `(!_{name}_vinit && {port}_{ch}_valid && !_w_{name})`.
3. Tests: bad TB that asserts valid without setting payload → warns;
   bad TB that leaves valid low and never sets payload → silent (no
   spurious warning).

**PR #2 — Option A (consumer-side compile-time lint)**
1. New pass in `typecheck.rs` or a sibling lint module: walks comb/seq
   bodies in every module that has bus ports with handshake channels.
2. For each read of `<port>.<payload_field>`, check the enclosing
   conditional chain for a condition that is (a) `<port>.<valid>`
   directly, (b) a negation of `!<port>.<valid>`, or (c) a reference
   to a `let` binding whose RHS is `<port>.<valid>`.
3. If no guard is in scope, emit a warning (not an error — users can
   always intentionally read unconditionally).
4. Tests: unguarded read → warn; `if port.valid` guard → silent;
   let-indirection guard → silent; unrelated branch → warn.

### Out of scope (both PRs)

- Producer-side compile-time check of "reg feeding payload port has a
  written-value reaching valid=1". Requires tracing comb/let chains
  from port back to reg; genuinely harder. Tier 2 SVA + EBMC already
  catch this class formally.
- `req_ack_2phase` guard semantics. The guard is `req ^ req_d`, which
  is a one-cycle-history expression. Doable but deferred.
- Auto-fix suggestions in the consumer-side lint. The lint message
  will include a pointer at the channel name; users manually add the
  `if` guard.

---

## Tier 1.5 — auto-guard on payload (original sketch)

Because the compiler now *owns* the knowledge that `X_valid` (or `X_req`)
gates `X_<field>`, every payload port receives an **implicit `guard`
annotation** equivalent to the user writing:

```
reg/port X_field: T guard X_valid;
```

Effects:
- **Consumer side** (`handshake X: receive`): `--check-uninit` stays silent at the
  use site when the consumer qualifies its reads with `if X_valid` (or
  conditions derived from it) — no manual `guard` annotation needed.
- **Producer side** (`handshake X: send`): the compiler requires that every
  payload field reg has *some* driver whose activity window covers the
  cycles where `X_valid` is asserted. If a producer asserts `X_valid` while
  a payload reg was never written in a path that reaches `X_valid=1`,
  `--check-uninit` fires with a diagnostic pointing at the channel name —
  the classic "producer bug" that today only shows up as X-propagation in
  4-state simulation.

For `req_ack_2phase`, the valid-qualifier is `X_req ^ X_req_d` (toggle edge)
rather than `X_req`. The compiler encodes this; the user never writes it.

`ready_only` has no natural valid signal for guarding — payload is
interpreted as "producer drives continuously, consumer decides when to
consume." No auto-guard applies; `--check-uninit` behaves as if the user
wrote no guard at all.

## Tier 2 — auto-emitted protocol assertions (SHIPPED — initial set)

**v1 coverage**:
- `valid_ready`: `_auto_hs_<port>_<ch>_valid_stable` — once valid is asserted it stays asserted until ready is observed.
- `valid_stall`: `_auto_hs_<port>_<ch>_valid_stable_while_stall` — valid must not change while stall is asserted.
- `req_ack_4phase`: `_auto_hs_<port>_<ch>_req_holds_until_ack` — req stays asserted until ack is observed.
- `valid_only`, `ready_only`, `req_ack_2phase`: parsed + ports expand correctly, but no Tier-2 assertion emitted yet (valid_only has no back-signal; ready_only has no valid; 2-phase req-toggle needs `$past` tracking — deferred).

All properties are concurrent SVA wrapped in `synopsys translate_off/on`, using the module's first Clock port and `disable iff (<reset>)` on the first Reset port — same convention as `_auto_bound_*` / `_auto_div0_*`. Modules with no clock skip assertion emission (the ports still expand).

Remaining work documented below for future extensions:



When `arch build` emits SV, each handshake additionally emits a small
block of concurrent SVA (inside `synopsys translate_off/on`, same
mechanism as the existing `_auto_bound_*` and `_auto_div0_*` assertions).

Assertions per variant:

### `valid_ready`
- `X_valid_stable_until_ready`: once `X_valid` is asserted, it must stay
  asserted until the cycle `X_ready` is seen. (Standard AXI-style
  liveness-of-handshake rule.)
- `X_payload_stable`: each payload field must hold its value across cycles
  while `X_valid` is asserted and `X_ready` is not yet seen.
- `X_no_valid_during_reset`: `X_valid` must be deasserted for the duration
  of reset.
- `X_ready_no_dependence_on_valid` (cover, not assert): the consumer's
  `X_ready` should not combinationally depend on `X_valid` to avoid
  handshake deadlock. Emitted as a documentation lint, not a hard assert,
  because legitimate bridges sometimes violate this.

### `valid_only`
- `X_no_valid_during_reset`.
- No stability requirement — producer may glitch valid arbitrarily;
  consumer samples.

### `valid_stall`
- `X_valid_stable_while_stall`: if `X_stall` is asserted, `X_valid` must
  not change until `X_stall` releases.

### `req_ack_4phase`
- `X_req_before_ack`: `X_ack` may only be asserted if `X_req` is asserted.
- `X_req_holds_until_ack`: `X_req` stays high until `X_ack` is observed.
- `X_ack_drops_before_req`: after handshake, both must return to 0 before a
  new transaction.

### `req_ack_2phase`
- `X_req_toggle_exactly_once`: between any two `X_ack` toggles, `X_req`
  toggles exactly once.

Each assertion inherits the module's reset polarity and clock, same as
`_auto_bound_*`. Labels follow the pattern `_auto_hs_<channel>_<rule>`.

## Interaction with existing constructs

- **`bus` + `generate_if`**: orthogonal. A `handshake` can sit inside a
  `generate_if` branch and is expanded per-branch exactly like individual
  port declarations today.
- **`initiator` / `target` flip**: applies to every signal a handshake
  expands into, including valid/ready. A bus declared with `handshake aw:
  out kind: valid_ready` attached as a `target` bus port yields
  `aw_valid: in`, `aw_ready: out`, and payload `in`.
- **`--check-uninit`**: uses the compiler-synthesized guard (Tier 1.5) as
  if the user wrote `reg ... guard X_valid;` on each payload.
- **`--debug` sim instrumentation**: prints handshake events as
  `[cycle][Mod.X] FIRE aw_addr=0x…` on each cycle where `valid & ready`
  holds. (Nice-to-have; not strictly needed for v1 of this feature.)
- **EBMC / formal**: the auto-emitted SVA becomes formal properties for
  free. Protocol violations that today show up only as Verilator asserts
  become BMC-provable properties.

## Open questions

1. **Single-line shorthand?** Channels with zero payload (e.g. a pure
   interrupt wire `handshake irq: send kind: valid_ready end`) look noisy.
   Could allow `handshake irq: send kind: valid_ready;` (no body) as a
   one-liner when the payload is empty.
2. **Payload sub-ports in SV output**: keep flattened (`aw_addr`, `aw_id`)
   to match the existing convention, or introduce optional `struct packed`
   grouping at SV level for verification-tool ergonomics? Recommendation:
   keep flat — the existing `bus` convention is load-bearing for tool
   compatibility (Verilator, EBMC, SymbiYosys all work cleanly today).
3. **Direction naming**: `out` / `in` follow payload-flow direction, which
   matches intuition but differs from some SV interface conventions
   (master/slave, initiator/target). Since `bus` already uses
   `initiator`/`target` for perspective, keeping `in`/`out` here for the
   channel direction is consistent with port declarations elsewhere.
4. **Tier 3 escape hatch**: when a stateful protocol (credit-based) is
   needed, does the user get a `handshake … kind: raw` that emits only
   the valid/payload skeleton and lets them write the counter by hand?
   Recommendation: no — point them at the future `credit_channel`
   construct. Keeping `handshake` to stateless shapes preserves the clean
   ADT semantics.

## Implementation roadmap

### Step 1 — parse + AST
- Add `HandshakePort` variant to `ast::BusBodyItem`.
- Parser accepts the grammar above; validates that `kind` is a known
  variant name.
- AST carries: channel name, direction, variant enum, payload fields (name,
  type, optional reset).

### Step 2 — elaboration-time expansion
- In `elaborate.rs`, when lowering a `bus` definition, expand each
  `HandshakePort` into a synthesized sequence of flat port declarations
  using the variant catalog above.
- The expansion preserves span info pointing back at the handshake block,
  so downstream errors still highlight the user's source.

### Step 3 — codegen (Tier 1 complete)
- No changes in `codegen.rs` — expansion produces ordinary ports that the
  existing bus flattening path emits unchanged.
- No changes in `sim_codegen.rs` for the same reason.

### Step 4 — auto-guard (Tier 1.5)
- Attach a synthetic `guard` field to each expanded payload port pointing
  at the channel's valid signal (or toggle-edge expression for 2-phase).
- `--check-uninit` machinery already respects `guard`; no further wiring
  needed.

### Step 5 — auto-emitted SVA (Tier 2)
- New module `formal::handshake_assertions` produces the per-variant
  assertion block, invoked from `codegen.rs` after emitting the bus ports.
- Each assertion uses the existing `_auto_*` labeling and
  `translate_off/on` wrapping convention.

### Step 6 — documentation + tests
- Spec: add §12.x to `ARCH_HDL_Specification.md` documenting the variants,
  direction rule, and auto-guard/auto-assertion behavior.
- AI reference card: one line per variant with port expansion shown.
- Tests:
  - Integration: `tests/handshake_valid_ready.arch` exercising a producer
    and consumer connected via a bus using the new primitive.
  - Snapshot: SV expansion with all 6 variants.
  - Formal: `arch formal` on a handshake-using module; observe the
    auto-emitted `_auto_hs_*` assertions among the proved properties.

### Step 7 — migration aid
- `examples/dma_engine.arch` and `tests/axi_dma_multi/BusAxi4.arch` are
  the canonical before/after pair: rewrite them using `handshake` to
  validate the primitive in a realistic setting and to exhibit the
  line-count / correctness improvement in the commit message.

## Not designed here

- `credit_channel` and any other stateful-protocol first-class constructs.
- Automatic SV struct grouping of payload (open question above).
- Debug-sim fire-event printing (open question above).
- Handshake *composition* (e.g. AXI4-full as a collection of handshakes
  with cross-channel ordering rules). The current `bus` composition
  mechanism is sufficient; no new machinery needed.

## Risk of leaky abstraction

The main risk is users needing *slightly* different semantics than a
variant provides — e.g. a protocol that's like `valid_ready` but allows
`valid` to deassert mid-transaction. The mitigation: all six variants
are port-shape-only at Tier 1, so a user whose protocol doesn't fit can
fall back to hand-rolled `bus` declarations with zero loss. Tier 2
assertions are opt-out via a `no_assert` modifier (`handshake X: send
kind: valid_ready no_assert`) for when the user knows the protocol
diverges from the canonical rules.
