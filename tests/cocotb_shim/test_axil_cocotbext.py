"""Drive the installed cocotbext-axi AXI-Lite master against AxilRegs.arch."""

import random

import cocotb
from cocotb.clock import Clock
from cocotb.triggers import RisingEdge

from cocotbext.axi import AxiLiteBus, AxiLiteMaster


async def setup(dut):
    cocotb.start_soon(Clock(dut.clk, 10, units='ns').start())
    master = AxiLiteMaster(
        AxiLiteBus.from_prefix(dut, "s_axil"), dut.clk, dut.rst
    )
    dut.rst.value = 1
    for _ in range(4):
        await RisingEdge(dut.clk)
    dut.rst.value = 0
    for _ in range(2):
        await RisingEdge(dut.clk)
    return master


@cocotb.test()
async def test_axil_single_write_read(dut):
    """AXI-Lite register reads and writes round-trip."""
    master = await setup(dut)
    await master.write_dword(0x00, 0x11223344)
    await master.write_dword(0x04, 0xDEADBEEF)
    await master.write_dword(0x3C, 0xA5A5A5A5)
    assert await master.read_dword(0x00) == 0x11223344
    assert await master.read_dword(0x04) == 0xDEADBEEF
    assert await master.read_dword(0x3C) == 0xA5A5A5A5


@cocotb.test()
async def test_axil_byte_strobes(dut):
    """Sub-word writes update only the strobed byte lanes."""
    master = await setup(dut)
    await master.write_dword(0x08, 0xAABBCCDD)
    await master.write(0x08, b'\x11')            # byte 0
    assert await master.read_dword(0x08) == 0xAABBCC11
    await master.write(0x0A, b'\x22')            # byte 2
    assert await master.read_dword(0x08) == 0xAA22CC11


@cocotb.test()
async def test_axil_queued_transactions(dut):
    """Multiple queued (non-awaited) writes all land, in order."""
    master = await setup(dut)
    events = [
        master.init_write(addr * 4, (0xC0DE0000 + addr).to_bytes(4, 'little'))
        for addr in range(8)
    ]
    for ev in events:
        await ev.wait()
    for addr in range(8):
        assert await master.read_dword(addr * 4) == 0xC0DE0000 + addr


@cocotb.test()
async def test_axil_random_soak(dut):
    """Randomized write/read soak against a mirror model."""
    master = await setup(dut)
    rng = random.Random(1234)
    mirror = {}
    for _ in range(50):
        addr = rng.randrange(16) * 4
        if addr in mirror and rng.random() < 0.4:
            assert await master.read_dword(addr) == mirror[addr]
        else:
            val = rng.getrandbits(32)
            await master.write_dword(addr, val)
            mirror[addr] = val
    for addr, val in mirror.items():
        assert await master.read_dword(addr) == val
