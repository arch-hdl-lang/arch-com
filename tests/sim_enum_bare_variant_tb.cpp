// arch#1031 facet 1 regression TB.
#include "VSimEnumBareVariant.h"
#include <cstdio>
static VSimEnumBareVariant dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)
int main() {
    // Sync reset: mode -> Idle, out_run == 0.
    dut.rst = 1; dut.go = 0; dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.out_run == 0, "after reset: Idle => out_run=0 (got %u)", (unsigned)dut.out_run);
    // Release reset, assert go, one posedge: mode -> Run, out_run == 1.
    dut.rst = 0; dut.go = 1; dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.out_run == 1, "after go posedge: Run => out_run=1 (got %u)", (unsigned)dut.out_run);
    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
