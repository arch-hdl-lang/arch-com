// AXI4→AXI3 long-burst splitter TB (READ path).
//
// DUT: Nic400Axi4ToAxi3 — splits a long AXI4 INCR read burst into ≤16-beat
// AXI3 sub-bursts and coalesces the AXI3 R streams back to one AXI4 R stream.
//
// The TB plays BOTH endpoints:
//   • AXI4 master : drives m_ar_*, accepts m_r_*.
//   • AXI3 slave  : accepts s_ar_* (capturing each sub-burst addr+len) and
//                   drives s_r_* (returns the right beat count per sub-burst
//                   with the AXI3 r_last on each sub-burst's final beat).
//
// Scenario: AXI4 read burst of 20 beats (ar_len=19), size=2, base=0x1000.
// Expect TWO AXI3 sub-bursts:
//   sub 0: addr=0x1000, ar_len=15  (16 beats)
//   sub 1: addr=0x1040, ar_len=3   ( 4 beats)   [0x1000 + 16*4]
// and one coalesced AXI4 R stream of 20 in-order beats, r_last ONLY on beat 20.
//
// Phasing matches the other NIC-400 TBs: sample combinational Mealy drives on
// pre_edge() (clk low), advance the slave's R data AFTER post_edge(), holding
// it stable across the clock edge.

#include "VNic400Axi4ToAxi3.h"
#include <cstdint>
#include <cstdio>

static VNic400Axi4ToAxi3 dut;
static uint64_t cycle = 0;

static void pre_edge()  { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }

static int fail(const char* m) {
    std::printf("FAIL %s (cycle=%llu)\n", m, (unsigned long long)cycle);
    return 1;
}

