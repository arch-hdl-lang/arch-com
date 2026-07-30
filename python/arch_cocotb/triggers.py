"""Cocotb-compatible trigger classes for arch sim.

Every trigger follows the same lifecycle: awaiting it registers a
wakeup with the scheduler and deregisters in a ``finally`` block, so a
killed task or a lost ``First`` race removes its registrations instead
of leaving them to fire later.
"""

import asyncio

from arch_cocotb.simulator import _get_sim, to_ps


class SimTimeoutError(Exception):
    """Raised by with_timeout when the deadline expires first."""


class Trigger:
    """Base class: awaiting runs _wait(), which returns the trigger."""

    def __await__(self):
        return self._wait().__await__()

    async def _wait(self):
        raise NotImplementedError


class _EdgeBase(Trigger):
    _kind = 'any'

    def __init__(self, signal):
        self._signal = signal

    async def _wait(self):
        sim = _get_sim()
        reg = sim.register_edge(self._signal, self._kind)
        try:
            await reg.fut
        finally:
            reg.deregister()
        return self

    def __repr__(self):
        return f"{type(self).__name__}({self._signal._name})"


class RisingEdge(_EdgeBase):
    """Resume on the next 0 -> 1 transition of the signal."""
    _kind = 'rising'


class FallingEdge(_EdgeBase):
    """Resume on the next 1 -> 0 transition of the signal."""
    _kind = 'falling'


class Edge(_EdgeBase):
    """Resume on any value change of the signal."""
    _kind = 'any'


class Timer(Trigger):
    """Resume at exactly the requested simulator time from now.

    Follows the cocotb signature: ``Timer(time, unit='step')``. One
    step is one picosecond. Zero and negative durations are rejected.
    """

    def __init__(self, time=None, unit=None, *, units=None, round_mode=None,
                 **kwargs):
        if time is None:
            time = kwargs.pop('duration', None)
        if time is None:
            time = kwargs.pop('timeout_time', None)
        if time is None:
            raise TypeError("Timer requires a duration")
        if units is not None and unit is None:
            unit = units
        self._delay_ps = to_ps(time, unit)
        if self._delay_ps <= 0:
            raise ValueError(
                f"Timer duration must be at least 1 ps, got {time!r} "
                f"{unit or 'step'}"
            )

    async def _wait(self):
        sim = _get_sim()
        reg = sim.register_timer(self._delay_ps)
        try:
            await reg.fut
        finally:
            reg.deregister()
        return self


class ReadOnly(Trigger):
    """Resume after all writes and comb settling in the current timestep.

    This is a scheduler phase transition: simulation time does not
    advance.
    """

    async def _wait(self):
        sim = _get_sim()
        reg = sim.register_readonly()
        try:
            await reg.fut
        finally:
            reg.deregister()
        return self


class ReadWrite(Trigger):
    """Resume after settling, before the ReadOnly phase. No time advance."""

    async def _wait(self):
        sim = _get_sim()
        reg = sim.register_readwrite()
        try:
            await reg.fut
        finally:
            reg.deregister()
        return self


class NextTimeStep(Trigger):
    """Resume when simulation time next advances."""

    async def _wait(self):
        sim = _get_sim()
        reg = sim.register_nextstep()
        try:
            await reg.fut
        finally:
            reg.deregister()
        return self


class NullTrigger(Trigger):
    """Resume after one scheduler yield, without advancing time."""

    def __init__(self, name=None):
        self._name = name

    async def _wait(self):
        loop = asyncio.get_running_loop()
        fut = loop.create_future()
        loop.call_soon(fut.set_result, None)
        await fut
        return self


class Clock:
    """Generate an exact periodic clock.

    The half-periods are computed in picoseconds: high time is
    ``period // 2`` and low time is the remainder, so a 3333 ps clock
    alternates 1666 ps high / 1667 ps low exactly.
    """

    def __init__(self, signal, period, units=None, unit=None, impl=None):
        self._signal = signal
        if unit is None:
            unit = units if units is not None else 'ns'
        period_ps = to_ps(period, unit)
        if period_ps < 2:
            raise ValueError("clock period must be at least 2 ps")
        self._period_ps = period_ps
        self._high_ps = period_ps // 2
        self._low_ps = period_ps - self._high_ps

    @property
    def period(self):
        return self._period_ps

    def start(self, start_high=True):
        """Return the coroutine driving the clock forever."""

        async def _run():
            sim = _get_sim()
            if start_high:
                while True:
                    self._signal.value = 1
                    await sim.register_timer(self._high_ps).fut
                    self._signal.value = 0
                    await sim.register_timer(self._low_ps).fut
            else:
                while True:
                    self._signal.value = 0
                    await sim.register_timer(self._low_ps).fut
                    self._signal.value = 1
                    await sim.register_timer(self._high_ps).fut

        return _run()

    def __repr__(self):
        return f"Clock({self._signal._name}, {self._period_ps} ps)"


class ClockCycles(Trigger):
    """Resume after the requested number of active clock edges."""

    def __init__(self, signal, num_cycles, rising=True):
        self._signal = signal
        self._num_cycles = num_cycles
        self._rising = rising

    async def _wait(self):
        kind = 'rising' if self._rising else 'falling'
        sim = _get_sim()
        for _ in range(self._num_cycles):
            reg = sim.register_edge(self._signal, kind)
            try:
                await reg.fut
            finally:
                reg.deregister()
        return self


