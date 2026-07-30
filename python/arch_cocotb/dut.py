"""DUT wrapper providing cocotb-compatible hierarchy and signal discovery."""

import logging
import re

from arch_cocotb.signal import ArchLocalSignal, ArchSignal, ArchSignalValue


_VEC_MEMBER_RE = re.compile(r"^(.+)_(\d+)$")


class _ArchVecProxy:
    """Indexable proxy over Vec ports flattened by the pybind generator."""

    __slots__ = ("_members", "_name")

    def __init__(self, name, members):
        self._name = name
        self._members = members

    def __getitem__(self, index):
        return self._members[index]

    def __len__(self):
        return len(self._members)

    def __iter__(self):
        return iter(self._members)

    @property
    def value(self):
        raw = 0
        offset = 0
        for signal in self._members:
            raw |= int(signal.value) << offset
            offset += len(signal)
        return ArchSignalValue(raw, offset)

    @value.setter
    def value(self, value):
        value = int(value)
        offset = 0
        for signal in self._members:
            mask = (1 << len(signal)) - 1
            signal.value = (value >> offset) & mask
            offset += len(signal)

    def setimmediatevalue(self, value):
        self.value = value


class ArchDUT:
    """Wrap a generated pybind model as a flat cocotb DUT hierarchy."""

    def __init__(self, model_class, name=None):
        object.__setattr__(self, "_model", model_class())
        object.__setattr__(self, "_name", name or model_class.__name__)
        object.__setattr__(self, "_log", logging.getLogger(self._name))
        object.__setattr__(self, "_signals", {})
        object.__setattr__(self, "_signal_list", [])
        object.__setattr__(self, "_vec_groups", {})
        object.__setattr__(self, "_casefold_names", {})
        object.__setattr__(self, "_simulator", None)
        self._register_from_port_info()

    def _attach_simulator(self, simulator):
        object.__setattr__(self, "_simulator", simulator)

    def _register_from_port_info(self):
        if not hasattr(type(self._model), "_port_info"):
            return
        for info in type(self._model)._port_info():
            name, width, signed, is_input, is_param, is_internal = info
            self.register_signal(
                name,
                width,
                signed=signed,
                is_input=is_input,
                is_param=is_param,
                is_internal=is_internal,
            )

        groups = {}
        for name, signal in self._signals.items():
            match = _VEC_MEMBER_RE.match(name)
            if not match:
                continue
            base, index_text = match.groups()
            if base in self._signals:
                continue
            groups.setdefault(base, {})[int(index_text)] = signal
        for base, members in groups.items():
            indices = sorted(members)
            if len(indices) < 2 or indices != list(range(len(indices))):
                continue
            proxy = _ArchVecProxy(base, [members[index] for index in indices])
            self._vec_groups[base] = proxy
            self._casefold_names.setdefault(base.casefold(), base)

        self._register_single_id_axi_defaults()

    def _register_single_id_axi_defaults(self):
        """Expose one-bit ID handles for otherwise complete ID-less AXI buses.

        `cocotbext-axi` models AXI channel IDs as mandatory handles even for a
        single-outstanding, single-ID interface. ARCH buses commonly omit
        those physically redundant ports. Recognize complete flattened AXI
        read/write channel groups and provide a shim-local one-bit ID channel
        tied to zero by default. Arbitrary missing signals are not synthesized.
        """

        names = set(self._signals)
        prefixes = set()
        for name in names:
            for suffix in ("_araddr", "_awaddr"):
                if name.endswith(suffix):
                    prefixes.add(name[: -len(suffix)])

        for prefix in sorted(prefixes):
            read_markers = {
                f"{prefix}_araddr",
                f"{prefix}_arvalid",
                f"{prefix}_arready",
                f"{prefix}_rdata",
                f"{prefix}_rvalid",
                f"{prefix}_rready",
            }
            if read_markers <= names:
                self._register_axi_id_pair(
                    prefix,
                    "arid",
                    "rid",
                    request_marker="araddr",
                    response_marker="rdata",
                )

            write_markers = {
                f"{prefix}_awaddr",
                f"{prefix}_awvalid",
                f"{prefix}_awready",
                f"{prefix}_wdata",
                f"{prefix}_wvalid",
                f"{prefix}_wready",
                f"{prefix}_bvalid",
                f"{prefix}_bready",
            }
            if write_markers <= names:
                self._register_axi_id_pair(
                    prefix,
                    "awid",
                    "bid",
                    request_marker="awaddr",
                    response_marker="bvalid",
                )

    def _register_axi_id_pair(
        self,
        prefix,
        request_id,
        response_id,
        request_marker,
        response_marker,
    ):
        request_name = f"{prefix}_{request_id}"
        response_name = f"{prefix}_{response_id}"
        existing = self._signals.get(request_name) or self._signals.get(response_name)
        width = len(existing) if existing is not None else 1
        for name, is_input in (
            (
                request_name,
                self._signals[f"{prefix}_{request_marker}"]._is_input,
            ),
            (
                response_name,
                self._signals[f"{prefix}_{response_marker}"]._is_input,
            ),
        ):
            if name in self._signals:
                continue
            signal = ArchLocalSignal(name, width, is_input=is_input)
            self._signals[name] = signal
            self._casefold_names.setdefault(name.casefold(), name)
            self._signal_list.append(signal)

    def register_signal(
        self,
        name,
        width,
        signed=False,
        is_input=False,
        is_param=False,
        is_internal=False,
        cpp_name=None,
    ):
        signal = ArchSignal(
            self,
            name,
            width,
            signed,
            is_input=is_input,
            is_param=is_param,
            is_internal=is_internal,
            cpp_name=cpp_name,
        )
        self._signals[name] = signal
        self._casefold_names.setdefault(name.casefold(), name)
        if not is_param:
            self._signal_list.append(signal)
        return signal

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        actual = self._casefold_names.get(name.casefold(), name)
        if actual in self._signals:
            return self._signals[actual]
        if actual in self._vec_groups:
            return self._vec_groups[actual]
        raise AttributeError(f"No signal '{name}' on DUT '{self._name}'")

    def __dir__(self):
        public = list(self._signals) + list(self._vec_groups)
        return sorted(set(object.__dir__(self)) | set(public))

    def __iter__(self):
        return iter(self._signal_list)

    def __len__(self):
        return len(self._signal_list)
