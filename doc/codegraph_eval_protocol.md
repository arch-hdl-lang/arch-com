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

The current `source_read_policy=exact-file-or-literal-glob-v2` also recognizes
a limited directory-search form: `rg -n --glob filename PATTERN DIR` (or `-g`
or `--glob=filename`). There must be exactly one literal basename glob equal
to the target file's basename, one in-workspace directory containing that
target, and a numbered match row explicitly resolving to the exact target.
Wildcard/negative/path globs, multiple globs or roots, hidden filenames,
context-only rows, replacement/preprocessor/count modes, and unrelated output
do not qualify. Existing direct-file reads retain their prior behavior.
This uses the trusted command trace, not arbitrary shell-output claims, and
adds no extra source-content reads. Answer/source-pin verification still runs
separately; recognizing a read never proves the answer is correct.

If the model exits normally with usage but lacks that read, the runner permits
one fresh ephemeral recovery invocation with the original task and a reminder
to perform a simple bounded read. It supplies neither the previous answer nor
the answer key. It does not retry infrastructure failures, missing usage,
forbidden graph use in search-only attempts, or incorrect answers that already
have recognized source evidence. A second
unverified response fails closed; there is no retry loop. Discovery/edit tasks
are unchanged. The original policy has live results below; subsequent
trace-recognition changes need fresh confirmation and do not guarantee savings.

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

## Opt-in source-bound answers

The source-prefetch live run exposed a different failure: a successful direct
read included the correct function, but the model said no such function existed.
The read-only recovery gate intentionally does not detect that. The new
`--verify-known-file-answer` policy requires a structured answer and independently
binds it to source. It applies identically to positive known-file controls in
`no_graph`, `default`, and `source_prefetch`; discovery/edit tasks stay unchanged.
Absence-verdict controls and guided graph arms are rejected for this policy.

The model must return only a JSON object with `symbol`, `path`, `line`, and
`quote`. The verifier requires the exact named path, a positive integer line,
and a verbatim prefix of the source line after surrounding whitespace is
removed from the source. The full line is also accepted. The quote must contain
the complete symbol as a whole identifier and cannot end inside an identifier
in the actual source. For example, a signature may omit its trailing opening
brace or body; `fn load` cannot witness `fn load_cache(...)`. Changed characters
or internal whitespace, arbitrary substrings, and invented signatures are not
accepted. This verifies a symbol and location, not every detail of a signature
that the quote omits. It rejects duplicate/extra fields, prose
or null answers, mismatched locations/quotes, external or metadata paths, and
missing, binary, invalid UTF-8, or over-1-MiB source files. Answers are bounded
to 8192 UTF-8 bytes. Source lines are LF-delimited. The verifier reads source
locally; it neither injects source into the model nor accepts an answer key.

A successful exit with usage but without both source-read evidence and a
source-bound answer permits one fresh recovery call. This flag enables that
same two-call maximum even without `--recover-known-file`; combining the flags
does not permit a third call. A format/evidence reminder replaces the read-only
reminder when the first attempt already read source. No previous answer, gold
symbol, expected line, or rubric result is supplied. Infrastructure failures,
missing usage, and forbidden graph calls retain the existing stop rules. A
second unsupported answer fails closed. Fresh prefetch evidence can still
satisfy the read requirement without a model tool call.

The independent answer rubric remains necessary. It grades only the verified
`symbol path:line`, not incidental symbols in the quote. A citation to an
existing but semantically wrong function does **not** trigger recovery based
on the gold answer; it fails the rubric. Literal source binding is not proof
of declaration syntax, semantic relevance, complete absence, or that the
model's own read included the cited line. This is a bounded evidence contract,
not a general semantic verifier or a production default.

The versioned policy is `source-line-prefix-v2`. Each stage records
`answer_evidence` with its policy, result, reason, source-read count,
bytes read, and source hash when available. Records add
`known_file_answer_verified` and `answer_evidence_seconds`; summaries add
`answer_verified_controls`, `answer_evidence_reads`, and evidence-check time.
These host reads are separate from prefetch reads and model tool calls. All
model stages, including failed answers, retain their full token/latency cost.
Successful evidence also records `quote_match` as `full_line` or `line_prefix`.
Metadata records `answer_evidence_policy`, the flag, and the maximum model-call
count. Verification-enabled runs predating this version field used the exact
whole-line rule; in particular, the 2026-11-14 run at `56a642b2` remains frozen
at 6/6 versus 5/6 despite offline acceptance of its omitted-brace answer under
the new policy. An offline policy replay is not a new model trial. Legacy records,
prompts with the flag disabled, frozen task rubrics, and old results are not
rewritten. No live effectiveness or savings claim follows from mocked retries;
a new, explicitly approved paired run is still required.

