// arch#807 regression: Concat/Repeat BitSlice/PartSelect base behavioral
// check under Verilator. Same vectors/expectations as the Icarus TB
// (tb_concat_repeat_slice_hoist.sv).
#include "VConcatRepeatSliceHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VConcatRepeatSliceHoist* dut = new VConcatRepeatSliceHoist;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%d expect=%d\n", label, got, expect);
            fails++;
        }
    };

    dut->a = 0x3; dut->c = 0xA; dut->eval();
    check(dut->y_concat_bitslice, 0xE, "concat_bitslice case1");
    check(dut->y_concat_partselect, 0xE, "concat_partselect case1");

    dut->a = 0xF; dut->c = 0x0; dut->eval();
    check(dut->y_concat_bitslice, 0xC, "concat_bitslice case2");
    check(dut->y_concat_partselect, 0xC, "concat_partselect case2");

    dut->a = 0x3; dut->eval();
    check(dut->y_repeat_bitslice, 0xC, "repeat_bitslice case1");
    check(dut->y_repeat_partselect, 0xC, "repeat_partselect case1");

    dut->a = 0x9; dut->eval();
    check(dut->y_repeat_bitslice, 0x6, "repeat_bitslice case2");
    check(dut->y_repeat_partselect, 0x6, "repeat_partselect case2");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
