"""Core simulation engine — event-driven scheduler for arch sim.

Time model
----------
Internal time is an integer count of **picoseconds**. All triggers
(`Timer`, `Clock`, timeouts) and `get_sim_time()` share this one time
base; durations convert exactly (round-to-nearest for sub-picosecond
remainders, e.g. femtosecond inputs).

Scheduling model
----------------
The scheduler is event-driven: it advances directly to the next timer
deadline instead of ticking once per nanosecond. Within one timestep the
observable phase order is:

1. Runnable coroutines execute (testbench writes apply immediately to
   the model's fields and mark the model dirty).
2. When no coroutine is runnable and the model is dirty, ``eval()``
   runs (comb settle -> clock-edge sequential logic -> comb settle).
3. Edge triggers whose condition became true resume; back to 1.
4. When fully settled, ``ReadWrite`` waiters resume, then ``ReadOnly``
   waiters resume — still in the same timestep, with no time advance.
5. Only when the timestep is quiescent does time jump to the next
   deadline; ``NextTimeStep`` waiters resume there.

A model evaluation is triggered by a testbench write, a clock
transition, or conservatively after any timer callback ran (a callback
may alter model inputs through raw field access the shim cannot see).
"""

import asyncio
import heapq

# Global simulator instance for the current test
_sim_instance = None


def _get_sim():
    assert _sim_instance is not None, "No simulator running"
    return _sim_instance


def _set_sim(sim):
    global _sim_instance
    _sim_instance = sim


# ── Exact time conversion ─────────────────────────────────────────────

# Femtoseconds per unit; internal resolution is 1 ps ('step' == 1 ps).
_UNIT_TO_FS = {
    'step': 1_000,
    'fs': 1,
    'ps': 1_000,
    'ns': 1_000_000,
    'us': 1_000_000_000,
    'ms': 1_000_000_000_000,
    'sec': 1_000_000_000_000_000,
    's': 1_000_000_000_000_000,
}


def _normalize_unit(unit):
    if unit is None:
        return 'step'
    u = str(unit).lower()
    if u in _UNIT_TO_FS:
        return u
    # tolerate 'psec', 'nsec', 'usec', 'msec' spellings
    if u.endswith('sec') and (u[:-3] + 's') in _UNIT_TO_FS:
        return u[:-3] + 's'
    raise ValueError(f"unknown time unit {unit!r}")


def to_ps(duration, unit='step'):
    """Convert a duration to integer picoseconds, exactly.

    Integer inputs convert with pure integer arithmetic. Float inputs
    round to the nearest femtosecond first, then to the nearest
    picosecond, so values like 0.5 ns are exact.
    """
    u = _normalize_unit(unit)
    fs_per = _UNIT_TO_FS[u]
    if isinstance(duration, int):
        fs = duration * fs_per
    else:
        fs = round(float(duration) * fs_per)
    ps, rem = divmod(fs, 1000)
    if rem >= 500:
        ps += 1
    return int(ps)


def ps_to_unit(time_ps, unit='step'):
    """Convert integer picoseconds to the requested unit."""
    u = _normalize_unit(unit)
    if u == 'step':
        return time_ps
    if u == 'fs':
        return time_ps * 1000
    return time_ps * 1000.0 / _UNIT_TO_FS[u]


# ── Trigger registrations ─────────────────────────────────────────────


class _Registration:
    """A cancellable trigger registration.

    Holds the future a coroutine awaits plus enough context to remove
    the registration when the await is abandoned (task killed, `First`
    race lost). Deregistration is idempotent.
    """

    __slots__ = ('fut', '_container', '_on_dereg')

    def __init__(self, fut, container=None, on_dereg=None):
        self.fut = fut
        self._container = container
        self._on_dereg = on_dereg

    def deregister(self):
        if self._container is not None:
            try:
                self._container.remove(self)
            except ValueError:
                pass
            self._container = None
        if self._on_dereg is not None:
            cb, self._on_dereg = self._on_dereg, None
            cb()
        if not self.fut.done():
            self.fut.cancel()


class _EdgeReg(_Registration):
    __slots__ = ('kind',)

    def __init__(self, fut, container, kind):
        super().__init__(fut, container)
        self.kind = kind


