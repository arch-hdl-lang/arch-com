# Compact relationship context live check (2026-11-12)

This approved paired run used clean source commit
`e8346841db29a8d35f3c46f5bb14b4388e2b7cb8`, matching ARCH compiler,
Codex CLI `0.159.2`, `gpt-6-luna`, and the local ARCH MCP configuration. It
selected the three user-discovery relationship tasks from the unchanged
manifest (SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
seed 55, two repeats, and `no_graph` versus unguided `default`: 12 attempts.
The shared user index took 31.025 s to build, excluded from token totals.

The [metadata, records, and summary](codegraph_efficiency_20261112) are
frozen. `records.jsonl` SHA-256:
`25ee09f42e15c5076b7b138085600b4c75b4f2e04029ef518d8b18402eeb291c`.
Reproduce the summary with:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_efficiency_20261112/records.jsonl
```

Both arms answered all six attempts correctly. Default successfully queried
graph in **6/6** attempts. It used **598,590 total / 133,438 uncached** tokens
versus search-only **489,916 total / 138,556 uncached**: **22.2% more total**
and **3.7% fewer uncached** tokens. Default took 101.2 agent-seconds versus
76.9 for search-only. Therefore the 70.2% smaller deterministic graph payload
did **not** produce total-token savings in this run.

Trace inspection found a prior-memory lookup in every default attempt and none
in the search-only arm. Both package defaults made an additional `rg` call
after graph to verify the target declaration; one Nic400 default read the
instance and target files after graph. The source-backed graph rows showed
the target path and name but not the target declaration text, so the extra
verification was understandable. A follow-up renderer change can add that
missing evidence, but these records do **not** measure such a change.

This small, cache-sensitive run cannot isolate the compact-output change from
agent retrieval choices or establish general graph-by-default savings. Full
raw traces are retained in the local benchmark bundle.
