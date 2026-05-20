# GPU co-simulation plan — `arch sim --backend gpu`

Status: **Proposed (RFC, not yet implemented).**
Date logged: 2026-05-20.
Companion: NVIDIA GEM paper (DAC 2025, NVlabs/GEM open source);
sister project `harc-com`'s `tb-ir-plan.md` and
`constraint-ir-design.md` for the TB-side architecture.

This doc specifies how `arch sim` grows a GPU backend that runs DUT
evaluation on CUDA, paired with `harc`-emitted on-device TB stimulus
and assertion machinery. The plan integrates GEM's "RTL as software
FPGA emulator" approach with the architectural advantage we have
that GEM doesn't: we own the testbench compiler.

The headline goal: turn `arch sim` from a single-design CPU process
into a GPU regression engine that runs N replicas of a DUT × M
batched cycles per kernel launch, with TBs compiled to device-side
state machines that only sync to the host on observable events.

## Why now

Three things converge:

1. **GEM proved the prior GPU-RTL graveyard wrong.** Manticore,
   RTLflow, and the rest tried event-driven or RTL-block-level
   parallelism and got beaten by Verilator because SIMT divergence
   wrecks heterogeneous-op warps. GEM ducks the problem by
   bit-blasting to AIG and treating the GPU as a software FPGA
   emulator. 5–40× over best CPU simulators on representative
   designs, 64× peak on NVDLA. The mapping toolchain is open source.

2. **`arch sim` already has structural alignment with what GEM wants.**
   2-state semantics, post-elaboration IR at codegen time, single
   driver per signal (no atomics in the eval graph), explicit `Clock<Domain>`
   tracking, first-class `ram` construct, structurally-lowered TLM
   methods, threads lowered to FSM by `lower_threads`, bus ports
   already flattened in `arch build`. None of GEM's usual ingest
   pain applies.

3. **`harc-com` is the TB compiler we own.** The hardest open
   problem in the prior round of GPU-RTL work — how to batch
   cocotb-style per-cycle yield/await across PCIe — collapses to
   nothing because `harc` is ours and we can compile testbenches
   straight to device-side FSMs. The "TB on device" idea that's a
   research project for other groups is just a codegen target for us.

## Target workloads

arch's design center is microarchitecture exploration: caches,
predictors, schedulers, arbiters, coherence protocols, accelerator
pipelines. Two parallelism axes matter for these:

- **Inter-replica:** "run 10k random seeds against this branch
  predictor overnight" — every replica runs identical kernel code
  on its own state. Naturally lockstep, zero SIMT divergence by
  construction. Saturates a GPU regardless of DUT size.
- **Intra-design:** "this big SoC has millions of gates; spread
  evaluation across SMs." Requires GEM-style packing/placement/
  routing on the netlist; the hard part of any GPU-RTL system.

For arch's typical workloads inter-replica is the primary axis. Most
arch DUTs (a cache controller, a TLB, a victim buffer, an arbiter)
have ≤100k gates — not enough to fill even one SM intra-design. With
1024 replicas times the same kernel, an A100 saturates trivially.
Intra-design is the dominant axis only for full-chip regressions on
large designs.

The v1 backend prioritizes inter-replica and treats intra-design as
a downstream optimization that lights up automatically once the
inter-replica path is working.

## Scope

This doc covers:

- **Backend dispatch.** `arch sim --backend gpu` and how it interacts
  with existing flags (`--debug`, `--debug+fsm`, `--wave`,
  `--check-uninit`, `--cdc-random`, `--thread-sim parallel|fsm|both`).
- **The arch-side codegen.** Emitting CUDA device code for the DUT
  eval kernel, lane-indexed state layout, port marshaling ABI.
- **Host-device ABI.** How `harc`-emitted host orchestration drives
  the kernel: stimulus buffers, batch boundaries, trigger predicates,
  exit reason codes, trace writeback.
- **Determinism contract.** What "bit-identical" means on GPU and
  the lint pass that enforces it.
- **Migration plan.** Phased delivery with parity gates against the
  CPU sim.

This doc does **not** cover:

- The intra-design partitioning/packing/placement problem (the
  GEM-shaped hard work). v1 either skips it (small designs saturate
  via replicas alone) or vendors GEM as a library; see "Phase 5 and
  beyond" for the placeholder.
