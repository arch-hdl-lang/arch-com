# Plan: `Vec` methods with implicit `item` / `index` binders

*Author: session of 2026-04-18. Status: design draft; not yet implemented.*

## Motivation

The one pattern that comes up every time someone reads for `break` in an HDL
is "search a `Vec` for something." That pattern — parallel compare +
priority encoder — is verbose to hand-write, error-prone (off-by-one in the
encoder, forgotten "found" flag), and indistinguishable between designs.
Giving `Vec` a small method family hides the encoder behind a named verb
and matches the style of `.trunc<N>()`, `.zext<N>()`, `.reverse(1)` that
ARCH already ships.

This plan covers both the method catalog and the **predicate / expression
binding syntax** — how the predicate references the iterated element
without introducing lambdas that would conflict with ARCH's existing
grammar.

## The binder decision: implicit `item` / `index`

ARCH doesn't have lambdas, and the common surface syntaxes clash with
existing grammar:

| Style | Conflict with ARCH |
|---|---|
| `vec.find(\|e\| e == x)` (Rust) | `\|` is bitwise OR; `vec.find(a \| b \| c)` would be ambiguous |
| `vec.find(e => e == x)` (Scala / JS) | `=>` is reserved for reset clauses (`reset rst => 0`) |
| `vec.find(\e -> e == x)` (Haskell) | introduces backslash as a new token just for this |
| `vec.find { it == x }` (Kotlin) | `{ }` is foreign to ARCH block grammar |
| **`vec.find(item == x)` (SV-style, implicit)** | **no new tokens, no grammar ambiguity** |

SystemVerilog already uses this convention (`q.find_first_index with (item == x)`),
and hardware engineers know it. For ARCH we drop the `with` keyword — the
compiler already knows "this is a Vec method argument position," so the
implicit binder is unambiguous from context.

**Two implicit identifiers bound inside predicate / expression arguments of
Vec methods:**

| Name | Bound to |
|---|---|
| `item` | The element of the Vec at this iteration position. Type = element type of the receiver. |
| `index` | The integer index of this iteration. Type = `UInt<clog2(N)>` where `N` is the Vec length. |

### Scoping rules (strict)

- `item` / `index` are **only** in scope inside the argument expression(s)
  of a Vec method call. Referencing them anywhere else is a **compile error
  with a precise diagnostic** — "`item` is only valid inside a Vec method
  predicate; use an explicit signal name here."
- They **shadow** any enclosing identically-named signal for the duration
  of the predicate. (If a user declares `wire item: T;` at module scope and
  then calls `vec.any(item == x)`, the method's `item` wins inside the
  argument; the warning "method-binder shadows enclosing signal `item`" is
  emitted so the user can rename their wire.)
- Nested Vec method calls: inner predicate's `item` / `index` shadow the
  outer's for the duration of the inner call. Example:
  `vec2d.any(row.any(item != 0))` — inner `item` is a Vec-element scalar.
- Closures over enclosing *signals* (module-scope wires, ports, params) work
  normally: `vec.find_first(item == needle)` reads `needle` from enclosing
  scope just like any other expression. This is not a capture; it's a
  standard identifier lookup.

## Method catalog

### Initial shipping scope (v1)

| Method | Signature | Hardware shape |
|---|---|---|
| `vec.find_first(pred: Bool)` | `(found: Bool, index: UInt<clog2(N)>)` | N parallel compares + priority encoder. Canonical "search" operation. |
| `vec.any(pred: Bool)` | `Bool` | N parallel compares + OR-reduce |
| `vec.all(pred: Bool)` | `Bool` | N parallel compares + AND-reduce |
| `vec.count(pred: Bool)` | `UInt<clog2(N+1)>` | N parallel compares + popcount tree |
| `vec.contains(x: T)` | `Bool` | Shorthand for `vec.any(item == x)` — no predicate needed |

Predicate argument type is `Bool`. The compiler type-checks the predicate
expression with `item: T_elem` and `index: UInt<clog2(N)>` injected into
scope.

### v2 candidates (not shipping with v1)

