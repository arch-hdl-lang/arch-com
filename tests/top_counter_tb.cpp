#include "VTop.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VTop* top = new VTop;

    int errors = 0;

    // Initialize
    top->clk = 0;
    top->rst = 1;
    top->en = 0;

    // Helper: toggle clock
    auto tick = [&]() {
        top->clk = 0;
        top->eval();
        top->clk = 1;
        top->eval();
    };

    // Reset for 3 cycles
    for (int i = 0; i < 3; i++) {
        tick();
    }

    // Check count is 0 after reset
    if (top->count_out != 0) {
        printf("FAIL: after reset, count_out = %d, expected 0\n", top->count_out);
        errors++;
    } else {
        printf("PASS: after reset, count_out = 0\n");
    }

    // Deassert reset, enable counter
    top->rst = 0;
    top->en = 1;

    // Run 10 cycles
    for (int i = 1; i <= 10; i++) {
        tick();
        if (top->count_out != i) {
            printf("FAIL: cycle %d, count_out = %d, expected %d\n", i, top->count_out, i);
            errors++;
        }
    }
    printf("PASS: counter incremented 1..10 correctly\n");

    // Disable enable, count should hold
    top->en = 0;
    uint8_t held = top->count_out;
    for (int i = 0; i < 5; i++) {
        tick();
        if (top->count_out != held) {
            printf("FAIL: en=0, count changed from %d to %d\n", held, top->count_out);
            errors++;
        }
    }
    printf("PASS: counter holds at %d when en=0\n", held);

    // Re-enable, should resume from held value
    top->en = 1;
    for (int i = 1; i <= 5; i++) {
        tick();
        uint8_t expected = held + i;
        if (top->count_out != expected) {
            printf("FAIL: resume cycle %d, count_out = %d, expected %d\n", i, top->count_out, expected);
            errors++;
        }
    }
    printf("PASS: counter resumes correctly after re-enable\n");

    // Test reset mid-count
    top->rst = 1;
    tick();
    if (top->count_out != 0) {
        printf("FAIL: mid-count reset, count_out = %d, expected 0\n", top->count_out);
        errors++;
    } else {
        printf("PASS: mid-count reset works\n");
    }

    top->final();
    delete top;

    if (errors == 0) {
        printf("\nALL TESTS PASSED\n");
        return 0;
    } else {
        printf("\n%d TESTS FAILED\n", errors);
        return 1;
    }
}
