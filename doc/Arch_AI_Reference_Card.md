**Arch HDL --- AI Reference Card**

*Compact AI context for hardware generation · v0.24.0 · Put this in context, add design intent, paste compiler errors to self-correct.*

**1. Universal Block Schema --- Every Construct Uses This**

> keyword Name
>
> param NAME: const = value; // compile-time constant
>
> param NAME: type = SomeType; // compile-time type parameter
>
> port name: in TypeExpr;
>
> port name: out TypeExpr;
>
> port name: initiator BusName; // bus port (initiator perspective)
>
> port name: target BusName; // bus port (directions flipped)
>
> generate for i in 0..N-1 // generated ports / instances
>
> port p\[i\]: in UInt\<8\>;
>
> end generate for i
>
> generate if PARAM \> 0 // conditional ports
>
> port opt: out Bool;
>
> end generate if
>
> assert name: expression; // (planned)
>
> cover name: expression; // (planned)
>
> end keyword Name
>
> SIGNAL ASSIGNMENT:
>
> comb y = expr; // one-line combinational (no end comb needed)
>
> comb y = expr; end comb // equivalent multi-line form
>
> reg r: T reset rst=0 sync high; // register decl with reset (reset value after =)
>
> reg r: T init 0 reset rst=0; // optional init sets SV declaration initializer
>
> reg p: T reset none; // register decl without reset
>
> reg default: reset rst=0; // wildcard default for all regs in scope
>
> reg r: UInt\<8\>; // inherits reset from reg default
>
> port reg q: out UInt\<8\> reset rst=0; // output port + register combined
>
> port reg q: out UInt\<8\>; // inherits reset from reg default
>
> pipe_reg delayed: source stages 3; // N-stage delay chain, type inferred
>
> default seq on clk rising; // set default clock for all seq blocks
>
> seq on clk rising // clocked process --- uses \<=
>
> r \<= expr; // compiler auto-generates if(rst) guard
>
> p \<= expr; // no reset guard (reset none)
>
> end seq
>
> seq r \<= expr; // one-line seq (uses default clock, no end seq)
>
> let x: UInt\<32\> = a + b; // combinational wire (explicit type required)
>
> CONDITIONALS: use elsif (one word), NOT else if (two words):
>
> if cond\_a r \<= val\_a; elsif cond\_b r \<= val\_b; else r \<= val\_c; end if
>
> FOR LOOPS (runtime, in comb/seq blocks):
>
> for i in 0..7 out\[i\] = data\[7 - i\]; end for // inclusive range, emits SV for loop
>
> for i in {0, 3, 7, 15} mask\[i\] = true; end for // value-list, compile-time unrolled
>
> // Range for = runtime SV loop; value-list for = compile-time unroll; generate for = ports/instances

**2. Types**

