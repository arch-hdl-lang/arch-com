# Thread Construct: Multi-Outstanding Transaction Support

## Overview

This spec extends the existing `thread` construct (§20 of `doc/thread_spec_section.md`) to support **pipelined outstanding transactions on a single shared interface** — the key missing capability for modeling protocols like AXI with multiple in-flight reads/writes.

The core additions are:

1. **`shared(reduction)` signal annotation** — allows multiple threads to drive the same signal with compiler-synthesized reduction logic
2. **Narrow-scope `lock`** — threads hold a shared bus only for the handshake phase, not the full round-trip
3. **Pattern-matched `wait`** — threads block until a response matches their ID

Together, these enable N generate-for threads to pipeline requests on a single AXI port without manual FSM, arbiter, or demux code.

---

## Motivation

The existing thread spec handles three concurrency patterns well:

| Pattern | Mechanism | Works? |
|---|---|---|
| N independent channels (vectorized ports) | `generate for` + thread array | Yes |
| Serialized shared bus access | `resource` / `lock` | Yes |
| Intra-transaction parallelism (e.g. AW + W) | `fork` / `join` | Yes |

But it cannot express **N outstanding transactions on a single shared interface** — the common case for AXI, where one AR channel and one R channel serve multiple in-flight reads tagged by ID. The problems:

- `lock` serializes the entire round-trip (AR issue → R collect), preventing pipelining
- Multiple threads driving `r_ready` violates the single-driver rule
- No mechanism to route `r_data` to the correct waiting thread by `r_id`

---

## New Language Feature: `shared(reduction)` Signals

### Syntax

```arch
signal r_ready : logic shared(or);
signal all_done : logic shared(and);
```

### Semantics

A `shared(reduction)` signal permits multiple drivers. The compiler synthesizes a single physical driver by applying the reduction operator across all thread-local driver values.

### Supported Reductions

| Annotation | Generated Logic | Use Case |
|---|---|---|
| `shared(or)` | `sig = drv_0 \| drv_1 \| ... \| drv_N` | Ready signals — assert when any thread is waiting |
| `shared(and)` | `sig = drv_0 & drv_1 & ... & drv_N` | Backpressure — assert only when all consumers are ready |

### Default value

When a thread is not actively driving a `shared` signal, its contribution defaults to the identity element of the reduction: `0` for `or`, `1` for `and`.

### Compiler behavior

1. **No single-driver error** on `shared` signals — multi-driver is explicitly permitted
2. Each thread's assignment to a `shared` signal becomes a thread-local wire (e.g. `r_ready__thread_3`)
3. The compiler emits a top-level continuous assignment applying the reduction across all thread-local wires
4. **Type checking**: only `logic` (1-bit) or `logic[N:0]` (bitwise reduction) signals may be `shared`

---

## Pipelined AXI Read: Full Example

```arch
module AxiReader #(
  parameter NUM_OUTSTANDING = 32,
  parameter ADDR_WIDTH = 32,
  parameter DATA_WIDTH = 64
) (
  input  clk,
  input  rst_n,
  // AR channel
  output ar_valid : logic,
  output ar_addr  : logic[ADDR_WIDTH-1:0],
  output ar_id    : logic[$clog2(NUM_OUTSTANDING)-1:0],
  input  ar_ready,
  // R channel
  input  r_valid,
  input  r_data  : logic[DATA_WIDTH-1:0],
  input  r_id    : logic[$clog2(NUM_OUTSTANDING)-1:0],
  output r_ready : logic shared(or)
);

  resource ar_bus : mutex<round_robin>;

  generate for i in 0..NUM_OUTSTANDING-1

    thread ReadReq_i on clk rising, rst_n low

      // === AR phase: exclusive access, held for 1 handshake only ===
      lock ar_bus
        ar_valid = 1;
        ar_addr  = addr_table[i];
        ar_id    = i;
        wait until ar_ready;
        ar_valid = 0;
      end lock
      // ar_bus released — next thread can issue AR immediately

      // === R phase: no lock needed, shared(or) on r_ready ===
      r_ready = 1;
      wait until r_valid && r_id == i;
      data_buf[i] <= r_data;
      r_ready = 0;

    end thread ReadReq_i

  end generate for i

end module
```

### What the compiler generates

**AR side** (from existing `resource`/`lock` mechanism):
- Round-robin arbiter selecting which thread drives `ar_valid`, `ar_addr`, `ar_id`
- Mux gated by arbiter grant
- Stall logic for non-granted threads

**R side** (new `shared(or)` mechanism):
- Per-thread wire: `r_ready__thread_i = (state_i == WAIT_R) ? 1 : 0`
- Reduction: `r_ready = r_ready__thread_0 | r_ready__thread_1 | ... | r_ready__thread_31`
- Per-thread capture: `data_buf[i] <= (r_valid && r_id == i) ? r_data : data_buf[i]`
- Per-thread state advance: transition out of WAIT_R state when `r_valid && r_id == i`

---

## Pipelined AXI Write: Full Example

