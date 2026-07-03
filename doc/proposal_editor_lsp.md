# Proposal: a real Language Server (diagnostics + go-to-definition + hover) for ARCH and HARC

*Status: research note / discussion. No implementation in this note.*

## Context

Both `editors/` directories (`arch-com/editors/`, `harc-com/editors/`) are
deliberately mirrored 1:1 — same Vim/VSCode layout, same maintenance note
("hand-maintained against `src/lexer.rs`'s keyword set"). But what they ship
today is **regex-based syntax highlighting only**:

- `editors/vscode/package.json` declares a `grammars` contribution
  (`arch.tmLanguage.json` / `harc.tmLanguage.json`, TextMate) and nothing else
  — no `languageserver`/`client` activation, no `main` entry point.
- `editors/vim/syntax/*.vim` is a `syn keyword` list.
- `doc/COMPILER_STATUS.md` records this accurately: "VSCode syntax extension —
  TextMate grammar … covers all keywords, types, operators, numeric literals,
  comments." No LSP row exists because there is no LSP.

I checked open issues/PRs in both repos and found no in-flight or proposed
work on this (the only tooling-adjacent open items are HARC #463 / ARCH #592,
a *compiler-native code graph* for AI-agent context — complementary, not a
substitute; see below).

## Problem

Both languages already have exactly the compiler-side machinery a real LSP
needs, just not wired to `textDocument/*`:

- `src/diagnostics.rs` is `miette`-based with real `SourceSpan`s on every
  variant (`UnexpectedToken`, `UndefinedName`, `TypeMismatch`,
  `WidthMismatch`, …) — this is already an LSP `Diagnostic` in disguise.
- `arch check` / `harc check` run the full parse → typecheck pipeline and
  already enforce the things a human most wants flagged inline: bit-width
  safety, clock-domain mismatches, single-driver, all-ports-connected,
  exhaustive FSM transitions, the four precedence foot-guns, naming.
- Separate compilation (`.archi` interface files, `ARCH_LIB_PATH`) and
  HARC's transactor/bus/regblock/addrmap constructs are exactly the places
  where plain-text search or "grep for the name" breaks down — a `bus`
  definition, a stdlib `BusAxiLite.arch`, or a TLM `tlm_method` signature
  can live in another file entirely.

Today, none of that reaches the editor. A user (or an LLM agent iterating on
generated `.arch`/`.harc`) only sees these diagnostics by running the CLI in
a terminal and manually mapping error text back to a line number. There's no
in-editor squiggle, no go-to-definition for a module/bus/struct/enum/tlm
method, no hover showing an inferred width or a `///` doc comment. This is a
bigger gap for ARCH/HARC than it would be for a mainstream language, because
the whole design premise (per `CLAUDE.md`) is that source is often
LLM-generated and a human is reviewing/iterating on it — tightening that
review loop is squarely in-scope for the project's stated goals.

## Proposal

Add a minimal, diagnostics-first Language Server to each compiler, reusing
existing compiler internals rather than building new analysis:

1. **New subcommand**: `arch lsp` / `harc lsp`, a stdio JSON-RPC server
   (`lsp-server` + `lsp-types` are small, dependency-light crates; no need
   for the heavier `tower-lsp` async stack for a single-threaded, in-process
   compiler).
2. **Diagnostics (phase 1, highest value / lowest complexity)**: on
   `didOpen`/`didChange`, run the existing parse+typecheck pipeline against
   the in-memory buffer (not the on-disk file) and translate the existing
   `miette::Diagnostic` + `SourceSpan` values directly into
   `textDocument/publishDiagnostics` — this is a formatting shim over data
   the compiler already produces, not new diagnostic logic.
3. **Go-to-definition / hover (phase 2)**: the type checker / resolver
   (`resolve.rs`, `elaborate.rs` in ARCH; the analogous IR-lowering pass in
   HARC) already builds a name → definition-site table to resolve modules,
   buses, structs, enums, and TLM methods (including across `.archi`
   interface files). Expose that table for `textDocument/definition`, and
   pair it with inferred type/width + the construct's `///` doc comment for
   `textDocument/hover`.
4. **Editor wiring**: extend the *existing* `editors/vscode/` extension
   (don't ship a second one) to spawn `arch lsp`/`harc lsp` as the language
   client backend, matching the "mirrored across both repos" pattern the
   editors directories already follow.

## Why this is the right scope

- No new compiler analysis is required for phase 1 — it's a translation
  layer over `diagnostics.rs`, which already exists and is exercised by
  every `check`/`build`/`sim` run.
- It's naturally incremental: diagnostics alone (phase 1) already closes
  the biggest gap (no live feedback loop) and can ship independently of
  go-to-definition/hover.
- It composes with, rather than duplicates, HARC #463 / ARCH #592 (the
  proposed compiler-native code graph for AI-agent context): the graph
  indexer is aimed at bulk/offline agent queries (`harc graph
  tests-for-dut`, impact analysis); an LSP is aimed at the interactive,
  single-file, keystroke-latency editing loop. Both can eventually share
  the same underlying symbol/definition data once the graph work lands,
  but neither blocks the other.

## Open questions

- Shared crate vs. per-repo implementation: ARCH and HARC have separate
  `Cargo.toml`s and diverge in AST/IR shape, but the LSP protocol glue
  (stdio framing, position/span conversion, capability negotiation) is
  identical — worth a small shared crate if both are pursued, otherwise
  duplicate the ~200 lines of protocol boilerplate per the "mirrored
  editors/ directory" precedent already established.
- Incremental re-check granularity: whole-file re-typecheck on every
  keystroke is almost certainly fine at current fixture sizes, but should be
  debounced (e.g. re-check on a short idle timer, not every keypress) rather
  than optimized prematurely.
- Multi-file projects (an edited file with unresolved `.archi` deps elsewhere
  in the workspace) need a workspace root / `ARCH_LIB_PATH`-equivalent
  configuration surface in `initialize`'s `initializationOptions` — scope
  this out for a v1 that only diagnoses single-file, no-cross-file-import
  fixtures, and extend once that's working.

## Related

- `doc/COMPILER_STATUS.md` "VSCode syntax extension" / "Vim syntax" rows
  (current state: highlighting only)
- HARC #463 / ARCH #592 — compiler-native code graph index (complementary,
  bulk/offline use case vs. this proposal's interactive use case)
- `src/diagnostics.rs` (ARCH) — existing `miette`-based diagnostics with
  `SourceSpan`s, the direct input to phase 1
