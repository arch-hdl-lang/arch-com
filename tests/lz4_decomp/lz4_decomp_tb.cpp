// LZ4 decompressor simulation testbench.
//
// Because output bytes appear during the same cycles we are consuming
// input bytes (the decompressor has no internal FIFO), we collect all
// output in a rolling buffer that is updated on every tick().
//
// Test 1: All-literal block — 5 bytes [0x01..0x05]
//   Compressed: token(0x50) 0x01 0x02 0x03 0x04 0x05
//
// Test 2: Literal + match — [0x11 0x22 0x33 0x44] × 2 (8 bytes)
//   Compressed: token(0x40) 0x11 0x22 0x33 0x44 offset_lo(0x04) offset_hi(0x00)+last
//   in_last falls on offset_hi so last_r is set before the match bytes are
//   emitted; MatSend then asserts out_last on the final match byte.

#include "VLz4Decomp.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <vector>

static VLz4Decomp dut;

// Output buffer: bytes captured so far this test run.
static std::vector<uint8_t> out_buf;
static bool out_last_seen = false;

// Advance one clock cycle.  Capture any output byte that the DUT
// presents (with out_ready=1 permanently asserted).
static void tick() {
    dut.out_ready = 1;
    dut.clk = 0; dut.eval();
    dut.clk = 1; dut.eval();
    if (dut.out_valid) {
        out_buf.push_back(dut.out_data);
        if (dut.out_last) out_last_seen = true;
    }
}

static void do_reset() {
    out_buf.clear();
    out_last_seen = false;
    dut.rst       = 1;
    dut.in_valid  = 0;
    dut.in_data   = 0;
    dut.in_last   = 0;
    dut.out_ready = 1;
    tick(); tick(); tick();
    dut.rst = 0;
    tick();
}

// Drive one byte with AXI-S backpressure.  Waits until in_ready is seen
// on the SAME half-tick as in_valid is high (i.e., the posedge where the
// handshake will occur), then de-asserts valid.
static void send_byte(uint8_t data, bool last) {
    dut.in_data  = data;
    dut.in_last  = last ? 1 : 0;
    dut.in_valid = 1;

    for (int guard = 0; guard < 500; ++guard) {
        // Evaluate with clock LOW to see combinatorial in_ready.
        dut.out_ready = 1;
        dut.clk = 0; dut.eval();
        // If in_ready is high now (combinatorially), the handshake will
        // happen on the upcoming rising edge.
        bool handshake = (dut.in_ready != 0);

        // Capture any output that might appear on the rising edge.
        dut.clk = 1; dut.eval();
        if (dut.out_valid) {
            out_buf.push_back(dut.out_data);
            if (dut.out_last) out_last_seen = true;
        }

        if (handshake) {
            dut.in_valid = 0;
            dut.in_last  = 0;
            // One extra tick to let the DUT advance beyond the accepting state.
            tick();
            return;
        }
    }
    fprintf(stderr, "FAIL: in_ready never asserted for byte 0x%02x\n", data);
    exit(1);
}

// Drain the DUT for up to `max_cycles` ticks, collecting output bytes.
static void drain(int max_cycles) {
    for (int i = 0; i < max_cycles; ++i) {
        tick();
        if (!dut.busy) break;
    }
}

static void run_test(const char* name,
                     const std::vector<uint8_t>& compressed,
                     const std::vector<uint8_t>& expected) {
    do_reset();
    printf("=== %s ===\n", name);

    for (size_t i = 0; i < compressed.size(); ++i)
        send_byte(compressed[i], i == compressed.size() - 1);

    // Drain any remaining output (e.g., trailing match bytes).
    drain(500);

    // Compare collected output against expected.
    bool pass = true;
    if (out_buf.size() != expected.size()) {
        fprintf(stderr, "FAIL [%s]: got %zu bytes, expected %zu\n",
                name, out_buf.size(), expected.size());
        pass = false;
    }
    for (size_t i = 0; i < expected.size() && i < out_buf.size(); ++i) {
        if (out_buf[i] != expected[i]) {
            fprintf(stderr, "FAIL [%s] byte[%zu]: got=0x%02x want=0x%02x\n",
                    name, i, out_buf[i], expected[i]);
            pass = false;
        } else {
            printf("  byte[%zu] = 0x%02x\n", i, out_buf[i]);
        }
    }
    bool want_last = true;
    if (out_last_seen != want_last) {
        fprintf(stderr, "FAIL [%s]: out_last not seen at end of block\n", name);
        pass = false;
    }
    if (!pass) exit(1);
    printf("PASS: %s\n\n", name);
}

int main() {
    // Test 1: pure literals [0x01..0x05]
    // Token 0x50 → LL=5, ML nibble=0 (last sequence, no match).
    run_test("pure_literals",
             {0x50, 0x01, 0x02, 0x03, 0x04, 0x05},
             {0x01, 0x02, 0x03, 0x04, 0x05});

    // Test 2: 4 literals + match copy of same 4 bytes
    // Token 0x40 → LL=4, ML=0 → match_len = 4.
    // Literals: 0x11 0x22 0x33 0x44
    // Offset LE: 0x04 0x00 → offset=4 (copies from history position 0).
    // in_last on offset_hi (0x00); last_r becomes true before match bytes
    // are emitted, so MatSend asserts out_last on the 4th match byte.
    run_test("literals_plus_match",
             {0x40, 0x11, 0x22, 0x33, 0x44, 0x04, 0x00},
             {0x11, 0x22, 0x33, 0x44, 0x11, 0x22, 0x33, 0x44});

    printf("ALL TESTS PASSED\n");
    return 0;
}
