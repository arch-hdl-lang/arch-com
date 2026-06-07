You are working with the ARCH hardware description language.

IMPORTANT WORKFLOW — follow this order when writing .arch code:
1. FIRST call get_construct_syntax() for each construct you will use (module, inst, fsm, etc.)
2. THEN write the .arch code using write_and_check() (writes + type-checks in one call)
3. THEN call arch_build_and_lint() to generate SV and verify with Verilator

WHEN A COMPILE ERROR APPEARS: call arch_advise(query="<error message keywords>") before attempting a fix. It retrieves past error→fix pairs from the user's local learning store (~/.arch/learn/). If a match exists, prefer its approach — the user has hit this before. Every check/build/sim/formal invocation silently records new error→fix pairs, so the store grows over time. Use arch_learn_stats() to see what's accumulated.

CONSTRUCT SELECTION — use first-class constructs when possible:
- FSM behavior → use 'fsm' (NOT a module with manual state register)
- FIFO → use 'fifo' (NOT a module with manual pointers); MUST declare a type parameter (e.g. 'param T: type = UInt<32>;') and use it on push_data/pop_data ports ('in T', NOT 'in UInt<32>')
- RAM/ROM → use 'ram' with appropriate kind (NOT a module with reg array)
- Arbiter → use 'arbiter' with policy (NOT manual grant logic in a module); built-in policies: round_robin, priority, lru, weighted<W>; if the requirement doesn't match any built-in policy (e.g. QoS-aware, starvation-prevention, custom fairness), use 'policy <FnName>' with a 'hook grant_select(...) -> UInt<N> = FnName(...);' and define the logic in a 'function' in the same file — the function receives req_mask + last_grant and returns a one-hot grant mask
- Pipeline → use 'pipeline' with stages (NOT manual valid/stall registers)
- Bus → use 'bus' for reusable port bundles (NOT manual individual port declarations for standard interfaces like AXI, APB, custom)
- Only use 'module' for pure combinational/registered logic that doesn't fit the above

Common mistakes to avoid:
- inst connections use 'port <- signal' for inputs and 'port -> wire' for outputs (NOT '=' or direct assignment, no 'connect' keyword)
- Hierarchical references (inst_name.port_name) are FORBIDDEN — always connect outputs explicitly
- 'let' has two forms: 'let x: T = expr;' declares a new wire (type required); 'let x = expr;' (no type) assigns to an already-declared output port or wire
- Do NOT use reserved keywords as signal/register names (counter, interface, domain, etc.)
- 'in', 'out', 'state' are contextual keywords — safe to use as port/signal names
- All input ports of an inst MUST be connected (compile error if missing); all output ports SHOULD be connected (warning if missing); Clock/Reset ports are exempt
- Use 'elsif' for chained conditionals (NOT 'else if'). 'else' starts a body block; 'elsif' chains.
- Bit-slice syntax: expr[hi:lo] extracts bits (NOT .trunc<Hi,Lo>())
- Bit/byte reverse: expr.reverse(1) for bit-reverse, expr.reverse(8) for byte-reverse (width must be divisible by N)
- Prefer concat {a, b} over bit-by-bit for loops; prefer direct boolean (z = (A == B);) over if/else
- For a registered output port, prefer `port name: out pipe_reg<T, N> reset rst => 0;` — N is the output latency (cycles between internal write and external observation) and is visible in the port signature. N=1 replaces the legacy `port reg name: out T`; N>=2 synthesizes an N-stage output pipe with uniform reset across every stage. Only use a separate `reg` + `let port = reg;` when the same register also feeds internal logic. Legacy `port reg name: out T` still parses and is exactly equivalent to `port name: out pipe_reg<T, 1>`.
- ASSIGNMENT: use `port_name@N <= expr` in a `seq` block; reads as "expr will be in port_name N cycles from now." For N=1, bare `port_name <= expr` is also accepted; for N>=2, bare assignment is a compile error (ambiguous). Reading a pipe_reg port on RHS returns the current output; `port_name@0` is the explicit spelling for "current value" (intermediate-stage reads `@K` for K>0 are not yet supported).
- OUTPUT TIMING: `pipe_reg<T, N>` outputs have N-cycle latency (output reflects the write from N clock edges ago). For FSM outputs that must respond in the SAME cycle as a state transition (e.g. cocotb tests that update model state+outputs simultaneously), use plain `port o: out T` driven by `comb` or `let`, NOT a pipe_reg port.
- Use 'onehot(index)' for one-hot decode instead of manual shift expressions — result width is inferred from assignment context. Example: 'bean_r <= onehot(i_bean_sel);' emits '(1 << i_bean_sel)' in SV.
- Functions can be declared inside a module body: 'function name(args) -> RetType ... end function name'. Use for one-off helpers instead of creating a full package.
- 'arch build' auto-emits '.archi' interface files alongside '.sv'. When a module instantiates an unknown sub-module, the compiler auto-discovers 'SubModule.archi' in the same directory or ARCH_LIB_PATH.
- Prefer putting next-value logic directly in seq (if/elsif) instead of splitting into separate comb + seq blocks. Use 'let x: T = expr;' for pure combinational expressions that feed into seq. Use 'let x = expr;' to drive an existing output port or wire. Only use 'wire' + 'comb ... end comb' when the value is conditionally assigned (if/elsif/else).
- In fsm states, do NOT write '-> X when true;' — omit the transition to stay in the current state (implicit hold), or restructure so the last branch uses a real condition
- Do NOT declare 'domain ... end domain' in pure combinational modules — domains are only needed when Clock/Reset ports are used
- SysDomain is built-in — do NOT declare 'domain SysDomain end domain SysDomain'; just use Clock<SysDomain> directly
- Bus signal access uses dot notation (itcm.cmd_valid), NOT underscore (itcm_cmd_valid)
- Bus ports use 'initiator BusName' or 'target BusName' to set the perspective — 'initiator' keeps signal directions as declared in the bus; 'target' flips them (in↔out)
- Use 'default seq on clk rising;' to set the default clock for seq blocks in the scope
- One-line seq requires 'default seq' — without it, 'seq' must have 'on clk rising/falling'
- Use 'package PkgName ... end package PkgName' to group shared enums/structs/functions/domains; import with 'use PkgName;' at file scope
- Domains declared in a package are shared across files via 'use PkgName;'
- Functions are legal at top level and inside packages; they are NOT legal inside modules
- 'inside' operator: expr inside {val1, val2, lo..hi} — returns Bool, set membership
- 'for i in {a, b, c}' — compile-time unrolled value-list iteration (inside comb/seq blocks)
- 'unique if' and 'unique match' assert mutual exclusivity to synthesis (parallel mux): use 'unique if sel == 0 ... end if' or 'unique match opcode ... end match'; emits SV 'unique if' / 'unique case'
- .trunc<N>() errors if N >= source width (not truncating); .zext<N>()/.sext<N>() error if N <= source width (not extending)
- signed(x) / unsigned(x): same-width reinterpret cast (no width arg needed); prefer signed(x) over x.sext<N>() when entering signed arithmetic chains
- Wrapping arithmetic operators +%, -%, *% give result width = max(W(a),W(b)) with no widening — prefer these over .trunc<N>() when the intent is modular arithmetic: 'let x: UInt<8> = a +% b;' instead of 'let x: UInt<8> = (a + b).trunc<8>();'
- SVA implication operators (v0.49.0+): use '|->' for overlap (same-cycle) and '|=>' for next-cycle implication inside 'assert' / 'cover' bodies. The legacy 'implies' keyword is a deprecated alias for '|->' — accepted but emits a stderr warning per use; new code should use '|->'. Both are restricted to SVA contexts; for plain Boolean implication outside 'assert'/'cover', write '(!a) || b'.

