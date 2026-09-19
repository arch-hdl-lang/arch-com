# CDC/RDC checker evidence

Compiled as an inventory of `arch-com` at tag `v0.72.2` (`3a3744e0`);
nothing was added or changed to produce the original survey. The
inventory turned up one defect — `pragma rdc_safe;` also suppressed CDC
checking (§3) — and two checks with no automated test at all (§2).
Both were fixed in **v0.72.3**; §2, §3, §5, §6 and §7 have been updated
to the post-fix state, with the pre-fix finding kept as history in §3.
§1 and §4 are unchanged from the v0.72.2 survey.

## 1. Where the checks are

| Check | Implementation | Diagnostic prefix |
|---|---|---|
| In-module cross-domain register read (seq→seq) | `src/typecheck.rs:1769` | `CDC violation: register ... is driven in domain ...` |
| Combinational fan-in to a foreign-domain flop (comb→seq) | `src/typecheck.rs:1809` | `CDC violation: comb signal ... reads register ...` |
| Instantiation-boundary crossing | `src/typecheck.rs:7200` (`check_inst_cdc`), called at `:1830` | `CDC violation at instance ...` |
| Reconvergent synchronizers (CDC and RDC variants) | RDC phase 2c walker | `CDC violation` / `RDC violation` / `RDC/CDC violation` |
| Multi-bit `kind ff` synchronizer | `src/typecheck.rs:8779` | warning |
| RDC phase 1 — one async reset in two clock domains | `src/typecheck.rs` (shares the gate at `:1743`) | `RDC violation` |
| RDC phase 2a — cross-async-reset-domain data path | `src/typecheck.rs:~1932` | `RDC violation` |
| RDC phase 2b — reset-driven clock gating | same pass | `RDC violation` |
| RDC phase 2c — reconvergent synchronizers | same pass | as above |
| RDC phase 2d — combiner-derived reset at `inst` | same pass | `RDC violation` |

Line numbers above are as of v0.72.2. After the v0.72.3 gate split the
anchors are: shared multi-clock gate `src/typecheck.rs:1744`, CDC
sub-gate `:1759` (seq→seq `:1771`, comb→seq `:1811`, `check_inst_cdc`
called at `:1832`), RDC phase-1 sub-gate `:1836`, multi-bit `kind ff`
warning `:8784`.

## 2. Automated tests: positive (must flag) and negative (must not flag)

Counted from `tests/rdc/*.arch` (verdict encoded in the filename suffix,
runner `tests/rdc/run_rdc.sh`) and the
`rdc_*`/`cdc_*`/`test_rdc_*`/`test_inst_*` functions in
`tests/integration_test.rs` (65 functions matching
`^fn (rdc_|cdc_|test_rdc_|test_inst_)` after the v0.72.3 additions, 58
before). Counts are scenario files; each also has an integration mirror,
except `cdc_p5` which asserts on a warning and so has no exit-code
scenario file.

