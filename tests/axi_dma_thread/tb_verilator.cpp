// Verilator testbench for ThreadMm2s
// Tests 4 burst reads of 4 beats each (total_xfers=4, burst_len=4).
#include "VThreadMm2s.h"
#include "verilated.h"
#include <cstdio>
#include <cassert>

VThreadMm2s *dut;
int cycle = 0;

void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    cycle++;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    dut = new VThreadMm2s;

    // Reset
    dut->rst = 1; dut->start = 0; dut->ar_ready = 1;
    dut->r_valid = 0; dut->r_data = 0; dut->r_id = 0; dut->r_last = 0;
    dut->push_ready = 1;
    dut->total_xfers = 0; dut->base_addr = 0; dut->burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    dut->rst = 0; tick();

    printf("[%d] idle=%d done=%d\n", cycle, dut->idle_out, dut->done);

    // Start: 4 xfers, 4 beats each
    dut->start = 1; dut->total_xfers = 4;
    dut->base_addr = 0x1000; dut->burst_len = 4;
    tick(); dut->start = 0;

    int ar_count = 0, r_sent = 0, push_count = 0;
    dut->ar_ready = 1;

    for (int c = 0; c < 300; c++) {
        // R slave: present beat, check handshake after eval
        if (r_sent < ar_count * 4) {
            dut->r_valid = 1;
            dut->r_data = 0xDA7A0000 + r_sent;
            dut->r_id = (r_sent / 4) % 4;
            dut->r_last = ((r_sent % 4) == 3) ? 1 : 0;
        } else {
            dut->r_valid = 0;
        }

        // Eval combinational logic
        dut->eval();

        // AR handshake
        if (dut->ar_valid && dut->ar_ready) {
            printf("[%d] AR: id=%d addr=0x%x len=%d\n",
                   cycle, dut->ar_id, dut->ar_addr, dut->ar_len + 1);
            ar_count++;
        }

        // R handshake
        if (dut->r_valid && dut->r_ready) {
            r_sent++;
        }

        // Push handshake
        if (dut->push_valid && dut->push_ready) push_count++;

        tick();

        if (dut->done) {
            printf("[%d] DONE! AR=%d R=%d push=%d\n", cycle, ar_count, r_sent, push_count);
            break;
        }
    }

    printf("Results: AR=%d R=%d push=%d done=%d\n",
           ar_count, r_sent, push_count, dut->done);

    if (ar_count >= 1 && push_count >= 4 && dut->done) {
        printf("PASS\n");
    } else {
        printf("FAIL\n");
    }

    delete dut;
    return 0;
}
