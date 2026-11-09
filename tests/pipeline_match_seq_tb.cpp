#include "VPipelineMatchSeq.h"

#include <cstdio>

static bool check_value(const char* name, unsigned actual, unsigned expected) {
    if (actual == expected) {
        return true;
    }
    std::printf("FAIL: %s = %u, expected %u\n", name, actual, expected);
    return false;
}

// Advance one rising edge with the given select / data applied before the edge.
static void edge(VPipelineMatchSeq& dut, unsigned sel, unsigned data_in) {
    dut.clk = 0;
    dut.eval();
    dut.sel = sel;
    dut.data_in = data_in;
    dut.rst = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    VPipelineMatchSeq dut;
    dut.clk = 0;
    dut.rst = 1;
    dut.sel = 0;
    dut.data_in = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval(); // sync reset edge -> acc == 0
    if (!check_value("reset", dut.data_out, 0)) {
        return 1;
    }

    edge(dut, 0, 0x55); // arm 0: acc <= data_in
    if (!check_value("sel0", dut.data_out, 0x55)) {
        return 1;
    }

    edge(dut, 1, 0x10); // arm 1: acc <= data_in + 1
    if (!check_value("sel1", dut.data_out, 0x11)) {
        return 1;
    }

    edge(dut, 2, 0x00); // default arm: acc <= 99
    if (!check_value("sel2_default", dut.data_out, 99)) {
        return 1;
    }

    edge(dut, 3, 0x00); // default arm again
    if (!check_value("sel3_default", dut.data_out, 99)) {
        return 1;
    }

    std::printf("PASS pipeline match-in-seq behavior\n");
    return 0;
}