## Whole-file prefetch mode

The focused recovery live check restored 4/4 default correctness after two
retries, but used 25.75% more total and 36.35% more uncached tokens than
search-only. A separate opt-in experiment now tests whether supplying source
before the first model invocation avoids that recovery cost. It is not enabled
by default. This section describes the whole-file mode; the later excerpt
experiment, frozen live result, and untested v2 repairs are recorded below.

Without `--prefetch-excerpts`, `source_prefetch` reads the exact `source_path` supplied by a
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
read does not trigger read-only recovery. The separately enabled source-bound
answer policy above also checks the final answer contract. Large-file fallbacks
use the existing bounded recovery policy when enabled. This does not guarantee
the model will use the source correctly or avoid redundant retrieval.

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

### Bounded source windows for oversized files

`--prefetch-excerpts` is an additional opt-in experiment. It requires
`--verify-known-file-answer` and the paired `no_graph` / `source_prefetch`
conditions; it cannot be combined with global `--prefetch-known-file`.
Search-only prompts and small-file source payloads are unchanged. Production
compiler/MCP defaults are unchanged. The v1 live result and subsequent v2
repairs are described below; v2 has not been live-tested.

When a complete file exceeds the payload budget, treatment can supply up to
four windows with five whole source lines each as the base. The v2
comment-tail rule below can extend these windows within the same byte budget.
Selection uses only source,
path, byte budget, and the task question, never the answer rubric or expected
symbol. It removes the named path and generic question words, then ranks
literal lexical overlap: case-sensitive term count first, case-folded count
second, inverse line-frequency weight for ties, then source order. Case is
useful for distinguishing a type such as `Wire` from a variable `wire`.
Only the first 64 ranked centers are considered. Each accepted window adds
two neighboring lines on either side when present, deduplicates overlaps,
and must fit the serialized UTF-8 JSON budget. Long lines are never clipped.

This is lexical retrieval, not parsing or proof of declaration ownership.
Comments, strings, imports, and duplicate names can match; the model must
inspect the context, and ambiguity or missing evidence requires a direct
read. The selector can miss paraphrases and identifiers not present in the
question. Its ranking was developed against the existing controls and
synthetic cases, so held-out tasks are needed before making general claims.
The v2 source-bound verifier is not an AST declaration checker: a comment or
string containing the expected name can be a valid literal witness. A
symbol/path-only answer rubric can therefore accept a misleading citation.
Future quality confirmation should use frozen declaration-line rubrics or an
independent semantic check; do not treat literal-witness validation as that
check or retroactively change the earlier manifests/results.

Read size is bounded at 1 MiB plus one overflow-detection byte; excerpt
selection additionally caps source lines at 32,768, question length at 8,192
characters, and distinct question terms at 64. Non-text, oversized, unmatched,
or unrenderable input supplies no excerpt and retains direct-read fallback.
Workspace and metadata-path exclusions apply as in whole-file mode.

The payload declares `complete=false`, total source lines, numbered literal
source rows, and the full-source hash. The prompt explicitly says omitted
content is unknown, not evidence of absence. A partial payload waives the
model tool-read requirement only when the source-bound answer verifier passes
and the citation line is among the supplied rows. A valid citation to an
omitted line still requires a recognized successful direct read. Otherwise
the same single recovery invocation is available, independent of the answer
key. Both invocations' costs are counted. Changes anywhere in the source,
including outside selected windows, fail the final freshness check.
The evidence-read hash must also match the supplied snapshot and the quote
must match the serialized row, preventing a changed-then-restored file from
validating an excerpt quote the model never received.

