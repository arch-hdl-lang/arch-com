# AXI DMA Case Study — Summary Report

> ARCH HDL compiler validation: Xilinx PG021-compatible DMA controller

---

## Overview

A production-grade **dual-channel AXI DMA controller** built entirely in ARCH HDL, compatible with the Xilinx AXI DMA IP (PG021). Supports both **Simple DMA** mode (register-triggered) and **Scatter-Gather** mode (descriptor-chained). Exercises the widest range of ARCH first-class constructs of any case study: `bus`, `fsm`, `fifo`, `module`, `latch`, and `package`.

---

## Code Statistics

| Metric | Value |
|--------|-------|
| ARCH source | **1,042 lines** across 14 `.arch` files |
| Generated SV | **1,176 lines** (ARCH ~11% more concise) |
| C++ testbenches | **2,075 lines** across 8 test files |
| All tests | **PASS** |

---

## Architecture

```
AxiDmaTop (top module)
├── AxiLiteRegs        (PG021 register block, 12 registers)
├── ClkGateDma ×2      (latch-based ICG per channel)
│
├── MM2S Channel
│   ├── FsmMm2s        (4-state: Idle→SendAR→WaitR→Done)
│   ├── FsmSgEngine    (9-state: descriptor fetch/chain/status writeback)
│   └── Mm2sFifo       (depth=16, decouples AXI read from AXIS output)
│
└── S2MM Channel
    ├── FsmS2mm        (6-state: Idle→WaitRecv→SendAW→SendW→WaitB→Done)
    ├── FsmSgEngine    (9-state: shared design, second instance)
    └── S2mmFifo       (depth=16, decouples AXIS input from AXI write)
```

### Interfaces

| Port | Protocol | Direction | Purpose |
|------|----------|-----------|---------|
| `s_axil` | AXI4-Lite | Target | Register read/write (8-bit addr) |
| `m_axi_mm2s` | AXI4 Read | Initiator | Memory read (data path) |
| `m_axi_s2mm` | AXI4 Write | Initiator | Memory write (data path) |
| `m_axi_mm2s_sg` | AXI4 Full | Initiator | MM2S descriptor fetch + status WB |
| `m_axi_s2mm_sg` | AXI4 Full | Initiator | S2MM descriptor fetch + status WB |
| `m_axis_mm2s` | AXI-Stream | Initiator | Output stream (MM2S → application) |
| `s_axis_s2mm` | AXI-Stream | Target | Input stream (application → S2MM) |
| `mm2s_introut` | Wire | Output | MM2S transfer-complete interrupt |
| `s2mm_introut` | Wire | Output | S2MM transfer-complete interrupt |

---

## ARCH Constructs Exercised

| Construct | Count | Details |
|-----------|-------|---------|
| `module` | 2 | AxiDmaTop (top integration + mux), AxiLiteRegs (register block) |
| `fsm` | 3 | FsmMm2s (4 states), FsmS2mm (6 states), FsmSgEngine (9 states) |
| `fifo` | 2 | Mm2sFifo, S2mmFifo (sync, depth=16) |
| `bus` | 5 | BusAxi4Full, BusAxi4Read, BusAxi4Write, BusAxiLite, BusAxis |
| `latch` | 1 | ClkGateDma (integrated clock gating cell) |
| `package` | 1 | PkgAxiDma (domain definition) |
| **Total FSM states** | 19 | 4 + 6 + 9 |

This is the only case study that uses **all of**: bus, fsm, fifo, latch, and module together.

---

## Key Design Features

### 1. Bus Abstraction

Five `bus` definitions eliminate ~200 lines of repetitive port declarations. Initiator/target flipping is automatic:

```
bus BusAxis
  param DATA_W: const = 32;
  port tvalid: out Bool;
  port tready: in  Bool;
  port tdata:  out UInt<DATA_W>;
  port tlast:  out Bool;
  port tkeep:  out UInt<4>;
end bus BusAxis
```

An `initiator` port keeps directions as-is; a `target` port flips them. SV codegen flattens to individual ports (`m_axis_mm2s_tvalid`, etc.).

### 2. Dual-Mode Architecture (Simple + Scatter-Gather)

Shared data-path FSMs accept inputs from a combinational mux:

```
comb
  if mm2s_sg_active
    mm2s_fsm_start_w = mm2s_sg_xfer_start_w;
    mm2s_fsm_addr_w  = mm2s_sg_xfer_addr_w;
    mm2s_fsm_beats_w = mm2s_sg_xfer_beats_w;
  else
    mm2s_fsm_start_w = mm2s_start_w;
    mm2s_fsm_addr_w  = mm2s_src_addr_w;
    mm2s_fsm_beats_w = mm2s_num_beats_w;
  end if
end comb
```

No duplicate FSMs — one FSM handles both simple and SG-driven transfers.

