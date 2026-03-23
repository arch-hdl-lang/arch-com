#include "VClkDivCounter.h"
#include <cstdio>

static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

static void half_tick(VClkDivCounter &d, int val) {
    d.clk = val;
    d.eval();
}

int main() {
    VClkDivCounter d;
    d.clk = 0; d.rst_n = 0;

    // Reset: 3 full cycles
    for (int i = 0; i < 3; i++) {
        half_tick(d, 0);
        half_tick(d, 1);
    }
    d.rst_n = 1;
    // One more cycle to exit reset cleanly
    half_tick(d, 0);
    half_tick(d, 1);

    printf("After reset: count=%d, div_clk=%d\n", d.count, d.div_clk);
    CHECK(d.count == 0, "count=0 after reset");

    // Now clock the design. The divider toggles on every rising clk edge.
    // So clk_slow rises every OTHER rising clk edge.
    // The counter should increment on each clk_slow rising edge.
    //
    // Expected timeline:
    //   fast clk rising #1: toggle_r 0->1, clk_slow rises => counter 0->1
    //   fast clk rising #2: toggle_r 1->0, clk_slow falls => counter holds
    //   fast clk rising #3: toggle_r 0->1, clk_slow rises => counter 1->2
    //   fast clk rising #4: toggle_r 1->0, clk_slow falls => counter holds

    int expected_count = 0;
    for (int cycle = 1; cycle <= 8; cycle++) {
        half_tick(d, 0);
        half_tick(d, 1);

        // On odd cycles the divider toggles high (clk_slow rises)
        // so counter should increment
        if (cycle % 2 == 1) {
            expected_count++;
        }

        char msg[128];
        snprintf(msg, sizeof(msg),
                 "cycle %d: count=%d expected=%d div_clk=%d",
                 cycle, d.count, expected_count, d.div_clk);
        printf("  %s\n", msg);

        char check_msg[128];
        snprintf(check_msg, sizeof(check_msg),
                 "cycle %d: count == %d", cycle, expected_count);
        CHECK(d.count == expected_count, check_msg);
    }

    printf("\n=== ClkDivCounter: %d/%d passed ===\n",
           test_count - fail_count, test_count);
    return fail_count ? 1 : 0;
}
