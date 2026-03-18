// Verilator testbench for e203_exu_regfile.sv
// Produces identical output to e203_exu_regfile_tb.cpp so results can be diff'd.
#include "Ve203_exu_regfile.h"
#include "verilated.h"
#include <cstdio>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto* dut = new Ve203_exu_regfile();
    int failures = 0;

    dut->clk = 0; dut->rst_n = 0; dut->test_mode = 0;
    dut->write_en = 0; dut->write_addr = 0; dut->write_data = 0;
    dut->read0_addr = 0; dut->read1_addr = 0;
    dut->eval();

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    auto write_reg = [&](uint32_t idx, uint32_t val) {
        dut->write_en = 1; dut->write_addr = idx; dut->write_data = val;
        tick();
        dut->write_en = 0;
    };

    auto read0 = [&](uint32_t idx) -> uint32_t {
        dut->read0_addr = idx; dut->eval(); return dut->read0_data;
    };
    auto read1 = [&](uint32_t idx) -> uint32_t {
        dut->read1_addr = idx; dut->eval(); return dut->read1_data;
    };

    printf("=== Test 1: x0 always reads 0 ===\n");
    write_reg(0, 0xDEADBEEF);
    if (read0(0) != 0) { printf("  FAIL: x0 = 0x%08x\n", read0(0)); failures++; }
    else printf("  PASS: x0 = 0 after attempted write\n");

    printf("=== Test 2: Write and read back x1-x5 ===\n");
    for (uint32_t i = 1; i <= 5; i++) {
        uint32_t val = 0xA0000000u | i;
        write_reg(i, val);
        uint32_t got = read0(i);
        if (got != val) { printf("  FAIL: x%u = 0x%08x, expected 0x%08x\n", i, got, val); failures++; }
        else printf("  PASS: x%u = 0x%08x\n", i, got);
    }

    printf("=== Test 3: Simultaneous two-port read ===\n");
    write_reg(10, 0xAAAAAAAAu);
    write_reg(20, 0x55555555u);
    dut->read0_addr = 10; dut->read1_addr = 20; dut->eval();
    if (dut->read0_data != 0xAAAAAAAAu || dut->read1_data != 0x55555555u) {
        printf("  FAIL: read0=0x%08x read1=0x%08x\n", dut->read0_data, dut->read1_data); failures++;
    } else printf("  PASS: read0=0x%08x  read1=0x%08x\n", dut->read0_data, dut->read1_data);

    printf("=== Test 4: Two ports read different regs simultaneously ===\n");
    write_reg(7, 0x12345678u); write_reg(15, 0x87654321u);
    dut->read0_addr = 7; dut->read1_addr = 15; dut->eval();
    if (dut->read0_data != 0x12345678u || dut->read1_data != 0x87654321u) {
        printf("  FAIL: read0=0x%08x read1=0x%08x\n", dut->read0_data, dut->read1_data); failures++;
    } else printf("  PASS: read0=0x%08x  read1=0x%08x\n", dut->read0_data, dut->read1_data);

    printf("=== Test 5: Write all 31 registers, verify no aliasing ===\n");
    for (uint32_t i = 1; i <= 31; i++) write_reg(i, i * 0x01010101u);
    int ok = 1;
    for (uint32_t i = 1; i <= 31; i++) {
        uint32_t got = read0(i);
        if (got != i * 0x01010101u) { ok = 0; printf("  FAIL x%u: 0x%08x\n", i, got); failures++; }
    }
    if (ok) printf("  PASS: all 31 registers correct, no aliasing\n");
    if (read0(0) != 0) { printf("  FAIL: x0 corrupted\n"); failures++; }
    else printf("  PASS: x0 still 0\n");

    dut->final();
    delete dut;
    printf("\n%s  (%d failure(s))\n", failures ? "FAILED" : "ALL TESTS PASSED", failures);
    return failures ? 1 : 0;
}
