// 3x4 write-path smoke test for the Nic400FabricRs1 (per-master reg slices).
//
// Mirrors tb_nic400_fabric_write.cpp but targets the wrapper module
// `Nic400FabricRs1`, which inserts a 1-stage `Nic400EdgeRegSlice` on
// every (master → inner-fabric) edge. The external port shape is
// identical to the un-sliced fabric (drop-in replacement), so the
// per-handshake checks are the same — the only behavioural delta is
// that each phase (AW, W, B) takes one extra cycle of latency through
// the slice, plus a similar +1-cycle hop on the response path. We
// bump the per-phase observation watchdog from 32 → 64 cycles to
// absorb that.
//
// Coverage (5 (master, slave) pairs):
//   M0 → S0, M1 → S1, M2 → S2, M0 → S3, M2 → S0.

#include "VNic400FabricRs1.h"
#include <cstdint>
#include <cstdio>

static VNic400FabricRs1 dut;
static uint64_t cycle = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle++;
}

// Pre-edge sample / post-edge advance, so the TB can observe Mealy
// handshake combos BEFORE the rising edge fuses them into next-state.
static void pre_edge() {
    dut.clk = 0; dut.eval();
}
static void post_edge() {
    dut.clk = 1; dut.eval();
    cycle++;
}

static void clear_inputs() {
    for (int i = 0; i < 3; ++i) {
        dut.m_ar_valid[i] = 0; dut.m_ar_addr[i] = 0; dut.m_ar_id[i] = 0;
        dut.m_ar_len[i] = 0;   dut.m_ar_size[i] = 0; dut.m_ar_burst[i] = 0;
        dut.m_r_ready[i] = 0;
        dut.m_aw_valid[i] = 0; dut.m_aw_addr[i] = 0; dut.m_aw_id[i] = 0;
        dut.m_aw_len[i] = 0;   dut.m_aw_size[i] = 0; dut.m_aw_burst[i] = 0;
        dut.m_w_valid[i] = 0;  dut.m_w_data[i] = 0;  dut.m_w_strb[i] = 0;
        dut.m_w_last[i] = 0;
        dut.m_b_ready[i] = 0;
    }
    for (int j = 0; j < 4; ++j) {
        dut.s_ar_ready[j] = 0;
        dut.s_r_valid[j] = 0; dut.s_r_data[j] = 0; dut.s_r_id[j] = 0;
        dut.s_r_resp[j] = 0;  dut.s_r_last[j] = 0;
        dut.s_aw_ready[j] = 0;
        dut.s_w_ready[j] = 0;
        dut.s_b_valid[j] = 0; dut.s_b_id[j] = 0; dut.s_b_resp[j] = 0;
    }
}

static int fail(const char* msg) {
    std::printf("FAIL %s (cycle=%llu)\n", msg, (unsigned long long)cycle);
    return 1;
}

