#include "VMutexRel.h"

#include <cstdio>

static VMutexRel dut;
static int cycle_count = 0;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    cycle_count++;
    std::printf("[cyc %2d] go=%u rel0=%u rel1=%u | busy0=%u busy1=%u done0=%u done1=%u\n",
                cycle_count, (unsigned)dut.go, (unsigned)dut.rel0, (unsigned)dut.rel1,
                (unsigned)dut.busy0, (unsigned)dut.busy1,
                (unsigned)dut.done0, (unsigned)dut.done1);
}

int main() {
    dut.rst = 1;
    dut.go = 0;
    dut.rel0 = 0;
    dut.rel1 = 0;
    tick();
    dut.rst = 0;
    tick();

    dut.go = 1;
    // Run until first holder's busy is visible, then pulse its release.
    int guard = 0;
    while (!dut.busy0 && !dut.busy1 && guard++ < 10) tick();
    if (guard >= 10) { std::puts("FAIL no thread ever acquired"); return 1; }

    bool t0_first = dut.busy0;
    if (t0_first) dut.rel0 = 1; else dut.rel1 = 1;
    tick();
    if (t0_first) dut.rel0 = 0; else dut.rel1 = 0;

    // Wait for the second thread to acquire, then pulse its release.
    guard = 0;
    while (!(t0_first ? dut.busy1 : dut.busy0) && guard++ < 10) tick();
    if (guard >= 10) { std::puts("FAIL second thread never acquired"); return 1; }

    if (t0_first) dut.rel1 = 1; else dut.rel0 = 1;
    tick();
    if (t0_first) dut.rel1 = 0; else dut.rel0 = 0;

    guard = 0;
    while (!(dut.done0 && dut.done1) && guard++ < 10) tick();
    if (guard >= 10) { std::puts("FAIL threads never completed"); return 1; }

    std::puts("PASS MutexRel");
    return 0;
}
