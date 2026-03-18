#include "VExuRegfile.h"
#include <cstdio>
#include <cassert>

int main() {
    auto* dut = new VExuRegfile();
    int failures = 0;

    // Reset state: all registers should read as 0
    dut->clk = 0;
    dut->rst_n = 0;
    dut->test_mode = 0;
    dut->wbck_dest_wen = 0;
    dut->wbck_dest_idx = 0;
    dut->wbck_dest_dat = 0;
    dut->read_src1_idx = 0;
    dut->read_src2_idx = 0;
    dut->eval();

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    auto write_reg = [&](uint32_t idx, uint32_t val) {
        dut->wbck_dest_wen = 1;
        dut->wbck_dest_idx = idx;
        dut->wbck_dest_dat = val;
        tick();
        dut->wbck_dest_wen = 0;
    };

    auto read1 = [&](uint32_t idx) -> uint32_t {
        dut->read_src1_idx = idx;
        dut->eval();
        return dut->read_src1_dat;
    };

    auto read2 = [&](uint32_t idx) -> uint32_t {
        dut->read_src2_idx = idx;
        dut->eval();
        return dut->read_src2_dat;
    };

    printf("=== Test 1: x0 always reads 0 ===\n");
    {
        // Attempt to write x0 (should be ignored)
        write_reg(0, 0xDEADBEEF);
        uint32_t v = read1(0);
        if (v != 0) { printf("  FAIL: x0 = 0x%08x, expected 0\n", v); failures++; }
        else         { printf("  PASS: x0 = 0 after attempted write\n"); }
    }

    printf("=== Test 2: Write and read back x1–x5 ===\n");
    {
        for (uint32_t i = 1; i <= 5; i++) {
            uint32_t val = 0xA0000000u | i;
            write_reg(i, val);
            uint32_t got = read1(i);
            if (got != val) { printf("  FAIL: x%u = 0x%08x, expected 0x%08x\n", i, got, val); failures++; }
            else             { printf("  PASS: x%u = 0x%08x\n", i, got); }
        }
    }

    printf("=== Test 3: x1_r tracks rf[1] ===\n");
    {
        write_reg(1, 0x12345678u);
        dut->eval();
        uint32_t x1 = dut->x1_r;
        uint32_t r1 = read1(1);
        if (x1 != 0x12345678u || x1 != r1) {
            printf("  FAIL: x1_r = 0x%08x, read_src1 = 0x%08x\n", x1, r1); failures++;
        } else {
            printf("  PASS: x1_r = read_src1_dat[1] = 0x%08x\n", x1);
        }
    }

    printf("=== Test 4: Simultaneous two-port read ===\n");
    {
        write_reg(10, 0xAAAAAAAAu);
        write_reg(20, 0x55555555u);
        dut->read_src1_idx = 10;
        dut->read_src2_idx = 20;
        dut->eval();
        uint32_t v1 = dut->read_src1_dat;
        uint32_t v2 = dut->read_src2_dat;
        if (v1 != 0xAAAAAAAAu || v2 != 0x55555555u) {
            printf("  FAIL: src1=0x%08x src2=0x%08x\n", v1, v2); failures++;
        } else {
            printf("  PASS: src1=0x%08x  src2=0x%08x\n", v1, v2);
        }
    }

    printf("=== Test 5: Write all 31 registers, verify no aliasing ===\n");
    {
        for (uint32_t i = 1; i <= 31; i++) write_reg(i, i * 0x01010101u);
        int ok = 1;
        for (uint32_t i = 1; i <= 31; i++) {
            uint32_t got = read1(i);
            if (got != i * 0x01010101u) { ok = 0; printf("  FAIL x%u: 0x%08x\n", i, got); failures++; }
        }
        if (ok) printf("  PASS: all 31 registers correct, no aliasing\n");
        // x0 still 0
        if (read1(0) != 0) { printf("  FAIL: x0 corrupted\n"); failures++; }
        else                { printf("  PASS: x0 still 0\n"); }
    }

    delete dut;
    printf("\n%s  (%d failure(s))\n", failures ? "FAILED" : "ALL TESTS PASSED", failures);
    return failures ? 1 : 0;
}
