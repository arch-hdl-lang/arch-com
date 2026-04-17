# `arch sim` — cocotb Integration Guide

`arch sim --pybind --test` runs Python testbenches against an ARCH design using a cocotb-compatible API, with no Verilator, iverilog, or VPI in the loop. The generated C++ model is wrapped with pybind11, and a thin scheduler (`arch_cocotb`) drives it tick-by-tick from an asyncio event loop.

The intent is "write the same testbench you would for real cocotb" — same decorators, triggers, signal handles, and coroutine patterns — while trading VPI fidelity for deterministic timing and faster iteration.

---

## Quick start

```sh
arch sim --pybind --test test_mymodule.py MyModule.arch
```

Under the hood this:

1. Runs the ARCH → C++ codegen (`SimCodegen::generate_pybind`), producing `VMyModule_pybind.cpp` next to the normal `VMyModule.cpp`.
2. Compiles the pybind11 wrapper with `python3 -m pybind11 --includes` plus `g++`, emitting `VMyModule_pybind.<ext>` into the build directory (default `arch_sim_build/`).
3. Spawns `python3 test_mymodule.py` with `PYTHONPATH` set to find three packages:
   - `python/cocotb_shim/cocotb/` — so `import cocotb` works unchanged
   - `python/arch_cocotb/` — the real implementation
   - `arch_sim_build/` — the compiled pybind11 `.so`

The test file drives the DUT exactly like a real cocotb TB. Example:

```python
import cocotb
from cocotb.triggers import RisingEdge, Timer
from cocotb.clock import Clock

@cocotb.test()
async def test_reset(dut):
    cocotb.start_soon(Clock(dut.clk, 10, units='ns').start())

    dut.rst_n.value = 0
    dut.enable.value = 0
    await RisingEdge(dut.clk)
    dut.rst_n.value = 1

    for _ in range(5):
        await RisingEdge(dut.clk)
    assert int(dut.count.value) == 0
```

---

## The `cocotb` shim

`python/cocotb_shim/cocotb/` is a drop-in stand-in that re-exports the real implementation from `arch_cocotb`. The shim exposes:

| `cocotb` symbol | Maps to |
|---|---|
| `cocotb.test` (decorator) | `arch_cocotb.decorators.test` |
| `cocotb.start_soon(coro)` | `arch_cocotb.decorators.start_soon` |
| `cocotb.start(coro)` | `arch_cocotb.decorators.start` |
| `cocotb.utils.get_sim_time(units)` | `arch_cocotb.utils.get_sim_time` |
| `cocotb.triggers.RisingEdge` | `arch_cocotb.triggers.RisingEdge` |
| `cocotb.triggers.FallingEdge` | `arch_cocotb.triggers.FallingEdge` |
| `cocotb.triggers.Timer` | `arch_cocotb.triggers.Timer` |
| `cocotb.triggers.ClockCycles` | `arch_cocotb.triggers.ClockCycles` |
| `cocotb.clock.Clock` | `arch_cocotb.triggers.Clock` |
| `cocotb.types.Logic` | `cocotb_shim.cocotb.types.Logic` (stub) |

Because of the shim, you can point the same test file at either `arch sim` or a real cocotb runner without edits, as long as the test only uses the intersection of the APIs listed above.

---

## API reference (what's actually implemented)

### `@cocotb.test()`

Registers an async function as a test. Supports `timeout_time=`, `timeout_unit=`, `expect_error=`, `expect_fail=`, and `skip=` for signature compatibility, but only `skip=True` affects behavior in v1.

```python
@cocotb.test()
async def test_foo(dut):
    ...
```

### Triggers

| Trigger | Behavior |
|---|---|
| `RisingEdge(signal)` | Suspends until the next 0→1 transition of `signal`. |
| `FallingEdge(signal)` | Suspends until the next 1→0 transition of `signal`. |
| `Timer(duration, units='ns')` | Suspends for the given duration. Accepts `ps`/`ns`/`us`/`ms`/`s` (and `unit=` alias). |
| `ClockCycles(signal, n, rising=True)` | Suspends for N edges of `signal`. |

