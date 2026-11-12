# Agent routing follow-up benchmark (2026-11-12)

This run evaluated the first top-level `AGENTS.md` selective-routing rule at
clean commit `2cb09fefb8dc43292af4d1140583022cac4310f8` with a matching
ARCH compiler binary. It used the same frozen seven-task manifest as the
earlier [follow-up benchmark](codegraph_followup_benchmark_20261112.md)
(SHA-256 `a4638cfa6ca4bf6af7d93f443c6f78c16c837548835d149c428092f05f263c26`),
Codex CLI `0.159.2`, `gpt-6-luna`, seed 55, two repeats, and only search-only
(`no_graph`) and unguided (`default`) arms: 28 attempts. Graph indexes were
prepared in 30.914 s for user code and 1.568 s for compiler code. Index time
is excluded from token counts.

The [metadata, records, and analyzer summary](codegraph_routing_20261112)
are frozen. `records.jsonl` SHA-256:
`63b328ffa8352a77ac6a4dc92f5badf530c66f81884275e73ec6ffc1976ceab0`.
Regenerate the summary with:

```sh
python3 tools/codegraph_eval.py analyze doc/codegraph_routing_20261112/records.jsonl
```

| User discovery arm | Correct | Total tokens | Uncached tokens | Graph attempts / successes |
| --- | ---: | ---: | ---: | ---: |
| Search-only | 6/6 | 426,259 | 94,867 | 0 / 0 |
| Unguided default | 6/6 | 756,225 | 118,657 | 2 / 0 |

Default used 77.4% more total and 25.1% more uncached tokens on these six
correct user discovery pairs. Across both cohorts, it attempted graph in 2/10
discovery attempts and used no graph calls on known-file controls. Both graph
attempts failed: the agent invoked bare `arch graph context`, but its login
shell resolved `arch` to macOS `/usr/bin/arch`, which reported
`arch: Can't find graph in PATH`. For example, a login shell in this checkout
returns `/usr/bin/arch` from `command -v arch` even when the benchmark process
prepends `target/debug` to `PATH`. The agents then fell back to source search.
Thus the run measures a failed invocation path, not successful unguided graph
retrieval. Full per-attempt traces, including both command errors, remain in
the local benchmark artifact bundle.

All eight user attempts in each arm passed. In the compiler cohort, search-only
passed 3/6 and default passed 1/6 under the frozen, exact-symbol rubric. One
default known-file control answered the wrong function without any retrieval.
No compiler-token saving claim follows. Two repeats and varying cache state
also limit any generalization from the user-token comparison.

After this run, `AGENTS.md` was changed to name `./target/debug/arch` in this
checkout and to warn about the macOS utility. That repair was **not** tested
by these 28 attempts. Do not claim improved graph selection or token savings
from it until a separate approved live rerun shows successful calls without
regressing known-file controls.
