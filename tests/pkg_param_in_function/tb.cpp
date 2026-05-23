#include <cstdio>
#include <cstdlib>
#include "VProbe.h"

int main() {
    VProbe dut;
    // a = 0x80000000 → top 4 bits = 0x8 after shift by REGION_BITS=28.
    dut.a = 0x80000000u;
    dut.eval();
    if (dut.out != 0x8) {
        std::fprintf(stderr, "FAIL: out=0x%x expected 0x8\n", (unsigned)dut.out);
        return 1;
    }
    dut.a = 0x10000000u;
    dut.eval();
    if (dut.out != 0x1) {
        std::fprintf(stderr, "FAIL: out=0x%x expected 0x1\n", (unsigned)dut.out);
        return 1;
    }
    std::printf("PASS pkg_param_in_function\n");
    return 0;
}
