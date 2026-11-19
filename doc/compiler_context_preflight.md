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

## Optional Evidence-Only Response

`tools/compiler_response.py` lets a host separate a bounded source-location
answer from a general coding session. It does not invoke Codex, change global
settings, or remove instructions or permissions from normal editing sessions.

```python
from tools.compiler_context import retrieve_compiler_context
from tools.compiler_response import prepare_response, verify_response

question = "Locate the counter AST, parser and backend owners."
context = retrieve_compiler_context(root, root / "target/debug/arch", question)
request = prepare_response(root, context, question)
if request["status"] == "ready":
    # The caller sends request["prompt"] with request["output_schema"] using
    # its response-only client profile, then records the actual tool events.
    # verdict = verify_response(root, context, answer, used_tools=used_tools)
    pass
# If preparation or verification requests retrieval, use the caller's usual
# targeted-source workflow. Include failed response and fallback costs.
```

Requests contain only the question, citation contract and verified source rows,
not raw retrieval logs, hashes or benchmark answer keys. No ownership names are
inferred by the helper. Recheck occurs both before the request and on response
verification, with separately reported validation I/O. Payload and answer are
each bounded to 4096 UTF-8 bytes; at most four 1-MiB source files are read.

`verify_response` rejects tools used during the response stage, stale sources,
null or malformed citations, duplicate JSON keys, unobserved rows, symbol/quote
mismatches and graph-role swaps. `needs_retrieval` is an explicit handoff, not an
automatic retry or an absence conclusion. Callers must pass the actual tool-use
flag; feature toggles alone are not a security boundary. Keep normal safety and
repository instructions, record every model attempt, and reject unexpected tools.

Contexts from a source-search provider can use the same response interface if
they supply `status`, `payload` (`path:line:source` rows), and `source_hashes`.
Without explicit role evidence, validation proves only an observed lexical
declaration, not correct semantic ownership. Even graph role bindings are curated
metadata, not a semantic proof. Edits, ambiguous questions, missing evidence and
compile checks still require the ordinary compiler workflow.

This interface makes a reduced tool catalog possible; it does not itself prove
token savings. Compare identical questions/evidence under full and reduced
profiles, and compare graph with equally host-managed search before attributing
benefits to graph retrieval.

## Bounded Search Control And Recovery Budget

`tools/compiler_search.py` is a graph-independent, opt-in search provider for
compiler ownership questions. Use it for an equally host-managed comparison or
an explicitly chosen search workflow, not as a change to graph-first routing.
It reads no graph index, curated feature map, benchmark manifest, or answer key.

```python
from tools.compiler_search import retrieve_search_context, retrieval_budget

context = retrieve_search_context(root, question)
request = prepare_response(root, context, question)
budget = retrieval_budget(context, total_operations=6, reserve_operations=2)
# Any caller-managed recovery must stay within budget["remaining_operations"].
```

The same `retrieval_budget` accounting can describe an existing graph context.
It does not undo work, enforce future agent calls, or account for earlier
retrievals. When chaining providers, callers must subtract all prior operations
from the next provider's `total_operations`; a new provider does not reset a
shared budget. `reserve_preserved` is false for contexts that already overspent.

Search extracts a named construct phrase (for example, `ARCH's ... construct`)
or removes role/request vocabulary from the first sentence. Multiword names
generate joined, snake-case and initialism candidates without feature-specific
aliases. Unsupported phrasings may need caller refinement. Generic output
instructions such as `symbol` and `citation` do not become search terms.
One recursive, bounded `rg --no-config` scans Rust source for exact declaration
names. AST `*Decl`, parser `parse_*`, SystemVerilog `emit_*`, and type-checker
`check_*` candidates are grouped by the same normalized construct stem and
conventional source directories. Missing complete groups, multiple complete
groups or duplicate declarations within a selected role request recovery.
No truncation or partial result is treated as evidence of uniqueness.

With the default six-operation budget and two-operation reserve, search spends
at most one command plus three unique source reads. Failed attempts count too.
The helper checks the allowance before reading sources, then verifies every
literal row and records source SHA-256 hashes. Missing, ambiguous, changed,
unsafe or oversized evidence produces `needs_search` with no consumable payload.
No automatic search retry, model call, or fallback is performed.

