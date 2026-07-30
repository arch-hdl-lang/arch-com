"""Cocotb compatibility shim backed by the native ARCH simulator."""

import logging

from arch_cocotb.decorators import start, start_soon, test
from arch_cocotb.task import ArchTask as Task
from arch_cocotb.triggers import SimTimeoutError, with_timeout
from arch_cocotb import utils

log = logging.getLogger("cocotb")
SIM_NAME = "arch"

__all__ = [
    "SIM_NAME",
    "SimTimeoutError",
    "Task",
    "log",
    "start",
    "start_soon",
    "test",
    "utils",
    "with_timeout",
]