int main() {
    // ── Reset (active-low) ────────────────────────────────────────────────
    dut.rst = 0;
    dut.m_ar_valid = 0; dut.m_ar_addr = 0; dut.m_ar_id = 0; dut.m_ar_len = 0;
    dut.m_ar_size = 0;  dut.m_ar_burst = 0; dut.m_ar_lock = 0;
    dut.m_ar_cache = 0; dut.m_ar_prot = 0;
    dut.m_r_ready = 0;
    dut.s_ar_ready = 0;
    dut.s_r_valid = 0; dut.s_r_data = 0; dut.s_r_id = 0; dut.s_r_resp = 0;
    dut.s_r_last = 0;
    for (int i = 0; i < 4; ++i) { pre_edge(); post_edge(); }
    dut.rst = 1;
    for (int i = 0; i < 2; ++i) { pre_edge(); post_edge(); }

    // ── AXI4 master issues a 20-beat INCR read ────────────────────────────
    dut.m_ar_addr  = 0x00001000u;
    dut.m_ar_id    = 7;
    dut.m_ar_len   = 19;        // 20 beats
    dut.m_ar_size  = 2;         // 4 bytes/beat
    dut.m_ar_burst = 1;         // INCR
    dut.m_ar_valid = 1;
    dut.m_r_ready  = 1;         // master always ready to accept R
    dut.s_ar_ready = 1;         // slave always ready to accept AR

    // Sub-burst AR capture.
    int      sub_seen  = 0;
    uint32_t sub0_addr = 0, sub1_addr = 0;
    uint32_t sub0_len  = 0, sub1_len  = 0;

    // AXI3-slave R driver state.
    int      sb_active = 0;     // streaming a sub-burst's R beats
    uint32_t sb_len    = 0;     // active sub-burst ar_len (beats-1)
    uint32_t sb_i      = 0;     // beats emitted in active sub-burst
    uint32_t glob_i    = 0;     // global beat index == R data

    // AXI4-master R observer.
    uint32_t r_beats      = 0;  // coalesced beats accepted
    int      last_count   = 0;  // # of m_r_last=1 beats
    int      last_beat_ix = -1; // beat index where r_last fired
    int      data_ok      = 1;  // data == beat index everywhere

    for (int i = 0; i < 400 && (sub_seen < 2 || r_beats < 20); ++i) {
        // Drive the AXI3 R beat for the active sub-burst BEFORE the edge so
        // the DUT's combinational m_r_* settle on this pre_edge.
        if (sb_active) {
            dut.s_r_valid = 1;
            dut.s_r_data  = glob_i;
            dut.s_r_id    = 7;
            dut.s_r_resp  = 0;
            dut.s_r_last  = (sb_i == sb_len) ? 1 : 0;
        } else {
            dut.s_r_valid = 0;
            dut.s_r_last  = 0;
        }

        pre_edge();   // settle combinational outputs with current inputs

        // Drop AXI4 ar_valid only once the splitter has moved past the AR
        // accept state (it began issuing the first AXI3 sub-burst AR). Dropping
        // it the same pre_edge the accept handshake completes would clear the
        // S0→S1 transition condition before the post_edge that latches it.
        if (dut.s_ar_valid) dut.m_ar_valid = 0;

        // Capture a sub-burst AR the cycle the DUT presents it.
        if (dut.s_ar_valid && dut.s_ar_ready && !sb_active) {
            if (sub_seen == 0) { sub0_addr = dut.s_ar_addr; sub0_len = dut.s_ar_len; sub_seen = 1; }
            else if (sub_seen == 1) { sub1_addr = dut.s_ar_addr; sub1_len = dut.s_ar_len; sub_seen = 2; }
            sb_active = 1; sb_len = dut.s_ar_len; sb_i = 0;
        }

        // Observe the coalesced AXI4 R beat (Mealy, sampled pre-edge).
        bool m_fire = dut.m_r_valid && dut.m_r_ready;
        bool s_fire = dut.s_r_valid && dut.s_r_ready;
        if (m_fire && r_beats < 20) {
            if (dut.m_r_data != r_beats) data_ok = 0;
            if (dut.m_r_last) { last_count++; last_beat_ix = (int)r_beats; }
            r_beats++;
        }

        post_edge();  // clock the registers; hold slave data stable across it

        // Advance the AXI3 slave AFTER the edge for the beat just consumed.
        if (s_fire && sb_active) {
            glob_i++;
            if (sb_i == sb_len) sb_active = 0;   // sub-burst done; await next AR
            else sb_i++;
        }
    }

    // ── Assertions ────────────────────────────────────────────────────────
    if (sub_seen != 2) { std::printf("  sub_seen=%d\n", sub_seen); return fail("expected exactly 2 AXI3 sub-burst ARs"); }
    if (sub0_addr != 0x00001000u) { std::printf("  sub0_addr=0x%x\n", sub0_addr); return fail("sub0 addr (expected 0x1000)"); }
    if (sub0_len  != 15) { std::printf("  sub0_len=%u\n", sub0_len); return fail("sub0 ar_len (expected 15 = 16 beats)"); }
    if (sub1_addr != 0x00001040u) { std::printf("  sub1_addr=0x%x\n", sub1_addr); return fail("sub1 addr (expected 0x1040 = base + 16*4)"); }
    if (sub1_len  != 3) { std::printf("  sub1_len=%u\n", sub1_len); return fail("sub1 ar_len (expected 3 = 4 beats)"); }
    if (r_beats   != 20) { std::printf("  r_beats=%u\n", r_beats); return fail("coalesced AXI4 R beats (expected 20)"); }
    if (!data_ok) return fail("coalesced R data out of order (beat i != data i)");
    if (last_count != 1) { std::printf("  last_count=%d\n", last_count); return fail("m_r_last must assert exactly once"); }
    if (last_beat_ix != 19) { std::printf("  last_beat_ix=%d\n", last_beat_ix); return fail("m_r_last must fire on beat 19 (20th beat)"); }

    std::printf("PASS Nic400Axi4ToAxi3: 20-beat AXI4 read split into AXI3 16+4 sub-bursts "
                "(addrs 0x1000/0x1040, lens 15/3), coalesced to one 20-beat AXI4 R stream "
                "with single final RLAST on beat 20\n");
    return 0;
}
