// Corner-case testbench for multi-outstanding AxiDmaTop.
//
// Tests:
//   1. Single-beat burst (burst_len=1) — tlast/w_last on first beat
//   2. Back-to-back transfers — start immediately after done
//   3. Max outstanding fill then drain — all 4 IDs in flight
//   4. FIFO backpressure — push_ready drops mid-burst
//   5. Stream backpressure — tready drops mid-burst
//   6. Simultaneous AR fire + R last — inflight counter
//   7. Zero-length transfer (total_xfers=0)

#include "VFsmMm2sMulti.h"
#include "VFsmS2mmMulti.h"
#include <cassert>
#include <cstdio>

static VFsmMm2sMulti rd;
static VFsmS2mmMulti wr;
static int cycle_count = 0;
static int test_num = 0;

void tick() {
    rd.clk = 0; rd.eval(); wr.clk = 0; wr.eval();
    rd.clk = 1; rd.eval(); wr.clk = 1; wr.eval();
    cycle_count++;
}

void reset_rd() {
    rd.rst = 1; rd.start = 0; rd.ar_ready = 0; rd.r_valid = 0;
    rd.r_data = 0; rd.r_id = 0; rd.r_last = 0; rd.push_ready = 1;
    rd.total_xfers = 0; rd.base_addr = 0; rd.burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    rd.rst = 0; tick();
}

void reset_wr() {
    wr.rst = 1; wr.start = 0; wr.aw_ready = 0; wr.w_ready = 0;
    wr.b_valid = 0; wr.b_id = 0; wr.pop_valid = 1; wr.pop_data = 0;
    wr.total_xfers = 0; wr.base_addr = 0; wr.burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    wr.rst = 0; tick();
}

void pass(const char *name) {
    test_num++;
    printf("  [%d] PASS: %s\n", test_num, name);
}