Triggers are awaitables. They yield a future to the scheduler and resume when the corresponding event fires.

### `Clock(signal, period, units='ns')`

```python
cocotb.start_soon(Clock(dut.clk, 10, units='ns').start())
```

`.start(start_high=False)` returns a coroutine that sets the signal to 0 (or 1) and toggles it forever at `period/2` intervals. The coroutine is infinite — always schedule it with `start_soon`.

### `start_soon(coro)` / `start(coro)`

`start_soon(coro)` schedules `coro` to run concurrently with the current test and returns immediately. `start(coro)` is an async version that returns a task handle. Both back onto `asyncio.create_task` on the underlying event loop.

### Signal access (`dut.<name>`)

Signal handles are instances of `ArchSignal`. They expose a `.value` property:

```python
dut.enable.value = 1                     # write
current = int(dut.count.value)           # read (via __int__)
signed  = dut.result.value.to_signed()   # sign-extended
unsigned = dut.addr.value.to_unsigned()
```

The returned value object (`ArchSignalValue`) supports `int()`, `bool()`, equality against `int`, `__str__`/`__repr__` (decimal), and hashing. It does **not** implement the full `LogicArray` API — in particular, there is no per-bit `X`/`Z` state (see "Deltas from cocotb" below).

Parameters are exposed as read-only signal handles with `.value.to_unsigned()`:

```python
WIDTH = int(dut.WIDTH.value)             # param, write raises AttributeError
```

Iteration (`for sig in dut`) yields all registered non-parameter signal handles, useful for bulk setup.

### `cocotb.utils.get_sim_time('ns')`

Returns the current sim time as a float. `'ps'`, `'ns'` (default), `'us'`, `'ms'` all work.

---

## Scheduler model

The scheduler is a 1-tick-at-a-time loop sitting on top of `asyncio`:

```
while test_task not done:
    await asyncio.sleep(0)            # let ready coroutines run
    if test_task done: break
    time_ns += 1
    model.eval()                      # atomic: comb → posedge → comb
    resolve Timer waiters whose deadline ≤ time_ns
    check every watched signal for rising/falling edges and wake waiters
    snapshot signal values for next-tick edge detection
```

Consequences:

- **Deterministic timing.** There is no VPI callback-ordering ambiguity. Writes from Python to `dut.port.value` take effect immediately (direct field set on the pybind11 C++ object) and are visible to `model.eval()` on the very next tick. This is how `arch sim --pybind` sidesteps the `interrupt_mask=X`-style bug that iverilog cocotb runs into.
- **1 tick = 1 ns by default.** Tunable via `time_unit_ns` in the runner (`arch_cocotb.runner.run_tests`).
- **Edge detection is sampled.** Each tick takes a snapshot of every signal currently being waited on, then compares against the previous snapshot. An edge that begins and ends inside a single tick (i.e., faster than 1 time unit) will be missed; edges slower than 1 time unit are always caught.
- **No delta cycles.** Writes do not re-trigger `eval()` within the same tick. If your test writes an input and wants to observe the combinational result, either `await RisingEdge(clk)` (preferred — matches RTL sampling) or `await Timer(1, 'ns')` (generic advance).

---

## Deltas from real cocotb

| Area | Real cocotb | `arch sim --pybind` |
|---|---|---|
| Backend | VPI / VHPI over iverilog, Verilator, Questa, VCS, etc. | Direct pybind11 on ARCH's native 2-state C++ sim |
| Logic values | 4-state (0/1/X/Z) via `LogicArray` | 2-state (uint). `X` and `Z` do not exist |
| Write timing | Inertial / NBA-region; takes effect in the RTL sampling region | Immediate; visible on the next `eval()` tick |
| Trigger granularity | Event-driven via simulator callbacks | Tick-sampled (see above) |
| Decorators | Full registration, regression runner, coverage, etc. | Only `@cocotb.test()`, minimal options honored |
| `LogicArray` / `BinaryValue` | Full bit-vector API with X/Z | `ArchSignalValue` — integer-like, no 4-state |
| Coroutines spawned from `start_soon` | Cancelled on test-function return | Cancelled on test-task done (same effect in practice) |
| Waveform output | FST/VCD via the simulator | Use `arch sim --wave out.vcd` separately; not wired through pybind yet |

