# Plan: `cam` first-class construct

> **Status**: design + phased rollout plan.

## Motivation

Content-addressable lookup ("given a key, find the index where it's stored") shows up in every cache, MSHR, TLB, scoreboard, and per-flow tracker. Today users hand-roll it: `Vec<reg> entry_valid` + `Vec<reg> entry_key` + a comb loop with a priority encoder. Three problems:

1. **Repetition** — every design re-writes the same priority-encoder pattern (see `tests/cvdp/cache_mshr.arch:84-95`).
2. **Subtle bugs** — `~prev_all_zeros == false` style guards are easy to get wrong.
3. **Multi-head linklist gap** — multi-head linklist's `req_head_idx` contract assumes a small fixed head index. Real workloads (MSHR, per-address pending tables) want chain identity = a wide content key, not an index. A `cam` construct + multi-head linklist composes cleanly: CAM maps content-key → head-idx, linklist takes it from there.

## Non-motivation

- **Ternary CAM (TCAM)** — match with don't-cares. Useful for routing tables; not in v1. Can be added as `kind: ternary` later.
- **Range matching** — out of scope.
- **Multi-write per cycle** — single write port in v1.

## Syntax

```arch
cam Mshr_Addr_Cam
  param DEPTH: const = 32;
  param KEY_W: const = 10;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;

  // write port: set or clear an entry
  port write_valid: in Bool;
  port write_idx:   in UInt<5>;       // $clog2(DEPTH)
  port write_key:   in UInt<10>;      // KEY_W
  port write_set:   in Bool;          // true=insert valid+key, false=clear valid

  // search port: combinational
  port search_key:  in UInt<10>;
  port search_mask: out UInt<32>;     // bitmask of matches; zero if no match
  port search_any:  out Bool;
  port search_first: out UInt<5>;     // LSB-priority first match (0 if none, gated by search_any)
end cam Mshr_Addr_Cam
```

## Semantics

- **Storage**: `entry_valid: Vec<Bool, DEPTH>`, `entry_key: Vec<UInt<KEY_W>, DEPTH>`. Both reset to 0/false.
- **Write port**:
  - `write_valid && write_set` → `entry_valid[write_idx] <= true; entry_key[write_idx] <= write_key;`
  - `write_valid && !write_set` → `entry_valid[write_idx] <= false;` (key untouched)
  - Multiple writes per cycle: not allowed (single write port; user serializes).
- **Search port**: combinational. `search_mask[i] = entry_valid[i] && entry_key[i] == search_key`. `search_any = OR(search_mask)`. `search_first = priority-encoded first set bit (LSB-priority)`.
- **Read-during-write**: search reflects pre-write state in the same cycle (write takes effect on the next clock edge). Matches arch's `reg` semantics.

## Phased rollout

Five PRs, each independently reviewable, mirroring the multi-head linklist plan.

### Phase A — parser + AST + typecheck gate

- AST: `CamDecl { common, depth, key_w, ports }`. Add `Cam(CamDecl)` to `Item` enum.
- Lexer: `cam` keyword.
- Parser: parse `cam Name ... end cam Name` — same shape as `ram`. Reuse `ConstructCommon` for params + ports.
- Typecheck:
  - Required params: `DEPTH` (const), `KEY_W` (const). Reject other param types.
  - Required ports (exact names + widths derived from params): `clk`, `rst`, `write_valid`, `write_idx: UInt<$clog2(DEPTH)>`, `write_key: UInt<KEY_W>`, `write_set`, `search_key: UInt<KEY_W>`, `search_mask: UInt<DEPTH>`, `search_any: Bool`, `search_first: UInt<$clog2(DEPTH)>`.
  - Codegen still errors out — Phase A only lands the language surface.

### Phase B — SV codegen

Lower to:
```sv
logic [DEPTH-1:0]            entry_valid_r;
logic [KEY_W-1:0]            entry_key_r [DEPTH];
logic [DEPTH-1:0]            search_mask_w;
// match comb
always_comb for (int i = 0; i < DEPTH; i++)
  search_mask_w[i] = entry_valid_r[i] && (entry_key_r[i] == search_key);
assign search_any = |search_mask_w;
// priority encoder for search_first (LSB-first)
// ...
// seq write
always_ff @(posedge clk) if (rst) entry_valid_r <= '0;
                         else if (write_valid) begin
                           if (write_set) begin entry_valid_r[write_idx] <= 1; entry_key_r[write_idx] <= write_key; end
                           else entry_valid_r[write_idx] <= 0;
                         end
```

### Phase C — sim mirror

`src/sim_codegen/cam.rs` — same C++ shape as the SV (per-entry valid + key vectors, comb match, priority encoder).

### Phase D — refactor cache_mshr

Replace lines 44-49 + the priority encoders at 70-95 with two `cam` instances (one for free-slot finding, one for address-CAM tail-finding). Confirm CVDP test still passes.

### Phase E — docs

Spec adds §13 `cam`. Reference card gets the construct + a 1-line MSHR example.

## Risks

| Risk | Mitigation |
|---|---|
| Write port doesn't fit "clear all on reset + per-entry update" cleanly | Phase A spec is explicit — `entry_valid_r` clears on reset, write port indexes one entry. If users need "clear all", they hold reset; otherwise wrap with their own logic. |
| Priority encoder for `search_first` is O(DEPTH) gate depth | Acceptable for DEPTH ≤ 64; document the limit. Add log-tree variant only if a benchmark demands it. |
| Cache_mshr's "find first matching with `~has_next` predicate" needs CAM-output post-filter | CAM exposes `search_mask`; cache_mshr ANDs with `~entry_has_next` and re-priority-encodes. Pure user code, no CAM extension needed. |

## Non-goals

- No `update` op (rewrite key in place) — clear + write next cycle.
- No multiple search ports — instantiate two CAMs.
- No CAM with payload data — that's a key→value RAM, use `ram` lookup-by-index after CAM gives the index.
