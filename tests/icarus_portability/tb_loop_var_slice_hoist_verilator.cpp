// arch#861 regression: hoisted bases referencing a live `for`-loop
// iterator, behavioral check under Verilator. Same vectors/expectations as
// the Icarus TB (tb_loop_var_slice_hoist.sv).
//
// Verilator flattens packed multi-dimensional arrays to a single integer,
// so `logic [3:0][7:0] v` is one 32-bit word and `logic [3:0][3:0] y` is
// one 16-bit word; element i occupies bits [w*i +: w].
//
// The `y_index` expectations are the ones that matter most: before the fix
// the emitted SV dropped the parens around the hoisted base and Verilator
// compiled the wrong expression *silently*, yielding 0x2 / 0x8 here.
#include "VLoopVarSliceHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VLoopVarSliceHoist* dut = new VLoopVarSliceHoist;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=0x%x expect=0x%x\n", label, got, expect);
            fails++;
        }
    };
    auto nib = [](unsigned word, int i) { return (word >> (4 * i)) & 0xF; };
    auto pack_v = [](unsigned v0, unsigned v1, unsigned v2, unsigned v3) {
        return v0 | (v1 << 8) | (v2 << 16) | (v3 << 24);
    };

    dut->a = 0x08; dut->v = pack_v(0x00, 0x08, 0x01, 0xF0); dut->eval();
    check(dut->y_index, 0xD, "index case1");
    check(nib(dut->y_concat_bitslice, 0), 0x0, "concat_bitslice[0] case1");
    check(nib(dut->y_concat_bitslice, 1), 0x2, "concat_bitslice[1] case1");
    check(nib(dut->y_concat_bitslice, 2), 0x0, "concat_bitslice[2] case1");
    check(nib(dut->y_concat_bitslice, 3), 0xC, "concat_bitslice[3] case1");
    check(nib(dut->y_concat_partselect, 1), 0x2, "concat_partselect[1] case1");
    check(nib(dut->y_concat_partselect, 3), 0xC, "concat_partselect[3] case1");
    check(nib(dut->y_method_bitslice, 1), 0x8, "method_bitslice[1] case1");
    check(nib(dut->y_method_bitslice, 3), 0x0, "method_bitslice[3] case1");

    dut->a = 0x10; dut->v = pack_v(0xFF, 0x00, 0x88, 0x00); dut->eval();
    check(dut->y_index, 0x5, "index case2");
    check(nib(dut->y_concat_bitslice, 0), 0xF, "concat_bitslice[0] case2");
    check(nib(dut->y_concat_bitslice, 2), 0x2, "concat_bitslice[2] case2");
    check(nib(dut->y_concat_partselect, 0), 0xF, "concat_partselect[0] case2");
    check(nib(dut->y_concat_partselect, 2), 0x2, "concat_partselect[2] case2");
    check(nib(dut->y_method_bitslice, 0), 0xF, "method_bitslice[0] case2");
    check(nib(dut->y_method_bitslice, 2), 0x8, "method_bitslice[2] case2");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
