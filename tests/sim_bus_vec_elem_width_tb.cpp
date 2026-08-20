// arch#858 follow-up: concat of Vec-typed bus-field elements must shift
// by the bus-param-substituted element width (16), not a degraded guess.
#include "Vsim_bus_vec_elem_width.h"
#include <cstdint>
#include <cstdio>
static Vsim_bus_vec_elem_width dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

int main() {
    dut.s_data[0] = 0xBEEF;
    dut.s_data[1] = 0xDEAD;
    dut.s_valid = 1;
    dut.eval();
    CHECK(dut.cat == 0xDEADBEEFu,
          "{s.data[1], s.data[0]} shifts by the bound W=16 (got 0x%08x)",
          (unsigned)dut.cat);

    dut.s_data[1] = 0x1234;
    dut.eval();
    CHECK(dut.cat == 0x1234BEEFu,
          "concat tracks s.data[1] rewrite (got 0x%08x)", (unsigned)dut.cat);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
