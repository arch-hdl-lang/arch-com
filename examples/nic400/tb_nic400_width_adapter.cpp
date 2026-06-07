// AXI4 width-adapter TB. Default ratio is 64→32 (RATIO=2).
//
// The TB plays both the wide AXI4 master (driving the adapter's `m` target
// port) and the narrow AXI4 slave (acking the adapter's `s` initiator
// port). Six scenarios:
//   1) Single-beat write — verify 1 master W (64b) splits into 2 slave W
//      beats (low half first).
//   2) Single-beat read — verify 2 slave R beats pack into 1 master R
//      beat (low half from first slave beat, high half from second), and
//      r_last fires exactly once on the master.
//   3) 4-beat INCR write (master axlen=3 → slave axlen=7).
//   4) 4-beat INCR read (master axlen=3 → slave axlen=7). This is the
//      scenario that hit compiler bug #422 before PR #430 landed.
//   5) Write with non-trivial w_strb — master strb=0xA5 splits to
//      [0xA, 0x5] on the two slave beats (little-endian).
//   6) SLVERR propagation — slave returns SLVERR (resp=2) on one R
//      sub-beat; master r_resp must surface SLVERR for the containing beat.
//
// Like the other NIC-400 TBs, we sample on pre_edge() (clk low) so we see
// Mealy combinational drives BEFORE the lowered FSM advances past the
// handshake state.

#include "VNic400WidthAdapter.h"
#include <cstdint>
#include <cstdio>

static VNic400WidthAdapter dut;
static uint64_t cycle = 0;

static void tick()      { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }
static void pre_edge()  { dut.clk = 0; dut.eval(); }
static void post_edge() { dut.clk = 1; dut.eval(); cycle++; }

static void clear_inputs() {
    // Master side (TB drives master-style requests INTO adapter).
    dut.m_ar_valid = 0; dut.m_ar_addr = 0; dut.m_ar_id = 0; dut.m_ar_len = 0;
    dut.m_ar_size = 0;  dut.m_ar_burst = 0; dut.m_ar_lock = 0;
    dut.m_ar_cache = 0; dut.m_ar_prot = 0;  dut.m_ar_qos = 0; dut.m_ar_region = 0;
    dut.m_r_ready = 0;
    dut.m_aw_valid = 0; dut.m_aw_addr = 0; dut.m_aw_id = 0; dut.m_aw_len = 0;
    dut.m_aw_size = 0;  dut.m_aw_burst = 0; dut.m_aw_lock = 0;
    dut.m_aw_cache = 0; dut.m_aw_prot = 0;  dut.m_aw_qos = 0; dut.m_aw_region = 0;
    dut.m_w_valid = 0; dut.m_w_data = 0; dut.m_w_strb = 0; dut.m_w_last = 0;
    dut.m_b_ready = 0;
    // Slave side (TB plays the downstream slave — drives ready/response).
    dut.s_ar_ready = 0;
    dut.s_r_valid = 0; dut.s_r_data = 0; dut.s_r_id = 0; dut.s_r_resp = 0; dut.s_r_last = 0;
    dut.s_aw_ready = 0;
    dut.s_w_ready = 0;
    dut.s_b_valid = 0; dut.s_b_id = 0; dut.s_b_resp = 0;
}

static int fail(const char* m) {
    std::printf("FAIL %s (cycle=%llu)\n", m, (unsigned long long)cycle);
    return 1;
}

// Wait for a condition (sampled on pre_edge) for up to `limit` cycles.
// Returns 0 if matched; -1 on timeout.
template <typename F>
static int wait_until(F cond, int limit, const char* what) {
    for (int i = 0; i < limit; ++i) {
        pre_edge();
        if (cond()) return 0;
        post_edge();
    }
    (void)what;
    return -1;
}

