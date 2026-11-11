# Unguided ARCH code-graph evaluation: first live result

## Frozen run

Repository `f71697ae6e54be1024dc78e6aad1d7a692aa7e55`, task manifest
SHA-256 `c0351fda8436941f58c63738c8fffc3a568f318faebb4b6b80e79a3ccaf0f8f5`.
Codex CLI `0.159.2`, `gpt-6-luna` at low reasoning, seed 53. Four discovery
tasks (two user-code, two compiler-owner), two repeats, randomized arm order.
Selected IDs: `user_fifo_lookup`, `user_bus_relationship`,
`compiler_arbiter_owners`, and `compiler_pipe_reg_owners`.
The `arch` binary was built from `e7010156`, which differs from the benchmark
HEAD only in harness code and documentation, not compiler graph implementation.
This result predates the PR branch's rebase onto `767d3f6a`; the two newer
upstream commits changed pipeline emission and module logging, not graph
retrieval. Re-run the live eval before treating the result as a current-main
performance claim.
The installed Codex configuration was loaded. A session-local ARCH MCP server
override avoided a stale local script path; an explicit preflight call to
`arch_graph_query("TxQueue")` completed successfully with automatic approval
and a read-only sandbox. The run gave neither arm an exact graph command.

| Measure | Search-only | Default |
| --- | ---: | ---: |
| Correct, tool-backed answers | 8/8 | 7/8 |
| Graph calls | 0 | **0** |
| Provider-reported tokens, all attempts | 799,056 | 858,147 |
| Agent time, all attempts | 127.6 s | 134.9 s |

The one incorrect default answer attributed standalone `pipe_reg` reset
emission to `emit_pipeline` rather than `emit_pipe_reg`.

Among the seven pairs where both answers were correct, search-only used
615,649 provider-reported tokens (123,105 uncached) and default used 708,273
(139,441 uncached), a **15.0% total-token regression** and **13.3% uncached
regression** in this small sample. This is *not* a causal graph-cost estimate:
the default arm never selected the graph. The known-file, coverage-miss, and
edit tasks had one earlier pilot pass, but its MCP calls were blocked by the
noninteractive CLI approval policy; those attempts are excluded from this
MCP-available comparison.

The user graph took 35.9 s to build and the developer graph 1.4 s. Those setup
costs are outside agent time and token totals. The raw `metadata.json`,
`records.jsonl`, `summary.json`, and per-attempt JSONL traces are retained in
the local benchmark artifact directory
`arch-codegraph-unguided-eval-20261110-mcp-approved` beside this checkout.
The deterministic runner and grader are in `tools/codegraph_eval.py`; ten
offline harness tests run in CI.

## Interpretation

The prior guided pilot showed that compact graph retrieval *can* save tokens
when an exact graph command is supplied. This run does not reproduce that
benefit under default tool choice, even though the MCP query was available.
The next experiment should change only graph-selection guidance (for example,
surface selective graph-first discovery in repository agent instructions),
then rerun the same frozen tasks and include the known-file controls. Do not
make graph calls unconditional: known files should remain direct reads, and
unmapped or empty graph results should fall back to ordinary search.

## Exploratory guidance follow-up

Three local, unmerged `AGENTS.md` variants were tested with the same frozen
manifest hash, `gpt-6-luna` at low reasoning, seed 53, and two repeats. These
are separate repository snapshots, not an A/B test of only one text change;
provider output varied across repetitions. The live runs were not CI jobs.

| Variant | Scope | Default accuracy | Search-only accuracy | Default graph use | Paired total-token change |
| --- | --- | ---: | ---: | ---: | ---: |
| Broad graph-first (`9679a6ea`) | 4 discovery tasks | 8/8 | 8/8 | 5/8 attempts | +81.6% (8 pairs) |
| Exact-symbol search preferred (`eba300d9`) | 4 discovery tasks | 6/8 | 8/8 | 1/8 attempts | +24.7% (6 correct pairs) |
| Compiler ownership graph required (`f4bdd254`) | 2 compiler tasks | 4/4 | 3/4 | 3/4 attempts | +81.8% (3 correct pairs) |

For the broad variant, default used 1,383,120 total / 231,760 uncached tokens
versus search-only's 761,662 total / 204,606 uncached tokens. Three of its
eight graph MCP calls returned `[ERROR (exit 1)]` for unsupported role names
(`sv_codegen`, `codegen`, or uppercase `AST`), then the agent retried without
roles. All five graph-using attempts eventually had an `[OK]` result. The evaluator now counts
the three errors as failures rather than successful graph calls. The refined
variant reduced token use but lost two compiler-owner answers: one omitted
`emit_pipe_reg`, and one confused a resource-policy parser with the arbiter
declaration parser. In the final compiler-only variant, one search-only
attempt said no repository-reading tool was available; it is excluded from
the three correct pairs.

None of these variants establishes token savings from autonomous graph use.
The broad variant increased uncached tokens by 13.3% as well as total tokens;
the refined variant's lower cost coincided with lower accuracy. Keep these
guidance changes unmerged. The next graph optimization should target costly
follow-on retrieval after a successful graph query, then pass the same frozen
correctness and token checks plus the known-file controls before changing the
default workflow. Raw traces and summaries are retained in the local
`arch-codegraph-*-eval-20261110-approved` artifact directories beside the
checkout.
