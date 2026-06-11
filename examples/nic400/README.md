# NIC-400 Demo

An ARM AMBA NIC-400-class interconnect rebuilt in ARCH. Started as a TDD
exercise for the four core mechanisms (address decode, per-slave
arbitration, ID remap, ID-prefix return routing) and grew into a
full-featured demo of the language's bus / thread / generate / pipeline
constructs against a real-IP target.

Reference spec: [`nic400_interconnect_spec.md`](nic400_interconnect_spec.md)

## What's shipped

| Component | Module | Coverage |
|-----------|--------|----------|
| Bus definitions | `BusAxi4`, `BusAhbLite`, `BusApb` | AXI4-full (with sideband), AHB-Lite v1.0, APB v2.0 |
| Crossbar fabric | `Nic400Fabric` (3 masters × 4 slaves, full R/W) | ID-prefix routing, per-channel `mutex<priority>` arbitration |
| Per-edge plumbing | `Nic400MasterPort`, `Nic400SlavePort` | Address decode + response demux by ID-prefix match |
| Timing closure | `Nic400FabricRs1`, `Nic400FabricRsSlave`, `Nic400EdgeRegSlice` | Per-master (ASIB) and per-slave (AMIB) register slice via wrapper module |
| AHB bridge (master attach) | `Nic400AhbBridge` (+ `IncrHwdataFifo`) | AHB master → AXI fabric. SINGLE / INCRn / WRAPn / INCR-undef up to 64 beats (multi-chunk) |
| AHB bridge (slave attach) | `Nic400AhbSlaveBridge` | AXI fabric → AHB peripheral (reverse/mirrored). v1 single-beat; axsize/axprot/axlock + HRESP→RRESP/BRESP mapping |
| AXI3 conversion | `BusAxi3`, `Nic400Axi4ToAxi3`, `Nic400Axi3ToAxi4` | AXI4→AXI3 long-burst split (≤16-beat sub-bursts, 4 KB-aware) + AXI3→AXI4 burst limiter. Read path |
| APB bridge | `Nic400ApbBridge` | AXI4 target → APB initiator, burst-split into APB phases |
| Width adapter | `Nic400WidthAdapter` | 64→32 downsize (1:RATIO beat split, w_strb forwarding) |
| Performance counters | `Nic400Pmu` | Per-master AR/AW/R/W/B handshake counters |
| Helpers | `Nic400ArbiterPolicy`, `Nic400QosFn`, `RegSliceChannel` | Module-local fns, generic single-channel skid buffer |

## How the pieces fit

```
                          Nic400Fabric (3×4)
                          ┌──────────────────────────────────────┐
   AHB-master (CPU) ──┐   │   MasterPort×3 ── edges[i][j] ──     │   ┌── APB peripheral
                      └── m[0]        │                          s[0] ─┘
       Nic400AhbBridge      │     ┌───┴───────┐                    │      Nic400ApbBridge
                            │     │ ID-prefix │
   Direct AXI master ─── m[1]     │ route +   │                    s[1] ── Direct AXI slave
                            │     │ arbitrate │
                            m[2]  └───────────┘                    s[2,3] ── Direct AXI slaves
                            └──────────────────────────────────────┘
                                       │
                  Nic400Pmu observes per-master handshake events
                                       │
                  Optional: Nic400FabricRs1 wraps Nic400Fabric with
                  one Nic400EdgeRegSlice on every m[i] for +1-cycle
                  timing closure on each master→fabric edge.

  Nic400WidthAdapter sits between a wide master (M_DATA_W=64) and the
  fabric, or between the fabric and a narrow slave (S_DATA_W=32).
```

## Module dependency graph

```
PkgNic400  BusAxi4  BusAhbLite  BusApb
   │         │          │          │
   │         ├──────────┴──────────┤
   │         ▼                     ▼
   │    Nic400MasterPort      Nic400ApbBridge
   │    Nic400SlavePort       Nic400AhbBridge ── IncrHwdataFifo
   │         │                Nic400WidthAdapter
   │         ▼
   │    Nic400Fabric ──────► RegSliceChannel
   │         │                     ▲
   │         ▼                     │
   │    Nic400FabricRs1 ── Nic400EdgeRegSlice
   │
   └─► Nic400Pmu (independent — taps handshake event lines)

Nic400ArbiterPolicy, Nic400QosFn — helper functions used by
Nic400MasterPort / Nic400SlavePort internally.

Nic400Read2x2 — older 2×2 read-only crossbar (predates Nic400Fabric).
                Kept for the four tb_nic400_read2x2_*.cpp regressions.
```