Run metadata pins `prefetch_policy=query-windows-v2`; ordinary runs record
`complete-file-v1` when prefetch is enabled, otherwise null; metadata separately
identifies `source_prefetch` as the excerpt-enabled condition. Receipts identify selected lines, completeness, read bytes,
and policy. Partial-source stages record `prefetch_answer_covered`; summaries
add `excerpt_prefetched_controls` and `prefetch_read_bytes` only for records
with the new policy, plus `answer_evidence_read_bytes` and their sum,
`host_read_bytes`. `prefetched_source_verified` means the supplied payload
is fresh; it does not by itself prove the answer is covered or correct.
Host reads include full-source rereads for freshness, separate from the
answer-verification reads. Historical records and summaries are unchanged.
Path guards reject stable escapes but are not a security sandbox against a
concurrent adversarial filesystem replacement; run in the controlled checkout.

For a new approved experiment, add `--prefetch-excerpts
--verify-known-file-answer` to the paired command above and use a fresh output
directory. The six-task, one-repeat schedule remains 12 trials, at most 24
model invocations. Payload size reductions are not measured model-token
savings; quality parity and provider-reported total/uncached usage remain
necessary. Extra trials or a new live run require fresh approval.

### Frozen held out confirmation

`doc/codegraph_eval_held_out_source_tasks.json` defines six new controls, three
ARCH and three compiler tasks. Two files fit whole-source prefetch; four need
excerpts. Their prompts and targets were selected before running the selector
on them. The retrieval policy remains the implementation at `607c17e7`.
Do not change its ranking or replace a missed task after seeing coverage.
"Held out" here means different questions/files from the prior tracked
known-file manifests and recorded known-file answers, not unseen model
training data or a representative random sample of repositories.

| Task | Frozen declaration | Source bytes | Payload bytes | Declaration supplied |
| --- | --- | ---: | ---: | --- |
| ROM | `examples/rom_lut.arch:2` (`RomLut`) | 358 | 630 | Yes, whole file |
| Block decoder | `examples/lz4_block_decoder.arch:44` (`Lz4BlockDecoder`) | 10,087 | 1,433 | Yes, excerpt |
| Mesh | `examples/mesh_noc_4x4/mesh4x4.arch:275` (`Mesh4x4`) | 16,659 | 764 | Yes, excerpt |
| Clock-gate emitter | `src/codegen/clkgate.rs:7` (`emit_clkgate`) | 2,670 | 3,449 | Yes, whole file |
| Format-tag lookup | `src/fp_format.rs:248` (`by_tag`) | 22,339 | 1,090 | No, excerpt misses it |
| Async FIFO detection | `src/resolve.rs:1258` (`detect_async_fifo`) | 51,826 | 1,066 | Yes, excerpt |

These are local payload measurements, not model results. The lookup miss is
retained to exercise explicit fallback. Small-file payloads retain the existing
complete-file encoding. The final manifest SHA-256 is
`55d46fc805d51e48e24ff927da84ffc9caf29c6537d4a8b47e7994f8f11e3859`.

Every task has an `expected_citation` with an exact case-sensitive symbol,
path, and positive line number. The source-bound verifier's normalized citation
must equal those fields, not merely contain matching words. This prevents a
filename from accidentally satisfying the symbol rubric (for example,
`Mesh4x4` in `mesh4x4.arch`) and rejects comments, references, and end markers
at other lines. Expected citations are independently specified and not passed
to the prompt, selector, or retry decision. The ordinary `answer_any` rubric
is unchanged for historical manifests; mixing the two rubric forms is rejected.
The grading-format correction was made after a local negative test exposed
the filename match, without changing any question, target, or selection rule.

Each source has a frozen `source_sha256`. Startup checks bounded file reads
before indexing or model launch, rejects mismatches and disallowed paths, and
records the validated pins, bytes, and check time. Pinned/exact-citation tasks
require `--verify-known-file-answer`. Each trial also compares the final
evidence-read hash against its pin and records `source_pin_verified`; a mismatch
fails without an answer-key-guided retry. These checks supplement, not replace,
the supplied-payload freshness check. Metadata pins `exact-citation-v1` for
these tasks and `answer-rubric-v1` for legacy tasks. Historical artifacts must
not be regraded with the new manifest.

The frozen pre-run plan (completed at `7d923bc9`) specified
**36 trials, at most 72 model invocations**:
six tasks, three repeats, two interleaved conditions, seed 83. Both arms use
`source-line-prefix-v2`, exact citation grading, and at most one recovery per
trial; only `source_prefetch` enables query windows. The common budget is
8,192 bytes, timeout 180 seconds, GPT-6 Luna with low reasoning effort, and
the local ARCH MCP. Approval must explicitly cover repository-derived source
and context export for this run. No live calls are authorized by local tests.

