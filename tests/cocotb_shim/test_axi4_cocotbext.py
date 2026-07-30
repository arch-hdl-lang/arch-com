"""Drive the installed cocotbext-axi AXI4 master against Axi4Mem.arch.

Covers incrementing burst reads/writes, byte strobes, independent
channel backpressure, queued transactions, and a final byte-addressed
memory comparison.
"""

import itertools
import random

import cocotb
from cocotb.clock import Clock
from cocotb.triggers import RisingEdge

from cocotbext.axi import AxiBus, AxiMaster


MEM_BYTES = 1024


async def setup(dut):
    cocotb.start_soon(Clock(dut.clk, 10, units='ns').start())
    master = AxiMaster(AxiBus.from_prefix(dut, "s_axi"), dut.clk, dut.rst)
    dut.rst.value = 1
    for _ in range(4):
        await RisingEdge(dut.clk)
    dut.rst.value = 0
    for _ in range(2):
        await RisingEdge(dut.clk)
    return master


@cocotb.test()
async def test_axi4_burst_write_read(dut):
    """AXI4 incrementing burst writes then burst reads round-trip."""
    master = await setup(dut)
    payload = bytes(range(256))          # 64 beats
    await master.write(0x000, payload)
    data = await master.read(0x000, len(payload))
    assert data.data == payload


@cocotb.test()
async def test_axi4_byte_strobes(dut):
    """Sub-word writes update only the strobed byte lanes."""
    master = await setup(dut)
    await master.write(0x40, bytes([0xDD, 0xCC, 0xBB, 0xAA]))
    await master.write(0x41, bytes([0x99]))
    data = await master.read(0x40, 4)
    assert data.data == bytes([0xDD, 0x99, 0xBB, 0xAA]), data.data


@cocotb.test()
async def test_axi4_backpressure(dut):
    """Independent address, data, and response backpressure."""
    master = await setup(dut)
    # Pause generators throttle each channel independently.
    master.write_if.aw_channel.set_pause_generator(
        itertools.cycle([0, 0, 1, 0, 1, 1, 0])
    )
    master.write_if.w_channel.set_pause_generator(
        itertools.cycle([0, 1, 0, 0, 1])
    )
    master.write_if.b_channel.set_pause_generator(
        itertools.cycle([1, 0, 0, 1, 0])
    )
    master.read_if.ar_channel.set_pause_generator(
        itertools.cycle([0, 1, 1, 0])
    )
    master.read_if.r_channel.set_pause_generator(
        itertools.cycle([0, 0, 1])
    )
    rng = random.Random(77)
    for base in range(0, 256, 64):
        payload = bytes(rng.getrandbits(8) for _ in range(64))
        await master.write(0x100 + base, payload)
        data = await master.read(0x100 + base, 64)
        assert data.data == payload


@cocotb.test()
async def test_axi4_queued_transactions(dut):
    """Multiple queued (non-awaited) transactions all complete in order."""
    master = await setup(dut)
    payloads = {
        addr: bytes([(addr >> 4) ^ b for b in range(32)])
        for addr in range(0x200, 0x300, 0x20)
    }
    write_ops = [
        master.init_write(addr, data) for addr, data in payloads.items()
    ]
    for op in write_ops:
        await op.wait()
    read_ops = [master.init_read(addr, 32) for addr in payloads]
    for op, (addr, expect) in zip(read_ops, payloads.items()):
        await op.wait()
        assert op.data.data == expect, hex(addr)


@cocotb.test()
async def test_axi4_memory_comparison(dut):
    """Randomized soak, then final byte-addressed memory comparison."""
    master = await setup(dut)
    rng = random.Random(4242)
    mirror = bytearray(MEM_BYTES)
    await master.write(0, bytes(MEM_BYTES))  # initialize backing store
    for _ in range(25):
        addr = rng.randrange(MEM_BYTES)
        length = rng.randrange(1, min(64, MEM_BYTES - addr) + 1)
        payload = bytes(rng.getrandbits(8) for _ in range(length))
        await master.write(addr, payload)
        mirror[addr:addr + length] = payload
    data = await master.read(0, MEM_BYTES)
    assert data.data == bytes(mirror), "final memory mismatch"
