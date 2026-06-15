# `arch fmt` — Canonical ARCH Source Formatter

**Date:** 2026-06-15  
**Status:** Proposal (not yet on roadmap)

---

## Problem

ARCH source files today have no canonical format. LLMs, humans, and auto-generated code all emit
different indentation, spacing, and line-break conventions. In practice this causes three pain
points:

1. **Merge noise.** PRs that touch shared ARCH files accumulate whitespace-only diffs that bury
   real logic changes and cause avoidable conflicts.
2. **LLM drift.** ARCH is explicitly designed for LLM codegen. Different LLMs (and different
   prompts to the same LLM) produce different whitespace. The MCP server's `write_and_check()`
   round-trip catches type errors but leaves formatting inconsistent.
3. **Learning retrieval noise.** The `arch advise` BM25 index works over stored diffs. An
   error→fix pair where the fix also reformats the file has a larger diff than the semantic
   change; that dilutes retrieval precision for logically identical future queries.

There is no `arch fmt` subcommand, no plan file, and no mention of formatting tooling anywhere
in the roadmap.

---

## Proposal

Add `arch fmt` — a source formatter that parses `.arch` files and re-emits them with a single
deterministic layout. Analogous to `gofmt`, `rustfmt`, and `verible format` in their respective
ecosystems.

### CLI surface

```
arch fmt [--check] [--in-place] [files...]
```

| Flag | Behaviour |
|---|---|
| *(default)* | Print formatted output to stdout |
| `--in-place` | Rewrite files in place (like `gofmt -w`) |
| `--check` | Exit 1 if any file would be reformatted; print unified diff to stderr. CI gate. |

### Formatting rules (proposed canonical style)

These match the CLAUDE.md examples and the `doc/ARCH_HDL_Specification.md` snippets:

| Rule | Detail |
|---|---|
| Indentation | 2 spaces per nesting level; no tabs |
| Keyword/end lines | `keyword Name` and `end keyword Name` on their own lines |
| Blank lines | One blank line between top-level constructs; none between body items |
| Trailing whitespace | Stripped on every line |
| Line endings | LF (Unix); CR stripped |
| Comments | Preserved verbatim, re-indented to match context; `///` / `//!` attachment preserved |
| Literals | Normalised to `N'hXXXX` (uppercase hex) for hex literals; lowercase for identifiers |

The formatter is **opinionated and non-configurable** (same philosophy as `gofmt`). One canonical
style; no style-guide debates.

### Implementation approach

The parser already builds a full typed AST (`src/ast.rs`, `src/parser.rs`). The formatter is a
separate AST printer that walks the same tree and re-emits it:

1. Add `src/fmt.rs` — an `AstPrinter` struct that walks `Item` / `Stmt` / `Expr` variants and
   emits text with tracked indentation depth.
2. Add `Command::Fmt` arm in `src/main.rs` (mirrors `Command::Check` but calls `fmt::format()`
   instead of the type checker).
3. Add `arch_fmt(path, content)` tool to the MCP server (`mcp/`) alongside the existing
   `write_and_check` and `arch_build_and_lint` tools.
4. Thread through `--check` mode: diff the formatted output against the original bytes; if they
   differ, emit the diff and exit 1.

**Key constraint:** The formatter must be a pure AST→text pass — it must NOT require
elaboration or type-checking to succeed. A file with type errors should still be formattable
(same as `gofmt` formats files that do not compile). This means the formatter works on
`parse()` output alone, before `typecheck()` / `elaborate()`.

**Comment preservation** is the hardest part. ARCH comments are not attached to AST nodes
today (they are lexed and discarded). Two options:
- (a) **Phase 1 easy path:** format only constructs, strip standalone comments. Useful for
  LLM-generated output which rarely has meaningful comments mid-body.
- (b) **Phase 2 full fidelity:** attach comments to AST nodes during parsing (lexer emits
  a token stream including trivia; parser attaches trivia to the nearest following node).
  Standard in `syn` / `rustfmt`; non-trivial but not novel.

Phase 1 is a useful MVP that covers the primary use case (LLM output normalization). Phase 2
can follow once the basic formatter exists and the team wants comment fidelity.

### Integration with existing tooling

- **MCP server:** Add `arch_fmt(path, content) -> string` — format content in-memory and
  return the formatted text. AI agents calling `write_and_check()` can first call `arch_fmt()`
  to normalise before writing. No disk I/O required for the in-memory form.
- **Learning store:** `arch check` → `arch advise` indexing already diffs on error→fix pairs.
  If both sides of a pair are pre-formatted, the diff captures only the semantic fix.
- **CI gate:** Adding `arch fmt --check src/` to the CI matrix catches drift before it
  accumulates, similar to how `cargo fmt --check` works in Rust projects.
- **VSCode extension:** The existing TextMate grammar (`editors/vscode/`) could expose a
  `Format Document` command that shells out to `arch fmt --in-place`.

---

## Why this matters

ARCH's primary design goal (CLAUDE.md) is to be LLM-generatable. LLM output is the input to
`arch check` and `arch build`. Today every LLM session produces slightly different whitespace,
making diffs noisier than they need to be and making the learning store's retrieval slightly
less precise.

A formatter closes that gap: LLM output → `arch fmt` → canonical → `arch check` → clean diff.
The MCP server can bake this into the `write_and_check()` call so the normalization is
invisible to agents.

Ecosystem precedent is strong: `gofmt` single-handedly ended style debates in the Go community.
`rustfmt` is a first-class Rust tool. `verible format` covers SystemVerilog. ARCH having a
canonical formatter is the natural next step for a language that takes LLM toolability seriously.

---

## Rough effort estimate

| Phase | Work | Effort |
|---|---|---|
| Phase 1 MVP | AST printer (no comment fidelity), CLI plumbing, `--check` mode | ~1–2 days |
| MCP tool | `arch_fmt()` in-memory tool | ~0.5 days |
| Phase 2 | Comment trivia attachment + fidelity preservation | ~2–3 days |
| CI integration | Add `arch fmt --check` job, fix drift in existing tests | ~0.5 days |

Total MVP (Phase 1 + MCP): ~2–3 days of implementation.

---

## What this idea is NOT

- It does not change the language grammar or semantics.
- It does not require changes to the type checker, elaborator, or any codegen backend.
- It does not enforce naming conventions (PascalCase / snake_case / UPPER_SNAKE) — those are
  recommended but not compiler-enforced and stay that way.

---

## Novelty check notes

Searched `src/`, `doc/`, and open/closed issues + PRs for:
- "arch fmt", "formatter", "pretty print" → no hits in plan docs or issues
- `--format` CLI flag → not in `src/main.rs`
- `editors/vscode/` → TextMate grammar only; no format-on-save integration

This idea is not on the current roadmap.