// ── Scenario 1: single-beat write ─────────────────────────────────────
// master axlen=0 ⇒ 1 master W beat (64b) ⇒ slave axlen=1, 2 slave W beats.
// The low 32 bits of the master beat go out FIRST.
static int test_single_write() {
    const uint64_t wdata = 0xCAFEBABE12345678ULL;
    const uint32_t exp_lo = 0x12345678u;
    const uint32_t exp_hi = 0xCAFEBABEu;
    const uint32_t addr = 0x1000;

    // Drive AW.
    dut.m_aw_valid = 1; dut.m_aw_addr = addr; dut.m_aw_id = 0; dut.m_aw_len = 0;
    dut.m_aw_size = 3;  dut.m_aw_burst = 1;  // size=3 means 8 bytes (matches M_DATA_W=64)
    dut.s_aw_ready = 1;

    int aw_seen = 0; uint32_t s_aw_len = 0xff; uint32_t s_aw_size = 0xff;
    for (int i = 0; i < 32 && !aw_seen; ++i) {
        pre_edge();
        if (dut.s_aw_valid && dut.s_aw_ready) {
            aw_seen = 1; s_aw_len = dut.s_aw_len; s_aw_size = dut.s_aw_size;
            if (dut.s_aw_addr != addr) return fail("s1: slave AW addr");
        }
        post_edge();
    }
    if (!aw_seen) return fail("s1: slave AW never observed");
    if (s_aw_len != 1) return fail("s1: slave AW len (expect 1 for ratio=2)");
    if (s_aw_size != 2) return fail("s1: slave AW size (expect 2 for S_DATA_W=32)");
    dut.m_aw_valid = 0; dut.s_aw_ready = 0;

    // Drive the wide W beat.
    dut.m_w_valid = 1; dut.m_w_data = wdata; dut.m_w_strb = 0xFF; dut.m_w_last = 1;
    dut.s_w_ready = 1;

    // Expect first slave beat: low half, w_last=0, w_strb=0xF.
    int beat = 0; uint32_t got_lo = 0, got_hi = 0;
    for (int i = 0; i < 32 && beat < 2; ++i) {
        pre_edge();
        if (dut.s_w_valid && dut.s_w_ready) {
            if (beat == 0) {
                got_lo = dut.s_w_data;
                if (dut.s_w_last) return fail("s1: slave w_last should be 0 on beat 0");
                if (dut.s_w_strb != 0xF) return fail("s1: slave w_strb beat 0");
                if (dut.m_w_ready) return fail("s1: m_w_ready must stay LOW on beat 0 (sub-beat)");
            } else if (beat == 1) {
                got_hi = dut.s_w_data;
                if (!dut.s_w_last) return fail("s1: slave w_last should be 1 on beat 1");
                if (dut.s_w_strb != 0xF) return fail("s1: slave w_strb beat 1");
                if (!dut.m_w_ready) return fail("s1: m_w_ready must be HIGH on last sub-beat");
            }
            beat++;
        }
        post_edge();
    }
    if (beat != 2) return fail("s1: did not see 2 slave W beats");
    if (got_lo != exp_lo) return fail("s1: slave beat 0 data != master low half");
    if (got_hi != exp_hi) return fail("s1: slave beat 1 data != master high half");
    dut.m_w_valid = 0; dut.m_w_data = 0; dut.m_w_strb = 0; dut.m_w_last = 0;
    dut.s_w_ready = 0;

    // B response.
    dut.s_b_valid = 1; dut.s_b_resp = 0;
    dut.m_b_ready = 1;
    if (wait_until([&]{ return dut.m_b_valid && dut.s_b_ready; }, 32, "m B") != 0)
        return fail("s1: master B never seen");
    if (dut.m_b_resp != 0) return fail("s1: master b_resp not OKAY");
    post_edge();
    dut.s_b_valid = 0; dut.m_b_ready = 0;

    std::printf("PASS s1: single-beat write (64b → 2x32b, little-endian)\n");
    return 0;
}

