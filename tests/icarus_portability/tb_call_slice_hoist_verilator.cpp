// arch#810 regression: FunctionCall/MethodCall BitSlice/PartSelect base
// behavioral check under Verilator. Same vectors/expectations as the
// Icarus TB (tb_call_slice_hoist.sv).
#include "VCallSliceHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VCallSliceHoist* dut = new VCallSliceHoist;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%d expect=%d\n", label, got, expect);
            fails++;
        }
    };

    dut->a = 0xA5; dut->b = 0xD; dut->w = 0xBEEF; dut->eval();
    check(dut->y_func_bitslice, 0x9, "func_bitslice case1");
    check(dut->y_func_partselect, 0x9, "func_partselect case1");
    check(dut->y_trunc_bitslice, 0xB, "trunc_bitslice case1");
    check(dut->y_trunc_partselect, 0xB, "trunc_partselect case1");
    check(dut->y_zext_bitslice, 0x3, "zext_bitslice case1");
    check(dut->y_zext_partselect, 0x3, "zext_partselect case1");
    check(dut->y_reverse_bitslice, 0x9, "reverse_bitslice case1");
    check(dut->y_reverse_partselect, 0x9, "reverse_partselect case1");

    dut->a = 0xB2; dut->b = 0x6; dut->w = 0x1234; dut->eval();
    check(dut->y_func_bitslice, 0xC, "func_bitslice case2");
    check(dut->y_func_partselect, 0xC, "func_partselect case2");
    check(dut->y_trunc_bitslice, 0xD, "trunc_bitslice case2");
    check(dut->y_trunc_partselect, 0xD, "trunc_partselect case2");
    check(dut->y_zext_bitslice, 0x1, "zext_bitslice case2");
    check(dut->y_zext_partselect, 0x1, "zext_partselect case2");
    check(dut->y_reverse_bitslice, 0x3, "reverse_bitslice case2");
    check(dut->y_reverse_partselect, 0x3, "reverse_partselect case2");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
