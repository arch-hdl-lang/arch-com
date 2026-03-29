
from cocotb.triggers import FallingEdge, RisingEdge, Timer
import random




async def dut_init(dut):
    # iterate all the input signals and initialize with 0
    for signal in dut:
        if (signal._type == "GPI_NET" or signal._name in {'allocate_addr', 'reset', 'finalize_id', 'finalize_valid', 'allocate_data', 'allocate_valid', 'allocate_rw', 'clk', 'data'}):
            signal.value = 0

async def reset_dut(reset, duration_ns = 10):
    # Restart Interface
    reset.value = 0
    await Timer(duration_ns, units="ns")
    reset.value = 1
    await Timer(duration_ns, units="ns")
    reset.value = 0
    await Timer(duration_ns, units='ns')
    reset._log.debug("Reset complete")


    
    
     
        
    