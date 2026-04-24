# Plan: multi-head linklist (shared node pool)

> **Status**: design + phased rollout plan.

## Motivation

Today `linklist` synthesizes a **single list** — one head, optionally one tail, one node pool, one free list. For independent lists (N task queues, per-VC buffers, timer wheels), users already compose N linklists via `generate_for inst`, which works end-to-end.

The gap: designs like **MSHR** (cache miss status holding register), **per-flow queues with shared storage**, and **per-address pending tables** need **K linked lists sharing a pool of N slots** — any slot can belong to any chain. N lists × DEPTH slots each would waste area; a shared-pool design trades that waste for a single free-list controller.

cache_mshr in CVDP hand-rolls exactly this: `entry_has_next` + `entry_next_idx` per entry, one chain per outstanding cache line. This plan adds the language support so it (and similar designs) can use the `linklist` construct.

## Non-motivation

- **N independent linklists** — already solved by `generate_for inst`. Don't duplicate.
- **General graph / tree structures** — BST-style left+right children, arbitrary DAG. Out of scope; `linklist` stays linear.
- **Runtime-variable NUM_HEADS** — compile-time constant only. Matches every other linklist param.

## Syntax

Add an optional `param NUM_HEADS: const = N;` to the linklist's `param` block. Default `1` (back-compat; all existing linklists behave exactly as today).

```arch
linklist MshrChains
  param DEPTH: const = 32;
  param NUM_HEADS: const = 16;
  param DATA: type = UInt<64>;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;

  kind singly;
  track tail:   true;
  track length: true;

  op insert_tail
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<4>;     // which chain (0..NUM_HEADS-1)
    port req_data:     in UInt<64>;
    port resp_valid:   out Bool;
    port resp_handle:  out UInt<5>;
  end op insert_tail

  op delete_head
    latency: 2;
    port req_valid:    in Bool;
    port req_ready:    out Bool;
    port req_head_idx: in UInt<4>;
    port resp_valid:   out Bool;
    port resp_data:    out UInt<64>;
    port resp_handle:  out UInt<5>;
  end op delete_head
end linklist MshrChains
```

- `NUM_HEADS = 1` (default): existing behavior; no `req_head_idx` port allowed.
- `NUM_HEADS > 1`: ops that touch a specific chain **must** include `req_head_idx: in UInt<ceil_log2(NUM_HEADS)>` in their port list. The compiler routes the op to the addressed head. Ops that operate on the shared pool (future `alloc` / `free`) stay head-independent.

Explicit port declaration (rather than implicit) follows ARCH's "everything explicit" rule — user sees the added port signature and understands per-head routing without reading the spec.

## Semantics

- **Free list** (FIFO of slot indices): shared across all heads. Size DEPTH.
- **Node pool** (simple_dual RAM): shared. Size DEPTH × (|data| + |ptr| × links).
- **Head register**: `Vec<handle, NUM_HEADS>`, reset per-head to `NULL`.
- **Tail register** (if `track tail`): `Vec<handle, NUM_HEADS>`, reset per-head to `NULL`.
- **Length counter** (if `track length`): `Vec<len, NUM_HEADS>`, reset per-head to 0. Per-head length, not aggregate.
- **Controller FSM**: single FSM; reads `req_head_idx` each op to index into head/tail/length vectors. No parallel multi-op support — same back-pressure contract as today.

Empty / full semantics:
- `head[i]` is NULL → chain i is empty. `delete_head` with empty chain stalls (`req_ready = 0`).
- Pool full (free list empty) → `insert_*` stalls regardless of head.

## Phased rollout

Five PRs, each independently reviewable:

### Phase A — parser + AST + typecheck gate (this session)

- AST: `LinklistDecl::num_heads: u32` (default 1).
- Parser: accept `param NUM_HEADS: const = N;` like other params; extract to the field during elaboration/resolve.
- Typecheck:
  - When `NUM_HEADS == 1`, reject any `req_head_idx` port with a clear error (backward-compat — existing single-head linklists don't have it).
  - When `NUM_HEADS > 1`, for every op with per-head semantics, require a `req_head_idx` port of width `ceil_log2(NUM_HEADS)` and reject otherwise.
  - For now, **codegen/sim still error out** when `NUM_HEADS > 1` (not yet implemented). Phase A only lands the language surface.

### Phase B — SV codegen multi-head

- Head / tail / length regs become `logic [HANDLE_W-1:0] _head_r [NUM_HEADS];` (or similar flat array) in the generated SV.
- Each op's controller FSM reads `req_head_idx` and indexes the vector.
- NUM_HEADS == 1 path unchanged (scalar `_head_r`) — verify byte-identical SV emission.

### Phase C — sim mirror

- `sim_codegen/linklist.rs` mirrors Phase B: head/tail/length become C++ arrays, ops route by head_idx.

### Phase D — refactor cache_mshr as demo

- `tests/cvdp/cache_mshr.arch` rewritten using `linklist` with `NUM_HEADS = NUM_CACHE_LINES_SUPPORTED`.
- Keep CVDP test passing — proves the construct handles a real workload.

### Phase E — docs + reference card update

- Spec §12 gets a NUM_HEADS subsection.
- Reference card gets a multi-head example.

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| Byte-identical SV for NUM_HEADS == 1 breaks | Regression test: existing `hw_task_queue.arch` produces identical SV before/after each phase. |
| `req_head_idx` width mismatch at inst-site | Typecheck enforces `UInt<$clog2(NUM_HEADS)>` exactly; clear error if user widens. |
| FSM complexity when ops share a controller but index different heads | Single FSM reads `req_head_idx` into a latched state variable at accept-time; subsequent cycles use the latched value. Same as how existing ops latch their request args. |
| cache_mshr has extra metadata (addr, rw, data) — does the demo actually fit? | Write the refactor in Phase D before declaring the design successful. If it doesn't fit, fall back to keeping cache_mshr as a module and demoing with a simpler fixture (per-flow credit table). |

## Non-goals

- Multi-head ops that span heads in one transaction (e.g., "move chain A into chain B in 2 cycles"). Each op touches at most one head.
- Priority / weight across heads. The FSM serves ops in arrival order, not head priority.
