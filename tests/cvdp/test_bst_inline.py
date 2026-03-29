import cocotb
from cocotb.triggers import RisingEdge, Timer
import random

async def clock(dut, period=10):
    while True:
        dut.clk.value = 0
        await Timer(period/2, units='ns')
        dut.clk.value = 1
        await Timer(period/2, units='ns')

async def reset_dut(dut, duration):
    dut.reset.value = 1
    for _ in range(duration):
        await RisingEdge(dut.clk)
    dut.reset.value = 0
    await RisingEdge(dut.clk)

def calculate_latency(array_size, sorted_flag):
    if array_size == 1:
        latency_build_tree = 5
        latency_sort_tree = 7

    if sorted_flag:
        latency_start = 1
        latency_build_tree = ((array_size - 1) * array_size)/2 + 2 * array_size + 2
        latency_sort_tree = 4 * array_size + 3

    total_latency = latency_start + latency_build_tree + latency_sort_tree
    return total_latency

async def run_test_case(name, dut, input_array, data_width, array_size, sort_flag):
    cocotb.log.info(f"Running Test: {name}")
    packed_input = 0
    for idx, val in enumerate(input_array):
        packed_input |= (val << (idx * data_width))
    dut.data_in.value = packed_input

    await RisingEdge(dut.clk)
    dut.start.value = 1
    await RisingEdge(dut.clk)
    dut.start.value = 0

    cycle_count = 0
    while True:
        await RisingEdge(dut.clk)
        cycle_count += 1
        if cycle_count > 10000:
            cocotb.log.error(f"TIMEOUT in {name}")
            assert False, f"Timeout in {name}"
        if dut.done.value == 1:
            break

    out_data_val = int(dut.sorted_out.value)
    output_array = [(out_data_val >> (i * data_width)) & ((1 << data_width) - 1) for i in range(array_size)]
    expected_output = sorted(input_array)

    cocotb.log.info(f"  Latency={cycle_count} Input={input_array} Output={output_array} Expected={expected_output}")

    assert output_array == expected_output, f"[{name}] Output incorrect. Got: {output_array}, Expected: {expected_output}"

    if (sort_flag) or (array_size == 1):
        expected_lat = calculate_latency(array_size, 1)
        cocotb.log.info(f"  Latency check: got={cycle_count}, expected={expected_lat}")
        assert expected_lat == cycle_count, f"[{name}] Latency incorrect. Got: {cycle_count}, Expected: {expected_lat}"

    cocotb.log.info(f"Test {name} passed.")

@cocotb.test()
async def test_bst_sorter(dut):
    ARRAY_SIZE = int(dut.ARRAY_SIZE.value)
    DATA_WIDTH = int(dut.DATA_WIDTH.value)

    clk_period = 10
    random.seed(0)

    cocotb.start_soon(clock(dut, clk_period))
    await reset_dut(dut, 5)
    dut.start.value = 0

    test_count = 3
    for idx in range(test_count):
        arr = [random.randint(0, (1 << DATA_WIDTH)-1) for _ in range(ARRAY_SIZE)]
        await run_test_case(f"Random {idx}", dut, arr, DATA_WIDTH, ARRAY_SIZE, 0)

    # Worst case descending
    arr = random.sample(range(1 << DATA_WIDTH), ARRAY_SIZE)
    await run_test_case("Worst case desc", dut, sorted(arr), DATA_WIDTH, ARRAY_SIZE, 1)

    # Worst case ascending
    arr = random.sample(range(1 << DATA_WIDTH), ARRAY_SIZE)
    await run_test_case("Worst case asc", dut, sorted(arr, reverse=True), DATA_WIDTH, ARRAY_SIZE, 1)

    # Balanced
    elements = sorted(random.sample(range(1 << DATA_WIDTH), ARRAY_SIZE))
    balanced_array = lambda nums: nums[len(nums)//2:len(nums)//2+1] + balanced_array(nums[:len(nums)//2]) + balanced_array(nums[len(nums)//2+1:]) if nums else []
    balanced_tree_array = balanced_array(elements)
    await run_test_case("Balanced Tree", dut, balanced_tree_array, DATA_WIDTH, ARRAY_SIZE, 0)

    # Min-max
    arr = [0 if i % 2 == 0 else (1 << DATA_WIDTH)-1 for i in range(ARRAY_SIZE)]
    await run_test_case("Min-Max", dut, arr, DATA_WIDTH, ARRAY_SIZE, 0)

    # Duplicates
    random_val = random.randint(0, (1 << DATA_WIDTH)-1)
    await run_test_case("Duplicates", dut, [random_val] * ARRAY_SIZE, DATA_WIDTH, ARRAY_SIZE, 1)
