#include "BusSync.h"
#include <cstdio>

int main() {
    BusSync dut;
    int errors = 0;

    // Reset
    dut.src_clk = 0; dut.dst_clk = 0; dut.rst = 0;
    dut.data_in = 0;
    dut.rst = 1;
    dut.src_clk = 1; dut.dst_clk = 1; dut.eval();
    dut.src_clk = 0; dut.dst_clk = 0; dut.eval();
    dut.rst = 0;

    printf("=== Test 1: single value transfer ===\n");
    dut.data_in = 0xDEADBEEF;

    // Run enough cycles for handshake to complete:
    // src captures data + toggles req (1 src cycle)
    // req syncs to dst (2 dst cycles)
    // ack syncs back to src (2 src cycles)
    // Total: ~10 cycles of both clocks should be plenty
    for (int i = 0; i < 10; i++) {
        dut.src_clk = 1; dut.eval();
        dut.dst_clk = 1; dut.eval();
        dut.src_clk = 0; dut.eval();
        dut.dst_clk = 0; dut.eval();
    }

    if (dut.data_out == 0xDEADBEEF) {
        printf("  Value 0xDEADBEEF transferred OK\n");
    } else {
        printf("  FAIL: expected 0xDEADBEEF, got 0x%08X\n", dut.data_out);
        errors++;
    }

    printf("\n=== Test 2: multiple values ===\n");
    uint32_t vals[] = {0x12345678, 0xCAFEBABE, 0x00000001, 0xFFFFFFFF};
    for (int v = 0; v < 4; v++) {
        dut.data_in = vals[v];
        for (int i = 0; i < 10; i++) {
            dut.src_clk = 1; dut.eval();
            dut.dst_clk = 1; dut.eval();
            dut.src_clk = 0; dut.eval();
            dut.dst_clk = 0; dut.eval();
        }
        int ok = (dut.data_out == vals[v]);
        printf("  val=0x%08X out=0x%08X %s\n", vals[v], dut.data_out, ok ? "OK" : "FAIL");
        if (!ok) errors++;
    }

    printf("\n=== Test 3: reset clears state ===\n");
    dut.rst = 1;
    dut.src_clk = 1; dut.dst_clk = 1; dut.eval();
    dut.src_clk = 0; dut.dst_clk = 0; dut.eval();
    dut.rst = 0;
    if (dut.data_out == 0) {
        printf("  Reset OK\n");
    } else {
        printf("  FAIL: expected 0 after reset, got 0x%08X\n", dut.data_out);
        errors++;
    }

    printf("\n%s: %d errors\n", errors ? "FAIL" : "PASS", errors);
    return errors ? 1 : 0;
}
