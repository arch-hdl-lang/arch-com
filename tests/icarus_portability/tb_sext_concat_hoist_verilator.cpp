// arch#827 B2 regression: `.sext<N>()` on a `Concat` receiver behavioral
// check under Verilator. Same vectors/expectations as the Icarus TB
// (tb_sext_concat_hoist.sv).
#include "VSextConcatHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VSextConcatHoist* dut = new VSextConcatHoist;
    int fails = 0;

    auto check = [&](int32_t got, int32_t expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%d expect=%d\n", label, got, expect);
            fails++;
        }
    };

    dut->a = 0x00; dut->b = 0x01; dut->eval();
    check(dut->y, 1, "case1");

    dut->a = 0x3F; dut->b = 0x3F; dut->eval();
    check(dut->y, -1, "case2");

    dut->a = 0x20; dut->b = 0x00; dut->eval();
    check(dut->y, -2048, "case3");

    dut->a = 0x1F; dut->b = 0x3F; dut->eval();
    check(dut->y, 2047, "case4");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
