// HBURST=INCR (undefined-length) chunked-burst test for the Nic400AhbBridge
// v3 chunked-burst path.
//
// MAX_INCR_BEATS = 16 (compile-time param). The bridge speculatively issues
// an AXI burst of axlen=15 and forwards master HWDATA as W beats. When the
// master signals end-of-burst by transitioning HTRANS to IDLE, the bridge
// pads remaining W beats with w_strb=0 (sparse-write).
//
// Coverage:
//   • INCR with 1 SEQ then IDLE: just 1 real beat (D0), 15 padded.
//   • INCR with 4 SEQ beats: 4 real (D0..D3), 12 padded.
//   • INCR with 16 SEQ beats: full chunk, no padding.
//
// The TB models the AHB master pipeline (addr_idx + data_idx) and counts
// "intended beats". After the last intended beat, master drives HTRANS=IDLE.
//
// Bridge-side acceptance criteria:
//   • Exactly MAX_INCR_BEATS = 16 W beats arrive on AXI (any combination of
//     real + padded), w_last on the 16th.
//   • Real beats have w_strb = 0xF and w_data = the intended D_i.
//   • Padded beats have w_strb = 0 (sparse). Data is don't-care.
//   • B response propagates back as HREADY=1 + HRESP pulse to AHB.

#include "VNic400AhbBridge.h"
#include <cstdint>
#include <cstdio>

#define MAX_BEATS 16

static VNic400AhbBridge dut;
static uint64_t cycle = 0;

static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }
static void pre_edge()  { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }

static void clear_inputs() {
    dut.h_hsel = 0; dut.h_haddr = 0; dut.h_hwrite = 0;
    dut.h_hsize = 0; dut.h_hburst = 0; dut.h_hprot = 0;
    dut.h_htrans = 0; dut.h_hmastlock = 0; dut.h_hwdata = 0;
    dut.axi_ar_ready = 0;
    dut.axi_r_valid = 0; dut.axi_r_data = 0; dut.axi_r_id = 0;
    dut.axi_r_resp = 0; dut.axi_r_last = 0;
    dut.axi_aw_ready = 0; dut.axi_w_ready = 0;
    dut.axi_b_valid = 0; dut.axi_b_id = 0; dut.axi_b_resp = 0;
}

static int fail(const char* m) { std::printf("FAIL %s (cycle=%llu)\n", m, (unsigned long long)cycle); return 1; }

// Drive AHB master for an INCR (HBURST=1) burst with `n_real` SEQ beats then
// IDLE. addr_idx tracks the master's next addr-phase beat index; data_idx
// tracks the data-phase beat index.
//   addr_idx in [0..n_real]   → drive NONSEQ or SEQ for that beat
//   addr_idx == n_real + 1+   → drive IDLE (master is done)
//   data_idx in [0..n_real-1] → HWDATA = data[data_idx]
//   data_idx ≥ n_real         → HWDATA don't care (master in IDLE data phase)
static void drive_master_incr(uint32_t addr_base, unsigned hsize,
                              const uint32_t* data, unsigned n_real,
                              int addr_idx, int data_idx) {
    if (addr_idx == 0) {
        dut.h_hsel = 1; dut.h_haddr = addr_base; dut.h_hwrite = 1;
        dut.h_hsize = hsize; dut.h_hburst = 1; dut.h_hprot = 0;
        dut.h_htrans = 2;       // NONSEQ
        dut.h_hmastlock = 0;
    } else if (addr_idx < (int)n_real) {
        dut.h_hsel = 1; dut.h_haddr = addr_base + addr_idx * 4; dut.h_hwrite = 1;
        dut.h_hsize = hsize; dut.h_hburst = 1; dut.h_hprot = 0;
        dut.h_htrans = 3;       // SEQ
        dut.h_hmastlock = 0;
    } else {
        // Past last intended addr phase — master is done.
        dut.h_hsel = 0; dut.h_htrans = 0;
    }
    if (data_idx >= 0 && data_idx < (int)n_real) {
        dut.h_hwdata = data[data_idx];
    } else {
        dut.h_hwdata = 0xDEADDEADu;   // would-be undef
    }
}

