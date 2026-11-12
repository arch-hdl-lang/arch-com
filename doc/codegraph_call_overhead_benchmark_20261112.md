# Graph call-overhead benchmark (2026-11-12)

This approved paired run evaluated the complete-command and source-backed
answer guidance at clean commit `abb4fd3566b33855683b683b943c9fddbd2acc92`,
with matching ARCH compiler, Codex CLI `0.159.2`, and `gpt-6-luna`. It used the
same frozen seven-task manifest (SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
seed 55, two repeats, and search-only (`no_graph`) and unguided (`default`)
arms: 28 attempts. User and compiler index preparation took 30.839 s and
1.588 s, excluded from token totals.

The [metadata, records, and summary](codegraph_call_overhead_20261112) are
frozen. `records.jsonl` SHA-256:
`8f237fe46a6f6c47be0159df66dfb00ccc0c57da5c5a5725321f371e8cbed376`.
Reproduce the summary with:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_call_overhead_20261112/records.jsonl
```

| User discovery arm | Correct | Total tokens | Uncached tokens | Tool calls | Graph calls |
| --- | ---: | ---: | ---: | ---: | ---: |
| Search-only | 6/6 | 488,994 | 110,242 | 12 | 0 |
| Unguided default | 6/6 | 429,853 | 104,093 | 11 | 6 |

On those six correct user discovery pairs, default used 12.1% fewer total and
5.6% fewer uncached tokens than search-only. It used graph successfully in
6/6 attempts, with five memory lookups but no separate `--help` calls or
source reads. The [prior path-fix run](codegraph_routing_pathfix_benchmark_20261112.md)
had 17 default tool calls in this subset, including redundant help checks and
two source reads. Different cache shares and stochastic tool choices mean the
cross-run reduction cannot be attributed solely to this guidance edit.

The known-file controls regressed. Default used graph in 3/4 controls, even
though both prompts name the file to inspect. One compiler control queried
the user `.archgraph` for a Rust source question, then answered
`emit_fifo_sync_fwft_body` instead of the required `emit_fifo_sync_body`.
Default passed 3/4 known-file attempts, versus 4/4 for search-only. The prior
path-fix run avoided graph on all four and passed all four. The user discovery
gain therefore does **not** justify broad default routing as-is.

Compiler discovery scores remain constrained by the exact-symbol rubric and
vary across two-repeat runs; no compiler-token saving claim follows. The
sample is small, provider cache shares vary, and index cost is excluded. Full
per-attempt traces are retained locally; published records include answers,
scores, tool breakdowns, and token counts.

After this run, `AGENTS.md` and the ARCH skill were tightened to direct-read
named files for in-file facts and to forbid using the user graph for Rust
compiler source. A separate approved
[known-file check](codegraph_known_file_benchmark_20261112.md) found no graph
calls but two wrong compiler answers produced without any source retrieval.
The rule was therefore tightened again to require one targeted named-file
read or search; that final correction remains unmeasured.
