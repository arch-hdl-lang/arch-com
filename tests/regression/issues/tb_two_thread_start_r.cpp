#include "VTwoThreadStartR.h"
#include <cstdio>
static VTwoThreadStartR dut;
static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); }
int main() {
    dut.rst = 0; dut.req = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 10; ++i) {
        std::printf("post-reset cyc=%d  count=%d\n", i, dut.count);
        tick();
    }
    return 0;
}
