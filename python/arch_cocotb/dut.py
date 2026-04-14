"""DUT wrapper that provides cocotb-compatible signal access."""

from arch_cocotb.signal import ArchSignal


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
        raise AttributeError(f"No signal '{name}' on DUT")

    def __iter__(self):
        return iter(self._signal_list)
