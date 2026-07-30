"""Cocotb result shim (legacy import location for SimTimeoutError)."""

from arch_cocotb.triggers import SimTimeoutError


class TestSuccess(Exception):
    """Raise to end a test early with a pass."""
