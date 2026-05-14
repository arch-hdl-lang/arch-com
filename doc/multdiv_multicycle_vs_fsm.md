# Multdiv multicycle vs. FSM — synthesis comparison

This is a head-to-head case study comparing two HDL strategies for a
multi-cycle arithmetic unit:

1. **`MultdivMulticycle`** (`examples/multdiv_multicycle.arch`) — uses
   the new `multicycle N` reg annotation (arch-com PR #345, Phase A) to
   declare per-reg multi-cycle slack. The arithmetic is written as a
   single flat expression; an emitted `.sdc` file carries
   `set_multicycle_path` constraints for the heavy result regs so a
   downstream synth tool can retime or share resources across the
   declared N-cycle window.
2. **`ibex_multdiv_fast`** (upstream Ibex / `arch-ibex`) — uses an
   explicit `thread` + shared-adder FSM. The thread sequencer schedules
   16x16 partial-product MACs across multiple cycles for the multiply
   and runs a restoring-divide step per cycle for the divide. The
   adder is reused cycle-to-cycle by hand.

The goal is to characterize the trade-off without a commercial synth
flow: under Yosys + ABC + Sky130 PDK, what does each design lower to,
and where does the SDC actually buy you something downstream?

## Functional interface delta

This comparison is over a subset-equivalent workload. The two designs
are NOT pin-compatible:

| Feature                       | `MultdivMulticycle` | `ibex_multdiv_fast` |
|-------------------------------|---------------------|---------------------|
| 32x32 -> low 32 multiply      | yes                 | yes (`MULL`)        |
| 32/32 unsigned quotient       | yes                 | yes (`DIVU`)        |
| MULH (high 32 of 64 product)  | no                  | yes                 |
| MULHSU / MULHU                | no                  | yes                 |
| Signed divide / remainder     | no                  | yes (`DIV`, `REM`)  |
| Divide-by-zero handling       | no                  | yes                 |
| MD_OP encoding for op select  | `is_mul` (1 bit)    | 2-bit `operator_i`  |

`MultdivMulticycle` is intentionally minimal — its purpose is to
exercise the `multicycle` annotation, not to fully replace
`ibex_multdiv_fast`. The FSM is therefore doing more work in the
comparison. The cell-count delta below should be read with that in
mind: the FSM is a more featureful module, but the multicycle design's
single multiply expression already approaches the FSM's total area.

## What the `.sdc` emission looks like

The `arch build` invocation that compiles `multdiv_multicycle.arch`
emits a `.sdc` companion alongside the `.sv`:

```
$ target/release/arch build examples/multdiv_multicycle.arch -o /tmp/multdiv-synth/multdiv_multicycle.sv
Wrote /tmp/multdiv-synth/multdiv_multicycle.sv
Wrote /tmp/multdiv-synth/multdiv_multicycle.sdc
```

The SDC file:

```sdc
# Auto-generated SDC constraints from arch HDL multicycle reg annotations.
# ...
# Module MultdivMulticycle: multicycle reg mul_result
set_multicycle_path 3 -setup -to {MultdivMulticycle/mul_result_reg[*]}
set_multicycle_path 2 -hold -to {MultdivMulticycle/mul_result_reg[*]}

# Module MultdivMulticycle: multicycle reg div_result
set_multicycle_path 36 -setup -to {MultdivMulticycle/div_result_reg[*]}
set_multicycle_path 35 -hold -to {MultdivMulticycle/div_result_reg[*]}
```

A commercial synth tool (Synopsys DC, Cadence Genus) consuming this
SDC will know the heavy paths into `mul_result_reg` and `div_result_reg`
are allowed 3 and 36 clock periods respectively, and may retime
arithmetic across them.

## Synthesis flow

Per-tool, what was used to produce the numbers below:

- **Compiler**: `target/release/arch` from arch-com
  `feat/multdiv-multicycle-example` (off `94edbd5 PR #345`).
- **SV translator**: `sv2v 0.0.13`. Yosys 0.64's built-in SV parser
  rejects several idioms the arch-com emitter uses (cast syntax,
  function-call-then-bit-select); `sv2v` converts to plain V2005.
- **Function-call indexing workaround**: `sv2v` preserves
  `fn(...)[15:0]`, which yosys 0.64's V2005 parser also rejects. A
  small Python helper (`examples/multdiv_multicycle_lift_funcidx.py`)
  lifts each call into a module-scope wire so the bit-select applies
  to a name. Purely a tooling workaround for yosys's parser; the
  semantics are unchanged. This is **only** needed for the FSM
  design — the multicycle design does not emit that pattern.
- **Synthesis**: Yosys 0.64 (built-in V2005 + ABC frontend).
- **Technology**: Sky130 PDK,
  `sky130_fd_sc_hd__tt_025C_1v80.lib` (typical-typical corner,
  25 °C, 1.80 V).
- **Driver script**: `examples/multdiv_multicycle_synth.sh`. Reruns
  end-to-end. Outputs land under `/tmp/multdiv-synth/` (override
  `OUT_DIR=...`).

The yosys script per design:

```
read_verilog <design>.v
hierarchy -check -top <top>
proc; opt; fsm; opt; memory; opt; techmap; opt
dfflibmap -liberty <sky130_lib>
abc -liberty <sky130_lib>
clean
stat -liberty <sky130_lib>
ltp -noff
```

Both designs run through the same flow.

## Results

| Design                           | Top-level cells | Chip area (µm²) | Flop count    | Seq. area (µm²) |
|----------------------------------|-----------------|-----------------|---------------|-----------------|
| `MultdivMulticycle` (this PR)    |          6,047  |       39,207.6  |            71 |          1,421  |
| `ibex_multdiv_fast` (FSM total)  |          7,206  |       49,817.8  |            74 |          1,877  |
| - `ibex_multdiv_fast` (top wrap) |             15  |           85.1  |             0 |              0  |
| - `_threads` submodule           |          7,191  |       49,732.7  |            74 |          1,877  |

(Raw `stat -liberty` output: `/tmp/multdiv-synth/<design>.stat.log`.)

### Observations

1. **Flop count is essentially the same** (71 vs. 74). Both designs
   reach roughly the same number of stored bits. The `multicycle`
   annotation does NOT reduce flop count — it only annotates the
   timing arc.

2. **Cell-count delta is ~16% in favor of the multicycle design** —
   despite the FSM doing more work (MULH, signed mode, REM, divide-by-
   zero handling). This is because yosys + ABC turn `*` and `/` into
   straightforward combinational shifters-and-adders without trying to
   retime across multiple cycles. The FSM's hand-written shared-adder
   loop produces both the structural overhead of the state machine and
   the partial-product MAC plumbing. Net: a single flat multiply
   expression is competitive with a hand-written FSM, before any SDC
   honoring.

3. **Sequential area is ~24% smaller for multicycle**. Same flop count
   (-3 nets), but the FSM uses asynchronous-reset flops
   (`dfrtp_1`, area 25.02) while the multicycle design's sync-reset
   regs lower to plain `dfxtp_1` (area 20.02). This is a property of
   the `Reset<Sync>` declaration in the example, not of `multicycle`.

4. **Critical-path delay is not reported** here. Yosys 0.64's `ltp`
   pass after `abc -liberty` gave degenerate length-0/1 paths, which
   means it lost the cell-level depth after the abc mapping. A real
   ns-level critical path would require OpenSTA or a commercial STA
   tool. See caveats.

## Caveats

These numbers come with limits worth flagging:

- **No SDC consumption**. Yosys + ABC do not read SDC, so the
  `set_multicycle_path` constraints emitted alongside
  `multdiv_multicycle.sv` are ignored by this flow. The synth depth
  shown is therefore the WORST-CASE (single-cycle treatment of the
  multicycle paths). A commercial flow that honors the SDC could
  retime or resource-share within the declared 3-cycle / 36-cycle
  windows, which would change both area and critical-path delay. The
  case for `multicycle` as a synthesis-deliverable mechanism therefore
  rests on the commercial tool flow — these yosys numbers are a lower
  bound on how the design fares without any multicycle-aware retiming.

- **No ns-level timing**. Critical path delay (ns / ps) would require
  OpenSTA or a commercial STA tool. The `ltp -noff` pass returned
  degenerate length-0/1 paths post-abc-mapping. Cell count + chip
  area are the comparable signals here.

- **FSM is more featureful**. ibex_multdiv_fast handles MULH, signed
  mode, REM, and divide-by-zero. The multicycle design covers only
  unsigned MULL / DIVU. A fair feature-matched comparison would
  enlarge the multicycle module; the gap above is the LOWER BOUND on
  the cell-count delta (FSM only gets bigger if you subtract its extra
  features).

- **Yosys-specific tooling workarounds**. The FSM SV had to go through
  `sv2v` + a python helper to lift `fn(...)[bits]` into named wires
  before yosys 0.64 would accept it. These are tooling artifacts, not
  semantic changes. The multicycle SV needed only `sv2v`.

## When to prefer which

The two patterns are not strictly substitutes:

- **Use `multicycle N` reg** when (a) the arithmetic is naturally a
  single expression, (b) the budget is multiple cycles, and (c) a
  commercial synth tool that consumes SDC is in the flow. The HDL is
  flat, readable, and lets synth pick the structure.

- **Use a `thread` FSM** when (a) the design has visible inter-cycle
  state (partial products to accumulate, restoring-divide steps to
  iterate), (b) the shared resource sharing is the value (e.g. one
  adder used 8 cycles in a row), or (c) the target flow is yosys-only
  and SDC won't be honored. The FSM gives the designer explicit
  control over scheduling.

For first-class MULH / DIV / REM with signed semantics — the FSM's
explicit state per operator is genuinely useful and a `multicycle`
single-expression port would not improve on it. Where `multicycle` is
likely most valuable is for ALU-like blocks with a single primary
operation budgeted N cycles, where the spec contract is "valid_o
high at cycle N" rather than "step-by-step state machine".

## Reproducing

```
$ cd ~/github/arch-com
$ cargo build --release
$ cd ~/github/arch-ibex && make build      # produces build/ibex_multdiv_fast.sv
$ cd ~/github/arch-com
$ bash examples/multdiv_multicycle_synth.sh
```

Outputs:
- `/tmp/multdiv-synth/multdiv_multicycle.sv` + `.sdc`
- `/tmp/multdiv-synth/ibex_multdiv_fast.sv`
- `/tmp/multdiv-synth/{multdiv_multicycle,ibex_multdiv_fast}.stat.log`

To re-run with a different PDK: `LIB=/path/to/your.lib bash
examples/multdiv_multicycle_synth.sh`. Without any `.lib`, the script
falls back to generic-cell synth and reports unmapped cell counts only.
