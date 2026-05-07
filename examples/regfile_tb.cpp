#include "VIntRegs.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VIntRegs* dut = new VIntRegs;

    int errors = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // Reset
    dut->clk = 0;
    dut->rst = 1;
    dut->write_en = 0;
    dut->write_addr = 0;
    dut->write_data = 0;
    dut->read0_addr = 0;
    dut->read1_addr = 0;
    dut->eval();
    tick(); tick();
    dut->rst = 0;

    // ── Write values to registers 1..7 ────────────────────────────────────────
    // Skip addr 0 — `init [0] = 0;` in int_regs.arch hardwires register 0 to
    // zero, so writes there are dropped (RISC-V x0 style).
    for (int i = 1; i < 8; i++) {
        dut->write_en   = 1;
        dut->write_addr = (uint8_t)i;
        dut->write_data = (uint8_t)((i * 11 + 5) & 0xFF);
        tick();
    }
    dut->write_en = 0;
    tick();

    // ── Read back via both read ports ─────────────────────────────────────────
    int read_errors = 0;
    for (int i = 0; i < 3; i++) {
        dut->read0_addr = (uint8_t)(i * 2 + 1);   // 1, 3, 5
        dut->read1_addr = (uint8_t)(i * 2 + 2);   // 2, 4, 6
        dut->eval();
        uint8_t got0 = (uint8_t)dut->read0_data;
        uint8_t got1 = (uint8_t)dut->read1_data;
        uint8_t exp0 = (uint8_t)(((i * 2 + 1) * 11 + 5) & 0xFF);
        uint8_t exp1 = (uint8_t)(((i * 2 + 2) * 11 + 5) & 0xFF);
        if (got0 != exp0 || got1 != exp1) {
            printf("FAIL: read[%d]: port0 got %d exp %d, port1 got %d exp %d\n",
                   i, (int)got0, (int)exp0, (int)got1, (int)exp1);
            read_errors++;
        }
    }
    // Confirm reg[0] stays zero through write attempts.
    dut->read0_addr = 7; dut->read1_addr = 0;
    dut->eval();
    if (dut->read0_data != (uint8_t)((7 * 11 + 5) & 0xFF)) {
        printf("FAIL: read[7]: got %d\n", (int)dut->read0_data);
        read_errors++;
    }
    if (dut->read1_data != 0) {
        printf("FAIL: reg[0] should be hardwired zero, got %d\n", (int)dut->read1_data);
        read_errors++;
    }
    if (read_errors == 0) {
        printf("PASS: regs 1..7 read correctly, reg[0] stays zero\n");
    } else {
        errors += read_errors;
    }

    // ── Forwarding: write and read same address ───────────────────────────────
    dut->write_en   = 1;
    dut->write_addr = 5;
    dut->write_data = 0xBE;
    dut->read0_addr = 5;
    dut->read1_addr = 0;
    dut->eval();
    {
        uint8_t got = (uint8_t)dut->read0_data;
        if (got != 0xBE) {
            printf("FAIL: forwarding: expected 0xBE, got 0x%02X\n", (int)got);
            errors++;
        } else {
            printf("PASS: forwarding write_before_read\n");
        }
    }
    tick();
    dut->write_en = 0;

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
