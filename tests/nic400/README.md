# NIC-400 Interconnect — ARCH Implementation Status

Reference spec: [`doc/nic400_interconnect_spec.md`](../../doc/nic400_interconnect_spec.md)

## Implementation summary

This directory contains a TDD-built NIC-400-style AXI4 read crossbar in ARCH,
exercising the four core mechanisms from the spec: address decode, per-slave
arbitration, ID remap, and ID-prefix return routing.

| File | Purpose | Status |
|---|---|---|
| `PkgNic400.arch` | Compile-time parameters (NUM_MASTERS, ID widths, REGION_BITS) | ✓ check |
| `BusAxi4.arch` | AXI4-full bus type with sideband (lock/cache/prot/qos/region) | ✓ check |
| `RegSliceChannel.arch` | Generic 1-stage register slice (skid buffer) | ✓ sim PASS |
| `Nic400ArbiterPolicy.arch` | QoS arbiter (4-requester) with custom hook | ✓ check |
| `Nic400QosFn.arch` | Pure-comb wrapper of QoS pick function for unit testing | ✓ sim PASS (7/7 cases) |
| `Nic400Read2x2.arch` | Monolithic 2x2 AXI4 read crossbar (v1) | ✓ sim PASS |
| `Nic400MasterPort.arch` | Per-master decode + route (v2, spec §7) | ✓ check + Verilator clean |
| `Nic400SlavePort.arch` | Per-slave arbitration + return (v2, spec §8) | ✓ check + Verilator clean |
| `Nic400Fabric.arch` | Hierarchical wiring harness — M MasterPort × N SlavePort (v2, spec §9) | ✓ check + arch sim PASS + Verilator clean |

## Verification — §15 of spec

| # | Test | Testbench | Status |
|---|---|---|---|
| 1 | Single master / single slave smoke | `tb_nic400_read2x2_smoke.cpp` + `Nic400Read2x2_smoke.harc` | ✓ PASS (M0→S0, M0→S1, M1→S0 routings) |
| 2 | Parallel disjoint targets | `tb_nic400_read2x2_parallel.cpp` | ✓ PASS (M0→S0, M1→S1 simultaneously) |
| 3 | Hot-slave contention arbitration | `tb_nic400_read2x2_hot_slave.cpp` | ✓ PASS (round_robin serializes both masters onto S0) |
| 4 | ID remap correctness | embedded in smoke + hot-slave | ✓ PASS (prefix=0 for M0, prefix=1 for M1, strip on return) |
| 5 | Out-of-order completion | `tb_nic400_read2x2_ooo.cpp` | ✓ PASS (R from S1 first, then S0; both land at M0 with correct IDs) |
| 6 | Register slice latency | `tb_reg_slice_channel.cpp` | ✓ PASS (1-cycle latency, sustained 1/cycle throughput, backpressure-correct) |
| 7 | `--auto-thread-asserts` runs silently | smoke TB with the flag | ✓ PASS (32 SVA properties; Verilator `--lint-only --assert` clean) |
| **+** | **v2 latency / throughput** | `tb_nic400_fabric_latency.cpp` | ✓ PASS — pins AR=0 cyc, R=0 cyc, 1 txn / cyc (9/9) |
| 8 | Formal property (per-slave issue→W order) | `arch formal` | △ DEFERRED — hierarchical formal v1 does not yet support sub-module `wire` declarations introduced by the lock-arbitration lowering pass (compiler limitation, not a design flaw) |

## Performance findings — v2 hierarchical design

Measured with `tb_nic400_fabric_latency.cpp` (runs under `arch sim`):

| Path | Spec §14.1 target | Observed (Mealy) | Observed (Moore) |
|---|---|---|---|
| AR forward (M → S, uncontested) | 0 cycles | **0 cycles ✓** | 1 cycle |
| R return (S → M, uncontested)   | 0 cycles | **0 cycles ✓** | 1 cycle |
| AR throughput (back-to-back)    | 1 txn / cycle | **1.00 t/c** (9/9) | 0.32 t/c (8/25) |

The MasterPort/SlavePort threads use the **`wait 0+ cycle until X;`** form (Mealy-style wait), which fuses with the immediately-following `do BODY until Y;` into a single state whose comb drives are gated by `X` and whose transition guard is `X && Y`. When both conditions hold at the same posedge, the thread progresses with zero added cycles — collapsing the entry-wait bubble that the standard `wait until` form imposes.

The standard `wait until X;` (Moore-style, ≥1 cycle) is also supported and is what the v1 monolithic design used; the comparison numbers above are from a quick rebuild against the Moore form.

The checker `tb_nic400_fabric_latency.cpp` pins the *observed* values and fails loudly if a future change inflates them.

### Inspecting the bubble

```bash
# Re-generate the VCD
arch sim --wave tests/nic400/fab_latency.vcd \
  tests/nic400/Nic400Fabric.arch tests/nic400/Nic400MasterPort.arch \
  tests/nic400/Nic400SlavePort.arch tests/nic400/BusAxi4.arch \
  --tb tests/nic400/tb_nic400_fabric_latency.cpp -o /tmp/fab_wave

# Open in a viewer
gtkwave tests/nic400/fab_latency.vcd        # or: surfer tests/nic400/fab_latency.vcd

# Or print a compact rising-edge sample table that pinpoints the bubble:
tests/nic400/probe_ar_bubble.sh tests/nic400/fab_latency.vcd
```

