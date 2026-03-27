# 19.  First-Class Construct: bus

A `bus` is a reusable, parameterized port bundle that eliminates repetitive port declarations across modules.  It serves two roles:

1. **RTL signal bundle** (implemented) — declares signal names, types, and directions.  The compiler flattens bus ports to individual SystemVerilog ports.
2. **TLM method interface** (planned) — declares transaction-level methods (`blocking`, `pipelined`, `out_of_order`, `burst`) on top of the signal bundle.  Used with `arch sim --tlm-lt/at` for high-speed architectural simulation.

Directions are declared from the **initiator's perspective**.  At the use site, `initiator` keeps directions as declared; `target` flips every `in` to `out` and every `out` to `in`.

## 19.1  Declaration — RTL Signals

```
bus_basic.arch
bus ItcmIcb
  param ADDR_W: const = 14;
  param DATA_W: const = 32;

  cmd_valid: out Bool;
  cmd_addr:  out UInt<ADDR_W>;
  cmd_ready: in  Bool;
  rsp_valid: in  Bool;
  rsp_data:  in  UInt<DATA_W>;
  rsp_ready: out Bool;
end bus ItcmIcb
```

**Signals** are declared as `name: direction Type;` — no `port` keyword.  Directions are from the initiator's (master's) point of view: `out` means the initiator drives, `in` means the initiator receives.

**Parameters** follow the same `param NAME: const = default;` syntax as all other constructs.  Parameter values propagate to signal widths.

A more complete example — AXI4-Lite with write address, write data, and write response channels:

```
axi_lite_bus.arch
bus AxiLite
  param ADDR_W: const = 32;
  param DATA_W: const = 32;

  // Write address channel
  aw_valid: out Bool;
  aw_ready: in  Bool;
  aw_addr:  out UInt<ADDR_W>;

  // Write data channel
  w_valid:  out Bool;
  w_ready:  in  Bool;
  w_data:   out UInt<DATA_W>;
  w_strb:   out UInt<DATA_W/8>;

  // Write response channel
  b_valid:  in  Bool;
  b_ready:  out Bool;
  b_resp:   in  UInt<2>;
end bus AxiLite
```

## 19.2  Declaration — TLM Methods (Planned)

A bus may optionally include a `methods` block that declares transaction-level operations.  Methods coexist with RTL signals — the same bus declaration serves both RTL simulation (`arch build`) and TLM simulation (`arch sim --tlm-lt/at`).

```
bus_with_tlm.arch
bus AxiLite
  param ADDR_W: const = 32;
  param DATA_W: const = 32;

  // RTL signals (same as above)
  aw_valid: out Bool;
  aw_ready: in  Bool;
  aw_addr:  out UInt<ADDR_W>;
  w_valid:  out Bool;
  w_ready:  in  Bool;
  w_data:   out UInt<DATA_W>;
  b_valid:  in  Bool;
  b_ready:  out Bool;
  b_resp:   in  UInt<2>;

  // TLM methods (planned — not yet implemented)
  methods
    blocking method write(addr: UInt<ADDR_W>, data: UInt<DATA_W>) -> UInt<2>
      timing: 2 cycles;
    end method write

    pipelined method read(addr: UInt<ADDR_W>) -> Future<UInt<DATA_W>>
      timing: 4 cycles;
      max_outstanding: 8;
    end method read
  end methods
end bus AxiLite
```

### 19.2.1  Method Concurrency Modes

| Mode | Return Type | Caller Behaviour | Use Case |
|---|---|---|---|
| `blocking` | `T` | Caller suspends until response arrives; next call cannot issue until this one completes | APB, AHB single-transfer, simple MMIO |
| `pipelined` | `Future<T>` | Caller gets a `Future` immediately and can issue more calls; responses arrive in order | AXI in-order reads, read-after-read pipelining |
| `out_of_order` | `Token<T, id_width: N>` | Caller gets a `Token` immediately; responses may arrive in any order, matched by ID | Full AXI with multiple IDs, out-of-order memory |
| `burst` | `Future<Vec<T, L>>` | One call issues N data beats; single AR, N data responses | AXI INCR bursts |

Synchronization primitives: `await f` (wait for one), `await_all(f0, f1, f2)` (wait for all), `await_any(t0, t1)` (wait for first to complete).

⚑  Methods describe the **transaction-level contract**.  At RTL (`arch build`), only the signal declarations are used.  At TLM (`arch sim --tlm-lt`), the compiler generates method call/response logic from the method declarations.  The `implement` block (§19.2.2) bridges the two: a cycle-accurate implement drives real signals behind the method call API, and the compiler can synthesize it to an FSM.

### 19.2.2  Implement Blocks — Cycle-Accurate Protocol Description (Planned)

An `implement` block maps a TLM method to cycle-accurate signal-level behavior.  The compiler lowers it to a synthesizable FSM — each `wait` statement becomes a state boundary.

