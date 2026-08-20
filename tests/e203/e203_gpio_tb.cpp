// ARCH sim testbench for e203_gpio — 32-bit GPIO peripheral with APB interface.
// Tests: reset state, output_val/output_en write + readback + pin mirroring,
// combinational input_val read, rising/falling edge interrupt pending capture,
// interrupt-enable gating of gpio_irq, write-1-to-clear of pending bits, and
// unmapped-address reads.
//
// NOTE: this replaces a stale tb (VGpio.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_gpio`, so the sim
// class is Ve203_gpio; the register map is unchanged.
//
// Run with:
//   arch sim tests/e203/e203_gpio.arch --tb tests/e203/e203_gpio_tb.cpp

#include "Ve203_gpio.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_gpio* dut;

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->psel = 0;
    dut->penable = 0;
    dut->paddr = 0;
    dut->pwdata = 0;
    dut->pwrite = 0;
    dut->gpio_in = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// Full APB write: setup phase, then access phase with penable high.
static void apb_write(uint32_t addr, uint32_t data) {
    dut->psel = 1; dut->penable = 0;
    dut->paddr = addr; dut->pwdata = data; dut->pwrite = 1;
    tick();
    dut->penable = 1;
    tick();
    dut->psel = 0; dut->penable = 0; dut->pwrite = 0;
    dut->eval();
}

// Full APB read: returns prdata sampled in the access phase.
static uint32_t apb_read(uint32_t addr) {
    dut->psel = 1; dut->penable = 0;
    dut->paddr = addr; dut->pwrite = 0;
    tick();
    dut->penable = 1;
    dut->eval();
    uint32_t v = dut->prdata;
    tick();
    dut->psel = 0; dut->penable = 0;
    dut->eval();
    return v;
}

