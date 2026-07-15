// TB for semaphore_relock_rr.arch (arch#696 regression). Runs the tight
// re-lock loop for many cycles and asserts round_robin fairness: all three
// lanes make progress and none is starved. Under `--thread-sim both` the
// harness additionally cross-checks the FSM and coroutine backends.
#include "VSemaphoreRelock.h"
#include <cstdio>

static VSemaphoreRelock dut;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

int main() {
    dut.rst = 1; tick();
    dut.rst = 0;
    for (int i = 0; i < 300; i++) tick();
    unsigned c0 = dut.cnt0, c1 = dut.cnt1, c2 = dut.cnt2;
    std::printf("cnt0=%u cnt1=%u cnt2=%u\n", c0, c1, c2);
    if (c0 == 0 || c1 == 0 || c2 == 0) {
        std::printf("FAIL SemRelock: a lane was starved (want all > 0)\n");
        return 1;
    }
    unsigned lo = c0 < c1 ? (c0 < c2 ? c0 : c2) : (c1 < c2 ? c1 : c2);
    unsigned hi = c0 > c1 ? (c0 > c2 ? c0 : c2) : (c1 > c2 ? c1 : c2);
    if (hi > lo * 3) {
        std::printf("FAIL SemRelock: unfair rotation (hi=%u lo=%u)\n", hi, lo);
        return 1;
    }
    std::printf("PASS SemRelock\n");
    return 0;
}
