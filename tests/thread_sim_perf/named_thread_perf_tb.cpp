// Phase 3.4 perf benchmark — measure cycles/sec for named_thread at
// --threads 1 vs --threads N.
#include "VNamedThreadDemo.h"
#include <cstdio>
#include <chrono>

static VNamedThreadDemo dut;

static void cycle() { dut.clk=0; dut.eval(); dut.clk=1; dut.eval(); }

int main(int argc, char** argv) {
    int n_cycles = (argc > 1) ? atoi(argv[1]) : 1000000;

    dut.rst_n=0; cycle();
    dut.rst_n=1;
    // Always-ack handshake: ack on every other cycle, so threads
    // alternate (assert-ack-loop). Real work per cycle.
    auto t0 = std::chrono::high_resolution_clock::now();
    for (int i = 0; i < n_cycles; i++) {
        dut.wr_ack = (i & 1);
        dut.rd_ack = (i & 1);
        cycle();
    }
    auto t1 = std::chrono::high_resolution_clock::now();
    double ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
    double cps = n_cycles / (ms / 1000.0);
    printf("=== %d cycles in %.2f ms = %.2f cycles/sec (%.2f Mcycles/sec) ===\n",
        n_cycles, ms, cps, cps / 1e6);
    return 0;
}
