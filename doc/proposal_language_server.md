# Proposal: a language server (LSP) for ARCH, sharing an indexer core with HARC

Status: research note / discussion. No implementation in this note.

## Motivation

Both `arch-com` and `harc-com` ship hand-maintained *static* editor support
today — `editors/vim/syntax/*.vim` and `editors/vscode/syntaxes/*.tmLanguage.json`
give keyword highlighting, comment strings, and bracket matching, and are
kept in sync with `src/lexer.rs`'s keyword list by hand (see each repo's
`editors/README.md` "Maintenance" section). That is the *entire* editor
experience: there is no semantic layer. Concretely, today a user (or an
agent editing `.arch`/`.harc` files in VSCode) gets:

- No inline diagnostics — a type error, an unresolved bus reference, or a
  violated FSM/param constraint is invisible until `arch check` /
  `harc check` is run from a terminal and the output is read back into the
  editor by hand.
- No hover — port types, param defaults, doc comments (once `///`/`//!`
  land, see below) are not visible at the cursor.
- No go-to-definition / find-references — jumping from an `inst Foo` site
  to `Foo`'s declaration, or from a `bus AxiLite` port reference to the
  `bus` block, means grep.
- No completion — construct keywords, port names on a known bus, or
  in-scope param names are not suggested.

This was already noticed in-repo: `doc/plan_arch_doc_comments.md` §7 lists
"IDE / LSP hover support" under "Out of scope (V1)" for the doc-comments
feature, correctly identifying doc comments as *a* consumer of an LSP —
but no issue, plan, or owner exists for the LSP itself. Searching both
repos' issues/PRs for "language server", "LSP", "lsp", "vscode" (as
distinct from the existing static grammar work) turns up nothing else.
This note is that missing plan.

## Why now

The gap compounds every time a new compile-time check ships, because each
one only reaches the user through a CLI invocation:

- #602 (FSM unreachable-state detection), #600 (param `where` clauses),
  #557 (pipeline output-coverage), #590 (pipeline wait-stage semantics) —
  each adds a `CompileError` with a precise `Span`, but that span's only
  consumer today is a terminal error line.
- #622 (context-typed float literals) and #629/#624 (BF16 rounding) are
  exactly the kind of subtle, silent-until-you-run-it semantics where
  inline hover ("this literal is FP32, not BF16 — did you mean
  `.to_bf16()`?") would catch mistakes before compile.
- #592 (ARCH code graph index) and #463 (HARC code graph index) are open,
  unimplemented issues proposing a compiler-native `nodes.jsonl` /
  `edges.jsonl` graph over exactly the same facts (`SymbolTable`,
  construct spans, instantiation/bus-binding edges) an LSP needs for
  go-to-definition and hover. Building the graph indexer and the LSP as
  two unrelated efforts risks two divergent partial indexes; scoping them
  together means the graph *is* the LSP's semantic backend, and the LSP
  is the graph's first real consumer (currently #592/#463 have no
  consumer at all — "MCP can answer a compact query" is their only
  acceptance criterion).

Since ARCH is explicitly designed to be generated correctly by LLMs from
natural-language descriptions (and in practice a large fraction of the
patches landing in both repos are agent-authored, per the PR history),
the audience for "real-time diagnostics while editing" includes coding
agents as much as humans: an agent editing a `.arch`/`.harc` file inside
an IDE-shaped tool loop benefits from the same `publishDiagnostics`
stream a human would see, without needing to shell out to `arch check`
after every edit.

## Proposed scope (v1)

Diagnostics-only, because it is the highest-value, lowest-risk slice: it
is a thin wrapper around a compiler pass that already exists and already
carries spans.

1. New binary target, e.g. `arch-lsp` (mirrored as `harc-lsp` in
   harc-com), built with `tower-lsp` + `lsp-types` (both permissively
   licensed, no new heavy deps).
2. On `textDocument/didOpen` / `didChange`: run the existing
   lex → parse → resolve → typecheck pipeline in-memory against the
   editor's buffer content (no temp files, no shelling out to the `arch`
   binary), reusing `CompileError`'s existing `Span` (already
   byte-offset or line/col — needs a one-time check which) to build
   `Diagnostic { range, severity, message }` and push via
   `publishDiagnostics`. Debounce on keystroke (e.g. 150–300ms) rather
   than re-checking every character.
