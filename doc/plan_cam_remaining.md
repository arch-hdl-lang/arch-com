# Cam construct — remaining work

> **Status (2026-04-24)**: shipped v1 (DEPTH/KEY_W, single write), v2 (dual write), v3 (value_type payload). The four features below were on the original §13 aspirational list in the spec. None has a consumer in the current `tests/` or `tests/cvdp/` tree — that's the gating signal: build them when a real design wants them, not before.

## What's already on main

| Feature | PR | Demo |
|---|---|---|
| v1: DEPTH/KEY_W, single write | #122 | `tests/cam_basic.arch` |
| v2: dual write port | #124 | `tests/cam_dual_basic.arch`, `tests/cvdp/cache_mshr.arch` |
| v3: value_type payload | #129 | `tests/cam_value_basic.arch`, `tests/mac_table.arch` |
| Spec + reference card | #123 + #129 | doc/ARCH_HDL_Specification.md §13.0/13.0a/13.0b |
| TLB demo (no value_type fit) | #126 | `tests/cvdp/TLB.arch` |
| Compiler bugs uncovered | #127 (sim_codegen Vec sizing), #128 (inst sub-Vec port wiring) | regression tests |

## Remaining features (build when motivated)

### 1. TCAM — ternary / wildcard match (`kind: ternary`)

**What it is**: each stored key bit can be 0, 1, or X (don't-care). Match condition becomes `(search_key & key_mask) == (stored_key & key_mask)` per entry. The mask is stored alongside the key.

**What would justify it**: a CVDP file (or new design) doing routing tables, longest-prefix-match, firewall ACLs, packet classification, IP filter rules. **Triage 2026-04-24**: no such file in CVDP. Skip until a routing/ACL design lands.

**Sketch**:
```arch
cam Acl_Cam
  param DEPTH: const = 16;
  param KEY_W: const = 32;
  kind ternary;                    // ← new

  port write_mask: in UInt<32>;    // ← new (alongside write_key)
  // search ports unchanged
end cam Acl_Cam
```
- Storage: add `entry_mask_r [DEPTH]`.
- Match: `search_mask[i] = entry_valid_r[i] && ((entry_key_r[i] ^ search_key) & entry_mask_r[i]) == 0` (treat 1-bits in mask as care-bits).
- Effort: small (~80 lines codegen each side, mostly mirroring v3).

### 2. Multi-cycle pipelined comparator

**What it is**: split the per-entry compare across N pipeline stages so Fmax stays high for DEPTH > 64. Caller declares `latency: 2` (or N) instead of the implicit 0; cam adds N stages of register between key arrival and search outputs.

**What would justify it**: a design with DEPTH > 64 (current cap of "comfortable single-cycle"). **Triage 2026-04-24**: no such design — largest CAM in tree is cache_mshr at DEPTH=32.

**Sketch**: emit `entry_valid_r_stage[N]`, `entry_key_r_stage[N]`, register `search_key` through the stages, `assign search_mask[i] = entry_valid_r_stageN[i] && ...`. Activation via `latency: N` keyword (matches the spec's existing `latency` syntax for ram).

**Catch**: changes interface contract — search_mask/any/first become N-cycle latent, not combinational. Spec needs to be explicit, and consumers like the existing TLB / mac_table would NOT just work — they assume combinational lookup. Decision: when this lands, leave existing CAMs as latency 0 and offer the pipelined version as a separate kind (or as `latency: 1+`).

**Effort**: medium (~150 lines codegen each side + stage-register propagation logic).

### 3. Multiple search ports

**What it is**: the cam exposes K independent `search_key{i}` / `search_mask{i}` / `search_any{i}` / `search_first{i}` (and `read_value{i}` if value_type) port sets, each performing a fully independent lookup against the shared storage.

**What would justify it**: a superscalar / multi-issue core that issues 2+ instruction lookups per cycle to the same scoreboard or rename table. **Triage 2026-04-24**: no superscalar designs in CVDP.

**Sketch**: add `param NUM_SEARCH: const = K;` (default 1). For K > 1, emit indexed port suffixes (`search_key0`, `search_key1`, …) and replicate the comparator + priority encoder K times. Storage is shared — single `entry_*_r` array.

**Catch**: K=1 must keep current port names exactly (no `search_key0` rename) for back-compat. So the codegen has to special-case K==1.

**Effort**: medium (~100 lines codegen each side, plus port-naming logic).

### 4. Replacement policy (`replace: lru | fifo | random | none`)

**What it is**: the cam tracks per-entry usage and exposes a `replace_idx: out UInt<$clog2(DEPTH)>` indicating which slot to overwrite next. Caller wires `write_idx <- replace_idx` to evict the LRU/FIFO/random victim instead of picking a slot externally.

**What would justify it**: a fully-associative cache with bounded entries that needs eviction (bounded MSHR with eviction, ARP table with TTL, branch target buffer, etc.). **Triage 2026-04-24**: no fully-associative caches in CVDP. The two existing consumers pick slots externally — TLB uses round-robin via its own `replacement_idx` reg, mac_table delegates entirely to the caller.

**Sketch**:
- LRU: `entry_age_r [DEPTH]` per entry; on each successful search, the matched entry's age resets and others increment (saturating). `replace_idx = argmax(entry_age_r)`.
- FIFO: single `next_replace_r: UInt<$clog2(DEPTH)>`; `replace_idx = next_replace_r`; on write, increment.
- Random: LFSR-driven; `replace_idx = lfsr_r[$clog2(DEPTH)-1:0]`.

**Catch**: LRU's age-update logic on every search has area + power cost. Should probably also expose a `replace_lock_mask: in UInt<DEPTH>` so callers can pin slots (TLB pinned-page support).

**Effort**: medium-large (~200 lines codegen each side — three policies, ages-on-search side effect, possible lock_mask).

## Decision rule

Pick up any of these when:
1. A real consumer (CVDP, new demo, or external user) shows up that *needs* the feature, and
2. The simpler workaround (cam + external state, like TLB does for flush) is materially worse than the proposed cam-internal version.

Don't build them speculatively. The TCAM + LRU combinations especially get painful (carrying age across don't-care matches, what counts as a "use" when multiple entries match, etc.) and would benefit from a real workload to anchor the design choices.

## Other directions

- **Helper module library**: a generic `freelist` or `priority_encoder` module that callers like cache_mshr / TLB / mac_table could instantiate (currently they each hand-roll the LSB-priority loop). Not a cam feature, but shares the "factor out the priority encoder" theme.
- **Spec cleanup**: §13.1+ (the original aspirational sketch with binary/ternary/associative and `op lookup` syntax) doesn't match what shipped. Once one or two of the above features lands, revise §13.1+ to reflect the actual surface.
