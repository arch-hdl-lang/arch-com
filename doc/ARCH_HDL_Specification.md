**ARCH**

Hardware Description Language

Language Specification · v0.41.0 · April 2026

*A purpose-built micro-architecture HDL --- clean RTL semantics, strong types, and first-class pipelines, FSMs, FIFOs, and arbiters. Incorrect design patterns --- multiple drivers, undriven ports, clock-domain crossings, width mismatches --- are compile-time errors, never runtime surprises. Designed to be generated correctly by AI without prior training.*

**1. Design Philosophy**

Arch is a hardware description language built from first principles for micro-architecture work. It carries no host-language baggage --- no JVM, no Python runtime, no Scala implicits. Every construct maps directly to a concrete hardware structure. The compiler emits deterministic, readable SystemVerilog compatible with any standard EDA tool.

  ---------------------- ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
      **No Baggage**     No JVM, no Python runtime, no Scala implicits. Every keyword maps directly to a hardware structure. Nothing is a library pattern hiding behind operator overloading.

     **Strong Types**    Bit widths, clock domains, port directions, and signal ownership are tracked statically. Every mismatch is a compile-time error --- never a simulation-time surprise.

   **Micro-Arch First**  Pipeline, FSM, FIFO, Arbiter, and RegFile are first-class language keywords with compiler-verified semantics --- not user-defined patterns bolted onto raw RTL.

    **AI-Generatable**   Uniform schema, named block endings, English keywords, no braces, and a todo! escape hatch let any LLM produce valid, correct Arch code from a natural-language description --- without fine-tuning.

   **Predictable RTL**   One Arch construct always produces the same SystemVerilog structure. Designers can audit every output line. Synthesis tools see clean, idiomatic RTL.
  ---------------------- ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**1.1 The AI-Generatability Contract**

Arch makes a hard design commitment: a large language model that has read this specification and nothing else must be able to generate structurally correct, type-safe Arch from a plain-English hardware description. Every syntactic choice serves this goal:

- **Uniform schema:** every construct uses the identical param / port / body / verification layout. Learn it once; apply it everywhere.

- **LL(1) grammar:** the next token always determines the parse action --- no backtracking, no multi-token lookahead. This means every token sequence either parses to exactly one AST or is caught immediately as an error. SV requires unbounded lookahead and context-dependent parsing. See §2.4.

- **Named block endings:** every block closes with end keyword name. The AI never loses context in deeply nested structures.

- **No braces:** the keyword+name header and end keyword name are the sole delimiters. There is no redundant { to emit or forget.

- **Intention-first syntax:** the construct kind always comes first --- pipeline Decode, fsm TrafficLight. What it is precedes all detail.

- **One assignment, one meaning:** comb blocks use = (combinational); reg blocks use \<= (registered). No context-dependent semantics.

- **todo! escape hatch:** any expression or block body may be replaced with todo! to produce a compilable, type-checked skeleton.

- **Required explicitness:** widths, domains, directions, and policies are always written out. There is no implicit default the AI must remember.

**1.2 Why First-Class Constructs vs Library Modules**

A natural question is: why bake FIFO, FSM, pipeline, arbiter, RAM, counter, regfile, and synchronizer into the language instead of shipping them as a standard library of SystemVerilog modules? There are six reasons Arch's approach wins for micro-architecture work.

**1.2.1 They are families of designs, not single modules**

A library FIFO typically ships as one or two parameterized modules. A real FIFO has a combinatorial explosion of variants:

- Synchronous vs dual-clock asynchronous (needs gray-code CDC — completely different RTL)
- `kind fifo` (block-when-full) vs `OVERFLOW=1` (overwrite oldest)
- `kind lifo` (stack semantics)
- Show-ahead vs registered output
- Bypass (zero-cycle) variant for depth=1

A library approach either ships N differently-named modules (`sync_fifo`, `async_fifo`, `lifo`, `fifo_overflow`, ...) — which users pick wrong — or one mega-module with a dozen parameters that mostly conflict. Arch's first-class `fifo` with a `kind` selector and auto-detected CDC (from distinct `Clock<D>` ports) picks the right RTL at codegen time. Illegal combinations become compile errors.

**1.2.2 Correctness gates a library cannot enforce**

First-class constructs let the type-checker use construct-specific knowledge:

- **FIFO async detection**: two `Clock<D1>` and `Clock<D2>` ports on a `fifo` auto-insert gray-code pointer synchronization. A library `fifo.sv` sees only wires — it cannot know whether the two clocks are in the same domain.
- **Arbiter fairness**: `policy: round_robin | priority | lru | weighted<W>` is a compile-time enumerated choice. The `policy custom <Fn>` path requires a user-defined function that is type-checked to return a one-hot grant mask of the correct width.
- **Clock-domain crossings**: `synchronizer` with `kind ff | gray | handshake | reset | pulse` emits the right RTL for the signal type (1-bit, multi-bit counter, arbitrary data, reset, pulse) and rejects unsafe combinations (e.g. `kind ff` on multi-bit data emits a warning).
- **Counter bounds**: the counter's `MAX` parameter is used both at runtime and in an auto-generated `count_r <= MAX` SVA assertion. Override-safe.

**1.2.3 Interface contracts are part of the construct**

A FIFO is not just storage — it is a push/pop handshake contract:

```
port push_valid: in Bool; port push_ready: out Bool; port push_data: in DATA_T;
port pop_valid:  out Bool; port pop_ready:  in Bool; port pop_data:  out DATA_T;
```

As a first-class construct, the compiler verifies both sides exist, enforces data-type consistency via a type parameter (`param WIDTH: type = UInt<32>`), and emits well-known signal names so downstream code doesn't guess. A library module makes the handshake convention implicit; every user either gets it right or introduces a subtle deadlock.

**1.2.4 Auto-generated assertions and coverage**

Because the compiler knows what a FIFO / counter / FSM is, it emits safety properties automatically:

- `_auto_no_overflow` / `_auto_no_underflow` on every FIFO
- `_auto_count_range` (`count_r <= MAX`) on every counter
- `_auto_legal_state` (`state_r < N`) plus per-state reachability covers and per-transition covers on every FSM
- Per-module and per-construct user `assert` / `cover` properties

All wrapped in `translate_off`/`on` for synthesis portability, and consumable by Verilator `--assert` and EBMC formal verification. A library `fifo.sv` would require every user to remember to add these — and each would write them slightly differently.

**1.2.5 Multi-backend code generation**

Arch targets multiple backends (SystemVerilog today, C++ simulation in `arch sim`, SMT-LIB2 formal in the roadmap). First-class constructs let the compiler emit the *right* representation per backend:

- A sim FIFO uses a C++ `std::deque` with protocol-level push/pop checking
- A formal FIFO uses symbolic state with cover/assert properties
- An RTL FIFO uses flops with gray-code CDC when asynchronous

A library FIFO is one SystemVerilog blob that doesn't port to non-RTL backends.

**1.2.6 Template fallback for non-pattern reuse**

For genuinely library-style reuse that doesn't fit a built-in pattern — custom protocols, domain-specific engines, third-party IP wrappers — Arch provides `template Name ... end template Name` and `module X implements Name`. The built-in constructs cover the common hardware patterns with strong safety; templates cover everything else.

**In summary**: first-class constructs for the dozen common micro-architecture patterns let the compiler enforce correctness (CDC, handshake, bounds), auto-emit assertions and coverage, generate backend-appropriate code, and give actionable error messages — things a black-box library module cannot provide. Templates exist for everything else.

**2. Lexical Conventions**

**2.1 Naming Rules (recommended, not compiler-enforced)**

  -------------------------------------------------------------------------------------------------------
  **Category**                              **Convention**               **Examples**
  ----------------------------------------- ---------------------------- --------------------------------
  **Modules, interfaces, structs, enums**   PascalCase                   FetchUnit, AluOp, DecodedInstr

  **Signals, registers, ports, locals**     snake_case                   pc_next, mem_addr, req_valid

  **Parameters and constants**              UPPER_SNAKE                  XLEN, CACHE_DEPTH, NUM_PORTS

  **Clock ports**                           typed Clock\<Domain\>        clk: in Clock\<SysDomain\>

  **Reset ports**                           typed Reset\<Sync\|Async, High\|Low\>   rst: in Reset\<Sync\> (polarity defaults High; e.g. Reset\<Sync, Low\> for active-low)
  -------------------------------------------------------------------------------------------------------

**2.2 Literals**

+-----------------------------------------------------------------------------------+
| *literals.arch*                                                                   |
|                                                                                   |
| // All literals require explicit type ascription --- no implicit widths           |
|                                                                                   |
| **let** a: UInt\<8\> = 255;                                                       |
|                                                                                   |
| **let** b: UInt\<8\> = 0xFF;                                                      |
|                                                                                   |
| **let** c: UInt\<8\> = 0b1111_1111; // underscores allowed anywhere               |
|                                                                                   |
| // Sized literals: width\'format\'value --- compiler checks against ascribed type |
|                                                                                   |
| **let** d: UInt\<4\> = 4\'b1010;                                                  |
|                                                                                   |
| **let** e: UInt\<16\> = 16\'hDEAD;                                                |
|                                                                                   |
| **let** f: SInt\<12\> = 12\'sd-42;                                                |
|                                                                                   |
| **let** g: Bool = true;                                                           |
|                                                                                   |
| **let** h: Bool = false;                                                          |
+-----------------------------------------------------------------------------------+

**2.3 Block Structure --- The Core Grammar Rule**

Every compound construct in Arch opens with a keyword-and-name header on its own line and closes with a matching end keyword name. No braces are used anywhere. This rule applies uniformly to every construct in the language without exception.

+--------------------------------------------------------------------+
| *block_structure.arch*                                             |
|                                                                    |
| // The universal block grammar:                                    |
|                                                                    |
| //                                                                 |
|                                                                    |
| // keyword Name                                                    |
|                                                                    |
| // \... body \...                                                  |
|                                                                    |
| // end keyword Name                                                |
|                                                                    |
| //                                                                 |
|                                                                    |
| // Examples across every construct type:                           |
|                                                                    |
| **module** Alu                                                     |
|                                                                    |
| // body                                                            |
|                                                                    |
| **end** **module** Alu                                             |
|                                                                    |
| **pipeline** Decode                                                |
|                                                                    |
| **stage** Fetch                                                    |
|                                                                    |
| // nested body                                                     |
|                                                                    |
| **end** **stage** Fetch                                            |
|                                                                    |
| **end** **pipeline** Decode                                        |
|                                                                    |
| **fifo** TxBuffer                                                  |
|                                                                    |
| // body                                                            |
|                                                                    |
| **end** **fifo** TxBuffer                                          |
|                                                                    |
| **fsm** TrafficLight                                               |
|                                                                    |
| **state** Red                                                      |
|                                                                    |
| // body                                                            |
|                                                                    |
| **end** **state** Red                                              |
|                                                                    |
| **end** **fsm** TrafficLight                                       |
|                                                                    |
| **synchronizer** EventSync                                         |
|                                                                    |
| **kind** pulse;                                                    |
|                                                                    |
| // body                                                            |
|                                                                    |
| **end** **synchronizer** EventSync                                 |
+--------------------------------------------------------------------+

> *⚑ No braces, no ambiguity. The opening keyword+name and closing end keyword name are always a matched pair. An AI generating code cannot accidentally close the wrong block.*

**2.4 LL(1) Grammar --- Why It Matters for AI**

Arch's grammar is **LL(1)**: at every point during parsing, the next single token unambiguously determines which production rule to apply. There is no backtracking, no multi-token lookahead, and no context-dependent parsing.

**What LL(1) means concretely:**

| Token seen | Parser action |
|------------|---------------|
| `module` | Parse a module declaration |
| `fsm` | Parse an FSM declaration |
| `generate_for` | Parse a generate-for loop |
| `generate_if` | Parse a generate-if conditional |
| `port` | Parse an individual port declaration |
| `ports` | Parse a RAM port group |
| `end` | Return to the enclosing construct (the caller already knows which `end keyword` to expect) |

Every construct is identified by its first keyword. Every closing is `end` followed by a single keyword token. No disambiguation is ever needed.

**Contrast with SystemVerilog:**

SystemVerilog requires unbounded lookahead and context-dependent parsing. Examples of ambiguity:

- `always` could be `always_ff`, `always_comb`, or `always_latch` --- the parser must look at the sensitivity list to decide.
- `module ... endmodule` vs `function ... endfunction` vs `begin ... end` use different closing keywords without a uniform rule.
- Type declarations, expressions, and module instantiations share overlapping syntax --- SV parsers require GLR or backtracking to resolve.
- Macro preprocessing (`define`, `ifdef`) creates a separate language layer that interacts with parsing.

**Benefits for AI code generation:**

1. **Token efficiency.** ARCH expresses the same hardware in fewer tokens than SV. Measurements across 156 VerilogEval problems show ARCH uses ~25% fewer lines than generated SV. Fewer tokens means more design fits in a fixed context window, allowing larger modules to be generated or reviewed in a single pass.

2. **No syntactic traps.** An LL(1) grammar has exactly one way to write each construct. The AI cannot produce syntactically ambiguous code, because the grammar has no ambiguities. Every token sequence either parses to exactly one AST, or is a syntax error caught immediately.

3. **Instant error localization.** Because the parser never backtracks, syntax errors are detected at the exact token where the grammar is violated. The AI (or human) gets a precise diagnostic: "expected `end module`, found `end fsm`" --- not a cascade of confusing secondary errors from a failed backtrack.

4. **Context-free understanding.** Any snippet of ARCH code can be parsed in isolation --- `module Foo ... end module Foo` is self-contained. An LLM does not need to hold the entire file in context to understand a block's boundaries. This is possible because the grammar is context-free and LL(1): the parser state at any point depends only on the current token and the call stack, not on arbitrarily distant tokens.

5. **Predictable token budget.** Because every construct follows the `keyword Name ... end keyword Name` pattern with no optional delimiters, the token count for a given design is predictable. There are no hidden costs from syntactic sugar, macro expansion, or optional semicolons that inflate token usage unpredictably.

> *⚑ Arch's LL(1) grammar is a deliberate design choice, not an accident. Every syntax decision --- fused keywords (`generate_for` not `generate for`), `ports` vs `port` for RAM groups, `end` + single keyword closings --- was made to keep the grammar strictly LL(1) while remaining readable.*

**2.5 The todo! Escape Hatch**

Any expression or block body may be replaced with todo! to produce a compilable skeleton. The compiler emits a warning for every todo! site. Simulation aborts with a clear diagnostic if a todo! site is reached at runtime.

+--------------------------------------------------------------------+
| *todo_escape.arch*                                                 |
|                                                                    |
| **module** Multiplier                                              |
|                                                                    |
| **param** WIDTH: **const** = 32;                                   |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** a: **in** UInt\<WIDTH\>;                                  |
|                                                                    |
| **port** b: **in** UInt\<WIDTH\>;                                  |
|                                                                    |
| **port** result: **out** UInt\<WIDTH\>;                            |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| result = **todo**!; // WARN: todo! at Multiplier::result           |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **module** Multiplier                                      |
+--------------------------------------------------------------------+

**3. Type System**

The Arch type system enforces four independent safety dimensions simultaneously. A signal that satisfies all four is guaranteed correct-by-construction --- no simulation required to catch structural errors.

**3.1 Primitive Types**

  ---------------------------------------------------------------------------------------------------------------
  **Type**             **Hardware Width**   **Notes**
  -------------------- -------------------- ---------------------------------------------------------------------
  **Bit**              1 bit                Raw logic bit. Not Bool --- use only for bit-level manipulation.

  **UInt\<N\>**        N bits               Unsigned integer. N must be a const expression (literal or param).

  **SInt\<N\>**        N bits               Two\'s-complement signed integer.

  **Bool**             1 bit                Logical boolean. Not implicitly castable to/from UInt or Bit.

  **Clock\<D\>**       1 bit                Carries clock-domain tag D. Cannot appear in arithmetic.

  **Reset\<Sync, High\|Low\>**    1 bit                Synchronous reset --- deasserted on the clock edge. Polarity defaults High.

  **Reset\<Async, High\|Low\>**   1 bit                Asynchronous reset --- deasserted immediately. Polarity defaults High.

  **Reset\<..., ..., Domain\>**   1 bit                Optional third parameter tags the reset with a domain name for RDC (Reset Domain Crossing) checking. See §5.4.

  **Tristate\<T\>**    \|T\| bits            Bidirectional pad type. Decomposes to \_out, \_oe, \_in internally. See §5.5.

  **Vec\<T,N\>**       N × \|T\|            Fixed-size array of any hardware type T.

  **struct S**         Σ fields             Named aggregate. Width = sum of field widths (packed).

  **enum E**           ⌈log₂n⌉ bits         Discriminated union. Compiler picks minimum encoding width.

  **Token**            0 bits               Pure handshake carrier. Holds valid/ready signals; zero data width.
  ---------------------------------------------------------------------------------------------------------------

**3.2 Bit-Width Safety**

Every assignment, port connection, and arithmetic result is width-checked at compile time. There is no implicit truncation, zero-extension, or sign-extension anywhere in the language.

+-----------------------------------------------------------------------------+
| *width_safety.arch*                                                         |
|                                                                             |
| **let** a: UInt\<8\> = 0xFF;                                                |
|                                                                             |
| **let** b: UInt\<16\> = 0x1234;                                             |
|                                                                             |
| // ✗ COMPILE ERROR --- cannot assign UInt\<16\> to UInt\<8\>                |
|                                                                             |
| **let** bad: UInt\<8\> = b;                                                 |
|                                                                             |
| // ✓ Explicit operations --- intent is always visible                       |
|                                                                             |
| **let** lo: UInt\<8\> = b.trunc\<8\>();           // narrow: error if N ≥ src width  |
|                                                                             |
| **let** rd: UInt\<5\> = instr[11:7];            // bit-slice [11:7]         |
|                                                                             |
| **let** ext: UInt\<16\> = a.zext\<16\>();         // widen unsigned: error if N ≤ src width |
|                                                                             |
| **let** sx: SInt\<16\> = (a **as** SInt\<8\>).sext\<16\>();  // widen signed: error if N ≤ src width |
|                                                                             |
| **let** any: UInt\<16\> = a.resize\<16\>();       // direction-agnostic: widen or narrow |
|                                                                             |
| **let** sm: UInt\<4\> = b.resize\<4\>();          // same: narrows when N < src width  |
|                                                                             |
| // Same-width signed/unsigned reinterpret (no width argument needed)       |
|                                                                             |
| **let** sa: SInt\<8\> = signed(a);   // UInt\<8\> → SInt\<8\>              |
|                                                                             |
| **let** ua: UInt\<8\> = unsigned(sa); // SInt\<8\> → UInt\<8\>             |
|                                                                             |
| // signed() is ideal for entering signed arithmetic chains:                |
|                                                                             |
| **let** diff: SInt\<9\> = signed(a) - signed(b);  // SInt\<8\> - SInt\<8\> → SInt\<9\> |
|                                                                             |
| // Arithmetic: result widths conservatively inferred; must match ascription |
|                                                                             |
| **let** sum: UInt\<9\> = a + a; // UInt\<8\> + UInt\<8\> → UInt\<9\>        |
|                                                                             |
| **let** wide: UInt\<24\> = a.zext\<24\>() \* b.zext\<24\>();                |
|                                                                             |
| // Common trap: counter increment widens by one bit                        |
|                                                                             |
| **reg** cnt: UInt\<8\> **init** 0;                                          |
|                                                                             |
| cnt \<= cnt + 1; // ✗ COMPILE ERROR: UInt\<8\> ← UInt\<9\>                 |
|                                                                             |
| cnt \<= (cnt + 1).trunc\<8\>(); // ✓ explicit wrap-around truncation        |
+-----------------------------------------------------------------------------+

**Shift operators: non-widening (IEEE 1800-2012 §11.6.1)**

Unlike addition and multiplication, shift operators (`<<`, `>>`) do **not** widen the result. The result width equals the left operand width, regardless of the shift amount:

```
let a: UInt<8> = 0xAB;
let shifted: UInt<8> = a << 1;           // UInt<8>, MSB lost — no widening
let wide: UInt<9> = a.zext<9>() << 1;    // UInt<9>, MSB preserved — explicit widen first
```

| Operation | Result width | IEEE §11.6 rule |
|-----------|-------------|-----------------|
| `a + b` | `max(W(a), W(b)) + 1` | Arithmetic widening |
| `a * b` | `W(a) + W(b)` | Multiplication widening |
| `a +% b` | `max(W(a), W(b))` | **Wrapping add** — no widening; SV: `max(W(a),W(b))'(a + b)` |
| `a -% b` | `max(W(a), W(b))` | **Wrapping sub** — no widening; SV: `max(W(a),W(b))'(a - b)` |
| `a *% b` | `max(W(a), W(b))` | **Wrapping mul** — no widening; SV: `max(W(a),W(b))'(a * b)` |
| `a << n` | `W(a)` | **Non-widening** — shift amount does not affect result width |
| `a >> n` | `W(a)` | **Non-widening** |

> *⚑ The compiler emits a **compile error** when a shift result is assigned to a wider target (e.g. `let wide: UInt<9> = a << 1;`), because the extra bit will always be zero --- the shift did not capture the overflow. The fix is to widen the operand first: `a.zext<9>() << 1`. Same-width shifts (e.g. `let x: UInt<8> = a << 1;`) are silent --- MSB loss is the normal, intended behavior of a fixed-width shift.*

**Built-in functions:**

| Function | Result | SV codegen |
|----------|--------|------------|
| `onehot(index)` | One-hot decode: `1 << index`. Width inferred from assignment target. | `(1 << index)` |
| `$clog2(expr)` | Ceiling log2 (compile-time constant). | `$clog2(expr)` |
| `signed(expr)` | Same-width reinterpret to `SInt`. | `$signed(expr)` |
| `unsigned(expr)` | Same-width reinterpret to `UInt`. | `$unsigned(expr)` |

Example: one-hot decode for bean selection (parameterized width):

```
port i_bean_sel: in UInt<NBW_BEANS>;
reg bean_r: UInt<NS_BEANS> reset rst => 0;

seq on clk rising
  bean_r <= onehot(i_bean_sel);    // SV: (1 << i_bean_sel)
end seq
```

> *⚑ Width inference follows IEEE 1800-2012 §11.6. Arch promotes all mismatches to hard errors --- never warnings. The arithmetic widening trap (`r <= r + 1`) is caught at the register-assignment level: the compiler diagnoses it and suggests `.trunc<N>()`. The `.trunc<N>()` method emits a SystemVerilog size cast `N'(expr)`, which is valid on any expression including compound ones. Bit-slice syntax `expr[hi:lo]` extracts a bit range: `instr[11:7]` emits `instr[11:7]` with result width hi−lo+1. This is essential for instruction field decoding. The compiler enforces cast direction: `.trunc<N>()` requires N < source width, `.zext<N>()`/`.sext<N>()` require N > source width — use `.resize<N>()` when the direction is parameter-dependent or intentionally flexible.*

> *⚑ **Wrapping operators** (`+%`, `-%`, `*%`) are the ergonomic alternative to the `.trunc<N>()` boilerplate: `let x: UInt<8> = a +% b;` is equivalent to `let x: UInt<8> = (a + b).trunc<8>();`. The result width is `max(W(a), W(b))` for all three. The SV backend emits a size cast `W'(a op b)`. Use wrapping ops when the intent is deliberate modular arithmetic, not overflow capture.*

**3.3 Struct and Enum Types**

+--------------------------------------------------------------------+
| *struct_enum.arch*                                                 |
|                                                                    |
| /// A decoded RISC-V instruction --- packed struct, total 57 bits  |
|                                                                    |
| **struct** DecodedInstr                                            |
|                                                                    |
| opcode: UInt\<7\>,                                                 |
|                                                                    |
| rd: UInt\<5\>,                                                     |
|                                                                    |
| rs1: UInt\<5\>,                                                    |
|                                                                    |
| rs2: UInt\<5\>,                                                    |
|                                                                    |
| funct3: UInt\<3\>,                                                 |
|                                                                    |
| imm: SInt\<32\>,                                                   |
|                                                                    |
| **end** **struct** DecodedInstr                                    |
|                                                                    |
| /// ALU operation selector --- compiler assigns 4-bit encoding     |
|                                                                    |
| **enum** AluOp                                                     |
|                                                                    |
| Add, Sub, And, Or, Xor,                                            |
|                                                                    |
| Sll, Srl, Sra, Slt, Sltu,                                          |
|                                                                    |
| **end** **enum** AluOp                                             |
|                                                                    |
| // Struct literal                                                  |
|                                                                    |
| **let** instr: DecodedInstr = DecodedInstr                         |
|                                                                    |
| opcode: 7\'b0110011, rd: 5\'d1, rs1: 5\'d2, rs2: 5\'d3,            |
|                                                                    |
| funct3: 3\'b000, imm: 0,                                           |
|                                                                    |
| **end** DecodedInstr                                               |
|                                                                    |
| // Field access                                                    |
|                                                                    |
| **let** dst: UInt\<5\> = instr.rd;                                 |
|                                                                    |
| // Enum match --- must be exhaustive                               |
|                                                                    |
| **let** result: UInt\<32\> = **match** op                          |
|                                                                    |
| AluOp::Add =\> a + b,                                              |
|                                                                    |
| AluOp::Sub =\> a - b,                                              |
|                                                                    |
| AluOp::And =\> a & b,                                              |
|                                                                    |
| \_ =\> **todo**!,                                                  |
|                                                                    |
| **end** **match**                                                  |
+--------------------------------------------------------------------+

**3.4 Signal Ownership (Single-Driver Rule)**

Every signal in Arch has exactly one driver --- the block (comb or reg) that assigns it. The compiler enforces this statically. Multiple drivers, floating signals, and unintentional latches are all compile-time errors.

+--------------------------------------------------------------------+
| *ownership.arch*                                                   |
|                                                                    |
| **module** Example                                                 |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** a: **in** UInt\<8\>;                                      |
|                                                                    |
| **port** b: **in** UInt\<8\>;                                      |
|                                                                    |
| **port** y: **out** UInt\<9\>;                                     |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| y = a.zext\<9\>() + b.zext\<9\>();                                 |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| // ✗ COMPILE ERROR --- y is already driven by the comb block above |
|                                                                    |
| // comb                                                            |
|                                                                    |
| // y = 0;                                                          |
|                                                                    |
| // end comb                                                        |
|                                                                    |
| **end** **module** Example                                         |
+--------------------------------------------------------------------+

> *⚑ The single-driver rule eliminates the most common class of RTL bugs: multiply-driven nets and unintentional latches. It also makes AI-generated code safe --- the compiler rejects any double-assignment.*
>
> ◈ **No implicit latches.** The compiler verifies that every signal assigned in a `comb` block is assigned on ALL control paths. A missing `else` branch or incomplete `match` is a compile error: *"signal \`x\` is not assigned on all control paths in comb block (infers a latch)."* The common default-then-override pattern is safe: `x = 0; if sel / x = a; end if` --- the unconditional default covers the missing else. An exhaustive enum `match` (all variants listed without `_` wildcard) also satisfies this check.
>
> ◈ **Comb match uses `=` syntax.** In `comb` blocks, `match` arms use `=` (combinational assign). In `seq` blocks, arms use `<=` (register assign). Enum exhaustiveness is checked in both contexts.

**4. Modules**

A module is the fundamental unit of design in Arch. Every module follows the same four-section schema --- params, ports, body, optional verification. This regularity is intentional: an AI encountering any Arch construct can immediately orient itself using the same mental model.

**4.1 Declaration Schema**

+--------------------------------------------------------------------------------+
| *module_schema.arch*                                                           |
|                                                                                |
| // The universal Arch schema --- identical across every first-class construct: |
|                                                                                |
| //                                                                             |
|                                                                                |
| // keyword Name                                                                |
|                                                                                |
| // param NAME: const = value;       // untyped int — emits `parameter int`     |
|                                                                                |
| // param NAME[hi:lo]: const = value; // width-qualified — emits `parameter [hi:lo]` |
|                                                                                |
| // param NAME: type = SomeType;     // type alias — emits `parameter type`     |
|                                                                                |
| // param NAME: EnumName = EnumName::Variant; // enum-typed — emits `parameter EnumName` |
|                                                                                |
| // local param NAME: const = expr;  // derived — emits `localparam` (not overridable) |
|                                                                                |
| //                                                                             |
|                                                                                |
| // port name: dir Type; // section 2: ports                                   |
|                                                                                |
| //                                                                             |
|                                                                                |
| // \<construct-specific body\> // section 3: logic / structure                 |
|                                                                                |
| //                                                                             |
|                                                                                |
| // assert / cover / assume \...; // section 4: verification (optional)         |
|                                                                                |
| //                                                                             |
|                                                                                |
| // end keyword Name                                                            |
|                                                                                |
| **module** Adder                                                               |
|                                                                                |
| **param** WIDTH: **const** = 8;                                                |
|                                                                                |
| **port** clk: **in** Clock\<SysDomain\>;                                       |
|                                                                                |
| **port** rst: **in** Reset\<Sync\>;                                            |
|                                                                                |
| **port** a: **in** UInt\<WIDTH\>;                                              |
|                                                                                |
| **port** b: **in** UInt\<WIDTH\>;                                              |
|                                                                                |
| **port** sum: **out** UInt\<WIDTH+1\>;                                         |
|                                                                                |
| **comb**                                                                       |
|                                                                                |
| sum = a.zext\<WIDTH+1\>() + b.zext\<WIDTH+1\>();                               |
|                                                                                |
| **end** **comb**                                                               |
|                                                                                |
| **end** **module** Adder                                                       |
+--------------------------------------------------------------------------------+

**4.2 Combinational vs Registered Blocks**

Arch has exactly two assignment forms. Mixing operators between them is a compile error.

  --------------------------------------------------------------------------------------------------------------------------
  **Block**           **Keyword**          **Operator**   **Hardware Meaning**
  ------------------- -------------------- -------------- ------------------------------------------------------------------
  **Combinational**   comb \... end comb   =              Continuous assignment; no flip-flop inferred.

  **Registered**      seq \... end seq            \<=            Clocked assignment; flip-flop inferred on the active clock edge. Reset is declared on each `reg` declaration, not on the `seq` block.
  --------------------------------------------------------------------------------------------------------------------------

+--------------------------------------------------------------------+
| *counter.arch*                                                     |
|                                                                    |
| **module** Counter                                                 |
|                                                                    |
| **param** WIDTH: **const** = 8;                                    |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** en: **in** Bool;                                          |
|                                                                    |
| **port** count: **out** UInt\<WIDTH\>;                             |
|                                                                    |
| **reg** count_r: UInt\<WIDTH\> **reset** rst=\>0;                  |
|                                                                    |
| **always** **on** clk rising                                       |
|                                                                    |
| **if** en                                                          |
|                                                                    |
| count_r \<= count_r + 1;                                           |
|                                                                    |
| **end** **if**                                                     |
|                                                                    |
| **end** **always**                                                 |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| count = count_r;                                                   |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **module** Counter                                         |
+--------------------------------------------------------------------+

> *⚑ The clock is named in `seq on clk rising`; reset is declared per register (`reset rst => 0 sync high` or `reset none`). The `reset SIGNAL=>VALUE` syntax requires an explicit reset value after the signal name. `init` is optional and only sets the SV declaration initializer (`logic x = VALUE;`). The compiler auto-generates the `if (rst)` guard and propagates domain membership automatically through all downstream logic in the module.*

**Default Clock for seq Blocks**

When a module or FSM has only one clock, the `seq on clk rising/falling` header can be factored out into a default declaration:

```
default seq on clk rising;
```

This sets the default clock and edge for all `seq` blocks in the construct. With this default in place, `seq` blocks no longer need the `on clk rising` clause:

```
module Counter
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port reg count: out UInt<8> reset rst => 0;

  default seq on clk rising;

  seq count <= (count + 1).trunc<8>();
end module Counter
```

**One-line seq syntax:** When `default seq` is declared, a single-assignment seq block can be written on one line without `end seq`:

```
seq target <= expr;
```

This is equivalent to `seq on clk rising target <= expr; end seq`. Multi-line seq blocks (with `if/elsif/else`, `for`, or multiple assignments) still use the full `seq ... end seq` form (but omit `on clk rising`).

Combinational blocks with a single assignment can use the one-line form, omitting `end comb`:

```
comb y = a & b;
```

This is equivalent to `comb y = a & b; end comb`. Blocks with multiple assignments, `if/else`, or `for` loops must use the full `comb ... end comb` form.

**4.2.1 Conditional Statements: if / elsif / else**

Arch uses `elsif` (one word) for chained conditionals, not `else if` (two words). In a brace-free language, `else if` is ambiguous — does `else` start a new body block, or does `else if` chain? The `elsif` keyword resolves this unambiguously:

```
if condition_a
  r <= val_a;
elsif condition_b
  r <= val_b;
elsif condition_c
  r <= val_c;
else
  r <= val_default;
end if
```

The same syntax applies in both `seq` and `comb` blocks. A chain begins with `if`, continues with zero or more `elsif` branches, optionally ends with `else`, and is always closed by a single `end if`. The compiler emits standard SystemVerilog `if / else if / else` from this syntax.

**4.2.1a For Loops in comb and seq Blocks**

A `for` loop iterates over an inclusive integer range inside a `comb` or `seq` block. The syntax is:

```
for VAR in START..END
  // body — VAR is an integer variable usable as an index
end for
```

The range `START..END` is inclusive (both endpoints are visited). The compiler emits a SystemVerilog `for` loop:

```systemverilog
for (int VAR = START; VAR <= END; VAR++) begin
  // body
end
```

Example in a `comb` block:

```
comb
  for i in 0..7
    out[i] = data[7 - i];
  end for
end comb
```

Example in a `seq` block:

```
seq on clk rising
  for i in 0..3
    shift_r[i] <= shift_r[i + 1];
  end for
  shift_r[3] <= data_in;
end seq
```

> *⚑ This `for` construct is a runtime loop emitted as a SV `for` statement. It differs from `generate for`, which is a compile-time unrolling that creates distinct ports and instances.*

**4.2.1b For Loops with Value Lists**

A `for` loop may also iterate over an explicit value list enclosed in braces. The syntax is:

```
for VAR in {val1, val2, val3}
  // body — VAR takes each value in turn
end for
```

The compiler unrolls the loop at compile time, emitting one copy of the body per value. This is useful when the iteration values are non-contiguous or follow no simple range pattern.

Example in a `comb` block:

```
comb
  for i in {0, 3, 7, 15}
    mask[i] = true;
  end for
end comb
```

The compiler emits one assignment per value:

```systemverilog
mask[0] = 1'b1;
mask[3] = 1'b1;
mask[7] = 1'b1;
mask[15] = 1'b1;
```

> *⚑ Value-list `for` is compile-time unrolled (like `generate for`), not a runtime SV `for` loop. Use range-based `for i in 0..N` for runtime loops.*

**4.2.1c The `inside` Set Membership Operator**

The `inside` operator tests whether an expression matches any value or falls within any range in a set. It returns `Bool`.

```
expr inside {val1, val2, lo..hi}
```

Individual values and inclusive ranges (`lo..hi`) may be freely mixed inside the braces. The compiler emits the SystemVerilog `inside` operator directly:

```systemverilog
expr inside {val1, val2, [lo:hi]}
```

Example:

```
let is_special: Bool = opcode inside {3, 7, 16..31};
```

Emits:

```systemverilog
assign is_special = opcode inside {3, 7, [16:31]};
```

The `inside` expression can be used anywhere a `Bool` expression is valid — in `if` conditions, ternary operands, `comb` assignments, `transition when` guards, etc.

**4.2.1d The `unique` Modifier for `if` and `match`**

The `unique` keyword may be prepended to any `if` or `match` statement to assert to the synthesis tool that all conditions are mutually exclusive. The compiler passes the `unique` qualifier directly to SystemVerilog, enabling parallel mux inference instead of priority encoding.

```
unique if sel == 0
  y = a;
else
  y = b;
end if
```

```
unique match opcode
  0 => result <= a;
  1 => result <= b;
  _ => result <= 0;
end match
```

The emitted SystemVerilog uses `unique if (...)` and `unique case (...)` respectively. Use `unique` when you know the conditions cannot overlap and want the synthesis tool to optimize accordingly. Omit it when conditions may overlap and priority resolution is required.

**4.2.2 Bit Concatenation and Replication**

Arch uses standard SystemVerilog syntax for bit concatenation and replication:

```
let word: UInt<16> = {high_byte, low_byte};       // concatenation (MSB first)
let sign_ext: UInt<8> = {8{sign_bit}};            // replication
let sext32: UInt<32> = {{24{sign_bit}}, byte_val}; // replication inside concat
```

`{a, b, c}` concatenates operands MSB-first, producing a `UInt` whose width is the sum of all operand widths. `{N{expr}}` replicates `expr` N times. Replication may be nested inside concatenation.

**4.2.3 Signal Declarations: let, wire, and reg**

Arch has three kinds of module-scope signal declarations. Each has a distinct syntax, assignment location, and SV equivalent.

| Construct | Syntax | Assigned in | SV equivalent |
|-----------|--------|-------------|---------------|
| `let` (declare) | `let x: T = expr;` | declaration — type required | `logic [W-1:0] x; assign x = expr;` |
| `let` (assign) | `let x = expr;` | declaration — x must already exist as output port or wire | `assign x = expr;` (no new declaration) |
| `wire` | `wire x: T;` | `comb` block (`=`) | `logic [W-1:0] x;` (driven in `assign`/`always_comb`) |
| `reg` | `reg x: T [init V] [reset R=>V];` | `seq` block (`<=`) | `logic [W-1:0] x = V;` (driven in `always_ff`) |

**`let`** has two forms:

- `let x: T = expr;` — declares a new combinational wire `x` of type `T`, fixed to `expr`. Emits `logic [W-1:0] x; assign x = expr;` in SV. Type annotation is required when declaring.
- `let x = expr;` — assigns to an **already-declared** output port or wire named `x`. No new signal is created; the type is taken from the existing declaration. Emits `assign x = expr;`. This replaces the old one-liner `comb x = expr;` form. Errors if `x` is not in scope, is an input port, or is a `reg` (use `seq` for those).

**`wire`** declares an explicitly-typed combinational net with no initializer. It must be driven by a `let x = expr;` assignment or inside a `comb ... end comb` block. Use `wire` when the value is conditionally assigned (`if/elsif/else`) — `let x: T = expr;` only supports a single fixed expression. The type checker enforces that only `wire` declarations and output ports are valid comb targets — assigning to a `reg` in `comb` is a compile error.

**`reg`** declares a flip-flop. It must be assigned inside a `seq` block using `<=`. Reset polarity and mode are declared per register. The syntax is `reg x: T [init VALUE] [reset SIGNAL=>VALUE];` where `init` is optional (sets only the SV declaration initializer `logic x = VALUE;`) and `reset SIGNAL=>VALUE` specifies both the reset signal and the value to load on reset. Use `reset none` for registers that should not be reset. A `reg default:` declaration sets the default init and reset for all subsequent registers in scope: `reg default: [init VALUE] reset SIGNAL=>VALUE;`.

**`port reg`** declares an output port that is also a register, eliminating the common `reg r` + `comb out = r; end comb` boilerplate. The syntax is `port reg name: out T [init V] [reset R=>V];`. It can only be used on output ports (`in` direction is a compile error). The port is assigned with `<=` inside a `seq` block, just like a regular `reg`. If `reg default:` is in scope, it inherits the default init and reset. In generated SV, the port is declared as `output logic [W-1:0] name` and driven directly in the `always_ff` block.

**`guard` clause** — `reg NAME: T guard VALID_SIG [init V] [reset R=>V];` declares that the register is intentionally uninitialized as long as `VALID_SIG` is low. This is the canonical valid-data pattern: a wide data register stays reset-free (saving area and power) while a companion valid flag gates consumers. The `guard` clause goes right after the type, before any `init` or `reset` clause. `VALID_SIG` must be a single identifier — for multi-signal predicates, combine them via a `let` binding first. `port reg` supports the same clause.

```
reg  axi_rdata: UInt<512> guard axi_rvalid;        // no reset — intentional
reg  axi_rvalid: Bool reset rst => false;
port reg dout:  out UInt<32> guard dout_valid;     // port form
```

The clause has three observable effects:

1. **Documents intent** in the source — the reader sees immediately that `axi_rdata` is valid-gated.
2. **Silences spurious `--check-uninit` warnings** at the consumer read site — a plain `reset none` reg reads as "WARNING: read of uninitialized reg 'axi_rdata'", but a guarded reg does not, since consumers are expected to qualify the read with `if axi_rvalid`.
3. **Catches the producer bug** at simulation runtime — if the guard asserts (`axi_rvalid == true`) but the data reg was never written, `--check-uninit` emits a warning. This is the bug the annotation is specifically designed to find: the valid flag goes live but the data bus still carries an uninitialized value.

```
module Mux2
  port sel: in Bool;
  port a:   in UInt<8>;
  port b:   in UInt<8>;
  port y:   out UInt<8>;

  wire result: UInt<8>;   // declared here, driven in comb below

  comb
    if sel
      result = a;
    else
      result = b;
    end if
    y = result;
  end comb
end module Mux2
```

> *⚑ The type checker enforces: `reg` cannot be a `comb` target (error: "cannot assign to register `x` in a comb block; use `<=` inside a seq block"). Only `wire` declarations and output ports are valid comb targets.*

**`multicycle` reg annotation (planned)** — `reg result: UInt<32> multicycle 3 reset rst => 0;` declares that the combinational path feeding this register has a multi-cycle timing budget. Unlike `pipe_reg` (which inserts N physical flip-flop stages), a `multicycle` register remains a single flop — no extra area or power. The compiler emits an SDC constraint (`set_multicycle_path N -to result`) and can statically verify that consumers only sample the value at the correct rate. This is useful for slow-settling operations (multipliers, dividers, complex ALU) where the path does not affect end-to-end throughput.

The Counter example from §4.2 can be simplified using `port reg`:

```
module Counter
  param WIDTH: const = 8;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port en: in Bool;
  port reg count: out UInt<WIDTH> reset rst => 0;

  seq on clk rising
    if en
      count <= (count + 1).trunc<WIDTH>();
    end if
  end seq
end module Counter
```

> *⚑ `port reg` eliminates the intermediate `reg count_r` and the `comb count = count_r; end comb` wiring block. The port is directly assigned in the `seq` block. This is the preferred style when the output port is a straightforward registered value.*

**Timing implication of `port reg` vs `port` outputs:**

| Output style | Declaration | Driven in | SV codegen | Output latency |
|---|---|---|---|---|
| **Registered** | `port reg o: out T reset ...` | `seq` block (`<=`) | `always_ff: o <= f(state)` | 1-cycle lag — output reflects state from the **previous** clock edge |
| **Combinational** | `port o: out T` | `comb` block (`=`) or `let o = expr;` | `assign o = f(state)` or `always_comb` | 0-cycle — output reflects **current** state immediately |

For FSM outputs that must change in the **same cycle** as a state transition (e.g., when a testbench model updates state and outputs simultaneously), use a plain `port` with `comb` assignment:

```
port o_active: out Bool;

comb
  o_active = (state_ff == 3);  // combinational: changes same cycle as state_ff
end comb
```

For FSM outputs that should be **registered** (glitch-free, timing-clean, but 1-cycle delayed):

```
port reg o_active: out Bool reset rst => false;

seq on clk rising
  o_active <= (state_ff == 3);  // registered: reflects state_ff from previous edge
end seq
```

> *⚑ Choose carefully: `port reg` adds a pipeline stage to the output path. If a testbench or downstream module expects zero-latency output response to state changes, use combinational `port` + `comb` instead.*

**`multicycle` reg annotation (planned)** — `reg result: UInt<32> multicycle 3 reset rst => 0;` declares that the combinational path feeding this register has a multi-cycle timing budget. Unlike `pipe_reg` (which inserts N physical flip-flop stages), a `multicycle` register remains a single flop — no extra area or power. The compiler auto-detects all input signals feeding the register by walking the assignment expression tree. Three modes of enforcement: (1) **Simulation** (`--check-uninit`): hidden valid tracking with input change detection and latency counter; reads before the counter expires return poison/X. (2) **Synthesis**: SDC constraint generation (`set_multicycle_path N -to result`). (3) **Formal**: optional `assert property` to verify the multicycle timing assumption holds.

**4.2.4 Width-Qualified Parameters**

An untyped `param NAME: const = value` emits `parameter int NAME = value` in SystemVerilog (32-bit signed). When a parameter is compared or assigned against a specific-width signal, this width mismatch produces Verilator `WIDTHEXPAND` warnings. Use the bracket form to declare the parameter's bit width explicitly:

```
param H_ACTIVE[9:0]: const = 640;   // emits: parameter [9:0] H_ACTIVE = 640
param H_FRONT[9:0]:  const = 16;    // emits: parameter [9:0] H_FRONT  = 16
```

The compiler validates that the default value fits within the declared range. Comparisons with `UInt<10>` signals are now width-matched and Verilator-clean.

**4.3 Module Instantiation**

+--------------------------------------------------------------------+
| *instantiation.arch*                                               |
|                                                                    |
| **module** Top                                                     |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** a: **in** UInt\<8\>;                                      |
|                                                                    |
| **port** b: **in** UInt\<8\>;                                      |
|                                                                    |
| **port** **out**: **out** UInt\<9\>;                               |
|                                                                    |
| **inst** add: Adder                                                |
|                                                                    |
| **param** WIDTH = 8;                                               |
|                                                                    |
| **clk \<- clk;                                           |
|                                                                    |
| **rst \<- rst;                                           |
|                                                                    |
| **a \<- a;                                               |
|                                                                    |
| **b \<- b;                                               |
|                                                                    |
| **sum -\> **out**;                                       |
|                                                                    |
| **end** **inst** add                                               |
|                                                                    |
| **end** **module** Top                                             |
+--------------------------------------------------------------------+

> ◈ port \<- signal drives an input. port -\> signal reads an output. The arrow direction always shows which way data flows --- a property that makes the design readable to both humans and AI without any context lookup.
>
> ◈ **Port group member syntax.** For constructs with named port groups (e.g. `regfile` with `ports[N] read`), connections use dot notation: `write.en <- wr_en;` (flattened to `write_en`). For indexed port groups, use bracket-dot notation: `read[0].addr <- sel;` (flattened to `read0_addr`). The index must be an integer literal. Both forms are resolved at parse time --- the rest of the compiler sees only the flattened name.
>
> ◈ **Whole-bus connections.** When an inst's bus port connects to a parent bus port (or wire), a single connection expands to all signals in the bus definition: `axi_rd -> m_axi_mm2s;` expands to `axi_rd_ar_valid -> m_axi_mm2s_ar_valid`, etc. Signal directions are derived from the bus definition and the port's perspective (`initiator` or `target`). This works for both `module` and `fsm` constructs.
>
> ◈ **Indexed bus port expressions.** Generated bus port arrays (via `generate for i in 0..N / port m_axi_i: initiator Bus`) can be referenced in comb/seq blocks using bracket-dot syntax: `m_axi[0].ar_valid = true;` flattens to `m_axi_0_ar_valid`. The index must be an integer literal.
>
> ◈ **Hierarchical instance references are forbidden.** Expressions like `inst_name.port_name` (e.g. `add.sum`) are a compile error. To read an instance output, use `port -> wire_name` inside the `inst` block and reference `wire_name` in the enclosing scope. Error message: *"hierarchical reference \`u.y\` is not allowed; use \`y -> wire_name\` in the inst block instead."*
>
> ◈ **Port connection completeness.** Every input port of an instantiated construct must appear in the connection list --- a missing input port is a compile error. Unconnected output ports produce a warning (discarding an output is sometimes intentional). Clock and Reset ports are exempt from this check.
>
> ◈ **SV codegen notes.** The SystemVerilog backend applies the following transformations for correctness across simulators and lint tools: (1) signed casts emit `$signed(x)` (not `logic signed [N-1:0]'(x)`) for Verilator compatibility; (2) right-shift `>>` on an `SInt` operand emits arithmetic shift `>>>` (correct SRA behavior); (3) `.zext<N>()` emits `N'($unsigned(x))` to prevent context-dependent width expansion; (4) `.resize<N>()` emits `N'($unsigned(x))` for unsigned/Bool base types and `N'($signed(x))` for signed base types — SV handles both widening (padding) and narrowing (truncation) via the size cast; (5) width literals are constant-folded into ranges: `UInt<8>` emits `logic [7:0]` (not `logic [8-1:0]`), while parameter-dependent widths like `UInt<XLEN>` emit `logic [XLEN-1:0]` and are preserved symbolically — Yosys and Verilator both accept both forms, but the folded form avoids expression evaluation during elaboration.

> ◈ **Sim codegen notes.** The C++ simulation backend applies the following fixes for correctness: (1) `.sext<N>()` properly replicates the MSB of the source value into all upper bits of the result — previously it was treated identically to `.zext<N>()` (plain C++ cast, no sign extension); the correct formula is `((val & ((1<<src)-1)) ^ (1<<(src-1))) - (1<<(src-1))` where `src` is the source width; (2) bit-slice `expr[Hi:Lo]` correctly computes the inferred width as `Hi-Lo+1` for subsequent operations.

**5. Clock Domains and CDC Safety**

Every Clock signal in Arch carries a domain tag as part of its type. The compiler tracks which domain every signal belongs to. Crossing domain boundaries without an explicit crossing block is a compile-time error --- never a simulation-time surprise.

**5.1 Declaring Domains**

**Built-in domain: `SysDomain`.** The domain `SysDomain` is always available without an explicit declaration. It is the conventional default clock domain. You may still declare `domain SysDomain freq_mhz: 200 end domain SysDomain` to set a specific frequency, but the domain name itself is pre-registered and can be used in `Clock<SysDomain>` without any domain block.

+-------------------------------------------------------------------------------+
| *domains.arch*                                                                |
|                                                                               |
| // Domains are declared at the top level --- before any module that uses them |
|                                                                               |
| **domain** SysDomain                                                          |
|                                                                               |
| freq_mhz: 200                                                                |
|                                                                               |
| **end** **domain** SysDomain                                                  |
|                                                                               |
| **domain** UsbDomain                                                          |
|                                                                               |
| freq_mhz: 48                                                                 |
|                                                                               |
| **end** **domain** UsbDomain                                                  |
|                                                                               |
| **domain** PcieDomain                                                         |
|                                                                               |
| freq_mhz: 250                                                                |
|                                                                               |
| **end** **domain** PcieDomain                                                 |
+-------------------------------------------------------------------------------+

**5.2 Clock Domain Crossing**

+---------------------------------------------------------------------------+
| *cdc.arch*                                                                |
|                                                                           |
| **module** UsbBridge                                                      |
|                                                                           |
| **port** sys_clk: **in** Clock\<SysDomain\>;                              |
|                                                                           |
| **port** usb_clk: **in** Clock\<UsbDomain\>;                              |
|                                                                           |
| **port** rst: **in** Reset\<Async\>;                                      |
|                                                                           |
| **port** sys_data: **in** UInt\<32\>;                                     |
|                                                                           |
| **port** usb_data: **out** UInt\<32\>;                                    |
|                                                                           |
| // ✗ COMPILE ERROR --- signal crosses domains without a declared crossing |
|                                                                           |
| // comb                                                                   |
|                                                                           |
| // usb_data = sys_data;                                                   |
|                                                                           |
| // end comb                                                               |
|                                                                           |
| // ✓ Explicit CDC --- compiler inserts a verified synchroniser            |
|                                                                           |
| **crossing** sys_to_usb                                                   |
|                                                                           |
| **from**: SysDomain,                                                      |
|                                                                           |
| **to**: UsbDomain,                                                        |
|                                                                           |
| **sync**: two_flop, // policy: two_flop \| gray_code \| handshake         |
|                                                                           |
| data: sys_data -\> usb_data,                                              |
|                                                                           |
| **end** **crossing** sys_to_usb                                           |
|                                                                           |
| **end** **module** UsbBridge                                              |
+---------------------------------------------------------------------------+

> *⚑ The compiler generates a verified synchroniser for each crossing declaration. Engineers choose the policy; correctness of the CDC structure is guaranteed by the language, not by convention or code review.*

**5.2a Reconvergent CDC Path Detection** *(planned)*

A reconvergent CDC hazard occurs when multiple bits of a source-domain signal cross independently through separate synchronizers, then recombine in the destination domain. Each bit is individually synchronized, but they may arrive on different clock cycles, causing the receiver to see a value that never existed in the source domain.

```
// BAD: two bits of 'data' cross through independent synchronizers
inst sync_lo: FfSync
  data_in <- data[0];     // from DomainA
  data_out -> synced_lo;  // in DomainB
end inst sync_lo

inst sync_hi: FfSync
  data_in <- data[1];     // from DomainA
  data_out -> synced_hi;  // in DomainB
end inst sync_hi

// synced_lo and synced_hi may arrive on different cycles
// → reconvergent CDC hazard
let result: UInt<2> = {synced_hi, synced_lo};  // may see a value that never existed
```

The compiler will detect this by:

1. At each synchronizer instance, recording `(source_signal, source_domain) → synchronizer_instance`.
2. Tracing `source_signal` back to its originating register (through bit-slices and simple combinational logic).
3. If two or more synchronizer instances in the same destination domain trace back to the same source register (or bits of it), emitting a warning:

> *warning: `sync_lo` and `sync_hi` both originate from register `data` in DomainA but cross independently --- reconvergent CDC hazard. Use a single `kind gray` or `kind handshake` synchronizer for multi-bit coherence.*

The compiler already warns when `kind ff` is used on multi-bit data (suggesting `kind gray` or `kind handshake`). Reconvergent path detection extends this to catch the case where a designer splits a multi-bit signal into individual bits and synchronizes each separately.

> *⚑ Reconvergent CDC detection is planned. Currently, the compiler detects direct cross-domain register reads and warns on multi-bit `kind ff` synchronizers, but does not trace signal origins across synchronizer boundaries.*

**5.3 Clock Output Ports**

`Clock<Domain>` may appear as an output port direction in any module, enabling clock passthrough, gating, and division:

```
// Passthrough
module ClkPassthrough
  port clk_in:  in Clock<SysDomain>;
  port clk_out: out Clock<SysDomain>;
  comb clk_out = clk_in;
end module ClkPassthrough

// Inline gate (AND with enable)
module ClkGate
  port clk_in:  in Clock<SysDomain>;
  port enable:  in Bool;
  port clk_out: out Clock<SysDomain>;
  comb clk_out = clk_in & enable;
end module ClkGate

// Divide-by-2
module ClkDiv2
  port clk_in:  in Clock<SysDomain>;
  port rst:     in Reset<Sync>;
  port clk_out: out Clock<SysDomain>;
  reg toggle: Bool reset rst=>false;
  default seq on clk_in rising;
  seq toggle <= ~toggle; end seq
  comb clk_out = toggle;
end module ClkDiv2
```

A `Clock<>` output port emits `output logic` in SV and may be driven by a `comb` assignment. For dedicated clock gating with integrated latch, use the first-class `clkgate` construct instead.

**5.4 Reset Domain Crossing (RDC)** *(planned)*

Just as `Clock<Domain>` carries a domain tag for CDC checking, `Reset<Kind, Polarity, Domain>` can carry an optional domain tag for RDC checking. When two or more reset domains are present in a module, the compiler will flag unsafe crossings:

```
port rst_a: in Reset<Async, High, PowerDomain>;
port rst_b: in Reset<Async, High, IoDomain>;
```

**RDC violations detected at compile time (planned):**

1. **Cross-reset-domain register read** --- a register held in reset by `rst_a` is read in a `seq` block governed by `rst_b`. The register may still be in reset (or just released) when the consumer is active, causing metastability or stale values.

2. **Asynchronous reset deassertion ordering** --- module B depends on module A's output, but A's reset releases after B's, so B may sample undefined values during the gap.

3. **Reset glitch propagation** --- an async reset from one domain is connected directly to another domain without a reset synchronizer.

The compiler will require a `reset_synchronizer` or explicit `rdc_safe` annotation to suppress the error, mirroring the existing CDC flow. The implementation will extend the existing `reg_domain` tracking in the type checker to build a parallel `reg_reset_domain` map.

> *⚑ Until RDC checking is implemented, the third domain parameter on `Reset<>` is accepted by the parser but not enforced. Engineers should manually verify reset domain crossings.*

**5.5 Tristate and Bidirectional I/O** *(planned)*

Inside a chip, all signals are unidirectional. Tristate (high-impedance) behavior only exists at the **pad ring** — the boundary between the chip and the outside world. Common examples include I2C (open-drain SDA/SCL), bidirectional data buses, and GPIO pins.

ARCH provides a `Tristate<T>` type and a `tristate` block for modeling pad-level bidirectional I/O:

```
module I2cPad
  port clk:     in Clock<SysDomain>;
  port rst:     in Reset<Sync>;
  port sda:     tristate UInt<1>;     // bidirectional pad
  port sda_oe:  in Bool;              // output enable from core
  port sda_out: in Bool;              // drive value from core
  port sda_in:  out Bool;             // read value to core

  tristate sda
    drive: sda_oe ? sda_out : high_z;
    read:  sda_in;
  end tristate
end module I2cPad
```

**Compiler behavior:**

| Backend | Behavior |
|---------|----------|
| **SV codegen** (`arch build`) | Emits `inout` port with `assign sda = sda_oe ? sda_out : 1'bz` |
| **Simulation** (`arch sim`) | Decomposes into `_out` / `_oe` / `_in` signals (2-state, Verilator-compatible); resolution logic: wire-OR of enables, mux of values; undriven nets default to 0 |
| **Type checker** | Restricts `tristate` ports to top-level modules or pad cells; using `tristate` on internal module ports is a compile error |

**Open-drain modeling (I2C, wire-AND):**

```
tristate sda
  drive: sda_oe ? 1'b0 : high_z;   // can only pull low
  read:  sda_in;                     // pull-up is external
end tristate
```

The sim backend resolves multiple open-drain drivers as `sda = ~(oe_a | oe_b)` — a wire-AND with implicit pull-up. The testbench must model external pull-ups explicitly.

> *⚑ Until tristate support is implemented, bidirectional pads should be modeled manually using separate `_out`, `_oe`, and `_in` ports. The compiler does not yet parse `tristate` blocks.*

**Design decision: pad-only tristate, no internal tristate buses.**

Tristate buses were widely used for on-chip shared buses in older process nodes (250nm and above, 1980s--early 2000s). The appeal was clear: N modules sharing a 32-bit bus needed only 32 wires instead of N×32 point-to-point wires. Early CPU architectures (8086, 68000, early ARM) and FPGAs (Xilinx 4000 TBUFs) relied heavily on this technique.

The industry abandoned internal tristate buses below 130nm for several reasons:

- **Contention and shoot-through current** — overlapping drivers during arbitration cause simultaneous PMOS/NMOS conduction, producing large current spikes that damage reliability at deep submicron.
- **Bus turnaround penalty** — dead cycles or guard time required between drivers became a significant performance cost as clock speeds rose.
- **DFT incompatibility** — tristate nets are difficult to control and observe during scan testing; ATPG tools struggle with bus contention states.
- **STA complexity** — N drivers on one net produce O(N²) timing arcs for static timing analysis.
- **Process scaling** — below 130nm, tristate buffers are physically larger than multiplexers for the same function; leakage through floating nets also becomes problematic.
- **FPGA removal** — Xilinx dropped internal TBUFs after Spartan-3 (~2003); Intel/Altera never supported them. Tools auto-convert to mux-based equivalents.

Modern replacements: crossbar (NxM mux fabric), multiplexed read/write buses, forwarding networks, and switched interconnects (AXI, NoC). These use more wires but are faster, testable, and timing-clean.

ARCH's restriction of `Tristate<T>` to pad-level I/O reflects this industry consensus. Internal buses should use ARCH's `bus` + `arbiter` constructs (mux-based), which the language already provides as first-class citizens. The type checker enforces this boundary: `tristate` on an internal module port is a compile error.

**6. First-Class Construct: pipeline**

A pipeline is a first-class Arch construct --- not a pattern you build from registers and muxes. The compiler understands stall, flush, and forward semantics natively and generates all hazard control logic automatically from declarative annotations.

**6.1 Declaration**

+--------------------------------------------------------------------+
| *pipeline.arch*                                                    |
|                                                                    |
| **pipeline** Decode                                                |
|                                                                    |
| **param** XLEN: **const** = 32;                                    |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** in_pc: **in** UInt\<XLEN\>;                               |
|                                                                    |
| **port** out_pc: **out** UInt\<XLEN\>;                             |
|                                                                    |
| **port** out_instr: **out** UInt\<32\>;                            |
|                                                                    |
| **stage** Fetch                                                    |
|                                                                    |
| **reg** pc: UInt\<XLEN\> **reset** rst => 0;                      |
|                                                                    |
| **seq** **on** clk rising                                          |
|                                                                    |
| pc \<= in_pc;                                                      |
|                                                                    |
| **end** **seq**                                                    |
|                                                                    |
| **end** **stage** Fetch                                            |
|                                                                    |
| **stage** Decode                                                   |
|                                                                    |
| **let** raw: UInt\<32\> = Fetch.pc;                                |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| out_pc = Fetch.pc;                                                 |
|                                                                    |
| out_instr = raw;                                                   |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **stage** Decode                                           |
|                                                                    |
| **stall** **when** Decode.out_instr == 0;                          |
|                                                                    |
| **flush** Fetch **when** branch_mispred;                           |
|                                                                    |
| **forward** Decode.rs1_val **from** Execute.alu_result             |
|                                                                    |
| **when** Execute.rd == Decode.rs1 **and** Execute.rd != 0;         |
|                                                                    |
| **end** **pipeline** Decode                                        |
+--------------------------------------------------------------------+

Each stage has a compiler-generated `valid_r` register that tracks whether the stage holds valid data. The first stage sets `valid_r <= 1` by default; downstream stages inherit from upstream. Users can override the first stage's valid in a `seq` block (e.g. `valid_r <= start;`) to gate pipeline entry on an input signal. The last stage's `valid_r` can be read in a `comb` block to drive a `done` output.

> ◈ stall, flush, and forward are declarative. The compiler generates all enable signals, bubble insertion logic, and bypass muxes. The designer describes intent; the compiler generates mechanism.

**7. First-Class Construct: fsm**

An fsm block declares a finite state machine with named states and exhaustive coverage enforced by the compiler. Missing transitions, undriven outputs in any state, and unreachable states are all compile-time errors.

FSMs support a `default ... end default` block that provides default combinational and/or sequential assignments emitted before the state `case` statement. This eliminates boilerplate assignments that would otherwise appear in every state body — states only override what differs. The default block may contain `comb ... end comb` and/or `seq on clk rising ... end seq` sub-blocks. Output ports that are not driven in a state body will naturally be X in the generated Verilog unless a default is provided via this block.

**Datapath Registers and Sequential Logic in FSMs**

FSMs may declare `reg` and `let` bindings at scope level (alongside `port` and `state`), and `seq on clk rising ... end seq` blocks inside state bodies (alongside `comb` and `transition`). This allows co-locating datapath logic with control logic — a significant readability improvement over SystemVerilog, where FSM state and datapath registers must be split across separate `always_ff` and `always_comb` blocks.

The compiler generates clean, separated SystemVerilog:

- A single `always_ff` block containing: reset logic for state register + all datapath registers, then `state_r <= state_next` followed by a `case` dispatching per-state `seq` assignments.
- An `always_comb` block for next-state transitions.
- An `always_comb` block for output logic (per-state `comb` assignments).

**7.1 Declaration**

+--------------------------------------------------------------------+
| *fsm.arch*                                                         |
|                                                                    |
| **fsm** TrafficLight                                               |
|                                                                    |
| **param** TIMER_W: **const** = 8;                                  |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** timer: **in** UInt\<TIMER_W\>;                            |
|                                                                    |
| **port** red:    **out** Bool;                                     |
|                                                                    |
| **port** yellow: **out** Bool;                                     |
|                                                                    |
| **port** green:  **out** Bool;                                     |
|                                                                    |
| **state** Red, Yellow, Green;                                      |
|                                                                    |
| **default** **state** Red;                                         |
|                                                                    |
| **default**                                                        |
|                                                                    |
|   **comb** red = false; yellow = false; green = false; **end comb**|
|                                                                    |
| **end** **default**                                                |
|                                                                    |
| **state** Red                                                      |
|                                                                    |
| **comb** red = true; **end** **comb**                              |
|                                                                    |
| **transition** **to** Green **when** timer == 0;                   |
|                                                                    |
| **end** **state** Red                                              |
|                                                                    |
| **state** Green                                                    |
|                                                                    |
| **comb** green = true; **end** **comb**                            |
|                                                                    |
| **transition** **to** Yellow **when** timer == 0;                  |
|                                                                    |
| **end** **state** Green                                            |
|                                                                    |
| **state** Yellow                                                   |
|                                                                    |
| **comb** yellow = true; **end** **comb**                           |
|                                                                    |
| **transition** **to** Red **when** timer == 0;                     |
|                                                                    |
| **end** **state** Yellow                                           |
|                                                                    |
| **end** **fsm** TrafficLight                                       |
+--------------------------------------------------------------------+

> *⚑ The compiler verifies: every state has at least one outgoing transition (dead-end states are a compile error); no two transitions from the same state can be simultaneously enabled. If no transition fires in a given cycle, the FSM holds in the current state — a catch-all `-> Self when true` is not required. Output ports not driven in a state and not covered by a `default` block will be X in the generated Verilog.*

**Transitions** use the `->` arrow syntax. The `when <cond>` clause is optional; omitting it produces an unconditional transition that always fires:

```
state Dispense
  seq
    dispense_item <= true;
  end seq
  -> ChangeCheck;        // always advances — no `when` needed
end state Dispense
```

Every state body must be closed with `end state Name`.

**7.2 FSM Default Block**

The `default ... end default` block provides default assignments that are emitted before the state `case` statement. It may contain `comb ... end comb` and/or `seq on clk rising ... end seq` sub-blocks:

```
default
  comb
    out_a = false;
    out_b = 0;
  end comb
  seq on clk rising
    data_reg <= 0;
  end seq
end default
```

**Semantics:**

- Default `comb` assignments are emitted at the top of the output `always_comb` block, before `case (state_r)`. States that override a signal simply reassign it inside their case branch.
- Default `seq` assignments are emitted at the top of the `always_ff` else branch, before the per-state `case`. States with `seq` blocks override as needed.
- Output ports not covered by either a default block or per-state comb assignments will be X (undriven) in the generated Verilog — this matches real hardware behavior.

**Generated SystemVerilog:**

```systemverilog
always_comb begin
  out_a = 1'b0;   // ← from default block
  out_b = 0;      // ← from default block
  case (state_r)
    FOO: begin
      out_a = 1'b1; // state-level override
    end
    default: ;     // other states inherit the defaults
  endcase
end
```

**7a. First-Class Construct: thread**

A `thread` block declares a sequential multi-cycle process inside a module. The compiler lowers each thread to a per-thread integer state register and associated `always_ff`/`always_comb` logic inside an auto-generated `_ModuleName_threads` submodule. The parent module retains non-thread items and wires the submodule outputs back by name.

Use `thread` instead of `fsm` when the behaviour is best expressed as straight-line sequential code with explicit wait points, rather than as a named-state machine with explicit transitions.

**7a.1 Declaration**

```
thread Name on clk rising, rst high
  [default when cond
    <seq assigns>
  end default]
  <body statements>
end thread Name
```

- `Name` — optional; auto-named `thread`, `thread1`, … if omitted
- `on clk rising` — clock port name and edge (`rising` or `falling`)
- `, rst high` — reset port name and active level (`high` or `low`)
- `default when cond … end default` — optional soft-reset clause (see §7a.4)
- Body is a sequence of statements (see §7a.2)

**Repeating vs once**: by default a thread loops — after the last body statement it returns to state 0. Write `thread once Name on …` to stop in the terminal state instead.

**Multiple threads** in one module are declared independently; they all compile into the same `_ModuleName_threads` submodule and share one `always_ff` block to avoid multi-driver conflicts.

**`generate_for` over threads:**

```
generate_for i in 0..NUM-1
  thread Worker_i on clk rising, rst high
    wait until active and queue[i].valid;
    // ... body referencing i
  end thread Worker_i
end generate_for
```

Expands to `NUM` independent threads, each with its own state register `_t{N}_state`.

**7a.2 Body Statements**

| Statement | Creates state boundary? | Notes |
|-----------|------------------------|-------|
| `x = expr` | No | Comb assign — drives output while FSM is in this state |
| `x <= expr` | No | Seq assign — register update fires when state exits |
| `if/else (no wait inside)` | No | Same-state conditional |
| `wait until cond;` | Yes | Advance when condition true |
| `wait N cycle;` | Yes | Stall exactly N clock cycles (counter-based) |
| `do { … } until cond;` | Yes | Hold state: drive comb outputs until condition fires |
| `for i in s..e { … } end for` | Yes (per iteration) | Runtime bound; `i` becomes `_loop_cnt` register |
| `lock res { … } end lock res` | Yes (per body state) | Acquire mutex; zero-cycle if uncontended |
| `fork … and … join` | Yes (product expansion) | Parallel branches; compiler generates product-state FSM |

**Trailing seq assigns** after the last `wait` in the body are merged into the preceding state's exit logic (guarded by its transition condition) to avoid a dead cycle.

**`if/else` with wait inside** is not supported — use separate threads or flatten the control flow.

**7a.3 Resource Locks**

```
resource chan_name;           // declared in the enclosing module

thread T on clk rising, rst high
  lock chan_name
    x = 1;
    y = 2;
  until accept;
  end lock chan_name
end thread T
```

For each `resource`, the compiler generates a fixed-priority combinational arbiter:

```
grant[0] = req[0]
grant[i] = req[i] && !grant[0] && … && !grant[i-1]
```

**Deadlock freedom** is a compile-time guarantee: with fixed priority the waits-for graph is acyclic — no circular wait can form. Thread 0 always makes progress and starvation is impossible as long as lock bodies terminate.

**Nested lock blocks are a compile error.** Nesting would allow a higher-priority thread to enter a critical section that a lower-priority thread is already executing (mutual exclusion violation). Sequential (non-nested) lock usage is safe.

Ports driven inside `lock` blocks that are also driven by other threads must be declared `shared(or)`.

**7a.4 `default when` (Soft Reset)**

```
thread T on clk rising, rst high
  default when start and not active_r
    active_r <= true;
    xfer_ctr <= 0;
  end default
  wait until active_r;
  // ...
end thread T
```

When `cond` is true, the thread executes the seq assigns and resets `_state` to 0, regardless of its current state. Equivalent to a synchronous soft reset that takes priority over all normal FSM transitions. Only seq assigns are permitted inside `default when`; comb assigns are silently ignored.

**7a.5 Shared Output Ports**

When multiple threads drive the same output port, the port must be declared `shared(or)`:

```
port r_ready: out Bool shared(or);
```

Each thread's contribution is OR-merged with the others. The default for all threads is 0; any thread asserting the port drives it high. For data signals where exactly one thread drives the port at a time (ensured by the lock arbiter), `shared(or)` still applies and the OR reduction is equivalent to a mux.

**7a.6 Generated Hardware**

Each thread `ti` with `n` states produces:

- `_t{i}_state`: `UInt<⌈log₂(n)⌉>` state register (reset to 0)
- `always_comb`: per-state `if (_t{i}_state == k)` blocks enabling comb outputs
- `always_ff`: per-state transition and seq-assign logic, merged with all other threads into one block
- `_t{i}_loop_cnt`: `UInt<W>` counter register (only when thread contains a `for` loop)
- `_t{i}_cnt`: `UInt<32>` counter register (only when thread uses `wait N cycle`)

The detailed lowering algorithm — including state partitioning rules, fork/join product expansion, lock state generation, and correctness invariants — is documented in `doc/thread_lowering_algorithm.md`.

**8. First-Class Construct: fifo**

A fifo is a first-class construct with compile-time-verified flow control. The designer specifies depth, width, and domain. The compiler generates the full implementation --- counters, full/empty flags, and gray-code pointer CDC for dual-clock FIFOs.

A **type parameter** is required to set the memory element width. The `push_data` and `pop_data` ports must reference this type parameter (e.g. `in WIDTH`), not a concrete type like `UInt<32>`. Omitting the type parameter is a compile error.

The optional `kind` keyword selects the buffering discipline (same syntax as `ram`). If omitted, the default is `fifo`.

| **Kind** | **Behaviour** | **Implementation** |
|----------|---------------|--------------------|
| `fifo` (default) | First-in, first-out (queue) | Circular buffer with read/write pointers |
| `lifo` | Last-in, first-out (stack) | Memory with single stack pointer; push writes at sp, pop reads at sp-1 |

> ◈ `kind lifo` is restricted to single-clock (synchronous) FIFOs. Dual-clock LIFO is a compile error --- stacks are inherently single-domain structures.

**8.1 Single-Clock FIFO**

+--------------------------------------------------------------------+
| *fifo_single.arch*                                                 |
|                                                                    |
| **fifo** TxQueue                                                   |
|                                                                    |
| **param** DEPTH: **const** = 16;                                   |
|                                                                    |
| **param** WIDTH: **type** = UInt\<32\>;                            |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** push_valid: **in** Bool;                                  |
|                                                                    |
| **port** push_ready: **out** Bool;                                 |
|                                                                    |
| **port** push_data: **in** WIDTH;                                  |
|                                                                    |
| **port** pop_valid: **out** Bool;                                  |
|                                                                    |
| **port** pop_ready: **in** Bool;                                   |
|                                                                    |
| **port** pop_data: **out** WIDTH;                                  |
|                                                                    |
| **port** **full**: **out** Bool;                                   |
|                                                                    |
| **port** **empty**: **out** Bool;                                  |
|                                                                    |
| **end** **fifo** TxQueue                                           |
+--------------------------------------------------------------------+

**8.2 Dual-Clock (Async) FIFO**

+--------------------------------------------------------------------+
| *fifo_async.arch*                                                  |
|                                                                    |
| **fifo** AsyncBridge                                               |
|                                                                    |
| **param** DEPTH: **const** = 32;                                   |
|                                                                    |
| **param** WIDTH: **type** = UInt\<8\>;                             |
|                                                                    |
| **port** wr_clk: **in** Clock\<FastDomain\>;                       |
|                                                                    |
| **port** rd_clk: **in** Clock\<SlowDomain\>;                       |
|                                                                    |
| **port** rst: **in** Reset\<Async\>;                               |
|                                                                    |
| **port** push_valid: **in** Bool;                                  |
|                                                                    |
| **port** push_ready: **out** Bool;                                 |
|                                                                    |
| **port** push_data: **in** WIDTH;                                  |
|                                                                    |
| **port** pop_valid: **out** Bool;                                  |
|                                                                    |
| **port** pop_ready: **in** Bool;                                   |
|                                                                    |
| **port** pop_data: **out** WIDTH;                                  |
|                                                                    |
| **end** **fifo** AsyncBridge                                       |
+--------------------------------------------------------------------+

> ◈ When two different Clock domains are detected on wr_clk and rd_clk, the compiler automatically selects gray-code pointer synchronisation. This can be overridden with an explicit sync: policy annotation inside the fifo body.

**8.2a LIFO (Stack)**

+--------------------------------------------------------------------+
| *lifo_stack.arch*                                                  |
|                                                                    |
| **fifo** LifoStack                                                 |
|                                                                    |
| **kind** lifo;                                                     |
|                                                                    |
| **param** DEPTH: **const** = 16;                                   |
|                                                                    |
| **param** TYPE: **type** = UInt\<8\>;                              |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** push_valid: **in** Bool;                                  |
|                                                                    |
| **port** push_ready: **out** Bool;                                 |
|                                                                    |
| **port** push_data: **in** TYPE;                                   |
|                                                                    |
| **port** pop_valid: **out** Bool;                                  |
|                                                                    |
| **port** pop_ready: **in** Bool;                                   |
|                                                                    |
| **port** pop_data: **out** TYPE;                                   |
|                                                                    |
| **port** **full**: **out** Bool;                                   |
|                                                                    |
| **port** **empty**: **out** Bool;                                  |
|                                                                    |
| **end** **fifo** LifoStack                                         |
+--------------------------------------------------------------------+

> ◈ The LIFO uses a single stack pointer `sp`. Push writes at `mem[sp]` and increments; pop decrements and reads `mem[sp-1]`. Simultaneous push+pop replaces the top of stack without changing the pointer. The same push/pop handshake protocol (valid/ready) is used for both FIFO and LIFO.

**8.2b Output Latency and FWFT** *(planned)*

The optional `latency` keyword controls how `pop_data` is driven:

| **Latency** | **pop_data source** | **Use case** |
|-------------|---------------------|--------------|
| `latency 0` (default) | Combinational read from memory array | Shallow FIFOs (depth <= 16); simple, lowest area |
| `latency 1` | Registered output with FWFT (first-word fall-through) prefetch | Deep FIFOs (depth > 16); timing-clean `pop_data` from a flop |

```
fifo DeepQueue
  param DEPTH: const = 256;
  param TYPE: type = UInt<64>;
  latency 1;                      // registered output + FWFT prefetch
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in TYPE;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out TYPE;
end fifo DeepQueue
```

**`latency 0` behavior (current):**

`pop_data` is driven by a combinational read from the memory array: `assign pop_data = mem[rd_ptr]`. Data is available on `pop_data` one cycle after a push to a previously empty FIFO (the `empty` flag clears on the next rising edge, exposing the combinational read). For large depths, this combinational read path through the memory array mux tree becomes a timing bottleneck.

**`latency 1` behavior (FWFT prefetch):**

The FIFO maintains an output register (`pop_data_r`). When the FIFO transitions from empty to non-empty, or after each successful pop, the FIFO automatically **prefetches** the next word: the read pointer advances and the memory output is captured into `pop_data_r` on the next clock edge.

From the consumer's perspective, the interface is identical: when `pop_valid` is high, `pop_data` holds valid data and asserting `pop_ready` consumes it. The difference is purely timing --- `pop_data` comes from a flop, not a memory read mux, giving a clean registered output suitable for deep FIFOs.

This is the same pattern used by Xilinx FIFO Generator's "First Word Fall Through" mode and Altera's "show-ahead" mode.

> ◈ The compiler does not auto-select latency --- the designer must choose explicitly. This follows ARCH's principle of no hidden behavior that affects timing.

**8.3 First-Class Construct: synchronizer**

A synchronizer is a first-class construct for clock domain crossing (CDC) of individual signals. While a dual-clock fifo handles bulk data transfer between domains, a synchronizer handles single signals --- control bits, status flags, counters, and event pulses. The designer declares the synchronization strategy via `kind` and the compiler generates the correct CDC logic, including all intermediate flip-flop stages, encoding/decoding, and protocol handshaking.

The `kind` keyword selects the synchronization strategy (same syntax as `ram`). If omitted, the default is `ff`. The `param STAGES` controls the flip-flop chain depth (default 2).

**8.3.1 Synchronizer Kinds**

| Kind | Signal Type | Description |
|------|-------------|-------------|
| `ff` (default) | `Bool` or 1-bit | N-stage flip-flop shift chain clocked on `dst_clk`. Best for single-bit level signals (e.g. status flags, enables). |
| `gray` | `UInt<N>` (multi-bit) | Binary-to-gray encode in source domain, N-stage FF chain, gray-to-binary decode in destination domain. Safe for multi-bit counters and pointers where only one bit changes per cycle. |
| `handshake` | Any type | Req/ack toggle protocol with synchronized control signals. Safe for arbitrary multi-bit data that changes infrequently. Higher latency than `gray` but works for any data pattern. |
| `reset` | `Bool` only | Reset synchronizer: asynchronous assert (immediate propagation), synchronous deassert through N-stage FF chain. Used for synchronizing reset deassertion to a clock domain. Compile error if data type is not `Bool`. |
| `pulse` | `Bool` only | Pulse synchronizer: converts a single-cycle pulse in the source domain into a level toggle, syncs the toggle through the N-stage FF chain, then edge-detects in the destination domain to regenerate a single-cycle pulse. Used for events, interrupts, and triggers across clock domains. Compile error if data type is not `Bool`. |

**8.3.2 Declaration**

+--------------------------------------------------------------------------+
| *sync_ff.arch --- 1-bit FF synchronizer (default kind)*                  |
|                                                                          |
| **synchronizer** StatusSync                                              |
|                                                                          |
| **param** STAGES: **const** = 2;                                         |
|                                                                          |
| **port** src_clk: **in** Clock\<SrcDomain\>;                             |
|                                                                          |
| **port** dst_clk: **in** Clock\<DstDomain\>;                             |
|                                                                          |
| **port** rst: **in** Reset\<Async\>;                                     |
|                                                                          |
| **port** data_in: **in** Bool;                                           |
|                                                                          |
| **port** data_out: **out** Bool;                                         |
|                                                                          |
| **end** **synchronizer** StatusSync                                      |
+--------------------------------------------------------------------------+

+--------------------------------------------------------------------------+
| *sync_gray.arch --- multi-bit gray-code synchronizer*                    |
|                                                                          |
| **synchronizer** PtrSync                                                 |
|                                                                          |
| **kind** gray;                                                           |
|                                                                          |
| **param** STAGES: **const** = 2;                                         |
|                                                                          |
| **port** src_clk: **in** Clock\<WrDomain\>;                              |
|                                                                          |
| **port** dst_clk: **in** Clock\<RdDomain\>;                              |
|                                                                          |
| **port** rst: **in** Reset\<Async\>;                                     |
|                                                                          |
| **port** data_in: **in** UInt\<5\>;                                      |
|                                                                          |
| **port** data_out: **out** UInt\<5\>;                                    |
|                                                                          |
| **end** **synchronizer** PtrSync                                         |
+--------------------------------------------------------------------------+

+--------------------------------------------------------------------------+
| *sync_handshake.arch --- multi-bit handshake synchronizer*               |
|                                                                          |
| **synchronizer** ConfigSync                                              |
|                                                                          |
| **kind** handshake;                                                      |
|                                                                          |
| **param** STAGES: **const** = 2;                                         |
|                                                                          |
| **port** src_clk: **in** Clock\<CpuDomain\>;                             |
|                                                                          |
| **port** dst_clk: **in** Clock\<AccelDomain\>;                           |
|                                                                          |
| **port** rst: **in** Reset\<Sync\>;                                      |
|                                                                          |
| **port** data_in: **in** UInt\<32\>;                                     |
|                                                                          |
| **port** data_out: **out** UInt\<32\>;                                   |
|                                                                          |
| **end** **synchronizer** ConfigSync                                      |
+--------------------------------------------------------------------------+

+--------------------------------------------------------------------------+
| *sync_reset.arch --- reset synchronizer*                                 |
|                                                                          |
| **synchronizer** RstSync                                                 |
|                                                                          |
| **kind** reset;                                                          |
|                                                                          |
| **param** STAGES: **const** = 3;                                         |
|                                                                          |
| **port** src_clk: **in** Clock\<PllDomain\>;                             |
|                                                                          |
| **port** dst_clk: **in** Clock\<CoreDomain\>;                            |
|                                                                          |
| **port** rst: **in** Reset\<Async\>;                                     |
|                                                                          |
| **port** data_in: **in** Bool;                                           |
|                                                                          |
| **port** data_out: **out** Bool;                                         |
|                                                                          |
| **end** **synchronizer** RstSync                                         |
+--------------------------------------------------------------------------+

+--------------------------------------------------------------------------+
| *sync_pulse.arch --- pulse synchronizer for events*                      |
|                                                                          |
| **synchronizer** EventSync                                               |
|                                                                          |
| **kind** pulse;                                                          |
|                                                                          |
| **param** STAGES: **const** = 2;                                         |
|                                                                          |
| **port** src_clk: **in** Clock\<SrcDomain\>;                             |
|                                                                          |
| **port** dst_clk: **in** Clock\<DstDomain\>;                             |
|                                                                          |
| **port** rst: **in** Reset\<Sync\>;                                      |
|                                                                          |
| **port** data_in: **in** Bool;                                           |
|                                                                          |
| **port** data_out: **out** Bool;                                         |
|                                                                          |
| **end** **synchronizer** EventSync                                       |
+--------------------------------------------------------------------------+

**8.3.3 Compile-Time Checks**

The compiler enforces the following rules for synchronizer constructs:

- The two `Clock<Domain>` ports must reference **different** domains. Same-domain clocks are a compile error --- use a direct assignment instead.
- `kind reset` and `kind pulse` require the data port type to be `Bool`. Multi-bit data is a compile error.
- `kind gray` requires `UInt<N>` data. The compiler warns if the source signal can change by more than one LSB per cycle (e.g. arbitrary data rather than a counter).
- `STAGES` must be at least 2. Values of 2 or 3 are typical; the compiler warns on values above 4 (diminishing returns, increased latency).

**8.3.4 Generated Hardware**

| Kind | SystemVerilog Output |
|------|---------------------|
| `ff` | Chain of `STAGES` flip-flops clocked on `dst_clk`: `always_ff @(posedge dst_clk)` with shift register `sync[0] <= data_in; sync[1] <= sync[0]; ...` |
| `gray` | Source domain: binary-to-gray encoder. Destination domain: `STAGES`-FF chain + gray-to-binary decoder. |
| `handshake` | Req toggle in source domain, req synchronized to destination domain via FF chain, ack toggle back to source domain via FF chain, data latched on ack. |
| `reset` | `always_ff @(posedge dst_clk or posedge data_in)`: async set on assertion, synchronous shift chain for deassertion. |
| `pulse` | Source domain: toggle register flipped on input pulse. Destination domain: `STAGES`-FF chain on toggle + XOR edge detector to regenerate single-cycle pulse. |

> ◈ The synchronizer construct and the dual-clock fifo are the two legal CDC crossing mechanisms in Arch. Any other cross-domain signal access is a compile error, with the error message directing the user to use either a synchronizer or an async fifo.

> ◈ **`--cdc-random` simulation flag.** When `arch sim` is invoked with `--cdc-random`, each synchronizer's FF-chain shift is probabilistically skipped on any given clock edge, adding +1 cycle of latency. This verifies that designs do not depend on exact synchronizer propagation delay. The probability is controlled by the `cdc_skip_pct` public member (0–100, default 25) on each generated C++ model, allowing testbenches to tune randomization intensity at runtime (e.g. `dut.cdc_skip_pct = 50;` for aggressive stress testing). Internally uses a 32-bit LFSR for deterministic pseudo-random sequencing.

> ◈ CDC checking extends across `inst` boundaries. When a parent module instantiates a child, the compiler traces clock port connections to map child domains to parent domains, then verifies that all data connections respect clock domain boundaries. If a signal from DomainA is connected to a port that operates in DomainB inside the child, the compiler reports a CDC violation --- the same error and guidance as for intra-module crossings.

**9. First-Class Construct: arbiter**

An arbiter manages N requesters competing for M shared resources. The designer declares port counts and an arbitration policy. The compiler generates all grant logic, stall propagation, and fairness guarantees.

**9.1 Declaration**

+--------------------------------------------------------------------+
| *arbiter_decl.arch*                                                |
|                                                                    |
| **arbiter** BusArbiter                                             |
|                                                                    |
| **param** NUM_REQ: **const** = 4;                                  |
|                                                                    |
| **param** NUM_RSRC: **const** = 1;                                 |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| ports\[NUM_REQ\] **request**                                       |
|                                                                    |
| **valid**: **in** Bool;                                            |
|                                                                    |
| **ready**: **out** Bool;                                           |
|                                                                    |
| **end** ports                                                      |
|                                                                    |
| ports\[NUM_RSRC\] **grant**                                        |
|                                                                    |
| **valid**: **out** Bool;                                           |
|                                                                    |
| requester: **out** UInt\<\$clog2(NUM_REQ)\>;                       |
|                                                                    |
| **end** ports                                                      |
|                                                                    |
| **policy** round_robin;                                            |
|                                                                    |
| **end** **arbiter** BusArbiter                                     |
+--------------------------------------------------------------------+

**9.2 Arbitration Policies**

  ------------------------------------------------------------------------------------------------------------------------------
  **Policy**          **Fairness**                                      **Latency**   **Best For**
  ------------------- ------------------------------------------------- ------------- ------------------------------------------
  **round_robin**     Strong --- every requester served in ≤ N cycles   O(N)          General-purpose bus arbitration

  **priority**        None --- highest-index wins unconditionally       O(1)          Interrupt controllers, emergency paths

  **weighted\<W\>**   Proportional to weight vector W                   O(N)          QoS-aware arbitration, bandwidth shaping

  **lru**             Approximate --- least-recently-used first         O(N)          Cache miss handling, memory controllers

  **custom fn**       User-defined --- compiler checks liveness         O(?)          Specialised protocols
  ------------------------------------------------------------------------------------------------------------------------------

**9.3 Custom Policy with hook**

When a built-in policy does not fit, the designer defines a custom grant function and binds it to the arbiter with a **hook** declaration. A hook is a named function binding inside a construct --- it declares the expected signature and maps it to a user-defined **function** in the same file.

+--------------------------------------------------------------------------------+
| *custom_arbiter.arch*                                                          |
|                                                                                |
| **function** MyGrantFn(req_mask: UInt\<4\>,                                    |
|                        last_grant: UInt\<4\>,                                  |
|                        extra: UInt\<8\>) -\> UInt\<4\>                         |
|   **let** masked: UInt\<4\> = req_mask & (last_grant \^ 0xF);                 |
|   **let** pick: UInt\<4\>   = masked != 0 ? masked : req_mask;                |
|   **let** pick_neg: UInt\<5\> = (pick \^ 0xF).zext\<5\>() + 1;               |
|   **return** pick & pick_neg.trunc\<4\>();                                     |
| **end** **function** MyGrantFn                                                 |
|                                                                                |
| **arbiter** CustomArb                                                          |
|   **policy** MyGrantFn;                                                        |
|   **param** N: **const** = 4;                                                  |
|   **port** clk:        **in** Clock\<SysDomain\>;                              |
|   **port** rst:        **in** Reset\<Sync\>;                                   |
|   **port** extra_port: **in** UInt\<8\>;                                       |
|   ports\[N\] **req**                                                           |
|     **valid**: **in** Bool;                                                    |
|     **ready**: **out** Bool;                                                   |
|   **end** ports **req**                                                        |
|   **port** grant_valid:     **out** Bool;                                      |
|   **port** grant_requester: **out** UInt\<2\>;                                 |
|   **hook** grant_select(req_mask: UInt\<4\>,                                   |
|                         last_grant: UInt\<4\>,                                 |
|                         extra_port: UInt\<8\>) -\> UInt\<4\>                   |
|     = MyGrantFn(req_mask, last_grant, extra_port);                             |
| **end** **arbiter** CustomArb                                                  |
+--------------------------------------------------------------------------------+

**hook semantics:**

- A hook declares a function signature and binds it to a concrete function with `= FnName(args);`.
- Hook arguments bind to: internal signals generated by the construct (e.g. `req_mask`, `last_grant`) or user-declared ports and params on the enclosing construct (e.g. `extra_port`).
- The bound function is emitted inline inside the generated SV module --- no separate module instantiation.
- Missing a required hook in a construct that declares one (or in a module that `implements` a template requiring one) is a compile error.
- Hooks are also used inside **template** contracts (see §24A) to specify required function bindings.

**10. First-Class Construct: regfile**

A regfile declares a structured register file with typed entries, multiple read ports, and multiple write ports. The compiler generates the read mux, write enable logic, and optionally write-before-read forwarding.

**10.1 Declaration**

+--------------------------------------------------------------------+
| *regfile.arch*                                                     |
|                                                                    |
| **regfile** IntRegs                                                |
|                                                                    |
| **param** XLEN: **const** = 32;                                    |
|                                                                    |
| **param** NREGS: **const** = 32;                                   |
|                                                                    |
| **param** NREAD: **const** = 2;                                    |
|                                                                    |
| **param** NWRITE: **const** = 1;                                   |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| ports\[NREAD\] **read**                                            |
|                                                                    |
| addr: **in** UInt\<\$clog2(NREGS)\>;                               |
|                                                                    |
| data: **out** UInt\<XLEN\>;                                        |
|                                                                    |
| **end** ports                                                      |
|                                                                    |
| ports\[NWRITE\] **write**                                          |
|                                                                    |
| en: **in** Bool;                                                   |
|                                                                    |
| addr: **in** UInt\<\$clog2(NREGS)\>;                               |
|                                                                    |
| data: **in** UInt\<XLEN\>;                                         |
|                                                                    |
| **end** ports                                                      |
|                                                                    |
| **init** \[0\] = 0;                                                |
|                                                                    |
| **forward** write_before_read: true;                               |
|                                                                    |
| **end** **regfile** IntRegs                                        |
+--------------------------------------------------------------------+

> ◈ forward write_before_read: true generates bypass muxes so a write and read to the same address in the same cycle returns the new value. Setting it to false generates simpler hardware and surfaces the hazard to the pipeline.

**11. First-Class Construct: ram**

A ram is a first-class construct that maps to a physical memory --- FPGA BRAM, distributed RAM, or ASIC SRAM macro. Unlike regfile (which always uses flip-flops), ram targets technology memory primitives. The designer declares the port topology, read timing, and a set of logical variables that live inside the RAM\'s address space. The compiler assigns address ranges, computes total depth and word width, and translates all logical accesses to physical addresses --- no manual address arithmetic required.

**11.1 Port Topologies**

  -------------------------------------------------------------------------------------------------------------------------------------
  **Topology**           **Keyword**        **Simultaneous Access**                        **Typical Mapping**
  ---------------------- ------------------ ---------------------------------------------- --------------------------------------------
  **Single port**        kind single        Read or write, never both                      Small LUT RAM, single-port SRAM macro

  **Simple dual port**   kind simple_dual   Read on one port, write on the other           Standard FPGA BRAM (most common mode)

  **True dual port**     kind true_dual     Any combination on both ports simultaneously   True dual-port BRAM, high-bandwidth caches

  **ROM**                kind rom           Read-only, no write ports                      Lookup tables, microcode, boot ROM
  -------------------------------------------------------------------------------------------------------------------------------------

ROM requires an `init` clause. No write-enable or write-data signals are permitted.

**11.2 Read Timing Modes**

  -------------------------------------------------------------------------------------------------------------------------------------
  **Mode**             **Latency**   **Behaviour**                                              **Use When**
  -------------------- ------------- ---------------------------------------------------------- ---------------------------------------
  **latency 0**        0 cycles      Output is combinationally derived from address             Register files, small look-up tables

  **latency 1**        1 cycle       Address and enable registered; data available next cycle   Standard BRAM, most cache SRAMs

  **latency 2**        2 cycles      Both input and output registered for max frequency         Timing-critical paths, deep pipelines
  -------------------------------------------------------------------------------------------------------------------------------------

**11.3 Single-Port RAM**

+--------------------------------------------------------------------------------+
| *ram_single.arch*                                                              |
|                                                                                |
| // Single-port ROM/RAM --- one port shared for reads and writes                |
|                                                                                |
| ram InstrRom                                                                   |
|                                                                                |
| **param** DEPTH: **const** = 4096;                                             |
|                                                                                |
| **port** clk: **in** Clock\<SysDomain\>;                                       |
|                                                                                |
| kind single;                                                                   |
|                                                                                |
| **read**: **sync**;                                                            |
|                                                                                |
| **write**: first; // write_first \| read_first \| no_change                    |
|                                                                                |
| // One logical variable filling the whole address space                        |
|                                                                                |
| store                                                                          |
|                                                                                |
| instructions: Vec\<UInt\<32\>, DEPTH\>;                                        |
|                                                                                |
| **end** store                                                                  |
|                                                                                |
| **port** access                                                                |
|                                                                                |
| en: **in** Bool;                                                               |
|                                                                                |
| wen: **in** Bool;                                                              |
|                                                                                |
| addr: **in** UInt\<\$clog2(DEPTH)\>;                                           |
|                                                                                |
| wdata: **in** UInt\<32\>;                                                      |
|                                                                                |
| rdata: **out** UInt\<32\>;                                                     |
|                                                                                |
| **end** **port** access                                                        |
|                                                                                |
| **init**: file \"instr_rom.hex\"; // load from hex file at simulation/power-on |
|                                                                                |
| **end** ram InstrRom                                                           |
+--------------------------------------------------------------------------------+

**11.4 Simple Dual-Port RAM**

+-----------------------------------------------------------------------------------+
| *ram_simple_dual.arch*                                                            |
|                                                                                   |
| // Simple dual-port --- dedicated read port and write port                        |
|                                                                                   |
| ram DataCache                                                                     |
|                                                                                   |
| **param** DEPTH: **const** = 1024;                                                |
|                                                                                   |
| **param** LINE: **type** = CacheLine; // compiler computes word width from struct |
|                                                                                   |
| **port** clk: **in** Clock\<SysDomain\>;                                          |
|                                                                                   |
| **port** rst: **in** Reset\<Sync\>;                                               |
|                                                                                   |
| kind simple_dual;                                                                 |
|                                                                                   |
| **read**: **sync**;                                                               |
|                                                                                   |
| store                                                                             |
|                                                                                   |
| lines: Vec\<LINE, DEPTH\>;                                                        |
|                                                                                   |
| **end** store                                                                     |
|                                                                                   |
| **port** read_port                                                                |
|                                                                                   |
| en: **in** Bool;                                                                  |
|                                                                                   |
| addr: **in** UInt\<\$clog2(DEPTH)\>;                                              |
|                                                                                   |
| data: **out** LINE;                                                               |
|                                                                                   |
| **end** **port** read_port                                                        |
|                                                                                   |
| **port** write_port                                                               |
|                                                                                   |
| en: **in** Bool;                                                                  |
|                                                                                   |
| addr: **in** UInt\<\$clog2(DEPTH)\>;                                              |
|                                                                                   |
| data: **in** LINE;                                                                |
|                                                                                   |
| mask: **in** Vec\<Bool, \$bytes(LINE)\>; // optional byte-enable mask             |
|                                                                                   |
| **end** **port** write_port                                                       |
|                                                                                   |
| **end** ram DataCache                                                             |
+-----------------------------------------------------------------------------------+

**11.5 True Dual-Port RAM**

+---------------------------------------------------------------------------------+
| *ram_true_dual.arch*                                                            |
|                                                                                 |
| // True dual-port --- both ports can read or write independently                |
|                                                                                 |
| ram SharedScratchpad                                                            |
|                                                                                 |
| **param** DEPTH: **const** = 512;                                               |
|                                                                                 |
| **param** WIDTH: **type** = UInt\<64\>;                                         |
|                                                                                 |
| **port** clk_a: **in** Clock\<CoreADomain\>;                                    |
|                                                                                 |
| **port** clk_b: **in** Clock\<CoreBDomain\>; // may be same or different domain |
|                                                                                 |
| kind true_dual;                                                                 |
|                                                                                 |
| **read**: **sync**;                                                             |
|                                                                                 |
| store                                                                           |
|                                                                                 |
| data: Vec\<WIDTH, DEPTH\>;                                                      |
|                                                                                 |
| **end** store                                                                   |
|                                                                                 |
| // Port A --- full read/write                                                   |
|                                                                                 |
| **port** a                                                                      |
|                                                                                 |
| en: **in** Bool;                                                                |
|                                                                                 |
| wen: **in** Bool;                                                               |
|                                                                                 |
| addr: **in** UInt\<\$clog2(DEPTH)\>;                                            |
|                                                                                 |
| wdata: **in** WIDTH;                                                            |
|                                                                                 |
| rdata: **out** WIDTH;                                                           |
|                                                                                 |
| **end** **port** a                                                              |
|                                                                                 |
| // Port B --- full read/write                                                   |
|                                                                                 |
| **port** b                                                                      |
|                                                                                 |
| en: **in** Bool;                                                                |
|                                                                                 |
| wen: **in** Bool;                                                               |
|                                                                                 |
| addr: **in** UInt\<\$clog2(DEPTH)\>;                                            |
|                                                                                 |
| wdata: **in** WIDTH;                                                            |
|                                                                                 |
| rdata: **out** WIDTH;                                                           |
|                                                                                 |
| **end** **port** b                                                              |
|                                                                                 |
| // Collision policy when both ports access the same address simultaneously      |
|                                                                                 |
| collision: port_a_wins; // port_a_wins \| port_b_wins \| undefined              |
|                                                                                 |
| **end** ram SharedScratchpad                                                    |
+---------------------------------------------------------------------------------+

**11.6 Multi-Variable Mapping (compiler-managed address layout)**

A ram\'s store block can declare multiple logical variables of different types and depths. The compiler assigns non-overlapping address ranges automatically and translates all logical accesses to physical addresses. The designer never computes base offsets manually.

+--------------------------------------------------------------------------------------------+
| *ram_multi_var.arch*                                                                       |
|                                                                                            |
| // One physical RAM holding integer registers, FP registers, and CSRs                      |
|                                                                                            |
| ram UnifiedRegRam                                                                          |
|                                                                                            |
| **port** clk: **in** Clock\<SysDomain\>;                                                   |
|                                                                                            |
| kind simple_dual;                                                                          |
|                                                                                            |
| **read**: **async**; // register-file mode --- combinational read                          |
|                                                                                            |
| // Compiler lays these out contiguously and picks the widest type as word width.           |
|                                                                                            |
| // Address map (auto-assigned):                                                            |
|                                                                                            |
| // int_regs → 0x000 .. 0x01F (32 entries)                                                  |
|                                                                                            |
| // fp_regs → 0x020 .. 0x03F (32 entries)                                                   |
|                                                                                            |
| // csr_regs → 0x040 .. 0x43F (1024 entries)                                                |
|                                                                                            |
| // Total depth: 1088, word width: 64 bits                                                  |
|                                                                                            |
| store                                                                                      |
|                                                                                            |
| int_regs: Vec\<UInt\<64\>, 32\>;                                                           |
|                                                                                            |
| fp_regs: Vec\<UInt\<64\>, 32\>;                                                            |
|                                                                                            |
| csr_regs: Vec\<UInt\<64\>, 1024\>;                                                         |
|                                                                                            |
| **end** store                                                                              |
|                                                                                            |
| **port** read_port                                                                         |
|                                                                                            |
| en: **in** Bool;                                                                           |
|                                                                                            |
| addr: **in** UInt\<\$clog2(1088)\>;                                                        |
|                                                                                            |
| data: **out** UInt\<64\>;                                                                  |
|                                                                                            |
| **end** **port** read_port                                                                 |
|                                                                                            |
| **port** write_port                                                                        |
|                                                                                            |
| en: **in** Bool;                                                                           |
|                                                                                            |
| addr: **in** UInt\<\$clog2(1088)\>;                                                        |
|                                                                                            |
| data: **in** UInt\<64\>;                                                                   |
|                                                                                            |
| **end** **port** write_port                                                                |
|                                                                                            |
| **end** ram UnifiedRegRam                                                                  |
|                                                                                            |
| // ── Accessing logical variables by name ─────────────────────────────                    |
|                                                                                            |
| // The compiler translates logical names to physical addresses automatically.              |
|                                                                                            |
| // No manual offset arithmetic ever appears in Arch source.                                |
|                                                                                            |
| **inst** regram: UnifiedRegRam                                                             |
|                                                                                            |
| **clk \<- clk;                                                                   |
|                                                                                            |
| // Access int_regs\[rs1\] --- compiler emits physical address rs1 + base(int_regs)         |
|                                                                                            |
| **read_port.en \<- true;                                                         |
|                                                                                            |
| **read_port.addr \<- int_regs\[rs1_addr\];                                       |
|                                                                                            |
| **read_port.data -\> rs1_val;                                                    |
|                                                                                            |
| // Access csr_regs\[csr_idx\] --- compiler emits physical address csr_idx + base(csr_regs) |
|                                                                                            |
| **write_port.en \<- csr_wen;                                                     |
|                                                                                            |
| **write_port.addr \<- csr_regs\[csr_idx\];                                       |
|                                                                                            |
| **write_port.data \<- csr_wdata;                                                 |
|                                                                                            |
| **end** **inst** regram                                                                    |
+--------------------------------------------------------------------------------------------+

> *⚑ The logical variable syntax int_regs\[index\] in a connect address expression is resolved entirely at compile time to physical_base + index. The generated SystemVerilog contains only numeric address expressions --- no overhead, no indirection.*

**11.7 Struct Types as RAM Words**

Any Arch struct can be used as the word type of a ram. The compiler packs the struct fields into the physical word width and generates the appropriate bit-slice assignments in the output SystemVerilog.

+-----------------------------------------------------------------------------+
| *ram_struct.arch*                                                           |
|                                                                             |
| **struct** CacheLine                                                        |
|                                                                             |
| **valid**: Bool,                                                            |
|                                                                             |
| dirty: Bool,                                                                |
|                                                                             |
| tag: UInt\<20\>,                                                            |
|                                                                             |
| data: Vec\<UInt\<32\>, 16\>, // 16-word cache line                          |
|                                                                             |
| **end** **struct** CacheLine                                                |
|                                                                             |
| // Compiler computes: word width = 1+1+20+(32×16) = 534 bits                |
|                                                                             |
| ram L1Cache                                                                 |
|                                                                             |
| **param** SETS: **const** = 256;                                            |
|                                                                             |
| **port** clk: **in** Clock\<SysDomain\>;                                    |
|                                                                             |
| kind simple_dual;                                                           |
|                                                                             |
| **read**: **sync**;                                                         |
|                                                                             |
| store                                                                       |
|                                                                             |
| lines: Vec\<CacheLine, SETS\>;                                              |
|                                                                             |
| **end** store                                                               |
|                                                                             |
| **port** read_port                                                          |
|                                                                             |
| en: **in** Bool;                                                            |
|                                                                             |
| addr: **in** UInt\<8\>;                                                     |
|                                                                             |
| data: **out** CacheLine; // returned as a fully decoded struct              |
|                                                                             |
| **end** **port** read_port                                                  |
|                                                                             |
| **port** write_port                                                         |
|                                                                             |
| en: **in** Bool;                                                            |
|                                                                             |
| addr: **in** UInt\<8\>;                                                     |
|                                                                             |
| data: **in** CacheLine;                                                     |
|                                                                             |
| **end** **port** write_port                                                 |
|                                                                             |
| **end** ram L1Cache                                                         |
|                                                                             |
| // Field access on the read result --- compiler emits the correct bit slice |
|                                                                             |
| **let** line: CacheLine = l1_rdata;                                         |
|                                                                             |
| **let** hit: Bool = line.**valid** **and** (line.tag == req_tag);           |
+-----------------------------------------------------------------------------+

> ◈ When a struct is the word type, the compiler automatically generates the packing/unpacking bit slices in the emitted SystemVerilog. Designers work entirely with named fields --- no manual \[hi:lo\] indexing into packed words.

**11.8 RAM Initialization**

  ----------------------------------------------------------------------------------------------------------------------------
  **Annotation**                **Behaviour**                                 **Supported In**
  ----------------------------- --------------------------------------------- ------------------------------------------------
  **init: zero**                All words set to zero at reset                Simulation and FPGA (BRAM init)

  **init: none**                Words undefined until written                 ASIC default; simulation X-propagation enabled

  **init: file("path", hex)**   Load hex file (`$readmemh`)                   Simulation and FPGA bitstream

  **init: file("path", bin)**   Load binary file (`$readmemb`)                Simulation and FPGA

  **init: value expr**          All words set to the given const expression   Simulation and FPGA

  **init: [v0, v1, ...]**       Inline array of integer literals              Simulation and FPGA (ROM tables)
  ----------------------------------------------------------------------------------------------------------------------------

**11.9 RAM vs regfile --- When to Use Which**

  -----------------------------------------------------------------------------------------------------------
                         **regfile**              **ram**
  ---------------------- ------------------------ -----------------------------------------------------------
  **Implementation**     Flip-flops (always)      Technology memory primitive (BRAM / SRAM macro)

  **Read latency**       Zero (async only)        Zero (async), one cycle (sync), two cycles (sync_out)

  **Depth**              Small (≤ 64 typically)   Any --- limited by target technology

  **Variable mapping**   Single flat array        Multiple typed logical variables, compiler assigns layout

  **Struct word type**   No                       Yes --- compiler packs/unpacks fields automatically

  **Byte enable mask**   No                       Yes --- mask port on write_port

  **FPGA primitive**     LUT / distributed RAM    BRAM18 / BRAM36 / URAM

  **ASIC target**        Std-cell array           SRAM macro (requires technology library)
  -----------------------------------------------------------------------------------------------------------

**12. First-Class Construct: linklist**

A linklist is a first-class Arch construct that maps a hardware-native linked list to a physical node pool RAM plus a compiler-generated controller IP. In hardware, pointers do not exist --- a linklist uses slot indices into a fixed-depth node pool as its internal references. The designer declares the pool depth, payload type, list topology, and a latency budget per operation. The compiler generates the node pool RAM, free-list FIFO, head/tail registers, and a controller FSM that meets the declared latency constraints.

**12.1 Physical Implementation Model**

Every linklist declaration compiles to the following hardware components, generated and wired automatically:

  ---------------------------------------------------------------------------------------------------------------------------------
  **Component**        **Implementation**                                      **Purpose**
  -------------------- ------------------------------------------------------- ----------------------------------------------------
  **Node pool RAM**    simple_dual RAM, depth × (\|data\| + \|ptr\| × links)   Stores payload and next/prev pointer per slot

  **Free list**        fifo, depth entries of UInt\<\$clog2(depth)\>           Tracks available slot indices for alloc/free

  **Head register**    reg, UInt\<\$clog2(depth)\>                             Points to the first node

  **Tail register**    reg, UInt\<\$clog2(depth)\>                             Points to the last node (if tail tracking enabled)

  **Length counter**   reg, UInt\<\$clog2(depth+1)\>                           Optional; tracks current list length

  **Controller FSM**   fsm, compiler-generated                                 Sequences all multi-cycle operations
  ---------------------------------------------------------------------------------------------------------------------------------

> *⚑ The slot index width is always \$clog2(depth) --- computed automatically. Designers never declare or manipulate raw pointer widths.*

**12.2 List Topology**

  -------------------------------------------------------------------------------------------------------------------------------------------
  **kind**              **Links per node**       **delete cost**                              **Best For**
  --------------------- ------------------------ -------------------------------------------- -----------------------------------------------
  **singly**            next only                O(N) --- must traverse to find predecessor   Queues, stacks, simple ordered lists

  **doubly**            next + prev              O(1) --- predecessor known directly          LRU lists, priority queues, reorderable lists

  **circular_singly**   next (tail→head)         O(N)                                         Token ring, round-robin scheduling

  **circular_doubly**   next + prev (circular)   O(1)                                         High-performance scheduling, deques
  -------------------------------------------------------------------------------------------------------------------------------------------

**12.3 Operation Latency**

Every operation on a linklist has a latency declaration --- the number of clock cycles from request assertion to result valid. The compiler uses this budget to choose an implementation: lower latency requires simpler (potentially slower-clocking) logic; higher latency allows pipelining for higher Fmax.

  --------------------------------------------------------------------------------------------------------------
  **Operation**         **Minimum Latency**   **Typical Latency**   **Notes**
  --------------------- --------------------- --------------------- --------------------------------------------
  **alloc**             1 cycle               1 cycle               Pop from free-list FIFO --- always O(1)

  **free**              1 cycle               1 cycle               Push to free-list FIFO --- always O(1)

  **insert_head**       1 cycle               2 cycles              Write node pool, update head register

  **insert_tail**       1 cycle               2 cycles              Write node pool, update tail register

  **insert_after(h)**   1 cycle               2 cycles              Write node, patch next/prev pointers

  **delete_head**       1 cycle               2 cycles              Read next, update head, free slot

  **delete(h)**         1 cycle (doubly)      2--3 cycles           singly: O(N) traverse; doubly: O(1)

  **next(h)**           1 cycle               1 cycle               One RAM read for next pointer

  **prev(h)**           1 cycle               1 cycle               doubly only; one RAM read for prev pointer

  **read_data(h)**      1 cycle               1--2 cycles           Read payload at slot h

  **write_data(h)**     1 cycle               1--2 cycles           Write payload at slot h

  **search(pred)**      N cycles              N cycles              Worst case: full traversal of N nodes

  **length**            0 cycles              0 cycles              Combinational from length counter register
  --------------------------------------------------------------------------------------------------------------

**12.4 Declaration --- Singly-Linked Queue**

+-----------------------------------------------------------------------------+
| *linklist_singly.arch*                                                      |
|                                                                             |
| // A hardware task queue --- singly linked, 64 nodes, 3-cycle insert/delete |
|                                                                             |
| linklist TaskQueue                                                          |
|                                                                             |
| **param** DEPTH: **const** = 64;                                            |
|                                                                             |
| **param** DATA: **type** = TaskDescriptor;                                  |
|                                                                             |
| **port** clk: **in** Clock\<SysDomain\>;                                    |
|                                                                             |
| **port** rst: **in** Reset\<Sync\>;                                         |
|                                                                             |
| kind singly;                                                                |
|                                                                             |
| track tail: true; // maintain a tail pointer for O(1) insert_tail           |
|                                                                             |
| track length: true; // maintain a length counter                            |
|                                                                             |
| // ── Operation port bundles ────────────────────────────────────           |
|                                                                             |
| // Each op has a valid/ready handshake and a declared latency.              |
|                                                                             |
| // The compiler generates the controller FSM to meet each budget.           |
|                                                                             |
| op alloc                                                                    |
|                                                                             |
| latency: 1;                                                                 |
|                                                                             |
| **port** req_valid: **in** Bool;                                            |
|                                                                             |
| **port** req_ready: **out** Bool;                                           |
|                                                                             |
| **port** resp_valid: **out** Bool;                                          |
|                                                                             |
| **port** resp_handle: **out** UInt\<\$clog2(DEPTH)\>; // the new slot index |
|                                                                             |
| **end** op alloc                                                            |
|                                                                             |
| op free                                                                     |
|                                                                             |
| latency: 1;                                                                 |
|                                                                             |
| **port** req_valid: **in** Bool;                                            |
|                                                                             |
| **port** req_ready: **out** Bool;                                           |
|                                                                             |
| **port** req_handle: **in** UInt\<\$clog2(DEPTH)\>;                         |
|                                                                             |
| **end** op free                                                             |
|                                                                             |
| op insert_tail                                                              |
|                                                                             |
| latency: 2;                                                                 |
|                                                                             |
| **port** req_valid: **in** Bool;                                            |
|                                                                             |
| **port** req_ready: **out** Bool;                                           |
|                                                                             |
| **port** req_data: **in** DATA;                                             |
|                                                                             |
| **port** resp_valid: **out** Bool;                                          |
|                                                                             |
| **port** resp_handle: **out** UInt\<\$clog2(DEPTH)\>;                       |
|                                                                             |
| **end** op insert_tail                                                      |
|                                                                             |
| op delete_head                                                              |
|                                                                             |
| latency: 2;                                                                 |
|                                                                             |
| **port** req_valid: **in** Bool;                                            |
|                                                                             |
| **port** req_ready: **out** Bool;                                           |
|                                                                             |
| **port** resp_valid: **out** Bool;                                          |
|                                                                             |
| **port** resp_data: **out** DATA;                                           |
|                                                                             |
| **end** op delete_head                                                      |
|                                                                             |
| op read_data                                                                |
|                                                                             |
| latency: 1;                                                                 |
|                                                                             |
| **port** req_valid: **in** Bool;                                            |
|                                                                             |
| **port** req_handle: **in** UInt\<\$clog2(DEPTH)\>;                         |
|                                                                             |
| **port** resp_valid: **out** Bool;                                          |
|                                                                             |
| **port** resp_data: **out** DATA;                                           |
|                                                                             |
| **end** op read_data                                                        |
|                                                                             |
| // Combinational status --- no latency                                      |
|                                                                             |
| **port** **empty**: **out** Bool;                                           |
|                                                                             |
| **port** **full**: **out** Bool;                                            |
|                                                                             |
| **port** length: **out** UInt\<\$clog2(DEPTH+1)\>;                          |
|                                                                             |
| **end** linklist TaskQueue                                                  |
+-----------------------------------------------------------------------------+

**12.5 Declaration --- Doubly-Linked LRU List**

+------------------------------------------------------------------------------------+
| *linklist_doubly.arch*                                                             |
|                                                                                    |
| // Doubly-linked list for an LRU cache eviction policy.                            |
|                                                                                    |
| // O(1) delete anywhere --- no traversal needed because prev pointer is available. |
|                                                                                    |
| linklist LruList                                                                   |
|                                                                                    |
| **param** DEPTH: **const** = 256;                                                  |
|                                                                                    |
| **param** DATA: **type** = UInt\<\$clog2(DEPTH)\>; // payload = cache set index    |
|                                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                                           |
|                                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                                |
|                                                                                    |
| kind doubly;                                                                       |
|                                                                                    |
| track tail: true;                                                                  |
|                                                                                    |
| track length: false; // not needed for LRU                                         |
|                                                                                    |
| op insert_head                                                                     |
|                                                                                    |
| latency: 2;                                                                        |
|                                                                                    |
| **port** req_valid: **in** Bool;                                                   |
|                                                                                    |
| **port** req_ready: **out** Bool;                                                  |
|                                                                                    |
| **port** req_data: **in** DATA;                                                    |
|                                                                                    |
| **port** resp_valid: **out** Bool;                                                 |
|                                                                                    |
| **port** resp_handle: **out** UInt\<\$clog2(DEPTH)\>;                              |
|                                                                                    |
| **end** op insert_head                                                             |
|                                                                                    |
| // delete(h): O(1) because prev pointer known --- doubly-linked                    |
|                                                                                    |
| op delete                                                                          |
|                                                                                    |
| latency: 3; // read prev+next, write two pointer patches                           |
|                                                                                    |
| **port** req_valid: **in** Bool;                                                   |
|                                                                                    |
| **port** req_ready: **out** Bool;                                                  |
|                                                                                    |
| **port** req_handle: **in** UInt\<\$clog2(DEPTH)\>;                                |
|                                                                                    |
| **port** resp_valid: **out** Bool;                                                 |
|                                                                                    |
| **end** op delete                                                                  |
|                                                                                    |
| // Evict: read and remove the tail node (LRU victim)                               |
|                                                                                    |
| op evict_tail                                                                      |
|                                                                                    |
| latency: 2;                                                                        |
|                                                                                    |
| **port** req_valid: **in** Bool;                                                   |
|                                                                                    |
| **port** req_ready: **out** Bool;                                                  |
|                                                                                    |
| **port** resp_valid: **out** Bool;                                                 |
|                                                                                    |
| **port** resp_data: **out** DATA;                                                  |
|                                                                                    |
| **end** op evict_tail                                                              |
|                                                                                    |
| **port** **empty**: **out** Bool;                                                  |
|                                                                                    |
| **port** **full**: **out** Bool;                                                   |
|                                                                                    |
| **end** linklist LruList                                                           |
+------------------------------------------------------------------------------------+

**12.6 Multi-List Shared Node Pool**

Multiple linklists can share a single physical node pool RAM --- directly analogous to the ram construct\'s multi-variable store block. The compiler manages a unified free list across all lists declared in the pool, computes the total depth, and assigns non-overlapping slot ranges. This is the hardware equivalent of a shared heap.

+--------------------------------------------------------------------+
| *linklist_pool.arch*                                               |
|                                                                    |
| // Shared node pool for two independent lists.                     |
|                                                                    |
| // Physical RAM: depth = 64 + 32 = 96 nodes.                       |
|                                                                    |
| // Unified free list: 96-entry FIFO of 7-bit slot indices.         |
|                                                                    |
| // Compiler assigns:                                               |
|                                                                    |
| // high_pri → slots 0 .. 63                                        |
|                                                                    |
| // low_pri → slots 64 .. 95                                        |
|                                                                    |
| linklist_pool SchedPool                                            |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| // The store block declares all lists that share this pool.        |
|                                                                    |
| // DATA type must be identical across all lists in a pool.         |
|                                                                    |
| store                                                              |
|                                                                    |
| high_pri: linklist\<TaskDescriptor, **depth** 64, kind singly\>;   |
|                                                                    |
| low_pri: linklist\<TaskDescriptor, **depth** 32, kind singly\>;    |
|                                                                    |
| **end** store                                                      |
|                                                                    |
| // Each list gets its own operation port set, prefixed by its name |
|                                                                    |
| op high_pri.insert_tail                                            |
|                                                                    |
| latency: 2;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** req_data: **in** TaskDescriptor;                          |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_handle: **out** UInt\<7\>;                           |
|                                                                    |
| **end** op high_pri.insert_tail                                    |
|                                                                    |
| op high_pri.delete_head                                            |
|                                                                    |
| latency: 2;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_data: **out** TaskDescriptor;                        |
|                                                                    |
| **end** op high_pri.delete_head                                    |
|                                                                    |
| op low_pri.insert_tail                                             |
|                                                                    |
| latency: 2;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** req_data: **in** TaskDescriptor;                          |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_handle: **out** UInt\<7\>;                           |
|                                                                    |
| **end** op low_pri.insert_tail                                     |
|                                                                    |
| op low_pri.delete_head                                             |
|                                                                    |
| latency: 2;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_data: **out** TaskDescriptor;                        |
|                                                                    |
| **end** op low_pri.delete_head                                     |
|                                                                    |
| **port** high_pri.**empty**: **out** Bool;                         |
|                                                                    |
| **port** high_pri.**full**: **out** Bool;                          |
|                                                                    |
| **port** low_pri.**empty**: **out** Bool;                          |
|                                                                    |
| **port** low_pri.**full**: **out** Bool;                           |
|                                                                    |
| **end** linklist_pool SchedPool                                    |
+--------------------------------------------------------------------+

> ◈ The shared pool\'s free list is unified --- an alloc from either list draws from the same physical slot space. The compiler prevents a slot assigned to high_pri from ever being accessed via low_pri addresses. Overflow of the shared pool is a compile-time warning if the total declared depth exceeds the physical RAM.

**12.7 Latency Contract and Pipelining**

The latency declaration on each op is a binding contract. The compiler selects an implementation that guarantees results arrive within the declared number of cycles. For operations that naturally require multiple RAM accesses, the compiler generates a pipelined controller sub-FSM.

+-------------------------------------------------------------------------------+
| *latency_contract.arch*                                                       |
|                                                                               |
| // Example: insert_after with latency 3 on a doubly-linked list.              |
|                                                                               |
| // The compiler generates a 3-stage controller pipeline:                      |
|                                                                               |
| // cycle 1: read current node\'s next pointer                                 |
|                                                                               |
| // cycle 2: write new node (data, next=old_next, prev=current)                |
|                                                                               |
| // cycle 3: patch old_next\'s prev pointer to new node                        |
|                                                                               |
| //                                                                            |
|                                                                               |
| // The designer only declares the interface and budget:                       |
|                                                                               |
| op insert_after                                                               |
|                                                                               |
| latency: 3;                                                                   |
|                                                                               |
| pipelined: true; // allow multiple outstanding requests                       |
|                                                                               |
| **port** req_valid: **in** Bool;                                              |
|                                                                               |
| **port** req_ready: **out** Bool;                                             |
|                                                                               |
| **port** req_handle: **in** UInt\<\$clog2(DEPTH)\>; // insert after this node |
|                                                                               |
| **port** req_data: **in** DATA;                                               |
|                                                                               |
| **port** resp_valid: **out** Bool;                                            |
|                                                                               |
| **port** resp_handle: **out** UInt\<\$clog2(DEPTH)\>; // handle of new node   |
|                                                                               |
| **end** op insert_after                                                       |
+-------------------------------------------------------------------------------+

> *⚑ pipelined: true tells the compiler the controller may accept a new request before the previous one completes. The compiler generates hazard detection for conflicting accesses to the same slot. pipelined: false (default) generates a simpler blocking controller.*

**12.8 Search with Predicate**

The search operation traverses the list and returns the first node whose payload satisfies a declared predicate. Worst-case latency is DEPTH cycles. The predicate is a pure combinational expression over the payload type.

+--------------------------------------------------------------------+
| *linklist_search.arch*                                             |
|                                                                    |
| op search                                                          |
|                                                                    |
| latency: DEPTH; // worst case --- full traversal                   |
|                                                                    |
| pipelined: false; // one search at a time                          |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| // Inline predicate: compiler generates comparator hardware        |
|                                                                    |
| **port** req_key: **in** UInt\<16\>; // search key                 |
|                                                                    |
| predicate: payload.task_id == req_key;                             |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_found: **out** Bool;                                 |
|                                                                    |
| **port** resp_handle: **out** UInt\<\$clog2(DEPTH)\>;              |
|                                                                    |
| **port** resp_data: **out** DATA;                                  |
|                                                                    |
| **end** op search                                                  |
+--------------------------------------------------------------------+

> ◈ The predicate expression is combinationally evaluated against the payload at each visited node. It may reference any field of the DATA struct and the req\_\* ports. The compiler verifies the predicate is pure --- no side effects, no register access.

**12.9 Generated Hardware Summary**

For a linklist of kind doubly, depth D, and payload type T, the compiler generates the following hardware automatically:

  ------------------------------------------------------------------------------------------------------------------
  **Generated Component**   **Size / Type**                    **Notes**
  ------------------------- ---------------------------------- -----------------------------------------------------
  **Data RAM**              simple_dual, D × \|T\| bits        Payload storage

  **Next pointer RAM**      simple_dual, D × \$clog2(D) bits   Forward links

  **Prev pointer RAM**      simple_dual, D × \$clog2(D) bits   Backward links (doubly only)

  **Free list FIFO**        depth D, width \$clog2(D)          Available slot tracking; pre-loaded 0..D-1 at reset

  **Head register**         reg, \$clog2(D) bits               Points to first node

  **Tail register**         reg, \$clog2(D) bits               Points to last node (if track tail: true)

  **Length counter**        reg, \$clog2(D+1) bits             Current occupancy (if track length: true)

  **Controller FSM**        1 per declared op                  Meets each op\'s declared latency contract

  **Search engine**         Linear scan FSM                    Generated only if op search declared
  ------------------------------------------------------------------------------------------------------------------

> *⚑ All generated components are named after the linklist instance in the output SystemVerilog, making the generated code fully auditable. For example: SchedPool_high_pri_data_ram, SchedPool_free_list, SchedPool_ctrl_insert_tail_fsm.*

**12.10 linklist vs fifo vs ram --- When to Use Which**

  ------------------------------------------------------------------------------------------------------
                           **fifo**         **ram**         **linklist**
  ------------------------ ---------------- --------------- --------------------------------------------
  **Order**                FIFO only        Random access   Any --- insert/delete anywhere

  **Access pattern**       Head/tail only   Any address     By handle (slot index)

  **Insertion**            Tail only        Any address     Head, tail, or after any node

  **Deletion**             Head only        Any address     Head, tail, or any node (doubly: O(1))

  **Physical impl**        Counters + RAM   BRAM / SRAM     RAM + free list + controller FSM

  **Latency**              1 cycle          1--2 cycles     1--N cycles per op (declared)

  **Dynamic allocation**   No               No              Yes --- alloc/free per node

  **Reordering**           No               N/A             Yes --- move nodes without copying payload
  ------------------------------------------------------------------------------------------------------

**13. First-Class Construct: cam**

A cam (Content-Addressable Memory) is a first-class construct that searches by content rather than by address. Instead of \'return the value at address X,\' a CAM answers \'is key X present, and if so at which index?\' The designer declares the CAM kind, depth, key and value types, match policy, and a lookup latency budget. The compiler generates the comparator array, priority encoder, replacement logic, and pipeline registers to meet the declared constraints.

**13.1 CAM Kinds**

  ----------------------------------------------------------------------------------------------------------------------------------------------------------
  **kind**          **Match Semantics**                             **Don\'t-Care Bits**   **Typical Applications**
  ----------------- ----------------------------------------------- ---------------------- -----------------------------------------------------------------
  **binary**        Exact match only --- every key bit must match   No                     TLBs, MAC address tables, exact-match classifiers

  **ternary**       Each key bit may be 0, 1, or X (wildcard)       Yes                    Longest-prefix-match routing, firewall ACLs, packet classifiers

  **associative**   Exact match; lookup returns value directly      No                     Cache tag arrays, register rename tables, forwarding tables
  ----------------------------------------------------------------------------------------------------------------------------------------------------------

**13.2 Lookup Latency and Implementation**

The latency parameter is the number of clock cycles from lookup request assertion to resp_valid. The compiler uses this budget to select a comparator implementation:

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Latency Budget**   **Comparator Strategy**                                               **Area**                    **Fmax**   **Best For**
  -------------------- --------------------------------------------------------------------- --------------------------- ---------- -----------------------------------------------------------------
  **1 cycle**          Fully parallel --- all entries compared simultaneously in one clock   High (O(depth × \|key\|))   Moderate   Small CAMs (≤ 64 entries), critical-path TLBs

  **2 cycles**         Two-stage pipelined comparator tree --- half in each stage            Medium                      High       Medium CAMs (64--256 entries)

  **N cycles**         N-stage pipeline --- compiler partitions comparator tree evenly       Lower                       Highest    Large TCAMs (256--4096 entries)

  **banked**           Entries divided into B banks; one bank checked per cycle (rotating)   Lowest                      High       Very large CAMs where not all entries need checking every cycle
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

> *⚑ The compiler always meets the declared latency exactly. If the requested latency is unachievable on the target technology given the key width and depth, it emits a compile-time error with the minimum achievable latency.*

**13.3 Binary CAM --- Exact Match**

+--------------------------------------------------------------------------------+
| *cam_binary.arch*                                                              |
|                                                                                |
| // TLB: 64-entry binary CAM mapping virtual page numbers to physical addresses |
|                                                                                |
| cam Tlb                                                                        |
|                                                                                |
| **param** DEPTH: **const** = 64;                                               |
|                                                                                |
| **param** VPN_W: **const** = 27; // virtual page number width (Sv39)           |
|                                                                                |
| **param** PPN_W: **const** = 44; // physical page number width                 |
|                                                                                |
| **port** clk: **in** Clock\<SysDomain\>;                                       |
|                                                                                |
| **port** rst: **in** Reset\<Sync\>;                                            |
|                                                                                |
| kind binary;                                                                   |
|                                                                                |
| **match**: first_match; // first_match \| all_matches                          |
|                                                                                |
| key_type: UInt\<VPN_W\>;                                                       |
|                                                                                |
| value_type: TlbEntry; // struct: PPN, flags, ASID, \...                        |
|                                                                                |
| // Replacement policy when CAM is full and a new entry is inserted             |
|                                                                                |
| replace: lru; // lru \| fifo \| random \| none                                 |
|                                                                                |
| op lookup                                                                      |
|                                                                                |
| latency: 1;                                                                    |
|                                                                                |
| **port** req_valid: **in** Bool;                                               |
|                                                                                |
| **port** req_key: **in** UInt\<VPN_W\>;                                        |
|                                                                                |
| **port** resp_valid: **out** Bool;                                             |
|                                                                                |
| **port** resp_hit: **out** Bool;                                               |
|                                                                                |
| **port** resp_index: **out** UInt\<\$clog2(DEPTH)\>;                           |
|                                                                                |
| **port** resp_value: **out** TlbEntry;                                         |
|                                                                                |
| **end** op lookup                                                              |
|                                                                                |
| op insert                                                                      |
|                                                                                |
| latency: 2;                                                                    |
|                                                                                |
| **port** req_valid: **in** Bool;                                               |
|                                                                                |
| **port** req_ready: **out** Bool;                                              |
|                                                                                |
| **port** req_key: **in** UInt\<VPN_W\>;                                        |
|                                                                                |
| **port** req_value: **in** TlbEntry;                                           |
|                                                                                |
| **port** resp_valid: **out** Bool;                                             |
|                                                                                |
| **port** resp_index: **out** UInt\<\$clog2(DEPTH)\>; // slot used              |
|                                                                                |
| **end** op insert                                                              |
|                                                                                |
| op invalidate                                                                  |
|                                                                                |
| latency: 1;                                                                    |
|                                                                                |
| **port** req_valid: **in** Bool;                                               |
|                                                                                |
| **port** req_index: **in** UInt\<\$clog2(DEPTH)\>;                             |
|                                                                                |
| **end** op invalidate                                                          |
|                                                                                |
| op invalidate_all                                                              |
|                                                                                |
| latency: 1;                                                                    |
|                                                                                |
| **port** req_valid: **in** Bool;                                               |
|                                                                                |
| **port** resp_valid: **out** Bool;                                             |
|                                                                                |
| **end** op invalidate_all                                                      |
|                                                                                |
| **port** **full**: **out** Bool;                                               |
|                                                                                |
| **port** **empty**: **out** Bool;                                              |
|                                                                                |
| **end** cam Tlb                                                                |
+--------------------------------------------------------------------------------+

**13.4 Ternary CAM --- Wildcard Match**

A TCAM stores a key and a mask per entry. During lookup, a key bit matches if the stored mask bit is X (don\'t-care) or if the stored key bit equals the lookup key bit. Entries are evaluated in priority order; the first matching entry wins unless all_matches is specified.

+-------------------------------------------------------------------------------+
| *cam_ternary.arch*                                                            |
|                                                                               |
| // IPv4 longest-prefix-match routing table                                    |
|                                                                               |
| cam RoutingTable                                                              |
|                                                                               |
| **param** DEPTH: **const** = 1024;                                            |
|                                                                               |
| **param** PREFIX_W: **const** = 32; // IPv4 address width                     |
|                                                                               |
| **port** clk: **in** Clock\<SysDomain\>;                                      |
|                                                                               |
| **port** rst: **in** Reset\<Sync\>;                                           |
|                                                                               |
| kind ternary;                                                                 |
|                                                                               |
| **match**: first_match; // priority-ordered; first hit wins (longest prefix)  |
|                                                                               |
| // TCAM key: the prefix bits to match                                         |
|                                                                               |
| // TCAM mask: X on don\'t-care bits (host portion of the prefix)              |
|                                                                               |
| key_type: UInt\<PREFIX_W\>;                                                   |
|                                                                               |
| mask_type: UInt\<PREFIX_W\>; // 1 = must match, 0 = don\'t-care               |
|                                                                               |
| value_type: RouteEntry; // struct: next_hop, port, metric, \...               |
|                                                                               |
| // Higher priority index = higher priority in first_match resolution          |
|                                                                               |
| priority: index_ascending; // index_ascending \| index_descending \| explicit |
|                                                                               |
| replace: none; // routing table managed explicitly by control plane           |
|                                                                               |
| op lookup                                                                     |
|                                                                               |
| latency: 2; // 1024 entries --- 2-stage pipelined comparator tree             |
|                                                                               |
| **port** req_valid: **in** Bool;                                              |
|                                                                               |
| **port** req_key: **in** UInt\<PREFIX_W\>;                                    |
|                                                                               |
| **port** resp_valid: **out** Bool;                                            |
|                                                                               |
| **port** resp_hit: **out** Bool;                                              |
|                                                                               |
| **port** resp_index: **out** UInt\<\$clog2(DEPTH)\>;                          |
|                                                                               |
| **port** resp_value: **out** RouteEntry;                                      |
|                                                                               |
| **end** op lookup                                                             |
|                                                                               |
| // TCAM insert: provide key, mask, value, and priority slot                   |
|                                                                               |
| op insert                                                                     |
|                                                                               |
| latency: 2;                                                                   |
|                                                                               |
| **port** req_valid: **in** Bool;                                              |
|                                                                               |
| **port** req_ready: **out** Bool;                                             |
|                                                                               |
| **port** req_key: **in** UInt\<PREFIX_W\>;                                    |
|                                                                               |
| **port** req_mask: **in** UInt\<PREFIX_W\>;                                   |
|                                                                               |
| **port** req_value: **in** RouteEntry;                                        |
|                                                                               |
| **port** req_index: **in** UInt\<\$clog2(DEPTH)\>; // explicit priority slot  |
|                                                                               |
| **port** resp_valid: **out** Bool;                                            |
|                                                                               |
| **end** op insert                                                             |
|                                                                               |
| op delete                                                                     |
|                                                                               |
| latency: 1;                                                                   |
|                                                                               |
| **port** req_valid: **in** Bool;                                              |
|                                                                               |
| **port** req_index: **in** UInt\<\$clog2(DEPTH)\>;                            |
|                                                                               |
| **end** op delete                                                             |
|                                                                               |
| // TCAM-specific: read back the stored key and mask at a given index          |
|                                                                               |
| op read_entry                                                                 |
|                                                                               |
| latency: 1;                                                                   |
|                                                                               |
| **port** req_valid: **in** Bool;                                              |
|                                                                               |
| **port** req_index: **in** UInt\<\$clog2(DEPTH)\>;                            |
|                                                                               |
| **port** resp_valid: **out** Bool;                                            |
|                                                                               |
| **port** resp_key: **out** UInt\<PREFIX_W\>;                                  |
|                                                                               |
| **port** resp_mask: **out** UInt\<PREFIX_W\>;                                 |
|                                                                               |
| **port** resp_value: **out** RouteEntry;                                      |
|                                                                               |
| **end** op read_entry                                                         |
|                                                                               |
| **port** occupancy: **out** UInt\<\$clog2(DEPTH+1)\>;                         |
|                                                                               |
| **end** cam RoutingTable                                                      |
+-------------------------------------------------------------------------------+

**13.5 Associative CAM --- Direct Value Return**

An associative CAM combines lookup and value retrieval in a single operation. It is the most common form in processor micro-architecture. The lookup key and its associated value are stored together; a hit returns the value directly without a separate RAM read.

+-------------------------------------------------------------------------------------+
| *cam_associative.arch*                                                              |
|                                                                                     |
| // Register rename table: maps architectural register numbers to physical registers |
|                                                                                     |
| cam RenameTable                                                                     |
|                                                                                     |
| **param** DEPTH: **const** = 128; // number of in-flight physical registers         |
|                                                                                     |
| **param** ARCH_W: **const** = 5; // architectural register number width (32 regs)   |
|                                                                                     |
| **param** PHYS_W: **const** = 7; // physical register number width (128 regs)       |
|                                                                                     |
| **port** clk: **in** Clock\<SysDomain\>;                                            |
|                                                                                     |
| **port** rst: **in** Reset\<Sync\>;                                                 |
|                                                                                     |
| kind associative;                                                                   |
|                                                                                     |
| **match**: first_match;                                                             |
|                                                                                     |
| key_type: UInt\<ARCH_W\>;                                                           |
|                                                                                     |
| value_type: UInt\<PHYS_W\>;                                                         |
|                                                                                     |
| replace: none; // rename table managed explicitly by rename stage                   |
|                                                                                     |
| // Two simultaneous lookups for rs1 and rs2 each cycle                              |
|                                                                                     |
| op lookup                                                                           |
|                                                                                     |
| latency: 1;                                                                         |
|                                                                                     |
| ports\[2\] req                                                                      |
|                                                                                     |
| **valid**: **in** Bool;                                                             |
|                                                                                     |
| key: **in** UInt\<ARCH_W\>;                                                         |
|                                                                                     |
| **end** ports                                                                       |
|                                                                                     |
| ports\[2\] resp                                                                     |
|                                                                                     |
| **valid**: **out** Bool;                                                            |
|                                                                                     |
| hit: **out** Bool;                                                                  |
|                                                                                     |
| value: **out** UInt\<PHYS_W\>;                                                      |
|                                                                                     |
| **end** ports                                                                       |
|                                                                                     |
| **end** op lookup                                                                   |
|                                                                                     |
| op insert                                                                           |
|                                                                                     |
| latency: 1;                                                                         |
|                                                                                     |
| **port** req_valid: **in** Bool;                                                    |
|                                                                                     |
| **port** req_ready: **out** Bool;                                                   |
|                                                                                     |
| **port** req_key: **in** UInt\<ARCH_W\>;                                            |
|                                                                                     |
| **port** req_value: **in** UInt\<PHYS_W\>;                                          |
|                                                                                     |
| **end** op insert                                                                   |
|                                                                                     |
| op delete_by_key                                                                    |
|                                                                                     |
| latency: 1;                                                                         |
|                                                                                     |
| **port** req_valid: **in** Bool;                                                    |
|                                                                                     |
| **port** req_key: **in** UInt\<ARCH_W\>;                                            |
|                                                                                     |
| **end** op delete_by_key                                                            |
|                                                                                     |
| op delete_by_index                                                                  |
|                                                                                     |
| latency: 1;                                                                         |
|                                                                                     |
| **port** req_valid: **in** Bool;                                                    |
|                                                                                     |
| **port** req_index: **in** UInt\<\$clog2(DEPTH)\>;                                  |
|                                                                                     |
| **end** op delete_by_index                                                          |
|                                                                                     |
| **end** cam RenameTable                                                             |
+-------------------------------------------------------------------------------------+

> ◈ Multi-port lookup (ports\[2\]) is a first-class feature of the cam op block --- the same ports\[\] syntax used in arbiter and regfile. The compiler replicates the comparator array once per lookup port. For a 2-port lookup with 128 entries and a 5-bit key, this is 2 × 128 comparators, all evaluated in parallel within one cycle.

**13.6 Multi-Table Shared Physical CAM**

Multiple logical CAM tables can share one physical CAM array --- the same multi-variable mapping pattern used in ram and linklist_pool. The compiler assigns non-overlapping index ranges to each logical table and generates per-table valid-bit arrays so lookups never cross table boundaries.

+--------------------------------------------------------------------+
| *cam_bank.arch*                                                    |
|                                                                    |
| // Shared TCAM for instruction TLB and data TLB.                   |
|                                                                    |
| // Physical CAM: 128 entries total.                                |
|                                                                    |
| // Compiler assigns:                                               |
|                                                                    |
| // itlb → indices 0 .. 63                                          |
|                                                                    |
| // dtlb → indices 64 .. 127                                        |
|                                                                    |
| cam_bank TlbBank                                                   |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| kind binary;                                                       |
|                                                                    |
| **match**: first_match;                                            |
|                                                                    |
| // Both tables share the same key and value types.                 |
|                                                                    |
| // The compiler generates a single comparator array of depth 128.  |
|                                                                    |
| key_type: UInt\<27\>; // virtual page number                       |
|                                                                    |
| value_type: TlbEntry;                                              |
|                                                                    |
| store                                                              |
|                                                                    |
| itlb: cam\<**depth** 64, replace lru\>;                            |
|                                                                    |
| dtlb: cam\<**depth** 64, replace lru\>;                            |
|                                                                    |
| **end** store                                                      |
|                                                                    |
| op itlb.lookup                                                     |
|                                                                    |
| latency: 1;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_key: **in** UInt\<27\>;                               |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_hit: **out** Bool;                                   |
|                                                                    |
| **port** resp_value: **out** TlbEntry;                             |
|                                                                    |
| **end** op itlb.lookup                                             |
|                                                                    |
| op itlb.insert                                                     |
|                                                                    |
| latency: 2;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** req_key: **in** UInt\<27\>;                               |
|                                                                    |
| **port** req_value: **in** TlbEntry;                               |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **end** op itlb.insert                                             |
|                                                                    |
| op dtlb.lookup                                                     |
|                                                                    |
| latency: 1;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_key: **in** UInt\<27\>;                               |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_hit: **out** Bool;                                   |
|                                                                    |
| **port** resp_value: **out** TlbEntry;                             |
|                                                                    |
| **end** op dtlb.lookup                                             |
|                                                                    |
| op dtlb.insert                                                     |
|                                                                    |
| latency: 2;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** req_key: **in** UInt\<27\>;                               |
|                                                                    |
| **port** req_value: **in** TlbEntry;                               |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **end** op dtlb.insert                                             |
|                                                                    |
| op itlb.invalidate_all                                             |
|                                                                    |
| latency: 1;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **end** op itlb.invalidate_all                                     |
|                                                                    |
| op dtlb.invalidate_all                                             |
|                                                                    |
| latency: 1;                                                        |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **end** op dtlb.invalidate_all                                     |
|                                                                    |
| **end** cam_bank TlbBank                                           |
+--------------------------------------------------------------------+

> *⚑ A lookup on itlb only evaluates entries in the itlb index range (0--63). The compiler generates a range-masked hit-vector so dtlb entries never produce a false hit on an itlb lookup, and vice versa. This masking is zero-overhead --- it is absorbed into the valid-bit array of the comparator.*

**13.7 Match Policies**

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Policy**        **Returns**                                                              **Generated Hardware**                                           **Typical Use**
  ----------------- ------------------------------------------------------------------------ ---------------------------------------------------------------- ---------------------------------------------------
  **first_match**   Index and value of lowest-priority matching entry                        Priority encoder over hit vector                                 TLBs, routing (longest prefix = highest priority)

  **all_matches**   A hit_vec: Vec\<Bool, DEPTH\> --- one bit per entry                      Hit vector register; no priority encoder                         Firewall logging, multicast replication

  **priority**      Index and value of highest-priority matching entry (explicit ordering)   Priority encoder, compiler generates order from entry metadata   ACL rules with explicit priority levels
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**13.8 Replacement Policies**

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Policy**   **Mechanism**                                         **Generated Hardware**                                   **Best For**
  ------------ ----------------------------------------------------- -------------------------------------------------------- -----------------------------------------
  **lru**      Evict least-recently-used entry on insert when full   LRU tracking list (uses linklist\<doubly\> internally)   TLBs, cache tag arrays

  **fifo**     Evict oldest-inserted entry                           Age counter per entry                                    Simple classifiers, approximate LRU

  **random**   Evict a pseudo-random entry                           LFSR                                                     Area-constrained designs, L1 caches

  **plru**     Pseudo-LRU tree approximation                         Binary tree of LRU bits                                  Large TLBs where true LRU is too costly

  **none**     No auto-eviction; insert fails when full              Valid-bit check only                                     Software-managed tables (routing, ACLs)
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------

> ◈ When replace: lru is selected, the compiler automatically instantiates a linklist\<doubly\> construct internally to manage the LRU ordering --- this is a clean example of first-class constructs composing with each other.

**13.9 Generated Hardware Summary**

For a cam of kind binary, depth D, key width K, value width V, and latency L, the compiler generates:

  -------------------------------------------------------------------------------------------------------------------------------
  **Generated Component**             **Size**                      **Notes**
  ----------------------------------- ----------------------------- -------------------------------------------------------------
  **Key storage RAM**                 D × K bits                    One key per entry; simple_dual for concurrent lookup+insert

  **Value storage RAM**               D × V bits                    Associated value; separate from key array for timing

  **Valid bit array**                 D × 1 bit                     Marks occupied entries; part of every comparator gate

  **Mask array (TCAM only)**          D × K bits                    Per-bit don\'t-care mask; absent for binary/associative

  **Comparator array**                D × K gates per lookup port   All entries compared in parallel within latency budget

  **Pipeline registers**              (L-1) stages                  Cut into comparator tree to meet latency; zero if L=1

  **Priority encoder**                \$clog2(D) bits               Reduces hit vector to index; one per match policy output

  **LRU linklist (if replace:lru)**   doubly-linked, depth D        Compiler-instantiated; tracks access order for eviction

  **Replacement controller FSM**      1 FSM                         Selects eviction candidate and writes new entry on insert
  -------------------------------------------------------------------------------------------------------------------------------

**13.10 cam vs ram vs linklist --- Choosing the Right Construct**

  ----------------------------------------------------------------------------------------------------------------------------------
                             **ram**                         **linklist**                       **cam**
  -------------------------- ------------------------------- ---------------------------------- ------------------------------------
  **Access model**           By address                      By handle (slot index)             By content (key match)

  **Search**                 O(N) --- must scan externally   O(N) --- traverse with predicate   O(1) --- parallel comparators

  **Key storage**            No --- caller manages           No                                 Yes --- stored alongside value

  **Dynamic alloc**          No                              Yes --- alloc/free                 Yes --- insert/evict

  **Don\'t-care matching**   No                              No                                 Yes --- TCAM only

  **Multi-port lookup**      Via multiple port decls         No                                 Yes --- ports\[N\] on op lookup

  **LRU tracking**           Manual                          Native --- kind doubly             Compiler-generated (replace: lru)

  **FPGA primitive**         BRAM / URAM                     BRAM + LUT logic                   LUT array (no dedicated primitive)

  **Area scaling**           O(depth)                        O(depth)                           O(depth × key_width) per port
  ----------------------------------------------------------------------------------------------------------------------------------

**14. First-Class Construct: crossbar**

A crossbar is a fully-connected N×M switch fabric. Any of the N input ports can be routed to any of the M output ports simultaneously, subject to the constraint that each output can accept at most one input per cycle. This is fundamentally different from an arbiter --- an arbiter selects among N requesters for one resource; a crossbar routes N streams to M destinations concurrently.

The compiler generates the per-output arbiters, the routing mux array, the input-side backpressure logic, and the optional virtual-channel buffers. The designer declares topology, data type, routing function, and arbitration policy.

**14.1 Declaration**

+------------------------------------------------------------------------------+
| *crossbar.arch*                                                              |
|                                                                              |
| // 4-input × 4-output crossbar for a 2D mesh NoC router                      |
|                                                                              |
| crossbar NoC4x4                                                              |
|                                                                              |
| **param** N: **const** = 4; // number of input ports                         |
|                                                                              |
| **param** M: **const** = 4; // number of output ports                        |
|                                                                              |
| **param** FLIT_W: **const** = 64; // data width per port                     |
|                                                                              |
| **port** clk: **in** Clock\<SysDomain\>;                                     |
|                                                                              |
| **port** rst: **in** Reset\<Sync\>;                                          |
|                                                                              |
| // Routing: input port declares destination as a tag in the data stream      |
|                                                                              |
| // The crossbar extracts the tag and steers the flit accordingly             |
|                                                                              |
| routing: by_tag;                                                             |
|                                                                              |
| tag_field: UInt\<\$clog2(M)\>; // high bits of each flit carry destination   |
|                                                                              |
| tag_offset: FLIT_W - \$clog2(M); // bit position of tag within flit          |
|                                                                              |
| // Per-output arbitration policy when multiple inputs target the same output |
|                                                                              |
| **policy**: round_robin;                                                     |
|                                                                              |
| // Optional: virtual channels per input port to avoid head-of-line blocking  |
|                                                                              |
| virtual_channels: 2;                                                         |
|                                                                              |
| ports\[N\] in_port                                                           |
|                                                                              |
| **valid**: **in** Bool;                                                      |
|                                                                              |
| **ready**: **out** Bool;                                                     |
|                                                                              |
| data: **in** UInt\<FLIT_W\>;                                                 |
|                                                                              |
| **end** ports                                                                |
|                                                                              |
| ports\[M\] out_port                                                          |
|                                                                              |
| **valid**: **out** Bool;                                                     |
|                                                                              |
| **ready**: **in** Bool;                                                      |
|                                                                              |
| data: **out** UInt\<FLIT_W\>;                                                |
|                                                                              |
| **end** ports                                                                |
|                                                                              |
| **end** crossbar NoC4x4                                                      |
+------------------------------------------------------------------------------+

**14.2 Routing Modes**

  -----------------------------------------------------------------------------------------------------------------------------
  **mode**        **How Destination is Determined**                            **Generated Hardware**
  --------------- ------------------------------------------------------------ ------------------------------------------------
  **by_tag**      Destination index encoded in a bit field of each flit        Tag extractor + mux per output port

  **by_table**    External routing table (cam) maps source+tag → output port   Tag extractor + cam lookup + mux

  **by_port**     Explicit per-input destination signal alongside the data     Dedicated dest port per input + mux

  **broadcast**   One input drives all outputs simultaneously                  Fanout + per-output backpressure AND reduction
  -----------------------------------------------------------------------------------------------------------------------------

**14.3 Structural Variants**

+-------------------------------------------------------------------------+
| *crossbar_variant.arch*                                                 |
|                                                                         |
| // Non-square: 8 inputs, 2 outputs --- concentrator topology            |
|                                                                         |
| crossbar MemConcentrator                                                |
|                                                                         |
| **param** N: **const** = 8;                                             |
|                                                                         |
| **param** M: **const** = 2;                                             |
|                                                                         |
| **param** W: **const** = 512;                                           |
|                                                                         |
| **port** clk: **in** Clock\<SysDomain\>;                                |
|                                                                         |
| **port** rst: **in** Reset\<Sync\>;                                     |
|                                                                         |
| routing: by_port;                                                       |
|                                                                         |
| **policy**: weighted\<8\'b11110000\>; // first 4 inputs weighted higher |
|                                                                         |
| ports\[N\] in_port                                                      |
|                                                                         |
| **valid**: **in** Bool;                                                 |
|                                                                         |
| **ready**: **out** Bool;                                                |
|                                                                         |
| data: **in** UInt\<W\>;                                                 |
|                                                                         |
| dest: **in** UInt\<\$clog2(M)\>; // explicit destination (by_port mode) |
|                                                                         |
| **end** ports                                                           |
|                                                                         |
| ports\[M\] out_port                                                     |
|                                                                         |
| **valid**: **out** Bool;                                                |
|                                                                         |
| **ready**: **in** Bool;                                                 |
|                                                                         |
| data: **out** UInt\<W\>;                                                |
|                                                                         |
| **end** ports                                                           |
|                                                                         |
| **end** crossbar MemConcentrator                                        |
+-------------------------------------------------------------------------+

> ◈ The compiler generates one arbiter instance (using the declared policy) per output port, plus an N-input mux per output. For N=8, M=2, this is 2 arbiters and 2 mux arrays --- all correctly wired and backpressure-connected.

**15. First-Class Construct: scoreboard**

A scoreboard tracks in-flight operations and their data dependencies. It decides whether a new operation can be issued (all source operands are ready) or must be stalled (a previous operation will write a needed operand). It is the central hazard-management unit of any pipelined or out-of-order design.

The designer declares the number of tracked entries, the number of simultaneous issue and writeback ports, and which hazard classes to detect. The compiler generates the dependency matrix, the ready-bit array, the issue gate logic, and the stall propagation signals.

**15.1 Hazard Classes**

  -------------------------------------------------------------------------------------------------------------------------
  **Hazard**       **Full Name**       **Condition**                                   **Default Action**
  ---------------- ------------------- ----------------------------------------------- ------------------------------------
  **RAW**          Read-After-Write    Consumer issued before producer writes back     Stall consumer until writeback

  **WAW**          Write-After-Write   Two writers to the same destination in flight   Stall second writer

  **WAR**          Write-After-Read    Writer issued before prior reader completes     Stall writer (only matters in OOO)

  **Structural**   Resource conflict   Two ops need the same functional unit           Stall later op
  -------------------------------------------------------------------------------------------------------------------------

**15.2 Declaration**

+---------------------------------------------------------------------------+
| *scoreboard.arch*                                                         |
|                                                                           |
| // Scoreboard for a 4-wide issue, 2-wide writeback in-order pipeline      |
|                                                                           |
| scoreboard IssueBoard                                                     |
|                                                                           |
| **param** ENTRIES: **const** = 32; // max in-flight operations            |
|                                                                           |
| **param** REG_W: **const** = 5; // register address width (32 arch regs)  |
|                                                                           |
| **param** ISSUE_W: **const** = 4; // simultaneous issue ports             |
|                                                                           |
| **param** WB_W: **const** = 2; // simultaneous writeback ports            |
|                                                                           |
| **port** clk: **in** Clock\<SysDomain\>;                                  |
|                                                                           |
| **port** rst: **in** Reset\<Sync\>;                                       |
|                                                                           |
| hazards: \[raw, waw\]; // detect RAW and WAW; WAR not needed for in-order |
|                                                                           |
| // Issue ports --- one per simultaneously-issued operation                |
|                                                                           |
| ports\[ISSUE_W\] issue                                                    |
|                                                                           |
| **valid**: **in** Bool;                                                   |
|                                                                           |
| **ready**: **out** Bool; // scoreboard grants or stalls                   |
|                                                                           |
| rs1_addr: **in** UInt\<REG_W\>;                                           |
|                                                                           |
| rs2_addr: **in** UInt\<REG_W\>;                                           |
|                                                                           |
| rd_addr: **in** UInt\<REG_W\>;                                            |
|                                                                           |
| rd_valid: **in** Bool; // does this op write a destination register?      |
|                                                                           |
| token: **out** UInt\<\$clog2(ENTRIES)\>; // scoreboard entry handle       |
|                                                                           |
| **end** ports                                                             |
|                                                                           |
| // Writeback ports --- mark an entry complete and release its destination |
|                                                                           |
| ports\[WB_W\] writeback                                                   |
|                                                                           |
| **valid**: **in** Bool;                                                   |
|                                                                           |
| token: **in** UInt\<\$clog2(ENTRIES)\>;                                   |
|                                                                           |
| **end** ports                                                             |
|                                                                           |
| // Flush: invalidate all entries (branch mispredict, exception)           |
|                                                                           |
| **port** flush_valid: **in** Bool;                                        |
|                                                                           |
| // Status                                                                 |
|                                                                           |
| **port** **full**: **out** Bool;                                          |
|                                                                           |
| **port** **empty**: **out** Bool;                                         |
|                                                                           |
| **end** scoreboard IssueBoard                                             |
+---------------------------------------------------------------------------+

**15.3 Scoreboard with Structural Hazards**

+-----------------------------------------------------------------------------+
| *scoreboard_ooo.arch*                                                       |
|                                                                             |
| // OOO scoreboard detecting all hazard types plus functional unit conflicts |
|                                                                             |
| scoreboard OooBoard                                                         |
|                                                                             |
| **param** ENTRIES: **const** = 64;                                          |
|                                                                             |
| **param** REG_W: **const** = 7; // 128 physical registers (post-rename)     |
|                                                                             |
| **param** ISSUE_W: **const** = 4;                                           |
|                                                                             |
| **param** WB_W: **const** = 4;                                              |
|                                                                             |
| **port** clk: **in** Clock\<SysDomain\>;                                    |
|                                                                             |
| **port** rst: **in** Reset\<Sync\>;                                         |
|                                                                             |
| hazards: \[raw, waw, war, structural\];                                     |
|                                                                             |
| // Functional unit availability --- structural hazard inputs                |
|                                                                             |
| **port** alu_free: **in** Bool;                                             |
|                                                                             |
| **port** mul_free: **in** Bool;                                             |
|                                                                             |
| **port** lsu_free: **in** Bool;                                             |
|                                                                             |
| **port** fpu_free: **in** Bool;                                             |
|                                                                             |
| ports\[ISSUE_W\] issue                                                      |
|                                                                             |
| **valid**: **in** Bool;                                                     |
|                                                                             |
| **ready**: **out** Bool;                                                    |
|                                                                             |
| rs1_addr: **in** UInt\<REG_W\>;                                             |
|                                                                             |
| rs2_addr: **in** UInt\<REG_W\>;                                             |
|                                                                             |
| rs3_addr: **in** UInt\<REG_W\>; // FMA third source                         |
|                                                                             |
| rd_addr: **in** UInt\<REG_W\>;                                              |
|                                                                             |
| rd_valid: **in** Bool;                                                      |
|                                                                             |
| fu_type: **in** FuType; // which functional unit needed                     |
|                                                                             |
| latency: **in** UInt\<4\>; // declared execution latency                    |
|                                                                             |
| token: **out** UInt\<\$clog2(ENTRIES)\>;                                    |
|                                                                             |
| **end** ports                                                               |
|                                                                             |
| ports\[WB_W\] writeback                                                     |
|                                                                             |
| **valid**: **in** Bool;                                                     |
|                                                                             |
| token: **in** UInt\<\$clog2(ENTRIES)\>;                                     |
|                                                                             |
| **end** ports                                                               |
|                                                                             |
| **port** flush_valid: **in** Bool;                                          |
|                                                                             |
| **port** **full**: **out** Bool;                                            |
|                                                                             |
| **end** scoreboard OooBoard                                                 |
+-----------------------------------------------------------------------------+

> *⚑ When latency: is provided per issue, the scoreboard automatically generates a countdown timer per entry and self-completes the entry when the timer expires --- eliminating the need for explicit writeback on fixed-latency operations.*

**16. First-Class Construct: reorder_buf**

A reorder buffer (ROB) is a circular buffer that allows operations to complete out-of-order while retiring their results strictly in-order. Entries are allocated at the tail in program order on issue, can be marked complete in any order as operations finish, and are committed from the head in order. The ROB is the commit stage of every out-of-order processor.

The Arch reorder_buf construct captures this circular, decoupled head/tail structure which cannot be expressed as a plain fifo. The designer declares the number of entries, the result payload type, the number of allocate and complete ports, and the commit policy. The compiler generates the circular index logic, the status array, the head/tail advancement logic, and the in-order commit gating.

**16.1 Declaration**

+------------------------------------------------------------------------------------+
| *rob.arch*                                                                         |
|                                                                                    |
| // ROB for a 4-wide issue, 2-wide commit OOO processor                             |
|                                                                                    |
| reorder_buf Rob                                                                    |
|                                                                                    |
| **param** DEPTH: **const** = 128; // max in-flight instructions                    |
|                                                                                    |
| **param** ALLOC_W: **const** = 4; // simultaneous allocation ports (= issue width) |
|                                                                                    |
| **param** COMMIT_W: **const** = 2; // simultaneous commit ports                    |
|                                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                                           |
|                                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                                |
|                                                                                    |
| // Payload stored per ROB entry --- whatever the commit stage needs                |
|                                                                                    |
| entry_type: RobEntry;                                                              |
|                                                                                    |
| // Allocation: reserve ROB slots for newly-issued instructions                     |
|                                                                                    |
| ports\[ALLOC_W\] alloc                                                             |
|                                                                                    |
| **valid**: **in** Bool;                                                            |
|                                                                                    |
| **ready**: **out** Bool;                                                           |
|                                                                                    |
| data: **in** RobEntry;                                                             |
|                                                                                    |
| token: **out** UInt\<\$clog2(DEPTH)\>; // ROB index --- carried through pipeline   |
|                                                                                    |
| **end** ports                                                                      |
|                                                                                    |
| // Complete: mark a slot done and write back its result                            |
|                                                                                    |
| // May arrive in any order                                                         |
|                                                                                    |
| ports\[ALLOC_W\] complete                                                          |
|                                                                                    |
| **valid**: **in** Bool;                                                            |
|                                                                                    |
| token: **in** UInt\<\$clog2(DEPTH)\>;                                              |
|                                                                                    |
| result: **in** RobEntry; // updated entry (result, exception flags, etc.)          |
|                                                                                    |
| **end** ports                                                                      |
|                                                                                    |
| // Commit: head entries that are complete retire in-order                          |
|                                                                                    |
| ports\[COMMIT_W\] commit                                                           |
|                                                                                    |
| **valid**: **out** Bool;                                                           |
|                                                                                    |
| data: **out** RobEntry;                                                            |
|                                                                                    |
| token: **out** UInt\<\$clog2(DEPTH)\>;                                             |
|                                                                                    |
| ack: **in** Bool; // downstream (regfile, store buffer) accepts commit             |
|                                                                                    |
| **end** ports                                                                      |
|                                                                                    |
| // Flush: squash all entries from a given token to the tail                        |
|                                                                                    |
| // Used on branch mispredict or exception                                          |
|                                                                                    |
| **port** flush_valid: **in** Bool;                                                 |
|                                                                                    |
| **port** flush_token: **in** UInt\<\$clog2(DEPTH)\>;                               |
|                                                                                    |
| // Status                                                                          |
|                                                                                    |
| **port** **full**: **out** Bool;                                                   |
|                                                                                    |
| **port** **empty**: **out** Bool;                                                  |
|                                                                                    |
| **port** occupancy: **out** UInt\<\$clog2(DEPTH+1)\>;                              |
|                                                                                    |
| **end** reorder_buf Rob                                                            |
+------------------------------------------------------------------------------------+

**16.2 Commit Policies**

  ----------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Policy**           **Behaviour**                                                                    **Use When**
  -------------------- -------------------------------------------------------------------------------- ----------------------------------------------------------
  **in_order**         Commit head entry only when it is marked complete; stall if head not done        Most OOO processors --- precise exception model required

  **speculative**      Commit head even if not complete; mark as speculative; confirm or squash later   Hardware transactional memory, speculative stores

  **exception_stop**   Stop committing at first entry with exception flag set; flush tail               Precise exception delivery --- default for RISC-V, x86
  ----------------------------------------------------------------------------------------------------------------------------------------------------------------

**16.3 ROB Entry Type**

+-----------------------------------------------------------------------+
| *rob_entry.arch*                                                      |
|                                                                       |
| // Typical ROB entry for an integer OOO pipeline                      |
|                                                                       |
| **struct** RobEntry                                                   |
|                                                                       |
| pc: UInt\<64\>, // program counter of the instruction                 |
|                                                                       |
| rd_addr: UInt\<7\>, // physical destination register                  |
|                                                                       |
| rd_valid: Bool, // does this inst write a register?                   |
|                                                                       |
| result: UInt\<64\>, // computed result value                          |
|                                                                       |
| complete: Bool, // has execution finished?                            |
|                                                                       |
| exception: Bool, // did this inst raise an exception?                 |
|                                                                       |
| exc_cause: UInt\<6\>, // exception code                               |
|                                                                       |
| store_ptr: UInt\<5\>, // index into store buffer, if applicable       |
|                                                                       |
| is_store: Bool,                                                       |
|                                                                       |
| is_branch: Bool,                                                      |
|                                                                       |
| mispredict: Bool, // branch was mispredicted                          |
|                                                                       |
| target_pc: UInt\<64\>, // correct PC after branch resolution          |
|                                                                       |
| **end** **struct** RobEntry                                           |
|                                                                       |
| // The reorder_buf compiler only stores and retrieves this struct.    |
|                                                                       |
| // It never inspects individual fields --- complete and exception are |
|                                                                       |
| // the only fields the ROB controller itself reads.                   |
+-----------------------------------------------------------------------+

> ◈ The ROB controller reads only the complete and exception fields internally. All other fields are stored opaquely and returned verbatim at commit time. This keeps the reorder_buf construct general --- it works for any pipeline by changing the entry_type.

**16.4 Connecting scoreboard and reorder_buf Together**

+---------------------------------------------------------------------+
| *ooo_frontend.arch*                                                 |
|                                                                     |
| // Typical OOO front-end: scoreboard gates issue; ROB orders commit |
|                                                                     |
| **module** OooFrontEnd                                              |
|                                                                     |
| **port** clk: **in** Clock\<SysDomain\>;                            |
|                                                                     |
| **port** rst: **in** Reset\<Sync\>;                                 |
|                                                                     |
| // \... other ports \...                                            |
|                                                                     |
| **inst** sb: OooBoard // scoreboard from §15                        |
|                                                                     |
| **clk \<- clk;                                            |
|                                                                     |
| **rst \<- rst;                                            |
|                                                                     |
| // issue port 0 connected to decode stage output                    |
|                                                                     |
| **issue\[0\].**valid** \<- dec_valid;                     |
|                                                                     |
| **issue\[0\].rs1_addr \<- dec_rs1;                        |
|                                                                     |
| **issue\[0\].rs2_addr \<- dec_rs2;                        |
|                                                                     |
| **issue\[0\].rd_addr \<- dec_rd;                          |
|                                                                     |
| **issue\[0\].rd_valid \<- dec_rd_en;                      |
|                                                                     |
| **issue\[0\].**ready** -\> can_issue;                     |
|                                                                     |
| **issue\[0\].token -\> sb_token;                          |
|                                                                     |
| // writeback from execution units                                   |
|                                                                     |
| **writeback\[0\].**valid** \<- ex0_done;                  |
|                                                                     |
| **writeback\[0\].token \<- ex0_token;                     |
|                                                                     |
| **flush_valid \<- **flush**;                              |
|                                                                     |
| **end** **inst** sb                                                 |
|                                                                     |
| **inst** rob: Rob                                                   |
|                                                                     |
| **clk \<- clk;                                            |
|                                                                     |
| **rst \<- rst;                                            |
|                                                                     |
| // allocate one ROB entry per issued instruction                    |
|                                                                     |
| **alloc\[0\].**valid** \<- can_issue;                     |
|                                                                     |
| **alloc\[0\].data \<- dec_rob_entry;                      |
|                                                                     |
| **alloc\[0\].token -\> rob_token;                         |
|                                                                     |
| // complete from execution units                                    |
|                                                                     |
| **complete\[0\].**valid** \<- ex0_done;                   |
|                                                                     |
| **complete\[0\].token \<- ex0_rob_token;                  |
|                                                                     |
| **complete\[0\].result \<- ex0_result;                    |
|                                                                     |
| // commit to architectural state                                    |
|                                                                     |
| **commit\[0\].**valid** -\> commit_valid;                 |
|                                                                     |
| **commit\[0\].data -\> commit_entry;                      |
|                                                                     |
| **commit\[0\].ack \<- retire_ack;                         |
|                                                                     |
| **flush_valid \<- **flush**;                              |
|                                                                     |
| **flush_token \<- flush_rob_token;                        |
|                                                                     |
| **end** **inst** rob                                                |
|                                                                     |
| **end** **module** OooFrontEnd                                      |
+---------------------------------------------------------------------+

**17. First-Class Construct: counter**

A counter is a first-class construct that captures the full variety of hardware counting patterns --- saturating, wrapping, up/down, gray-code, one-hot, and Johnson. Each mode has distinct synthesis implications that a raw reg block does not express. The compiler generates the correct arithmetic, encoding, and overflow/underflow detection for each mode, and catches impossible parameter combinations at compile time.

**17.1 Counter Modes**

  -----------------------------------------------------------------------------------------------------------------------------------------------
  **kind**       **Behaviour at Limit**                          **Encoding**   **Primary Use**
  -------------- ----------------------------------------------- -------------- -----------------------------------------------------------------
  **wrap**       Overflows back to 0 (or max)                    Binary         FIFOs, address generators, time slices

  **saturate**   Clamps at max (or 0) and holds                  Binary         Rate limiters, credit counters, performance monitors

  **gray**       Increments through Gray code sequence           Gray           CDC --- only one bit changes per increment; safe across domains

  **one_hot**    Single hot bit shifts through N positions       One-hot        State indicators, rotating selectors, low-power FSMs

  **johnson**    Shift-register ring --- 2N states from N bits   Johnson        Low-power clock dividers, phase generators
  -----------------------------------------------------------------------------------------------------------------------------------------------

**17.2 Declaration**

+--------------------------------------------------------------------------------------------+
| *counter.arch*                                                                             |
|                                                                                            |
| // Saturating credit counter --- clamps at 0 and MAX_CREDITS                               |
|                                                                                            |
| counter CreditCounter                                                                      |
|                                                                                            |
| **param** MAX: **const** = 64;                                                             |
|                                                                                            |
| **port** clk: **in** Clock\<SysDomain\>;                                                   |
|                                                                                            |
| **port** rst: **in** Reset\<Sync\>;                                                        |
|                                                                                            |
| **kind** saturate;                                                                         |
|                                                                                            |
| direction: up_down; // up \| down \| up_down                                               |
|                                                                                            |
| **init**: MAX; // reset value                                                              |
|                                                                                            |
| **port** inc: **in** Bool;                                                                 |
|                                                                                            |
| **port** dec: **in** Bool;                                                                 |
|                                                                                            |
| **port** value: **out** UInt\<\$clog2(MAX+1)\>;                                            |
|                                                                                            |
| **port** at_max: **out** Bool; // combinational: value == MAX                              |
|                                                                                            |
| **port** at_min: **out** Bool; // combinational: value == 0                                |
|                                                                                            |
| **end** counter CreditCounter                                                              |
|                                                                                            |
| // Gray-code counter for safe clock-domain crossing                                        |
|                                                                                            |
| counter GrayPtrCounter                                                                     |
|                                                                                            |
| **param** DEPTH: **const** = 256;                                                          |
|                                                                                            |
| **port** clk: **in** Clock\<WriteDomain\>;                                                 |
|                                                                                            |
| **port** rst: **in** Reset\<Async\>;                                                       |
|                                                                                            |
| **kind** gray;                                                                             |
|                                                                                            |
| direction: up;                                                                             |
|                                                                                            |
| **init**: 0;                                                                               |
|                                                                                            |
| **port** inc: **in** Bool;                                                                 |
|                                                                                            |
| **port** gray_value: **out** UInt\<\$clog2(DEPTH)\>; // safe to sample across domain       |
|                                                                                            |
| **port** bin_value: **out** UInt\<\$clog2(DEPTH)\>; // binary equivalent, same domain only |
|                                                                                            |
| **end** counter GrayPtrCounter                                                             |
+--------------------------------------------------------------------------------------------+

**17.3 One-Hot and Johnson Counters**

+--------------------------------------------------------------------------+
| *counter_onehot.arch*                                                    |
|                                                                          |
| // One-hot counter: exactly one bit active, shifts each cycle            |
|                                                                          |
| counter RoundRobinSel                                                    |
|                                                                          |
| **param** N: **const** = 8; // number of positions                       |
|                                                                          |
| **port** clk: **in** Clock\<SysDomain\>;                                 |
|                                                                          |
| **port** rst: **in** Reset\<Sync\>;                                      |
|                                                                          |
| **kind** one_hot;                                                        |
|                                                                          |
| direction: up;                                                           |
|                                                                          |
| **init**: 0; // start at position 0 (bit 0 hot)                          |
|                                                                          |
| **port** advance: **in** Bool;                                           |
|                                                                          |
| **port** sel: **out** Vec\<Bool, N\>; // one-hot selection vector        |
|                                                                          |
| **port** index: **out** UInt\<\$clog2(N)\>; // binary equivalent         |
|                                                                          |
| **end** counter RoundRobinSel                                            |
|                                                                          |
| // Johnson counter: 2N states from N flip-flops, glitch-free transitions |
|                                                                          |
| counter PhaseGen                                                         |
|                                                                          |
| **param** N: **const** = 4; // generates 8 phases                        |
|                                                                          |
| **port** clk: **in** Clock\<SysDomain\>;                                 |
|                                                                          |
| **port** rst: **in** Reset\<Sync\>;                                      |
|                                                                          |
| **kind** johnson;                                                        |
|                                                                          |
| direction: up;                                                           |
|                                                                          |
| **init**: 0;                                                             |
|                                                                          |
| **port** advance: **in** Bool;                                           |
|                                                                          |
| **port** phase: **out** UInt\<N\>; // Johnson-encoded phase              |
|                                                                          |
| **end** counter PhaseGen                                                 |
+--------------------------------------------------------------------------+

> *⚑ The compiler rejects impossible combinations at compile time --- for example, kind gray with direction: up_down is undefined (Gray codes only have a defined unidirectional sequence) and kind one_hot with saturate semantics is self-contradictory.*

**18. First-Class Construct: pqueue**

A hardware priority queue (pqueue) maintains a dynamic set of elements and provides O(log N) insert and O(1) extract of the minimum or maximum element. It is implemented as a pipelined binary heap in a RAM. The construct is fundamentally different from fifo (which has no ordering), linklist (which is sequentially ordered by insertion), and arbiter (which selects among fixed simultaneous requesters). Primary uses include hardware task schedulers, packet schedulers (WFQ, DRR, LSTF), and timer management units.

**18.1 Declaration**

+---------------------------------------------------------------------------+
| *pqueue.arch*                                                             |
|                                                                           |
| // Hardware min-heap task scheduler: 256 tasks, 32-bit priority key       |
|                                                                           |
| pqueue TaskScheduler                                                      |
|                                                                           |
| **param** DEPTH: **const** = 256;                                         |
|                                                                           |
| **param** KEY_W: **const** = 32; // priority key width                    |
|                                                                           |
| **param** DATA_W: **type** = TaskDescriptor;                              |
|                                                                           |
| **port** clk: **in** Clock\<SysDomain\>;                                  |
|                                                                           |
| **port** rst: **in** Reset\<Sync\>;                                       |
|                                                                           |
| order: min_first; // min_first \| max_first                               |
|                                                                           |
| // Insert: add an element with its priority key                           |
|                                                                           |
| // Latency: O(log2 DEPTH) cycles --- heap sift-up                         |
|                                                                           |
| op insert                                                                 |
|                                                                           |
| latency: \$clog2(DEPTH); // 8 cycles for depth 256                        |
|                                                                           |
| **port** req_valid: **in** Bool;                                          |
|                                                                           |
| **port** req_ready: **out** Bool;                                         |
|                                                                           |
| **port** req_key: **in** UInt\<KEY_W\>;                                   |
|                                                                           |
| **port** req_data: **in** DATA_W;                                         |
|                                                                           |
| **port** resp_valid: **out** Bool;                                        |
|                                                                           |
| **end** op insert                                                         |
|                                                                           |
| // Extract: remove and return the top element (min or max)                |
|                                                                           |
| // Latency: O(log2 DEPTH) cycles --- heap sift-down after removal         |
|                                                                           |
| op extract                                                                |
|                                                                           |
| latency: \$clog2(DEPTH);                                                  |
|                                                                           |
| **port** req_valid: **in** Bool;                                          |
|                                                                           |
| **port** req_ready: **out** Bool;                                         |
|                                                                           |
| **port** resp_valid: **out** Bool;                                        |
|                                                                           |
| **port** resp_key: **out** UInt\<KEY_W\>;                                 |
|                                                                           |
| **port** resp_data: **out** DATA_W;                                       |
|                                                                           |
| **end** op extract                                                        |
|                                                                           |
| // Peek: read the top element without removing it --- O(1), combinational |
|                                                                           |
| op **peek**                                                               |
|                                                                           |
| latency: 0;                                                               |
|                                                                           |
| **port** resp_key: **out** UInt\<KEY_W\>;                                 |
|                                                                           |
| **port** resp_data: **out** DATA_W;                                       |
|                                                                           |
| **end** op **peek**                                                       |
|                                                                           |
| // Update: change the priority key of an element already in the heap      |
|                                                                           |
| // Requires a handle returned at insert time                              |
|                                                                           |
| op update                                                                 |
|                                                                           |
| latency: \$clog2(DEPTH);                                                  |
|                                                                           |
| **port** req_valid: **in** Bool;                                          |
|                                                                           |
| **port** req_ready: **out** Bool;                                         |
|                                                                           |
| **port** req_handle: **in** UInt\<\$clog2(DEPTH)\>;                       |
|                                                                           |
| **port** req_new_key: **in** UInt\<KEY_W\>;                               |
|                                                                           |
| **port** resp_valid: **out** Bool;                                        |
|                                                                           |
| **end** op update                                                         |
|                                                                           |
| **port** **full**: **out** Bool;                                          |
|                                                                           |
| **port** **empty**: **out** Bool;                                         |
|                                                                           |
| **port** occupancy: **out** UInt\<\$clog2(DEPTH+1)\>;                     |
|                                                                           |
| **end** pqueue TaskScheduler                                              |
+---------------------------------------------------------------------------+

**18.2 Pipelining**

The latency of insert and extract is O(log₂ DEPTH) by the nature of heap sift operations. The compiler generates a pipelined sift controller that can accept a new request every cycle even while a sift is in progress, provided the new request does not conflict with an in-flight sift on the same heap path. The compiler generates the conflict detection logic automatically.

+--------------------------------------------------------------------------------------+
| *pqueue_pipelined.arch*                                                              |
|                                                                                      |
| // Pipelined insert: new insert accepted every cycle if no path conflict             |
|                                                                                      |
| op insert                                                                            |
|                                                                                      |
| latency: \$clog2(DEPTH);                                                             |
|                                                                                      |
| pipelined: true; // compiler generates sift-path conflict detection                  |
|                                                                                      |
| **port** req_valid: **in** Bool;                                                     |
|                                                                                      |
| **port** req_ready: **out** Bool;                                                    |
|                                                                                      |
| **port** req_key: **in** UInt\<KEY_W\>;                                              |
|                                                                                      |
| **port** req_data: **in** DATA_W;                                                    |
|                                                                                      |
| **port** resp_valid: **out** Bool;                                                   |
|                                                                                      |
| **port** resp_handle: **out** UInt\<\$clog2(DEPTH)\>; // heap slot for update/cancel |
|                                                                                      |
| **end** op insert                                                                    |
+--------------------------------------------------------------------------------------+

**18.3 Composite Priority Keys**

Real schedulers rarely use a single integer key. Arch allows struct types as priority keys with a declared comparison function.

+--------------------------------------------------------------------+
| *pqueue_composite.arch*                                            |
|                                                                    |
| // Packet scheduler key: deadline + flow_id for tie-breaking       |
|                                                                    |
| **struct** SchedKey                                                |
|                                                                    |
| deadline: UInt\<32\>,                                              |
|                                                                    |
| flow_id: UInt\<16\>,                                               |
|                                                                    |
| **end** **struct** SchedKey                                        |
|                                                                    |
| pqueue PacketSched                                                 |
|                                                                    |
| **param** DEPTH: **const** = 512;                                  |
|                                                                    |
| **param** KEY_W: **type** = SchedKey;                              |
|                                                                    |
| **param** DATA_W: **type** = PacketDescriptor;                     |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| order: min_first;                                                  |
|                                                                    |
| // Comparison function: primary key = deadline (ascending),        |
|                                                                    |
| // tie-break = flow_id (ascending)                                 |
|                                                                    |
| compare: **fn**(a: SchedKey, b: SchedKey) -\> Bool                 |
|                                                                    |
| a.deadline \< b.deadline **or**                                    |
|                                                                    |
| (a.deadline == b.deadline **and** a.flow_id \< b.flow_id)          |
|                                                                    |
| **end** **fn**;                                                    |
|                                                                    |
| op insert                                                          |
|                                                                    |
| latency: \$clog2(DEPTH);                                           |
|                                                                    |
| pipelined: true;                                                   |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** req_key: **in** SchedKey;                                 |
|                                                                    |
| **port** req_data: **in** PacketDescriptor;                        |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_handle: **out** UInt\<\$clog2(DEPTH)\>;              |
|                                                                    |
| **end** op insert                                                  |
|                                                                    |
| op extract                                                         |
|                                                                    |
| latency: \$clog2(DEPTH);                                           |
|                                                                    |
| **port** req_valid: **in** Bool;                                   |
|                                                                    |
| **port** req_ready: **out** Bool;                                  |
|                                                                    |
| **port** resp_valid: **out** Bool;                                 |
|                                                                    |
| **port** resp_key: **out** SchedKey;                               |
|                                                                    |
| **port** resp_data: **out** PacketDescriptor;                      |
|                                                                    |
| **end** op extract                                                 |
|                                                                    |
| op **peek**                                                        |
|                                                                    |
| latency: 0;                                                        |
|                                                                    |
| **port** resp_key: **out** SchedKey;                               |
|                                                                    |
| **port** resp_data: **out** PacketDescriptor;                      |
|                                                                    |
| **end** op **peek**                                                |
|                                                                    |
| **port** **empty**: **out** Bool;                                  |
|                                                                    |
| **port** **full**: **out** Bool;                                   |
|                                                                    |
| **end** pqueue PacketSched                                         |
+--------------------------------------------------------------------+

> ◈ The compare function is a pure combinational expression over the key type. The compiler uses it as the comparison gate at every level of the heap, generating the correct comparator hardware throughout the sift controller. It may reference any field of the key struct and any arithmetic operator.

**18.4 Generated Hardware**

  ----------------------------------------------------------------------------------------------------------------------------------------
  **Component**              **Implementation**                         **Notes**
  -------------------------- ------------------------------------------ ------------------------------------------------------------------
  **Heap storage RAM**       simple_dual, DEPTH × (KEY_W + DATA_W)      Stores key+data per node; index 0 = root

  **Handle→index table**     cam\<associative\>                         Maps user handle to current heap position; updated on every sift

  **Sift-up controller**     Pipeline FSM, log₂(DEPTH) stages           Executes insert and bubble-up

  **Sift-down controller**   Pipeline FSM, log₂(DEPTH) stages           Executes extract and trickle-down

  **Comparator**             Combinational, generated from compare fn   One instance per heap level

  **Conflict detector**      Bitmask of active sift paths               Blocks pipelined requests that share a heap path

  **Occupancy counter**      counter\<wrap\>                            Tracks current size; drives full/empty
  ----------------------------------------------------------------------------------------------------------------------------------------

**19. Compile-Time Generation: generate**

The generate system allows any structural element of an Arch construct --- ports, instances, connections, registers, and assertions --- to be created by compile-time iteration or conditional evaluation. This is a fundamental capability that SystemVerilog generate lacks: in SystemVerilog, the port list of a module is always a fixed declaration; generate can add internal logic but cannot add or remove ports. In Arch, generate operates before elaboration, so generated ports are indistinguishable from hand-written ports from the perspective of the type system, safety checks, and callers.

**19.1 generate for --- Iteration**

A generate for loop iterates a compile-time constant range and emits one copy of its body per iteration. The loop variable is a compile-time integer constant within the body. It may appear at the top level of any construct body to generate ports, instances, registers, connections, or assertions.

+--------------------------------------------------------------------+
| *gen_for.arch*                                                     |
|                                                                    |
| // Syntax                                                          |
|                                                                    |
| generate **for** VARNAME **in** START..END // inclusive range      |
|                                                                    |
| // body --- VARNAME is a compile-time const within here            |
|                                                                    |
| **end** generate **for** VARNAME                                   |
|                                                                    |
| generate **for** VARNAME **in** START..END step STEP               |
|                                                                    |
| // body                                                            |
|                                                                    |
| **end** generate **for** VARNAME                                   |
|                                                                    |
| // Example: generate 8 input ports of varying width                |
|                                                                    |
| **module** FanIn                                                   |
|                                                                    |
| **param** N: **const** = 8;                                        |
|                                                                    |
| **param** BASE: **const** = 4; // port\[i\] has width BASE\*(i+1)  |
|                                                                    |
| generate **for** i **in** 0..N-1                                   |
|                                                                    |
| **port** **in**\[i\]: **in** UInt\<BASE \* (i + 1)\>;              |
|                                                                    |
| **port** vld\[i\]: **in** Bool;                                    |
|                                                                    |
| **end** generate **for** i                                         |
|                                                                    |
| **port** **out**: **out** UInt\<BASE \* N\>;                       |
|                                                                    |
| **port** out_vld: **out** Bool;                                    |
|                                                                    |
| **end** **module** FanIn                                           |
+--------------------------------------------------------------------+

> ◈ The range START..END is inclusive on both ends. START..END step STEP increments by STEP each iteration. All three expressions must be compile-time constants derivable from param declarations.

**19.2 generate if --- Conditional Ports and Logic**

A generate if block is evaluated entirely at compile time. If the condition is true, the body is emitted; if false, it is as if the block was never written. This enables optional ports --- a capability that does not exist in SystemVerilog.

+-----------------------------------------------------------------------------+
| *gen_if.arch*                                                               |
|                                                                             |
| // Syntax                                                                   |
|                                                                             |
| generate **if** CONDITION                                                   |
|                                                                             |
| // emitted only when CONDITION is true at compile time                      |
|                                                                             |
| **end** generate **if**                                                     |
|                                                                             |
| generate **if** CONDITION                                                   |
|                                                                             |
| // true branch                                                              |
|                                                                             |
| **end** generate **if**                                                     |
|                                                                             |
| generate **else**                                                           |
|                                                                             |
| // false branch                                                             |
|                                                                             |
| **end** generate **else**                                                   |
|                                                                             |
| // Example: optional debug port --- only exists when DEBUG_EN param is true |
|                                                                             |
| **module** Accelerator                                                      |
|                                                                             |
| **param** DEBUG_EN: **const** = false;                                      |
|                                                                             |
| **param** DATA_W: **const** = 32;                                           |
|                                                                             |
| **param** DEPTH: **const** = 256;                                           |
|                                                                             |
| **port** clk: **in** Clock\<SysDomain\>;                                    |
|                                                                             |
| **port** rst: **in** Reset\<Sync\>;                                         |
|                                                                             |
| **port** data_in: **in** UInt\<DATA_W\>;                                    |
|                                                                             |
| **port** data_out: **out** UInt\<DATA_W\>;                                  |
|                                                                             |
| // These ports only exist when DEBUG_EN is true.                            |
|                                                                             |
| // When DEBUG_EN is false, connecting to them is a compile error.           |
|                                                                             |
| generate **if** DEBUG_EN                                                    |
|                                                                             |
| **port** dbg_pc: **out** UInt\<32\>;                                        |
|                                                                             |
| **port** dbg_state: **out** UInt\<8\>;                                      |
|                                                                             |
| **port** dbg_valid: **out** Bool;                                           |
|                                                                             |
| **end** generate **if**                                                     |
|                                                                             |
| // Optional second memory port for DEPTH \> 512 --- bandwidth required      |
|                                                                             |
| generate **if** DEPTH \> 512                                                |
|                                                                             |
| **port** mem_port_b                                                         |
|                                                                             |
| en: **in** Bool;                                                            |
|                                                                             |
| addr: **in** UInt\<\$clog2(DEPTH)\>;                                        |
|                                                                             |
| data: **out** UInt\<DATA_W\>;                                               |
|                                                                             |
| **end** **port** mem_port_b                                                 |
|                                                                             |
| **end** generate **if**                                                     |
|                                                                             |
| **end** **module** Accelerator                                              |
+-----------------------------------------------------------------------------+

> *⚑ A caller that instantiates Accelerator with DEBUG_EN = false and attempts to dbg_pc receives a compile-time error: port dbg_pc does not exist when DEBUG_EN = false. The type system is fully aware of which ports exist for each parameter combination.*

**19.3 generate match --- Structural Variant Selection**

A generate match selects one of several structural bodies based on the value of a compile-time expression. It is the generate-level equivalent of a type-level switch. It is commonly used to select between different internal implementations based on a mode parameter.

+----------------------------------------------------------------------------------------------+
| *gen_match.arch*                                                                             |
|                                                                                              |
| // Syntax                                                                                    |
|                                                                                              |
| generate **match** EXPRESSION                                                                |
|                                                                                              |
| case VALUE_A:                                                                                |
|                                                                                              |
| // body for VALUE_A                                                                          |
|                                                                                              |
| **end** case                                                                                 |
|                                                                                              |
| case VALUE_B:                                                                                |
|                                                                                              |
| // body for VALUE_B                                                                          |
|                                                                                              |
| **end** case                                                                                 |
|                                                                                              |
| **default**:                                                                                 |
|                                                                                              |
| // fallback body                                                                             |
|                                                                                              |
| **end** **default**                                                                          |
|                                                                                              |
| **end** generate **match**                                                                   |
|                                                                                              |
| // Example: select internal FIFO implementation based on a mode parameter                    |
|                                                                                              |
| **module** DataPath                                                                          |
|                                                                                              |
| **param** FIFO_MODE: **const** = \'standard\'; // \'standard\' \| \'async\' \| \'showahead\' |
|                                                                                              |
| **param** DEPTH: **const** = 64;                                                             |
|                                                                                              |
| **param** WIDTH: **type** = UInt\<32\>;                                                      |
|                                                                                              |
| **port** clk_wr: **in** Clock\<WriteDomain\>;                                                |
|                                                                                              |
| **port** clk_rd: **in** Clock\<ReadDomain\>;                                                 |
|                                                                                              |
| **port** rst: **in** Reset\<Async\>;                                                         |
|                                                                                              |
| generate **match** FIFO_MODE                                                                 |
|                                                                                              |
| case \'standard\':                                                                           |
|                                                                                              |
| // Single-clock FIFO --- both clocks must be same domain                                     |
|                                                                                              |
| **fifo** DataFifo                                                                            |
|                                                                                              |
| **param** DEPTH = DEPTH; **param** WIDTH = WIDTH;                                            |
|                                                                                              |
| **port** clk \<- clk_wr;                                                                     |
|                                                                                              |
| **port** push_valid \<- push_vld;                                                            |
|                                                                                              |
| **end** **fifo** DataFifo                                                                    |
|                                                                                              |
| **end** case                                                                                 |
|                                                                                              |
| case \'**async**\':                                                                          |
|                                                                                              |
| // Dual-clock FIFO --- compiler generates gray-code CDC                                      |
|                                                                                              |
| **fifo** DataFifo                                                                            |
|                                                                                              |
| **param** DEPTH = DEPTH; **param** WIDTH = WIDTH;                                            |
|                                                                                              |
| **port** wr_clk \<- clk_wr; **port** rd_clk \<- clk_rd;                                      |
|                                                                                              |
| **port** push_valid \<- push_vld;                                                            |
|                                                                                              |
| **end** **fifo** DataFifo                                                                    |
|                                                                                              |
| **end** case                                                                                 |
|                                                                                              |
| case \'showahead\':                                                                          |
|                                                                                              |
| // FIFO with combinational pop --- registered internally, async output                       |
|                                                                                              |
| **fifo** DataFifo                                                                            |
|                                                                                              |
| **param** DEPTH = DEPTH; **param** WIDTH = WIDTH;                                            |
|                                                                                              |
| **port** clk \<- clk_wr;                                                                     |
|                                                                                              |
| **read**: **async**;                                                                         |
|                                                                                              |
| **port** push_valid \<- push_vld;                                                            |
|                                                                                              |
| **end** **fifo** DataFifo                                                                    |
|                                                                                              |
| **end** case                                                                                 |
|                                                                                              |
| **end** generate **match**                                                                   |
|                                                                                              |
| **end** **module** DataPath                                                                  |
+----------------------------------------------------------------------------------------------+

**19.4 Generated Instance Arrays --- With Inter-Instance Wiring**

The primary structural use of generate for is instantiating N copies of a submodule and wiring them together. Arch handles the common case of chain wiring --- where instance i receives output from instance i-1 --- with a boundary expression using the ?: operator. The boundary condition is evaluated at compile time for i=0.

+-----------------------------------------------------------------------------------+
| *systolic_generate.arch*                                                          |
|                                                                                   |
| // Systolic array: N processing elements wired in a chain                         |
|                                                                                   |
| // The sum_in of PE\[0\] is 0; for PE\[i\>0\] it is PE\[i-1\].sum_out             |
|                                                                                   |
| **pipeline** SystolicArray                                                        |
|                                                                                   |
| **param** SIZE: **const** = 4; // change to 8, 16, 32 --- no other changes needed |
|                                                                                   |
| **param** ACC_W: **const** = 32;                                                  |
|                                                                                   |
| **port** clk: **in** Clock\<SysDomain\>;                                          |
|                                                                                   |
| **port** rst: **in** Reset\<Sync\>;                                               |
|                                                                                   |
| **port** en: **in** Bool;                                                         |
|                                                                                   |
| // Generated ports --- SIZE input activations, SIZE weight inputs, SIZE results   |
|                                                                                   |
| // This is impossible in SystemVerilog --- ports cannot be generated.             |
|                                                                                   |
| generate **for** i **in** 0..SIZE-1                                               |
|                                                                                   |
| **port** a_in\[i\]: **in** SInt\<8\>;                                             |
|                                                                                   |
| **port** b_in\[i\]: **in** SInt\<8\>;                                             |
|                                                                                   |
| **port** result\[i\]: **out** SInt\<ACC_W\>;                                      |
|                                                                                   |
| **end** generate **for** i                                                        |
|                                                                                   |
| **stage** Compute                                                                 |
|                                                                                   |
| generate **for** i **in** 0..SIZE-1                                               |
|                                                                                   |
| **inst** pe\[i\]: SystolicPE                                                      |
|                                                                                   |
| **param** ACC_W = ACC_W;                                                          |
|                                                                                   |
| **clk \<- clk;                                                          |
|                                                                                   |
| **rst \<- rst;                                                          |
|                                                                                   |
| **en \<- en;                                                            |
|                                                                                   |
| **a_in \<- a_in\[i\];                                                   |
|                                                                                   |
| **b_in \<- b_in\[i\];                                                   |
|                                                                                   |
| // Boundary expression: PE\[0\] gets 0; PE\[i\] gets PE\[i-1\].sum_out            |
|                                                                                   |
| **sum_in \<- i == 0 ? 0.sext\<ACC_W\>() : pe\[i-1\].sum_out;            |
|                                                                                   |
| **sum_out -\> result\[i\];                                              |
|                                                                                   |
| **end** **inst** pe\[i\]                                                          |
|                                                                                   |
| **end** generate **for** i                                                        |
|                                                                                   |
| **end** **stage** Compute                                                         |
|                                                                                   |
| **stall** **when** en == false;                                                   |
|                                                                                   |
| // Generated assertions --- one overflow check per PE                             |
|                                                                                   |
| generate **for** i **in** 0..SIZE-1                                               |
|                                                                                   |
| **assert** pe_valid\[i\]: pe\[i\].acc_reg \>= -SInt\<ACC_W\>().min() **and**      |
|                                                                                   |
| pe\[i\].acc_reg \<= SInt\<ACC_W\>().max();                                        |
|                                                                                   |
| **end** generate **for** i                                                        |
|                                                                                   |
| **end** **pipeline** SystolicArray                                                |
|                                                                                   |
| // Instantiation --- the caller sees SIZE ports by name                           |
|                                                                                   |
| **inst** array: SystolicArray                                                     |
|                                                                                   |
| **param** SIZE = 8; // scale from 4×4 to 8×8 by changing one param                |
|                                                                                   |
| **param** ACC_W = 32;                                                             |
|                                                                                   |
| **clk \<- clk;                                                          |
|                                                                                   |
| **en \<- compute_en;                                                    |
|                                                                                   |
| // Generated ports accessed by index                                              |
|                                                                                   |
| generate **for** i **in** 0..7                                                    |
|                                                                                   |
| **a_in\[i\] \<- act_row\[i\];                                           |
|                                                                                   |
| **b_in\[i\] \<- wgt_col\[i\];                                           |
|                                                                                   |
| **result\[i\] -\> output_row\[i\];                                      |
|                                                                                   |
| **end** generate **for** i                                                        |
|                                                                                   |
| **end** **inst** array                                                            |
+-----------------------------------------------------------------------------------+

> ◈ The SIZE parameter change from 4 to 8 requires one edit. The compiler regenerates all 8 port declarations, 8 PE instances with correct chain wiring, and 8 overflow assertions. In SystemVerilog, the equivalent change requires rewriting 64 PE instantiations, 8 port declarations, and every inter-instance wire.

**19.5 Generated Signals in Buses**

Generated signals are fully supported inside bus declarations. This allows a parameterized bus whose signal bundle varies with configuration --- a common need in bus protocols and configurable datapaths.

+-------------------------------------------------------------------------+
| *gen_bus.arch*                                                          |
|                                                                         |
| // AXI-lite bus with a configurable number of ID bits                   |
|                                                                         |
| **bus** AxiLite                                                         |
|   **param** ADDR_W: **const** = 32;                                     |
|   **param** DATA_W: **const** = 32;                                     |
|   **param** ID_W: **const** = 0; // 0 = no ID field (standard AXI-lite) |
|                                                                         |
|   awvalid: **out** Bool;  awready: **in** Bool;                         |
|   awaddr:  **out** UInt\<ADDR_W\>;                                      |
|                                                                         |
|   // ID field is optional --- only present when ID_W \> 0               |
|   generate **if** ID_W \> 0                                             |
|     awid: **out** UInt\<ID_W\>;                                         |
|   **end** generate **if**                                               |
|                                                                         |
|   wvalid: **out** Bool;  wready: **in** Bool;                           |
|   wdata:  **out** UInt\<DATA_W\>;                                       |
|   wstrb:  **out** UInt\<DATA_W / 8\>;                                   |
|   bvalid: **in** Bool;   bready: **out** Bool;                          |
|   bresp:  **in** UInt\<2\>;                                             |
|   arvalid: **out** Bool; arready: **in** Bool;                          |
|   araddr:  **out** UInt\<ADDR_W\>;                                      |
|                                                                         |
|   generate **if** ID_W \> 0                                             |
|     arid: **out** UInt\<ID_W\>;                                         |
|     rid:  **in** UInt\<ID_W\>;                                          |
|   **end** generate **if**                                               |
|                                                                         |
|   rvalid: **in** Bool;   rready: **out** Bool;                          |
|   rdata:  **in** UInt\<DATA_W\>;                                        |
|   rresp:  **in** UInt\<2\>;                                             |
| **end** **bus** AxiLite                                                 |
|                                                                         |
| // A module using this bus with ID_W=4 (initiator side)                 |
|                                                                         |
| **module** DmaEngine                                                    |
|   **port** clk: **in** Clock\<SysDomain\>;                              |
|   **port** rst: **in** Reset\<Sync\>;                                   |
|   **port** axi: **initiator** AxiLite\<ADDR_W=32, DATA_W=32, ID_W=4\>; |
|   // Compiler knows axi.awid and axi.arid exist --- ID_W=4 \> 0        |
| **end** **module** DmaEngine                                            |
|                                                                         |
| // A module using standard AXI-lite, no ID (target side)                |
|                                                                         |
| **module** SimplePeripheral                                             |
|   **port** clk: **in** Clock\<SysDomain\>;                              |
|   **port** rst: **in** Reset\<Sync\>;                                   |
|   **port** axi: **target** AxiLite\<ADDR_W=32, DATA_W=32, ID_W=0\>;    |
|   // Compiler knows axi.awid does NOT exist --- compile error if used   |
| **end** **module** SimplePeripheral                                     |
+-------------------------------------------------------------------------+

> *⚑ When ID_W = 0, any attempt to access axi.awid is a compile-time error. This is structurally impossible in SystemVerilog --- optional fields in a bus must be worked around with unused signals or conditional compilation macros.*

**19.6 generate for over Type Lists**

A generate for loop may iterate over a list of types in addition to integer ranges. This allows generating ports or instances of different types within one loop --- useful for heterogeneous datapaths.

+--------------------------------------------------------------------+
| *gen_typelist.arch*                                                |
|                                                                    |
| // Generate three accumulator banks of different widths            |
|                                                                    |
| **module** MultiPrecisionAcc                                       |
|                                                                    |
| **param** N: **const** = 4; // accumulators per precision          |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| // Type list --- three precisions                                  |
|                                                                    |
| generate **for** T **in** \[UInt\<8\>, UInt\<16\>, UInt\<32\>\]    |
|                                                                    |
| generate **for** i **in** 0..N-1                                   |
|                                                                    |
| **port** in_T\_\[i\]: **in** T;                                    |
|                                                                    |
| **port** acc_T\_\[i\]: **out** T;                                  |
|                                                                    |
| **reg** r_T\_\[i\]: T **init** 0;                                  |
|                                                                    |
| **reg** r_T\_\[i\] **on** clk rising, rst high                     |
|                                                                    |
| **if** rst r_T\_\[i\] \<= 0;                                       |
|                                                                    |
| **end** **if**                                                     |
|                                                                    |
| **else** r_T\_\[i\] \<= r_T\_\[i\] + in_T\_\[i\];                  |
|                                                                    |
| **end** **else**                                                   |
|                                                                    |
| **end** **reg**                                                    |
|                                                                    |
| **comb** acc_T\_\[i\] = r_T\_\[i\]; **end** **comb**               |
|                                                                    |
| **end** generate **for** i                                         |
|                                                                    |
| **end** generate **for** T                                         |
|                                                                    |
| **end** **module** MultiPrecisionAcc                               |
|                                                                    |
| // Generates: in_UInt8_0..3, acc_UInt8_0..3,                       |
|                                                                    |
| // in_UInt16_0..3, acc_UInt16_0..3,                                |
|                                                                    |
| // in_UInt32_0..3, acc_UInt32_0..3                                 |
+--------------------------------------------------------------------+

> ◈ The type name is mangled into the port name using underscore concatenation. Generated port names are always deterministic and derivable from the parameter values --- the compiler documents the name mapping in the output port list for callers.

**19.7 Constraints and Compile-Time Errors**

  --------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Rule**                                                **Error Raised When**
  ------------------------------------------------------- ------------------------------------------------------------------------------------------------------
  **Range must be const**                                 Loop bound contains a runtime expression --- e.g., depends on a port value

  **No overlap between generated and manual ports**       A hand-written port name matches a generated port name

  **Boundary expression must be type-safe**               i-1 boundary in sum_in \<- i==0 ? 0 : pe\[i-1\].sum_out has mismatched types

  **generate if condition must be const**                 Condition references a port signal rather than a param

  **Caller must satisfy all generated ports**             Instantiating a module with generate for ports without connecting all generated ports is an error

  **Type list generate must produce uniform structure**   Each type in the list must produce the same structural shape so the compiler can verify completeness
  --------------------------------------------------------------------------------------------------------------------------------------------------------------

**19.8 generate vs ports\[\] --- When to Use Which**

  --------------------------------------------------------------------------------------------------------------------------------------
                              **ports\[N\]**                           **generate for ports**
  --------------------------- ---------------------------------------- -----------------------------------------------------------------
  **Port types**              All ports in the bundle are identical    Ports may differ by index --- different types, widths, or names

  **Count source**            Must be a single const param             Any compile-time expression, including inequalities

  **Conditional existence**   No --- all N ports always exist          Yes --- generate if wraps the entire group

  **Inter-port arithmetic**   Not applicable                           Yes --- port\[i\] can reference port\[i-1\]

  **Bus reuse**               Works with ports\[\] bundles             Works with bus declarations

  **Best for**                Uniform bus: N identical data channels   Heterogeneous: chain wiring, precision banks, optional ports
  --------------------------------------------------------------------------------------------------------------------------------------

**19.9 SystemVerilog Comparison**

  -------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Feature**                        **Arch generate**                                     **SystemVerilog generate**
  ---------------------------------- ----------------------------------------------------- --------------------------------------------------------------------
  **Generate ports**                 Yes --- generate for and generate if can add ports    No --- port list is fixed at module declaration

  **Optional ports**                 Yes --- generate if PARAM \> 0 wrapping port decl     No --- all ports always exist; unused signals are left unconnected

  **Conditional bus fields**         Yes --- inside bus declarations                       No --- bus signal list is fixed

  **Named loop variable in body**    Yes --- i is a named const throughout the body        Yes --- genvar i

  **Named block endings**            end generate for i --- unambiguous                    end --- no name; nesting requires counting ends

  **Inter-instance wiring**          pe\[i-1\].sum_out with boundary expression            Separate genvar + generate if for boundary case

  **Type list iteration**            Yes --- generate for T in \[UInt\<8\>, UInt\<16\>\]   No --- must use parameters and conditional casts

  **Assertion generation**           Yes --- assert inside generate for body               Yes --- property/assert inside generate

  **Compile-time error quality**     Error names the parameter and generated construct     Generic elaboration error with line number only
  -------------------------------------------------------------------------------------------------------------------------------------------------------------

**20. Native Execution and Compiled Simulation**

Arch compiles directly to a native binary for simulation. No external simulator --- no VCS, ModelSim, Questa, or Icarus --- is required. The arch sim command produces a self-contained executable that models the design cycle-accurately, evaluates all assertions, accumulates coverage, and optionally writes waveform files. This is possible because Arch\'s type system and structural rules eliminate the sources of complexity that make traditional HDL simulation hard to compile efficiently.

**20.1 Why Arch Can Be Natively Compiled**

Traditional Verilog simulators carry a heavyweight runtime because the language permits constructs that require dynamic resolution at simulation time. Arch eliminates every one of them at the language level:

  --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Verilog/SV Runtime Complexity**                                                    **Arch Equivalent**                                                                         **Enables**
  ------------------------------------------------------------------------------------ ------------------------------------------------------------------------------------------- -----------------------------------------------------------------------------
  **Delta cycles --- multiple zero-time evaluation passes until stability**            No combinational loops (compile error) --- evaluation order is a static DAG computed once   Bounded settle (1--2 passes, statically determined); no unbounded convergence loop

  **X/Z propagation --- unknown and high-impedance states require 4-valued logic**     No undriven signals (single-driver rule) --- all signals have defined values at all times   2-valued logic + runtime undefined-behavior detection (`--check-uninit`); see §20.1a for residual X sources

  **Multiple drivers --- wired-OR/AND resolution requires dynamic arbitration**        Single-driver rule enforced at compile time --- no multi-driver resolution at runtime       Every signal has exactly one update site; no resolution function needed

  **Non-deterministic always block ordering --- simulator-defined evaluation order**   Topological evaluation order computed at compile time from the dataflow DAG                 Deterministic simulation; identical results on every run and every platform

  **Implicit latches --- partial assignments create state-holding elements**           No implicit latches (compile error) --- every register is explicitly declared               Register set is fully enumerated; allocation is static

  **Dynamic clock generation --- clocks can be any signal**                            All clocks are Clock\<D\> typed --- domain membership verified at compile time              Clock schedule is fully static; multi-clock interleaving is pre-computed
  --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

> ◈ These are not runtime workarounds --- they are language-level invariants. The compiler proves them once during type-checking. The simulation binary never needs to check them again. This is what makes the simulation kernel fast: it is executing a pre-verified, topologically ordered dataflow graph, not interpreting an arbitrary event-driven program.

**20.1a Limitations of 2-State Simulation**

Arch's 2-state simulation eliminates X/Z propagation by construction, but several categories of undefined behavior remain that would manifest as X in a 4-state simulator. The `--check-uninit` flag detects some but not all of these at runtime:

| **Undefined Behavior Source** | **Detection** | **Status** |
|-------------------------------|---------------|------------|
| Read of `reset none` register before first write | `--check-uninit` runtime warning | ✅ Implemented |
| Read of `pipe_reg` before pipeline fills | `--check-uninit` runtime warning (propagates through chain) | ✅ Implemented |
| Read of primary input that TB never drove | `--inputs-start-uninit` runtime warning (per-port setter `dut.set_<port>()` marks init) | ✅ Implemented |
| Read of RAM cell before first write | `--check-uninit-ram` runtime warning (per-cell valid bitmap; `init:` cells marked valid at construction; ROMs exempt because they require `init:`) | ✅ Implemented |
| Out-of-bounds `Vec<T,N>` index at runtime | `arch sim`: hard abort (`_ARCH_BCHK`). Generated SV: auto-emitted `assert property (@(posedge clk) disable iff (rst) idx < N)` inside `translate_off/on`. Seq/latch contexts only (comb is deferred). Always on, no flag; constant indices verified statically. Verified with Verilator 5.034 (`--assert` trips `$fatal` on OOB) and EBMC 5.11 (PROVED when the index type structurally fits; REFUTED on unconstrained wider inputs — caller must constrain). | ✅ Implemented |
| Out-of-range bit-select `val[i]` on `UInt<W>`/`SInt<W>` at runtime | `arch sim`: hard abort. Generated SV: auto-emitted `assert property (idx < W)`. Same scope/flag story as Vec. EBMC-verified. | ✅ Implemented |
| Out-of-range variable part-select `val[start +:W]` / `val[start -:W]` | `arch sim`: hard abort on over/underflow. Generated SV: `assert property (start + W <= W_base)` for `+:`, and `start < W_base && start >= W - 1` for `-:`. | ✅ Implemented |
| Division by zero in a `const` expression (param default, const let initializer) | `arch check`: compile-time error `divide by zero in constant expression: divisor evaluates to 0` — catches `param X: const = A / 0;` and transitive cases where a param folds to zero. | ✅ Implemented |
| Division by zero at runtime (`/` or `%` with a non-const divisor) | `arch sim`: hard abort via `_ARCH_DCHK` (always on). Generated SV: auto-emitted `_auto_div0_div0_<n>: assert property ((divisor) != 0)` in `translate_off/on`. Seq/latch contexts only. Const divisors (literals, const-param refs, folded arithmetic) are exempt from both runtime check and SVA. Verified with EBMC 5.11: PROVED when the divisor is structurally non-zero (e.g., `den \| 1`); REFUTED for unconstrained inputs. | ✅ Implemented |
| Undriven output port | Compile-time error (single-driver rule) | ✅ Static |
| Implicit latch (incomplete `comb`) | Compile-time error | ✅ Static |
| Multiple drivers | Compile-time error | ✅ Static |
| Clock-domain crossing without synchronizer | Compile-time error | ✅ Static |

The first column lists sources of undefined values. The second column indicates how each is detected. The third column shows implementation status.

**Planned extensions to `--check-uninit`:**

(All items from earlier plans have been implemented. This list is retained for historical reference; see the table above for current coverage.)

> ◈ These runtime checks are simulation-only; they have no effect on generated SystemVerilog. The goal is to detect, at simulation time, the undefined behaviors that a 4-state simulator would expose via X propagation, without the overhead of a full 4-state simulation kernel.

**20.2 Arch Intermediate Representation (FIR)**

All three Arch output targets --- native simulation binary, SystemVerilog for synthesis, and SMT-LIB for formal verification --- are lowered from a common intermediate representation called FIR (Arch Intermediate Representation). FIR is a typed, clock-domain-aware dataflow graph.

+--------------------------------------------------------------------------------------+
| *fir_conceptual.fir*                                                                 |
|                                                                                      |
| // FIR is an internal compiler format --- shown here for conceptual clarity          |
|                                                                                      |
| // It is not written by designers; it is generated by the compiler from Arch source. |
|                                                                                      |
| // FIR nodes: signals, registers, combinational expressions, clock edges             |
|                                                                                      |
| fir_module SystolicPE                                                                |
|                                                                                      |
| // Signals --- typed, domain-tagged                                                  |
|                                                                                      |
| sig a_in: SInt\<8\> **domain** SysDomain;                                            |
|                                                                                      |
| sig b_in: SInt\<8\> **domain** SysDomain;                                            |
|                                                                                      |
| sig sum_in: SInt\<32\> **domain** SysDomain;                                         |
|                                                                                      |
| sig sum_out: SInt\<32\> **domain** SysDomain;                                        |
|                                                                                      |
| // Register state --- initial value + update expression + trigger                    |
|                                                                                      |
| **reg** acc_reg: SInt\<32\>                                                          |
|                                                                                      |
| **init** = 0;                                                                        |
|                                                                                      |
| **clock** = SysDomain.clk rising;                                                    |
|                                                                                      |
| **reset** = SysDomain.rst high → 0;                                                  |
|                                                                                      |
| update = sum_in + sext\<32\>(a_in) \* sext\<32\>(b_in);                              |
|                                                                                      |
| **end** **reg**                                                                      |
|                                                                                      |
| // Combinational graph --- topological evaluation order pre-assigned                 |
|                                                                                      |
| // Order 0: inputs (no dependencies)                                                 |
|                                                                                      |
| // Order 1: depends only on inputs                                                   |
|                                                                                      |
| // Order 2: depends on order-1 nodes                                                 |
|                                                                                      |
| // \...                                                                              |
|                                                                                      |
| **comb** sum_out = acc_reg; // order 0 --- from register                             |
|                                                                                      |
| // Assertions --- evaluated after comb settle                                        |
|                                                                                      |
| **assert** no_overflow                                                               |
|                                                                                      |
| expr = acc_reg \>= SInt\<32\>.min **and** acc_reg \<= SInt\<32\>.max;                |
|                                                                                      |
| **end** **assert**                                                                   |
|                                                                                      |
| **end** fir_module SystolicPE                                                        |
+--------------------------------------------------------------------------------------+

From FIR, the three backends diverge:

  ------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Backend**             **arch Command**   **AIR Lowering Strategy**                                             **Output**
  ----------------------- ------------------ --------------------------------------------------------------------- -------------------------------------------------
  **Native simulation**   arch sim           FIR nodes → LLVM IR → native machine code (x86-64 / ARM64 / RISC-V)   Self-contained simulation binary

  **Synthesis**           arch build         FIR nodes → SystemVerilog always_ff / always_comb / assign            Synthesisable SV for FPGA/ASIC tools

  **Formal**              arch formal        FIR nodes → SMT-LIB2 transition relation                              Constraint file for Z3 / Bitwuzla / JaspserGold
  ------------------------------------------------------------------------------------------------------------------------------------------------------------------

**20.3 The Simulation Execution Model**

The native simulation binary implements a cycle-accurate two-phase execution loop. The two phases per clock edge are identical to hardware behaviour: evaluate combinational logic, then commit register updates. This ordering is statically determined from FIR at compile time --- the binary contains no dynamic scheduling logic.

+----------------------------------------------------------------------------------------+
| *sim_kernel_pseudocode*                                                                |
|                                                                                        |
| // Pseudocode of the generated simulation kernel --- illustrative only                 |
|                                                                                        |
| // Phase 1: evaluate all combinational nodes in pre-computed topological order         |
|                                                                                        |
| // The order is a compile-time constant array --- no dynamic scheduling.               |
|                                                                                        |
| **fn** eval_comb(**state**: &mut SimState) {                                           |
|                                                                                        |
| // Each line is generated from one Arch comb expression.                               |
|                                                                                        |
| // The compiler unrolls the topological order into straight-line code.                 |
|                                                                                        |
| **state**.dot_product = vec_dot(**state**.q, **state**.k); // order 0                  |
|                                                                                        |
| **state**.scaled = (**state**.dot_product \* RECIP_SQRT) \>\> 8; // order 1            |
|                                                                                        |
| **state**.softmax_out = piecewise_exp(**state**.scaled); // order 2                    |
|                                                                                        |
| // \...                                                                                |
|                                                                                        |
| }                                                                                      |
|                                                                                        |
| // Phase 2: commit all register updates (non-blocking semantics)                       |
|                                                                                        |
| // All reg blocks evaluated against pre-phase-1 state, committed simultaneously.       |
|                                                                                        |
| **fn** commit_regs(**state**: &mut SimState, next: &SimState) {                        |
|                                                                                        |
| **state**.acc_reg = next.acc_reg;                                                      |
|                                                                                        |
| **state**.pc = next.pc;                                                                |
|                                                                                        |
| // \...                                                                                |
|                                                                                        |
| }                                                                                      |
|                                                                                        |
| // Main simulation loop                                                                |
|                                                                                        |
| **fn** simulate(cycles: u64) {                                                         |
|                                                                                        |
| **let** mut **state** = SimState::**init**(); // all registers at declared init values |
|                                                                                        |
| **for** cycle **in** 0..cycles {                                                       |
|                                                                                        |
| **let** next = compute_next_state(&**state**); // all reg update expressions           |
|                                                                                        |
| eval_comb(&mut **state**); // combinational settle                                     |
|                                                                                        |
| commit_regs(&mut **state**, &next); // register commit                                 |
|                                                                                        |
| eval_assertions(&**state**, cycle); // assert/cover evaluation                         |
|                                                                                        |
| **if** waveform_enabled { record_wave(&**state**, cycle); }                            |
|                                                                                        |
| }                                                                                      |
|                                                                                        |
| }                                                                                      |
+----------------------------------------------------------------------------------------+

> *⚑ The two-phase model is identical to how FPGA place-and-route tools model setup/hold timing. Phase 1 (combinational settle) determines the data that arrives at register inputs. Phase 2 (commit) captures that data on the clock edge. This is not an approximation --- it is the exact semantics of synchronous digital hardware.*

**20.4 Multi-Clock Scheduling**

Designs with multiple clock domains are handled by a compile-time-generated multi-clock scheduler. The scheduler interleaves clock edges from different domains at their declared frequencies and ensures crossing signals are sampled at the correct relative timing.

+------------------------------------------------------------------------------------+
| *multiclock_schedule.arch*                                                         |
|                                                                                    |
| // Domain declarations drive the simulation scheduler                              |
|                                                                                    |
| **domain** FastDomain { freq_mhz: 500 **end** **domain** FastDomain // period: 2ns |
|                                                                                    |
| **domain** SlowDomain { freq_mhz: 125 **end** **domain** SlowDomain // period: 8ns |
|                                                                                    |
| // The compiler generates a static event table:                                    |
|                                                                                    |
| // t=0ns: FastDomain rising, SlowDomain rising                                     |
|                                                                                    |
| // t=2ns: FastDomain rising                                                        |
|                                                                                    |
| // t=4ns: FastDomain rising                                                        |
|                                                                                    |
| // t=6ns: FastDomain rising                                                        |
|                                                                                    |
| // t=8ns: FastDomain rising, SlowDomain rising                                     |
|                                                                                    |
| // t=10ns: FastDomain rising                                                       |
|                                                                                    |
| // \... (LCM = 8ns → 4 Fast edges per 1 Slow edge)                                 |
|                                                                                    |
| // CDC signals declared with crossing blocks are sampled                           |
|                                                                                    |
| // at the correct phase --- the compiler inserts the two-flop                      |
|                                                                                    |
| // synchroniser model into the event table automatically.                          |
|                                                                                    |
| // Runtime: the scheduler is a pre-sorted static array of (time, domain) pairs.    |
|                                                                                    |
| // No dynamic priority queue --- O(1) per step.                                    |
+------------------------------------------------------------------------------------+

**20.5 The testbench Construct**

A testbench is a first-class Arch construct that describes simulation stimulus, checking, and control. It is not synthesisable --- the compiler rejects arch build on a design containing a testbench. It compiles only for arch sim and arch formal targets.

+-----------------------------------------------------------------------------------------+
| *testbench.arch*                                                                        |
|                                                                                         |
| testbench AttnUnitTb                                                                    |
|                                                                                         |
| // Declare the design under test                                                        |
|                                                                                         |
| dut: AttentionUnit                                                                      |
|                                                                                         |
| **param** D_K = 64;                                                                     |
|                                                                                         |
| **param** SEQ_LEN = 2048;                                                               |
|                                                                                         |
| **end** dut                                                                             |
|                                                                                         |
| // Simulation parameters                                                                |
|                                                                                         |
| run_for: 10000 cycles;                                                                  |
|                                                                                         |
| timeout: 50000 cycles; // fail if simulation does not finish                            |
|                                                                                         |
| // Declare testbench signals                                                            |
|                                                                                         |
| sig clk: Clock\<SysDomain\>;                                                            |
|                                                                                         |
| sig rst: Reset\<Sync\>;                                                                 |
|                                                                                         |
| sig in_valid: Bool;                                                                     |
|                                                                                         |
| sig in_token: QKVToken;                                                                 |
|                                                                                         |
| sig out_ready: Bool;                                                                    |
|                                                                                         |
| // Clock generation --- the testbench drives clocks                                     |
|                                                                                         |
| **clock** clk period 2ns;                                                               |
|                                                                                         |
| // Reset sequence --- runs at simulation start                                          |
|                                                                                         |
| **init**                                                                                |
|                                                                                         |
| rst = high;                                                                             |
|                                                                                         |
| wait 10 cycles;                                                                         |
|                                                                                         |
| rst = low;                                                                              |
|                                                                                         |
| wait 2 cycles;                                                                          |
|                                                                                         |
| **end** **init**                                                                        |
|                                                                                         |
| // Stimulus task --- a reusable named sequence                                          |
|                                                                                         |
| task send_token(q: Vec\<SInt\<8\>,64\>, k: Vec\<SInt\<8\>,64\>, v: Vec\<SInt\<8\>,64\>) |
|                                                                                         |
| in_valid = true;                                                                        |
|                                                                                         |
| in_token.q = q;                                                                         |
|                                                                                         |
| in_token.k = k;                                                                         |
|                                                                                         |
| in_token.v = v;                                                                         |
|                                                                                         |
| wait until dut.in_ready == true;                                                        |
|                                                                                         |
| wait 1 cycle;                                                                           |
|                                                                                         |
| in_valid = false;                                                                       |
|                                                                                         |
| **end** task send_token                                                                 |
|                                                                                         |
| // Check task --- verify output                                                         |
|                                                                                         |
| task expect_score(expected: SInt\<16\>, tolerance: UInt\<4\>)                           |
|                                                                                         |
| wait until dut.out_valid == true;                                                       |
|                                                                                         |
| check dut.out_score within expected ± tolerance                                         |
|                                                                                         |
| message \"Score mismatch: got {dut.out_score}, expected {expected}±{tolerance}\";       |
|                                                                                         |
| out_ready = true;                                                                       |
|                                                                                         |
| wait 1 cycle;                                                                           |
|                                                                                         |
| out_ready = false;                                                                      |
|                                                                                         |
| **end** task expect_score                                                               |
|                                                                                         |
| // Test body --- the main stimulus sequence                                             |
|                                                                                         |
| sequence main                                                                           |
|                                                                                         |
| // Test 1: zero input --- score should be 0                                             |
|                                                                                         |
| send_token(zeros(), zeros(), zeros());                                                  |
|                                                                                         |
| expect_score(expected: 0, tolerance: 1);                                                |
|                                                                                         |
| // Test 2: identity input --- score equals d_k                                          |
|                                                                                         |
| send_token(identity_vec(), identity_vec(), ones());                                     |
|                                                                                         |
| expect_score(expected: 64, tolerance: 2);                                               |
|                                                                                         |
| // Test 3: stress --- random inputs for 1000 cycles                                     |
|                                                                                         |
| repeat 1000                                                                             |
|                                                                                         |
| send_token(rand_vec(), rand_vec(), rand_vec());                                         |
|                                                                                         |
| wait until dut.out_valid == true;                                                       |
|                                                                                         |
| out_ready = true; wait 1 cycle; out_ready = false;                                      |
|                                                                                         |
| **end** repeat                                                                          |
|                                                                                         |
| // Test 4: backpressure --- hold out_ready low for 5 cycles                             |
|                                                                                         |
| send_token(rand_vec(), rand_vec(), rand_vec());                                         |
|                                                                                         |
| out_ready = false;                                                                      |
|                                                                                         |
| wait 5 cycles;                                                                          |
|                                                                                         |
| out_ready = true;                                                                       |
|                                                                                         |
| wait until dut.out_valid == true;                                                       |
|                                                                                         |
| wait 1 cycle; out_ready = false;                                                        |
|                                                                                         |
| report \"All tests passed.\";                                                           |
|                                                                                         |
| **end** sequence main                                                                   |
|                                                                                         |
| // Waveform output --- optional; adds \~20% simulation overhead                         |
|                                                                                         |
| waveform: \"attn_tb.fst\"; // FST format for GTKWave / Surfer                           |
|                                                                                         |
| waveform_depth: 2; // record signals 2 levels deep into dut                             |
|                                                                                         |
| **end** testbench AttnUnitTb                                                            |
+-----------------------------------------------------------------------------------------+

**20.6 Native FFI --- Linking with C, Rust, and Python**

The simulation binary exposes a C-compatible ABI so testbenches written in any language can drive the simulation directly. This enables hardware-software co-simulation without an external simulator bridge like DPI or VPI.

+------------------------------------------------------------------------------------+
| *ffi.arch*                                                                         |
|                                                                                    |
| // The Arch compiler generates a C header for every module marked with export_sim: |
|                                                                                    |
| **module** AttentionUnit                                                           |
|                                                                                    |
| export_sim: true; // generate C ABI wrapper                                        |
|                                                                                    |
| **param** D_K: **const** = 64;                                                     |
|                                                                                    |
| // \... rest of module \...                                                        |
|                                                                                    |
| **end** **module** AttentionUnit                                                   |
|                                                                                    |
| // Generated C header: attention_unit_sim.h                                        |
|                                                                                    |
| //                                                                                 |
|                                                                                    |
| // typedef struct AttentionUnit_State AttentionUnit_State;                         |
|                                                                                    |
| // AttentionUnit_State\* attention_unit_new();                                     |
|                                                                                    |
| // void attention_unit_free(AttentionUnit_State\*);                                |
|                                                                                    |
| // void attention_unit_reset(AttentionUnit_State\*);                               |
|                                                                                    |
| // void attention_unit_step(AttentionUnit_State\*); // advance one cycle           |
|                                                                                    |
| // void attention_unit_set_in_valid(AttentionUnit_State\*, bool);                  |
|                                                                                    |
| // void attention_unit_set_in_token(AttentionUnit_State\*, const int8_t\* q,       |
|                                                                                    |
| // const int8_t\* k, const int8_t\* v);                                            |
|                                                                                    |
| // bool attention_unit_get_out_valid(const AttentionUnit_State\*);                 |
|                                                                                    |
| // int16_t attention_unit_get_out_score(const AttentionUnit_State\*);              |
|                                                                                    |
| // Python binding (auto-generated via ctypes wrapper):                             |
|                                                                                    |
| // import arch_sim                                                                 |
|                                                                                    |
| // dut = arch_sim.AttentionUnit(D_K=64)                                            |
|                                                                                    |
| // dut.reset()                                                                     |
|                                                                                    |
| // dut.in_valid = True                                                             |
|                                                                                    |
| // dut.in_token_q = \[1\]\*64                                                      |
|                                                                                    |
| // dut.step()                                                                      |
|                                                                                    |
| // print(dut.out_score)                                                            |
|                                                                                    |
| // Rust binding (auto-generated via arch-rs crate):                                |
|                                                                                    |
| // let mut dut = AttentionUnit::new(D_K: 64);                                      |
|                                                                                    |
| // dut.reset();                                                                    |
|                                                                                    |
| // dut.in_valid.set(true);                                                         |
|                                                                                    |
| // dut.step();                                                                     |
|                                                                                    |
| // println!(\"{}\", dut.out_score.get());                                          |
+------------------------------------------------------------------------------------+

> ◈ The Python binding enables direct integration with ML training frameworks. A PyTorch or JAX training loop can call the Arch hardware simulation on every forward pass to verify numerical equivalence between software and hardware implementations --- without leaving the Python environment.

**20.7 Assertion and Coverage Runtime**

All assert and cover properties declared in Arch source are active in the simulation binary with no additional instrumentation step. Failed assertions terminate the simulation with a precise error report including the cycle number, signal values, and the source location of the failing assertion.

+---------------------------------------------------------------------------------+
| *assert_coverage_output*                                                        |
|                                                                                 |
| // At simulation runtime, each assert becomes a native conditional halt:        |
|                                                                                 |
| //                                                                              |
|                                                                                 |
| // cycle 4721:                                                                  |
|                                                                                 |
| // ASSERTION FAILED: AttentionUnit.assert score_in_range                        |
|                                                                                 |
| // Location: attention_unit.arch:81                                             |
|                                                                                 |
| // Condition: Scale.scaled \>= -32767 and Scale.scaled \<= 32767                |
|                                                                                 |
| // Actual: Scale.scaled = -34201                                                |
|                                                                                 |
| // Signal dump:                                                                 |
|                                                                                 |
| // DotProduct.acc = -274008                                                     |
|                                                                                 |
| // Scale.RECIP_SQRT = 32                                                        |
|                                                                                 |
| // Scale.scaled = -34201 ← FAIL                                                 |
|                                                                                 |
| //                                                                              |
|                                                                                 |
| // arch sim outputs a waveform snapshot of the 100 cycles preceding the failure |
|                                                                                 |
| // automatically when an assertion fires --- no manual waveform setup needed.   |
|                                                                                 |
| // Coverage report --- generated at end of simulation run:                      |
|                                                                                 |
| //                                                                              |
|                                                                                 |
| // COVERAGE SUMMARY: AttentionUnit                                              |
|                                                                                 |
| // ┌─────────────────────────┬────────┬────────┬───────────┐                    |
|                                                                                 |
| // │ Property │ Kind │ Count │ Status │                                         |
|                                                                                 |
| // ├─────────────────────────┼────────┼────────┼───────────┤                    |
|                                                                                 |
| // │ score_in_range │ assert │ 10000 │ PASS │                                   |
|                                                                                 |
| // │ softmax_nonzero │ assert │ 10000 │ PASS │                                  |
|                                                                                 |
| // │ pipeline_full │ cover │ 147 │ HIT ✓ │                                      |
|                                                                                 |
| // │ push_when_full │ cover │ 0 │ MISS ✗ │ ← not exercised                      |
|                                                                                 |
| // └─────────────────────────┴────────┴────────┴───────────┘                    |
|                                                                                 |
| // Coverage: 75.0% (3/4 properties hit)                                         |
+---------------------------------------------------------------------------------+

> ◈ ✓ A missed cover property is a warning, not an error. It tells the designer their test suite does not exercise a declared important scenario --- exactly the information needed to improve test quality.

**20.8 Simulation Performance Model**

Because Arch\'s execution model compiles to a straight-line native loop with no dynamic dispatch, its simulation throughput is competitive with Verilator --- currently the fastest open-source HDL simulator. The performance advantage over event-driven simulators grows with design size.

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Simulator**                    **Execution Model**                                     **Relative Speed**   **Overhead Source**
  -------------------------------- ------------------------------------------------------- -------------------- -------------------------------------------------------------------
  **VCS / Questa (interpreted)**   Event-driven, 4-state logic, dynamic scheduling         1× (baseline)        Event queue, X/Z resolution, always-block scheduler

  **VCS / Questa (compiled)**      Compiled C, but still 4-state and event-driven          3--8×                4-state kernel, residual event overhead

  **Verilator**                    Cycle-accurate compiled C++, 2-state                    10--50×              C++ ABI overhead, conservative eval ordering

  **Arch native sim**              Cycle-accurate compiled LLVM IR, 2-state, pre-ordered   15--60×              Near-zero: straight-line loop, static order, no GC

  **Arch native sim (SIMD)**       Vectorised LLVM IR using AVX-512 / NEON                 50--200×             Only for designs with wide Vec\<\> operations --- AI accelerators
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

> *⚑ SIMD vectorisation is automatic for designs that operate on Vec\<SInt\<8\>,N\> or Vec\<UInt\<N\>,M\> types --- exactly the types used in systolic arrays and attention units. The compiler maps these to native SIMD intrinsics when targeting x86-64 with AVX-512 or ARM with NEON.*

**20.9 arch sim Command**

+-----------------------------------------------------------------------+
| *arch_sim_commands*                                                   |
|                                                                       |
| \# Compile **and** run a testbench                                    |
|                                                                       |
| arch sim AttnUnitTb.arch                                              |
|                                                                       |
| \# Compile only (produce binary, do **not** run)                      |
|                                                                       |
| arch sim \--compile-only \--output attn_tb.bin AttnUnitTb.arch        |
|                                                                       |
| \# Run **with** waveform output                                       |
|                                                                       |
| arch sim \--wave attn_tb.fst AttnUnitTb.arch                          |
|                                                                       |
| \# Run **for** a fixed number **of** cycles                           |
|                                                                       |
| arch sim \--cycles 100000 AttnUnitTb.arch                             |
|                                                                       |
| \# Run **with** code coverage **of** Arch source                      |
|                                                                       |
| arch sim \--coverage AttnUnitTb.arch                                  |
|                                                                       |
| arch coverage report \--format html                                   |
|                                                                       |
| \# Enable SIMD vectorisation (auto-detected; explicit flag forces it) |
|                                                                       |
| arch sim \--simd avx512 AttnUnitTb.arch                               |
|                                                                       |
| \# Produce a C header **for** FFI integration                         |
|                                                                       |
| arch sim \--export-c \--output attn_unit_sim.h AttentionUnit.arch     |
|                                                                       |
| \# Produce a Python binding                                           |
|                                                                       |
| arch sim \--export-python \--output arch_sim.py AttentionUnit.arch    |
|                                                                       |
| \# Produce a Rust crate                                               |
|                                                                       |
| arch sim \--export-rust \--crate-name arch-attn AttentionUnit.arch    |
|                                                                       |
| \# Interactive step-by-step simulation (REPL)                         |
|                                                                       |
| arch sim \--interactive AttnUnitTb.arch                               |
|                                                                       |
| arch\> step 10 \# advance 10 cycles                                   |
|                                                                       |
| arch\> print dut.Scale.scaled                                         |
|                                                                       |
| arch\> break **when** dut.out_valid == true                           |
|                                                                       |
| arch\> continue                                                       |
|                                                                       |
| arch\> wave dump last 50 \# dump 50-cycle waveform snapshot           |
+-----------------------------------------------------------------------+

**20.10 Comparison with Existing Simulation Flows**

  ----------------------------------------------------------------------------------------------------------------------------------------------
  **Capability**                       **Arch Native Sim**             **Verilator**           **VCS / Questa**             **Icarus Verilog**
  ------------------------------------ ------------------------------- ----------------------- ---------------------------- --------------------
  **Requires external tool install**   No --- self-contained binary    Yes --- Verilator       Yes --- commercial licence   Yes --- Icarus

  **Simulation language**              Arch testbench (first-class)    C++ testbench wrapper   SystemVerilog/SV-UVM         Verilog/SV

  **Python integration**               Native auto-generated binding   Via verilator-python    Via VPI (slow)               Via vpipy (slow)

  **Assertion evaluation**             Native --- no overhead          Native                  Native                       Interpreted

  **Waveform output**                  FST (optional, default off)     VCD/FST                 VCD/FSDB                     VCD

  **SIMD vectorisation**               Automatic for Vec\<\> types     Manual with DPI         No                           No

  **Interactive debugger**             Built-in REPL                   GDB on C++              Simulator GUI                No

  **Formal verification target**       Same source → arch formal       Separate sby flow       Jasper/VC Formal             No

  **Licence cost**                     Free (open compiler)            Free                    \$10k--\$500k/year           Free
  ----------------------------------------------------------------------------------------------------------------------------------------------

**21. Parallel Simulation --- Multi-Core Native Execution**

Arch supports parallel cycle-accurate simulation across all available CPU cores. Three levels of parallelism are exploited simultaneously, each enabled by structural invariants that Arch enforces at compile time:

- **DAG-level parallelism:** independent nodes in the same topological level execute concurrently within a single clock cycle.

- **Module-level parallelism:** independent module instances with no intra-cycle data dependency execute on separate threads.

- **Domain-level parallelism:** separate clock domains run on dedicated threads, synchronising only at CDC crossing points and at common clock edges.

These three levels are not in conflict --- the compiler stacks them. A design with 4 clock domains and 16 modules per domain can potentially exploit all three levels simultaneously, distributing work across dozens of cores.

**21.1 Why Parallel Simulation is Hard for SystemVerilog --- and Easy for Arch**

  -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Problem**                      **SystemVerilog**                                                               **Arch**
  -------------------------------- ------------------------------------------------------------------------------- --------------------------------------------------------------------------------
  **Dependency graph**             Dynamic --- known only at runtime; any always block can read/write any signal   Static --- full DAG pre-computed at compile time from single-driver rule

  **Write conflicts**              Any signal can have multiple always block writers --- requires locking          Single-driver rule: exactly one writer per signal --- zero locking needed

  **Evaluation ordering**          Non-deterministic across always blocks --- simulator-defined                    Topological order fixed at compile time --- deterministic on all thread counts

  **Delta cycles**                 Each delta may create new events --- feedback requires convergence detection    No combinational loops --- DAG is acyclic --- single pass per level

  **Clock domain boundaries**      Implicit --- clocks are just signals; no declared partition points              Explicit --- every Clock\<D\> declaration is a partition candidate

  **CDC synchronisation points**   Unknown until runtime --- requires event queue                                  Statically known from domain freq_mhz and crossing block declarations

  **Determinism guarantee**        Only with +define+SIM_DETERMINISTIC and restricted coding style                 Structural guarantee --- identical results regardless of thread count
  -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**21.2 Level 1 --- DAG-Level (Intra-Cycle) Parallelism**

Within a single clock cycle, the combinational evaluation DAG is partitioned into levels. All nodes at the same level have no dependencies on each other --- they read only from nodes at lower levels or from register state. Nodes at the same level can execute in parallel on separate cores with no synchronisation.

+------------------------------------------------------------------------------------------+
| *dag_levels_comment*                                                                     |
|                                                                                          |
| // Example: attention unit combinational cone                                            |
|                                                                                          |
| // The compiler assigns each node a DAG level at compile time.                           |
|                                                                                          |
| // Nodes at the same level are fully independent.                                        |
|                                                                                          |
| // Level 0 --- reads only from registers or ports (no comb dependency)                   |
|                                                                                          |
| // pe\[0\].a_in, pe\[1\].a_in, pe\[2\].a_in, pe\[3\].a_in                                |
|                                                                                          |
| // pe\[0\].b_in, pe\[1\].b_in, pe\[2\].b_in, pe\[3\].b_in                                |
|                                                                                          |
| // q\[0..63\], k\[0..63\] (from RAM read, registered)                                    |
|                                                                                          |
| // Level 1 --- depends only on level-0 nodes                                             |
|                                                                                          |
| // pe\[0\].product = pe\[0\].a_in \* pe\[0\].b_in ← parallel with pe\[1..3\]             |
|                                                                                          |
| // pe\[1\].product = pe\[1\].a_in \* pe\[1\].b_in                                        |
|                                                                                          |
| // pe\[2\].product = pe\[2\].a_in \* pe\[2\].b_in                                        |
|                                                                                          |
| // pe\[3\].product = pe\[3\].a_in \* pe\[3\].b_in                                        |
|                                                                                          |
| // dot_partial\[0\] = q\[0\] \* k\[0\] ← parallel with above                             |
|                                                                                          |
| // dot_partial\[1\] = q\[1\] \* k\[1\]                                                   |
|                                                                                          |
| // \... (64 independent multiplies at level 1)                                           |
|                                                                                          |
| // Level 2 --- depends on level-1 nodes                                                  |
|                                                                                          |
| // pe\[0\].sum_out = pe\[0\].sum_in + pe\[0\].product ← chain dependency                 |
|                                                                                          |
| // dot_acc = reduce_add(dot_partial\[0..63\]) ← tree reduction                           |
|                                                                                          |
| // Level 3                                                                               |
|                                                                                          |
| // pe\[1\].sum_out = pe\[1\].sum_in + pe\[1\].product (pe\[1\].sum_in = pe\[0\].sum_out) |
|                                                                                          |
| // scaled = dot_acc \* RECIP_SQRT \>\> 8                                                 |
|                                                                                          |
| // The generated parallel kernel dispatches each level to a thread pool.                 |
|                                                                                          |
| // Threads at level N+1 start only after all level-N threads complete.                   |
|                                                                                          |
| // This is a barrier per DAG level --- typically 3--8 barriers per cycle.                |
+------------------------------------------------------------------------------------------+

> ◈ The number of DAG levels (and therefore the number of barriers per cycle) is printed by arch sim \--stats. Flat designs with wide parallel datapaths (like systolic arrays) typically have 4--6 levels. Deeply chained designs may have more but each level has fewer nodes.

**21.3 Level 2 --- Module-Level Parallelism**

Module instances that are independent within a cycle --- no signal flows from one to the other within that cycle --- are assigned to separate worker threads. The compiler computes the inter-module dependency graph at elaboration time and partitions modules into independent groups.

+-------------------------------------------------------------------------------+
| *module_parallel.arch*                                                        |
|                                                                               |
| // A top-level AI accelerator with independent processing lanes               |
|                                                                               |
| **module** AcceleratorTop                                                     |
|                                                                               |
| **param** NUM_LANES: **const** = 8;                                           |
|                                                                               |
| **port** clk: **in** Clock\<SysDomain\>;                                      |
|                                                                               |
| **port** rst: **in** Reset\<Sync\>;                                           |
|                                                                               |
| // 8 independent attention heads --- no intra-cycle data flow between them    |
|                                                                               |
| // The compiler identifies these as independent and assigns each to a thread. |
|                                                                               |
| generate **for** i **in** 0..NUM_LANES-1                                      |
|                                                                               |
| **inst** head\[i\]: AttentionUnit                                             |
|                                                                               |
| **param** D_K = 64;                                                           |
|                                                                               |
| **param** SEQ_LEN = 2048;                                                     |
|                                                                               |
| **clk \<- clk;                                                      |
|                                                                               |
| **rst \<- rst;                                                      |
|                                                                               |
| **in_valid \<- head_in_valid\[i\];                                  |
|                                                                               |
| **in_token \<- head_in_token\[i\];                                  |
|                                                                               |
| **out_ready \<- head_out_ready\[i\];                                |
|                                                                               |
| **out_score -\> head_out_score\[i\];                                |
|                                                                               |
| **end** **inst** head\[i\]                                                    |
|                                                                               |
| **end** generate **for** i                                                    |
|                                                                               |
| // Aggregator --- depends on all head outputs                                 |
|                                                                               |
| // Assigned to its own thread; starts after all head\[i\] threads complete    |
|                                                                               |
| **inst** agg: ScoreAggregator                                                 |
|                                                                               |
| **param** N = NUM_LANES;                                                      |
|                                                                               |
| generate **for** i **in** 0..NUM_LANES-1                                      |
|                                                                               |
| **score_in\[i\] \<- head_out_score\[i\];                            |
|                                                                               |
| **end** generate **for** i                                                    |
|                                                                               |
| **end** **inst** agg                                                          |
|                                                                               |
| **end** **module** AcceleratorTop                                             |
|                                                                               |
| // Compiler partitions the module graph:                                      |
|                                                                               |
| //                                                                            |
|                                                                               |
| // Thread 0: head\[0\] ─┐                                                     |
|                                                                               |
| // Thread 1: head\[1\] ─┤                                                     |
|                                                                               |
| // Thread 2: head\[2\] ─┤ barrier → Thread 0: agg                             |
|                                                                               |
| // Thread 3: head\[3\] ─┤                                                     |
|                                                                               |
| // \... ─┤                                                                    |
|                                                                               |
| // Thread 7: head\[7\] ─┘                                                     |
|                                                                               |
| //                                                                            |
|                                                                               |
| // All 8 head instances run in parallel.                                      |
|                                                                               |
| // agg runs after all heads complete --- data dependency enforced by barrier. |
+-------------------------------------------------------------------------------+

**21.4 Level 3 --- Domain-Level Parallelism**

Different clock domains run on dedicated threads. Each domain thread advances through time at its own pace. Threads synchronise only when they reach a common time point (e.g. both have a rising edge at t=8ns) or when a CDC crossing is about to transfer data. This is the coarsest and most efficient form of parallelism --- domain threads may run thousands of cycles before needing to synchronise.

+-----------------------------------------------------------------------------------------+
| *domain_parallel.arch*                                                                  |
|                                                                                         |
| // Multi-domain AI system: compute core + memory interface + PCIe link                  |
|                                                                                         |
| **domain** ComputeDomain { freq_mhz: 500 **end** **domain** ComputeDomain // 2ns period |
|                                                                                         |
| **domain** HbmDomain { freq_mhz: 300 **end** **domain** HbmDomain // 3.33ns period      |
|                                                                                         |
| **domain** PcieDomain { freq_mhz: 250 **end** **domain** PcieDomain // 4ns period       |
|                                                                                         |
| // LCM(2, 3.33, 4) ≈ 20ns --- threads synchronise every 20ns of sim time.               |
|                                                                                         |
| // ComputeDomain runs 10 cycles, HbmDomain runs 6 cycles, PcieDomain runs 5 cycles      |
|                                                                                         |
| // before the next common synchronisation point.                                        |
|                                                                                         |
| // Between synchronisation points, each domain thread runs independently:               |
|                                                                                         |
| //                                                                                      |
|                                                                                         |
| // Core 0: ComputeDomain thread                                                         |
|                                                                                         |
| // t=0: cycle 1                                                                         |
|                                                                                         |
| // t=2: cycle 2                                                                         |
|                                                                                         |
| // t=4: cycle 3 ← CDC transfer to HbmDomain scheduled at t=4                            |
|                                                                                         |
| // t=4: \[post CDC data to shared buffer; HbmDomain thread reads it at t=6.67\]         |
|                                                                                         |
| // t=6: cycle 4                                                                         |
|                                                                                         |
| // \...                                                                                 |
|                                                                                         |
| //                                                                                      |
|                                                                                         |
| // Core 1: HbmDomain thread                                                             |
|                                                                                         |
| // t=0: cycle 1                                                                         |
|                                                                                         |
| // t=3.33: cycle 2                                                                      |
|                                                                                         |
| // t=6.67: cycle 3 ← reads CDC data posted by ComputeDomain at t=4                      |
|                                                                                         |
| // \...                                                                                 |
|                                                                                         |
| //                                                                                      |
|                                                                                         |
| // Core 2: PcieDomain thread                                                            |
|                                                                                         |
| // t=0: cycle 1                                                                         |
|                                                                                         |
| // t=4: cycle 2                                                                         |
|                                                                                         |
| // t=8: cycle 3                                                                         |
|                                                                                         |
| // \...                                                                                 |
|                                                                                         |
| // CDC crossing --- the declared crossing block defines the synchronisation protocol    |
|                                                                                         |
| **crossing** compute_to_hbm                                                             |
|                                                                                         |
| **from**: ComputeDomain,                                                                |
|                                                                                         |
| **to**: HbmDomain,                                                                      |
|                                                                                         |
| **sync**: two_flop,                                                                     |
|                                                                                         |
| data: compute_req -\> hbm_req,                                                          |
|                                                                                         |
| **end** **crossing** compute_to_hbm                                                     |
|                                                                                         |
| // The compiler generates a lock-free SPSC queue between the two domain threads.        |
|                                                                                         |
| // ComputeDomain pushes; HbmDomain polls at its own clock rate.                         |
|                                                                                         |
| // No global lock needed --- producer and consumer are on separate cores.               |
+-----------------------------------------------------------------------------------------+

> *⚑ The synchronisation point schedule --- when domain threads must rendezvous --- is pre-computed from the domain frequency declarations at compile time. It is a static sorted array, not a dynamic event queue. No global simulation clock exists at runtime; each thread maintains its own local time and advances until the next rendezvous.*

**21.5 The Parallel Simulation Scheduler**

The runtime parallel scheduler has three components, all generated at compile time with no dynamic allocation:

  ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Component**           **Implementation**                                                       **Role**
  ----------------------- ------------------------------------------------------------------------ ---------------------------------------------------------------------------------
  **Thread pool**         Fixed-size OS thread pool, one thread per physical core (configurable)   Executes DAG-level and module-level work items

  **DAG barrier**         Cache-line-padded atomic counter per DAG level                           Enforces level ordering within a cycle; threads decrement on completion

  **Domain rendezvous**   Static rendezvous table: (sim_time, domain_set) pairs                    Defines when domain threads must synchronise; computed from LCM of all freq_mhz

  **CDC channel**         Lock-free single-producer single-consumer ring buffer per crossing       Transfers data between domain threads without a global lock

  **Work queue**          Per-thread work-stealing deque                                           Balances load across cores when module partitions are uneven
  ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**21.6 Enabling Parallel Simulation**

+-----------------------------------------------------------------------------------------+
| *parallel_commands*                                                                     |
|                                                                                         |
| \# Auto-detect core count **and** use all available cores                               |
|                                                                                         |
| arch sim \--parallel AttnUnitTb.arch                                                    |
|                                                                                         |
| \# Explicit core count                                                                  |
|                                                                                         |
| arch sim \--parallel \--cores 16 AttnUnitTb.arch                                        |
|                                                                                         |
| \# Domain-level parallelism only (safest; recommended **for** first run)                |
|                                                                                         |
| arch sim \--parallel \--strategy **domain** AttnUnitTb.arch                             |
|                                                                                         |
| \# All three levels (maximum throughput)                                                |
|                                                                                         |
| arch sim \--parallel \--strategy all AttnUnitTb.arch                                    |
|                                                                                         |
| \# Show parallelism analysis --- partition report before running                        |
|                                                                                         |
| arch sim \--parallel \--report-partition AttnUnitTb.arch                                |
|                                                                                         |
| \# Pin **domain** threads **to** specific cores (NUMA-aware)                            |
|                                                                                         |
| arch sim \--parallel \--pin ComputeDomain:0,HbmDomain:1,PcieDomain:2 AttnUnitTb.arch    |
|                                                                                         |
| \# Profile parallel efficiency (prints per-core utilisation **and** barrier wait times) |
|                                                                                         |
| arch sim \--parallel \--profile AttnUnitTb.arch                                         |
|                                                                                         |
| \# Output example:                                                                      |
|                                                                                         |
| \# Core 0 (ComputeDomain): 94.2% active, 5.8% waiting at barriers                       |
|                                                                                         |
| \# Core 1 (HbmDomain): 87.1% active, 12.9% waiting at barriers                          |
|                                                                                         |
| \# Core 2 (PcieDomain): 61.4% active, 38.6% waiting at barriers                         |
|                                                                                         |
| \# Core 3--7 (**module** pool): 91.3% active avg                                        |
|                                                                                         |
| \# Parallel efficiency: 88.6% (ideal: 100%)                                             |
|                                                                                         |
| \# Bottleneck: PcieDomain --- consider splitting into 2 subdomains                      |
+-----------------------------------------------------------------------------------------+

**21.7 Partition Report**

The \--report-partition flag prints the full parallel decomposition before running, so the designer can understand and tune the partition before committing to a long simulation run.

+--------------------------------------------------------------------------+
| *partition_report*                                                       |
|                                                                          |
| \$ arch sim \--parallel \--report-partition AcceleratorTop.arch          |
|                                                                          |
| PARALLEL SIMULATION PARTITION REPORT                                     |
|                                                                          |
| ======================================                                   |
|                                                                          |
| Clock Domains (**domain**-level threads):                                |
|                                                                          |
| ComputeDomain 500 MHz Core 0                                             |
|                                                                          |
| HbmDomain 300 MHz Core 1                                                 |
|                                                                          |
| PcieDomain 250 MHz Core 2                                                |
|                                                                          |
| Rendezvous period: 20 ns (10/6/5 cycles respectively)                    |
|                                                                          |
| Module Partitions (**module**-level threads within ComputeDomain):       |
|                                                                          |
| Group A (parallel): head\[0\], head\[1\], head\[2\], head\[3\]           |
|                                                                          |
| head\[4\], head\[5\], head\[6\], head\[7\]                               |
|                                                                          |
| → Assigned **to** Core 0 thread pool (8 workers)                         |
|                                                                          |
| Group B (sequential after A): agg                                        |
|                                                                          |
| → Assigned **to** Core 0 after barrier                                   |
|                                                                          |
| DAG Levels (intra-cycle, within each **module**):                        |
|                                                                          |
| AttentionUnit: 6 levels                                                  |
|                                                                          |
| Level 0: 32 nodes (register reads, **port** inputs)                      |
|                                                                          |
| Level 1: 64 nodes (element-wise multiply) ← SIMD-vectorised              |
|                                                                          |
| Level 2: 8 nodes (partial reduce)                                        |
|                                                                          |
| Level 3: 4 nodes (**full** reduce, scale)                                |
|                                                                          |
| Level 4: 2 nodes (softmax, value-weight)                                 |
|                                                                          |
| Level 5: 1 node (output mux)                                             |
|                                                                          |
| CDC Channels:                                                            |
|                                                                          |
| compute_to_hbm: SPSC ring, **depth** 4 Core 0 → Core 1                   |
|                                                                          |
| hbm_to_pcie: SPSC ring, **depth** 2 Core 1 → Core 2                      |
|                                                                          |
| Estimated Parallel Efficiency:                                           |
|                                                                          |
| Domain-level: \~88% (PcieDomain underloaded)                             |
|                                                                          |
| Module-level: \~95% (8 balanced head instances)                          |
|                                                                          |
| DAG-level: \~82% (6 levels → 5 barriers per cycle)                       |
|                                                                          |
| Combined: \~70% **on** 11 cores (ideal speedup: 7.7×)                    |
|                                                                          |
| Recommendation:                                                          |
|                                                                          |
| PcieDomain has low utilisation. If PCIe logic can be partitioned,        |
|                                                                          |
| consider splitting into PcieRxDomain **and** PcieTxDomain **to** improve |
|                                                                          |
| **domain**-level balance.                                                |
+--------------------------------------------------------------------------+

**21.8 Determinism Guarantee**

Parallel Arch simulation is guaranteed to produce bit-identical results to sequential simulation, regardless of the number of cores, OS thread scheduling, or hardware memory ordering. This guarantee rests on three structural properties:

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Property**                 **Guarantee**                                                                                                             **Mechanism**
  ---------------------------- ------------------------------------------------------------------------------------------------------------------------- --------------------------------------------------------------------------------------------------------------------------------------------------------
  **No write conflicts**       Two threads never write the same signal in the same cycle                                                                 Single-driver rule enforced at compile time --- provably no shared write sites

  **Read-after-write order**   A thread reading a signal always reads the value from the previous cycle, not from a concurrent write in the same cycle   Register commit barrier: all next-state values computed before any state update; committed atomically after all threads complete their comb evaluation

  **Evaluation order**         The result of every combinational expression is identical regardless of which core evaluates it                           DAG level barriers ensure level N+1 never starts before level N completes on all threads; no speculative reads across levels

  **CDC channel ordering**     Data crosses domain boundaries in the correct cycle-relative order                                                        SPSC ring buffers are sequentially consistent by construction; no reordering possible
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

> ◈ ✓ Determinism is a theorem, not a best-effort property. A simulation that passes on 1 core will produce the same assertion results on 64 cores. This is not true of Verilog parallel simulation, where some tools require +define+NONDETERMINISM_WARNINGS to catch ordering bugs.

**21.9 Performance Scaling**

Achievable speedup depends on the design\'s inherent parallelism --- the ratio of independent work to synchronisation overhead. Designs well-suited to Arch parallel simulation share a common structure: multiple independent processing lanes (attention heads, systolic PE rows, network ports) within each clock domain.

  ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Design Type**                                          **Parallelism Profile**                                      **Expected Speedup (8 cores)**   **Expected Speedup (32 cores)**
  -------------------------------------------------------- ------------------------------------------------------------ -------------------------------- ---------------------------------
  **Single-domain, deeply chained pipeline**               Low --- chain creates DAG depth with few wide levels         1.5--2×                          2--3×

  **Single-domain, wide parallel array (e.g. systolic)**   Medium --- many independent PEs at same DAG level            3--5×                            5--8×

  **Multi-domain with balanced loads**                     High --- domain threads run independently most of the time   5--7×                            10--18×

  **Multi-domain + wide arrays per domain**                Very high --- all three parallelism levels active            6--8×                            15--25×

  **AI accelerator (attention + systolic + HBM)**          Very high --- matches Arch\'s design target                  7--9×                            18--30×
  ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

> *⚑ Speedup beyond the number of clock domains is achieved by module-level and DAG-level parallelism within each domain thread. A 4-domain design on 32 cores can still achieve 20× speedup if each domain has sufficient internal parallelism.*

**21.10 Parallel Simulation vs External Tools**

  -------------------------------------------------------------------------------------------------------------------------------------------------------------------------
                                  **Arch parallel sim**             **Verilator \--threads**                 **VCS \--parallel**            **Cadence Incisive parallel**
  ------------------------------- --------------------------------- ---------------------------------------- ------------------------------ -------------------------------
  **Determinism guarantee**       Structural --- always identical   Best-effort --- non-det warnings exist   Tool-specific flags required   Tool-specific flags required

  **Domain-aware partitioning**   Automatic from Clock\<D\> decls   Manual pragma annotations                Manual partition file          Manual partition file

  **Lock-free CDC channels**      Yes --- SPSC ring per crossing    No --- shared memory with locks          No                             No

  **Partition report**            arch sim \--report-partition      No built-in report                       Separate analysis step         Separate analysis step

  **Work stealing**               Yes --- per-thread deque          Yes                                      Yes                            Yes

  **SIMD + parallel combined**    Yes --- stacked automatically     Manual --- separate \--simd flags        No                             No

  **Speedup on AI designs**       18--30× (32 cores)                5--12× (32 cores)                        8--15× (32 cores)              8--15× (32 cores)
  -------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**22. Transaction Level Modeling (TLM)**

Arch supports Transaction Level Modeling (TLM) as a first-class abstraction layer above RTL. At the TLM level, modules communicate by calling methods on typed interfaces rather than by driving individual signals cycle by cycle. A memory read is membus.read(addr) → data --- one call, one response --- rather than a sequence of valid/ready handshake cycles. TLM enables fast architectural simulation, software/hardware co-simulation, and performance modeling, all from the same Arch source as the RTL implementation.

**22.1 Abstraction Levels in Arch**

  --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Level**                          **Communication Model**                             **Time Model**                                          **Simulation Speed**   **Use When**
  ---------------------------------- --------------------------------------------------- ------------------------------------------------------- ---------------------- ----------------------------------------------------------------------
  **RTL**                            Signal-level: valid/ready handshakes, clock edges   Cycle-accurate                                          Baseline (1×)          Verification, timing closure, formal

  **Approximately-timed TLM (AT)**   Method calls with declared timing annotations       Nanoseconds --- one call may advance sim time by N ns   10--50×                Performance modeling, latency analysis, power estimation

  **Loosely-timed TLM (LT)**         Method calls, timing ignored for speed              Zero-time or coarse quantum                             100--1000×             SW/HW co-simulation, firmware development, architectural exploration
  --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**22.2 Transaction Concurrency --- Blocking, Pipelined, and Out-of-Order**

The most important design decision for a TLM method interface is whether calls block the caller until the response arrives, or whether multiple calls can be in-flight simultaneously. Arch makes this explicit with four method concurrency modes. Getting this right is the difference between a TLM model that accurately represents AXI pipelining and one that degenerates into a serial bus that no real hardware resembles.

**22.2.1 blocking --- Serial, Caller Suspends**

+---------------------------------------------------------------------------+
| *blocking_method.arch*                                                    |
|                                                                           |
| // The caller suspends until the response is received.                    |
|                                                                           |
| // Next call cannot issue until this one completes.                       |
|                                                                           |
| // Correct for: APB, AHB single-transfer, simple memory-mapped registers. |
|                                                                           |
| methods                                                                   |
|                                                                           |
| blocking method **read**(addr: UInt\<32\>) -\> UInt\<64\>                 |
|                                                                           |
| timing: 4 cycles;                                                         |
|                                                                           |
| **end** method **read**                                                   |
|                                                                           |
| **end** methods                                                           |
|                                                                           |
| // In a sequence body --- call 2 cannot start until call 1 returns.       |
|                                                                           |
| // Total time: 4 + 4 = 8 cycles. Serial.                                  |
|                                                                           |
| sequence read_two                                                         |
|                                                                           |
| **let** d0: UInt\<64\> = mem.**read**(0x1000); // suspends 4 cycles       |
|                                                                           |
| **let** d1: UInt\<64\> = mem.**read**(0x2000); // suspends 4 more cycles  |
|                                                                           |
| **end** sequence read_two                                                 |
+---------------------------------------------------------------------------+

**22.2.2 pipelined --- Multiple Outstanding, In-Order Responses**

+-----------------------------------------------------------------------------------+
| *pipelined_method.arch*                                                           |
|                                                                                   |
| // The caller gets a Future\<T\> immediately and can issue more calls.            |
|                                                                                   |
| // Responses arrive in the same order as requests.                                |
|                                                                                   |
| // Correct for: AXI without out-of-order IDs, read-after-read pipelining.         |
|                                                                                   |
| methods                                                                           |
|                                                                                   |
| pipelined method **read**(addr: UInt\<32\>) -\> Future\<UInt\<64\>\>              |
|                                                                                   |
| timing: 4 cycles; // latency from issue to response                               |
|                                                                                   |
| max_outstanding: 8; // AXI AR channel depth                                       |
|                                                                                   |
| **end** method **read**                                                           |
|                                                                                   |
| **end** methods                                                                   |
|                                                                                   |
| // In a sequence body --- all three calls issue immediately (back-to-back),       |
|                                                                                   |
| // then the caller awaits each response in order.                                 |
|                                                                                   |
| // Total time: 4 cycles (all overlap). Pipelined.                                 |
|                                                                                   |
| sequence read_three_pipelined                                                     |
|                                                                                   |
| **let** f0: Future\<UInt\<64\>\> = mem.**read**(0x1000); // issues immediately    |
|                                                                                   |
| **let** f1: Future\<UInt\<64\>\> = mem.**read**(0x2000); // issues immediately    |
|                                                                                   |
| **let** f2: Future\<UInt\<64\>\> = mem.**read**(0x3000); // issues immediately    |
|                                                                                   |
| // Await responses in order                                                       |
|                                                                                   |
| **let** d0: UInt\<64\> = await f0; // waits up to 4 cycles                        |
|                                                                                   |
| **let** d1: UInt\<64\> = await f1; // already done if pipelined correctly         |
|                                                                                   |
| **let** d2: UInt\<64\> = await f2;                                                |
|                                                                                   |
| **end** sequence read_three_pipelined                                             |
|                                                                                   |
| // If more than max_outstanding calls are issued before any response is received, |
|                                                                                   |
| // the (max_outstanding+1)th call blocks until one slot frees --- modelling       |
|                                                                                   |
| // the AXI AR channel handshake stall.                                            |
+-----------------------------------------------------------------------------------+

> ◈ Future\<T\> is a first-class Arch TLM type. It is not a general-purpose async primitive --- it only appears in TLM method return positions and sequence await expressions. The compiler tracks Future lifetimes and emits an error if a Future is awaited after the simulation window in which it was issued.

**22.2.3 out_of_order --- Multiple Outstanding, Any-Order Responses**

+----------------------------------------------------------------------------------------+
| *ooo_method.arch*                                                                      |
|                                                                                        |
| // The caller gets a Token\<T, ID\> immediately.                                       |
|                                                                                        |
| // Responses may arrive in any order; matched to the original call by ID.              |
|                                                                                        |
| // Correct for: full AXI with multiple IDs, out-of-order memory systems.               |
|                                                                                        |
| methods                                                                                |
|                                                                                        |
| out_of_order method **read**(                                                          |
|                                                                                        |
| addr: UInt\<32\>                                                                       |
|                                                                                        |
| ) -\> Token\<UInt\<64\>, id_width: 4\> // 4-bit AXI ID field                           |
|                                                                                        |
| timing: 4..20 cycles; // range: best-case to worst-case latency                        |
|                                                                                        |
| max_outstanding: 16;                                                                   |
|                                                                                        |
| **end** method **read**                                                                |
|                                                                                        |
| **end** methods                                                                        |
|                                                                                        |
| // In a sequence body --- issue multiple reads, await in any order.                    |
|                                                                                        |
| // Responses may return in a different order than requests.                            |
|                                                                                        |
| sequence read_ooo                                                                      |
|                                                                                        |
| **let** t0: Token\<UInt\<64\>, 4\> = mem.**read**(0x1000); // ID assigned by interface |
|                                                                                        |
| **let** t1: Token\<UInt\<64\>, 4\> = mem.**read**(0x2000);                             |
|                                                                                        |
| **let** t2: Token\<UInt\<64\>, 4\> = mem.**read**(0x3000);                             |
|                                                                                        |
| // await_any --- return whichever token completes first                                |
|                                                                                        |
| **match** await_any(t0, t1, t2)                                                        |
|                                                                                        |
| t0 =\> **let** d0: UInt\<64\> = t0.value; // t0 finished first                         |
|                                                                                        |
| t1 =\> **let** d1: UInt\<64\> = t1.value;                                              |
|                                                                                        |
| t2 =\> **let** d2: UInt\<64\> = t2.value;                                              |
|                                                                                        |
| **end** **match**                                                                      |
|                                                                                        |
| // await_all --- wait for all outstanding tokens                                       |
|                                                                                        |
| **let** (d0, d1, d2) = await_all(t0, t1, t2);                                          |
|                                                                                        |
| **end** sequence read_ooo                                                              |
|                                                                                        |
| // Token\<T, ID\> carries the assigned AXI ID automatically.                           |
|                                                                                        |
| // The transactor extracts the ID field from the AXI R channel response                |
|                                                                                        |
| // and routes it back to the correct Token --- no designer involvement.                |
+----------------------------------------------------------------------------------------+

**22.2.4 burst --- One Call, N Responses**

+-----------------------------------------------------------------------------------+
| *burst_method.arch*                                                               |
|                                                                                   |
| // A burst method models AXI INCR bursts natively.                                |
|                                                                                   |
| // One call issues one AR transaction; N data beats return as a Vec.              |
|                                                                                   |
| // This is more accurate than N separate pipelined reads --- it models            |
|                                                                                   |
| // the real AXI burst mechanism: one address, N sequential data beats.            |
|                                                                                   |
| methods                                                                           |
|                                                                                   |
| burst method read_burst(                                                          |
|                                                                                   |
| addr: UInt\<32\>,                                                                 |
|                                                                                   |
| length: UInt\<8\> // AXI ARLEN: 0=1 beat, 255=256 beats                           |
|                                                                                   |
| ) -\> Future\<Vec\<UInt\<64\>, length\>\>                                         |
|                                                                                   |
| timing: 4 + length cycles; // address latency + one beat per cycle                |
|                                                                                   |
| max_outstanding: 4; // max simultaneous burst transactions                        |
|                                                                                   |
| **end** method read_burst                                                         |
|                                                                                   |
| burst method write_burst(                                                         |
|                                                                                   |
| addr: UInt\<32\>,                                                                 |
|                                                                                   |
| data: Vec\<UInt\<64\>, length\>,                                                  |
|                                                                                   |
| strobe: Vec\<UInt\<8\>, length\>                                                  |
|                                                                                   |
| ) -\> Future\<WriteResp\>                                                         |
|                                                                                   |
| timing: 1 + length cycles;                                                        |
|                                                                                   |
| max_outstanding: 4;                                                               |
|                                                                                   |
| **end** method write_burst                                                        |
|                                                                                   |
| **end** methods                                                                   |
|                                                                                   |
| // Burst call: one AR transaction, 16 data beats returned as Vec\<UInt\<64\>,16\> |
|                                                                                   |
| sequence dma_burst_read                                                           |
|                                                                                   |
| **let** f: Future\<Vec\<UInt\<64\>, 16\>\> = mem.read_burst(0x1000, length: 15);  |
|                                                                                   |
| // Issue more work here while burst is in-flight\...                              |
|                                                                                   |
| **let** beats: Vec\<UInt\<64\>, 16\> = await f;                                   |
|                                                                                   |
| // beats\[0\] is the first 64-bit beat, beats\[15\] is the last                   |
|                                                                                   |
| **end** sequence dma_burst_read                                                   |
|                                                                                   |
| // Compare: 16 separate pipelined reads vs 1 burst read                           |
|                                                                                   |
| // Pipelined: 16 AR channel transactions --- bandwidth-inefficient, not accurate  |
|                                                                                   |
| // Burst: 1 AR channel transaction --- matches real AXI behaviour                 |
+-----------------------------------------------------------------------------------+

> *⚑ The length parameter in a burst method declaration is a special form --- it is an implicit UInt\<8\> input parameter that also parameterises the return type Vec\<T, length\>. The compiler infers the Vec length from the call site. This is the only case in Arch where a method parameter influences the return type.*

**22.3 Concurrency Mode Comparison**

  ------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Mode**           **Return Type**              **Caller Blocks?**          **Multiple Outstanding?**       **Response Order**   **Models**
  ------------------ ---------------------------- --------------------------- ------------------------------- -------------------- ---------------------------------
  **blocking**       T directly                   Yes --- until response      No --- one at a time            N/A                  APB, AHB, simple MMIO

  **pipelined**      Future\<T\>                  No --- issues immediately   Yes --- up to max_outstanding   In-order             AXI (no OOO), AMBA AHB burst

  **out_of_order**   Token\<T, ID\>               No --- issues immediately   Yes --- up to max_outstanding   Any order            Full AXI with multiple IDs, DDR

  **burst**          Future\<Vec\<T, length\>\>   No --- issues immediately   Yes --- up to max_outstanding   In-order beats       AXI INCR burst, HBM row access
  ------------------------------------------------------------------------------------------------------------------------------------------------------------------

**22.4 Full AXI4 Interface Declaration**

Using all four concurrency modes, the AXI4 read and write channels can be modeled with full fidelity --- including ID-based out-of-order completion, byte strobes, burst types, and response codes.

+----------------------------------------------------------------------+
| *axi4_interface.arch*                                                |
|                                                                      |
| **bus** Axi4                                                   |
|                                                                      |
| **param** ADDR_W: **const** = 32;                                    |
|                                                                      |
| **param** DATA_W: **const** = 64;                                    |
|                                                                      |
| **param** ID_W: **const** = 4;                                       |
|                                                                      |
| // ── RTL signals (present at RTL level) ────────────────────────    |
|                                                                      |
| // AW channel                                                        |
|                                                                      |
| **port** awvalid: **out** Bool; **port** awready: **in** Bool;       |
|                                                                      |
| **port** awaddr: **out** UInt\<ADDR_W\>;                             |
|                                                                      |
| **port** awid: **out** UInt\<ID_W\>;                                 |
|                                                                      |
| **port** awlen: **out** UInt\<8\>;                                   |
|                                                                      |
| **port** awsize: **out** UInt\<3\>;                                  |
|                                                                      |
| **port** awburst: **out** UInt\<2\>;                                 |
|                                                                      |
| // W channel                                                         |
|                                                                      |
| **port** wvalid: **out** Bool; **port** wready: **in** Bool;         |
|                                                                      |
| **port** wdata: **out** UInt\<DATA_W\>;                              |
|                                                                      |
| **port** wstrb: **out** UInt\<DATA_W/8\>;                            |
|                                                                      |
| **port** wlast: **out** Bool;                                        |
|                                                                      |
| // B channel                                                         |
|                                                                      |
| **port** bvalid: **in** Bool; **port** bready: **out** Bool;         |
|                                                                      |
| **port** bresp: **in** UInt\<2\>; **port** bid: **in** UInt\<ID_W\>; |
|                                                                      |
| // AR channel                                                        |
|                                                                      |
| **port** arvalid: **out** Bool; **port** arready: **in** Bool;       |
|                                                                      |
| **port** araddr: **out** UInt\<ADDR_W\>;                             |
|                                                                      |
| **port** arid: **out** UInt\<ID_W\>;                                 |
|                                                                      |
| **port** arlen: **out** UInt\<8\>;                                   |
|                                                                      |
| **port** arsize: **out** UInt\<3\>;                                  |
|                                                                      |
| **port** arburst: **out** UInt\<2\>;                                 |
|                                                                      |
| // R channel                                                         |
|                                                                      |
| **port** rvalid: **in** Bool; **port** rready: **out** Bool;         |
|                                                                      |
| **port** rdata: **in** UInt\<DATA_W\>;                               |
|                                                                      |
| **port** rresp: **in** UInt\<2\>;                                    |
|                                                                      |
| **port** rid: **in** UInt\<ID_W\>;                                   |
|                                                                      |
| **port** rlast: **in** Bool;                                         |
|                                                                      |
| // ── TLM methods (present at TLM level) ────────────────────────    |
|                                                                      |
| methods                                                              |
|                                                                      |
| // Single read --- pipelined, in-order, up to 16 outstanding         |
|                                                                      |
| pipelined method **read**(                                           |
|                                                                      |
| addr: UInt\<ADDR_W\>                                                 |
|                                                                      |
| ) -\> Future\<UInt\<DATA_W\>\>                                       |
|                                                                      |
| timing: 4 cycles;                                                    |
|                                                                      |
| max_outstanding: 16;                                                 |
|                                                                      |
| **end** method **read**                                              |
|                                                                      |
| // Burst read --- one AR transaction, up to 256 data beats           |
|                                                                      |
| burst method read_burst(                                             |
|                                                                      |
| addr: UInt\<ADDR_W\>,                                                |
|                                                                      |
| length: UInt\<8\>                                                    |
|                                                                      |
| ) -\> Future\<Vec\<UInt\<DATA_W\>, length\>\>                        |
|                                                                      |
| timing: 4 + length cycles;                                           |
|                                                                      |
| max_outstanding: 4;                                                  |
|                                                                      |
| **end** method read_burst                                            |
|                                                                      |
| // Out-of-order read --- ID-tagged, responses may reorder            |
|                                                                      |
| out_of_order method read_ooo(                                        |
|                                                                      |
| addr: UInt\<ADDR_W\>                                                 |
|                                                                      |
| ) -\> Token\<UInt\<DATA_W\>, id_width: ID_W\>                        |
|                                                                      |
| timing: 4..20 cycles;                                                |
|                                                                      |
| max_outstanding: 16;                                                 |
|                                                                      |
| **end** method read_ooo                                              |
|                                                                      |
| // Single write --- pipelined, in-order                              |
|                                                                      |
| pipelined method **write**(                                          |
|                                                                      |
| addr: UInt\<ADDR_W\>,                                                |
|                                                                      |
| data: UInt\<DATA_W\>,                                                |
|                                                                      |
| strobe: UInt\<DATA_W/8\>                                             |
|                                                                      |
| ) -\> Future\<AxiResp\>                                              |
|                                                                      |
| timing: 2 cycles;                                                    |
|                                                                      |
| max_outstanding: 16;                                                 |
|                                                                      |
| **end** method **write**                                             |
|                                                                      |
| // Burst write --- one AW transaction, N data beats                  |
|                                                                      |
| burst method write_burst(                                            |
|                                                                      |
| addr: UInt\<ADDR_W\>,                                                |
|                                                                      |
| data: Vec\<UInt\<DATA_W\>, length\>,                                 |
|                                                                      |
| strobe: Vec\<UInt\<DATA_W/8\>, length\>,                             |
|                                                                      |
| length: UInt\<8\>                                                    |
|                                                                      |
| ) -\> Future\<AxiResp\>                                              |
|                                                                      |
| timing: 2 + length cycles;                                           |
|                                                                      |
| max_outstanding: 4;                                                  |
|                                                                      |
| **end** method write_burst                                           |
|                                                                      |
| **end** methods                                                      |
|                                                                      |
| **end** **bus** Axi4                                           |
|                                                                      |
| **struct** AxiResp                                                   |
|                                                                      |
| ok: Bool,                                                            |
|                                                                      |
| resp: UInt\<2\>, // OKAY=0 EXOKAY=1 SLVERR=2 DECERR=3                |
|                                                                      |
| id: UInt\<4\>,                                                       |
|                                                                      |
| **end** **struct** AxiResp                                           |
+----------------------------------------------------------------------+

**22.5 Using the AXI4 Interface --- Initiator Examples**

+---------------------------------------------------------------------------------+
| *dma_axi4_initiator.arch*                                                       |
|                                                                                 |
| // DMA engine using all three read modes                                        |
|                                                                                 |
| **module** DmaEngine                                                            |
|                                                                                 |
| socket axi: initiator Axi4\<ADDR_W 32, DATA_W 64, ID_W 4\>;                     |
|                                                                                 |
| **port** clk: **in** Clock\<SysDomain\>;                                        |
|                                                                                 |
| **port** rst: **in** Reset\<Sync\>;                                             |
|                                                                                 |
| // ── Sequential read --- blocking, one at a time ──────────────────            |
|                                                                                 |
| sequence config_read                                                            |
|                                                                                 |
| // Read 4 config registers --- serial, total latency: 16 cycles                 |
|                                                                                 |
| **let** ctrl: UInt\<64\> = await axi.**read**(0xF000_0000);                     |
|                                                                                 |
| **let** stat: UInt\<64\> = await axi.**read**(0xF000_0008);                     |
|                                                                                 |
| **let** base: UInt\<64\> = await axi.**read**(0xF000_0010);                     |
|                                                                                 |
| **let** limit: UInt\<64\> = await axi.**read**(0xF000_0018);                    |
|                                                                                 |
| // Note: using await directly on a pipelined method forces serial behaviour.    |
|                                                                                 |
| // For maximum throughput, issue all futures first (see below).                 |
|                                                                                 |
| **end** sequence config_read                                                    |
|                                                                                 |
| // ── Pipelined read --- all 4 issued immediately, overlap ─────────            |
|                                                                                 |
| sequence config_read_fast                                                       |
|                                                                                 |
| // All 4 issue in the same cycle --- total latency: \~4 cycles not 16           |
|                                                                                 |
| **let** f_ctrl = axi.**read**(0xF000_0000);                                     |
|                                                                                 |
| **let** f_stat = axi.**read**(0xF000_0008);                                     |
|                                                                                 |
| **let** f_base = axi.**read**(0xF000_0010);                                     |
|                                                                                 |
| **let** f_limit = axi.**read**(0xF000_0018);                                    |
|                                                                                 |
| **let** (ctrl, stat, base, limit) = await_all(f_ctrl, f_stat, f_base, f_limit); |
|                                                                                 |
| **end** sequence config_read_fast                                               |
|                                                                                 |
| // ── Burst read --- 64-word cache line fill ───────────────────────            |
|                                                                                 |
| sequence cache_line_fill(addr: UInt\<32\>)                                      |
|                                                                                 |
| // One AR transaction, 64 data beats returned as Vec                            |
|                                                                                 |
| **let** f = axi.read_burst(addr, length: 63);                                   |
|                                                                                 |
| // Issue next transaction or do other work here\...                             |
|                                                                                 |
| **let** line: Vec\<UInt\<64\>, 64\> = await f;                                  |
|                                                                                 |
| // Process line\...                                                             |
|                                                                                 |
| **end** sequence cache_line_fill                                                |
|                                                                                 |
| // ── Out-of-order --- two reads, process whichever returns first ──            |
|                                                                                 |
| sequence speculative_prefetch(addr_a: UInt\<32\>, addr_b: UInt\<32\>)           |
|                                                                                 |
| **let** t_a = axi.read_ooo(addr_a);                                             |
|                                                                                 |
| **let** t_b = axi.read_ooo(addr_b);                                             |
|                                                                                 |
| **match** await_any(t_a, t_b)                                                   |
|                                                                                 |
| t_a =\> process_a(t_a.value); process_b(await t_b);                             |
|                                                                                 |
| t_b =\> process_b(t_b.value); process_a(await t_a);                             |
|                                                                                 |
| **end** **match**                                                               |
|                                                                                 |
| **end** sequence speculative_prefetch                                           |
|                                                                                 |
| **end** **module** DmaEngine                                                    |
+---------------------------------------------------------------------------------+

**22.6 TLM Interface Declaration --- Method Calls**

An Arch bus gains a methods block alongside its signal declarations. The bus serves both roles simultaneously: RTL ports are driven when operating at the RTL abstraction level; TLM methods are called when operating at the TLM level. The arch sim flag selects which.

+--------------------------------------------------------------------+
| *interface_tlm.arch*                                               |
|                                                                    |
| // Annotated bus anatomy                                           |
|                                                                    |
| **bus** MemBusTlm                                            |
|                                                                    |
| **param** ADDR_W: **const** = 32;                                  |
|                                                                    |
| **param** DATA_W: **const** = 64;                                  |
|                                                                    |
| // RTL signals ─ synthesisable, used at RTL level                  |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** **valid**: **out** Bool;                                  |
|                                                                    |
| **port** **ready**: **in** Bool;                                   |
|                                                                    |
| **port** addr: **out** UInt\<ADDR_W\>;                             |
|                                                                    |
| **port** rdata: **in** UInt\<DATA_W\>;                             |
|                                                                    |
| **port** wdata: **out** UInt\<DATA_W\>;                            |
|                                                                    |
| **port** wen: **out** Bool;                                        |
|                                                                    |
| // TLM methods ─ simulation-only, used at TLM level                |
|                                                                    |
| methods                                                            |
|                                                                    |
| blocking method **read**(                                          |
|                                                                    |
| addr: UInt\<ADDR_W\>                                               |
|                                                                    |
| ) -\> UInt\<DATA_W\>                                               |
|                                                                    |
| timing: 10 ns;                                                     |
|                                                                    |
| **end** method **read**                                            |
|                                                                    |
| pipelined method read_pipe(                                        |
|                                                                    |
| addr: UInt\<ADDR_W\>                                               |
|                                                                    |
| ) -\> Future\<UInt\<DATA_W\>\>                                     |
|                                                                    |
| timing: 10 ns;                                                     |
|                                                                    |
| max_outstanding: 8;                                                |
|                                                                    |
| **end** method read_pipe                                           |
|                                                                    |
| non_blocking method **write**(                                     |
|                                                                    |
| addr: UInt\<ADDR_W\>,                                              |
|                                                                    |
| data: UInt\<DATA_W\>,                                              |
|                                                                    |
| strobe: UInt\<DATA_W/8\>                                           |
|                                                                    |
| ) -\> Future\<WriteResp\>                                          |
|                                                                    |
| timing: 10 ns;                                                     |
|                                                                    |
| max_outstanding: 8;                                                |
|                                                                    |
| **end** method **write**                                           |
|                                                                    |
| **end** methods                                                    |
|                                                                    |
| **end** **bus** MemBusTlm                                    |
+--------------------------------------------------------------------+

**22.7 Timing Models --- LT and AT**

+------------------------------------------------------------------------------+
| *timing_models.arch*                                                         |
|                                                                              |
| // Loosely-Timed (LT) --- zero timing, maximum speed                         |
|                                                                              |
| // arch sim \--tlm-lt                                                        |
|                                                                              |
| // All method calls complete in zero simulation time.                        |
|                                                                              |
| // Use for: firmware bring-up, API correctness, SW development.              |
|                                                                              |
| // Approximately-Timed (AT) --- nanosecond timing                            |
|                                                                              |
| // arch sim \--tlm-at                                                        |
|                                                                              |
| // Each method call advances sim time by its declared timing value.          |
|                                                                              |
| // For pipelined/OOO methods, the LATENCY elapses from issue to await.       |
|                                                                              |
| // For blocking methods, the caller is suspended for the duration.           |
|                                                                              |
| // Use for: performance analysis, latency budgeting, throughput measurement. |
|                                                                              |
| // Quantum --- LT with periodic coarse time advance                          |
|                                                                              |
| // arch sim \--tlm-quantum 100ns                                             |
|                                                                              |
| // Sim time advances in 100ns steps regardless of method calls.              |
|                                                                              |
| // Balances speed and temporal accuracy for mixed SW/HW workloads.           |
|                                                                              |
| // Per-method override --- selectively restore timing in LT mode             |
|                                                                              |
| methods                                                                      |
|                                                                              |
| blocking method dram_read(addr: UInt\<32\>) -\> UInt\<64\>                   |
|                                                                              |
| timing: 40 ns;                                                               |
|                                                                              |
| timing_in_lt: true; // honour this latency even in LT mode                   |
|                                                                              |
| **end** method dram_read // useful for DRAM latency-sensitive tests          |
|                                                                              |
| **end** methods                                                              |
+------------------------------------------------------------------------------+

**22.8 Socket Binding and Automatic Transactor**

+--------------------------------------------------------------------------+
| *binding.arch*                                                           |
|                                                                          |
| // One-to-one binding                                                    |
|                                                                          |
| bind dma.axi -\> dram.axi;                                               |
|                                                                          |
| // N-to-1: compiler inserts round-robin TLM arbiter                      |
|                                                                          |
| generate **for** i **in** 0..NUM_CORES-1                                 |
|                                                                          |
| bind core\[i\].axi -\> dram.axi **arbiter**: round_robin; **end** bind   |
|                                                                          |
| **end** generate **for** i                                               |
|                                                                          |
| // 1-to-N: compiler inserts address decoder                              |
|                                                                          |
| bind cpu.axi -\> dram.axi **when** addr **in** 0x0000_0000..0x0FFF_FFFF; |
|                                                                          |
| bind cpu.axi -\> uart.axi **when** addr **in** 0x1000_0000..0x1000_FFFF; |
|                                                                          |
| // RTL ↔ TLM transactor: RTL DMA → TLM DRAM                              |
|                                                                          |
| bind dma.mem_rtl_port -\> dram.axi                                       |
|                                                                          |
| transactor: protocol Axi4Lite;                                           |
|                                                                          |
| **end** bind                                                             |
|                                                                          |
| // Built-in transactor protocols: AXI4, AXI4Lite, AXI4Stream,            |
|                                                                          |
| // AHB, APB, TileLink, Wishbone                                          |
+--------------------------------------------------------------------------+

**22.9 TLM Testbench and Python Co-Simulation**

+------------------------------------------------------------------------------------------+
| *tlm_testbench.arch*                                                                     |
|                                                                                          |
| testbench SocTb                                                                          |
|                                                                                          |
| dut: SocFabric **end** dut                                                               |
|                                                                                          |
| run_for: 1 ms;                                                                           |
|                                                                                          |
| socket tb_master: initiator Axi4\<ADDR_W 32, DATA_W 64, ID_W 4\>;                        |
|                                                                                          |
| bind tb_master -\> dut.cpu.axi;                                                          |
|                                                                                          |
| task init_memory(base: UInt\<32\>, data: Vec\<UInt\<64\>, 256\>)                         |
|                                                                                          |
| // Issue all 256 writes as pipelined futures --- all in-flight simultaneously            |
|                                                                                          |
| **let** futures: Vec\<Future\<AxiResp\>, 256\>;                                          |
|                                                                                          |
| repeat 256 **with** idx                                                                  |
|                                                                                          |
| futures\[idx\] = tb_master.**write**(base + idx\*8, data\[idx\], 8\'hFF);                |
|                                                                                          |
| **end** repeat                                                                           |
|                                                                                          |
| await_all(futures); // wait for all write responses                                      |
|                                                                                          |
| **end** task init_memory                                                                 |
|                                                                                          |
| sequence main                                                                            |
|                                                                                          |
| init_memory(base: 0x0000_0000, data: boot_rom_image);                                    |
|                                                                                          |
| // Burst read back --- one AXI transaction, 256 beats                                    |
|                                                                                          |
| **let** f = tb_master.read_burst(0x0000_0000, length: 255);                              |
|                                                                                          |
| **let** readback: Vec\<UInt\<64\>, 256\> = await f;                                      |
|                                                                                          |
| repeat 256 **with** i                                                                    |
|                                                                                          |
| check readback\[i\] == boot_rom_image\[i\]                                               |
|                                                                                          |
| message \"Mismatch at word {i}\";                                                        |
|                                                                                          |
| **end** repeat                                                                           |
|                                                                                          |
| report \"Memory verified.\";                                                             |
|                                                                                          |
| **end** sequence main                                                                    |
|                                                                                          |
| **end** testbench SocTb                                                                  |
|                                                                                          |
| // ── Python co-simulation (LT mode) ───────────────────────────────────                 |
|                                                                                          |
| // import arch_sim                                                                       |
|                                                                                          |
| // dut = arch_sim.AcceleratorTop(tlm_mode=\'lt\')                                        |
|                                                                                          |
| // dut.reset()                                                                           |
|                                                                                          |
| // \# Pipelined write: 1024 writes issued immediately, futures collected                 |
|                                                                                          |
| // futures = \[dut.axi.write(WEIGHT_BASE + i\*8, int(w)) for i,w in enumerate(weights)\] |
|                                                                                          |
| // dut.await_all(futures)                                                                |
|                                                                                          |
| // \# Burst read: one call, 64 words returned                                            |
|                                                                                          |
| // output = dut.axi.read_burst(OUTPUT_BASE, length=63)                                   |
|                                                                                          |
| // assert list(output) == torch_reference.tolist()                                       |
+------------------------------------------------------------------------------------------+

**22.10 TLM vs RTL --- When to Use Which**

  -----------------------------------------------------------------------------------------------------------------------------------------
  **Use Case**                                      **TLM Level**              **RTL**       **Reason**
  ------------------------------------------------- -------------------------- ------------- ----------------------------------------------
  **Firmware development, driver bring-up**         LT --- 1000× faster        Not needed    SW runs before HW is complete

  **Memory subsystem performance modeling**         AT --- timing annotation   Optional      Latency budgeting needs ns accuracy

  **AXI burst bandwidth measurement**               AT with burst method       Optional      Burst method models AR/R channels accurately

  **Out-of-order reorder buffer verification**      OOO method + AT            Also useful   Token\<ID\> models AXI ID routing accurately

  **Formal verification of protocol correctness**   RTL                        Required      Formal tools operate on signals not methods

  **Gate-level timing verification**                RTL                        Required      Post-synthesis netlist is always RTL

  **ML framework co-simulation (PyTorch/JAX)**      LT                         Optional      Python loops; cycle accuracy irrelevant

  **NoC throughput analysis**                       AT with OOO methods        Optional      Throughput and latency need timing
  -----------------------------------------------------------------------------------------------------------------------------------------

**22.11 Comparison with SystemC TLM-2.0**

  -----------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Feature**                 **Arch TLM**                                                       **SystemC TLM-2.0**
  --------------------------- ------------------------------------------------------------------ ------------------------------------------------------------------------------
  **Concurrency modes**       blocking, pipelined, out_of_order, burst --- explicit per method   blocking transport only; non-blocking transport manual phase machine

  **Pipelined calls**         Future\<T\> return --- issue all, await later                      Manual --- must use nb_transport + phase machine + event notifications

  **Out-of-order calls**      Token\<T,ID\> return --- await_any / await_all                     Manual --- custom ID tracking + payload extension

  **Burst method**            Native: one call → Vec\<T,N\> response                             Not native --- must model as N separate transactions or use custom extension

  **Method call syntax**      axi.read(addr) --- typed, checked at compile time                  b_transport(trans, delay) --- generic payload, cast at runtime

  **Phase machine**           Compiler-generated from concurrency mode + timing                  Hand-coded: BEGIN_REQ/END_REQ/BEGIN_RESP/END_RESP per method

  **await_any / await_all**   Built-in --- compiler generates wait logic                         Manual --- sc_event and wait() combinations

  **Automatic transactor**    bind with transactor: Axi4 --- compiler generates                  Manual adapter: hundreds of lines per protocol

  **Type safety**             Width, ID, length all compile-time checked                         Runtime --- wrong payload size is a simulation assertion
  -----------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**23. Cycle-Accurate TLM --- RTL-Backed Method Implementations**

A TLM method implementation can be backed by actual RTL signal-level logic instead of a nanosecond timing annotation. When it is, calling the method drives real signals and waits for real clock edges --- making the method call cycle-accurate. The timing is no longer declared; it emerges from the actual RTL behavior. The initiator code is identical at all three abstraction levels.

**23.1 The Three Implementation Tiers**

  ----------------------------------------------------------------------------------------------------------------------------------------------------
  **Tier**                  **Keyword**             **Timing Source**     **Speed**   **Accuracy**          **Use When**
  ------------------------- ----------------------- --------------------- ----------- --------------------- ------------------------------------------
  **Loosely-timed**         timing: loosely_timed   Zero --- instant      \~1000×     Functional only       SW bring-up, API testing

  **Approximately-timed**   timing: N ns            Declared annotation   \~50×       Approximate latency   Performance modeling, latency budget

  **RTL-backed**            implement \... rtl      Actual clock cycles   1×          Cycle-accurate        Full verification, timing closure, power
  ----------------------------------------------------------------------------------------------------------------------------------------------------

All three tiers share the same initiator call syntax. arch sim selects the tier at simulation time, or individual modules can be selectively refined to RTL while others remain at TLM.

**23.2 RTL-Backed Method --- Syntax**

An implement block with an rtl body drives actual interface signals and uses wait until expressions to suspend on real clock events. The method completes when the rtl body reaches a return statement or falls through. The number of cycles consumed is determined by how many clock edges the body passes through --- not by any declared annotation.

+-------------------------------------------------------------------------+
| *rtl_backed_impl.arch*                                                  |
|                                                                         |
| // AXI4 memory model --- three tiers for the same read method           |
|                                                                         |
| // ── Tier 1: Loosely-timed ─────────────────────────────────────────   |
|                                                                         |
| implement axi.**read**(addr) -\> Future\<UInt\<64\>\>                   |
|                                                                         |
| timing: loosely_timed;                                                  |
|                                                                         |
| **return** Storage.data\[addr\[27:3\]\]; // instant --- no cycle cost   |
|                                                                         |
| **end** implement axi.**read**                                          |
|                                                                         |
| // ── Tier 2: Approximately-timed ───────────────────────────────────   |
|                                                                         |
| implement axi.**read**(addr) -\> Future\<UInt\<64\>\>                   |
|                                                                         |
| timing: 40 ns; // declared annotation --- approximate                   |
|                                                                         |
| **return** Storage.data\[addr\[27:3\]\];                                |
|                                                                         |
| **end** implement axi.**read**                                          |
|                                                                         |
| // ── Tier 3: RTL-backed --- cycle-accurate ─────────────────────────── |
|                                                                         |
| // Drives actual AR/R channel signals. Timing emerges from clock edges. |
|                                                                         |
| implement axi.**read**(addr) -\> Future\<UInt\<64\>\>                   |
|                                                                         |
| rtl                                                                     |
|                                                                         |
| // Drive AR channel --- assert address and valid                        |
|                                                                         |
| arvalid = true;                                                         |
|                                                                         |
| araddr = addr;                                                          |
|                                                                         |
| arid = alloc_id(); // assign next available AXI ID                      |
|                                                                         |
| arlen = 0; // single beat                                               |
|                                                                         |
| arsize = 3; // 8 bytes                                                  |
|                                                                         |
| arбurst = 2\'b01; // INCR                                               |
|                                                                         |
| // Wait for AR handshake --- actual cycles consumed here                |
|                                                                         |
| wait until arready == true;                                             |
|                                                                         |
| wait 1 cycle;                                                           |
|                                                                         |
| arvalid = false;                                                        |
|                                                                         |
| // Wait for R channel response matching our AXI ID                      |
|                                                                         |
| wait until rvalid == true **and** rid == arid;                          |
|                                                                         |
| **let** data: UInt\<64\> = rdata;                                       |
|                                                                         |
| rready = true;                                                          |
|                                                                         |
| wait 1 cycle;                                                           |
|                                                                         |
| rready = false;                                                         |
|                                                                         |
| free_id(arid);                                                          |
|                                                                         |
| **return** data;                                                        |
|                                                                         |
| **end** rtl                                                             |
|                                                                         |
| **end** implement axi.**read**                                          |
+-------------------------------------------------------------------------+

> ◈ The wait until and wait N cycle expressions inside an rtl body consume real simulation cycles. The method\'s actual latency is the number of cycles between the method call and the return --- determined entirely by the signal behavior of the target it is connected to, not by any annotation.

**23.3 Pipelined and Out-of-Order RTL-Backed Methods**

RTL-backed implementations of pipelined and out_of_order methods model the actual signal-level pipeline naturally. The AR channel handshake and the R channel response are separated --- the method issues the AR channel in the first phase and registers a pending response, then the response phase fires when the R channel data arrives.

+------------------------------------------------------------------------------+
| *rtl_backed_pipeline.arch*                                                   |
|                                                                              |
| // RTL-backed pipelined read --- AXI AR and R channels are separated         |
|                                                                              |
| implement axi.read_pipe(addr) -\> Future\<UInt\<64\>\>                       |
|                                                                              |
| rtl                                                                          |
|                                                                              |
| // Phase 1: issue request on AR channel                                      |
|                                                                              |
| // Returns a Future immediately after AR handshake.                          |
|                                                                              |
| // The Future resolves when R channel data arrives.                          |
|                                                                              |
| arvalid = true;                                                              |
|                                                                              |
| araddr = addr;                                                               |
|                                                                              |
| arid = alloc_id();                                                           |
|                                                                              |
| wait until arready == true; // AR accepted --- may be 0 or more cycles       |
|                                                                              |
| wait 1 cycle;                                                                |
|                                                                              |
| arvalid = false;                                                             |
|                                                                              |
| // Future is now pending --- caller gets control back here                   |
|                                                                              |
| // while we wait for R channel in the background                             |
|                                                                              |
| // Phase 2: wait for R channel (runs concurrently with caller)               |
|                                                                              |
| wait until rvalid == true **and** rid == arid;                               |
|                                                                              |
| **let** data: UInt\<64\> = rdata;                                            |
|                                                                              |
| rready = true;                                                               |
|                                                                              |
| wait 1 cycle;                                                                |
|                                                                              |
| rready = false;                                                              |
|                                                                              |
| free_id(arid);                                                               |
|                                                                              |
| **return** data; // resolves the Future --- caller\'s await unblocks         |
|                                                                              |
| **end** rtl                                                                  |
|                                                                              |
| **end** implement axi.read_pipe                                              |
|                                                                              |
| // The initiator\'s pipelined sequence is unchanged --- same code as before: |
|                                                                              |
| sequence read_four_pipelined                                                 |
|                                                                              |
| // All four ARs issue back-to-back --- real AR channel handshakes            |
|                                                                              |
| **let** f0 = axi.read_pipe(0x1000);                                          |
|                                                                              |
| **let** f1 = axi.read_pipe(0x2000);                                          |
|                                                                              |
| **let** f2 = axi.read_pipe(0x3000);                                          |
|                                                                              |
| **let** f3 = axi.read_pipe(0x4000);                                          |
|                                                                              |
| // Await all R channel responses --- actual cycle count depends on           |
|                                                                              |
| // the target\'s RTL implementation, not on any declared annotation          |
|                                                                              |
| **let** (d0, d1, d2, d3) = await_all(f0, f1, f2, f3);                        |
|                                                                              |
| **end** sequence read_four_pipelined                                         |
+------------------------------------------------------------------------------+

> *⚑ The two-phase structure of the rtl body --- issue phase (before first yield point) and response phase (after) --- directly models the split AR/R channels of AXI. The compiler recognises this pattern and generates the correct coroutine split: the Future is created after the AR handshake; the response phase runs as a background coroutine until the R channel fires.*

**23.4 Gradual Refinement --- Mixed TLM and RTL in One System**

The most powerful use of RTL-backed methods is gradual refinement. A full system starts at LT TLM for maximum simulation speed. Modules are refined to RTL-backed implementations one at a time. The initiator code is identical at every stage --- only the target implementation changes. Modules at different refinement levels coexist in the same simulation; the automatic transactor at each socket boundary handles the level mismatch.

+------------------------------------------------------------------------------------+
| *refinement_stages.arch*                                                           |
|                                                                                    |
| // Refinement ladder --- same system at three stages                               |
|                                                                                    |
| // ── Stage 1: Full LT system --- all methods loosely-timed ───────────            |
|                                                                                    |
| // Speed: \~1000× RTL. Use for: firmware bring-up, API testing.                    |
|                                                                                    |
| **module** Stage1System                                                            |
|                                                                                    |
| **inst** cpu: CpuTlmModel **end** **inst** cpu // LT: instant method calls         |
|                                                                                    |
| **inst** dram: DramLtModel **end** **inst** dram // LT: zero-cycle reads           |
|                                                                                    |
| **inst** noc: NocLtModel **end** **inst** noc // LT: zero-latency routing          |
|                                                                                    |
| bind cpu.mem -\> noc.upstream;                                                     |
|                                                                                    |
| bind noc.**port** -\> dram.mem;                                                    |
|                                                                                    |
| **end** **module** Stage1System                                                    |
|                                                                                    |
| // ── Stage 2: DRAM refined to AT --- memory latency modeled ──────────            |
|                                                                                    |
| // Speed: \~50× RTL. Use for: memory subsystem performance analysis.               |
|                                                                                    |
| // CPU and NoC still LT. Only DRAM timing annotation added.                        |
|                                                                                    |
| **module** Stage2System                                                            |
|                                                                                    |
| **inst** cpu: CpuTlmModel **end** **inst** cpu // LT --- unchanged                 |
|                                                                                    |
| **inst** dram: DramAtModel **end** **inst** dram // AT: 40ns read latency          |
|                                                                                    |
| **inst** noc: NocLtModel **end** **inst** noc // LT --- unchanged                  |
|                                                                                    |
| bind cpu.mem -\> noc.upstream;                                                     |
|                                                                                    |
| bind noc.**port** -\> dram.mem;                                                    |
|                                                                                    |
| **end** **module** Stage2System                                                    |
|                                                                                    |
| // ── Stage 3: DRAM refined to RTL-backed --- cycle-accurate memory ───            |
|                                                                                    |
| // Speed: \~10× RTL (DRAM is RTL; CPU+NoC still TLM).                              |
|                                                                                    |
| // Use for: memory controller verification, power estimation.                      |
|                                                                                    |
| **module** Stage3System                                                            |
|                                                                                    |
| **inst** cpu: CpuTlmModel **end** **inst** cpu // TLM --- still fast               |
|                                                                                    |
| **inst** dram: DramRtlModel **end** **inst** dram // RTL-backed --- cycle accurate |
|                                                                                    |
| **inst** noc: NocLtModel **end** **inst** noc // TLM --- still fast                |
|                                                                                    |
| bind cpu.mem -\> noc.upstream;                                                     |
|                                                                                    |
| // Transactor auto-generated: TLM socket → RTL-backed target                       |
|                                                                                    |
| bind noc.**port** -\> dram.mem transactor: protocol Axi4;                          |
|                                                                                    |
| **end** **module** Stage3System                                                    |
|                                                                                    |
| // ── Stage 4: Full RTL --- all modules cycle-accurate ────────────────            |
|                                                                                    |
| // Speed: 1× RTL (parallel simulation on 32 cores: \~20× wall-clock).              |
|                                                                                    |
| // Use for: final verification, formal, gate-level simulation.                     |
|                                                                                    |
| **module** Stage4System                                                            |
|                                                                                    |
| **inst** cpu: CpuRtlModule **end** **inst** cpu // Full RTL                        |
|                                                                                    |
| **inst** dram: DramRtlModel **end** **inst** dram // Full RTL                      |
|                                                                                    |
| **inst** noc: NocRtlModule **end** **inst** noc // Full RTL                        |
|                                                                                    |
| // All sockets now directly RTL --- no transactors needed                          |
|                                                                                    |
| bind cpu.mem -\> noc.upstream;                                                     |
|                                                                                    |
| bind noc.**port** -\> dram.mem;                                                    |
|                                                                                    |
| **end** **module** Stage4System                                                    |
+------------------------------------------------------------------------------------+

**23.5 Selective Refinement via arch sim \--refine**

+-------------------------------------------------------------------------------+
| *arch_refine_commands*                                                        |
|                                                                               |
| \# Run at **full** LT speed --- all methods loosely-timed                     |
|                                                                               |
| arch sim \--tlm-lt SocTb.arch                                                 |
|                                                                               |
| \# Refine DRAM **to** AT timing --- everything **else** stays LT              |
|                                                                               |
| arch sim \--tlm-lt \--refine DramModel:at SocTb.arch                          |
|                                                                               |
| \# Refine DRAM **to** RTL-backed --- everything **else** stays LT             |
|                                                                               |
| arch sim \--tlm-lt \--refine DramModel:rtl SocTb.arch                         |
|                                                                               |
| \# Refine DRAM **and** NoC **to** RTL; CPU stays LT                           |
|                                                                               |
| arch sim \--tlm-lt \--refine DramModel:rtl \--refine NocRouter:rtl SocTb.arch |
|                                                                               |
| \# Full RTL simulation **with** parallel execution                            |
|                                                                               |
| arch sim \--rtl \--parallel \--cores 32 SocTb.arch                            |
|                                                                               |
| \# Refinement equivalence check:                                              |
|                                                                               |
| \# Verify that the RTL-backed method produces the same responses              |
|                                                                               |
| \# **as** the LT model **for** a given stimulus. Catches RTL bugs early.      |
|                                                                               |
| arch sim \--check-refinement DramModel SocTb.arch                             |
|                                                                               |
| \# Runs both LT **and** RTL-backed implementations **in** lockstep.           |
|                                                                               |
| \# Reports any cycle **where** the responses differ.                          |
|                                                                               |
| \# Output:                                                                    |
|                                                                               |
| \# DramModel refinement check: 10000 transactions verified                    |
|                                                                               |
| \# PASS --- RTL **and** LT responses identical **on** all transactions        |
+-------------------------------------------------------------------------------+

**23.6 Timing Emergence vs Timing Declaration**

The distinction between declared and emergent timing is central to understanding RTL-backed methods.

  ------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Property**            **AT: timing: 40 ns**                                   **RTL-backed: implement \... rtl**
  ----------------------- ------------------------------------------------------- ----------------------------------------------------------------------------
  **Timing source**       Declared by designer --- fixed annotation               Emerges from actual signal behavior --- varies per transaction

  **Accuracy**            Approximate --- same latency for all transactions       Cycle-accurate --- row hits differ from row misses; bank conflicts modeled

  **Backpressure**        Not modeled --- max_outstanding limits concurrency      Fully modeled --- arready stalls consume real cycles

  **Contention**          Not modeled --- all transactions get declared latency   Fully modeled --- competing requests delay each other realistically

  **Power correlation**   Cannot drive power estimation --- no signal activity    Signal transitions drive switching activity for power estimation

  **Debugging**           No waveform --- method is a black box                   Full waveform --- every AR/R/AW/W/B signal visible in GTKWave

  **Speed**               \~50× RTL simulation speed                              1× RTL (parallel on 32 cores: \~20× wall-clock)
  ------------------------------------------------------------------------------------------------------------------------------------------------------------

> ◈ A common pattern: develop firmware at LT (1000× speed), run performance analysis at AT (50× speed) to verify latency budgets, then run final verification at RTL-backed (1× speed) with full waveform capture. All three use the same testbench and the same initiator code.

**23.7 Refinement Equivalence Checking**

The arch sim \--check-refinement command runs the LT and RTL-backed implementations in lockstep and verifies that they produce the same functional responses. This is not formal equivalence --- it is transaction-level equivalence checking: for the same sequence of method calls with the same arguments, do both implementations return the same values?

+----------------------------------------------------------------------------------+
| *equivalence_check.arch*                                                         |
|                                                                                  |
| // Equivalence is checked at the method boundary --- not at the signal level.    |
|                                                                                  |
| // The RTL implementation may have different internal timing and signal behavior |
|                                                                                  |
| // as long as every method call returns the same value as the LT model.          |
|                                                                                  |
| // What the checker verifies:                                                    |
|                                                                                  |
| // LT: data = Storage.data\[addr\] → returns 0x0102030405060708                  |
|                                                                                  |
| // RTL: AR channel → wait arready → R channel → returns 0x0102030405060708       |
|                                                                                  |
| // PASS: both return the same value                                              |
|                                                                                  |
| // What the checker does NOT verify:                                             |
|                                                                                  |
| // Cycle count (RTL may take 4 cycles, LT takes 0)                               |
|                                                                                  |
| // Internal signal transitions (RTL drives arvalid etc., LT does not)            |
|                                                                                  |
| // Side effects inside the target (both must have the same memory state)         |
|                                                                                  |
| // Equivalence failure example:                                                  |
|                                                                                  |
| // LT: read(0x1008) returns 0x0000_0000_0000_0000 (zero-init)                    |
|                                                                                  |
| // RTL: read(0x1008) returns 0xXXXX_XXXX_XXXX_XXXX (uninitialised RAM)           |
|                                                                                  |
| // FAIL --- RTL implementation has different reset state than LT model           |
|                                                                                  |
| // Fix: add init: zero to the RTL RAM declaration                                |
+----------------------------------------------------------------------------------+

**23.8 Complete Refinement Example --- DRAM Controller**

+----------------------------------------------------------------------------------------------------------------------+
| *dram_three_tiers.arch*                                                                                              |
|                                                                                                                      |
| // Single module with all three tiers declared.                                                                      |
|                                                                                                                      |
| // arch sim selects which implement block to use.                                                                    |
|                                                                                                                      |
| **module** DramController                                                                                            |
|                                                                                                                      |
| **param** SIZE_MB: **const** = 256;                                                                                  |
|                                                                                                                      |
| **param** ROW_HIT_NS: **const** = 10;                                                                                |
|                                                                                                                      |
| **param** ROW_MISS_NS: **const** = 40;                                                                               |
|                                                                                                                      |
| **port** clk: **in** Clock\<SysDomain\>;                                                                             |
|                                                                                                                      |
| **port** rst: **in** Reset\<Sync\>;                                                                                  |
|                                                                                                                      |
| socket axi: target Axi4\<ADDR_W 32, DATA_W 64, ID_W 4\>;                                                             |
|                                                                                                                      |
| // Internal storage --- shared across all tiers                                                                      |
|                                                                                                                      |
| ram Storage                                                                                                          |
|                                                                                                                      |
| **param** DEPTH: **const** = SIZE_MB \* 131072;                                                                      |
|                                                                                                                      |
| **port** clk: **in** Clock\<SysDomain\>;                                                                             |
|                                                                                                                      |
| kind simple_dual; **read**: **sync**;                                                                                |
|                                                                                                                      |
| store data: Vec\<UInt\<64\>, DEPTH\>; **end** store                                                                  |
|                                                                                                                      |
| **port** read_port { en: **in** Bool; addr: **in** UInt\<24\>; data: **out** UInt\<64\>; **end** **port** read_port  |
|                                                                                                                      |
| **port** write_port { en: **in** Bool; addr: **in** UInt\<24\>; data: **in** UInt\<64\>; **end** **port** write_port |
|                                                                                                                      |
| **init**: zero;                                                                                                      |
|                                                                                                                      |
| **end** ram Storage                                                                                                  |
|                                                                                                                      |
| // Track open DRAM row per bank (4 banks)                                                                            |
|                                                                                                                      |
| **reg** open_row: Vec\<UInt\<14\>, 4\> **init** Vec::splat(14\'hFFFF);                                               |
|                                                                                                                      |
| // ── LT implementation ─────────────────────────────────────────                                                    |
|                                                                                                                      |
| implement axi.**read**(addr) -\> Future\<UInt\<64\>\>                                                                |
|                                                                                                                      |
| tier: loosely_timed;                                                                                                 |
|                                                                                                                      |
| **return** Storage.data\[addr\[26:3\]\];                                                                             |
|                                                                                                                      |
| **end** implement axi.**read**                                                                                       |
|                                                                                                                      |
| // ── AT implementation --- row hit/miss modeled ──────────────────                                                  |
|                                                                                                                      |
| implement axi.**read**(addr) -\> Future\<UInt\<64\>\>                                                                |
|                                                                                                                      |
| tier: approximately_timed;                                                                                           |
|                                                                                                                      |
| **let** bank: UInt\<2\> = addr\[4:3\];                                                                               |
|                                                                                                                      |
| **let** row: UInt\<14\> = addr\[26:13\];                                                                             |
|                                                                                                                      |
| **let** latency: UInt\<8\> =                                                                                         |
|                                                                                                                      |
| open_row\[bank\] == row ? ROW_HIT_NS : ROW_MISS_NS;                                                                  |
|                                                                                                                      |
| open_row\[bank\] = row;                                                                                              |
|                                                                                                                      |
| wait latency ns;                                                                                                     |
|                                                                                                                      |
| **return** Storage.data\[addr\[26:3\]\];                                                                             |
|                                                                                                                      |
| **end** implement axi.**read**                                                                                       |
|                                                                                                                      |
| // ── RTL-backed implementation --- cycle accurate ─────────────────                                                 |
|                                                                                                                      |
| implement axi.**read**(addr) -\> Future\<UInt\<64\>\>                                                                |
|                                                                                                                      |
| tier: rtl;                                                                                                           |
|                                                                                                                      |
| rtl                                                                                                                  |
|                                                                                                                      |
| arvalid = true; araddr = addr; arid = alloc_id();                                                                    |
|                                                                                                                      |
| arlen = 0; arsize = 3; arburst = 2\'b01;                                                                             |
|                                                                                                                      |
| wait until arready == true;                                                                                          |
|                                                                                                                      |
| wait 1 cycle;                                                                                                        |
|                                                                                                                      |
| arvalid = false;                                                                                                     |
|                                                                                                                      |
| // Row hit/miss determined by actual open_row register state                                                         |
|                                                                                                                      |
| // --- no annotation; real cycles consumed by real logic                                                             |
|                                                                                                                      |
| wait until rvalid == true **and** rid == arid;                                                                       |
|                                                                                                                      |
| **let** data: UInt\<64\> = rdata;                                                                                    |
|                                                                                                                      |
| rready = true; wait 1 cycle; rready = false;                                                                         |
|                                                                                                                      |
| free_id(arid);                                                                                                       |
|                                                                                                                      |
| **return** data;                                                                                                     |
|                                                                                                                      |
| **end** rtl                                                                                                          |
|                                                                                                                      |
| **end** implement axi.**read**                                                                                       |
|                                                                                                                      |
| **end** **module** DramController                                                                                    |
|                                                                                                                      |
| // ── Simulation commands ───────────────────────────────────────────                                                |
|                                                                                                                      |
| // arch sim \--refine DramController:lt SocTb.arch // \~1000× RTL                                                    |
|                                                                                                                      |
| // arch sim \--refine DramController:at SocTb.arch // \~50× RTL                                                      |
|                                                                                                                      |
| // arch sim \--refine DramController:rtl SocTb.arch // 1× RTL                                                        |
|                                                                                                                      |
| // arch sim \--check-refinement DramController SocTb.arch // verify equivalence                                      |
+----------------------------------------------------------------------------------------------------------------------+

**22.12 Cycle-Accurate TLM --- RTL Signal Fidelity Behind Method Interfaces**

A TLM method call can be backed by a full RTL signal-level implementation. When this mode is active, calling axi.read(addr) executes the complete AXI AR and R channel handshake cycle by cycle --- every valid, ready, and data signal toggles exactly as it would in hardware --- while the caller still uses the clean method call API. This is cycle-accurate TLM: the initiator sees an abstracted interface; the implementation has full signal fidelity.

**22.12.1 Three Implementation Strategies per Method**

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Strategy**                   **Keyword**                       **Timing Model**                                 **Signal Fidelity**                                    **Use When**
  ------------------------------ --------------------------------- ------------------------------------------------ ------------------------------------------------------ --------------------------------------------------------------------
  **Functional model**           implement \... timing: N cycles   AT/LT --- N cycles per call                      None --- no signals toggled                            Architectural exploration, SW bring-up, max speed

  **Cycle-accurate model**       implement \... rtl_accurate       Cycle-exact --- runs real FSM                    Full --- every signal toggles per clock                Protocol verification, interface testing, latency-sensitive models

  **Automatic from RTL ports**   implement \... from_rtl           Cycle-exact --- derived from port declarations   Full --- compiler generates the FSM from port bundle   When the RTL port bundle fully describes the protocol
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**22.12.2 rtl_accurate Implementation**

An implement block marked rtl_accurate replaces the timing: N annotation with a full RTL description of the handshake. The method call launches the RTL FSM, which drives signals cycle by cycle and completes the Future or Token when the protocol completes. The caller\'s sequence body is suspended exactly as long as the real hardware would take.

+--------------------------------------------------------------------------------------+
| *rtl_accurate_impl.arch*                                                             |
|                                                                                      |
| // AXI4 read method --- rtl_accurate implementation                                  |
|                                                                                      |
| // The caller writes: let f = axi.read(addr); await f;                               |
|                                                                                      |
| // Underneath, this FSM runs, driving every AXI AR and R signal:                     |
|                                                                                      |
| implement axi.**read**(addr: UInt\<32\>) -\> Future\<UInt\<64\>\>                    |
|                                                                                      |
| rtl_accurate;                                                                        |
|                                                                                      |
| // Internal state for this transaction                                               |
|                                                                                      |
| **reg** ar_sent: Bool **init** false;                                                |
|                                                                                      |
| **reg** data_out: UInt\<64\> **init** 0;                                             |
|                                                                                      |
| // Phase 1: drive AR channel until arready asserted                                  |
|                                                                                      |
| **comb**                                                                             |
|                                                                                      |
| arvalid = **not** ar_sent;                                                           |
|                                                                                      |
| araddr = addr;                                                                       |
|                                                                                      |
| arid = next_id(); // allocate next available AXI ID                                  |
|                                                                                      |
| arlen = 0; // single beat                                                            |
|                                                                                      |
| arsize = 3\'b011; // 8 bytes                                                         |
|                                                                                      |
| arburst = 2\'b01; // INCR                                                            |
|                                                                                      |
| **end** **comb**                                                                     |
|                                                                                      |
| **reg** **on** clk rising, rst high                                                  |
|                                                                                      |
| **if** rst                                                                           |
|                                                                                      |
| ar_sent \<= false;                                                                   |
|                                                                                      |
| **end** **if**                                                                       |
|                                                                                      |
| **else** **if** arvalid **and** arready                                              |
|                                                                                      |
| ar_sent \<= true; // AR channel handshake complete                                   |
|                                                                                      |
| **end** **else**                                                                     |
|                                                                                      |
| **end** **reg**                                                                      |
|                                                                                      |
| // Phase 2: wait for R channel response                                              |
|                                                                                      |
| **comb**                                                                             |
|                                                                                      |
| rready = ar_sent; // assert rready once AR is accepted                               |
|                                                                                      |
| **end** **comb**                                                                     |
|                                                                                      |
| **reg** **on** clk rising, rst high                                                  |
|                                                                                      |
| **if** rvalid **and** rready **and** rid == arid                                     |
|                                                                                      |
| data_out \<= rdata;                                                                  |
|                                                                                      |
| complete future **with** data_out; // resolve the Future --- caller unblocks         |
|                                                                                      |
| **end** **if**                                                                       |
|                                                                                      |
| **end** **reg**                                                                      |
|                                                                                      |
| **end** implement axi.**read**                                                       |
|                                                                                      |
| // The compiler verifies:                                                            |
|                                                                                      |
| // 1. Every RTL port of the AXI bus is driven in exactly one implement block         |
|                                                                                      |
| // or declared as driven by another implement block (e.g. write drives awvalid)      |
|                                                                                      |
| // 2. The \'complete future\' statement is reachable on all non-reset paths          |
|                                                                                      |
| // 3. No signal driven here conflicts with another implement block in the same cycle |
+--------------------------------------------------------------------------------------+

> ◈ complete future with value is the statement that resolves the pending Future and unblocks the caller\'s await expression. The compiler tracks that every implement block with a non-void return type contains exactly one reachable complete statement --- a missing complete is a compile error, not a simulation hang.

**22.12.3 from_rtl --- Compiler-Generated Cycle-Accurate Implementation**

When the RTL port bundle fully expresses the protocol --- standard valid/ready with named fields matching AXI, AHB, APB, or TileLink conventions --- the compiler can generate the cycle-accurate implement block automatically. The designer writes only the bus declaration; the compiler synthesises the FSM.

+---------------------------------------------------------------------------------------------------------+
| *from_rtl_impl.arch*                                                                                    |
|                                                                                                         |
| // Interface with from_rtl directive --- compiler generates the rtl_accurate FSM                        |
|                                                                                                         |
| **bus** Axi4Lite                                                                                  |
|                                                                                                         |
| **param** ADDR_W: **const** = 32;                                                                       |
|                                                                                                         |
| **param** DATA_W: **const** = 32;                                                                       |
|                                                                                                         |
| // RTL ports --- standard AXI4-Lite signal names                                                        |
|                                                                                                         |
| **port** awvalid: **out** Bool; **port** awready: **in** Bool; **port** awaddr: **out** UInt\<ADDR_W\>; |
|                                                                                                         |
| **port** wvalid: **out** Bool; **port** wready: **in** Bool; **port** wdata: **out** UInt\<DATA_W\>;    |
|                                                                                                         |
| **port** wstrb: **out** UInt\<DATA_W/8\>;                                                               |
|                                                                                                         |
| **port** bvalid: **in** Bool; **port** bready: **out** Bool; **port** bresp: **in** UInt\<2\>;          |
|                                                                                                         |
| **port** arvalid: **out** Bool; **port** arready: **in** Bool; **port** araddr: **out** UInt\<ADDR_W\>; |
|                                                                                                         |
| **port** rvalid: **in** Bool; **port** rready: **out** Bool; **port** rdata: **in** UInt\<DATA_W\>;     |
|                                                                                                         |
| **port** rresp: **in** UInt\<2\>;                                                                       |
|                                                                                                         |
| methods                                                                                                 |
|                                                                                                         |
| pipelined method **read**(addr: UInt\<ADDR_W\>) -\> Future\<UInt\<DATA_W\>\>                            |
|                                                                                                         |
| timing: 2 cycles;                                                                                       |
|                                                                                                         |
| max_outstanding: 1; // AXI4-Lite: no pipelining                                                         |
|                                                                                                         |
| implement: from_rtl // compiler generates AXI4-Lite AR+R FSM                                            |
|                                                                                                         |
| protocol: Axi4Lite;                                                                                     |
|                                                                                                         |
| **end** implement                                                                                       |
|                                                                                                         |
| **end** method **read**                                                                                 |
|                                                                                                         |
| pipelined method **write**(                                                                             |
|                                                                                                         |
| addr: UInt\<ADDR_W\>, data: UInt\<DATA_W\>, strobe: UInt\<DATA_W/8\>                                    |
|                                                                                                         |
| ) -\> Future\<AxiResp\>                                                                                 |
|                                                                                                         |
| timing: 2 cycles;                                                                                       |
|                                                                                                         |
| max_outstanding: 1;                                                                                     |
|                                                                                                         |
| implement: from_rtl                                                                                     |
|                                                                                                         |
| protocol: Axi4Lite;                                                                                     |
|                                                                                                         |
| **end** implement                                                                                       |
|                                                                                                         |
| **end** method **write**                                                                                |
|                                                                                                         |
| **end** methods                                                                                         |
|                                                                                                         |
| **end** **bus** Axi4Lite                                                                          |
|                                                                                                         |
| // Built-in from_rtl protocol drivers:                                                                  |
|                                                                                                         |
| // Axi4, Axi4Lite, Axi4Stream, AHB, AHBLite, APB, TileLink, Wishbone                                    |
|                                                                                                         |
| // Custom protocols can be registered as Arch bus modules.                                              |
+---------------------------------------------------------------------------------------------------------+

**22.12.4 Selecting Implementation Strategy at Simulation Time**

The implementation strategy is selectable per-bus at simulation invocation, without changing source. This enables a three-tier simulation flow from a single design source:

+--------------------------------------------------------------------------------------+
| *sim_tiers.arch*                                                                     |
|                                                                                      |
| \# Tier 1: Functional TLM --- maximum speed, no signal fidelity                      |
|                                                                                      |
| \# Use: SW bring-up, firmware, architectural exploration                             |
|                                                                                      |
| \# Speed: 100--1000× RTL baseline                                                    |
|                                                                                      |
| arch sim \--tlm-lt AttnUnitTb.arch                                                   |
|                                                                                      |
| \# Tier 2: Approximate TLM --- nanosecond timing, no signal fidelity                 |
|                                                                                      |
| \# Use: latency budgeting, throughput analysis, performance estimation               |
|                                                                                      |
| \# Speed: 10--50× RTL baseline                                                       |
|                                                                                      |
| arch sim \--tlm-at AttnUnitTb.arch                                                   |
|                                                                                      |
| \# Tier 3: Cycle-accurate TLM --- **full** RTL signal fidelity behind method calls   |
|                                                                                      |
| \# Use: protocol correctness, AXI compliance testing, interface bring-up       |
|                                                                                      |
| \# Speed: 1--2× RTL baseline (small overhead **from** method call bookkeeping)       |
|                                                                                      |
| arch sim \--tlm-rtl AttnUnitTb.arch                                                  |
|                                                                                      |
| \# Mixed: some interfaces at RTL, others at TLM                                      |
|                                                                                      |
| \# Use: **when** one subsystem needs signal-level verification but others do **not** |
|                                                                                      |
| arch sim \--tlm-rtl dut.axi \--tlm-at dut.hbm \--tlm-lt dut.pcie AttnUnitTb.arch     |
|                                                                                      |
| \# Full RTL --- no TLM abstraction anywhere, all ports driven **as** signals         |
|                                                                                      |
| \# Use: regression, formal, gate-level verification                                  |
|                                                                                      |
| arch sim AttnUnitTb.arch                                                             |
+--------------------------------------------------------------------------------------+

**22.12.5 What Cycle-Accurate TLM Catches That AT/LT Cannot**

Running a testbench with \--tlm-rtl surfaces protocol-level bugs that the approximate timing model cannot detect, because the full signal handshake executes:

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Bug Class**                                    **AT/LT**                                       **Cycle-Accurate TLM**                      **RTL Full Sim**
  ------------------------------------------------ ----------------------------------------------- ------------------------------------------- ------------------
  **arready deasserted mid-burst by target**       Not visible --- method returns after timing:N   Caught --- FSM waits for real arready       Caught

  **rvalid without matching arid (ID mismatch)**   Not visible                                     Caught --- rid check in FSM                 Caught

  **wvalid/awvalid issued in wrong order**         Not visible                                     Caught --- AW/W channel ordering verified   Caught

  **bvalid not asserted within N cycles**          Not visible                                     Caught --- timeout assert in FSM            Caught

  **Initiator violates max_outstanding**           Compile-time warning only                       Caught --- ready stalls FSM correctly       Caught

  **Wrong ARLEN for burst length**                 Not visible --- burst method hides length       Caught --- arlen drives wire directly       Caught

  **Back-to-back transactions with no gap**        Not visible --- LT has no time                  Caught --- real cycle timing enforced       Caught
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------

**22.12.6 The Refinement Workflow**

The practical flow for an AI accelerator or SoC team proceeds through three stages, each using a different simulation tier from the same Arch source:

+----------------------------------------------------------------------------------+
| *refinement_workflow.arch*                                                       |
|                                                                                  |
| // ── Stage 1: Architecture (weeks 1--4) ────────────────────────────────        |
|                                                                                  |
| // All interfaces are LT. Simulation speed: \~500× RTL.                          |
|                                                                                  |
| // Goal: verify SW stack, API correctness, memory map, interrupt model.          |
|                                                                                  |
| // arch sim \--tlm-lt SystemTb.arch                                              |
|                                                                                  |
| // ── Stage 2: Performance (weeks 5--8) ─────────────────────────────────        |
|                                                                                  |
| // Memory interfaces switch to AT. Compute stays LT.                             |
|                                                                                  |
| // Goal: verify latency budgets, bandwidth, bottleneck identification.           |
|                                                                                  |
| // arch sim \--tlm-at dut.hbm \--tlm-at dut.noc \--tlm-lt dut.cpu SystemTb.arch  |
|                                                                                  |
| // ── Stage 3: Protocol Verification (weeks 9--12) ───────────────────────       |
|                                                                                  |
| // Critical interfaces switch to rtl_accurate. Others stay AT.                   |
|                                                                                  |
| // Goal: AXI compliance, CDC correctness, handshake protocol fidelity.           |
|                                                                                  |
| // arch sim \--tlm-rtl dut.axi \--tlm-at dut.hbm \--tlm-lt dut.cpu SystemTb.arch |
|                                                                                  |
| // ── Stage 4: Full RTL Regression ──────────────────────────────────────        |
|                                                                                  |
| // All TLM abstraction removed. Parallel simulation active.                      |
|                                                                                  |
| // Goal: final gate-level sign-off, coverage closure.                            |
|                                                                                  |
| // arch sim \--parallel \--cores 32 SystemTb.arch                                |
|                                                                                  |
| // The testbench source is identical across all four stages.                     |
|                                                                                  |
| // The design source is identical across all four stages.                        |
|                                                                                  |
| // Only the arch sim flags change.                                               |
+----------------------------------------------------------------------------------+

> ◈ The same testbench sequence --- tb_master.read(addr), check result, report --- runs unchanged across all four stages. The caller never knows whether the implementation is functional, approximate, or cycle-accurate. This is the abstraction boundary working correctly.

**22.12.7 Correctness Guarantee Across Abstraction Levels**

The Arch compiler enforces that an rtl_accurate implement block is a correct refinement of its method signature:

  ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Property Checked**               **What It Means**                                                                         **How Enforced**
  ---------------------------------- ----------------------------------------------------------------------------------------- -----------------------------------------------------------------------------------------------------
  **Completion reachability**        complete future is reachable on all non-reset paths                                       Compiler performs reachability analysis; missing complete is a compile error

  **Port coverage**                  every RTL port in the bus is driven by exactly one implement block                  Compiler checks port-to-implement assignment; undriven ports are errors

  **No cross-implement conflicts**   two implement blocks for different methods do not drive the same port in the same cycle   Compiler inserts priority mux and warns if simultaneous drive is possible

  **Timing monotonicity**            an rtl_accurate method takes at least as many cycles as its timing: annotation            Compiler emits warning if FSM can complete faster than timing: declares --- signals a spec mismatch

  **max_outstanding enforcement**    no more than N transactions simultaneously in the FSM                                     Compiler generates saturating counter; (N+1)th call blocks automatically
  ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**22.13 Cycle Accuracy of Pipelined Method Calls**

A pipelined method call returning Future\<T\> correctly models transaction concurrency --- multiple calls in-flight, responses in order. However, the degree of timing accuracy depends on the implementation strategy behind the method. This section makes the distinction precise, because the difference matters for latency-sensitive designs.

**22.13.1 What timing: N Gets Right and Wrong**

The timing: N declaration models a fixed-latency response with no backpressure. It correctly models concurrency semantics --- a pipelined caller can issue multiple calls before any respond --- but deviates from RTL in three specific ways:

+-----------------------------------------------------------------------------------------------+
| *timing_n_gap.arch*                                                                           |
|                                                                                               |
| // RTL actual waveform --- arready deasserted for 2 cycles (target busy):                     |
|                                                                                               |
| //                                                                                            |
|                                                                                               |
| // cycle: 0 1 2 3 4 5 6 7                                                                     |
|                                                                                               |
| // arvalid: 1 1 1 0 0 0 0 0                                                                   |
|                                                                                               |
| // arready: 0 0 1 0 0 1 0 0 // target busy cycles 0--1, 4--5                                  |
|                                                                                               |
| // rvalid: 0 0 0 0 1 0 0 1                                                                    |
|                                                                                               |
| //                                                                                            |
|                                                                                               |
| // Call 0: issued c0, accepted c2, response c4. Future 0 resolves: cycle 4                    |
|                                                                                               |
| // Call 1: issued c3 (after acceptance), accepted c5, response c7. Future 1 resolves: cycle 7 |
|                                                                                               |
| // pipelined + timing: 4 cycles --- what the model produces:                                  |
|                                                                                               |
| //                                                                                            |
|                                                                                               |
| // Call 0: issued cycle 0. Future 0 resolves: cycle 4 ✓ coincidentally correct                |
|                                                                                               |
| // Call 1: issued cycle 0. Future 1 resolves: cycle 4 ✗ RTL says cycle 7                      |
|                                                                                               |
| //                                                                                            |
|                                                                                               |
| // Three failures:                                                                            |
|                                                                                               |
| // 1. Call 1 issued cycle 0 --- wrong. RTL: cycle 3 (arready held call 0 for 2 cycles)        |
|                                                                                               |
| // 2. Future 1 resolves cycle 4 --- wrong. RTL: cycle 7                                       |
|                                                                                               |
| // 3. Backpressure invisible --- both calls overlap fully regardless of target load           |
+-----------------------------------------------------------------------------------------------+

**22.13.2 Backpressure --- The Critical Failure Mode**

The most consequential divergence is backpressure. When a target deasserts arready because it is busy, the RTL initiator must hold arvalid and stall. The timing: N model has no arready signal to observe --- it issues all calls immediately at the declared rate, making the target appear always-ready. This produces optimistic throughput estimates proportional to how congested the real target is.

+-----------------------------------------------------------------------------+
| *backpressure_gap.arch*                                                     |
|                                                                             |
| // 8 pipelined reads, target has 50% arready availability:                  |
|                                                                             |
| // RTL: calls issue every 2 cycles (arready throttles)                      |
|                                                                             |
| // total time: 18 cycles                                                    |
|                                                                             |
| // timing: 4 cycles: all 8 calls issue cycle 0, all Futures resolve cycle 4 |
|                                                                             |
| // total time modeled: 4 cycles ← 4.5× optimistic                           |
|                                                                             |
| // rtl_accurate: calls issue every 2 cycles (arready modeled)               |
|                                                                             |
| // total time: 18 cycles ← matches RTL exactly                              |
+-----------------------------------------------------------------------------+

> *⚑ ⚠ timing: N for pipelined methods can be severely optimistic when target arready availability is limited. For accurate bandwidth modeling, use rtl_accurate on both sides.*

**22.13.3 Concurrency vs Timing Fidelity --- Two Orthogonal Axes**

  ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
                     **timing: N (approximate)**                                                    **rtl_accurate (cycle-exact)**
  ------------------ ------------------------------------------------------------------------------ ----------------------------------------------------------------------------------
  **blocking**       Serial, fixed N-cycle latency, no backpressure                                 Serial, real FSM, backpressure modeled

  **pipelined**      Concurrent issue, fixed N latency, no backpressure --- overlap is optimistic   Concurrent issue, real handshake, backpressure propagated --- cycle-accurate

  **out_of_order**   Concurrent, fixed latency range, ID routing modeled, no backpressure           Concurrent, real FSM per ID, backpressure and ID interleaving --- cycle-accurate

  **burst**          One AR issue, N beats at fixed rate, no per-beat backpressure                  One AR issue, real INCR burst, wlast/rlast modeled --- cycle-accurate
  ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

> ◈ Concurrency mode (pipelined, out_of_order, burst) describes the caller API contract --- how many calls can be in-flight and what order responses arrive. Timing fidelity (timing: N vs rtl_accurate) describes how accurately the implementation models real hardware. These are independent choices.

**22.13.4 Making Pipelined Calls Fully Cycle-Accurate**

Cycle accuracy requires rtl_accurate on both the initiator and target implement blocks. A cycle-accurate initiator driving a functional target achieves partial accuracy --- issue timing is correct, but completion timing still reflects the target\'s fixed-latency response rather than real backpressure.

+-------------------------------------------------------------------------------------------+
| *rtl_accurate_both.arch*                                                                  |
|                                                                                           |
| // Initiator --- rtl_accurate: drives real AR/R signals                                   |
|                                                                                           |
| implement axi.**read**(addr: UInt\<32\>) -\> Future\<UInt\<64\>\>                         |
|                                                                                           |
| rtl_accurate;                                                                             |
|                                                                                           |
| **reg** ar_sent: Bool **init** false;                                                     |
|                                                                                           |
| **comb** arvalid = **not** ar_sent; araddr = addr; **end** **comb**                       |
|                                                                                           |
| **reg** **on** clk rising, rst high                                                       |
|                                                                                           |
| **if** arvalid **and** arready ar_sent \<= true; **end** **if** // waits for real arready |
|                                                                                           |
| **end** **reg**                                                                           |
|                                                                                           |
| **comb** rready = ar_sent; **end** **comb**                                               |
|                                                                                           |
| **reg** **on** clk rising, rst high                                                       |
|                                                                                           |
| **if** rvalid **and** rready                                                              |
|                                                                                           |
| complete future **with** rdata; // resolves at real rvalid cycle --- not N                |
|                                                                                           |
| **end** **if**                                                                            |
|                                                                                           |
| **end** **reg**                                                                           |
|                                                                                           |
| **end** implement axi.**read**                                                            |
|                                                                                           |
| // Target --- rtl_accurate: asserts arready based on internal state                       |
|                                                                                           |
| implement mem.**read**(addr) -\> UInt\<64\>                                               |
|                                                                                           |
| rtl_accurate;                                                                             |
|                                                                                           |
| **reg** busy: Bool **init** false;                                                        |
|                                                                                           |
| **comb** arready = **not** busy; **end** **comb** // deasserts when pipeline full         |
|                                                                                           |
| **reg** **on** clk rising, rst high                                                       |
|                                                                                           |
| **if** arvalid **and** arready                                                            |
|                                                                                           |
| busy \<= true;                                                                            |
|                                                                                           |
| after 3 cycles // real SRAM latency                                                       |
|                                                                                           |
| rvalid \<= true; rdata \<= Storage.data\[addr\[27:3\]\];                                  |
|                                                                                           |
| after 1 cycle rvalid \<= false; busy \<= false; **end** after                             |
|                                                                                           |
| **end** after                                                                             |
|                                                                                           |
| **end** **if**                                                                            |
|                                                                                           |
| **end** **reg**                                                                           |
|                                                                                           |
| **end** implement mem.**read**                                                            |
|                                                                                           |
| // With both rtl_accurate: the pipelined call sequence is cycle-for-cycle                 |
|                                                                                           |
| // identical to full RTL simulation of the same design.                                   |
+-------------------------------------------------------------------------------------------+

**22.13.5 When is timing: N Sufficient?**

  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Simulation Goal**                                                              **timing: N sufficient?**   **Reason**
  -------------------------------------------------------------------------------- --------------------------- --------------------------------------------------------------------
  **SW API correctness --- does the driver issue the right call sequence?**        Yes                         Call ordering and response routing modeled; cycle count irrelevant

  **Functional data correctness --- does the right data reach the right place?**   Yes                         Data flow modeled completely regardless of timing fidelity

  **Worst-case latency estimation**                                                No                          Backpressure absent --- timing: N gives best-case only

  **Throughput under load**                                                        No                          All calls overlap fully --- optimistic by design

  **AXI protocol compliance (arvalid held, wlast on last beat)**                   No                          Signal-level rules require signal transitions

  **Performance regression detection**                                             Sometimes                   Only catches regressions larger than N-cycle variance
  ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

**22.13.6 The Honest Summary**

A pipelined method call with timing: N cycles is not cycle-accurate compared to the eventual RTL implementation. It correctly models that multiple transactions are in-flight simultaneously and that responses arrive in order --- the concurrency contract --- but does not model arready backpressure, variable-latency responses, or the exact cycle at which each Future resolves under load.

Cycle accuracy requires rtl_accurate on both initiator and target implement blocks. At that point the pipelined call API --- issue Future, continue, await --- becomes cycle-for-cycle identical to RTL simulation. The abstraction stays clean; the fidelity is complete.

The timing: N mode is not intended to approximate RTL timing --- it is intended to provide maximum simulation speed for use cases where cycle accuracy is not the goal. The arch sim flags \--tlm-lt, \--tlm-at, and \--tlm-rtl exist to make this tradeoff explicit per simulation run.

**25. AI-Assisted Hardware Design Workflow**

Arch is designed for AI-assisted hardware generation. This section describes how the workflow operates in practice --- how a designer interacts with an AI assistant to produce correct Arch, why the full language specification does not need to be in the AI\'s context window, and what a minimal, effective AI context looks like.

**25.1 Why the Full Spec Does Not Need to Be in Context**

A large language model assisting with Arch hardware design does not need the complete specification in its context window. Three properties of the language make this possible:

- **Uniform construct schema:** every Arch construct --- fifo, pipeline, cam, crossbar, scoreboard --- has identical structure: param / port / body / verification / end keyword Name. An AI that knows the schema for one construct knows the shape for all of them. Novel constructs do not require novel syntax.

- **Compiler as verifier:** the Arch compiler produces precise, structured error messages that name the construct, the signal, the type mismatch, and the line. When generated code contains an error, the compiler output is sufficient context for the AI to self-correct --- without consulting the spec. The compiler replaces the spec in the feedback loop.

- **Intent-to-construct mapping:** hardware intent maps unambiguously to an Arch construct. A FIFO is always fifo. A state machine is always fsm. A pipelined datapath is always pipeline. The AI does not need to choose between implementation strategies --- it needs to identify the right construct and fill in the schema.

**25.2 The Core Workflow**

The practical AI-assisted Arch design loop has four steps, typically completing in two to four iterations:

+-----------------------------------------------------------------------------------------+
| *workflow_steps*                                                                        |
|                                                                                         |
| Step 1 --- Designer states intent **in** natural language                               |
|                                                                                         |
| \'I need a 4-**stage** AXI4 **read** **pipeline** **with** a 16-entry scoreboard        |
|                                                                                         |
| that handles **out**-**of**-order responses **and** stalls **on** structural hazards.\' |
|                                                                                         |
| Step 2 --- AI generates Arch                                                            |
|                                                                                         |
| The AI identifies the constructs (**pipeline**, scoreboard, **bus** Axi4),        |
|                                                                                         |
| applies the uniform schema **to** each, **and** emits Arch source.                      |
|                                                                                         |
| It uses **todo**! **for** any body it is uncertain about.                               |
|                                                                                         |
| Step 3 --- Designer runs the compiler                                                   |
|                                                                                         |
| arch check ReadPipeline.arch                                                            |
|                                                                                         |
| The compiler emits zero errors, **or** precise typed errors.                            |
|                                                                                         |
| Step 4 --- AI **self**-corrects **from** compiler output                                |
|                                                                                         |
| Compiler error: \'ReadPipeline: **port** rid has **type** UInt\<4\>                     |
|                                                                                         |
| but scoreboard token expects UInt\<ID_W\>;                                              |
|                                                                                         |
| declare **param** ID_W: **const** = 4 **or** widen token.\'                             |
|                                                                                         |
| AI fixes the **param** declaration. No spec lookup required.                            |
+-----------------------------------------------------------------------------------------+

**25.3 What Goes in the AI Context Window**

The effective AI context for Arch hardware design has three components, all compact:

  ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
  **Component**                          **Size**            **Content**                                                                  **Purpose**
  -------------------------------------- ------------------- ---------------------------------------------------------------------------- --------------------------------------------------------------
  **Arch AI Reference Card**             \~400 lines         Construct catalog, universal schema, type table, one example per construct   Activates construct knowledge --- fits in one context window

  **Design intent**                      5--20 lines         Natural language description of the block to build                           Specifies what to generate

  **Compiler output (when iterating)**   5--30 lines         arch check error messages from the previous iteration                        Replaces spec lookup for error correction
  ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------

> ◈ The full specification is a reference document for human designers learning the language. The AI Reference Card is what an AI assistant actually uses. It is structured differently --- optimised for construct lookup and schema recall, not for human reading.

**25.4 Effective Prompting Patterns**

Several prompting patterns consistently produce high-quality Arch output:

**Pattern 1 --- Construct-First**

+--------------------------------------------------------------------------+
| *construct_first_pattern*                                                |
|                                                                          |
| // Tell the AI which construct to use.                                   |
|                                                                          |
| // The schema fills itself in from the construct name.                   |
|                                                                          |
| // Instead of:                                                           |
|                                                                          |
| // \'Design a hardware component that stores data in a ring buffer\...\' |
|                                                                          |
| // Write:                                                                |
|                                                                          |
| // \'Generate an Arch fifo construct named InstrQueue with:              |
|                                                                          |
| // depth 64, element type struct InstrPacket, single clock SysDomain.    |
|                                                                          |
| // Expose push_valid/push_ready and pop_valid/pop_ready ports.           |
|                                                                          |
| // Add a cover property for push_when_full.\'                            |
|                                                                          |
| // The construct name resolves all structural ambiguity.                 |
|                                                                          |
| // The AI fills param / port / body / verification.                      |
+--------------------------------------------------------------------------+

**Pattern 2 --- Interface-First**

+--------------------------------------------------------------------------+
| *interface_first_pattern*                                                |
|                                                                          |
| // Describe the bus contract before the implementation.                  |
|                                                                          |
| // The AI generates the port bundle and TLM methods first,               |
|                                                                          |
| // then fills in the body to satisfy the contract.                       |
|                                                                          |
| // \'The AttentionUnit module has:                                       |
|                                                                          |
| // - a TLM pipelined socket (initiator) on MemBusTlm for weight reads    |
|                                                                          |
| // - a blocking socket (target) on HostBus for register-mapped control   |
|                                                                          |
| // - ports in_valid/in_token (QKVToken)/out_valid/out_score (SInt\<16\>) |
|                                                                          |
| // - parameter D_K: const (key dimension)                                |
|                                                                          |
| // - parameter SEQ_LEN: const (max sequence length)\',                   |
|                                                                          |
| // The AI generates the full module skeleton with todo! bodies.          |
|                                                                          |
| // The designer fills in or iterates on the todo! blocks.                |
+--------------------------------------------------------------------------+

**Pattern 3 --- todo! Scaffolding**

+-------------------------------------------------------------------------+
| *todo_scaffold_pattern*                                                 |
|                                                                         |
| // Use todo! to get a compilable skeleton quickly.                      |
|                                                                         |
| // Ask the AI to generate the structure, not the full implementation.   |
|                                                                         |
| // \'Generate the Arch skeleton for a 5-stage RISC-V integer pipeline.  |
|                                                                         |
| // Use todo! for all stage bodies and hazard logic.                     |
|                                                                         |
| // I will fill in the stages myself.\'                                  |
|                                                                         |
| // The AI generates:                                                    |
|                                                                         |
| **pipeline** RiscVPipe                                                  |
|                                                                         |
| **param** XLEN: **const** = 32;                                         |
|                                                                         |
| **port** clk: **in** Clock\<SysDomain\>;                                |
|                                                                         |
| **port** rst: **in** Reset\<Sync\>;                                     |
|                                                                         |
| // \... ports \...                                                      |
|                                                                         |
| **stage** Fetch                                                         |
|                                                                         |
| **todo**!                                                               |
|                                                                         |
| **end** **stage** Fetch                                                 |
|                                                                         |
| **stage** Decode                                                        |
|                                                                         |
| **todo**!                                                               |
|                                                                         |
| **end** **stage** Decode                                                |
|                                                                         |
| // \... 3 more stages \...                                              |
|                                                                         |
| **stall** **when** **todo**!;                                           |
|                                                                         |
| **forward** **todo**! **from** **todo**! **when** **todo**!;            |
|                                                                         |
| **end** **pipeline** RiscVPipe                                          |
|                                                                         |
| // This compiles. The designer iterates on each todo! independently.    |
|                                                                         |
| // Each stage can be handed back to the AI with its own focused prompt. |
+-------------------------------------------------------------------------+

**Pattern 4 --- Compiler-Error-as-Prompt**

+------------------------------------------------------------------------------+
| *compiler_error_pattern*                                                     |
|                                                                              |
| // Paste the full compiler output as the next prompt.                        |
|                                                                              |
| // No spec lookup needed --- the error is self-contained.                    |
|                                                                              |
| // Designer prompt:                                                          |
|                                                                              |
| // \'Fix this Arch compiler error:                                           |
|                                                                              |
| //                                                                           |
|                                                                              |
| // ERROR AttentionUnit.arch:47                                               |
|                                                                              |
| // cdc_crossing: signal score_raw crosses from ComputeDomain to OutputDomain |
|                                                                              |
| // without a declared crossing block.                                        |
|                                                                              |
| // Add: crossing compute_to_output { from ComputeDomain to OutputDomain      |
|                                                                              |
| // data score_raw -\> score_sync } or register score_raw in OutputDomain.\'  |
|                                                                              |
| // The AI reads the error, identifies the missing crossing block,            |
|                                                                              |
| // and inserts it at the correct location.                                   |
|                                                                              |
| // It does not need to look up CDC rules --- the error contains them.        |
+------------------------------------------------------------------------------+

**25.5 The Self-Correction Loop**

When the AI generates Arch with errors, the compiler output is sufficient for self-correction in the majority of cases. Arch error messages are designed with AI consumers in mind --- they name the construct, the exact signal, the type mismatch, and suggest the fix.

+----------------------------------------------------------------------------------------------+
| *compiler_errors*                                                                            |
|                                                                                              |
| // Examples of Arch compiler errors that are self-sufficient for AI correction:              |
|                                                                                              |
| // Type width mismatch:                                                                      |
|                                                                                              |
| // ERROR SystolicPE.arch:23                                                                  |
|                                                                                              |
| // reg acc_reg: SInt\<32\> += SInt\<8\> \* SInt\<8\>                                         |
|                                                                                              |
| // Product SInt\<16\> cannot be assigned to SInt\<32\> without explicit extension.           |
|                                                                                              |
| // Use: (a.sext\<32\>() \* b.sext\<32\>()).trunc\<32\>() or sext both operands first.        |
|                                                                                              |
| // Missing port connection:                                                                  |
|                                                                                              |
| // ERROR SystolicArray.arch:41                                                               |
|                                                                                              |
| // inst pe\[3\]: SystolicPE --- port sum_in not connected.                                   |
|                                                                                              |
| // Expected: sum_in \<- pe\[2\].sum_out (or boundary value for i=0)                  |
|                                                                                              |
| // CDC violation:                                                                            |
|                                                                                              |
| // ERROR DmaEngine.arch:78                                                                   |
|                                                                                              |
| // Signal req_valid: domain WriteDomain drives register in ReadDomain.                       |
|                                                                                              |
| // Add a crossing block or use a fifo with wr_clk / rd_clk ports.                            |
|                                                                                              |
| // Unreachable complete in rtl_accurate implement block:                                     |
|                                                                                              |
| // WARNING Axi4Transactor.arch:112                                                           |
|                                                                                              |
| // implement axi.read --- \'complete future\' not reachable when rvalid is never asserted.   |
|                                                                                              |
| // Ensure complete future with rdata appears on all non-reset paths.                         |
|                                                                                              |
| // Each error contains: location, construct, signal names, type information, fix suggestion. |
|                                                                                              |
| // The AI can apply the fix without accessing the specification.                             |
+----------------------------------------------------------------------------------------------+

**25.6 Scaling Complexity --- One Construct at a Time**

Large hardware systems in Arch are built by composing constructs. An AI assistant is most effective when each prompt targets one construct. This matches the language\'s structure and keeps each prompt focused and verifiable.

+----------------------------------------------------------------------------------+
| *incremental_prompts*                                                            |
|                                                                                  |
| // Building a full AI accelerator --- prompt sequence:                           |
|                                                                                  |
| // Prompt 1: \'Generate the QKVToken struct and ScoreResult struct\'             |
|                                                                                  |
| // Prompt 2: \'Generate the WeightCache ram construct, 256 entries, UInt\<8\>\'  |
|                                                                                  |
| // Prompt 3: \'Generate the SystolicPE module --- single MAC, SInt\<8\> inputs\' |
|                                                                                  |
| // Prompt 4: \'Generate the SystolicArray pipeline using SystolicPE with         |
|                                                                                  |
| // generate for SIZE PEs\'                                                       |
|                                                                                  |
| // Prompt 5: \'Generate the ScoreScaler pipeline --- dot product + RECIP_SQRT\'  |
|                                                                                  |
| // Prompt 6: \'Generate the AttentionUnit module connecting the above\'          |
|                                                                                  |
| // Prompt 7: \'Generate the testbench for AttentionUnit with 3 test sequences\'  |
|                                                                                  |
| // Each prompt generates one construct.                                          |
|                                                                                  |
| // Each construct is compiled independently before the next is started.          |
|                                                                                  |
| // Errors are caught and fixed at the smallest possible scope.                   |
|                                                                                  |
| // The AI reference card stays in context throughout.                            |
+----------------------------------------------------------------------------------+

> ◈ This one-construct-per-prompt discipline is not a limitation --- it is the correct way to design hardware. Each construct is a verified unit. Composing verified units produces a correct system faster than generating the entire system at once and debugging globally.

**25.7 Abstraction-Level Progression**

For complex blocks, the AI workflow proceeds through abstraction levels --- from TLM behavioral to cycle-accurate RTL --- using the same prompting discipline:

+-----------------------------------------------------------------------------------+
| *abstraction_progression*                                                         |
|                                                                                   |
| // Level 1 --- TLM skeleton (fastest to generate, fastest to simulate)            |
|                                                                                   |
| // Prompt: \'Generate the AttentionUnit module with TLM pipelined sockets.        |
|                                                                                   |
| // Use todo! for all internal logic.\'                                            |
|                                                                                   |
| // Purpose: verify the API contract and testbench before building internals.      |
|                                                                                   |
| // Level 2 --- Behavioral RTL (correct function, no timing)                       |
|                                                                                   |
| // Prompt: \'Replace the todo! in the Compute stage with a behavioral             |
|                                                                                   |
| // implementation of scaled dot-product. Timing does not matter yet.\'            |
|                                                                                   |
| // Purpose: verify functional correctness with fast simulation.                   |
|                                                                                   |
| // Level 3 --- Pipelined RTL (correct timing, pipeline structure)                 |
|                                                                                   |
| // Prompt: \'Rework the Compute stage as a 3-stage pipeline:                      |
|                                                                                   |
| // stage 1: dot product accumulate, stage 2: scale, stage 3: output.\'            |
|                                                                                   |
| // Purpose: verify cycle-level behaviour and latency.                             |
|                                                                                   |
| // Level 4 --- Resource-optimised (FPGA/ASIC target)                              |
|                                                                                   |
| // Prompt: \'Optimise the dot product stage for Xilinx UltraScale DSP48E2 blocks. |
|                                                                                   |
| // Map the SInt\<8\>\*SInt\<8\> multiplies to DSP cascade chains.\'               |
|                                                                                   |
| // Purpose: target-specific optimisation from verified baseline.                  |
|                                                                                   |
| // Each level uses arch sim to verify before proceeding.                          |
|                                                                                   |
| // The testbench is identical at all four levels.                                 |
+-----------------------------------------------------------------------------------+

**24. Bus --- Reusable Port Bundles**

A bus is a named, parameterized port bundle reusable across module, pipeline, fifo, and arbiter boundaries. Buses eliminate the repetitive port declarations that make large designs brittle. Signal directions are declared from the **initiator** perspective; the compiler automatically flips directions for the **target** side.

**24.1 Declaring and Using Buses**

+--------------------------------------------------------------------------------+
| *bus_axilite.arch*                                                             |
|                                                                                |
| /// AXI4-Lite bus bundle                                                       |
|                                                                                |
| **bus** AxiLite                                                                |
|   **param** ADDR_W: **const** = 32;                                            |
|   **param** DATA_W: **const** = 32;                                            |
|                                                                                |
|   aw_valid: **out** Bool;                                                      |
|   aw_ready: **in** Bool;                                                       |
|   aw_addr:  **out** UInt\<ADDR_W\>;                                            |
|   w_valid:  **out** Bool;                                                      |
|   w_ready:  **in** Bool;                                                       |
|   w_data:   **out** UInt\<DATA_W\>;                                            |
|   b_valid:  **in** Bool;                                                       |
|   b_ready:  **out** Bool;                                                      |
|   b_resp:   **in** UInt\<2\>;                                                  |
| **end** **bus** AxiLite                                                        |
+--------------------------------------------------------------------------------+

Bus signals are declared without the `port` keyword --- they are bundle members, not module ports.

**24.2 Using Buses in Modules**

+--------------------------------------------------------------------------------+
| *bus_usage.arch*                                                               |
|                                                                                |
| **module** Master                                                              |
|   **port** clk: **in** Clock\<SysDomain\>;                                     |
|   **port** rst: **in** Reset\<Sync\>;                                          |
|   **port** axi: **initiator** AxiLite\<ADDR_W=32, DATA_W=64\>;                |
|                                                                                |
|   **comb**                                                                     |
|     axi.aw_valid = 1;                                                          |
|     axi.aw_addr  = addr_r;                                                     |
|     axi.w_valid  = 1;                                                          |
|     axi.w_data   = data_r;                                                     |
|     axi.b_ready  = 1;                                                          |
|   **end** **comb**                                                             |
| **end** **module** Master                                                      |
|                                                                                |
| **module** Slave                                                               |
|   **port** clk: **in** Clock\<SysDomain\>;                                     |
|   **port** rst: **in** Reset\<Sync\>;                                          |
|   **port** axi: **target** AxiLite\<ADDR_W=32, DATA_W=64\>;                   |
|                                                                                |
|   **comb**                                                                     |
|     axi.aw_ready = 1;                                                          |
|     axi.w_ready  = 1;                                                          |
|     axi.b_valid  = 1;                                                          |
|     axi.b_resp   = 0;                                                          |
|   **end** **comb**                                                             |
| **end** **module** Slave                                                       |
+--------------------------------------------------------------------------------+

- **initiator** keeps signal directions as declared in the bus (out stays out, in stays in).
- **target** flips all directions (out becomes in, in becomes out).
- Bus signals are accessed with dot notation: `axi.aw_valid`, `axi.b_resp`.
- In SV codegen, bus ports are flattened to individual ports: `axi.aw_valid` → `axi_aw_valid`.
- Bus parameters are passed with angle-bracket syntax: `AxiLite<ADDR_W=32, DATA_W=64>`.

> *⚑ Declaring a port as **initiator** BusName keeps all bus outputs driving outward and all bus inputs expecting inward. Using **target** flips every direction automatically --- no manual rewiring needed for the subordinate side.*

**24A. Templates --- Interface Contracts**

A **template** is a compile-time-only construct that defines a contract: a set of required params, ports, and hooks that any implementing module must provide. Templates emit no SystemVerilog --- they exist purely to enforce structural conformance across modules.

**24A.1 Declaring a Template**

+--------------------------------------------------------------------------------+
| *arbiter_template.arch*                                                        |
|                                                                                |
| **template** Arbiter                                                           |
|   **param** NUM_REQ: **const**;                                                |
|   **port** clk:         **in** Clock\<SysDomain\>;                             |
|   **port** rst:         **in** Reset\<Sync\>;                                  |
|   **port** grant_valid: **out** Bool;                                          |
|   **hook** grant_select(req_mask: UInt\<4\>) -\> UInt\<4\>;                    |
| **end** **template** Arbiter                                                   |
+--------------------------------------------------------------------------------+

A template body contains only declarations --- params (without defaults), ports, and hook signatures. No logic, no registers, no `comb`/`seq` blocks.

**24A.2 Implementing a Template**

A module opts into a template contract with the **implements** keyword:

+--------------------------------------------------------------------------------+
| *my_arbiter.arch*                                                              |
|                                                                                |
| **function** FixedGrant(req_mask: UInt\<4\>) -\> UInt\<4\>                     |
|   **return** req_mask & (\~req_mask + 1).trunc\<4\>();                         |
| **end** **function** FixedGrant                                                |
|                                                                                |
| **module** MyArbiter **implements** Arbiter                                    |
|   **param** NUM_REQ: **const** = 4;                                            |
|   **port** clk:         **in** Clock\<SysDomain\>;                             |
|   **port** rst:         **in** Reset\<Sync\>;                                  |
|   **port** req_mask:    **in** UInt\<4\>;                                      |
|   **port** grant_valid: **out** Bool;                                          |
|   **port** grant_out:   **out** UInt\<4\>;                                     |
|                                                                                |
|   **hook** grant_select(req_mask: UInt\<4\>) -\> UInt\<4\>                     |
|     = FixedGrant(req_mask);                                                    |
|                                                                                |
|   **let** grant: UInt\<4\> = FixedGrant(req_mask);                             |
|                                                                                |
|   **comb**                                                                     |
|     grant_valid = grant != 0;                                                  |
|     grant_out = grant;                                                         |
|   **end** **comb**                                                             |
| **end** **module** MyArbiter                                                   |
+--------------------------------------------------------------------------------+

**24A.3 Compiler Checks**

The compiler validates that every implementing module satisfies the template contract:

  -------------------------------------------------------------------------------------------
  **Check**                         **Error if**
  --------------------------------- ---------------------------------------------------------
  **Param presence**                A template param is missing from the implementing module

  **Port presence and direction**   A template port is missing or has the wrong direction/type

  **Hook binding**                  A template hook is declared but no binding appears in the module
  -------------------------------------------------------------------------------------------

The implementing module may declare additional params, ports, and logic beyond what the template requires --- the template is a minimum contract, not an exhaustive specification.

> *⚑ Templates are the Arch equivalent of traits or interfaces in software languages. They let library authors define reusable contracts (e.g. "any arbiter must expose grant_valid and provide a grant_select hook") while leaving implementation freedom to the designer.*

**25. Lightweight Verification Built-ins**

Arch includes three verification constructs built directly into the language. They use the same types, signals, and naming as the design --- there is no separate verification sublanguage to learn.

**12.1 assert --- Invariant Checking**

+--------------------------------------------------------------------+
| *assert.arch*                                                      |
|                                                                    |
| **module** SafeAdder                                               |
|                                                                    |
| **port** a: **in** UInt\<8\>;                                      |
|                                                                    |
| **port** b: **in** UInt\<8\>;                                      |
|                                                                    |
| **port** sum: **out** UInt\<9\>;                                   |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| sum = a.zext\<9\>() + b.zext\<9\>();                               |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| // Fires in simulation; formal tools prove it statically           |
|                                                                    |
| **assert** no_overflow: a.zext\<9\>() + b.zext\<9\>() \<= 9\'h0FF; |
|                                                                    |
| **end** **module** SafeAdder                                       |
+--------------------------------------------------------------------+

**12.2 cover --- Reachability**

+--------------------------------------------------------------------+
| *cover.arch*                                                       |
|                                                                    |
| **fifo** TxQueue                                                   |
|                                                                    |
| // \... ports as before \...                                       |
|                                                                    |
| **cover** full_seen: **full** == true;                             |
|                                                                    |
| **cover** empty_seen: **empty** == true;                           |
|                                                                    |
| **end** **fifo** TxQueue                                           |
+--------------------------------------------------------------------+

**12.3 assume --- Formal Constraints**

+--------------------------------------------------------------------+
| *assume.arch*                                                      |
|                                                                    |
| **module** FormalWrapper                                           |
|                                                                    |
| **port** req: **in** UInt\<4\>;                                    |
|                                                                    |
| // Constrain the formal tool: only consider one-hot inputs         |
|                                                                    |
| **assume** one_hot: req == 0 **or** (req & (req - 1)) == 0;        |
|                                                                    |
| **end** **module** FormalWrapper                                   |
+--------------------------------------------------------------------+

  -------------------------------------------------------------------------------------------------------------
  **Keyword**             **Simulation**   **Formal Tool**      **Meaning**
  ----------------------- ---------------- -------------------- -----------------------------------------------
  **assert name: expr**   Error if false   Property to prove    Invariant --- must always hold

  **cover name: expr**    Log when true    Reachability check   Confirms a scenario is reachable

  **assume name: expr**   Ignored          Input constraint     Restricts the input space for formal analysis
  -------------------------------------------------------------------------------------------------------------

**26. AI-Generatability Design Rationale**

This section documents the explicit language decisions made so that any LLM --- without hardware-specific fine-tuning --- can produce correct Arch from a plain-English description.

**13.1 Uniform Declaration Schema**

Every first-class construct uses the same four-section layout. The AI applies one template universally:

+-----------------------------------------------------------------------------+
| *universal_template.arch*                                                   |
|                                                                             |
| // Valid for: module, pipeline, fsm, fifo, arbiter, regfile, bus            |
|                                                                             |
| keyword Name                                                                |
|                                                                             |
| **param** NAME: **const** = value; // section 1: compile-time parameters    |
|                                                                             |
| **param** NAME: **type** = SomeType;                                        |
|                                                                             |
| **port** name: **in** Type; // section 2: ports (direction always explicit) |
|                                                                             |
| **port** name: **out** Type;                                                |
|                                                                             |
| **port** name: **inout** Type;                                              |
|                                                                             |
| // section 3: construct-specific body                                       |
|                                                                             |
| **assert** name: expr; // section 4: verification (optional)                |
|                                                                             |
| **cover** name: expr;                                                       |
|                                                                             |
| **end** keyword Name                                                        |
+-----------------------------------------------------------------------------+

**13.2 Named Endings Prevent Structural Hallucination**

The most common failure mode in LLM-generated code is incorrect nesting --- closing the wrong block or leaving one open. Named endings make this a hard compiler error:

+------------------------------------------------------------------------+
| *named_endings.arch*                                                   |
|                                                                        |
| // The compiler matches each ending to its opener by keyword AND name. |
|                                                                        |
| // Mismatches are errors. The AI always knows exactly where it is.     |
|                                                                        |
| **end** **stage** Fetch                                                |
|                                                                        |
| **end** **pipeline** Decode                                            |
|                                                                        |
| **end** **module** Core                                                |
|                                                                        |
| // Compare with traditional HDLs where all closers look identical:     |
|                                                                        |
| // end end end endmodule ← which block does each close?                |
+------------------------------------------------------------------------+

**13.3 Directional Connect Arrows Encode Data Flow**

In Verilog, signal assignment direction is determined by context --- LLMs frequently confuse this. In Arch, every port connection encodes direction explicitly:

+-----------------------------------------------------------------------------------+
| *connect_arrows.arch*                                                             |
|                                                                                   |
| **data_in \<- local_signal; // \<- drives an input FROM a local signal  |
|                                                                                   |
| **data_out -\> local_signal; // -\> reads an output INTO a local signal |
|                                                                                   |
| // Direction is visible in the syntax itself.                                     |
|                                                                                   |
| // An AI cannot silently reverse data flow.                                       |
+-----------------------------------------------------------------------------------+

**13.4 todo! Enables Incremental AI-Assisted Design**

A practical AI workflow: generate a correct skeleton with todo! for all logic, then fill in logic section by section. Every intermediate state compiles and type-checks.

+--------------------------------------------------------------------+
| *todo_workflow.arch*                                               |
|                                                                    |
| // Step 1: AI generates skeleton --- compiles immediately          |
|                                                                    |
| **pipeline** RiscVCore                                             |
|                                                                    |
| **param** XLEN: **const** = 32;                                    |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** imem_addr: **out** UInt\<XLEN\>;                          |
|                                                                    |
| **port** imem_data: **in** UInt\<32\>;                             |
|                                                                    |
| **stage** Fetch                                                    |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| imem_addr = **todo**!;                                             |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **stage** Fetch                                            |
|                                                                    |
| **stage** Decode                                                   |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| **todo**!;                                                         |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **stage** Decode                                           |
|                                                                    |
| **stage** Execute                                                  |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| **todo**!;                                                         |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **stage** Execute                                          |
|                                                                    |
| **end** **pipeline** RiscVCore                                     |
|                                                                    |
| // Step 2: fill in Fetch --- recompile, types checked              |
|                                                                    |
| // Step 3: fill in Decode --- recompile, etc.                      |
+--------------------------------------------------------------------+

**13.5 No Implicit Behaviour --- Full Explicitness Table**

  --------------------------------------------------------------------------------------------------------------
  **Traditional HDL implicit**                     **Arch explicit equivalent**
  ------------------------------------------------ -------------------------------------------------------------
  **Undriven output defaults to high-Z**           Compile error --- every output must have exactly one driver

  **reg with no reset holds unknown state**        Reset value required: reg x: UInt\<8\> reset rst => 0;

  **Wire driven by last assignment wins**          Compile error --- single-driver rule enforced statically

  **Unsigned/signed determined by context**        Explicit: .zext\<N\>() .sext\<N\>() as SInt\<N\>

  **Clock inferred from sensitivity list**         Explicit: seq on clk rising; reset on reg decl: reset rst => 0 sync high

  **Module port width from implicit param math**   Explicit: port sum: out UInt\<WIDTH+1\>;

  **CDC implicitly allowed across assignments**    Compile error --- crossing block required
  --------------------------------------------------------------------------------------------------------------

**27. Compilation and Output**

**14.1 Compiler Pipeline**

  ---------------------------------------------------------------------------------------------------------
  **Phase**                 **Key Checks and Actions**
  ------------------------- -------------------------------------------------------------------------------
  **Parse**                 Syntax, named-block matching

  **Elaboration**           Parameter resolution, generic instantiation, type expansion

  **Type Check**            Bit-width safety, clock domain tracking, direction safety, single-driver rule

  **Micro-Arch Lowering**   Pipeline hazard generation, FIFO implementation, arbiter logic, FSM encoding

  **Verification Emit**     assert/cover/assume converted to SystemVerilog Assertions (SVA)

  **SV Emit**               One deterministic, lint-clean SystemVerilog file per Arch top-level module
  ---------------------------------------------------------------------------------------------------------

**14.2 Output Guarantee**

The generated SystemVerilog is guaranteed to contain none of the following:

- Latches --- all state is explicit reg with reset value.

- Multiply-driven nets --- enforced by the single-driver rule.

- Unresolved high-Z outputs --- every output port has exactly one driver.

- X-propagation from uninitialised state --- all reg declarations require a reset value (via `reset SIGNAL=>VALUE`).

- Implicit clock-domain crossings --- all CDCs are declared and synchroniser-wrapped.

**14.2.1 Compiler Warnings**

The compiler emits warnings (non-fatal, printed before "OK: no errors") for the following:

- **`todo!` sites** --- every `todo!` expression or block body emits a warning. Simulation aborts if one is reached at runtime.

- **Redundant reset branch** --- a `seq` block whose top-level `if` tests a reset signal that also appears in a `reset signal=>value` declaration. The `if` branch is dead code because the declaration already generates an outer reset guard:

  ```
  reg q: UInt<8> reset rst => 0;
  seq on clk rising
    if rst          // WARNING: redundant — dead inside the outer reset guard
      q <= 0;
    else
      q <= d;
    end if
  end seq
  ```

  Correct form:

  ```
  seq on clk rising
    q <= d;         // reset guard is generated from the declaration
  end seq
  ```

> *⚑ The output guarantee means synthesis tools receive RTL that passes lint clean with no attributes, no translate_off pragmas, and no synthesis workarounds required.*

**14.3 Toolchain Targets**

  ------------------------------------------------------------------------------------------------------------------------------
  **Target**                        **Command**                                  **Notes**
  --------------------------------- -------------------------------------------- -----------------------------------------------
  **SystemVerilog (ASIC)**          arch build \--target asic                    IEEE 1800-2017 output, no vendor primitives

  **SystemVerilog (FPGA Xilinx)**   arch build \--target fpga \--vendor xilinx   Inserts BRAM/DSP primitives where appropriate

  **SystemVerilog (FPGA Intel)**    arch build \--target fpga \--vendor intel    Inserts Intel BRAM/DSP primitives

  **Formal verification**           arch build \--target formal                  SVA + SymbiYosys script

  **Simulation**                    arch sim \--tb MyTb                          Compiles with Verilator or ModelSim/Questa

  **Documentation**                 arch doc                                     HTML reference from /// doc comments
  ------------------------------------------------------------------------------------------------------------------------------

**28. Complete Example: 3-Stage RISC-V Integer Pipeline**

The following example shows all major Arch constructs working together --- module, pipeline with stall and forward, ram with multi-variable mapping, fifo, arbiter, and verification. No braces appear anywhere. Every block is opened by its keyword and name, and closed by end keyword name.

Design: a 3-stage in-order RISC-V integer pipeline with a unified register + CSR RAM (multi-variable mapping), a shared memory bus arbitrated between instruction fetch and data memory, and a 4-entry instruction queue between fetch and decode.

**28.1 Shared Types and Domains**

+--------------------------------------------------------------------+
| *types.arch*                                                       |
|                                                                    |
| **domain** SysDomain                                               |
|                                                                    |
| freq_mhz: 100                                                     |
|                                                                    |
| **end** **domain** SysDomain                                       |
|                                                                    |
| **struct** DecodedInstr                                            |
|                                                                    |
| opcode: UInt\<7\>,                                                 |
|                                                                    |
| rd: UInt\<5\>,                                                     |
|                                                                    |
| rs1: UInt\<5\>,                                                    |
|                                                                    |
| rs2: UInt\<5\>,                                                    |
|                                                                    |
| funct3: UInt\<3\>,                                                 |
|                                                                    |
| imm: SInt\<32\>,                                                   |
|                                                                    |
| alu_op: AluOp,                                                     |
|                                                                    |
| **end** **struct** DecodedInstr                                    |
|                                                                    |
| **enum** AluOp                                                     |
|                                                                    |
| Add, Sub, And, Or, Xor, Sll, Srl, Sra, Slt, Sltu,                  |
|                                                                    |
| **end** **enum** AluOp                                             |
+--------------------------------------------------------------------+

**28.2 Unified Register and CSR RAM**

+-----------------------------------------------------------------------------+
| *unified_regs.arch*                                                         |
|                                                                             |
| // A single physical RAM holds integer registers and CSRs.                  |
|                                                                             |
| // The compiler assigns non-overlapping address ranges automatically:       |
|                                                                             |
| // int_regs → 0x000 .. 0x01F (32 entries × 32 bits)                         |
|                                                                             |
| // csr_regs → 0x020 .. 0x41F (1024 entries × 32 bits)                       |
|                                                                             |
| // Total: depth = 1056, word width = 32 bits                                |
|                                                                             |
| //                                                                          |
|                                                                             |
| // Designers access int_regs\[rs1\] and csr_regs\[addr\] by name.           |
|                                                                             |
| // The compiler emits physical_base + index in the generated SystemVerilog. |
|                                                                             |
| ram UnifiedRegs                                                             |
|                                                                             |
| **port** clk: **in** Clock\<SysDomain\>;                                    |
|                                                                             |
| **port** rst: **in** Reset\<Sync\>;                                         |
|                                                                             |
| kind simple_dual;                                                           |
|                                                                             |
| **read**: **async**; // combinational read --- register-file behaviour      |
|                                                                             |
| store                                                                       |
|                                                                             |
| int_regs: Vec\<UInt\<32\>, 32\>;                                            |
|                                                                             |
| csr_regs: Vec\<UInt\<32\>, 1024\>;                                          |
|                                                                             |
| **end** store                                                               |
|                                                                             |
| **port** read_port                                                          |
|                                                                             |
| en: **in** Bool;                                                            |
|                                                                             |
| addr: **in** UInt\<\$clog2(1056)\>;                                         |
|                                                                             |
| data: **out** UInt\<32\>;                                                   |
|                                                                             |
| **end** **port** read_port                                                  |
|                                                                             |
| **port** write_port                                                         |
|                                                                             |
| en: **in** Bool;                                                            |
|                                                                             |
| addr: **in** UInt\<\$clog2(1056)\>;                                         |
|                                                                             |
| data: **in** UInt\<32\>;                                                    |
|                                                                             |
| **end** **port** write_port                                                 |
|                                                                             |
| **init**: zero; // x0 and all CSRs initialise to zero on reset              |
|                                                                             |
| **end** ram UnifiedRegs                                                     |
+-----------------------------------------------------------------------------+

**28.3 Memory Bus and Supporting Constructs**

+--------------------------------------------------------------------+
| *support.arch*                                                     |
|                                                                    |
| **bus** MemBus                                                     |
|                                                                    |
| **valid**: **out** Bool;                                           |
|                                                                    |
| **ready**: **in** Bool;                                            |
|                                                                    |
| addr: **out** UInt\<32\>;                                          |
|                                                                    |
| rdata: **in** UInt\<32\>;                                          |
|                                                                    |
| wdata: **out** UInt\<32\>;                                         |
|                                                                    |
| wen: **out** Bool;                                                 |
|                                                                    |
| **end** **bus** MemBus                                             |
|                                                                    |
| // Priority arbiter --- Fetch (port 0) beats LSU (port 1)          |
|                                                                    |
| **arbiter** MemArbiter                                             |
|                                                                    |
| **param** NUM_REQ: **const** = 2;                                  |
|                                                                    |
| **param** NUM_RSRC: **const** = 1;                                 |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| ports\[NUM_REQ\] **request**                                       |
|                                                                    |
| **valid**: **in** Bool;                                            |
|                                                                    |
| **ready**: **out** Bool;                                           |
|                                                                    |
| **end** ports                                                      |
|                                                                    |
| ports\[NUM_RSRC\] **grant**                                        |
|                                                                    |
| **valid**: **out** Bool;                                           |
|                                                                    |
| requester: **out** UInt\<1\>;                                      |
|                                                                    |
| **end** ports                                                      |
|                                                                    |
| **policy** priority;                                               |
|                                                                    |
| **end** **arbiter** MemArbiter                                     |
|                                                                    |
| // 4-entry instruction queue between Fetch and Decode stages       |
|                                                                    |
| **fifo** InstrQueue                                                |
|                                                                    |
| **param** DEPTH: **const** = 4;                                    |
|                                                                    |
| **param** WIDTH: **type** = UInt\<32\>;                            |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** push_valid: **in** Bool;                                  |
|                                                                    |
| **port** push_ready: **out** Bool;                                 |
|                                                                    |
| **port** push_data: **in** UInt\<32\>;                             |
|                                                                    |
| **port** pop_valid: **out** Bool;                                  |
|                                                                    |
| **port** pop_ready: **in** Bool;                                   |
|                                                                    |
| **port** pop_data: **out** UInt\<32\>;                             |
|                                                                    |
| **port** **full**: **out** Bool;                                   |
|                                                                    |
| **port** **empty**: **out** Bool;                                  |
|                                                                    |
| **end** **fifo** InstrQueue                                        |
+--------------------------------------------------------------------+

**28.4 The 3-Stage Pipeline**

+--------------------------------------------------------------------+
| *pipeline.arch*                                                    |
|                                                                    |
| **pipeline** RiscVPipe                                             |
|                                                                    |
| **param** XLEN: **const** = 32;                                    |
|                                                                    |
| **port** clk: **in** Clock\<SysDomain\>;                           |
|                                                                    |
| **port** rst: **in** Reset\<Sync\>;                                |
|                                                                    |
| **port** imem: **out** MemBus                                      |
|                                                                    |
| **end** **port** imem                                              |
|                                                                    |
| **port** dmem: **out** MemBus                                      |
|                                                                    |
| **end** **port** dmem                                              |
|                                                                    |
| // Register file read results arrive combinationally (async RAM)   |
|                                                                    |
| **port** rs1_val: **in** UInt\<XLEN\>;                             |
|                                                                    |
| **port** rs2_val: **in** UInt\<XLEN\>;                             |
|                                                                    |
| **port** rd_wen: **out** Bool;                                     |
|                                                                    |
| **port** rd_addr: **out** UInt\<5\>;                               |
|                                                                    |
| **port** rd_data: **out** UInt\<XLEN\>;                            |
|                                                                    |
| **stage** Fetch                                                    |
|                                                                    |
| **reg** pc: UInt\<32\> **init** 0;                                 |
|                                                                    |
| **reg** **on** clk rising, rst high                                |
|                                                                    |
| **if** rst                                                         |
|                                                                    |
| pc \<= 0;                                                          |
|                                                                    |
| **end** **if**                                                     |
|                                                                    |
| **else**                                                           |
|                                                                    |
| pc \<= pc + 4;                                                     |
|                                                                    |
| **end** **else**                                                   |
|                                                                    |
| **end** **reg**                                                    |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| imem.addr = pc;                                                    |
|                                                                    |
| imem.**valid** = true;                                             |
|                                                                    |
| imem.wen = false;                                                  |
|                                                                    |
| imem.wdata = 0;                                                    |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **stage** Fetch                                            |
|                                                                    |
| **stage** Decode                                                   |
|                                                                    |
| **let** raw: UInt\<32\> = imem.rdata;                              |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| **todo**!; // instruction decode --- fills DecodedInstr fields     |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **stage** Decode                                           |
|                                                                    |
| **stage** Execute                                                  |
|                                                                    |
| **comb**                                                           |
|                                                                    |
| **todo**!; // ALU, branch resolution, LSU address generation       |
|                                                                    |
| **end** **comb**                                                   |
|                                                                    |
| **end** **stage** Execute                                          |
|                                                                    |
| **stall** **when** iq_full == true;                                |
|                                                                    |
| **forward** Decode.rs1_val **from** Execute.rd_data                |
|                                                                    |
| **when** Execute.instr.rd == Decode.instr.rs1                      |
|                                                                    |
| **and** Execute.instr.rd != 0;                                     |
|                                                                    |
| **forward** Decode.rs2_val **from** Execute.rd_data                |
|                                                                    |
| **when** Execute.instr.rd == Decode.instr.rs2                      |
|                                                                    |
| **and** Execute.instr.rd != 0;                                     |
|                                                                    |
| **assert** pc_aligned: Fetch.pc\[1:0\] == 2\'b00;                  |
|                                                                    |
| **cover** fetch_stall: iq_full == true;                            |
|                                                                    |
| **end** **pipeline** RiscVPipe                                     |
+--------------------------------------------------------------------+

**28.5 Top-Level Module**

+------------------------------------------------------------------------+
| *top.arch*                                                             |
|                                                                        |
| **module** RiscVCore                                                   |
|                                                                        |
| **port** clk: **in** Clock\<SysDomain\>;                               |
|                                                                        |
| **port** rst: **in** Reset\<Sync\>;                                    |
|                                                                        |
| **port** imem: **out** MemBus                                          |
|                                                                        |
| **end** **port** imem                                                  |
|                                                                        |
| **port** dmem: **out** MemBus                                          |
|                                                                        |
| **end** **port** dmem                                                  |
|                                                                        |
| // ── Unified register + CSR RAM ───────────────────────────────       |
|                                                                        |
| **inst** regs: UnifiedRegs                                             |
|                                                                        |
| **clk \<- clk;                                               |
|                                                                        |
| **rst \<- rst;                                               |
|                                                                        |
| // Read port: address is either int_regs\[rs1\] or csr_regs\[csr_idx\] |
|                                                                        |
| // Compiler translates logical names to physical addresses             |
|                                                                        |
| **read_port.en \<- true;                                     |
|                                                                        |
| **read_port.addr \<- int_regs\[rs1_addr\];                   |
|                                                                        |
| **read_port.data -\> rs1_val;                                |
|                                                                        |
| // Write port: writeback from execute stage                            |
|                                                                        |
| **write_port.en \<- rd_wen;                                  |
|                                                                        |
| **write_port.addr \<- int_regs\[rd_addr\];                   |
|                                                                        |
| **write_port.data \<- rd_data;                               |
|                                                                        |
| **end** **inst** regs                                                  |
|                                                                        |
| // ── Instruction queue ────────────────────────────────────────       |
|                                                                        |
| **inst** iq: InstrQueue                                                |
|                                                                        |
| **param** DEPTH = 4;                                                   |
|                                                                        |
| **param** WIDTH = UInt\<32\>;                                          |
|                                                                        |
| **clk \<- clk;                                               |
|                                                                        |
| **rst \<- rst;                                               |
|                                                                        |
| **push_valid \<- imem.**valid**;                             |
|                                                                        |
| **push_data \<- imem.rdata;                                  |
|                                                                        |
| **push_ready -\> imem.**ready**;                             |
|                                                                        |
| **pop_ready \<- true;                                        |
|                                                                        |
| ****full** -\> iq_full;                                      |
|                                                                        |
| **end** **inst** iq                                                    |
|                                                                        |
| // ── Memory bus arbiter ───────────────────────────────────────       |
|                                                                        |
| **inst** arb: MemArbiter                                               |
|                                                                        |
| **param** NUM_REQ = 2;                                                 |
|                                                                        |
| **param** NUM_RSRC = 1;                                                |
|                                                                        |
| **clk \<- clk;                                               |
|                                                                        |
| **rst \<- rst;                                               |
|                                                                        |
| ****request**\[0\].**valid** \<- imem.**valid**;             |
|                                                                        |
| ****request**\[0\].**ready** -\> imem.**ready**;             |
|                                                                        |
| ****request**\[1\].**valid** \<- dmem.**valid**;             |
|                                                                        |
| ****request**\[1\].**ready** -\> dmem.**ready**;             |
|                                                                        |
| ****grant**\[0\].**valid** -\> bus_grant;                    |
|                                                                        |
| ****grant**\[0\].requester -\> bus_winner;                   |
|                                                                        |
| **end** **inst** arb                                                   |
|                                                                        |
| // ── Pipeline ─────────────────────────────────────────────────       |
|                                                                        |
| **inst** pipe: RiscVPipe                                               |
|                                                                        |
| **param** XLEN = 32;                                                   |
|                                                                        |
| **clk \<- clk;                                               |
|                                                                        |
| **rst \<- rst;                                               |
|                                                                        |
| **imem -\> imem;                                             |
|                                                                        |
| **dmem -\> dmem;                                             |
|                                                                        |
| **rs1_val \<- rs1_val;                                       |
|                                                                        |
| **rs2_val \<- rs2_val;                                       |
|                                                                        |
| **rd_wen -\> rd_wen;                                         |
|                                                                        |
| **rd_addr -\> rd_addr;                                       |
|                                                                        |
| **rd_data -\> rd_data;                                       |
|                                                                        |
| **end** **inst** pipe                                                  |
|                                                                        |
| **end** **module** RiscVCore                                           |
+------------------------------------------------------------------------+

> ◈ The entire design compiles with two todo! sites as the only incomplete logic. All types are checked, clock domains verified, all ports driven, pipeline hazard annotations structurally correct, and the RAM\'s logical variable mapping fully resolved. An AI fills in the two todo! sites independently without touching anything else.

---

**29. Packages and Imports**

**29.1 The package Construct**

A **package** groups related type definitions, constants, and functions into a reusable namespace. Packages follow the universal block grammar:

> **package** PkgName
>
> // enum, struct, function, and param declarations
>
> **end** **package** PkgName

**What can go inside a package:**

| Item | Example |
|------|---------|
| `enum` | `enum BusOp Read, Write, Idle end enum BusOp` |
| `struct` | `struct BusReq op: BusOp; addr: UInt<32>; end struct BusReq` |
| `function` | `function max(a: UInt<32>, b: UInt<32>) -> UInt<32> ... end function max` |
| `param` | `param BUS_WIDTH: const = 64;` |
| `domain` | `domain FastClk freq_mhz: 500 end domain FastClk` |

Packages currently contain only compile-time definitions (types, functions, constants, domains). A planned extension will allow **package-scoped hardware constructs** (modules, FSMs, pipelines, etc.), enabling namespace-qualified instantiation:

```
package FloatLib
  module Adder
    ...
  end module Adder
end package FloatLib

package FixedLib
  module Adder
    ...
  end module Adder
end package FixedLib

module Top
  inst a: FloatLib::Adder
    ...
  end inst a
  inst b: FixedLib::Adder
    ...
  end inst b
end module Top
```

SV codegen flattens to unique names: `FloatLib_Adder`, `FixedLib_Adder`.

> *⚑ SystemVerilog does not allow modules inside packages --- modules are always in the global namespace, disambiguated only through tool-specific library mapping. ARCH's package-scoped modules provide compile-time namespace resolution without external tool configuration. This is a planned feature; currently all constructs share a flat global namespace.*

**29.2 The use Import**

A consumer file imports a package with the **use** statement at file scope (before any construct declaration):

> **use** PkgName;

This makes all names defined inside the package available unqualified in the importing file. Multiple `use` statements are allowed.

**29.3 File Resolution**

The compiler resolves `use PkgName;` by searching for `PkgName.arch` in the same directory as the importing file. The package file must contain exactly one `package PkgName ... end package PkgName` construct whose name matches the file name.

When using multi-file compilation (`arch build a.arch b.arch`), packages included on the command line are also resolved.

**29.4 Complete Example**

Source file `BusPkg.arch`:

```
package BusPkg
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
```

Consumer file `Consumer.arch`:

```
use BusPkg;

module Consumer
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port req: in BusReq;
  port addr_out: out UInt<32>;

  comb addr_out = req.addr;
end module Consumer
```

**29.5 Generated SystemVerilog**

The compiler emits a standard SV `package` / `import` pair:

```sv
package BusPkg;
  typedef enum logic [1:0] { READ, WRITE, IDLE } BusOp;
  typedef struct packed {
    BusOp op;
    logic [31:0] addr;
    logic [31:0] data;
  } BusReq;
  function automatic logic [31:0] max(input logic [31:0] a, input logic [31:0] b);
    return (a > b) ? a : b;
  endfunction
endpackage

import BusPkg::*;
module Consumer (
  input  wire        clk,
  input  wire        rst,
  input  BusReq      req,
  output logic [31:0] addr_out
);
  assign addr_out = req.addr;
endmodule
```

Each `use PkgName;` in Arch emits one `import PkgName::*;` in SV, placed immediately before the consuming module.

**29.6 Module-Local Functions**

Functions can also be declared inside a module body for one-off helpers that don't warrant a full package:

```
module MyModule
  param WIDTH: const = 8;
  port a: in UInt<WIDTH>;
  port b: in UInt<WIDTH>;
  port sum: out UInt<WIDTH>;

  function add_wrap(x: UInt<WIDTH>, y: UInt<WIDTH>) -> UInt<WIDTH>
    return (x + y).trunc<WIDTH>();
  end function add_wrap

  let sum = add_wrap(a, b);
end module MyModule
```

The compiler emits module-local functions as SV `function automatic` inside the module block:

```sv
module MyModule #(parameter int WIDTH = 8) (input logic [WIDTH-1:0] a, ...);
  function automatic logic [WIDTH-1:0] add_wrap(input logic [WIDTH-1:0] x, input logic [WIDTH-1:0] y);
    return WIDTH'(x + y);
  endfunction
  assign sum = add_wrap(a, b);
endmodule
```

Module-local functions have access to the module's parameters. They are pure combinational (no state, no side effects).

Function bodies support `let`, `return`, `if/elsif/else`, `for` loops, and local variable assignment (`=`):

```
function popcount(x: UInt<8>) -> UInt<4>
  let result: UInt<4> = 0;
  for i in 0..7
    if x[i]
      result = result + 1;
    end if
  end for
  return result;
end function popcount
```

> *⚑ **No-latch rule:** every code path in a function must reach a `return` statement. An `if` without an `else` that contains a `return` is a compile error — the missing branch would infer a latch on the return value. Fix by adding an `else` branch or a final `return` after the `if`:*
>
> ```
> // ✗ ERROR: not all code paths return a value
> function bad(x: UInt<8>) -> UInt<8>
>   if x[0]
>     return 42;
>   end if                     // ← missing else
> end function bad
>
> // ✓ OK: all paths return
> function good(x: UInt<8>) -> UInt<8>
>   if x[0]
>     return 42;
>   else
>     return 0;
>   end if
> end function good
>
> // ✓ OK: return after if (fallthrough)
> function also_good(x: UInt<8>) -> UInt<8>
>   let r: UInt<8> = 0;
>   if x[0]
>     r = 42;
>   end if
>   return r;
> end function also_good
> ```

---

## 30. Separate Compilation

**30.1 Interface Files (`.archi`)**

`arch build` automatically emits a `.archi` interface file alongside each `.sv` file. The `.archi` contains only the module signature — params and ports, no body:

```
// SubModule.archi (auto-generated)
module SubModule
  param WIDTH: const = 32;
  port clk: in Clock<SysDomain>;
  port data_in: in UInt<WIDTH>;
  port data_out: out UInt<WIDTH>;
end module SubModule
```

The `.archi` file is valid ARCH syntax and can be parsed by the compiler directly. It is named by **module name** (not source filename), so `fifo_async_r2w_sync.arch` (which defines `synchronizer r2w_sync`) generates `r2w_sync.archi`.

**30.2 Dependency Discovery**

When the compiler encounters `inst sub: SubModule` and `SubModule` is not defined in the input files, it automatically searches for:

1. `SubModule.arch` in the input file's directory
2. `SubModule.archi` in the input file's directory
3. `SubModule.arch` or `SubModule.archi` in directories listed in `ARCH_LIB_PATH` (colon-separated)

This enables separate compilation workflows:

```bash
# Step 1: build sub-module (generates .sv + .archi)
arch build SubModule.arch

# Step 2: build top module (auto-discovers SubModule.archi for type-checking)
arch build TopModule.arch

# Or build everything at once
arch build *.arch
```

**30.3 Error Diagnostics**

When a module reference cannot be resolved, the compiler provides an actionable hint:

```
× undefined module: `SubModule`
  help: build the sub-module first: `arch build SubModule.arch`
        (generates SubModule.archi), then re-compile this module
```

---

*ARCH Language Specification v0.1 · March 2026 · Draft for Review*
