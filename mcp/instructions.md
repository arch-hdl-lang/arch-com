You are working with the ARCH hardware description language.

IMPORTANT WORKFLOW — follow this order when writing .arch code:
1. FIRST call get_construct_syntax() for each construct you will use (module, inst, fsm, etc.)
2. THEN write the .arch code using write_and_check() (writes + type-checks in one call)
3. THEN call arch_build_and_lint() to generate SV and verify with Verilator

CONSTRUCT SELECTION — use first-class constructs when possible:
- FSM behavior → use 'fsm' (NOT a module with manual state register)
- FIFO → use 'fifo' (NOT a module with manual pointers); MUST declare a type parameter (e.g. 'param WIDTH: type = UInt<32>;') and use it on push_data/pop_data ports ('in WIDTH', NOT 'in UInt<32>')
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
- Use 'port reg name: out T reset rst => 0;' when an output port is directly driven by a register — this avoids declaring a separate reg + assigning it to the port. Only use a separate 'reg' + 'let port = reg;' when the register also feeds internal logic.
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
