# `!` prefix logical-not: documented and tokenized, but not parsed

**Status:** ✅ IMPLEMENTED (Option C) — owner-approved 2026-06-03. Parser now
accepts `!` as prefix logical-not; reference card notes the `!`==`not` alias.
**Found:** 2026-06-03 daily code review, while reviewing PR #493 (`&&`/`||` aliases)
**Related:** #493 (added `&&`/`||` as symbolic aliases for `and`/`or`), #488 / #485
(recurring `if !…` → `if not …` churn in NIC-400 designs)

## The inconsistency (now resolved)

ARCH advertised `!` as the symbolic spelling of logical-not in three separate
places, but the parser rejected it — only the `not` keyword parsed.

| Surface | Said `!` is logical-not? | Before | After |
|---|---|---|---|
| `doc/Arch_AI_Reference_Card.md` §3 | yes — *"use `(!a) \|\| b` …"* | mentioned `not` only | `!`==`not` noted |
| `doc/ARCH_HDL_Specification.md:7783` | yes — same `(!a) \|\| b` guidance | guidance was a parse error | now valid |
| `doc/arch.ebnf:789` `prefix_op` | yes — `"!" (* logical not *)` | grammar-only | parser conforms |
| `src/lexer.rs:350` | tokenizes `!` as `Bang` | token unused in prefix | wired up |
| `src/parser.rs` prefix-unary | **no — parse error** | — | `Bang ⇒ UnaryOp::Not` |

Repro that used to fail and now compiles:

```arch
comb
  y = !a;                 // was: × unexpected token: expected expression, found !
  impl_out = (!a) || b;   // the exact form the reference card prescribes
  nested = !(a and b);
end comb
```

## Why this was a real, recurring papercut

- **`arch advise` learning store** surfaced this exact error with a 2×
  retrieval count: `if !outs[j].b_valid` in `tests/nic400/Nic400MasterPort.arch`,
  fixed by hand to `if not outs[j].b_valid`.
- **PR #488** was titled *"restore if-not/wait-until pattern"* — the same
  `!` → `not` rewrite, churned again after a regression.
- **21** bundled `.arch` files already used `not` where a human or model would
  naturally reach for `!`.
- It was the precise sibling of the gap **#493** closed for `&&`/`||`: a
  symbolic operator the grammar documented but the front-end never wired up.

## Implementation (Option C)

One arm in `parse_prefix` (`src/parser.rs`), exactly parallel to the existing
`Tilde ⇒ UnaryOp::BitNot`:

```rust
// `not` keyword and `!` symbol are exact aliases for logical-not.
Some(TokenKind::Not) | Some(TokenKind::Bang) => { … UnaryOp::Not … }
```

**No new lowering work** — the `UnaryOp::Not` path already existed end-to-end:
typecheck (`Ty::Bool`, `typecheck.rs:2950`), SV emit (`(!o)`), native sim
(width-1 after #493), formal (`formal.rs`). `TokenKind::Not` is dispatched only
in `parse_prefix`, so no expression-start helper needed updating. No `!=`
ambiguity: `!=` is the distinct `BangEq` token (longest-match), so `a != b` is
unaffected and `!a` / `!(expr)` are unambiguous in prefix position.

Plus the matching doc note in the reference card (`!`==`not`), mirroring #495's
`&&`==`and` / `||`==`or` note. The EBNF already listed `!` in `prefix_op` and
the #495 alias note already covered `not`==`!` — aspirational then, accurate now.

## Tests

- `src/lexer.rs::test_bang_vs_bang_eq` — `! != !a` lexes as `Bang BangEq Bang Ident`.
- `tests/integration_test.rs::test_bang_prefix_is_logical_not_alias` — `!a`,
  `(!a) || b`, `!(a and b)` lower to SV `!`; `a != b` unaffected; and the whole
  module lowers **byte-identically** to its `not`/`or` keyword spelling.
- Full suite green: 558 integration tests, 25 lexer unit tests, 0 failures.

## Scope / non-goals

- Logical-not only (`!`). Bitwise complement stays `~` (`BitNot`), unchanged.
- No precedence change: `!` takes the same prefix-unary tier the EBNF already
  assigned it, identical to `not`.
