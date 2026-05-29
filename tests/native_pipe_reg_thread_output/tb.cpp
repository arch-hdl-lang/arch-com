#include "VNativePipeRegThreadOutputProbe.h"

#include <cstdio>

static VNativePipeRegThreadOutputProbe dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    dut.rst = 1;
    dut.start = 0;
    dut.payload_in = 0;
    tick();

    dut.rst = 0;
    tick();

    dut.payload_in = 0x5a;
    dut.start = 1;
    tick();
    tick();

    if (!dut.valid_out || dut.payload_out != 0x5a) {
        std::printf("FAIL: public pipe_reg outputs valid=%u payload=0x%02x, expected valid=1 payload=0x5a\n",
                    (unsigned)dut.valid_out, (unsigned)dut.payload_out);
        return 1;
    }

    dut.start = 0;
    tick();
    if (dut.valid_out) {
        std::printf("FAIL: valid_out stayed asserted after one-cycle pulse\n");
        return 1;
    }

    std::printf("PASS native pipe_reg thread output\n");
    return 0;
}