- TB compilation details. That's `harc-com`'s
  `docs/tb-ir-plan.md`; this doc cross-references the ABI but
  doesn't specify the TB-IR.
- HDL syntax changes. None required.

## Research summary — what GEM does

For the record so future readers don't have to re-derive it. GEM's
prior-art-beating moves:

1. **Synthesize the design into an AIG** (and-inverter graph) — a
   uniform 2-input AND + inverter netlist. The same primitive Yosys
   and ABC use, with optionally LUT-packed extensions.
2. **Map the AIG onto a "virtual manycore Boolean processor"** with
   a **VLIW** instruction set. Every warp lane runs identical VLIW
   bundles on different gate state; instruction uniformity is what
   makes SIMT happy.
3. **The mapper is the paper.** Packing (gathering critical-path
   chains into the same issue bundle), partitioning (width-bounded
   per virtual processor), placement (timing-driven bit placement),
   routing (inter-processor signal wiring), RAM mapping (dense
   memories can't lower to AIG). The Rust frontend is ~91% of LOC;
   ~6% is the CUDA interpreter kernel.
4. **One-time per-design mapping cost.** Slow synth+map (minutes to
   hours); fast simulation thereafter (5–40× Verilator).

Why bit-blasting wins where prior word-level GPU sims failed: a
small uarch DUT only has thousands of word-level ops per cycle —
not enough work to saturate an H100. AIG explodes 1 add into ~96
gates; sounds wasteful but gives the GPU enough uniform parallel
work to fill warps. Heterogeneous word-level ops divergent warps;
homogeneous bit-level ops don't.

**The bit-blast tax is real but bounded** — datapath-heavy designs
(NVDLA, multiplier-heavy accelerators) pay the most because a tensor
op that's one `dp4a` instruction becomes hundreds of AIG gates.
arch's design center is control-heavy (caches, predictors, schedulers)
so the tax matters less than for NVDLA.

## The TB-on-device insight

Most prior GPU-RTL work struggles with this: per-cycle CPU↔GPU sync
for testbench I/O is ~10 µs/cycle vs. ~70 ns/cycle for a CPU sim
(arch sim does 14.3 Mcyc/s on `named_thread`). PCIe round-trips
make the GPU 100–150× *slower* than the CPU you already have. Any
honest GPU plan has to avoid per-cycle sync.

The standard answer is **batched cycles with compiled triggers**:
DUT state resident on device, K cycles of pre-scheduled inputs per
launch, output trace written into a device-side buffer, `wait until
<signal>` compiles to an in-kernel predicate that early-exits on
fire. K is the checkpoint interval; somewhere in 100–1000 is a
reasonable starting range.

What makes our story unusual: **`harc` owns the TB compiler**, so
we don't have to coalesce per-cycle yields across an opaque runtime
(the cocotb problem). The harc compiler statically partitions TB
code into three buckets:

- **Stimulus-only regions** (sequence drivers, scripted vectors,
  constrained-random seeds with no DUT observation) — lift to device
  memory once, kernel reads at the right cycle, zero sync.
- **DUT-observing regions that don't talk to the host** (assertion
  checks, coverage updates, protocol monitors, scoreboards) — compile
  to `__device__` code that runs every cycle inside the eval kernel.
  Pass/fail/coverage state lives on device, drains at end-of-batch.
- **Host-talking regions** (explicit `log`, `fatal`, interactive
  prompts, `randomize` calls that need Z3) — kernel exits with a
  tagged reason code; host handles it; kernel resumes.

The first two cover almost everything a regression TB does. PCIe
sync is paid only at batch boundaries and on the rare host-talking
exit.

This is "transactor offload" in FPGA-emulator terminology
(Palladium/ZeBu/Veloce have done this for decades), but cleaner
because we're not bridging a vendor API — both sides compile from
the same source.

## Architecture

