"""Utility functions compatible with cocotb.utils."""

from arch_cocotb.simulator import _get_sim, ps_to_unit


def get_sim_time(unit=None, units=None):
    """Return the current simulation time in the requested unit.

    One simulator step is one picosecond. 'step' and 'fs' return exact
    integers; other units return floats.
    """
    if unit is None:
        unit = units  # accept legacy keyword
    sim = _get_sim()
    return ps_to_unit(sim.get_sim_time_ps(), unit or 'step')
