"""Verify generated VlWide pybind properties use arbitrary Python integers."""

import cocotb
from cocotb.triggers import ReadOnly


@cocotb.test()
async def wide_round_trip(dut):
    value = (1 << 129) | (0x123456789ABCDEF << 33) | 0x1A5
    dut.data_in.value = value
    await ReadOnly()
    assert len(dut.data_in) == 130
    assert int(dut.data_in.value) == value
    assert int(dut.data_out.value) == value

    dut.data_in.value = -1
    await ReadOnly()
    assert int(dut.data_out.value) == (1 << 130) - 1