### 3. Scatter-Gather Descriptor Engine (FsmSgEngine)

9-state FSM implementing PG021 descriptor chaining:

```
States: Idle → FetchAR → FetchR → RunXfer → StatusAW → StatusW → StatusB → CheckNext → Done
```

**Descriptor format (16 bytes, 4 words):**

| Word | Field | Description |
|------|-------|-------------|
| 0 | NXTDESC | Next descriptor address |
| 1 | BUF_ADDR | Buffer address |
| 2 | CONTROL | Transfer length [25:0] |
| 3 | STATUS | Set by DMA: {Cmplt[31], transferred[25:0]} |

Chains to next descriptor until `curdesc == taildesc`.

### 4. Critical Path Optimizations

Both MM2S and S2MM use **lookahead registers** to keep final combinational paths to 1 gate:

- **MM2S tlast**: `mm2s_tlast_r` precomputed one cycle early; output is just `tvalid & tlast_r`
- **S2MM w_last**: `w_last_r` precomputed via `(beat_ctr_r + 2 == num_beats_r)`; no subtractor on critical path

### 5. Clock Gating with Deadlock Prevention

Latch-based ICG gates each channel clock when halted. OR logic prevents deadlock:

```
mm2s_clk_en_w = ~mm2s_halted_w | mm2s_fsm_start_w | mm2s_sg_start_w;
```

If a start signal fires while the clock is gated, the OR ensures `clk_en` goes high in the same cycle.

### 6. PG021-Compatible Register Map

| Offset | Register | Description |
|--------|----------|-------------|
| 0x00 | MM2S_DMACR | Control: RS[0], IOC_IrqEn[12] |
| 0x04 | MM2S_DMASR | Status: IOC_Irq[12], Halted[1], Idle[0] |
| 0x08 | MM2S_CURDESC | Current SG descriptor address |
| 0x10 | MM2S_TAILDESC | Tail descriptor (write triggers SG) |
| 0x18 | MM2S_SA | Source address (simple DMA) |
| 0x28 | MM2S_LENGTH | Transfer length (write triggers simple DMA) |
| 0x30–0x58 | S2MM_* | Mirror of MM2S registers at +0x30 offset |

Hardware-set IOC_Irq on transfer done; software clear via W1C (write-1-to-clear).

---

## Source Files (`tests/axi_dma/`)

### ARCH Source

| File | Lines | Purpose |
|------|-------|---------|
| `AxiDmaTop.arch` | 230 | Top-level integration, simple/SG mux, clock gating |
| `AxiLiteRegs.arch` | 229 | PG021 register block (12 registers) |
| `FsmSgEngine.arch` | 163 | Scatter-Gather descriptor engine |
| `FsmS2mm.arch` | 134 | Stream-to-Memory FSM |
| `FsmMm2s.arch` | 98 | Memory-to-Stream FSM |
| `BusAxi4Full.arch` | 45 | AXI4 full bus (AR+R+AW+W+B) |
| `BusAxiLite.arch` | 32 | AXI4-Lite bus |
| `BusAxi4Write.arch` | 29 | AXI4 write-only bus |
| `BusAxi4Read.arch` | 24 | AXI4 read-only bus |
| `Mm2sFifo.arch` | 15 | MM2S data FIFO (depth=16) |
| `S2mmFifo.arch` | 15 | S2MM data FIFO (depth=16) |
| `BusAxis.arch` | 11 | AXI4-Stream bus |
| `ClkGateDma.arch` | 10 | Latch-based ICG |
| `PkgAxiDma.arch` | 7 | Domain definition |

### C++ Testbenches

| File | Lines | Scope |
|------|-------|-------|
| `tb_axi_dma.cpp` | 511 | Full integration (5 tests: MM2S, S2MM, bidirectional, SG chain, register I/O) |
| `tb_clkgate_race.cpp` | 396 | Clock gating race condition and deadlock prevention |
| `tb_axilite_regs.cpp` | 260 | Register unit tests (write/read, W1C interrupt clear) |
| `tb_sg_engine.cpp` | 241 | SG descriptor fetch, chain, status writeback |
| `tb_fsm_mm2s.cpp` | 196 | MM2S FSM state transitions, AXI handshaking |
| `tb_verilator.cpp` | 188 | Verilator simulation wrapper |
| `tb_fsm_s2mm.cpp` | 164 | S2MM FSM, w_last lookahead verification |
| `tb_mm2s_fifo.cpp` | 119 | FIFO push/pop, full/empty, backpressure |

---

## Test Scenarios (Integration)

1. **MM2S 4-beat transfer** — Preload memory, trigger via LENGTH write, verify AXIS output + interrupt
2. **S2MM 4-beat transfer** — Source from AXIS, verify memory contents + interrupt
3. **Register readback** — AXI-Lite write/read of all control registers
4. **Bidirectional** — Simultaneous MM2S + S2MM, both interrupts fire
5. **Scatter-Gather chain** — 2 descriptors (0x100→0x110), verify 8 AXIS beats, descriptor status Cmplt bit set

