// Testbench with VCD waveform dump for ThreadMm2s
#include "VThreadMm2s.h"
#include "verilated.h"
#include "verilated_vcd_c.h"
#include <cstdio>

static VThreadMm2s dut;
static VerilatedVcdC *tfp;
static vluint64_t sim_time = 0;
static int cy = 0;

void tick() {
    dut.clk = 0; dut.eval(); tfp->dump(sim_time++);
    dut.clk = 1; dut.eval(); tfp->dump(sim_time++);
    cy++;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    Verilated::traceEverOn(true);
    tfp = new VerilatedVcdC;
    dut.trace(tfp, 99);
    tfp->open("thread_mm2s.vcd");

    dut.rst = 1; dut.start = 0; dut.ar_ready = 1;
    dut.r_valid = 0; dut.r_data = 0; dut.r_id = 0; dut.r_last = 0;
    dut.push_ready = 1; dut.total_xfers = 0; dut.base_addr = 0; dut.burst_len = 0;
    for (int i = 0; i < 5; i++) tick();
    dut.rst = 0; tick();

    dut.start = 1; dut.total_xfers = 4;
    dut.base_addr = 0x1000; dut.burst_len = 4;
    tick(); dut.start = 0;

    int ar_count = 0, r_sent = 0, push_count = 0;

    for (int c = 0; c < 300; c++) {
        if (r_sent < ar_count * 4) {
            dut.r_valid = 1;
            dut.r_data = 0xDA7A0000 + r_sent;
            dut.r_id = (r_sent / 4) % 4;
            dut.r_last = ((r_sent % 4) == 3) ? 1 : 0;
        } else {
            dut.r_valid = 0;
        }

        dut.eval();

        if (dut.ar_valid && dut.ar_ready) {
            printf("[%3d] AR: id=%d addr=0x%x\n", cy, dut.ar_id, dut.ar_addr);
            ar_count++;
        }
        if (dut.r_valid && dut.r_ready) r_sent++;
        if (dut.push_valid && dut.push_ready) push_count++;

        tick();

        if (dut.done) {
            printf("[%3d] DONE! AR=%d R=%d push=%d\n", cy, ar_count, r_sent, push_count);
            break;
        }
    }

    printf("Results: AR=%d R=%d push=%d done=%d\n", ar_count, r_sent, push_count, dut.done);
    tfp->close();
    delete tfp;
    return dut.done ? 0 : 1;
}
