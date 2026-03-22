// Verilator testbench for E203 ClintTimer — cross-check
#include "VClintTimer.h"
#include "verilated.h"
#include <cstdio>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

static void tick(VClintTimer* m) {
    m->clk = 0; m->eval();
    m->clk = 1; m->eval();
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto m = new VClintTimer;
    m->clk = 0; m->rst = 1;
    m->reg_addr = 0; m->reg_wdata = 0; m->reg_wen = 0;
    m->eval();
    tick(m); tick(m);
    m->rst = 0;

    // After reset: mtime=0, mtimecmp=0xFFFFFFFF
    m->reg_addr = 0x0; m->eval();
    CHECK(m->reg_rdata == 0, "reset: mtime_lo");
    m->reg_addr = 0x8; m->eval();
    CHECK(m->reg_rdata == 0xFFFFFFFF, "reset: mtimecmp_lo=0x%X", m->reg_rdata);
    CHECK(m->tmr_irq == 0, "no irq at reset");

    // Tick 5 times
    for (int i = 0; i < 5; i++) tick(m);
    m->reg_addr = 0x0; m->eval();
    CHECK(m->reg_rdata == 5, "mtime=5, got %d", m->reg_rdata);

    // Set mtimecmp = 8
    m->reg_addr = 0x8; m->reg_wdata = 8; m->reg_wen = 1; tick(m);
    m->reg_addr = 0xC; m->reg_wdata = 0; m->reg_wen = 1; tick(m);
    m->reg_wen = 0;

    // mtime was 5, ticked 2 more = 7
    m->reg_addr = 0x0; m->eval();
    CHECK(m->reg_rdata == 7, "mtime=7, got %d", m->reg_rdata);
    CHECK(m->tmr_irq == 0, "no irq: 7<8");

    // Tick to 8 → IRQ
    tick(m);
    CHECK(m->tmr_irq == 1, "irq: mtime=8>=mtimecmp=8");

    // Write mtime to 0 → clears
    m->reg_addr = 0x0; m->reg_wdata = 0; m->reg_wen = 1; tick(m);
    m->reg_wen = 0; m->eval();
    CHECK(m->tmr_irq == 0, "irq cleared after mtime reset");

    printf("\n=== ClintTimer Verilator: %d tests, %d errors ===\n", test_num, errors);
    delete m;
    return errors ? 1 : 0;
}
