// Testbench for RomLutFile — verify ROM loaded from hex file produces
// identical read-back to the inline-array version.
#include "VRomLutFile.h"
#include <cstdio>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

static void tick(VRomLutFile &m) {
    m.clk = 0; m.eval();
    m.clk = 1; m.eval();
}

int main() {
    VRomLutFile m;

    // Same expected values as the inline-array ROM
    uint8_t expected[] = {0x00, 0x31, 0x5A, 0x76, 0x7F, 0x76, 0x5A, 0x31};

    // Latency-1 reads: present addr, tick, read result
    for (int i = 0; i < 8; i++) {
        m.rd_addr = i;
        m.rd_en = 1;
        tick(m);
        CHECK(m.rd_data == expected[i],
              "addr %d: got 0x%02X, expected 0x%02X", i, m.rd_data, expected[i]);
    }

    // Test en=0 holds previous value
    uint8_t prev = m.rd_data;
    m.rd_addr = 0; m.rd_en = 0;
    tick(m);
    CHECK(m.rd_data == prev, "en=0 holds: got 0x%02X, expected 0x%02X", m.rd_data, prev);

    printf("\n=== RomLutFile test: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
