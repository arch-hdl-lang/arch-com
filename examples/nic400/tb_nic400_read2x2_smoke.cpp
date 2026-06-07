// Smoke test for Nic400Read2x2: M0->S0, M0->S1, M1->S0 + R returns.

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

static void clear_inputs() {
    dut.m0_ar_valid = 0; dut.m0_ar_addr = 0; dut.m0_ar_id = 0;
    dut.m0_ar_len = 0;   dut.m0_ar_size = 0; dut.m0_ar_burst = 0;
    dut.m0_r_ready = 0;
    dut.m1_ar_valid = 0; dut.m1_ar_addr = 0; dut.m1_ar_id = 0;
    dut.m1_ar_len = 0;   dut.m1_ar_size = 0; dut.m1_ar_burst = 0;
    dut.m1_r_ready = 0;
    dut.s0_ar_ready = 0;
    dut.s0_r_valid = 0;  dut.s0_r_data = 0; dut.s0_r_id = 0;
    dut.s0_r_resp = 0;   dut.s0_r_last = 0;
    dut.s1_ar_ready = 0;
    dut.s1_r_valid = 0;  dut.s1_r_data = 0; dut.s1_r_id = 0;
    dut.s1_r_resp = 0;   dut.s1_r_last = 0;
}

static int fail(const char *msg) {
    std::printf("FAIL Nic400Read2x2: %s\n", msg);
    return 1;
}

// Run AR phase: drive master side, accept on selected slave's ready.
// Returns 0 on success; verifies expected ID and decode target.
static int do_ar(int master, uint32_t addr, uint32_t id, int expect_slave, uint32_t expect_slave_id) {
    if (master == 0) {
        dut.m0_ar_valid = 1; dut.m0_ar_addr = addr; dut.m0_ar_id = id;
        dut.m0_ar_len = 0;   dut.m0_ar_size = 2;   dut.m0_ar_burst = 1;
    } else {
        dut.m1_ar_valid = 1; dut.m1_ar_addr = addr; dut.m1_ar_id = id;
        dut.m1_ar_len = 0;   dut.m1_ar_size = 2;   dut.m1_ar_burst = 1;
    }
    if (expect_slave == 0) dut.s0_ar_ready = 1; else dut.s1_ar_ready = 1;

    int saw = 0;
    for (int i = 0; i < 8 && !saw; ++i) {
        tick();
        bool fired = (expect_slave == 0) ? (dut.s0_ar_valid && dut.s0_ar_ready)
                                         : (dut.s1_ar_valid && dut.s1_ar_ready);
        if (fired) {
            saw = 1;
            // Check the OTHER slave's AR is not firing
            if (expect_slave == 0 && dut.s1_ar_valid != 0) return fail("wrong slave fired (s1)");
            if (expect_slave == 1 && dut.s0_ar_valid != 0) return fail("wrong slave fired (s0)");
            uint32_t got_id = (expect_slave == 0) ? dut.s0_ar_id : dut.s1_ar_id;
            if (got_id != expect_slave_id) {
                std::printf("AR id got 0x%x, expected 0x%x\n", got_id, expect_slave_id);
                return 1;
            }
            uint32_t got_addr = (expect_slave == 0) ? dut.s0_ar_addr : dut.s1_ar_addr;
            if (got_addr != addr) return fail("AR addr mismatch");
        }
    }
    if (!saw) return fail("AR never fired");
    // Run one more tick so the state machine advances past the handshake.
    tick();
    // Now release everything
    if (master == 0) dut.m0_ar_valid = 0; else dut.m1_ar_valid = 0;
    dut.s0_ar_ready = 0;
    dut.s1_ar_ready = 0;
    tick();
    return 0;
}

// Run R phase: slave injects R, expect it to land on the originating master.
static int do_r(int slave_src, uint32_t data, uint32_t slave_id, int expect_master, uint32_t expect_master_id) {
    if (slave_src == 0) {
        dut.s0_r_valid = 1; dut.s0_r_data = data; dut.s0_r_id = slave_id;
        dut.s0_r_resp = 0;  dut.s0_r_last = 1;
    } else {
        dut.s1_r_valid = 1; dut.s1_r_data = data; dut.s1_r_id = slave_id;
        dut.s1_r_resp = 0;  dut.s1_r_last = 1;
    }
    if (expect_master == 0) dut.m0_r_ready = 1; else dut.m1_r_ready = 1;

    int saw = 0;
    for (int i = 0; i < 8 && !saw; ++i) {
        tick();
        bool fired = (expect_master == 0) ? (dut.m0_r_valid && dut.m0_r_ready)
                                          : (dut.m1_r_valid && dut.m1_r_ready);
        if (fired) {
            saw = 1;
            if (expect_master == 0 && dut.m1_r_valid != 0) return fail("wrong master got R (m1)");
            if (expect_master == 1 && dut.m0_r_valid != 0) return fail("wrong master got R (m0)");
            uint32_t got_data = (expect_master == 0) ? dut.m0_r_data : dut.m1_r_data;
            uint32_t got_id   = (expect_master == 0) ? dut.m0_r_id   : dut.m1_r_id;
            uint32_t got_last = (expect_master == 0) ? dut.m0_r_last : dut.m1_r_last;
            if (got_data != data) return fail("R data mismatch");
            if (got_id != expect_master_id) {
                std::printf("R id got 0x%x, expected 0x%x\n", got_id, expect_master_id);
                return 1;
            }
            if (got_last != 1) return fail("R last not 1");
        }
    }
    if (!saw) return fail("R never landed");
    tick();
    if (slave_src == 0) dut.s0_r_valid = 0; else dut.s1_r_valid = 0;
    dut.m0_r_ready = 0;
    dut.m1_r_ready = 0;
    tick();
    return 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    tick();

    // Test 1: M0 → S0
    if (do_ar(0, 0x00001000u, 3, /*slave*/0, /*slave_id*/3)) return 1;
    if (do_r(/*slave*/0, 0xDEADBEEFu, /*slave_id*/3, /*master*/0, /*master_id*/3)) return 1;

    // Test 2: M0 → S1
    if (do_ar(0, 0x10001000u, 5, /*slave*/1, /*slave_id*/5)) return 1;
    if (do_r(/*slave*/1, 0xCAFEBABEu, /*slave_id*/5, /*master*/0, /*master_id*/5)) return 1;

    // Test 3: M1 → S0 — verify ID prefix = 1
    if (do_ar(1, 0x00002000u, 7, /*slave*/0, /*slave_id*/0xF)) return 1;
    if (do_r(/*slave*/0, 0xC0FFEE00u, /*slave_id*/0xF, /*master*/1, /*master_id*/7)) return 1;

    std::printf("PASS Nic400Read2x2 smoke: decode + ID remap + return route OK\n");
    return 0;
}
