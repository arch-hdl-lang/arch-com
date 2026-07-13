# Plan: `reg <name>: <T> guard <valid_sig>;` syntax

## Context

ARCH flags reads of reset-none regs as uninitialized (`--check-uninit`) and the planned `--reset-analysis` feature lists them as "never init" false positives. But a very common hardware idiom is **valid-guarded data**: a register is intentionally left uninitialized as long as a companion `valid` signal tells consumers "the data is meaningful now". This saves area/power on wide data paths (AXI data, FIFO storage, cache lines).

```arch
reg data:  UInt<32>;                      // no reset — intentional, area-optimized
reg valid: Bool reset rst => false;       // reset to "not valid"

seq on clk rising
  if en
    data  <= din;
    valid <= true;
  end if
end seq
```

Consumers read `data` only when `valid == true`. This is safe, but the compiler doesn't know it.

**Solution**: new syntax `reg data: UInt<32> guard valid_r;` that:
1. Documents intent in the source
2. Suppresses *spurious* uninit-read warnings (noise) while still catching *real* bugs
3. Excludes `data` from reset-analysis "must init" (when guard is `false`)
4. Emits **runtime checks** in `arch sim`:
   - **Producer bug**: `valid_r` asserts but `data` was never written (the data bus is live but carries X)
   - **Consumer bug**: someone reads `data` before it's ever been written (regardless of guard state)
5. Emits **SVA assertions** for formal/Verilator: `valid_r |-> _data_written` — formally provable with EBMC

## Non-goals (v1)
- Multi-signal guards (e.g. `guard valid_a && valid_b`) — v1 takes a single `Ident` only. Users can combine via a `let` binding.
- Auto-inference from control-flow — explicit annotation only.
- Guards on `wire` or `let` declarations — regs only in v1.
- **Check C** (per-read-site "read data while valid=0" detection) — requires per-read-site instrumentation in every comb/seq block. Deferred to v1.1. The v1 Check A (producer bug) catches the highest-value case.

---

## Syntax

```arch
reg NAME: TYPE guard SIG;                                       // no reset, guarded — the common case
reg NAME: TYPE guard SIG init VALUE;                           // with init
reg NAME: TYPE guard SIG reset RST => VAL;                     // with reset (unusual but legal)
reg NAME: TYPE guard SIG init VALUE reset RST => VAL;          // fully specified

port reg NAME: out TYPE guard SIG;                              // port reg form
port reg NAME: out TYPE guard SIG init V;
```

`guard` clause comes **right after TYPE**, before any `init` / `reset` clauses. Rationale: `guard` is a structural qualifier about the reg (like a type modifier) — it sits next to the type where the reader looks. `init` / `reset` are value-setting clauses that naturally pair together and go at the end.

Common case stays tight:
```arch
reg axi_data: UInt<512> guard axi_valid;
reg axi_user: UInt<8>   guard axi_valid;
reg axi_id:   UInt<4>   guard axi_valid;
```

`SIG` must be:
- An identifier (not an expression)
- Resolvable in scope: ports, regs, wires, lets
- Of type `Bool` (typecheck error otherwise)

---

## Critical Files

| File | Role |
|------|------|
| `src/lexer.rs` | Add `guard` as a keyword (check no collision today — audit says none) |
| `src/ast.rs` | Add `guard: Option<Ident>` to `RegDecl` and `PortRegInfo` |
| `src/parser.rs` | Parse trailing `guard <ident>` clause in `parse_reg_decl` and port-reg parsing |
| `src/typecheck.rs` | Validate guard signal exists + is Bool |
| `src/sim_codegen.rs` | Exclude guard-protected regs from `--check-uninit` warning set |
| `src/codegen.rs` | Optional SVA emission: `cover property (guard \|-> data_was_written)` — v1.1 |

---

## Step 1 — Lexer (`src/lexer.rs`)

Add `guard` to the keyword token list. Audit confirmed no existing collision. If `Guard` as a TokenKind is not yet present, add:
```rust
#[token("guard")] Guard,
```
and matching Display impl.

---

## Step 2 — AST (`src/ast.rs`)

Extend `RegDecl`:
```rust
pub struct RegDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    pub init: Option<Expr>,
    pub reset: RegReset,
    pub guard: Option<Ident>,      // NEW: valid-signal guard
    pub span: Span,
}
```

Extend `PortRegInfo`:
```rust
pub struct PortRegInfo {
    pub init: Option<Expr>,
    pub reset: RegReset,
    pub guard: Option<Ident>,      // NEW
}
```

Update all `RegDecl { ... }` / `PortRegInfo { ... }` constructors in the codebase to pass `guard: None` by default (should be ~10 sites, mostly in parser.rs and elaborate.rs).

---

## Step 3 — Parser (`src/parser.rs`)

In `parse_reg_decl()` (lines 662–698), **immediately after TYPE is parsed**, before parsing `init` / `reset`:
```rust
let guard = if self.peek_keyword("guard") {
    self.advance();
    Some(self.expect_ident()?)
} else {
    None
};
// then proceed to parse `init` and `reset` as today
```
Thread `guard` into the RegDecl constructor.

