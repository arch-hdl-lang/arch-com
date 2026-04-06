// pipe_reg testbench: verify 3-stage delay chain
#include "VPipeRegTest.h"
#include <cstdio>

static int pass = 0, fail = 0;
#define CHECK(cond, msg, ...) \
  do { if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass; } \
       else      { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail; } } while(0)

int main() {
    VPipeRegTest dut;

    // Reset for 2 cycles
    dut.rst = 1; dut.data_in = 0;
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    dut.rst = 0;

    // Drive data_in = 0x42 and clock it through 3 stages
    dut.data_in = 0x42;
    // After XOR with 0xFF, w = 0xBD

    // Cycle 1: w enters stg1
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.data_out == 0, "cycle 1: data_out==0x%02X expected 0x00", dut.data_out);

    // Cycle 2: stg1->stg2
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.data_out == 0, "cycle 2: data_out==0x%02X expected 0x00", dut.data_out);

    // Cycle 3: stg2->delayed (output)
    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.data_out == 0xBD, "cycle 3: data_out==0x%02X expected 0xBD", dut.data_out);

    // Change input to 0x00 (w = 0xFF), verify pipeline propagation
    dut.data_in = 0x00;

    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.data_out == 0xBD, "cycle 4: data_out==0x%02X expected 0xBD (still old)", dut.data_out);

    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.data_out == 0xBD, "cycle 5: data_out==0x%02X expected 0xBD (still old)", dut.data_out);

    dut.clk = 0; dut.eval(); dut.clk = 1; dut.eval();
    CHECK(dut.data_out == 0xFF, "cycle 6: data_out==0x%02X expected 0xFF (new)", dut.data_out);

    printf("\nPipeRegTest: %d/%d passed\n", pass, pass + fail);
    return fail ? 1 : 0;
}
