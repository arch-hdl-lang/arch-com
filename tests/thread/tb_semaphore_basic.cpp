#include "VSemaphorePool.h"

#include <cstdio>

static VSemaphorePool dut;
static int cycle_count = 0;
static int max_concurrent = 0;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    cycle_count++;

    int busy = (int)dut.busy0 + (int)dut.busy1 + (int)dut.busy2 + (int)dut.busy3;
    if (busy > max_concurrent) max_concurrent = busy;
    if (busy > 2) {
        std::printf("FAIL over-subscribed cycle=%d busy=%d (want <= 2)\n", cycle_count, busy);
        std::exit(1);
    }
}

int main() {
    dut.rst = 1;
    dut.go = 0;
    dut.rel0 = dut.rel1 = dut.rel2 = dut.rel3 = 0;
    tick();
    dut.rst = 0;
    tick();

    // All 4 threads contend at once. N=2 admits exactly 2; the other 2
    // must wait (never oversubscribed, checked every cycle in tick()).
    dut.go = 1;
    tick();
    tick();
    if (max_concurrent != 2) {
        std::printf("FAIL expected exactly 2 concurrent holders once contended, got max=%d\n",
                    max_concurrent);
        return 1;
    }
    // Exactly one of {busy0,busy1,busy2,busy3} pairs is held; whichever two
    // are NOT held are still waiting (round_robin picks the lowest-index
    // pair on a tie, since round_robin's initial last_grant favors low
    // indices first — asserting the *set* is what matters here, not which
    // specific pair).
    int held_before = (int)dut.busy0 + (int)dut.busy1 + (int)dut.busy2 + (int)dut.busy3;
    if (held_before != 2) {
        std::printf("FAIL expected exactly 2 held at steady state, got %d\n", held_before);
        return 1;
    }

    // Release the currently-held threads one at a time; each release
    // should let a previously-waiting thread in without ever exceeding 2
    // concurrent holders, and every thread should eventually finish.
    for (int round = 0; round < 6; round++) {
        if (dut.busy0) dut.rel0 = 1; else dut.rel0 = 0;
        if (dut.busy1) dut.rel1 = 1; else dut.rel1 = 0;
        if (dut.busy2) dut.rel2 = 1; else dut.rel2 = 0;
        if (dut.busy3) dut.rel3 = 1; else dut.rel3 = 0;
        tick();
    }
    // Hold rel* high for stragglers to guarantee everyone finishes.
    dut.rel0 = dut.rel1 = dut.rel2 = dut.rel3 = 1;
    for (int i = 0; i < 10; i++) tick();

    if (!(dut.done0 && dut.done1 && dut.done2 && dut.done3)) {
        std::printf(
            "FAIL fairness: not all threads acquired the semaphore "
            "(done0=%d done1=%d done2=%d done3=%d)\n",
            (int)dut.done0, (int)dut.done1, (int)dut.done2, (int)dut.done3);
        return 1;
    }

    std::puts("PASS SemaphorePool");
    return 0;
}
