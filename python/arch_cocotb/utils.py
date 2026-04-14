"""Utility functions compatible with cocotb.utils."""

from arch_cocotb.simulator import _get_sim


def get_sim_time(units='ns'):
    """Return the current simulation time."""
    sim = _get_sim()
    t = sim.get_sim_time_ns()
    units = units.lower().rstrip('s')
    if units == 'n':
        return float(t)
    elif units == 'p':
        return float(t * 1000)
    elif units == 'u' or units == 'micro':
        return float(t / 1000)
    elif units == 'm' or units == 'milli':
        return float(t / 1_000_000)
    return float(t)
