# Known-file routing check (2026-11-12)

This approved focused run evaluated the direct-read boundary on clean commit
`81111ef70096d78dc697db0d60b8ba539e9e4519`, with matching ARCH compiler,
Codex CLI `0.159.2`, and `gpt-6-luna`. It selected the two known-file controls
from the unchanged seven-task manifest (SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
seed 55, two repeats, and search-only (`no_graph`) and unguided (`default`)
arms: eight attempts. User and compiler index preparation took 30.591 s and
1.783 s, excluded from token totals.

The [metadata, records, and summary](codegraph_known_file_20261112) are
frozen. `records.jsonl` SHA-256:
`a920ba5a8aab5e5005a3413ace18e2235af35295b9d6ab3f6ce6df3e4307a2b4`.
Reproduce the summary with:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_known_file_20261112/records.jsonl
```

Default made no graph calls in all four known-file attempts, repairing the
tool-choice regression seen in the prior
[call-overhead run](codegraph_call_overhead_benchmark_20261112.md). Both user
known-file defaults used targeted search and passed. Both compiler known-file
defaults, however, made **no retrieval call** and incorrectly answered
`gen_sync_fifo`; `src/codegen/fifo.rs:222` defines the required
`emit_fifo_sync_body`. Search-only passed all four controls. Default passed
only 2/4, so its lower aggregate token count is not a saving: the failed
compiler answers skipped verification altogether.

After this run, `AGENTS.md` and the ARCH skill were tightened to require one
targeted read or `rg -n` of a named file before answering an in-file fact.
That correction is **unmeasured** by these records. The two-repeat pilot does
not establish known-file correctness or general token savings. Complete raw
traces remain in the local benchmark artifact bundle; published records
contain each answer, score, tool breakdown, and token count.
