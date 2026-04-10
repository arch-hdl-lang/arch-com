// Performance benchmark for multi-outstanding AXI DMA FSMs.
//
// Measures:
//   1. Read bandwidth: cycles to transfer N beats, utilization %
//   2. Write bandwidth: cycles to transfer N beats, utilization %
//   3. Startup latency: cycles from start to first data beat
//   4. Pipeline efficiency: dead cycles between bursts
//
// Ideal: 1 data beat per cycle = 100% utilization

#include "VFsmMm2sMulti.h"
#include "VFsmS2mmMulti.h"
#include <cassert>
#include <cstdio>

static VFsmMm2sMulti rd;
static VFsmS2mmMulti wr;
static int cycle_count = 0;

void tick() {
    rd.clk = 0; rd.eval(); wr.clk = 0; wr.eval();
    rd.clk = 1; rd.eval(); wr.clk = 1; wr.eval();
    cycle_count++;
}

void reset_all() {
    rd.rst = 1; wr.rst = 1;
    rd.start = 0; wr.start = 0;
    rd.ar_ready = 1; rd.r_valid = 0; rd.r_data = 0; rd.r_id = 0; rd.r_last = 0;
    rd.push_ready = 1;
    wr.aw_ready = 1; wr.w_ready = 1; wr.b_valid = 0; wr.b_id = 0;
    wr.pop_valid = 1; wr.pop_data = 0;
    for (int i = 0; i < 5; i++) tick();
    rd.rst = 0; wr.rst = 0;
    tick();
    cycle_count = 0;  // Reset counter after init
}

// ═══════════════════════════════════════════════════════════════════
// Read benchmark: measure R-channel utilization
// ═══════════════════════════════════════════════════════════════════
void bench_read(int total_xfers, int burst_len) {
    reset_all();
    int total_beats = total_xfers * burst_len;

    rd.start = 1; rd.total_xfers = total_xfers; rd.base_addr = 0;
    rd.burst_len = burst_len;
    tick(); rd.start = 0;

    int start_cycle = cycle_count;
    int ar_count = 0;
    int r_beats = 0;
    int r_active_cycles = 0;      // cycles where R data transferred
    int first_r_cycle = -1;

    // Immediate AR accept + zero-latency R response
    rd.ar_ready = 1;
    int pending_r = 0;             // R beats queued per burst
    int r_burst_id = 0;
    int r_beat_in_burst = 0;

    for (int c = 0; c < total_beats * 4 + 50; c++) {
        // Accept AR and queue R beats
        if (rd.ar_valid) {
            ar_count++;
            pending_r += burst_len;
        }

        // Send R beats as fast as possible
        if (pending_r > 0 && rd.r_ready) {
            rd.r_valid = 1;
            rd.r_data = r_beats;
            rd.r_id = r_burst_id % 4;
            rd.r_last = (r_beat_in_burst == burst_len - 1) ? 1 : 0;
            r_beat_in_burst++;
            if (r_beat_in_burst == burst_len) {
                r_beat_in_burst = 0;
                r_burst_id++;
            }
            pending_r--;
            r_beats++;
            r_active_cycles++;
            if (first_r_cycle < 0) first_r_cycle = cycle_count;
        } else {
            rd.r_valid = 0;
        }

        tick();
        if (rd.done) break;
    }

    int end_cycle = cycle_count;
    int total_cycles = end_cycle - start_cycle;
    int data_cycles = end_cycle - first_r_cycle;
    double utilization = (double)r_active_cycles / total_cycles * 100.0;
    double data_util = (double)r_active_cycles / data_cycles * 100.0;
    int startup_lat = first_r_cycle - start_cycle;

    printf("  READ  %2dx%3d: %4d beats in %3d cyc, util=%.1f%%, "
           "data-phase util=%.1f%%, startup=%d cyc\n",
           total_xfers, burst_len, total_beats, total_cycles,
           utilization, data_util, startup_lat);
}

// ═══════════════════════════════════════════════════════════════════
// Write benchmark: measure W-channel utilization
// ═══════════════════════════════════════════════════════════════════
void bench_write(int total_xfers, int burst_len) {
    reset_all();
    int total_beats = total_xfers * burst_len;

    wr.start = 1; wr.total_xfers = total_xfers; wr.base_addr = 0;
    wr.burst_len = burst_len;
    tick(); wr.start = 0;

    int start_cycle = cycle_count;
    int w_beats = 0;
    int w_active_cycles = 0;
    int first_w_cycle = -1;
    int b_sent = 0;
    int w_bursts_done = 0;

    // Immediate AW accept, instant W accept, delayed B
    wr.aw_ready = 1; wr.w_ready = 1;
    wr.pop_valid = 1;

    for (int c = 0; c < total_beats * 4 + 50; c++) {
        wr.pop_data = w_beats;

        // Count W beats (before tick)
        if (wr.w_valid && wr.w_ready) {
            w_beats++;
            w_active_cycles++;
            if (first_w_cycle < 0) first_w_cycle = cycle_count;
        }

        // Track W bursts complete (w_last seen)
        if (wr.w_valid && wr.w_ready && wr.w_last) w_bursts_done++;

        // Zero-latency B: send on same cycle as w_last (ideal slave)
        if (wr.b_ready && b_sent < w_bursts_done) {
            wr.b_valid = 1; wr.b_id = 0;
            b_sent++;
        } else {
            wr.b_valid = 0;
        }

        tick();
        if (wr.done) break;
    }

    int end_cycle = cycle_count;
    int total_cycles = end_cycle - start_cycle;
    int data_cycles = (first_w_cycle >= 0) ? end_cycle - first_w_cycle : 1;
    double utilization = (double)w_active_cycles / total_cycles * 100.0;
    double data_util = (double)w_active_cycles / data_cycles * 100.0;
    int startup_lat = (first_w_cycle >= 0) ? first_w_cycle - start_cycle : -1;

    printf("  WRITE %2dx%3d: %4d beats in %3d cyc, util=%.1f%%, "
           "data-phase util=%.1f%%, startup=%d cyc\n",
           total_xfers, burst_len, total_beats, total_cycles,
           utilization, data_util, startup_lat);
}

int main() {
    printf("=== Multi-Outstanding DMA Performance Benchmark ===\n");
    printf("    (NUM_OUTSTANDING=4, ideal=1 beat/cycle=100%%)\n\n");

    printf("--- Read (MM2S) ---\n");
    bench_read(1, 4);      // Single burst
    bench_read(1, 16);
    bench_read(1, 64);
    bench_read(4, 4);      // 4 outstanding, short bursts
    bench_read(4, 16);
    bench_read(4, 64);
    bench_read(8, 4);      // 8 xfers (needs 2 rounds of 4)
    bench_read(8, 16);
    bench_read(16, 16);

    printf("\n--- Write (S2MM) ---\n");
    bench_write(1, 4);
    bench_write(1, 16);
    bench_write(1, 64);
    bench_write(4, 4);
    bench_write(4, 16);
    bench_write(4, 64);
    bench_write(8, 4);
    bench_write(8, 16);
    bench_write(16, 16);

    printf("\n=== DONE ===\n");
    return 0;
}