// ── Scenario 2: single-beat read ───────────────────────────────────────
static int test_single_read() {
    const uint32_t slave_lo = 0x01020304u;
    const uint32_t slave_hi = 0x55667788u;
    const uint64_t exp = ((uint64_t)slave_hi << 32) | slave_lo;
    const uint32_t addr = 0x2000;

    dut.m_ar_valid = 1; dut.m_ar_addr = addr; dut.m_ar_len = 0;
    dut.m_ar_size = 3;  dut.m_ar_burst = 1;
    dut.s_ar_ready = 1;

    int ar_seen = 0; uint32_t s_ar_len = 0xff; uint32_t s_ar_size = 0xff;
    for (int i = 0; i < 32 && !ar_seen; ++i) {
        pre_edge();
        if (dut.s_ar_valid && dut.s_ar_ready) {
            ar_seen = 1; s_ar_len = dut.s_ar_len; s_ar_size = dut.s_ar_size;
            if (dut.s_ar_addr != addr) return fail("s2: slave AR addr");
        }
        post_edge();
    }
    if (!ar_seen) return fail("s2: slave AR never seen");
    if (s_ar_len != 1) return fail("s2: slave AR len (expect 1 for ratio=2)");
    if (s_ar_size != 2) return fail("s2: slave AR size (expect 2)");
    dut.m_ar_valid = 0; dut.s_ar_ready = 0;

    // Drive the 2 slave R beats. Sub-beat 0 = low; sub-beat 1 = high + r_last.
    dut.m_r_ready = 1;

    // Beat 0: capture phase — adapter drives s_r_ready=1, master should NOT see r_valid yet.
    dut.s_r_valid = 1; dut.s_r_data = slave_lo; dut.s_r_resp = 0; dut.s_r_last = 0;

    int beat0_done = 0;
    for (int i = 0; i < 32 && !beat0_done; ++i) {
        pre_edge();
        if (dut.s_r_valid && dut.s_r_ready) {
            beat0_done = 1;
            if (dut.m_r_valid) return fail("s2: master must not see r_valid on first sub-beat");
        }
        post_edge();
    }
    if (!beat0_done) return fail("s2: first sub-beat not consumed");

    // Beat 1: terminal — drive r_last=1; master should see r_valid + assembled data + r_last.
    dut.s_r_data = slave_hi; dut.s_r_last = 1;

    int m_seen = 0;
    for (int i = 0; i < 32 && !m_seen; ++i) {
        pre_edge();
        if (dut.m_r_valid && dut.m_r_ready) {
            m_seen = 1;
            uint64_t got = ((uint64_t)dut.m_r_data);
            if (got != exp) {
                std::printf("  got=0x%016llx exp=0x%016llx\n",
                            (unsigned long long)got, (unsigned long long)exp);
                return fail("s2: master r_data assembly");
            }
            if (!dut.m_r_last) return fail("s2: master r_last must be 1");
            if (dut.m_r_resp != 0) return fail("s2: master r_resp not OKAY");
        }
        post_edge();
    }
    if (!m_seen) return fail("s2: master R never seen");
    dut.s_r_valid = 0; dut.s_r_last = 0; dut.m_r_ready = 0;

    std::printf("PASS s2: single-beat read (2x32b → 64b packed, little-endian)\n");
    return 0;
}

