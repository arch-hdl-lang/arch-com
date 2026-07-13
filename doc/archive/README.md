# Archived plan docs

This directory holds `plan_*.md` design docs whose proposed feature has
**fully shipped** and is no longer the working design surface. They're kept
for historical rationale (why a design was shaped the way it was, what
alternatives were rejected) — the **normative** description of current
behavior lives in `doc/ARCH_HDL_Specification.md`, `doc/COMPILER_STATUS.md`,
and `doc/Arch_AI_Reference_Card.md`. If a normative doc and one of these
archived plans disagree, the normative doc wins.

Moved here 2026-07-12 as part of the P6 doc-hygiene pass (live `doc/`
should only contain active roadmaps + normative references).

| Doc | What it planned | Shipped | Current home |
|---|---|---|---|
| [`plan_fp_types.md`](plan_fp_types.md) | `FP32`/`BF16` first-class IEEE-754 types: front-end, sim, synthesizable SV emission, differential Verilator equivalence campaign | v1 + P2 RTL landed (tracking issue #609); doc's own header already carried a "v1 implemented" note | `doc/COMPILER_STATUS.md`; `src/fp_ir.rs`, `src/fp_lit.rs`, `src/codegen/fp.rs`, `src/fp_smt_proof.rs` |
| [`plan_tlm_method.md`](plan_tlm_method.md) | Synthesizable `tlm_method` bus sub-construct (blocking + out-of-order tagged calls) | Implemented current subset; doc header says "implemented current subset" | `doc/ARCH_HDL_Specification.md` §18d/§22; `doc/COMPILER_STATUS.md` |
| [`plan_bus_unification.md`](plan_bus_unification.md) | Make `bus` the universal interface construct with `handshake_channel` / `credit_channel` / `tlm_method` as nested sub-constructs | Landed — spec §18a/§18c/§18d show all three as `bus` sub-constructs exactly per this plan's proposed shape; doc's own header already called itself "historical design draft" | `doc/ARCH_HDL_Specification.md` §18a–§18d |
| [`plan_handshake_construct.md`](plan_handshake_construct.md) | `handshake` primitive (Tier 1 + Tier 2 protocol SVA), later renamed `handshake_channel` per the bus-unification plan | Shipped v0.43.0 (PRs #21, #23, #25, #26, #27); rename to `handshake_channel` shipped v0.44.0 | `doc/ARCH_HDL_Specification.md` §18a; `doc/COMPILER_STATUS.md` |
| [`plan_credit_channel.md`](plan_credit_channel.md) | `credit_channel` stateful credit-based flow-control bus sub-construct | Shipped v0.44.8+ (sender counter, receiver FIFO, Tier-2 SVA) | `doc/ARCH_HDL_Specification.md` §18c; `doc/COMPILER_STATUS.md`; `src/sim_credit_channel.rs` |
| [`plan_credit_channel_ast_lift.md`](plan_credit_channel_ast_lift.md) | Lift `credit_channel` synthesized state into AST-level `RegDecl`s so all backends (codegen/sim_codegen/formal) share one representation | Formally deferred 2026-04-23 — no drift bug ever materialized; PR-hf4 (see `plan_hierarchical_formal.md`) unblocked the motivating formal use case via a formal-local synthesis workaround instead. Revisit only if one of the doc's own trigger conditions fires. | `doc/plan_hierarchical_formal.md`'s PR-hf4 note (also archived) |
| [`plan_pipe_reg_at_syntax.md`](plan_pipe_reg_at_syntax.md) | `pipe_reg<T, N>` port type + `@N` latency-tap operator, replacing the `port reg` footgun | Shipped — `pipe_reg` parsing/typecheck/codegen fully in `src/parser.rs`, `@N` latency taps documented in `doc/ARCH_HDL_Specification.md` | `doc/ARCH_HDL_Specification.md` (pipe_reg / `@N` sections) |
| [`plan_reg_guard_syntax.md`](plan_reg_guard_syntax.md) | `reg NAME: T guard VALID_SIG;` — valid-gated reset-free register annotation | Shipped — documented in `doc/ARCH_HDL_Specification.md` line ~1280, implemented in `src/parser.rs`/`src/ast.rs` | `doc/ARCH_HDL_Specification.md` (guard clause) |
| [`plan_hierarchical_formal.md`](plan_hierarchical_formal.md) | Hierarchical `arch formal` — sub-instance flattening, cross-module properties, credit_channel occupancy invariants | PR-hf1/hf1b/hf2 (one level of `inst` nesting) shipped 2026-04-24; PR-hf4 (credit_channel occupancy invariant, `CarriedCreditSite`) landed via PRs #108/#109; PR-hf3 (connect-by-name sugar) formally deferred — blocked on a whole-compiler typecheck/inst-resolution change, not scheduled | `doc/COMPILER_STATUS.md` "arch formal" row (hierarchical v1 note); `src/formal.rs` |
| [`plan_cam.md`](plan_cam.md) | `cam` (content-addressable memory) first-class construct | Shipped — `cam` keyword, parser (`parse_cam`), AST (`Item::Cam`), sim/codegen support all present | `doc/ARCH_HDL_Specification.md` §13; `src/parser.rs`, `src/ast.rs` |
| [`plan_cam_remaining.md`](plan_cam_remaining.md) | Backlog of `cam` v2+ features not covered by the original plan (TCAM/`kind: ternary`, range matching, multi-write) | Base `cam` (v1 DEPTH/KEY_W, v2 dual-write, v3 value_type payload — PRs #122/#124/#129) fully shipped; the four remaining items were explicitly "build when motivated," have zero test-tree demand, and are not on any active roadmap | `doc/ARCH_HDL_Specification.md` §13.0/13.0a/13.0b; file an issue if one of the remaining features becomes needed |
| [`plan_stdlib_buses.md`](plan_stdlib_buses.md) | Curated standard-library `bus` definitions (`BusApb`, `BusAxiLite`, `BusAxiStream`) shipped with the compiler, resolved via `use BusX;` | Shipped — `stdlib/BusApb.arch`, `stdlib/BusAxiLite.arch`, `stdlib/BusAxiStream.arch` exist; discovery/search-path behavior documented | `doc/ARCH_HDL_Specification.md` (stdlib bus package section, ~line 5591) |
| [`plan_arch_learning_system.md`](plan_arch_learning_system.md) | Local-first compiler learning system: capture error→fix pairs, retrieval for LLM agents, promotion to lints | v1 scope (capture, retrieval, `arch learn-index`/`arch advise`, doc-comment harvesting) shipped v0.42.0+ | `doc/COMPILER_STATUS.md` "learning capture" rows; `src/learn.rs`; `README.md` |

## Not archived (kept in `doc/`, for reference)

A few docs looked like archive candidates but were kept after evidence review:

- `doc/plan_arch_doc_comments.md` — status is still "design, not yet implemented" per its own header, and it's bundled into the `arch-programming` skill snapshot (`scripts/sync_skill_snapshots.sh`); kept in place.
- `doc/plan_arch_sim_reset_analysis.md`, `doc/plan_vec_methods.md` — no implementation found (`--reset-analysis` flag, `Vec` method binder) — still unimplemented roadmap.
- `doc/plan_compiler_refactor.md` — active, partially-landed backlog (item #5a done, items #2/#4/#6 open).
- `doc/plan_linklist_multi_head.md`, `doc/plan_sim_inst_inlining.md`, `doc/plan_thread_parallel_sim_phase3.md` — unimplemented future work.
- `doc/plan_thread_parallel_sim.md` — phases 1+2 (coroutine single-core sim, dual-pass cross-check) have shipped, but the doc is the umbrella design doc for phases 3–5 (OS-thread partitioning, multi-clock barriers, wait-skip perf), which remain unimplemented and undocumented elsewhere; kept as the live design reference for that work.
