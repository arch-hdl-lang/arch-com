#include "DataSync.h"
#include <cstdio>

int main() {
    DataSync dut;
    int errors = 0;

    // Reset (sync reset)
    dut.src_clk = 0; dut.dst_clk = 0; dut.rst = 0; dut.data_in = 0;
    dut.rst = 1;
    dut.dst_clk = 1; dut.eval();
    dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    // ── Test 1: 3-stage propagation (data appears after 3 cycles) ──
    printf("=== Test 1: 3-stage propagation ===\n");
    dut.data_in = 0xAB;

    for (int c = 1; c <= 4; c++) {
        dut.dst_clk = 1; dut.eval();
        uint8_t expect = (c >= 3) ? 0xAB : 0;
        int ok = (dut.data_out == expect);
        printf("Cycle %d: data_out=0x%02X (expect 0x%02X) %s\n",
               c, dut.data_out, expect, ok ? "OK" : "FAIL");
        if (!ok) errors++;
        dut.dst_clk = 0; dut.eval();
    }

    // ── Test 2: changing data ──
    printf("\n=== Test 2: data stream ===\n");
    uint8_t vals[] = {0x11, 0x22, 0x33, 0x44, 0x55};
    // After reset, chain is [0xAB, 0xAB, 0xAB] (saturated from test 1)
    // Inject new values and check 3-cycle delay
    for (int i = 0; i < 5; i++) {
        dut.data_in = vals[i];
        dut.dst_clk = 1; dut.eval();
        // Output should be vals[i-2] if i>=2, else 0xAB (from test 1 saturation)
        uint8_t expect = (i >= 2) ? vals[i - 2] : 0xAB;
        int ok = (dut.data_out == expect);
        printf("i=%d: in=0x%02X out=0x%02X (expect 0x%02X) %s\n",
               i, vals[i], dut.data_out, expect, ok ? "OK" : "FAIL");
        if (!ok) errors++;
        dut.dst_clk = 0; dut.eval();
    }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
