#include "VPipelineMatchComb.h"

#include <cstdio>

static bool check_value(const char* name, unsigned actual, unsigned expected) {
    if (actual == expected) {
        return true;
    }
    std::printf("FAIL: %s = %u, expected %u\n", name, actual, expected);
    return false;
}

static void edge(VPipelineMatchComb& dut) {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    VPipelineMatchComb dut;
    dut.clk = 0;
    dut.rst = 1;
    dut.sel = 0;
    dut.a = 0;
    dut.b = 0;
    dut.eval();
    edge(dut); // reset: stage registers -> 0
    dut.rst = 0;

    // Latch inputs, then read the combinational match output one cycle later.
    dut.sel = 0;
    dut.a = 0xAA;
    dut.b = 0xBB;
    edge(dut); // sel_r == 0 -> y = a_r
    if (!check_value("sel0", dut.y, 0xAA)) {
        return 1;
    }

    dut.sel = 1;
    edge(dut); // sel_r == 1 -> y = b_r
    if (!check_value("sel1", dut.y, 0xBB)) {
        return 1;
    }

    dut.sel = 2;
    edge(dut); // sel_r == 2 -> default arm, y = a_r ^ b_r
    if (!check_value("sel2_default", dut.y, 0xAA ^ 0xBB)) {
        return 1;
    }

    std::printf("PASS pipeline match-in-comb behavior\n");
    return 0;
}
