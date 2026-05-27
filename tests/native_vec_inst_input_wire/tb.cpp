#include "VNativeVecInstInputWireProbe.h"

#include <cstdio>

static VNativeVecInstInputWireProbe dut;

static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); }

int main() {
    dut.rst = 0;
    dut.drive[0] = 0; dut.drive[1] = 0; dut.drive[2] = 0;
    for (int i = 0; i < 3; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 2; ++i) tick();

    // Lane 0: 3 pulses. Lane 1: 1 pulse. Lane 2: 0 pulses.
    for (int i = 0; i < 3; ++i) { dut.drive[0] = 1; tick(); dut.drive[0] = 0; tick(); }
    dut.drive[1] = 1; tick(); dut.drive[1] = 0;
    for (int i = 0; i < 3; ++i) tick();

    if (dut.count[0] != 3 || dut.count[1] != 1 || dut.count[2] != 0) {
        std::printf("FAIL: count = [%u, %u, %u], expected [3, 1, 0]\n",
                    (unsigned)dut.count[0], (unsigned)dut.count[1], (unsigned)dut.count[2]);
        return 1;
    }
    std::printf("PASS native Vec inst input wire\n");
    return 0;
}
