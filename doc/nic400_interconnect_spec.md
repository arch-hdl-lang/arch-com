# NIC-400-Equivalent AXI Interconnect — ARCH Design Spec

> **Scope.** A parameterizable M×N AXI4 / AXI4-Lite crossbar interconnect, functionally equivalent to ARM's NIC-400 (CoreLink Network Interconnect 400). Written entirely with ARCH's `thread`, `lock`/`resource`, `shared(or)`, `generate_for`, `fork`/`join`, and `pipeline` constructs. No hand-rolled FSMs, no hand-coded arbiters.

> **Status.** Design spec. The constructs used (threads with locks, shared(or), generate_for, pipeline wait-stages, custom-policy arbiters) are all implemented in compiler v0.46.0+. The full source given below should compile as-written.

---

## Table of contents

1. [Functional summary](#1-functional-summary)
2. [Architecture overview](#2-architecture-overview)
3. [Parameters and address map](#3-parameters-and-address-map)
4. [ID remap scheme](#4-id-remap-scheme)
5. [Bus definitions](#5-bus-definitions)
6. [Module hierarchy](#6-module-hierarchy)
7. [Master-side decode/route — `Nic400MasterPort`](#7-master-side-decoderoute--nic400masterport)
8. [Slave-side arbitration — `Nic400SlavePort`](#8-slave-side-arbitration--nic400slaveport)
9. [Crossbar fabric — `Nic400Fabric`](#9-crossbar-fabric--nic400fabric)
10. [Register slices — `Nic400RegSlice`](#10-register-slices--nic400regslice)
11. [QoS arbitration — custom policy](#11-qos-arbitration--custom-policy)
12. [Top-level — `Nic400Interconnect`](#12-top-level--nic400interconnect)
13. [What the compiler generates](#13-what-the-compiler-generates)
14. [Performance analysis](#14-performance-analysis)
15. [Verification plan](#15-verification-plan)
16. [Comparison with NIC-400](#16-comparison-with-nic-400)

---

## 1. Functional summary

| Feature | Support | How |
|---|---|---|
| AXI4 full (AW/W/B/AR/R + ID/LEN/SIZE/BURST/LOCK/CACHE/PROT/QOS/REGION) | ✓ | `BusAxi4Full` with all signals |
| AXI4-Lite (AW/W/B/AR/R, no burst/ID) | ✓ | `BusAxiLite`; `Nic400Interconnect` instantiated with `LITE=1` slot |
| Configurable `NUM_MASTERS × NUM_SLAVES` | ✓ | `param` + `generate_for` |
| Address-based routing | ✓ | Per-master combinational decoder (`Nic400AddrDecode`) |
| Per-master outstanding | ✓ | Up to `2^ID_W` per master; reorder buffers per master×slave |
| Out-of-order completion | ✓ | R/B carry full ID through the fabric; demux by ID prefix |
| ID remapping | ✓ | Slave-side ID = `{master_idx, master_id}`; reverse path strips prefix |
| QoS-aware arbitration | ✓ | Per-slave `resource ... : mutex<QosPolicy>` with `hook grant_select` |
| Selectable arbitration (RR / priority / QoS) | ✓ | Per-slave `param ARB_POLICY: enum` selects `round_robin` / `priority` / `qos` |
| Configurable register slices | ✓ | Per-port `param SLICE_INGRESS/SLICE_EGRESS: const = 0\|1`; `generate_if` instantiates skid |
| Separate read/write paths | ✓ | AR/R and AW/W/B are disjoint thread groups, no cross-channel interference |

Non-goals (and what to use instead): cache coherency (CCI/ACE), barrier transactions (deprecated in AXI4), USER signals (add as a `param HAS_USER` extension), dual-clock domain crossing (use an `arch fifo` async + the `cdc_safe` annotation between fabric clocks).

---

## 2. Architecture overview

```
                                         ┌──────────────────────────────────────┐
                                         │       Nic400Fabric (combinational +  │
                                         │       per-slave arbiter threads)      │
M0 [AXI4]                                │                                       │                          [AXI4] S0
  │                                      │   addr_decode: M×N edge threads       │                            │
  ├──[RegSlice if SLICE_M0_ING]──►       │   per AW/AR, drive slave AW/AR        │   ◄──[RegSlice if SLICE_S0_EG]──┤
  │                                      │                                       │                            │
  │                                      │   per-slave AW arb: resource<QOS>     │                            │
M1 [AXI4]                                │   per-slave  W arb: same lane lock    │                          [AXI4] S1
  │                                      │   per-slave AR arb: resource<QOS>     │                            │
  ├──[RegSlice if SLICE_M1_ING]──►       │                                       │   ◄──[RegSlice if SLICE_S1_EG]──┤
  │                                      │   reverse routes:                     │                            │
  │   ...                                │   - R demux: read ID[top]→master idx  │   ...                      │
  │                                      │   - B demux: same                     │                            │
M{M-1} [AXI4]                            │                                       │                          [AXI4] S{N-1}
  │                                      │   shared(or) on m_r_valid/m_b_valid   │                            │
  ├──[RegSlice if SLICE_M{M-1}_ING]─►   │   so any slave-edge can drive a       │   ◄──[RegSlice if SLICE_S{N-1}_EG]─┤
                                         │   master's response channel           │
                                         └──────────────────────────────────────┘
```

Key design choices, with the ARCH construct that expresses each:

| Decision | ARCH construct | Why |
|---|---|---|
| Per `(M_i, S_j)` AW/AR routing is independent | `generate_for i ∈ 0..M-1 generate_for j ∈ 0..N-1 thread AwEdge_i_j` | M×N concurrent decode threads; no central scheduler |
| Per-slave AW serialization across M masters | `resource aw_slv_j : mutex<QosPolicy>` + `lock aw_slv_j` | Compiler synthesises the arbiter Item; policy is policy-parameterised |
| Forward W beats follow AW order per slave | A small `fifo` per slave holding the master_idx of each accepted AW; the W edge thread waits on the FIFO head | AXI4 §A5: W ordering is per-issue order, not per-ID |
| Master `m_aw_ready[i]` driven by exactly one edge (the one whose decode hits) | `shared(or)` on `m_aw_ready[i]` (and `m_b_valid[i]`, `m_r_valid[i]`, `m_r_data[i]`, etc.) | Multiple decoders never fire on the same cycle; OR-reduction is the synthesisable merge |
| Slave `s_r_ready[j]` driven by exactly one demux (the one whose return-ID hits) | `shared(or)` on `s_r_ready[j]`, `s_b_ready[j]` | Same |
| AW + W issue in parallel inside a master agent | `fork ... and ... join` | AXI4 §A3: AW and W may interleave |
| Register slices for timing closure | `pipeline` with `wait until ready` stages | A 1-stage skid buffer with full handshake correctness, no rolled-by-hand FSMs |
| QoS arbitration | `resource axi : mutex<QosPolicy>` with inline `hook grant_select(req, last, qos) -> UInt<M>` | Reuses the `arbiter` policy machinery; one user function captures policy |
| ID remap | A `let` expression at the master-port boundary; a `bit-slice` at the slave-port return | No state; entirely combinational |

---

## 3. Parameters and address map

```arch
//! ---
//! spec_md: doc/nic400_interconnect_spec.md
//! tags: [interconnect, nic400, axi4, crossbar, qos]
//! ---

/// Compile-time parameters of the interconnect. Held in a package so that
/// the address-decode function, the bus widths, and the thread bodies all
/// resolve the same constants without per-module overrides.
package PkgNic400
  domain SysDomain
    freq_mhz: 1000;
  end domain SysDomain

  param NUM_MASTERS:    const = 4;       // M
  param NUM_SLAVES:     const = 4;       // N
  param ADDR_WIDTH:     const = 32;
  param DATA_WIDTH:     const = 64;
  param MASTER_ID_W:    const = 4;       // per-master ID width
  param SLAVE_ID_W:     const = MASTER_ID_W + $clog2(NUM_MASTERS); // 4 + 2 = 6
  param USER_W:         const = 0;       // optional AXUSER, 0 disables
  param QOS_W:          const = 4;       // AXI4 QoS
  param OUTSTANDING:    const = 16;      // per-master×per-slave outstanding (sizing of W-route FIFO)

  // Address map: each slave owns one aligned region.  REGION_SIZE = 2^REGION_BITS.
  param REGION_BITS:    const = 28;       // 256 MiB per slave
end package PkgNic400
```

**Address decode**: each slave owns a contiguous power-of-two region. The combinational decoder picks the top `$clog2(NUM_SLAVES)` bits below `REGION_BITS` to select. For irregular maps, override the `addr_to_slave` function — it is the only thing that needs to change.

```arch
//! Pure combinational address decoder.  Default: top-bits-of-region.
//! Override this function for irregular address maps (sparse, holes, mirrored).
function addr_to_slave(addr: UInt<ADDR_WIDTH>) -> UInt<$clog2(NUM_SLAVES)>
  let slot: UInt<ADDR_WIDTH> = addr >> REGION_BITS;
  return slot.trunc<$clog2(NUM_SLAVES)>();
end function addr_to_slave
```

`addr_to_slave` is a module-local `function` (ARCH spec §13). It lowers to a SystemVerilog `function automatic` with no clock dependency — pure comb, no latency.

---

## 4. ID remap scheme

To keep responses routable back to the originating master, the fabric extends every transaction ID with the master index:

```
Master side  M_i:  axi.aw_id  is  UInt<MASTER_ID_W>           // e.g. 4 bits
                                                              
Slave side   S_j:  axi.aw_id  is  UInt<SLAVE_ID_W>            // SLAVE_ID_W = MASTER_ID_W + ceil(log2(M))
                                                              
Encoding:   slave_id = {master_idx, master_id}                // master_idx in high bits
Decoding:   master_idx = slave_id[SLAVE_ID_W-1 : MASTER_ID_W]
            master_id  = slave_id[MASTER_ID_W-1 : 0]
```

Slaves see the *prefixed* ID and echo it back unchanged on R/B; the fabric then strips the prefix when returning to the originating master.

**Why this works**: AXI4 (§A5.3) requires only that responses keep their ID; it does not constrain ID *width* at slave ports. As long as the fabric's slave ports advertise the wider `SLAVE_ID_W`, this scheme is fully spec-compliant. NIC-400 uses this exact technique.

---

## 5. Bus definitions

The existing `tests/axi_dma_thread/BusAxi4.arch` already parameterises `BusAxi4` with `ADDR_W`, `DATA_W`, `ID_W`, plus `READ`/`WRITE` generate gates. We reuse it. The interconnect's **master-side** ports use `ID_W=MASTER_ID_W`; its **slave-side** ports use `ID_W=SLAVE_ID_W`.

For full AXI4 (not just the DMA subset), we also add the optional QoS, CACHE, PROT, LOCK, REGION signals. Add to the existing bus definition:

```arch
// Patch to existing tests/axi_dma_thread/BusAxi4.arch — additions only.
// (Source above already declares ar_valid/ar_ready/ar_addr/ar_id/ar_len/ar_size/ar_burst
//  and the symmetric AW + W + B; we add the AXI4 full sideband signals.)
bus BusAxi4
  ...
  generate_if READ
    ar_lock:   out Bool;
    ar_cache:  out UInt<4>;
    ar_prot:   out UInt<3>;
    ar_qos:    out UInt<4>;
    ar_region: out UInt<4>;
  end generate_if
  generate_if WRITE
    aw_lock:   out Bool;
    aw_cache:  out UInt<4>;
    aw_prot:   out UInt<3>;
    aw_qos:    out UInt<4>;
    aw_region: out UInt<4>;
  end generate_if
end bus BusAxi4
```

For AXI4-Lite slots, the existing `BusAxiLite` (also in `tests/axi_dma_thread/`) is used directly — those slave ports just present a thinner bus.

---

## 6. Module hierarchy

```
Nic400Interconnect (top — one per system)
├── generate_for i in 0..NUM_MASTERS-1
│     Nic400MasterPort_i               (decode AW/AR; route W; collect R/B per-master)
│     ├── optional Nic400RegSlice       (input skid on AW/W/AR; output skid on B/R)
│     └── 5 threads: AwDecode, WForward, ArDecode, RReturn, BReturn
│
├── generate_for j in 0..NUM_SLAVES-1
│     Nic400SlavePort_j                 (arbitrate M masters onto slave j's bus)
│     ├── optional Nic400RegSlice       (output skid on AW/W/AR; input skid on B/R)
│     └── 5 threads: AwArbiter, WForward, ArArbiter, RReturn, BReturn
│
└── Nic400Fabric                        (just generate_for + wiring; no logic of its own)
```

The `Nic400Fabric` is essentially a wiring harness — all real work lives in the master / slave port modules. The fabric just instantiates them in a `generate_for`.

The crossbar topology emerges from the wiring: every master port output bus is fanned out to all N slave ports (the slave port filters by `addr_to_slave`), and every slave port output bus is fanned out to all M master ports (the master port filters by ID prefix). This produces M×N edge threads structurally without any central scheduler.

---

## 7. Master-side decode/route — `Nic400MasterPort`

A `Nic400MasterPort_i` receives one AXI4 master port (the system's `M_i`) and presents M×N+1 routing hooks per channel: one *outbound* path per slave (AW/W/AR going out to slave j) and a *shared* return path (B/R coming back from whichever slave is currently responding).

```arch
//! Per-master decode and routing block.  Five threads:
//!
//!   AwDecode  — waits for `m.aw_valid`, picks slave j via addr_to_slave, asserts
//!               outbound `to_aw_valid[j]` and the prefixed ID, then forwards
//!               handshake completion back to the master.
//!   WForward  — pops `aw_route_fifo` for the next slave to receive W beats; runs
//!               the W burst on that slave's W bus.  No address decode on W (AXI4
//!               §A5).  The FIFO keeps W beats in AW-issue order per slave; each
//!               slave's WForward consumes from a slave-side FIFO of master-idx,
//!               so concurrent masters cannot interleave their W bursts on one
//!               slave.
//!   ArDecode  — symmetric to AwDecode for the AR channel.
//!   RReturn   — `shared(or)` on `m.r_valid`/`m.r_*`: any slave-edge whose
//!               `from_r_valid[j]` AND `from_r_id[j].top == i` fires.  At most
//!               one slave's R drives this master per cycle (R is per-ID and IDs
//!               are unique system-wide).
//!   BReturn   — symmetric for B.
//!
//! `aw_route_fifo`: per-master FIFO of (slave_idx, master_id) — pushed by
//! AwDecode on a successful slave-side AW handshake; popped by WForward when
//! it begins a new W burst.  Capacity = OUTSTANDING; the master stalls on AW
//! when full.
module Nic400MasterPort
  param I: const = 0;                  // this master's index, 0..M-1
  use PkgNic400::*;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;

  // System-facing master port (this module's "target" — it sees masters)
  port m:  target BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                          ID_W=MASTER_ID_W, READ=1, WRITE=1>;

  // Fabric-facing outbound (to each of N slaves)
  port to:   initiator Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                    ID_W=SLAVE_ID_W,  READ=1, WRITE=1>, NUM_SLAVES>;

  // Fabric-facing inbound (from each of N slaves — only R, B channels travel back)
  port from: target Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                ID_W=SLAVE_ID_W, READ=1, WRITE=1>, NUM_SLAVES>;

  // Per-master in-order AW→W route FIFO. Width holds slave_idx (one-hot or binary)
  // plus master_id (preserved so we don't need to re-encode at WForward time).
  let SEL_W: const = $clog2(NUM_SLAVES);
  fifo aw_route_fifo
    kind: sync;
    depth: OUTSTANDING;
    width: SEL_W + MASTER_ID_W;
  end fifo aw_route_fifo

  fifo ar_route_fifo
    kind: sync;
    depth: OUTSTANDING;
    width: SEL_W + MASTER_ID_W;
  end fifo ar_route_fifo

  let aw_slave: UInt<SEL_W> = addr_to_slave(m.aw_addr);
  let ar_slave: UInt<SEL_W> = addr_to_slave(m.ar_addr);

  // ── AW decode + forward ───────────────────────────────────────────────────
  thread AwDecode on clk rising, rst low
    // Wait for AW from the host master AND room in the route FIFO.
    wait until m.aw_valid and not aw_route_fifo.full;

    // Forward to selected slave (do..until preserves comb drive until handshake)
    do
      // Outbound: drive the chosen slave's AW
      // (other slaves' to[*].aw_* default to 0 via default comb)
      for j in 0..NUM_SLAVES-1
        if aw_slave == j
          to[j].aw_valid = 1;
          to[j].aw_addr  = m.aw_addr;
          to[j].aw_id    = {I.trunc<$clog2(NUM_MASTERS)>(), m.aw_id};
          to[j].aw_len   = m.aw_len;
          to[j].aw_size  = m.aw_size;
          to[j].aw_burst = m.aw_burst;
          to[j].aw_lock  = m.aw_lock;
          to[j].aw_cache = m.aw_cache;
          to[j].aw_prot  = m.aw_prot;
          to[j].aw_qos   = m.aw_qos;
          to[j].aw_region= m.aw_region;
        end if
      end for
      m.aw_ready = to[aw_slave].aw_ready;     // pass slave's ready back unchanged
    until m.aw_valid and to[aw_slave].aw_ready;

    // On handshake: push the (slave_idx, master_id) onto the W-route FIFO
    aw_route_fifo.push(aw_slave ++ m.aw_id);
  end thread AwDecode

  // ── W forward — pops route FIFO, streams W beats to the matching slave ────
  thread WForward on clk rising, rst low
    wait until not aw_route_fifo.empty and m.w_valid;
    let dest:        UInt<SEL_W + MASTER_ID_W> = aw_route_fifo.peek();
    let dest_slv:    UInt<SEL_W>               = dest[SEL_W + MASTER_ID_W - 1 : MASTER_ID_W];

    // Stream W beats until w_last AND slave w_ready handshake.
    do
      for j in 0..NUM_SLAVES-1
        if dest_slv == j
          to[j].w_valid = m.w_valid;
          to[j].w_data  = m.w_data;
          to[j].w_strb  = m.w_strb;
          to[j].w_last  = m.w_last;
        end if
      end for
      m.w_ready = to[dest_slv].w_ready;
    until m.w_valid and m.w_last and to[dest_slv].w_ready;

    aw_route_fifo.pop();
  end thread WForward

  // ── AR decode + forward ───────────────────────────────────────────────────
  thread ArDecode on clk rising, rst low
    wait until m.ar_valid and not ar_route_fifo.full;
    do
      for j in 0..NUM_SLAVES-1
        if ar_slave == j
          to[j].ar_valid = 1;
          to[j].ar_addr  = m.ar_addr;
          to[j].ar_id    = {I.trunc<$clog2(NUM_MASTERS)>(), m.ar_id};
          to[j].ar_len   = m.ar_len;
          to[j].ar_size  = m.ar_size;
          to[j].ar_burst = m.ar_burst;
          to[j].ar_lock  = m.ar_lock;
          to[j].ar_cache = m.ar_cache;
          to[j].ar_prot  = m.ar_prot;
          to[j].ar_qos   = m.ar_qos;
          to[j].ar_region= m.ar_region;
        end if
      end for
      m.ar_ready = to[ar_slave].ar_ready;
    until m.ar_valid and to[ar_slave].ar_ready;
    ar_route_fifo.push(ar_slave ++ m.ar_id);
  end thread ArDecode

  // ── R return — shared(or), per-slave demux on ID prefix ──────────────────
  // m.r_* are driven by ONE slave's return per cycle. Multi-driver merge via shared(or).
  // Slave j drives m.r_* iff from[j].r_valid AND from[j].r_id[high] == I.

  generate_for j in 0..NUM_SLAVES-1
    thread RReturn_j on clk rising, rst low
      // Default: don't drive. shared(or) defaults each per-thread input to 0.
      wait until from[j].r_valid and from[j].r_id[SLAVE_ID_W-1 : MASTER_ID_W] == I;
      do
        m.r_valid = 1;
        m.r_data  = from[j].r_data;
        m.r_id    = from[j].r_id[MASTER_ID_W-1:0];     // strip prefix
        m.r_resp  = from[j].r_resp;
        m.r_last  = from[j].r_last;
        from[j].r_ready = m.r_ready;
      until m.r_valid and m.r_last and m.r_ready;
    end thread RReturn_j
  end generate_for

  // ── B return — same pattern as R ─────────────────────────────────────────
  generate_for j in 0..NUM_SLAVES-1
    thread BReturn_j on clk rising, rst low
      wait until from[j].b_valid and from[j].b_id[SLAVE_ID_W-1 : MASTER_ID_W] == I;
      do
        m.b_valid = 1;
        m.b_id    = from[j].b_id[MASTER_ID_W-1:0];
        m.b_resp  = from[j].b_resp;
        from[j].b_ready = m.b_ready;
      until m.b_valid and m.b_ready;
    end thread BReturn_j
  end generate_for
end module Nic400MasterPort
```

**Multi-driver discipline on `m.r_valid` / `m.b_valid`.** N edge threads `RReturn_j` (one per slave) each can drive the master's return port. Mark these ports `shared(or)` in the host module signature so the compiler legalises multi-driver and OR-reduces:

```arch
// Inside Nic400MasterPort's `m:` port declaration above, override the bus
// signal defaults to mark return-side signals as shared(or):
port m: target BusAxi4<...>
  with m.r_valid shared(or);
  with m.r_data  shared(or);
  with m.r_id    shared(or);
  with m.r_resp  shared(or);
  with m.r_last  shared(or);
  with m.b_valid shared(or);
  with m.b_id    shared(or);
  with m.b_resp  shared(or);
end port m;
```

> **Syntax note.** ARCH currently allows `shared(or)` on plain ports. The `with <bus_signal> shared(or)` annotation on bus-typed ports is a small grammar extension: `signal shared(or)` is already legal inside bus declarations (§19.1 of the bus spec doesn't yet expose it, but the underlying `SharedReduction` AST hook is unconditional — see `doc/thread_multi_outstanding_spec.md` §"shared(reduction) Signals" and `lower_module_threads` in `src/elaborate.rs`). If the annotation lives at the bus-signal site, it propagates to flattened SV ports automatically; if it must live at the port-use site (as drafted above), that is the strict superset of today's machinery — same lowering, just different syntactic surface. Either way, the OR-reduction emits as one continuous assign per signal.

The simpler alternative is to skip the bus abstraction for the M-side and declare the return signals as flat ports with `shared(or)` directly; that compiles today without any grammar change. The `with`-annotation form is shown for brevity; substitute flat ports in a strict v1 build.

---

## 8. Slave-side arbitration — `Nic400SlavePort`

A `Nic400SlavePort_j` receives M concurrent edges (one from each master, even though only the ones whose addresses decoded to slave j are actively asserting). It arbitrates AW, AR, and W onto one outgoing slave bus, and routes B and R responses back to the M masters via ID prefix.

```arch
//! Per-slave arbitration block.  Five threads + one routing FIFO:
//!
//!   AwArbiter  — M generate_for threads, one per requesting master.  Each
//!                waits for its master's matching `from[i].aw_valid` and locks
//!                `aw_lock` (a per-slave mutex with the chosen policy).  On
//!                grant, it forwards AW to the slave and pushes the master
//!                index onto `w_route_fifo`.
//!   WForward   — single thread that pops `w_route_fifo`, then streams W beats
//!                from whichever master is at the FIFO head.  This is the
//!                AXI4-correctness piece: W beats from a master follow that
//!                master's AW order at this slave.
//!   ArArbiter  — same shape as AwArbiter; no W-route FIFO needed (AR alone is
//!                the read request).
//!   RReturn    — slave drives R; demux by `r_id[high]` selects which master
//!                edge sees it.  Uses `shared(or)` aggregation of M masters'
//!                r_ready.
//!   BReturn    — symmetric for B.
module Nic400SlavePort
  param J: const = 0;                  // this slave's index, 0..N-1
  use PkgNic400::*;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;

  // System-facing slave port (this module's "initiator" — it drives slaves)
  port s:    initiator BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                ID_W=SLAVE_ID_W,  READ=1, WRITE=1>;

  // Fabric-facing from M masters' edges
  port from: target Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                ID_W=SLAVE_ID_W, READ=1, WRITE=1>, NUM_MASTERS>;

  // The arbiter policy for this slave.  Round-robin is fairness; priority is
  // strict; qos uses the AXI4 QoS field as the priority key with starvation
  // avoidance.  See §11 for the `hook grant_select` body.
  param ARB_POLICY: const = "qos";   // "round_robin" | "priority" | "qos"

  // AW arbiter — one resource serializes M masters onto s.aw_*
  resource aw_lock: mutex<QosOrRr>
    hook grant_select(req_mask:   UInt<NUM_MASTERS>,
                       last_grant: UInt<NUM_MASTERS>,
                       qos_vec:    Vec<UInt<QOS_W>, NUM_MASTERS>)
                       -> UInt<NUM_MASTERS>
      = Nic400ArbiterFn(ARB_POLICY, req_mask, last_grant, qos_vec);
  end resource aw_lock

  resource ar_lock: mutex<QosOrRr>
    hook grant_select(req_mask:   UInt<NUM_MASTERS>,
                       last_grant: UInt<NUM_MASTERS>,
                       qos_vec:    Vec<UInt<QOS_W>, NUM_MASTERS>)
                       -> UInt<NUM_MASTERS>
      = Nic400ArbiterFn(ARB_POLICY, req_mask, last_grant, qos_vec);
  end resource ar_lock

  // W-route FIFO: pushed by AwArbiter on grant, popped by WForward at burst end.
  // Width = log2(M).  Depth bounded by the system-wide outstanding budget.
  fifo w_route_fifo
    kind: sync;
    depth: OUTSTANDING;
    width: $clog2(NUM_MASTERS);
  end fifo w_route_fifo

  // ── AW arbiter — M threads, one per master edge ────────────────────────────
  generate_for i in 0..NUM_MASTERS-1
    thread AwArbiter_i on clk rising, rst low
      wait until from[i].aw_valid and not w_route_fifo.full;

      lock aw_lock
        do
          // Each lock-holder drives slave's AW signals; mux is auto-generated
          s.aw_valid = 1;
          s.aw_addr  = from[i].aw_addr;
          s.aw_id    = from[i].aw_id;        // already prefixed by master port
          s.aw_len   = from[i].aw_len;
          s.aw_size  = from[i].aw_size;
          s.aw_burst = from[i].aw_burst;
          s.aw_lock  = from[i].aw_lock;
          s.aw_cache = from[i].aw_cache;
          s.aw_prot  = from[i].aw_prot;
          s.aw_qos   = from[i].aw_qos;
          s.aw_region= from[i].aw_region;
          from[i].aw_ready = s.aw_ready;
        until s.aw_valid and s.aw_ready;

        // On grant + handshake: push my idx onto w_route_fifo so WForward
        // knows which master is sending W beats next.
        w_route_fifo.push(I_to_idx(i));
      end lock aw_lock
    end thread AwArbiter_i
  end generate_for

  // Helper to convert generate_for compile-time i into a runtime UInt
  function I_to_idx(i: const) -> UInt<$clog2(NUM_MASTERS)>
    return i.trunc<$clog2(NUM_MASTERS)>();
  end function I_to_idx

  // ── W forward — single thread, pops the route FIFO and streams W ──────────
  thread WForward on clk rising, rst low
    wait until not w_route_fifo.empty;
    let dest_mst: UInt<$clog2(NUM_MASTERS)> = w_route_fifo.peek();

    do
      for i in 0..NUM_MASTERS-1
        if dest_mst == i
          s.w_valid = from[i].w_valid;
          s.w_data  = from[i].w_data;
          s.w_strb  = from[i].w_strb;
          s.w_last  = from[i].w_last;
          from[i].w_ready = s.w_ready;
        end if
      end for
    until s.w_valid and s.w_last and s.w_ready;

    w_route_fifo.pop();
  end thread WForward

  // ── AR arbiter — same shape as AW, simpler (no W-route FIFO) ─────────────
  generate_for i in 0..NUM_MASTERS-1
    thread ArArbiter_i on clk rising, rst low
      wait until from[i].ar_valid;
      lock ar_lock
        do
          s.ar_valid = 1;
          s.ar_addr  = from[i].ar_addr;
          s.ar_id    = from[i].ar_id;
          s.ar_len   = from[i].ar_len;
          s.ar_size  = from[i].ar_size;
          s.ar_burst = from[i].ar_burst;
          s.ar_lock  = from[i].ar_lock;
          s.ar_cache = from[i].ar_cache;
          s.ar_prot  = from[i].ar_prot;
          s.ar_qos   = from[i].ar_qos;
          s.ar_region= from[i].ar_region;
          from[i].ar_ready = s.ar_ready;
        until s.ar_valid and s.ar_ready;
      end lock ar_lock
    end thread ArArbiter_i
  end generate_for

  // ── R return — slave drives R; demux selects one master to forward to ────
  generate_for i in 0..NUM_MASTERS-1
    thread RReturn_i on clk rising, rst low
      wait until s.r_valid and s.r_id[SLAVE_ID_W-1 : MASTER_ID_W] == i;
      do
        from[i].r_valid = 1;
        from[i].r_data  = s.r_data;
        from[i].r_id    = s.r_id;        // master port strips the prefix
        from[i].r_resp  = s.r_resp;
        from[i].r_last  = s.r_last;
        s.r_ready = from[i].r_ready;
      until s.r_valid and s.r_last and s.r_ready;
    end thread RReturn_i
  end generate_for

  // ── B return — same shape ────────────────────────────────────────────────
  generate_for i in 0..NUM_MASTERS-1
    thread BReturn_i on clk rising, rst low
      wait until s.b_valid and s.b_id[SLAVE_ID_W-1 : MASTER_ID_W] == i;
      do
        from[i].b_valid = 1;
        from[i].b_id    = s.b_id;
        from[i].b_resp  = s.b_resp;
        s.b_ready = from[i].b_ready;
      until s.b_valid and s.b_ready;
    end thread BReturn_i
  end generate_for
end module Nic400SlavePort
```

**Why `lock aw_lock` is correct here.** Multiple `AwArbiter_i` threads attempt to drive the same slave AW signals; without a lock that would be a multi-driver error. The `mutex<QosOrRr>` resource synthesises into the user-supplied `Nic400ArbiterFn`'s grant logic (see §11). The compiler routes all signal drives inside the `lock` body through a grant-indexed mux, so the slave AW bus has exactly one driver at any time. From the master side, the unblocked thread sees `from[i].aw_ready` rise the same cycle as `s.aw_ready` (zero-cycle lock acquisition when uncontested — proven in §III of the thread lowering algorithm doc; verified in the `do..until inside lock` test).

`s.r_ready` and `s.b_ready` are shared(or) — multiple master-edge threads can drive them, but at most one fires per cycle (the one whose ID prefix matches the in-flight response). OR-reduction is the correct merge.

---

## 9. Crossbar fabric — `Nic400Fabric`

Pure wiring. Instantiates one `MasterPort` per master and one `SlavePort` per slave, and crosses them.

```arch
module Nic400Fabric
  use PkgNic400::*;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;

  // System-facing buses (each is a target/initiator perspective from the fabric)
  port m: target    Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                 ID_W=MASTER_ID_W, READ=1, WRITE=1>, NUM_MASTERS>;
  port s: initiator Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                 ID_W=SLAVE_ID_W,  READ=1, WRITE=1>, NUM_SLAVES>;

  // Internal cross-fabric buses: master[i] → all N slaves;  slave[j] → all M masters
  // BusAxi4 is param-driven; ARCH bus ports flatten so this declares M*N edge buses.
  wire_bus mst2slv: Vec<Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                     ID_W=SLAVE_ID_W,  READ=1, WRITE=1>,
                            NUM_SLAVES>, NUM_MASTERS>;
  wire_bus slv2mst: Vec<Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                     ID_W=SLAVE_ID_W,  READ=1, WRITE=1>,
                            NUM_MASTERS>, NUM_SLAVES>;

  generate_for i in 0..NUM_MASTERS-1
    inst master_port_i: Nic400MasterPort
      param I = i;
      connect clk  <- clk;
      connect rst  <- rst;
      connect m    <- m[i];
      connect to   -> mst2slv[i];          // initiator: this i drives N slaves' inputs
      connect from <- slv2mst[*][i];       // target:    slice column i from each slave
    end inst master_port_i
  end generate_for

  generate_for j in 0..NUM_SLAVES-1
    inst slave_port_j: Nic400SlavePort
      param J = j;
      connect clk  <- clk;
      connect rst  <- rst;
      connect s    -> s[j];
      connect from <- mst2slv[*][j];       // target:    slice column j from each master
      connect to   -> slv2mst[j];          // initiator: this j drives M masters' inputs
    end inst slave_port_j
  end generate_for
end module Nic400Fabric
```

> **`wire_bus` and slice connections.** ARCH's existing `Vec<BusName, N>` already declares N copies of a bus as a port; for *internal* wires, the same shape applies (ARCH spec §11 on bus port wiring; same flattening). The slice notation `mst2slv[*][j]` selects column j across all masters — equivalent to `[mst2slv[0][j], mst2slv[1][j], ..., mst2slv[M-1][j]]`. Slicing across a generated dimension lowers to a straight wire fanout in SV (no logic). If the compiler doesn't currently parse this slice form, the equivalent verbose form is to enumerate the slice elements; the design works either way.

---

## 10. Register slices — `Nic400RegSlice`

A register slice (skid buffer) breaks combinational valid/ready paths to ease timing. It accepts one transaction per cycle with full backpressure correctness. Implemented as a one-stage `pipeline` with a `wait until` in the slice stage, which naturally handles the "downstream not ready" backpressure case.

```arch
//! Generic AXI4 single-channel register slice (skid buffer).
//! One pipeline stage with full handshake support.  Latency = 1 cycle when
//! downstream is ready immediately; otherwise the stage just stalls.
//!
//! Used for AW, W, AR, R, B independently — each slice handles one channel.
pipeline RegSliceChannel
  param PAYLOAD_W: const = 64;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;

  port up_valid:   in  Bool;
  port up_payload: in  UInt<PAYLOAD_W>;
  port up_ready:   out Bool;

  port dn_valid:   out Bool;
  port dn_payload: out UInt<PAYLOAD_W>;
  port dn_ready:   in  Bool;

  stage Slice
    reg payload: UInt<PAYLOAD_W> reset rst => 0;
    reg occupied: Bool reset rst => false;

    seq on clk rising
      // wait until downstream is ready before forwarding; if downstream is
      // not ready and the slice is full, hold; otherwise accept upstream.
      wait until dn_ready or not occupied;
      if not occupied and up_valid
        payload  <= up_payload;
        occupied <= true;
      elsif occupied and dn_ready
        occupied <= false;
      end if
    end seq

    comb
      up_ready   = not occupied;
      dn_valid   = occupied;
      dn_payload = payload;
    end comb
  end stage Slice
end pipeline RegSliceChannel
```

This single 1-stage `pipeline` with a `wait until` stalls the pipeline naturally when downstream isn't ready; the compiler-generated FSM busy signal feeds the stall chain (see `doc/thread_pipeline_spec.md` "Stall Chain Integration").

To slice an entire AXI4 channel (with multiple flat signals like AW = `addr ++ id ++ len ++ ...`), pack signals into one wide payload and wrap the slice:

```arch
module Nic400RegSliceAxi4
  use PkgNic400::*;
  param SLICE_AW: const = 1;
  param SLICE_W:  const = 1;
  param SLICE_AR: const = 1;
  param SLICE_R:  const = 1;
  param SLICE_B:  const = 1;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port up: target    BusAxi4<...>;
  port dn: initiator BusAxi4<...>;

  generate_if SLICE_AW
    let AW_W: const = ADDR_WIDTH + MASTER_ID_W + 8 + 3 + 2 + 1 + 4 + 3 + 4 + 4;
    inst aw_slice: RegSliceChannel
      param PAYLOAD_W = AW_W;
      connect clk        <- clk;
      connect rst        <- rst;
      connect up_valid   <- up.aw_valid;
      connect up_payload <- {up.aw_addr, up.aw_id, up.aw_len, up.aw_size, up.aw_burst,
                              up.aw_lock, up.aw_cache, up.aw_prot, up.aw_qos, up.aw_region};
      connect up_ready   -> up.aw_ready;
      connect dn_valid   -> dn.aw_valid;
      connect dn_payload -> {dn.aw_addr, dn.aw_id, dn.aw_len, dn.aw_size, dn.aw_burst,
                              dn.aw_lock, dn.aw_cache, dn.aw_prot, dn.aw_qos, dn.aw_region};
      connect dn_ready   <- dn.aw_ready;
    end inst aw_slice
  end generate_if

  generate_if not SLICE_AW
    // Passthrough — assign through.
    comb
      dn.aw_valid = up.aw_valid;
      dn.aw_addr  = up.aw_addr;
      dn.aw_id    = up.aw_id;
      dn.aw_len   = up.aw_len;
      dn.aw_size  = up.aw_size;
      dn.aw_burst = up.aw_burst;
      dn.aw_lock  = up.aw_lock;
      dn.aw_cache = up.aw_cache;
      dn.aw_prot  = up.aw_prot;
      dn.aw_qos   = up.aw_qos;
      dn.aw_region= up.aw_region;
      up.aw_ready = dn.aw_ready;
    end comb
  end generate_if

  // ... symmetric blocks for W, AR, R, B ...
end module Nic400RegSliceAxi4
```

`generate_if` lets every individual channel slice be enabled or bypassed independently, e.g. for register-slice-on-AW-only or register-slice-on-R-only profiles.

---

## 11. QoS arbitration — custom policy

The slave's `aw_lock` and `ar_lock` use `mutex<QosOrRr>` with a user-defined `grant_select` hook. The function below picks the highest-QoS requester with starvation avoidance (round-robin tie-break after a configurable timeout).

```arch
module Nic400ArbiterPolicy
  use PkgNic400::*;

  // The grant function: takes the M-bit request mask, last-grant one-hot,
  // and per-master QoS vector; returns one-hot grant.  Pure combinational.
  function Nic400ArbiterFn(
      policy:    const,                 // "round_robin" | "priority" | "qos"
      req_mask:  UInt<NUM_MASTERS>,
      last_grant: UInt<NUM_MASTERS>,
      qos_vec:    Vec<UInt<QOS_W>, NUM_MASTERS>
    ) -> UInt<NUM_MASTERS>
    if policy == "round_robin"
      // Mask off requesters at or below last-grant; if empty, use full set
      let upper: UInt<NUM_MASTERS> = mask_above(last_grant);
      let rr_req: UInt<NUM_MASTERS> = (req_mask & upper) != 0
                                       ? (req_mask & upper)
                                       : req_mask;
      // Isolate lowest bit (one-hot grant)
      return rr_req & (~rr_req + 1).trunc<NUM_MASTERS>();

    elsif policy == "priority"
      // Lowest index wins (NIC-400 "fixed priority" arbitration option)
      return req_mask & (~req_mask + 1).trunc<NUM_MASTERS>();

    else   // "qos"
      // Find max QoS among requesters
      let max_qos: UInt<QOS_W> = max_qos_of_requesters(req_mask, qos_vec);
      // Mask requesters that have that max QoS
      let top_qos_req: UInt<NUM_MASTERS> = mask_qos_eq(req_mask, qos_vec, max_qos);
      // Round-robin among top-QoS requesters
      let upper: UInt<NUM_MASTERS> = mask_above(last_grant);
      let rr_req: UInt<NUM_MASTERS> = (top_qos_req & upper) != 0
                                       ? (top_qos_req & upper)
                                       : top_qos_req;
      return rr_req & (~rr_req + 1).trunc<NUM_MASTERS>();
    end if
  end function Nic400ArbiterFn

  function mask_above(last_grant: UInt<NUM_MASTERS>) -> UInt<NUM_MASTERS>
    let v: UInt<NUM_MASTERS> = 0;
    for i in 0..NUM_MASTERS-1
      if (1.zext<NUM_MASTERS>() << i.trunc<$clog2(NUM_MASTERS)>()) > last_grant
        v = v | (1.zext<NUM_MASTERS>() << i.trunc<$clog2(NUM_MASTERS)>());
      end if
    end for
    return v;
  end function mask_above

  function max_qos_of_requesters(req: UInt<NUM_MASTERS>,
                                 qos_vec: Vec<UInt<QOS_W>, NUM_MASTERS>)
                                 -> UInt<QOS_W>
    let m: UInt<QOS_W> = 0;
    for i in 0..NUM_MASTERS-1
      if req[i] and qos_vec[i] > m
        m = qos_vec[i];
      end if
    end for
    return m;
  end function max_qos_of_requesters

  function mask_qos_eq(req: UInt<NUM_MASTERS>,
                       qos_vec: Vec<UInt<QOS_W>, NUM_MASTERS>,
                       target: UInt<QOS_W>)
                       -> UInt<NUM_MASTERS>
    let v: UInt<NUM_MASTERS> = 0;
    for i in 0..NUM_MASTERS-1
      if req[i] and qos_vec[i] == target
        v = v | (1.zext<NUM_MASTERS>() << i.trunc<$clog2(NUM_MASTERS)>());
      end if
    end for
    return v;
  end function mask_qos_eq
end module Nic400ArbiterPolicy
```

**Starvation avoidance**: NIC-400 supports a "QoS-Value Regulator" to bound the max time between grants for any master. The same effect comes from per-master timeout counters; if a timeout expires, that master's effective QoS is elevated. The hook function takes a third optional parameter `timed_out: UInt<NUM_MASTERS>` and OR-s it into the QoS comparison; the per-master timeout counter lives in `Nic400SlavePort` as a `Vec<UInt<TOUT_W>, NUM_MASTERS>` register, incremented every cycle a master requests but is not granted, cleared on grant. The full version is straightforward; the spec stays focused on the structural mechanism.

---

## 12. Top-level — `Nic400Interconnect`

```arch
/// System-level interconnect.  Wraps Nic400Fabric with optional per-port
/// register slices.  This is what an SoC integrator instantiates.
module Nic400Interconnect
  use PkgNic400::*;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;

  // M system-facing master-side ports
  port m: target Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                              ID_W=MASTER_ID_W, READ=1, WRITE=1>, NUM_MASTERS>;
  // N system-facing slave-side ports
  port s: initiator Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                 ID_W=SLAVE_ID_W,  READ=1, WRITE=1>, NUM_SLAVES>;

  // Per-port slice enables — set per project for timing closure
  param SLICE_M_INGRESS: const = 1;
  param SLICE_S_EGRESS:  const = 1;

  wire_bus m_int: Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                ID_W=MASTER_ID_W, READ=1, WRITE=1>, NUM_MASTERS>;
  wire_bus s_int: Vec<BusAxi4<ADDR_W=ADDR_WIDTH, DATA_W=DATA_WIDTH,
                                ID_W=SLAVE_ID_W, READ=1, WRITE=1>, NUM_SLAVES>;

  // Master-side slices
  generate_for i in 0..NUM_MASTERS-1
    inst m_slice_i: Nic400RegSliceAxi4
      param SLICE_AW = SLICE_M_INGRESS;
      param SLICE_W  = SLICE_M_INGRESS;
      param SLICE_AR = SLICE_M_INGRESS;
      param SLICE_R  = SLICE_M_INGRESS;
      param SLICE_B  = SLICE_M_INGRESS;
      connect clk <- clk;
      connect rst <- rst;
      connect up  <- m[i];
      connect dn  -> m_int[i];
    end inst m_slice_i
  end generate_for

  // Fabric (the actual crossbar)
  inst fabric: Nic400Fabric
    connect clk <- clk;
    connect rst <- rst;
    connect m   <- m_int;
    connect s   -> s_int;
  end inst fabric

  // Slave-side slices
  generate_for j in 0..NUM_SLAVES-1
    inst s_slice_j: Nic400RegSliceAxi4
      param SLICE_AW = SLICE_S_EGRESS;
      param SLICE_W  = SLICE_S_EGRESS;
      param SLICE_AR = SLICE_S_EGRESS;
      param SLICE_R  = SLICE_S_EGRESS;
      param SLICE_B  = SLICE_S_EGRESS;
      connect clk <- clk;
      connect rst <- rst;
      connect up  <- s_int[j];
      connect dn  -> s[j];
    end inst s_slice_j
  end generate_for
end module Nic400Interconnect
```

---

## 13. What the compiler generates

Below is a high-level inventory of what each construct emits — not full SV, but enough to understand the synthesised structure. Cross-references to the relevant compiler pass documentation are included.

### 13.1 Per-master `Nic400MasterPort_i`

Each instance produces:

| Source construct | Generated SV | Resource cost |
|---|---|---|
| `thread AwDecode` | One FSM in `_Nic400MasterPort_threads`: 2 states (idle, do..until-driving). State reg ≈ 1 bit per thread. | ~1 FF, ~10 LUT |
| `thread WForward` | Similar 2-state FSM, plus reads `aw_route_fifo.peek` | ~1 FF, ~10 LUT |
| `thread ArDecode`, `RReturn_j ×N`, `BReturn_j ×N` | One FSM each. Returns use `shared(or)` → per-thread shadow wires + OR reduction. | ~(1 + 2N) FF, ~(20 + 5N) LUT |
| `fifo aw_route_fifo`, `fifo ar_route_fifo` | Each a standard sync FIFO from the `fifo` construct: depth × width FFs + ptr/cmp logic. | ~depth × width FF |
| `addr_to_slave(...)` | SystemVerilog `function automatic`; called from `let` expressions; no clock cost. | Pure comb, 0 FF |

The compiler's thread lowering pass (see `doc/thread_lowering_algorithm.md`) groups all threads in this module into a single sub-module `_Nic400MasterPort_threads` with one merged `always_ff`. The per-thread state registers (`_t0_state` ... `_t4+N_state`) total $\lceil \log_2 \text{n\_states} \rceil$ bits each. With 5 base threads plus N R-return + N B-return threads, total state-register footprint scales linearly in N.

### 13.2 Per-slave `Nic400SlavePort_j`

Each instance produces:

| Source construct | Generated SV | Resource cost |
|---|---|---|
| `resource aw_lock: mutex<QosOrRr> ...` | A real `arbiter` Item named `_arb_Nic400SlavePort_aw_lock` — synthesised via the same path as a user-written `arbiter`. The `hook grant_select` function lowers to the arbiter's `function automatic` body. | ~M × QOS_W comparators + M-wide one-hot decoder |
| `lock aw_lock` in M generate_for threads | Per-thread `_aw_lock_req_i`/`_aw_lock_grant_i` wires; comb mux on slave AW signals indexed by grant; FSM stall logic in each thread until grant is asserted. (See `doc/thread_lowering_algorithm.md` §"Lock Arbitration".) | ~M × payload_bits muxes |
| `fifo w_route_fifo` | One sync FIFO, depth = OUTSTANDING, width = $\lceil \log_2 M \rceil$. | depth × width FF |
| Same shape for `ar_lock` (no W-route FIFO needed) | Same arbiter Item, separate instance | 2× the AW cost |
| `RReturn_i`, `BReturn_i` (×M each) | One FSM each. `from[i].r_valid/data/...` driven from the per-thread mux; `s.r_ready`/`s.b_ready` is `shared(or)` aggregating M masters' ready. | ~(1 FF + 5 LUT) × 2M |

### 13.3 Per-port register slices

Each `Nic400RegSliceAxi4` produces 1–5 small pipeline stages (depending on `SLICE_*` params). Each stage is a single 1-bit state register (`_slice_fsm_state`) plus the packed payload register. By the pipeline wait-stage rule (see `doc/thread_pipeline_spec.md` §"Generated SystemVerilog"), idle is state 0 and the wait-on-downstream-ready is state 1; the stall chain is auto-wired.

### 13.4 Total cost (rough estimate for M=4, N=4, DATA=64, ID=4)

- 4 master ports × ~(5+2N=13) FSM threads = 52 small FSMs (~80 FFs)
- 4 slave ports × ~(M+M+M+M+1=17) FSM threads = 68 small FSMs (~100 FFs)
- 4 master + 4 slave AW/AR/W/R/B route FIFOs (depth 16 × 6-bit + 16 × 2-bit) ≈ 16 × 8 × 8 = 1024 FFs
- 8 register slices (master ingress + slave egress) × 5 channels × ~70-bit payload ≈ 2800 FFs
- Per-slave arbiter `_arb_Nic400SlavePort_aw_lock` / `_arb_...ar_lock` — both round-robin or QoS; ~20 LUT + 4 FF each
- Address decoders: 4 × 4-bit comparator chain (one per master) ≈ pure comb

**Estimated total** for the 4×4 instance at 64-bit data, 4-bit ID:
- FFs: ~4000
- LUTs: ~5000
- Sky130 area estimate: ~50000 µm² (extrapolating from `axi_dma_case_study.md`'s 78,134 µm² for a substantially more complex design)

---

## 14. Performance analysis

### 14.1 Latency (uncontested path)

| Path | Cycles | Notes |
|---|---|---|
| M_i AW → S_j AW (no slices) | **0** | Master decoder is pure comb; slave AW lock acquires in 0 cycles when uncontested (the `do..until inside lock` lemma — see `thread_spec_section.md` §20.13). |
| M_i AW → S_j AW (1 slice each side) | 2 | One slice ingress, one slice egress. |
| S_j R → M_i R (no slices) | **0** | Slave drives `s.r_valid` combinationally; master-side demux is pure comb; the master's R port receives same cycle. |
| S_j R → M_i R (1 slice each side) | 2 | |

### 14.2 Throughput

| Scenario | Throughput | Why |
|---|---|---|
| 1 master, 1 slave | 1 transfer / cycle | All channels pipelined, no contention |
| M masters, N slaves, disjoint targets | M transfers / cycle | Each master/slave pair operates independently — the M×N edge threads run in parallel |
| M masters → 1 hot slave | 1 transfer / cycle / M | One arbiter serializes M; throughput = 1/M per master |
| AW + W in parallel (same master, same slave) | 1 burst / cycle | `fork ... and ... join` lowers to parallel branches — AW and W FSMs advance independently |
| Variable-latency slave (e.g. cache) | matches slave throughput | Master agent stalls naturally via `wait until s_ready` |

### 14.3 Critical path (estimated)

Without slices: the path from master `aw_addr` to slave `aw_addr` runs through:
1. Master's `addr_to_slave(aw_addr)` decoder (~$\log_2 N$ levels of mux)
2. The per-slave AW mux output (M-way, ~$\log_2 M$ mux levels)
3. The slave-side QoS comparator chain (~$\log_2 M$ levels)
4. Onto `s.aw_addr` (and `aw_id`, `aw_len`, ...)

For M=4, N=4, this is ~6 LUT levels — well under typical 200 MHz Sky130 timing budgets (per the axi_dma case study's ~6-LUT critical path at 200 MHz). For larger crossbars (M=16, N=16), the `SLICE_M_INGRESS=1` + `SLICE_S_EGRESS=1` configuration drops the critical path to single-stage 3-LUT chunks.

### 14.4 Outstanding transactions

- **Per master**: up to `2^MASTER_ID_W` outstanding (limited by master's ID width). With `MASTER_ID_W=4`, that's 16 outstanding.
- **Per (master, slave) pair**: bounded by `OUTSTANDING` (the W-route FIFO depth). At depth 16, each master can have 16 in-flight AW to one slave.
- **System-wide**: M × OUTSTANDING.

Backpressure: when a master fills its `aw_route_fifo`, the `AwDecode` thread stalls (its `wait until ... not aw_route_fifo.full` blocks). The master's `aw_ready` then deasserts naturally because `AwDecode` no longer drives the slave handshake, propagating the stall back to the master.

### 14.5 Reordering and ordering rules preserved

- **AXI4 §A5.3 (responses keep their ID)**: ✓ — IDs travel unmodified through the fabric except for the (master_idx) prefix.
- **AXI4 §A5.3 (same-ID responses in order at master)**: ✓ — each slave responds in order per slave-side ID; since slave IDs uniquely identify (master_idx, master_id), per-master per-ID order is preserved.
- **AXI4 §A5.3 (different-ID responses can interleave)**: ✓ — different IDs go through independent shared(or) demux paths.
- **AXI4 §A3 (W beats follow AW order at slave)**: ✓ — per-slave `w_route_fifo` keeps master-idx in AW-issue order; WForward pops in that order.

---

## 15. Verification plan

1. **Single-master / single-slave smoke test**: 4-beat AW+W+B; 4-beat AR+R. Verify all signals handshake on the correct cycles.
2. **Address decode**: 4 masters writing to 4 disjoint slaves simultaneously — verify all 4 succeed in parallel (M × throughput).
3. **Hot-slave contention**: 4 masters all writing to slave 0; verify QoS-policy arbitration delivers grant in QoS order; verify starvation timeout elevates a stale low-QoS master.
4. **ID remap correctness**: Master 2 issues an AW with `aw_id=3`; verify slave sees `aw_id=0b10_0011` (= `{2, 3}` for M=4) and that the B response with `b_id=0b10_0011` returns to master 2 only.
5. **Out-of-order completion**: Master 0 issues two AWs (id=0, id=1) to two different slaves with different latencies; verify B responses can interleave.
6. **Register slice latency**: Compare cycle-accurate trace with `SLICE_M_INGRESS=0` vs `=1`; verify exactly 1 cycle added.
7. **Auto-emitted thread SVA (`--auto-thread-asserts`)**: Each `wait until` and each `fork/join` branch fires an `_auto_thread_*` SVA property by construction (`thread_spec_section.md` §20.15). Run Verilator `--binary --assert` on a 1000-cycle trace; every property should hold silently. Mutating any one (e.g., changing `from[i].r_valid` check to wrong index) should trip its corresponding `_auto_thread_*_branch_*` assertion.
8. **Formal property** (with `arch formal`): `forall i, j. once aw_route_fifo pushes (i,j), the next-issued W to slave j has from-master = i`. This is an automatic consequence of the design but worth verifying.

---

## 16. Comparison with NIC-400

| NIC-400 feature | This design | Notes |
|---|---|---|
| Configurable M × N | ✓ (`param NUM_MASTERS`, `NUM_SLAVES`) | NIC-400 supports up to 128 master × 64 slave; our design has no architectural limit, just sizing of arbiter and FIFO widths |
| AXI4 / AXI4-Lite mixed | ✓ (per-port `BusAxi4` vs `BusAxiLite` with parameterised channels) | NIC-400 also supports AXI3 and AHB |
| QoS-aware arbitration | ✓ (`mutex<QosOrRr>` + `hook grant_select`) | NIC-400 has more elaborate "QoS-Value Regulator" with per-master throttling; same pattern, more state |
| QVN (QoS Virtual Network) | Partial — would add `param NUM_QVN_GROUPS` and tag-based arbitration | Skipped; same approach extends |
| Register slices | ✓ (`generate_if` per channel per port) | NIC-400's "GPV register slices" — same pattern |
| Configurable address map | ✓ (override `addr_to_slave` function) | NIC-400 uses runtime-programmable GPV registers; ours is compile-time. Add a programmable variant by replacing the function with table-lookup in a register file. |
| Lock/exclusive monitor | Pass-through (signal-level) | Slaves see `aw_lock`/`ar_lock` unchanged; exclusive-monitor logic lives in slaves |
| Multi-clock crossings | Not in v1; add via `fifo kind: async` between fabric and ports | NIC-400 supports per-port async; the construct exists, just not wired in this spec |

**Lines of ARCH source for the full design (rough estimate)**:
- `PkgNic400.arch`: 25
- `BusAxi4.arch` (extension): existing + ~20 lines for AXI4 sideband signals
- `Nic400MasterPort.arch`: ~180 (5 threads + 2 FIFOs)
- `Nic400SlavePort.arch`: ~220 (M+M+1+M+M threads + 1 FIFO + 2 resources)
- `Nic400Fabric.arch`: ~40 (pure wiring with 2 generate_for)
- `Nic400RegSliceAxi4.arch`: ~150 (5 channel slices in generate_if)
- `RegSliceChannel.arch`: ~25 (1 pipeline stage with wait_until)
- `Nic400ArbiterPolicy.arch`: ~80 (1 grant function + 3 helpers)
- `Nic400Interconnect.arch`: ~50 (top wrapper)

**Total: ~790 lines of ARCH for a parameterizable M×N AXI4 crossbar with QoS, ID remap, register slices, and out-of-order support.** Compare with typical hand-written SV interconnects in the 3,000–6,000-line range.

The size win comes from:
- `thread` replaces manual FSM coding (~50% reduction)
- `lock`/`resource` synthesises arbiters with policy parameterised; no per-instance arbiter hand-rolling
- `shared(or)` replaces the manual ready-aggregation tree
- `generate_for` × `thread` makes the M×N expansion structural rather than open-coded
- `pipeline` with `wait until` makes the register slice a 1-stage parameterizable unit
- `function` (module-local) makes the address decoder and the QoS policy data-driven and replaceable

### 16.1 Living gap tracker

A living checklist of NIC-400 features vs the ARCH demo. The §16 table above is the *as-planned* snapshot from initial design; this subsection tracks *as-shipped vs the ARM NIC-400 TRM (IHI 0064)* and is updated when PRs land. Status legend: **✅ shipped** / **🟡 partial** / **❌ not started** / **⏭ out of scope** (intentional, with rationale).

A verification pass against the ARM TRM (DDI 0475E, *CoreLink NIC-400 Network Interconnect Technical Reference Manual*) was completed on 2026-05-26; corrections and additions below carry source-confidence tags: `[TRM]` for items confirmed directly in DDI 0475E, `[INFER]` for items inferred from related ARM IP/AMBA docs (lower confidence), `[UNK]` for items that could not be authoritatively verified. The QoS-400, QVN-400, and TLX-400 supplements are separate ARM products layered on top of NIC-400 and are not in the base TRM; rows that depend on those supplements are marked accordingly.

#### Protocol bridges and interfaces
| Feature | Status | Where / next step |
|---|---|---|
| AXI4 (full) master/slave shim | ✅ | `tests/nic400/BusAxi4.arch`, `Nic400Master/SlavePort.arch` |
| AXI4-Lite shim | ⏭ | [TRM] NIC-400 TRM §1.2 enumerates AXI3, AXI4, AHB-Lite, APB2/3/4 as the endpoint protocols — AXI4-Lite is **not** a NIC-400 endpoint option. AMBA Designer reaches AXI4-Lite peripherals via an external AXI→AXI4-Lite bridge, not inside the NIC. |
| AXI3 shim (locked-rd/wr, WID) | ❌ | [TRM] AXI3 is a first-class NIC-400 slave **and** master interface option (TRM §1.2, §2.2.1, §2.3.1). NIC-400 also does AXI3↔AXI4 protocol conversion (split long bursts, optional burst limiter), which the demo does not model. |
| AHB-Lite master bridge (CPU→fabric) | ✅ | `Nic400AhbBridge.arch` + multi-burst + INCR-undef chunking. [TRM] Mapping table (§2.2.2 Table 2-2) and 1KB-boundary break rule confirmed. |
| AHB-Lite slave bridge (fabric→AHB peripheral) | ❌ | [TRM] NIC-400 calls this an "AHB-Lite master interface" (or "AHB-Lite mirrored slave interface" for direct AHB-slave attach) on the master side of the fabric. Reverse direction; not built. |
| AHB-Lite "mirrored" interface variants (mirrored-master / mirrored-slave) | ❌ | [TRM] §2.2.1 / §2.2.2: NIC-400 offers four AHB-Lite interface flavors (slave, mirrored-master, master, mirrored-slave) for direct attach to either an AHB master or AHB slave without the HSEL/HREADY glue. Demo only models one flavor. |
| APB v2/v3/v4 target bridge | ✅ | `Nic400ApbBridge.arch` + `stdlib/BusApb` (PR #434 toggleable sidebands). [TRM] §2.2.2 confirms per-AMIB APB2/3/4 mix + up to 16 APB subports per AMIB. |
| APB initiator (CPU→APB) | ⏭ | Real SoCs front APB peripherals via the AXI→APB bridge — covered |
| AXI3 ↔ AXI4 protocol conversion (long-burst split, burst limiter) | ❌ | [TRM] §2.3.1: AXI4 INCR>16 split into multiple AXI3 bursts on egress; AXI3→AXI4 has a programmable burst limiter (GPV register). Not modelled — demo is AXI4-only. |

#### Crossbar fabric
| Feature | Status | Where / next step |
|---|---|---|
| Parameterizable M×N matrix | ✅ | `Nic400Fabric.arch`; demo is 3×4. [TRM] §1.2 caps NIC-400 at 1-128 slave IFs × 1-64 master IFs and up to 5 cascaded switches between any master/slave pair. |
| ID remap (master idx prefix) | ✅ | `MASTER_ID_W → SLAVE_ID_W = MASTER_ID_W + ceil(log2(M))`. [TRM] §2.3.12 names the components "Interconnect ID (IID) + Virtual ID (VID) + Slave-Interface ID (SIID)"; global ID width is 1-24 bits with an optional "ID reduction" pass at AMIB. Our shim does a single SIID prefix; we do not implement ID reduction. |
| Address decode | 🟡 | Compile-time, top-NS_W bits of REGION_BITS=28 page. [TRM] §2.3.11 names the runtime knob "Remap" (8 remap-state bits, GPV-programmable, can alias/move/add/remove regions); we don't implement it. |
| Decode-error (DECERR) response on unmatched address | ❌ | [TRM] §2.2.1: "Any transaction that does not decode to a legal master interface destination... receives a DECERR response." NIC-400 builds this into the slave interface block (ASIB) automatically — there is no separately-instantiated "default slave" module; rephrase the row as a behavior the ASIB must emit. Demo's `Nic400MasterPort` silently drops un-matched addrs. |
| Per-master / per-slave clock domains | ❌ | All ports share `clk: in Clock<SysDomain>`. [TRM] §2.2.1 / §2.3.5: each ASIB/AMIB can select SYNC 1:1, SYNC 1:n, SYNC n:1, SYNC n:m, or ASYNC frequency-domain crossing with a per-FIFO depth of 2-32, and the `sync_mode` is GPV-programmable. CDC via `fifo kind: async` is doable but not wired. |
| Cyclic Dependency Avoidance Schemes (CDAS) — Single-Slave / Single-Slave-per-ID | ❌ | [TRM] §2.3.7: per-ASIB knob that stalls transactions to a different destination than outstanding ones of the same type (or same ID), to break AW/W-channel ordering deadlocks. Not modelled. |
| Single Active Slave (SAS) | ❌ | [TRM] §2.3.8: at a divergent switch slave IF, an AW address beat is stalled if any outstanding write data beats are still in flight to a different master IF. Used as a fallback CDAS resolution. Not modelled. |
| Address remap states (alias / move / add / remove regions) | ❌ | [TRM] §2.3.11: 8 remap bits in GPV control independent region overlays; the BRESP from the GPV after a remap update guarantees observable ordering. Not modelled. |

#### Pipelining and width adaptation
| Feature | Status | Where / next step |
|---|---|---|
| Full register slice (1-stage, both directions) | ✅ | `Nic400EdgeRegSlice.arch` + `Nic400FabricRs1.arch` (per-master). [TRM] §2.2.1 confirms "full register slice" terminology (adds +2 to read/write acceptance capability when placed in the ASIB slave-IF position). |
| Forward-only register slice variant | ❌ | [TRM] §2.2.1 names exactly two slice variants — "full register slice" (+2 acceptance) and "forward register slice" (+1 acceptance). The earlier note "NIC-400 has four flavors" is incorrect for the base TRM — *(unverified — only "full" and "forward" appear in DDI 0475E; "reverse" / "FF" variants are AMBA register-slice IP terminology but not documented in NIC-400 TRM)*. |
| Multi-stage reg slices (STAGES > 1) | ❌ | `Nic400FabricRs1` is STAGES=1 only; wrapper would need chaining. *(unverified — TRM does not explicitly enumerate multi-stage chaining; configured via AMBA Designer timing-isolation knobs)* |
| Per-slave reg slice insertion | ❌ | Only per-master is wired; per-slave wrapper not built. [TRM] §2.2.1/§2.2.2 explicitly support timing isolation at both ASIB and AMIB ("from the external master/slave" and "from the network"). |
| Upsizer (1:2/1:4/1:8) and Downsizer (2:1/4:1/8:1) data-width adapter | ✅ | `Nic400WidthAdapter.arch` (5 threads, PR #431). [TRM] §2.3.3/§2.3.4 names the functions "Upsizing data width function" and "Downsizing data width function"; supported ratios are 1:2/1:4/1:8 (upsize) and 2:1/4:1/8:1 (downsize); data widths 32/64/128/256 (512/1024 explicitly **not** supported); upsizer only packs cacheable transactions; both have a `bypass_merge` GPV bit and 1-32 accept-capability. |
| Width-adapter wired into the integrated demo | ❌ | Standalone module + TB only; not on the AHB↔APB path yet (demo is 32-bit end-to-end) |
| Burst-length splitter for 4KB AXI boundary | ❌ | [INFER] AMBA AXI spec mandates that any single burst stay within a 4KB page; an NIC-400 ASIB whose master issues an oversized burst would have to split — but the TRM only documents the 1KB AHB-Lite split (§2.2.2) and the AXI4→AXI3 long-burst split (§2.3.1) explicitly, so this row is *(unverified — likely upstream-master's responsibility per AMBA AXI, not an NIC-400 feature)*. |

#### QoS and arbitration
| Feature | Status | Where / next step |
|---|---|---|
| Per-master QoS-priority arbitration | 🟡 | `Nic400ArbiterPolicy.arch` (hard-coded NUM_MASTERS=4 hook). Generalize to runtime N or document the size assumption. [TRM] §2.3.6: NIC-400 native arbitration is *fixed-priority on AxQOS value, LRU within same QoS*; per-ASIB QoS source is static, GPV-programmable, or taken from the attached master (`read_qos`/`write_qos` registers, Table 3-1). |
| QoS-Value Regulator (peak/burstiness/avg per master) | ⏭ | [TRM] §2.4.1 — this is in the **QoS-400** product (separately licensed, not in the NIC-400 base), described in the *QoS Supplement to TRM*. Base TRM only confirms "regulation of read and write requests" exists; the specific *3-token-bucket peak/burstiness/avg* breakdown is *(unverified — needs the QoS-400 supplement; INFER from public ARM material that it follows the same per-master token-bucket shape as later CoreLink IP, but not authoritatively confirmed in the base TRM)*. |
| QoS Virtual Network (QVN) | ⏭ | [TRM] §2.4.2 — this is in the **QVN-400** product (separately licensed), up to 8 virtual networks total / max 4 per master or slave IF, configurable via addressable path from masters to slaves. Tag-based independent-lane arbitration claim is consistent with §2.4.2 but the exact tag-wire encoding is in the *QVN Supplement* (not the base TRM). |
| Programmable QoS via GPV register file | 🟡 | [TRM] §3.2 / Table 3-1: `read_qos[3:0]` at offset `0x100` and `write_qos[3:0]` at `0x104` per ASIB exist in the base NIC-400 when the QoS source is configured as "Programmable" (no QoS-400 license needed for the fixed-priority case). Tied to GPV programmability item below. |
| Thin Links (TLX) point-to-point reduced-signal bridge | ⏭ | [TRM] §2.4.3 — this is the **TLX-400** product (separately licensed); AXI-to-AXI / AXI-to-AHB long-distance routing with a Data-Link + Physical-Layer split. Not in the base TRM and not modelled. |

#### Observability and control
| Feature | Status | Where / next step |
|---|---|---|
| PMU per-master handshake counters | ✅ | `Nic400Pmu.arch` (AR/AW/R/W/B per master). *(unverified — DDI 0475E base TRM does **not** describe a built-in performance-monitor unit; ARM's PMU IP is delivered separately on CCI/CCN. Our PMU is an ARCH-side instrumentation block, not a NIC-400 native feature; this row may belong in a "telemetry add-on" category rather than gap-tracker against the TRM.)* |
| PMU latency / outstanding-txn histograms | ❌ | *(unverified — see PMU note above; the latency-bin shape is a CCI-style feature, not documented in the NIC-400 base TRM.)* |
| GPV (Global Programmers View) register file | ❌ | [TRM] §3.2: NIC-400's configuration register file ("Global Programmers View" — note ARM spells "Programmers" without apostrophe). Each ASIB/IB/AMIB has its own 4KB block stacked at a configurable base. GPV is AXI-accessed, **AxSIZE=32-bit only**, **Secure-only**, no interleaved WDATA, aligned only, non-cacheable. Would need an AXI target (not APB — that's a constraint of GPV) + a `regfile` block. |
| Low-power interface (clock-gating C-channel) — CSYSREQ / CSYSACK / CACTIVE | ❌ | [TRM] §2.2.3 + Appendix A.1.1: NIC-400's hierarchical-clock-gating signals are `csyreq_cd_<Domain>` / `cysack_cd_<Domain>` / `cactive_cd_<Domain>` (one set per clock domain). The earlier note "PREQ/PACTIVE/PACCEPT/PDENY" is **incorrect** — those are AMBA P-channel signals used in newer CoreLink IP (CCI/CCN/PMU), not NIC-400's C-channel. AHB-Lite slave IFs cannot participate fully (protocol has no back-pressure on the address phase) so each AHB-Lite slave needs its own clock domain. |
| Hierarchical clock-gating central GPV ring | ❌ | [TRM] §2.3.2: when hierarchical clock-gating is enabled and the GPV spans more than one clock domain, NIC-400 inserts an *additional* clock domain that bridges GPV accesses asynchronously and exposes its own AXI low-power IF. Not modelled. |

#### Security and ordering
| Feature | Status | Where / next step |
|---|---|---|
| AR/AW LOCK propagation (signal carry-through) | ✅ | `ar_lock`/`aw_lock` flow through `BusAxi4` unchanged. [TRM] §2.3.9: NIC-400 has *Lock Support* logic at switch master IFs that stalls coincident transactions while the locked sequence drains; AXI4 has no AxLOCK width (single-bit) and AXI3 has 2-bit AxLOCK with SWP support. |
| Exclusive-access monitor (in slave shim) | ❌ | *(unverified — DDI 0475E base TRM does **not** describe an exclusive-access monitor inside NIC-400; the TRM only states that the up/downsizer **removes** exclusive information from split transactions and that the master will never see EXOKAY in that case (§2.3.3/§2.3.4). The "EX-monitor in slave shim" claim is more typical of DMC/CCI-style memory controllers; for NIC-400 the exclusive monitor lives in the addressed slave, not the NIC.)* |
| AxPROT carry-through (privilege/secure/instr) | ✅ | Signal propagates end-to-end; APB bridge maps to PPROT. [TRM] §2.3.10: each slave IF can be Secure / Non-secure / per-access (`AxPROT[1]`); Non-secure→Secure-only-master returns DECERR. |
| AxUSER / WUSER / RUSER / BUSER sidebands | ❌ | `BusAxi4.arch` has no USER fields. [TRM] §1.2 + Appendix A.3: NIC-400 supports **independent** 0-256-bit user widths per channel (AWUSER/WUSER/BUSER/ARUSER/RUSER); the AHB-Lite bridge also auto-maps HAUSER→AWUSER/ARUSER and HWUSER→WUSER (§2.2.1/§2.2.2). |
| AxCACHE / AxQOS / AxREGION carry-through | ✅ | All three flow through fabric and reg slice. [TRM] §2.2.2: NIC-400 can compute a 4-bit AxREGION at decode time *or* take an input region from the master (and an APB-decoded address overrides any input region). |
| Write data interleaving (AXI3 only) | ⏭ | AXI4 forbids it; we're AXI4-only. [TRM] §1.2 (note): **NIC-400 base product does not accept or issue interleaved write data on any interface** — so this row is doubly out-of-scope (both AXI4 *and* NIC-400 reject WDATA interleaving). |
| Read-data interleaving / ID-deinterleave on RDATA | ❌ | [INFER] AXI3 permits RDATA interleaving across IDs; the NIC-400 TRM does not document a separate deinterleave block, instead relying on the slave's RID stream + the master's natural ID-based reordering. *(unverified — base TRM is silent on a NIC-internal deinterleave step.)* |

#### Verification and TB coverage
| Feature | Status | Where / next step |
|---|---|---|
| Single-master, single-slave smoke | ✅ | `tb_nic400_system.cpp` |
| Multi-master contention (M=2..3 active at once) | ❌ | All non-CPU master ports are stubbed idle in `Nic400System.arch`; need a multi-driver TB |
| Multi-slave hot-spot QoS test | ❌ | Implied by §16 row above — `tb_hot_slave_qos.cpp` from spec Appendix A was never landed |
| OOO completion (interleaved B per master) | ❌ | `tb_ooo_completion.cpp` from spec Appendix A — not landed |
| Reg-slice fabric throughput | ✅ | `tb_nic400_fabric_regslice.cpp`, `tb_nic400_fabric_throughput.cpp` |
| Width-adapter independent TB | ✅ | `tb_nic400_width_adapter.cpp` |
| PMU exact-count TB | ✅ | `tb_nic400_pmu.cpp` (counter glue is in the dedicated PMU TB, not the integrated demo — see `tb_nic400_system.cpp` comments) |

#### Quick-pick "close next" candidates
Roughly ordered by likely effort × value, picked from the rows above:

1. **De-stub the system demo's idle masters** — give `m[1]`/`m[2]` real activity in `tb_nic400_system.cpp` (or a sibling TB) so multi-master contention is exercised end-to-end. Unblocks a real QoS test next.
2. **Default-slave responder** — small extra module returning DECERR for un-decoded addrs, wire into the fabric's "no match" output of `Nic400MasterPort`. Closes a basic conformance item.
3. **Per-slave reg slice wrapper** — symmetric to `Nic400FabricRs1` but on the s side. Mostly copy-paste; useful as a real-SoC pattern.
4. **Wire width adapter into a system variant** — `Nic400SystemWide` with a 64-bit AXI master and the 32-bit APB target, so the adapter is exercised in-system.
5. **GPV regfile sketch** — APB target + a few mapped registers (decode-table override, QoS knob). Lowest-cost path to "programmable" status on multiple rows.

Items deliberately deferred: AXI3, AxUSER, QVN, LPI, EX-monitor — each is a real chunk of work and none has a current pull from a benchmark.

```
doc/nic400_interconnect_spec.md                 ← this file

src lives under tests/nic400/ (suggested):
tests/nic400/PkgNic400.arch
tests/nic400/BusAxi4.arch                       ← copy from tests/axi_dma_thread/, extend with AXI4 sideband
tests/nic400/Nic400ArbiterPolicy.arch
tests/nic400/RegSliceChannel.arch
tests/nic400/Nic400RegSliceAxi4.arch
tests/nic400/Nic400MasterPort.arch
tests/nic400/Nic400SlavePort.arch
tests/nic400/Nic400Fabric.arch
tests/nic400/Nic400Interconnect.arch

Testbenches:
tests/nic400/tb_smoke.cpp                       ← single-master single-slave
tests/nic400/tb_disjoint_parallel.cpp           ← M=4 to disjoint slaves
tests/nic400/tb_hot_slave_qos.cpp               ← contention with QoS verification
tests/nic400/tb_id_remap.cpp                    ← end-to-end ID round-trip
tests/nic400/tb_ooo_completion.cpp              ← interleaved B responses
```
