"""Core simulation engine — tick-by-tick async event loop for arch sim."""

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


class ArchSimulator:
    """Drives an arch sim model with deterministic timing.

    The simulator advances time tick-by-tick (1 tick = 1 time unit, typically 1ns).
    At each tick it calls model.eval() and resolves any pending triggers.

    Timing is deterministic because:
    - Python writes to port fields take effect immediately (direct field set)
    - model.eval() is atomic: comb → posedge → comb
    - No VPI callback ordering ambiguity
    """

    def __init__(self, dut, time_unit_ns=1):
        self._dut = dut
        self._time_ns = 0
        self._time_unit_ns = time_unit_ns
        # Timer waiters: heap of (wake_time_ns, id, future)
        self._timer_heap = []
        self._timer_id = 0
        # Edge waiters: signal_name -> [(edge_type, future)]
        self._edge_waiters = {}
        # Previous signal values for edge detection
        self._prev_values = {}
        # Background tasks (from start_soon)
        self._bg_tasks = []
        self._loop = None

    # ── Trigger API (called by RisingEdge, FallingEdge, Timer) ────────

    def wait_rising_edge(self, signal_name):
        """Return a future that resolves on the next rising edge of signal_name."""
        fut = self._loop.create_future()
        self._edge_waiters.setdefault(signal_name, []).append(('rising', fut))
        return fut

    def wait_falling_edge(self, signal_name):
        """Return a future that resolves on the next falling edge of signal_name."""
        fut = self._loop.create_future()
        self._edge_waiters.setdefault(signal_name, []).append(('falling', fut))
        return fut

    def wait_timer(self, duration_ns):
        """Return a future that resolves after duration_ns nanoseconds."""
        fut = self._loop.create_future()
        wake_time = self._time_ns + duration_ns
        self._timer_id += 1
        heapq.heappush(self._timer_heap, (wake_time, self._timer_id, fut))
        return fut

    def schedule(self, coro):
        """Schedule a coroutine to run concurrently (for start_soon)."""
        task = self._loop.create_task(coro)
        self._bg_tasks.append(task)
        return task

    def get_sim_time_ns(self):
        return self._time_ns

    # ── Core simulation loop ──────────────────────────────────────────

    async def run_test(self, test_fn, dut):
        """Run a single test coroutine against the DUT."""
        self._loop = asyncio.get_event_loop()
        _set_sim(self)

        # Snapshot initial signal values for edge detection
        self._snapshot_signals()

        # Start the test coroutine
        test_task = self._loop.create_task(test_fn(dut))

        # Run until test completes or deadlocks
        while not test_task.done():
            # Let pending coroutines run (test + background tasks)
            await asyncio.sleep(0)

            if test_task.done():
                break

            # Advance one tick
            self._tick()

        # Check for exceptions
        if test_task.exception():
            raise test_task.exception()

        # Cancel background tasks
        for t in self._bg_tasks:
            if not t.done():
                t.cancel()
        self._bg_tasks.clear()

    def _tick(self):
        """Advance simulation by one time unit."""
        self._time_ns += self._time_unit_ns

        # Call model.eval() — atomic: comb → posedge → comb
        self._dut._model.eval()

        # Check and resolve timer waiters
        while self._timer_heap and self._timer_heap[0][0] <= self._time_ns:
            _, _, fut = heapq.heappop(self._timer_heap)
            if not fut.done():
                fut.set_result(None)

        # Check and resolve edge waiters
        self._check_edges()

        # Update snapshot for next tick
        self._snapshot_signals()

    def _snapshot_signals(self):
        """Record current signal values for edge detection."""
        for name in list(self._edge_waiters.keys()):
            cpp_name = name
            sig = self._dut._signals.get(name)
            if sig:
                cpp_name = sig._cpp_name
            try:
                self._prev_values[name] = getattr(self._dut._model, cpp_name)
            except AttributeError:
                pass

    def _check_edges(self):
        """Detect rising/falling edges and resolve waiting futures."""
        for name, waiters in list(self._edge_waiters.items()):
            cpp_name = name
            sig = self._dut._signals.get(name)
            if sig:
                cpp_name = sig._cpp_name

            try:
                curr = getattr(self._dut._model, cpp_name)
            except AttributeError:
                continue

            prev = self._prev_values.get(name, curr)
            triggered = []
            remaining = []

            for edge_type, fut in waiters:
                if fut.done():
                    continue
                if edge_type == 'rising' and prev == 0 and curr == 1:
                    triggered.append(fut)
                elif edge_type == 'falling' and prev == 1 and curr == 0:
                    triggered.append(fut)
                else:
                    remaining.append((edge_type, fut))

            self._edge_waiters[name] = remaining
            for fut in triggered:
                fut.set_result(None)
