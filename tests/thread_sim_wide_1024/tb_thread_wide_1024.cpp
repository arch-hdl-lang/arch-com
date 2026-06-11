#include "VThreadWide1024.h"
#include <cstdio>
#include <cstdint>

static void tick(VThreadWide1024& dut) {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
    dut.clk = 0;
    dut.eval();
}

static uint64_t low64(const VlWide<32>& v) {
    return (uint64_t)v._data[0] | ((uint64_t)v._data[1] << 32);
}

int main() {
    VThreadWide1024 dut;
    dut.rst = 1;
    tick(dut);

    const uint64_t expected = 0xCAFEF00DDEADBEEFull;
    dut.rst = 0;
    dut.data_in = VlWide<32>(expected);
    tick(dut);
    tick(dut);
    tick(dut);

    uint64_t got = low64(dut.data_out);
    if (got != expected) {
        std::printf("FAIL thread_wide_1024: got 0x%016llx expected 0x%016llx\n",
                    (unsigned long long)got,
                    (unsigned long long)expected);
        return 1;
    }

    std::printf("PASS thread_wide_1024\n");
    return 0;
}
