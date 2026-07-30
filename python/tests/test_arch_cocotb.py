"""Conformance tests for the native ARCH cocotb compatibility layer."""

import asyncio
import unittest

import cocotb
from arch_cocotb.dut import ArchDUT
from arch_cocotb.simulator import ArchSimulator
from cocotb.clock import Clock
from cocotb.queue import Queue, QueueEmpty, QueueFull
from cocotb.triggers import (
    ClockCycles,
    Edge,
    Event,
    First,
    ReadOnly,
    RisingEdge,
    SimTimeoutError,
    Timer,
    with_timeout,
)
from cocotb.types import LogicArray
from cocotb.utils import get_sim_time


class FakeModel:
    """Small edge-sensitive model matching the generated pybind API."""

    def __init__(self):
        self.clk = 0
        self.data = 0
        self.q = 0
        self.wide = 0
        self._clock_previous = 0
        self.eval_count = 0

    @staticmethod
    def _port_info():
        return [
            ("clk", 1, False, True, False, False),
            ("data", 8, False, True, False, False),
            ("q", 8, False, False, False, False),
            ("wide", 96, False, True, False, False),
            ("DEPTH", 32, False, False, True, False),
        ]

    @property
    def DEPTH(self):
        return 16

    def eval(self):
        self.eval_count += 1
        if self._clock_previous == 0 and self.clk != 0:
            self.q = self.data
        self._clock_previous = self.clk


class IdlessAxiModel:
    """Minimal flattened AXI4 interface with no physical ID ports."""

    _input_suffixes = {
        "arready",
        "rdata",
        "rvalid",
        "awready",
        "wready",
        "bvalid",
    }

    def __init__(self):
        for name, *_ in self._port_info():
            setattr(self, name, 0)

    @classmethod
    def _port_info(cls):
        names = [
            "m_axi_araddr",
            "m_axi_arvalid",
            "m_axi_arready",
            "m_axi_rdata",
            "m_axi_rvalid",
            "m_axi_rready",
            "m_axi_awaddr",
            "m_axi_awvalid",
            "m_axi_awready",
            "m_axi_wdata",
            "m_axi_wvalid",
            "m_axi_wready",
            "m_axi_bvalid",
            "m_axi_bready",
        ]
        return [
            (
                name,
                32 if name.endswith(("addr", "data")) else 1,
                False,
                name.rsplit("_", 1)[-1] in cls._input_suffixes,
                False,
                False,
            )
            for name in names
        ]


class IdlessAxiTargetModel(IdlessAxiModel):
    """The same ID-less interface viewed from an AXI target DUT."""

    _input_suffixes = {
        "araddr",
        "arvalid",
        "rready",
        "awaddr",
        "awvalid",
        "wdata",
        "wvalid",
        "bready",
    }


class PhasedHandshakeModel:
    """Registered valid/ready model used to verify cocotb sampling phases."""

    def __init__(self):
        self.clk = 0
        self.trigger = 0
        self.ready = 0
        self.valid = 0
        self._valid_reg = 0
        self._clock_previous = 0

    @staticmethod
    def _port_info():
        return [
            ("clk", 1, False, True, False, False),
            ("trigger", 1, False, True, False, False),
            ("ready", 1, False, True, False, False),
            ("valid", 1, False, False, False, False),
        ]

    @staticmethod
    def _arch_supports_phased_eval():
        return True

    def eval(self):
        self.eval_comb()
        self.eval_posedge()
        self.eval_comb()

    def eval_comb(self):
        self.valid = self._valid_reg

    def eval_posedge(self):
        rising = not self._clock_previous and self.clk
        self._clock_previous = self.clk
        if rising:
            if self._valid_reg and self.ready:
                self._valid_reg = 0
            elif self.trigger:
                self._valid_reg = 1


def run_native(test_body):
    dut = ArchDUT(FakeModel, name="dut")
    simulator = ArchSimulator(dut)
    return asyncio.run(simulator.run_test(test_body, dut)), simulator, dut


class TimingTests(unittest.TestCase):
    def test_exact_picosecond_clock_and_time_conversion(self):
        async def body(dut):
            cocotb.start_soon(
                Clock(dut.clk, 3333, units="ps").start(start_high=False)
            )
            times = []
            for _ in range(4):
                await Edge(dut.clk)
                times.append(get_sim_time("ps"))
            self.assertEqual(times, [1667, 3333, 5000, 6666])
            self.assertEqual(get_sim_time("fs"), 6_666_000)
            self.assertEqual(get_sim_time("ns"), 6.666)
            self.assertEqual(get_sim_time("us"), 0.006666)
            self.assertEqual(get_sim_time("ms"), 0.000006666)

        run_native(body)

    def test_timer_deadline_and_readonly_phase(self):
        async def body(dut):
            await Timer(1251, units="ps")
            self.assertEqual(get_sim_time("ps"), 1251)
            before = get_sim_time("ps")
            dut.data.value = 0x5A
            await ReadOnly()
            self.assertEqual(get_sim_time("ps"), before)

        run_native(body)

    def test_event_driven_model_does_not_tick_when_idle(self):
        async def body(dut):
            await Timer(100, units="us")

        _, _, dut = run_native(body)
        self.assertEqual(dut._model.eval_count, 1)

    def test_registered_valid_is_sampled_before_post_edge_comb(self):
        async def body(dut):
            cocotb.start_soon(
                Clock(dut.clk, 10, units="ps").start(start_high=False)
            )
            wake = Event()
            captured = Event()

            async def valid_monitor():
                while True:
                    await RisingEdge(dut.valid)
                    wake.set()

            async def sink():
                while True:
                    await RisingEdge(dut.clk)
                    if int(dut.valid.value) and int(dut.ready.value):
                        captured.set()
                        return
                    wake.clear()
                    await wake.wait()

            cocotb.start_soon(valid_monitor())
            cocotb.start_soon(sink())
            dut.ready.value = 1
            dut.trigger.value = 1
            await RisingEdge(dut.clk)
            dut.trigger.value = 0
            await with_timeout(captured.wait(), 30, "ps")
            await ReadOnly()
            self.assertEqual(int(dut.valid.value), 0)

        dut = ArchDUT(PhasedHandshakeModel, name="dut")
        simulator = ArchSimulator(dut)
        asyncio.run(simulator.run_test(body, dut))


