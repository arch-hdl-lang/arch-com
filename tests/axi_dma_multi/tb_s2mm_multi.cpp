// Testbench for FsmS2mmMulti — multi-outstanding AXI write engine.
//
// Scenario: 4 burst writes of 4 beats each (total_xfers=4, burst_len=4).
// AW is accepted immediately, W with some backpressure,
// B responses arrive with staggered latency.

#include "VFsmS2mmMulti.h"
#include <cassert>
#include <cstdio>
#include <queue>

static VFsmS2mmMulti dut;
static int cycle_count = 0;

// Pending B responses (simulates AXI slave with latency)
struct BResp { uint32_t id; int delay; };
static std::queue<BResp> b_queue;

void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cycle_count++;
}

void reset() {
    dut.rst = 1;
    dut.start = 0;
    dut.aw_ready = 0;
    dut.w_ready = 0;
    dut.b_valid = 0;
    dut.b_id = 0;
    dut.pop_valid = 1;  // FIFO always has data
    dut.pop_data = 0xBEEF;
    dut.total_xfers = 0;
    dut.base_addr = 0;
    dut.burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    dut.rst = 0;
    tick();
}

int main() {
    reset();

    assert(dut.idle_out == 1);
    printf("[cycle %3d] Idle OK\n", cycle_count);

    // Start: 4 transfers, 4 beats each
    dut.start = 1;
    dut.total_xfers = 4;
    dut.base_addr = 0x2000;
    dut.burst_len = 4;
    tick();
    dut.start = 0;

    int aw_accepted = 0;
    int w_beats = 0;
    int b_sent = 0;
    int b_delay_ctr = 0;

    for (int c = 0; c < 300; c++) {
        // AW slave: accept immediately
        dut.aw_ready = dut.aw_valid ? 1 : 0;
        if (dut.aw_valid && dut.aw_ready) {
            printf("[cycle %3d] AW accepted: id=%d addr=0x%x len=%d\n",
                   cycle_count, dut.aw_id, dut.aw_addr, dut.aw_len + 1);
            // Queue B response with 3-cycle delay
            b_queue.push({(uint32_t)dut.aw_id, 3});
            aw_accepted++;
        }

        // W slave: accept with some backpressure
        dut.w_ready = (c % 3 != 0) ? 1 : 0;  // skip every 3rd cycle
        if (dut.w_valid && dut.w_ready) {
            w_beats++;
            if (dut.w_last) {
                printf("[cycle %3d] W burst complete (beat %d)\n", cycle_count, w_beats);
            }
        }

        // B slave: send responses after delay
        dut.b_valid = 0;
        if (!b_queue.empty()) {
            BResp &resp = b_queue.front();
            resp.delay--;
            if (resp.delay <= 0 && dut.b_ready) {
                dut.b_valid = 1;
                dut.b_id = resp.id;
                b_queue.pop();
                b_sent++;
                printf("[cycle %3d] B response: id=%d\n", cycle_count, dut.b_id);
            }
        }

        // FIFO: always has data with incrementing pattern
        dut.pop_valid = 1;
        dut.pop_data = 0xD000 + w_beats;

        tick();

        if (dut.done) {
            printf("[cycle %3d] DONE! AW=%d W=%d B=%d\n",
                   cycle_count, aw_accepted, w_beats, b_sent);
            break;
        }
    }

    assert(dut.done == 1);
    assert(aw_accepted == 4);
    assert(w_beats == 16);
    assert(b_sent == 4);

    tick();
    assert(dut.idle_out == 1);

    printf("PASS: FsmS2mmMulti — 4 outstanding writes, 4 beats each\n");
    return 0;
}
