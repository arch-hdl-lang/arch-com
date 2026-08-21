"""Signal values and handles compatible with the common cocotb surface."""


def _coerce_integer(value):
    if isinstance(value, ArchSignalValue):
        return value.to_unsigned()
    if isinstance(value, str):
        # Native simulation is intentionally two-state. Unknown and
        # high-impedance digits therefore resolve to zero.
        bits = "".join("0" if char.lower() in "xz" else char for char in value)
        return int(bits or "0", 2)
    if hasattr(value, "to_unsigned"):
        return int(value.to_unsigned())
    return int(value)


class ArchSignalValue:
    """Integer-backed, two-state equivalent of cocotb's signal value."""

    def __init__(self, value, width, signed=False):
        self._width = int(width)
        self._signed = bool(signed)
        self._value = int(value) & self._mask

    @property
    def _mask(self):
        return (1 << self._width) - 1 if self._width else 0

    @property
    def integer(self):
        return self.to_unsigned()

    @property
    def signed_integer(self):
        return self.to_signed()

    @property
    def binstr(self):
        return format(self.to_unsigned(), f"0{self._width}b")

    @property
    def is_resolvable(self):
        return True

    def to_unsigned(self):
        return self._value

    def to_signed(self):
        value = self.to_unsigned()
        if self._width and value >= (1 << (self._width - 1)):
            value -= 1 << self._width
        return value

    def __len__(self):
        return self._width

    def __index__(self):
        return self.to_unsigned()

    def __int__(self):
        return self.to_unsigned()

    def __bool__(self):
        return self.to_unsigned() != 0

    def __eq__(self, other):
        try:
            return self.to_unsigned() == _coerce_integer(other)
        except (TypeError, ValueError):
            return NotImplemented

    def __ne__(self, other):
        result = self.__eq__(other)
        return result if result is NotImplemented else not result

    def __repr__(self):
        return self.binstr

    def __str__(self):
        return str(self.to_unsigned())

    def __hash__(self):
        return hash(self.to_unsigned())


class ArchSignal:
    """A cocotb-style value handle backed by a pybind model attribute."""

    def __init__(
        self,
        dut,
        name,
        width,
        signed=False,
        is_input=False,
        is_param=False,
        is_internal=False,
        cpp_name=None,
    ):
        self._dut = dut
        self._name = name
        self._width = int(width)
        self._signed = bool(signed)
        self._is_input = bool(is_input)
        self._is_param = bool(is_param)
        self._is_internal = bool(is_internal)
        self._cpp_name = cpp_name or name
        self._type = "GPI_PARAMETER" if is_param else "GPI_NET"

    @property
    def value(self):
        raw = getattr(self._dut._model, self._cpp_name)
        return ArchSignalValue(raw, self._width, self._signed)

    @value.setter
    def value(self, value):
        if self._is_param:
            raise AttributeError(f"Cannot write to parameter '{self._name}'")
        masked = _coerce_integer(value) & ((1 << self._width) - 1)
        setattr(self._dut._model, self._cpp_name, masked)
        if self._is_input:
            marker = getattr(
                self._dut._model,
                f"_mark_input_{self._cpp_name}",
                None,
            )
            if marker is not None:
                marker()
        simulator = self._dut._simulator
        if simulator is not None:
            simulator.signal_written()

    def setimmediatevalue(self, value):
        self.value = value

    def __len__(self):
        return self._width

    def __repr__(self):
        return f"ArchSignal({self._name!r}, width={self._width})"


class ArchLocalSignal:
    """Cocotb-style signal backed by shim-local two-state storage.

    This is used for protocol defaults that have no physical ARCH port, such
    as the one-bit zero ID channel on an ID-less AXI4 interface.
    """

    def __init__(self, name, width=1, value=0, is_input=False):
        self._name = name
        self._width = int(width)
        self._is_input = bool(is_input)
        self._value = _coerce_integer(value) & self._mask
        self._type = "GPI_NET"

    @property
    def _mask(self):
        return (1 << self._width) - 1 if self._width else 0

    @property
    def value(self):
        return ArchSignalValue(self._value, self._width)

    @value.setter
    def value(self, value):
        self._value = _coerce_integer(value) & self._mask

    def setimmediatevalue(self, value):
        self.value = value

    def __len__(self):
        return self._width

    def __repr__(self):
        return f"ArchLocalSignal({self._name!r}, width={self._width})"