```sh
python3 tools/codegraph_eval.py run \
  --repo . --arch-bin target/debug/arch \
  --mcp-python /private/tmp/arch-codegraph-known-file-live-venv/bin/python3 \
  --tasks doc/codegraph_eval_held_out_source_tasks.json \
  --condition no_graph --condition source_prefetch \
  --prefetch-excerpts --verify-known-file-answer --recover-known-file \
  --prefetch-byte-budget 8192 --model gpt-6-luna \
  --repeats 3 --seed 83 --timeout 180 --ignore-user-config \
  --output /tmp/arch-held-out-source-live-NEW_RUN_ID
```

Before launch, rebuild the compiler at the clean benchmark commit and use a
new output directory. Freeze and record the manifest, source pins, repository,
binary, CLI, MCP, and instruction versions. Do not tune prompts, thresholds,
or ranking during the run, replace failed cases, or spend unused call capacity
on extra trials. Preserve completed artifacts on interruption rather than
restarting completed trials.

Report all scheduled trials, including failures and all recovery costs.
A clean savings result requires **18/18 correct in each arm**, valid usage,
approved methods, and lower full-arm total and uncached tokens. Otherwise
report the quality or cost tradeoff, not an overall savings win. Audit traces
for answer-key/benchmark-artifact access as well as prohibited graph use;
oracle access invalidates the claimed comparison even if the answer is correct.
Report per-task and per-repeat totals, both whole-file/excerpt strata, model
tool calls, recoveries, host reads/bytes/time, and shared setup separately.
The existing paired-success-only summary is secondary, never a replacement
for full-arm totals. Three repeats of six tasks are not 18 independent task
samples; describe cache variation and avoid generalizing a single saving rate.

The frozen `7d923bc9` live confirmation completed 36 trials in 40 model calls:
18/18 final passes in both arms, with 42.05% fewer total and 4.21% fewer
uncached tokens for the combined treatment. However, the excerpt stratum used
2.98% more uncached tokens, and the full-arm uncached advantage depended on
an unnecessary baseline retry. Removing only that retry diagnostically gives
39.17% fewer total but 1.11% more uncached tokens; this is not a rescore.
The three format-tag treatment trials all needed recovery. These cases are
now development regressions, not untouched held-out tasks for further tuning.
Reproducing the frozen policy requires its original commit, not the current
CLI and selector.

### Post confirmation repairs

The v2 read recognizer accepts the successful literal-basename directory
search that caused the ROM baseline's unnecessary retry, while requiring
explicit numbered target-path output and retaining the other method gates.
Offline replay recognizes that first trace now; the original failed read gate,
retry, token totals, and final grade remain frozen in the live artifact.

`query-windows-v2` retains v1 ranking, four-window and 64-candidate limits,
source hashes, and the 8,192-byte default budget. When every line from a match
center through the base window's right edge starts with `//` after indentation,
the window may extend by at most eight whole lines. It follows the consecutive
comment tail and, if reached within that limit, includes the first following
non-comment line plus up to two more lines. It does not scan across code or a
blank line already inside the base window. The maximum is 13 lines per window,
52 before deduplication across four windows. If the extension cannot fit the
rendered byte budget, the original base window is tried; no line is clipped.

This is bounded lexical context completion, not an AST declaration detector.
Long comment blocks, attributes, block comments, and multiline strings can
still leave required evidence outside the payload. The partial-source warning,
literal-row coverage, exact citation, source pin, and freshness checks remain
required; an omitted declaration still requires a real source read.

Local regression measurements on the six formerly held-out cases show the
expected declaration included in 6/6 payloads rather than 5/6. The format-tag
payload grows from 1,090 to 1,948 bytes; async-FIFO detection grows from 1,066
to 1,215 bytes; the other four payload sizes are unchanged. These are payload
coverage results, not agent correctness or token savings. Synthetic tests
cover comment tails, EOF, CRLF, intervening code, oversized lines, budget
fallback, unrelated glob output, and grading independence. New metadata pins
both policies; historical summary calculations remain unchanged.

No new live run has been made for these repairs. A confirmation on these six
tasks would be a regression run, not fresh held-out evidence; either that run
or a new held-out experiment requires separate approval and must retain all
attempt costs and quality failures.

### Source-backed discovery roles

