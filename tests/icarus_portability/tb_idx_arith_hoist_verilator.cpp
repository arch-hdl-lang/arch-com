// arch#650 regression: (a - b)[i] behavioral check under Verilator.
#include "VIdxArithHoist.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VIdxArithHoist* dut = new VIdxArithHoist;
    int fails = 0;
    auto check = [&](int a, int b, int i, int expect) {
        dut->a = a;
        dut->b = b;
        dut->i = i;
        dut->eval();
        if (dut->y != expect) {
            printf("FAIL: a=%d b=%d i=%d y=%d expect=%d\n", a, b, i, dut->y, expect);
            fails++;
        }
    };
    check(5, 3, 1, 1);
    check(200, 7, 0, 1);
    check(10, 10, 0, 0);
    delete dut;
    if (fails) {
        printf("FAILURES: %d\n", fails);
        return 1;
    }
    printf("PASS\n");
    return 0;
}
