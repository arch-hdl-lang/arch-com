// Hot-slave handoff throughput probe for the Nic400Fabric (3×4 R/W).
//
// Closes the v0.70.0 §16.1 gap-tracker question: when two masters both
// target the same slave, does the slave-side `ar_lock` arbiter introduce
// a dead cycle on the M0↔M1 handoff?
//
// Background
// ──────────
// `tb_nic400_fabric_throughput.cpp` already pins:
//   • single-master single-slave: depth-∞ multi-outstanding ARs
//   • single-master alternating slaves: 1.00 t/c (cross-slave handoff on
//     the master-side `ar_ch` lock — no bubble)
//   • 3M → 3S concurrent disjoint pairs: 3.00 t/c aggregate (linear)
//
// `tb_nic400_read2x2_hot_slave.cpp` checks fairness (no starvation
// under the drop-after-grant pattern) but never counts cycles. So the
// "zero-cycle handoff on the slave-side arbiter when two masters
// contend for one slave" property is architecturally claimed but not
// measured. This TB measures it.
//
// Arbiter shape
// ─────────────
// `Nic400SlavePort.ar_lock` is declared as `mutex<round_robin>`. Under
// persistent simultaneous contention (both m_ar_valid held high
// continuously), the round-robin arbiter distributes grants fairly
// between masters (roughly equal share, no starvation).
//
// Measuring handoff cycles
// ────────────────────────
// To time the M↔M handoff directly, the TB sets up an "M0 wins, then
// swap to M1" event and counts how many cycles pass between M0's grant
// and M1's grant. Under a zero-cycle handoff, M1 grants on the very
// next cycle after M0 (1-cycle period); a dead cycle on swap would
// stretch that to 2 cycles.
//
// Scenarios
// ─────────
//   1. Direct swap test: 10 explicit M0→M1 and M1→M0 swap events.
//      Each swap must complete in exactly 1 cycle (no dead cycle).
//   2. Persistent contention (round-robin fairness check):
//      both valids held high. Round-robin should distribute grants to
//      both masters; confirm 1.00 t/c aggregate and no starvation.
//   3. Contender dropout: M0 monopolises for a few ARs, then drops
//      permanently. M1 should grant on the very next cycle (no bubble
//      on the contender-disappears event) and continue at 1.00 t/c.
//   4. Asymmetric-load starvation guard: M0 valid=1 continuously while
//      M1 toggles valid once every 5 cycles for 50 cycles. Catches the
//      canonical round-robin failure mode where the pointer advances
//      every cycle instead of advancing on grant — under that bug the
//      low-frequency requester is starved. Fair RR must grant M1 within
//      a bounded number of cycles of every M1 valid epoch.

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

// Common per-master AR setup for slave-0 targeting.
static void prime_master(int i, uint32_t addr) {
    dut.m_ar_addr[i]  = addr;
    dut.m_ar_size[i]  = 2;
    dut.m_ar_burst[i] = 1;
    dut.m_ar_valid[i] = 1;
}

