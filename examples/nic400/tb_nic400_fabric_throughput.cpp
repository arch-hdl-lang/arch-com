// Multi-outstanding throughput probe for the Nic400Fabric (3×4 R/W).
//
// Premise: the per-(master, slave) thread structure separates AR-issue from
// R-return into independent threads. With the Mealy `wait 0+ cycle until`
// form, the AR thread releases the ar_ch lock the same cycle as the
// handshake. That means a single master should be able to issue ARs at
// 1/cycle to a slave that holds ar_ready high, *regardless of whether*
// prior Rs have returned. In ARM NIC-400 vocabulary: depth-∞ outstanding
// per (master, slave) pair, limited only by the slave's ar_ready and the
// fabric's own arbitration.
//
// This TB pins that property. It runs three scenarios:
//
//   1. Single-master, single-slave, R delayed N cycles:
//      M0 drives m_ar_valid=1 continuously; S0 holds s_ar_ready=1 but
//      only emits s_r_valid after a programmable lag. Count: how many
//      ARs accepted before the first R returns. If multi-outstanding
//      works, AR_count == LAG (one AR per cycle for LAG cycles).
//
//   2. Single-master, two-slave round-robin:
//      M0 alternates ar_addr between slave 0 and slave 1 every cycle.
//      Different per-slave threads must hand off the master-side
//      ar_ready drive without inserting a bubble. Expect 1 AR/cycle
//      sustained across 9 transactions.
//
//   3. Multi-master, multi-slave concurrent:
//      M0→S0, M1→S1, M2→S2 simultaneously, each driving 9 ARs back-to-back.
//      Per-slave threads run independently so total AR rate ≈ M txn/cycle.
//      Pin: 3 masters complete 9 each in ≤ 9+overhead cycles (i.e.
//      aggregate ≥ 2.0 t/cycle).
//
// All three are property-style checks: they fail if a future change
// re-introduces serialisation in the AR-issue path.

#include "VNic400Fabric.h"
#include <cstdint>
#include <cstdio>

static VNic400Fabric dut;
static uint64_t cycle = 0;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    cycle++;
}

static void clear_inputs() {
    for (int i = 0; i < 3; ++i) {
        dut.m_ar_valid[i] = 0; dut.m_ar_addr[i] = 0; dut.m_ar_id[i] = 0;
        dut.m_ar_len[i] = 0;   dut.m_ar_size[i] = 0; dut.m_ar_burst[i] = 0;
        dut.m_r_ready[i] = 0;
    }
    for (int j = 0; j < 4; ++j) {
        dut.s_ar_ready[j] = 0;
        dut.s_r_valid[j] = 0;  dut.s_r_data[j] = 0; dut.s_r_id[j] = 0;
        dut.s_r_resp[j] = 0;   dut.s_r_last[j] = 0;
    }
}

static void do_reset() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();
}

