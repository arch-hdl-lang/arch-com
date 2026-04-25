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

## Baseline numbers (Apple M-series)

| Workload | fsm | par N=1 | par N=2 (per-cycle) | par N=2 (batch) | par N=5 (batch) |
|---|---|---|---|---|---|
| named_thread (2 user threads, trivial body) | 39.4 Mcyc/s | 11.3 Mcyc/s | 1.9 Mcyc/s | 9.2 Mcyc/s | — |
| ThreadMm2s (5 user threads, AXI DMA logic) | 6.5 Mcyc/s | 4.0 Mcyc/s | 0.02 Mcyc/s | **14.3 Mcyc/s** | **11.9 Mcyc/s** |

**ThreadMm2s at N=2 with batching is 2.2× faster than fsm and 700× faster than per-cycle MT** — the cycle-batching API (Phase 3.5) makes multi-OS-thread parallel sim a real speedup. Per-cycle-mode N>1 remains slow on Apple Silicon (P/E core jitter at sub-µs work granularity).

## Honest interpretation

**Multi-OS-thread parallel sim is currently SLOWER than single-thread for these workloads on Apple Silicon.** The architecture is correct (5/5 cross-checks pass at N=1, named_thread bit-identical at N=2), but per-cycle work is too fine-grained to amortize barrier + cross-core cache traffic costs. ~50µs per cycle observed at N=5 vs ~250ns at N=1.

Root causes:
- Apple Silicon P/E core heterogeneity — `std::thread` can land on E-cores
- Cross-cluster cache bouncing on barrier atomics (even with cache-line padding)
- Per-cycle work is sub-µs — barrier rendezvous (~10s of µs in practice) dominates

## Path to further speedup (deferred)

- **Affinity to perf cores**: macOS `thread_policy_set(THREAD_AFFINITY_POLICY)` is advisory but worth trying.
- **Test on x86 Linux**: consistent core performance, no P/E asymmetry. Per-cycle MT mode likely usable on x86.
- **Larger per-cycle work**: real designs with deeper compute per cycle (e.g., compression, crypto) would amortize even better.

## Cycle batching usage

When using `--threads N>1` for performance, prefer `dut.run_cycles(K)` over per-cycle `dut.eval()` loops. Trade-off: per-cycle observability is sacrificed (segment switches/eval/debug/VCD only fire at batch end). Use when:
- Inputs are stable across the batch
- Throughput matters more than per-cycle waveform/log fidelity
- Long stretches of pure simulation between checkpoints

Per-cycle mode (eval() loop) still works at N>1 for correctness validation but won't deliver speedup on Apple Silicon. Use cycle batching for actual benchmarking.
