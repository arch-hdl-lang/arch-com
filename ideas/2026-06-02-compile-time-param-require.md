# Enhancement: Compile-time `require` constraints for module parameters

**Date:** 2026-06-02  
**Status:** Proposal — needs team discussion before implementation  
**Related issues:** #462 (local-param scoping collision), #306 (thread wait-exit timing), #383 (formal wire-to-let)  
**Motivation:** real design failures in FPT26 attention tile, NIC-400 crossbar, arch-ibex Phase A

---

## Problem

ARCH module parameters are unguarded. A caller can instantiate:

```arch
module SyncFifo
  param DEPTH: const = 16;
  param DATA_W: const = 32;
  ...
end module SyncFifo
```

with `DEPTH=0`, `DATA_W=0`, or `DEPTH=3` (not a power of two — breaking any
gray-code pointer logic) and the compiler accepts all of them silently. The
error only surfaces later, deep in the downstream computation, as a confusing
type mismatch or wrong synthesis result.

Issue #462 illustrates the sharpest edge of this problem: a `local param
PRODUCT_WIDTH` in `Bf16PvTileEngine` resolves to `38` in isolation but was
seen as `32` when the module was instantiated alongside a sibling dependency
that also defined a `PRODUCT_WIDTH` local param. The compiler reported a
`type mismatch: expected SInt<32>, found SInt<38>` error pointing deep into the
multiply expression — far from the instantiation site where the wrong value was
injected. A module-level `require PRODUCT_WIDTH == 38` would have caught the
mistake immediately at the instantiation site with a message pointing at both
the violated constraint and the call site.

Similar gaps surface throughout the codebase:

| Module | Implicit contract | Currently enforced? |
|--------|-------------------|--------------------|
| Any gray-code FIFO | `DEPTH` is a power of two | No |
| Parameterized arbiter | `WEIGHT_W >= clog2(MAX_WEIGHT)` | No |
| Multiply unit | product width = operand widths summed | No (see #462) |
| Multi-outstanding TLM | `TAGS >= 2` | No |
| Pipeline | `STAGES >= 1` | No |

These invariants are documented only in comments (if at all), invisible to the
compiler, and discovered only when the downstream SV fails to synthesize or
simulate correctly.

---

## Proposed: `require` declaration

Add a new module-body item:

```
require EXPR [: "diagnostic message"];
```

`EXPR` is a compile-time boolean expression over the module's `param` and
`local param` names. It is evaluated once per instantiation after all parameter
substitutions are applied. If the expression evaluates to `false`, the compiler
emits a `CompileError::RequireViolation` that points at both the `require`
declaration and the call-site instantiation.

### Syntax

```arch
module SyncFifo
  param DEPTH: const = 16;
  param DATA_W: const = 32;

  require DEPTH > 0 : "DEPTH must be at least 1";
  require (DEPTH & (DEPTH - 1)) == 0 : "DEPTH must be a power of two for gray-code pointers";
  require DATA_W in 1..512 : "DATA_W must be between 1 and 512 bits";

  ...
end module SyncFifo
```

```arch
module Bf16PvTileEngine
  param WEIGHT_FRAC_BITS: const = 7;
  param VALUE_WIDTH: const = 31;

  local param WEIGHT_STORAGE_WIDTH: const = WEIGHT_FRAC_BITS + 1;
  local param WEIGHT_SIGNED_WIDTH:  const = WEIGHT_STORAGE_WIDTH + 1;
  local param PRODUCT_WIDTH:        const = WEIGHT_SIGNED_WIDTH + VALUE_WIDTH;

  require PRODUCT_WIDTH == WEIGHT_SIGNED_WIDTH + VALUE_WIDTH
    : "PRODUCT_WIDTH scoping check — if this fires, a local param leaked from another module";

  ...
