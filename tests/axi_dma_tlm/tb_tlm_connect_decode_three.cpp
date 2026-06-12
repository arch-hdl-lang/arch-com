#include "VTlmConnectDecodeThreeTop.h"

#include <cstdint>
#include <cstdio>

static VTlmConnectDecodeThreeTop dut;

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

    constexpr uint64_t lo_expected = 0xAAAA000000000000ull;
    constexpr uint64_t mid_expected = 0xBBBB000000000000ull;
    constexpr uint64_t hi_expected = 0xCCCC000000000000ull;

    for (int cycle = 0; cycle < 200; ++cycle) {
        tick();
        if (dut.lo_o == lo_expected
            && dut.mid_o == mid_expected
            && dut.hi_o == hi_expected) {
            std::printf("PASS decoded connect 3way: lo=0x%016llx mid=0x%016llx hi=0x%016llx\n",
                        static_cast<unsigned long long>(dut.lo_o),
                        static_cast<unsigned long long>(dut.mid_o),
                        static_cast<unsigned long long>(dut.hi_o));
            return 0;
        }
    }

    std::printf("FAIL decoded connect 3way: lo=0x%016llx mid=0x%016llx hi=0x%016llx\n",
                static_cast<unsigned long long>(dut.lo_o),
                static_cast<unsigned long long>(dut.mid_o),
                static_cast<unsigned long long>(dut.hi_o));
    return 1;
}
