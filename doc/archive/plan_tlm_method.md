# Plan: Synthesizable TLM

*Status: implemented current subset. Last cleanup: 2026-05-17.*

This document is the living plan/status note for ARCH **synthesizable TLM**.
Older step-by-step PR roadmaps were completed or superseded and have been
removed from this file. Normative syntax lives in
`doc/ARCH_HDL_Specification.md` §18d and §22; compact usage guidance lives in
`doc/Arch_AI_Reference_Card.md`.

## Current Surface

`tlm_method` is a bus sub-construct:

```arch
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
  tlm_method read_ooo(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
end bus Mem
```

Target methods are implemented by dotted-name threads:

```arch
thread s.read(addr) on clk rising, rst high
  wait 2 cycle;
  return data_for(addr);
end thread s.read
```

Tagged target lanes use indexed dotted-name threads:

```arch
generate_for t in 0..3
  thread s.read[t](addr) on clk rising, rst high
    return lane_rsp[t];
  end thread s.read
end generate_for
```

Initiator calls are legal only inside `thread` bodies as direct RHS
assignments:

```arch
d <= m.read(addr);
d <= fork m.read(addr);
```

## Implemented Lowering

The compiler lowers TLM calls to ordinary RTL-shaped request/response wires and
state machines. `arch sim` is the executable semantics for that lowered design;
`arch sim --thread-sim both` and Verilator simulation are the equivalence and
codegen checks.

Implemented initiator shapes:

- Single blocking call: caller issues request, waits for response, captures
  return payload.
- Multiple direct named worker threads sharing one method.
- `generate_for` worker threads sharing one method.
- One direct-call `fork ... and ... join` cohort.
- Literal-bounded initiator `for` loops, unrolled before TLM lowering.
- Serialized runtime-bounded initiator `for` loops.
- Serialized `if`/`elsif`/`else` branches containing direct blocking calls.
- RHS-fork groups for timed multiple-outstanding issue:

  ```arch
  d0 <= fork m.read(addr0);
  wait 1 cycle;
  d1 <= fork m.read(addr1);
  join all;
  checksum <= d0 +% d1; // optional compute-only tail
  ```

Implemented target shapes:

- Assignments and waits before `return`.
- `if`/`elsif`/`else` with branch-local `return`.
- Counted `for` loops.
- `fork`/`join`.
- `lock` blocks.
- Indexed OOO target lanes with generated response arbitration. Default
  response arbitration is priority; wrapping lane `return` in
  `lock RESOURCE ... end lock RESOURCE` uses the matching
  `resource RESOURCE: mutex<POLICY>;` policy.

Implemented payload shapes:

- Scalar returns.
- Void methods, represented by response valid/ready without data.
- Fixed-size `Vec<T, N>` returns.
- Struct returns, including structs containing fixed-size `Vec` fields.
- Bounded burst-like responses via static `Vec<T, MAX>` plus `len`/`resp`
  fields in the payload or response struct.

## Protocol Contract

For a method named `read`, the flattened bus protocol is:

- Request channel:
  - `read_req_valid`
  - one payload signal per method argument, e.g. `read_addr`
  - `read_req_ready`
- Response channel:
  - `read_rsp_valid`
  - `read_rsp_data` when the method has a return payload
  - `read_rsp_ready`
- For `out_of_order tags N`:
  - `read_req_tag`
  - `read_rsp_tag`

`arch build` emits `_auto_tlm_<port>_<method>_{req,rsp}_stable` SVA under
`translate_off/on`: request payload/tag and response payload/tag must stay
stable under backpressure. Validate these with Verilator `--assert`.

## Current Restrictions

- TLM calls are thread-body-only direct RHS assignments.
- TLM calls are rejected in `comb`, `seq`, module-level `let`,
  module-local `function`, `pipeline`, and `fsm`.
- Nested/composed expressions such as `x <= m.read(a) + 1;` are rejected.
- Runtime-loop and conditional-branch TLM calls are serialized direct blocking
  calls.
- RHS-fork offsets require literal `wait N cycle;`.
- RHS-fork tails after `join all;` are compute-only: sequential assignments and
  nested compute-only `if`/`elsif`/`else` branches.
- `out_of_order tags N` requires a literal tag count with enough tag values for
  the worker cohort.
- Dynamic-length return types are not supported; use static `Vec<T, MAX>` or a
  response struct carrying `data`, `len`, and `resp`.
