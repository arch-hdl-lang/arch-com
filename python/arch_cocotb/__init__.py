"""
arch_cocotb — cocotb-compatible adapter for ARCH HDL simulator.

Provides deterministic timing by driving the arch sim C++ model directly
from Python via pybind11, eliminating VPI callback ordering ambiguity.
"""

from arch_cocotb.decorators import test, start_soon, start
from arch_cocotb import utils

__version__ = "0.1.0"
