#include "EventSync.h"
#include <cstdio>

int main() {
    EventSync dut;
    int errors = 0;

    // Initialize
    dut.src_clk = 0; dut.dst_clk = 0; dut.rst = 0; dut.data_in = 0;

    // Reset
    dut.rst = 1;
    dut.src_clk = 1; dut.dst_clk = 1; dut.eval();
    dut.src_clk = 0; dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    // Helper: clock one src rising edge
    auto src_tick = [&]() {
        dut.src_clk = 1; dut.eval();
        dut.src_clk = 0; dut.eval();
    };
    // Helper: clock one dst rising edge
    auto dst_tick = [&]() {
        dut.dst_clk = 1; dut.eval();
        dut.dst_clk = 0; dut.eval();
    };

    // ── Test 1: single pulse transfer ──
    printf("=== Test 1: single pulse ===\n");

    // Send a 1-cycle pulse on src_clk
    dut.data_in = 1;
    src_tick();
    dut.data_in = 0;
    src_tick();  // ensure toggle is captured

    // Output should be 0 still (toggle hasn't propagated)
    printf("  Before dst clocks: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }

    // Clock dst to propagate toggle through 2-stage chain
    dst_tick();  // sync0 = toggle
    printf("  After 1 dst_clk: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }

    dst_tick();  // sync1 = sync0 (toggle arrived), edge detected
    printf("  After 2 dst_clk: data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }

    dst_tick();  // next cycle: no more edge → pulse gone
    printf("  After 3 dst_clk: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }

    // ── Test 2: two pulses ──
    printf("\n=== Test 2: two separate pulses ===\n");

    // First pulse
    dut.data_in = 1; src_tick(); dut.data_in = 0; src_tick();
    dst_tick(); dst_tick();
    printf("  Pulse 1: data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }
    dst_tick();  // clear
    if (dut.data_out != 0) { printf("  FAIL: pulse didn't clear\n"); errors++; }

    // Second pulse
    dut.data_in = 1; src_tick(); dut.data_in = 0; src_tick();
    dst_tick(); dst_tick();
    printf("  Pulse 2: data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }
    dst_tick();
    if (dut.data_out != 0) { printf("  FAIL: pulse didn't clear\n"); errors++; }

    // ── Test 3: no pulse → no output ──
    printf("\n=== Test 3: no input → no output ===\n");
    for (int i = 0; i < 5; i++) {
        src_tick(); dst_tick();
        if (dut.data_out != 0) {
            printf("  FAIL: spurious output at step %d\n", i);
            errors++;
        }
    }
    printf("  No spurious pulses: OK\n");

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
