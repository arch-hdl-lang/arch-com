// Phase 3.4 perf benchmark — ThreadMm2s (5 user threads).
#include "VThreadMm2s.h"
#include <cstdio>
#include <chrono>

static VThreadMm2s dut;
static void cycle() { dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }

int main(int argc, char** argv) {
    int n_cycles = (argc > 1) ? atoi(argv[1]) : 1000000;
    dut.rst=1; cycle(); dut.rst=0;
    // Always-ack pattern — keeps all 5 threads busy.
    dut.start = 1;
    dut.total_xfers = 16;
    dut.base_addr = 0x1000;
    dut.burst_len = 4;
    auto t0 = std::chrono::high_resolution_clock::now();
    for (int i = 0; i < n_cycles; i++) {
        dut.ar_ready = 1;
        dut.r_valid = (i & 1);
        dut.r_id = (i >> 1) & 3;
        dut.r_last = ((i & 7) == 7);
        dut.push_ready = 1;
        cycle();
    }
    auto t1 = std::chrono::high_resolution_clock::now();
    double ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
    double cps = n_cycles / (ms / 1000.0);
    printf("=== %d cycles in %.2f ms = %.2f Mcyc/s ===\n",
        n_cycles, ms, cps / 1e6);
    return 0;
}
