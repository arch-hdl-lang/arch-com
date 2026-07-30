"""Async queue compatible with cocotb.queue, used by cocotb bus models."""

import asyncio
import collections
import heapq


class QueueFull(asyncio.QueueFull):
    """Raised by put_nowait() on a full queue."""


class QueueEmpty(asyncio.QueueEmpty):
    """Raised by get_nowait() on an empty queue."""


class Queue:
    """A FIFO queue with async put/get and non-blocking variants."""

    def __init__(self, maxsize=0):
        self._maxsize = maxsize
        self._getters = collections.deque()
        self._putters = collections.deque()
        self._init()

    # Subclass hooks (PriorityQueue / LifoQueue override these)
    def _init(self):
        self._queue = collections.deque()

    def _put(self, item):
        self._queue.append(item)

    def _get(self):
        return self._queue.popleft()

    # ── Introspection ────────────────────────────────────────────────

    @property
    def maxsize(self):
        return self._maxsize

    def qsize(self):
        return len(self._queue)

    def empty(self):
        return not self._queue

    def full(self):
        if self._maxsize <= 0:
            return False
        return self.qsize() >= self._maxsize

    # ── Waiter plumbing ──────────────────────────────────────────────

    @staticmethod
    def _wakeup_next(waiters):
        while waiters:
            fut = waiters.popleft()
            if not fut.done():
                fut.set_result(None)
                break

    # ── Blocking API ─────────────────────────────────────────────────

    async def put(self, item):
        while self.full():
            fut = asyncio.get_running_loop().create_future()
            self._putters.append(fut)
            try:
                await fut
            except asyncio.CancelledError:
                if fut.done() and not fut.cancelled():
                    # We were woken then cancelled: pass the slot on.
                    self._wakeup_next(self._putters)
                raise
        self.put_nowait(item)

    async def get(self):
        while self.empty():
            fut = asyncio.get_running_loop().create_future()
            self._getters.append(fut)
            try:
                await fut
            except asyncio.CancelledError:
                if fut.done() and not fut.cancelled():
                    self._wakeup_next(self._getters)
                raise
        return self.get_nowait()

    # ── Non-blocking API ─────────────────────────────────────────────

    def put_nowait(self, item):
        if self.full():
            raise QueueFull()
        self._put(item)
        self._wakeup_next(self._getters)

    def get_nowait(self):
        if self.empty():
            raise QueueEmpty()
        item = self._get()
        self._wakeup_next(self._putters)
        return item

    def __repr__(self):
        return f"<{type(self).__name__} qsize={self.qsize()}>"


class PriorityQueue(Queue):
    """Queue returning entries in priority order (lowest first)."""

    def _init(self):
        self._queue = []

    def _put(self, item):
        heapq.heappush(self._queue, item)

    def _get(self):
        return heapq.heappop(self._queue)


class LifoQueue(Queue):
    """Queue returning the most recently added entry first."""

    def _init(self):
        self._queue = []

    def _put(self, item):
        self._queue.append(item)

    def _get(self):
        return self._queue.pop()
