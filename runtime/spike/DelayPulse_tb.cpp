#include "DelayPulse_thread.h"
#include <cstdio>

static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { \
    if (c) { ++pass; printf("PASS: " m "\n", ##__VA_ARGS__); } \
    else   { ++fail; printf("FAIL: " m "\n", ##__VA_ARGS__); } \
} while (0)

static DelayPulse dut;

// One full clock cycle: low → high (posedge fires the thread).
static void cycle() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval(); dut.posedge_clk();
}

int main() {
    // Reset sequence.
    dut.rst_n = 0; dut.start = 0; dut.eval();
    dut.posedge_clk();   // reset: thread (re)constructed.
    dut.rst_n = 1;

    // Cycle 0: thread runs first time, hits `wait until start`. Suspended.
    cycle();
    CHECK(dut.pulse == 0, "C0: pulse=0 before start");

    // Cycle 1: still no start.
    cycle();
    CHECK(dut.pulse == 0, "C1: still pulse=0");

    // Assert start.
    dut.start = 1;

    // Cycle 2: pred satisfied → resume → hits `wait 5 cycle`. Suspended.
    // Sets cycles_remaining = 5.
    cycle();
    CHECK(dut.pulse == 0, "C2: just hit wait-5-cycle, pulse still 0");

    // Cycles 3,4,5,6: cycles_remaining decrements 5→4→3→2→1.
    cycle(); CHECK(dut.pulse == 0, "C3: pulse=0 (cycles_remaining=4)");
    cycle(); CHECK(dut.pulse == 0, "C4: pulse=0 (cycles_remaining=3)");
    cycle(); CHECK(dut.pulse == 0, "C5: pulse=0 (cycles_remaining=2)");
    cycle(); CHECK(dut.pulse == 0, "C6: pulse=0 (cycles_remaining=1)");

    // Cycle 7: cycles_remaining hits 0 → resume → `pulse = 1` → hits
    // `wait 1 cycle`. Suspended.
    cycle();
    CHECK(dut.pulse == 1, "C7: pulse asserted after 5-cycle wait");

    // Cycle 8: 1-cycle wait elapses → resume → co_return → Done.
    // pulse falls back to 0 because the per-tick default zeroes it and
    // the coroutine no longer re-asserts (it's done).
    cycle();
    CHECK(dut.pulse == 0, "C8: pulse=0 after thread completes");

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