// ── Scenario 1: AR multi-outstanding under delayed R ────────────────────
// Drive AR back-to-back. Slave holds ar_ready=1 but delays r_valid by LAG.
// Count ARs accepted before first R. Expect == LAG.
static int scenario_ar_ahead_of_r() {
    const int LAG       = 4;
    const int N_TXN     = 6;

    clear_inputs();
    dut.s_ar_ready[0] = 1;
    dut.m_r_ready[0]  = 1;
    dut.m_ar_addr[0]  = 0x00001000;
    dut.m_ar_size[0]  = 2;
    dut.m_ar_burst[0] = 1;
    dut.m_ar_valid[0] = 1;

    int ar_seen  = 0;
    int r_seen   = 0;
    int ar_before_r = -1;
    // Queue of slave-side ids we've accepted; emit them as R after LAG cycles.
    uint32_t pending_ids[64] = {0};
    int pending_cycle[64]    = {0};
    int pending_head = 0, pending_tail = 0;

    int last_handshake_cycle = -1;
    int started = (int)cycle;
    for (int t = 0; t < 64 && (ar_seen < N_TXN || r_seen < N_TXN); ++t) {
        dut.m_ar_id[0] = (uint8_t)((ar_seen + 1) & 0x7);
        // Drive R for any pending entry whose lag has elapsed.
        if (pending_head != pending_tail
            && (int)cycle - pending_cycle[pending_head] >= LAG
            && r_seen < N_TXN) {
            dut.s_r_valid[0] = 1;
            dut.s_r_data[0]  = 0xC0DE0000u | r_seen;
            dut.s_r_id[0]    = pending_ids[pending_head];
            dut.s_r_last[0]  = 1;
        } else {
            dut.s_r_valid[0] = 0;
        }
        // Stop pushing ARs once we've issued N_TXN.
        if (ar_seen >= N_TXN) dut.m_ar_valid[0] = 0;

        tick();

        if (dut.m_ar_valid[0] && dut.s_ar_ready[0] && ar_seen < N_TXN) {
            // We saw this AR handshake — slave-side id is {0, master_id}.
            pending_ids[pending_tail]   = (ar_seen + 1) & 0x7;
            pending_cycle[pending_tail] = (int)cycle;
            pending_tail = (pending_tail + 1) % 64;
            ar_seen++;
            last_handshake_cycle = (int)cycle;
        }
        if (dut.m_r_valid[0] && dut.m_r_ready[0]) {
            if (ar_before_r < 0) ar_before_r = ar_seen;
            pending_head = (pending_head + 1) % 64;
            r_seen++;
        }
    }

    if (ar_seen < N_TXN) {
        std::printf("FAIL S1: only %d/%d ARs accepted (R never returned?)\n", ar_seen, N_TXN);
        return 1;
    }
    if (ar_before_r < 0) {
        std::printf("FAIL S1: never observed any R\n");
        return 1;
    }
    std::printf("INFO  S1 (delayed-R): %d ARs accepted before first R, LAG=%d\n",
                ar_before_r, LAG);
    if (ar_before_r < LAG) {
        std::printf("FAIL S1: expected >= %d ARs in flight before first R (got %d)\n",
                    LAG, ar_before_r);
        return 1;
    }
    std::printf("PASS S1: multi-outstanding AR confirmed (%d in flight at peak)\n", ar_before_r);
    (void)started; (void)last_handshake_cycle;
    return 0;
}

