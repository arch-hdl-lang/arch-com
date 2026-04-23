# Plan: `tlm_method` — transaction-level bus sub-construct

*Author: session of 2026-04-22. Status: design draft, not yet implemented.*

Cross-refs:
- `doc/plan_bus_unification.md` — sets the unified-bus frame. `tlm_method` is
  one of the four sub-constructs nested inside `bus` (stateless sig-level
  `handshake_channel`, stateful sig-level `credit_channel`, stateless
  txn-level bus raw signals, **stateful txn-level `tlm_method`**).
- `doc/bus_spec_section.md` §19.2 — the original pre-unification TLM sketch
  (`methods ... end methods` block with `implement X.m rtl ... end`). This
  plan supersedes the grammar there; the *semantics* carry over.

## The pitch

Transaction-level methods collapse what is otherwise a hand-rolled FSM
per call site: a module that wants to issue an AXI read writes
`let val = m_axi.read(addr);` inside a thread, and the compiler
generates the AR-channel valid/ready shake, the cycle-accurate response
wait, and the FSM stalling for the enclosing thread. On the target
side, the user writes a `thread s.read(addr) ... return mem[addr]; end`
body; the compiler wires it to the response channel.

`tlm_method` sits beside `handshake_channel` and `credit_channel` under
the unified `bus` umbrella. It is the **transaction-level** + **stateful**
quadrant of the 2×2 — stateful because the compiler owns request/response
FSMs, Future/Token synchronization state, and outstanding-transaction
bookkeeping.

## v1 scope (what ships first)

To keep the feature tractable and reviewable, v1 is **`blocking` mode
only**. Pipelined / out_of_order / burst modes follow as independent
PRs once the blocking foundation is solid and the wire protocol is
validated.

Shipping in v1:
1. Grammar for `tlm_method name(args) -> ret: blocking;` inside a bus.
2. Grammar for target-side `thread p.method(args) ... return expr; end thread p.method`.
3. Grammar for initiator-side call-site expressions `let x = m.method(args);`
   inside a `thread` body (threads already have `wait until` lowering).
4. Synthesis of the two-channel wire protocol per method:
   - **Request channel** (initiator → target): valid + {args...}, ready-back.
   - **Response channel** (target → initiator): valid + ret payload,
     ready-back.
5. Initiator call-site elaboration: issue request, suspend the enclosing
   thread until response arrives, destructure the ret payload into the
   receiving `let` binding.
6. Target thread-body elaboration: the declared `thread s.method(args)`
   body becomes a reusable FSM that accepts requests, optionally
   suspends on `wait until`, and issues the response when it hits `return`.
7. Tier-2 SVA: `req_valid_stable_until_ready`, `rsp_valid_stable_until_ready`,
   `rsp_only_when_req_outstanding` (one outstanding in blocking mode).
8. sim_codegen mirror (C++ request/response FIFOs of depth 1, following
   the credit_channel / sim_credit_channel module pattern).

Explicitly **deferred** to v2:
- `pipelined` mode + `Future<T>` type + `await` primitive.
- `out_of_order` mode + `Token<T, id_width: N>`.
- `burst` mode + `Future<Vec<T, L>>`.
- `max_outstanding` / `timing` annotations from the original sketch.
- `fork` / `join` inside thread bodies (already a thread v2 topic).
- `implement X.m rtl` protocol-level mapping (covered by threads today).
- Multiple initiators on one method (arbiter composition at target).

## Syntax (v1, blocking only)

### Bus declaration

```
bus Mem
  param ADDR_W: const = 32;
  param DATA_W: const = 64;

  tlm_method read(addr: UInt<ADDR_W>) -> UInt<DATA_W>: blocking;
  tlm_method write(addr: UInt<ADDR_W>, data: UInt<DATA_W>) -> Bool: blocking;
end bus Mem
```

Grammar addition (one line per method — trailing `: blocking` is the
concurrency mode for v1; no body yet):
```
TlmMethod      := 'tlm_method' Ident '(' ParamList ')' ('->' TypeExpr)? ':' Mode ';'
Mode           := 'blocking'     // v1
                | 'pipelined'    // v2
                | 'out_of_order' // v2
                | 'burst'        // v2
ParamList      := (Ident ':' TypeExpr (',' Ident ':' TypeExpr)*)?
```

Zero-arg and void-return (`-> Bool`-only as success/fail) are both valid.

### Target-side implementation

The target implements each method as a `thread` whose name is the
method path `port.method`. The body runs once per received request;
locals inside the thread are per-request.

