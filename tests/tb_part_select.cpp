#include "VPartSelectTest.h"
#include <cstdio>
#include <cstdlib>

static VPartSelectTest* dut;
static void fail(const char* msg) { printf("FAIL: %s\n", msg); exit(1); }

int main() {
    dut = new VPartSelectTest;
    dut->rst = 1; dut->eval();
    dut->rst = 0;

    // data_in = 0xABCDEF12, offset = 8
    dut->data_in = 0xABCDEF12U;
    dut->offset  = 8;
    dut->eval();

    // byte_out = data_in[8 +: 8] = bits[15:8] of 0xABCDEF12
    //          = 0xEF
    if (dut->byte_out != 0xEF) {
        printf("FAIL byte_out: got 0x%02x expected 0xEF\n", dut->byte_out);
        exit(1);
    }
    printf("Test 1 PASS: data_in[8 +: 8] = 0x%02x\n", dut->byte_out);

    // byte_msb_out = data_in[8 -: 8] = bits[8:1] of 0xABCDEF12
    //   0xABCDEF12 bits [8:1] = (0xEF12 >> 1) & 0xFF = 0x789 & 0xFF = 0x89
    if (dut->byte_msb_out != 0x89) {
        printf("FAIL byte_msb_out: got 0x%02x expected 0x89\n", dut->byte_msb_out);
        exit(1);
    }
    printf("Test 2 PASS: data_in[8 -: 8] = 0x%02x\n", dut->byte_msb_out);

    // hi_byte = data_in[16 +: 8] = bits[23:16] of 0xABCDEF12
    //         = 0xCD
    if (dut->hi_byte != 0xCD) {
        printf("FAIL hi_byte: got 0x%02x expected 0xCD\n", dut->hi_byte);
        exit(1);
    }
    printf("Test 3 PASS: data_in[16 +: 8] = 0x%02x\n", dut->hi_byte);

    // Test with offset = 16
    dut->offset = 16;
    dut->eval();
    // byte_out = data_in[16 +: 8] = 0xCD
    if (dut->byte_out != 0xCD) {
        printf("FAIL test4 byte_out: got 0x%02x expected 0xCD\n", dut->byte_out);
        exit(1);
    }
    printf("Test 4 PASS: data_in[16 +: 8] with offset=16 = 0x%02x\n", dut->byte_out);

    printf("PASS\n");
    delete dut;
    return 0;
}
