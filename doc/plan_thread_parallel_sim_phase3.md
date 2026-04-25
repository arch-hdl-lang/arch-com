# Phase 3: Multi-Core Parallel Thread Sim

> **Status**: design draft. Phases 1+2 shipped (#143, #144, #147, #148, #149, #150, #151, #152). Phase 3 makes parallel sim actually parallel — i.e. faster than fsm — by mapping `thread` blocks onto multiple OS threads.

## Goals

After Phase 3:
- An N-thread design (e.g. multi-channel DMA, NoC mesh) sims at near-N× the current single-core throughput when the host has ≥N cores.
- `arch sim --thread-sim parallel-mt` is opt-in; existing `--thread-sim parallel` stays single-OS-thread (cooperative coroutine scheduling).
- All five `tests/thread/` cross-checks remain bit-identical between fsm, parallel, and parallel-mt paths.

Non-goals:
- Parallelizing `gen_module`'s comb blocks. Threads are the user-marked concurrency boundary.
- Replacing fsm or single-thread parallel paths. Both stay supported.
- Cross-process / multi-machine sim. Single binary, multi-threaded.

## Design choices (lock these before coding)

### 1. Threading model: 1:1 thread-to-OS-thread

Each `thread` block in the source maps to one std::thread. No grouping in v1. Reasons:
- Simpler scheduling (no per-group cooperative scheduler atop OS threads)
- Matches user intent: each `thread` is the parallelism unit
- For designs with > host_cores threads, OS scheduler handles oversubscription

If oversubscription becomes a bottleneck, add **thread groups** (multiple `thread` blocks per OS thread, scheduled cooperatively) as a Phase 3.5 follow-up. Verilator's `--threads N` works similarly.

### 2. Synchronization: atomic spin-wait barriers

One barrier per posedge. Every OS thread reaches the barrier before any advances:

```cpp
class Barrier {
  std::atomic<uint32_t> count{0};
  std::atomic<uint32_t> generation{0};
  uint32_t target;
public:
  Barrier(uint32_t target) : target(target) {}
  void wait() {
    uint32_t gen = generation.load(std::memory_order_acquire);
    if (count.fetch_add(1, std::memory_order_acq_rel) + 1 == target) {
      count.store(0, std::memory_order_release);
      generation.fetch_add(1, std::memory_order_release);
    } else {
      while (generation.load(std::memory_order_acquire) == gen) {
        // spin (or std::this_thread::yield() after N spins)
      }
    }
  }
};
```

~10-30 ns per barrier on x86 vs ~µs for std::condition_variable. Acceptable overhead for cycle-granular sim.

### 3. Two-region execution model

Each posedge cycle has two phases per thread, with barriers between:

```
Phase A — pre-tick comb settle:
  - Every thread evaluates its current segment's hold_comb based on
    PRIOR-tick signal values (snapshot at end of last cycle).
  - Outputs written to per-thread WRITE BUFFER, not directly to fields.
  - barrier()

Phase B — publish + tick:
  - Per-thread write buffer published to canonical fields (deterministic
    merge order: thread index 0..N-1).
  - shared(or) outputs OR-reduced; resource holders updated via CAS.
  - Each thread evaluates wait predicates against the published values,
    advances Ready/WaitCycles slots, runs coroutines until next wait.
  - Outputs of the new segment populate write buffer for next cycle.
  - barrier()

Phase C — eval() returns:
  - VCD dump (single thread, the one TB is calling from).
  - --debug log (same).
```

This matches Verilator's "active region → NBA region" pattern.

### 4. Shared signal access protocol

Three signal categories, each with a different access pattern:

| Category | Definition | Access pattern |
|---|---|---|
| **Owned-output** | Output port driven by exactly one thread's `hold_comb` | Direct write into per-thread buffer; published at barrier; no synchronization |
| **Shared-output** (`shared(or)`) | Output port driven by ≥2 threads | Per-thread buffer holds local value; barrier OR-reduces into canonical field |
| **Cross-input** | Signal one thread reads that another writes | Reader sees the value as of the last barrier; writer's update visible after publish |

Compile-time analysis (extends `collect_thread_driven_outputs`) classifies each port. Most are owned — the cross-input shape is the slow path.

### 5. Resource locks: atomic CAS

Each `_resource_<name>_holder` field becomes `std::atomic<int32_t>`. Lock acquire:

```cpp
int32_t expected;
do {
  expected = -1;  // try free first
} while (!_resource_X_holder.compare_exchange_weak(expected, my_id) && expected != my_id);
```

Priority arbitration when multiple threads contend at the same barrier: first-CAS-wins is non-deterministic. Fix: the barrier publish phase processes threads in fixed thread-index order, so lower-id threads always claim first. (Same determinism rule as Phase 2's iterated-tick `resumed[]` ordering.)

### 6. Determinism: fixed thread→core + ordered publish

Two sources of non-determinism in naive multi-threading:
- OS thread scheduling jitter
- CAS / fetch-add winner depending on hardware contention

Mitigations:
- **Fixed thread→core mapping** at construction (pthread_setaffinity_np on Linux, thread_policy_set on macOS). Optional — adds robustness on busy hosts.
- **Ordered publish** in barrier B1: iterate per-thread buffers in thread-index order, so writes/CAS ties resolve identically every run.

Result: same input + same `--threads N` ⇒ bit-identical VCD across runs.

### 7. CLI: `--thread-sim parallel-mt` (new mode)

Three modes total:

| Mode | Threads | Use case |
|---|---|---|
| `fsm` (default) | Single | Synth-equivalence ground truth |
| `parallel` | Single (cooperative coroutines) | Validates parallel runtime; cross-check reference |
| `parallel-mt` | One OS thread per `thread` block | Performance lane |

`--thread-sim both` continues to compare fsm vs parallel (single-core, deterministic). A future `--thread-sim three` could add parallel-mt to the diff once perf-mode determinism is proven.

### 8. Existing features: integration points

| Feature | Impact |
|---|---|
| `--debug` | Logging needs serialization. Cheap: each thread appends to a per-thread buffer; the eval-returning thread (TB caller) merges in thread-index order. Can use `std::vector<std::string>` per thread plus a final merge. |
| `--wave` (VCD) | Single writer (eval-caller) reads all per-thread buffers post-publish, dumps to .vcd. Same as today's single-thread path. |
| `--coverage` | Per-thread counter arrays merged at exit. Atomic-add per increment is too slow; per-thread accumulation + atomic-merge at process exit is the standard pattern. |
| Coroutines | Each OS thread runs its own coroutine slot; no scheduler-wide slot vector. The Phase 2 scheduler becomes per-thread-local. |
| Resource locks | Holder field becomes `std::atomic<int32_t>`; CAS at acquire (see §5). |
| `default when` | Soft-reset triggered by combinational pred; needs synchronization. Simplest: each thread checks pred at start of every Phase A; if true, fires its own seq + resets its coroutine. The pred is on shared signals, so all threads agree. Race-free. |

## Phased rollout (Phase 3 sub-phases)

| Sub-phase | Scope | Gate |
|---|---|---|
| 3.0 — design lock | This doc, reviewer sign-off | Approval |
| 3.1 — barrier + two-region runtime | Implement Barrier, write-buffer publish, two-region tick. Wire single-thread sim through it (no actual parallelism yet) — proves the runtime correct. | All Phase 2 cross-checks still PASS via parallel-mt with N=1 thread |
| 3.2 — N OS threads | std::thread per `thread` block; barrier-synchronized loop per OS thread. No deterministic ordering yet. | tests/thread/ all pass under `--thread-sim parallel-mt`; result correct (may be non-deterministic) |
| 3.3 — determinism | Fixed thread→core mapping; ordered publish; ordered debug log merge | Same VCD bit-identical across 10 runs |
| 3.4 — perf measurement | Benchmark on tests/axi_dma_thread/ (5 threads): cycles-per-second under fsm, parallel, parallel-mt with N=1, N=2, N=4, N=8 | Speedup ≥ 2× at N=4 vs N=1 |
| 3.5 — thread groups (optional) | If oversubscription hurts: bundle small threads. Static analysis to estimate per-thread work. | Speedup recovers when N > host_cores |

## Risks

| Risk | Mitigation |
|---|---|
| Spin-wait barriers waste CPU on under-loaded systems | After N=1000 spins, fall back to `std::this_thread::yield()`. Verilator does this. |
| Determinism breaks under contention | Ordered publish + fixed affinity. Cross-check against single-thread parallel via `--thread-sim three` once that ships. |
| Memory model bugs (missing acquire/release) | Use Sanitizer: `-fsanitize=thread` in CI for parallel-mt builds. Catches data races at runtime. |
| Resource-lock CAS livelock under heavy contention | Bounded retry (N=100) before falling back to a brief sleep. Real workloads have low contention; livelock is a pathology. |
| Coverage-counter merging serializes | Per-thread counter arrays, merged once at exit. No per-cycle synchronization. |
| --debug output interleaves non-deterministically | Per-thread buffer + merged in thread-index order at end of eval(). Deterministic. |

## Open questions for the reviewer

1. **Default-when racing** — if the soft-reset condition involves outputs from two different threads, can the threads disagree on whether to reset? *Likely no — the pred reads from the previous-cycle published values, which all threads see the same way. But worth a worked example before coding.*
2. ~~**Module-level `comb` and `seq` blocks**~~ **Decided**: run on the eval-caller thread (the one the TB calls `dut.eval()` from). This avoids spawning a "thread 0" OS thread for what is conceptually setup/wrapper logic, keeps the comb/seq close to the TB driver (lowest latency to TB-driven inputs), and matches the existing single-thread parallel sim's structure. Synchronization: place a barrier between the caller-thread comb/seq evaluation and the worker threads' Phase B publish, so workers see the just-updated reg values consistently. The caller thread effectively executes the wrapper while workers wait at the barrier — minor serialization overhead, but the wrapper is typically small (single-digit assignments).
3. **Bus ports** — none of the existing thread tests use them; deferred. But Phase 3 will want a story for bus-port writes from multiple threads (e.g. AXI shared by N initiators).
4. **Affinity on macOS** — `thread_policy_set` is allowed but advisory; on Apple Silicon it doesn't bind to perf-cores. May need to skip affinity on macOS and rely on ordered publish alone for determinism.
5. **CLI naming** — `parallel-mt` vs `parallel-threads N`? Latter is more Verilator-like. Lean: `parallel-mt` for "yes/no", with default `--threads = #thread blocks`. Override via `--threads N`.

## Estimated effort

- Sub-phase 3.1 (runtime + N=1): 2-3 sessions
- Sub-phase 3.2 (N OS threads): 2-3 sessions
- Sub-phase 3.3 (determinism): 1-2 sessions
- Sub-phase 3.4 (perf measurement + tuning): 1 session
- Sub-phase 3.5 (groups, optional): 2-3 sessions if needed

Total: roughly 6-10 focused sessions for sub-phases 3.1-3.4. Each sub-phase is independently shippable (each has its own gate).

## Verilator parity reality check

Verilator at `--threads N` achieves ~1.5-3× speedup on typical RTL designs (per the V4 paper). They do this on flattened netlist with auto-discovered MTasks. Our advantage: we have user-marked thread boundaries (no MTask discovery cost) and 2-state semantics. Realistic ceiling: comparable to or slightly better than Verilator on thread-heavy designs; comparable on thread-light designs (since we don't auto-extract parallelism from comb logic).

Reaching parity ≠ exceeding. For exceeding Verilator, we'd need: thread groups (sub-phase 3.5) + speculative comb skip (sub-phase 5 of original plan) + wait-skip optimization. Those are post-Phase 3 work.
