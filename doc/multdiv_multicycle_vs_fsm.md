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

4. **Critical-path delay** is broken out in the STA section below.
   The headline finding: with multicycle paths actually honored, the
   design's combinational max-Fmax is **264 MHz**, vs **87 MHz** for
   the FSM. Without multicycle honored, the multicycle design falls
   to **7.3 MHz** (single-cycle treatment of the divide).

## Static timing analysis (OpenSTA 3.1.0)

Post-synth gate-level netlists (Sky130 tt) were re-run through
OpenSTA 3.1.0 at `~/OpenSTA/build/sta`. Driver:
`examples/multdiv_multicycle_sta.{tcl,sh}`. Each design was swept to
find the smallest target clock period that still produces WNS = 0.

| Design                                  | Critical path (ns) | Fmax (MHz) | Critical-path endpoint                              |
|-----------------------------------------|--------------------|------------|-----------------------------------------------------|
| `ibex_multdiv_fast` (FSM)               | 11.372             |    ~87     | `op_b_i[1]` → `imd_val_d_o[32]` (combinational out) |
| `MultdivMulticycle`, multicycle honored | 1.447 (control)    |   ~264     | `start` → `op_is_mul` flop                          |
| `MultdivMulticycle`, NO multicycle      | 136.235            |     ~7.3   | `operand_b[17]` → `div_result[0]` flop              |

(Bracket period sweeps: FSM passes at 11.5 ns and fails at 11.4 ns;
multicycle-honored passes at 3.79 ns and fails at 3.78 ns;
multicycle-unhonored passes at 137 ns and fails at 136 ns.)

### Interpretation

The three rows are the three regimes a `multicycle` reg can be in:

- **Multicycle-honored** (row 2): the synth/STA tool reads the SDC,
  applies the 3-cycle setup window to `mul_result_reg[*]` and the
  36-cycle window to `div_result_reg[*]`, and the multiply / divide
  paths are no longer the binding constraint. The binding path
  becomes the small control logic (`start` → `op_is_mul`). Min period
  is bounded by `divide_delay / 36 = 136.235 / 36 ≈ 3.785 ns`. That
  is the regime `multicycle` is *intended* to deliver.
- **Multicycle-unhonored** (row 3): the SDC is dropped, every flop is
  treated as single-cycle, so the binding path is the full
  combinational divide. This is what yosys's own ABC mapping was
  doing in the area-only synthesis section above — the cell counts up
  there are slightly inflated because ABC tried to compress an
  unconstrained 136-ns path. Multicycle annotation does NOT help
  area-wise when downstream tools don't honor the SDC.
- **FSM** (row 1): explicit per-state scheduling caps the per-cycle
  delay at the level a single shared-adder MAC can sustain. Min
  period is ~11.4 ns, almost an order of magnitude faster than the
  multicycle design with the multicycle paths NOT honored.

### Honest framing

The multicycle approach wins on Fmax **if and only if** the downstream
tool actually retimes / accepts a multi-period path. With the SDC
respected, multicycle gives ~3x the FSM's Fmax in this example
(264 vs 87 MHz). With the SDC dropped, the FSM wins by an order of
magnitude.

### arch-com SDC parser issue (filed)

While running this experiment, OpenSTA 3.1.0 **rejected** the literal
SDC arch-com emits. The line

```sdc
set_multicycle_path 3 -setup -to {MultdivMulticycle/mul_result_reg[*]}
```

fails OpenSTA with `Error: stoi: no conversion`. The brace-pattern
shorthand for the `-to` object form is not part of the OpenSTA `-to`
grammar; OpenSTA expects `-to [get_pins ...]` or `-to [get_cells ...]`.
A SECOND issue is that yosys's flop-instance renaming (`_NNNN_`
anonymized) means even a corrected SDC syntax referencing
`mul_result_reg[*]` would not resolve against the post-synth netlist.

The numbers in row 2 of the STA table above come from a manual
translation of arch-com's SDC: a small TCL helper in
`examples/multdiv_multicycle_sta.tcl` walks all DFF D-pins, looks at
the cell's Q-net, and collects D-pins whose Q drives a net matching
the original signal name. It then issues
`set_multicycle_path 3 -setup -to <pin_list>` against that list. This
demonstrates the *value* of multicycle when honored, but it does
**not** validate arch-com's emitted SDC on OpenSTA — that SDC is
currently not consumable by OpenSTA 3.1.0 as-emitted.

Tracking item: file arch-com issue for SDC format compatibility with
OpenSTA's `-to` grammar (likely needs to switch to
`-to [get_pins ...]` form, or emit an alternative SDC variant for
open-source flows).

## Caveats

These numbers come with limits worth flagging:

- **arch-com SDC not directly consumable by OpenSTA**. The values in
  the "multicycle honored" row come from a manual TCL translation of
  the SDC; see the section above.

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
$ bash examples/multdiv_multicycle_synth.sh   # synth + emit gate netlists
$ bash examples/multdiv_multicycle_sta.sh     # STA + Fmax sweep (needs OpenSTA)
```

Outputs:
- `/tmp/multdiv-synth/multdiv_multicycle.sv` + `.sdc`
- `/tmp/multdiv-synth/ibex_multdiv_fast.sv`
- `/tmp/multdiv-synth/{multdiv_multicycle,ibex_multdiv_fast}.stat.log`
- `/tmp/multdiv-synth/{MultdivMulticycle,ibex_multdiv_fast}_synth.v`
  (post-synth gate-level netlists for STA)
- `/tmp/multdiv-synth/sta_*.log` (STA WNS/TNS + critical paths)

To re-run with a different PDK:
`LIB=/path/to/your.lib bash examples/multdiv_multicycle_synth.sh`.
Without any `.lib`, the script falls back to generic-cell synth and
reports unmapped cell counts only (no STA in that mode).

To use a different OpenSTA install:
`STA=/path/to/sta bash examples/multdiv_multicycle_sta.sh`.
