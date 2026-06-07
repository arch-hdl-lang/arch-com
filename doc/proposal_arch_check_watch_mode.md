# `arch check --watch` — Incremental Watch Mode with `.archi`-Based Dependency Tracking

**Status:** Proposal  
**Date:** 2026-06-07  
**Applies to:** arch-com compiler

---

## Problem

Every `arch check` invocation re-parses and re-type-checks every file listed on
the command line from scratch. For a 39-module design like E203 or a large
attention-tile SoC with 20+ ARCH files, a single-character typo in one leaf
module triggers a full multi-file pass. Developers using ARCH in an
LLM-assisted workflow (the primary use case) iterate quickly; the round-trip
penalty accumulates.

The `.archi` interface file system already encodes the inter-module dependency
graph — when module A instantiates B, A declares `inst b: B` and the compiler
auto-discovers `B.archi`. This is exactly the information needed for
**incremental re-checking**: if only B changed, only B and its reverse
dependents (modules that `inst` B) need to be re-checked.

## Proposed Feature

```
arch check --watch [--archi-dir PATH] FILE [FILE ...]
```

Runs an initial `arch check` pass, then **stays alive** and watches the listed
`.arch` files (and their inferred dependencies) for changes using filesystem
events. On any change:

1. Re-parse only the changed file(s).
2. Walk the reverse-dependency graph (who `inst`s this module?) and re-check
   upward through the hierarchy.
3. Print the same error output as `arch check` — same miette-formatted
   diagnostics, same exit-code semantics per check cycle.
4. Print a summary line after each cycle: `[watch] OK (3 files checked, 36 skipped)` or `[watch] 1 error — waiting for changes`.

The user interrupts with `Ctrl-C`; the process exits cleanly.

### Dependency graph construction

`arch check --watch` builds the graph during the initial pass:

- **Forward edge**: module A `inst`s B → A depends on B.
- **Source of truth**: the `inst` declarations in each parsed file; confirmed
  by checking which `.archi` file is loaded for each inst site.
- **Invalidation rule**: when file X changes, invalidate X plus every module
  that (transitively) depends on X.

This mirrors how `tsc --watch` tracks TypeScript project references, or how
`cargo` tracks crate dependency graphs for incremental compilation.

### Integration with `.archi` files

When `arch build` or `arch check` runs on multi-file inputs it already emits
`.archi` beside each `.sv`. `--watch` can read pre-existing `.archi` files to
bootstrap the graph before the first full pass, enabling faster startup on
already-built projects.

`--archi-dir PATH` tells the watcher where to find pre-built interface files
(default: same directory as each source file, then `ARCH_LIB_PATH`).

## Why This Matters

**Iteration speed for LLM-assisted design.** ARCH is explicitly designed for
LLM-generated hardware. A typical workflow is: prompt → generated `.arch` →
`arch check` error → fix prompt → repeat. Each check invocation today requires
the user (or script) to re-invoke the compiler. Watch mode cuts the
re-invocation overhead and lets the user keep focus in the editor or prompt
window.

