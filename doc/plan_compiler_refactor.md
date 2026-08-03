# Compiler refactor backlog (2026-04-28)

Captured from the tech-debt review on `feature/regfile-latch-flops-config`. Six
refactors, ordered by ROI / risk. Items 1–3 are pure cleanup (no design
choices); items 4–6 want a design discussion before scheduling.

## Status update (2026-08-03)

This doc went stale: items #1, #2, #3, and #5 (both phases) shipped the same
week they were opened, but nobody came back to flip the status markers below
or update the "Status snapshot" line counts, which still describe the
2026-04-28 pre-refactor state. That staleness caused a real duplicate-work
near-miss — an agent was tasked on 2026-08-03 to "execute Phase 3a: the
CombStmt/Stmt merge" against this doc and against `project_refactor_plan.md`
(same problem, same staleness) before discovering via `git log` that item #5
had been done for over three months. Findings from that pass, folded into the
per-item sections below:

- **#1 DONE** — `src/width.rs` exists (`clog2`, `index_width`); landed via
  PR #207 (`aa4e4713`, 2026-04-28).
- **#2 DONE** — `src/codegen.rs` monolith is gone; `src/codegen/` now holds
  14 per-construct files. First installment landed via commit `6bf689bb`
  (2026-04-28); the doc's own follow-up list (counter/arbiter/ram/cam/fifo/
  fsm/pipeline/linklist/module) is fully checked off in the current tree.
- **#3 DONE** — `ConstructCommon::param_int` / `resolve_count_expr` exist in
  `src/ast.rs` (~line 1893).
- **#4 NOT independently verified this pass** — the named target functions
  now live in their post-#2-split files (e.g. `emit_module` →
  `src/codegen/module.rs`, `cpp_expr_inner` → `src/sim_codegen/expr_codegen.rs`)
  but nobody re-measured whether they're under the 300-line success
  criterion. Re-check line counts before treating this as blocking anything.
- **#5 DONE (both phases)** — see the item below; full evidence and PR list
  inline.
- **#6 STARTED, NOT COMPLETE** — `pub trait Construct` exists in `src/ast.rs`
  (~line 1513), explicitly labeled "Phase 1 (this PR) covers only the
  always-applicable accessors... Future PRs will add pass methods
  (`typecheck`, `emit_sv`, `emit_sim`, …) one at a time." `src/constructs/`
  does not exist yet — constructs have not been migrated off the `Item::*`
  match dispatch. Treat #6 as in-progress, not done.

