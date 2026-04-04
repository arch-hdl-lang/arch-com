#include "VRamDataArray.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

static VRamDataArray* dut;

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
    dut = new VRamDataArray(ctx);

    dut->clk = 0;
    dut->rd_port_en = 0; dut->rd_port_addr = 0;
    dut->wr_port_en = 0; dut->wr_port_addr = 0; dut->wr_port_wdata = 0;
    dut->eval();

    // Test 1: write 64-bit word at addr {set=1, way=2, word=3} = {001, 010, 011} = 0x053
    // addr = set[5:0] << 6 | way[2:0] << 3 | word[2:0]
    uint16_t addr1 = (1 << 6) | (2 << 3) | 3; // = 0x053
    uint64_t data1 = 0xCAFEBABEDEAD1234ULL;

    dut->wr_port_en = 1; dut->wr_port_addr = addr1; dut->wr_port_wdata = data1;
    tick();

    dut->wr_port_en = 0;
    dut->rd_port_en = 1; dut->rd_port_addr = addr1;
    tick(); // latency=1

    dut->rd_port_en = 0;
    if (dut->rd_port_rdata != data1)
        fail("test1: word readback mismatch");

    // Test 2: different set/way/word
    uint16_t addr2 = (63 << 6) | (7 << 3) | 7; // last entry
    uint64_t data2 = 0x0123456789ABCDEFULL;

    dut->wr_port_en = 1; dut->wr_port_addr = addr2; dut->wr_port_wdata = data2;
    tick();

    dut->wr_port_en = 0;
    dut->rd_port_en = 1; dut->rd_port_addr = addr2;
    tick();

    dut->rd_port_en = 0;
    if (dut->rd_port_rdata != data2)
        fail("test2: last entry readback mismatch");

    // Test 3: overwrite addr1 and verify
    uint64_t data3 = 0xFFFFFFFFFFFFFFFFULL;
    dut->wr_port_en = 1; dut->wr_port_addr = addr1; dut->wr_port_wdata = data3;
    tick();

    dut->wr_port_en = 0;
    dut->rd_port_en = 1; dut->rd_port_addr = addr1;
    tick();

    dut->rd_port_en = 0;
    if (dut->rd_port_rdata != data3)
        fail("test3: overwrite readback mismatch");

    // Test 4: addr2 still holds data2
    dut->rd_port_en = 1; dut->rd_port_addr = addr2;
    tick();
    dut->rd_port_en = 0;
    if (dut->rd_port_rdata != data2)
        fail("test4: addr2 corrupted");

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
