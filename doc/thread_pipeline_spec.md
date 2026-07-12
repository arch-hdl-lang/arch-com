# Pipeline Wait-Stages: Variable-Latency Pipeline Stages

> **Status: Implemented (v0.40.0).** The `pipeline` construct now supports `wait until` and `do..until` inside stage `seq` blocks. The compiler generates a per-stage FSM, wires it into the stall chain, and handles flush. This replaces the earlier thread-based pipeline proposal.

---

## Core Idea

The existing `pipeline` construct handles fixed-latency stages with automatic valid propagation, stall backpressure, flush, and forwarding. However, some stages have **variable latency** — a cache stage that takes 1 cycle on a hit and many cycles on a miss, or a memory interface that blocks until a response arrives.

Previously, variable-latency behavior required either dropping to a manual `module` with explicit FSM, or using the `thread` construct. Now, `wait until` and `do..until` can appear directly inside a pipeline stage's `seq` block, and the compiler automatically:

1. Generates a per-stage FSM for multi-cycle operation
2. Wires the FSM's "busy" signal into the pipeline stall chain
3. Clears the FSM on flush

All existing pipeline features (valid propagation, stall backpressure, flush, forward, cross-stage references) continue to work unchanged.

---

## Syntax

### `wait until` in a pipeline stage

```arch
stage DataAccess
  reg data: UInt<32> reset rst => 0;
  seq on clk rising
    wait until mem_valid;       // stage stalls until mem_valid is true
    data <= mem_data;           // captures data when condition is met
  end seq
end stage DataAccess
```

### `do..until` in a pipeline stage

```arch
stage MemRead
  reg data: UInt<32> reset rst => 0;
  seq on clk rising
    do
      mem_req <= 1;             // held high while waiting
    until mem_valid;
    mem_req <= 0;
    data <= mem_rdata;
  end seq
end stage MemRead
```

---

## Full Example: Variable-Latency Memory Pipeline

```arch
pipeline CachedPipe
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port addr_in: in UInt<32>;
  port data_out: out UInt<32>;
  port mem_valid: in Bool;
  port mem_data: in UInt<32>;

  stage Fetch
    reg addr: UInt<32> reset rst => 0;
    seq on clk rising
      addr <= addr_in;
    end seq
  end stage Fetch

  stage DataAccess
    reg data: UInt<32> reset rst => 0;
    seq on clk rising
      wait until mem_valid;
      data <= mem_data;
    end seq
  end stage DataAccess

  stage Writeback
    reg result: UInt<32> reset rst => 0;
    seq on clk rising
      result <= DataAccess.data;
    end seq
    comb
      data_out = result;
    end comb
  end stage Writeback

  flush Fetch when branch_mispredict;
  flush DataAccess when branch_mispredict;
end pipeline CachedPipe
```

---

## Compiler Behavior

### Per-Stage FSM Generation

When a pipeline stage contains `wait until` or `do..until`, the compiler generates:

1. **FSM state register**: `logic [W-1:0] <prefix>_fsm_state` with minimum-width encoding
2. **FSM busy signal**: `logic <prefix>_fsm_busy = (<prefix>_fsm_state != '0)`
3. **FSM case logic**: inside the pipeline's `always_ff` block

The FSM has these states:
- **State 0 (idle)**: checks upstream valid, fast-paths if wait condition is already met. When a stage has two or more sequential waits, the fast path also runs the *next* wait group's pre-assigns (the assignments that sit between the first and second wait) — see [Fast Path](#fast-path) below.
- **State 1..N (waiting)**: loops each cycle checking the condition; advances when true

### Stall Chain Integration

The FSM busy signal is added to the stage's stall term:

```
stage_stall = user_stall_cond || fsm_busy || downstream_stall
```

When the FSM is active (state != 0), all upstream stages stall automatically through the existing backpressure mechanism. No manual stall wiring is needed.

### Flush Integration

When a `flush Stage when cond` directive targets a wait-stage, the compiler additionally resets the FSM state register:

```systemverilog
if (flush_cond) begin
  stage_valid_r <= 1'b0;
  stage_fsm_state <= '0;    // return to idle
end
```

### Fast Path

