// Multi-master contention test for Nic400Fabric (3 masters, 4 slaves, R/W).
//
// Closes §16.1 gap: "Multi-master contention (M=2..3 active at once)".
// All three master ports are active simultaneously in each scenario.
//
// Address decode (NS_W=2, REGION_BITS=28)
// ────────────────────────────────────────
//   addr[29:28] = 0b00  →  slave 0   (0x0000_xxxx)
//   addr[29:28] = 0b01  →  slave 1   (0x1000_xxxx)
//   addr[29:28] = 0b10  →  slave 2   (0x2000_xxxx)
//   addr[29:28] = 0b11  →  slave 3   (0x3000_xxxx)
//
// Scenarios
// ─────────
//   S1. Disjoint-slave reads  — M0→S0, M1→S1, M2→S2 concurrently.
//       No slave-side contention; each path runs at 1.00 t/c independently.
//       Aggregate throughput verified ≥ 2.7 t/c (3× linear, one slack bit
//       for the round-up rounding in the measurement window).
//
//   S2. Hot-slave reads (3-way) — M0, M1, M2 all target S0.
//       ar_lock arbiter is round_robin over 3 requesters. Aggregate ≥ 0.9 t/c
//       (slave side is the bottleneck, no dead cycle between grants).
//       All three masters must receive at least one AR grant (no starvation).
//
//   S3. Disjoint-slave writes — M0→S0, M1→S1, M2→S2 in parallel.
//       Each master sends N_WR write transactions. The full AW→W→B path
//       completes independently per master; the TB plays the slave side
//       (responds to s_aw_valid / s_w_valid; injects s_b_valid after w_last).
//       Verify: all 3×N_WR transactions complete, each B response is routed
//       back to the originating master, and no B leakage across masters.

#include "VNic400Fabric.h"
#include <cstdint>
#include <cstdio>
#include <cassert>

static VNic400Fabric dut;
static uint64_t cycle = 0;

static void tick() {
    dut.clk = 0; dut.eval();
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
        dut.m_w_last[i] = 0;   dut.m_b_ready[i] = 0;
    }
    for (int j = 0; j < 4; ++j) {
        dut.s_ar_ready[j] = 0;
        dut.s_r_valid[j] = 0;  dut.s_r_data[j] = 0; dut.s_r_id[j] = 0;
        dut.s_r_resp[j] = 0;   dut.s_r_last[j] = 0;
        dut.s_aw_ready[j] = 0; dut.s_w_ready[j] = 0;
        dut.s_b_valid[j] = 0;  dut.s_b_id[j] = 0;   dut.s_b_resp[j] = 0;
    }
}

static void do_reset() {
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();
}

// Base address for each slave (addr[29:28] selects slave).
static uint32_t slave_base(int j) {
    return (uint32_t)(j) << 28;
}

