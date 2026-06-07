# V1 Spec — ARCH Doc Comments and Spec Front Matter

> **Status: design.** Not yet implemented. Targets v0.47.0+.
>
> Adds three lexical surfaces for attaching natural-language design intent
> to ARCH source: `///` outer doc comments, `//!` inner doc comments, and a
> YAML-frontmatter block carrying structured metadata (external `.md` spec
> references, tags, citations). The compiler ingests these into the AST
> but does not interpret them; downstream tooling (RAG indexer,
> documentation generator) consumes the raw strings.

---

## 1. Motivation

The compiler today ingests `.arch` source and emits SystemVerilog. Design
intent — the *spec* — lives entirely outside the compiler's view (markdown
docs, datasheets, the engineer's head). For the planned spec→RTL
retrieval system (see "RAG harvesting" thread), we need a stable in-source
surface for spec text. Three concerns:

- **Adjacent prose:** every construct should be able to carry a free-form
  description right next to its declaration.
- **External-spec linking:** when the authoritative spec lives in a
  separate markdown file or external doc, the source needs to *name* it so
  the indexer can pull both pieces together.
- **MCP-agent friendliness:** when an agent (e.g. via the ARCH MCP server)
  generates `.arch` code from a user-supplied design spec, it needs a
  consistent slot to write that spec into. The user passes the design spec
  to MCP; MCP instructs the agent to insert it as `///` and `//!`
  comments + front-matter; the spec rides along with the generated code.

This V1 covers the lexical/AST surface only. RAG harvesting and doc
rendering are separate work items.

---

## 2. Lexical surfaces

### 2.1 `///` — outer doc comment

Line-comment lexeme where each `///` line attaches to the **next**
construct in the file. Mirrors Rust's outer doc comment.

```arch
/// 4-channel round-robin AXI write arbiter.
///
/// Picks among threads holding the lock using a rotating priority pointer.
/// Lock release is implicit at `end lock` (no explicit unlock op).
arbiter AxiWrArb
  policy round_robin;
  ...
end arbiter AxiWrArb
```

Lexer treatment:
- A `///` token is a separate kind from `//`. Three slashes followed by any
  character (including space, but **not** a fourth slash) starts an outer
  doc-comment line that continues to end-of-line.
- `////+` (4+ slashes) is a regular line comment, not a doc comment —
  matches Rust's behavior and gives users an escape hatch for ASCII art /
  banners.
- Consecutive `///` lines are accumulated as a single attached doc-string.
  A blank line or any other token between them does *not* break the run as
  long as no item-level syntax intervenes.
- Leading single space after `///` is preserved in the AST string; the
  rendering layer strips it.

Attachment rule: the accumulated `///` lines attach to the next syntactic
item that introduces a top-level or module-body construct (`module`,
`fsm`, `arbiter`, `counter`, `fifo`, `ram`, `regfile`, `pipeline`, `cam`,
`linklist`, `bus`, `synchronizer`, `clkgate`, `package`, `domain`,
`struct`, `enum`, `function`, `thread`, `let`, `reg`, `wire`, `port`,
`inst`, `resource`). If the next item is something else (e.g. closing
keyword, end-of-file), the doc block is dropped with a warning.

### 2.2 `//!` — inner doc comment

Line-comment lexeme where each `//!` line attaches to the **enclosing**
item. Mirrors Rust's inner doc comment.

Two contexts:

- At the **top of a file**, before any item: documents the file as a whole
  (the implicit "translation unit"). Stored on `SourceFile.inner_doc`.
- **Immediately after an item's opening keyword + name**, before any
  declaration inside the item's body: documents that item from the inside.
  Stored on `ConstructCommon.inner_doc`.

```arch
//! AXI4 write arbitration utilities.
//!
//! All arbiters in this file use round-robin scheduling unless explicitly
//! marked priority.

arbiter AxiWrArb
  //! Per-bus arbiter — see top-of-file comment for shared conventions.

  policy round_robin;
  ...
end arbiter AxiWrArb
```

Inner doc comments may not appear in arbitrary positions inside a body —
only immediately after the opening keyword/name. Anywhere else is a parse
error.

### 2.3 Front matter (top-of-file YAML block)

A YAML-style frontmatter block embedded in the **leading `//!` block** of
a file, delimited by `---` on its own line. Used to carry structured
metadata that's awkward to express in free prose.