```
module MemTarget
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port s:   target Mem;
  ram cells: UInt<DATA_W> [1 << ADDR_W];

  thread s.read(addr)
    wait until cells.ready;
    return cells.read(addr);
  end thread s.read

  thread s.write(addr, data)
    wait until cells.ready;
    cells.write(addr, data);
    return 1'b1;
  end thread s.write
end module MemTarget
```

Method args are bound as thread-local names. `return expr;` terminates
the thread iteration and drives the response channel. Type checking
verifies the return expression matches the declared ret type.

### Initiator-side call

Call sites live **inside a thread** (they contain implicit waits, so
they cannot run in pure `comb` or `seq`):

```
module MemInitiator
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port m:   initiator Mem;
  port reg last_val: out UInt<DATA_W> reset rst => 0;

  thread driver
    let d0 = m.read(32'h1000);          // suspends until response
    let d1 = m.read(32'h1004);
    last_val <= d0 + d1;
    let _ack = m.write(32'h2000, d0);
  end thread driver
end module MemInitiator
```

Call-site semantics (blocking mode):
1. Assert request valid + drive args. Hold until ready goes high.
2. Wait for response valid. Capture ret payload.
3. Deassert request; fall through to the next statement.

Parser recognizes `<bus_port>.<method_ident>(args)` as a TLM call
expression when `<bus_port>` has `bus_info` and the method name is a
declared `tlm_method` on that bus. Inside a thread, the call lowers to
a state-machine fragment that blocks on the request-ready and
response-valid handshakes.

## Wire protocol (v1, blocking)

Per `tlm_method name(args...) -> ret: blocking;` the compiler flattens
**two handshake channels** at the bus port:

| Channel | Direction (from initiator) | Signals |
|---|---|---|
| request  | out | `<name>_req_valid`, `<name>_<arg>` per arg, `<name>_req_ready` in |
| response | in  | `<name>_rsp_valid` in, `<name>_rsp_data` in, `<name>_rsp_ready` out |

(If the declared return type is `Bool`, `rsp_data` is still emitted as a
1-bit signal for uniformity; a future optimization can elide it when
the handshake alone carries the ack.)

Direction flip on `target` perspective: req becomes input side, rsp
becomes output side — same mechanism `handshake_channel` uses.

Invariants (auto-SVA, Tier 2):
- `req_valid |=> req_valid` until `req_ready` (valid-stable).
- `rsp_valid |=> rsp_valid` until `rsp_ready` (valid-stable).
- `rsp_valid` only when there is an outstanding request (v1: at most
  one outstanding per method, trivially satisfied by the
  initiator-side blocking FSM).

## Lowering

### Initiator side

Per call site `let x = m.method(arg0, arg1);` inside a thread body, the
thread-lowering pass synthesizes two state transitions:

```
ISSUE_method_N:
  m.method_req_valid = 1;
  m.method_arg0 = <arg0 expr>;
  m.method_arg1 = <arg1 expr>;
  wait until m.method_req_ready;
  → WAIT_method_N

WAIT_method_N:
  m.method_req_valid = 0;
  m.method_rsp_ready = 1;
  wait until m.method_rsp_valid;
  x <= m.method_rsp_data;
  → <next user state>
```

Each call site gets a monotonically-numbered pair of states. The
existing `wait until` machinery is reused, so no new scheduling
primitive is needed.

### Target side

The declared `thread port.method(args)` body is lowered like any other
`thread`, but with three additions:
- Entry state is guarded on `req_valid`; on entry, arg-named thread
  locals are assigned from the request bus signals.
- `req_ready` is pulsed in the entry state (1-cycle accept).
- `return expr;` inside the body lowers to a response state that drives
  `rsp_valid = 1`, `rsp_data = expr`, then waits for `rsp_ready` before
  returning to the entry state.

Multiple `return` points in the body (from nested conditions) all route
to the same response state by staging the `rsp_data` into a thread-local
before branching.

## Interaction with existing constructs

- **`thread`**: unchanged for autonomous sequencers. The TLM target side
  reuses the same lowering with a few extra rules (method entry gate,
  arg binding, return-to-response mapping). Spec §7a stays correct;
  §7b (TBD) documents the method-bound flavor.
- **`handshake_channel`**: structurally what each of the two channels
  reduces to (same SVA rules, same direction-flip mechanism). An
  implementation option is to literally emit TLM as two internal
  `handshake_channel` entries plus the call-site / thread-body machinery.
  Prefer sharing the flattening code rather than duplicating it.
- **`credit_channel`**: orthogonal. A `bus` may carry both (TLM control
  plane + credit-channel data plane, for example).
- **`arch formal`**: the three Tier-2 SVA properties should PROVE on a
  matched pair. A mismatched pair (wrong method count/type) should
  REFUTE or bounds-error cleanly.
- **`--check-uninit`**: request args get the same treatment as
  handshake payloads — warn on read-before-valid at the target side.