In state 0 (idle), the compiler checks if the *first* wait condition is *already* true when upstream data arrives. If so, the stage skips straight past state 1 (the dedicated wait state) on the very same clock edge:

- **Single wait** (`wait until cond; <trailing assigns>;`): the trailing assigns execute immediately and the stage returns to idle — the pipeline advances in a single cycle, identical to a non-wait stage.
- **Two or more waits** (`wait until cond1; <group-1 assigns>; wait until cond2; ...`): the fast path additionally runs the *next* wait group's pre-assigns (`<group-1 assigns>`, i.e. everything between the first and second wait) before advancing to state 2. This mirrors the state-1 → state-2 transition edge exactly, so the fast path (condition already true at dispatch) and the slow path (condition becomes true one or more cycles later) always execute the same assignments on the transition out of each wait — they only differ in which clock edge that transition happens to land on.

#### Same-cycle collapse

Because the fast path can run a whole wait group's worth of code on the *same* edge as the group before it, a register that is assigned **both** immediately before the first wait **and** immediately after it (i.e. in the group that runs right after the first wait resolves) can have both writes land on the same clock edge whenever the first wait's condition is already true at dispatch. When that happens the second write wins (last-assignment-wins, same as any other single-cycle multiple-assignment resolution) and the pre-wait value is **never observable** — not in a register read, not in a waveform — because it existed for zero cycles.

```arch
seq on clk rising
  x <= 1;            // pre-wait assign
  wait until go;
  x <= 2;             // post-wait assign — same register
end seq
```

If `go` is already `1` the cycle this stage dispatches, both `x <= 1` and `x <= 2` execute on that edge and `x` becomes `2` directly; `x == 1` is never observable. If `go` is `0` at dispatch, `x <= 1` fires on the dispatch edge and `x <= 2` fires later once `go` goes high — both values *are* observable in that case. The compiler emits a warning (`arch check` / `arch build` / `arch sim`) whenever it detects this shape, naming the register and the wait it straddles, so this is a compile-time-flagged design smell rather than a silent behavior change:

```
warning: register 'x' is assigned both before and after a `wait` in stage `Work`; when the wait condition is already true at dispatch both assignments execute in the same cycle and the last write wins — the pre-wait value is never observable on the fast path
```

The warning is advisory, not an error — some designs intentionally want "the wait's completion value wins, whether or not the wait actually blocked," in which case the two assignments are equivalent to a single conditional assignment and the warning can be safely ignored. If distinct behavior is required for the blocked vs. non-blocked case, use two different registers instead of reusing one across the wait boundary.

---

## Generated SystemVerilog (Illustrative)

For the `DataAccess` stage with `wait until mem_valid; data <= mem_data;`:

```systemverilog
// FSM registers
logic [0:0] dataaccess_fsm_state;
logic dataaccess_fsm_busy;
assign dataaccess_fsm_busy = (dataaccess_fsm_state != '0);

// Stall chain
assign dataaccess_stall = dataaccess_fsm_busy || writeback_stall;

// FSM logic (inside always_ff)
case (dataaccess_fsm_state)
  1'd0: begin                       // Idle
    if (fetch_valid_r) begin         // Upstream has data
      if (mem_valid) begin           // Fast path: condition already true
        dataaccess_data <= mem_data;
        dataaccess_valid_r <= fetch_valid_r;
      end else begin                 // Slow path: enter wait state
        dataaccess_fsm_state <= 1'd1;
      end
    end
  end
  1'd1: begin                       // Waiting for mem_valid
    if (mem_valid) begin
      dataaccess_data <= mem_data;   // Capture on condition met
      dataaccess_fsm_state <= '0;   // Return to idle
      dataaccess_valid_r <= 1'b1;
    end
  end
  default: dataaccess_fsm_state <= '0;
endcase
```

For a stage with **two** waits — `wait until go; flag <= true; do cnt <= cnt+1; until ready; done <= true;` — state 0's fast path additionally runs the second group's pre-assign (`flag <= true`) before jumping to state 2:

```systemverilog
case (work_fsm_state)
  2'd0: begin                       // Idle
    if (1'b1) begin
      if (go) begin                  // Fast path: first wait already true
        work_flag <= 1'b1;           // group-2 pre-assign — now emitted here too (#590)
        work_fsm_state <= 2'd2;      // jump straight to the do-until state
      end else begin                 // Slow path: enter wait state
        work_fsm_state <= 2'd1;
      end
    end
  end
  2'd1: begin                       // Waiting for go
    if (go) begin
      work_flag <= 1'b1;             // same group-2 pre-assign, slow-path edge
      work_fsm_state <= 2'd2;
    end
  end
  2'd2: begin                       // do..until ready
    work_cnt <= (work_cnt + 1'b1);
    if (ready) begin
      work_done <= 1'b1;
      work_fsm_state <= '0;
      work_valid_r <= 1'b1;
    end
  end
  default: work_fsm_state <= '0;
endcase
```

Both the state-0→state-2 edge and the state-1→state-2 edge now emit `work_flag <= 1'b1`, so `flag` is set whenever `wait until go` resolves — whether that happens on the very first cycle (fast path) or after one or more cycles of waiting (slow path).

---

## Restrictions

| Condition | Compiler behavior |
|---|---|
| `wait until` in module `seq` block (not pipeline) | Error: only valid in pipeline stage seq blocks |
| `do..until` in module `seq` block (not pipeline) | Error: only valid in pipeline stage seq blocks |
| `wait until` condition not `Bool` | Error: condition must be Bool |
| `wait` inside `if/else` branches | Not yet supported (same restriction as threads) |

---

## Comparison with `thread` Pipelines

The earlier design (see git history for the thread-based pipeline spec) proposed using cooperating `thread` blocks to describe pipelines. That approach required manual inter-stage handshake registers, explicit valid/ready flags, and did not integrate with the pipeline construct's existing stall chain, flush directives, or forwarding.

The current approach — extending the pipeline construct with `wait` — is simpler:

| Feature | Thread pipeline (old proposal) | Pipeline wait-stage (implemented) |
|---|---|---|
| Valid propagation | Manual `*_valid` registers | Automatic `*_valid_r` per stage |
| Stall backpressure | Manual `wait until downstream_ready` | Automatic stall chain |
| Flush | Manual `cancel` / `on signal: cancel` | `flush Stage when cond` — one-liner |
| Forward/bypass | Not addressed | `forward` directive (existing) |
| Cross-stage refs | Manual shared registers | `Stage.signal` syntax |
| Variable latency | Natural (each wait blocks) | `wait until` / `do..until` in stage |

---

## Prior Art

### HLS Tools (Vivado HLS / Vitis HLS, Catapult)

HLS tools synthesize pipelines from C/C++ with `#pragma HLS pipeline II=1`. The tool decides stage boundaries. ARCH's approach differs: **the designer defines stages** with explicit `stage` blocks, and the compiler handles variable latency within a stage via `wait until`. This gives RTL-engineer control over cycle boundaries while keeping code readable.

### Bluespec Pipeline Modules

Bluespec BSV uses FIFO-coupled rules for pipeline stages. Rules fire when input FIFOs are non-empty. ARCH's pipeline stages with `wait` are more explicit — the wait condition is visible in the source, and the stall chain is generated deterministically.

### Chisel Pipeline Utilities

Chisel's `Decoupled` wrappers provide ready/valid handshaking. ARCH's pipeline construct hides this boilerplate — `wait until` and the stall chain replace manual ready/valid wiring.

---

## Implementation Guide

### Files Modified

| File | Changes |
|---|---|
| `src/ast.rs` | Added `WaitUntil(Expr, Span)` and `DoUntil { body, cond, span }` to `Stmt` enum |
| `src/parser.rs` | Extended `parse_reg_stmt` to accept `wait until` and `do..until` |
| `src/codegen.rs` | `emit_pipeline`: detect wait-stages, generate FSM registers/busy/case logic, wire into stall/flush |
| `src/typecheck.rs` | Condition type checking (Bool); rejection outside pipeline stage seq blocks |
| `src/sim_codegen.rs` | Panic for now (pipeline wait-stages not yet supported in simulation) |

### Key codegen helpers

- `stage_has_wait(stage)` — detects if a stage contains wait/do-until
- `count_wait_fsm_states(stage)` — counts FSM states (1 idle + N waits)
- `emit_pipeline_wait_stage_ff(...)` — generates per-stage FSM case logic
