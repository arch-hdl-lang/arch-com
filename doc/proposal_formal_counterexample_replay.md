# Proposal: `arch formal --replay` — turn a BMC counterexample into a runnable `arch sim` reproduction

*Author: scheduled research review, 2026-07-12. Status: research note — no
code changes proposed yet.*

## Context

`arch formal` is a real, verified capability: direct AST→SMT-LIB2 bounded
model checking with z3/boolector/bitwuzla, hierarchical v1 flattening, and
end-to-end verification against EBMC (see `CLAUDE.md` §"Runtime bounds
checking" / §"Runtime divide-by-zero checking" for worked REFUTED/PROVED
examples, and `doc/COMPILER_STATUS.md` line ~317 for the `arch formal`
feature summary). When a property fails, the engine already computes a
faithful counterexample and threads it through the pipeline:

- `src/formal.rs::render_counterexample` (~line 3429) builds a per-cycle
  human-readable text block: `"Counterexample for `<prop>` at cycle N:"`
  followed by register values at each cycle up to the failing one.
- `CheckResult.counterexample: Option<String>` (~line 49) carries that text
  on every `Refuted`/`Hit` result.
- The CLI driver (~line 3590) prints it to stderr, indented, one line per
  register-at-cycle.

That is the entire lifecycle: SMT model → per-cycle register assignments →
formatted text → stderr. Once printed, the counterexample is a dead end —
the register trace the solver derived is discarded, and a human has to
manually transcribe register values back into a `.cpp` testbench (or a
`--pybind --test` Python TB) to reproduce the failure in `arch sim` and
step through it with `--debug+fsm`, add scoped `log()` calls, or attach a
waveform. For any property with more than a handful of registers or more
than a few cycles to the failing point, that transcription is tedious and
error-prone — exactly the kind of task that should be automated given the
data already exists inside the compiler.

## Problem

Formal and simulation are the project's two verification backends, and the
project already invests heavily in cross-checking them (`arch sim
--thread-sim both`, EBMC + Verilator `--assert` verification runs recorded
in `CLAUDE.md`). But the handoff runs only one direction today: *sim explores,
formal proves*. There's no return path — *formal refutes, sim replays* — even
though formal is strictly better at finding the failing input (BMC explores
the full input space up to the bound; a human-written TB only explores what
they thought to drive). The practical effect: a `REFUTED` result is
useful for "yes, there's a bug" but not for "let me single-step through it,"
which is where `arch sim --debug+fsm` already excels. Users (and the
project's own EBMC-verification workflow, per `CLAUDE.md`) are left
re-deriving by hand a reproduction that the solver already computed.

## Proposed approach

Add a machine-readable counterexample format alongside the existing text
rendering, and a flag to convert it directly into an `arch sim` stimulus.

1. **Structured counterexample.** Keep `render_counterexample`'s text output
   (useful for quick reading) but also populate a typed
   `Vec<CycleAssignment>` — `{cycle: u32, port_or_reg: String, value: BigUint}`
   — from the same SMT model values that already feed the text renderer.
   This is a refactor of the existing render path, not new solver work: the
   values are already extracted from the model to build the text lines.

2. **`--emit-cex-json out.json`** on `arch formal`: dump the structured
   trace (property name, failing cycle, and the full per-cycle
   primary-input assignment — not just registers, since registers are
   derived from inputs and a replay TB needs to *drive* inputs, not
   registers) as JSON.

3. **`arch formal --replay-tb out.cpp`** (or a small separate tool reading
   the JSON): emit a minimal `arch sim`-compatible C++ testbench that
   drives exactly the primary inputs the solver used, cycle by cycle, via
   the generated `dut.set_<port>(v)` setters, then stops. This reuses
   existing scaffolding — it's structurally the same shape as any other
   `arch sim --tb tb.cpp` testbench, just auto-generated instead of
   hand-written. Running the emitted TB under `arch sim --debug+fsm --wave
   cex.vcd` immediately gives a waveform and FSM trace of the exact
   failure, for free.

4. Optionally, a `--pybind`-flavored emitter (`--replay-tb out.py`) for
   users already in the cocotb-shim flow (`arch sim --pybind --test`,
   documented in `doc/arch_sim_cocotb.md`), since driving inputs via
   `.value =` writes is even more mechanical than the C++ setter form.

## Why this is worth doing over other backlog items

- **Reuses data that already exists.** No new formal-engine capability is
  needed — the per-cycle model values are already extracted for the text
  renderer; this proposal is a structured-output + codegen problem, not a
  BMC problem.
- **Compounds with existing strengths.** The project's `--debug+fsm`
  instrumentation and coverage-annotated sim are already best-in-class
  debugging tools per `CLAUDE.md`; this proposal is the shortest path to
  pointing them at formal's output instead of only at hand-written stimuli.
  It directly serves the "Fix-PR lifecycle" discipline in `CLAUDE.md`
  ("re-reproduce before you fix") by making the reproduction step
  mechanical instead of manual for any bug first caught by `arch formal`.
- **Low blast radius.** Purely additive CLI surface (`--emit-cex-json`,
  `--replay-tb`); no change to existing `arch formal` semantics, exit
  codes, or the SV/SVA path consumed by EBMC and Verilator.

## Open questions (need validation before scoping real work)

1. Scope of "primary inputs" for hierarchical (multi-inst) designs — does
   the replay TB need to drive sub-instance-internal state directly, or is
   driving only top-level primary inputs always sufficient to reproduce the
   failure? (Should be sufficient by construction, since BMC only
   constrains primary inputs/free variables, but worth confirming against
   the hierarchical v1 flattening pre-pass.)
2. `arch formal`'s v1 scope is scalar types, single clock, no Vec/struct/
   enum (per `doc/COMPILER_STATUS.md`) — the replay emitter inherits that
   same scope for free, but should say so explicitly rather than silently
   producing a TB that only covers the supported subset.
3. Whether `--replay-tb` should also emit the matching `.arch` file's
   `arch sim --wave` invocation as a one-liner in a comment, so the full
   repro is copy-pasteable from the formal run's own output.

## Suggested next step

Not implementation yet — this is a research note. If a maintainer wants to
pick this up, the first concrete PR is narrow: refactor
`render_counterexample` to build the structured `Vec<CycleAssignment>`
first and derive the existing text output from it (behavior-preserving),
which unblocks both `--emit-cex-json` and `--replay-tb` as independent
follow-ups.