class Event:
    """Inter-coroutine notification: set(), clear(), is_set(), wait()."""

    def __init__(self, name=None):
        self.name = name
        self.data = None
        self._fired = False
        self._waiters = []

    def set(self, data=None):
        """Wake every coroutine currently waiting on this event."""
        self._fired = True
        self.data = data
        waiters, self._waiters = self._waiters, []
        for fut in waiters:
            if not fut.done():
                fut.set_result(None)

    def clear(self):
        self._fired = False

    def is_set(self):
        return self._fired

    def wait(self):
        return _EventWait(self)

    def __repr__(self):
        return f"Event({self.name or ''} set={self._fired})"


class _EventWait(Trigger):
    def __init__(self, event):
        self._event = event

    async def _wait(self):
        ev = self._event
        loop = asyncio.get_running_loop()
        fut = loop.create_future()
        if ev._fired:
            # Already set: still yield once through the scheduler so a
            # wait-loop cannot starve other coroutines.
            loop.call_soon(fut.set_result, None)
            await fut
            return self
        ev._waiters.append(fut)
        try:
            await fut
        finally:
            try:
                ev._waiters.remove(fut)
            except ValueError:
                pass
        return self


class Lock:
    """Mutual-exclusion lock usable as an async context manager."""

    def __init__(self, name=None):
        self.name = name
        self._locked = False
        self._pending = []

    def locked(self):
        return self._locked

    async def acquire(self):
        if not self._locked:
            self._locked = True
            return
        loop = asyncio.get_running_loop()
        fut = loop.create_future()
        self._pending.append(fut)
        try:
            await fut
        except asyncio.CancelledError:
            if fut in self._pending:
                self._pending.remove(fut)
            elif fut.done() and not fut.cancelled():
                # Ownership was handed to us as we were cancelled;
                # pass it on.
                self.release()
            raise
        # Ownership transferred by release(); self._locked stays True.

    def release(self):
        if not self._locked:
            raise RuntimeError("Lock.release() called on an unlocked lock")
        while self._pending:
            fut = self._pending.pop(0)
            if not fut.done():
                fut.set_result(None)
                return  # ownership handed off; stays locked
        self._locked = False

    async def __aenter__(self):
        await self.acquire()
        return self

    async def __aexit__(self, exc_type, exc, tb):
        self.release()


async def _await_trigger(trigger):
    """Await a trigger/task/coroutine, returning what completing it returns."""
    return await trigger


class First(Trigger):
    """Resume when the first of the supplied triggers completes.

    Returns the object that completed first. Losing triggers are
    cancelled, which removes their scheduler registrations.
    """

    def __init__(self, *triggers):
        if not triggers:
            raise ValueError("First() requires at least one trigger")
        self._triggers = triggers

    async def _wait(self):
        loop = asyncio.get_running_loop()
        tasks = [
            loop.create_task(_await_trigger(t)) for t in self._triggers
        ]
        try:
            done, _pending = await asyncio.wait(
                tasks, return_when=asyncio.FIRST_COMPLETED
            )
        finally:
            for t in tasks:
                if not t.done():
                    t.cancel()
        # Prefer the earliest trigger in argument order among those done.
        for t in tasks:
            if t.done() and not t.cancelled():
                return t.result()
        raise asyncio.CancelledError()


class Combine(Trigger):
    """Resume when every supplied trigger has completed."""

    def __init__(self, *triggers):
        self._triggers = triggers

    async def _wait(self):
        loop = asyncio.get_running_loop()
        tasks = [
            loop.create_task(_await_trigger(t)) for t in self._triggers
        ]
        try:
            await asyncio.gather(*tasks)
        except asyncio.CancelledError:
            for t in tasks:
                if not t.done():
                    t.cancel()
            raise
        return self


async def with_timeout(trigger, timeout_time, timeout_unit=None):
    """Await a trigger/coroutine/task with a simulation-time deadline.

    Returns the awaited result, or raises SimTimeoutError at the exact
    deadline. On timeout the awaited work is cancelled and its trigger
    registrations are removed.
    """
    from arch_cocotb.task import ArchTask

    sim = _get_sim()
    if asyncio.iscoroutine(trigger):
        trigger = sim.schedule(trigger)

    loop = asyncio.get_running_loop()
    wait_task = loop.create_task(_await_trigger(trigger))
    timeout_ps = to_ps(timeout_time, timeout_unit)
    timer_reg = sim.register_timer(timeout_ps)
    timer_fut = timer_reg.fut
    try:
        done, _pending = await asyncio.wait(
            {wait_task, timer_fut}, return_when=asyncio.FIRST_COMPLETED
        )
        if wait_task in done and not wait_task.cancelled():
            return wait_task.result()
        # Deadline hit first.
        wait_task.cancel()
        if isinstance(trigger, ArchTask):
            trigger.kill()
        raise SimTimeoutError(
            f"timed out after {timeout_time} {timeout_unit or 'step'}"
        )
    finally:
        timer_reg.deregister()
        if not wait_task.done():
            wait_task.cancel()
