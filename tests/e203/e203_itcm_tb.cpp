// ARCH sim testbench for e203_itcm — 64KB instruction TCM: a thin wrapper
// over a simple_dual latency-1 SRAM (independent read and write ports).
// Tests: write-then-read across several addresses including both ends of the
// 16384-word array, the 1-cycle registered read latency, output hold when
// rd_en is low, wr_en gating, and same-cycle read+write to different
// addresses on the two ports.
//
// NOTE: this replaces a stale Verilator-flavor tb (VE203Itcm.h + verilated.h)
// that targeted the pre-2026-04 PascalCase fixture naming. The construct is
// now `e203_itcm`, so the arch-sim class is Ve203_itcm (no verilated.h in the
// arch sim flow).
//
// Run with:
//   arch sim tests/e203/e203_itcm.arch --tb tests/e203/e203_itcm_tb.cpp

#include "Ve203_itcm.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_itcm* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->rd_en = 0;
    dut->rd_addr = 0;
    dut->wr_en = 0;
    dut->wr_addr = 0;
    dut->wr_data = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

static void write_word(uint32_t addr, uint32_t data) {
    dut->wr_en = 1;
    dut->wr_addr = addr;
    dut->wr_data = data;
    tick();
    dut->wr_en = 0;
    dut->eval();
}

// Latency-1 read: present the address, clock once, sample rd_data.
static uint32_t read_word(uint32_t addr) {
    dut->rd_en = 1;
    dut->rd_addr = addr;
    tick();
    dut->rd_en = 0;
    dut->eval();
    return dut->rd_data;
}

int main() {
    dut = new Ve203_itcm;

    // ── Test 1: Write then read back ─────────────────────────────────
    printf("Test 1: Write/readback\n");
    reset();
    write_word(0x0000, 0x00000013u);      // nop
    write_word(0x0001, 0x002081B3u);      // add x3,x1,x2
    write_word(0x2AAA, 0xDEADBEEFu);
    write_word(0x3FFF, 0xCAFEBABEu);      // last word
    uint32_t v = read_word(0x0000);
    CHECK(v == 0x00000013u, "word 0 should be 0x00000013, got 0x%08x", v);
    v = read_word(0x0001);
    CHECK(v == 0x002081B3u, "word 1 should be 0x002081B3, got 0x%08x", v);
    v = read_word(0x2AAA);
    CHECK(v == 0xDEADBEEFu, "word 0x2AAA should be 0xDEADBEEF, got 0x%08x", v);
    v = read_word(0x3FFF);
    CHECK(v == 0xCAFEBABEu, "last word should be 0xCAFEBABE, got 0x%08x", v);

    // ── Test 2: Overwrite in place ───────────────────────────────────
    printf("Test 2: Overwrite\n");
    write_word(0x0001, 0x11112222u);
    v = read_word(0x0001);
    CHECK(v == 0x11112222u, "word 1 should update to 0x11112222, got 0x%08x", v);
    v = read_word(0x0000);
    CHECK(v == 0x00000013u, "word 0 must be untouched, got 0x%08x", v);

    // ── Test 3: Registered output holds when rd_en=0 ─────────────────
    printf("Test 3: Output hold\n");
    v = read_word(0x2AAA);                // rd_data now 0xDEADBEEF
    dut->rd_en = 0;
    dut->rd_addr = 0x0000;                // address changes, no enable
    tick(); tick();
    dut->eval();
    CHECK(dut->rd_data == 0xDEADBEEFu, "rd_data must hold with rd_en=0, got 0x%08x", dut->rd_data);

    // ── Test 4: wr_en gating ─────────────────────────────────────────
    printf("Test 4: wr_en gating\n");
    dut->wr_en = 0;
    dut->wr_addr = 0x2AAA;
    dut->wr_data = 0x55555555u;
    tick();
    v = read_word(0x2AAA);
    CHECK(v == 0xDEADBEEFu, "write without wr_en must not land, got 0x%08x", v);

    // ── Test 5: Same-cycle read + write on the two ports ─────────────
    printf("Test 5: Dual-port concurrency\n");
    // Read word 0 while writing word 0x100 in the same cycle.
    dut->rd_en = 1;
    dut->rd_addr = 0x0000;
    dut->wr_en = 1;
    dut->wr_addr = 0x0100;
    dut->wr_data = 0xA5A5A5A5u;
    tick();
    dut->rd_en = 0;
    dut->wr_en = 0;
    dut->eval();
    CHECK(dut->rd_data == 0x00000013u, "concurrent read of word 0 should be 0x00000013, got 0x%08x",
          dut->rd_data);
    v = read_word(0x0100);
    CHECK(v == 0xA5A5A5A5u, "concurrent write to 0x100 should land, got 0x%08x", v);

    printf("\n=== e203_itcm: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
