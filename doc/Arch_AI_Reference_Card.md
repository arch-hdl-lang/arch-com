**Arch HDL --- AI Reference Card**

*Compact AI context for hardware generation · v0.1 · Put this in context, add design intent, paste compiler errors to self-correct.*

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
> socket name: initiator InterfaceName; // TLM initiator
>
> socket name: target InterfaceName; // TLM target
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
> assert name: expression;
>
> cover name: expression;
>
> end keyword Name
>
> SIGNAL ASSIGNMENT:
>
> comb y = expr; end comb // combinational --- uses =
>
> reg r: T init 0; // register decl
>
> reg on clk rising, rst high // register update --- uses \<=
>
> if rst r \<= 0; end if
>
> else r \<= expr; end else
>
> end reg

**2. Types**

> UInt\<N\> SInt\<N\> Bool Bit
>
> Clock\<Domain\> Reset\<Sync\|Async\>
>
> Vec\<T,N\> struct S { f: T, } enum E { A, B, }
>
> Token Future\<T\> Token\<T, id_width: N\>
>
> Width conversions (always explicit): x.trunc\<N\>() x.zext\<N\>() x.sext\<N\>()

**3. Construct Cards**

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
| reg r: UInt\<W\> init 0;              |                                  |
|                                       |                                  |
| reg on clk rising, rst high           |                                  |
|                                       |                                  |
| if rst r \<= 0; end if                |                                  |
|                                       |                                  |
| else r \<= a; end else                |                                  |
|                                       |                                  |
| end reg                               |                                  |
|                                       |                                  |
| comb y = r; end comb                  |                                  |
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

**pipeline --- staged datapath, compiler generates hazard logic**

+-------------------------------+-------------------------------+
| pipeline Name                 | Compiler generates:           |
|                               |                               |
| port clk: in Clock\<D\>;      | valid/stall propagation,      |
|                               |                               |
| port rst: in Reset\<Sync\>;   | flush masks, forward muxes.   |
|                               |                               |
| stage S1                      | todo! compiles --- use for    |
|                               |                               |
| reg r1: T init 0;             | scaffolding before filling in |
|                               |                               |
| reg on clk rising, rst high   | stage bodies.                 |
|                               |                               |
| if rst r1 \<= 0; end if       |                               |
|                               |                               |
| else r1 \<= in; end else      |                               |
|                               |                               |
| end reg                       |                               |
|                               |                               |
| end stage S1                  |                               |
|                               |                               |
| stage S2 todo! end stage S2   |                               |
|                               |                               |
| stall when !in_valid;         |                               |
|                               |                               |
| flush S2 when mispredict;     |                               |
|                               |                               |
| forward result from wb_result |                               |
|                               |                               |
| when wb_valid;                |                               |
|                               |                               |
| end pipeline Name             |                               |
+-------------------------------+-------------------------------+

**fsm --- finite state machine**

+-----------------------------+-----------------------------------+
| fsm Name                    | Compiler checks exhaustive        |
|                             |                                   |
| port clk: in Clock\<D\>;    | transitions --- every state needs |
|                             |                                   |
| port rst: in Reset\<Sync\>; | a default or full coverage.       |
|                             |                                   |
| state Idle                  | reset_state required.             |
|                             |                                   |
| comb out = false; end comb  |                                   |
|                             |                                   |
| on in == true -\> Active;   |                                   |
|                             |                                   |
| end state Idle              |                                   |
|                             |                                   |
| state Active                |                                   |
|                             |                                   |
| comb out = true; end comb   |                                   |
|                             |                                   |
| on in == false -\> Idle;    |                                   |
|                             |                                   |
| default -\> Active;         |                                   |
|                             |                                   |
| end state Active            |                                   |
|                             |                                   |
| reset_state: Idle;          |                                   |
|                             |                                   |
| end fsm Name                |                                   |
+-----------------------------+-----------------------------------+

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
| param DEPTH: const = 1024;        | read: async\|sync\|sync_out          |
|                                   |                                      |
| port clk: in Clock\<D\>;          | init: zero\|none\|file \'x.hex\'     |
|                                   |                                      |
| kind simple_dual;                 | store: multiple named logical        |
|                                   |                                      |
| read: sync;                       | vars --- compiler auto-assigns       |
|                                   |                                      |
| store                             | address ranges.                      |
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

