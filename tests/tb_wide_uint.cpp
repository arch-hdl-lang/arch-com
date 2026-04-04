#include "VWideUintTest.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>

static VWideUintTest* dut;
static void fail(const char* msg) { printf("FAIL: %s\n", msg); exit(1); }

static void tick(int n = 1) {
    for (int i = 0; i < n; i++) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    }
}

// Set a 512-bit VlWide<16> from 8 × 64-bit words (word0 = bits [63:0])
static void set512(VlWide<16>& w, uint64_t w0, uint64_t w1, uint64_t w2, uint64_t w3,
                                   uint64_t w4, uint64_t w5, uint64_t w6, uint64_t w7) {
    w._data[ 0] = (uint32_t)w0; w._data[ 1] = (uint32_t)(w0>>32);
    w._data[ 2] = (uint32_t)w1; w._data[ 3] = (uint32_t)(w1>>32);
    w._data[ 4] = (uint32_t)w2; w._data[ 5] = (uint32_t)(w2>>32);
    w._data[ 6] = (uint32_t)w3; w._data[ 7] = (uint32_t)(w3>>32);
    w._data[ 8] = (uint32_t)w4; w._data[ 9] = (uint32_t)(w4>>32);
    w._data[10] = (uint32_t)w5; w._data[11] = (uint32_t)(w5>>32);
    w._data[12] = (uint32_t)w6; w._data[13] = (uint32_t)(w6>>32);
    w._data[14] = (uint32_t)w7; w._data[15] = (uint32_t)(w7>>32);
}

static uint64_t get64_word(const VlWide<16>& w, int word_idx) {
    int base = word_idx * 2;
    return (uint64_t)w._data[base] | ((uint64_t)w._data[base+1] << 32);
}

int main() {
    dut = new VWideUintTest;

    // Reset
    dut->rst = 1; dut->clk = 0; dut->eval();
    tick(3);
    dut->rst = 0;

    // ── Test 1: 512-bit register + word0 extraction ───────────────────────
    set512(dut->line_in,
           0xA000000000000001ULL, 0xB000000000000002ULL,
           0xC000000000000003ULL, 0xD000000000000004ULL,
           0xE000000000000005ULL, 0xF000000000000006ULL,
           0x1000000000000007ULL, 0x2000000000000008ULL);
    tick(1); dut->eval();

    uint64_t w0_got = dut->word0_out;
    if (w0_got != 0xA000000000000001ULL) {
        printf("FAIL test1: word0_out=0x%016llx exp=0x%016llx\n",
               (unsigned long long)w0_got, (unsigned long long)0xA000000000000001ULL);
        exit(1);
    }
    printf("Test 1 PASS: 512-bit reg word0 extraction\n");

    // ── Test 2: line_out passthrough ──────────────────────────────────────
    uint64_t lo0 = get64_word(dut->line_out, 0);
    uint64_t lo7 = get64_word(dut->line_out, 7);
    if (lo0 != 0xA000000000000001ULL) fail("test2: line_out word0");
    if (lo7 != 0x2000000000000008ULL) fail("test2: line_out word7");
    printf("Test 2 PASS: 512-bit line_out passthrough\n");

    // ── Test 3: packed_out = concat of 8 narrow words ────────────────────
    dut->w0 = 0x1111111111111111ULL;
    dut->w1 = 0x2222222222222222ULL;
    dut->w2 = 0x3333333333333333ULL;
    dut->w3 = 0x4444444444444444ULL;
    dut->w4 = 0x5555555555555555ULL;
    dut->w5 = 0x6666666666666666ULL;
    dut->w6 = 0x7777777777777777ULL;
    dut->w7 = 0x8888888888888888ULL;
    dut->eval();

    if (get64_word(dut->packed_out, 0) != 0x1111111111111111ULL) fail("test3: packed word0");
    if (get64_word(dut->packed_out, 7) != 0x8888888888888888ULL) fail("test3: packed word7");
    if (get64_word(dut->packed_out, 3) != 0x4444444444444444ULL) fail("test3: packed word3");
    printf("Test 3 PASS: 512-bit concat from 8 × 64-bit words\n");

    // ── Test 4: 512-bit equality ──────────────────────────────────────────
    if (!dut->eq_result) fail("test4: eq_result should be 1 (equal)");
    printf("Test 4 PASS: 512-bit equality (equal)\n");

    set512(dut->line_in, 0xDEADBEEFDEADBEEFULL, 0, 0, 0, 0, 0, 0, 0);
    dut->eval();
    if (dut->eq_result) fail("test4b: eq_result should be 0 (unequal)");
    printf("Test 4b PASS: 512-bit equality (unequal)\n");

    // ── Test 5: 2048-bit passthrough ─────────────────────────────────────
    for (int i = 0; i < 64; i++) dut->huge_in._data[i] = 0;
    dut->huge_in._data[0]  = 0xDEADBEEFU;
    dut->huge_in._data[62] = 0xCAFEBABEU;
    dut->eval();
    if (dut->huge_out._data[0]  != 0xDEADBEEFU) fail("test5: huge_out[31:0]");
    if (dut->huge_out._data[62] != 0xCAFEBABEU) fail("test5: huge_out[1983:1984]");
    printf("Test 5 PASS: 2048-bit passthrough\n");

    printf("PASS\n");
    delete dut;
    return 0;
}
