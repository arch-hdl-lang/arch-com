// AES-128 Encryption Verilator Testbench
// Uses NIST FIPS-197 Appendix B test vector:
//   Key:        000102030405060708090a0b0c0d0e0f
//   Plaintext:  00112233445566778899aabbccddeeff
//   Ciphertext: 69c4e0d86a7b0430d8cdb78070b4c55a

#include "VAesCipherTop.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>

static void set128(uint32_t* w, uint32_t w3, uint32_t w2, uint32_t w1, uint32_t w0) {
    // Verilator VlWide: w[0] is LSB word, w[3] is MSB word
    w[0] = w0;
    w[1] = w1;
    w[2] = w2;
    w[3] = w3;
}

static void print128(const char* label, const uint32_t* w) {
    printf("%s: %08x_%08x_%08x_%08x\n", label, w[3], w[2], w[1], w[0]);
}

static bool cmp128(const uint32_t* got, uint32_t e3, uint32_t e2, uint32_t e1, uint32_t e0) {
    return got[0] == e0 && got[1] == e1 && got[2] == e2 && got[3] == e3;
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VAesCipherTop* dut = new VAesCipherTop;

    int errors = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    };

    // ── Reset ──
    dut->clk = 0;
    dut->rst = 1;
    dut->ld  = 0;
    set128(dut->key.data(),     0, 0, 0, 0);
    set128(dut->text_in.data(), 0, 0, 0, 0);
    dut->eval();
    tick(); tick();
    dut->rst = 0;
    tick();

    // ── Test 1: NIST FIPS-197 Appendix B ──
    printf("=== Test 1: NIST AES-128 test vector ===\n");

    // Key:       0x000102030405060708090a0b0c0d0e0f
    // Plaintext: 0x00112233445566778899aabbccddeeff
    set128(dut->key.data(),     0x00010203, 0x04050607, 0x08090a0b, 0x0c0d0e0f);
    set128(dut->text_in.data(), 0x00112233, 0x44556677, 0x8899aabb, 0xccddeeff);

    // Assert ld for one cycle
    dut->ld = 1;
    tick();
    dut->ld = 0;

    // Run for enough cycles (12 cycles should be enough: 1 load + 1 init + 10 rounds)
    for (int i = 0; i < 14; i++) {
        tick();
        if (dut->done) {
            printf("  Done asserted at cycle %d after ld\n", i + 1);
            break;
        }
    }

    if (!dut->done) {
        printf("FAIL: done never asserted\n");
        errors++;
    } else {
        print128("  Expected", (const uint32_t[]){0xccddeeff & 0, 0, 0, 0}); // placeholder
        // Expected ciphertext: 69c4e0d86a7b0430d8cdb78070b4c55a
        print128("  Got     ", dut->text_out.data());
        if (cmp128(dut->text_out.data(), 0x69c4e0d8, 0x6a7b0430, 0xd8cdb780, 0x70b4c55a)) {
            printf("  PASS: ciphertext matches NIST vector\n");
        } else {
            printf("  FAIL: ciphertext mismatch!\n");
            printf("  Expected: 69c4e0d8_6a7b0430_d8cdb780_70b4c55a\n");
            errors++;
        }
    }

    // ── Test 2: zero key, zero plaintext ──
    printf("\n=== Test 2: zero key, zero plaintext ===\n");
    dut->rst = 1;
    tick(); tick();
    dut->rst = 0;
    tick();

    set128(dut->key.data(),     0x00000000, 0x00000000, 0x00000000, 0x00000000);
    set128(dut->text_in.data(), 0x00000000, 0x00000000, 0x00000000, 0x00000000);

    dut->ld = 1;
    tick();
    dut->ld = 0;

    for (int i = 0; i < 14; i++) {
        tick();
        if (dut->done) {
            printf("  Done asserted at cycle %d after ld\n", i + 1);
            break;
        }
    }

    if (!dut->done) {
        printf("FAIL: done never asserted (test 2)\n");
        errors++;
    } else {
        print128("  Got     ", dut->text_out.data());
        // AES-128(key=0, pt=0) = 66e94bd4ef8a2c3b884cfa59ca342b2e
        if (cmp128(dut->text_out.data(), 0x66e94bd4, 0xef8a2c3b, 0x884cfa59, 0xca342b2e)) {
            printf("  PASS: ciphertext matches known result\n");
        } else {
            printf("  FAIL: ciphertext mismatch!\n");
            printf("  Expected: 66e94bd4_ef8a2c3b_884cfa59_ca342b2e\n");
            errors++;
        }
    }

    dut->final();
    delete dut;

    if (errors == 0) { printf("\nALL TESTS PASSED\n"); return 0; }
    else             { printf("\n%d TESTS FAILED\n", errors); return 1; }
}
