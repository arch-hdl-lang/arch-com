// Testbench for FsmS2mmMulti — out-of-order B response completion.
//
// Issues 4 burst writes (total_xfers=4, burst_len=2, IDs 0-3).
// AW is accepted immediately. W beats have some backpressure.
// B responses arrive in REVERSE order: ID 3 first, then 2, 1, 0.
// This exercises out-of-order write completion across outstanding transactions.

#include "VFsmS2mmMulti.h"
#include <cassert>
#include <cstdio>
#include <vector>
#include <deque>

static VFsmS2mmMulti dut;
static int cycle_count = 0;

struct BResp { uint32_t id; };

// B responses queued per AW, served in reverse ID order
static std::deque<BResp> b_pending;
static std::vector<int> b_serve_order;
static int b_serve_idx = 0;
static bool all_aw_done = false;

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
    dut.pop_valid = 1;
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

    // Start: 4 transfers, 2 beats each
    dut.start = 1;
    dut.total_xfers = 4;
    dut.base_addr = 0x2000;
    dut.burst_len = 2;
    tick();
    dut.start = 0;

    int aw_accepted = 0;
    int w_beats = 0;
    int w_bursts_done = 0;
    int b_sent = 0;

    // Track AW IDs in order issued, then reverse for B
    std::vector<uint32_t> aw_ids;

    for (int c = 0; c < 400; c++) {
        // AW slave: accept immediately
        if (dut.aw_valid) {
            dut.aw_ready = 1;
            uint32_t id = dut.aw_id;
            printf("[cycle %3d] AW accepted: id=%d addr=0x%x len=%d\n",
                   cycle_count, id, dut.aw_addr, dut.aw_len + 1);
            aw_ids.push_back(id);
            aw_accepted++;
        } else {
            dut.aw_ready = 0;
        }

        // W slave: accept with backpressure (skip every 4th cycle)
        dut.w_ready = (c % 4 != 0) ? 1 : 0;
        if (dut.w_valid && dut.w_ready) {
            w_beats++;
            if (dut.w_last) {
                w_bursts_done++;
                printf("[cycle %3d] W burst %d complete (total w_beats=%d)\n",
                       cycle_count, w_bursts_done, w_beats);
            }
        }

        // FIFO: always has data with pattern
        dut.pop_valid = 1;
        dut.pop_data = 0xDA00 + w_beats;

        // B slave: once all W bursts done, send B in REVERSE ID order
        dut.b_valid = 0;
        if (w_bursts_done == 4 && !all_aw_done) {
            all_aw_done = true;
            // Queue B responses in reverse order
            for (int i = (int)aw_ids.size() - 1; i >= 0; i--) {
                b_pending.push_back({aw_ids[i]});
            }
            printf("\n[cycle %3d] All W bursts done. Sending B in REVERSE order.\n\n",
                   cycle_count);
        }
        if (!b_pending.empty() && dut.b_ready) {
            dut.b_valid = 1;
            dut.b_id = b_pending.front().id;
            printf("[cycle %3d] B response: id=%d\n", cycle_count, dut.b_id);
            b_pending.pop_front();
            b_sent++;
        }

        tick();

        if (dut.done) {
            printf("\n[cycle %3d] DONE!\n", cycle_count);
            break;
        }
    }

    // Drain
    for (int i = 0; i < 5; i++) {
        if (dut.done) break;
        tick();
    }

    printf("\nResults: AW=%d, W beats=%d, W bursts=%d, B=%d\n",
           aw_accepted, w_beats, w_bursts_done, b_sent);

    printf("AW IDs issued: ");
    for (auto id : aw_ids) printf("%d ", id);
    printf("\nB responses sent in reverse: ");
    // Already sent, just confirm count
    printf("(count=%d)\n", b_sent);

    assert(aw_accepted == 4);
    assert(w_beats == 8);      // 4 xfers * 2 beats
    assert(w_bursts_done == 4);
    assert(b_sent == 4);

    printf("\nPASS: FsmS2mmMulti — 4 outstanding writes, B responses out-of-order (reverse)\n");
    return 0;
}
