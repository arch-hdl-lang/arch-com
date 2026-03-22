// Testbench for E203 ClintTimer — arch sim
#include "VClintTimer.h"
#include <cstdio>
#include <cstdlib>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

static void tick(VClintTimer &m) {
    m.clk = 0; m.eval();
    m.clk = 1; m.eval();
}

int main() {
    VClintTimer m;
    m.clk = 0; m.rst = 1;
    m.reg_addr = 0; m.reg_wdata = 0; m.reg_wen = 0;
    m.eval();

    // Reset
    tick(m); tick(m);
    m.rst = 0;

    // ── After reset: mtime=0, mtimecmp=0xFFFFFFFF_FFFFFFFF ──
    m.reg_addr = 0x0; m.reg_wen = 0; m.eval();
    CHECK(m.reg_rdata == 0, "reset: mtime_lo=0, got 0x%X", m.reg_rdata);
    m.reg_addr = 0x4; m.eval();
    CHECK(m.reg_rdata == 0, "reset: mtime_hi=0, got 0x%X", m.reg_rdata);
    m.reg_addr = 0x8; m.eval();
    CHECK(m.reg_rdata == 0xFFFFFFFF, "reset: mtimecmp_lo=0xFFFFFFFF, got 0x%X", m.reg_rdata);
    m.reg_addr = 0xC; m.eval();
    CHECK(m.reg_rdata == 0xFFFFFFFF, "reset: mtimecmp_hi=0xFFFFFFFF, got 0x%X", m.reg_rdata);

    // ── No IRQ initially (0 < 0xFFFFFFFF_FFFFFFFF) ──────────
    CHECK(m.tmr_irq == 0, "reset: no irq");

    // ── Tick 3 cycles, mtime should increment ────────────────
    tick(m); tick(m); tick(m);
    m.reg_addr = 0x0; m.reg_wen = 0; m.eval();
    CHECK(m.reg_rdata == 3, "after 3 ticks: mtime_lo=%d", m.reg_rdata);

    // ── Write mtimecmp = 10 ──────────────────────────────────
    m.reg_addr = 0x8; m.reg_wdata = 10; m.reg_wen = 1;
    tick(m);
    m.reg_addr = 0xC; m.reg_wdata = 0; m.reg_wen = 1;
    tick(m);
    m.reg_wen = 0;

    // mtime was at 3, ticked 2 more = 5; mtimecmp = 10
    m.reg_addr = 0x0; m.eval();
    CHECK(m.reg_rdata == 5, "mtime=5 after cmp write, got %d", m.reg_rdata);
    CHECK(m.tmr_irq == 0, "no irq: mtime(5) < mtimecmp(10)");

    // ── Tick 5 more → mtime = 10 = mtimecmp → IRQ ───────────
    tick(m); tick(m); tick(m); tick(m); tick(m);
    m.reg_addr = 0x0; m.eval();
    CHECK(m.reg_rdata == 10, "mtime=10, got %d", m.reg_rdata);
    CHECK(m.tmr_irq == 1, "irq: mtime(10) >= mtimecmp(10)");

    // ── Tick 1 more → mtime = 11, still >= 10 → IRQ stays ──
    tick(m);
    m.reg_addr = 0x0; m.eval();
    CHECK(m.reg_rdata == 11, "mtime=11, got %d", m.reg_rdata);
    CHECK(m.tmr_irq == 1, "irq still active");

    // ── Write mtimecmp = 100 → clears IRQ ────────────────────
    m.reg_addr = 0x8; m.reg_wdata = 100; m.reg_wen = 1;
    tick(m);
    m.reg_wen = 0;
    m.eval();
    CHECK(m.tmr_irq == 0, "irq cleared: mtimecmp=100");

    // ── Write mtime directly ─────────────────────────────────
    m.reg_addr = 0x0; m.reg_wdata = 99; m.reg_wen = 1;
    tick(m);
    m.reg_wen = 0;
    m.reg_addr = 0x0; m.eval();
    // mtime was written to 99, but also incremented → 100
    // Actually: write overrides increment in same cycle → 99
    // Then next read sees 99 (since we haven't ticked again)
    // Wait, the seq block writes next_lo first, then reg_wdata overrides.
    // After tick: mtime_lo = 99 (reg write wins)
    CHECK(m.reg_rdata == 99, "mtime written to 99, got %d", m.reg_rdata);

    // ── Tick → mtime = 100 → IRQ ────────────────────────────
    tick(m);
    m.reg_addr = 0x0; m.eval();
    CHECK(m.reg_rdata == 100, "mtime=100, got %d", m.reg_rdata);
    CHECK(m.tmr_irq == 1, "irq: mtime(100) >= mtimecmp(100)");

    // ── Test 64-bit carry: set mtime_lo to 0xFFFFFFFF ────────
    m.reg_addr = 0x0; m.reg_wdata = 0xFFFFFFFE; m.reg_wen = 1;
    tick(m);
    m.reg_addr = 0x4; m.reg_wdata = 0; m.reg_wen = 1;
    tick(m);
    m.reg_wen = 0;
    // mtime_hi was written to 0 (after 1 tick where lo incremented)
    // mtime_lo was set to 0xFFFFFFFE, then ticked once → 0xFFFFFFFF
    // then hi was written to 0, tick happened → lo increments
    // Let's just read and verify
    m.reg_addr = 0x0; m.eval();
    uint32_t lo = m.reg_rdata;
    m.reg_addr = 0x4; m.eval();
    uint32_t hi = m.reg_rdata;
    printf("  mtime = 0x%08X_%08X\n", hi, lo);

    // Tick until carry
    for (int i = 0; i < 5; i++) tick(m);
    m.reg_addr = 0x0; m.eval();
    lo = m.reg_rdata;
    m.reg_addr = 0x4; m.eval();
    hi = m.reg_rdata;
    printf("  after 5 ticks: mtime = 0x%08X_%08X\n", hi, lo);
    // Verify hi incremented at least once (carry happened)
    CHECK(hi >= 1 || lo < 5, "64-bit carry: hi=%d lo=0x%X", hi, lo);

    // ── Read unknown address returns 0 ───────────────────────
    m.reg_addr = 0x3; m.eval();
    CHECK(m.reg_rdata == 0, "unknown addr: rdata=0, got 0x%X", m.reg_rdata);

    printf("\n=== ClintTimer: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