// ── Scenario 3: 4-beat INCR write ─────────────────────────────────────
static int test_incr4_write() {
    const uint64_t wd[4] = {
        0x0000000111111111ULL,
        0x0000000222222222ULL,
        0x0000000333333333ULL,
        0x0000000444444444ULL,
    };
    const uint32_t addr = 0x3000;

    dut.m_aw_valid = 1; dut.m_aw_addr = addr; dut.m_aw_len = 3;
    dut.m_aw_size = 3;  dut.m_aw_burst = 1;
    dut.s_aw_ready = 1;

    int aw_seen = 0; uint32_t s_aw_len = 0xff;
    for (int i = 0; i < 32 && !aw_seen; ++i) {
        pre_edge();
        if (dut.s_aw_valid && dut.s_aw_ready) { aw_seen = 1; s_aw_len = dut.s_aw_len; }
        post_edge();
    }
    if (!aw_seen) return fail("s3: slave AW never seen");
    if (s_aw_len != 7) return fail("s3: slave AW len (expect 7 for master len=3, ratio=2)");
    dut.m_aw_valid = 0; dut.s_aw_ready = 0;

    // Feed 4 master W beats, expect 8 slave W beats.
    dut.s_w_ready = 1;
    int m_beat = 0;
    int s_beat = 0;
    uint32_t slave_data[8] = {0};
    int last_seen = -1;

    dut.m_w_valid = 1; dut.m_w_data = wd[0]; dut.m_w_strb = 0xFF; dut.m_w_last = 0;

    for (int i = 0; i < 200 && (m_beat < 4 || s_beat < 8); ++i) {
        pre_edge();
        if (dut.s_w_valid && dut.s_w_ready && s_beat < 8) {
            slave_data[s_beat] = dut.s_w_data;
            if (dut.s_w_last) last_seen = s_beat;
            s_beat++;
        }
        // Master beat advance: m_w_ready high during the last sub-beat of each master beat.
        if (dut.m_w_valid && dut.m_w_ready && m_beat < 4) {
            m_beat++;
            if (m_beat < 4) {
                dut.m_w_data = wd[m_beat];
                dut.m_w_last = (m_beat == 3) ? 1 : 0;
            }
        }
        post_edge();
    }
    if (m_beat != 4) return fail("s3: master never advanced 4 wide beats");
    if (s_beat != 8) return fail("s3: did not see 8 slave W beats");
    if (last_seen != 7) return fail("s3: s_w_last must fire on slave beat 7");
    dut.m_w_valid = 0; dut.s_w_ready = 0; dut.m_w_data = 0; dut.m_w_last = 0;

    // Verify slave beats are in order: lo, hi, lo, hi, ...
    for (int b = 0; b < 4; ++b) {
        uint32_t exp_lo = (uint32_t)(wd[b] & 0xffffffff);
        uint32_t exp_hi = (uint32_t)(wd[b] >> 32);
        if (slave_data[2*b] != exp_lo) {
            std::printf("  beat %d.lo got=0x%08x exp=0x%08x\n", b, slave_data[2*b], exp_lo);
            return fail("s3: slave low-half mismatch");
        }
        if (slave_data[2*b+1] != exp_hi) {
            std::printf("  beat %d.hi got=0x%08x exp=0x%08x\n", b, slave_data[2*b+1], exp_hi);
            return fail("s3: slave high-half mismatch");
        }
    }

    dut.s_b_valid = 1; dut.s_b_resp = 0; dut.m_b_ready = 1;
    if (wait_until([&]{ return dut.m_b_valid && dut.s_b_ready; }, 32, "m B") != 0)
        return fail("s3: master B never seen");
    post_edge();
    dut.s_b_valid = 0; dut.m_b_ready = 0;

    std::printf("PASS s3: 4-beat INCR write (4x64b → 8x32b, ordered)\n");
    return 0;
}