- `connect a.m -> b.s;` is point-to-point sugar.
- Blocking TLM buses also support one-initiator-to-many-target sugar by
  repeating ordinary `connect a.m -> b.s;` statements. Each target inst must
  override literal `SLAVE_START_ADDR` / `SLAVE_END_ADDR` params, and every TLM
  method exposed by the initiator bus port must have an `addr` argument.
  Elaboration synthesizes private bus wires plus comb/seq routing logic, using
  the enclosing module's single Clock and Reset ports for response-route
  registers. Ranges must cover the full decode width. Target endpoints may be
  method-subset subtypes via bus params (for example `Mem<WRITE=0>`); decoded
  requests for missing target methods receive a local zero/false response, or a
  zero-filled struct with `resp=1` when the response type has a `resp` field.
- Tagged `out_of_order` decoded connect and custom routing/arbitration policy
  still belong in an explicit router module.

## Refinement Guidance: TLM to Explicit Threads

Use `tlm_method` as the first executable hardware contract for a transaction
API. Once the design needs channel-level control, refine the method boundary
into explicit bus signals and ordinary `thread` code.

Recommended sequence:

1. Keep the TLM model as the golden contract until the explicit thread version
   passes the same checks.
2. Name explicit bus signals after the generated method protocol:
   `req_valid`, argument payload fields, `req_ready`, `rsp_valid`, optional
   `rsp_data`, optional `rsp_tag`, and `rsp_ready`.
3. Port initiator calls into caller threads:
   - blocking call -> issue request, wait for response, capture result;
   - worker cohort -> one explicit worker per lane plus arbitration;
   - RHS-fork group -> timed issue threads plus a join/scoreboard condition.
4. Port the target body into a target thread that latches request args, runs the
   same waits/compute/control flow, then drives the response channel.
5. Copy or rewrite the generated TLM stable-payload assertions for the explicit
   bus signals.
6. Run the same golden HARC, C++ or Python smoke test through `arch sim`, then
   run `arch sim --thread-sim both` where applicable, and finish with
   Verilator simulation of the generated SV.

Refine to explicit threads when:

- the protocol has independent address/data/response channels;
- burst beats need per-beat interleaving or arbitration;
- ready/valid timing requires deliberate register placement;
- response IDs, error codes, retry, cancellation, or ordering rules are
  protocol-specific enough that generated TLM is too generic;
- area/power work needs manual state sharing, gating, or channel pruning.

## Historical Decisions

These directions were explored and intentionally closed for the current TLM
surface:

- `Future<T>` / `await`: rejected because ordinary worker threads already
  express hardware concurrency without a new lifetime model.
- `reentrant [max N]`: rejected and parser support removed. Use
  `generate_for` threads for static parallel copies.
- TLM `pipelined` as a separate in-order mode: replaced by worker cohorts over
  `blocking` methods and by `out_of_order tags N` where response order can vary.
  The old pipelining sketch also considered `Future<T>`/`await`, `reentrant`
  threads, and a separate `tlm_method: pipelined` mode. Those were rejected;
  the useful part shipped as ordinary multi-worker lowering. Current
  concurrency is expressed with named workers, `generate_for` workers,
  direct-call `fork ... and ... join` cohorts, RHS-fork groups, and
  `lock`/`resource` arbitration. Blocking responses route by issue-order FIFO;
  out-of-order responses route by compiler-managed tags.
- File-scope `implement Bus.method rtl`: not supported. The accepted
  `implement` spelling is a thread-header annotation over the same TLM
  call-site/cohort lowering; it is not a separate pool API. The original
  `implement` pool sketch would have let several
  `thread ... implement m.method()` bodies create a separate runtime pool with
  IDs, arbitration, and response routing. That design was narrowed: initiator
  `implement m.method()` only annotates ordinary direct RHS call/cohort
  lowering, and target `implement target s.read(addr)` is sugar-equivalent to
  dotted target syntax. Multiple target implementers are supported only with
  indexed tagged lanes such as `thread s.read[t](addr)` on an
  `out_of_order tags N` method; non-indexed multiple target implementers remain
  a targeted compile error.
- Separate LT/AT simulation modes: not supported. ARCH TLM is synthesizable TLM
  that runs through ordinary `arch sim` and SV simulation.

## Remaining Work

Current useful future slices are:

1. **Explicit-thread exemplars**: add one or two examples that start from a
   `tlm_method` model and refine to raw bus signals plus threads, preserving the
   same golden test.
2. **First-class beat-stream helpers**: only if a design proves that bounded
   `Vec`/response structs and explicit threads are not ergonomic enough.
3. **Richer diagnostics**: continue improving errors for unsupported composed
   TLM expressions and mixed router/connection shapes.
4. **Protocol assertion templates for explicit refinements**: make it easy to
   copy the TLM stable-payload contract onto manually written buses.
