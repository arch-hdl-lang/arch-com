"""Utility functions compatible with :mod:`cocotb.utils`."""

from decimal import Decimal

from arch_cocotb.simulator import _get_sim
from arch_cocotb.triggers import _unit_ps


def get_sim_time(units="ns", unit=None):
    """Return current simulation time in the requested unit."""
    sim = _get_sim()
    requested = unit or units
    one_unit_ps = _unit_ps(requested, sim.step_ps)
    value = Decimal(sim.get_sim_time_ps()) / Decimal(one_unit_ps)
    if value == value.to_integral_value():
        return int(value)
    return float(value)
