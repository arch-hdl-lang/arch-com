---
name: arch-programming
description: Write, modify, debug, and verify ARCH HDL code. Use when working with `.arch` files, ARCH language constructs such as module/fsm/fifo/ram/arbiter/pipeline/bus/thread/testbench, ARCH compiler errors, SystemVerilog generation from ARCH, ARCH simulation, formal checks, or ARCH coding guidance.
---

# Arch Programming

Use this skill to produce correct ARCH HDL and to iterate through the ARCH compiler workflow. Prefer the local repository's current syntax and validation behavior over memory.

## Workflow

1. Choose retrieval by expected search cost. Read a named file directly for an in-file fact. For an exact ARCH import or declaration symbol, try one targeted `rg -n -g '*.arch'` in likely source roots first; use graph only if that result is ambiguous or broad. Use graph-first for uncertain instance targets, callers, impact, or compiler ownership where one graph query can replace several searches:
   - For ARCH code, use `arch_graph_context(task=..., token_budget=200, include_source=True)` with `source_paths` for a new or stale project index. For an external workspace, pass an `index` under that workspace too. Use `arch_graph_callers()` or `arch_graph_impact()` for a named symbol after indexing.
   - For compiler ownership, use `arch_dev_graph_query(query=..., token_budget=200, include_source=True)` with relevant `roles`; it refreshes the reviewed developer index when stale. A quoted ARCH syntax token does not identify its Rust AST, parser, emitter, or fixture owner: make one developer-graph query before broad Rust search. From an `arch-com` checkout, the equivalent CLI command is `./target/debug/arch graph dev-query "<feature>" --root . --index .archdevgraph --token-budget 200 --include-source` after building the index.
   - If the question names an exact file and asks for a line, function, or other fact inside it, run one targeted `rg -n` or source read of that file before answering; do not query either graph or answer from memory. Never use the user `.archgraph` for Rust compiler source. Do not run CLI `--help` when the command above suffices. A fresh, unambiguous source-backed result is enough for a cross-file location answer; read files if details are missing or code will be changed. Fall back to ordinary search when graph results are incomplete.
2. Read `references/Arch_AI_Reference_Card.md` first for compact syntax and common pitfalls.
3. For grammar, semantics, or a construct not covered by the card, read the relevant section of `references/ARCH_HDL_Specification.md`.
4. For project setup, CLI behavior, simulation, and formal commands, read `references/README.md`.
5. When Python/cocotb-style simulation is involved, read `references/arch_sim_cocotb.md`.
6. If ARCH MCP tools are available, use them before hand-writing unfamiliar syntax:
   - Call `get_construct_syntax()` for each construct to be used.
   - Write and type-check with `write_and_check()`.
   - Build and lint generated SystemVerilog with `arch_build_and_lint()`.
   - On compiler errors, call `arch_advise()` with the error keywords before attempting a fix.
7. If `tool_search` is available but ARCH MCP tools are not loaded, search for `arch-hdl`. If MCP tools are still unavailable, read `references/mcp_README.md` for setup or fall back to the local CLI.
8. If MCP tools are not available, validate with the local CLI:

```sh
arch check <file.arch>
arch build <file.arch>
arch sim <file.arch> --tb <tb.cpp>
arch formal <file.arch>
```

Use explicit Cargo commands from an `arch-com` checkout when `arch` is not on `PATH`:

```sh
cargo run -- check <file.arch>
cargo run -- build <file.arch>
cargo run -- sim <file.arch> --tb <tb.cpp>
cargo run -- formal <file.arch>
```

## Search Map

Use targeted search before loading large reference sections:

```sh
rg -n "^##|^###|module|fsm|fifo|ram|arbiter|pipeline|bus|pipe_reg|inst connections|let has two forms" references/Arch_AI_Reference_Card.md
rg -n "^\\*\\*|^##|^###|<construct-or-error-keyword>" references/ARCH_HDL_Specification.md
rg -n "sim|pybind|cocotb|debug|formal|learn|advise|coverage|wave" references/README.md references/arch_sim_cocotb.md
```

For syntax mistakes, search the compact card first. For semantic disputes or compiler behavior, search the full specification. For CLI and simulation behavior, search the README or cocotb guide.

Bundled reference files are snapshots copied from the same `arch-com` commit that contains this skill. Some links inside those snapshots remain repo-relative; use `references/SNAPSHOT.md` for provenance and packaging notes.

## Coding Rules

Use first-class ARCH constructs whenever they match the intent: `fsm` for state machines, `fifo` for queues, `ram` for memories, `arbiter` for arbitration, `pipeline` for staged dataflow, and `bus` for reusable interfaces. Use `module` for logic that does not fit a stronger construct.

Preserve design provenance in generated `.arch` files. Add file front matter with `//! ---` metadata when a user supplied a design spec, and put a `///` role comment above every top-level construct.

Prefer visible timing in port signatures. Use `port name: out pipe_reg<T, N> reset rst => value;` for registered outputs and assign `name@N <= expr` in `seq` blocks. Use plain `out T` driven by `comb` or `let` when the output must respond in the same cycle.

Use explicit width handling. Prefer wrapping arithmetic (`+%`, `-%`, `*%`) for deliberate modular arithmetic; otherwise use `.trunc<N>()`, `.zext<N>()`, `.sext<N>()`, `.resize<N>()`, slices, `signed()`, or `unsigned()` as required by the type checker. Bit-selecting or slicing a cast/conversion/arithmetic result — `unsigned(x)[i]`, `x.sext<N>()[i]`, `(a + b)[7:0]`, chained slices like `a[7:4][1:0]` — needs no workaround: the compiler auto-hoists any non-atomic base to a named temp before selecting from it (arch#650/#813).

Avoid common syntax errors: instantiation connections use `port <- signal` for inputs and `port -> wire` for outputs; chained conditionals use `elsif`; bit slices use `expr[hi:lo]`; bus fields use dot notation; `SysDomain` is built in.

## Debugging

Treat compiler diagnostics as the next source of truth. Read the exact error, consult `arch_advise` or `arch advise` when available, make the smallest syntax or type correction, then rerun the same check.

For simulation failures, rerun with targeted debug flags before changing RTL:

```sh
arch sim --debug <file.arch> --tb <tb.cpp>
arch sim --debug+fsm --depth 2 <file.arch> --tb <tb.cpp>
arch sim --pybind --test <test.py> <file.arch>
```

For uninitialized state issues, use `--check-uninit`, `--inputs-start-uninit`, or `--check-uninit-ram`.

## References

- `references/SNAPSHOT.md`: provenance, freshness, license, and repo-relative link notes for bundled references.
- `references/Arch_AI_Reference_Card.md`: compact syntax, timing, types, constructs, and common errors.
- `references/ARCH_HDL_Specification.md`: full language specification.
- `references/COMPILER_STATUS.md`: compiler feature status and changelog.
- `references/README.md`: CLI, simulation, formal, and repository overview.
- `references/arch_sim_cocotb.md`: Python/cocotb-style simulation surface and differences.
- `references/plan_arch_doc_comments.md`: `///`, `//!`, and front-matter provenance comment design.
- `references/mcp_README.md` and `references/mcp_instructions.md`: ARCH MCP server behavior and tool workflow.
- `references/LICENSE`: license text for copied ARCH reference material.
