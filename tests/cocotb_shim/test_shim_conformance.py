"""Conformance tests for the arch_cocotb shim (spec: ArchNativeCocotbShim).

Covers Timing, Tasks And Triggers, and Handles against a real
arch-generated pybind model (ShimProbe.arch).
"""

import cocotb
from cocotb.clock import Clock
from cocotb.queue import Queue, QueueFull
from cocotb.triggers import (
    ClockCycles,
    Edge,
    Event,
    FallingEdge,
    First,
    ReadOnly,
    RisingEdge,
    SimTimeoutError,
    Timer,
    with_timeout,
)
from cocotb.utils import get_sim_time
from cocotb.types import LogicArray


async def start_clock(dut, period=3333, units='ps'):
    cocotb.start_soon(Clock(dut.clk, period, units=units).start())


async def reset_dut(dut):
    dut.rst.value = 1
    dut.in_a.value = 0
    dut.in_b.value = 0
    dut.wide_in.value = 0
    await RisingEdge(dut.clk)
    await RisingEdge(dut.clk)
    dut.rst.value = 0
    await RisingEdge(dut.clk)


# ── Timing ────────────────────────────────────────────────────────────


@cocotb.test()
async def test_odd_period_clock_halves(dut):
    """A 3333 ps clock produces alternating 1666/1667 ps half-periods."""
    await start_clock(dut)
    await RisingEdge(dut.clk)
    t0 = get_sim_time('ps')
    await FallingEdge(dut.clk)
    t1 = get_sim_time('ps')
    await RisingEdge(dut.clk)
    t2 = get_sim_time('ps')
    await FallingEdge(dut.clk)
    t3 = get_sim_time('ps')
    assert t1 - t0 == 1666, f"high half {t1 - t0}"
    assert t2 - t1 == 1667, f"low half {t2 - t1}"
    assert t3 - t2 == 1666, f"second high half {t3 - t2}"


@cocotb.test()
async def test_timer_exact_deadline(dut):
    """Timer wakes at its exact deadline in the shared ps time base."""
    t0 = get_sim_time('ps')
    await Timer(1, 'ps')
    assert get_sim_time('ps') - t0 == 1
    await Timer(3333, 'ps')
    assert get_sim_time('ps') - t0 == 1 + 3333
    await Timer(1.5, 'ns')
    assert get_sim_time('ps') - t0 == 1 + 3333 + 1500
    await Timer(2, 'us')
    assert get_sim_time('ps') - t0 == 1 + 3333 + 1500 + 2_000_000


@cocotb.test()
async def test_get_sim_time_units(dut):
    """get_sim_time() converts correctly between ps, ns, us, ms."""
    await Timer(1234567, 'ps')
    ps = get_sim_time('ps')
    assert ps == 1234567
    assert get_sim_time() == 1234567  # 'step' == 1 ps
    assert abs(get_sim_time('ns') - 1234.567) < 1e-9
    assert abs(get_sim_time('us') - 1.234567) < 1e-12
    assert abs(get_sim_time('ms') - 0.001234567) < 1e-15


@cocotb.test()
async def test_readonly_no_time_advance(dut):
    """ReadOnly() resumes in the current time step."""
    await start_clock(dut)
    await RisingEdge(dut.clk)
    before = get_sim_time('ps')
    await ReadOnly()
    assert get_sim_time('ps') == before
    # ReadOnly observes same-timestep writes from other coroutines.
    seen = {}

    async def writer():
        await RisingEdge(dut.clk)
        dut.in_a.value = 0x5A

    async def reader():
        await RisingEdge(dut.clk)
        await ReadOnly()
        seen['a'] = int(dut.in_a.value)
        seen['t'] = get_sim_time('ps')

    cocotb.start_soon(writer())
    cocotb.start_soon(reader())
    await RisingEdge(dut.clk)
    edge_t = get_sim_time('ps')
    await Timer(1, 'ps')
    assert seen == {'a': 0x5A, 't': edge_t}, seen


# ── Tasks And Triggers ────────────────────────────────────────────────


@cocotb.test()
async def test_deterministic_concurrent_observation(dut):
    """Concurrent tasks observe the same deterministic signal sequence."""
    await start_clock(dut, 10, 'ns')
    await reset_dut(dut)
    seqs = [[], []]

    async def watcher(idx):
        for _ in range(5):
            await RisingEdge(dut.clk)
            seqs[idx].append(int(dut.count.value))

    t1 = cocotb.start_soon(watcher(0))
    t2 = cocotb.start_soon(watcher(1))
    await ClockCycles(dut.clk, 7)
    assert t1.done() and t2.done()
    assert seqs[0] == seqs[1], seqs
    assert seqs[0] == sorted(seqs[0]), seqs  # monotone counter


@cocotb.test()
async def test_event_wakes_all_waiters(dut):
    """Event wakes every current waiter after set()."""
    ev = Event()
    woke = []

    async def waiter(i):
        await ev.wait()
        woke.append(i)

    for i in range(4):
        cocotb.start_soon(waiter(i))
    await Timer(5, 'ns')
    assert woke == []
    ev.set()
    await Timer(1, 'ns')
    assert sorted(woke) == [0, 1, 2, 3], woke


@cocotb.test()
async def test_first_returns_first_and_cleans_losers(dut):
    """First returns the first completed trigger; losers are removed."""
    await start_clock(dut, 10, 'ns')
    # Anchor mid-cycle: the next rising edge is a full period away, so
    # the 2 ns timer must win the race.
    await RisingEdge(dut.clk)
    fast = Timer(2, 'ns')
    slow = Timer(200, 'ns')
    edge = RisingEdge(dut.clk)
    t0 = get_sim_time('ps')
    winner = await First(slow, fast, edge)
    assert winner is fast
    assert get_sim_time('ps') - t0 == 2000
    # The losing edge registration must not linger: no waiters remain
    # on clk after a settle point.
    sim = __import__('arch_cocotb.simulator', fromlist=['_get_sim'])._get_sim()
    await Timer(1, 'ns')
    for name, waiters in sim._edge_waiters.items():
        live = [w for w in waiters if not w.fut.done()]
        assert not live, f"stale edge registration on {name}"


