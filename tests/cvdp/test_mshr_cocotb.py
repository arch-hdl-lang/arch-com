#!/usr/bin/env python3
"""Full cocotb test matching the benchmark exactly, testing all 4 sections."""
import subprocess, tempfile, os, shutil

sv_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cache_mshr.sv')

test_code = '''
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import FallingEdge, RisingEdge, Timer
import random

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

@cocotb.test()
async def test_full(dut):
    dut_clock_period = CLK_PERIOD
    DUT_CLK = Clock(dut.clk, dut_clock_period, 'ns')
    await cocotb.start(DUT_CLK.start())
    await dut_init(dut)
    await reset_dut(dut.reset, dut_clock_period)

    for _ in range(2):
        await RisingEdge(dut.clk)

    MSHR_SIZE = int(dut.MSHR_SIZE.value)
    CS_LINE_ADDR_WIDTH = int(dut.CS_LINE_ADDR_WIDTH.value)
    DATA_WIDTH = int(dut.DATA_WIDTH.value)

    assert dut.allocate_id.value == 0
    assert dut.allocate_ready.value == 1

    # Test 1: Fill
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 1
    cycles = 0
    while dut.allocate_ready.value == 1:
        await RisingEdge(dut.clk)
        cycles += 1
        await FallingEdge(dut.clk)
    dut.allocate_valid.value = 0
    assert cycles == MSHR_SIZE, f"Fill: {cycles} != {MSHR_SIZE}"

    await reset_dut(dut.reset, dut_clock_period)

    # Test 2: Linked list
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 1
    dut.allocate_addr.value = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
    dut.finalize_valid.value = 0
    for i in range(MSHR_SIZE):
        dut.allocate_rw.value = random.randint(0,1)
        dut.allocate_data.value = random.randint(0, DATA_WIDTH)
        await FallingEdge(dut.clk)
        assert int(dut.allocate_id.value) == i, f"T2 ID: {int(dut.allocate_id.value)} != {i}"
        if i != 0:
            assert int(dut.allocate_pending.value) == 1
            assert int(dut.allocate_previd.value) == i-1
    dut.allocate_valid.value = 0

    await reset_dut(dut.reset, dut_clock_period)

    # Test 3: Allocate+finalize reuse
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 1
    addr = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
    dut.allocate_addr.value = addr
    dut.allocate_rw.value = random.randint(0,1)
    dut.allocate_data.value = random.randint(0, DATA_WIDTH)
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 0
    allocated_id = int(dut.allocate_id.value)
    dut.finalize_valid.value = 1
    dut.finalize_id.value = allocated_id
    await FallingEdge(dut.clk)
    dut.finalize_valid.value = 0
    dut.allocate_valid.value = 0
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 1
    dut.allocate_addr.value = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 0
    assert allocated_id == int(dut.allocate_id.value), f"T3: {allocated_id} != {int(dut.allocate_id.value)}"

    await reset_dut(dut.reset, dut_clock_period)

    # Test 4: Simultaneous allocate+finalize
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 1
    addr = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
    dut.allocate_addr.value = addr
    dut.allocate_rw.value = random.randint(0,1)
    dut.allocate_data.value = random.randint(0, DATA_WIDTH)
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 1
    dut.allocate_addr.value = addr % 4
    dut.allocate_rw.value = random.randint(0,1)
    dut.allocate_data.value = random.randint(0, DATA_WIDTH)
    allocated_id = int(dut.allocate_id.value)
    dut.finalize_valid.value = 1
    dut.finalize_id.value = allocated_id
    await FallingEdge(dut.clk)
    dut.allocate_valid.value = 0
    dut.finalize_valid.value = 0
    assert int(dut.allocate_id.value) == 1, f"T4: {int(dut.allocate_id.value)} != 1"

    dut._log.info("ALL TESTS PASSED")
'''

for clk_period in [2, 4, 6, 8, 10, 12, 14, 16, 18, 20]:
    workdir = tempfile.mkdtemp(prefix='mshr_test_')
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
    parameters={{'MSHR_SIZE': 24}},
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
        capture_output=True, text=True, timeout=30
    )
    passed = r.returncode == 0 and 'PASS=1' in r.stdout
    fail_msg = ''
    if not passed:
        for line in r.stdout.split('\n'):
            if 'FAIL' in line and 'assert' in line.lower():
                fail_msg = line.strip()
                break
        for line in r.stderr.split('\n'):
            if 'Error' in line or 'error' in line:
                fail_msg = line.strip()
                break
    print(f"clk_period={clk_period:2d}: {'PASS' if passed else 'FAIL'} {fail_msg}")
    shutil.rmtree(workdir)
