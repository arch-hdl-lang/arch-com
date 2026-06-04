# Enhancement: `--auto-pipeline-asserts` — compiler-generated SVA hazard contracts for `pipeline` constructs

**Date:** 2026-06-03
**Status:** Proposal — needs team discussion before implementation
**Related:** `--auto-thread-asserts` (shipped, `src/codegen/`), bounds-check SVA (shipped), div-by-zero SVA (shipped)

---

## Problem

The `pipeline` construct auto-generates stall propagation, flush masks, and forward muxes from
`stall when`, `flush`, and `forward` directives.  The generated logic is correct *in isolation*,
but there is currently **no machine-checkable contract that says so**.  If a later edit to the
pipeline body, a stall condition expression, or a downstream consumer introduces a hazard, the
first symptom is a functional simulation failure — not a compiler or formal diagnostic.

Pipeline hazard bugs are the #2 class of subtle RTL bugs after CDC, and they are particularly
expensive to root-cause because the failure (a wrong value read N cycles after a stall or flush)
is temporally distant from the cause (the stall/flush logic that failed to freeze/clear a stage).

The ARCH compiler already auto-generates SVA contracts for three other domains:

| Feature | Flag | What it asserts |
|---|---|---|
| Thread state transitions | `--auto-thread-asserts` | Each `wait until` / `wait N cycle` advances state correctly |
| Vec / bit-select bounds | always-on | Index stays within declared bounds |
| Divide-by-zero | always-on | Divisor is non-zero when expression evaluates |

The `pipeline` construct is the natural next target.

---

## Proposed flag

```
arch build   Pipe.arch --auto-pipeline-asserts    # emit SV with embedded SVA
arch formal  Pipe.arch --auto-pipeline-asserts    # also works (passes option to codegen)
arch sim     Pipe.arch --auto-pipeline-asserts    # checked by Verilator via --assert
```

Off by default (same as `--auto-thread-asserts`).  All emitted SVA is wrapped in
`// synopsys translate_off / on` so synthesis tools ignore it.  Reset polarity is inferred
from the module's `Reset<Kind, Polarity>` port — the same helper already used by the thread
and bounds-check SVA emitters.

---

## Property classes

### P1 — Stall-freeze: stalled stage holds its valid and data registers

When a stage is stalled (`{stage}_stall` is asserted), its registered state must not change on
the next clock edge.  A violation means the stall backpressure logic has a bug (a hole in the
stall chain, or a `<=` assign that bypasses the stall gate).

```systemverilog
// synopsys translate_off
_auto_pipe_fetch_stall_freeze_0: assert property (
    @(posedge clk) disable iff (rst)
    (fetch_stall |-> ##1 fetch_valid_r == $past(fetch_valid_r, 1))
) else $fatal(1, "PIPELINE VIOLATION: PipelineName._auto_pipe_fetch_stall_freeze_0");

_auto_pipe_fetch_stall_freeze_1: assert property (
    @(posedge clk) disable iff (rst)
    (fetch_stall |-> ##1 fetch_data == $past(fetch_data, 1))
) else $fatal(1, "PIPELINE VIOLATION: PipelineName._auto_pipe_fetch_stall_freeze_1");
// synopsys translate_on
```

One property per emitted stage register plus one for `{stage}_valid_r`.  Counter `_N` increments
globally across all pipeline stages so names remain unique per module.

**Scope:** stages that have at least one stall condition (either local `stall when` or
propagated backpressure).  Stall-free stages have a trivially-true stall property; skip them
to avoid vacuous assertions.

### P2 — Flush-clears-valid: flushed stage loses its valid token

When a `flush` directive fires, the targeted stage's `valid_r` register must be zero on the
following clock edge.  A violation means the flush did not write `valid_r <= 0` — either the
write is in the wrong clock domain, gated incorrectly, or the flush expression is wrong.

```systemverilog
// synopsys translate_off
_auto_pipe_decode_flush_clears_0: assert property (
    @(posedge clk) disable iff (rst)
    (flush_cond |=> !decode_valid_r)
) else $fatal(1, "PIPELINE VIOLATION: PipelineName._auto_pipe_decode_flush_clears_0");
// synopsys translate_on
```

One property per `flush` directive targeting a named stage.

**Interaction with stall.** If a flush fires simultaneously with a stall on the same stage,
SV pipeline convention is flush-wins (the `valid_r <= 0` override in `always_ff` is unconditional
under flush).  The compiler already generates the flush-wins override; the property correctly
validates it.

