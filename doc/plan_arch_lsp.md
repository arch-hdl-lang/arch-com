# Enhancement Proposal: `arch lsp` — Language Server Protocol for Real-Time IDE Feedback

*Author: session of 2026-06-25. Status: proposal — not yet started.*

---

## One-line summary

Add an `arch lsp` command that implements the Language Server Protocol, giving any
LSP-capable editor (VS Code, Neovim, Emacs, Helix, Zed, …) real-time inline diagnostics,
hover type info, go-to-definition, and completion for `.arch` and `.harc` files.

---

## Motivation

### The gap

ARCH has two integration stories today:

| Audience | Tool | Capability |
|---|---|---|
| AI agents | MCP server (`arch://`, `write_and_check`, `arch_graph_*`) | Real-time error feedback, graph queries, construct syntax |
| Human developers | TextMate grammar + Vim syntax | **Syntax coloring only — no semantic understanding** |

When a developer reviews or edits LLM-generated `.arch` code, they get no inline error
feedback. They must context-switch to a terminal, run `arch check`, read the output, map
it back to the editor manually, and repeat. For a language with strict rules —
bit-width arithmetic, CDC domain tracking, single-driver enforcement, exhaustive FSM
transitions, operator-precedence rejects — this is a slow and error-prone loop.

LSP closes this gap for the human side exactly as the MCP server closes it for the AI agent
side. Both need the same underlying data (compiler diagnostics, type info, name resolution)
delivered through different protocols (LSP JSON-RPC over stdio vs. MCP tool calls).

### Why LSP matters for ARCH specifically

1. **Complex type rules that are hard to track mentally.** `UInt<8> + UInt<8>` → `UInt<9>`.
   Assigning that back into a `UInt<8>` reg is an error. `.trunc<8>()` is required. Seeing
   the widened type on hover, and the error inline at the assignment, makes this legible
   without running the compiler.

2. **CDC domain checking.** A signal read in the wrong domain is a compile error. Without
   hover info showing `Clock<DomainA>` vs `Clock<DomainB>`, developers have to hold the
   wiring diagram in their head.

3. **Thread lowering semantics.** A `wait until` condition that reads a signal comb-driven
   by the same thread triggers the dead-skid lint. Seeing that warning inline as you type the
   wait condition is far faster than running `arch check`.

4. **LLM-generated code review.** ARCH is explicitly designed for LLM-generated code that
   humans then review and iterate on. Good LSP tooling makes that review loop fast enough to
   be practical on large designs.

5. **Precedent in similar languages.** Rust's adoption curve bent sharply upward after
   `rust-analyzer` shipped. Verilog/SV has had `verilator_ls` and Veridian. A language that
   lacks LSP carries an invisible adoption tax.

---

## Non-goals

- Replacing `arch check` on the CLI. LSP runs alongside the CLI, not instead.
- A wave viewer or schematic renderer inside the IDE. Out of scope.
- Semantic analysis of external SV (e.g. Verilator DUT backends in HARC). Phase 1 scopes to
  `.arch` / `.harc` source only.
- Completion of signal names inside C++ testbench files. HARC codegen produces the C++ header;
  the editor's C++ plugin handles it.
- Renaming / refactoring across files. Deferred — requires reliable cross-file index.
- Inlay hints for every subexpression. The hover surface is sufficient for v1.

---

## Proposed CLI surface

```
arch lsp              # start LSP server on stdin/stdout (standard mode)
arch lsp --tcp 7878   # debug mode: accept one JSON-RPC connection on TCP
```

Editors launch `arch lsp` as a child process and communicate over stdin/stdout, which is the
universal LSP client/server contract. No extra configuration needed for any LSP client that
supports stdio transport.

The `harc` binary would get the same entry point:

```
harc lsp
```

In practice, a single server binary could handle both `.arch` and `.harc` files (shared
frontend) with the file extension disambiguating the parser entry point — matching how
`arch check` and `harc check` both compile the same IR.

---

## Phased implementation plan

### Phase 1 — Diagnostics (highest ROI, ~1 week)

**Deliverable**: inline error squiggles in any LSP client for every `arch check` / `harc check` error.

