#include "VModuleRegLog.h"

#include <cstdio>

// Advance one rising edge with data_in applied before the edge (rst deasserted).
static void edge(VModuleRegLog& dut, unsigned data_in) {
    dut.clk = 0;
    dut.eval();
    dut.data_in = data_in;
    dut.rst = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    VModuleRegLog dut;
    dut.clk = 0;
    dut.rst = 1;
    dut.data_in = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval(); // sync reset edge -> acc == 0, and NO log (gated under !rst)
    if (dut.acc_out != 0) {
        std::printf("FAIL: reset acc_out=%u, expected 0\n", (unsigned)dut.acc_out);
        return 1;
    }

    // Four non-reset edges. The bare `log` fires on each; the nested `both` log
    // fires only when data_in[0] == 1 (0x11 and 0x33). Functional state (acc)
    // must track data_in regardless — the log fix changes observability, not
    // register values.
    edge(dut, 0x11);
    if (dut.acc_out != 0x11) {
        std::printf("FAIL: e1 acc_out=%u, expected 0x11\n", (unsigned)dut.acc_out);
        return 1;
    }
    edge(dut, 0x22);
    if (dut.acc_out != 0x22) {
        std::printf("FAIL: e2 acc_out=%u, expected 0x22\n", (unsigned)dut.acc_out);
        return 1;
    }
    edge(dut, 0x33);
    if (dut.acc_out != 0x33) {
        std::printf("FAIL: e3 acc_out=%u, expected 0x33\n", (unsigned)dut.acc_out);
        return 1;
    }
    edge(dut, 0x44);
    if (dut.acc_out != 0x44) {
        std::printf("FAIL: e4 acc_out=%u, expected 0x44\n", (unsigned)dut.acc_out);
        return 1;
    }

    std::printf("PASS module-reg-log behavior\n");
    return 0;
}
