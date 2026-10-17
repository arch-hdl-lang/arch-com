// arch#1031 facet 2 regression TB.
#include "VSimEnumLetType.h"
#include <cstdio>
static VSimEnumLetType dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)
int main() {
    dut.sel = 1; dut.eval();
    CHECK(dut.out_run == 1, "sel=1 => m=Run => out_run=1 (got %u)", (unsigned)dut.out_run);
    dut.sel = 0; dut.eval();
    CHECK(dut.out_run == 0, "sel=0 => m=Idle => out_run=0 (got %u)", (unsigned)dut.out_run);
    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
