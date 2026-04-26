# 20.  Thread Block

> **Status: Implemented.** All features below are supported: `wait until`, `wait N cycle`, `thread once`, named threads, `fork`/`join`, `for` loops with `wait`, `generate for/if` with threads, `resource`/`lock`, `shared(or|and)`. Compiler lowers thread → FSM + inst at AST level.

A `thread` block is a sequential block that may span multiple clock cycles.  The compiler lowers it to a synthesizable FSM — each `wait` statement becomes a state boundary.  It provides the same expressive power as a hand-written `fsm` but reads as straight-line sequential code.

`seq` remains stateless (one clock edge, no implicit state).  `thread` is explicitly stateful.

## 20.1  Basic Syntax

```
thread on clk rising, rst_n low
  // Drive AXI read address
  ar_valid = 1;
  ar_addr  = addr_r;
  wait until ar_ready;
  ar_valid = 0;

  // Collect read response
  r_ready = 1;
  wait until r_valid;
  r_ready = 0;
  data_r <= r_data;
end thread
```

The clock and reset clause follows the same syntax as `seq`.  The thread repeats from the top after reaching `end thread` (implicit loop), or can be made one-shot with `thread once`.

### Compiler output

The above lowers to:

```
// Compiler-generated FSM (2-bit state register)
reg [1:0] _proc_state;

always_ff @(posedge clk or negedge rst_n) begin
  if (!rst_n)
    _proc_state <= 2'd0;
  else case (_proc_state)
    0: if (ar_ready) _proc_state <= 1;
    1: if (r_valid)  _proc_state <= 0;
  endcase
end

always_comb begin
  ar_valid = (_proc_state == 0);
  ar_addr  = addr_r;
  r_ready  = (_proc_state == 1);
end

always_ff @(posedge clk)
  if (_proc_state == 1 && r_valid)
    data_r <= r_data;
```

## 20.2  Protocol Primitives

All primitives from §19.2.2 are available inside `thread` blocks:

| Primitive | Meaning |
|-----------|---------|
| `wait until cond` | Pause until condition is true — becomes a state boundary |
| `wait N cycle` | Pause for exactly N clock cycles — counter + state boundary |
| `fork ... and ... join` | Drive parallel channels — per-arm done-bit registers |
| `if/elsif/else` | Conditional logic within a state |
| `for i in 0..N` | Repeated operations — loop counter + state boundary per iteration |

## 20.3  Fork/Join

```
thread on clk rising, rst_n low
  // AXI write: address and data channels in parallel
  fork
    aw_valid = 1;
    aw_addr  = addr_r;
    wait until aw_ready;
    aw_valid = 0;
  and
    w_valid = 1;
    w_data  = data_r;
    wait until w_ready;
    w_valid = 0;
  join

  // Wait for write response
  b_ready = 1;
  wait until b_valid;
  b_ready = 0;
  resp_r <= b_resp;
end thread
```

The `fork/join` lowers to parallel done-bit tracking as described in §19.2.2 — no simulation-only constructs.

## 20.4  Loops

```
thread on clk rising, rst_n low
  // AXI burst read: issue one AR, collect N beats
  ar_valid = 1;
  ar_addr  = base_addr;
  ar_len   = BURST_LEN - 1;
  wait until ar_ready;
  ar_valid = 0;

  for i in 0..BURST_LEN-1
    r_ready = 1;
    wait until r_valid;
    buf[i] <= r_data;
  end for
  r_ready = 0;
end thread
```

The `for` loop with `wait` generates a counter register (`_loop_cnt`) and a loop-body state. The compiler infers the minimum counter width from the end expression's type: if the end expression references a `UInt<N>` port or register, the counter is `UInt<N>` rather than the default `UInt<16>`. For example, `for i in 0..burst_len_r-1` where `burst_len_r: UInt<8>` generates an 8-bit counter instead of 16-bit, saving FFs in synthesized designs.

## 20.5  One-Shot Thread

By default, a thread repeats from the top.  Use `thread once` for initialization or single-transaction sequences:

```
thread once on clk rising, rst_n low
  // One-time calibration sequence
  cal_start = 1;
  wait until cal_done;
  cal_start = 0;
  cal_valid_r <= 1;
end thread once
```

The compiler generates a terminal state that holds after completion.

## 20.6  Named Thread

Threads can be named for readability and to support multiple threades in one module:

