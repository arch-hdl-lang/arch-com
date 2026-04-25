// Drive Top's `src` Vec, expect `dst` to mirror element-by-element.
// If the sub-instance Vec port wiring is broken, dst stays at zero.

#include "VTop.h"
#include <cstdio>
#include <cstdint>

static VTop dut;
static int pass = 0, fail = 0;

#define CHECK(cond, msg, ...) do { \
  if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass; } \
  else      { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail; } \
} while (0)

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

int main() {
    printf("=== inst_vec_port_regression sim ===\n");
    dut.rst = 1; tick(); tick();
    dut.rst = 0; tick();

    // Drive each element of `src` to a distinct value; expect `dst` to mirror.
    dut.src_0 = 0x11;
    dut.src_1 = 0x22;
    dut.src_2 = 0x33;
    dut.src_3 = 0x44;
    dut.src_4 = 0x55;
    dut.src_5 = 0x66;
    dut.src_6 = 0x77;
    dut.src_7 = 0x88;
    dut.eval();

    CHECK(dut.dst_0 == 0x11, "dst[0] = 0x11 (got 0x%02x)", (unsigned)dut.dst_0);
    CHECK(dut.dst_1 == 0x22, "dst[1] = 0x22 (got 0x%02x)", (unsigned)dut.dst_1);
    CHECK(dut.dst_2 == 0x33, "dst[2] = 0x33 (got 0x%02x)", (unsigned)dut.dst_2);
    CHECK(dut.dst_3 == 0x44, "dst[3] = 0x44 (got 0x%02x)", (unsigned)dut.dst_3);
    CHECK(dut.dst_4 == 0x55, "dst[4] = 0x55 (got 0x%02x)", (unsigned)dut.dst_4);
    CHECK(dut.dst_5 == 0x66, "dst[5] = 0x66 (got 0x%02x)", (unsigned)dut.dst_5);
    CHECK(dut.dst_6 == 0x77, "dst[6] = 0x77 (got 0x%02x)", (unsigned)dut.dst_6);
    CHECK(dut.dst_7 == 0x88, "dst[7] = 0x88 (got 0x%02x)", (unsigned)dut.dst_7);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