end module Bf16PvTileEngine
```

### Expression subset

Only const-evaluable expressions are accepted in `require`:

- Arithmetic: `+`, `-`, `*`, `/`, `%`, `<<`, `>>`
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logical: `and`, `or`, `not`
- Bitwise: `&`, `|`, `^`, `~`
- Range check: `EXPR in LOW..HIGH` (closed interval, both endpoints included)
- `$clog2(EXPR)` and `$onehot(EXPR)` built-ins
- References to `param` and `local param` names in the same module
- Integer literals

Signal reads, port references, function calls, and non-const expressions are
rejected at parse time with a clear "require expressions must be const" error.

### Error report shape

```
Error: require constraint violated in instantiation of `SyncFifo`
    ╭─[rtl/arch/QueueController.arch:42:3]
 42 │   inst fifo: SyncFifo
    ·   ─────────┬─────────
    ·             ╰── instantiated here with DEPTH = 3
    ╰────

  note: violated constraint at:
    ╭─[rtl/arch/SyncFifo.arch:8:3]
  8 │   require (DEPTH & (DEPTH - 1)) == 0 : "DEPTH must be a power of two for gray-code pointers";
    ╰────

  note: DEPTH evaluated to 3 (3 & 2 = 2, not 0)
```

The error spans two files: the `require` declaration and the instantiation site.
This is the same double-span pattern already used by the existing port-type
mismatch errors.

---

## Why this matters

### 1. Earlier, more actionable error messages (closes the #462 class)

The `PRODUCT_WIDTH` scoping bug (#462) would have been caught at the
instantiation site of `AttentionTileEngine`, pointing directly at the parameter
that was resolved incorrectly — rather than surfacing as a `SInt<32>` vs
`SInt<38>` type mismatch at the multiply expression.

More broadly, any time a parameter resolution bug produces a wrong width, the
`require PRODUCT_WIDTH == expected_value` pattern surfaces it immediately.

### 2. Self-documenting module contracts

`require` declarations serve as machine-checkable documentation. A reader of
`SyncFifo.arch` immediately knows:
- DEPTH must be a power of two
- DATA_W has a valid range

No comment extraction needed. The constraint is a first-class language item,
visible to documentation generators, IDE hover hints, and LLM context windows.

This is especially relevant for the AI-agent workflow (CLAUDE.md, AGENTS.md):
when an agent generates `inst fifo: SyncFifo` with `DEPTH=5`, the compile error
gives the agent a precise, actionable message it can use to self-correct without
human intervention.

### 3. Gradual adoption

`require` is additive and opt-in. Existing designs without `require` clauses
continue to compile identically. Teams can add constraints incrementally as
they encounter bugs or onboard new modules.

### 4. Fits ARCH's "no magic" philosophy

ARCH is explicit about timing, signal direction, and reset behavior. `require`
extends this explicitness to parametric contracts. There is no hidden inference;
every constraint is a source-level declaration that the compiler enforces.

---

## Interaction with `local param`

`require` can reference `local param` names. This is the key mechanism for
auto-consistency checks:

```arch
module MultiplierUnit
  param A_W: const = 8;
  param B_W: const = 8;

  local param OUT_W: const = A_W + B_W;

  require A_W > 0 : "A operand width must be positive";
  require B_W > 0 : "B operand width must be positive";
  require OUT_W <= 64 : "product width OUT_W = A_W + B_W must fit in 64 bits";

  port a: in UInt<A_W>;
  port b: in UInt<B_W>;
  port out: out UInt<OUT_W>;
  ...
end module MultiplierUnit
```

The `require OUT_W <= 64` check runs after `local param OUT_W` is resolved,
giving a precise message when callers pass excessively large widths.

---

## Interaction with `generate for` / `generate if`

`require` interacts cleanly with generate constructs:

- A `require` in a module body is checked for every instantiation of that module,
  regardless of which `generate if` branches are active.
- A `require` inside a `generate if COND ... end generate if` block is only
  checked when `COND` is true at elaboration time — same semantics as any other
  body item inside a conditional generate.

This allows:

```arch
module DualClockFifo
  param DEPTH: const = 16;
  param ASYNC_CDC: const = 1;    // 1 = async (dual clock), 0 = sync

  require DEPTH > 0;

  generate if ASYNC_CDC == 1
    // gray-code requires power-of-two depth
    require (DEPTH & (DEPTH - 1)) == 0
      : "async FIFO requires power-of-two depth for gray-code pointers";
  end generate if