int main() {
    dut = new Ve203_gpio;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->gpio_out == 0, "gpio_out should be 0 after reset, got 0x%08x", dut->gpio_out);
    CHECK(dut->gpio_oe == 0, "gpio_oe should be 0 after reset, got 0x%08x", dut->gpio_oe);
    CHECK(dut->gpio_irq == 0, "gpio_irq should be 0 after reset, got %d", dut->gpio_irq);
    CHECK(dut->pready == 1, "pready is constant 1 in this design, got %d", dut->pready);

    // ── Test 2: output_val write mirrors to gpio_out and reads back ──
    printf("Test 2: output_val (0x00)\n");
    apb_write(0x00, 0xFF00FF00u);
    CHECK(dut->gpio_out == 0xFF00FF00u, "gpio_out should be 0xFF00FF00, got 0x%08x", dut->gpio_out);
    uint32_t v = apb_read(0x00);
    CHECK(v == 0xFF00FF00u, "output_val readback should be 0xFF00FF00, got 0x%08x", v);

    // ── Test 3: output_en write mirrors to gpio_oe and reads back ────
    printf("Test 3: output_en (0x04)\n");
    apb_write(0x04, 0x0000FFFFu);
    CHECK(dut->gpio_oe == 0x0000FFFFu, "gpio_oe should be 0x0000FFFF, got 0x%08x", dut->gpio_oe);
    v = apb_read(0x04);
    CHECK(v == 0x0000FFFFu, "output_en readback should be 0x0000FFFF, got 0x%08x", v);

    // ── Test 4: input_val read is combinational from gpio_in ─────────
    printf("Test 4: input_val (0x08)\n");
    dut->gpio_in = 0xDEADBEEFu;
    v = apb_read(0x08);
    CHECK(v == 0xDEADBEEFu, "input_val read should be 0xDEADBEEF, got 0x%08x", v);
    // Return the pins to a stable low state; the ticks inside apb_read already
    // recorded 0xDEADBEEF into gpio_prev_r, so drive low and let the fall
    // pendings latch, then clear them so later irq tests start clean.
    dut->gpio_in = 0;
    tick();
    apb_write(0x18, 0xFFFFFFFFu);   // clear fall_ip from that excursion
    apb_write(0x10, 0xFFFFFFFFu);   // clear rise_ip from that excursion

    // ── Test 5: Rising edge sets rise_ip; irq gated by rise_ie ───────
    printf("Test 5: Rising edge interrupt\n");
    apb_write(0x0C, 0x00000001u);   // rise_ie bit 0
    CHECK(dut->gpio_irq == 0, "gpio_irq should be 0 before any edge, got %d", dut->gpio_irq);
    dut->gpio_in = 1;               // rising edge on bit 0
    tick();
    dut->eval();
    CHECK(dut->gpio_irq == 1, "gpio_irq should assert on enabled rising edge, got %d", dut->gpio_irq);
    v = apb_read(0x10);
    CHECK((v & 1) == 1, "rise_ip bit 0 should be set, got 0x%08x", v);
    // Holding gpio_in high must not clear pending (level is not edge).
    tick(); dut->eval();
    CHECK(dut->gpio_irq == 1, "gpio_irq should stay asserted while pending, got %d", dut->gpio_irq);

    // ── Test 6: Write-1-to-clear rise_ip ─────────────────────────────
    printf("Test 6: rise_ip W1C (0x10)\n");
    // gpio_in is held at 1 (prev==1, no new edge), so the clear must stick.
    apb_write(0x10, 0x00000001u);
    v = apb_read(0x10);
    CHECK((v & 1) == 0, "rise_ip bit 0 should be cleared, got 0x%08x", v);
    CHECK(dut->gpio_irq == 0, "gpio_irq should drop once pending is cleared, got %d", dut->gpio_irq);

    // ── Test 7: Falling edge sets fall_ip; irq gated by fall_ie ──────
    printf("Test 7: Falling edge interrupt\n");
    dut->gpio_in = 0;               // falling edge on bit 0
    tick();
    dut->eval();
    // fall_ie is still 0: pending latches but irq stays masked.
    v = apb_read(0x18);
    CHECK((v & 1) == 1, "fall_ip bit 0 should be set, got 0x%08x", v);
    CHECK(dut->gpio_irq == 0, "gpio_irq should stay 0 with fall_ie=0, got %d", dut->gpio_irq);
    apb_write(0x14, 0x00000001u);   // fall_ie bit 0
    CHECK(dut->gpio_irq == 1, "gpio_irq should assert once fall_ie enables the pending bit, got %d",
          dut->gpio_irq);
    v = apb_read(0x14);
    CHECK(v == 0x00000001u, "fall_ie readback should be 1, got 0x%08x", v);

    // ── Test 8: Write-1-to-clear fall_ip ─────────────────────────────
    printf("Test 8: fall_ip W1C (0x18)\n");
    apb_write(0x18, 0x00000001u);
    v = apb_read(0x18);
    CHECK((v & 1) == 0, "fall_ip bit 0 should be cleared, got 0x%08x", v);
    CHECK(dut->gpio_irq == 0, "gpio_irq should drop once fall pending is cleared, got %d", dut->gpio_irq);

    // ── Test 9: Enabled-but-not-pending does not interrupt ───────────
    printf("Test 9: ie without ip\n");
    // rise_ie bit 0 and fall_ie bit 0 are both enabled here, no pendings.
    CHECK(dut->gpio_irq == 0, "gpio_irq should be 0 with ie set but no pending, got %d", dut->gpio_irq);

    // ── Test 10: Unmapped address reads 0 ────────────────────────────
    printf("Test 10: Unmapped address\n");
    v = apb_read(0x1C);
    CHECK(v == 0, "unmapped offset 0x1C should read 0, got 0x%08x", v);
    v = apb_read(0x40);
    CHECK(v == 0, "unmapped offset 0x40 should read 0, got 0x%08x", v);

    printf("\n=== e203_gpio: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