```arch
module AxiWriter #(
  parameter NUM_OUTSTANDING = 4
) (
  input  clk, rst_n,
  // AW channel
  output aw_valid : logic,
  output aw_addr  : logic[31:0],
  output aw_id    : logic[1:0],
  input  aw_ready,
  // W channel
  output w_valid  : logic,
  output w_data   : logic[63:0],
  output w_last   : logic,
  input  w_ready,
  // B channel
  input  b_valid,
  input  b_id     : logic[1:0],
  output b_ready  : logic shared(or)
);

  resource aw_bus : mutex<round_robin>;
  resource w_bus  : mutex<round_robin>;

  generate for i in 0..NUM_OUTSTANDING-1

    thread WriteReq_i on clk rising, rst_n low

      // AW + W can pipeline independently
      fork
        lock aw_bus
          aw_valid = 1;
          aw_addr  = waddr_table[i];
          aw_id    = i;
          wait until aw_ready;
          aw_valid = 0;
        end lock
      and
        lock w_bus
          w_valid = 1;
          w_data  = wdata_table[i];
          w_last  = 1;
          wait until w_ready;
          w_valid = 0;
        end lock
      join

      // B response: shared(or) on b_ready, match by ID
      b_ready = 1;
      wait until b_valid && b_id == i;
      b_ready = 0;

    end thread WriteReq_i

  end generate for i

end module
```

---

## Implementation Guide

### Files to modify

All changes build on the existing thread implementation path described in `doc/thread_spec_section.md`.

#### 1. `src/lexer.rs` — New keyword token

- `Shared` keyword token

#### 2. `src/ast.rs` — New AST nodes

```
// Signal declaration modifier
enum SharedReduction {
    Or,
    And,
}

// Extend signal/port declarations
struct SignalDecl {
    name: String,
    typ: Type,
    shared: Option<SharedReduction>,  // NEW
    ...
}
```

#### 3. `src/parser.rs` — Parse `shared(or|and)`

- In signal/port declaration parsing, after type annotation, check for `shared` keyword
- Parse `(or)` or `(and)` as the reduction policy
- Store in `SignalDecl.shared`

#### 4. `src/typecheck.rs` — Validation rules

- **Lift single-driver restriction**: if signal has `shared` annotation, allow multiple drivers
- **Type constraint**: `shared` signals must be `logic` or `logic[N:0]` (no structs, enums, etc.)
- **Thread-only drivers**: `shared` signals may only be driven from within `thread` blocks (not from bare `always` or continuous assign)
- **Reduction identity**: warn if a thread drives a `shared(or)` signal to `1` unconditionally (always-on driver defeats the purpose)

#### 5. `src/codegen.rs` — Emit reduction logic

For each `shared` signal:

1. **Collect all thread-local drivers** — each thread's assignment becomes a gated wire:
   ```verilog
   wire r_ready__thread_0 = (proc_state_0 == S_WAIT_R) ? 1'b1 : 1'b0;
   wire r_ready__thread_1 = (proc_state_1 == S_WAIT_R) ? 1'b1 : 1'b0;
   ...
   ```

2. **Emit reduction assignment**:
   ```verilog
   // shared(or)
   assign r_ready = r_ready__thread_0 | r_ready__thread_1 | ... ;

   // shared(and)
   assign all_done = all_done__thread_0 & all_done__thread_1 & ... ;
   ```

3. **Thread FSM state transitions** — the `wait until r_valid && r_id == i` condition is per-thread and uses the original (pre-reduction) `r_valid` and `r_id` inputs, not the shared output.

#### 6. Interaction with existing `resource`/`lock`

No changes needed to the lock mechanism. The key design principle:

- **`lock`** = exclusive access, one driver at a time (AR/AW channels) — existing behavior
- **`shared(reduction)`** = concurrent multi-driver with synthesized merge (R/B ready signals) — new behavior

These are complementary. A signal is either `shared` (multi-driver with reduction) or not (single-driver, arbitrated by `lock`). A signal cannot be both `shared` and driven inside a `lock` block — the type checker should reject this.

---

## Relationship to Prior Art

| System | Analog | Difference |
|---|---|---|
| Go goroutines + channels | `generate for` + thread = goroutines; `lock` = channel send; `shared(or)` = `select` on multiple receivers | arch-com is synthesizable, compile-time fixed N |
| CSP (Hoare 1978) | `wait until` = channel receive; `lock` = synchronized send | Formal model underneath |
| Erlang actors | `wait until r_valid && r_id == i` = pattern-matched `receive` | Runtime vs compile-time demux |
| Bluespec rules | Thread = guarded atomic action; `shared` = compiler conflict resolution | Bluespec uses implicit scheduling; arch-com is explicit sequential |
| SystemC SC_THREAD | Direct analog for sequential-to-FSM | SystemC is simulation-only; arch-com synthesizes |

---

## Open Questions

1. **`shared(mux, select_signal)`** — should we support an arbitrated mux reduction where one of N drivers is selected by an explicit signal? This would generalize beyond boolean reductions.

2. **`shared(xor)`** — useful for parity generation across threads. Low priority but trivial to add.

3. **Multi-bit reduction** — for `logic[N:0] shared(or)`, the reduction is bitwise OR across all drivers. Should we also support word-level reductions (e.g. `shared(max)`, `shared(add)`)? These would need multi-cycle logic.

4. **`semaphore<N>`** from §20.8.4 — with `shared` signals, is the semaphore still needed? It might still be useful for limiting concurrency (e.g. only 4 of 32 threads can be in AR phase simultaneously) without the strict exclusion of mutex.

5. **Deadlock analysis** — with narrow-scope locks (AR only) and shared R collection, circular dependencies are unlikely but the compiler should still verify: no thread holds lock A while waiting for lock B if another thread holds B and waits for A.
