# Proposal: `arch formal --wave-cex` — Counterexample Waveform Export

**Date:** 2026-06-10  
**Status:** Proposal — not yet filed as a GitHub issue  

---

## Problem

When `arch formal` reports a REFUTED property the developer gets a compact
text table printed to stderr:

```
[Assert] cnt_overflow             REFUTED  — at cycle 15
    Counterexample for `cnt_overflow` at cycle 15:

    cycle  rst  cnt_r
       13  0x1  0x0d
       14  0x1  0x0e
       15  0x1  0x0f
```

This is helpful for short traces but falls apart quickly when:

- the failing property involves many registers and input signals,
- the counterexample path is long (40+ cycles for a TLM request/response
  protocol property), or
- the developer already has a GTKWave/Surfer session open for the same design
  via `arch sim --wave`.

Every commercial formal tool (Jasper, Questa Formal, SymbiYosys) provides a
waveform of the counterexample witness as a first-class output.  ARCH already
has full waveform infrastructure (`arch sim --wave out.vcd`) but the formal
path has none, so the debug workflows for sim and formal are asymmetric.

---

## Proposed Enhancement

Add `--wave-cex <out.vcd>` to `arch formal`:

```bash
arch formal MyModule.arch --bound 30 --wave-cex cex.vcd
```

When any property is **REFUTED** (or a `cover` is **HIT**), the solver's
satisfying assignment is serialized as a VCD file spanning cycles 0 through
the failing/hitting cycle, including all design inputs, registers, and `let`
bindings already tracked by `arch formal`.

---

## Why This Matters

### 1 — The data is already in memory

`formal.rs::render_counterexample` (≈ line 2990) already populates a
`HashMap<String, u64>` keyed as `"signal_cycle"` from the solver's
`(get-model)` response.  That is exactly the time-sliced value sequence that
VCD needs.  No new solver query is required — only a small VCD serializer on
top of data that is already parsed.

### 2 — VCD format is small and self-contained

The subset required here (no X/Z, integer signals, fixed timescale, single
module scope) is roughly 150–200 lines of Rust, zero external crates.  The
format is well understood; `arch sim` already produces VCD (via the compiled
C++ simulation binary).  The formal-path VCD would be generated directly from
Rust without invoking a simulator.

### 3 — Consistent developer experience

Today a developer runs `arch sim --wave` to debug functional issues and
`arch formal` to verify properties.  These two workflows produce incompatible
output formats: one gives a VCD the developer can browse in GTKWave/Surfer,
the other gives stderr text.  Adding `--wave-cex` closes the gap: the same
waveform viewer works for both sim and formal debugging.

### 4 — Pairs with the formal road-map

**Issue #383** (formal rejects wire decls in lock-arbitration sub-modules)
blocks `arch formal` on any non-trivial thread design.  When that is resolved
and hierarchical formal v2 lands, counterexample traces will get long (one
FSM state per `wait until`).  Text dumps will be unreadable at that point;
waveform output is the right long-term interface.

---

## Implementation Sketch

### CLI (src/main.rs)

Add `--wave-cex <path>` alongside the existing `--emit-smt`:

```
arch formal F.arch [--bound N] [--emit-smt out.smt2] [--wave-cex out.vcd]
```

Multiple-property runs: the first REFUTED/HIT property wins for v1.
A `%s` template (`--wave-cex cex_%s.vcd`) for per-property files is a
follow-up.

### VCD writer (src/formal_vcd.rs — new, ≈ 150 lines)

```rust
pub fn write_formal_cex_vcd(
    path: &Path,
    top_name: &str,
    signals: &[(String, u32)],           // (name, bit_width)
    max_cycle: u32,
    assignments: &HashMap<String, u64>,  // "signal_cycle" → value
) -> std::io::Result<()>
```

**Header:**
```
$date 2026-06-10 $end
$timescale 1ns $end
$scope module <top_name> $end
$var wire <W> <id> <name> $end
...
$upscope $end
$enddefinitions $end
$dumpvars ... $end
```

**Signal values:** iterate `t` from 0 to `max_cycle`; emit `#<t×10>` timestamp,
then for each signal whose value changed from the previous cycle emit
`b<binary_val> <id>`.

Signal widths are already available from `FormalCtx.reg_widths` and
`FormalCtx.input_widths` (used for correct bit-vector encoding of the SMT
problem).

### Wire-up in the property-check loop (src/formal.rs)

After `render_counterexample` is called and the property is REFUTED/HIT,
if `--wave-cex` was supplied, call `write_formal_cex_vcd` with the same
`assignments` map before `render_report` prints to stderr.

### Tests

Extend `tests/formal/` with at least one REFUTED case that:
- asserts a `.vcd` file is emitted,
- checks that the VCD starts with `$date` and contains the expected signal
  names and the failing-cycle value, and
- verifies the file is parseable (a minimal 50-line Rust VCD reader in the
  test helper is sufficient; no need for an external dependency).

The existing `counter` 4-bit overflow and `guard-contract`
REFUTED integration tests are natural candidates.

---

## Scope for v1

| In scope | Deferred |
|---|---|
| Scalar signals (current flat-module formal scope) | Hierarchical formal (depends on hierarchical v2) |
| Single-clock designs | Multi-clock witness |
| First-REFUTED-property witness | Per-property files (`%s` template) |
| VCD format | FST format |
| `--wave-cex <path>` flag | Automatic waveform viewer launch |

---

## Related Work

- `arch sim --wave out.vcd` — simulation VCD (implemented, CLI row in
  COMPILER_STATUS.md).  Provides the model for the API surface.
- `arch formal --emit-smt out.smt2` — raw SMT output.  Different audience
  (SMT power users); `--wave-cex` targets the design-debug audience.
- `render_counterexample` in `src/formal.rs:2990` — current text-table
  renderer.  The VCD writer would be a parallel output path from the same
  `assignments` HashMap.
- **Issue #383** — formal rejects `WireDecl` in thread sub-modules.  Fixing
  that makes `arch formal` viable on lock-based designs, where
  `--wave-cex` is most needed.
- **Issue #244** — dual-backend (SV + arch sim) equivalence suite.  Formal
  could eventually participate: a PROVED bound on the equivalence assertion
  is a stronger guarantee than a trace match, and `--wave-cex` would surface
  any divergence formally discovered.
