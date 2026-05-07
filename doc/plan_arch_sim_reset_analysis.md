# Plan: `arch sim --reset-analysis`

## Context

Reset analysis is a common bring-up check: "if I hold reset high for N cycles, are **all** flops, latches, and primary outputs in a known (non-X) state?" Today, ARCH has no way to answer this short of writing a manual testbench and eyeballing waveforms. We already have `--check-uninit` that tracks per-signal `_<name>_vinit` shadow bits for `reset none` regs and `pipe_reg` chains — this is the foundation.

Goal: add `arch sim --reset-analysis <file.arch>` that:
1. Asserts reset, clocks the design repeatedly, deasserts reset, keeps clocking.
2. Reports the minimum cycle count at which **every** tracked signal has become initialized.
3. If some signals never initialize within a budget (default 1000 cycles), lists them — this identifies regs driven only by unreachable code paths, or regs that depend on stimulus the auto-harness can't provide.

## Non-goals (v1)
- **Stimulus generation** — auto-harness drives all primary inputs to 0 (or to declared defaults where possible). Regs that require specific input sequences to initialize will show up as "never init" — that's a valid analysis output, not a limitation.
- **Latch-only designs** — ARCH latches have enable signals, same story as regs.
- **Formal proof** — runtime simulation, not static analysis.
- **Multi-clock correctness** — v1 handles single-clock or picks the first clock port. Multi-clock gated stepping is v1.1.

---

## Output format

Successful analysis:
```
RESET ANALYSIS: CounterCheck
  Tracked signals: 7 regs, 2 pipe_reg stages, 3 primary outputs
  Reset polarity:  active-high (rst)
  Reset held:      5 cycles
  All initialized at cycle 8 (3 cycles after reset deassert)
  Exit code: 0
```

Partial success:
```
RESET ANALYSIS: CounterCheck
  Tracked signals: 7 regs, 2 pipe_reg stages, 3 primary outputs
  Reset held:      5 cycles
  Budget:          1000 cycles
  Reached cycle budget; 2 signals never initialized:
    - reg stream_cnt      (reset none; driver requires en=1 and valid_in=1)
    - out ready_out       (combinational driver reads stream_cnt)
  Exit code: 1
```

---

## Critical Files

| File | Role |
|------|------|
| `src/main.rs` | Add `--reset-analysis` CLI flag; route to new `run_reset_analysis()` function |
| `src/sim_codegen.rs` | Emit helper methods `is_all_init()`, `print_uninit()`, `reset_analysis()` into each module's C++ class |
| `src/codegen.rs` | No changes |
| `src/ast.rs` | No changes |

---

## Step 1 — CLI (`src/main.rs`)

Add to the `Sim` command:
```rust
/// Run reset analysis: hold reset, clock until all flops/outputs initialized
#[arg(long)]
reset_analysis: bool,
/// Max cycles to run reset analysis before giving up (default 1000)
#[arg(long, default_value_t = 1000)]
reset_budget: u64,
```

Implies `--check-uninit` (we need the `_vinit` tracking). When `reset_analysis` is set:
- Pass `check_uninit = true` to SimCodegen regardless of explicit flag
- Generate a special main harness `.cpp` stub instead of expecting a user testbench (if no `--tb` provided)
- If a `--tb` is also given, the testbench can call the generated methods directly

---

## Step 2 — `SimCodegen` field + builder

Add `reset_analysis: bool` field and `.reset_analysis(bool)` builder to `SimCodegen` (mirrors `--debug`).

---

## Step 3 — Collect "tracked signals" into a runtime-accessible list

Extend the existing `uninit_regs: HashSet<String>` (sim_codegen.rs line 2653) — it already covers reset-none regs + port-reg reset-none. For the analysis we need a **broader set**: signals that have `_vinit` tracking enabled.

Build a Vec at sim_codegen gen time:
```rust
struct TrackedSignal {
    name: String,             // C++ field name (e.g. "_stream_cnt" or "_let_ready")
    source: SignalSource,     // Reg, PipeRegStage, PortRegOut, PrimaryOutput
    display_name: String,     // User-facing "stream_cnt" / "ready_out"
}
```

Populate from:
- Each RegDecl with `RegReset::None` (already in `uninit_regs`)
- Each pipe_reg stage (already gets `_vinit`)
- Each port-reg output with `reset none`
- **New**: each primary output port — check at analysis time whether its combinational driver currently reads any uninit signal. For v1, approximate: an output is "init" iff all regs it depends on are init. Conservative fallback: treat ALL output ports as "init" once no `_vinit` bit is false. (v1 simplification — we report "all signals init" rather than per-output init.)

---

## Step 4 — Generate helper methods in each module's C++ class

Only when `self.reset_analysis` is true:

```cpp
// Header
bool is_all_init() const;           // true iff every _vinit bit is true
std::vector<const char*> uninit_names() const;  // list of display names not yet init
void print_uninit(const char* prefix = "  ") const;

// Optional auto-harness (see Step 5)
struct ResetAnalysisResult {
    bool all_init;              // true = success
    uint64_t settle_cycle;      // first cycle where all_init became true (or u64_max if never)
    uint64_t cycles_run;
    uint32_t tracked_count;
};
ResetAnalysisResult reset_analysis(uint64_t max_cycles = 1000);
```

Implementation sketch:
- `is_all_init()`: `return _a_vinit && _b_vinit && ... ;` (codegen OR over all tracked shadow bits)
- `uninit_names()`: per-shadow-bit branch appending to vector
- `reset_analysis()`: inline the full harness — see Step 5