| Check | Positive | Negative | Scenario files |
|---|---|---|---|
| In-module seq→seq CDC | 2 | 2 | `cdc_p1`, `rdc_n1` / `cdc_p2`, `rdc_n2` |
| In-module comb→seq CDC | 1 | 1 | `cdc_p3` / `cdc_p4` |
| Instantiation-boundary CDC | 1 + 1 example | 2 + 1 example | `test_inst_cross_domain_data_is_a_violation`; `test_inst_clock_domain_rebind_is_not_a_violation`; `test_nic400_cdc_axi4_rw_bridge_crosses_all_five_axi_channels`; `examples/cdc_inst_violation.arch` (recorded as `BUILD_FAIL` in `tests/portability_baseline.tsv`), `examples/cdc_inst_safe.arch` |
| Reconvergent synchronizers, CDC variant | 6 | 4 | `rdc_j1`, `rdc_j4`, `rdc_m1`–`rdc_m4` / `rdc_j2`, `rdc_j3`, `rdc_m5`, `rdc_m6` |
| Multi-bit `kind ff` warning | 1 | 1 | `cdc_p5_multibit_ff_synchronizer_warns` (integration-only: `UInt<8>` warns, `Bool` does not) |
| RDC phase 1 (cross-clock async reset) | 2 | 2 | `rdc_d1`, `test_rdc_violation_one_reset_two_domains` / `test_rdc_clean_two_resets_per_domain`, `rdc_h3` |
| RDC phase 2a (data path) | 11 + 2 | 5 + 1 | `rdc_a2`–`a4`, `b1`–`b3`, `c1`–`c3`, `d2`, `e2`, `test_rdc_guard_waiver_does_not_apply_*` (2) / `rdc_a1`, `a5`, `e1`, `f1`, `f2`, `test_rdc_guard_waiver_async_same_domain_passes` |
| RDC phase 2b (clkgate enable) | 1 | 2 | `rdc_g1` / `rdc_g2`, `rdc_g3` |
| RDC phase 2c (reconvergent, RDC variant) | 2 | 2 | `rdc_h1`, `rdc_h4` / `rdc_h2`, `rdc_h3` |
| RDC phase 2d (combiner at inst) | 2 | 2 | `rdc_k1`, `rdc_k2` / `rdc_k3`, `rdc_k4` |
| `pragma rdc_safe` suppresses each RDC phase | 1 | 4 | `rdc_n1` (must **not** suppress CDC) / `rdc_l1`–`rdc_l4` |
| `pragma cdc_safe` suppresses CDC/phase 1 | — | 2 | `rdc_n2`; inline in `rdc_d2_diff_async_diff_clocks_with_path_fail.arch`, which carries the pragma to isolate phase 2a |

Gap statement (v0.72.2): the two in-module CDC checks and the multi-bit
synchronizer warning — the most basic checks the paper describes — had
no dedicated automated test. They were exercised only incidentally.
**Closed in v0.72.3** by `cdc_p1`–`cdc_p5`; the pragma-asymmetry
regression is pinned by `rdc_n1`/`rdc_n2`.

## 3. Pragma semantics as implemented

| Property | `pragma cdc_safe;` | `pragma rdc_safe;` |
|---|---|---|
| Parsed at | `src/parser.rs:1191` | `src/parser.rs:1192` |
| Stored as | `ModuleDecl.cdc_safe` (`src/ast.rs:229`) | `ModuleDecl.rdc_safe` (`src/ast.rs:234`) |
| Scope | whole module | whole module |
| Per-signal or per-crossing waiver | no | no |
| Diagnostic emitted when present | **none** — no note, warning, or summary line | **none** |
| Unknown pragma name | parse error | parse error |
| Gates in-module CDC checks (seq→seq, comb→seq) | yes | **no** (was yes ≤ v0.72.2 — see below) |
| Gates instantiation-boundary CDC check | yes | **no** (was yes ≤ v0.72.2 — see below) |
| Gates RDC phase 1 | yes | yes |
| Gates RDC phases 2a–2d | no | yes |

**Finding (discrepancy between documentation and code) — fixed in
v0.72.3, kept here as history.** The spec
(§5.4) and the paper (§3.5) describe `rdc_safe` as suppressing "every RDC
phase" and `cdc_safe` as suppressing "CDC + phase 1," implying that
`rdc_safe` leaves CDC checking on. In the code the entire CDC pass and
phase 1 share one gate:

```rust
// src/typecheck.rs:1743
if clk_domain.len() >= 2 && !m.cdc_safe && !m.rdc_safe {
    // ... in-module CDC checks, check_inst_cdc, phase 1 ...
}
```

so `pragma rdc_safe;` also disabled all CDC checks for the module. A
module that legitimately opts out of RDC (e.g., a deliberately
multi-reset block) silently lost CDC checking.

**Resolution (v0.72.3).** The gate was split as recommended — the
multi-clock condition and the reg→domain map stay shared, the CDC
checks moved under `!m.cdc_safe`, and RDC phase 1 keeps
`!m.cdc_safe && !m.rdc_safe`:

```rust
// src/typecheck.rs:1744
if clk_domain.len() >= 2 {
    // reg → domain map, built once
    if !m.cdc_safe {
        // in-module CDC checks, check_inst_cdc
    }
    if !m.cdc_safe && !m.rdc_safe {
        // RDC phase 1
    }
}
```

No diagnostic text changed and the phase 2a–2d gate was not touched.
`rdc_n1_rdc_safe_does_not_suppress_cdc_fail` is the regression test (it
fails on a pre-fix binary, passes after); `rdc_n2` documents that
`cdc_safe` still does suppress CDC.

