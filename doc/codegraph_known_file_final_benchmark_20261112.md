# Final known-file routing check (2026-11-12)

This approved live run evaluated the named-file lookup rule on clean commit
`7b0dd478b0b8a3fbd257c82aae47420a1ffc20aa`, with the matching ARCH
compiler, Codex CLI `0.159.2`, and `gpt-6-luna` with the local ARCH MCP
configured. It used the unchanged task manifest (SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
seed 55, two repeats of each known-file task, and `no_graph` and unguided
`default` conditions: eight attempts. User and compiler index preparation took
30.761 s and 1.525 s, excluded from token totals.

The [metadata, records, and summary](codegraph_known_file_final_20261112) are
frozen. `records.jsonl` SHA-256:
`fa04147a2e35e79f18c70a4998002338d869790e54fb7203a2fc990a91fa2f03`.
Recompute the summary with:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_known_file_final_20261112/records.jsonl
```

Default passed **4/4**, made exactly one targeted `rg` call against the named
file in each attempt, and made **0/4 graph calls**. In particular, both
compiler attempts found `emit_fifo_sync_body` at `src/codegen/fifo.rs:222`.
This addresses the earlier [known-file failure](codegraph_known_file_benchmark_20261112.md),
where both compiler defaults guessed without retrieval.

Search-only passed **3/4**. Its failed compiler attempt made zero tool calls
and guessed `gen_fifo_body`; the other three search-only attempts used one
targeted search each. The harness marked that failed attempt's `valid_method`
as true, so the method field alone is not evidence of source verification;
the per-attempt tool breakdown and raw trace were checked as well.

Among the three pairs where both arms were correct, default used 0.05% more
total tokens than search-only, effectively tied. Comparing all eight token
totals would be misleading because the incorrect zero-call answer was much
shorter. This small known-file control validates the final default lookup
behavior in these attempts; it does **not** measure graph savings or prove
general correctness. Full raw traces remain in the local benchmark bundle.
