// arch#709 regression TB: a lone mutex requester must make progress every cycle
// after reset; registering the release event must not insert a reacquisition
// bubble when no contender is present.
#include "VMutexRelockSingle709.h"
#include <cstdio>

static VMutexRelockSingle709 dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    dut.rst = 1;
    tick();
    dut.rst = 0;

    unsigned previous = dut.count;
    unsigned increments = 0;
    for (int cycle = 0; cycle < 20; ++cycle) {
        tick();
        unsigned current = dut.count;
        if (current != previous + 1) {
            std::printf(
                "FAIL MutexRelockSingle709: bubble at cycle %d (previous=%u current=%u)\n",
                cycle, previous, current);
            return 1;
        }
        previous = current;
        ++increments;
    }

    std::printf("increments=%u final_count=%u\n", increments, previous);
    std::printf("PASS MutexRelockSingle709\n");
    return 0;
}