// ── Scenario 1: direct M↔M swap timing ──────────────────────────────────
// The cleanest way to time the handoff is to set up explicit swap
// events: cycle K-1 has only M_a requesting; cycle K we flip to only
// M_b requesting. Under a zero-cycle handoff, M_a grants on K-1 and
// M_b grants on K — the gap between consecutive grants is 1 cycle.
// A dead cycle on the swap would stretch that to 2 cycles.
//
// We run 10 swap events alternating M0↔M1 and assert that EVERY swap
// hits a 1-cycle gap. We also tally aggregate t/c over the whole run.
static int scenario_swap_timing() {
    const int N_SWAPS = 10;

    clear_inputs();
    dut.s_ar_ready[0] = 1;
    dut.m_r_ready[0]  = 1;
    dut.m_r_ready[1]  = 1;
    dut.m_ar_addr[0]  = 0x00001000;
    dut.m_ar_size[0]  = 2;
    dut.m_ar_burst[0] = 1;
    dut.m_ar_addr[1]  = 0x00002000;
    dut.m_ar_size[1]  = 2;
    dut.m_ar_burst[1] = 1;

    int total   = 0;
    int t_start = (int)cycle;
    int last_grant_cycle = -1;
    int worst_gap = 0;
    int over_one_gap_count = 0;
    int last_winner = -1;     // 0 or 1

    // Issue N_SWAPS+1 grants total: each swap event = one grant in the
    // new master right after the previous master's grant. Set valids
    // pre-tick to control who's requesting at the posedge.
    int swaps_done = 0;
    int seen_m0 = 0, seen_m1 = 0;
    for (int t = 0; t < (N_SWAPS + 1) * 4 && swaps_done <= N_SWAPS; ++t) {
        // Whose turn is it to request? Start with M0, alternate after each grant.
        int requester = (last_winner == 0) ? 1 : 0;
        dut.m_ar_valid[0] = (requester == 0) ? 1 : 0;
        dut.m_ar_valid[1] = (requester == 1) ? 1 : 0;
        dut.m_ar_id[0]    = (uint8_t)(seen_m0 & 0x7);
        dut.m_ar_id[1]    = (uint8_t)(seen_m1 & 0x7);

        tick();

        if (dut.s_ar_valid[0] && dut.s_ar_ready[0]) {
            uint32_t prefix = (dut.s_ar_id[0]) >> 3;
            int winner = (int)prefix;
            int now    = (int)cycle - t_start;
            if (last_grant_cycle >= 0) {
                int gap = now - last_grant_cycle;
                if (gap > worst_gap) worst_gap = gap;
                if (last_winner != winner && gap != 1) {
                    over_one_gap_count++;
                    std::printf("INFO  S1: M%d→M%d swap event at cycle %d "
                                "saw gap=%d (expected 1)\n",
                                last_winner, winner, now, gap);
                }
            }
            if (winner == 0)      seen_m0++;
            else if (winner == 1) seen_m1++;
            else {
                std::printf("FAIL S1: bad prefix %u\n", prefix);
                return 1;
            }
            if (last_winner != -1 && last_winner != winner) swaps_done++;
            last_winner = winner;
            last_grant_cycle = now;
            total++;
        }
    }

    if (swaps_done < N_SWAPS) {
        std::printf("FAIL S1: only %d/%d swap events observed in %d cycles\n",
                    swaps_done, N_SWAPS, last_grant_cycle);
        return 1;
    }
    double tpc = (double)total / last_grant_cycle;
    std::printf("INFO  S1 (direct M↔M swap timing): %d grants over %d swaps "
                "= %.2f t/c, worst gap=%d cycles, swaps>1-cycle=%d\n",
                total, swaps_done, tpc, worst_gap, over_one_gap_count);

    if (over_one_gap_count > 0) {
        std::printf("FAIL S1: %d swap events had a >1-cycle gap "
                    "(dead cycle on M↔M handoff)\n", over_one_gap_count);
        return 1;
    }
    if (worst_gap != 1) {
        std::printf("FAIL S1: worst gap %d != 1 cycle\n", worst_gap);
        return 1;
    }
    std::printf("PASS S1: every M↔M swap completed in exactly 1 cycle "
                "(zero-cycle handoff, %.2f t/c aggregate)\n", tpc);

    dut.m_ar_valid[0] = 0;
    dut.m_ar_valid[1] = 0;
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

// ── Scenario 2: persistent contention — mutex<round_robin> fairness ─────
// Both valids high forever. The round-robin mutex should distribute
// grants fairly across masters. Confirm:
//   (a) aggregate t/c ≥ 0.9 (no bubble between consecutive grants), and
//   (b) both masters receive at least one AR (no starvation).
static int scenario_persistent_contention() {
    const int N_TXN = 16;

    clear_inputs();
    dut.s_ar_ready[0] = 1;
    dut.m_r_ready[0]  = 1;
    dut.m_r_ready[1]  = 1;
    prime_master(0, 0x00001000);
    prime_master(1, 0x00002000);

    int seen_m0 = 0, seen_m1 = 0;
    int total   = 0;
    int t_start = (int)cycle;
    int last    = -1;
    for (int t = 0; t < N_TXN * 4 && total < N_TXN; ++t) {
        dut.m_ar_id[0] = (uint8_t)(seen_m0 & 0x7);
        dut.m_ar_id[1] = (uint8_t)(seen_m1 & 0x7);
        tick();
        if (dut.s_ar_valid[0] && dut.s_ar_ready[0]) {
            uint32_t prefix = (dut.s_ar_id[0]) >> 3;
            if (prefix == 0) seen_m0++;
            else if (prefix == 1) seen_m1++;
            total++;
            last = (int)cycle - t_start;
        }
    }
    double tpc = (double)total / last;
    std::printf("INFO  S2 (persistent contention): %d ARs in %d cycles = %.2f t/c "
                "(m0=%d, m1=%d)\n", total, last, tpc, seen_m0, seen_m1);

    if ((double)total < 0.9 * (double)last) {
        std::printf("FAIL S2: %.2f t/c < 0.9 gate (back-to-back grants have a bubble)\n",
                    tpc);
        return 1;
    }
    // With mutex<round_robin> both masters should get grants — no starvation.
    bool both_got_some = (seen_m0 > 0 && seen_m1 > 0);
    if (!both_got_some) {
        std::printf("FAIL S2: round_robin should give both masters access "
                    "(m0=%d, m1=%d)\n", seen_m0, seen_m1);
        return 1;
    }
    std::printf("PASS S2: mutex<round_robin> — both masters got ARs "
                "(m0=%d, m1=%d, %.2f t/c — no bubble)\n", seen_m0, seen_m1, tpc);

    dut.m_ar_valid[0] = 0;
    dut.m_ar_valid[1] = 0;
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

// ── Scenario 3: contender dropout ───────────────────────────────────────
// Run forced alternation for a few rounds, then drop M0 permanently.
// M1 should run at 1.00 t/c on its own afterwards.
static int scenario_contender_dropout() {
    const int N_ALT_BEFORE_DROP = 4;       // total alternating ARs before M0 quits
    const int N_M1_TAIL         = 6;       // ARs M1 must complete after M0 drops

    clear_inputs();
    dut.s_ar_ready[0] = 1;
    dut.m_r_ready[0]  = 1;
    dut.m_r_ready[1]  = 1;
    prime_master(0, 0x00001000);
    prime_master(1, 0x00002000);

    int seen_m0 = 0, seen_m1 = 0;
    int total = 0;
    int dropped_at_cycle = -1;
    int m1_tail_seen = 0;
    int m1_tail_cycle = -1;
    int just_won = -1;
    int t_start = (int)cycle;
    for (int t = 0; t < (N_ALT_BEFORE_DROP + N_M1_TAIL) * 8
                    && (total < N_ALT_BEFORE_DROP || m1_tail_seen < N_M1_TAIL); ++t) {
        dut.m_ar_id[0] = (uint8_t)(seen_m0 & 0x7);
        dut.m_ar_id[1] = (uint8_t)(seen_m1 & 0x7);
        // Re-prime after 1-cycle dropout, BEFORE the M0-drop threshold.
        if (just_won >= 0 && total < N_ALT_BEFORE_DROP) {
            if (just_won == 0) dut.m_ar_valid[0] = 1;
            if (just_won == 1) dut.m_ar_valid[1] = 1;
            just_won = -1;
        }
        tick();
        if (dut.s_ar_valid[0] && dut.s_ar_ready[0]) {
            uint32_t prefix = (dut.s_ar_id[0]) >> 3;
            if (prefix == 0) { seen_m0++; just_won = 0; dut.m_ar_valid[0] = 0; }
            else if (prefix == 1) { seen_m1++; just_won = 1; dut.m_ar_valid[1] = 0; }
            total++;
            if (total == N_ALT_BEFORE_DROP) {
                dut.m_ar_valid[0] = 0;
                dropped_at_cycle  = (int)cycle - t_start;
            }
            if (total > N_ALT_BEFORE_DROP && prefix == 1) {
                m1_tail_seen++;
                m1_tail_cycle = (int)cycle - t_start;
                dut.m_ar_valid[1] = 1;     // keep M1 running solo
            }
        }
    }
    if (m1_tail_seen < N_M1_TAIL) {
        std::printf("FAIL S3: M1 tail only %d/%d ARs after M0 dropped\n",
                    m1_tail_seen, N_M1_TAIL);
        return 1;
    }
    int tail_cycles = m1_tail_cycle - dropped_at_cycle;
    double tpc_tail = (double)m1_tail_seen / tail_cycles;
    std::printf("INFO  S3 (M0 dropped after %d alternating ARs): "
                "M1 ran %d ARs in %d tail cycles = %.2f t/c\n",
                N_ALT_BEFORE_DROP, m1_tail_seen, tail_cycles, tpc_tail);
    if ((double)m1_tail_seen < 0.9 * (double)tail_cycles) {
        std::printf("FAIL S3: M1-alone tail %.2f t/c < 0.9 gate\n", tpc_tail);
        return 1;
    }
    std::printf("PASS S3: contender dropout — M1-alone tail = %.2f t/c\n", tpc_tail);

    dut.m_ar_valid[0] = 0;
    dut.m_ar_valid[1] = 0;
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

// ── Scenario 4: asymmetric-load starvation guard ────────────────────────
// M0 drives ar_valid=1 continuously across 50 cycles. M1 re-arms a fresh
// request every 5 cycles: valid goes high and stays high until granted,
// then drops for the rest of the 5-cycle slot. Canonical asymmetric-load
// pattern that exposes a broken round-robin pointer that advances every
// cycle (instead of advancing on grant) — under that bug, M0 (always
// valid) would keep winning and M1 would be starved. Fair RR must grant
// M1 within at most 15 cycles of every M1 valid epoch.
static int scenario_asymmetric_load() {
    const int N_CYCLES = 50;
    const int M1_PERIOD = 5;
    const int M1_MAX_GAP = 15;

    clear_inputs();
    dut.s_ar_ready[0] = 1;
    dut.m_r_ready[0]  = 1;
    dut.m_r_ready[1]  = 1;
    dut.m_ar_addr[0]  = 0x00001000;
    dut.m_ar_size[0]  = 2;
    dut.m_ar_burst[0] = 1;
    dut.m_ar_addr[1]  = 0x00002000;
    dut.m_ar_size[1]  = 2;
    dut.m_ar_burst[1] = 1;
    dut.m_ar_valid[0] = 1;   // M0 always valid
    dut.m_ar_valid[1] = 0;

    int seen_m0 = 0, seen_m1 = 0;
    int cyc = 0;
    int last_m1_grant = 0;
    int worst_m1_gap = 0;
    int m1_epoch_starts = 0;
    for (int t = 0; t < N_CYCLES; ++t) {
        // Re-arm M1 valid every M1_PERIOD cycles; hold until granted.
        if ((cyc % M1_PERIOD) == 0) {
            dut.m_ar_valid[1] = 1;
            m1_epoch_starts++;
        }
        dut.m_ar_id[0] = (uint8_t)(seen_m0 & 0x7);
        dut.m_ar_id[1] = (uint8_t)(seen_m1 & 0x7);
        tick();
        cyc++;
        if (dut.s_ar_valid[0] && dut.s_ar_ready[0]) {
            uint32_t prefix = (dut.s_ar_id[0]) >> 3;
            if (prefix == 0) {
                seen_m0++;
            } else if (prefix == 1) {
                seen_m1++;
                int gap = cyc - last_m1_grant;
                if (gap > worst_m1_gap) worst_m1_gap = gap;
                last_m1_grant = cyc;
                // M1 served — drop valid until next epoch.
                dut.m_ar_valid[1] = 0;
            }
        }
    }

    std::printf("INFO  S4 (asymmetric load): %d cycles, m0=%d m1=%d/%d epochs, "
                "worst_m1_gap=%d\n",
                cyc, seen_m0, seen_m1, m1_epoch_starts, worst_m1_gap);

    if (seen_m1 < m1_epoch_starts) {
        std::printf("FAIL S4: M1 starved — only %d/%d epoch grants in %d cycles "
                    "(m0=%d)\n", seen_m1, m1_epoch_starts, cyc, seen_m0);
        return 1;
    }
    if (worst_m1_gap > M1_MAX_GAP) {
        std::printf("FAIL S4: M1 starvation — worst gap=%d cycles between "
                    "consecutive M1 grants (expected <= %d)\n",
                    worst_m1_gap, M1_MAX_GAP);
        return 1;
    }
    // Sanity: M0 should still receive most grants (always valid).
    if (seen_m0 <= seen_m1) {
        std::printf("FAIL S4: unexpected — M1 (%d) >= M0 (%d); M0 was held "
                    "valid continuously\n", seen_m1, seen_m0);
        return 1;
    }
    std::printf("PASS S4: asymmetric load — m0=%d m1=%d/%d epochs worst_m1_gap=%d "
                "(round-robin grants low-frequency requester within window)\n",
                seen_m0, seen_m1, m1_epoch_starts, worst_m1_gap);

    dut.m_ar_valid[0] = 0;
    dut.m_ar_valid[1] = 0;
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    do_reset();

    if (scenario_swap_timing())           return 1;
    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();

    if (scenario_persistent_contention()) return 1;
    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();

    if (scenario_contender_dropout())     return 1;
    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();

    if (scenario_asymmetric_load())       return 1;

    std::printf("PASS Nic400Fabric hot-slave throughput: zero-cycle M↔M handoff\n");
    return 0;
}
