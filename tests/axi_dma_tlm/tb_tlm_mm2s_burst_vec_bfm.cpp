#include "VTlmMm2sBurstVecBfmTop.h"

#include <cstdint>
#include <cstdio>

static VTlmMm2sBurstVecBfmTop dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    dut.rst = 1;
    dut.base_addr = 0x2000;
    dut.len0_i = 2;
    dut.len1_i = 4;
    for (int i = 0; i < 4; ++i) {
        tick();
    }

    dut.rst = 0;
    for (int i = 0; i < 16; ++i) {
        tick();
    }

    const uint32_t exp0 = 0xB0002000u;
    const uint32_t exp1 = 0xB1002010u;
    if (dut.req_count_o != 2 || dut.req0_len_o != 2 || dut.req1_len_o != 4 ||
        dut.data0_0 != exp0 + 0 || dut.data0_1 != exp0 + 1 ||
        dut.data0_2 != exp0 + 2 || dut.data0_3 != exp0 + 3 ||
        dut.data1_0 != exp1 + 0 || dut.data1_1 != exp1 + 1 ||
        dut.data1_2 != exp1 + 2 || dut.data1_3 != exp1 + 3) {
        std::printf("FAIL Vec BFM: req_count=%u len0=%u len1=%u data0_0=0x%08x data1_3=0x%08x\n",
                    dut.req_count_o, dut.req0_len_o, dut.req1_len_o,
                    dut.data0_0, dut.data1_3);
        return 1;
    }

    std::printf("PASS TlmMm2sBurstVecBfm len0=%u len1=%u data0[0]=0x%08x data1[3]=0x%08x\n",
                dut.req0_len_o, dut.req1_len_o, dut.data0_0, dut.data1_3);
    return 0;
}