**LSP methods implemented:**
- `initialize` / `initialized` — handshake and capability advertisement
- `textDocument/didOpen` + `textDocument/didChange` + `textDocument/didClose` — document lifecycle
- `textDocument/publishDiagnostics` — push errors/warnings to the client

**Implementation sketch:**

```rust
// arch-lsp/src/main.rs  (~200 lines)
use tower_lsp::{LspService, Server};
use tower_lsp::lsp_types::*;
use arch_core::check;   // new: expose compiler as a library function

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend { client, docs: Default::default() });
    Server::new(stdin, stdout, socket).serve(service).await;
}

async fn on_change(&self, uri: Url, text: String) {
    let errors = arch_core::check(&text, uri.path());   // returns Vec<Diagnostic>
    self.client.publish_diagnostics(uri, errors, None).await;
}
```

**Key prerequisite:** expose `arch_core::check(source: &str, path: &str) -> Vec<LspDiagnostic>`
as a public API. The compiler already does this work; it just `process::exit(1)`s instead of
returning. The change is making the diagnostics pipeline return a `Vec` rather than printing
to stderr and exiting.

**What developers see immediately:**
- Red squiggles at the exact span of every type error, CDC error, precedence reject, etc.
- Hover over the squiggle to see the error message
- The "Problems" panel in VS Code lists all errors across open files

### Phase 2 — Hover (type info on demand, ~1 week)

**LSP method:** `textDocument/hover`

For any position in a `.arch` file, return:
- Signal/reg type: `reg pc: UInt<32>` → hover shows `UInt<32>`
- Port type and direction: `port data: in UInt<64>` → hover shows `in UInt<64>`
- Module signature on `inst` keyword: hover on `inst sub: Cache` shows Cache's parameter/port list
- Operator result width: hover on `a + b` where both are `UInt<8>` shows `UInt<9>` (IEEE widening)
- Enum variant value: hover on `State::Idle` shows the auto-assigned encoding

**Implementation**: the typechecker already resolves types for every AST node. Phase 2 adds
a "position-to-node" lookup that maps an (line, column) pair to the AST node and reads its
resolved type.

### Phase 3 — Go-to-definition (~3 days)

**LSP method:** `textDocument/definition`

- `inst foo: SubModule` → jump to `SubModule`'s definition (in same file or cross-file)
- `use PkgName;` → jump to `PkgName.arch`
- Signal name reference → jump to the `reg`/`let`/`wire`/`port` declaration
- Enum variant `State::Idle` → jump to the `enum State` declaration and the `Idle` arm

**Implementation**: name resolution already maps identifiers to definition spans. The change
is making those spans addressable from LSP position queries.

### Phase 4 — Completion (~1 week)

**LSP method:** `textDocument/completion`

Context-sensitive completions:
- After `port name: in ` → offer type keywords (`UInt`, `SInt`, `Bool`, `Clock`, `Reset`, `Vec`)
- After `inst foo: ` → offer module names visible in the current project
- After `seq on ` → offer clock port names declared in the module
- After `->` in an FSM state body → offer state names in the current `fsm`
- After `end ` → offer the matching opening keyword+name (`end module Foo`)
- After `kind: ` inside `ram` → offer `single`, `simple_dual`, `true_dual`, `rom`
- After `policy: ` inside `arbiter` → offer `round_robin`, `priority`, `lru`, `weighted`
- Keywords in any position → full keyword list filtered by what's legal here (e.g. `thread` inside `module`)

**Note on LL(1):** ARCH's grammar is LL(1). This is a major advantage for completion — the
prefix context is always unambiguous, so the completion provider never needs to speculatively
parse multiple alternatives.

### Phase 5 — References and rename (~1 week, lower priority)

**LSP methods:** `textDocument/references`, `textDocument/rename`

- "Find all references to `Cache` module" → list every `inst` that instantiates `Cache`
- "Rename `DataWidth` param" → update all 47 occurrences across the project

These require a cross-file index (similar to `arch graph`). Can reuse the graph index or
build a lightweight inverse-lookup table during workspace scan.

---

## Technical prerequisites

### 1. Library API for the compiler frontend