New discovery manifests may opt into `role-citations-v2` instead of the legacy
substring rubric. `citation_roles` lists public answer slots (for example
`ast`, `parser`, `backend`, or paired instance/target slots).
`expected_citations` maps each slot to an exact symbol/path/line triple;
`source_pins` maps every referenced file to its SHA-256. The loader rejects
missing, extra, duplicate or malformed roles and unpinned paths. Source pins
are checked before setup or any model call and again through answer witnesses.

The answer is a JSON object keyed by the public roles, each containing exactly
`symbol`, `path`, `line`, and `quote`. Duplicate JSON keys, wrong role assignments,
wrong source locations, stale source, missing roles and fabricated quotes fail.
Each quote follows the existing bounded literal-prefix source policy and must
also be visible in successful retrieval output: a recognized direct-file read,
a numbered directory/multi-file `rg` source row, or a graph CLI/MCP row containing
its exact path and line. Directory search witnesses are discovery-only; the
known-file read policy is unchanged. Chained graph shell
commands, failed commands, MCP errors, and unrelated tool output do not count.
This is a conservative trace recognizer plus curated answer oracle, not an AST
relationship proof or adversarial command-execution sandbox.

Expected citations and source pins never enter the model prompt or retry
feedback. Only public roles, the task question and the common evidence contract
do. Discovery has one model invocation and **no answer-key-driven recovery**;
known-file recovery behavior is unchanged. The record includes the evidence
policy, per-role witnesses, host read count/bytes/time and the verified verdict.
Historical records and summaries retain their stored verdicts and policies.

Version 2 applies to new executions of role-citation tasks. Frozen v1 records
must be replayed with their pinned harness, not regraded in place. Compared to
v1, discovery quotes may include leading ASCII spaces or tabs; these alone are
removed before literal-prefix verification. Interior whitespace, identifier
boundaries, exact source locations and source hashes remain checked. The
known-file quote and source-read policies are unchanged.

Discovery `rg` witnesses accept allowlisted options before or after positional
operands, including globs, repeated `-e` patterns and numeric context/count
limits. Directory/multi-file results still require a numbered filename prefix
and the quote on the same matching row. Output rewriting, preprocessing,
file/count-only modes, unknown flags, chained commands and pipelines remain
unrecognized. Prompts now explicitly require single retrieval commands.
The three-retrieval-call ceiling is enforced for role-citation tasks; exceeding
it fails `valid_method` even when citations are correct. Failed attempts retain
all usage and have no discovery retry.
The method gate also rejects compound or opaque command forms anywhere in the
attempt, even if a later clean retrieval provides valid witnesses. Supported
forms are allowlisted `rg`, `cat`, bounded `sed -n` print commands, and the graph
CLI/MCP queries. Scripts, preprocessing and arbitrary MCP tools are not allowed.
One multi-file search still counts as one retrieval command; this is not a count
of files read or an adversarial sandbox against replaced executables.

Role-citation runs also pass `--output-schema` to `codex exec`. The generated
schema contains only public role names and the common citation field types,
never expected symbols, paths, lines or hashes. A missing role's value may be
null, which remains an unverified answer. Each invocation retains its schema
file and records its SHA-256. There is no post-hoc JSON repair or fallback retry
if structured output is unsupported or malformed. Local command-construction
tests do not establish live model support or a token saving.

`doc/codegraph_eval_cross_file_regression_tasks.json` is a development regression
set derived from the November 14 cross-file run, **not a fresh held-out set**.
It resolves the AES declaration-selection ambiguity in the public question rather than
silently changing the original answer key. Copy the manifest outside the model
checkout and freeze it before a separately approved live run. The frozen
95f0fc0c experiment and its original pass/fail results remain unchanged.
Naming the intended AES declaration files makes that task easier; its costs
are not directly comparable to the original ambiguous question. The specified
files are source-location lookup targets, not a combined compilation set.

Keep new experiment manifests outside the model checkout. Both arms must avoid
benchmark oracles/artifacts, memory and Git metadata; do not read the curated
feature-map file directly. The same restrictions now also appear in exact-citation
known-file prompts. Prompt restrictions do not prove isolation: audit command
outputs as well as commands for leakage before claiming savings.
Freeze tasks before live execution, retain preflight misses, disclose local
development exposure, and do not tune graph retrieval on the confirmation set.
Report all-arm quality and costs, graph adoption, fallback and known-file routing
separately. A source-citation benchmark does not establish code-edit correctness.
