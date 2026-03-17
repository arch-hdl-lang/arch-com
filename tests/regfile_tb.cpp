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
    dut->read_addr[0] = 0;
    dut->read_addr[1] = 0;
    dut->eval();
    tick(); tick();
    dut->rst = 0;

    // ── Write values to registers ─────────────────────────────────────────────
    for (int i = 0; i < 8; i++) {
        dut->write_en   = 1;
        dut->write_addr = (uint8_t)i;
        dut->write_data = (uint8_t)((i * 11 + 5) & 0xFF);
        tick();
    }
    dut->write_en = 0;
    tick();

    // ── Read back via both read ports ─────────────────────────────────────────
    int read_errors = 0;
    for (int i = 0; i < 4; i++) {
        dut->read_addr[0] = (uint8_t)(i * 2);
        dut->read_addr[1] = (uint8_t)(i * 2 + 1);
        dut->eval();
        uint8_t got0 = (uint8_t)dut->read_data[0];
        uint8_t got1 = (uint8_t)dut->read_data[1];
        uint8_t exp0 = (uint8_t)((i * 2 * 11 + 5) & 0xFF);
        uint8_t exp1 = (uint8_t)(((i * 2 + 1) * 11 + 5) & 0xFF);
        if (got0 != exp0 || got1 != exp1) {
            printf("FAIL: read[%d]: port0 got %d exp %d, port1 got %d exp %d\n",
                   i, (int)got0, (int)exp0, (int)got1, (int)exp1);
            read_errors++;
        }
    }
    if (read_errors == 0) {
        printf("PASS: all 8 registers read correctly via 2 read ports\n");
    } else {
        errors += read_errors;
    }

    // ── Forwarding: write and read same address ───────────────────────────────
    dut->write_en   = 1;
    dut->write_addr = 5;
    dut->write_data = 0xBE;
    dut->read_addr[0] = 5;
    dut->read_addr[1] = 0;
    dut->eval();
    {
        uint8_t got = (uint8_t)dut->read_data[0];
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
