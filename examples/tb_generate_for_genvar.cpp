// Sim TB for the shape-stable SV-genvar `generate_for + inst` form.
//
// Verifies that arch sim's local unroll of preserved Generate(For) blocks
// produces the same wire-through behavior as the pre-#399 always-unroll-at-
// elaboration path: each pt_i forwards req[i] to gnt[i].

#include "VGenDemo.h"
#include <cstdio>

int main() {
    VGenDemo dut;

    auto check = [&](int r0, int r1) {
        dut.req[0] = r0;
        dut.req[1] = r1;
        dut.eval();
        if (dut.gnt[0] != r0 || dut.gnt[1] != r1) {
            printf("FAIL: req={%d,%d} gnt={%d,%d}\n",
                   r0, r1, dut.gnt[0], dut.gnt[1]);
            return false;
        }
        printf("OK: req={%d,%d} gnt={%d,%d}\n",
               r0, r1, dut.gnt[0], dut.gnt[1]);
        return true;
    };

    if (!check(0, 0)) return 1;
    if (!check(1, 0)) return 1;
    if (!check(0, 1)) return 1;
    if (!check(1, 1)) return 1;

    printf("PASS generate_for genvar sim\n");
    return 0;
}