Note also that the gate `clk_domain.len() >= 2` means the CDC pass runs
only for modules that themselves declare two or more clock domains. A
module whose only foreign-domain signal arrives through an instance
output is covered because the instance's clock must be bound to one of
the parent's declared clocks.

## 4. Patterns not modeled — confirmed against source

| Pattern | Evidence | Behavior |
|---|---|---|
| Cross-domain synchronous reset | phase 2a only originates domains at `Async` flops (spec §5.4; `reach[f]` rule) | unflagged by design |
| RDC in non-`module` constructs | the RDC pass is invoked from the module type-checker only | unflagged |
| Cross-instance RDC reach-set propagation | reach-sets are built from `m.body` registers only; no propagation through `Inst` items | unflagged; planned |
| Derived/gated clocks keep the parent tag | `Clock<D>` output ports carry the declared `D`; no domain refinement anywhere in `src/typecheck.rs` | not a crossing in the type system |
| Combinational logic before a synchronizer | no "registered source" rule exists; phase 2c intentionally walks through comb (`rdc_m3_cdc_common_source_via_comb_fail` demonstrates the walk) | permitted |
| Static fast-to-slow timing | `freq_mhz` appears 0 times in `src/typecheck.rs`; used only in `src/sim_codegen/mod.rs` (clock periods for simulation) | no compile-time check |
| Two-clock true-dual RAM coherency | port-domain checks at connection only (`test_true_dual_ram_rejects_*` cover mapping, not coherency) | not checked |
| `.archi` interface stubs | checked at declared port domains only | opaque |
| Reset recovery/removal timing | never | STA |

## 5. Catalog correspondence (documented only)

- Reconvergent-synchronizer detection (phase 2c) is documented in the
  spec as closing "the Aldec article 2140 patterns (bit-slice splitting,
  common-source register, comb-fanout)"; tests `rdc_m1`–`rdc_m6`
  correspond one-to-one to those three patterns plus two negatives.
- The five RDC classes were described in the spec as "all five article-3
  RDC bug classes catalogued in mainstream literature." No such article
  was cited by title in code, tests, docs or the paper, so the phrase
  was **removed in v0.72.3**: the spec (§5.4) and `COMPILER_STATUS.md`
  now refer to the five classes listed in place, and claim no external
  catalog. Should a specific source be identified later, adding the
  citation is a spec edit, not a change to the checker.
- No other catalog correspondence is documented.

## 6. Run confirmation

Run **2026-09-18**, macOS (darwin 25.6.0), `arch 0.72.2` built from the
v0.72.3 fix branch at `d584046d`.

| | `tests/rdc/run_rdc.sh` | `cargo test --no-fail-fast` |
|---|---|---|
| Before the fix (`src/typecheck.rs` at the parent commit) | PASS 47 · **FAIL 1** · XFAIL 0 · total 48 | — |
| After the fix | PASS 48 · FAIL 0 · XFAIL 0 · total 48 | **1291 passed, 0 failed** |

The single pre-fix failure is the regression test for the §3 defect:

```
FAIL    rdc_n1_rdc_safe_does_not_suppress_cdc_fail   (expected violation, got pass)
```

`cargo fmt -- --check` is clean. arch binary SHA-256:

```
e13301217a6389f074494bd6f5d0a5aff5e81b8131bb971cc501a125cd4c3f9b  target/release/arch
```

That binary reports `arch 0.72.2` — it predates the version bump in the
same PR. Rebuilt after the bump, with the corpus pragma annotations in
place, the results are unchanged (48 PASS / 0 FAIL / 0 XFAIL, 1291
tests passed, `cargo fmt -- --check` clean) and the binary is:

```
21c9fb8a79cfb40f4fbaa04d4ede40efe1b5e4b7e4163ec570b2570d4b511e3b  target/release/arch   (arch 0.72.3)
```

The `tests/rdc/README.md` "Currently" column and
`tests/arch_regression_baseline.json` record PASS as of their last
refresh but are not dated per entry.

### Release identity