> UInt\<N\> SInt\<N\> Bool Bit
>
> Clock\<Domain\> Reset\<Sync\|Async, High\|Low\> (polarity defaults High)
>
> SysDomain is built-in --- no `domain SysDomain end domain SysDomain` needed
>
> Vec\<T,N\> struct S { f: T, } enum E { A, B, }
>
> Bool and UInt\<1\> are identical --- freely assignable, bitwise ops on 1-bit return Bool
>
> Token Future\<T\> Token\<T, id_width: N\> (planned --- TLM only)
>
> Width conversions (always explicit): x.trunc\<N\>() x.zext\<N\>() x.sext\<N\>() x[hi:lo]
>
> trunc\<N\>() → lowest N bits (SV: N'(x)); x[hi:lo] → bit-slice (SV: x[hi:lo])
>
> Arithmetic: UInt\<8\> + UInt\<8\> → UInt\<9\> (auto-widen); must .trunc\<8\>() to assign back
>
> $clog2(expr) supported in type args: UInt\<$clog2(DEPTH)\>
>
> Bit concat: {a, b} → SV {a, b} (MSB first). Bit repeat: {N{expr}} → SV {N{expr}}.
>
> Nestable: {{8{sign\_bit}}, byte\_val} → 16-bit sign-extended value.

**3. Expressions & Operators**

> Arithmetic: + - \* / %
>
> Comparison: == != \< \> \<= \>=
>
> Logical: and or not
>
> Bitwise: & \| ^ ~ \<\< \>\>
>
> Ternary: cond ? a : b (right-associative; chains for priority muxes)
>
> Match expression: match x { E::A => val1, E::B => val2, _ => default }
>
> Set membership: expr inside {val1, val2, lo..hi} — returns Bool, emits SV inside operator
>
> Field access: s.field Array index: a\[i\]
>
> Function call: FnName(arg1, arg2) (overload-resolved by argument types)
>
> Enum variant: E::Variant Struct literal: S { f: val }
>
> Sized literals: 8'hFF 16'd1024 4'b1010 (Verilog-style)
>
> todo! --- compilable placeholder; warns at compile, aborts at sim runtime

**4. Construct Cards**

**module --- combinational or registered logic**

+---------------------------------------+----------------------------------+
| module Name                           | No implicit latches (error)      |
|                                       |                                  |
| param W: const = 8;                   | Single driver per signal (error) |
|                                       |                                  |
| port clk: in Clock\<D\>;              | All ports must be connected      |
|                                       |                                  |
| port rst: in Reset\<Sync\>;           |                                  |
|                                       |                                  |
| port a: in UInt\<W\>;                 |                                  |
|                                       |                                  |
| port y: out UInt\<W\>;                |                                  |
|                                       |                                  |
| reg default: reset rst=0;             | Wildcard default for all regs    |
|                                       |                                  |
| reg r: UInt\<W\>;                     | Inherits reset from default      |
|                                       |                                  |
| port reg y: out UInt\<W\>;            | Output port + register combined  |
|                                       |                                  |
| pipe_reg d: r stages 2;              | 2-stage delay of r (read-only)   |
|                                       |                                  |
| default seq on clk rising;            | Set default clock for seq blocks |
|                                       |                                  |
| seq on clk rising                     | Compiler auto-generates          |
|                                       |                                  |
| r \<= a;                              | if(rst) reset guard from reg decl|
|                                       |                                  |
| y \<= r;                              | port reg assigned directly       |
|                                       |                                  |
| end seq                               |                                  |
|                                       |                                  |
| seq r \<= a;                          | One-line seq (uses default clk)  |
|                                       |                                  |
| end module Name                       |                                  |
|                                       |                                  |
| // Instantiation:                     |                                  |
|                                       |                                  |
| inst u: Name                          |                                  |
|                                       |                                  |
| param W = 16;                         |                                  |
|                                       |                                  |
| connect clk \<- clk;                  |                                  |
|                                       |                                  |
| connect a \<- sig; connect y -\> out; |                                  |
|                                       |                                  |
| end inst u                            |                                  |
|                                       |                                  |
| // Hierarchical refs FORBIDDEN:       | inst_name.port is a compile      |
|                                       | error. Use connect y -> wire     |
| // ✗ add.sum  (compile error)         | in the inst block and reference  |
|                                       | wire in the enclosing scope.     |
| // ✓ connect sum -> my_sum;           |                                  |
|    // then use my_sum                 |                                  |
+---------------------------------------+----------------------------------+

**function --- pure combinational, overloadable**

+---------------------------------------+----------------------------------+
| function AddSat(a: UInt\<8\>,         | Pure comb --- no state, no clk   |
|                 b: UInt\<8\>)          |                                  |
|   -\> UInt\<8\>                       | Overloading: same name, different|
|                                       | arg types (mangled in SV)        |
| let sum: UInt\<9\> = a.zext\<9\>()   |                                  |
|   + b.zext\<9\>();                    | let bindings as temporaries      |
|                                       |                                  |
| return sum\[8\] ? 8'hFF              | Ternary / match in return        |
|   : sum[7:0];                        |                                  |
|                                       | Emits SV: function automatic     |
| end function AddSat                   |                                  |
+---------------------------------------+----------------------------------+

**pipeline --- staged datapath, compiler generates hazard logic**

+---------------------------------------+-------------------------------+
| pipeline Name                         | Compiler generates:           |
|                                       |                               |
| port clk: in Clock\<D\>;              | per-stage valid\_r registers, |
|                                       |                               |
| port rst: in Reset\<Sync\>;           | stall chain (backpressure),   |
|                                       |                               |
| stage Fetch stall when !in\_valid     | flush masks, comb wire decls. |
|                                       |                               |
| reg r1: T reset rst=0;           | Cross-stage refs rewritten:   |
|                                       |                               |
| seq on clk rising                     | Fetch.pc → fetch\_pc          |
|                                       |                               |
| r1 \<= in;                            |                               |
|                                       |                               |
| end seq                               | valid\_r accessible per-stage |
|                                       |                               |
| end stage Fetch                       | for output gating:            |
|                                       |                               |
| stage Exec                            | wb\_we = valid and valid\_r;  |
|                                       |                               |
| reg r2: T reset rst=0;           |                               |
|                                       |                               |
| seq on clk rising                     | Explicit forwarding via comb  |
|                                       |                               |
| r2 \<= Fetch.r1;                      | if/else mux inside stage.     |
|                                       |                               |
| end seq                               |                               |
|                                       |                               |
| inst alu0: Alu                        | inst inside stages supported  |
|                                       |                               |
| connect a \<- Fetch.r1;               | (output wires auto-declared). |
|                                       |                               |
| end inst alu0                         |                               |
|                                       |                               |
| end stage Exec                        |                               |
|                                       |                               |
| flush Fetch when mispredict;          |                               |
|                                       |                               |
| end pipeline Name                     |                               |
+---------------------------------------+-------------------------------+

**fsm --- finite state machine**

+------------------------------------------+-------------------------------------------+
| fsm Name                                 | Compiler checks exhaustive transitions.   |
|                                          |                                           |
| port clk: in Clock\<D\>;                 | `default state` required (reset value).  |
|                                          |                                           |
| port rst: in Reset\<Sync\>;              | `default ... end default` block provides  |
|                                          | default comb/seq assignments emitted      |
| port active: out Bool;                   | before the state case statement; states   |
|                                          | only override what differs.               |
| port fire\_irq: out Bool;               |                                           |
|                                          | Undriven outputs are X (real HW).         |
| state Idle, Running, Done;               |                                           |
|                                          |                                           |
| default state Idle;                      |                                           |
|                                          |                                           |
| default                                  |                                           |
|   comb                                   |                                           |
|     active = false;                      |                                           |
|     fire\_irq = false;                   |                                           |
|   end comb                               |                                           |
| end default                              |                                           |
|                                          |                                           |
| state Idle                               | Transition syntax:                        |
|   transition to Running when start;      |                                           |
| end state Idle                           | transition to Next when \<expr\>;         |
|                                          |                                           |
| // or one-line (no end state needed):    | Multiple transitions are checked for      |
| state Idle                               | mutual exclusivity; `unique if` emitted   |
|   transition to Running when start;      | when exclusive, `priority if` otherwise.  |
|                                          | Implicit hold: if no transition fires,    |
|                                          | FSM stays in current state. No catch-all  |
|                                          | `transition to Self when true` needed.    |
|                                          |                                           |
| state Running                            |                                           |
|                                          |                                           |
| comb active = true; end comb             |                                           |
|                                          |                                           |
| transition to Done when all\_done;       |                                           |
| transition to Running when not all\_done;|                                           |
|                                          |                                           |
| end state Running                        |                                           |
|                                          |                                           |
| state Done                               |                                           |
|                                          |                                           |
| comb fire\_irq = true; end comb          |                                           |
|                                          |                                           |
| transition to Idle when true;            |                                           |
|                                          |                                           |
| end state Done                           |                                           |
|                                          |                                           |
| end fsm Name                             |                                           |
+------------------------------------------+-------------------------------------------+

**fsm datapath extension** --- `reg`, `let`, and `seq` inside FSMs:

FSMs may declare `reg` and `let` at scope level, and `seq on clk rising ... end seq` blocks inside state bodies. The compiler emits separate `always_ff` (state + datapath regs) and `always_comb` (transitions + outputs). This co-locates control and datapath — a readability win over SV's split-block style.

```
fsm MulDiv
  reg acc_r: UInt<64> reset rst=0 sync high;
  let done: Bool = (cycle_r == 31);
  state Idle
    seq on clk rising
      acc_r <= 0;
    end seq
    transition to Multiply when req_valid;
  end state Idle
  ...
end fsm MulDiv
```

**fifo --- sync or dual-clock async (gray-code auto-generated)**

+--------------------------------------------------------+-------------------------------+
| fifo Name                                              | Dual-clock: replace clk with  |
|                                                        |                               |
| param DEPTH: const = 64;                               | port wr_clk: in Clock\<WrD\>; |
|                                                        |                               |
| param WIDTH: type = UInt\<32\>;                        | port rd_clk: in Clock\<RdD\>; |
|                                                        |                               |
| port clk: in Clock\<D\>; // or wr_clk+rd_clk for async | Compiler adds gray-code CDC.  |
|                                                        |                               |
| port rst: in Reset\<Sync\>;                            |                               |
|                                                        |                               |
| port push_valid: in Bool;                              |                               |
|                                                        |                               |
| port push_ready: out Bool;                             |                               |
|                                                        |                               |
| port push_data: in WIDTH;                              |                               |
|                                                        |                               |
| port pop_valid: out Bool;                              |                               |
|                                                        |                               |
| port pop_ready: in Bool;                               |                               |
|                                                        |                               |
| port pop_data: out WIDTH;                              |                               |
|                                                        |                               |
| end fifo Name                                          |                               |
+--------------------------------------------------------+-------------------------------+

**synchronizer --- CDC synchronizer (ff / gray / handshake / reset / pulse)**

> synchronizer Name
>
> kind ff; // ff (default) | gray | handshake | reset | pulse
>
> param STAGES: const = 2; // 2 or 3 (default 2)
>
> port src_clk: in Clock\<SrcDomain\>;
>
> port dst_clk: in Clock\<DstDomain\>; // chain clocked on dst_clk
>
> port rst: in Reset\<Async\>; // optional
>
> port data_in: in Bool; // or UInt\<N\> for multi-bit
>
> port data_out: out Bool;
>
> end synchronizer Name

Strategies:

- `kind ff;` (default) --- N-stage flip-flop shift chain on dst clock. Best for 1-bit signals.
- `kind gray;` --- Binary-to-gray encode, FF chain, gray-to-binary decode. Safe for multi-bit counters/pointers.
- `kind handshake;` --- Req/ack toggle protocol with synchronized control signals. Safe for arbitrary multi-bit data.
- `kind reset;` --- Reset synchronizer: async assert (immediate), sync deassert through N-stage FF chain. Single-bit only (Bool). Used for synchronizing reset deassertion to a clock domain.
- `kind pulse;` --- Pulse synchronizer: converts a single-cycle pulse into a level toggle in the source domain, syncs the toggle through the FF chain, then edge-detects in the destination domain to regenerate a single-cycle pulse. Single-bit only (Bool). Used for events, interrupts, triggers across clock domains.

Notes: two Clock ports must reference different domains (compile error otherwise). SV codegen emits strategy-specific logic; sim codegen generates C++ models for all 5 kinds.

Type checker warnings/errors: `kind ff` with multi-bit data (UInt\<N\> where N>1) warns — use `kind gray` or `kind handshake` instead. `kind reset` and `kind pulse` error if data is not Bool.

CDC detection covers both seq→seq and comb→seq crossings: a comb block reading a register from domain A whose output feeds a seq block in domain B is a compile error. CDC checking also extends across `inst` boundaries — the compiler traces clock port connections to map child domains to parent domains and flags any data connection that crosses domain boundaries without a synchronizer or async fifo.

**ram --- FPGA BRAM / ASIC SRAM**

+-----------------------------------+--------------------------------------+
| ram Name                          | kind: single\|simple_dual\|true_dual |
|                                   |                                      |
| param DEPTH: const = 1024;        | latency 0 = async (comb read)        |
|                                   | latency 1 = sync (1-cycle read)      |
| port clk: in Clock\<D\>;          | latency 2 = sync_out (2-cycle read)  |
|                                   |                                      |
| kind simple_dual;                 | init: zero\|none\|file \'x.hex\'     |
|                                   |                                      |
| latency 1;                        | store: multiple named logical         |
|                                   |                                      |
| store                             | address ranges (planned).            |
|                                   |                                      |
| weights: Vec\<SInt\<8\>, DEPTH\>; |                                      |
|                                   |                                      |
| biases: Vec\<SInt\<16\>, DEPTH\>; |                                      |
|                                   |                                      |
| end store                         |                                      |
|                                   |                                      |
| port rd                           |                                      |
|                                   |                                      |
| en: in Bool; addr: in UInt\<10\>; |                                      |
|                                   |                                      |
| data: out SInt\<8\>;              |                                      |
|                                   |                                      |
| end port rd                       |                                      |
|                                   |                                      |
| port wr                           |                                      |
|                                   |                                      |
| en: in Bool; addr: in UInt\<10\>; |                                      |
|                                   |                                      |
| data: in SInt\<8\>;               |                                      |
|                                   |                                      |
| end port wr                       |                                      |
|                                   |                                      |
| init: zero;                       |                                      |
|                                   |                                      |
| end ram Name                      |                                      |
+-----------------------------------+--------------------------------------+

**counter --- wrap/saturate/gray/one_hot/johnson**

+-----------------------------------+--------------------------------------+
| counter Name                      | mode: wrap\|saturate\|gray\|         |
|                                   |   one_hot\|johnson                   |
| param WIDTH: const = 8;           |                                      |
|                                   | direction: up\|down\|up_down         |
| port clk: in Clock\<D\>;          |                                      |
|                                   | at_max / at_min output ports         |
| port rst: in Reset\<Sync\>;       |                                      |
|                                   |                                      |
| port en: in Bool;                 |                                      |
|                                   |                                      |
| port count: out UInt\<WIDTH\>;    |                                      |
|                                   |                                      |
| port at_max: out Bool;            |                                      |
|                                   |                                      |
| mode: wrap;                       |                                      |
|                                   |                                      |
| direction: up;                    |                                      |
|                                   |                                      |
| end counter Name                  |                                      |
+-----------------------------------+--------------------------------------+

**arbiter --- N requesters, policy-driven grant with hook + latency**

+--------------------------------------------+---------------------------------+
| arbiter Name                               | Built-in policies:              |
|                                            |   round_robin, priority,        |
| param N: const = 4;                        |   lru, weighted                 |
|                                            |                                 |
| port clk: in Clock\<D\>;                   | Custom policy: use function     |
|                                            | name as policy + hook decl:     |
| port rst: in Reset\<Sync\>;                |                                 |
|                                            | policy: MyGrantFn;              |
| ports\[N\] req                             | hook grant_select(              |
|                                            |   req_mask: UInt\<N\>,          |
| valid: in Bool; ready: out Bool;           |   last_grant: UInt\<N\>,        |
|                                            |   extra_port: UInt\<8\>)        |
| end ports req                              |   -\> UInt\<N\>                 |
|                                            |   = MyGrantFn(req_mask,         |
| port grant_valid: out Bool;                |     last_grant, extra_port);    |
|                                            |                                 |
| port grant_requester: out UInt\<$clog2(N)\>; | Hook args bind to:           |
|                                            |   hook params (internal sigs),  |
| policy: round_robin;                       |   user-declared ports/params    |
|                                            |                                 |
| latency 1;  // default: comb grant        | latency 2 = +1 pipeline stage   |
|                                            | latency 3 = +2 pipeline stages  |
| end arbiter Name                           |                                 |
+--------------------------------------------+---------------------------------+

**regfile --- multi-port register file**

+-----------------------------------+--------------------------------------+
| regfile Name                      | Multiple read + write ports          |
|                                   |                                      |
| param DEPTH: const = 32;          | forward write_before_read: true      |
|                                   | enables bypass forwarding            |
| param WIDTH: const = 32;          |                                      |
|                                   | init \[i\] = v; sets reset values   |
| port clk: in Clock\<D\>;          |                                      |
|                                   |                                      |
| port rst: in Reset\<Sync\>;       |                                      |
|                                   |                                      |
| port rd0                          |                                      |
|                                   |                                      |
| addr: in UInt\<5\>;               |                                      |
|                                   |                                      |
| data: out UInt\<WIDTH\>;          |                                      |
|                                   |                                      |
| end port rd0                      |                                      |
|                                   |                                      |
| port wr0                          |                                      |
|                                   |                                      |
| en: in Bool; addr: in UInt\<5\>;  |                                      |
|                                   |                                      |
| data: in UInt\<WIDTH\>;           |                                      |
|                                   |                                      |
| end port wr0                      |                                      |
|                                   |                                      |
| forward write_before_read: false; |                                      |
|                                   |                                      |
| init \[0\] = 0;                   |                                      |
|                                   |                                      |
| end regfile Name                  |                                      |
+-----------------------------------+--------------------------------------+

**linklist --- singly/doubly/circular linked list**

+-------------------------------------------+--------------------------------------+
| linklist Name                             | variant: singly\|doubly\|            |
|                                           |   circular_singly\|circular_doubly   |
| param DEPTH: const = 256;                 |                                      |
|                                           | Operations (via op port):            |
| param DATA_WIDTH: const = 32;             |   insert_head, insert_tail,          |
|                                           |   insert_after, delete_head,         |
| port clk: in Clock\<D\>;                  |   delete, next, prev (doubly),       |
|                                           |   alloc, free, read_data, write_data |
| port rst: in Reset\<Sync\>;               |                                      |
|                                           | Built-in free list + FSM controller  |
| variant: doubly;                          |                                      |
|                                           | 2-cycle latency per operation        |
| end linklist Name                         |                                      |
+-------------------------------------------+--------------------------------------+

**generate --- compile-time ports and instances**

+--------------------------------------+-----------------------------------+
| generate for i in 0..SIZE-1          | Generates REAL named ports.       |
|                                      |                                   |
| port a\[i\]: in SInt\<8\>;           | Caller uses: a\[0\], a\[3\], etc. |
|                                      |                                   |
| inst pe\[i\]: ProcElem               | Type-checked per index.           |
|                                      |                                   |
| connect clk \<- clk;                 | Boundary expression handles       |
|                                      |                                   |
| connect a_in \<- a\[i\];             | chain wiring: i==0 ? 0 : prev     |
|                                      |                                   |
| connect sum_in \<-                   | generate if: port does NOT        |
|                                      |                                   |
| i==0 ? 0 : pe\[i-1\].sum_out;        | exist when condition false.       |
|                                      |                                   |
| end inst pe\[i\]                     |                                   |
|                                      |                                   |
| end generate for i                   |                                   |
|                                      |                                   |
| generate if DEBUG_EN                 |                                   |
|                                      |                                   |
| port dbg: out UInt\<32\>;            |                                   |
|                                      |                                   |
| end generate if                      |                                   |
|                                      |                                   |
| // accessing dbg when DEBUG_EN=false |                                   |
|                                      |                                   |
| // is a COMPILE ERROR                |                                   |
+--------------------------------------+-----------------------------------+

**bus --- reusable port bundle with initiator/target perspectives**

+-------------------------------------------+--------------------------------------+
| bus AxiLite                               | Signals from initiator's perspective |
|                                           |                                      |
| param ADDR_W: const = 32;                 | `initiator` keeps directions         |
|                                           |                                      |
| aw_valid: out Bool;                       | `target` flips all directions        |
|                                           |                                      |
| aw_ready: in Bool;                        | Usage in module:                     |
|                                           |                                      |
| aw_addr: out UInt\<ADDR_W\>;             | port axi: initiator AxiLite;         |
|                                           |                                      |
| end bus AxiLite                           | port axi: target AxiLite;            |
|                                           |                                      |
|                                           | Access: axi.aw\_valid (dot notation) |
|                                           |                                      |
|                                           | SV: flattened axi\_aw\_valid, etc.   |
+-------------------------------------------+--------------------------------------+

**template --- user-defined interface contract**

+-------------------------------------------+--------------------------------------+
| template MyInterface                      | Compile-time only --- no SV emitted  |
|                                           |                                      |
| param NUM_REQ: const;                     | Defines required params, ports,      |
|                                           | and hooks that implementing          |
| port clk: in Clock\<D\>;                  | modules must provide.                |
|                                           |                                      |
| port rst: in Reset\<Sync\>;               | Missing any required item is a       |
|                                           | compile error.                       |
| port grant_valid: out Bool;               |                                      |
|                                           | Modules opt in with:                 |
| hook grant_select(                        | module Foo implements MyInterface    |
|   req_mask: UInt\<4\>)                    |                                      |
|   -\> UInt\<4\>;                          | Hooks in template: signature only    |
|                                           | Hooks in module: + binding           |
| end template MyInterface                  |   = FnName(args);                    |
+-------------------------------------------+--------------------------------------+

**package --- reusable type/function namespace**

+-------------------------------------------+--------------------------------------+
| package BusPkg                            | Contains: enum, struct, function,    |
|                                           | param declarations only.             |
| enum BusOp                                |                                      |
|   Read, Write, Idle                       | No modules/pipelines/FSMs inside.    |
| end enum BusOp                            |                                      |
|                                           | File: PkgName.arch (one package      |
| struct BusReq                             | per file, name must match).          |
|   op: BusOp;                              |                                      |
|   addr: UInt\<32\>;                       | Consumer imports with:               |
|   data: UInt\<32\>;                       |   use BusPkg;                        |
| end struct BusReq                         |                                      |
|                                           | Emits SV:                            |
| function max(a: UInt\<32\>,               |   package BusPkg; ... endpackage     |
|              b: UInt\<32\>)               |   import BusPkg::*;                  |
|   -> UInt\<32\>                           |                                      |
| return a > b ? a : b;                     | Resolved from same directory or      |
| end function max                          | multi-file command line.             |
|                                           |                                      |
| end package BusPkg                        |                                      |
+-------------------------------------------+--------------------------------------+

**5. Logging**

> log(Level, "TAG", "format %0d", arg);
>
> Levels: Always, Low, Medium, High, Full, Debug
>
> Works in seq and comb blocks
>
> Runtime control: +arch_verbosity=N (0=Always only ... 5=Debug)
>
> NBA semantics in seq: value printed is last cycle's registered value

**6. TLM Concurrency Modes (planned)**

> blocking ret: T directly --- caller suspends until done --- APB/MMIO
>
> pipelined ret: Future\<T\> --- issue many, await later --- AXI in-order
>
> out_of_order ret: Token\<T,id: N\> --- any-order response by ID --- Full AXI
>
> burst ret: Future\<Vec\<T,L\>\>--- one AR, N data beats --- AXI INCR burst
>
> await f // wait for one Future
>
> await_all(f0,f1,f2) // wait for all
>
> await_any(t0,t1) // first Token to complete (out_of_order only)

**7. Simulation & Build**

> arch check F.arch // type-check only
>
> arch build F.arch // emit SystemVerilog
>
> arch build F.arch -o out.sv // combined output
>
> arch build a.arch b.arch // multi-file: one .sv per input
>
> arch sim F.arch --tb F_tb.cpp // compile + run C++ testbench
>
> arch sim F.arch --tb F_tb.cpp --outdir build/
>
> arch sim F.arch --tb F_tb.cpp --check-uninit // warn on reads of uninitialized reset-none regs
>
> arch sim F.arch --tb F_tb.cpp --cdc-random // randomize synchronizer latency; cdc_skip_pct (0-100, default 25) controllable from testbench
>
> arch sim F.arch // generate models only (no testbench)
>
> arch formal F.arch // emit SMT-LIB2 (planned)

> **arch sim C++ testbench interface (Verilator-compatible)**
>
> // Ports map to public fields: dut->clk, dut->rst, dut->data_in
>
> // Wide ports (UInt\<N\> where N\>64) use VlWide\<WORDS\>:
>
> // dut->key.data() returns uint32_t\* (word 0 = LSB, word N-1 = MSB)
>
> // Standard cycle loop:
>
> auto tick = \[\&\]() \{ dut->clk=0; dut->eval(); dut->clk=1; dut->eval(); \};
>
> // Multi-clock modules: compiler auto-generates tick() from domain freq\_mhz.
>
> // tick() toggles each clock at the correct frequency ratio and calls eval().
>
> dut.tick(); // auto-toggles fast\_clk (200MHz) and slow\_clk (50MHz) at 4:1 ratio
>
> // Manual control also works — each seq block fires only on its own clock's rising edge.
>
> // Compile: g++ -std=c++17 build/verilated.cpp build/V\*.cpp tb.cpp -Ibuild -o sim

**8. AI Prompting Patterns**

> 1\. CONSTRUCT-FIRST (most reliable)
>
> 'Generate an Arch fifo named InstrQueue, depth 64, element type
>
> InstrPacket, single clock SysDomain. Add cover push_when_full.'
>
> 2\. todo! SCAFFOLDING
>
> 'Generate the skeleton for a 5-stage RISC-V pipeline.
>
> Use todo! for all stage bodies.'
>
> 3\. PASTE COMPILER ERRORS
>
> 'Fix this Arch error: \[paste arch check output\]'
>
> Errors are self-sufficient --- no spec lookup needed.
>
> 4\. ONE CONSTRUCT PER PROMPT
>
> structs → functions → primitives → pipeline → top module → testbench
>
> Compile and verify each before moving to the next.
>
> 5\. ABSTRACTION PROGRESSION
>
> Start --tlm-lt. Add rtl_accurate only after function verified.

**9. ARCH MCP Server**

> Tools available when running under the ARCH MCP server:
>
> `get_construct_syntax(construct)` — returns syntax template + reserved keywords for any construct
>
> `write_and_check(path, content)` — write .arch file + type-check in one call
>
> `arch_build_and_lint(files, top_module)` — build SV + Verilator lint in one call
>
> Recommended AI workflow: fetch syntax first → write_and_check → arch_build_and_lint

*Arch AI Reference Card · March 2026 · v0.24.0 · arch check is your first line of defence*