Sample output of the bubble probe (`fab_latency.vcd` captured from the
checked-in TB; M = master 0, S = slave 0):

```
rising t=13  M.ar_v=0  M.ar_r=0  M.ar_id=000    S.ar_v=0  S.ar_r=0  S.ar_id=0000
rising t=15  M.ar_v=1  M.ar_r=0  M.ar_id=001    S.ar_v=0  S.ar_r=1  S.ar_id=0000   ← TB drives request + slave already ready
rising t=17  M.ar_v=1  M.ar_r=1  M.ar_id=001    S.ar_v=1  S.ar_r=1  S.ar_id=0001   ← 1 cycle later, slave-side AR finally up + handshake
rising t=19  M.ar_v=0  M.ar_r=0  M.ar_id=001    S.ar_v=1  S.ar_r=0  S.ar_id=0001   ← TB cleanup
```

The gap between `t=15` (master + slave both prepared) and `t=17` (slave-side `ar_valid` finally high) is the 1-cycle bubble. ID `b0001` confirms the prefix encoding (`{master_idx=0, master_id=001}`).

## Scope notes — deviations from the spec

The spec describes a parameterizable M×N crossbar with separate
`MasterPort`/`SlavePort`/`Fabric`/`Interconnect` modules. The implementation
here is **a monolithic 2×2 read-only crossbar**:

- **2×2 instead of 4×4**: the spec patterns repeat mechanically; scaling
  involves enumerating more threads and widening the ID prefix. The 2×2
  exercises every core mechanism (decode, arbitrate, ID remap, return route).
- **Read-only**: the write path (AW arbitration, W routing by master-idx FIFO,
  B return) is a structural mirror of the read path. AR→R is enough to
  validate the design pattern.
- **Monolithic, not hierarchical**: ARCH does not currently support
  `Vec<BusName, N>` port arrays at module signatures, which the spec relies on
  heavily. The monolithic form sidesteps this by enumerating ports flatly.
  When the language adds bus arrays, the design can be refactored into the
  spec's hierarchical form without changing the threads.
- **Round-robin arbitration**: `resource ... : mutex<round_robin>` is what the
  compiler currently accepts. The QoS pick function (`Nic400QosFn`) is
  implemented and unit-tested in isolation; integration into the crossbar
  requires either `mutex<UserFn>` support or wrapping the QoS arbiter as a
  separate `arbiter` instance.
- **No register slices in the 2×2**: `RegSliceChannel` is a tested building
  block. Slices are inserted at top-level by instantiating it between the
  master/slave ports and the crossbar; not done here to keep the core
  verification surface focused.

## Compiler features used / probed

Working:
- `package` with `param`, `domain`, `enum`, `struct`, module-local `function`
- `resource X: mutex<round_robin>` / `mutex<priority>` + `lock X ... end lock`
- `thread T on clk rising, rst low ... default comb ... wait until ... do..until`
- `fork/and/join` (not used here but available)
- `generate_for` at module scope for threads
- `shared(or)` on individual ports
- `arbiter` construct with `policy <FnName>` and `hook grant_select`
- `--auto-thread-asserts` emits 32 SVA properties on this design

Tested missing / not-real syntactic forms (driving the monolithic structure above):
- **`Vec<BusName, N>` port** — parse error: `unexpected token: expected identifier, found Vec` in port type position. No syntactic form for an array of bus ports works today; `generate_for i / port name_i: initiator B` is also rejected ("'port' declarations are not allowed inside generate_for"). Together this blocks declaring N bus-typed edges at a module signature.
- **`with <bus_signal> shared(or)` annotation on bus ports** — parse error. Worked around by flattening to individual ports each carrying `shared(or)`.
- **`mst2slv[*][j]` slice notation** — not tested directly; the NIC-400 spec doc itself notes it as speculative ("If the compiler doesn't currently parse this slice form, the equivalent verbose form is to enumerate the slice elements"). Treat as untested rather than confirmed-missing.

Not real keywords (i.e., not "missing features" — they don't exist in the language at all):
- **`wire_bus`** — appears only in `doc/nic400_interconnect_spec.md`. Not in the lexer, parser, or any other doc. The correct working form is `wire X: BusName;` (e.g. `wire w: FooBus;` from `tests/integration_test.rs`'s Parent example).

Available but I previously claimed missing (correction):
- **`mutex<UserPolicyFn>`** — supported. Requires a `hook grant_select(...) = UserPolicyFn(...);` block attached to the resource. Working pattern in `tests/integration_test.rs:9841` (`test_resource_lock_custom_policy_with_hook`):
  ```arch
  resource shared_lk: mutex<PickHigh>
    hook grant_select(req_mask: UInt<2>, last_grant: UInt<2>) -> UInt<2>
         = PickHigh(req_mask, last_grant);
  end resource shared_lk
  ```
  This means the QoS arbitration in the slave-port `aw_lock` can be wired in directly — replacing `mutex<round_robin>` with `mutex<nic400_qos_pick>` plus the hook block — without needing a separate `arbiter` instance.

Real compiler-side gap exposed by this design:
- Hierarchical formal v1 rejects auto-generated thread sub-modules that contain `wire` decls produced by lock-arbitration lowering. Filed as [arch-hdl-lang/arch-com#383](https://github.com/arch-hdl-lang/arch-com/issues/383).
