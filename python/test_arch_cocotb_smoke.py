"""Smoke test for arch_cocotb adapter with coffee_machine."""

import sys
import os

# Add paths
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'cocotb_shim'))
sys.path.insert(0, os.path.dirname(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'arch_sim_build'))

import Vcoffee_machine_pybind as sim_module
from arch_cocotb.dut import ArchDUT
from arch_cocotb.simulator import ArchSimulator
from arch_cocotb.triggers import RisingEdge, Clock
import arch_cocotb.decorators as decorators
import asyncio


async def test_reset(dut):
    """Test that reset clears all outputs."""
    from arch_cocotb.simulator import _get_sim
    sim = _get_sim()

    # Start clock
    async def clock_gen():
        while True:
            dut._model.clk = 0
            await sim.wait_timer(5)
            dut._model.clk = 1
            await sim.wait_timer(5)
    sim.schedule(clock_gen())

    # Assert reset
    dut.rst_async_n.value = 0
    dut.i_start.value = 0
    dut.i_sensor.value = 0
    dut.i_operation_sel.value = 0
    dut.i_grind_delay.value = 0
    dut.i_heat_delay.value = 0
    dut.i_pour_delay.value = 0
    dut.i_bean_sel.value = 0

    await RisingEdge(dut.clk)

    # Deassert reset
    dut.rst_async_n.value = 1
    await RisingEdge(dut.clk)

    # Check reset values
    assert dut.o_heat_water.value == 0, f"heat_water={dut.o_heat_water.value}"
    assert dut.o_pour_coffee.value == 0
    assert dut.o_grind_beans.value == 0
    assert dut.o_use_powder.value == 0
    assert dut.o_error.value == 0
    assert dut.state_ff.value == 0
    print("  Reset test: OK")

    # Test a simple operation: op=0 (hot water only)
    dut.i_start.value = 1
    dut.i_operation_sel.value = 0
    dut.i_heat_delay.value = 2
    dut.i_pour_delay.value = 2

    await RisingEdge(dut.clk)  # Edge A: i_start_r captures 1
    dut.i_start.value = 0

    await RisingEdge(dut.clk)  # Edge B: state transitions to HEAT(3)
    assert dut.state_ff.value == 3, f"Expected HEAT(3), got {dut.state_ff.value}"
    # Outputs still reflect old state (0) due to port reg 1-cycle lag
    assert dut.o_heat_water.value == 0, f"port reg lag: expected 0, got {dut.o_heat_water.value}"
    print(f"  Edge B: state_ff={int(dut.state_ff.value)}, hw={int(dut.o_heat_water.value)} (port reg lag OK)")

    await RisingEdge(dut.clk)  # Edge C: outputs now reflect HEAT
    assert dut.o_heat_water.value == 1, f"Expected hw=1, got {dut.o_heat_water.value}"
    print(f"  Edge C: state_ff={int(dut.state_ff.value)}, hw={int(dut.o_heat_water.value)} (HEAT output OK)")

    # Wait for operation to complete
    for _ in range(20):
        await RisingEdge(dut.clk)
        if dut.state_ff.value == 0:
            break

    assert dut.state_ff.value == 0, f"Expected IDLE(0), got {dut.state_ff.value}"
    print("  Operation completed back to IDLE: OK")


def main():
    print("=== arch_cocotb smoke test: coffee_machine ===\n")

    dut = ArchDUT(sim_module.Vcoffee_machine)
    print(f"DUT created with {len(dut._signals)} signals")
    print(f"  Params: NBW_DLY={int(dut.NBW_DLY.value)}, NBW_BEANS={int(dut.NBW_BEANS.value)}, NS_BEANS={int(dut.NS_BEANS.value)}")

    sim = ArchSimulator(dut, time_unit_ns=1)
    print(f"  Running test_reset...")
    asyncio.run(sim.run_test(test_reset, dut))

    print("\n=== ALL TESTS PASSED ===")


if __name__ == '__main__':
    main()
