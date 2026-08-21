"""Minimal two-state cocotb value types used by protocol libraries."""

from arch_cocotb.signal import _coerce_integer


class Logic:
    """A single two-state logic digit; X and Z resolve to zero."""

    def __init__(self, value=0):
        if isinstance(value, str) and value.lower() in ("x", "z"):
            value = 0
        self._value = int(value) & 1

    @property
    def is_resolvable(self):
        return True

    def to_unsigned(self):
        return self._value

    def to_signed(self):
        return self._value

    def __int__(self):
        return self._value

    def __index__(self):
        return self._value

    def __bool__(self):
        return bool(self._value)

    def __str__(self):
        return str(self._value)


class LogicArray:
    """Integer-backed subset of cocotb's ``LogicArray`` API."""

    def __init__(self, value=0, range=None, *, width=None):
        if width is None:
            if range is not None:
                try:
                    width = len(range)
                except TypeError:
                    width = abs(range.left - range.right) + 1
            elif isinstance(value, str):
                width = len(value)
            else:
                width = max(1, int(value).bit_length())
        self._width = int(width)
        self._value = _coerce_integer(value) & ((1 << self._width) - 1)

    @property
    def integer(self):
        return self.to_unsigned()

    @property
    def signed_integer(self):
        return self.to_signed()

    @property
    def binstr(self):
        return format(self._value, f"0{self._width}b")

    @property
    def is_resolvable(self):
        return True

    def to_unsigned(self):
        return self._value

    def to_signed(self):
        if self._width and self._value >= (1 << (self._width - 1)):
            return self._value - (1 << self._width)
        return self._value

    def __len__(self):
        return self._width

    def __int__(self):
        return self._value

    def __index__(self):
        return self._value

    def __bool__(self):
        return bool(self._value)

    def __str__(self):
        return self.binstr

    def __repr__(self):
        return f"LogicArray({self.binstr!r})"
