// AXI3→AXI4 limiter TB (READ path).
//
// DUT: Nic400Axi3ToAxi4 — forwards an AXI3 read burst onto an AXI4 slave,
// chopping the onward AxLEN to the programmable cap MAX_BURST (default 16). At
// MAX_BURST=16 an AXI3 burst (max 16 beats) forwards as ONE AXI4 sub-burst.
//
// The TB plays the AXI3 master (drives m_ar_*, accepts m_r_*) and the AXI4
// slave (accepts s_ar_*, drives s_r_*). Scenario: AXI3 16-beat INCR read
// (ar_len=15), size=2, base=0x2000, ar_lock=1 (EXCLUSIVE). Expect:
//   • exactly ONE AXI4 AR, addr=0x2000, ar_len=15 (forwarded unchanged),
//     ar_lock=1 (AXI3 EXCLUSIVE→AXI4 1), ar_qos=0, ar_region=0 (AXI3 has none).
//   • 16 coalesced AXI4→AXI3 R beats in order, RLAST once on the 16th beat.
//
// pre_edge sampling per the NIC-400 TB convention.

#include "VNic400Axi3ToAxi4.h"
#include <cstdint>
#include <cstdio>

static VNic400Axi3ToAxi4 dut;
static uint64_t cycle = 0;
static void pre_edge()  { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }
static int fail(const char* m) { std::printf("FAIL %s (cycle=%llu)\n", m, (unsigned long long)cycle); return 1; }

int main() {
    dut.rst = 0;
    dut.m_ar_valid = 0; dut.m_ar_addr = 0; dut.m_ar_id = 0; dut.m_ar_len = 0;
    dut.m_ar_size = 0;  dut.m_ar_burst = 0; dut.m_ar_lock = 0;
    dut.m_ar_cache = 0; dut.m_ar_prot = 0;
    dut.m_r_ready = 0;
    dut.s_ar_ready = 0;
    dut.s_r_valid = 0; dut.s_r_data = 0; dut.s_r_id = 0; dut.s_r_resp = 0; dut.s_r_last = 0;
    for (int i = 0; i < 4; ++i) { pre_edge(); post_edge(); }
    dut.rst = 1;
    for (int i = 0; i < 2; ++i) { pre_edge(); post_edge(); }

    // AXI3 master issues a 16-beat INCR read, EXCLUSIVE lock.
    dut.m_ar_addr  = 0x00002000u;
    dut.m_ar_id    = 5;
    dut.m_ar_len   = 15;        // AXI3 4-bit: 16 beats
    dut.m_ar_size  = 2;
    dut.m_ar_burst = 1;         // INCR
    dut.m_ar_lock  = 1;         // AXI3 EXCLUSIVE (2'b01)
    dut.m_ar_valid = 1;
    dut.m_r_ready  = 1;
    dut.s_ar_ready = 1;

    int      ar_seen   = 0;
    uint32_t s_addr    = 0;
    uint32_t s_len     = 0;
    uint32_t s_lock    = 0;
    uint32_t s_qos     = 0;
    uint32_t s_region  = 0;

    int      sb_active = 0;
    uint32_t sb_len    = 0, sb_i = 0, glob = 0;

    uint32_t r_beats = 0;
    int      last_count = 0, last_beat_ix = -1, data_ok = 1;

    for (int i = 0; i < 200 && (ar_seen < 1 || r_beats < 16); ++i) {
        if (sb_active) {
            dut.s_r_valid = 1; dut.s_r_data = glob; dut.s_r_id = 5;
            dut.s_r_resp = 0; dut.s_r_last = (sb_i == sb_len) ? 1 : 0;
        } else { dut.s_r_valid = 0; dut.s_r_last = 0; }

        pre_edge();

        if (dut.s_ar_valid) dut.m_ar_valid = 0;

        if (dut.s_ar_valid && dut.s_ar_ready && !ar_seen) {
            s_addr = dut.s_ar_addr; s_len = dut.s_ar_len; s_lock = dut.s_ar_lock;
            s_qos = dut.s_ar_qos;   s_region = dut.s_ar_region;
            ar_seen = 1; sb_active = 1; sb_len = dut.s_ar_len; sb_i = 0;
        } else if (dut.s_ar_valid && dut.s_ar_ready && ar_seen && !sb_active) {
            // a second AXI4 AR would mean the limiter split a ≤16-beat burst
            return fail("limiter issued a second AXI4 sub-burst for a 16-beat AXI3 read");
        }

        bool m_fire = dut.m_r_valid && dut.m_r_ready;
        bool s_fire = dut.s_r_valid && dut.s_r_ready;
        if (m_fire && r_beats < 16) {
            if (dut.m_r_data != r_beats) data_ok = 0;
            if (dut.m_r_last) { last_count++; last_beat_ix = (int)r_beats; }
            r_beats++;
        }

        post_edge();

        if (s_fire && sb_active) { glob++; if (sb_i == sb_len) sb_active = 0; else sb_i++; }
    }

    if (!ar_seen) return fail("no AXI4 AR issued");
    if (s_addr != 0x00002000u) { std::printf("  s_addr=0x%x\n", s_addr); return fail("AXI4 AR addr (expected 0x2000)"); }
    if (s_len  != 15) { std::printf("  s_len=%u\n", s_len); return fail("AXI4 AR len (expected 15 — forwarded unchanged at MAX_BURST=16)"); }
    if (s_lock != 1)  { std::printf("  s_lock=%u\n", s_lock); return fail("AXI4 AR lock (expected 1 — AXI3 EXCLUSIVE→AXI4)"); }
    if (s_qos != 0)   { std::printf("  s_qos=%u\n", s_qos); return fail("AXI4 AR qos (expected 0 — AXI3 has none)"); }
    if (s_region != 0){ std::printf("  s_region=%u\n", s_region); return fail("AXI4 AR region (expected 0 — AXI3 has none)"); }
    if (r_beats != 16){ std::printf("  r_beats=%u\n", r_beats); return fail("coalesced R beats (expected 16)"); }
    if (!data_ok) return fail("R data out of order");
    if (last_count != 1) { std::printf("  last_count=%d\n", last_count); return fail("RLAST must fire exactly once"); }
    if (last_beat_ix != 15) { std::printf("  last_beat_ix=%d\n", last_beat_ix); return fail("RLAST must fire on beat 15 (16th beat)"); }

    std::printf("PASS Nic400Axi3ToAxi4: AXI3 16-beat EXCLUSIVE read forwarded as ONE AXI4 AR "
                "(addr 0x2000, len 15, lock 1, qos/region 0), 16 R beats in order with single RLAST\n");
    return 0;
}
