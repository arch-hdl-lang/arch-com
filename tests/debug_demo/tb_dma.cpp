#include "VDmaEngine.h"
#include "verilated.h"
#include <cstdio>

VDmaEngine dut;
int cy = 0;

void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    cy++;
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);

    dut.clk = 0; dut.rst = 1; dut.start = 0; dut.src_data = 0;
    for (int i = 0; i < 3; i++) tick();
    dut.rst = 0; tick();

    printf("=== Starting DMA transfer ===\n");
    dut.start = 1; dut.src_data = 10;
    tick();
    dut.start = 0;

    for (int c = 0; c < 20; c++) {
        dut.src_data = (c + 1) * 5;
        tick();
        if (dut.done) {
            printf("=== DONE at cycle %d, result=%u ===\n", cy, dut.result);
            break;
        }
    }

    if (dut.done && dut.result > 0) printf("PASS\n");
    else printf("FAIL\n");
    return dut.done ? 0 : 1;
}