## Implementation roadmap

### PR-tlm-0: plan doc (this document)

Lock the v1 scope and wire protocol. No code.

### PR-tlm-1: parser/AST scaffolding

- Lexer: `tlm_method` token.
- AST: `BusDecl.tlm_methods: Vec<TlmMethodMeta>` with name, args, ret,
  mode.
- Parser: recognize the one-line method declaration inside a bus body.
- Typecheck: reject any bus containing a `tlm_method` with "parser
  scaffolding only" until PR-tlm-2 lands (same guard pattern used for
  credit_channel PR #59).

### PR-tlm-2: wire flattening

- Extend `BusInfo::effective_signals` to emit the req/rsp signal pairs
  per `tlm_method`. Direction derived; target-perspective flip works
  out of the box.
- Lift the PR-tlm-1 scaffolding reject. Users can drive the flattened
  signals manually (same freedom as post-PR #60 credit_channel).
- Tests: assert SV port list contains the expected req/rsp signals
  for both initiator and target perspective.

### PR-tlm-3: target-side thread syntax + lowering

- Parser: `thread port.method(args) ... end thread port.method` —
  extends the existing thread syntax to accept a dotted name.
- Elaboration: walk module bodies, match each `thread port.method` to
  the declaring bus's `tlm_method`, inject arg bindings + entry gate
  + return-to-response transitions into the thread's lowered FSM.
- Tests: end-to-end compile of a simple target module.

### PR-tlm-4: initiator-side call-site lowering

- Parser / expression recognition: `port.method(args)` inside a thread
  body.
- Elaboration: synthesize the ISSUE/WAIT state pair per call site.
  Integrates with the existing `wait until` lowering.
- Tests: end-to-end initiator + target pair; SV compiles, Verilator
  simulates without assertions tripping.

### PR-tlm-5: Tier-2 SVA

Three auto-emitted properties labeled
`_auto_tlm_<port>_<method>_<rule>`, wrapped in `translate_off/on`.

### PR-tlm-6: sim_codegen mirror

- `src/sim_codegen/tlm_method.rs` (mirrors
  `src/sim_credit_channel.rs` pattern): emit request/response
  1-element FIFOs in C++, wire the thread FSM updates.
- Tests: `compile_to_sim_h` regression for a TLM initiator + target.

### PR-tlm-7: docs + spec section + end-to-end test

- Spec §18d (`tlm_method`) — grammar, wire protocol, lowering, SVA.
- Reference card entry.
- Canonical validation: a 2-module mem-initiator + mem-target pair
  exercising blocking `read` and `write`, plus a thread driver issuing
  a small sequence.

### Future (not in this plan)

- PR-tlm-V2a: `pipelined` mode + `Future<T>` type.
- PR-tlm-V2b: `out_of_order` mode + `Token<T, id_width: N>`.
- PR-tlm-V2c: `burst` mode + `Future<Vec<T, L>>`.
- PR-tlm-V2d: DMA test migration — rewrite `ThreadMm2s` / `ThreadS2mm`
  to use `tlm_method` and compare SV/behavior against the hand-rolled
  baseline (§`plan_bus_unification.md`).

## Resolved design decisions (locked 2026-04-22)

1. **Return keyword in TLM thread bodies**: reuse `return expr;`.
2. **Void methods**: allow `-> RetType` clause to be omitted; response
   channel becomes a bare valid/ready handshake with no data payload.
3. **Arg direction**: all args flow initiator → target on the request
   channel; the response flows via the separate `-> RetType` channel.
   No per-arg `out` keyword in v1 — multi-value returns pack into a
   struct as the ret type (composes with existing ARCH features,
   avoids committing to arg-direction grammar before we know we
   need it).
4. **Multiple threads touching the same method** in the same module:
   compile error in v1. Pipelined / OoO modes in v2 are the answer
   for genuine concurrency.
5. **Call site outside a thread**: compile error with a targeted
   message pointing the user at `thread X ... end thread X`.
6. **Shared method body across multiple ports on target**: out of v1.
   Declare separate `thread s1.read` / `thread s2.read`, or refactor
   shared logic into a `function`.

## Non-goals

- Not committing to v2 timeline. Each v2 mode is its own plan.
- Not addressing TLM-to-RTL timing annotations (`timing: N cycles` from
  the old sketch). Those are a spec feature for approximately-timed
  simulation; v1 cycles are whatever the FSM lowering produces.
- Not designing the `arch sim --tlm-lt` TLM-LT simulation mode. That
  bypass of RTL simulation is a separate project; v1 `tlm_method`
  compiles to ordinary RTL and runs under existing `arch sim
  --pybind --test` and Verilator.