```
thread WriteHandler on clk rising, rst_n low
  ...
end thread WriteHandler

thread ReadHandler on clk rising, rst_n low
  ...
end thread ReadHandler
```

Each named thread generates an independent FSM with its own state register (`_write_handler_state`, `_read_handler_state`).

## 20.7  Generate with Threads

Threads work with `generate for` and `generate if`.  The loop variable is a compile-time constant within each unrolled instance.

```
module MultiChannelDma
  param NUM_CH: const = 4;
  port clk:   in Clock<SysDomain>;
  port rst_n: in Reset<Async, Low>;
  port addr:  in Vec<UInt<32>, NUM_CH>;
  port data:  in Vec<UInt<32>, NUM_CH>;
  port done:  out Vec<Bool, NUM_CH>;

  generate for i in 0..NUM_CH-1
    thread DmaChannel_i on clk rising, rst_n low
      // each thread gets its own FSM, parameterized by i
      aw_valid[i] = 1;
      aw_addr[i]  = addr[i];
      wait until aw_ready[i];
      aw_valid[i] = 0;
      done[i] <= 1;
    end thread DmaChannel_i
  end generate for i
end module MultiChannelDma
```

The compiler unrolls and produces `NUM_CH` independent FSMs with state registers `_dma_channel_0_state`, `_dma_channel_1_state`, etc.

Conditional generation:

```
generate if HAS_WRITE
  thread WriteHandler on clk rising, rst_n low
    ...
  end thread WriteHandler
end generate if
```

## 20.8  Resource Locking (`resource` / `lock`)

When multiple threads share a bus or set of signals, they need exclusive access.  The `resource` declaration and `lock` block provide synthesizable mutual exclusion.

### 20.8.1  Declaration

```
resource axi_wr: mutex<round_robin>;
```

The `mutex<policy>` type declares a shared resource with an arbitration policy.  The policy reuses existing `arbiter` policies: `round_robin`, `priority`, `lru`, `weighted`.

### 20.8.2  Lock Block

```
generate for i in 0..NUM_CH-1
  thread DmaChannel_i on clk rising, rst_n low
    // ... prepare addr/data locally ...

    lock axi_wr
      fork
        axi.aw_valid = 1;
        axi.aw_addr  = ch_addr[i];
        wait until axi.aw_ready;
        axi.aw_valid = 0;
      and
        axi.w_valid = 1;
        axi.w_data  = ch_data[i];
        wait until axi.w_ready;
        axi.w_valid = 0;
      join

      axi.b_ready = 1;
      wait until axi.b_valid;
      axi.b_ready = 0;
    end lock axi_wr

    ch_done[i] <= 1;
  end thread DmaChannel_i
end generate for i
```

A thread reaching `lock` asserts its request.  The thread stalls until granted.  Signals inside the `lock` body are driven through a grant-indexed mux.  When execution reaches `end lock`, the thread releases the resource.

### 20.8.3  Compiler-Generated Hardware

The compiler generates three components:

1. **Arbiter** — reuses the existing `arbiter` construct internally, with the declared policy.  Each thread produces a `req[i]` signal; the arbiter outputs `grant[i]`.

2. **Mux** — all signals driven inside `lock` bodies are routed through a `grant`-indexed mux.  Only the granted thread's values reach the shared signals.

3. **Stall logic** — a thread's FSM holds at the `lock` entry state while `grant[i]` is deasserted.

```systemverilog
// Compiler-generated sketch for mutex<round_robin> with 4 threads:
wire [3:0] axi_wr_req;
wire [3:0] axi_wr_grant;
wire [1:0] grant_idx;

// Arbiter (round-robin)
// ... standard round-robin logic ...

// Mux: granted thread drives the bus
always_comb begin
  case (grant_idx)
    0: begin axi_aw_addr = ch_addr_0; axi_w_data = ch_data_0; end
    1: begin axi_aw_addr = ch_addr_1; axi_w_data = ch_data_1; end
    2: begin axi_aw_addr = ch_addr_2; axi_w_data = ch_data_2; end
    3: begin axi_aw_addr = ch_addr_3; axi_w_data = ch_data_3; end
  endcase
end

// Per-thread FSM stalls at lock state when not granted
// Thread 0: if (_dma_channel_0_state == LOCK_ENTRY && !axi_wr_grant[0]) hold;
```

### 20.8.4  Resource Types

