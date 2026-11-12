# Code graph follow-up benchmark (2026-11-12)

Clean PR #1137 head `51c07a2f88ee94e91cb913cdc1b689480bac74c9`;
`arch 0.74.2 (git 51c07a2f88ee)`, Codex CLI `0.159.2`, `gpt-6-luna`.
The frozen seven-task manifest is `doc/codegraph_eval_cross_file_tasks.json`
(SHA-256 `a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`).
Two repeats per task, schedule seed 55, three arms (`no_graph`, `default`,
`graph_cli`), and a 200-token guided graph response with source lines produced
42 attempts. Indexing took 30.717 s for user sources and 1.530 s for compiler
sources; these times are excluded from token counts. The run recorded no MCP
failure. Frozen records, metadata, and the analyzer summary are under
[`codegraph_followup_20261112`](codegraph_followup_20261112).
`records.jsonl` SHA-256:
`8f40236d110790708ae3ca5b696501af03f8d1011bb424254481c057b9dc040f`.

Reproduce the summary:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_followup_20261112/records.jsonl
```

All 24 user attempts passed the frozen answer rubric. For the six user
discovery pairs, excluding the known-file control:

| Arm | Correct | Total tokens | Uncached tokens | Graph calls |
| --- | ---: | ---: | ---: | ---: |
| Search-only | 6/6 | 458,680 | 125,752 | 0 |
| Default | 6/6 | 604,965 | 113,957 | 1 |
| Guided CLI | 6/6 | 322,134 | 85,718 | 6 |

Guided graph used 29.8% fewer total and 31.8% fewer uncached tokens than
search-only in these six correct pairs. Default used 31.9% more total tokens
than search-only and selected graph only once in six user discovery attempts.
That one graph-using Nic400 attempt succeeded with one graph call and no
follow-up tool calls, but used 136,768 total tokens versus 79,558 for its
search-only pair. Both guided Nic400 repeats succeeded in one graph call each,
using 108,036 total / 21,252 uncached tokens together versus 159,508 / 46,996
for search-only. The previous pilot already used one guided call per Nic400
repeat, so the new ranking fix is supported by the deterministic 200-token
audit, not by a measured drop in guided call count.

Guided graph on the user known-file control increased total tokens 52.1% and
uncached tokens 396.4% versus direct search; default avoided graph on both
known-file controls. Keep direct reads for known files.

The split compiler discovery tasks passed the frozen rubric in 4/4 guided,
1/4 search-only, and 1/4 default attempts. Only one compiler discovery pair
passed on both sides, so do not infer compiler-token savings from the aggregate.
The strict rubric requires one specific fixture and emitter symbol. Some
search/default answers give valid alternative fixtures or discuss the reset
parser or FSM emitter; other answers name a genuinely wrong emitter. Preserve
the frozen scores, but use semantic, source-backed grading in a future run.

This is a two-repeat pilot with strongly order-dependent cached-token shares.
It tests guided CLI, not `graph_mcp`, and does not establish a stable causal or
general provider-cost estimate. Keep graph for unknown ownership or cross-file
discovery, but do not claim token savings for default-agent behavior. The full
per-attempt traces are retained in the local benchmark artifact bundle; the
published records contain each answer, score, tool breakdown, and token count.
