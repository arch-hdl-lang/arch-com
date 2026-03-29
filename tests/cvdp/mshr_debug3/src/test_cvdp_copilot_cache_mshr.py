import cocotb
from cocotb.clock import Clock
from cocotb.triggers import FallingEdge, RisingEdge, ClockCycles, Timer
import random
import time
import harness_library as hrs_lb



@cocotb.test()
async def test_cvdp_copilot_cache_mshr(dut): 
   # Start clock
   dut_clock_period = random.randint(2, 20) # starting from 2, t high must be integer! 
   print(f"Clk period is {dut_clock_period}")
   DUT_CLK = Clock(dut.clk, dut_clock_period, 'ns')
   await cocotb.start(DUT_CLK.start())
   dut.clk._log.info(f"clk STARTED")

   await hrs_lb.dut_init(dut)

   # Apply reset 
   await hrs_lb.reset_dut(dut.reset, dut_clock_period)

   for i in range(2):
      await RisingEdge(dut.clk)

   # Ensure  outputs reset value 
   assert dut.allocate_id.value == 0, f"allocate_id is not zero after reset: {dut.allocate_id.value}"
   assert dut.allocate_ready.value == 1, f"allocate_ready should be asserted: {dut.allocate_ready.value}"

   # Get parameter values from top module
   MSHR_SIZE = int(dut.MSHR_SIZE.value)
   CS_LINE_ADDR_WIDTH   = int(dut.CS_LINE_ADDR_WIDTH.value)
   WORD_SEL_WIDTH       = int(dut.WORD_SEL_WIDTH.value)
   WORD_SIZE            = int(dut.WORD_SIZE.value)
   MSHR_ADDR_WIDTH      = int(dut.MSHR_ADDR_WIDTH.value) 
   TAG_WIDTH            = int(dut.TAG_WIDTH.value) 
   CS_WORD_WIDTH        = int(dut.CS_WORD_WIDTH.value)
   DATA_WIDTH           = int(dut.DATA_WIDTH.value)

   #1. Testing Sequential Full condition: Setting acquire request for MSHR_SIZE cycles After reset (empty) should result in Full assertion
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 1
   cycles_to_full = 0
   while (dut.allocate_ready.value == 1):
      await RisingEdge(dut.clk)
      cycles_to_full = cycles_to_full + 1
      await FallingEdge(dut.clk)
   dut.allocate_valid.value = 0
   assert cycles_to_full == MSHR_SIZE, f"full should be asserted. Asserted after: {cycles_to_full}, Expected: {MSHR_SIZE}"

   await hrs_lb.reset_dut(dut.reset, dut_clock_period)  

   #2. Test linked list structure , requests to the same cache line are misses
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 1
   # generate random cache address
   dut.allocate_addr.value = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
    
   # this case doesn't exercise finalize
   dut.finalize_valid.value = 0

   for i in range(MSHR_SIZE):
      dut.allocate_rw.value   = random.randint(0,1) 
      dut.allocate_data.value = random.randint(0, DATA_WIDTH) 
      await FallingEdge(dut.clk)
      allocate_id_val = int(dut.allocate_id.value)
      

      assert allocate_id_val == i, f"ID mismatch: expected {i}, got: {allocate_id_val}"
      if i !=0:
         allocate_pending_val = int(dut.allocate_pending.value)
         allocate_previd_val = int(dut.allocate_previd.value)
         assert allocate_pending_val == 1, f"Pending should be asserted"
         assert allocate_previd_val == i-1, f"Pending should be asserted"
      
   dut.allocate_valid.value = 0
   

   await hrs_lb.reset_dut(dut.reset, dut_clock_period) 
   
   # 3. Test the scenario where a request is allocated then it found out to be a hit so it will be released
   # the way to check it's released for now is to have allocate req and the idx should match the one recently released
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 1
   addr = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
   rw    = random.randint(0,1)
   data =  random.randint(0, DATA_WIDTH)

   dut.allocate_addr.value = addr
   dut.allocate_rw.value = rw
   dut.allocate_data.value = data 
  
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 0
   allocated_id = int(dut.allocate_id.value)
   hit = 1
   dut.finalize_valid.value = hit
   dut.finalize_id.value =   allocated_id     
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 0
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 1
   addr = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
   rw    = random.randint(0,1)
   data =  random.randint(0, DATA_WIDTH)
   await FallingEdge(dut.clk)
   dut.allocate_valid.value = 0
   assert allocated_id ==  int(dut.allocate_id.value), f"ERROR"
      

   await hrs_lb.reset_dut(dut.reset, dut_clock_period) 

   #4. Allocate and finalize at the same cycle
   # a . allocate an address
   # b . Allocate another address while finalizing the first one
   
   await FallingEdge(dut.clk) # allocate 0x0
   dut.allocate_valid.value = 1
   addr = random.randint(0, 2**CS_LINE_ADDR_WIDTH-1)
   rw    = random.randint(0,1)
   data =  random.randint(0, DATA_WIDTH)

   dut.allocate_addr.value = addr
   dut.allocate_rw.value = rw
   dut.allocate_data.value = data 
  
   await FallingEdge(dut.clk) # allocate + finalize 0x1 and release 0x0

   dut.allocate_valid.value = 1
   addr = addr%4
   rw    = random.randint(0,1)
   data =  random.randint(0, DATA_WIDTH)

   dut.allocate_addr.value = addr
   dut.allocate_rw.value = rw
   dut.allocate_data.value = data 

   allocated_id = int(dut.allocate_id.value)
   hit = 1
   dut.finalize_valid.value = hit
   dut.finalize_id.value =   allocated_id  

   allocate_id_val = int(dut.allocate_id.value)
      
   await FallingEdge(dut.clk) 
   dut.allocate_valid.value = 0
   allocate_id_val = int(dut.allocate_id.value)

   assert allocate_id_val == 1, f"ID mismatch: expected {1}, got: {allocate_id_val}"