end module DualClockFifo
```

---

## What `require` is NOT

- **Not a runtime assertion.** `require` is evaluated purely at elaboration time
  (before any SV is emitted) against constant parameter values. It has no
  runtime component and emits nothing to the generated SV — or at most a
  `// @require: DEPTH must be a power of two` comment for documentation.

- **Not a replacement for `assert`.** `assert name: expression` checks signal
  values during simulation and formal verification. `require` checks parameter
  values before any simulation begins.

- **Not a type system extension.** `require` doesn't add dependent types or
  type-level arithmetic. It's a simple "evaluate this const expression and fail
  if false" check — the same mental model as C++'s `static_assert`.

---

## Implementation plan

### Phase 1 — Parser + AST (~150 LoC)

Add `Require` as a new `ModuleBodyItem` variant:

```rust
// src/ast.rs
pub struct RequireDecl {
    pub expr:    Expr,           // must be const-evaluable
    pub message: Option<String>, // optional : "..." diagnostic
    pub span:    Span,
}
```

Parse `require EXPR [: "message"] ;` in `parser.rs::parse_module_body_item`.
The expression parser already handles arithmetic and comparison operators;
the only new production is the optional `: "string"` suffix.

### Phase 2 — Const-expression evaluator check (~80 LoC)

During elaboration, after parameter substitution, validate that `require`
expressions reference only const values. Reject port names, wire names, and
non-const subexpressions with a `CompileError::RequireNonConst` that points
at the offending subexpression.

The existing const-evaluation path (already used for `local param` arithmetic
and `generate if` conditions) can be reused here with minimal extension.

### Phase 3 — Instantiation-time evaluation (~120 LoC)

In `elaborate.rs::instantiate_module`, after resolving all parameter values:

```rust
for req in &module.body.requires {
    let value = eval_const_expr(&req.expr, &resolved_params)?;
    if !value.as_bool() {
        return Err(CompileError::RequireViolation {
            module: module.name.clone(),
            expr_span: req.span,
            message: req.message.clone(),
            inst_span: inst.span,
            resolved_params: resolved_params.clone(),
        });
    }
}
```

The `eval_const_expr` function already exists for `local param` arithmetic;
it needs to handle comparison and logical operators if not already present.

### Phase 4 — Error reporting (~60 LoC)

Add `CompileError::RequireViolation` to `src/diagnostics.rs`. Use miette's
multi-span labels to point simultaneously at the `require` declaration (with
the evaluated parameter values) and the instantiation call site.

### Phase 5 — SV annotation (optional, ~30 LoC)

In `src/codegen/mod.rs`, emit each `require` as a comment in the generated
SV module header:

```sv
// @require: (DEPTH & (DEPTH - 1)) == 0 — DEPTH must be a power of two
// @require: DATA_W in 1..512
module SyncFifo #(parameter DEPTH = 16, parameter DATA_W = 32) ( ... );
```

This preserves the constraint documentation in the generated SV, aiding
downstream consumers (synthesis tools, linters) that parse module headers.

### Tests

| Test | Expected result |
|------|-----------------|
| `require DEPTH > 0` with `DEPTH=5` | passes |
| `require DEPTH > 0` with `DEPTH=0` | `CompileError::RequireViolation` at inst site |
| `require (DEPTH & (DEPTH - 1)) == 0` with `DEPTH=4` | passes |
| `require (DEPTH & (DEPTH - 1)) == 0` with `DEPTH=3` | error with evaluated value in message |
| `require X in 1..128` with `X=1`, `X=128`, `X=0`, `X=129` | pass, pass, fail, fail |
| `require` referencing a `local param` derived from other params | correct resolution and check |
| `require` with signal name (non-const) | `RequireNonConst` error |
| `generate if COND` containing `require` — COND false | `require` not evaluated |
| ARCH module with no `require` clauses | no change in behavior |

