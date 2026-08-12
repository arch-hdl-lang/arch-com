// arch#827 B1 regression: `.sext<N>()` on a runtime-indexed `Vec` element
// behavioral check under Verilator. Same vectors/expectations as the
// Icarus TB (tb_sext_index_hoist.sv).
#include "VSextIndexHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VSextIndexHoist* dut = new VSextIndexHoist;
    int fails = 0;

    auto check = [&](int got, int expect, const char* label) {
        if (got != expect) {
            printf("FAIL %s: got=%d expect=%d\n", label, got, expect);
            fails++;
        }
    };

    // din packed as [3:0][7:0] signed; Verilator flattens a small packed
    // array port like this into a single scalar data member sized to the
    // total bit width (32 bits here), laid out with element 0 in the low
    // byte — same order as the SV literal `din[0]`/`din[1]`/... indices.
    uint8_t d0 = 5, d1 = (uint8_t)-1, d2 = 0x80, d3 = 127;
    dut->din = (uint32_t)d0 | ((uint32_t)d1 << 8) | ((uint32_t)d2 << 16) | ((uint32_t)d3 << 24);

    dut->sel = 0; dut->eval();
    check((int16_t)(dut->y_runtime_idx << 4) >> 4, 5, "sel0 runtime");
    check((int16_t)(dut->y_const_idx << 4) >> 4, -128, "sel0 const");

    dut->sel = 1; dut->eval();
    check((int16_t)(dut->y_runtime_idx << 4) >> 4, -1, "sel1 runtime");

    dut->sel = 2; dut->eval();
    check((int16_t)(dut->y_runtime_idx << 4) >> 4, -128, "sel2 runtime");

    dut->sel = 3; dut->eval();
    check((int16_t)(dut->y_runtime_idx << 4) >> 4, 127, "sel3 runtime");
    check((int16_t)(dut->y_const_idx << 4) >> 4, -128, "sel3 const");

    delete dut;
    if (fails) { printf("FAILURES: %d\n", fails); return 1; }
    printf("PASS\n");
    return 0;
}
