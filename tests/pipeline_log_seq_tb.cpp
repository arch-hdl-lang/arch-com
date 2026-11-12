#include "VPipeLogSeq.h"

#include <cstdio>

// Advance one rising edge with `data_in` applied before the edge.
static void edge(VPipeLogSeq& dut, unsigned data_in) {
    dut.clk = 0;
    dut.eval();
    dut.data_in = data_in;
    dut.rst = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    VPipeLogSeq dut;
    dut.clk = 0;
    dut.rst = 1;
    dut.data_in = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval(); // sync reset edge -> acc == 0, and NO log line during reset
    if (dut.data_out != 0) {
        std::printf("FAIL: reset data_out=%u\n", dut.data_out);
        return 1;
    }

    // acc pre-edge value is logged, then acc <= data_in commits.
    edge(dut, 0x11); // odd: logs "acc=0 in=17" + "odd acc=0"
    if (dut.data_out != 0x11) {
        std::printf("FAIL: e0 data_out=%u\n", dut.data_out);
        return 1;
    }
    edge(dut, 0x22); // even: logs "acc=17 in=34" only
    if (dut.data_out != 0x22) {
        std::printf("FAIL: e1 data_out=%u\n", dut.data_out);
        return 1;
    }
    edge(dut, 0x33); // odd: logs "acc=34 in=51" + "odd acc=34"
    if (dut.data_out != 0x33) {
        std::printf("FAIL: e2 data_out=%u\n", dut.data_out);
        return 1;
    }

    std::printf("PASS pipeline-log-seq behavior\n");
    return 0;
}
