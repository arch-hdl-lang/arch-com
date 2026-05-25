#include "VProbe.h"
#include <cstdio>
static VProbe dut;
static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); }
static void write_cell(unsigned o, unsigned i, unsigned v) {
    dut.we = 1; dut.idx_outer = o; dut.idx_inner = i; dut.wdata = v;
    tick();
    dut.we = 0;
}
static unsigned read_cell(unsigned o, unsigned i) {
    dut.idx_outer = o; dut.idx_inner = i;
    dut.eval();
    return (unsigned)dut.out;
}
int main() {
    dut.rst = 0; dut.we = 0; dut.idx_outer = 0; dut.idx_inner = 0; dut.wdata = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    // Distinct values across all 8*4 = 32 cells confirm the inner dim
    // isn't being silently aliased to bit-extraction.
    for (unsigned o = 0; o < 8; ++o)
        for (unsigned i = 0; i < 4; ++i)
            write_cell(o, i, 0xCAFE0000u | (o << 4) | i);
    // Read every cell back; any aliasing would scramble values.
    for (unsigned o = 0; o < 8; ++o) {
        for (unsigned i = 0; i < 4; ++i) {
            unsigned got = read_cell(o, i);
            unsigned want = 0xCAFE0000u | (o << 4) | i;
            if (got != want) {
                std::printf("FAIL rf[%u][%u] = 0x%X (expected 0x%X)\n", o, i, got, want);
                return 1;
            }
        }
    }
    std::printf("PASS nested-Vec storage: 32 distinct cells round-trip\n");
    return 0;
}