@cocotb.test()
async def test_with_timeout_result_and_deadline(dut):
    """with_timeout returns results and raises SimTimeoutError on time."""
    trig = await with_timeout(Timer(3, 'ns'), 100, 'ns')
    assert trig is not None
    t0 = get_sim_time('ps')
    try:
        await with_timeout(Timer(500, 'ns'), 7, 'ns')
        raise AssertionError("expected SimTimeoutError")
    except SimTimeoutError:
        pass
    assert get_sim_time('ps') - t0 == 7000


@cocotb.test()
async def test_kill_stops_execution_and_writes(dut):
    """kill() prevents further execution and signal writes."""
    await start_clock(dut, 10, 'ns')
    await reset_dut(dut)
    writes = []

    async def victim():
        n = 0
        while True:
            await RisingEdge(dut.clk)
            n += 1
            dut.in_a.value = n
            writes.append(n)

    task = cocotb.start_soon(victim())
    await ClockCycles(dut.clk, 4)
    task.kill()
    n_at_kill = len(writes)
    val_at_kill = int(dut.in_a.value)
    await ClockCycles(dut.clk, 4)
    assert task.done()
    assert len(writes) == n_at_kill
    assert int(dut.in_a.value) == val_at_kill
    # No stale trigger registrations survive the kill.
    sim = __import__('arch_cocotb.simulator', fromlist=['_get_sim'])._get_sim()
    for name, waiters in sim._edge_waiters.items():
        for w in waiters:
            assert w.fut.done() or name == 'clk', \
                f"stale registration on {name}"


@cocotb.test()
async def test_queue_semantics(dut):
    """Queue provides async FIFO behavior; QueueFull matches cocotb."""
    q = Queue(maxsize=2)
    q.put_nowait('a')
    q.put_nowait('b')
    try:
        q.put_nowait('c')
        raise AssertionError("expected QueueFull")
    except QueueFull:
        pass
    got = []

    async def consumer():
        for _ in range(3):
            got.append(await q.get())

    cocotb.start_soon(consumer())
    await Timer(1, 'ns')
    await q.put('c')  # blocks until consumer drains one
    await Timer(1, 'ns')
    assert got == ['a', 'b', 'c'], got


# ── Handles ───────────────────────────────────────────────────────────


@cocotb.test()
async def test_ports_in_dir(dut):
    """Every top-level port appears in dir(dut)."""
    names = dir(dut)
    for p in ('clk', 'rst', 'in_a', 'in_b', 'wide_in',
              'count', 'echo_a', 'echo_b', 'wide_out'):
        assert p in names, f"{p} missing from dir(dut)"


@cocotb.test()
async def test_prefix_discovery(dut):
    """Prefix-based discovery over dir(dut) finds matching signals."""
    hits = [n for n in dir(dut) if n.startswith('echo_')]
    assert sorted(hits) == ['echo_a', 'echo_b'], hits
    # And hasattr-based discovery (cocotb_bus style) agrees.
    assert hasattr(dut, 'echo_a') and hasattr(dut, 'echo_b')
    assert not hasattr(dut, 'echo_c')


@cocotb.test()
async def test_len_reports_width(dut):
    """len(signal) and len(signal.value) report the declared width."""
    assert len(dut.in_a) == 8
    assert len(dut.in_b) == 12
    assert len(dut.wide_in) == 48
    assert len(dut.count) == 16
    assert len(dut.in_a.value) == 8
    assert len(dut.wide_in.value) == 48


@cocotb.test()
async def test_setimmediatevalue(dut):
    """setimmediatevalue() updates an input with no scheduled delay."""
    await start_clock(dut, 10, 'ns')
    dut.in_a.setimmediatevalue(0x7E)
    await ReadOnly()  # settle combinational echo without advancing time
    assert int(dut.echo_a.value) == 0x7E


@cocotb.test()
async def test_signed_unsigned_conversion(dut):
    """Signed conversion uses the declared width, not host width."""
    await start_clock(dut, 10, 'ns')
    dut.in_b.value = -5  # SInt<12>, masked to 12 bits
    await ReadOnly()
    v = dut.echo_b.value
    assert v.to_unsigned() == 0xFFB
    assert v.to_signed() == -5
    dut.in_b.value = 0x800  # most negative 12-bit value
    await Timer(1, 'ns')
    assert dut.echo_b.value.to_signed() == -2048


@cocotb.test()
async def test_wide_masking(dut):
    """Wide values are masked to the declared width on write."""
    await start_clock(dut, 10, 'ns')
    dut.wide_in.value = (1 << 60) | 0xDEADBEEF0000 | 0x1234
    await ReadOnly()
    expect = ((1 << 60) | 0xDEADBEEF0000 | 0x1234) & ((1 << 48) - 1)
    assert int(dut.wide_out.value) == expect
    dut.wide_in.value = -1
    await Timer(1, 'ns')
    assert int(dut.wide_out.value) == (1 << 48) - 1


@cocotb.test()
async def test_logicarray_assignment(dut):
    """LogicArray inputs convert X/Z deterministically to zero."""
    await start_clock(dut, 10, 'ns')
    dut.in_a.value = LogicArray("1X1Z01X1")
    await ReadOnly()
    assert int(dut.echo_a.value) == 0b10100101
