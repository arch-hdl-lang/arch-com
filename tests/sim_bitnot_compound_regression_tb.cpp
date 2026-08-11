// Compound `~` regression: verify that `~(a & b)` on 1-bit operands evaluates
// to 1/0 in arch sim, not to the unmasked uint8_t 255/254.
//
// Every check below fails pre-fix: the compound operand skipped the clamp, so
// nand_ab read 255/254, the == comparisons never matched, and the loop guard
// was true for all inputs.
#include "Vsim_bitnot_compound_regression.h"
#include <cstdio>
static Vsim_bitnot_compound_regression dut;
static int pass = 0, fail = 0;
#define CHECK(c, m, ...) do { if (c) { printf("  PASS: " m "\n", ##__VA_ARGS__); ++pass; } \
                              else  { printf("  FAIL: " m "\n", ##__VA_ARGS__); ++fail; } } while (0)

static void drive(unsigned a, unsigned b) { dut.a = a; dut.b = b; dut.eval(); }

int main() {
    // a=1,b=1 → (a & b)=1 → ~(a & b) must be exactly 0.
    drive(1, 1);
    CHECK(dut.nand_ab == 0,         "a=1,b=1: ~(a&b) == 0 (got %u)",        (unsigned)dut.nand_ab);
    CHECK(dut.nand_ab_eq_true == 0, "a=1,b=1: (~(a&b)) == true is false (got %u)", (unsigned)dut.nand_ab_eq_true);
    CHECK(dut.nor_ab == 0,          "a=1,b=1: ~(a|b) == 0 (got %u)",        (unsigned)dut.nor_ab);
    CHECK(dut.double_neg == 1,      "a=1,b=1: ~~(a&b) == 1 (got %u)",       (unsigned)dut.double_neg);
    CHECK(dut.guard_count == 0,     "a=1,b=1: guard never taken (got %u)",  (unsigned)dut.guard_count);

    // a=1,b=0 → (a & b)=0 → ~(a & b) must be exactly 1.
    drive(1, 0);
    CHECK(dut.nand_ab == 1,         "a=1,b=0: ~(a&b) == 1 (got %u)",        (unsigned)dut.nand_ab);
    CHECK(dut.nand_ab_eq_true == 1, "a=1,b=0: (~(a&b)) == true (got %u)",   (unsigned)dut.nand_ab_eq_true);
    CHECK(dut.nor_ab == 0,          "a=1,b=0: ~(a|b) == 0 (got %u)",        (unsigned)dut.nor_ab);
    CHECK(dut.double_neg == 0,      "a=1,b=0: ~~(a&b) == 0 (got %u)",       (unsigned)dut.double_neg);
    CHECK(dut.guard_count == 4,     "a=1,b=0: guard taken 4x (got %u)",     (unsigned)dut.guard_count);

    // a=0,b=0 → both AND and OR are 0, so both inversions are 1.
    drive(0, 0);
    CHECK(dut.nand_ab == 1,         "a=0,b=0: ~(a&b) == 1 (got %u)",        (unsigned)dut.nand_ab);
    CHECK(dut.nor_ab == 1,          "a=0,b=0: ~(a|b) == 1 (got %u)",        (unsigned)dut.nor_ab);
    CHECK(dut.guard_count == 4,     "a=0,b=0: guard taken 4x (got %u)",     (unsigned)dut.guard_count);

    printf("=== %d pass / %d fail ===\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
