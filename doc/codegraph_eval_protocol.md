# Unguided ARCH code-graph evaluation

## Direct retrieval gate

`tools/codegraph_direct_eval.py` runs in cargo-test CI against the built
compiler. It indexes `examples/`, `stdlib/`, and `tests/` as separate graph
roots, builds the developer graph, and queries four frozen discovery prompts
without launching Codex. Each required path and symbol must appear near the
top of a 100-token-budget response; its reported line must still contain that
symbol in the current source. The script prints index time, query time, output
length, and ranks as JSON and fails on a coverage or bound regression. It does
not estimate provider tokens or test whether an agent chooses the graph.
An opt-in `--include-source` mode appends a short line excerpt from the
freshness-checked source and, for `use` or instance nodes, the indexed target
declaration. The direct gate also checks this mode at 200 tokens against the
frozen discovery prompts and the separate
`doc/codegraph_eval_cross_file_tasks.json` relationship cases.
The cross-file user relationship gate uses 100 tokens; the owner excerpt
checks use 200 tokens to retain the AST/parser/backend triple.

The direct gate is deliberately cheaper than the live agent evaluation below.
Run the latter only after a retrieval change passes the direct gate, including
known-file controls and answer quality before claiming token savings.

## Selective-routing hypothesis

The next routing comparison should test exact-symbol imports separately from
uncertain instance targets and compiler impact questions. A named package
declaration that one bounded `rg -n -g '*.arch'` can locate should take the
direct-search path; ambiguous matches or multi-target relationships should
take one focused graph query. Preserve known-file direct-read controls and
graph fallback for incomplete results. The deterministic gate checks retrieval
quality, not agent tool selection or provider tokens. Only a separately
approved paired live run can establish whether this policy reduces tokens
without lowering correctness.
Before that run, record the installed ARCH skill digest and align its routing
text with the reviewed repository skill, or explicitly isolate repository
instructions with `--ignore-user-config` in both arms. An older installed skill
can contradict the new routing rule and confound the comparison.

## Agent selection and tokens

`tools/codegraph_eval.py` compares ordinary-search-only attempts with the
current default ARCH workflow. Neither arm receives a graph command or index
path in the task prompt. The default arm may choose graph retrieval, direct
file reads, or search. The search-only arm receives a graph prohibition and
an instruction to use ordinary repository search. Both arms see the same
repository and session-local ARCH MCP server; the installed Codex skill is
available unless `--ignore-user-config`
is selected. Both arms are equally told to make bounded retrieval calls and
finish the task, so a planning message alone cannot masquerade as an answer.
This measures actual graph selection as well as token use.

The runner uses `--approve-for-me` with a session-local read-only sandbox for
lookup tasks. Without that approval mode, noninteractive Codex rejects ARCH
MCP calls before the server runs. Edit tasks use an automatically reviewed
workspace-write sandbox inside disposable clones. Failed MCP calls are counted
separately from attempted graph calls.

The frozen task set is `doc/codegraph_eval_tasks.json`: user and compiler
discovery, known-file controls, coverage misses, and two comment-only edits.
Edits run in temporary local clones. The grader checks changed paths, added
comments, and a compiler command, then discards those clones. Read-only runs
cannot edit the checkout. Each attempt starts an ephemeral Codex session.

```sh
python3 tools/codegraph_eval.py run \
  --repo . \
  --arch-bin target/debug/arch \
  --mcp-python /path/to/mcp/.venv/bin/python3 \
  --model gpt-6-luna --repeats 3 --seed 7 \
  --output /tmp/arch-codegraph-eval
python3 tools/codegraph_eval.py analyze /tmp/arch-codegraph-eval/records.jsonl
```

The runner refuses a dirty repository or an existing records file. It builds
fresh user and developer indexes and records build time separately from agent
latency. It sets `ARCH_STDLIB_PATH` to the benchmark checkout's `stdlib/` for
indexing and agent attempts, so a binary built in another worktree cannot
silently pull dependencies from that build checkout. `metadata.json` pins the
repository SHA, CLI/compiler versions, model,
seed, task-file digest, and selected task IDs. JSONL traces, stderr, edit diffs,
validation notes, and `summary.json` are retained. The per-attempt `graph_calls` field measures
actual graph CLI or MCP use. A correct default answer with zero graph calls is
*not* evidence of graph savings. Known-file controls should normally avoid
graph calls. Read-only answers with no tool call are unverified and do not
count as successes, even if the final text matches the rubric. Empty or stale
graph results must fall back to source search.
For MCP graph calls, transport completion alone is not success: the response
must begin with `[OK]`; an `[ERROR]` response counts as a failed call.
Direct reads of `.archgraph` or `.archdevgraph` also count as graph use.
Location attempts time out after 90 seconds by default; edit attempts get at
least 180 seconds. Timed-out subprocess groups are terminated and scored as
failures, never as token savings.

The deterministic harness tests run in CI without model credentials. Live
provider-token trials are opt-in because they require an authenticated Codex
installation and can consume substantial tokens. Compare provider-reported
input plus output tokens, uncached tokens, answer/edit success, graph selection,
tool calls, latency, and index overhead. Report per-task values; do not treat
shorter but incorrect attempts as savings. Human review is still required for
alternative correct answers and edit quality beyond the deterministic rubric.
The summary distinguishes the default-vs-search delta from savings on pairs
where the default arm actually used the graph. If no such pair exists, the
graph-saving metric is null.

For a controlled transport comparison, select `--condition graph_cli
--condition graph_mcp`. Both guided arms request one graph lookup with a
100-token response budget, but only the transport differs. Use
`--graph-token-budget 200 --include-source` for the answer-ready experiment;
compare it with the same 200-token budget without excerpts. The trace records
memory, graph, search, source-read, and other tool call counts and output
characters, plus non-graph calls after the first graph call. These are
retrieval-work proxies, not provider-token attribution per tool; provider
usage remains available only for the whole agent attempt.
