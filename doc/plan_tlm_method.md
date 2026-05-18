# Plan: `tlm_method` — transaction-level bus sub-construct

*Author: session of 2026-04-22. Status: implemented in stages; updated 2026-05-13 for looped/locked initiator call-site lowering and bounded generated mux trees.*

> **Implemented surface.** Current `tlm_method` syntax is only:
> `tlm_method name(args) -> Ret: blocking;` or
> `tlm_method name(args) -> Ret: out_of_order tags N;` inside a `bus`.
> Target implementations are dotted-name threads:
> `thread port.method(args) on clk rising, rst high ... return expr; end thread port.method`.
> Tagged OOO targets may use indexed lanes:
> `thread port.method[t](args) ... return expr; end thread port.method`.
> Initiator calls are legal only inside `thread` bodies as a direct RHS
> assignment (`dst <= port.method(args);`) or as an RHS-fork issue
> (`dst <= fork port.method(args); ... join all;`). Counted `for` loops
> and `if`/`elsif`/`else` branches inside initiator threads may contain
> serialized direct TLM assignments; literal loops are unrolled and runtime
> loops lower to a loop counter. RHS-fork groups may include a compute-only
> tail after `join all;`. `lock RESOURCE ... end lock RESOURCE`
> is accepted around initiator TLM calls and uses the matching
> `resource RESOURCE: mutex<POLICY>;` declaration for shared-method
> arbitration (`mutex<round_robin>` emits a rotating grant pointer; otherwise
> the compiler uses default priority). The compiler rejects TLM calls in
> `comb`, `seq`, module-level `let`, module-local `function`, and
> `pipeline`/`fsm` contexts. Use `generate_for`
> worker threads when a compile-time number of independent workers is needed.
> Fixed-size `Vec<T, N>` returns and response structs containing Vec fields are
> supported for bounded burst-like payloads.
> There is no current `Future<T>`, `await`, user-visible `Token<T>`,
> `pipelined`, or `burst` API.

Cross-refs:
- `doc/plan_bus_unification.md` — sets the unified-bus frame. `tlm_method` is
  one of the four sub-constructs nested inside `bus` (stateless sig-level
  `handshake_channel`, stateful sig-level `credit_channel`, stateless
  txn-level bus raw signals, **stateful txn-level `tlm_method`**).
- `doc/bus_spec_section.md` §19.2 — current bus/TLM user-facing syntax and
  constraints.

## The pitch

Transaction-level methods collapse what is otherwise a hand-rolled FSM
per call site: a module that wants to issue an AXI read writes
`val <= m_axi.read(addr);` inside a thread, and the compiler
generates the AR-channel valid/ready shake, the cycle-accurate response
wait, and the FSM stalling for the enclosing thread. On the target
side, the user writes a `thread s.read(addr) ... return mem[addr]; end`
body; the compiler wires it to the response channel.

`tlm_method` sits beside `handshake_channel` and `credit_channel` under
the unified `bus` umbrella. It is the **transaction-level** + **stateful**
quadrant of the 2×2 — stateful because the compiler owns request/response
FSMs and any outstanding-transaction bookkeeping.

## Historical v1 scope (superseded by the implemented surface above)

To keep the feature tractable and reviewable, v1 is **`blocking` mode
only**. In-order thread-cohort lowering, out-of-order tagged routing,
and burst support follow as independent PRs once the blocking
foundation is solid and the wire protocol is validated.

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

Explicitly **deferred** beyond the blocking foundation:
- In-order generated-thread mapping for a finite worker cohort sharing
  one method.
- Out-of-order mode via compiler-managed request/response tags.
- Burst-oriented protocol support, if a viable explicit beat-stream
  lowering is found.
- `max_outstanding` / `timing` annotations from the original sketch.
- Richer `fork` / `join` lowering around non-trivial TLM call bodies.
- Older file-scope `implement X.m rtl` protocol-level mapping. Current code
  uses dotted target threads / thread-header `implement target` instead.
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
                | 'out_of_order' 'tags' ConstExpr  // implemented after v1
                | 'burst'        // rejected/deferred beat-stream protocol
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

Literal counted loops are unrolled before this state-machine lowering:

```
thread driver on clk rising, rst high
  for i in 0..7
    ack <= m.read(i.zext<32>());
  end for
end thread driver
```

The unrolled call sites still drive one physical request/response interface
for `m.read`; they do not create multiple SystemVerilog drivers.

When multiple thread call sites share a method, use `lock` plus a `resource`
declaration to make the arbitration policy explicit:

```
resource mem_ch: mutex<round_robin>;

generate_for lane in 0..3
  thread Worker_lane on clk rising, rst high
    lock mem_ch
      ack[lane] <= m.read(lane.zext<32>());
    end lock mem_ch
  end thread Worker_lane
end generate_for
```

