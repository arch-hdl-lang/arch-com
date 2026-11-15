# Known-file prompt live check (2026-11-13)

The approved focused run tested the known-file prompt change at clean commit
`7fdc64af71f80d1b66c49f7bd20bb37c37e42e12`. It used the unchanged
seven-task manifest (SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
selecting its two known-file controls, two repeats, seed 55, `no_graph` and
unguided `default`, `gpt-6-luna` at low reasoning effort, Codex CLI 0.159.2,
the local ARCH MCP, `--ignore-user-config`, source excerpts, and a 200-token
graph budget. The ARCH binary was built from `b7c3bbe2`; intervening commits
changed only instructions, benchmark code, or reports. Shared index setup
took 30.847 s for user files and 1.595 s for compiler files, excluded from
agent token and latency totals.

The first sandboxed invocation failed before any model attempt because nested
Codex could not initialize its state database. It yielded eight zero-token
records and is excluded. The authorized retry produced eight usage-bearing
attempts with no failed process exits. Its [metadata, records, and summary](codegraph_known_file_prompt_20261113)
are frozen; `records.jsonl` SHA-256 is
`f392ad80f7e6eca93d0d3b64df7c3327dc2a7e06a230062636fab429aba2c52b`.
Raw traces are in the local
`../arch-codegraph-known-file-live-7fdc64af-20261113-escalated/` bundle.

| Arm | Correct | Total tokens | Uncached tokens | Graph calls |
| --- | ---: | ---: | ---: | ---: |
| Search-only | 4/4 | 171,331 | 50,755 | 0 |
| Default | 2/4 | 151,332 | 41,508 | 0 |

The lower default aggregate is **not a saving**: one incorrect default
answered `examples/fsm_counter.arch:1` without a retrieval call, and the
other returned `emit_sync_fifo_body` after a broad `rg -n -C 2` against the
named Rust file. That search emitted 9,097 characters and included the
correct `fn emit_fifo_sync_body` declaration at line 222. The two successful
pairs had virtually equal total tokens (85,631 default versus 85,583
search-only). No graph call occurred; this run measures known-file quality,
not graph savings.

The prompt-only change did not restore quality parity. Do not use the default
arm's lower aggregate tokens as an efficiency result or mark the draft PR
ready on this evidence. A follow-up should address retrieval compliance and
answer extraction separately, then validate on fresh tasks rather than tuning
only these two controls.
