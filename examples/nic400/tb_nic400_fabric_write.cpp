// 3x4 write-path smoke test for the Nic400Fabric.
//
// Covers the AW → W → B path on the new 3×4 R/W fabric. Strategy: drive
// AW, W, and slave-side ready/b_valid/b_id all at once (AXI4 permits W
// to race AR/AW), and observe the master-side B handshake as the proof
// that the AW + W phases completed correctly. Direct observation of the
// AW/W mid-cycle is awkward because the Mealy thread fuses the handshake
// + transition into a single eval; the TB sees post-handshake state by
// the time it polls.
//
// Per-handshake checks:
//   1. s.aw_id has the master_idx prefix in the high MIDX_W bits.
//   2. s.w_data is the value the master drove.
//   3. m.b_id has the prefix stripped (returns the original master_id).
//   4. No other slave's s.aw_valid or master's m.b_valid was asserted.

#include "VNic400Fabric.h"
#include <cstdint>
#include <cstdio>

static VNic400Fabric dut;
static uint64_t cycle = 0;

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle++;
}

// Pre-edge sample: settle comb with clk=0 so the TB can observe Mealy-style
// handshake signals (s.aw_valid && s.aw_ready etc.) BEFORE the rising edge
// advances state into the next phase. Pair with post_edge() to complete one
// full clock cycle.
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

// Issue one single-beat write from `master` to `slave` (address chooses slave
// via address bits [REGION_BITS+NS_W-1:REGION_BITS]). The slave-side b_id
// must come back with the master-idx prefix; the master must see b_id with
// the prefix stripped.
//
// Drive AW, W, and slave-side ready/b_valid all together. Track which side
// of the handshake we've observed via flags. When the master sees its B
// handshake, the entire AW → W → B sequence has completed.
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

    int aw_seen = 0, w_seen = 0;
    int aw_id_correct = -1;
    int w_data_correct = -1;

    // Split tick into pre-edge sample / post-edge advance so we can observe
    // AW/W handshakes BEFORE the rising edge fuses them away into next-state.
    for (int i = 0; i < 32; ++i) {
        pre_edge();   // comb settles with current state; handshake visible here
        if (!aw_seen && dut.s_aw_valid[slave] && dut.s_aw_ready[slave]) {
            aw_seen = 1;
            aw_id_correct = (dut.s_aw_id[slave] == expect_slave_id) ? 1 : 0;
            for (unsigned other_s = 0; other_s < 4; ++other_s) {
                if (other_s != slave && dut.s_aw_valid[other_s]) {
                    return fail("AW leaked to wrong slave");
                }
            }
        }
        if (!w_seen && dut.s_w_valid[slave] && dut.s_w_ready[slave]) {
            w_seen = 1;
            w_data_correct = (dut.s_w_data[slave] == data && dut.s_w_last[slave]) ? 1 : 0;
        }
        post_edge();  // rising edge advances state
        if (aw_seen && w_seen) {
            dut.m_aw_valid[master] = 0;
            dut.m_w_valid[master]  = 0;
            dut.s_aw_ready[slave]  = 0;
            dut.s_w_ready[slave]   = 0;
            break;
        }
    }
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
    for (int i = 0; i < 32; ++i) {
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

    std::printf("PASS Nic400Fabric write 3x4: AW + W + B routes correctly across 5 (M,S) pairs\n");
    return 0;
}
