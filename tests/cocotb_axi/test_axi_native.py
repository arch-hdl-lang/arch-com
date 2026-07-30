"""Run the installed cocotbext-axi AXI4 master on native ARCH sim."""

import random

import cocotb
from cocotb.clock import Clock
from cocotb.triggers import ClockCycles
from cocotbext.axi import AxiBus, AxiMaster


def pause_pattern(seed):
    randomizer = random.Random(seed)
    while True:
        yield randomizer.randrange(5) == 0


@cocotb.test(timeout_time=500, timeout_unit="us")
async def axi_memory_conformance(dut):
    cocotb.start_soon(
        Clock(dut.clk, 10, units="ns").start(start_high=False)
    )

    dut.rst.value = 1
    await ClockCycles(dut.clk, 3)
    dut.rst.value = 0
    await ClockCycles(dut.clk, 2)

    master = AxiMaster(AxiBus.from_prefix(dut, "s_axi"), dut.clk, dut.rst)

    master.write_if.aw_channel.set_pause_generator(pause_pattern(11))
    master.write_if.w_channel.set_pause_generator(pause_pattern(12))
    master.write_if.b_channel.set_pause_generator(pause_pattern(13))
    master.read_if.ar_channel.set_pause_generator(pause_pattern(14))
    master.read_if.r_channel.set_pause_generator(pause_pattern(15))

    reference = bytearray(1024)

    # Long transfers are emitted as AXI4 INCR bursts.
    payload = bytes((index * 29 + 3) & 0xFF for index in range(300))
    await master.write(0x80, payload)
    reference[0x80:0x80 + len(payload)] = payload
    response = await master.read(0x80, len(payload))
    assert response.data == payload

    # Unaligned endpoints exercise first/last-beat byte strobes.
    partial = bytes(range(1, 18))
    await master.write(0x93, partial)
    reference[0x93:0x93 + len(partial)] = partial
    response = await master.read(0x8C, 40)
    assert response.data == bytes(reference[0x8C:0x8C + 40])

    # Queue independent transactions before awaiting their responses.
    writes = []
    for index in range(6):
        address = 0x300 + index * 20
        data = bytes((index * 31 + offset) & 0xFF for offset in range(15))
        reference[address:address + len(data)] = data
        writes.append(master.init_write(address, data))
    for event in writes:
        await event.wait()

    reads = []
    for index in range(6):
        address = 0x300 + index * 20
        reads.append((master.init_read(address, 15), address))
    for event, address in reads:
        await event.wait()
        assert event.data.data == bytes(reference[address:address + 15])

    final = await master.read(0, len(reference))
    assert final.data == bytes(reference)
