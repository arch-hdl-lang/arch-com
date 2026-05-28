// Testbench for the `expect_verilator_fatal` harness self-test.
//
// We hold the design in reset for a few cycles, deassert reset, then
// drive `we=1, idx=5` for one rising edge. `idx == 5` is out of bounds
// for `Vec<UInt<8>, 4>` and the codegen-emitted SVA
// `_auto_bound_vec_0` must trip on that edge — Verilator `--assert`
// turns the failure into `$fatal(1, "BOUNDS VIOLATION: ...")`, which
// terminates the simulation with a non-zero exit status.
//
// The harness asserts on (a) non-zero exit AND (b) the substring
// "BOUNDS VIOLATION" appearing in the combined stdout+stderr.

#include "VProbe.h"

#include <cstdio>
#include <cstdlib>

static VProbe dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    // Hold reset.
    dut.rst = 1;
    dut.we  = 0;
    dut.idx = 0;
    dut.d   = 0;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();

    // Now drive an out-of-bounds index. The SVA fires on the next
    // rising edge while `we && rst` are not both gating it out, so
    // the simulation aborts before reaching the print below.
    dut.we  = 1;
    dut.idx = 5;
    dut.d   = 0xAB;
    tick();

    // Defensive — if we reach here the SVA didn't fire and the
    // harness should treat the run as a failure (zero exit, no
    // "BOUNDS VIOLATION" substring).
    std::printf("FAIL: simulation did not abort on out-of-bounds Vec write\n");
    return 0;
}
