"""Event-driven asyncio scheduler for the native ARCH simulation model."""

import asyncio
import heapq

from arch_cocotb.task import ArchTask


_sim_instance = None


def _get_sim():
    if _sim_instance is None:
        raise RuntimeError("No ARCH simulator is running")
    return _sim_instance


def _set_sim(sim):
    global _sim_instance
    _sim_instance = sim


class ArchSimDeadlock(RuntimeError):
    """Raised when coroutines are blocked and no future event can wake them."""


class ArchSimulator:
    """Drive a pybind ARCH model using integer-picosecond event scheduling.

    A model evaluation is the native simulator's atomic hardware step:
    pending Python writes are applied, combinational logic settles, sequential
    logic observes any clock edge, and combinational logic settles again.
    Python coroutines then resume from edge triggers before ``ReadOnly`` waiters
    are released in the same simulation timestamp.
    """

    _MAX_DELTA_CYCLES = 10000

    def __init__(self, dut, time_unit_ns=1):
        self._dut = dut
        self._time_ps = 0
        self._step_ps = int(time_unit_ns * 1000)
        if self._step_ps <= 0:
            raise ValueError("time_unit_ns must describe at least one picosecond")
        self._timer_heap = []
        self._timer_id = 0
        self._edge_waiters = {}
        self._readonly_waiters = []
        self._registrations = {}
        self._last_values = {}
        self._tasks = set()
        self._task_handles = {}
        self._loop = None
        self._dirty = False
        self._activity = 0
        supports_phased_eval = getattr(
            type(self._dut._model),
            "_arch_supports_phased_eval",
            None,
        )
        self._supports_phased_eval = bool(
            supports_phased_eval is not None and supports_phased_eval()
        )
        self._post_edge_comb_pending = False

    # Trigger registration -------------------------------------------------

    def _future(self):
        if self._loop is None:
            raise RuntimeError("ARCH simulator has not started")
        return self._loop.create_future()

    def wait_edge(self, signal, edge_type, result):
        if edge_type not in ("rising", "falling", "either"):
            raise ValueError(f"unknown edge type: {edge_type}")
        fut = self._future()
        name = signal._name
        self._last_values.setdefault(name, int(signal.value))
        registration = (edge_type, fut, result)
        self._edge_waiters.setdefault(name, []).append(registration)
        self._registrations[fut] = ("edge", name, registration)
        fut.add_done_callback(self._remove_registration)
        self._activity += 1
        return fut

    def wait_timer(self, duration_ps, result=None):
        if duration_ps < 0:
            raise ValueError("Timer duration must not be negative")
        fut = self._future()
        wake_time = self._time_ps + duration_ps
        self._timer_id += 1
        registration = (wake_time, self._timer_id, fut, result)
        heapq.heappush(self._timer_heap, registration)
        self._registrations[fut] = ("timer", registration)
        fut.add_done_callback(self._remove_registration)
        self._activity += 1
        return fut

    def wait_readonly(self, result):
        fut = self._future()
        registration = (fut, result)
        self._readonly_waiters.append(registration)
        self._registrations[fut] = ("readonly", registration)
        fut.add_done_callback(self._remove_registration)
        self._activity += 1
        return fut

    def _remove_registration(self, fut):
        registration = self._registrations.pop(fut, None)
        if registration is None:
            return
        kind = registration[0]
        if kind == "edge":
            name, item = registration[1], registration[2]
            waiters = self._edge_waiters.get(name, [])
            if item in waiters:
                waiters.remove(item)
            if not waiters:
                self._edge_waiters.pop(name, None)
                self._last_values.pop(name, None)
        elif kind == "timer":
            item = registration[1]
            if item in self._timer_heap:
                self._timer_heap.remove(item)
                heapq.heapify(self._timer_heap)
        else:
            item = registration[1]
            if item in self._readonly_waiters:
                self._readonly_waiters.remove(item)

    # Task and write API ---------------------------------------------------

    def schedule(self, coro):
        task = self._loop.create_task(coro)
        handle = ArchTask(task, self)
        self._tasks.add(task)
        self._task_handles[task] = handle
        task.add_done_callback(self._task_done)
        self._activity += 1
        return handle

    def _task_done(self, task):
        self._activity += 1

    def signal_written(self):
        self._dirty = True
        self._activity += 1

    def get_sim_time_ps(self):
        return self._time_ps

    @property
    def step_ps(self):
        return self._step_ps

    # Core scheduler -------------------------------------------------------

    async def run_test(self, test_fn, dut):
        self._loop = asyncio.get_running_loop()
        _set_sim(self)
        self._dut._attach_simulator(self)
        test_task = self._loop.create_task(test_fn(dut))
        try:
            # Let the test's time-zero setup run before the first model
            # evaluation. This matches cocotb's initialization phase and is
            # essential for --inputs-start-uninit: a test must be able to
            # drive inputs before combinational logic first reads them.
            self._dirty = True

            while not test_task.done():
                await self._quiesce()
                self._raise_unhandled_background()
                if test_task.done():
                    break

                if self._readonly_waiters:
                    self._fire_readonly()
                    continue

                if self._timer_heap:
                    self._advance_to_next_timer()
                    continue

                raise ArchSimDeadlock(
                    "simulation deadlock: test is waiting, but no timer or "
                    "runnable coroutine can advance the model"
                )

            # Let completion callbacks and immediately-woken background tasks
            # run before deciding whether a task exception was unhandled.
            await self._quiesce()
            if test_task.cancelled():
                raise asyncio.CancelledError()
            exception = test_task.exception()
            if exception is not None:
                raise exception
            self._raise_unhandled_background()
            return test_task.result()
        finally:
            await self._cleanup(test_task)
            self._dut._attach_simulator(None)
            _set_sim(None)

    async def _quiesce(self):
        """Run ready coroutines and hardware deltas until nothing changes."""
        stable_passes = 0
        for _ in range(self._MAX_DELTA_CYCLES):
            before = self._activity
            await asyncio.sleep(0)
            if self._post_edge_comb_pending:
                self._evaluate_post_edge()
            elif self._dirty:
                self._evaluate_model()
            # asyncio callbacks created by First/Event/Queue are not all ARCH
            # tasks and therefore do not increment `_activity`. If the loop
            # still has ready callbacks, advancing simulation time here would
            # let a later timer beat an already-completed earlier trigger.
            asyncio_ready = bool(getattr(self._loop, "_ready", ()))
            if self._activity == before and not self._dirty and not asyncio_ready:
                stable_passes += 1
                if stable_passes >= 3:
                    return
            else:
                stable_passes = 0
        raise RuntimeError(
            "simulation did not settle after "
            f"{self._MAX_DELTA_CYCLES} zero-time delta cycles"
        )

    def _evaluate_model(self):
        if self._supports_phased_eval:
            self._evaluate_pre_edge()
            return

        self._dirty = False
        previous = dict(self._last_values)
        self._dut._model.eval()
        self._fire_changed_edges(previous)

    def _evaluate_pre_edge(self):
        """Evaluate state updates while preserving pre-edge sampled outputs.

        cocotb bus agents sample ready/valid at a clock edge, then drive their
        next-cycle values.  A generated model's all-in-one ``eval()`` performs
        its final combinational pass before Python gets the clock trigger,
        exposing post-edge outputs too early.  Split-capable models therefore
        run comb + sequential logic first, wake input-edge waiters, and defer
        the post-edge comb pass until those coroutines have sampled and driven.
        """

        self._dirty = False
        previous = dict(self._last_values)
        self._dut._model.eval_comb()
        self._dut._model.eval_posedge()
        self._fire_changed_edges(previous, inputs_only=True)
        self._post_edge_comb_pending = True

    def _evaluate_post_edge(self):
        """Finish a split hardware step after clock-edge coroutines run."""

        self._post_edge_comb_pending = False
        self._dirty = False
        previous = dict(self._last_values)
        self._dut._model.eval_comb()
        self._fire_changed_edges(previous)

    def _fire_changed_edges(self, previous, inputs_only=False):
        for name, waiters in list(self._edge_waiters.items()):
            if not waiters:
                continue
            signal = self._dut._signals[name]
            if inputs_only and not signal._is_input:
                continue
            current = int(signal.value)
            prior = previous.get(name, current)
            fired = []
            for edge_type, fut, result in list(waiters):
                rising = prior == 0 and current != 0
                falling = prior != 0 and current == 0
                if (
                    (edge_type == "rising" and rising)
                    or (edge_type == "falling" and falling)
                    or (edge_type == "either" and prior != current)
                ):
                    fired.append((fut, result))
            self._last_values[name] = current
            for fut, result in fired:
                if not fut.done():
                    fut.set_result(result)
                    self._activity += 1

    def _fire_readonly(self):
        waiters = list(self._readonly_waiters)
        for fut, result in waiters:
            if not fut.done():
                fut.set_result(result)
                self._activity += 1

    def _advance_to_next_timer(self):
        wake_time = self._timer_heap[0][0]
        if wake_time < self._time_ps:
            raise RuntimeError("timer heap moved simulation time backwards")
        self._time_ps = wake_time
        due = []
        while self._timer_heap and self._timer_heap[0][0] == wake_time:
            due.append(heapq.heappop(self._timer_heap))
        for _, _, fut, result in due:
            if not fut.done():
                fut.set_result(result)
                self._activity += 1

    def _raise_unhandled_background(self):
        for task in list(self._tasks):
            if not task.done() or task.cancelled():
                continue
            handle = self._task_handles[task]
            if handle._observed:
                continue
            exception = task.exception()
            if exception is not None:
                handle._observed = True
                raise RuntimeError("unhandled background task exception") from exception

    async def _cleanup(self, test_task):
        pending = [task for task in self._tasks if not task.done()]
        if not test_task.done():
            pending.append(test_task)
        for task in pending:
            task.cancel()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)

        for fut in list(self._registrations):
            if not fut.done():
                fut.cancel()
        self._timer_heap.clear()
        self._edge_waiters.clear()
        self._readonly_waiters.clear()
        self._registrations.clear()
        self._last_values.clear()
        self._tasks.clear()
        self._task_handles.clear()
        self._post_edge_comb_pending = False