```arch
//! ---
//! spec_md: doc/specs/axi_wr_arb.md#round-robin
//! tags: [arbitration, axi, axi4]
//! refs:
//!   - "AXI4 spec §A3.3.1"
//!   - "FOO-1234"
//! ---
//!
//! 4-channel round-robin AXI write arbiter, used by all DMA channels in
//! the SoC. See `spec_md` above for the authoritative behavior contract.

arbiter AxiWrArb
  ...
```

Rules:
- The frontmatter block is the contiguous `//!` lines at the top of the
  file beginning with `//! ---` and ending at the next `//! ---`. Anything
  before `//! ---` makes it a *non*-frontmatter inner doc comment (so
  `//!` prose can come before frontmatter only if you don't open a `---`
  block).
- Inside the block, `//! ` (note the trailing space) is the prefix for
  each line; the YAML body is everything after the prefix on each line.
- The compiler does **not** parse the YAML in v1. It stores the raw text
  of the block on `SourceFile.frontmatter: Option<String>` and passes it
  through unchanged. Downstream tooling (RAG indexer) parses it.
- Recognized field semantics (interpreted by tooling, not the compiler):
  - `spec_md` (string) — relative path to an external markdown spec, with
    optional `#section` anchor.
  - `tags` (list of strings) — feature tags for retrieval.
  - `refs` (list of strings) — citations, ticket IDs, URLs.
  - Tooling MAY add fields; the compiler is forwards-compatible by virtue
    of not interpreting them.

---

## 3. AST changes

```rust
// In src/ast.rs:

pub struct SourceFile {
    pub items: Vec<Item>,
    /// Raw inner-doc-comment text from leading `//!` lines, frontmatter
    /// block included verbatim (with the `//!` prefix stripped). None
    /// when the file has no leading `//!` block.
    pub inner_doc: Option<String>,
    /// Raw text of the `//! ---\n...\n//! ---` block at the top of the
    /// file, with `//! ` prefixes stripped and `---` markers retained.
    /// None when no frontmatter is present. Always a substring of
    /// `inner_doc` when both are present.
    pub frontmatter: Option<String>,
}

pub struct ConstructCommon {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub asserts: Vec<AssertDecl>,
    pub span: Span,
    /// Outer doc comment from immediately-preceding `///` lines.
    pub doc: Option<String>,           // NEW
    /// Inner doc comment from `//!` lines immediately after the opening
    /// keyword and name. Distinct from `doc` so harvesters can tell
    /// "from the outside" from "from the inside" prose apart.
    pub inner_doc: Option<String>,     // NEW
}
```

Constructs that don't use `ConstructCommon` today (lighter AST nodes like
`StructDecl`, `EnumDecl`, `PackageDecl`) gain the same two `Option<String>`
fields directly.

`PortDecl`, `RegDecl`, `WireDecl`, `LetBinding`, `InstDecl`, `ResourceDecl`
also gain `doc: Option<String>` so members of a body can carry their own
prose.

Memory cost: two `Option<String>` per AST node carrying docs ≈ 32 bytes
overhead when empty. Negligible.

---

## 4. Attachment rules — exact

Precedence from outermost to innermost:

1. **Frontmatter:** if the file starts with `//!`, scan that contiguous
   block. The first `//! ---` line opens a frontmatter section; the next
   `//! ---` closes it. Stored on `SourceFile.frontmatter`.

2. **File-level inner doc:** all `//!` lines at the top of the file
   (including the frontmatter block) → `SourceFile.inner_doc` as raw text
   with `//! ` prefixes stripped. Frontmatter is *included* in
   `inner_doc` for fidelity.

3. **Construct-level outer doc:** any `///` lines immediately preceding a
   construct's opening keyword (with no intervening item-level syntax)
   attach to that construct's `doc` field.

4. **Construct-level inner doc:** `//!` lines that appear after a
   construct's opening keyword + name and before any other body item
   attach to the construct's `inner_doc` field.

5. **Member-level outer doc:** `///` lines immediately preceding a
   `port`/`reg`/`wire`/`let`/`inst`/`resource` declaration attach to that
   member's `doc` field.

Conflicts:
- `///` followed by `//!` followed by a construct: parse error
  ("conflicting outer + inner doc comments — pick one").
- `///` with no following construct: warning, comment dropped.
- `//!` outside the legal positions (file-top, post-keyword): parse error.

---

## 5. Examples

**Minimal — outer doc on a single construct:**
```arch
/// Saturating up-counter — wraps to MAX, never overflows.
counter SatCtr
  kind saturate;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Async, Low>;
  port inc: in Bool;
  port max: in UInt<8>;
  port value: out UInt<8>;
end counter SatCtr
```

