// arch#845 regression: pipeline BitSlice/PartSelect slice-base hoist,
// behavioral check under Verilator. Same vectors/expectations as the Icarus
// TB (tb_pipe_slice_hoist.sv) — see its header for the hand-derived table.
#include "VPipeSliceHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VPipeSliceHoist* dut = new VPipeSliceHoist;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%x expect=%x\n", label, got, expect);
            fails++;
        }
    };
    auto tick = [&]() {
        dut->clk = 0;
        dut->eval();
        dut->clk = 1;
        dut->eval();
    };

    dut->rst = 1;
    dut->a = 0;
    dut->b = 0;
    dut->w = 0;
    dut->sel = 0;
    tick();
    dut->rst = 0;

    dut->a = 0xA5; dut->b = 0xD; dut->w = 0xBEEF; dut->sel = 1;
    tick();
    check(dut->y_trunc_seq,  0xB, "trunc_seq case1");
    check(dut->y_zext_seq,   0x3, "zext_seq case1");
    check(dut->y_concat_seq, 0x5, "concat_seq case1");
    check(dut->y_repeat_seq, 0xD, "repeat_seq case1");
    check(dut->y_rev_seq,    0x9, "rev_seq case1");
    check(dut->y_trunc_let,  0xD, "trunc_let case1");
    check(dut->y_concat_let, 0x6, "concat_let case1");
    check(dut->y_rev_let,    0xA, "rev_let case1");
    check(dut->y_mux_comb,   0x7, "mux_comb case1");

    dut->a = 0xB2; dut->b = 0x6; dut->w = 0x1234; dut->sel = 0;
    tick();
    check(dut->y_trunc_seq,  0xD, "trunc_seq case2");
    check(dut->y_zext_seq,   0x1, "zext_seq case2");
    check(dut->y_concat_seq, 0x2, "concat_seq case2");
    check(dut->y_repeat_seq, 0x6, "repeat_seq case2");
    check(dut->y_rev_seq,    0x3, "rev_seq case2");
    check(dut->y_trunc_let,  0x6, "trunc_let case2");
    check(dut->y_concat_let, 0xA, "concat_let case2");
    check(dut->y_rev_let,    0x4, "rev_let case2");
    check(dut->y_mux_comb,   0x1, "mux_comb case2");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
