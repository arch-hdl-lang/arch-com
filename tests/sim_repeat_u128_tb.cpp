// Regression TB for u128-band `{N{x}}` replication (see sim_repeat_u128.arch).
// Expected values cross-checked against Verilator-emitted SV.
#include "VRepeatU128.h"
#include <cstdio>
static VRepeatU128 dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

int main() {
    dut.b40 = 0xA5C3E1F00FULL;
    dut.b64 = 0x0123456789ABCDEFULL;
    dut.b24 = 0xABCDEFULL;
    dut.eval();

    // {3{b40}} = 120 bits: [119:80]=b40, [79:40]=b40, [39:0]=b40.
    CHECK(dut.hi == 0xA5C3E1F00FA5C3ULL,
          "{3{b40}}[119:64] = 0x%llx (want a5c3e1f00fa5c3)", (unsigned long long)dut.hi);
    CHECK(dut.lo == 0xE1F00FA5C3E1F00FULL,
          "{3{b40}}[63:0] = 0x%llx (want e1f00fa5c3e1f00f)", (unsigned long long)dut.lo);

    // {2{b64}} = 128 bits (upper band boundary): both halves == b64.
    CHECK(dut.w128_hi == 0x0123456789ABCDEFULL,
          "{2{b64}}[127:64] = 0x%llx", (unsigned long long)dut.w128_hi);
    CHECK(dut.w128_lo == 0x0123456789ABCDEFULL,
          "{2{b64}}[63:0] = 0x%llx", (unsigned long long)dut.w128_lo);

    // {2{b24}} = 48 bits (<=64 control, unchanged uint64 path).
    CHECK(dut.c48 == 0xABCDEFABCDEFULL,
          "{2{b24}} = 0x%llx (want abcdefabcdef)", (unsigned long long)dut.c48);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