class TaskAndTriggerTests(unittest.TestCase):
    def test_event_wakes_all_waiters_with_data(self):
        async def body(dut):
            event = Event()
            results = []

            async def waiter(index):
                results.append((index, await event.wait()))

            first = cocotb.start_soon(waiter(1))
            second = cocotb.start_soon(waiter(2))
            await Timer(1, units="ps")
            event.set("ready")
            await first
            await second
            self.assertEqual(sorted(results), [(1, "ready"), (2, "ready")])
            self.assertTrue(event.is_set())
            self.assertEqual(event.data, "ready")
            event.clear()
            self.assertFalse(event.is_set())

        run_native(body)

    def test_first_cleans_loser_and_timeout_behaves(self):
        async def body(dut):
            early = Timer(7, units="ps")
            late = Timer(100, units="ps")
            self.assertIs(await First(late, early), early)
            await Timer(0, units="ps")
            simulator = dut._simulator
            wake_times = [item[0] for item in simulator._timer_heap]
            self.assertNotIn(100, wake_times)

            async def result():
                await Timer(3, units="ps")
                return 42

            self.assertEqual(await with_timeout(result(), 10, "ps"), 42)
            trigger = Timer(1, units="ps")
            self.assertIs(await with_timeout(trigger, 2, "ps"), trigger)
            with self.assertRaises(SimTimeoutError):
                await with_timeout(result(), 2, "ps")

        run_native(body)

    def test_task_handle_queue_and_kill(self):
        async def body(dut):
            queue = Queue(maxsize=1)
            queue.put_nowait(1)
            with self.assertRaises(QueueFull):
                queue.put_nowait(2)
            self.assertEqual(queue.get_nowait(), 1)
            with self.assertRaises(QueueEmpty):
                queue.get_nowait()

            async def writer():
                await Timer(10, units="ps")
                dut.data.value = 0xFF

            task = cocotb.start_soon(writer())
            self.assertFalse(task.done())
            task.kill()
            await Timer(20, units="ps")
            self.assertTrue(task.done())
            self.assertEqual(int(dut.data.value), 0)

        run_native(body)

    def test_clock_cycles_and_deterministic_concurrency(self):
        async def body(dut):
            cocotb.start_soon(
                Clock(dut.clk, 10, units="ps").start(start_high=False)
            )
            observed_a = []
            observed_b = []

            async def observer(target):
                for _ in range(3):
                    await RisingEdge(dut.clk)
                    await ReadOnly()
                    target.append(int(dut.q.value))

            first = cocotb.start_soon(observer(observed_a))
            second = cocotb.start_soon(observer(observed_b))
            for value in (1, 2, 3):
                dut.data.value = value
                await ClockCycles(dut.clk, 1)
            await first
            await second
            self.assertEqual(observed_a, observed_b)
            self.assertEqual(observed_a, [1, 2, 3])

        run_native(body)


class HandleTests(unittest.TestCase):
    def test_discovery_width_conversion_and_masking(self):
        async def body(dut):
            self.assertIn("data", dir(dut))
            self.assertIn("DEPTH", dir(dut))
            self.assertIs(dut.DATA, dut.data)
            self.assertEqual([signal._name for signal in dut], [
                "clk",
                "data",
                "q",
                "wide",
            ])
            self.assertEqual(len(dut.data), 8)
            self.assertEqual(len(dut.data.value), 8)
            dut.data.setimmediatevalue(0x1FF)
            self.assertEqual(int(dut.data.value), 0xFF)
            dut.wide.value = (1 << 120) | 0x1234
            self.assertEqual(int(dut.wide.value), 0x1234)

            dut.data.value = 0xFF
            self.assertEqual(dut.data.value.to_signed(), -1)
            self.assertEqual(dut.data.value.to_unsigned(), 255)
            logic = LogicArray("10xz")
            self.assertEqual(len(logic), 4)
            self.assertEqual(int(logic), 8)

        run_native(body)

    def test_idless_axi_gets_single_id_compatibility_handles(self):
        for model, input_ids in (
            (IdlessAxiModel, {"m_axi_rid", "m_axi_bid"}),
            (IdlessAxiTargetModel, {"m_axi_arid", "m_axi_awid"}),
        ):
            dut = ArchDUT(model, name="dut")
            for name in ("m_axi_arid", "m_axi_rid", "m_axi_awid", "m_axi_bid"):
                self.assertIn(name, dir(dut))
                signal = getattr(dut, name)
                self.assertEqual(len(signal), 1)
                self.assertEqual(int(signal.value), 0)
                self.assertEqual(signal._is_input, name in input_ids)
                signal.setimmediatevalue(3)
                self.assertEqual(int(signal.value), 1)

            with self.assertRaises(AttributeError):
                getattr(dut, "m_axi_missing")


if __name__ == "__main__":
    unittest.main()
