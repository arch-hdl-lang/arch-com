// Multi-beat AHB-Lite write burst test for the Nic400AhbBridge v2.
//
// Models the AHB master pipeline (addr_idx, data_idx) and drives HSEL/
// HTRANS/HADDR/HWDATA accordingly each cycle, advancing on HREADY=1 sampled
// at the end of the cycle. Plays the AXI slave too: acks AW, captures W
// beats, returns a B response after the last W beat.
//
// Coverage:
//   • INCR4 (4-beat burst): no backpressure. Verifies axlen=3, ascending
//     HWDATA streams as ascending AXI W beats, w_last on the 4th.
//   • INCR4 with mid-burst AXI backpressure (w_ready=0 for 2 cycles in the
//     middle): verifies that the AHB master gets stalled (HWDATA stays
//     stable) and resumes correctly.
//   • INCR8 (8-beat) clean: verifies axlen=7.
//
// Same pre_edge/post_edge split-tick pattern as the other TBs so we observe
// Mealy drives before the lowered FSM advances past them.

#include "VNic400AhbBridge.h"
#include <cstdint>
#include <cstdio>

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

// Drive AHB master signals given the current pipeline indices.
//   addr_idx: index of the beat currently in the address phase (0..n_beats);
//             == n_beats means "no more addr phases to drive" (IDLE).
//   data_idx: index of the beat currently in the data phase (-1 before first
//             advance, then 0..n_beats-1 during streaming, == n_beats after
//             last beat).
static void drive_master(uint32_t addr_base, unsigned hburst, unsigned hsize,
                         const uint32_t* data, unsigned n_beats,
                         int addr_idx, int data_idx) {
    if (addr_idx < (int)n_beats) {
        dut.h_hsel    = 1;
        dut.h_haddr   = addr_base + addr_idx * 4;
        dut.h_hwrite  = 1;
        dut.h_hsize   = hsize;
        dut.h_hburst  = hburst;
        dut.h_hprot   = 0;
        dut.h_htrans  = (addr_idx == 0) ? 2 : 3;   // NONSEQ first, then SEQ
        dut.h_hmastlock = 0;
    } else {
        dut.h_hsel    = 0;
        dut.h_htrans  = 0;
    }
    dut.h_hwdata = (data_idx >= 0 && data_idx < (int)n_beats) ? data[data_idx] : 0;
}