```
implement AxiLite.write rtl
  // Drive write-address and write-data channels in parallel
  fork
    // AW channel
    aw_valid = 1;
    aw_addr  = addr;
    wait until aw_ready;
    aw_valid = 0;
  and
    // W channel
    w_valid = 1;
    w_data  = data;
    w_strb  = {DATA_W/8{1'b1}};
    wait until w_ready;
    w_valid = 0;
  join

  // Wait for write response
  b_ready = 1;
  wait until b_valid;
  b_ready = 0;
  return b_resp;
end implement AxiLite.write
```

#### Protocol primitives

| Primitive | Meaning | Synthesizable lowering |
|-----------|---------|----------------------|
| `wait until cond` | Pause until condition is true on a clock edge | State boundary — stay in current state while `!cond` |
| `wait N cycle` | Pause for exactly N clock cycles | Counter + state boundary |
| `fork ... and ... join` | Drive parallel channels independently | Per-arm done-bit registers; advance when all done |
| `if/elsif/else` | Conditional signal driving | Combinational mux within a state |
| `for i in 0..N` | Repeated beats (bursts) | Loop counter + state boundary per iteration |
| `wait until cond timeout N cycle` | Wait with timeout (planned) | Counter + OR condition for state exit |

#### Fork/join synthesis

`fork/join` does **not** create simulation threads.  The compiler lowers it to parallel done-tracking — the same pattern hardware engineers write manually for multi-channel protocols:

```
// Compiler-generated FSM for the fork/join above:
state WRITE_REQ:
  aw_valid = ~aw_done;
  w_valid  = ~w_done;
  aw_addr  = addr;
  w_data   = data;
  if aw_ready & ~aw_done
    aw_done <= 1;
  end if
  if w_ready & ~w_done
    w_done <= 1;
  end if
  transition to WRITE_RESP when (aw_done | aw_ready) & (w_done | w_ready);

state WRITE_RESP:
  b_ready = 1;
  transition to IDLE when b_valid;
```

Each `fork` arm becomes a done-bit register.  Each arm's `wait until` conditionally sets its done-bit.  The `join` transitions when all arms have completed (either previously or in the current cycle).  Done-bits reset on the outgoing transition.

For arms with multiple sequential waits, each arm becomes a mini sub-FSM with its own state register, and the join waits for all sub-FSMs to reach their terminal state.

#### Protocol expressiveness

The primitive set (`wait until`, `wait N cycle`, `fork/join`, `for`, `if/elsif`) covers the following protocol families:

| Protocol | Key pattern | Primitives used |
|----------|------------|-----------------|
| APB | Sequential phases (setup → access) | `wait 1 cycle`, `wait until pready` |
| AHB | Pipelined address/data with HREADY | `wait until hready` |
| AXI | 5 independent channels, bursts | `fork/join` (AW+W parallel), `for` (burst beats) |
| Wishbone | Single-channel handshake | `wait until ack` |
| TileLink | Multi-channel with ordering | `fork/join`, `wait until` |
| SPI/I2C | Bit-serial with clock manipulation | `wait 1 cycle`, `for` (bit loop) |

The design principle: these primitives compose to express any protocol that can be described as a sequence of conditional waits with optional parallelism.  This is equivalent in expressive power to a manually-written FSM (since the compiler lowers to exactly that), but with clearer intent.

#### Synthesizability

All four method modes are synthesizable when their static bounds are declared.  The compiler generates progressively more hardware, but every component is deterministic and follows standard microarchitecture patterns.

| Method mode | Bound required | Generated hardware |
|-------------|---------------|-------------------|
| `blocking` | — | Single FSM from `implement` block lowering |
| `pipelined` | `max_outstanding` | Request FSM + outstanding counter + response FIFO (`max_outstanding` entries) |
| `out_of_order` | `id_width` | Request FSM + completion buffer (`2^id_width` entries × data width) + free list + response collector |
| `burst` | `max_burst_len` | Request FSM + beat counter |

**Pipelined** example (back-to-back AXI writes): the request FSM issues writes using the same fork/join lowering as blocking, but instead of waiting for the B response, it increments an outstanding counter and accepts the next call immediately.  A separate response collector watches `b_valid`/`b_ready`, resolves Futures in FIFO order, and decrements the counter.  When outstanding hits `max_outstanding`, the request FSM stalls the caller.

**Out-of-order** example (AXI reads with multiple IDs): the request FSM allocates an ID from a free list, issues AR, and returns a `Token` to the caller.  A response collector watches `r_valid`/`r_ready` and writes `r_data` into a completion buffer indexed by `r_id`.  When the caller awaits a Token, the hardware checks the completion buffer — returns immediately if valid, stalls if not yet arrived.  The completion buffer has `2^id_width` entries, each `DATA_W` bits wide plus a valid bit.

All bounds are already required in the method declaration (`max_outstanding`, `id_width`, `max_burst_len`), so the compiler always has the static information needed to size the generated hardware.

## 19.3  Using a Bus Port

```
module Ift2Icb
  port clk:   in Clock<SysDomain>;
  port rst_n: in Reset<Async, Low>;
  port itcm:  initiator ItcmIcb;          // directions as declared
end module Ift2Icb

module ItcmSram
  port clk:   in Clock<SysDomain>;
  port rst_n: in Reset<Async, Low>;
  port itcm:  target ItcmIcb;             // directions flipped
end module ItcmSram
```

