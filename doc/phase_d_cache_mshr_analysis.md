# Phase D analysis: cache_mshr does not fit multi-head linklist

## Finding

`tests/cvdp/cache_mshr.arch` cannot be refactored to use `linklist NUM_HEADS = N` cleanly. Multi-head linklist requires the head index to be a compile-time-bounded port input (`req_head_idx: UInt<$clog2(NUM_HEADS)>`). cache_mshr's chains are keyed by `allocate_addr` (10-bit cache line address, up to 1024 distinct heads), and at allocate time the controller must **search** for an existing chain whose address matches — chain identity is content-addressed, not index-addressed.

Concrete mismatches:

1. **Head count**: NUM_HEADS would need to be 2^CS_LINE_ADDR_WIDTH = 1024, with at most MSHR_SIZE=32 simultaneously non-empty. Per-head head/tail/length vectors would dominate area for no benefit.
2. **Insert routing**: cache_mshr's allocate doesn't take a head index — it scans `entry_valid & entry_addr == allocate_addr & ~entry_has_next` to find the tail of the matching chain (the `prev_idx` priority encoder, lines 84-95). Linklist's `insert_tail` op assumes the caller supplies the head idx.
3. **Dynamic chain creation**: A new chain comes into being when `allocate_addr` doesn't match any valid entry. Linklist treats heads as fixed slots, all "alive" from reset.

## Recommendation

Per the Phase D risk row in `doc/plan_linklist_multi_head.md`: keep cache_mshr as a hand-rolled module, and demo multi-head linklist with a fixture whose chain identity *is* a small fixed index. Candidates:

- **Per-flow credit table**: K flows, each tracking pending credits as a linked list of credit-grant tickets in a shared pool. Head index = flow id.
- **Per-priority task queue**: K priority levels, shared task descriptor pool.
- **Per-VC buffer**: K virtual channels, shared flit pool. Closest to NoC use.

All three have static `NUM_HEADS` known at compile time and head index supplied by the requester — exactly the multi-head linklist contract.

## Status

Phase D is unblocked architecturally — Phases A/B/C shipped the construct correctly. The blocker is choosing which demo fixture to write. Awaiting user pick.