The compiler emits a single method driver and selects the active call site.
`mutex<round_robin>` emits round-robin arbitration for simultaneous workers.
Unprotected grouped call sites are still electrically single-driver, but use
the compiler's default priority selection; in a single sequential thread those
call sites are normally mutually exclusive FSM states.

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

For grouped/looped initiator call sites, the generated request-valid,
response-ready, payload mux, and arbitration expressions are split into
intermediate wires. This keeps generated SystemVerilog line lengths bounded
for large unrolled TLM traces and avoids simulator/preprocessor limits such
as Verilator's per-line token cap.

### Target side

The declared `thread port.method(args)` body is lowered like any other
`thread`, but with three additions:
- Entry state is guarded on `req_valid`; on entry, arg-named thread
  locals are assigned from the request bus signals.
- `req_ready` is pulsed in the entry state (1-cycle accept).
- `return expr;` inside the body lowers to a response state that drives
  `rsp_valid = 1`, `rsp_data = expr`, then waits for `rsp_ready` before
  returning to the entry state.

The implemented target lowering accepts branch-local `return` points. Rich
control flow before those returns (assignments, waits, `if`, counted `for`,
and `lock`) reuses ordinary thread lowering. Each return lowers to a generated
response state that drives the selected `rsp_data` expression; statements after
`return` in the same block are rejected.

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

### Implemented follow-on slices

- PR-tlm-V2a: in-order thread-cohort mapping for ordinary blocking
  calls. A `generate_for` cohort, explicit worker threads, or direct-call
  `fork ... join` branches may share a `(port, method)` pair; the
  compiler lowers the group to an issue arbiter plus in-order response
  router. No new `tlm_method` mode and no `Future<T>` type. See
  `doc/plan_tlm_pipelined.md`.
- PR-tlm-V2b: out-of-order mode via compiler-managed request/response
  tags. Keep the worker syntax; change the protocol contract so the
  compiler can route by `rsp_tag` instead of FIFO issue order. Parser,
  wire flattening, target tag echo, single-thread tag defaults, and
  cohort tag routing are implemented in the thread-cohort branch.
- PR-tlm-V2c: serialized runtime-bounded initiator `for` loops. Direct
  blocking TLM assignments inside a runtime loop lower to loop-counter
  issue/wait states; each iteration completes before the next begins.
- PR-tlm-V2d: richer target method bodies. Target-side dotted threads
  reuse ordinary thread lowering before generated response states,
  enabling assignments, waits, `if`, counted `for`, `fork`/`join`, and
  `lock` blocks.
- PR-tlm-V2e: branch-local target returns. Target-side returns inside
  conditional bodies lower to per-return response states so response data
  still observes prior nonblocking updates.
- PR-tlm-V2f: indexed target response arbitration. Tagged OOO target lanes
  now feed a generated response-channel arbiter before driving the shared
  response handshake. Default policy is priority; wrapping the lane return in
  `lock RESOURCE ... end lock RESOURCE` uses the matching module-scope
  `resource RESOURCE: mutex<POLICY>;` policy for that method's response
  channel. This is still an atomic response payload, not beat-stream burst
  interleaving.

### Future / deferred

- First-class beat-stream / burst protocol support remains deferred. The
  current compiler supports bounded burst-like payloads through static
  `Vec<T, MAX>` returns or response structs carrying `data`, returned
  `len`, and `resp`; this is not a dynamic-length return type.
- Richer TLM initiator control flow beyond serialized `for` loops and
  `if`/`elsif`/`else` branches remains deferred. Today, call sites must stay
  direct RHS assignments or RHS-fork assignments; nested/composed TLM
  expressions are still rejected.
- One-to-many decoded interconnect remains explicit router code. `connect
  a.m -> b.s;` is point-to-point sugar; address decode and decode-error
  response ownership belong in a router module.

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
   supported when they match the finite cohort shapes listed above.
   Blocking cohorts route by issue-order FIFO; `out_of_order tags N`
   cohorts route with compiler-managed tags, not `Future<T>`.
5. **Call site outside a thread**: compile error with a targeted
   message pointing the user at `thread X ... end thread X`.
6. **Shared method body across multiple ports on target**: out of v1.
   Declare separate `thread s1.read` / `thread s2.read`, or refactor
   shared logic into a `function`.

## Non-goals

- Not committing to v2 timeline. Each v2 mode is its own plan.
- Not addressing TLM-to-RTL timing annotations (`timing: N cycles` from
  the old sketch). ARCH TLM is now defined as synthesizable TLM, so v1 cycles
  are whatever the thread/FSM lowering produces.
- Not designing separate LT/AT simulation modes. `tlm_method` compiles to
  ordinary RTL-shaped state machines and runs under existing `arch sim`,
  `arch sim --thread-sim both`, `arch sim --pybind --test`, and Verilator.