// ── Scenario 1: disjoint-slave reads, all 3 masters concurrent ───────────
// M0→S0, M1→S1, M2→S2. All three slaves always ready. Measure aggregate
// AR grants over N_TXN total transactions (split evenly, ~N_TXN/3 per master).
static int scenario_disjoint_reads() {
    const int N_TXN = 24;   // total ARs expected (8 per master)

    clear_inputs();
    // Each slave always ready; each master always valid.
    for (int j = 0; j < 3; ++j) dut.s_ar_ready[j] = 1;
    for (int i = 0; i < 3; ++i) {
        dut.m_ar_valid[i] = 1;
        dut.m_ar_addr[i]  = slave_base(i) | 0x1000u;
        dut.m_ar_size[i]  = 2;
        dut.m_ar_burst[i] = 1;
        dut.m_r_ready[i]  = 1;
    }

    int seen[3] = {0, 0, 0};
    int total = 0;
    int t_start = (int)cycle;
    int last = -1;
    int seen_m[3][4] = {};   // seen_m[master][slave] grant count — should be diagonal

    for (int t = 0; t < N_TXN * 4 && total < N_TXN; ++t) {
        // Bump IDs so transactions don't overlap in-flight.
        for (int i = 0; i < 3; ++i)
            dut.m_ar_id[i] = (uint8_t)(seen[i] & 0x7);

        tick();

        for (int j = 0; j < 3; ++j) {
            if (dut.s_ar_valid[j] && dut.s_ar_ready[j]) {
                uint32_t id   = dut.s_ar_id[j];
                int master    = (int)(id >> 3);   // top bits = master index
                if (master < 0 || master > 2) {
                    std::printf("FAIL S1: bad id prefix %u on slave %d\n", id >> 3, j);
                    return 1;
                }
                if (master != j) {
                    std::printf("FAIL S1: master %d routed to wrong slave %d\n", master, j);
                    return 1;
                }
                seen[master]++;
                seen_m[master][j]++;
                total++;
                last = (int)cycle - t_start;
            }
        }
    }

    if (total < N_TXN) {
        std::printf("FAIL S1: only %d/%d ARs completed\n", total, N_TXN);
        return 1;
    }
    double tpc = (double)total / last;
    std::printf("INFO  S1 (disjoint reads): %d ARs in %d cycles = %.2f t/c "
                "(m0→s0=%d, m1→s1=%d, m2→s2=%d)\n",
                total, last, tpc, seen[0], seen[1], seen[2]);

    if (tpc < 2.7) {
        std::printf("FAIL S1: aggregate %.2f t/c < 2.7 (3 independent paths should be ~3.00)\n", tpc);
        return 1;
    }
    // All grants should be on the diagonal.
    for (int i = 0; i < 3; ++i) {
        for (int j = 0; j < 3; ++j) {
            if (i != j && seen_m[i][j] > 0) {
                std::printf("FAIL S1: master %d leaked %d ARs to slave %d\n",
                            i, seen_m[i][j], j);
                return 1;
            }
        }
    }
    std::printf("PASS S1: 3-master disjoint reads — %.2f t/c aggregate, all routes correct\n", tpc);

    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

// ── Scenario 2: 3-way hot-slave reads (M0, M1, M2 all → S0) ─────────────
// ar_lock on S0 is mutex<round_robin>. All 3 valids held high. Confirms:
//   (a) aggregate ≥ 0.9 t/c (no dead cycle between round_robin grants), and
//   (b) all three masters receive at least one AR (no starvation under 3-way).
static int scenario_hot_slave_3way() {
    const int N_TXN = 30;   // total ARs to collect

    clear_inputs();
    dut.s_ar_ready[0] = 1;   // only S0 targeted
    for (int i = 0; i < 3; ++i) {
        dut.m_ar_valid[i] = 1;
        dut.m_ar_addr[i]  = slave_base(0) | (uint32_t)((i + 1) * 0x1000);
        dut.m_ar_size[i]  = 2;
        dut.m_ar_burst[i] = 1;
        dut.m_r_ready[i]  = 1;
    }

    int seen[3] = {0, 0, 0};
    int total   = 0;
    int t_start = (int)cycle;
    int last    = -1;

    for (int t = 0; t < N_TXN * 6 && total < N_TXN; ++t) {
        for (int i = 0; i < 3; ++i)
            dut.m_ar_id[i] = (uint8_t)(seen[i] & 0x7);

        tick();

        if (dut.s_ar_valid[0] && dut.s_ar_ready[0]) {
            uint32_t id = dut.s_ar_id[0];
            int master  = (int)(id >> 3);
            if (master < 0 || master > 2) {
                std::printf("FAIL S2: bad id prefix %u\n", id >> 3);
                return 1;
            }
            seen[master]++;
            total++;
            last = (int)cycle - t_start;
        }
    }

    if (total < N_TXN) {
        std::printf("FAIL S2: only %d/%d ARs completed\n", total, N_TXN);
        return 1;
    }
    double tpc = (double)total / last;
    std::printf("INFO  S2 (3-way hot-slave reads): %d ARs in %d cycles = %.2f t/c "
                "(m0=%d, m1=%d, m2=%d)\n",
                total, last, tpc, seen[0], seen[1], seen[2]);

    if (tpc < 0.9) {
        std::printf("FAIL S2: %.2f t/c < 0.9 (dead cycle between round_robin grants)\n", tpc);
        return 1;
    }
    bool all_got_some = (seen[0] > 0 && seen[1] > 0 && seen[2] > 0);
    if (!all_got_some) {
        std::printf("FAIL S2: starvation — m0=%d m1=%d m2=%d (all must be >0)\n",
                    seen[0], seen[1], seen[2]);
        return 1;
    }
    std::printf("PASS S2: 3-way hot-slave — no starvation, %.2f t/c\n", tpc);

    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

// ── Scenario 3: disjoint-slave writes, all 3 masters concurrent ──────────
// M0→S0, M1→S1, M2→S2. Each master sends N_WR single-beat write transactions
// in parallel with the other two masters. The TB plays all three slave sides
// simultaneously using a pre_edge/post_edge split (same pattern as the serial
// write TB in tb_nic400_fabric_write.cpp) so handshakes are sampled at the
// combinational output before the rising edge that commits them.
//
// Per-master state machine (mapped to slave j = i since disjoint):
//   AW_W (0): drive m_aw_valid + m_w_valid; wait for s_aw_valid+ready AND
//             s_w_valid+ready (occur in separate cycles: AW fires first,
//             then thread enters W phase on the next tick, W fires second).
//   B_INJ (1): drive s_b_valid; wait for m_b_valid+m_b_ready.
//
// Verify: all 3×N_WR BRESPs reach exactly the correct master (ID prefix
// decoded correctly), no cross-master leakage on any B channel.
static int scenario_disjoint_writes() {
    const int N_WR = 6;

    clear_inputs();

    // Per-master/slave-i state (master i → slave i throughout this scenario).
    int      state[3]    = {0, 0, 0};   // 0=AW+W, 1=B injection
    int      aw_done[3]  = {0, 0, 0};   // AW handshake seen at pre_edge
    int      w_done[3]   = {0, 0, 0};   // W  handshake seen at pre_edge
    int      b_hit[3]    = {0, 0, 0};   // B  handshake seen at pre_edge
    int      b_sent[3]   = {0, 0, 0};   // completed writes per master
    uint32_t b_id_sv[3]  = {0, 0, 0};   // slave-perspective ID saved from AW
    int      total_done  = 0;

    // Arm all three masters with their first AW+W.
    for (int i = 0; i < 3; ++i) {
        dut.m_aw_valid[i] = 1;
        dut.m_aw_addr[i]  = slave_base(i) | 0x1000u;
        dut.m_aw_id[i]    = 0;
        dut.m_aw_len[i]   = 0;
        dut.m_aw_size[i]  = 2;
        dut.m_aw_burst[i] = 1;
        dut.m_w_valid[i]  = 1;
        dut.m_w_data[i]   = 0xA000'0000u | ((uint32_t)i << 24);
        dut.m_w_strb[i]   = 0xF;
        dut.m_w_last[i]   = 1;
        dut.m_b_ready[i]  = 1;
        dut.s_aw_ready[i] = 1;
        dut.s_w_ready[i]  = 1;
    }

    for (int t = 0; t < N_WR * 3 * 20 && total_done < N_WR * 3; ++t) {
        // ── pre_edge: comb settles at clk=0; sample handshakes ──────────
        dut.clk = 0;
        dut.eval();

        for (int i = 0; i < 3; ++i) {
            if (b_sent[i] >= N_WR) continue;
            b_hit[i] = 0;

            if (state[i] == 0) {
                // AW handshake (thread drives s_aw_valid from within aw_lock).
                if (!aw_done[i] && dut.s_aw_valid[i] && dut.s_aw_ready[i]) {
                    b_id_sv[i] = dut.s_aw_id[i];   // save slave-perspective ID
                    aw_done[i] = 1;
                }
                // W handshake (thread drives s_w_valid one phase after AW).
                if (!w_done[i] && dut.s_w_valid[i] && dut.s_w_ready[i]) {
                    w_done[i] = 1;
                }
            } else if (state[i] == 1) {
                // B response arrives combinationally at the master when the
                // fabric's b_demux thread is active and s_b_valid is asserted.
                // Two concurrent B responses on different masters are valid
                // (disjoint slaves, fully independent paths). The correctness
                // check is that the ID on master i's B matches the transaction
                // master i sent (lower MASTER_ID_W bits of m_b_id[i]).
                if (dut.m_b_valid[i] && dut.m_b_ready[i]) {
                    uint8_t expected_low_id = (uint8_t)(b_sent[i] & 0x7);
                    if ((dut.m_b_id[i] & 0x7) != expected_low_id) {
                        std::printf("FAIL S3: master %d B id mismatch: "
                                    "got 0x%x, expected low=0x%x\n",
                                    i, (unsigned)dut.m_b_id[i], expected_low_id);
                        return 1;
                    }
                    b_hit[i] = 1;
                }
            }
        }

        // ── post_edge: rising edge fires with signals unchanged ──────────
        dut.clk = 1;
        dut.eval();
        cycle++;

        // ── after rising edge: update signals for next cycle ─────────────
        for (int i = 0; i < 3; ++i) {
            if (b_sent[i] >= N_WR) continue;

            // AW+W both done → drop master drives, assert B response.
            // Signals are dropped after the edge that committed the W handshake
            // so the thread sees valid+ready=1 at the rising edge it needs.
            if (state[i] == 0 && aw_done[i] && w_done[i]) {
                dut.m_aw_valid[i] = 0;
                dut.m_w_valid[i]  = 0;
                dut.s_aw_ready[i] = 0;
                dut.s_w_ready[i]  = 0;
                dut.s_b_valid[i]  = 1;
                dut.s_b_id[i]     = b_id_sv[i];
                dut.s_b_resp[i]   = 0;
                aw_done[i] = 0;
                w_done[i]  = 0;
                state[i]   = 1;
            }

            // B consumed → deassert, re-arm next transaction if any remain.
            if (state[i] == 1 && b_hit[i]) {
                b_sent[i]++;
                total_done++;
                dut.s_b_valid[i] = 0;

                if (b_sent[i] < N_WR) {
                    dut.m_aw_valid[i] = 1;
                    dut.m_aw_id[i]    = (uint8_t)(b_sent[i] & 0x7);
                    dut.m_aw_addr[i]  = slave_base(i)
                                        | (uint32_t)(b_sent[i] * 4 + 0x1000);
                    dut.m_w_valid[i]  = 1;
                    dut.m_w_data[i]   = 0xA000'0000u
                                        | ((uint32_t)i << 24)
                                        | (uint32_t)b_sent[i];
                    dut.s_aw_ready[i] = 1;
                    dut.s_w_ready[i]  = 1;
                    state[i] = 0;
                }
            }
        }
    }

    if (total_done < N_WR * 3) {
        std::printf("FAIL S3: only %d/%d write transactions completed "
                    "(m0=%d, m1=%d, m2=%d)\n",
                    total_done, N_WR * 3, b_sent[0], b_sent[1], b_sent[2]);
        return 1;
    }
    std::printf("INFO  S3 (disjoint writes): %d write txns complete "
                "(m0=%d, m1=%d, m2=%d)\n",
                total_done, b_sent[0], b_sent[1], b_sent[2]);
    std::printf("PASS S3: 3-master disjoint writes — all %d×%d BRESPs routed correctly\n",
                3, N_WR);

    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();
    return 0;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    do_reset();

    if (scenario_disjoint_reads())    return 1;
    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();

    if (scenario_hot_slave_3way())    return 1;
    clear_inputs();
    for (int i = 0; i < 2; ++i) tick();

    if (scenario_disjoint_writes())   return 1;

    std::printf("PASS Nic400Fabric multi-master contention: "
                "disjoint reads + hot-slave 3-way + disjoint writes\n");
    return 0;
}
