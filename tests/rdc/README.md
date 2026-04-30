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
- `XFAIL` — `*_fail.arch` whose violation depends on the phase-2a data-path
  reach analysis (not yet implemented). Listed in `PHASE_2A_PENDING` inside
  the runner. Will flip to `PASS` once phase 2a lands.
- `FAIL` — outcome doesn't match the filename suffix; investigate.

## Coverage map

The agreed semantic (option 1, sync flops are transparent):

```
reach[f] = { f.reset }            if f.reset_kind == Async
         = ⋃ reach[srcs]            otherwise

violation:
  f.Async       and any reach[src] contains a domain ≠ f.reset
  f.{Sync,None} and |reach[f]| > 1
```

Sync flops originate no domain — they propagate whatever async domains
reach their data input. The metastability chain only "breaks" at another
async-reset flop.

| File | Class | Expected | Currently |
|---|---|---|---|
| `rdc_a1_same_async_direct_ok.arch` | direct edge, same domain | ok | PASS |
| `rdc_a2_diff_async_direct_fail.arch` | direct cross-domain | fail | XFAIL (phase 2a) |
| `rdc_a3_async_to_sync_ok.arch` | async → sync (transparent) | ok | PASS |
| `rdc_a4_async_to_none_ok.arch` | async → reset-none | ok | PASS |
| `rdc_a5_sync_source_ok.arch` | sync source, no async upstream | ok | PASS |
| `rdc_b1_async_none_async_diff_fail.arch` | reset-less bridge, diff async | fail | XFAIL (phase 2a) |
| `rdc_b2_async_none_async_same_ok.arch` | reset-less bridge, same async | ok | PASS |
| `rdc_b3_async_sync_async_diff_fail.arch` | sync intermediate, diff async | fail | XFAIL (phase 2a) |
| `rdc_c1_two_async_converge_at_none_fail.arch` | two domains converge at none | fail | XFAIL (phase 2a) |
| `rdc_c2_two_same_domain_converge_ok.arch` | same domain converges at none | ok | PASS |
| `rdc_c3_async_plus_port_at_none_ok.arch` | port input + async → none | ok | PASS |
| `rdc_d1_same_async_two_clocks_no_data_path_fail.arch` | shared async, two clocks | fail | PASS (phase 1 catches) |
| `rdc_d2_diff_async_diff_clocks_with_path_fail.arch` | cross-clock with data path | fail | XFAIL (phase 2a) |
| `rdc_e1_self_loop_same_domain_ok.arch` | self-loop, same domain | ok | PASS |
| `rdc_e2_mutual_feedback_diff_domains_fail.arch` | mutual feedback, diff domains | fail | XFAIL (phase 2a) |
| `rdc_f1_single_async_domain_ok.arch` | sanity: one domain, several flops | ok | PASS |
| `rdc_f2_no_async_flops_ok.arch` | sanity: no async resets at all | ok | PASS |

## Relationship to the Rust integration tests

The same 17 scenarios are also encoded as Rust unit tests in
`tests/integration_test.rs` (functions `rdc_*`). The `.arch` files in this
directory are the human-readable mirror — same source, same expected
outcome — kept in sync intentionally.

When phase 2a lands, the `#[ignore]` markers in the Rust tests come off and
this directory's `XFAIL` entries flip to `PASS` simultaneously.
