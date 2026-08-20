// arch#868 regression TB: runtime-bound width-80 slice of a 160-bit
// (VlWide<5>) base. The 160-bit input cannot be driven through the
// scalar assignment path, so load the backing words directly.
#include "VWideVlRuntimeSlice.h"
#include <cstdio>
static VWideVlRuntimeSlice dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

int main() {
    uint32_t* vw = dut.v.data();
    vw[0] = 0x11111111u; vw[1] = 0x22222222u; vw[2] = 0x33333333u;
    vw[3] = 0x44444444u; vw[4] = 0x55555555u;

    // i = 16 (b0 != 0): o = v[95:16], width 80.
    dut.i = 16; dut.eval();
    const uint32_t* d = dut.o.data();
    CHECK(d[0] == 0x22221111u && d[1] == 0x33332222u && (d[2] & 0xffff) == 0x3333u,
          "i=16: v[95:16] = 0x%04x_%08x%08x", d[2] & 0xffff, d[1], d[0]);

    // i = 64 (word-aligned lo, b0 == 0): o = v[143:64], width 80.
    dut.i = 64; dut.eval();
    d = dut.o.data();
    CHECK(d[0] == 0x33333333u && d[1] == 0x44444444u && (d[2] & 0xffff) == 0x5555u,
          "i=64: v[143:64] = 0x%04x_%08x%08x", d[2] & 0xffff, d[1], d[0]);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