These raise real design questions (multi-binder predicates, accumulator
types, index-aware maps) that are cleaner to address after v1 usage lands.

| Method | Notes |
|---|---|
| `vec.map(expr)` | Returns `Vec<U, N>` where `U` is the expression's inferred type. Element-wise combinational transform. |
| `vec.fold(init, expr)` | Reduction tree. Introduces a second binder `acc` for the accumulator — needs design decision on evaluation order (left-fold vs tree-fold for associative ops). |
| `vec.zip(other)` | Returns `Vec<(T,U), N>`. Tuple type in ARCH is not yet first-class. |
| `vec.find_last(pred)` | Reverse priority encoder; symmetric to `find_first`. |
| `vec.take_while(pred)` / `vec.drop_while(pred)` | Need the "index of boundary" primitive; composable from `find_first`. |
| `vec.index_of(x)` | Shorthand for `vec.find_first(item == x)`; arguably redundant once `find_first` exists. |
| `vec.reduce_or` / `vec.reduce_and` / `vec.reduce_xor` | Reductions without a predicate; shortcuts for common fold patterns. Worth shipping in v1 actually — simple and unambiguous. |

I'd promote `reduce_or` / `reduce_and` / `reduce_xor` into v1 as a parallel
family since they're zero-argument and cost almost nothing to implement.

## Syntax examples

```
// v1 methods, single predicate binder
let first_digit: (found: Bool, index: UInt<clog2(N)>) =
    chars.find_first(item >= 48 && item <= 57);

let any_high:   Bool = flags.any(item);
let all_valid:  Bool = valid_vec.all(item);
let n_ones:     UInt<clog2(N+1)> = bits.count(item == 1'b1);
let has_needle: Bool = haystack.contains(needle);

// Using index
let first_after_start: (found, index) =
    chars.find_first(item == needle && index >= start);

// Shadows outer signal (with warning)
wire item: UInt<8>;
let _found: Bool = vec.any(item == 0);  // inner `item` = Vec element; outer shadowed

// v2-era examples (not yet)
let doubled: Vec<UInt<9>, N> = vec.map(item +% item);
let sum:     UInt<W+clog2(N)> = vec.fold(0, acc + item);
```

## Codegen per method

All v1 methods lower to pure combinational SV inside the emission site:

```systemverilog
// find_first lowering (conceptual)
logic [N-1:0] _find_first_hit;
for (genvar i = 0; i < N; i++) begin
  assign _find_first_hit[i] = <predicate with item=vec[i], index=i>;
end
logic _find_first_found;
logic [$clog2(N)-1:0] _find_first_idx;
assign _find_first_found = |_find_first_hit;
// Priority encoder: lowest set bit wins
always_comb begin
  _find_first_idx = '0;
  for (int k = N-1; k >= 0; k--) begin
    if (_find_first_hit[k]) _find_first_idx = k[$clog2(N)-1:0];
  end
end
```

Sim codegen emits the natural C++ equivalent — for-loop over `N` elements,
break-on-hit in simulation (break is fine in software!), to avoid quadratic
work at large N.

Formal codegen: the unrolled compares become N clauses in SMT; priority
encoder becomes a set of `ite` expressions. Within-scope for `arch formal`.

## Non-trivial details

1. **Type inference for predicate**: inside the argument expression, the
   parser sees bare `item` as an identifier. The typecheck pass must, when
   checking a Vec method call, push `item: T_elem` and `index: UInt<clog2(N)>`
   onto the local symbol scope *before* type-checking the predicate. This
   is the only new machinery.

2. **Empty-Vec case**: a Vec literal or param-driven Vec with `N=0` should
   be a compile error at the method call site (`find_first` on zero-length
   Vec has no meaningful index type). The type checker already rejects
   `Vec<T, 0>` constructions, but let's add a method-site check in case
   generate-driven constructions can produce N=0 here.

3. **Predicate with side effects**: predicates must be pure combinational.
   Assignments inside the predicate (`item = 0`) are a compile error,
   matching today's rules for `let` and `wire` RHS expressions. No new
   wording needed; the existing "no assignments in expressions" rule
   applies.

