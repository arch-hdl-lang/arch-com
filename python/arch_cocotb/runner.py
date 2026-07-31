"""Test runner for cocotb-style tests on native ARCH simulation models."""

import asyncio
import importlib
import sys
import traceback

from arch_cocotb.decorators import _test_registry
from arch_cocotb.dut import ArchDUT
from arch_cocotb.result import TestSuccess
from arch_cocotb.simulator import ArchSimulator
from arch_cocotb.triggers import with_timeout


def run_tests(model_class, test_module_name, time_unit_ns=1):
    """Import a module and run all of its registered cocotb tests."""
    _test_registry.clear()
    if test_module_name in sys.modules:
        del sys.modules[test_module_name]
    module = importlib.import_module(test_module_name)
    if not _test_registry:
        for name in dir(module):
            candidate = getattr(module, name)
            if callable(candidate) and getattr(candidate, "_cocotb_test", False):
                _test_registry.append(candidate)
    return run_registered_tests(model_class, time_unit_ns=time_unit_ns)


def run_registered_tests(model_class, time_unit_ns=1):
    """Run the current registry without importing the test module again."""
    tests = list(dict.fromkeys(_test_registry))
    if not tests:
        print("WARNING: No @cocotb.test functions were registered")
        return False

    results = []
    total = len(tests)
    for index, test_fn in enumerate(tests, start=1):
        name = test_fn.__name__
        options = getattr(test_fn, "_cocotb_test_options", None)
        if options is not None and options.skip:
            print(f"[{index}/{total}] SKIP: {name}", flush=True)
            results.append((name, "SKIP"))
            continue

        print(f"[{index}/{total}] Running {name}...", flush=True)
        dut = ArchDUT(model_class, name=getattr(model_class, "__name__", "dut"))
        simulator = ArchSimulator(dut, time_unit_ns=time_unit_ns)

        async def invoke(active_dut):
            coroutine = test_fn(active_dut)
            if options is not None and options.timeout_time is not None:
                return await with_timeout(
                    coroutine,
                    options.timeout_time,
                    options.timeout_unit,
                )
            return await coroutine

        error = None
        try:
            asyncio.run(simulator.run_test(invoke, dut))
        except BaseException as caught:
            error = caught

        status = _classify_result(options, error)
        results.append((name, status))
        print(f"  {status}: {name}")
        if status == "FAIL" and error is not None:
            traceback.print_exception(type(error), error, error.__traceback__)

    passed = sum(status == "PASS" for _, status in results)
    failed = sum(status == "FAIL" for _, status in results)
    skipped = sum(status == "SKIP" for _, status in results)
    print()
    print("=" * 60)
    print(
        f"Results: {total} tests; {passed} passed, "
        f"{failed} failed, {skipped} skipped"
    )
    print("=" * 60)
    return failed == 0


def _classify_result(options, error):
    if isinstance(error, TestSuccess):
        return "PASS"

    expect_error = None if options is None else options.expect_error
    expect_fail = False if options is None else options.expect_fail

    if expect_error is not None:
        expected = expect_error if isinstance(expect_error, tuple) else (expect_error,)
        return "PASS" if isinstance(error, expected) else "FAIL"
    if expect_fail:
        return "PASS" if error is not None else "FAIL"
    return "PASS" if error is None else "FAIL"
