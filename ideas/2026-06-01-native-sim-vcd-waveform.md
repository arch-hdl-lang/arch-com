# Enhancement: `arch sim --wave` — VCD Waveform Export from Native Sim

**Date:** 2026-06-01
**Status:** Proposal — new idea, not yet tracked as an issue
**Related issues:** #437 (native sim vs. Verilator disagreement), #244 (dual-backend equivalence suite)
**Related PRs:** #483 (thread-map HTML sidecar), harc-com #321 (`--check-backends` trace diff)

---

## Problem

`arch sim` (the native C++ sim path) has no waveform output. When a simulation
produces wrong results — or diverges from Verilator (issue #437) — the only
available debugging tools are:

1. **`--debug` text dump** — emits per-cycle signal values as a flat text file.
   Useful for automated comparison (`--check-backends`), but impossible to use
   with standard waveform viewers for interactive debugging.
2. **Custom `--tb` C++ print statements** — the testbench author has to manually
   instrument every signal they want to observe.
3. **Switch to `arch build` + Verilator** — Verilator does support `--trace`
   VCD/FST output, but then you're debugging the SV backend, not the native sim.

The Verilator path through HARC gained waveform support in harc-com #209. The
native `arch sim` path is the only remaining simulation mode with no standard
waveform output.

### Concrete pain: issue #437

Issue #437 reports that native `arch sim` disagrees with Verilator on the first
`weight_out` value for `SoftmaxEngine` — the native sim emits 0 where Verilator
emits 76592. Diagnosing this required writing a custom testbench to binary-search
the cycle where the outputs diverge. With `--wave`, the investigation would have
been: run both, open both `.vcd` files in GTKWave side-by-side, spot the
divergence in 5 minutes.

---

## Proposal: `arch sim --wave <path.vcd>`

Add a `--wave` flag to `arch sim` that writes a Value Change Dump (VCD) file
covering all top-level ports and module-scope registers.

### Usage

```sh
# Run sim with VCD output
arch sim MyModule.arch --tb tb_my_module.cpp --wave build/my_module.vcd

# Then open in GTKWave
gtkwave build/my_module.vcd
```

### Scope (v1)

- **All top-level ports** (`in`, `out`, `inout`) including `Clock`, `Reset`, and
  bus-flattened signals.
- **All module-scope registers** (`reg`, `pipe_reg`, thread FSM state registers
  `_t{i}_state`, thread counter registers `_t{i}_cnt`).
- **One level deep** — signals inside child instances are not included in v1.
  Hierarchical VCD (multiple `$scope` levels) is a natural follow-up.

Out of scope for v1:
- `let` bindings and `comb` intermediate wires (these are not persistent state;
  their values are visible via port observations anyway for most debugging).
- Child-instance internals.
- Binary FST format (VCD is simpler to generate; GTKWave reads both).

---

## Why VCD?

VCD (Value Change Dump) is the IEEE 1364 standard waveform format. It is:

- **Universally supported**: GTKWave, Surfer, Sigrok, WaveTrace (VS Code),
  and every major EDA waveform viewer reads VCD.
- **Simple to generate**: the format is ASCII text. A full VCD writer that
  handles `logic`, `wire`, and `reg` variables is ~150 LoC of C++.
- **Lossless for digital signals**: captures exact bit values at every change,
  which is exactly what's needed for native-sim vs. Verilator comparison.

FST (Fast Signal Trace) is a compressed binary format that GTKWave also supports.
It can be added later by writing VCD and running `vcd2fst` as a post-step,
or by implementing an FST writer directly.

---

## Implementation approach

Two touch points:

### 1. VCD writer in sim_codegen (`src/sim_codegen/mod.rs`)

The native sim generates a C++ model for each module. When `--wave` is enabled,
the codegen emits a small `VcdWriter` class inline in the generated header:

```cpp
// Generated when --wave is active
struct VcdWriter {
    FILE*    fp;
    uint64_t time;  // simulation time in ticks

    void header(const char* path, const char* timescale);
    void var(const char* type, int width, const char* id, const char* name);
    void enddefs();
    void timestamp(uint64_t t);
    void val_scalar(char id, bool v);
    void val_vector(char id, uint64_t v, int width);
    void close();
};
```

The generated `tick()` method is augmented with:

```cpp
void tick(bool clk_val, ...) {
    // existing tick logic
    _clk = clk_val;
    // ... settle loop ...

    // VCD dump: emit only changed signals
    if (_vcd) {
        _vcd->timestamp(_cycle++);
        if (_clk != _prev_clk) _vcd->val_scalar('a', _clk);
        if (_rst != _prev_rst) _vcd->val_scalar('b', _rst);
        if (_out_val != _prev_out_val)
            _vcd->val_vector('c', _out_val, OUT_WIDTH);
        // ... one if-changed per tracked signal ...
        _prev_clk = _clk; _prev_rst = _rst; /* ... */
    }
}
```

The `id` characters (`'a'`, `'b'`, `'c'`, …) are assigned sequentially at
header-emit time and stored in the generated `$var` declarations.

The change-only emission (tracking `_prev_*` values) keeps VCD file size small:
most signals change only on clock edges, so per-cycle overhead is one
`timestamp()` write plus one write per changed signal.

### 2. CLI flag in `src/main.rs`

```rust
/// Write VCD waveform to PATH (top-level ports and regs)
#[arg(long, value_name = "PATH")]
wave: Option<PathBuf>,
```

The `--wave` path is threaded through to `sim_codegen::emit_with_opts` and
controls whether the VCD writer infrastructure is emitted. If `None`,
no VCD code is generated (zero overhead).

### Signal coverage

| Signal category | v1 included? | Notes |
|---|---|---|
| `in`/`out` ports (scalar) | ✅ | |
| `in`/`out` ports (bus-flattened) | ✅ | Each flat bus signal is a separate `$var` |
| `Clock`, `Reset` ports | ✅ | Scalar logic |
| Module-scope `reg` (scalar) | ✅ | |
| Module-scope `reg` (Vec) | ✅ | Each element is a separate `$var wire [W-1:0]` |
| Thread FSM state regs (`_t{i}_state`) | ✅ | Makes thread transitions directly visible in waveform |
| Thread counter regs (`_t{i}_cnt`) | ✅ | |
| `pipe_reg` stage values | ✅ | |
| `let` / intermediate comb wires | ❌ deferred | Not persistent state; can add in Phase 2 |
| Child-instance internals | ❌ deferred | Phase 2 (hierarchical VCD) |
| Ram/CAM cell contents | ❌ deferred | Potentially very large; use `--check-uninit-ram` for debugging |

### Size estimate

- New VCD writer struct + emit: ~200 LoC in `sim_codegen/mod.rs` (inline in
  generated header, no separate file dependency).
- CLI flag + threading: ~20 LoC in `src/main.rs`.
- Prev-value shadow generation: ~30 LoC per signal type in codegen, reusing
  the existing `_prev_<inst>_<port>` shadow pattern from construct-port toggle
  coverage.
- Total: ~300 LoC across two files, zero new external dependencies.

---

## Integration with `--check-backends`

The `--check-backends` flag (harc-com #321) currently compares native sim and
Verilator traces as text diffs of `--debug` output. An alternative path for
v2: both backends emit VCD, and the comparison is a VCD-to-VCD trace diff
(per-signal, per-cycle). This would make the equivalence check:

1. More robust: per-signal diff, not line-by-line text.
2. More actionable: a divergence report names the exact signal and cycle.
3. Visualizable: both VCDs can be loaded side-by-side in GTKWave.

This is deferred to a follow-up; the current `--check-backends` design
(harc-com PR #321 + #323) is correct for its current use cases.

---

## Connection to issue #437

Issue #437 ("ARCH native sim SoftmaxEngine emits first valid weight as zero
while Verilator passes") is the canonical motivating case. With `--wave`:

```sh
# Native sim
arch sim SoftmaxEngine.arch --tb tb_softmax.cpp --wave native.vcd

# Verilator (via arch build + harc sim)
arch build SoftmaxEngine.arch -o SoftmaxEngine.sv
harc sim --sv SoftmaxEngine.sv tb_softmax.harc --wave verilator.vcd

# Open side-by-side
gtkwave native.vcd &
gtkwave verilator.vcd &
```

The waveform would immediately show whether:
- `weight_out` diverges from cycle 0 (output timing issue), or
- diverges after some state transition (registered-output misalignment), or
- the thread FSM `_t{i}_state` reaches state N in the wrong cycle.

All three hypotheses in issue #437's body ("registered output timing/alignment
issue for thread-lowered SoftmaxEngine outputs") could be confirmed or refuted
in minutes.

---

## Connection to thread-map HTML (#483)

PR #483 ships `--emit-thread-map`, an HTML sidecar that shows thread state
names, wait conditions, and (in PR #486) dead-skid hazard overlays. The
VCD waveform complements this: the HTML shows the *structure* of the thread,
the VCD waveform shows its *runtime behaviour*. Together they give:

- Static view: which state does each `wait until` live in?
- Dynamic view: when did the FSM enter/exit each state? What values drove
  the `wait` condition?

Including the thread FSM state registers (`_t{i}_state`) in the VCD output
(as listed in the signal coverage table above) directly enables this workflow.

---

## Acceptance criteria

- [ ] `arch sim MyModule.arch --tb tb.cpp --wave out.vcd` produces a valid
      VCD file that GTKWave and `vcd2fst` accept without warnings.
- [ ] The VCD file includes: all top-level ports, all module-scope scalar/Vec
      regs, all thread FSM state and counter regs.
- [ ] Without `--wave`, no VCD code is generated in the C++ model (size check).
- [ ] With `--wave`, correct value change recording: a signal that holds its
      value across 10 cycles appears only in the initial dump + final cycle
      (change-only emission).
- [ ] New integration test: `test_wave_flag_produces_valid_vcd` — runs `arch sim`
      on `tests/thread/wait_cycles.arch` with `--wave`, reads the output VCD,
      asserts the `$var` declarations, checks that the thread state register
      transitions match the expected FSM schedule.
- [ ] Issue #437 investigation: running the SoftmaxEngine repro under `--wave`
      should make the first-output divergence locatable in ≤ 5 minutes via
      GTKWave (not a CI test, but a human-verified debugging milestone).

---

## Why this, why now

The signal flow graph (PR #470) and dead-skid lint (PR #486) have dramatically
improved *static* visibility into ARCH designs. The thread-map HTML (PR #483)
added a *structural* view of thread FSMs. The one remaining gap in debugging
visibility is *dynamic* runtime behaviour of the native sim.

The Verilator path has waveforms. The pybind11 path has cocotb's trace support.
Native `arch sim` is the odd one out — and it's the path used for fast, direct
debug iterations (no Verilator compile) and for issue #437 root-cause analysis.

Adding VCD output closes that gap with minimal code, no language changes, and
no new dependencies.
