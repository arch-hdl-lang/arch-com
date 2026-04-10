#include "VBasicThread.h"
#include <cassert>
#include <cstdio>

int main() {
    VBasicThread dut;

    // Reset
    dut.rst_n = 0;
    dut.ar_ready = 0;
    dut.r_valid = 0;
    dut.r_data = 0;
    for (int i = 0; i < 3; i++) {
        dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    }
    dut.rst_n = 1;

    // Cycle 1: should be in S0 — ar_valid=1, ar_addr=100
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    assert(dut.ar_valid == 1);
    assert(dut.ar_addr == 100);

    // Provide ar_ready handshake
    dut.ar_ready = 1;
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    dut.ar_ready = 0;

    // Now in S1: r_ready=1
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    assert(dut.r_ready == 1);

    // Provide r_valid handshake
    dut.r_valid = 1;
    dut.r_data = 0xDEADBEEF;
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    dut.r_valid = 0;

    // S2 fires data_r <= r_data, then wraps to S0
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    assert(dut.data_out == 0xDEADBEEF);

    printf("PASS: VBasicThread sim test\n");
    return 0;
}
