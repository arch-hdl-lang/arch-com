// Parallel disjoint targets:
//   M0 issues AR to S0, M1 issues AR to S1, simultaneously.
//   Both should advance independently — 2 transactions/cycle throughput.

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

    // Both masters issue AR simultaneously, to disjoint slaves.
    dut.m0_ar_valid = 1; dut.m0_ar_addr = 0x00001000; dut.m0_ar_id = 2;
    dut.m0_ar_size = 2; dut.m0_ar_burst = 1;
    dut.m1_ar_valid = 1; dut.m1_ar_addr = 0x10002000; dut.m1_ar_id = 4;
    dut.m1_ar_size = 2; dut.m1_ar_burst = 1;
    dut.s0_ar_ready = 1;
    dut.s1_ar_ready = 1;

    int s0_fired = 0, s1_fired = 0;
    uint32_t s0_id = 0, s1_id = 0;
    for (int i = 0; i < 8 && !(s0_fired && s1_fired); ++i) {
        tick();
        if (!s0_fired && dut.s0_ar_valid && dut.s0_ar_ready) {
            s0_fired = 1;
            s0_id = dut.s0_ar_id;
        }
        if (!s1_fired && dut.s1_ar_valid && dut.s1_ar_ready) {
            s1_fired = 1;
            s1_id = dut.s1_ar_id;
        }
    }
    if (!s0_fired || !s1_fired) {
        std::printf("FAIL parallel: s0_fired=%d s1_fired=%d\n", s0_fired, s1_fired);
        return 1;
    }
    // s0 should see id={0, m0_id=2} = 2
    // s1 should see id={1, m1_id=4} = 0b1_100 = 12
    if (s0_id != 2 || s1_id != 12) {
        std::printf("FAIL parallel: ID remap s0=0x%x s1=0x%x (expected 2, 12)\n", s0_id, s1_id);
        return 1;
    }
    tick();
    dut.m0_ar_valid = 0; dut.m1_ar_valid = 0;
    dut.s0_ar_ready = 0; dut.s1_ar_ready = 0;
    tick();

    // R responses arrive simultaneously on both slaves
    dut.s0_r_valid = 1; dut.s0_r_data = 0xAA00AA00; dut.s0_r_id = 2;  dut.s0_r_last = 1;
    dut.s1_r_valid = 1; dut.s1_r_data = 0xBB00BB00; dut.s1_r_id = 12; dut.s1_r_last = 1;
    dut.m0_r_ready = 1;
    dut.m1_r_ready = 1;

    int m0_got = 0, m1_got = 0;
    uint32_t m0_data = 0, m1_data = 0;
    for (int i = 0; i < 8 && !(m0_got && m1_got); ++i) {
        tick();
        if (!m0_got && dut.m0_r_valid && dut.m0_r_ready) {
            m0_got = 1; m0_data = dut.m0_r_data;
        }
        if (!m1_got && dut.m1_r_valid && dut.m1_r_ready) {
            m1_got = 1; m1_data = dut.m1_r_data;
        }
    }
    if (!m0_got || !m1_got) {
        std::printf("FAIL parallel R: m0_got=%d m1_got=%d\n", m0_got, m1_got);
        return 1;
    }
    if (m0_data != 0xAA00AA00 || m1_data != 0xBB00BB00) {
        std::printf("FAIL parallel R data: m0=0x%x m1=0x%x\n", m0_data, m1_data);
        return 1;
    }

    std::printf("PASS Nic400Read2x2 parallel: 2 disjoint AR/R simultaneously OK\n");
    return 0;
}
