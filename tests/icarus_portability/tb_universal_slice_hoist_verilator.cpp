// arch#813 P1 regression: behavioral check under Verilator for the base
// kinds the retired allowlist used to refuse. Same vectors/expectations as
// the Icarus TB (tb_universal_slice_hoist.sv).
#include "VUniversalSliceHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VUniversalSliceHoist* dut = new VUniversalSliceHoist;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=0x%x expect=0x%x\n", label, got, expect);
            fails++;
        }
    };

    dut->a = 0xA5; dut->b = 0x30; dut->s = 2; dut->c = 1; dut->eval();
    check(dut->y_arith, 0x5, "arith case1");
    check(dut->y_chain, 0x2, "chain case1");
    check(dut->y_shift, 0x2, "shift case1");
    check(dut->y_tern,  0x5, "tern case1");
    check(dut->y_lit,   0x3, "lit case1");

    dut->a = 0x3C; dut->b = 0x0F; dut->s = 0; dut->c = 0; dut->eval();
    check(dut->y_arith, 0xD, "arith case2");
    check(dut->y_chain, 0x3, "chain case2");
    check(dut->y_shift, 0xE, "shift case2");
    check(dut->y_tern,  0xF, "tern case2");
    check(dut->y_lit,   0x3, "lit case2");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
