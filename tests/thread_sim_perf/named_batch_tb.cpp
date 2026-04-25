#include "VNamedThreadDemo.h"
#include <cstdio>
#include <chrono>
static VNamedThreadDemo dut;
int main(int argc, char** argv) {
    int n_cycles = (argc > 1) ? atoi(argv[1]) : 1000000;
    dut.clk=0; dut.eval(); dut.clk=1; dut.eval();
    dut.rst_n=0;
    dut.clk=0; dut.eval(); dut.clk=1; dut.eval();
    dut.rst_n=1;
    dut.wr_ack=1; dut.rd_ack=1;
    auto t0 = std::chrono::high_resolution_clock::now();
    dut.run_cycles((uint64_t)n_cycles);
    auto t1 = std::chrono::high_resolution_clock::now();
    double ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
    printf("=== %d cycles in %.2f ms = %.2f Mcyc/s (BATCH) ===\n",
        n_cycles, ms, n_cycles / (ms / 1000.0) / 1e6);
    return 0;
}