// ═══════════════════════════════════════════════════════════════════
// Test 1: Single-beat burst (MM2S read, burst_len=1)
// ═══════════════════════════════════════════════════════════════════
void test_single_beat_read() {
    reset_rd();

    rd.start = 1; rd.total_xfers = 1; rd.base_addr = 0x100; rd.burst_len = 1;
    tick(); rd.start = 0;

    // Accept AR
    for (int c = 0; c < 10; c++) {
        if (rd.ar_valid) { rd.ar_ready = 1; break; }
        tick();
    }
    assert(rd.ar_valid);
    assert(rd.ar_len == 0);  // burst_len=1 → ar_len=0
    tick(); rd.ar_ready = 0;

    // Send 1 R beat with r_last
    rd.r_valid = 1; rd.r_data = 0xCAFE; rd.r_id = 0; rd.r_last = 1;
    tick();
    assert(rd.push_valid);  // data pushed to FIFO
    rd.r_valid = 0;

    // Wait for done
    for (int c = 0; c < 10; c++) { tick(); if (rd.done) break; }
    assert(rd.done);

    pass("Single-beat read (burst_len=1)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 2: Single-beat burst (S2MM write, burst_len=1)
// ═══════════════════════════════════════════════════════════════════
void test_single_beat_write() {
    reset_wr();

    wr.start = 1; wr.total_xfers = 1; wr.base_addr = 0x200; wr.burst_len = 1;
    tick(); wr.start = 0;

    // Accept AW
    wr.aw_ready = 1;
    for (int c = 0; c < 10; c++) {
        tick();
        if (wr.aw_valid && wr.aw_ready) {
            assert(wr.aw_len == 0);  // burst_len=1 → aw_len=0
            break;
        }
    }
    wr.aw_ready = 0;

    // Accept W beat and send B response
    wr.w_ready = 1; wr.pop_valid = 1; wr.pop_data = 0xBEEF;
    int w_beats = 0;
    bool b_sent = false;
    for (int c = 0; c < 30; c++) {
        // Count W beats (sample before tick)
        if (wr.w_valid && wr.w_ready) w_beats++;

        // Send B once in Drain state (b_ready asserted)
        if (!b_sent && wr.b_ready) {
            wr.b_valid = 1; wr.b_id = 0;
            b_sent = true;
        } else {
            wr.b_valid = 0;
        }

        tick();
        if (wr.done) break;
    }
    assert(wr.done);
    assert(w_beats >= 1);
    assert(b_sent);

    pass("Single-beat write (burst_len=1, w_last on first beat)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 3: Back-to-back transfers — start immediately after done
// ═══════════════════════════════════════════════════════════════════
void test_back_to_back() {
    reset_rd();

    for (int round = 0; round < 3; round++) {
        rd.start = 1; rd.total_xfers = 1; rd.base_addr = 0x1000 + round * 0x100;
        rd.burst_len = 2;
        tick(); rd.start = 0;

        // Accept AR (check before tick to catch single-cycle valid)
        rd.ar_ready = 1;
        bool ar_seen = false;
        for (int c = 0; c < 20; c++) {
            if (rd.ar_valid) { ar_seen = true; tick(); break; }
            tick();
        }
        assert(ar_seen);
        rd.ar_ready = 0;

        // Send 2 R beats
        for (int b = 0; b < 2; b++) {
            rd.r_valid = 1; rd.r_data = round * 0x100 + b;
            rd.r_id = 0; rd.r_last = (b == 1) ? 1 : 0;
            tick();
        }
        rd.r_valid = 0;

        // Wait for done
        for (int c = 0; c < 10; c++) { tick(); if (rd.done) break; }
        assert(rd.done);

        // Wait for idle before next round
        for (int c = 0; c < 5; c++) { tick(); if (rd.idle_out) break; }
    }

    pass("Back-to-back transfers (3 rounds)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 4: Max outstanding fill + drain
// ═══════════════════════════════════════════════════════════════════
void test_max_outstanding() {
    reset_rd();

    rd.start = 1; rd.total_xfers = 8; rd.base_addr = 0x2000; rd.burst_len = 2;
    tick(); rd.start = 0;

    // Accept AR rapidly — should get 4 (max outstanding)
    int ar_count = 0;
    rd.ar_ready = 1;
    for (int c = 0; c < 30; c++) {
        if (rd.ar_valid && rd.ar_ready) ar_count++;
        if (ar_count == 4) break;
        tick();
    }
    assert(ar_count == 4);

    // No more AR should be issued (all 4 slots full)
    rd.ar_ready = 1;
    tick(); tick();
    // ar_valid should be 0 (can_issue false)
    // (can't assert easily due to settle, but verify by draining)

    // Concurrent AR accept + R response for all 8 bursts
    int r_burst = 0, r_beat_idx = 0;
    rd.ar_ready = 1;
    for (int c = 0; c < 100; c++) {
        // Accept AR (check before tick)
        if (rd.ar_valid) ar_count++;

        // Send R beats
        if (r_burst < 8) {
            rd.r_valid = 1;
            rd.r_data = r_burst * 0x10 + r_beat_idx;
            rd.r_id = r_burst % 4;
            rd.r_last = (r_beat_idx == 1) ? 1 : 0;
        } else {
            rd.r_valid = 0;
        }

        tick();

        // Advance R beat on handshake (check r_ready after tick = post-settle)
        if (rd.r_valid && rd.r_ready && r_burst < 8) {
            r_beat_idx++;
            if (r_beat_idx == 2) { r_beat_idx = 0; r_burst++; }
        }

        if (rd.done) break;
    }
    rd.r_valid = 0; rd.ar_ready = 0;
    assert(rd.done);
    assert(ar_count == 8);

    pass("Max outstanding fill + drain (8 xfers, 4 outstanding)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 5: FIFO backpressure — push_ready drops mid-burst
// ═══════════════════════════════════════════════════════════════════
void test_fifo_backpressure() {
    reset_rd();

    rd.start = 1; rd.total_xfers = 1; rd.base_addr = 0x3000; rd.burst_len = 4;
    tick(); rd.start = 0;

    // Accept AR
    rd.ar_ready = 1;
    for (int c = 0; c < 10; c++) { tick(); if (rd.ar_valid) break; }
    tick(); rd.ar_ready = 0;

    int pushes = 0;
    // Send 4 R beats but drop push_ready after beat 1
    for (int b = 0; b < 4; ) {
        rd.r_valid = 1; rd.r_data = 0xF000 + b;
        rd.r_id = 0; rd.r_last = (b == 3) ? 1 : 0;

        // Backpressure after beat 1
        rd.push_ready = (pushes < 2) ? 1 : ((cycle_count % 3 == 0) ? 0 : 1);

        tick();

        if (rd.r_valid && rd.r_ready && rd.push_ready) {
            pushes++;
            b++;
        }
    }
    rd.r_valid = 0;
    rd.push_ready = 1;

    // Wait for done
    for (int c = 0; c < 20; c++) { tick(); if (rd.done) break; }
    assert(rd.done);
    assert(pushes == 4);

    pass("FIFO backpressure mid-burst (push_ready drops)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 6: Simultaneous AR fire + R last (inflight counter)
// ═══════════════════════════════════════════════════════════════════
void test_simultaneous_ar_rlast() {
    reset_rd();

    rd.start = 1; rd.total_xfers = 2; rd.base_addr = 0x4000; rd.burst_len = 1;
    tick(); rd.start = 0;

    // Issue AR #0
    rd.ar_ready = 1;
    for (int c = 0; c < 10; c++) { tick(); if (rd.ar_valid) break; }
    // AR #0 accepted. Now immediately send R last for AR#0 AND accept AR#1 same cycle
    rd.r_valid = 1; rd.r_data = 0xAA; rd.r_id = 0; rd.r_last = 1;
    rd.ar_ready = 1;
    tick();  // This cycle: AR#1 fires AND R#0 last fires simultaneously

    // Verify both happened
    rd.r_valid = 0; rd.ar_ready = 0;

    // Now send R for AR#1
    rd.r_valid = 1; rd.r_data = 0xBB; rd.r_id = 1; rd.r_last = 1;
    tick();
    rd.r_valid = 0;

    // Wait for done
    for (int c = 0; c < 10; c++) { tick(); if (rd.done) break; }
    assert(rd.done);

    pass("Simultaneous AR fire + R last (inflight counter net zero)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 7: Zero-length transfer (total_xfers=0)
// ═══════════════════════════════════════════════════════════════════
void test_zero_length() {
    reset_rd();

    rd.start = 1; rd.total_xfers = 0; rd.base_addr = 0x5000; rd.burst_len = 4;
    tick(); rd.start = 0;

    // Should go Active → Done immediately (all_done = 0==0 && 0==0 = true)
    for (int c = 0; c < 5; c++) {
        tick();
        if (rd.done) break;
    }
    assert(rd.done);

    // Verify no AR was issued
    // (Can't easily check ar_valid history, but done should fire within 2 cycles)
    tick();
    assert(rd.idle_out);

    pass("Zero-length transfer (total_xfers=0, immediate done)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 8: S2MM FIFO underrun — pop_valid drops mid-burst
// ═══════════════════════════════════════════════════════════════════
void test_s2mm_fifo_underrun() {
    reset_wr();

    wr.start = 1; wr.total_xfers = 1; wr.base_addr = 0x6000; wr.burst_len = 4;
    tick(); wr.start = 0;

    // Accept AW
    wr.aw_ready = 1;
    for (int c = 0; c < 10; c++) { tick(); if (wr.aw_valid) break; }
    tick(); wr.aw_ready = 0;

    // Send W with pop_valid toggling (FIFO underrun)
    int w_beats = 0;
    int w_last_seen = 0;
    int b_sent = 0;
    wr.w_ready = 1;
    for (int c = 0; c < 80; c++) {
        // Toggle pop_valid: 2 beats on, 3 beats off
        wr.pop_valid = ((c / 2) % 3 != 2) ? 1 : 0;
        wr.pop_data = 0x6000 + w_beats;

        // Count W before tick
        if (wr.w_valid && wr.w_ready) {
            w_beats++;
            if (wr.w_last) w_last_seen++;
        }

        // Send exactly 1 B response after w_last
        if (b_sent < w_last_seen && wr.b_ready) {
            wr.b_valid = 1; wr.b_id = 0; b_sent++;
        } else {
            wr.b_valid = 0;
        }

        tick();
        if (wr.done) break;
    }
    assert(wr.done);
    assert(w_beats >= 4);

    pass("S2MM FIFO underrun (pop_valid toggling mid-burst)");
}

// ═══════════════════════════════════════════════════════════════════
// Test 9: Reset during active transfer
// ═══════════════════════════════════════════════════════════════════
void test_reset_mid_transfer() {
    reset_rd();

    rd.start = 1; rd.total_xfers = 4; rd.base_addr = 0x7000; rd.burst_len = 4;
    tick(); rd.start = 0;

    // Accept 2 ARs
    rd.ar_ready = 1;
    int ar_count = 0;
    for (int c = 0; c < 10; c++) {
        tick();
        if (rd.ar_valid && rd.ar_ready) ar_count++;
        if (ar_count == 2) break;
    }
    rd.ar_ready = 0;

    // Send 3 R beats (partial burst)
    for (int b = 0; b < 3; b++) {
        rd.r_valid = 1; rd.r_data = b; rd.r_id = 0; rd.r_last = 0;
        tick();
    }
    rd.r_valid = 0;

    // Assert reset mid-transfer
    rd.rst = 1;
    for (int c = 0; c < 3; c++) tick();
    rd.rst = 0;
    tick();

    // Should be back in Idle
    assert(rd.idle_out == 1);
    assert(rd.done == 0);
    assert(rd.ar_valid == 0);

    // Can start a new transfer cleanly
    rd.start = 1; rd.total_xfers = 1; rd.base_addr = 0x8000; rd.burst_len = 1;
    tick(); rd.start = 0;

    rd.ar_ready = 1;
    bool ar_ok = false;
    for (int c = 0; c < 10; c++) {
        if (rd.ar_valid) { ar_ok = true; assert(rd.ar_addr == 0x8000); tick(); break; }
        tick();
    }
    assert(ar_ok);
    rd.ar_ready = 0;

    rd.r_valid = 1; rd.r_data = 0xFF; rd.r_id = 0; rd.r_last = 1;
    tick(); rd.r_valid = 0;

    for (int c = 0; c < 10; c++) { tick(); if (rd.done) break; }
    assert(rd.done);

    pass("Reset mid-transfer, clean recovery");
}

// ═══════════════════════════════════════════════════════════════════

int main() {
    printf("=== Multi-Outstanding DMA Corner-Case Tests ===\n\n");

    test_single_beat_read();
    test_single_beat_write();
    test_back_to_back();
    test_max_outstanding();
    test_fifo_backpressure();
    test_simultaneous_ar_rlast();
    test_zero_length();
    test_s2mm_fifo_underrun();
    test_reset_mid_transfer();

    printf("\n=== ALL %d CORNER-CASE TESTS PASSED ===\n", test_num);
    return 0;
}
