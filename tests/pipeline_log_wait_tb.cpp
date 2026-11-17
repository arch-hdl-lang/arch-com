#include "VPipeLogWait.h"

#include <cstdio>

// Advance one rising edge with the given inputs applied before the edge.
static void edge(VPipeLogWait& dut, unsigned mem_valid, unsigned mem_data) {
    dut.clk = 0;
    dut.eval();
    dut.rst = 0;
    dut.mem_valid = mem_valid;
    dut.mem_data = mem_data;
    dut.addr_in = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    VPipeLogWait dut;
    dut.clk = 0;
    dut.rst = 1;
    dut.mem_valid = 0;
    dut.mem_data = 0;
    dut.addr_in = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval(); // sync reset

    // mem_valid held high: DataAccess advances every cycle, so mem_data
    // propagates through data -> result -> data_out. Clock enough edges for it
    // to settle, then confirm the value reached the output.
    for (int i = 0; i < 6; i++) {
        edge(dut, 1, 0xAB);
    }
    if (dut.data_out != 0xAB) {
        std::printf("FAIL: data_out=%u expected 171\n", dut.data_out);
        return 1;
    }

    std::printf("PASS pipeline-log-wait behavior\n");
    return 0;
}
