// ARCH sim testbench for e203_clint_timer — 64-bit CLINT mtime/mtimecmp timer
// with a simple register read/write interface (not APB) and a synchronous
// active-high reset. Tests: reset values, the free-running increment, register
// writes to all four halves with readback, the lo->hi carry at 0xFFFFFFFF,
// tmr_irq threshold crossing at the exact boundary cycle, irq deassertion when
// mtimecmp is raised, and true 64-bit (not 32-bit-halved) comparison.
//
// NOTE: this replaces a stale tb (VClintTimer.h) that targeted the
// pre-2026-04 PascalCase fixture naming, and also folds in the coverage of
// the deleted e203_clint_timer_vltor_tb.cpp Verilator-flavor duplicate (no
// harness path ever ran vltor tbs). The construct is now `e203_clint_timer`,
// so the sim class is Ve203_clint_timer.
//
// Run with:
//   arch sim tests/e203/e203_clint_timer.arch --tb tests/e203/e203_clint_timer_tb.cpp

#include "Ve203_clint_timer.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_clint_timer* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst = 1;               // synchronous, active HIGH
    dut->reg_addr = 0;
    dut->reg_wdata = 0;
    dut->reg_wen = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst = 0;
    dut->eval();
}

// Combinational read of one 32-bit register half.
static uint32_t rd(uint32_t addr) {
    dut->reg_addr = addr;
    dut->eval();
    return dut->reg_rdata;
}

// One-cycle register write (the counter keeps running underneath).
static void wr(uint32_t addr, uint32_t data) {
    dut->reg_addr = addr;
    dut->reg_wdata = data;
    dut->reg_wen = 1;
    tick();
    dut->reg_wen = 0;
    dut->eval();
}

int main() {
    dut = new Ve203_clint_timer;

    // ── Test 1: Reset values ─────────────────────────────────────────
    printf("Test 1: Reset values\n");
    reset();
    uint32_t v = rd(0x0);
    CHECK(v == 0, "mtime_lo should be 0 after reset, got 0x%08x", v);
    v = rd(0x4);
    CHECK(v == 0, "mtime_hi should be 0 after reset, got 0x%08x", v);
    v = rd(0x8);
    CHECK(v == 0xFFFFFFFFu, "mtimecmp_lo should reset to 0xFFFFFFFF, got 0x%08x", v);
    v = rd(0xC);
    CHECK(v == 0xFFFFFFFFu, "mtimecmp_hi should reset to 0xFFFFFFFF, got 0x%08x", v);
    CHECK(dut->tmr_irq == 0, "tmr_irq should be 0 after reset, got %d", dut->tmr_irq);
    v = rd(0x2);
    CHECK(v == 0, "unmapped register address should read 0, got 0x%08x", v);

    // ── Test 2: Free-running increment ───────────────────────────────
    printf("Test 2: Free-running counter\n");
    reset();
    for (int i = 0; i < 5; i++) tick();
    v = rd(0x0);
    CHECK(v == 5, "mtime_lo should be 5 after 5 ticks, got 0x%08x", v);
    v = rd(0x4);
    CHECK(v == 0, "mtime_hi should still be 0, got 0x%08x", v);

    // ── Test 3: Register writes + readback ───────────────────────────
    printf("Test 3: Register writes\n");
    reset();
    wr(0x0, 100);
    v = rd(0x0);
    CHECK(v == 100, "mtime_lo should be 100 right after the write, got 0x%08x", v);
    tick();
    v = rd(0x0);
    CHECK(v == 101, "mtime_lo should keep counting from the written value, got 0x%08x", v);
    wr(0x8, 0x00001000u);
    v = rd(0x8);
    CHECK(v == 0x00001000u, "mtimecmp_lo readback should be 0x1000, got 0x%08x", v);
    wr(0xC, 0x00000002u);
    v = rd(0xC);
    CHECK(v == 0x00000002u, "mtimecmp_hi readback should be 2, got 0x%08x", v);
    // mtimecmp does not tick: it must hold its value.
    tick(); tick();
    v = rd(0x8);
    CHECK(v == 0x00001000u, "mtimecmp_lo must not change on its own, got 0x%08x", v);

    // ── Test 4: lo -> hi carry ───────────────────────────────────────
    printf("Test 4: 32-bit carry\n");
    reset();
    wr(0x0, 0xFFFFFFFFu);           // mtime = 0x0_FFFFFFFF (hi still 0)
    v = rd(0x0);
    CHECK(v == 0xFFFFFFFFu, "mtime_lo should be 0xFFFFFFFF, got 0x%08x", v);
    tick();                          // increment carries into hi
    v = rd(0x0);
    CHECK(v == 0, "mtime_lo should wrap to 0, got 0x%08x", v);
    v = rd(0x4);
    CHECK(v == 1, "mtime_hi should carry to 1, got 0x%08x", v);

    // ── Test 5: irq threshold boundary ───────────────────────────────
    printf("Test 5: irq threshold\n");
    reset();
    wr(0xC, 0);                      // mtimecmp = 105 (hi first: reset hi is all-1s)
    wr(0x8, 105);
    wr(0x0, 100);                    // mtime = 100
    // Ticks: 101, 102, 103, 104 -> still below the compare value.
    for (int i = 0; i < 4; i++) tick();
    v = rd(0x0);
    CHECK(v == 104, "mtime_lo should be 104, got 0x%08x", v);
    CHECK(dut->tmr_irq == 0, "tmr_irq must stay 0 while mtime < mtimecmp, got %d", dut->tmr_irq);
    tick();                          // mtime = 105 == mtimecmp
    dut->eval();
    CHECK(dut->tmr_irq == 1, "tmr_irq should assert when mtime == mtimecmp, got %d", dut->tmr_irq);
    tick();                          // mtime = 106 > mtimecmp
    dut->eval();
    CHECK(dut->tmr_irq == 1, "tmr_irq should stay asserted while mtime > mtimecmp, got %d",
          dut->tmr_irq);
    // Raising mtimecmp deasserts the (level) interrupt.
    wr(0x8, 0xFFFFFFFFu);
    wr(0xC, 0xFFFFFFFFu);
    dut->eval();
    CHECK(dut->tmr_irq == 0, "tmr_irq should drop once mtimecmp is raised, got %d", dut->tmr_irq);

    // ── Test 6: Comparison is 64-bit, not per-half ───────────────────
    printf("Test 6: 64-bit compare\n");
    reset();
    // mtimecmp = 0x00000001_00000000, mtime = 0x00000000_FFFF0000:
    // lo(mtime) > lo(cmp) but the full 64-bit value is smaller -> no irq.
    wr(0xC, 0x00000001u);
    wr(0x8, 0x00000000u);
    wr(0x0, 0xFFFF0000u);
    dut->eval();
    CHECK(dut->tmr_irq == 0, "irq must use the full 64-bit compare (mtime < cmp), got %d",
          dut->tmr_irq);
    // Now push mtime_hi to 1: mtime = 0x1_xxxxxxx >= 0x1_00000000 -> irq.
    wr(0x4, 0x00000001u);
    dut->eval();
    CHECK(dut->tmr_irq == 1, "irq should assert once mtime_hi reaches cmp_hi, got %d",
          dut->tmr_irq);

    printf("\n=== e203_clint_timer: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
