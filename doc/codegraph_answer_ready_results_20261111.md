# Answer-ready code graph evaluation (November 11, 2026)

## Scope and method

The experiment used `gpt-6-luna` at low reasoning with a clean ARCH checkout,
fresh user/developer indexes, a pinned compiler binary, session-local MCP, and
provider-reported usage from `codex exec --json`. Search-only and guided CLI
arms used the same task prompts and completion instruction. The graph arm was
explicitly told to make a bounded graph query; these runs do **not** measure
autonomous graph selection. Index build time (~30 seconds user, ~1.3 seconds
developer) is separate from agent time and tokens. Tool-category counts and
output characters are retrieval-work proxies, not per-tool provider tokens.
The expanded three-task run used commit `bf3a1b9811847e841ff00fab2191272d73a34587`,
task-manifest SHA-256 `a908728c244ccf082aded952e19b7453a739c80029996a97c4e19dc2e24e853d`,
Codex CLI `0.159.2`, seed 55, and two repeats with the installed user
configuration. The earlier two-task run used commit
`d52050d74f784cde0333bd7aec50e1b3b6c1786a` and manifest SHA-256
`7f5821998886d69ab3f25209af174136afa24e3f87b1bdeb304945fcfee848fa`.

The opt-in `--include-source` response appends a short line from the
freshness-checked source. For `use` and instance hits, it also reports the
indexed target declaration path and line. The excerpt format is opt-in; compact
format remains the default.
The deterministic gate passes 12/12 frozen compact, excerpt, and cross-file
cases. Both named instance targets fit within a 100-token output bound.

## Paired findings

| Trial | Search/compact total | Graph/excerpt total | Search/compact uncached | Graph/excerpt uncached | Correctness |
| --- | ---: | ---: | ---: | ---: | --- |
| Two user cross-file tasks, three repeats, 100-token excerpt vs search | 486,002 | 345,709 | 77,042 | 69,101 | 6/6 both, human-reviewed |
| Three user cross-file tasks, two repeats, 100-token excerpt vs search | 466,971 | 317,161 | 77,339 | 84,713 | 6/6 both |
| Instance-target subset of preceding row (DMA and Nic400) | 361,556 | 211,627 | 67,924 | 57,643 | 4/4 both |

The three-task run saved 32.1% total tokens but **increased uncached tokens
9.5%**. The instance-target subset saved 41.5% total and 15.1% uncached, with
one successful graph call and no follow-up retrieval in each graph attempt.
The simple package-import task was roughly break-even in total tokens and
regressed in uncached tokens. Thus the evidence favors opt-in graph retrieval
for named cross-file instance targets, not a general graph-first default.

A one-repeat compiler ownership trial used 52,928 total / 18,880 uncached
tokens with the compact 100-token graph, versus 118,295 / 25,495 for search.
Both answers were semantically correct: the search answer named a different
shipped fixture with reset-qualified `pipe_reg` ports, which the original
exact-path rubric rejected. This is too small for a compiler-token claim.
Two known-file controls passed in both default and search-only arms with zero
graph calls in the unguided checkout.

## Mechanism and limits

At a 200-token budget, three guided tasks compared compact graph replies with
source excerpts. Compact used 245,538 total / 69,666 uncached tokens and made
three post-graph retrieval calls. Excerpts used 217,114 / 60,570 and made two
post-graph calls. Both user tasks improved, but the compiler task regressed:
its compact owner triple was already answer-ready, while excerpts prompted
extra search and reading. Keep excerpts opt-in and prefer compact developer
queries.

For two earlier guided discovery tasks, pinned CLI retrieval used 132,567
total / 38,231 uncached tokens versus MCP's 260,267 / 53,419. Both arms
answered correctly with one successful graph call per task. This one-repeat
transport result is suggestive, not a causal estimate of MCP overhead.

An isolated `AGENTS.md` rule limited graph-first use to named instance targets.
In its one-repeat smoke test, default used 240,045 total tokens versus
164,020 search-only. One attempt never tried the graph; the other invoked an
older `arch` from PATH, failed, and fell back to search. No successful graph
call occurred. That guidance remains unmerged, and no default-activation or
general token-saving claim follows from the guided results.

The pilot artifacts and JSONL traces are under `/private/tmp/arch-codegraph-*`
with run names `transport-pinned-20261111`, `cross-cli-compact-20261111`,
`cross-cli-source-20261111`, `cross-source100-v-search-20261111`,
`cross-source100-v-search-repeat2-20261111`, `cross-three-user-20261111`,
`cross-compiler-v-search-20261111`, `cross-known-controls-20261111`, and
`instance-default-smoke-20261111`. Each has `metadata.json`, `records.jsonl`,
per-attempt traces, and `summary.json`. The frozen task manifests are
`doc/codegraph_eval_tasks.json` and `doc/codegraph_eval_cross_file_tasks.json`.
