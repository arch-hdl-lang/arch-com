> **Correction (2026-06-01):** The original version of this note proposed adding
> VCD waveform output to native `arch sim`. That was wrong — `arch sim --wave`
> is fully implemented (COMPILER_STATUS.md row 301, PR shipped, used in e203_soc_top
> tests). The original proposal has been replaced with a different idea below.

---

# Enhancement: `arch check --format json` — Machine-Readable Diagnostic Output

**Date:** 2026-06-01
**Status:** Proposal — not tracked as an issue or in any ideas doc
**Related:** `src/diagnostics.rs` (CompileError enum), `src/main.rs` (error
reporting), `doc/plan_arch_learning_system.md`

---

## Problem

`arch check` outputs rich, human-readable diagnostics via `miette` — colorized
spans, labels, source context. This is excellent for humans reading a terminal.
It is brittle for everything else:

- **LLM agents** working with ARCH (Claude, Codex, etc.) must parse ANSI-escaped
  text to extract the file, line, column, and error message. The current output
  has no stable schema.
- **IDE integrations** (VS Code Problem Matcher, JetBrains inspections) require
  either a custom regex per error type or a machine-readable format.
- **CI annotation** (GitHub Actions `::error file=X,line=Y::` syntax) requires
  post-processing the text output.
- **`arch advise` / learning system** records error→fix pairs from stderr
  (`src/learn.rs`). The pairs are reconstructed from the human-readable text,
  which means any future change to an error message string breaks the extraction.

The `CompileError` enum (`src/diagnostics.rs`) is already a rich structured type:
every variant carries a span, a message, and often a help string. The information
is there; it just has no serialization path other than miette's terminal renderer.

---

## Proposal: `arch check --format json`

Add a `--format` flag to `arch check` (and propagate to `arch build` / `arch sim`
where they also report diagnostics):

```sh
arch check MyModule.arch --format json
```

Output: one JSON object per diagnostic, one per line (newline-delimited JSON /
NDJSON — trivially streamable and `jq`-friendly):

```json
{"severity":"error","code":"TypeMismatch","file":"MyModule.arch","line":70,"col":42,"end_line":70,"end_col":55,"message":"type mismatch: expected SInt<32>, found SInt<38>","label":"here","help":null}
{"severity":"error","code":"TypeMismatch","file":"Bf16PvTileEngine.arch","line":70,"col":42,...}
```

Exit code remains 1 on any error (same as today). `--format human` is the
default (identical to current behavior).

### Schema

```json
{
  "severity": "error" | "warning" | "note",
  "code":     "<VariantName>",        // e.g. "TypeMismatch", "UndefinedName"
  "file":     "<path>",               // absolute or relative to CWD
  "line":     <number>,               // 1-based
  "col":      <number>,               // 1-based
  "end_line": <number | null>,
  "end_col":  <number | null>,
  "message":  "<string>",             // error description
  "label":    "<string | null>",      // span label text (e.g. "here", "first defined here")
  "help":     "<string | null>",      // #[diagnostic(help(...))] text if present
  "secondary": [                      // additional labeled spans (e.g. "first driver here")
    { "file": "...", "line": ..., "col": ..., "end_line": ..., "end_col": ..., "label": "..." }
  ]
}
```

The `code` field is the `CompileError` variant name — a stable string identifier
that can be referenced in suppression rules or `arch explain` lookups (see
"Future work" below).

### Why NDJSON?

- One diagnostic per line → `grep`-able, `jq`-pipeable, streamable.
- No need to buffer the entire output before parsing.
- GitHub Actions problem matchers expect one annotation per line.
- `arch check` already streams errors as it encounters them; NDJSON preserves
  this streaming property.

---

## Implementation

### 1. Add `--format` flag to CLI (`src/main.rs`)

```rust
#[arg(long, value_enum, default_value = "human")]
format: DiagnosticFormat,

#[derive(clap::ValueEnum, Clone, Default)]
enum DiagnosticFormat { #[default] Human, Json }
```

Thread `format` through to every `run_check` / `run_build` / `run_sim` call.

### 2. JSON serializer in `src/diagnostics.rs`

