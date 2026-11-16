# Developer Graph Coverage Validation

## Integration Scope

This change extracts the developer graph implementation and feature map from
experimental candidate `2a2d5c1bf63e9d0fd5093df6df3954ad13f02484` onto main
`fc1644ce`. It includes the developer-only named-query anchors, aliases and
explicit bounded-output truncation prerequisites from draft PR #1138. It does
not integrate that PR's user-graph changes, experimental host router, evaluation
harness, lookup MCP profile, or broader workflow recommendations.

The feature map is byte-identical to the live-tested candidate. Integration
testing additionally caught a ranking regression: a round-robin arbitration
question selected the new function feature before the arbiter. Exact aliases
now receive the same ranking preference as feature names, and query-matched
symbol words prioritize the round-robin emitter within the backend role.
Local tests and CI validate these integration fixes; the earlier live experiment
is not represented as a fresh measurement of this branch.

## Live Regression Evidence

On 2026-11-16, GPT-6 Luna with low reasoning effort reran the exact counter and
register-file ownership prompts from the completed development benchmark, two
repeats each, using the experimental routed-context arm. The same evaluator,
source pins, citation grading, 200-token graph byte budget, 8192-byte host
budget, and lifecycle/usage guards were retained. Four trials completed in four
model calls with no repairs or transport failures.

| Metric | Historical four trials | Coverage-candidate rerun |
| --- | ---: | ---: |
| Source-verified correct answers | 4/4 | 4/4 |
| Valid retrieval contracts | 4/4 | 4/4 |
| Graph attempts falling back to search | 4/4 | 0/4 |
| Host graph commands | 4 | 4 |
| All host retrieval commands | 8 | 4 |
| Total model tokens | 110,341 | 89,086 |
| Uncached input plus output tokens | 88,581 | 77,310 |

All four reruns received AST, parser and backend source evidence from the
developer graph, one graph command and a 307-byte source payload per trial.
The nested agents made no tool calls because the host supplied the graph
evidence. Agent-only `graph_calls=0` therefore does not mean graph was unused.

Total tokens were 19.3% lower and uncached tokens 12.7% lower than the four
historical trials. These are consumed development tasks and a descriptive
before/after comparison, not a fresh paired graph-versus-search ablation or a
causal savings claim. Cache state and model variation remain confounders.
No generalized savings, default-MCP efficiency, or HARC parity is established.

Independent replay checked raw lifecycle/usage, source-bound answers, the exact
four task/repeat keys, successful host graph commands, and candidate/historical
integrity. Original results were retained unchanged. The complete local bundle
is `arch-codegraph-feature-coverage-rerun-20261116`; raw session traces are not
included in this repository. Rerun records SHA-256:
`49d1de75db4be07d0bbb6b067ed79ca1ae4396d727d2071ac0fe31d971f8b73b`.

## Reproducing Offline Coverage Checks

```sh
cargo test --lib dev_graph::tests
cargo test --test integration_test test_dev_graph_cli_query_and_stale_mapping
cargo run -- graph dev-index --root . --out .archdevgraph
cargo run -- graph dev-query "counter AST parser backend" --root . --index .archdevgraph --token-budget 200 --include-source
cargo run -- graph dev-query "register-file AST parser backend" --root . --index .archdevgraph --token-budget 200 --include-source
```

The tests independently enumerate all 147 core AST variants, check exact unique
coverage, reject coverage drift and ambiguity, query every variant's owner,
preserve legacy/partial custom maps, verify freshness, and check role ordering
and bounded output. Symbol resolution verifies declarations, not semantic
equivalence; curated ownership still requires source review.