```
                ┌────────────────────────────────────────────────┐
                │  host orchestrator (harc-emitted C++)          │
                │  - per-replica stimulus buffer fill            │
                │  - Z3 pre-solve for randomize call sites       │
                │  - batch launch + exit-reason dispatch         │
                │  - trace ringbuffer drain                      │
                └─────────────────┬──────────────────────────────┘
                                  │  CUDA stream
                                  ▼
   ┌──────────────────────────────────────────────────────────────┐
   │  eval kernel (one launch per K cycles per N replicas)        │
   │                                                              │
   │  ┌─────────────────────────┐    ┌─────────────────────────┐  │
   │  │  DUT state (arch-emit)  │    │  TB state (harc-emit)   │  │
   │  │  - regs (lane-indexed)  │◄──►│  - FSM state per replica│  │
   │  │  - RAMs (lane-indexed)  │    │  - scoreboards, counters│  │
   │  │  - shadow-copy buffers  │    │  - trigger predicates   │  │
   │  └─────────────────────────┘    └─────────────────────────┘  │
   │                                                              │
   │  per cycle, per lane:                                        │
   │    1. TB-FSM emits this-cycle stimulus into DUT input ports  │
   │    2. DUT_eval(): comb settle → posedge → comb settle        │
   │    3. TB-FSM observes DUT outputs, advances FSM, runs        │
   │       assertions/coverage, evaluates trigger predicates      │
   │    4. If any trigger fires or batch ends → record exit       │
   │       reason; lane parks until kernel exit                   │
   └──────────────────────────────────────────────────────────────┘
                                  │
                                  ▼  pinned-memory ringbuffer
                ┌────────────────────────────────────────────────┐
                │  host: drain trace, handle exits, refill, loop │
                └────────────────────────────────────────────────┘
```

Two parallelism axes:

- **Replica (lane within warp)**: N replicas of the same DUT+TB
  code in lockstep; warp lanes diverge only when a trigger fires on
  some lanes and not others (handled with a per-lane "parked" flag
  + tail of dead cycles).
- **Cycle (sequential per lane)**: K cycles per launch, no
  inter-cycle sync to host within a batch.

Optional third axis (Phase 5+): intra-design partitioning across
SMs, only for designs large enough to need it.

### State layout

Per-replica state lives in `__device__` arrays indexed by lane:

```cpp
// Generated by arch sim --backend gpu
struct DutState_Foo {
    uint32_t  port_clk      [N_REPLICAS];
    uint32_t  port_data_in  [N_REPLICAS];
    uint32_t  port_data_out [N_REPLICAS];
    uint32_t  reg_state     [N_REPLICAS];
    uint32_t  reg_state_next[N_REPLICAS];   // shadow for posedge commit
    uint64_t  ram_cache     [N_REPLICAS][CACHE_DEPTH];
    // ... per arch IR
};
```

Lane-indexed layout means all 32 lanes in a warp read/write adjacent
memory for the same logical state — perfectly coalesced. No SoA-vs-AoS
ambiguity; per-lane is always the inner index.

Wide registers (>64b) become arrays of words; existing
`VlWide<N>`-style packing in arch's C++ sim translates directly.

### Eval kernel structure

```cpp
__global__ void eval_Foo(
    DutState_Foo*       dut,
    TbState_FooTb*      tb,
    StimBuf*            stim,         // pre-staged per-replica inputs
    TraceRing*          trace,
    ExitReason*         exits,        // per-replica
    uint32_t            k_cycles,
    uint32_t            base_cycle
) {
    uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= N_REPLICAS) return;

    for (uint32_t c = 0; c < k_cycles; ++c) {
        if (exits[lane] != ExitReason::None) continue;   // parked

        uint32_t cycle = base_cycle + c;

        // 1. TB drives stimulus (compiled from harc TB-FSM)
        tb_step_drive(tb, dut, stim, lane, cycle);

        // 2. DUT evaluation: comb settle + posedge + comb settle
        dut_eval_comb(dut, lane);
        if (rising_edge(dut, lane)) {
            dut_eval_seq(dut, lane);
            dut_commit_shadow(dut, lane);
            dut_eval_comb(dut, lane);
        }

        // 3. TB observes + advances FSM + asserts + coverage
        tb_step_observe(tb, dut, lane, cycle);

        // 4. Triggers
        ExitReason r = tb_check_triggers(tb, dut, lane, cycle);
        if (r != ExitReason::None) {
            exits[lane] = r;
            trace_record_event(trace, lane, cycle, r);
        }

        // 5. Optional waveform sampling — sparse, opt-in
        if constexpr (TRACE_WAVE) {
            trace_sample_ports(trace, dut, lane, cycle);
        }
    }
}
```