4. **Index width for large Vecs**: `UInt<clog2(N)>` is the natural index
   width, but `clog2(1) = 0` is a corner case. Force minimum width 1 for
   `N == 1` — even though the index is trivially 0, having a 0-bit type
   is awkward. Well-trodden territory; `counter` already handles this.

5. **Precedence with bitwise operators in predicates**: `vec.any(item & mask)`
   — is `item & mask` a Bool predicate or a partially-applied operation?
   Existing operator-precedence rules apply unchanged. The predicate's
   result type is checked to be `Bool`, so `item & mask` (which returns
   `UInt<W>`) fails the check with a clear error: "predicate must be Bool,
   got UInt<8>. Did you mean `(item & mask) != 0`?"

## Implementation roadmap

### Step 1 — Parser + AST
- Recognize `<expr>.<method>(<args>)` where `<method>` is one of the v1
  method names. Today this probably already parses as `MethodCall`; verify
  and route to a dedicated AST node only if the existing one doesn't
  preserve enough information for the binder injection (e.g., it may need
  an `is_vec_method: bool` hint).

### Step 2 — Typecheck
- New helper `typecheck_vec_method_call(...)` that:
  1. Verifies the receiver's type is `Vec<T, N>`.
  2. Pushes `item: T`, `index: UInt<clog2(N)>` onto the local scope.
  3. Type-checks each argument expression against the method's expected
     predicate / value type.
  4. Pops the scope.
  5. Returns the method's result type (tuple for `find_first`, scalar
     otherwise).
- Diagnostic for bare `item` / `index` outside a Vec method predicate.
- Diagnostic for `item` / `index` shadowing an enclosing signal (warning,
  not error).

### Step 3 — Codegen
- One lowering helper per method. All combinational; all emit parallel
  compares + reduction/priority logic.
- Sim codegen emits equivalent C++ using native `for` (with break inside
  the software loop for efficiency — not hardware-visible).

### Step 4 — Tests
- Integration test per method covering: basic case, empty Vec (error),
  nested method calls, `item` / `index` in compound predicates, shadowing
  warning.
- Snapshot test for SV emission shape.

### Step 5 — Docs
- Add `**3.X Vec methods**` subsection to `ARCH_HDL_Specification.md`
  documenting the catalog, binder rules, scoping, and examples.
- Add a **Vec methods** card to `Arch_AI_Reference_Card.md`.
- Update `doc/COMPILER_STATUS.md` when shipped.

## Non-goals

- **General-purpose closures.** This plan does not introduce lambdas to
  ARCH. `item` / `index` are context-bound magic identifiers, not
  user-declarable closure parameters. If genuine closures turn out to be
  needed later (e.g., for user-defined higher-order functions), that's a
  separate design.
- **Running methods on non-Vec types.** Struct / scalar methods are
  already covered by `.trunc/.zext/.sext/.reverse`; this plan is strictly
  about Vec-shaped receivers.
- **Multi-binder predicates.** v1 ships with a single `item` (plus
  position-aware `index`). Fold-style `acc + item` belongs in v2 because
  it requires design around the accumulator type, order of evaluation,
  and reduction-tree associativity for large N.
- **Runtime "break" semantics.** Nothing in this plan adds sequential
  control-flow primitives; all method lowerings are purely combinational.

## Risks

- **Shadowing confusion.** A user who already has a wire named `item` at
  module scope and tries `vec.any(item == 0)` will get the Vec element
  bound inside the predicate, not their wire. The shadowing warning
  should point this out explicitly and suggest renaming the outer wire
  or using a different expression inside the predicate.
- **Quadratic hardware for large N**. `find_first` on N=256 generates 256
  compares plus a priority encoder — that's fine, but a user applying
  `map` + `find_first` + `fold` in a chain is quadratic in compares. No
  lint today; may want one later.
- **Temptation to over-build v1**. The v1 scope is deliberately small
  (5 methods) to ship a real, useful feature quickly. Resist the urge to
  land `map` / `fold` / `zip` in v1 — they raise real questions about
  tuple types and accumulator binders that aren't yet answered.
