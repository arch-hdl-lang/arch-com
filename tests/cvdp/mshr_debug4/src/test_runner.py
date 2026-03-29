import os
from cocotb_tools.runner import get_runner
import pytest
import random

verilog_sources = os.getenv("VERILOG_SOURCES").split()
toplevel_lang   = os.getenv("TOPLEVEL_LANG")
sim             = os.getenv("SIM", "icarus")
toplevel        = os.getenv("TOPLEVEL")
module          = os.getenv("MODULE")
wave            = bool(os.getenv("WAVE"))

def runner(MSHR_SIZE):
    print("Inside Runner")
    print(MSHR_SIZE)
    runner = get_runner(sim)
    runner.build(
        sources=verilog_sources,
        hdl_toplevel=toplevel,
        parameters={'MSHR_SIZE': MSHR_SIZE},
        always=True,
        clean=True,
        waves=wave,
        verbose=False,
        timescale=("1ns", "1ns"),
        log_file="sim.log")
    print("Running")    
    runner.test(hdl_toplevel=toplevel, test_module=module, waves=True)
# random test 
test_param = [(random.randint(1, 8) * 4) for _ in range(10)]

@pytest.mark.parametrize('MSHR_SIZE', test_param )
def test_allocator(MSHR_SIZE):
    print("Calling Runner")
    runner(MSHR_SIZE)

if __name__ == "__main__":
    import pytest; pytest.main([__file__, "-x", "-v"])
