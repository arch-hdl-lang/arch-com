// arch#846 regression: hoist temps synthesized inside a procedural scope
// must still compute the right values once their declaration + continuous
// assign are relocated to module scope (and, for a `function` body, once
// the assignment becomes blocking). Verilator half; same vectors and
// expectations as the Icarus TB (tb_seq_slice_hoist.sv).
#include "VSeqSliceHoist.h"
#include "verilated.h"
#include <cstdio>

static void tick(VSeqSliceHoist* dut) {
    dut->clk = 0;
    dut->eval();
    dut->clk = 1;
    dut->eval();
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VSeqSliceHoist* dut = new VSeqSliceHoist;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%x expect=%x\n", label, got, expect);
            fails++;
        }
    };

    dut->rst = 1;
    dut->a = 0xA5;
    dut->w = 0xBEEF;
    dut->sel = 1;
    tick(dut);

    dut->rst = 0;
    tick(dut);
    check(dut->y_seq_concat, 0xB, "seq_concat case1");
    check(dut->y_seq_trunc, 0xE, "seq_trunc case1");
    check(dut->y_seq_idx, 0x1, "seq_idx case1");
    check(dut->y_comb_concat, 0x6, "comb_concat case1");
    check(dut->y_fn_concat, 0x6, "fn_concat case1");

    dut->a = 0xB2;
    dut->w = 0x1234;
    dut->sel = 1;
    tick(dut);
    check(dut->y_seq_concat, 0xD, "seq_concat case2");
    check(dut->y_seq_trunc, 0x3, "seq_trunc case2");
    check(dut->y_seq_idx, 0x0, "seq_idx case2");
    check(dut->y_comb_concat, 0xA, "comb_concat case2");
    check(dut->y_fn_concat, 0xA, "fn_concat case2");

    dut->sel = 0;
    dut->eval();
    check(dut->y_comb_concat, 0x0, "comb_concat sel=0");
    check(dut->y_fn_concat, 0xA, "fn_concat sel=0");

    delete dut;
    if (fails) {
        printf("FAILURES: %d\n", fails);
        return 1;
    }
    printf("PASS\n");
    return 0;
}
