// Testbench for RomLut — verify inline array init and read-back
#include "VRomLut.h"
#include <cstdio>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

static void tick(VRomLut &m) {
    m.clk = 0; m.eval();
    m.clk = 1; m.eval();
}

int main() {
    VRomLut m;

    // Expected values from init array
    uint8_t expected[] = {0x00, 0x31, 0x5A, 0x76, 0x7F, 0x76, 0x5A, 0x31};

    // Read each address (latency 1: result appears next cycle)
    for (int i = 0; i < 8; i++) {
        m.rd_addr = i;
        m.rd_en = 1;
        tick(m);
        // After this tick, rd_data holds mem[i] (latched on rising edge)
    }

    // Read back: address 0 was presented on cycle 0, result on cycle 1
    // Let's do proper latency-1 reads
    m.rd_addr = 0; m.rd_en = 1;
    tick(m); // latch mem[0]
    CHECK(m.rd_data == expected[0], "addr 0: got 0x%02X, expected 0x%02X", m.rd_data, expected[0]);

    m.rd_addr = 1; m.rd_en = 1;
    tick(m); // latch mem[1]
    CHECK(m.rd_data == expected[1], "addr 1: got 0x%02X, expected 0x%02X", m.rd_data, expected[1]);

    m.rd_addr = 4; m.rd_en = 1;
    tick(m); // latch mem[4]
    CHECK(m.rd_data == expected[4], "addr 4: got 0x%02X, expected 0x%02X", m.rd_data, expected[4]);

    m.rd_addr = 7; m.rd_en = 1;
    tick(m); // latch mem[7]
    CHECK(m.rd_data == expected[7], "addr 7: got 0x%02X, expected 0x%02X", m.rd_data, expected[7]);

    // Test en=0 holds previous value
    uint8_t prev = m.rd_data;
    m.rd_addr = 0; m.rd_en = 0;
    tick(m);
    CHECK(m.rd_data == prev, "en=0 holds: got 0x%02X, expected 0x%02X", m.rd_data, prev);

    printf("\n=== RomLut test: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
