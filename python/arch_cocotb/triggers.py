"""Cocotb-compatible trigger classes for arch sim."""

from arch_cocotb.simulator import _get_sim


class RisingEdge:
    """Suspend until the next rising edge (0→1) of a signal."""

    def __init__(self, signal):
        self._signal = signal

    def __await__(self):
        sim = _get_sim()
        return sim.wait_rising_edge(self._signal._name).__await__()


class FallingEdge:
    """Suspend until the next falling edge (1→0) of a signal."""

    def __init__(self, signal):
        self._signal = signal

    def __await__(self):
        sim = _get_sim()
        return sim.wait_falling_edge(self._signal._name).__await__()


class Timer:
    """Suspend for a specified duration."""

    def __init__(self, duration=None, units='ns', unit=None,
                 timeout_time=None, timeout_unit=None, **kwargs):
        # Handle cocotb's various Timer signatures
        if duration is None and timeout_time is not None:
            duration = timeout_time
        if unit is not None:
            units = unit
        if duration is None:
            duration = kwargs.get('time', 0)
        self._duration_ns = _to_ns(duration, units)

    def __await__(self):
        sim = _get_sim()
        return sim.wait_timer(self._duration_ns).__await__()


class Clock:
    """Generate a periodic clock signal.

    Usage:
        cocotb.start_soon(Clock(dut.clk, 10, units='ns').start())
    """

    def __init__(self, signal, period, units='ns', unit=None):
        self._signal = signal
        if unit is not None:
            units = unit
        self._half_period_ns = _to_ns(period, units) // 2
        if self._half_period_ns < 1:
            self._half_period_ns = 1

    def start(self, start_high=False):
        """Return a coroutine that toggles the clock forever."""
        async def _run():
            sim = _get_sim()
            val = 1 if start_high else 0
            self._signal.value = val
            while True:
                await sim.wait_timer(self._half_period_ns)
                val ^= 1
                self._signal.value = val
        return _run()


class ClockCycles:
    """Suspend for N rising edges of a clock signal."""

    def __init__(self, signal, num_cycles, rising=True):
        self._signal = signal
        self._num_cycles = num_cycles
        self._rising = rising

    def __await__(self):
        return self._wait().__await__()

    async def _wait(self):
        sim = _get_sim()
        for _ in range(self._num_cycles):
            if self._rising:
                await sim.wait_rising_edge(self._signal._name)
            else:
                await sim.wait_falling_edge(self._signal._name)


def _to_ns(duration, units):
    """Convert a duration to nanoseconds."""
    units = units.lower().rstrip('s')  # normalize: 'ns' 'us' 'ms' etc.
    scale = {
        'p': 0.001,
        'n': 1,
        'u': 1_000,
        'micro': 1_000,
        'm': 1_000_000,
        'milli': 1_000_000,
        '': 1_000_000_000,
        'sec': 1_000_000_000,
    }
    mult = scale.get(units, 1)
    return max(1, int(duration * mult))