// ── Scenario 4: 4-beat INCR read (this hit compiler bug #422) ─────────
static int test_incr4_read() {
    const uint32_t addr = 0x4000;
    // 8 narrow beats — assembled as 4 wide beats.
    const uint32_t sd[8] = {
        0x11111111u, 0x22222222u,   // → 0x22222222_11111111
        0x33333333u, 0x44444444u,   // → 0x44444444_33333333
        0x55555555u, 0x66666666u,   // → 0x66666666_55555555
        0x77777777u, 0x88888888u,   // → 0x88888888_77777777
    };

    dut.m_ar_valid = 1; dut.m_ar_addr = addr; dut.m_ar_len = 3;
    dut.m_ar_size = 3;  dut.m_ar_burst = 1;
    dut.s_ar_ready = 1;

    int ar_seen = 0; uint32_t s_ar_len = 0xff;
    for (int i = 0; i < 32 && !ar_seen; ++i) {
        pre_edge();
        if (dut.s_ar_valid && dut.s_ar_ready) { ar_seen = 1; s_ar_len = dut.s_ar_len; }
        post_edge();
    }
    if (!ar_seen) return fail("s4: slave AR never seen");
    if (s_ar_len != 7) return fail("s4: slave AR len (expect 7)");
    dut.m_ar_valid = 0; dut.s_ar_ready = 0;

    dut.m_r_ready = 1;

    int s_beat = 0;
    int m_beat = 0;
    int m_last_beat = -1;

    dut.s_r_valid = 1; dut.s_r_data = sd[0]; dut.s_r_resp = 0; dut.s_r_last = 0;

    for (int i = 0; i < 400 && (s_beat < 8 || m_beat < 4); ++i) {
        pre_edge();
        bool s_fire = dut.s_r_valid && dut.s_r_ready;
        bool m_fire = dut.m_r_valid && dut.m_r_ready;

        if (m_fire && m_beat < 4) {
            int b = m_beat;
            uint64_t got = (uint64_t)dut.m_r_data;
            uint64_t exp = ((uint64_t)sd[2*b+1] << 32) | sd[2*b];
            if (got != exp) {
                std::printf("  s4 beat %d got=0x%016llx exp=0x%016llx\n",
                            b, (unsigned long long)got, (unsigned long long)exp);
                return fail("s4: master beat assembly");
            }
            if (dut.m_r_last) m_last_beat = b;
            m_beat++;
        }
        // Hold s_r_data stable through the clock edge — only advance AFTER post_edge.
        post_edge();
        if (s_fire && s_beat < 8) {
            s_beat++;
            if (s_beat < 8) {
                dut.s_r_data = sd[s_beat];
                dut.s_r_last = (s_beat == 7) ? 1 : 0;
            }
        }
    }
    if (s_beat != 8) return fail("s4: did not consume 8 slave R beats");
    if (m_beat != 4) return fail("s4: did not deliver 4 master R beats");
    if (m_last_beat != 3) return fail("s4: master r_last must fire on beat 3 only");
    dut.s_r_valid = 0; dut.s_r_last = 0; dut.m_r_ready = 0;

    std::printf("PASS s4: 4-beat INCR read (8x32b → 4x64b packed, nested-for + lock-per-branch — issue #422 path)\n");
    return 0;
}

// ── Scenario 5: write with non-trivial w_strb ─────────────────────────
// Master strb = 0xA5 (= 1010_0101). Low slave beat strb = 0x5; high = 0xA.
static int test_strb_write() {
    const uint32_t addr = 0x5000;
    const uint64_t wdata = 0xAABBCCDDEEFF0011ULL;
    const uint32_t exp_lo = 0xEEFF0011u;
    const uint32_t exp_hi = 0xAABBCCDDu;

    dut.m_aw_valid = 1; dut.m_aw_addr = addr; dut.m_aw_len = 0;
    dut.m_aw_size = 3;  dut.m_aw_burst = 1;
    dut.s_aw_ready = 1;
    if (wait_until([&]{ return dut.s_aw_valid && dut.s_aw_ready; }, 32, "s AW") != 0)
        return fail("s5: slave AW never seen");
    post_edge();
    dut.m_aw_valid = 0; dut.s_aw_ready = 0;

    dut.m_w_valid = 1; dut.m_w_data = wdata; dut.m_w_strb = 0xA5; dut.m_w_last = 1;
    dut.s_w_ready = 1;

    int beat = 0;
    uint32_t got_lo = 0, got_hi = 0;
    uint32_t strb_lo = 0xff, strb_hi = 0xff;
    for (int i = 0; i < 32 && beat < 2; ++i) {
        pre_edge();
        if (dut.s_w_valid && dut.s_w_ready) {
            if (beat == 0) { got_lo = dut.s_w_data; strb_lo = dut.s_w_strb; }
            else           { got_hi = dut.s_w_data; strb_hi = dut.s_w_strb; }
            beat++;
        }
        post_edge();
    }
    if (beat != 2) return fail("s5: did not see 2 slave W beats");
    if (got_lo != exp_lo || got_hi != exp_hi) return fail("s5: data mismatch");
    if (strb_lo != 0x5) return fail("s5: low-beat strb should be 0x5");
    if (strb_hi != 0xA) return fail("s5: high-beat strb should be 0xA");
    dut.m_w_valid = 0; dut.s_w_ready = 0; dut.m_w_data = 0; dut.m_w_strb = 0; dut.m_w_last = 0;

    dut.s_b_valid = 1; dut.s_b_resp = 0; dut.m_b_ready = 1;
    if (wait_until([&]{ return dut.m_b_valid && dut.s_b_ready; }, 32, "m B") != 0)
        return fail("s5: master B never seen");
    post_edge();
    dut.s_b_valid = 0; dut.m_b_ready = 0;

    std::printf("PASS s5: w_strb split (master 0xA5 → slave [0x5, 0xA])\n");
    return 0;
}

