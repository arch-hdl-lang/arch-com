# ARCH code graph acceptance baseline

The user-source graph and compiler-development graph are separate indexes.
The developer map at `doc/codegraph_feature_map.json` covers ten reviewed
features: module, bus, fifo, thread, fsm, pipeline, pipe_reg declaration,
pipe_reg output-port reset, tlm_method, and arbiter. Links to Rust symbols are syntax-parsed and
location-checked; all ownership edges are explicitly curated.

Developer queries must name a feature identifier (the first word of its map
name), a curated alias, or an exact mapped Rust symbol. Matching is
case-insensitive and token-bounded; generic roles and shared source paths do
not establish coverage. A feature can declare `"aliases": ["arbitration"]`
to support alternate terminology without broad substring matching. Aliases
are matched as complete token sequences. `(no matches)` means no curated
match for the query, not absence from the language; use targeted source search.

Developer-query text ends with `(truncated)` when the result limit or output
budget omits matching rows. For budgets too small for that marker, it returns
`...`; a tiny budget can also shorten the no-match status to `...`.
The marker and CLI newline fit within the approximate four-byte-per-token
budget; source rows are never split. An unmarked result means no matching rows
were dropped by these limits, not that the curated map covers every owner.

Source-backed user context compacts explicit instance-target lists in one named
file, including comma-separated lists and lists longer than two names. Each
requested name must identify one instance with one existing target; incomplete
lists, duplicate names, ambiguous instances or edges, and insufficient result
limits fall back to ordinary ranked context. Names need not contain underscores.
This conservative fast path is not a general natural-language parser; other
wordings continue to use ordinary context ranking. It does not apply without
`--include-source`. Run `tools/codegraph_direct_eval.py --arch-bin target/debug/arch`
for source-checked CLI regressions, including list paraphrases. Output size is
retrieval evidence, not a measurement of live model tokens or answer correctness.

The 2026-11-13 local comparison used the same 20 direct-evaluation cases against
the `8c6b4137` binary and the list-compaction patch. All 16 existing cases passed
unchanged. Three of four added list variants failed compactness on the baseline;
all four passed after the patch, retaining source-checked instance and target
declaration lines. At the same 200-token approximate output budget, the two AXI
DMA comma-list variants shrank from 793 to 303 characters each, and the NIC-400
comma list from 749 to 368; the Oxford-comma control stayed at 368. These selected
paraphrases test the identified failure, not held-out generalization. No model
was invoked, and the prior live answer-extraction failure remains unresolved.

## Reproduce

```sh
cargo test --offline --lib dev_graph::tests
cargo test --offline --test integration_test test_graph
cargo test --offline --test integration_test test_dev_graph_cli_query_and_stale_mapping
python -m unittest discover -s mcp -p 'test_*.py'
arch graph index examples/pipe_reg_test.arch --out .archgraph --clean
arch graph context 'pipe_reg' --index .archgraph --token-budget 80
arch graph dev-index --root . --out .archdevgraph
arch graph dev-query 'pipe_reg declaration' --root . --index .archdevgraph --token-budget 160
arch graph dev-query 'pipe_reg output-port reset' --root . --index .archdevgraph --token-budget 200
```

The focused user index has 1 file, 12 nodes, and 17 edges. The developer
index has 10 features, 53 nodes, and 72 edges. The bounded declaration query
locates `PipeRegDecl`, `parse_pipe_reg_decl`, `emit_pipe_reg`, and the example at
source lines, plus the specification and guidance. The user query returns a
compact source neighborhood. Stale source files and changed generator IDs
are rejected; MCP context can refresh a stale index when explicit source
paths are supplied. MCP graph queries reject indexes whose recorded source
paths escape configured workspace roots.

A broad `arch graph index examples stdlib --out /tmp/arch-codegraph-broad-audit
--clean` run indexed 103 files, 2,026 nodes, and 7,252 edges. It reported
eight skipped roots with reasons, including an intentional CDC-violation
example, standalone files with unresolved dependencies, and the outdated
`bus_basic.arch`. A bounded `query BusAxiLite` finds the bus definition at
`stdlib/BusAxiLite.arch:21`. Context retrieval prioritizes source constructs
over generated artifact names; a FIFO-reset task starts with FIFO nodes.

All nine distinct feature-map ARCH fixtures index successfully. The old
`examples/bus_basic.arch` does not pass current compiler driver checks, so
the bus mapping uses `tests/native_param_bus_wire/Probe.arch` instead.

This is functional acceptance evidence, not a token-efficiency claim. A
paired baseline/graph benchmark with fixed tasks and provider token counts
is still needed before claiming savings comparable to HARC.