| Keyword | Meaning | Effect on `out` signals | Effect on `in` signals |
|---|---|---|---|
| `initiator` | This module drives the bus | `out` (output) | `in` (input) |
| `target` | This module receives the bus | `in` (input) | `out` (output) |

**Parameter overrides** are specified inline: `port axi: initiator AxiLite<ADDR_W=32, DATA_W=64>;`.  Unspecified parameters use their defaults from the bus declaration.

## 19.4  Signal Access

Bus signals are accessed with dot notation in `comb` and `seq` blocks:

```
comb
  itcm.cmd_valid = req_valid & ~stall;
  itcm.cmd_addr  = pc[15:2];
  ready          = itcm.cmd_ready & ~stall;
end comb

seq on clk rising
  if ~stall
    data_r <= itcm.rsp_data;
  end if
end seq
```

The type checker resolves `itcm.cmd_addr` to `UInt<14>` (from the bus declaration with `ADDR_W=14`) and verifies that only output signals are assigned and only input signals are read, according to the port's perspective.

## 19.5  Instance Connections

When instantiating a module with bus ports, connect individual signals using dot notation on the port name:

```
inst bridge: Ift2Icb
  connect clk              <- clk;
  connect rst_n            <- rst_n;
  connect itcm.cmd_valid   -> itcm_cmd_valid_w;
  connect itcm.cmd_addr    -> itcm_cmd_addr_w;
  connect itcm.cmd_ready   <- itcm_cmd_ready_w;
  connect itcm.rsp_valid   <- itcm_rsp_valid_w;
  connect itcm.rsp_data    <- itcm_rsp_data_w;
  connect itcm.rsp_ready   -> itcm_rsp_ready_w;
end inst bridge
```

The parser converts `itcm.cmd_valid` in the connect statement to the flattened name `itcm_cmd_valid`, matching the generated SV port.

## 19.6  SystemVerilog Output

Bus ports flatten to individual SV ports.  The naming convention is `{port}_{signal}`:

```systemverilog
module Ift2Icb (
  input  logic        clk,
  input  logic        rst_n,
  output logic        itcm_cmd_valid,
  output logic [13:0] itcm_cmd_addr,
  input  logic        itcm_cmd_ready,
  input  logic        itcm_rsp_valid,
  input  logic [31:0] itcm_rsp_data,
  output logic        itcm_rsp_ready
);
  ...
endmodule
```

No SV `interface`, `modport`, or `virtual interface` is used.  The output is plain structural Verilog compatible with every synthesis, simulation, and formal tool.

## 19.7  Type Checker Guarantees

| Check | Description |
|---|---|
| Bus exists | The bus name in `initiator BusName` must resolve to a `bus` declaration |
| Signal exists | `itcm.xyz` errors if `xyz` is not a declared signal in the bus |
| Direction correctness | Assigning to an input signal or reading an undriven output is an error |
| Per-signal drive coverage | Each output signal of a bus port must be driven — the checker expands the bus and verifies each flattened signal individually |
| Parameter validation | Overridden params must exist in the bus declaration |

## 19.8  Simulation Codegen

The sim codegen (C++ model generation) applies the same flattening.  Each bus signal becomes an individual `uint*_t` struct field:

```cpp
class VIft2Icb {
public:
  uint8_t  itcm_cmd_valid;
  uint16_t itcm_cmd_addr;
  uint8_t  itcm_cmd_ready;
  uint8_t  itcm_rsp_valid;
  uint32_t itcm_rsp_data;
  uint8_t  itcm_rsp_ready;
  ...
};
```

All bus signals are automatically included in VCD waveform trace output.

## 19.9  Why `bus` Instead of SV `interface`

SystemVerilog `interface` has well-documented weaknesses that `bus` avoids:

| SV `interface` Problem | `bus` Solution |
|---|---|
| Requires `modport` declarations that duplicate signal lists | Single declaration; `initiator`/`target` at use site flips directions automatically |
| `virtual interface` needed for class-based testbenches — adds indirection and synthesis limitations | Not applicable — flattened ports work everywhere |
| Tool support varies — many synthesis tools restrict or reject `interface` constructs | Output is plain `logic` ports — universal tool compatibility |
| Cannot parameterize signal sets cleanly (no generate inside interface in most tools) | Standard `param` syntax; params propagate to signal widths |
| Separate `modport` per perspective (manager vs subordinate) | One declaration, two perspectives (`initiator` / `target`) |

The `bus` construct provides better source-level ergonomics with 100% portable RTL output.

## 19.10  Relation to `process` Blocks

`implement BusName.method rtl` (§19.2.2) uses the same lowering machinery as `process` blocks (§20).  Both compile `wait until`/`fork`-`join`/`for` into synthesizable FSMs.  The difference is scope:

- A `process` block lives inside a `module` and operates on the module's ports and signals.
- An `implement ... rtl` block lives at file scope and defines how a bus method maps to bus signals.

Users writing protocol logic inside a module should prefer `process` over a manual `fsm` when the logic is naturally sequential.
