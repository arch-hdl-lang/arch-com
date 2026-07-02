# Proposal: a minimal ARCH Language Server (`arch-lsp`)

*Author: scheduled enhancement-scouting session, 2026-07-02. Status: idea for
discussion — no code changes proposed yet.*

## Problem

Editor support for `.arch` today is TextMate-grammar-only:
`editors/vscode/` ships a `syntaxes/arch.tmLanguage.json` for coloring plus a
one-line README ("Syntax highlighting for ARCH HDL"), and `editors/vim/` is
the vim-classic `syntax/` + `ftplugin/` + `ftdetect/` triad — also coloring
only. Neither wires up diagnostics, hover, or go-to-definition. Confirmed via
`grep -rli "language server|lsp" doc/ README.md AGENTS.md` — no prior design
note or issue proposes one, and GitHub issue/PR search for
`lsp`/`language server` against this repo returns zero hits.

**Not to be confused with:** the existing "ARCH MCP server" (COMPILER_STATUS.md
§Tool, `get_construct_syntax` / `write_and_check` / `arch_build_and_lint` /
`arch_advise` / `arch_graph_query` etc.). That server exposes compiler
capabilities as *tool calls for an AI agent* (Claude, etc.) over MCP — it is
not consumed by an editor's native LSP client and doesn't give a human typing
in VS Code/Neovim real-time squiggles, hover, or jump-to-definition in the
editor UI itself. This proposal is about the standard Language Server
Protocol (`textDocument/publishDiagnostics`, `textDocument/hover`,
`textDocument/definition`) that any LSP-capable editor speaks natively — a
different protocol, a different consumer, and (per the searches above) not
yet proposed anywhere in either repo.

The practical effect: an `.arch` author (human or an LLM co-editing loop,
which is the language's explicit target audience per `CLAUDE.md` — "designed
to be generated correctly by LLMs from natural-language hardware
descriptions") gets no red squiggles, no hover type info, and no
jump-to-definition. Every check is a manual `arch check` round-trip in a
terminal, parsed by eye out of miette's pretty-printed (human-oriented, not
machine-parseable) output. That's the same iteration-speed tax that made
LSPs table-stakes for TypeScript/Rust/Go; it's a bigger tax here because the
typical edit loop is "LLM proposes a diff → human or agent wants instant
feedback on whether it type-checks" rather than "human types and pauses to
think."

## Why now / why ARCH is well-positioned for a *cheap* LSP

Most of the hard parts already exist as CLI-shaped compiler passes — an LSP
here is mostly plumbing, not new compiler engineering:

- **Diagnostics**: `arch check` already runs the full parse + type-check
  pipeline and produces span-carrying `miette::Report`s
  (`src/main.rs` uses `miette::{NamedSource, Report}` throughout). It's not
  machine-readable yet (fancy terminal rendering only), but the spans are
  already there — just not serialized.
- **Symbols / go-to-definition**: `arch graph index` (COMPILER_STATUS.md,
  "Implemented → CLI" table) already builds `nodes.jsonl` / `edges.jsonl`
  with per-construct file/span metadata, plus `query`, `callers`, `impact`,
  and `context` read commands. This is exactly the index an LSP needs for
  `textDocument/definition` and `textDocument/references` — it just isn't
  wired to the LSP request shape.
- **Hover types**: the type checker already renders human-readable `Ty`
  strings for every error message; the same formatter gives hover text.
- **Doc comments**: `///`/`//!` doc comments are already parsed and attached
  to AST nodes (`doc/plan_arch_doc_comments.md`) — free `hover` markdown.

So the LSP doesn't need a second front-end; it needs a thin JSON-RPC-over-
stdio shim that calls into the existing `arch check` / `arch graph`
machinery and reshapes the output.

## Proposed scope (incremental, each phase independently shippable)

