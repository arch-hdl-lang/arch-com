# `arch sim` — cocotb Integration Guide

`arch sim --pybind --test` runs Python testbenches against an ARCH design using a cocotb-compatible API, with no Verilator, iverilog, or VPI in the loop. The generated C++ model is wrapped with pybind11, and an event-driven scheduler (`arch_cocotb`) drives it from an asyncio event loop. This flow also supports `--thread-sim parallel`; the pybind wrapper adapts to the pre-lowering thread-sim model's edge-sensitive `eval()` API.

The intent is "write the same testbench you would for real cocotb" — same decorators, triggers, signal handles, and coroutine patterns. The shim implements enough of the cocotb 2.x surface that **the installed `cocotbext-axi` package drives ARCH designs unmodified** (see `tests/cocotb_shim/`).

---

## Quick start

```sh
arch sim --pybind --test test_mymodule.py MyModule.arch
arch sim --thread-sim parallel --pybind --test test_mymodule.py MyThreadModule.arch
```

Under the hood this:

1. Runs the ARCH → C++ codegen (`SimCodegen::generate_pybind`), producing `VMyModule_pybind.cpp` next to the normal `VMyModule.cpp`.
2. Compiles the pybind11 wrapper with `python3 -m pybind11 --includes` plus the configured C++ compiler (`g++` by default; override with `ARCH_CXX`, e.g. `ARCH_CXX=clang++` — required on Linux, where GCC miscompiles the C++20 coroutine testbench scheduler), emitting `VMyModule_pybind.<ext>` into the build directory (`--outdir` if given, else `$ARCH_SIM_BUILD_DIR`, else `arch_sim_build/`).
3. Spawns `python3 test_mymodule.py` with `PYTHONPATH` set to find three packages:
   - `python/cocotb_shim/cocotb/` — so `import cocotb` works unchanged (and shadows a real installed cocotb)
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

`python/cocotb_shim/cocotb/` is a drop-in stand-in that re-exports the real implementation from `arch_cocotb`:

| `cocotb` module | Symbols |
|---|---|
| `cocotb` | `test` (decorator), `start_soon`, `start`, `create_task`, `__version__` |
| `cocotb.triggers` | `RisingEdge`, `FallingEdge`, `Edge`, `Timer`, `ClockCycles`, `ReadOnly`, `ReadWrite`, `NextTimeStep`, `NullTrigger`, `Event`, `First`, `Combine`, `Lock`, `with_timeout`, `SimTimeoutError` |
| `cocotb.clock` | `Clock` |
| `cocotb.queue` | `Queue`, `PriorityQueue`, `LifoQueue`, `QueueFull`, `QueueEmpty` |
| `cocotb.types` | `Logic`, `LogicArray`, `Range` |
| `cocotb.utils` | `get_sim_time` |
| `cocotb.result` | `SimTimeoutError`, `TestSuccess` |
| `cocotb.handle` | `SimHandleBase` (alias of `ArchSignal`) |

Because of the shim, you can point the same test file at either `arch sim` or a real cocotb runner without edits, and cocotb-based bus libraries (`cocotbext-axi`, `cocotb_bus.bus.Bus`) import and run against the native model.

---

## Time model

Internal simulator time is an **integer count of picoseconds** (one cocotb `step` = 1 ps). `Clock`, `Timer`, `@cocotb.test(timeout_time=...)`, `with_timeout`, and `get_sim_time()` all share this one time base, and durations convert exactly — a 3333 ps clock alternates 1666 ps / 1667 ps half-periods, and `Timer(1.5, 'ns')` wakes exactly 1500 ps later. `get_sim_time('step')` (the default) returns the integer ps count; other units return floats.

`Timer` rejects zero and negative durations (matching cocotb). Sub-picosecond durations (`fs`) round to the nearest picosecond.

## Scheduler model

The scheduler is **event-driven**: it advances directly to the next timer deadline (clock transitions are timers under the hood) instead of evaluating the model once per nanosecond. A model evaluation runs when a testbench write is applied, when a clock/input transition occurs, or conservatively after timer callbacks ran.

Within one timestep, the observable phase order mirrors an event-driven simulator:

1. Runnable coroutines execute; `sig.value = x` writes are **buffered deposits**.
2. Deposits apply to the model's input fields.
3. Edge waiters on directly-written signals (e.g. `RisingEdge(clk)` when the `Clock` task toggles `clk`) resume **before** the model evaluates the edge — their reads observe pre-edge register state, exactly like cocotb coroutines woken in the active region before NBA updates land. Their reaction writes are buffered as new deposits.
4. `eval()` runs: sequential logic samples the pre-reaction input values.
5. Edge waiters on eval-derived signals (registered outputs) resume after eval.
6. The loop repeats until quiescent; then `ReadWrite` waiters resume, then `ReadOnly` waiters — same timestep, no time advance. `ReadOnly()` is a scheduler phase transition, not a timer.
7. Only then does time jump to the next deadline; `NextTimeStep` waiters resume there.

This ordering is what cocotb bus models depend on: a source that does `await RisingEdge(clk)` then samples `ready` sees the pre-edge value, and a monitor that does `await ReadOnly()` sees the fully settled post-edge state.

**Write timing.** `sig.value = x` is a deposit — it is *not* visible to the same edge's sequential logic, and a read-back in the same coroutine step returns the old value (same as real cocotb). Use `sig.setimmediatevalue(x)` for an immediate update. All writes are masked to the signal's declared width.

**Trigger cleanup.** Every trigger registration is removed when its await is abandoned: a task killed with `task.kill()`, a `First()` race loser, or a `with_timeout` expiry deregisters its timer/edge/phase registrations. Stale registrations never fire later and never retain task objects.

---

## API reference (what's actually implemented)

### `@cocotb.test()`

Registers an async function as a test. Honors `skip=True`, `timeout_time=`/`timeout_unit=` (enforced via `with_timeout` → `SimTimeoutError`), `expect_error=`, and `expect_fail=`.

### Triggers

| Trigger | Behavior |
|---|---|
| `RisingEdge(signal)` | Resume on the next 0→1 transition. |
| `FallingEdge(signal)` | Resume on the next 1→0 transition. |
| `Edge(signal)` | Resume on any value change. |
| `Timer(time, unit='step')` | Resume at the exact deadline. Units: `fs`/`ps`/`ns`/`us`/`ms`/`sec`/`step` (`units=` alias accepted). |
| `ClockCycles(signal, n, rising=True)` | Count N active edges. |
| `ReadOnly()` | Resume after settling in the current timestep (no time advance). |
| `ReadWrite()` | Resume after settling, before the ReadOnly phase. |
| `NextTimeStep()` | Resume when simulation time next advances. |
| `NullTrigger()` | Resume after one scheduler yield. |
| `Event` | `set(data)`, `clear()`, `is_set()`, `wait()`; `set()` wakes every current waiter. |
| `First(*triggers)` | Resume on the first completion; returns that trigger; losers deregistered. |
| `Combine(*triggers)` | Resume when all complete. |
| `Lock` | Async context manager with FIFO handoff. |
| `with_timeout(trigger, t, unit)` | Result of the awaited thing, or `SimTimeoutError` at the exact deadline. |

### `Clock(signal, period, units='ns')`

`.start(start_high=True)` returns an infinite coroutine — schedule it with `start_soon`. High time is `period // 2` (in ps), low time is the remainder, so odd periods are exact.

### Task handles

`start_soon(coro)` returns a task handle supporting `await task`, `done()`, `result()`, `kill()` (and the cocotb 2.x `cancel()`), plus `join()`. `cocotb.start(coro)` additionally yields once so the child runs up to its first await. Killing a task removes all of its trigger registrations and prevents further signal writes.

### Signal access (`dut.<name>`)

Signal handles (`ArchSignal`) expose:

```python
dut.enable.value = 1                     # deposit (next sync point)
dut.enable.setimmediatevalue(1)          # immediate
current = int(dut.count.value)           # read
len(dut.addr)                            # declared width in bits
signed  = dut.result.value.to_signed()   # sign at declared width
```

`ArchSignalValue` supports `int()`, `bool()`, `len()`, equality against ints and other values, `.integer`/`.signed_integer`/`.binstr`, and `to_unsigned()`/`to_signed()` at the declared width. There is still no per-bit `X`/`Z` state (2-state model); `LogicArray` inputs convert `X`/`Z` bits deterministically to 0 on assignment.

The DUT handle provides `_name`, `_log` (a standard `logging.Logger`), `dir(dut)` listing every port/parameter (this is what prefix-based bus discovery uses), case-insensitive attribute fallback, and stable iteration over signals.

Parameters are read-only handles; writing one raises `AttributeError`.

### `cocotb.utils.get_sim_time(unit='step')`

`'step'` returns the integer picosecond count; `'fs'` an exact integer; `'ps'`/`'ns'`/`'us'`/`'ms'`/`'sec'` floats.

---

## Deltas from real cocotb