The compiler today calls `process::exit()` on fatal errors. To drive an LSP server loop
(which must never exit), the parse+typecheck pipeline needs a `-> Result<Ast, Vec<Diag>>`
return type. The `plan_compiler_refactor.md` backlog already plans splitting codegen; the
same spirit applies to the frontend — extract `arch-core` as a `lib.rs` target so external
callers (LSP, future WASM playground, test harness) can call `check(source)` programmatically.

Estimated scope: 2–3 days to add `lib.rs` with a clean public API, matching the existing
binary entry point. No compiler logic changes; just restructuring the call graph to return
errors instead of printing+exiting.

### 2. Span fidelity

The compiler already attaches `Span { file, line, col, end_line, end_col }` to every AST
node, and error messages reference these spans. Verify that all Phase 1 errors map to exact
spans (not just line numbers) and that the span's character offsets are correct — LSP uses
UTF-16 character offsets, which differs from byte offsets for non-ASCII source.

### 3. Incremental re-check

A naïve implementation re-runs `arch check` on every keystroke. This is fine for small
files (< 500 lines, < 50ms) but will stall on large hierarchical designs. Phase 1 ships
the naïve approach; a follow-up adds debouncing (200ms idle after last change) and
incremental reparse (only re-parse the changed construct, not the whole file).

### 4. Multi-file workspace awareness

`arch check` today takes file paths as arguments. For LSP, the server must discover the
full set of `.arch` / `.harc` files in the workspace, resolve `use` and `inst` references
across files, and maintain a project-level name table. The separate compilation path
(`.archi` files, `ARCH_LIB_PATH`) already defines the cross-file reference model; the LSP
server uses the same mechanism.

---

## Editor integration

### VS Code (primary target)

Update `editors/vscode/` to:
1. Set `"arch"` language ID for `.arch` / `.harc` files (already done)
2. Add a `"languageServer"` entry in `package.json` that launches `arch lsp` on activation
3. Keep the TextMate grammar as the fallback (it's instant; LSP startup takes ~100ms)

No semantic token grammar needed for Phase 1 — the TextMate grammar already colors keywords,
types, and operators. LSP adds the semantic layer on top.

### Neovim / Emacs / Helix / Zed

These editors speak LSP natively. Users just configure `arch lsp` as the server for
the `arch` / `harc` filetype in their init.lua / init.el / languages.toml, and every
Phase 1+ capability works automatically. No editor-specific plugin code needed.

---

## Acceptance criteria (Phase 1 complete)

- `arch lsp` launches and responds to `initialize` within 500ms.
- Opening a `.arch` file with a known type error causes a red squiggle at the correct
  span within 1 second on a file of < 500 lines.
- Fixing the error causes the squiggle to disappear within 1 second of the last keystroke.
- All existing integration tests continue to pass (no compiler behavior changes).
- `arch lsp --tcp 7878` accepts a JSON-RPC connection for debugging (can test with `nc`).

---

## Relationship to existing tooling

| Tool | Layer | Audience |
|---|---|---|
| TextMate / Vim grammar | Syntax only | All editors, instant |
| `arch lsp` (this proposal) | Semantic — diagnostics, types, nav | All LSP editors, human devs |
| ARCH MCP server | Semantic — tool calls for AI workflows | LLM agents |
| `arch graph` | Code-graph index — static analysis | CI, agents, reviewers |

The LSP server and the MCP server share the same underlying compiler library after Phase 1's
library extraction. The LSP server is the human developer's interface to the compiler; the MCP
server is the agent's interface. Neither replaces the other.

---

## Estimated total effort

| Phase | Deliverable | Effort |
|---|---|---|
| Library extraction | `arch-core` crate with `check()` API | 2–3 days |
| Phase 1: Diagnostics | Inline errors in any LSP editor | 3–4 days |
| Phase 2: Hover | Type info on demand | 4–5 days |
| Phase 3: Definition | Go-to-definition | 2–3 days |
| Phase 4: Completion | Context-sensitive completions | 5–7 days |
| Phase 5: References | Cross-file find-references | 3–4 days |

Phase 1 (library extraction + diagnostics) is the highest-ROI increment and can ship
independently. Phases 2–5 stack on top of the same infrastructure.