Estimated total: **~440 LoC** across parser, elaborate, diagnostics, codegen.
No changes to emitted SV semantics. No changes to `arch sim`, `arch formal`,
or `arch build` output (other than the optional header comments).

---

## Relationship to existing plan docs

This proposal is orthogonal to all existing plan documents:

- `plan_reg_guard_syntax.md` — register-level guards for uninit checking; different layer
- `plan_compiler_refactor.md` — internal IR changes; `require` builds on existing const-eval
- `plan_hierarchical_formal.md` — formal reasoning about module connectivity; `require`
  catches bad params before formal even runs
- `plan_tlm_method.md` — TLM method semantics; `require TAGS >= 2` would validate TLM
  out-of-order usage pre-elaboration

The closest analogue is `plan_arch_doc_comments.md` (documentation extraction from module
interfaces). `require` declarations are *machine-checkable* documentation — a natural
companion to doc-comment extraction.

---

## Concrete example: fixing the #462 class of bug

With `require`, the `Bf16PvTileEngine` module from issue #462 would include:

```arch
module Bf16PvTileEngine
  param WEIGHT_FRAC_BITS: const = 7;
  param VALUE_WIDTH:      const = 31;
  param LEN_WIDTH:        const = 8;

  local param WEIGHT_STORAGE_WIDTH: const = WEIGHT_FRAC_BITS + 1;
  local param WEIGHT_SIGNED_WIDTH:  const = WEIGHT_STORAGE_WIDTH + 1;
  local param PRODUCT_WIDTH:        const = WEIGHT_SIGNED_WIDTH + VALUE_WIDTH;
  local param ACC_WIDTH:            const = PRODUCT_WIDTH + LEN_WIDTH;

  // Explicit self-consistency checks — if any of these fire,
  // a param scoping bug has leaked a wrong value from a sibling dependency.
  require WEIGHT_STORAGE_WIDTH == WEIGHT_FRAC_BITS + 1;
  require WEIGHT_SIGNED_WIDTH  == WEIGHT_FRAC_BITS + 2;
  require PRODUCT_WIDTH        == WEIGHT_FRAC_BITS + VALUE_WIDTH + 2;

  ...
end module Bf16PvTileEngine
```

With these constraints in place, when `Bf16PvTileEngine` is instantiated inside
`AttentionTileEngine` and the `PRODUCT_WIDTH` scoping bug leaks the wrong value
(32 instead of 38), the compiler would immediately report:

```
Error: require constraint violated in instantiation of `Bf16PvTileEngine`
    ╭─[rtl/arch/AttentionTileEngine.arch:23:3]
 23 │   inst pv_eng: Bf16PvTileEngine
    ·   ──────────────────┬──────────
    ·                     ╰── instantiated here
    ╰────

  note: violated constraint:
    ╭─[rtl/arch/Bf16PvTileEngine.arch:12:3]
 12 │   require PRODUCT_WIDTH == WEIGHT_FRAC_BITS + VALUE_WIDTH + 2;
    ╰────

  note: PRODUCT_WIDTH resolved to 32 but WEIGHT_FRAC_BITS + VALUE_WIDTH + 2
        evaluates to 38 (WEIGHT_FRAC_BITS=7, VALUE_WIDTH=31)
        — this likely indicates a local param name collision with another dependency
```

The engineer sees the scoping problem immediately, rather than chasing a
`SInt<32>` vs `SInt<38>` type error deep in the multiply expression.

---

## Novelty check

- Not in COMPILER_STATUS.md (last updated 2026-05-02, v0.60.0)
- Not in any `doc/plan_*.md` file
- Not in any `ideas/*.md` file
- `assert name: expression` is an existing construct but targets runtime/formal semantics;
  `require` is purely compile-time / elaboration-time
- The keyword `require` does not appear as a user-facing language construct in
  `src/parser.rs` or `src/ast.rs`
