"""
Cocotb testbench for hw_task_queue (linklist construct).

Tests:
  1. Basic enqueue + dequeue — FIFO order preserved
  2. Fill to capacity — full asserts, further enqueue blocked
  3. Drain from full — empty asserts after all dequeued
  4. Interleaved enqueue/dequeue — streaming behaviour
  5. Back-pressure: dequeue while empty is blocked (req_ready low)
"""
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import RisingEdge, FallingEdge, Timer
import random

DEPTH = 8
CLK_PERIOD_NS = 10


async def reset_dut(dut):
    dut.rst.value = 1
    dut.insert_tail_req_valid.value = 0
    dut.insert_tail_req_data.value = 0
    dut.delete_head_req_valid.value = 0
    for _ in range(4):
        await RisingEdge(dut.clk)
    dut.rst.value = 0
    await RisingEdge(dut.clk)


async def enqueue(dut, data, timeout=20):
    """Drive insert_tail handshake, wait for resp_valid. Returns handle."""
    dut.insert_tail_req_valid.value = 1
    dut.insert_tail_req_data.value = data
    # Wait for req_ready
    for _ in range(timeout):
        await RisingEdge(dut.clk)
        if dut.insert_tail_req_ready.value:
            break
    else:
        raise TimeoutError(f"enqueue({data:#010x}) req_ready never asserted")
    dut.insert_tail_req_valid.value = 0
    dut.insert_tail_req_data.value = 0
    # Wait for resp_valid (2-cycle latency)
    for _ in range(timeout):
        await RisingEdge(dut.clk)
        if dut.insert_tail_resp_valid.value:
            return int(dut.insert_tail_resp_handle.value)
    raise TimeoutError(f"enqueue({data:#010x}) resp_valid never asserted")


async def dequeue(dut, timeout=20):
    """Drive delete_head handshake, wait for resp_valid. Returns data."""
    dut.delete_head_req_valid.value = 1
    for _ in range(timeout):
        await RisingEdge(dut.clk)
        if dut.delete_head_req_ready.value:
            break
    else:
        raise TimeoutError("dequeue req_ready never asserted")
    dut.delete_head_req_valid.value = 0
    for _ in range(timeout):
        await RisingEdge(dut.clk)
        if dut.delete_head_resp_valid.value:
            return int(dut.delete_head_resp_data.value)
    raise TimeoutError("dequeue resp_valid never asserted")


@cocotb.test()
async def test_basic_fifo_order(dut):
    """Enqueue N items then dequeue — verify FIFO order."""
    cocotb.start_soon(Clock(dut.clk, CLK_PERIOD_NS, units="ns").start())
    await reset_dut(dut)

    assert dut.empty.value == 1, "Should be empty after reset"
    assert dut.full.value == 0

    tasks = [0xDEAD0000 | i for i in range(4)]
    for t in tasks:
        await enqueue(dut, t)

    assert int(dut.length.value) == 4, f"Expected length 4, got {int(dut.length.value)}"

    for expected in tasks:
        got = await dequeue(dut)
        assert got == expected, f"FIFO order violated: expected {expected:#010x}, got {got:#010x}"

    assert dut.empty.value == 1, "Should be empty after draining"


@cocotb.test()
async def test_fill_to_capacity(dut):
    """Fill queue to DEPTH — full asserts, one more enqueue is back-pressured."""
    cocotb.start_soon(Clock(dut.clk, CLK_PERIOD_NS, units="ns").start())
    await reset_dut(dut)

    for i in range(DEPTH):
        await enqueue(dut, 0xA0000000 | i)

    assert dut.full.value == 1, "Should be full after DEPTH enqueues"
    assert dut.empty.value == 0

    # Attempting enqueue should see req_ready=0
    dut.insert_tail_req_valid.value = 1
    dut.insert_tail_req_data.value = 0xDEADBEEF
    await RisingEdge(dut.clk)
    assert dut.insert_tail_req_ready.value == 0, "req_ready should be 0 when full"
    dut.insert_tail_req_valid.value = 0

    # Drain one slot, then enqueue should succeed
    await dequeue(dut)
    assert dut.full.value == 0

    await enqueue(dut, 0xB0000000)


@cocotb.test()
async def test_drain_to_empty(dut):
    """Fill then drain — verify empty asserts and length tracks correctly."""
    cocotb.start_soon(Clock(dut.clk, CLK_PERIOD_NS, units="ns").start())
    await reset_dut(dut)

    n = 5
    for i in range(n):
        await enqueue(dut, 0xC0C00000 | i)

    assert int(dut.length.value) == n

    results = []
    for _ in range(n):
        results.append(await dequeue(dut))

    assert dut.empty.value == 1
    assert int(dut.length.value) == 0
    assert results == [0xC0C00000 | i for i in range(n)], f"Wrong order: {results}"


@cocotb.test()
async def test_interleaved(dut):
    """Interleave enqueues and dequeues — streaming FIFO order maintained."""
    cocotb.start_soon(Clock(dut.clk, CLK_PERIOD_NS, units="ns").start())
    await reset_dut(dut)

    expected = []
    results = []
    rng = random.Random(42)

    for i in range(20):
        action = rng.choice(["enq", "deq", "enq"])  # bias toward enqueue
        if action == "enq" and not dut.full.value:
            val = rng.randint(0, 0xFFFFFFFF)
            await enqueue(dut, val)
            expected.append(val)
        elif action == "deq" and not dut.empty.value:
            got = await dequeue(dut)
            results.append(got)

    # Drain remaining
    while not dut.empty.value:
        results.append(await dequeue(dut))

    assert results == expected[:len(results)], \
        f"FIFO order violated in interleaved test\nExpected: {expected}\nGot: {results}"


@cocotb.test()
async def test_empty_dequeue_blocked(dut):
    """Dequeue on empty queue — req_ready stays low."""
    cocotb.start_soon(Clock(dut.clk, CLK_PERIOD_NS, units="ns").start())
    await reset_dut(dut)

    assert dut.empty.value == 1

    dut.delete_head_req_valid.value = 1
    await RisingEdge(dut.clk)
    assert dut.delete_head_req_ready.value == 0, "req_ready should be 0 when empty"
    dut.delete_head_req_valid.value = 0

    # Enqueue something and verify dequeue now works
    await enqueue(dut, 0x12345678)
    got = await dequeue(dut)
    assert got == 0x12345678
