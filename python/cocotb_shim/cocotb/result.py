"""Compatibility result exceptions for legacy cocotb tests."""

from arch_cocotb.result import TestSuccess
from arch_cocotb.triggers import SimTimeoutError

__all__ = ["SimTimeoutError", "TestSuccess"]
