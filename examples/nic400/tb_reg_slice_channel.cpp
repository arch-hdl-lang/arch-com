// C++ runner for RegSliceChannel_test.harc.
// Companion to the HARC source-of-truth test; mirrors its assertions.

#include "VRegSliceChannel.h"

#include <cstdint>
#include <cstdio>

static VRegSliceChannel dut;

static void tick() {
    dut.clk = 0;
    dut.eval();
    dut.clk = 1;
    dut.eval();
}

static int fail(const char *msg) {
    std::printf("FAIL RegSliceChannel: %s\n", msg);
    return 1;
}

int main() {
    // Reset (active-low Reset<Async, Low>): rst=0 = in reset
    dut.rst = 0;
    dut.up_valid = 0;
    dut.up_payload = 0;
    dut.dn_ready = 0;
    for (int i = 0; i < 4; ++i) tick();
    dut.rst = 1;
    for (int i = 0; i < 2; ++i) tick();

    // 1. Empty after reset
    if (dut.dn_valid != 0) return fail("post-reset dn_valid != 0");
    if (dut.up_ready != 1) return fail("post-reset up_ready != 1");

    // 2. One-beat traversal, latency = 1
    dut.dn_ready = 1;
    dut.up_valid = 1;
    dut.up_payload = 0xDEADBEEFu;
    tick();
    if (dut.dn_valid != 1) return fail("after 1 cycle, dn_valid != 1");
    if (dut.dn_payload != 0xDEADBEEFu) return fail("dn_payload mismatch");

    // Drain
    dut.up_valid = 0;
    for (int i = 0; i < 2; ++i) tick();
    if (dut.dn_valid != 0) return fail("after drain, dn_valid != 0");

    // 3. Backpressure: downstream stalled
    dut.dn_ready = 0;
    dut.up_valid = 1;
    dut.up_payload = 0xCAFEBABEu;
    tick();
    if (dut.dn_valid != 1) return fail("after fill+stall, dn_valid != 1");
    if (dut.up_ready != 0) return fail("when occupied and dn_ready=0, up_ready must be 0");
    // Try to overwrite -- should NOT happen
    dut.up_payload = 0x0BADC0DEu;
    for (int i = 0; i < 2; ++i) tick();
    if (dut.dn_payload != 0xCAFEBABEu) return fail("payload changed under backpressure");
    if (dut.dn_valid != 1) return fail("dn_valid dropped under backpressure");

    // Release
    dut.up_valid = 0;
    dut.dn_ready = 1;
    for (int i = 0; i < 2; ++i) tick();
    if (dut.dn_valid != 0) return fail("after release, dn_valid != 0");

    // 4. Sustained throughput
    int count = 0;
    dut.dn_ready = 1;
    dut.up_valid = 1;
    for (uint32_t i = 0; i < 8; ++i) {
        dut.up_payload = 0x1000u + i;
        tick();
        if (dut.dn_valid && dut.dn_ready) count++;
    }
    dut.up_valid = 0;
    if (count < 7) {
        std::printf("FAIL RegSliceChannel: throughput too low: %d/8\n", count);
        return 1;
    }

    std::printf("PASS RegSliceChannel latency=1 backpressure=ok throughput=%d/8\n", count);
    return 0;
}