// ── Scenario 6: SLVERR propagation on read ────────────────────────────
// Slave returns SLVERR (resp=2) on the low sub-beat. Master r_resp must
// surface SLVERR on the containing wide beat.
static int test_slverr_read() {
    const uint32_t addr = 0x6000;

    dut.m_ar_valid = 1; dut.m_ar_addr = addr; dut.m_ar_len = 0;
    dut.m_ar_size = 3;  dut.m_ar_burst = 1;
    dut.s_ar_ready = 1;
    if (wait_until([&]{ return dut.s_ar_valid && dut.s_ar_ready; }, 32, "s AR") != 0)
        return fail("s6: slave AR never seen");
    post_edge();
    dut.m_ar_valid = 0; dut.s_ar_ready = 0;

    dut.m_r_ready = 1;

    // Sub-beat 0: SLVERR (resp=2).
    dut.s_r_valid = 1; dut.s_r_data = 0xDEADBEEFu; dut.s_r_resp = 2; dut.s_r_last = 0;
    int beat0 = 0;
    for (int i = 0; i < 32 && !beat0; ++i) {
        pre_edge();
        if (dut.s_r_valid && dut.s_r_ready) beat0 = 1;
        post_edge();
    }
    if (!beat0) return fail("s6: first slave sub-beat not consumed");

    // Sub-beat 1: OKAY (resp=0) + r_last.
    dut.s_r_data = 0xCAFEBABEu; dut.s_r_resp = 0; dut.s_r_last = 1;

    int m_seen = 0;
    for (int i = 0; i < 32 && !m_seen; ++i) {
        pre_edge();
        if (dut.m_r_valid && dut.m_r_ready) {
            m_seen = 1;
            if (dut.m_r_resp != 2) {
                std::printf("  m_r_resp=%u expected 2 (SLVERR)\n", (unsigned)dut.m_r_resp);
                return fail("s6: master r_resp should reflect SLVERR");
            }
            if (!dut.m_r_last) return fail("s6: master r_last must be 1");
        }
        post_edge();
    }
    if (!m_seen) return fail("s6: master R never seen");
    dut.s_r_valid = 0; dut.s_r_last = 0; dut.s_r_resp = 0; dut.m_r_ready = 0;

    std::printf("PASS s6: SLVERR propagation (slave resp=2 OR-reduces into master beat)\n");
    return 0;
}

