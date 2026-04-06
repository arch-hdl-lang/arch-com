// Testbench for FsmCounter — FSM with datapath registers
#include "VFsmCounter.h"
#include <cstdio>
#include <cstdint>

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

int main() {
    dut = new VFsmCounter();
    reset();

    printf("=== FsmCounter testbench ===\n");

    // Test 1: count to 3
    printf("Test 1: count to 3\n");
    dut->go = 1; dut->target = 3; tick();
    dut->go = 0;
    // Should take some cycles to count 0->1->2->3 then done
    for (int i = 0; i < 20 && !dut->done; i++) tick();
    CHECK(dut->done, "expected done");
    CHECK(dut->count == 4, "expected count=4, got %d", dut->count);

    // After done, it goes back to Idle on next cycle
    tick();
    CHECK(!dut->done, "should be back to idle");

    // Test 2: count to 1
    printf("Test 2: count to 1\n");
    dut->go = 1; dut->target = 1; tick();
    dut->go = 0;
    for (int i = 0; i < 20 && !dut->done; i++) tick();
    CHECK(dut->done, "expected done");
    CHECK(dut->count == 2, "expected count=2, got %d", dut->count);
    tick();

    // Test 3: count to 0 (should be done immediately after 1 count cycle? cnt starts at 0, tgt=0, so cnt==tgt in Counting)
    printf("Test 3: count to 0\n");
    dut->go = 1; dut->target = 0; tick();
    dut->go = 0;
    for (int i = 0; i < 20 && !dut->done; i++) tick();
    CHECK(dut->done, "expected done");

    if (fail_count == 0) printf("\nAll tests PASSED\n");
    else printf("\n%d test(s) FAILED\n", fail_count);

    delete dut;
    return fail_count ? 1 : 0;
}
