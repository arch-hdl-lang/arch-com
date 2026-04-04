# ARCH Construct Syntax Reference
#
# Format: each construct is delimited by:
#   ### construct_name
#   ...syntax...
#   ### end
#
# The Python server parses this file to populate CONSTRUCT_SYNTAX.

### module
module ModuleName
  param PARAM_NAME: const = 32;
  port clk:   in Clock<SysDomain>;      // SysDomain is built-in
  port rst:   in Reset<Sync>;
  port a:     in UInt<8>;
  port reg q: out UInt<8> reset rst=0;  // port reg: output + register in one

  default seq on clk rising;             // sets default clock for all seq

  let sum: UInt<9> = a + 1;             // let REQUIRES initializer
  wire w: UInt<8>;                       // wire: driven in comb blocks

  comb w = a;                            // one-line comb (single assignment)

  comb                                   // multi-line comb (multiple assignments or if/else)
    w = a;
    q = w + 1;                           // ERROR: q is reg, can only assign in seq
  end comb

  seq q <= a;                            // one-line seq (uses default clock)

  seq                                    // multi-line seq (omits 'on clk' when default is set)
    if rst
      q <= 0;
    else
      q <= a;
    end if
  end seq

  seq on clk falling                     // explicit clock overrides default
    q <= a;
  end seq

  // Value-list for (compile-time unrolled, each value gets its own block):
  comb
    for i in {0, 3, 7, 15}
      mask[i] = true;
    end for
  end comb

  // inside operator (set membership, returns Bool):
  let is_special: Bool = opcode inside {3, 7, 16..31};

  // unique if — assert mutual exclusivity; synthesis emits parallel mux:
  comb
    unique if sel == 0
      y = a;
    else
      y = b;
    end if
  end comb

  // unique match — assert mutual exclusivity; emits SV unique case:
  comb
    unique match opcode
      0 => result = a;
      1 => result = b;
      _ => result = 0;
    end match
  end comb
end module ModuleName
### end

### inst
// Instance syntax — use 'port <- signal' for inputs,
//                       'port -> wire' for outputs.
// Hierarchical references (inst_name.port) are FORBIDDEN.
// All output ports MUST be explicitly connected.

  inst my_inst: ChildModule
    param WIDTH = 16;
    clk   <- clk;
    rst   <- rst;
    data_in  <- input_signal;
    data_out -> output_wire;
  end inst my_inst

  // Then use output_wire in comb/seq blocks (NOT my_inst.data_out)
### end

### fsm
fsm FsmName
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port go:  in Bool;
  port done: out Bool;

  reg cnt: UInt<4> reset rst=0;

  state [Idle, Running, Done]
  default state Idle;                    // reset state
  default seq on clk rising;             // default clock for all seq in states
  default                                // default outputs (overridden per-state)
    comb done = false;
  end default

  state Idle
    -> Running when go;
  end state Idle

  state Running
    comb done = false;                   // override default if needed
    seq
      cnt <= cnt + 1;
    end seq
    -> Done when cnt == 10;
  end state Running

  state Done
    comb done = true;                    // override default
  end state Done                         // no transition = stays in Done
end fsm FsmName
### end

### pipeline
pipeline PipeName
  param DEPTH: const = 3;
  port clk:  in Clock<SysDomain>;
  port rst:  in Reset<Sync>;
  port data_in:  in UInt<32>;
  port data_out: out UInt<32>;

  stage S0
    let x: UInt<32> = data_in + 1;
  end stage S0

  stage S1
    let y: UInt<32> = S0.x + 2;
  end stage S1

  comb
    data_out = S1.y;
  end comb
end pipeline PipeName
### end

### synchronizer
// kind: ff | gray | handshake | reset | pulse
synchronizer SyncName
  kind ff;
  param STAGES: const = 2;
  port src_clk:  in Clock<SrcDomain>;
  port dst_clk:  in Clock<DstDomain>;
  port rst:      in Reset<Async>;
  port data_in:  in Bool;
  port data_out: out Bool;
end synchronizer SyncName
### end

### fifo
fifo FifoName
  param DEPTH: const = 16;
  port wr_clk:  in Clock<WrDomain>;
  port rd_clk:  in Clock<RdDomain>;   // different domain = async FIFO
  port rst:     in Reset<Async>;
  port wr_en:   in Bool;
  port wr_data: in UInt<8>;
  port rd_en:   in Bool;
  port rd_data: out UInt<8>;
  port full:    out Bool;
  port empty:   out Bool;
end fifo FifoName
### end

### ram
// kind: single | simple_dual | true_dual
// latency: 0 (async read) | 1 (sync read) | 2 (output reg)
ram RamName
  kind simple_dual;
  latency 1;
  param DEPTH: const = 256;
  param WIDTH: const = 32;
  port clk:    in Clock<SysDomain>;
  port wr_en:  in Bool;
  port wr_addr: in UInt<8>;
  port wr_data: in UInt<32>;
  port rd_addr: in UInt<8>;
  port rd_data: out UInt<32>;
end ram RamName
### end

### arbiter
// policy: round_robin | priority | weighted<W> | lru | <FnName> (custom)
// Built-in policies handle standard cases; use custom when requirement
// doesn't fit (e.g. QoS-aware, starvation-prevention, custom fairness).

// ── Built-in policy example ──────────────────────────────────────────────
arbiter ArbName
  policy round_robin;
  param N: const = 4;
  port clk:   in Clock<SysDomain>;
  port rst:   in Reset<Sync>;
  ports[N] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid:      out Bool;
  port grant_requester:  out UInt<2>;
end arbiter ArbName

