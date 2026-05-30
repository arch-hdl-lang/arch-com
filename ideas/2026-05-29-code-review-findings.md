# Daily code review — findings 2026-05-29

Scope: PRs merged 2026-05-28 → 2026-05-29 UTC.

- arch-com: #459, #460, #461, #463, #464, #465, #466, #467, #468, #469, #470,
  #471, #473, #474, #475 (plus the older #445–#458 batch reviewed yesterday
  in PR #463 — only the resolution status is revisited here).
- harc-com: #310, #311, #312, #313, #314, #315, #317, #318, #319, #320, #321.

The two headline new features are:

- **arch-com #470** — first compile-time multi-driver check (SFG Phase 1, closes #375).
- **harc-com #310 + #321** — passive RAL `record_write` / `record_read` plus
  callbacks (closes harc#218), and a `--check-backends` dual-backend trace diff
  (closes the arch-com #437 regression class).

Per `CLAUDE.md`, internal codegen / sim / docs / tests can land autonomously;
anything touching the language surface needs user confirmation. Each finding
labels its category.

---

## TL;DR

| # | Finding | Severity | Status | Category |
|---|---|---|---|---|
| 1 | harc-com #310's end-to-end fixture (`regblock_record_test.harc`) is not wired into `tests/run_fixtures.sh`, so the headline new RAL API has no CI integration coverage | **HIGH** | open | test gap |
| 2 | arch-com #470's SFG check has no test pinning the **intended** Vec-index aggregation behaviour (`vec[i]` and `vec[j]` collapse to one driver via `lhs_base_name`) | MED | open | test gap |
| 3 | arch-com `doc/COMPILER_STATUS.md` does not list the multi-driver check (#470) as shipped, despite this being a user-visible compile-time error class with `examples` linkage in [`ideas/2026-05-28-signal-flow-graph.md`](ideas/2026-05-28-signal-flow-graph.md) | MED | open | doc drift |
| 4 | harc-com #317's RAL callback recursion guard (`HARC_RAL_CB_MAX_DEPTH = 16`) emits a FATAL abort that no test exercises — `regblock_record_write_emits_recursion_guard` is substring-only | MED | open | test gap |
| 5 | arch-com #475 Hot-slave M↔M handoff test has no asymmetric-load scenario (one master always valid, one slow toggle) — round-robin starvation under asymmetric traffic is the canonical pathological case and remains uncovered | MED | open | test gap |
| 6 | arch-com #466 INCR-4K and EXCLUSIVE preconditions on `Nic400WidthAdapter` are not exercised by a dedicated expect-fatal TB; the PR cites "structurally identical" but the WidthAdapter's `axlen` scaling means the boundary math differs from the APB bridge case | MED | open | test gap |
| 7 | ~~nic400 SVAs lack `disable iff` gating~~ — **corrected**: compiler-side bug in `emit_assert_sva` affecting every user-written `assert`/`cover` across the codebase. Spec §7783 promises `disable iff (rst)`; emitter ignored reset polarity. **Fixed in [PR #479](https://github.com/arch-hdl-lang/arch-com/pull/479).** | MED | **fixed** | bug |
| 8 | harc-com #321 `diff_trace_strings` compares normalised lines by index — assumes deterministic event ordering across backends; assumption is undocumented | LOW | open | tech debt |
| 9 | Stale "Mealy fusion" / "`wait 0+ cycle until`" comments in 7 nic400 testbenches and `probe_ar_bubble.sh` after arch-com #471 retired the syntax | LOW | open | doc drift |
| 10 | arch-com #461 (`SHA-256 compression`) and harc-com #319 (`sha256.sv` + HARC TB) landed on the same day but neither cross-references the other; both lack a FIPS 180-4 vector citation in source | LOW | open | doc drift |
| 11 | arch-com #463 §9 (doc drift) is partially closed by #455/#466/#467 but the resolution table in PR #463 was never amended; the table will look fully open to the next reviewer | LOW | open | meta-debt |
| 12 | arch-com #470 explicitly defers multi-driver checks for `RegBlock`, `LatchBlock`, and `Thread` items — the inline comment cites TLM lowering as the blocker, but no tracking issue / follow-up was filed; the deferred surface is invisible to anyone reading only the merge | LOW | open | feature gap |

## Quick-win follow-ups (internal-only, can land autonomously)

Two of the HIGH/MED items are mechanical and safe to land without user
confirmation per `CLAUDE.md`. Companion PRs filed alongside this note:

- **Finding 1**: append `regblock_record_test | AxiLiteRegs | AxiLiteRegs.sv |` to
  `tests/run_fixtures.sh` (harc-com). The fixture and DUT already exist; this
  is a one-line registration.
- **Finding 2**: add a passing test
  `test_multi_driver_vec_index_writes_no_error` in
  `tests/integration_test.rs` (arch-com) — two `comb` blocks (or two
  generate-for-instantiated `Inst` outputs) writing `vec[0]` and `vec[1]`
  must not error. This pins the intent of the `lhs_base_name` collapse so
  any future change to that helper breaks loudly.

The remaining items are queued for the next batch.

---

## 1. harc-com #310 end-to-end fixture is dead code (HIGH, test gap)

PR #310 ships `tests/fixtures/regblock_record_test.harc` and presents it as
"an end-to-end fixture" backing the `record_write` / `record_read` /
`on regs.REG` API. But `tests/run_fixtures.sh` lists six `regblock_*_test`
entries:

```
regblock_basic_test     | AxiLiteRegs    | AxiLiteRegs.sv         |
regblock_fields_test    | AxiLiteRegs    | AxiLiteRegs.sv         |
regblock_access_test    | AxiLiteRegs    | AxiLiteRegs.sv         |
regblock_bitbash_test   | AxiLiteRegs    | AxiLiteRegs.sv         |
regblock_addrmap_test   | AxiLiteRegs    | AxiLiteRegs.sv         |
regblock_alias_test     | AxiLiteRegs    | AxiLiteRegs.sv         |
```

`regblock_record_test` is conspicuously absent. The compile-to-string unit
tests in `tests/codegen.rs` validate the emitter, but a real simulator run
exercising the decode ladder + callback dispatch + mirror-update + recursion
guard never executes in CI.

**Why this matters.** PR #317 went to material lengths to guard against
runaway callback recursion (`HARC_RAL_CB_MAX_DEPTH = 16`, per-binding
counter, FATAL abort). If the abort path is broken, CI will not catch it
until a real-world checker hits the recursion limit. The combination
of #310 (passive API) + #312 (else-if chaining) + #317 (recursion guard)
+ #318 (topo sort) + #320 (lock RAL active/passive asymmetry) is a five-PR
stack with no end-to-end integration check.

**Recommended action.** One-line addition to `tests/run_fixtures.sh`. Filed
as harc-com follow-up PR alongside this note.

---

## 2. arch-com #470 has no test pinning Vec-index aggregation intent (MED, test gap)

`src/signal_flow.rs::lhs_base_name` correctly collapses `vec[i]`, `vec[j]`,
`vec[…][bit]`, and `bus.cmd[…]` to their underlying signal name. The doc
comment is explicit about why:

> Bit-slice, part-select, vec-index, and latency-at write to the same
> underlying signal as the base, so collapse them. Bus-wire field
> accesses are *preserved* — `bus_wire.cmd` and `bus_wire.resp` are
> distinct flat signals in the generated SV, so driving both from
> different blocks is legal and must not trigger a multi-driver error.

The check correctly flags two blocks driving the same scalar / wire / port,
and tests cover:

- `test_multi_driver_two_comb_blocks_same_output`
- `test_multi_driver_single_comb_block_conditional_no_error`
- `test_multi_driver_two_inst_scalar_outputs_same_wire`
- `test_multi_driver_comb_and_inst_same_wire`
- `test_multi_driver_shared_or_port_exempt`
- `test_multi_driver_bus_wire_two_inst_connections_no_error`

But there is **no test** verifying that two writers to different indices of
the same `Vec` are intentionally aggregated into one driver. The intent
matters in both directions:

1. **Positive case**: `comb a = ...; vec_x[0] = a; end comb` + `comb b = ...; vec_x[1] = b; end comb`
   *currently* errors (collapsed to one base, two block-drivers detected) — but
   the design intent in the doc comment suggests this *should* error, because
   the underlying SV is one `always_comb` per block and SV's multi-driver
   rule treats `vec[0]` and `vec[1]` from different `always_comb` blocks as
   conflicting writes to the same `logic` array. So the current behaviour is
   correct, but it is **completely untested**, and the doc-comment intent
   makes it sound like the opposite.

2. **The legitimate "different indices in same block" case** (a single
   `comb` block writing `vec[0]` and `vec[1]` to two separate values) is
   already covered by the single-comb-block test.

**Recommended action.** Add `test_multi_driver_vec_index_from_two_blocks_errors`
(asserts the error fires for the cross-block Vec case) AND
`test_multi_driver_vec_index_in_single_block_no_error` (asserts the same
block writing multiple indices is fine). The pair pins both halves of the
design intent. Filed as arch-com follow-up PR alongside this note.

---

## 3. `doc/COMPILER_STATUS.md` doesn't list the multi-driver check (MED, doc drift)

`ideas/2026-05-28-signal-flow-graph.md` plans Phase 1 Check 1 (multi-driver)
as the first SFG check to ship, closing issue #375. PR #470 lands the check
with a new user-visible compile error class
(`CompileError::MultipleDrivers`).

`doc/COMPILER_STATUS.md` still describes the type-check surface without
referencing the new check. A new ARCH user reading the status doc would not
learn that "multiple drivers on the same signal" is now a static error in
arch v0.46+. The spec already states the rule (`doc/ARCH_HDL_Specification.md:485`),
but the status doc — which is the LLM/IDE onboarding surface — doesn't list
it under "compile-time checks shipped".

**Recommended action.** Add one row to the checks table in
`doc/COMPILER_STATUS.md` referencing PR #470 + the spec line, and update the
`ideas/2026-05-28-signal-flow-graph.md` plan to mark Phase 1 Check 1 as
DONE.

---

## 4. RAL callback recursion abort is unexercised (MED, test gap)

PR #317 emits a per-binding depth counter and a FATAL abort once
`HARC_RAL_CB_MAX_DEPTH = 16` is exceeded:

```cpp
if (regs_cb_depth >= HARC_RAL_CB_MAX_DEPTH) {
  sim_log_line("FATAL", "RAL record_write callback recursion exceeded …");
  errors++;
  _fatal = true;
} else {
  regs_cb_depth++;
  // … decode …
  regs_cb_depth--;
}
```

`regblock_record_write_emits_recursion_guard` is the only test for this
path, and it asserts only that:

- the `#ifndef HARC_RAL_CB_MAX_DEPTH` constant declaration is present,
- the `regs_cb_depth` counter is declared,
- the `if (regs_cb_depth >= HARC_RAL_CB_MAX_DEPTH)` guard is emitted,
- the `regs_cb_depth++` / `regs_cb_depth--` pair is present,
- the FATAL log line is emitted.

None of these prove the abort actually fires on a recursive scenario, nor
that `_fatal = true` short-circuits the rest of the test. A regression that
inadvertently changed `>=` to `>` would still pass all five substring
checks. A regression that flipped the unbump (`regs_cb_depth--` outside
the else arm) would also pass.

**Recommended action.** Add either (a) an end-to-end fixture
(`regblock_record_recursion_test.harc`) with a self-write callback to bury
the depth past 16 and assert the test ends in FATAL, or (b) a compile-only
test that pretty-prints the emitted code and runs `assertions` against the
abort logic (e.g. assert that the unbump is inside the else branch). (a)
is more valuable; (b) is cheaper.

---

## 5. PR #475 Hot-slave M↔M handoff doesn't cover asymmetric-load starvation (MED, test gap)

The three scenarios in `tests/nic400/Nic400FabricHotSlave_test.harc`:

- **S1**: strict M0↔M1 alternation; gap=1 cycle.
- **S2**: M0 and M1 both `valid=1` continuously; ≥ 0.9 t/c, no starvation.
- **S3**: M0 monopolises 4 ARs then drops; M1 tail sustains.

None of these match the canonical round-robin starvation pattern: one
master valid every cycle, the other valid once every K cycles. A buggy
round-robin pointer that *advances on every cycle* (instead of advancing
on grant) would silently starve the low-frequency requester in this
scenario but would pass S1/S2/S3.

The Hot-slave construct is a single bottleneck across all masters; this is
the worst place for the starvation gap to remain uncovered.

**Recommended action.** Add an **S4**: M0 drives `valid=1` continuously,
M1 toggles `valid` once every 5 cycles for 50 cycles. Assert that M1
receives at least one grant within every 15-cycle window. Cheap to add,
catches the failure mode.

---

## 6. Nic400WidthAdapter INCR-4K / EXCLUSIVE SVAs lack a dedicated expect-fatal TB (MED, test gap)

PR #466 adds the four SVAs (`ar_incr_no_4k_cross_wa`, `aw_incr_no_4k_cross_wa`,
`ar_excl_len_legal_wa`, `aw_excl_len_legal_wa`) to `Nic400WidthAdapter.arch`,
and the commit note states:

> WidthAdapter has structurally identical SVAs; harness CI on APB is
> sufficient confirmation that the SVA shape works.

This is true for *shape*, but the WidthAdapter's `axlen` scaling
((N+1) → (N+1)·RATIO) means the 4 KB boundary math differs: a master
burst that fits in 4 KB at one data width can step *across* the boundary
after `axlen` doubling. The "structurally identical" claim is therefore
load-bearing on the assumption that the SVA references `axlen` *before*
scaling — but only an explicit TB exercises this assumption.

**Recommended action.** Add `tb_nic400_width_adapter_incr_4k_cross.cpp`:
drive `M_DATA_W=64`, `S_DATA_W=32`, `RATIO=2`, master
`ar_addr=0x0FF8`, `ar_size=3`, `ar_len=7` (master burst fits in 4 KB; the
slave burst would step into the next page). Use `expect_verilator_fatal_multi`
with the assertion name. ~50 lines of TB.

---

## 7. User-written assert/cover SVAs miss `disable iff` despite the spec — **FIXED in PR #479**

> **Correction (post-publish):** the original draft of this finding said
> nic400 SVAs lacked `disable iff` and recommended either a per-module
> sweep or a TB-side reset contract. On verification with the user, the
> real shape was different — and worse.

`doc/ARCH_HDL_Specification.md:7783` states that user-written `assert`
and `cover` bodies are evaluated *"at every clock edge under the
construct's `posedge clk` with `disable iff (rst)`."* The auto-emitted
SVA family (`_auto_bound_*`, `_auto_div0_*`, `_auto_hs_*`,
`_auto_thread_*`) already honours this — `src/codegen/mod.rs` lines
2980 / 3092 / 3599 / 3680 each compute `rst_active` from the module's
`Reset<Kind, Polarity>` port and splice it into a `disable iff (...)`
clause.

`emit_assert_sva` at `src/codegen/mod.rs:2769-2791` (pre-PR-#479) was
the **only** SVA emitter that ignored reset polarity:

```rust
"{label}: assert property (@(posedge {clk}) {expr_str})"
```

— bare `@(posedge clk)`, no `disable iff`. Spec promises one shape, the
compiler emitted another. Every user-written `assert` and `cover`
across the codebase was affected — nic400, ibex, l1d, every example —
not just nic400.

**Fixed in [arch-com#479](https://github.com/arch-hdl-lang/arch-com/pull/479).**
A new helper `Codegen::rst_active_from_ports(&[PortDecl]) ->
Option<String>` resolves the active-level reset expression from a port
list (`!rst` for `Reset<_, Low>`, bare `rst` for `Reset<_, High>`,
`None` when there is no reset port). `emit_assert_sva` and
`emit_asserts_for_construct` take a new `rst_active: Option<&str>`
parameter and splice it into the `disable iff (...)` slot. All 10 call
sites (`arbiter`, `cam`, `counter`, `fifo`, `fsm`, `linklist`,
`module`, `pipeline`, `ram`, `regfile`) compute `rst_active` from the
construct's ports and pass it through. Reset-less / clock-less modules
emit no `disable iff` (matching the existing `_auto_bound_*`
behaviour).

Verified end-to-end: full `cargo test --release` is green (539 passed)
and every nic400 expect-fatal Verilator test still trips the right
`$fatal` because the violations occur outside reset. Four new
regression tests pin the active-high / active-low / cover / no-reset
shapes.

**Why the original draft was wrong.** The nic400 `.arch` source has no
`disable iff` because the syntax for it doesn't surface there — user
asserts are written as `assert name: expr;` and the compiler is meant
to wrap them with the construct-level clock and reset. Reading just
the `.arch` file makes it look like a nic400 omission; checking the
emitted SV (or the spec) reveals it's a compiler omission. Trust but
verify: the auto-emit family already proved the polarity-inference
machinery exists — the gap was that one emitter wasn't using it.

---

## 8. harc-com #321 trace diff assumes deterministic event order (LOW, tech debt)

`src/check_backends.rs::diff_trace_strings` walks both traces by line
index:

```rust
for i in 0..max {
    let arch_line = arch_lines.get(i);
    let sv_line   = sv_lines.get(i);
    // … compare normalised forms …
}
```

If a future backend (e.g. parallel arch sim, multi-thread sim) emits two
events in different orders than the single-threaded sv backend (even when
both orders are semantically valid — e.g. two TLM responses on the same
cycle), `--check-backends` will report false-positive divergences.

This is acceptable for the MVP, but the assumption is not documented in
the function comment or the `--check-backends` flag help text. The next
contributor extending the trace format risks adding a non-deterministic
event ordering and being surprised by CI failure.

**Recommended action.** Add a `// REQUIRES: backend trace lines are
emitted in deterministic cycle/timestamp order. …` doc comment above
`diff_trace_strings`, and a one-line note in the `--check-backends` help
text: "Compares traces line-by-line; backends must emit events in
deterministic order."

---

## 9. Stale "Mealy fusion" / "wait 0+ cycle until" comments after #471 (LOW, doc drift)

PR #471 retired the `wait 0+ cycle until <cond>;` syntax with a clear
parser error message. The retirement is reflected in `src/parser.rs:2235`,
in `doc/arch.ebnf`, and in the test pinning the parse error
(`tests/integration_test.rs:5281`).

But 7 nic400 testbench `.cpp` files (and `probe_ar_bubble.sh`) still
reference "Mealy fusion" or "`wait 0+ cycle until`" in comments:

```
tests/nic400/tb_nic400_fabric_latency.cpp:62
tests/nic400/tb_nic400_fabric_latency.cpp:151
tests/nic400/tb_nic400_fabric_throughput.cpp:4
tests/nic400/tb_nic400_system.cpp:196
tests/nic400/tb_nic400_fabric_write.cpp:7
tests/nic400/tb_nic400_fabric_write.cpp:30
tests/nic400/tb_nic400_ahb_bridge_burst.cpp:17
tests/nic400/tb_nic400_apb_bridge.cpp:8
tests/nic400/tb_nic400_ahb_bridge.cpp:12
tests/nic400/tb_nic400_fabric_regslice.cpp:29
tests/nic400/tb_nic400_width_adapter.cpp:20
tests/nic400/probe_ar_bubble.sh:12
```

Five of those describe `pre_edge()` sampling timing — they're historically
correct, the source-level Mealy syntax is just gone — and are arguably
fine. Three reference `wait 0+ cycle until` as the *technique*; those are
now misleading.

**Recommended action.** Sweep the three explicit references
(tb_nic400_fabric_latency:62 / :151, tb_nic400_fabric_throughput:4,
tb_nic400_system:196, probe_ar_bubble.sh:12) and update to
"`if not X; wait until X; end if`" or "fast-path lowering". Leave the
generic "Mealy" pre_edge timing notes; they describe the lowered FSM
shape, not the source.

---

## 10. SHA-256 examples landed in both repos with no cross-reference (LOW, doc drift)

arch-com #461 adds an example `arch_sha256_compression` (ARCH source).
harc-com #319 adds `tests/dut/sha256.sv` + a HARC testbench. Both landed
on 2026-05-29. Neither references the other.

If the ARCH version is the intended source-of-truth (and the SV is a
hand-written reference for HARC TB development), that's worth saying. If
the SV came first (e.g. ported from `realbench`), the ARCH example should
cite the SV as the reference implementation it matches.

Neither source cites a FIPS 180-4 known-answer vector by index. The HARC
TB does check `SHA-256("abc")` and `SHA-256("")` against expected digests,
but the digest values appear as hex constants without a citation.

**Recommended action.** Add a one-line comment header to both source
files: `// FIPS 180-4 §6.2.2; test vectors from FIPS 180-4 Appendix B.1 /
B.2.` Add a `See also:` cross-reference between the two.

---

## 11. PR #463 resolution table needs an update (LOW, meta-debt)

The post-merge resolution table in PR #463 was last updated 2026-05-29
(in PR #463 itself, before merge). Today's merges close more of the
open LOW findings:

- **§3** (arch-sim cross-check for RR on non-pow-of-2): closed by
  arch-com #465 (`tests(coverage): arch-sim cross-check for RR + AW
  expect-fatal for WRAP SVAs`).
- **§4** (AW expect-fatal coverage for WRAP): closed by arch-com #465.
- **§5** (INCR 4 KB + AxLOCK preconditions unasserted): closed by
  arch-com #466.
- **§9 remainder** (`ARCH_HDL_Specification.md` §7a.3 + Reference Card
  thread block + COMPILER_STATUS.md `--thread-sim both` + nic400
  §16.1 SVA inventory): closed across #455 / #466 / #467 (verified —
  `doc/COMPILER_STATUS.md` line 39 now states "honours the declared
  mutex policy (PR #460)", `doc/nic400_interconnect_spec.md` line 1106
  now lists the new SVAs).

That leaves only §6 (mirror-update dedup), §7 (Lifecycle AST shape — closed
by harc-com #320 §7), and §8 (ambiguous-owner topo-sort — closed by harc-com
#320 §8 + #318) actually open.

**Recommended action.** Either amend PR #463's resolution table (a small
follow-up commit on `ideas/2026-05-28-code-review-findings.md`) or
explicitly note in this 2026-05-29 doc that the §3–§9 LOW findings have
mostly closed. I've done the latter here; an explicit amend would close
the audit loop. (See Finding C in PR #463 for context on the recurring
debt-tracking pattern.)

---

## 12. SFG defers RegBlock/LatchBlock/Thread driver checks with no tracking issue (LOW, feature gap)

`src/signal_flow.rs` explicitly skips multi-driver checking for three of
the four block kinds that can drive signals:

```rust
// RegBlock (seq) multi-driver checking is deferred: the TLM
// target-thread inline lowering generates multiple RegBlocks that
// legitimately share register assignments (gated by state
// conditions), so a naive count-of-writers check produces false
// positives here.  The C-seq repro from issue #375 needs a
// follow-up PR that can distinguish user-written from
// compiler-generated blocks before this check is safe to enable.
ModuleBodyItem::RegBlock(_) => HashMap::new(),
ModuleBodyItem::LatchBlock(_) => HashMap::new(),
ModuleBodyItem::Thread(_) => HashMap::new(),
```

This means: two user-written `seq` blocks driving the same `reg` *do not*
trigger the check, and the user gets the (much less helpful) SV
"`multiple drivers`" error from Verilator instead. Issue #375 is the
original ask, but the comment refers to "a follow-up PR that can
distinguish user-written from compiler-generated blocks" with no GitHub
issue number to follow.

**Recommended action.** File a tracking issue ("SFG Phase 1 Check 1 part
2: seq/latch/thread multi-driver after `synthesized` flag") so the
deferred surface is visible. Mention it in `ideas/2026-05-28-signal-flow-graph.md`
as a known limitation.

---

## Language / architecture: no new proposal this batch

PR #463 Finding C (the recurring "param-aware vs bare codegen helper"
language footgun) was partially addressed by arch-com #464 — option 1 (delete
the bare form via `#[deprecated]`). The recurring footgun pattern
(arch-com #427, #439, #442, #458, #464) has now consumed five PRs over
~4 weeks. The remaining bare helpers (`type_width`, `cpp_port_type`,
`cpp_internal_type`, `vec_array_info`) are still live per PR #463's
Finding 1, but #464 + future mechanical migration covers the runway. No
new proposal needed.

A second pattern worth flagging — but not yet enough evidence to propose:
**doc drift between code and `COMPILER_STATUS.md`**. Finding 3 (multi-driver
check missing from status) and Finding 11 (resolution table never amended)
are the second and third instances this month. If the pattern recurs in
the next 2–3 batches, a "spec section + status row required for every
user-visible new feature" PR template check would be worth proposing.

---

## Test plan

- [ ] Land this note as the canonical record of the 2026-05-29 review pass.
- [ ] Land the two quick-win follow-up PRs (Findings 1 and 2) — internal-only,
      per `CLAUDE.md`.
- [x] Finding 7 — fixed in [PR #479](https://github.com/arch-hdl-lang/arch-com/pull/479)
      after verification surfaced it as a compiler/spec drift rather than a
      nic400-local gap.
- [ ] Queue Findings 3, 4, 5, 6, 9, 10, 11, 12 for the next batch.
- [ ] Finding 8 requires user direction (touches public CLI semantics) —
      flag for human review.
