// Debug testbench for ThreadMm2s — correct AXI handshake timing
// Pattern: set inputs → eval() → check handshake → tick()
#include "VThreadMm2s.h"
#include "VThreadMm2s___024root.h"
#include <cstdio>

static VThreadMm2s dut;
static int cy = 0;

void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cy++;
}

int main() {
    dut.rst = 1; dut.start = 0; dut.ar_ready = 0;
    dut.r_valid = 0; dut.r_data = 0; dut.r_id = 0; dut.r_last = 0;
    dut.push_ready = 1; dut.total_xfers = 0; dut.base_addr = 0; dut.burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    dut.rst = 0; tick();

    dut.start = 1; dut.total_xfers = 4;
    dut.base_addr = 0x1000; dut.burst_len = 4;
    tick(); dut.start = 0;

    int ar_count = 0, r_sent = 0, push_count = 0;
    auto *root = dut.rootp;

    for (int c = 0; c < 200; c++) {
        // Set R channel inputs
        if (r_sent < ar_count * 4) {
            dut.r_valid = 1;
            dut.r_data = 0xDA7A0000 + r_sent;
            dut.r_id = (r_sent / 4) % 4;
            dut.r_last = ((r_sent % 4) == 3) ? 1 : 0;
        } else {
            dut.r_valid = 0;
        }

        // AR always ready
        dut.ar_ready = 1;

        // Eval combinational logic with current inputs
        dut.eval();

        // Check AR handshake
        if (dut.ar_valid && dut.ar_ready) {
            printf("[%3d] AR: id=%d addr=0x%x\n", cy, dut.ar_id, dut.ar_addr);
            ar_count++;
        }

        // Check R handshake (after eval with current inputs)
        if (dut.r_valid && dut.r_ready) {
            r_sent++;
        }

        // Check push handshake
        if (dut.push_valid && dut.push_ready) push_count++;

        if (cy <= 55 || (cy % 10 == 0))
            printf("  [%3d] s0=%d s1=%d s2=%d s3=%d s4=%d | d=%d/%d/%d/%d done=%d rdy=%d rv=%d pv=%d rsnt=%d pc=%d\n",
                   cy,
                   root->ThreadMm2s__DOT___threads__DOT___t0_state,
                   root->ThreadMm2s__DOT___threads__DOT___t1_state,
                   root->ThreadMm2s__DOT___threads__DOT___t2_state,
                   root->ThreadMm2s__DOT___threads__DOT___t3_state,
                   root->ThreadMm2s__DOT___threads__DOT___t4_state,
                   root->ThreadMm2s__DOT___threads__DOT__done_0,
                   root->ThreadMm2s__DOT___threads__DOT__done_1,
                   root->ThreadMm2s__DOT___threads__DOT__done_2,
                   root->ThreadMm2s__DOT___threads__DOT__done_3,
                   dut.done,
                   dut.r_ready,
                   dut.r_valid,
                   dut.push_valid,
                   r_sent, push_count);

        tick();

        if (dut.done) {
            printf("[%3d] DONE! AR=%d R=%d push=%d\n", cy, ar_count, r_sent, push_count);
            break;
        }
    }

    printf("Final: AR=%d R=%d push=%d done=%d\n", ar_count, r_sent, push_count, dut.done);
    if (ar_count == 4 && push_count >= 16 && dut.done) printf("PASS\n");
    else printf("FAIL\n");
    return dut.done ? 0 : 1;
}