### P3 — Forward consistency: forwarded value equals the source-stage register

When a `forward X.reg from Y when cond` directive evaluates its condition as true, the
mux output that `X` reads must equal `Y.reg` at that cycle.  A violation means the forward
condition is wrong, or the mux wiring has a logic error.

```systemverilog
// synopsys translate_off
_auto_pipe_fwd_data_0: assert property (
    @(posedge clk) disable iff (rst)
    (execute_fwd_en |-> decode_data_out == execute_result_r)
) else $fatal(1, "PIPELINE VIOLATION: PipelineName._auto_pipe_fwd_data_0");
// synopsys translate_on
```

`execute_fwd_en` is the forwarding condition expression, `execute_result_r` is the source
stage's register, `decode_data_out` is the forwarding mux output consumed by the destination
stage.  These three are all named signals in the generated SV; the codegen already knows them
from the `ForwardDirective` AST node.

One property per `forward` directive.

---

## Rationale: why these three and not others

| Candidate | Included? | Reason |
|---|---|---|
| Stall-freeze | ✅ | Direct SV coverage of the backpressure chain; catches the most common hazard class |
| Flush-clears-valid | ✅ | Directly validates the flush-wins override; catches wrong flush expressions |
| Forward consistency | ✅ | Validates the mux; catches forwarding-condition bugs |
| Token liveness ("valid entering stage N eventually exits") | ❌ | Requires `s_eventually` (unbounded liveness) — out of scope per COMPILER_STATUS.md |
| No-duplication ("at most one valid token per stage") | ❌ | Trivially true for linear pipelines; vacuous unless multi-issue is supported |

---

## Implementation approach

The change is **entirely within `src/codegen/pipeline.rs`**.  No new pass, no new AST node.

### 1. Add the flag to CLI and codegen opts

```rust
// src/main.rs — alongside auto_thread_asserts in Build/Formal/Sim variants
#[arg(long, default_value_t = false)]
auto_pipeline_asserts: bool,
```

Pass it through `BuildOpts` / `FormalOpts` to `Codegen::new(opts)`.  `Codegen` already carries
`opts.auto_thread_asserts`; add `opts.auto_pipeline_asserts` as a sibling field.

### 2. Emit assertions at the end of `emit_pipeline`

After the `endmodule` line (or before it, following the thread-assert convention), call a new
`emit_pipeline_asserts(&p, ...)` helper if `self.opts.auto_pipeline_asserts`.

```rust
fn emit_pipeline_asserts(&mut self, p: &PipelineDecl,
    stage_names: &[&str], stage_regs: &[Vec<(String, String, String)>],
    clk_name: &str, rst_name: &str, is_low: bool) {

    let disable_iff = if is_low {
        format!("disable iff (!{})", rst_name)
    } else {
        format!("disable iff ({})", rst_name)
    };

    let mut n = 0usize;   // global counter for unique SVA names

    // P1: stall-freeze
    for (si, stage) in p.stages.iter().enumerate() {
        if !stage_has_stall(stage, p) { continue; }
        let prefix = stage_names[si].to_lowercase();
        let stall = format!("{prefix}_stall");
        // valid_r
        self.emit_sva_assert(
            &format!("_auto_pipe_{prefix}_stall_freeze_{n}"),
            &format!("@(posedge {clk_name}) {disable_iff} ({stall} |-> ##1 {prefix}_valid_r == $past({prefix}_valid_r, 1))"),
            &format!("PIPELINE VIOLATION: {}._auto_pipe_{prefix}_stall_freeze_{n}", p.name.name),
        );
        n += 1;
        // data regs
        for (reg_name, _, _) in &stage_regs[si] {
            let sig = format!("{prefix}_{reg_name}");
            self.emit_sva_assert(
                &format!("_auto_pipe_{prefix}_stall_freeze_{n}"),
                &format!("@(posedge {clk_name}) {disable_iff} ({stall} |-> ##1 {sig} == $past({sig}, 1))"),
                &format!("PIPELINE VIOLATION: {}._auto_pipe_{prefix}_stall_freeze_{n}", p.name.name),
            );
            n += 1;
        }
    }

    // P2: flush-clears-valid
    for flush in &p.flush_directives {
        let stage_prefix = flush.stage.name.to_lowercase();
        let cond = self.emit_pipeline_expr_str(&flush.condition, stage_names, stage_regs, &Default::default());
        self.emit_sva_assert(
            &format!("_auto_pipe_{stage_prefix}_flush_clears_{n}"),
            &format!("@(posedge {clk_name}) {disable_iff} ({cond} |=> !{stage_prefix}_valid_r)"),
            &format!("PIPELINE VIOLATION: {}._auto_pipe_{stage_prefix}_flush_clears_{n}", p.name.name),
        );
        n += 1;
    }

    // P3: forward consistency
    for fwd in &p.forward_directives {
        let cond = self.emit_pipeline_expr_str(&fwd.condition, stage_names, stage_regs, &Default::default());
        let dest = self.emit_pipeline_expr_str(&fwd.dest,      stage_names, stage_regs, &Default::default());
        let src  = self.emit_pipeline_expr_str(&fwd.source,    stage_names, stage_regs, &Default::default());
        self.emit_sva_assert(
            &format!("_auto_pipe_fwd_{n}"),
            &format!("@(posedge {clk_name}) {disable_iff} ({cond} |-> {dest} == {src})"),
            &format!("PIPELINE VIOLATION: {}._auto_pipe_fwd_{n}", p.name.name),
        );
        n += 1;
    }
}
```