Do the same for port-reg parsing (wherever `PortRegInfo` is built). Audit didn't pinpoint the exact parser site for port-regs — locate via `grep -n 'PortRegInfo' src/parser.rs` during impl.

Also update `parse_reg_default_decl()` (lines 627–646) if we want `reg default: guard X;` to set a module-wide default guard (probably out of scope for v1; skip).

---

## Step 4 — Typecheck (`src/typecheck.rs`)

In the per-reg check loop (wherever RegDecl fields are validated — similar pattern to reset signal validation at lines 883–931), add:
```rust
if let Some(ref guard_id) = reg.guard {
    // Look up in ports + regs + wires + lets for this module
    let ty = lookup_signal_type(guard_id, m, ...);
    match ty {
        Some(Ty::Bool) => {} // OK
        Some(other) => errors.push(CompileError::general(
            &format!("guard signal `{}` must be Bool, found {:?}", guard_id.name, other),
            guard_id.span,
        )),
        None => errors.push(CompileError::undefined(&guard_id.name, guard_id.span)),
    }
}
```

Same check for `PortRegInfo.guard`.

---

## Step 5 — Runtime checks in `arch sim` (`src/sim_codegen.rs`)

Guard-protected regs **keep** their `_<name>_vinit` shadow bit — we still need to know whether the reg has ever been written. The behavior changes:

1. **Generic `--check-uninit` warning**: silenced for guarded regs. The blanket read-anywhere warning was too noisy for the data-valid pattern; the guard annotation is the user's contract saying "trust my consumers".

2. **Check A — producer bug (always emitted when `--check-uninit` is on)**:
   In `eval_posedge()`, after register commit, for each guard-protected reg:
   ```cpp
   // Warn once per module per signal
   if (<guard_sig> && !_<data>_vinit) {
       static bool _w_<data>_guard = false;
       if (!_w_<data>_guard) {
           _w_<data>_guard = true;
           fprintf(stderr, "GUARD VIOLATION: <ModName>.<data> — "
                           "<guard_sig>=1 but <data> was never written\n");
       }
   }
   ```
   This catches the very real bug where a producer sets `valid <= true` but forgets to write `data`, or the write path is broken. The guard contract is violated from the producer side.

3. **Check B — consumer bug (always emitted when `--check-uninit` is on)**:
   Keep the existing per-read uninit warning for guarded regs too, but only fire when the read happens before the first write (i.e. `_<data>_vinit == false`). After the first write, reads are fine regardless of guard state. This catches consumers who bypass the guard contract entirely.

4. **The blanket "unguarded read" warning** (existing `--check-uninit` behavior for non-guarded regs) is **unchanged** for regs WITHOUT a guard clause.

### AST + emit changes

At line 2650, keep `uninit_regs` as-is (guard regs stay in the set for Check B). Add a new parallel map:
```rust
// reg name → guard signal name, for guarded regs only
let guarded_regs: HashMap<String, String> = /* collect from RegDecl.guard and PortRegInfo.guard */;
```

In the `eval_posedge()` emitter, after the existing reg-commit block, iterate `guarded_regs` and emit Check A printf blocks.

### Warning vs abort

Both checks default to **warn once** (stderr). A future `--strict` flag could upgrade them to `abort()`, but not in v1.

---

## Step 6 — `--reset-analysis` integration

When the reset-analysis plan (`doc/plan_arch_sim_reset_analysis.md`) is implemented, it already consults `uninit_regs`. Since guard-protected regs are excluded from that set, the analysis automatically treats them as "init" — no extra work needed.

Follow-up refinement (v1.1): in the reset-analysis report, mention guard-protected regs separately:
```
RESET ANALYSIS: AxiDataPath
  Tracked signals:      12 regs, 2 pipe_reg stages
  Guard-protected:      3 (data, sg_meta, burst_buf)
  All non-guarded initialized at cycle 7
```

---

## Step 7 — SVA assertion for guard contract (`src/codegen.rs`)

Emit the formal contract as a concurrent SVA property, inside `translate_off/on`:

```systemverilog
// Shadow: tracks whether `data` has ever been written (sim-only; formal uses auxiliary state)
logic _data_written;
always_ff @(posedge clk) begin
  if (rst) _data_written <= 0;
  else if (_data_written_this_cycle) _data_written <= 1;
end

_data_guard_contract: assert property (@(posedge clk) valid_r |-> _data_written)
  else $fatal(1, "GUARD VIOLATION: <ModName>.data — valid_r=1 but data never written");
```

### Getting `_data_written_this_cycle`

