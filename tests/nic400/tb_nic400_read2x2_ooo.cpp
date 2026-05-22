// Out-of-order completion:
//   M0 issues AR(id=1) → S0 and AR(id=2) → S1.
//   S1 responds first (id=2), then S0 (id=1).
//   Both R responses must land at M0 with the correct stripped ID and data.

#include "VNic400Read2x2.h"
#include <cstdint>
#include <cstdio>

static VNic400Read2x2 dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    dut.rst = 0;
    dut.m0_ar_valid = 0; dut.m1_ar_valid = 0;
    dut.s0_ar_ready = 0; dut.s1_ar_ready = 0;
    dut.s0_r_valid = 0;  dut.s1_r_valid = 0;
    dut.m0_r_ready = 0;  dut.m1_r_ready = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    tick();

    // Issue AR id=1 → S0
    dut.m0_ar_valid = 1; dut.m0_ar_addr = 0x00001000; dut.m0_ar_id = 1;
    dut.m0_ar_size = 2; dut.m0_ar_burst = 1;
    dut.s0_ar_ready = 1;
    for (int i = 0; i < 8; ++i) {
        tick();
        if (dut.s0_ar_valid && dut.s0_ar_ready) break;
    }
    tick();
    dut.m0_ar_valid = 0; dut.s0_ar_ready = 0;
    tick();

    // Issue AR id=2 → S1
    dut.m0_ar_valid = 1; dut.m0_ar_addr = 0x10001000; dut.m0_ar_id = 2;
    dut.s1_ar_ready = 1;
    for (int i = 0; i < 8; ++i) {
        tick();
        if (dut.s1_ar_valid && dut.s1_ar_ready) break;
    }
    tick();
    dut.m0_ar_valid = 0; dut.s1_ar_ready = 0;
    tick();

    // Now slave 1 responds FIRST (out of order vs. issue)
    dut.s1_r_valid = 1; dut.s1_r_data = 0x22222222; dut.s1_r_id = 2; dut.s1_r_last = 1;
    dut.m0_r_ready = 1;
    int got_first = 0;
    uint32_t first_id = 0, first_data = 0;
    for (int i = 0; i < 8 && !got_first; ++i) {
        tick();
        if (dut.m0_r_valid && dut.m0_r_ready) {
            got_first = 1;
            first_id = dut.m0_r_id;
            first_data = dut.m0_r_data;
        }
    }
    if (!got_first || first_id != 2 || first_data != 0x22222222) {
        std::printf("FAIL ooo: first response id=0x%x data=0x%x\n", first_id, first_data);
        return 1;
    }
    tick();
    dut.s1_r_valid = 0;
    tick();

    // Then slave 0 responds
    dut.s0_r_valid = 1; dut.s0_r_data = 0x11111111; dut.s0_r_id = 1; dut.s0_r_last = 1;
    int got_second = 0;
    uint32_t second_id = 0, second_data = 0;
    for (int i = 0; i < 8 && !got_second; ++i) {
        tick();
        if (dut.m0_r_valid && dut.m0_r_ready) {
            got_second = 1;
            second_id = dut.m0_r_id;
            second_data = dut.m0_r_data;
        }
    }
    if (!got_second || second_id != 1 || second_data != 0x11111111) {
        std::printf("FAIL ooo: second response id=0x%x data=0x%x\n", second_id, second_data);
        return 1;
    }

    std::printf("PASS Nic400Read2x2 OOO: R(id=2) first, R(id=1) second — interleaved completion OK\n");
    return 0;
}