static int do_write(unsigned master, unsigned addr_high_bits,
                    unsigned slave, unsigned master_id, uint32_t data) {
    uint32_t addr = (slave << 28) | (addr_high_bits << 12) | 0x0;
    unsigned expect_slave_id = ((master & 0x3) << 3) | (master_id & 0x7);

    dut.m_aw_addr[master]  = addr;
    dut.m_aw_id[master]    = master_id;
    dut.m_aw_len[master]   = 0;
    dut.m_aw_size[master]  = 2;
    dut.m_aw_burst[master] = 1;
    dut.m_aw_valid[master] = 1;

    dut.m_w_valid[master] = 1;
    dut.m_w_data[master]  = data;
    dut.m_w_strb[master]  = 0xF;
    dut.m_w_last[master]  = 1;

    dut.s_aw_ready[slave] = 1;
    dut.s_w_ready[slave]  = 1;

    int aw_seen_m = 0, w_seen_m = 0;     // master-side handshake (when slice accepts)
    int aw_seen_s = 0, w_seen_s = 0;     // slave-side handshake (for routing check + data check)
    int aw_id_correct = -1;
    int w_data_correct = -1;

    // Watchdog: 64 cycles (vs 32 in the un-sliced TB) — each handshake
    // walks through one reg slice + the inner fabric thread.
    //
    // Critical: with a reg slice on the master side, AXI protocol says the
    // master must drop aw_valid as soon as it observes its OWN aw_ready=1
    // (slice's up-side handshake). If we wait for the slave-side handshake
    // to clear m_aw_valid, the still-held m_aw_valid causes the slice to
    // accept-while-drain → phantom second AW reaches the slave → SlavePort
    // gets stuck on the second AW waiting for an s_aw_ready that we already
    // cleared.
    //
    // So: track master-side handshake separately and clear m_aw_valid as
    // soon as the slice accepts it. Track slave-side independently for the
    // routing-correctness assertions.
    for (int i = 0; i < 64; ++i) {
        pre_edge();
        // Master-side: clear m_aw_valid the moment the slice accepts.
        if (!aw_seen_m && dut.m_aw_valid[master] && dut.m_aw_ready[master]) {
            aw_seen_m = 1;
        }
        if (!w_seen_m && dut.m_w_valid[master] && dut.m_w_ready[master]) {
            w_seen_m = 1;
        }
        // Slave-side: assert routing + capture data.
        if (!aw_seen_s && dut.s_aw_valid[slave] && dut.s_aw_ready[slave]) {
            aw_seen_s = 1;
            aw_id_correct = (dut.s_aw_id[slave] == expect_slave_id) ? 1 : 0;
            for (unsigned other_s = 0; other_s < 4; ++other_s) {
                if (other_s != slave && dut.s_aw_valid[other_s]) {
                    return fail("AW leaked to wrong slave");
                }
            }
        }
        if (!w_seen_s && dut.s_w_valid[slave] && dut.s_w_ready[slave]) {
            w_seen_s = 1;
            w_data_correct = (dut.s_w_data[slave] == data && dut.s_w_last[slave]) ? 1 : 0;
        }
        post_edge();
        // Clear master-side drives as soon as the slice accepts them.
        if (aw_seen_m) dut.m_aw_valid[master] = 0;
        if (w_seen_m)  dut.m_w_valid[master]  = 0;
        if (aw_seen_s && w_seen_s) {
            dut.s_aw_ready[slave]  = 0;
            dut.s_w_ready[slave]   = 0;
            break;
        }
    }
    int aw_seen = aw_seen_s;
    int w_seen  = w_seen_s;
    if (!aw_seen) return fail("AW handshake never observed");
    if (!w_seen)  return fail("W handshake never observed");
    if (aw_id_correct == 0) {
        std::printf("FAIL AW id wrong at slave %u (expected 0x%x)\n", slave, expect_slave_id);
        return 1;
    }
    if (w_data_correct == 0) {
        std::printf("FAIL W data or last wrong at slave %u (expected data 0x%x, last=1)\n", slave, data);
        return 1;
    }

    // ── B phase ─────────────────────────────────────────────────────────
    dut.m_b_ready[master] = 1;
    dut.s_b_valid[slave]  = 1;
    dut.s_b_id[slave]     = expect_slave_id;
    dut.s_b_resp[slave]   = 0;

    int b_seen = 0;
    for (int i = 0; i < 64; ++i) {
        pre_edge();
        if (dut.m_b_valid[master] && dut.m_b_ready[master]) {
            if ((dut.m_b_id[master] & 0x7) != (master_id & 0x7)) {
                std::printf("FAIL B id strip at master %u: got 0x%x, expected 0x%x\n",
                            master, (unsigned)dut.m_b_id[master], master_id);
                return 1;
            }
            for (unsigned other_m = 0; other_m < 3; ++other_m) {
                if (other_m != master && dut.m_b_valid[other_m]) {
                    return fail("B leaked to wrong master");
                }
            }
            b_seen = 1;
            post_edge();
            break;
        }
        post_edge();
    }
    if (!b_seen) return fail("B handshake never observed at master");

    dut.s_b_valid[slave]  = 0;
    dut.m_b_ready[master] = 0;
    tick();
    return 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    if (do_write(0, 0x100, 0, 3, 0xDEAD0000u)) return 1;
    if (do_write(1, 0x200, 1, 5, 0xCAFE1111u)) return 1;
    if (do_write(2, 0x300, 2, 7, 0xB00B2222u)) return 1;
    if (do_write(0, 0x400, 3, 1, 0xFEED3333u)) return 1;
    if (do_write(2, 0x500, 0, 2, 0xABCD4444u)) return 1;

    std::printf("PASS Nic400FabricRs1 write 3x4: AW + W + B route through per-master reg slices across 5 (M,S) pairs\n");
    return 0;
}
