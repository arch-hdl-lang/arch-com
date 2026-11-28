// Regression TB for 65..=128-bit `+%` / `-%` / `*%` width-correctness in native
// `arch sim` (see sim_u128_band_mask.arch). All expected values were
// cross-checked against the SystemVerilog backend under Verilator 5.048.
#include "VBandMask.h"
#include <cstdio>
static VBandMask dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

static void drive(unsigned long long a, unsigned b) {
    dut.a = a; dut.b = b;
    dut.rst = 1; dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); dut.clk = 0; dut.eval();
    dut.rst = 0;
    for (int i = 0; i < 3; i++) { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); }
    dut.clk = 0; dut.eval();
}

int main() {
    // main vector: a = 0x0123456789ABCDEF, b = 1  (w96 = 0x0123456789ABCDEF_00000001)
    drive(0x0123456789ABCDEFULL, 0x1);
    CHECK(dut.sub_hi  == 0x1234567u,          "w96 -%% 1   [95:64] = 0x%x (want 1234567)",  (unsigned)dut.sub_hi);
    CHECK(dut.add_hi  == 0x2468acfu,          "w96 +%% w96 [95:64] = 0x%x (want 2468acf)",  (unsigned)dut.add_hi);
    CHECK(dut.mul_hi  == 0x2468acfu,          "w96 *%% 2   [95:64] = 0x%x (want 2468acf)",  (unsigned)dut.mul_hi);
    CHECK(dut.mix_hi  == 0x1234567u,          "w96 +%% c40 [95:64] = 0x%x (want 1234567)",  (unsigned)dut.mix_hi);
    CHECK(dut.sw_hi   == 0x1234567u,          "signed(w96) -%% 1 [95:64] = 0x%x (want 1234567)", (unsigned)dut.sw_hi);
    CHECK(dut.w65_bit64 == 0,                 "w65 bit64 (a=0123..)  = %d (want 0)",        (int)dut.w65_bit64);
    CHECK(dut.w127_hi == 0x2468acf13579bdeULL,"w127+%%w127 [126:64] = 0x%llx (want 2468acf13579bde)", (unsigned long long)dut.w127_hi);
    CHECK(dut.w128_hi == 0x123456789abcdefULL,"{a,a}+%%1   [127:64] = 0x%llx (want 123456789abcdef)", (unsigned long long)dut.w128_hi);
    CHECK(dut.acc_hi  == 0x369d036u,          "acc(3x w96) [95:64] = 0x%x (want 369d036)",  (unsigned)dut.acc_hi);

    // carry vector: a = 0xFFFFFFFFFFFFFFFF, b = 0  (exercises carry into bit 64 + wraps)
    drive(0xFFFFFFFFFFFFFFFFULL, 0x0);
    CHECK(dut.w65_bit64 == 1,                 "w65 bit64 (a=FF..)    = %d (want 1)",        (int)dut.w65_bit64);
    CHECK(dut.sw_hi   == 0xffffffffu,         "signed(w96) -%% 1 [95:64] (neg) = 0x%x (want ffffffff)", (unsigned)dut.sw_hi);
    CHECK(dut.sub_hi  == 0xffffffffu,         "w96 -%% 1   [95:64] = 0x%x (want ffffffff)", (unsigned)dut.sub_hi);
    CHECK(dut.w128_hi == 0x0ULL,              "{a,a}+%%1   [127:64] = 0x%llx (want 0, wrap)", (unsigned long long)dut.w128_hi);
    CHECK(dut.acc_hi  == 0xffffffffu,         "acc(3x w96) [95:64] = 0x%x (want ffffffff)", (unsigned)dut.acc_hi);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
