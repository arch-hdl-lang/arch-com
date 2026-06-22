# Enhancement Proposal: `--warn-output-timing` — Output Latency Transparency Lint

**Date:** 2026-06-22  
**Status:** Proposal — not yet tracked in any issue or plan doc  
**Scope:** `arch check`, `arch build`, `arch sim`  
**Effort estimate:** Small — purely diagnostic; no new language constructs, no IR changes

---

## Problem

ARCH's `port reg` declaration adds exactly 1 cycle of output latency: the
output reflects the value clocked in on the *previous* posedge, not the
current combinational state. A plain `port` output driven in `comb` or via
`let` is zero-latency. Both are correct ARCH; both compile without warning.

The distinction is easy to miss, especially for:

- developers coming from SystemVerilog (where all `always_ff` outputs are
  registered but `output logic` doesn't signal this at the declaration site)
- AI agents generating ARCH code, which lack intuitive cycle-timing intuition
- readers of unfamiliar code scanning a large module definition

Evidence that this is a real pain point:

> **`doc/cvdp_learnings.md` ("The `port reg` Timing Lesson"):**  
> "Root cause of the hardest debug: `port reg` adds 1-cycle output latency
> that wasn't documented … The reference SV from the dataset also fails the
> same test with `port reg` — it's a timing contract issue."

> **`doc/arch_coding_reflection_2026-04-12.md` ("What Felt Frictional", item 2):**  
> "Timing intent can be easy to misread from surface syntax … `port reg` vs
> `let out = reg_x` … Add a clearer notion of output timing intent. Possible
> improvements: optional lints like 'output depends on reg but is
> combinational'."

The reflection identifies this as the **#2 most costly friction source** in
real coding sessions, yet it has never been turned into an issue or plan doc.

---

## Proposed Change

### New flag: `--warn-output-timing`

Accepted by `arch check`, `arch build`, and `arch sim`. Off by default;
opt-in so it never breaks existing CI.

When enabled, `arch check` (and the check phase of `build`/`sim`) emits
`TIMING-NOTE` messages to stderr after type-checking succeeds. These are not
errors; they are informational annotations tied to specific output port
declarations.

### Three lint classes

#### 1. Registered output (`port reg`)

**Pattern:** any `port reg o: out T` declaration.

**Message:**
```
TIMING-NOTE: Module.o is a registered output (1-cycle latency).
  The testbench or downstream consumer reads the value clocked in at
  the previous posedge — not the current combinational state.
  If zero-latency output is needed, use `port o: out T` driven in a
  `comb` block or via `let`.
  → Module.arch:12:3
```

#### 2. Combinational passthrough of a register

**Pattern:** `let o = some_reg;` or `comb o = some_reg;` where `o` is an
output port and `some_reg` is a `reg` declaration.

**Message:**
```
TIMING-NOTE: Module.o is a combinational passthrough of register some_reg.
  Output latency is 0 cycles from the output port itself, but `some_reg`
  already reflects the value assigned on the previous posedge.
  Net effect: the output shows the previous-cycle value of `some_reg`.
  → Module.arch:18:3
```

This class is subtler than the first: the output port itself is
combinational, yet it exposes state that is 1 cycle stale. The note makes
that explicit without changing any semantics.

#### 3. FSM/thread output driven from state register

**Pattern:** an FSM or `thread`-lowered module where an output port is
driven combinationally from the state register (the common "Mealy vs Moore"
ambiguity).

**Message:**
```
TIMING-NOTE: Fsm.mode_out is driven combinationally from the FSM state
  register t0_state. It reflects the *current* state (Moore output) — reads
  in the same cycle as a state transition are valid.
  → Fsm.arch:34:3
```

This is an informational note, not a warning — Moore outputs from FSMs are
idiomatic and correct. The note helps testbench authors confirm the latency
contract without reading codegen.

### Optional SV annotation: `--annotate-timing`

A companion flag for `arch build` that inlines single-line timing comments
into the generated SV:

```systemverilog
// [ARCH: registered output, 1-cycle latency]
output logic [7:0] data_out,
// [ARCH: combinational output, 0-cycle latency]
output logic       valid_out,
```

This makes the latency contract visible in the SV output that downstream
tools and reviewers consume. Comments are in `// [ARCH: ...]` form to be
grep-friendly.

---

## Implementation Sketch

All implementation lives in the compiler's existing check/type-check phase.
No new IR nodes, no codegen changes (except the optional SV comment for
`--annotate-timing`).

### `src/typecheck.rs` (or a new `src/lint.rs`)

After the type-check pass succeeds, add a `lint_output_timing()` function
that:

1. Iterates all module/fsm/pipeline output ports.
2. For each output port, determines its drive source from the already-built
   symbol table:
   - `SeqDriven` (driven in `seq`) → class 1 (registered output)
   - `CombDriven` where the RHS is a `Reg` identifier → class 2 (passthrough)
   - `CombDriven` where the RHS is the FSM state reg → class 3 (FSM Moore)
   - All other `CombDriven` / `LetDriven` → no note
3. If `--warn-output-timing` is set, emit a `TIMING-NOTE` to stderr with
   the source span.

The symbol table and drive-source information is already available after
type-check (the single-driver check already classifies each port as
`SeqDriven` vs `CombDriven`). This is a read-only traversal of existing
compiler state.

### `src/build.rs` (SV emitter) — optional

For `--annotate-timing`: when emitting each output port in the SV port list,
query the drive classification (already computed) and prepend the comment.
~10 lines of codegen code.

### CLI (`src/main.rs`)

Add `--warn-output-timing` (bool, default false) to the `CheckOpts`/`BuildOpts`
structs and thread it through to the lint pass. Add `--annotate-timing` to
`BuildOpts`.

---

## What This Is Not

- **Not a breaking change.** All lint messages go to stderr only; exit code
  and stdout are unchanged. Existing CI that does not pass `--warn-output-timing`
  is unaffected.
- **Not a replacement for `arch advise`.** `arch advise` retrieves past
  error→fix pairs from the learning store. This lint fires proactively on
  valid, compile-succeeding code.
- **Not a replacement for `--debug` or `--check-uninit`.** Those flags
  address simulation-time uninitialized reads and value tracing. This lint
  is a compile-time structural note about output latency contracts.
- **Not the code graph** (issue #592). The code graph is about module-level
  navigation across a design. This is a per-module, per-port annotation tied
  to the type-checker.

---

## Rationale for Off-by-Default

Emitting `TIMING-NOTE` on every `port reg` output unconditionally would be
noisy for developers who already understand ARCH's timing model. Making it
opt-in means:

- CI pipelines are unaffected until explicitly updated
- Developers can enable it when onboarding to an unfamiliar codebase
- AI agents can pass `--warn-output-timing` when generating code to get
  explicit confirmation of latency contracts
- MCP tooling can expose it as a targeted query ("explain the output timing
  of this module")

A future pass could promote it to a default `--warn` category (like
`--warn-output-timing=always`) once the community has calibrated the
signal-to-noise ratio.

---

## Relationship to Existing Work

| Existing feature | Overlap | Distinction |
|---|---|---|
| Operator-precedence ambiguity check (`arch check`) | Same opt-out model (structural check at compile time) | Addresses syntax-level footguns; this addresses semantic timing footguns |
| `--check-uninit` | Both are compile-time diagnostic flags | Uninit checks reads-of-unwritten registers; this checks output latency declarations |
| `--auto-thread-asserts` | Both emit supplemental information about lowered state | Thread asserts emit SVA properties for formal tools; this emits human/machine-readable notes |
| `arch advise <query>` | Both surface compiler knowledge to the user | `advise` retrieves past error→fix pairs; this is a proactive structural lint |
| Code graph / `arch check --explain` (issue #592) | Both surface semantic information | Code graph is module-graph navigation; this is per-declaration timing annotation |

---

## Success Criteria

- `arch check --warn-output-timing Foo.arch` emits at least one `TIMING-NOTE`
  for every `port reg` output in `Foo.arch`, pointing to the correct source line.
- `arch check --warn-output-timing Foo.arch` emits no `TIMING-NOTE` for any
  `port` output driven in a `comb` block or via `let` from a non-register
  signal.
- `arch build Foo.arch --annotate-timing` produces SV with `// [ARCH: ...]`
  latency comments on each output port.
- Exit code is unchanged (0 on successful compile regardless of notes emitted).
- All existing `arch check` integration tests continue to pass without changes.
- The coffee_machine module from CVDP, compiled with `--warn-output-timing`,
  produces a `TIMING-NOTE` for the `port reg` output that caused the debug
  regression in the benchmark.

---

## Examples

**Before (today):**
```arch
module Alu
  port clk:    in Clock<SysDomain>;
  port rst:    in Reset<Sync>;
  port a, b:   in UInt<8>;
  port result: out UInt<8>;      // combinational, 0-cycle latency
  port reg acc: out UInt<8> reset rst => 0;  // registered, 1-cycle latency
  ...
end module Alu
```
```
$ arch check Alu.arch
# (no output — both are valid)
```

**After (with `--warn-output-timing`):**
```
$ arch check --warn-output-timing Alu.arch
TIMING-NOTE: Alu.acc is a registered output (1-cycle latency).
  The testbench or downstream consumer reads the value clocked in at
  the previous posedge — not the current combinational state.
  If zero-latency output is needed, use `port acc: out UInt<8>` driven in
  a `comb` block or via `let`.
  → Alu.arch:6:3
```

**With `--annotate-timing` on build:**
```systemverilog
module Alu (
  input  logic       clk,
  input  logic       rst,
  input  logic [7:0] a,
  input  logic [7:0] b,
  // [ARCH: combinational output, 0-cycle latency]
  output logic [7:0] result,
  // [ARCH: registered output, 1-cycle latency]
  output logic [7:0] acc
);
```
