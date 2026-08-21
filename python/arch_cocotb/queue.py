"""Queue compatibility backed by asyncio's deterministic single-threaded queue."""

from asyncio import Queue, QueueEmpty, QueueFull

__all__ = ["Queue", "QueueEmpty", "QueueFull"]