The line counts in "Status snapshot" below were not recomputed as part of
this pass (out of scope for the #5 task that triggered this update) — treat
every number in that paragraph as pre-refactor history, not current fact.

## Status snapshot (as of 2026-04-28 — see status update above, now stale)

`src/codegen.rs` 9.3k lines, `src/sim_codegen/mod.rs` 6.7k, `parser.rs` 5.7k,
`elaborate.rs` 5.4k, `typecheck.rs` 5.2k. Six functions over 400 lines (largest:
`sim_codegen::gen_module` at 2376 lines). 273 `Item::*` match sites across 11
files. Recent regfile work touched 5 files for one feature — that's the cost
the refactors below should reduce.

---

## #1 — Shared `clog2` + width helpers (`src/width.rs`)

**Why.** `if n <= 1 { 1 } else { (n as f64).log2().ceil() as u32 }` appears
verbatim in 14 places across `codegen.rs`, `sim_codegen/`, and `typecheck.rs`.
Width inference for arithmetic ops is also duplicated between `codegen.rs` and
`sim_codegen/mod.rs` — the `+%` wrapping-op bug had to be fixed twice. Float
arithmetic for compile-time integer math is a small smell on its own.

**Scope.**
- New `src/width.rs` with:
  - `pub fn clog2(n: u64) -> u32` — exact integer (`n.next_power_of_two().trailing_zeros()`).
  - `pub fn infer_expr_width(expr: &Expr, ctx: &WidthCtx) -> Option<u64>` —
    one source of truth for IEEE 1800-2012 §11.6 widening rules and `+%` / `-%`
    / `*%` non-widening rules.
- Replace 14 `clog2`-shaped sites and the duplicated width inference at
  `codegen::infer_expr_width_internal` and `sim_codegen::*` width helpers.

**Risk.** Low. Pure substitution + one new function; behavior preserved by
existing tests.

**Effort.** 1 hour for `clog2`; 4 hours for `infer_expr_width` (depends on how
much of typecheck's `resolve_expr_type` overlaps).

**Success criteria.** No `(_ as f64).log2()` outside `width.rs`. Both
`codegen.rs` and `sim_codegen/mod.rs` width helpers replaced. 217/217 tests
green.

---

## #2 — Split `codegen.rs` into per-construct files

**Why.** 9.3k-line monolith. `sim_codegen/` has already proven the split
works — it has `mod.rs` + `fsm.rs` / `pipeline.rs` / `linklist.rs` / `ram.rs` /
`fifo.rs` / `cam.rs` / `thread_sim.rs`. `codegen.rs` deserves the same shape.

**Scope.**
- New `src/codegen/{module,fsm,fifo,ram,cam,counter,arbiter,regfile,pipeline,linklist,synchronizer,clkgate,bus}.rs`.
- Move `emit_<construct>` and tightly-scoped helpers (e.g. `emit_fifo_port_type`,
  `emit_ram_signal_type`, `emit_ll_port_type`) into the construct's file.
- Keep shared helpers (`emit_expr_str`, `emit_type_str`, `extract_reset_info`,
  `ff_sensitivity`, `rst_condition`, `fold_width_str`) in `codegen/mod.rs` or
  the new `width.rs`.
- Per-file `pub(super) fn` extension methods on `impl Codegen` mirror the
  sim_codegen pattern.

**Risk.** Low. Pure mechanical move; no behavior change.

**Effort.** 1 day. The blocker is making sure nothing depends on private
visibility within `codegen.rs` — quick `cargo check` after each file move.

**Success criteria.** `wc -l src/codegen/*.rs` — no file over 1500 lines.
Identical SV output for all 217 tests. Snapshot tests unchanged.

**Dependency.** Item #1 lands first or at the same time; otherwise width
helpers move twice.

---

## #3 — `param_int` / `resolve_count` on `ConstructCommon`

**Why.** The same 5-line closure is re-defined in `codegen::emit_regfile`,
`sim_codegen::gen_regfile`, and `sim_codegen/linklist.rs`. The pattern (and a
similar `resolve_count`) belongs on the shared base struct.

**Scope.**
- Add to `impl ConstructCommon`:
  ```rust
  pub fn param_int(&self, name: &str, default: u64) -> u64;
  pub fn param_int_opt(&self, name: &str) -> Option<u64>;
  pub fn resolve_count_expr(&self, expr: &Expr) -> u64;
  ```
- Replace the duplicated closures.

**Risk.** Low. Trivial method extraction.

**Effort.** 2 hours.

**Success criteria.** No `let param_int = |...|` closures in `codegen.rs` or
`sim_codegen/`. No regression.

---

## #4 — Split mega-functions by match arm

**Why.** Six functions over 400 lines, four over 700. They are flat dispatch
trees on `Item` / `TypeExpr` / `ExprKind` / `Stmt` and are read top-to-bottom
so often that splits are pure ergonomics. Targets:

| Function | Lines | File |
|---|---:|---|
| `sim_codegen::gen_module` | 2376 | `sim_codegen/mod.rs` |
| `elaborate::lower_module_threads` | 932 | `elaborate.rs` |
| `typecheck::check_module` | 778 | `typecheck.rs` |
| `typecheck::resolve_expr_type` | 743 | `typecheck.rs` |
| `sim_codegen::cpp_expr_inner` | 550 | `sim_codegen/mod.rs` |
| `codegen::emit_module` | 549 | `codegen.rs` |

**Scope.** Each function gets one `match` arm extracted per private helper.
No behavior change; no new abstractions.

**Risk.** Medium. Big diffs; reviewer fatigue. Do one function per PR so
mistakes localize.

**Effort.** 1–2 days, mostly mechanical.

**Success criteria.** No function over 300 lines outside hand-written match
arms. Tests green.

**Dependency.** Item #2 makes `emit_module` smaller for free; do that first.

---

## #5 — Merge `Stmt` / `CombStmt` (`ThreadStmt` stays separate)

**STATUS (2026-08-03): DONE — both phases shipped 2026-04-28, same day this
item was opened.** Confirmed via `git log`: `CombStmt` no longer exists as a
type (zero references outside historical doc comments in `ast.rs`); `Stmt` is
the single unified enum described in "Scope" below; `BlockKind` (typecheck)
and `AssignCtx` (codegen) are both live and used exactly as specified.
Shipped as four sequential PRs plus one same-day follow-up fix:

- `fd44b8c2` / PR #202 (`refactor/stmt-bodies-generic`) — generalize
  `ForLoop<S>` / `MatchArm<S>` / `MatchStmt<S>` (this is "Phase 5a" below).
- `bd99ca16` / PR #203 (`refactor/stmt-merge-5b`) — drop the `CombStmt` enum
  (Phase 5b part 1).
- `f7b0ba5a` / PR #204 (`refactor/stmt-merge-5b-part2`) — unify the
  codegen reg/comb walkers under `AssignCtx` (Phase 5b part 2).
- `e58b7c28` / PR #205 (`refactor/stmt-merge-5b-part3`) — unify the
  typecheck reg/comb walkers via `BlockKind` (Phase 5b part 3); this is
  where `check_reg_stmt` / `check_comb_stmt` became thin wrappers around one
  `check_stmt(..., block_kind: BlockKind, ...)`.
  Read: `src/typecheck.rs` — `check_stmt`.
- `aada8ccd` (Phase 5b part 4, same day, direct-to-main) — sim_codegen
  comb match-arm full recurse + walker collapse.

Net effect on `ast.rs`: `Stmt` carries `Assign(RegAssign)` where
`RegAssign = Assign` and `CombAssign = Assign` are now both aliases for one
struct; `IfElse = IfElseOf<Stmt>` (the doc comments' `CombIfElse` /
`CombMatch` aliases named in "Scope" below were never actually introduced —
the collapse went straight to the single names). `src/codegen/mod.rs` and
`src/codegen/module.rs` use `AssignCtx::{Blocking, NonBlocking}` in one
`emit_stmt` walker instead of parallel comb/seq emitters.

**Known residual cleanup (not done, low priority, not code-behavior-affecting):**
a handful of doc comments in `src/ast.rs` (`ForLoop`, `MatchStmt`, `IfElseOf`)
still describe the pre-collapse `ForLoop<CombStmt>` / `MatchStmt<CombStmt>` /
`CombIfElse` / `CombMatch` shapes as if those types exist. They don't anymore.
Harmless (comments only) but worth a follow-up sweep.

**Why (historical — the problem this solved).** `Stmt` (seq blocks) and `CombStmt` (comb blocks) share ~85% of their
variants. The shared types `ForLoop` and `MatchArm` carry `body: Vec<Stmt>`,
which forces `check_comb_stmt` to delegate to `check_reg_stmt` for for-loop and
match-arm bodies — comb code gets type-checked as if it were seq code in
nested contexts. This is currently latent (matching shape happens to be
compatible), but it means the comb-vs-seq distinction is enforced at the
*outermost* statement only.

`ThreadStmt` is genuinely a different domain (procedural with fork/join, wait,
lock, do-until). Its variants don't generalize cleanly into a comb/seq shape,
and after `lower_threads` it's gone — backends never see ThreadStmt. Merging
it would just bloat the unified enum with variants illegal everywhere except
in threads.

**The "log missing in threads" motivation in the original plan is stale** —
`ThreadStmt::Log` already exists, parses, and lowers. The remaining motivation
is the body-type duplication and the cross-delegation in typecheck.

**Scope.**
1. Generalize `ForLoop<S>` and `MatchArm<S>` (also `MatchStmt<S>`,
   `CombMatch<S>`) parametrically over the statement type.
2. Merge `Stmt` + `CombStmt` into one `Stmt` enum:
   - Variants: `Assign(Assign)`, `IfElse(IfElse)`, `Match(MatchStmt<Stmt>)`,
     `Log(LogStmt)`, `For(ForLoop<Stmt>)`, `Init(InitBlock)`,
     `WaitUntil(Expr, Span)`, `DoUntil { ... }`.
   - The parser already stores `=` and `<=` in the same `Assign` struct; the
     merged enum keeps that — block context (comb vs seq) determines the
     emitted operator.
3. Add `BlockKind { Comb, Seq, PipelineStage }` to typecheck. Reject:
   - `Stmt::Init` outside `Seq`.
   - `Stmt::WaitUntil` / `Stmt::DoUntil` outside `PipelineStage` seq.
   - Assignments to `reg`-typed targets in `Comb` context (existing rule).
   - Assignments to `wire`-typed targets in `Seq` context (existing rule).
4. `ThreadStmt::Log`-like variants: keep ThreadStmt as-is. Re-evaluate after
   the lowering pass if any thread-specific concerns surface.

**Risk.** Medium. ~30 match sites grow. The `Vec<Stmt>` ↔ `Vec<CombStmt>` swap
in shared types affects everything that constructs or walks `ForLoop` /
`MatchArm`. Mitigation: fix in one PR; rely on the test suite (217 tests).

**Effort.** 1 day. Two phases:
- Phase 5a (this branch): generalize `ForLoop<S>` / `MatchArm<S>`,
  introduce `BlockKind` typecheck, keep the two enum names for now via
  type aliases. Verifies the generalization in isolation.
- Phase 5b (follow-up): collapse `CombStmt` into `Stmt` proper.

**Success criteria.** `check_comb_stmt` no longer delegates to
`check_reg_stmt` for For/Match bodies. `Stmt` and `CombStmt` are either one
type or thin aliases. 217/217 tests green.

**MET.** `check_comb_stmt` / `check_reg_stmt` are both now thin
`BlockKind`-parameterized wrappers around one `check_stmt`; `CombStmt` no
longer exists as a type at all (stronger than the "thin alias" bar this
criterion set). See the STATUS block at the top of this item for PR
references.

---

## #6 — `trait Construct` to centralize the per-construct dispatch

**Why.** 273 `Item::*` match sites across 11 files. 69 functions named
`(parse|emit|gen|check)_<construct>`. Adding a new construct (or a new variant
in `kind:`) means 5 file edits at minimum. This is the biggest long-term cost
in the codebase and the one most resistant to incremental fixes.

**Scope.**
- New `trait Construct` with associated methods `parse(parser) -> Self`,
  `typecheck(checker)`, `emit_sv(codegen)`, `emit_sim(simgen)`,
  `emit_formal(formal)`.
- Each construct moves to its own file under `src/constructs/{name}.rs`,
  implementing the trait. The top-level passes (`parser::parse_source_file`,
  `typecheck::check`, etc.) iterate over a registry of `dyn Construct` rather
  than `match`-ing on `Item`.
- Migration: do this construct-by-construct. Start with simple ones
  (`counter`, `synchronizer`) to validate the trait shape; then port complex
  ones (`pipeline`, `thread`).

**Risk.** High. This is a redesign, not a refactor. The trait API has to
accommodate every existing construct's quirks (TLM bus methods, generate
expansion, thread lowering) and we won't know all of them until we hit them.

**Effort.** 1 week realistically. 2 weeks if surprises.

**Success criteria.** A new construct lands as one file. `Item::*` match
sites drop from 273 to under 50 (the residual being the registry itself and
debug/learn passes).

**Recommendation.** Do this *last*. Items 1–5 will reduce the cost of #6 by
trimming the per-construct surface area; doing #6 first means re-doing parts
of it after each later refactor.

---

## Recommended order

1. ~~(today) **#1 + #3** — clog2/width + param_int methods.~~ **DONE 2026-04-28.**
2. ~~(this week) **#5a** — generalize `ForLoop<S>` / `MatchArm<S>` + `BlockKind`.~~ **DONE 2026-04-28.**
3. ~~(next week) **#2** — split codegen.rs.~~ **DONE 2026-04-28.**
4. ~~(after) **#5b** — collapse `CombStmt` into `Stmt`.~~ **DONE 2026-04-28.**
5. (after) **#4** — split mega-functions one-PR-at-a-time. Line counts not
   re-verified since the #2 split moved these functions to new files —
   re-measure before assuming this is still open.
6. (in progress) **#6** — `trait Construct` redesign. Phase 1 (accessor
   methods only) has landed on `Item` in `src/ast.rs`; per-pass method
   migration (`typecheck`/`emit_sv`/`emit_sim`/`emit_formal`) and the
   `src/constructs/` split have not started.

Items 1–5 shipped inside 24 hours of this doc's original authoring (all on
2026-04-28) — this doc simply never got its status markers updated
afterward, which is what the 2026-08-03 status update above is for. Only #4
and #6 remain open work.
