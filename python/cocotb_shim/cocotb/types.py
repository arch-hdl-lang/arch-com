"""Cocotb types shim."""

class Logic:
    """Stub for cocotb.types.Logic."""
    def __init__(self, value=0):
        self._value = int(value)

    def to_unsigned(self):
        return self._value

    def to_signed(self):
        return self._value

    def __int__(self):
        return self._value
