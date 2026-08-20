// ARCH sim testbench for e203_uart — simplified APB UART with shift-register
// serial TX/RX. Tests: reset/idle state, control-register write + readback,
// a full 10-bit TX frame (start/data LSB-first/stop) sampled on the pin,
// tx_busy status and write-while-busy lockout, a full RX frame driven on the
// pin with irq assertion, rxdata readout, and read-to-clear of rx_valid.
//
// NOTE: this replaces a stale tb (VUart.h) that targeted the pre-2026-04
// PascalCase fixture naming. The construct is now `e203_uart`, so the sim
// class is Ve203_uart.
//
// The status register (0x14) source comment says "bit 0 = tx_busy, bit 1 =
// rx_valid" but the code emits {tx_busy_r, rx_valid_r}, which puts tx_busy at
// bit 1 and rx_valid at bit 0. This tb checks the code's actual bit order.
//
// Run with:
//   arch sim tests/e203/e203_uart.arch --tb tests/e203/e203_uart_tb.cpp

#include "Ve203_uart.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_uart* dut;

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
    dut->uart_rx = 1;           // serial idle is high
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
    dut = new Ve203_uart;

    // ── Test 1: Reset / idle state ───────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->uart_tx == 1, "uart_tx should idle high after reset, got %d", dut->uart_tx);
    CHECK(dut->uart_irq == 0, "uart_irq should be 0 after reset, got %d", dut->uart_irq);
    CHECK(dut->pready == 1, "pready is constant 1 in this design, got %d", dut->pready);
    uint32_t v = apb_read(0x14);
    CHECK(v == 0, "status should be 0 at idle (no busy, no valid), got 0x%08x", v);
    v = apb_read(0x10);
    CHECK(v == 1, "baud div resets to 1, got 0x%08x", v);

    // ── Test 2: Control register write + readback ────────────────────
    printf("Test 2: Register write/readback\n");
    apb_write(0x10, 0x1234);
    v = apb_read(0x10);
    CHECK(v == 0x1234, "div readback should be 0x1234, got 0x%08x", v);
    apb_write(0x08, 1);
    v = apb_read(0x08);
    CHECK(v == 1, "txctrl readback should be 1, got 0x%08x", v);
    apb_write(0x0C, 1);
    v = apb_read(0x0C);
    CHECK(v == 1, "rxctrl readback should be 1, got 0x%08x", v);
    v = apb_read(0x20);
    CHECK(v == 0, "unmapped offset 0x20 should read 0, got 0x%08x", v);

    // ── Test 3: TX frame on the pin ──────────────────────────────────
    // div=0 makes baud_tick true every cycle: one serial bit per clock.
    printf("Test 3: TX frame (0xA5)\n");
    reset();
    apb_write(0x10, 0);             // div = 0 -> 1 bit per clock
    const uint8_t tx_byte = 0xA5;
    apb_write(0x00, tx_byte);       // load {stop, data, start}, busy=1
    CHECK(dut->uart_tx == 0, "start bit should drive uart_tx low, got %d", dut->uart_tx);
    for (int i = 0; i < 8; i++) {
        tick(); dut->eval();
        int expect = (tx_byte >> i) & 1;
        CHECK(dut->uart_tx == expect, "data bit %d should be %d (LSB first), got %d",
              i, expect, dut->uart_tx);
    }
    tick(); dut->eval();
    CHECK(dut->uart_tx == 1, "stop bit should drive uart_tx high, got %d", dut->uart_tx);
    tick(); dut->eval();
    CHECK(dut->uart_tx == 1, "uart_tx should return to idle high, got %d", dut->uart_tx);
    v = apb_read(0x14);
    CHECK((v & 0x2) == 0, "status tx_busy (bit 1) should clear after the frame, got 0x%08x", v);

    // ── Test 4: tx_busy status and write-while-busy lockout ──────────
    printf("Test 4: tx_busy\n");
    reset();
    apb_write(0x10, 0xFFFF);        // very slow baud so the frame stalls
    apb_write(0x00, 0xFF);          // start a frame: busy, start bit on the pin
    CHECK(dut->uart_tx == 0, "start bit of the slow frame should be low, got %d", dut->uart_tx);
    v = apb_read(0x00);
    CHECK((v >> 31) == 1, "txdata read bit 31 (full) should be 1 while busy, got 0x%08x", v);
    v = apb_read(0x14);
    CHECK((v & 0x2) != 0, "status tx_busy (bit 1) should be set while busy, got 0x%08x", v);
    // A second write while busy must be ignored: the shift register still
    // holds the first frame's start bit.
    apb_write(0x00, 0x00);
    CHECK(dut->uart_tx == 0, "write-while-busy must not disturb the frame, got tx=%d", dut->uart_tx);

    // ── Test 5: RX frame, irq, readout, read-to-clear ────────────────
    printf("Test 5: RX frame (0x5A)\n");
    reset();
    apb_write(0x10, 0);             // div = 0 -> 1 bit per clock
    apb_write(0x0C, 1);             // rxen
    const uint8_t rx_byte = 0x5A;
    CHECK(dut->uart_irq == 0, "uart_irq should be 0 before reception, got %d", dut->uart_irq);
    // Start-bit falling edge: detector arms (rx_cnt=9) on this edge.
    dut->uart_rx = 0;
    tick();
    // First armed sample is the start bit itself, then 8 data bits LSB first.
    tick();                          // sample start bit (discarded)
    for (int i = 0; i < 8; i++) {
        dut->uart_rx = (rx_byte >> i) & 1;
        tick();                      // sample data bit i
    }
    dut->uart_rx = 1;                // stop / idle
    tick();
    dut->eval();
    CHECK(dut->uart_irq == 1, "uart_irq should assert once a byte is received, got %d", dut->uart_irq);
    v = apb_read(0x14);
    CHECK((v & 0x1) != 0, "status rx_valid (bit 0) should be set, got 0x%08x", v);
    v = apb_read(0x04);
    CHECK((v & 0xFF) == rx_byte, "rxdata should be 0x%02x, got 0x%08x", rx_byte, v);
    CHECK((v >> 31) == 0, "rxdata bit 31 (empty) should be 0 while valid, got 0x%08x", v);
    // The read itself clears rx_valid.
    dut->eval();
    CHECK(dut->uart_irq == 0, "uart_irq should clear after rxdata is read, got %d", dut->uart_irq);
    v = apb_read(0x04);
    CHECK((v >> 31) == 1, "rxdata bit 31 (empty) should be 1 after the read, got 0x%08x", v);
    v = apb_read(0x14);
    CHECK((v & 0x1) == 0, "status rx_valid should be cleared, got 0x%08x", v);

    // ── Test 6: RX ignored when rxen=0 ───────────────────────────────
    printf("Test 6: RX disabled\n");
    reset();
    apb_write(0x10, 0);
    // rxen left 0: drive a full frame, nothing should latch.
    dut->uart_rx = 0;
    tick(); tick();
    for (int i = 0; i < 8; i++) { dut->uart_rx = 1; tick(); }
    dut->uart_rx = 1;
    tick();
    dut->eval();
    CHECK(dut->uart_irq == 0, "uart_irq must stay 0 with rxen=0, got %d", dut->uart_irq);
    v = apb_read(0x14);
    CHECK((v & 0x1) == 0, "status rx_valid must stay 0 with rxen=0, got 0x%08x", v);

    printf("\n=== e203_uart: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
