#include "VFsmStateProbe.h"
#include <cstdio>

static int pass = 0;
static int fail = 0;

#define CHECK(cond, msg, ...) do { \
    if (cond) { std::printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass; } \
    else { std::printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail; } \
} while (0)

static void tick(VFsmStateProbe& dut) {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    dut.clk = 0;
    dut.eval();
}

int main() {
    VFsmStateProbe dut;

    dut.rst = 1;
    dut.advance = 0;
    tick(dut);
    CHECK(dut.state_r == VFsmStateProbe::STATE_IDLE, "reset exposes Idle on state_r");
    CHECK(dut.state_code == 0, "Idle drives state_code 0");

    dut.rst = 0;
    tick(dut);
    CHECK(dut.state_r == VFsmStateProbe::STATE_IDLE, "state_r holds Idle without advance");

    dut.advance = 1;
    tick(dut);
    CHECK(dut.state_r == VFsmStateProbe::STATE_BUSY, "state_r advances to Busy");
    CHECK(dut.state_code == 1, "Busy drives state_code 1");

    tick(dut);
    CHECK(dut.state_r == VFsmStateProbe::STATE_DONE, "state_r advances to Done");
    CHECK(dut.state_code == 2, "Done drives state_code 2");

    tick(dut);
    CHECK(dut.state_r == VFsmStateProbe::STATE_IDLE, "state_r wraps back to Idle");

    std::printf("PASS native FSM state_r probe: %d pass / %d fail\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
