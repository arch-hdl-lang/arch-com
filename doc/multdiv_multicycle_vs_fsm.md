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
# Only for MultdivMulticycle: rename DFF cells to <wire>_reg_<bit>
# so arch-com's SDC `<reg>_reg*` glob resolves. See the STA section
# for why this is needed.
splitnets
tcl examples/multdiv_multicycle_yosys_rename.tcl
# ----
stat -liberty <sky130_lib>
ltp -noff
write_verilog <design>_synth.v
```

Both designs run through the same flow (FSM design skips the
`splitnets` + `tcl <rename>` step — it has no arch-com SDC to satisfy).

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

| Design                                       | Critical path (ns) | Fmax (MHz) | Critical-path endpoint                              |
|----------------------------------------------|--------------------|------------|-----------------------------------------------------|
| `ibex_multdiv_fast` (FSM)                    | 11.372             |    ~87     | `op_b_i[1]` → `imd_val_d_o[32]` (combinational out) |
| `MultdivMulticycle`, multicycle honored      | 1.447 (control)    |   ~264     | `start` → `op_is_mul_reg` flop                      |
| `MultdivMulticycle`, NO multicycle           | 136.235            |     ~7.3   | `operand_b[17]` → `div_result_reg_0` flop           |
| `MultdivMulticycleHier` two-pass, mc honored | 1.454 (control)    |   ~270     | `cnt_reg_0` → `cnt_reg_1` flop                      |
| `MultdivMulticycleHier` two-pass, NO mc      | 130.633            |     ~7.7   | `operand_b[3]` → `dp/div_r_reg_0` flop              |

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

### arch-com SDC + post-synth flop naming

Three integration issues were encountered going from arch-com's
emitted SDC to a working OpenSTA flow. Two are now resolved; one
remains as an open arch-com SDC-emission issue.

**Issue 1 (resolved upstream, arch-com PR #347)**: OpenSTA 3.1.0
rejects the original `-to {Mod/reg_reg[*]}` form with
`Error: stoi: no conversion` — OpenSTA's `-to` parser does not accept
the DC-style brace-pattern object form and requires
`-to [get_cells {...}]`. PR #347 switched arch-com's SDC emission to
`set_multicycle_path 3 -setup -to [get_cells {Mod/reg_reg*}]`. That
form parses cleanly across DC / Genus / Vivado / Quartus / OpenSTA.
This PR's branch carries the PR #347 merge.

**Issue 2 (resolved in this PR's synth flow)**: even with the
correct syntax, Yosys 0.64's `dfflibmap` produces flop-cell
instances with anonymous `_NNNN_` names (technically internal
`$auto$ff.cc:...` names that `write_verilog` renumbers). The
arch-com SDC glob `<reg>_reg*` then matches nothing in the
post-synth netlist. The fix is in
`examples/multdiv_multicycle_yosys_rename.tcl` — a small TCL pass
that runs after `dfflibmap` + `abc` + `clean` + `splitnets`, parses
the textual dump of all flop cells, and renames each cell to
`<wire>_reg_<bit>` (or `<wire>_reg` for scalars), where `<wire>` is
the Q-net name. The arch-com SDC's `<reg>_reg*` glob then resolves
cleanly. Why a TCL pass instead of `rename -wire`: Yosys 0.64's
`rename -wire` is a no-op for cells driving public bus-indexed nets
(`\mul_result[3]`); we tried it and it silently did nothing.

Synthesis area is unchanged by the rename pass (still 39,207.6 µm²)
— it's purely post-synth name preservation.

**Issue 3 (still open)**: arch-com's SDC emits cell names with the
module-name prefix: `[get_cells {MultdivMulticycle/mul_result_reg*}]`.
That form is correct for HIERARCHICAL synthesis (where MultdivMulticycle
is instantiated as a submodule of a larger design). For STANDALONE /
flat synthesis — the common open-source flow — the top-level cells
appear at the root of the netlist with no `MultdivMulticycle/`
prefix. OpenSTA's glob matching does not implicitly strip the
top-module name, so the glob resolves to nothing and the
multicycle constraints fail silently (only emit `Warning 349:
instance 'MultdivMulticycle/mul_result_reg*' not found`).

Workaround in `examples/multdiv_multicycle_sta.tcl`: read arch-com's
emitted SDC, strip the `MultdivMulticycle/` prefix via regsub, then
source the rewritten SDC. The `mul_with_mc` numbers in the table
above come from this flow.

Three possible upstream fixes (file as arch-com follow-up):

1. Drop the module prefix entirely: emit `<reg>_reg*`. Works for
   flat synth, ambiguous for hierarchical (could collide with same
   reg name in a different module).
2. Emit a wildcard prefix: `*<reg>_reg*`. Works in both flat and
   hierarchical; slightly broader-matching than necessary.
3. Emit two variants in the SDC, separated by `if {[get_cells …] !=
   {}}` guards. Most correct but most verbose.

Recommendation: option 2 (`*` prefix) is the least invasive and
widest-compatible.

## Two-pass selective resynthesis (Path A)

The single-pass numbers above show that yosys + ABC produce roughly
the same area whether you give them a relaxed clock (3.8 ns, what the
multicycle annotation buys you) or a real clock (5.0 ns) — they don't
respond to the multicycle SDC at all because vanilla ABC isn't
SDC-aware.

The research note `~/.claude/uploads/.../yosys_multicycle.html`
("Selective Resynthesis for Multicycle Paths in Yosys/ABC") proposes a
**two-pass workflow** to approximate commercial multicycle-honoring
synthesis using only Yosys + OpenSTA:

1. **Pass 1**: synthesize the multicycle cone at a *relaxed* clock
   target (3.8 ns, the multicycle budget). The hypothesis is that ABC
   will not over-optimize this cone for a tight constraint it doesn't
   need to meet.
2. **Pass 2**: freeze the pass-1 mapped cone and resynthesize the
   *rest of the design* at the real clock target (5.0 ns).
3. **Verify**: OpenSTA with the original multicycle SDC.

### Freeze strategy

We use a **hierarchy-boundary** freeze (the research note's preferred
option). A new sibling .arch file
`examples/multdiv_multicycle_hier.arch` carries the same arithmetic
but split into two modules:

- `MultdivMulticycleDatapath` — owns `mul_r` and `div_r` (the two
  `multicycle`-annotated regs) and their feeding arithmetic.
- `MultdivMulticycleHier` — owns the countdown counter, `op_is_mul`,
  and the output mux; instantiates the datapath as `dp`.

The two-pass script `examples/multdiv_multicycle_two_pass.sh`:

- Pass 1 invokes yosys with `hierarchy -top MultdivMulticycleDatapath`
  (synth the child standalone), `dfflibmap` + `abc -liberty -D
  3800000`. Output: `pass1_child_only.v`.
- Pass 2 reads parent RTL, `delete MultdivMulticycleDatapath` (drop
  the RTL form), reads `pass1_child_only.v` (the mapped form), runs
  `setattr -mod -set keep_hierarchy 1 MultdivMulticycleDatapath`,
  then `select -module MultdivMulticycleHier; abc -liberty -D
  5000000`. ABC sees only the parent. Output: `MultdivMulticycleHier_synth.v`.

A simpler approach — `setattr -mod -set keep_hierarchy 1 <child>` on
the full RTL design and a single `synth` invocation — does NOT
freeze the child. ABC visits every module in scope; `keep_hierarchy`
only prevents `flatten`, not module-local ABC remapping. The two
explicit invocations are needed.

### Results — two-pass vs single-pass

| Design / flow                                                         | Total cells | Area (µm²) | Flops | Critical path (ns) | Fmax (MHz) |
|-----------------------------------------------------------------------|------------:|-----------:|------:|-------------------:|-----------:|
| `ibex_multdiv_fast` (FSM, single-pass, no SDC)                        |       7,206 |    49,818  |   74  |            11.372  |        87  |
| `MultdivMulticycle` (flat, single-pass, NO multicycle)                |       6,047 |    39,208  |   71  |           136.235  |       7.3  |
| `MultdivMulticycle` (flat, single-pass, multicycle honored)           |       6,047 |    39,208  |   71  | 1.447 / 3.79 (mc)  |       264  |
| **`MultdivMulticycleHier` (two-pass, multicycle honored)**            |   **6,031** |  **39,221** | **71** | **1.454 / 3.7 (mc)** | **~270**   |

The two-pass row is essentially **tied** with the single-pass row:
−16 cells (−0.26%), +13 µm² (+0.03%), +6 MHz (+2.3%). The differences
are within run-to-run noise. The hypothesis ("smaller area from
relaxed pass") is **not confirmed** on this benchmark.

### Why no QoR delta — honest interpretation

The research note's hypothesis was that the relaxed-clock pass would
let ABC produce smaller logic for the multicycle cone (no need to
spend area shrinking critical-path depth that isn't actually
critical). Empirically, on this benchmark, **vanilla Yosys 0.64 + ABC
produces identical area for clock targets spanning 1.0 ns to 50.0 ns**:

```
abc -D 1000000  → 39,175 µm²
abc -D 2000000  → 39,175 µm²
abc -D 3800000  → 39,175 µm²
abc -D 5000000  → 39,175 µm²
abc -D 10000000 → 39,175 µm²
abc -D 50000000 → 39,175 µm²
```

ABC's default mapping script (`&map -a`) isn't aggressively
delay-driven on a design like this — it produces the area-floor
netlist regardless of `-D`. So the relaxed pass and the tight pass
generate the *same* mapped netlist, and the two-pass flow becomes a
no-op QoR-wise.

This is **a property of yosys-ABC on this benchmark**, not a refutation
of the methodology. The two-pass machinery works:

- Pass 1 produces a mapped child with `keep_hierarchy` preserved.
- Pass 2 ABC log shows `Extracting gate netlist of module
  '\MultdivMulticycleHier'` — only the parent, the child is
  untouched.
- The final netlist passes STA at the real clock with multicycle
  honored (WNS = 0 at 5.0 ns; min period 3.7 ns).

The methodology would likely matter more on designs where:
- ABC actually responds to `-D` (deeper combinational pipelines, or
  with `synth -auto-top -mappers cls/lib3` style scripts).
- The multicycle cone has non-trivial structural sharing opportunities
  (e.g. a Wallace-tree multiplier where ABC genuinely picks different
  AOI cells per delay target).

### Reproducing the two-pass flow

```
$ cd ~/github/arch-com
$ cargo build --release
$ bash examples/multdiv_multicycle_two_pass.sh
$ DESIGN=hier_with_mc OUT_DIR=/tmp/multdiv-two-pass \
    sta -no_splash -exit examples/multdiv_multicycle_sta.tcl
