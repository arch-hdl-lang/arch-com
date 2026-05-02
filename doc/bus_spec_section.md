# 19.  First-Class Construct: bus

A `bus` is a reusable, parameterized port bundle that eliminates repetitive port declarations across modules.  It serves two roles:

1. **RTL signal bundle** (implemented) — declares signal names, types, and directions.  The compiler flattens bus ports to individual SystemVerilog ports.
2. **TLM method interface** (implemented subset) — declares transaction-level methods (`blocking`, plus tagged `out_of_order tags N`) on top of the signal bundle. The compiler lowers method calls to RTL handshake wires, so the same design works in `arch build`, `arch sim`, and `arch sim --thread-sim parallel`.

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

## 19.2  Declaration — TLM Methods

A bus may optionally include `tlm_method` declarations. Methods coexist with RTL signals: the bus still flattens to ordinary request/response wires, and the method syntax is a compact way to generate the initiator and target state machines.

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

  // TLM methods
  tlm_method write(addr: UInt<ADDR_W>, data: UInt<DATA_W>) -> UInt<2>: blocking;
  tlm_method read(addr: UInt<ADDR_W>) -> UInt<DATA_W>: blocking;
  tlm_method read_ooo(addr: UInt<ADDR_W>) -> UInt<DATA_W>: out_of_order tags 4;
end bus AxiLite
```

### 19.2.1  Method Concurrency Modes

| Mode | Return Type | Caller Behaviour | Use Case |
|---|---|---|---|
| `blocking` | `T` directly | A single call waits for its response. Multiple direct worker calls may be outstanding when expressed as a thread cohort; responses route by issue-order FIFO. | Simple MMIO, request/response memory models, in-order worker pools |
| `out_of_order tags N` | `T` directly plus hidden tag wires | A worker cohort may receive responses in any order. The compiler assigns a tag per worker and routes by `<method>_rsp_tag`. | Small tagged memory/model interfaces |
| `pipelined` | — | Not supported as a separate TLM mode. Use ordinary worker threads, `generate_for`, or direct-call `fork/join` cohorts. | Deferred |
| `burst` | — | Not supported. Model explicit beats with thread/RTL protocol code for now. | Deferred |

There is no current `Future<T>`, `await`, or user-visible `Token<T>` API. The source remains blocking-style at each worker:

```
thread w0 on clk rising, rst high
  data0 <= m.read(32'h1000);
end thread w0
```

Concurrency comes from the surrounding thread structure, not from returning a future value.

### 19.2.2  Flattened Wire Protocol

For `tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;`, an initiator port `m` flattens to:

| Wire | Initiator direction | Meaning |
|------|---------------------|---------|
| `m_read_req_valid` | out | request valid |
| `m_read_addr` | out | request argument |
| `m_read_req_ready` | in | target accepted request |
| `m_read_rsp_valid` | in | response valid |
| `m_read_rsp_data` | in | response payload, omitted for void methods |
| `m_read_rsp_ready` | out | initiator accepted response |

For `out_of_order tags N`, two more wires are present:

| Wire | Initiator direction | Meaning |
|------|---------------------|---------|
| `m_read_req_tag` | out | compiler-assigned worker tag |
| `m_read_rsp_tag` | in | target response tag |

The `target` perspective flips every direction.

### 19.2.3  Target Threads

A target implements a method with a dotted-name thread:

```
module MemTarget
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port s: target Mem;
  port ready: in Bool;

  thread s.read(addr) on clk rising, rst high
    wait until ready;
    return 64'h42;
  end thread s.read
end module MemTarget
```

The compiler lowers the target thread into ordinary state registers and comb/seq blocks in the parent module. For tagged OOO methods, the target latches `req_tag` with the arguments and echoes it on `rsp_tag`.

### 19.2.4  Initiator Cohorts

Direct TLM calls inside multiple workers can share one method:

```
module LoadPair
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port m: initiator Mem;
  reg d0: UInt<64> reset rst => 0;
  reg d1: UInt<64> reset rst => 0;

  thread workers on clk rising, rst high
    fork
      d0 <= m.read(32'h10);
    and
      d1 <= m.read(32'h20);
    join
  end thread workers
end module LoadPair
```

Supported cohort shapes:

- Multiple direct named worker threads using the same method.
- `generate_for` workers.
- One direct-call `fork ... and ... join` thread.

Current restrictions:

- Each worker/branch body is exactly one direct assignment: `dst <= port.method(args);`.
- All workers in the cohort use the same clock/reset.
- `out_of_order tags N` requires a literal tag count and enough tags for all workers.
- Nested TLM calls and composed call expressions are rejected.

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

## 19.10  Relation to `thread` Blocks

`implement BusName.method rtl` (§19.2.2) uses the same lowering machinery as `thread` blocks (§20).  Both compile `wait until`/`fork`-`join`/`for` into synthesizable FSMs.  The difference is scope:

- A `thread` block lives inside a `module` and operates on the module's ports and signals.
- An `implement ... rtl` block lives at file scope and defines how a bus method maps to bus signals.

Users writing protocol logic inside a module should prefer `thread` over a manual `fsm` when the logic is naturally sequential.