| Type | Meaning | Hardware | Use case |
|------|---------|----------|----------|
| `mutex<policy>` | Exclusive — one holder at a time | Arbiter + mux | Single-port bus sharing |
| `semaphore<N, policy>` | Up to N concurrent holders | Counter-based arbiter | Multi-port memories, banked buses |

`semaphore` is planned for future implementation.

### 20.8.5  Rules

- **Exclusive drive**: signals driven inside a `lock` block must not be driven outside any `lock` on the same resource.  The compiler enforces this — these signals have no defined driver when no thread holds the lock (the mux defaults to zero or the last granted value, configurable).
- **Multiple resources**: a thread may lock different resources in sequence or hold multiple locks simultaneously (nested `lock` blocks on different resources).
- **Deadlock warning**: if two threads lock resources A and B in opposite order, the compiler emits a warning.  The compiler performs static lock-order analysis across all threads in the module.
- **No lock in `comb`/`seq`**: `lock` is only valid inside `thread` blocks.

## 20.9  Interaction with Other Blocks

| Block | Reads from thread | Writes to thread |
|-------|-------------------|-------------------|
| `comb` | Can read registers written by thread | Can drive wires read by `wait until` conditions |
| `seq` | Can read registers written by thread | Can write registers read by thread (separate driver — no conflict if different signals) |
| `thread` | Can read registers from another thread | Must not write the same register as another thread (single-driver rule) |

The single-driver rule applies per signal: a register may be driven by exactly one `thread`, `seq`, or `fsm` block.

## 20.10  Thread vs FSM vs Seq

| Feature | `seq` | `fsm` | `thread` |
|---------|-------|-------|-----------|
| Spans multiple cycles | No | Yes | Yes |
| Explicit states | — | Yes (named) | No (implicit from `wait`) |
| State visible to user | — | Yes (enum) | No (compiler-internal) |
| `wait until` | No | No (use `transition ... when`) | Yes |
| `fork/join` | No | No | Yes |
| Best for | Simple registered logic | Complex control with named states | Sequential protocols, handshakes |

**Rule of thumb:** use `seq` for single-cycle register updates, `fsm` when you want named states and explicit transitions, `thread` when the logic is naturally sequential but spans multiple cycles.

## 20.11  Relation to Bus Implement Blocks

`implement BusName.method rtl` (§19.2.2) is syntactic sugar for a `thread` block that is scoped to a bus method's signals and parameters.  The same lowering machinery is used.  The difference is scope:

- `thread` lives inside a `module` and operates on the module's signals
- `implement ... rtl` lives at file scope and defines how a bus method maps to signals

## 20.12  Multi-Round Threads: Static Round-Robin Assignment

When a thread must process more work items than there are threads (e.g. `total_xfers > NUM_OUTSTANDING`), the natural pattern is **static round-robin assignment**: thread `i` owns work items `i, i+N, i+2N, ...`.  No shared counter is needed — each thread tracks only its own completion count.

### Pattern

```arch
// Per-thread completion counter (module-level reg)
reg thread_complete: Vec<UInt<16>, NUM_OUTSTANDING> reset rst => 0;

generate_for i in 0..NUM_OUTSTANDING-1
  thread Worker_i on clk rising, rst high
    // Wait condition: thread i's next item must be in range.
    // Item index = i + thread_complete[i] * NUM_OUTSTANDING
    wait until active and ((thread_complete[i] << $clog2(NUM_OUTSTANDING)) + i < total_work_r);

    // ... do work for item index (thread_complete[i] * NUM_OUTSTANDING + i) ...

    // Advance this thread's counter — no race, only thread i writes thread_complete[i]
    thread_complete[i] <= (thread_complete[i] + 1).trunc<16>();
  end thread Worker_i
end generate_for
```

### Done detection

```arch
// Sum completions across all threads
let tc01: UInt<17> = thread_complete[0] + thread_complete[1];
let tc23: UInt<17> = thread_complete[2] + thread_complete[3];
let total_complete: UInt<18> = tc01 + tc23;
let all_done: Bool = active_r and (total_work_r != 0)
    and (total_complete == total_work_r.zext<18>());
```

### Why not a shared counter?

A shared counter (`xfers_issued_r`) appears simpler but has a race condition: all threads see `issued < total` simultaneously and all advance to the lock-request state in the same cycle.  By the time later threads acquire the lock, the counter may already be at `total`, but they are committed to issuing work — causing over-issuance and `done` never firing.

Static round-robin avoids this entirely: thread `i` can only issue its own subset of items, and its local completion counter is only incremented by thread `i`.

