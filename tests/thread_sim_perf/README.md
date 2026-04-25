# Thread-sim performance benchmarks

Phase 3.4 baseline measurements. Run with:

```
arch sim --thread-sim parallel --threads N <design.arch> --tb <perf_tb.cpp>
```

Where `--threads N` selects fsm path equivalents:
- `--thread-sim fsm` (no --threads) — fsm baseline
- `--thread-sim parallel --threads 1` — single-OS-thread cooperative
- `--thread-sim parallel --threads N` (N>1) — multi-OS-thread

Each TB takes an optional cycle count (default 1M) and reports cycles/sec.

## Baseline numbers (Apple M-series, Phase 3.4)

| Workload | fsm | par N=1 | par N=2 | par N=5 |
|---|---|---|---|---|
| named_thread (2 user threads, trivial body) | 39.4 Mcyc/s | 11.3 Mcyc/s | 1.9 Mcyc/s | — |
| ThreadMm2s (5 user threads, AXI DMA logic) | 6.5 Mcyc/s | 4.0 Mcyc/s | 0.02 Mcyc/s | 0.02 Mcyc/s |

## Honest interpretation

**Multi-OS-thread parallel sim is currently SLOWER than single-thread for these workloads on Apple Silicon.** The architecture is correct (5/5 cross-checks pass at N=1, named_thread bit-identical at N=2), but per-cycle work is too fine-grained to amortize barrier + cross-core cache traffic costs. ~50µs per cycle observed at N=5 vs ~250ns at N=1.

Root causes:
- Apple Silicon P/E core heterogeneity — `std::thread` can land on E-cores
- Cross-cluster cache bouncing on barrier atomics (even with cache-line padding)
- Per-cycle work is sub-µs — barrier rendezvous (~10s of µs in practice) dominates

## Path to actual speedup (deferred)

- **Cycle batching**: run K cycles between barriers when no inter-thread observable changes (Verilator's "MTask" approach). Amortizes barrier cost over K cycles.
- **Affinity to perf cores**: macOS `thread_policy_set(THREAD_AFFINITY_POLICY)` is advisory but worth trying.
- **Test on x86 Linux**: consistent core performance, no P/E asymmetry. Likely to show smaller (but still positive) overhead.
- **Larger per-cycle work**: real designs with deeper compute per cycle (e.g., compression, crypto) would amortize better.

These are real follow-up engineering, not architectural fixes. Phase 3.4 result: the parallel-MT machinery works correctly but doesn't deliver speedup yet for fine-grained per-cycle workloads.
