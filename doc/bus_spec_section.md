# 19.  First-Class Construct: bus

A `bus` is a reusable, parameterized port bundle for RTL signal-level interfaces.  It replaces the repetitive individual `port` declarations that make SoC interconnects brittle and verbose.  The bus defines signal names, types, and directions from the **initiator's perspective**.  At the use site, `initiator` keeps directions as declared; `target` flips every `in` to `out` and every `out` to `in`.  The compiler flattens bus ports to individual SystemVerilog ports — 100% portable output with no SV `interface` or `modport` constructs.

## 19.1  Declaration

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

## 19.2  Using a Bus Port

```
module Ift2Icb
  port clk:   in Clock<SysDomain>;
  port rst_n: in Reset<Async, Low>;
  port itcm:  initiator ItcmIcb;          // directions as declared
  port ifu:   target    IfuFetchBus;       // directions flipped
  ...
end module Ift2Icb
```

| Keyword | Meaning | Effect on `out` signals | Effect on `in` signals |
|---|---|---|---|
| `initiator` | This module drives the bus | `out` (output) | `in` (input) |
| `target` | This module receives the bus | `in` (input) | `out` (output) |

**Parameter overrides** are specified inline: `port axi: initiator AxiLite<ADDR_W=32, DATA_W=64>;`.  Unspecified parameters use their defaults from the bus declaration.

## 19.3  Signal Access

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

## 19.4  Instance Connections

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

## 19.5  SystemVerilog Output

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

## 19.6  Type Checker Guarantees

| Check | Description |
|---|---|
| Bus exists | The bus name in `initiator BusName` must resolve to a `bus` declaration |
| Signal exists | `itcm.xyz` errors if `xyz` is not a declared signal in the bus |
| Direction correctness | Assigning to an input signal or reading an undriven output is an error |
| Per-signal drive coverage | Each output signal of a bus port must be driven — the checker expands the bus and verifies each flattened signal individually |
| Parameter validation | Overridden params must exist in the bus declaration |

## 19.7  Simulation Codegen

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

## 19.8  Bus vs Interface

| Feature | `bus` | `interface` (§24) |
|---|---|---|
| Purpose | RTL signal-level port bundles | TLM method-level abstractions |
| Perspective | `initiator` / `target` | `out` / `in` on port declaration |
| TLM methods | No | Yes — `blocking`, `pipelined`, `out_of_order`, `burst` |
| SV output | Flattened individual ports | Flattened individual ports (RTL) or method calls (TLM) |
| Use when | Connecting RTL modules with standard bus protocols | Modeling transaction-level communication |

Both constructs share the same design principle: declare the bundle once, use it everywhere, let the compiler manage direction flipping and port expansion.  `bus` is the lightweight RTL-only form; `interface` is the full TLM-capable form.
