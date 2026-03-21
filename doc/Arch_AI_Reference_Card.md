**Arch HDL --- AI Reference Card**

*Compact AI context for hardware generation · v0.21.0 · Put this in context, add design intent, paste compiler errors to self-correct.*

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
> socket name: initiator InterfaceName; // TLM initiator (planned)
>
> socket name: target InterfaceName; // TLM target (planned)
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
> comb y = expr; end comb // combinational --- uses =
>
> reg r: T init 0 reset rst sync high; // register decl with reset
>
> reg p: T init 0 reset none; // register decl without reset
>
> reg default: init 0 reset rst; // wildcard default for all regs in scope
>
> reg r: UInt\<8\>; // inherits init/reset from reg default
>
> pipe_reg delayed: source stages 3; // N-stage delay chain, type inferred
>
> seq on clk rising // clocked process --- uses \<=
>
> r \<= expr; // compiler auto-generates if(rst) guard
>
> p \<= expr; // no reset guard (reset none)
>
> end seq
>
> let x: UInt\<32\> = a + b; // combinational wire (explicit type required)

**2. Types**

> UInt\<N\> SInt\<N\> Bool Bit
>
> Clock\<Domain\> Reset\<Sync\|Async, High\|Low\> (polarity defaults High)
>
> Vec\<T,N\> struct S { f: T, } enum E { A, B, }
>
> Bool and UInt\<1\> are identical --- freely assignable, bitwise ops on 1-bit return Bool
>
> Token Future\<T\> Token\<T, id_width: N\> (planned --- TLM only)
>
> Width conversions (always explicit): x.trunc\<N\>() x.trunc\<N,M\>() x.zext\<N\>() x.sext\<N\>()
>
> trunc\<N\>() → lowest N bits (SV: N'(x)); trunc\<N,M\>() → bit range [N:M] (SV: x[N:M])
>
> Arithmetic: UInt\<8\> + UInt\<8\> → UInt\<9\> (auto-widen); must .trunc\<8\>() to assign back
>
> $clog2(expr) supported in type args: UInt\<$clog2(DEPTH)\>

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
| reg default: init 0 reset rst;        | Wildcard default for all regs    |
|                                       |                                  |
| reg r: UInt\<W\>;                     | Inherits init/reset from default |
|                                       |                                  |
| pipe_reg d: r stages 2;              | 2-stage delay of r (read-only)   |
|                                       |                                  |
| seq on clk rising                     | Compiler auto-generates          |
|                                       |                                  |
| r \<= a;                              | if(rst) reset guard from reg decl|
|                                       |                                  |
| end seq                               |                                  |
|                                       |                                  |
| comb y = d; end comb                  |                                  |
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
|   : sum.trunc\<8\>();                 |                                  |
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
| reg r1: T init 0 reset rst;           | Cross-stage refs rewritten:   |
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
| reg r2: T init 0 reset rst;           |                               |
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
| port rst: in Reset\<Sync\>;              | Output ports with `default expr` need     |
|                                          | not be driven in every state — the        |
| port active: out Bool default false;     | compiler emits the default at the top of  |
|                                          | the always\_comb output block; states     |
| port fire\_irq: out Bool default false;  | only override what differs.               |
|                                          |                                           |
| state Idle, Running, Done;               | Ports **without** `default` must be       |
|                                          | driven in every state (compile error      |
| default state Idle;                      | otherwise).                               |
|                                          |                                           |
| state Idle                               | Transition syntax:                        |
|                                          |                                           |
| // no comb block — both stay at default  | transition to Next when \<expr\>;         |
|                                          |                                           |
| transition to Running when start;        | Multiple transitions are checked for      |
| transition to Idle when not start;       | mutual exclusivity; `unique if` emitted   |
|                                          | when exclusive, `priority if` otherwise.  |
| end state Idle                           |                                           |
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

*Arch AI Reference Card · March 2026 · v0.21.0 · arch check is your first line of defence*