The fix and its tests shipped in **v0.72.3**, tag commit
`8b1f324963cd0a9664fe4912e45cc8e11167797a` (squash-merge of PR #1025,
<https://github.com/arch-hdl-lang/arch-com/pull/1025>). Release:
<https://github.com/arch-hdl-lang/arch-com/releases/tag/v0.72.3>.

Published asset SHA-256s (cargo-dist's own `.sha256` sidecars):

| Asset | SHA-256 |
|---|---|
| `arch-aarch64-apple-darwin.tar.xz` | `11da4952ca9dafe6d43690b916c2c46742bf177097a48a8aa1fdb48c2bcbf9ce` |
| `arch-x86_64-apple-darwin.tar.xz` | `a895fdddaf1c3f8804e1bb69d98c51b2a11525f75b658a0ea39b4e82073eb189` |
| `arch-aarch64-unknown-linux-gnu.tar.xz` | `eaca234f5ac09556affd6a164ef00878bc409d09e48058b2f8d87c501073846c` |
| `arch-x86_64-unknown-linux-gnu.tar.xz` | `be87e3c81c5632c35f7a9e1f186059ad361f1d9785fc4686a20b43e29a16973f` |
| `arch-x86_64-pc-windows-msvc.zip` | `8ba2b8d3ef43c2dc723e5926efea3d2b3d706677f1ebb9ae2d4dc81660ca94f5` |
| `source.tar.gz` | `16648bf8a9cb447831f6db5dff343f2d1d99363bca2cf7c738a30f455002b2ef` |

The `arch 0.72.3` binary hash recorded above reproduces byte-for-byte
from a clean `cargo build --release` of the tag commit on the same host
(macOS 15 / darwin 25.6.0, aarch64-apple-darwin), so the run totals in
this section are tied to a rebuildable artifact rather than to a
one-off local build.

The first release run failed in
`build-local-artifacts (x86_64-apple-darwin)` — the build itself
succeeded and only the artifact **upload** hit
`Failed to CreateArtifact: Unable to make request: ENOTFOUND`, a
transient GitHub Actions network fault. A `--failed` re-run of the same
workflow succeeded with no change to the tree, so the assets above are
built from the tag commit exactly as tagged.

## 7. Related observation: pragma use in the compiler's own test corpus

`tests/cvdp/` (CVDP-derived designs kept as compiler regression units,
distinct from the benchmark artifact repositories) contains four files
with `pragma cdc_safe;`: `async_filo.arch`, `findfasterclock.arch`,
`apb_dsp_op.arch`, `glitch_free_mux.arch`. All four declare two clock
domains. They suppress the CDC check rather than declare crossings. They
are not evaluated benchmark candidates and do not affect the paper's
benchmark pragma count, but they are in the public repository and
should either be rewritten with synchronizers or annotated with the
reason for the opt-out.

**Disposition (v0.72.3).** Each file was re-checked with the pragma
stripped, in its real multi-file compilation context:

| File | Violations without the pragma | Action | Generated SV |
|---|---|---|---|
| `async_filo.arch` | none | pragma removed — the `w_clk`/`r_clk` crossing already goes through the `WToRGraySync` / `RToWGraySync` synchroniser instances | byte-identical |
| `apb_dsp_op.arch` | none | pragma removed — `clk_dsp` is declared but never clocks anything; the only `seq` runs on `PCLK`, so there is no crossing | byte-identical |
| `glitch_free_mux.arch` | 2 CDC + 1 RDC | pragma kept, reason documented above it — the cross-coupled enables *are* the mechanism: `en1` (clk1) is gated on `en2` (clk2) and vice versa, so one enable can only assert after the other deasserts. A synchroniser in either path breaks the mutual exclusion | comment text propagates into the emitted SV; logic unchanged |
| `findfasterclock.arch` | 4 CDC + 1 RDC | pragma kept, reason documented above it — a genuine dual-clock period-measurement design (see below) | comment text propagates into the emitted SV; logic unchanged |

`findfasterclock` is the one file where a synchroniser rewrite would be
a real design change rather than a bookkeeping fix: the `measure_A` /
`measure_B` handshake flags each need a `synchronizer kind ff`, the
32-bit `a_count` / `b_count` counters are read across domains and would
need `kind gray` or a handshake-guarded capture, and the shared `rst_n`
needs a per-domain `synchronizer kind reset` to clear the phase-1 RDC
error. The two remaining opt-outs are now declared rather than silent.
