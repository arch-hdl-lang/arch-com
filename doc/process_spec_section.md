# 20.  Process Block

A `process` block is a sequential block that may span multiple clock cycles.  The compiler lowers it to a synthesizable FSM — each `wait` statement becomes a state boundary.  It provides the same expressive power as a hand-written `fsm` but reads as straight-line sequential code.

`seq` remains stateless (one clock edge, no implicit state).  `process` is explicitly stateful.

## 20.1  Basic Syntax

```
process on clk rising, rst_n low
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
end process
```

The clock and reset clause follows the same syntax as `seq`.  The process repeats from the top after reaching `end process` (implicit loop), or can be made one-shot with `process once`.

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

All primitives from §19.2.2 are available inside `process` blocks:

| Primitive | Meaning |
|-----------|---------|
| `wait until cond` | Pause until condition is true — becomes a state boundary |
| `wait N cycle` | Pause for exactly N clock cycles — counter + state boundary |
| `fork ... and ... join` | Drive parallel channels — per-arm done-bit registers |
| `if/elsif/else` | Conditional logic within a state |
| `for i in 0..N` | Repeated operations — loop counter + state boundary per iteration |

## 20.3  Fork/Join

```
process on clk rising, rst_n low
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
end process
```

The `fork/join` lowers to parallel done-bit tracking as described in §19.2.2 — no simulation-only constructs.

## 20.4  Loops

```
process on clk rising, rst_n low
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
end process
```

The `for` loop with `wait` generates a counter register and a loop-body state.

## 20.5  One-Shot Process

By default, a process repeats from the top.  Use `process once` for initialization or single-transaction sequences:

```
process once on clk rising, rst_n low
  // One-time calibration sequence
  cal_start = 1;
  wait until cal_done;
  cal_start = 0;
  cal_valid_r <= 1;
end process once
```

The compiler generates a terminal state that holds after completion.

## 20.6  Named Process

Processes can be named for readability and to support multiple processes in one module:

```
process WriteHandler on clk rising, rst_n low
  ...
end process WriteHandler

process ReadHandler on clk rising, rst_n low
  ...
end process ReadHandler
```

Each named process generates an independent FSM with its own state register (`_write_handler_state`, `_read_handler_state`).

## 20.7  Interaction with Other Blocks

| Block | Reads from process | Writes to process |
|-------|-------------------|-------------------|
| `comb` | Can read registers written by process | Can drive wires read by `wait until` conditions |
| `seq` | Can read registers written by process | Can write registers read by process (separate driver — no conflict if different signals) |
| `process` | Can read registers from another process | Must not write the same register as another process (single-driver rule) |

The single-driver rule applies per signal: a register may be driven by exactly one `process`, `seq`, or `fsm` block.

## 20.8  Process vs FSM vs Seq

| Feature | `seq` | `fsm` | `process` |
|---------|-------|-------|-----------|
| Spans multiple cycles | No | Yes | Yes |
| Explicit states | — | Yes (named) | No (implicit from `wait`) |
| State visible to user | — | Yes (enum) | No (compiler-internal) |
| `wait until` | No | No (use `transition ... when`) | Yes |
| `fork/join` | No | No | Yes |
| Best for | Simple registered logic | Complex control with named states | Sequential protocols, handshakes |

**Rule of thumb:** use `seq` for single-cycle register updates, `fsm` when you want named states and explicit transitions, `process` when the logic is naturally sequential but spans multiple cycles.

## 20.9  Relation to Bus Implement Blocks

`implement BusName.method rtl` (§19.2.2) is syntactic sugar for a `process` block that is scoped to a bus method's signals and parameters.  The same lowering machinery is used.  The difference is scope:

- `process` lives inside a `module` and operates on the module's signals
- `implement ... rtl` lives at file scope and defines how a bus method maps to signals
