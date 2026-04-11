// Read performance benchmark for FsmMm2sMulti.
// Same test patterns as thread version for direct comparison.

#include "VFsmMm2sMulti.h"
#include <cstdio>

static VFsmMm2sMulti dut;
static int cycle_count = 0;

void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

void reset() {
    dut.rst = 1; dut.start = 0; dut.ar_ready = 1;
    dut.r_valid = 0; dut.r_data = 0; dut.r_id = 0; dut.r_last = 0;
    dut.push_ready = 1;
    dut.total_xfers = 0; dut.base_addr = 0; dut.burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    dut.rst = 0; tick();
    cycle_count = 0;
}

void bench_read(int total_xfers, int burst_len) {
    reset();
    int total_beats = total_xfers * burst_len;

    dut.start = 1; dut.total_xfers = total_xfers; dut.base_addr = 0;
    dut.burst_len = burst_len;
    tick(); dut.start = 0;

    int start_cycle = cycle_count;
    int ar_count = 0;
    int r_beats = 0;
    int r_active_cycles = 0;
    int first_r_cycle = -1;

    dut.ar_ready = 1;
    int pending_r = 0;
    int r_burst_id = 0;
    int r_beat_in_burst = 0;

    for (int c = 0; c < total_beats * 4 + 50; c++) {
        // Present R beats
        if (pending_r > 0) {
            dut.r_valid = 1;
            dut.r_data = r_beats;
            dut.r_id = r_burst_id % 4;
            dut.r_last = (r_beat_in_burst == burst_len - 1) ? 1 : 0;
        } else {
            dut.r_valid = 0;
        }

        dut.eval();

        // Check AR handshake
        if (dut.ar_valid && dut.ar_ready) {
            ar_count++;
            pending_r += burst_len;
        }

        // Check R handshake
        if (dut.r_valid && dut.r_ready) {
            r_beat_in_burst++;
            if (r_beat_in_burst == burst_len) {
                r_beat_in_burst = 0;
                r_burst_id++;
            }
            pending_r--;
            r_beats++;
            r_active_cycles++;
            if (first_r_cycle < 0) first_r_cycle = cycle_count;
        }

        tick();
        if (dut.done) break;
    }

    int end_cycle = cycle_count;
    int total_cycles = end_cycle - start_cycle;
    int data_cycles = (first_r_cycle >= 0) ? end_cycle - first_r_cycle : 1;
    double utilization = (double)r_active_cycles / total_cycles * 100.0;
    double data_util = (double)r_active_cycles / data_cycles * 100.0;
    int startup_lat = (first_r_cycle >= 0) ? first_r_cycle - start_cycle : -1;

    printf("  READ  %2dx%3d: %4d beats in %3d cyc, util=%.1f%%, "
           "data-phase util=%.1f%%, startup=%d cyc\n",
           total_xfers, burst_len, total_beats, total_cycles,
           utilization, data_util, startup_lat);
}

int main() {
    printf("=== FSM MM2S Read Performance Benchmark ===\n");
    printf("    (NUM_OUTSTANDING=4, ideal=1 beat/cycle=100%%)\n\n");

    bench_read(1, 4);
    bench_read(1, 16);
    bench_read(1, 64);
    bench_read(4, 4);
    bench_read(4, 16);
    bench_read(4, 64);
    bench_read(8, 4);
    bench_read(8, 16);
    bench_read(16, 16);

    printf("\n=== DONE ===\n");
    return 0;
}
