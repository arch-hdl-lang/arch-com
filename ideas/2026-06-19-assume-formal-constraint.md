# Enhancement: `assume` keyword for formal verification input constraints

**Date:** 2026-06-19
**Status:** Proposal — ready for implementation discussion
**Related:** `arch formal` (BMC encoder, `src/formal.rs`), bounds-check SVA (shipped),
  div-by-zero SVA (shipped), CLAUDE.md §"Runtime divide-by-zero checking" (EBMC examples)

---

## Problem

`arch formal` proves or refutes properties by model-checking under **unconstrained symbolic
inputs**. This is correct and sound, but it routinely produces spurious "REFUTED"
counterexamples when the design's true operating envelope is narrower than its declared
input types.

The CLAUDE.md documentation captures this exactly:

> *EBMC 5.11*: `UInt<4> wr_idx` into `Vec<_,4>` ⇒ **REFUTED** at the leaf module
> (unconstrained input can reach 15; caller must constrain). Same access with
> `UInt<2> wr_idx` ⇒ **PROVED up to bound 10** (structurally safe by type width).

And for divide-by-zero:

> *EBMC 5.11*: unconstrained `den: UInt<8>` ⇒ **REFUTED** (caller must gate).
> With `den_safe = den_raw | 1` ⇒ **PROVED up to bound 5**.