// ── Scenario 7: 4-beat WRAP read ──────────────────────────────────────
// master: addr=0x100, len=3 (4 beats), size=3 (8B), burst=WRAP. Wrap
// window = (4)*(8) = 32B aligned to 0x100.
// slave (after RATIO=2 scaling): addr=0x100, len=7 (8 beats), size=2 (4B),
// burst=WRAP forwarded unchanged. Wrap window math is byte-count-preserved
// — (master_len+1)*M_STRB_W == (slave_len+1)*S_STRB_W ⇒ both 32B.
//
// The TB doesn't model wrap addr arithmetic on the slave (that's the
// downstream slave's job); it asserts that ar_burst forwards as WRAP, the
// scaled axlen is correct, and assembly still works. Same data shape as
// the INCR test — the wrap window matters semantically to the slave, not
// to the adapter.
static int test_wrap4_read() {
    const uint32_t addr = 0x100;
    const uint32_t sd[8] = {
        0xA1A1A1A1u, 0xB1B1B1B1u,
        0xA2A2A2A2u, 0xB2B2B2B2u,
        0xA3A3A3A3u, 0xB3B3B3B3u,
        0xA4A4A4A4u, 0xB4B4B4B4u,
    };

    dut.m_ar_valid = 1; dut.m_ar_addr = addr; dut.m_ar_len = 3;
    dut.m_ar_size = 3;  dut.m_ar_burst = 2;   // WRAP
    dut.s_ar_ready = 1;

    int ar_seen = 0;
    uint32_t s_ar_len = 0xff, s_ar_burst = 0xff, s_ar_addr = 0;
    for (int i = 0; i < 32 && !ar_seen; ++i) {
        pre_edge();
        if (dut.s_ar_valid && dut.s_ar_ready) {
            ar_seen = 1;
            s_ar_len   = dut.s_ar_len;
            s_ar_burst = dut.s_ar_burst;
            s_ar_addr  = dut.s_ar_addr;
        }
        post_edge();
    }
    if (!ar_seen) return fail("s7: slave AR never seen");
    if (s_ar_len != 7)    return fail("s7: slave AR len (expect 7)");
    if (s_ar_burst != 2)  return fail("s7: slave AR burst must forward WRAP");
    if (s_ar_addr != addr)return fail("s7: slave AR addr must equal master AR addr");
    dut.m_ar_valid = 0; dut.s_ar_ready = 0;

    dut.m_r_ready = 1;
    int s_beat = 0, m_beat = 0, m_last_beat = -1;
    dut.s_r_valid = 1; dut.s_r_data = sd[0]; dut.s_r_resp = 0; dut.s_r_last = 0;

    for (int i = 0; i < 400 && (s_beat < 8 || m_beat < 4); ++i) {
        pre_edge();
        bool s_fire = dut.s_r_valid && dut.s_r_ready;
        bool m_fire = dut.m_r_valid && dut.m_r_ready;
        if (m_fire && m_beat < 4) {
            int b = m_beat;
            uint64_t got = (uint64_t)dut.m_r_data;
            uint64_t exp = ((uint64_t)sd[2*b+1] << 32) | sd[2*b];
            if (got != exp) {
                std::printf("  s7 beat %d got=0x%016llx exp=0x%016llx\n",
                            b, (unsigned long long)got, (unsigned long long)exp);
                return fail("s7: master beat assembly under WRAP forwarding");
            }
            if (dut.m_r_last) m_last_beat = b;
            m_beat++;
        }
        post_edge();
        if (s_fire && s_beat < 8) {
            s_beat++;
            if (s_beat < 8) {
                dut.s_r_data = sd[s_beat];
                dut.s_r_last = (s_beat == 7) ? 1 : 0;
            }
        }
    }
    if (s_beat != 8) return fail("s7: did not consume 8 slave R beats");
    if (m_beat != 4) return fail("s7: did not deliver 4 master R beats");
    if (m_last_beat != 3) return fail("s7: master r_last must fire on beat 3 only");
    dut.s_r_valid = 0; dut.s_r_last = 0; dut.m_r_ready = 0;

    std::printf("PASS s7: 4-beat WRAP read (burst=2 forwarded, axlen scaled, data assembly intact)\n");
    return 0;
}

// FIXED-burst rejection (ar_burst_supported / aw_burst_supported, PR #441)
// is exercised by a standalone TB with its own `main` whose exit-code
// semantics are "must abort" — see
// `examples/nic400/tb_nic400_width_adapter_fixed_reject.cpp` and the Rust
// test `test_nic400_width_adapter_fixed_burst_is_rejected_by_sva` in
// `tests/integration_test.rs` (consumes the `expect_verilator_fatal`
// harness from PR #453). The manual repro recipe in
// `doc/nic400_interconnect_spec.md` §15.1 is still useful for ad-hoc
// investigation.

int main() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    if (test_single_write()) return 1;
    for (int i = 0; i < 3; ++i) tick();
    if (test_single_read())  return 1;
    for (int i = 0; i < 3; ++i) tick();
    if (test_incr4_write())  return 1;
    for (int i = 0; i < 3; ++i) tick();
    if (test_incr4_read())   return 1;
    for (int i = 0; i < 3; ++i) tick();
    if (test_strb_write())   return 1;
    for (int i = 0; i < 3; ++i) tick();
    if (test_slverr_read())  return 1;
    for (int i = 0; i < 3; ++i) tick();
    if (test_wrap4_read())   return 1;

    std::printf("ALL PASS Nic400WidthAdapter: 7/7 scenarios\n");
    return 0;
}
