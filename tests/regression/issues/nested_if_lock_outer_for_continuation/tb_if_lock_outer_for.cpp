// Testbench for IfLockOuterForRepro.
//
// Pulses `go` for one cycle, then watches `outer_count` and `inner_count`
// for ~50 cycles. The thread should:
//   - take 3 outer iterations (outer_count: 0 → 1 → 2 → 3),
//   - run 2 inner sub-beats per outer iteration (inner_count: 0 → 6),
// and then park back at `wait until go;` with both counters frozen.
//
// With the bug: the inner-for's if-branch state transitions to S0_entry
// instead of the outer-for loop-continuation state, so on the very first
// outer iteration the thread restarts at the top (`wait until go;`). With
// go=0 by then, the thread sits forever; outer_count never reaches 3.
#include "VIfLockOuterForRepro.h"
#include <cstdio>
#include <cstdlib>

static VIfLockOuterForRepro dut;
static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

int main() {
    dut.rst = 0;
    dut.go  = 0;
    for (int i = 0; i < 4; ++i) tick();   // hold in reset
    dut.rst = 1;                          // deassert async-low reset
    dut.go  = 1;
    tick();
    dut.go  = 0;                          // single-cycle pulse

    int max_outer = 0, max_inner = 0;
    for (int i = 0; i < 60; ++i) {
        std::printf("cyc=%d  outer=%d  inner=%d\n",
                    i, dut.outer_count, dut.inner_count);
        if (dut.outer_count > max_outer) max_outer = dut.outer_count;
        if (dut.inner_count > max_inner) max_inner = dut.inner_count;
        tick();
    }
    std::printf("max_outer=%d max_inner=%d\n", max_outer, max_inner);

    // Expected once the codegen bug is fixed:
    //   max_outer == 3, max_inner == 6
    // Current buggy behaviour: max_outer is stuck below 3 (likely 0 or 1).
    // Inner branches add 1 (k==0) and 2 (k==1) — per outer iter inner += 3.
    if (max_outer != 3 || max_inner != 9) {
        std::fprintf(stderr,
            "FAIL: expected max_outer=3, max_inner=9 (got %d, %d)\n",
            max_outer, max_inner);
        return 1;
    }
    std::printf("PASS\n");
    return 0;
}
