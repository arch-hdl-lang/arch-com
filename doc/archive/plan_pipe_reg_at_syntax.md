# Plan: `pipe_reg<T, N>` port type + `@N` latency operator

*Author: session of 2026-04-21. Status: design draft; not yet implemented.*

## Motivation

`port reg q: out T;` is ARCH's most common LLM-footgun. The keyword sits
in the port decl but the latency it implies (1 clock edge between
internal write and external observation) isn't visible at any of the
call sites that assign to `q`. Agents routinely produce testbenches
that sample the output the same cycle the state transitions, off by one.

Three fixes proposed together so the timing information is present
*everywhere* it matters:

1. **Port-type form `pipe_reg<T, N>`** — replaces `port reg`, names the
   latency in the port signature. LLMs reading the port list see N
   directly.
2. **Module-scope `pipe_reg` drops explicit source** — matches the port
   form; one name per pipe, written implicitly via normal assignments.
3. **`@N` latency operator** on LHS (seq & comb) and `@0` on RHS — makes
   every write and every read spell out the cycle offset when it
   matters.

With these, `port reg q: out T` + `q <= Y` becomes `port q: out
pipe_reg<T, 1>` + `q@1 <= Y`. Both sides of the assignment now carry
the `1`, so an agent can't miss it.

## Surface

### Port declaration

```
port q: out UInt<8>;                            // comb, unchanged
port q: out pipe_reg<UInt<8>, 1>;               // 1-cycle registered output (today's `port reg`)
port q: out pipe_reg<UInt<8>, 3> reset rst => 0;// 3-cycle pipe, resettable
port q: out pipe_reg<UInt<8>, 2> init 0;        // 2-cycle pipe with init
```

Grammar:
```
PipeRegType := 'pipe_reg' '<' TypeExpr ',' ConstExpr '>'
PortType    := TypeExpr | PipeRegType
```

Valid on output ports only (`in pipe_reg<T, N>` is a separate future
question — see §Non-goals).

### Module-scope `pipe_reg`

```
pipe_reg x: UInt<8> stages 3 reset rst => 0;    // 3-stage internal pipe
```

Source is implicit — users assign via `x@N <= Y` (seq) or `x@0 = Y`
(comb). The stand-alone declaration names only the pipe's output.

Grammar: unchanged from today's `pipe_reg name: T stages N ...;` form,
except the `from SOURCE` clause (currently `pipe_reg x: source stages N`)
is removed. See §Migration below for auto-rewrite of existing code.

### `@N` operator

Single new postfix operator on identifiers that name a pipe_reg (or
`port reg`-equivalent). `N` is a constant expression, not a runtime
signal.

```
Expr        := Expr '@' ConstExpr | …   (RHS position)
AssignTgt   := Ident '@' ConstExpr | Ident | …   (LHS position)
```

Meaning depends on position:

