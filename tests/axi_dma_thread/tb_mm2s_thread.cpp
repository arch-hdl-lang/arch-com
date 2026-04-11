// Testbench for ThreadMm2s — same test as tb_mm2s_multi but with thread module.
// Issues 4 burst reads of 4 beats each (total_xfers=4, burst_len=4).

#include "VThreadMm2s.h"
#include <cassert>
#include <cstdio>

static VThreadMm2s dut;
static int cycle_count = 0;

void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

void reset() {
    dut.rst = 1;
    dut.start = 0;
    dut.ar_ready = 0;
    dut.r_valid = 0;
    dut.r_data = 0;
    dut.r_id = 0;
    dut.r_last = 0;
    dut.push_ready = 1;
    dut.total_xfers = 0;
    dut.base_addr = 0;
    dut.burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    dut.rst = 0;
    tick();
}

int main() {
    reset();

    printf("[cycle %3d] Idle: idle=%d done=%d\n", cycle_count, dut.idle_out, dut.done);

    // Start: 4 transfers, 4 beats each
    dut.start = 1;
    dut.total_xfers = 4;
    dut.base_addr = 0x1000;
    dut.burst_len = 4;
    tick();
    dut.start = 0;

    int ar_accepted = 0;
    int r_beats_sent = 0;
    int push_count = 0;

    for (int c = 0; c < 200; c++) {
        // AR slave: accept
        if (dut.ar_valid) {
            dut.ar_ready = 1;
            printf("[cycle %3d] AR: id=%d addr=0x%x len=%d idle=%d done=%d\n",
                   cycle_count, dut.ar_id, dut.ar_addr, dut.ar_len + 1,
                   dut.idle_out, dut.done);
            ar_accepted++;
        } else {
            dut.ar_ready = 0;
        }

        // R slave: send beats for accepted ARs
        if (ar_accepted > 0 && r_beats_sent < ar_accepted * 4 && dut.r_ready) {
            int burst = r_beats_sent / 4;
            int beat = r_beats_sent % 4;
            dut.r_valid = 1;
            dut.r_data = (burst << 24) | (0x1000 + r_beats_sent * 4);
            dut.r_id = burst % 4;
            dut.r_last = (beat == 3) ? 1 : 0;
            r_beats_sent++;
        } else {
            dut.r_valid = 0;
        }

        // Count FIFO pushes
        if (dut.push_valid && dut.push_ready) push_count++;

        tick();

        if (dut.done) {
            printf("[cycle %3d] DONE! AR=%d R=%d push=%d\n",
                   cycle_count, ar_accepted, r_beats_sent, push_count);
            break;
        }
    }

    printf("Results: AR=%d, R=%d, push=%d, done=%d\n",
           ar_accepted, r_beats_sent, push_count, dut.done);

    if (ar_accepted >= 1 && r_beats_sent >= 4 && push_count >= 4) {
        printf("PASS: ThreadMm2s basic test\n");
    } else {
        printf("FAIL: ThreadMm2s basic test\n");
        return 1;
    }
    return 0;
}