**File-level frontmatter + inner doc + per-construct outer doc:**
```arch
//! ---
//! spec_md: doc/specs/dma_engine.md
//! tags: [dma, axi]
//! refs: ["AXI4 §A3.3.1"]
//! ---
//!
//! Multi-channel DMA engine. See spec_md for the channel state diagram
//! and the AXI4 protocol footprint.

domain SysDomain
  freq_mhz: 100
end domain SysDomain

/// Per-channel beat counter — counts AXI transfers and asserts at_max
/// when the channel completes.
counter BeatCounter
  kind wrap;
  ...
end counter BeatCounter
```

**Inner doc inside a construct body:**
```arch
arbiter AxiWrArb
  //! Round-robin policy chosen because all 4 channels are equal-priority;
  //! see ticket FOO-1234 for the QoS-aware variant proposed for v2.

  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter AxiWrArb
```

**Member-level outer doc:**
```arch
module FifoStage
  /// Backpressure signal — high while the upstream stage must stall.
  port stall_o: out Bool;
  /// Held value — only valid when `stall_o == 0` on the same cycle.
  reg payload_r: UInt<32> reset rst => 0;
  ...
end module FifoStage
```

---

## 6. MCP integration

The ARCH MCP server gains a directive in the `arch_design` tool's prompt
template that instructs agents to:

1. **Receive** the user's design spec text via a tool input
   (`design_spec` field, free-form markdown).
2. **Insert** it into the generated `.arch` file as:
   - A frontmatter block at the top with structured fields (`spec_md` if
     a separate markdown file is to be created; `tags` derived from the
     spec's headings or keywords; `refs` for any citations the spec
     mentions).
   - A file-level `//!` inner doc comment with a one-paragraph summary.
   - A `///` outer doc comment for each construct describing its role
     in the overall design.
3. **Preserve** these comments verbatim across edits — agents must not
   strip or rewrite them when modifying unrelated parts of the file.

Concretely, the MCP tool definition includes a system-prompt fragment
along the lines of:

> When generating ARCH code from a design spec, you must:
>
> 1. Open the file with a `//! ---` frontmatter block listing
>    `spec_md`, `tags`, and `refs` derived from the spec.
> 2. Follow the frontmatter with a 1–3 sentence file-level inner-doc
>    summary as `//!` lines.
> 3. Place a `///` outer-doc block above each top-level construct
>    (module, fsm, arbiter, counter, fifo, ram, etc.) capturing the
>    construct's role in the design.
> 4. When editing an existing file, preserve all `///`, `//!`, and
>    frontmatter content unless the user explicitly asks to change it.

The agent — not the MCP server — is responsible for sourcing the design
spec from the user (typically the conversation context). MCP just
formats it. This keeps the MCP server stateless and the agent in charge
of provenance.

---

## 7. Out of scope (V1)

- **Doc rendering / `arch doc` command.** Generating HTML/Markdown from
  doc-comment content is a separate feature. V1 only stores the strings.
- **Doctests.** Rust's `cargo test` extracts code blocks marked
  ```` ```rust ```` from doc comments and runs them. Useful but expensive.
  Defer until the harvesting layer is in place.
- **YAML parsing in the compiler.** V1 stores frontmatter as raw text.
  Downstream tooling parses it.
- **Markdown rendering inside the editor / IDE.** A separate IDE plugin
  concern.
- **Diagnostic citations** (e.g. "see refs[0] in the file's frontmatter").
  Possible future use of the structured fields by the compiler itself.
- **RAG harvester / `arch advise --feature`.** The whole point of this
  surface, but a separate work item that consumes the V1 AST.

---

## 8. Migration

- All fields are `Option<String>` defaulting to `None`. Existing code
  parses unchanged.
- Existing `// regular comments` continue to work and are preserved by
  the lexer's comment-extraction path (used by SV codegen).
- The grammar additions are *purely additive*. No keyword reservations.
- `////` (4+ slashes) and `//!!` (which doesn't currently exist) remain
  regular comments, giving an escape hatch.

---

## 9. Implementation phases (informational, not part of this spec)

V1 ships in two PRs:

- **PR-doc-1: Lexer + parser surface.** Recognize `///` and `//!`,
  extract frontmatter, attach to AST. Add the new AST fields.
- **PR-doc-2: MCP integration.** Update the ARCH MCP server's prompt
  template with the directive in §6.

Subsequent work (separate plans):
- RAG harvesting that consumes the new AST fields.
- `arch doc` HTML/Markdown renderer.
- IDE / LSP hover support.
