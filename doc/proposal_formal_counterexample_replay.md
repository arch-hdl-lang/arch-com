# Proposal: `arch formal --emit-cex-vcd` / `--replay-tb` — turn a BMC counterexample into a waveform or a runnable `arch sim` reproduction

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

- `src/formal.rs::parse_model` (~line 3222) parses the solver's raw
  `(get-model)` response into `assignments: HashMap<String, u64>`, keyed
  `"{signal}_{cycle}"` for the reset, every primary input, and every
  register — **the full trace, cycle 0 through the bound**, not just the
  cycles near the failure. This comes directly from the solver's model, not
  from re-simulating anything.
- `render_counterexample` (~line 3429) is handed that same full map but
  only *prints* a 2-cycle window before the failing cycle
  (`start = cycle.saturating_sub(2)`) as a human-readable text block:
  `"Counterexample for `<prop>` at cycle N:"` followed by a small table.
- `CheckResult.counterexample: Option<String>` (~line 49) carries that text
  on every `Refuted`/`Hit` result.
- The CLI driver (~line 3590) prints it to stderr, indented, one line per
  register-at-cycle.

So the underlying data is already complete — the full per-cycle value of
every signal the solver assigned — but the only consumer built on top of it
is a truncated text dump to stderr. Once printed, the rest of that trace is
a dead end: there's no VCD, no JSON, nothing a downstream tool can read.
A human who wants more than the 2-cycle snippet, or who wants to see the
failure evolve in a waveform viewer, or who wants to step through it in
`arch sim --debug+fsm`, has to manually transcribe values back into a
`.cpp` or `--pybind --test` testbench. For any property with more than a
handful of registers or more than a few cycles to the failing point, that
transcription is tedious and error-prone — exactly the kind of task that
should be automated given the data already exists inside the compiler.

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

Two independent options, roughly ordered by effort. Neither requires new
solver capability — the SMT solver itself has no notion of time-stepped
waveforms or of "replay"; it only ever returns a flat variable→value model.
All of the temporal structure below is something `arch formal` already
imposes when it unrolls the design into per-cycle variables, and both
options just present the model data the compiler *already has* in a more
useful shape.

### Option A (small, do first): `--emit-cex-vcd out.vcd`

`assignments` already contains the complete cycle-0..bound trace for every
reset/input/register (see Context above) — `render_counterexample` just
throws most of it away by windowing to 2 cycles. A first PR can walk that
same map from cycle 0 to the failing cycle and emit standard VCD directly:
`$var` declarations for the reset/inputs/regs already listed in
`render_counterexample`'s `names` vector, then one `#N` + value-change block
per cycle. No C++ codegen, no invoking `arch sim`, no dependency on the
design's runtime semantics matching between cycles — it's a direct,
mechanical translation of data the compiler already extracted from the
solver. Immediately viewable in GTKWave/Surfer, same as `arch sim --wave`
output. This is the cheapest way to close the "I can only see 2 cycles of
text" gap.

### Option B (larger, more capable): `--replay-tb out.cpp`

For interactive debugging beyond a static waveform — stepping with
`arch sim --debug+fsm`, adding scoped `log()` calls, or exploring inputs
beyond what the solver's bound covered — regenerate the counterexample as
an actual testbench:

1. **Structured counterexample.** Populate a typed `Vec<CycleAssignment>` —
   `{cycle: u32, port_or_reg: String, value: BigUint}` — from the same
   `assignments` map Option A reads. Shared groundwork for both options.
2. **`--emit-cex-json out.json`**: dump the structured trace (property
   name, failing cycle, and the full per-cycle *primary-input* assignment —
   not just registers, since registers are derived from inputs and a replay
   TB needs to *drive* inputs, not registers) as JSON.
3. **`arch formal --replay-tb out.cpp`** (or a small separate tool reading
   the JSON): emit a minimal `arch sim`-compatible C++ testbench that
   drives exactly the primary inputs the solver used, cycle by cycle, via
   the generated `dut.set_<port>(v)` setters, then stops. This reuses
   existing scaffolding — it's structurally the same shape as any other
   `arch sim --tb tb.cpp` testbench, just auto-generated instead of
   hand-written. Running the emitted TB under `arch sim --debug+fsm --wave
   cex.vcd` gives the same waveform as Option A, plus an FSM trace and the
   ability to keep stepping past the bound.
4. Optionally, a `--pybind`-flavored emitter (`--replay-tb out.py`) for
   users already in the cocotb-shim flow (`arch sim --pybind --test`,
   documented in `doc/arch_sim_cocotb.md`), since driving inputs via
   `.value =` writes is even more mechanical than the C++ setter form.

## Why this is worth doing over other backlog items

- **Reuses data that already exists.** No new formal-engine capability is
  needed for either option — the per-cycle model values are already
  extracted and sitting in `assignments`; this proposal is a
  structured-output + (for Option B) codegen problem, not a BMC problem.
- **Compounds with existing strengths.** The project's `--debug+fsm`
  instrumentation and coverage-annotated sim are already best-in-class
  debugging tools per `CLAUDE.md`; Option B is the shortest path to
  pointing them at formal's output instead of only at hand-written stimuli.
  Both options directly serve the "Fix-PR lifecycle" discipline in
  `CLAUDE.md` ("re-reproduce before you fix") by making the reproduction
  step mechanical instead of manual for any bug first caught by
  `arch formal`.
- **Low blast radius.** Purely additive CLI surface (`--emit-cex-vcd`,
  `--emit-cex-json`, `--replay-tb`); no change to existing `arch formal`
  semantics, exit codes, or the SV/SVA path consumed by EBMC and Verilator.

## Open questions (need validation before scoping real work)

1. For Option A, whether `assignments` truly holds dense per-cycle coverage
   for every named signal at every cycle 0..bound in all cases (e.g. when a
   variable is unconstrained at some cycle and the solver omits/underspecifies
   it) — `render_counterexample` defaults missing keys to `0` via
   `.unwrap_or(0)`, which is fine for a text hint but would silently draw a
   flat/wrong line in a VCD; worth confirming against a design with a
   genuinely "don't care" signal.
2. Scope of "primary inputs" for hierarchical (multi-inst) designs (Option
   B) — does the replay TB need to drive sub-instance-internal state
   directly, or is driving only top-level primary inputs always sufficient
   to reproduce the failure? (Should be sufficient by construction, since
   BMC only constrains primary inputs/free variables, but worth confirming
   against the hierarchical v1 flattening pre-pass.)
3. `arch formal`'s v1 scope is scalar types, single clock, no Vec/struct/
   enum (per `doc/COMPILER_STATUS.md`) — both emitters inherit that same
   scope for free, but should say so explicitly rather than silently
   producing an incomplete VCD/TB.
4. Whether `--replay-tb` should also emit the matching `.arch` file's
   `arch sim --wave` invocation as a one-liner in a comment, so the full
   repro is copy-pasteable from the formal run's own output.

## Suggested next step

Not implementation yet — this is a research note. If a maintainer wants to
pick this up, Option A (`--emit-cex-vcd`) is the smallest concrete PR: walk
the existing `assignments` map from cycle 0 to the failing cycle and emit
VCD directly, no new data plumbing required. Option B builds on the same
`assignments` map but needs the `Vec<CycleAssignment>` refactor first and
is best scoped as an independent follow-up.
