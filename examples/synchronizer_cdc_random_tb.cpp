#include "FlagSync.h"
#include <cstdio>

// Test that data eventually propagates even with random latency.
// With --cdc-random, the synchronizer may take STAGES or more cycles.
// cdc_skip_pct controls the probability (0-100) of +1 cycle per edge.

int main() {
    FlagSync dut;
    int errors = 0;

    // Testbench controls skip probability: 50% for aggressive stress testing
    dut.cdc_skip_pct = 50;

    // Reset
    dut.src_clk = 0; dut.dst_clk = 0; dut.rst = 0; dut.data_in = 0;
    dut.rst = 1;
    dut.dst_clk = 1; dut.eval();
    dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    // ── Test: data_in=1 must appear at data_out within STAGES+8 cycles ──
    printf("=== CDC random test: eventual propagation (cdc_skip_pct=50) ===\n");
    dut.data_in = 1;

    int appeared = 0;
    for (int c = 0; c < 10; c++) {
        dut.dst_clk = 1; dut.eval();
        dut.dst_clk = 0; dut.eval();
        if (dut.data_out == 1) {
            printf("  data_out=1 appeared at cycle %d (ok, STAGES=2)\n", c + 1);
            appeared = 1;
            break;
        }
    }
    if (!appeared) {
        printf("  FAIL: data_out never became 1 within 10 cycles\n");
        errors++;
    }

    // ── Test: data_in=0 must propagate back ──
    dut.data_in = 0;
    appeared = 0;
    for (int c = 0; c < 10; c++) {
        dut.dst_clk = 1; dut.eval();
        dut.dst_clk = 0; dut.eval();
        if (dut.data_out == 0) {
            printf("  data_out=0 appeared at cycle %d (ok)\n", c + 1);
            appeared = 1;
            break;
        }
    }
    if (!appeared) {
        printf("  FAIL: data_out never returned to 0 within 10 cycles\n");
        errors++;
    }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
