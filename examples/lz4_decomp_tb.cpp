// LZ4 decompressor testbench
// Input:  compressed block for "hello, hello!"
// Output: verify 13 decompressed bytes match "hello, hello!"
//
// Compressed layout (12 bytes):
//   0x71  — token: lit_len=7, mat_nibble=1 (match_len=5)
//   "hello, " — 7 literal bytes
//   0x07 0x00 — match offset 7 (LE)
//   0x10  — token: lit_len=1, mat_nibble=0 (last seq, no match)
//   '!'   — 1 literal byte

#include "VLz4Decomp.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>

static const uint8_t compressed[] = {
    0x71,                               // token: lit_len=7, mat_nibble=1
    'h','e','l','l','o',',',' ',        // 7 literal bytes
    0x07, 0x00,                         // match offset = 7 (LE)
    0x10,                               // token: lit_len=1, mat_nibble=0
    '!'                                 // 1 literal byte (last)
};
static const int N_COMP = (int)sizeof(compressed);
static const char expected[] = "hello, hello!";
static const int N_EXP = 13;

static void tick(VLz4Decomp* d) {
    d->clk = 0; d->eval();
    d->clk = 1; d->eval();
}

int main() {
    VLz4Decomp dut;

    // Reset
    dut.rst       = 1;
    dut.in_valid  = 0;
    dut.in_data   = 0;
    dut.in_last   = 0;
    dut.out_ready = 1;   // always ready to accept output
    dut.clk = 0;
    dut.eval();
    tick(&dut); tick(&dut); tick(&dut);
    dut.rst = 0;

    int in_idx  = 0;
    int out_idx = 0;
    char out_buf[32] = {};
    bool done_seen = false;

    for (int cycle = 0; cycle < 500; ++cycle) {
        // Drive input
        if (in_idx < N_COMP) {
            dut.in_data  = compressed[in_idx];
            dut.in_valid = 1;
            dut.in_last  = (in_idx == N_COMP - 1) ? 1 : 0;
        } else {
            dut.in_valid = 0;
            dut.in_last  = 0;
        }

        dut.eval();

        // Sample output (combinational signals visible before tick)
        if (dut.out_valid && dut.out_ready) {
            if (out_idx < (int)sizeof(out_buf) - 1)
                out_buf[out_idx] = (char)dut.out_data;
            out_idx++;
        }

        // Advance input pointer when handshake fires
        if (dut.in_valid && dut.in_ready)
            in_idx++;

        if (dut.done)
            done_seen = true;

        tick(&dut);
    }

    printf("Output (%d bytes): \"%.*s\"\n", out_idx, out_idx, out_buf);

    int pass = 1;
    if (!done_seen) {
        printf("FAIL: done never asserted\n");
        pass = 0;
    }
    if (out_idx != N_EXP) {
        printf("FAIL: output length %d, expected %d\n", out_idx, N_EXP);
        pass = 0;
    }
    if (out_idx == N_EXP && memcmp(out_buf, expected, N_EXP) != 0) {
        printf("FAIL: output mismatch\n");
        printf("  got:  \"%.*s\"\n", out_idx, out_buf);
        printf("  want: \"%s\"\n", expected);
        pass = 0;
    }
    if (pass)
        printf("PASS: output matches \"%s\"\n", expected);

    return pass ? 0 : 1;
}
