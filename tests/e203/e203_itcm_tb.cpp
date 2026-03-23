// Verilator testbench for E203Itcm
#include "VE203Itcm.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;
static VE203Itcm* dut;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { printf("  FAIL: " fmt "\n", ##__VA_ARGS__); fail_count++; } \
} while(0)

static void tick() { dut->clk = 0; dut->eval(); dut->clk = 1; dut->eval(); }

static void reset() {
    dut->rst_n = 0;
    dut->rd_en = 0; dut->rd_addr = 0;
    dut->wr_en = 0; dut->wr_addr = 0; dut->wr_data = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1; tick();
}

static void write_word(uint32_t addr, uint32_t data) {
    dut->wr_en = 1; dut->wr_addr = addr; dut->wr_data = data;
    tick();
    dut->wr_en = 0;
}

static uint32_t read_word(uint32_t addr) {
    dut->rd_en = 1; dut->rd_addr = addr;
    tick();  // initiate read (latency 1)
    dut->rd_en = 0;
    return dut->rd_data;
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    dut = new VE203Itcm();
    reset();

    // ── Test 1: Write then read ──
    printf("Test 1: Write then read\n");
    write_word(0, 0xDEADBEEF);
    uint32_t val = read_word(0);
    CHECK(val == 0xDEADBEEF, "addr 0: got 0x%08X, expected 0xDEADBEEF", val);

    // ── Test 2: Multiple writes to different addresses ──
    printf("Test 2: Multiple writes\n");
    write_word(1, 0x11111111);
    write_word(2, 0x22222222);
    write_word(3, 0x33333333);
    val = read_word(1);
    CHECK(val == 0x11111111, "addr 1: got 0x%08X, expected 0x11111111", val);
    val = read_word(2);
    CHECK(val == 0x22222222, "addr 2: got 0x%08X, expected 0x22222222", val);
    val = read_word(3);
    CHECK(val == 0x33333333, "addr 3: got 0x%08X, expected 0x33333333", val);

    // ── Test 3: Overwrite ──
    printf("Test 3: Overwrite\n");
    write_word(0, 0xCAFEBABE);
    val = read_word(0);
    CHECK(val == 0xCAFEBABE, "addr 0 overwrite: got 0x%08X, expected 0xCAFEBABE", val);

    // ── Test 4: Back-to-back writes then reads ──
    printf("Test 4: Back-to-back access\n");
    for (uint32_t i = 100; i < 110; i++) {
        write_word(i, i * 0x1000);
    }
    for (uint32_t i = 100; i < 110; i++) {
        val = read_word(i);
        CHECK(val == i * 0x1000, "addr %u: got 0x%08X, expected 0x%08X", i, val, i * 0x1000);
    }

    // ── Test 5: High address ──
    printf("Test 5: High address\n");
    write_word(16383, 0xFFFFFFFF);
    val = read_word(16383);
    CHECK(val == 0xFFFFFFFF, "addr 16383: got 0x%08X, expected 0xFFFFFFFF", val);

    printf("\n%s — %d failure(s)\n", fail_count ? "FAIL" : "PASS", fail_count);
    delete dut;
    return fail_count ? 1 : 0;
}