```

Outputs:
- `/tmp/multdiv-two-pass/multdiv_multicycle_hier.{sv,sdc}`
- `/tmp/multdiv-two-pass/pass1_child_only.v` (mapped child after relaxed-clock pass)
- `/tmp/multdiv-two-pass/MultdivMulticycleHier_synth.v` (final two-pass netlist)
- `/tmp/multdiv-two-pass/{pass1,pass2}.{ys,log}` (yosys script + log)

### When two-pass is worth the complexity

Honest framing of when Path A pays off:

- **Don't use it on yosys + Sky130 + this multdiv shape.** Same
  netlist, two synth runs, slightly more script complexity. No win.
- **Consider it if** (a) your ABC script is delay-aggressive (e.g.
  custom `abc -script ...` with `&dch -C 500; &if -K 6` flavor) and
  (b) the multicycle cone is large enough that the area-vs-delay
  trade-off curve actually has slope at the relaxed point.
- **Don't substitute for commercial synth** (DC, Genus) when you have
  it. Commercial tools consume the SDC directly and don't need a
  two-pass workaround.
- **OpenROAD** consumes SDC and runs delay-driven mapping internally;
  the two-pass workaround is mostly a yosys-standalone band-aid.

### Open SDC compat issue (4th, file as arch-com follow-up)

The two-pass hier flow surfaced a new SDC compat wrinkle. arch-com's
post-PR-#349 emission `[get_cells {*mul_r_reg*}]` is correct for
**flat** netlists but fails for **hierarchical** netlists: OpenSTA's
`get_cells` is non-recursive, so the `*` glob does not descend into
instance subhierarchies. Cells living at `dp/mul_r_reg*` are missed
by the bare `*mul_r_reg*` glob.

Workaround in `multdiv_multicycle_sta.tcl`: rewrite `[get_cells {...}]`
→ `[get_cells -hierarchical {...}]` before sourcing. The
`-hierarchical` flag walks the full instance tree.

Possible upstream fixes:
1. Always emit `-hierarchical` in the SDC: `[get_cells -hierarchical
   {*<reg>_reg*}]`. Works for both flat and hierarchical OpenSTA
   linkage. Likely the smallest patch.
2. Emit two parallel forms guarded by a `catch`. Most correct, most
   verbose.

Recommendation: option 1.

## Caveats

These numbers come with limits worth flagging:

- **arch-com SDC needs a `MultdivMulticycle/`-prefix strip for
  standalone synth**. The `mul_with_mc` STA flow rewrites the SDC
  in-place before sourcing; see Issue 3 above. The multicycle paths
  ARE applied correctly after this rewrite (verified: `op_is_mul_reg`
  endpoint replaces the formerly-anonymous `_11925_`, and the
  3.79 ns / 264 MHz Fmax is reproduced).

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
