#include "VMultiClockSync.h"
#include <cstdio>

int main() {
    VMultiClockSync dut;
    int errors = 0;

    // Reset (both clocks)
    dut.rst = 1;
    dut.fast_clk = 0; dut.slow_clk = 0; dut.data_in = 0;
    dut.eval();
    dut.fast_clk = 1; dut.slow_clk = 1; dut.eval();
    dut.fast_clk = 0; dut.slow_clk = 0; dut.eval();
    dut.rst = 0;

    // ── Test 1: 4:1 ratio (200MHz vs 50MHz) ──
    printf("=== Test 1: 4:1 ratio ===\n");
    uint8_t expected_fast = 0;
    uint8_t expected_slow = 0;

    for (int cycle = 0; cycle < 20; cycle++) {
        dut.data_in = (uint8_t)(cycle * 7);
        dut.fast_clk = 1;
        if (cycle % 4 == 0) {
            dut.slow_clk = 1;
            expected_slow = dut.data_in;
        }
        dut.eval();
        expected_fast++;

        int ok = (dut.fast_count == expected_fast && dut.data_out == expected_slow);
        printf("cycle=%2d  fast=%3d(exp=%3d)  slow=%3d(exp=%3d)  %s\n",
               cycle, dut.fast_count, expected_fast, dut.data_out, expected_slow,
               ok ? "OK" : "FAIL");
        if (!ok) errors++;

        dut.fast_clk = 0; dut.slow_clk = 0; dut.eval();
    }

    // ── Test 2: 3:2 ratio — simultaneous edges ──
    // Reset
    dut.rst = 1;
    dut.fast_clk = 1; dut.slow_clk = 1; dut.eval();
    dut.fast_clk = 0; dut.slow_clk = 0; dut.eval();
    dut.rst = 0;
    expected_fast = 0;
    expected_slow = 0;

    printf("\n=== Test 2: 3:2 ratio (simultaneous edges) ===\n");
    // Time steps: fast rises at t=0,2,4,6,8,10,...  slow rises at t=0,3,6,9,...
    // In terms of fast cycles: slow rises at fast cycles 0, 3 (approx), 6, 9...
    // Simpler: fast=every step, slow=every 1.5 steps → use time-based approach
    // At picosecond resolution: fast_period=5ns, slow_period=7.5ns (not clean ratio)
    // Use GCD approach: fast rises at multiples of 2, slow at multiples of 3
    for (int t = 0; t < 30; t++) {
        int fast_rise = (t % 2 == 0);
        int slow_rise = (t % 3 == 0);

        if (!fast_rise && !slow_rise) continue;  // no edge this step

        dut.data_in = (uint8_t)(t * 3);
        dut.fast_clk = fast_rise ? 1 : 0;
        dut.slow_clk = slow_rise ? 1 : 0;
        dut.eval();

        if (fast_rise) expected_fast++;
        if (slow_rise) expected_slow = dut.data_in;

        int ok = (dut.fast_count == expected_fast && dut.data_out == expected_slow);
        printf("t=%2d  fast_edge=%d slow_edge=%d  fast=%3d(exp=%3d)  slow=%3d(exp=%3d)  %s\n",
               t, fast_rise, slow_rise, dut.fast_count, expected_fast,
               dut.data_out, expected_slow, ok ? "OK" : "FAIL");
        if (!ok) errors++;

        // Falling edges
        dut.fast_clk = 0; dut.slow_clk = 0; dut.eval();
    }

    // ── Test 3: Only slow_clk toggles (fast stays low) ──
    printf("\n=== Test 3: slow_clk only ===\n");
    dut.rst = 1;
    dut.fast_clk = 1; dut.slow_clk = 1; dut.eval();
    dut.fast_clk = 0; dut.slow_clk = 0; dut.eval();
    dut.rst = 0;
    expected_fast = 0;
    expected_slow = 0;

    for (int i = 0; i < 5; i++) {
        dut.data_in = (uint8_t)(i + 100);
        dut.slow_clk = 1;
        dut.eval();
        expected_slow = dut.data_in;

        int ok = (dut.fast_count == expected_fast && dut.data_out == expected_slow);
        printf("i=%d  fast=%3d(exp=%3d)  slow=%3d(exp=%3d)  %s\n",
               i, dut.fast_count, expected_fast, dut.data_out, expected_slow,
               ok ? "OK" : "FAIL");
        if (!ok) errors++;

        dut.slow_clk = 0; dut.eval();
    }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
