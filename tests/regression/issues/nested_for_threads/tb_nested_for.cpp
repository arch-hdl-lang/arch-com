#include "VNestedForRepro.h"
#include <cstdio>
#include <cstdlib>
static VNestedForRepro dut;
static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); }
int main() {
    dut.rst = 0; dut.go = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    dut.go = 1;
    // Pulse go for one cycle so the thread runs the outer-for once and then
    // parks back at `wait until go` (since the next iteration sees go=0).
    tick();
    dut.go = 0;
    // Expected after enough time: outer = 3, inner = 12 (3 outer × 4 inner).
    // If the bug (issue #414) is back: outer stays 0 and inner climbs unbounded.
    int max_outer = 0, max_inner = 0;
    for (int i = 0; i < 40; ++i) {
        std::printf("cyc=%d  outer=%d  inner=%d\n", i, dut.outer_visits, dut.inner_visits);
        if (dut.outer_visits > max_outer) max_outer = dut.outer_visits;
        if (dut.inner_visits > max_inner) max_inner = dut.inner_visits;
        tick();
    }
    std::printf("max_outer=%d max_inner=%d\n", max_outer, max_inner);
    if (max_outer != 3 || max_inner != 12) {
        std::fprintf(stderr,
            "FAIL: expected max_outer=3, max_inner=12 (got %d, %d)\n",
            max_outer, max_inner);
        return 1;
    }
    std::printf("PASS\n");
    return 0;
}