// Run a multi-beat write burst. `backpressure_at_beat` (-1 to disable) holds
// axi.w_ready=0 for `backpressure_cycles` cycles when the bridge is in the
// W loop for that beat index. Returns 0 on success.
static int do_write_burst(uint32_t addr_base, unsigned hburst, unsigned hsize,
                          const uint32_t* data, unsigned n_beats,
                          unsigned expect_axlen, unsigned bresp,
                          int backpressure_at_beat, int backpressure_cycles) {
    int addr_idx = 0;
    int data_idx = -1;
    int aw_acked = 0;
    unsigned beats_received = 0;
    int w_last_seen_on = -1;
    uint32_t w_data_log[32];
    int b_phase_done = 0;
    int bp_remaining = 0;

    dut.axi_aw_ready = 1;
    dut.axi_w_ready  = 1;

    for (int c = 0; c < 256 && !b_phase_done; ++c) {
        drive_master(addr_base, hburst, hsize, data, n_beats, addr_idx, data_idx);

        // Backpressure controller: if we're at the target beat and w_valid is
        // about to be asserted, drop w_ready for the configured cycle count.
        if (backpressure_at_beat >= 0 && (int)beats_received == backpressure_at_beat && bp_remaining == 0 && backpressure_cycles > 0) {
            // Trigger only once.
            bp_remaining = backpressure_cycles;
            backpressure_cycles = 0;
        }
        if (bp_remaining > 0) {
            dut.axi_w_ready = 0;
        } else {
            dut.axi_w_ready = 1;
        }

        // Drive B after the last W beat is sent (so the bridge picks it up in
        // the B-wait state on a later cycle).
        if (beats_received == n_beats && !dut.axi_b_valid) {
            dut.axi_b_valid = 1;
            dut.axi_b_resp  = bresp;
        }

        pre_edge();

        // Sample bridge outputs (pre-edge: comb settled with current state).
        int hready_now = dut.h_hready ? 1 : 0;

        if (!aw_acked && dut.axi_aw_valid && dut.axi_aw_ready) {
            aw_acked = 1;
            if (dut.axi_aw_addr != addr_base) return fail("AW addr mismatch");
            if (dut.axi_aw_len  != expect_axlen) return fail("AW len mismatch");
            if (dut.axi_aw_size != hsize) return fail("AW size mismatch");
        }
        if (dut.axi_w_valid && dut.axi_w_ready) {
            if (beats_received >= 32) return fail("more W beats than expected");
            w_data_log[beats_received] = (uint32_t)dut.axi_w_data;
            if (dut.axi_w_last) w_last_seen_on = (int)beats_received;
            beats_received++;
        }
        if (beats_received == n_beats && dut.axi_b_valid && hready_now) {
            // Bridge has pulsed hready in B phase — check HRESP.
            unsigned expect_hresp = (bresp >> 1) & 1;
            if ((unsigned)dut.h_hresp != expect_hresp) return fail("HRESP mismatch");
            b_phase_done = 1;
        }

        post_edge();

        // Advance AHB pipeline based on HREADY observed during the cycle.
        if (hready_now) {
            if (data_idx >= 0) data_idx++;
            if (addr_idx < (int)n_beats) {
                if (data_idx < 0) data_idx = 0;
                addr_idx++;
            }
        }
        if (bp_remaining > 0) bp_remaining--;
    }

    if (!b_phase_done) return fail("burst never completed");
    if (beats_received != n_beats) return fail("wrong number of W beats");
    if (w_last_seen_on != (int)n_beats - 1) {
        std::printf("FAIL w_last fired on beat %d, expected %u\n", w_last_seen_on, n_beats - 1);
        return 1;
    }
    for (unsigned i = 0; i < n_beats; ++i) {
        if (w_data_log[i] != data[i]) {
            std::printf("FAIL W beat %u: got 0x%x, expected 0x%x\n", i, w_data_log[i], data[i]);
            return 1;
        }
    }

    // Cleanup.
    dut.axi_b_valid = 0; dut.axi_w_ready = 0; dut.axi_aw_ready = 0;
    dut.h_hsel = 0; dut.h_htrans = 0; dut.h_hwdata = 0;
    tick();
    return 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // INCR4 clean (no backpressure). HBURST=3 → axlen=3 (4 beats).
    {
        uint32_t data[4] = { 0x11111111u, 0x22222222u, 0x33333333u, 0x44444444u };
        if (do_write_burst(0x1000, 3, 2, data, 4, 3, 0, -1, 0)) return 1;
        std::printf("  OK INCR4 clean\n");
    }

    // INCR4 with backpressure at beat 1 (w_ready=0 for 2 cycles).
    {
        uint32_t data[4] = { 0xAAAA0000u, 0xAAAA1111u, 0xAAAA2222u, 0xAAAA3333u };
        if (do_write_burst(0x2000, 3, 2, data, 4, 3, 0, 1, 2)) return 1;
        std::printf("  OK INCR4 with backpressure at beat 1 (2 cyc)\n");
    }

    // INCR8 clean (HBURST=5 → axlen=7, 8 beats).
    {
        uint32_t data[8];
        for (int i = 0; i < 8; ++i) data[i] = 0xC0DE0000u | i;
        if (do_write_burst(0x3000, 5, 2, data, 8, 7, 0, -1, 0)) return 1;
        std::printf("  OK INCR8 clean\n");
    }

    // INCR4 with SLVERR.
    {
        uint32_t data[4] = { 0xBEEF0000u, 0xBEEF1111u, 0xBEEF2222u, 0xBEEF3333u };
        if (do_write_burst(0x4000, 3, 2, data, 4, 3, 2, -1, 0)) return 1;
        std::printf("  OK INCR4 SLVERR\n");
    }

    std::printf("PASS Nic400AhbBridge burst: INCR4 + backpressure + INCR8 + SLVERR\n");
    return 0;
}
