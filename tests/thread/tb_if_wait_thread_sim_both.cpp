#include "VIfWaitThreadSimBoth.h"

#include <cstdio>

static VIfWaitThreadSimBoth dut;
static int cycle_count = 0;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    cycle_count++;
}

static bool expect_phase(unsigned expected, const char* label) {
    if (dut.phase != expected) {
        std::printf("FAIL %s cycle=%d phase=%u expected=%u\n",
                    label, cycle_count, (unsigned)dut.phase, expected);
        return false;
    }
    return true;
}

int main() {
    dut.rst = 1;
    dut.req = 0;
    dut.is_mul = 0;
    tick();
    dut.rst = 0;
    tick();

    dut.req = 1;
    dut.is_mul = 1;
    tick();
    if (!expect_phase(1, "mul dispatch")) return 1;
    dut.req = 0;
    tick();
    if (!expect_phase(2, "mul wait")) return 1;
    tick();
    if (!expect_phase(2, "mul rejoin")) return 1;

    dut.req = 1;
    dut.is_mul = 0;
    tick();
    if (!expect_phase(3, "div dispatch")) return 1;
    dut.req = 0;
    tick();
    if (!expect_phase(4, "div wait")) return 1;
    tick();
    if (!expect_phase(4, "div rejoin")) return 1;

    std::puts("PASS IfWaitThreadSimBoth");
    return 0;
}
