// Bool ~ regression: verify that `~bool_value == false` evaluates
// as "bool_value is true" (the obvious intent), not always-false.
#include "Vsim_bool_not_regression.h"
#include <cstdio>
static Vsim_bool_not_regression dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)
int main() {
    // a=true → ~a=false → (~a == false) is TRUE
    dut.a = 1; dut.b = 1; dut.eval();
    CHECK(dut.not_a_eq_false == 1, "a=1: (~a == false) is true (got %u)", (unsigned)dut.not_a_eq_false);
    CHECK(dut.not_b_neg_a == 4,    "b=1: loop ran 4 times (got %u)",       (unsigned)dut.not_b_neg_a);

    // a=false → ~a=true → (~a == false) is FALSE
    dut.a = 0; dut.b = 0; dut.eval();
    CHECK(dut.not_a_eq_false == 0, "a=0: (~a == false) is false (got %u)", (unsigned)dut.not_a_eq_false);
    CHECK(dut.not_b_neg_a == 0,    "b=0: loop ran 0 times (got %u)",       (unsigned)dut.not_b_neg_a);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
