# Selective-routing live check (2026-11-12)

This separately approved paired run used clean commit
`64bd32acc80f078e7a8a24643d0802ba545223e9`, its matching ARCH compiler,
Codex CLI `0.159.2`, `gpt-6-luna` at low reasoning effort, and the local ARCH
MCP. The unchanged seven-task manifest has SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`.
With seed 55, two repeats, and `no_graph` versus unguided `default`, there were
28 attempts. Both arms used `--ignore-user-config`; this disables
`$CODEX_HOME/config.toml`, while the harness supplied the same local MCP
configuration to both arms. The installed ARCH skill digest differed from the
repository skill (`ad6d28e7971f77a05f7a499b23cd81f74173171d3c5edb972e17b8b294fd1345`
versus `ac870874f1ef13411844755a5d15eb814ca1292621fdfb5621967d042150167e`);
the event stream does not prove whether the installed skill was loaded. These conditions differ
from earlier runs without `--ignore-user-config`, so cross-run deltas are not
causal estimates of the routing edit.

The [metadata, records, and summary](codegraph_selective_routing_20261112) are
frozen. `records.jsonl` SHA-256:
`35ec3c5b772549896d75fdd7487153aea3caf866e60b7041f91386477d7320ea`.
Full raw event and stderr traces are retained locally at
`../arch-codegraph-selective-routing-live-64bd32ac-20261112/`.
The analyzer reproduced `summary.json` byte-for-byte:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_selective_routing_20261112/records.jsonl
```

The shared user and compiler indexes took 31.685 s and 1.514 s to build,
respectively, outside agent token and latency totals.

| Cohort | Search-only rubric pass | Default rubric pass | Search-only total / uncached | Default total / uncached | Default graph calls |
| --- | ---: | ---: | ---: | ---: | ---: |
| User discovery | 6/6 | 6/6 | 390,435 / 53,027 | 279,397 / 63,077 | 3/6 |
| User known-file | 2/2 | 2/2 | 84,674 / 15,810 | 86,352 / 16,336 | 0/2 |
| Compiler discovery | 0/4 | 1/4 | 483,788 / 81,612 | 424,856 / 68,760 | 0/4 |
| Compiler known-file | 2/2 | 2/2 | 85,359 / 18,543 | 85,225 / 27,753 | 0/2 |

For the six user-discovery pairs, default used **28.4% fewer total tokens**
but **19.0% more uncached tokens**, with 7 retrieval calls versus 12 for
search-only. All three graph calls succeeded and needed no follow-up retrieval.
The two AXI DMA defaults used graph; the two exact package-import defaults
used scoped `.arch` searches and no graph. Nic400 used graph once and search
once. Across all eight user tasks, default used 23.0% fewer total tokens but
15.4% more uncached tokens and took 92.8 versus 85.0 agent-seconds. No memory
lookups were observed in either arm. These are token counts, not a cost or
latency saving claim.

The compiler discovery cases do not measure developer-graph effectiveness:
neither default attempt queried it. The four default attempts passed the
frozen exact-symbol rubric once, versus zero search-only passes. Trace review
found answers that located surrounding AST/parser code but omitted the
required specific emitter or fixture; the rubric misses are not treated as
token savings. Both known-file controls avoided graph and passed in both arms.

The two-repeat sample is small and provider cache shares varied substantially.
It supports selective graph use for the tested AXI DMA relationship and direct
search for the exact package import; it does not establish general token or
cost savings, nor improvement over the prior routing policy.
