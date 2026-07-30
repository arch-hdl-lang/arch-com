"""Cocotb-compatible task wrapper for the native ARCH scheduler."""


class ArchTask:
    """Small compatibility wrapper around :class:`asyncio.Task`."""

    def __init__(self, task, simulator):
        self._task = task
        self._simulator = simulator
        self._observed = False

    def __await__(self):
        self._observed = True
        return self._task.__await__()

    def done(self):
        return self._task.done()

    def result(self):
        self._observed = True
        return self._task.result()

    def exception(self):
        self._observed = True
        return self._task.exception()

    def kill(self):
        self._task.cancel()

    cancel = kill

    def cancelled(self):
        return self._task.cancelled()
