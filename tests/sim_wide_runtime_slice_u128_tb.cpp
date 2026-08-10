// arch#868 regression TB: runtime-bound width-80 slice of a 128-bit base.
#include "VWideRuntimeSlice.h"
#include <cstdio>
static VWideRuntimeSlice dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

int main() {
    _arch_u128 v = ((_arch_u128)0xABCDEF0123456789ULL << 64) | (_arch_u128)0x9ABCDEF012345678ULL;
    dut.v = v;

    // i = 0: o = v[79:0] = 0x6789_9ABCDEF012345678
    dut.i = 0; dut.eval();
    const uint32_t* d = dut.o.data();
    CHECK(d[0] == 0x12345678u && d[1] == 0x9abcdef0u && (d[2] & 0xffff) == 0x6789u,
          "i=0: v[79:0] = 0x%04x_%08x%08x", d[2] & 0xffff, d[1], d[0]);

    // i = 48: o = v[127:48] = 0xABCD_EF012345_67899ABC (top 80 bits)
    dut.i = 48; dut.eval();
    d = dut.o.data();
    CHECK(d[0] == 0x67899ABCu && d[1] == 0xEF012345u && (d[2] & 0xffff) == 0xABCDu,
          "i=48: v[127:48] = 0x%04x_%08x%08x", d[2] & 0xffff, d[1], d[0]);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
