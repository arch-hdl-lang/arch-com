#include "VTrafficLight.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VTrafficLight* dut = new VTrafficLight;

    int errors = 0;

    auto tick = [&](int timer_val) {
        dut->clk   = 0;
        dut->timer = timer_val;
        dut->eval();
        dut->clk = 1;
        dut->eval();
    };

    // Reset
    dut->clk = 0; dut->rst = 1; dut->timer = 1;
    dut->eval();
    for (int i = 0; i < 2; i++) tick(1);

    // After reset: should be in Red (red=1, yellow=0, green=0)
    if (!dut->red || dut->yellow || dut->green) {
        printf("FAIL: after reset not in Red state (red=%d yellow=%d green=%d)\n",
               dut->red, dut->yellow, dut->green);
        errors++;
    } else {
        printf("PASS: reset → Red state\n");
    }

    dut->rst = 0;

    // Keep timer=1 (non-zero): Red should hold
    for (int i = 0; i < 3; i++) tick(1);
    if (!dut->red || dut->yellow || dut->green) {
        printf("FAIL: Red did not hold with timer!=0\n");
        errors++;
    } else {
        printf("PASS: Red holds while timer != 0\n");
    }

    // timer=0: Red → Green
    tick(0);
    tick(1); // one more cycle for state to latch
    if (dut->red || dut->yellow || !dut->green) {
        printf("FAIL: Red did not transition to Green (red=%d yellow=%d green=%d)\n",
               dut->red, dut->yellow, dut->green);
        errors++;
    } else {
        printf("PASS: Red → Green\n");
    }

    // Green holds
    for (int i = 0; i < 3; i++) tick(1);
    if (dut->red || dut->yellow || !dut->green) {
        printf("FAIL: Green did not hold\n");
        errors++;
    } else {
        printf("PASS: Green holds\n");
    }

    // Green → Yellow
    tick(0);
    tick(1);
    if (dut->red || !dut->yellow || dut->green) {
        printf("FAIL: Green did not transition to Yellow (red=%d yellow=%d green=%d)\n",
               dut->red, dut->yellow, dut->green);
        errors++;
    } else {
        printf("PASS: Green → Yellow\n");
    }

    // Yellow → Red
    tick(0);
    tick(1);
    if (!dut->red || dut->yellow || dut->green) {
        printf("FAIL: Yellow did not transition back to Red (red=%d yellow=%d green=%d)\n",
               dut->red, dut->yellow, dut->green);
        errors++;
    } else {
        printf("PASS: Yellow → Red\n");
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
