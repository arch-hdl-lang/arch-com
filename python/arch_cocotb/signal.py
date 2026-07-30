"""Signal value and handle classes compatible with cocotb's interface."""

from arch_cocotb import simulator as _simulator


def _coerce_int(v, width):
    """Convert a write value (int, bool, str, LogicArray, signal value)
    to an int masked to the declared width. X/Z bits become 0."""
    if isinstance(v, ArchSignalValue):
        v = v.to_unsigned()
    elif isinstance(v, str):
        # Binary string, possibly with x/z bits (deterministically 0).
        v = int(''.join(c if c in '01' else '0' for c in v) or '0', 2)
    elif hasattr(v, 'integer'):
        v = v.integer
    else:
        v = int(v)
    mask = (1 << width) - 1
    return v & mask


class ArchSignalValue:
    """Wraps a raw integer value, mimicking cocotb's LogicArray surface."""

    def __init__(self, value, width, signed=False):
        self._value = int(value)
        self._width = width
        self._signed = signed

    def to_unsigned(self):
        return self._value & ((1 << self._width) - 1)

    def to_signed(self):
        v = self.to_unsigned()
        if self._width > 0 and v >= (1 << (self._width - 1)):
            v -= 1 << self._width
        return v

    # cocotb 1.x BinaryValue compatibility
    @property
    def integer(self):
        return self.to_unsigned()

    @property
    def signed_integer(self):
        return self.to_signed()

    @property
    def binstr(self):
        return format(self.to_unsigned(), f'0{self._width}b')

    def __int__(self):
        return self.to_unsigned()

    def __index__(self):
        return self.to_unsigned()

    def __bool__(self):
        return self.to_unsigned() != 0

    def __len__(self):
        return self._width

    def __eq__(self, other):
        if isinstance(other, (int, bool)):
            return self.to_unsigned() == int(other)
        if isinstance(other, ArchSignalValue):
            return self.to_unsigned() == other.to_unsigned()
        if hasattr(other, 'integer'):
            return self.to_unsigned() == other.integer
        return NotImplemented

    def __ne__(self, other):
        result = self.__eq__(other)
        if result is NotImplemented:
            return result
        return not result

    def __repr__(self):
        return str(self.to_unsigned())

    def __str__(self):
        return str(self.to_unsigned())

    def __hash__(self):
        return hash(self.to_unsigned())


class ArchSignal:
    """Mimics a cocotb signal handle with .value read/write."""

    def __init__(self, dut, name, width, signed=False, is_param=False,
                 is_internal=False, cpp_name=None):
        self._dut = dut
        self._name = name
        self._width = width
        self._signed = signed
        self._is_param = is_param
        self._is_internal = is_internal
        self._cpp_name = cpp_name or name
        self._type = "GPI_PARAMETER" if is_param else "GPI_NET"

    def __len__(self):
        """Declared width in bits."""
        return self._width

    def _raw_read(self):
        return int(getattr(self._dut._model, self._cpp_name))

    def _apply_raw(self, masked_value):
        """Write an already-masked value straight to the model field."""
        setattr(self._dut._model, self._cpp_name, masked_value)

    @property
    def value(self):
        return ArchSignalValue(self._raw_read(), self._width, self._signed)

    @value.setter
    def value(self, v):
        """Deposit a write, applied at the next scheduler sync point.

        Mirrors cocotb: a write made in reaction to a clock edge is not
        seen by that same edge's sequential logic. Use
        setimmediatevalue() for an immediate update.
        """
        if self._is_param:
            raise AttributeError(f"Cannot write to parameter '{self._name}'")
        masked = _coerce_int(v, self._width)
        sim = _simulator._sim_instance
        if sim is not None:
            sim.deposit(self, masked)
        else:
            self._apply_raw(masked)

    def setimmediatevalue(self, v):
        """Update the model input now, without any scheduled delay."""
        if self._is_param:
            raise AttributeError(f"Cannot write to parameter '{self._name}'")
        self._apply_raw(_coerce_int(v, self._width))
        sim = _simulator._sim_instance
        if sim is not None:
            sim.notify_write()

    def __repr__(self):
        return f"<ArchSignal {self._name}[{self._width}]>"
