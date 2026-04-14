"""Test runner for arch_cocotb — runs cocotb-style tests against arch sim models."""

import asyncio
import importlib
import sys
import traceback

from arch_cocotb.dut import ArchDUT
from arch_cocotb.simulator import ArchSimulator, _set_sim
from arch_cocotb.decorators import _test_registry


def run_tests(model_class, test_module_name, time_unit_ns=1):
    """Run all @cocotb.test() decorated tests in a Python module.

    Args:
        model_class: The pybind11 model class (e.g., VCoffeeMachine)
        test_module_name: Name of the Python test module to import
        time_unit_ns: Simulation time unit in nanoseconds (default 1)
    """
    _test_registry.clear()

    # Import the test module — this triggers @cocotb.test() decorators
    if test_module_name in sys.modules:
        del sys.modules[test_module_name]
    mod = importlib.import_module(test_module_name)

    if not _test_registry:
        # Maybe tests were registered via cocotb directly — scan module
        for name in dir(mod):
            obj = getattr(mod, name)
            if callable(obj) and getattr(obj, '_cocotb_test', False):
                _test_registry.append(obj)

    if not _test_registry:
        print(f"WARNING: No @cocotb.test() functions found in {test_module_name}")
        return False

    all_passed = True
    total = len(_test_registry)

    for i, test_fn in enumerate(_test_registry):
        name = test_fn.__name__
        print(f"[{i+1}/{total}] Running {name}...", flush=True)

        dut = ArchDUT(model_class)
        sim = ArchSimulator(dut, time_unit_ns=time_unit_ns)

        try:
            asyncio.run(sim.run_test(test_fn, dut))
            print(f"  PASS: {name}")
        except Exception as e:
            print(f"  FAIL: {name}")
            traceback.print_exc()
            all_passed = False

    passed = sum(1 for _ in range(total)) if all_passed else "?"
    print(f"\n{'='*60}")
    print(f"Results: {total} tests, {'ALL PASSED' if all_passed else 'SOME FAILED'}")
    print(f"{'='*60}")
    return all_passed
