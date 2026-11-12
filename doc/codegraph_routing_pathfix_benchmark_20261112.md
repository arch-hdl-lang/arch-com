# Agent routing path-fix benchmark (2026-11-12)

This is the approved rerun of the earlier
[routing benchmark](codegraph_routing_benchmark_20261112.md), after
`AGENTS.md` was changed to name the checkout's ARCH compiler instead of bare
`arch`. The source checkout was clean at
`eb7af551fb759a0e031fd628988d7c642a143001`, with matching compiler
`arch 0.74.2 (git eb7af551fb75)`, Codex CLI `0.159.2`, and `gpt-6-luna`.
The same frozen seven-task manifest had SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`.
Seed 55, two repeats, and search-only (`no_graph`) and unguided (`default`)
arms produced 28 attempts. User and compiler indexes took 31.330 s and 1.601 s
to prepare; index time is excluded from token counts.

The [metadata, per-attempt records, and analyzer summary](codegraph_routing_pathfix_20261112)
are frozen. `records.jsonl` SHA-256:
`129253f7c2f99f993971aace99a825dbaa2fc215ba0006bbd83eaa7299aea715`.
Reproduce the summary with:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_routing_pathfix_20261112/records.jsonl
```

| User discovery arm | Correct | Total tokens | Uncached tokens | Successful graph calls |
| --- | ---: | ---: | ---: | ---: |
| Search-only | 6/6 | 488,300 | 55,788 | 0 |
| Unguided default | 6/6 | 616,126 | 69,438 | 6 |

The path repair succeeded as a tool-routing intervention: all six default
user discovery attempts and two of four compiler discovery attempts made
successful graph calls (8/10 discovery attempts total). Neither user nor
compiler known-file controls used graph, and all eight known-file attempts
passed the frozen answer rubric. In the prior routing run, default made two
graph attempts, both failed because its login shell ran macOS `/usr/bin/arch`.

Successful graph selection did **not** save provider tokens here. On the six
correct user discovery pairs, default used 26.2% more total and 24.5% more
uncached tokens than search-only. Five of six user graph attempts needed no
follow-up retrieval, so the extra tokens cannot be ascribed simply to repeated
search. The agent's prompt, tool-call, and cache behavior remain part of the
measured total. The user known-file control made no graph calls; the default
agent read directly as intended.

Trace inspection found 17 tool calls across the six default user discovery
attempts versus 12 across search-only. Default made six graph calls, five
memory lookups, two source reads, and four other calls; every graph-using
trace also checked graph `--help` before the query (sometimes bundled with a
memory lookup). The next guidance change provides complete CLI commands and
allows a fresh, unambiguous `--include-source` result to answer a location
question without a redundant source read. That change happened **after** this
run. A separate approved [call-overhead run](codegraph_call_overhead_benchmark_20261112.md)
subsequently showed lower user-discovery token use, but also a known-file
routing regression.

Compiler discovery results remain unsuitable for a savings claim: the frozen
rubric passed two default declaration answers, two search-only declaration
answers, and no output-port-reset answers in either arm. It requires an exact
fixture and emitter symbol and can reject source-backed alternatives. The
known-file compiler control passed in both arms without graph use.

This is a two-repeat pilot with changing cached-token shares and model
stochasticity. It establishes that the corrected instruction can make graph
calls succeed while preserving known-file routing, but it does not establish
token savings or general default-agent superiority. The complete raw traces
remain in the local benchmark artifact bundle; the published records retain
each answer, score, tool breakdown, and token count.