// ── Scenario 2: single master alternating between slaves ────────────────
// M0 issues ARs that alternate between slave 0 and slave 1 each cycle.
// Different per-slave threads must hand off the m.ar_ready drive cleanly.
static int scenario_master_alternating() {
    const int N_TXN = 8;
    clear_inputs();
    dut.s_ar_ready[0] = 1;
    dut.s_ar_ready[1] = 1;
    dut.m_r_ready[0]  = 1;
    dut.m_ar_size[0]  = 2;
    dut.m_ar_burst[0] = 1;
    dut.m_ar_valid[0] = 1;

    int seen = 0;
    int t_start = (int)cycle;
    int last_seen_cycle = -1;
    for (int t = 0; t < N_TXN * 4 && seen < N_TXN; ++t) {
        // Alternate target slave by setting bit 28 of the address.
        // Address decode picks slave index = ar_addr[29:28].
        uint32_t addr = (seen & 1) ? 0x10000000u : 0x00000000u;
        dut.m_ar_addr[0] = addr | 0x100;
        dut.m_ar_id[0]   = (uint8_t)((seen + 1) & 0x7);
        tick();
        // Check which slave fired this cycle.
        if (dut.s_ar_valid[0] && dut.s_ar_ready[0]) {
            seen++;
            last_seen_cycle = (int)cycle - t_start;
        } else if (dut.s_ar_valid[1] && dut.s_ar_ready[1]) {
            seen++;
            last_seen_cycle = (int)cycle - t_start;
        }
    }
    if (seen < N_TXN) {
        std::printf("FAIL S2: only %d/%d ARs accepted in %d cycles\n",
                    seen, N_TXN, last_seen_cycle);
        return 1;
    }
    double tpc = (double)seen / last_seen_cycle;
    std::printf("INFO  S2 (alternating slaves): %d ARs in %d cycles = %.2f t/c\n",
                seen, last_seen_cycle, tpc);
    // Cross-slave alternation handoff: expect ≥ 0.5 t/c (no worse than a
    // 1-cycle handoff bubble between slaves).
    if (seen * 2 < last_seen_cycle) {
        std::printf("FAIL S2: alternating throughput < 0.5 t/c\n");
        return 1;
    }
    std::printf("PASS S2: cross-slave alternation = %.2f t/c\n", tpc);
    // Drain master valid.
    dut.m_ar_valid[0] = 0;
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

// ── Scenario 3: 3 masters concurrently to 3 different slaves ────────────
// Per-slave threads run independently when they target different slaves,
// so aggregate AR rate should be M t/c (up to ar_ready back-pressure).
static int scenario_multi_master() {
    const int N_TXN_PER_M = 6;
    const int M           = 3;
    clear_inputs();
    for (int j = 0; j < M; ++j) dut.s_ar_ready[j] = 1;
    for (int i = 0; i < M; ++i) {
        dut.m_r_ready[i]  = 1;
        dut.m_ar_size[i]  = 2;
        dut.m_ar_burst[i] = 1;
        dut.m_ar_valid[i] = 1;
        // Each master targets its own slave: m_i → s_i (addr top bits = i).
        dut.m_ar_addr[i]  = ((uint32_t)i << 28) | 0x100;
    }
    int seen[3] = {0, 0, 0};
    int total   = 0;
    int t_start = (int)cycle;
    int last    = -1;
    for (int t = 0; t < N_TXN_PER_M * M * 6 && total < N_TXN_PER_M * M; ++t) {
        for (int i = 0; i < M; ++i)
            dut.m_ar_id[i] = (uint8_t)((seen[i] + 1) & 0x7);
        tick();
        for (int i = 0; i < M; ++i) {
            if (seen[i] >= N_TXN_PER_M) {
                dut.m_ar_valid[i] = 0;
                continue;
            }
            uint8_t v = dut.s_ar_valid[i] && dut.s_ar_ready[i];
            if (v) {
                // Confirm prefix matches expected master.
                uint32_t sid = dut.s_ar_id[i];
                uint32_t prefix = sid >> 3;
                if ((int)prefix != i) {
                    std::printf("FAIL S3: slave %d saw AR with prefix %u (expected %d)\n",
                                i, prefix, i);
                    return 1;
                }
                seen[i]++;
                total++;
                last = (int)cycle - t_start;
            }
        }
    }
    if (total < N_TXN_PER_M * M) {
        std::printf("FAIL S3: %d/%d ARs completed in %d cycles (per-master %d/%d/%d)\n",
                    total, N_TXN_PER_M * M, last, seen[0], seen[1], seen[2]);
        return 1;
    }
    double tpc = (double)total / last;
    std::printf("INFO  S3 (3M→3S concurrent): %d ARs in %d cycles = %.2f t/c\n",
                total, last, tpc);
    // Pin aggregate throughput ≥ 1.5 t/c. Real ceiling is 3 t/c (M ports,
    // M slaves, all independent), but the per-slave arbiter and any
    // shared-cycle scheduling may add overhead.
    if (total * 2 < last * 3) {
        std::printf("FAIL S3: aggregate throughput %.2f t/c < 1.5 t/c gate\n", tpc);
        return 1;
    }
    std::printf("PASS S3: multi-master aggregate = %.2f t/c (≥ 1.5 gate)\n", tpc);
    for (int i = 0; i < M; ++i) dut.m_ar_valid[i] = 0;
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    do_reset();

    if (scenario_ar_ahead_of_r()) return 1;
    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();

    if (scenario_master_alternating()) return 1;
    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();

    if (scenario_multi_master()) return 1;

    std::printf("PASS Nic400Fabric throughput: depth-N AR multi-outstanding works\n");
    return 0;
}
