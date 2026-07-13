#include "VAllReadyAndSeq.h"
#include <cassert>
#include <cstdio>

// Exercises the shared(and) seq-driven reduction (AllReadyAndSeq,
// tests/thread/shared_and_reduction_seq.arch): per-thread shadow wires
// default to the AND identity (1), fold with `&`, and register into
// all_ready on the clock edge.
int main() {
    VAllReadyAndSeq dut;
    auto tick = [&]() {
        dut.clk = 0;
        dut.eval();
        dut.clk = 1;
        dut.eval();
    };

    dut.rst_n = 0;
    dut.r0 = 0;
    dut.r1 = 0;
    for (int i = 0; i < 3; i++) tick();
    dut.rst_n = 1;
    tick();

    dut.r0 = 1;
    dut.r1 = 1;
    tick();
    tick();
    assert(dut.all_ready == 1);

    dut.r0 = 1;
    dut.r1 = 0;
    tick();
    tick();
    assert(dut.all_ready == 0);

    dut.r0 = 0;
    dut.r1 = 0;
    tick();
    tick();
    assert(dut.all_ready == 0);

    dut.r0 = 1;
    dut.r1 = 1;
    tick();
    tick();
    assert(dut.all_ready == 1);

    printf("PASS SharedAndReductionSeq\n");
    return 0;
}
