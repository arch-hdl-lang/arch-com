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

The `for` loop with `wait` generates a counter register and a loop-body state.

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
