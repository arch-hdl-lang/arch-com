// arch#867 regression: native-sim FP32 -> sub-8-bit MX narrows
// (`to_fp4e2m1` / `to_fp6e2m3` / `to_fp6e3m2`) must compile and
// round-trip. Reference codes come from the OCP MX format tables and
// match the widen-direction values the FP/MX review recorded.
#include "Vsim_fp_narrow_mx_regression.h"
#include <cstdio>
#include <cstring>
#include <cstdint>
static Vsim_fp_narrow_mx_regression dut;
static int pass = 0, fail = 0;
static uint32_t f2b(float f) { uint32_t b; memcpy(&b, &f, 4); return b; }
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

int main() {
    // FP4E2M1: value set +-{0,0.5,1,1.5,2,3,4,6}, code = s exp[1:0] man.
    dut.x = f2b(0.5f);  dut.eval();
    CHECK(dut.y1 == 0x1, "0.5 -> E2M1 0x1 (got 0x%x)", (unsigned)dut.y1);
    dut.x = f2b(6.0f);  dut.eval();
    CHECK(dut.y1 == 0x7, "6.0 -> E2M1 0x7 max finite (got 0x%x)", (unsigned)dut.y1);
    dut.x = f2b(-6.0f); dut.eval();
    CHECK(dut.y1 == 0xF, "-6.0 -> E2M1 0xF (got 0x%x)", (unsigned)dut.y1);

    // FP6E2M3: 1+2+3, bias 1, max finite 7.5 (code 0x1F = 0 11 111).
    dut.x = f2b(1.0f);  dut.eval();
    CHECK(dut.y2 == 0x08, "1.0 -> E2M3 0x08 (got 0x%x)", (unsigned)dut.y2);
    dut.x = f2b(7.5f);  dut.eval();
    CHECK(dut.y2 == 0x1F, "7.5 -> E2M3 0x1F max finite (got 0x%x)", (unsigned)dut.y2);

    // FP6E3M2: 1+3+2, bias 3. 1.0 = 0 011 00 = 0x0C.
    dut.x = f2b(1.0f);  dut.eval();
    CHECK(dut.y3 == 0x0C, "1.0 -> E3M2 0x0C (got 0x%x)", (unsigned)dut.y3);
    dut.x = f2b(-1.0f); dut.eval();
    CHECK(dut.y3 == 0x2C, "-1.0 -> E3M2 0x2C (got 0x%x)", (unsigned)dut.y3);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
