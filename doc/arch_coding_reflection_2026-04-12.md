# ARCH Coding Reflection (2026-04-12)

## Context

This note reflects on the experience of writing and debugging real ARCH designs during the CVDP benchmark work, plus using the `arch-hdl` MCP workflow to iterate on `.arch` files quickly.

The perspective here is practical: what felt good when implementing RTL in ARCH, what repeatedly caused friction, and what language/compiler changes would likely improve real coding velocity and correctness.

## What Felt Strong

### 1. ARCH is expressive enough for serious RTL work

ARCH is already capable of handling a wide range of designs:

- combinational datapaths
- parameterized arithmetic blocks
- FSM-oriented controllers
- medium-complexity protocol/control modules
- vectorized structures like `Vec<UInt<N>, M>`

The language is especially pleasant when the design maps directly onto one of its first-class ideas:

- `fsm`
- `clkgate`
- `counter`
- explicit reset typing/polarity
- width-aware integer operations

When a design fits those constructs, ARCH tends to be safer and faster to write than raw SystemVerilog.

### 2. The explicit type/width discipline pays off

The no-implicit-conversion rule catches real bugs. In benchmark work, many issues were genuinely width-related, and ARCH surfaces them early instead of silently generating questionable SV.

This is one of the strongest parts of the language.

### 3. MCP materially improves the coding loop

The best ARCH workflow I used was:

1. ask MCP for construct syntax
2. edit `.arch`
3. `arch_check`
4. `arch_build`
5. run the external cocotb harness

That loop is much better than guessing syntax from memory. The MCP server is not just convenience tooling; it meaningfully reduces avoidable language errors.

## What Felt Frictional

### 1. Some legal structure is non-obvious without examples

The biggest recent example was functions: they are legal at top level, but not nested inside a module. That is fine as a language rule, but it is easy to guess wrong if you are coming from SV/SystemVerilog-like mental models.

This kind of rule is learnable, but it costs iteration unless the tooling teaches it immediately.

### 2. Timing intent can be easy to misread from surface syntax

Several benchmark bugs boiled down to understanding whether something was:

- combinational now
- registered one cycle later
- visible to cocotb before or after the edge

Examples:

- `port reg` vs `let out = reg_x`
- reset visibility
- delayed control strobes like `start_intra`

ARCH is precise, but the code can still look deceptively simple relative to the timing consequences.

### 3. Generated SV sometimes exposes simulator-sensitive patterns

A few generated forms are logically fine but less portable or less Icarus-friendly than they could be:

- precedence-sensitive boolean expressions
- unpacked array whole-assignment patterns
- dynamic indexing patterns on unpacked arrays

These are not always compiler bugs in the strict sense, but they create extra debug noise downstream.

### 4. Multi-file/module workflows still have rough edges

A lot of realistic benchmark designs are not single-file modules. In practice, debugging often depended on:

- matching filename vs module name
- making sure dependent modules were present
- building multiple `.arch` files together

ARCH can handle this, but the workflow still feels more fragile than it should.

## Suggested Language Changes

### 1. Improve helper-function discoverability and locality

ARCH already supports reusable helper functions via top-level declarations and packages, and package-scoped functions are a valid way to share helpers across modules.

The friction I hit was more about discoverability and locality than raw capability:

- it is easy to assume helper functions can be nested inside modules
- the package-based pattern is not always the first thing a new user will reach for
- for one-off helpers, a package can feel heavier than the conceptual size of the problem

So the main improvement here may be documentation/tooling first:

- make package-scoped helper functions more prominent in docs and MCP guidance
- show “small local helper” examples using packages or top-level functions
- improve parser diagnostics so declaration-placement mistakes point users toward packages

That said, module-local helper syntax could still be a nice ergonomic enhancement in the future, mainly for organization rather than expressiveness.

### 2. Add a clearer notion of output timing intent

Today the difference between combinational and registered outputs is correct but easy to misread.

Possible improvements:

- syntax sugar for registered outputs with explicit latency semantics
- optional lints like “output depends on reg but is combinational”
- an annotation that makes cycle visibility obvious in generated docs/MCP help

### 3. Add better support for array/vector mux idioms

A lot of practical RTL wants “select one element from a vector/array of buses” in a way that compiles robustly across backends/simulators.

Helpful additions:

- first-class vector select helper
- compiler-lowered safe mux generation for dynamic `Vec` indexing
- explicit syntax for small-N muxes over arrays

This would reduce hand-written mux fallback code.

### 4. Add a better story for module-private constants/helpers

ARCH has params, local params, lets, top-level functions, and package-scoped helpers, but there is still a gap for medium-sized local helper logic that is not a full submodule.

It may help to support:

- module-local pure helper declarations
- named combinational helper blocks
- reusable compile-time helper expressions/macros

## Suggested Compiler Changes

### 1. Emit more conservative SV for simulator portability

The compiler should prefer slightly more verbose SV if it avoids common simulator issues.

Examples:

- parenthesize boolean expressions aggressively
- avoid unpacked array whole-assignment when per-element assignment is safer
- lower dynamic array indexing into explicit muxes when practical

This would trade a little output elegance for fewer downstream tool surprises.

### 2. Add lints for “probably legal but probably surprising”

Some of the most expensive bugs were not syntax errors; they were valid ARCH that likely did not mean what the author intended.

Useful warning categories:

- output timing ambiguity
- filename/module-name mismatch
- parameters that imply invalid configurations
- delayed control strobes that never clear or become sticky
- use of constructs that generate simulator-fragile SV

### 3. Improve diagnostics around declaration placement rules

When a function is declared in the wrong place, the current parser error is technically correct, but it would be better if the diagnostic explicitly said something like:

“Functions must be declared at top level or in packages, not inside modules. Consider moving this helper to a package and `use`-ing it.”

That kind of message would save a full context switch.

### 4. Improve multi-file elaboration ergonomics

For real projects, the compiler should make multi-file relationships easier to manage.

Possible improvements:

- better missing-module diagnostics
- explicit dependency reporting
- “build this module and all referenced local ARCH files” mode
- optional module-name/file-name consistency warnings

### 5. Surface generated-SV intent in tooling

It would be very helpful if MCP/compiler tooling could explain not just that something compiles, but how it lowers.

Examples:

- “this output lowers to a registered output”
- “this `Vec` index becomes an unpacked array access”
- “this parameter default is preserved as an expression”

That would make ARCH easier to trust on timing-sensitive designs.

## MCP-Specific Suggestions

### 1. Make construct guidance easier to chain

The current `get_construct_syntax()` tool is already very useful. It would be even better if MCP also exposed:

- “common pitfalls for this construct”
- “related constructs”
- “generated SV shape”

### 2. Add a targeted “why did this fail?” mode

An MCP helper that classifies failures would save a lot of time:

- parse/type-system issue
- elaboration issue
- simulator portability issue
- likely functional mismatch

That would make debugging much more direct.

## Bottom Line

ARCH is already good enough to write substantial real RTL, and the benchmark work showed that clearly.

The language feels strongest when it is explicit:

- explicit widths
- explicit resets
- explicit state
- explicit first-class constructs

The main improvements I would prioritize are:

1. better helper-function discoverability and locality
2. stronger portability-minded SV lowering
3. better diagnostics for surprising-but-legal constructs
4. smoother multi-file/module workflows
5. richer MCP explanations tied to generated behavior

Those changes would not just make ARCH nicer; they would make it faster to trust in real iterative hardware development.