### Performance

Static round-robin achieves the same throughput as a shared counter in the common case (work items ≥ NUM_OUTSTANDING), with zero arbitration overhead.  For unbalanced workloads where some items complete faster than others, threads with a higher assigned load finish last, but there is no work-stealing.  If load balancing is required, a shared counter with a larger lock scope (locking while reading AND incrementing the counter) is correct but limits parallelism.

## 20.13  `do..until` Inside `lock` — No Dead Cycle

The `do..until` loop inside a `lock` block has a subtle but important timing property: the first iteration fires **the same cycle the lock is granted**.

```arch
lock ar_ch
  do
    ar_valid = 1;
    ar_addr  = addr_r;
    ar_id    = i;
  until ar_ready;    // exits when ar_ready is asserted
end lock ar_ch
```

The compiler generates:
- **Comb outputs** (`ar_valid`, `ar_addr`, `ar_id`) are gated by `grant_i` — they appear the cycle the grant is asserted
- **State transition** fires when `grant_i && ar_ready` — same edge
- If `ar_ready` is already high when the grant fires, the lock is acquired and released in a single cycle (zero lock latency for trivially ready slaves)

Contrast with `wait until` after the lock header: `wait until` always adds a one-cycle pause between arriving at the state and observing the condition.  Use `do..until` when the loop body should drive combinationally while waiting.

## 20.14  Trailing Seq Assigns After State Boundaries

Seq assigns (`<=`) that appear immediately after a `do..until` or `wait until` statement are **automatically merged** into the preceding state's exit condition by the compiler.  This eliminates a dead cycle:

```arch
// Without merge: seq assign would fire a cycle AFTER the do..until exits
do
  ar_valid = 1;
until ar_ready;
ar_done_r <= 1;   // ← compiler merges this into the do..until exit guard
```

The merge fires `ar_done_r <= 1` on the same clock edge as the `ar_ready` handshake — no extra cycle.  This is only applied when:
- The seq assign follows immediately after the `do..until` / `wait until` (no intervening comb assigns or control flow)
- The state has a single, unconditional-except-for-condition transition

If the seq assign appears after a multi-transition state (e.g. inside a `for` loop), the compiler uses the loop exit condition as the guard instead of every iteration.

## 20.15  Auto-Emitted Spec-Contract SVA (`--auto-thread-asserts`)

Off-by-default flag on `arch build` / `arch sim` / `arch formal`. When set, the thread lowerer emits SVA properties anchored to the lowered state register `_t{i}_state` and per-thread counter `_t{i}_cnt`. They encode contracts the source spelled out (`wait until`, `wait N cycle`, `fork/join` branches) but the lowered comb+seq blob has lost — no downstream pass can recover them.

| Source construct | Property class | SVA shape |
|---|---|---|
| `wait until <cond>` | `_auto_thread_t{i}_wait_until_s{si}` | `(rst_inactive && state==si && cond) \|=> state==next` |
| `wait <N> cycle` | `_auto_thread_t{i}_wait_stay_s{si}` + `_..._wait_done_s{si}` | stay (`cnt!=0` ⇒ stay) and done (`cnt==0` ⇒ advance) |
| `fork`/`join` branches | `_auto_thread_t{i}_branch_s{si}_b{bi}` | per-(cond, target) `(rst_inactive && state==si && cond) \|=> state==target` |

All wrapped in `synopsys translate_off/on`. Reset polarity inverted to the not-in-reset guard (active-low → bare `rst`, active-high → `!rst`). Skipped for terminal `thread once` last states (vacuous) and threads with `default_when` (the soft-reset escape can preempt any state). Unconditional transitions are not asserted: `|=> next` is trivially true and adds noise without catching anything new.

State-space-integrity properties (reachability cover, "state stays in declared range") are out of scope here — they belong with FSM-construct auto-gen and lowered threads will inherit them.

**Why these properties hold by construction.** Each property is a corollary of the lowering equivalence proof in [doc/thread_lowering_proof.md](thread_lowering_proof.md): Corollary W (wait_until progress, §II.11.1) follows from Theorem 1 / Lemma 2 clause (c); Corollary C (wait_cycles bounded liveness, §II.11.2) from Lemma 2 clause (d); Corollary B (fork/join branch faithfulness, §II.11.3) from Lemma F. An `ASSERTION FAILED` from one of these labels is therefore evidence of a compiler bug or a hand-edit of the lowered RTL, not a user-program bug.