## File index

### Bus definitions

- **`BusAxi4.arch`** — AXI4-full with sideband (`lock`/`cache`/`prot`/`qos`/`region`) on AR/AW. Parameterized: `ADDR_W`, `DATA_W`, `STRB_W = DATA_W/8`, `ID_W`, `READ`, `WRITE`.
- **`BusAhbLite.arch`** — AHB-Lite v1.0 from initiator (master) perspective. `target` flips directions for the slave side. Used by `Nic400AhbBridge`.
- **APB bus** — uses `stdlib/BusApb.arch` with `USE_PPROT=1, USE_PSTRB=1` enabled (gives psel/penable/paddr/pwrite/pwdata/pstrb/pprot out + prdata/pready/pslverr in). The earlier NIC-400-local `BusApb` duplicate has been removed in favour of the stdlib def.

### Crossbar fabric

- **`Nic400Fabric.arch`** — 3-master × 4-slave full R/W crossbar. Scaled via `generate_for` over both `NUM_MASTERS` and `NUM_SLAVES`. 2D bus wire `edges[i][j]` is the (master_i ↔ slave_j) edge. ID-prefix routing: master injects its index into the high bits of AW/AR `id`; slave demux matches the prefix on response; master strips it.
- **`Nic400MasterPort.arch`** — per-master decode + route. One AR thread + one R thread + one AW→W→B thread *per slave*, contending on per-channel `mutex<priority>` resources. Address decode picks slave via `addr[REGION_BITS+NS_W-1:REGION_BITS]`.
- **`Nic400SlavePort.arch`** — per-slave arbitration + return demux. One AR arbiter + one R demux + one AW→W→B thread *per master*. Same-slave write-pipeline depth is 1; cross-slave concurrency works.
- **`PkgNic400.arch`** — shared package (currently minimal; reserved for cross-module constants).
- **`Nic400SparseAddrDecode.arch`** — per-master **sparse connectivity** decoder (the real NIC-400 feature — each ASIB is wired to its own subset of slaves; *not* arbitrary per-master remap, which base NIC-400 lacks). Pure-combinational: params `NUM_SLAVES` / `REGION_BITS` / `ADDR_W` / `CONNECT_MASK` (a per-instance reachable-slave bitmap); outputs `slave_idx` and `decerr`. `decerr` asserts when the decoded slave is out of the populated map (in-window hole for non-power-of-two `NUM_SLAVES`, full-width compare keeps it Verilator-clean) **or** its bit is clear in `CONNECT_MASK` (not connected to this master). So each master sees a distinct valid-region view; a non-connected access returns DECERR.
- **`Nic400SparseDemo.arch`** — instantiates two decoders over the same 4-slave map with different masks (M0 = `{0,1}` / `0x3`, M1 = `{1,2}` / `0x6`), showing distinct reachability per master. Verified by `Nic400SparseDemo_test.harc` (all region cases + a squelched `valid=0`) on both backends with matching traces.

### Timing closure

- **`RegSliceChannel.arch`** — generic single-stage register slice for ONE channel. 1 cycle latency, 1 transfer/cycle sustained. Ready path stays combinational.
- **`Nic400EdgeRegSlice.arch`** — per-edge AXI4 reg slice; wraps 5 `RegSliceChannel` instances (AR/R/AW/W/B) on packed `UInt<PAYLOAD_W>` payloads. Direction is per-channel: AR/AW/W use up→dn (master→fabric), R/B use dn→up (fabric→master).
- **`Nic400FabricRs1.arch`** — wrapper around `Nic400Fabric` with one `Nic400EdgeRegSlice` per master between external `m[i]` and `inner.m[i]`. Slave side forwarded directly via whole-Vec forwarding (`inner.s -> s`).
- **`Nic400FabricRsSlave.arch`** — the per-SLAVE mirror of `Nic400FabricRs1`: one `Nic400EdgeRegSlice` per slave between `inner.s[j]` and external `s[j]` (AMIB timing isolation), master side forwarded directly. Adds +1 cycle to the slave-edge round-trip. Each slice overrides `ID_W = SLAVE_ID_W`.

