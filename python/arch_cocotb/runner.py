"""Test runner for arch_cocotb — runs cocotb-style tests against arch sim models."""

import asyncio
import importlib
import logging
import sys
import traceback

from arch_cocotb.dut import ArchDUT
from arch_cocotb.simulator import ArchSimulator
from arch_cocotb.decorators import _test_registry


def _setup_logging():
    root = logging.getLogger("cocotb")
    if not root.handlers:
        handler = logging.StreamHandler(sys.stdout)
        handler.setFormatter(
            logging.Formatter("%(levelname)-8s %(name)s: %(message)s")
        )
        root.addHandler(handler)
        root.setLevel(logging.INFO)


def run_tests(model_class, test_module_name, time_unit_ns=None):
    """Run all @cocotb.test() decorated tests in a Python module.

    Args:
        model_class: The pybind11 model class (e.g., VCoffeeMachine)
        test_module_name: Name of the Python test module to import
        time_unit_ns: Deprecated and ignored (scheduling is event-driven
            at 1 ps resolution).

    Returns True when every non-skipped test passed.
    """
    _setup_logging()
    _test_registry.clear()

    # Import the test module — this triggers @cocotb.test() decorators.
    # A stale sys.modules entry is dropped so each run registers every
    # decorated test exactly once.
    if test_module_name in sys.modules:
        del sys.modules[test_module_name]
    mod = importlib.import_module(test_module_name)

    if not _test_registry:
        # Maybe tests were registered via cocotb directly — scan module
        for name in dir(mod):
            obj = getattr(mod, name)
            if callable(obj) and getattr(obj, '_cocotb_test', False):
                _test_registry.append({
                    'func': obj, 'name': obj.__name__,
                    'timeout_time': None, 'timeout_unit': 'step',
                    'skip': False, 'expect_error': None,
                    'expect_fail': False,
                })

    if not _test_registry:
        print(f"WARNING: No @cocotb.test() functions found in {test_module_name}")
        return False

    entries = list(_test_registry)
    total = len(entries)
    n_pass = n_fail = n_skip = 0

    for i, entry in enumerate(entries):
        name = entry['name']
        if entry['skip']:
            print(f"[{i+1}/{total}] SKIP: {name}", flush=True)
            n_skip += 1
            continue
        print(f"[{i+1}/{total}] Running {name}...", flush=True)

        # Fresh model and scheduler for every test.
        dut = ArchDUT(model_class)
        sim = ArchSimulator(dut)

        test_fn = entry['func']
        if entry['timeout_time'] is not None:
            test_fn = _wrap_timeout(
                test_fn, entry['timeout_time'], entry['timeout_unit']
            )

        failed = False
        exc = None
        try:
            asyncio.run(sim.run_test(test_fn, dut))
        except BaseException as e:  # noqa: BLE001 — report, don't crash
            failed = True
            exc = e

        expect_error = entry['expect_error']
        if expect_error is not None:
            expected = (
                isinstance(exc, expect_error)
                if isinstance(expect_error, (type, tuple))
                else exc is not None
            )
            failed = not expected
        elif entry['expect_fail']:
            failed = not failed

        if failed:
            print(f"  FAIL: {name}")
            if exc is not None:
                traceback.print_exception(exc)
            n_fail += 1
        else:
            print(f"  PASS: {name}")
            n_pass += 1

    print(f"\n{'='*60}")
    print(
        f"Results: {total} tests — {n_pass} passed, {n_fail} failed, "
        f"{n_skip} skipped"
    )
    print(f"{'='*60}")
    return n_fail == 0


def _wrap_timeout(test_fn, timeout_time, timeout_unit):
    from arch_cocotb.triggers import with_timeout

    async def _runner(dut):
        return await with_timeout(test_fn(dut), timeout_time, timeout_unit)

    _runner.__name__ = test_fn.__name__
    return _runner
