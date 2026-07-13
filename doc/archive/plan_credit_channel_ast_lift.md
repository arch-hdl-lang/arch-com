# Plan: lift credit_channel state into AST RegDecls

> **Status (2026-04-23): DEFERRED — premature consolidation.**
>
> Phase A scaffolding was implemented in PR #114 (closed) and the
> rationale was reviewed. The "drift risk" motivation is theoretical:
> the three synthesis paths (codegen / sim_codegen / formal) have
> stayed in sync since credit_channel shipped, and the recent compiler
> bugs in this area (PRs #110 / #112 / #113) were about implicit bus
> wires — not credit_channel state drift. AST-lift wouldn't have
> prevented them.
>
> Revisit this plan if/when one of the following triggers fires:
> - A real drift bug appears (a credit_channel feature that works in
>   one backend but not another because the synthesis paths diverged).
> - A new backend (yosys, custom RTL emitter, third-party formal tool)
>   needs credit_channel support and the duplication becomes painful.
> - An incremental feature (multi-VC, batched credit return, CDC
>   credit channels) requires changing the state shape in all three
>   places at once and the consolidation pays for itself.
>
> Until then, the 3-way synthesis is fine. This doc stays as a
> "we already thought about this" reference.

---

## Original design + scaffolding

## Motivation

Today, credit_channel state is synthesized **three independent times** — once each in `codegen.rs`, `sim_codegen/sim_credit_channel.rs`, and `formal.rs`. The shapes happen to match (same names, same widths, same reset values) because all three follow §18c.2 of the spec, but nothing enforces it. Drift would silently break consumers — for example PR #110, #112, #113 each had to fix a separate "implicit bus wire" issue in a different backend.

A single elaboration pass that lifts the state into real `RegDecl` / `WireDecl` items in the AST gives:

- **One source of truth** — names, widths, reset values, transition logic all defined once.
- **Backend uniformity** — each backend just emits the regs it sees in the AST. New backends (yosys, EBMC dump, etc.) inherit credit_channel support automatically.
- **Smaller backends** — codegen, sim_codegen, formal each lose ~150–300 LoC of emit_credit_channel_* helpers.
- **SynthIdent eliminated for credit_channel** — `port.ch.can_send` rewrites to a real `Ident` referencing the lifted wire.

## Scope

For each module port `p` with `bus_info` whose bus carries one or more `credit_channel ch`, lift produces:

### Sender side (initiator/Out or target/In)
- `reg __<p>_<ch>_credit: UInt<W>` where `W = ceil_log2(DEPTH+1)`. Reset value `DEPTH`.
- `wire __<p>_<ch>_can_send: Bool` driven by `__<p>_<ch>_credit != 0` (or registered variant when `CAN_SEND_REGISTERED=1`).
- `seq` block updates: `++`/`--`/hold per send_valid && credit_return.

### Receiver side (target/Out or initiator/In)
- `reg __<p>_<ch>_occ: UInt<W>` reset 0.
- `reg __<p>_<ch>_head: UInt<P>` reset 0 (skip when DEPTH=1).
- `reg __<p>_<ch>_tail: UInt<P>` reset 0 (skip when DEPTH=1).
- `reg __<p>_<ch>_buf: Vec<T, DEPTH>` (no reset; data only valid when occ != 0).
- `wire __<p>_<ch>_valid: Bool = (__<p>_<ch>_occ != 0)`.
- `wire __<p>_<ch>_data: T = __<p>_<ch>_buf[__<p>_<ch>_head]`.
- `seq` block updates for occ/head/tail per send_valid && credit_return.
- `seq` write to buf on push (`if push: buf[tail] <= send_data`).

The handshake signals (`send_valid`, `send_data`, `credit_return`) stay as bus-port-flattened signals — they're *interface* not *state*.

## Where the pass runs

After `lower_credit_channel_dispatch` (which rewrites `port.ch.X` to SynthIdent), before `resolve` and `typecheck`. Concrete pipeline:

```
parse
  → lower_tlm_target_threads / lower_tlm_initiator_calls / lower_threads
  → lower_pipe_reg_ports
  → lower_credit_channel_dispatch
  → lift_credit_channel_state            ← NEW
  → resolve
  → TypeChecker
  → backends
```

After the lift pass, the AST contains real RegDecls / WireDecls / RegBlocks for credit_channel state. Resolve and typecheck see them as ordinary signals. Backends emit them through their normal paths.

## Phasing — incremental safe rollout

Going straight from the current 3-way synthesis to a single AST-lifted source risks bugs in any of the four code paths. Phased rollout:

### Phase A — scaffolding (this session, behind a flag)

- Implement `elaborate::lift_credit_channel_state` returning a new `SourceFile` with the lifted items.
- Gate behind `--experimental-cc-lift` CLI flag (default off). When off, the AST is returned unchanged and existing 3-way synthesis still runs.
- Add a unit test that calls the pass on a tiny module + asserts the expected RegDecl shape.
- **No backend changes.** Existing tests stay green.

### Phase B — codegen consumes lifted regs (separate PR)

- When the flag is on, codegen detects lifted regs (look for `__<p>_<ch>_credit` in the module body) and skips its own `emit_credit_channel_state` / `emit_credit_channel_receiver_state` paths.
- Tier-2 SVA emission moves into the lift pass (or stays in codegen but reads from the lifted regs).
- Verify on `tests/formal/credit_channel_*` and `tests/noc_credit/` that SV codegen output is **byte-identical** with flag on vs off (fold the constants away).

### Phase C — sim_codegen consumes lifted regs (separate PR)

- Same detection in sim_codegen — skip `sim_credit_channel::emit_*` paths when lifted regs present.
- Verify all existing sim tests still pass.

### Phase D — formal consumes lifted regs (separate PR)

- `formal::register_credit_channel_state` becomes a no-op when lifted regs present (they get registered through the normal port/reg walk).
- `register_carried_credit_sites` similarly skipped — flatten will inline the sub's lifted RegDecls under prefix.

### Phase E — flip default (separate PR)

- `--experimental-cc-lift` becomes the default-on `--legacy-cc-state` opt-out.
- After a release cycle, remove the legacy paths and the flag.

Total: 5 PRs over 5 sessions, each independently reviewable + revertable.

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| Naming drift breaks `port.ch.X` → SynthIdent dispatch | Lift uses identical naming. Add a unit test checking exact name strings. |
| Reset semantics differ from codegen's hand-emitted always_ff | Lift uses same `RegReset::Inherit` shape as user code. Compare emitted SV diff in Phase B. |
| Vec storage in the receiver tries to flow through formal v1 (which rejects Vec) | Already documented — formal skips data signal modelling. Lift can mark the buf as `formal_skip` or formal can heuristically skip Vec regs whose name starts with `__` and ends with `_buf`. |
| Hierarchical formal carry breaks because flatten_for_formal expects synthesized state | flatten will inline RegDecls under prefix; carried_sites go away. Phase D rewrites flatten accordingly. |

## Non-goals

- **Multi-VC, multi-flit** — same as current. Lift only handles single-VC single-flit.
- **CAN_SEND_REGISTERED v2 variants** — the bool param stays; lift emits the right shape per param.
- **Removing SynthIdent entirely** — only credit_channel uses go away. Other compile-time-derived references (handshake variants, tlm_method internals) keep their SynthIdent path.
