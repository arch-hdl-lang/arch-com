"""Task handles returned by cocotb.start_soon / cocotb.start."""

import asyncio


class ArchTask:
    """Wraps an asyncio.Task with the cocotb Task surface.

    Supports awaiting, ``done()``, ``result()``, ``kill()`` /
    ``cancel()``, and ``join()``. Killing a task cancels the underlying
    asyncio task; any trigger the task was awaiting deregisters itself
    as the cancellation unwinds (see the try/finally in each trigger's
    ``_wait``), so no stale registrations remain and the task can write
    no further signals.
    """

    def __init__(self, task, name=None):
        self._task = task
        self._name = name or getattr(task, 'get_name', lambda: 'task')()

    def __await__(self):
        return self._task.__await__()

    def done(self):
        return self._task.done()

    def cancelled(self):
        return self._task.cancelled()

    def result(self):
        return self._task.result()

    def exception(self):
        return self._task.exception()

    def kill(self):
        """Stop the task; no further statements in it will run."""
        self._task.cancel()

    # cocotb 2.x name for kill()
    def cancel(self, msg=None):
        self._task.cancel(msg)

    def join(self):
        """Return a trigger that completes when this task finishes."""
        return _Join(self)

    def __repr__(self):
        state = 'done' if self._task.done() else 'running'
        return f"<ArchTask {self._name} {state}>"


class _Join:
    """Awaitable that completes when the joined task finishes.

    Unlike awaiting the task directly, joining a killed task does not
    raise CancelledError — it simply completes, matching cocotb.
    """

    def __init__(self, arch_task):
        self._arch_task = arch_task

    def __await__(self):
        return self._wait().__await__()

    async def _wait(self):
        try:
            await asyncio.shield(self._arch_task._task)
        except asyncio.CancelledError:
            # If *we* were cancelled, re-raise; if the joined task was
            # killed, treat the join as complete.
            cur = asyncio.current_task()
            if cur is not None and cur.cancelled():
                raise
            if not self._arch_task._task.cancelled():
                raise
        except Exception:
            # The task raising is surfaced via task.result(), not join.
            pass
        return self
