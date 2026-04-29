#include "VWrapCounter.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VWrapCounter* dut = new VWrapCounter;

    int errors = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // Reset; set the runtime-programmable wrap boundary to 15 (4-bit max).
    dut->clk = 0;
    dut->rst = 1;
    dut->inc = 0;
    dut->max = 15;
    dut->eval();
    tick(); tick();
    dut->rst = 0;

    // Count from 0 to MAX (15)
    for (int i = 0; i < 16; i++) {
        uint8_t got = dut->value;
        if (got != (uint8_t)i) {
            printf("FAIL: value at step %d: got %d, expected %d\n", i, (int)got, i);
            errors++;
        }
        uint8_t at_max = dut->at_max;
        if (i == 15 && !at_max) {
            printf("FAIL: at_max should be 1 when value=15\n");
            errors++;
        }
        if (i < 15 && at_max) {
            printf("FAIL: at_max should be 0 when value=%d\n", i);
            errors++;
        }
        dut->inc = 1;
        tick();
    }

    // Should have wrapped back to 0
    {
        uint8_t got = dut->value;
        if (got != 0) {
            printf("FAIL: wrap: expected 0, got %d\n", (int)got);
            errors++;
        } else {
            printf("PASS: wrap at MAX=15\n");
        }
    }

    // Test inc=0 holds value
    dut->inc = 0;
    tick(); tick();
    {
        uint8_t got = dut->value;
        if (got != 0) {
            printf("FAIL: hold: expected 0, got %d\n", (int)got);
            errors++;
        } else {
            printf("PASS: hold when inc=0\n");
        }
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
