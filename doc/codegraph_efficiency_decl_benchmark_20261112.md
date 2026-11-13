# Target-declaration context live check (2026-11-12)

This separately approved paired run tested the source-backed target-declaration
excerpts on clean commit `10b14b99d1fce18e066f6a0c9c9ccc4e1f0b0b88`, with
the matching ARCH compiler, Codex CLI `0.159.2`, `gpt-6-luna`, and the local
ARCH MCP configuration. It used the unchanged manifest (SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
the same three user-discovery relationship tasks, seed 55, two repeats, and
`no_graph` versus unguided `default`: 12 attempts. The shared user index took
30.979 s to build, excluded from token totals.

The [metadata, records, and summary](codegraph_efficiency_decl_20261112) are
frozen. `records.jsonl` SHA-256:
`ba94119834d6fd0de87939a8e9443918363a2709070d44c2bc394f505608ceb2`.
Recompute the summary with:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_efficiency_decl_20261112/records.jsonl
```

Both arms answered **6/6** correctly, and default successfully used graph in
**6/6** attempts. Default used **478,469 total / 88,965 uncached** tokens
versus search-only **438,514 total / 83,570 uncached**: **9.1% more total**
and **6.5% more uncached** tokens. Default took 85.5 agent-seconds versus 72.6
for search-only. The result does **not** establish token savings.

By task, default used 132,251 versus 166,501 total tokens for AXI DMA
(20.6% fewer), 158,626 versus 140,268 for Nic400 (13.1% more), and 187,592
versus 131,745 for the package import (42.4% more). Search-only located the
named package declaration cheaply; graph was an unnecessary detour there in
these attempts. Default made 13 retrieval calls versus search-only's 10,
including five prior-memory lookups absent from the search-only arm. One
package default still searched after graph. The preceding
[compact-output run](codegraph_efficiency_benchmark_20261112.md) observed 15
default calls and three after-graph calls; this run observed 13 and one,
respectively. Those are different agent samples, not a causal estimate of the
renderer change.

The two-repeat, cache-sensitive test is limited to these user-discovery tasks.
It does not show that compact output or target excerpts reduce provider tokens
generally; full raw traces remain in the local benchmark bundle.
