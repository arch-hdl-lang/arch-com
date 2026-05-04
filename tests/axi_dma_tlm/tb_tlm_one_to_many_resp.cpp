#include "VTlmOneToManyRespTop.h"

#include <cstdint>
#include <cstdio>

static VTlmOneToManyRespTop dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

static void reset() {
    dut.rst = 1;
    for (int i = 0; i < 4; ++i) {
        tick();
    }
    dut.rst = 0;
    tick();
}

int main() {
    reset();

    constexpr uint64_t d0 = 0x1000000000000000ull;
    constexpr uint64_t d1 = 0x2000000000000000ull;
    constexpr uint64_t d2 = 0x3000000000000000ull;
    constexpr uint64_t d3 = 0x4000000000000000ull;

    for (int cycle = 0; cycle < 120; ++cycle) {
        tick();
        if (dut.data0_o == d0
            && dut.data1_o == d1
            && dut.data2_o == d2
            && dut.data3_o == d3
            && dut.bad_data_o == 0
            && dut.resp0_o == 0
            && dut.resp1_o == 0
            && dut.resp2_o == 0
            && dut.resp3_o == 0
            && dut.bad_resp_o == 1) {
            std::printf("PASS one-to-many response router: d0=0x%016llx d3=0x%016llx bad_resp=%u\n",
                        static_cast<unsigned long long>(dut.data0_o),
                        static_cast<unsigned long long>(dut.data3_o),
                        static_cast<unsigned>(dut.bad_resp_o));
            return 0;
        }
    }

    std::printf("FAIL one-to-many response router: "
                "d0=0x%016llx d1=0x%016llx d2=0x%016llx d3=0x%016llx "
                "bad=0x%016llx resp=%u/%u/%u/%u bad_resp=%u\n",
                static_cast<unsigned long long>(dut.data0_o),
                static_cast<unsigned long long>(dut.data1_o),
                static_cast<unsigned long long>(dut.data2_o),
                static_cast<unsigned long long>(dut.data3_o),
                static_cast<unsigned long long>(dut.bad_data_o),
                static_cast<unsigned>(dut.resp0_o),
                static_cast<unsigned>(dut.resp1_o),
                static_cast<unsigned>(dut.resp2_o),
                static_cast<unsigned>(dut.resp3_o),
                static_cast<unsigned>(dut.bad_resp_o));
    return 1;
}
