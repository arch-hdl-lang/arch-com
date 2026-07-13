#include "VAllReadyAnd.h"
#include <cassert>
#include <cstdio>

// Exercises the shared(and) comb-driven reduction (AllReadyAnd,
// tests/thread/shared_and_reduction.arch). Core regression: a thread
// that has not yet reached its drive point (T2, gated by `wait until
// go2`) must contribute the AND identity element (1) — not 0 — so it
// doesn't spuriously force the reduction low.
int main() {
    VAllReadyAnd dut;
    auto tick = [&]() {
        dut.clk = 0;
        dut.eval();
        dut.clk = 1;
        dut.eval();
    };

    // Reset
    dut.rst_n = 0;
    dut.r0 = 0;
    dut.r1 = 0;
    dut.go2 = 0;
    for (int i = 0; i < 3; i++) tick();
    dut.rst_n = 1;
    tick();

    // T2 stays idle (go2=0) the whole test. Both active drivers ready
    // and idle T2 must NOT force all_ready low.
    dut.r0 = 1;
    dut.r1 = 1;
    tick();
    tick();
    assert(dut.all_ready == 1 &&
           "idle thread should contribute AND identity (1), not force 0");

    // Any one deasserted -> 0.
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

    // Recovers to 1 once both are ready again.
    dut.r0 = 1;
    dut.r1 = 1;
    tick();
    tick();
    assert(dut.all_ready == 1);

    printf("PASS SharedAndReduction\n");
    return 0;
}
