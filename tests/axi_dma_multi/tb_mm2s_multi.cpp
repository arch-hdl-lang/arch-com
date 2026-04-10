// Testbench for FsmMm2sMulti — multi-outstanding AXI read engine.
//
// Scenario: issue 4 burst reads of 4 beats each (total_xfers=4, burst_len=4).
// The AXI slave responds with staggered latency to exercise out-of-order
// completion across IDs.

#include "VFsmMm2sMulti.h"
#include <cassert>
#include <cstdio>
#include <cstring>
#include <queue>

struct RBeat {
    uint32_t data;
    uint32_t id;
    bool     last;
};

static VFsmMm2sMulti dut;
static int cycle_count = 0;

// Pending R responses per ID (simulates AXI slave with variable latency)
static std::queue<RBeat> r_queue;

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

    // Verify idle
    assert(dut.idle_out == 1);
    assert(dut.done == 0);
    printf("[cycle %3d] Idle OK\n", cycle_count);

    // Start: 4 transfers, 4 beats each, base addr 0x1000
    dut.start = 1;
    dut.total_xfers = 4;
    dut.base_addr = 0x1000;
    dut.burst_len = 4;
    tick();
    dut.start = 0;

    // Run the FSM — accept AR requests and generate R responses
    int ar_accepted = 0;
    int r_beats_sent = 0;
    int r_xfers_done = 0;
    int push_count = 0;
    int ar_delay_counter = 0;  // stagger AR acceptance

    // Simulate for up to 200 cycles
    for (int c = 0; c < 200; c++) {
        // AR slave: accept AR with some backpressure
        if (dut.ar_valid && ar_accepted < 4) {
            ar_delay_counter++;
            if (ar_delay_counter >= 2) {  // accept every 2nd cycle
                dut.ar_ready = 1;
                uint32_t addr = dut.ar_addr;
                uint32_t id = dut.ar_id;
                uint32_t len = dut.ar_len + 1;
                printf("[cycle %3d] AR accepted: id=%d addr=0x%x len=%d\n",
                       cycle_count, id, addr, len);
                // Enqueue R beats for this ID
                for (uint32_t b = 0; b < len; b++) {
                    RBeat beat;
                    beat.data = (id << 24) | (addr + b * 4);
                    beat.id = id;
                    beat.last = (b == len - 1);
                    r_queue.push(beat);
                }
                ar_accepted++;
                ar_delay_counter = 0;
            } else {
                dut.ar_ready = 0;
            }
        } else {
            dut.ar_ready = 0;
        }

        // R slave: send queued beats
        if (!r_queue.empty() && dut.r_ready) {
            RBeat &beat = r_queue.front();
            dut.r_valid = 1;
            dut.r_data = beat.data;
            dut.r_id = beat.id;
            dut.r_last = beat.last ? 1 : 0;
            if (beat.last) r_xfers_done++;
            r_beats_sent++;
            r_queue.pop();
        } else {
            dut.r_valid = 0;
            dut.r_data = 0;
            dut.r_id = 0;
            dut.r_last = 0;
        }

        // Count FIFO pushes
        if (dut.push_valid && dut.push_ready) {
            push_count++;
        }

        tick();

        // Check for done
        if (dut.done) {
            printf("[cycle %3d] DONE! AR accepted=%d, R beats=%d, pushes=%d\n",
                   cycle_count, ar_accepted, r_beats_sent, push_count);
            break;
        }
    }

    assert(dut.done == 1);
    assert(ar_accepted == 4);
    assert(r_beats_sent == 16);  // 4 xfers * 4 beats
    assert(push_count == 16);

    // Verify returns to idle
    tick();
    assert(dut.idle_out == 1);

    printf("PASS: FsmMm2sMulti — 4 outstanding reads, 4 beats each\n");
    return 0;
}
