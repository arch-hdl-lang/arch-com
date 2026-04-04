#include "VRamTagArray.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

static VRamTagArray* dut;

static void tick(int n = 1) {
    for (int i = 0; i < n; i++) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    }
}

static void fail(const char* msg) {
    printf("FAIL: %s\n", msg);
    exit(1);
}

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    dut = new VRamTagArray(ctx);

    dut->clk = 0;
    dut->rd_port_en = 0; dut->rd_port_addr = 0;
    dut->wr_port_en = 0; dut->wr_port_addr = 0; dut->wr_port_wdata = 0;
    dut->eval();

    // Test 1: write valid entry {tag=0xDEAD, dirty=0, valid=1} at addr=5
    // Encoding: [53:2]=tag, [1]=dirty, [0]=valid
    uint64_t entry1 = (0xDEADULL << 2) | 0x1; // dirty=0, valid=1
    dut->wr_port_en = 1; dut->wr_port_addr = 5; dut->wr_port_wdata = entry1;
    tick();

    dut->wr_port_en = 0;
    dut->rd_port_en = 1; dut->rd_port_addr = 5;
    tick(); // latency=1: data available after this edge

    dut->rd_port_en = 0;
    if ((uint64_t)dut->rd_port_rdata != entry1)
        fail("test1: entry1 readback mismatch");

    // Test 2: write dirty+valid entry at addr=10
    uint64_t entry2 = (0xBEEFULL << 2) | 0x3; // dirty=1, valid=1
    dut->wr_port_en = 1; dut->wr_port_addr = 10; dut->wr_port_wdata = entry2;
    tick();

    dut->wr_port_en = 0;
    dut->rd_port_en = 1; dut->rd_port_addr = 10;
    tick();

    dut->rd_port_en = 0;
    if ((uint64_t)dut->rd_port_rdata != entry2)
        fail("test2: entry2 readback mismatch");

    // Test 3: write to last valid address (63)
    uint64_t entry3 = (0x123456789AULL << 2) | 0x1;
    // Mask to 54 bits
    entry3 &= ((1ULL << 54) - 1);
    dut->wr_port_en = 1; dut->wr_port_addr = 63; dut->wr_port_wdata = entry3;
    tick();

    dut->wr_port_en = 0;
    dut->rd_port_en = 1; dut->rd_port_addr = 63;
    tick();

    dut->rd_port_en = 0;
    if ((uint64_t)dut->rd_port_rdata != entry3)
        fail("test3: addr=63 readback mismatch");

    // Test 4: addr=5 still holds entry1 (no corruption)
    dut->rd_port_en = 1; dut->rd_port_addr = 5;
    tick();
    dut->rd_port_en = 0;
    if ((uint64_t)dut->rd_port_rdata != entry1)
        fail("test4: addr=5 corrupted after other writes");

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
