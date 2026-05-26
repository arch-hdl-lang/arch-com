// Drive the S4LockupRepro: pulse go, feed the FIFO with 8 beats, ack AW,
// W, and B handshakes as a generic AXI slave would. Expected: 2 chunks
// process, aw_count=2, b_count=2, chunk_count=2.
// Actual (the bug): aw_count=1, b_count=0 (B handshake fires but state
// machine doesn't advance), chunk_count=0. Iteration 1 of the outer for
// never starts.

#include "VS4LockupRepro.h"
#include <cstdio>
#include <cstdint>

static VS4LockupRepro dut;
static int cycle = 0;

static void tick() { dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval(); cycle++; }

int main() {
    dut.rst = 0;
    dut.go = 0;
    dut.aw_ready = 0; dut.w_ready = 0; dut.b_valid = 0;
    dut.push_valid = 0; dut.push_data = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 3; ++i) tick();

    // Pulse go for one cycle.
    dut.go = 1;
    tick();
    dut.go = 0;

    // Ack handshakes and feed FIFO. b_valid: pulse for each AW.
    dut.aw_ready = 1; dut.w_ready = 1;
    int pending_bs = 0;   // # AWs accepted but B not yet sent
    uint32_t data[8];
    for (int i = 0; i < 8; ++i) data[i] = 0xA0000000u | i;
    int push_idx = 0;

    for (int i = 0; i < 80; ++i) {
        if (push_idx < 8 && dut.push_ready) {
            dut.push_valid = 1; dut.push_data = data[push_idx]; push_idx++;
        } else {
            dut.push_valid = 0;
        }
        // Count this cycle's AW handshake (pre-edge sample).
        if (dut.aw_valid && dut.aw_ready) pending_bs++;
        // Hold b_valid high whenever there's a pending B.
        dut.b_valid = (pending_bs > 0) ? 1 : 0;

        std::printf("cyc=%d  aw_cnt=%d  b_cnt=%d  chunk_cnt=%d  awv=%d awr=%d  wv=%d wr=%d  bv=%d br=%d  pop_v=%d pop_r=%d\n",
                    cycle, dut.aw_count_out, dut.b_count_out, dut.chunk_count_out,
                    dut.aw_valid, dut.aw_ready, dut.w_valid, dut.w_ready,
                    dut.b_valid, dut.b_ready, 0, 0);
        // Count this cycle's B handshake BEFORE ticking.
        if (dut.b_valid && dut.b_ready && pending_bs > 0) pending_bs--;
        tick();

        if (dut.chunk_count_out == 2) {
            std::printf("DONE: both chunks completed at cyc=%d\n", cycle);
            return 0;
        }
    }
    std::printf("FAIL: only %d/%d chunks completed after 80 cycles\n",
                dut.chunk_count_out, 2);
    return 1;
}
