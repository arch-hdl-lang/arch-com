# L1D Cache Case Study — Summary Report

> ARCH HDL compiler validation: production-quality cache implementation

---

## Overview

A production-quality **8-way set-associative write-back/write-allocate L1 data cache** built entirely in ARCH HDL. CVA6-compatible CPU interface, AXI4 memory interface, 64 sets x 8 ways x 64B lines = **32 KiB**.

---

## Code Statistics

| Metric | Value |
|--------|-------|
| ARCH source | **1,143 lines** across 12 `.arch` files |
| Generated SV | **1,217 lines** (ARCH ~6% more concise) |
| C++ testbenches | **1,321 lines** across 9 testbenches |
| All tests | **PASS** (completed 2026-03-29) |

---

## Architecture

```
L1DCache (top module)
├── FsmCacheCtrl     (9-state controller: Idle→Lookup→Hit/Miss→Refill→Writeback)
├── FsmAxi4Fill      (4-state AXI4 read burst FSM)
├── FsmAxi4Wb        (4-state AXI4 write burst FSM)
├── ModuleLruUpdate  (8-way pseudo-LRU tree, combinational)
├── RamTagArray ×8   (64×54b per way: tag[53:2]|dirty[1]|valid[0])
├── RamDataArray     (4096×64b, indexed by {set,way,word})
└── RamLruState      (64×7b pseudo-LRU tree state)
```

---

## ARCH Constructs Exercised

| Construct | Count | Examples |
|-----------|-------|---------|
| `module` | 2 | L1DCache (top), ModuleLruUpdate |
| `fsm` | 3 | CacheCtrl (9 states), Fill (4), Wb (4) |
| `ram` | 3 | Tag, Data, LRU — all `simple_dual`, `latency 1` |
| `bus` | 2 | BusDcpu (CPU), BusAxi4 (memory) |
| `generate for` | 1 | 8-way tag array instantiation |
| `package` | 1 | PkgL1d (enums, types) |

---

## Key Design Decisions

- **Parallel tag hit**: All 8 ways compared simultaneously, one-hot to binary in 3 OR levels (10 total logic levels, optimized from 14)
- **FSM-to-FSM handshaking**: Controller orchestrates Fill and Wb FSMs via start/done pulse signals
- **Variable-indexed Vec**: `tag_rd_data[lru_victim_way][53:2]` — no manual mux trees
- **Tag encoding**: 54-bit packed {tag, dirty, valid} stored in UInt
- **Internal `_w` suffix**: Avoids name conflicts between internal wires and top-level output ports

---

## Performance

| Path | Latency |
|------|---------|
| Load hit | 3 cycles |
| Load miss (clean eviction) | ~15 cycles |
| Dirty eviction + refill | ~25 cycles |

---

## Test Coverage

| Testbench | Tests | Scope |
|-----------|-------|-------|
| `tb_fsm_cache_ctrl.cpp` | 6 | Cold misses, hits, store hits/misses, evictions |
| `tb_l1dcache.cpp` | 7 | Full integration with AXI4 memory model |
| `tb_module_lru_update.cpp` | exhaustive | All 128 tree states x 8 ways |
| `tb_fsm_axi4_fill.cpp` | 2 | AXI protocol, back-to-back fills |
| `tb_fsm_axi4_wb.cpp` | 2 | AXI write protocol, stall handling |
| `tb_ram_*.cpp` | 1 each | Write/read, multi-way isolation |

The AXI4 memory model uses `std::map<uint64_t, uint64_t>` for sparse memory simulation.

---

## What It Demonstrates

1. **Modular FSM composition** — 3 independent FSMs with clear responsibilities
2. **First-class `ram` construct** — SRAMs declared in 23 lines each, latency modeled automatically
3. **`bus` abstraction** — AXI4 and CPU interfaces as reusable port bundles with initiator/target flipping
4. **`generate for`** — 8-way tag array instantiation without code duplication
5. **Optimization possible in ARCH** — parallel comparisons, packed bit-field manipulation, variable Vec indexing

---

## Source Files

### ARCH Source (`tests/l1d/`)

| File | Lines | Purpose |
|------|-------|---------|
| `FsmCacheCtrl.arch` | 380 | Main cache controller FSM |
| `L1DCache.arch` | 319 | Top-level module integrating all subcomponents |
| `FsmAxi4Wb.arch` | 107 | AXI4 write (writeback) burst FSM |
| `FsmAxi4Fill.arch` | 91 | AXI4 read (fill) burst FSM |
| `ModuleLruUpdate.arch` | 60 | 8-way pseudo-LRU tree update (combinational) |
| `PkgL1d.arch` | 49 | Type definitions and state enums |
| `bus_axi4.arch` | 47 | AXI4 parameterized bus definition |
| `RamDataArray.arch` | 24 | Data SRAM (4096 x 64 bits) |
| `RamTagArray.arch` | 23 | Tag SRAM per way (64 x 54 bits, 8 instances) |
| `RamLruState.arch` | 23 | LRU tree state SRAM (64 x 7 bits) |
| `bus_dcpu.arch` | 20 | CVA6 CPU-to-cache bus definition |

### C++ Testbenches (`tests/l1d/`)

| File | Lines | Scope |
|------|-------|-------|
| `tb_l1dcache.cpp` | 306 | Integration: full cache with AXI memory model |
| `tb_fsm_cache_ctrl.cpp` | 286 | Controller FSM unit tests |
| `tb_fsm_axi4_wb.cpp` | 126 | Writeback FSM unit tests |
| `tb_l1d_debug.cpp` | 125 | Debug/instrumentation |
| `tb_module_lru_update.cpp` | 123 | LRU exhaustive testing |
| `tb_fsm_axi4_fill.cpp` | 122 | Fill FSM unit tests |
| `tb_ram_data_array.cpp` | 84 | Data array unit tests |
| `tb_ram_tag_array.cpp` | 82 | Tag array unit tests |
| `tb_ram_lru_state.cpp` | 67 | LRU SRAM unit tests |

---

## Git History

| Commit | Description |
|--------|-------------|
| `84f1b09` | L1D tag hit optimization (14 to 10 logic levels) |
| `852fd79` | Compiler fix: Bool width inference for concat/bitwise ops |
| `34fff85` | Language feature: `let x = expr` assign-to-existing-port syntax |

---

## Status

**Complete.** All 7 integration tasks done, all unit and integration tests passing. Not yet parameterized (geometry hardcoded) — could be generalized with ARCH templates as future work.
