#include "RstSync.h"
#include <cstdio>

int main() {
    RstSync dut;
    int errors = 0;

    // Initialize: clocks low, reset deasserted
    dut.src_clk = 0; dut.dst_clk = 0; dut.data_in = 0;
    dut.eval();

    // ── Test 1: async assert ──
    // When data_in goes high, data_out should go high immediately
    // (no clock edge needed)
    printf("=== Test 1: async assert ===\n");
    dut.data_in = 1;
    dut.eval();  // no clock edge, just combinational
    printf("  data_in=1, data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }

    // ── Test 2: sync deassert ──
    // When data_in goes low, data_out should stay high until
    // 2 dst_clk rising edges propagate the 0 through the chain
    printf("\n=== Test 2: sync deassert (2-stage) ===\n");
    dut.data_in = 0;
    dut.eval();  // no clock yet — output should still be 1
    printf("  After deassert, no clk: data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }

    // Cycle 1: stage0=0, stage1 still=1 → output=1
    dut.dst_clk = 1; dut.eval();
    printf("  After 1 dst_clk: data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }
    dut.dst_clk = 0; dut.eval();

    // Cycle 2: stage0=0, stage1=0 → output=0
    dut.dst_clk = 1; dut.eval();
    printf("  After 2 dst_clk: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }
    dut.dst_clk = 0; dut.eval();

    // ── Test 3: re-assert mid-deassert ──
    // Assert again, then deassert and verify clean sync deassert
    printf("\n=== Test 3: re-assert then deassert ===\n");
    dut.data_in = 1;
    dut.eval();
    if (dut.data_out != 1) { printf("  FAIL: assert didn't take\n"); errors++; }

    // One dst_clk while asserted — output stays 1
    dut.dst_clk = 1; dut.eval();
    dut.dst_clk = 0; dut.eval();
    if (dut.data_out != 1) { printf("  FAIL: output dropped during assert\n"); errors++; }

    // Deassert and clock twice
    dut.data_in = 0;
    dut.dst_clk = 1; dut.eval(); dut.dst_clk = 0; dut.eval();
    printf("  1 clk after deassert: data_out=%d (expect 1)\n", dut.data_out);
    if (dut.data_out != 1) { printf("  FAIL\n"); errors++; }

    dut.dst_clk = 1; dut.eval(); dut.dst_clk = 0; dut.eval();
    printf("  2 clk after deassert: data_out=%d (expect 0)\n", dut.data_out);
    if (dut.data_out != 0) { printf("  FAIL\n"); errors++; }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
