# Compiler Context Preflight

`tools/compiler_context.py` is an opt-in host workflow step for compiler source
questions. It chooses retrieval before an agent is invoked, rather than relying
on the agent to select graph over search. It does not install an agent hook,
change normal MCP selection, or launch a model.

## Workflow Integration

Build the local compiler and developer index first:

```sh
cargo build
./target/debug/arch graph dev-index --root . --out .archdevgraph
python3 tools/compiler_context.py "Locate the counter AST, parser and backend owners."
```

Python hosts can import the API from the checkout's `tools` package:

```python
from pathlib import Path
from tools.compiler_context import prepare_compiler_prompt

root = Path("/path/to/arch-com")
prompt, audit = prepare_compiler_prompt(
    root, root / "target/debug/arch",
    "Locate the counter AST, parser and backend owners.",
)
# Send prompt to the agent using the host's existing model invocation.
# Persist audit separately; do not send raw command logs as source evidence.
```

For a fact in a known file, pass `source_path="src/ast.rs"` and, when known,
`start_line` and `line_limit`. This bypasses graph and index access entirely.
The caller selects known-file mode explicitly; prose is not heuristically
classified. Only repository-relative Rust source paths resolving inside the
checkout's `src/` tree are accepted. A bounded window is not a complete file or
evidence of absence.

Unknown ownership always executes one `dev-query` with the original question
unchanged, `--best-match`, source excerpts, and requested role filters (default:
AST, parser, backend). `--best-match` chooses the uniquely highest-ranked
feature and rejects equal-score ties. Ordinary `dev-query` behavior is unchanged.
Selected symbol positions are revalidated with the compiler's Rust parser;
a reference to a symbol is not accepted as its declaration location.
This is ranked discovery, not proof that the selected feature is semantically
the only possible answer. A query about several features may need separate
queries or targeted search.

The preflight rejects truncated output, multiple visible feature blocks,
multiple owners for one requested role, missing roles, unsupported rows,
unsafe paths, mismatched excerpts, and changed source. Graph positions are
checked against actual source, including the schema-v1 index fingerprint.
Delivered evidence carries SHA-256 source hashes; prompt preparation rechecks
them immediately before inclusion. Later consumers can call `check_context`
again after a delay. The result is a snapshot, not a lock on source files.
If the consumption check fails, the returned status becomes `needs_search`
and consumable evidence is cleared; original source hashes remain diagnostic.

## Result And Fallback

- `ready`: the requested roles have literal source witnesses, or the explicitly
  requested source window was read. It does not imply successful compilation,
  complete semantic coverage, or absence elsewhere.
- `needs_search`: no payload is supplied. `reason` explains the failure and
  `missing_roles` identifies unresolved role coverage when applicable. Prompt
  preparation asks for targeted source search, not a guessed answer.

Missing/stale indexes are never silently rebuilt; callers choose whether to
refresh the index or search. There is no automatic broad-search fallback,
mutation, compiler invocation beyond `dev-query`, or nested agent call.

## Bounds And Accounting

Input is capped at 8192 UTF-8 bytes; a graph command uses a 200-token byte budget
and a 20-second deadline. Captured stdout and stderr are independently capped at
8192 bytes. Source snapshots are limited to four files of at most 1 MiB each;
known-file windows contain at most 80 lines. Evidence payloads are capped at
4096 bytes by default (maximum 8192) without splitting source rows. Index files
are capped at 4 MiB each and may not be symlinks.

`commands`, `host_graph_calls`, source/index read counters, payload size and
the separate `consumption_check` preserve host work for evaluation. They are
not agent tool calls. Wrapper read counters do not include internal compiler
I/O: `dev-query` also validates all index manifest sources. Index construction,
freshness checks and failed retrieval costs should be reported separately.

No token savings or improved default-agent adoption is claimed by this change.
Fresh paired live evaluation is separate; older benchmark records and their
grading policies must remain frozen.

## Offline Regression Mode

```sh
python3 -m unittest discover -s tools -p test_compiler_context.py
python3 tools/codegraph_direct_eval.py --compiler-context --arch-bin target/debug/arch
```

This mode exercises actual counter and register-file ownership queries,
known-file bypass, missing coverage, payload exhaustion, a reference masquerading
as a declaration, and a stale-index mutation in a disposable index. It has independent expected owner names,
never launches an agent, and reports separately from the existing graph-ranking
audit. Both modes run in CI.
