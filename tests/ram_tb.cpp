#include "VSimpleMem.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VSimpleMem* dut = new VSimpleMem;

    int errors = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // All signals idle
    dut->clk = 0;
    dut->access_en  = 0;
    dut->access_wen = 0;
    dut->access_addr  = 0;
    dut->access_wdata = 0;
    dut->eval();
    tick(); // settle / init block runs

    // ── Write 8 values ────────────────────────────────────────────────────────
    for (int i = 0; i < 8; i++) {
        dut->access_en  = 1;
        dut->access_wen = 1;
        dut->access_addr  = (uint8_t)i;
        dut->access_wdata = (uint8_t)((i * 7 + 3) & 0xFF);
        tick();
    }
    dut->access_en  = 0;
    dut->access_wen = 0;
    tick(); // deassert

    // ── Read 8 values back (no_change sync: 1-cycle latency) ─────────────────
    int read_errors = 0;
    for (int i = 0; i < 8; i++) {
        dut->access_en  = 1;
        dut->access_wen = 0;
        dut->access_addr = (uint8_t)i;
        // Rising edge: rdata_r <= mem[addr]
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
        // rdata_r (and thus access_rdata) is now updated
        uint8_t got      = (uint8_t)dut->access_rdata;
        uint8_t expected = (uint8_t)((i * 7 + 3) & 0xFF);
        if (got != expected) {
            printf("FAIL: read addr %d: got %d, expected %d\n", i, (int)got, (int)expected);
            read_errors++;
        }
    }
    dut->access_en = 0;
    if (read_errors == 0) {
        printf("PASS: all 8 values read correctly (no_change sync)\n");
    } else {
        errors += read_errors;
    }

    // ── Write-enable gating: write then check without wen ────────────────────
    // Write 0xAB to addr 10
    dut->access_en = 1; dut->access_wen = 1;
    dut->access_addr = 10; dut->access_wdata = 0xAB;
    tick();
    // Read back addr 10
    dut->access_wen = 0;
    dut->access_addr = 10;
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    {
        uint8_t got = (uint8_t)dut->access_rdata;
        if (got != 0xAB) {
            printf("FAIL: write-enable test: got 0x%02X, expected 0xAB\n", (int)got);
            errors++;
        } else {
            printf("PASS: write-enable test: 0xAB read correctly\n");
        }
    }

    // ── no_change: wen=1 should NOT update rdata_r ───────────────────────────
    // addr 10 holds 0xAB; do a write-cycle (wen=1) — rdata_r must not change
    dut->access_en = 1; dut->access_wen = 1;
    dut->access_addr = 10; dut->access_wdata = 0xCD;
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    // rdata_r should still be 0xAB (no_change: write cycle does not update read reg)
    {
        uint8_t got = (uint8_t)dut->access_rdata;
        if (got != 0xAB) {
            printf("FAIL: no_change: rdata changed on write cycle (got 0x%02X, expected 0xAB)\n",
                   (int)got);
            errors++;
        } else {
            printf("PASS: no_change: rdata stable during write cycle\n");
        }
    }
    // Verify the write actually happened: read addr 10
    dut->access_en = 1; dut->access_wen = 0;
    dut->access_addr = 10;
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
    {
        uint8_t got = (uint8_t)dut->access_rdata;
        if (got != 0xCD) {
            printf("FAIL: write committed: got 0x%02X, expected 0xCD\n", (int)got);
            errors++;
        } else {
            printf("PASS: write committed: 0xCD verified\n");
        }
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
