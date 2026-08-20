// arch#827 P4.1 loop-var regression: `.sext<N>()` on a receiver that
// references a live runtime `for`-loop iterator, behavioral check under
// Verilator. Same vectors/expectations as the Icarus TB
// (tb_sext_loop_var_hoist.sv).
//
// Verilator flattens packed multi-dimensional arrays to a single integer:
// `logic [3:0][7:0] din` is one 32-bit word, `logic [3:0][11:0] dout`
// (48 bits) is one 64-bit word; element i occupies bits [w*i +: w].
#include "VSextLoopVarHoist.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VSextLoopVarHoist* dut = new VSextLoopVarHoist;
    int fails = 0;

    auto check = [&](int32_t got, int32_t expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%d expect=%d\n", label, got, expect);
            fails++;
        }
    };
    auto elem12 = [](uint64_t word, int i) -> int32_t {
        uint32_t v = (uint32_t)((word >> (12 * i)) & 0xFFF);
        // sign-extend from 12 bits
        if (v & 0x800) v |= 0xFFFFF000u;
        return (int32_t)v;
    };

    uint32_t d0 = 5, d1 = (uint32_t)(uint8_t)-1, d2 = 0x80, d3 = 127;
    dut->din = d0 | (d1 << 8) | (d2 << 16) | (d3 << 24);
    dut->eval();

    check(elem12(dut->dout, 0), 5, "dout[0]");
    check(elem12(dut->dout, 1), -1, "dout[1]");
    check(elem12(dut->dout, 2), -128, "dout[2]");
    check(elem12(dut->dout, 3), 127, "dout[3]");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
