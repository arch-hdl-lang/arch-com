"""DUT wrapper that provides cocotb-compatible signal access."""

import re

from arch_cocotb.signal import ArchSignal, ArchSignalValue


_VEC_MEMBER_RE = re.compile(r"^(.+)_(\d+)$")


class _ArchVecProxy:
    """Indexable proxy over Vec ports.

    arch sim's pybind layer flattens `port: <dir> Vec<T, N>` (packed or
    unpacked) into N scalar attributes named `port_0` .. `port_{N-1}`.
    Tests written against real Verilator cocotb expect both styles:

      dut.port[i].value = lane_value     # per-lane (works for both)
      dut.port.value    = whole_packed   # whole-value (packed Vec only
                                         # under Verilator, since
                                         # `vec[i]` errors with
                                         # "Packed objects cannot be
                                         # indexed")

    The proxy supports both: `[i]` maps to the underlying `port_i`
    ArchSignal handle, and `.value` composes/decomposes the lanes. Lane
    0 occupies the LSB bits, matching the cocotb-Verilator convention
    for packed `logic [N-1:0][W-1:0]`.
    """

    __slots__ = ("_members",)

    def __init__(self, members):
        # members: list[ArchSignal] in index order
        self._members = members

    def __getitem__(self, idx):
        return self._members[idx]

    def __len__(self):
        return len(self._members)

    def __iter__(self):
        return iter(self._members)

    @property
    def value(self):
        """Compose lanes into a single packed integer (lane 0 = LSB)."""
        raw = 0
        offset = 0
        for sig in self._members:
            lane = int(sig.value) & ((1 << sig._width) - 1)
            raw |= lane << offset
            offset += sig._width
        return ArchSignalValue(raw, offset, signed=False)

    @value.setter
    def value(self, v):
        """Decompose a packed integer into lanes (lane 0 = LSB)."""
        if isinstance(v, ArchSignalValue):
            v = v.to_unsigned()
        v = int(v)
        offset = 0
        for sig in self._members:
            mask = (1 << sig._width) - 1
            sig.value = (v >> offset) & mask
            offset += sig._width


class ArchDUT:
    """Wraps a pybind11 arch sim model instance.

    Provides cocotb-compatible attribute access:
      dut.signal_name.value          # read
      dut.signal_name.value = 42     # write
      dut.PARAM_NAME.value.to_unsigned()  # parameter
      for sig in dut: ...            # iterate signals
    """

    def __init__(self, model_class):
        object.__setattr__(self, '_model', model_class())
        object.__setattr__(self, '_signals', {})
        object.__setattr__(self, '_signal_list', [])
        object.__setattr__(self, '_vec_groups', {})
        self._register_from_port_info()

    def _register_from_port_info(self):
        """Auto-register signals from the model's _port_info() metadata."""
        if not hasattr(type(self._model), '_port_info'):
            return
        for info in type(self._model)._port_info():
            # info = (name, width, signed, is_input, is_param, is_internal)
            name, width, signed, _is_input, is_param, is_internal = info
            # pybind11 exposes internal regs without underscore prefix
            cpp_name = name
            sig = ArchSignal(
                self, name, width, signed,
                is_param=is_param, is_internal=is_internal,
                cpp_name=cpp_name,
            )
            self._signals[name] = sig
            if not is_param:
                self._signal_list.append(sig)
        # Detect unpacked-Vec port groups: any name `base_<idx>` where
        # `base` is not itself a registered scalar AND there are at least
        # two consecutive indices starting at 0. This avoids false
        # positives on names that incidentally end in `_<digit>` (e.g. an
        # SV constant `pkt_512`).
        groups: dict[str, dict[int, ArchSignal]] = {}
        for name, sig in self._signals.items():
            m = _VEC_MEMBER_RE.match(name)
            if not m:
                continue
            base, idx_str = m.group(1), m.group(2)
            if base in self._signals:
                continue
            groups.setdefault(base, {})[int(idx_str)] = sig
        for base, members in groups.items():
            indices = sorted(members)
            if len(indices) < 2 or indices != list(range(len(indices))):
                continue
            self._vec_groups[base] = _ArchVecProxy([members[i] for i in indices])

    def register_signal(self, name, width, signed=False, is_param=False,
                        is_internal=False, cpp_name=None):
        """Manually register a signal (for models without _port_info)."""
        sig = ArchSignal(
            self, name, width, signed,
            is_param=is_param, is_internal=is_internal,
            cpp_name=cpp_name,
        )
        self._signals[name] = sig
        if not is_param:
            self._signal_list.append(sig)

    def __getattr__(self, name):
        if name.startswith('_'):
            raise AttributeError(name)
        sigs = object.__getattribute__(self, '_signals')
        if name in sigs:
            return sigs[name]
        groups = object.__getattribute__(self, '_vec_groups')
        if name in groups:
            return groups[name]
        raise AttributeError(f"No signal '{name}' on DUT")

    def __iter__(self):
        return iter(self._signal_list)
