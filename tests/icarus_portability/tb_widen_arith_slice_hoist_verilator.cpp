// Regression: slices into the widened high bits of `a + b` / `a * b`
// behavioral check under Verilator.
#include "VWidenArithSliceHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VWidenArithSliceHoist* dut = new VWidenArithSliceHoist;
    int fails = 0;
    auto check = [&](int a, int b, int carry, int ps, int hi, int top) {
        dut->a = a;
        dut->b = b;
        dut->eval();
        if (dut->add_carry != carry || dut->add_ps != ps ||
            dut->mul_hi != hi || dut->mul_top != top) {
            printf("FAIL a=%d b=%d: carry=%d(exp %d) ps=%d(exp %d) hi=%d(exp %d) top=%d(exp %d)\n",
                   a, b, dut->add_carry, carry, dut->add_ps, ps,
                   dut->mul_hi, hi, dut->mul_top, top);
            fails++;
        }
    };
    check(200, 100, 1, 9,  78,  0);
    check(255, 255, 1, 15, 254, 1);
    check(100, 100, 0, 6,  39,  0);
    check(0,   0,   0, 0,  0,   0);
    delete dut;
    if (fails) {
        printf("FAILURES: %d\n", fails);
        return 1;
    }
    printf("PASS\n");
    return 0;
}