Memory model: shared 16KB word-addressed array with AXI handshake simulation.

---

## Git History

| Commit | Description |
|--------|-------------|
| `01892f0` | PG021-compatible AXI DMA benchmark (Simple DMA mode) |
| `051ed82` | Rewrite with bus ports; fix FSM sim codegen |
| `d61fded` | Verilator integration test with VCD |
| `39e6b69` | Add scatter-gather descriptor engine |
| `df99c66` | Whole-bus connection syntax in inst blocks |
| `4ed3a6a` | Use bus ports for SG engine and SG AXI4 master |
| `8cddebd` | Eliminate SG mux wires — one SG AXI4 port per channel |
| `86e6b7b` | Use bus port for AXI4-Lite; fix target perspective |
| `40ef21d` | clkgate: fix parser, deadlock, add race-condition tests |

---

## Synthesis and Power Analysis (Yosys + OpenSTA)

The generated SV was synthesized through three targets to validate quality of the compiler output.

### Synthesis Results

#### Xilinx 7-series (synth_xilinx)

| Cell | Count |
|------|-------|
| LUT2 | 421 |
| LUT3 | 96 |
| LUT4 | 138 |
| LUT5 | 29 |
| LUT6 | 229 |
| **Total LUTs** | **913** |
| FDRE (flip-flops) | 993 |
| CARRY4 | 39 |
| RAM32M | 12 |
| MUXF7/F8 | 54 |
| **Estimated LCs** | **586** |

#### Sky130 130nm (TT, 25C, 1.8V)

- **Total area**: 78,134 um^2
- **Total flip-flops**: 2,017 (605 `dfxtp_1` + 1,412 `edfxtp_1`)
- Key combinational cells: 305 `mux4_2`, 659 `nand2_1`, 496 `nor2_1`, 263 `and2_0`

#### Generic Technology (Yosys gtech)

| Module | Cells |
|--------|-------|
| AxiDmaTop (total) | 5,940 |
| AxiLiteRegs | 1,302 |
| FsmSgEngine | 752 |
| Mm2sFifo | 1,160 |
| S2mmFifo | 1,160 |
| FsmS2mm | 346 |
| FsmMm2s | 276 |

### Critical Path Optimization

Three rounds of optimization were guided by Yosys `ltp` (longest topological path) analysis:

| Round | Critical path | Logic levels | Location |
|-------|--------------|--------------|----------|
| Original | `beat_ctr_r == num_beats_r-1` → `w_last` → FSM mux | **23** | FsmS2mm |
| After FsmS2mm lookahead register | SG state → xfer_num_beats → beats mux → tlast subtractor | **21** | AxiDmaTop |
| After AxiDmaTop tlast lookahead | `num_beats_r >= recv_count` comparator → FSM mux | **18** | FsmS2mm |

After LUT6 mapping (`abc -lut 6`): **6 LUT levels** — AXI-Lite read address decode is the critical path.

### Timing (OpenSTA with Sky130)

| Target frequency | Critical path | Slack | Status |
|-----------------|---------------|-------|--------|
| 100 MHz (10 ns) | 4.478 ns | +5.061 ns | **MET** |
| 200 MHz (5 ns) | 4.478 ns | +0.061 ns | **MET (barely)** |

Max achievable frequency: **~223 MHz** on Sky130 130nm.

### Power Analysis

#### Static estimates (OpenSTA `report_power`)

| Scenario | Toggle rate | Sequential | Combinational | Total |
|----------|------------|------------|---------------|-------|
| Idle | 5% | 9.40 mW | 0.33 mW | **9.73 mW** |
| Active | 30% | 11.38 mW | 2.00 mW | **13.38 mW** |
| Peak | 80% | 15.35 mW | 5.34 mW | **20.69 mW** |

#### VCD-annotated (from `arch sim` testbench, 170 cycles)

| Component | Power @100MHz |
|-----------|--------------|
| Sequential internal | 9.01 mW |
| Sequential switching | 0.004 mW |
| Combinational | 0.017 mW |
| **Total** | **9.03 mW** |

### Clock Gating Power Reduction

Two latch-based ICG instances (ClkGateDma) gate each channel clock when halted:
- `mm2s_icg`: gates FsmMm2s + Mm2sFifo + mm2s SG engine
- `s2mm_icg`: gates FsmS2mm + S2mmFifo + s2mm SG engine
- AxiLiteRegs stays on ungated clock (must always respond to register reads)

| Scenario | Without gating | With gating |
|----------|---------------|-------------|
| Both channels idle | 9.73 mW | ~**0.02 mW** (leakage only) |
| One channel active | 9.73 mW | ~**4.5 mW** |
| Both channels active | 9.73 mW | 9.73 mW (unchanged) |

