# Phase 5b — Collapse `CombStmt` into `Stmt`

**Status (2026-04-28): parts 1, 2, 3, 4 DONE; formal kept parallel by design.**

- **Part 1** (PR #203, merged): drop `CombStmt`, `CombIfElse`, `CombMatch`; unify all references to `Stmt`; `CombBlock.stmts` is now `Vec<Stmt>`. 218/218 tests, zero snapshot drift.
- **Part 2** (this PR): introduce `AssignCtx { Blocking, NonBlocking }` and a unified `Codegen::emit_stmt(stmt, ctx)` + `emit_if_else(ie, ctx, is_chain)`. Deletes the `emit_reg_stmt_as_comb` workaround the original plan called out, plus the dead `emit_comb_if_else` / `emit_reg_if_else` helpers and the duplicate `comb_stmt_span_start`. Net −122 lines in `codegen.rs`. 218/218 tests, zero snapshot drift.
- **Part 3** (this PR): collapse the typecheck walker pair via design choice D3 (`BlockKind` explicit param). `check_reg_stmt` and `check_comb_stmt` are now thin wrappers around a unified `check_stmt(stmt, ..., block_kind, reg_names)`. The branch-aware driven-tracking, reg-vs-wire LHS check, and Init/WaitUntil/DoUntil rejection are all gated by `block_kind`. The previously-`unreachable!()` arms for seq-only variants in comb context now emit proper typecheck errors (defensive — the parser already routes them, but a programmatic AST mutation that bypasses the parser will land a diagnostic, not panic).

- **Part 4** (this PR): collapse the sim_codegen walker pair. Introduces `SimAssignKind { Seq, Comb }` and unified `emit_stmt` / `emit_stmts` / `emit_if_else`. **Fixes a latent bug**: pre-collapse, `emit_comb_stmt::Match` shortcut-emitted only `Stmt::Assign` arm bodies and silently dropped nested `if/else`, `for`, nested `match`, and `log` — meaning `arch sim` and `arch build` would diverge for any comb match arm with non-trivial body. `emit_reg_stmt::Match` did the right thing (full recurse). Post-fix, both go through one walker that always recurses. The wide-output-port `_arch_u128` conversion stays gated on `kind == Comb`; the `_n_{name}` shadow vs `_{name}` live LHS resolution is `kind == Seq`. The historical names (`emit_reg_stmt`, `emit_comb_stmt`, etc.) survive as 1-line delegating wrappers so call sites read semantically.

  **Formal kept parallel by design.** Its `walk_reg_stmt` and `walk_comb_stmt` write to *different output data structures* (`reg_writes: HashMap<String, Vec<(cond, value)>>` vs `comb_assigns: Vec<CombAssignFlat>`). Forcing them into one function with a kind flag would just add an `if kind == Seq { reg_writes... } else { comb_assigns... }` at the only differing site — no consolidation win, and no latent-bug story to justify it (looked; didn't find one).

---


Follow-up to phase 5a (merged in PR #202, [doc/plan_compiler_refactor.md](doc/plan_compiler_refactor.md) item #5). Phase 5a generalized `ForLoop<S>` / `MatchArm<S>` / `MatchStmt<S>` so that comb For/Match bodies type-check under comb semantics. Phase 5b finishes the merge: drop `CombStmt` entirely; have one `Stmt` enum that's valid in both comb and seq blocks; thread a `BlockKind` context through typecheck and codegen so the right rules and operators apply per block.

`ThreadStmt` stays separate. Its variants (fork/join, wait until, wait cycles, lock, do-until, return) are domain-specific and after `lower_threads` it's gone — no payoff in merging.

Status snapshot (2026-04-28): 13 parallel walker functions across typecheck, codegen, sim_codegen, formal, elaborate, comb_graph; 221 sites that dispatch on `CombStmt::` / `Stmt::`; ~85% variant overlap.

---

## Decisions to make first

These are the design choices Phase 5b commits to. My recommendation for each is in **bold**; the alternatives are recorded so reviewers can push back.

### D1. How does codegen know to emit `=` vs `<=`?

After the merge, `Stmt::Assign(Assign)` is the only assign variant. SV codegen needs to pick blocking `=` or non-blocking `<=` somehow.

- **(a) From enclosing block context.** Codegen calls `emit_stmt(stmt, ctx: AssignCtx::{Blocking, NonBlocking, ...})`. The parser already enforces `=` in `comb { }` and `<=` in `seq { }`, so the context is unambiguous. **Recommended** — keeps the AST clean (the syntactic distinction is *where* it lives, not *what* it carries).
- (b) Explicit `is_blocking: bool` field on `Assign`. Rejected: redundant with parser context, easy to set wrong, still requires context to validate.
- (c) Two enum variants `Stmt::CombAssign(Assign)` / `Stmt::SeqAssign(Assign)`. Rejected: this is exactly the split we're trying to remove.

### D2. Where do `Init`, `WaitUntil`, `DoUntil` live?

These three variants are seq-only or pipeline-stage-only. After the merge:

- **(a) Keep as variants of unified `Stmt`; reject in `BlockKind::Comb` at typecheck.** **Recommended.** Single source of truth; rejection is one line in `check_stmt`.
- (b) Hoist them into `SeqStmt` that wraps `Stmt + extras`. Rejected — reintroduces the split this phase is removing.

### D3. How does `BlockKind` propagate through the type checker?

`BlockKind { Comb, Seq, PipelineStage }` decides:
- Whether `reg` targets are valid (allowed in `Seq`/`PipelineStage`, error in `Comb`).
- Whether `wire` targets are valid (allowed in `Comb`, error in `Seq`).
- Whether `Init` / `WaitUntil` / `DoUntil` are legal.
- Whether the assignment operator is blocking or not (used by codegen, not typecheck).

Two threading options:

- **(a) Explicit parameter:** `check_stmt(stmt, ..., block_kind: BlockKind)` — every recursive walk passes it through. **Recommended.** Functional, obvious at every site, no hidden state. Adds ~30 function signature changes, but mechanical.
- (b) Stack on `&mut TypeChecker`. Push on block entry, pop on exit. Less plumbing, but easy to forget pop on early return.

### D4. Does the parser produce `Stmt` directly, or keep a thin parse-time discriminator?

After the merge there's only one `Stmt`. The parser still has separate paths because syntax differs (comb uses `=`, seq uses `<=`):

- **(a) `parse_seq_stmt` / `parse_comb_stmt` both return `Stmt`. The seq path accepts `<=`, the comb path accepts `=`. Both produce the same enum.** **Recommended.** Mirrors current shape; reuses generic `parse_for_loop_generic` from phase 5a; minimal churn.
- (b) One `parse_stmt(block_kind)` function. Rejected — the operator dispatch in the parser body is awkward to thread through `parse_assign` and friends.

### D5. Migration in one PR or two?

- **(a) One-shot.** Big diff (~750 LOC removed, ~150 added) but coherent. **Recommended.** Test coverage is solid (218 tests), snapshot tests catch SV drift, the dispatch is mechanical.
- (b) Stepwise. Introduce `Stmt2`, migrate consumers one-by-one with adapters, then rename and delete. More PRs to review, more total work, but each PR is small.

### D6. Should `CombMatch` / `CombIfElse` / `CombAssign` aliases survive the merge?

- **(a) Delete all three.** `CombMatch` becomes `MatchStmt`; `CombIfElse` becomes `IfElse`; `CombAssign` becomes `Assign`. **Recommended** — the whole point is one source of truth.
- (b) Keep aliases for source compat at call sites. Rejected — postpones the cleanup.

---

## Migration steps (assuming the recommended decisions)

### Step 1 — AST surface
- Drop `CombStmt`, `CombIfElse`, `CombMatch`, `CombAssign` aliases.
- Keep `Stmt`, `IfElse = IfElseOf<Stmt>`, `MatchStmt<S = Stmt>`, `ForLoop<S = Stmt>`, `MatchArm<S = Stmt>`. `ThreadStmt` and friends stay.
- Add `pub enum BlockKind { Comb, Seq, PipelineStage }` to `ast.rs` (or a small new module — TBD by the executor).
- `CombBlock.stmts: Vec<CombStmt>` becomes `CombBlock.stmts: Vec<Stmt>`. (`CombBlock` itself stays — it's the enclosing wrapper that tags "these are comb".)

### Step 2 — Parser
- `parse_comb_stmt -> Stmt` (was `CombStmt`). The `=` parser path produces `Stmt::Assign` directly.
- `parse_comb_for_loop` returns `Stmt::For(ForLoop<Stmt>)` (no longer `CombStmt::For(ForLoop<CombStmt>)`).
- `parse_comb_match` builds `MatchStmt<Stmt>` arms.
- Generic `parse_for_loop_generic` from phase 5a stays — its `S` is now always `Stmt`. Could be inlined back into `parse_seq_for_loop` / `parse_comb_for_loop`, but harmless to keep generic.
- Reject `<=` in `comb { }` and `=` in `seq { }` at parse time (already enforced — verify still enforced).

### Step 3 — Typecheck
- `check_reg_stmt` and `check_comb_stmt` collapse into `check_stmt(stmt, ..., block_kind)`. The body of the new function:
  - `Stmt::Assign`: target must be a valid LHS for `block_kind` (reg in Seq, wire/port in Comb).
  - `Stmt::Init`/`WaitUntil`/`DoUntil`: legal only in `Seq` or `PipelineStage`.
  - `Stmt::IfElse`/`Match`/`For`: recurse with the same `block_kind`.
- `collect_comb_stmt_reads` / `collect_comb_stmt_targets` and the seq equivalents collapse. New: `collect_stmt_reads(stmts: &[Stmt], block_kind, out)`.
- The driven-set / latch-targets / handshake-payload walkers all unify the same way.

### Step 4 — Codegen
- `emit_reg_stmt` and `emit_comb_stmt` collapse into `emit_stmt(stmt, ctx: AssignCtx)`. `AssignCtx::Blocking` for comb; `AssignCtx::NonBlocking` for seq; `AssignCtx::PipelineComb` retains the pipeline-stage prefix-rewriting variant currently in `emit_pipeline_comb_stmt`.
- `emit_reg_stmt_as_comb` (introduced as a fix for the comb For body bug pre-5a) is now redundant — delete.
- `emit_for_loop_sv<S>` becomes `emit_for_loop_sv(f: &ForLoop, body_emit)` (no longer generic over S). Kept generic over the closure.

### Step 5 — Sim codegen
- `emit_reg_stmt` and `emit_comb_stmt` in `sim_codegen/mod.rs` collapse. The C++ assignment operator is `=` in both cases, so the merge is even simpler than for SV codegen — just unify the dispatch.
- `sim_codegen/thread_sim.rs::emit_seq_stmt` / `emit_comb_stmt` collapse the same way.

### Step 6 — Formal, elaborate, comb_graph
- `formal::walk_reg_stmt` / `walk_comb_stmt` → single `walk_stmt`.
- `elaborate::rewrite_reg_stmt_cc` / `rewrite_comb_stmt_cc` → single `rewrite_stmt_cc`.
- `comb_graph::scan_comb_stmt` already only walks `CombStmt`. After merge, it's `scan_stmt` and walks `Stmt` from inside `CombBlock` only — same behavior, smaller surface.

### Step 7 — Tests
- Add a focused test: `<=` in a `comb` block → parse error.
- Add a focused test: `=` in a `seq` block → parse error.
- Add: `init on rst.asserted` inside a `comb` block → typecheck error (currently caught by it being a `Stmt`-only variant; after merge needs explicit `BlockKind::Comb` check).
- Run full suite + snapshot diff. Expect zero behavioral changes.

---

## Risk assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Wrong assign operator emitted (`=` vs `<=` flipped at a call site) | Medium | Snapshot tests catch SV drift on every existing module |
| `BlockKind` not propagated correctly into nested scopes | Medium | Phase 5a's regression test covers nested for-loop bodies; expand to nested if/match |
| Pipeline-stage seq stmts (`WaitUntil`, `DoUntil`) misrouted | Low | `BlockKind::PipelineStage` variant carries the distinction; existing pipeline tests cover lowering |
| `parse_comb_match` arm-body parse drift | Low | Phase 5a already fixed body type; this PR only changes the wrapping enum |
| Snapshot drift not caught locally | Low | `cargo test` runs all 218 tests including 34 snapshot tests; CI catches anything else |
| Sim semantics drift (NBA shadow vs immediate) | Low | Sim codegen uses `=` for both; merge doesn't change the actual C++ shape |

The biggest non-obvious risk is **step 4**: the SV emitter has more comb/seq divergence than the sim emitter (sensitivity lists, `always_comb` vs `always_ff`, blocking vs non-blocking). The unified `emit_stmt` has to take `AssignCtx` *and* know whether it's inside an `always_ff` for the assertion-emit path.

---

## Effort & success criteria

**Effort:** 1 day for an experienced reviewer of this codebase. Two-thirds of the diff is mechanical search-and-replace; the design work is concentrated in step 4 (codegen) and the `BlockKind` plumbing in step 3.

**Success criteria (no negotiating):**
- `cargo test` — 218/218 pass with zero snapshot drift.
- `wc -l src/codegen.rs src/sim_codegen/mod.rs` — net decrease of ~600 lines combined (the duplicated walkers).
- `grep -c CombStmt src/ -r` — zero (modulo doc comments referencing the historical name).
- New tests cover the rejected-by-context cases (D2/D3): `<=` in comb, `=` in seq, `init` in comb.

**Out of scope:**
- `ThreadStmt` merge.
- Full `BlockKind`-aware refactor of pipeline-stage timing analysis.
- Renaming `CombBlock` (it's still meaningful — the *block* wrapper distinguishes comb from seq even when the *statement* type is unified).

---

## Open questions for the user

Before executing, I want to confirm:

1. **D1 (assign-operator dispatch)**: Implicit-from-context (a) or explicit field (b)?
2. **D3 (`BlockKind` threading)**: Explicit param (a) or stack on `TypeChecker` (b)?
3. **D5 (migration)**: One-shot PR (a) or stepwise (b)?

Defaults are (a, a, a) — say "yes" if you want all three, or override individually.
