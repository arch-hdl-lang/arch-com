#!/usr/bin/env python3
"""Reproduce the exact harness test with various MSHR sizes and clock periods."""
import subprocess, tempfile, os, shutil

sv_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cache_mshr.sv')

# Exact copy of the harness test
test_code = '''
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import FallingEdge, RisingEdge, ClockCycles, Timer
import random
import time

async def dut_init(dut):
    for signal in dut:
        if signal._type == "GPI_NET" or signal._name in {'allocate_addr', 'reset', 'finalize_id', 'finalize_valid', 'allocate_data', 'allocate_valid', 'allocate_rw', 'clk', 'data'}:
            signal.value = 0

async def reset_dut(reset, duration_ns=10):
    reset.value = 0
    await Timer(duration_ns, units="ns")
    reset.value = 1
    await Timer(duration_ns, units="ns")
    reset.value = 0
    await Timer(duration_ns, units='ns')
    reset._log.debug("Reset complete")

@cocotb.test()
async def test_cvdp_copilot_cache_mshr(dut):
   dut_clock_period = CLK_PERIOD
   DUT_CLK = Clock(dut.clk, dut_clock_period, 'ns')
   await cocotb.start(DUT_CLK.start())
   dut.clk._log.info(f"clk STARTED")

   await dut_init(dut)
   await reset_dut(dut.reset, dut_clock_period)

   for i in range(2):
      await RisingEdge(dut.clk)

   MSHR_SIZE = int(dut.MSHR_SIZE.value)
   CS_LINE_ADDR_WIDTH = int(dut.CS_LINE_ADDR_WIDTH.value)
   WORD_SEL_WIDTH = int(dut.WORD_SEL_WIDTH.value)
   WORD_SIZE = int(dut.WORD_SIZE.value)
   MSHR_ADDR_WIDTH = int(dut.MSHR_ADDR_WIDTH.value)
   TAG_WIDTH = int(dut.TAG_WIDTH.value)
   CS_WORD_WIDTH = int(dut.CS_WORD_WIDTH.value)
   DATA_WIDTH = int(dut.DATA_WIDTH.value)

   dut._log.info(f"MSHR_SIZE={MSHR_SIZE} ADDR_W={MSHR_ADDR_WIDTH} DATA_W={DATA_WIDTH}")

   assert dut.allocate_id.value == 0, f"allocate_id not 0: {dut.allocate_id.value}"
   assert dut.allocate_ready.value == 1, f"allocate_ready not 1: {dut.allocate_ready.value}"

   #1. Fill
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 1
   cycles_to_full = 0
   while (dut.allocate_ready.value == 1):
      await RisingEdge(dut.clk)
      cycles_to_full = cycles_to_full + 1
      await FallingEdge(dut.clk)
   dut.allocate_valid.value = 0
   assert cycles_to_full == MSHR_SIZE, f"full: {cycles_to_full} != {MSHR_SIZE}"
   dut._log.info(f"Test 1 PASS")

   await reset_dut(dut.reset, dut_clock_period)

   #2. Linked list
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 1
   dut.allocate_addr.value = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
   dut.finalize_valid.value = 0

   for i in range(MSHR_SIZE):
      dut.allocate_rw.value = random.randint(0,1)
      dut.allocate_data.value = random.randint(0, DATA_WIDTH)
      await FallingEdge(dut.clk)
      allocate_id_val = int(dut.allocate_id.value)
      dut._log.info(f"i={i}: id={allocate_id_val} pending={int(dut.allocate_pending.value)} previd={int(dut.allocate_previd.value)}")
      assert allocate_id_val == i, f"ID mismatch: expected {i}, got: {allocate_id_val}"
      if i != 0:
         assert int(dut.allocate_pending.value) == 1
         assert int(dut.allocate_previd.value) == i-1
   dut.allocate_valid.value = 0
   dut._log.info("Test 2 PASS")
'''

results = []
for mshr_size in [4, 8, 12, 16, 20, 24, 28, 32]:
    for clk_period in [2, 6, 10, 14, 20]:
        workdir = tempfile.mkdtemp(prefix='mshr_exact_')
        os.makedirs(os.path.join(workdir, 'rtl'), exist_ok=True)
        shutil.copy(sv_file, os.path.join(workdir, 'rtl', 'cache_mshr.sv'))

        tc = test_code.replace('CLK_PERIOD', str(clk_period))
        with open(os.path.join(workdir, 'test_mshr.py'), 'w') as f:
            f.write(tc)

        runner_code = f'''
import os
from cocotb_tools.runner import get_runner
runner = get_runner("icarus")
runner.build(
    sources=["{os.path.join(workdir, 'rtl', 'cache_mshr.sv')}"],
    hdl_toplevel="cache_mshr",
    parameters={{'MSHR_SIZE': {mshr_size}}},
    always=True, clean=True, waves=False, verbose=False,
    timescale=("1ns", "1ns"), log_file="sim.log")
runner.test(hdl_toplevel="cache_mshr", test_module="test_mshr", waves=False)
'''
        with open(os.path.join(workdir, 'run.py'), 'w') as f:
            f.write(runner_code)

        env = os.environ.copy()
        env['MODULE'] = 'test_mshr'
        env['TOPLEVEL'] = 'cache_mshr'
        env['SIM'] = 'icarus'
        env['TOPLEVEL_LANG'] = 'verilog'
        env['VERILOG_SOURCES'] = os.path.join(workdir, 'rtl', 'cache_mshr.sv')

        r = subprocess.run(
            ['python3', os.path.join(workdir, 'run.py')],
            cwd=workdir, env=env,
            capture_output=True, text=True, timeout=60
        )
        passed = r.returncode == 0 and 'PASS=1' in r.stdout
        if not passed:
            results.append(f"FAIL: MSHR={mshr_size} CLK={clk_period}")
            # Print details
            for line in r.stdout.split('\n'):
                if 'Test 1' in line or 'Test 2' in line or 'FAIL' in line:
                    results.append(f"  {line.strip()}")
        shutil.rmtree(workdir)

if results:
    print("FAILURES:")
    for r in results:
        print(r)
else:
    print("ALL PASS")
