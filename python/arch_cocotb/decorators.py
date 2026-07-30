"""Cocotb-compatible decorators and task scheduling."""

import asyncio

from arch_cocotb.simulator import _get_sim

# Registry of test entries decorated with @test().
# Each entry: dict(func, name, timeout_time, timeout_unit, skip,
#                  expect_error, expect_fail)
_test_registry = []


class test:
    """Decorator that registers an async test function.

    Usage:
        @cocotb.test()
        async def test_example(dut):
            ...

    Supported options: timeout_time/timeout_unit (enforced with
    SimTimeoutError), skip, expect_error, expect_fail.
    """

    def __init__(self, timeout_time=None, timeout_unit=None, units=None,
                 expect_error=None, expect_fail=False, skip=False, **kwargs):
        self.timeout_time = timeout_time
        self.timeout_unit = timeout_unit or units or 'step'
        self.expect_error = expect_error
        self.expect_fail = expect_fail
        self.skip = skip

    def __call__(self, func):
        _test_registry.append({
            'func': func,
            'name': func.__name__,
            'timeout_time': self.timeout_time,
            'timeout_unit': self.timeout_unit,
            'skip': self.skip,
            'expect_error': self.expect_error,
            'expect_fail': self.expect_fail,
        })
        func._cocotb_test = True
        return func


def start_soon(coro):
    """Schedule a coroutine to run concurrently; returns a task handle."""
    sim = _get_sim()
    return sim.schedule(coro)


async def start(coro):
    """Schedule a coroutine and yield once so it starts executing."""
    sim = _get_sim()
    task = sim.schedule(coro)
    # One scheduler yield: the new task runs up to its first await
    # before control returns, matching cocotb.start().
    loop = asyncio.get_running_loop()
    fut = loop.create_future()
    loop.call_soon(fut.set_result, None)
    await fut
    return task


def create_task(coro):
    """cocotb 2.x alias used by some libraries."""
    return _get_sim().schedule(coro)