**Scale.** The E203 SoC has 39 modules. The FPT26 attention tile (from
open issues #437, #462, #472) has 10+ ARCH files. On a 20-module design where
the average check takes 400 ms, touching a leaf module today forces a
400 ms wait. With watch mode + incremental invalidation, the same touch
re-checks only the leaf and its 3 direct parents: ~60 ms.

**No new language surface.** This is pure tooling — zero changes to the ARCH
language grammar, type system, or SV output. It adds one CLI flag.

## Implementation Sketch

### Crate dependencies

- `notify` (v6, cross-platform inotify/kqueue/FSEvents wrapper) — already
  used in the Rust ecosystem by tools like `cargo-watch`. Add to `Cargo.toml`
  under an optional feature `watch` or directly.

### New source file

`src/watch.rs` (~300 LOC):

```rust
pub struct WatchSession {
    // Parsed + elaborated state per file, keyed by canonical path.
    cache: HashMap<PathBuf, CachedFile>,
    // Reverse dependency graph: module name → set of files that inst it.
    rdeps: HashMap<String, HashSet<PathBuf>>,
}

struct CachedFile {
    mtime: SystemTime,
    content_hash: u64,   // xxhash of source bytes
    ast: Arc<SourceFile>, // re-used if nothing changed
    errors: Vec<CompileError>,
}
```

On each filesystem event:

1. Compare `mtime` + `content_hash`; skip if unchanged (handles spurious events).
2. Re-parse changed file → new `ast`.
3. Re-run type-check on changed file and all files in `rdeps[module_name]`
   transitively. Use cached `ast`s for unchanged files.
4. Print updated diagnostics; clear terminal or prefix with a clear marker.

### Entry point in `main.rs`

Add `Command::Check` a `--watch` flag:

```rust
Check {
    files: Vec<PathBuf>,
    /// Re-check on change; stays alive until Ctrl-C.
    #[arg(long)]
    watch: bool,
}
```

When `watch: true`, delegate to `watch::run(files)` instead of the existing
single-pass `run_check(files)`.

### Parse-result caching

The existing `parser::parse()` returns an owned `SourceFile`. For watch mode,
wrap it in `Arc<SourceFile>` and store in `CachedFile`. The type-checker
takes a `&SourceFile`, so the `Arc::deref()` path works with no other
changes to `typecheck.rs`.

### Approximate line count

| File | LOC |
|------|-----|
| `src/watch.rs` (new) | ~300 |
| `src/main.rs` (add `--watch` arm) | ~30 |
| `Cargo.toml` (add `notify`) | ~3 |
| **Total** | **~335** |

No changes to `parser.rs`, `typecheck.rs`, `elaborate.rs`, or any codegen.

## Interaction With Existing Features

- **Learning capture** (`--check-uninit`, error→fix store): watch mode should
  still invoke the learning capture hook on each check cycle — same behavior as
  a normal `arch check`.
- **`--depth N` hierarchy control**: not applicable; watch mode only runs
  `arch check`, not `arch sim`.
- **`.archi` discovery (`ARCH_LIB_PATH`)**: respected by the dependency graph
  builder; external library modules are treated as leaves (no re-check needed
  unless their `.archi` file changes on disk).
- **Multi-file inputs**: `arch check --watch a.arch b.arch c.arch` watches all
  three and their mutual dependencies.

## Out of Scope (v1)

- Watching `--tb` C++ testbench files for `arch sim --watch`.
- Automatic re-running of `arch build` or `arch sim` on change (separate
  commands; a shell script can chain them).
- Structural diff output (showing *what* changed between two check cycles).
- LSP integration (`arch check --lsp` via stdio). Natural follow-on but a
  separate proposal.

## Acceptance Criteria

1. `arch check --watch Foo.arch Bar.arch` runs, prints initial diagnostics,
   and stays alive.
2. Editing `Foo.arch` triggers a re-check of `Foo.arch` and any module that
   `inst`s it; unrelated modules are skipped.
3. The output format matches `arch check` (same miette diagnostics).
4. `Ctrl-C` exits cleanly (no dangling watcher threads).
5. A `tests/watch/` integration test exercises the invalidation logic using
   a temp directory: write a file → start watcher → modify file → assert new
   diagnostics appear within 500 ms.

## Alternatives Considered

**Shell wrapper (`cargo-watch`-style):** A shell `while true; do arch check; sleep 0.5; done` re-runs the full pass on every tick. This is simple but (a) adds 500 ms latency regardless of whether anything changed, (b) always re-checks all files, and (c) requires a separate tool. The integrated incremental watcher is faster and dependency-aware.

**LSP server:** A full Language Server Protocol server would subsume watch mode
and add hover, go-to-definition, completion. It's the right long-term target but
is a 2000–5000 LOC project. Watch mode delivers 80% of the iteration-speed
benefit at ~10% of the implementation cost, and can serve as the foundation
for an LSP server later.

**Parallel type-check:** Running checks in parallel across independent modules
is orthogonal and complementary. Watch mode's incremental invalidation already
reduces the working set; parallelism would speed up the re-check of the
invalidated subset.
