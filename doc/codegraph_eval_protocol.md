# Unguided ARCH code-graph evaluation

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
latency. `metadata.json` pins the repository SHA, CLI/compiler versions, model,
seed, task-file digest, and selected task IDs. JSONL traces, stderr, edit diffs,
validation notes, and `summary.json` are retained. The per-attempt `graph_calls` field measures
actual graph CLI or MCP use. A correct default answer with zero graph calls is
*not* evidence of graph savings. Known-file controls should normally avoid
graph calls. Read-only answers with no tool call are unverified and do not
count as successes, even if the final text matches the rubric. Empty or stale
graph results must fall back to source search.
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
