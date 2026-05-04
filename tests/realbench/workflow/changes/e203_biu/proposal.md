# e203_biu — Fix Proposal

## Intent
Fix e203_biu to pass RealBench Verilator simulation (currently 222/222 mismatches — 100% failure).

## Root Cause
The current ARCH implementation is a simplified combinational priority mux that directly connects LSU/IFU command ports to downstream targets. This is fundamentally insufficient. The RealBench spec requires:

1. **Arbitration with FIFO tracking** — the `sirv_gnrl_icb_arbt` submodule provides priority arbitration (LSU > IFU) with outstanding transaction tracking (FIFO_OUTS_NUM = E203_BIU_OUTS_NUM), user-flag-based response routing, and ALLOW_0CYCL_RSP = 0.
2. **Command/response buffering** — the `sirv_gnrl_icb_buffer` submodule provides pipelined command buffering (CMD_DP) and response buffering (RSP_DP) with flow control.
3. **Address-based splitting** — the `sirv_gnrl_icb_splt` submodule routes commands to the correct downstream target (PPI, CLINT, PLIC, FIO, MEM, ifuerr) based on region_indic address comparison, with response aggregation back.
4. **IFU error channel** — when IFU attempts to access peripheral space, the request is routed to an ifuerr channel that returns zero-cycle error response (rsp_err=1, rsp_data=0).

The current ARCH code has none of these — it's a bare combinational mux with no buffering, no transaction tracking, no splitter handshake, and no IFU error handling.

## Scope
- In scope: Complete redesign of e203_biu.arch matching spec behavior
- Out of scope: Implementing sirv_gnrl_icb_arbt/buffer/splt as separate reusable .arch modules (inline the behavior in e203_biu for now)

## Approach
Redesign as an FSM-based module with:
- Priority arbiter with grant-hold (LSU > IFU)
- Outstanding transaction counter for flow control
- Address decode + command routing per spec §5.3
- Response tracking with usr flag for initiator demux
- IFU error channel with zero-cycle response