`ready` means a consistent set of **lexical candidates**, not parsed Rust or
verified semantic owners. A declaration-shaped line in a block comment or raw
string can match; this limitation is explicitly regression-tested. Role
metadata is intentionally omitted, so `verify_response` only checks literal
declarations. Independent source/semantic grading is still required when
measuring answer quality. A naming-based search control is not an oracle.

Host operation counts are not filesystem I/O counts: `rg` scans source internally,
and freshness/response verification reads remain separately reported. Budget
reservation alone is not evidence that fallback will succeed or save tokens.

## Evidence Selection And Typed Recovery

The optional `tools/compiler_selection.py` interface avoids asking a model to
copy declaration quotes. `prepare_selection` exposes bounded source rows with
content-addressed IDs and a role-to-ID schema; `verify_selection` revalidates
the snapshot and renders exact citations from the selected rows. IDs are bound
to the policy, question, requested roles, source hashes, literal rows and source
role metadata. They are evidence identities, not authorization capabilities.
The caller must associate each response with its own host-held request/context.
Null, duplicate or unknown IDs, changed snapshots, graph role swaps, duplicate
JSON keys and tool-assisted replies request retrieval instead of an answer.

```python
from tools.compiler_selection import prepare_selection, verify_selection

request = prepare_selection(root, context, question)
# After a healthy, response-only model call using request["output_schema"]:
# verified = verify_selection(root, context, reply, query=question,
#                             used_tools=actual_tool_activity)
# Only verified["answer"] is a rendered citation answer.
```

No expected benchmark answer, model-supplied quote or invented source text is
used for rendering. Existing graph role bindings are retained. Unbound search
rows remain lexical evidence, not semantic ownership proofs. The source-only
request and later rendering each recheck source freshness; rendering also runs
the existing citation verifier. All validation reads, including failed checks,
are reported separately from retrieval operations. The legacy quote-response
interface and its policy remain available and unchanged.

`tools/compiler_recovery.py` provides one optional, host-owned recovery attempt:

```python
from tools.compiler_recovery import RecoverySession

# Use a trusted, absolute rg path. Include ALL earlier retrieval work in context.
recovery = RecoverySession(root, context, rg_binary=Path("/absolute/path/to/rg"))
request = recovery.prepare(question)
# After a healthy, tool-free reply containing only {"construct": "keyword"}:
# result = recovery.recover(reply, used_tools=actual_tool_activity)
# If ready, replace context with result["context"], prepare_selection, and
# make one new response-only call. Do not combine graph and search role metadata.
```

Only a lowercase construct identifier (maximum 64 characters) is accepted.
The typed name bypasses prose stopwords, so language keywords such as `function`
are not discarded as generic request vocabulary. The host owns root, roles,
executable, source paths, command options and remaining budget. Search is one
bounded argv invocation, without a shell; it cannot execute a model-provided
command. The executable's resolved absolute path and SHA-256 are recorded and
checked before retrieval, with separately reported integrity-read I/O. This
controls host retrieval, not arbitrary agent-shell PATHs or operating-system
TOCTOU races. The binary is a trusted host dependency, not a model input.

The session snapshots initial retrieval counters, charges failed searches, and
consumes its single recovery attempt even for invalid replies or failed work.
It never resets the six-operation total. A fresh three-file answer normally
requires four operations; it cannot fit after four prior operations. In that
case, search may spend one command but performs no over-budget source reads.
Two remaining operations are not a promise of successful recovery.

The host must reject unhealthy model stages before calling these helpers,
record every model/tool attempt and its cost, and stop on unknown usage rather
than retrying. The helpers themselves launch no models and cannot certify model
health or enforce the caller's entire workflow. They do not change defaults,
perform an automatic retry, or claim token savings. Fresh paired measurement
must retain all failures and distinguish consumed regression tasks from holdouts.

## Offline Regression Mode

```sh
python3 -m unittest discover -s tools -p test_compiler_context.py
python3 -m unittest discover -s tools -p test_compiler_response.py
python3 -m unittest discover -s tools -p test_compiler_search.py
python3 -m unittest discover -s tools -p test_compiler_selection.py
python3 -m unittest discover -s tools -p test_compiler_recovery.py
python3 tools/codegraph_direct_eval.py --compiler-context --arch-bin target/debug/arch
```

This mode exercises actual counter and register-file ownership queries,
known-file bypass, missing coverage, payload exhaustion, a reference masquerading
as a declaration, and a stale-index mutation in a disposable index. It has independent expected owner names,
never launches an agent, and reports separately from the existing graph-ranking
audit. Both modes run in CI.