**Phase 0 — machine-readable diagnostics (foundation, useful even without an LSP).**
Add `arch check --error-format json` (name TBD) that serializes the existing
miette diagnostics to a JSON array of
`{severity, code, message, file, span: {start_line, start_col, end_line, end_col}}`.
`serde_json` is already a dependency (`Cargo.toml`), so this is additive, not
a new dependency. This alone is useful for CI (GitHub Actions
`::error file=...::` annotations) independent of any editor work — worth
landing even if the LSP phases stall.

**Phase 1 — diagnostics-only LSP.**
A new `src/bin/arch_lsp.rs` (or separate `arch-lsp` crate under the same
workspace) speaking LSP over stdio: on `didOpen`/`didSave`, shell out to
`arch check --error-format json <file>` and republish via
`textDocument/publishDiagnostics`. No new parsing/typechecking logic — pure
plumbing around the phase-0 output. This is the highest-value, lowest-risk
slice: red squiggles in the editor with zero duplicated compiler logic.

**Phase 2 — hover + go-to-definition via `arch graph`.**
On `textDocument/hover` / `textDocument/definition`, look up the symbol at
the cursor position against a `.archgraph` index (building it on first
request if missing/stale, same freshness contract the CLI already has).
Resolve through `defines`/`uses_type`/`calls` edges and return the target
span from `nodes.jsonl`. Attach doc-comment text to hover where present.

**Phase 3 — editor wiring.**
`editors/vscode/`: extend the existing extension with a `vscode-languageclient`
activation that spawns `arch-lsp` (config point for a custom binary path,
mirroring how most LSP extensions work). `editors/vim/`: a documented
minimal `nvim-lspconfig` snippet (no new vimscript machinery needed — Neovim's
built-in LSP client just needs a command + filetype registration).

## Implementation notes / constraints

- Keep the JSON-RPC transport dependency-light. LSP-over-stdio is a small
  enough protocol (`Content-Length` framed JSON-RPC) that a hand-rolled
  reader/writer avoids pulling in `tower-lsp`'s async runtime if the project
  prefers to stay off `tokio`; alternatively `lsp-server` + `lsp-types` (sync,
  no async runtime forced) is a lighter-weight off-the-shelf option than
  `tower-lsp`. Either way this is a build/dependency decision for whoever
  picks this up, not settled here.
- Per this repo's compiler-freshness rule (`CLAUDE.md` "Never invoke a stale
  compiler binary"), the LSP process must invoke the **same** `arch` binary
  build it ships alongside (or re-exec via `cargo run --release --` in dev),
  not a cached path — otherwise diagnostics silently drift from what `arch
  check` on the command line reports.
- Scope this to ARCH first. HARC's equivalent graph index is tracked
  separately and not yet built (harc-com issue #463, open) — once it lands,
  the same `arch-lsp` architecture (diagnostics-json + graph-backed
  hover/definition) should port directly to a `harc-lsp` with minimal new
  design work, since both compilers already share the CLI-shaped-passes
  philosophy.
- No new compiler passes are needed for phases 0–2 — this is intentionally
  scoped to *exposing* existing internals over a standard protocol, not
  building new analysis.

## Open questions

- `lsp-server`/`lsp-types` vs. hand-rolled transport — depends on whether the
  project wants to take on an async runtime dependency at all.
- Incremental re-check granularity: whole-file re-check on every keystroke
  (via `didChange`) is likely too slow once designs get large; may want to
  debounce to `didSave` for v1 and revisit incremental re-check only if it
  proves to be a real bottleneck.
- Should `arch graph index` auto-run (and auto-refresh) inside the LSP
  process, or should the LSP degrade gracefully (diagnostics-only) when no
  `.archgraph/` is present and tell the user to run `arch graph index`
  once? Leaning toward the latter for v1 simplicity.

## If this doesn't get picked up

No action needed — this is a discussion-starter, not a commitment. If the
team decides in-editor diagnostics aren't the next priority, the phase-0
JSON diagnostics output is still worth keeping in mind independently, since
it's useful for CI annotations regardless of whether an LSP ever lands.
