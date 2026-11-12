# ARCH graph direct retrieval exploration

The direct audit was run against the frozen four discovery prompts on the
current PR branch, without nested Codex calls. It is a retrieval-quality gate,
not a provider-token benchmark. The user index covered `examples/`, `stdlib/`,
and `tests/`; index build took 29.4 seconds and the developer index 1.3 seconds.

At an approximate 100-token output budget, all four cases passed. `TxQueue` moved from
rank 15 to rank 1; the `BusAxiLite` declaration and test copy were at ranks 4
and 1; the arbiter and `pipe_reg` AST/parser/backend rows were at ranks 2-4.
Each response was 335-374 characters. User context queries took about 0.6
seconds and developer queries about 0.01 seconds on this machine. All reported
source lines were checked against the current files.

The same 100-token audit against the unchanged compiler failed two ranking
gates: `TxQueue` and `pipe_reg`'s AST/backend rows were absent from the
bounded response. With a 300-token probe, those rows were at ranks 15, 11,
and 13 respectively. The other two cases passed before and after.

The changes only affect ranking: `context` now rewards an exact node-name
term in a natural-language task, and `dev-query` rewards a feature-name phrase
appearing in the task. No index schema, source graph data, or MCP signature
changed. The next live agent experiment should test whether the tighter
results reduce follow-on tool calls and provider tokens without harming
known-file behavior or answer correctness.
