// Preemption + reverse-handoff stress for lock hold-stability:
//  1. T1 (lower priority) acquires first (go1 only).
//  2. T0 (higher priority) starts requesting while T1 holds — with a
//     hold-stable arbiter T1 must keep the lock (busy1 stays high).
//  3. rel1 pulses: T1 releases; T0 (lower thread index than the
//     releaser) must acquire — same-cycle in the FSM path.
//  4. rel0 pulses: T0 completes.
#include "VMutexPreR.h"

#include <cstdio>

static VMutexPreR dut;
static int cycle_count = 0;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    cycle_count++;
    std::printf("[cyc %2d] go0=%u go1=%u rel0=%u rel1=%u | busy0=%u busy1=%u done0=%u done1=%u\n",
                cycle_count, (unsigned)dut.go0, (unsigned)dut.go1,
                (unsigned)dut.rel0, (unsigned)dut.rel1,
                (unsigned)dut.busy0, (unsigned)dut.busy1,
                (unsigned)dut.done0, (unsigned)dut.done1);
}

int main() {
    dut.rst = 1;
    dut.go0 = 0;
    dut.go1 = 0;
    dut.rel0 = 0;
    dut.rel1 = 0;
    tick();
    dut.rst = 0;
    tick();

    // T1 acquires alone.
    dut.go1 = 1;
    int guard = 0;
    while (!dut.busy1 && guard++ < 10) tick();
    if (guard >= 10) { std::puts("FAIL T1 never acquired"); return 1; }

    // T0 contends while T1 holds. T1 must keep the lock.
    dut.go0 = 1;
    for (int i = 0; i < 3; i++) {
        tick();
        if (!dut.busy1 || dut.busy0) {
            std::puts("FAIL hold violated: T0 preempted T1 mid-hold");
            return 1;
        }
    }

    // T1 releases; T0 must take over.
    dut.rel1 = 1;
    tick();
    dut.rel1 = 0;
    guard = 0;
    while (!dut.busy0 && guard++ < 10) tick();
    if (guard >= 10) { std::puts("FAIL T0 never acquired after release"); return 1; }

    dut.rel0 = 1;
    tick();
    dut.rel0 = 0;
    guard = 0;
    while (!(dut.done0 && dut.done1) && guard++ < 10) tick();
    if (guard >= 10) { std::puts("FAIL threads never completed"); return 1; }

    std::puts("PASS MutexPreR");
    return 0;
}
