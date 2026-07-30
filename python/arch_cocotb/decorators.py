"""Cocotb-compatible test registration and task scheduling."""

import asyncio

from arch_cocotb.simulator import _get_sim


_test_registry = []


class _Test:
    def __init__(
        self,
        timeout_time=None,
        timeout_unit="ns",
        expect_error=None,
        expect_fail=False,
        skip=False,
        **kwargs,
    ):
        self.timeout_time = timeout_time
        self.timeout_unit = timeout_unit
        self.expect_error = expect_error
        self.expect_fail = expect_fail
        self.skip = skip

    def __call__(self, func):
        func._cocotb_test = True
        func._cocotb_test_options = self
        _test_registry.append(func)
        return func


def test(func=None, **kwargs):
    """Register ``@cocotb.test`` or ``@cocotb.test(...)``."""
    decorator = _Test(**kwargs)
    if func is not None:
        return decorator(func)
    return decorator


def start_soon(coro):
    """Schedule a coroutine and return an awaitable task handle."""
    return _get_sim().schedule(coro)


async def start(coro):
    """Deprecated cocotb-compatible scheduling helper."""
    task = start_soon(coro)
    await asyncio.sleep(0)
    return task