static int do_incr_burst(uint32_t addr_base, unsigned hsize,
                         const uint32_t* data, unsigned n_real, unsigned bresp) {
    int addr_idx = 0;
    int data_idx = -1;
    int aw_acked = 0;
    unsigned beats_received = 0;
    int w_last_seen_on = -1;
    uint32_t w_data_log[MAX_BEATS];
    uint8_t  w_strb_log[MAX_BEATS];
    int b_phase_done = 0;

    dut.axi_aw_ready = 1;
    dut.axi_w_ready  = 1;

    for (int c = 0; c < 512 && !b_phase_done; ++c) {
        drive_master_incr(addr_base, hsize, data, n_real, addr_idx, data_idx);

        if (beats_received == MAX_BEATS && !dut.axi_b_valid) {
            dut.axi_b_valid = 1;
            dut.axi_b_resp  = bresp;
        }

        pre_edge();
        int hready_now = dut.h_hready ? 1 : 0;

        if (!aw_acked && dut.axi_aw_valid && dut.axi_aw_ready) {
            aw_acked = 1;
            if (dut.axi_aw_addr != addr_base) return fail("AW addr mismatch");
            if (dut.axi_aw_len  != MAX_BEATS - 1) return fail("AW len != MAX-1");
            if (dut.axi_aw_burst != 1) return fail("AW burst != INCR");
        }
        if (dut.axi_w_valid && dut.axi_w_ready) {
            if (beats_received >= MAX_BEATS) return fail("more W beats than MAX_BEATS");
            w_data_log[beats_received] = (uint32_t)dut.axi_w_data;
            w_strb_log[beats_received] = (uint8_t) dut.axi_w_strb;
            if (dut.axi_w_last) w_last_seen_on = (int)beats_received;
            beats_received++;
        }
        if (beats_received == MAX_BEATS && dut.axi_b_valid && hready_now) {
            unsigned expect_hresp = (bresp >> 1) & 1;
            if ((unsigned)dut.h_hresp != expect_hresp) return fail("HRESP mismatch");
            b_phase_done = 1;
        }

        post_edge();

        if (hready_now) {
            if (data_idx >= 0) data_idx++;
            // addr_idx advances as long as the master is still presenting
            // addr-phase beats. After addr_idx reaches n_real, addr phase
            // transitions to IDLE — done.
            if (data_idx < 0) data_idx = 0;
            addr_idx++;
        }
    }

    if (!b_phase_done) return fail("INCR burst never completed");
    if (beats_received != MAX_BEATS) return fail("not MAX_BEATS W beats received");
    if (w_last_seen_on != MAX_BEATS - 1) return fail("w_last not on the final beat");

    // Validate beat shape:
    //   beats [0..n_real-1] must have strb=0xF and data=intended.
    //   beats [n_real..MAX-1] must have strb=0 (padded).
    for (unsigned i = 0; i < n_real; ++i) {
        if (w_strb_log[i] != 0xF) {
            std::printf("FAIL real beat %u: strb=0x%x, expected 0xF\n", i, w_strb_log[i]);
            return 1;
        }
        if (w_data_log[i] != data[i]) {
            std::printf("FAIL real beat %u: data=0x%x, expected 0x%x\n", i, w_data_log[i], data[i]);
            return 1;
        }
    }
    for (unsigned i = n_real; i < MAX_BEATS; ++i) {
        if (w_strb_log[i] != 0) {
            std::printf("FAIL pad beat %u: strb=0x%x, expected 0\n", i, w_strb_log[i]);
            return 1;
        }
    }

    dut.axi_b_valid = 0; dut.axi_w_ready = 0; dut.axi_aw_ready = 0;
    dut.h_hsel = 0; dut.h_htrans = 0;
    tick();
    return 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // 4-beat INCR-undef: 4 real, 12 padded.
    {
        uint32_t data[4] = { 0x10101010u, 0x20202020u, 0x30303030u, 0x40404040u };
        if (do_incr_burst(0x1000, 2, data, 4, 0)) return 1;
        std::printf("  OK INCR-undef 4 beats (12 padded)\n");
    }

    // 1-beat INCR (just NONSEQ then IDLE — degenerate case).
    {
        uint32_t data[1] = { 0xFFEE0001u };
        if (do_incr_burst(0x2000, 2, data, 1, 0)) return 1;
        std::printf("  OK INCR-undef 1 beat (15 padded)\n");
    }

    // 16-beat INCR (full chunk, no padding).
    {
        uint32_t data[16];
        for (int i = 0; i < 16; ++i) data[i] = 0xC0DE0000u | i;
        if (do_incr_burst(0x3000, 2, data, 16, 0)) return 1;
        std::printf("  OK INCR-undef 16 beats (no padding)\n");
    }

    // 4-beat INCR with SLVERR.
    {
        uint32_t data[4] = { 0xBEEFAAAAu, 0xBEEFBBBBu, 0xBEEFCCCCu, 0xBEEFDDDDu };
        if (do_incr_burst(0x4000, 2, data, 4, 2)) return 1;
        std::printf("  OK INCR-undef 4 beats SLVERR\n");
    }

    std::printf("PASS Nic400AhbBridge INCR-undef: 1/4/16-beat + SLVERR\n");
    return 0;
}