The shape mirrors arch's existing C++ `eval()` body — comb settle,
edge detection, posedge body, shadow-commit, re-settle — just made
warp-aware via lane indexing.

### Host-device ABI

The contract between arch-emitted DUT code and harc-emitted TB code:

```cpp
// arch emits:
struct DutPorts_Foo {
    // every input port → DutState_Foo::port_<name>[lane] reference
    // every output port → ditto
};

__device__ void dut_eval_comb (DutState_Foo*, uint32_t lane);
__device__ void dut_eval_seq  (DutState_Foo*, uint32_t lane);
__device__ void dut_commit_shadow(DutState_Foo*, uint32_t lane);
__device__ bool rising_edge   (DutState_Foo*, uint32_t lane);

// harc emits:
__device__ void tb_step_drive   (TbState_*, DutState_Foo*, StimBuf*, uint32_t lane, uint32_t cycle);
__device__ void tb_step_observe (TbState_*, DutState_Foo*, uint32_t lane, uint32_t cycle);
__device__ ExitReason tb_check_triggers(TbState_*, DutState_Foo*, uint32_t lane, uint32_t cycle);
```

The ABI is the seam where arch and harc meet on device. It's
versioned (`ABI_VERSION` constant, bumped on breaking changes) and
the harness emits a compile-time `static_assert` that the two
compilers agree.

### Exit reasons

```cpp
enum class ExitReason : uint8_t {
    None              = 0,
    BatchComplete     = 1,
    LogPending        = 2,    // log() that needs host stderr
    FatalCalled       = 3,    // fatal("..."); lane permanently dead
    AssertFailed      = 4,
    RandomizeNeeded   = 5,    // Z3 call ran out of pre-solved tuples
    UserTrigger       = 6,    // harc wait_until predicate fired
    BoundsViolation   = 7,    // mirrors arch's --check-uninit aborts
    DivByZero         = 8,
    UninitRead        = 9,
    Timeout           = 10,
};
```

The host runtime dispatches on these and either:
- handles + relaunches (LogPending, RandomizeNeeded, UserTrigger)
- accumulates + reports at end (AssertFailed, FatalCalled)
- aborts the whole run (BoundsViolation, DivByZero, kernel internal error)

### Determinism contract

GPU non-determinism shows up via atomics and reductions (atomicAdd
ordering, warp-shuffle reduction order). arch's IR has neither in
the eval graph by construction: single-driver-per-signal at type
check time means every cross-thread write goes through statically-
indexed memory and is sequenced by grid sync.

The contract: **bit-identical results per compiled bitstream across
driver/hardware versions, conditional on the codegen never emitting
`atomicAdd` / `atomicMax` / warp-shuffle / cooperative reductions in
the eval kernel.** Enforced by a lint pass over generated CUDA that
runs in CI.

Realistic exceptions:
- Floating-point ops: arch HDL doesn't have FP today, so n/a.
- IEEE 754 special-case handling: ditto.
- Timer-based triggers: not GPU-side, host-side; doesn't affect
  determinism of the eval graph.

Reduced contract for `--cdc-random`: the host derives a per-replica
LFSR seed and uploads it; the device-side LFSR is deterministic per
seed. CPU sim and GPU sim produce different LFSR sequences (the LFSR
itself is fine; the question is whether `skip_pct` decisions match
across backends). Document loudly as a known seed-equivalence
boundary, not a determinism violation.

## Backend dispatch

```
arch sim --backend gpu Foo.arch --tb tb.cpp                # 1 replica, default K
arch sim --backend gpu --replicas 1024 Foo.arch --tb tb.cpp
arch sim --backend gpu --replicas 1024 --batch 256 Foo.arch --tb tb.cpp
arch sim --backend cpu Foo.arch --tb tb.cpp                # explicit; today's default
arch sim --backend both --replicas 64 Foo.arch --tb tb.cpp # cross-check mode
```

Interactions with existing flags:

