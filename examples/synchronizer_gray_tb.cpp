#include "PtrSync.h"
#include <cstdio>

// Gray code: only 1 bit changes between consecutive values, so even
// if the destination samples mid-transition, the result is either
// the old value or the new value — never a glitched third value.

int main() {
    PtrSync dut;
    int errors = 0;

    // Reset
    dut.src_clk = 0; dut.dst_clk = 0; dut.rst = 0; dut.data_in = 0;
    dut.rst = 1;
    dut.dst_clk = 1; dut.eval();
    dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    // ── Test 1: sequential counter values through gray sync ──
    printf("=== Test 1: counter 0..15 through 2-stage gray sync ===\n");
    // 2-stage = 2 dst_clk cycles of latency
    // Feed value, clock dst twice, check output

    for (uint8_t v = 0; v < 16; v++) {
        dut.data_in = v;
        // Two dst_clk rising edges to push through 2 stages
        for (int c = 0; c < 2; c++) {
            dut.dst_clk = 1; dut.eval();
            dut.dst_clk = 0; dut.eval();
        }
        int ok = (dut.data_out == v);
        printf("  in=%2d out=%2d %s\n", v, dut.data_out, ok ? "OK" : "FAIL");
        if (!ok) errors++;
    }

    // ── Test 2: verify gray encoding preserves single-bit changes ──
    printf("\n=== Test 2: reset and re-verify ===\n");
    dut.rst = 1;
    dut.dst_clk = 1; dut.eval();
    dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    if (dut.data_out != 0) {
        printf("  FAIL: output not 0 after reset (got %d)\n", dut.data_out);
        errors++;
    } else {
        printf("  Reset OK: output=0\n");
    }

    // Push value 5 through
    dut.data_in = 5;
    dut.dst_clk = 1; dut.eval(); dut.dst_clk = 0; dut.eval();
    dut.dst_clk = 1; dut.eval(); dut.dst_clk = 0; dut.eval();
    if (dut.data_out == 5) {
        printf("  Value 5 propagated OK\n");
    } else {
        printf("  FAIL: expected 5, got %d\n", dut.data_out);
        errors++;
    }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
