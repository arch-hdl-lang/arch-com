# `arch sim` — native cocotb integration

`arch sim --pybind --test` runs cocotb-style Python tests directly against the
ARCH-generated C++ model. Verilator, iverilog, and VPI are not involved. The
compiler builds a pybind11 extension, installs the repository's `cocotb`
compatibility package on `PYTHONPATH`, and runs every decorated test with a
fresh model and scheduler.

```sh
arch sim --pybind --test test_mymodule.py MyModule.arch
arch sim --thread-sim parallel --pybind --test test_threads.py Threads.arch
```

The native adapter is event-driven and uses exact integer picoseconds. It is
also sufficient for the AXI4 and AXI4-Lite masters in the unmodified
`cocotbext-axi` 0.1.28 package.

## Installation

The Python used by `arch` needs pybind11:

```sh
python3 -m pip install pybind11
```

For the optional upstream AXI conformance tests:

```sh
python3 -m pip install -r python/requirements-cocotbext-axi.txt
PYTHON=python3 scripts/test_native_cocotb.sh
```

`PYTHON` may point at a virtual-environment interpreter. The test script runs
the focused shim tests, builds both native pybind designs, and runs the
installed `cocotbext-axi` package directly.

## Example

```python
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import ClockCycles, ReadOnly, RisingEdge


@cocotb.test(timeout_time=20, timeout_unit="us")
async def test_counter(dut):
    cocotb.start_soon(Clock(dut.clk, 3333, units="ps").start(False))

    dut.rst.value = 1
    dut.enable.value = 0
    await ClockCycles(dut.clk, 2)
    dut.rst.value = 0

    dut.enable.value = 1
    await RisingEdge(dut.clk)
    await ReadOnly()
    assert int(dut.count.value) == 1
```

The same imports can be used with a conventional cocotb simulator, provided
the test stays within the compatible surface below.

## Scheduler semantics

Simulation time is stored as integer picoseconds. `Timer`, `Clock`, timeout
handling, and `get_sim_time()` all use that time base; there is no intermediate
rounding to nanoseconds. For example, a 3333 ps clock alternates 1666 ps and
1667 ps half-periods without drift.

At each timestamp the scheduler:

1. Runs ready Python coroutines and batches their signal writes.
2. Calls the native model's atomic `eval()` operation.
3. Lets the model settle combinational logic, apply detected clock edges, and
   settle combinational logic again.
4. Wakes matching edge coroutines.
5. Repeats zero-time deltas until no task or model value can make progress.
6. Wakes `ReadOnly` waiters without advancing time.
7. Jumps directly to the next timer or clock deadline.

An idle 100 us timer therefore causes one time jump, not 100,000 one-nanosecond
model evaluations. A bounded delta-cycle check reports a non-settling
zero-time loop, and a test blocked with no possible future event reports a
simulation deadlock.

Signal writes are applied to the pybind object immediately and trigger a model
evaluation in the current timestamp. They do not have Verilog NBA semantics.
The generated model's `eval()` remains the hardware atomicity boundary.

## Supported cocotb API

### Tests and tasks

- `@cocotb.test` and `@cocotb.test(...)`
- `timeout_time`, `timeout_unit`, `expect_error`, `expect_fail`, and `skip`
- `cocotb.start_soon(coro)`
- `await cocotb.start(coro)`

`start_soon()` returns an awaitable task with `done()`, `result()`,
`exception()`, `cancelled()`, and `kill()`. Killing a task cancels its current
timer, edge, or phase registration. Pending background tasks are cancelled
when their owning test ends; an unobserved background exception fails the
test.

### Triggers and timing

- `RisingEdge`, `FallingEdge`, and `Edge`
- `Timer`
- `ClockCycles`
- `Clock`
- `ReadOnly`
- `Event`
- `First`
- `with_timeout` and `SimTimeoutError`
- `cocotb.utils.get_sim_time`

`Event.set(data)` stores `Event.data` and wakes every current waiter.
`Event.wait()` returns a reusable awaitable, as expected by cocotb bus
libraries. `First` returns the winning trigger's result and cancels only the
losing awaitables it created; existing task handles are left alive.

Timers accept `fs`, `ps`, `ns`, `us`, `ms`, `s`, and `step`. A duration that
cannot be represented at one-picosecond precision is rejected rather than
silently rounded.

### Queues and value types

- `cocotb.queue.Queue`
- `QueueFull` and `QueueEmpty`
- `cocotb.types.Logic`
- `cocotb.types.LogicArray`

The queue API is backed by `asyncio.Queue`. `Logic` and `LogicArray` are
minimal two-state types. Binary strings are accepted and `X`/`Z` digits
resolve deterministically to zero.

## DUT and signal handles

`ArchDUT` exposes `_name`, `_log`, stable signal iteration, and every generated
port or parameter through `dir(dut)`. Attribute lookup is case-insensitive,
which lets `cocotb-bus` perform its normal prefix discovery:

```python
from cocotbext.axi import AxiLiteBus, AxiLiteMaster

master = AxiLiteMaster(
    AxiLiteBus.from_prefix(dut, "s_axil"),
    dut.clk,
    dut.rst,
)
```

`ArchSignal` supports:

- read/write `.value`
- `len(signal)`
- `setimmediatevalue(value)`
- stable `_name` and `_type`

Assignments are masked to the declared width. When
`--inputs-start-uninit` is active, pybind writes to checked scalar and
bus-flattened data inputs also mark the generated input-valid shadow bit, just
like the generated C++ `set_<port>()` method. Clock and reset inputs are
excluded. Per-lane uninitialized tracking for Vec inputs remains outside the
current checker scope.

`ArchSignalValue` supports integer, Boolean, index, signed, and unsigned
conversion; width; integer equality; `integer`, `signed_integer`, `binstr`,
and `is_resolvable`.

Generated wide pybind properties preserve arbitrary-width Python integers.
Values wider than 64 bits are copied across every `VlWide` word and masked at
the actual port width.

## Runner behavior

The launcher executes the test file once as `__main__`, preserving legacy
scripts with a `main()` block. If decorators registered tests, it consumes
that existing registry without importing the file a second time. This avoids
duplicate registration and repeated module-level side effects.

For each decorated test the runner:

1. Constructs a fresh pybind model, `ArchDUT`, and scheduler.
2. Runs the test and all scheduled background work.
3. Applies decorator timeout/expected-failure behavior.
4. Cancels remaining tasks and removes trigger registrations.
5. Prints a per-test result and traceback on failure.

The process exits nonzero if compilation or any test fails.

## Verified `cocotbext-axi` coverage

`tests/cocotb_axi/` contains small ARCH memory targets and Python tests that
import the installed package without patching or replacing it:

- AXI4-Lite reads and writes
- AXI4 incrementing burst reads and writes
- unaligned and byte-strobe writes
- independent randomized AW, W, B, AR, and R channel pauses
- multiple queued transactions
- final byte-addressed memory comparison

The test targets present handshake outputs on the falling edge so they remain
stable across the next rising-edge sample. Native C++ simulation now honors
`seq on clk falling`; it previously clocked every module seq block from the
rising-edge detector.

## Differences from full cocotb

| Area | Native ARCH behavior |
|---|---|
| Logic | Two-state only. Unknown and high-impedance values resolve to zero. |
| Scheduling | Event-driven Python scheduler around atomic native `eval()`; not the complete GPI/VPI phase machine. |
| Writes | Immediate pybind assignment followed by same-timestamp evaluation; no NBA queue. |
| Hierarchy | Flat generated ports, parameters, and selected internal registers; no general sub-hierarchy handle traversal. |
| Triggers | The triggers documented above are supported. `ReadWrite`, `NextTimeStep`, `Combine`, `Join`, and `Lock` are not yet implemented. |
| Force/deposit | No simulator force, freeze, release, or deposit action objects. |
| Four-state APIs | No genuine X/Z propagation, resolution, strength, or per-bit four-state arithmetic. |
| Wave control | Use the normal ARCH simulation waveform options; cocotb runner waveform controls are not implemented. |

The most important portability difference is two-state startup. Use
`--inputs-start-uninit` and `--check-uninit-ram` when a test should detect
forgotten input or memory initialization.

## Troubleshooting

- `pybind11 not found`: install it into the `python3` environment invoked by
  `arch`, or put the intended virtual environment first on `PATH`.
- `No signal '<name>'`: inspect `dir(dut)` and the generated `_port_info()`.
  Bus signals are flattened (`axi.aw_valid` becomes `axi_aw_valid`).
- Deadlock with no timer: start the clock before awaiting its edge.
- Timeout while a bus is idle: make sure the DUT and BFM use the same reset
  polarity and that every required bus port appears in `dir(dut)`.
- Test runs twice: do not manually call a decorated coroutine from a
  `__main__` block. The launcher executes the file once and then runs the
  decorator registry.

## Source and tests

| Path | Purpose |
|---|---|
| `python/arch_cocotb/simulator.py` | Picosecond event scheduler and trigger cleanup |
| `python/arch_cocotb/task.py` | Cocotb-compatible task handle |
| `python/arch_cocotb/triggers.py` | Edges, timing, events, `First`, and timeouts |
| `python/arch_cocotb/dut.py` | DUT discovery and Vec proxies |
| `python/arch_cocotb/signal.py` | Signal handles and values |
| `python/arch_cocotb/types.py` | Minimal two-state `Logic`/`LogicArray` |
| `python/arch_cocotb/queue.py` | Queue compatibility |
| `python/arch_cocotb/runner.py` | Per-test lifecycle and result reporting |
| `python/cocotb_shim/cocotb/` | Drop-in `cocotb` import namespace |
| `python/tests/test_arch_cocotb.py` | Focused scheduler/API conformance |
| `tests/cocotb_axi/` | Installed-package AXI4/AXI4-Lite conformance |
| `scripts/test_native_cocotb.sh` | Complete local native cocotb test command |