3. VSCode: extend the existing `editors/vscode` extension with a
   `client.ts` using `vscode-languageclient` that spawns the `arch-lsp`
   binary (resolved via `ARCH_LSP_PATH`/`PATH`, mirroring how
   `ARCH_LIB_PATH` is already resolved for `.archi` discovery) and wires
   it as a `LanguageClient`. The existing TextMate grammar stays as-is
   for syntax coloring and as an offline fallback if the server binary
   isn't found.
4. Vim: an ALE/nvim-lspconfig snippet in `editors/vim/README` pointing at
   the same binary — no new vim-side code, since both are generic LSP
   clients.

## Proposed scope (v2, after v1 ships and is used for a while)

- **Hover.** Reuse `ConstructCommon`/`PortDecl`/`ParamDecl` metadata to
  render port type, direction, and (once `doc`/`inner_doc` land per
  `plan_arch_doc_comments.md`) the attached `///` prose, on hover over a
  port/param/instance reference.
- **Go-to-definition.** The resolver already builds a `SymbolTable`
  mapping names to definition sites for typecheck; expose that mapping
  (or the #592/#463 graph, if it has landed by then) via
  `textDocument/definition`.
- **Basic completion.** Construct keywords, in-scope param/port names,
  and bus field names on a known bus type — a fixed, syntax-driven list
  first; type-directed completion is a later phase.

## Explicitly out of scope

- **Rename / refactoring.** Needs the full reference graph, not just
  definitions; defer until #592/#463 land or the LSP's own lighter
  index proves sufficient.
- **Format-on-save.** Neither compiler has a formatter yet (the "matches
  `harc fmt` output" line in harc-com's `editors/README.md` describes an
  aspirational convention only — no `fmt` subcommand exists in
  `src/main.rs` today). Formatting is a separate proposal.
- **Semantic highlighting beyond the existing TextMate grammar.** Static
  highlighting already works; v1 only adds the layer static highlighting
  structurally cannot provide (diagnostics tied to type/resolve/const-eval
  passes).
- **Cross-file impact analysis** ("what breaks if I change this port
  width") — that is squarely #592/#463's job once implemented; the LSP
  should consume it, not reimplement it.

## Shared core across ARCH and HARC

`arch-com` and `harc-com` are sister compilers with parallel AST/resolver
shapes (this parity is already a maintained property — see e.g. harc#473
tracking ARCH/HARC operator parity, and the cross-repo TLM consistency
note in `doc/proposal_arch_harc_tlm_consistency.md`). The LSP transport
and diagnostic-shape logic (span → `Range` conversion, debouncing,
`publishDiagnostics` plumbing, the VSCode client shim) is identical
between the two languages; only the "run the compiler pipeline on this
buffer" call differs. Suggest factoring that shared transport logic into
a small crate (or just a template followed twice) so a bug fix in one
doesn't silently miss the other — the same drift risk flagged for the
TB-IR emitter duplication in harc-com#355.

## Risks / open questions

- **Incremental re-check performance** on large files — v1's "re-run the
  whole pipeline on every debounce tick" is fine for typical module
  sizes but may need incremental re-parsing for very large `.arch`/`.harc`
  files. Defer until real usage shows it's needed (mirrors the "defer
  until the perf delta is characterized" posture already taken for
  `--mt` in harc-com#316).
- **Span representation.** Needs confirming `Span` already carries
  (line, col) or only byte offsets — if only offsets, the LSP needs a
  line-index table per open document (standard, cheap, but worth calling
  out as real work, not hand-waved).
- **Separate compilation / `.archi` resolution** — the LSP needs the same
  `ARCH_LIB_PATH` discovery `arch build` uses today so cross-file
  `inst Sub: SubModule` references resolve during editing, not just at
  full-project build time.

## Suggested first PR

A `check`-only `arch-lsp` binary (scope item 1–2 above) plus the VSCode
client wiring (item 3), with no hover/goto-def yet. This is scoped small
enough to land and get real usage before committing to the v2 surface,
and it's a pure addition — no existing CLI, codegen, or spec behavior
changes.