### Protocol bridges

- **`Nic400AhbBridge.arch`** — AHB-Lite → AXI4. Two threads (`ReadXact`, `WriteXact`) sharing AHB target drives via `h_resp_lock` mutex. Handles SINGLE / INCRn / WRAPn / INCR-undef. INCR-undef spawns a Producer/Consumer pair coordinating through `IncrHwdataFifo` and issues up to `MAX_INCR_CHUNKS` (default 4) AXI bursts of `MAX_INCR_BEATS` (default 16) — supports up to 64-beat INCR-undef writes.
- **`IncrHwdataFifo.arch`** — HWDATA buffer used by `Nic400AhbBridge`'s INCR-undef path. Depth = one chunk.
- **`Nic400AhbSlaveBridge.arch`** — the reverse/mirrored bridge: AXI4 `target` → AHB-Lite `initiator` (master), for attaching an external AHB peripheral on the slave side of the fabric. Two threads mirroring `Nic400AhbBridge` with bus roles flipped. v1 single-beat (HBURST=SINGLE): AXI AR→AHB read→AXI R, AXI AW+W→AHB write→AXI B; axsize→HSIZE, axprot→HPROT, axlock→HMASTLOCK, HRESP OKAY/ERROR→AXI OKAY/SLVERR.
- **`BusAxi3.arch`** — AXI3 bus bundle: 4-bit `AxLEN` (max 16-beat bursts), 2-bit `AxLOCK`, `WID` on the W channel, no QoS/REGION. Consumed by the AXI3↔AXI4 converters below.
- **`Nic400Axi4ToAxi3.arch`** — AXI4→AXI3 long-burst splitter. One thread splits an AXI4 INCR AR (len ≤255) into `ceil((ar_len+1)/16)` AXI3 sub-bursts with per-sub-burst start address `base+((i*16)<<size)` and short final length; 4 KB-boundary SVA; merges R responses back to one AXI4 stream (RLAST only on the true final beat, `r_resp` sticky-OR'd). Read path.
- **`Nic400Axi3ToAxi4.arch`** — AXI3→AXI4 converter with a GPV-programmable `MAX_BURST` limiter (caps issued onward `AxLEN`); maps 2-bit AXI3 lock → 1-bit AXI4, ties QoS/REGION to 0.
- **`Nic400ApbBridge.arch`** — AXI4 target → APB initiator. Two threads sharing APB drives via `apb_lock`. Splits AXI bursts into sequential APB Setup → Access phases. Per-beat `paddr = base + (b << size)`.
- **`Nic400WidthAdapter.arch`** — AXI4 downsize adapter (`M_DATA_W` > `S_DATA_W`). 5 per-channel threads. Master beats split into RATIO little-endian slave sub-beats; reads pack RATIO slave beats into one master beat with OR-reduced `r_resp`.

### Clock-domain crossing (async master/slave)

- **`Nic400CdcAxi4.arch`** — AXI4 read-path bridge whose master side runs on `MClkDom` (200 MHz) and slave side on `SClkDom` (150 MHz). The non-integer 4:3 frequency ratio is deliberate: the slave clock's edges stay genuinely non-aligned with the master's, so the gray-code pointer synchronisers cross unrelated edges — an exact 2:1 ratio can let edges line up and mask CDC bugs. The AR (M→S) and R (S→M) channels each cross the boundary through a dual-clock async FIFO; the compiler auto-synthesises gray-code pointer CDC because each FIFO's `wr_clk`/`rd_clk` reference different domains. Each channel's fields are packed into one FIFO word (concat on push, bit-select on pop) — `comb` wires the valid/ready handshakes, no thread/FSM needed. No combinational path crosses a FIFO (both FIFOs register `push_ready`/`pop_valid`/`pop_data` through their gray-code pointer synchronisers), so the whole-design comb-cycle detector — which is construct-aware and models a `fifo` inst as registered, not transparent — reports the design loop-free; Verilator agrees.
- **`Nic400ArCdcFifo.arch`** — AR-channel crossing FIFO. Write port `MClkDom`, read port `SClkDom`; 64-bit packed AR payload (`addr32 id3 len8 size3 burst2 lock1 cache4 prot3 qos4 region4`).
- **`Nic400RCdcFifo.arch`** — R-channel crossing FIFO (reverse direction). Write port `SClkDom`, read port `MClkDom`; 38-bit packed R payload (`data32 id3 resp2 last1`).

Verified with `Nic400CdcAxi4_test.harc` (a dual-clock HARC testbench that drives both endpoints): AR crosses M→S and R crosses S→M with payloads intact, on both the ARCH native sim and the Verilator backend, with matching cross-backend traces (`harc sim --check-backends`).

- **`Nic400CdcAxi4Rw.arch`** — the full read+write async-clock AXI4 CDC bridge. Extends `Nic400CdcAxi4` from read-only (AR/R) to all **five** AXI4 channels: AW + W cross M→S and B crosses S→M through additional dual-clock async FIFOs. The five per-channel crossing FIFOs are `Nic400GpvArCdcFifo` / `RCdcFifo` / `AwCdcFifo` / `WCdcFifo` / `BCdcFifo` (45/39/45/37/6 bits, sized to a 12-bit-addr / 32-bit-data / 4-bit-ID bus). Closes the write-path-CDC gap.
- **`Nic400GpvRing.arch`** — the **hierarchical clock-gating GPV ring** demonstrator (spec §16.1, TRM §2.3.2): a config master in domain A (`MClkDom`, 200 MHz) writes **and** reads a `Nic400Gpv` register block in domain B (`SClkDom`, 150 MHz) across the `Nic400CdcAxi4Rw` async bridge — so the GPV stays reachable across a clock-domain boundary, the essence of the multi-domain GPV ring. Reuses `Nic400Gpv`/`Nic400GpvRegs`/`BusAxi4` unmodified; exposes two typed resets (async-low for the bridge, sync-high for the GPV). Verified by `Nic400GpvRing_test.harc`: a Secure 32-bit write of `0xDEADBEEF` to `reg[2]` crosses A→B, the B response crosses back, and a read returns `0xDEADBEEF` — on both backends with matching traces (`harc sim --check-backends`).

### Observability

- **`Nic400Pmu.arch`** — performance monitor counters. Per-master AR/AW/R/W/B counters, `COUNTER_W`-wide (default 32). Inputs are pre-qualified one-cycle event pulses (typically `m[i].x_valid && m[i].x_ready`). `r`/`w` count beats; `ar`/`aw`/`b` count transactions.

### Configuration (GPV)

- **`Nic400Gpv.arch`** — the **Global Programmers View**: the interconnect's configuration register file, presented as an AXI4 *target* (slave). Per the NIC-400 TRM (§3.2), the GPV is AXI-accessed, **32-bit only** (`AxSIZE=2`), **Secure-only**, non-cacheable, aligned, single-beat. This shim is an AXI4-Lite-style target (one outstanding read, one outstanding write) backed by `Nic400GpvRegs`, enforcing the two constraints: a wrong size → **SLVERR** (`0b10`), a Non-secure access (`AxPROT[1]=1`) → **DECERR** (`0b11`, priority over SLVERR), and a rejected write leaves the bank unchanged. The lowest-cost path to programmable QoS/decode/remap state (spec §16.1 roadmap item #4). Verified by `Nic400Gpv_test.harc` on the ARCH sim **and** Verilator with matching cross-backend traces (`harc sim --check-backends`).
- **`Nic400GpvRegs.arch`** — the backing `regfile` bank: `NREGS` 32-bit config registers, one read + one write port, write-before-read forwarding.

### Low-power (clock gating)

- **`Nic400CChannelClockGate.arch`** — the AMBA Low-Power Interface **C-channel** clock-gating controller for one clock domain (TRM §2.2.3). The system clock controller drives `cc.csysreq` to request the domain quiesce (0) or run (1); this 2-state `fsm` drops `cc.csysack` (and the `clk_en` output) only once the gated logic is idle (`busy == 0`), reflects pending work on `cc.cactive`, and wakes immediately on activity (`busy`) or a run request. Runs on the always-on controller clock so the handshake advances while the downstream domain is parked. Exercises an `fsm` driving a `target` bus + a fresh handshake bundle; verified by `Nic400CChannelClockGate_test.harc` (gate / wake-on-activity / wake-on-request) on the ARCH sim **and** Verilator with matching cross-backend traces (`harc sim --check-backends`).
- **`BusCChannel.arch`** — the C-channel handshake bundle (`csysreq` / `csysack` / `cactive`), initiator = system clock controller.

### Helpers

- **`Nic400ArbiterPolicy.arch`** — module-local `qos_grant_select(req_mask, last_grant, qos_vec)` for QoS-weighted grant selection.
- **`Nic400QosFn.arch`** — module-local functions decoding QoS / region bits per channel.

### Older standalone crossbar

- **`Nic400Read2x2.arch`** — 2-master × 2-slave read-only crossbar. Predates the hierarchical `Nic400Fabric`. Kept for the four `tb_nic400_read2x2_*.cpp` regressions it carries (smoke, hot-slave, OOO, parallel).

## Testbenches

| TB | Tests |
|----|-------|
| `tb_nic400_fabric_smoke.cpp` | Hierarchical 2×2 decode + ID remap + return route |
| `tb_nic400_fabric_latency.cpp` | AR/R latency cycles + throughput (1.00 transfers/cycle) |
| `tb_nic400_fabric_write.cpp` | 5 (M,S) write pairs + ID prefix correctness on AW/W, ID strip on B |
| `tb_nic400_fabric_throughput.cpp` | Multi-outstanding ARs per (M,S); 3M→3S aggregate throughput |
| `tb_nic400_fabric_regslice.cpp` | Same 5 (M,S) write pairs through `Nic400FabricRs1` (1-cycle reg slice each) |
| `tb_nic400_ahb_bridge.cpp` | Single-beat reads + writes (HBURST=SINGLE) |
| `tb_nic400_ahb_bridge_burst.cpp` | Fixed-length INCR4 / INCR8 + backpressure + SLVERR |
| `tb_nic400_ahb_bridge_incr.cpp` | Short INCR-undef (1/4/16-beat) + SLVERR |
| `tb_nic400_ahb_bridge_long.cpp` | Long INCR-undef multi-chunk (17/24/32/48/64-beat) + SLVERR |
| `tb_nic400_apb_bridge.cpp` | Single-beat R/W + INCR4 burst + pready stall + SLVERR |
| `tb_nic400_width_adapter.cpp` | 64→32 downsize: 1-beat R/W + INCR4 R/W + strb + SLVERR |
| `tb_nic400_pmu.cpp` | Per-master AR/AW/R/W/B counter integration |
| `tb_nic400_qos_fn.cpp` | QoS function decoding |
| `tb_reg_slice_channel.cpp` | Generic single-channel `RegSliceChannel` skid buffer |
| `tb_nic400_read2x2_*.cpp` | Older 2×2 read-only crossbar regressions |

## Performance findings

Measured via `tb_nic400_fabric_latency.cpp` and `tb_nic400_fabric_throughput.cpp`:

| Path | Spec target | Observed (fast-gate wait) | Observed (Moore `wait`) |
|------|-------------|----------------------------|--------------------------|
| AR forward (M → S, uncontested) | 0 cycles | **0 cycles ✓** | 1 cycle |
| R return (S → M, uncontested) | 0 cycles | **0 cycles ✓** | 1 cycle |
| AR throughput (back-to-back) | 1 txn/cycle | **1.00 t/c** (9/9) | 0.32 t/c (8/25) |
| 3M→3S concurrent aggregate | linear M scaling | **3.00 t/c** (18/6) | — |

The MasterPort/SlavePort threads use the fast-gate form:

```arch
if not X
  wait until X;
end if
```

When immediately followed by `do BODY until Y;` or `lock R do BODY until
Y; end lock R;`, this fuses into a single Mealy-style state — both the
comb drives and the transition guard live in one posedge, collapsing the
entry-wait bubble that the Moore form imposes.

### Multi-outstanding (no design change required)

The current design supports depth-∞ outstanding ARs per (master, slave)
pair out-of-the-box — no ROB, no AR FIFO, no `outstanding` parameter.
`tb_nic400_fabric_throughput.cpp` pins:

- M0 → S0 with R-lag delayed 4 cycles: **6 ARs in flight before first R returns**.
- M0 alternating S0/S1 each cycle: **1.00 t/c (8/8)**.
- 3M → 3S concurrent (6 ARs each, simultaneous): **3.00 t/c aggregate**, linear M scaling.

The `_ch` locks are single-driver guards, not throughput throttles —
they only serialise cycle-level drives to shared `m.*` outputs, which is
exactly what AXI4 allows (one handshake per cycle). The "depth-1 per
(master, slave)" caveat that appears in earlier source comments refers
specifically to the *write* path's AW→W→B sequencing inside a single
thread; the read path's split AR/R threads do not have that property.

## Bumping the fabric size

Bumping `NUM_MASTERS` / `NUM_SLAVES` on `Nic400Fabric` is a one-line
change — every per-port instance and connection unrolls via
`generate_for`. The matching defaults on `Nic400MasterPort` and
`Nic400SlavePort` need to track (they don't auto-inherit from the
Fabric's params; the inst connections rely on the defaults). The header
comment in `Nic400Fabric.arch` calls out the gotcha.

## Patterns to follow / pitfalls to avoid

Patterns that work and are exercised in this demo:

- Back-to-back `lock R do … until …; end lock R;` blocks inside a top-level thread body, with multi-thread contention on a shared `mutex<priority>` resource.
- `generate_for` over masters / slaves / bus arrays — handles M×N inst declarations from one source.
- Nested `for` loops inside threads (each gets its own loop counter; see #414's fix).
- AXI bus aliases bound by module params and referenced from `generate_if` bodies (see #423's fix).
- Whole-Vec<Bus,N> inst port forwarding `m <- m_top` (see #424's fix).
- Mealy fusion of `if not X; wait until X; end if; lock R do BODY until Y; end lock R;` for zero-overhead handshake throughput.
- `Concat({addr, id, len, …})` over bus port signals with module-param-bound bus alias params (see #427's fix).
- Inner-for body ending in if/else with lock-per-branch, where each branch advances state (see #422's fix; used by `Nic400WidthAdapter`'s R thread).

Patterns that bite or remain limited:

- `do BODY until cond` at the top level of a `thread` is rejected if `BODY` contains nested `lock`/`for`/`wait` — use a `lock` block instead (see #410's resolution).
- HTRANS=BUSY is not handled by `Nic400AhbBridge`; the bridge assumes cache-line-fill style INCR-undef where the master uses SEQ exclusively.
- `Nic400AhbBridge` INCR-undef bursts longer than `MAX_INCR_BEATS * MAX_INCR_CHUNKS` (default 64 beats) hang the master.
- `Nic400ApbBridge` supports one outstanding R + one outstanding W (no AXI exclusive, no APB v3 pwakeup).
- `Nic400WidthAdapter` is downsize-only; upsize needs an accumulator buffer.

## Compiler debt — context for future contributors

Building this demo uncovered **eight** thread-lowering / elaboration-scope issues that were filed and fixed along the way:

| Issue | Fix PR | Topic |
|-------|--------|-------|
| #410 | #411 | top-level `do BODY until cond` looped infinitely; reject nested control flow |
| #412 | #413 | Mealy-fused fast-gate `do … until …;` seq assigns ungated by wake condition |
| #414 | #415 | nested `for` loops shared a single `_loop_cnt` register |
| #422 | #430 | inner-for + if/else with lock-per-branch lost outer-for continuation transitions |
| #423 | #425 | type alias inside `generate_if` lost bound bus params |
| #424 | #426 | whole-Vec<Bus,N> inst port forwarding raised undriven-port errors |
| #427 | #428 | sim_codegen `Concat` width wrong with module-param-bound bus alias |

Each fix shipped with a minimal repro committed under `tests/regression/issues/`. Together with [`arch-hdl-lang/arch-com#383`](https://github.com/arch-hdl-lang/arch-com/issues/383) (hierarchical formal rejecting auto-generated thread sub-modules with lock-arbitration `wire` decls — open), they form the most concentrated cluster of thread/elaboration issues uncovered in any single demo build.
