#include "FlagSync.h"
#include <cstdio>

int main() {
    FlagSync dut;
    int errors = 0;

    // Initialize
    dut.src_clk = 0; dut.dst_clk = 0; dut.rst = 0; dut.data_in = 0;

    // Reset (async reset)
    dut.rst = 1;
    dut.dst_clk = 1; dut.eval();
    dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    // Verify output is 0 after reset
    if (dut.data_out != 0) { printf("FAIL: data_out not 0 after reset\n"); errors++; }

    // ── Test 1: propagation through 2-stage chain ──
    printf("=== Test 1: 2-stage propagation ===\n");
    dut.data_in = 1;

    // Cycle 1: data_in=1 enters stage0
    dut.dst_clk = 1; dut.eval();
    printf("After cycle 1: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }
    dut.dst_clk = 0; dut.eval();

    // Cycle 2: stage0 -> stage1 (output)
    dut.dst_clk = 1; dut.eval();
    printf("After cycle 2: data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }
    dut.dst_clk = 0; dut.eval();

    // ── Test 2: data changes ──
    printf("\n=== Test 2: data change ===\n");
    dut.data_in = 0;

    dut.dst_clk = 1; dut.eval();
    printf("After cycle 3: data_out=%d (expect 1, old value in stage1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }
    dut.dst_clk = 0; dut.eval();

    dut.dst_clk = 1; dut.eval();
    printf("After cycle 4: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }
    dut.dst_clk = 0; dut.eval();

    // ── Test 3: reset mid-operation ──
    printf("\n=== Test 3: mid-operation reset ===\n");
    dut.data_in = 1;
    dut.dst_clk = 1; dut.eval();
    dut.dst_clk = 0; dut.eval();
    // Now stage0=1, stage1=0. Assert reset.
    dut.rst = 1;
    dut.dst_clk = 1; dut.eval();
    printf("After reset: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }
    dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
