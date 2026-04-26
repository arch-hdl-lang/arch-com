// Testbench for tests/if_wait_for_in_then.arch
//
// Verifies that the for-loop-in-then-branch correctly iterates burst_len
// times, asserting `done` once per iteration. Before the bugfix, the
// for-loop body executed exactly once and control jumped to the post-if
// wait, so `done` would only pulse once.

#include <cstdio>
#include <cstdint>
#include <cstdlib>
#include "VM.h"
#include "verilated.h"

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto* dut = new VM;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // Reset (active-low, async)
    dut->rst = 0;
    dut->clk = 0;
    dut->go = 0;
    dut->doit = 0;
    dut->ack = 0;
    dut->burst = 4;     // expect 4 done pulses
    dut->eval();
    tick(); tick();
    dut->rst = 1;
    tick();

    // Trigger: go + doit (take the then-branch with the for-loop)
    dut->go = 1;
    dut->doit = 1;
    tick();
    dut->go = 0;

    // Now the thread should be in the for-loop. Each iteration waits for
    // ack, then asserts done for one cycle. Count `done` pulses.
    //
    // burst_len = 4, so the loop runs for i in 0..3 (4 iterations) and
    // expect 4 `done` pulses across the run.
    constexpr int EXPECTED_DONE_PULSES = 4;
    int done_pulses = 0;
    int prev_done = 0;
    int max_cycles = 100;
    int cycles_since_last_done = 0;

    for (int c = 0; c < max_cycles; c++) {
        // Pulse ack every couple of cycles to drive the wait_until.
        dut->ack = (c % 3 == 0) ? 1 : 0;
        tick();
        if (dut->done && !prev_done) {
            done_pulses++;
            cycles_since_last_done = 0;
        } else {
            cycles_since_last_done++;
        }
        prev_done = dut->done;
        // If we've seen the expected pulses and waited ~10 idle cycles,
        // the post-if wait_cycles has had time to fire — break out.
        if (done_pulses >= EXPECTED_DONE_PULSES && cycles_since_last_done > 10) break;
    }

    printf("done_pulses=%d (expected %d)\n", done_pulses, EXPECTED_DONE_PULSES);
    int rc = (done_pulses == EXPECTED_DONE_PULSES) ? 0 : 1;
    if (rc != 0) {
        printf("FAIL: for-loop did not iterate the expected number of times\n");
    } else {
        printf("PASS: for-loop iterated %d times correctly\n", done_pulses);
    }

    delete dut;
    return rc;
}
