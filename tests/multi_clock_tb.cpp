#include "VMultiClockSync.h"
#include <cstdio>

int main() {
    VMultiClockSync dut;

    // Reset
    dut.rst = 1;
    dut.fast_clk = 0; dut.slow_clk = 0; dut.data_in = 0;
    dut.eval();
    dut.fast_clk = 1; dut.eval();
    dut.fast_clk = 0; dut.eval();
    dut.rst = 0;

    // Run: fast_clk toggles 4x per slow_clk toggle (200MHz vs 50MHz)
    int errors = 0;
    uint8_t expected_fast = 0;
    uint8_t expected_slow = 0;

    for (int cycle = 0; cycle < 20; cycle++) {
        dut.data_in = (uint8_t)(cycle * 7);  // arbitrary data

        // Toggle fast_clk rising
        dut.fast_clk = 1;
        // Toggle slow_clk every 4th fast cycle
        if (cycle % 4 == 0) {
            dut.slow_clk = 1;
            expected_slow = dut.data_in;
        }
        dut.eval();

        expected_fast++;

        printf("cycle=%2d  fast_count=%3d (exp=%3d)  data_out=%3d (exp=%3d)  %s\n",
               cycle, dut.fast_count, expected_fast, dut.data_out, expected_slow,
               (dut.fast_count == expected_fast && dut.data_out == expected_slow) ? "OK" : "FAIL");

        if (dut.fast_count != expected_fast || dut.data_out != expected_slow) errors++;

        // Falling edges
        dut.fast_clk = 0;
        dut.slow_clk = 0;
        dut.eval();
    }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
