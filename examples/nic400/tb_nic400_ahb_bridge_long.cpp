// Multi-chunk INCR-undef tests for Nic400AhbBridge v4.
// Exercises bursts that span > 1 AXI chunk (MAX_INCR_BEATS=16, MAX_INCR_CHUNKS=4).
//   17 beats → 2 chunks (chunk 0 full + chunk 1: 1 real + 15 padded).
//   24 beats → 2 chunks (full + 8 real + 8 padded).
//   32 beats → 2 chunks (both full).
//   48 beats → 3 chunks.
//   64 beats → 4 chunks (max).
//   plus a 24-beat with SLVERR.
//
// Each scenario reset/sets up clean state. Critical: drive axi.b_valid as a
// LEVEL (high whenever pending B count > 0), not as a one-shot pulse — the
// bridge's chunk-N B-wait may not enter until several cycles after chunk-(N-1)
// completes, and pulsing b_valid races against that delay.

#include "VNic400AhbBridge.h"
#include <cstdint>
#include <cstdio>

#define CHUNK 16

static VNic400AhbBridge dut;
static uint64_t cycle = 0;
static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }
static void pre_edge() { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }

static void clear_inputs() {
    dut.h_hsel = 0; dut.h_haddr = 0; dut.h_hwrite = 0;
    dut.h_hsize = 0; dut.h_hburst = 0; dut.h_hprot = 0;
    dut.h_htrans = 0; dut.h_hmastlock = 0; dut.h_hwdata = 0;
    dut.axi_ar_ready = 0; dut.axi_r_valid = 0;
    dut.axi_aw_ready = 0; dut.axi_w_ready = 0;
    dut.axi_b_valid = 0; dut.axi_b_resp = 0;
}

static int run_burst(uint32_t addr_base, unsigned n_real, unsigned bresp,
                     const char* tag) {
    unsigned expect_chunks = (n_real + CHUNK - 1) / CHUNK;
    unsigned expect_beats  = expect_chunks * CHUNK;

    uint32_t data[64];
    for (unsigned i = 0; i < n_real; ++i) data[i] = 0xA0000000u | i;

    int addr_idx = 0, data_idx = -1;
    unsigned aw_count = 0, beats_received = 0, b_count = 0;
    uint32_t aw_addrs[8] = {0};
    uint32_t w_data_log[64] = {0};
    uint8_t  w_strb_log[64] = {0};
    int hresp_seen = -1;

    dut.axi_aw_ready = 1;
    dut.axi_w_ready  = 1;

    for (int c = 0; c < 512 && hresp_seen < 0; ++c) {
        if (addr_idx == 0) {
            dut.h_hsel = 1; dut.h_haddr = addr_base; dut.h_hwrite = 1;
            dut.h_hsize = 2; dut.h_hburst = 1; dut.h_htrans = 2;
        } else if (addr_idx < (int)n_real) {
            dut.h_hsel = 1; dut.h_haddr = addr_base + addr_idx * 4;
            dut.h_htrans = 3;
        } else {
            dut.h_hsel = 0; dut.h_htrans = 0;
        }
        dut.h_hwdata = (data_idx >= 0 && data_idx < (int)n_real)
                       ? data[data_idx] : 0xDEADBEEFu;
        // LEVEL b_valid: drive high whenever there's a pending B.
        dut.axi_b_valid = (aw_count > b_count) ? 1 : 0;
        dut.axi_b_resp  = bresp;

        pre_edge();
        int hready_now = dut.h_hready ? 1 : 0;

        if (dut.axi_aw_valid && dut.axi_aw_ready) {
            if (aw_count < 8) aw_addrs[aw_count] = (uint32_t)dut.axi_aw_addr;
            aw_count++;
        }
        if (dut.axi_w_valid && dut.axi_w_ready) {
            if (beats_received < 64) {
                w_data_log[beats_received] = (uint32_t)dut.axi_w_data;
                w_strb_log[beats_received] = (uint8_t)dut.axi_w_strb;
            }
            beats_received++;
        }
        if (dut.axi_b_valid && dut.axi_b_ready) b_count++;
        if (beats_received >= expect_beats && b_count >= expect_chunks && hready_now) {
            hresp_seen = (unsigned)dut.h_hresp & 1;
        }
        post_edge();
        if (hready_now) {
            if (data_idx >= 0) data_idx++;
            if (data_idx < 0) data_idx = 0;
            addr_idx++;
        }
    }

    if (hresp_seen < 0) {
        std::printf("FAIL [%s] stuck — aw=%u (exp %u), beats=%u (exp %u), b=%u\n",
                    tag, aw_count, expect_chunks, beats_received, expect_beats, b_count);
        return 1;
    }
    if (aw_count != expect_chunks) {
        std::printf("FAIL [%s] aw_count = %u, expected %u\n", tag, aw_count, expect_chunks);
        return 1;
    }
    if (beats_received != expect_beats) {
        std::printf("FAIL [%s] beats = %u, expected %u\n", tag, beats_received, expect_beats);
        return 1;
    }
    unsigned expect_hresp = (bresp >> 1) & 1;
    if ((unsigned)hresp_seen != expect_hresp) {
        std::printf("FAIL [%s] hresp = %d, expected %u\n", tag, hresp_seen, expect_hresp);
        return 1;
    }
    for (unsigned k = 0; k < expect_chunks; ++k) {
        uint32_t want = addr_base + k * CHUNK * 4;
        if (aw_addrs[k] != want) {
            std::printf("FAIL [%s] AW[%u] = 0x%x, expected 0x%x\n", tag, k, aw_addrs[k], want);
            return 1;
        }
    }
    for (unsigned i = 0; i < n_real; ++i) {
        if (w_strb_log[i] != 0xF || w_data_log[i] != data[i]) {
            std::printf("FAIL [%s] beat %u strb=0x%x data=0x%x (expected 0xF / 0x%x)\n",
                        tag, i, w_strb_log[i], w_data_log[i], data[i]);
            return 1;
        }
    }
    for (unsigned i = n_real; i < expect_beats; ++i) {
        if (w_strb_log[i] != 0) {
            std::printf("FAIL [%s] pad %u strb=0x%x expected 0\n", tag, i, w_strb_log[i]);
            return 1;
        }
    }

    // Drain phantom chunks + final HRESP before next test
    clear_inputs();
    for (int i = 0; i < 80; ++i) tick();
    std::printf("  OK [%s] %u-beat -> %u chunks, hresp=%u, cyc=%llu\n",
                tag, n_real, expect_chunks, expect_hresp, (unsigned long long)cycle);
    return 0;
}

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    if (run_burst(0x1000, 17, 0, "17-beat"))  return 1;
    if (run_burst(0x2000, 24, 0, "24-beat"))  return 1;
    if (run_burst(0x3000, 32, 0, "32-beat"))  return 1;
    if (run_burst(0x4000, 48, 0, "48-beat 3 chunks")) return 1;
    if (run_burst(0x5000, 64, 0, "64-beat 4 chunks (max)")) return 1;
    if (run_burst(0x6000, 24, 2, "24-beat SLVERR")) return 1;

    std::printf("PASS Nic400AhbBridge v4 long INCR: 17/24/32/48/64-beat + SLVERR\n");
    return 0;
}
