"""Cocotb-compatible triggers for ARCH's native event scheduler."""

import asyncio
from decimal import Decimal, InvalidOperation

from arch_cocotb.simulator import _get_sim
from arch_cocotb.task import ArchTask


class SimTimeoutError(TimeoutError):
    """Raised by :func:`with_timeout` when simulation time expires."""


_TIME_UNIT_PS = {
    "fs": Decimal("0.001"),
    "f": Decimal("0.001"),
    "ps": Decimal(1),
    "p": Decimal(1),
    "ns": Decimal(1000),
    "n": Decimal(1000),
    "us": Decimal(1_000_000),
    "u": Decimal(1_000_000),
    "micro": Decimal(1_000_000),
    "ms": Decimal(1_000_000_000),
    "m": Decimal(1_000_000_000),
    "milli": Decimal(1_000_000_000),
    "s": Decimal(1_000_000_000_000),
    "sec": Decimal(1_000_000_000_000),
}


class _EdgeTrigger:
    _edge_type = "either"

    def __init__(self, signal):
        self.signal = signal

    def __await__(self):
        return _get_sim().wait_edge(
            self.signal, self._edge_type, self
        ).__await__()


class RisingEdge(_EdgeTrigger):
    """Suspend until the next 0-to-1 transition."""

    _edge_type = "rising"


class FallingEdge(_EdgeTrigger):
    """Suspend until the next 1-to-0 transition."""

    _edge_type = "falling"


class Edge(_EdgeTrigger):
    """Suspend until the next value transition."""


class ReadOnly:
    """Resume after the current timestamp's writes and model deltas settle."""

    def __await__(self):
        return _get_sim().wait_readonly(self).__await__()


class Timer:
    """Suspend for an exact integer number of picoseconds."""

    def __init__(
        self,
        duration=None,
        units="ns",
        unit=None,
        timeout_time=None,
        timeout_unit=None,
        **kwargs,
    ):
        if duration is None and timeout_time is not None:
            duration = timeout_time
            units = timeout_unit or units
        if duration is None:
            duration = kwargs.get("time", 0)
        if unit is not None:
            units = unit
        self.duration = duration
        self.units = units

    def __await__(self):
        sim = _get_sim()
        duration_ps = _to_ps(self.duration, self.units, sim.step_ps)
        return sim.wait_timer(duration_ps, self).__await__()


class Clock:
    """Drive a clock with picosecond-accurate alternating half periods."""

    def __init__(self, signal, period, units="ns", unit=None):
        self.signal = signal
        self.period = period
        self.units = unit or units

    async def start(self, start_high=True):
        sim = _get_sim()
        period_ps = _to_ps(self.period, self.units, sim.step_ps)
        if period_ps < 2:
            raise ValueError("clock period must be at least 2 ps")
        first_half = period_ps // 2
        second_half = period_ps - first_half
        value = 1 if start_high else 0
        self.signal.value = value
        delay = first_half if start_high else second_half
        while True:
            await sim.wait_timer(delay)
            value ^= 1
            self.signal.value = value
            delay = first_half if value else second_half


class ClockCycles:
    """Suspend for a number of rising or falling clock edges."""

    def __init__(self, signal, num_cycles, rising=True):
        self.signal = signal
        self.num_cycles = int(num_cycles)
        self.rising = rising

    def __await__(self):
        return self._wait().__await__()

    async def _wait(self):
        trigger_type = RisingEdge if self.rising else FallingEdge
        for _ in range(self.num_cycles):
            await trigger_type(self.signal)
        return self


class Event:
    """One-shot coroutine notification carrying optional data."""

    def __init__(self, name=None):
        self.name = name
        self.data = None
        self._is_set = False
        self._waiters = []

    def set(self, data=None):
        self.data = data
        self._is_set = True
        waiters, self._waiters = self._waiters, []
        for future in waiters:
            if not future.done():
                future.set_result(data)

    def clear(self):
        self._is_set = False

    def is_set(self):
        return self._is_set

    def wait(self):
        # Cocotb returns a reusable trigger object here. cocotbext-axi caches
        # that object outside loops, so returning a one-shot Python coroutine
        # would fail with "cannot reuse already awaited coroutine".
        return _EventWaiter(self)

    async def _wait(self):
        if self._is_set:
            return self.data
        future = _get_sim()._future()
        self._waiters.append(future)
        try:
            return await future
        finally:
            if future in self._waiters:
                self._waiters.remove(future)


class _EventWaiter:
    def __init__(self, event):
        self._event = event

    def __await__(self):
        return self._event._wait().__await__()


class First:
    """Resume with the result of the first completed awaitable."""

    def __init__(self, *awaitables):
        if not awaitables:
            raise ValueError("First requires at least one awaitable")
        self.awaitables = awaitables

    def __await__(self):
        return self._wait().__await__()

    async def _wait(self):
        tasks = []
        owned = []
        for awaitable in self.awaitables:
            if isinstance(awaitable, ArchTask):
                task = awaitable._task
                awaitable._observed = True
            else:
                task = asyncio.ensure_future(awaitable)
                owned.append(task)
            tasks.append(task)
        done, _ = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
        winner = next(task for task in tasks if task in done)
        for task in owned:
            if task is not winner and not task.done():
                task.cancel()
        if owned:
            await asyncio.gather(
                *(task for task in owned if task is not winner),
                return_exceptions=True,
            )
        return await winner


async def with_timeout(
    awaitable,
    timeout_time,
    timeout_unit="ns",
    *,
    timeout_units=None,
):
    """Await an object, raising ``SimTimeoutError`` on simulated timeout."""

    timer = Timer(timeout_time, units=timeout_units or timeout_unit)
    if isinstance(awaitable, ArchTask):
        operation = awaitable
        owned = False
    else:
        async def run_awaitable():
            return await awaitable

        operation = _get_sim().schedule(run_awaitable())
        owned = True
    winner = await First(operation, timer)
    if winner is timer:
        if owned:
            operation.kill()
        raise SimTimeoutError(
            f"timed out after {timeout_time} {timeout_units or timeout_unit}"
        )
    return winner


def _to_ps(duration, units, step_ps=1000):
    """Convert cocotb time values to exact integer picoseconds."""
    scale = _unit_ps(units, step_ps)
    try:
        picoseconds = Decimal(str(duration)) * scale
    except (InvalidOperation, ValueError) as error:
        raise ValueError(f"invalid timer duration: {duration!r}") from error
    integral = picoseconds.to_integral_value()
    if picoseconds != integral:
        raise ValueError(
            f"{duration} {units} is not representable at 1 ps precision"
        )
    result = int(integral)
    if result < 0:
        raise ValueError("Timer duration must not be negative")
    return result


def _unit_ps(units, step_ps=1000):
    """Return the picosecond scale for a cocotb time unit."""
    unit = "step" if units is None else str(units).lower().rstrip("s")
    if unit == "step":
        return Decimal(step_ps)
    if unit not in _TIME_UNIT_PS:
        raise ValueError(f"unsupported time unit: {units}")
    return _TIME_UNIT_PS[unit]
