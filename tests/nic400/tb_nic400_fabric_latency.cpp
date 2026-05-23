// Cycle-accurate latency probe for the NIC-400 v2 Fabric.
//
// Measures:
//   1. AR forward latency = cycles from m_0_ar_valid rising to s_0_ar_valid
//      (with s_0_ar_ready held high — no contention, no register slices).
//   2. R return latency   = cycles from s_0_r_valid rising to m_0_r_valid
//      (with m_0_r_ready held high).
//
// Both latencies SHOULD be 0 by the spec (§14.1: "AR → S AW: 0 cycles
// (no slices). The master decoder is pure comb; slave AW lock acquires
// in 0 cycles when uncontested."). Any non-zero latency = a thread-FSM
// bubble that the spec didn't account for.
//
// The checker fails with a diagnostic if observed latency exceeds the
// configured `MAX_LAT_AR` / `MAX_LAT_R` thresholds, so the test sounds
// the alarm when a future change to the lowering pipeline adds extra
// states.

#include "VNic400Fabric.h"
#include <cstdint>
#include <cstdio>

// Forward declared so we don't need to include `verilated.h` here — the
// model's header already pulls in the shim. The runner passes
// `+trace+<path>` for `--wave`; this main() must hand argc/argv to the
// shim so the model picks up the trace filename.
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
    dut.m_0_ar_valid = 0; dut.m_0_ar_addr = 0; dut.m_0_ar_id = 0;
    dut.m_0_ar_len = 0;   dut.m_0_ar_size = 0; dut.m_0_ar_burst = 0;
    dut.m_0_r_ready = 0;
    dut.m_1_ar_valid = 0; dut.m_1_ar_addr = 0; dut.m_1_ar_id = 0;
    dut.m_1_ar_len = 0;   dut.m_1_ar_size = 0; dut.m_1_ar_burst = 0;
    dut.m_1_r_ready = 0;
    dut.s_0_ar_ready = 0;
    dut.s_0_r_valid = 0;  dut.s_0_r_data = 0; dut.s_0_r_id = 0;
    dut.s_0_r_resp = 0;   dut.s_0_r_last = 0;
    dut.s_1_ar_ready = 0;
    dut.s_1_r_valid = 0;  dut.s_1_r_data = 0; dut.s_1_r_id = 0;
    dut.s_1_r_resp = 0;   dut.s_1_r_last = 0;
}

