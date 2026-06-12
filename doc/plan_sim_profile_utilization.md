# `arch sim --profile` — Cycle-Accurate Construct Utilization Profiling

## Problem

As ARCH designs grow in complexity (NIC-400 interconnect, FPT26 attention tile, E203
RISC-V), a new class of question arises that the current toolset cannot answer:

> "My simulation is functionally correct — but *why is it slow?* Which FIFO is stalling
> the pipeline? Which thread is spending most of its time waiting? Which arbiter lane
> is starving?"

Today there are two tools in this space:

- **`--debug`** emits a text log of every port-value change and FSM transition. It is
  invaluable for single-cycle correctness bugs, but produces megabytes of noise for
  any non-trivial design, and has no aggregation. Finding "FIFO X was 80% full on
  average" requires manually post-processing thousands of lines.

- **`--coverage`** (6 categories: branch, block, FSM state/transition, toggle,
  construct port toggle) answers "was each code path exercised?" — verification
  completeness, not design efficiency.

Neither tool surfaces aggregate, cycle-counted utilization data. Engineers currently
work around this by inserting `printf` or `$display` counters into testbenches — the
exact pattern CLAUDE.md discourages, and one that requires test-specific knowledge of
what to instrument.

The gap is completely absent from COMPILER_STATUS.md and has no open issue. It is a
genuine blind spot.

---

## Proposed Enhancement

Add `--profile <file.json>` to `arch sim`. At the end of a simulation run, serialize
a structured JSON document capturing per-construct utilization statistics accumulated
over the entire simulation.

### Example invocation

```bash
arch sim --profile profile.json NicTop.arch --tb nic_tb.cpp
arch advise --profile profile.json NicTop.arch     # optional AI interpretation step
```

### Output schema (illustrative)

```json
{
  "sim_cycles": 10000,
  "constructs": [
    {
      "kind": "fifo",
      "name": "TxFifo",
      "depth": 16,
      "push_cycles": 4200,
      "pop_cycles": 3800,
      "stall_full_cycles": 310,
      "stall_empty_cycles": 95,
      "occupancy_histogram": [0, 12, 45, 210, 830, 1400, 2100, 1950, 1800, 1200, 780, 450, 220, 100, 42, 11],
      "avg_occupancy": 7.3,
      "max_occupancy": 15
    },
    {
      "kind": "pipeline",
      "name": "DecodePipe",
      "stages": 4,
      "total_items": 8901,
      "stall_cycles_by_stage": [0, 240, 185, 310],
      "flush_cycles": 42,
      "bubble_rate": 0.12
    },
    {
      "kind": "thread",
      "name": "IssueThread",
      "states": 6,
      "cycles_per_state": [1200, 3400, 800, 2100, 1500, 1000],
      "wait_cycles_total": 4200,
      "fork_join_instances": 120,
      "avg_fork_latency_cycles": 4.1
    },
    {
      "kind": "arbiter",
      "name": "BusArb",
      "policy": "round_robin",
      "requesters": 4,
      "grants_per_requester": [2310, 2295, 2280, 2320],
      "contention_cycles": 890,
      "starvation_high_watermark": 3
    },
    {
      "kind": "ram",
      "name": "DataCache",
      "latency": 1,
      "read_ops": 5200,
      "write_ops": 2100,
      "read_conflict_cycles": 45,
      "port_utilization": 0.73
    }
  ]
}
```

---

## Why It Matters

1. **Replaces manual instrumentation.** The `--debug` flag is opt-in per-design; profiling
   would be zero-intrusion like `--coverage`. No testbench changes needed.

2. **Natural fit with the sim codegen architecture.** Each construct already has a
   C++ model in `src/sim_codegen/`. Adding `uint64_t` counter fields to those models
   and incrementing them on the hot paths (push, pop, stall, state transition) is
   additive and isolated per file.

3. **Enables the AI analysis layer.** `arch advise` already exists and queries the
   `~/.arch/learn/` error→fix knowledge base. Profile data is a natural second input:
   "FIFO `TxFifo` was full-stall 31% of the time — consider deepening to 32 or
   adding a rate-limiter upstream."

4. **Zero performance overhead when not enabled.** Counters are only injected into
   the generated C++ when `--profile` is passed. When absent, the codegen path is
   unchanged.

5. **Directly supports NIC-400 and similar interconnect workloads.** The NIC-400
   build-out (PRs #558–566) added AXI shims, AHB bridges, and CDC crossings — a
   topology where FIFO stall rates and arbiter fairness are the primary performance
   knobs.

---

## Implementation Approach

### Phase 1 — Per-construct counter injection in sim codegen

For each construct kind, add a `ProfileData` struct with counters. Emit them when
`--profile` is passed in `SimOptions`. All changes are confined to
`src/sim_codegen/`:

| File | Change |
|------|--------|
| `mod.rs` | Add `profile: bool` to `SimOptions`; emit `ArchProfile` C++ struct initialization and `write_profile(path)` call at end of `main()` |
| `fifo.rs` | Add `push_count`, `pop_count`, `stall_full_cycles`, `stall_empty_cycles`, occupancy histogram array (size = depth+1); increment in `push()`/`pop()`/`tick()` |
| `pipeline.rs` | Per-stage `stall_cycles[]`, `flush_cycles`, `items_passed` counters; increment in `advance_stage()` |
| `thread_sim.rs` | Per-state `cycles_in_state[]`, `wait_cycles`, `fork_join_count`, fork latency accumulator; increment in the coroutine scheduler tick |
| `arbiter.rs` | Per-requester `grant_count[]`, `contention_cycles`, `starvation_hwm`; increment in grant logic |
| `ram.rs` | `read_ops`, `write_ops`, `conflict_cycles`, `busy_cycles`; increment in port arbitration |

The `ArchProfile` C++ struct is a header-only helper emitted alongside the design's
generated `.cpp`. At simulation end:

```cpp
if (profile_enabled) {
    auto j = arch_profile_to_json(profile);
    std::ofstream(profile_path) << j;
}
```

### Phase 2 — `arch advise --profile` interpretation

Extend `arch advise` (currently `src/learn.rs`) to accept `--profile <file>`. Read
the JSON, apply heuristic thresholds (e.g., `stall_full_cycles / sim_cycles > 0.20`
→ warn), and emit actionable suggestions in the same human-readable format used for
error explanations.

### Phase 3 — Waveform annotation (optional)

When both `--profile` and `--wave` are passed, annotate the VCD/FST with
per-construct utilization time-series (one value-change per cycle per metric). This
allows GTKWave/Surfer to display FIFO occupancy as a waveform alongside port values —
closing the loop between functional waveform debugging and performance analysis.

---

## Scope and Novelty Check

- **Not in COMPILER_STATUS.md** — no `--profile` flag, no utilization counters listed
  anywhere in the implemented surface.
- **Not in any open issue** — issues #244, #379, #500, #501, #557 are unrelated.
- **Distinct from `--coverage`** — coverage asks "was this path hit?"; profiling asks
  "how many times, and at what cost?"
- **Distinct from `--debug`** — debug is per-event verbose text; profiling is per-run
  aggregated statistics.

---

## Rationale for Priority

The existing pain points in issues #383, #501, and #557 are correctness gaps that
block specific users. Profiling is a DX gap that affects *all* users of non-trivial
designs and becomes more acute as design complexity grows. Given the trajectory of
the project (interconnects, attention tiles, full SoCs), this gap will only widen.

Phase 1 is self-contained, confined to `src/sim_codegen/`, and produces a reviewable
artifact (the JSON file) that is easy to validate end-to-end in a single integration
test.
