"""Run the installed cocotbext-axi AXI-Lite master on native ARCH sim."""

import random

import cocotb
from cocotb.clock import Clock
from cocotb.triggers import ClockCycles
from cocotbext.axi import AxiLiteBus, AxiLiteMaster


def pause_pattern(seed):
    randomizer = random.Random(seed)
    while True:
        yield randomizer.randrange(4) == 0


@cocotb.test(timeout_time=200, timeout_unit="us")
async def axil_memory_conformance(dut):
    cocotb.start_soon(Clock(dut.clk, 10, units="ns").start(False))

    dut.rst.value = 1
    await ClockCycles(dut.clk, 3)
    dut.rst.value = 0
    await ClockCycles(dut.clk, 2)

    master = AxiLiteMaster(
        AxiLiteBus.from_prefix(dut, "s_axil"),
        dut.clk,
        dut.rst,
    )

    # Independent pauses cover address, data, response, and read channels.
    master.write_if.aw_channel.set_pause_generator(pause_pattern(1))
    master.write_if.w_channel.set_pause_generator(pause_pattern(2))
    master.write_if.b_channel.set_pause_generator(pause_pattern(3))
    master.read_if.ar_channel.set_pause_generator(pause_pattern(4))
    master.read_if.r_channel.set_pause_generator(pause_pattern(5))

    reference = bytearray(1024)
    initial = bytes((index * 13 + 7) & 0xFF for index in range(96))
    await master.write(0x40, initial)
    reference[0x40:0xA0] = initial
    response = await master.read(0x40, len(initial))
    assert response.data == initial

    # Unaligned short writes force partial first/last words and byte strobes.
    partial = b"\xA5\x5A\xC3\x3C\x77"
    await master.write(0x47, partial)
    reference[0x47:0x4C] = partial
    response = await master.read(0x40, 32)
    assert response.data == bytes(reference[0x40:0x60])

    # Queue several operations before awaiting completion.
    queued = []
    for index in range(8):
        address = 0x100 + index * 12
        payload = bytes(((index << 4) + byte) & 0xFF for byte in range(9))
        reference[address:address + len(payload)] = payload
        queued.append((master.init_write(address, payload), address, payload))
    for event, _, _ in queued:
        await event.wait()

    reads = [
        (master.init_read(address, len(payload)), address, payload)
        for _, address, payload in queued
    ]
    for event, address, payload in reads:
        await event.wait()
        assert event.data.data == payload, hex(address)

    final = await master.read(0, len(reference))
    assert final.data == bytes(reference)
