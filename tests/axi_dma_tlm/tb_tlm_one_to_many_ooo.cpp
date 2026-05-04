#include "VTlmOneToManyOooTop.h"

#include <cstdint>
#include <cstdio>

static VTlmOneToManyOooTop dut;

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

    constexpr uint64_t lo_expected = 0x1111000000000000ull;
    constexpr uint64_t hi_expected = 0x2222000000000000ull;

    for (int cycle = 0; cycle < 80; ++cycle) {
        tick();
        if (dut.lo_data_o == lo_expected && dut.hi_data_o == hi_expected) {
            std::printf("PASS one-to-many OOO: lo=0x%016llx hi=0x%016llx\n",
                        static_cast<unsigned long long>(dut.lo_data_o),
                        static_cast<unsigned long long>(dut.hi_data_o));
            return 0;
        }
    }

    std::printf("FAIL one-to-many OOO: lo=0x%016llx hi=0x%016llx\n",
                static_cast<unsigned long long>(dut.lo_data_o),
                static_cast<unsigned long long>(dut.hi_data_o));
    return 1;
}