| Position | Form | Meaning |
|---|---|---|
| LHS seq (`<=`) | `X@N <= Y` | Y arrives at X's output N cycles from now. `N` must equal X's declared depth. |
| LHS comb (`=`) | `X@0 = Y` | Y feeds X's input this cycle; propagates through N stages. Semantically equivalent to `X@N <= Y` in a seq block for the same Y. |
| LHS bare on pipe_reg | `X <= Y` | **Error** — ambiguous latency. Compiler suggests `X@<depth> <= Y`. |
| RHS | `Y @0` | Explicit "current value." Same as bare `Y` but labels the timing. |
| RHS | `Y @K` for K > 0 | **Error** in v1 — reading intermediate stages not yet supported. Reserved for future `@-K` (past values) or `@K` (future values in formal) extensions. |
| RHS bare on pipe_reg | `Y` | Reads the final output stage (the value that's emerged after `depth` cycles). Equivalent to `Y@0` for the reader; the pipe's depth is invisible on read. |
| LHS/RHS on plain comb port | `q@0 = Y` | **Error** in v1 — `@0` on a non-pipe_reg port is redundant. Keeps the grammar focused on timing-bearing signals. |

Error messages (tested):

- `q@5 <= Y` on `pipe_reg<T, 3>`: *"`q@5` exceeds declared depth 3 — use `q@3 <= Y` or change the port's depth"*
- `q <= Y` on `pipe_reg<T, N>`: *"assignment to `q` without `@N` is ambiguous — write `q@<depth> <= Y` (depth = N for this port)"*
- `q@0 = Y` on `port q: out UInt<8>` (comb port): *"`@0` annotation is only valid on `pipe_reg<T, N>` ports; drop the `@0` or change the port to `pipe_reg<T, k>`"*
- `q@2` on RHS: *"reading intermediate stage 2 is not yet supported; read `q` or `q@0` for the current output"*

## Semantics and lowering

`pipe_reg<T, N>` on a port ≡ a hidden N-stage flop chain between the
module's internal write site and the external port. Lowering to SV:

```
port q: out pipe_reg<UInt<8>, 3> reset rst => 0;
seq on clk rising
  q@3 <= next_q;
end seq
```

emits:
```systemverilog
logic [7:0] q_stg1, q_stg2;
logic [7:0] q;
always_ff @(posedge clk) begin
  if (rst) begin
    q_stg1 <= 8'd0;
    q_stg2 <= 8'd0;
    q      <= 8'd0;
  end else begin
    q_stg1 <= next_q;
    q_stg2 <= q_stg1;
    q      <= q_stg2;
  end
end
```

Same shape as today's `pipe_reg` internal lowering — we reuse the
existing chain emission and just attach the output to the port.

For N=1, the SV shape is identical to today's `port reg`:
```systemverilog
always_ff @(posedge clk) if (rst) q <= 8'd0; else q <= next_q;
```

## Migration (auto-rewrite)

Existing code uses two forms:
- `port reg q: out T [reset R => V] [init V];` with `seq q <= Y`
- `pipe_reg x: src stages N;` with no assignments

One-shot rewrite pass:

1. Every `port reg q: out T ARG*` becomes `port q: out pipe_reg<T, 1> ARG*`.
   Every corresponding `q <= Y` in a seq block (identified by resolver)
   becomes `q@1 <= Y`.
2. Every `pipe_reg x: SRC stages N;` becomes:
   ```
   pipe_reg x: <type of SRC> stages N;
   seq ... x@N <= SRC; end seq      // OR x@0 = SRC in a comb block
   ```
   Placed immediately after the pipe_reg decl; uses the same seq block
   as the original SRC-producing code when unambiguous, otherwise emits
   a fresh seq block. In practice the compiler's existing "find the
   clock" heuristic already resolves which seq to use.
3. Deprecation warning on both old forms pointing at the new spelling.
4. In the *next* minor release (e.g., v0.45.0), remove the old forms
   entirely.

The rewrite is mechanical and testable — feed every existing `.arch`
file through and `cargo test` proves semantic equivalence.

## Open questions deferred to v2

- **`pipe_reg<T, N>` on input ports.** Would mean "the compiler latches
  my input for N cycles before anything in this module touches it."
  Useful for timing-closure at module boundaries; rarely needed. Not in
  v1.
- **RHS `@K` for K > 0 (reading intermediate stages).** Could return
  the value at stage K of a pipe_reg — useful for FIR filters, skid
  buffers. Deferred; v1 starts strict (only `@0` on RHS).
- **RHS `@-K` (past values).** Could read the value from K cycles ago,
  auto-generating a history buffer. Interesting for protocol/formal
  assertions. Not in v1.
- **`pipe_reg<T, 0>` degenerate case.** Technically equivalent to `out
  T` (comb). Should it be a syntax error (use plain form) or accepted
  as a no-op degenerate? v1 errors out: *"pipe_reg depth must be ≥ 1;
  use plain `out T` for combinational"*.

## Implementation roadmap

### Step 1 — grammar
- Lexer: `@` as a new token (probably already a punct char; check
  collisions with assertion shorthand).
- Parser:
  - `pipe_reg<T, N>` in port type position (currently only `T` forms
    allowed).
  - `Ident '@' ConstExpr` as both expression and assign-target.

### Step 2 — AST
- Add `PortType::PipeReg(T, N)` variant alongside today's direct-type
  port form. Or keep the Boxed TypeExpr and wrap pipe_reg as a new
  TypeExpr variant visible only at port declarations.
- Add `latency_offset: Option<u32>` to `AssignTarget` and `Expr::Ident`
  (or a wrapping `LatencyExpr` variant).

### Step 3 — typecheck
- When seeing `q@N <= Y` on a `pipe_reg<T, depth>` port, verify `N == depth`.
- When seeing bare `q <= Y` on a pipe_reg port, emit the ambiguity error.
- When seeing `q@0 = Y` in comb, verify the port is pipe_reg and that
  the same-named port isn't also driven in a seq block (single-driver).
- When seeing `q@K` for K > 0 on RHS, emit the deferred-feature error.
- When seeing `@` on any non-pipe_reg signal, emit the targeted error.

### Step 4 — elaboration
- Auto-rewrite: old `port reg` → `port pipe_reg<T,1>` and old
  `pipe_reg name: src stages N` → new form with synthesized assignment.
  One pass at the start of elaboration; emits a deprecation warning
  pointing at the old syntax location.

### Step 5 — codegen
- SV: reuse existing pipe_reg chain emission; attach final stage to the
  port instead of an internal wire. For N=1 on a port, reuses the
  today's `port reg` codegen path verbatim.
- Sim: same — emit the multi-stage shadow fields and advance in
  eval_posedge. No new machinery needed.

### Step 6 — tests
- Conversion test: every existing `port reg` test compiles identically
  before and after the auto-rewrite (byte-for-byte SV compare).
- New tests for:
  - Depth mismatch error
  - Bare-assignment error
  - `@0` on plain port error
  - RHS intermediate-stage error
  - Multi-stage output pipe (`pipe_reg<T, 3>` end-to-end via
    Verilator sim with a TB checking 3-cycle latency)
  - Forward compatibility: old-form files still parse and rewrite
    cleanly

### Step 7 — docs
- Spec §4.X updated with the new port forms + `@N` operator semantics
  table.
- Reference card: replace every `port reg` example with the `pipe_reg<T,
  1>` form; add a short "@N latency" card.
- Memory entries (`feedback_port_reg_timing`) updated to reflect the
  new spelling.

## Non-goals

- Changing the semantics of `reg` (the single-flop declaration). This
  plan only touches `port reg` and `pipe_reg`.
- Introducing `wire<T>` as a port-type constructor. Bare `T` as the
  combinational default is retained; `wire<T>` isn't added. (The user's
  earlier discussion considered it; we're keeping bare `T` for
  ergonomics.)
