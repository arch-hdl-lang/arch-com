"""Legacy cocotb result exceptions supported by the native shim."""


class TestSuccess(Exception):
    """Raise to end a test early with a passing result."""
