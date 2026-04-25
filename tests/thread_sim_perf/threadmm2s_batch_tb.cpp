// Phase 3.4 + cycle batching: ThreadMm2s perf with run_cycles(K).
#include "VThreadMm2s.h"
#include <cstdio>
#include <chrono>

static VThreadMm2s dut;

int main(int argc, char** argv) {
    int n_cycles = (argc > 1) ? atoi(argv[1]) : 1000000;

    // Reset (per-cycle eval)
    dut.clk=0; dut.eval(); dut.clk=1; dut.eval();
    dut.rst=1;
    dut.clk=0; dut.eval(); dut.clk=1; dut.eval();
    dut.rst=0;
    // Setup inputs once; held for entire batch
    dut.start = 1;
    dut.total_xfers = 16;
    dut.base_addr = 0x1000;
    dut.burst_len = 4;
    dut.ar_ready = 1;
    dut.r_valid = 1;
    dut.r_id = 0;
    dut.r_last = 0;
    dut.push_ready = 1;

    auto t0 = std::chrono::high_resolution_clock::now();
    // Cycle-batch run: workers do all K ticks before returning.
    dut.run_cycles((uint64_t)n_cycles);
    auto t1 = std::chrono::high_resolution_clock::now();
    double ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
    double cps = n_cycles / (ms / 1000.0);
    printf("=== %d cycles in %.2f ms = %.2f Mcyc/s (BATCH) ===\n",
        n_cycles, ms, cps / 1e6);
    return 0;
}
