# Compiler-routing live check (2026-11-12)

This separately approved paired run tested the compiler-ownership routing
clarification at clean commit `b7c3bbe2bee86c41d0fe51af7261eb504d415d43`
with its matching ARCH compiler, Codex CLI `0.159.2`, `gpt-6-luna` at low
reasoning effort, and the local ARCH MCP. It reused the unchanged seven-task
manifest (SHA-256
`a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
seed 55, two repeats, `no_graph` versus unguided `default`, a 200-token graph
budget, source excerpts, and `--ignore-user-config`: 28 attempts. The latter
disables `$CODEX_HOME/config.toml`; the event stream does not prove whether the
older installed ARCH skill was loaded. The shared user and compiler indexes
took 30.813 s and 1.570 s, outside agent token and latency totals.

The [metadata, records, and summary](codegraph_compiler_routing_20261112) are
frozen. `records.jsonl` SHA-256:
`f16ede9ea4980f55e79a162f88ea0b056d9bc42cfcd847ed95cf7e088b889553`.
There are 28 unique attempt keys, no failed process exits, and three
successful graph calls. The analyzer reproduced `summary.json` byte-for-byte:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_compiler_routing_20261112/records.jsonl
```

Full raw event and stderr traces remain in the local
`../arch-codegraph-compiler-routing-live-b7c3bbe2-20261112/` bundle.

| Cohort | Search-only rubric pass | Default rubric pass | Search-only total / uncached | Default total / uncached | Default graph calls |
| --- | ---: | ---: | ---: | ---: | ---: |
| User discovery | 6/6 | 6/6 | 375,591 / 68,135 | 350,039 / 73,943 | 0/6 |
| User known-file | 2/2 | 1/2 | 84,886 / 17,814 | 65,189 / 20,389 | 0/2 |
| Compiler discovery | 2/4 | 3/4 | 470,450 / 76,210 | 354,613 / 81,205 | 3/4 |
| Compiler known-file | 2/2 | 1/2 | 86,065 / 21,041 | 64,000 / 10,752 | 0/2 |

The compiler default queried the developer graph in three of four discovery
attempts; all three graph-using answers passed the frozen rubric. The fourth
began with broad `rg` and failed. On the two `pipe_reg` declaration-map pairs
that both arms passed, default used 108,688 total / 22,544 uncached tokens
versus 231,088 / 36,016 for search-only: 53.0% fewer total and 37.4% fewer
uncached, with three versus seven retrieval calls. One default still read
source after graph; the other answered from one graph call. For the reset-map
task, default passed one of two and search-only passed none, but default used
245,925 total / 58,661 uncached tokens versus 239,362 / 40,194. This is not
a uniform compiler-work saving.

All six user-discovery attempts passed in both arms, but **neither arm used
graph**, so their 6.8% default total-token reduction cannot be credited to
graph. Default used 8.5% more uncached tokens on that cohort. Both exact
package-import defaults searched directly. Two default known-file controls
answered with zero retrieval calls and were wrong: the user control gave
`examples/fsm_counter.arch:1` instead of line 16, and the compiler control
gave `emit_sync_fifo_body` instead of `emit_fifo_sync_body`. Their short token
counts must not be treated as savings. Search-only passed all four known-file
attempts. Across all 14 attempts per arm, default passed 11 frozen rubrics
versus search-only's 12.

Human review found valid alternate answers rejected by the exact-symbol
rubric. A search-only reset-map answer correctly identified
`emit_reg_block` and a valid `Prob041_dff8r.arch` fixture. A default reset-map
answer cited reset-sensitive `always_ff` emission at
`src/codegen/module.rs:1555` (inside `emit_reg_block`) and a valid
`tests/assert_cover/light.arch` fixture. Crediting both, without changing
the frozen records, makes compiler discovery 4/4 default versus 3/4
search-only. The other search-only reset-map answer points at a generic
helper rather than the registered-port reset emitter and remains incomplete.

This is a two-repeat, cache-sensitive agent-choice sample. The previous run
made zero compiler developer-graph calls, while this one made three; the
cross-run change is consistent with improved routing but cannot isolate the
instruction edit from stochastic agent behavior. Known-file failures and
zero user-graph use prevent a general graph-by-default or whole-workflow
token-saving claim. Keep the PR draft pending a quality-preserving result.
