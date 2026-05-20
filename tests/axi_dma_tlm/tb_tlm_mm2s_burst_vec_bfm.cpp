#include "VTlmMm2sBurstVecBfmTop.h"

#include <cstdint>
#include <cstdio>

static VTlmMm2sBurstVecBfmTop dut;

static void program_rsp0(uint32_t base) {
    dut.rsp0_data_i[0] = base + 0;
    dut.rsp0_data_i[1] = base + 1;
    dut.rsp0_data_i[2] = base + 2;
    dut.rsp0_data_i[3] = base + 3;
}

static void program_rsp1(uint32_t base) {
    dut.rsp1_data_i[0] = base + 0;
    dut.rsp1_data_i[1] = base + 1;
    dut.rsp1_data_i[2] = base + 2;
    dut.rsp1_data_i[3] = base + 3;
}

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    const uint32_t req0_addr = 0x2000u;
    const uint32_t req1_addr = 0x2010u;
    const uint32_t exp0 = 0xA5000000u;
    const uint32_t exp1 = 0x5A000000u;

    dut.rst = 1;
    dut.base_addr = req0_addr;
    dut.len0_i = 2;
    dut.len1_i = 4;
    program_rsp0(exp0);
    program_rsp1(exp1);
    for (int i = 0; i < 4; ++i) {
        tick();
    }

    dut.rst = 0;
    for (int i = 0; i < 16; ++i) {
        tick();
    }

    if (dut.req_count_o != 2 ||
        dut.req0_addr_o != req0_addr || dut.req1_addr_o != req1_addr ||
        dut.req0_len_o != 2 || dut.req1_len_o != 4 ||
        dut.data0_0 != exp0 + 0 || dut.data0_1 != exp0 + 1 ||
        dut.data0_2 != exp0 + 2 || dut.data0_3 != exp0 + 3 ||
        dut.data1_0 != exp1 + 0 || dut.data1_1 != exp1 + 1 ||
        dut.data1_2 != exp1 + 2 || dut.data1_3 != exp1 + 3) {
        std::printf("FAIL Vec BFM: req_count=%u addr0=0x%08x addr1=0x%08x len0=%u len1=%u data0_0=0x%08x data1_3=0x%08x\n",
                    dut.req_count_o, dut.req0_addr_o, dut.req1_addr_o,
                    dut.req0_len_o, dut.req1_len_o,
                    dut.data0_0, dut.data1_3);
        return 1;
    }

    std::printf("PASS TlmMm2sBurstVecBfm addr0=0x%08x addr1=0x%08x data0[0]=0x%08x data1[3]=0x%08x\n",
                dut.req0_addr_o, dut.req1_addr_o, dut.data0_0, dut.data1_3);
    return 0;
}
