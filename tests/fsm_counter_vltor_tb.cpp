// Verilator cross-check testbench for FsmCounter
#include "VFsmCounter.h"
#include "verilated.h"
#include <cstdio>

static int fail_count = 0;
static VFsmCounter* dut;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { printf("  FAIL: " fmt "\n", ##__VA_ARGS__); fail_count++; } \
} while(0)

static void tick() { dut->clk = 0; dut->eval(); dut->clk = 1; dut->eval(); }

static void reset() {
    dut->rst = 1; dut->go = 0; dut->target = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst = 0; tick();
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    dut = new VFsmCounter();
    reset();

    printf("=== FsmCounter Verilator cross-check ===\n");

    printf("Test 1: count to 3\n");
    dut->go = 1; dut->target = 3; tick();
    dut->go = 0;
    for (int i = 0; i < 20 && !dut->done; i++) tick();
    CHECK(dut->done, "expected done");
    CHECK(dut->count == 4, "expected count=4, got %d", dut->count);
    tick();

    printf("Test 2: count to 1\n");
    dut->go = 1; dut->target = 1; tick();
    dut->go = 0;
    for (int i = 0; i < 20 && !dut->done; i++) tick();
    CHECK(dut->done, "expected done");
    CHECK(dut->count == 2, "expected count=2, got %d", dut->count);

    if (fail_count == 0) printf("\nAll Verilator tests PASSED\n");
    else printf("\n%d test(s) FAILED\n", fail_count);

    delete dut;
    return fail_count ? 1 : 0;
}
