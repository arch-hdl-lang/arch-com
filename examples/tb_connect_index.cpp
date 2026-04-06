#include "VConnectIndexTest.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

static VConnectIndexTest* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    dut = new VConnectIndexTest(ctx);

    // Reset
    dut->rst = 1; dut->clk = 0; dut->eval();
    tick(); tick();
    dut->rst = 0;

    // Write 0xAA to way=0, addr=3
    dut->wr_en = 1; dut->wr_way = 0;
    dut->wr_addr = 3; dut->wr_data = 0xAA;
    tick();

    // Write 0xBB to way=1, addr=5
    dut->wr_way = 1; dut->wr_addr = 5; dut->wr_data = 0xBB;
    tick();
    dut->wr_en = 0;

    // Read from both ways
    dut->rd_addr = 3; dut->eval();
    if (dut->rd_data0 != 0xAA) {
        printf("FAIL: rd_data0 way0[3]=0x%02x expected 0xAA\n", dut->rd_data0);
        exit(1);
    }
    printf("Test 1 PASS: way0[3] = 0x%02x\n", dut->rd_data0);

    dut->rd_addr = 5; dut->eval();
    if (dut->rd_data1 != 0xBB) {
        printf("FAIL: rd_data1 way1[5]=0x%02x expected 0xBB\n", dut->rd_data1);
        exit(1);
    }
    printf("Test 2 PASS: way1[5] = 0x%02x\n", dut->rd_data1);

    // Verify cross-way isolation: way1[3] should be 0 (not written)
    dut->rd_addr = 3; dut->eval();
    if (dut->rd_data1 != 0x00) {
        printf("FAIL: way1[3]=0x%02x expected 0x00 (isolation)\n", dut->rd_data1);
        exit(1);
    }
    printf("Test 3 PASS: way isolation verified\n");

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
