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
text with the reviewed repository skill. `--ignore-user-config` disables
config.toml; it does not establish isolation from installed skills or plugins.
An older installed skill can contradict the new routing rule and confound the
comparison.

## Agent selection and tokens

`tools/codegraph_eval.py` compares ordinary-search-only attempts with the
current default ARCH workflow. Neither arm receives a graph command or index
path in the task prompt. The default arm may choose graph retrieval, direct
file reads, or search. The search-only arm receives a graph prohibition and
an instruction to use ordinary repository search. Both arms see the same
repository and session-local ARCH MCP server; record the installed skill state
separately from the `--ignore-user-config` setting. Both arms are equally told
to make bounded retrieval calls and finish the task, so a planning message
alone cannot masquerade as an answer.
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

## Opt-in known-file recovery

The 2026-11-13 live check still produced two incorrect default known-file
answers without a source read, despite explicit prompting. To test an
orchestration remedy, add `--recover-known-file` to a separately approved run.
This is a new workflow policy, not a graph retrieval improvement or a change to
the frozen scoring rubric. It applies identically to `no_graph`, `default`,
and the direct-source `source_prefetch` condition; guided graph conditions are
rejected when this flag is enabled.

Known-file tasks provide a `source_path` field naming the already-known file,
not a gold symbol or line number. Both checked-in manifests add only this
metadata to their existing controls; their digests therefore change, while
task prompts and answer keys stay unchanged. The runner rejects missing files
and paths outside the checkout before launching a model.

The policy checks the completed trace for a successful, nonempty direct read
of that exact file. It recognizes simple `rg` searches, numeric `sed -n`
line ranges, and `cat`, including single shell wrappers. Failed commands,
filename/count-only searches, unrelated paths, graph calls, command chains,
redirections, and unrecognized shell forms do not satisfy the gate. This
conservative trace classifier is not an adversarial security boundary and
does not prove that the read excerpt supports the answer. The answer rubric
remains independently required.

If the model exits normally with usage but lacks that read, the runner permits
one fresh ephemeral recovery invocation with the original task and a reminder
to perform a simple bounded read. It supplies neither the previous answer nor
the answer key. It does not retry infrastructure failures, missing usage,
forbidden graph use in search-only attempts, or incorrect answers that already
have recognized source evidence. A second
unverified response fails closed; there is no retry loop. Discovery/edit tasks
are unchanged. This policy has deterministic/mock coverage only until a new
live test is approved; it does not guarantee recovery or token savings.

The original trace retains its filename; the recovery gets a
`-recovery1.jsonl` trace and matching stderr file. `records.jsonl` retains one
row per scheduled task/repeat/condition, with `stages`, `model_attempts`,
`recovery_attempts`, and `known_file_source_verified` on controlled rows.
Tokens, cached tokens, tool calls, and latency include both stages, including
the rejected first response. Only the final response is graded; a forbidden
graph call in either stage still invalidates the search-only arm. Summary arms
report recovery counts when present. Legacy records and summary output are
unchanged when recovery metadata is absent.

`metadata.json` records the policy and maximum model invocations. The seven-task,
two-repeat paired suite has 28 scheduled rows and **up to 36 model invocations**
with recovery (eight known-file rows may each retry once). Obtain approval for
that maximum, not merely 28 calls. Never present recovered successes without
their initial token cost, or silently rescore earlier evidence under this policy.

## First-pass source prefetch experiment

The focused recovery live check restored 4/4 default correctness after two
retries, but used 25.75% more total and 36.35% more uncached tokens than
search-only. A separate opt-in experiment now tests whether supplying source
before the first model invocation avoids that recovery cost. It is not enabled
by default and has no new live token result yet.

The `source_prefetch` condition reads the exact `source_path` supplied by a
known-file task, before starting Codex. It provides the **complete file** as
JSON containing numbered lines, a SHA-256 digest, and a trailing-newline flag.
It does not inspect the task prompt, rubric, symbol names, or graph to select
lines. For identical file bytes, path, and budget, the payload is independent
of the question.
Comments and strings are explicitly labeled as source data, not instructions.

`--prefetch-byte-budget` defaults to 8192 bytes and accepts 1 through 65536.
Both the raw read and rendered JSON are bounded. A file that does not fit is
not partially supplied: the prompt retains the ordinary direct-read workflow.
Missing, unreadable, empty, or non-UTF-8 files also fall back. Sources outside
the workspace and graph-index/Git metadata paths are never prefetched. Existing
recovery-mode startup validation still rejects missing or external source
paths when `--recover-known-file` is enabled.

After the model finishes, the runner rechecks a supplied file's hash. Stale
prefetched evidence fails the trial, even if the answer matches the rubric.
A fresh complete source payload can satisfy the retrieval requirement without
an agent tool call, but it cannot bypass the answer rubric, usage requirement,
process status, or graph prohibition. An incorrect answer after a supplied
read does not trigger recovery. Large-file fallbacks use the existing bounded
recovery policy when enabled. This does not guarantee the model will use the
source correctly or avoid redundant retrieval.

Use a dedicated paired comparison, with both arms subject to the same recovery
policy and graph prohibition:

```sh
python3 tools/codegraph_eval.py run \
  --repo . --arch-bin target/debug/arch --mcp-python /path/to/mcp/.venv/bin/python3 \
  --tasks doc/codegraph_eval_known_file_tasks.json \
  --condition no_graph --condition source_prefetch \
  --recover-known-file --prefetch-byte-budget 8192 \
  --model gpt-6-luna --repeats 1 --seed 55 --ignore-user-config \
  --output /tmp/arch-known-file-prefetch-eval
```

This requires separate approval for **up to 24 model invocations**: six tasks,
two conditions, one repeat, and at most one recovery per scheduled trial.
The randomized schedule interleaves both conditions. The six new controls use
different files from the previous live known-file controls: four small ARCH/Rust files
and two larger fallback files. They are budget-coverage cases, not a statistically
representative sample; additional repeats and held-out tasks remain necessary.
No selection rules are tuned from their answer keys.

For a feasibility run applying prefetch to both existing arms, use
`--prefetch-known-file` instead of the `source_prefetch` condition. That flag
must not be combined with the dedicated condition, to prevent accidentally
prefetching the comparison baseline. A default-versus-search comparison with
the flag on both arms does not measure the effect of prefetch itself.

Every prefetch attempt writes a `-prefetch.json` receipt, including the exact
payload sent (or fallback status), source hash when available, and freshness
result. Records keep the receipt path, `prefetched_source_verified`, source
read counts, payload bytes, and host read/check time. `tool_calls` still counts
only model-issued calls; host reads are reported separately, including the
freshness check and failed/oversized reads. Provider usage already includes
the injected source, and recovery costs remain included. Do not subtract an
estimated source-token cost from provider totals.

Summary arms include `host_source_reads`, `prefetch_payload_bytes`, and
`prefetch_seconds`; `source_prefetch_vs_search` reports only pairs with valid
answers in both arms. Raw arm totals retain failures. Direct-source prefetch
is not classified as graph use or graph savings. Historical records without
prefetch fields retain their previous analysis.
