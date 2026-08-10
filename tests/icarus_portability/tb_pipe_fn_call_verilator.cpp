// arch#852 regression: a top-level `function` called from inside a
// `pipeline`, behavioral check under Verilator. Same vectors/expectations as
// the Icarus TB (tb_pipe_fn_call.sv) — see its header for the hand-derived
// table.
#include "VPipeFnCall.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VPipeFnCall* dut = new VPipeFnCall;
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
    dut->sel = 0;
    tick();
    dut->rst = 0;

    dut->a = 0xA5; dut->sel = 1;
    tick();
    check(dut->y_ident_seq, 0xA5, "ident_seq case1");
    check(dut->y_add_seq,   0xA8, "add_seq case1");
    check(dut->y_slice_seq, 0x9,  "slice_seq case1");
    check(dut->y_add_let,   0xA8, "add_let case1");
    check(dut->y_slice_let, 0x4,  "slice_let case1");
    check(dut->y_mux_comb,  0xA,  "mux_comb case1");

    dut->a = 0xB2; dut->sel = 0;
    tick();
    check(dut->y_ident_seq, 0xB2, "ident_seq case2");
    check(dut->y_add_seq,   0xB5, "add_seq case2");
    check(dut->y_slice_seq, 0xC,  "slice_seq case2");
    check(dut->y_add_let,   0xB5, "add_let case2");
    check(dut->y_slice_let, 0x6,  "slice_let case2");
    check(dut->y_mux_comb,  0x5,  "mux_comb case2");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