The biggest gotcha is **2-state logic**. A TB that leaves an input undriven sees `0` under `arch sim`, not `X`. Use `arch sim --inputs-start-uninit` to catch this at simulation time (see CLAUDE.md § "Catching X-propagation from undriven inputs").

---

## Writing a portable testbench

If you want the same `.py` file to run under both `arch sim --pybind` and a real cocotb flow, stay inside the intersection:

1. Import via the plain `cocotb` namespace (the shim handles it).
2. Stick to `RisingEdge` / `FallingEdge` / `Timer` / `ClockCycles` / `Clock` / `start_soon` / `@cocotb.test()` / `cocotb.utils.get_sim_time`.
3. Treat `dut.sig.value` as an integer-compatible object. Use `int(...)` for comparisons, not `.binstr` / `.is_resolvable` / other 4-state attributes.
4. Don't rely on any simulator-specific primitives (`ReadOnly`, `ReadWrite`, `NextTimeStep`, `First`, `Lock`, `Event`, `Join`, etc.) — none are in the shim today.
5. Don't initialize inputs with `1'bx` / `'X'` — pick 0 or run with `--inputs-start-uninit` for the same effect.

Tests that need richer cocotb features (e.g., full `First()` composition, `LogicArray`, `NextTimeStep`) should run under a real simulator via a separate runner.

---

## Troubleshooting

- **`ModuleNotFoundError: No module named 'VFoo_pybind'`** — the pybind11 build failed silently or the Python process did not pick up `arch_sim_build/` on `PYTHONPATH`. Check the `g++` output from `arch sim` for compile errors.
- **`pybind11 not found`** — `pip install pybind11` into the Python environment `arch sim` will invoke. `--pybind` shells out to `python3 -m pybind11 --includes` to locate headers.
- **`AttributeError: No signal 'foo' on DUT`** — the port name you referenced is not in the generated `_port_info()` list. Check the `.arch` file — names are case-sensitive and must match the port declaration.
- **A test hangs** — the scheduler advances one tick per loop iteration, so an `await RisingEdge(dut.clk)` with no `Clock` task running never returns. Make sure `cocotb.start_soon(Clock(...).start())` runs before the first edge await.
- **An edge fires one cycle late/early** — remember writes are immediate, not NBA-delayed. If you set an input and then immediately `await RisingEdge(clk)`, the DUT samples that input on *this* tick's edge, not next tick's.

---

## Source layout

| Path | Purpose |
|---|---|
| `src/sim_codegen.rs` — `generate_pybind()` | Emits `V<Module>_pybind.cpp` and `_port_info()` metadata |
| `python/arch_cocotb/decorators.py` | `@test`, `start_soon`, `start` |
| `python/arch_cocotb/triggers.py` | `RisingEdge`, `FallingEdge`, `Timer`, `Clock`, `ClockCycles` |
| `python/arch_cocotb/dut.py` | `ArchDUT` — auto-registers signals from `_port_info()` |
| `python/arch_cocotb/signal.py` | `ArchSignal`, `ArchSignalValue` |
| `python/arch_cocotb/simulator.py` | `ArchSimulator` — the tick loop, edge detector, timer heap |
| `python/arch_cocotb/runner.py` | `run_tests(model_class, test_module_name)` |
| `python/arch_cocotb/utils.py` | `get_sim_time` |
| `python/cocotb_shim/cocotb/` | Drop-in `cocotb` namespace backed by `arch_cocotb` |
