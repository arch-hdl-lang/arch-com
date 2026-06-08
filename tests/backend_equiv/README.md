# Backend-equivalence torture fixtures

Tiny ARCH designs that deliberately **stack** the compiler's load-bearing
composition axes (3–4 at a time) to surface bugs that only appear at feature
*interactions* — the class that hit the NIC-400 interconnect ~25 times, always
at 3–4-way intersections, never at single features.

## Load-bearing axes
- **A** — `Vec<Bus,N>` ports (and nested `Vec<Vec<Bus,N>,M>`) — wiring fabric
- **B** — `thread` + `lock`/`mutex` (round_robin vs priority) — protocol logic
- **C** — `generate_for` / `generate_if` — replication
- **D** — param-driven widths & counts — parameterization
- **E** — `inst` hierarchy + cross-boundary signal forwarding — composition

## Running
```sh
./run.sh                        # uses ../../target/release/arch
ARCH=/path/to/arch ./run.sh
```
`run.sh` checks/builds/sims every fixture; exits non-zero on any regression.
This is the **arch-com side** (check/build/sim soundness). The full per-cycle
**sim↔SV trace equivalence** runs these same designs under
`harc sim --check-backends` in harc-com CI.

## Parity-pass fixtures (clean interactions)
| Fixture | Axes | What it stacks |
|---|---|---|
| `Fx1VecTapFabric` | A+C+E | generate_for over `Vec<Bus>`, each elem → child inst |
| `Fx2GenThreadLock` | A+B+C | generate_for of N threads driving `Vec<Bus>` via a lock |
| `Fx3ThreadVecParamW` | A+B+D | thread+lock driving a runtime-selected `Vec<Bus>` at param width |
| `Fx4MutexContendRR` | B+C+D | N threads contending on `mutex<round_robin>`, param N |
| `Fx5WholeVecForward` | A+D+E | `Vec<Bus>` sliced across an inst boundary, param N |
| `Fx7ThreadFwdInst` | B+D+E | thread output at param width forwarded into a child inst |
| `Fx8GenIfSelect{,Mode0}` | C+D+E | `generate_if` param-gated, different child inst per arm |
| `Fx9MiniFabric` | A+B+C+D | the full stack: per-lane `Vec<Bus>` + threads + lock + param |
| `Fx10MutexContendPrio` | B+C+D | `mutex<priority>` twin of Fx4 — policy-parity guard |

## Regression fixtures — each caught a real bug, now fixed
These were authored as bug *reproducers*; the bugs are fixed and the fixtures
must stay **green** (a regression re-breaks them). All defects were
`Vec<Bus>`-interaction bugs that no prior test caught.

| Fixture | Axes | Bug it locks down | Fix |
|---|---|---|---|
| `Fx5bWholeVecForwardCrash` | A+D+E | `arch build` **stack overflow** on whole-vector `Vec<Bus>` inst forward | `77bd55e` |
| `Fx3bVarIndexVecBusBug` (+`…Thread`, `…ThreadWrite`) | A+B+D | variable-index `Vec<Bus>` element access miscompiled in arch-sim (SV was correct → backend divergence) | `5fab9d6`, `f2c7e38` |
| `Fx1bMultiDriverBug` | A+C+E | false "multiple drivers" when a `Vec<Bus>` per-element forward sits beside a non-bus per-element forward in one `generate_for` | #528 |
| `Fx6Nested2D` | A+C+E (2-D) | same false multi-driver, one dimension deeper (`edges[r][c]`) | #533 |

## Mutex policy parity
`Fx4` (round_robin) and `Fx10` (priority) are value-cross-checked on both
backends: round_robin 10/10/10, priority 30/0/0 — identical across arch-sim and
Verilator, guarding against the silent-policy-divergence class.

## Companion leaves
`BusVr` (the shared tiny valid/ready/data bus), `VrTap`, `VrTapScalar`,
`Fx5Sink`, `Fx7Latch`, `Fx8Inc`, `Fx8Dbl`, `Fx6Prod`, `Fx6Cons`, `CrashSink`.