// ── Custom policy example ────────────────────────────────────────────────
// Define the grant function in the SAME file (compiler inlines it into SV).
// Function receives req_mask (one-hot of pending requesters) and last_grant
// (one-hot of previous winner for fairness) and returns one-hot grant mask.
function MyGrant(req_mask: UInt<4>, last_grant: UInt<4>) -> UInt<4>
  // Example: lowest-set-bit with last-grant deprioritization
  let masked: UInt<4> = req_mask & (last_grant ^ 0xF);
  let pick: UInt<4>   = masked != 0 ? masked : req_mask;
  let pick_neg: UInt<5> = (pick ^ 0xF).zext<5>() + 1;
  return pick & pick_neg.trunc<4>();
end function MyGrant

arbiter CustomArbiter
  policy MyGrant;                // <— name of the function above
  param N: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[N] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid:      out Bool;
  port grant_requester:  out UInt<2>;
  // Hook wires the function into the arbiter's grant logic:
  hook grant_select(req_mask: UInt<4>, last_grant: UInt<4>) -> UInt<4>
    = MyGrant(req_mask, last_grant);
end arbiter CustomArbiter
### end

### regfile
regfile RegfileName
  param XLEN: const = 32;
  param REGS: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port rs1_idx: in UInt<5>;
  port rs1_data: out UInt<32>;
  port wr_en:  in Bool;
  port wr_idx: in UInt<5>;
  port wr_data: in UInt<32>;

  init [0] = 0;
  forward write_before_read: false;
end regfile RegfileName
### end

### package
// Package: reusable namespace for enums, structs, functions, params, domains.
// File must be named PkgName.arch; consumer imports with 'use PkgName;'

// BusPkg.arch
package BusPkg
  domain FastClk
    freq_mhz: 500
  end domain FastClk

  enum BusOp
    Read, Write, Idle
  end enum BusOp

  struct BusReq
    op: BusOp;
    addr: UInt<32>;
    data: UInt<32>;
  end struct BusReq

  function max(a: UInt<32>, b: UInt<32>) -> UInt<32>
    return a > b ? a : b;
  end function max
end package BusPkg

// Consumer.arch
use BusPkg;

module Consumer
  port req: in BusReq;
  port addr_out: out UInt<32>;
  comb addr_out = req.addr;
end module Consumer

// SV output:
//   package BusPkg; ... endpackage
//   import BusPkg::*;
//   module Consumer (...); ... endmodule
### end

### bus
// ── Bus declaration: reusable port bundle ──
bus ItcmIcb
  param ADDR_W: const = 14;
  param DATA_W: const = 32;

  cmd_valid: out Bool;          // direction from initiator's perspective
  cmd_addr:  out UInt<ADDR_W>;
  cmd_ready: in  Bool;
  rsp_valid: in  Bool;
  rsp_data:  in  UInt<DATA_W>;
  rsp_ready: out Bool;
end bus ItcmIcb

// ── Using a bus port ──
module Master
  port clk:  in Clock<SysDomain>;
  port rst:  in Reset<Sync>;
  port itcm: initiator ItcmIcb;                    // directions as declared
  // With param overrides:
  // port axi: initiator AxiLite<ADDR_W=32, DATA_W=64>;

  comb
    itcm.cmd_valid = 1;          // dot notation for signal access
    itcm.cmd_addr  = addr_r;
  end comb
end module Master

module Slave
  port clk:  in Clock<SysDomain>;
  port rst:  in Reset<Sync>;
  port itcm: target ItcmIcb;                       // directions FLIPPED (in↔out)

  comb
    itcm.cmd_ready = 1;          // cmd_ready is output for target
    itcm.rsp_valid = 1;
  end comb
end module Slave

// ── Instance connections: use dot notation on port name ──
// inst m: Master
//   itcm.cmd_valid -> cmd_valid_w;
//   itcm.cmd_ready <- cmd_ready_w;
// end inst m

// ── SV output: flattened to individual ports ──
// module Master (
//   output logic        itcm_cmd_valid,    // {port}_{signal}
//   output logic [13:0] itcm_cmd_addr,
//   input  logic        itcm_cmd_ready,
//   ...
// );
### end

### types
// ── Type System ──
// UInt<N>, SInt<N>, Bool, Bit
// Clock<DomainName>, Reset<Sync|Async, High|Low>
// Vec<T, N>, struct StructName / ... / end struct StructName
// enum EnumName / ... / end enum EnumName

// ── Width rules ──
// UInt<8> + UInt<8> → UInt<9>   (result widens by 1)
// No implicit conversions — use .trunc<N>(), .zext<N>(), .sext<N>()
// .trunc<N>() requires N < source width (compiler error otherwise)
// .zext<N>()/.sext<N>() require N > source width (compiler error otherwise)
// signed(x): same-width UInt<N>→SInt<N> reinterpret (SV: $signed(x))
// unsigned(x): same-width SInt<N>→UInt<N> reinterpret (SV: $unsigned(x))
// Use signed() for signed arithmetic chains: signed(a) + signed(b) → SInt<9>
// Bit slice: x[7:4] extracts bits 7 down to 4
// Single bit: x[3] extracts bit 3
// Cast: (x as SInt<32>), (x as UInt<32>)
// Concat: {a, b}   Replication: {4{a}}
// Reduction: &x (AND), |x (OR), ^x (XOR)
// Set membership: expr inside {val1, val2, lo..hi} — returns Bool, emits SV inside
// Ternary: cond ? a : b
// Bit/byte reverse: x.reverse(1) for bit-reverse, x.reverse(8) for byte-reverse

// ── Naming conventions (recommended, NOT compiler-enforced) ──
// Modules/structs/enums: PascalCase (recommended)
// Signals/ports/regs:    snake_case (recommended)
// Params/constants:      UPPER_SNAKE (recommended)
// Module names are emitted as-is in SV — use the exact name the testbench expects
### end