Both workarounds are type-width tricks: shrink the type so it can't reach the illegal
value, or compute a structurally-safe surrogate. These couple the *type* to the
*verification concern*, obscure design intent, and don't work when the constraint is
relational (e.g., "req is always one-hot") or protocol-shaped (e.g., "valid doesn't
deassert once raised").

The ARCH spec already names the solution — **§12.3 `assume`** — with example, semantics
table, and SV mapping. The keyword is mentioned in the grammar comment in `src/parser.rs`
(line 703). But the implementation stops at `AssertKind { Assert, Cover }`;
`Assume` is absent from the AST, lexer, parser, codegen, and formal encoder.

---

## Concrete example of today's pain

```arch
module OneHotArb
  port req:   in  UInt<4>;
  port grant: out UInt<4>;

  // Only one-hot or zero requests are legal in this protocol.
  // Without assume, EBMC finds req=0b1010 and refutes the grant property.
  assert grant_onehot: (grant == 0) or ((grant & (grant - 1)) == 0);
end module OneHotArb
```

Running `arch formal OneHotArb.arch` with an unconstrained `req` causes EBMC to REFUTE
the `grant_onehot` property (it can construct a multi-bit `req` that the arbiter can't
satisfy one-hot). The correct fix isn't to shrink `req` to `UInt<1>` — it's to tell
the prover which inputs are valid:

```arch
module OneHotArb
  port req:   in  UInt<4>;
  port grant: out UInt<4>;

  // Constrain the formal tool: only consider one-hot or zero inputs.
  assume req_onehot: (req == 0) or ((req & (req - 1)) == 0);

  assert grant_onehot: (grant == 0) or ((grant & (grant - 1)) == 0);
end module OneHotArb
```

With the `assume`, the prover restricts its search to the constrained input space and
can PROVE `grant_onehot` up to any bound.

---

## Proposed semantics (per spec §12.3)

| Keyword | Simulation | `arch build` SV | `arch formal` |
|---|---|---|---|
| `assert name: expr` | Error if false | `assert property (...)` | Property to prove — find violations |
| `cover name: expr`  | Log when true  | `cover property (...)`  | Reachability — find a satisfying trace |
| **`assume name: expr`** | **Silently ignored** | **`assume property (...)`** | **Input constraint — restrict solver input space** |

Simulation ignores `assume` deliberately: the constraint is for the *prover*, not the
*testbench*. The testbench author knows what inputs they're driving; the prover doesn't.

---

## Implementation approach (4 files, ~40 lines net)

### 1. `src/lexer.rs` — add `Assume` token

```rust
// In TokenKind enum, alongside Assert and Cover:
Assume,

// In keyword map:
"assume" => TokenKind::Assume,

// In Display impl:
TokenKind::Assume => write!(f, "assume"),
```

### 2. `src/ast.rs` — add `Assume` variant

```rust
pub enum AssertKind { Assert, Cover, Assume }
```

The `AssertDecl` struct is unchanged (same `name`, `expr`, `span` fields).

### 3. `src/parser.rs` — handle `TokenKind::Assume` in all 18 `Assert | Cover` sites

Each `Some(TokenKind::Assert) | Some(TokenKind::Cover)` match arm (in module, FSM, pipeline,
thread, arbiter, FIFO, RAM, and other construct parsers) becomes:

```rust
Some(TokenKind::Assert) | Some(TokenKind::Cover) | Some(TokenKind::Assume) => {
    asserts.push(self.parse_assert_decl()?);
}
```

In `parse_assert_decl` itself, add the third arm:

```rust
Some(TokenKind::Assume) => { self.advance(); AssertKind::Assume }
```

### 4. `src/codegen/mod.rs` — emit `assume property` in SV

The existing `emit_assert_decl` function gains a third arm (wrapped in `translate_off/on`
like the other two):

```rust
AssertKind::Assume => {
    format!("{label}: assume property (@(posedge {clk}){disable} {expr_str});")
}
```

The anonymous-label fallback similarly:
```rust
AssertKind::Assume => "_assume_anon".to_string(),
```

### 5. `src/formal.rs` — fold `Assume` into the base transition formula

Currently, `FormalCtx::preprocess` collects all `AssertDecl`s into `self.properties`.
`Assume` declarations must instead be folded into the **base SMT-LIB2 formula**
(the `emit_base` path) — one `(assert ...)` per cycle per `assume`, applied before any
property check:

```rust
// In preprocess: separate assume from assert/cover
for decl in module_body_asserts {
    match decl.kind {
        AssertKind::Assume => self.assume_exprs.push(decl.expr.clone()),
        _                  => self.properties.push(PropertyDecl { ... }),
    }
}

// In emit_base, inside the per-cycle loop:
for expr in &self.assume_exprs {
    let term = self.encode_expr(expr, t, None)?;
    out.push_str(&format!("(assert (= {} #b1))\n", term.s));
}
```

Because `assume` exprs are `(assert ...)` in the base, they hold for **every** cycle the
solver explores — correctly constraining the unconstrained input ports throughout the
unrolled transition relation.

A new `PropertyStatus::Assumed(u32)` variant is NOT needed: `assume` declarations are
constraints, not results. The formal report mentions them only in a header comment if
`--emit-smt` is requested, to aid debugging.

---

## Scope note: temporal forms

`assume` bodies follow the **same expression grammar** as `assert` / `cover`. All
currently supported temporal forms (`|->`, `|=>`, `past(expr, N)`, `##N expr`,
`rose(a)`, `fell(a)`) are legal inside `assume`. The cycle-range clamping that
`run_property` already applies to `assert`/`cover` temporal properties is reused
for the assume-encoding loop.

Formal `assume` with temporal content (`past(req, 1) |-> ack`) is
industry-standard (SVA `assume property`) and maps cleanly to the existing
cycle-indexed SMT encoding.

---

## Edge cases

- **No clock port:** `assume` in a clockless module — same rule as `assert`/`cover`
  (compile error; concurrent SVA needs a clock edge context). The existing check applies.
- **Simulation drop:** the existing `arch sim` path drops `assert`/`cover` at runtime
  (see COMPILER_STATUS.md §arch sim). `assume` is ignored in simulation per spec, so
  dropping it is correct — no new handling needed.
- **Multiple `assume` exprs:** the base-formula injection is additive: each one adds
  one constraint per cycle. Contradictory assumes would make the formula UNSAT and
  every `assert` vacuously PROVED — the same behavior as any over-constrained formal
  model (and EBMC will report UNSAT on the base before checking properties).
- **`arch build` SV output:** `assume property (...)` is accepted by EBMC (`--ebmc`),
  JasperGold, OneSpin, and other formal tools. Verilator silently ignores `assume`
  (it doesn't simulate assumptions). Correct behavior per spec.

---

## Why now / why this proposal wins

1. **Already in the spec** (§12.3) — not a new language concept, just a missing
   implementation of a committed design decision.

2. **Directly unblocks real formal workflows**: the examples in CLAUDE.md and
   `tests/` repeatedly note "caller must constrain" — `assume` is that constraint
   mechanism.

3. **Small, self-contained change** — 4 files, ~40 net lines, no new IR, no new
   pass. The existing `assert` infrastructure provides 90% of the scaffolding.

4. **No test-only workarounds needed**: users no longer need to narrow types or
   compute structural surrogates to avoid spurious refutations in formal.

5. **Unlocks `arch formal` as a real pre-silicon sign-off tool**: a team can add
   `assume` constraints matching the system-level protocol contract (one-hot req,
   non-zero divisor, valid-before-ready) and get provably correct results without
   touching the RTL types.

---

## Open question for team discussion

**Should `assume` warnings be emitted in `arch sim` mode?**

Option A (spec-faithful): silently skip `assume` in sim — this is correct per §12.3.

Option B (defensive): emit a one-time `INFO: assume '<name>' ignored in sim mode`
to remind the user that the constraint is formal-only and won't catch violations
in simulation.

Recommendation: Option A. The spec is clear, and Option B would produce noise on
every `arch sim` run for designs that co-locate formal constraints with RTL.
The user who writes `assume` knows it's formal-only.
