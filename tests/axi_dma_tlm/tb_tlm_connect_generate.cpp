#include "VTlmConnectGenerateTop.h"

#include <cstdint>
#include <cstdio>

static VTlmConnectGenerateTop dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

int main() {
    dut.rst = 1;
    for (int i = 0; i < 2; ++i) {
        tick();
    }

    dut.rst = 0;
    for (int i = 0; i < 12; ++i) {
        tick();
    }

    const uint64_t exp0 = 0x1111222233334444ULL;
    const uint64_t exp1 = 0x5555666677778888ULL;
    if (dut.data_0 != exp0 || dut.data_1 != exp1) {
        std::printf("FAIL data_0=0x%016llx data_1=0x%016llx\n",
                    static_cast<unsigned long long>(dut.data_0),
                    static_cast<unsigned long long>(dut.data_1));
        return 1;
    }

    return 0;
}
