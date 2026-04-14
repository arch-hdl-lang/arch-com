"""Cocotb-compatible decorators and task scheduling."""

from arch_cocotb.simulator import _get_sim

# Registry of test functions decorated with @test()
_test_registry = []


class test:
    """Decorator that registers an async test function.

    Usage:
        @cocotb.test()
        async def test_example(dut):
            ...
    """

    def __init__(self, timeout_time=None, timeout_unit='ns', expect_error=None,
                 expect_fail=False, skip=False, **kwargs):
        self.timeout_time = timeout_time
        self.skip = skip

    def __call__(self, func):
        if not self.skip:
            _test_registry.append(func)
        func._cocotb_test = True
        return func


def start_soon(coro):
    """Schedule a coroutine to run concurrently (like cocotb.start_soon)."""
    sim = _get_sim()
    return sim.schedule(coro)


async def start(coro):
    """Schedule a coroutine and return immediately (like cocotb.start)."""
    sim = _get_sim()
    return sim.schedule(coro)
