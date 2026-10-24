// Shared behavioral testbench for arch#1058 non-power-of-two FIFO fixtures.
//
// DUT-agnostic: the same template runs against both the arch-sim-generated
// `V<Name>` class and the Verilator-generated `V<Name>` class, so a passing
// run under arch sim AND under Verilator proves the two agree.
//
// The design under test is a `fifo` with active-high `Reset<Sync>` and the
// standard push/pop handshake plus `full`/`empty` side-band ports.
//
// Three phases:
//   1. Fill   — push until `!push_ready`; capacity must equal DEPTH exactly.
//   2. Drain  — pop until `!pop_valid`; popped order must match push order.
//   3. Stream — keep <= DEPTH items in flight for >4*DEPTH items, so the
//               modular pointer wrap (at 2*DEPTH) is exercised, not just a
//               single 0..DEPTH fill.
//
// All loops are bounded so a broken DUT (e.g. a DEPTH=1 FIFO that never
// asserts `full`) fails loudly instead of hanging the simulator.
#pragma once
#include <cstdio>
#include <queue>
#include "verilated.h"

template <class DUT>
int run_fifo_cap_test(DUT* dut, int expected_depth) {
    int errors = 0;
    std::queue<int> ref;

    // --- Reset (active-high, Reset<Sync>) ---
    dut->rst = 1;
    dut->push_valid = 0;
    dut->pop_ready = 0;
    dut->push_data = 0;
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    dut->rst = 0;
    dut->clk = 0; dut->eval();
    if (!dut->empty || dut->full) {
        printf("FAIL: post-reset empty=%d full=%d (want empty=1 full=0)\n",
               (int)dut->empty, (int)dut->full);
        errors++;
    }

    const int GUARD = 4 * expected_depth + 64;

    // --- Phase 1: fill until full ---
    int val = 10;
    int cap = 0;
    int guard = 0;
    while (guard++ < GUARD) {
        dut->pop_ready = 0;
        dut->push_valid = 1;
        dut->push_data = val & 0xff;
        dut->clk = 0; dut->eval();        // comb settle; sample push_ready
        if (!dut->push_ready) break;      // full -> stop filling
        dut->clk = 1; dut->eval();        // commit push
        ref.push(val & 0xff);
        val++;
        cap++;
    }
    dut->push_valid = 0;
    dut->clk = 0; dut->eval();

    if (cap != expected_depth) {
        printf("FAIL: capacity %d, expected DEPTH=%d\n", cap, expected_depth);
        errors++;
    }
    if (!dut->full) {
        printf("FAIL: full not asserted after filling %d items\n", cap);
        errors++;
    }
    if (dut->push_ready) {
        printf("FAIL: push_ready still high while full\n");
        errors++;
    }

    // --- Phase 2: drain, checking FIFO order ---
    guard = 0;
    int popped = 0;
    while (guard++ < GUARD) {
        dut->push_valid = 0;
        dut->pop_ready = 1;
        dut->clk = 0; dut->eval();        // comb: pop_data = current head
        if (!dut->pop_valid) break;       // empty -> stop draining
        int got = (int)dut->pop_data;
        int exp = ref.front(); ref.pop();
        if (got != exp) {
            printf("FAIL: drain item %d: got %d expected %d\n", popped, got, exp);
            errors++;
        }
        dut->clk = 1; dut->eval();        // commit pop
        popped++;
    }
    dut->pop_ready = 0;
    dut->clk = 0; dut->eval();
    if (popped != cap) {
        printf("FAIL: drained %d but filled %d\n", popped, cap);
        errors++;
    }
    if (!dut->empty) {
        printf("FAIL: not empty after draining all items\n");
        errors++;
    }

    // --- Phase 3: streaming (<= DEPTH in flight), exercises pointer wrap ---
    while (!ref.empty()) ref.pop();
    const int STREAM_N = 4 * expected_depth + 5;
    const int SGUARD = 200 * (expected_depth + 2) + 200;
    int next_val = 100;
    int pushed_s = 0, popped_s = 0;
    guard = 0;
    while (popped_s < STREAM_N && guard++ < SGUARD) {
        bool want_push = (pushed_s < STREAM_N);
        dut->push_valid = want_push ? 1 : 0;
        dut->push_data = want_push ? (next_val & 0xff) : 0;
        dut->pop_ready = 1;
        dut->clk = 0; dut->eval();        // comb settle
        bool do_push = want_push && dut->push_ready;
        bool do_pop = dut->pop_valid;     // pop_ready asserted
        int got = do_pop ? (int)dut->pop_data : 0;
        int exp = do_pop ? ref.front() : 0;
        dut->clk = 1; dut->eval();        // commit
        if (do_push) { ref.push(next_val & 0xff); next_val++; pushed_s++; }
        if (do_pop) {
            if (got != exp) {
                printf("FAIL: stream pop %d: got %d expected %d\n", popped_s, got, exp);
                errors++;
            }
            ref.pop();
            popped_s++;
        }
    }
    if (popped_s != STREAM_N) {
        printf("FAIL: streamed %d of %d items (deadlock/hang?)\n", popped_s, STREAM_N);
        errors++;
    }

    dut->final();

    if (errors == 0) {
        printf("\nALL TESTS PASSED\n");
        return 0;
    }
    printf("\n%d TESTS FAILED\n", errors);
    return 1;
}