class ArchSimulator:
    """Drives an arch sim pybind model with cocotb-style scheduling."""

    # Safety caps: hard bounds on scheduler spins inside one timestep so
    # pathological user code raises instead of hanging the process.
    _DRAIN_CAP = 100_000
    _READONLY_ROUNDS_CAP = 1_000

    def __init__(self, dut, time_unit_ns=None):
        # time_unit_ns is accepted for backward compatibility and
        # ignored: the scheduler is event-driven at 1 ps resolution.
        self._dut = dut
        self._time_ps = 0
        self._loop = None
        # Timers: heap of (deadline_ps, seq, fut)
        self._timer_heap = []
        self._timer_seq = 0
        self._active_timers = 0
        # Edge waiters: cpp_name -> [ _EdgeReg ]
        self._edge_waiters = {}
        self._prev_values = {}
        # Phase waiters
        self._readwrite_waiters = []
        self._readonly_waiters = []
        self._nextstep_waiters = []
        # Model state: start dirty so the model settles before user code
        # reads anything.
        self._dirty = True
        # Buffered `.value =` deposits: list of (signal, value). Applied
        # at the next scheduler sync point, mirroring cocotb's write
        # timing (setimmediatevalue bypasses this buffer).
        self._deposits = []
        # Tasks spawned via start_soon (includes clock generators)
        self._bg_tasks = []

    # ── Introspection ────────────────────────────────────────────────

    @property
    def time_ps(self):
        return self._time_ps

    def get_sim_time_ps(self):
        return self._time_ps

    # ── Write notification ───────────────────────────────────────────

    def notify_write(self):
        """A testbench write changed a model input; eval before reads."""
        self._dirty = True

    def deposit(self, signal, value):
        """Buffer a `.value =` write until the next scheduler sync point.

        This mirrors cocotb's write timing: a deposit made in reaction
        to a clock edge is not visible to the sequential logic of that
        same edge.
        """
        self._deposits.append((signal, value))

    def _apply_deposits(self):
        deposits, self._deposits = self._deposits, []
        for sig, value in deposits:
            sig._apply_raw(value)
        if deposits:
            self._dirty = True
        return bool(deposits)

    # ── Trigger API ──────────────────────────────────────────────────

    def register_timer(self, delay_ps):
        """Register a wakeup `delay_ps` picoseconds from now."""
        if delay_ps <= 0:
            raise ValueError("timer duration must be a positive time")
        fut = self._loop.create_future()
        self._timer_seq += 1
        heapq.heappush(
            self._timer_heap, (self._time_ps + delay_ps, self._timer_seq, fut)
        )
        self._active_timers += 1

        def _on_dereg():
            # The heap entry is removed lazily; keep the live count exact
            # so deadlock detection stays accurate.
            self._active_timers -= 1

        return _Registration(fut, on_dereg=_on_dereg)

    def register_edge(self, signal, kind):
        """Register an edge waiter. kind: 'rising' | 'falling' | 'any'."""
        name = signal._cpp_name
        fut = self._loop.create_future()
        waiters = self._edge_waiters.setdefault(name, [])
        if not waiters:
            # First waiter on this signal: capture the current value as
            # the edge-detection baseline.
            self._prev_values[name] = self._read_raw(name)
        reg = _EdgeReg(fut, waiters, kind)
        waiters.append(reg)
        return reg

    def register_readwrite(self):
        fut = self._loop.create_future()
        reg = _Registration(fut, self._readwrite_waiters)
        self._readwrite_waiters.append(reg)
        return reg

    def register_readonly(self):
        fut = self._loop.create_future()
        reg = _Registration(fut, self._readonly_waiters)
        self._readonly_waiters.append(reg)
        return reg

    def register_nextstep(self):
        fut = self._loop.create_future()
        reg = _Registration(fut, self._nextstep_waiters)
        self._nextstep_waiters.append(reg)
        return reg

    def schedule(self, coro, name=None):
        """Schedule a coroutine to run concurrently (for start_soon)."""
        from arch_cocotb.task import ArchTask

        task = self._loop.create_task(coro)
        self._bg_tasks.append(task)
        return ArchTask(task, name=name)

    # ── Model access ─────────────────────────────────────────────────

    def _read_raw(self, cpp_name):
        try:
            return int(getattr(self._dut._model, cpp_name))
        except AttributeError:
            return 0

    def _eval(self):
        self._dut._model.eval()
        self._dirty = False

    # ── Scheduler core ───────────────────────────────────────────────

    async def _drain(self):
        """Yield to the event loop until no callback is immediately runnable.

        A sentinel future is queued behind the currently-ready
        callbacks; when the loop's ready queue is empty after our wake,
        every runnable coroutine has run. Falls back to a fixed number
        of yields if the loop does not expose its ready queue.
        """
        loop = self._loop
        ready = getattr(loop, '_ready', None)
        if ready is None:
            for _ in range(50):
                await asyncio.sleep(0)
            return
        for _ in range(self._DRAIN_CAP):
            fut = loop.create_future()
            loop.call_soon(fut.set_result, None)
            await fut
            if not ready:
                return
        raise RuntimeError(
            "arch_cocotb: simulation cannot settle — a coroutine is "
            "spinning without awaiting a simulator trigger"
        )

    def _fire_edges(self):
        """Resolve edge waiters whose transition occurred. Returns count."""
        fired = 0
        for name, waiters in self._edge_waiters.items():
            if not waiters:
                continue
            curr = self._read_raw(name)
            prev = self._prev_values.get(name, curr)
            self._prev_values[name] = curr
            if curr == prev:
                continue
            rising = prev == 0 and curr != 0
            falling = prev != 0 and curr == 0
            remaining = []
            for reg in waiters:
                if reg.fut.done():
                    continue  # deregistered: drop
                hit = (
                    reg.kind == 'any'
                    or (reg.kind == 'rising' and rising)
                    or (reg.kind == 'falling' and falling)
                )
                if hit:
                    reg.fut.set_result(None)
                    fired += 1
                else:
                    remaining.append(reg)
            waiters[:] = remaining
        return fired

    @staticmethod
    def _fire_list(waiters):
        fired = 0
        pending, waiters[:] = waiters[:], []
        for reg in pending:
            if not reg.fut.done():
                reg.fut.set_result(None)
                fired += 1
        return fired

    async def _settle_timestep(self):
        """Run the current timestep to quiescence, including RW/RO phases.

        Region ordering per applied write batch (mirrors event-driven
        simulator semantics, which cocotb bus models depend on):

        1. Deposits apply to the model's input fields.
        2. Edge waiters on directly-written signals (e.g. the clock)
           resume BEFORE eval — their reads see pre-edge register
           state, exactly like a cocotb coroutine woken in the active
           region before the NBA updates land.
        3. Their reaction writes buffer as new deposits.
        4. eval() runs: sequential logic samples the pre-reaction
           input values.
        5. Edge waiters on eval-derived signals (registered outputs)
           resume after eval, like NBA-driven value-change callbacks.
        6. The loop repeats, applying the reaction deposits and comb-
           settling them; ReadOnly then observes the settled state.
        """
        readonly_rounds = 0
        while True:
            await self._drain()
            if self._dirty:
                self._eval()
                self._fire_edges()
                # Woken edge waiters may write and re-dirty the model;
                # loop to drain them.
                continue
            if self._deposits:
                self._apply_deposits()
                if self._fire_edges():
                    # Pre-eval waiters (active region): let them run and
                    # buffer their reaction writes before the edge is
                    # evaluated.
                    await self._drain()
                self._eval()
                self._fire_edges()
                continue
            if self._readwrite_waiters:
                self._fire_list(self._readwrite_waiters)
                continue
            if self._readonly_waiters:
                readonly_rounds += 1
                if readonly_rounds > self._READONLY_ROUNDS_CAP:
                    raise RuntimeError(
                        "arch_cocotb: ReadOnly() livelock — a coroutine "
                        "re-awaits ReadOnly() without advancing time"
                    )
                self._fire_list(self._readonly_waiters)
                continue
            return

    def _advance(self):
        """Jump to the next timer deadline and fire everything due there."""
        heap = self._timer_heap
        # Drop deregistered entries at the head.
        while heap and heap[0][2].done():
            heapq.heappop(heap)
        if not heap:
            waiting = sorted(
                name for name, w in self._edge_waiters.items() if w
            )
            raise RuntimeError(
                "arch_cocotb: deadlock — no timers pending and the test "
                "has not finished"
                + (f"; coroutines are waiting on edges of: {waiting}"
                   if waiting else "")
            )
        deadline = heap[0][0]
        assert deadline > self._time_ps, "timer deadline not in the future"
        self._time_ps = deadline
        while heap and heap[0][0] <= deadline:
            _, _, fut = heapq.heappop(heap)
            if not fut.done():
                fut.set_result(None)
                self._active_timers -= 1
        # NextTimeStep waiters resume as time moves.
        self._fire_list(self._nextstep_waiters)
        # Conservative: a woken callback may alter model inputs through
        # raw field access this shim cannot observe.
        self._dirty = True

    # ── Test execution ───────────────────────────────────────────────

    async def run_test(self, test_fn, dut):
        """Run a single test coroutine against the DUT."""
        self._loop = asyncio.get_running_loop()
        _set_sim(self)
        try:
            test_task = self._loop.create_task(test_fn(dut))
            while True:
                await self._settle_timestep()
                if test_task.done():
                    break
                self._advance()
            if test_task.cancelled():
                raise RuntimeError("test task was cancelled")
            exc = test_task.exception()
            if exc is not None:
                raise exc
            return test_task.result()
        finally:
            # Cancel background tasks (clocks, monitors) cleanly; their
            # trigger registrations deregister as the cancellations
            # unwind during the drain.
            for t in self._bg_tasks:
                if not t.done():
                    t.cancel()
            try:
                await self._drain()
            except RuntimeError:
                pass
            self._bg_tasks.clear()
            _set_sim(None)

    # ── Backward-compatible helpers (pre-event-driven API) ──────────

    def wait_timer(self, duration_ns):
        """Deprecated: nanosecond timer wait kept for old tests."""
        reg = self.register_timer(to_ps(duration_ns, 'ns'))
        return reg.fut

    def wait_rising_edge(self, signal_name):
        """Deprecated: edge wait by name kept for old tests."""
        sig = self._dut._signals.get(signal_name)
        if sig is None:
            raise AttributeError(f"No signal '{signal_name}' on DUT")
        return self.register_edge(sig, 'rising').fut

    def wait_falling_edge(self, signal_name):
        """Deprecated: edge wait by name kept for old tests."""
        sig = self._dut._signals.get(signal_name)
        if sig is None:
            raise AttributeError(f"No signal '{signal_name}' on DUT")
        return self.register_edge(sig, 'falling').fut

    def get_sim_time_ns(self):
        """Deprecated: integer-nanosecond time kept for old tests."""
        return self._time_ps // 1000
