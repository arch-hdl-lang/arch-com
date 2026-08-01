#include "VProbe.h"
#include <cstdint>
#include <cstdio>

int main() {
    VProbe dut;
    dut.addr = 0x89abcdefu;
    dut.len = 0x12u;
    dut.size = 0x5u;
    dut.burst = 0x2u;
    dut.eval();

    const uint64_t expected_packed =
        (uint64_t{2} << 43) |
        (uint64_t{5} << 40) |
        (uint64_t{0x12} << 32) |
        uint64_t{0x89abcdef};

    if (dut.wide_low != 0x98765432u) {
        std::fprintf(stderr, "wide_low mismatch: got 0x%08x\n", dut.wide_low);
        return 1;
    }
    if (dut.wide_user != 0x1234u) {
        std::fprintf(stderr, "wide_user mismatch: got 0x%04x\n", dut.wide_user);
        return 1;
    }
    if (dut.packed_out != expected_packed) {
        std::fprintf(
            stderr,
            "packed_out mismatch: got 0x%llx expected 0x%llx\n",
            static_cast<unsigned long long>(dut.packed_out),
            static_cast<unsigned long long>(expected_packed)
        );
        return 1;
    }

    std::puts("PASS native parameterized bus wire");
    return 0;
}
