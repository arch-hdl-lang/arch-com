// Regression TB for u128-band `onehot(idx)` (see sim_onehot_u128.arch).
// Only in-range bit positions are asserted per probe; all expected values
// were cross-checked against Verilator-emitted SV.
#include "VOnehotU128.h"
#include <cstdio>
static VOnehotU128 dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

static void drive(int i) { dut.idx = i; dut.eval(); }

int main() {
    // idx=5: in range for all three widths. bit5 -> 0x20 in the low word.
    drive(5);
    CHECK(dut.oh48 == 0x20ULL,          "onehot(5):UInt<48> = 0x%llx (want 20)", (unsigned long long)dut.oh48);
    CHECK(dut.oh100_lo == 0x20ULL && dut.oh100_hi == 0x0ULL,
          "onehot(5):UInt<100> hi=0x%llx lo=0x%llx (want 0,20)", (unsigned long long)dut.oh100_hi, (unsigned long long)dut.oh100_lo);
    CHECK(dut.oh128_lo == 0x20ULL && dut.oh128_hi == 0x0ULL,
          "onehot(5):UInt<128> hi=0x%llx lo=0x%llx (want 0,20)", (unsigned long long)dut.oh128_hi, (unsigned long long)dut.oh128_lo);

    // idx=64: lower boundary of the u128 band. bit64 -> hi word bit 0.
    drive(64);
    CHECK(dut.oh100_hi == 0x1ULL && dut.oh100_lo == 0x0ULL,
          "onehot(64):UInt<100> hi=0x%llx lo=0x%llx (want 1,0)", (unsigned long long)dut.oh100_hi, (unsigned long long)dut.oh100_lo);
    CHECK(dut.oh128_hi == 0x1ULL && dut.oh128_lo == 0x0ULL,
          "onehot(64):UInt<128> hi=0x%llx lo=0x%llx (want 1,0)", (unsigned long long)dut.oh128_hi, (unsigned long long)dut.oh128_lo);

    // idx=99: mid-band. bit99 -> hi word bit 35 -> 0x800000000.
    drive(99);
    CHECK(dut.oh100_hi == 0x800000000ULL && dut.oh100_lo == 0x0ULL,
          "onehot(99):UInt<100> hi=0x%llx lo=0x%llx (want 800000000,0)", (unsigned long long)dut.oh100_hi, (unsigned long long)dut.oh100_lo);
    CHECK(dut.oh128_hi == 0x800000000ULL && dut.oh128_lo == 0x0ULL,
          "onehot(99):UInt<128> hi=0x%llx lo=0x%llx (want 800000000,0)", (unsigned long long)dut.oh128_hi, (unsigned long long)dut.oh128_lo);

    // idx=127: top bit of the 128-bit result. bit127 -> hi word bit 63.
    drive(127);
    CHECK(dut.oh128_hi == 0x8000000000000000ULL && dut.oh128_lo == 0x0ULL,
          "onehot(127):UInt<128> hi=0x%llx lo=0x%llx (want 8000000000000000,0)", (unsigned long long)dut.oh128_hi, (unsigned long long)dut.oh128_lo);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
