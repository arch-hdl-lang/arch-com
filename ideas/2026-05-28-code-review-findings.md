# Daily code review — findings 2026-05-28

Scope: PRs merged 2026-05-28 in arch-com (#445–#458) and harc-com
(#311–#318). This batch is *predominantly follow-up work* to the
findings note dated 2026-05-27 (`ideas/2026-05-27-code-review-findings.md`,
landed as PR #447). Today's review verifies each follow-up actually
closed its referenced finding, plus a fresh look at adjacent code and
new debt introduced.

Per `CLAUDE.md`, compiler/user-facing-semantics fixes require user
confirmation before landing. Internal codegen / sim-runtime fixes
that don't touch the spec may proceed autonomously. Each finding
below labels its category.

---

## Post-merge resolution status (updated 2026-05-29)

| # | Finding | Original status | Resolution |
|---|---|---|---|
| 1 | §1 follow-up #458 — sibling helpers unmigrated | HIGH open | **fixed** (#464 — 23 sites migrated, 4 bare helpers deprecated, combined FSM fixture added) |
| 2 / §A | #459 — `port reg` timing false positive | HIGH open | **fixed** (#459 force-pushed to use the existing `legacy_port_reg` gate instead of a new field — 5 files → 2 files, also silences user-written `pipe_reg<T, N>` correctly) |
| 2 / §3 | #460 — `--thread-sim` mutex policy downgrade | HIGH open, wrong shape | **fixed for real** (#460 redirected: the merged form is the **scheduler implementation** of round_robin/lru/weighted/custom — not the warning. Doc fairness caveats in `thread_spec_section.md` §20.8.1 and `thread_lowering_algorithm.md` were removed by the redirect; `--thread-sim both` is a real cross-check again) |
| 9 (subset) | `thread_spec_section.md` §20.8.1 caveat + `thread_lowering_algorithm.md` policy doc | MED open | **fixed alongside #460** — both files updated as part of the scheduler-redirect PR |
| 3 / 4 / 5 / 6 / 7 / 8 | LOW residual test / refactor / feature gaps | LOW open | unchanged — queued for next batch |
| 9 (remainder) | `ARCH_HDL_Specification.md` §7a.3 fixed-priority claim, `Arch_AI_Reference_Card.md` thread block, `COMPILER_STATUS.md` `--thread-sim both`, `nic400_interconnect_spec.md` §16.1 SVA inventory | MED open | unchanged — these were already stale before this batch and would benefit from a separate doc-sweep PR |
| A | `_cb_depth` MT safety | LOW tracked | unchanged (harc-com#316) |
| B | `topo_sort_component_indices` visibility | INFO | unchanged |
| C | bare/param-aware helper duality language proposal | INFO | informally addressed by #464 (option 1 — delete the bare form via deprecation); options 2/3 still tabled |

**HIGH findings: all three closed.** Three follow-up PRs landed:
[#459](https://github.com/arch-hdl-lang/arch-com/pull/459) (legacy_port_reg gate, my replacement),
[#460](https://github.com/arch-hdl-lang/arch-com/pull/460) (scheduler honours mutex policies — the real fix, not the warning),
[#464](https://github.com/arch-hdl-lang/arch-com/pull/464) (sibling-helper migration).

The original sections below remain the **historical record** of what
the review found before resolution. The findings table above is the
source of truth for current status.

---

## TL;DR

| # | Finding | Severity | Status | Category |
|---|---|---|---|---|
| 1 | §1 follow-up #458 leaves 4 sibling bare-API helpers unmigrated and undeprecated | **HIGH** | open | internal codegen |
| 2 | 2026-05-27 findings note declares §3 (#460) and §A (#459) fixed, both PRs still **OPEN**; **#460 takes the wrong approach** — warning is not the fix, scheduler should honour the policy | **HIGH** | open | sim runtime |
| 3 | §2 follow-up: arch-sim has no behavioral test on non-pow-of-2 RR; only Verilator runs on `RRArb3` | LOW | open | test gap |
| 4 | §4 follow-up: WRAP AW-side SVAs added but no expect-fatal TB exercises the AW path | LOW | open | test gap |
| 5 | §4(d) still open: INCR 4 KB-boundary precondition unasserted; AxLOCK=EXCLUSIVE preconditions also unasserted (new) | LOW | open | feature gap |
| 6 | §6 mirror-update logic still duplicated across 3 sites in `cpp_tb.rs` | LOW | open | refactor debt |
| 7 | §7 lifecycle AST shape still uses `Lifecycle(ScopeDecl)` field-overload; duplicate-check invariant remains implicit | LOW | open | refactor debt |
| 8 | §8 follow-up #318 silently drops topo edges when two items declare the same hookable name; no fixture | LOW | open | test gap |
| 9 | Stale thread/policy claims in `ARCH_HDL_Specification.md` §7a.3, `Arch_AI_Reference_Card.md` thread block, `COMPILER_STATUS.md` `--thread-sim both` description, `nic400_interconnect_spec.md` §16.1 SVA inventory | MED | open | doc drift |
| A | `_cb_depth` recursion guard is plain `uint32_t`, races silently if `mt: true` ever ships (already tracked) | LOW | tracked | harc-com#316 |
| B | `topo_sort_component_indices` was bumped from private to `pub` for unit test; minor surface widening | INFO | open | API surface |
| C | Language proposal: split param-aware vs bare codegen helpers is a recurring footgun — proposal sketches a single-source `Ctx`-threaded API | INFO | proposal | architecture |

---

## 1. §1 follow-up #458 is structurally incomplete (HIGH)

PR #458 deprecated and migrated `type_bits_te` and `eval_const_expr`
to the `_with_params` form. But the same hazard class — a bare
helper that delegates to its `_with_params` twin with `&[]` — has
**four** other instances in `src/sim_codegen/mod.rs` that were not
touched, and they have ~25 live call sites:

| Bare helper | Definition | Live callers (non-`_with_params`) |
|---|---|---|
| `type_width` | [mod.rs:1132](src/sim_codegen/mod.rs:1132) | [mod.rs:1171](src/sim_codegen/mod.rs:1171), [fifo.rs:223,236](src/sim_codegen/fifo.rs:223), [ram.rs:44](src/sim_codegen/ram.rs:44), [fsm.rs:449](src/sim_codegen/fsm.rs:449) |
| `cpp_port_type` | [mod.rs:1239](src/sim_codegen/mod.rs:1239) | [mod.rs:7664,7809,7814,7821,7987,8012,8408](src/sim_codegen/mod.rs:7664), [fsm.rs:129,134](src/sim_codegen/fsm.rs:129), [cam.rs:52](src/sim_codegen/cam.rs:52), [pipeline.rs:208](src/sim_codegen/pipeline.rs:208), [fifo.rs:37,40](src/sim_codegen/fifo.rs:37), [linklist.rs:47,60](src/sim_codegen/linklist.rs:47) |
| `cpp_internal_type` | [mod.rs:1734](src/sim_codegen/mod.rs:1734) | [mod.rs:4353,4355,4424,4457,7789,7858,8362](src/sim_codegen/mod.rs:4353), [fsm.rs:142,151,156,269](src/sim_codegen/fsm.rs:142), [pipeline.rs:37,47,136,156,274,374](src/sim_codegen/pipeline.rs:37), [fifo.rs:23](src/sim_codegen/fifo.rs:23) |
| `vec_array_info` | [mod.rs:1776](src/sim_codegen/mod.rs:1776) | [mod.rs:481,5539,5561,5589,8358](src/sim_codegen/mod.rs:481), [fsm.rs:87,139,266,280,374](src/sim_codegen/fsm.rs:87) |

**Why this matters.** An FSM with `param ACC: const = 48; port out: out UInt<ACC>; reg buf: Vec<UInt<ACC>, 4>` would trigger the exact bug pattern PR #458 set out to close:

- [fsm.rs:129](src/sim_codegen/fsm.rs:129) (`cpp_port_type(&p.ty)`) silently buckets a `UInt<48>` port into `uint32_t` instead of `uint64_t`. C++ writes to `port` will truncate.
- [fsm.rs:142](src/sim_codegen/fsm.rs:142) (`cpp_internal_type(&reg.ty)`) silently picks the wrong reg-storage scalar type for `reg buf: UInt<ACC>`.
- [fsm.rs:266](src/sim_codegen/fsm.rs:266) (`vec_array_info(&reg.ty)`) — direct twin of #442's CamMatch sites for the FSM lowering path. `Vec<_, ACC>` reset loop iterates zero times.

The user-confirmed regression fixture for §1
([tests/regression/issues/fsm_bus_flat_param_width/FsmBusFlatParamWidth.arch](tests/regression/issues/fsm_bus_flat_param_width/FsmBusFlatParamWidth.arch))
covers only one site (VCD-trace bus-flat width via `type_bits_te`).
The proposed combined fixture from §1 of the prior review — FSM with
`reg buf: Vec<UInt<N>, 4>`, scalar `out UInt<N>`, and Vec output port
— was not added by #458 and is exactly what would catch the four
sibling helpers above in one fixture.

**Proposal.** A small follow-up PR that mirrors #458 against the
four sibling helpers (same `#[deprecated]` + migration + landmine
shape), plus the combined-FSM fixture. Pure internal codegen, no
spec change.

---

## 2. 2026-05-27 findings note declares fixes that haven't merged — AND #460 takes the wrong approach (HIGH)

`ideas/2026-05-27-code-review-findings.md` (landed via PR #447) and
the PR description both claim:

- **§3** "`--thread-sim` silently downgrades policy to priority" → **fixed (#460)**
- **§A** "Synthesized `port reg` triggers `check_port_reg_timing` false positive" → **fixed (#459)**

As of 2026-05-28 06:50 UTC, both [#459](https://github.com/arch-hdl-lang/arch-com/pull/459)
and [#460](https://github.com/arch-hdl-lang/arch-com/pull/460) are
still **OPEN**. The findings note is on `main` claiming fixes that
have not landed.

### #459 (§A) — right approach, just needs to land

Internal flag on `PortRegInfo` to suppress the false positive on
thread-synthesized port-regs. Straightforward, scoped, and matches
the original §A proposal verbatim. Land as-is.

### #460 (§3) — **wrong approach; should be redirected**

PR #460 implements §3's *short-term* option (emit a codegen-time
warning) instead of the *real* fix: teach the `--thread-sim`
scheduler to honour the mutex policy. The original §3 finding
listed both options and called the scheduler fix "longer-term", but
the warning-only approach leaves the semantic divergence live.
`mutex<round_robin>` is documented as fair; if `--thread-sim`
delivers priority instead, the scheduler is buggy, not the user's
design. A warning shifts the burden onto the user (read every warn
line before trusting `--thread-sim both` results) and doesn't
remove the false-PASS surface.

This applies to a broader principle worth tagging explicitly: when
a code path silently degrades a documented semantic (RR → priority,
multi-threaded → single-threaded, etc.), the default fix is to
implement the documented behaviour. A warning is acceptable only as
a transition step with a tracking issue for the real fix.

**Proposal:**

1. Close PR #460 or redirect to "warning + tracking issue for the
   real fix", whichever the author prefers. The warning itself is
   not harmful — it just isn't the closure.
2. Open a new PR (or reframe #460) that **implements the policy in
   the `--thread-sim parallel` scheduler**. Today the scheduler at
   `src/sim_codegen/thread_sim.rs:808-814` is "free or already
   mine," which is structural priority. The fix adds a per-resource
   policy field that the runtime consults: maintain a per-resource
   `last_grant` index for round-robin, an LRU stack for `mutex<lru>`,
   a credit counter for weighted, and a hook callback dispatch for
   `mutex<MyFn>`. The custom-hook case in particular needs to fire
   the user's `hook grant_select(...)` and use its one-hot result.
3. Once the scheduler honours the policy, remove the codegen-time
   warning from #460 — it stops carrying signal.

This is internal codegen / sim runtime, so per `CLAUDE.md` the
implementation can land without spec confirmation. But the design
of the scheduler state machinery (especially the credit/weighted
policy and custom-hook calling convention) is non-trivial and
benefits from human review before code.

### #461 missing from the resolution table

The resolution table in #447 is also missing PR #461 (SHA-256
example), which landed in the same 24h window. Unrelated to the
review's findings — just noting that the table is currently
incomplete.

---

## 3. §2 follow-up: arch-sim has no behavioral test on non-pow-of-2 RR (LOW)

PRs #451 (sim cycle-1 fix) and #452 (SV non-pow-of-2 fix) are both
correct: the SV scan now uses `(rr_ptr_r + i) % NUM_REQ` with a wrap
ternary at `grant_requester == NUM_REQ-1` ([arbiter.rs:283-285](src/codegen/arbiter.rs:283)),
and the sim scans `(_last_grant + 1 + _i) % N` with `_last_grant`
reset to `N-1` ([mod.rs:8438,8488,8501](src/sim_codegen/mod.rs:8438)).
Both agree cycle-1 picks index 0, and both use modular (not masked)
arithmetic.

But the regression test [tests/arbiter_rr_nonpow2/RRArb3.arch](tests/arbiter_rr_nonpow2/RRArb3.arch)
is **Verilator-only**. The §2 ask was "byte-identical expectation
under both default sim and Verilator." `arch sim` is exercised on
`RRArb3` substring-only ([integration_test.rs:592](tests/integration_test.rs:592)
checks `_last_grant(3)` appears in the emitted header) — there's no
behavioral check that `arch sim` produces grant sequence 0,1,2,0,1,2
on the same fixture.

**Proposal.** Extend the existing test to run `arch sim` on `RRArb3`
and assert byte-identical TB-output to the Verilator path. Internal
test, no spec impact.

---

## 4. §4 follow-up: WRAP AW-side SVAs added but no expect-fatal TB drives them (LOW)

PR #456 added 8 SVAs total (4 per module × {AR,AW}) for WRAP `axlen ∈ {1,3,7,15}`
and base-alignment preconditions on `Nic400ApbBridge` and
`Nic400WidthAdapter`. The expect-fatal harness (PR #453) now drives
4 new TBs at [tests/integration_test.rs:17593,17613,17634,17652](tests/integration_test.rs:17593),
but **all four exercise the AR path only**. If a future codegen
change broke the AW SVA label or condition, CI would not fire.

**Proposal.** Mirror 2 of the 4 expect-fatal TBs to drive `aw_*`
(one len, one alignment) on either module. Pure TB, no spec impact.

---

## 5. §4(d) and AxLOCK preconditions still unasserted (LOW)

PR #456 closed §4(a) and §4(c). §4(b) (upsize-WRAP) is structurally
untestable in `Nic400WidthAdapter` — that module is downsize-only
([Nic400WidthAdapter.arch:10-11](tests/nic400/Nic400WidthAdapter.arch:10)).
§4(d) remains open.

Additional adjacent AXI preconditions both modules forward without
checks:

- **4 KB INCR boundary** ([Nic400ApbBridge.arch:116-128](tests/nic400/Nic400ApbBridge.arch:116)).
  Per AXI4 §A3.4.1 a single burst must not cross a 4 KB page boundary.
  Currently logged as `❌ unverified` in [doc/nic400_interconnect_spec.md:1131](doc/nic400_interconnect_spec.md:1131)
  with the gloss "upstream-master's responsibility" — but the SVA
  shape is straightforward (`(burst==INCR) |-> (addr[11:0] + (len+1)*(1<<size)) <= 4096`)
  and would catch master bugs at simulation, not in silicon.
- **AxLOCK=EXCLUSIVE preconditions** (AXI4 §A7.2.4). Exclusive
  accesses require `axlen ≤ 15` total beats, power-of-2 byte count,
  and naturally aligned base. Both modules echo `ax_lock` blindly
  ([Nic400WidthAdapter.arch:119,219](tests/nic400/Nic400WidthAdapter.arch:119)).

**Proposal.** Two new SVAs per module (INCR-4K, EXCLUSIVE-aligned),
mirrored to the expect-fatal TB. Internal-only.

---

## 6. §6 mirror-update logic still duplicated across 3 sites (LOW)

§6's tech-debt proposal — extract `emit_mirror_update_and_dispatch`
— was not implemented. Mirror-write codegen is hand-duplicated at:

- Field-level frontdoor write: [src/codegen/cpp_tb.rs:15166-15171](src/codegen/cpp_tb.rs:15166)
- Register-level frontdoor write: [src/codegen/cpp_tb.rs:15195-15201](src/codegen/cpp_tb.rs:15195)
- Passive `record_write`: [src/codegen/cpp_tb.rs:14743](src/codegen/cpp_tb.rs:14743)

Three subtly different mask/cast/dispatch shapes. The active-side
no-callback asymmetry is intentional (per `docs/ral-support.md`
§3.2) but the duplication makes that easy to break silently when
the next RAL slice lands (field callbacks, addrmap recording).

**Proposal.** Extract before the next RAL feature lands. Internal-only.

---

## 7. §7 lifecycle AST tightening still open (LOW)

§7 proposed replacing `ComponentItem::Lifecycle(ScopeDecl)` with a
typed variant `Lifecycle(LifecyclePhase, Block)`. PR #313 fixed the
proximate wire-up bug but left the AST shape unchanged. Today:

- [src/ast.rs:503](src/ast.rs:503) still wraps each phase keyword in
  a 4-field `ScopeDecl` with 3 fields `None` and 1 `Some`.
- [src/parser.rs:1664-1678](src/parser.rs:1664) duplicate-check
  walks every prior `ComponentItem` inspecting which of the 3 phase
  fields is `Some`.
- [src/codegen/cpp_tb.rs:16727-16740](src/codegen/cpp_tb.rs:16727)
  aggregation is "last writer wins per phase" across those stubs.

Functionally fine **today**; the load-bearing assumption ("one
populated field per `Lifecycle` node") is implicit and untyped.
Also missing: a fixture asserting source-order independence (e.g.
declaring `check` source-before `setup`).

**Proposal.** Type-level refactor + source-order-independence
fixture. Internal-only.

---

## 8. §8 follow-up #318 silently drops edges on ambiguous callees (LOW)

PR #318's new visitor at [cpp_tb.rs:2478-2484](src/codegen/cpp_tb.rs:2478)
extends the topo-sort dep graph by walking `Call { callee: Field { name } }`
hooks. The rule is: add an edge from caller to the (single) item
that owns a hookable of that name. **If two items both declare a
hookable by the same name, the edge is silently dropped** and the
fallback is the field-rule.

This is documented in the comment and is the conservative behaviour.
But if both owners are reachable only via call (not via field), the
original §8 bug class returns silently. No fixture exercises the
ambiguous-owner case.

Also pre-existing (not regressed): the field-rule's
`type_simple_name` walker at [cpp_tb.rs:2435-2443](src/codegen/cpp_tb.rs:2435)
only matches simple names, not generic-wrapped (`Vec<Foo, 4>`) or
container-typed fields. PR #318 doesn't widen this.

**Proposal.** Add an ambiguous-owner fixture and either (a) widen
the dispatch (resolve by receiver type) or (b) emit a compile error
on ambiguous hookable names with no field-edge anchor. Internal-only.

---

## 9. Doc consistency gaps (MED)

Documentation should not be amended until the underlying PRs land
(see Finding 2 for #459/#460). Once they do, the following deltas
are needed:

- **`doc/thread_spec_section.md` §20.8.1 (line 216)** — currently
  describes only the runtime behaviour. The right closure here
  depends on Finding 2: once the scheduler actually honours
  `mutex<round_robin|lru|weighted|custom>`, §20.8.1's caveat about
  fairness under `--thread-sim` can be **removed entirely**
  (`--thread-sim both` becomes a real cross-check again). If the
  warning-only path from #460 lands as a transition step, document
  it here; otherwise leave the section alone until the scheduler
  fix lands.
- **`doc/ARCH_HDL_Specification.md` §7a.3 (lines 1737–1746)** —
  stale: "the compiler generates a fixed-priority combinational
  arbiter" and "with fixed priority the waits-for graph is acyclic
  — Thread 0 always makes progress". Contradicts the v0.46.0 policy
  menu. Same file's line 66 already names the menu for arbiters —
  this §7a.3 lock section is the outlier.
- **`doc/Arch_AI_Reference_Card.md` thread block (lines 591–593)** —
  same stale claim ("`grant[i] = req[i] && !grant[j<i]`"). Same
  file's line 1124 already uses `mutex<round_robin>` correctly.
- **`doc/COMPILER_STATUS.md` (lines 7, 39)** — still describes
  `--thread-sim both` as the equivalence/codegen check with no
  caveat. The known false-pass under `mutex<round_robin|lru|weighted|custom>`
  (PR #443, PR #460) should be reflected.
- **`doc/nic400_interconnect_spec.md` §16.1 rows (lines 1106, 1129)** —
  inventory lists only the legacy `ar_burst_legal` / `ar_burst_supported`
  SVAs. PR #456 added 8 new SVAs (`ar_wrap_len_legal_*`,
  `aw_wrap_len_legal_*`, `ar_wrap_addr_aligned_*`, `aw_wrap_addr_aligned_*`).
  Inventory should reflect the new coverage.
- **`README.md` line 147** — uses `port reg light: out UInt<2>` in
  an example despite the deprecation note at spec line 996.
  Pre-existing, but worth tracking under the same `port reg`-on-the-way-out
  policy.

---

## A. `_cb_depth` recursion guard is not thread-safe (LOW, tracked)

PR #317's per-binding `{regs_var}_cb_depth` counter at
[cpp_tb.rs:14123](https://github.com/arch-hdl-lang/harc-com/blob/main/src/codegen/cpp_tb.rs#L14123)
is a plain `uint32_t` C++ member. Single-threaded TB today; if
`mt: true` ever ships, parallel `record_write` calls into the same
binding could race the counter and mis-fire the FATAL or mis-bound
the depth. Already in-scope of [harc-com#316](https://github.com/arch-hdl-lang/harc-com/issues/316)
(deferred-by-design until `mt: true` shows >2× speedup).

No new action required.

---

## B. `topo_sort_component_indices` widened from private to `pub` (INFO)

To allow [`transactor_topo_sort_honors_hookable_call_edges`](https://github.com/arch-hdl-lang/harc-com/blob/main/tests/codegen.rs)
to call directly, PR #318 changed the visibility of
`topo_sort_component_indices` from module-private to `pub`. Minor
surface widening; acceptable for now, but the test should ideally
exercise the topo sort *through* the public emitter API rather than
calling the helper. No immediate action.

---

## C. Language proposal: bare/param-aware codegen helper duality (INFO)

Reflective note, not a fix. The recurring footgun closed by PRs
#427 / #439 / #442 / #458 (and incompletely closed there — see
Finding 1) has a deeper root cause: **the codegen library has two
parallel APIs for the same operation**. Every helper that resolves
a width / count / type bucket comes in two flavours:

```
fn helper(ty: &TypeExpr) -> X                                // BARE
fn helper_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> X  // PARAM-AWARE
```

The bare form falls back to default widths and produces silently
wrong codegen when a param is in scope. The param-aware form is
always-correct but verbose at every call site.

The recurring bug is that contributors reach for the bare form when
the param-aware form would be appropriate but isn't visibly
required by the type signature.

**Options that would close the class of bug, in order of cost:**

1. **Delete the bare form entirely.** Force every call site to
   pass `params` explicitly (use `&[]` only where literal-only is
   correct, with a doc comment). Maximum invasiveness, maximum
   safety. Diff would extend PR #458 by ~25 sites.
2. **Thread an `EmitCtx` carrying the enclosing-construct params.**
   The codegen functions take `&mut self: &mut SimCodegen` and the
   `SimCodegen` struct could carry a `current_params: &[ParamDecl]`
   field set by `gen_module` / `gen_fsm` / `gen_pipeline` at entry
   and restored at exit (RAII helper, mirroring harc-com#314's
   `CurrentMethodGuard`). The bare helpers then become methods on
   `&self` and consult the field. Larger refactor; eliminates the
   call-site verbosity *and* the footgun in one pass.
3. **Compiler check.** Add a `clippy` lint or `#[deprecated]`-like
   attribute that fires whenever a bare helper is called from a
   context where `params: &[ParamDecl]` is in scope. Static
   detection without an API redesign.

This is internal compiler architecture, not user-facing language
surface — so it's autonomously actionable per `CLAUDE.md`. Option
(2) is the cleanest end state but the largest one-time refactor;
option (1) buys 90% of the benefit for 10% of the change. Option
(3) is the lowest-friction stepping stone.

No urgency; tabling for a future refactor pass.

---

## Acknowledgements

The 2026-05-28 batch is overwhelmingly *good* follow-up work:
- 21 PRs across both repos with focused scope.
- Most §-references in PR titles/bodies are explicit and traceable.
- Tests-before-code pattern held (e.g. #454 + #456, #312 + #315, #318 + matching fixture).
- harc-com PR #314's `CurrentMethodGuard` is the standout — turned an implicit invariant into a compiler-enforced one with a 2-line lifetime story.

The findings above are residual debt and test-coverage gaps after a
batch that closed the substantive items from 2026-05-27. The HIGH
items (#1 and #2) are both quick to close in a follow-up sprint.
