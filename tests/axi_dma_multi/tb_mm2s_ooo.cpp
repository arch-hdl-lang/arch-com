// Testbench for FsmMm2sMulti — out-of-order R response completion.
//
// Issues 4 burst reads (total_xfers=4, burst_len=2, IDs 0-3).
// AXI slave responds with R beats in REVERSE order:
//   ID 3 first, then ID 2, ID 1, ID 0.
// This exercises out-of-order completion across outstanding transactions.

#include "VFsmMm2sMulti.h"
#include <cassert>
#include <cstdio>
#include <vector>
#include <deque>

static VFsmMm2sMulti dut;
static int cycle_count = 0;

struct RBeat {
    uint32_t data;
    uint32_t id;
    bool     last;
};

// Per-ID response queues (responses generated per-ID, served in reverse ID order)
static std::vector<std::deque<RBeat>> id_queues;
static int serve_order[] = {3, 2, 1, 0};  // reverse order
static int serve_idx = 0;

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
    id_queues.resize(4);
    reset();

    assert(dut.idle_out == 1);
    printf("[cycle %3d] Idle OK\n", cycle_count);

    // Start: 4 transfers, 2 beats each
    dut.start = 1;
    dut.total_xfers = 4;
    dut.base_addr = 0x1000;
    dut.burst_len = 2;
    tick();
    dut.start = 0;

    int ar_accepted = 0;
    int r_beats_sent = 0;
    int push_count = 0;
    std::vector<uint32_t> pushed_data;

    // Phase 1: Accept all 4 AR requests (they should be issued rapidly)
    for (int c = 0; c < 50; c++) {
        if (dut.ar_valid && ar_accepted < 4) {
            dut.ar_ready = 1;
            uint32_t id = dut.ar_id;
            uint32_t addr = dut.ar_addr;
            uint32_t len = dut.ar_len + 1;
            printf("[cycle %3d] AR accepted: id=%d addr=0x%x len=%d\n",
                   cycle_count, id, addr, len);

            // Queue R beats for this ID
            for (uint32_t b = 0; b < len; b++) {
                RBeat beat;
                beat.data = (id << 16) | (b & 0xFFFF);
                beat.id = id;
                beat.last = (b == len - 1);
                id_queues[id].push_back(beat);
            }
            ar_accepted++;
        } else {
            dut.ar_ready = 0;
        }

        // Don't send R yet — let all AR accumulate first
        dut.r_valid = 0;
        tick();

        if (ar_accepted == 4) break;
    }

    printf("\n[cycle %3d] All 4 AR accepted. Now sending R in REVERSE ID order.\n\n",
           cycle_count);
    assert(ar_accepted == 4);
    dut.ar_ready = 0;

    // Phase 2: Send R beats in reverse ID order (3, 2, 1, 0)
    // Each ID has 2 beats. Complete one ID fully before moving to next.
    for (int c = 0; c < 100; c++) {
        // Find next ID to serve
        while (serve_idx < 4 && id_queues[serve_order[serve_idx]].empty()) {
            serve_idx++;
        }
        if (serve_idx >= 4) {
            dut.r_valid = 0;
            tick();
            if (dut.done) break;
            continue;
        }

        int cur_id = serve_order[serve_idx];
        auto &q = id_queues[cur_id];

        if (!q.empty() && dut.r_ready) {
            RBeat &beat = q.front();
            dut.r_valid = 1;
            dut.r_data = beat.data;
            dut.r_id = beat.id;
            dut.r_last = beat.last ? 1 : 0;

            printf("[cycle %3d] R: id=%d data=0x%08x last=%d\n",
                   cycle_count, beat.id, beat.data, beat.last);
            r_beats_sent++;
            q.pop_front();
        } else {
            dut.r_valid = 0;
        }

        tick();

        // Count pushes after tick (settle evaluates push_valid from the R data)
        if (dut.push_valid && dut.push_ready) {
            pushed_data.push_back(dut.push_data);
            push_count++;
        }

        if (dut.done) {
            printf("\n[cycle %3d] DONE!\n", cycle_count);
            break;
        }
    }

    // Drain: give FSM time to transition to Done
    dut.r_valid = 0;
    for (int i = 0; i < 10; i++) {
        if (dut.done) {
            printf("[cycle %3d] DONE\n", cycle_count);
            break;
        }
        tick();
    }
    for (int i = 0; i < 5; i++) {
        if (dut.push_valid && dut.push_ready) {
            pushed_data.push_back(dut.push_data);
            push_count++;
        }
        tick();
    }

    printf("\nResults: AR=%d, R beats=%d, pushes=%d\n",
           ar_accepted, r_beats_sent, push_count);

    // Verify counts (done was a 1-cycle pulse, already captured above)
    assert(ar_accepted == 4);
    assert(r_beats_sent == 8);  // 4 xfers * 2 beats
    assert(push_count == 8);

    // Verify data arrived — should be in R response order (reverse ID):
    // ID3 beat0, ID3 beat1, ID2 beat0, ID2 beat1, ID1 beat0, ID1 beat1, ID0 beat0, ID0 beat1
    printf("\nPushed data (in order received):\n");
    uint32_t expected[] = {
        0x00030000, 0x00030001,  // ID 3
        0x00020000, 0x00020001,  // ID 2
        0x00010000, 0x00010001,  // ID 1
        0x00000000, 0x00000001,  // ID 0
    };
    for (size_t i = 0; i < pushed_data.size(); i++) {
        printf("  [%zu] 0x%08x (expected 0x%08x) %s\n",
               i, pushed_data[i], expected[i],
               pushed_data[i] == expected[i] ? "OK" : "MISMATCH");
    }

    for (size_t i = 0; i < 8; i++) {
        assert(pushed_data[i] == expected[i]);
    }

    // Back to idle
    tick();
    assert(dut.idle_out == 1);

    printf("\nPASS: FsmMm2sMulti — 4 outstanding, out-of-order completion (reverse ID)\n");
    return 0;
}
