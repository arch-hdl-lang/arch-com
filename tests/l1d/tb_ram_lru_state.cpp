#include "VRamLruState.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

static VRamLruState* dut;

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
    dut = new VRamLruState(ctx);

    dut->clk = 0;
    dut->rd_port_en = 0; dut->rd_port_addr = 0;
    dut->wr_port_en = 0; dut->wr_port_addr = 0; dut->wr_port_wdata = 0;
    dut->eval();

    // Test 1: write 7-bit LRU tree at set=0
    uint8_t tree0 = 0x55; // 0b1010101 = 7 bits
    tree0 &= 0x7F;        // mask to 7 bits
    dut->wr_port_en = 1; dut->wr_port_addr = 0; dut->wr_port_wdata = tree0;
    tick();

    dut->wr_port_en = 0;
    dut->rd_port_en = 1; dut->rd_port_addr = 0;
    tick();

    dut->rd_port_en = 0;
    if ((uint8_t)(dut->rd_port_rdata & 0x7F) != tree0)
        fail("test1: set=0 LRU tree mismatch");

    // Test 2: write different trees to all 64 sets, read back
    for (int s = 0; s < 64; s++) {
        uint8_t tree = (uint8_t)(s ^ 0x3F) & 0x7F;
        dut->wr_port_en = 1; dut->wr_port_addr = s; dut->wr_port_wdata = tree;
        tick();
    }

    for (int s = 0; s < 64; s++) {
        uint8_t expected = (uint8_t)(s ^ 0x3F) & 0x7F;
        dut->wr_port_en = 0;
        dut->rd_port_en = 1; dut->rd_port_addr = s;
        tick();
        dut->rd_port_en = 0;
        uint8_t got = dut->rd_port_rdata & 0x7F;
        if (got != expected) {
            printf("FAIL: set=%d expected=0x%02x got=0x%02x\n", s, expected, got);
            exit(1);
        }
    }

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
