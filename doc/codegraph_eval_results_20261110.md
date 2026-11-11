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
The deterministic runner and grader are in `tools/codegraph_eval.py`; nine
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