| Area | Real cocotb | `arch sim --pybind` |
|---|---|---|
| Backend | VPI / VHPI over iverilog, Verilator, Questa, VCS, etc. | Direct pybind11 on ARCH's native 2-state C++ sim |
| Logic values | 4-state (0/1/X/Z) via `LogicArray` | 2-state. `X`/`Z` convert to 0 on assignment; reads never produce them |
| Sub-timestep detail | Full delta-cycle model | Region-ordered phases per timestep (see scheduler model) |
| Decorators | Full regression manager, coverage | `@cocotb.test()` with skip/timeout/expect options |
| Waveform output | FST/VCD via the simulator | Use `arch sim --wave out.vcd` separately; not wired through pybind yet |
| Missing triggers | — | `Join` (use `task.join()`), `ClockCycles` edge-type variants beyond rising/falling |

The biggest remaining gotcha is **2-state logic**. A TB that leaves an input undriven sees `0` under `arch sim`, not `X`. Use `arch sim --inputs-start-uninit` to catch this at simulation time (see CLAUDE.md § "Catching X-propagation from undriven inputs").

---

## Using cocotb bus libraries

`cocotbext-axi` works unmodified:

```python
from cocotbext.axi import AxiBus, AxiMaster

master = AxiMaster(AxiBus.from_prefix(dut, "s_axi"), dut.clk, dut.rst)
await master.write(0x0000, b'test')
data = await master.read(0x0000, 4)
```

Declare the DUT's ports with cocotbext's flat naming convention (`s_axi_awaddr`, `s_axi_awvalid`, …) so `from_prefix` discovers them; see `tests/cocotb_shim/AxilRegs.arch` and `tests/cocotb_shim/Axi4Mem.arch` for working AXI-Lite and AXI4 slaves, and `tests/cocotb_shim/test_axil_cocotbext.py` / `test_axi4_cocotbext.py` for the conformance tests (bursts, byte strobes, independent per-channel backpressure via pause generators, queued transactions).

Note that ports wider than 64 bits are truncated by the pybind layer today — keep bus data widths ≤ 64 bits for pybind-driven tests.

---

## Troubleshooting

- **`ModuleNotFoundError: No module named 'VFoo_pybind'`** — the pybind11 build failed silently or the Python process did not pick up `arch_sim_build/` on `PYTHONPATH`. Check the `g++` output from `arch sim` for compile errors.
- **`pybind11 not found`** — `pip install pybind11` into the Python environment `arch sim` will invoke. `--pybind` shells out to `python3 -m pybind11 --includes` to locate headers.
- **`AttributeError: No signal 'foo' on DUT`** — the port name you referenced is not in the generated `_port_info()` list. Check the `.arch` file (a case-insensitive match is attempted before raising).
- **A test hangs / deadlock error** — an `await RisingEdge(dut.clk)` with no `Clock` task running can never fire. The scheduler raises `deadlock — no timers pending` naming the awaited signals instead of spinning.
- **A write isn't visible where you expect** — `sig.value = x` is a buffered deposit (cocotb semantics): the same edge's sequential logic doesn't see it, and neither does an immediate read-back. Use `setimmediatevalue()` when you need the old immediate behavior.

---

## Source layout

| Path | Purpose |
|---|---|
| `src/sim_codegen/mod.rs` — `generate_pybind()` | Emits `V<Module>_pybind.cpp` and `_port_info()` metadata |
| `python/arch_cocotb/simulator.py` | Event-driven scheduler: ps time base, deposits, phases, deadlock detection |
| `python/arch_cocotb/triggers.py` | All triggers, `Event`, `First`, `Combine`, `Lock`, `with_timeout`, `Clock` |
| `python/arch_cocotb/task.py` | Task handles (`kill`, `join`) |
| `python/arch_cocotb/queue.py` | `Queue`, `PriorityQueue`, `LifoQueue` |
| `python/arch_cocotb/types.py` | `Logic`, `LogicArray`, `Range` |
| `python/arch_cocotb/dut.py` | `ArchDUT` — signal registration, `dir()`, case-insensitive lookup |
| `python/arch_cocotb/signal.py` | `ArchSignal`, `ArchSignalValue` — deposits, masking, widths |
| `python/arch_cocotb/decorators.py` | `@test`, `start_soon`, `start` |
| `python/arch_cocotb/runner.py` | `run_tests(model_class, test_module_name)` |
| `python/cocotb_shim/cocotb/` | Drop-in `cocotb` namespace backed by `arch_cocotb` |
| `tests/cocotb_shim/` | Conformance suite + cocotbext-axi AXI-Lite/AXI4 fixtures |