`CompileError` already derives `Debug`. Add a `to_json` method that walks the
enum variants and produces the schema above:

```rust
impl CompileError {
    pub fn to_json(&self, source_map: &SourceMap) -> String {
        // extract variant name as `code`
        // resolve Span → (file, line, col) via source_map
        // serialize to NDJSON with serde_json
    }
}
```

`SourceMap` is already threaded through the check pipeline to resolve spans for
miette. The JSON serializer reuses exactly the same span resolution.

Only new dependency: `serde` + `serde_json` (both are already in Cargo.toml for
the learning system: `src/learn.rs` writes JSON events).

### 3. Output routing in `src/main.rs`

Where errors are currently printed via `miette`'s `Report` handler:

```rust
// current
eprintln!("{:?}", miette::Report::new(err));

// new — branch on DiagnosticFormat
match format {
    DiagnosticFormat::Human => eprintln!("{:?}", miette::Report::new(err)),
    DiagnosticFormat::Json  => println!("{}", err.to_json(&source_map)),
}
```

In JSON mode, output goes to **stdout** (not stderr) so downstream tools can
capture it cleanly with `$(arch check --format json ...)`.

### 4. Warnings

The existing `WarningCollector` (dead-skid lint, operator-precedence warnings,
etc.) emits to stderr as plain text. In JSON mode, convert each warning to the
same schema with `"severity":"warning"`.

### Estimate

| Task | Lines |
|------|-------|
| `--format` CLI flag | ~20 |
| `CompileError::to_json` + span resolution | ~100 |
| `WarningCollector` JSON path | ~40 |
| Tests: JSON shape for TypeMismatch, UndefinedName, MultipleDrivers, warning | ~4 tests, ~60 LoC |
| **Total** | **~220 LoC** |

No new dependencies beyond `serde_json`, which is already in the dependency
tree via `src/learn.rs`.

---

## Use cases this unlocks

### 1. LLM-assisted development (immediate)

Claude currently parses `arch check` output as text. With JSON output:

```sh
ERRORS=$(arch check MyModule.arch --format json)
# Claude receives structured [{code, file, line, message}, ...] — no regex needed
```

The `learn.rs` error-capture loop can also be refactored to consume JSON directly
instead of parsing the miette text, making it robust to future error-message wording
changes.

### 2. VS Code problem matcher (no extension required)

A single `.vscode/tasks.json` entry with a `problemMatcher` regex on the NDJSON
output gives inline red squiggles in VS Code without building a full LSP server:

```json
{
  "problemMatcher": {
    "pattern": { "regexp": "^{\"severity\":\"error\",.*\"file\":\"([^\"]+)\",\"line\":([0-9]+),\"col\":([0-9]+),\"message\":\"([^\"]+)\"" }
  }
}
```

### 3. GitHub Actions inline annotations

```yaml
- run: arch check *.arch --format json | while IFS= read -r line; do
    file=$(echo "$line" | jq -r .file)
    line_no=$(echo "$line" | jq -r .line)
    msg=$(echo "$line" | jq -r .message)
    echo "::error file=$file,line=$line_no::$msg"
  done
```

### 4. Future: `arch explain <code>`

Once errors have stable `code` names, `arch explain TypeMismatch` can show a
full explanation with examples — similar to `rustc --explain E0308`. The `code`
field is the hook that makes this possible without a separate numbering scheme.

---

## What this does not do

- Does not add `arch explain` (separate follow-up; this proposal adds the
  prerequisite `code` field).
- Does not implement a full Language Server Protocol server.
- Does not change any language semantics or emitted SV.
- Does not change default behavior (`--format human` is the default).

---

## Rationale: why now

The `arch advise` / learning system already writes JSON events to
`~/.arch/learn/events.jsonl`. `serde_json` is in the dependency tree. The
`CompileError` enum is already structured. The only missing piece is a
serialization path from `CompileError` → stdout when `--format json` is
requested. The cost is ~220 LoC; the benefit compounds across every LLM session,
every IDE user, and every CI pipeline that runs `arch check`.

The dead-skid lint (PR #486) and multi-driver check (PR #470) both added new
warning/error classes. As the static-analysis surface grows, machine-readable
output becomes more valuable — not less.