int main(int argc, char **argv) {
    // Forward argv so `+trace+<path>` lands in the shim's commandArgs
    // and `dut.eval()` opens the VCD file on the first call.
    Verilated::commandArgs(argc, argv);

    // Tunable thresholds — set to the spec's stated value (0). Bumped if
    // the design intentionally adds register slices; today both are 0.
    // Observed latencies for the thread-based MasterPort/SlavePort design:
    //
    // Observed latencies for the thread-based MasterPort/SlavePort design
    // after the `wait 0+ cycle until` Mealy upgrade — matches the spec
    // §14.1 0-cycle pass-through (and 1 txn/cycle sustained throughput
    // probed below). Any future regression that re-introduces a bubble
    // (e.g. switching back to standard `wait until`, or inserting a
    // register slice) fails this checker loudly.
    const int MAX_LAT_AR = 0;
    const int MAX_LAT_R  = 0;

    // Reset (active-low)
    dut.rst = 0;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    // A few idle cycles to let any post-reset state churn settle.
    for (int i = 0; i < 3; ++i) tick();

    // ── AR forward latency ───────────────────────────────────────────────
    // Hold s_0_ar_ready high so the slave isn't a back-pressure source.
    // Then assert m_0_ar_valid and count how many ticks until s_0_ar_valid
    // is observed handshaked with s_0_ar_ready.
    dut.s_0_ar_ready = 1;
    dut.m_0_ar_addr = 0x00001000;
    dut.m_0_ar_id   = 1;
    dut.m_0_ar_size = 2;
    dut.m_0_ar_burst = 1;

    uint64_t t_ar_drive = cycle;
    dut.m_0_ar_valid = 1;

    int lat_ar = -1;
    for (int i = 0; i <= MAX_LAT_AR + 8; ++i) {
        tick();
        if (dut.s_0_ar_valid && dut.s_0_ar_ready) {
            lat_ar = (int)(cycle - 1 - t_ar_drive);
            break;
        }
    }
    if (lat_ar < 0) {
        std::printf("FAIL Latency: AR never propagated to slave\n");
        return 1;
    }
    std::printf("INFO  AR forward latency: %d cycle(s) (cycle %llu → %llu)\n",
                lat_ar, (unsigned long long)t_ar_drive,
                (unsigned long long)(t_ar_drive + lat_ar));
    if (lat_ar > MAX_LAT_AR) {
        std::printf("FAIL Latency: AR forward bubble — observed %d cycle(s), max %d\n",
                    lat_ar, MAX_LAT_AR);
        return 1;
    }

    // Clean up the AR phase.
    dut.m_0_ar_valid = 0;
    dut.s_0_ar_ready = 0;
    for (int i = 0; i < 2; ++i) tick();

    // ── R return latency ────────────────────────────────────────────────
    // Slave drives R, master holds ready. Count cycles until master sees R.
    dut.m_0_r_ready = 1;
    dut.s_0_r_data  = 0xDEADBEEF;
    dut.s_0_r_id    = 1;          // {master_idx=0, master_id=1} = 1
    dut.s_0_r_resp  = 0;
    dut.s_0_r_last  = 1;

    uint64_t t_r_drive = cycle;
    dut.s_0_r_valid = 1;

    int lat_r = -1;
    for (int i = 0; i <= MAX_LAT_R + 8; ++i) {
        tick();
        if (dut.m_0_r_valid && dut.m_0_r_ready) {
            lat_r = (int)(cycle - 1 - t_r_drive);
            break;
        }
    }
    if (lat_r < 0) {
        std::printf("FAIL Latency: R never propagated to master\n");
        return 1;
    }
    std::printf("INFO  R return latency:  %d cycle(s) (cycle %llu → %llu)\n",
                lat_r, (unsigned long long)t_r_drive,
                (unsigned long long)(t_r_drive + lat_r));
    if (lat_r > MAX_LAT_R) {
        std::printf("FAIL Latency: R return bubble — observed %d cycle(s), max %d\n",
                    lat_r, MAX_LAT_R);
        return 1;
    }

    // ── AR throughput: back-to-back transactions ───────────────────────
    // Spec §14.2 quotes "1 transfer/cycle (sustained)" once the pipeline is
    // warm. With Mealy fusion (`wait 0+ cycle until` + `do..until`), every
    // posedge with valid && ready fires a handshake — no entry-wait state
    // bubble. Pin to exactly 1.0 t/c: any future regression that
    // re-introduces a bubble (e.g. accidentally switching to standard
    // `wait until`, or inserting a register slice) fails this checker.
    const int MIN_THROUGHPUT_NUM   = 1;
    const int MIN_THROUGHPUT_DENOM = 1;
    clear_inputs();
    for (int i = 0; i < 4; ++i) tick();

    const int N_TXN = 9;
    dut.s_0_ar_ready = 1;          // Slave always ready
    dut.m_0_ar_addr  = 0x00001000;
    dut.m_0_ar_size  = 2;
    dut.m_0_ar_burst = 1;
    dut.m_0_ar_valid = 1;          // Master always presents a request

    int seen = 0;
    int last_seen_cycle = -1;
    uint64_t t_start = cycle;
    for (int i = 0; i < N_TXN * 8 && seen < N_TXN; ++i) {
        dut.m_0_ar_id = (uint8_t)(seen + 1);
        tick();
        if (dut.s_0_ar_valid && dut.s_0_ar_ready) {
            seen++;
            last_seen_cycle = (int)(cycle - t_start);
        }
    }
    if (seen < N_TXN) {
        std::printf("FAIL Throughput: only %d/%d AR transfers completed in %d cycles\n",
                    seen, N_TXN, last_seen_cycle);
        return 1;
    }
    // last_seen_cycle is 1-indexed: first tick that fires a handshake gives
    // last_seen_cycle = 1. If `seen` handshakes complete with the last on
    // tick K, the back-to-back window is exactly K cycles wide (ticks
    // 1..K), so total_cycles = last_seen_cycle (no +1).
    int total_cycles = last_seen_cycle;
    std::printf("INFO  AR throughput: %d transfers in %d cycles = %.2f transfers/cycle\n",
                seen, total_cycles, (double)seen / total_cycles);
    // Cross-multiply to avoid float: seen * DENOM >= total_cycles * NUM
    if (seen * MIN_THROUGHPUT_DENOM < total_cycles * MIN_THROUGHPUT_NUM) {
        std::printf("FAIL Throughput: AR < %d/%d transfers/cycle (observed %d/%d)\n",
                    MIN_THROUGHPUT_NUM, MIN_THROUGHPUT_DENOM, seen, total_cycles);
        return 1;
    }

    std::printf("PASS Nic400Fabric perf: AR=%d cyc, R=%d cyc, AR throughput %d/%d cyc\n",
                lat_ar, lat_r, seen, total_cycles);
    return 0;
}
