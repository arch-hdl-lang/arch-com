// Testbench for Lz4BlockDecoder — LZ4 block decompressor
//
// Three functional tests plus an error-path test, all with out_ready=1
// (no back-pressure). Compressed bytes are fed one per cycle; the
// decoder outputs one byte per cycle (literals) or per match-copy step.
//
// Sampling discipline: combinational outputs (out_valid, out_data, done)
// are read at the negedge eval before posedge register update.

#include "VLz4BlockDecoder.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>
#include <vector>

static int g_errors = 0;
static VLz4BlockDecoder* dut;

#define CHECK(cond, fmt, ...) \
    do { if (!(cond)) { printf("  FAIL: " fmt "\n", ##__VA_ARGS__); g_errors++; } } while(0)

// ── Simulation helpers ────────────────────────────────────────────────────

static void reset() {
    dut->rst       = 1;
    dut->in_valid  = 0;
    dut->in_data   = 0;
    dut->in_last   = 0;
    dut->out_ready = 0;
    for (int i = 0; i < 3; i++) {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
    }
    dut->rst = 0;
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

// Decode one LZ4 block.
// `input`: compressed bytes with in_last=1 on the last one.
// Returns decompressed bytes collected from out_valid/out_data.
// Stops when `done` is sampled high or `error` is latched, or `max_cycles`.
static std::vector<uint8_t> decode_block(const std::vector<uint8_t>& input,
                                          int max_cycles = 4096) {
    std::vector<uint8_t> output;
    size_t in_idx = 0;

    dut->out_ready = 1;     // accept output every cycle
    dut->in_valid  = 0;
    dut->in_last   = 0;
    dut->in_data   = 0;

    for (int c = 0; c < max_cycles; c++) {
        // Present the next unshipped input byte when we have one
        if (in_idx < input.size()) {
            dut->in_valid = 1;
            dut->in_data  = input[in_idx];
            dut->in_last  = (in_idx == input.size() - 1) ? 1 : 0;
        } else {
            dut->in_valid = 0;
            dut->in_last  = 0;
        }

        // Negedge eval: combinational outputs valid here (registers hold
        // previous-cycle values; in/out signals are driven above)
        dut->clk = 0; dut->eval();

        // Sample handshake signals at this point (stable comb values)
        bool out_fire = (dut->out_valid != 0) && (dut->out_ready != 0);
        bool in_fire  = (dut->in_valid  != 0) && (dut->in_ready  != 0);
        bool is_done  = (dut->done      != 0);
        bool is_err   = (dut->error     != 0);

        if (out_fire)          output.push_back((uint8_t)dut->out_data);
        if (in_fire)           in_idx++;

        // Posedge: advance registers
        dut->clk = 1; dut->eval();

        if (is_done || is_err) break;
    }

    dut->in_valid  = 0;
    dut->in_last   = 0;
    dut->out_ready = 0;
    return output;
}

// ── Test helpers ──────────────────────────────────────────────────────────

static void check_output(const char* test, const std::vector<uint8_t>& got,
                          const std::vector<uint8_t>& expected) {
    CHECK(got.size() == expected.size(),
          "%s: output length %zu, expected %zu", test, got.size(), expected.size());
    size_t n = got.size() < expected.size() ? got.size() : expected.size();
    for (size_t i = 0; i < n; i++) {
        CHECK(got[i] == expected[i],
              "%s byte[%zu]: got 0x%02x, expected 0x%02x",
              test, i, got[i], expected[i]);
    }
}

// ── Test 1: Pure literals — "Hello" (5 bytes, no match) ──────────────────
//
// Compressed:  token=0x50 (lit=5, ml_raw=0)
//              literals: 'H','e','l','l','o'   (in_last on 'o')
// Expected out: H e l l o
static void test1_pure_literals() {
    printf("Test 1: pure literals — \"Hello\"\n");
    reset();

    // 0x50 = {lit=5, ml=0}
    std::vector<uint8_t> input = { 0x50, 'H', 'e', 'l', 'l', 'o' };
    std::vector<uint8_t> expected = { 'H', 'e', 'l', 'l', 'o' };
    // in_last is on the last byte ('o')

    // Annotate last byte manually via decode_block (it treats last element as in_last=1)
    auto output = decode_block(input);
    check_output("Test1", output, expected);

    CHECK(!dut->error, "Test1: unexpected error");
    printf("  output: ");
    for (uint8_t b : output) printf("'%c'(0x%02x) ", (b >= 0x20 && b < 0x7f) ? (char)b : '.', b);
    printf("\n  %s\n", g_errors == 0 ? "PASS" : "FAIL");
}

// ── Test 2: Literals + match copy + final literal — "abcdabcdX" (9 bytes) ─
//
// Sequence 1: token=0x40 (lit=4, ml_raw=0 → match_len=4)
//             literals: a b c d
//             offset: {0x04, 0x00} (LE) = 4 → copy a b c d from history
// Sequence 2: token=0x10 (lit=1, ml_raw=0, last sequence — no match)
//             literal: 'X' (in_last=1)
// Expected: a b c d a b c d X
static void test2_match_copy() {
    printf("Test 2: match copy — \"abcdabcdX\"\n");
    reset();

    std::vector<uint8_t> input = {
        0x40,                           // token: lit=4, ml=0
        'a', 'b', 'c', 'd',            // 4 literals
        0x04, 0x00,                     // offset = 4 (LE)
        0x10,                           // token: lit=1, ml=0 (last sequence)
        'X'                             // 1 literal, in_last=1 (last byte)
    };
    std::vector<uint8_t> expected = { 'a','b','c','d','a','b','c','d','X' };

    auto output = decode_block(input);
    check_output("Test2", output, expected);

    CHECK(!dut->error, "Test2: unexpected error");
    printf("  output: ");
    for (uint8_t b : output) printf("'%c'(0x%02x) ", (b >= 0x20 && b < 0x7f) ? (char)b : '.', b);
    printf("\n  %s\n", (g_errors == 0) ? "PASS" : "FAIL");
}

// ── Test 3: Run-length expansion — "aaaaaz" (1 lit + 4-byte run + 1 lit) ──
//
// LZ4 matches with offset=1 (copy from 1 byte back) implement run-length
// encoding: the match read-pointer "chases" the write pointer, re-reading
// the same byte as it is written. This is a valid and common LZ4 pattern.
//
// Sequence 1: token=0x10 (lit=1, ml_raw=0 → match_len=4)
//             literal: 'a'          → history[0]='a', wr_ptr=1
//             offset: {0x01, 0x00}  → mat_rd_ptr = 1-1 = 0
//             match 4:  hist[0]='a', hist[1]='a', hist[2]='a', hist[3]='a'
// Sequence 2: token=0x10 (lit=1, last)
//             literal: 'z' (in_last=1)
// Expected: a a a a a z
static void test3_runlength_match() {
    printf("Test 3: run-length match — \"aaaaaz\"\n");
    reset();

    std::vector<uint8_t> input = {
        0x10,                           // token: lit=1, ml=0
        'a',                            // 1 literal
        0x01, 0x00,                     // offset = 1 (LE)
        0x10,                           // token: lit=1, ml=0 (last sequence)
        'z'                             // 1 literal, in_last=1
    };
    std::vector<uint8_t> expected = { 'a','a','a','a','a','z' };

    auto output = decode_block(input);
    check_output("Test3", output, expected);

    CHECK(!dut->error, "Test3: unexpected error");
    printf("  output: ");
    for (uint8_t b : output) printf("'%c'(0x%02x) ", (b >= 0x20 && b < 0x7f) ? (char)b : '.', b);
    printf("\n  %s\n", (g_errors == 0) ? "PASS" : "FAIL");
}

// ── Test 4: Error path — offset == 0 (invalid) ────────────────────────────
//
// LZ4 prohibits offset=0. Our decoder must latch error and hold it.
static void test4_error_offset_zero() {
    printf("Test 4: error path — offset == 0\n");
    reset();

    // Sequence: lit=1, then offset={0x00,0x00} (invalid)
    std::vector<uint8_t> input = {
        0x10,                           // token: lit=1, ml=0
        'x',                            // 1 literal
        0x00, 0x00,                     // offset = 0 (INVALID → should latch error)
        // in_last is on 0x00 (last byte) — but error should latch first
        // so this byte may not even be consumed
    };
    // Patch: last byte of sequence is the second 0x00 (in_last=1)
    // decode_block will detect error and stop

    auto output = decode_block(input);

    // After offset-zero error, error flag must be latched
    // (re-eval to ensure error is visible)
    dut->clk = 0; dut->eval();
    CHECK(dut->error, "Test4: expected error=1 for offset==0");
    dut->clk = 1; dut->eval();

    // error must persist across cycles without reset
    for (int i = 0; i < 3; i++) {
        dut->clk = 0; dut->eval();
        CHECK(dut->error, "Test4: error should persist (cycle %d after error)", i);
        dut->clk = 1; dut->eval();
    }

    // Reset should clear error
    reset();
    dut->clk = 0; dut->eval();
    CHECK(!dut->error, "Test4: error should clear after reset");
    dut->clk = 1; dut->eval();

    printf("  %s\n", (g_errors == 0) ? "PASS" : "FAIL");
}

// ── Main ──────────────────────────────────────────────────────────────────

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    dut = new VLz4BlockDecoder;
    dut->clk = 0;

    printf("=== Lz4BlockDecoder testbench (CAST LZ4SNP-D analog) ===\n");

    test1_pure_literals();
    test2_match_copy();
    test3_runlength_match();
    test4_error_offset_zero();

    printf("\n=== %s (%d error%s) ===\n",
           g_errors == 0 ? "ALL TESTS PASSED" : "TESTS FAILED",
           g_errors, g_errors == 1 ? "" : "s");

    delete dut;
    return g_errors ? 1 : 0;
}
