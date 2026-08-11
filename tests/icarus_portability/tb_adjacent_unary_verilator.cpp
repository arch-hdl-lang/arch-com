// arch#892 regression: adjacent prefix operators, behavioral check under
// Verilator. Same vectors/expectations as the Icarus TB
// (tb_adjacent_unary.sv).
//
// `y_negneg` is a signed 6-bit port, which Verilator exposes as a raw
// 6-bit CData — sign-extend it before comparing against a negative value.
#include "VAdjacentUnary.h"
#include "verilated.h"
#include <cstdio>

static int sext6(unsigned v) {
    v &= 0x3F;
    return (v & 0x20) ? static_cast<int>(v) - 64 : static_cast<int>(v);
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VAdjacentUnary* dut = new VAdjacentUnary;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%d expect=%d\n", label, got, expect);
            fails++;
        }
    };

    dut->a = 0xA; dut->p = 1; dut->q = 0; dut->eval();
    check(dut->y_notnot, 0xA, "notnot case1");
    check(sext6(dut->y_negneg), -6, "negneg case1");
    check(dut->y_lognot, 1, "lognot case1");
    check(dut->y_redand, 0, "redand case1");
    check(dut->y_redor, 1, "redor case1");
    check(dut->y_redxor, 0, "redxor case1");

    dut->a = 0x7; dut->p = 0; dut->q = 1; dut->eval();
    check(dut->y_notnot, 0x7, "notnot case2");
    check(sext6(dut->y_negneg), 7, "negneg case2");
    check(dut->y_lognot, 0, "lognot case2");
    check(dut->y_redand, 0, "redand case2");
    check(dut->y_redor, 1, "redor case2");
    check(dut->y_redxor, 1, "redxor case2");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