| Flag | GPU backend behavior |
|---|---|
| `--debug` / `--debug+fsm` | port traces emitted to device ringbuffer, host-drained per batch; FSM transitions tagged with cycle + cause as today. Per-replica indexing in output. |
| `--depth N` | sub-module traces: arch flattens at AIG synth, so hierarchical labels need a sidecar emitted by `arch build --keep-instance-labels`. v1 supports top-level only; doc as known limitation. |
| `--wave out.fst` | per-replica FST sampling; opt-in trace-port set to avoid PCIe bandwidth blowup at K×ports×replicas. |
| `--check-uninit` / `--inputs-start-uninit` | per-lane `_vinit` bitset; warning drained at batch end. |
| `--check-uninit-ram` | per-(lane, cell) valid bit; same. |
| `--cdc-random` | embed LFSR in kernel; seed plumbed from host. |
| `--thread-sim parallel` | **rejected** for GPU backend — coroutines aren't synthesizable. `lower_threads` to FSM is forced. |
| `--thread-sim both` | extended to also diff GPU backend against CPU backend on a corpus. Becomes the v1 parity gate. |

## Migration plan

Eight phases. Each phase ships as a separate PR; the cross-check
fixture corpus is the parity gate from Phase 3 onward.

| Phase | Status | Scope | Gate |
|---|---|---|---|
| 0 — vendor GEM | not started | Add NVlabs/GEM as a git submodule under `vendor/gem`. Smoke test: synth + map a hand-picked example design through GEM's existing CLI, confirm bitstream produced. No arch integration yet. | Build clean on Linux + CUDA toolkit; one design's bitstream produced. |
| 1 — DUT codegen skeleton | not started | New `src/sim_codegen_gpu/`. Emit CUDA for the smallest viable design subset: scalar ports, `reg` with sync reset, `comb` block, single `Clock`. No RAM, no Vec, no FSM/pipeline/fifo/ram/cam, no TLM. Generates compilable CUDA + a no-op host orchestrator. | Smallest arch fixture (counter / shift register) compiles to CUDA and links; doesn't have to run on real hardware yet. |
| 2 — host orchestrator + 1-cycle batch | not started | Host launches the kernel for K=1, single replica. PCIe round-trip per cycle (slow on purpose; baseline for the batching win). Bounds/div0/uninit checks mirror to device-side error flags. | On cloud GPU rental: counter fixture runs 1000 cycles and produces same outputs as `arch sim --backend cpu`. |
| 3 — batched cycles (K>1) | not started | DUT state resident on device, K cycles per launch, stimulus buffer pre-staged. No TB-on-device yet — host drives stimulus through a buffer. | K=100 produces identical results to K=1; K=1000 does too. Throughput measurement vs CPU sim documented. |
| 4 — TB-on-device via harc ABI | not started | Coordinate with harc-com (its TB-IR has to be in place). Harc emits `tb_step_drive` / `tb_step_observe` / `tb_check_triggers`; arch + harc emissions link into one binary. | One end-to-end harc TB runs on GPU and produces same scoreboard output as CPU. |
| 5 — Replica-parallel (N>1) | not started | Lane-indexed state; warp-aligned launches. Per-replica seed plumbing. | 1024 replicas of a small DUT (cache controller, branch predictor) produce 1024 distinct seed-stable outputs in one launch. |
| 6 — Wave + sparse trace | not started | Opt-in trace-port set; per-replica FST writer. Device ringbuffer, async host drain. | One fixture's wave output matches CPU FST within reordering tolerance. |
| 7 — Cross-check mode | not started | `--backend both --replicas N` runs CPU + GPU side by side, diffs port traces every cycle, aborts on first divergence. | 56 harc fixtures (or arch's equivalent corpus) pass cross-check for N=4 replicas, K=100. |
| 8 — Intra-design partitioning (deferred) | future | Wire GEM's mapper for designs too large for replica saturation. AIG synth path. Hierarchical label sidecar for `--depth N`. | Full-SoC fixture saturates SM utilization above replica-only baseline. |

Phase 8 is explicitly deferred — for arch's typical workloads,
replica-parallel saturates without it. Revisit if (a) measurement
shows large-DUT workloads are common, or (b) the bit-blast tax on
NVDLA-style designs is a felt pain point.

## Phase 1 starting sketch

```rust
// src/sim_codegen_gpu/mod.rs

pub struct GpuCodegen<'a> {
    pub module:   &'a Module,
    pub abi_ver:  u32,
    pub replicas: u32,
    pub batch_k:  u32,
}

pub struct GpuModel {
    pub class_name:  String,
    pub cuda_device: String,    // __device__ functions
    pub cuda_global: String,    // __global__ eval kernel
    pub host_glue:   String,    // C++ launcher + state allocators
    pub abi_header:  String,    // shared with harc-emitted TB
}

impl<'a> GpuCodegen<'a> {
    pub fn generate(&self) -> GpuModel { /* ... */ }

    fn emit_dut_state_struct(&self) -> String { /* ... */ }
    fn emit_dut_eval_comb(&self) -> String { /* ... */ }
    fn emit_dut_eval_seq(&self) -> String { /* ... */ }
    fn emit_dut_commit_shadow(&self) -> String { /* ... */ }
    fn emit_eval_kernel(&self) -> String { /* ... */ }
    fn emit_host_glue(&self) -> String { /* ... */ }
}
```

The codegen mirrors `src/sim_codegen/`'s structure: a `mod.rs`
dispatcher plus per-construct emitters (`gen_module`, `gen_fsm`,
etc. — but for GPU). v1 only needs `gen_module` + scalar `reg` + scalar
`port`. The other emitters land in later phases.

Lint pass `forbid_atomics_in_eval_kernel` runs over generated CUDA
in CI: rejects any occurrence of `atomicAdd` / `atomicMax` / `__shfl_*`
/ `cooperative_groups::reduce` inside `__device__` functions reachable
from the eval kernel. Determinism contract enforced at the source
level.

## Development without an NVIDIA GPU

The vast majority of the work doesn't need a GPU during development:

- **Phases 0–1, ~80% of v1 codegen work**: CPU-only. Building the
  Rust emitter, writing tests against generated CUDA source as
  text, integrating GEM's mapper (Rust-only). `nvcc` can compile
  CUDA on a CPU-only box to catch syntax errors; it just won't run.
- **Phases 2–7, end-to-end smoke tests**: need real CUDA. Cheapest
  options:
  - Google Colab free tier (T4, intermittent)
  - vast.ai community GPUs (~$0.10–0.30/hour for T4/A4000)
  - Modal / RunPod / Lambda ($0.50–2/hour, managed)
  - AWS/GCP/Azure spot
- **CPU-side CUDA emulators** (GPGPU-Sim, POCL, ZLUDA) exist but
  are slow (10⁴× slower than real hardware for GPGPU-Sim) and/or
  fragile. Useful for behavioral correctness in a pinch; not for
  perf measurement.

Practical recommendation: develop on CPU, validate behavior on a
$0.20/hour vast.ai box at each phase boundary. $20 of cloud time
gets you ~100 hours of validation — well past any reasonable v1
timeline.

## Non-goals

Explicit list to head off scope creep:

- **No X-state.** GPU sim is 2-state. arch sim is already 2-state;
  CPU↔GPU parity preserved.
- **No SystemVerilog ingestion.** GPU backend consumes arch IR
  post-elaboration, same as `sim_codegen`. SV users go through
  `arch build` first, then synth their SV → AIG outside this path.
- **No coroutine threads.** `--thread-sim parallel` rejected;
  `lower_threads` is forced.
- **No `todo!` in reachable code.** Hard-rejected at codegen entry.
- **No reduction primitives in the eval kernel.** `forbid_atomics`
  lint enforces.
- **No per-cycle host sync.** All host-talking exits batched at
  trigger boundaries.
- **No SV-emitted-then-re-synthesized roundtrip.** v1 emits CUDA
  directly from arch IR (skipping the `arch build` SV detour
  proposed in earlier sketches) so debug labels and bounds-check
  metadata survive.

## Worked example

A 16-bit counter:

```arch
module Counter
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port count: out UInt<16>;

  reg cnt: UInt<16> reset rst => 0;

  comb count = cnt; end comb

  seq on clk rising
    cnt <= cnt + 1;
  end seq
end module Counter
```

Phase 1 emission (sketch):

```cpp
// abi_header (shared with harc TB):
constexpr uint32_t COUNTER_ABI_VERSION = 1;
struct DutState_Counter {
    uint8_t  port_clk   [N_REPLICAS];
    uint8_t  port_rst   [N_REPLICAS];
    uint16_t port_count [N_REPLICAS];
    uint16_t reg_cnt    [N_REPLICAS];
    uint16_t reg_cnt_n  [N_REPLICAS];
    uint8_t  clk_prev   [N_REPLICAS];   // for edge detection
};

// device:
__device__ void dut_eval_comb(DutState_Counter* s, uint32_t lane) {
    s->port_count[lane] = s->reg_cnt[lane];
}

__device__ bool rising_edge(DutState_Counter* s, uint32_t lane) {
    bool r = s->port_clk[lane] && !s->clk_prev[lane];
    s->clk_prev[lane] = s->port_clk[lane];
    return r;
}

__device__ void dut_eval_seq(DutState_Counter* s, uint32_t lane) {
    if (s->port_rst[lane]) {
        s->reg_cnt_n[lane] = 0;
    } else {
        s->reg_cnt_n[lane] = (uint16_t)(s->reg_cnt[lane] + 1);
    }
}

__device__ void dut_commit_shadow(DutState_Counter* s, uint32_t lane) {
    s->reg_cnt[lane] = s->reg_cnt_n[lane];
}
```

Reads like the C++ sim emitter today, with `[lane]` indexing
everywhere. That's the v1 codegen story in one example.

Phase 5 (replica-parallel) just sets `N_REPLICAS = 1024` and
launches `eval_Counter<<<32, 32>>>(...)`. No further changes to
the device code.

## Open questions

1. **Direct CUDA emission vs SV roundtrip.** Earlier sketch
   proposed `arch build → SV → GEM synth → bitstream`. This doc
   argues for direct CUDA emission to preserve debug labels and
   bounds-check metadata. The cost: we don't reuse GEM's mature
   AIG mapper in v1. Phase 8 revisits when intra-design
   partitioning becomes necessary.

2. **K (batch size) tuning.** Static per-design or runtime-tuned?
   Per-design via a sidecar config file is simpler; runtime-tuned
   based on trigger-fire frequency is more adaptive but adds
   complexity. v1 ships static; revisit if measurement shows
   poor utilization at fixed K.

3. **Multi-clock domains.** Per-clock edge detection per lane is
   straightforward; cross-domain CDC via gray-code FIFOs is more
   delicate. The existing `fifo` construct's dual-clock handling
   ports naturally if the gray-code generation stays on device.
   Phase 6 or later.

4. **`--debug+fsm` on GPU.** Per-lane FSM transition tracing adds
   ringbuffer pressure. Probably opt-in via
   `--debug+fsm --sampled` or similar; full-trace mode kept for
   small-replica debugging.

5. **Integration with `harc`'s constraint IR (Phase 4 in flight).**
   The GPU backend's `RandomizeNeeded` exit-reason path expects
   pre-solved Z3 tuples in a stimulus buffer. That depends on
   harc's `harc_solve_queued` runtime landing (their Phase 5).
   No hard dependency; v1 GPU backend can stub `RandomizeNeeded`
   to always exit-to-host and let the existing v0 inline Z3 path
   solve. Wire the queue once harc's runtime is in.

6. **Failure mode: kernel too long for driver timeout.** Linux
   nvidia driver kills kernels that run >60s by default. K too
   large + long-running TB → kernel TDR. Mitigations: split
   batches at TDR-safe boundaries (~5s of wall time), or use
   the driver's persistence mode. Production knob to expose, not
   v1 blocker.

## Decision log

- 2026-05-20: Decided to do TB-IR refactor on the harc-com side
  *before* TB-on-device work, since divergence-debt between two
  TB backends would be permanent otherwise. GPU backend can land
  the DUT side (phases 1–3) without TB-on-device; phase 4 picks
  up when harc's IR is ready.
- 2026-05-20: Decided to emit CUDA directly from arch IR rather
  than route through `arch build → SV → GEM synth`. Reasons:
  preserve debug labels for hierarchical port traces; preserve
  bounds-check SVA metadata as device-side error flags; avoid
  GEM's per-design mapping cost for small designs that don't need
  it. Phase 8 revisits.
- 2026-05-20: Decided to ban atomics/reductions in the eval
  kernel via a CI lint pass, giving us a stronger determinism
  contract than the realistic "per-device + per-driver" floor.
  Bit-identical results across hardware revisions conditional on
  the lint pass passing.
- 2026-05-20: Picked **inter-replica** as the primary parallelism
  axis over intra-design. Reasoning: arch's typical DUTs are
  small (≤100k gates), not enough to saturate a GPU intra-design;
  replicas are how we actually use the silicon. Phase 8 deferred
  intra-design until measurement shows the need.