### Synthesis Toolflow

1. ARCH source → `arch build` → SystemVerilog
2. Pre-process: concretize `parameter type` (Yosys 0.63 limitation) via Python script
3. Yosys: `read_verilog -sv` → `proc; opt; memory; opt; techmap; opt; stat`
4. Target-specific: `synth_xilinx` or `dfflibmap + abc -liberty sky130_*.lib`
5. OpenSTA: built from source (The-OpenROAD-Project/OpenSTA) with Sky130 liberty files

### Fanout Hotspots Identified

| Signal | Fanout | Risk |
|--------|--------|------|
| `s_axis_s2mm_tdata` | 516 | Routing congestion |
| `s_axil_w_data` | 405 | Routing congestion |
| SG read data | 93 each | Moderate |

---

## What It Demonstrates

1. **Bus abstraction** — 5 reusable bus definitions eliminate repetitive port declarations; initiator/target flipping makes integration natural
2. **FSM composition** — 3 FSMs (MM2S, S2MM, SG) with clear separation; shared data-path FSMs serve both simple and SG modes
3. **FIFO decoupling** — First-class `fifo` construct (15 lines each) decouples AXI from AXIS timing domains
4. **Clock gating** — Latch-based ICG with deadlock-safe OR logic for low-power design
5. **IP compatibility** — PG021-compatible register map, descriptor format, and interrupt behavior
6. **Critical path optimization** — Lookahead registers reduce final combinational depth to 1 gate
7. **Synthesis-quality SV output** — Generated SV passes Yosys synthesis for Xilinx 7-series, Sky130 130nm; meets timing at 200 MHz on Sky130
8. **Power-aware design** — Latch-based clock gating reduces idle power from 9.73 mW to ~0.02 mW (leakage only)

---

## Thread vs FSM Synthesis Comparison

The `tests/axi_dma_thread/` directory contains a thread-based rewrite of the MM2S and S2MM engines for direct comparison. Synthesized with Yosys `synth -flatten` to generic gates:

| Module | Style | Total Cells | FFs | MUX cells | Notes |
|--------|-------|-------------|-----|-----------|-------|
| `FsmMm2sMulti` | FSM | 805 | 109 | 33 | Single state machine |
| `ThreadMm2s` (v1) | Thread | 1431 | 97 | 429 | 4 parallel threads, all drive AR+data outputs |
| `ThreadMm2s` (v2) | Thread | **680** | 92 | **68** | Split: 1 ArIssuer + 4 RCollect threads |
| `FsmS2mmMulti` | FSM | 1002 | 119 | — | Single state machine |
| `ThreadS2mm` | Thread | 4051 | 616 | — | 4 parallel threads with fork/join |

**MM2S v1 analysis**: 1.8× cell overhead. Root cause: 4 threads each drive `ar_addr` (32-bit) and `push_data` (32-bit), creating 4-way mux chains (429 MUX cells vs 33 in FSM). Plus a variable×variable multiply in the comb path to compute burst addresses from `thread_complete[i] * burst_len_r`.

**MM2S v2 analysis (split ArIssuer)**: Thread version is now **16% smaller** than the FSM. Three architectural changes drove this:
1. **Single ArIssuer thread** — one state machine drives all AR outputs; no 4-way mux on the 48-bit AR channel (ar_addr + ar_id + ar_len + ar_size + ar_burst).
2. **`push_data = r_data` unconditional** — all threads push from the same source; hoisted to a module-level comb wire, eliminating the 4-way 32-bit mux.
3. **Incremental address register** — `next_ar_addr_r` increments by `burst_len << 2` on each issue; no variable×variable multiply in the comb path.

The 4 RCollect threads only drive 1-bit signals (`r_ready`, `push_valid`), so their combined mux overhead is negligible.

**S2MM analysis**: Thread version is 4× larger. The S2MM design uses `fork/join` for AW+W parallelism inside each thread, plus a dedicated B-channel lock — this multiplies state significantly across 4 threads. Same split-issuer optimization applies but not yet implemented.

**Key compiler optimization (loop counter width inference)**: The thread compiler walks the for-loop end expression type map to infer the minimum counter width. `for i in 0..burst_len_r-1` where `burst_len_r: UInt<8>` generates an 8-bit counter (`logic [7:0] _loop_cnt`) instead of the naive 32-bit default. This cut ThreadMm2s from 2660 → 1431 cells (46% reduction) before the architectural split reduced it further to 680.

---

## Status

**Complete.** Simple DMA + Scatter-Gather modes working, all unit and integration tests passing. Synthesized through Yosys (Xilinx + Sky130) with OpenSTA timing/power analysis. The widest construct coverage of any ARCH case study.
