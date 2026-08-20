// ARCH sim testbench for e203_spi — APB SPI master with a single-byte shift
// register. Tests: reset state, ctrl/div register write + readback, enable
// gating of transfers, a full 8-bit mode-0 transfer (MSB-first MOSI checked
// per bit, MISO looped in per bit), chip-select framing, done/irq assertion,
// rxdata readout, status read-to-clear, busy status mid-transfer, and CPOL=1
// clock idling.
//
// NOTE: this replaces a stale tb (VSpi.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_spi`, so the sim
// class is Ve203_spi.
//
// Run with:
//   arch sim tests/e203/e203_spi.arch --tb tests/e203/e203_spi_tb.cpp

#include "Ve203_spi.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_spi* dut;

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
    dut->spi_miso = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

static void apb_write(uint32_t addr, uint32_t data) {
    dut->psel = 1; dut->penable = 0;
    dut->paddr = addr; dut->pwdata = data; dut->pwrite = 1;
    tick();
    dut->penable = 1;
    tick();
    dut->psel = 0; dut->penable = 0; dut->pwrite = 0;
    dut->eval();
}

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
    dut = new Ve203_spi;

    // ── Test 1: Reset state ──────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->spi_cs_n == 1, "cs_n should idle high after reset, got %d", dut->spi_cs_n);
    CHECK(dut->spi_sclk == 0, "sclk should idle low after reset (CPOL=0), got %d", dut->spi_sclk);
    CHECK(dut->spi_irq == 0, "spi_irq should be 0 after reset, got %d", dut->spi_irq);
    CHECK(dut->pready == 1, "pready is constant 1 in this design, got %d", dut->pready);
    uint32_t v = apb_read(0x0C);
    CHECK(v == 4, "div resets to 4, got 0x%08x", v);
    v = apb_read(0x08);
    CHECK(v == 0, "ctrl resets to 0, got 0x%08x", v);
    v = apb_read(0x10);
    CHECK(v == 0, "status should be 0 at idle, got 0x%08x", v);

    // ── Test 2: ctrl/div write + readback ────────────────────────────
    printf("Test 2: Register write/readback\n");
    apb_write(0x08, 0x7);           // en + cpol + cpha
    v = apb_read(0x08);
    CHECK(v == 0x7, "ctrl readback should be 0x7 (en|cpol|cpha), got 0x%08x", v);
    apb_write(0x0C, 0x20);
    v = apb_read(0x0C);
    CHECK(v == 0x20, "div readback should be 0x20, got 0x%08x", v);
    v = apb_read(0x30);
    CHECK(v == 0, "unmapped offset 0x30 should read 0, got 0x%08x", v);

    // ── Test 3: txdata write ignored while disabled ──────────────────
    printf("Test 3: Enable gating\n");
    reset();
    apb_write(0x00, 0xAB);          // ctrl_en=0: must not start
    v = apb_read(0x00);
    CHECK((v >> 31) == 0, "busy must stay 0 with enable off, got 0x%08x", v);
    CHECK(dut->spi_cs_n == 1, "cs_n must stay high with enable off, got %d", dut->spi_cs_n);

    // ── Test 4: Full mode-0 transfer ─────────────────────────────────
    // div=0 -> div_tick every cycle: one SPI half-period per clock,
    // so each bit is one setup tick + one sample tick.
    printf("Test 4: Mode-0 transfer\n");
    reset();
    apb_write(0x08, 0x1);           // enable, CPOL=0, CPHA=0
    apb_write(0x0C, 0x0);           // div = 0
    const uint8_t mosi_byte = 0xC5;
    const uint8_t miso_byte = 0x3A;
    apb_write(0x00, mosi_byte);     // start transfer
    CHECK(dut->spi_cs_n == 0, "cs_n should drop when the transfer starts, got %d", dut->spi_cs_n);
    CHECK(dut->spi_sclk == 0, "sclk should start at CPOL=0, got %d", dut->spi_sclk);
    for (int i = 7; i >= 0; i--) {
        dut->eval();
        int expect = (mosi_byte >> i) & 1;
        CHECK(dut->spi_mosi == expect, "MOSI bit %d should be %d (MSB first), got %d",
              i, expect, dut->spi_mosi);
        dut->spi_miso = (miso_byte >> i) & 1;
        tick();                     // setup half: sclk toggles to active
        CHECK(dut->spi_sclk == 1, "sclk should be high in the sample half of bit %d, got %d",
              i, dut->spi_sclk);
        tick();                     // sample half: MISO latched, shift
    }
    dut->eval();
    CHECK(dut->spi_cs_n == 1, "cs_n should rise when the transfer completes, got %d", dut->spi_cs_n);
    CHECK(dut->spi_sclk == 0, "sclk should return to CPOL=0 after the transfer, got %d", dut->spi_sclk);
    CHECK(dut->spi_irq == 1, "spi_irq should assert on done, got %d", dut->spi_irq);
    v = apb_read(0x04);
    CHECK(v == miso_byte, "rxdata should be 0x%02x, got 0x%08x", miso_byte, v);
    v = apb_read(0x10);
    CHECK((v & 0x2) != 0, "status done (bit 1) should be set, got 0x%08x", v);
    CHECK((v & 0x1) == 0, "status busy (bit 0) should be clear, got 0x%08x", v);
    // The status read clears done.
    dut->eval();
    CHECK(dut->spi_irq == 0, "spi_irq should clear after status read, got %d", dut->spi_irq);
    v = apb_read(0x10);
    CHECK((v & 0x2) == 0, "status done should be cleared by the previous read, got 0x%08x", v);

    // ── Test 5: busy status mid-transfer ─────────────────────────────
    printf("Test 5: Busy mid-transfer\n");
    reset();
    apb_write(0x08, 0x1);
    apb_write(0x0C, 0x80);          // slow: transfer stalls across APB reads
    apb_write(0x00, 0xFF);
    v = apb_read(0x00);
    CHECK((v >> 31) == 1, "txdata read bit 31 should show busy, got 0x%08x", v);
    v = apb_read(0x10);
    CHECK((v & 0x1) != 0, "status busy (bit 0) should be set mid-transfer, got 0x%08x", v);
    CHECK(dut->spi_cs_n == 0, "cs_n should stay low mid-transfer, got %d", dut->spi_cs_n);
    // A second txdata write while busy must be ignored (MOSI still MSB of 0xFF).
    apb_write(0x00, 0x00);
    CHECK(dut->spi_mosi == 1, "write-while-busy must not disturb the shift register, got mosi=%d",
          dut->spi_mosi);

    // ── Test 6: CPOL=1 clock idling ──────────────────────────────────
    printf("Test 6: CPOL=1\n");
    reset();
    apb_write(0x08, 0x3);           // enable + CPOL=1
    apb_write(0x0C, 0x0);
    apb_write(0x00, 0x00);
    CHECK(dut->spi_sclk == 1, "sclk should start at CPOL=1, got %d", dut->spi_sclk);
    tick(); dut->eval();
    CHECK(dut->spi_sclk == 0, "sclk should toggle low in the sample half (CPOL=1), got %d",
          dut->spi_sclk);
    for (int i = 0; i < 15; i++) tick();   // finish the remaining halves
    dut->eval();
    CHECK(dut->spi_sclk == 1, "sclk should return to CPOL=1 after the transfer, got %d",
          dut->spi_sclk);
    CHECK(dut->spi_cs_n == 1, "cs_n should be high after the CPOL=1 transfer, got %d",
          dut->spi_cs_n);

    printf("\n=== e203_spi: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
