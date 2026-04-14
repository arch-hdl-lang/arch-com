"""Signal value and handle classes compatible with cocotb's interface."""


class ArchSignalValue:
    """Wraps a raw integer value, mimicking cocotb's BinaryValue/LogicArray."""

    def __init__(self, value, width, signed=False):
        self._value = int(value)
        self._width = width
        self._signed = signed

    def to_unsigned(self):
        return self._value & ((1 << self._width) - 1)

    def to_signed(self):
        v = self.to_unsigned()
        if v >= (1 << (self._width - 1)):
            v -= 1 << self._width
        return v

    def __int__(self):
        return self.to_unsigned()

    def __bool__(self):
        return self.to_unsigned() != 0

    def __eq__(self, other):
        if isinstance(other, int):
            return self.to_unsigned() == other
        if isinstance(other, ArchSignalValue):
            return self.to_unsigned() == other.to_unsigned()
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
    """Mimics cocotb signal handle with .value property."""

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

    @property
    def value(self):
        raw = getattr(self._dut._model, self._cpp_name)
        return ArchSignalValue(raw, self._width, self._signed)

    @value.setter
    def value(self, v):
        if self._is_param:
            raise AttributeError(f"Cannot write to parameter '{self._name}'")
        if isinstance(v, ArchSignalValue):
            v = v.to_unsigned()
        setattr(self._dut._model, self._cpp_name, int(v))
