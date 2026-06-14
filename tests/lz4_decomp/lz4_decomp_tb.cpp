// C++ testbench for Lz4BlockDecomp.
// Tests two LZ4 raw blocks:
//   Test 1: "ABCDE" — all-literal block (no matches)
//     Compressed: [0x50, 0x41, 0x42, 0x43, 0x44, 0x45]
//     Token 0x50: litlen=5, match_nibble=0 (last sequence, no match)
//   Test 2: "abcabcabc" — 3 literals + 1 match of length 6
//     Compressed: [0x32, 0x61, 0x62, 0x63, 0x03, 0x00]
//     Token 0x32: litlen=3, match_nibble=2, match_len=6, offset=3

#include "VLz4BlockDecomp.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>
#include <cstdlib>
#include <vector>
#include <string>

static VLz4BlockDecomp dut;
static int pass = 0, fail = 0;

#define CHECK(cond, msg, ...) do { \
  if (cond) { printf("  PASS: " msg "\n", ##__VA_ARGS__); ++pass; } \
  else      { printf("  FAIL: " msg "\n", ##__VA_ARGS__); ++fail; } \
} while (0)

static void tick() {
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
}

static void do_reset() {
    dut.rst      = 1;
    dut.in_data  = 0;
    dut.in_valid = 0;
    dut.in_last  = 0;
    dut.out_ready = 1;
    tick(); tick();
    dut.rst = 0;
    tick();
}

// Send a single byte; set last=1 if this is the last byte.
// Returns the collected output byte if out_valid was asserted (else -1).
static int send_byte(uint8_t byte, bool last) {
    dut.in_data  = byte;
    dut.in_valid = 1;
    dut.in_last  = last ? 1 : 0;
    dut.eval();
    int out = -1;
    if (dut.out_valid) {
        out = dut.out_data;
    }
    tick();
    dut.in_valid = 0;
    dut.in_last  = 0;
    return out;
}

int main() {
    printf("=== LZ4 block decompressor sim ===\n\n");

    // ── Test 1: All-literal "ABCDE" ──────────────────────────────────────
    printf("--- Test 1: All-literal ABCDE ---\n");
    do_reset();

    // Compressed bytes: 0x50 0x41 0x42 0x43 0x44 0x45
    // Token 0x50: litlen=5 (high nibble), match_nibble=0 (low nibble)
    // Since match_nibble==0 and in_last is NOT set on token, we go ST_OFF_LO
    // BUT since this is the last sequence (in_last on last literal), we go
    // back to ST_TOKEN instead of ST_OFF_LO.
    //
    // The testbench driving strategy:
    //   - Token byte: in_last=0 (there are literal bytes after it)
    //   - Literal bytes A-D: in_last=0
    //   - Literal byte E: in_last=1 (last byte of compressed stream)
    //
    // In ST_EMIT_LIT, output is combinational: out_data=in_data when in_valid.
    // We drive each input byte, check the combinational output, then tick.

    // State: ST_TOKEN. Drive token byte 0x50.
    // After this tick, state becomes ST_EMIT_LIT, lit_remain=5.
    dut.in_data  = 0x50;
    dut.in_valid = 1;
    dut.in_last  = 0;
    dut.out_ready = 1;
    tick();  // token consumed; now in ST_EMIT_LIT

    // Now drive literal bytes one at a time and collect output.
    // In ST_EMIT_LIT: out_valid=in_valid, out_data=in_data (combinational).

    std::vector<uint8_t> collected;
    const uint8_t expected1[] = {65, 66, 67, 68, 69}; // ABCDE
    const uint8_t lits1[]     = {65, 66, 67, 68, 69};

    for (int i = 0; i < 5; i++) {
        dut.in_data  = lits1[i];
        dut.in_valid = 1;
        dut.in_last  = (i == 4) ? 1 : 0;
        dut.out_ready = 1;
        dut.eval();  // evaluate combinational paths

        CHECK(dut.out_valid == 1, "T1[%d] out_valid", i);
        CHECK(dut.out_data  == lits1[i], "T1[%d] out_data=%d want=%d", i, (int)dut.out_data, (int)lits1[i]);
        if (i < 4) {
            CHECK(dut.out_last == 0, "T1[%d] out_last==0", i);
        } else {
            CHECK(dut.out_last == 1, "T1[last] out_last==1");
        }

        collected.push_back(dut.out_data);
        tick();
    }
    dut.in_valid = 0;
    dut.in_last  = 0;

    std::string t1_out(collected.begin(), collected.end());
    CHECK(t1_out == "ABCDE", "Test 1 full output == ABCDE (got '%s')", t1_out.c_str());
    printf("  Test 1 PASS: 'ABCDE' decompressed correctly\n\n");

    // ── Test 2: "abcabcabc" with back-reference ───────────────────────────
    printf("--- Test 2: abcabcabc back-reference ---\n");
    do_reset();

    // Compressed bytes: 0x32 0x61 0x62 0x63 0x03 0x00
    // Token 0x32: litlen=3 (0x3), match_nibble=2 → match_len = 2+4 = 6
    // Literals: a(97) b(98) c(99)
    // Offset low: 3, Offset high: 0 (discarded)
    // Match: 6 bytes at history[wptr-3..] = abcabc
    // Expected output: a b c a b c a b c

    // Token: state ST_TOKEN → ST_EMIT_LIT (lit_remain=3, match_len=6)
    dut.in_data   = 0x32;
    dut.in_valid  = 1;
    dut.in_last   = 0;
    dut.out_ready = 1;
    tick();  // now in ST_EMIT_LIT, lit_remain=3

    // Emit literals a, b, c
    const uint8_t lits2[] = {97, 98, 99};
    collected.clear();
    for (int i = 0; i < 3; i++) {
        dut.in_data  = lits2[i];
        dut.in_valid = 1;
        dut.in_last  = 0;  // not last byte yet
        dut.out_ready = 1;
        dut.eval();

        CHECK(dut.out_valid == 1, "T2 lit[%d] out_valid", i);
        CHECK(dut.out_data  == lits2[i], "T2 lit[%d] out_data=%d want=%d", i, (int)dut.out_data, (int)lits2[i]);
        CHECK(dut.out_last  == 0, "T2 lit[%d] out_last==0", i);

        collected.push_back(dut.out_data);
        tick();
    }
    // After tick: wptr=3, history[0]=97, history[1]=98, history[2]=99
    // state=ST_OFF_LO

    // Offset low byte (state: ST_OFF_LO → ST_OFF_HI)
    dut.in_data  = 3;
    dut.in_valid = 1;
    dut.in_last  = 0;
    dut.eval();
    CHECK(dut.out_valid == 0, "T2 off_lo: out_valid==0");
    tick();

    // Offset high byte (state: ST_OFF_HI → ST_COPY_MATCH)
    // in_last=1 on this byte (last compressed byte)
    dut.in_data  = 0;
    dut.in_valid = 1;
    dut.in_last  = 1;
    dut.eval();
    CHECK(dut.out_valid == 0, "T2 off_hi: out_valid==0");
    tick();
    // After tick: saw_last=1, match_remain=6, copy_rd_ptr=wptr-3=0, state=ST_COPY_MATCH

    dut.in_valid = 0;
    dut.in_last  = 0;

    // 6 match copies: a(97) b(98) c(99) a(97) b(98) c(99)
    const uint8_t match_expected[] = {97, 98, 99, 97, 98, 99};
    for (int i = 0; i < 6; i++) {
        dut.out_ready = 1;
        dut.eval();

        CHECK(dut.out_valid == 1, "T2 match[%d] out_valid", i);
        CHECK(dut.out_data == match_expected[i], "T2 match[%d] out_data=%d want=%d", i, (int)dut.out_data, (int)match_expected[i]);
        if (i < 5) {
            CHECK(dut.out_last == 0, "T2 match[%d] out_last==0", i);
        } else {
            CHECK(dut.out_last == 1, "T2 match[last] out_last==1");
        }

        collected.push_back(dut.out_data);
        tick();
    }

    std::string t2_out(collected.begin(), collected.end());
    CHECK(t2_out == "abcabcabc", "Test 2 full output == abcabcabc (got '%s')", t2_out.c_str());
    printf("  Test 2 PASS: 'abcabcabc' decompressed correctly\n");

    printf("\n=== %d pass / %d fail ===\n", pass, fail);
    if (fail == 0) {
        printf("ALL TESTS PASSED\n");
        return 0;
    } else {
        printf("SOME TESTS FAILED\n");
        return 1;
    }
}
