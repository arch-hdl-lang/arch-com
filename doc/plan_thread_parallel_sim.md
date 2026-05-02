# Plan: pre-lowering parallel thread simulation

> **Status (2026-05-01)**: initial coroutine-based `arch sim
> --thread-sim parallel` path exists for ordinary, pre-lowering thread
> blocks. It is not a full multi-core scheduler yet, but it can simulate
> non-TLM thread modules directly before `lower_threads`.
>
> TLM integration note: TLM target threads and initiator calls are
> consumed by TLM lowering before the parallel thread emitter runs. The
> simulator now handles this hybrid case by emitting coroutine models for
> modules that still contain ordinary threads and regular reg/seq/comb
> C++ models for modules whose TLM threads were lowered away. This keeps
> `--thread-sim parallel` working for TLM designs without teaching the
> coroutine emitter a separate TLM protocol.

## Motivation

Today every `thread` block is rewritten by `lower_threads`
([src/elaborate.rs:1156](../src/elaborate.rs#L1156)) into an `fsm` AST
node before sim_codegen runs. The post-lowering AST is the single source
of truth for both `arch sim` and `arch build`, which gives strong
sim/synth equivalence — but it costs us three things:

1. **Coverage legibility.** Synthesized state names (`_thr_0`, `_thr_1`,
   …) replace user landmarks (`wait until ready`, `fork`/`join`). The
   `coverage.dat` is correct but unreadable without the lowering map.
2. **Wait-cycle simulation cost.** `wait N cycle` cranks N posedges in
   the fsm sim; pre-lowering sim could skip directly to the wake event.
3. **Single-threaded execution.** Many-thread designs (SoCs with N DMA
   engines, NoC mesh nodes, per-channel sequencers) serialize through
   one `eval()` loop on one core. No matter how optimized the inner
   loop, ceiling = 1× core throughput.

(3) is the load-bearing motivation. (1) and (2) are real but the
annotation fix in [doc/plan_arch_coverage.md](plan_arch_coverage.md)
covers (1), and (2) is local to designs that wait a lot. (3) is a
structural ceiling we cannot raise from the post-lowering side.

## Design

### Thread runtime model

Three options; recommend **(c) hybrid**:

| Option | Per-thread cost | Parallelism | Determinism | Complexity |
|---|---|---|---|---|
| (a) Coroutines (`<coroutine>` / Boost.Context) | ~KB stack | Single-core | Trivial | Low — same as today, just cooperative |
| (b) OS threads (`std::thread`) | ~MB stack, syscall context-switch | True multi-core | Hard — needs barriers + ordered reads | High |
| (c) Hybrid: thread groups | KB stack inside group, MB across groups | True multi-core across groups | Medium — barriers between groups only | Medium |

**(c) Hybrid:** the user (or a heuristic on thread count) partitions
threads into N groups where N = available cores. Each group runs in one
OS thread; threads inside a group run as cooperatively-scheduled
coroutines. Barriers run at clock edges across groups; intra-group
threads share state lock-free because they never run concurrently.

This matches the SystemC SC_THREAD model and the Verilator
`--threads N` partitioning, both proven workable.

### Clock-edge barrier scheduler

Single-clock-domain version (extend later for multi-domain):

```
loop:
  // Phase A — combinational settle (parallel across groups)
  for each group in parallel:
    run all threads until each blocks on (wait edge | wait until cond | yield)
    settle comb signals owned by this group
  barrier()

  // Phase B — read combinational from other groups, settle again if needed
  // (fixed-point iteration — bounded by combinational path depth)
  for each group in parallel:
    re-evaluate threads whose `wait until` predicates depend on cross-group
    signals
    if any signal changed: mark dirty
  barrier()
  if any dirty: goto Phase B  (in practice converges in 1-3 iters)

  // Phase C — clock posedge: wake threads waiting on `wait edge` /
  // `wait N cycle` whose deadline matches; commit reg writes
  for each group in parallel:
    advance time, wake matching threads, commit nonblocking writes
  barrier()
```

Determinism: at each barrier, a fixed iteration order over groups (and
over threads within a group) makes inter-group signal propagation
order reproducible regardless of OS thread scheduling.

### Shared-signal access protocol

Two categories:

- **Owned signals** (declared inside a thread or written by exactly one
  thread): no synchronization. Owner writes during its phase, others
  read snapshot at the next barrier.
- **Cross-thread signals** (read by ≥1 threads other than owner): each
  group keeps a local-write buffer during Phase A; barrier publishes
  buffers; Phase B reads the published values. Same shape as
  Verilator's NBA region or VCS's PLI sample-and-update.

Compile-time analysis (already partially done by `comb_graph` and
`collect_thread_signals` in [src/elaborate.rs](../src/elaborate.rs))
identifies which signals fall in which category. Cross-group signals
get the buffer treatment; everything else is direct-access. Most signals
are owned — buffer overhead is minimal.

### Dual-pass cross-check harness

CLI: `arch sim --thread-sim=parallel`, `--thread-sim=fsm` (default),
`--thread-sim=both`.

- **`fsm`** (default): existing path, post-lowering, fully validated
  against `arch build` SV.
- **`parallel`**: new path, pre-lowering, multi-core. Used after
  cross-check confidence builds.
- **`both`**: run parallel and fsm in the same process, share the
  testbench, compare a chosen observable each cycle. Mismatch ⇒
  `ARCH-ERROR: thread sim divergence at cycle N: <signal> parallel=X
  fsm=Y` + abort.

Comparison observable: the **set of all output port values at each
positive clock edge of the top clock**. Rationale: this is the
synthesizable contract — internal sequencer states can differ
(different schedulers, possible) as long as the design's externally
observable behavior matches.

Cross-check coverage targets: every thread-using `.arch` test in
`tests/` runs `--thread-sim=both` in CI on small testbench
inputs. Large benchmarks (axi_dma multi-channel, NoC mesh) opt
into `parallel` directly once a baseline cross-check signs off.

## Phased rollout

| Phase | Scope | Gate |
|---|---|---|
| 0 — design lock | This doc, refined after one round of review | Reviewer sign-off |
| 1 — coroutine single-core | Pre-lowering thread sim using `<coroutine>`, single OS thread, wait-skip optimization. Validates the runtime model on an existing thread test (e.g. `tests/axi_dma_thread/`). Not yet faster — proves correctness. | Cross-check passes on all `tests/axi_dma_thread/*` |
| 2 — dual-pass `--thread-sim=both` | Comparison harness, divergence abort, CI integration | Green CI |
| 3 — group partitioning + OS threads | Hybrid scheduler, configurable group count, barriers | Single-clock-domain perf win measured on axi_dma 4-channel |
| 4 — multi-clock domain barriers | Per-domain scheduler, CDC-aware sync | Multi-clock thread test passes cross-check |
| 5 — wait-skip + speculative comb | Pre-lowering perf optimizations that the FSM path cannot match (skip `wait N cycle`, batch comb-only updates) | Measurable speedup on `wait`-heavy benchmark |

## Non-goals (v1)

- **Thread-level parallelism in `gen_module`** (parallelizing comb
  blocks). Out of scope — only thread blocks are user-marked as
  concurrent. Module comb blocks have implicit ordering.
- **Replacing the FSM path.** The post-lowering FSM sim stays the
  default and the synth-equivalence ground truth. The parallel path is
  an opt-in performance lane.
- **GPU / SIMD execution.** Coarse-grained thread parallelism only;
  bit-level vectorization is a separate workstream.

## Risks

| Risk | Mitigation |
|---|---|
| Sim/synth divergence in parallel path missed by cross-check | Cross-check observable is exhaustive (all output ports per cycle). Any divergence aborts. CI gate prevents regression. |
| Thread-runtime maintenance burden doubles sim_codegen | Coroutine path is ~500 LOC; hybrid scheduler ~300 LOC on top. Cap accepted in exchange for the perf lane. Reviewable per-phase. |
| Determinism breaks when porting C++ across compilers / OSes | Fixed barrier order + fixed group→thread mapping makes scheduling reproducible. Test in CI on macOS + Linux. |
| Parallel speedup eaten by barrier overhead on small designs | Heuristic: fall back to single-OS-thread (coroutine-only) when thread count < 2× core count. User can override. |
| `wait until cross-group-signal` introduces inter-group dependency cycles | Phase B fixed-point iteration converges in bounded steps; if not, error out with a "combinational cycle across thread boundary" diagnostic at compile time (extend `comb_graph` to thread-cross edges). |

## Prior art: what to borrow from Verilator

Verilator's `--threads N` mode (production since ~2019) solves a harder
version of the same problem (parallelism discovered from a flattened
RTL netlist rather than user-marked at the source). Several specific
techniques transfer directly:

1. **Static MTask partitioning.** The dependency graph between parallel
   work units is computed at C++ generation time and baked into the
   emitted scheduler. No runtime work-stealing, no per-cycle thread-pool
   decisions. For us: each `thread` block (and possibly each independent
   module instance) is one MTask analog. We get the partitioning for
   free — the user already drew the boundaries.
   *Source:* `V3Partition.cpp` in Verilator.
2. **Lock-free per-edge sync words.** Inter-MTask synchronization is one
   `std::atomic<uint64_t>` per dependency edge: producer increments at
   completion, consumer spins until ≥ expected. ~10 ns per barrier vs
   ~µs for OS mutex/condvar. Load-bearing perf trick.
   *Source:* `verilated_threads.h` (`VlMTaskVertex`,
   `VlAssignableWaitable`).
3. **Two-region evaluation with one barrier per cycle.** Settle
   combinational across all threads → barrier → commit NBA writes →
   barrier → advance time. Maps to our Phase A/B/C scheduler; we may be
   able to collapse to one barrier per posedge if the comb fixed-point
   stays intra-thread.
4. **Hash-stable MTask ordering.** Topological-sort tiebreak on a hash
   of the MTask's source position guarantees same input + same
   `--threads N` = same execution order = bit-identical traces. **We
   need exactly this for `--thread-sim=both` cross-check to be
   meaningful** — without reproducibility the divergence detector fires
   on legitimate scheduling jitter.
5. **Per-thread bump-arena allocators.** Worker threads never call
   `malloc` on the hot path; signal buffers come from per-thread arenas
   reset at the cycle boundary.

What **doesn't** transfer or isn't needed:

- Verilator's dynamic re-partitioning + cost model (added in 5.x). At
  `thread`-granularity (tens to low hundreds of parallel units, not
  thousands of fine-grained MTasks), static one-thread-per-MTask is
  enough. Revisit only if a real consumer hits a load-balance ceiling.
- Sub-module / sub-`always_comb` MTask splitting. Verilator needs this
  because it has no source-level concurrency hint; we do.
- `force`/`release`/hierarchical-reference handling. ARCH has no
  equivalent SV testbench constructs, so the cross-thread access
  protocol stays simpler than Verilator's.

Both Verilator and ARCH are **2-state by default** — same integer-typed
signal storage, same lack of X/Z propagation cost. So the parallel-sim
techniques above port over without semantic adjustment.

References:
- Snyder, *Verilator and SystemPerl: Open Source SystemVerilog
  Simulation* — original SystemPerl/Verilator paper.
- Lane, et al., *Verilator V4.0 multithreaded simulation* — DVCon
  presentation describing the MTask scheduler and sync-word design.
- Verilator source: `src/V3Partition.cpp` (graph partitioning),
  `include/verilated_threads.h` (runtime).

## Open questions

- Coroutine library: C++20 `<coroutine>` (clean but heap-allocates frames
  by default), Boost.Context (stackful, fast switch), or roll-our-own
  ucontext. Defer to Phase 1 prototype.
- Should `--thread-sim=both` run in a single process (shared TB,
  in-memory comparison) or two processes (subprocess + diff trace)?
  Single-process is faster but couples the two sim binaries; two-process
  cleaner but slower. Lean single-process.
- Does the cross-check observable need to include reg-state at clock
  edges, or only ports? Reg-state catches more bugs but is brittle
  across legitimate schedule differences. Lean ports-only with reg-state
  as an opt-in `--thread-sim=both --strict` mode.