`emit_sva_assert` is a 4-line helper (already used by thread/bounds/div0 emitters) that emits
the `translate_off` wrapper, the `assert property` line, the `else $fatal`, and `translate_on`.

### 3. Tests

| Test | Expected |
|---|---|
| Stall-freeze: correct design, stall asserted → data held | Verilator `--assert` passes |
| Stall-freeze: mutated design, `<=` escapes stall gate → data changes under stall | `$fatal(1, "PIPELINE VIOLATION: ...")` |
| Flush-clears-valid: flush fires → valid clears next cycle | Verilator `--assert` passes |
| Flush-clears-valid: missing `valid_r <= 0` in flush body | `$fatal(1, ...)` |
| Forward consistency: fwd_en true → dest == src | Verilator `--assert` passes |
| Forward consistency: wrong fwd condition (misses the case) | Property vacuously true (no false fire) — documented limitation |
| EBMC: unconstrained stall input → P1 PROVED (stall freezes by design) | PROVED up to bound 5 |
| No stall/flush/forward pipeline | `--auto-pipeline-asserts` emits no SVA, no compilation error |

---

## What this does not do

- Does not change emitted SV for designs that don't use `--auto-pipeline-asserts`.
- Does not assert liveness (`s_eventually` is out of scope).
- Does not instrument `sim_codegen/pipeline.rs` — assertion firing is caught by Verilator
  `--assert` on the emitted SV, not by the native simulator directly (same limitation as
  `--auto-thread-asserts` today).
- Does not cover `wait`-stage FSMs within a pipeline (those are already covered by
  `--auto-thread-asserts` on the underlying thread lowering).  The interaction is additive:
  run both flags to get thread-state contracts and pipeline-hazard contracts simultaneously.

---

## Precedent in the compiler

This follows the exact pattern of all other auto-SVA emitters:

| Emitter | Source | Properties |
|---|---|---|
| Bounds check | `codegen/mod.rs:emit_auto_bounds_sva` | `idx < N` for Vec/bit-select |
| Div-by-zero | `codegen/mod.rs:emit_auto_div0_sva` | `divisor != 0` |
| Thread contracts | `lower_threads.rs` | `wait until` → next state; `wait N cycle` stay/done |
| **Pipeline hazards** | `codegen/pipeline.rs` (proposed) | Stall-freeze, flush-clears-valid, forward consistency |

The pattern is: compiler lowers a high-level construct → emits SVA that captures the *intent*
the lowered comb+seq blob no longer expresses in prose → downstream tools (Verilator `--assert`,
EBMC) can prove or disprove it.

---

## Why this matters

A pipeline with five stages and two forwarding paths requires **manually** writing ~15 SVA
properties to cover the hazard matrix.  That work is almost never done in practice, so the
first indication of a stall or forward bug is a simulation failure that requires waveform
archaeology to diagnose.

With `--auto-pipeline-asserts`, the compiler generates those 15 properties for free from
the same `stall when`, `flush`, and `forward` directives the designer already wrote.  Running
`arch build Pipe.arch --auto-pipeline-asserts | verilator --binary --assert` is a one-liner
that turns every pipeline definition into a self-checking module.  The cost is one extra flag.