**arbiter --- N requesters, policy-driven grant**

+----------------------------------+---------------------------------+
| arbiter Name                     | policy: round_robin \| priority |
|                                  |                                 |
| param N: const = 4;              | \| weighted\<W\> \| lru         |
|                                  |                                 |
| port clk: in Clock\<D\>;         | \| custom fn(mask)-\>mask       |
|                                  |                                 |
| port rst: in Reset\<Sync\>;      |                                 |
|                                  |                                 |
| ports\[N\] req                   |                                 |
|                                  |                                 |
| valid: in Bool; ready: out Bool; |                                 |
|                                  |                                 |
| data: in UInt\<32\>;             |                                 |
|                                  |                                 |
| end ports req                    |                                 |
|                                  |                                 |
| ports\[1\] grant                 |                                 |
|                                  |                                 |
| valid: out Bool; ready: in Bool; |                                 |
|                                  |                                 |
| data: out UInt\<32\>;            |                                 |
|                                  |                                 |
| sel: out UInt\<\$clog2(N)\>;     |                                 |
|                                  |                                 |
| end ports grant                  |                                 |
|                                  |                                 |
| policy: round_robin;             |                                 |
|                                  |                                 |
| end arbiter Name                 |                                 |
+----------------------------------+---------------------------------+

**generate --- compile-time ports and instances (impossible in SV)**

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

**4. TLM Concurrency Modes**

> blocking ret: T directly --- caller suspends until done --- APB/MMIO
>
> pipelined ret: Future\<T\> --- issue many, await later --- AXI in-order
>
> out_of_order ret: Token\<T,id: N\> --- any-order response by ID --- Full AXI
>
> burst ret: Future\<Vec\<T,L\>\>--- one AR, N data beats --- AXI INCR burst
>
> !! timing: N is NOT cycle-accurate --- no backpressure, optimistic throughput !!
>
> !! For cycle accuracy: implement \... rtl_accurate on BOTH initiator AND target !!
>
> await f // wait for one Future
>
> await_all(f0,f1,f2) // wait for all
>
> await_any(t0,t1) // first Token to complete (out_of_order only)

**5. Simulation Flags**

> arch check F.arch // type-check only
>
> arch sim Tb.arch // run (single core)
>
> arch sim \--parallel Tb.arch // all cores
>
> arch sim \--tlm-lt // max speed, no timing
>
> arch sim \--tlm-at // ns-accurate AT timing
>
> arch sim \--tlm-rtl // full signal fidelity
>
> arch sim \--wave out.fst // waveform (GTKWave/Surfer)
>
> arch build F.arch // emit SystemVerilog
>
> arch formal F.arch // emit SMT-LIB2

**6. AI Prompting Patterns**

> 1\. CONSTRUCT-FIRST (most reliable)
>
> \'Generate an Arch fifo named InstrQueue, depth 64, element type
>
> InstrPacket, single clock SysDomain. Add cover push_when_full.\'
>
> 2\. todo! SCAFFOLDING
>
> \'Generate the skeleton for a 5-stage RISC-V pipeline.
>
> Use todo! for all stage bodies.\'
>
> 3\. PASTE COMPILER ERRORS
>
> \'Fix this Arch error: \[paste arch check output\]\'
>
> Errors are self-sufficient --- no spec lookup needed.
>
> 4\. ONE CONSTRUCT PER PROMPT
>
> structs → primitives → pipeline → top module → testbench
>
> Compile and verify each before moving to the next.
>
> 5\. ABSTRACTION PROGRESSION
>
> Start \--tlm-lt. Add rtl_accurate only after function verified.

*Arch AI Reference Card · March 2026 · arch check is your first line of defence*
