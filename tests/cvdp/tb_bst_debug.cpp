#include <cstdio>

// This is a Verilator C++ testbench
#include "Vbinary_search_tree_sort.h"
#include "verilated.h"

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto* dut = new Vbinary_search_tree_sort;

    // Reset
    dut->reset = 1;
    dut->start = 0;
    dut->data_in = 0;
    for (int i = 0; i < 10; i++) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    }
    dut->reset = 0;
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();

    // Test: [49, 53, 5, 33] with DATA_WIDTH=6, ARRAY_SIZE=4 (default is 8,32)
    // Actually, Verilator uses default params. Let's use default DATA_WIDTH=32, ARRAY_SIZE=8
    // Simple test: [3, 1, 4, 1, 5, 9, 2, 6]
    // Pack into data_in: index 0 = LSB
    uint64_t packed = 0;
    int arr[] = {3, 1, 4, 1, 5, 9, 2, 6};
    // data_in is 8*32=256 bits - too wide for uint64_t
    // Let's just test with a small subset
    // Actually we can't easily do this with default params
    // Let's just check basic FSM operation with all zeros
    dut->data_in[0] = 3;  // element 0
    dut->data_in[1] = 0;
    dut->data_in[2] = 0;
    dut->data_in[3] = 0;
    dut->data_in[4] = 0;
    dut->data_in[5] = 0;
    dut->data_in[6] = 0;
    dut->data_in[7] = 0;

    // Clock cycle
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    dut->start = 1;
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    dut->start = 0;

    int cycles = 0;
    while (cycles < 500) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
        cycles++;
        if (dut->done) {
            printf("Done after %d cycles\n", cycles);
            printf("sorted_out[0] = %u\n", dut->sorted_out[0]);
            break;
        }
    }
    if (cycles >= 500) printf("TIMEOUT\n");

    delete dut;
    return 0;
}
