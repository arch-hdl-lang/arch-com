#include "VGenElseTest.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    ctx->commandArgs(argc, argv);
    VGenElseTest* dut = new VGenElseTest(ctx);

    dut->eval();

    // false-branch: main_out should be 0xABCD (else branch active)
    if (dut->main_out != 0xABCD) {
        printf("FAIL: main_out=0x%04x expected 0xABCD\n", dut->main_out);
        exit(1);
    }
    printf("Test 1 PASS: generate else branch, main_out=0x%04x\n", dut->main_out);

    // true-branch: then_out should be 0x42 (then branch active)
    if (dut->then_out != 0x42) {
        printf("FAIL: then_out=0x%02x expected 0x42\n", dut->then_out);
        exit(1);
    }
    printf("Test 2 PASS: generate if then branch, then_out=0x%02x\n", dut->then_out);

    // Verify skip_out and debug_out do NOT exist as members (compile-time check):
    // If generate else is broken and skip_out/debug_out were generated instead,
    // the C++ compile would fail — so reaching here is the test.

    printf("PASS\n");
    delete dut; delete ctx;
    return 0;
}
