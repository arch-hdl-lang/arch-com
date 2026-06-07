#include "VNativeWaitUntilFoldTargetProbe.h"

#include <cstdio>

static void tick(VNativeWaitUntilFoldTargetProbe& dut) {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    dut.clk = 0;
    dut.eval();
}

int main() {
    VNativeWaitUntilFoldTargetProbe dut;

    dut.rst = 1;
    dut.go = 0;
    tick(dut);

    dut.rst = 0;
    tick(dut);

    if (dut.phase != 0) {
        std::printf("FAIL: reset/release phase=%u, expected 0\n", (unsigned)dut.phase);
        return 1;
    }

    // The wait-until exit assignment is folded into the go-detection arm, so
    // phase must update on this tick. The folded transition target must skip
    // the absorbed action state and enter the following wait-2-cycle state.
    dut.go = 1;
    tick(dut);
    dut.go = 0;

    if (dut.phase != 1) {
        std::printf("FAIL: phase after go=%u, expected folded assignment value 1\n",
                    (unsigned)dut.phase);
        return 1;
    }

    // If the folded wait state incorrectly targets the absorbed action state,
    // native sim gets stuck because that state has no emitted body. Reaching
    // phase=2 proves the target advanced into the counted-wait path instead.
    bool reached_phase_2 = false;
    for (int i = 0; i < 8; ++i) {
        tick(dut);
        if (dut.phase == 2) {
            reached_phase_2 = true;
            break;
        }
    }

    if (!reached_phase_2) {
        std::printf("FAIL: phase never reached 2 after folded wait target; final phase=%u\n",
                    (unsigned)dut.phase);
        return 1;
    }

    std::printf("PASS native wait-until folded target\n");
    return 0;
}
