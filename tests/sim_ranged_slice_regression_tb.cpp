// arch#847 regression: byte-masked write through runtime-bound ranged
// part-selects (`mem[addr][(i*8+7):(i*8)] <= din[...]` under
// `if wem[i:i]`) must update exactly the masked byte lanes.
#include "Vsim_ranged_slice_regression.h"
#include <cstdio>
static Vsim_ranged_slice_regression dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

int main() {
    dut.clk = 0; dut.rst_n = 1;
    dut.cs = 0; dut.we = 0; dut.addr = 0; dut.wem = 0; dut.din = 0;
    dut.eval();

    // Full-mask write of a distinctive pattern to addr 5.
    dut.cs = 1; dut.we = 1; dut.addr = 5; dut.wem = 0xF;
    dut.din = 0x11223344u;
    tick();

    // Byte-masked write: only lanes 0 and 3.
    dut.wem = 0x9; dut.din = 0xAABBCC99u;
    tick();

    // Read back addr 5: lane 3 = 0xAA, lanes 2..1 unchanged, lane 0 = 0x99.
    dut.we = 0; dut.wem = 0; tick();
    CHECK(dut.dout == 0xAA223399u,
          "masked lanes 0+3 written, 1+2 preserved (got 0x%08x)", (unsigned)dut.dout);

    // wem=0 write must change nothing.
    dut.we = 1; dut.wem = 0x0; dut.din = 0xFFFFFFFFu; tick();
    dut.we = 0; tick();
    CHECK(dut.dout == 0xAA223399u,
          "wem=0 write is a no-op (got 0x%08x)", (unsigned)dut.dout);

    // Single-lane write to a different address; addr 5 stays intact.
    dut.we = 1; dut.addr = 2; dut.wem = 0x4; dut.din = 0x00DD0000u; tick();
    dut.we = 0; tick();
    CHECK(dut.dout == 0x00DD0000u,
          "lane-2-only write to fresh word (got 0x%08x)", (unsigned)dut.dout);
    dut.addr = 5; tick();
    CHECK(dut.dout == 0xAA223399u,
          "addr 5 untouched by write to addr 2 (got 0x%08x)", (unsigned)dut.dout);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
