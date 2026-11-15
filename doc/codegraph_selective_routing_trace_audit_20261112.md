# Selective-routing trace audit (2026-11-12)

This offline review uses the 28 frozen attempts and local raw event traces
from the [selective-routing live check](codegraph_selective_routing_benchmark_20261112.md).
It launches no model or MCP calls. All 28 records had unique task/repeat/arm
keys and successful process exits. The event stream reports one aggregate
`turn.completed` usage object per attempt, not per model request or retrieval
call, so provider tokens cannot be attributed to a specific tool call.

## User discovery

| Task, two repeats | Search-only total / uncached | Default total / uncached | Default graph calls |
| --- | ---: | ---: | ---: |
| AXI DMA instance targets | 154,560 / 21,824 | 87,238 / 21,574 | 2 |
| Nic400 FIFO targets | 150,775 / 22,263 | 107,051 / 21,163 | 1 |
| Exact package import | 85,100 / 8,940 | 85,108 / 20,340 | 0 |

The package task made one direct search call in every attempt. Its total-token
arms differed by only eight tokens, while default had 11,400 more uncached
tokens and correspondingly fewer cached tokens. That one cache-share difference
exceeds the 10,050-token net uncached regression across all six user-discovery
pairs. AXI DMA and Nic400 together used 1,350 fewer uncached tokens in default.
Therefore the aggregate uncached regression does not show that the package
routing consumed more model work, and no tool-level cost attribution follows.

Default used graph in both AXI DMA attempts and one Nic400 attempt. All three
graph calls succeeded with no subsequent retrieval. The package defaults used
scoped `.arch` searches, and both known-file controls in each arm avoided
graph. Across the six user-discovery attempts, default made 7 retrieval calls
versus 12 for search-only. The two-repeat sample still cannot establish a
general saving or isolate the new routing wording from agent/cache variation.

## Compiler discovery

All four compiler-default attempts began with broad `rg` and made zero
developer-graph calls. The frozen exact-symbol rubric passed one of four
default answers and zero of four search-only answers. Three `pipe_reg`
declaration-map answers (one default, two search-only) instead cited valid
shipped fixtures: `tests/cvdp/iir_filter.arch:15` or
`tests/buf_mgr/buf_mgr.arch:78`, both containing the requested declaration
syntax. Crediting those alternate fixtures without changing the frozen score
gives 2/4 compiler-discovery answers in each arm. The port-reset answers
still failed to identify the specific sequential emitter function.
The [direct retrieval gate](codegraph_eval_protocol.md) independently passed
the two frozen `pipe_reg` owner cases at a 200-token budget (751 and 768
output characters, 0.016 s each). This shows the indexed rows were available,
not that an agent would use them or answer correctly from them.

The existing direct-search exception for an exact ARCH declaration token may
be read too broadly when the question actually asks for compiler ownership.
`AGENTS.md` and the repository ARCH skill now explicitly route cross-stage
AST/parser/emitter/fixture questions to one developer-graph query, while
preserving direct reads for named Rust-file facts. This instruction change
has no post-fix live token or correctness result.