- RDC/CDC extensions (`pipe_reg` clocked by a different domain than the
  source). Today's implicit-clock rule applies; no domain attribute in
  v1.

## Risks

- **Parser ambiguity with `@` in expressions.** ARCH doesn't currently
  use `@`. SV uses it for always-block sensitivity; keeping `@` out of
  arithmetic contexts should prevent confusion. Verify no lexer
  collision during implementation.
- **Auto-rewrite edge cases.** `port reg q: out T init V;` with no
  reset — today's form. Rewrite must preserve all modifiers. The test
  corpus has ~50 uses; audit before flipping the default.
- **Seq vs comb write on pipe_reg.** Allowing both `x@0 = Y` (comb) and
  `x@N <= Y` (seq) to feed the same pipe is conceptually clean but could
  confuse when users mix them. Single-driver check must fire if both
  forms target the same pipe in the same module.
- **`.archi` interface file.** `port reg` today emits as `port reg q:
  out T;` in the interface file. Migration means every downstream
  consumer of a `.archi` must understand the new form; OK since `.archi`
  is locally regenerated on `arch build`.

## Timeline

Four focused PRs:

1. **PR #1**: grammar + AST + typecheck for the new `pipe_reg<T, N>`
   port type and `@N` operator. No codegen; error paths and the parse
   shape only. Gates future work behind a feature that's syntactically
   accessible but semantically inert.
2. **PR #2**: codegen lowering (reuse today's pipe_reg chain machinery).
   End-to-end tests for depth=1, depth=3, with and without reset.
3. **PR #3**: auto-rewrite pass in elaboration; migrate the test
   corpus; emit deprecation warnings.
4. **PR #4**: spec + reference card + memory updates. Land after the
   code PRs are in the main branch long enough to shake out edge cases.