---

## Step 5 — Generate the auto-harness method

The key decision from the reviewer: how to drive inputs. V1 choice: **all inputs zero except reset**. Simple, deterministic, matches the most common "bring-up smoke test" semantics.

```cpp
ResetAnalysisResult <Class>::reset_analysis(uint64_t max_cycles) {
    // Zero all primary inputs (clock, reset will be driven explicitly)
    <port_a> = 0; <port_b> = 0; ...  // skip clock ports

    // Phase 1: assert reset, hold for 5 cycles (enough for reset-driven regs)
    <rst_port> = <asserted_value>;   // 1 for active-high, 0 for active-low
    for (int i = 0; i < 5; i++) {
        <clk_port> = 0; eval();
        <clk_port> = 1; eval();
    }

    // Phase 2: deassert reset, clock until all init or budget exceeded
    <rst_port> = <deasserted_value>;
    for (uint64_t cy = 0; cy < max_cycles; cy++) {
        <clk_port> = 0; eval();
        <clk_port> = 1; eval();
        if (is_all_init()) {
            return {true, cy, cy, <tracked_count>};
        }
    }
    return {false, UINT64_MAX, max_cycles, <tracked_count>};
}
```

Uses `extract_reset_info` (already shared in `ast.rs`) to get reset name + polarity.
Uses first `Clock<*>` port for the clock. For multi-clock modules, v1 picks the first clock only and logs a warning; v1.1 can drive all clocks.

---

## Step 6 — Standalone harness binary

When `--reset-analysis` is passed and no `--tb` is given, generate a default `tb_reset_analysis.cpp`:

```cpp
#include "V<ModuleName>.h"
#include "verilated.h"
#include <cstdio>
int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    V<ModuleName> dut;
    auto result = dut.reset_analysis(<budget>);
    printf("RESET ANALYSIS: %s\n", "<ModuleName>");
    printf("  Tracked signals: %u\n", result.tracked_count);
    if (result.all_init) {
        printf("  All initialized at cycle %llu\n", (unsigned long long)result.settle_cycle);
        return 0;
    } else {
        printf("  Budget reached (%llu cycles). Still uninitialized:\n",
               (unsigned long long)result.cycles_run);
        dut.print_uninit("    - ");
        return 1;
    }
}
```

The `run_reset_analysis()` function in `main.rs` writes this stub next to the generated `.cpp`, compiles everything, and runs it. Exit code propagates (0 on success, non-zero on signals still uninit).

If the user DOES provide a `--tb`, the helper methods `is_all_init()` / `print_uninit()` / `reset_analysis()` are available for them to call directly — no auto-harness generated.

---

## Step 7 — Top-module selection

For a multi-module design (e.g. testbench file includes 5 modules), `reset_analysis` is called on the **root module** (the one not instantiated by any other). Reuse the root-detection logic from `--debug` (sim_codegen.rs ~line 78 where `debug_module_set` is computed).

Sub-instances are **not** recursively analyzed in v1 — a single report for the top module. Sub-instance vinit bits are already rolled up into the top's `is_all_init()` through the existing pipe_reg + inst output wiring.

---

## Step 8 — Edge cases

- **No reset port**: report error and exit. Can't analyze reset if there's no reset.
- **No clock port** (pure combinational module): `is_all_init()` returns true immediately; report "0 cycles (combinational)".
- **Async reset**: asserted reset immediately sets reset-driven regs; deassertion triggers propagation. Same harness works.
- **Multiple reset ports** (rare; dual-domain modules): use the first one in v1; warn user.
- **`reset none` regs that are never driven**: will show up as "never init" in the report — correct behavior, this is the value of the analysis.

---

## Verification

1. **Positive case**: run on `CounterCheck` (single reset-driven reg, simple). Expect:
   - `All initialized at cycle 0` (1 posedge of rst is enough).
2. **Pipe_reg chain**: module with 3-stage `pipe_reg` sourced from a reset-none reg that's driven by comb logic on input. With inputs held at 0, source may never init → report "never init".
3. **Reset-none reg driven by input**: module with `reg cnt: UInt<8>;` (no reset) updated as `cnt <= in_val` when `en = 1`. With `en = 0` (default stimulus), `cnt` never inits → report "never init: cnt".
4. **Full design**: run on `vending_machine` FSM. Expect state_r initialized after reset, all datapath regs init.
5. **Negative test**: design with `reg dead: UInt<8>;` that's only written in a state the FSM never enters from reset. Report should flag `dead` as never init.
6. **CLI integration**: `arch sim --reset-analysis counter_check.arch` runs without a user testbench, prints summary, exits 0.

---

## Interaction with existing flags

- `--reset-analysis` implies `--check-uninit` (sets it automatically)
- Can combine with `--debug` / `--debug+fsm` for verbose trace during analysis
- Cannot combine with `--wave` sensibly (no testbench to control trace lifecycle in auto mode) — emit a warning if both are set; waveform will only cover the auto-harness run

---

## Future (v1.1+)

- **Stimulus scripts**: `--reset-analysis --stim inputs.txt` to drive non-zero inputs for signals that need specific patterns
- **Sub-instance recursion**: run reset analysis on every module in the hierarchy
- **Multi-clock full support**: drive all clocks at their declared frequencies during analysis
- **Per-signal settle time**: report not just "all init at cycle N" but "reg X settles at cycle N1, reg Y at cycle N2, ..."
- **Integration with `--coverage`**: analysis run contributes to coverage counters
- **Formal mode**: static analysis (dataflow) to prove settle cycles without running sim
