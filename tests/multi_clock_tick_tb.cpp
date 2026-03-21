#include "VMultiClockSync.h"
#include <cstdio>

int main() {
    VMultiClockSync dut;
    int errors = 0;

    // Reset: hold rst high for a few ticks
    dut.rst = 1;
    for (int i = 0; i < 16; i++) dut.tick();
    dut.rst = 0;

    // Track expected values
    uint8_t prev_fast_clk = dut.fast_clk;
    uint8_t prev_slow_clk = dut.slow_clk;
    uint8_t expected_fast = 0;
    uint8_t expected_slow = 0;
    int fast_edges = 0;
    int slow_edges = 0;

    printf("Running 80 ticks (200MHz fast, 50MHz slow)...\n\n");

    for (int t = 0; t < 80; t++) {
        dut.data_in = (uint8_t)((t * 13) & 0xFF);  // varying data

        uint8_t old_fast = dut.fast_clk;
        uint8_t old_slow = dut.slow_clk;
        dut.tick();

        // Detect rising edges
        bool fast_rose = (dut.fast_clk == 0 && old_fast == 0) ? false :
                         (old_fast == 0 && dut.fast_clk == 1);
        bool slow_rose = (old_slow == 0 && dut.slow_clk == 1);

        // Actually, after tick(), clk has been toggled and eval() called.
        // Rising edge = was 0, now 1. But we set data_in BEFORE tick.
        // The eval happens with the new clock values.

        if (fast_rose) {
            expected_fast++;
            fast_edges++;
        }
        if (slow_rose) {
            // slow_r latches data_in on slow_clk rising
            expected_slow = dut.data_in;
            slow_edges++;
        }

        if (fast_rose || slow_rose) {
            int ok = (dut.fast_count == expected_fast && dut.data_out == expected_slow);
            printf("t=%2d time=%6lu ps  fast_edge=%d slow_edge=%d  fast=%3d(exp=%3d)  slow=%3d(exp=%3d)  %s\n",
                   t, (unsigned long)dut.time_ps, fast_rose?1:0, slow_rose?1:0,
                   dut.fast_count, expected_fast, dut.data_out, expected_slow,
                   ok ? "OK" : "FAIL");
            if (!ok) errors++;
        }
    }

    printf("\nfast rising edges: %d  slow rising edges: %d\n", fast_edges, slow_edges);
    printf("Expected ratio ~4:1, actual: %.1f:1\n", (double)fast_edges / slow_edges);
    // 200MHz/50MHz = 4:1 — fast should have ~4x more edges
    if (fast_edges < slow_edges * 3 || fast_edges > slow_edges * 5) {
        printf("FAIL: frequency ratio out of range\n");
        errors++;
    }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