DESIGN SPEC PROVENANCE — when generating .arch from a user-supplied design spec, you MUST capture the spec inline so it rides with the code:

1. FILE FRONT MATTER. Open the file with a YAML-style block embedded in '//!' lines, delimited by '//! ---' on its own line:

   //! ---
   //! spec_md: doc/specs/<name>.md            (when an external spec file exists or you create one)
   //! tags: [<feature_tag>, <feature_tag>]    (3-6 short tags derived from the spec — e.g. arbitration, axi, axi4)
   //! refs:                                   (citations the spec mentions — datasheet sections, ticket IDs, URLs)
   //!   - "AXI4 spec §A3.3.1"
   //!   - "FOO-1234"
   //! ---
   //!
   //! <1-3 sentence file-level summary in plain prose>

   The compiler stores the YAML verbatim — it does not parse it. Downstream tooling (RAG indexer, formal-tool feeders) consumes it.

2. PER-CONSTRUCT OUTER DOC. Place a '///' block above EVERY top-level construct (module, fsm, fifo, ram, counter, arbiter, pipeline, regfile, synchronizer, clkgate, bus, struct, enum, function, package, use). Capture the construct's role in the design — 1-3 sentences, not implementation details.

   Example:
     /// 4-channel round-robin AXI write arbiter.
     ///
     /// Picks among DMA channels using a rotating priority pointer.
     arbiter AxiWrArb
       ...
     end arbiter AxiWrArb

3. CONSTRUCT INNER DOC (optional). When the construct has design intent that's specific to the body — policy choice, trade-off, ticket reference — place a '//!' block immediately after the opening keyword + name:

     arbiter AxiWrArb
       //! Round-robin chosen because all 4 channels are equal-priority;
       //! see ticket FOO-1234 for the QoS-aware variant proposed for v2.

       policy round_robin;
       ...

4. PRESERVATION ON EDIT. When modifying an existing .arch file, NEVER strip or rewrite '///', '//!', or '//! ---' content unless the user explicitly asks to change it. Treat doc text as load-bearing — it's the project's spec→RTL provenance trail.

5. ESCAPE HATCH. '////+' (4 or more slashes) is a regular comment, not a doc comment — use it for ASCII banners or notes you don't want indexed.

You are responsible for sourcing the design spec from the conversation context. The MCP server does not. If the user has not provided a design spec, write a brief plain-prose '//!' file-level summary based on the user's request and a concise '///' line per construct — do not invent ref/tag fields you can't justify.
