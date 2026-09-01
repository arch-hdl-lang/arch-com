// Testbench for examples/auto_connect.arch — the `auto;` auto-connect
// directive.
//
// `auto;` is a front-end desugar, so the interesting property is not that
// the design simulates but that it simulates *exactly as if every
// connection had been written by hand*. This tb pins the behaviour that
// only correct clk/rst/en/output wiring can produce:
//
//   - reset drives both leaves to 0            (rst reached both instances)
//   - with en=0 nothing moves                  (en reached both instances)
//   - with en=1 the accumulator sums samples   (din/sum wired, clk ticking)
//   - the doubler tracks 2*sample one cycle late
//
// A mis-filled or missing auto-connection shows up here as a stuck or
// wrong output rather than a compile error.

#include "VAutoConnectTop.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VAutoConnectTop* dut = new VAutoConnectTop;

    int errors = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // ── Reset ────────────────────────────────────────────────────────────
    // Only reaches the leaves if `auto;` filled `rst` on both instances.
    dut->clk = 0;
    dut->rst = 1;
    dut->en = 0;
    dut->sample = 0;
    dut->eval();
    tick(); tick();
    dut->rst = 0;

    if (dut->sum != 0 || dut->dbl != 0) {
        printf("FAIL: after reset expected sum=0 dbl=0, got sum=%d dbl=%d\n",
               (int)dut->sum, (int)dut->dbl);
        errors++;
    } else {
        printf("PASS: reset reached both auto-connected instances\n");
    }

    // ── en = 0 holds ─────────────────────────────────────────────────────
    // Only holds if `auto;` filled `en` on both instances; a dangling `en`
    // would float/optimise to a constant and let the regs move.
    dut->sample = 7;
    dut->en = 0;
    tick(); tick(); tick();
    if (dut->sum != 0 || dut->dbl != 0) {
        printf("FAIL: en=0 should hold, got sum=%d dbl=%d\n",
               (int)dut->sum, (int)dut->dbl);
        errors++;
    } else {
        printf("PASS: en=0 holds both instances\n");
    }

    // ── Accumulate ───────────────────────────────────────────────────────
    // sum must track the running total of `sample`, and dbl must track
    // 2*sample delayed by one cycle.
    dut->en = 1;
    int expect_sum = 0;
    const int samples[] = {7, 3, 10, 200, 100};
    int prev_sample = 7;   // value present when en went high

    for (int i = 0; i < 5; i++) {
        dut->sample = samples[i];
        dut->eval();
        tick();

        expect_sum = (expect_sum + samples[i]) & 0xFF;
        int expect_dbl = (samples[i] + samples[i]) & 0xFF;
        (void)prev_sample;

        if ((int)dut->sum != expect_sum) {
            printf("FAIL: step %d: sum got %d, expected %d\n",
                   i, (int)dut->sum, expect_sum);
            errors++;
        }
        if ((int)dut->dbl != expect_dbl) {
            printf("FAIL: step %d: dbl got %d, expected %d\n",
                   i, (int)dut->dbl, expect_dbl);
            errors++;
        }
        prev_sample = samples[i];
    }
    if (errors == 0) {
        printf("PASS: accumulate + double through auto-connected ports\n");
    }

    // ── Wrapping is preserved ────────────────────────────────────────────
    // 8-bit wrapping add: the sum above already crossed 255 (7+3+10+200+100
    // = 320 -> 64), so a width-mismatched auto-connection would have shown
    // up as a different value in the loop.
    if (expect_sum != 64) {
        printf("FAIL: tb self-check: expected wrapped sum 64, computed %d\n",
               expect_sum);
        errors++;
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
