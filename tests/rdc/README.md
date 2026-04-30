# RDC test corpus

Standalone `.arch` scenarios for the Reset Domain Crossing checker. Each file
is a complete, parseable ARCH source that exercises one specific RDC pattern.
The expected outcome is encoded in the filename suffix:

- `*_ok.arch` — must type-check cleanly (`arch check` exits 0)
- `*_fail.arch` — must be rejected with an RDC error (`arch check` exits 1)

## Run them

```bash
cargo build --release
tests/rdc/run_rdc.sh                 # all scenarios
tests/rdc/run_rdc.sh c1              # filter by substring
```

Output classes:

- `PASS` — outcome matches the filename suffix
- `XFAIL` — `*_fail.arch` whose violation depends on a not-yet-implemented
  RDC phase. Listed in `PHASE_2A_PENDING` inside the runner. Currently empty
  (phase 2a shipped 2026-04-30). Re-add entries when scoping a follow-up
  phase (e.g. phase 2b clkgate, phase 2c reconvergent, phase 2d cross-inst).
- `FAIL` — outcome doesn't match the filename suffix; investigate.

## Coverage map

Strict textbook semantic — a flop downstream of an async-reset flop must
itself be async-reset by the **same** signal, or be separated by a
synchroniser. Sync and reset-none flops are transparent (originate no
domain, just propagate) and **always** trip the rule when any async
domain reaches their data input — they can't gate the clock-edge capture
on the upstream's async reset event.

```
reach[f] = { f.reset }            if f.reset_kind == Async
         = ⋃ reach[srcs]            otherwise

violation:
  f.Async   and any reach[src] contains a domain ≠ f.reset
  f.Sync    and reach[f] is non-empty
  f.None    and reach[f] is non-empty
```

| File | Class | Expected | Currently |
|---|---|---|---|
| `rdc_a1_same_async_direct_ok.arch` | direct edge, same async domain | ok | PASS |
| `rdc_a2_diff_async_direct_fail.arch` | direct edge, diff async domains | fail | PASS |
| `rdc_a3_async_to_sync_fail.arch` | async → sync (sync can't gate input) | fail | PASS |
| `rdc_a4_async_to_none_fail.arch` | async → reset-none | fail | PASS |
| `rdc_a5_sync_source_ok.arch` | sync source, no async upstream | ok | PASS |
| `rdc_b1_async_none_async_diff_fail.arch` | reset-less bridge, diff async ends | fail | PASS |
| `rdc_b2_async_none_async_same_fail.arch` | reset-less bridge, same async ends (mid-flop trips) | fail | PASS |
| `rdc_b3_async_sync_async_diff_fail.arch` | sync intermediate, diff async ends | fail | PASS |
| `rdc_c1_two_async_converge_at_none_fail.arch` | two domains converge at reset-none | fail | PASS |
| `rdc_c2_two_same_domain_converge_fail.arch` | same domain converges at reset-none | fail | PASS |
| `rdc_c3_async_plus_port_at_none_fail.arch` | port input + async → reset-none | fail | PASS |
| `rdc_d1_same_async_two_clocks_no_data_path_fail.arch` | shared async across two clocks (any data path) | fail | PASS (phase 1 catches) |
| `rdc_d2_diff_async_diff_clocks_with_path_fail.arch` | cross-clock with data path | fail | PASS |
| `rdc_e1_self_loop_same_domain_ok.arch` | self-loop, same async domain | ok | PASS |
| `rdc_e2_mutual_feedback_diff_domains_fail.arch` | mutual feedback, diff async domains | fail | PASS |
| `rdc_f1_single_async_domain_ok.arch` | sanity: one async domain, all flops | ok | PASS |
| `rdc_f2_no_async_flops_ok.arch` | sanity: no async resets at all | ok | PASS |
| `rdc_g1_clkgate_enable_from_async_flop_fail.arch` | clkgate enable driven by async-reset flop | fail | PASS (phase 2b) |
| `rdc_g2_clkgate_enable_from_port_ok.arch` | clkgate enable from input port | ok | PASS |
| `rdc_g3_clkgate_enable_from_sync_flop_ok.arch` | clkgate enable from sync-only-upstream flop | ok | PASS |
| `rdc_h1_reconvergent_two_syncs_same_domain_fail.arch` | one source → 2 reset syncs → same dest domain | fail | PASS (phase 2c) |
| `rdc_h2_single_reset_sync_ok.arch` | one source → 1 reset sync (no reconvergence) | ok | PASS |
| `rdc_h3_reset_syncs_to_diff_domains_ok.arch` | one source → 2 reset syncs → different dest domains | ok | PASS |
| `rdc_h4_reconvergent_three_syncs_same_domain_fail.arch` | one source → 3 reset syncs → same dest domain | fail | PASS (phase 2c) |
| `rdc_j1_cdc_reconvergent_two_ff_syncs_same_domain_fail.arch` | one data source → 2 ff-syncs → same dest domain | fail | PASS (CDC reconvergence) |
| `rdc_j2_cdc_single_ff_sync_ok.arch` | one data source → 1 ff-sync (no reconvergence) | ok | PASS |
| `rdc_j3_cdc_syncs_to_diff_domains_ok.arch` | one data source → 2 ff-syncs → different dest domains | ok | PASS |
| `rdc_j4_mixed_reset_and_data_sync_same_source_same_domain_fail.arch` | one source → 1 reset-sync + 1 ff-sync → same dest | fail | PASS (RDC/CDC mixed) |
| `rdc_k1_combiner_or_at_inst_fail.arch` | sub's Reset input driven by `rst_a \| rst_b` at inst boundary | fail | PASS (phase 2d) |
| `rdc_k2_negation_at_inst_fail.arch` | sub's Reset input driven by `not rst_a` at inst boundary | fail | PASS (phase 2d) |
| `rdc_k3_direct_reset_at_inst_ok.arch` | sub's Reset input driven directly by a parent Reset port | ok | PASS |
| `rdc_k4_sync_output_to_reset_ok.arch` | sub's Reset input driven by a `synchronizer kind reset` output (direct ident) | ok | PASS |

## Why D1 still flags (phase 1 backstop)

D1 has no register-to-register data path between the two clock domains, so the
phase-2a data-path rule alone would let it through. Phase 1 keeps flagging it
on a stricter structural rule:

> An async reset signal is bound to a single clock domain — the one its
> deassertion edge was synchronised to. Connecting it to flops in a second
> clock domain re-creates the original RDC hazard, regardless of whether
> the reset is a raw chip-level input or the output of a `synchronizer
> kind reset` upstream.

The fix is one synchroniser instance per receiving domain, each driving its
own per-domain `Reset<Async>` port. Sharing a single sync output across
domains is unsafe even when each domain's flop subset is independent.

## Relationship to the Rust integration tests

The same 17 scenarios are also encoded as Rust unit tests in
`tests/integration_test.rs` (functions `rdc_*`). The `.arch` files in this
directory are the human-readable mirror — same source, same expected
outcome — kept in sync intentionally.

When phase 2a lands, the `#[ignore]` markers in the Rust tests come off and
this directory's `XFAIL` entries flip to `PASS` simultaneously.
