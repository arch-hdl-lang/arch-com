// Smoke test for the hierarchical Nic400Fabric — read-only 2x2 crossbar.
// Drives master 0 → slave 0, master 1 → slave 1, master 0 → slave 1 in
// sequence. Verifies decode + ID prefix encoding + return-route demux all
// work end-to-end through the MasterPort/SlavePort modules.

#include "VNic400Fabric.h"
#include <cstdint>
#include <cstdio>

static VNic400Fabric dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

static int fail(const char *msg) {
    std::printf("FAIL Nic400Fabric: %s\n", msg);
    return 1;
}

// Drive AR phase. master 0..1, addr (bit 28 picks slave 0|1), id (3-bit).
// Returns 0 on success after observing the AR handshake at the expected slave.
static int do_ar(int master, uint32_t addr, uint32_t id, int expect_slave, uint32_t expect_slave_id) {
    if (master == 0) {
        dut.m_0_ar_valid = 1; dut.m_0_ar_addr = addr; dut.m_0_ar_id = id;
        dut.m_0_ar_len = 0; dut.m_0_ar_size = 2; dut.m_0_ar_burst = 1;
    } else {
        dut.m_1_ar_valid = 1; dut.m_1_ar_addr = addr; dut.m_1_ar_id = id;
        dut.m_1_ar_len = 0; dut.m_1_ar_size = 2; dut.m_1_ar_burst = 1;
    }
    if (expect_slave == 0) dut.s_0_ar_ready = 1; else dut.s_1_ar_ready = 1;

    for (int i = 0; i < 8; ++i) {
        tick();
        uint8_t v = (expect_slave == 0) ? dut.s_0_ar_valid : dut.s_1_ar_valid;
        uint8_t r = (expect_slave == 0) ? dut.s_0_ar_ready : dut.s_1_ar_ready;
        if (v && r) {
            uint32_t got_id = (expect_slave == 0) ? dut.s_0_ar_id : dut.s_1_ar_id;
            if (got_id != expect_slave_id) {
                std::printf("AR id got 0x%x, expected 0x%x\n", got_id, expect_slave_id);
                return 1;
            }
            // The OTHER slave's AR must NOT fire.
            uint8_t other = (expect_slave == 0) ? dut.s_1_ar_valid : dut.s_0_ar_valid;
            if (other != 0) return fail("wrong slave fired");
            tick();
            if (master == 0) dut.m_0_ar_valid = 0; else dut.m_1_ar_valid = 0;
            dut.s_0_ar_ready = 0;
            dut.s_1_ar_ready = 0;
            tick();
            return 0;
        }
    }
    return fail("AR never fired");
}

// Drive R return from one slave; expect it to land at the originating
// master with the stripped ID.
static int do_r(int slave_src, uint32_t data, uint32_t slave_id, int expect_master, uint32_t expect_master_id) {
    if (slave_src == 0) {
        dut.s_0_r_valid = 1; dut.s_0_r_data = data; dut.s_0_r_id = slave_id;
        dut.s_0_r_resp = 0;  dut.s_0_r_last = 1;
    } else {
        dut.s_1_r_valid = 1; dut.s_1_r_data = data; dut.s_1_r_id = slave_id;
        dut.s_1_r_resp = 0;  dut.s_1_r_last = 1;
    }
    if (expect_master == 0) dut.m_0_r_ready = 1; else dut.m_1_r_ready = 1;

    for (int i = 0; i < 16; ++i) {
        tick();
        uint8_t v = (expect_master == 0) ? dut.m_0_r_valid : dut.m_1_r_valid;
        uint8_t r = (expect_master == 0) ? dut.m_0_r_ready : dut.m_1_r_ready;
        if (v && r) {
            uint32_t got_data = (expect_master == 0) ? dut.m_0_r_data : dut.m_1_r_data;
            uint32_t got_id   = (expect_master == 0) ? dut.m_0_r_id   : dut.m_1_r_id;
            if (got_data != data) return fail("R data mismatch");
            if (got_id != expect_master_id) {
                std::printf("R id got 0x%x, expected 0x%x\n", got_id, expect_master_id);
                return 1;
            }
            uint8_t other = (expect_master == 0) ? dut.m_1_r_valid : dut.m_0_r_valid;
            if (other != 0) return fail("wrong master got R");
            tick();
            if (slave_src == 0) dut.s_0_r_valid = 0; else dut.s_1_r_valid = 0;
            dut.m_0_r_ready = 0;
            dut.m_1_r_ready = 0;
            tick();
            return 0;
        }
    }
    return fail("R never landed");
}

int main() {
    // Reset (active-low)
    dut.rst = 0;
    dut.m_0_ar_valid = 0; dut.m_1_ar_valid = 0;
    dut.s_0_ar_ready = 0; dut.s_1_ar_ready = 0;
    dut.s_0_r_valid = 0;  dut.s_1_r_valid = 0;
    dut.m_0_r_ready = 0;  dut.m_1_r_ready = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    tick();

    // M0 → S0 (id=3 → slave-side id={0, 3} = 3)
    if (do_ar(0, 0x00001000, 3, /*slave*/0, /*slave_id*/3)) return 1;
    if (do_r(/*slave*/0, 0xDEADBEEF, /*slave_id*/3, /*master*/0, /*master_id*/3)) return 1;

    // M1 → S1 (id=5 → slave-side id={1, 5} = 0b1_101 = 13)
    if (do_ar(1, 0x10002000, 5, /*slave*/1, /*slave_id*/13)) return 1;
    if (do_r(/*slave*/1, 0xCAFEBABE, /*slave_id*/13, /*master*/1, /*master_id*/5)) return 1;

    // M0 → S1 (cross-slave route, id=2 → slave-side id={0, 2} = 2)
    if (do_ar(0, 0x10001000, 2, /*slave*/1, /*slave_id*/2)) return 1;
    if (do_r(/*slave*/1, 0xC0FFEE00, /*slave_id*/2, /*master*/0, /*master_id*/2)) return 1;

    std::printf("PASS Nic400Fabric smoke: hierarchical 2x2 decode + ID remap + return route OK\n");
    return 0;
}
