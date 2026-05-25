// AHB-Lite ↔ AXI4 bridge smoke test.
//
// Single-beat read and write. The TB plays the AHB master AND the AXI slave,
// drives a NONSEQ address phase, and checks that:
//   • The bridge launches AR (or AW) with HSIZE/HBURST/HMASTLOCK forwarded.
//   • For reads: when the TB returns an R beat from the AXI side, the bridge
//     pulses HREADY=1 with HRDATA = the returned data.
//   • For writes: when the bridge sees HWDATA, it issues a W beat; the TB
//     captures it, returns a B response, and the bridge pulses HREADY=1
//     with HRESP from b_resp.
//
// As with the fabric write TB, we sample on pre_edge() so we observe Mealy
// drives BEFORE the lowered FSM advances past the handshake state.

#include "VNic400AhbBridge.h"
#include <cstdint>
#include <cstdio>

static VNic400AhbBridge dut;
static uint64_t cycle = 0;

static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }
static void pre_edge()  { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }

static void clear_inputs() {
    // AHB master-driven (TB drives, bridge sees IN).
    dut.h_hsel = 0; dut.h_haddr = 0; dut.h_hwrite = 0;
    dut.h_hsize = 0; dut.h_hburst = 0; dut.h_hprot = 0;
    dut.h_htrans = 0; dut.h_hmastlock = 0; dut.h_hwdata = 0;
    // AXI slave-driven (TB drives, bridge sees IN).
    dut.axi_ar_ready = 0;
    dut.axi_r_valid = 0; dut.axi_r_data = 0; dut.axi_r_id = 0;
    dut.axi_r_resp = 0; dut.axi_r_last = 0;
    dut.axi_aw_ready = 0;
    dut.axi_w_ready = 0;
    dut.axi_b_valid = 0; dut.axi_b_id = 0; dut.axi_b_resp = 0;
}

static int fail(const char* m) { std::printf("FAIL %s (cycle=%llu)\n", m, (unsigned long long)cycle); return 1; }

// Single-beat AHB read: TB drives NONSEQ+!HWRITE+addr, plays AXI slave.
// expect: bridge issues AR (we ack), then drives one R beat back from us, and
// pulses HREADY=1 with HRDATA. HSIZE forwarded to ar_size; HMASTLOCK to ar_lock.
static int do_read(uint32_t addr, unsigned hsize, unsigned hburst,
                   unsigned hprot, bool hmastlock, uint32_t rdata) {
    // Cycle 0: drive AHB address phase.
    dut.h_hsel = 1; dut.h_haddr = addr; dut.h_hwrite = 0;
    dut.h_hsize = hsize; dut.h_hburst = hburst; dut.h_hprot = hprot;
    dut.h_htrans = 2; dut.h_hmastlock = hmastlock ? 1 : 0;

    // Be ready to ack AR immediately.
    dut.axi_ar_ready = 1;

    int ar_seen = 0;
    for (int i = 0; i < 32 && !ar_seen; ++i) {
        pre_edge();
        if (dut.axi_ar_valid && dut.axi_ar_ready) {
            ar_seen = 1;
            if (dut.axi_ar_addr != addr) return fail("AR addr mismatch");
            if (dut.axi_ar_size != hsize) return fail("AR size mismatch");
            if ((unsigned)dut.axi_ar_lock != (hmastlock ? 1u : 0u)) return fail("AR lock mismatch");
            if (dut.axi_ar_cache != hprot) return fail("AR cache (=hprot) mismatch");
            if (dut.h_hready) return fail("HREADY should be low during AR stall");
        }
        post_edge();
    }
    if (!ar_seen) return fail("AR never observed");
    dut.axi_ar_ready = 0;

    // Drop AHB address phase signals; master moves to data phase / idle.
    dut.h_hsel = 0; dut.h_htrans = 0;

    // Drive the AXI R beat with the requested data.
    dut.axi_r_valid = 1; dut.axi_r_data = rdata; dut.axi_r_resp = 0; dut.axi_r_last = 1;

    int r_seen = 0;
    for (int i = 0; i < 32 && !r_seen; ++i) {
        pre_edge();
        if (dut.h_hready && (uint32_t)dut.h_hrdata == rdata && !dut.h_hresp) {
            r_seen = 1;
        }
        post_edge();
    }
    if (!r_seen) return fail("R beat never delivered to AHB master with correct data");
    dut.axi_r_valid = 0; dut.axi_r_data = 0; dut.axi_r_last = 0;
    tick();
    return 0;
}

