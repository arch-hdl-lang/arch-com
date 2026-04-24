# Plan: hierarchical `arch formal` (sub-instances + cross-module properties)

> **Status (2026-04-24)**: PR-hf1b (#100) + PR-hf2 (#101) shipped. Sub-modules
> with `let` + `comb` + `reg` + `seq` flatten end-to-end; PROVED/REFUTED
> verified on Adder, SubCounter, and multi-inst SubCounter designs.
>
> **PR-hf3** (connect-by-name sugar) — DEFERRED. Attempted 2026-04-24 as a
> formal-only feature but the typecheck driven-port check fires before
> formal flattening, rejecting unconnected ports in source. Making
> connect-by-name work end-to-end requires a whole-compiler change
> (typecheck + inst resolution + all backends), which is a separate
> design project. Not blocking — users write explicit connections today.
>
> **PR-hf4** (credit_channel occupancy invariant) — BLOCKED on
> credit_channel's synthesized state (sender counter + receiver FIFO)
> being generated at codegen time rather than materialized as AST-level
> regs. Unblocking requires either:
>   (a) an elaborate pass lifting credit_channel state into AST regs
>       (~200 LoC, cleanest — makes state uniformly visible to all
>       backends; also simplifies codegen's emit_credit_channel_* functions).
>   (b) formal-specific synthesis duplicating codegen logic (simpler
>       scope but risks divergence).
> Separate focused session. (a) is the preferred path.
>
> PR-hf5 is this status update.


*Author: session of 2026-04-23. Expands the direct-SMT formal
encoding (`doc/COMPILER_STATUS.md` arch formal row — v1 shipped
2026-04-17) from flat-module BMC to composable hierarchy.*

## Motivation

Today `arch formal` encodes **one module** as BV state + next-state
`ite` chain + per-cycle assert/cover disjunctions. Sub-instances error
out. This blocks several valuable cross-module properties:

- **credit_channel occupancy**: the sender's `credit` counter and the
  receiver's `__buf_occ` add up to `DEPTH` always. Requires observing
  both sides simultaneously.
- **handshake liveness across an `inst` boundary**: `p.aw_valid` drives
  an instance's input; wanting to prove the inst's output responds
  within N cycles needs composed encoding.
- **datapath end-to-end properties**: pipeline-stage invariants that
  span sub-module boundaries (decode → execute → writeback).
- **Real-design formal**: `ThreadMm2s` + a memory model connected via
  inst — the whole DUT is hierarchical.

Without hierarchy, users fall back to `arch build` + external tools
(EBMC, SymbiYosys) — defeating the direct-SMT path's advantage of
minimum toolchain dependency.

## Scope for v1 (this plan)

Start small. Three increments:

1. **Single level of nesting** (parent with `inst foo: Sub;` + simple
   connections). Sub-modules have no further sub-instances.
2. **Scalar state only** (same v1 restrictions — no Vec/struct/enum
   inside sub-modules). Matches current flat-module formal.
3. **Same-clock hierarchy** (all modules in the design use the top's
   clock). Multi-clock composition is v2 for formal.

Deferred to v2:
- Nested hierarchy (`inst` inside an instance).
- Vec/struct/enum across boundaries.
- Multi-clock formal composition.
- Param-generics on inst sites (param-polymorphic sub-modules).

## Approach: name-mangled composition

Each sub-inst becomes a **namespaced copy** of the sub-module's BV
encoding. For `inst foo: Sub ...`:

- Sub's register `Sub.r` becomes `foo_r` in the parent's SMT model.
- Sub's port `Sub.a: in UInt<W>` becomes `foo_a` as a BV variable
  whose next-state is bound by the parent's connection expression.
- Sub's next-state logic, comb logic, and asserts/covers are inlined
  under the `foo_` prefix.

Connections bind ports:
```
inst foo: Sub
  a <- parent_sig;        // foo_a = parent_sig  at every cycle
  y -> parent_out;        // parent_out = foo_y  at every cycle
end inst
```

Compose at elaboration time — flatten the hierarchy into a single
`FormalModule` equivalent to the v1 encoder's input, but with name-
mangled identifiers from each inst.

This is the standard **bottom-up flattening** approach used by many
formal tools. Trades some SMT size for implementation simplicity —
the solver handles a single flat model, our encoder stays flat.

### Why flatten, not compose at SMT level?

Alternative: encode each sub-module as its own SMT scope with
`push`/`pop` + `assume` bindings. This would be closer to modular
verification (prove sub-module lemmas, compose up). But:

- SMT-LIB doesn't have ergonomic sub-scope composition.
- Our BMC horizon is shallow (user-set bound, typically ≤50 cycles).
- Flatten-at-encode keeps the encoder readable.

The tradeoff is: whole-design SMT grows O(N * bound) where N is total
register count including instances. For bound=50 and N=200 regs, the
encoded SMT is still tractable for boolector/bitwuzla/z3.

## Implementation roadmap

### PR-hf1: walk one level of inst + emit flattened SMT

- Remove the "sub-instances not supported" hard-error in
  `src/formal.rs`.
- Instead, for each `ModuleBodyItem::Inst(inst)`:
  - Resolve `inst.module_name` to the sub-module's AST.
  - Bottom-up: encode the sub-module's registers, combinational nets,
    and next-state as flattened entries under `<inst_name>_`.
  - Bind connections: each `a <- parent_sig` adds an SMT equality
    `(= <inst>_a <parent_sig>)` at every cycle; each `y -> parent_out`
    adds `(= parent_out <inst>_y)`.
  - Include the sub-module's asserts/covers in the global property
    set (also with the `<inst>_` prefix on labels).
- Reset scope: sub-module sees the parent's reset (for v1, assume
  same reset signal drives both — typical for flat single-clock
  designs).

### PR-hf2: connections by name + default tie-offs

- Today inst connections require explicit `a <- X` / `y -> Y` lines
  per port. PR-hf2: allow **connect-by-name** via a shorthand (if
  the AST doesn't already support it) and tie unconnected sub-module
  inputs to 0.
- Validate: no sub-module input is left undriven; no sub-module
  output is connected to more than one parent sig.

### PR-hf3: cross-module assert/cover tests

- End-to-end tests: a 2-module design with a property spanning both.
  E.g., parent has a register `r` incrementing every cycle; inst has
  a counter that sees `r`; assert that inst's counter ≤ parent's `r`
  at every cycle.
- Validate PROVED / REFUTED / COVER HIT behaviors with z3, boolector,
  bitwuzla.

### PR-hf4: credit_channel occupancy invariant

The motivating property: `sender.credit + receiver.__buf_occ == DEPTH`
at every cycle, with clean reset. Write a small 2-module test that
compiles with PR-hf1's hierarchy support and PROVES this invariant.

### PR-hf5: docs + scope note in COMPILER_STATUS

- Spec section for the formal backend gets a "hierarchical support
  (v1)" subsection listing accepted / rejected patterns.
- COMPILER_STATUS row updated.
- Reference card addition (brief).

## Open questions

1. **Reset scoping**: sub-module's `port rst: in Reset<Sync>` — does
   it receive the same reset as the parent, or can different inst
   sites see different resets? **Leaning same-for-v1** (simplify;
   common case). Multi-reset-domain hierarchy is v2.

2. **Parameter inheritance**: sub-module declared with `param W: const = 32;`
   — if the parent instantiates with `inst foo: Sub<W=16>`, the
   flattened encoding uses W=16 for `foo_*`. **Leaning straightforward
   substitute-at-elaborate** (what the existing `inst` resolution does).

3. **Unsupported sub-module features** (thread/pipeline/fsm/ram/fifo
   inside an inst): reject each with a targeted message in PR-hf1,
   extend coverage incrementally. v1 formal already rejects these on
   a flat module; same error should just propagate up under the inst's
   name. Confirm.

4. **SMT naming collision**: if parent and inst both have `r`, the
   flatten produces `r` and `foo_r` — no collision. But what about
   two inst sites both of Sub with reg `r`? Both resolve to `foo_r`
   and `bar_r` per their own inst prefix → no collision. ✓ by
   construction.

5. **Assert / cover label preservation**: sub-module's
   `assert _auto_bound_vec_0: ...` becomes `foo._auto_bound_vec_0`
   in the output to preserve the name's debuggability. Leaning
   `<inst>.<label>` naming (with the dot literal in the SMT label).

6. **Bound scaling**: hierarchical designs have more state; users may
   need higher `--bound` values. Document but don't auto-adjust.

7. **Formal-only vs parallel with `arch build`**: should hierarchical
   formal work stay in sync with `arch build`'s inst handling? **Yes**
   — shared elaboration output would be ideal. For v1 we just walk
   the AST; full elaboration reuse is a v2 refactor.

Confirm all defaults and I start PR-hf1.

---

## PR-hf4 design note (2026-04-23)

Revising the earlier plan. The blocker isn't "synthesized state lives
in codegen" — it's that `arch formal` has no path to *reach* that
state from SMT. Two implementation tracks:

- **(a) AST lift**: a pass that materializes credit_channel state as
  real RegDecls / WireDecls in the module body, then all backends
  consume the same lifted regs. Cleanest but touches `codegen.rs`,
  `sim_credit_channel.rs`, `sim_codegen/mod.rs`, and `formal.rs` —
  multi-session refactor.
- **(b) Formal-local synthesis** (chosen for Phase 1): formal walks
  the same bus-ports + credit_channels metadata that codegen walks
  and emits matching BV state in the SMT encoding. Duplicates the
  state shape in one more place; unblocks hf4 without touching other
  backends.

(a) is still the long-term goal. (b) ships the invariant proof now.

### Phase 1 work breakdown (track b)

Scope: prove `sender.credit + receiver.__buf_occ == DEPTH` on a 2-module
design connected by an explicit bus with one credit_channel.

1. **Collect credit_channel sites** (~50 LoC). In `FormalCtx::preprocess()`,
   walk each module port's `bus_info` and collect `(port_name, ch_meta,
   role_dir)` tuples for every credit_channel field. Stash on the ctx.
2. **Register BV state per site** (~100 LoC). For each collected site,
   synthesize BV entries matching codegen's exact names:
   - Sender role: `__<port>_<ch>_credit` (width `ceil_log2(DEPTH+1)`,
     reset `DEPTH`), optional `__<port>_<ch>_can_send` wire.
   - Receiver role: `__<port>_<ch>_occ` (width `ceil_log2(DEPTH+1)`,
     reset 0), `__<port>_<ch>_head` / `__<port>_<ch>_tail` (width
     `ceil_log2(DEPTH)`, reset 0). **Storage `__buf[DEPTH]` is
     skipped**: the occupancy invariant doesn't need payload state, and
     modelling Vec storage trips formal v1's scalar-only restriction.
     A follow-up PR can extend formal to model one Vec when a
     data-path property needs it.
3. **Emit reset + transitions** (~80 LoC). For each state reg, add
   to `declarations`, `reset_constraints`, and `transitions` following
   the codegen logic at `codegen.rs:2795–3070`. Handshake signals
   (`<port>_<ch>_send_valid`, `<port>_<ch>_credit_return`) are module
   ports in post-elaborate AST and encode as ordinary port BVs.
4. **Resolve SynthIdent** (~20 LoC). Replace the hard-error at
   `formal.rs:1340` with a lookup into the lifted-state table; if the
   synthetic name was registered in step 2, encode as that BV.
5. **Hierarchical carry** (~100 LoC). `flatten_for_formal` currently
   inlines sub-module body items but doesn't propagate credit_channel
   metadata across the inst boundary. Extend the flatten pass to
   preserve each sub-module's bus-port credit_channels (renamed under
   the inst prefix) so step 1's collector sees both sender and receiver
   sides of the channel in the flat module.
6. **Test** (~30 LoC .arch + harness). Two-module design: `Sender`
   with the initiator side, `Receiver` with the target side, parent
   wires them through a bus. Assert `inst_s.credit + inst_r.occ == DEPTH`
   always (with reset in flight). Verify PROVED at bound ≥ DEPTH+2.

Total: ~400 LoC + test. Land as a chain of small commits (collection →
state registration → transitions → SynthIdent → hierarchical → test)
so each piece reviews cleanly.

### What this does not solve

- Data-path properties spanning the channel (payload correctness) —
  blocked on formal Vec state.
- Full path (a) consolidation — still the v2 refactor to do when
  sim_credit_channel, codegen, and formal start drifting.