Options:
- **Conservative fallback (v1)**: assume the reg is written whenever the driver block executes. Set `_data_written <= 1` whenever the enclosing `seq on clk` block body runs without taking the reset branch. Technically "written ever" not "written this cycle", but it correctly implements the "has been written at least once" semantic we need.
- Emit at each `seq` block that targets `data`:
  ```systemverilog
  always_ff @(posedge clk) begin
    if (rst) _data_written <= 0;
    else if (<reg_commit_fires>) _data_written <= 1;
  end
  ```
  Where `<reg_commit_fires>` is inferred from the driver (the `if en ... data <= din` pattern becomes `else if (en) _data_written <= 1;`).

For v1, start with the "fires whenever the seq block commits" approximation: any cycle where the seq block's reset branch is not taken, set `_data_written <= 1`. This may over-approximate (`_data_written` goes to 1 earlier than strictly necessary) but the direction is safe: it only leads to missing some bug detections, never false alarms.

### Scope in v1
- Shadow `_data_written` reg emitted for each guard-annotated reg.
- SVA assertion `<guard> |-> _data_written` emitted per guarded reg.
- Wrapped in `// synopsys translate_off` / `on`.
- Verilator `--assert` and EBMC `--bound N --reset "rst==1"` both pick it up automatically.

### Formal verification payoff
EBMC can **prove** the contract holds across all input sequences. Example expected output:
```
[AxiSlave._data_guard_contract] always valid_r |-> _data_written: PROVED up to bound 20
```
or
```
[AxiSlave._data_guard_contract] always valid_r |-> _data_written: REFUTED
Counterexample: at cycle 3, valid_r=1, _data_written=0
```

---

## Verification (v1)

### Parser + typecheck
1. **Positive parse**: `reg data: UInt<32> guard valid_r;` in a module where `valid_r: Bool` exists — parses, type-checks clean.
2. **Undefined guard**: `reg data: UInt<32> guard nonexistent;` — error "undefined signal `nonexistent`" with span.
3. **Wrong type guard**: `reg data: UInt<32> guard counter_r;` where `counter_r: UInt<8>` — error "guard signal `counter_r` must be Bool".
4. **Combined**: `reg data: UInt<32> guard valid_r init 0 reset rst => 0;` — all four clauses in canonical order.
5. **Port reg**: `port reg out_data: out UInt<32> guard out_valid;` — same treatment.
6. **Regression**: all 267 CVDP tests still pass.

### Runtime checks (sim)
7. **Check A positive**: testbench asserts `valid_r = 1` without ever writing `data` → sim prints `GUARD VIOLATION: ... valid_r=1 but data was never written`, once.
8. **Check A no-op**: testbench writes `data` then asserts `valid_r` → no warning.
9. **Check B positive**: consumer reads `data` before first write (regardless of `valid_r`) → sim prints uninit-read warning.
10. **Noise check**: non-buggy testbench that correctly writes before asserting valid → zero warnings (proves blanket suppression works correctly; Check A doesn't false-alarm).

### SVA (formal + Verilator)
11. **Generated SV inspection**: the generated `.sv` file for a guarded reg contains:
    - `logic _data_written;` shadow reg
    - `always_ff` block that sets it to 1 when the seq block commits
    - `_data_guard_contract: assert property (@(posedge clk) valid_r |-> _data_written) else $fatal(...);`
    - All wrapped in `// synopsys translate_off` / `on`.
12. **Verilator `--assert`**: buggy testbench (valid asserts early) → `$fatal` fires with "GUARD VIOLATION" message.
13. **EBMC formal**: `ebmc --top Mod --bound 20 --reset "rst==1" file.sv` — correct design shows `_data_guard_contract: PROVED`. Broken design (missing data write) shows `REFUTED` with counterexample.
14. **Yosys synthesis**: `.sv` synthesizes clean; no `$check` cells (contract excluded via `translate_off`).

---

## Interaction with existing flags

- `--check-uninit`: suppresses the generic read-anywhere warning on guarded regs BUT still emits Check A (producer bug) and Check B (first-read-before-write).
- `--reset-analysis` (when landed): excludes guard-protected regs from must-init set (they're "conditionally init"; the SVA contract is the formal check).
- `--debug` / `--debug+fsm`: no interaction (guard is static).
- `--coverage` (when landed): the auto-generated `_data_guard_contract` assertion counts as a cover/assert property and gets counted in the coverage report automatically.

---

## Future (v1.1+)

- **Check C** (per-read-site): when consumer reads `data` while `valid_r == 0`, warn. Requires instrumenting every read site in comb/seq block emission.
- **Composite guards**: `guard (valid_a and valid_b)` accepting an expression
- **Guard on wires/lets**: `let stale_data: UInt<32> = ... guard valid;` for combinational data paths
- **Module-level default**: `reg default: ... guard default_valid;` applied when no per-reg override
- **Reset-analysis report**: show guard-protected regs in a separate "safely uninit" section
- **Precise `_data_written`**: tighten the "writes whenever seq block commits" approximation to fire only when the actual `data <= ...` assignment path is taken
- **`--strict` flag**: upgrade Check A / Check B warnings to abort
- **Formal library**: package common guard patterns (AXI valid/ready, FIFO level, etc.) as reusable `guard` macros