// Single-beat AHB write: TB drives NONSEQ+HWRITE+addr, holds HWDATA next cycle,
// plays AXI slave (acks AW, captures W, returns B).
static int do_write(uint32_t addr, unsigned hsize, unsigned hburst,
                    unsigned hprot, bool hmastlock, uint32_t wdata, unsigned bresp) {
    dut.h_hsel = 1; dut.h_haddr = addr; dut.h_hwrite = 1;
    dut.h_hsize = hsize; dut.h_hburst = hburst; dut.h_hprot = hprot;
    dut.h_htrans = 2; dut.h_hmastlock = hmastlock ? 1 : 0;

    dut.axi_aw_ready = 1;
    dut.axi_w_ready  = 1;

    int aw_seen = 0;
    for (int i = 0; i < 32 && !aw_seen; ++i) {
        pre_edge();
        if (dut.axi_aw_valid && dut.axi_aw_ready) {
            aw_seen = 1;
            if (dut.axi_aw_addr != addr) return fail("AW addr mismatch");
            if (dut.axi_aw_size != hsize) return fail("AW size mismatch");
            if ((unsigned)dut.axi_aw_lock != (hmastlock ? 1u : 0u)) return fail("AW lock mismatch");
        }
        post_edge();
    }
    if (!aw_seen) return fail("AW never observed");
    dut.axi_aw_ready = 0;

    // AW done — bridge now pulses HREADY=1 to advance master into data phase.
    // Master places HWDATA on the bus the cycle after sampling HREADY=1, so
    // we model the same: drop HSEL/HTRANS now (no further AHB request) and
    // present HWDATA.
    dut.h_hsel = 0; dut.h_htrans = 0;
    dut.h_hwdata = wdata;

    int w_seen = 0;
    for (int i = 0; i < 32 && !w_seen; ++i) {
        pre_edge();
        if (dut.axi_w_valid && dut.axi_w_ready) {
            w_seen = 1;
            if (dut.axi_w_data != wdata) return fail("W data mismatch");
            if (!dut.axi_w_last) return fail("W last must be 1 for single-beat");
            if (dut.axi_w_strb != 0xF) return fail("W strb mismatch (expected 0xF for full-width)");
        }
        post_edge();
    }
    if (!w_seen) return fail("W never observed");
    dut.axi_w_ready = 0;
    dut.h_hwdata = 0;

    // Drive B response.
    dut.axi_b_valid = 1; dut.axi_b_resp = bresp;

    int b_to_ahb = 0;
    for (int i = 0; i < 32 && !b_to_ahb; ++i) {
        pre_edge();
        if (dut.h_hready) {
            // Expect HRESP to mirror bresp[1] (0/1 → 0 (OKAY), 2/3 → 1 (ERROR)).
            unsigned expect_hresp = (bresp >> 1) & 1;
            if (((unsigned)dut.h_hresp & 1) != expect_hresp) return fail("HRESP mismatch");
            b_to_ahb = 1;
        }
        post_edge();
    }
    if (!b_to_ahb) return fail("B response never propagated to AHB HREADY");
    dut.axi_b_valid = 0; dut.axi_b_resp = 0;
    tick();
    return 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // Read: hsize=2 (word), hburst=0 (SINGLE), hprot=0xB, hmastlock=0.
    if (do_read(0x1000, 2, 0, 0xB, false, 0xDEADBEEFu)) return 1;
    // Read with HMASTLOCK=1.
    if (do_read(0x2004, 2, 0, 0x3, true,  0xCAFE0001u)) return 1;
    // Write OKAY.
    if (do_write(0x3000, 2, 0, 0x7, false, 0xA5A5A5A5u, 0)) return 1;
    // Write SLVERR (bresp=2 → HRESP=1).
    if (do_write(0x4000, 2, 0, 0x7, false, 0x5A5A5A5Au, 2)) return 1;

    std::printf("PASS Nic400AhbBridge: 2 reads + 2 writes (1 OKAY, 1 SLVERR) round-trip\n");
    return 0;
}
